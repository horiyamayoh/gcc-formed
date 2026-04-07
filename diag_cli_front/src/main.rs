use diag_adapter_gcc::{ingest, producer_for_version, tool_for_backend};
use diag_backend_probe::{ProbeCache, ResolveRequest, SupportTier};
use diag_capture_runtime::{
    CaptureOutcome, CaptureRequest, ExecutionMode, ExitStatusInfo, cleanup_capture, run_capture,
    trace_sanitized_env_keys,
};
use diag_core::{
    DiagnosticDocument, DocumentCompleteness, LanguageMode, RunInfo, SnapshotKind, WrapperSurface,
    snapshot_json,
};
use diag_enrich::enrich_document;
use diag_render::{
    DebugRefs, PathPolicy, RenderCapabilities, RenderProfile, RenderRequest, SourceExcerptPolicy,
    StreamKind, TypeDisplayPolicy, WarningVisibility, render,
};
use diag_trace::{
    BuildManifest, RetentionPolicy, TraceArtifactRef, TraceCapabilities, TraceChildExit,
    TraceEnvelope, TraceEnvironmentSummary, TraceFingerprintSummary, TraceParserResultSummary,
    TraceRedactionStatus, TraceTiming, TraceVersionSummary, WrapperPaths, build_target_triple,
    default_build_manifest, secure_private_file, trace_id, write_trace, write_trace_at,
};
use serde::Deserialize;
use serde_json::json;
use std::env;
use std::ffi::{OsStr, OsString};
use std::fs;
use std::io::{IsTerminal, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};
use std::time::Instant;

