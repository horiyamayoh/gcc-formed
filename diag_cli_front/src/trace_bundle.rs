use crate::args::TraceBundleSink;
use diag_capture_runtime::{CaptureBundle, CaptureOutcome};
use diag_public_export::PublicDiagnosticExport;
use diag_trace::{
    DEFAULT_TRACE_BUNDLE_SIZE_CAP_BYTES, TRACE_BUNDLE_MANIFEST_FILE,
    TRACE_BUNDLE_PUBLIC_EXPORT_FILE, TRACE_BUNDLE_REPLAY_INPUT_FILE, TraceBundleArchiveEntry,
    TraceBundleArchiveSource, TraceBundleManifest, TraceBundleManifestArtifact,
    TraceBundleRedactionSummary, TraceBundleReplayArtifact, TraceBundleReplayInput, TraceChildExit,
    TraceEnvelope, TraceError, TraceRedactionStatus, WrapperPaths, extract_trace_bundle_archive,
    write_trace_bundle_archive,
};
use serde_json::{Value, json};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

pub(crate) enum TraceBundleWriteOutcome {
    Created(PathBuf),
    Failed {
        output_path: PathBuf,
        local_trace_dir: Option<PathBuf>,
        error: String,
    },
    Skipped,
}

pub(crate) struct TraceBundleWriteRequest<'a> {
    pub(crate) sink: Option<&'a TraceBundleSink>,
    pub(crate) paths: &'a WrapperPaths,
    pub(crate) capture: &'a CaptureOutcome,
    pub(crate) bundle: &'a CaptureBundle,
    pub(crate) public_export: &'a PublicDiagnosticExport,
}

pub(crate) fn maybe_write_trace_bundle(
    request: TraceBundleWriteRequest<'_>,
) -> TraceBundleWriteOutcome {
    let Some(sink) = request.sink else {
        return TraceBundleWriteOutcome::Skipped;
    };
    let Some(retained_trace_dir) = request.capture.retained_trace_dir.as_ref() else {
        return TraceBundleWriteOutcome::Failed {
            output_path: fallback_output_path(request.paths, sink),
            local_trace_dir: None,
            error: "retained trace directory was not produced".to_string(),
        };
    };

    match write_trace_bundle(
        request.paths,
        sink,
        retained_trace_dir,
        request.bundle,
        request.public_export,
    ) {
        Ok(path) => TraceBundleWriteOutcome::Created(path),
        Err((output_path, error)) => {
            let _ = fs::remove_file(&output_path);
            TraceBundleWriteOutcome::Failed {
                output_path,
                local_trace_dir: Some(retained_trace_dir.clone()),
                error: error.to_string(),
            }
        }
    }
}

pub(crate) fn emit_trace_bundle_note(outcome: &TraceBundleWriteOutcome) {
    match outcome {
        TraceBundleWriteOutcome::Created(path) => {
            eprintln!("note: trace bundle saved to {}", path.display());
        }
        TraceBundleWriteOutcome::Failed {
            output_path,
            local_trace_dir,
            error,
            ..
        } => {
            if let Some(dir) = local_trace_dir {
                eprintln!(
                    "note: trace bundle was not created at {} ({error}); local trace directory retained at {}",
                    output_path.display(),
                    dir.display(),
                );
            } else {
                eprintln!(
                    "note: trace bundle was not created at {} ({error})",
                    output_path.display()
                );
            }
        }
        TraceBundleWriteOutcome::Skipped => {}
    }
}

