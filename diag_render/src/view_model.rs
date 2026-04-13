use crate::RenderRequest;
use crate::budget::render_policy;
use crate::excerpt::load_excerpt;
use crate::family::{
    extract_conflict_slots, extract_context_slots, extract_contrast_slots, extract_linker_slots,
    extract_lookup_slots, extract_missing_header_slots, extract_parser_slots,
    is_conservative_useful_subset_card, summarize_supporting_evidence,
};
use crate::path::format_location;
use crate::presentation::{
    RenderSemanticCard, RenderSemanticSlot, ResolvedCardPresentation, ResolvedPresentationPolicy,
    SemanticShape, SemanticSlotId, SessionMode,
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

struct SemanticCardInput<'a> {
    family: Option<&'a str>,
    title: &'a str,
    first_action: Option<&'a str>,
    canonical_location: Option<&'a str>,
    conservative_useful_subset: bool,
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
    let semantic_card = build_semantic_card(
        request,
        node,
        presentation_policy,
        SemanticCardInput {
            family: family.as_deref(),
            title: &title,
            first_action: first_action.as_deref(),
            canonical_location: canonical_location.as_deref(),
            conservative_useful_subset,
        },
    );
    let (context_lines, child_notes, mut collapsed_notices) =
        adapt_supporting_evidence_for_presentation(supporting_evidence, &semantic_card);
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
    request: &RenderRequest,
    node: &DiagnosticNode,
    presentation_policy: &ResolvedPresentationPolicy,
    input: SemanticCardInput<'_>,
) -> RenderSemanticCard {
    let mut resolved_presentation = presentation_policy.resolve_card_presentation(input.family);
    let mut slots = Vec::new();
    let fallback_to_generic = |resolved: &mut crate::presentation::ResolvedCardPresentation| {
        resolved.template_id = presentation_policy.generic_template_id.clone();
        resolved.semantic_shape = SemanticShape::Generic;
        resolved.shape_fallbacks.clear();
        resolved.fell_back_to_generic_template = true;
    };
    if let Some(first_action) = input.first_action {
        slots.push(RenderSemanticSlot {
            slot: SemanticSlotId::FirstAction,
            value: first_action.to_string(),
            label: presentation_policy.label("help").map(str::to_string),
        });
    }

    let mut extracted_family_slots = false;
    if resolved_presentation.subject_first_header && !input.conservative_useful_subset {
        for (shape, display_family_override) in semantic_shape_attempts(&resolved_presentation) {
            let Some(extracted_slots) =
                extract_slots_for_shape(request, node, presentation_policy, shape)
            else {
                continue;
            };

            let Some(template_id) = presentation_policy
                .template_id_for_shape(&resolved_presentation.template_id, shape)
            else {
                continue;
            };

            resolved_presentation.template_id = template_id.to_string();
            resolved_presentation.semantic_shape = shape;
            if let Some(display_family) = display_family_override {
                resolved_presentation.display_family = Some(display_family);
            }
            resolved_presentation.shape_fallbacks.clear();
            slots.extend(extracted_slots);
            extracted_family_slots = true;
            break;
        }

        if !extracted_family_slots && resolved_presentation.semantic_shape != SemanticShape::Generic
        {
            fallback_to_generic(&mut resolved_presentation);
        }
    }

    if !extracted_family_slots
        || resolved_presentation.template_id == presentation_policy.generic_template_id
        || resolved_presentation.template_id.starts_with("legacy_")
    {
        slots.push(RenderSemanticSlot {
            slot: SemanticSlotId::Raw,
            value: node.message.raw_text.clone(),
            label: presentation_policy
                .slot_label(SemanticSlotId::Raw)
                .map(str::to_string),
        });
    }

    RenderSemanticCard {
        internal_family: input.family.map(ToString::to_string),
        display_family: resolved_presentation.display_family.clone(),
        subject: input.title.to_string(),
        presentation: resolved_presentation,
        slots,
        canonical_location: input.canonical_location.map(ToString::to_string),
        raw_message: node.message.raw_text.clone(),
    }
}

fn semantic_shape_attempts(
    resolved_presentation: &ResolvedCardPresentation,
) -> Vec<(SemanticShape, Option<String>)> {
    let mut attempts = resolved_presentation
        .shape_fallbacks
        .iter()
        .map(|fallback| (fallback.shape, fallback.display_family.clone()))
        .collect::<Vec<_>>();
    attempts.push((
        resolved_presentation.semantic_shape,
        resolved_presentation.display_family.clone(),
    ));
    attempts
}

