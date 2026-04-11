use crate::budget::{WarningFailureMode, budget_for};
use crate::family::renderer_specificity_rank;
use crate::{RenderProfile, RenderRequest, WarningVisibility};
use diag_core::{
    DiagnosticNode, DisclosureConfidence, NodeCompleteness, Ownership, Phase, SemanticRole,
    Severity,
};

#[derive(Debug)]
pub struct Selection {
    pub cards: Vec<DiagnosticNode>,
    pub summary_only_cards: Vec<DiagnosticNode>,
    pub suppressed_warning_count: usize,
}

pub fn select_groups(request: &RenderRequest) -> Selection {
    let budget = budget_for(request.profile);
    let mut diagnostics = request.document.diagnostics.clone();
    diagnostics.sort_by(|left, right| {
        sort_key(right)
            .cmp(&sort_key(left))
            .then_with(|| left.id.cmp(&right.id))
    });
    let has_failure = diagnostics
        .iter()
        .any(|node| matches!(node.severity, Severity::Fatal | Severity::Error));

    let mut suppressed_warning_count = 0;
    if has_failure
        && should_filter_warnings(request.warning_visibility, budget.warning_failure_mode)
    {
        diagnostics.retain(|node| {
            if matches!(node.severity, Severity::Warning) {
                suppressed_warning_count += 1;
                false
            } else {
                true
            }
        });
    }

    let mut expanded_groups = match request.profile {
        RenderProfile::Default if !has_failure => 2,
        _ => budget.expanded_groups,
    };
    if diagnostics.len() > expanded_groups
        && matches!(
            request.profile,
            RenderProfile::Default | RenderProfile::Concise | RenderProfile::Ci
        )
        && diagnostics
            .first()
            .is_some_and(|node| is_low_confidence(node) || is_summary_only(node))
    {
        expanded_groups = expanded_groups.max(2);
    }
    let mut diagnostics = diagnostics.into_iter();
    let expanded = diagnostics.by_ref().take(expanded_groups).collect();
    let summary_only_cards = diagnostics.collect();
    Selection {
        cards: expanded,
        summary_only_cards,
        suppressed_warning_count,
    }
}

fn should_filter_warnings(
    visibility: WarningVisibility,
    warning_failure_mode: WarningFailureMode,
) -> bool {
    match visibility {
        WarningVisibility::ShowAll => false,
        WarningVisibility::SuppressAll => true,
        WarningVisibility::Auto => !matches!(warning_failure_mode, WarningFailureMode::Show),
    }
}

fn sort_key(node: &DiagnosticNode) -> (u8, u8, u8, u8, u8, u8, u8, usize) {
    (
        severity_rank(&node.severity),
        ownership_rank(best_ownership(node)),
        confidence_rank(disclosure_confidence(node)),
        phase_rank(&node.phase),
        semantic_role_rank(&node.semantic_role),
        specificity_rank(node),
        completeness_rank(&node.node_completeness),
        std::cmp::Reverse(node.message.raw_text.len()).0,
    )
}

fn severity_rank(severity: &Severity) -> u8 {
    match severity {
        Severity::Fatal => 7,
        Severity::Error => 6,
        Severity::Warning => 5,
        Severity::Note => 4,
        Severity::Remark => 3,
        Severity::Info => 2,
        Severity::Debug => 1,
        Severity::Unknown => 0,
    }
}

fn ownership_rank(ownership: Option<&Ownership>) -> u8 {
    match ownership {
        Some(Ownership::User) => 4,
        Some(Ownership::Vendor) => 3,
        Some(Ownership::Generated) => 2,
        Some(Ownership::System) => 1,
        _ => 0,
    }
}

fn best_ownership(node: &DiagnosticNode) -> Option<&Ownership> {
    node.primary_location()
        .and_then(|location| location.ownership())
        .or_else(|| {
            node.locations
                .iter()
                .filter_map(|location| location.ownership())
                .max_by_key(|ownership| ownership_rank(Some(*ownership)))
        })
}

fn confidence_rank(confidence: DisclosureConfidence) -> u8 {
    match confidence {
        DisclosureConfidence::Certain => 4,
        DisclosureConfidence::Likely => 3,
        DisclosureConfidence::Possible => 2,
        DisclosureConfidence::Hidden => 1,
    }
}

