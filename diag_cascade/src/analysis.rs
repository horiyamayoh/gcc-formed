use crate::{
    AnchorSource, CandidatePair, CandidateReason, CascadeContext, CascadeError, CascadeReport,
    DocumentAnalyzer, LogicalGroup, candidate_pairs, extract_logical_groups,
};
use diag_core::{
    CascadePolicySnapshot, CompressionLevel, DiagnosticDocument, DiagnosticEpisode, DiagnosticNode,
    DocumentAnalysis, EpisodeGraph, EpisodeRelation, EpisodeRelationKind, GroupCascadeAnalysis,
    GroupCascadeRole, Ownership, Score, VisibilityFloor, fingerprint_for,
};
use diag_rulepack::{CascadeRedundancyPolicy, checked_in_cascade_rulepack};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, Copy, Default)]
pub struct SafeDocumentAnalyzer;

#[derive(Debug, Clone)]
struct RootAssessment {
    score: f32,
    evidence_tags: BTreeSet<String>,
}

#[derive(Debug, Clone)]
struct RelationAssessment {
    parent_index: usize,
    child_index: usize,
    kind: EpisodeRelationKind,
    score: f32,
    evidence_tags: BTreeSet<String>,
    strong_evidence_count: usize,
    medium_evidence_count: usize,
}

#[derive(Debug, Clone, Default)]
struct ParentSelection {
    accepted: Option<RelationAssessment>,
    best_candidate: Option<RelationAssessment>,
    best_margin: Option<f32>,
    ambiguous: bool,
}

impl DocumentAnalyzer for SafeDocumentAnalyzer {
    fn analyze_document(
        &self,
        document: &mut DiagnosticDocument,
        context: &CascadeContext,
        policy: &CascadePolicySnapshot,
    ) -> Result<CascadeReport, CascadeError> {
        let analysis = materialize_document_analysis(document, context, policy);
        document.document_analysis = Some(analysis);
        Ok(CascadeReport {
            document_analysis_present: true,
        })
    }
}

pub(crate) fn materialize_document_analysis(
    document: &mut DiagnosticDocument,
    context: &CascadeContext,
    policy: &CascadePolicySnapshot,
) -> DocumentAnalysis {
    let groups = extract_logical_groups(document);
    stamp_group_refs(document, &groups);

    let roots = groups
        .iter()
        .map(|group| score_root_group(document, group, context))
        .collect::<Vec<_>>();
    let pair_candidates = candidate_pairs(&groups);
    let by_child = score_relations(document, &groups, &roots, &pair_candidates);
    let parent_selection = choose_parent_forest(&by_child, policy, context);
    build_document_analysis(
        document,
        policy,
        &groups,
        &roots,
        &parent_selection,
        context,
    )
}

fn stamp_group_refs(document: &mut DiagnosticDocument, groups: &[LogicalGroup]) {
    for group in groups {
        if let Some(node) = document.diagnostics.get_mut(group.node_index)
            && let Some(analysis) = node.analysis.as_mut()
        {
            analysis.group_ref = Some(group.group_ref.clone());
        }
    }
}

