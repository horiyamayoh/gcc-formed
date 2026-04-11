//! Ingestion orchestration for GCC diagnostic artifacts.

use crate::fallback::{failed_document, fallback_document, passthrough_document, passthrough_node};
use crate::gcc_json::from_gcc_json_artifact;
use crate::sarif::from_sarif_artifact;
use crate::stderr::augment_context_chains_from_stderr;
use diag_backend_probe::ProcessingPath;
use diag_capture_runtime::{
    CaptureBundle, CaptureInvocation, CapturePlan, ExecutionMode, ExitStatusInfo, LocaleHandling,
    NativeTextCapturePolicy, StructuredCapturePolicy,
};
use diag_core::{
    ArtifactKind, ArtifactStorage, CaptureArtifact, Confidence, DiagnosticDocument,
    DocumentCompleteness, FallbackGrade, FallbackReason, IntegrityIssue, IssueSeverity, IssueStage,
    NodeCompleteness, ProducerInfo, Provenance, ProvenanceSource, RunInfo, SemanticRole,
    SourceAuthority,
};
use diag_residual_text::classify;
use diag_trace::RetentionPolicy;
use std::fs;
use std::path::Path;

/// Errors that can occur during GCC diagnostic ingestion.
#[derive(Debug, thiserror::Error)]
pub enum AdapterError {
    /// An I/O error occurred while reading an artifact from disk.
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    /// The SARIF or GCC-JSON payload could not be parsed as valid JSON.
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),
    /// The SARIF `version` field is not a supported 2.1.x version.
    #[error("unsupported SARIF version: {0}")]
    UnsupportedVersion(String),
    /// The SARIF payload has no top-level `runs` array.
    #[error("missing runs array in SARIF payload")]
    MissingRuns,
}

/// Result of a simplified ingestion via [`crate::ingest_with_reason`].
#[derive(Debug)]
pub struct IngestOutcome {
    /// The converted diagnostic document.
    pub document: DiagnosticDocument,
    /// If the adapter fell back to a non-structured path, the reason why.
    pub fallback_reason: Option<FallbackReason>,
}

/// Configuration that governs how a capture bundle is ingested.
#[derive(Debug, Clone)]
pub struct IngestPolicy {
    /// Metadata about the tool that produced the diagnostic document.
    pub producer: ProducerInfo,
    /// Runtime context for the compiler invocation being ingested.
    pub run: RunInfo,
}

/// Full ingestion report returned by [`crate::ingest_bundle`].
#[derive(Debug)]
pub struct IngestReport {
    /// The converted diagnostic document.
    pub document: DiagnosticDocument,
    /// Whether the document was derived from structured or residual input.
    pub source_authority: SourceAuthority,
    /// Upper bound on the confidence of diagnostics in this report.
    pub confidence_ceiling: Confidence,
    /// Degree to which fallback processing was applied.
    pub fallback_grade: FallbackGrade,
    /// Integrity issues encountered during ingestion.
    pub warnings: Vec<IntegrityIssue>,
    /// If the adapter fell back to a non-structured path, the reason why.
    pub fallback_reason: Option<FallbackReason>,
}

