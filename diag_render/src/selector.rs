use crate::budget::{WarningFailureMode, budget_for};
use crate::family::renderer_specificity_rank;
use crate::presentation::{ResolvedPresentationPolicy, SessionMode};
use crate::{RenderProfile, RenderRequest, WarningVisibility};
use diag_core::{
    CascadePolicySnapshot, CompressionLevel, DiagnosticDocument, DiagnosticNode,
    DisclosureConfidence, DocumentAnalysis, GroupCascadeAnalysis, GroupCascadeRole,
    NodeCompleteness, Ownership, Phase, SemanticRole, Severity, VisibilityFloor,
};
use std::collections::{BTreeMap, BTreeSet};

type VisibleGroupSortKey = (
    u8,
    Option<diag_core::Score>,
    Option<diag_core::Score>,
    u8,
    u8,
    u8,
    u8,
    u8,
    u8,
    usize,
);

/// The result of group selection: expanded cards, summary-only cards, and suppression counts.
#[derive(Debug, Default)]
pub struct Selection {
    /// Diagnostic nodes selected for full rendering.
    pub cards: Vec<DiagnosticNode>,
    /// Diagnostic nodes shown only as one-line summaries.
    pub summary_only_cards: Vec<DiagnosticNode>,
    /// Number of warnings suppressed because a failure was present.
    pub suppressed_warning_count: usize,
    /// Number of groups omitted without their own summary-only entry.
    pub hidden_group_count: usize,
    /// Additional collapsed notices synthesized during episode-first selection.
    pub collapsed_notices_by_group_ref: BTreeMap<String, Vec<String>>,
}

/// Selects, ranks, and partitions diagnostic groups from the request document.
pub fn select_groups(request: &RenderRequest) -> Selection {
    let presentation_policy = ResolvedPresentationPolicy::default();
    select_groups_with_presentation_policy(request, &presentation_policy)
}

/// Selects, ranks, and partitions diagnostic groups using an explicit presentation policy.
pub fn select_groups_with_presentation_policy(
    request: &RenderRequest,
    presentation_policy: &ResolvedPresentationPolicy,
) -> Selection {
    let mut diagnostics = request.document.diagnostics.clone();
    let budget = budget_for(request.profile);
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

    if let Some(selection) = select_episode_groups(
        request,
        presentation_policy,
        &request.document,
        &diagnostics,
        has_failure,
        suppressed_warning_count,
    ) {
        return selection;
    }

    legacy_select_groups(
        request,
        presentation_policy,
        diagnostics,
        has_failure,
        suppressed_warning_count,
    )
}

fn legacy_select_groups(
    request: &RenderRequest,
    presentation_policy: &ResolvedPresentationPolicy,
    diagnostics: Vec<DiagnosticNode>,
    has_failure: bool,
    suppressed_warning_count: usize,
) -> Selection {
    let budget = budget_for(request.profile);
    let session_mode = resolve_session_mode(presentation_policy, has_failure);
    let expand_all_failure_roots = expands_all_failure_roots(session_mode, has_failure);
    let mut expanded_groups = if expand_all_failure_roots {
        diagnostics.len()
    } else {
        match session_mode {
            SessionMode::LeadPlusSummary
                if matches!(request.profile, RenderProfile::Default) && !has_failure =>
            {
                2
            }
            SessionMode::LeadPlusSummary | SessionMode::CappedBlocks => match request.profile {
                RenderProfile::Default if !has_failure => 2,
                _ => budget.expanded_groups,
            },
            SessionMode::AllVisibleBlocks => budget.expanded_groups,
        }
    };
    if !expand_all_failure_roots
        && diagnostics.len() > expanded_groups
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
    let summary_only_cards = if should_summary_overflow_visible_roots(session_mode, has_failure) {
        diagnostics.collect()
    } else {
        Vec::new()
    };
    Selection {
        cards: expanded,
        summary_only_cards,
        suppressed_warning_count,
        hidden_group_count: 0,
        collapsed_notices_by_group_ref: BTreeMap::new(),
    }
}

