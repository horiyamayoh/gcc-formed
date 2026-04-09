use crate::args::{ParsedArgs, os_to_string};
use crate::backend::backend_binary_name;
use crate::mode::{ModeDecision, is_ci, language_mode_from_invocation};
use diag_adapter_gcc::tool_for_backend;
use diag_capture_runtime::{
    CaptureOutcome, ExecutionMode, ExitStatusInfo, trace_sanitized_env_keys,
};
use diag_core::{
    DiagnosticDocument, DocumentCompleteness, FallbackReason, SnapshotKind, WrapperSurface,
    snapshot_json,
};
use diag_render::{DebugRefs, RenderCapabilities, RenderProfile};
use diag_trace::{
    TraceArtifactRef, TraceCapabilities, TraceChildExit, TraceEnvelope, TraceEnvironmentSummary,
    TraceFingerprintSummary, TraceParserResultSummary, TraceRedactionStatus, TraceTiming,
    TraceVersionSummary, WrapperPaths, build_target_triple, secure_private_file, trace_id,
    write_trace, write_trace_at,
};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

pub(crate) fn maybe_write_trace(
    paths: &WrapperPaths,
    document: &DiagnosticDocument,
    capture: &CaptureOutcome,
    parsed: &ParsedArgs,
    backend: &diag_backend_probe::ProbeResult,
    mode_decision: &ModeDecision,
    profile: RenderProfile,
    capabilities: &RenderCapabilities,
    fallback_reason: Option<FallbackReason>,
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
        wrapper_verdict: Some(trace_wrapper_verdict(mode_decision.mode, fallback_reason)),
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
        parser_result_summary: Some(parsed_parser_result_summary(document, fallback_reason)),
        fingerprint_summary: trace_fingerprint_summary_from_document(document),
        redaction_status: Some(trace_redaction_status(
            mode_decision.mode,
            retained_trace_dir.is_some(),
        )),
        decision_log: mode_decision.decision_log.clone(),
        fallback_reason,
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

pub(crate) fn maybe_write_passthrough_trace(
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
        fallback_reason: mode_decision.fallback_reason,
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

pub(crate) fn argv_for_trace(parsed: &ParsedArgs) -> Vec<String> {
    parsed.forwarded_args.iter().map(os_to_string).collect()
}

pub(crate) fn wrapper_surface() -> WrapperSurface {
    if is_ci() {
        WrapperSurface::Ci
    } else {
        WrapperSurface::Terminal
    }
}

pub(crate) fn build_primary_tool(backend: &diag_backend_probe::ProbeResult) -> diag_core::ToolInfo {
    tool_for_backend(
        backend_binary_name(backend),
        Some(backend.version_string.clone()),
    )
}

pub(crate) fn build_language_mode(argv0: &str) -> diag_core::LanguageMode {
    language_mode_from_invocation(argv0)
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
        version_band: snake_case_label(&backend.version_band()),
        processing_path: snake_case_label(&resolved_processing_path(backend, mode)),
        support_level: snake_case_label(&backend.support_level()),
        injected_flags: trace_injected_flags(capture),
        sanitized_env_keys: trace_sanitized_env_keys(mode),
        temp_artifact_paths: trace_temp_artifact_paths(capture),
    }
}

fn resolved_processing_path(
    backend: &diag_backend_probe::ProbeResult,
    mode: ExecutionMode,
) -> diag_backend_probe::ProcessingPath {
    match mode {
        ExecutionMode::Passthrough => diag_backend_probe::ProcessingPath::Passthrough,
        _ if backend.capability_profile().dual_sink => {
            diag_backend_probe::ProcessingPath::DualSinkStructured
        }
        _ => diag_backend_probe::ProcessingPath::NativeTextCapture,
    }
}

fn snake_case_label<T: serde::Serialize>(value: &T) -> String {
    serde_json::to_value(value)
        .ok()
        .and_then(|value| value.as_str().map(|value| value.to_string()))
        .unwrap_or_else(|| "unknown".to_string())
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

fn trace_wrapper_verdict(mode: ExecutionMode, fallback_reason: Option<FallbackReason>) -> String {
    match mode {
        ExecutionMode::Render => {
            if fallback_reason.is_some() {
                "render_fallback".to_string()
            } else {
                "rendered".to_string()
            }
        }
        ExecutionMode::Shadow => "shadow_observed".to_string(),
        ExecutionMode::Passthrough => match fallback_reason {
            Some(FallbackReason::UserOptOut) => "passthrough_requested".to_string(),
            _ => "passthrough_fallback".to_string(),
        },
    }
}

fn parsed_parser_result_summary(
    document: &DiagnosticDocument,
    fallback_reason: Option<FallbackReason>,
) -> TraceParserResultSummary {
    TraceParserResultSummary {
        status: if matches!(
            fallback_reason,
            Some(FallbackReason::SarifMissing | FallbackReason::SarifParseFailed)
        ) {
            "fallback".to_string()
        } else {
            "parsed".to_string()
        },
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
