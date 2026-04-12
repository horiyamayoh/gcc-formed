//! Classification and inference helpers for diagnostic family assignment.

use diag_core::{ContextChain, ContextChainKind, Phase, SemanticRole};
use diag_rulepack::{EnrichRulepack, checked_in_rulepack};
use serde_json::Value;
use std::path::Path;

#[derive(Debug, Clone)]
pub(crate) struct AdapterFamilyDecision {
    pub(crate) family: String,
    pub(crate) first_action_hint: String,
    pub(crate) rule_id: String,
    pub(crate) matched_conditions: Vec<String>,
    pub(crate) suppression_reason: Option<String>,
}

pub(crate) fn classify_family_seed(message: &str) -> AdapterFamilyDecision {
    classify_family_seed_with_rulepack(message, checked_in_rulepack().enrich())
}

fn classify_family_seed_with_rulepack(
    message: &str,
    rulepack: &EnrichRulepack,
) -> AdapterFamilyDecision {
    let lowered = message.to_lowercase();
    let mut best_match = None;

    for (index, rule) in rulepack.adapter_seed_rules.iter().enumerate() {
        let matched_conditions = rule
            .terms
            .iter()
            .filter(|needle| lowered.contains(needle.as_str()))
            .map(|needle| format!("message_contains={needle}"))
            .collect::<Vec<_>>();

        if matched_conditions.is_empty() {
            continue;
        }

        let should_replace = match best_match.as_ref() {
            None => true,
            Some((best_priority, best_index, _, _)) => {
                (rule.priority, index) < (*best_priority, *best_index)
            }
        };
        if should_replace {
            best_match = Some((rule.priority, index, rule, matched_conditions));
        }
    }

    if let Some((_, _, rule, matched_conditions)) = best_match {
        return build_family_decision(
            rule.family.clone(),
            rule.rule_id.clone(),
            matched_conditions,
            None,
        );
    }

    let fallback = &rulepack.unknown_fallback;
    build_family_decision(
        fallback.family.clone(),
        fallback.rule_id.clone(),
        fallback.matched_conditions.clone(),
        Some(fallback.suppression_reason.clone()),
    )
}

fn build_family_decision(
    family: String,
    rule_id: String,
    matched_conditions: Vec<String>,
    suppression_reason: Option<String>,
) -> AdapterFamilyDecision {
    AdapterFamilyDecision {
        first_action_hint: checked_in_rulepack()
            .residual()
            .action_hint_for_family(&family)
            .to_string(),
        family,
        rule_id,
        matched_conditions,
        suppression_reason,
    }
}

pub(crate) struct PhaseInferenceSignals<'a> {
    pub(crate) message: &'a str,
    pub(crate) context_chains: &'a [ContextChain],
    pub(crate) option: Option<&'a str>,
    pub(crate) rule_id: Option<&'a str>,
    pub(crate) tool_component: Option<&'a str>,
}

pub(crate) fn infer_phase(signals: PhaseInferenceSignals<'_>) -> Phase {
    let message = signals.message.to_lowercase();

    if has_analyzer_context(signals.context_chains)
        || is_analyzer_option(signals.option)
        || is_analyzer_option(signals.rule_id)
    {
        Phase::Analyze
    } else if is_constraints_option(signals.option) || is_constraints_option(signals.rule_id) {
        Phase::Constraints
    } else if let Some(phase) = infer_tool_phase(signals.tool_component, &message) {
        phase
    } else if is_constraints_message(&message) {
        Phase::Constraints
    } else if is_preprocess_message(&message, signals.context_chains) {
        Phase::Preprocess
    } else if is_driver_message(&message) {
        Phase::Driver
    } else if is_assemble_message(&message) {
        Phase::Assemble
    } else if let Some(phase) = infer_internal_compiler_phase(&message) {
        phase
    } else if message.contains("undefined reference") || message.contains("multiple definition") {
        Phase::Link
    } else if signals
        .context_chains
        .iter()
        .any(|chain| matches!(chain.kind, ContextChainKind::TemplateInstantiation))
    {
        Phase::Instantiate
    } else if is_parse_message(&message) {
        Phase::Parse
    } else {
        Phase::Semantic
    }
}