fn select_episode_groups(
    request: &RenderRequest,
    presentation_policy: &ResolvedPresentationPolicy,
    document: &DiagnosticDocument,
    diagnostics: &[DiagnosticNode],
    has_failure: bool,
    suppressed_warning_count: usize,
) -> Option<Selection> {
    let document_analysis = document.document_analysis.as_ref()?;
    if document_analysis.episode_graph.episodes.is_empty()
        || document_analysis.group_analysis.is_empty()
    {
        return None;
    }

    let representatives = build_group_representatives(diagnostics);
    let group_analysis_by_ref = build_group_analysis_index(document_analysis)?;
    if representatives.is_empty()
        || representatives
            .keys()
            .any(|group_ref| !group_analysis_by_ref.contains_key(group_ref.as_str()))
    {
        return None;
    }

    if document_analysis
        .episode_graph
        .episodes
        .iter()
        .any(|episode| {
            !group_analysis_by_ref.contains_key(episode.lead_group_ref.as_str())
                || !representatives.contains_key(episode.lead_group_ref.as_str())
                || episode.member_group_refs.iter().any(|member_ref| {
                    !group_analysis_by_ref.contains_key(member_ref.as_str())
                        || !representatives.contains_key(member_ref.as_str())
                })
        })
    {
        return None;
    }

    let mut visible_group_refs = group_analysis_by_ref
        .values()
        .filter(|group| should_keep_group_visible(group))
        .map(|group| group.group_ref.as_str())
        .collect::<Vec<_>>();
    if visible_group_refs.is_empty() {
        return None;
    }
    visible_group_refs.sort_by(|left, right| {
        compare_visible_groups(
            representatives
                .get(*left)
                .expect("visible group representative"),
            group_analysis_by_ref
                .get(*left)
                .expect("visible group analysis"),
            representatives
                .get(*right)
                .expect("visible group representative"),
            group_analysis_by_ref
                .get(*right)
                .expect("visible group analysis"),
        )
        .then_with(|| left.cmp(right))
    });

    let session_mode = resolve_session_mode(presentation_policy, has_failure);
    let expanded_limit = expanded_independent_root_limit(request, has_failure, session_mode);
    let expanded_group_refs = visible_group_refs
        .iter()
        .take(expanded_limit)
        .copied()
        .collect::<BTreeSet<_>>();
    let visible_group_ref_set = visible_group_refs.iter().copied().collect::<BTreeSet<_>>();

    let cards = visible_group_refs
        .iter()
        .take(expanded_limit)
        .map(|group_ref| {
            representatives
                .get(*group_ref)
                .expect("expanded representative")
                .clone()
        })
        .collect::<Vec<_>>();
    let mut summary_only_cards = if should_summary_overflow_visible_roots(session_mode, has_failure)
    {
        visible_group_refs
            .iter()
            .skip(expanded_limit)
            .map(|group_ref| {
                representatives
                    .get(*group_ref)
                    .expect("summary-only representative")
                    .clone()
            })
            .collect::<Vec<_>>()
    } else {
        Vec::new()
    };

    let mut collapsed_notices_by_group_ref = BTreeMap::new();
    let mut hidden_group_count = 0usize;
    let mut groups_in_episodes = BTreeSet::new();
    for episode in &document_analysis.episode_graph.episodes {
        let lead_group_ref = episode.lead_group_ref.as_str();
        let lead_expanded = expanded_group_refs.contains(lead_group_ref);
        let mut follow_on_count = 0usize;
        let mut duplicate_count = 0usize;
        let mut related_count = 0usize;

        for member_ref in &episode.member_group_refs {
            groups_in_episodes.insert(member_ref.as_str());
            if member_ref == &episode.lead_group_ref
                || visible_group_ref_set.contains(member_ref.as_str())
            {
                continue;
            }
            let group = group_analysis_by_ref
                .get(member_ref.as_str())
                .expect("episode member analysis");
            let hide_member = should_hide_episode_member(request, group);
            let summarize_member = should_materialize_episode_member_as_summary(request, group);
            if should_surface_episode_member_as_summary(
                request,
                session_mode,
                has_failure,
                lead_expanded,
                summarize_member,
                hide_member,
            ) {
                summary_only_cards.push(
                    representatives
                        .get(member_ref.as_str())
                        .expect("episode member representative")
                        .clone(),
                );
                continue;
            }
            match group.role {
                GroupCascadeRole::FollowOn if lead_expanded => follow_on_count += 1,
                GroupCascadeRole::Duplicate if lead_expanded => duplicate_count += 1,
                _ if lead_expanded => related_count += 1,
                _ => hidden_group_count += 1,
            }
        }

        if lead_expanded && (follow_on_count > 0 || duplicate_count > 0 || related_count > 0) {
            let notices = collapsed_notices_by_group_ref
                .entry(lead_group_ref.to_string())
                .or_insert_with(Vec::new);
            if follow_on_count > 0 {
                notices.push(format!("omitted {follow_on_count} follow-on diagnostic(s)"));
            }
            if duplicate_count > 0 {
                notices.push(format!("omitted {duplicate_count} duplicate diagnostic(s)"));
            }
            if related_count > 0 {
                notices.push(format!("omitted {related_count} related diagnostic(s)"));
            }
        }
    }

    for group in group_analysis_by_ref.values() {
        let group_ref = group.group_ref.as_str();
        if visible_group_ref_set.contains(group_ref) || groups_in_episodes.contains(group_ref) {
            continue;
        }
        hidden_group_count += 1;
    }

    Some(Selection {
        cards,
        summary_only_cards,
        suppressed_warning_count,
        hidden_group_count,
        collapsed_notices_by_group_ref,
    })
}

