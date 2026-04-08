use crate::excerpt::load_excerpt;
use crate::family::summarize_context;
use crate::{RenderProfile, RenderRequest};
use diag_core::{Confidence, DiagnosticNode, Severity};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RenderSessionSummary {
    pub failure_kind: String,
    pub partial_notice: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RenderGroupCard {
    pub group_id: String,
    pub severity: String,
    pub title: String,
    pub confidence_label: String,
    pub first_action: Option<String>,
    pub canonical_location: Option<String>,
    pub raw_message: String,
    pub excerpts: Vec<crate::ExcerptBlock>,
    pub context_lines: Vec<String>,
    pub child_notes: Vec<String>,
    pub collapsed_notices: Vec<String>,
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
        },
        cards: rendered_cards,
    }
}

fn build_card(request: &RenderRequest, node: &DiagnosticNode) -> RenderGroupCard {
    let confidence = node
        .analysis
        .as_ref()
        .and_then(|analysis| analysis.confidence.as_ref());
    let confidence_label = confidence_label(confidence).to_string();
    let title = select_title(node, confidence);
    let first_action = select_first_action(node, confidence);
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
    let context_lines = summarize_context(node, request.profile);
    let note_limit = match request.profile {
        RenderProfile::Verbose => 10,
        _ => 3,
    };
    let unique_child_notes = dedup_lines(
        node.children
            .iter()
            .map(|child| {
                child
                    .message
                    .raw_text
                    .lines()
                    .next()
                    .unwrap_or_default()
                    .to_string()
            })
            .collect(),
    );
    let unique_child_note_count = unique_child_notes.len();
    let child_notes = unique_child_notes
        .into_iter()
        .take(note_limit)
        .collect::<Vec<_>>();
    let mut collapsed_notices = Vec::new();
    if unique_child_note_count > note_limit {
        collapsed_notices.push(format!(
            "omitted {} additional note(s)",
            unique_child_note_count - note_limit
        ));
    }

    RenderGroupCard {
        group_id: node.id.clone(),
        severity: severity_label(&node.severity).to_string(),
        title,
        confidence_label,
        first_action,
        canonical_location,
        raw_message: node.message.raw_text.clone(),
        excerpts,
        context_lines,
        child_notes,
        collapsed_notices,
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

fn dedup_lines(lines: Vec<String>) -> Vec<String> {
    let mut deduped = Vec::new();
    for line in lines {
        if !line.trim().is_empty() && !deduped.iter().any(|existing| existing == &line) {
            deduped.push(line);
        }
    }
    deduped
}