fn main() -> ExitCode {
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

    if let Some(command) = parsed.introspection.clone() {
        return handle_wrapper_introspection(command, &paths);
    }

    let mut cache = ProbeCache::default();
    let backend = cache.get_or_probe(ResolveRequest {
        explicit_backend: parsed.backend.clone().or(config.backend.gcc.clone()),
        env_backend: env::var_os("FORMED_BACKEND_GCC").map(PathBuf::from),
        invoked_as: argv0.clone(),
    })?;

    if is_compiler_introspection(&parsed.forwarded_args) {
        return passthrough_inherit(
            &backend.resolved_path,
            &parsed.forwarded_args,
            &env::current_dir()?,
        );
    }

    let explicit_mode = parsed.mode.or(config.runtime.mode);
    let hard_conflict = has_hard_conflict(&parsed.forwarded_args);
    let mode_decision = select_mode(backend.support_tier, explicit_mode, hard_conflict);
    let mode = mode_decision.mode;
    let capabilities = detect_capabilities();
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

    let cwd = env::current_dir()?;
    let capture = run_capture(&CaptureRequest {
        backend: backend.clone(),
        args: parsed.forwarded_args.clone(),
        cwd: cwd.clone(),
        mode,
        capture_passthrough_stderr: should_capture_passthrough_stderr(retention_policy, debug_refs),
        retention: retention_policy,
        paths: paths.clone(),
        inject_sarif: mode != ExecutionMode::Passthrough
            && matches!(backend.support_tier, SupportTier::A),
    })?;

    let exit_code = exit_code_from_status(&capture.exit_status);
    if matches!(mode, ExecutionMode::Passthrough) {
        maybe_write_passthrough_trace(
            &paths,
            &capture,
            &parsed,
            &backend,
            &mode_decision,
            profile,
            &capabilities,
            wrapper_started.elapsed().as_millis() as u64,
        )?;
        cleanup_capture(&capture)?;
        return Ok(exit_code);
    }

    let run_info = RunInfo {
        invocation_id: trace_id(),
        invoked_as: Some(argv0.clone()),
        argv_redacted: parsed.forwarded_args.iter().map(os_to_string).collect(),
        cwd_display: Some(cwd.display().to_string()),
        exit_status: exit_code,
        primary_tool: tool_for_backend(
            backend
                .resolved_path
                .file_name()
                .and_then(OsStr::to_str)
                .unwrap_or("gcc"),
            Some(backend.version_string.clone()),
        ),
        secondary_tools: Vec::new(),
        language_mode: Some(language_mode_from_invocation(&argv0)),
        target_triple: None,
        wrapper_mode: Some(if is_ci() {
            WrapperSurface::Ci
        } else {
            WrapperSurface::Terminal
        }),
    };
    let mut document = ingest(
        capture.sarif_path.as_deref(),
        &String::from_utf8_lossy(&capture.stderr_bytes),
        producer_for_version(env!("CARGO_PKG_VERSION")),
        run_info,
    )?;
    document.captures = capture.artifacts.clone();
    enrich_document(&mut document, &cwd);

    if matches!(mode, ExecutionMode::Shadow) {
        maybe_write_trace(
            &paths,
            &document,
            &capture,
            &parsed,
            &backend,
            &mode_decision,
            profile,
            &capabilities,
            None,
            wrapper_started.elapsed().as_millis() as u64,
        )?;
        cleanup_capture(&capture)?;
        return Ok(exit_code);
    }

    let render_started = Instant::now();
    let render_result = render(RenderRequest {
        document: document.clone(),
        profile,
        capabilities: capabilities.clone(),
        cwd: Some(cwd),
        path_policy: config
            .render
            .path_policy
            .unwrap_or(PathPolicy::ShortestUnambiguous),
        warning_visibility: WarningVisibility::Auto,
        debug_refs,
        type_display_policy: TypeDisplayPolicy::CompactSafe,
        source_excerpt_policy: SourceExcerptPolicy::Auto,
    })?;
    let render_duration_ms = render_started.elapsed().as_millis() as u64;
    let mut stderr = std::io::stderr().lock();
    stderr.write_all(render_result.text.as_bytes())?;
    stderr.write_all(b"\n")?;

    maybe_write_trace(
        &paths,
        &document,
        &capture,
        &parsed,
        &backend,
        &mode_decision,
        profile,
        &capabilities,
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

fn maybe_write_trace(
    paths: &WrapperPaths,
    document: &DiagnosticDocument,
    capture: &CaptureOutcome,
    parsed: &ParsedArgs,
    backend: &diag_backend_probe::ProbeResult,
    mode_decision: &ModeDecision,
    profile: RenderProfile,
    capabilities: &RenderCapabilities,
    render_duration_ms: Option<u64>,
    total_duration_ms: u64,
) -> Result<(), Box<dyn std::error::Error>> {
    let retained_trace_dir = capture.retained_trace_dir.as_ref();
    if retained_trace_dir.is_none()
        && !matches!(
            parsed.debug_refs,
            Some(DebugRefs::TraceId | DebugRefs::CaptureRef)
        )
    {
        return Ok(());
    }
    if let Some(dir) = retained_trace_dir {
        write_retained_normalized_ir(dir, document)?;
    }
    let trace = TraceEnvelope {
        trace_id: document.run.invocation_id.clone(),
        selected_mode: format!("{:?}", mode_decision.mode).to_lowercase(),
        selected_profile: format!("{profile:?}").to_lowercase(),
        support_tier: format!("{:?}", backend.support_tier).to_lowercase(),
        wrapper_verdict: Some(trace_wrapper_verdict(
            mode_decision.mode,
            mode_decision.fallback_reason,
        )),
        version_summary: Some(trace_version_summary()),
        environment_summary: Some(trace_environment_summary(
            backend,
            mode_decision.mode,
            capture,
        )),
        capabilities: Some(trace_capabilities(capabilities)),
        timing: Some(TraceTiming {
            capture_ms: capture.capture_duration_ms,
            render_ms: render_duration_ms,
            total_ms: total_duration_ms,
        }),
        child_exit: Some(trace_child_exit(&capture.exit_status)),
        parser_result_summary: Some(parsed_parser_result_summary(document)),
        fingerprint_summary: trace_fingerprint_summary_from_document(document),
        redaction_status: Some(trace_redaction_status(
            mode_decision.mode,
            retained_trace_dir.is_some(),
        )),
        decision_log: mode_decision.decision_log.clone(),
        fallback_reason: mode_decision.fallback_reason.map(str::to_string),
        warning_messages: document
            .integrity_issues
            .iter()
            .map(|issue| issue.message.clone())
            .collect(),
        artifacts: build_trace_artifact_refs(
            document,
            retained_trace_dir.map(|path| path.as_path()),
        ),
    };
    if let Some(dir) = retained_trace_dir {
        write_trace_at(&dir.join("trace.json"), &trace)?;
    }
    write_trace(paths, &trace, "trace.json")?;
    Ok(())
}

fn maybe_write_passthrough_trace(
    paths: &WrapperPaths,
    capture: &CaptureOutcome,
    parsed: &ParsedArgs,
    backend: &diag_backend_probe::ProbeResult,
    mode_decision: &ModeDecision,
    profile: RenderProfile,
    capabilities: &RenderCapabilities,
    total_duration_ms: u64,
) -> Result<(), Box<dyn std::error::Error>> {
    let retained_trace_dir = capture.retained_trace_dir.as_ref();
    if retained_trace_dir.is_none()
        && !matches!(
            parsed.debug_refs,
            Some(DebugRefs::TraceId | DebugRefs::CaptureRef)
        )
    {
        return Ok(());
    }

    let trace = TraceEnvelope {
        trace_id: trace_id(),
        selected_mode: format!("{:?}", mode_decision.mode).to_lowercase(),
        selected_profile: format!("{profile:?}").to_lowercase(),
        support_tier: format!("{:?}", backend.support_tier).to_lowercase(),
        wrapper_verdict: Some(trace_wrapper_verdict(
            mode_decision.mode,
            mode_decision.fallback_reason,
        )),
        version_summary: Some(trace_version_summary()),
        environment_summary: Some(trace_environment_summary(
            backend,
            mode_decision.mode,
            capture,
        )),
        capabilities: Some(trace_capabilities(capabilities)),
        timing: Some(TraceTiming {
            capture_ms: capture.capture_duration_ms,
            render_ms: None,
            total_ms: total_duration_ms,
        }),
        child_exit: Some(trace_child_exit(&capture.exit_status)),
        parser_result_summary: Some(skipped_parser_result_summary(&capture.artifacts)),
        fingerprint_summary: Some(trace_fingerprint_summary_from_capture(capture)),
        redaction_status: Some(trace_redaction_status(
            mode_decision.mode,
            retained_trace_dir.is_some(),
        )),
        decision_log: mode_decision.decision_log.clone(),
        fallback_reason: mode_decision.fallback_reason.map(str::to_string),
        warning_messages: Vec::new(),
        artifacts: build_trace_artifact_refs_for_captures(
            &capture.artifacts,
            retained_trace_dir.map(|path| path.as_path()),
        ),
    };

    if let Some(dir) = retained_trace_dir {
        write_trace_at(&dir.join("trace.json"), &trace)?;
    }
    write_trace(paths, &trace, "trace.json")?;
    Ok(())
}

fn build_trace_artifact_refs(
    document: &DiagnosticDocument,
    retained_trace_dir: Option<&Path>,
) -> Vec<TraceArtifactRef> {
    build_trace_artifact_refs_for_captures(&document.captures, retained_trace_dir)
}

fn build_trace_artifact_refs_for_captures(
    captures: &[diag_core::CaptureArtifact],
    retained_trace_dir: Option<&Path>,
) -> Vec<TraceArtifactRef> {
    let mut refs = captures
        .iter()
        .map(|capture| TraceArtifactRef {
            id: capture.id.clone(),
            path: retained_trace_dir.and_then(|dir| {
                let candidate = dir.join(&capture.id);
                candidate.exists().then_some(candidate)
            }),
        })
        .collect::<Vec<_>>();

    if let Some(dir) = retained_trace_dir {
        let invocation = dir.join("invocation.json");
        if invocation.exists() {
            refs.push(TraceArtifactRef {
                id: "invocation.json".to_string(),
                path: Some(invocation),
            });
        }
        let normalized_ir = dir.join("ir.analysis.json");
        if normalized_ir.exists() {
            refs.push(TraceArtifactRef {
                id: "ir.analysis.json".to_string(),
                path: Some(normalized_ir),
            });
        }
        refs.push(TraceArtifactRef {
            id: "trace.json".to_string(),
            path: Some(dir.join("trace.json")),
        });
    }

    refs
}

fn trace_capabilities(capabilities: &RenderCapabilities) -> TraceCapabilities {
    TraceCapabilities {
        stream_kind: format!("{:?}", capabilities.stream_kind).to_lowercase(),
        width_columns: capabilities.width_columns,
        ansi_color: capabilities.ansi_color,
        unicode: capabilities.unicode,
        hyperlinks: capabilities.hyperlinks,
        interactive: capabilities.interactive,
    }
}

fn trace_version_summary() -> TraceVersionSummary {
    TraceVersionSummary {
        wrapper_version: env!("CARGO_PKG_VERSION").to_string(),
        build_target_triple: build_target_triple().to_string(),
        ir_spec_version: diag_core::IR_SPEC_VERSION.to_string(),
        adapter_spec_version: diag_core::ADAPTER_SPEC_VERSION.to_string(),
        renderer_spec_version: diag_core::RENDERER_SPEC_VERSION.to_string(),
    }
}

fn trace_environment_summary(
    backend: &diag_backend_probe::ProbeResult,
    mode: ExecutionMode,
    capture: &CaptureOutcome,
) -> TraceEnvironmentSummary {
    TraceEnvironmentSummary {
        backend_path: backend.resolved_path.clone(),
        backend_version: backend.version_string.clone(),
        injected_flags: trace_injected_flags(capture),
        sanitized_env_keys: trace_sanitized_env_keys(mode),
        temp_artifact_paths: trace_temp_artifact_paths(capture),
    }
}

fn trace_injected_flags(capture: &CaptureOutcome) -> Vec<String> {
    capture
        .sarif_path
        .as_ref()
        .map(|path| {
            vec![format!(
                "-fdiagnostics-add-output=sarif:version=2.1,file={}",
                path.display()
            )]
        })
        .unwrap_or_default()
}

fn trace_temp_artifact_paths(capture: &CaptureOutcome) -> Vec<PathBuf> {
    let mut paths = vec![
        capture.temp_dir.clone(),
        capture.temp_dir.join("invocation.json"),
    ];
    if let Some(sarif_path) = capture.sarif_path.as_ref() {
        paths.push(sarif_path.clone());
    }
    paths
}

fn trace_child_exit(status: &ExitStatusInfo) -> TraceChildExit {
    TraceChildExit {
        code: status.code,
        signal: status.signal,
        success: status.success,
    }
}

fn trace_wrapper_verdict(mode: ExecutionMode, fallback_reason: Option<&str>) -> String {
    match mode {
        ExecutionMode::Render => "rendered".to_string(),
        ExecutionMode::Shadow => "shadow_observed".to_string(),
        ExecutionMode::Passthrough => match fallback_reason {
            Some("explicit_passthrough") => "passthrough_requested".to_string(),
            _ => "passthrough_fallback".to_string(),
        },
    }
}

fn parsed_parser_result_summary(document: &DiagnosticDocument) -> TraceParserResultSummary {
    TraceParserResultSummary {
        status: "parsed".to_string(),
        document_completeness: Some(document_completeness_label(&document.document_completeness)),
        diagnostic_count: document.diagnostics.len(),
        integrity_issue_count: document.integrity_issues.len(),
        capture_count: document.captures.len(),
    }
}

fn skipped_parser_result_summary(
    captures: &[diag_core::CaptureArtifact],
) -> TraceParserResultSummary {
    TraceParserResultSummary {
        status: "skipped".to_string(),
        document_completeness: None,
        diagnostic_count: 0,
        integrity_issue_count: 0,
        capture_count: captures.len(),
    }
}

fn trace_fingerprint_summary_from_document(
    document: &DiagnosticDocument,
) -> Option<TraceFingerprintSummary> {
    document
        .fingerprints
        .as_ref()
        .map(|fingerprints| TraceFingerprintSummary {
            raw: fingerprints.raw.clone(),
            normalized: Some(fingerprints.structural.clone()),
            family: Some(fingerprints.family.clone()),
        })
}

fn trace_fingerprint_summary_from_capture(capture: &CaptureOutcome) -> TraceFingerprintSummary {
    TraceFingerprintSummary {
        raw: diag_core::fingerprint_for(&capture.stderr_bytes),
        normalized: None,
        family: None,
    }
}

fn trace_redaction_status(
    mode: ExecutionMode,
    retained_trace_dir_exists: bool,
) -> TraceRedactionStatus {
    TraceRedactionStatus {
        class: "restricted".to_string(),
        local_only: true,
        normalized_artifacts: if retained_trace_dir_exists
            && !matches!(mode, ExecutionMode::Passthrough)
        {
            vec!["ir.analysis.json".to_string()]
        } else {
            Vec::new()
        },
    }
}

fn document_completeness_label(completeness: &DocumentCompleteness) -> String {
    format!("{completeness:?}").to_lowercase()
}

fn write_retained_normalized_ir(
    retained_trace_dir: &Path,
    document: &DiagnosticDocument,
) -> Result<(), Box<dyn std::error::Error>> {
    let path = retained_trace_dir.join("ir.analysis.json");
    let payload = snapshot_json(document, SnapshotKind::AnalysisIncluded)?;
    fs::write(&path, payload)?;
    secure_private_file(&path)?;
    Ok(())
}

fn handle_wrapper_introspection(
    command: WrapperIntrospection,
    paths: &WrapperPaths,
) -> Result<i32, Box<dyn std::error::Error>> {
    match command {
        WrapperIntrospection::Version => {
            println!("{}", env!("CARGO_PKG_VERSION"));
        }
        WrapperIntrospection::VersionVerbose => {
            let manifest = build_manifest()?;
            println!("product version: {}", manifest.product_version);
            println!("target triple: {}", manifest.artifact_target_triple);
            println!("git commit: {}", manifest.git_commit);
            println!("build profile: {}", manifest.build_profile);
            println!("rustc version: {}", manifest.rustc_version);
            println!("cargo version: {}", manifest.cargo_version);
            println!("build timestamp: {}", manifest.build_timestamp);
            println!("support tier: {}", manifest.support_tier_declaration);
            println!("IR spec version: {}", manifest.ir_spec_version);
            println!("adapter spec version: {}", manifest.adapter_spec_version);
            println!("renderer spec version: {}", manifest.renderer_spec_version);
            println!("install root: {}", paths.install_root.display());
            println!("config path: {}", paths.config_path.display());
        }
        WrapperIntrospection::PrintPaths => {
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({
                    "config_path": paths.config_path,
                    "cache_root": paths.cache_root,
                    "state_root": paths.state_root,
                    "runtime_root": paths.runtime_root,
                    "trace_root": paths.trace_root,
                    "install_root": paths.install_root,
                }))?
            );
        }
        WrapperIntrospection::SelfCheck => {
            println!("{}", serde_json::to_string_pretty(&self_check(paths)?)?);
        }
        WrapperIntrospection::DumpBuildManifest => {
            println!("{}", serde_json::to_string_pretty(&build_manifest()?)?);
        }
    }
    Ok(0)
}

