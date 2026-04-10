use crate::RenderRequest;
use crate::budget::budget_for;
use diag_core::{
    Confidence, ContextChainKind, ContextFrame, DiagnosticNode, DocumentCompleteness,
    NodeCompleteness, Ownership, ProvenanceSource,
};
use std::path::Path;

#[derive(Debug, Default)]
pub struct SupportingEvidence {
    pub context_lines: Vec<String>,
    pub child_notes: Vec<String>,
    pub collapsed_notices: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RendererFamilyKind {
    Unknown,
    Syntax,
    Template,
    MacroInclude,
    TypeOverload,
    Linker,
}

pub(crate) fn renderer_family_kind(node: &DiagnosticNode) -> RendererFamilyKind {
    match node
        .analysis
        .as_ref()
        .and_then(|analysis| analysis.family.as_deref())
    {
        Some("syntax") => RendererFamilyKind::Syntax,
        Some("template") => RendererFamilyKind::Template,
        Some("macro_include") => RendererFamilyKind::MacroInclude,
        Some("type_overload") => RendererFamilyKind::TypeOverload,
        Some(family)
            if family.starts_with("linker.") && family != "linker.file_format_or_relocation" =>
        {
            RendererFamilyKind::Linker
        }
        _ => RendererFamilyKind::Unknown,
    }
}

pub(crate) fn is_conservative_useful_subset_card(
    request: &RenderRequest,
    node: &DiagnosticNode,
) -> bool {
    matches!(
        renderer_family_kind(node),
        RendererFamilyKind::Syntax
            | RendererFamilyKind::Template
            | RendererFamilyKind::TypeOverload
            | RendererFamilyKind::Linker
    ) && matches!(band_c_gcc_major(request), Some(9..=12))
        && matches!(
            request.document.document_completeness,
            DocumentCompleteness::Partial
        )
        && matches!(
            node.node_completeness,
            NodeCompleteness::Partial | NodeCompleteness::Passthrough
        )
        && matches!(node.provenance.source, ProvenanceSource::ResidualText)
        && matches!(
            node.analysis
                .as_ref()
                .and_then(|analysis| analysis.confidence.as_ref()),
            Some(Confidence::Low) | Some(Confidence::Unknown) | None
        )
}

pub fn summarize_supporting_evidence(
    request: &RenderRequest,
    node: &DiagnosticNode,
) -> SupportingEvidence {
    let budget = budget_for(request.profile);
    let conservative_useful_subset = is_conservative_useful_subset_card(request, node);

    match renderer_family_kind(node) {
        RendererFamilyKind::Template => summarize_template(
            request,
            node,
            constrained_template_frames(
                request,
                budget.template_frames,
                conservative_useful_subset,
            ),
        ),
        RendererFamilyKind::MacroInclude => {
            summarize_macro_include(request, node, budget.macro_include_frames)
        }
        RendererFamilyKind::TypeOverload => summarize_overload(
            request,
            node,
            constrained_candidate_notes(
                request,
                budget.candidate_notes,
                conservative_useful_subset,
            ),
            conservative_useful_subset,
        ),
        RendererFamilyKind::Linker => summarize_linker(request, node, conservative_useful_subset),
        _ => summarize_generic(request, node),
    }
}

fn summarize_template(
    request: &RenderRequest,
    node: &DiagnosticNode,
    frame_limit: usize,
) -> SupportingEvidence {
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
    let visible = summarize_frames(request, node, &chain.frames, frame_limit);
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

fn summarize_macro_include(
    request: &RenderRequest,
    node: &DiagnosticNode,
    frame_limit: usize,
) -> SupportingEvidence {
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
        let visible = summarize_frames(request, node, &chain.frames, frame_limit);
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
        let visible = summarize_frames(request, node, &chain.frames, frame_limit);
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

fn summarize_overload(
    request: &RenderRequest,
    node: &DiagnosticNode,
    note_limit: usize,
    conservative: bool,
) -> SupportingEvidence {
    let mut evidence = SupportingEvidence::default();
    let mut rendered = node
        .children
        .iter()
        .enumerate()
        .filter_map(|(index, child)| {
            let note = normalized_child_note(request, child);
            if note.is_empty() {
                return None;
            }
            let location = best_location(request, child)
                .map(|location| {
                    format!(
                        " at {}:{}:{}",
                        location.path, location.line, location.column
                    )
                })
                .unwrap_or_default();
            let rendered = if conservative && note.starts_with("candidate ") {
                format!("{note}{location}")
            } else if conservative {
                format!("compiler note: {note}{location}")
            } else if note.starts_with("candidate ") {
                format!("because: {note}{location}")
            } else {
                format!("because: {note}")
            };
            Some((child_rank(request, child), index, rendered))
        })
        .collect::<Vec<_>>();
    rendered.sort_by(|left, right| right.0.cmp(&left.0).then_with(|| left.1.cmp(&right.1)));

    let mut unique_rendered = Vec::new();
    for (_, _, line) in rendered {
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

fn summarize_linker(
    request: &RenderRequest,
    node: &DiagnosticNode,
    conservative: bool,
) -> SupportingEvidence {
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
        let mut related_objects = symbol_context.related_objects.clone();
        related_objects.sort_by(|left, right| {
            linker_object_rank(request, right)
                .cmp(&linker_object_rank(request, left))
                .then_with(|| left.cmp(right))
        });
        for object in related_objects
            .into_iter()
            .take(linker_object_limit(request, conservative))
        {
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

fn summarize_generic(request: &RenderRequest, node: &DiagnosticNode) -> SupportingEvidence {
    let budget = budget_for(request.profile);
    let limit = match request.profile {
        crate::RenderProfile::Verbose => usize::MAX,
        crate::RenderProfile::RawFallback => 0,
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
        for frame in summarize_frames(request, node, &chain.frames, limit) {
            push_unique(&mut evidence.context_lines, format!("{label}: {frame}"));
        }
        let unique_count = dedup_frames(&chain.frames).len();
        if unique_count > limit {
            push_unique(
                &mut evidence.context_lines,
                format!("omitted {} {label} frames", unique_count - limit),
            );
        }
    }

    let mut notes = node
        .children
        .iter()
        .enumerate()
        .filter_map(|(index, child)| {
            let note = child
                .message
                .raw_text
                .lines()
                .next()
                .unwrap_or_default()
                .trim();
            (!note.is_empty()).then(|| (child_rank(request, child), index, note.to_string()))
        })
        .collect::<Vec<_>>();
    notes.sort_by(|left, right| right.0.cmp(&left.0).then_with(|| left.1.cmp(&right.1)));
    let note_limit = budget.candidate_notes.min(limit);
    for (_, _, note) in notes.iter().take(note_limit) {
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

fn summarize_frames(
    request: &RenderRequest,
    node: &DiagnosticNode,
    frames: &[ContextFrame],
    frame_limit: usize,
) -> Vec<String> {
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

    let mut selected = Vec::new();
    push_index(&mut selected, 0);
    for (index, frame) in unique.iter().enumerate() {
        if frame
            .path
            .as_deref()
            .is_some_and(|path| is_user_owned_path(request, node, path))
        {
            push_index(&mut selected, index);
        }
    }
    push_index(&mut selected, unique.len().saturating_sub(1));
    for index in 0..unique.len() {
        push_index(&mut selected, index);
        if selected.len() == frame_limit {
            break;
        }
    }
    selected.truncate(frame_limit);
    selected.sort_by(|left, right| {
        frame_rank(request, node, &unique[*right])
            .cmp(&frame_rank(request, node, &unique[*left]))
            .then_with(|| left.cmp(right))
    });
    selected
        .into_iter()
        .map(|index| format_frame(&unique[index]))
        .collect()
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

fn child_rank(request: &RenderRequest, node: &DiagnosticNode) -> u8 {
    node.locations
        .iter()
        .map(|location| ownership_rank(request, &location.path, location.ownership.as_ref()))
        .max()
        .unwrap_or(0)
}

fn best_location<'a>(
    request: &RenderRequest,
    node: &'a DiagnosticNode,
) -> Option<&'a diag_core::Location> {
    node.locations
        .iter()
        .enumerate()
        .max_by_key(|(index, location)| {
            (
                ownership_rank(request, &location.path, location.ownership.as_ref()),
                u8::from(*index == 0),
            )
        })
        .map(|(_, location)| location)
}

fn normalized_child_note(request: &RenderRequest, node: &DiagnosticNode) -> String {
    let mut note = node
        .message
        .raw_text
        .lines()
        .next()
        .unwrap_or_default()
        .trim()
        .to_string();
    if let Some(location) = best_location(request, node) {
        let prefix = format!("{}:{}:{}:", location.path, location.line, location.column);
        if let Some(stripped) = note.strip_prefix(&prefix) {
            note = stripped.trim().to_string();
        }
    }
    if let Some(stripped) = note.strip_prefix("note:") {
        note = stripped.trim().to_string();
    }
    note
}

fn ownership_rank(request: &RenderRequest, path: &str, ownership: Option<&Ownership>) -> u8 {
    match ownership {
        Some(Ownership::User) => 4,
        Some(Ownership::Vendor) => 3,
        Some(Ownership::Generated) => 2,
        Some(Ownership::System) => 1,
        None if looks_workspace_owned(request, path) => 3,
        _ => 0,
    }
}

fn is_user_owned_path(request: &RenderRequest, node: &DiagnosticNode, path: &str) -> bool {
    node.locations.iter().any(|location| {
        location.path == path && matches!(location.ownership, Some(Ownership::User))
    }) || looks_workspace_owned(request, path)
}

fn looks_workspace_owned(request: &RenderRequest, path: &str) -> bool {
    let path = Path::new(path);
    path.is_relative()
        || request
            .cwd
            .as_ref()
            .is_some_and(|cwd| path.strip_prefix(cwd).is_ok())
        || path
            .components()
            .next()
            .is_some_and(|component| component.as_os_str() == "src")
}

fn linker_object_rank(request: &RenderRequest, object: &str) -> u8 {
    if object.contains("/tmp/cc") || object.contains("/var/folders/") {
        return 0;
    }
    if looks_workspace_owned(request, object) {
        return 4;
    }
    if object.ends_with(".c")
        || object.ends_with(".cc")
        || object.ends_with(".cpp")
        || object.ends_with(".cxx")
    {
        return 3;
    }
    if object.ends_with(".o") || object.ends_with(".a") {
        return 2;
    }
    1
}

fn band_c_gcc_major(request: &RenderRequest) -> Option<u32> {
    request
        .document
        .run
        .primary_tool
        .version
        .as_deref()
        .and_then(|version| {
            version
                .split(|ch: char| !ch.is_ascii_digit())
                .find(|part| !part.is_empty())
        })
        .and_then(|major| major.parse().ok())
}

fn constrained_template_frames(
    request: &RenderRequest,
    default_limit: usize,
    conservative: bool,
) -> usize {
    if !conservative {
        return default_limit;
    }

    default_limit.min(match request.profile {
        crate::RenderProfile::Verbose => 6,
        crate::RenderProfile::Default => 3,
        crate::RenderProfile::Concise | crate::RenderProfile::Ci => 2,
        crate::RenderProfile::RawFallback => 0,
    })
}

fn constrained_candidate_notes(
    request: &RenderRequest,
    default_limit: usize,
    conservative: bool,
) -> usize {
    if !conservative {
        return default_limit;
    }

    default_limit.min(match request.profile {
        crate::RenderProfile::Verbose => 3,
        crate::RenderProfile::Default => 2,
        crate::RenderProfile::Concise | crate::RenderProfile::Ci => 1,
        crate::RenderProfile::RawFallback => 0,
    })
}

fn linker_object_limit(request: &RenderRequest, conservative: bool) -> usize {
    if !conservative {
        return 3;
    }

    match request.profile {
        crate::RenderProfile::Verbose => 2,
        crate::RenderProfile::Default
        | crate::RenderProfile::Concise
        | crate::RenderProfile::Ci => 1,
        crate::RenderProfile::RawFallback => 0,
    }
}

fn push_index(indices: &mut Vec<usize>, index: usize) {
    if !indices.iter().any(|existing| *existing == index) {
        indices.push(index);
    }
}

fn frame_rank(request: &RenderRequest, node: &DiagnosticNode, frame: &ContextFrame) -> u8 {
    u8::from(
        frame
            .path
            .as_deref()
            .is_some_and(|path| is_user_owned_path(request, node, path)),
    )
}

fn push_unique(lines: &mut Vec<String>, line: String) {
    if !lines.iter().any(|existing| existing == &line) {
        lines.push(line);
    }
}
