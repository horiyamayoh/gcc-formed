//! Diagnostic rendering engine for gcc-formed.
//!
//! Converts a [`DiagnosticDocument`] into formatted, themed text output suitable
//! for terminal display, CI logs, or pipe consumption.
//!
//! Key types:
//! - [`RenderRequest`] -- input bundle carrying the document, profile, and capabilities.
//! - [`RenderResult`] -- output bundle with the rendered text and metadata.
//! - [`RenderProfile`] -- verbosity/layout preset (default, concise, verbose, debug, CI, raw).
//! - [`RenderViewModel`] -- structured intermediate representation used by the formatter.

mod budget;
mod excerpt;
mod fallback;
mod family;
mod formatter;
mod layout;
mod path;
mod presentation;
mod selector;
mod suggestion;
mod theme;
mod view_model;

use diag_core::{
    CascadePolicySnapshot, DiagnosticDocument, DocumentCompleteness, FallbackReason, IntegrityIssue,
};
use serde::{Deserialize, Serialize};

/// A single source-code excerpt block attached to a diagnostic card.
pub use excerpt::ExcerptBlock;
/// Re-exported presentation policy and semantic slot types.
pub use presentation::{
    LocationPlacement, ResolvedCardPresentation, ResolvedFamilyPresentation,
    ResolvedLocationPolicy, ResolvedPresentationPolicy, ResolvedTemplate, ResolvedTemplateLine,
    SemanticSlotId, SessionMode,
};
/// Selects and ranks diagnostic groups for rendering.
pub use selector::{select_groups, select_groups_with_presentation_policy};
/// Re-exported view-model types used to inspect the rendering intermediate representation.
pub use view_model::{
    RenderActionItem, RenderGroupCard, RenderSessionSummary, RenderViewModel, SummaryOnlyGroup,
};

/// Controls the verbosity and layout preset for diagnostic rendering.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RenderProfile {
    /// Balanced output for interactive terminal use.
    Default,
    /// Minimal output focused on the primary error.
    Concise,
    /// Expanded output including all groups and context frames.
    Verbose,
    /// Maximum detail with rule explainability metadata.
    Debug,
    /// Machine-friendly, path-first layout for CI log parsers.
    Ci,
    /// Bypasses analysis entirely; emits preserved raw compiler output.
    RawFallback,
}

/// Describes the kind of output stream the renderer is targeting.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StreamKind {
    /// Interactive terminal (supports color, width detection).
    Tty,
    /// Piped to another process.
    Pipe,
    /// Redirected to a file.
    File,
    /// CI/CD log capture.
    CiLog,
}

/// Controls how file paths are displayed in rendered output.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PathPolicy {
    /// Use the shortest suffix that uniquely identifies the file.
    ShortestUnambiguous,
    /// Display paths relative to the current working directory.
    RelativeToCwd,
    /// Always display absolute paths.
    Absolute,
}

/// Controls whether warnings are shown alongside errors.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WarningVisibility {
    /// Let the render profile decide based on failure context.
    Auto,
    /// Always show all warnings.
    ShowAll,
    /// Suppress all warnings.
    SuppressAll,
}

/// Controls which debug reference identifiers are appended to output.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DebugRefs {
    /// Do not append any debug references.
    None,
    /// Append the invocation trace ID.
    TraceId,
    /// Append capture artifact identifiers.
    CaptureRef,
}

/// Controls how C++ template and type names are displayed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TypeDisplayPolicy {
    /// Show the full, unabbreviated type name.
    Full,
    /// Use a compact representation when safe to do so.
    CompactSafe,
    /// Prefer the raw compiler representation.
    RawFirst,
}

/// Controls whether source code excerpts are included in the output.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceExcerptPolicy {
    /// Let the profile and budget decide.
    Auto,
    /// Always include source excerpts when source files are available.
    ForceOn,
    /// Never include source excerpts.
    ForceOff,
}

/// Input bundle for the diagnostic renderer.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RenderRequest {
    /// The diagnostic document to render.
    pub document: DiagnosticDocument,
    /// Resolved cascade policy shared with document analysis.
    pub cascade_policy: CascadePolicySnapshot,
    /// Verbosity and layout preset.
    pub profile: RenderProfile,
    /// Terminal and stream capabilities of the target output.
    pub capabilities: RenderCapabilities,
    /// Current working directory used for path resolution.
    pub cwd: Option<std::path::PathBuf>,
    /// How file paths should be displayed.
    pub path_policy: PathPolicy,
    /// Whether warnings are shown when errors are present.
    pub warning_visibility: WarningVisibility,
    /// Which debug reference identifiers to append.
    pub debug_refs: DebugRefs,
    /// How C++ type names are displayed.
    pub type_display_policy: TypeDisplayPolicy,
    /// Whether source code excerpts are included.
    pub source_excerpt_policy: SourceExcerptPolicy,
}

/// Describes the capabilities and constraints of the target output stream.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RenderCapabilities {
    /// The kind of output stream (TTY, pipe, file, CI log).
    pub stream_kind: StreamKind,
    /// Terminal width in columns, if known.
    pub width_columns: Option<usize>,
    /// Whether ANSI color escape sequences are supported.
    pub ansi_color: bool,
    /// Whether Unicode box-drawing and symbols are supported.
    pub unicode: bool,
    /// Whether terminal hyperlinks (OSC 8) are supported.
    pub hyperlinks: bool,
    /// Whether the session is interactive (e.g. user can scroll).
    pub interactive: bool,
}

/// Output bundle produced by the diagnostic renderer.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RenderResult {
    /// The final rendered text ready for display.
    pub text: String,
    /// Whether analysis overlays contributed to the output.
    pub used_analysis: bool,
    /// Whether the output fell back to raw compiler output.
    pub used_fallback: bool,
    /// The reason a fallback was triggered, if applicable.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fallback_reason: Option<FallbackReason>,
    /// Group reference IDs that were fully rendered.
    pub displayed_group_refs: Vec<String>,
    /// Number of groups shown only as summary lines.
    pub suppressed_group_count: usize,
    /// Number of warnings suppressed due to a co-occurring failure.
    pub suppressed_warning_count: usize,
    /// Whether the output was truncated to fit the screen budget.
    pub truncation_occurred: bool,
    /// Integrity issues encountered during rendering.
    pub render_issues: Vec<IntegrityIssue>,
}

/// Errors that can occur during diagnostic rendering.
#[derive(Debug, thiserror::Error)]
pub enum RenderError {
    /// The render pipeline encountered an unrecoverable failure.
    #[error("render failed")]
    Failed,
}

/// Renders a [`DiagnosticDocument`] into formatted text.
///
/// Selects the appropriate rendering path based on the request profile and
/// document completeness, falling back to raw output when necessary.
pub fn render(request: RenderRequest) -> Result<RenderResult, RenderError> {
    let presentation_policy = ResolvedPresentationPolicy::legacy_v1();
    render_with_presentation_policy(request, &presentation_policy)
}

/// Renders a [`DiagnosticDocument`] using an explicit resolved presentation policy.
pub fn render_with_presentation_policy(
    request: RenderRequest,
    presentation_policy: &ResolvedPresentationPolicy,
) -> Result<RenderResult, RenderError> {
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

    let selected = selector::select_groups_with_presentation_policy(&request, presentation_policy);
    if selected.cards.is_empty() {
        return Ok(fallback::render_fallback(
            &request,
            FallbackReason::RendererLowConfidence,
        ));
    }
    let view_model = view_model::build(
        &request,
        selected.cards,
        selected.summary_only_cards,
        selected.collapsed_notices_by_group_ref,
        presentation_policy,
    );
    Ok(formatter::emit(
        &request,
        view_model,
        selected.hidden_group_count,
        selected.suppressed_warning_count,
    ))
}

/// Builds the intermediate [`RenderViewModel`] without emitting text.
///
/// Returns `None` when the document would trigger a fallback path (raw profile,
/// passthrough/failed completeness, or empty selection).
pub fn build_view_model(request: &RenderRequest) -> Option<RenderViewModel> {
    let presentation_policy = ResolvedPresentationPolicy::legacy_v1();
    build_view_model_with_presentation_policy(request, &presentation_policy)
}

