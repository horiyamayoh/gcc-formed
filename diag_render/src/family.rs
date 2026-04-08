use crate::RenderProfile;
use crate::budget::budget_for;
use diag_core::{ContextChainKind, ContextFrame, DiagnosticNode};

#[derive(Debug, Default)]
pub struct SupportingEvidence {
    pub context_lines: Vec<String>,
    pub child_notes: Vec<String>,
    pub collapsed_notices: Vec<String>,
}

pub fn summarize_supporting_evidence(
    node: &DiagnosticNode,
    profile: RenderProfile,
) -> SupportingEvidence {
    let family = node
        .analysis
        .as_ref()
        .and_then(|analysis| analysis.family.as_deref())
        .unwrap_or("unknown");
    let budget = budget_for(profile);

    match family {
        "template" => summarize_template(node, budget.template_frames),
        "macro_include" => summarize_macro_include(node, budget.macro_include_frames),
        "type_overload" => summarize_overload(node, budget.candidate_notes),
        family if family.starts_with("linker") => summarize_linker(node),
        _ => summarize_generic(node, profile),
    }
}

fn summarize_template(node: &DiagnosticNode, frame_limit: usize) -> SupportingEvidence {
    let mut evidence = SupportingEvidence::default();
    let Some(chain) = node
        .context_chains
        .iter()
        .find(|chain| matches!(chain.kind, ContextChainKind::TemplateInstantiation))
    else {
        push_unique(
            &mut evidence.context_lines,
            "while instantiating: preserved template context".to_string(),
        );
        return evidence;
    };

    push_unique(
        &mut evidence.context_lines,
        "while instantiating:".to_string(),
    );
    let visible = summarize_frames(&chain.frames, frame_limit);
    for frame in &visible {
        push_unique(&mut evidence.context_lines, format!("  - {frame}"));
    }
    let unique_count = dedup_frames(&chain.frames).len();
    if unique_count > visible.len() {
        push_unique(
            &mut evidence.context_lines,
            format!(
                "omitted {} internal template frames",
                unique_count - visible.len()
            ),
        );
    }

    evidence
}

fn summarize_macro_include(node: &DiagnosticNode, frame_limit: usize) -> SupportingEvidence {
    let mut evidence = SupportingEvidence::default();
    let macro_chain = node
        .context_chains
        .iter()
        .find(|chain| matches!(chain.kind, ContextChainKind::MacroExpansion));
    let include_chain = node
        .context_chains
        .iter()
        .find(|chain| matches!(chain.kind, ContextChainKind::Include));

    if let Some(chain) = macro_chain {
        push_unique(
            &mut evidence.context_lines,
            "through macro expansion:".to_string(),
        );
        let visible = summarize_frames(&chain.frames, frame_limit);
        for frame in &visible {
            push_unique(&mut evidence.context_lines, format!("  - {frame}"));
        }
        let unique_count = dedup_frames(&chain.frames).len();
        if unique_count > visible.len() {
            push_unique(
                &mut evidence.context_lines,
                format!(
                    "  - omitted {} intermediate macro frames",
                    unique_count - visible.len()
                ),
            );
        }
    }

    if let Some(chain) = include_chain {
        push_unique(
            &mut evidence.context_lines,
            "from include chain:".to_string(),
        );
        let visible = summarize_frames(&chain.frames, frame_limit);
        for frame in &visible {
            push_unique(&mut evidence.context_lines, format!("  - {frame}"));
        }
        let unique_count = dedup_frames(&chain.frames).len();
        if unique_count > visible.len() {
            push_unique(
                &mut evidence.context_lines,
                format!(
                    "  - through {} intermediate includes",
                    unique_count - visible.len()
                ),
            );
        }
    }

    if evidence.context_lines.is_empty() {
        push_unique(
            &mut evidence.context_lines,
            "through macro expansion: preserved macro/include context".to_string(),
        );
    }

    evidence
}

fn summarize_overload(node: &DiagnosticNode, note_limit: usize) -> SupportingEvidence {
    let mut evidence = SupportingEvidence::default();
    let mut rendered = Vec::new();

    for child in &node.children {
        let note = child
            .message
            .raw_text
            .lines()
            .next()
            .unwrap_or_default()
            .trim()
            .to_string();
        if note.is_empty() {
            continue;
        }
        if note.starts_with("candidate ") {
            let location = child
                .primary_location()
                .map(|location| {
                    format!(
                        " at {}:{}:{}",
                        location.path, location.line, location.column
                    )
                })
                .unwrap_or_default();
            rendered.push(format!("because: {note}{location}"));
        } else {
            rendered.push(format!("because: {note}"));
        }
    }

    let mut unique_rendered = Vec::new();
    for line in rendered {
        push_unique(&mut unique_rendered, line);
    }

    let visible = unique_rendered
        .iter()
        .take(note_limit)
        .cloned()
        .collect::<Vec<_>>();
    for line in &visible {
        push_unique(&mut evidence.context_lines, line.clone());
    }
    if unique_rendered.len() > visible.len() {
        push_unique(
            &mut evidence.context_lines,
            format!(
                "omitted {} other overload notes",
                unique_rendered.len() - visible.len()
            ),
        );
    }

    evidence
}