fn write_trace_bundle(
    paths: &WrapperPaths,
    sink: &TraceBundleSink,
    retained_trace_dir: &Path,
    bundle: &CaptureBundle,
    public_export: &PublicDiagnosticExport,
) -> Result<PathBuf, (PathBuf, TraceError)> {
    let trace = load_trace_envelope(&retained_trace_dir.join("trace.json"))
        .map_err(|error| (fallback_output_path(paths, sink), error))?;
    let output_path = resolve_output_path(paths, sink, &trace)
        .map_err(|error| (fallback_output_path(paths, sink), error))?;
    let output_path_kind = output_path_kind_label(sink).to_string();

    let sanitized_trace = sanitize_trace_for_bundle(&trace);
    let sanitized_invocation =
        sanitized_invocation_summary(&retained_trace_dir.join("invocation.json"), bundle);
    let replay_input = build_replay_input(bundle, &trace);
    let public_export_payload = public_export
        .canonical_json()
        .map_err(|error| (output_path.clone(), TraceError::Json(error)))?;
    let trace_payload = serde_json::to_vec_pretty(&sanitized_trace)
        .map_err(|error| (output_path.clone(), TraceError::Json(error)))?;
    let invocation_payload = serde_json::to_vec_pretty(&sanitized_invocation)
        .map_err(|error| (output_path.clone(), TraceError::Json(error)))?;
    let replay_payload = serde_json::to_vec_pretty(&replay_input)
        .map_err(|error| (output_path.clone(), TraceError::Json(error)))?;

    let mut entries = vec![
        TraceBundleArchiveEntry {
            file_name: "trace.json".to_string(),
            source: TraceBundleArchiveSource::Bytes(trace_payload.clone()),
        },
        TraceBundleArchiveEntry {
            file_name: "invocation.json".to_string(),
            source: TraceBundleArchiveSource::Bytes(invocation_payload.clone()),
        },
        TraceBundleArchiveEntry {
            file_name: TRACE_BUNDLE_REPLAY_INPUT_FILE.to_string(),
            source: TraceBundleArchiveSource::Bytes(replay_payload.clone()),
        },
        TraceBundleArchiveEntry {
            file_name: TRACE_BUNDLE_PUBLIC_EXPORT_FILE.to_string(),
            source: TraceBundleArchiveSource::Bytes(public_export_payload.clone().into_bytes()),
        },
    ];

    let mut manifest_artifacts = vec![
        manifest_artifact(
            "trace.json",
            "trace.json",
            "trace_envelope",
            true,
            false,
            trace_payload.len() as u64,
        ),
        manifest_artifact(
            "invocation.json",
            "invocation.json",
            "sanitized_invocation_summary",
            true,
            false,
            invocation_payload.len() as u64,
        ),
        manifest_artifact(
            TRACE_BUNDLE_REPLAY_INPUT_FILE,
            TRACE_BUNDLE_REPLAY_INPUT_FILE,
            "replay_input",
            true,
            false,
            replay_payload.len() as u64,
        ),
        manifest_artifact(
            TRACE_BUNDLE_PUBLIC_EXPORT_FILE,
            TRACE_BUNDLE_PUBLIC_EXPORT_FILE,
            "public_export",
            true,
            false,
            public_export_payload.len() as u64,
        ),
    ];

    for artifact in retained_artifact_specs(retained_trace_dir, bundle) {
        manifest_artifacts.push(manifest_artifact(
            &artifact.id,
            &artifact.file_name,
            artifact.role,
            artifact.required,
            artifact.sensitive,
            artifact.size_bytes,
        ));
        entries.push(TraceBundleArchiveEntry {
            file_name: artifact.file_name,
            source: TraceBundleArchiveSource::File(artifact.path),
        });
    }

    let redaction = bundle_redaction_summary(trace.redaction_status.as_ref());
    let manifest_labels = manifest_labels(bundle, &trace);
    let manifest = TraceBundleManifest {
        schema_version: "1.0.0-alpha.1".to_string(),
        kind: "gcc_formed_trace_bundle_manifest".to_string(),
        trace_id: trace.trace_id.clone(),
        selected_mode: trace.selected_mode.clone(),
        selected_profile: trace.selected_profile.clone(),
        wrapper_verdict: trace.wrapper_verdict.clone(),
        version_band: manifest_labels.version_band,
        processing_path: manifest_labels.processing_path,
        support_level: manifest_labels.support_level,
        output_path_kind,
        size_cap_bytes: DEFAULT_TRACE_BUNDLE_SIZE_CAP_BYTES,
        redaction,
        artifacts: manifest_artifacts,
    };
    let manifest_payload = serde_json::to_vec_pretty(&manifest)
        .map_err(|error| (output_path.clone(), TraceError::Json(error)))?;
    entries.push(TraceBundleArchiveEntry {
        file_name: TRACE_BUNDLE_MANIFEST_FILE.to_string(),
        source: TraceBundleArchiveSource::Bytes(manifest_payload),
    });

    write_trace_bundle_archive(&output_path, &entries, DEFAULT_TRACE_BUNDLE_SIZE_CAP_BYTES)
        .map_err(|error| (output_path.clone(), error))?;
    Ok(output_path)
}

fn load_trace_envelope(path: &Path) -> Result<TraceEnvelope, TraceError> {
    let payload = fs::read_to_string(path)?;
    Ok(serde_json::from_str(&payload)?)
}