fn score_root_group(
    document: &DiagnosticDocument,
    group: &LogicalGroup,
    context: &CascadeContext,
) -> RootAssessment {
    let rulepack = checked_in_cascade_rulepack();
    let node = &document.diagnostics[group.node_index];
    let mut score = 0.34;
    let mut tags = BTreeSet::new();
    let family = node
        .analysis
        .as_ref()
        .and_then(|analysis| analysis.family.as_deref())
        .unwrap_or("unknown");
    let message = normalized_message(node);
    let message_lower = message.to_lowercase();

    if is_strong_root_family(rulepack, family, node, &message_lower) {
        score += 0.22;
        tags.insert("strong_root_family".to_string());
    } else if family != "unknown" {
        score += 0.06;
        tags.insert("classified_family".to_string());
    }

    match primary_ownership(node) {
        Some(Ownership::User) => {
            score += 0.18;
            tags.insert("user_owned_primary".to_string());
        }
        Some(Ownership::Vendor) | Some(Ownership::System) => {
            score -= 0.16;
            tags.insert("system_vendor_depth".to_string());
        }
        Some(Ownership::Generated) | Some(Ownership::Tool) => {
            score -= 0.10;
            tags.insert("compiler_owned_only".to_string());
        }
        Some(Ownership::Unknown) | None => {}
    }

    if matches!(
        group.canonical_anchor.source,
        AnchorSource::TemplateFrontier
            | AnchorSource::MacroFrontier
            | AnchorSource::IncludeFrontier
    ) && is_probably_user_path(group.canonical_anchor.path_key.as_deref(), context)
    {
        score += 0.10;
        tags.insert("first_user_frontier".to_string());
    }

    if group.keys.symbol_key.is_some()
        || group.keys.template_frontier_key.is_some()
        || group.keys.macro_frontier_key.is_some()
        || group.keys.include_frontier_key.is_some()
    {
        score += 0.06;
        tags.insert("specific_symbol_or_frontier".to_string());
    }

    if has_short_first_action(node) {
        score += 0.08;
        tags.insert("actionability_short".to_string());
    }

    let ordinal_bonus = match group.keys.ordinal_in_invocation {
        0 => 0.08,
        1 => 0.05,
        2 => 0.02,
        _ => 0.0,
    };
    if ordinal_bonus > 0.0 {
        score += ordinal_bonus;
        tags.insert("early_invocation".to_string());
    }

    if is_generic_follow_on(rulepack, node, family, &message_lower) {
        score -= 0.26;
        tags.insert("generic_follow_on".to_string());
    }

    if is_candidate_note_repeat(rulepack, node, family, &message_lower) {
        score -= 0.18;
        tags.insert("candidate_note_repeat".to_string());
    }

    if node.primary_location().is_none() {
        score -= 0.10;
        tags.insert("no_primary_location".to_string());
    }

    if is_generic_linker_wrapper(rulepack, node, family, &message_lower) {
        score -= 0.20;
        tags.insert("generic_linker_wrapper".to_string());
    }

    RootAssessment {
        score: clamp01(score),
        evidence_tags: tags,
    }
}

fn score_relations(
    document: &DiagnosticDocument,
    groups: &[LogicalGroup],
    roots: &[RootAssessment],
    pair_candidates: &[CandidatePair],
) -> Vec<Vec<RelationAssessment>> {
    let mut by_child = vec![Vec::new(); groups.len()];

    for pair in pair_candidates {
        let Some(relation) = score_relation(document, groups, roots, pair) else {
            continue;
        };
        by_child[relation.child_index].push(relation);
    }

    for relations in &mut by_child {
        relations.sort_by(|left, right| {
            right
                .score
                .total_cmp(&left.score)
                .then_with(|| left.parent_index.cmp(&right.parent_index))
        });
    }

    by_child
}

