use crate::RenderRequest;
use crate::budget::render_policy;
use crate::excerpt::load_excerpt;
use crate::family::{is_conservative_useful_subset_card, summarize_supporting_evidence};
use crate::path::format_location;
use crate::presentation::{
    RenderSemanticCard, RenderSemanticSlot, ResolvedPresentationPolicy, SemanticSlotId, SessionMode,
};
use crate::selector::{
    render_group_ref, should_hide_episode_member_for_profile,
    should_materialize_episode_member_as_summary_for_profile,
};
use crate::suggestion::build_action_items;
use diag_core::{
    CompressionLevel, DiagnosticNode, DisclosureConfidence, DocumentCompleteness,
    GroupCascadeAnalysis, GroupCascadeRole, NodeCompleteness, Severity, SuggestionApplicability,
    VisibilityFloor,
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

/// Top-level session summary included in the view model.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RenderSessionSummary {
    /// Category of the session outcome (e.g. `"compile_failure"`, `"warnings_only"`).
    pub failure_kind: String,
    /// Whether a partial-document notice should be shown.
    pub partial_notice: bool,
    /// Optional hint directing the user to raw diagnostic output.
    pub raw_diagnostics_hint: Option<String>,
    /// Internal session mode used for formatter/session behavior.
    #[serde(skip, default = "default_session_mode")]
    pub(crate) session_mode: SessionMode,
}

/// A diagnostic group rendered only as a one-line summary.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SummaryOnlyGroup {
    /// Unique group identifier from the diagnostic node.
    pub group_id: String,
    /// Severity label (e.g. `"error"`, `"warning"`).
    pub severity: String,
    /// Display title for this group.
    pub title: String,
    /// Formatted canonical location, if available.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub canonical_location: Option<String>,
    /// Debug-only cascade explainability for this summary-only group.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cascade_debug: Option<CascadeDebugInfo>,
}

/// A fully expanded diagnostic group card in the view model.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RenderGroupCard {
    /// Unique group identifier from the diagnostic node.
    pub group_id: String,
    /// Severity label (e.g. `"error"`, `"warning"`).
    pub severity: String,
    /// Analysis family name, if one was matched.
    pub family: Option<String>,
    /// Display title (analysis headline or raw message).
    pub title: String,
    /// Confidence bucket label (e.g. `"certain"`, `"likely"`, `"possible"`).
    pub confidence_label: String,
    /// Low-confidence honesty notice, if applicable.
    pub confidence_notice: Option<String>,
    /// Suggested first action from the analysis overlay.
    pub first_action: Option<String>,
    /// Formatted canonical source location.
    pub canonical_location: Option<String>,
    /// Raw compiler message text.
    pub raw_message: String,
    /// Source code excerpt blocks.
    pub excerpts: Vec<crate::ExcerptBlock>,
    /// Context lines from supporting evidence (template, macro, linker chains).
    pub context_lines: Vec<String>,
    /// Child compiler notes.
    pub child_notes: Vec<String>,
    /// Notices about collapsed or omitted content.
    pub collapsed_notices: Vec<String>,
    /// Render-ready suggestions/fix-its for this diagnostic.
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub suggestions: Vec<RenderActionItem>,
    /// Label preceding the raw sub-block.
    #[serde(
        skip_serializing_if = "is_default_raw_block_label",
        default = "default_raw_block_label"
    )]
    pub raw_block_label: String,
    /// Raw compiler message lines shown verbatim for partial nodes.
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub raw_sub_block: Vec<String>,
    /// Matched rule identifier from the analysis overlay.
    pub rule_id: Option<String>,
    /// Matched condition strings from the analysis overlay.
    pub matched_conditions: Vec<String>,
    /// Reason this group was suppressed, if applicable.
    pub suppression_reason: Option<String>,
    /// Debug-only cascade explainability for this group.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cascade_debug: Option<CascadeDebugInfo>,
    /// Internal semantic card kept off the serialized legacy view model for now.
    #[serde(skip)]
    pub(crate) semantic_card: RenderSemanticCard,
}

