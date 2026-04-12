//! Document-wide cascade analysis entrypoints.
//!
//! This crate owns the analysis-stage boundary between node-local enrichment and
//! renderer consumption. The current work package adds deterministic logical
//! groups, canonical anchor/key derivation, and candidate prefiltering without
//! changing renderer output yet.

mod logical_group;
mod prefilter;

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

/// Analyze the document using the default no-op analyzer.
pub fn analyze_document(
    document: &mut DiagnosticDocument,
    context: &CascadeContext,
    policy: &CascadePolicySnapshot,
) -> Result<CascadeReport, CascadeError> {
    NoopDocumentAnalyzer.analyze_document(document, context, policy)
}

#[cfg(test)]
mod tests {
    use super::*;
    use diag_core::{
        AnalysisOverlay, ContextChain, ContextChainKind, ContextFrame, DiagnosticDocument,
        DiagnosticNode, DocumentCompleteness, LanguageMode, Location, LocationRole, MessageText,
        NodeCompleteness, Origin, Ownership, Phase, ProducerInfo, Provenance, ProvenanceSource,
        RunInfo, SemanticRole, Severity, SymbolContext, ToolInfo, WrapperSurface,
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
            captures: Vec::new(),
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

    #[test]
    fn noop_analyzer_clears_document_analysis_without_failing() {
        let mut document = sample_document(Vec::new());
        document.document_analysis = Some(diag_core::DocumentAnalysis::default());
        let report = analyze_document(
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
}
