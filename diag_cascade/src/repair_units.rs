use diag_core::{
    DiagnosticDocument, DiagnosticNode, EvidenceAuthority, EvidenceEdge, EvidenceEdgeKind,
    EvidenceRecord, EvidenceTarget, IntegrityIssue, IssueSeverity, IssueStage, RepairObservability,
    RepairProofClass, RepairUnit, RepairUnitAnalysis, RepairUnitStats, VisibilityFloor,
};
use std::collections::{BTreeMap, BTreeSet};

/// Runs deterministic, evidence-constrained `RepairUnit` inference.
///
/// Only proof-bearing structural edges may reduce the visible unit count.
/// Family names, normalized messages, proximity, ordering, and float scores are
/// deliberately absent from this algorithm.
pub fn infer_repair_units(document: &mut DiagnosticDocument) {
    ensure_evidence_records(document);
    add_syntax_fixit_edges(document);
    add_structural_frontier_edges(document);
    add_exact_duplicate_edges(document);

    let Some(graph) = document
        .document_analysis
        .as_ref()
        .and_then(|analysis| analysis.repair_analysis.as_ref())
        .map(|repair| repair.evidence_graph.clone())
    else {
        return;
    };

    let node_map = all_nodes(document);
    let top_level_refs = document
        .diagnostics
        .iter()
        .map(|node| format!("node:{}", node.id))
        .collect::<BTreeSet<_>>();
    let mut union = DisjointSet::new(
        graph
            .evidence
            .iter()
            .filter_map(node_evidence_ref)
            .collect(),
    );
    for (reference, node) in &node_map {
        if (top_level_refs.contains(reference)
            || node.semantic_role == diag_core::SemanticRole::Root)
            && let Some(anchor) = repair_anchor(node)
        {
            union.add_anchor(reference, anchor);
        }
    }
    let mut rationale = BTreeMap::<String, Vec<String>>::new();
    let mut conflicts = Vec::new();

    for edge in &graph.edges {
        if !is_must_link(edge) {
            continue;
        }
        let left = edge.from_evidence_ref.as_str();
        let right = edge.to_evidence_ref.as_str();
        if !union.contains(left) || !union.contains(right) {
            continue;
        }
        if independent_anchor_conflict(&union, left, right, edge.kind) {
            conflicts.push(edge.edge_ref.clone());
            continue;
        }
        let root = union.union(left, right);
        rationale
            .entry(root)
            .or_default()
            .push(edge.edge_ref.clone());
    }

    let mut components = BTreeMap::<String, Vec<String>>::new();
    for node_ref in union.items() {
        let root = union.find(node_ref);
        components.entry(root).or_default().push(node_ref.clone());
    }

    let mut units = Vec::new();
    let mut exact_duplicate_count = 0u32;
    for mut members in components.into_values() {
        members.sort();
        let unit_roots = members
            .iter()
            .filter(|reference| {
                top_level_refs.contains(*reference)
                    || node_map
                        .get(*reference)
                        .is_some_and(|node| node.semantic_role == diag_core::SemanticRole::Root)
            })
            .cloned()
            .collect::<Vec<_>>();
        if unit_roots.is_empty() {
            continue;
        }
        let lead = unit_roots[0].clone();
        let member_set = members.iter().cloned().collect::<BTreeSet<_>>();
        let mut rationale_edges = graph
            .edges
            .iter()
            .filter(|edge| {
                member_set.contains(&edge.from_evidence_ref)
                    && member_set.contains(&edge.to_evidence_ref)
                    && is_must_link(edge)
            })
            .map(|edge| edge.edge_ref.clone())
            .collect::<Vec<_>>();
        rationale_edges.sort();
        rationale_edges.dedup();
        exact_duplicate_count += graph
            .edges
            .iter()
            .filter(|edge| {
                edge.kind == EvidenceEdgeKind::ExactDuplicate
                    && rationale_edges.contains(&edge.edge_ref)
            })
            .count() as u32;

        let proof_class = rationale_edges
            .iter()
            .filter_map(|edge_ref| graph.edges.iter().find(|edge| &edge.edge_ref == edge_ref))
            .map(|edge| edge.proof_class)
            .min_by_key(proof_rank)
            .unwrap_or(RepairProofClass::Unresolved);
        let mut fixit_anchors = members
            .iter()
            .filter_map(|reference| node_map.get(reference))
            .flat_map(|node| node.suggestions.iter())
            .flat_map(|suggestion| suggestion.edits.iter())
            .map(edit_anchor)
            .collect::<Vec<_>>();
        fixit_anchors.sort();
        fixit_anchors.dedup();
        let mut location_anchors = members
            .iter()
            .filter_map(|reference| node_map.get(reference))
            .filter_map(|node| repair_anchor(node))
            .collect::<Vec<_>>();
        location_anchors.sort();
        location_anchors.dedup();
        let mut context_anchors = members
            .iter()
            .filter_map(|reference| node_map.get(reference))
            .flat_map(|node| node.context_chains.iter())
            .flat_map(|chain| chain.frames.iter())
            .filter_map(context_frame_anchor)
            .collect::<Vec<_>>();
        context_anchors.sort();
        context_anchors.dedup();
        let mut grounded_actions = grounded_actions(&members, &node_map);
        grounded_actions.sort();
        grounded_actions.dedup();
        let mut raw_refs = members
            .iter()
            .filter_map(|reference| node_map.get(reference))
            .flat_map(|node| node.provenance.capture_refs.iter().cloned())
            .collect::<Vec<_>>();
        raw_refs.sort();
        raw_refs.dedup();

        units.push(RepairUnit {
            repair_unit_ref: format!("repair-unit:{}", stable_component_key(&members)),
            visible: true,
            lead_evidence_ref: lead,
            member_evidence_refs: members,
            primary_repair_anchors: if fixit_anchors.is_empty() {
                location_anchors.clone()
            } else {
                fixit_anchors
            },
            alternate_repair_anchors: location_anchors
                .into_iter()
                .chain(context_anchors)
                .collect(),
            proof_class,
            observability: if rationale_edges.is_empty() {
                RepairObservability::Unresolved
            } else {
                RepairObservability::Observable
            },
            grounded_action_refs: grounded_actions,
            visibility_floor: if matches!(
                proof_class,
                RepairProofClass::Tentative | RepairProofClass::Unresolved
            ) {
                VisibilityFloor::NeverHidden
            } else {
                VisibilityFloor::HiddenAllowed
            },
            raw_capture_refs: raw_refs,
            rationale_edge_refs: rationale_edges,
            legacy_group_refs: Vec::new(),
            legacy_episode_refs: Vec::new(),
        });
    }
    units.sort_by(|left, right| left.repair_unit_ref.cmp(&right.repair_unit_ref));

    let referenced = units
        .iter()
        .flat_map(|unit| unit.member_evidence_refs.iter())
        .collect::<BTreeSet<_>>()
        .len() as u32;
    let unresolved = units
        .iter()
        .filter(|unit| unit.proof_class == RepairProofClass::Unresolved)
        .map(|unit| unit.member_evidence_refs.len() as u32)
        .sum();
    for edge_ref in conflicts {
        document.integrity_issues.push(IntegrityIssue {
            severity: IssueSeverity::Warning,
            stage: IssueStage::Analyze,
            message: format!(
                "RepairUnit must-link rejected by independent repair-anchor constraint: {edge_ref}"
            ),
            provenance: None,
        });
    }
    let repair = document
        .document_analysis
        .as_mut()
        .and_then(|analysis| analysis.repair_analysis.as_mut())
        .expect("evidence graph was initialized");
    repair.stats = RepairUnitStats {
        visible_unit_count: units.len() as u32,
        unresolved_evidence_count: unresolved,
        merged_evidence_count: units
            .iter()
            .map(|unit| unit.member_evidence_refs.len().saturating_sub(1) as u32)
            .sum(),
        exact_duplicate_count,
        referenced_fact_count: referenced,
        total_fact_count: repair.evidence_graph.evidence.len() as u32,
    };
    repair.repair_units = units;
}

