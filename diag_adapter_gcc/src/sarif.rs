//! SARIF diagnostic parsing.

use crate::classify::{
    classify_family_seed, combined_message_seed, first_action_hint, infer_phase,
    infer_related_phase, infer_related_role, is_candidate_count_message, related_messages,
};
use crate::ingest::AdapterError;
use crate::{json_str, json_u64};
use diag_core::{
    AnalysisOverlay, CaptureArtifact, Confidence, ContextChain, ContextChainKind,
    DiagnosticDocument, DiagnosticNode, DocumentCompleteness, FingerprintSet, IntegrityIssue,
    IssueSeverity, IssueStage, Location, MessageText, NodeCompleteness, Origin, ProducerInfo,
    Provenance, ProvenanceSource, RunInfo, SemanticRole, Severity,
};
use serde_json::Value;
use std::fs;
use std::path::Path;

/// Parse a SARIF file on disk and return a [`DiagnosticDocument`].
///
/// Reads the file at `sarif_path`, validates the SARIF version, and
/// converts each run result into a [`DiagnosticNode`].
pub fn from_sarif(
    sarif_path: &Path,
    producer: ProducerInfo,
    run: RunInfo,
) -> Result<DiagnosticDocument, AdapterError> {
    let json = fs::read_to_string(sarif_path)?;
    from_sarif_payload(&json, "diagnostics.sarif", &producer, &run)
}

pub(crate) fn from_sarif_artifact(
    artifact: &CaptureArtifact,
    producer: &ProducerInfo,
    run: &RunInfo,
) -> Result<DiagnosticDocument, AdapterError> {
    let json = read_structured_artifact_text(artifact)?;
    from_sarif_payload(&json, &artifact.id, producer, run)
}

fn from_sarif_payload(
    json: &str,
    capture_ref: &str,
    producer: &ProducerInfo,
    run: &RunInfo,
) -> Result<DiagnosticDocument, AdapterError> {
    let root: Value = serde_json::from_str(json)?;
    let version_str = json_str(&root, "version");
    let version = if version_str.is_empty() {
        "unknown".to_string()
    } else {
        version_str.to_string()
    };
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
        producer: producer.clone(),
        run: run.clone(),
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

pub(crate) fn read_structured_artifact_text(
    artifact: &CaptureArtifact,
) -> Result<String, AdapterError> {
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
    let related = related_messages(result);
    let family_seed = combined_message_seed(&raw_text, &related);
    let family_decision = classify_family_seed(&family_seed);
    let severity = match json_str(result, "level") {
        "error" => Severity::Error,
        "warning" => Severity::Warning,
        "note" => Severity::Note,
        "none" => Severity::Info,
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
            family: Some(family_decision.family.clone().into()),
            family_version: None,
            family_confidence: None,
            root_cause_score: None,
            actionability_score: None,
            user_code_priority: None,
            headline: Some(raw_text.lines().next().unwrap_or(&raw_text).to_string().into()),
            first_action_hint: Some(first_action_hint(family_decision.family.as_str()).into()),
            confidence: Some(Confidence::Medium.score()),
            preferred_primary_location_id: None,
            rule_id: Some(family_decision.rule_id.into()),
            matched_conditions: family_decision.matched_conditions.into_iter().map(Into::into).collect(),
            suppression_reason: family_decision.suppression_reason,
            collapsed_child_ids: Vec::new(),
            collapsed_chain_ids: Vec::new(),
            group_ref: None,
            reasons: Vec::new(),
            policy_profile: None,
            producer_version: None,
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
        let start_line = json_u64(&region, "startLine").unwrap_or(1) as u32;
        let start_column = json_u64(&region, "startColumn").unwrap_or(1) as u32;
        let mut parsed = Location::caret(
            path,
            start_line,
            start_column,
            diag_core::LocationRole::Primary,
        );
        if let (Some(end_line), Some(end_column)) = (
            json_u64(&region, "endLine").map(|value| value as u32),
            json_u64(&region, "endColumn").map(|value| value as u32),
        ) {
            parsed =
                parsed.with_range_end(end_line, end_column, diag_core::BoundarySemantics::Unknown);
        }
        locations.push(parsed);
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
        line: json_u64(&region, "startLine").map(|value| value as u32),
        column: json_u64(&region, "startColumn").map(|value| value as u32),
    }
}

pub(crate) fn push_chain_frame(
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
