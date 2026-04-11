//! GCC JSON diagnostic parsing.

use crate::classify::{
    classify_family_seed, combined_message_seed, first_action_hint, infer_phase,
    infer_related_phase, infer_related_role, structured_message_text,
};
use crate::fixits::suggestion_from_edits;
use crate::ingest::AdapterError;
use crate::sarif::{push_chain_frame, read_structured_artifact_text};
use crate::{is_valid_text_edit, json_str, json_u32};
use diag_core::{
    AnalysisOverlay, CaptureArtifact, Confidence, ContextChain, ContextChainKind,
    DiagnosticDocument, DiagnosticNode, DocumentCompleteness, FingerprintSet, IntegrityIssue,
    IssueSeverity, IssueStage, Location, MessageText, NodeCompleteness, Origin, ProducerInfo,
    Provenance, ProvenanceSource, RunInfo, SemanticRole, Severity, Suggestion,
    SuggestionApplicability, TextEdit,
};
use serde_json::Value;

pub(crate) fn from_gcc_json_artifact(
    artifact: &CaptureArtifact,
    producer: &ProducerInfo,
    run: &RunInfo,
) -> Result<DiagnosticDocument, AdapterError> {
    let json = read_structured_artifact_text(artifact)?;
    from_gcc_json_payload(&json, &artifact.id, producer, run)
}

pub(crate) fn from_gcc_json_payload(
    json: &str,
    capture_ref: &str,
    producer: &ProducerInfo,
    run: &RunInfo,
) -> Result<DiagnosticDocument, AdapterError> {
    let diagnostics: Vec<Value> = serde_json::from_str(json)?;
    let mut document = DiagnosticDocument {
        document_id: format!("gcc-json-{}", run.invocation_id),
        schema_version: diag_core::IR_SPEC_VERSION.to_string(),
        document_completeness: DocumentCompleteness::Complete,
        producer: producer.clone(),
        run: run.clone(),
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
    let suggestions = parse_fixit_suggestions(diagnostic);
    let completeness = if locations.is_empty() {
        NodeCompleteness::Partial
    } else {
        NodeCompleteness::Complete
    };
    let severity = gcc_json_severity(json_str(diagnostic, "kind"));
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
        suggestions,
        context_chains,
        symbol_context: None,
        node_completeness: completeness,
        provenance: Provenance {
            source: ProvenanceSource::Compiler,
            capture_refs: vec![capture_ref.to_string()],
        },
        analysis: is_root.then_some(AnalysisOverlay {
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

    let mut parsed = Location::caret(path, line, column, diag_core::LocationRole::Primary);
    if let (Some(end_line), Some(end_column)) = (
        finish.and_then(gcc_json_point_line),
        finish.and_then(gcc_json_point_column),
    ) {
        parsed = parsed.with_range_end(end_line, end_column, diag_core::BoundarySemantics::Unknown);
    }
    Some(parsed)
}

fn gcc_json_point_file(point: &Value) -> Option<String> {
    let file = json_str(point, "file");
    if file.is_empty() {
        None
    } else {
        Some(file.to_string())
    }
}

fn gcc_json_point_line(point: &Value) -> Option<u32> {
    json_u32(point, "line")
}

fn gcc_json_point_column(point: &Value) -> Option<u32> {
    json_u32(point, "column")
        .or_else(|| json_u32(point, "display-column"))
        .or_else(|| json_u32(point, "byte-column"))
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

fn parse_fixit_suggestions(diagnostic: &Value) -> Vec<Suggestion> {
    diagnostic
        .get("fixits")
        .or_else(|| diagnostic.get("fixit-hints"))
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(parse_fixit_suggestion)
        .collect()
}

fn parse_fixit_suggestion(fixit: &Value) -> Option<Suggestion> {
    let edit = parse_fixit_text_edit(fixit)?;
    suggestion_from_edits(None, SuggestionApplicability::MachineApplicable, vec![edit])
}

fn parse_fixit_text_edit(fixit: &Value) -> Option<TextEdit> {
    let start = fixit.get("start")?;
    let end = fixit.get("next")?;
    let path = gcc_json_point_file(start)?;
    let end_path = gcc_json_point_file(end).unwrap_or_else(|| path.clone());
    if path != end_path {
        return None;
    }

    let edit = TextEdit {
        path,
        start_line: gcc_json_point_line(start)?,
        start_column: gcc_json_point_column(start)?,
        end_line: gcc_json_point_line(end)?,
        end_column: gcc_json_point_column(end)?,
        replacement: fixit.get("string")?.as_str()?.to_string(),
    };

    is_valid_text_edit(&edit).then_some(edit)
}

fn json_message_text(message: Option<&Value>) -> Option<String> {
    structured_message_text(message)
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
            || lowered.contains("required by substitution")
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
        path: location.map(|location| location.path_raw().to_string()),
        line: location.map(|location| location.line()),
        column: location.map(|location| location.column()),
    }
}

fn gcc_json_severity(kind: &str) -> Severity {
    match kind {
        "fatal error" | "fatal" => Severity::Fatal,
        "warning" => Severity::Warning,
        "note" => Severity::Note,
        "remark" => Severity::Remark,
        "info" => Severity::Info,
        _ => Severity::Error,
    }
}

fn node_is_partial(node: &DiagnosticNode) -> bool {
    matches!(node.node_completeness, NodeCompleteness::Partial)
        || node.children.iter().any(node_is_partial)
}
