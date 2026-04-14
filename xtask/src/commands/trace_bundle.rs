use diag_adapter_gcc::{IngestPolicy, ingest_bundle, producer_for_version, tool_for_backend};
use diag_backend_probe::{ProcessingPath, SupportLevel, VersionBand};
use diag_capture_runtime::{
    CaptureBundle, CaptureInvocation, CapturePlan, ExecutionMode, ExitStatusInfo, LocaleHandling,
    NativeTextCapturePolicy, StructuredCapturePolicy,
};
use diag_cascade::{CascadeContext, DocumentAnalyzer, SafeDocumentAnalyzer};
use diag_core::{
    ArtifactStorage, CaptureArtifact, DiagnosticDocument, FallbackReason, RunInfo, SourceAuthority,
    WrapperSurface,
};
use diag_enrich::enrich_document;
use diag_public_export::{PublicDiagnosticExport, PublicExportContext, export_from_document};
use diag_render::{
    DebugRefs, PathPolicy, RenderCapabilities, RenderProfile, RenderRequest, SourceExcerptPolicy,
    StreamKind, TypeDisplayPolicy, WarningVisibility, render,
};
use diag_trace::{
    TRACE_BUNDLE_MANIFEST_FILE, TRACE_BUNDLE_PUBLIC_EXPORT_FILE, TRACE_BUNDLE_REPLAY_INPUT_FILE,
    TraceBundleManifest, TraceBundleReplayInput, TraceEnvelope, extract_trace_bundle_archive,
};
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct TraceBundleReplayReport {
    pub(crate) kind: String,
    pub(crate) schema_version: String,
    pub(crate) bundle_path: PathBuf,
    pub(crate) report_dir: PathBuf,
    pub(crate) render_path: PathBuf,
    pub(crate) public_export_path: PathBuf,
    pub(crate) provenance_summary_path: PathBuf,
    pub(crate) public_export_source: String,
    pub(crate) degradation_warnings: Vec<String>,
}

