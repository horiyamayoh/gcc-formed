use diag_core::{Confidence, ContextChainKind, DiagnosticNode, Ownership, Phase, SemanticRole};
use diag_rulepack::{
    ChildNoteConditionKind, ConfidenceClauseConfig, ConfidencePolicyConfig, ConfidenceSignal,
    ContextConditionKind, EnrichRulepack, FamilyRuleConfig, MatchStrategyConfig,
    PhaseAnnotationWhen, TermGroupConfig, checked_in_rulepack,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FamilyDecision {
    pub(crate) family: String,
    pub(crate) rule_id: String,
    pub(crate) matched_conditions: Vec<String>,
    pub(crate) suppression_reason: Option<String>,
}

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
        let rulepack = rulepack();
        let template_rule = rulepack.rule("template");
        let macro_include_rule = rulepack.rule("macro_include");
        let type_overload_rule = rulepack.rule("type_overload");

        let message = node.message.raw_text.to_lowercase();
        let child_messages = node
            .children
            .iter()
            .map(|child| child.message.raw_text.to_lowercase())
            .collect::<Vec<_>>()
            .join("\n");

        let macro_terms = macro_include_rule
            .child_message_groups
            .first()
            .map(|group| group.terms.as_slice())
            .unwrap_or(&[]);
        let include_terms = macro_include_rule
            .child_message_groups
            .get(1)
            .map(|group| group.terms.as_slice())
            .unwrap_or(&[]);

        Self {
            node,
            message,
            child_messages,
            primary_ownership: node
                .primary_location()
                .and_then(|location| location.ownership()),
            has_user_owned_location: node
                .locations
                .iter()
                .any(|location| location.ownership() == Some(&Ownership::User)),
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
                    || has_any_pattern(
                        &child.message.raw_text.to_lowercase(),
                        &type_overload_rule.candidate_child_terms,
                    )
            }),
            has_template_child: node.children.iter().any(|child| {
                has_any_pattern(
                    &child.message.raw_text.to_lowercase(),
                    &flatten_terms(&template_rule.child_message_groups),
                )
            }),
            has_macro_child: node
                .children
                .iter()
                .any(|child| has_any_pattern(&child.message.raw_text.to_lowercase(), macro_terms)),
            has_include_child: node.children.iter().any(|child| {
                has_any_pattern(&child.message.raw_text.to_lowercase(), include_terms)
            }),
        }
    }

    fn message_conditions(&self, group: &TermGroupConfig) -> Vec<String> {
        matching_conditions(&group.prefix, &self.message, &group.terms)
    }

    fn child_message_conditions(&self, group: &TermGroupConfig) -> Vec<String> {
        matching_conditions(&group.prefix, &self.child_messages, &group.terms)
    }

    fn has_context(&self, kind: ContextConditionKind) -> bool {
        match kind {
            ContextConditionKind::TemplateInstantiation => self.has_template_context,
            ContextConditionKind::MacroExpansion => self.has_macro_context,
            ContextConditionKind::Include => self.has_include_context,
            ContextConditionKind::LinkerResolution => self.has_linker_context,
        }
    }

    fn has_child_note_kind(&self, kind: ChildNoteConditionKind) -> bool {
        match kind {
            ChildNoteConditionKind::TemplateContext => self.has_template_child,
            ChildNoteConditionKind::MacroExpansion => self.has_macro_child,
            ChildNoteConditionKind::Include => self.has_include_child,
        }
    }
}

pub(crate) fn classify_family(node: &DiagnosticNode) -> FamilyDecision {
    let input = RuleInput::from(node);

    for rule in &rulepack().rules {
        if let Some(matched_conditions) = match_family_rule(&input, rule) {
            return finalize_family_decision(node, rule, matched_conditions);
        }
    }

    finalize_unknown_family(node)
}

