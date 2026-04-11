mod budget;
mod excerpt;
mod fallback;
mod family;
mod formatter;
mod layout;
mod selector;
mod theme;
mod view_model;

use diag_core::{DiagnosticDocument, DocumentCompleteness, FallbackReason, IntegrityIssue};
use serde::{Deserialize, Serialize};

pub use excerpt::ExcerptBlock;
pub use selector::select_groups;
pub use view_model::{RenderGroupCard, RenderSessionSummary, RenderViewModel, SummaryOnlyGroup};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RenderProfile {
    Default,
    Concise,
    Verbose,
    Debug,
    Ci,
    RawFallback,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StreamKind {
    Tty,
    Pipe,
    File,
    CiLog,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PathPolicy {
    ShortestUnambiguous,
    RelativeToCwd,
    Absolute,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WarningVisibility {
    Auto,
    ShowAll,
    SuppressAll,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DebugRefs {
    None,
    TraceId,
    CaptureRef,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TypeDisplayPolicy {
    Full,
    CompactSafe,
    RawFirst,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceExcerptPolicy {
    Auto,
    ForceOn,
    ForceOff,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RenderRequest {
    pub document: DiagnosticDocument,
    pub profile: RenderProfile,
    pub capabilities: RenderCapabilities,
    pub cwd: Option<std::path::PathBuf>,
    pub path_policy: PathPolicy,
    pub warning_visibility: WarningVisibility,
    pub debug_refs: DebugRefs,
    pub type_display_policy: TypeDisplayPolicy,
    pub source_excerpt_policy: SourceExcerptPolicy,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RenderCapabilities {
    pub stream_kind: StreamKind,
    pub width_columns: Option<usize>,
    pub ansi_color: bool,
    pub unicode: bool,
    pub hyperlinks: bool,
    pub interactive: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RenderResult {
    pub text: String,
    pub used_analysis: bool,
    pub used_fallback: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fallback_reason: Option<FallbackReason>,
    pub displayed_group_refs: Vec<String>,
    pub suppressed_group_count: usize,
    pub suppressed_warning_count: usize,
    pub truncation_occurred: bool,
    pub render_issues: Vec<IntegrityIssue>,
}

#[derive(Debug, thiserror::Error)]
pub enum RenderError {
    #[error("render failed")]
    Failed,
}

pub fn render(request: RenderRequest) -> Result<RenderResult, RenderError> {
    if matches!(request.profile, RenderProfile::RawFallback) {
        return Ok(fallback::render_fallback(
            &request,
            FallbackReason::UserOptOut,
        ));
    }
    if matches!(
        request.document.document_completeness,
        DocumentCompleteness::Passthrough
    ) {
        return Ok(fallback::render_fallback(
            &request,
            FallbackReason::ResidualOnly,
        ));
    }
    if matches!(
        request.document.document_completeness,
        DocumentCompleteness::Failed
    ) {
        return Ok(fallback::render_fallback(
            &request,
            FallbackReason::InternalError,
        ));
    }

    let selected = selector::select_groups(&request);
    if selected.cards.is_empty() {
        return Ok(fallback::render_fallback(
            &request,
            FallbackReason::RendererLowConfidence,
        ));
    }
    let view_model = view_model::build(&request, selected.cards, selected.summary_only_cards);
    Ok(formatter::emit(
        &request,
        view_model,
        selected.suppressed_warning_count,
    ))
}

pub fn build_view_model(request: &RenderRequest) -> Option<RenderViewModel> {
    if matches!(request.profile, RenderProfile::RawFallback)
        || matches!(
            request.document.document_completeness,
            DocumentCompleteness::Passthrough | DocumentCompleteness::Failed
        )
    {
        return None;
    }
    let selected = selector::select_groups(request);
    if selected.cards.is_empty() {
        None
    } else {
        Some(view_model::build(
            request,
            selected.cards,
            selected.summary_only_cards,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::family::summarize_supporting_evidence;
    use crate::selector::select_groups;
    use diag_core::{
        AnalysisOverlay, CaptureArtifact, ContextChain, ContextChainKind, ContextFrame,
        DiagnosticDocument, DocumentCompleteness, Location, MessageText, NodeCompleteness, Origin,
        Ownership, Phase, ProducerInfo, Provenance, ProvenanceSource, RunInfo, SemanticRole,
        Severity, ToolInfo,
    };
    use std::fs;
    use std::path::PathBuf;

    fn sample_location(path: &str, line: u32, column: u32, ownership: Ownership) -> Location {
        Location::caret(path, line, column, diag_core::LocationRole::Primary)
            .with_ownership(ownership, ownership_reason(ownership))
    }

    fn ownership_reason(ownership: Ownership) -> &'static str {
        match ownership {
            Ownership::User => "user_workspace",
            Ownership::Vendor => "vendor_path",
            Ownership::System => "system_path",
            Ownership::Generated => "generated_path",
            Ownership::Tool => "tool_generated",
            Ownership::Unknown => "unknown",
        }
    }

    fn sample_analysis(
        family: &str,
        headline: &str,
        first_action_hint: Option<&str>,
        confidence: diag_core::Confidence,
        rule_id: &str,
    ) -> AnalysisOverlay {
        AnalysisOverlay {
            family: Some(family.to_string()),
            family_version: None,
            family_confidence: None,
            root_cause_score: None,
            actionability_score: None,
            user_code_priority: None,
            headline: Some(headline.to_string()),
            first_action_hint: first_action_hint.map(ToString::to_string),
            confidence: Some(confidence.score()),
            preferred_primary_location_id: None,
            rule_id: Some(rule_id.to_string()),
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

    fn sample_request() -> RenderRequest {
        RenderRequest {
            document: DiagnosticDocument {
                document_id: "doc".to_string(),
                schema_version: "1".to_string(),
                document_completeness: DocumentCompleteness::Complete,
                producer: ProducerInfo {
                    name: "gcc-formed".to_string(),
                    version: "0.1.0".to_string(),
                    git_revision: None,
                    build_profile: None,
                    rulepack_version: None,
                },
                run: RunInfo {
                    invocation_id: "inv".to_string(),
                    invoked_as: Some("gcc-formed".to_string()),
                    argv_redacted: vec![
                        "gcc".to_string(),
                        "-c".to_string(),
                        "src/main.c".to_string(),
                    ],
                    cwd_display: Some("/tmp/project".to_string()),
                    exit_status: 1,
                    primary_tool: ToolInfo {
                        name: "gcc".to_string(),
                        version: Some("15.1.0".to_string()),
                        component: None,
                        vendor: Some("GNU".to_string()),
                    },
                    secondary_tools: Vec::new(),
                    language_mode: Some(diag_core::LanguageMode::C),
                    target_triple: None,
                    wrapper_mode: Some(diag_core::WrapperSurface::Terminal),
                },
                captures: vec![CaptureArtifact {
                    id: "stderr.raw".to_string(),
                    kind: diag_core::ArtifactKind::CompilerStderrText,
                    media_type: "text/plain".to_string(),
                    encoding: Some("utf-8".to_string()),
                    digest_sha256: None,
                    size_bytes: Some(12),
                    storage: diag_core::ArtifactStorage::Inline,
                    inline_text: Some("stderr".to_string()),
                    external_ref: None,
                    produced_by: None,
                }],
                integrity_issues: Vec::new(),
                diagnostics: vec![diag_core::DiagnosticNode {
                    id: "root".to_string(),
                    origin: Origin::Gcc,
                    phase: Phase::Parse,
                    severity: Severity::Error,
                    semantic_role: SemanticRole::Root,
                    message: MessageText {
                        raw_text: "expected ';' before '}' token".to_string(),
                        normalized_text: None,
                        locale: None,
                    },
                    locations: vec![sample_location("src/main.c", 2, 13, Ownership::User)],
                    children: Vec::new(),
                    suggestions: Vec::new(),
                    context_chains: Vec::new(),
                    symbol_context: None,
                    node_completeness: NodeCompleteness::Complete,
                    provenance: Provenance {
                        source: ProvenanceSource::Compiler,
                        capture_refs: vec!["stderr.raw".to_string()],
                    },
                    analysis: Some({
                        let mut analysis = sample_analysis(
                            "syntax",
                            "syntax error",
                            Some("fix the first parser error at the user-owned location"),
                            diag_core::Confidence::High,
                            "rule.syntax.expected_or_before",
                        );
                        analysis.matched_conditions = vec!["message_contains=expected".to_string()];
                        analysis
                    }),
                    fingerprints: None,
                }],
                fingerprints: None,
            },
            profile: RenderProfile::Default,
            capabilities: RenderCapabilities {
                stream_kind: StreamKind::Pipe,
                width_columns: Some(100),
                ansi_color: false,
                unicode: false,
                hyperlinks: false,
                interactive: false,
            },
            cwd: Some(PathBuf::from("/tmp/project")),
            path_policy: PathPolicy::RelativeToCwd,
            warning_visibility: WarningVisibility::Auto,
            debug_refs: DebugRefs::None,
            type_display_policy: TypeDisplayPolicy::CompactSafe,
            source_excerpt_policy: SourceExcerptPolicy::ForceOff,
        }
    }

    fn write_source_file(root: &tempfile::TempDir, relative: &str, contents: &str) {
        let path = root.path().join(relative);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(path, contents).unwrap();
    }

    #[test]
    fn view_model_serialization_is_stable() {
        let request = sample_request();
        let left = diag_core::canonical_json(&build_view_model(&request).unwrap()).unwrap();
        let right = diag_core::canonical_json(&build_view_model(&request).unwrap()).unwrap();
        assert_eq!(left, right);
        assert!(left.contains("syntax error"));
    }

    #[test]
    fn verbose_render_includes_rule_explainability() {
        for profile in [RenderProfile::Verbose, RenderProfile::Debug] {
            let mut request = sample_request();
            request.profile = profile;
            let output = render(request).unwrap();
            assert!(!output.used_fallback);
            assert_eq!(output.fallback_reason, None);
            assert!(
                output
                    .text
                    .contains("debug: rule_id=rule.syntax.expected_or_before")
            );
            assert!(
                output
                    .text
                    .contains("debug: matched_conditions=message_contains=expected")
            );
        }
    }

    #[test]
    fn debug_profile_uses_documented_budget() {
        let budget = crate::budget::budget_for(RenderProfile::Debug);
        let disclosure = crate::budget::disclosure_policy_for(RenderProfile::Debug);

        assert_eq!(budget.expanded_groups, usize::MAX);
        assert_eq!(budget.first_screenful_max_lines, 120);
        assert_eq!(budget.source_excerpts, 8);
        assert_eq!(budget.template_frames, 30);
        assert_eq!(budget.macro_include_frames, 20);
        assert_eq!(budget.candidate_notes, 20);
        assert!(matches!(
            budget.warning_failure_mode,
            crate::budget::WarningFailureMode::Show
        ));
        assert_eq!(disclosure.raw_sub_block_lines, 6);
        assert!(
            disclosure
                .truncation_notice
                .contains("--formed-profile=debug")
        );
    }

    #[test]
    fn raw_fallback_profile_sets_user_opt_out_reason() {
        let mut request = sample_request();
        request.profile = RenderProfile::RawFallback;
        let output = render(request).unwrap();

        assert!(output.used_fallback);
        assert_eq!(output.fallback_reason, Some(FallbackReason::UserOptOut));
        assert!(output.text.contains("showing a conservative wrapper view"));
    }

    #[test]
    fn raw_fallback_prefers_preserved_stderr_capture() {
        let mut request = sample_request();
        request.profile = RenderProfile::RawFallback;
        request.document.captures[0].inline_text = Some(
            "In file included from src/wrapper.h:1:\nsrc/main.c:2:13: error: original compiler order\nsrc/main.c:2:13: note: prefixed context".to_string(),
        );
        request.document.captures[0].size_bytes = Some(122);
        request.document.diagnostics[0].message.raw_text =
            "reconstructed diagnostic text should stay hidden".to_string();

        let output = render(request).unwrap();
        let header_index = output
            .text
            .find("  In file included from src/wrapper.h:1:")
            .unwrap();
        let error_index = output
            .text
            .find("  src/main.c:2:13: error: original compiler order")
            .unwrap();
        let note_index = output
            .text
            .find("  src/main.c:2:13: note: prefixed context")
            .unwrap();

        assert!(output.used_fallback);
        assert!(header_index < error_index);
        assert!(error_index < note_index);
        assert!(
            !output
                .text
                .contains("reconstructed diagnostic text should stay hidden")
        );
        assert!(!output.text.contains(
            "raw stderr capture is unavailable; showing reconstructed diagnostic messages"
        ));
    }

    #[test]
    fn raw_fallback_marks_reconstructed_output_when_stderr_capture_missing() {
        let mut request = sample_request();
        request.profile = RenderProfile::RawFallback;
        request.document.captures.clear();
        request.document.diagnostics[0].message.raw_text =
            "first reconstructed line\nsecond reconstructed line".to_string();
        request
            .document
            .diagnostics
            .push(diag_core::DiagnosticNode {
                id: "secondary".to_string(),
                origin: Origin::Gcc,
                phase: Phase::Semantic,
                severity: Severity::Error,
                semantic_role: SemanticRole::Root,
                message: MessageText {
                    raw_text: "third reconstructed line".to_string(),
                    normalized_text: None,
                    locale: None,
                },
                locations: vec![sample_location("src/other.c", 9, 4, Ownership::User)],
                children: Vec::new(),
                suggestions: Vec::new(),
                context_chains: Vec::new(),
                symbol_context: None,
                node_completeness: NodeCompleteness::Complete,
                provenance: Provenance {
                    source: ProvenanceSource::Compiler,
                    capture_refs: Vec::new(),
                },
                analysis: None,
                fingerprints: None,
            });

        let output = render(request).unwrap();
        let first_index = output.text.find("  first reconstructed line").unwrap();
        let second_index = output.text.find("  second reconstructed line").unwrap();
        let third_index = output.text.find("  third reconstructed line").unwrap();

        assert!(output.used_fallback);
        assert!(output.text.contains(
            "note: raw stderr capture is unavailable; showing reconstructed diagnostic messages"
        ));
        assert!(first_index < second_index);
        assert!(second_index < third_index);
    }

    #[test]
    fn excerpt_emits_caret_annotation_for_point_location() {
        let tempdir = tempfile::tempdir().unwrap();
        write_source_file(
            &tempdir,
            "src/main.c",
            "int main(void) {\n    return }\n}\n",
        );
        let mut request = sample_request();
        request.cwd = Some(tempdir.path().to_path_buf());
        request.source_excerpt_policy = SourceExcerptPolicy::ForceOn;
        request.document.diagnostics[0].locations =
            vec![sample_location("src/main.c", 2, 12, Ownership::User)];

        let view = build_view_model(&request).unwrap();
        assert_eq!(view.cards[0].excerpts[0].lines, vec!["    return }"]);
        assert_eq!(
            view.cards[0].excerpts[0].annotations,
            vec![format!("{}^", " ".repeat(11))]
        );

        let output = render(request).unwrap();
        assert!(output.text.contains("|     return }"));
        assert!(output.text.contains(&format!("| {}^", " ".repeat(11))));
    }

    #[test]
    fn excerpt_emits_range_annotation_for_single_line_range() {
        let tempdir = tempfile::tempdir().unwrap();
        write_source_file(&tempdir, "src/main.c", "int wrong;\n");
        let mut request = sample_request();
        request.cwd = Some(tempdir.path().to_path_buf());
        request.source_excerpt_policy = SourceExcerptPolicy::ForceOn;
        let mut location = sample_location("src/main.c", 1, 5, Ownership::User).with_range_end(
            1,
            8,
            diag_core::BoundarySemantics::InclusiveEnd,
        );
        location.label = Some("bad token".to_string());
        request.document.diagnostics[0].locations = vec![location];

        let view = build_view_model(&request).unwrap();
        assert_eq!(
            view.cards[0].excerpts[0].annotations,
            vec![format!("{}^~~~ bad token", " ".repeat(4))]
        );
    }

    #[test]
    fn excerpt_uses_honest_summary_for_multiline_ranges() {
        let tempdir = tempfile::tempdir().unwrap();
        write_source_file(
            &tempdir,
            "src/main.c",
            "int main(void) {\n    first();\n    second();\n}\n",
        );
        let mut request = sample_request();
        request.cwd = Some(tempdir.path().to_path_buf());
        request.source_excerpt_policy = SourceExcerptPolicy::ForceOn;
        request.document.diagnostics[0].locations = vec![
            sample_location("src/main.c", 2, 5, Ownership::User).with_range_end(
                3,
                10,
                diag_core::BoundarySemantics::HalfOpen,
            ),
        ];

        let view = build_view_model(&request).unwrap();
        assert_eq!(view.cards[0].excerpts[0].lines, vec!["    first();"]);
        assert_eq!(
            view.cards[0].excerpts[0].annotations,
            vec![format!("{}^ range spans 2 lines to 3:10", " ".repeat(4))]
        );
    }

    #[test]
    fn excerpt_uses_column_summary_when_precise_alignment_is_not_safe() {
        let tempdir = tempfile::tempdir().unwrap();
        write_source_file(&tempdir, "src/main.c", "    café();\n");
        let mut request = sample_request();
        request.cwd = Some(tempdir.path().to_path_buf());
        request.source_excerpt_policy = SourceExcerptPolicy::ForceOn;
        request.document.diagnostics[0].locations =
            vec![sample_location("src/main.c", 1, 9, Ownership::User)];

        let view = build_view_model(&request).unwrap();
        assert_eq!(view.cards[0].excerpts[0].annotations, vec!["column 9"]);
    }

    #[test]
    fn passthrough_document_sets_residual_only_reason() {
        let mut request = sample_request();
        request.document.document_completeness = DocumentCompleteness::Passthrough;
        let output = render(request).unwrap();

        assert!(output.used_fallback);
        assert_eq!(output.fallback_reason, Some(FallbackReason::ResidualOnly));
    }

    #[test]
    fn failed_document_sets_internal_error_reason() {
        let mut request = sample_request();
        request.document.document_completeness = DocumentCompleteness::Failed;
        let output = render(request).unwrap();

        assert!(output.used_fallback);
        assert_eq!(output.fallback_reason, Some(FallbackReason::InternalError));
    }

    #[test]
    fn empty_selection_sets_renderer_low_confidence_reason() {
        let mut request = sample_request();
        request.document.diagnostics.clear();
        let output = render(request).unwrap();

        assert!(output.used_fallback);
        assert_eq!(
            output.fallback_reason,
            Some(FallbackReason::RendererLowConfidence)
        );
        assert!(output.text.contains("stderr"));
    }

    #[test]
    fn selector_prefers_user_owned_high_confidence_root() {
        let mut request = sample_request();
        request
            .document
            .diagnostics
            .push(diag_core::DiagnosticNode {
                id: "secondary".to_string(),
                origin: Origin::Gcc,
                phase: Phase::Semantic,
                severity: Severity::Error,
                semantic_role: SemanticRole::Supporting,
                message: MessageText {
                    raw_text: "system header error".to_string(),
                    normalized_text: None,
                    locale: None,
                },
                locations: vec![sample_location(
                    "/usr/include/stdio.h",
                    4,
                    2,
                    Ownership::System,
                )],
                children: Vec::new(),
                suggestions: Vec::new(),
                context_chains: Vec::new(),
                symbol_context: None,
                node_completeness: NodeCompleteness::Complete,
                provenance: Provenance {
                    source: ProvenanceSource::Compiler,
                    capture_refs: vec!["stderr.raw".to_string()],
                },
                analysis: Some({
                    let mut analysis = sample_analysis(
                        "type_overload",
                        "type or overload mismatch",
                        Some("compare the expected type and actual argument at the call site"),
                        diag_core::Confidence::Medium,
                        "rule.family.type_overload.message",
                    );
                    analysis.matched_conditions =
                        vec!["message_contains=invalid conversion".to_string()];
                    analysis
                }),
                fingerprints: None,
            });

        let selection = select_groups(&request);
        assert_eq!(selection.cards.len(), 1);
        assert_eq!(selection.cards[0].id, "root");
    }

    #[test]
    fn selector_prefers_complete_structured_root_over_partial_residual_echo() {
        let mut request = sample_request();
        request.document.document_completeness = DocumentCompleteness::Partial;
        request
            .document
            .diagnostics
            .push(diag_core::DiagnosticNode {
                id: "residual-compiler-0".to_string(),
                origin: Origin::Gcc,
                phase: Phase::Parse,
                severity: Severity::Error,
                semantic_role: SemanticRole::Root,
                message: MessageText {
                    raw_text: "src/main.c:1:1: error: expected ';' before '}' token".to_string(),
                    normalized_text: None,
                    locale: None,
                },
                locations: vec![
                    Location::caret("src/main.c", 1, 1, diag_core::LocationRole::Primary)
                        .with_ownership(Ownership::User, ownership_reason(Ownership::User)),
                ],
                children: Vec::new(),
                suggestions: Vec::new(),
                context_chains: Vec::new(),
                symbol_context: None,
                node_completeness: NodeCompleteness::Partial,
                provenance: Provenance {
                    source: ProvenanceSource::ResidualText,
                    capture_refs: vec!["stderr.raw".to_string()],
                },
                analysis: Some({
                    let mut analysis = sample_analysis(
                        "syntax",
                        "syntax error",
                        Some("fix the first parser error at the user-owned location"),
                        diag_core::Confidence::High,
                        "rule.family.syntax.phase_or_message",
                    );
                    analysis.matched_conditions = vec![
                        "message_contains=expected".to_string(),
                        "message_contains=before".to_string(),
                    ];
                    analysis
                }),
                fingerprints: None,
            });

        let selection = select_groups(&request);
        assert_eq!(selection.cards.len(), 1);
        assert_eq!(selection.cards[0].id, "root");
    }

    #[test]
    fn selector_does_not_boost_unknown_family_over_useful_subset() {
        let mut request = sample_request();
        request.document.diagnostics[0].id = "z-syntax".to_string();
        request.document.diagnostics[0]
            .analysis
            .as_mut()
            .unwrap()
            .family = Some("syntax".to_string());

        let mut opaque = request.document.diagnostics[0].clone();
        opaque.id = "a-opaque".to_string();
        opaque.message.raw_text = "opaque compatibility residual".to_string();
        let analysis = opaque.analysis.as_mut().unwrap();
        analysis.family = Some("compiler.residual".to_string());
        analysis.headline = Some("opaque compatibility residual".to_string());
        analysis.rule_id = Some("rule.residual.compiler_unknown".to_string());

        request.document.diagnostics.push(opaque);

        let selection = select_groups(&request);
        assert_eq!(selection.cards.len(), 1);
        assert_eq!(selection.cards[0].id, "z-syntax");
    }

    #[test]
    fn default_profile_suppresses_warnings_after_failure() {
        let mut request = sample_request();
        request
            .document
            .diagnostics
            .push(diag_core::DiagnosticNode {
                id: "warning".to_string(),
                origin: Origin::Gcc,
                phase: Phase::Semantic,
                severity: Severity::Warning,
                semantic_role: SemanticRole::Supporting,
                message: MessageText {
                    raw_text: "unused variable 'tmp'".to_string(),
                    normalized_text: None,
                    locale: None,
                },
                locations: vec![sample_location("src/main.c", 7, 5, Ownership::User)],
                children: Vec::new(),
                suggestions: Vec::new(),
                context_chains: Vec::new(),
                symbol_context: None,
                node_completeness: NodeCompleteness::Complete,
                provenance: Provenance {
                    source: ProvenanceSource::Compiler,
                    capture_refs: vec!["stderr.raw".to_string()],
                },
                analysis: None,
                fingerprints: None,
            });

        let selection = select_groups(&request);
        assert_eq!(selection.cards.len(), 1);
        assert_eq!(selection.cards[0].id, "root");
        assert_eq!(selection.suppressed_warning_count, 1);
        assert!(selection.summary_only_cards.is_empty());
    }

    #[test]
    fn verbose_profile_keeps_warnings_after_failure() {
        for profile in [RenderProfile::Verbose, RenderProfile::Debug] {
            let mut request = sample_request();
            request.profile = profile;
            request
                .document
                .diagnostics
                .push(diag_core::DiagnosticNode {
                    id: "warning".to_string(),
                    origin: Origin::Gcc,
                    phase: Phase::Semantic,
                    severity: Severity::Warning,
                    semantic_role: SemanticRole::Supporting,
                    message: MessageText {
                        raw_text: "unused variable 'tmp'".to_string(),
                        normalized_text: None,
                        locale: None,
                    },
                    locations: vec![sample_location("src/main.c", 7, 5, Ownership::User)],
                    children: Vec::new(),
                    suggestions: Vec::new(),
                    context_chains: Vec::new(),
                    symbol_context: None,
                    node_completeness: NodeCompleteness::Complete,
                    provenance: Provenance {
                        source: ProvenanceSource::Compiler,
                        capture_refs: vec!["stderr.raw".to_string()],
                    },
                    analysis: None,
                    fingerprints: None,
                });

            let selection = select_groups(&request);
            assert_eq!(selection.cards.len(), 2);
            assert_eq!(selection.suppressed_warning_count, 0);
            assert!(selection.summary_only_cards.is_empty());
        }
    }

    #[test]
    fn default_profile_expands_two_warning_groups() {
        let mut request = sample_request();
        request.document.diagnostics = (1..=3)
            .map(|index| diag_core::DiagnosticNode {
                id: format!("warning-{index}"),
                origin: Origin::Gcc,
                phase: Phase::Semantic,
                severity: Severity::Warning,
                semantic_role: SemanticRole::Root,
                message: MessageText {
                    raw_text: format!("warning {index}"),
                    normalized_text: None,
                    locale: None,
                },
                locations: vec![sample_location("src/main.c", index, 1, Ownership::User)],
                children: Vec::new(),
                suggestions: Vec::new(),
                context_chains: Vec::new(),
                symbol_context: None,
                node_completeness: NodeCompleteness::Complete,
                provenance: Provenance {
                    source: ProvenanceSource::Compiler,
                    capture_refs: vec!["stderr.raw".to_string()],
                },
                analysis: None,
                fingerprints: None,
            })
            .collect();

        let selection = select_groups(&request);
        assert_eq!(selection.cards.len(), 2);
        assert_eq!(selection.suppressed_warning_count, 0);
        assert_eq!(selection.summary_only_cards.len(), 1);
        assert_eq!(selection.summary_only_cards[0].id, "warning-3");
    }

    #[test]
    fn low_confidence_primary_group_expands_second_group() {
        let mut request = sample_request();
        request.document.diagnostics[0]
            .analysis
            .as_mut()
            .unwrap()
            .set_confidence_bucket(diag_core::Confidence::Low);
        request
            .document
            .diagnostics
            .push(diag_core::DiagnosticNode {
                id: "supporting-note".to_string(),
                origin: Origin::Gcc,
                phase: Phase::Semantic,
                severity: Severity::Note,
                semantic_role: SemanticRole::Root,
                message: MessageText {
                    raw_text: "candidate expects an int parameter".to_string(),
                    normalized_text: None,
                    locale: None,
                },
                locations: vec![sample_location("src/main.c", 1, 5, Ownership::User)],
                children: Vec::new(),
                suggestions: Vec::new(),
                context_chains: Vec::new(),
                symbol_context: None,
                node_completeness: NodeCompleteness::Complete,
                provenance: Provenance {
                    source: ProvenanceSource::Compiler,
                    capture_refs: vec!["stderr.raw".to_string()],
                },
                analysis: Some({
                    let mut analysis = sample_analysis(
                        "type_overload",
                        "candidate expects an int parameter",
                        None,
                        diag_core::Confidence::High,
                        "rule.family.type_overload.note",
                    );
                    analysis.matched_conditions = vec!["semantic_role=root".to_string()];
                    analysis
                }),
                fingerprints: None,
            });

        let selection = select_groups(&request);
        assert_eq!(selection.cards.len(), 2);
        assert_eq!(selection.cards[0].id, "root");
        assert_eq!(selection.cards[1].id, "supporting-note");
        assert!(selection.summary_only_cards.is_empty());
    }

    #[test]
    fn selector_keeps_non_lead_groups_as_summary_only_when_group_budget_is_exceeded() {
        let mut request = sample_request();
        request
            .document
            .diagnostics
            .push(diag_core::DiagnosticNode {
                id: "secondary".to_string(),
                origin: Origin::Gcc,
                phase: Phase::Semantic,
                severity: Severity::Error,
                semantic_role: SemanticRole::Root,
                message: MessageText {
                    raw_text: "secondary failure".to_string(),
                    normalized_text: None,
                    locale: None,
                },
                locations: vec![sample_location("src/other.c", 9, 4, Ownership::User)],
                children: Vec::new(),
                suggestions: Vec::new(),
                context_chains: Vec::new(),
                symbol_context: None,
                node_completeness: NodeCompleteness::Complete,
                provenance: Provenance {
                    source: ProvenanceSource::Compiler,
                    capture_refs: vec!["stderr.raw".to_string()],
                },
                analysis: None,
                fingerprints: None,
            });
        request
            .document
            .diagnostics
            .push(diag_core::DiagnosticNode {
                id: "tertiary".to_string(),
                origin: Origin::Gcc,
                phase: Phase::Semantic,
                severity: Severity::Error,
                semantic_role: SemanticRole::Root,
                message: MessageText {
                    raw_text: "tertiary failure".to_string(),
                    normalized_text: None,
                    locale: None,
                },
                locations: vec![sample_location("src/third.c", 12, 7, Ownership::User)],
                children: Vec::new(),
                suggestions: Vec::new(),
                context_chains: Vec::new(),
                symbol_context: None,
                node_completeness: NodeCompleteness::Complete,
                provenance: Provenance {
                    source: ProvenanceSource::Compiler,
                    capture_refs: vec!["stderr.raw".to_string()],
                },
                analysis: None,
                fingerprints: None,
            });

        let selection = select_groups(&request);

        assert_eq!(selection.cards.len(), 1);
        assert_eq!(selection.cards[0].id, "root");
        assert_eq!(selection.summary_only_cards.len(), 2);
        assert_eq!(selection.summary_only_cards[0].id, "secondary");
        assert_eq!(selection.summary_only_cards[1].id, "tertiary");
    }

    #[test]
    fn render_emits_summary_only_groups_and_reports_suppressed_count() {
        let mut request = sample_request();
        request
            .document
            .diagnostics
            .push(diag_core::DiagnosticNode {
                id: "secondary".to_string(),
                origin: Origin::Gcc,
                phase: Phase::Semantic,
                severity: Severity::Error,
                semantic_role: SemanticRole::Root,
                message: MessageText {
                    raw_text: "secondary failure".to_string(),
                    normalized_text: None,
                    locale: None,
                },
                locations: vec![sample_location("src/other.c", 9, 4, Ownership::User)],
                children: Vec::new(),
                suggestions: Vec::new(),
                context_chains: Vec::new(),
                symbol_context: None,
                node_completeness: NodeCompleteness::Complete,
                provenance: Provenance {
                    source: ProvenanceSource::Compiler,
                    capture_refs: vec!["stderr.raw".to_string()],
                },
                analysis: None,
                fingerprints: None,
            });
        request
            .document
            .diagnostics
            .push(diag_core::DiagnosticNode {
                id: "tertiary".to_string(),
                origin: Origin::Gcc,
                phase: Phase::Semantic,
                severity: Severity::Error,
                semantic_role: SemanticRole::Root,
                message: MessageText {
                    raw_text: "tertiary failure".to_string(),
                    normalized_text: None,
                    locale: None,
                },
                locations: vec![sample_location("src/third.c", 12, 7, Ownership::User)],
                children: Vec::new(),
                suggestions: Vec::new(),
                context_chains: Vec::new(),
                symbol_context: None,
                node_completeness: NodeCompleteness::Complete,
                provenance: Provenance {
                    source: ProvenanceSource::Compiler,
                    capture_refs: vec!["stderr.raw".to_string()],
                },
                analysis: None,
                fingerprints: None,
            });

        let output = render(request).unwrap();

        assert!(!output.used_fallback);
        assert_eq!(output.displayed_group_refs, vec!["root".to_string()]);
        assert_eq!(output.suppressed_group_count, 2);
        assert!(output.text.contains("other errors:"));
        assert!(
            output
                .text
                .contains("  - src/other.c:9:4: error: secondary failure")
        );
        assert!(
            output
                .text
                .contains("  - src/third.c:12:7: error: tertiary failure")
        );
    }

    #[test]
    fn low_confidence_render_uses_raw_title_and_honesty_notice() {
        let mut request = sample_request();
        request.document.diagnostics[0].message.raw_text =
            "static assertion failed: size must be 4 bytes".to_string();
        let analysis = request.document.diagnostics[0].analysis.as_mut().unwrap();
        analysis.family = Some("unknown".to_string());
        analysis.headline = Some("template instantiation failed".to_string());
        analysis.first_action_hint = Some(
            "start from the first user-owned template frame and match template arguments"
                .to_string(),
        );
        analysis.set_confidence_bucket(diag_core::Confidence::Low);

        let output = render(request).unwrap();

        assert!(
            output
                .text
                .contains("error: static assertion failed: size must be 4 bytes")
        );
        assert!(output.text.contains(
            "note: wrapper confidence is low; verify against the preserved raw diagnostics"
        ));
        assert!(
            !output
                .text
                .contains("help: start from the first user-owned template frame")
        );
    }

    #[test]
    fn subthreshold_score_hides_analysis_title_and_first_action() {
        let mut request = sample_request();
        request.document.diagnostics[0].message.raw_text =
            "static assertion failed: size must be 4 bytes".to_string();
        let analysis = request.document.diagnostics[0].analysis.as_mut().unwrap();
        analysis.family = Some("unknown".to_string());
        analysis.headline = Some("type or overload mismatch".to_string());
        analysis.first_action_hint =
            Some("compare the expected type and actual argument at the call site".to_string());
        analysis.set_confidence_score(0.59);

        let view_model = build_view_model(&request).unwrap();
        let card = &view_model.cards[0];

        assert_eq!(card.confidence_label, "possible");
        assert_eq!(card.title, "static assertion failed: size must be 4 bytes");
        assert_eq!(card.first_action, None);
        assert!(card.confidence_notice.is_some());
    }

    #[test]
    fn threshold_score_keeps_analysis_title_and_first_action() {
        let mut request = sample_request();
        request.document.diagnostics[0].message.raw_text =
            "static assertion failed: size must be 4 bytes".to_string();
        let analysis = request.document.diagnostics[0].analysis.as_mut().unwrap();
        analysis.family = Some("unknown".to_string());
        analysis.headline = Some("type or overload mismatch".to_string());
        analysis.first_action_hint =
            Some("compare the expected type and actual argument at the call site".to_string());
        analysis.set_confidence_score(0.60);

        let view_model = build_view_model(&request).unwrap();
        let card = &view_model.cards[0];

        assert_eq!(card.confidence_label, "likely");
        assert_eq!(card.title, "type or overload mismatch");
        assert_eq!(
            card.first_action.as_deref(),
            Some("compare the expected type and actual argument at the call site")
        );
        assert_eq!(card.confidence_notice, None);
    }

    #[test]
    fn band_c_useful_subset_render_strengthens_notice_and_raw_label() {
        let mut request = sample_request();
        request.document.run.primary_tool.version = Some("12.2.0".to_string());
        request.document.document_completeness = DocumentCompleteness::Partial;
        request.document.diagnostics[0].node_completeness = NodeCompleteness::Partial;
        request.document.diagnostics[0].phase = Phase::Semantic;
        request.document.diagnostics[0].provenance.source = ProvenanceSource::ResidualText;
        request.document.diagnostics[0].message.raw_text =
            "src/main.cpp:5:7: error: no matching function for call to 'takes(int)'".to_string();
        request.document.diagnostics[0].locations[0].set_path_raw("src/main.cpp");
        request.document.diagnostics[0].locations[0].set_anchor(5, 7);
        let analysis = request.document.diagnostics[0].analysis.as_mut().unwrap();
        analysis.family = Some("type_overload".to_string());
        analysis.headline = Some("type or overload mismatch".to_string());
        analysis.first_action_hint =
            Some("compare the expected type and actual argument at the call site".to_string());
        analysis.set_confidence_bucket(diag_core::Confidence::Low);
        analysis.rule_id = Some("rule.residual.compiler_type_overload".to_string());
        analysis.matched_conditions = vec!["family=type_overload".to_string()];

        request.document.diagnostics[0].children = vec![diag_core::DiagnosticNode {
            id: "candidate-1".to_string(),
            origin: Origin::Gcc,
            phase: Phase::Semantic,
            severity: Severity::Note,
            semantic_role: SemanticRole::Candidate,
            message: MessageText {
                raw_text: "src/main.cpp:2:6: note: candidate 1: 'void takes(int, int)'".to_string(),
                normalized_text: None,
                locale: None,
            },
            locations: vec![sample_location("src/main.cpp", 2, 6, Ownership::User)],
            children: Vec::new(),
            suggestions: Vec::new(),
            context_chains: Vec::new(),
            symbol_context: None,
            node_completeness: NodeCompleteness::Partial,
            provenance: Provenance {
                source: ProvenanceSource::ResidualText,
                capture_refs: vec!["stderr.raw".to_string()],
            },
            analysis: None,
            fingerprints: None,
        }];

        let output = render(request).unwrap();

        assert!(output.text.contains(
            "note: GCC 9-12 native-text summaries are conservative; verify against the preserved raw diagnostics"
        ));
        assert!(output.text.contains("raw compiler excerpt:"));
        assert!(
            output
                .text
                .contains("candidate 1: 'void takes(int, int)' at src/main.cpp:2:6")
        );
        assert!(!output.text.contains("because:"));
        assert!(
            !output
                .text
                .contains("help: compare the expected type and actual argument at the call site")
        );
    }

    #[test]
    fn partial_render_emits_mixed_fallback_raw_block() {
        let mut request = sample_request();
        request.document.document_completeness = DocumentCompleteness::Partial;
        request.document.diagnostics[0].node_completeness = NodeCompleteness::Partial;
        request.document.diagnostics[0].message.raw_text =
            "src/main.c:2:13: error: expected ';' before '}' token".to_string();

        let output = render(request).unwrap();

        assert!(!output.used_fallback);
        assert!(output.text.contains(
            "note: some compiler details were not fully structured; original diagnostics are preserved"
        ));
        assert!(
            output
                .text
                .contains("raw:\n  src/main.c:2:13: error: expected ';' before '}' token")
        );
    }

    #[test]
    fn complete_lead_with_sidecar_residual_omits_partial_notice() {
        let mut request = sample_request();
        request.document.document_completeness = DocumentCompleteness::Partial;
        request
            .document
            .diagnostics
            .push(diag_core::DiagnosticNode {
                id: "residual-compiler-0".to_string(),
                origin: Origin::Gcc,
                phase: Phase::Parse,
                severity: Severity::Error,
                semantic_role: SemanticRole::Root,
                message: MessageText {
                    raw_text: "src/main.c:1:1: error: expected ';' before '}' token".to_string(),
                    normalized_text: None,
                    locale: None,
                },
                locations: vec![
                    Location::caret("src/main.c", 1, 1, diag_core::LocationRole::Primary)
                        .with_ownership(Ownership::User, ownership_reason(Ownership::User)),
                ],
                children: Vec::new(),
                suggestions: Vec::new(),
                context_chains: Vec::new(),
                symbol_context: None,
                node_completeness: NodeCompleteness::Partial,
                provenance: Provenance {
                    source: ProvenanceSource::ResidualText,
                    capture_refs: vec!["stderr.raw".to_string()],
                },
                analysis: Some(sample_analysis(
                    "syntax",
                    "syntax error",
                    Some("fix the first parser error at the user-owned location"),
                    diag_core::Confidence::High,
                    "rule.family.syntax.phase_or_message",
                )),
                fingerprints: None,
            });

        let output = render(request).unwrap();

        assert!(!output.used_fallback);
        assert!(!output.text.contains(
            "note: some compiler details were not fully structured; original diagnostics are preserved"
        ));
        assert!(output.text.contains(
            "raw: rerun with --formed-profile=raw_fallback to inspect the original compiler output"
        ));
    }

    #[test]
    fn ci_render_sanitizes_transient_object_paths() {
        let mut request = sample_request();
        request.profile = RenderProfile::Ci;
        request.document.document_completeness = DocumentCompleteness::Partial;
        request.document.diagnostics[0].phase = Phase::Link;
        request.document.diagnostics[0].node_completeness = NodeCompleteness::Partial;
        request.document.diagnostics[0].locations.clear();
        request.document.diagnostics[0].message.raw_text =
            "helper.c:(.text+0x0): multiple definition of `duplicate'; /tmp/ccnwX900.o:main.c:(.text+0x0): first defined here".to_string();
        request.document.diagnostics[0].analysis = Some({
            let mut analysis = sample_analysis(
                "linker.multiple_definition",
                "multiple definition of `duplicate`",
                Some(
                    "remove the duplicate definition or make the symbol internal to one translation unit",
                ),
                diag_core::Confidence::High,
                "rule.family.linker.multiple_definition",
            );
            analysis.matched_conditions = vec!["symbol_context=present".to_string()];
            analysis
        });

        let output = render(request).unwrap();

        assert!(output.text.contains(
            "why: helper.c:(.text+0x0): multiple definition of `duplicate'; <temp-object>:main.c:(.text+0x0): first defined here"
        ));
        assert!(output.text.contains(
            "raw:\n  helper.c:(.text+0x0): multiple definition of `duplicate'; <temp-object>:main.c:(.text+0x0): first defined here"
        ));
        assert!(!output.text.contains("/tmp/ccnwX900.o"));
    }

    #[test]
    fn summarize_context_deduplicates_repeated_macro_frames() {
        let mut request = sample_request();
        request.document.diagnostics[0]
            .analysis
            .as_mut()
            .unwrap()
            .family = Some("macro_include".to_string());
        request.document.diagnostics[0].context_chains = vec![ContextChain {
            kind: ContextChainKind::MacroExpansion,
            frames: vec![
                ContextFrame {
                    label: "in expansion of macro 'READ_FIELD'".to_string(),
                    path: Some("src/main.c".to_string()),
                    line: Some(3),
                    column: Some(25),
                },
                ContextFrame {
                    label: "in expansion of macro 'READ_FIELD'".to_string(),
                    path: Some("src/main.c".to_string()),
                    line: Some(3),
                    column: Some(25),
                },
            ],
        }];

        let evidence = summarize_supporting_evidence(&request, &request.document.diagnostics[0]);
        assert_eq!(evidence.context_lines[0], "through macro expansion:");
        assert!(
            evidence
                .context_lines
                .iter()
                .filter(|line| line.contains("READ_FIELD"))
                .count()
                == 1
        );
        assert!(
            !evidence
                .context_lines
                .iter()
                .any(|line| line.contains("omitted"))
        );
    }

    #[test]
    fn template_supporting_evidence_respects_default_budget() {
        let mut request = sample_request();
        request.document.diagnostics[0]
            .analysis
            .as_mut()
            .unwrap()
            .family = Some("template".to_string());
        request.document.diagnostics[0].context_chains = vec![ContextChain {
            kind: ContextChainKind::TemplateInstantiation,
            frames: (1..=7)
                .map(|index| ContextFrame {
                    label: format!("instantiated from here #{index}"),
                    path: Some(format!("src/t{index}.hpp")),
                    line: Some(index),
                    column: Some(1),
                })
                .collect(),
        }];

        let evidence = summarize_supporting_evidence(&request, &request.document.diagnostics[0]);
        assert_eq!(evidence.context_lines[0], "while instantiating:");
        assert_eq!(evidence.context_lines.len(), 7);
        assert_eq!(
            evidence.context_lines[6],
            "omitted 2 internal template frames"
        );
    }

    #[test]
    fn band_c_template_supporting_evidence_uses_tighter_budget() {
        let mut request = sample_request();
        request.document.run.primary_tool.version = Some("12.3.0".to_string());
        request.document.document_completeness = DocumentCompleteness::Partial;
        request.document.diagnostics[0].node_completeness = NodeCompleteness::Partial;
        let analysis = request.document.diagnostics[0].analysis.as_mut().unwrap();
        analysis.family = Some("template".to_string());
        analysis.set_confidence_bucket(diag_core::Confidence::Low);
        analysis.rule_id = Some("rule.residual.compiler_template".to_string());
        analysis.matched_conditions = vec!["family=template".to_string()];
        request.document.diagnostics[0].provenance.source = ProvenanceSource::ResidualText;
        request.document.diagnostics[0].context_chains = vec![ContextChain {
            kind: ContextChainKind::TemplateInstantiation,
            frames: (1..=7)
                .map(|index| ContextFrame {
                    label: format!("instantiated from here #{index}"),
                    path: Some(format!("src/t{index}.hpp")),
                    line: Some(index),
                    column: Some(1),
                })
                .collect(),
        }];

        let evidence = summarize_supporting_evidence(&request, &request.document.diagnostics[0]);
        assert_eq!(evidence.context_lines[0], "while instantiating:");
        assert_eq!(evidence.context_lines.len(), 5);
        assert_eq!(
            evidence.context_lines[4],
            "omitted 4 internal template frames"
        );
    }

    #[test]
    fn template_supporting_evidence_prioritizes_user_owned_frames_when_compacted() {
        let mut request = sample_request();
        request.document.diagnostics[0]
            .analysis
            .as_mut()
            .unwrap()
            .family = Some("template".to_string());
        request.document.diagnostics[0].context_chains = vec![ContextChain {
            kind: ContextChainKind::TemplateInstantiation,
            frames: vec![
                ContextFrame {
                    label: "instantiated from here".to_string(),
                    path: Some("/usr/include/alpha.hpp".to_string()),
                    line: Some(3),
                    column: Some(1),
                },
                ContextFrame {
                    label: "instantiated from here".to_string(),
                    path: Some("/usr/include/beta.hpp".to_string()),
                    line: Some(4),
                    column: Some(1),
                },
                ContextFrame {
                    label: "instantiated from here".to_string(),
                    path: Some("/usr/include/gamma.hpp".to_string()),
                    line: Some(5),
                    column: Some(1),
                },
                ContextFrame {
                    label: "instantiated from here".to_string(),
                    path: Some("src/main.cpp".to_string()),
                    line: Some(6),
                    column: Some(1),
                },
                ContextFrame {
                    label: "instantiated from here".to_string(),
                    path: Some("/usr/include/delta.hpp".to_string()),
                    line: Some(7),
                    column: Some(1),
                },
                ContextFrame {
                    label: "instantiated from here".to_string(),
                    path: Some("/usr/include/epsilon.hpp".to_string()),
                    line: Some(8),
                    column: Some(1),
                },
            ],
        }];

        let evidence = summarize_supporting_evidence(&request, &request.document.diagnostics[0]);
        assert_eq!(evidence.context_lines[0], "while instantiating:");
        assert!(evidence.context_lines[1].contains("src/main.cpp:6:1"));
        assert!(
            evidence
                .context_lines
                .contains(&"omitted 1 internal template frames".to_string())
        );
    }

    #[test]
    fn overload_supporting_evidence_uses_best_owned_location_for_candidate_notes() {
        let mut request = sample_request();
        request.document.diagnostics[0]
            .analysis
            .as_mut()
            .unwrap()
            .family = Some("type_overload".to_string());

        let mut system_note = request.document.diagnostics[0].clone();
        system_note.id = "system-note".to_string();
        system_note.message.raw_text = "candidate conversion remains internal".to_string();
        system_note.locations = vec![sample_location(
            "/usr/include/vector",
            18,
            7,
            Ownership::System,
        )];
        system_note.children = Vec::new();
        system_note.suggestions = Vec::new();
        system_note.context_chains = Vec::new();
        system_note.symbol_context = None;
        system_note.analysis = None;
        system_note.node_completeness = NodeCompleteness::Complete;

        let mut user_note = request.document.diagnostics[0].clone();
        user_note.id = "user-note".to_string();
        user_note.message.raw_text = "candidate conversion matches the call site".to_string();
        user_note.locations = vec![
            sample_location("/usr/include/vector", 19, 3, Ownership::System),
            sample_location("src/main.cpp", 21, 9, Ownership::User),
        ];
        user_note.children = Vec::new();
        user_note.suggestions = Vec::new();
        user_note.context_chains = Vec::new();
        user_note.symbol_context = None;
        user_note.analysis = None;
        user_note.node_completeness = NodeCompleteness::Complete;

        request.document.diagnostics[0].children = vec![system_note, user_note];

        let evidence = summarize_supporting_evidence(&request, &request.document.diagnostics[0]);
        assert_eq!(
            evidence.context_lines[0],
            "because: candidate conversion matches the call site at src/main.cpp:21:9"
        );
        assert_eq!(
            evidence.context_lines[1],
            "because: candidate conversion remains internal at /usr/include/vector:18:7"
        );
    }

    #[test]
    fn band_c_overload_supporting_evidence_stays_neutral() {
        let mut request = sample_request();
        request.document.run.primary_tool.version = Some("12.1.0".to_string());
        request.document.document_completeness = DocumentCompleteness::Partial;
        request.document.diagnostics[0].node_completeness = NodeCompleteness::Partial;
        request.document.diagnostics[0].provenance.source = ProvenanceSource::ResidualText;
        let analysis = request.document.diagnostics[0].analysis.as_mut().unwrap();
        analysis.family = Some("type_overload".to_string());
        analysis.set_confidence_bucket(diag_core::Confidence::Low);
        analysis.rule_id = Some("rule.residual.compiler_type_overload".to_string());
        analysis.matched_conditions = vec!["family=type_overload".to_string()];

        request.document.diagnostics[0].children = vec![diag_core::DiagnosticNode {
            id: "candidate".to_string(),
            origin: Origin::Gcc,
            phase: Phase::Semantic,
            severity: Severity::Note,
            semantic_role: SemanticRole::Candidate,
            message: MessageText {
                raw_text: "src/main.cpp:2:6: note: candidate 1: 'void takes(int, int)'".to_string(),
                normalized_text: None,
                locale: None,
            },
            locations: vec![sample_location("src/main.cpp", 2, 6, Ownership::User)],
            children: Vec::new(),
            suggestions: Vec::new(),
            context_chains: Vec::new(),
            symbol_context: None,
            node_completeness: NodeCompleteness::Partial,
            provenance: Provenance {
                source: ProvenanceSource::ResidualText,
                capture_refs: vec!["stderr.raw".to_string()],
            },
            analysis: None,
            fingerprints: None,
        }];

        let evidence = summarize_supporting_evidence(&request, &request.document.diagnostics[0]);
        assert_eq!(
            evidence.context_lines,
            vec!["candidate 1: 'void takes(int, int)' at src/main.cpp:2:6"]
        );
    }

    #[test]
    fn generic_notes_emit_omission_notice() {
        let mut request = sample_request();
        request.document.diagnostics[0].children = (1..=5)
            .map(|index| diag_core::DiagnosticNode {
                id: format!("note-{index}"),
                origin: Origin::Gcc,
                phase: Phase::Semantic,
                severity: Severity::Note,
                semantic_role: SemanticRole::Supporting,
                message: MessageText {
                    raw_text: format!("related note {index}"),
                    normalized_text: None,
                    locale: None,
                },
                locations: Vec::new(),
                children: Vec::new(),
                suggestions: Vec::new(),
                context_chains: Vec::new(),
                symbol_context: None,
                node_completeness: NodeCompleteness::Complete,
                provenance: Provenance {
                    source: ProvenanceSource::Compiler,
                    capture_refs: vec!["stderr.raw".to_string()],
                },
                analysis: None,
                fingerprints: None,
            })
            .collect();

        let evidence = summarize_supporting_evidence(&request, &request.document.diagnostics[0]);
        assert_eq!(evidence.child_notes.len(), 3);
        assert_eq!(
            evidence.collapsed_notices,
            vec!["omitted 2 additional note(s)"]
        );
    }

    #[test]
    fn enhanced_render_escapes_terminal_control_sequences() {
        let mut request = sample_request();
        request.document.diagnostics[0].message.raw_text =
            "\u{001b}[31mexpected ';' before '}' token".to_string();
        request.document.diagnostics[0]
            .analysis
            .as_mut()
            .unwrap()
            .headline = Some("\u{001b}[31msyntax error".to_string());
        request.document.diagnostics[0].children = vec![diag_core::DiagnosticNode {
            id: "note-esc".to_string(),
            origin: Origin::Gcc,
            phase: Phase::Parse,
            severity: Severity::Note,
            semantic_role: SemanticRole::Supporting,
            message: MessageText {
                raw_text: "saw escape sequence \u{001b}[0m in source".to_string(),
                normalized_text: None,
                locale: None,
            },
            locations: Vec::new(),
            children: Vec::new(),
            suggestions: Vec::new(),
            context_chains: Vec::new(),
            symbol_context: None,
            node_completeness: NodeCompleteness::Partial,
            provenance: Provenance {
                source: ProvenanceSource::Compiler,
                capture_refs: vec!["stderr.raw".to_string()],
            },
            analysis: None,
            fingerprints: None,
        }];

        let output = render(request).unwrap();

        assert!(!output.text.contains('\u{001b}'));
        assert!(output.text.contains("\\x1b[31msyntax error"));
        assert!(
            output
                .text
                .contains("\\x1b[31mexpected ';' before '}' token")
        );
        assert!(
            output
                .text
                .contains("note: saw escape sequence \\x1b[0m in source")
        );
    }

    #[test]
    fn fallback_render_escapes_terminal_control_sequences() {
        let mut request = sample_request();
        request.profile = RenderProfile::RawFallback;
        request.document.captures[0].inline_text =
            Some("\u{001b}[31mraw compiler stderr".to_string());

        let output = render(request).unwrap();

        assert!(output.used_fallback);
        assert!(!output.text.contains('\u{001b}'));
        assert!(output.text.contains("\\x1b[31mraw compiler stderr"));
    }
}
