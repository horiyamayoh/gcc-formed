use crate::args::ParsedArgs;
use crate::config::ConfigFile;
use crate::mode::{
    ModeDecision, compatibility_scope_notice, has_hard_conflict, select_mode,
    should_capture_passthrough_stderr,
};
use diag_backend_probe::{ProbeCache, ResolveRequest, SupportTier};
use diag_capture_runtime::{CaptureRequest, ExecutionMode, ExitStatusInfo};
use diag_core::LanguageMode;
use diag_render::{DebugRefs, RenderCapabilities, RenderProfile, StreamKind};
use diag_trace::{RetentionPolicy, WrapperPaths};
use std::env;
use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Debug, Clone)]
pub(crate) struct ExecutionPlan {
    pub(crate) backend: diag_backend_probe::ProbeResult,
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
    let mode_decision = select_mode(backend.support_tier, explicit_mode, hard_conflict);
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
        scope_notice: compatibility_scope_notice(backend.support_tier, &mode_decision),
        capture_passthrough_stderr: should_capture_passthrough_stderr(retention_policy, debug_refs),
        inject_sarif: mode_decision.mode != ExecutionMode::Passthrough
            && matches!(backend.support_tier, SupportTier::A),
        backend,
        mode_decision,
        profile,
        debug_refs,
        retention_policy,
        capabilities,
    })
}

pub(crate) fn passthrough_inherit(
    backend: &Path,
    forwarded_args: &[std::ffi::OsString],
    cwd: &Path,
) -> Result<i32, Box<dyn std::error::Error>> {
    let status = Command::new(backend)
        .current_dir(cwd)
        .args(forwarded_args)
        .stdin(std::process::Stdio::inherit())
        .stdout(std::process::Stdio::inherit())
        .stderr(std::process::Stdio::inherit())
        .status()?;
    Ok(exit_code_from_process_status(&status))
}

pub(crate) fn detect_capabilities() -> RenderCapabilities {
    let stderr = std::io::stderr();
    let is_terminal = std::io::IsTerminal::is_terminal(&stderr);
    let width = env::var("COLUMNS")
        .ok()
        .and_then(|value| value.parse().ok())
        .or(Some(100));
    RenderCapabilities {
        stream_kind: if is_ci() {
            StreamKind::CiLog
        } else if is_terminal {
            StreamKind::Tty
        } else {
            StreamKind::Pipe
        },
        width_columns: width,
        ansi_color: is_terminal,
        unicode: false,
        hyperlinks: false,
        interactive: is_terminal,
    }
}

pub(crate) fn detect_profile_from_capabilities(capabilities: &RenderCapabilities) -> RenderProfile {
    match capabilities.stream_kind {
        StreamKind::CiLog => RenderProfile::Ci,
        StreamKind::Tty if capabilities.interactive => RenderProfile::Default,
        _ => RenderProfile::Concise,
    }
}

pub(crate) fn is_ci() -> bool {
    env::var_os("CI").is_some()
}

pub(crate) fn language_mode_from_invocation(invoked_as: &str) -> LanguageMode {
    if invoked_as.contains("g++") || invoked_as.contains("c++") {
        LanguageMode::Cpp
    } else {
        LanguageMode::C
    }
}

pub(crate) fn exit_code_from_status(status: &ExitStatusInfo) -> i32 {
    status
        .code
        .or_else(|| status.signal.map(|signal| 128 + signal))
        .unwrap_or(1)
}

pub(crate) fn exit_code_from_process_status(status: &std::process::ExitStatus) -> i32 {
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;

        status
            .code()
            .or_else(|| status.signal().map(|signal| 128 + signal))
            .unwrap_or(1)
    }
    #[cfg(not(unix))]
    {
        status.code().unwrap_or(1)
    }
}

pub(crate) fn backend_binary_name(backend: &diag_backend_probe::ProbeResult) -> &str {
    backend
        .resolved_path
        .file_name()
        .and_then(OsStr::to_str)
        .unwrap_or("gcc")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn signal_exit_status_uses_conventional_code() {
        let status = ExitStatusInfo {
            code: None,
            signal: Some(15),
            success: false,
        };
        assert_eq!(exit_code_from_status(&status), 143);
    }

    #[test]
    fn ci_profile_follows_capabilities() {
        let capabilities = RenderCapabilities {
            stream_kind: StreamKind::CiLog,
            width_columns: Some(120),
            ansi_color: false,
            unicode: false,
            hyperlinks: false,
            interactive: false,
        };
        assert_eq!(
            detect_profile_from_capabilities(&capabilities),
            RenderProfile::Ci
        );
    }
}
