use crate::args::ParsedArgs;
use crate::config::ConfigFile;
use crate::error::CliError;
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
    pub(crate) scope_notice: Option<String>,
}

impl ExecutionPlan {
    pub(crate) fn mode(&self) -> ExecutionMode {
        self.capture_plan.execution_mode
    }

    pub(crate) fn processing_path(&self) -> ProcessingPath {
        self.capture_plan.processing_path
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

fn normalized_backend_override(raw: Option<OsString>) -> Option<PathBuf> {
    raw.filter(|value| !value.is_empty()).map(PathBuf::from)
}

pub(crate) fn env_backend_override() -> Option<PathBuf> {
    normalized_backend_override(env::var_os("FORMED_BACKEND_GCC"))
}

pub(crate) fn env_launcher_override() -> Option<PathBuf> {
    normalized_backend_override(env::var_os("FORMED_BACKEND_LAUNCHER"))
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
        ProcessingPath::DualSinkStructured
            if compatibility_seam.should_inject_sarif(mode, processing_path) =>
        {
            StructuredCapturePolicy::SarifFile
        }
        ProcessingPath::SingleSinkStructured if compatibility_seam.prefers_json_single_sink() => {
            StructuredCapturePolicy::SingleSinkJsonFile
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
            processing_path,
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
) -> Result<ExecutionPlan, CliError> {
    let backend = cache
        .get_or_probe(ResolveRequest {
            cli_backend: parsed.backend.clone(),
            env_backend: env_backend_override(),
            config_backend: config.backend.gcc.clone(),
            cli_launcher: parsed.launcher.clone(),
            env_launcher: env_launcher_override(),
            config_launcher: config.backend.launcher.clone(),
            invoked_as: argv0.to_string(),
            wrapper_path: env::current_exe().ok(),
        })
        .map_err(|e| CliError::Backend(e.to_string()))?;
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
    )
    .map_err(CliError::Config)?;
    let profile = parsed
        .profile
        .or(config.render.profile)
        .unwrap_or_else(|| detect_profile_from_capabilities(&capabilities));
    let debug_refs = parsed
        .debug_refs
        .or(config.render.debug_refs)
        .unwrap_or(DebugRefs::None);
    let retention_policy = if parsed.trace_bundle.is_some() {
        RetentionPolicy::Always
    } else {
        parsed
            .trace
            .or(config.trace.retention_policy)
            .unwrap_or(RetentionPolicy::OnWrapperFailure)
    };
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
    use diag_backend_probe::VersionBand;

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
    fn gcc15_render_plan_keeps_dual_sink_capture() {
        let plan = build_capture_plan(
            &CliCompatibilitySeam::from_version_band(VersionBand::Gcc15),
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
    fn gcc13_shadow_plan_keeps_native_text_capture() {
        let plan = build_capture_plan(
            &CliCompatibilitySeam::from_version_band(VersionBand::Gcc13_14),
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
    fn gcc9_render_plan_keeps_native_text_capture() {
        let plan = build_capture_plan(
            &CliCompatibilitySeam::from_version_band(VersionBand::Gcc9_12),
            ExecutionMode::Render,
            ProcessingPath::NativeTextCapture,
            RetentionPolicy::OnWrapperFailure,
            DebugRefs::None,
            &tty_capabilities(),
            &[],
        );

        assert_eq!(plan.processing_path, ProcessingPath::NativeTextCapture);
        assert_eq!(plan.structured_capture, StructuredCapturePolicy::Disabled);
        assert_eq!(
            plan.native_text_capture,
            NativeTextCapturePolicy::CaptureOnly
        );
        assert!(plan.preserve_native_color);
        assert_eq!(plan.locale_handling, LocaleHandling::ForceMessagesC);
    }

    #[test]
    fn passthrough_plan_only_tees_when_retention_or_debug_requires_it() {
        let passthrough = build_capture_plan(
            &CliCompatibilitySeam::from_version_band(VersionBand::Gcc13_14),
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
            &CliCompatibilitySeam::from_version_band(VersionBand::Gcc13_14),
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
            &CliCompatibilitySeam::from_version_band(VersionBand::Gcc13_14),
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
    fn gcc13_single_sink_structured_plan_uses_explicit_sarif_file_capture() {
        let plan = build_capture_plan(
            &CliCompatibilitySeam::from_version_band(VersionBand::Gcc13_14),
            ExecutionMode::Render,
            ProcessingPath::SingleSinkStructured,
            RetentionPolicy::OnWrapperFailure,
            DebugRefs::None,
            &tty_capabilities(),
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
        assert!(plan.preserve_native_color);
    }

    #[test]
    fn gcc9_single_sink_structured_plan_uses_explicit_json_file_capture() {
        let plan = build_capture_plan(
            &CliCompatibilitySeam::from_version_band(VersionBand::Gcc9_12),
            ExecutionMode::Render,
            ProcessingPath::SingleSinkStructured,
            RetentionPolicy::OnWrapperFailure,
            DebugRefs::None,
            &tty_capabilities(),
            &[],
        );

        assert_eq!(plan.processing_path, ProcessingPath::SingleSinkStructured);
        assert_eq!(
            plan.structured_capture,
            StructuredCapturePolicy::SingleSinkJsonFile
        );
        assert_eq!(
            plan.native_text_capture,
            NativeTextCapturePolicy::CaptureOnly
        );
        assert!(plan.preserve_native_color);
    }

    #[test]
    fn normalized_backend_override_ignores_empty_env_values() {
        assert_eq!(normalized_backend_override(Some(OsString::new())), None);
        assert_eq!(
            normalized_backend_override(Some(OsString::from("/opt/gcc"))),
            Some(PathBuf::from("/opt/gcc"))
        );
    }
}