/// Debug-only cascade explainability attached to rendered groups.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CascadeDebugInfo {
    /// Group reference used by cascade analysis.
    pub group_ref: String,
    /// Episode reference when this group belongs to an episode.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub episode_ref: Option<String>,
    /// Cascade role assigned by document analysis.
    pub cascade_role: String,
    /// Visibility floor assigned by document analysis.
    pub visibility_floor: String,
    /// Best candidate parent, when one exists.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub best_parent_group_ref: Option<String>,
    /// Evidence tags supporting the cascade decision.
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub evidence_tags: Vec<String>,
    /// Raw provenance capture refs that can be opened for this group.
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub provenance_capture_refs: Vec<String>,
    /// Debug-only policy explanation kept separate from the facts above.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub suppression_policy: Option<String>,
}

/// A render-ready suggestion or fix-it item.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RenderActionItem {
    /// Applicability-aware display label.
    pub label: String,
    /// Human-readable summary text.
    pub text: String,
    /// Original applicability from the IR.
    pub applicability: SuggestionApplicability,
    /// Compact inline patch preview when it can be reconstructed safely.
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub inline_patch: Vec<String>,
}

/// The complete intermediate representation consumed by the formatter.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RenderViewModel {
    /// Session-level summary metadata.
    pub summary: RenderSessionSummary,
    /// Fully expanded diagnostic group cards.
    pub cards: Vec<RenderGroupCard>,
    /// Groups shown only as one-line summaries.
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub summary_only_groups: Vec<SummaryOnlyGroup>,
}

/// Builds a [`RenderViewModel`] from the selected diagnostic groups.
pub fn build(
    request: &RenderRequest,
    cards: Vec<DiagnosticNode>,
    summary_only_cards: Vec<DiagnosticNode>,
    collapsed_notices_by_group_ref: BTreeMap<String, Vec<String>>,
    presentation_policy: &ResolvedPresentationPolicy,
) -> RenderViewModel {
    let policy = render_policy(request.profile);
    let selected_cards_include_incomplete = cards.iter().any(|node| {
        !matches!(
            node.node_completeness,
            NodeCompleteness::Complete | NodeCompleteness::Synthesized
        )
    });
    let rendered_cards = cards
        .into_iter()
        .map(|node| {
            build_card(
                request,
                &node,
                &collapsed_notices_by_group_ref,
                presentation_policy,
            )
        })
        .collect::<Vec<_>>();
    let has_failure = rendered_cards
        .iter()
        .any(|card| card.severity == "fatal" || card.severity == "error");
    RenderViewModel {
        summary: RenderSessionSummary {
            failure_kind: if has_failure {
                "compile_failure".to_string()
            } else {
                "warnings_only".to_string()
            },
            partial_notice: !matches!(
                request.document.document_completeness,
                diag_core::DocumentCompleteness::Complete
            ) && selected_cards_include_incomplete,
            raw_diagnostics_hint: request
                .document
                .captures
                .iter()
                .any(|capture| capture.id == "stderr.raw")
                .then_some(policy.disclosure.raw_diagnostics_hint.to_string()),
            session_mode: resolved_session_mode(presentation_policy, has_failure),
        },
        cards: rendered_cards,
        summary_only_groups: summary_only_cards
            .into_iter()
            .map(|node| build_summary_only_group(request, &node))
            .collect(),
    }
}

