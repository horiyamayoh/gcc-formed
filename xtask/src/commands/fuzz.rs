use diag_adapter_gcc::{ingest_with_reason, producer_for_version, tool_for_backend};
use diag_backend_probe::probe_backend;
use diag_capture_runtime::{CaptureRequest, ExecutionMode, cleanup_capture, run_capture};
use diag_core::{
    AnalysisOverlay, ArtifactKind, ArtifactStorage, CaptureArtifact, Confidence, ContextChain,
    ContextChainKind, ContextFrame, DiagnosticDocument, DiagnosticNode, DocumentCompleteness,
    FallbackReason, LanguageMode, Location, MessageText, NodeCompleteness, Origin, Ownership,
    Phase, ProducerInfo, Provenance, ProvenanceSource, RunInfo, SemanticRole, Severity, ToolInfo,
    ValidationErrors,
};
use diag_render::{
    DebugRefs, PathPolicy, RenderCapabilities, RenderProfile, RenderRequest, SourceExcerptPolicy,
    StreamKind, TypeDisplayPolicy, WarningVisibility, build_view_model, render,
};
use diag_trace::{
    RetentionPolicy, TraceArtifactRef, TraceChildExit, TraceEnvelope, TraceParserResultSummary,
    TraceRedactionStatus, TraceTiming, TraceVersionSummary, WrapperPaths, write_trace_at,
};
use serde::{Deserialize, Serialize};
use std::ffi::OsString;
use std::fs;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::{Path, PathBuf};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

