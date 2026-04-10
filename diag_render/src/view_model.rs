use crate::budget::render_policy;
use crate::excerpt::load_excerpt;
use crate::family::{is_conservative_useful_subset_card, summarize_supporting_evidence};
use crate::{RenderProfile, RenderRequest};
use diag_core::{Confidence, DiagnosticNode, DocumentCompleteness, NodeCompleteness, Severity};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RenderSessionSummary {
    pub failure_kind: String,
    pub partial_notice: bool,
    pub raw_diagnostics_hint: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RenderGroupCard {
    pub group_id: String,
    pub severity: String,
    pub family: Option<String>,
    pub title: String,
    pub confidence_label: String,
    pub confidence_notice: Option<String>,
    pub first_action: Option<String>,
    pub canonical_location: Option<String>,
    pub raw_message: String,
    pub excerpts: Vec<crate::ExcerptBlock>,
    pub context_lines: Vec<String>,
    pub child_notes: Vec<String>,
    pub collapsed_notices: Vec<String>,
    #[serde(
        skip_serializing_if = "is_default_raw_block_label",
        default = "default_raw_block_label"
    )]
    pub raw_block_label: String,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub raw_sub_block: Vec<String>,
    pub rule_id: Option<String>,
    pub matched_conditions: Vec<String>,
    pub suppression_reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RenderViewModel {
    pub summary: RenderSessionSummary,
    pub cards: Vec<RenderGroupCard>,
}

pub fn build(request: &RenderRequest, cards: Vec<DiagnosticNode>) -> RenderViewModel {
    let policy = render_policy(request.profile);
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
            ),
            raw_diagnostics_hint: request
                .document
                .captures
                .iter()
                .any(|capture| capture.id == "stderr.raw")
                .then_some(policy.disclosure.raw_diagnostics_hint.to_string()),
        },
        cards: rendered_cards,
    }
}

fn build_card(request: &RenderRequest, node: &DiagnosticNode) -> RenderGroupCard {
    let policy = render_policy(request.profile);
    let conservative_useful_subset = is_conservative_useful_subset_card(request, node);
    let confidence = node
        .analysis
        .as_ref()
        .and_then(|analysis| analysis.confidence.as_ref());
    let confidence_label = confidence_label(confidence).to_string();
    let title = select_title(node, confidence);
    let first_action = select_first_action(node, confidence);
    let family = node
        .analysis
        .as_ref()
        .and_then(|analysis| analysis.family.clone());
    let canonical_location = node.primary_location().map(|location| {
        let path = if matches!(request.profile, RenderProfile::Ci) {
            location.path.clone()
        } else if let Some(cwd) = request.cwd.as_ref() {
            std::path::Path::new(&location.path)
                .strip_prefix(cwd)
                .map(|path| path.display().to_string())
                .unwrap_or_else(|_| location.path.clone())
        } else {
            location.path.clone()
        };
        format!("{path}:{}:{}", location.line, location.column)
    });
    let excerpts = load_excerpt(request, node);
    let supporting_evidence = summarize_supporting_evidence(request, node);
    let context_lines = supporting_evidence.context_lines;
    let child_notes = supporting_evidence.child_notes;
    let collapsed_notices = supporting_evidence.collapsed_notices;
    let confidence_notice = if conservative_useful_subset {
        Some(conservative_band_c_notice().to_string())
    } else {
        matches!(
            confidence,
            Some(Confidence::Low) | Some(Confidence::Unknown) | None
        )
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

fn select_title(node: &DiagnosticNode, confidence: Option<&Confidence>) -> String {
    match confidence {
        Some(Confidence::High) | Some(Confidence::Medium) => node
            .analysis
            .as_ref()
            .and_then(|analysis| analysis.headline.clone())
            .unwrap_or_else(|| raw_title(node)),
        Some(Confidence::Low) | Some(Confidence::Unknown) | None => raw_title(node),
    }
}

fn select_first_action(node: &DiagnosticNode, confidence: Option<&Confidence>) -> Option<String> {
    match confidence {
        Some(Confidence::High) | Some(Confidence::Medium) => node
            .analysis
            .as_ref()
            .and_then(|analysis| analysis.first_action_hint.clone()),
        Some(Confidence::Low) | Some(Confidence::Unknown) | None => None,
    }
}

fn confidence_label(confidence: Option<&Confidence>) -> &'static str {
    match confidence {
        Some(Confidence::High) => "certain",
        Some(Confidence::Medium) => "likely",
        Some(Confidence::Low) => "possible",
        Some(Confidence::Unknown) | None => "hidden",
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