pub(crate) fn run_replay_trace_bundle(
    bundle_path: &Path,
    report_dir: &Path,
) -> Result<TraceBundleReplayReport, Box<dyn std::error::Error>> {
    fs::create_dir_all(report_dir)?;
    let extract_root = (!bundle_path.is_dir())
        .then(tempfile::tempdir)
        .transpose()?;
    if let Some(root) = extract_root.as_ref() {
        extract_trace_bundle_archive(bundle_path, root.path())
            .map_err(|error| format!("extract {}: {error}", bundle_path.display()))?;
    }
    let bundle_root = extract_root
        .as_ref()
        .map(|root| root.path())
        .unwrap_or(bundle_path);

    let manifest: TraceBundleManifest = read_json(&bundle_root.join(TRACE_BUNDLE_MANIFEST_FILE))?;
    let trace: TraceEnvelope = read_json(&bundle_root.join("trace.json"))?;
    let replay_input: TraceBundleReplayInput =
        read_json(&bundle_root.join(TRACE_BUNDLE_REPLAY_INPUT_FILE))?;
    let capture_bundle = replay_capture_bundle(bundle_root, &replay_input)?;

    let producer = producer_for_version(env!("CARGO_PKG_VERSION"));
    let run_info = RunInfo {
        invocation_id: trace.trace_id.clone(),
        invoked_as: Some("cargo xtask replay-trace-bundle".to_string()),
        argv_redacted: vec!["<trace_bundle_replay>".to_string()],
        cwd_display: None,
        exit_status: trace
            .child_exit
            .as_ref()
            .and_then(|exit| exit.code)
            .unwrap_or(1),
        primary_tool: tool_for_backend(
            &replay_input.backend_tool,
            replay_input.backend_version.clone(),
        ),
        secondary_tools: Vec::new(),
        language_mode: None,
        target_triple: None,
        wrapper_mode: Some(WrapperSurface::Terminal),
    };
    let ingest_report = ingest_bundle(
        &capture_bundle,
        IngestPolicy {
            producer,
            run: run_info,
        },
    )?;

    let mut document = ingest_report.document;
    document.captures = capture_bundle.capture_artifacts();
    let replay_cwd = Path::new("/bundle-root");
    enrich_document(&mut document, replay_cwd);
    let version_band: VersionBand = parse_label(&manifest.version_band)?;
    let processing_path: ProcessingPath = parse_label(&manifest.processing_path)?;
    let support_level: SupportLevel = parse_label(&manifest.support_level)?;
    let _ = run_cascade_analysis(
        &SafeDocumentAnalyzer,
        &mut document,
        &CascadeContext {
            version_band,
            processing_path,
            source_authority: ingest_report.source_authority,
            fallback_grade: ingest_report.fallback_grade,
            cwd: replay_cwd.to_path_buf(),
        },
        &diag_core::CascadePolicySnapshot::default(),
    );

    let degradation_warnings = replay_degradation_warnings(&document);
    let render_result = render(RenderRequest {
        document: document.clone(),
        cascade_policy: diag_core::CascadePolicySnapshot::default(),
        profile: RenderProfile::Default,
        capabilities: deterministic_render_capabilities(),
        cwd: None,
        path_policy: PathPolicy::ShortestUnambiguous,
        warning_visibility: WarningVisibility::Auto,
        debug_refs: DebugRefs::None,
        type_display_policy: TypeDisplayPolicy::CompactSafe,
        source_excerpt_policy: SourceExcerptPolicy::Auto,
    })?;
    let replay_text = replay_render_text(&degradation_warnings, &render_result.text);
    let render_path = report_dir.join("render.default.txt");
    fs::write(&render_path, replay_text.as_bytes())?;

    let public_export_path = report_dir.join(TRACE_BUNDLE_PUBLIC_EXPORT_FILE);
    let public_export_source = write_public_export_from_bundle_or_replay(
        bundle_root,
        &public_export_path,
        &document,
        &trace,
        version_band,
        processing_path,
        support_level,
        ingest_report.source_authority,
        ingest_report.fallback_grade,
        ingest_report.fallback_reason.or(trace.fallback_reason),
    )?;

    let provenance_summary_path = report_dir.join("provenance.summary.json");
    let provenance_summary = serde_json::to_vec_pretty(&serde_json::json!({
        "kind": "gcc_formed_trace_bundle_replay_summary",
        "schema_version": "1.0.0-alpha.1",
        "bundle_manifest": manifest,
        "trace_id": trace.trace_id,
        "wrapper_verdict": trace.wrapper_verdict,
        "fallback_reason": trace.fallback_reason,
        "public_export_source": public_export_source.clone(),
        "degradation_warnings": degradation_warnings.clone(),
        "render_path": render_path.clone(),
        "public_export_path": public_export_path.clone(),
    }))?;
    fs::write(&provenance_summary_path, provenance_summary)?;

    print!("{replay_text}");
    Ok(TraceBundleReplayReport {
        kind: "gcc_formed_trace_bundle_replay_report".to_string(),
        schema_version: "1.0.0-alpha.1".to_string(),
        bundle_path: bundle_path.to_path_buf(),
        report_dir: report_dir.to_path_buf(),
        render_path,
        public_export_path,
        provenance_summary_path,
        public_export_source,
        degradation_warnings,
    })
}

fn deterministic_render_capabilities() -> RenderCapabilities {
    RenderCapabilities {
        stream_kind: StreamKind::Pipe,
        width_columns: Some(100),
        ansi_color: false,
        unicode: false,
        hyperlinks: false,
        interactive: false,
    }
}

fn replay_capture_bundle(
    extract_root: &Path,
    input: &TraceBundleReplayInput,
) -> Result<CaptureBundle, Box<dyn std::error::Error>> {
    let execution_mode: ExecutionMode = parse_label(&input.execution_mode)?;
    let processing_path: ProcessingPath = parse_label(&input.processing_path)?;
    let structured_capture: StructuredCapturePolicy = parse_label(&input.structured_capture)?;
    let native_text_capture: NativeTextCapturePolicy = parse_label(&input.native_text_capture)?;
    let locale_handling: LocaleHandling = parse_label(&input.locale_handling)?;

    let raw_text_artifacts = input
        .raw_text_artifacts
        .iter()
        .map(|artifact| replay_raw_text_artifact(extract_root, artifact))
        .collect::<Result<Vec<_>, _>>()?;
    let structured_artifacts = input
        .structured_artifacts
        .iter()
        .map(|artifact| replay_structured_artifact(extract_root, artifact))
        .collect::<Result<Vec<_>, _>>()?;

    Ok(CaptureBundle {
        plan: CapturePlan {
            execution_mode,
            processing_path,
            structured_capture,
            native_text_capture,
            preserve_native_color: input.preserve_native_color,
            locale_handling,
            retention_policy: parse_label(&input.retention_policy)?,
        },
        invocation: CaptureInvocation {
            backend_path: input.backend_tool.clone(),
            launcher_path: input.launcher_tool.clone(),
            spawn_path: input.backend_tool.clone(),
            argv: Vec::new(),
            spawn_argv: Vec::new(),
            argv_hash: input.argv_hash.clone(),
            cwd: "<bundle>".to_string(),
            selected_mode: execution_mode,
            processing_path,
        },
        raw_text_artifacts,
        structured_artifacts,
        exit_status: ExitStatusInfo {
            code: input.child_exit.code,
            signal: input.child_exit.signal,
            success: input.child_exit.success,
        },
        integrity_issues: input.integrity_issues.clone(),
    })
}

