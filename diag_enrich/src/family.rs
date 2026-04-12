use diag_core::{Confidence, ContextChainKind, DiagnosticNode, Ownership, Phase, SemanticRole};
use diag_rulepack::{
    ChildNoteConditionKind, ConfidenceClauseConfig, ConfidencePolicyConfig, ConfidenceSignal,
    ContextConditionKind, EnrichRulepack, FamilyRuleConfig, MatchConditionConfig,
    MatchStrategyConfig, PhaseAnnotationWhen, TermGroupConfig, checked_in_rulepack,
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
    fn from(node: &'a DiagnosticNode, rulepack: &EnrichRulepack) -> Self {
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
    classify_family_with_rulepack(node, rulepack())
}

pub(crate) fn classify_confidence(node: &DiagnosticNode, decision: &FamilyDecision) -> Confidence {
    classify_confidence_with_rulepack(node, decision, rulepack())
}

fn rulepack() -> &'static EnrichRulepack {
    checked_in_rulepack().enrich()
}

fn classify_family_with_rulepack(
    node: &DiagnosticNode,
    rulepack: &EnrichRulepack,
) -> FamilyDecision {
    let input = RuleInput::from(node, rulepack);

    for rule in &rulepack.rules {
        if let Some(matched_conditions) = match_family_rule(&input, rule) {
            return finalize_family_decision(node, rulepack, rule, matched_conditions);
        }
    }

    finalize_unknown_family(node, rulepack)
}

fn classify_confidence_with_rulepack(
    node: &DiagnosticNode,
    decision: &FamilyDecision,
    rulepack: &EnrichRulepack,
) -> Confidence {
    let input = RuleInput::from(node, rulepack);
    evaluate_confidence_policy(
        &input,
        decision,
        rulepack.confidence_policy_for(decision.family.as_str()),
    )
}

#[derive(Debug, Default)]
struct MatchState {
    matched_conditions: Vec<String>,
    matched_message_terms: bool,
    matched_candidate_child: bool,
    matched_any: bool,
}

fn match_family_rule(input: &RuleInput<'_>, rule: &FamilyRuleConfig) -> Option<Vec<String>> {
    let message_conditions = collect_group_conditions(input, &rule.message_groups, false);
    let child_conditions = collect_group_conditions(input, &rule.child_message_groups, true);
    let mut state = MatchState::default();

    for phase in effective_phase_match(rule) {
        if input.node.phase == phase {
            state.matched_any = true;
            state.matched_conditions.push(format!("phase={phase}"));
        }
    }

    for condition in effective_require_any_of(rule) {
        evaluate_match_condition(
            input,
            &condition,
            &message_conditions,
            &child_conditions,
            &mut state,
        );
    }

    if !state.matched_any {
        return None;
    }

    if state.matched_message_terms {
        push_phase_annotations(
            &mut state.matched_conditions,
            rule,
            &input.node.phase,
            PhaseAnnotationWhen::MessageTerms,
        );
    }
    if state.matched_message_terms || state.matched_candidate_child {
        push_phase_annotations(
            &mut state.matched_conditions,
            rule,
            &input.node.phase,
            PhaseAnnotationWhen::MessageOrCandidate,
        );
    }
    push_phase_annotations(
        &mut state.matched_conditions,
        rule,
        &input.node.phase,
        PhaseAnnotationWhen::RuleMatched,
    );

    Some(state.matched_conditions)
}

fn effective_phase_match(rule: &FamilyRuleConfig) -> Vec<Phase> {
    if let Some(phases) = &rule.phase_match {
        return phases.clone();
    }

    let is_linker_like = rule
        .contexts
        .iter()
        .any(|context| matches!(context.kind, ContextConditionKind::LinkerResolution))
        || rule.symbol_context_condition.is_some();
    if is_linker_like {
        vec![Phase::Link]
    } else {
        Vec::new()
    }
}

fn effective_require_any_of(rule: &FamilyRuleConfig) -> Vec<MatchConditionConfig> {
    if !rule.require_any_of.is_empty() {
        return rule.require_any_of.clone();
    }

    let mut conditions = Vec::new();
    for context in &rule.contexts {
        conditions.push(MatchConditionConfig::HasContext {
            context: context.kind,
        });
    }
    if rule.symbol_context_condition.is_some() {
        conditions.push(MatchConditionConfig::HasSymbolContext);
    }
    if rule.candidate_child_condition.is_some() || !rule.candidate_child_terms.is_empty() {
        conditions.push(MatchConditionConfig::HasCandidateChild);
    }
    for child_note in &rule.child_notes {
        conditions.push(MatchConditionConfig::HasChildNoteKind {
            child_note: child_note.kind,
        });
    }
    if !rule.message_groups.is_empty() {
        conditions.push(MatchConditionConfig::MessageTerms);
    }
    if !rule.child_message_groups.is_empty() {
        conditions.push(MatchConditionConfig::ChildMessageTerms);
    }
    if let Some(semantic_role) =
        legacy_semantic_role(rule.match_strategy, rule.semantic_role_condition.as_deref())
    {
        conditions.push(MatchConditionConfig::SemanticRoleIs { semantic_role });
    }
    conditions
}