fn build_manifest() -> Result<BuildManifest, Box<dyn std::error::Error>> {
    let workspace_root = workspace_root();
    let lockfile_hash = hash_file(&workspace_root.join("Cargo.lock"))?;
    let vendor_hash = hash_vendor(&workspace_root.join("vendor"))?;
    Ok(default_build_manifest(lockfile_hash, vendor_hash))
}

fn self_check(paths: &WrapperPaths) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
    let manifest = build_manifest()?;
    let mut cache = ProbeCache::default();
    let backend = cache.get_or_probe(ResolveRequest {
        explicit_backend: None,
        env_backend: env::var_os("FORMED_BACKEND_GCC").map(PathBuf::from),
        invoked_as: "gcc-formed".to_string(),
    })?;

    paths.ensure_dirs()?;
    let state_access = probe_write_access(&paths.state_root);
    let runtime_access = probe_write_access(&paths.runtime_root);
    let install_probe_root = existing_probe_root(&paths.install_root);
    let install_access = probe_write_access(&install_probe_root);
    let target_matches_build = manifest.artifact_target_triple == build_target_triple();
    let install_root_includes_target =
        install_root_includes_target(&paths.install_root, &manifest.artifact_target_triple);
    let separated_roots = paths_are_separated(paths);
    let mut warnings = Vec::new();
    if !target_matches_build {
        warnings.push("build target triple diverges from embedded manifest".to_string());
    }
    if !install_root_includes_target {
        warnings.push("install root does not include target triple".to_string());
    }
    if !separated_roots {
        warnings.push("install root overlaps mutable state paths".to_string());
    }
    if state_access.is_err() {
        warnings.push("state root is not writable".to_string());
    }
    if runtime_access.is_err() {
        warnings.push("runtime root is not writable".to_string());
    }
    if install_access.is_err() {
        warnings.push("install root probe ancestor is not writable".to_string());
    }

    Ok(json!({
        "binary": "ok",
        "manifest": {
            "target_triple": manifest.artifact_target_triple,
            "target_triple_matches_build": target_matches_build,
            "support_tier": manifest.support_tier_declaration,
        },
        "paths": {
            "config_path": paths.config_path,
            "cache_root": paths.cache_root,
            "state_root": paths.state_root,
            "runtime_root": paths.runtime_root,
            "trace_root": paths.trace_root,
            "install_root": paths.install_root,
            "install_probe_root": install_probe_root,
            "state_root_access": access_label(&state_access),
            "runtime_root_access": access_label(&runtime_access),
            "install_root_access": access_label(&install_access),
            "install_root_includes_target_triple": install_root_includes_target,
            "separated_from_install_root": separated_roots,
        },
        "backend": {
            "path": backend.resolved_path,
            "version": backend.version_string,
            "support_tier": format!("{:?}", backend.support_tier).to_lowercase(),
        },
        "warnings": warnings,
    }))
}

