//! Canonicalization helpers for compiler-version-stable note clusters.

use crate::classify::{is_numbered_candidate_message, structured_message_text};
use serde_json::Value;

pub(crate) fn canonicalize_sarif_result(result: &Value) -> Value {
    let mut canonical = result.clone();
    let Some(root_message) = structured_message_text(canonical.get("message")) else {
        return canonical;
    };
    let Some(related_locations) = canonical
        .get_mut("relatedLocations")
        .and_then(Value::as_array_mut)
    else {
        return canonical;
    };
    canonicalize_message_cluster(&root_message, related_locations, |entry| {
        structured_message_text(entry.get("message"))
    });
    canonical
}

pub(crate) fn canonicalize_gcc_json_diagnostic(diagnostic: &Value) -> Value {
    let mut canonical = diagnostic.clone();
    canonicalize_gcc_json_diagnostic_in_place(&mut canonical);
    canonical
}

fn canonicalize_gcc_json_diagnostic_in_place(diagnostic: &mut Value) {
    let root_message = structured_message_text(diagnostic.get("message"));
    if let (Some(root_message), Some(children)) = (
        root_message,
        diagnostic.get_mut("children").and_then(Value::as_array_mut),
    ) {
        canonicalize_message_cluster(&root_message, children, |entry| {
            structured_message_text(entry.get("message"))
        });
        for child in children.iter_mut().filter(|child| child.is_object()) {
            canonicalize_gcc_json_diagnostic_in_place(child);
        }
    }
}

fn canonicalize_message_cluster<F>(root_message: &str, notes: &mut Vec<Value>, message_for: F)
where
    F: Fn(&Value) -> Option<String>,
{
    let Some(call_argument_count) = call_argument_count(root_message) else {
        return;
    };

    let mut replacements = Vec::new();
    let mut removals = Vec::new();
    let messages = notes.iter().map(&message_for).collect::<Vec<_>>();
    let mut index = 0;

    while index < messages.len() {
        let Some(message) = messages[index].as_deref() else {
            index += 1;
            continue;
        };
        let Some(candidate_argument_count) = candidate_argument_count(message) else {
            index += 1;
            continue;
        };
        if candidate_argument_count == call_argument_count {
            index += 1;
            continue;
        }

        let Some(next_message) = messages
            .get(index + 1)
            .and_then(|message| message.as_deref())
        else {
            index += 1;
            continue;
        };
        let normalized_message = format_candidate_expects_message(
            leading_whitespace(next_message),
            candidate_argument_count,
            call_argument_count,
        );

        if is_candidate_expects_message(next_message) {
            replacements.push((index + 1, normalized_message));
            index += 2;
            continue;
        }

        if !is_template_deduction_failed_message(next_message) {
            index += 1;
            continue;
        }

        replacements.push((index + 1, normalized_message));
        if let Some(detail_message) = messages
            .get(index + 2)
            .and_then(|message| message.as_deref())
            && should_collapse_supporting_detail(detail_message)
        {
            removals.push(index + 2);
            index += 3;
            continue;
        }

        index += 2;
    }

    for (index, message) in replacements {
        set_message_text(&mut notes[index], &message);
    }
    removals.sort_unstable();
    removals.dedup();
    for index in removals.into_iter().rev() {
        notes.remove(index);
    }
}

fn call_argument_count(message: &str) -> Option<usize> {
    let lowered = message.to_ascii_lowercase();
    if !lowered.contains("no matching function for call to") {
        return None;
    }
    let argument_list = first_parenthesized_contents(message)?;
    Some(count_top_level_arguments(argument_list))
}

fn candidate_argument_count(message: &str) -> Option<usize> {
    if !is_candidate_message(message) {
        return None;
    }
    let argument_list = first_parenthesized_contents(message)?;
    Some(count_top_level_arguments(argument_list))
}

