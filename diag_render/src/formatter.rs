use crate::budget::{WarningFailureMode, render_policy};
use crate::layout::LayoutProfile;
use crate::presentation::SessionMode;
use crate::theme::ThemePolicy;
use crate::view_model::{CascadeDebugInfo, RenderViewModel, SummaryOnlyGroup};
use crate::{DebugRefs, RenderRequest, RenderResult};
use diag_core::SuppressedCountVisibility;

/// Emits the final rendered text from a view model, applying layout, theme, and truncation.
pub fn emit(
    request: &RenderRequest,
    view_model: RenderViewModel,
    hidden_group_count: usize,
    suppressed_warning_count: usize,
) -> RenderResult {
    let policy = render_policy(request.profile);
    let theme = ThemePolicy::for_request(request);
    let layout = LayoutProfile::for_request(request);
    let use_block_local_budget = uses_block_local_budget(&view_model);
    let mut lines = Vec::new();
    let mut truncation_occurred = false;
    if view_model.summary.partial_notice {
        lines.push(policy.disclosure.partial_document_notice.to_string());
    }

    for (index, card) in view_model.cards.iter().enumerate() {
        let mut card_lines = Vec::new();
        layout.render_card(&theme, card, &mut card_lines);
        if matches!(
            request.profile,
            crate::RenderProfile::Verbose | crate::RenderProfile::Debug
        ) || matches!(request.debug_refs, DebugRefs::CaptureRef)
        {
            if let Some(rule_id) = card.rule_id.as_ref() {
                card_lines.push(format!("debug: rule_id={rule_id}"));
            }
            if !card.matched_conditions.is_empty() {
                card_lines.push(format!(
                    "debug: matched_conditions={}",
                    card.matched_conditions.join(", ")
                ));
            }
            if let Some(suppression_reason) = card.suppression_reason.as_ref() {
                card_lines.push(format!("debug: suppression_reason={suppression_reason}"));
            }
            if matches!(request.profile, crate::RenderProfile::Debug) {
                append_cascade_debug_lines(&mut card_lines, "", card.cascade_debug.as_ref());
            }
        }
        if use_block_local_budget {
            truncation_occurred |= truncate_block_lines(&mut card_lines, &policy);
        }
        lines.extend(card_lines);
        if use_block_local_budget && index + 1 < view_model.cards.len() {
            lines.push(String::new());
        }
    }
    if !view_model.summary_only_groups.is_empty() {
        lines.push(summary_only_heading(&view_model.summary_only_groups));
        for group in &view_model.summary_only_groups {
            lines.push(format!("  - {}", render_summary_only_group(&theme, group)));
            if matches!(request.profile, crate::RenderProfile::Debug) {
                append_cascade_debug_lines(&mut lines, "    ", group.cascade_debug.as_ref());
            }
        }
    }
    if should_emit_hidden_group_notice(request, hidden_group_count) {
        lines.push(format!(
            "note: omitted {hidden_group_count} related diagnostic(s) already covered by visible roots"
        ));
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

    let session_truncation_occurred =
        !use_block_local_budget && lines.len() > policy.budget.first_screenful_max_lines;
    if session_truncation_occurred {
        lines.truncate(policy.budget.first_screenful_max_lines.saturating_sub(1));
        lines.push(policy.disclosure.truncation_notice.to_string());
    }
    truncation_occurred |= session_truncation_occurred;

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
        suppressed_group_count: view_model.summary_only_groups.len() + hidden_group_count,
        suppressed_warning_count,
        truncation_occurred,
        render_issues: Vec::new(),
    }
}

fn uses_block_local_budget(view_model: &RenderViewModel) -> bool {
    matches!(
        view_model.summary.session_mode,
        SessionMode::AllVisibleBlocks
    ) && view_model.summary.failure_kind == "compile_failure"
}

fn truncate_block_lines(lines: &mut Vec<String>, policy: &crate::budget::RenderPolicy) -> bool {
    let target_budget_exceeded = lines.len() > policy.budget.target_lines_per_block;
    if lines.len() <= policy.budget.hard_max_lines_per_block {
        return false;
    }
    if !target_budget_exceeded {
        return false;
    }
    lines.truncate(policy.budget.hard_max_lines_per_block.saturating_sub(1));
    lines.push(policy.disclosure.block_truncation_notice.to_string());
    true
}

fn should_emit_hidden_group_notice(request: &RenderRequest, hidden_group_count: usize) -> bool {
    if hidden_group_count == 0 {
        return false;
    }
    match request.cascade_policy.show_suppressed_count {
        SuppressedCountVisibility::Always => true,
        SuppressedCountVisibility::Never => false,
        SuppressedCountVisibility::Auto => matches!(
            request.profile,
            crate::RenderProfile::Default
                | crate::RenderProfile::Concise
                | crate::RenderProfile::Ci
        ),
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

fn append_cascade_debug_lines(
    lines: &mut Vec<String>,
    indent: &str,
    cascade_debug: Option<&CascadeDebugInfo>,
) {
    let Some(cascade_debug) = cascade_debug else {
        return;
    };

    let mut facts = vec![
        format!("group_ref={}", cascade_debug.group_ref),
        format!("role={}", cascade_debug.cascade_role),
        format!("visibility_floor={}", cascade_debug.visibility_floor),
    ];
    if let Some(episode_ref) = cascade_debug.episode_ref.as_ref() {
        facts.push(format!("episode_ref={episode_ref}"));
    }
    lines.push(format!("{indent}debug-facts: {}", facts.join(", ")));

    if let Some(best_parent_group_ref) = cascade_debug.best_parent_group_ref.as_ref() {
        lines.push(format!(
            "{indent}debug-facts: best_parent_group_ref={best_parent_group_ref}"
        ));
    }
    if !cascade_debug.evidence_tags.is_empty() {
        lines.push(format!(
            "{indent}debug-facts: evidence_tags={}",
            cascade_debug.evidence_tags.join(", ")
        ));
    }
    if let Some(suppression_policy) = cascade_debug.suppression_policy.as_ref() {
        lines.push(format!("{indent}debug-policy: {suppression_policy}"));
    }
    if !cascade_debug.provenance_capture_refs.is_empty() {
        lines.push(format!(
            "{indent}debug-raw: provenance_capture_refs={}",
            cascade_debug.provenance_capture_refs.join(", ")
        ));
    }
}