fn hash_file(path: &Path) -> Result<String, Box<dyn std::error::Error>> {
    let contents = fs::read_to_string(path).unwrap_or_default();
    Ok(diag_core::fingerprint_for(&contents))
}

fn hash_vendor(path: &Path) -> Result<String, Box<dyn std::error::Error>> {
    if !path.exists() {
        return Ok("vendor-missing".to_string());
    }
    let mut entries = Vec::new();
    collect_paths(path, &mut entries)?;
    Ok(diag_core::fingerprint_for(&entries))
}

fn collect_paths(path: &Path, entries: &mut Vec<String>) -> Result<(), Box<dyn std::error::Error>> {
    for entry in fs::read_dir(path)? {
        let entry = entry?;
        let child = entry.path();
        if child.is_dir() {
            collect_paths(&child, entries)?;
        } else {
            entries.push(child.display().to_string());
        }
    }
    Ok(())
}

fn install_root_includes_target(install_root: &Path, target_triple: &str) -> bool {
    if target_triple == "unknown-target" {
        return true;
    }
    install_root
        .components()
        .any(|component| component.as_os_str() == OsStr::new(target_triple))
}

fn existing_probe_root(path: &Path) -> PathBuf {
    let mut current = path;
    loop {
        if current.exists() {
            return current.to_path_buf();
        }
        if let Some(parent) = current.parent() {
            current = parent;
        } else {
            return path.to_path_buf();
        }
    }
}

