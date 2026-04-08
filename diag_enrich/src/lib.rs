use diag_core::{
    AnalysisOverlay, Confidence, ContextChainKind, DiagnosticDocument, DiagnosticNode, Ownership,
    Phase, SemanticRole,
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
    let family = classify_family(node);
    let confidence = classify_confidence(node, family.family.as_str());
    let presentation = presentation_for(node, family.family.as_str());

    let analysis = node.analysis.get_or_insert(AnalysisOverlay {
        family: None,
        headline: None,
        first_action_hint: None,
        confidence: None,
        rule_id: None,
        matched_conditions: Vec::new(),
        suppression_reason: None,
        collapsed_child_ids: Vec::new(),
        collapsed_chain_ids: Vec::new(),
    });
    analysis.family = Some(family.family);
    analysis.headline = Some(presentation.headline);
    analysis.first_action_hint = Some(presentation.first_action_hint);
    analysis.confidence = Some(confidence);
    analysis.rule_id = Some(family.rule_id);
    analysis.matched_conditions = family.matched_conditions;
    analysis.suppression_reason = family.suppression_reason;

    for child in &mut node.children {
        enrich_node(child, cwd);
    }
}

#[derive(Debug, Clone)]
struct FamilyDecision {
    family: String,
    rule_id: String,
    matched_conditions: Vec<String>,
    suppression_reason: Option<String>,
}

#[derive(Debug, Clone, Copy)]
struct NodeFamilyRule {
    id: &'static str,
    family: &'static str,
    requires_link_phase: bool,
    requires_template_context: bool,
    requires_macro_include_context: bool,
    requires_passthrough_role: bool,
    message_contains_any: &'static [&'static str],
    child_message_contains_any: &'static [&'static str],
}

#[derive(Debug, Clone, Copy)]
struct PresentationRule {
    family: &'static str,
    headline: &'static str,
    first_action_hint: &'static str,
}

const FAMILY_RULES: &[NodeFamilyRule] = &[
    NodeFamilyRule {
        id: "rule.family.linker.phase_or_message",
        family: "linker",
        requires_link_phase: true,
        requires_template_context: false,
        requires_macro_include_context: false,
        requires_passthrough_role: false,
        message_contains_any: &["undefined reference", "multiple definition"],
        child_message_contains_any: &[],
    },
    NodeFamilyRule {
        id: "rule.family.template.context_or_message",
        family: "template",
        requires_link_phase: false,
        requires_template_context: true,
        requires_macro_include_context: false,
        requires_passthrough_role: false,
        message_contains_any: &["template"],
        child_message_contains_any: &["template", "deduction/substitution", "deduced conflicting"],
    },
    NodeFamilyRule {
        id: "rule.family.macro_include.context_or_message",
        family: "macro_include",
        requires_link_phase: false,
        requires_template_context: false,
        requires_macro_include_context: true,
        requires_passthrough_role: false,
        message_contains_any: &["macro", "include"],
        child_message_contains_any: &["macro", "include"],
    },
    NodeFamilyRule {
        id: "rule.family.type_overload.message",
        family: "type_overload",
        requires_link_phase: false,
        requires_template_context: false,
        requires_macro_include_context: false,
        requires_passthrough_role: false,
        message_contains_any: &[
            "cannot convert",
            "invalid conversion",
            "no matching",
            "candidate",
            "incompatible type",
            "passing argument",
        ],
        child_message_contains_any: &[],
    },
    NodeFamilyRule {
        id: "rule.family.syntax.message",
        family: "syntax",
        requires_link_phase: false,
        requires_template_context: false,
        requires_macro_include_context: false,
        requires_passthrough_role: false,
        message_contains_any: &["expected", "before", "missing"],
        child_message_contains_any: &[],
    },
    NodeFamilyRule {
        id: "rule.family.passthrough.semantic_role",
        family: "passthrough",
        requires_link_phase: false,
        requires_template_context: false,
        requires_macro_include_context: false,
        requires_passthrough_role: true,
        message_contains_any: &[],
        child_message_contains_any: &[],
    },
];

const STATIC_PRESENTATION_RULES: &[PresentationRule] = &[
    PresentationRule {
        family: "syntax",
        headline: "syntax error",
        first_action_hint: "fix the first parser error at the user-owned location",
    },
    PresentationRule {
        family: "type_overload",
        headline: "type or overload mismatch",
        first_action_hint: "compare the expected type and actual argument at the call site",
    },
    PresentationRule {
        family: "template",
        headline: "template instantiation failed",
        first_action_hint: "start from the first user-owned template frame and match template arguments",
    },
    PresentationRule {
        family: "macro_include",
        headline: "error surfaced through macro/include context",
        first_action_hint: "inspect the user-owned macro invocation or include edge that reaches the failing line",
    },
    PresentationRule {
        family: "linker",
        headline: "linker reported a failure",
        first_action_hint: "check the missing/duplicate symbol and the object or library inputs",
    },
    PresentationRule {
        family: "passthrough",
        headline: "showing conservative wrapper view",
        first_action_hint: "inspect the preserved raw diagnostics for the first corrective action",
    },
];