fn replay_raw_text_artifact(
    extract_root: &Path,
    artifact: &diag_trace::TraceBundleReplayArtifact,
) -> Result<CaptureArtifact, Box<dyn std::error::Error>> {
    let (storage, inline_text, external_ref, size_bytes) = if artifact.available {
        let path = extract_root.join(
            artifact
                .file_name
                .as_deref()
                .ok_or("raw text replay artifact missing file_name")?,
        );
        let payload = fs::read(&path)?;
        (
            ArtifactStorage::Inline,
            Some(String::from_utf8_lossy(&payload).into_owned()),
            None,
            Some(payload.len() as u64),
        )
    } else {
        (
            ArtifactStorage::Unavailable,
            None,
            None,
            artifact.size_bytes,
        )
    };
    Ok(CaptureArtifact {
        id: artifact.id.clone(),
        kind: artifact.kind.clone(),
        media_type: artifact.media_type.clone(),
        encoding: artifact.encoding.clone(),
        digest_sha256: None,
        size_bytes,
        storage,
        inline_text,
        external_ref,
        produced_by: artifact.produced_by.clone(),
    })
}

fn replay_structured_artifact(
    extract_root: &Path,
    artifact: &diag_trace::TraceBundleReplayArtifact,
) -> Result<CaptureArtifact, Box<dyn std::error::Error>> {
    let (storage, external_ref) = if artifact.available {
        let path = extract_root.join(
            artifact
                .file_name
                .as_deref()
                .ok_or("structured replay artifact missing file_name")?,
        );
        (
            ArtifactStorage::ExternalRef,
            Some(path.display().to_string()),
        )
    } else {
        (ArtifactStorage::Unavailable, None)
    };
    Ok(CaptureArtifact {
        id: artifact.id.clone(),
        kind: artifact.kind.clone(),
        media_type: artifact.media_type.clone(),
        encoding: artifact.encoding.clone(),
        digest_sha256: None,
        size_bytes: artifact.size_bytes,
        storage,
        inline_text: None,
        external_ref,
        produced_by: artifact.produced_by.clone(),
    })
}

fn replay_degradation_warnings(document: &DiagnosticDocument) -> Vec<String> {
    let has_source_snippet = document
        .captures
        .iter()
        .any(|capture| matches!(capture.kind, diag_core::ArtifactKind::SourceSnippet));
    let has_locations = document
        .diagnostics
        .iter()
        .any(|node| !node.locations.is_empty());
    let has_absolute_paths = document
        .diagnostics
        .iter()
        .flat_map(|node| &node.locations)
        .any(|location| Path::new(location.path_raw()).is_absolute());

    let mut warnings = vec![
        "replay uses stored bundle contents only; the original source tree is not consulted"
            .to_string(),
    ];
    if has_locations && !has_source_snippet {
        warnings.push(
            "source excerpts may be unavailable because the bundle does not include source files or snippet captures"
                .to_string(),
        );
    }
    if has_absolute_paths {
        warnings.push(
            "path shortening may differ from the original run because the original workspace root is unavailable"
                .to_string(),
        );
    }
    warnings
}

fn replay_render_text(warnings: &[String], render_text: &str) -> String {
    if warnings.is_empty() {
        return render_text.to_string();
    }
    let prefix = warnings
        .iter()
        .map(|warning| format!("note: {warning}"))
        .collect::<Vec<_>>()
        .join("\n");
    format!("{prefix}\n{render_text}")
}