fn score_relation(
    document: &DiagnosticDocument,
    groups: &[LogicalGroup],
    roots: &[RootAssessment],
    pair: &CandidatePair,
) -> Option<RelationAssessment> {
    let rulepack = checked_in_cascade_rulepack();
    let weights = rulepack.weights();
    let parent = &groups[pair.left_index];
    let child = &groups[pair.right_index];
    let parent_node = &document.diagnostics[parent.node_index];
    let child_node = &document.diagnostics[child.node_index];
    let parent_message = normalized_message(parent_node).to_lowercase();
    let child_message = normalized_message(child_node).to_lowercase();
    let child_policy = rulepack.family_policy(child.keys.family_key.as_str());

    if parent.keys.ordinal_in_invocation >= child.keys.ordinal_in_invocation {
        return None;
    }

    let mut cascade = 0.10;
    let mut duplicate = 0.08;
    let mut tags = BTreeSet::new();
    let mut strong = 0usize;
    let mut medium = 0usize;

    tags.insert("earlier_ordinal".to_string());
    medium += 1;

    for reason in &pair.reasons {
        match reason {
            CandidateReason::SharedSymbol => {
                cascade += 0.34;
                duplicate += 0.36;
                tags.insert("shared_symbol".to_string());
                strong += 1;
            }
            CandidateReason::SharedTemplateFrontier => {
                cascade += 0.30;
                duplicate += 0.24;
                tags.insert("shared_template_frontier".to_string());
                strong += 1;
            }
            CandidateReason::SharedMacroFrontier => {
                cascade += 0.28;
                duplicate += 0.22;
                tags.insert("shared_macro_frontier".to_string());
                strong += 1;
            }
            CandidateReason::SharedIncludeFrontier => {
                cascade += 0.28;
                duplicate += 0.22;
                tags.insert("shared_include_frontier".to_string());
                strong += 1;
            }
            CandidateReason::SharedFamilyMessage => {
                cascade += 0.14;
                duplicate += 0.28;
                tags.insert("shared_normalized_message".to_string());
                medium += 1;
            }
            CandidateReason::TranslationUnitWindow => {
                cascade += 0.10;
                duplicate += 0.08;
                tags.insert("same_translation_unit".to_string());
                medium += 1;
            }
            CandidateReason::NearbyFileBucket => {
                cascade += 0.09;
                duplicate += 0.10;
                tags.insert("same_primary_file_bucket".to_string());
                medium += 1;
            }
            CandidateReason::FamilyPhaseWindow => {
                cascade += 0.05;
            }
            CandidateReason::LinkerSummaryWindow => {
                cascade += weights.linker_summary_window_bonus;
                duplicate += 0.06;
                tags.insert("linker_summary_window".to_string());
                strong += 1;
            }
        }
    }

    if is_generic_follow_on(
        rulepack,
        child_node,
        child.keys.family_key.as_str(),
        &child_message,
    ) {
        cascade += child_policy.follow_on_cascade_bonus;
        tags.insert("generic_follow_on_child".to_string());
        medium += 1;
    }

    if is_candidate_note_repeat(
        rulepack,
        child_node,
        child.keys.family_key.as_str(),
        &child_message,
    ) {
        cascade += 0.08;
        duplicate += child_policy.candidate_duplicate_bonus;
        tags.insert("candidate_repeat_child".to_string());
        medium += 1;
    }

    if is_generic_linker_wrapper(
        rulepack,
        child_node,
        child.keys.family_key.as_str(),
        &child_message,
    ) {
        cascade += child_policy.generic_wrapper_cascade_bonus;
        tags.insert("generic_linker_wrapper_child".to_string());
        medium += 1;
    }

    if is_strong_root_family(
        rulepack,
        parent.keys.family_key.as_str(),
        parent_node,
        &parent_message,
    ) {
        cascade += 0.12;
    }

    if let (Some(parent_ownership), Some(child_ownership)) = (
        primary_ownership(parent_node),
        primary_ownership(child_node),
    ) && parent_ownership == Ownership::User
        && child_ownership != Ownership::User
    {
        cascade += 0.08;
        tags.insert("parent_user_owned_child_non_user".to_string());
        medium += 1;
    }

    if roots[pair.left_index].score
        >= roots[pair.right_index].score + weights.parent_root_advantage_min
    {
        cascade += 0.10;
        tags.insert("parent_root_advantage".to_string());
        medium += 1;
    } else {
        cascade -= 0.18;
        duplicate -= 0.04;
        tags.insert("higher_root_competitor".to_string());
    }

    if separate_primary_problem(parent, child, parent_node, child_node) {
        cascade -= 0.22;
        duplicate -= 0.18;
        tags.insert("separate_primary_problem".to_string());
    }

    if parent_node.phase != child_node.phase {
        cascade -= 0.35;
        tags.insert("cross_phase_without_shared_key".to_string());
    }

    cascade = clamp01(cascade);
    duplicate = clamp01(duplicate);

    let (kind, score) = if duplicate >= weights.duplicate_threshold && duplicate >= cascade - 0.02 {
        (EpisodeRelationKind::Duplicate, duplicate)
    } else if cascade >= weights.dependency_threshold {
        (EpisodeRelationKind::Cascade, cascade)
    } else {
        return None;
    };

    Some(RelationAssessment {
        parent_index: pair.left_index,
        child_index: pair.right_index,
        kind,
        score,
        evidence_tags: tags,
        strong_evidence_count: strong,
        medium_evidence_count: medium,
    })
}