pub(crate) fn classify_confidence(node: &DiagnosticNode, decision: &FamilyDecision) -> Confidence {
    let input = RuleInput::from(node);
    evaluate_confidence_policy(
        &input,
        decision,
        rulepack().confidence_policy_for(decision.family.as_str()),
    )
}

fn rulepack() -> &'static EnrichRulepack {
    checked_in_rulepack().enrich()
}

fn match_family_rule(input: &RuleInput<'_>, rule: &FamilyRuleConfig) -> Option<Vec<String>> {
    match (rule.match_strategy, rule.family.as_str()) {
        (MatchStrategyConfig::StructuredOrMessage, "linker") => match_linker(input, rule),
        (MatchStrategyConfig::StructuredOrMessage, "template") => match_template(input, rule),
        (MatchStrategyConfig::StructuredOrMessage, "macro_include") => {
            match_macro_include(input, rule)
        }
        (MatchStrategyConfig::StructuredOrMessage, "type_overload") => {
            match_type_overload(input, rule)
        }
        (MatchStrategyConfig::PhaseOrMessage, "syntax") => match_syntax(input, rule),
        (MatchStrategyConfig::SemanticRole, "passthrough") => match_passthrough(input, rule),
        _ => None,
    }
}

fn match_linker(input: &RuleInput<'_>, rule: &FamilyRuleConfig) -> Option<Vec<String>> {
    let message_conditions = collect_group_conditions(input, &rule.message_groups, false);
    let has_match = matches!(input.node.phase, Phase::Link)
        || input.has_linker_context
        || input.has_symbol_context
        || !message_conditions.is_empty();
    if !has_match {
        return None;
    }

    let mut matched_conditions = Vec::new();
    push_phase_annotations(
        &mut matched_conditions,
        rule,
        &input.node.phase,
        PhaseAnnotationWhen::RuleMatched,
    );
    push_context_conditions(&mut matched_conditions, input, rule);
    if let Some(condition) = &rule.symbol_context_condition
        && input.has_symbol_context
    {
        matched_conditions.push(condition.clone());
    }
    matched_conditions.extend(message_conditions);
    Some(matched_conditions)
}

fn match_template(input: &RuleInput<'_>, rule: &FamilyRuleConfig) -> Option<Vec<String>> {
    let message_conditions = collect_group_conditions(input, &rule.message_groups, false);
    let child_conditions = collect_group_conditions(input, &rule.child_message_groups, true);
    let has_match = input.has_template_context
        || input.has_template_child
        || !message_conditions.is_empty()
        || !child_conditions.is_empty();
    if !has_match {
        return None;
    }

    let mut matched_conditions = Vec::new();
    push_context_conditions(&mut matched_conditions, input, rule);
    push_child_note_conditions(&mut matched_conditions, input, rule);
    push_phase_annotations(
        &mut matched_conditions,
        rule,
        &input.node.phase,
        PhaseAnnotationWhen::RuleMatched,
    );
    matched_conditions.extend(message_conditions);
    matched_conditions.extend(child_conditions);
    Some(matched_conditions)
}

fn match_macro_include(input: &RuleInput<'_>, rule: &FamilyRuleConfig) -> Option<Vec<String>> {
    let mut matched_conditions = Vec::new();
    push_context_conditions(&mut matched_conditions, input, rule);
    push_child_note_conditions(&mut matched_conditions, input, rule);
    matched_conditions.extend(collect_group_conditions(input, &rule.message_groups, false));
    matched_conditions.extend(collect_group_conditions(
        input,
        &rule.child_message_groups,
        true,
    ));
    (!matched_conditions.is_empty()).then_some(matched_conditions)
}