fn probe_write_access(path: &Path) -> Result<(), std::io::Error> {
    fs::create_dir_all(path)?;
    let probe = path.join(format!(
        ".formed-self-check-{}-{}",
        std::process::id(),
        trace_id()
    ));
    fs::write(&probe, b"ok")?;
    fs::remove_file(probe)?;
    Ok(())
}

fn access_label(result: &Result<(), std::io::Error>) -> &'static str {
    if result.is_ok() { "ok" } else { "error" }
}

fn paths_are_separated(paths: &WrapperPaths) -> bool {
    let install_root = &paths.install_root;
    for mutable_root in [
        &paths.cache_root,
        &paths.state_root,
        &paths.runtime_root,
        &paths.trace_root,
    ] {
        if mutable_root.starts_with(install_root) || install_root.starts_with(mutable_root) {
            return false;
        }
    }
    true
}

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .to_path_buf()
}

fn detect_capabilities() -> RenderCapabilities {
    let stderr = std::io::stderr();
    let is_terminal = stderr.is_terminal();
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

fn detect_profile_from_capabilities(capabilities: &RenderCapabilities) -> RenderProfile {
    match capabilities.stream_kind {
        StreamKind::CiLog => RenderProfile::Ci,
        StreamKind::Tty if capabilities.interactive => RenderProfile::Default,
        _ => RenderProfile::Concise,
    }
}

fn is_ci() -> bool {
    env::var_os("CI").is_some()
}

fn language_mode_from_invocation(invoked_as: &str) -> LanguageMode {
    if invoked_as.contains("g++") || invoked_as.contains("c++") {
        LanguageMode::Cpp
    } else {
        LanguageMode::C
    }
}

fn exit_code_from_status(status: &diag_capture_runtime::ExitStatusInfo) -> i32 {
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

fn is_compiler_introspection(args: &[OsString]) -> bool {
    args.iter().any(|arg| {
        let value = os_to_string(arg);
        matches!(
            value.as_str(),
            "--help" | "--version" | "-dumpmachine" | "-dumpversion" | "-dumpfullversion" | "-###"
        ) || value.starts_with("-dump")
            || value.starts_with("-print-")
    })
}

fn has_hard_conflict(args: &[OsString]) -> bool {
    args.iter().any(|arg| {
        let value = os_to_string(arg);
        value.starts_with("-fdiagnostics-format=")
            || value.starts_with("-fdiagnostics-add-output=")
            || value.starts_with("-fdiagnostics-set-output=")
            || value == "-fdiagnostics-parseable-fixits"
            || value == "-fdiagnostics-generate-patch"
    })
}

fn select_mode(
    tier: SupportTier,
    requested: Option<ExecutionMode>,
    hard_conflict: bool,
) -> ModeDecision {
    let mut decision_log = vec![format!("support_tier={:?}", tier).to_lowercase()];
    if hard_conflict {
        decision_log.push("hard_conflict=diagnostic_sink_override".to_string());
        return ModeDecision {
            mode: ExecutionMode::Passthrough,
            fallback_reason: Some("hard_conflict"),
            decision_log,
        };
    }
    if let Some(ExecutionMode::Passthrough) = requested {
        decision_log.push("requested_mode=passthrough".to_string());
        return ModeDecision {
            mode: ExecutionMode::Passthrough,
            fallback_reason: Some("explicit_passthrough"),
            decision_log,
        };
    }
    let mode = match tier {
        SupportTier::A => {
            decision_log.push(format!(
                "tier_a_mode={}",
                format!("{:?}", requested.unwrap_or(ExecutionMode::Render)).to_lowercase()
            ));
            requested.unwrap_or(ExecutionMode::Render)
        }
        SupportTier::B => match requested {
            Some(ExecutionMode::Shadow) => {
                decision_log.push("tier_b_mode=shadow_raw_only".to_string());
                ExecutionMode::Shadow
            }
            Some(ExecutionMode::Render) => {
                decision_log.push("tier_b_render_unsupported=passthrough".to_string());
                ExecutionMode::Passthrough
            }
            None => {
                decision_log.push("tier_b_default=passthrough".to_string());
                ExecutionMode::Passthrough
            }
            Some(ExecutionMode::Passthrough) => ExecutionMode::Passthrough,
        },
        SupportTier::C => {
            decision_log.push("tier_c_mode=passthrough_only".to_string());
            ExecutionMode::Passthrough
        }
    };

    let fallback_reason = match mode {
        ExecutionMode::Passthrough => match tier {
            SupportTier::A => None,
            SupportTier::B => Some(match requested {
                Some(ExecutionMode::Render) => "tier_b_render_unsupported",
                Some(ExecutionMode::Shadow) => unreachable!(),
                Some(ExecutionMode::Passthrough) => "explicit_passthrough",
                None => "tier_b_default",
            }),
            SupportTier::C => Some("tier_c_only"),
        },
        ExecutionMode::Render | ExecutionMode::Shadow => None,
    };

    ModeDecision {
        mode,
        fallback_reason,
        decision_log,
    }
}

fn os_to_string(value: &OsString) -> String {
    value.to_string_lossy().into_owned()
}

fn should_capture_passthrough_stderr(
    retention_policy: RetentionPolicy,
    debug_refs: DebugRefs,
) -> bool {
    matches!(
        retention_policy,
        RetentionPolicy::OnChildError | RetentionPolicy::Always
    ) || matches!(debug_refs, DebugRefs::CaptureRef)
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ModeDecision {
    mode: ExecutionMode,
    fallback_reason: Option<&'static str>,
    decision_log: Vec<String>,
}

#[derive(Debug, Clone, Default)]
struct ParsedArgs {
    mode: Option<ExecutionMode>,
    profile: Option<RenderProfile>,
    backend: Option<PathBuf>,
    trace: Option<RetentionPolicy>,
    debug_refs: Option<DebugRefs>,
    introspection: Option<WrapperIntrospection>,
    forwarded_args: Vec<OsString>,
}

impl ParsedArgs {
    fn parse(args: Vec<OsString>) -> Result<Self, Box<dyn std::error::Error>> {
        let mut parsed = ParsedArgs::default();
        for arg in args.into_iter().skip(1) {
            let value = arg.to_string_lossy();
            if let Some(mode) = value.strip_prefix("--formed-mode=") {
                parsed.mode = Some(parse_mode(mode)?);
            } else if let Some(profile) = value.strip_prefix("--formed-profile=") {
                parsed.profile = Some(parse_profile(profile)?);
            } else if let Some(path) = value.strip_prefix("--formed-backend-gcc=") {
                parsed.backend = Some(PathBuf::from(path));
            } else if let Some(policy) = value.strip_prefix("--formed-trace=") {
                parsed.trace = Some(parse_retention_policy(policy)?);
            } else if let Some(debug_refs) = value.strip_prefix("--formed-debug-refs=") {
                parsed.debug_refs = Some(parse_debug_refs(debug_refs)?);
            } else if value == "--formed-version" {
                parsed.introspection = Some(WrapperIntrospection::Version);
            } else if value == "--formed-version=verbose" {
                parsed.introspection = Some(WrapperIntrospection::VersionVerbose);
            } else if value == "--formed-print-paths" {
                parsed.introspection = Some(WrapperIntrospection::PrintPaths);
            } else if value == "--formed-self-check" {
                parsed.introspection = Some(WrapperIntrospection::SelfCheck);
            } else if value == "--formed-dump-build-manifest" {
                parsed.introspection = Some(WrapperIntrospection::DumpBuildManifest);
            } else {
                parsed.forwarded_args.push(arg);
            }
        }
        Ok(parsed)
    }
}

#[derive(Debug, Clone, Copy)]
enum WrapperIntrospection {
    Version,
    VersionVerbose,
    PrintPaths,
    SelfCheck,
    DumpBuildManifest,
}

fn parse_mode(value: &str) -> Result<ExecutionMode, Box<dyn std::error::Error>> {
    match value {
        "render" => Ok(ExecutionMode::Render),
        "shadow" => Ok(ExecutionMode::Shadow),
        "passthrough" => Ok(ExecutionMode::Passthrough),
        _ => Err(format!("unsupported mode: {value}").into()),
    }
}

fn parse_profile(value: &str) -> Result<RenderProfile, Box<dyn std::error::Error>> {
    match value {
        "default" => Ok(RenderProfile::Default),
        "concise" => Ok(RenderProfile::Concise),
        "verbose" => Ok(RenderProfile::Verbose),
        "ci" => Ok(RenderProfile::Ci),
        "raw_fallback" => Ok(RenderProfile::RawFallback),
        _ => Err(format!("unsupported profile: {value}").into()),
    }
}

fn parse_retention_policy(value: &str) -> Result<RetentionPolicy, Box<dyn std::error::Error>> {
    match value {
        "never" => Ok(RetentionPolicy::Never),
        "on-wrapper-failure" => Ok(RetentionPolicy::OnWrapperFailure),
        "on-child-error" => Ok(RetentionPolicy::OnChildError),
        "always" => Ok(RetentionPolicy::Always),
        _ => Err(format!("unsupported trace policy: {value}").into()),
    }
}

fn parse_debug_refs(value: &str) -> Result<DebugRefs, Box<dyn std::error::Error>> {
    match value {
        "none" => Ok(DebugRefs::None),
        "trace_id" => Ok(DebugRefs::TraceId),
        "capture_ref" => Ok(DebugRefs::CaptureRef),
        _ => Err(format!("unsupported debug ref mode: {value}").into()),
    }
}

#[derive(Debug, Clone, Default, Deserialize)]
struct ConfigFile {
    #[serde(default)]
    schema_version: Option<u32>,
    #[serde(default)]
    backend: BackendSection,
    #[serde(default)]
    runtime: RuntimeSection,
    #[serde(default)]
    render: RenderSection,
    #[serde(default)]
    trace: TraceSection,
}

#[derive(Debug, Clone, Default, Deserialize)]
struct BackendSection {
    #[serde(default)]
    gcc: Option<PathBuf>,
}

#[derive(Debug, Clone, Default, Deserialize)]
struct RuntimeSection {
    #[serde(default, deserialize_with = "deserialize_optional_mode")]
    mode: Option<ExecutionMode>,
}

#[derive(Debug, Clone, Default, Deserialize)]
struct RenderSection {
    #[serde(default, deserialize_with = "deserialize_optional_profile")]
    profile: Option<RenderProfile>,
    #[serde(default, deserialize_with = "deserialize_optional_path_policy")]
    path_policy: Option<PathPolicy>,
    #[serde(default, deserialize_with = "deserialize_optional_debug_refs")]
    debug_refs: Option<DebugRefs>,
}

#[derive(Debug, Clone, Default, Deserialize)]
struct TraceSection {
    #[serde(default, deserialize_with = "deserialize_optional_retention")]
    retention_policy: Option<RetentionPolicy>,
}

impl ConfigFile {
    fn load(paths: &WrapperPaths) -> Result<Self, Box<dyn std::error::Error>> {
        let mut merged = ConfigFile::default();
        if let Some(admin) = admin_config_path() {
            if admin.exists() {
                merged = merge_config(merged, toml::from_str(&fs::read_to_string(admin)?)?);
            }
        }
        if paths.config_path.exists() {
            merged = merge_config(
                merged,
                toml::from_str(&fs::read_to_string(&paths.config_path)?)?,
            );
        }
        Ok(merged)
    }
}

fn admin_config_path() -> Option<PathBuf> {
    let dirs = env::var_os("XDG_CONFIG_DIRS")
        .map(|value| env::split_paths(&value).collect::<Vec<_>>())
        .unwrap_or_else(|| vec![PathBuf::from("/etc/xdg")]);
    dirs.into_iter()
        .next()
        .map(|dir| dir.join("cc-formed").join("config.toml"))
}

fn merge_config(base: ConfigFile, overlay: ConfigFile) -> ConfigFile {
    ConfigFile {
        schema_version: overlay.schema_version.or(base.schema_version),
        backend: BackendSection {
            gcc: overlay.backend.gcc.or(base.backend.gcc),
        },
        runtime: RuntimeSection {
            mode: overlay.runtime.mode.or(base.runtime.mode),
        },
        render: RenderSection {
            profile: overlay.render.profile.or(base.render.profile),
            path_policy: overlay.render.path_policy.or(base.render.path_policy),
            debug_refs: overlay.render.debug_refs.or(base.render.debug_refs),
        },
        trace: TraceSection {
            retention_policy: overlay
                .trace
                .retention_policy
                .or(base.trace.retention_policy),
        },
    }
}

fn deserialize_optional_mode<'de, D>(deserializer: D) -> Result<Option<ExecutionMode>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = Option::<String>::deserialize(deserializer)?;
    value
        .map(|value| parse_mode(&value).map_err(serde::de::Error::custom))
        .transpose()
}

fn deserialize_optional_profile<'de, D>(deserializer: D) -> Result<Option<RenderProfile>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = Option::<String>::deserialize(deserializer)?;
    value
        .map(|value| parse_profile(&value).map_err(serde::de::Error::custom))
        .transpose()
}

fn deserialize_optional_path_policy<'de, D>(deserializer: D) -> Result<Option<PathPolicy>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = Option::<String>::deserialize(deserializer)?;
    match value.as_deref() {
        None => Ok(None),
        Some("shortest_unambiguous") => Ok(Some(PathPolicy::ShortestUnambiguous)),
        Some("relative_to_cwd") => Ok(Some(PathPolicy::RelativeToCwd)),
        Some("absolute") => Ok(Some(PathPolicy::Absolute)),
        Some(other) => Err(serde::de::Error::custom(format!(
            "unsupported path policy: {other}"
        ))),
    }
}

