use crate::args::ParsedArgs;
use crate::backend::build_execution_plan;
use crate::config::ConfigFile;
use crate::mode::is_compiler_introspection;
use crate::render::{
    argv_for_trace, build_language_mode, build_primary_tool, maybe_write_passthrough_trace,
    maybe_write_trace, wrapper_surface,
};
use crate::self_check::handle_wrapper_introspection;
use diag_adapter_gcc::{ingest_with_reason, producer_for_version};
use diag_backend_probe::ProbeCache;
use diag_capture_runtime::{ExecutionMode, ExitStatusInfo, cleanup_capture, run_capture};
use diag_core::RunInfo;
use diag_enrich::enrich_document;
use diag_render::{
    PathPolicy, RenderRequest, SourceExcerptPolicy, TypeDisplayPolicy, WarningVisibility, render,
};
use diag_trace::{WrapperPaths, trace_id};
use std::env;
use std::ffi::OsString;
use std::io::Write;
use std::path::Path;
use std::process::{Command, ExitCode};
use std::time::Instant;

pub(crate) fn entrypoint() -> ExitCode {
    match real_main() {
        Ok(code) => ExitCode::from(code as u8),
        Err(error) => {
            eprintln!("gcc-formed: {error}");
            ExitCode::from(1)
        }
    }
}

fn real_main() -> Result<i32, Box<dyn std::error::Error>> {
    let wrapper_started = Instant::now();
    let argv0 = env::args()
        .next()
        .unwrap_or_else(|| "gcc-formed".to_string());
    let parsed = ParsedArgs::parse(env::args_os().collect())?;
    let paths = WrapperPaths::discover();
    let config = ConfigFile::load(&paths)?;

    if let Some(command) = parsed.introspection {
        return handle_wrapper_introspection(command, &paths);
    }

    let mut cache = ProbeCache::default();
    let plan = build_execution_plan(&argv0, &parsed, &config, &mut cache)?;

    if is_compiler_introspection(&parsed.forwarded_args) {
        return passthrough_inherit(
            &plan.backend.resolved_path,
            &parsed.forwarded_args,
            &env::current_dir()?,
        );
    }

    if let Some(note) = plan.scope_notice {
        eprintln!("{note}");
    }

    let cwd = env::current_dir()?;
    let capture = run_capture(&plan.capture_request(&paths, &parsed, &cwd))?;
    let exit_code = exit_code_from_status(&capture.exit_status);

    if matches!(plan.mode(), ExecutionMode::Passthrough) {
        maybe_write_passthrough_trace(
            &paths,
            &capture,
            &parsed,
            &plan.backend,
            &plan.mode_decision,
            plan.profile,
            &plan.capabilities,
            wrapper_started.elapsed().as_millis() as u64,
        )?;
        cleanup_capture(&capture)?;
        return Ok(exit_code);
    }

    let run_info = RunInfo {
        invocation_id: trace_id(),
        invoked_as: Some(argv0.clone()),
        argv_redacted: argv_for_trace(&parsed),
        cwd_display: Some(cwd.display().to_string()),
        exit_status: exit_code,
        primary_tool: build_primary_tool(&plan.backend),
        secondary_tools: Vec::new(),
        language_mode: Some(build_language_mode(&argv0)),
        target_triple: None,
        wrapper_mode: Some(wrapper_surface()),
    };
    let authoritative_sarif_path = capture.authoritative_sarif_path();
    let stderr_text = capture.stderr_text();
    let ingest_outcome = ingest_with_reason(
        authoritative_sarif_path.as_deref(),
        stderr_text.as_ref(),
        producer_for_version(env!("CARGO_PKG_VERSION")),
        run_info,
    )?;
    let mut document = ingest_outcome.document;
    document.captures = capture.capture_artifacts();
    enrich_document(&mut document, &cwd);

    if matches!(plan.mode(), ExecutionMode::Shadow) {
        maybe_write_trace(
            &paths,
            &document,
            &capture,
            &parsed,
            &plan.backend,
            &plan.mode_decision,
            plan.profile,
            &plan.capabilities,
            plan.mode_decision
                .fallback_reason
                .or(ingest_outcome.fallback_reason),
            None,
            wrapper_started.elapsed().as_millis() as u64,
        )?;
        cleanup_capture(&capture)?;
        return Ok(exit_code);
    }

    let render_started = Instant::now();
    let render_result = render(RenderRequest {
        document: document.clone(),
        profile: plan.profile,
        capabilities: plan.capabilities.clone(),
        cwd: Some(cwd),
        path_policy: config
            .render
            .path_policy
            .unwrap_or(PathPolicy::ShortestUnambiguous),
        warning_visibility: WarningVisibility::Auto,
        debug_refs: plan.debug_refs,
        type_display_policy: TypeDisplayPolicy::CompactSafe,
        source_excerpt_policy: SourceExcerptPolicy::Auto,
    })?;
    let effective_fallback_reason = plan
        .mode_decision
        .fallback_reason
        .or(ingest_outcome.fallback_reason)
        .or(render_result.fallback_reason);
    let render_duration_ms = render_started.elapsed().as_millis() as u64;
    let mut stderr = std::io::stderr().lock();
    stderr.write_all(render_result.text.as_bytes())?;
    stderr.write_all(b"\n")?;

    maybe_write_trace(
        &paths,
        &document,
        &capture,
        &parsed,
        &plan.backend,
        &plan.mode_decision,
        plan.profile,
        &plan.capabilities,
        effective_fallback_reason,
        Some(render_duration_ms),
        wrapper_started.elapsed().as_millis() as u64,
    )?;
    cleanup_capture(&capture)?;
    Ok(exit_code)
}

fn passthrough_inherit(
    backend: &Path,
    forwarded_args: &[OsString],
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

pub(crate) fn exit_code_from_status(status: &ExitStatusInfo) -> i32 {
    status
        .code
        .or_else(|| status.signal.map(|signal| 128 + signal))
        .unwrap_or(1)
}

fn exit_code_from_process_status(status: &std::process::ExitStatus) -> i32 {
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
}
