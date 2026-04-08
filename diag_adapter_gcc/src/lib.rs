use diag_core::{
    AnalysisOverlay, Confidence, ContextChain, ContextChainKind, DiagnosticDocument,
    DiagnosticNode, DocumentCompleteness, FingerprintSet, IntegrityIssue, IssueSeverity,
    IssueStage, Location, MessageText, NodeCompleteness, Origin, Phase, ProducerInfo, Provenance,
    ProvenanceSource, RunInfo, SemanticRole, Severity, ToolInfo,
};
use diag_residual_text::classify;
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

pub fn ingest(
    sarif_path: Option<&Path>,
    stderr_text: &str,
    producer: ProducerInfo,
    run: RunInfo,
) -> Result<DiagnosticDocument, AdapterError> {
    let has_authoritative_sarif = sarif_path.filter(|path| path.exists()).is_some();
    let mut document = if let Some(path) = sarif_path.filter(|path| path.exists()) {
        from_sarif(path, producer, run)?
    } else {
        passthrough_document(producer, run)
    };

    let residual_nodes = classify(stderr_text, !has_authoritative_sarif);
    if document.diagnostics.is_empty() && residual_nodes.is_empty() && !stderr_text.is_empty() {
        document.document_completeness = DocumentCompleteness::Passthrough;
        document.diagnostics.push(passthrough_node(stderr_text));
    } else if !residual_nodes.is_empty() {
        if matches!(
            document.document_completeness,
            DocumentCompleteness::Complete
        ) {
            document.document_completeness = DocumentCompleteness::Partial;
        }
        document.diagnostics.extend(residual_nodes);
    }
    if has_authoritative_sarif {
        augment_context_chains_from_stderr(&mut document, stderr_text);
    }
    document.refresh_fingerprints();
    Ok(document)
}

pub fn from_sarif(
    sarif_path: &Path,
    producer: ProducerInfo,
    run: RunInfo,
) -> Result<DiagnosticDocument, AdapterError> {
    let json = fs::read_to_string(sarif_path)?;
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
            let node = result_to_node(run_index, result_index, result);
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
                capture_refs: vec!["diagnostics.sarif".to_string()],
            }),
        });
    }

    Ok(document)
}

fn result_to_node(run_index: usize, result_index: usize, result: &Value) -> DiagnosticNode {
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
    let children = parse_related_locations(run_index, result_index, result);
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
            capture_refs: vec!["diagnostics.sarif".to_string()],
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
                    capture_refs: vec!["diagnostics.sarif".to_string()],
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
        rulepack_version: Some("phase1".to_string()),
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
        let document = from_sarif(
            &path,
            producer_for_version("0.1.0"),
            RunInfo {
                invocation_id: "inv".to_string(),
                invoked_as: Some("gcc-formed".to_string()),
                argv_redacted: vec!["gcc".to_string()],
                cwd_display: None,
                exit_status: 1,
                primary_tool: tool_for_backend("gcc", Some("15.1.0".to_string())),
                secondary_tools: Vec::new(),
                language_mode: Some(LanguageMode::C),
                target_triple: None,
                wrapper_mode: Some(WrapperSurface::Terminal),
            },
        )
        .unwrap();
        assert_eq!(document.diagnostics.len(), 1);
        assert_eq!(document.diagnostics[0].locations[0].path, "src/main.c");
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
        let document = from_sarif(
            &path,
            producer_for_version("0.1.0"),
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
            },
        )
        .unwrap();

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
                invocation_id: "inv".to_string(),
                invoked_as: Some("gcc-formed".to_string()),
                argv_redacted: vec!["g++".to_string()],
                cwd_display: None,
                exit_status: 1,
                primary_tool: tool_for_backend("g++", Some("15.2.0".to_string())),
                secondary_tools: Vec::new(),
                language_mode: Some(LanguageMode::Cpp),
                target_triple: None,
                wrapper_mode: Some(WrapperSurface::Terminal),
            },
        )
        .unwrap();

        assert_eq!(document.diagnostics[0].children.len(), 1);
        assert_eq!(
            document.diagnostics[0].children[0].message.raw_text,
            "candidate 1: 'void takes(int, int)'"
        );
    }
}
