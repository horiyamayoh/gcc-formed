use diag_core::{Confidence, ContextChainKind, DiagnosticNode, Ownership, Phase, SemanticRole};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FamilyDecision {
    pub(crate) family: String,
    pub(crate) rule_id: String,
    pub(crate) matched_conditions: Vec<String>,
    pub(crate) suppression_reason: Option<String>,
}

#[derive(Debug, Clone, Copy)]
enum MatchStrategy {
    LinkPhaseOrMessage,
    TemplateContextOrMessage,
    MacroIncludeContextOrMessage,
    PassthroughRole,
    MessageContains,
}

#[derive(Debug, Clone, Copy)]
struct NodeFamilyRule {
    id: &'static str,
    family: &'static str,
    strategy: MatchStrategy,
    message_contains_any: &'static [&'static str],
    child_message_contains_any: &'static [&'static str],
}

const FAMILY_RULES: &[NodeFamilyRule] = &[
    NodeFamilyRule {
        id: "rule.family.linker.phase_or_message",
        family: "linker",
        strategy: MatchStrategy::LinkPhaseOrMessage,
        message_contains_any: &["undefined reference", "multiple definition"],
        child_message_contains_any: &[],
    },
    NodeFamilyRule {
        id: "rule.family.template.context_or_message",
        family: "template",
        strategy: MatchStrategy::TemplateContextOrMessage,
        message_contains_any: &["template"],
        child_message_contains_any: &["template", "deduction/substitution", "deduced conflicting"],
    },
    NodeFamilyRule {
        id: "rule.family.macro_include.context_or_message",
        family: "macro_include",
        strategy: MatchStrategy::MacroIncludeContextOrMessage,
        message_contains_any: &["macro", "include"],
        child_message_contains_any: &["macro", "include"],
    },
    NodeFamilyRule {
        id: "rule.family.type_overload.message",
        family: "type_overload",
        strategy: MatchStrategy::MessageContains,
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
        strategy: MatchStrategy::MessageContains,
        message_contains_any: &["expected", "before", "missing"],
        child_message_contains_any: &[],
    },
    NodeFamilyRule {
        id: "rule.family.passthrough.semantic_role",
        family: "passthrough",
        strategy: MatchStrategy::PassthroughRole,
        message_contains_any: &[],
        child_message_contains_any: &[],
    },
];

pub(crate) fn classify_family(node: &DiagnosticNode) -> FamilyDecision {
    let message = node.message.raw_text.to_lowercase();
    let child_messages = node
        .children
        .iter()
        .map(|child| child.message.raw_text.to_lowercase())
        .collect::<Vec<_>>()
        .join("\n");

    for rule in FAMILY_RULES {
        if let Some(matched_conditions) = match_rule(node, rule, &message, &child_messages) {
            return finalize_family_decision(node, rule, matched_conditions);
        }
    }

    finalize_unknown_family(node)
}

pub(crate) fn classify_confidence(node: &DiagnosticNode, decision: &FamilyDecision) -> Confidence {
    if matches!(decision.family.as_str(), "passthrough" | "unknown") {
        return Confidence::Low;
    }

    let has_user_owned_location = node
        .locations
        .iter()
        .any(|location| location.ownership == Some(Ownership::User));
    let has_structured_signal = decision.matched_conditions.iter().any(|condition| {
        condition.starts_with("phase=")
            || condition.starts_with("context=")
            || condition.starts_with("semantic_role=")
            || condition.starts_with("existing_specific_family=")
    });
    let has_lexical_signal = decision.matched_conditions.iter().any(|condition| {
        condition.starts_with("message_contains=")
            || condition.starts_with("child_message_contains=")
    });

    if has_user_owned_location && (has_structured_signal || has_lexical_signal) {
        Confidence::High
    } else if has_structured_signal || !node.locations.is_empty() {
        Confidence::Medium
    } else {
        Confidence::Low
    }
}

fn match_rule(
    node: &DiagnosticNode,
    rule: &NodeFamilyRule,
    message: &str,
    child_messages: &str,
) -> Option<Vec<String>> {
    let mut matched_conditions = Vec::new();

    match rule.strategy {
        MatchStrategy::LinkPhaseOrMessage => {
            let link_phase_match = matches!(node.phase, Phase::Link);
            let link_message_match = contains_any(message, rule.message_contains_any);
            if !link_phase_match && !link_message_match {
                return None;
            }
            if link_phase_match {
                matched_conditions.push("phase=link".to_string());
            }
            matched_conditions.extend(matching_conditions(
                "message_contains",
                message,
                rule.message_contains_any,
            ));
        }
        MatchStrategy::TemplateContextOrMessage => {
            let template_context = node
                .context_chains
                .iter()
                .any(|chain| matches!(chain.kind, ContextChainKind::TemplateInstantiation));
            let template_message = contains_any(message, rule.message_contains_any);
            let template_child = contains_any(child_messages, rule.child_message_contains_any);
            if !template_context && !template_message && !template_child {
                return None;
            }
            if template_context {
                matched_conditions.push("context=template_instantiation".to_string());
            }
            matched_conditions.extend(matching_conditions(
                "message_contains",
                message,
                rule.message_contains_any,
            ));
            matched_conditions.extend(matching_conditions(
                "child_message_contains",
                child_messages,
                rule.child_message_contains_any,
            ));
        }
        MatchStrategy::MacroIncludeContextOrMessage => {
            let macro_include_context = node.context_chains.iter().any(|chain| {
                matches!(
                    chain.kind,
                    ContextChainKind::MacroExpansion | ContextChainKind::Include
                )
            });
            let macro_include_message = contains_any(message, rule.message_contains_any);
            let macro_include_child = contains_any(child_messages, rule.child_message_contains_any);
            if !macro_include_context && !macro_include_message && !macro_include_child {
                return None;
            }
            if macro_include_context {
                matched_conditions.push("context=macro_or_include".to_string());
            }
            matched_conditions.extend(matching_conditions(
                "message_contains",
                message,
                rule.message_contains_any,
            ));
            matched_conditions.extend(matching_conditions(
                "child_message_contains",
                child_messages,
                rule.child_message_contains_any,
            ));
        }
        MatchStrategy::PassthroughRole => {
            if !matches!(node.semantic_role, SemanticRole::Passthrough) {
                return None;
            }
            matched_conditions.push("semantic_role=passthrough".to_string());
        }
        MatchStrategy::MessageContains => {
            if !contains_any(message, rule.message_contains_any) {
                return None;
            }
            matched_conditions.extend(matching_conditions(
                "message_contains",
                message,
                rule.message_contains_any,
            ));
        }
    }

    Some(matched_conditions)
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
