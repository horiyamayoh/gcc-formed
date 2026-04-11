//! Core IR types for the gcc-formed diagnostic pipeline.
//!
//! This crate defines the intermediate representation (IR) shared between the adapter,
//! enrichment, and rendering stages. The main types are:
//!
//! - [`DiagnosticDocument`] -- top-level envelope carrying metadata, captures, and diagnostics.
//! - [`DiagnosticNode`] -- a single diagnostic with locations, suggestions, and analysis.
//! - [`Location`] / [`FileRef`] / [`SourcePoint`] / [`SourceRange`] -- source-code coordinates.
//! - [`AnalysisOverlay`] -- enrichment-stage annotations (family, confidence, scores).
//! - [`FingerprintSet`] -- deterministic hashes for drift detection.
//!
//! All types derive `Serialize`/`Deserialize` so the IR can be round-tripped through JSON.

mod analysis;
mod capture;
mod confidence;
mod document;
mod fingerprint;
mod location;
mod types;
mod validation;

pub use analysis::*;
pub use capture::*;
pub use confidence::*;
pub use document::*;
pub use fingerprint::*;
pub use location::*;
pub use types::*;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::location::ownership_reason_key;
    use ordered_float::OrderedFloat;

    fn sample_document() -> DiagnosticDocument {
        DiagnosticDocument {
            document_id: "doc-1".to_string(),
            schema_version: IR_SPEC_VERSION.to_string(),
            document_completeness: DocumentCompleteness::Complete,
            producer: ProducerInfo {
                name: "gcc-formed".to_string(),
                version: "0.1.0".to_string(),
                git_revision: None,
                build_profile: Some("test".to_string()),
                rulepack_version: None,
            },
            run: RunInfo {
                invocation_id: "inv-1".to_string(),
                invoked_as: Some("gcc-formed".to_string()),
                argv_redacted: vec!["gcc".to_string(), "-c".to_string(), "main.c".to_string()],
                cwd_display: Some("/tmp/project".to_string()),
                exit_status: 1,
                primary_tool: ToolInfo {
                    name: "gcc".to_string(),
                    version: Some("15.1.0".to_string()),
                    component: None,
                    vendor: Some("GNU".to_string()),
                },
                secondary_tools: Vec::new(),
                language_mode: Some(LanguageMode::C),
                target_triple: Some("x86_64-unknown-linux-gnu".to_string()),
                wrapper_mode: Some(WrapperSurface::Terminal),
            },
            captures: vec![CaptureArtifact {
                id: "stderr.raw".to_string(),
                kind: ArtifactKind::CompilerStderrText,
                media_type: "text/plain".to_string(),
                encoding: Some("utf-8".to_string()),
                digest_sha256: Some("deadbeef".to_string()),
                size_bytes: Some(12),
                storage: ArtifactStorage::Inline,
                inline_text: Some("main.c:1:1".to_string()),
                external_ref: None,
                produced_by: None,
            }],
            integrity_issues: Vec::new(),
            diagnostics: vec![DiagnosticNode {
                id: "root-1".to_string(),
                origin: Origin::Gcc,
                phase: Phase::Parse,
                severity: Severity::Error,
                semantic_role: SemanticRole::Root,
                message: MessageText {
                    raw_text: "expected ';' before '}' token".to_string(),
                    normalized_text: None,
                    locale: Some("C".to_string()),
                },
                locations: vec![
                    Location::caret("src/main.c", 4, 1, LocationRole::Primary)
                        .with_ownership(Ownership::User, ownership_reason_key(Ownership::User)),
                ],
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
                    family: Some("syntax".to_string()),
                    family_version: None,
                    family_confidence: None,
                    root_cause_score: None,
                    actionability_score: None,
                    user_code_priority: None,
                    headline: Some("syntax error".to_string()),
                    first_action_hint: Some("insert the missing semicolon".to_string()),
                    confidence: Some(Confidence::High.score()),
                    preferred_primary_location_id: Some("loc:src/main.c:4:1:4:1".to_string()),
                    rule_id: Some("rule.syntax.expected_or_before".to_string()),
                    matched_conditions: vec!["message_contains=expected".to_string()],
                    suppression_reason: None,
                    collapsed_child_ids: Vec::new(),
                    collapsed_chain_ids: Vec::new(),
                    group_ref: None,
                    reasons: Vec::new(),
                    policy_profile: None,
                    producer_version: None,
                }),
                fingerprints: None,
            }],
            fingerprints: None,
        }
    }

    #[test]
    fn validates_and_fingerprints_document() {
        let mut document = sample_document();
        assert!(document.validate().is_ok());
        document.refresh_fingerprints();
        assert!(document.fingerprints.is_some());
        assert!(document.diagnostics[0].fingerprints.is_some());
    }

    #[test]
    fn canonical_json_is_deterministic() {
        let document = sample_document();
        let left = document.canonical_json().unwrap();
        let right = document.canonical_json().unwrap();
        assert_eq!(left, right);
    }

    #[test]
    fn snapshot_variants_are_deterministic() {
        let mut document = sample_document();
        document.refresh_fingerprints();

        let facts_left = snapshot_json(&document, SnapshotKind::FactsOnly).unwrap();
        let facts_right = snapshot_json(&document, SnapshotKind::FactsOnly).unwrap();
        let analysis = snapshot_json(&document, SnapshotKind::AnalysisIncluded).unwrap();

        assert_eq!(facts_left, facts_right);
        assert!(facts_left.contains("<document>"));
        assert!(!facts_left.contains("syntax error"));
        assert!(analysis.contains("syntax error"));
    }

    #[test]
    fn rejects_duplicate_node_ids() {
        let mut document = sample_document();
        let duplicate = document.diagnostics[0].clone();
        document.diagnostics.push(duplicate);
        let errors = document.validate().unwrap_err();
        assert!(
            errors
                .errors
                .iter()
                .any(|error| error.contains("duplicate node id"))
        );
    }

    #[test]
    fn prefers_analysis_primary_location_id() {
        let mut document = sample_document();
        document.diagnostics[0].locations.push(Location::caret(
            "src/secondary.c",
            8,
            3,
            LocationRole::Primary,
        ));
        document.diagnostics[0]
            .analysis
            .as_mut()
            .unwrap()
            .preferred_primary_location_id = Some("loc:src/secondary.c:8:3:8:3".to_string());

        let location = document.diagnostics[0].primary_location().unwrap();

        assert_eq!(location.path_raw(), "src/secondary.c");
    }

    #[test]
    fn rejects_missing_preferred_primary_location() {
        let mut document = sample_document();
        document.diagnostics[0]
            .analysis
            .as_mut()
            .unwrap()
            .preferred_primary_location_id = Some("missing".to_string());

        let errors = document.validate().unwrap_err();

        assert!(
            errors
                .errors
                .iter()
                .any(|error| error.contains("preferred_primary_location_id"))
        );
    }

    #[test]
    fn rejects_unparseable_schema_version() {
        let mut document = sample_document();
        document.schema_version = "v1alpha".to_string();

        let errors = document.validate().unwrap_err();

        assert!(
            errors
                .errors
                .iter()
                .any(|error| error.contains("schema_version v1alpha must be parseable semver"))
        );
    }

    #[test]
    fn rejects_missing_capture_refs_across_document_scopes() {
        let mut document = sample_document();
        document.diagnostics[0].provenance.capture_refs = vec!["missing-node".to_string()];
        document.diagnostics[0].locations[0].provenance_override = Some(Provenance {
            source: ProvenanceSource::Policy,
            capture_refs: vec!["missing-location".to_string()],
        });
        document.integrity_issues.push(IntegrityIssue {
            severity: IssueSeverity::Warning,
            stage: IssueStage::Normalize,
            message: "capture drift".to_string(),
            provenance: Some(Provenance {
                source: ProvenanceSource::ResidualText,
                capture_refs: vec!["missing-issue".to_string()],
            }),
        });

        let errors = document.validate().unwrap_err();

        assert!(errors.errors.iter().any(|error| {
            error.contains("node root-1 provenance references missing capture missing-node")
        }));
        assert!(
            errors
                .errors
                .iter()
                .any(|error| error.contains("node root-1 location loc:src/main.c:4:1:4:1 provenance_override references missing capture missing-location"))
        );
        assert!(errors.errors.iter().any(|error| {
            error.contains("integrity_issue[0] provenance references missing capture missing-issue")
        }));
    }

    #[test]
    fn rejects_invalid_location_integrity() {
        let mut document = sample_document();
        document.diagnostics[0].locations[0]
            .anchor
            .as_mut()
            .unwrap()
            .line = 0;
        document.diagnostics[0].locations.push(Location {
            id: "loc:missing".to_string(),
            file: FileRef::new("src/missing.c"),
            anchor: None,
            range: None,
            role: LocationRole::Secondary,
            source_kind: LocationSourceKind::Other,
            label: None,
            ownership_override: None,
            provenance_override: None,
            source_excerpt_ref: None,
        });

        let errors = document.validate().unwrap_err();

        assert!(
            errors
                .errors
                .iter()
                .any(|error| error.contains("anchor line must be >= 1"))
        );
        assert!(
            errors
                .errors
                .iter()
                .any(|error| error.contains("must have anchor or range"))
        );
    }

    #[test]
    fn rejects_synthesized_nodes_with_non_wrapper_provenance() {
        let mut document = sample_document();
        document.diagnostics[0].node_completeness = NodeCompleteness::Synthesized;
        document.diagnostics[0].provenance.source = ProvenanceSource::Compiler;

        let errors = document.validate().unwrap_err();

        assert!(
            errors.errors.iter().any(|error| error.contains(
                "is synthesized but provenance.source is not wrapper_generated or policy"
            ))
        );
    }

    #[test]
    fn rejects_collapsed_child_ids_that_are_not_descendants() {
        let mut document = sample_document();
        document.diagnostics[0]
            .analysis
            .as_mut()
            .unwrap()
            .collapsed_child_ids = vec!["missing-child".to_string()];

        let errors = document.validate().unwrap_err();

        assert!(errors.errors.iter().any(|error| {
            error.contains("collapsed_child_id missing-child does not reference a descendant")
        }));
    }

    #[test]
    fn confidence_thresholds_follow_renderer_contract() {
        assert_eq!(
            DisclosureConfidence::from_score(Some(OrderedFloat(0.85))),
            DisclosureConfidence::Certain
        );
        assert_eq!(
            DisclosureConfidence::from_score(Some(OrderedFloat(0.84))),
            DisclosureConfidence::Likely
        );
        assert_eq!(
            DisclosureConfidence::from_score(Some(OrderedFloat(0.60))),
            DisclosureConfidence::Likely
        );
        assert_eq!(
            DisclosureConfidence::from_score(Some(OrderedFloat(0.59))),
            DisclosureConfidence::Possible
        );
        assert_eq!(
            DisclosureConfidence::from_score(Some(OrderedFloat(0.35))),
            DisclosureConfidence::Possible
        );
        assert_eq!(
            DisclosureConfidence::from_score(Some(OrderedFloat(0.34))),
            DisclosureConfidence::Hidden
        );
        assert_eq!(
            Confidence::from_score(Some(OrderedFloat(0.84))),
            Confidence::Medium
        );
        assert_eq!(
            Confidence::from_score(Some(OrderedFloat(0.34))),
            Confidence::Unknown
        );
    }
}