fn deserialize_optional_debug_refs<'de, D>(deserializer: D) -> Result<Option<DebugRefs>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = Option::<String>::deserialize(deserializer)?;
    value
        .map(|value| parse_debug_refs(&value).map_err(serde::de::Error::custom))
        .transpose()
}

fn deserialize_optional_retention<'de, D>(
    deserializer: D,
) -> Result<Option<RetentionPolicy>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = Option::<String>::deserialize(deserializer)?;
    value
        .map(|value| parse_retention_policy(&value).map_err(serde::de::Error::custom))
        .transpose()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selects_passthrough_with_reason_for_hard_conflict() {
        let decision = select_mode(SupportTier::A, None, true);
        assert_eq!(decision.mode, ExecutionMode::Passthrough);
        assert_eq!(decision.fallback_reason, Some("hard_conflict"));
        assert!(
            decision
                .decision_log
                .iter()
                .any(|entry| entry == "hard_conflict=diagnostic_sink_override")
        );
    }

    #[test]
    fn keeps_tier_b_shadow_without_fallback_reason() {
        let decision = select_mode(SupportTier::B, Some(ExecutionMode::Shadow), false);
        assert_eq!(decision.mode, ExecutionMode::Shadow);
        assert_eq!(decision.fallback_reason, None);
        assert!(
            decision
                .decision_log
                .iter()
                .any(|entry| entry == "tier_b_mode=shadow_raw_only")
        );
    }

    #[test]
    fn install_root_check_requires_target_component() {
        assert!(install_root_includes_target(
            Path::new("/home/test/.local/opt/cc-formed/x86_64-unknown-linux-musl"),
            "x86_64-unknown-linux-musl"
        ));
        assert!(!install_root_includes_target(
            Path::new("/home/test/.local/opt/cc-formed"),
            "x86_64-unknown-linux-musl"
        ));
    }

    #[test]
    fn mutable_paths_must_not_overlap_install_root() {
        let separated = WrapperPaths {
            config_path: PathBuf::from("/cfg/config.toml"),
            cache_root: PathBuf::from("/cache"),
            state_root: PathBuf::from("/state"),
            runtime_root: PathBuf::from("/runtime"),
            trace_root: PathBuf::from("/state/traces"),
            install_root: PathBuf::from("/opt/cc-formed/x86_64-unknown-linux-musl"),
        };
        let overlapping = WrapperPaths {
            config_path: PathBuf::from("/cfg/config.toml"),
            cache_root: PathBuf::from("/opt/cc-formed/cache"),
            state_root: PathBuf::from("/state"),
            runtime_root: PathBuf::from("/runtime"),
            trace_root: PathBuf::from("/state/traces"),
            install_root: PathBuf::from("/opt/cc-formed"),
        };

        assert!(paths_are_separated(&separated));
        assert!(!paths_are_separated(&overlapping));
    }

    #[test]
    fn captures_passthrough_stderr_only_when_requested() {
        assert!(should_capture_passthrough_stderr(
            RetentionPolicy::Always,
            DebugRefs::None
        ));
        assert!(should_capture_passthrough_stderr(
            RetentionPolicy::OnChildError,
            DebugRefs::None
        ));
        assert!(should_capture_passthrough_stderr(
            RetentionPolicy::Never,
            DebugRefs::CaptureRef
        ));
        assert!(!should_capture_passthrough_stderr(
            RetentionPolicy::Never,
            DebugRefs::None
        ));
        assert!(!should_capture_passthrough_stderr(
            RetentionPolicy::OnWrapperFailure,
            DebugRefs::TraceId
        ));
    }

    #[test]
    fn signal_exit_status_uses_conventional_code() {
        let status = diag_capture_runtime::ExitStatusInfo {
            code: None,
            signal: Some(15),
            success: false,
        };
        assert_eq!(exit_code_from_status(&status), 143);
    }
}
