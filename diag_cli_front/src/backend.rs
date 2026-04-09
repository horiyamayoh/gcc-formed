use crate::args::ParsedArgs;
use crate::config::ConfigFile;
use crate::mode::{
    CliCompatibilitySeam, ModeDecision, compatibility_scope_notice_for_seam, detect_capabilities,
    detect_profile_from_capabilities, has_hard_conflict, select_mode_for_seam,
    should_capture_passthrough_stderr,
};
use diag_backend_probe::{ProbeCache, ProbeResult, ResolveRequest};
use diag_capture_runtime::{CaptureRequest, ExecutionMode};
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
    retention_policy: RetentionPolicy,
    pub(crate) capabilities: RenderCapabilities,
    capture_passthrough_stderr: bool,
    inject_sarif: bool,
    pub(crate) scope_notice: Option<&'static str>,
}

impl ExecutionPlan {
    pub(crate) fn mode(&self) -> ExecutionMode {
        self.mode_decision.mode
    }

    pub(crate) fn capture_request(
        &self,
        paths: &WrapperPaths,
        parsed: &ParsedArgs,
        cwd: &Path,
    ) -> CaptureRequest {
        CaptureRequest {
            backend: self.backend.clone(),
            args: parsed.forwarded_args.clone(),
            cwd: cwd.to_path_buf(),
            mode: self.mode(),
            capture_passthrough_stderr: self.capture_passthrough_stderr,
            retention: self.retention_policy,
            paths: paths.clone(),
            inject_sarif: self.inject_sarif,
        }
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
        capture_passthrough_stderr: should_capture_passthrough_stderr(retention_policy, debug_refs),
        inject_sarif: compatibility_seam.should_inject_sarif(mode_decision.mode),
        backend,
        mode_decision,
        profile,
        debug_refs,
        retention_policy,
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
