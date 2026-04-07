use diag_core::{
    AnalysisOverlay, Confidence, ContextChainKind, DiagnosticDocument, DiagnosticNode, Ownership,
    Phase,
};
use std::path::{Path, PathBuf};

pub fn enrich_document(document: &mut DiagnosticDocument, cwd: &Path) {
    for node in &mut document.diagnostics {
        enrich_node(node, cwd);
    }
    document.refresh_fingerprints();
}

fn enrich_node(node: &mut DiagnosticNode, cwd: &Path) {
    for location in &mut node.locations {
        location.ownership = Some(classify_ownership(&location.path, cwd));
    }
    let derived_family = classify_family(node);
    let family = node
        .analysis
        .as_ref()
        .and_then(|analysis| analysis.family.as_ref())
        .filter(|family| family.contains('.') && derived_family != "unknown")
        .cloned()
        .unwrap_or(derived_family);
    let confidence = classify_confidence(node, family.as_str());
    let headline = headline_for(node, &family);
    let first_action = first_action_for(family.as_str());

    let analysis = node.analysis.get_or_insert(AnalysisOverlay {
        family: None,
        headline: None,
        first_action_hint: None,
        confidence: None,
        collapsed_child_ids: Vec::new(),
        collapsed_chain_ids: Vec::new(),
    });
    analysis.family = Some(family.clone());
    analysis.headline = Some(headline);
    analysis.first_action_hint = Some(first_action);
    analysis.confidence = Some(confidence);

    for child in &mut node.children {
        enrich_node(child, cwd);
    }
}

fn classify_family(node: &DiagnosticNode) -> String {
    let message = node.message.raw_text.to_lowercase();
    let child_messages = node
        .children
        .iter()
        .map(|child| child.message.raw_text.to_lowercase())
        .collect::<Vec<_>>()
        .join("\n");
    if matches!(node.phase, Phase::Link) || message.contains("undefined reference") {
        "linker".to_string()
    } else if node
        .context_chains
        .iter()
        .any(|chain| matches!(chain.kind, ContextChainKind::TemplateInstantiation))
        || message.contains("template")
        || child_messages.contains("template")
        || child_messages.contains("deduction/substitution")
        || child_messages.contains("deduced conflicting")
    {
        "template".to_string()
    } else if node.context_chains.iter().any(|chain| {
        matches!(
            chain.kind,
            ContextChainKind::MacroExpansion | ContextChainKind::Include
        )
    }) || message.contains("macro")
        || message.contains("include")
        || child_messages.contains("macro")
        || child_messages.contains("include")
    {
        "macro_include".to_string()
    } else if message.contains("cannot convert")
        || message.contains("invalid conversion")
        || message.contains("no matching")
        || message.contains("candidate")
        || message.contains("incompatible type")
        || message.contains("passing argument")
    {
        "type_overload".to_string()
    } else if message.contains("expected")
        || message.contains("before")
        || message.contains("missing")
    {
        "syntax".to_string()
    } else if matches!(node.semantic_role, diag_core::SemanticRole::Passthrough) {
        "passthrough".to_string()
    } else {
        "unknown".to_string()
    }
}

fn classify_confidence(node: &DiagnosticNode, family: &str) -> Confidence {
    if family == "passthrough" || family == "unknown" {
        Confidence::Low
    } else if !node.locations.is_empty() {
        Confidence::High
    } else {
        Confidence::Medium
    }
}

fn headline_for(node: &DiagnosticNode, family: &str) -> String {
    match family {
        "syntax" => "syntax error".to_string(),
        "type_overload" => "type or overload mismatch".to_string(),
        "template" => "template instantiation failed".to_string(),
        "macro_include" => "error surfaced through macro/include context".to_string(),
        "linker.undefined_reference" => node
            .symbol_context
            .as_ref()
            .and_then(|symbol| symbol.primary_symbol.clone())
            .map(|symbol| format!("undefined reference to `{symbol}`"))
            .unwrap_or_else(|| "undefined reference reported by linker".to_string()),
        "linker.multiple_definition" => node
            .symbol_context
            .as_ref()
            .and_then(|symbol| symbol.primary_symbol.clone())
            .map(|symbol| format!("multiple definition of `{symbol}`"))
            .unwrap_or_else(|| "duplicate symbol definition reported by linker".to_string()),
        "linker" => node
            .symbol_context
            .as_ref()
            .and_then(|symbol| symbol.primary_symbol.clone())
            .map(|symbol| format!("linker failed to resolve `{symbol}`"))
            .unwrap_or_else(|| "linker reported a failure".to_string()),
        "passthrough" => "showing conservative wrapper view".to_string(),
        _ => node
            .message
            .raw_text
            .lines()
            .next()
            .unwrap_or("diagnostic")
            .to_string(),
    }
}

