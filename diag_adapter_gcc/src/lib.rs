use diag_backend_probe::ProcessingPath;
use diag_capture_runtime::{
    CaptureBundle, CaptureInvocation, CapturePlan, ExecutionMode, ExitStatusInfo, LocaleHandling,
    NativeTextCapturePolicy, StructuredCapturePolicy,
};
use diag_core::{
    AnalysisOverlay, ArtifactKind, ArtifactStorage, CaptureArtifact, Confidence, ContextChain,
    ContextChainKind, DiagnosticDocument, DiagnosticNode, DocumentCompleteness, FallbackGrade,
    FallbackReason, FingerprintSet, IntegrityIssue, IssueSeverity, IssueStage, Location,
    MessageText, NodeCompleteness, Origin, Phase, ProducerInfo, Provenance, ProvenanceSource,
    RunInfo, SemanticRole, Severity, SourceAuthority, ToolInfo,
};
use diag_residual_text::classify;
use diag_rulepack::checked_in_rulepack_version;
use diag_trace::RetentionPolicy;
use serde_json::Value;
use std::fs;
use std::path::Path;

#[derive(Debug, thiserror::Error)]
pub enum AdapterError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("unsupported SARIF version: {0}")]
    UnsupportedVersion(String),
    #[error("missing runs array in SARIF payload")]
    MissingRuns,
}

#[derive(Debug)]
pub struct IngestOutcome {
    pub document: DiagnosticDocument,
    pub fallback_reason: Option<FallbackReason>,
}

#[derive(Debug, Clone)]
pub struct IngestPolicy {
    pub producer: ProducerInfo,
    pub run: RunInfo,
}

