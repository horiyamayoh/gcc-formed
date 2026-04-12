use crate::path::{display_path_for_raw, format_edit_span, resolved_path};
use crate::view_model::RenderActionItem;
use crate::{RenderProfile, RenderRequest};
use diag_core::{DiagnosticNode, Suggestion, SuggestionApplicability, TextEdit};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;

pub(crate) fn build_action_items(
    request: &RenderRequest,
    node: &DiagnosticNode,
) -> Vec<RenderActionItem> {
    let mut ordered = node.suggestions.iter().enumerate().collect::<Vec<_>>();
    ordered.sort_by(|(left_index, left), (right_index, right)| {
        suggestion_order_key(request, node, left, *left_index).cmp(&suggestion_order_key(
            request,
            node,
            right,
            *right_index,
        ))
    });

    ordered
        .into_iter()
        .map(|(_, suggestion)| RenderActionItem {
            label: applicability_label(&suggestion.applicability).to_string(),
            text: suggestion_summary_text(request, suggestion),
            applicability: suggestion.applicability.clone(),
            inline_patch: build_inline_patch(request, node, suggestion).unwrap_or_default(),
        })
        .collect()
}

fn suggestion_order_key(
    request: &RenderRequest,
    node: &DiagnosticNode,
    suggestion: &Suggestion,
    original_index: usize,
) -> (u8, u8, u8, usize, usize) {
    (
        applicability_rank(&suggestion.applicability),
        u8::from(!touches_user_owned_path(request, node, suggestion)),
        u8::from(!is_single_file_edit(suggestion)),
        suggestion.edits.len(),
        original_index,
    )
}

fn applicability_rank(applicability: &SuggestionApplicability) -> u8 {
    match applicability {
        SuggestionApplicability::MachineApplicable => 0,
        SuggestionApplicability::MaybeIncorrect => 1,
        SuggestionApplicability::Manual => 2,
    }
}

fn applicability_label(applicability: &SuggestionApplicability) -> &'static str {
    match applicability {
        SuggestionApplicability::MachineApplicable => "suggested edit",
        SuggestionApplicability::MaybeIncorrect => "likely edit",
        SuggestionApplicability::Manual => "consider",
    }
}

fn suggestion_summary_text(request: &RenderRequest, suggestion: &Suggestion) -> String {
    let base = if suggestion.label.trim().is_empty() {
        generated_summary_from_edits(request, suggestion)
    } else {
        suggestion.label.trim().to_string()
    };

    match suggestion.edits.as_slice() {
        [] => base,
        [edit] => format!("{base} at {}", format_edit_span(request, edit)),
        edits => {
            let unique_paths = unique_edit_paths(edits);
            if unique_paths.len() == 1 {
                format!(
                    "{base} in {} ({} edits)",
                    display_path_for_raw(request, unique_paths[0]),
                    edits.len()
                )
            } else {
                format!(
                    "{base} across {} files ({} edits)",
                    unique_paths.len(),
                    edits.len()
                )
            }
        }
    }
}

fn generated_summary_from_edits(request: &RenderRequest, suggestion: &Suggestion) -> String {
    match suggestion.edits.as_slice() {
        [] => "review the compiler-provided suggestion".to_string(),
        [edit] => format!("edit {}", format_edit_span(request, edit)),
        edits => {
            let unique_paths = unique_edit_paths(edits);
            if unique_paths.len() == 1 {
                format!(
                    "{} edits in {}",
                    edits.len(),
                    display_path_for_raw(request, unique_paths[0])
                )
            } else {
                format!("{} edits across {} files", edits.len(), unique_paths.len())
            }
        }
    }
}

