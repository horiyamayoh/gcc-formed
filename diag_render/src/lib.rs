mod excerpt;
mod fallback;
mod family;
mod formatter;
mod selector;
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
    use crate::family::summarize_context;
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
            "note: wrapper confidence is low; original compiler wording is preserved below"
        ));
        assert!(
            !output
                .text
                .contains("help: start from the first user-owned template frame")
        );
    }

    #[test]
    fn summarize_context_deduplicates_repeated_macro_frames() {
        let mut request = sample_request();
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

        let lines = summarize_context(&request.document.diagnostics[0], request.profile);
        assert_eq!(lines.len(), 1);
        assert_eq!(
            lines[0],
            "macro: in expansion of macro 'READ_FIELD' @ src/main.c"
        );
    }
}