fn resolve_output_path(
    paths: &WrapperPaths,
    sink: &TraceBundleSink,
    trace: &TraceEnvelope,
) -> Result<PathBuf, TraceError> {
    let output_path = match sink {
        TraceBundleSink::Auto => paths
            .trace_root
            .join("bundles")
            .join(format!("{}.trace-bundle.tar.gz", trace.trace_id)),
        TraceBundleSink::File(path) => normalize_user_path(path),
    };
    let install_root = normalize_user_path(&paths.install_root);
    if output_path.starts_with(&install_root) {
        return Err(TraceError::InvalidBundlePath(format!(
            "bundle output must not live under install root: {}",
            output_path.display()
        )));
    }
    Ok(output_path)
}

fn fallback_output_path(paths: &WrapperPaths, sink: &TraceBundleSink) -> PathBuf {
    match sink {
        TraceBundleSink::Auto => paths
            .trace_root
            .join("bundles")
            .join("trace-bundle-unavailable.tar.gz"),
        TraceBundleSink::File(path) => normalize_user_path(path),
    }
}

fn normalize_user_path(path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("."))
            .join(path)
    }
}

fn output_path_kind_label(sink: &TraceBundleSink) -> &'static str {
    match sink {
        TraceBundleSink::Auto => "state_root",
        TraceBundleSink::File(_) => "user_specified",
    }
}

fn sanitize_trace_for_bundle(trace: &TraceEnvelope) -> TraceEnvelope {
    let mut sanitized = trace.clone();
    if let Some(summary) = sanitized.environment_summary.as_mut() {
        summary.backend_path = basename_path(&summary.backend_path);
        summary.backend_launcher_path = summary
            .backend_launcher_path
            .as_ref()
            .map(|path| basename_path(path));
        summary.temp_artifact_paths.clear();
    }
    if let Some(redaction) = sanitized.redaction_status.as_mut() {
        redaction.local_only = false;
    }
    for artifact in &mut sanitized.artifacts {
        artifact.path = artifact
            .path
            .as_ref()
            .and_then(|path| path.file_name().map(PathBuf::from));
    }
    sanitized
}

fn basename_path(path: &Path) -> PathBuf {
    path.file_name()
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(path))
}

fn sanitized_invocation_summary(path: &Path, bundle: &CaptureBundle) -> Value {
    let raw = fs::read_to_string(path)
        .ok()
        .and_then(|payload| serde_json::from_str::<Value>(&payload).ok());
    let normalized_invocation = raw
        .as_ref()
        .and_then(|value| value.get("normalized_invocation"))
        .cloned()
        .unwrap_or_else(|| json!({}));
    let redaction_class = raw
        .as_ref()
        .and_then(|value| value.get("redaction_class"))
        .and_then(Value::as_str)
        .unwrap_or("restricted");
    let child_env_policy = raw
        .as_ref()
        .and_then(|value| value.get("child_env_policy"))
        .cloned()
        .unwrap_or_else(|| json!({}));
    let wrapper_env_keys = raw
        .as_ref()
        .and_then(|value| value.get("wrapper_env"))
        .and_then(Value::as_object)
        .map(|env| env.keys().cloned().map(Value::String).collect::<Vec<_>>())
        .unwrap_or_default();
    json!({
        "backend_tool": file_name_string(&bundle.invocation.backend_path),
        "launcher_tool": bundle.invocation.launcher_path.as_deref().map(file_name_string),
        "selected_mode": snake_case_label(&bundle.plan.execution_mode),
        "processing_path": snake_case_label(&bundle.plan.processing_path),
        "argv_hash": bundle.invocation.argv_hash,
        "normalized_invocation": normalized_invocation,
        "redaction_class": redaction_class,
        "child_env_policy": child_env_policy,
        "wrapper_env_keys": wrapper_env_keys,
        "cwd": "<redacted>"
    })
}