#[derive(Debug, Clone, Copy)]
enum StructuredInput<'a> {
    AvailableSarif(&'a CaptureArtifact),
    MissingSarif(&'a CaptureArtifact),
    AvailableGccJson(&'a CaptureArtifact),
    MissingGccJson(&'a CaptureArtifact),
    Unsupported(&'a CaptureArtifact),
    None,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ResidualContract {
    BoundedRender,
    FailOpen,
}

/// Ingest GCC output and return a [`DiagnosticDocument`].
///
/// Convenience wrapper around [`crate::ingest_with_reason`] that discards the
/// fallback reason. Accepts an optional SARIF file path and raw stderr text.
pub fn ingest(
    sarif_path: Option<&Path>,
    stderr_text: &str,
    producer: ProducerInfo,
    run: RunInfo,
) -> Result<DiagnosticDocument, AdapterError> {
    Ok(ingest_with_reason(sarif_path, stderr_text, producer, run)?.document)
}

/// Ingest a full [`CaptureBundle`] and return an [`IngestReport`].
///
/// This is the primary entry point for production use. It examines the
/// bundle's structured artifacts (SARIF, GCC-JSON) and stderr text,
/// selects the best ingestion strategy, and produces a report that
/// includes source-authority, confidence, and fallback metadata.
pub fn ingest_bundle(
    bundle: &CaptureBundle,
    policy: IngestPolicy,
) -> Result<IngestReport, AdapterError> {
    let structured_input = structured_input(bundle);
    let has_authoritative_structured = matches!(
        structured_input,
        StructuredInput::AvailableSarif(_) | StructuredInput::AvailableGccJson(_)
    );
    let stderr_text = bundle.stderr_text().unwrap_or_default();

    let (mut document, source_authority, fallback_grade, fallback_reason) = match structured_input {
        StructuredInput::AvailableSarif(artifact) => {
            match from_sarif_artifact(artifact, &policy.producer, &policy.run) {
                Ok(document) => (
                    document,
                    SourceAuthority::Structured,
                    FallbackGrade::None,
                    None,
                ),
                Err(error) => (
                    failed_document(
                        &policy.producer,
                        &policy.run,
                        stderr_text,
                        format!(
                            "failed to parse authoritative SARIF; preserving raw diagnostics: {error}"
                        ),
                        Some(artifact.id.as_str()),
                    ),
                    source_authority_for_residual(stderr_text),
                    FallbackGrade::FailOpen,
                    Some(FallbackReason::SarifParseFailed),
                ),
            }
        }
        StructuredInput::MissingSarif(artifact) => (
            fallback_document(
                &policy.producer,
                &policy.run,
                DocumentCompleteness::Passthrough,
                stderr_text,
                "expected authoritative SARIF was not produced; preserving raw diagnostics"
                    .to_string(),
                Some(artifact.id.as_str()),
            ),
            source_authority_for_residual(stderr_text),
            FallbackGrade::FailOpen,
            Some(FallbackReason::SarifMissing),
        ),
        StructuredInput::AvailableGccJson(artifact) => {
            match from_gcc_json_artifact(artifact, &policy.producer, &policy.run) {
                Ok(document) => (
                    document,
                    SourceAuthority::Structured,
                    FallbackGrade::None,
                    None,
                ),
                Err(error) => (
                    failed_document(
                        &policy.producer,
                        &policy.run,
                        stderr_text,
                        format!(
                            "failed to parse structured GCC JSON; preserving raw diagnostics: {error}"
                        ),
                        Some(artifact.id.as_str()),
                    ),
                    source_authority_for_residual(stderr_text),
                    FallbackGrade::FailOpen,
                    None,
                ),
            }
        }
        StructuredInput::MissingGccJson(artifact) => (
            fallback_document(
                &policy.producer,
                &policy.run,
                DocumentCompleteness::Passthrough,
                stderr_text,
                "expected structured GCC JSON was not produced; preserving raw diagnostics"
                    .to_string(),
                Some(artifact.id.as_str()),
            ),
            source_authority_for_residual(stderr_text),
            FallbackGrade::FailOpen,
            None,
        ),
        StructuredInput::Unsupported(artifact) => {
            let mut document = passthrough_document(&policy.producer, &policy.run);
            document.integrity_issues.push(IntegrityIssue {
                severity: IssueSeverity::Warning,
                stage: IssueStage::Parse,
                message: format!(
                    "structured artifact '{}' is not yet supported; preserving raw diagnostics",
                    artifact.id
                ),
                provenance: Some(Provenance {
                    source: ProvenanceSource::Policy,
                    capture_refs: vec![artifact.id.clone()],
                }),
            });
            (
                document,
                source_authority_for_residual(stderr_text),
                FallbackGrade::Compatibility,
                None,
            )
        }
        StructuredInput::None => (
            passthrough_document(&policy.producer, &policy.run),
            source_authority_for_residual(stderr_text),
            fallback_grade_for_residual(stderr_text),
            None,
        ),
    };
    materialize_capture_artifacts(&mut document, bundle);

    let residual_nodes = classify(stderr_text, !has_authoritative_structured);
    let has_renderable_residual = residual_nodes.iter().any(|node| {
        !matches!(node.semantic_role, SemanticRole::Passthrough)
            && !matches!(node.node_completeness, NodeCompleteness::Passthrough)
    });
    let residual_contract = residual_contract_for(stderr_text, has_renderable_residual);
    if document.diagnostics.is_empty() && residual_nodes.is_empty() && !stderr_text.is_empty() {
        if !matches!(document.document_completeness, DocumentCompleteness::Failed) {
            document.document_completeness = DocumentCompleteness::Passthrough;
        }
        document.diagnostics.push(passthrough_node(stderr_text));
    } else if !residual_nodes.is_empty() {
        if has_renderable_residual
            && !matches!(document.document_completeness, DocumentCompleteness::Failed)
        {
            document.document_completeness = DocumentCompleteness::Partial;
        }
        document.diagnostics.extend(residual_nodes);
    }
    if has_authoritative_structured {
        augment_context_chains_from_stderr(&mut document, stderr_text);
    }
    document.refresh_fingerprints();

    let (fallback_grade, fallback_reason) = match structured_input {
        StructuredInput::None | StructuredInput::Unsupported(_) => residual_outcome_for_contract(
            residual_contract,
            source_authority,
            fallback_grade,
            fallback_reason,
        ),
        StructuredInput::AvailableSarif(_)
        | StructuredInput::MissingSarif(_)
        | StructuredInput::AvailableGccJson(_)
        | StructuredInput::MissingGccJson(_) => (fallback_grade, fallback_reason),
    };

    let warnings = document.integrity_issues.clone();
    Ok(IngestReport {
        confidence_ceiling: confidence_ceiling_for(source_authority, fallback_grade),
        document,
        source_authority,
        fallback_grade,
        warnings,
        fallback_reason,
    })
}

fn materialize_capture_artifacts(document: &mut DiagnosticDocument, bundle: &CaptureBundle) {
    document.captures = bundle.capture_artifacts();
}

/// Ingest GCC output and return an [`IngestOutcome`] that includes a
/// fallback reason when structured input was unavailable or unparseable.
///
/// Builds a compatibility [`CaptureBundle`] from the legacy path/stderr
/// arguments and delegates to [`crate::ingest_bundle`].
pub fn ingest_with_reason(
    sarif_path: Option<&Path>,
    stderr_text: &str,
    producer: ProducerInfo,
    run: RunInfo,
) -> Result<IngestOutcome, AdapterError> {
    let report = ingest_bundle(
        &compatibility_bundle_from_legacy_inputs(sarif_path, stderr_text, &run),
        IngestPolicy { producer, run },
    )?;
    Ok(IngestOutcome {
        document: report.document,
        fallback_reason: report.fallback_reason,
    })
}

fn structured_input(bundle: &CaptureBundle) -> StructuredInput<'_> {
    if let Some(artifact) = bundle
        .structured_artifacts
        .iter()
        .find(|artifact| matches!(artifact.kind, ArtifactKind::GccSarif))
    {
        if structured_artifact_payload_available(artifact) {
            return StructuredInput::AvailableSarif(artifact);
        }
        return StructuredInput::MissingSarif(artifact);
    }

    if let Some(artifact) = bundle
        .structured_artifacts
        .iter()
        .find(|artifact| matches!(artifact.kind, ArtifactKind::GccJson))
    {
        if structured_artifact_payload_available(artifact) {
            return StructuredInput::AvailableGccJson(artifact);
        }
        return StructuredInput::MissingGccJson(artifact);
    }

    bundle
        .structured_artifacts
        .first()
        .map(StructuredInput::Unsupported)
        .unwrap_or(StructuredInput::None)
}

fn structured_artifact_payload_available(artifact: &CaptureArtifact) -> bool {
    artifact.inline_text.is_some() || artifact.external_ref.is_some()
}

fn source_authority_for_residual(stderr_text: &str) -> SourceAuthority {
    if stderr_text.trim().is_empty() {
        SourceAuthority::None
    } else {
        SourceAuthority::ResidualText
    }
}

fn fallback_grade_for_residual(stderr_text: &str) -> FallbackGrade {
    if stderr_text.trim().is_empty() {
        FallbackGrade::None
    } else {
        FallbackGrade::Compatibility
    }
}

fn residual_contract_for(stderr_text: &str, has_renderable_residual: bool) -> ResidualContract {
    if stderr_text.trim().is_empty() || has_renderable_residual {
        ResidualContract::BoundedRender
    } else {
        ResidualContract::FailOpen
    }
}

fn residual_outcome_for_contract(
    contract: ResidualContract,
    source_authority: SourceAuthority,
    fallback_grade: FallbackGrade,
    fallback_reason: Option<FallbackReason>,
) -> (FallbackGrade, Option<FallbackReason>) {
    if !matches!(source_authority, SourceAuthority::ResidualText) {
        return (fallback_grade, fallback_reason);
    }

    match contract {
        ResidualContract::BoundedRender => (fallback_grade, fallback_reason),
        ResidualContract::FailOpen => (
            FallbackGrade::FailOpen,
            fallback_reason.or(Some(FallbackReason::ResidualOnly)),
        ),
    }
}

fn confidence_ceiling_for(
    source_authority: SourceAuthority,
    fallback_grade: FallbackGrade,
) -> Confidence {
    match (source_authority, fallback_grade) {
        (SourceAuthority::Structured, FallbackGrade::None) => Confidence::Medium,
        (SourceAuthority::ResidualText, _) => Confidence::Low,
        (SourceAuthority::Structured, _) => Confidence::Low,
        (SourceAuthority::None, _) => Confidence::Unknown,
    }
}

pub(crate) fn compatibility_bundle_from_legacy_inputs(
    sarif_path: Option<&Path>,
    stderr_text: &str,
    run: &RunInfo,
) -> CaptureBundle {
    let has_sarif_path = sarif_path.is_some();
    let processing_path = if has_sarif_path {
        ProcessingPath::DualSinkStructured
    } else {
        ProcessingPath::Passthrough
    };
    let selected_mode = if has_sarif_path {
        ExecutionMode::Render
    } else {
        ExecutionMode::Passthrough
    };
    let primary_tool = run.primary_tool.clone();

    CaptureBundle {
        plan: CapturePlan {
            execution_mode: selected_mode,
            processing_path,
            structured_capture: if has_sarif_path {
                StructuredCapturePolicy::SarifFile
            } else {
                StructuredCapturePolicy::Disabled
            },
            native_text_capture: if has_sarif_path {
                NativeTextCapturePolicy::CaptureOnly
            } else {
                NativeTextCapturePolicy::Passthrough
            },
            preserve_native_color: false,
            locale_handling: LocaleHandling::Preserve,
            retention_policy: RetentionPolicy::Never,
        },
        invocation: CaptureInvocation {
            backend_path: run.primary_tool.name.clone(),
            argv: run.argv_redacted.clone(),
            argv_hash: diag_core::fingerprint_for(&run.argv_redacted),
            cwd: run.cwd_display.clone().unwrap_or_default(),
            selected_mode,
            processing_path,
        },
        raw_text_artifacts: if stderr_text.is_empty() {
            Vec::new()
        } else {
            vec![CaptureArtifact {
                id: "stderr.raw".to_string(),
                kind: ArtifactKind::CompilerStderrText,
                media_type: "text/plain".to_string(),
                encoding: Some("utf-8".to_string()),
                digest_sha256: None,
                size_bytes: Some(stderr_text.len() as u64),
                storage: ArtifactStorage::Inline,
                inline_text: Some(stderr_text.to_string()),
                external_ref: None,
                produced_by: Some(primary_tool.clone()),
            }]
        },
        structured_artifacts: sarif_path
            .map(|path| CaptureArtifact {
                id: "diagnostics.sarif".to_string(),
                kind: ArtifactKind::GccSarif,
                media_type: "application/sarif+json".to_string(),
                encoding: Some("utf-8".to_string()),
                digest_sha256: None,
                size_bytes: fs::metadata(path).ok().map(|metadata| metadata.len()),
                storage: if path.exists() {
                    ArtifactStorage::ExternalRef
                } else {
                    ArtifactStorage::Unavailable
                },
                inline_text: None,
                external_ref: if path.exists() {
                    Some(path.display().to_string())
                } else {
                    None
                },
                produced_by: Some(primary_tool),
            })
            .into_iter()
            .collect(),
        exit_status: ExitStatusInfo {
            code: Some(run.exit_status),
            signal: None,
            success: run.exit_status == 0,
        },
        integrity_issues: Vec::new(),
    }
}
