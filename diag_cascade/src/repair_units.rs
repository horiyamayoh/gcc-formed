use diag_core::{
    DiagnosticDocument, DiagnosticNode, EvidenceRecord, EvidenceTarget, RepairObservability,
    RepairProofClass, RepairUnit, RepairUnitAnalysis, RepairUnitStats, VisibilityFloor,
};
use std::collections::BTreeSet;

/// Conservative family-independent `RepairUnit` seed API.
///
/// Every compiler top-level diagnostic remains visible until structural
/// evidence proves membership. Family and localized wording are never read.
pub fn seed_repair_units_without_family(document: &mut DiagnosticDocument) {
    let mut seed_evidence = document
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
    for node in &document.diagnostics {
        collect_evidence_records(node, &mut seed_evidence);
    }
    let analysis = document
        .document_analysis
        .get_or_insert_with(Default::default);
    let repair = analysis
        .repair_analysis
        .get_or_insert_with(RepairUnitAnalysis::default);
    if repair.evidence_graph.evidence.is_empty() {
        repair.evidence_graph.evidence = seed_evidence;
    }
    let available = repair
        .evidence_graph
        .evidence
        .iter()
        .map(|item| item.evidence_ref.as_str())
        .collect::<BTreeSet<_>>();
    let mut units = Vec::new();
    for node in &document.diagnostics {
        let lead = format!("node:{}", node.id);
        let mut members = Vec::new();
        collect_node_evidence(node, &available, &mut members);
        if !members.contains(&lead) {
            members.push(lead.clone());
        }
        members.sort();
        members.dedup();
        let mut raw = Vec::new();
        collect_capture_refs(node, &mut raw);
        raw.sort();
        raw.dedup();
        units.push(RepairUnit {
            repair_unit_ref: format!("repair-unit:{}", node.id),
            visible: true,
            lead_evidence_ref: lead,
            member_evidence_refs: members,
            primary_repair_anchors: node
                .primary_location()
                .map(|location| {
                    vec![format!(
                        "{}:{}:{}",
                        location.path_raw(),
                        location.line(),
                        location.column()
                    )]
                })
                .unwrap_or_default(),
            alternate_repair_anchors: Vec::new(),
            proof_class: RepairProofClass::Unresolved,
            observability: RepairObservability::Unresolved,
            grounded_action_refs: Vec::new(),
            visibility_floor: VisibilityFloor::NeverHidden,
            raw_capture_refs: raw,
            rationale_edge_refs: Vec::new(),
            legacy_group_refs: Vec::new(),
            legacy_episode_refs: Vec::new(),
        });
    }
    units.sort_by(|a, b| a.repair_unit_ref.cmp(&b.repair_unit_ref));
    repair.stats = RepairUnitStats {
        visible_unit_count: units.len() as u32,
        unresolved_evidence_count: units
            .iter()
            .map(|unit| unit.member_evidence_refs.len() as u32)
            .sum(),
        merged_evidence_count: 0,
        exact_duplicate_count: 0,
        referenced_fact_count: repair.evidence_graph.evidence.len() as u32,
        total_fact_count: repair.evidence_graph.evidence.len() as u32,
    };
    repair.repair_units = units;
}

fn collect_evidence_records(node: &DiagnosticNode, out: &mut Vec<EvidenceRecord>) {
    out.push(EvidenceRecord {
        evidence_ref: format!("node:{}", node.id),
        target: EvidenceTarget::DiagnosticNode {
            node_ref: node.id.clone(),
        },
        hidden: false,
        unresolved: false,
    });
    for child in &node.children {
        collect_evidence_records(child, out);
    }
}

fn collect_node_evidence(node: &DiagnosticNode, available: &BTreeSet<&str>, out: &mut Vec<String>) {
    let reference = format!("node:{}", node.id);
    if available.contains(reference.as_str()) {
        out.push(reference);
    }
    for child in &node.children {
        collect_node_evidence(child, available, out);
    }
}

fn collect_capture_refs(node: &DiagnosticNode, out: &mut Vec<String>) {
    out.extend(node.provenance.capture_refs.iter().cloned());
    for child in &node.children {
        collect_capture_refs(child, out);
    }
}