/// Compatibility name for callers that only require family-independent seeds.
pub fn seed_repair_units_without_family(document: &mut DiagnosticDocument) {
    infer_repair_units(document);
}

fn is_must_link(edge: &EvidenceEdge) -> bool {
    edge.proof_class == RepairProofClass::Proven
        && matches!(
            edge.kind,
            EvidenceEdgeKind::CompilerHierarchy
                | EvidenceEdgeKind::SameFixitSpan
                | EvidenceEdgeKind::ContextFrontier
                | EvidenceEdgeKind::ExactDuplicate
                | EvidenceEdgeKind::SymbolRelation
        )
        && !matches!(edge.authority, EvidenceAuthority::Heuristic)
}

fn independent_anchor_conflict(
    union: &DisjointSet,
    left: &str,
    right: &str,
    kind: EvidenceEdgeKind,
) -> bool {
    if matches!(
        kind,
        EvidenceEdgeKind::ExactDuplicate | EvidenceEdgeKind::SameFixitSpan
    ) {
        return false;
    }
    let left_anchors = union.anchors(left);
    let right_anchors = union.anchors(right);
    !left_anchors.is_empty()
        && !right_anchors.is_empty()
        && left_anchors.is_disjoint(&right_anchors)
}

fn add_syntax_fixit_edges(document: &mut DiagnosticDocument) {
    let mut edits = BTreeMap::<String, Vec<String>>::new();
    for node in &document.diagnostics {
        collect_syntax_edits(node, &mut edits);
    }
    let Some(repair) = document
        .document_analysis
        .as_mut()
        .and_then(|analysis| analysis.repair_analysis.as_mut())
    else {
        return;
    };
    for (edit_key, mut refs) in edits {
        refs.sort();
        refs.dedup();
        for right in refs.iter().skip(1) {
            let left = &refs[0];
            repair.evidence_graph.edges.push(EvidenceEdge {
                edge_ref: format!("same-fixit:{left}->{right}:{edit_key}"),
                from_evidence_ref: left.clone(),
                to_evidence_ref: right.clone(),
                kind: EvidenceEdgeKind::SameFixitSpan,
                authority: EvidenceAuthority::CompilerDeclared,
                proof_class: RepairProofClass::Proven,
                evidence_tags: vec!["compiler_fixit_exact_edit".into()],
            });
        }
    }
    repair
        .evidence_graph
        .edges
        .sort_by(|left, right| left.edge_ref.cmp(&right.edge_ref));
    repair
        .evidence_graph
        .edges
        .dedup_by(|left, right| left.edge_ref == right.edge_ref);
}