fn build_inline_patch(
    request: &RenderRequest,
    node: &DiagnosticNode,
    suggestion: &Suggestion,
) -> Option<Vec<String>> {
    if !matches!(request.profile, RenderProfile::Default | RenderProfile::Ci) {
        return None;
    }
    if !matches!(
        suggestion.applicability,
        SuggestionApplicability::MachineApplicable | SuggestionApplicability::MaybeIncorrect
    ) {
        return None;
    }
    if suggestion.edits.is_empty() || suggestion.edits.len() > 3 {
        return None;
    }

    let edit_path = unique_edit_paths(&suggestion.edits)
        .into_iter()
        .next()
        .filter(|_| unique_edit_paths(&suggestion.edits).len() == 1)?;

    let mut line_to_edits = BTreeMap::<u32, Vec<&TextEdit>>::new();
    let mut total_changed_chars = 0usize;
    for edit in &suggestion.edits {
        if edit.path != edit_path || edit.start_line != edit.end_line {
            return None;
        }
        line_to_edits.entry(edit.start_line).or_default().push(edit);
    }
    if line_to_edits.len() > 3 {
        return None;
    }

    let mut inline_patch = vec![format!(
        "patch: {}",
        display_path_for_raw(request, edit_path)
    )];

    for (line_no, edits) in line_to_edits {
        let original = load_source_line(request, node, edit_path, line_no)?;
        if !original.is_ascii() || original.contains('\t') {
            return None;
        }
        let updated = apply_edits_to_line(&original, &edits, &mut total_changed_chars)?;
        if original == updated {
            return None;
        }
        inline_patch.push(format!("{line_no} - {original}"));
        inline_patch.push(format!("{line_no} + {updated}"));
    }

    (total_changed_chars <= 80).then_some(inline_patch)
}

fn apply_edits_to_line(
    original: &str,
    edits: &[&TextEdit],
    total_changed_chars: &mut usize,
) -> Option<String> {
    let mut ordered = edits.to_vec();
    ordered.sort_by(|left, right| {
        right
            .start_column
            .cmp(&left.start_column)
            .then_with(|| right.end_column.cmp(&left.end_column))
    });

    let mut last_start = None::<u32>;
    let mut result = original.to_string();
    for edit in ordered {
        if edit.start_column == 0 || edit.end_column == 0 || edit.end_column < edit.start_column {
            return None;
        }
        if let Some(previous_start) = last_start
            && previous_start < edit.end_column
        {
            return None;
        }
        let start = usize::try_from(edit.start_column - 1).ok()?;
        let end = usize::try_from(edit.end_column - 1).ok()?;
        if end > original.len() || start > end {
            return None;
        }

        *total_changed_chars += (end - start) + edit.replacement.len();
        result.replace_range(start..end, &edit.replacement);
        last_start = Some(edit.start_column);
    }
    Some(result)
}

fn load_source_line(
    request: &RenderRequest,
    node: &DiagnosticNode,
    path: &str,
    line_no: u32,
) -> Option<String> {
    let resolved = resolved_path(request, path);
    if let Ok(content) = fs::read_to_string(&resolved) {
        let index = usize::try_from(line_no.saturating_sub(1)).ok()?;
        return content.lines().nth(index).map(|line| line.to_string());
    }

    node.locations
        .iter()
        .find(|location| {
            same_path(request, location.path_raw(), path) && location.line() == line_no
        })
        .and_then(|location| location.source_excerpt_ref.as_deref())
        .and_then(|source_excerpt_ref| source_snippet_line(request, source_excerpt_ref))
}

fn source_snippet_line(request: &RenderRequest, source_excerpt_ref: &str) -> Option<String> {
    let capture = request
        .document
        .captures
        .iter()
        .find(|capture| capture.id == source_excerpt_ref)?;

    if let Some(text) = capture.inline_text.as_ref() {
        return text.lines().next().map(|line| line.to_string());
    }

    capture
        .external_ref
        .as_ref()
        .and_then(|path| fs::read_to_string(path).ok())
        .and_then(|text| text.lines().next().map(|line| line.to_string()))
}

fn same_path(request: &RenderRequest, left: &str, right: &str) -> bool {
    if left == right {
        return true;
    }
    resolved_path(request, left) == resolved_path(request, right)
}

fn touches_user_owned_path(
    request: &RenderRequest,
    node: &DiagnosticNode,
    suggestion: &Suggestion,
) -> bool {
    suggestion.edits.iter().any(|edit| {
        node.locations.iter().any(|location| {
            same_path(request, location.path_raw(), &edit.path)
                && matches!(location.ownership(), Some(diag_core::Ownership::User))
        }) || Path::new(&edit.path).is_relative()
    })
}

fn is_single_file_edit(suggestion: &Suggestion) -> bool {
    unique_edit_paths(&suggestion.edits).len() <= 1
}

fn unique_edit_paths(edits: &[TextEdit]) -> Vec<&str> {
    let mut unique = BTreeSet::new();
    for edit in edits {
        unique.insert(edit.path.as_str());
    }
    unique.into_iter().collect()
}
