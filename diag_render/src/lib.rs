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
pub use view_model::{RenderGroupCard, RenderSessionSummary, RenderViewModel};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RenderProfile {
    Default,
    Concise,
    Verbose,
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
    let view_model = view_model::build(&request, selected.cards);
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
        Some(view_model::build(request, selected.cards))
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
    use std::path::PathBuf;

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
                    locations: vec![Location {
                        path: "src/main.c".to_string(),
                        line: 2,
                        column: 13,
                        end_line: None,
                        end_column: None,
                        display_path: None,
                        ownership: Some(Ownership::User),
                    }],
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
                        headline: Some("syntax error".to_string()),
                        first_action_hint: Some(
                            "fix the first parser error at the user-owned location".to_string(),
                        ),
                        confidence: Some(diag_core::Confidence::High),
                        rule_id: Some("rule.syntax.expected_or_before".to_string()),
                        matched_conditions: vec!["message_contains=expected".to_string()],
                        suppression_reason: None,
                        collapsed_child_ids: Vec::new(),
                        collapsed_chain_ids: Vec::new(),
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
        let mut request = sample_request();
        request.profile = RenderProfile::Verbose;
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
                locations: vec![Location {
                    path: "/usr/include/stdio.h".to_string(),
                    line: 4,
                    column: 2,
                    end_line: None,
                    end_column: None,
                    display_path: None,
                    ownership: Some(Ownership::System),
                }],
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
                    family: Some("type_overload".to_string()),
                    headline: Some("type or overload mismatch".to_string()),
                    first_action_hint: Some(
                        "compare the expected type and actual argument at the call site"
                            .to_string(),
                    ),
                    confidence: Some(diag_core::Confidence::Medium),
                    rule_id: Some("rule.family.type_overload.message".to_string()),
                    matched_conditions: vec!["message_contains=invalid conversion".to_string()],
                    suppression_reason: None,
                    collapsed_child_ids: Vec::new(),
                    collapsed_chain_ids: Vec::new(),
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
                locations: vec![Location {
                    path: "src/main.c".to_string(),
                    line: 7,
                    column: 5,
                    end_line: None,
                    end_column: None,
                    display_path: None,
                    ownership: Some(Ownership::User),
                }],
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
    }

    #[test]
    fn verbose_profile_keeps_warnings_after_failure() {
        let mut request = sample_request();
        request.profile = RenderProfile::Verbose;
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
                locations: vec![Location {
                    path: "src/main.c".to_string(),
                    line: 7,
                    column: 5,
                    end_line: None,
                    end_column: None,
                    display_path: None,
                    ownership: Some(Ownership::User),
                }],
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
                locations: vec![Location {
                    path: "src/main.c".to_string(),
                    line: index,
                    column: 1,
                    end_line: None,
                    end_column: None,
                    display_path: None,
                    ownership: Some(Ownership::User),
                }],
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
    }

    #[test]
    fn low_confidence_primary_group_expands_second_group() {
        let mut request = sample_request();
        request.document.diagnostics[0]
            .analysis
            .as_mut()
            .unwrap()
            .confidence = Some(diag_core::Confidence::Low);
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
                locations: vec![Location {
                    path: "src/main.c".to_string(),
                    line: 1,
                    column: 5,
                    end_line: None,
                    end_column: None,
                    display_path: None,
                    ownership: Some(Ownership::User),
                }],
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
                    family: Some("type_overload".to_string()),
                    headline: Some("candidate expects an int parameter".to_string()),
                    first_action_hint: None,
                    confidence: Some(diag_core::Confidence::High),
                    rule_id: Some("rule.family.type_overload.note".to_string()),
                    matched_conditions: vec!["semantic_role=root".to_string()],
                    suppression_reason: None,
                    collapsed_child_ids: Vec::new(),
                    collapsed_chain_ids: Vec::new(),
                }),
                fingerprints: None,
            });

        let selection = select_groups(&request);
        assert_eq!(selection.cards.len(), 2);
        assert_eq!(selection.cards[0].id, "root");
        assert_eq!(selection.cards[1].id, "supporting-note");
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
        analysis.confidence = Some(diag_core::Confidence::Low);

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
    fn band_c_useful_subset_render_strengthens_notice_and_raw_label() {
        let mut request = sample_request();
        request.document.run.primary_tool.version = Some("12.2.0".to_string());
        request.document.document_completeness = DocumentCompleteness::Partial;
        request.document.diagnostics[0].node_completeness = NodeCompleteness::Partial;
        request.document.diagnostics[0].phase = Phase::Semantic;
        request.document.diagnostics[0].provenance.source = ProvenanceSource::ResidualText;
        request.document.diagnostics[0].message.raw_text =
            "src/main.cpp:5:7: error: no matching function for call to 'takes(int)'".to_string();
        request.document.diagnostics[0].locations[0].path = "src/main.cpp".to_string();
        request.document.diagnostics[0].locations[0].line = 5;
        request.document.diagnostics[0].locations[0].column = 7;
        let analysis = request.document.diagnostics[0].analysis.as_mut().unwrap();
        analysis.family = Some("type_overload".to_string());
        analysis.headline = Some("type or overload mismatch".to_string());
        analysis.first_action_hint =
            Some("compare the expected type and actual argument at the call site".to_string());
        analysis.confidence = Some(diag_core::Confidence::Low);
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
            locations: vec![Location {
                path: "src/main.cpp".to_string(),
                line: 2,
                column: 6,
                end_line: None,
                end_column: None,
                display_path: None,
                ownership: Some(Ownership::User),
            }],
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
    fn ci_render_sanitizes_transient_object_paths() {
        let mut request = sample_request();
        request.profile = RenderProfile::Ci;
        request.document.document_completeness = DocumentCompleteness::Partial;
        request.document.diagnostics[0].phase = Phase::Link;
        request.document.diagnostics[0].node_completeness = NodeCompleteness::Partial;
        request.document.diagnostics[0].locations.clear();
        request.document.diagnostics[0].message.raw_text =
            "helper.c:(.text+0x0): multiple definition of `duplicate'; /tmp/ccnwX900.o:main.c:(.text+0x0): first defined here".to_string();
        request.document.diagnostics[0].analysis = Some(AnalysisOverlay {
            family: Some("linker.multiple_definition".to_string()),
            headline: Some("multiple definition of `duplicate`".to_string()),
            first_action_hint: Some(
                "remove the duplicate definition or make the symbol internal to one translation unit"
                    .to_string(),
            ),
            confidence: Some(diag_core::Confidence::High),
            rule_id: Some("rule.family.linker.multiple_definition".to_string()),
            matched_conditions: vec!["symbol_context=present".to_string()],
            suppression_reason: None,
            collapsed_child_ids: Vec::new(),
            collapsed_chain_ids: Vec::new(),
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
        analysis.confidence = Some(diag_core::Confidence::Low);
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
        system_note.locations = vec![Location {
            path: "/usr/include/vector".to_string(),
            line: 18,
            column: 7,
            end_line: None,
            end_column: None,
            display_path: None,
            ownership: Some(Ownership::System),
        }];
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
            Location {
                path: "/usr/include/vector".to_string(),
                line: 19,
                column: 3,
                end_line: None,
                end_column: None,
                display_path: None,
                ownership: Some(Ownership::System),
            },
            Location {
                path: "src/main.cpp".to_string(),
                line: 21,
                column: 9,
                end_line: None,
                end_column: None,
                display_path: None,
                ownership: Some(Ownership::User),
            },
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
        analysis.confidence = Some(diag_core::Confidence::Low);
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
            locations: vec![Location {
                path: "src/main.cpp".to_string(),
                line: 2,
                column: 6,
                end_line: None,
                end_column: None,
                display_path: None,
                ownership: Some(Ownership::User),
            }],
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
        request.document.diagnostics[0].message.raw_text =
            "\u{001b}[31mraw compiler stderr".to_string();

        let output = render(request).unwrap();

        assert!(output.used_fallback);
        assert!(!output.text.contains('\u{001b}'));
        assert!(output.text.contains("\\x1b[31mraw compiler stderr"));
    }
}
