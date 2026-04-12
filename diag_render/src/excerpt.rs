use crate::budget::budget_for;
use crate::path::{format_location, resolved_path};
use crate::{RenderProfile, RenderRequest, SourceExcerptPolicy};
use diag_core::{ArtifactKind, BoundarySemantics, DiagnosticNode, Location, Ownership};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

const DEFAULT_EXCERPT_WIDTH: usize = 100;
const EXCERPT_LINE_PREFIX_WIDTH: usize = 2;
const ELLIPSIS: &str = "...";

/// A rendered source code excerpt block with annotations.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExcerptBlock {
    /// Formatted file location string (e.g. `src/main.c:2:12`).
    pub location: String,
    /// Source lines included in the excerpt.
    pub lines: Vec<String>,
    /// Caret/range annotations aligned beneath the source lines.
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub annotations: Vec<String>,
}

#[derive(Debug, Clone, Copy)]
struct AnnotationColumns {
    start: u32,
    end: u32,
}

#[derive(Debug, Clone)]
struct WindowedSourceLine {
    text: String,
    columns: AnnotationColumns,
}

/// Loads source code excerpts for the primary locations of a diagnostic node.
pub fn load_excerpt(request: &RenderRequest, node: &DiagnosticNode) -> Vec<ExcerptBlock> {
    if matches!(request.source_excerpt_policy, SourceExcerptPolicy::ForceOff) {
        return Vec::new();
    }
    let limit = match request.profile {
        RenderProfile::RawFallback => 0,
        _ if matches!(request.source_excerpt_policy, SourceExcerptPolicy::ForceOn) => usize::MAX,
        _ => budget_for(request.profile).source_excerpts,
    };
    let mut locations = node.locations.iter().enumerate().collect::<Vec<_>>();
    locations.sort_by(|left, right| {
        excerpt_rank(request, right.1)
            .cmp(&excerpt_rank(request, left.1))
            .then_with(|| left.0.cmp(&right.0))
    });
    let mut excerpts = Vec::new();
    for (_, location) in locations {
        if excerpts.len() >= limit {
            break;
        }
        if let Some(excerpt) = build_excerpt_block(request, location) {
            excerpts.push(excerpt);
        }
    }
    excerpts
}

pub(crate) fn source_line_text(request: &RenderRequest, location: &Location) -> Option<String> {
    let (content, snippet_backed) = excerpt_source_text(request, location)?;
    let line_index = usize::try_from(location.line().saturating_sub(1)).ok()?;
    let line = if snippet_backed {
        content
            .lines()
            .nth(line_index)
            .or_else(|| content.lines().next())?
    } else {
        content.lines().nth(line_index)?
    };
    Some(line.to_string())
}

fn build_excerpt_block(request: &RenderRequest, location: &Location) -> Option<ExcerptBlock> {
    let source_line = source_line_text(request, location)?;
    let (display_line, precise_annotation_possible) = renderable_source_line(&source_line);
    let windowed_line = window_excerpt_line(request, location, &display_line);

    Some(ExcerptBlock {
        location: format_location(request, location),
        lines: vec![windowed_line.text],
        annotations: excerpt_annotations(
            location,
            precise_annotation_possible,
            windowed_line.columns,
        ),
    })
}

fn excerpt_source_text(request: &RenderRequest, location: &Location) -> Option<(String, bool)> {
    if let Some(source_excerpt_ref) = location.source_excerpt_ref.as_deref()
        && let Some(text) = source_snippet_text(request, source_excerpt_ref)
    {
        return Some((text, true));
    }

    let resolved_path = resolved_path(request, location.path_raw());
    fs::read_to_string(&resolved_path)
        .ok()
        .map(|content| (content, false))
}

fn source_snippet_text(request: &RenderRequest, source_excerpt_ref: &str) -> Option<String> {
    let capture = request.document.captures.iter().find(|capture| {
        capture.id == source_excerpt_ref && matches!(capture.kind, ArtifactKind::SourceSnippet)
    })?;
    if let Some(text) = capture.inline_text.as_ref() {
        return Some(text.clone());
    }
    capture
        .external_ref
        .as_ref()
        .and_then(|path| fs::read(path).ok())
        .map(|bytes| String::from_utf8_lossy(&bytes).into_owned())
}