#[allow(clippy::too_many_arguments)]
fn write_public_export_from_bundle_or_replay(
    extract_root: &Path,
    output_path: &Path,
    document: &DiagnosticDocument,
    trace: &TraceEnvelope,
    version_band: VersionBand,
    processing_path: ProcessingPath,
    support_level: SupportLevel,
    source_authority: SourceAuthority,
    fallback_grade: diag_core::FallbackGrade,
    fallback_reason: Option<FallbackReason>,
) -> Result<String, Box<dyn std::error::Error>> {
    let stored_path = extract_root.join(TRACE_BUNDLE_PUBLIC_EXPORT_FILE);
    if let Ok(payload) = fs::read_to_string(&stored_path)
        && serde_json::from_str::<Value>(&payload).is_ok()
    {
        fs::write(output_path, payload)?;
        return Ok("stored_bundle_export".to_string());
    }

    let export: PublicDiagnosticExport = export_from_document(
        document,
        &PublicExportContext::from_document(
            document,
            version_band,
            processing_path,
            support_level,
            representative_allowed_processing_paths(version_band),
            source_authority,
            fallback_grade,
            fallback_reason.or(trace.fallback_reason),
        ),
    );
    fs::write(output_path, export.canonical_json()?)?;
    Ok("reconstructed_from_replay".to_string())
}

fn representative_allowed_processing_paths(version_band: VersionBand) -> Vec<ProcessingPath> {
    match version_band {
        VersionBand::Gcc15 => {
            vec![
                ProcessingPath::DualSinkStructured,
                ProcessingPath::Passthrough,
            ]
        }
        VersionBand::Gcc13_14 | VersionBand::Gcc9_12 => vec![
            ProcessingPath::SingleSinkStructured,
            ProcessingPath::NativeTextCapture,
            ProcessingPath::Passthrough,
        ],
        VersionBand::Gcc16Plus | VersionBand::Unknown => vec![ProcessingPath::Passthrough],
    }
}

fn read_json<T: DeserializeOwned>(path: &Path) -> Result<T, Box<dyn std::error::Error>> {
    let payload =
        fs::read_to_string(path).map_err(|error| format!("read {}: {error}", path.display()))?;
    serde_json::from_str(&payload)
        .map_err(|error| format!("parse {}: {error}", path.display()).into())
}

fn parse_label<T: DeserializeOwned>(label: &str) -> Result<T, Box<dyn std::error::Error>> {
    Ok(serde_json::from_value(Value::String(label.to_string()))?)
}

