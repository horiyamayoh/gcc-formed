use diag_core::{
    AnalysisOverlay, Confidence, ContextChain, ContextChainKind, DiagnosticDocument,
    DiagnosticNode, DocumentCompleteness, FingerprintSet, IntegrityIssue, IssueSeverity,
    IssueStage, Location, MessageText, NodeCompleteness, Origin, Phase, ProducerInfo, Provenance,
    ProvenanceSource, RunInfo, SemanticRole, Severity, ToolInfo,
};
use diag_residual_text::classify as classify_residual;
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
    let mut document = if let Some(path) = sarif_path.filter(|path| path.exists()) {
        from_sarif(path, producer, run)?
    } else {
        passthrough_document(producer, run)
    };

    let residual_nodes = classify_residual(stderr_text);
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
            family: Some(infer_family(&raw_text)),
            headline: Some(raw_text.lines().next().unwrap_or(&raw_text).to_string()),
            first_action_hint: Some(first_action_hint(&raw_text)),
            confidence: Some(Confidence::Medium),
            collapsed_child_ids: Vec::new(),
            collapsed_chain_ids: Vec::new(),
        }),
        fingerprints: Some(FingerprintSet {
            raw: diag_core::fingerprint_for(&raw_text),
            structural: diag_core::fingerprint_for(&result),
            family: diag_core::fingerprint_for(&infer_family(&raw_text)),
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
        .map(|(index, location)| DiagnosticNode {
            id: format!("sarif-{run_index}-{result_index}-related-{index}"),
            origin: Origin::Gcc,
            phase: Phase::Semantic,
            severity: Severity::Note,
            semantic_role: SemanticRole::Supporting,
            message: MessageText {
                raw_text: location
                    .get("message")
                    .and_then(|message| message.get("text"))
                    .and_then(Value::as_str)
                    .unwrap_or("related location")
                    .to_string(),
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

fn infer_family(message: &str) -> String {
    let message = message.to_lowercase();
    if message.contains("undefined reference") {
        "linker.undefined_reference".to_string()
    } else if message.contains("multiple definition") {
        "linker.multiple_definition".to_string()
    } else if message.contains("template") {
        "template".to_string()
    } else if message.contains("macro") || message.contains("include") {
        "macro_include".to_string()
    } else if message.contains("cannot convert")
        || message.contains("no matching")
        || message.contains("invalid conversion")
    {
        "type_overload".to_string()
    } else if message.contains("expected") || message.contains("before") {
        "syntax".to_string()
    } else {
        "unknown".to_string()
    }
}

fn first_action_hint(message: &str) -> String {
    let family = infer_family(message);
    match family.as_str() {
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
}
