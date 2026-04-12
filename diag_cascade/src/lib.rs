//! Document-wide cascade analysis entrypoints.
//!
//! This crate owns the analysis-stage boundary between node-local enrichment and
//! renderer consumption. The current work package adds deterministic logical
//! groups, safe relation scoring, and episode materialization without changing
//! renderer output yet.

mod analysis;
mod logical_group;
mod prefilter;

pub use analysis::SafeDocumentAnalyzer;
pub use logical_group::{
    AnchorSource, CanonicalAnchor, GroupKeySet, LogicalGroup, PRIMARY_LINE_BUCKET_WIDTH,
    canonical_group_ref, derive_canonical_anchor, derive_group_keys, extract_logical_groups,
};
pub use prefilter::{
    CandidatePair, CandidateReason, FAMILY_PHASE_ORDINAL_WINDOW, TRANSLATION_UNIT_ORDINAL_WINDOW,
    candidate_pairs,
};

use diag_backend_probe::{ProcessingPath, VersionBand};
use diag_core::{CascadePolicySnapshot, DiagnosticDocument, FallbackGrade, SourceAuthority};
use std::path::PathBuf;

/// Execution context for document-wide cascade analysis.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CascadeContext {
    /// Compiler version band resolved for the active backend.
    pub version_band: VersionBand,
    /// Processing path used for the current invocation.
    pub processing_path: ProcessingPath,
    /// Which source was authoritative for the ingested document.
    pub source_authority: SourceAuthority,
    /// Fallback grade already assigned during ingestion.
    pub fallback_grade: FallbackGrade,
    /// Working directory for the invocation being analyzed.
    pub cwd: PathBuf,
}

/// Summary of one cascade-analysis attempt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CascadeReport {
    /// Whether the analyzer materialized document-wide analysis into the IR.
    pub document_analysis_present: bool,
}

/// Errors produced by cascade analysis before fail-open handling at the caller.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum CascadeError {
    /// The requested analysis is not implemented for the current conditions.
    #[error("cascade analysis unsupported: {reason}")]
    Unsupported { reason: String },
    /// An internal error occurred while trying to analyze the document.
    #[error("cascade analysis failed: {reason}")]
    Internal { reason: String },
}

/// Trait implemented by document-wide cascade analyzers.
pub trait DocumentAnalyzer {
    /// Analyze the document and optionally materialize `document_analysis`.
    fn analyze_document(
        &self,
        document: &mut DiagnosticDocument,
        context: &CascadeContext,
        policy: &CascadePolicySnapshot,
    ) -> Result<CascadeReport, CascadeError>;
}

/// Conservative no-op analyzer used to install the pipeline seam without
/// changing current renderer behavior.
#[derive(Debug, Clone, Copy, Default)]
pub struct NoopDocumentAnalyzer;

impl DocumentAnalyzer for NoopDocumentAnalyzer {
    fn analyze_document(
        &self,
        document: &mut DiagnosticDocument,
        _context: &CascadeContext,
        _policy: &CascadePolicySnapshot,
    ) -> Result<CascadeReport, CascadeError> {
        document.document_analysis = None;
        Ok(CascadeReport {
            document_analysis_present: false,
        })
    }
}

/// Analyze the document using the default safe analyzer.
pub fn analyze_document(
    document: &mut DiagnosticDocument,
    context: &CascadeContext,
    policy: &CascadePolicySnapshot,
) -> Result<CascadeReport, CascadeError> {
    SafeDocumentAnalyzer.analyze_document(document, context, policy)
}

#[cfg(test)]
mod tests {
    use super::*;
    use diag_core::{
        AnalysisOverlay, ArtifactKind, ArtifactStorage, CaptureArtifact, ContextChain,
        ContextChainKind, ContextFrame, DiagnosticDocument, DiagnosticNode, DocumentCompleteness,
        LanguageMode, Location, LocationRole, MessageText, NodeCompleteness, Origin, Ownership,
        Phase, ProducerInfo, Provenance, ProvenanceSource, RunInfo, SemanticRole, Severity,
        SymbolContext, ToolInfo, WrapperSurface,
    };

