use std::collections::{HashMap, HashSet, VecDeque};

use semver::Version;

use crate::{
    ArtifactKind, ArtifactStorage, DiagnosticDocument, DiagnosticNode, DocumentAnalysis,
    DocumentCompleteness, EpisodeGraph, EpisodeRelation, GroupCascadeAnalysis, GroupCascadeRole,
    IntegrityIssue, Location, NodeCompleteness, Phase, Provenance, ProvenanceSource, SemanticRole,
    ValidationErrors, VisibilityFloor,
};

impl DiagnosticDocument {
    /// Validates the document, returning all detected errors.
    ///
    /// Checks include: non-empty IDs, valid semver, unique capture/node IDs,
    /// referential integrity of provenance `capture_refs`, and analysis score ranges.
    pub fn validate(&self) -> Result<(), ValidationErrors> {
        let mut errors = Vec::new();
        let mut capture_ids = HashSet::new();
        let mut capture_kinds = HashMap::new();
        let mut node_ids = HashSet::new();

        if self.document_id.trim().is_empty() {
            errors.push("document_id must be non-empty".to_string());
        }
        if self.schema_version.trim().is_empty() {
            errors.push("schema_version must be non-empty".to_string());
        } else if Version::parse(self.schema_version.trim()).is_err() {
            errors.push(format!(
                "schema_version {} must be parseable semver",
                self.schema_version
            ));
        }
        if self.diagnostics.is_empty()
            && !matches!(
                self.document_completeness,
                DocumentCompleteness::Failed | DocumentCompleteness::Passthrough
            )
        {
            errors.push(
                "diagnostics may be empty only for failed or passthrough documents".to_string(),
            );
        }
        for capture in &self.captures {
            if capture.id.trim().is_empty() {
                errors.push("capture id must be non-empty".to_string());
            }
            if !capture_ids.insert(capture.id.clone()) {
                errors.push(format!("duplicate capture id: {}", capture.id));
            } else {
                capture_kinds.insert(capture.id.clone(), capture.kind.clone());
            }
            match capture.storage {
                ArtifactStorage::Inline => {
                    if capture.inline_text.is_none() {
                        errors.push(format!("inline capture {} missing inline_text", capture.id));
                    }
                    if capture.external_ref.is_some() {
                        errors.push(format!(
                            "inline capture {} must not set external_ref",
                            capture.id
                        ));
                    }
                }
                ArtifactStorage::ExternalRef => {
                    if capture.external_ref.is_none() {
                        errors.push(format!(
                            "external_ref capture {} missing external_ref",
                            capture.id
                        ));
                    } else if capture
                        .external_ref
                        .as_deref()
                        .is_some_and(|external_ref| external_ref.trim().is_empty())
                    {
                        errors.push(format!(
                            "external_ref capture {} external_ref must be non-empty",
                            capture.id
                        ));
                    }
                    if capture.inline_text.is_some() {
                        errors.push(format!(
                            "external_ref capture {} must not set inline_text",
                            capture.id
                        ));
                    }
                }
                ArtifactStorage::Unavailable => {
                    if capture.inline_text.is_some() || capture.external_ref.is_some() {
                        errors.push(format!(
                            "unavailable capture {} must not set inline_text or external_ref",
                            capture.id
                        ));
                    }
                }
            }
        }
        for (index, issue) in self.integrity_issues.iter().enumerate() {
            validate_integrity_issue(issue, index, &capture_ids, &mut errors);
        }
        for node in &self.diagnostics {
            validate_node(
                node,
                &capture_ids,
                &capture_kinds,
                &mut node_ids,
                &mut errors,
                true,
            );
        }
        if let Some(document_analysis) = self.document_analysis.as_ref() {
            validate_document_analysis(document_analysis, &mut errors);
        }
        if errors.is_empty() {
            Ok(())
        } else {
            Err(ValidationErrors { errors })
        }
    }
}

