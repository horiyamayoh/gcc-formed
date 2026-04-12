use crate::presentation::{RenderSemanticSlot, ResolvedPresentationPolicy, SessionMode};
use crate::view_model::RenderViewModel;
use crate::{RenderGroupCard, SummaryOnlyGroup};
use serde::{Deserialize, Serialize};

/// Session-level presentation metadata kept in the internal snapshot artifact.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RenderPresentationSessionSummary {
    /// Session outcome category chosen by the render view model.
    pub failure_kind: String,
    /// Whether the rendered surface is disclosing partial input coverage.
    pub partial_notice: bool,
    /// Raw-diagnostics escape hatch when the view model surfaced one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub raw_diagnostics_hint: Option<String>,
    /// Policy-declared session mode before failure-specific adaptation.
    pub policy_session_mode: SessionMode,
    /// Effective session mode used by the view model.
    pub resolved_session_mode: SessionMode,
}

/// A single resolved semantic slot recorded in the internal presentation artifact.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RenderPresentationSlot {
    /// Semantic slot identifier.
    pub slot: crate::SemanticSlotId,
    /// Resolved slot value shown to the user.
    pub value: String,
    /// Optional human-facing label after catalog resolution.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
}

/// Presentation-focused view of a rendered card for snapshot review and debugging.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RenderPresentationCard {
    /// Group identifier shared with the render view model.
    pub group_id: String,
    /// Rendered severity bucket.
    pub severity: String,
    /// Machine-facing family selected by analysis, if any.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub internal_family: Option<String>,
    /// Human-facing display family selected by the presentation policy, if any.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display_family: Option<String>,
    /// Legacy display title kept for side-by-side review.
    pub title: String,
    /// Subject-first headline subject resolved for the card.
    pub subject: String,
    /// Resolved card presentation, including template and location policy.
    pub presentation: crate::ResolvedCardPresentation,
    /// Ordered semantic slots used to populate the template.
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub slots: Vec<RenderPresentationSlot>,
    /// Canonical location after path policy resolution, if any.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub canonical_location: Option<String>,
    /// Raw compiler message preserved for debugging fail-open behavior.
    pub raw_message: String,
}

/// Summary-only groups retained in the internal presentation artifact.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RenderPresentationSummaryOnlyGroup {
    /// Group identifier shared with the render view model.
    pub group_id: String,
    /// Severity bucket of the collapsed group.
    pub severity: String,
    /// Summary title shown on the rendered surface.
    pub title: String,
    /// Canonical location, if one was surfaced for the summary line.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub canonical_location: Option<String>,
}

/// Internal-only snapshot of presentation decisions for a rendered view model.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RenderPresentationSnapshot {
    /// Resolved preset identifier.
    pub preset_id: String,
    /// Whether config resolution fell back to the built-in default policy.
    pub fell_back_to_default: bool,
    /// Non-fatal presentation warnings collected during policy resolution.
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub warnings: Vec<String>,
    /// Session-level presentation metadata.
    pub summary: RenderPresentationSessionSummary,
    /// Fully rendered cards in display order.
    pub cards: Vec<RenderPresentationCard>,
    /// Groups rendered only as one-line summaries.
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub summary_only_groups: Vec<RenderPresentationSummaryOnlyGroup>,
}

pub(crate) fn from_view_model(
    view_model: &RenderViewModel,
    presentation_policy: &ResolvedPresentationPolicy,
) -> RenderPresentationSnapshot {
    RenderPresentationSnapshot {
        preset_id: presentation_policy.preset_id.clone(),
        fell_back_to_default: presentation_policy.fell_back_to_default,
        warnings: presentation_policy.warnings.clone(),
        summary: RenderPresentationSessionSummary {
            failure_kind: view_model.summary.failure_kind.clone(),
            partial_notice: view_model.summary.partial_notice,
            raw_diagnostics_hint: view_model.summary.raw_diagnostics_hint.clone(),
            policy_session_mode: presentation_policy.session_mode,
            resolved_session_mode: view_model.summary.session_mode,
        },
        cards: view_model
            .cards
            .iter()
            .map(snapshot_card)
            .collect::<Vec<_>>(),
        summary_only_groups: view_model
            .summary_only_groups
            .iter()
            .map(snapshot_summary_only_group)
            .collect::<Vec<_>>(),
    }
}

fn snapshot_card(card: &RenderGroupCard) -> RenderPresentationCard {
    RenderPresentationCard {
        group_id: card.group_id.clone(),
        severity: card.severity.clone(),
        internal_family: card.semantic_card.internal_family.clone(),
        display_family: card.semantic_card.display_family.clone(),
        title: card.title.clone(),
        subject: card.semantic_card.subject.clone(),
        presentation: card.semantic_card.presentation.clone(),
        slots: card
            .semantic_card
            .slots
            .iter()
            .map(snapshot_slot)
            .collect::<Vec<_>>(),
        canonical_location: card.semantic_card.canonical_location.clone(),
        raw_message: card.semantic_card.raw_message.clone(),
    }
}

fn snapshot_slot(slot: &RenderSemanticSlot) -> RenderPresentationSlot {
    RenderPresentationSlot {
        slot: slot.slot,
        value: slot.value.clone(),
        label: slot.label.clone(),
    }
}

fn snapshot_summary_only_group(group: &SummaryOnlyGroup) -> RenderPresentationSummaryOnlyGroup {
    RenderPresentationSummaryOnlyGroup {
        group_id: group.group_id.clone(),
        severity: group.severity.clone(),
        title: group.title.clone(),
        canonical_location: group.canonical_location.clone(),
    }
}
