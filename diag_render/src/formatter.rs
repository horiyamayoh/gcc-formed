use crate::budget::{WarningFailureMode, render_policy};
use crate::layout::LayoutProfile;
use crate::theme::ThemePolicy;
use crate::view_model::{RenderViewModel, SummaryOnlyGroup};
use crate::{DebugRefs, RenderRequest, RenderResult};

/// Emits the final rendered text from a view model, applying layout, theme, and truncation.
pub fn emit(
    request: &RenderRequest,
    view_model: RenderViewModel,
    suppressed_warning_count: usize,
) -> RenderResult {
    let policy = render_policy(request.profile);
    let theme = ThemePolicy::for_request(request);
    let layout = LayoutProfile::for_request(request);
    let mut lines = Vec::new();
    if view_model.summary.partial_notice {
        lines.push(policy.disclosure.partial_document_notice.to_string());
    }

    for card in &view_model.cards {
        layout.render_card(&theme, card, &mut lines);
        if matches!(
            request.profile,
            crate::RenderProfile::Verbose | crate::RenderProfile::Debug
        ) || matches!(request.debug_refs, DebugRefs::CaptureRef)
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
    if !view_model.summary_only_groups.is_empty() {
        lines.push(summary_only_heading(&view_model.summary_only_groups));
        for group in &view_model.summary_only_groups {
            lines.push(format!("  - {}", render_summary_only_group(&theme, group)));
        }
    }

    if suppressed_warning_count > 0
        && matches!(
            policy.budget.warning_failure_mode,
            WarningFailureMode::Summarize
        )
    {
        lines.push(
            policy
                .disclosure
                .suppressed_warning_notice
                .replace("{count}", &suppressed_warning_count.to_string()),
        );
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

    let truncation_occurred = lines.len() > policy.budget.first_screenful_max_lines;
    if truncation_occurred {
        lines.truncate(policy.budget.first_screenful_max_lines.saturating_sub(1));
        lines.push(policy.disclosure.truncation_notice.to_string());
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
        suppressed_group_count: view_model.summary_only_groups.len(),
        suppressed_warning_count,
        truncation_occurred,
        render_issues: Vec::new(),
    }
}

fn summary_only_heading(groups: &[SummaryOnlyGroup]) -> String {
    if groups.iter().all(|group| group.severity == "warning") {
        "other warnings:".to_string()
    } else if groups
        .iter()
        .any(|group| group.severity == "fatal" || group.severity == "error")
    {
        "other errors:".to_string()
    } else {
        "other diagnostics:".to_string()
    }
}

fn render_summary_only_group(theme: &ThemePolicy, group: &SummaryOnlyGroup) -> String {
    match group.canonical_location.as_ref() {
        Some(location) => format!(
            "{}: {}: {}",
            theme.inline(location),
            group.severity,
            theme.inline(&group.title)
        ),
        None => format!("{}: {}", group.severity, theme.inline(&group.title)),
    }
}