fn build_card(
    request: &RenderRequest,
    node: &DiagnosticNode,
    collapsed_notices_by_group_ref: &BTreeMap<String, Vec<String>>,
    presentation_policy: &ResolvedPresentationPolicy,
) -> RenderGroupCard {
    let policy = render_policy(request.profile);
    let conservative_useful_subset = is_conservative_useful_subset_card(request, node);
    let confidence = node
        .analysis
        .as_ref()
        .map(|analysis| analysis.disclosure_confidence())
        .unwrap_or(DisclosureConfidence::Hidden);
    let confidence_label = confidence_label(confidence).to_string();
    let title = select_title(node, confidence);
    let first_action = select_first_action(node, confidence);
    let family = node
        .analysis
        .as_ref()
        .and_then(|analysis| analysis.family.as_ref().map(|c| c.to_string()));
    let canonical_location = canonical_location(request, node);
    let excerpts = load_excerpt(request, node);
    let supporting_evidence = summarize_supporting_evidence(request, node);
    let context_lines = supporting_evidence.context_lines;
    let child_notes = supporting_evidence.child_notes;
    let mut collapsed_notices = supporting_evidence.collapsed_notices;
    if let Some(selector_notices) = collapsed_notices_by_group_ref.get(&render_group_ref(node)) {
        collapsed_notices.extend(selector_notices.iter().cloned());
    }
    let suggestions = build_action_items(request, node);
    let confidence_notice = if conservative_useful_subset {
        Some(conservative_band_c_notice().to_string())
    } else {
        confidence
            .requires_low_confidence_notice()
            .then_some(policy.disclosure.low_confidence_notice.to_string())
    };
    let raw_sub_block = raw_sub_block(request, node);
    let semantic_card = build_semantic_card(
        presentation_policy,
        family.as_deref(),
        &title,
        first_action.as_deref(),
        canonical_location.as_deref(),
        &node.message.raw_text,
    );

    RenderGroupCard {
        group_id: render_group_ref(node),
        severity: severity_label(&node.severity).to_string(),
        family,
        title,
        confidence_label,
        confidence_notice,
        first_action,
        canonical_location,
        raw_message: node.message.raw_text.clone(),
        excerpts,
        context_lines,
        child_notes,
        collapsed_notices,
        suggestions,
        raw_block_label: if conservative_useful_subset {
            conservative_raw_block_label().to_string()
        } else {
            policy.disclosure.raw_block_label.to_string()
        },
        raw_sub_block,
        rule_id: node
            .analysis
            .as_ref()
            .and_then(|analysis| analysis.rule_id.as_ref().map(|c| c.to_string())),
        matched_conditions: node
            .analysis
            .as_ref()
            .map(|analysis| {
                analysis
                    .matched_conditions
                    .iter()
                    .map(|c| c.to_string())
                    .collect()
            })
            .unwrap_or_default(),
        suppression_reason: node
            .analysis
            .as_ref()
            .and_then(|analysis| analysis.suppression_reason.clone()),
        cascade_debug: cascade_debug_info(request, node, false),
        semantic_card,
    }
}

fn build_summary_only_group(request: &RenderRequest, node: &DiagnosticNode) -> SummaryOnlyGroup {
    SummaryOnlyGroup {
        group_id: render_group_ref(node),
        severity: severity_label(&node.severity).to_string(),
        title: select_title(
            node,
            node.analysis
                .as_ref()
                .map(|analysis| analysis.disclosure_confidence())
                .unwrap_or(DisclosureConfidence::Hidden),
        ),
        canonical_location: canonical_location(request, node),
        cascade_debug: cascade_debug_info(request, node, true),
    }
}

fn build_semantic_card(
    presentation_policy: &ResolvedPresentationPolicy,
    family: Option<&str>,
    title: &str,
    first_action: Option<&str>,
    canonical_location: Option<&str>,
    raw_message: &str,
) -> RenderSemanticCard {
    let resolved_presentation = presentation_policy.resolve_card_presentation(family);
    let mut slots = Vec::new();
    if let Some(first_action) = first_action {
        slots.push(RenderSemanticSlot {
            slot: SemanticSlotId::FirstAction,
            value: first_action.to_string(),
            label: presentation_policy.label("help").map(str::to_string),
        });
    }
    slots.push(RenderSemanticSlot {
        slot: SemanticSlotId::WhyRaw,
        value: raw_message.to_string(),
        label: presentation_policy
            .slot_label(SemanticSlotId::WhyRaw)
            .map(str::to_string),
    });

    RenderSemanticCard {
        internal_family: family.map(ToString::to_string),
        display_family: resolved_presentation.display_family.clone(),
        subject: title.to_string(),
        presentation: resolved_presentation,
        slots,
        canonical_location: canonical_location.map(ToString::to_string),
        raw_message: raw_message.to_string(),
    }
}

