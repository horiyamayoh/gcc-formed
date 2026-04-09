use diag_core::{Confidence, ContextChainKind, DiagnosticNode, Ownership, Phase, SemanticRole};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FamilyDecision {
    pub(crate) family: String,
    pub(crate) rule_id: String,
    pub(crate) matched_conditions: Vec<String>,
    pub(crate) suppression_reason: Option<String>,
}

type RuleMatcher = fn(&RuleInput<'_>) -> Option<Vec<String>>;

#[derive(Debug, Clone, Copy)]
struct FamilyRule {
    id: &'static str,
    family: &'static str,
    matcher: RuleMatcher,
}

const LINKER_MESSAGE_TERMS: &[&str] = &[
    "undefined reference",
    "multiple definition",
    "cannot find -l",
    "cannot find",
];
const TEMPLATE_MESSAGE_TERMS: &[&str] =
    &["template", "deduction/substitution", "deduced conflicting"];
const MACRO_MESSAGE_TERMS: &[&str] = &["macro"];
const INCLUDE_MESSAGE_TERMS: &[&str] = &["include", "included from"];
const TYPE_OVERLOAD_MESSAGE_TERMS: &[&str] = &[
    "cannot convert",
    "invalid conversion",
    "no matching",
    "candidate",
    "incompatible type",
    "passing argument",
];
const SYNTAX_MESSAGE_TERMS: &[&str] = &["expected", "before", "missing"];

const FAMILY_RULES: &[FamilyRule] = &[
    FamilyRule {
        id: "rule.family.linker.structured_or_message",
        family: "linker",
        matcher: match_linker,
    },
    FamilyRule {
        id: "rule.family.template.structured_or_message",
        family: "template",
        matcher: match_template,
    },
    FamilyRule {
        id: "rule.family.macro_include.structured_or_message",
        family: "macro_include",
        matcher: match_macro_include,
    },
    FamilyRule {
        id: "rule.family.type_overload.structured_or_message",
        family: "type_overload",
        matcher: match_type_overload,
    },
    FamilyRule {
        id: "rule.family.syntax.phase_or_message",
        family: "syntax",
        matcher: match_syntax,
    },
    FamilyRule {
        id: "rule.family.passthrough.semantic_role",
        family: "passthrough",
        matcher: match_passthrough,
    },
];

#[derive(Debug)]
struct RuleInput<'a> {
    node: &'a DiagnosticNode,
    message: String,
    child_messages: String,
    primary_ownership: Option<&'a Ownership>,
    has_user_owned_location: bool,
    has_template_context: bool,
    has_macro_context: bool,
    has_include_context: bool,
    has_linker_context: bool,
    has_symbol_context: bool,
    has_candidate_child: bool,
    has_template_child: bool,
    has_macro_child: bool,
    has_include_child: bool,
}

impl<'a> RuleInput<'a> {
    fn from(node: &'a DiagnosticNode) -> Self {
        let message = node.message.raw_text.to_lowercase();
        let child_messages = node
            .children
            .iter()
            .map(|child| child.message.raw_text.to_lowercase())
            .collect::<Vec<_>>()
            .join("\n");

        Self {
            node,
            message,
            child_messages,
            primary_ownership: node
                .primary_location()
                .and_then(|location| location.ownership.as_ref()),
            has_user_owned_location: node
                .locations
                .iter()
                .any(|location| location.ownership == Some(Ownership::User)),
            has_template_context: node
                .context_chains
                .iter()
                .any(|chain| matches!(chain.kind, ContextChainKind::TemplateInstantiation)),
            has_macro_context: node
                .context_chains
                .iter()
                .any(|chain| matches!(chain.kind, ContextChainKind::MacroExpansion)),
            has_include_context: node
                .context_chains
                .iter()
                .any(|chain| matches!(chain.kind, ContextChainKind::Include)),
            has_linker_context: node
                .context_chains
                .iter()
                .any(|chain| matches!(chain.kind, ContextChainKind::LinkerResolution)),
            has_symbol_context: node.symbol_context.is_some(),
            has_candidate_child: node.children.iter().any(|child| {
                matches!(child.semantic_role, SemanticRole::Candidate)
                    || has_any_pattern(&child.message.raw_text.to_lowercase(), &["candidate "])
            }),
            has_template_child: node.children.iter().any(|child| {
                has_any_pattern(
                    &child.message.raw_text.to_lowercase(),
                    TEMPLATE_MESSAGE_TERMS,
                )
            }),
            has_macro_child: node.children.iter().any(|child| {
                has_any_pattern(&child.message.raw_text.to_lowercase(), MACRO_MESSAGE_TERMS)
            }),
            has_include_child: node.children.iter().any(|child| {
                has_any_pattern(
                    &child.message.raw_text.to_lowercase(),
                    INCLUDE_MESSAGE_TERMS,
                )
            }),
        }
    }