pub(crate) const FUZZ_SMOKE_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum FuzzCaseStatus {
    Pass,
    Fail,
    Crash,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum FuzzSmokeStatus {
    Pass,
    Fail,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct FuzzCaseResult {
    pub(crate) id: String,
    pub(crate) description: String,
    pub(crate) kind: String,
    pub(crate) status: FuzzCaseStatus,
    pub(crate) duration_ms: u64,
    pub(crate) budget_ms: u64,
    pub(crate) budget_exceeded: bool,
    pub(crate) summary: String,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct FuzzSmokeReport {
    pub(crate) schema_version: u32,
    pub(crate) generated_at_unix_seconds: u64,
    pub(crate) root: PathBuf,
    pub(crate) overall_status: FuzzSmokeStatus,
    pub(crate) case_count: usize,
    pub(crate) passed_case_count: usize,
    pub(crate) failed_case_count: usize,
    pub(crate) crash_count: usize,
    pub(crate) budget_violation_count: usize,
    pub(crate) corpus_replay_passed: bool,
    pub(crate) cases: Vec<FuzzCaseResult>,
}

#[derive(Debug, Clone, Deserialize)]
struct FuzzCaseFile {
    schema_version: u32,
    id: String,
    description: String,
    time_budget_ms: u64,
    #[serde(skip)]
    root_dir: PathBuf,
    #[serde(flatten)]
    kind: FuzzCaseKind,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum FuzzCaseKind {
    SarifIngest {
        sarif_asset: String,
        #[serde(default)]
        stderr_asset: Option<String>,
        #[serde(default)]
        expect_fallback_reason: Option<FallbackReason>,
        #[serde(default)]
        expect_document_completeness: Option<DocumentCompleteness>,
        #[serde(default)]
        expect_min_diagnostic_count: Option<usize>,
    },
    ResidualClassify {
        stderr_asset: String,
        include_passthrough: bool,
        expect_min_nodes: usize,
        #[serde(default)]
        expect_family_prefixes: Vec<String>,
    },
    CoreSynthetic {
        scenario: CoreSyntheticScenario,
        expect_validation: ValidationExpectation,
    },
    RenderSynthetic {
        scenario: RenderSyntheticScenario,
        profile: RenderProfile,
        #[serde(default)]
        repeat_count: Option<usize>,
        #[serde(default)]
        depth: Option<usize>,
        expect_used_fallback: bool,
        #[serde(default)]
        required_substrings: Vec<String>,
        #[serde(default)]
        forbidden_substrings: Vec<String>,
    },
    TraceSynthetic {
        scenario: TraceSyntheticScenario,
        #[serde(default)]
        forbidden_substrings: Vec<String>,
    },
    CaptureRuntime {
        stderr_asset: String,
        #[serde(default)]
        sarif_asset: Option<String>,
        mode: ExecutionMode,
        inject_sarif: bool,
        capture_passthrough_stderr: bool,
        retain_trace: bool,
        #[serde(default)]
        expect_artifact_ids: Vec<String>,
    },
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
enum ValidationExpectation {
    Valid,
    Invalid,
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
enum CoreSyntheticScenario {
    DuplicateNodeIds,
    InlineCaptureMissingText,
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
enum RenderSyntheticScenario {
    RepeatedNotes,
    TemplateExplosion,
    PartialEscape,
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
enum TraceSyntheticScenario {
    EscapeEnvelope,
}

pub(crate) fn run_fuzz_smoke(
    root: &Path,
    report_dir: Option<&Path>,
) -> Result<FuzzSmokeReport, Box<dyn std::error::Error>> {
    let cases = discover_cases(root)?;
    if cases.is_empty() {
        return Err(format!("no fuzz cases found under {}", root.display()).into());
    }

    let mut results = Vec::new();
    for case in &cases {
        let started = Instant::now();
        let outcome = catch_unwind(AssertUnwindSafe(|| execute_case(case)));
        let duration_ms = started.elapsed().as_millis() as u64;
        let budget_exceeded = duration_ms > case.time_budget_ms;
        let result = match outcome {
            Ok(Ok(summary)) => FuzzCaseResult {
                id: case.id.clone(),
                description: case.description.clone(),
                kind: case.kind_name().to_string(),
                status: if budget_exceeded {
                    FuzzCaseStatus::Fail
                } else {
                    FuzzCaseStatus::Pass
                },
                duration_ms,
                budget_ms: case.time_budget_ms,
                budget_exceeded,
                summary: if budget_exceeded {
                    format!("{summary}; exceeded budget ({} ms)", case.time_budget_ms)
                } else {
                    summary
                },
            },
            Ok(Err(summary)) => FuzzCaseResult {
                id: case.id.clone(),
                description: case.description.clone(),
                kind: case.kind_name().to_string(),
                status: FuzzCaseStatus::Fail,
                duration_ms,
                budget_ms: case.time_budget_ms,
                budget_exceeded,
                summary: if budget_exceeded {
                    format!("{summary}; exceeded budget ({} ms)", case.time_budget_ms)
                } else {
                    summary
                },
            },
            Err(_) => FuzzCaseResult {
                id: case.id.clone(),
                description: case.description.clone(),
                kind: case.kind_name().to_string(),
                status: FuzzCaseStatus::Crash,
                duration_ms,
                budget_ms: case.time_budget_ms,
                budget_exceeded,
                summary: "panic while exercising fuzz seed".to_string(),
            },
        };
        results.push(result);
    }

    let report = FuzzSmokeReport {
        schema_version: FUZZ_SMOKE_SCHEMA_VERSION,
        generated_at_unix_seconds: unix_now_seconds(),
        root: root.to_path_buf(),
        overall_status: if results
            .iter()
            .all(|case| case.status == FuzzCaseStatus::Pass)
        {
            FuzzSmokeStatus::Pass
        } else {
            FuzzSmokeStatus::Fail
        },
        case_count: results.len(),
        passed_case_count: results
            .iter()
            .filter(|case| case.status == FuzzCaseStatus::Pass)
            .count(),
        failed_case_count: results
            .iter()
            .filter(|case| case.status == FuzzCaseStatus::Fail)
            .count(),
        crash_count: results
            .iter()
            .filter(|case| case.status == FuzzCaseStatus::Crash)
            .count(),
        budget_violation_count: results.iter().filter(|case| case.budget_exceeded).count(),
        corpus_replay_passed: results
            .iter()
            .all(|case| case.status == FuzzCaseStatus::Pass),
        cases: results,
    };

    if let Some(report_dir) = report_dir {
        fs::create_dir_all(report_dir)?;
        write_json(&report_dir.join("fuzz-smoke-report.json"), &report)?;
    }

    Ok(report)
}

fn discover_cases(root: &Path) -> Result<Vec<FuzzCaseFile>, Box<dyn std::error::Error>> {
    let mut paths = Vec::new();
    walk_case_files(&root.join("cases"), &mut paths)?;
    paths.sort();

    let mut cases = Vec::new();
    for path in paths {
        let case: FuzzCaseFile = serde_json::from_slice(&fs::read(&path)?)?;
        if case.schema_version != FUZZ_SMOKE_SCHEMA_VERSION {
            return Err(format!(
                "unsupported fuzz case schema version {} at {}",
                case.schema_version,
                path.display()
            )
            .into());
        }
        cases.push(FuzzCaseFile {
            root_dir: path
                .parent()
                .map(Path::to_path_buf)
                .unwrap_or_else(|| root.to_path_buf()),
            ..case
        });
    }
    Ok(cases)
}

fn walk_case_files(
    root: &Path,
    paths: &mut Vec<PathBuf>,
) -> Result<(), Box<dyn std::error::Error>> {
    if !root.exists() {
        return Ok(());
    }
    for entry in fs::read_dir(root)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            walk_case_files(&path, paths)?;
        } else if path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name == "case.json")
        {
            paths.push(path);
        }
    }
    Ok(())
}

fn execute_case(case: &FuzzCaseFile) -> Result<String, String> {
    match &case.kind {
        FuzzCaseKind::SarifIngest {
            sarif_asset,
            stderr_asset,
            expect_fallback_reason,
            expect_document_completeness,
            expect_min_diagnostic_count,
        } => run_sarif_ingest_case(
            case,
            sarif_asset,
            stderr_asset.as_deref(),
            *expect_fallback_reason,
            expect_document_completeness.clone(),
            *expect_min_diagnostic_count,
        ),
        FuzzCaseKind::ResidualClassify {
            stderr_asset,
            include_passthrough,
            expect_min_nodes,
            expect_family_prefixes,
        } => run_residual_case(
            case,
            stderr_asset,
            *include_passthrough,
            *expect_min_nodes,
            expect_family_prefixes,
        ),
        FuzzCaseKind::CoreSynthetic {
            scenario,
            expect_validation,
        } => run_core_synthetic_case(*scenario, *expect_validation),
        FuzzCaseKind::RenderSynthetic {
            scenario,
            profile,
            repeat_count,
            depth,
            expect_used_fallback,
            required_substrings,
            forbidden_substrings,
        } => run_render_synthetic_case(
            *scenario,
            *profile,
            repeat_count.unwrap_or(0),
            depth.unwrap_or(0),
            *expect_used_fallback,
            required_substrings,
            forbidden_substrings,
        ),
        FuzzCaseKind::TraceSynthetic {
            scenario,
            forbidden_substrings,
        } => run_trace_synthetic_case(*scenario, forbidden_substrings),
        FuzzCaseKind::CaptureRuntime {
            stderr_asset,
            sarif_asset,
            mode,
            inject_sarif,
            capture_passthrough_stderr,
            retain_trace,
            expect_artifact_ids,
        } => run_capture_runtime_case(
            case,
            stderr_asset,
            sarif_asset.as_deref(),
            *mode,
            *inject_sarif,
            *capture_passthrough_stderr,
            *retain_trace,
            expect_artifact_ids,
        ),
    }
}

fn run_sarif_ingest_case(
    case: &FuzzCaseFile,
    sarif_asset: &str,
    stderr_asset: Option<&str>,
    expect_fallback_reason: Option<FallbackReason>,
    expect_document_completeness: Option<DocumentCompleteness>,
    expect_min_diagnostic_count: Option<usize>,
) -> Result<String, String> {
    let case_dir = case.root_dir.clone();
    let sarif_path = case_dir.join(sarif_asset);
    let stderr_text = stderr_asset
        .map(|asset| read_text_asset(&case_dir.join(asset)))
        .transpose()?
        .unwrap_or_default();

    let outcome = ingest_with_reason(
        Some(&sarif_path),
        &stderr_text,
        producer_for_version("fuzz-smoke"),
        synthetic_run("sarif_ingest"),
    )
    .map_err(|error| format!("ingest failed: {error}"))?;

    if let Some(expected) = expect_fallback_reason {
        if outcome.fallback_reason != Some(expected) {
            return Err(format!(
                "expected fallback_reason={expected}, got {:?}",
                outcome.fallback_reason
            ));
        }
    }
    if let Some(expected) = expect_document_completeness {
        if outcome.document.document_completeness != expected {
            return Err(format!(
                "expected completeness {:?}, got {:?}",
                expected, outcome.document.document_completeness
            ));
        }
    }
    if let Some(minimum) = expect_min_diagnostic_count {
        if outcome.document.diagnostics.len() < minimum {
            return Err(format!(
                "expected at least {minimum} diagnostics, got {}",
                outcome.document.diagnostics.len()
            ));
        }
    }
    outcome.document.validate().map_err(validation_summary)?;

    Ok(format!(
        "fallback_reason={:?} diagnostics={}",
        outcome.fallback_reason,
        outcome.document.diagnostics.len()
    ))
}

fn run_residual_case(
    case: &FuzzCaseFile,
    stderr_asset: &str,
    include_passthrough: bool,
    expect_min_nodes: usize,
    expect_family_prefixes: &[String],
) -> Result<String, String> {
    let stderr_text = read_text_asset(&case.root_dir.join(stderr_asset))?;
    let nodes = diag_residual_text::classify(&stderr_text, include_passthrough);
    if nodes.len() < expect_min_nodes {
        return Err(format!(
            "expected at least {expect_min_nodes} residual nodes, got {}",
            nodes.len()
        ));
    }
    for family_prefix in expect_family_prefixes {
        let matched = nodes.iter().any(|node| {
            node.analysis
                .as_ref()
                .and_then(|analysis| analysis.family.as_deref())
                .map(|family| family.starts_with(family_prefix))
                .unwrap_or(false)
        });
        if !matched {
            return Err(format!("missing residual family prefix {family_prefix}"));
        }
    }
    Ok(format!("classified {} residual node(s)", nodes.len()))
}

fn run_core_synthetic_case(
    scenario: CoreSyntheticScenario,
    expect_validation: ValidationExpectation,
) -> Result<String, String> {
    let document = match scenario {
        CoreSyntheticScenario::DuplicateNodeIds => duplicate_node_ids_document(),
        CoreSyntheticScenario::InlineCaptureMissingText => inline_capture_missing_text_document(),
    };

    let validation = document.validate();
    match expect_validation {
        ValidationExpectation::Valid if validation.is_err() => {
            Err(validation_summary(validation.err().unwrap()))
        }
        ValidationExpectation::Invalid if validation.is_ok() => {
            Err("expected validation failure, but document validated".to_string())
        }
        _ => {
            let _ = document
                .canonical_json()
                .map_err(|error| format!("canonical_json failed: {error}"))?;
            Ok(match expect_validation {
                ValidationExpectation::Valid => "document validated".to_string(),
                ValidationExpectation::Invalid => "invalid document rejected cleanly".to_string(),
            })
        }
    }
}

fn run_render_synthetic_case(
    scenario: RenderSyntheticScenario,
    profile: RenderProfile,
    repeat_count: usize,
    depth: usize,
    expect_used_fallback: bool,
    required_substrings: &[String],
    forbidden_substrings: &[String],
) -> Result<String, String> {
    let document = match scenario {
        RenderSyntheticScenario::RepeatedNotes => repeated_notes_document(repeat_count.max(64)),
        RenderSyntheticScenario::TemplateExplosion => {
            template_explosion_document(depth.max(64), repeat_count.max(32))
        }
        RenderSyntheticScenario::PartialEscape => partial_escape_document(repeat_count.max(16)),
    };

    let request = synthetic_render_request(document.clone(), profile);
    let view_model = build_view_model(&request);
    let output = render(request).map_err(|error| format!("render failed: {error}"))?;

    if expect_used_fallback != output.used_fallback {
        return Err(format!(
            "expected used_fallback={}, got {}",
            expect_used_fallback, output.used_fallback
        ));
    }
    if !output.used_fallback && view_model.is_none() {
        return Err("expected render view model, got none".to_string());
    }
    if output.text.contains('\u{001b}') {
        return Err("render output still contained a raw ESC byte".to_string());
    }
    for needle in required_substrings {
        if !output.text.contains(needle) {
            return Err(format!(
                "render output missing required substring: {needle}"
            ));
        }
    }
    for needle in forbidden_substrings {
        if output.text.contains(needle) {
            return Err(format!(
                "render output contained forbidden substring: {needle}"
            ));
        }
    }

    Ok(format!(
        "used_fallback={} lines={}",
        output.used_fallback,
        output.text.lines().count()
    ))
}

fn run_trace_synthetic_case(
    scenario: TraceSyntheticScenario,
    forbidden_substrings: &[String],
) -> Result<String, String> {
    let trace = match scenario {
        TraceSyntheticScenario::EscapeEnvelope => trace_escape_envelope(),
    };

    let temp = tempfile::tempdir().map_err(|error| error.to_string())?;
    let path = temp.path().join("nested/trace.json");
    write_trace_at(&path, &trace).map_err(|error| format!("write_trace_at failed: {error}"))?;
    let payload = fs::read_to_string(&path).map_err(|error| error.to_string())?;
    let round_trip: TraceEnvelope =
        serde_json::from_str(&payload).map_err(|error| format!("trace reparse failed: {error}"))?;
    if round_trip.selected_mode != trace.selected_mode {
        return Err("trace round-trip lost selected_mode".to_string());
    }
    for needle in forbidden_substrings {
        if payload.contains(needle) {
            return Err(format!(
                "trace payload contained forbidden substring: {needle}"
            ));
        }
    }
    Ok(format!("trace_bytes={}", payload.len()))
}

fn run_capture_runtime_case(
    case: &FuzzCaseFile,
    stderr_asset: &str,
    sarif_asset: Option<&str>,
    mode: ExecutionMode,
    inject_sarif: bool,
    capture_passthrough_stderr: bool,
    retain_trace: bool,
    expect_artifact_ids: &[String],
) -> Result<String, String> {
    let temp = tempfile::tempdir().map_err(|error| error.to_string())?;
    let case_dir = case.root_dir.clone();
    let stderr_path = case_dir.join(stderr_asset);
    let stderr_text = read_text_asset(&stderr_path)?;
    let sarif_payload_path = sarif_asset.map(|asset| case_dir.join(asset));

    let backend = temp.path().join("fake-gcc");
    fs::write(&backend, fake_backend_script()).map_err(|error| error.to_string())?;
    make_executable(&backend)?;

    let probe = probe_backend(&backend, "gcc-formed".to_string())
        .map_err(|error| format!("probe_backend failed: {error}"))?;

    let cwd = temp.path().join("cwd,with=spaces");
    fs::create_dir_all(&cwd).map_err(|error| error.to_string())?;
    let runtime_root = temp.path().join("runtime,root=unsafe path");
    let trace_root = temp.path().join("trace-root");
    let paths = WrapperPaths {
        config_path: temp.path().join("config.toml"),
        cache_root: temp.path().join("cache-root"),
        state_root: temp.path().join("state-root"),
        runtime_root,
        trace_root,
        install_root: temp.path().join("install-root"),
    };

    let request = CaptureRequest {
        backend: probe,
        args: vec![OsString::from("-c"), OsString::from("src/main.c")],
        cwd,
        mode,
        capture_passthrough_stderr,
        retention: if retain_trace {
            RetentionPolicy::Always
        } else {
            RetentionPolicy::Never
        },
        paths,
        inject_sarif,
    };

    let output = run_capture_with_env(&request, &stderr_path, sarif_payload_path.as_deref())?;

    if output.exit_status.code != Some(1) {
        return Err(format!(
            "expected exit code 1, got {:?}",
            output.exit_status.code
        ));
    }
    if retain_trace && !output.retained {
        return Err("expected retained trace bundle".to_string());
    }
    for artifact_id in expect_artifact_ids {
        if !output
            .artifacts
            .iter()
            .any(|artifact| artifact.id == *artifact_id)
        {
            return Err(format!("missing artifact id {artifact_id}"));
        }
    }
    let invocation_path = if let Some(trace_dir) = output.retained_trace_dir.as_ref() {
        trace_dir.join("invocation.json")
    } else {
        output.temp_dir.join("invocation.json")
    };
    let invocation = fs::read_to_string(&invocation_path).map_err(|error| error.to_string())?;
    let invocation_value = serde_json::from_str::<serde_json::Value>(&invocation)
        .map_err(|error| format!("invalid invocation record: {error}"))?;
    let injected_arg = invocation_value["argv"]
        .as_array()
        .and_then(|argv| {
            argv.iter().find_map(|value| {
                value.as_str().and_then(|arg| {
                    arg.strip_prefix("-fdiagnostics-add-output=sarif:version=2.1,file=")
                })
            })
        })
        .ok_or_else(|| "missing injected sarif flag".to_string())?;
    if injected_arg.contains(',') || injected_arg.contains(' ') || injected_arg.contains('=') {
        return Err(format!(
            "injected sarif path was not sanitized: {injected_arg}"
        ));
    }
    cleanup_capture(&output).map_err(|error| format!("cleanup_capture failed: {error}"))?;

    Ok(format!(
        "retained={} stderr_bytes={}",
        output.retained,
        stderr_text.len()
    ))
}

fn run_capture_with_env(
    request: &CaptureRequest,
    stderr_payload_path: &Path,
    sarif_payload_path: Option<&Path>,
) -> Result<diag_capture_runtime::CaptureOutcome, String> {
    let temp = tempfile::tempdir().map_err(|error| error.to_string())?;
    let backend = request.backend.resolved_path.clone();
    let wrapper = temp.path().join("backend-wrapper");
    let script = format!(
        "#!/usr/bin/env bash\nset -euo pipefail\nexport STDERR_PAYLOAD_PATH='{}'\n{}\nexec '{}' \"$@\"\n",
        stderr_payload_path.display(),
        sarif_payload_path
            .map(|path| format!("export SARIF_PAYLOAD_PATH='{}'\n", path.display()))
            .unwrap_or_default(),
        backend.display()
    );
    fs::write(&wrapper, script).map_err(|error| error.to_string())?;
    make_executable(&wrapper)?;

    let mut request = request.clone();
    request.backend = probe_backend(&wrapper, "gcc-formed".to_string())
        .map_err(|error| format!("wrapper probe failed: {error}"))?;
    run_capture(&request).map_err(|error| format!("run_capture failed: {error}"))
}

fn read_text_asset(path: &Path) -> Result<String, String> {
    fs::read_to_string(path).map_err(|error| format!("failed to read {}: {error}", path.display()))
}

fn synthetic_run(name: &str) -> RunInfo {
    RunInfo {
        invocation_id: format!("fuzz-{name}"),
        invoked_as: Some("gcc-formed".to_string()),
        argv_redacted: vec![
            "gcc".to_string(),
            "-c".to_string(),
            "src/main.c".to_string(),
        ],
        cwd_display: Some("/workspace".to_string()),
        exit_status: 1,
        primary_tool: tool_for_backend("gcc", Some("15.2.0".to_string())),
        secondary_tools: Vec::new(),
        language_mode: Some(LanguageMode::C),
        target_triple: Some("x86_64-unknown-linux-gnu".to_string()),
        wrapper_mode: Some(diag_core::WrapperSurface::Terminal),
    }
}

fn synthetic_document(
    root: DiagnosticNode,
    completeness: DocumentCompleteness,
) -> DiagnosticDocument {
    let raw_capture = root.message.raw_text.clone();
    let mut document = DiagnosticDocument {
        document_id: "fuzz-doc".to_string(),
        schema_version: diag_core::IR_SPEC_VERSION.to_string(),
        document_completeness: completeness,
        producer: ProducerInfo {
            name: "gcc-formed".to_string(),
            version: "fuzz-smoke".to_string(),
            git_revision: None,
            build_profile: None,
            rulepack_version: Some("phase1".to_string()),
        },
        run: synthetic_run("render"),
        captures: vec![CaptureArtifact {
            id: "stderr.raw".to_string(),
            kind: ArtifactKind::CompilerStderrText,
            media_type: "text/plain".to_string(),
            encoding: Some("utf-8".to_string()),
            digest_sha256: None,
            size_bytes: Some(raw_capture.len() as u64),
            storage: ArtifactStorage::Inline,
            inline_text: Some(raw_capture),
            external_ref: None,
            produced_by: Some(ToolInfo {
                name: "gcc".to_string(),
                version: Some("15.2.0".to_string()),
                component: None,
                vendor: Some("GNU".to_string()),
            }),
        }],
        integrity_issues: Vec::new(),
        diagnostics: vec![root],
        fingerprints: None,
    };
    document.refresh_fingerprints();
    document
}

fn root_node(
    family: &str,
    title: &str,
    raw_message: &str,
    severity: Severity,
    phase: Phase,
    completeness: NodeCompleteness,
) -> DiagnosticNode {
    DiagnosticNode {
        id: "root".to_string(),
        origin: Origin::Gcc,
        phase,
        severity,
        semantic_role: SemanticRole::Root,
        message: MessageText {
            raw_text: raw_message.to_string(),
            normalized_text: None,
            locale: None,
        },
        locations: vec![Location {
            path: "src/main.cpp".to_string(),
            line: 7,
            column: 22,
            end_line: None,
            end_column: None,
            display_path: None,
            ownership: Some(Ownership::User),
        }],
        children: Vec::new(),
        suggestions: Vec::new(),
        context_chains: Vec::new(),
        symbol_context: None,
        node_completeness: completeness,
        provenance: Provenance {
            source: ProvenanceSource::Compiler,
            capture_refs: vec!["stderr.raw".to_string()],
        },
        analysis: Some(AnalysisOverlay {
            family: Some(family.to_string()),
            headline: Some(title.to_string()),
            first_action_hint: Some(match family {
                "template" => {
                    "start from the first user-owned template frame and match template arguments"
                        .to_string()
                }
                "linker" => {
                    "inspect the preserved compiler stderr and the first failing link input"
                        .to_string()
                }
                _ => "fix the first parser error before reading follow-up notes".to_string(),
            }),
            confidence: Some(Confidence::High),
            rule_id: Some(format!("rule.family.{family}.synthetic")),
            matched_conditions: vec!["synthetic_fuzz_case=true".to_string()],
            suppression_reason: None,
            collapsed_child_ids: Vec::new(),
            collapsed_chain_ids: Vec::new(),
        }),
        fingerprints: None,
    }
}

fn repeated_notes_document(repeat_count: usize) -> DiagnosticDocument {
    let mut root = root_node(
        "syntax",
        "syntax error",
        "expected ';' before '}' token",
        Severity::Error,
        Phase::Parse,
        NodeCompleteness::Complete,
    );
    root.children = (0..repeat_count)
        .map(|index| DiagnosticNode {
            id: format!("note-{index}"),
            origin: Origin::Gcc,
            phase: Phase::Parse,
            severity: Severity::Note,
            semantic_role: SemanticRole::Supporting,
            message: MessageText {
                raw_text: format!("follow-up note {index}: consider nearby token"),
                normalized_text: None,
                locale: None,
            },
            locations: Vec::new(),
            children: Vec::new(),
            suggestions: Vec::new(),
            context_chains: Vec::new(),
            symbol_context: None,
            node_completeness: NodeCompleteness::Partial,
            provenance: Provenance {
                source: ProvenanceSource::Compiler,
                capture_refs: vec!["stderr.raw".to_string()],
            },
            analysis: None,
            fingerprints: None,
        })
        .collect();
    synthetic_document(root, DocumentCompleteness::Complete)
}

fn template_explosion_document(depth: usize, repeat_count: usize) -> DiagnosticDocument {
    let mut root = root_node(
        "template",
        "template instantiation failed",
        "class template argument deduction failed:",
        Severity::Error,
        Phase::Instantiate,
        NodeCompleteness::Complete,
    );
    root.context_chains = vec![ContextChain {
        kind: ContextChainKind::TemplateInstantiation,
        frames: (0..depth)
            .map(|index| ContextFrame {
                label: format!(
                    "instantiated from FancyTemplate<{index}, NestedType<{index}, Value<{index}>>>"
                ),
                path: Some(format!("include/t{index}.hpp")),
                line: Some((index + 1) as u32),
                column: Some(1),
            })
            .collect(),
    }];
    root.children = (0..repeat_count)
        .map(|index| DiagnosticNode {
            id: format!("template-note-{index}"),
            origin: Origin::Gcc,
            phase: Phase::Instantiate,
            severity: Severity::Note,
            semantic_role: if index % 2 == 0 {
                SemanticRole::Candidate
            } else {
                SemanticRole::Supporting
            },
            message: MessageText {
                raw_text: format!(
                    "candidate {index}: 'template<class T{index}> void accept(T{index})'"
                ),
                normalized_text: None,
                locale: None,
            },
            locations: Vec::new(),
            children: Vec::new(),
            suggestions: Vec::new(),
            context_chains: Vec::new(),
            symbol_context: None,
            node_completeness: NodeCompleteness::Partial,
            provenance: Provenance {
                source: ProvenanceSource::Compiler,
                capture_refs: vec!["stderr.raw".to_string()],
            },
            analysis: Some(AnalysisOverlay {
                family: Some("template".to_string()),
                headline: Some("template instantiation failed".to_string()),
                first_action_hint: None,
                confidence: Some(Confidence::Medium),
                rule_id: Some("rule.family.template.synthetic_child".to_string()),
                matched_conditions: vec!["synthetic_fuzz_case=true".to_string()],
                suppression_reason: None,
                collapsed_child_ids: Vec::new(),
                collapsed_chain_ids: Vec::new(),
            }),
            fingerprints: None,
        })
        .collect();
    synthetic_document(root, DocumentCompleteness::Complete)
}

fn partial_escape_document(repeat_count: usize) -> DiagnosticDocument {
    let mut root = root_node(
        "linker",
        "linker file format or relocation failure",
        "\u{001b}[31mld: file format not recognized for /tmp/cc12345.o",
        Severity::Error,
        Phase::Link,
        NodeCompleteness::Partial,
    );
    root.children = (0..repeat_count)
        .map(|index| DiagnosticNode {
            id: format!("raw-note-{index}"),
            origin: Origin::Linker,
            phase: Phase::Link,
            severity: Severity::Note,
            semantic_role: SemanticRole::Supporting,
            message: MessageText {
                raw_text: format!(
                    "helper note {index}: \u{001b}[0mbroken archive member /tmp/cc{index:05}.o"
                ),
                normalized_text: None,
                locale: None,
            },
            locations: Vec::new(),
            children: Vec::new(),
            suggestions: Vec::new(),
            context_chains: Vec::new(),
            symbol_context: None,
            node_completeness: NodeCompleteness::Partial,
            provenance: Provenance {
                source: ProvenanceSource::ResidualText,
                capture_refs: vec!["stderr.raw".to_string()],
            },
            analysis: None,
            fingerprints: None,
        })
        .collect();
    synthetic_document(root, DocumentCompleteness::Partial)
}

fn duplicate_node_ids_document() -> DiagnosticDocument {
    let mut document = repeated_notes_document(2);
    if let Some(child) = document.diagnostics[0].children.get_mut(0) {
        child.id = "root".to_string();
    }
    document
}

fn inline_capture_missing_text_document() -> DiagnosticDocument {
    let mut document = repeated_notes_document(1);
    document.captures[0].inline_text = None;
    document
}

fn synthetic_render_request(document: DiagnosticDocument, profile: RenderProfile) -> RenderRequest {
    RenderRequest {
        document,
        profile,
        capabilities: RenderCapabilities {
            stream_kind: StreamKind::Pipe,
            width_columns: Some(100),
            ansi_color: false,
            unicode: false,
            hyperlinks: false,
            interactive: false,
        },
        cwd: Some(PathBuf::from("/workspace")),
        path_policy: PathPolicy::RelativeToCwd,
        warning_visibility: WarningVisibility::Auto,
        debug_refs: DebugRefs::None,
        type_display_policy: TypeDisplayPolicy::CompactSafe,
        source_excerpt_policy: SourceExcerptPolicy::Auto,
    }
}

fn trace_escape_envelope() -> TraceEnvelope {
    TraceEnvelope {
        trace_id: "trace-\u{001b}[31mseed".to_string(),
        selected_mode: "render".to_string(),
        selected_profile: "default".to_string(),
        support_tier: "gcc15_primary".to_string(),
        wrapper_verdict: Some("rendered".to_string()),
        version_summary: Some(TraceVersionSummary {
            wrapper_version: "0.2.0-beta.1".to_string(),
            build_target_triple: "x86_64-unknown-linux-musl".to_string(),
            ir_spec_version: diag_core::IR_SPEC_VERSION.to_string(),
            adapter_spec_version: diag_core::ADAPTER_SPEC_VERSION.to_string(),
            renderer_spec_version: diag_core::RENDERER_SPEC_VERSION.to_string(),
        }),
        environment_summary: None,
        capabilities: None,
        timing: Some(TraceTiming {
            capture_ms: 12,
            render_ms: Some(8),
            total_ms: 20,
        }),
        child_exit: Some(TraceChildExit {
            code: Some(1),
            signal: None,
            success: false,
        }),
        parser_result_summary: Some(TraceParserResultSummary {
            status: "parsed".to_string(),
            document_completeness: Some("partial".to_string()),
            diagnostic_count: 2,
            integrity_issue_count: 0,
            capture_count: 1,
        }),
        fingerprint_summary: None,
        redaction_status: Some(TraceRedactionStatus {
            class: "restricted".to_string(),
            local_only: true,
            normalized_artifacts: vec!["ir.analysis.json".to_string()],
        }),
        decision_log: vec!["rendered escape-heavy trace".to_string()],
        fallback_reason: None,
        warning_messages: vec!["saw raw escape \u{001b}[0m in child stderr".to_string()],
        artifacts: vec![TraceArtifactRef {
            id: "stderr.raw".to_string(),
            path: Some(PathBuf::from("/tmp/cc12345.o")),
        }],
    }
}

fn fake_backend_script() -> &'static str {
    r#"#!/usr/bin/env bash
set -euo pipefail
if [[ "${1:-}" == "--version" ]]; then
  echo "gcc (Fake) 15.2.0"
  exit 0
fi
sarif=""
for arg in "$@"; do
  if [[ "$arg" == -fdiagnostics-add-output=sarif:version=2.1,file=* ]]; then
    sarif="${arg#-fdiagnostics-add-output=sarif:version=2.1,file=}"
  fi
done
if [[ -n "$sarif" && -n "${SARIF_PAYLOAD_PATH:-}" ]]; then
  cat "$SARIF_PAYLOAD_PATH" > "$sarif"
fi
cat "$STDERR_PAYLOAD_PATH" >&2
exit 1
"#
}

#[cfg(unix)]
fn make_executable(path: &Path) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;

    let mut permissions = fs::metadata(path)
        .map_err(|error| error.to_string())?
        .permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(path, permissions).map_err(|error| error.to_string())
}

#[cfg(not(unix))]
fn make_executable(_path: &Path) -> Result<(), String> {
    Ok(())
}

fn validation_summary(errors: ValidationErrors) -> String {
    errors.errors.join("; ")
}

fn write_json<T: Serialize>(path: &Path, value: &T) -> Result<(), Box<dyn std::error::Error>> {
    fs::write(path, serde_json::to_vec_pretty(value)?)?;
    Ok(())
}

fn unix_now_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or_default()
}

impl FuzzCaseFile {
    fn kind_name(&self) -> &'static str {
        match &self.kind {
            FuzzCaseKind::SarifIngest { .. } => "sarif_ingest",
            FuzzCaseKind::ResidualClassify { .. } => "residual_classify",
            FuzzCaseKind::CoreSynthetic { .. } => "core_synthetic",
            FuzzCaseKind::RenderSynthetic { .. } => "render_synthetic",
            FuzzCaseKind::TraceSynthetic { .. } => "trace_synthetic",
            FuzzCaseKind::CaptureRuntime { .. } => "capture_runtime",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn fuzz_smoke_runs_single_synthetic_case() {
        let temp = tempfile::tempdir().unwrap();
        let case_dir = temp.path().join("cases/render-case");
        fs::create_dir_all(&case_dir).unwrap();
        fs::write(
            case_dir.join("case.json"),
            serde_json::to_vec_pretty(&json!({
                "schema_version": FUZZ_SMOKE_SCHEMA_VERSION,
                "id": "render-case",
                "description": "partial escape render stays terminal-safe",
                "time_budget_ms": 1000,
                "kind": "render_synthetic",
                "scenario": "partial_escape",
                "profile": "default",
                "repeat_count": 8,
                "expect_used_fallback": false,
                "required_substrings": ["\\x1b[31m"],
                "forbidden_substrings": ["\u{001b}["]
            }))
            .unwrap(),
        )
        .unwrap();

        let report = run_fuzz_smoke(temp.path(), None).unwrap();

        assert_eq!(report.case_count, 1);
        assert_eq!(report.overall_status, FuzzSmokeStatus::Pass);
        assert_eq!(report.crash_count, 0);
    }

    #[test]
    fn fuzz_smoke_surfaces_expectation_failures() {
        let temp = tempfile::tempdir().unwrap();
        let case_dir = temp.path().join("cases/ingest-case");
        fs::create_dir_all(&case_dir).unwrap();
        fs::write(case_dir.join("input.sarif"), b"{ invalid json").unwrap();
        fs::write(case_dir.join("stderr.txt"), b"compiler stderr").unwrap();
        fs::write(
            case_dir.join("case.json"),
            serde_json::to_vec_pretty(&json!({
                "schema_version": FUZZ_SMOKE_SCHEMA_VERSION,
                "id": "ingest-case",
                "description": "bad expectation should fail the smoke report",
                "time_budget_ms": 1000,
                "kind": "sarif_ingest",
                "sarif_asset": "input.sarif",
                "stderr_asset": "stderr.txt",
                "expect_fallback_reason": "sarif_missing"
            }))
            .unwrap(),
        )
        .unwrap();

        let report = run_fuzz_smoke(temp.path(), None).unwrap();

        assert_eq!(report.overall_status, FuzzSmokeStatus::Fail);
        assert_eq!(report.failed_case_count, 1);
        assert_eq!(report.crash_count, 0);
    }
}