fn canonical_location(request: &RenderRequest, node: &DiagnosticNode) -> Option<String> {
    node.primary_location()
        .map(|location| format_location(request, location))
}

fn cascade_debug_info(
    request: &RenderRequest,
    node: &DiagnosticNode,
    summary_only: bool,
) -> Option<CascadeDebugInfo> {
    if !matches!(request.profile, crate::RenderProfile::Debug) {
        return None;
    }
    let group_ref = render_group_ref(node);
    let group = request
        .document
        .document_analysis
        .as_ref()?
        .group_analysis
        .iter()
        .find(|group| group.group_ref == group_ref)?;

    Some(CascadeDebugInfo {
        group_ref,
        episode_ref: group.episode_ref.clone(),
        cascade_role: cascade_role_label(group.role).to_string(),
        visibility_floor: visibility_floor_label(group.visibility_floor).to_string(),
        best_parent_group_ref: group.best_parent_group_ref.clone(),
        evidence_tags: group.evidence_tags.clone(),
        provenance_capture_refs: provenance_capture_refs(node),
        suppression_policy: summary_only.then(|| suppression_policy_for_debug(request, group)),
    })
}

fn provenance_capture_refs(node: &DiagnosticNode) -> Vec<String> {
    let mut refs = BTreeSet::new();
    refs.extend(node.provenance.capture_refs.iter().cloned());
    for location in &node.locations {
        if let Some(provenance) = location.provenance_override.as_ref() {
            refs.extend(provenance.capture_refs.iter().cloned());
        }
    }
    refs.into_iter().collect()
}

fn suppression_policy_for_debug(request: &RenderRequest, group: &GroupCascadeAnalysis) -> String {
    if group.visibility_floor != VisibilityFloor::HiddenAllowed {
        return format!(
            "debug keeps this member visible; visibility_floor={} prevents full hiding",
            visibility_floor_label(group.visibility_floor)
        );
    }

    if request.cascade_policy.compression_level == CompressionLevel::Off {
        return "debug keeps this member visible; compression_level=off disables hidden suppression"
            .to_string();
    }

    if should_hide_episode_member_for_profile(
        crate::RenderProfile::Default,
        &request.cascade_policy,
        group,
    ) {
        let suppress_likelihood = format_optional_score(group.suppress_likelihood);
        return format!(
            "debug keeps this member visible; default profiles may hide it because suppress_likelihood={suppress_likelihood} meets the current {} threshold",
            compression_level_label(request.cascade_policy.compression_level)
        );
    }

    if should_materialize_episode_member_as_summary_for_profile(
        crate::RenderProfile::Default,
        &request.cascade_policy,
        group,
    ) {
        let summary_likelihood = format_optional_score(group.summary_likelihood);
        return format!(
            "debug keeps this member visible; default profiles keep it summary-only because summary_likelihood={summary_likelihood} meets the current threshold"
        );
    }

    "debug keeps this member visible; default profiles may collapse it into the lead group's omission notice"
        .to_string()
}

fn format_optional_score(score: Option<diag_core::Score>) -> String {
    score
        .map(|score| format!("{:.2}", score.into_inner()))
        .unwrap_or_else(|| "unknown".to_string())
}