fn classify_family(node: &DiagnosticNode) -> FamilyDecision {
    let message = node.message.raw_text.to_lowercase();
    let child_messages = node
        .children
        .iter()
        .map(|child| child.message.raw_text.to_lowercase())
        .collect::<Vec<_>>()
        .join("\n");
    for rule in FAMILY_RULES {
        let mut matched_conditions = Vec::new();
        if rule.requires_link_phase {
            let link_phase_match = matches!(node.phase, Phase::Link);
            let link_message_match = contains_any(&message, rule.message_contains_any);
            if !link_phase_match && !link_message_match {
                continue;
            }
            if link_phase_match {
                matched_conditions.push("phase=link".to_string());
            }
            if link_message_match {
                matched_conditions.extend(matching_conditions(
                    "message_contains",
                    &message,
                    rule.message_contains_any,
                ));
            }
        }
        if rule.requires_template_context {
            let template_context = node
                .context_chains
                .iter()
                .any(|chain| matches!(chain.kind, ContextChainKind::TemplateInstantiation));
            let template_message = contains_any(&message, rule.message_contains_any);
            let template_child = contains_any(&child_messages, rule.child_message_contains_any);
            if !template_context && !template_message && !template_child {
                continue;
            }
            if template_context {
                matched_conditions.push("context=template_instantiation".to_string());
            }
            matched_conditions.extend(matching_conditions(
                "message_contains",
                &message,
                rule.message_contains_any,
            ));
            matched_conditions.extend(matching_conditions(
                "child_message_contains",
                &child_messages,
                rule.child_message_contains_any,
            ));
        }
        if rule.requires_macro_include_context {
            let macro_include_context = node.context_chains.iter().any(|chain| {
                matches!(
                    chain.kind,
                    ContextChainKind::MacroExpansion | ContextChainKind::Include
                )
            });
            let macro_include_message = contains_any(&message, rule.message_contains_any);
            let macro_include_child =
                contains_any(&child_messages, rule.child_message_contains_any);
            if !macro_include_context && !macro_include_message && !macro_include_child {
                continue;
            }
            if macro_include_context {
                matched_conditions.push("context=macro_or_include".to_string());
            }
            matched_conditions.extend(matching_conditions(
                "message_contains",
                &message,
                rule.message_contains_any,
            ));
            matched_conditions.extend(matching_conditions(
                "child_message_contains",
                &child_messages,
                rule.child_message_contains_any,
            ));
        }
        if rule.requires_passthrough_role {
            if !matches!(node.semantic_role, SemanticRole::Passthrough) {
                continue;
            }
            matched_conditions.push("semantic_role=passthrough".to_string());
        }
        if !rule.requires_link_phase
            && !rule.requires_template_context
            && !rule.requires_macro_include_context
            && !rule.requires_passthrough_role
        {
            if !contains_any(&message, rule.message_contains_any) {
                continue;
            }
            matched_conditions.extend(matching_conditions(
                "message_contains",
                &message,
                rule.message_contains_any,
            ));
        }
        return finalize_family_decision(node, rule, matched_conditions);
    }
    finalize_unknown_family(node)
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

#[derive(Debug, Clone)]
struct PresentationDecision {
    headline: String,
    first_action_hint: String,
}

fn presentation_for(node: &DiagnosticNode, family: &str) -> PresentationDecision {
    match family {
        "linker.undefined_reference" => PresentationDecision {
            headline: node
                .symbol_context
                .as_ref()
                .and_then(|symbol| symbol.primary_symbol.clone())
                .map(|symbol| format!("undefined reference to `{symbol}`"))
                .unwrap_or_else(|| "undefined reference reported by linker".to_string()),
            first_action_hint:
                "define the missing symbol or link the object/library that provides it".to_string(),
        },
        "linker.multiple_definition" => PresentationDecision {
            headline: node
                .symbol_context
                .as_ref()
                .and_then(|symbol| symbol.primary_symbol.clone())
                .map(|symbol| format!("multiple definition of `{symbol}`"))
                .unwrap_or_else(|| "duplicate symbol definition reported by linker".to_string()),
            first_action_hint:
                "remove the duplicate definition or make the symbol internal to one translation unit"
                    .to_string(),
        },
        _ => {
            if let Some(rule) = STATIC_PRESENTATION_RULES.iter().find(|rule| {
                rule.family == family || (rule.family == "linker" && family.starts_with("linker."))
            }) {
                PresentationDecision {
                    headline: if rule.family == "linker" {
                        node.symbol_context
                            .as_ref()
                            .and_then(|symbol| symbol.primary_symbol.clone())
                            .map(|symbol| format!("linker failed to resolve `{symbol}`"))
                            .unwrap_or_else(|| rule.headline.to_string())
                    } else {
                        rule.headline.to_string()
                    },
                    first_action_hint: rule.first_action_hint.to_string(),
                }
            } else {
                PresentationDecision {
                    headline: node
                        .message
                        .raw_text
                        .lines()
                        .next()
                        .unwrap_or("diagnostic")
                        .to_string(),
                    first_action_hint:
                        "inspect the preserved raw diagnostics for the first corrective action"
                            .to_string(),
                }
            }
        }
    }
}

fn finalize_family_decision(
    node: &DiagnosticNode,
    rule: &NodeFamilyRule,
    mut matched_conditions: Vec<String>,
) -> FamilyDecision {
    let existing_specific = node
        .analysis
        .as_ref()
        .and_then(|analysis| analysis.family.as_ref())
        .filter(|family| family.contains('.') && rule.family != "unknown")
        .cloned();
    if let Some(existing_family) = existing_specific {
        matched_conditions.push(format!("existing_specific_family={existing_family}"));
        FamilyDecision {
            family: existing_family,
            rule_id: "rule.family.ingress_specific_override".to_string(),
            matched_conditions,
            suppression_reason: Some("preserved_specific_family_from_ingress".to_string()),
        }
    } else {
        FamilyDecision {
            family: rule.family.to_string(),
            rule_id: rule.id.to_string(),
            matched_conditions,
            suppression_reason: None,
        }
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
    for rule in OWNERSHIP_RULES {
        if contains_any(&rendered, rule.path_contains_any)
            || ends_with_any(&rendered, rule.path_suffixes)
        {
            return rule.ownership.clone();
        }
    }
    Ownership::User
}

#[derive(Debug, Clone)]
struct OwnershipRule {
    ownership: Ownership,
    path_contains_any: &'static [&'static str],
    path_suffixes: &'static [&'static str],
}