#[derive(Debug)]
pub struct IngestReport {
    pub document: DiagnosticDocument,
    pub source_authority: SourceAuthority,
    pub confidence_ceiling: Confidence,
    pub fallback_grade: FallbackGrade,
    pub warnings: Vec<IntegrityIssue>,
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

pub fn ingest(
    sarif_path: Option<&Path>,
    stderr_text: &str,
    producer: ProducerInfo,
    run: RunInfo,
) -> Result<DiagnosticDocument, AdapterError> {
    Ok(ingest_with_reason(sarif_path, stderr_text, producer, run)?.document)
}

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
            match from_sarif_artifact(artifact, policy.producer.clone(), policy.run.clone()) {
                Ok(document) => (
                    document,
                    SourceAuthority::Structured,
                    FallbackGrade::None,
                    None,
                ),
                Err(error) => (
                    failed_document(
                        policy.producer,
                        policy.run,
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
                policy.producer,
                policy.run,
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
            match from_gcc_json_artifact(artifact, policy.producer.clone(), policy.run.clone()) {
                Ok(document) => (
                    document,
                    SourceAuthority::Structured,
                    FallbackGrade::None,
                    None,
                ),
                Err(error) => (
                    failed_document(
                        policy.producer,
                        policy.run,
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
                policy.producer,
                policy.run,
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
            let mut document = passthrough_document(policy.producer, policy.run);
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
            passthrough_document(policy.producer, policy.run),
            source_authority_for_residual(stderr_text),
            fallback_grade_for_residual(stderr_text),
            None,
        ),
    };

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

fn compatibility_bundle_from_legacy_inputs(
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

pub fn from_sarif(
    sarif_path: &Path,
    producer: ProducerInfo,
    run: RunInfo,
) -> Result<DiagnosticDocument, AdapterError> {
    let json = fs::read_to_string(sarif_path)?;
    from_sarif_payload(&json, "diagnostics.sarif", producer, run)
}

fn from_sarif_artifact(
    artifact: &CaptureArtifact,
    producer: ProducerInfo,
    run: RunInfo,
) -> Result<DiagnosticDocument, AdapterError> {
    let json = read_structured_artifact_text(artifact)?;
    from_sarif_payload(&json, &artifact.id, producer, run)
}

fn from_sarif_payload(
    json: &str,
    capture_ref: &str,
    producer: ProducerInfo,
    run: RunInfo,
) -> Result<DiagnosticDocument, AdapterError> {
    let root: Value = serde_json::from_str(&json)?;
    let version = root
        .get("version")
        .and_then(Value::as_str)
        .unwrap_or("unknown")
        .to_string();
    if !version.starts_with("2.1") {
        return Err(AdapterError::UnsupportedVersion(version));
    }
    let runs = root
        .get("runs")
        .and_then(Value::as_array)
        .ok_or(AdapterError::MissingRuns)?;

    let mut document = DiagnosticDocument {
        document_id: format!("sarif-{}", run.invocation_id),
        schema_version: diag_core::IR_SPEC_VERSION.to_string(),
        document_completeness: DocumentCompleteness::Complete,
        producer,
        run,
        captures: Vec::new(),
        integrity_issues: Vec::new(),
        diagnostics: Vec::new(),
        fingerprints: None,
    };

    for (run_index, run_value) in runs.iter().enumerate() {
        let results = run_value
            .get("results")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        for (result_index, result) in results.iter().enumerate() {
            let node = result_to_node(run_index, result_index, result, capture_ref);
            document.diagnostics.push(node);
        }
    }

    if document.diagnostics.is_empty() {
        document.document_completeness = DocumentCompleteness::Partial;
        document.integrity_issues.push(IntegrityIssue {
            severity: IssueSeverity::Warning,
            stage: IssueStage::Parse,
            message: "SARIF contained no diagnostic results".to_string(),
            provenance: Some(Provenance {
                source: ProvenanceSource::Compiler,
                capture_refs: vec![capture_ref.to_string()],
            }),
        });
    }

    Ok(document)
}

fn read_structured_artifact_text(artifact: &CaptureArtifact) -> Result<String, AdapterError> {
    if let Some(text) = artifact.inline_text.as_ref() {
        return Ok(text.clone());
    }

    let path = artifact.external_ref.as_deref().ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::NotFound,
            format!(
                "structured artifact '{}' has no readable payload",
                artifact.id
            ),
        )
    })?;
    Ok(fs::read_to_string(path)?)
}

fn result_to_node(
    run_index: usize,
    result_index: usize,
    result: &Value,
    capture_ref: &str,
) -> DiagnosticNode {
    let raw_text = result
        .get("message")
        .and_then(|message| message.get("text").or_else(|| message.get("markdown")))
        .and_then(Value::as_str)
        .unwrap_or("compiler reported a diagnostic")
        .to_string();
    let related_messages = related_messages(result);
    let family_seed = combined_message_seed(&raw_text, &related_messages);
    let family_decision = classify_family_seed(&family_seed);
    let severity = match result.get("level").and_then(Value::as_str) {
        Some("error") => Severity::Error,
        Some("warning") => Severity::Warning,
        Some("note") => Severity::Note,
        Some("none") => Severity::Info,
        _ => Severity::Error,
    };
    let locations = parse_locations(result);
    let context_chains = parse_context_chains(result);
    let children = parse_related_locations(run_index, result_index, result, capture_ref);
    let completeness = if locations.is_empty() {
        NodeCompleteness::Partial
    } else {
        NodeCompleteness::Complete
    };

    DiagnosticNode {
        id: format!("sarif-{run_index}-{result_index}"),
        origin: Origin::Gcc,
        phase: infer_phase(&raw_text, &context_chains),
        severity,
        semantic_role: SemanticRole::Root,
        message: MessageText {
            raw_text: raw_text.clone(),
            normalized_text: None,
            locale: None,
        },
        locations,
        children,
        suggestions: Vec::new(),
        context_chains,
        symbol_context: None,
        node_completeness: completeness,
        provenance: Provenance {
            source: ProvenanceSource::Compiler,
            capture_refs: vec![capture_ref.to_string()],
        },
        analysis: Some(AnalysisOverlay {
            family: Some(family_decision.family.clone()),
            headline: Some(raw_text.lines().next().unwrap_or(&raw_text).to_string()),
            first_action_hint: Some(first_action_hint(family_decision.family.as_str())),
            confidence: Some(Confidence::Medium),
            rule_id: Some(family_decision.rule_id),
            matched_conditions: family_decision.matched_conditions,
            suppression_reason: family_decision.suppression_reason,
            collapsed_child_ids: Vec::new(),
            collapsed_chain_ids: Vec::new(),
        }),
        fingerprints: Some(FingerprintSet {
            raw: diag_core::fingerprint_for(&raw_text),
            structural: diag_core::fingerprint_for(&result),
            family: diag_core::fingerprint_for(&family_decision.family),
        }),
    }
}

fn parse_locations(result: &Value) -> Vec<Location> {
    let mut locations = Vec::new();
    let values = result
        .get("locations")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    for location in values {
        let physical = location
            .get("physicalLocation")
            .or_else(|| location.get("physical_location"));
        let path = physical
            .and_then(|physical| physical.get("artifactLocation"))
            .and_then(|artifact| artifact.get("uri"))
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        let region = physical
            .and_then(|physical| physical.get("region"))
            .cloned()
            .unwrap_or(Value::Null);
        if path.is_empty() {
            continue;
        }
        locations.push(Location {
            path,
            line: region.get("startLine").and_then(Value::as_u64).unwrap_or(1) as u32,
            column: region
                .get("startColumn")
                .and_then(Value::as_u64)
                .unwrap_or(1) as u32,
            end_line: region
                .get("endLine")
                .and_then(Value::as_u64)
                .map(|value| value as u32),
            end_column: region
                .get("endColumn")
                .and_then(Value::as_u64)
                .map(|value| value as u32),
            display_path: None,
            ownership: None,
        });
    }
    locations
}

fn parse_related_locations(
    run_index: usize,
    result_index: usize,
    result: &Value,
    capture_ref: &str,
) -> Vec<DiagnosticNode> {
    let related = result
        .get("relatedLocations")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    related
        .into_iter()
        .enumerate()
        .filter_map(|(index, location)| {
            let message = location
                .get("message")
                .and_then(|message| message.get("text"))
                .and_then(Value::as_str)
                .map(str::to_string)?;
            if message.trim().is_empty() || is_candidate_count_message(&message) {
                return None;
            }
            Some(DiagnosticNode {
                id: format!("sarif-{run_index}-{result_index}-related-{index}"),
                origin: Origin::Gcc,
                phase: infer_related_phase(&message),
                severity: Severity::Note,
                semantic_role: infer_related_role(&message),
                message: MessageText {
                    raw_text: message,
                    normalized_text: None,
                    locale: None,
                },
                locations: parse_locations(&serde_json::json!({ "locations": [location] })),
                children: Vec::new(),
                suggestions: Vec::new(),
                context_chains: Vec::new(),
                symbol_context: None,
                node_completeness: NodeCompleteness::Partial,
                provenance: Provenance {
                    source: ProvenanceSource::Compiler,
                    capture_refs: vec![capture_ref.to_string()],
                },
                analysis: None,
                fingerprints: None,
            })
        })
        .collect()
}

fn parse_context_chains(result: &Value) -> Vec<ContextChain> {
    let mut chains = Vec::new();
    if result.get("codeFlows").is_some() {
        chains.push(ContextChain {
            kind: ContextChainKind::AnalyzerPath,
            frames: Vec::new(),
        });
    }
    let message = result
        .get("message")
        .and_then(|message| message.get("text"))
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_lowercase();
    if message.contains("template") {
        chains.push(ContextChain {
            kind: ContextChainKind::TemplateInstantiation,
            frames: Vec::new(),
        });
    }
    if message.contains("macro") {
        chains.push(ContextChain {
            kind: ContextChainKind::MacroExpansion,
            frames: Vec::new(),
        });
    }
    if message.contains("include") {
        chains.push(ContextChain {
            kind: ContextChainKind::Include,
            frames: Vec::new(),
        });
    }
    for location in result
        .get("relatedLocations")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let related_message = location
            .get("message")
            .and_then(|message| message.get("text"))
            .and_then(Value::as_str)
            .unwrap_or_default();
        let frame = context_frame_from_related_location(related_message, location);
        let lowered = related_message.to_lowercase();
        if lowered.contains("template")
            || lowered.contains("deduction/substitution")
            || lowered.contains("deduced conflicting")
        {
            push_chain_frame(
                &mut chains,
                ContextChainKind::TemplateInstantiation,
                frame.clone(),
            );
        }
        if lowered.contains("macro") {
            push_chain_frame(&mut chains, ContextChainKind::MacroExpansion, frame.clone());
        }
        if lowered.contains("include") {
            push_chain_frame(&mut chains, ContextChainKind::Include, frame);
        }
    }
    chains
}

fn from_gcc_json_artifact(
    artifact: &CaptureArtifact,
    producer: ProducerInfo,
    run: RunInfo,
) -> Result<DiagnosticDocument, AdapterError> {
    let json = read_structured_artifact_text(artifact)?;
    from_gcc_json_payload(&json, &artifact.id, producer, run)
}

fn from_gcc_json_payload(
    json: &str,
    capture_ref: &str,
    producer: ProducerInfo,
    run: RunInfo,
) -> Result<DiagnosticDocument, AdapterError> {
    let diagnostics: Vec<Value> = serde_json::from_str(json)?;
    let mut document = DiagnosticDocument {
        document_id: format!("gcc-json-{}", run.invocation_id),
        schema_version: diag_core::IR_SPEC_VERSION.to_string(),
        document_completeness: DocumentCompleteness::Complete,
        producer,
        run,
        captures: Vec::new(),
        integrity_issues: Vec::new(),
        diagnostics: Vec::new(),
        fingerprints: None,
    };
    let mut has_partial_nodes = false;

    for (index, diagnostic) in diagnostics.iter().enumerate() {
        if !diagnostic.is_object() {
            has_partial_nodes = true;
            document.integrity_issues.push(IntegrityIssue {
                severity: IssueSeverity::Warning,
                stage: IssueStage::Parse,
                message: format!("GCC JSON diagnostic #{index} was not an object"),
                provenance: Some(Provenance {
                    source: ProvenanceSource::Compiler,
                    capture_refs: vec![capture_ref.to_string()],
                }),
            });
            continue;
        }

        let node =
            gcc_json_diagnostic_to_node(format!("json-{index}"), diagnostic, capture_ref, true);
        has_partial_nodes |= node_is_partial(&node);
        document.diagnostics.push(node);
    }

    if document.diagnostics.is_empty() {
        document.document_completeness = DocumentCompleteness::Partial;
        document.integrity_issues.push(IntegrityIssue {
            severity: IssueSeverity::Warning,
            stage: IssueStage::Parse,
            message: "GCC JSON contained no diagnostic entries".to_string(),
            provenance: Some(Provenance {
                source: ProvenanceSource::Compiler,
                capture_refs: vec![capture_ref.to_string()],
            }),
        });
    } else if has_partial_nodes {
        document.document_completeness = DocumentCompleteness::Partial;
    }

    Ok(document)
}

fn gcc_json_diagnostic_to_node(
    id: String,
    diagnostic: &Value,
    capture_ref: &str,
    is_root: bool,
) -> DiagnosticNode {
    let raw_text = json_message_text(diagnostic.get("message"))
        .unwrap_or_else(|| "compiler reported a diagnostic".to_string());
    let child_messages = json_child_messages(diagnostic);
    let family_seed = combined_message_seed(&raw_text, &child_messages);
    let family_decision = classify_family_seed(&family_seed);
    let locations = parse_gcc_json_locations(diagnostic);
    let children = parse_gcc_json_children(&id, diagnostic, capture_ref);
    let context_chains = parse_gcc_json_context_chains(&raw_text, &children);
    let completeness = if locations.is_empty() {
        NodeCompleteness::Partial
    } else {
        NodeCompleteness::Complete
    };
    let severity = gcc_json_severity(diagnostic.get("kind").and_then(Value::as_str));
    let semantic_role = if is_root {
        SemanticRole::Root
    } else {
        infer_related_role(&raw_text)
    };
    let phase = if is_root {
        infer_phase(&family_seed, &context_chains)
    } else {
        infer_related_phase(&raw_text)
    };

    DiagnosticNode {
        id,
        origin: Origin::Gcc,
        phase,
        severity,
        semantic_role,
        message: MessageText {
            raw_text: raw_text.clone(),
            normalized_text: None,
            locale: None,
        },
        locations,
        children,
        suggestions: Vec::new(),
        context_chains,
        symbol_context: None,
        node_completeness: completeness,
        provenance: Provenance {
            source: ProvenanceSource::Compiler,
            capture_refs: vec![capture_ref.to_string()],
        },
        analysis: is_root.then_some(AnalysisOverlay {
            family: Some(family_decision.family.clone()),
            headline: Some(raw_text.lines().next().unwrap_or(&raw_text).to_string()),
            first_action_hint: Some(first_action_hint(family_decision.family.as_str())),
            confidence: Some(Confidence::Medium),
            rule_id: Some(family_decision.rule_id),
            matched_conditions: family_decision.matched_conditions,
            suppression_reason: family_decision.suppression_reason,
            collapsed_child_ids: Vec::new(),
            collapsed_chain_ids: Vec::new(),
        }),
        fingerprints: is_root.then_some(FingerprintSet {
            raw: diag_core::fingerprint_for(&raw_text),
            structural: diag_core::fingerprint_for(diagnostic),
            family: diag_core::fingerprint_for(&family_decision.family),
        }),
    }
}

fn parse_gcc_json_locations(diagnostic: &Value) -> Vec<Location> {
    diagnostic
        .get("locations")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(gcc_json_location)
        .collect()
}

fn gcc_json_location(location: &Value) -> Option<Location> {
    let primary = location
        .get("caret")
        .or_else(|| location.get("start"))
        .or_else(|| location.get("finish"));
    let finish = location.get("finish");
    let path = primary
        .and_then(gcc_json_point_file)
        .or_else(|| finish.and_then(gcc_json_point_file))?;
    let line = primary
        .and_then(gcc_json_point_line)
        .or_else(|| finish.and_then(gcc_json_point_line))
        .unwrap_or(1);
    let column = primary
        .and_then(gcc_json_point_column)
        .or_else(|| finish.and_then(gcc_json_point_column))
        .unwrap_or(1);

    Some(Location {
        path,
        line,
        column,
        end_line: finish.and_then(gcc_json_point_line),
        end_column: finish.and_then(gcc_json_point_column),
        display_path: None,
        ownership: None,
    })
}

fn gcc_json_point_file(point: &Value) -> Option<String> {
    point
        .get("file")
        .and_then(Value::as_str)
        .map(ToString::to_string)
}

fn gcc_json_point_line(point: &Value) -> Option<u32> {
    point
        .get("line")
        .and_then(Value::as_u64)
        .map(|value| value as u32)
}

fn gcc_json_point_column(point: &Value) -> Option<u32> {
    point
        .get("column")
        .and_then(Value::as_u64)
        .or_else(|| point.get("display-column").and_then(Value::as_u64))
        .or_else(|| point.get("byte-column").and_then(Value::as_u64))
        .map(|value| value as u32)
}

fn parse_gcc_json_children(
    parent_id: &str,
    diagnostic: &Value,
    capture_ref: &str,
) -> Vec<DiagnosticNode> {
    diagnostic
        .get("children")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .enumerate()
        .filter(|(_, child)| child.is_object())
        .map(|(index, child)| {
            gcc_json_diagnostic_to_node(
                format!("{parent_id}-child-{index}"),
                child,
                capture_ref,
                false,
            )
        })
        .collect()
}

fn json_message_text(message: Option<&Value>) -> Option<String> {
    let message = message?;
    message.as_str().map(ToString::to_string).or_else(|| {
        message
            .get("text")
            .or_else(|| message.get("markdown"))
            .and_then(Value::as_str)
            .map(ToString::to_string)
    })
}

fn json_child_messages(diagnostic: &Value) -> Vec<String> {
    diagnostic
        .get("children")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|child| json_message_text(child.get("message")))
        .collect()
}

fn parse_gcc_json_context_chains(message: &str, children: &[DiagnosticNode]) -> Vec<ContextChain> {
    let mut chains = Vec::new();
    let lowered = message.to_lowercase();
    if lowered.contains("template") {
        chains.push(ContextChain {
            kind: ContextChainKind::TemplateInstantiation,
            frames: Vec::new(),
        });
    }
    if lowered.contains("macro") {
        chains.push(ContextChain {
            kind: ContextChainKind::MacroExpansion,
            frames: Vec::new(),
        });
    }
    if lowered.contains("include") {
        chains.push(ContextChain {
            kind: ContextChainKind::Include,
            frames: Vec::new(),
        });
    }

    for child in children {
        let frame = context_frame_from_node(child);
        let lowered = child.message.raw_text.to_lowercase();
        if lowered.contains("template")
            || lowered.contains("required from")
            || lowered.contains("deduction/substitution")
            || lowered.contains("deduced conflicting")
        {
            push_chain_frame(
                &mut chains,
                ContextChainKind::TemplateInstantiation,
                frame.clone(),
            );
        }
        if lowered.contains("macro") {
            push_chain_frame(&mut chains, ContextChainKind::MacroExpansion, frame.clone());
        }
        if lowered.contains("include")
            || child
                .message
                .raw_text
                .trim_start()
                .starts_with("In file included from ")
            || child.message.raw_text.trim_start().starts_with("from ")
        {
            push_chain_frame(&mut chains, ContextChainKind::Include, frame);
        }
    }

    chains
}

fn context_frame_from_node(node: &DiagnosticNode) -> diag_core::ContextFrame {
    let location = node.primary_location();
    diag_core::ContextFrame {
        label: node.message.raw_text.trim().to_string(),
        path: location.map(|location| location.path.clone()),
        line: location.map(|location| location.line),
        column: location.map(|location| location.column),
    }
}

fn gcc_json_severity(kind: Option<&str>) -> Severity {
    match kind.unwrap_or("error") {
        "fatal error" | "fatal" => Severity::Fatal,
        "warning" => Severity::Warning,
        "note" => Severity::Note,
        "remark" => Severity::Remark,
        "info" => Severity::Info,
        "error" => Severity::Error,
        _ => Severity::Error,
    }
}

fn node_is_partial(node: &DiagnosticNode) -> bool {
    matches!(node.node_completeness, NodeCompleteness::Partial)
        || node.children.iter().any(node_is_partial)
}

fn infer_phase(message: &str, context_chains: &[ContextChain]) -> Phase {
    let message = message.to_lowercase();
    if message.contains("undefined reference") || message.contains("multiple definition") {
        Phase::Link
    } else if context_chains
        .iter()
        .any(|chain| matches!(chain.kind, ContextChainKind::TemplateInstantiation))
    {
        Phase::Instantiate
    } else if message.contains("expected") || message.contains("before") {
        Phase::Parse
    } else {
        Phase::Semantic
    }
}

#[derive(Debug, Clone)]
struct AdapterFamilyDecision {
    family: String,
    rule_id: String,
    matched_conditions: Vec<String>,
    suppression_reason: Option<String>,
}

#[derive(Debug, Clone, Copy)]
struct AdapterFamilyRule {
    id: &'static str,
    family: &'static str,
    contains_any: &'static [&'static str],
}

