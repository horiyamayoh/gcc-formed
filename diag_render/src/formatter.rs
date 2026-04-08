use crate::view_model::RenderViewModel;
use crate::{DebugRefs, RenderProfile, RenderRequest, RenderResult};

pub fn emit(
    request: &RenderRequest,
    view_model: RenderViewModel,
    suppressed_warning_count: usize,
) -> RenderResult {
    let mut lines = Vec::new();
    if view_model.summary.partial_notice {
        lines.push("note: some compiler details were not fully structured; original diagnostics are preserved".to_string());
    }

    let max_lines = line_budget(request.profile);
    for card in &view_model.cards {
        if matches!(request.profile, RenderProfile::Ci) {
            let first_line = card
                .canonical_location
                .as_ref()
                .map(|location| format!("{location}: {}: {}", card.severity, card.title))
                .unwrap_or_else(|| format!("{}: {}", card.severity, card.title));
            lines.push(first_line);
        } else {
            lines.push(format!("{}: {}", card.severity, card.title));
            if let Some(location) = card.canonical_location.as_ref() {
                lines.push(format!("--> {location}"));
            }
        }
        if let Some(first_action) = card.first_action.as_ref() {
            lines.push(format!("help: {first_action}"));
        }
        lines.push(format!("why: {}", first_line(&card.raw_message)));
        for excerpt in &card.excerpts {
            lines.push(format!("| {}", excerpt.location));
            for source in &excerpt.lines {
                lines.push(format!("| {source}"));
            }
        }
        for context in &card.context_lines {
            lines.push(context.clone());
        }
        for note in &card.child_notes {
            lines.push(format!("note: {note}"));
        }
        if matches!(request.profile, RenderProfile::Verbose)
            || matches!(request.debug_refs, DebugRefs::CaptureRef)
        {
            if let Some(rule_id) = card.rule_id.as_ref() {
                lines.push(format!("debug: rule_id={rule_id}"));
            }
            if !card.matched_conditions.is_empty() {
                lines.push(format!(
                    "debug: matched_conditions={}",
                    card.matched_conditions.join(", ")
                ));
            }
            if let Some(suppression_reason) = card.suppression_reason.as_ref() {
                lines.push(format!("debug: suppression_reason={suppression_reason}"));
            }
        }
    }

    if suppressed_warning_count > 0 {
        lines.push(format!("note: suppressed {suppressed_warning_count} warning(s) while focusing on the failing group"));
    }
    if matches!(request.debug_refs, DebugRefs::TraceId) {
        lines.push(format!("trace: {}", request.document.run.invocation_id));
    }
    if matches!(request.debug_refs, DebugRefs::CaptureRef) {
        let capture_ids = request
            .document
            .captures
            .iter()
            .map(|capture| capture.id.clone())
            .collect::<Vec<_>>()
            .join(", ");
        if !capture_ids.is_empty() {
            lines.push(format!("captures: {capture_ids}"));
        }
    }

    let truncation_occurred = lines.len() > max_lines;
    if truncation_occurred {
        lines.truncate(max_lines.saturating_sub(1));
        lines.push(
            "note: omitted additional details; rerun with --formed-profile=verbose".to_string(),
        );
    }

    RenderResult {
        text: lines.join("\n"),
        used_analysis: true,
        used_fallback: false,
        displayed_group_refs: view_model
            .cards
            .iter()
            .map(|card| card.group_id.clone())
            .collect(),
        suppressed_group_count: 0,
        suppressed_warning_count,
        truncation_occurred,
        render_issues: Vec::new(),
    }
}

fn first_line(raw_message: &str) -> String {
    raw_message
        .lines()
        .next()
        .unwrap_or(raw_message)
        .to_string()
}

fn line_budget(profile: RenderProfile) -> usize {
    match profile {
        RenderProfile::Verbose => 80,
        RenderProfile::Ci => 16,
        RenderProfile::Concise => 14,
        RenderProfile::Default => 28,
        RenderProfile::RawFallback => usize::MAX,
    }
}
