use crate::RenderRequest;
use crate::budget::render_policy;
use crate::excerpt::load_excerpt;
use crate::family::{is_conservative_useful_subset_card, summarize_supporting_evidence};
use crate::path::format_location;
use diag_core::{
    DiagnosticNode, DisclosureConfidence, DocumentCompleteness, NodeCompleteness, Severity,
};
use serde::{Deserialize, Serialize};

/// Top-level session summary included in the view model.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RenderSessionSummary {
    /// Category of the session outcome (e.g. `"compile_failure"`, `"warnings_only"`).
    pub failure_kind: String,
    /// Whether a partial-document notice should be shown.
    pub partial_notice: bool,
    /// Optional hint directing the user to raw diagnostic output.
    pub raw_diagnostics_hint: Option<String>,
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
        .map(|node| build_card(request, &node))
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
        },
        cards: rendered_cards,
        summary_only_groups: summary_only_cards
            .into_iter()
            .map(|node| build_summary_only_group(request, &node))
            .collect(),
    }
}

fn build_card(request: &RenderRequest, node: &DiagnosticNode) -> RenderGroupCard {
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
        .and_then(|analysis| analysis.family.clone());
    let canonical_location = canonical_location(request, node);
    let excerpts = load_excerpt(request, node);
    let supporting_evidence = summarize_supporting_evidence(request, node);
    let context_lines = supporting_evidence.context_lines;
    let child_notes = supporting_evidence.child_notes;
    let collapsed_notices = supporting_evidence.collapsed_notices;
    let confidence_notice = if conservative_useful_subset {
        Some(conservative_band_c_notice().to_string())
    } else {
        confidence
            .requires_low_confidence_notice()
            .then_some(policy.disclosure.low_confidence_notice.to_string())
    };
    let raw_sub_block = raw_sub_block(request, node);

    RenderGroupCard {
        group_id: node.id.clone(),
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
        raw_block_label: if conservative_useful_subset {
            conservative_raw_block_label().to_string()
        } else {
            policy.disclosure.raw_block_label.to_string()
        },
        raw_sub_block,
        rule_id: node
            .analysis
            .as_ref()
            .and_then(|analysis| analysis.rule_id.clone()),
        matched_conditions: node
            .analysis
            .as_ref()
            .map(|analysis| analysis.matched_conditions.clone())
            .unwrap_or_default(),
        suppression_reason: node
            .analysis
            .as_ref()
            .and_then(|analysis| analysis.suppression_reason.clone()),
    }
}

fn build_summary_only_group(request: &RenderRequest, node: &DiagnosticNode) -> SummaryOnlyGroup {
    SummaryOnlyGroup {
        group_id: node.id.clone(),
        severity: severity_label(&node.severity).to_string(),
        title: select_title(
            node,
            node.analysis
                .as_ref()
                .map(|analysis| analysis.disclosure_confidence())
                .unwrap_or(DisclosureConfidence::Hidden),
        ),
        canonical_location: canonical_location(request, node),
    }
}

fn canonical_location(request: &RenderRequest, node: &DiagnosticNode) -> Option<String> {
    node.primary_location()
        .map(|location| format_location(request, location))
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
            .and_then(|analysis| analysis.headline.clone())
            .unwrap_or_else(|| raw_title(node))
    } else {
        raw_title(node)
    }
}

fn select_first_action(node: &DiagnosticNode, confidence: DisclosureConfidence) -> Option<String> {
    if confidence.allows_first_action() {
        node.analysis
            .as_ref()
            .and_then(|analysis| analysis.first_action_hint.clone())
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

fn is_default_raw_block_label(label: &str) -> bool {
    label == "raw:"
}

fn conservative_band_c_notice() -> &'static str {
    "note: GCC 9-12 native-text summaries are conservative; verify against the preserved raw diagnostics"
}

fn conservative_raw_block_label() -> &'static str {
    "raw compiler excerpt:"
}