fn build_group_representatives(diagnostics: &[DiagnosticNode]) -> BTreeMap<String, DiagnosticNode> {
    let mut representatives = BTreeMap::new();
    for node in diagnostics {
        let group_ref = render_group_ref(node);
        match representatives.get(group_ref.as_str()) {
            Some(current) if sort_key(node) <= sort_key(current) => {}
            _ => {
                representatives.insert(group_ref, node.clone());
            }
        }
    }
    representatives
}

fn build_group_analysis_index(
    document_analysis: &DocumentAnalysis,
) -> Option<BTreeMap<&str, &GroupCascadeAnalysis>> {
    let mut index = BTreeMap::new();
    for group in &document_analysis.group_analysis {
        if group.group_ref.trim().is_empty()
            || index.insert(group.group_ref.as_str(), group).is_some()
        {
            return None;
        }
    }
    Some(index)
}

fn should_keep_group_visible(group: &GroupCascadeAnalysis) -> bool {
    matches!(
        group.role,
        GroupCascadeRole::LeadRoot | GroupCascadeRole::IndependentRoot
    ) || group.visibility_floor == VisibilityFloor::NeverHidden
}

fn compare_visible_groups(
    left_node: &DiagnosticNode,
    left_group: &GroupCascadeAnalysis,
    right_node: &DiagnosticNode,
    right_group: &GroupCascadeAnalysis,
) -> std::cmp::Ordering {
    if share_visible_problem_signature(left_node, right_node) {
        return severity_rank(&right_node.severity)
            .cmp(&severity_rank(&left_node.severity))
            .then_with(|| {
                visible_group_sort_key(right_node, right_group)
                    .cmp(&visible_group_sort_key(left_node, left_group))
            });
    }

    visible_group_sort_key(right_node, right_group)
        .cmp(&visible_group_sort_key(left_node, left_group))
}

fn share_visible_problem_signature(left: &DiagnosticNode, right: &DiagnosticNode) -> bool {
    let left_family = left
        .analysis
        .as_ref()
        .and_then(|analysis| analysis.family.as_deref());
    let right_family = right
        .analysis
        .as_ref()
        .and_then(|analysis| analysis.family.as_deref());
    if left_family.is_none() || left_family != right_family {
        return false;
    }

    let (Some(left_location), Some(right_location)) =
        (left.primary_location(), right.primary_location())
    else {
        return false;
    };

    left_location.path_raw() == right_location.path_raw()
        && left_location.line() == right_location.line()
        && left_location.column() == right_location.column()
}

fn visible_group_sort_key(
    node: &DiagnosticNode,
    group: &GroupCascadeAnalysis,
) -> VisibleGroupSortKey {
    (
        cascade_role_rank(group.role),
        group.root_score,
        group.independence_score,
        severity_rank(&node.severity),
        ownership_rank(best_ownership(node)),
        confidence_rank(disclosure_confidence(node)),
        phase_rank(&node.phase),
        semantic_role_rank(&node.semantic_role),
        specificity_rank(node),
        std::cmp::Reverse(node.message.raw_text.len()).0,
    )
}

fn cascade_role_rank(role: GroupCascadeRole) -> u8 {
    match role {
        GroupCascadeRole::LeadRoot => 3,
        GroupCascadeRole::IndependentRoot => 2,
        GroupCascadeRole::Uncertain => 1,
        GroupCascadeRole::FollowOn | GroupCascadeRole::Duplicate => 0,
    }
}

fn expanded_independent_root_limit(
    request: &RenderRequest,
    has_failure: bool,
    session_mode: SessionMode,
) -> usize {
    if expands_all_failure_roots(session_mode, has_failure) {
        return usize::MAX;
    }

    match session_mode {
        SessionMode::LeadPlusSummary
        | SessionMode::CappedBlocks
        | SessionMode::AllVisibleBlocks => match request.profile {
            RenderProfile::Verbose | RenderProfile::Debug => usize::MAX,
            RenderProfile::RawFallback => 0,
            RenderProfile::Default if !has_failure => 2,
            RenderProfile::Default | RenderProfile::Concise | RenderProfile::Ci => {
                request.cascade_policy.max_expanded_independent_roots.max(1)
            }
        },
    }
}