fn phase_rank(phase: &Phase) -> u8 {
    match phase {
        Phase::Parse => 9,
        Phase::Semantic => 8,
        Phase::Instantiate => 7,
        Phase::Constraints => 6,
        Phase::Analyze => 5,
        Phase::Codegen => 4,
        Phase::Assemble => 3,
        Phase::Link => 2,
        Phase::Driver | Phase::Preprocess | Phase::Optimize | Phase::Archive | Phase::Unknown => 1,
    }
}

fn specificity_rank(node: &DiagnosticNode) -> u8 {
    let symbol_rank = u8::from(node.symbol_context.is_some());
    let first_action_rank = has_first_action(node);
    renderer_specificity_rank(node) + symbol_rank + u8::from(first_action_rank)
}

fn completeness_rank(completeness: &NodeCompleteness) -> u8 {
    match completeness {
        NodeCompleteness::Complete => 3,
        NodeCompleteness::Partial => 2,
        NodeCompleteness::Synthesized => 1,
        NodeCompleteness::Passthrough => 0,
    }
}

fn semantic_role_rank(role: &SemanticRole) -> u8 {
    match role {
        SemanticRole::Root => 7,
        SemanticRole::Summary => 6,
        SemanticRole::Help => 5,
        SemanticRole::Supporting => 4,
        SemanticRole::Candidate => 3,
        SemanticRole::PathEvent => 2,
        SemanticRole::Passthrough => 1,
        SemanticRole::Unknown => 0,
    }
}

fn is_low_confidence(node: &DiagnosticNode) -> bool {
    disclosure_confidence(node).requires_low_confidence_notice()
}

fn is_summary_only(node: &DiagnosticNode) -> bool {
    matches!(
        node.semantic_role,
        SemanticRole::Summary | SemanticRole::Passthrough
    ) || matches!(node.node_completeness, NodeCompleteness::Passthrough)
        || (matches!(node.node_completeness, NodeCompleteness::Partial)
            && node.primary_location().is_none()
            && node.symbol_context.is_none()
            && !has_first_action(node))
}

fn has_first_action(node: &DiagnosticNode) -> bool {
    let Some(analysis) = node.analysis.as_ref() else {
        return false;
    };
    if !analysis.disclosure_confidence().allows_first_action() {
        return false;
    }
    analysis
        .first_action_hint
        .as_ref()
        .is_some_and(|hint| !hint.trim().is_empty())
}

fn disclosure_confidence(node: &DiagnosticNode) -> DisclosureConfidence {
    node.analysis
        .as_ref()
        .map(|analysis| analysis.disclosure_confidence())
        .unwrap_or(DisclosureConfidence::Hidden)
}

#[cfg(test)]
mod tests {
    use super::*;
    use diag_core::{
        AnalysisOverlay, DiagnosticNode, MessageText, NodeCompleteness, Origin, Phase, Provenance,
        ProvenanceSource, SemanticRole, Severity,
    };

    fn sample_node(family: &str) -> DiagnosticNode {
        DiagnosticNode {
            id: family.to_string(),
            origin: Origin::Gcc,
            phase: Phase::Semantic,
            severity: Severity::Error,
            semantic_role: SemanticRole::Root,
            message: MessageText {
                raw_text: "message".to_string(),
                normalized_text: None,
                locale: None,
            },
            locations: Vec::new(),
            children: Vec::new(),
            suggestions: Vec::new(),
            context_chains: Vec::new(),
            symbol_context: None,
            node_completeness: NodeCompleteness::Complete,
            provenance: Provenance {
                source: ProvenanceSource::Compiler,
                capture_refs: vec!["stderr.raw".to_string()],
            },
            analysis: Some(AnalysisOverlay {
                family: Some(family.to_string()),
                family_version: None,
                family_confidence: None,
                root_cause_score: None,
                actionability_score: None,
                user_code_priority: None,
                headline: Some("headline".to_string()),
                first_action_hint: Some("hint".to_string()),
                confidence: None,
                preferred_primary_location_id: None,
                rule_id: Some("rule".to_string()),
                matched_conditions: Vec::new(),
                suppression_reason: None,
                collapsed_child_ids: Vec::new(),
                collapsed_chain_ids: Vec::new(),
                group_ref: None,
                reasons: Vec::new(),
                policy_profile: None,
                producer_version: None,
            }),
            fingerprints: None,
        }
    }

    #[test]
    fn specificity_rank_uses_rulepack_policy() {
        assert!(
            specificity_rank(&sample_node("linker.undefined_reference"))
                > specificity_rank(&sample_node("template"))
        );
        assert!(
            specificity_rank(&sample_node("template"))
                > specificity_rank(&sample_node("linker.file_format_or_relocation"))
        );
    }
}
