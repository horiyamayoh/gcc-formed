//! SARIF diagnostic parsing.

use crate::classify::{
    classify_family_seed, combined_message_seed, first_action_hint, infer_phase,
    infer_related_phase, infer_related_role, is_candidate_count_message, related_messages,
    structured_message_text,
};
use crate::context::{extend_unique_context_kinds, metadata_context_kinds, text_context_kinds};
use crate::fixits::suggestion_from_edits;
use crate::ingest::AdapterError;
use crate::{is_valid_text_edit, json_str, json_u32, text_edits_overlap};
use diag_core::{
    AnalysisOverlay, CaptureArtifact, Confidence, ContextChain, ContextChainKind,
    DiagnosticDocument, DiagnosticNode, DocumentCompleteness, FingerprintSet, IntegrityIssue,
    IssueSeverity, IssueStage, Location, MessageText, NodeCompleteness, Origin, ProducerInfo,
    Provenance, ProvenanceSource, RunInfo, SemanticRole, Severity, Suggestion,
    SuggestionApplicability, TextEdit,
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
    let raw_text = structured_message_text(result.get("message"))
        .unwrap_or_else(|| "compiler reported a diagnostic".to_string());
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
    let suggestions = parse_suggestions(result);
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
        phase: infer_phase(&family_seed, &context_chains),
        severity,
        semantic_role: SemanticRole::Root,
        message: MessageText {
            raw_text: raw_text.clone(),
            normalized_text: None,
            locale: None,
        },
        locations,
        children,
        suggestions,
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
            headline: Some(
                raw_text
                    .lines()
                    .next()
                    .unwrap_or(&raw_text)
                    .to_string()
                    .into(),
            ),
            first_action_hint: Some(first_action_hint(family_decision.family.as_str()).into()),
            confidence: Some(Confidence::Medium.score()),
            preferred_primary_location_id: None,
            rule_id: Some(family_decision.rule_id.into()),
            matched_conditions: family_decision
                .matched_conditions
                .into_iter()
                .map(Into::into)
                .collect(),
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
        let start_line = json_u32(&region, "startLine").unwrap_or(1);
        let start_column = json_u32(&region, "startColumn").unwrap_or(1);
        let mut parsed = Location::caret(
            path,
            start_line,
            start_column,
            diag_core::LocationRole::Primary,
        );
        if let (Some(end_line), Some(end_column)) =
            (json_u32(&region, "endLine"), json_u32(&region, "endColumn"))
        {
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
            let message = structured_message_text(location.get("message"))?;
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

fn parse_suggestions(result: &Value) -> Vec<Suggestion> {
    result
        .get("fixes")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .enumerate()
        .filter_map(|(index, fix)| parse_suggestion(index, fix))
        .collect()
}

fn parse_suggestion(index: usize, fix: &Value) -> Option<Suggestion> {
    let edits = parse_suggestion_edits(fix)?;
    let description =
        structured_message_text(fix.get("description")).filter(|text| !text.is_empty());
    let applicability = if edits
        .iter()
        .map(|edit| edit.path.as_str())
        .collect::<std::collections::BTreeSet<_>>()
        .len()
        > 1
    {
        SuggestionApplicability::Manual
    } else if description.is_some() {
        SuggestionApplicability::MaybeIncorrect
    } else {
        SuggestionApplicability::MachineApplicable
    };

    suggestion_from_edits(
        description.or_else(|| {
            if edits.len() > 1 {
                Some(format!("apply compiler fix-it #{}", index + 1))
            } else {
                None
            }
        }),
        applicability,
        edits,
    )
}

fn parse_suggestion_edits(fix: &Value) -> Option<Vec<TextEdit>> {
    let artifact_changes = fix.get("artifactChanges")?.as_array()?;
    if artifact_changes.is_empty() {
        return None;
    }

    let mut edits = Vec::new();
    for change in artifact_changes {
        let path = change
            .get("artifactLocation")
            .and_then(|artifact| artifact.get("uri"))
            .and_then(Value::as_str)
            .filter(|path| !path.is_empty())?
            .to_string();
        let replacements = change.get("replacements")?.as_array()?;
        if replacements.is_empty() {
            return None;
        }

        for replacement in replacements {
            edits.push(parse_replacement(&path, replacement)?);
        }
    }

    if edits.is_empty() || text_edits_overlap(&edits) {
        return None;
    }

    Some(edits)
}

fn parse_replacement(path: &str, replacement: &Value) -> Option<TextEdit> {
    let deleted_region = replacement.get("deletedRegion")?;
    let inserted_content = replacement.get("insertedContent");
    let replacement_text = match inserted_content {
        None => String::new(),
        Some(Value::Object(_)) => inserted_content
            .and_then(|content| content.get("text"))
            .and_then(Value::as_str)
            .map(ToString::to_string)?,
        Some(_) => return None,
    };

    let start_line = json_u32(deleted_region, "startLine")?;
    let start_column = json_u32(deleted_region, "startColumn")?;
    let end_column = json_u32(deleted_region, "endColumn")?;
    let edit = TextEdit {
        path: path.to_string(),
        start_line,
        start_column,
        end_line: json_u32(deleted_region, "endLine").unwrap_or(start_line),
        end_column,
        replacement: replacement_text,
    };

    is_valid_text_edit(&edit).then_some(edit)
}

fn parse_context_chains(result: &Value) -> Vec<ContextChain> {
    let mut chains = Vec::new();
    let metadata_kinds = sarif_metadata_context_kinds(result);
    let mut has_structured_frames = false;

    for code_flow in result
        .get("codeFlows")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        for thread_flow in code_flow
            .get("threadFlows")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            let frames = parse_thread_flow_frames(thread_flow);
            if frames.is_empty() {
                continue;
            }
            let kind = infer_thread_flow_kind(thread_flow)
                .or_else(|| metadata_kinds.first().cloned())
                .unwrap_or(ContextChainKind::AnalyzerPath);
            for frame in frames {
                push_chain_frame(&mut chains, kind.clone(), frame);
            }
            has_structured_frames = true;
        }
    }

    if result.get("codeFlows").is_some() && !has_structured_frames && metadata_kinds.is_empty() {
        ensure_chain(&mut chains, ContextChainKind::AnalyzerPath);
    }

    for kind in metadata_kinds {
        ensure_chain(&mut chains, kind);
    }

    for location in result
        .get("relatedLocations")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let related_message = structured_message_text(location.get("message")).unwrap_or_default();
        let Some(frame) = context_frame_from_related_location(&related_message, location) else {
            continue;
        };
        for kind in text_context_kinds(&related_message) {
            push_chain_frame(&mut chains, kind, frame.clone());
        }
    }

    let message = structured_message_text(result.get("message")).unwrap_or_default();
    for kind in text_context_kinds(&message) {
        ensure_chain(&mut chains, kind);
    }

    chains
}