const ADAPTER_FAMILY_RULES: &[AdapterFamilyRule] = &[
    AdapterFamilyRule {
        id: "rule.family_seed.linker.undefined_reference",
        family: "linker.undefined_reference",
        contains_any: &["undefined reference"],
    },
    AdapterFamilyRule {
        id: "rule.family_seed.linker.multiple_definition",
        family: "linker.multiple_definition",
        contains_any: &["multiple definition"],
    },
    AdapterFamilyRule {
        id: "rule.family_seed.template",
        family: "template",
        contains_any: &["template", "deduction/substitution", "deduced conflicting"],
    },
    AdapterFamilyRule {
        id: "rule.family_seed.macro_include",
        family: "macro_include",
        contains_any: &["macro", "include"],
    },
    AdapterFamilyRule {
        id: "rule.family_seed.type_overload",
        family: "type_overload",
        contains_any: &[
            "cannot convert",
            "no matching",
            "invalid conversion",
            "incompatible type",
            "passing argument",
        ],
    },
    AdapterFamilyRule {
        id: "rule.family_seed.syntax",
        family: "syntax",
        contains_any: &["expected", "before"],
    },
];

fn classify_family_seed(message: &str) -> AdapterFamilyDecision {
    let lowered = message.to_lowercase();
    for rule in ADAPTER_FAMILY_RULES {
        let matched_conditions = rule
            .contains_any
            .iter()
            .filter(|needle| lowered.contains(**needle))
            .map(|needle| format!("message_contains={needle}"))
            .collect::<Vec<_>>();
        if !matched_conditions.is_empty() {
            return AdapterFamilyDecision {
                family: rule.family.to_string(),
                rule_id: rule.id.to_string(),
                matched_conditions,
                suppression_reason: None,
            };
        }
    }
    AdapterFamilyDecision {
        family: "unknown".to_string(),
        rule_id: "rule.family_seed.unknown".to_string(),
        matched_conditions: vec!["no_seed_rule_matched".to_string()],
        suppression_reason: Some("generic_fallback".to_string()),
    }
}

