use diag_core::{
    DiagnosticDocument, DiagnosticEvidenceGraph, DiagnosticNode, EvidenceAuthority, EvidenceEdge,
    EvidenceEdgeKind, EvidenceRecord, EvidenceTarget, RepairProofClass, RepairUnitAnalysis,
    RepairUnitStats,
};

pub(crate) fn materialize_evidence_graph(document: &mut DiagnosticDocument) {
    let mut graph = DiagnosticEvidenceGraph::default();
    for capture in &document.captures {
        graph.evidence.push(EvidenceRecord {
            evidence_ref: format!("capture:{}", capture.id),
            target: EvidenceTarget::Capture {
                capture_ref: capture.id.clone(),
            },
            hidden: false,
            unresolved: false,
        });
    }
    for node in &document.diagnostics {
        materialize_node(document, node, None, &mut graph);
    }
    graph
        .evidence
        .sort_by(|a, b| a.evidence_ref.cmp(&b.evidence_ref));
    graph.edges.sort_by(|a, b| a.edge_ref.cmp(&b.edge_ref));
    let referenced_fact_count = graph.evidence.len() as u32;
    let unresolved_evidence_count =
        graph.evidence.iter().filter(|item| item.unresolved).count() as u32;
    let repair = RepairUnitAnalysis {
        evidence_graph: graph,
        repair_units: Vec::new(),
        stats: RepairUnitStats {
            visible_unit_count: 0,
            unresolved_evidence_count,
            merged_evidence_count: 0,
            exact_duplicate_count: 0,
            referenced_fact_count,
            total_fact_count: referenced_fact_count,
        },
    };
    document
        .document_analysis
        .get_or_insert_with(Default::default)
        .repair_analysis = Some(repair);
}

fn materialize_node(
    document: &DiagnosticDocument,
    node: &DiagnosticNode,
    parent_ref: Option<&str>,
    graph: &mut DiagnosticEvidenceGraph,
) {
    let node_ref = format!("node:{}", node.id);
    graph.evidence.push(EvidenceRecord {
        evidence_ref: node_ref.clone(),
        target: EvidenceTarget::DiagnosticNode {
            node_ref: node.id.clone(),
        },
        hidden: false,
        unresolved: matches!(
            node.node_completeness,
            diag_core::NodeCompleteness::Passthrough
        ),
    });
    let authority = authority_for(node, document);
    if let Some(parent) = parent_ref {
        graph.edges.push(EvidenceEdge {
            edge_ref: format!("hierarchy:{parent}->{node_ref}"),
            from_evidence_ref: parent.to_string(),
            to_evidence_ref: node_ref.clone(),
            kind: EvidenceEdgeKind::CompilerHierarchy,
            authority,
            proof_class: if matches!(authority, EvidenceAuthority::CompilerDeclared) {
                RepairProofClass::Proven
            } else {
                RepairProofClass::Strong
            },
            evidence_tags: vec!["preserved_child_order".into()],
        });
    }
    for capture_ref in &node.provenance.capture_refs {
        let capture_evidence = format!("capture:{capture_ref}");
        graph.edges.push(EvidenceEdge {
            edge_ref: format!("provenance:{node_ref}->{capture_evidence}"),
            from_evidence_ref: node_ref.clone(),
            to_evidence_ref: capture_evidence,
            kind: EvidenceEdgeKind::ContextFrontier,
            authority,
            proof_class: RepairProofClass::Proven,
            evidence_tags: vec!["raw_provenance".into()],
        });
        if let Some(capture) = document
            .captures
            .iter()
            .find(|capture| &capture.id == capture_ref)
            && let Some(raw) = capture.inline_text.as_deref()
            && let Some(start) = raw.find(&node.message.raw_text)
        {
            let end = start + node.message.raw_text.len();
            let span_ref = format!("raw:{}:{capture_ref}:{start}:{end}", node.id);
            graph.evidence.push(EvidenceRecord {
                evidence_ref: span_ref.clone(),
                target: EvidenceTarget::RawChunk {
                    capture_ref: capture_ref.clone(),
                    start: start as u64,
                    end: end as u64,
                },
                hidden: false,
                unresolved: false,
            });
            graph.edges.push(EvidenceEdge {
                edge_ref: format!("span:{node_ref}->{span_ref}"),
                from_evidence_ref: node_ref.clone(),
                to_evidence_ref: span_ref,
                kind: EvidenceEdgeKind::ContextFrontier,
                authority,
                proof_class: RepairProofClass::Proven,
                evidence_tags: vec!["exact_raw_byte_span".into()],
            });
        }
    }
    for child in &node.children {
        materialize_node(document, child, Some(&node_ref), graph);
    }
}

fn authority_for(node: &DiagnosticNode, document: &DiagnosticDocument) -> EvidenceAuthority {
    if matches!(
        node.provenance.source,
        diag_core::ProvenanceSource::ResidualText
    ) {
        return EvidenceAuthority::TextParserDerived;
    }
    let structured = node.provenance.capture_refs.iter().any(|reference| {
        document
            .captures
            .iter()
            .find(|capture| &capture.id == reference)
            .is_some_and(|capture| {
                matches!(
                    capture.kind,
                    diag_core::ArtifactKind::GccSarif | diag_core::ArtifactKind::GccJson
                )
            })
    });
    if structured {
        EvidenceAuthority::CompilerDeclared
    } else {
        EvidenceAuthority::Heuristic
    }
}
