use diag_core::{Suggestion, SuggestionApplicability, TextEdit};
use std::collections::BTreeSet;

pub(crate) fn suggestion_from_edits(
    label: Option<String>,
    applicability: SuggestionApplicability,
    edits: Vec<TextEdit>,
) -> Option<Suggestion> {
    if edits.is_empty() {
        return None;
    }

    Some(Suggestion {
        label: label
            .filter(|label| !label.trim().is_empty())
            .unwrap_or_else(|| summarize_text_edits(&edits)),
        applicability,
        edits,
    })
}

fn summarize_text_edits(edits: &[TextEdit]) -> String {
    if edits.len() == 1 {
        return summarize_single_edit(&edits[0]);
    }

    let files = edits
        .iter()
        .map(|edit| edit.path.as_str())
        .collect::<BTreeSet<_>>();
    if files.len() == 1 {
        format!(
            "apply {} coordinated edits in {}",
            edits.len(),
            files.iter().next().copied().unwrap_or("the source file")
        )
    } else {
        format!(
            "apply coordinated edits across {} files",
            files.len().max(1)
        )
    }
}

fn summarize_single_edit(edit: &TextEdit) -> String {
    if is_insertion(edit) {
        return format!("insert {}", quoted_snippet(&edit.replacement));
    }
    if is_deletion(edit) {
        return "remove text".to_string();
    }
    format!("replace text with {}", quoted_snippet(&edit.replacement))
}

fn is_insertion(edit: &TextEdit) -> bool {
    edit.start_line == edit.end_line
        && edit.start_column == edit.end_column
        && !edit.replacement.is_empty()
}

fn is_deletion(edit: &TextEdit) -> bool {
    edit.replacement.is_empty()
}

fn quoted_snippet(text: &str) -> String {
    if text.is_empty() {
        return "an empty string".to_string();
    }

    let escaped = text
        .chars()
        .flat_map(|ch| ch.escape_default())
        .collect::<String>();
    let preview = if escaped.chars().count() > 24 {
        let prefix = escaped.chars().take(21).collect::<String>();
        format!("{prefix}...")
    } else {
        escaped
    };
    format!("'{preview}'")
}