fn choose_parent_forest(
    by_child: &[Vec<RelationAssessment>],
    policy: &CascadePolicySnapshot,
    context: &CascadeContext,
) -> Vec<ParentSelection> {
    let weights = checked_in_cascade_rulepack().weights();
    let hidden_policy = hidden_policy(context);
    let threshold = weights.dependency_threshold + hidden_policy.dependency_threshold_delta_score();
    let min_margin = policy.min_parent_margin + hidden_policy.margin_delta_score();
    let mut selected = vec![ParentSelection::default(); by_child.len()];
    let mut accepted_parents = vec![None; by_child.len()];

    for child_index in 0..by_child.len() {
        let Some(best_candidate) = by_child[child_index].first().cloned() else {
            continue;
        };
        let second_score = by_child[child_index]
            .get(1)
            .map(|candidate| candidate.score);
        let margin = second_score
            .map(|score| best_candidate.score - score)
            .unwrap_or(1.0);
        let ambiguous = best_candidate.score >= threshold && margin < min_margin;

        selected[child_index].best_candidate = Some(best_candidate.clone());
        selected[child_index].best_margin = Some(margin);
        selected[child_index].ambiguous = ambiguous;

        if best_candidate.score < threshold || ambiguous {
            continue;
        }
        if would_create_cycle(&accepted_parents, best_candidate.parent_index, child_index) {
            continue;
        }
        accepted_parents[child_index] = Some(best_candidate.parent_index);
        selected[child_index].accepted = Some(best_candidate);
    }

    selected
}