fn first_action_hint(family: &str) -> String {
    match family {
        "syntax" => "fix the parse error at the first user-owned location".to_string(),
        "type_overload" => "compare the expected and actual types at the call site".to_string(),
        "template" => "start from the first user-owned template frame and match template arguments"
            .to_string(),
        "macro_include" => {
            "inspect the user-owned include edge or macro invocation that triggers the error"
                .to_string()
        }
        "linker.undefined_reference" => {
            "define the missing symbol or adjust link order/library inputs".to_string()
        }
        _ => "inspect the preserved compiler diagnostics for the first corrective step".to_string(),
    }
}

fn augment_context_chains_from_stderr(document: &mut DiagnosticDocument, stderr_text: &str) {
    let mut include_frames = Vec::new();
    let mut macro_frames = Vec::new();
    for line in stderr_text.lines() {
        let trimmed = line.trim_start();
        if let Some(frame) = parse_include_frame(trimmed) {
            include_frames.push(frame);
            continue;
        }
        if trimmed.contains("in expansion of macro") {
            macro_frames.push(diag_core::ContextFrame {
                label: trimmed.to_string(),
                path: parse_path_prefix(trimmed),
                line: parse_line_prefix(trimmed),
                column: parse_column_prefix(trimmed),
            });
        }
    }
    if let Some(lead) = document.diagnostics.first_mut() {
        if !include_frames.is_empty() {
            push_chain_frames(lead, ContextChainKind::Include, include_frames);
        }
        if !macro_frames.is_empty() {
            push_chain_frames(lead, ContextChainKind::MacroExpansion, macro_frames);
        }
    }
}

fn parse_include_frame(line: &str) -> Option<diag_core::ContextFrame> {
    let prefix = if let Some(value) = line.strip_prefix("In file included from ") {
        value
    } else {
        line.strip_prefix("from ")?
    };
    let (path, line_number) = split_path_line(prefix)?;
    Some(diag_core::ContextFrame {
        label: line.to_string(),
        path: Some(path.to_string()),
        line: Some(line_number),
        column: None,
    })
}

fn split_path_line(value: &str) -> Option<(&str, u32)> {
    let separator = value.rfind(':')?;
    let path = value[..separator].trim_end_matches(',').trim();
    let remainder = value[separator + 1..]
        .trim_end_matches(',')
        .trim_end_matches(':')
        .trim();
    Some((path, remainder.parse().ok()?))
}

fn parse_path_prefix(line: &str) -> Option<String> {
    let first = line.split(':').next()?;
    if first.is_empty() || first.contains(' ') {
        None
    } else {
        Some(first.to_string())
    }
}

fn parse_line_prefix(line: &str) -> Option<u32> {
    let mut parts = line.split(':');
    parts.next()?;
    parts.next()?.parse().ok()
}

fn parse_column_prefix(line: &str) -> Option<u32> {
    let mut parts = line.split(':');
    parts.next()?;
    parts.next()?;
    parts.next()?.parse().ok()
}

fn push_chain_frames(
    node: &mut DiagnosticNode,
    kind: ContextChainKind,
    mut frames: Vec<diag_core::ContextFrame>,
) {
    if let Some(existing) = node
        .context_chains
        .iter_mut()
        .find(|chain| chain.kind == kind)
    {
        existing.frames.append(&mut frames);
    } else {
        node.context_chains.push(ContextChain { kind, frames });
    }
}

fn related_messages(result: &Value) -> Vec<String> {
    result
        .get("relatedLocations")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|location| {
            location
                .get("message")
                .and_then(|message| message.get("text"))
                .and_then(Value::as_str)
                .map(ToString::to_string)
        })
        .collect()
}

fn combined_message_seed(raw_text: &str, related_messages: &[String]) -> String {
    let mut parts = vec![raw_text.to_string()];
    parts.extend(related_messages.iter().cloned());
    parts.join("\n")
}

fn infer_related_role(message: &str) -> SemanticRole {
    let lowered = message.to_lowercase();
    if lowered.contains("candidate:") || is_numbered_candidate_message(&lowered) {
        SemanticRole::Candidate
    } else if lowered.contains("template") || lowered.contains("required from") {
        SemanticRole::Supporting
    } else {
        SemanticRole::Supporting
    }
}