fn excerpt_rank(request: &RenderRequest, location: &diag_core::Location) -> u8 {
    match location.ownership() {
        Some(Ownership::User) => 4,
        Some(Ownership::Vendor) => 3,
        Some(Ownership::Generated) => 2,
        Some(Ownership::System) => 1,
        None if looks_workspace_owned(request, location.path_raw()) => 3,
        _ => 0,
    }
}

fn looks_workspace_owned(request: &RenderRequest, path: &str) -> bool {
    let path = Path::new(path);
    path.is_relative()
        || request
            .cwd
            .as_ref()
            .is_some_and(|cwd| path.strip_prefix(cwd).is_ok())
}

fn renderable_source_line(source_line: &str) -> (String, bool) {
    if source_line.is_ascii() && !source_line.contains('\t') {
        return (source_line.to_string(), true);
    }
    if !source_line.is_ascii() {
        return (source_line.to_string(), false);
    }
    (expand_tabs(source_line), true)
}

fn expand_tabs(source_line: &str) -> String {
    let mut expanded = String::new();
    let mut column = 0usize;
    for ch in source_line.chars() {
        if ch == '\t' {
            let tab_width = 8 - (column % 8);
            expanded.push_str(&" ".repeat(tab_width));
            column += tab_width;
        } else {
            expanded.push(ch);
            column += 1;
        }
    }
    expanded
}

fn window_excerpt_line(
    request: &RenderRequest,
    location: &Location,
    display_line: &str,
) -> WindowedSourceLine {
    let columns = annotation_columns(location);
    let max_width = excerpt_window_width(request);
    if display_line.chars().count() <= max_width {
        return WindowedSourceLine {
            text: display_line.to_string(),
            columns,
        };
    }

    let highlight_end = highlight_end_column(location);
    let highlight_width = highlight_end
        .saturating_sub(columns.start)
        .saturating_add(1) as usize;
    let min_window_body_width = ELLIPSIS.len() * 2 + 1;
    if max_width <= min_window_body_width || highlight_width + ELLIPSIS.len() * 2 >= max_width {
        return WindowedSourceLine {
            text: display_line.to_string(),
            columns,
        };
    }

    let line_chars = display_line.chars().collect::<Vec<_>>();
    let line_len = line_chars.len();
    let body_width = max_width.saturating_sub(ELLIPSIS.len() * 2);
    let highlight_start = columns.start.saturating_sub(1) as usize;
    let highlight_end = highlight_end.saturating_sub(1) as usize;
    let available_context = body_width.saturating_sub(highlight_width);
    let desired_left_context = available_context / 2;

    let mut window_start = highlight_start.saturating_sub(desired_left_context);
    let min_window_end = highlight_end.saturating_add(1).min(line_len);
    let mut window_end = window_start.saturating_add(body_width).min(line_len);
    if window_end < min_window_end {
        window_end = min_window_end;
        window_start = window_end.saturating_sub(body_width);
    }
    if window_end == line_len {
        window_start = line_len.saturating_sub(body_width);
    }

    let left_trimmed = window_start > 0;
    let right_trimmed = window_end < line_len;
    let mut text = String::new();
    if left_trimmed {
        text.push_str(ELLIPSIS);
    }
    text.extend(line_chars[window_start..window_end].iter());
    if right_trimmed {
        text.push_str(ELLIPSIS);
    }

    let display_start = if left_trimmed {
        ELLIPSIS.len() as u32 + 1
    } else {
        1
    };
    let columns = AnnotationColumns {
        start: display_start + (highlight_start.saturating_sub(window_start) as u32),
        end: display_start
            + (columns
                .end
                .saturating_sub(1)
                .saturating_sub(window_start as u32)),
    };

    WindowedSourceLine { text, columns }
}

fn excerpt_window_width(request: &RenderRequest) -> usize {
    request
        .capabilities
        .width_columns
        .map(|width| width.saturating_sub(EXCERPT_LINE_PREFIX_WIDTH).max(1))
        .unwrap_or(DEFAULT_EXCERPT_WIDTH.saturating_sub(EXCERPT_LINE_PREFIX_WIDTH))
}

fn annotation_columns(location: &Location) -> AnnotationColumns {
    let start = location.column().max(1);
    AnnotationColumns {
        start,
        end: marker_end_input(location),
    }
}