fn validate_node(
    node: &DiagnosticNode,
    capture_ids: &HashSet<String>,
    capture_kinds: &HashMap<String, ArtifactKind>,
    node_ids: &mut HashSet<String>,
    errors: &mut Vec<String>,
    top_level: bool,
) {
    if node.id.trim().is_empty() {
        errors.push("node id must be non-empty".to_string());
    }
    if !node_ids.insert(node.id.clone()) {
        errors.push(format!("duplicate node id: {}", node.id));
    }
    validate_provenance(
        &format!("node {} provenance", node.id),
        &node.provenance,
        capture_ids,
        errors,
    );
    if node.message.raw_text.trim().is_empty() {
        errors.push(format!("node {} missing raw_text", node.id));
    }
    if matches!(node.node_completeness, NodeCompleteness::Passthrough)
        && node.provenance.capture_refs.is_empty()
    {
        errors.push(format!(
            "node {} is passthrough but provenance.capture_refs is empty",
            node.id
        ));
    }
    if top_level
        && !matches!(
            node.semantic_role,
            SemanticRole::Root | SemanticRole::Summary | SemanticRole::Passthrough
        )
    {
        errors.push(format!(
            "top-level node {} must be root, summary, or passthrough",
            node.id
        ));
    }
    for child in &node.children {
        if matches!(child.semantic_role, SemanticRole::Root) {
            errors.push(format!(
                "child node {} must not have semantic_role=root",
                child.id
            ));
        }
        validate_node(child, capture_ids, capture_kinds, node_ids, errors, false);
    }
    if matches!(node.node_completeness, NodeCompleteness::Synthesized)
        && !matches!(
            node.provenance.source,
            ProvenanceSource::WrapperGenerated | ProvenanceSource::Policy
        )
    {
        errors.push(format!(
            "node {} is synthesized but provenance.source is not wrapper_generated or policy",
            node.id
        ));
    }
    if matches!(
        node.phase,
        Phase::Parse | Phase::Semantic | Phase::Instantiate
    ) && node.locations.is_empty()
        && matches!(node.node_completeness, NodeCompleteness::Complete)
    {
        errors.push(format!(
            "node {} is complete in parse/semantic/instantiate phase but has no locations",
            node.id
        ));
    }
    let child_ids = descendant_node_ids(node);
    if let Some(analysis) = node.analysis.as_ref() {
        for (label, score) in [
            ("family_confidence", analysis.family_confidence),
            ("root_cause_score", analysis.root_cause_score),
            ("actionability_score", analysis.actionability_score),
            ("user_code_priority", analysis.user_code_priority),
            ("confidence", analysis.confidence),
        ] {
            if let Some(score) = score
                && !(0.0..=1.0).contains(&score.into_inner())
            {
                errors.push(format!(
                    "node {} analysis {} must be within 0.0..=1.0",
                    node.id, label
                ));
            }
        }
        if let Some(preferred_id) = analysis.preferred_primary_location_id.as_deref()
            && !node
                .locations
                .iter()
                .any(|location| location.id == preferred_id)
        {
            errors.push(format!(
                "node {} preferred_primary_location_id {} does not exist",
                node.id, preferred_id
            ));
        }
        for child_id in &analysis.collapsed_child_ids {
            if !child_ids.contains(child_id) {
                errors.push(format!(
                    "node {} collapsed_child_id {} does not reference a descendant",
                    node.id, child_id
                ));
            }
        }
    }
    let mut location_ids = HashSet::new();
    for location in &node.locations {
        if !location_ids.insert(location.id.clone()) {
            errors.push(format!(
                "node {} has duplicate location id {}",
                node.id, location.id
            ));
        }
        validate_location(node, location, capture_ids, capture_kinds, errors);
    }
}

fn validate_integrity_issue(
    issue: &IntegrityIssue,
    index: usize,
    capture_ids: &HashSet<String>,
    errors: &mut Vec<String>,
) {
    if let Some(provenance) = issue.provenance.as_ref() {
        validate_provenance(
            &format!("integrity_issue[{index}] provenance"),
            provenance,
            capture_ids,
            errors,
        );
    }
}

