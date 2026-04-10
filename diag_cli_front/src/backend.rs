use crate::args::ParsedArgs;
use crate::config::ConfigFile;
use crate::mode::{
    CliCompatibilitySeam, ModeDecision, compatibility_scope_notice_for_path, detect_capabilities,
    detect_profile_from_capabilities, has_hard_conflict, select_mode_for_seam,
    select_processing_path_for_seam, should_capture_passthrough_stderr,
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
use std::ffi::OsString;
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
    processing_path: ProcessingPath,
    retention_policy: RetentionPolicy,
    debug_refs: DebugRefs,
    capabilities: &RenderCapabilities,
    forwarded_args: &[OsString],
) -> CapturePlan {
    let structured_capture = match processing_path {
        ProcessingPath::DualSinkStructured if compatibility_seam.should_inject_sarif(mode) => {
            StructuredCapturePolicy::SarifFile
        }
        ProcessingPath::SingleSinkStructured => StructuredCapturePolicy::SingleSinkSarifFile,
        ProcessingPath::DualSinkStructured
        | ProcessingPath::NativeTextCapture
        | ProcessingPath::Passthrough => StructuredCapturePolicy::Disabled,
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
        processing_path,
        structured_capture,
        native_text_capture,
        preserve_native_color: compatibility_seam.should_preserve_tty_color(
            mode,
            capabilities,
            forwarded_args,
        ),
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
    let requested_processing_path = parsed.processing_path.or(config.runtime.processing_path);
    let hard_conflict = has_hard_conflict(&parsed.forwarded_args);
    let compatibility_seam = CliCompatibilitySeam::from_probe(&backend);
    let mode_decision = select_mode_for_seam(&compatibility_seam, explicit_mode, hard_conflict);
    let processing_path = select_processing_path_for_seam(
        &compatibility_seam,
        &mode_decision,
        requested_processing_path,
    );
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
    let capture_plan = build_capture_plan(
        &compatibility_seam,
        mode_decision.mode,
        processing_path,
        retention_policy,
        debug_refs,
        &capabilities,
        &parsed.forwarded_args,
    );
    Ok(ExecutionPlan {
        scope_notice: compatibility_scope_notice_for_path(
            &compatibility_seam,
            &mode_decision,
            capture_plan.processing_path,
        ),
        capture_plan,
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

    fn tty_capabilities() -> RenderCapabilities {
        RenderCapabilities {
            stream_kind: diag_render::StreamKind::Tty,
            width_columns: Some(100),
            ansi_color: true,
            unicode: false,
            hyperlinks: false,
            interactive: true,
        }
    }

    fn pipe_capabilities() -> RenderCapabilities {
        RenderCapabilities {
            stream_kind: diag_render::StreamKind::Pipe,
            width_columns: Some(100),
            ansi_color: false,
            unicode: false,
            hyperlinks: false,
            interactive: false,
        }
    }

    #[test]
    fn tier_a_render_plan_keeps_dual_sink_capture() {
        let plan = build_capture_plan(
            &CliCompatibilitySeam::from_support_tier(SupportTier::A),
            ExecutionMode::Render,
            ProcessingPath::DualSinkStructured,
            RetentionPolicy::OnWrapperFailure,
            DebugRefs::None,
            &tty_capabilities(),
            &[],
        );

        assert_eq!(plan.execution_mode, ExecutionMode::Render);
        assert_eq!(plan.processing_path, ProcessingPath::DualSinkStructured);
        assert_eq!(plan.structured_capture, StructuredCapturePolicy::SarifFile);
        assert_eq!(
            plan.native_text_capture,
            NativeTextCapturePolicy::CaptureOnly
        );
        assert!(plan.preserve_native_color);
        assert_eq!(plan.locale_handling, LocaleHandling::ForceMessagesC);
        assert_eq!(plan.retention_policy, RetentionPolicy::OnWrapperFailure);
    }

    #[test]
    fn tier_b_shadow_plan_keeps_native_text_capture() {
        let plan = build_capture_plan(
            &CliCompatibilitySeam::from_support_tier(SupportTier::B),
            ExecutionMode::Shadow,
            ProcessingPath::NativeTextCapture,
            RetentionPolicy::OnWrapperFailure,
            DebugRefs::None,
            &pipe_capabilities(),
            &[],
        );

        assert_eq!(plan.processing_path, ProcessingPath::NativeTextCapture);
        assert_eq!(plan.structured_capture, StructuredCapturePolicy::Disabled);
        assert_eq!(
            plan.native_text_capture,
            NativeTextCapturePolicy::TeeToParent
        );
        assert!(!plan.preserve_native_color);
        assert_eq!(plan.locale_handling, LocaleHandling::Preserve);
    }

    #[test]
    fn passthrough_plan_only_tees_when_retention_or_debug_requires_it() {
        let passthrough = build_capture_plan(
            &CliCompatibilitySeam::from_support_tier(SupportTier::B),
            ExecutionMode::Passthrough,
            ProcessingPath::Passthrough,
            RetentionPolicy::OnWrapperFailure,
            DebugRefs::None,
            &pipe_capabilities(),
            &[],
        );
        assert_eq!(passthrough.processing_path, ProcessingPath::Passthrough);
        assert_eq!(
            passthrough.native_text_capture,
            NativeTextCapturePolicy::Passthrough
        );
        assert!(!passthrough.preserve_native_color);

        let retained = build_capture_plan(
            &CliCompatibilitySeam::from_support_tier(SupportTier::B),
            ExecutionMode::Passthrough,
            ProcessingPath::Passthrough,
            RetentionPolicy::Always,
            DebugRefs::None,
            &pipe_capabilities(),
            &[],
        );
        assert_eq!(
            retained.native_text_capture,
            NativeTextCapturePolicy::TeeToParent
        );

        let debug_capture_ref = build_capture_plan(
            &CliCompatibilitySeam::from_support_tier(SupportTier::B),
            ExecutionMode::Passthrough,
            ProcessingPath::Passthrough,
            RetentionPolicy::OnWrapperFailure,
            DebugRefs::CaptureRef,
            &pipe_capabilities(),
            &[],
        );
        assert_eq!(
            debug_capture_ref.native_text_capture,
            NativeTextCapturePolicy::TeeToParent
        );
    }

    #[test]
    fn tier_b_single_sink_structured_plan_uses_explicit_sarif_file_capture() {
        let plan = build_capture_plan(
            &CliCompatibilitySeam::from_support_tier(SupportTier::B),
            ExecutionMode::Render,
            ProcessingPath::SingleSinkStructured,
            RetentionPolicy::OnWrapperFailure,
            DebugRefs::None,
            &pipe_capabilities(),
            &[],
        );

        assert_eq!(plan.processing_path, ProcessingPath::SingleSinkStructured);
        assert_eq!(
            plan.structured_capture,
            StructuredCapturePolicy::SingleSinkSarifFile
        );
        assert_eq!(
            plan.native_text_capture,
            NativeTextCapturePolicy::CaptureOnly
        );
    }
}