    fn message_conditions(&self, prefix: &str, patterns: &[&str]) -> Vec<String> {
        matching_conditions(prefix, &self.message, patterns)
    }

    fn child_message_conditions(&self, prefix: &str, patterns: &[&str]) -> Vec<String> {
        matching_conditions(prefix, &self.child_messages, patterns)
    }
}

pub(crate) fn classify_family(node: &DiagnosticNode) -> FamilyDecision {
    let input = RuleInput::from(node);

    for rule in FAMILY_RULES {
        if let Some(matched_conditions) = (rule.matcher)(&input) {
            return finalize_family_decision(node, rule, matched_conditions);
        }
    }

    finalize_unknown_family(node)
}

pub(crate) fn classify_confidence(node: &DiagnosticNode, decision: &FamilyDecision) -> Confidence {
    if matches!(decision.family.as_str(), "passthrough" | "unknown") {
        return Confidence::Low;
    }

    let input = RuleInput::from(node);
    let lexical_signal_count = lexical_signal_count(decision);
    let has_structured_signal = has_structured_signal(decision);
    let has_specific_family = decision
        .matched_conditions
        .iter()
        .any(|condition| condition.starts_with("existing_specific_family="));

    match decision.family.as_str() {
        "syntax" => {
            if input.has_user_owned_location
                && matches!(node.phase, Phase::Parse)
                && lexical_signal_count > 0
            {
                Confidence::High
            } else if matches!(node.phase, Phase::Parse)
                || (input.has_user_owned_location && lexical_signal_count > 0)
            {
                Confidence::Medium
            } else {
                Confidence::Low
            }
        }
        "type_overload" => {
            if input.has_user_owned_location && input.has_candidate_child {
                Confidence::High
            } else if input.has_candidate_child || lexical_signal_count > 0 {
                Confidence::Medium
            } else {
                Confidence::Low
            }
        }
        "template" => {
            if input.has_user_owned_location
                && (input.has_template_context || input.has_template_child || has_specific_family)
            {
                Confidence::High
            } else if input.has_template_context
                || input.has_template_child
                || lexical_signal_count > 0
                || has_specific_family
            {
                Confidence::Medium
            } else {
                Confidence::Low
            }
        }
        "macro_include" => {
            if input.has_user_owned_location
                && (input.has_macro_context
                    || input.has_include_context
                    || input.has_macro_child
                    || input.has_include_child)
            {
                Confidence::High
            } else if input.has_macro_context
                || input.has_include_context
                || input.has_macro_child
                || input.has_include_child
                || lexical_signal_count > 0
            {
                Confidence::Medium
            } else {
                Confidence::Low
            }
        }
        family if family.starts_with("linker") => {
            if input.has_user_owned_location && (input.has_symbol_context || has_specific_family) {
                Confidence::High
            } else if matches!(node.phase, Phase::Link)
                || input.has_linker_context
                || input.has_symbol_context
                || has_specific_family
                || lexical_signal_count > 0
            {
                Confidence::Medium
            } else {
                Confidence::Low
            }
        }
        _ => {
            if matches!(input.primary_ownership, Some(Ownership::User)) && has_structured_signal {
                Confidence::High
            } else if has_structured_signal || lexical_signal_count > 0 {
                Confidence::Medium
            } else {
                Confidence::Low
            }
        }
    }
}

fn match_linker(input: &RuleInput<'_>) -> Option<Vec<String>> {
    let mut matched_conditions = Vec::new();

    if matches!(input.node.phase, Phase::Link) {
        matched_conditions.push("phase=link".to_string());
    }
    if input.has_linker_context {
        matched_conditions.push("context=linker_resolution".to_string());
    }
    if input.has_symbol_context {
        matched_conditions.push("symbol_context=present".to_string());
    }
    matched_conditions.extend(input.message_conditions("message_contains", LINKER_MESSAGE_TERMS));

    (!matched_conditions.is_empty()).then_some(matched_conditions)
}