fn match_type_overload(input: &RuleInput<'_>, rule: &FamilyRuleConfig) -> Option<Vec<String>> {
    let message_conditions = collect_group_conditions(input, &rule.message_groups, false);
    let has_message_terms = !message_conditions.is_empty();
    let has_match = input.has_candidate_child || has_message_terms;
    if !has_match {
        return None;
    }

    let mut matched_conditions = Vec::new();
    if let Some(condition) = &rule.candidate_child_condition
        && input.has_candidate_child
    {
        matched_conditions.push(condition.clone());
    }
    if has_message_terms {
        push_phase_annotations(
            &mut matched_conditions,
            rule,
            &input.node.phase,
            PhaseAnnotationWhen::MessageTerms,
        );
    }
    if input.has_candidate_child || has_message_terms {
        push_phase_annotations(
            &mut matched_conditions,
            rule,
            &input.node.phase,
            PhaseAnnotationWhen::MessageOrCandidate,
        );
    }
    matched_conditions.extend(message_conditions);
    Some(matched_conditions)
}

fn match_syntax(input: &RuleInput<'_>, rule: &FamilyRuleConfig) -> Option<Vec<String>> {
    let message_conditions = collect_group_conditions(input, &rule.message_groups, false);
    if message_conditions.is_empty() {
        return None;
    }

    let mut matched_conditions = message_conditions;
    push_phase_annotations(
        &mut matched_conditions,
        rule,
        &input.node.phase,
        PhaseAnnotationWhen::RuleMatched,
    );
    Some(matched_conditions)
}

fn match_passthrough(input: &RuleInput<'_>, rule: &FamilyRuleConfig) -> Option<Vec<String>> {
    if !matches!(input.node.semantic_role, SemanticRole::Passthrough) {
        return None;
    }
    Some(
        rule.semantic_role_condition
            .iter()
            .cloned()
            .collect::<Vec<_>>(),
    )
}

fn finalize_family_decision(
    node: &DiagnosticNode,
    rule: &FamilyRuleConfig,
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
            rule_id: rulepack().ingress_specific_override_rule_id.clone(),
            matched_conditions,
            suppression_reason: Some("preserved_specific_family_from_ingress".to_string()),
        }
    } else {
        FamilyDecision {
            family: rule.family.clone(),
            rule_id: rule.rule_id.clone(),
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
            rule_id: rulepack().ingress_specific_override_rule_id.clone(),
            matched_conditions: vec!["derived_family=unknown".to_string()],
            suppression_reason: Some("preserved_specific_family_from_ingress".to_string()),
        };
    }

    let fallback = if matches!(node.semantic_role, SemanticRole::Passthrough) {
        &rulepack().passthrough_fallback
    } else {
        &rulepack().unknown_fallback
    };

    FamilyDecision {
        family: fallback.family.clone(),
        rule_id: fallback.rule_id.clone(),
        matched_conditions: fallback.matched_conditions.clone(),
        suppression_reason: Some(fallback.suppression_reason.clone()),
    }
}

fn evaluate_confidence_policy(
    input: &RuleInput<'_>,
    decision: &FamilyDecision,
    policy: &ConfidencePolicyConfig,
) -> Confidence {
    if let Some(fixed) = policy.fixed {
        return fixed.into();
    }

    if policy
        .high_when_any
        .iter()
        .any(|clause| confidence_clause_matches(input, decision, clause))
    {
        Confidence::High
    } else if policy
        .medium_when_any
        .iter()
        .any(|clause| confidence_clause_matches(input, decision, clause))
    {
        Confidence::Medium
    } else {
        policy.default_confidence.into()
    }
}

fn confidence_clause_matches(
    input: &RuleInput<'_>,
    decision: &FamilyDecision,
    clause: &ConfidenceClauseConfig,
) -> bool {
    clause
        .all
        .iter()
        .all(|signal| confidence_signal_matches(input, decision, *signal))
}

