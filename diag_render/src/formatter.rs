use crate::budget::{WarningFailureMode, budget_for};
use crate::view_model::RenderViewModel;
use crate::{DebugRefs, RenderProfile, RenderRequest, RenderResult};
use regex::Regex;
use std::sync::OnceLock;

pub fn emit(
    request: &RenderRequest,
    view_model: RenderViewModel,
    suppressed_warning_count: usize,
) -> RenderResult {
    let budget = budget_for(request.profile);
    let mut lines = Vec::new();
    if view_model.summary.partial_notice {
        lines.push("note: some compiler details were not fully structured; original diagnostics are preserved".to_string());
    }

    for card in &view_model.cards {
        if matches!(request.profile, RenderProfile::Ci) {
            let first_line = card
                .canonical_location
                .as_ref()
                .map(|location| format!("{location}: {}: {}", card.severity, card.title))
                .unwrap_or_else(|| {
                    if card
                        .family
                        .as_deref()
                        .is_some_and(|family| family.starts_with("linker"))
                    {
                        format!("linker: {}: {}", card.severity, card.title)
                    } else {
                        format!("{}: {}", card.severity, card.title)
                    }
                });
            lines.push(first_line);
        } else {
            lines.push(format!("{}: {}", card.severity, card.title));
            if let Some(location) = card.canonical_location.as_ref() {
                lines.push(format!("--> {location}"));
            }
        }
        if let Some(confidence_notice) = card.confidence_notice.as_ref() {
            lines.push(confidence_notice.clone());
        }
        if let Some(first_action) = card.first_action.as_ref() {
            lines.push(format!("help: {first_action}"));
        }
        lines.push(format!(
            "why: {}",
            display_raw_line(&card.raw_message, request.profile)
        ));
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
        for notice in &card.collapsed_notices {
            lines.push(format!("note: {notice}"));
        }
        if !card.raw_sub_block.is_empty() {
            lines.push("raw:".to_string());
            for raw_line in &card.raw_sub_block {
                lines.push(format!("  {}", display_raw_line(raw_line, request.profile)));
            }
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

    if suppressed_warning_count > 0
        && matches!(budget.warning_failure_mode, WarningFailureMode::Summarize)
    {
        lines.push(format!("note: suppressed {suppressed_warning_count} warning(s) while focusing on the failing group"));
    }
    if let Some(raw_hint) = view_model.summary.raw_diagnostics_hint.as_ref() {
        lines.push(raw_hint.clone());
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

    let truncation_occurred = lines.len() > budget.hard_max_lines;
    if truncation_occurred {
        lines.truncate(budget.hard_max_lines.saturating_sub(1));
        lines.push(
            "note: omitted additional details; rerun with --formed-profile=verbose".to_string(),
        );
    }

    RenderResult {
        text: lines.join("\n"),
        used_analysis: true,
        used_fallback: false,
        fallback_reason: None,
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

fn display_raw_line(raw_message: &str, profile: RenderProfile) -> String {
    let line = first_line(raw_message);
    match profile {
        RenderProfile::RawFallback => line,
        _ => sanitize_transient_object_paths(&line),
    }
}

fn sanitize_transient_object_paths(text: &str) -> String {
    transient_object_path_pattern()
        .replace_all(text, "<temp-object>")
        .into_owned()
}

fn transient_object_path_pattern() -> &'static Regex {
    static PATTERN: OnceLock<Regex> = OnceLock::new();
    PATTERN.get_or_init(|| {
        Regex::new(r#"(?:(?:/private)?/tmp|/var/folders/[^:\s]+/T)/cc[^:\s'"`]+\.o"#)
            .expect("valid transient object path regex")
    })
}