fn validate_location(
    node: &DiagnosticNode,
    location: &Location,
    capture_ids: &HashSet<String>,
    capture_kinds: &HashMap<String, ArtifactKind>,
    errors: &mut Vec<String>,
) {
    if location.id.trim().is_empty() {
        errors.push(format!("node {} location id must be non-empty", node.id));
    }
    if location.file.path_raw.trim().is_empty() {
        errors.push(format!(
            "node {} location {} file.path_raw must be non-empty",
            node.id, location.id
        ));
    }
    if location.anchor.is_none() && location.range.is_none() {
        errors.push(format!(
            "node {} location {} must have anchor or range",
            node.id, location.id
        ));
    }
    if let Some(anchor) = location.anchor.as_ref()
        && anchor.line < 1
    {
        errors.push(format!(
            "node {} location {} anchor line must be >= 1",
            node.id, location.id
        ));
    }
    if let Some(range) = location.range.as_ref() {
        if range.start.line < 1 {
            errors.push(format!(
                "node {} location {} range.start line must be >= 1",
                node.id, location.id
            ));
        }
        if range.end.line < 1 {
            errors.push(format!(
                "node {} location {} range.end line must be >= 1",
                node.id, location.id
            ));
        }
        if source_point_order_key(&range.start) > source_point_order_key(&range.end) {
            errors.push(format!(
                "node {} location {} range.start must not come after range.end",
                node.id, location.id
            ));
        }
    }
    if let Some(provenance) = location.provenance_override.as_ref() {
        validate_provenance(
            &format!(
                "node {} location {} provenance_override",
                node.id, location.id
            ),
            provenance,
            capture_ids,
            errors,
        );
    }
    if let Some(source_excerpt_ref) = location.source_excerpt_ref.as_deref() {
        if source_excerpt_ref.trim().is_empty() {
            errors.push(format!(
                "node {} location {} source_excerpt_ref must be non-empty when present",
                node.id, location.id
            ));
        } else {
            match capture_kinds.get(source_excerpt_ref) {
                None => errors.push(format!(
                    "node {} location {} source_excerpt_ref references missing capture {}",
                    node.id, location.id, source_excerpt_ref
                )),
                Some(ArtifactKind::SourceSnippet) => {}
                Some(_) => errors.push(format!(
                    "node {} location {} source_excerpt_ref {} must reference a source_snippet capture",
                    node.id, location.id, source_excerpt_ref
                )),
            }
        }
    }
}

fn source_point_order_key(point: &crate::SourcePoint) -> (u32, u32) {
    (
        point.line,
        point
            .column_display
            .or(point.column_native)
            .or(point.column_byte)
            .unwrap_or(1),
    )
}

fn validate_provenance(
    scope: &str,
    provenance: &Provenance,
    capture_ids: &HashSet<String>,
    errors: &mut Vec<String>,
) {
    for capture_ref in &provenance.capture_refs {
        if !capture_ids.contains(capture_ref) {
            errors.push(format!(
                "{scope} references missing capture {}",
                capture_ref
            ));
        }
    }
}

fn descendant_node_ids(node: &DiagnosticNode) -> HashSet<String> {
    let mut ids = HashSet::new();
    collect_descendant_node_ids(node, &mut ids);
    ids
}

fn collect_descendant_node_ids(node: &DiagnosticNode, ids: &mut HashSet<String>) {
    for child in &node.children {
        ids.insert(child.id.clone());
        collect_descendant_node_ids(child, ids);
    }
}

fn validate_document_analysis(document_analysis: &DocumentAnalysis, errors: &mut Vec<String>) {
    let mut group_refs = HashSet::new();
    let mut episode_members_by_ref: HashMap<String, HashSet<String>> = HashMap::new();
    let mut episode_lead_by_ref: HashMap<String, String> = HashMap::new();
    for group in &document_analysis.group_analysis {
        if group.group_ref.trim().is_empty() {
            errors.push("document_analysis group_ref must be non-empty".to_string());
        }
        if !group_refs.insert(group.group_ref.clone()) {
            errors.push(format!(
                "document_analysis duplicate group_ref {}",
                group.group_ref
            ));
        }
        for (label, score) in [
            ("root_score", group.root_score),
            ("independence_score", group.independence_score),
            ("suppress_likelihood", group.suppress_likelihood),
            ("summary_likelihood", group.summary_likelihood),
        ] {
            validate_score(
                &format!("document_analysis group {} {}", group.group_ref, label),
                score,
                errors,
            );
        }
        validate_group_materialization(group, errors);
    }

    validate_episode_graph(
        &document_analysis.episode_graph,
        &group_refs,
        &mut episode_members_by_ref,
        &mut episode_lead_by_ref,
        errors,
    );

    for group in &document_analysis.group_analysis {
        validate_group_episode_membership(
            group,
            &group_refs,
            &episode_members_by_ref,
            &episode_lead_by_ref,
            errors,
        );
    }
}