fn infer_related_phase(message: &str) -> Phase {
    let lowered = message.to_lowercase();
    if lowered.contains("template") || lowered.contains("deduction/substitution") {
        Phase::Instantiate
    } else {
        Phase::Semantic
    }
}

fn is_candidate_count_message(message: &str) -> bool {
    let lowered = message.trim().to_lowercase();
    if let Some(rest) = lowered.strip_prefix("there are ") {
        return rest.ends_with(" candidates");
    }
    lowered == "there is 1 candidate"
}

fn is_numbered_candidate_message(message: &str) -> bool {
    let Some(rest) = message.trim().strip_prefix("candidate ") else {
        return false;
    };
    let digit_len = rest.chars().take_while(|ch| ch.is_ascii_digit()).count();
    digit_len > 0 && rest[digit_len..].starts_with(':')
}

fn context_frame_from_related_location(message: &str, location: &Value) -> diag_core::ContextFrame {
    let physical = location
        .get("physicalLocation")
        .or_else(|| location.get("physical_location"));
    let region = physical
        .and_then(|physical| physical.get("region"))
        .cloned()
        .unwrap_or(Value::Null);
    diag_core::ContextFrame {
        label: message.trim().to_string(),
        path: physical
            .and_then(|physical| physical.get("artifactLocation"))
            .and_then(|artifact| artifact.get("uri"))
            .and_then(Value::as_str)
            .map(ToString::to_string),
        line: region
            .get("startLine")
            .and_then(Value::as_u64)
            .map(|value| value as u32),
        column: region
            .get("startColumn")
            .and_then(Value::as_u64)
            .map(|value| value as u32),
    }
}

fn push_chain_frame(
    chains: &mut Vec<ContextChain>,
    kind: ContextChainKind,
    frame: diag_core::ContextFrame,
) {
    if let Some(existing) = chains.iter_mut().find(|chain| chain.kind == kind) {
        existing.frames.push(frame);
    } else {
        chains.push(ContextChain {
            kind,
            frames: vec![frame],
        });
    }
}

fn passthrough_document(producer: ProducerInfo, run: RunInfo) -> DiagnosticDocument {
    DiagnosticDocument {
        document_id: format!("passthrough-{}", run.invocation_id),
        schema_version: diag_core::IR_SPEC_VERSION.to_string(),
        document_completeness: DocumentCompleteness::Passthrough,
        producer,
        run,
        captures: Vec::new(),
        integrity_issues: Vec::new(),
        diagnostics: Vec::new(),
        fingerprints: None,
    }
}

fn fallback_document(
    producer: ProducerInfo,
    run: RunInfo,
    completeness: DocumentCompleteness,
    stderr_text: &str,
    integrity_message: String,
    capture_ref: Option<&str>,
) -> DiagnosticDocument {
    let mut document = passthrough_document(producer, run);
    document.document_completeness = completeness;
    document.integrity_issues.push(IntegrityIssue {
        severity: IssueSeverity::Error,
        stage: IssueStage::Parse,
        message: integrity_message,
        provenance: capture_ref.map(|capture_ref| Provenance {
            source: ProvenanceSource::Compiler,
            capture_refs: vec![capture_ref.to_string()],
        }),
    });
    if !stderr_text.trim().is_empty() {
        document.diagnostics.push(passthrough_node(stderr_text));
    }
    document
}

fn failed_document(
    producer: ProducerInfo,
    run: RunInfo,
    stderr_text: &str,
    integrity_message: String,
    capture_ref: Option<&str>,
) -> DiagnosticDocument {
    fallback_document(
        producer,
        run,
        DocumentCompleteness::Failed,
        stderr_text,
        integrity_message,
        capture_ref,
    )
}

fn passthrough_node(stderr_text: &str) -> DiagnosticNode {
    DiagnosticNode {
        id: "passthrough-0".to_string(),
        origin: Origin::Wrapper,
        phase: Phase::Unknown,
        severity: Severity::Error,
        semantic_role: SemanticRole::Passthrough,
        message: MessageText {
            raw_text: stderr_text.to_string(),
            normalized_text: None,
            locale: None,
        },
        locations: Vec::new(),
        children: Vec::new(),
        suggestions: Vec::new(),
        context_chains: Vec::new(),
        symbol_context: None,
        node_completeness: NodeCompleteness::Passthrough,
        provenance: Provenance {
            source: ProvenanceSource::ResidualText,
            capture_refs: vec!["stderr.raw".to_string()],
        },
        analysis: Some(AnalysisOverlay {
            family: Some("passthrough".to_string()),
            headline: Some("showing conservative wrapper view".to_string()),
            first_action_hint: Some(
                "inspect the preserved raw diagnostics and rerun with --formed-debug-refs=capture_ref if needed"
                    .to_string(),
            ),
            confidence: Some(Confidence::Low),
            rule_id: Some("rule.family_seed.passthrough".to_string()),
            matched_conditions: vec!["semantic_role=passthrough".to_string()],
            suppression_reason: Some("generic_fallback".to_string()),
            collapsed_child_ids: Vec::new(),
            collapsed_chain_ids: Vec::new(),
        }),
        fingerprints: None,
    }
}

pub fn producer_for_version(version: &str) -> ProducerInfo {
    ProducerInfo {
        name: "gcc-formed".to_string(),
        version: version.to_string(),
        git_revision: option_env!("FORMED_GIT_COMMIT").map(ToString::to_string),
        build_profile: option_env!("FORMED_BUILD_PROFILE").map(ToString::to_string),
        rulepack_version: Some(checked_in_rulepack_version().to_string()),
    }
}