/// Builds the intermediate [`RenderViewModel`] using an explicit resolved presentation policy.
pub fn build_view_model_with_presentation_policy(
    request: &RenderRequest,
    presentation_policy: &ResolvedPresentationPolicy,
) -> Option<RenderViewModel> {
    if matches!(request.profile, RenderProfile::RawFallback)
        || matches!(
            request.document.document_completeness,
            DocumentCompleteness::Passthrough | DocumentCompleteness::Failed
        )
    {
        return None;
    }
    let selected = selector::select_groups_with_presentation_policy(request, presentation_policy);
    if selected.cards.is_empty() {
        None
    } else {
        Some(view_model::build(
            request,
            selected.cards,
            selected.summary_only_cards,
            selected.collapsed_notices_by_group_ref,
            presentation_policy,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::family::summarize_supporting_evidence;
    use crate::selector::select_groups;
    use diag_core::{
        AnalysisOverlay, ArtifactKind, ArtifactStorage, CaptureArtifact, CompressionLevel,
        ContextChain, ContextChainKind, ContextFrame, DiagnosticDocument, DiagnosticEpisode,
        DocumentAnalysis, DocumentCompleteness, EpisodeGraph, GroupCascadeAnalysis,
        GroupCascadeRole, Location, MessageText, NodeCompleteness, Origin, Ownership, Phase,
        ProducerInfo, Provenance, ProvenanceSource, RunInfo, SemanticRole, Severity, Suggestion,
        SuggestionApplicability, SuppressedCountVisibility, SymbolContext, TextEdit, ToolInfo,
        VisibilityFloor,
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
            family: Some(family.to_string().into()),
            family_version: None,
            family_confidence: None,
            root_cause_score: None,
            actionability_score: None,
            user_code_priority: None,
            headline: Some(headline.to_string().into()),
            first_action_hint: first_action_hint.map(|s| s.to_string().into()),
            confidence: Some(confidence.score()),
            preferred_primary_location_id: None,
            rule_id: Some(rule_id.to_string().into()),
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

    fn sample_suggestion(
        label: &str,
        applicability: SuggestionApplicability,
        edits: Vec<TextEdit>,
    ) -> Suggestion {
        Suggestion {
            label: label.to_string(),
            applicability,
            edits,
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
                        analysis.matched_conditions = vec!["message_contains=expected".into()];
                        analysis
                    }),
                    fingerprints: None,
                }],
                document_analysis: None,
                fingerprints: None,
            },
            cascade_policy: CascadePolicySnapshot::default(),
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

    fn score(value: f32) -> diag_core::Score {
        value.into()
    }

    fn grouped_error_node(
        id: &str,
        group_ref: &str,
        path: &str,
        line: u32,
        message: &str,
    ) -> diag_core::DiagnosticNode {
        let mut node = sample_request().document.diagnostics[0].clone();
        node.id = id.to_string();
        node.message.raw_text = message.to_string();
        node.locations = vec![sample_location(path, line, 1, Ownership::User)];
        let analysis = node.analysis.as_mut().unwrap();
        analysis.family = Some("syntax".into());
        analysis.headline = Some(message.to_string().into());
        analysis.first_action_hint = Some("fix the user-owned source location first".into());
        analysis.rule_id = Some(format!("rule.{id}").into());
        analysis.group_ref = Some(group_ref.to_string());
        analysis.set_confidence_bucket(diag_core::Confidence::High);
        node
    }

    fn episode(
        episode_ref: &str,
        lead_group_ref: &str,
        member_group_refs: Vec<&str>,
        lead_root_score: f32,
    ) -> DiagnosticEpisode {
        DiagnosticEpisode {
            episode_ref: episode_ref.to_string(),
            lead_group_ref: lead_group_ref.to_string(),
            member_group_refs: member_group_refs
                .into_iter()
                .map(ToString::to_string)
                .collect(),
            family: Some("syntax".to_string()),
            lead_root_score: Some(score(lead_root_score)),
            confidence: Some(score(0.9)),
        }
    }

    fn lead_root_group(
        group_ref: &str,
        episode_ref: &str,
        root_score: f32,
        independence_score: f32,
    ) -> GroupCascadeAnalysis {
        GroupCascadeAnalysis {
            group_ref: group_ref.to_string(),
            episode_ref: Some(episode_ref.to_string()),
            role: GroupCascadeRole::LeadRoot,
            best_parent_group_ref: None,
            root_score: Some(score(root_score)),
            independence_score: Some(score(independence_score)),
            suppress_likelihood: Some(score(0.08)),
            summary_likelihood: Some(score(0.14)),
            visibility_floor: VisibilityFloor::NeverHidden,
            evidence_tags: vec!["user_owned_primary".to_string()],
        }
    }

    fn dependent_group(
        group_ref: &str,
        episode_ref: &str,
        parent_group_ref: &str,
        role: GroupCascadeRole,
    ) -> GroupCascadeAnalysis {
        GroupCascadeAnalysis {
            group_ref: group_ref.to_string(),
            episode_ref: Some(episode_ref.to_string()),
            role,
            best_parent_group_ref: Some(parent_group_ref.to_string()),
            root_score: Some(score(0.18)),
            independence_score: Some(score(0.12)),
            suppress_likelihood: Some(score(0.89)),
            summary_likelihood: Some(score(0.76)),
            visibility_floor: VisibilityFloor::HiddenAllowed,
            evidence_tags: vec!["cascade".to_string()],
        }
    }

    fn document_analysis(
        episodes: Vec<DiagnosticEpisode>,
        group_analysis: Vec<GroupCascadeAnalysis>,
    ) -> DocumentAnalysis {
        DocumentAnalysis {
            policy_profile: Some("default-aggressive".to_string()),
            producer_version: Some("test".to_string()),
            episode_graph: EpisodeGraph {
                episodes,
                relations: Vec::new(),
            },
            group_analysis,
            stats: Default::default(),
        }
    }

    #[test]
    fn view_model_builds_applicability_aware_suggestions_with_inline_patch() {
        let tempdir = tempfile::tempdir().unwrap();
        write_source_file(
            &tempdir,
            "src/main.c",
            "int main(void) {\n    return 0\n}\n",
        );

        let mut request = sample_request();
        request.cwd = Some(tempdir.path().to_path_buf());
        request.document.diagnostics[0].suggestions = vec![sample_suggestion(
            "insert ';'",
            SuggestionApplicability::MachineApplicable,
            vec![TextEdit {
                path: "src/main.c".to_string(),
                start_line: 2,
                start_column: 13,
                end_line: 2,
                end_column: 13,
                replacement: ";".to_string(),
            }],
        )];

        let view = build_view_model(&request).unwrap();
        assert_eq!(view.cards[0].suggestions.len(), 1);
        assert_eq!(view.cards[0].suggestions[0].label, "suggested edit");
        assert_eq!(
            view.cards[0].suggestions[0].text,
            "insert ';' at src/main.c:2:13"
        );
        assert_eq!(
            view.cards[0].suggestions[0].inline_patch,
            vec![
                "patch: src/main.c".to_string(),
                "2 -     return 0".to_string(),
                "2 +     return 0;".to_string(),
            ]
        );

        let output = render(request).unwrap();
        assert!(
            output
                .text
                .contains("suggested edit: insert ';' at src/main.c:2:13")
        );
        assert!(output.text.contains("  patch: src/main.c"));
        assert!(output.text.contains("  2 -     return 0"));
        assert!(output.text.contains("  2 +     return 0;"));
    }

    #[test]
    fn render_keeps_summary_only_when_patch_cannot_be_reconstructed() {
        let mut request = sample_request();
        request.document.diagnostics[0].suggestions = vec![sample_suggestion(
            "replace the condition",
            SuggestionApplicability::MaybeIncorrect,
            vec![TextEdit {
                path: "src/missing.c".to_string(),
                start_line: 4,
                start_column: 8,
                end_line: 4,
                end_column: 13,
                replacement: "ready".to_string(),
            }],
        )];

        let view = build_view_model(&request).unwrap();
        assert_eq!(view.cards[0].suggestions.len(), 1);
        assert_eq!(view.cards[0].suggestions[0].label, "likely edit");
        assert_eq!(
            view.cards[0].suggestions[0].text,
            "replace the condition at src/missing.c:4:8-13"
        );
        assert!(view.cards[0].suggestions[0].inline_patch.is_empty());

        let output = render(request).unwrap();
        assert!(
            output
                .text
                .contains("likely edit: replace the condition at src/missing.c:4:8-13")
        );
        assert!(!output.text.contains("patch: src/missing.c"));
    }

    #[test]
    fn render_skips_inline_patch_for_manual_suggestions() {
        let tempdir = tempfile::tempdir().unwrap();
        write_source_file(&tempdir, "src/main.c", "value()\n");

        let mut request = sample_request();
        request.cwd = Some(tempdir.path().to_path_buf());
        request.document.diagnostics[0].suggestions = vec![sample_suggestion(
            "rename the helper for clarity",
            SuggestionApplicability::Manual,
            vec![TextEdit {
                path: "src/main.c".to_string(),
                start_line: 1,
                start_column: 1,
                end_line: 1,
                end_column: 6,
                replacement: "result".to_string(),
            }],
        )];

        let view = build_view_model(&request).unwrap();
        assert_eq!(view.cards[0].suggestions.len(), 1);
        assert_eq!(view.cards[0].suggestions[0].label, "consider");
        assert!(view.cards[0].suggestions[0].inline_patch.is_empty());

        let output = render(request).unwrap();
        assert!(
            output
                .text
                .contains("consider: rename the helper for clarity at src/main.c:1:1-6")
        );
        assert!(!output.text.contains("patch: src/main.c"));
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
    fn explicit_legacy_presentation_policy_matches_default_render() {
        let request = sample_request();
        let default_output = render(request.clone()).unwrap();
        let legacy_policy = ResolvedPresentationPolicy::legacy_v1();

        let explicit_output = render_with_presentation_policy(request, &legacy_policy).unwrap();

        assert_eq!(default_output.text, explicit_output.text);
        assert_eq!(
            default_output.displayed_group_refs,
            explicit_output.displayed_group_refs
        );
    }

    #[test]
    fn subject_blocks_policy_builds_semantic_slot_skeleton() {
        let request = sample_request();
        let subject_blocks = ResolvedPresentationPolicy::subject_blocks_v1();

        let view = build_view_model_with_presentation_policy(&request, &subject_blocks).unwrap();
        let card = &view.cards[0];

        assert_eq!(card.semantic_card.subject, "syntax error");
        assert_eq!(card.semantic_card.presentation.template_id, "parser_block");
        assert_eq!(
            card.semantic_card.slot_text(SemanticSlotId::FirstAction),
            Some("fix the first parser error at the user-owned location")
        );
        assert_eq!(
            card.semantic_card.slot_text(SemanticSlotId::WhyRaw),
            Some("expected ';' before '}' token")
        );
    }

    #[test]
    fn unknown_template_falls_open_to_generic_legacy_adapter() {
        let request = sample_request();
        let mut policy = ResolvedPresentationPolicy::subject_blocks_v1();
        policy.family_mappings = vec![ResolvedFamilyPresentation {
            matcher: "syntax".to_string(),
            display_family: Some("syntax".to_string()),
            template_id: "missing_block".to_string(),
        }];

        let view = build_view_model_with_presentation_policy(&request, &policy).unwrap();
        assert_eq!(
            view.cards[0].semantic_card.presentation.template_id,
            "generic_block"
        );
        assert!(
            view.cards[0]
                .semantic_card
                .presentation
                .fell_back_to_generic_template
        );

        let output = render_with_presentation_policy(request, &policy).unwrap();
        assert!(
            output
                .text
                .contains("help: fix the first parser error at the user-owned location")
        );
        assert!(output.text.contains("why: expected ';' before '}' token"));
    }

    #[test]
    fn missing_slot_data_keeps_generic_path_alive() {
        let mut request = sample_request();
        request.document.diagnostics[0]
            .analysis
            .as_mut()
            .unwrap()
            .first_action_hint = None;
        let subject_blocks = ResolvedPresentationPolicy::subject_blocks_v1();

        let view = build_view_model_with_presentation_policy(&request, &subject_blocks).unwrap();
        assert_eq!(
            view.cards[0]
                .semantic_card
                .slot_text(SemanticSlotId::FirstAction),
            None
        );

        let output = render_with_presentation_policy(request, &subject_blocks).unwrap();
        assert!(!output.text.contains("help:"));
        assert!(output.text.contains("why: expected ';' before '}' token"));
    }

    #[test]
    fn subject_blocks_contrast_render_uses_want_got_via_slots() {
        let mut request = sample_request();
        request.document.diagnostics[0].phase = Phase::Semantic;
        request.document.diagnostics[0].message.raw_text =
            "passing argument 1 of 'takes_int' makes integer from pointer without a cast"
                .to_string();
        request.document.diagnostics[0].locations =
            vec![sample_location("src/main.c", 5, 22, Ownership::User)];
        request.document.diagnostics[0].children = vec![diag_core::DiagnosticNode {
            id: "expected-note".to_string(),
            origin: Origin::Gcc,
            phase: Phase::Semantic,
            severity: Severity::Note,
            semantic_role: SemanticRole::Supporting,
            message: MessageText {
                raw_text: "expected 'int' but argument is of type 'const char *'".to_string(),
                normalized_text: None,
                locale: None,
            },
            locations: vec![sample_location("src/main.c", 1, 19, Ownership::User)],
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
        let analysis = request.document.diagnostics[0].analysis.as_mut().unwrap();
        analysis.family = Some("type_overload".into());
        analysis.headline = Some("type or overload mismatch".into());
        analysis.first_action_hint =
            Some("compare the expected type and actual argument at the call site".into());
        analysis.rule_id = Some("rule.family.type_overload.structured_or_message".into());

        let subject_blocks = ResolvedPresentationPolicy::subject_blocks_v1();
        let view = build_view_model_with_presentation_policy(&request, &subject_blocks).unwrap();
        let card = &view.cards[0];

        assert_eq!(
            card.semantic_card.presentation.template_id,
            "contrast_block"
        );
        assert_eq!(
            card.semantic_card.slot_text(SemanticSlotId::Want),
            Some("int")
        );
        assert_eq!(
            card.semantic_card.slot_text(SemanticSlotId::Got),
            Some("const char *")
        );
        assert_eq!(
            card.semantic_card.slot_text(SemanticSlotId::Via),
            Some("takes_int")
        );
        assert_eq!(card.semantic_card.slot_text(SemanticSlotId::WhyRaw), None);

        let output = render_with_presentation_policy(request, &subject_blocks).unwrap();
        assert!(
            output
                .text
                .contains("error: [type_mismatch] type or overload mismatch @ src/main.c:5:22")
        );
        assert!(output.text.contains("want: int"));
        assert!(output.text.contains("got : const char *"));
        assert!(output.text.contains("via : takes_int"));
        assert!(!output.text.contains("why:"));
    }

    #[test]
    fn subject_blocks_linker_render_uses_symbol_and_from_slots() {
        let mut request = sample_request();
        request.document.diagnostics[0].origin = Origin::Linker;
        request.document.diagnostics[0].phase = Phase::Link;
        request.document.diagnostics[0].locations.clear();
        request.document.diagnostics[0].node_completeness = NodeCompleteness::Partial;
        request.document.diagnostics[0].message.raw_text =
            "helper.c:(.text+0x0): multiple definition of `duplicate'; /tmp/ccnwX900.o:main.c:(.text+0x0): first defined here"
                .to_string();
        request.document.diagnostics[0].symbol_context = Some(SymbolContext {
            primary_symbol: Some("duplicate".to_string()),
            related_objects: vec![
                "obj/vendor.o".to_string(),
                "src/main.o".to_string(),
                "lib/helper.o".to_string(),
            ],
            archive: Some("libfoo.a".to_string()),
        });
        request.document.diagnostics[0].analysis = Some(sample_analysis(
            "linker.multiple_definition",
            "multiple definition of `duplicate`",
            Some(
                "remove the duplicate definition or make the symbol internal to one translation unit",
            ),
            diag_core::Confidence::High,
            "rule.family.linker.multiple_definition",
        ));

        let subject_blocks = ResolvedPresentationPolicy::subject_blocks_v1();
        let view = build_view_model_with_presentation_policy(&request, &subject_blocks).unwrap();
        let card = &view.cards[0];

        assert_eq!(card.semantic_card.presentation.template_id, "linker_block");
        assert_eq!(
            card.semantic_card.slot_text(SemanticSlotId::Symbol),
            Some("duplicate")
        );
        assert_eq!(
            card.semantic_card.slot_text(SemanticSlotId::From),
            Some("lib/helper.o  +2 references")
        );
        assert_eq!(
            card.semantic_card.slot_text(SemanticSlotId::Archive),
            Some("libfoo.a")
        );
        assert_eq!(card.semantic_card.slot_text(SemanticSlotId::WhyRaw), None);

        let output = render_with_presentation_policy(request, &subject_blocks).unwrap();
        assert!(
            output
                .text
                .contains("error: [linker] multiple definition of `duplicate`")
        );
        assert!(output.text.contains("symbol : duplicate"));
        assert!(output.text.contains("from   : lib/helper.o  +2 references"));
        assert!(output.text.contains("archive: libfoo.a"));
        assert!(!output.text.contains("why:"));
    }

    #[test]
    fn subject_blocks_contrast_extraction_failure_falls_back_to_generic_block() {
        let mut request = sample_request();
        request.document.diagnostics[0].severity = Severity::Warning;
        request.document.diagnostics[0].phase = Phase::Semantic;
        request.document.diagnostics[0].message.raw_text =
            "comparison of integer expressions of different signedness: 'int' and 'unsigned int'"
                .to_string();
        let analysis = request.document.diagnostics[0].analysis.as_mut().unwrap();
        analysis.family = Some("conversion_narrowing".into());
        analysis.headline = Some("implicit or narrowing conversion detected".into());
        analysis.first_action_hint =
            Some("add an explicit cast or change the variable type to match".into());
        analysis.rule_id = Some("rule.family.conversion_narrowing.message_terms".into());
        request.document.diagnostics[0].children.clear();

        let subject_blocks = ResolvedPresentationPolicy::subject_blocks_v1();
        let view = build_view_model_with_presentation_policy(&request, &subject_blocks).unwrap();
        let card = &view.cards[0];

        assert_eq!(card.semantic_card.presentation.template_id, "generic_block");
        assert!(
            card.semantic_card
                .presentation
                .fell_back_to_generic_template
        );
        assert_eq!(
            card.semantic_card.slot_text(SemanticSlotId::WhyRaw),
            Some(
                "comparison of integer expressions of different signedness: 'int' and 'unsigned int'"
            )
        );

        let output = render_with_presentation_policy(request, &subject_blocks).unwrap();
        assert!(
            output
                .text
                .contains("implicit or narrowing conversion detected")
        );
        assert!(output.text.contains(
            "why: comparison of integer expressions of different signedness: 'int' and 'unsigned int'"
        ));
        assert!(!output.text.contains("want:"));
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
        assert_eq!(budget.target_lines_per_block, 60);
        assert_eq!(budget.hard_max_lines_per_block, 120);
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
                .block_truncation_notice
                .contains("diagnostic block")
        );
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
    fn passthrough_default_profile_emits_preserved_stderr_without_wrapper_header() {
        let mut request = sample_request();
        request.document.document_completeness = DocumentCompleteness::Passthrough;
        request.document.captures[0].inline_text = Some(
            "src/main.c:2:13: error: original compiler order\nsrc/main.c:2:13: note: prefixed context\n"
                .to_string(),
        );

        let output = render(request).unwrap();

        assert!(output.used_fallback);
        assert_eq!(output.fallback_reason, Some(FallbackReason::ResidualOnly));
        assert_eq!(
            output.text,
            "src/main.c:2:13: error: original compiler order\nsrc/main.c:2:13: note: prefixed context\n"
        );
        assert!(!output.text.contains("showing a conservative wrapper view"));
        assert!(!output.text.contains("fallback reason ="));
        assert!(!output.text.contains("\nraw:\n"));
    }

    #[test]
    fn passthrough_verbose_profiles_keep_fallback_reason_visible() {
        for profile in [RenderProfile::Verbose, RenderProfile::Debug] {
            let mut request = sample_request();
            request.profile = profile;
            request.document.document_completeness = DocumentCompleteness::Passthrough;
            request.document.captures[0].inline_text =
                Some("src/main.c:2:13: error: original compiler order\n".to_string());

            let output = render(request).unwrap();

            assert!(output.used_fallback);
            assert_eq!(output.fallback_reason, Some(FallbackReason::ResidualOnly));
            assert!(output.text.contains("showing a conservative wrapper view"));
            assert!(
                output
                    .text
                    .contains("note: fallback reason = residual_only")
            );
            assert!(
                output
                    .text
                    .contains("raw:\n  src/main.c:2:13: error: original compiler order")
            );
        }
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
    fn relative_to_cwd_path_policy_keeps_full_relative_location_and_excerpt_headers() {
        let tempdir = tempfile::tempdir().unwrap();
        write_source_file(
            &tempdir,
            "src/main.c",
            "int main(void) {\n    return }\n}\n",
        );
        let absolute = tempdir.path().join("src/main.c");
        let absolute = absolute.display().to_string();

        let mut request = sample_request();
        request.cwd = Some(tempdir.path().to_path_buf());
        request.path_policy = PathPolicy::RelativeToCwd;
        request.source_excerpt_policy = SourceExcerptPolicy::ForceOn;
        request.document.diagnostics[0].locations =
            vec![sample_location(&absolute, 2, 12, Ownership::User)];

        let view = build_view_model(&request).unwrap();
        assert_eq!(
            view.cards[0].canonical_location.as_deref(),
            Some("src/main.c:2:12")
        );
        assert_eq!(view.cards[0].excerpts[0].location, "src/main.c:2:12");

        let output = render(request).unwrap();
        assert!(output.text.contains("--> src/main.c:2:12"));
        assert!(output.text.contains("| src/main.c:2:12"));
    }

    #[test]
    fn renderer_prefers_display_path_for_location_and_excerpt_headers() {
        let tempdir = tempfile::tempdir().unwrap();
        write_source_file(
            &tempdir,
            "raw/build/main.c",
            "int main(void) {\n    return }\n}\n",
        );
        let raw = tempdir
            .path()
            .join("raw/build/main.c")
            .display()
            .to_string();

        let mut request = sample_request();
        request.cwd = Some(tempdir.path().to_path_buf());
        request.path_policy = PathPolicy::RelativeToCwd;
        request.source_excerpt_policy = SourceExcerptPolicy::ForceOn;
        request.document.diagnostics[0].locations =
            vec![sample_location(&raw, 2, 12, Ownership::User).with_display_path("src/main.c")];

        let view = build_view_model(&request).unwrap();
        assert_eq!(
            view.cards[0].canonical_location.as_deref(),
            Some("src/main.c:2:12")
        );
        assert_eq!(view.cards[0].excerpts[0].location, "src/main.c:2:12");

        let output = render(request).unwrap();
        assert!(output.text.contains("--> src/main.c:2:12"));
        assert!(output.text.contains("| src/main.c:2:12"));
        assert!(!output.text.contains("raw/build/main.c:2:12"));
    }

    #[test]
    fn shortest_unambiguous_path_policy_shortens_unique_location_and_excerpt_headers() {
        let tempdir = tempfile::tempdir().unwrap();
        write_source_file(
            &tempdir,
            "src/main.c",
            "int main(void) {\n    return }\n}\n",
        );
        let absolute = tempdir.path().join("src/main.c");
        let absolute = absolute.display().to_string();

        let mut request = sample_request();
        request.cwd = Some(tempdir.path().to_path_buf());
        request.path_policy = PathPolicy::ShortestUnambiguous;
        request.source_excerpt_policy = SourceExcerptPolicy::ForceOn;
        request.document.diagnostics[0].locations =
            vec![sample_location(&absolute, 2, 12, Ownership::User)];

        let view = build_view_model(&request).unwrap();
        assert_eq!(
            view.cards[0].canonical_location.as_deref(),
            Some("main.c:2:12")
        );
        assert_eq!(view.cards[0].excerpts[0].location, "main.c:2:12");
    }

    #[test]
    fn shortest_unambiguous_path_policy_uses_display_paths_for_disambiguation() {
        let tempdir = tempfile::tempdir().unwrap();
        write_source_file(
            &tempdir,
            "raw/cache/ab/main.c",
            "int main(void) {\n    return }\n}\n",
        );
        write_source_file(
            &tempdir,
            "raw/cache/cd/main.c",
            "int helper(void) {\n    return 0;\n}\n",
        );
        let primary = tempdir
            .path()
            .join("raw/cache/ab/main.c")
            .display()
            .to_string();
        let competing = tempdir
            .path()
            .join("raw/cache/cd/main.c")
            .display()
            .to_string();

        let mut request = sample_request();
        request.cwd = Some(tempdir.path().to_path_buf());
        request.path_policy = PathPolicy::ShortestUnambiguous;
        request.source_excerpt_policy = SourceExcerptPolicy::ForceOn;
        request.document.diagnostics[0].locations =
            vec![sample_location(&primary, 2, 12, Ownership::User).with_display_path("src/main.c")];
        request
            .document
            .diagnostics
            .push(diag_core::DiagnosticNode {
                id: "secondary".to_string(),
                origin: Origin::Gcc,
                phase: Phase::Semantic,
                severity: Severity::Warning,
                semantic_role: SemanticRole::Root,
                message: MessageText {
                    raw_text: "other main.c warning".to_string(),
                    normalized_text: None,
                    locale: None,
                },
                locations: vec![
                    sample_location(&competing, 2, 5, Ownership::User)
                        .with_display_path("tests/main.c"),
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
                analysis: None,
                fingerprints: None,
            });

        let view = build_view_model(&request).unwrap();
        assert_eq!(
            view.cards[0].canonical_location.as_deref(),
            Some("src/main.c:2:12")
        );
        assert_eq!(view.cards[0].excerpts[0].location, "src/main.c:2:12");
    }

    #[test]
    fn shortest_unambiguous_path_policy_adds_prefix_when_basename_is_ambiguous() {
        let tempdir = tempfile::tempdir().unwrap();
        write_source_file(
            &tempdir,
            "src/main.c",
            "int main(void) {\n    return }\n}\n",
        );
        write_source_file(
            &tempdir,
            "tests/main.c",
            "int helper(void) {\n    return 0;\n}\n",
        );
        let primary = tempdir.path().join("src/main.c").display().to_string();
        let competing = tempdir.path().join("tests/main.c").display().to_string();

        let mut request = sample_request();
        request.cwd = Some(tempdir.path().to_path_buf());
        request.path_policy = PathPolicy::ShortestUnambiguous;
        request.source_excerpt_policy = SourceExcerptPolicy::ForceOn;
        request.document.diagnostics[0].locations =
            vec![sample_location(&primary, 2, 12, Ownership::User)];
        request
            .document
            .diagnostics
            .push(diag_core::DiagnosticNode {
                id: "secondary".to_string(),
                origin: Origin::Gcc,
                phase: Phase::Semantic,
                severity: Severity::Warning,
                semantic_role: SemanticRole::Root,
                message: MessageText {
                    raw_text: "other main.c warning".to_string(),
                    normalized_text: None,
                    locale: None,
                },
                locations: vec![sample_location(&competing, 2, 5, Ownership::User)],
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

        let view = build_view_model(&request).unwrap();
        assert_eq!(
            view.cards[0].canonical_location.as_deref(),
            Some("src/main.c:2:12")
        );
        assert_eq!(view.cards[0].excerpts[0].location, "src/main.c:2:12");
    }

    #[test]
    fn absolute_path_policy_is_honored_in_ci_render_while_ci_layout_stays_path_first() {
        let tempdir = tempfile::tempdir().unwrap();
        write_source_file(
            &tempdir,
            "src/main.c",
            "int main(void) {\n    return }\n}\n",
        );
        let absolute = tempdir.path().join("src/main.c").display().to_string();

        let mut request = sample_request();
        request.cwd = Some(tempdir.path().to_path_buf());
        request.profile = RenderProfile::Ci;
        request.path_policy = PathPolicy::Absolute;
        request.source_excerpt_policy = SourceExcerptPolicy::ForceOn;
        request.document.diagnostics[0].locations =
            vec![sample_location("src/main.c", 2, 12, Ownership::User)];

        let expected_location = format!("{absolute}:2:12");
        let view = build_view_model(&request).unwrap();
        assert_eq!(
            view.cards[0].canonical_location.as_deref(),
            Some(expected_location.as_str())
        );
        assert_eq!(view.cards[0].excerpts[0].location, expected_location);

        let output = render(request).unwrap();
        assert!(output.text.starts_with(&format!("{absolute}:2:12: error:")));
        assert!(output.text.contains(&format!("| {absolute}:2:12")));
        assert!(!output.text.contains(&format!("--> {absolute}:2:12")));
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
    fn force_on_source_excerpt_policy_ignores_profile_excerpt_budget() {
        let tempdir = tempfile::tempdir().unwrap();
        write_source_file(&tempdir, "src/main.c", "one();\ntwo();\nthree();\n");

        let mut request = sample_request();
        request.cwd = Some(tempdir.path().to_path_buf());
        request.profile = RenderProfile::Default;
        request.source_excerpt_policy = SourceExcerptPolicy::ForceOn;
        request.document.diagnostics[0].locations = vec![
            sample_location("src/main.c", 1, 1, Ownership::User),
            sample_location("src/main.c", 2, 1, Ownership::User),
            sample_location("src/main.c", 3, 1, Ownership::User),
        ];

        let view = build_view_model(&request).unwrap();

        assert_eq!(view.cards[0].excerpts.len(), 3);
        assert_eq!(
            view.cards[0]
                .excerpts
                .iter()
                .map(|excerpt| excerpt.location.as_str())
                .collect::<Vec<_>>(),
            vec!["src/main.c:1:1", "src/main.c:2:1", "src/main.c:3:1"]
        );
    }

    #[test]
    fn excerpt_budget_skips_unreadable_locations_and_uses_next_readable_source() {
        let tempdir = tempfile::tempdir().unwrap();
        write_source_file(&tempdir, "src/readable.c", "ok();\n");

        let mut request = sample_request();
        request.cwd = Some(tempdir.path().to_path_buf());
        request.profile = RenderProfile::Ci;
        request.source_excerpt_policy = SourceExcerptPolicy::Auto;
        request.document.diagnostics[0].locations = vec![
            sample_location("src/missing.c", 1, 1, Ownership::User),
            sample_location("src/readable.c", 1, 1, Ownership::User),
        ];

        let view = build_view_model(&request).unwrap();

        assert_eq!(view.cards[0].excerpts.len(), 1);
        assert_eq!(view.cards[0].excerpts[0].location, "src/readable.c:1:1");
        assert_eq!(view.cards[0].excerpts[0].lines, vec!["ok();"]);
    }

    #[test]
    fn excerpt_uses_source_snippet_capture_when_source_file_is_missing() {
        let mut request = sample_request();
        request.source_excerpt_policy = SourceExcerptPolicy::ForceOn;
        request.document.captures.push(CaptureArtifact {
            id: "snippet-1".to_string(),
            kind: ArtifactKind::SourceSnippet,
            media_type: "text/plain".to_string(),
            encoding: Some("utf-8".to_string()),
            digest_sha256: None,
            size_bytes: Some(12),
            storage: ArtifactStorage::Inline,
            inline_text: Some("captured();\n".to_string()),
            external_ref: None,
            produced_by: None,
        });
        let mut location = sample_location("src/missing.c", 42, 5, Ownership::User);
        location.source_excerpt_ref = Some("snippet-1".to_string());
        request.document.diagnostics[0].locations = vec![location];

        let view = build_view_model(&request).unwrap();

        assert_eq!(view.cards[0].excerpts.len(), 1);
        assert_eq!(view.cards[0].excerpts[0].location, "src/missing.c:42:5");
        assert_eq!(view.cards[0].excerpts[0].lines, vec!["captured();"]);
        assert_eq!(
            view.cards[0].excerpts[0].annotations,
            vec![format!("{}^", " ".repeat(4))]
        );
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
                        vec!["message_contains=invalid conversion".into()];
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
                        "message_contains=expected".into(),
                        "message_contains=before".into(),
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
            .family = Some("syntax".into());

        let mut opaque = request.document.diagnostics[0].clone();
        opaque.id = "a-opaque".to_string();
        opaque.message.raw_text = "opaque compatibility residual".to_string();
        let analysis = opaque.analysis.as_mut().unwrap();
        analysis.family = Some("compiler.residual".into());
        analysis.headline = Some("opaque compatibility residual".into());
        analysis.rule_id = Some("rule.residual.compiler_unknown".into());

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
                    analysis.matched_conditions = vec!["semantic_role=root".into()];
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
    fn legacy_preset_keeps_lead_plus_summary_for_multi_error_failures() {
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

        let legacy_policy = ResolvedPresentationPolicy::legacy_v1();
        let selection = select_groups_with_presentation_policy(&request, &legacy_policy);

        assert_eq!(selection.cards.len(), 1);
        assert_eq!(selection.summary_only_cards.len(), 2);

        let output = render_with_presentation_policy(request, &legacy_policy).unwrap();
        assert_eq!(output.displayed_group_refs, vec!["root".to_string()]);
        assert!(output.text.contains("other errors:"));
    }

    #[test]
    fn subject_blocks_failure_run_emits_all_visible_roots_as_blocks() {
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

        let subject_blocks = ResolvedPresentationPolicy::subject_blocks_v1();
        let selection = select_groups_with_presentation_policy(&request, &subject_blocks);

        assert_eq!(selection.cards.len(), 3);
        assert!(selection.summary_only_cards.is_empty());
        assert_eq!(selection.cards[0].id, "root");
        assert_eq!(selection.cards[1].id, "secondary");
        assert_eq!(selection.cards[2].id, "tertiary");

        let output = render_with_presentation_policy(request, &subject_blocks).unwrap();
        assert_eq!(
            output.displayed_group_refs,
            vec![
                "root".to_string(),
                "secondary".to_string(),
                "tertiary".to_string()
            ]
        );
        assert!(!output.text.contains("other errors:"));
        assert!(
            output
                .text
                .contains("\n\nerror: [unknown] secondary failure @ src/other.c:9:4")
        );
        assert!(
            output
                .text
                .contains("error: [unknown] tertiary failure @ src/third.c:12:7")
        );
    }

    #[test]
    fn episode_first_selection_keeps_overflow_roots_visible_and_counts_hidden_dependents() {
        let mut request = sample_request();
        request.document.diagnostics = vec![
            grouped_error_node("root-a", "group-a", "src/main.c", 2, "primary failure"),
            grouped_error_node("root-b", "group-b", "src/other.c", 5, "secondary failure"),
            grouped_error_node("root-c", "group-c", "src/third.c", 9, "tertiary failure"),
            grouped_error_node(
                "tail-c",
                "group-c-tail",
                "src/third.c",
                10,
                "parser tail failure",
            ),
        ];
        request.document.document_analysis = Some(document_analysis(
            vec![
                episode("episode-a", "group-a", vec!["group-a"], 0.96),
                episode("episode-b", "group-b", vec!["group-b"], 0.93),
                episode(
                    "episode-c",
                    "group-c",
                    vec!["group-c", "group-c-tail"],
                    0.88,
                ),
            ],
            vec![
                lead_root_group("group-a", "episode-a", 0.96, 0.91),
                lead_root_group("group-b", "episode-b", 0.93, 0.88),
                lead_root_group("group-c", "episode-c", 0.88, 0.83),
                dependent_group(
                    "group-c-tail",
                    "episode-c",
                    "group-c",
                    GroupCascadeRole::FollowOn,
                ),
            ],
        ));

        let selection = select_groups(&request);
        assert_eq!(selection.cards.len(), 2);
        assert_eq!(selection.cards[0].id, "root-a");
        assert_eq!(selection.cards[1].id, "root-b");
        assert_eq!(selection.summary_only_cards.len(), 1);
        assert_eq!(selection.summary_only_cards[0].id, "root-c");
        assert_eq!(selection.hidden_group_count, 1);

        let output = render(request).unwrap();
        assert_eq!(
            output.displayed_group_refs,
            vec!["group-a".to_string(), "group-b".to_string()]
        );
        assert_eq!(output.suppressed_group_count, 2);
        assert!(output.text.contains("other errors:"));
        assert!(
            output
                .text
                .contains("  - src/third.c:9:1: error: tertiary failure")
        );
        assert!(
            output
                .text
                .contains("note: omitted 1 related diagnostic(s) already covered by visible roots")
        );
        assert!(!output.text.contains("parser tail failure"));
    }

    #[test]
    fn subject_blocks_episode_selection_keeps_all_visible_roots_as_blocks() {
        let mut request = sample_request();
        request.document.diagnostics = vec![
            grouped_error_node("root-a", "group-a", "src/main.c", 2, "primary failure"),
            grouped_error_node("root-b", "group-b", "src/other.c", 5, "secondary failure"),
            grouped_error_node("root-c", "group-c", "src/third.c", 9, "tertiary failure"),
            grouped_error_node(
                "tail-c",
                "group-c-tail",
                "src/third.c",
                10,
                "parser tail failure",
            ),
        ];
        request.document.document_analysis = Some(document_analysis(
            vec![
                episode("episode-a", "group-a", vec!["group-a"], 0.96),
                episode("episode-b", "group-b", vec!["group-b"], 0.93),
                episode(
                    "episode-c",
                    "group-c",
                    vec!["group-c", "group-c-tail"],
                    0.88,
                ),
            ],
            vec![
                lead_root_group("group-a", "episode-a", 0.96, 0.91),
                lead_root_group("group-b", "episode-b", 0.93, 0.88),
                lead_root_group("group-c", "episode-c", 0.88, 0.83),
                dependent_group(
                    "group-c-tail",
                    "episode-c",
                    "group-c",
                    GroupCascadeRole::FollowOn,
                ),
            ],
        ));

        let subject_blocks = ResolvedPresentationPolicy::subject_blocks_v1();
        let selection = select_groups_with_presentation_policy(&request, &subject_blocks);
        assert_eq!(selection.cards.len(), 3);
        assert!(selection.summary_only_cards.is_empty());
        assert_eq!(selection.hidden_group_count, 0);

        let output = render_with_presentation_policy(request, &subject_blocks).unwrap();
        assert_eq!(
            output.displayed_group_refs,
            vec![
                "group-a".to_string(),
                "group-b".to_string(),
                "group-c".to_string()
            ]
        );
        assert_eq!(output.suppressed_group_count, 0);
        assert!(!output.text.contains("other errors:"));
        assert!(output.text.contains("primary failure"));
        assert!(output.text.contains("secondary failure"));
        assert!(output.text.contains("tertiary failure"));
        assert!(
            output
                .text
                .contains("note: omitted 1 follow-on diagnostic(s)")
        );
        assert!(!output.text.contains("parser tail failure"));
    }

    #[test]
    fn episode_first_selection_prefers_higher_severity_for_same_family_same_anchor() {
        let mut request = sample_request();
        request.profile = RenderProfile::Verbose;

        let mut warning = grouped_error_node(
            "shared-warning",
            "group-warning",
            "src/main.c",
            3,
            "asm operand probably does not match constraints",
        );
        warning.severity = Severity::Warning;
        warning.locations = vec![sample_location("src/main.c", 3, 5, Ownership::User)];
        let warning_analysis = warning.analysis.as_mut().unwrap();
        warning_analysis.family = Some("asm_inline".into());
        warning_analysis.headline = Some("inline assembly constraint warning".into());

        let mut error = grouped_error_node(
            "shared-error",
            "group-error",
            "src/main.c",
            3,
            "impossible constraint in 'asm'",
        );
        error.locations = vec![sample_location("src/main.c", 3, 5, Ownership::User)];
        let error_analysis = error.analysis.as_mut().unwrap();
        error_analysis.family = Some("asm_inline".into());
        error_analysis.headline = Some("inline assembly constraint error".into());

        request.document.diagnostics = vec![warning, error];
        request.document.document_analysis = Some(document_analysis(
            vec![
                episode(
                    "episode-warning",
                    "group-warning",
                    vec!["group-warning"],
                    0.96,
                ),
                episode("episode-error", "group-error", vec!["group-error"], 0.91),
            ],
            vec![
                lead_root_group("group-warning", "episode-warning", 0.96, 0.88),
                lead_root_group("group-error", "episode-error", 0.91, 0.84),
            ],
        ));

        let selection = select_groups(&request);
        let view_model = build_view_model(&request).unwrap();

        assert_eq!(selection.cards.len(), 2);
        assert_eq!(selection.cards[0].id, "shared-error");
        assert_eq!(selection.cards[1].id, "shared-warning");
        assert_eq!(view_model.cards.len(), 2);
        assert_eq!(view_model.cards[0].group_id, "group-error");
        assert_eq!(view_model.cards[0].severity, "error");
        assert_eq!(
            view_model.cards[0].canonical_location.as_deref(),
            Some("src/main.c:3:5")
        );
        assert_eq!(view_model.cards[1].group_id, "group-warning");
        assert_eq!(view_model.cards[1].severity, "warning");
        assert_eq!(
            view_model.cards[1].canonical_location.as_deref(),
            Some("src/main.c:3:5")
        );
    }

    #[test]
    fn episode_first_render_collapses_follow_on_and_duplicates_into_lead_notice() {
        let mut request = sample_request();
        request.document.diagnostics = vec![
            grouped_error_node("root", "group-root", "src/main.c", 2, "primary failure"),
            grouped_error_node(
                "follow-on",
                "group-follow",
                "src/main.c",
                3,
                "follow-on parse failure",
            ),
            grouped_error_node(
                "duplicate",
                "group-duplicate",
                "src/main.c",
                4,
                "duplicate parse failure",
            ),
        ];
        request.document.document_analysis = Some(document_analysis(
            vec![episode(
                "episode-root",
                "group-root",
                vec!["group-root", "group-follow", "group-duplicate"],
                0.97,
            )],
            vec![
                lead_root_group("group-root", "episode-root", 0.97, 0.94),
                dependent_group(
                    "group-follow",
                    "episode-root",
                    "group-root",
                    GroupCascadeRole::FollowOn,
                ),
                dependent_group(
                    "group-duplicate",
                    "episode-root",
                    "group-root",
                    GroupCascadeRole::Duplicate,
                ),
            ],
        ));

        let selection = select_groups(&request);
        assert_eq!(selection.cards.len(), 1);
        assert!(selection.summary_only_cards.is_empty());
        assert_eq!(selection.hidden_group_count, 0);
        assert_eq!(
            selection
                .collapsed_notices_by_group_ref
                .get("group-root")
                .cloned()
                .unwrap(),
            vec![
                "omitted 1 follow-on diagnostic(s)".to_string(),
                "omitted 1 duplicate diagnostic(s)".to_string(),
            ]
        );

        let output = render(request).unwrap();
        assert_eq!(output.displayed_group_refs, vec!["group-root".to_string()]);
        assert_eq!(output.suppressed_group_count, 0);
        assert!(!output.text.contains("other errors:"));
        assert!(
            output
                .text
                .contains("note: omitted 1 follow-on diagnostic(s)")
        );
        assert!(
            output
                .text
                .contains("note: omitted 1 duplicate diagnostic(s)")
        );
        assert!(!output.text.contains("follow-on parse failure"));
        assert!(!output.text.contains("duplicate parse failure"));
    }

    #[test]
    fn renderer_fail_opens_to_legacy_selection_when_episode_graph_is_absent() {
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
        request.document.document_analysis = Some(DocumentAnalysis::default());

        let selection = select_groups(&request);
        assert_eq!(selection.cards.len(), 1);
        assert_eq!(selection.cards[0].id, "root");
        assert_eq!(selection.summary_only_cards.len(), 2);
        assert_eq!(selection.summary_only_cards[0].id, "secondary");
        assert_eq!(selection.summary_only_cards[1].id, "tertiary");

        let output = render(request).unwrap();
        assert_eq!(output.displayed_group_refs, vec!["root".to_string()]);
        assert_eq!(output.suppressed_group_count, 2);
        assert!(output.text.contains("other errors:"));
    }

    #[test]
    fn cascade_policy_max_expanded_roots_controls_episode_budget() {
        let mut request = sample_request();
        request.cascade_policy.max_expanded_independent_roots = 1;
        request.document.diagnostics = vec![
            grouped_error_node("root-a", "group-a", "src/main.c", 2, "primary failure"),
            grouped_error_node("root-b", "group-b", "src/other.c", 5, "secondary failure"),
            grouped_error_node("root-c", "group-c", "src/third.c", 9, "tertiary failure"),
        ];
        request.document.document_analysis = Some(document_analysis(
            vec![
                episode("episode-a", "group-a", vec!["group-a"], 0.96),
                episode("episode-b", "group-b", vec!["group-b"], 0.93),
                episode("episode-c", "group-c", vec!["group-c"], 0.88),
            ],
            vec![
                lead_root_group("group-a", "episode-a", 0.96, 0.91),
                lead_root_group("group-b", "episode-b", 0.93, 0.88),
                lead_root_group("group-c", "episode-c", 0.88, 0.83),
            ],
        ));

        let selection = select_groups(&request);
        assert_eq!(selection.cards.len(), 1);
        assert_eq!(selection.cards[0].id, "root-a");
        assert_eq!(selection.summary_only_cards.len(), 2);
        assert_eq!(selection.summary_only_cards[0].id, "root-b");
        assert_eq!(selection.summary_only_cards[1].id, "root-c");
    }

    #[test]
    fn cascade_level_off_keeps_episode_members_out_of_hidden_suppression() {
        let mut request = sample_request();
        request.cascade_policy.compression_level = CompressionLevel::Off;
        request.document.diagnostics = vec![
            grouped_error_node("root", "group-root", "src/main.c", 2, "primary failure"),
            grouped_error_node(
                "follow-on",
                "group-follow",
                "src/main.c",
                3,
                "follow-on parse failure",
            ),
            grouped_error_node(
                "duplicate",
                "group-duplicate",
                "src/main.c",
                4,
                "duplicate parse failure",
            ),
        ];
        request.document.document_analysis = Some(document_analysis(
            vec![episode(
                "episode-root",
                "group-root",
                vec!["group-root", "group-follow", "group-duplicate"],
                0.97,
            )],
            vec![
                lead_root_group("group-root", "episode-root", 0.97, 0.94),
                dependent_group(
                    "group-follow",
                    "episode-root",
                    "group-root",
                    GroupCascadeRole::FollowOn,
                ),
                dependent_group(
                    "group-duplicate",
                    "episode-root",
                    "group-root",
                    GroupCascadeRole::Duplicate,
                ),
            ],
        ));

        let selection = select_groups(&request);
        assert_eq!(selection.cards.len(), 1);
        assert_eq!(selection.summary_only_cards.len(), 2);
        assert_eq!(selection.hidden_group_count, 0);
        assert!(selection.collapsed_notices_by_group_ref.is_empty());

        let output = render(request).unwrap();
        assert_eq!(output.suppressed_group_count, 2);
        assert!(output.text.contains("other errors:"));
        assert!(output.text.contains("follow-on parse failure"));
        assert!(output.text.contains("duplicate parse failure"));
        assert!(!output.text.contains("omitted 1 follow-on diagnostic(s)"));
        assert!(!output.text.contains("omitted 1 duplicate diagnostic(s)"));
    }

    #[test]
    fn suppress_threshold_moves_member_between_hidden_and_summary_only() {
        let diagnostics = vec![
            grouped_error_node("root", "group-root", "src/main.c", 2, "primary failure"),
            grouped_error_node(
                "follow-on",
                "group-follow",
                "src/main.c",
                3,
                "follow-on parse failure",
            ),
        ];
        let document_analysis = document_analysis(
            vec![episode(
                "episode-root",
                "group-root",
                vec!["group-root", "group-follow"],
                0.97,
            )],
            vec![
                lead_root_group("group-root", "episode-root", 0.97, 0.94),
                dependent_group(
                    "group-follow",
                    "episode-root",
                    "group-root",
                    GroupCascadeRole::FollowOn,
                ),
            ],
        );

        let mut hidden_request = sample_request();
        hidden_request.document.diagnostics = diagnostics.clone();
        hidden_request.document.document_analysis = Some(document_analysis.clone());
        hidden_request.cascade_policy.compression_level = CompressionLevel::Aggressive;
        hidden_request.cascade_policy.suppress_likelihood_threshold = 0.78;
        hidden_request.cascade_policy.summary_likelihood_threshold = 0.70;

        let hidden_selection = select_groups(&hidden_request);
        assert_eq!(hidden_selection.summary_only_cards.len(), 0);
        assert_eq!(hidden_selection.hidden_group_count, 0);
        assert_eq!(
            hidden_selection
                .collapsed_notices_by_group_ref
                .get("group-root")
                .cloned()
                .unwrap(),
            vec!["omitted 1 follow-on diagnostic(s)".to_string()]
        );

        let mut summary_request = sample_request();
        summary_request.document.diagnostics = diagnostics;
        summary_request.document.document_analysis = Some(document_analysis);
        summary_request.cascade_policy.compression_level = CompressionLevel::Aggressive;
        summary_request.cascade_policy.suppress_likelihood_threshold = 0.95;
        summary_request.cascade_policy.summary_likelihood_threshold = 0.70;

        let summary_selection = select_groups(&summary_request);
        assert_eq!(summary_selection.summary_only_cards.len(), 1);
        assert_eq!(summary_selection.summary_only_cards[0].id, "follow-on");
        assert_eq!(summary_selection.hidden_group_count, 0);

        let output = render(summary_request).unwrap();
        assert!(output.text.contains("follow-on parse failure"));
        assert!(!output.text.contains("omitted 1 related diagnostic(s)"));
    }

    #[test]
    fn debug_profile_and_suppressed_count_visibility_change_hidden_output() {
        let diagnostics = vec![
            grouped_error_node("root", "group-root", "src/main.c", 2, "primary failure"),
            grouped_error_node(
                "follow-on",
                "group-follow",
                "src/main.c",
                3,
                "follow-on parse failure",
            ),
        ];
        let document_analysis = document_analysis(
            vec![episode(
                "episode-root",
                "group-root",
                vec!["group-root", "group-follow"],
                0.97,
            )],
            vec![
                lead_root_group("group-root", "episode-root", 0.97, 0.94),
                dependent_group(
                    "group-follow",
                    "episode-root",
                    "group-root",
                    GroupCascadeRole::FollowOn,
                ),
            ],
        );

        let mut default_request = sample_request();
        default_request.document.diagnostics = diagnostics.clone();
        default_request.document.document_analysis = Some(document_analysis.clone());
        default_request.cascade_policy.compression_level = CompressionLevel::Aggressive;
        default_request.cascade_policy.show_suppressed_count = SuppressedCountVisibility::Never;

        let default_output = render(default_request).unwrap();
        assert!(
            !default_output
                .text
                .contains("note: omitted 1 related diagnostic(s) already covered by visible roots")
        );
        assert!(!default_output.text.contains("follow-on parse failure"));

        let mut debug_request = sample_request();
        debug_request.profile = RenderProfile::Debug;
        debug_request.document.diagnostics = diagnostics;
        debug_request.document.document_analysis = Some(document_analysis);
        debug_request.cascade_policy.compression_level = CompressionLevel::Aggressive;
        debug_request.cascade_policy.show_suppressed_count = SuppressedCountVisibility::Never;

        let debug_output = render(debug_request).unwrap();
        assert!(debug_output.text.contains("other errors:"));
        assert!(debug_output.text.contains("follow-on parse failure"));
        assert!(
            !debug_output
                .text
                .contains("note: omitted 1 related diagnostic(s) already covered by visible roots")
        );
        assert!(
            debug_output
                .text
                .contains("debug-facts: group_ref=group-follow, role=follow_on, visibility_floor=hidden_allowed, episode_ref=episode-root")
        );
        assert!(
            debug_output
                .text
                .contains("debug-facts: best_parent_group_ref=group-root")
        );
        assert!(
            debug_output
                .text
                .contains("debug-facts: evidence_tags=cascade")
        );
        assert!(
            debug_output
                .text
                .contains("debug-policy: debug keeps this member visible; default profiles may hide it because suppress_likelihood=0.89 meets the current aggressive threshold")
        );
        assert!(
            debug_output
                .text
                .contains("debug-raw: provenance_capture_refs=stderr.raw")
        );
    }

    #[test]
    fn debug_view_model_keeps_cascade_explainability_separate_from_summary_only_facts() {
        let diagnostics = vec![
            grouped_error_node("root", "group-root", "src/main.c", 2, "primary failure"),
            grouped_error_node(
                "follow-on",
                "group-follow",
                "src/main.c",
                3,
                "follow-on parse failure",
            ),
        ];
        let document_analysis = document_analysis(
            vec![episode(
                "episode-root",
                "group-root",
                vec!["group-root", "group-follow"],
                0.97,
            )],
            vec![
                lead_root_group("group-root", "episode-root", 0.97, 0.94),
                dependent_group(
                    "group-follow",
                    "episode-root",
                    "group-root",
                    GroupCascadeRole::FollowOn,
                ),
            ],
        );

        let mut request = sample_request();
        request.profile = RenderProfile::Debug;
        request.document.diagnostics = diagnostics;
        request.document.document_analysis = Some(document_analysis);
        request.cascade_policy.compression_level = CompressionLevel::Aggressive;

        let view_model = build_view_model(&request).unwrap();
        assert_eq!(view_model.cards.len(), 1);
        assert_eq!(view_model.summary_only_groups.len(), 1);

        let lead_debug = view_model.cards[0].cascade_debug.as_ref().unwrap();
        assert_eq!(lead_debug.group_ref, "group-root");
        assert_eq!(lead_debug.cascade_role, "lead_root");
        assert_eq!(
            lead_debug.provenance_capture_refs,
            vec!["stderr.raw".to_string()]
        );
        assert!(lead_debug.suppression_policy.is_none());

        let summary_debug = view_model.summary_only_groups[0]
            .cascade_debug
            .as_ref()
            .unwrap();
        assert_eq!(summary_debug.group_ref, "group-follow");
        assert_eq!(summary_debug.episode_ref.as_deref(), Some("episode-root"));
        assert_eq!(summary_debug.cascade_role, "follow_on");
        assert_eq!(summary_debug.visibility_floor, "hidden_allowed");
        assert_eq!(
            summary_debug.best_parent_group_ref.as_deref(),
            Some("group-root")
        );
        assert_eq!(summary_debug.evidence_tags, vec!["cascade".to_string()]);
        assert_eq!(
            summary_debug.provenance_capture_refs,
            vec!["stderr.raw".to_string()]
        );
        assert!(
            summary_debug
                .suppression_policy
                .as_deref()
                .is_some_and(|policy| policy.contains("default profiles may hide it"))
        );
    }

    #[test]
    fn low_confidence_render_uses_raw_title_and_honesty_notice() {
        let mut request = sample_request();
        request.document.diagnostics[0].message.raw_text =
            "static assertion failed: size must be 4 bytes".to_string();
        let analysis = request.document.diagnostics[0].analysis.as_mut().unwrap();
        analysis.family = Some("unknown".into());
        analysis.headline = Some("template instantiation failed".into());
        analysis.first_action_hint = Some(
            "start from the first user-owned template frame and match template arguments".into(),
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
        analysis.family = Some("unknown".into());
        analysis.headline = Some("type or overload mismatch".into());
        analysis.first_action_hint =
            Some("compare the expected type and actual argument at the call site".into());
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
        analysis.family = Some("unknown".into());
        analysis.headline = Some("type or overload mismatch".into());
        analysis.first_action_hint =
            Some("compare the expected type and actual argument at the call site".into());
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
        analysis.family = Some("type_overload".into());
        analysis.headline = Some("type or overload mismatch".into());
        analysis.first_action_hint =
            Some("compare the expected type and actual argument at the call site".into());
        analysis.set_confidence_bucket(diag_core::Confidence::Low);
        analysis.rule_id = Some("rule.residual.compiler_type_overload".into());
        analysis.matched_conditions = vec!["family=type_overload".into()];

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
            analysis.matched_conditions = vec!["symbol_context=present".into()];
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
            .family = Some("macro_include".into());
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
            .family = Some("template".into());
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
        analysis.family = Some("template".into());
        analysis.set_confidence_bucket(diag_core::Confidence::Low);
        analysis.rule_id = Some("rule.residual.compiler_template".into());
        analysis.matched_conditions = vec!["family=template".into()];
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
            .family = Some("template".into());
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
            .family = Some("type_overload".into());

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
    fn overload_supporting_evidence_uses_display_path_and_strips_display_prefixed_note() {
        let tempdir = tempfile::tempdir().unwrap();
        let raw = tempdir
            .path()
            .join("raw/build/main.cpp")
            .display()
            .to_string();

        let mut request = sample_request();
        request.cwd = Some(tempdir.path().to_path_buf());
        request.path_policy = PathPolicy::RelativeToCwd;
        request.document.diagnostics[0]
            .analysis
            .as_mut()
            .unwrap()
            .family = Some("type_overload".into());

        let mut note = request.document.diagnostics[0].clone();
        note.id = "candidate-remapped".to_string();
        note.message.raw_text =
            "src/main.cpp:21:9: note: candidate conversion matches the call site".to_string();
        note.locations =
            vec![sample_location(&raw, 21, 9, Ownership::User).with_display_path("src/main.cpp")];
        note.children = Vec::new();
        note.suggestions = Vec::new();
        note.context_chains = Vec::new();
        note.symbol_context = None;
        note.analysis = None;
        note.node_completeness = NodeCompleteness::Complete;

        request.document.diagnostics[0].children = vec![note];

        let evidence = summarize_supporting_evidence(&request, &request.document.diagnostics[0]);
        assert_eq!(
            evidence.context_lines,
            vec!["because: candidate conversion matches the call site at src/main.cpp:21:9"]
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
        analysis.family = Some("type_overload".into());
        analysis.set_confidence_bucket(diag_core::Confidence::Low);
        analysis.rule_id = Some("rule.residual.compiler_type_overload".into());
        analysis.matched_conditions = vec!["family=type_overload".into()];

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
            .headline = Some("\u{001b}[31msyntax error".to_string().into());
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