fn build_replay_input(bundle: &CaptureBundle, trace: &TraceEnvelope) -> TraceBundleReplayInput {
    TraceBundleReplayInput {
        schema_version: "1.0.0-alpha.1".to_string(),
        kind: "gcc_formed_trace_bundle_replay_input".to_string(),
        trace_id: trace.trace_id.clone(),
        execution_mode: snake_case_label(&bundle.plan.execution_mode),
        processing_path: snake_case_label(&bundle.plan.processing_path),
        structured_capture: snake_case_label(&bundle.plan.structured_capture),
        native_text_capture: snake_case_label(&bundle.plan.native_text_capture),
        locale_handling: snake_case_label(&bundle.plan.locale_handling),
        retention_policy: snake_case_label(&bundle.plan.retention_policy),
        preserve_native_color: bundle.plan.preserve_native_color,
        backend_tool: file_name_string(&bundle.invocation.backend_path),
        backend_version: trace
            .environment_summary
            .as_ref()
            .map(|summary| summary.backend_version.clone()),
        launcher_tool: bundle
            .invocation
            .launcher_path
            .as_deref()
            .map(file_name_string),
        argv_hash: bundle.invocation.argv_hash.clone(),
        child_exit: TraceChildExit {
            code: bundle.exit_status.code,
            signal: bundle.exit_status.signal,
            success: bundle.exit_status.success,
        },
        integrity_issues: bundle.integrity_issues.clone(),
        raw_text_artifacts: bundle
            .raw_text_artifacts
            .iter()
            .map(|artifact| TraceBundleReplayArtifact {
                id: artifact.id.clone(),
                kind: artifact.kind.clone(),
                media_type: artifact.media_type.clone(),
                encoding: artifact.encoding.clone(),
                size_bytes: artifact.size_bytes,
                produced_by: artifact.produced_by.clone(),
                file_name: Some(artifact.id.clone()),
                available: true,
            })
            .collect(),
        structured_artifacts: bundle
            .structured_artifacts
            .iter()
            .map(|artifact| TraceBundleReplayArtifact {
                id: artifact.id.clone(),
                kind: artifact.kind.clone(),
                media_type: artifact.media_type.clone(),
                encoding: artifact.encoding.clone(),
                size_bytes: artifact.size_bytes,
                produced_by: artifact.produced_by.clone(),
                file_name: Some(artifact.id.clone()),
                available: artifact.external_ref.as_deref().is_some_and(|_| true),
            })
            .collect(),
    }
}

fn bundle_redaction_summary(status: Option<&TraceRedactionStatus>) -> TraceBundleRedactionSummary {
    let normalized_artifacts = status
        .map(|status| status.normalized_artifacts.clone())
        .unwrap_or_default();
    TraceBundleRedactionSummary {
        class: status
            .map(|status| status.class.clone())
            .unwrap_or_else(|| "restricted".to_string()),
        review_before_sharing: true,
        normalized_artifacts,
        warnings: vec![
            "review before sharing; raw compiler artifacts may still contain file paths, usernames, repo paths, source excerpts, and compiler flags".to_string(),
            "do not upload this bundle to public issues for security-sensitive or embargoed incidents".to_string(),
        ],
    }
}

fn manifest_artifact(
    id: &str,
    file_name: &str,
    role: &str,
    required: bool,
    sensitive: bool,
    size_bytes: u64,
) -> TraceBundleManifestArtifact {
    TraceBundleManifestArtifact {
        id: id.to_string(),
        file_name: file_name.to_string(),
        role: role.to_string(),
        required,
        sensitive,
        size_bytes,
    }
}

struct RetainedArtifactSpec {
    id: String,
    file_name: String,
    path: PathBuf,
    role: &'static str,
    required: bool,
    sensitive: bool,
    size_bytes: u64,
}

fn retained_artifact_specs(
    retained_trace_dir: &Path,
    bundle: &CaptureBundle,
) -> Vec<RetainedArtifactSpec> {
    let mut specs = Vec::new();
    for file_name in ["stderr.raw", "ir.analysis.json"] {
        let path = retained_trace_dir.join(file_name);
        if let Ok(metadata) = fs::metadata(&path) {
            specs.push(RetainedArtifactSpec {
                id: file_name.to_string(),
                file_name: file_name.to_string(),
                path,
                role: if file_name == "stderr.raw" {
                    "raw_stderr"
                } else {
                    "normalized_analysis"
                },
                required: file_name == "stderr.raw",
                sensitive: file_name == "stderr.raw",
                size_bytes: metadata.len(),
            });
        }
    }
    for artifact in &bundle.structured_artifacts {
        let file_name = artifact.id.clone();
        let path = retained_trace_dir.join(&file_name);
        let Ok(metadata) = fs::metadata(&path) else {
            continue;
        };
        specs.push(RetainedArtifactSpec {
            id: artifact.id.clone(),
            file_name,
            path,
            role: "structured_capture",
            required: true,
            sensitive: true,
            size_bytes: metadata.len(),
        });
    }
    specs
}