pub fn tool_for_backend(name: &str, version: Option<String>) -> ToolInfo {
    ToolInfo {
        name: name.to_string(),
        version,
        component: None,
        vendor: Some("GNU".to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use diag_core::{LanguageMode, RunInfo, WrapperSurface};

    fn base_run_info() -> RunInfo {
        RunInfo {
            invocation_id: "inv".to_string(),
            invoked_as: Some("gcc-formed".to_string()),
            argv_redacted: vec!["gcc".to_string()],
            cwd_display: None,
            exit_status: 1,
            primary_tool: tool_for_backend("gcc", Some("15.2.0".to_string())),
            secondary_tools: Vec::new(),
            language_mode: Some(LanguageMode::C),
            target_triple: None,
            wrapper_mode: Some(WrapperSurface::Terminal),
        }
    }

    #[test]
    fn producer_uses_checked_in_rulepack_version() {
        let producer = producer_for_version("0.1.0");
        assert_eq!(
            producer.rulepack_version.as_deref(),
            Some(checked_in_rulepack_version())
        );
    }

    #[test]
    fn parses_minimal_sarif() {
        let tempdir = tempfile::tempdir().unwrap();
        let path = tempdir.path().join("diag.sarif");
        fs::write(
            &path,
            r#"{
              "version":"2.1.0",
              "runs":[
                {
                  "results":[
                    {
                      "level":"error",
                      "message":{"text":"expected ';' before '}' token"},
                      "locations":[
                        {
                          "physicalLocation":{
                            "artifactLocation":{"uri":"src/main.c"},
                            "region":{"startLine":4,"startColumn":1}
                          }
                        }
                      ]
                    }
                  ]
                }
              ]
            }"#,
        )
        .unwrap();
        let document = from_sarif(&path, producer_for_version("0.1.0"), base_run_info()).unwrap();
        assert_eq!(document.diagnostics.len(), 1);
        assert_eq!(document.diagnostics[0].locations[0].path, "src/main.c");
    }

    #[test]
    fn parses_minimal_gcc_json() {
        let document = from_gcc_json_payload(
            r#"[
              {
                "kind":"error",
                "message":"expected ';' before '}' token",
                "locations":[
                  {
                    "caret":{"file":"src/main.c","line":4,"column":1},
                    "finish":{"file":"src/main.c","line":4,"column":4}
                  }
                ]
              }
            ]"#,
            "diagnostics.json",
            producer_for_version("0.1.0"),
            base_run_info(),
        )
        .unwrap();

        assert_eq!(document.diagnostics.len(), 1);
        assert_eq!(document.diagnostics[0].locations[0].path, "src/main.c");
        assert_eq!(document.diagnostics[0].locations[0].end_column, Some(4));
        assert_eq!(
            document.diagnostics[0].provenance.capture_refs,
            vec!["diagnostics.json".to_string()]
        );
    }

    #[test]
    fn ignores_message_less_related_locations() {
        let tempdir = tempfile::tempdir().unwrap();
        let path = tempdir.path().join("diag.sarif");
        fs::write(
            &path,
            r#"{
              "version":"2.1.0",
              "runs":[
                {
                  "results":[
                    {
                      "level":"error",
                      "message":{"text":"'missing_symbol' undeclared"},
                      "locations":[
                        {
                          "physicalLocation":{
                            "artifactLocation":{"uri":"src/main.c"},
                            "region":{"startLine":3,"startColumn":25}
                          }
                        }
                      ],
                      "relatedLocations":[
                        {
                          "physicalLocation":{
                            "artifactLocation":{"uri":"src/main.c"},
                            "region":{"startLine":3,"startColumn":25}
                          },
                          "message":{"text":"each undeclared identifier is reported only once"}
                        },
                        {
                          "physicalLocation":{
                            "artifactLocation":{"uri":"src/wrapper.h"},
                            "region":{"startLine":1}
                          }
                        },
                        {
                          "physicalLocation":{
                            "artifactLocation":{"uri":"src/main.c"},
                            "region":{"startLine":1}
                          }
                        }
                      ]
                    }
                  ]
                }
              ]
            }"#,
        )
        .unwrap();
        let document = from_sarif(&path, producer_for_version("0.1.0"), base_run_info()).unwrap();

        assert_eq!(document.diagnostics.len(), 1);
        assert_eq!(document.diagnostics[0].children.len(), 1);
        assert_eq!(
            document.diagnostics[0].children[0].message.raw_text,
            "each undeclared identifier is reported only once"
        );
    }

    #[test]
    fn ignores_candidate_count_related_locations() {
        let tempdir = tempfile::tempdir().unwrap();
        let path = tempdir.path().join("diag.sarif");
        fs::write(
            &path,
            r#"{
              "version":"2.1.0",
              "runs":[
                {
                  "results":[
                    {
                      "level":"error",
                      "message":{"text":"no matching function for call to 'takes(int)'"},
                      "locations":[
                        {
                          "physicalLocation":{
                            "artifactLocation":{"uri":"src/main.cpp"},
                            "region":{"startLine":5,"startColumn":5}
                          }
                        }
                      ],
                      "relatedLocations":[
                        {
                          "physicalLocation":{
                            "artifactLocation":{"uri":"src/main.cpp"},
                            "region":{"startLine":5,"startColumn":5}
                          },
                          "message":{"text":"there are 2 candidates"}
                        },
                        {
                          "physicalLocation":{
                            "artifactLocation":{"uri":"src/main.cpp"},
                            "region":{"startLine":1,"startColumn":6}
                          },
                          "message":{"text":"candidate 1: 'void takes(int, int)'"}
                        }
                      ]
                    }
                  ]
                }
              ]
            }"#,
        )
        .unwrap();
        let document = from_sarif(
            &path,
            producer_for_version("0.1.0"),
            RunInfo {
                argv_redacted: vec!["g++".to_string()],
                primary_tool: tool_for_backend("g++", Some("15.2.0".to_string())),
                language_mode: Some(LanguageMode::Cpp),
                ..base_run_info()
            },
        )
        .unwrap();

        assert_eq!(document.diagnostics[0].children.len(), 1);
        assert_eq!(
            document.diagnostics[0].children[0].message.raw_text,
            "candidate 1: 'void takes(int, int)'"
        );
    }

    #[test]
    fn parses_gcc_json_children_as_structured_notes() {
        let document = from_gcc_json_payload(
            r#"[
              {
                "kind":"error",
                "message":"no matching function for call to 'takes(int)'",
                "locations":[
                  {
                    "caret":{"file":"src/main.cpp","line":5,"column":5}
                  }
                ],
                "children":[
                  {
                    "kind":"note",
                    "message":"candidate 1: 'void takes(int, int)'",
                    "locations":[
                      {
                        "caret":{"file":"src/main.cpp","line":1,"column":6}
                      }
                    ]
                  }
                ]
              }
            ]"#,
            "diagnostics.json",
            producer_for_version("0.1.0"),
            RunInfo {
                argv_redacted: vec!["g++".to_string()],
                primary_tool: tool_for_backend("g++", Some("12.3.0".to_string())),
                language_mode: Some(LanguageMode::Cpp),
                ..base_run_info()
            },
        )
        .unwrap();

        assert_eq!(document.diagnostics[0].children.len(), 1);
        assert_eq!(
            document.diagnostics[0].children[0].semantic_role,
            SemanticRole::Candidate
        );
        assert_eq!(
            document.diagnostics[0]
                .analysis
                .as_ref()
                .and_then(|analysis| analysis.family.as_deref()),
            Some("type_overload")
        );
    }

    #[test]
    fn fail_opens_when_authoritative_sarif_is_missing() {
        let tempdir = tempfile::tempdir().unwrap();
        let path = tempdir.path().join("missing.sarif");
        let outcome = ingest_with_reason(
            Some(&path),
            "src/main.c:4:1: error: expected ';' before '}' token\n",
            producer_for_version("0.1.0"),
            base_run_info(),
        )
        .unwrap();

        assert_eq!(outcome.fallback_reason, Some(FallbackReason::SarifMissing));
        assert_eq!(
            outcome.document.document_completeness,
            DocumentCompleteness::Partial
        );
        assert!(outcome.document.diagnostics.iter().any(|node| {
            matches!(node.semantic_role, SemanticRole::Root)
                && node
                    .message
                    .raw_text
                    .contains("expected ';' before '}' token")
                && node.primary_location().is_some()
        }));
        assert!(
            outcome.document.integrity_issues[0]
                .message
                .contains("authoritative SARIF was not produced")
        );
    }

    #[test]
    fn fail_opens_when_authoritative_sarif_is_invalid() {
        let tempdir = tempfile::tempdir().unwrap();
        let path = tempdir.path().join("diag.sarif");
        fs::write(&path, "{\"version\":").unwrap();
        let outcome = ingest_with_reason(
            Some(&path),
            "src/main.c:4:1: error: expected ';' before '}' token\n",
            producer_for_version("0.1.0"),
            base_run_info(),
        )
        .unwrap();

        assert_eq!(
            outcome.fallback_reason,
            Some(FallbackReason::SarifParseFailed)
        );
        assert_eq!(
            outcome.document.document_completeness,
            DocumentCompleteness::Failed
        );
        assert_eq!(outcome.document.diagnostics.len(), 1);
        assert!(
            outcome.document.integrity_issues[0]
                .message
                .contains("failed to parse authoritative SARIF")
        );
    }

    #[test]
    fn ingest_bundle_reports_structured_authority_for_valid_sarif() {
        let tempdir = tempfile::tempdir().unwrap();
        let path = tempdir.path().join("diag.sarif");
        fs::write(
            &path,
            r#"{
              "version":"2.1.0",
              "runs":[
                {
                  "results":[
                    {
                      "level":"error",
                      "message":{"text":"expected ';' before '}' token"}
                    }
                  ]
                }
              ]
            }"#,
        )
        .unwrap();
        let run = base_run_info();
        let report = ingest_bundle(
            &compatibility_bundle_from_legacy_inputs(
                Some(&path),
                "src/main.c:4:1: error: expected ';' before '}' token\n",
                &run,
            ),
            IngestPolicy {
                producer: producer_for_version("0.1.0"),
                run,
            },
        )
        .unwrap();

        assert_eq!(report.source_authority, SourceAuthority::Structured);
        assert_eq!(report.fallback_grade, FallbackGrade::None);
        assert_eq!(report.confidence_ceiling, Confidence::Medium);
        assert!(report.fallback_reason.is_none());
        assert!(report.warnings.is_empty());
        assert_eq!(report.document.diagnostics.len(), 1);
    }

    #[test]
    fn ingest_bundle_reports_structured_authority_for_valid_gcc_json() {
        let tempdir = tempfile::tempdir().unwrap();
        let path = tempdir.path().join("diagnostics.json");
        fs::write(
            &path,
            r#"[
              {
                "kind":"error",
                "message":"expected ';' before '}' token",
                "locations":[
                  {
                    "caret":{"file":"src/main.c","line":4,"column":1}
                  }
                ]
              }
            ]"#,
        )
        .unwrap();
        let run = base_run_info();
        let mut bundle = compatibility_bundle_from_legacy_inputs(None, "", &run);
        bundle.structured_artifacts.push(CaptureArtifact {
            id: "diagnostics.json".to_string(),
            kind: ArtifactKind::GccJson,
            media_type: "application/json".to_string(),
            encoding: Some("utf-8".to_string()),
            digest_sha256: None,
            size_bytes: None,
            storage: ArtifactStorage::ExternalRef,
            inline_text: None,
            external_ref: Some(path.display().to_string()),
            produced_by: Some(run.primary_tool.clone()),
        });

        let report = ingest_bundle(
            &bundle,
            IngestPolicy {
                producer: producer_for_version("0.1.0"),
                run,
            },
        )
        .unwrap();

        assert_eq!(report.source_authority, SourceAuthority::Structured);
        assert_eq!(report.fallback_grade, FallbackGrade::None);
        assert_eq!(report.confidence_ceiling, Confidence::Medium);
        assert!(report.fallback_reason.is_none());
        assert!(report.warnings.is_empty());
        assert_eq!(
            report.document.document_completeness,
            DocumentCompleteness::Complete
        );
        assert_eq!(report.document.diagnostics.len(), 1);
        assert_eq!(
            report.document.diagnostics[0].provenance.capture_refs,
            vec!["diagnostics.json".to_string()]
        );
    }

    #[test]
    fn ingest_bundle_accepts_residual_only_path() {
        let run = base_run_info();
        let report = ingest_bundle(
            &compatibility_bundle_from_legacy_inputs(
                None,
                "src/main.c:4:1: error: expected ';' before '}' token\n",
                &run,
            ),
            IngestPolicy {
                producer: producer_for_version("0.1.0"),
                run,
            },
        )
        .unwrap();

        assert_eq!(report.source_authority, SourceAuthority::ResidualText);
        assert_eq!(report.fallback_grade, FallbackGrade::Compatibility);
        assert_eq!(report.confidence_ceiling, Confidence::Low);
        assert!(report.fallback_reason.is_none());
        assert_eq!(
            report.document.document_completeness,
            DocumentCompleteness::Partial
        );
        assert!(report.document.diagnostics.iter().any(|node| {
            matches!(node.semantic_role, SemanticRole::Root)
                && node
                    .message
                    .raw_text
                    .contains("expected ';' before '}' token")
                && node.primary_location().is_some()
        }));
    }

    #[test]
    fn ingest_bundle_recognizes_type_overload_residual_useful_subset() {
        let run = base_run_info();
        let stderr = "\
src/main.cpp:5:7: error: no matching function for call to 'takes(int)'\n\
src/main.cpp:2:6: note: candidate 1: 'void takes(int, int)'\n";
        let report = ingest_bundle(
            &compatibility_bundle_from_legacy_inputs(None, stderr, &run),
            IngestPolicy {
                producer: producer_for_version("0.1.0"),
                run,
            },
        )
        .unwrap();

        assert_eq!(report.source_authority, SourceAuthority::ResidualText);
        assert_eq!(report.fallback_grade, FallbackGrade::Compatibility);
        assert_eq!(report.confidence_ceiling, Confidence::Low);
        assert_eq!(report.fallback_reason, None);
        assert_eq!(
            report.document.document_completeness,
            DocumentCompleteness::Partial
        );
        assert_eq!(report.document.diagnostics.len(), 1);
        assert_eq!(
            report.document.diagnostics[0]
                .analysis
                .as_ref()
                .and_then(|analysis| analysis.family.as_deref()),
            Some("type_overload")
        );
        assert!(
            report.document.diagnostics[0]
                .children
                .iter()
                .any(|child| matches!(child.semantic_role, SemanticRole::Candidate))
        );
    }

    #[test]
    fn ingest_bundle_recognizes_template_residual_useful_subset() {
        let run = base_run_info();
        let stderr = "\
src/main.cpp:8:15: error: no matching function for call to 'expect_ptr(int&)'\n\
src/main.cpp:3:7: note: template argument deduction/substitution failed:\n\
src/main.cpp:8:15: note:   required from here\n";
        let report = ingest_bundle(
            &compatibility_bundle_from_legacy_inputs(None, stderr, &run),
            IngestPolicy {
                producer: producer_for_version("0.1.0"),
                run,
            },
        )
        .unwrap();

        assert_eq!(report.source_authority, SourceAuthority::ResidualText);
        assert_eq!(report.fallback_grade, FallbackGrade::Compatibility);
        assert_eq!(report.confidence_ceiling, Confidence::Low);
        assert_eq!(report.fallback_reason, None);
        assert_eq!(
            report.document.document_completeness,
            DocumentCompleteness::Partial
        );
        assert_eq!(
            report.document.diagnostics[0]
                .analysis
                .as_ref()
                .and_then(|analysis| analysis.family.as_deref()),
            Some("template")
        );
        assert!(
            report.document.diagnostics[0]
                .context_chains
                .iter()
                .any(|chain| matches!(chain.kind, ContextChainKind::TemplateInstantiation))
        );
    }

    #[test]
    fn ingest_bundle_recognizes_linker_residual_useful_subset() {
        let run = base_run_info();
        let stderr = "\
/usr/bin/ld: main.o: in function `main':\n\
main.c:(.text+0x15): undefined reference to `foo`\n";
        let report = ingest_bundle(
            &compatibility_bundle_from_legacy_inputs(None, stderr, &run),
            IngestPolicy {
                producer: producer_for_version("0.1.0"),
                run,
            },
        )
        .unwrap();

        assert_eq!(report.source_authority, SourceAuthority::ResidualText);
        assert_eq!(report.fallback_grade, FallbackGrade::Compatibility);
        assert_eq!(report.confidence_ceiling, Confidence::Low);
        assert_eq!(report.fallback_reason, None);
        assert_eq!(
            report.document.document_completeness,
            DocumentCompleteness::Partial
        );
        assert_eq!(
            report.document.diagnostics[0]
                .analysis
                .as_ref()
                .and_then(|analysis| analysis.family.as_deref()),
            Some("linker.undefined_reference")
        );
    }

    #[test]
    fn ingest_bundle_keeps_unclassified_residuals_on_passthrough_document() {
        let run = base_run_info();
        let report = ingest_bundle(
            &compatibility_bundle_from_legacy_inputs(
                None,
                "totally unstructured compiler output\n",
                &run,
            ),
            IngestPolicy {
                producer: producer_for_version("0.1.0"),
                run,
            },
        )
        .unwrap();

        assert_eq!(
            report.document.document_completeness,
            DocumentCompleteness::Passthrough
        );
        assert!(report.document.diagnostics.iter().any(|node| {
            matches!(node.semantic_role, SemanticRole::Passthrough)
                && node
                    .message
                    .raw_text
                    .contains("totally unstructured compiler output")
        }));
        assert_eq!(report.fallback_grade, FallbackGrade::FailOpen);
        assert_eq!(report.fallback_reason, Some(FallbackReason::ResidualOnly));
    }

    #[test]
    fn ingest_bundle_fail_opens_on_opaque_compiler_residuals() {
        let run = base_run_info();
        let stderr = "\
src/main.c:4:1: error: opaque compiler wording here\n\
src/main.c:4:1: note: extra opaque detail\n";
        let report = ingest_bundle(
            &compatibility_bundle_from_legacy_inputs(None, stderr, &run),
            IngestPolicy {
                producer: producer_for_version("0.1.0"),
                run,
            },
        )
        .unwrap();

        assert_eq!(report.source_authority, SourceAuthority::ResidualText);
        assert_eq!(report.fallback_grade, FallbackGrade::FailOpen);
        assert_eq!(report.confidence_ceiling, Confidence::Low);
        assert_eq!(report.fallback_reason, Some(FallbackReason::ResidualOnly));
        assert_eq!(
            report.document.document_completeness,
            DocumentCompleteness::Passthrough
        );
        assert!(report.document.diagnostics.iter().any(|node| {
            matches!(node.semantic_role, SemanticRole::Passthrough)
                && node
                    .message
                    .raw_text
                    .contains("opaque compiler wording here")
        }));
    }

    #[test]
    fn ingest_bundle_marks_partial_for_incomplete_gcc_json() {
        let run = base_run_info();
        let tempdir = tempfile::tempdir().unwrap();
        let path = tempdir.path().join("diagnostics.json");
        fs::write(
            &path,
            r#"[
              {
                "kind":"error",
                "message":"expected ';' before '}' token"
              }
            ]"#,
        )
        .unwrap();
        let mut bundle = compatibility_bundle_from_legacy_inputs(None, "", &run);
        bundle.structured_artifacts.push(CaptureArtifact {
            id: "diagnostics.json".to_string(),
            kind: ArtifactKind::GccJson,
            media_type: "application/json".to_string(),
            encoding: Some("utf-8".to_string()),
            digest_sha256: None,
            size_bytes: None,
            storage: ArtifactStorage::ExternalRef,
            inline_text: None,
            external_ref: Some(path.display().to_string()),
            produced_by: Some(run.primary_tool.clone()),
        });

        let report = ingest_bundle(
            &bundle,
            IngestPolicy {
                producer: producer_for_version("0.1.0"),
                run,
            },
        )
        .unwrap();

        assert_eq!(report.source_authority, SourceAuthority::Structured);
        assert_eq!(report.fallback_grade, FallbackGrade::None);
        assert_eq!(report.confidence_ceiling, Confidence::Medium);
        assert!(report.fallback_reason.is_none());
        assert_eq!(
            report.document.document_completeness,
            DocumentCompleteness::Partial
        );
        assert_eq!(
            report.document.diagnostics[0].node_completeness,
            NodeCompleteness::Partial
        );
    }

    #[test]
    fn ingest_bundle_fail_opens_on_invalid_gcc_json() {
        let run = base_run_info();
        let tempdir = tempfile::tempdir().unwrap();
        let path = tempdir.path().join("diagnostics.json");
        fs::write(&path, "[").unwrap();
        let stderr = "src/main.c:4:1: error: expected ';' before '}' token\n";
        let mut bundle = compatibility_bundle_from_legacy_inputs(None, stderr, &run);
        bundle.structured_artifacts.push(CaptureArtifact {
            id: "diagnostics.json".to_string(),
            kind: ArtifactKind::GccJson,
            media_type: "application/json".to_string(),
            encoding: Some("utf-8".to_string()),
            digest_sha256: None,
            size_bytes: None,
            storage: ArtifactStorage::ExternalRef,
            inline_text: None,
            external_ref: Some(path.display().to_string()),
            produced_by: Some(run.primary_tool.clone()),
        });

        let report = ingest_bundle(
            &bundle,
            IngestPolicy {
                producer: producer_for_version("0.1.0"),
                run,
            },
        )
        .unwrap();

        assert_eq!(report.source_authority, SourceAuthority::ResidualText);
        assert_eq!(report.fallback_grade, FallbackGrade::FailOpen);
        assert_eq!(report.confidence_ceiling, Confidence::Low);
        assert!(report.fallback_reason.is_none());
        assert_eq!(
            report.document.document_completeness,
            DocumentCompleteness::Failed
        );
        assert!(report.document.integrity_issues.iter().any(|issue| {
            issue
                .message
                .contains("failed to parse structured GCC JSON")
        }));
        assert!(report.document.diagnostics.iter().any(|node| {
            node.message
                .raw_text
                .contains("expected ';' before '}' token")
        }));
    }

    #[test]
    fn ingest_with_reason_matches_bundle_report_for_missing_sarif() {
        let tempdir = tempfile::tempdir().unwrap();
        let path = tempdir.path().join("missing.sarif");
        let stderr = "src/main.c:4:1: error: expected ';' before '}' token\n";
        let run = base_run_info();
        let report = ingest_bundle(
            &compatibility_bundle_from_legacy_inputs(Some(&path), stderr, &run),
            IngestPolicy {
                producer: producer_for_version("0.1.0"),
                run: run.clone(),
            },
        )
        .unwrap();
        let outcome =
            ingest_with_reason(Some(&path), stderr, producer_for_version("0.1.0"), run).unwrap();

        assert_eq!(outcome.fallback_reason, report.fallback_reason);
        assert_eq!(
            outcome.document.document_completeness,
            report.document.document_completeness
        );
        assert_eq!(
            outcome.document.diagnostics.len(),
            report.document.diagnostics.len()
        );
        assert_eq!(
            outcome.document.integrity_issues,
            report.document.integrity_issues
        );
    }
}