fn build_document_analysis(
    document: &DiagnosticDocument,
    policy: &CascadePolicySnapshot,
    groups: &[LogicalGroup],
    roots: &[RootAssessment],
    selection: &[ParentSelection],
    context: &CascadeContext,
) -> DocumentAnalysis {
    let episodes = build_episode_components(groups, selection);
    let mut episode_refs = BTreeMap::new();
    let mut diagnostic_episodes = Vec::with_capacity(episodes.len());

    for component in &episodes {
        let lead_index = lead_index_for_component(component, selection, roots);
        let episode_ref = episode_ref_for_component(groups, component, lead_index);
        for &index in component {
            episode_refs.insert(index, episode_ref.clone());
        }
        diagnostic_episodes.push(DiagnosticEpisode {
            episode_ref,
            lead_group_ref: groups[lead_index].group_ref.clone(),
            member_group_refs: component
                .iter()
                .map(|index| groups[*index].group_ref.clone())
                .collect(),
            family: document.diagnostics[groups[lead_index].node_index]
                .analysis
                .as_ref()
                .and_then(|analysis| analysis.family.as_deref())
                .map(ToOwned::to_owned),
            lead_root_score: Some(score(roots[lead_index].score)),
            confidence: Some(score(component_confidence(component, selection, roots))),
        });
    }

    let mut group_analysis = Vec::with_capacity(groups.len());
    let mut relations = Vec::new();
    let hidden_policy = hidden_policy(context);
    let margin_floor = policy.min_parent_margin + hidden_policy.margin_delta_score();
    let weights = checked_in_cascade_rulepack().weights();

    for (index, group) in groups.iter().enumerate() {
        let root_score = roots[index].score;
        let component = component_for_index(&episodes, index);
        let lead_index = lead_index_for_component(component, selection, roots);
        let accepted = selection[index].accepted.as_ref();
        let best_candidate = selection[index].best_candidate.as_ref();
        let best_parent_group_ref = accepted
            .map(|relation| groups[relation.parent_index].group_ref.clone())
            .or_else(|| {
                selection[index]
                    .ambiguous
                    .then(|| {
                        best_candidate
                            .map(|relation| groups[relation.parent_index].group_ref.clone())
                    })
                    .flatten()
            });

        let role = if let Some(relation) = accepted {
            match relation.kind {
                EpisodeRelationKind::Duplicate => GroupCascadeRole::Duplicate,
                EpisodeRelationKind::Cascade => GroupCascadeRole::FollowOn,
                EpisodeRelationKind::Context => GroupCascadeRole::FollowOn,
            }
        } else if component.len() > 1 && index == lead_index {
            GroupCascadeRole::LeadRoot
        } else if selection[index].ambiguous || root_score < weights.independent_root_score {
            GroupCascadeRole::Uncertain
        } else {
            GroupCascadeRole::IndependentRoot
        };

        let mut evidence_tags = roots[index].evidence_tags.clone();
        if selection[index].ambiguous {
            evidence_tags.insert("weak_parent_margin".to_string());
        }
        if let Some(candidate) = best_candidate {
            evidence_tags.extend(candidate.evidence_tags.iter().cloned());
        }

        let visibility_floor = visibility_floor_for(
            role,
            accepted,
            selection[index].best_margin,
            margin_floor,
            context,
        );
        let suppress_likelihood = suppress_likelihood_for(
            role,
            accepted,
            selection[index].best_margin,
            margin_floor,
            visibility_floor,
            context,
        );
        let summary_likelihood = summary_likelihood_for(role, accepted, root_score);
        let independence_score = independence_score_for(role, accepted, root_score);

        if let Some(relation) = accepted {
            relations.push(EpisodeRelation {
                from_group_ref: groups[relation.parent_index].group_ref.clone(),
                to_group_ref: group.group_ref.clone(),
                kind: relation.kind,
                confidence: score(relation.score),
                evidence_tags: relation.evidence_tags.iter().cloned().collect(),
            });
        }

        group_analysis.push(GroupCascadeAnalysis {
            group_ref: group.group_ref.clone(),
            episode_ref: episode_refs.get(&index).cloned(),
            role,
            best_parent_group_ref,
            root_score: Some(score(root_score)),
            independence_score: Some(score(independence_score)),
            suppress_likelihood: Some(score(suppress_likelihood)),
            summary_likelihood: Some(score(summary_likelihood)),
            visibility_floor,
            evidence_tags: evidence_tags.into_iter().collect(),
        });
    }

    let stats = diag_core::CascadeStats {
        independent_root_count: group_analysis
            .iter()
            .filter(|group| {
                matches!(
                    group.role,
                    GroupCascadeRole::LeadRoot | GroupCascadeRole::IndependentRoot
                )
            })
            .count() as u32,
        dependent_follow_on_count: group_analysis
            .iter()
            .filter(|group| group.role == GroupCascadeRole::FollowOn)
            .count() as u32,
        duplicate_count: group_analysis
            .iter()
            .filter(|group| group.role == GroupCascadeRole::Duplicate)
            .count() as u32,
        uncertain_count: group_analysis
            .iter()
            .filter(|group| group.role == GroupCascadeRole::Uncertain)
            .count() as u32,
    };

    DocumentAnalysis {
        policy_profile: Some(format!(
            "default-{}",
            compression_level_name(policy.compression_level)
        )),
        producer_version: Some(env!("CARGO_PKG_VERSION").to_string()),
        episode_graph: EpisodeGraph {
            episodes: diagnostic_episodes,
            relations,
        },
        group_analysis,
        stats,
    }
}

fn build_episode_components(
    groups: &[LogicalGroup],
    selection: &[ParentSelection],
) -> Vec<Vec<usize>> {
    let mut adjacency = vec![Vec::new(); groups.len()];
    for (child_index, decision) in selection.iter().enumerate() {
        if let Some(relation) = decision.accepted.as_ref() {
            adjacency[relation.parent_index].push(child_index);
            adjacency[child_index].push(relation.parent_index);
        }
    }

    let mut visited = vec![false; groups.len()];
    let mut components = Vec::new();
    for start in 0..groups.len() {
        if visited[start] {
            continue;
        }
        let mut stack = vec![start];
        let mut component = Vec::new();
        visited[start] = true;
        while let Some(index) = stack.pop() {
            component.push(index);
            for &next in &adjacency[index] {
                if !visited[next] {
                    visited[next] = true;
                    stack.push(next);
                }
            }
        }
        component.sort();
        components.push(component);
    }
    components
}