fn cascade_role_label(role: GroupCascadeRole) -> &'static str {
    match role {
        GroupCascadeRole::LeadRoot => "lead_root",
        GroupCascadeRole::IndependentRoot => "independent_root",
        GroupCascadeRole::FollowOn => "follow_on",
        GroupCascadeRole::Duplicate => "duplicate",
        GroupCascadeRole::Uncertain => "uncertain",
    }
}

fn visibility_floor_label(visibility_floor: VisibilityFloor) -> &'static str {
    match visibility_floor {
        VisibilityFloor::NeverHidden => "never_hidden",
        VisibilityFloor::SummaryOrExpandedOnly => "summary_or_expanded_only",
        VisibilityFloor::HiddenAllowed => "hidden_allowed",
    }
}

fn compression_level_label(compression_level: CompressionLevel) -> &'static str {
    match compression_level {
        CompressionLevel::Off => "off",
        CompressionLevel::Conservative => "conservative",
        CompressionLevel::Balanced => "balanced",
        CompressionLevel::Aggressive => "aggressive",
    }
}

fn severity_label(severity: &Severity) -> &'static str {
    match severity {
        Severity::Fatal => "fatal",
        Severity::Error => "error",
        Severity::Warning => "warning",
        Severity::Note => "note",
        Severity::Remark => "remark",
        Severity::Info => "info",
        Severity::Debug => "debug",
        Severity::Unknown => "unknown",
    }
}

fn select_title(node: &DiagnosticNode, confidence: DisclosureConfidence) -> String {
    if confidence.allows_analysis_title() {
        node.analysis
            .as_ref()
            .and_then(|analysis| analysis.headline.as_ref().map(|c| c.to_string()))
            .unwrap_or_else(|| raw_title(node))
    } else {
        raw_title(node)
    }
}

fn select_first_action(node: &DiagnosticNode, confidence: DisclosureConfidence) -> Option<String> {
    if confidence.allows_first_action() {
        node.analysis
            .as_ref()
            .and_then(|analysis| analysis.first_action_hint.as_ref().map(|c| c.to_string()))
    } else {
        None
    }
}

fn confidence_label(confidence: DisclosureConfidence) -> &'static str {
    match confidence {
        DisclosureConfidence::Certain => "certain",
        DisclosureConfidence::Likely => "likely",
        DisclosureConfidence::Possible => "possible",
        DisclosureConfidence::Hidden => "hidden",
    }
}

fn raw_title(node: &DiagnosticNode) -> String {
    node.message
        .raw_text
        .lines()
        .next()
        .unwrap_or("diagnostic")
        .to_string()
}

fn raw_sub_block(request: &RenderRequest, node: &DiagnosticNode) -> Vec<String> {
    let policy = render_policy(request.profile);
    if !matches!(
        request.document.document_completeness,
        DocumentCompleteness::Partial
    ) || !matches!(
        node.node_completeness,
        NodeCompleteness::Partial | NodeCompleteness::Passthrough
    ) {
        return Vec::new();
    }

    node.message
        .raw_text
        .lines()
        .take(policy.disclosure.raw_sub_block_lines)
        .filter(|line| !line.trim().is_empty())
        .map(|line| line.to_string())
        .collect()
}

fn default_raw_block_label() -> String {
    "raw:".to_string()
}

fn default_session_mode() -> SessionMode {
    SessionMode::LeadPlusSummary
}

fn is_default_raw_block_label(label: &str) -> bool {
    label == "raw:"
}

fn conservative_band_c_notice() -> &'static str {
    "note: GCC 9-12 native-text summaries are conservative; verify against the preserved raw diagnostics"
}

fn conservative_raw_block_label() -> &'static str {
    "raw compiler excerpt:"
}

fn resolved_session_mode(
    presentation_policy: &ResolvedPresentationPolicy,
    has_failure: bool,
) -> SessionMode {
    if has_failure {
        presentation_policy.session_mode
    } else {
        SessionMode::LeadPlusSummary
    }
}