fn run_cascade_analysis<A: DocumentAnalyzer>(
    analyzer: &A,
    document: &mut DiagnosticDocument,
    context: &CascadeContext,
    policy: &diag_core::CascadePolicySnapshot,
) -> Option<diag_cascade::CascadeReport> {
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        analyzer.analyze_document(document, context, policy)
    })) {
        Ok(Ok(report)) => Some(report),
        Ok(Err(_)) | Err(_) => {
            document.document_analysis = None;
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use diag_core::{ArtifactKind, ToolInfo};
    use diag_trace::{
        TRACE_BUNDLE_MANIFEST_FILE, TRACE_BUNDLE_PUBLIC_EXPORT_FILE,
        TRACE_BUNDLE_REPLAY_INPUT_FILE, TraceBundleArchiveEntry, TraceBundleArchiveSource,
        TraceBundleManifestArtifact, TraceBundleRedactionSummary, TraceRedactionStatus,
        TraceVersionSummary, write_trace_bundle_archive,
    };

    fn sample_trace() -> TraceEnvelope {
        TraceEnvelope {
            trace_id: "trace-123".to_string(),
            selected_mode: "render".to_string(),
            selected_profile: "default".to_string(),
            wrapper_verdict: Some("rendered".to_string()),
            version_summary: Some(TraceVersionSummary {
                wrapper_version: "0.2.0-beta.1".to_string(),
                build_target_triple: "x86_64-unknown-linux-musl".to_string(),
                ir_spec_version: diag_core::IR_SPEC_VERSION.to_string(),
                adapter_spec_version: diag_core::ADAPTER_SPEC_VERSION.to_string(),
                renderer_spec_version: diag_core::RENDERER_SPEC_VERSION.to_string(),
            }),
            environment_summary: Some(diag_trace::TraceEnvironmentSummary {
                backend_path: PathBuf::from("gcc"),
                backend_launcher_path: None,
                backend_version: "gcc (Fake) 15.2.0".to_string(),
                version_band: "gcc15".to_string(),
                processing_path: "single_sink_structured".to_string(),
                support_level: "in_scope".to_string(),
                backend_topology_kind: "direct".to_string(),
                backend_topology_policy_version: "1".to_string(),
                injected_flags: vec!["-fdiagnostics-format=json-file".to_string()],
                sanitized_env_keys: vec!["LC_MESSAGES".to_string()],
                temp_artifact_paths: Vec::new(),
            }),
            capabilities: None,
            timing: None,
            child_exit: Some(diag_trace::TraceChildExit {
                code: Some(1),
                signal: None,
                success: false,
            }),
            parser_result_summary: None,
            fingerprint_summary: None,
            redaction_status: Some(TraceRedactionStatus {
                class: "restricted".to_string(),
                local_only: false,
                normalized_artifacts: vec!["trace.json".to_string()],
            }),
            decision_log: Vec::new(),
            cascade_explainability: None,
            fallback_reason: None,
            warning_messages: Vec::new(),
            artifacts: Vec::new(),
        }
    }

    #[test]
    fn replay_trace_bundle_writes_render_export_and_summary() {
        let temp = tempfile::tempdir().unwrap();
        let bundle_path = temp.path().join("incident.trace-bundle.tar.gz");
        let report_dir = temp.path().join("report");
        let diagnostics_payload = r#"[
          {
            "kind":"error",
            "message":"expected ';' before '}' token",
            "locations":[
              {
                "caret":{"file":"src/main.c","line":4,"column":1}
              }
            ]
          }
        ]"#;

        let replay_input = TraceBundleReplayInput {
            schema_version: "1.0.0-alpha.1".to_string(),
            kind: "gcc_formed_trace_bundle_replay_input".to_string(),
            trace_id: "trace-123".to_string(),
            execution_mode: "render".to_string(),
            processing_path: "single_sink_structured".to_string(),
            structured_capture: "single_sink_json_file".to_string(),
            native_text_capture: "capture_only".to_string(),
            locale_handling: "force_messages_c".to_string(),
            retention_policy: "always".to_string(),
            preserve_native_color: false,
            backend_tool: "gcc".to_string(),
            backend_version: Some("gcc (Fake) 15.2.0".to_string()),
            launcher_tool: None,
            argv_hash: "hash".to_string(),
            child_exit: diag_trace::TraceChildExit {
                code: Some(1),
                signal: None,
                success: false,
            },
            integrity_issues: Vec::new(),
            raw_text_artifacts: vec![diag_trace::TraceBundleReplayArtifact {
                id: "stderr.raw".to_string(),
                kind: ArtifactKind::CompilerStderrText,
                media_type: "text/plain".to_string(),
                encoding: Some("utf-8".to_string()),
                size_bytes: Some(52),
                produced_by: Some(ToolInfo {
                    name: "gcc".to_string(),
                    version: Some("15.2.0".to_string()),
                    component: None,
                    vendor: Some("GNU".to_string()),
                }),
                file_name: Some("stderr.raw".to_string()),
                available: true,
            }],
            structured_artifacts: vec![diag_trace::TraceBundleReplayArtifact {
                id: "diagnostics.json".to_string(),
                kind: ArtifactKind::GccJson,
                media_type: "application/json".to_string(),
                encoding: Some("utf-8".to_string()),
                size_bytes: Some(diagnostics_payload.len() as u64),
                produced_by: Some(ToolInfo {
                    name: "gcc".to_string(),
                    version: Some("15.2.0".to_string()),
                    component: None,
                    vendor: Some("GNU".to_string()),
                }),
                file_name: Some("diagnostics.json".to_string()),
                available: true,
            }],
        };
        let manifest = TraceBundleManifest {
            schema_version: "1.0.0-alpha.1".to_string(),
            kind: "gcc_formed_trace_bundle_manifest".to_string(),
            trace_id: "trace-123".to_string(),
            selected_mode: "render".to_string(),
            selected_profile: "default".to_string(),
            wrapper_verdict: Some("rendered".to_string()),
            version_band: "gcc15".to_string(),
            processing_path: "single_sink_structured".to_string(),
            support_level: "in_scope".to_string(),
            output_path_kind: "user_specified".to_string(),
            size_cap_bytes: diag_trace::DEFAULT_TRACE_BUNDLE_SIZE_CAP_BYTES,
            redaction: TraceBundleRedactionSummary {
                class: "restricted".to_string(),
                review_before_sharing: true,
                normalized_artifacts: vec!["trace.json".to_string()],
                warnings: vec!["review before sharing".to_string()],
            },
            artifacts: vec![TraceBundleManifestArtifact {
                id: "trace.json".to_string(),
                file_name: "trace.json".to_string(),
                role: "trace_envelope".to_string(),
                required: true,
                sensitive: false,
                size_bytes: 1,
            }],
        };

        let entries = vec![
            TraceBundleArchiveEntry {
                file_name: TRACE_BUNDLE_MANIFEST_FILE.to_string(),
                source: TraceBundleArchiveSource::Bytes(
                    serde_json::to_vec_pretty(&manifest).unwrap(),
                ),
            },
            TraceBundleArchiveEntry {
                file_name: "trace.json".to_string(),
                source: TraceBundleArchiveSource::Bytes(
                    serde_json::to_vec_pretty(&sample_trace()).unwrap(),
                ),
            },
            TraceBundleArchiveEntry {
                file_name: TRACE_BUNDLE_REPLAY_INPUT_FILE.to_string(),
                source: TraceBundleArchiveSource::Bytes(
                    serde_json::to_vec_pretty(&replay_input).unwrap(),
                ),
            },
            TraceBundleArchiveEntry {
                file_name: "stderr.raw".to_string(),
                source: TraceBundleArchiveSource::Bytes(
                    b"src/main.c:4:1: error: expected ';' before '}' token\n".to_vec(),
                ),
            },
            TraceBundleArchiveEntry {
                file_name: "diagnostics.json".to_string(),
                source: TraceBundleArchiveSource::Bytes(diagnostics_payload.as_bytes().to_vec()),
            },
            TraceBundleArchiveEntry {
                file_name: TRACE_BUNDLE_PUBLIC_EXPORT_FILE.to_string(),
                source: TraceBundleArchiveSource::Bytes(
                    serde_json::to_vec_pretty(&serde_json::json!({
                        "kind": "gcc_formed_public_diagnostic_export",
                        "status": "available"
                    }))
                    .unwrap(),
                ),
            },
        ];
        write_trace_bundle_archive(
            &bundle_path,
            &entries,
            diag_trace::DEFAULT_TRACE_BUNDLE_SIZE_CAP_BYTES,
        )
        .unwrap();

        let report = run_replay_trace_bundle(&bundle_path, &report_dir).unwrap();
        let render_text = fs::read_to_string(&report.render_path).unwrap();
        assert!(
            render_text.contains("error: [syntax] syntax error")
                || render_text.contains("expected ';' before '}' token")
        );
        assert!(render_text.contains("note: replay uses stored bundle contents only"));
        let export_text = fs::read_to_string(&report.public_export_path).unwrap();
        assert!(export_text.contains("\"status\": \"available\""));
        let summary_text = fs::read_to_string(&report.provenance_summary_path).unwrap();
        assert!(summary_text.contains("\"public_export_source\": \"stored_bundle_export\""));
    }

    #[test]
    fn replay_trace_bundle_handles_missing_public_export_by_reconstructing() {
        let temp = tempfile::tempdir().unwrap();
        let bundle_path = temp.path().join("incident.trace-bundle.tar.gz");
        let report_dir = temp.path().join("report");
        let diagnostics_payload = r#"[
          {
            "kind":"error",
            "message":"expected ';' before '}' token",
            "locations":[
              {
                "caret":{"file":"src/main.c","line":4,"column":1}
              }
            ]
          }
        ]"#;

        let replay_input = TraceBundleReplayInput {
            schema_version: "1.0.0-alpha.1".to_string(),
            kind: "gcc_formed_trace_bundle_replay_input".to_string(),
            trace_id: "trace-123".to_string(),
            execution_mode: "render".to_string(),
            processing_path: "single_sink_structured".to_string(),
            structured_capture: "single_sink_json_file".to_string(),
            native_text_capture: "capture_only".to_string(),
            locale_handling: "force_messages_c".to_string(),
            retention_policy: "always".to_string(),
            preserve_native_color: false,
            backend_tool: "gcc".to_string(),
            backend_version: Some("gcc (Fake) 15.2.0".to_string()),
            launcher_tool: None,
            argv_hash: "hash".to_string(),
            child_exit: diag_trace::TraceChildExit {
                code: Some(1),
                signal: None,
                success: false,
            },
            integrity_issues: Vec::new(),
            raw_text_artifacts: Vec::new(),
            structured_artifacts: vec![diag_trace::TraceBundleReplayArtifact {
                id: "diagnostics.json".to_string(),
                kind: ArtifactKind::GccJson,
                media_type: "application/json".to_string(),
                encoding: Some("utf-8".to_string()),
                size_bytes: Some(diagnostics_payload.len() as u64),
                produced_by: None,
                file_name: Some("diagnostics.json".to_string()),
                available: true,
            }],
        };
        let manifest = TraceBundleManifest {
            schema_version: "1.0.0-alpha.1".to_string(),
            kind: "gcc_formed_trace_bundle_manifest".to_string(),
            trace_id: "trace-123".to_string(),
            selected_mode: "render".to_string(),
            selected_profile: "default".to_string(),
            wrapper_verdict: Some("rendered".to_string()),
            version_band: "gcc15".to_string(),
            processing_path: "single_sink_structured".to_string(),
            support_level: "in_scope".to_string(),
            output_path_kind: "user_specified".to_string(),
            size_cap_bytes: diag_trace::DEFAULT_TRACE_BUNDLE_SIZE_CAP_BYTES,
            redaction: TraceBundleRedactionSummary {
                class: "restricted".to_string(),
                review_before_sharing: true,
                normalized_artifacts: vec!["trace.json".to_string()],
                warnings: vec!["review before sharing".to_string()],
            },
            artifacts: Vec::new(),
        };

        let entries = vec![
            TraceBundleArchiveEntry {
                file_name: TRACE_BUNDLE_MANIFEST_FILE.to_string(),
                source: TraceBundleArchiveSource::Bytes(
                    serde_json::to_vec_pretty(&manifest).unwrap(),
                ),
            },
            TraceBundleArchiveEntry {
                file_name: "trace.json".to_string(),
                source: TraceBundleArchiveSource::Bytes(
                    serde_json::to_vec_pretty(&sample_trace()).unwrap(),
                ),
            },
            TraceBundleArchiveEntry {
                file_name: TRACE_BUNDLE_REPLAY_INPUT_FILE.to_string(),
                source: TraceBundleArchiveSource::Bytes(
                    serde_json::to_vec_pretty(&replay_input).unwrap(),
                ),
            },
            TraceBundleArchiveEntry {
                file_name: "diagnostics.json".to_string(),
                source: TraceBundleArchiveSource::Bytes(diagnostics_payload.as_bytes().to_vec()),
            },
        ];
        write_trace_bundle_archive(
            &bundle_path,
            &entries,
            diag_trace::DEFAULT_TRACE_BUNDLE_SIZE_CAP_BYTES,
        )
        .unwrap();

        let report = run_replay_trace_bundle(&bundle_path, &report_dir).unwrap();
        let summary_text = fs::read_to_string(&report.provenance_summary_path).unwrap();
        assert!(summary_text.contains("\"public_export_source\": \"reconstructed_from_replay\""));
        let export_text = fs::read_to_string(&report.public_export_path).unwrap();
        assert!(export_text.contains("\"kind\": \"gcc_formed_public_diagnostic_export\""));
    }

    #[test]
    fn replay_trace_bundle_accepts_extracted_bundle_directory() {
        let temp = tempfile::tempdir().unwrap();
        let bundle_path = temp.path().join("incident.trace-bundle.tar.gz");
        let extract_dir = temp.path().join("bundle");
        let report_dir = temp.path().join("report");
        let replay_input = TraceBundleReplayInput {
            schema_version: "1.0.0-alpha.1".to_string(),
            kind: "gcc_formed_trace_bundle_replay_input".to_string(),
            trace_id: "trace-123".to_string(),
            execution_mode: "render".to_string(),
            processing_path: "single_sink_structured".to_string(),
            structured_capture: "single_sink_json_file".to_string(),
            native_text_capture: "capture_only".to_string(),
            locale_handling: "force_messages_c".to_string(),
            retention_policy: "always".to_string(),
            preserve_native_color: false,
            backend_tool: "gcc".to_string(),
            backend_version: Some("gcc (Fake) 15.2.0".to_string()),
            launcher_tool: None,
            argv_hash: "hash".to_string(),
            child_exit: diag_trace::TraceChildExit {
                code: Some(1),
                signal: None,
                success: false,
            },
            integrity_issues: Vec::new(),
            raw_text_artifacts: Vec::new(),
            structured_artifacts: vec![diag_trace::TraceBundleReplayArtifact {
                id: "diagnostics.json".to_string(),
                kind: ArtifactKind::GccJson,
                media_type: "application/json".to_string(),
                encoding: Some("utf-8".to_string()),
                size_bytes: Some(2),
                produced_by: None,
                file_name: Some("diagnostics.json".to_string()),
                available: true,
            }],
        };
        let manifest = TraceBundleManifest {
            schema_version: "1.0.0-alpha.1".to_string(),
            kind: "gcc_formed_trace_bundle_manifest".to_string(),
            trace_id: "trace-123".to_string(),
            selected_mode: "render".to_string(),
            selected_profile: "default".to_string(),
            wrapper_verdict: Some("rendered".to_string()),
            version_band: "gcc15".to_string(),
            processing_path: "single_sink_structured".to_string(),
            support_level: "in_scope".to_string(),
            output_path_kind: "user_specified".to_string(),
            size_cap_bytes: diag_trace::DEFAULT_TRACE_BUNDLE_SIZE_CAP_BYTES,
            redaction: TraceBundleRedactionSummary {
                class: "restricted".to_string(),
                review_before_sharing: true,
                normalized_artifacts: vec!["trace.json".to_string()],
                warnings: vec!["review before sharing".to_string()],
            },
            artifacts: Vec::new(),
        };
        let entries = vec![
            TraceBundleArchiveEntry {
                file_name: TRACE_BUNDLE_MANIFEST_FILE.to_string(),
                source: TraceBundleArchiveSource::Bytes(
                    serde_json::to_vec_pretty(&manifest).unwrap(),
                ),
            },
            TraceBundleArchiveEntry {
                file_name: "trace.json".to_string(),
                source: TraceBundleArchiveSource::Bytes(
                    serde_json::to_vec_pretty(&sample_trace()).unwrap(),
                ),
            },
            TraceBundleArchiveEntry {
                file_name: TRACE_BUNDLE_REPLAY_INPUT_FILE.to_string(),
                source: TraceBundleArchiveSource::Bytes(
                    serde_json::to_vec_pretty(&replay_input).unwrap(),
                ),
            },
            TraceBundleArchiveEntry {
                file_name: "diagnostics.json".to_string(),
                source: TraceBundleArchiveSource::Bytes(b"[]".to_vec()),
            },
        ];
        write_trace_bundle_archive(
            &bundle_path,
            &entries,
            diag_trace::DEFAULT_TRACE_BUNDLE_SIZE_CAP_BYTES,
        )
        .unwrap();
        extract_trace_bundle_archive(&bundle_path, &extract_dir).unwrap();

        let report = run_replay_trace_bundle(&extract_dir, &report_dir).unwrap();
        assert!(report.render_path.exists());
        assert!(report.provenance_summary_path.exists());
    }

    #[test]
    fn replay_trace_bundle_fails_cleanly_for_corrupt_archive() {
        let temp = tempfile::tempdir().unwrap();
        let bundle_path = temp.path().join("incident.trace-bundle.tar.gz");
        let report_dir = temp.path().join("report");
        fs::write(&bundle_path, b"not a valid gzip tar archive").unwrap();

        let error = run_replay_trace_bundle(&bundle_path, &report_dir)
            .err()
            .unwrap()
            .to_string();
        assert!(error.contains("extract"));
        assert!(error.contains("incident.trace-bundle.tar.gz"));
    }

    #[test]
    fn replay_trace_bundle_fails_cleanly_for_missing_manifest() {
        let temp = tempfile::tempdir().unwrap();
        let bundle_path = temp.path().join("incident.trace-bundle.tar.gz");
        let report_dir = temp.path().join("report");
        write_trace_bundle_archive(
            &bundle_path,
            &[TraceBundleArchiveEntry {
                file_name: "trace.json".to_string(),
                source: TraceBundleArchiveSource::Bytes(
                    serde_json::to_vec_pretty(&sample_trace()).unwrap(),
                ),
            }],
            diag_trace::DEFAULT_TRACE_BUNDLE_SIZE_CAP_BYTES,
        )
        .unwrap();

        let error = run_replay_trace_bundle(&bundle_path, &report_dir)
            .err()
            .unwrap()
            .to_string();
        assert!(error.contains(TRACE_BUNDLE_MANIFEST_FILE));
    }
}