fn lead_index_for_component(
    component: &[usize],
    selection: &[ParentSelection],
    roots: &[RootAssessment],
) -> usize {
    component
        .iter()
        .copied()
        .filter(|index| selection[*index].accepted.is_none())
        .max_by(|left, right| {
            roots[*left]
                .score
                .total_cmp(&roots[*right].score)
                .then_with(|| right.cmp(left))
        })
        .unwrap_or_else(|| {
            component
                .iter()
                .copied()
                .max_by(|left, right| {
                    roots[*left]
                        .score
                        .total_cmp(&roots[*right].score)
                        .then_with(|| right.cmp(left))
                })
                .unwrap_or(0)
        })
}

fn component_for_index(components: &[Vec<usize>], index: usize) -> &[usize] {
    components
        .iter()
        .find(|component| component.contains(&index))
        .map(Vec::as_slice)
        .unwrap_or(&[])
}

fn episode_ref_for_component(
    groups: &[LogicalGroup],
    component: &[usize],
    lead_index: usize,
) -> String {
    let digest = fingerprint_for(&(
        groups[lead_index].group_ref.clone(),
        component
            .iter()
            .map(|index| groups[*index].group_ref.clone())
            .collect::<Vec<_>>(),
    ));
    format!("episode-{}", &digest[..12])
}

fn component_confidence(
    component: &[usize],
    selection: &[ParentSelection],
    roots: &[RootAssessment],
) -> f32 {
    let accepted_scores = component
        .iter()
        .filter_map(|index| {
            selection[*index]
                .accepted
                .as_ref()
                .map(|relation| relation.score)
        })
        .collect::<Vec<_>>();
    if accepted_scores.is_empty() {
        return roots[component[0]].score;
    }
    clamp01(accepted_scores.iter().sum::<f32>() / accepted_scores.len() as f32)
}

fn visibility_floor_for(
    role: GroupCascadeRole,
    accepted: Option<&RelationAssessment>,
    best_margin: Option<f32>,
    margin_floor: f32,
    context: &CascadeContext,
) -> VisibilityFloor {
    match role {
        GroupCascadeRole::LeadRoot
        | GroupCascadeRole::IndependentRoot
        | GroupCascadeRole::Uncertain => VisibilityFloor::NeverHidden,
        GroupCascadeRole::FollowOn | GroupCascadeRole::Duplicate => {
            let Some(relation) = accepted else {
                return VisibilityFloor::SummaryOrExpandedOnly;
            };
            if hidden_allowed(relation, best_margin, margin_floor, context) {
                VisibilityFloor::HiddenAllowed
            } else {
                VisibilityFloor::SummaryOrExpandedOnly
            }
        }
    }
}

fn suppress_likelihood_for(
    role: GroupCascadeRole,
    accepted: Option<&RelationAssessment>,
    best_margin: Option<f32>,
    margin_floor: f32,
    visibility_floor: VisibilityFloor,
    context: &CascadeContext,
) -> f32 {
    match role {
        GroupCascadeRole::LeadRoot => 0.08,
        GroupCascadeRole::IndependentRoot => 0.18,
        GroupCascadeRole::Uncertain => 0.26,
        GroupCascadeRole::FollowOn | GroupCascadeRole::Duplicate => {
            let Some(relation) = accepted else {
                return 0.24;
            };
            let hidden_policy = hidden_policy(context);
            let base = match relation.kind {
                EpisodeRelationKind::Duplicate => 0.70,
                EpisodeRelationKind::Cascade => 0.54,
                EpisodeRelationKind::Context => 0.40,
            };
            let evidence_points =
                relation.strong_evidence_count * 2 + relation.medium_evidence_count;
            let redundancy_bonus = if evidence_points >= 4 {
                0.14
            } else if evidence_points >= 3 {
                0.08
            } else {
                -0.10
            };
            let margin_bonus = if best_margin.unwrap_or_default() >= margin_floor {
                0.08
            } else {
                -0.18
            };
            let floor_cap = if visibility_floor == VisibilityFloor::HiddenAllowed {
                1.0
            } else {
                0.66
            };
            clamp01(
                (base + redundancy_bonus + margin_bonus + hidden_policy.suppress_penalty_score())
                    .min(floor_cap),
            )
        }
    }
}

