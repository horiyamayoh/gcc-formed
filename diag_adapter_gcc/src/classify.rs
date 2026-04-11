//! Classification and inference helpers for diagnostic family assignment.

use diag_core::{ContextChain, ContextChainKind, Phase, SemanticRole};
use serde_json::Value;

#[derive(Debug, Clone)]
pub(crate) struct AdapterFamilyDecision {
    pub(crate) family: String,
    pub(crate) rule_id: String,
    pub(crate) matched_conditions: Vec<String>,
    pub(crate) suppression_reason: Option<String>,
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

pub(crate) fn classify_family_seed(message: &str) -> AdapterFamilyDecision {
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

pub(crate) fn first_action_hint(family: &str) -> String {
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

pub(crate) fn infer_phase(message: &str, context_chains: &[ContextChain]) -> Phase {
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

pub(crate) fn infer_related_role(message: &str) -> SemanticRole {
    let lowered = message.to_lowercase();
    if lowered.contains("candidate:") || is_numbered_candidate_message(&lowered) {
        SemanticRole::Candidate
    } else {
        SemanticRole::Supporting
    }
}

pub(crate) fn infer_related_phase(message: &str) -> Phase {
    let lowered = message.to_lowercase();
    if lowered.contains("template") || lowered.contains("deduction/substitution") {
        Phase::Instantiate
    } else {
        Phase::Semantic
    }
}

pub(crate) fn is_candidate_count_message(message: &str) -> bool {
    let lowered = message.trim().to_lowercase();
    if let Some(rest) = lowered.strip_prefix("there are ") {
        return rest.ends_with(" candidates");
    }
    lowered == "there is 1 candidate"
}

pub(crate) fn is_numbered_candidate_message(message: &str) -> bool {
    let Some(rest) = message.trim().strip_prefix("candidate ") else {
        return false;
    };
    let digit_len = rest.chars().take_while(|ch| ch.is_ascii_digit()).count();
    digit_len > 0 && rest[digit_len..].starts_with(':')
}

pub(crate) fn related_messages(result: &Value) -> Vec<String> {
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

pub(crate) fn combined_message_seed(raw_text: &str, related_messages: &[String]) -> String {
    let mut parts = vec![raw_text.to_string()];
    parts.extend(related_messages.iter().cloned());
    parts.join("\n")
}