fn legacy_semantic_role(
    strategy: Option<MatchStrategyConfig>,
    condition: Option<&str>,
) -> Option<SemanticRole> {
    if !matches!(strategy, Some(MatchStrategyConfig::SemanticRole)) {
        return None;
    }

    match condition.unwrap_or_default().trim() {
        "semantic_role=passthrough" => Some(SemanticRole::Passthrough),
        "semantic_role=root" => Some(SemanticRole::Root),
        "semantic_role=supporting" => Some(SemanticRole::Supporting),
        "semantic_role=help" => Some(SemanticRole::Help),
        "semantic_role=candidate" => Some(SemanticRole::Candidate),
        "semantic_role=path_event" => Some(SemanticRole::PathEvent),
        "semantic_role=summary" => Some(SemanticRole::Summary),
        "semantic_role=unknown" => Some(SemanticRole::Unknown),
        _ => None,
    }
}

fn evaluate_match_condition(
    input: &RuleInput<'_>,
    condition: &MatchConditionConfig,
    message_conditions: &[String],
    child_conditions: &[String],
    state: &mut MatchState,
) {
    match condition {
        MatchConditionConfig::PhaseIs { phase } if &input.node.phase == phase => {
            state.matched_any = true;
            state.matched_conditions.push(format!("phase={phase}"));
        }
        MatchConditionConfig::HasContext { context } if input.has_context(*context) => {
            state.matched_any = true;
            state
                .matched_conditions
                .push(format!("context={}", context_condition_name(*context)));
        }
        MatchConditionConfig::HasSymbolContext if input.has_symbol_context => {
            state.matched_any = true;
            state
                .matched_conditions
                .push("symbol_context=present".to_string());
        }
        MatchConditionConfig::HasCandidateChild if input.has_candidate_child => {
            state.matched_any = true;
            state.matched_candidate_child = true;
            state
                .matched_conditions
                .push("child_role=candidate".to_string());
        }
        MatchConditionConfig::HasTemplateChild if input.has_template_child => {
            state.matched_any = true;
            state
                .matched_conditions
                .push("child_note_kind=template_context".to_string());
        }
        MatchConditionConfig::HasChildNoteKind { child_note }
            if input.has_child_note_kind(*child_note) =>
        {
            state.matched_any = true;
            state.matched_conditions.push(format!(
                "child_note_kind={}",
                child_note_kind_name(*child_note)
            ));
        }
        MatchConditionConfig::MessageTerms if !message_conditions.is_empty() => {
            state.matched_any = true;
            state.matched_message_terms = true;
            state
                .matched_conditions
                .extend(message_conditions.iter().cloned());
        }
        MatchConditionConfig::ChildMessageTerms if !child_conditions.is_empty() => {
            state.matched_any = true;
            state
                .matched_conditions
                .extend(child_conditions.iter().cloned());
        }
        MatchConditionConfig::SemanticRoleIs { semantic_role }
            if &input.node.semantic_role == semantic_role =>
        {
            state.matched_any = true;
            state.matched_conditions.push(format!(
                "semantic_role={}",
                semantic_role_name(semantic_role)
            ));
        }
        _ => {}
    }
}

fn context_condition_name(kind: ContextConditionKind) -> &'static str {
    match kind {
        ContextConditionKind::TemplateInstantiation => "template_instantiation",
        ContextConditionKind::MacroExpansion => "macro_expansion",
        ContextConditionKind::Include => "include",
        ContextConditionKind::LinkerResolution => "linker_resolution",
    }
}

fn child_note_kind_name(kind: ChildNoteConditionKind) -> &'static str {
    match kind {
        ChildNoteConditionKind::TemplateContext => "template_context",
        ChildNoteConditionKind::MacroExpansion => "macro_expansion",
        ChildNoteConditionKind::Include => "include",
    }
}

fn semantic_role_name(role: &SemanticRole) -> &'static str {
    match role {
        SemanticRole::Root => "root",
        SemanticRole::Supporting => "supporting",
        SemanticRole::Help => "help",
        SemanticRole::Candidate => "candidate",
        SemanticRole::PathEvent => "path_event",
        SemanticRole::Summary => "summary",
        SemanticRole::Passthrough => "passthrough",
        SemanticRole::Unknown => "unknown",
    }
}