fn collect_syntax_edits(node: &DiagnosticNode, out: &mut BTreeMap<String, Vec<String>>) {
    if matches!(
        node.phase,
        diag_core::Phase::Parse | diag_core::Phase::Preprocess
    ) {
        for suggestion in &node.suggestions {
            for edit in &suggestion.edits {
                out.entry(edit_identity(edit))
                    .or_default()
                    .push(format!("node:{}", node.id));
            }
        }
    }
    for child in &node.children {
        collect_syntax_edits(child, out);
    }
}

fn edit_identity(edit: &diag_core::TextEdit) -> String {
    format!(
        "{}:{}:{}:{}:{}:{:?}:{}",
        edit.path,
        edit.start_line,
        edit.start_column,
        edit.end_line,
        edit.end_column,
        edit.boundary,
        edit.replacement
    )
}

fn edit_anchor(edit: &diag_core::TextEdit) -> String {
    format!(
        "{}:{}:{}-{}:{}",
        edit.path, edit.start_line, edit.start_column, edit.end_line, edit.end_column
    )
}

fn grounded_actions(members: &[String], nodes: &BTreeMap<String, &DiagnosticNode>) -> Vec<String> {
    let mut actions = Vec::new();
    for reference in members {
        let Some(node) = nodes.get(reference) else {
            continue;
        };
        for (suggestion_index, suggestion) in node.suggestions.iter().enumerate() {
            for edit_index in 0..suggestion.edits.len() {
                actions.push(format!("fixit:{reference}:{suggestion_index}:{edit_index}"));
            }
        }
    }
    actions
}