fn validate_episode_graph(
    episode_graph: &EpisodeGraph,
    group_refs: &HashSet<String>,
    episode_members_by_ref: &mut HashMap<String, HashSet<String>>,
    episode_lead_by_ref: &mut HashMap<String, String>,
    errors: &mut Vec<String>,
) {
    let mut episode_refs = HashSet::new();
    for episode in &episode_graph.episodes {
        if episode.episode_ref.trim().is_empty() {
            errors.push("document_analysis episode_ref must be non-empty".to_string());
        }
        if !episode_refs.insert(episode.episode_ref.clone()) {
            errors.push(format!(
                "document_analysis duplicate episode_ref {}",
                episode.episode_ref
            ));
        }
        if episode.lead_group_ref.trim().is_empty() {
            errors.push(format!(
                "document_analysis episode {} lead_group_ref must be non-empty",
                episode.episode_ref
            ));
        } else if !group_refs.contains(&episode.lead_group_ref) {
            errors.push(format!(
                "document_analysis episode {} lead_group_ref {} does not exist",
                episode.episode_ref, episode.lead_group_ref
            ));
        }
        for member_ref in &episode.member_group_refs {
            if !group_refs.contains(member_ref) {
                errors.push(format!(
                    "document_analysis episode {} member_group_ref {} does not exist",
                    episode.episode_ref, member_ref
                ));
            }
        }
        validate_score(
            &format!(
                "document_analysis episode {} lead_root_score",
                episode.episode_ref
            ),
            episode.lead_root_score,
            errors,
        );
        validate_score(
            &format!(
                "document_analysis episode {} confidence",
                episode.episode_ref
            ),
            episode.confidence,
            errors,
        );
        let member_refs = episode
            .member_group_refs
            .iter()
            .cloned()
            .collect::<HashSet<_>>();
        if member_refs.len() != episode.member_group_refs.len() {
            errors.push(format!(
                "document_analysis episode {} member_group_refs must be unique",
                episode.episode_ref
            ));
        }
        if !member_refs.contains(&episode.lead_group_ref) {
            errors.push(format!(
                "document_analysis episode {} lead_group_ref {} must be included in member_group_refs",
                episode.episode_ref, episode.lead_group_ref
            ));
        }
        episode_members_by_ref.insert(episode.episode_ref.clone(), member_refs);
        episode_lead_by_ref.insert(episode.episode_ref.clone(), episode.lead_group_ref.clone());
    }

    for relation in &episode_graph.relations {
        validate_relation(relation, group_refs, errors);
    }

    validate_relation_graph_acyclic(episode_graph, group_refs, errors);
}

fn validate_relation(
    relation: &EpisodeRelation,
    group_refs: &HashSet<String>,
    errors: &mut Vec<String>,
) {
    if relation.from_group_ref.trim().is_empty() {
        errors.push("document_analysis relation from_group_ref must be non-empty".to_string());
    } else if !group_refs.contains(&relation.from_group_ref) {
        errors.push(format!(
            "document_analysis relation from_group_ref {} does not exist",
            relation.from_group_ref
        ));
    }
    if relation.to_group_ref.trim().is_empty() {
        errors.push("document_analysis relation to_group_ref must be non-empty".to_string());
    } else if !group_refs.contains(&relation.to_group_ref) {
        errors.push(format!(
            "document_analysis relation to_group_ref {} does not exist",
            relation.to_group_ref
        ));
    } else if relation.from_group_ref == relation.to_group_ref {
        errors.push(format!(
            "document_analysis relation {} -> {} must not self-reference",
            relation.from_group_ref, relation.to_group_ref
        ));
    }
    validate_score(
        &format!(
            "document_analysis relation {} -> {} confidence",
            relation.from_group_ref, relation.to_group_ref
        ),
        Some(relation.confidence),
        errors,
    );
}

fn validate_relation_graph_acyclic(
    episode_graph: &EpisodeGraph,
    group_refs: &HashSet<String>,
    errors: &mut Vec<String>,
) {
    let mut indegree = group_refs
        .iter()
        .map(|group_ref| (group_ref.clone(), 0usize))
        .collect::<HashMap<_, _>>();
    let mut adjacency: HashMap<String, Vec<String>> = HashMap::new();

    for relation in &episode_graph.relations {
        if relation.from_group_ref == relation.to_group_ref {
            continue;
        }
        if !(group_refs.contains(&relation.from_group_ref)
            && group_refs.contains(&relation.to_group_ref))
        {
            continue;
        }
        adjacency
            .entry(relation.from_group_ref.clone())
            .or_default()
            .push(relation.to_group_ref.clone());
        *indegree.entry(relation.to_group_ref.clone()).or_insert(0) += 1;
    }

    let mut queue = indegree
        .iter()
        .filter_map(|(group_ref, degree)| (*degree == 0).then_some(group_ref.clone()))
        .collect::<VecDeque<_>>();
    let mut processed = 0usize;

    while let Some(group_ref) = queue.pop_front() {
        processed += 1;
        if let Some(next_group_refs) = adjacency.get(&group_ref) {
            for next_group_ref in next_group_refs {
                if let Some(degree) = indegree.get_mut(next_group_ref) {
                    *degree = degree.saturating_sub(1);
                    if *degree == 0 {
                        queue.push_back(next_group_ref.clone());
                    }
                }
            }
        }
    }

    if processed != indegree.len() {
        let mut cycle_group_refs = indegree
            .into_iter()
            .filter_map(|(group_ref, degree)| (degree > 0).then_some(group_ref))
            .collect::<Vec<_>>();
        cycle_group_refs.sort();
        errors.push(format!(
            "document_analysis episode_graph must be acyclic; cycle involves {}",
            cycle_group_refs.join(", ")
        ));
    }
}