fn extract_slots_for_shape(
    request: &RenderRequest,
    node: &DiagnosticNode,
    presentation_policy: &ResolvedPresentationPolicy,
    shape: SemanticShape,
) -> Option<Vec<RenderSemanticSlot>> {
    let mut slots = Vec::new();
    let push = |slots: &mut Vec<RenderSemanticSlot>, slot: SemanticSlotId, value: String| {
        slots.push(RenderSemanticSlot {
            slot,
            value,
            label: presentation_policy.slot_label(slot).map(str::to_string),
        });
    };

    match shape {
        SemanticShape::Contrast => {
            let contrast = extract_contrast_slots(request, node)?;
            push(&mut slots, SemanticSlotId::Want, contrast.want);
            push(&mut slots, SemanticSlotId::Got, contrast.got);
            if let Some(via) = contrast.via {
                push(&mut slots, SemanticSlotId::Via, via);
            }
        }
        SemanticShape::Parser => {
            let parser = extract_parser_slots(request, node)?;
            if let Some(want) = parser.want {
                push(&mut slots, SemanticSlotId::Want, want);
            }
            if let Some(near) = parser.near {
                push(&mut slots, SemanticSlotId::Near, near);
            }
        }
        SemanticShape::Lookup => {
            let lookup = extract_lookup_slots(request, node)?;
            if let Some(name) = lookup.name {
                push(&mut slots, SemanticSlotId::Name, name);
            }
            if let Some(use_site) = lookup.use_site {
                push(&mut slots, SemanticSlotId::Use, use_site);
            }
            if let Some(need) = lookup.need {
                push(&mut slots, SemanticSlotId::Need, need);
            }
            if let Some(from) = lookup.from {
                push(&mut slots, SemanticSlotId::From, from);
            }
            if let Some(near) = lookup.near {
                push(&mut slots, SemanticSlotId::Near, near);
            }
        }
        SemanticShape::MissingHeader => {
            let missing_header = extract_missing_header_slots(request, node)?;
            push(&mut slots, SemanticSlotId::Need, missing_header.need);
            if let Some(from) = missing_header.from {
                push(&mut slots, SemanticSlotId::From, from);
            }
        }
        SemanticShape::Conflict => {
            let conflict = extract_conflict_slots(request, node)?;
            if let Some(now) = conflict.now {
                push(&mut slots, SemanticSlotId::Now, now);
            }
            if let Some(prev) = conflict.prev {
                push(&mut slots, SemanticSlotId::Prev, prev);
            }
        }
        SemanticShape::Context => {
            let context = extract_context_slots(request, node)?;
            if let Some(from) = context.from {
                push(&mut slots, SemanticSlotId::From, from);
            }
            if let Some(via) = context.via {
                push(&mut slots, SemanticSlotId::Via, via);
            }
        }
        SemanticShape::Linker => {
            let linker = extract_linker_slots(request, node)?;
            push(&mut slots, SemanticSlotId::Symbol, linker.symbol);
            if let Some(from) = linker.from {
                push(&mut slots, SemanticSlotId::From, from);
            }
            if let Some(archive) = linker.archive {
                push(&mut slots, SemanticSlotId::Archive, archive);
            }
            if let Some(now) = linker.now {
                push(&mut slots, SemanticSlotId::Now, now);
            }
            if let Some(prev) = linker.prev {
                push(&mut slots, SemanticSlotId::Prev, prev);
            }
        }
        SemanticShape::Generic => return None,
    }

    (!slots.is_empty()).then_some(slots)
}

fn adapt_supporting_evidence_for_presentation(
    supporting_evidence: crate::family::SupportingEvidence,
    semantic_card: &RenderSemanticCard,
) -> (Vec<String>, Vec<String>, Vec<String>) {
    let mut context_lines = supporting_evidence.context_lines;
    let mut child_notes = supporting_evidence.child_notes;
    let collapsed_notices = supporting_evidence.collapsed_notices;

    match semantic_card.presentation.semantic_shape {
        SemanticShape::Contrast => {
            context_lines.retain(|line| {
                let trimmed = line.trim_start();
                !trimmed.starts_with("because:")
                    && !trimmed.starts_with("candidate ")
                    && !(trimmed.starts_with("omitted ")
                        && (trimmed.contains("overload")
                            || trimmed.contains("candidate")
                            || trimmed.contains("note")))
            });
            child_notes.clear();
        }
        SemanticShape::Linker => {
            context_lines.retain(|line| {
                let trimmed = line.trim_start();
                !(trimmed.starts_with("linker:")
                    || trimmed.starts_with("omitted ") && trimmed.contains("reference"))
            });
            child_notes.clear();
        }
        SemanticShape::Lookup | SemanticShape::MissingHeader | SemanticShape::Conflict => {
            child_notes.clear();
        }
        SemanticShape::Context => {
            if let Some(include_chain_start) = context_lines
                .iter()
                .position(|line| line.trim_start() == "from include chain:")
            {
                context_lines = context_lines.split_off(include_chain_start);
            } else {
                context_lines.clear();
            }
            child_notes.clear();
        }
        SemanticShape::Parser | SemanticShape::Generic => {}
    }

    (context_lines, child_notes, collapsed_notices)
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