fn has_analyzer_context(context_chains: &[ContextChain]) -> bool {
    context_chains
        .iter()
        .any(|chain| matches!(chain.kind, ContextChainKind::AnalyzerPath))
}

fn is_analyzer_option(option: Option<&str>) -> bool {
    option
        .map(str::trim)
        .map(str::to_ascii_lowercase)
        .is_some_and(|option| option == "-fanalyzer" || option.starts_with("-wanalyzer-"))
}

fn is_constraints_option(option: Option<&str>) -> bool {
    option
        .map(str::trim)
        .map(str::to_ascii_lowercase)
        .is_some_and(|option| option == "-wconcepts")
}

fn infer_tool_phase(tool_component: Option<&str>, message: &str) -> Option<Phase> {
    let normalized = normalize_tool_component(tool_component?)?;

    if normalized == "as" {
        return Some(Phase::Assemble);
    }
    if normalized == "collect2" || normalized.starts_with("ld") {
        return Some(Phase::Link);
    }
    if (normalized == "gcc" || normalized == "g++") && is_driver_message(message) {
        return Some(Phase::Driver);
    }
    if normalized == "cc1" || normalized == "cc1plus" {
        return infer_internal_compiler_phase(message);
    }

    None
}

fn normalize_tool_component(tool_component: &str) -> Option<String> {
    let trimmed = tool_component.trim();
    if trimmed.is_empty() {
        return None;
    }

    let component = Path::new(trimmed)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(trimmed);

    Some(component.to_ascii_lowercase())
}

fn is_constraints_message(message: &str) -> bool {
    message.contains("constraints not satisfied")
        || message.contains(" because ")
            && (message.contains("concept") || message.contains("constraint"))
        || message.contains("concept")
}

fn is_preprocess_message(message: &str, context_chains: &[ContextChain]) -> bool {
    if message.contains("#error") || message.contains("#warning") {
        return true;
    }

    let has_include_context = context_chains
        .iter()
        .any(|chain| matches!(chain.kind, ContextChainKind::Include));
    has_include_context
        && message.contains("no such file or directory")
        && (message.contains("#include") || message.contains("included from"))
}

fn is_driver_message(message: &str) -> bool {
    message.contains("no input files")
        || message.contains("unrecognized command-line option")
        || message.contains("unrecognized option")
        || message.starts_with("gcc: fatal error:")
        || message.starts_with("g++: fatal error:")
}

fn is_assemble_message(message: &str) -> bool {
    message.starts_with("as:")
        || message.starts_with("assembler:")
        || message.contains("assembler messages")
}

fn infer_internal_compiler_phase(message: &str) -> Option<Phase> {
    if message.contains("during gimple pass")
        || message.contains("during ipa pass")
        || message.contains("optimization pass")
        || message.contains("optimizing")
    {
        Some(Phase::Optimize)
    } else if message.contains("during rtl pass")
        || message.contains("during code generation")
        || message.contains("during expand")
    {
        Some(Phase::Codegen)
    } else {
        None
    }
}

