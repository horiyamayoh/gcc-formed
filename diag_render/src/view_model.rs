use crate::excerpt::load_excerpt;
use crate::family::summarize_context;
use crate::{RenderProfile, RenderRequest};
use diag_core::{DiagnosticNode, Severity};

#[derive(Debug, Clone)]
pub struct RenderSessionSummary {
    pub failure_kind: String,
    pub partial_notice: bool,
}

#[derive(Debug, Clone)]
pub struct RenderGroupCard {
    pub group_id: String,
    pub severity: String,
    pub title: String,
    pub first_action: Option<String>,
    pub canonical_location: Option<String>,
    pub raw_message: String,
    pub excerpts: Vec<crate::ExcerptBlock>,
    pub context_lines: Vec<String>,
    pub child_notes: Vec<String>,
}

#[derive(Debug, Clone)]
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
    let title = node
        .analysis
        .as_ref()
        .and_then(|analysis| analysis.headline.clone())
        .unwrap_or_else(|| {
            node.message
                .raw_text
                .lines()
                .next()
                .unwrap_or("diagnostic")
                .to_string()
        });
    let first_action = node
        .analysis
        .as_ref()
        .and_then(|analysis| analysis.first_action_hint.clone());
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
    let child_notes = node
        .children
        .iter()
        .take(match request.profile {
            RenderProfile::Verbose => 10,
            _ => 3,
        })
        .map(|child| {
            child
                .message
                .raw_text
                .lines()
                .next()
                .unwrap_or_default()
                .to_string()
        })
        .collect::<Vec<_>>();

    RenderGroupCard {
        group_id: node.id.clone(),
        severity: severity_label(&node.severity).to_string(),
        title,
        first_action,
        canonical_location,
        raw_message: node.message.raw_text.clone(),
        excerpts,
        context_lines,
        child_notes,
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
