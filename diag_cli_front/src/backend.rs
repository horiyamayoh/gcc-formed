use crate::args::ParsedArgs;
use crate::config::ConfigFile;
use crate::mode::{
    CliCompatibilitySeam, ModeDecision, compatibility_scope_notice_for_seam, detect_capabilities,
    detect_profile_from_capabilities, has_hard_conflict, select_mode_for_seam,
    should_capture_passthrough_stderr,
};
use diag_backend_probe::{ProbeCache, ProbeResult, ProcessingPath, ResolveRequest};
use diag_capture_runtime::{
    CapturePlan, CaptureRequest, ExecutionMode, LocaleHandling, NativeTextCapturePolicy,
    StructuredCapturePolicy,
};
use diag_render::{DebugRefs, RenderCapabilities, RenderProfile};
use diag_trace::{RetentionPolicy, WrapperPaths};
use std::env;
use std::ffi::OsStr;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub(crate) struct ExecutionPlan {
    pub(crate) backend: ProbeResult,
    pub(crate) mode_decision: ModeDecision,
    pub(crate) profile: RenderProfile,
    pub(crate) debug_refs: DebugRefs,
    capture_plan: CapturePlan,
    pub(crate) capabilities: RenderCapabilities,
    pub(crate) scope_notice: Option<&'static str>,
}

impl ExecutionPlan {
    pub(crate) fn mode(&self) -> ExecutionMode {
        self.capture_plan.execution_mode
    }

    pub(crate) fn capture_request(
        &self,
        paths: &WrapperPaths,
        parsed: &ParsedArgs,
        cwd: &Path,
    ) -> CaptureRequest {
        CaptureRequest::from_plan(
            self.backend.clone(),
            parsed.forwarded_args.clone(),
            cwd.to_path_buf(),
            paths.clone(),
            self.capture_plan,
        )
    }
}

fn build_capture_plan(
    compatibility_seam: &CliCompatibilitySeam,
    mode: ExecutionMode,
    retention_policy: RetentionPolicy,
    debug_refs: DebugRefs,
) -> CapturePlan {
    let structured_capture = if compatibility_seam.should_inject_sarif(mode) {
        StructuredCapturePolicy::SarifFile
    } else {
        StructuredCapturePolicy::Disabled
    };
    let native_text_capture = match mode {
        ExecutionMode::Passthrough
            if should_capture_passthrough_stderr(retention_policy, debug_refs) =>
        {
            NativeTextCapturePolicy::TeeToParent
        }
        ExecutionMode::Passthrough => NativeTextCapturePolicy::Passthrough,
        ExecutionMode::Render => NativeTextCapturePolicy::CaptureOnly,
        ExecutionMode::Shadow => NativeTextCapturePolicy::TeeToParent,
    };

    CapturePlan {
        execution_mode: mode,
        processing_path: match mode {
            ExecutionMode::Passthrough => ProcessingPath::Passthrough,
            _ if matches!(structured_capture, StructuredCapturePolicy::SarifFile) => {
                ProcessingPath::DualSinkStructured
            }
            _ => ProcessingPath::NativeTextCapture,
        },
        structured_capture,
        native_text_capture,
        locale_handling: if matches!(mode, ExecutionMode::Render) {
            LocaleHandling::ForceMessagesC
        } else {
            LocaleHandling::Preserve
        },
        retention_policy,
    }
}

