use crate::budget::budget_for;
use crate::path::{format_location, resolved_path};
use crate::{RenderProfile, RenderRequest, SourceExcerptPolicy};
use diag_core::{ArtifactKind, BoundarySemantics, DiagnosticNode, Location, Ownership};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

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

    Some(ExcerptBlock {
        location: format_location(request, location),
        lines: vec![display_line],
        annotations: excerpt_annotations(location, precise_annotation_possible),
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

fn excerpt_annotations(location: &Location, precise_annotation_possible: bool) -> Vec<String> {
    let start_column = location.column().max(1);
    let prefix = " ".repeat(start_column.saturating_sub(1) as usize);
    match location.range.as_ref() {
        None => vec![annotation_line(
            precise_annotation_possible,
            &prefix,
            "^",
            location.label.as_deref(),
            &format!("column {}", start_column),
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
                location.end_column().unwrap_or(start_column),
                range.boundary_semantics,
            );
            let summary = if marker == "^" {
                format!("column {}", start_column)
            } else {
                let end_column = location.end_column().unwrap_or(start_column);
                format!("columns {}-{}", start_column, end_column)
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