fn confidence_signal_matches(
    input: &RuleInput<'_>,
    decision: &FamilyDecision,
    signal: ConfidenceSignal,
) -> bool {
    match signal {
        ConfidenceSignal::UserOwnedLocation => input.has_user_owned_location,
        ConfidenceSignal::PrimaryOwnershipUser => {
            matches!(input.primary_ownership, Some(Ownership::User))
        }
        ConfidenceSignal::PhaseParse => matches!(input.node.phase, Phase::Parse),
        ConfidenceSignal::PhaseSemantic => matches!(input.node.phase, Phase::Semantic),
        ConfidenceSignal::PhaseInstantiate => matches!(input.node.phase, Phase::Instantiate),
        ConfidenceSignal::PhaseLink => matches!(input.node.phase, Phase::Link),
        ConfidenceSignal::TemplateContext => input.has_template_context,
        ConfidenceSignal::MacroContext => input.has_macro_context,
        ConfidenceSignal::IncludeContext => input.has_include_context,
        ConfidenceSignal::LinkerContext => input.has_linker_context,
        ConfidenceSignal::SymbolContext => input.has_symbol_context,
        ConfidenceSignal::CandidateChild => input.has_candidate_child,
        ConfidenceSignal::TemplateChild => input.has_template_child,
        ConfidenceSignal::MacroChild => input.has_macro_child,
        ConfidenceSignal::IncludeChild => input.has_include_child,
        ConfidenceSignal::LexicalSignal => lexical_signal_count(decision) > 0,
        ConfidenceSignal::StructuredSignal => has_structured_signal(decision),
        ConfidenceSignal::ExistingSpecificFamily => decision
            .matched_conditions
            .iter()
            .any(|condition| condition.starts_with("existing_specific_family=")),
    }
}

fn push_context_conditions(
    matched_conditions: &mut Vec<String>,
    input: &RuleInput<'_>,
    rule: &FamilyRuleConfig,
) {
    for context in &rule.contexts {
        if input.has_context(context.kind) {
            matched_conditions.push(context.condition.clone());
        }
    }
}

fn push_child_note_conditions(
    matched_conditions: &mut Vec<String>,
    input: &RuleInput<'_>,
    rule: &FamilyRuleConfig,
) {
    for child_note in &rule.child_notes {
        if input.has_child_note_kind(child_note.kind) {
            matched_conditions.push(child_note.condition.clone());
        }
    }
}

fn push_phase_annotations(
    matched_conditions: &mut Vec<String>,
    rule: &FamilyRuleConfig,
    phase: &Phase,
    when: PhaseAnnotationWhen,
) {
    for annotation in &rule.phase_annotations {
        if annotation.when == when && &annotation.phase == phase {
            matched_conditions.push(annotation.condition.clone());
        }
    }
}

fn collect_group_conditions(
    input: &RuleInput<'_>,
    groups: &[TermGroupConfig],
    child: bool,
) -> Vec<String> {
    let mut matched_conditions = Vec::new();
    for group in groups {
        let conditions = if child {
            input.child_message_conditions(group)
        } else {
            input.message_conditions(group)
        };
        matched_conditions.extend(conditions);
    }
    matched_conditions
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

fn flatten_terms(groups: &[TermGroupConfig]) -> Vec<String> {
    groups
        .iter()
        .flat_map(|group| group.terms.iter().cloned())
        .collect()
}

fn has_any_pattern(haystack: &str, patterns: &[String]) -> bool {
    patterns.iter().any(|pattern| haystack.contains(pattern))
}

fn matching_conditions(prefix: &str, haystack: &str, patterns: &[String]) -> Vec<String> {
    patterns
        .iter()
        .filter(|pattern| haystack.contains(pattern.as_str()))
        .map(|pattern| format!("{prefix}={pattern}"))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loads_checked_in_enrich_rulepack() {
        let rulepack = rulepack();
        assert_eq!(rulepack.rulepack_version, "phase1");
        assert_eq!(rulepack.rules.len(), 6);
        assert!(std::ptr::eq(rulepack, checked_in_rulepack().enrich()));
        assert_eq!(
            rulepack.rule("syntax").rule_id,
            "rule.family.syntax.phase_or_message"
        );
    }
}
