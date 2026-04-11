//! Fallback and passthrough document/node builders.

use diag_core::{
    AnalysisOverlay, Confidence, DiagnosticDocument, DiagnosticNode, DocumentCompleteness,
    IntegrityIssue, IssueSeverity, IssueStage, MessageText, NodeCompleteness, Origin, Phase,
    ProducerInfo, Provenance, ProvenanceSource, RunInfo, SemanticRole, Severity, ToolInfo,
};
use diag_rulepack::checked_in_rulepack_version;

/// Build a [`ProducerInfo`] for the given adapter version string.
pub fn producer_for_version(version: &str) -> ProducerInfo {
    ProducerInfo {
        name: "gcc-formed".to_string(),
        version: version.to_string(),
        git_revision: option_env!("FORMED_GIT_COMMIT").map(ToString::to_string),
        build_profile: option_env!("FORMED_BUILD_PROFILE").map(ToString::to_string),
        rulepack_version: Some(checked_in_rulepack_version().to_string()),
    }
}

/// Build a [`ToolInfo`] describing a GCC-family backend tool.
pub fn tool_for_backend(name: &str, version: Option<String>) -> ToolInfo {
    ToolInfo {
        name: name.to_string(),
        version,
        component: None,
        vendor: Some("GNU".to_string()),
    }
}

pub(crate) fn passthrough_document(producer: &ProducerInfo, run: &RunInfo) -> DiagnosticDocument {
    DiagnosticDocument {
        document_id: format!("passthrough-{}", run.invocation_id),
        schema_version: diag_core::IR_SPEC_VERSION.to_string(),
        document_completeness: DocumentCompleteness::Passthrough,
        producer: producer.clone(),
        run: run.clone(),
        captures: Vec::new(),
        integrity_issues: Vec::new(),
        diagnostics: Vec::new(),
        fingerprints: None,
    }
}

pub(crate) fn fallback_document(
    producer: &ProducerInfo,
    run: &RunInfo,
    completeness: DocumentCompleteness,
    stderr_text: &str,
    integrity_message: String,
    capture_ref: Option<&str>,
) -> DiagnosticDocument {
    let mut document = passthrough_document(producer, run);
    document.document_completeness = completeness;
    document.integrity_issues.push(IntegrityIssue {
        severity: IssueSeverity::Error,
        stage: IssueStage::Parse,
        message: integrity_message,
        provenance: capture_ref.map(|capture_ref| Provenance {
            source: ProvenanceSource::Compiler,
            capture_refs: vec![capture_ref.to_string()],
        }),
    });
    if !stderr_text.trim().is_empty() {
        document.diagnostics.push(passthrough_node(stderr_text));
    }
    document
}

pub(crate) fn failed_document(
    producer: &ProducerInfo,
    run: &RunInfo,
    stderr_text: &str,
    integrity_message: String,
    capture_ref: Option<&str>,
) -> DiagnosticDocument {
    fallback_document(
        producer,
        run,
        DocumentCompleteness::Failed,
        stderr_text,
        integrity_message,
        capture_ref,
    )
}

pub(crate) fn passthrough_node(stderr_text: &str) -> DiagnosticNode {
    DiagnosticNode {
        id: "passthrough-0".to_string(),
        origin: Origin::Wrapper,
        phase: Phase::Unknown,
        severity: Severity::Error,
        semantic_role: SemanticRole::Passthrough,
        message: MessageText {
            raw_text: stderr_text.to_string(),
            normalized_text: None,
            locale: None,
        },
        locations: Vec::new(),
        children: Vec::new(),
        suggestions: Vec::new(),
        context_chains: Vec::new(),
        symbol_context: None,
        node_completeness: NodeCompleteness::Passthrough,
        provenance: Provenance {
            source: ProvenanceSource::ResidualText,
            capture_refs: vec!["stderr.raw".to_string()],
        },
        analysis: Some(AnalysisOverlay {
            family: Some("passthrough".into()),
            family_version: None,
            family_confidence: None,
            root_cause_score: None,
            actionability_score: None,
            user_code_priority: None,
            headline: Some("showing conservative wrapper view".into()),
            first_action_hint: Some(
                "inspect the preserved raw diagnostics and rerun with --formed-debug-refs=capture_ref if needed"
                    .into(),
            ),
            confidence: Some(Confidence::Low.score()),
            preferred_primary_location_id: None,
            rule_id: Some("rule.family_seed.passthrough".into()),
            matched_conditions: vec!["semantic_role=passthrough".into()],
            suppression_reason: Some("generic_fallback".to_string()),
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