fn file_name_string(path: &str) -> String {
    Path::new(path)
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or(path)
        .to_string()
}

fn snake_case_label<T: serde::Serialize>(value: &T) -> String {
    serde_json::to_value(value)
        .ok()
        .and_then(|value| value.as_str().map(ToOwned::to_owned))
        .unwrap_or_else(|| "unknown".to_string())
}

struct ManifestLabels {
    version_band: String,
    processing_path: String,
    support_level: String,
}

fn manifest_labels(bundle: &CaptureBundle, trace: &TraceEnvelope) -> ManifestLabels {
    ManifestLabels {
        version_band: trace
            .environment_summary
            .as_ref()
            .map(|summary| summary.version_band.clone())
            .unwrap_or_else(|| "unknown".to_string()),
        processing_path: trace
            .environment_summary
            .as_ref()
            .map(|summary| summary.processing_path.clone())
            .unwrap_or_else(|| snake_case_label(&bundle.plan.processing_path)),
        support_level: trace
            .environment_summary
            .as_ref()
            .map(|summary| summary.support_level.clone())
            .unwrap_or_else(|| "unknown".to_string()),
    }
}

#[allow(dead_code)]
pub(crate) fn inspect_trace_bundle(path: &Path, destination: &Path) -> Result<(), TraceError> {
    extract_trace_bundle_archive(path, destination)
}

#[cfg(test)]
mod tests {
    use super::*;
    use diag_capture_runtime::{
        CaptureInvocation, CapturePlan, ExecutionMode, LocaleHandling, NativeTextCapturePolicy,
        StructuredCapturePolicy,
    };
    use diag_core::ArtifactKind;
    use diag_trace::RetentionPolicy;

    fn sample_bundle() -> CaptureBundle {
        CaptureBundle {
            plan: CapturePlan {
                execution_mode: ExecutionMode::Render,
                processing_path: diag_backend_probe::ProcessingPath::SingleSinkStructured,
                structured_capture: StructuredCapturePolicy::SingleSinkJsonFile,
                native_text_capture: NativeTextCapturePolicy::CaptureOnly,
                preserve_native_color: false,
                locale_handling: LocaleHandling::ForceMessagesC,
                retention_policy: RetentionPolicy::Always,
            },
            invocation: CaptureInvocation {
                backend_path: "gcc".to_string(),
                launcher_path: None,
                spawn_path: "gcc".to_string(),
                argv: Vec::new(),
                spawn_argv: Vec::new(),
                argv_hash: "hash".to_string(),
                cwd: "/tmp/project".to_string(),
                selected_mode: ExecutionMode::Render,
                processing_path: diag_backend_probe::ProcessingPath::SingleSinkStructured,
            },
            raw_text_artifacts: vec![diag_core::CaptureArtifact {
                id: "stderr.raw".to_string(),
                kind: ArtifactKind::CompilerStderrText,
                media_type: "text/plain".to_string(),
                encoding: Some("utf-8".to_string()),
                digest_sha256: None,
                size_bytes: Some(1),
                storage: diag_core::ArtifactStorage::Inline,
                inline_text: Some("x".to_string()),
                external_ref: None,
                produced_by: None,
            }],
            structured_artifacts: Vec::new(),
            exit_status: diag_capture_runtime::ExitStatusInfo {
                code: Some(1),
                signal: None,
                success: false,
            },
            integrity_issues: Vec::new(),
        }
    }

    #[test]
    fn manifest_labels_do_not_confuse_version_band_with_processing_path() {
        let labels = manifest_labels(
            &sample_bundle(),
            &TraceEnvelope {
                trace_id: "trace-1".to_string(),
                selected_mode: "render".to_string(),
                selected_profile: "default".to_string(),
                wrapper_verdict: Some("rendered".to_string()),
                version_summary: None,
                environment_summary: None,
                capabilities: None,
                timing: None,
                child_exit: None,
                parser_result_summary: None,
                fingerprint_summary: None,
                redaction_status: None,
                decision_log: Vec::new(),
                cascade_explainability: None,
                fallback_reason: None,
                warning_messages: Vec::new(),
                artifacts: Vec::new(),
            },
        );

        assert_eq!(labels.version_band, "unknown");
        assert_eq!(labels.processing_path, "single_sink_structured");
        assert_eq!(labels.support_level, "unknown");
    }
}