fn match_template(input: &RuleInput<'_>) -> Option<Vec<String>> {
    let mut matched_conditions = Vec::new();
    let message_conditions = input.message_conditions("message_contains", TEMPLATE_MESSAGE_TERMS);
    let child_conditions =
        input.child_message_conditions("child_message_contains", TEMPLATE_MESSAGE_TERMS);

    if input.has_template_context {
        matched_conditions.push("context=template_instantiation".to_string());
    }
    if input.has_template_child {
        matched_conditions.push("child_note_kind=template_context".to_string());
    }
    if matches!(input.node.phase, Phase::Instantiate)
        && (input.has_template_context
            || input.has_template_child
            || !message_conditions.is_empty()
            || !child_conditions.is_empty())
    {
        matched_conditions.push("phase=instantiate".to_string());
    }
    matched_conditions.extend(message_conditions);
    matched_conditions.extend(child_conditions);

    (!matched_conditions.is_empty()).then_some(matched_conditions)
}

fn match_macro_include(input: &RuleInput<'_>) -> Option<Vec<String>> {
    let mut matched_conditions = Vec::new();
    let message_conditions = input.message_conditions("message_contains", MACRO_MESSAGE_TERMS);
    let include_message_conditions =
        input.message_conditions("message_contains", INCLUDE_MESSAGE_TERMS);
    let child_macro_conditions =
        input.child_message_conditions("child_message_contains", MACRO_MESSAGE_TERMS);
    let child_include_conditions =
        input.child_message_conditions("child_message_contains", INCLUDE_MESSAGE_TERMS);

    if input.has_macro_context {
        matched_conditions.push("context=macro_expansion".to_string());
    }
    if input.has_include_context {
        matched_conditions.push("context=include".to_string());
    }
    if input.has_macro_child {
        matched_conditions.push("child_note_kind=macro_expansion".to_string());
    }
    if input.has_include_child {
        matched_conditions.push("child_note_kind=include".to_string());
    }
    matched_conditions.extend(message_conditions);
    matched_conditions.extend(include_message_conditions);
    matched_conditions.extend(child_macro_conditions);
    matched_conditions.extend(child_include_conditions);

    (!matched_conditions.is_empty()).then_some(matched_conditions)
}

fn match_type_overload(input: &RuleInput<'_>) -> Option<Vec<String>> {
    let mut matched_conditions = Vec::new();
    let message_conditions =
        input.message_conditions("message_contains", TYPE_OVERLOAD_MESSAGE_TERMS);

    if input.has_candidate_child {
        matched_conditions.push("child_role=candidate".to_string());
    }
    if matches!(input.node.phase, Phase::Semantic) && !message_conditions.is_empty() {
        matched_conditions.push("phase=semantic".to_string());
    }
    if matches!(input.node.phase, Phase::Instantiate)
        && (input.has_candidate_child || !message_conditions.is_empty())
    {
        matched_conditions.push("phase=instantiate".to_string());
    }
    matched_conditions.extend(message_conditions);

    (!matched_conditions.is_empty()).then_some(matched_conditions)
}

fn match_syntax(input: &RuleInput<'_>) -> Option<Vec<String>> {
    let mut matched_conditions = input.message_conditions("message_contains", SYNTAX_MESSAGE_TERMS);
    if matched_conditions.is_empty() {
        return None;
    }
    if matches!(input.node.phase, Phase::Parse) {
        matched_conditions.push("phase=parse".to_string());
    }
    Some(matched_conditions)
}

fn match_passthrough(input: &RuleInput<'_>) -> Option<Vec<String>> {
    matches!(input.node.semantic_role, SemanticRole::Passthrough)
        .then(|| vec!["semantic_role=passthrough".to_string()])
}

fn finalize_family_decision(
    node: &DiagnosticNode,
    rule: &FamilyRule,
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

fn has_structured_signal(decision: &FamilyDecision) -> bool {
    decision.matched_conditions.iter().any(|condition| {
        condition.starts_with("phase=")
            || condition.starts_with("context=")
            || condition.starts_with("semantic_role=")
            || condition.starts_with("symbol_context=")
            || condition.starts_with("child_role=")
            || condition.starts_with("child_note_kind=")
            || condition.starts_with("existing_specific_family=")
    })
}

fn lexical_signal_count(decision: &FamilyDecision) -> usize {
    decision
        .matched_conditions
        .iter()
        .filter(|condition| {
            condition.starts_with("message_contains=")
                || condition.starts_with("child_message_contains=")
        })
        .count()
}

fn has_any_pattern(haystack: &str, patterns: &[&str]) -> bool {
    patterns.iter().any(|pattern| haystack.contains(pattern))
}

fn matching_conditions(prefix: &str, haystack: &str, patterns: &[&str]) -> Vec<String> {
    patterns
        .iter()
        .filter(|pattern| haystack.contains(**pattern))
        .map(|pattern| format!("{prefix}={pattern}"))
        .collect()
}