fn is_parse_message(message: &str) -> bool {
    message.contains(" before ")
        || message.contains(" at end of input")
        || message.contains("expected declaration or statement")
        || message.contains("expected expression")
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
    if lowered.contains("template")
        || lowered.contains("required from")
        || lowered.contains("required by substitution")
        || lowered.contains("deduction/substitution")
        || lowered.contains("deduced conflicting")
    {
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
        .filter_map(|location| structured_message_text(location.get("message")))
        .collect()
}

pub(crate) fn combined_message_seed(raw_text: &str, related_messages: &[String]) -> String {
    let mut parts = vec![raw_text.to_string()];
    parts.extend(related_messages.iter().cloned());
    parts.join("\n")
}

pub(crate) fn structured_message_text(message: Option<&Value>) -> Option<String> {
    let message = message?;
    if let Some(text) = message.as_str().filter(|text| !text.trim().is_empty()) {
        return Some(text.to_string());
    }

    message
        .get("text")
        .and_then(Value::as_str)
        .filter(|text| !text.trim().is_empty())
        .or_else(|| {
            message
                .get("markdown")
                .and_then(Value::as_str)
                .filter(|text| !text.trim().is_empty())
        })
        .map(ToString::to_string)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn infer(message: &str) -> Phase {
        infer_phase(PhaseInferenceSignals {
            message,
            context_chains: &[],
            option: None,
            rule_id: None,
            tool_component: None,
        })
    }

    #[test]
    fn infers_analyze_from_warning_option() {
        assert_eq!(
            infer_phase(PhaseInferenceSignals {
                message: "dereference of NULL 'ptr'",
                context_chains: &[],
                option: Some("-Wanalyzer-null-dereference"),
                rule_id: None,
                tool_component: None,
            }),
            Phase::Analyze
        );
    }

    #[test]
    fn infers_constraints_from_rule_id() {
        assert_eq!(
            infer_phase(PhaseInferenceSignals {
                message: "no matching function for call to 'consume(int)'",
                context_chains: &[],
                option: None,
                rule_id: Some("-Wconcepts"),
                tool_component: None,
            }),
            Phase::Constraints
        );
    }

    #[test]
    fn infers_preprocess_from_directive_message() {
        assert_eq!(infer("#error stop here"), Phase::Preprocess);
        assert_eq!(infer("#warning deprecated branch"), Phase::Preprocess);
    }

    #[test]
    fn infers_driver_from_message_pattern() {
        assert_eq!(infer("gcc: fatal error: no input files"), Phase::Driver);
        assert_eq!(
            infer("gcc: error: unrecognized option '--wat'"),
            Phase::Driver
        );
    }

    #[test]
    fn infers_assemble_from_tool_component() {
        assert_eq!(
            infer_phase(PhaseInferenceSignals {
                message: "fatal error: Killed signal terminated program as",
                context_chains: &[],
                option: None,
                rule_id: None,
                tool_component: Some("/usr/bin/as"),
            }),
            Phase::Assemble
        );
    }

    #[test]
    fn infers_internal_compiler_phase_from_ice_context() {
        assert_eq!(
            infer_phase(PhaseInferenceSignals {
                message: "internal compiler error: Segmentation fault\nduring gimple pass: vrp",
                context_chains: &[],
                option: None,
                rule_id: None,
                tool_component: Some("cc1"),
            }),
            Phase::Optimize
        );
        assert_eq!(
            infer_phase(PhaseInferenceSignals {
                message: "internal compiler error: unexpected failure\nduring rtl pass: expand",
                context_chains: &[],
                option: None,
                rule_id: None,
                tool_component: Some("cc1plus"),
            }),
            Phase::Codegen
        );
    }

    #[test]
    fn preserves_existing_link_instantiate_parse_and_semantic_paths() {
        assert_eq!(
            infer("undefined reference to `missing_symbol`"),
            Phase::Link
        );
        assert_eq!(
            infer_phase(PhaseInferenceSignals {
                message: "no matching function for call to 'consume(int)'",
                context_chains: &[ContextChain {
                    kind: ContextChainKind::TemplateInstantiation,
                    frames: Vec::new(),
                }],
                option: None,
                rule_id: None,
                tool_component: None,
            }),
            Phase::Instantiate
        );
        assert_eq!(infer("expected ';' before '}' token"), Phase::Parse);
        assert_eq!(
            infer(
                "passing argument 1 of 'takes_int' makes integer from pointer without a cast\nexpected 'int' but argument is of type 'const char *'"
            ),
            Phase::Semantic
        );
        assert_eq!(
            infer("invalid conversion from 'int' to 'char *'"),
            Phase::Semantic
        );
    }

    #[test]
    fn classifies_family_seed_from_checked_in_rulepack() {
        let decision =
            classify_family_seed("undefined reference to `missing_symbol`\nrequired from here");

        assert_eq!(decision.family, "linker.undefined_reference");
        assert_eq!(
            decision.rule_id,
            "rule.family_seed.linker.undefined_reference"
        );
        assert_eq!(
            decision.matched_conditions,
            vec!["message_contains=undefined reference".to_string()]
        );
    }

    #[test]
    fn uses_rulepack_action_hint_for_specific_linker_seed() {
        let decision =
            classify_family_seed("helper.c:(.text+0x0): multiple definition of `duplicate_symbol'");

        assert_eq!(decision.family, "linker.multiple_definition");
        assert_eq!(
            decision.first_action_hint,
            "remove the duplicate definition or make the symbol internal to one translation unit"
        );
    }

    #[test]
    fn classifies_scope_declaration_seed_from_rulepack() {
        let decision = classify_family_seed("'missing_value' was not declared in this scope");

        assert_eq!(decision.family, "scope_declaration");
        assert_eq!(decision.rule_id, "rule.family_seed.scope_declaration");
        assert_eq!(
            decision.first_action_hint,
            "check for typos, missing #include, or missing namespace qualifier"
        );
    }

    #[test]
    fn classifies_concepts_constraints_seed_from_combined_message() {
        let decision = classify_family_seed(
            "no matching function for call to 'consume(1)'\nconstraints not satisfied",
        );

        assert_eq!(decision.family, "concepts_constraints");
        assert_eq!(decision.rule_id, "rule.family_seed.concepts_constraints");
        assert_eq!(
            decision.first_action_hint,
            "match the required concept or requires-clause against the actual template arguments"
        );
    }

    #[test]
    fn classifies_preprocessor_directive_seed_before_macro_terms() {
        let decision = classify_family_seed("#error macro guard missing");

        assert_eq!(decision.family, "preprocessor_directive");
        assert_eq!(decision.rule_id, "rule.family_seed.preprocessor_directive");
    }

    #[test]
    fn classifies_unused_seed_from_rulepack() {
        let decision = classify_family_seed("unused variable 'temporary' [-Wunused-variable]");

        assert_eq!(decision.family, "unused");
        assert_eq!(decision.rule_id, "rule.family_seed.unused");
        assert_eq!(
            decision.first_action_hint,
            "remove the unused declaration or prefix with underscore if intentional"
        );
    }

    #[test]
    fn classifies_analyzer_seed_from_rulepack() {
        let decision =
            classify_family_seed("double-'free' of 'ptr' [CWE-415] [-Wanalyzer-double-free]");

        assert_eq!(decision.family, "analyzer");
        assert_eq!(decision.rule_id, "rule.family_seed.analyzer");
        assert_eq!(
            decision.first_action_hint,
            "follow the analyzer event path and fix the first invalid state transition"
        );
    }

    #[test]
    fn classifies_conversion_narrowing_seed_from_rulepack() {
        let decision = classify_family_seed(
            "comparison of integer expressions of different signedness: 'int' and 'unsigned int' [-Wsign-compare]",
        );

        assert_eq!(decision.family, "conversion_narrowing");
        assert_eq!(decision.rule_id, "rule.family_seed.conversion_narrowing");
        assert_eq!(
            decision.first_action_hint,
            "add an explicit cast or change the variable type to match"
        );
    }

    #[test]
    fn classifies_coroutine_seed_from_rulepack() {
        let decision = classify_family_seed("unable to find the promise type for this coroutine");

        assert_eq!(decision.family, "coroutine");
        assert_eq!(decision.rule_id, "rule.family_seed.coroutine");
        assert_eq!(
            decision.first_action_hint,
            "define a valid promise_type or align the coroutine return type with the coroutine body"
        );
    }

    #[test]
    fn classifies_module_import_seed_from_rulepack() {
        let decision = classify_family_seed(
            "failed to read compiled module: imports must be built before being imported",
        );

        assert_eq!(decision.family, "module_import");
        assert_eq!(decision.rule_id, "rule.family_seed.module_import");
        assert_eq!(
            decision.first_action_hint,
            "build or export the requested module interface before importing it"
        );
    }

    #[test]
    fn classifies_deprecated_seed_from_rulepack() {
        let decision = classify_family_seed(
            "'int old_api()' is deprecated: use new_api [-Wdeprecated-declarations]",
        );

        assert_eq!(decision.family, "deprecated");
        assert_eq!(decision.rule_id, "rule.family_seed.deprecated");
        assert_eq!(
            decision.first_action_hint,
            "replace the deprecated API with the recommended alternative or silence the warning intentionally"
        );
    }

    #[test]
    fn preserves_type_overload_seed_precedence_over_conversion_narrowing() {
        let decision = classify_family_seed("invalid conversion from 'const char *' to 'int'");

        assert_eq!(decision.family, "type_overload");
        assert_eq!(decision.rule_id, "rule.family_seed.type_overload");
    }

    #[test]
    fn classifies_inheritance_virtual_seed_from_rulepack() {
        let decision =
            classify_family_seed("cannot declare variable 'node' to be of abstract type 'Derived'");

        assert_eq!(decision.family, "inheritance_virtual");
        assert_eq!(decision.rule_id, "rule.family_seed.inheritance_virtual");
        assert_eq!(
            decision.first_action_hint,
            "implement all pure virtual functions or fix the class hierarchy"
        );
    }

    #[test]
    fn classifies_constexpr_seed_from_rulepack() {
        let decision = classify_family_seed("static assertion failed: int size mismatch");

        assert_eq!(decision.family, "constexpr");
        assert_eq!(decision.rule_id, "rule.family_seed.constexpr");
        assert_eq!(
            decision.first_action_hint,
            "ensure all expressions and function calls in the constexpr context are constant-evaluable"
        );
    }

    #[test]
    fn classifies_lambda_closure_seed_from_rulepack() {
        let decision = classify_family_seed("'value' is not captured");

        assert_eq!(decision.family, "lambda_closure");
        assert_eq!(decision.rule_id, "rule.family_seed.lambda_closure");
        assert_eq!(
            decision.first_action_hint,
            "add the variable to the capture list or specify a default capture mode [=] or [&]"
        );
    }

    #[test]
    fn classifies_lifetime_dangling_seed_from_rulepack() {
        let decision = classify_family_seed(
            "address of local variable 'value' returned [-Wreturn-local-addr]",
        );

        assert_eq!(decision.family, "lifetime_dangling");
        assert_eq!(decision.rule_id, "rule.family_seed.lifetime_dangling");
        assert_eq!(
            decision.first_action_hint,
            "return by value instead of by pointer/reference, or extend the object lifetime"
        );
    }

    #[test]
    fn classifies_init_order_seed_from_rulepack() {
        let decision = classify_family_seed(
            "'Example::value' will be initialized after 'int Example::count' [-Wreorder]",
        );

        assert_eq!(decision.family, "init_order");
        assert_eq!(decision.rule_id, "rule.family_seed.init_order");
        assert_eq!(
            decision.first_action_hint,
            "reorder member initializers to match declaration order, or fix the initializer list"
        );
    }

    #[test]
    fn falls_back_to_rulepack_unknown_seed() {
        let decision = classify_family_seed("this diagnostic does not match any adapter seed");

        assert_eq!(decision.family, "unknown");
        assert_eq!(decision.rule_id, "rule.family.unknown");
        assert_eq!(
            decision.matched_conditions,
            vec!["no_family_rule_matched".to_string()]
        );
        assert_eq!(
            decision.suppression_reason.as_deref(),
            Some("generic_fallback")
        );
        assert_eq!(
            decision.first_action_hint,
            "inspect the preserved raw diagnostics for the first corrective action"
        );
    }
}