fn add_structural_frontier_edges(document: &mut DiagnosticDocument) {
    let mut buckets = BTreeMap::<String, Vec<(String, RepairProofClass)>>::new();
    for node in &document.diagnostics {
        let Some(invocation_anchor) = repair_anchor(node) else {
            continue;
        };
        for chain in &node.context_chains {
            if !matches!(
                chain.kind,
                diag_core::ContextChainKind::TemplateInstantiation
                    | diag_core::ContextChainKind::MacroExpansion
            ) {
                continue;
            }
            let frontier = chain
                .frames
                .iter()
                .filter_map(context_frame_anchor)
                .collect::<Vec<_>>()
                .join(">");
            if frontier.is_empty() {
                continue;
            }
            let proof = if matches!(
                node.provenance.source,
                diag_core::ProvenanceSource::ResidualText
            ) {
                RepairProofClass::Strong
            } else {
                RepairProofClass::Proven
            };
            buckets
                .entry(format!("{:?}|{invocation_anchor}|{frontier}", chain.kind))
                .or_default()
                .push((format!("node:{}", node.id), proof));
        }
    }
    let Some(repair) = document
        .document_analysis
        .as_mut()
        .and_then(|analysis| analysis.repair_analysis.as_mut())
    else {
        return;
    };
    for (frontier, mut refs) in buckets {
        refs.sort_by(|left, right| left.0.cmp(&right.0));
        refs.dedup_by(|left, right| left.0 == right.0);
        for right in refs.iter().skip(1) {
            let left = &refs[0];
            let proof = if left.1 == RepairProofClass::Proven && right.1 == RepairProofClass::Proven
            {
                RepairProofClass::Proven
            } else {
                RepairProofClass::Strong
            };
            repair.evidence_graph.edges.push(EvidenceEdge {
                edge_ref: format!("frontier:{}->{}:{frontier}", left.0, right.0),
                from_evidence_ref: left.0.clone(),
                to_evidence_ref: right.0.clone(),
                kind: EvidenceEdgeKind::ContextFrontier,
                authority: if proof == RepairProofClass::Proven {
                    EvidenceAuthority::StructuredAdapterDerived
                } else {
                    EvidenceAuthority::TextParserDerived
                },
                proof_class: proof,
                evidence_tags: vec!["same_invocation_anchor_and_frontier".into()],
            });
        }
    }
    repair
        .evidence_graph
        .edges
        .sort_by(|left, right| left.edge_ref.cmp(&right.edge_ref));
    repair
        .evidence_graph
        .edges
        .dedup_by(|left, right| left.edge_ref == right.edge_ref);
}

fn context_frame_anchor(frame: &diag_core::ContextFrame) -> Option<String> {
    Some(format!(
        "{}:{}:{}",
        frame.path.as_deref()?,
        frame.line?,
        frame.column.unwrap_or(1)
    ))
}

fn add_exact_duplicate_edges(document: &mut DiagnosticDocument) {
    let mut buckets = BTreeMap::<String, Vec<String>>::new();
    for node in &document.diagnostics {
        buckets
            .entry(exact_identity(node))
            .or_default()
            .push(format!("node:{}", node.id));
    }
    let Some(repair) = document
        .document_analysis
        .as_mut()
        .and_then(|analysis| analysis.repair_analysis.as_mut())
    else {
        return;
    };
    for refs in buckets.into_values().filter(|refs| refs.len() > 1) {
        for right in refs.iter().skip(1) {
            let left = &refs[0];
            repair.evidence_graph.edges.push(EvidenceEdge {
                edge_ref: format!("exact-duplicate:{left}->{right}"),
                from_evidence_ref: left.clone(),
                to_evidence_ref: right.clone(),
                kind: EvidenceEdgeKind::ExactDuplicate,
                authority: EvidenceAuthority::StructuredAdapterDerived,
                proof_class: RepairProofClass::Proven,
                evidence_tags: vec!["full_structural_identity".into()],
            });
        }
    }
    repair
        .evidence_graph
        .edges
        .sort_by(|left, right| left.edge_ref.cmp(&right.edge_ref));
    repair
        .evidence_graph
        .edges
        .dedup_by(|left, right| left.edge_ref == right.edge_ref);
}

fn exact_identity(node: &DiagnosticNode) -> String {
    let locations = node
        .locations
        .iter()
        .map(|location| format!("{:?}", location))
        .collect::<Vec<_>>()
        .join("|");
    format!(
        "{:?}|{:?}|{:?}|{:?}|{}|{}",
        node.origin,
        node.phase,
        node.severity,
        node.semantic_role,
        node.message.raw_text,
        locations
    )
}

fn ensure_evidence_records(document: &mut DiagnosticDocument) {
    let existing = document
        .document_analysis
        .as_ref()
        .and_then(|analysis| analysis.repair_analysis.as_ref())
        .is_some_and(|repair| !repair.evidence_graph.evidence.is_empty());
    if existing {
        return;
    }
    let mut evidence = document
        .captures
        .iter()
        .map(|capture| EvidenceRecord {
            evidence_ref: format!("capture:{}", capture.id),
            target: EvidenceTarget::Capture {
                capture_ref: capture.id.clone(),
            },
            hidden: false,
            unresolved: false,
        })
        .collect::<Vec<_>>();
    let mut edges = Vec::new();
    for node in &document.diagnostics {
        collect_node_records(node, None, &mut evidence, &mut edges);
    }
    evidence.sort_by(|left, right| left.evidence_ref.cmp(&right.evidence_ref));
    let repair = document
        .document_analysis
        .get_or_insert_with(Default::default)
        .repair_analysis
        .get_or_insert_with(RepairUnitAnalysis::default);
    repair.evidence_graph.evidence = evidence;
    repair.evidence_graph.edges = edges;
}