fn summary_likelihood_for(
    role: GroupCascadeRole,
    accepted: Option<&RelationAssessment>,
    root_score: f32,
) -> f32 {
    match role {
        GroupCascadeRole::LeadRoot => 0.18,
        GroupCascadeRole::IndependentRoot => clamp01(0.36 + ((1.0 - root_score) * 0.10)),
        GroupCascadeRole::Uncertain => 0.58,
        GroupCascadeRole::FollowOn => clamp01(
            0.56 + accepted
                .map(|relation| relation.score * 0.12)
                .unwrap_or(0.0),
        ),
        GroupCascadeRole::Duplicate => clamp01(
            0.74 + accepted
                .map(|relation| relation.score * 0.08)
                .unwrap_or(0.0),
        ),
    }
}

fn independence_score_for(
    role: GroupCascadeRole,
    accepted: Option<&RelationAssessment>,
    root_score: f32,
) -> f32 {
    match role {
        GroupCascadeRole::LeadRoot => clamp01(root_score.max(0.72)),
        GroupCascadeRole::IndependentRoot => clamp01(root_score.max(0.62)),
        GroupCascadeRole::Uncertain => clamp01(root_score.max(0.44)),
        GroupCascadeRole::FollowOn => {
            clamp01(0.24 + (1.0 - accepted.map(|relation| relation.score).unwrap_or(0.5)) * 0.15)
        }
        GroupCascadeRole::Duplicate => {
            clamp01(0.14 + (1.0 - accepted.map(|relation| relation.score).unwrap_or(0.5)) * 0.10)
        }
    }
}

fn hidden_allowed(
    relation: &RelationAssessment,
    best_margin: Option<f32>,
    margin_floor: f32,
    context: &CascadeContext,
) -> bool {
    let policy = hidden_policy(context);
    if policy.hidden_disabled {
        return false;
    }
    if policy.duplicate_only && relation.kind != EpisodeRelationKind::Duplicate {
        return false;
    }
    if best_margin.unwrap_or_default() < margin_floor {
        return false;
    }

    let evidence_points = relation.strong_evidence_count * 2 + relation.medium_evidence_count;
    let required_points = 3 + policy.extra_evidence_points;
    relation.strong_evidence_count >= 1 && evidence_points >= required_points
}

fn hidden_policy(context: &CascadeContext) -> CascadeRedundancyPolicy {
    checked_in_cascade_rulepack().redundancy_policy(
        context.version_band,
        context.processing_path,
        context.source_authority,
        context.fallback_grade,
    )
}

fn would_create_cycle(
    accepted_parents: &[Option<usize>],
    proposed_parent: usize,
    child: usize,
) -> bool {
    let mut cursor = Some(proposed_parent);
    while let Some(parent) = cursor {
        if parent == child {
            return true;
        }
        cursor = accepted_parents[parent];
    }
    false
}

fn is_strong_root_family(
    rulepack: &diag_rulepack::CascadeRulepack,
    family: &str,
    node: &DiagnosticNode,
    message_lower: &str,
) -> bool {
    rulepack.is_strong_root(family, message_lower)
        || family == "macro_include"
        || matches!(
            node.phase,
            diag_core::Phase::Parse | diag_core::Phase::Instantiate | diag_core::Phase::Constraints
        ) && (message_lower.contains("expected ")
            || message_lower.contains("undeclared")
            || message_lower.contains("does not name a type")
            || message_lower.contains("undefined reference")
            || message_lower.contains("multiple definition"))
}