pub(crate) fn build_execution_plan(
    argv0: &str,
    parsed: &ParsedArgs,
    config: &ConfigFile,
    cache: &mut ProbeCache,
) -> Result<ExecutionPlan, Box<dyn std::error::Error>> {
    let backend = cache.get_or_probe(ResolveRequest {
        explicit_backend: parsed.backend.clone().or(config.backend.gcc.clone()),
        env_backend: env::var_os("FORMED_BACKEND_GCC").map(PathBuf::from),
        invoked_as: argv0.to_string(),
    })?;
    let capabilities = detect_capabilities();
    let explicit_mode = parsed.mode.or(config.runtime.mode);
    let hard_conflict = has_hard_conflict(&parsed.forwarded_args);
    let compatibility_seam = CliCompatibilitySeam::from_probe(&backend);
    let mode_decision = select_mode_for_seam(&compatibility_seam, explicit_mode, hard_conflict);
    let profile = parsed
        .profile
        .or(config.render.profile)
        .unwrap_or_else(|| detect_profile_from_capabilities(&capabilities));
    let debug_refs = parsed
        .debug_refs
        .or(config.render.debug_refs)
        .unwrap_or(DebugRefs::None);
    let retention_policy = parsed
        .trace
        .or(config.trace.retention_policy)
        .unwrap_or(RetentionPolicy::OnWrapperFailure);
    Ok(ExecutionPlan {
        scope_notice: compatibility_scope_notice_for_seam(&compatibility_seam, &mode_decision),
        capture_plan: build_capture_plan(
            &compatibility_seam,
            mode_decision.mode,
            retention_policy,
            debug_refs,
        ),
        backend,
        mode_decision,
        profile,
        debug_refs,
        capabilities,
    })
}

pub(crate) fn backend_binary_name(backend: &ProbeResult) -> &str {
    backend
        .resolved_path
        .file_name()
        .and_then(OsStr::to_str)
        .unwrap_or("gcc")
}

#[cfg(test)]
mod tests {
    use super::*;
    use diag_backend_probe::SupportTier;

    #[test]
    fn tier_a_render_plan_keeps_dual_sink_capture() {
        let plan = build_capture_plan(
            &CliCompatibilitySeam::from_support_tier(SupportTier::A),
            ExecutionMode::Render,
            RetentionPolicy::OnWrapperFailure,
            DebugRefs::None,
        );

        assert_eq!(plan.execution_mode, ExecutionMode::Render);
        assert_eq!(plan.processing_path, ProcessingPath::DualSinkStructured);
        assert_eq!(plan.structured_capture, StructuredCapturePolicy::SarifFile);
        assert_eq!(
            plan.native_text_capture,
            NativeTextCapturePolicy::CaptureOnly
        );
        assert_eq!(plan.locale_handling, LocaleHandling::ForceMessagesC);
        assert_eq!(plan.retention_policy, RetentionPolicy::OnWrapperFailure);
    }

    #[test]
    fn tier_b_shadow_plan_keeps_native_text_capture() {
        let plan = build_capture_plan(
            &CliCompatibilitySeam::from_support_tier(SupportTier::B),
            ExecutionMode::Shadow,
            RetentionPolicy::OnWrapperFailure,
            DebugRefs::None,
        );

        assert_eq!(plan.processing_path, ProcessingPath::NativeTextCapture);
        assert_eq!(plan.structured_capture, StructuredCapturePolicy::Disabled);
        assert_eq!(
            plan.native_text_capture,
            NativeTextCapturePolicy::TeeToParent
        );
        assert_eq!(plan.locale_handling, LocaleHandling::Preserve);
    }

    #[test]
    fn passthrough_plan_only_tees_when_retention_or_debug_requires_it() {
        let passthrough = build_capture_plan(
            &CliCompatibilitySeam::from_support_tier(SupportTier::B),
            ExecutionMode::Passthrough,
            RetentionPolicy::OnWrapperFailure,
            DebugRefs::None,
        );
        assert_eq!(passthrough.processing_path, ProcessingPath::Passthrough);
        assert_eq!(
            passthrough.native_text_capture,
            NativeTextCapturePolicy::Passthrough
        );

        let retained = build_capture_plan(
            &CliCompatibilitySeam::from_support_tier(SupportTier::B),
            ExecutionMode::Passthrough,
            RetentionPolicy::Always,
            DebugRefs::None,
        );
        assert_eq!(
            retained.native_text_capture,
            NativeTextCapturePolicy::TeeToParent
        );

        let debug_capture_ref = build_capture_plan(
            &CliCompatibilitySeam::from_support_tier(SupportTier::B),
            ExecutionMode::Passthrough,
            RetentionPolicy::OnWrapperFailure,
            DebugRefs::CaptureRef,
        );
        assert_eq!(
            debug_capture_ref.native_text_capture,
            NativeTextCapturePolicy::TeeToParent
        );
    }
}