const OWNERSHIP_RULES: &[OwnershipRule] = &[
    OwnershipRule {
        ownership: Ownership::System,
        path_contains_any: &["/usr/include", "/usr/lib", "/opt/homebrew"],
        path_suffixes: &[],
    },
    OwnershipRule {
        ownership: Ownership::Vendor,
        path_contains_any: &["/vendor/", "/third_party/", "/external/"],
        path_suffixes: &[],
    },
    OwnershipRule {
        ownership: Ownership::Generated,
        path_contains_any: &["/generated/", "/build/"],
        path_suffixes: &[".generated.h", ".generated.hpp"],
    },
];

fn finalize_unknown_family(node: &DiagnosticNode) -> FamilyDecision {
    let existing_specific = node
        .analysis
        .as_ref()
        .and_then(|analysis| analysis.family.as_ref())
        .filter(|family| family.contains('.'))
        .cloned();
    if let Some(existing_family) = existing_specific {
        return FamilyDecision {
            family: existing_family,
            rule_id: "rule.family.ingress_specific_override".to_string(),
            matched_conditions: vec!["derived_family=unknown".to_string()],
            suppression_reason: Some("preserved_specific_family_from_ingress".to_string()),
        };
    }
    FamilyDecision {
        family: if matches!(node.semantic_role, SemanticRole::Passthrough) {
            "passthrough".to_string()
        } else {
            "unknown".to_string()
        },
        rule_id: if matches!(node.semantic_role, SemanticRole::Passthrough) {
            "rule.family.passthrough.semantic_role".to_string()
        } else {
            "rule.family.unknown".to_string()
        },
        matched_conditions: vec!["no_family_rule_matched".to_string()],
        suppression_reason: Some("generic_fallback".to_string()),
    }
}

fn contains_any(haystack: &str, needles: &[&str]) -> bool {
    needles.iter().any(|needle| haystack.contains(needle))
}

fn matching_conditions(prefix: &str, haystack: &str, needles: &[&str]) -> Vec<String> {
    needles
        .iter()
        .filter(|needle| haystack.contains(**needle))
        .map(|needle| format!("{prefix}={needle}"))
        .collect()
}

fn ends_with_any(haystack: &str, suffixes: &[&str]) -> bool {
    suffixes.iter().any(|suffix| haystack.ends_with(suffix))
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
