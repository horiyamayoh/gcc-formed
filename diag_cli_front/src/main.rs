use diag_adapter_gcc::{ingest, producer_for_version, tool_for_backend};
use diag_backend_probe::{ProbeCache, ResolveRequest, SupportTier};
use diag_capture_runtime::{CaptureRequest, ExecutionMode, cleanup_capture, run_capture};
use diag_core::{DiagnosticDocument, LanguageMode, RunInfo, WrapperSurface};
use diag_enrich::enrich_document;
use diag_render::{
    DebugRefs, PathPolicy, RenderCapabilities, RenderProfile, RenderRequest, SourceExcerptPolicy,
    StreamKind, TypeDisplayPolicy, WarningVisibility, render,
};
use diag_trace::{
    BuildManifest, RetentionPolicy, TraceArtifactRef, TraceEnvelope, WrapperPaths,
    default_build_manifest, trace_id, write_trace,
};
use serde::Deserialize;
use std::env;
use std::ffi::{OsStr, OsString};
use std::fs;
use std::io::{IsTerminal, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

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
    let profile = parsed
        .profile
        .or(config.render.profile)
        .unwrap_or_else(detect_profile);
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
        retention: retention_policy,
        paths: paths.clone(),
        inject_sarif: mode != ExecutionMode::Passthrough
            && matches!(backend.support_tier, SupportTier::A),
    })?;

    let exit_code = exit_code_from_status(&capture.exit_status);
    if matches!(mode, ExecutionMode::Passthrough) {
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
            &parsed,
            &backend,
            &mode_decision,
            profile,
            capture.retained_trace_dir.as_ref(),
        )?;
        cleanup_capture(&capture)?;
        return Ok(exit_code);
    }

    let render_result = render(RenderRequest {
        document: document.clone(),
        profile,
        capabilities: detect_capabilities(),
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
    let mut stderr = std::io::stderr().lock();
    stderr.write_all(render_result.text.as_bytes())?;
    stderr.write_all(b"\n")?;

    maybe_write_trace(
        &paths,
        &document,
        &parsed,
        &backend,
        &mode_decision,
        profile,
        capture.retained_trace_dir.as_ref(),
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
    Ok(status.code().unwrap_or(1))
}

fn maybe_write_trace(
    paths: &WrapperPaths,
    document: &DiagnosticDocument,
    parsed: &ParsedArgs,
    backend: &diag_backend_probe::ProbeResult,
    mode_decision: &ModeDecision,
    profile: RenderProfile,
    retained_trace_dir: Option<&PathBuf>,
) -> Result<(), Box<dyn std::error::Error>> {
    if retained_trace_dir.is_none()
        && !matches!(
            parsed.debug_refs,
            Some(DebugRefs::TraceId | DebugRefs::CaptureRef)
        )
    {
        return Ok(());
    }
    let trace = TraceEnvelope {
        trace_id: document.run.invocation_id.clone(),
        selected_mode: format!("{:?}", mode_decision.mode).to_lowercase(),
        selected_profile: format!("{profile:?}").to_lowercase(),
        support_tier: format!("{:?}", backend.support_tier).to_lowercase(),
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
    write_trace(paths, &trace, "trace.json")?;
    Ok(())
}

fn build_trace_artifact_refs(
    document: &DiagnosticDocument,
    retained_trace_dir: Option<&Path>,
) -> Vec<TraceArtifactRef> {
    let mut refs = document
        .captures
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
    }

    refs
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
            let mut cache = ProbeCache::default();
            let backend = cache.get_or_probe(ResolveRequest {
                explicit_backend: None,
                env_backend: env::var_os("FORMED_BACKEND_GCC").map(PathBuf::from),
                invoked_as: "gcc-formed".to_string(),
            })?;
            paths.ensure_dirs()?;
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({
                    "binary": "ok",
                    "paths": "ok",
                    "backend": backend.resolved_path,
                    "support_tier": format!("{:?}", backend.support_tier).to_lowercase(),
                }))?
            );
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

fn detect_profile() -> RenderProfile {
    let capabilities = detect_capabilities();
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
}