fn finalize_family_decision(
    node: &DiagnosticNode,
    rulepack: &EnrichRulepack,
    rule: &FamilyRuleConfig,
    mut matched_conditions: Vec<String>,
) -> FamilyDecision {
    let existing_specific = node
        .analysis
        .as_ref()
        .and_then(|analysis| analysis.family.as_ref())
        .filter(|family| family.contains('.') && rule.family != "unknown")
        .cloned()
        .map(|c| c.into_owned());
    if let Some(existing_family) = existing_specific {
        matched_conditions.push(format!("existing_specific_family={existing_family}"));
        FamilyDecision {
            family: existing_family,
            rule_id: rulepack.ingress_specific_override_rule_id.clone(),
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

fn finalize_unknown_family(node: &DiagnosticNode, rulepack: &EnrichRulepack) -> FamilyDecision {
    let existing_specific = node
        .analysis
        .as_ref()
        .and_then(|analysis| analysis.family.as_ref())
        .filter(|family| family.contains('.'))
        .cloned()
        .map(|c| c.into_owned());
    if let Some(existing_family) = existing_specific {
        return FamilyDecision {
            family: existing_family,
            rule_id: rulepack.ingress_specific_override_rule_id.clone(),
            matched_conditions: vec!["derived_family=unknown".to_string()],
            suppression_reason: Some("preserved_specific_family_from_ingress".to_string()),
        };
    }

    let fallback = if matches!(node.semantic_role, SemanticRole::Passthrough) {
        &rulepack.passthrough_fallback
    } else {
        &rulepack.unknown_fallback
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
    use diag_core::{
        Location, MessageText, NodeCompleteness, Origin, Provenance, ProvenanceSource, Severity,
    };
    use diag_rulepack::{ConfidenceLevelConfig, ConfidencePolicyConfig};

    fn sample_node(message: &str) -> DiagnosticNode {
        DiagnosticNode {
            id: "n1".to_string(),
            origin: Origin::Gcc,
            phase: Phase::Semantic,
            severity: Severity::Error,
            semantic_role: SemanticRole::Root,
            message: MessageText {
                raw_text: message.to_string(),
                normalized_text: None,
                locale: None,
            },
            locations: vec![Location::caret(
                "src/main.c",
                3,
                1,
                diag_core::LocationRole::Primary,
            )],
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
        }
    }

    #[test]
    fn loads_checked_in_enrich_rulepack() {
        let rulepack = rulepack();
        assert_eq!(rulepack.rulepack_version, "phase1");
        assert_eq!(rulepack.rules.len(), 28);
        assert!(std::ptr::eq(rulepack, checked_in_rulepack().enrich()));
        assert_eq!(
            rulepack.rule("syntax").rule_id,
            "rule.family.syntax.phase_or_message"
        );
        assert_eq!(rulepack.rule("syntax").match_strategy, None);
        assert!(!rulepack.rule("syntax").require_any_of.is_empty());
    }

    #[test]
    fn classifies_dummy_family_from_rulepack_only_configuration() {
        let mut custom_rulepack = rulepack().clone();
        custom_rulepack.rules.insert(
            0,
            FamilyRuleConfig {
                rule_id: "rule.family.dummy.message_terms".to_string(),
                family: "dummy".to_string(),
                phase_match: None,
                require_any_of: vec![MatchConditionConfig::MessageTerms],
                match_strategy: None,
                message_groups: vec![TermGroupConfig {
                    prefix: "message_contains".to_string(),
                    terms: vec!["synthetic dummy marker".to_string()],
                }],
                child_message_groups: Vec::new(),
                candidate_child_terms: Vec::new(),
                contexts: Vec::new(),
                child_notes: Vec::new(),
                symbol_context_condition: None,
                candidate_child_condition: None,
                semantic_role_condition: None,
                phase_annotations: Vec::new(),
            },
        );
        custom_rulepack
            .confidence_policies
            .push(ConfidencePolicyConfig {
                family: Some("dummy".to_string()),
                fixed: Some(ConfidenceLevelConfig::Medium),
                high_when_any: Vec::new(),
                medium_when_any: Vec::new(),
                default_confidence: ConfidenceLevelConfig::Low,
            });

        let node = sample_node("synthetic dummy marker from a JSON-only rule");
        let family = classify_family_with_rulepack(&node, &custom_rulepack);
        let confidence = classify_confidence_with_rulepack(&node, &family, &custom_rulepack);

        assert_eq!(family.family, "dummy");
        assert_eq!(family.rule_id, "rule.family.dummy.message_terms");
        assert_eq!(
            family.matched_conditions,
            vec!["message_contains=synthetic dummy marker".to_string()]
        );
        assert_eq!(confidence, Confidence::Medium);
    }
}