fn is_generic_follow_on(
    rulepack: &diag_rulepack::CascadeRulepack,
    node: &DiagnosticNode,
    family: &str,
    message_lower: &str,
) -> bool {
    matches!(
        node.severity,
        diag_core::Severity::Note | diag_core::Severity::Remark | diag_core::Severity::Info
    ) || family == "passthrough"
        || rulepack.is_generic_follow_on(family, message_lower)
}

fn is_candidate_note_repeat(
    rulepack: &diag_rulepack::CascadeRulepack,
    node: &DiagnosticNode,
    family: &str,
    message_lower: &str,
) -> bool {
    node.semantic_role == diag_core::SemanticRole::Candidate
        || rulepack.is_candidate_repeat(family, message_lower)
}

fn is_generic_linker_wrapper(
    rulepack: &diag_rulepack::CascadeRulepack,
    _node: &DiagnosticNode,
    family: &str,
    message_lower: &str,
) -> bool {
    rulepack.is_generic_wrapper(family, message_lower)
}

fn separate_primary_problem(
    parent: &LogicalGroup,
    child: &LogicalGroup,
    parent_node: &DiagnosticNode,
    child_node: &DiagnosticNode,
) -> bool {
    let both_user_owned = matches!(primary_ownership(parent_node), Some(Ownership::User))
        && matches!(primary_ownership(child_node), Some(Ownership::User));
    if !both_user_owned {
        return false;
    }
    if parent.keys.symbol_key.is_some() || child.keys.symbol_key.is_some() {
        return false;
    }
    if parent.keys.template_frontier_key.is_some()
        && parent.keys.template_frontier_key == child.keys.template_frontier_key
    {
        return false;
    }
    if parent.keys.macro_frontier_key.is_some()
        && parent.keys.macro_frontier_key == child.keys.macro_frontier_key
    {
        return false;
    }
    if parent.keys.include_frontier_key.is_some()
        && parent.keys.include_frontier_key == child.keys.include_frontier_key
    {
        return false;
    }
    if parent.keys.primary_file_key != child.keys.primary_file_key {
        return true;
    }
    match (
        parent.keys.primary_line_bucket,
        child.keys.primary_line_bucket,
    ) {
        (Some(parent_bucket), Some(child_bucket)) => parent_bucket.abs_diff(child_bucket) > 1,
        _ => false,
    }
}

fn normalized_message(node: &DiagnosticNode) -> String {
    node.message
        .normalized_text
        .as_deref()
        .map(str::trim)
        .filter(|message| !message.is_empty())
        .unwrap_or_else(|| node.message.raw_text.trim())
        .to_string()
}

fn has_short_first_action(node: &DiagnosticNode) -> bool {
    node.analysis
        .as_ref()
        .and_then(|analysis| analysis.first_action_hint.as_deref())
        .map(str::trim)
        .is_some_and(|hint| !hint.is_empty() && hint.len() <= 96)
}

fn primary_ownership(node: &DiagnosticNode) -> Option<Ownership> {
    node.primary_location()
        .and_then(|location| location.ownership())
        .copied()
}

fn is_probably_user_path(path: Option<&str>, context: &CascadeContext) -> bool {
    let Some(path) = path else {
        return false;
    };
    let path = path.trim();
    if path.is_empty()
        || path.starts_with("/usr/")
        || path.starts_with("/opt/")
        || path.starts_with('<')
        || path.contains("/include/c++/")
        || path.contains("/lib/gcc/")
    {
        return false;
    }
    if path.starts_with("./") || path.starts_with("../") {
        return true;
    }
    let cwd = context.cwd.to_string_lossy().replace('\\', "/");
    path.starts_with(&cwd) || !path.starts_with('/')
}

fn compression_level_name(level: CompressionLevel) -> &'static str {
    match level {
        CompressionLevel::Off => "off",
        CompressionLevel::Conservative => "conservative",
        CompressionLevel::Balanced => "balanced",
        CompressionLevel::Aggressive => "aggressive",
    }
}

fn clamp01(score: f32) -> f32 {
    score.clamp(0.0, 1.0)
}

fn score(value: f32) -> Score {
    clamp01(value).into()
}