fn highlight_end_column(location: &Location) -> u32 {
    let start_column = location.column().max(1);
    let Some(range) = location.range.as_ref() else {
        return start_column;
    };
    if range.start.line != range.end.line {
        return start_column;
    }
    let end_column = location.end_column().unwrap_or(start_column);
    match range.boundary_semantics {
        BoundarySemantics::InclusiveEnd => end_column.max(start_column),
        BoundarySemantics::Point => start_column,
        BoundarySemantics::HalfOpen | BoundarySemantics::Unknown => {
            end_column.saturating_sub(1).max(start_column)
        }
    }
}

fn marker_end_input(location: &Location) -> u32 {
    let start_column = location.column().max(1);
    let Some(range) = location.range.as_ref() else {
        return start_column;
    };
    if range.start.line != range.end.line {
        return start_column;
    }
    match range.boundary_semantics {
        BoundarySemantics::Point => start_column,
        BoundarySemantics::InclusiveEnd
        | BoundarySemantics::HalfOpen
        | BoundarySemantics::Unknown => location.end_column().unwrap_or(start_column),
    }
}

fn excerpt_annotations(
    location: &Location,
    precise_annotation_possible: bool,
    columns: AnnotationColumns,
) -> Vec<String> {
    let start_column = columns.start.max(1);
    let prefix = " ".repeat(start_column.saturating_sub(1) as usize);
    match location.range.as_ref() {
        None => vec![annotation_line(
            precise_annotation_possible,
            &prefix,
            "^",
            location.label.as_deref(),
            &format!("column {}", location.column().max(1)),
        )],
        Some(range) if range.start.line != range.end.line => {
            let span_lines = range
                .end
                .line
                .saturating_sub(range.start.line)
                .saturating_add(1);
            let end_column = location.end_column().unwrap_or(start_column);
            vec![range_summary_annotation(
                precise_annotation_possible,
                &prefix,
                location.label.as_deref(),
                &format!(
                    "range spans {span_lines} lines to {}:{}",
                    range.end.line, end_column
                ),
            )]
        }
        Some(range) => {
            let marker = range_marker(
                start_column,
                columns.end.max(start_column),
                range.boundary_semantics,
            );
            let summary = if marker == "^" {
                format!("column {}", location.column().max(1))
            } else {
                let end_column = location.end_column().unwrap_or(start_column);
                format!("columns {}-{}", location.column().max(1), end_column)
            };
            vec![annotation_line(
                precise_annotation_possible,
                &prefix,
                &marker,
                location.label.as_deref(),
                &summary,
            )]
        }
    }
}

fn annotation_line(
    precise_annotation_possible: bool,
    prefix: &str,
    marker: &str,
    label: Option<&str>,
    summary: &str,
) -> String {
    if precise_annotation_possible {
        let mut line = format!("{prefix}{marker}");
        if let Some(label) = label {
            line.push(' ');
            line.push_str(label);
        }
        return line;
    }

    let mut line = summary.to_string();
    if let Some(label) = label {
        line.push_str(" (");
        line.push_str(label);
        line.push(')');
    }
    line
}

fn range_summary_annotation(
    precise_annotation_possible: bool,
    prefix: &str,
    label: Option<&str>,
    summary: &str,
) -> String {
    let mut line = if precise_annotation_possible {
        format!("{prefix}^ {summary}")
    } else {
        summary.to_string()
    };
    if let Some(label) = label {
        if precise_annotation_possible {
            line.push(' ');
            line.push_str(label);
        } else {
            line.push_str(" (");
            line.push_str(label);
            line.push(')');
        }
    }
    line
}

fn range_marker(start_column: u32, end_column: u32, semantics: BoundarySemantics) -> String {
    let width = match semantics {
        BoundarySemantics::InclusiveEnd => {
            end_column.saturating_sub(start_column).saturating_add(1)
        }
        BoundarySemantics::Point => 1,
        BoundarySemantics::HalfOpen | BoundarySemantics::Unknown => {
            end_column.saturating_sub(start_column)
        }
    }
    .max(1);
    if width <= 1 {
        "^".to_string()
    } else {
        format!("^{}", "~".repeat(width.saturating_sub(1) as usize))
    }
}