fn collect_node_records(
    node: &DiagnosticNode,
    parent: Option<&str>,
    evidence: &mut Vec<EvidenceRecord>,
    edges: &mut Vec<EvidenceEdge>,
) {
    let reference = format!("node:{}", node.id);
    evidence.push(EvidenceRecord {
        evidence_ref: reference.clone(),
        target: EvidenceTarget::DiagnosticNode {
            node_ref: node.id.clone(),
        },
        hidden: false,
        unresolved: false,
    });
    if let Some(parent) = parent {
        edges.push(EvidenceEdge {
            edge_ref: format!("hierarchy:{parent}->{reference}"),
            from_evidence_ref: parent.into(),
            to_evidence_ref: reference.clone(),
            kind: EvidenceEdgeKind::CompilerHierarchy,
            authority: EvidenceAuthority::CompilerDeclared,
            proof_class: RepairProofClass::Proven,
            evidence_tags: vec!["compiler_child_tree".into()],
        });
    }
    for child in &node.children {
        collect_node_records(child, Some(&reference), evidence, edges);
    }
}

fn all_nodes(document: &DiagnosticDocument) -> BTreeMap<String, &DiagnosticNode> {
    fn visit<'a>(node: &'a DiagnosticNode, out: &mut BTreeMap<String, &'a DiagnosticNode>) {
        out.insert(format!("node:{}", node.id), node);
        for child in &node.children {
            visit(child, out);
        }
    }
    let mut nodes = BTreeMap::new();
    for node in &document.diagnostics {
        visit(node, &mut nodes);
    }
    nodes
}

fn repair_anchor(node: &DiagnosticNode) -> Option<String> {
    node.primary_location().map(|location| {
        format!(
            "{}:{}:{}",
            location.path_raw(),
            location.line(),
            location.column()
        )
    })
}

fn node_evidence_ref(record: &EvidenceRecord) -> Option<String> {
    matches!(record.target, EvidenceTarget::DiagnosticNode { .. })
        .then(|| record.evidence_ref.clone())
}

fn proof_rank(proof: &RepairProofClass) -> u8 {
    match proof {
        RepairProofClass::Proven => 3,
        RepairProofClass::Strong => 2,
        RepairProofClass::Tentative => 1,
        RepairProofClass::Unresolved => 0,
    }
}

fn stable_component_key(members: &[String]) -> String {
    members.join("+").replace(':', "-")
}

#[derive(Debug)]
struct DisjointSet {
    parents: BTreeMap<String, String>,
    anchors: BTreeMap<String, BTreeSet<String>>,
}

impl DisjointSet {
    fn new(items: Vec<String>) -> Self {
        Self {
            parents: items.into_iter().map(|item| (item.clone(), item)).collect(),
            anchors: BTreeMap::new(),
        }
    }

    fn find(&self, item: &str) -> String {
        let mut current = item;
        while let Some(parent) = self.parents.get(current) {
            if parent == current {
                return current.to_string();
            }
            current = parent;
        }
        item.to_string()
    }

    fn contains(&self, item: &str) -> bool {
        self.parents.contains_key(item)
    }

    fn union(&mut self, left: &str, right: &str) -> String {
        let left_root = self.find(left);
        let right_root = self.find(right);
        let (root, child) = if left_root <= right_root {
            (left_root, right_root)
        } else {
            (right_root, left_root)
        };
        let child_anchors = self.anchors.remove(&child).unwrap_or_default();
        self.parents.insert(child, root.clone());
        self.anchors
            .entry(root.clone())
            .or_default()
            .extend(child_anchors);
        root
    }

    fn add_anchor(&mut self, item: &str, anchor: String) {
        let root = self.find(item);
        self.anchors.entry(root).or_default().insert(anchor);
    }

    fn anchors(&self, item: &str) -> BTreeSet<String> {
        self.anchors
            .get(&self.find(item))
            .cloned()
            .unwrap_or_default()
    }

    fn items(&self) -> impl Iterator<Item = &String> {
        self.parents.keys()
    }
}