fn sarif_metadata_context_kinds(result: &Value) -> Vec<ContextChainKind> {
    let mut kinds = Vec::new();
    if let Some(rule_id) = result.get("ruleId").and_then(Value::as_str) {
        extend_unique_context_kinds(&mut kinds, metadata_context_kinds(rule_id));
    }
    for taxon in result
        .get("taxa")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        for seed in sarif_taxon_metadata_seeds(taxon) {
            extend_unique_context_kinds(&mut kinds, metadata_context_kinds(&seed));
        }
    }
    kinds
}

fn sarif_taxon_metadata_seeds(taxon: &Value) -> Vec<String> {
    let mut seeds = Vec::new();
    for key in ["id", "name"] {
        if let Some(text) = taxon.get(key).and_then(Value::as_str)
            && !text.trim().is_empty()
        {
            seeds.push(text.to_string());
        }
    }
    for key in ["shortDescription", "fullDescription"] {
        if let Some(text) = structured_message_text(taxon.get(key))
            && !text.trim().is_empty()
        {
            seeds.push(text);
        }
    }
    seeds
}

fn parse_thread_flow_frames(thread_flow: &Value) -> Vec<diag_core::ContextFrame> {
    thread_flow
        .get("locations")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(context_frame_from_thread_flow_location)
        .collect()
}

fn infer_thread_flow_kind(thread_flow: &Value) -> Option<ContextChainKind> {
    if let Some(message) = structured_message_text(thread_flow.get("message")) {
        if let Some(kind) = text_context_kinds(&message).into_iter().next() {
            return Some(kind);
        }
    }
    thread_flow
        .get("locations")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(thread_flow_location_label)
        .find_map(|label| text_context_kinds(&label).into_iter().next())
}

fn context_frame_from_thread_flow_location(
    thread_flow_location: &Value,
) -> Option<diag_core::ContextFrame> {
    let location = thread_flow_location
        .get("location")
        .unwrap_or(thread_flow_location);
    let message = structured_message_text(thread_flow_location.get("message"))
        .or_else(|| structured_message_text(location.get("message")));
    context_frame_from_sarif_location(message.as_deref(), location)
}

fn thread_flow_location_label(thread_flow_location: &Value) -> Option<String> {
    structured_message_text(thread_flow_location.get("message")).or_else(|| {
        thread_flow_location
            .get("location")
            .and_then(|location| structured_message_text(location.get("message")))
    })
}

fn context_frame_from_related_location(
    message: &str,
    location: &Value,
) -> Option<diag_core::ContextFrame> {
    context_frame_from_sarif_location(Some(message), location)
}

fn context_frame_from_sarif_location(
    message: Option<&str>,
    location: &Value,
) -> Option<diag_core::ContextFrame> {
    let physical = location
        .get("physicalLocation")
        .or_else(|| location.get("physical_location"));
    let region = physical
        .and_then(|physical| physical.get("region"))
        .cloned()
        .unwrap_or(Value::Null);
    let path = physical
        .and_then(|physical| physical.get("artifactLocation"))
        .and_then(|artifact| artifact.get("uri"))
        .and_then(Value::as_str)
        .map(ToString::to_string);
    let label = message
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .map(ToString::to_string)
        .or_else(|| path.clone())?;

    Some(diag_core::ContextFrame {
        label,
        path,
        line: json_u32(&region, "startLine"),
        column: json_u32(&region, "startColumn"),
    })
}

pub(crate) fn ensure_chain(chains: &mut Vec<ContextChain>, kind: ContextChainKind) {
    if chains.iter().any(|chain| chain.kind == kind) {
        return;
    }
    chains.push(ContextChain {
        kind,
        frames: Vec::new(),
    });
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
