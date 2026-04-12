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
                    family: Some("syntax".into()),
                    family_version: None,
                    family_confidence: None,
                    root_cause_score: None,
                    actionability_score: None,
                    user_code_priority: None,
                    headline: Some("syntax error".into()),
                    first_action_hint: Some("insert the missing semicolon".into()),
                    confidence: Some(Confidence::High.score()),
                    preferred_primary_location_id: Some("loc:src/main.c:4:1:4:1".to_string()),
                    rule_id: Some("rule.syntax.expected_or_before".into()),
                    matched_conditions: vec!["message_contains=expected".into()],
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
            document_analysis: None,
            fingerprints: None,
        }
    }

    fn sample_document_analysis() -> DocumentAnalysis {
        DocumentAnalysis {
            policy_profile: Some("default-aggressive".to_string()),
            producer_version: Some("0.2.0-beta.1".to_string()),
            episode_graph: EpisodeGraph {
                episodes: vec![DiagnosticEpisode {
                    episode_ref: "episode-1".to_string(),
                    lead_group_ref: "group-1".to_string(),
                    member_group_refs: vec!["group-1".to_string(), "group-2".to_string()],
                    family: Some("syntax".to_string()),
                    lead_root_score: Some(OrderedFloat(0.91)),
                    confidence: Some(OrderedFloat(0.88)),
                }],
                relations: vec![EpisodeRelation {
                    from_group_ref: "group-1".to_string(),
                    to_group_ref: "group-2".to_string(),
                    kind: EpisodeRelationKind::Cascade,
                    confidence: OrderedFloat(0.86),
                    evidence_tags: vec!["shared_primary_file".to_string()],
                }],
            },
            group_analysis: vec![
                GroupCascadeAnalysis {
                    group_ref: "group-1".to_string(),
                    episode_ref: Some("episode-1".to_string()),
                    role: GroupCascadeRole::LeadRoot,
                    best_parent_group_ref: None,
                    root_score: Some(OrderedFloat(0.91)),
                    independence_score: Some(OrderedFloat(0.88)),
                    suppress_likelihood: Some(OrderedFloat(0.06)),
                    summary_likelihood: Some(OrderedFloat(0.24)),
                    visibility_floor: VisibilityFloor::NeverHidden,
                    evidence_tags: vec!["user_owned_primary".to_string()],
                },
                GroupCascadeAnalysis {
                    group_ref: "group-2".to_string(),
                    episode_ref: Some("episode-1".to_string()),
                    role: GroupCascadeRole::FollowOn,
                    best_parent_group_ref: Some("group-1".to_string()),
                    root_score: Some(OrderedFloat(0.22)),
                    independence_score: Some(OrderedFloat(0.18)),
                    suppress_likelihood: Some(OrderedFloat(0.84)),
                    summary_likelihood: Some(OrderedFloat(0.62)),
                    visibility_floor: VisibilityFloor::SummaryOrExpandedOnly,
                    evidence_tags: vec!["parser_follow_on".to_string()],
                },
            ],
            stats: CascadeStats {
                independent_root_count: 1,
                dependent_follow_on_count: 1,
                duplicate_count: 0,
                uncertain_count: 0,
            },
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
        document.document_analysis = Some(sample_document_analysis());
        document.refresh_fingerprints();

        let facts_left = snapshot_json(&document, SnapshotKind::FactsOnly).unwrap();
        let facts_right = snapshot_json(&document, SnapshotKind::FactsOnly).unwrap();
        let analysis = snapshot_json(&document, SnapshotKind::AnalysisIncluded).unwrap();

        assert_eq!(facts_left, facts_right);
        assert!(facts_left.contains("<document>"));
        assert!(!facts_left.contains("syntax error"));
        assert!(!facts_left.contains("episode_graph"));
        assert!(analysis.contains("syntax error"));
        assert!(analysis.contains("episode_graph"));
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
    fn round_trips_document_analysis_via_serde() {
        let mut document = sample_document();
        document.document_analysis = Some(sample_document_analysis());

        let encoded = serde_json::to_string(&document).unwrap();
        let decoded: DiagnosticDocument = serde_json::from_str(&encoded).unwrap();

        assert_eq!(decoded.document_analysis, document.document_analysis);
        assert!(decoded.validate().is_ok());
    }

    #[test]
    fn rejects_self_referential_document_analysis_links() {
        let mut document = sample_document();
        let mut analysis = sample_document_analysis();
        analysis.group_analysis[0].best_parent_group_ref = Some("group-1".to_string());
        analysis.episode_graph.relations.push(EpisodeRelation {
            from_group_ref: "group-2".to_string(),
            to_group_ref: "group-2".to_string(),
            kind: EpisodeRelationKind::Context,
            confidence: OrderedFloat(0.42),
            evidence_tags: Vec::new(),
        });
        document.document_analysis = Some(analysis);

        let errors = document.validate().unwrap_err();

        assert!(
            errors
                .errors
                .iter()
                .any(|error| error.contains("best_parent_group_ref must not reference itself"))
        );
        assert!(
            errors
                .errors
                .iter()
                .any(|error| error.contains("must not self-reference"))
        );
    }

    #[test]
    fn rejects_cyclic_document_analysis_relations() {
        let mut document = sample_document();
        let mut analysis = sample_document_analysis();
        analysis.episode_graph.relations.push(EpisodeRelation {
            from_group_ref: "group-2".to_string(),
            to_group_ref: "group-1".to_string(),
            kind: EpisodeRelationKind::Cascade,
            confidence: OrderedFloat(0.41),
            evidence_tags: vec!["reverse_edge".to_string()],
        });
        document.document_analysis = Some(analysis);

        let errors = document.validate().unwrap_err();

        assert!(
            errors
                .errors
                .iter()
                .any(|error| error.contains("episode_graph must be acyclic"))
        );
    }

    #[test]
    fn rejects_incoherent_document_analysis_materialization() {
        let mut document = sample_document();
        let mut analysis = sample_document_analysis();
        analysis.group_analysis[0].best_parent_group_ref = Some("group-2".to_string());
        analysis.group_analysis[0].visibility_floor = VisibilityFloor::HiddenAllowed;
        analysis.episode_graph.episodes[0].member_group_refs = vec!["group-2".to_string()];
        document.document_analysis = Some(analysis);

        let errors = document.validate().unwrap_err();

        assert!(
            errors.errors.iter().any(|error| {
                error.contains("role lead_root must not have best_parent_group_ref")
            })
        );
        assert!(errors.errors.iter().any(|error| {
            error.contains("role lead_root must use visibility_floor never_hidden")
        }));
        assert!(errors.errors.iter().any(|error| {
            error.contains("lead_group_ref group-1 must be included in member_group_refs")
        }));
        assert!(errors.errors.iter().any(|error| {
            error.contains("episode_ref episode-1 does not include the group in member_group_refs")
        }));
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
    fn rejects_empty_internal_ids() {
        let mut document = sample_document();
        document.captures[0].id.clear();
        document.diagnostics[0].id.clear();
        document.diagnostics[0].locations[0].id.clear();

        let errors = document.validate().unwrap_err();

        assert!(
            errors
                .errors
                .iter()
                .any(|error| error == "capture id must be non-empty")
        );
        assert!(
            errors
                .errors
                .iter()
                .any(|error| error == "node id must be non-empty")
        );
        assert!(
            errors
                .errors
                .iter()
                .any(|error| { error.contains("location id must be non-empty") })
        );
    }

    #[test]
    fn rejects_capture_storage_payload_mismatches() {
        let mut document = sample_document();
        document.captures[0].external_ref = Some("trace://stderr.raw".to_string());
        document.captures.push(CaptureArtifact {
            id: "sarif".to_string(),
            kind: ArtifactKind::GccSarif,
            media_type: "application/sarif+json".to_string(),
            encoding: Some("utf-8".to_string()),
            digest_sha256: None,
            size_bytes: None,
            storage: ArtifactStorage::ExternalRef,
            inline_text: Some("{\"version\":\"2.1.0\"}".to_string()),
            external_ref: Some("trace://diagnostics.sarif".to_string()),
            produced_by: None,
        });
        document.captures.push(CaptureArtifact {
            id: "missing-json".to_string(),
            kind: ArtifactKind::GccJson,
            media_type: "application/json".to_string(),
            encoding: Some("utf-8".to_string()),
            digest_sha256: None,
            size_bytes: None,
            storage: ArtifactStorage::Unavailable,
            inline_text: Some("{}".to_string()),
            external_ref: None,
            produced_by: None,
        });

        let errors = document.validate().unwrap_err();

        assert!(errors.errors.iter().any(|error| {
            error.contains("inline capture stderr.raw must not set external_ref")
        }));
        assert!(errors.errors.iter().any(|error| {
            error.contains("external_ref capture sarif must not set inline_text")
        }));
        assert!(errors.errors.iter().any(|error| {
            error.contains(
                "unavailable capture missing-json must not set inline_text or external_ref",
            )
        }));
    }

    #[test]
    fn rejects_blank_external_ref_payloads() {
        let mut document = sample_document();
        document.captures[0].storage = ArtifactStorage::ExternalRef;
        document.captures[0].inline_text = None;
        document.captures[0].external_ref = Some("   ".to_string());

        let errors = document.validate().unwrap_err();

        assert!(errors.errors.iter().any(|error| {
            error.contains("external_ref capture stderr.raw external_ref must be non-empty")
        }));
    }

    #[test]
    fn rejects_duplicate_location_ids_within_a_node() {
        let mut document = sample_document();
        let duplicate = document.diagnostics[0].locations[0].clone();
        document.diagnostics[0].locations.push(duplicate);

        let errors = document.validate().unwrap_err();

        assert!(
            errors.errors.iter().any(|error| {
                error.contains("has duplicate location id loc:src/main.c:4:1:4:1")
            })
        );
    }

    #[test]
    fn rejects_backwards_location_ranges() {
        let mut document = sample_document();
        document.diagnostics[0].locations[0] =
            Location::caret("src/main.c", 8, 12, LocationRole::Primary).with_range_end(
                8,
                4,
                BoundarySemantics::Unknown,
            );

        let errors = document.validate().unwrap_err();

        assert!(
            errors
                .errors
                .iter()
                .any(|error| { error.contains("range.start must not come after range.end") })
        );
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
    fn rejects_empty_location_path_and_blank_excerpt_ref() {
        let mut document = sample_document();
        document.diagnostics[0].locations[0].file.path_raw.clear();
        document.diagnostics[0].locations[0].source_excerpt_ref = Some("   ".to_string());

        let errors = document.validate().unwrap_err();

        assert!(
            errors
                .errors
                .iter()
                .any(|error| { error.contains("file.path_raw must be non-empty") })
        );
        assert!(
            errors.errors.iter().any(|error| {
                error.contains("source_excerpt_ref must be non-empty when present")
            })
        );
    }

    #[test]
    fn rejects_missing_and_non_snippet_excerpt_refs() {
        let mut document = sample_document();
        document.diagnostics[0].locations[0].source_excerpt_ref =
            Some("missing-snippet".to_string());
        document.captures.push(CaptureArtifact {
            id: "stderr.secondary".to_string(),
            kind: ArtifactKind::CompilerStderrText,
            media_type: "text/plain".to_string(),
            encoding: Some("utf-8".to_string()),
            digest_sha256: None,
            size_bytes: Some(8),
            storage: ArtifactStorage::Inline,
            inline_text: Some("note: context".to_string()),
            external_ref: None,
            produced_by: None,
        });
        document.diagnostics[0].locations.push(Location {
            id: "loc:src/snippet.c:9:2:9:2".to_string(),
            file: FileRef::new("src/snippet.c"),
            anchor: Some(SourcePoint::new(9, 2)),
            range: None,
            role: LocationRole::Secondary,
            source_kind: LocationSourceKind::Caret,
            label: None,
            ownership_override: None,
            provenance_override: None,
            source_excerpt_ref: Some("stderr.secondary".to_string()),
        });

        let errors = document.validate().unwrap_err();

        assert!(errors.errors.iter().any(|error| {
            error.contains("source_excerpt_ref references missing capture missing-snippet")
        }));
        assert!(errors.errors.iter().any(|error| {
            error.contains(
                "source_excerpt_ref stderr.secondary must reference a source_snippet capture",
            )
        }));
    }

    #[test]
    fn accepts_valid_source_snippet_excerpt_refs() {
        let mut document = sample_document();
        document.captures.push(CaptureArtifact {
            id: "snippet-1".to_string(),
            kind: ArtifactKind::SourceSnippet,
            media_type: "text/plain".to_string(),
            encoding: Some("utf-8".to_string()),
            digest_sha256: None,
            size_bytes: Some(20),
            storage: ArtifactStorage::Inline,
            inline_text: Some("int main(void) {}\n".to_string()),
            external_ref: None,
            produced_by: None,
        });
        document.diagnostics[0].locations[0].source_excerpt_ref = Some("snippet-1".to_string());

        assert!(document.validate().is_ok());
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
