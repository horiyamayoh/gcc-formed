use std::collections::{HashMap, HashSet};

use semver::Version;

use crate::{
    ArtifactKind, ArtifactStorage, DiagnosticDocument, DiagnosticNode, DocumentCompleteness,
    IntegrityIssue, Location, NodeCompleteness, Phase, Provenance, ProvenanceSource, SemanticRole,
    ValidationErrors,
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