fn should_materialize_episode_member_as_summary(
    request: &RenderRequest,
    group: &GroupCascadeAnalysis,
) -> bool {
    should_materialize_episode_member_as_summary_for_profile(
        request.profile,
        &request.cascade_policy,
        group,
    )
}

pub(crate) fn should_materialize_episode_member_as_summary_for_profile(
    profile: RenderProfile,
    policy: &CascadePolicySnapshot,
    group: &GroupCascadeAnalysis,
) -> bool {
    if matches!(profile, RenderProfile::Verbose | RenderProfile::Debug) {
        return true;
    }
    if policy.compression_level == CompressionLevel::Off {
        return true;
    }
    if matches!(
        group.visibility_floor,
        VisibilityFloor::NeverHidden | VisibilityFloor::SummaryOrExpandedOnly
    ) {
        return true;
    }
    group
        .summary_likelihood
        .map(|score| score.into_inner())
        .unwrap_or_default()
        >= policy.summary_likelihood_threshold
}

fn should_hide_episode_member(request: &RenderRequest, group: &GroupCascadeAnalysis) -> bool {
    should_hide_episode_member_for_profile(request.profile, &request.cascade_policy, group)
}

pub(crate) fn should_hide_episode_member_for_profile(
    profile: RenderProfile,
    policy: &CascadePolicySnapshot,
    group: &GroupCascadeAnalysis,
) -> bool {
    if matches!(
        profile,
        RenderProfile::Verbose | RenderProfile::Debug | RenderProfile::RawFallback
    ) {
        return false;
    }
    if group.visibility_floor != VisibilityFloor::HiddenAllowed {
        return false;
    }
    let suppress_score = group
        .suppress_likelihood
        .map(|score| score.into_inner())
        .unwrap_or_default();
    let minimum_threshold = match policy.compression_level {
        CompressionLevel::Off => return false,
        CompressionLevel::Conservative => {
            if group.role != GroupCascadeRole::Duplicate {
                return false;
            }
            policy.suppress_likelihood_threshold
        }
        CompressionLevel::Balanced => match group.role {
            GroupCascadeRole::Duplicate => policy.suppress_likelihood_threshold,
            GroupCascadeRole::FollowOn => (policy.suppress_likelihood_threshold + 0.10).min(0.95),
            _ => return false,
        },
        CompressionLevel::Aggressive => {
            if !matches!(
                group.role,
                GroupCascadeRole::FollowOn | GroupCascadeRole::Duplicate
            ) {
                return false;
            }
            policy.suppress_likelihood_threshold
        }
    };
    suppress_score >= minimum_threshold
}

pub(crate) fn render_group_ref(node: &DiagnosticNode) -> String {
    node.analysis
        .as_ref()
        .and_then(|analysis| analysis.group_ref.as_ref().map(|value| value.trim()))
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
        .unwrap_or_else(|| node.id.clone())
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

fn resolve_session_mode(
    presentation_policy: &ResolvedPresentationPolicy,
    has_failure: bool,
) -> SessionMode {
    if has_failure {
        presentation_policy.session_mode
    } else {
        SessionMode::LeadPlusSummary
    }
}

fn should_summary_overflow_visible_roots(session_mode: SessionMode, has_failure: bool) -> bool {
    !expands_all_failure_roots(session_mode, has_failure)
}

fn should_surface_episode_member_as_summary(
    request: &RenderRequest,
    session_mode: SessionMode,
    has_failure: bool,
    lead_expanded: bool,
    summarize_member: bool,
    hide_member: bool,
) -> bool {
    if hide_member {
        return false;
    }
    if !lead_expanded {
        return true;
    }
    if !expands_all_failure_roots(session_mode, has_failure) {
        return summarize_member;
    }
    matches!(
        request.profile,
        RenderProfile::Verbose | RenderProfile::Debug
    ) || request.cascade_policy.compression_level == CompressionLevel::Off
}

fn expands_all_failure_roots(session_mode: SessionMode, has_failure: bool) -> bool {
    has_failure && matches!(session_mode, SessionMode::AllVisibleBlocks)
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
                family: Some(family.to_string().into()),
                family_version: None,
                family_confidence: None,
                root_cause_score: None,
                actionability_score: None,
                user_code_priority: None,
                headline: Some("headline".into()),
                first_action_hint: Some("hint".into()),
                confidence: None,
                preferred_primary_location_id: None,
                rule_id: Some("rule".into()),
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