fn is_candidate_message(message: &str) -> bool {
    let trimmed = message.trim_start();
    trimmed.starts_with("candidate:") || is_numbered_candidate_message(trimmed)
}

fn is_candidate_expects_message(message: &str) -> bool {
    let trimmed = message.trim_start().to_ascii_lowercase();
    trimmed.starts_with("candidate expects ") && trimmed.ends_with(" provided")
}

fn is_template_deduction_failed_message(message: &str) -> bool {
    message
        .trim_start()
        .eq_ignore_ascii_case("template argument deduction/substitution failed:")
}

fn should_collapse_supporting_detail(message: &str) -> bool {
    let trimmed = message.trim_start();
    if trimmed.is_empty() || is_candidate_message(trimmed) {
        return false;
    }

    let lowered = trimmed.to_ascii_lowercase();
    lowered.starts_with("mismatched types ")
        || lowered.starts_with("deduced conflicting ")
        || lowered.starts_with("couldn")
        || lowered.starts_with("no known conversion ")
}

fn leading_whitespace(message: &str) -> &str {
    let trimmed_len = message.trim_start().len();
    &message[..message.len() - trimmed_len]
}

fn format_candidate_expects_message(
    indent: &str,
    candidate_arity: usize,
    call_arity: usize,
) -> String {
    let plural = if candidate_arity == 1 { "" } else { "s" };
    format!("{indent}candidate expects {candidate_arity} argument{plural}, {call_arity} provided")
}

fn set_message_text(entry: &mut Value, new_text: &str) {
    match entry.get_mut("message") {
        Some(Value::String(message)) => {
            *message = new_text.to_string();
        }
        Some(Value::Object(message)) => {
            message.insert("text".to_string(), Value::String(new_text.to_string()));
        }
        _ => {}
    }
}

fn first_parenthesized_contents(text: &str) -> Option<&str> {
    let start = text.find('(')?;
    let mut depth = 0usize;
    let mut content_start = None;

    for (offset, ch) in text[start..].char_indices() {
        let index = start + offset;
        match ch {
            '(' => {
                depth += 1;
                if depth == 1 {
                    content_start = Some(index + ch.len_utf8());
                }
            }
            ')' => {
                if depth == 0 {
                    return None;
                }
                depth -= 1;
                if depth == 0 {
                    return content_start.map(|content_start| &text[content_start..index]);
                }
            }
            _ => {}
        }
    }

    None
}

fn count_top_level_arguments(argument_list: &str) -> usize {
    let trimmed = argument_list.trim();
    if trimmed.is_empty() {
        return 0;
    }

    let mut angle_depth = 0usize;
    let mut paren_depth = 0usize;
    let mut bracket_depth = 0usize;
    let mut brace_depth = 0usize;
    let mut single_quote = false;
    let mut double_quote = false;
    let mut previous_was_escape = false;
    let mut count = 1usize;

    for ch in trimmed.chars() {
        if single_quote || double_quote {
            if ch == '\\' && !previous_was_escape {
                previous_was_escape = true;
                continue;
            }
            if ch == '\'' && single_quote && !previous_was_escape {
                single_quote = false;
            } else if ch == '"' && double_quote && !previous_was_escape {
                double_quote = false;
            }
            previous_was_escape = false;
            continue;
        }

        match ch {
            '\'' => single_quote = true,
            '"' => double_quote = true,
            '<' => angle_depth += 1,
            '>' if angle_depth > 0 => angle_depth -= 1,
            '(' => paren_depth += 1,
            ')' if paren_depth > 0 => paren_depth -= 1,
            '[' => bracket_depth += 1,
            ']' if bracket_depth > 0 => bracket_depth -= 1,
            '{' => brace_depth += 1,
            '}' if brace_depth > 0 => brace_depth -= 1,
            ',' if angle_depth == 0
                && paren_depth == 0
                && bracket_depth == 0
                && brace_depth == 0 =>
            {
                count += 1;
            }
            _ => {}
        }
    }

    count
}