fn summarize_linker(node: &DiagnosticNode) -> SupportingEvidence {
    let mut evidence = SupportingEvidence::default();
    if let Some(symbol) = node
        .symbol_context
        .as_ref()
        .and_then(|symbol_context| symbol_context.primary_symbol.as_deref())
    {
        push_unique(
            &mut evidence.context_lines,
            format!("linker: symbol `{symbol}`"),
        );
    } else {
        push_unique(
            &mut evidence.context_lines,
            "linker: original linker diagnostics are preserved".to_string(),
        );
    }

    if let Some(symbol_context) = node.symbol_context.as_ref() {
        for object in symbol_context.related_objects.iter().take(3) {
            push_unique(
                &mut evidence.context_lines,
                format!("linker: referenced from {object}"),
            );
        }
        if let Some(archive) = symbol_context.archive.as_deref() {
            push_unique(
                &mut evidence.context_lines,
                format!("linker: archive {archive}"),
            );
        }
    }

    evidence
}

fn summarize_generic(node: &DiagnosticNode, profile: RenderProfile) -> SupportingEvidence {
    let budget = budget_for(profile);
    let limit = match profile {
        RenderProfile::Verbose => usize::MAX,
        RenderProfile::RawFallback => 0,
        _ => 3,
    };
    let mut evidence = SupportingEvidence::default();

    for chain in &node.context_chains {
        let label = match chain.kind {
            ContextChainKind::TemplateInstantiation => "template",
            ContextChainKind::MacroExpansion => "macro",
            ContextChainKind::Include => "include",
            ContextChainKind::LinkerResolution => "linker",
            ContextChainKind::AnalyzerPath => "path",
            ContextChainKind::Other => "context",
        };
        if chain.frames.is_empty() {
            push_unique(&mut evidence.context_lines, format!("{label}: preserved"));
            continue;
        }
        for frame in chain.frames.iter().take(limit) {
            push_unique(
                &mut evidence.context_lines,
                format!("{label}: {}", format_frame(frame)),
            );
        }
        if chain.frames.len() > limit {
            push_unique(
                &mut evidence.context_lines,
                format!("omitted {} {label} frames", chain.frames.len() - limit),
            );
        }
    }

    let mut notes = Vec::new();
    for child in &node.children {
        let note = child
            .message
            .raw_text
            .lines()
            .next()
            .unwrap_or_default()
            .trim();
        if !note.is_empty() {
            push_unique(&mut notes, note.to_string());
        }
    }
    let note_limit = budget.candidate_notes.min(limit);
    for note in notes.iter().take(note_limit) {
        push_unique(&mut evidence.child_notes, note.clone());
    }
    if notes.len() > evidence.child_notes.len() {
        push_unique(
            &mut evidence.collapsed_notices,
            format!(
                "omitted {} additional note(s)",
                notes.len() - evidence.child_notes.len()
            ),
        );
    }

    evidence
}

fn summarize_frames(frames: &[ContextFrame], frame_limit: usize) -> Vec<String> {
    if frames.is_empty() || frame_limit == 0 {
        return Vec::new();
    }

    let unique = dedup_frames(frames);
    if unique.len() <= frame_limit {
        return unique
            .into_iter()
            .map(|frame| format_frame(&frame))
            .collect();
    }

    let mut visible = Vec::new();
    if let Some(first) = unique.first() {
        visible.push(format_frame(first));
    }
    if frame_limit > 1 {
        for frame in unique.iter().skip(1).take(frame_limit.saturating_sub(2)) {
            visible.push(format_frame(frame));
        }
        if let Some(last) = unique.last() {
            let rendered_last = format_frame(last);
            if !visible.iter().any(|entry| entry == &rendered_last) {
                visible.push(rendered_last);
            }
        }
    }
    visible
}

fn dedup_frames(frames: &[ContextFrame]) -> Vec<ContextFrame> {
    let mut unique = Vec::new();
    for frame in frames {
        if !unique.iter().any(|existing| existing == frame) {
            unique.push(frame.clone());
        }
    }
    unique
}

fn format_frame(frame: &ContextFrame) -> String {
    let mut label = frame.label.trim().trim_end_matches(',').to_string();
    if let Some(stripped) = label.strip_prefix("note: ") {
        label = stripped.trim().to_string();
    }
    let mut rendered = String::new();
    if let Some(path) = frame.path.as_deref() {
        let prefix = match (frame.line, frame.column) {
            (Some(line), Some(column)) => Some(format!("{path}:{line}:{column}")),
            (Some(line), None) => Some(format!("{path}:{line}")),
            (None, _) => Some(path.to_string()),
        };
        if let Some(prefix) = prefix.as_deref() {
            if let Some(stripped) = label.strip_prefix(prefix) {
                label = stripped.trim_start_matches(':').trim().to_string();
            }
        }
        if let Some(stripped) = label.strip_prefix("note: ") {
            label = stripped.trim().to_string();
        }
        if label.starts_with("In file included from ") {
            return label;
        }
        rendered.push_str(path);
        if let Some(line) = frame.line {
            rendered.push(':');
            rendered.push_str(&line.to_string());
            if let Some(column) = frame.column {
                rendered.push(':');
                rendered.push_str(&column.to_string());
            }
        }
        rendered.push(' ');
    }
    rendered.push_str(&label);
    rendered
}

fn push_unique(lines: &mut Vec<String>, line: String) {
    if !lines.iter().any(|existing| existing == &line) {
        lines.push(line);
    }
}