fn first_action_for(family: &str) -> String {
    match family {
        "syntax" => "fix the first parser error at the user-owned location".to_string(),
        "type_overload" => {
            "compare the expected type and actual argument at the call site".to_string()
        }
        "template" => "start from the first user-owned template frame and match template arguments"
            .to_string(),
        "macro_include" => {
            "inspect the user-owned macro invocation or include edge that reaches the failing line"
                .to_string()
        }
        "linker.undefined_reference" => {
            "define the missing symbol or link the object/library that provides it".to_string()
        }
        "linker.multiple_definition" => {
            "remove the duplicate definition or make the symbol internal to one translation unit"
                .to_string()
        }
        "linker" => {
            "check the missing/duplicate symbol and the object or library inputs".to_string()
        }
        _ => "inspect the preserved raw diagnostics for the first corrective action".to_string(),
    }
}

fn classify_ownership(path: &str, cwd: &Path) -> Ownership {
    let path = PathBuf::from(path);
    if path.is_relative() {
        return Ownership::User;
    }
    if path.starts_with(cwd) {
        return Ownership::User;
    }
    let rendered = path.display().to_string();
    if rendered.contains("/usr/include")
        || rendered.contains("/usr/lib")
        || rendered.contains("/opt/homebrew")
    {
        Ownership::System
    } else if rendered.contains("/vendor/")
        || rendered.contains("/third_party/")
        || rendered.contains("/external/")
    {
        Ownership::Vendor
    } else if rendered.contains("/generated/")
        || rendered.contains("/build/")
        || rendered.ends_with(".generated.h")
        || rendered.ends_with(".generated.hpp")
    {
        Ownership::Generated
    } else {
        Ownership::User
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use diag_core::{
        DocumentCompleteness, MessageText, NodeCompleteness, Origin, ProducerInfo, Provenance,
        ProvenanceSource, RunInfo, SemanticRole, Severity, ToolInfo,
    };

    #[test]
    fn annotates_user_owned_syntax_diagnostic() {
        let mut document = DiagnosticDocument {
            document_id: "doc".to_string(),
            schema_version: "1".to_string(),
            document_completeness: DocumentCompleteness::Complete,
            producer: ProducerInfo {
                name: "gcc-formed".to_string(),
                version: "0.1.0".to_string(),
                git_revision: None,
                build_profile: None,
                rulepack_version: None,
            },
            run: RunInfo {
                invocation_id: "inv".to_string(),
                invoked_as: None,
                argv_redacted: Vec::new(),
                cwd_display: None,
                exit_status: 1,
                primary_tool: ToolInfo {
                    name: "gcc".to_string(),
                    version: None,
                    component: None,
                    vendor: None,
                },
                secondary_tools: Vec::new(),
                language_mode: None,
                target_triple: None,
                wrapper_mode: None,
            },
            captures: Vec::new(),
            integrity_issues: Vec::new(),
            diagnostics: vec![DiagnosticNode {
                id: "n1".to_string(),
                origin: Origin::Gcc,
                phase: Phase::Parse,
                severity: Severity::Error,
                semantic_role: SemanticRole::Root,
                message: MessageText {
                    raw_text: "expected ';' before '}' token".to_string(),
                    normalized_text: None,
                    locale: None,
                },
                locations: vec![diag_core::Location {
                    path: "src/main.c".to_string(),
                    line: 3,
                    column: 1,
                    end_line: None,
                    end_column: None,
                    display_path: None,
                    ownership: None,
                }],
                children: Vec::new(),
                suggestions: Vec::new(),
                context_chains: Vec::new(),
                symbol_context: None,
                node_completeness: NodeCompleteness::Complete,
                provenance: Provenance {
                    source: ProvenanceSource::Compiler,
                    capture_refs: vec!["stderr.raw".to_string()],
                },
                analysis: None,
                fingerprints: None,
            }],
            fingerprints: None,
        };
        enrich_document(&mut document, Path::new("/tmp/project"));
        let analysis = document.diagnostics[0].analysis.as_ref().unwrap();
        assert_eq!(analysis.family.as_deref(), Some("syntax"));
        assert_eq!(
            document.diagnostics[0].locations[0].ownership,
            Some(Ownership::User)
        );
    }
}