    fn sample_document(diagnostics: Vec<DiagnosticNode>) -> DiagnosticDocument {
        DiagnosticDocument {
            document_id: "cascade-doc".to_string(),
            schema_version: diag_core::IR_SPEC_VERSION.to_string(),
            document_completeness: DocumentCompleteness::Partial,
            producer: ProducerInfo {
                name: "gcc-formed".to_string(),
                version: "0.2.0-beta.1".to_string(),
                git_revision: None,
                build_profile: Some("test".to_string()),
                rulepack_version: Some("phase1".to_string()),
            },
            run: RunInfo {
                invocation_id: "invocation".to_string(),
                invoked_as: Some("gcc-formed".to_string()),
                argv_redacted: vec!["gcc".to_string(), "-c".to_string(), "main.c".to_string()],
                cwd_display: Some("/tmp/project".to_string()),
                exit_status: 1,
                primary_tool: ToolInfo {
                    name: "gcc".to_string(),
                    version: Some("15.2.0".to_string()),
                    component: None,
                    vendor: Some("GNU".to_string()),
                },
                secondary_tools: Vec::new(),
                language_mode: Some(LanguageMode::C),
                target_triple: None,
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
            diagnostics,
            document_analysis: None,
            fingerprints: None,
        }
    }

    fn sample_context() -> CascadeContext {
        CascadeContext {
            version_band: VersionBand::Gcc15Plus,
            processing_path: ProcessingPath::DualSinkStructured,
            source_authority: SourceAuthority::Structured,
            fallback_grade: FallbackGrade::None,
            cwd: PathBuf::from("/tmp/project"),
        }
    }

    fn conservative_context() -> CascadeContext {
        CascadeContext {
            version_band: VersionBand::Gcc9_12,
            processing_path: ProcessingPath::NativeTextCapture,
            source_authority: SourceAuthority::ResidualText,
            fallback_grade: FallbackGrade::FailOpen,
            cwd: PathBuf::from("/tmp/project"),
        }
    }

    fn sample_analysis(family: &str) -> AnalysisOverlay {
        AnalysisOverlay {
            family: Some(family.to_string().into()),
            family_version: None,
            family_confidence: None,
            root_cause_score: None,
            actionability_score: None,
            user_code_priority: None,
            headline: Some("headline".into()),
            first_action_hint: Some("hint".into()),
            confidence: Some(diag_core::Confidence::High.score()),
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
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn sample_node(
        id: &str,
        raw_text: &str,
        origin: Origin,
        phase: Phase,
        family: &str,
        path: Option<&str>,
        line: Option<u32>,
        ownership: Ownership,
    ) -> DiagnosticNode {
        DiagnosticNode {
            id: id.to_string(),
            origin,
            phase,
            severity: Severity::Error,
            semantic_role: SemanticRole::Root,
            message: MessageText {
                raw_text: raw_text.to_string(),
                normalized_text: None,
                locale: None,
            },
            locations: path
                .zip(line)
                .map(|(path, line)| {
                    vec![
                        Location::caret(path, line, 1, LocationRole::Primary)
                            .with_ownership(ownership, "test_owner"),
                    ]
                })
                .unwrap_or_default(),
            children: Vec::new(),
            suggestions: Vec::new(),
            context_chains: Vec::new(),
            symbol_context: None,
            node_completeness: NodeCompleteness::Complete,
            provenance: Provenance {
                source: ProvenanceSource::Compiler,
                capture_refs: vec!["stderr.raw".to_string()],
            },
            analysis: Some(sample_analysis(family)),
            fingerprints: None,
        }
    }

    fn template_chain(path: &str, line: u32) -> ContextChain {
        ContextChain {
            kind: ContextChainKind::TemplateInstantiation,
            frames: vec![ContextFrame {
                label: "instantiated from here".to_string(),
                path: Some(path.to_string()),
                line: Some(line),
                column: Some(7),
            }],
        }
    }

    fn run_safe_analysis(
        diagnostics: Vec<DiagnosticNode>,
        context: &CascadeContext,
    ) -> diag_core::DocumentAnalysis {
        run_safe_analysis_with_policy(diagnostics, context, &CascadePolicySnapshot::default())
    }

    fn run_safe_analysis_with_policy(
        diagnostics: Vec<DiagnosticNode>,
        context: &CascadeContext,
        policy: &CascadePolicySnapshot,
    ) -> diag_core::DocumentAnalysis {
        let mut document = sample_document(diagnostics);
        let report = analyze_document(&mut document, context, policy).unwrap();
        assert!(report.document_analysis_present);
        document.validate().unwrap();
        document.document_analysis.unwrap()
    }

    #[test]
    fn noop_analyzer_clears_document_analysis_without_failing() {
        let mut document = sample_document(Vec::new());
        document.document_analysis = Some(diag_core::DocumentAnalysis::default());
        let report = NoopDocumentAnalyzer
            .analyze_document(
                &mut document,
                &sample_context(),
                &CascadePolicySnapshot::default(),
            )
            .unwrap();

        assert!(!report.document_analysis_present);
        assert!(document.document_analysis.is_none());
    }

    #[test]
    fn logical_group_extraction_is_deterministic() {
        let mut same_file_left = sample_node(
            "node-a",
            "expected ';' before '}' token",
            Origin::Gcc,
            Phase::Parse,
            "syntax",
            Some("src/main.c"),
            Some(5),
            Ownership::User,
        );
        same_file_left.analysis.as_mut().unwrap().group_ref = Some("shared-hint".to_string());
        let mut same_file_right = sample_node(
            "node-b",
            "undeclared identifier 'value'",
            Origin::Gcc,
            Phase::Semantic,
            "scope_declaration",
            Some("src/main.c"),
            Some(17),
            Ownership::User,
        );
        same_file_right.analysis.as_mut().unwrap().group_ref = Some("shared-hint".to_string());
        let mut linker = sample_node(
            "node-c",
            "helper.c:(.text+0x0): undefined reference to `missing_symbol`",
            Origin::Linker,
            Phase::Link,
            "linker.undefined_reference",
            Some("helper.c"),
            Some(1),
            Ownership::User,
        );
        linker.symbol_context = Some(SymbolContext {
            primary_symbol: Some("missing_symbol".to_string()),
            related_objects: vec!["helper.o".to_string()],
            archive: None,
        });

        let document = sample_document(vec![same_file_left, same_file_right, linker]);
        let first = extract_logical_groups(&document);
        let second = extract_logical_groups(&document);

        assert_eq!(first, second);
    }

    #[test]
    fn same_file_roots_stay_in_separate_groups_even_with_shared_hint() {
        let mut left = sample_node(
            "node-a",
            "expected ';' before '}' token",
            Origin::Gcc,
            Phase::Parse,
            "syntax",
            Some("src/main.c"),
            Some(5),
            Ownership::User,
        );
        left.analysis.as_mut().unwrap().group_ref = Some("hinted-cluster".to_string());
        let mut right = sample_node(
            "node-b",
            "another independent error",
            Origin::Gcc,
            Phase::Parse,
            "syntax",
            Some("src/main.c"),
            Some(40),
            Ownership::User,
        );
        right.analysis.as_mut().unwrap().group_ref = Some("hinted-cluster".to_string());

        let groups = extract_logical_groups(&sample_document(vec![left, right]));

        assert_eq!(groups.len(), 2);
        assert_eq!(
            groups
                .iter()
                .map(|group| group.hint_group_ref.as_deref())
                .collect::<Vec<_>>(),
            vec![Some("hinted-cluster"), Some("hinted-cluster")]
        );
        assert_ne!(groups[0].group_ref, groups[1].group_ref);
        assert_ne!(
            groups[0].keys.primary_line_bucket,
            groups[1].keys.primary_line_bucket
        );
    }

    #[test]
    fn multi_file_groups_keep_distinct_primary_and_translation_unit_keys() {
        let main = sample_node(
            "node-main",
            "main failure",
            Origin::Gcc,
            Phase::Semantic,
            "scope_declaration",
            Some("src/main.c"),
            Some(10),
            Ownership::User,
        );
        let helper = sample_node(
            "node-helper",
            "helper failure",
            Origin::Gcc,
            Phase::Semantic,
            "scope_declaration",
            Some("src/helper.c"),
            Some(10),
            Ownership::User,
        );

        let groups = extract_logical_groups(&sample_document(vec![main, helper]));

        assert_eq!(groups.len(), 2);
        assert_ne!(
            groups[0].keys.primary_file_key,
            groups[1].keys.primary_file_key
        );
        assert_ne!(
            groups[0].keys.translation_unit_key,
            groups[1].keys.translation_unit_key
        );
    }

    #[test]
    fn linker_group_derives_symbol_keys_for_structured_and_message_only_nodes() {
        let mut structured = sample_node(
            "node-structured",
            "undefined reference to `missing_symbol`",
            Origin::Linker,
            Phase::Link,
            "linker.undefined_reference",
            Some("src/main.c"),
            Some(8),
            Ownership::User,
        );
        structured.symbol_context = Some(SymbolContext {
            primary_symbol: Some("missing_symbol".to_string()),
            related_objects: vec!["src/main.o".to_string()],
            archive: None,
        });
        let message_only = sample_node(
            "node-message",
            "helper.c:(.text+0x0): undefined reference to `missing_symbol`",
            Origin::Linker,
            Phase::Link,
            "linker.undefined_reference",
            Some("helper.c"),
            Some(1),
            Ownership::User,
        );

        let groups = extract_logical_groups(&sample_document(vec![structured, message_only]));

        assert_eq!(groups[0].keys.symbol_key.as_deref(), Some("missing_symbol"));
        assert_eq!(groups[1].keys.symbol_key.as_deref(), Some("missing_symbol"));
    }

    #[test]
    fn structured_and_native_warning_paths_produce_matching_canonical_keys() {
        let mut structured = sample_node(
            "node-structured",
            "control reaches end of non-void function",
            Origin::Gcc,
            Phase::Semantic,
            "return_type",
            Some("src/main.c"),
            Some(2),
            Ownership::User,
        );
        structured.provenance.source = ProvenanceSource::Compiler;

        let mut native = sample_node(
            "node-native",
            "src/main.c:2:1: warning: control reaches end of non-void function [-Wreturn-type]",
            Origin::Gcc,
            Phase::Semantic,
            "return_type",
            Some("src/main.c"),
            Some(2),
            Ownership::User,
        );
        native.provenance.source = ProvenanceSource::ResidualText;

        let structured_group = extract_logical_groups(&sample_document(vec![structured]))
            .into_iter()
            .next()
            .unwrap();
        let native_group = extract_logical_groups(&sample_document(vec![native]))
            .into_iter()
            .next()
            .unwrap();

        assert_eq!(
            structured_group.keys.primary_file_key,
            native_group.keys.primary_file_key
        );
        assert_eq!(
            structured_group.keys.primary_line_bucket,
            native_group.keys.primary_line_bucket
        );
        assert_eq!(
            structured_group.keys.translation_unit_key,
            native_group.keys.translation_unit_key
        );
        assert_eq!(
            structured_group.keys.origin_phase_key,
            native_group.keys.origin_phase_key
        );
        assert_eq!(
            structured_group.keys.family_key,
            native_group.keys.family_key
        );
        assert_eq!(
            structured_group.keys.normalized_message_key,
            native_group.keys.normalized_message_key
        );
        assert_eq!(structured_group.group_ref, native_group.group_ref);
    }

    #[test]
    fn structured_and_native_linker_paths_produce_matching_symbol_and_message_keys() {
        let mut structured = sample_node(
            "node-structured",
            "undefined reference to `foo`",
            Origin::Linker,
            Phase::Link,
            "linker.undefined_reference",
            Some("src/main.c"),
            Some(8),
            Ownership::User,
        );
        structured.symbol_context = Some(SymbolContext {
            primary_symbol: Some("foo".to_string()),
            related_objects: vec!["src/main.o".to_string()],
            archive: None,
        });

        let native = sample_node(
            "node-native",
            "src/main.c:(.text+0x15): undefined reference to `foo`",
            Origin::Linker,
            Phase::Link,
            "linker.undefined_reference",
            Some("src/main.c"),
            Some(8),
            Ownership::User,
        );

        let structured_group = extract_logical_groups(&sample_document(vec![structured]))
            .into_iter()
            .next()
            .unwrap();
        let native_group = extract_logical_groups(&sample_document(vec![native]))
            .into_iter()
            .next()
            .unwrap();

        assert_eq!(
            structured_group.keys.symbol_key,
            native_group.keys.symbol_key
        );
        assert_eq!(
            structured_group.keys.normalized_message_key,
            native_group.keys.normalized_message_key
        );
    }

    #[test]
    fn canonical_anchor_falls_back_to_frontier_when_primary_location_is_missing() {
        let mut node = sample_node(
            "node-template",
            "template argument deduction/substitution failed",
            Origin::Gcc,
            Phase::Instantiate,
            "template",
            None,
            None,
            Ownership::Unknown,
        );
        node.context_chains = vec![template_chain("src/main.cpp", 12)];

        let group = extract_logical_groups(&sample_document(vec![node]))
            .into_iter()
            .next()
            .unwrap();

        assert_eq!(
            group.canonical_anchor.source,
            AnchorSource::TemplateFrontier
        );
        assert_eq!(
            group.canonical_anchor.path_key.as_deref(),
            Some("src/main.cpp")
        );
        assert_eq!(
            group.keys.template_frontier_key.as_deref(),
            Some("src/main.cpp:12")
        );
    }

    #[test]
    fn candidate_prefilter_limits_same_file_comparisons_by_bucket() {
        let first = sample_node(
            "node-1",
            "first failure",
            Origin::Gcc,
            Phase::Semantic,
            "scope_declaration",
            Some("src/main.c"),
            Some(4),
            Ownership::User,
        );
        let second = sample_node(
            "node-2",
            "second failure",
            Origin::Gcc,
            Phase::Semantic,
            "scope_declaration",
            Some("src/main.c"),
            Some(11),
            Ownership::User,
        );
        let far = sample_node(
            "node-3",
            "far failure",
            Origin::Gcc,
            Phase::Semantic,
            "scope_declaration",
            Some("src/main.c"),
            Some(96),
            Ownership::User,
        );
        let other_file = sample_node(
            "node-4",
            "other file failure",
            Origin::Gcc,
            Phase::Semantic,
            "scope_declaration",
            Some("src/helper.c"),
            Some(4),
            Ownership::User,
        );

        let groups = extract_logical_groups(&sample_document(vec![first, second, far, other_file]));
        let pairs = candidate_pairs(&groups);

        assert!(pairs.iter().any(|pair| {
            pair.left_group_ref == groups[0].group_ref
                && pair.right_group_ref == groups[1].group_ref
                && pair.reasons.contains(&CandidateReason::NearbyFileBucket)
        }));
        assert!(!pairs.iter().any(|pair| {
            (pair.left_group_ref == groups[0].group_ref
                && pair.right_group_ref == groups[2].group_ref)
                || (pair.left_group_ref == groups[2].group_ref
                    && pair.right_group_ref == groups[0].group_ref)
        }));
        assert!(pairs.len() < 6);
    }

    #[test]
    fn candidate_prefilter_uses_symbol_and_family_phase_fallback_keys() {
        let mut same_symbol_left = sample_node(
            "node-link-left",
            "undefined reference to `missing_symbol`",
            Origin::Linker,
            Phase::Link,
            "linker.undefined_reference",
            Some("src/main.c"),
            Some(5),
            Ownership::User,
        );
        same_symbol_left.symbol_context = Some(SymbolContext {
            primary_symbol: Some("missing_symbol".to_string()),
            related_objects: vec!["src/main.o".to_string()],
            archive: None,
        });
        let same_symbol_right = sample_node(
            "node-link-right",
            "helper.c:(.text+0x0): undefined reference to `missing_symbol`",
            Origin::Linker,
            Phase::Link,
            "linker.undefined_reference",
            Some("src/helper.c"),
            Some(9),
            Ownership::User,
        );
        let sparse_left = sample_node(
            "node-sparse-left",
            "sparse failure one",
            Origin::Gcc,
            Phase::Semantic,
            "scope_declaration",
            None,
            None,
            Ownership::Unknown,
        );
        let sparse_right = sample_node(
            "node-sparse-right",
            "sparse failure two",
            Origin::Gcc,
            Phase::Semantic,
            "scope_declaration",
            None,
            None,
            Ownership::Unknown,
        );

        let groups = extract_logical_groups(&sample_document(vec![
            same_symbol_left,
            same_symbol_right,
            sparse_left,
            sparse_right,
        ]));
        let pairs = candidate_pairs(&groups);

        assert!(pairs.iter().any(|pair| {
            pair.left_group_ref == groups[0].group_ref
                && pair.right_group_ref == groups[1].group_ref
                && pair.reasons.contains(&CandidateReason::SharedSymbol)
        }));
        assert!(pairs.iter().any(|pair| {
            pair.left_group_ref == groups[2].group_ref
                && pair.right_group_ref == groups[3].group_ref
                && pair.reasons.contains(&CandidateReason::FamilyPhaseWindow)
        }));
    }

    #[test]
    fn safe_analyzer_materializes_roles_visibility_and_episodes() {
        let mut lead = sample_node(
            "node-lead",
            "undefined reference to `foo`",
            Origin::Linker,
            Phase::Link,
            "linker.undefined_reference",
            Some("src/main.c"),
            Some(8),
            Ownership::User,
        );
        lead.symbol_context = Some(SymbolContext {
            primary_symbol: Some("foo".to_string()),
            related_objects: vec!["src/main.o".to_string()],
            archive: None,
        });
        let duplicate = sample_node(
            "node-dup",
            "src/main.c:(.text+0x15): undefined reference to `foo`",
            Origin::Linker,
            Phase::Link,
            "linker.undefined_reference",
            Some("src/main.c"),
            Some(9),
            Ownership::User,
        );

        let analysis = run_safe_analysis(vec![lead, duplicate], &sample_context());

        assert_eq!(analysis.episode_graph.episodes.len(), 1);
        assert_eq!(analysis.group_analysis.len(), 2);
        assert_eq!(
            analysis.group_analysis[0].role,
            diag_core::GroupCascadeRole::LeadRoot
        );
        assert_eq!(
            analysis.group_analysis[0].visibility_floor,
            diag_core::VisibilityFloor::NeverHidden
        );
        assert_eq!(
            analysis.group_analysis[1].role,
            diag_core::GroupCascadeRole::Duplicate
        );
        assert_eq!(
            analysis.group_analysis[1].visibility_floor,
            diag_core::VisibilityFloor::HiddenAllowed
        );
        assert_eq!(
            analysis.group_analysis[1].best_parent_group_ref.as_deref(),
            Some(analysis.group_analysis[0].group_ref.as_str())
        );
        assert_eq!(analysis.stats.independent_root_count, 1);
        assert_eq!(analysis.stats.duplicate_count, 1);
    }

    #[test]
    fn collect2_summary_stays_under_the_specific_linker_root() {
        let mut lead = sample_node(
            "node-link-root",
            "undefined reference to `missing_symbol`",
            Origin::Linker,
            Phase::Link,
            "linker.undefined_reference",
            Some("src/main.c"),
            Some(8),
            Ownership::User,
        );
        lead.symbol_context = Some(SymbolContext {
            primary_symbol: Some("missing_symbol".to_string()),
            related_objects: vec!["src/main.o".to_string()],
            archive: None,
        });
        let mut summary = sample_node(
            "node-collect2",
            "collect2: error: ld returned 1 exit status",
            Origin::Driver,
            Phase::Link,
            "collect2_summary",
            None,
            None,
            Ownership::Unknown,
        );
        summary.node_completeness = NodeCompleteness::Partial;

        let analysis = run_safe_analysis(vec![lead, summary], &sample_context());

        assert_eq!(analysis.episode_graph.episodes.len(), 1);
        assert_eq!(
            analysis.group_analysis[0].role,
            diag_core::GroupCascadeRole::LeadRoot
        );
        assert_eq!(
            analysis.group_analysis[1].role,
            diag_core::GroupCascadeRole::FollowOn
        );
        assert_eq!(
            analysis.group_analysis[1].best_parent_group_ref.as_deref(),
            Some(analysis.group_analysis[0].group_ref.as_str())
        );
        assert!(
            analysis.group_analysis[0].root_score.unwrap()
                > analysis.group_analysis[1].root_score.unwrap()
        );
    }

    #[test]
    fn ambiguous_best_parent_margin_keeps_child_uncertain() {
        let mut first = sample_node(
            "node-first",
            "undefined reference to `foo`",
            Origin::Linker,
            Phase::Link,
            "linker.undefined_reference",
            Some("src/main.c"),
            Some(8),
            Ownership::User,
        );
        first.symbol_context = Some(SymbolContext {
            primary_symbol: Some("foo".to_string()),
            related_objects: vec!["src/main.o".to_string()],
            archive: None,
        });
        let second = sample_node(
            "node-second",
            "src/main.c:(.text+0x15): undefined reference to `foo`",
            Origin::Linker,
            Phase::Link,
            "linker.undefined_reference",
            Some("src/main.c"),
            Some(9),
            Ownership::User,
        );
        let mut third = sample_node(
            "node-third",
            "src/main.c:(.text+0x20): undefined reference to `foo`",
            Origin::Linker,
            Phase::Link,
            "linker.undefined_reference",
            Some("src/main.c"),
            Some(10),
            Ownership::User,
        );
        third.severity = Severity::Note;

        let analysis = run_safe_analysis(vec![first, second, third], &sample_context());

        assert_eq!(analysis.episode_graph.episodes.len(), 2);
        assert_eq!(
            analysis.group_analysis[1].role,
            diag_core::GroupCascadeRole::Duplicate
        );
        assert_eq!(
            analysis.group_analysis[2].role,
            diag_core::GroupCascadeRole::Uncertain
        );
        assert_eq!(
            analysis.group_analysis[2].visibility_floor,
            diag_core::VisibilityFloor::NeverHidden
        );
        assert!(analysis.group_analysis[2].best_parent_group_ref.is_some());
        assert_eq!(analysis.episode_graph.relations.len(), 1);
    }

    #[test]
    fn policy_min_parent_margin_changes_parent_acceptance_deterministically() {
        let mut first = sample_node(
            "node-first",
            "undefined reference to `foo`",
            Origin::Linker,
            Phase::Link,
            "linker.undefined_reference",
            Some("src/main.c"),
            Some(8),
            Ownership::User,
        );
        first.symbol_context = Some(SymbolContext {
            primary_symbol: Some("foo".to_string()),
            related_objects: vec!["src/main.o".to_string()],
            archive: None,
        });
        let second = sample_node(
            "node-second",
            "src/main.c:(.text+0x15): undefined reference to `foo`",
            Origin::Linker,
            Phase::Link,
            "linker.undefined_reference",
            Some("src/main.c"),
            Some(9),
            Ownership::User,
        );
        let mut third = sample_node(
            "node-third",
            "src/main.c:(.text+0x20): undefined reference to `foo`",
            Origin::Linker,
            Phase::Link,
            "linker.undefined_reference",
            Some("src/main.c"),
            Some(10),
            Ownership::User,
        );
        third.severity = Severity::Note;

        let default_analysis = run_safe_analysis(
            vec![first.clone(), second.clone(), third.clone()],
            &sample_context(),
        );
        assert_eq!(
            default_analysis.group_analysis[2].role,
            diag_core::GroupCascadeRole::Uncertain
        );

        let relaxed_policy = CascadePolicySnapshot {
            min_parent_margin: 0.0,
            ..CascadePolicySnapshot::default()
        };
        let relaxed_analysis = run_safe_analysis_with_policy(
            vec![first, second, third],
            &sample_context(),
            &relaxed_policy,
        );

        assert_ne!(
            relaxed_analysis.group_analysis[2].role,
            diag_core::GroupCascadeRole::Uncertain
        );
        assert!(
            relaxed_analysis.group_analysis[2]
                .best_parent_group_ref
                .is_some()
        );
    }

    #[test]
    fn syntax_desync_tail_becomes_follow_on_instead_of_another_root() {
        let root = sample_node(
            "node-parse-root",
            "expected ';' before '}' token",
            Origin::Gcc,
            Phase::Parse,
            "syntax",
            Some("src/main.c"),
            Some(11),
            Ownership::User,
        );
        let mut tail = sample_node(
            "node-parse-tail",
            "expected declaration or statement at end of input",
            Origin::Gcc,
            Phase::Parse,
            "syntax",
            Some("src/main.c"),
            Some(12),
            Ownership::User,
        );
        tail.node_completeness = NodeCompleteness::Partial;

        let analysis = run_safe_analysis(vec![root, tail], &sample_context());

        assert_eq!(analysis.episode_graph.episodes.len(), 1);
        assert_eq!(
            analysis.group_analysis[0].role,
            diag_core::GroupCascadeRole::LeadRoot
        );
        assert_eq!(
            analysis.group_analysis[1].role,
            diag_core::GroupCascadeRole::FollowOn
        );
        assert!(matches!(
            analysis.group_analysis[1].visibility_floor,
            diag_core::VisibilityFloor::HiddenAllowed
                | diag_core::VisibilityFloor::SummaryOrExpandedOnly
        ));
    }

    #[test]
    fn weak_evidence_does_not_open_hidden_suppression() {
        let mut root = sample_node(
            "node-root",
            "template argument deduction/substitution failed",
            Origin::Gcc,
            Phase::Instantiate,
            "template",
            Some("src/main.cpp"),
            Some(12),
            Ownership::User,
        );
        root.context_chains = vec![template_chain("src/main.cpp", 12)];

        let mut follow_on = sample_node(
            "node-follow-on",
            "required from here",
            Origin::Gcc,
            Phase::Instantiate,
            "template",
            None,
            None,
            Ownership::Unknown,
        );
        follow_on.severity = Severity::Note;
        follow_on.node_completeness = NodeCompleteness::Partial;
        follow_on.context_chains = vec![template_chain("src/main.cpp", 12)];

        let analysis = run_safe_analysis(vec![root, follow_on], &conservative_context());

        assert_eq!(
            analysis.group_analysis[1].role,
            diag_core::GroupCascadeRole::FollowOn
        );
        assert_eq!(
            analysis.group_analysis[1].visibility_floor,
            diag_core::VisibilityFloor::SummaryOrExpandedOnly
        );
        assert!(
            analysis.group_analysis[1]
                .suppress_likelihood
                .unwrap()
                .into_inner()
                < CascadePolicySnapshot::default().suppress_likelihood_threshold
        );
    }

    #[test]
    fn template_candidate_repeat_is_compressed_as_dependent_detail() {
        let mut root = sample_node(
            "node-template-root",
            "template argument deduction/substitution failed",
            Origin::Gcc,
            Phase::Instantiate,
            "template",
            Some("src/main.cpp"),
            Some(20),
            Ownership::User,
        );
        root.context_chains = vec![template_chain("src/main.cpp", 20)];

        let mut candidate = sample_node(
            "node-template-candidate",
            "candidate 1: 'template<class T> Pair(T, T) -> Pair<T>'",
            Origin::Gcc,
            Phase::Instantiate,
            "template",
            Some("src/main.cpp"),
            Some(20),
            Ownership::User,
        );
        candidate.semantic_role = SemanticRole::Summary;
        candidate.severity = Severity::Note;
        candidate.context_chains = vec![template_chain("src/main.cpp", 20)];

        let analysis = run_safe_analysis(vec![root, candidate], &sample_context());

        assert_eq!(analysis.episode_graph.episodes.len(), 1);
        assert!(matches!(
            analysis.group_analysis[1].role,
            diag_core::GroupCascadeRole::Duplicate | diag_core::GroupCascadeRole::FollowOn
        ));
        assert!(matches!(
            analysis.group_analysis[1].visibility_floor,
            diag_core::VisibilityFloor::HiddenAllowed
                | diag_core::VisibilityFloor::SummaryOrExpandedOnly
        ));
    }

    #[test]
    fn band_and_path_only_reduce_hidden_aggressiveness_for_the_same_template_follow_on() {
        let mut root = sample_node(
            "node-template-root",
            "template argument deduction/substitution failed",
            Origin::Gcc,
            Phase::Instantiate,
            "template",
            Some("src/main.cpp"),
            Some(12),
            Ownership::User,
        );
        root.context_chains = vec![template_chain("src/main.cpp", 12)];

        let mut follow_on = sample_node(
            "node-template-follow-on",
            "required from here",
            Origin::Gcc,
            Phase::Instantiate,
            "template",
            None,
            None,
            Ownership::Unknown,
        );
        follow_on.severity = Severity::Note;
        follow_on.node_completeness = NodeCompleteness::Partial;
        follow_on.context_chains = vec![template_chain("src/main.cpp", 12)];

        let default_analysis =
            run_safe_analysis(vec![root.clone(), follow_on.clone()], &sample_context());
        let conservative_analysis =
            run_safe_analysis(vec![root, follow_on], &conservative_context());

        assert_eq!(
            default_analysis.group_analysis[1].role,
            diag_core::GroupCascadeRole::FollowOn
        );
        assert_eq!(
            conservative_analysis.group_analysis[1].role,
            diag_core::GroupCascadeRole::FollowOn
        );
        assert_eq!(
            default_analysis.group_analysis[1].visibility_floor,
            diag_core::VisibilityFloor::HiddenAllowed
        );
        assert_eq!(
            conservative_analysis.group_analysis[1].visibility_floor,
            diag_core::VisibilityFloor::SummaryOrExpandedOnly
        );
    }

    #[test]
    fn same_file_independent_roots_remain_separate_episodes_after_analysis() {
        let left = sample_node(
            "node-left",
            "expected ';' before '}' token",
            Origin::Gcc,
            Phase::Parse,
            "syntax",
            Some("src/main.c"),
            Some(5),
            Ownership::User,
        );
        let right = sample_node(
            "node-right",
            "expected ')' before ';' token",
            Origin::Gcc,
            Phase::Parse,
            "syntax",
            Some("src/main.c"),
            Some(40),
            Ownership::User,
        );

        let analysis = run_safe_analysis(vec![left, right], &sample_context());

        assert_eq!(analysis.episode_graph.episodes.len(), 2);
        assert_eq!(
            analysis
                .group_analysis
                .iter()
                .map(|group| group.role)
                .collect::<Vec<_>>(),
            vec![
                diag_core::GroupCascadeRole::IndependentRoot,
                diag_core::GroupCascadeRole::IndependentRoot,
            ]
        );
        assert!(
            analysis
                .group_analysis
                .iter()
                .all(|group| group.best_parent_group_ref.is_none())
        );
    }
}