fn validate_group_materialization(group: &GroupCascadeAnalysis, errors: &mut Vec<String>) {
    match group.role {
        GroupCascadeRole::LeadRoot | GroupCascadeRole::IndependentRoot => {
            if group.best_parent_group_ref.is_some() {
                errors.push(format!(
                    "document_analysis group {} role {} must not have best_parent_group_ref",
                    group.group_ref,
                    group_role_name(group.role)
                ));
            }
            if group.visibility_floor != VisibilityFloor::NeverHidden {
                errors.push(format!(
                    "document_analysis group {} role {} must use visibility_floor never_hidden",
                    group.group_ref,
                    group_role_name(group.role)
                ));
            }
        }
        GroupCascadeRole::Uncertain => {
            if group.visibility_floor != VisibilityFloor::NeverHidden {
                errors.push(format!(
                    "document_analysis group {} role uncertain must use visibility_floor never_hidden",
                    group.group_ref
                ));
            }
        }
        GroupCascadeRole::FollowOn | GroupCascadeRole::Duplicate => {
            if group.best_parent_group_ref.is_none() {
                errors.push(format!(
                    "document_analysis group {} role {} must set best_parent_group_ref",
                    group.group_ref,
                    group_role_name(group.role)
                ));
            }
        }
    }
}

fn validate_group_episode_membership(
    group: &GroupCascadeAnalysis,
    group_refs: &HashSet<String>,
    episode_members_by_ref: &HashMap<String, HashSet<String>>,
    episode_lead_by_ref: &HashMap<String, String>,
    errors: &mut Vec<String>,
) {
    if let Some(parent_ref) = group.best_parent_group_ref.as_deref() {
        if !group_refs.contains(parent_ref) {
            errors.push(format!(
                "document_analysis group {} best_parent_group_ref {} does not exist",
                group.group_ref, parent_ref
            ));
        } else if parent_ref == group.group_ref {
            errors.push(format!(
                "document_analysis group {} best_parent_group_ref must not reference itself",
                group.group_ref
            ));
        }
    }

    if let Some(episode_ref) = group.episode_ref.as_deref() {
        let Some(member_refs) = episode_members_by_ref.get(episode_ref) else {
            errors.push(format!(
                "document_analysis group {} references missing episode {}",
                group.group_ref, episode_ref
            ));
            return;
        };
        if !member_refs.contains(&group.group_ref) {
            errors.push(format!(
                "document_analysis group {} episode_ref {} does not include the group in member_group_refs",
                group.group_ref, episode_ref
            ));
        }
        if matches!(
            group.role,
            GroupCascadeRole::LeadRoot | GroupCascadeRole::IndependentRoot
        ) && episode_lead_by_ref
            .get(episode_ref)
            .is_some_and(|lead_group_ref| lead_group_ref != &group.group_ref)
        {
            errors.push(format!(
                "document_analysis group {} role {} must not be assigned to episode {} as a non-lead member",
                group.group_ref,
                group_role_name(group.role),
                episode_ref
            ));
        }
    }
}

fn group_role_name(role: GroupCascadeRole) -> &'static str {
    match role {
        GroupCascadeRole::LeadRoot => "lead_root",
        GroupCascadeRole::IndependentRoot => "independent_root",
        GroupCascadeRole::FollowOn => "follow_on",
        GroupCascadeRole::Duplicate => "duplicate",
        GroupCascadeRole::Uncertain => "uncertain",
    }
}

fn validate_score(scope: &str, score: Option<crate::Score>, errors: &mut Vec<String>) {
    if let Some(score) = score
        && !(0.0..=1.0).contains(&score.into_inner())
    {
        errors.push(format!("{scope} must be within 0.0..=1.0"));
    }
}
