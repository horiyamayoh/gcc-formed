use crate::budget::budget_for;
use crate::path::format_location;
use crate::{RenderProfile, RenderRequest};
use diag_core::{
    ContextChainKind, ContextFrame, DiagnosticNode, DocumentCompleteness, NodeCompleteness,
    Ownership, ProvenanceSource,
};
use diag_rulepack::{
    RenderRulepack, RendererFamilyKind, RendererFamilyPolicy, checked_in_rulepack,
};
use std::path::Path;

/// Collected supporting context lines, child notes, and collapse notices for a diagnostic card.
#[derive(Debug, Default)]
pub struct SupportingEvidence {
    /// Context lines derived from context chains (template, macro, include, linker).
    pub context_lines: Vec<String>,
    /// Compiler notes from child diagnostic nodes.
    pub child_notes: Vec<String>,
    /// Notices about omitted content (e.g. "omitted N additional note(s)").
    pub collapsed_notices: Vec<String>,
}

fn render_rulepack() -> &'static RenderRulepack {
    checked_in_rulepack().render()
}

fn renderer_family_policy(node: &DiagnosticNode) -> Option<&'static RendererFamilyPolicy> {
    node.analysis
        .as_ref()
        .and_then(|analysis| analysis.family.as_deref())
        .and_then(|family| render_rulepack().policy_for_family(family))
}

pub(crate) fn renderer_family_kind(node: &DiagnosticNode) -> RendererFamilyKind {
    renderer_family_policy(node)
        .map(|policy| policy.kind)
        .unwrap_or(RendererFamilyKind::Unknown)
}

pub(crate) fn renderer_specificity_rank(node: &DiagnosticNode) -> u8 {
    renderer_family_policy(node)
        .map(|policy| policy.specificity_rank)
        .unwrap_or(0)
}

pub(crate) fn is_conservative_useful_subset_card(
    request: &RenderRequest,
    node: &DiagnosticNode,
) -> bool {
    renderer_family_policy(node).is_some_and(|policy| policy.band_c_conservative_useful_subset)
        && matches!(band_c_gcc_major(request), Some(9..=12))
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
                .map(|analysis| analysis.disclosure_confidence()),
            Some(diag_core::DisclosureConfidence::Possible)
                | Some(diag_core::DisclosureConfidence::Hidden)
                | None
        )
}

/// Summarizes supporting evidence (context chains, child notes) for a diagnostic node.
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
    let unique_count = dedup_frames(&chain.frames).len();
    let visible = summarize_template_frames(request, node, &chain.frames, frame_limit);
    for frame in &visible {
        push_unique(&mut evidence.context_lines, format!("  - {frame}"));
    }
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
        let unique_count = dedup_frames(&chain.frames).len();
        let visible = summarize_macro_frames(request, node, &chain.frames, frame_limit);
        for frame in &visible {
            push_unique(&mut evidence.context_lines, format!("  - {frame}"));
        }
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
        let unique_count = dedup_frames(&chain.frames).len();
        let visible = summarize_include_frames(request, node, &chain.frames, frame_limit);
        for frame in &visible {
            push_unique(&mut evidence.context_lines, format!("  - {frame}"));
        }
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
                .map(|location| format!(" at {}", format_location(request, location)))
                .unwrap_or_default();
            let candidate_like = note_is_candidate_like(&note);
            let rendered = if conservative && note.starts_with("candidate ") {
                format!("{note}{location}")
            } else if conservative {
                format!("compiler note: {note}{location}")
            } else if note.starts_with("candidate ") {
                format!("because: {note}{location}")
            } else {
                format!("because: {note}")
            };
            Some((child_rank(request, child), index, rendered, candidate_like))
        })
        .collect::<Vec<_>>();
    rendered.sort_by(|left, right| right.0.cmp(&left.0).then_with(|| left.1.cmp(&right.1)));

    let mut unique_rendered = Vec::new();
    for (_, _, line, candidate_like) in rendered {
        if !unique_rendered
            .iter()
            .any(|(existing, _)| existing == &line)
        {
            unique_rendered.push((line, candidate_like));
        }
    }

    let visible = unique_rendered
        .iter()
        .take(note_limit)
        .map(|(line, _)| line.clone())
        .collect::<Vec<_>>();
    for line in &visible {
        push_unique(&mut evidence.context_lines, line.clone());
    }
    if unique_rendered.len() > visible.len() {
        let omitted = unique_rendered.len() - visible.len();
        let candidate_only = unique_rendered
            .iter()
            .all(|(_, candidate_like)| *candidate_like);
        push_unique(
            &mut evidence.context_lines,
            if candidate_only {
                format!("omitted {omitted} other overload candidates")
            } else {
                format!("omitted {omitted} other overload notes")
            },
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
        crate::RenderProfile::Debug => usize::MAX,
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

fn summarize_template_frames(
    request: &RenderRequest,
    node: &DiagnosticNode,
    frames: &[ContextFrame],
    frame_limit: usize,
) -> Vec<String> {
    let unique = if matches!(
        request.profile,
        RenderProfile::Verbose | RenderProfile::Debug
    ) {
        dedup_frames(frames)
    } else {
        compress_template_frames(dedup_frames(frames))
    };
    if unique.is_empty() || frame_limit == 0 {
        return Vec::new();
    }
    select_template_frame_indices(request, node, &unique, frame_limit)
        .into_iter()
        .map(|index| format_frame(&unique[index]))
        .collect()
}

fn summarize_macro_frames(
    request: &RenderRequest,
    node: &DiagnosticNode,
    frames: &[ContextFrame],
    frame_limit: usize,
) -> Vec<String> {
    summarize_family_frames(
        request,
        node,
        &dedup_frames(frames),
        frame_limit,
        select_macro_frame_indices,
    )
}

fn summarize_include_frames(
    request: &RenderRequest,
    node: &DiagnosticNode,
    frames: &[ContextFrame],
    frame_limit: usize,
) -> Vec<String> {
    summarize_family_frames(
        request,
        node,
        &dedup_frames(frames),
        frame_limit,
        select_include_frame_indices,
    )
}

fn summarize_family_frames(
    request: &RenderRequest,
    node: &DiagnosticNode,
    frames: &[ContextFrame],
    frame_limit: usize,
    selector: fn(&RenderRequest, &DiagnosticNode, &[ContextFrame], usize) -> Vec<usize>,
) -> Vec<String> {
    if frames.is_empty() || frame_limit == 0 {
        return Vec::new();
    }
    selector(request, node, frames, frame_limit)
        .into_iter()
        .map(|index| format_frame(&frames[index]))
        .collect()
}

fn compress_template_frames(frames: Vec<ContextFrame>) -> Vec<ContextFrame> {
    let mut compressed: Vec<ContextFrame> = Vec::new();
    for frame in frames {
        let slot = compressed.iter().position(|existing| {
            existing.path == frame.path && existing.line == frame.line && existing.line.is_some()
        });
        if let Some(slot) = slot {
            if template_frame_priority(&frame) > template_frame_priority(&compressed[slot]) {
                compressed[slot] = frame;
            }
        } else {
            compressed.push(frame);
        }
    }
    compressed
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
        if let Some(prefix) = prefix.as_deref()
            && let Some(stripped) = label.strip_prefix(prefix)
        {
            label = stripped.trim_start_matches(':').trim().to_string();
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

fn select_template_frame_indices(
    request: &RenderRequest,
    node: &DiagnosticNode,
    frames: &[ContextFrame],
    frame_limit: usize,
) -> Vec<usize> {
    if frames.is_empty() || frame_limit == 0 {
        return Vec::new();
    }
    let user_indices = user_owned_frame_indices(request, node, frames);
    let frontier = user_indices.first().copied().unwrap_or(0);
    let mut selected = Vec::new();

    push_index(&mut selected, frontier);
    for index in user_indices.iter().copied().skip(1) {
        push_index(&mut selected, index);
        if selected.len() == frame_limit {
            return selected;
        }
    }
    if frontier > 0 {
        push_index(&mut selected, frontier - 1);
    }
    push_index(&mut selected, 0);
    push_index(&mut selected, frames.len().saturating_sub(1));
    for index in frontier + 1..frames.len() {
        push_index(&mut selected, index);
        if selected.len() == frame_limit {
            return selected;
        }
    }
    for index in (0..frontier).rev() {
        push_index(&mut selected, index);
        if selected.len() == frame_limit {
            return selected;
        }
    }
    selected.truncate(frame_limit);
    selected
}

fn select_macro_frame_indices(
    request: &RenderRequest,
    node: &DiagnosticNode,
    frames: &[ContextFrame],
    frame_limit: usize,
) -> Vec<usize> {
    if frames.is_empty() || frame_limit == 0 {
        return Vec::new();
    }
    let user_indices = user_owned_frame_indices(request, node, frames);
    let boundary = user_indices
        .last()
        .copied()
        .unwrap_or(frames.len().saturating_sub(1));
    let mut selected = Vec::new();

    push_index(&mut selected, boundary);
    if boundary > 0 {
        push_index(&mut selected, boundary - 1);
    }
    push_index(&mut selected, 0);
    for index in (0..boundary).rev() {
        push_index(&mut selected, index);
        if selected.len() == frame_limit {
            return selected;
        }
    }
    for index in boundary + 1..frames.len() {
        push_index(&mut selected, index);
        if selected.len() == frame_limit {
            return selected;
        }
    }
    selected.truncate(frame_limit);
    selected
}

fn select_include_frame_indices(
    request: &RenderRequest,
    node: &DiagnosticNode,
    frames: &[ContextFrame],
    frame_limit: usize,
) -> Vec<usize> {
    if frames.is_empty() || frame_limit == 0 {
        return Vec::new();
    }
    let user_indices = user_owned_frame_indices(request, node, frames);
    let boundary = user_indices
        .last()
        .copied()
        .unwrap_or(frames.len().saturating_sub(1));
    let mut selected = Vec::new();

    push_index(&mut selected, 0);
    push_index(&mut selected, boundary);
    if boundary > 0 {
        push_index(&mut selected, boundary - 1);
    }
    push_index(&mut selected, frames.len().saturating_sub(1));
    for index in 0..frames.len() {
        push_index(&mut selected, index);
        if selected.len() == frame_limit {
            return selected;
        }
    }
    selected.truncate(frame_limit);
    selected
}

fn user_owned_frame_indices(
    request: &RenderRequest,
    node: &DiagnosticNode,
    frames: &[ContextFrame],
) -> Vec<usize> {
    frames
        .iter()
        .enumerate()
        .filter_map(|(index, frame)| {
            frame
                .path
                .as_deref()
                .filter(|path| is_user_owned_path(request, node, path))
                .map(|_| index)
        })
        .collect()
}

fn template_frame_priority(frame: &ContextFrame) -> u8 {
    let label = frame.label.to_ascii_lowercase();
    if label.contains("deduced conflicting")
        || label.contains("mismatched types")
        || label.contains("no known conversion")
        || label.contains("cannot convert")
        || label.contains("invalid conversion")
    {
        4
    } else if label.contains("candidate ") {
        3
    } else if label.contains("required from here") || label.contains("instantiated from here") {
        2
    } else if label.contains("deduction/substitution failed") {
        1
    } else {
        2
    }
}

fn note_is_candidate_like(note: &str) -> bool {
    let note = note.trim_start();
    note.starts_with("candidate ")
        || note.starts_with("candidate:")
        || note.contains("candidate expects")
        || note.contains("conversion candidate")
}

fn child_rank(request: &RenderRequest, node: &DiagnosticNode) -> u8 {
    node.locations
        .iter()
        .map(|location| ownership_rank(request, location.path_raw(), location.ownership()))
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
                ownership_rank(request, location.path_raw(), location.ownership()),
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
        for prefix in location_prefixes(request, location) {
            if let Some(stripped) = note.strip_prefix(&prefix) {
                note = stripped.trim().to_string();
                break;
            }
        }
    }
    if let Some(stripped) = note.strip_prefix("note:") {
        note = stripped.trim().to_string();
    }
    note
}

fn location_prefixes(request: &RenderRequest, location: &diag_core::Location) -> Vec<String> {
    let mut prefixes = vec![format!(
        "{}:{}:{}:",
        location.path_raw(),
        location.line(),
        location.column()
    )];
    if location.display_path() != location.path_raw() {
        prefixes.push(format!(
            "{}:{}:{}:",
            location.display_path(),
            location.line(),
            location.column()
        ));
    }
    let formatted = format!("{}:", format_location(request, location));
    if !prefixes.iter().any(|prefix| prefix == &formatted) {
        prefixes.push(formatted);
    }
    prefixes.sort_by_key(|prefix| std::cmp::Reverse(prefix.len()));
    prefixes
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
        location.path_raw() == path && matches!(location.ownership(), Some(Ownership::User))
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

    default_limit.min(
        render_rulepack()
            .policy_for_kind(RendererFamilyKind::Template)
            .and_then(|policy| conservative_limit(policy, request.profile))
            .unwrap_or(default_limit),
    )
}

fn constrained_candidate_notes(
    request: &RenderRequest,
    default_limit: usize,
    conservative: bool,
) -> usize {
    if !conservative {
        return default_limit;
    }

    default_limit.min(
        render_rulepack()
            .policy_for_kind(RendererFamilyKind::TypeOverload)
            .and_then(|policy| conservative_limit(policy, request.profile))
            .unwrap_or(default_limit),
    )
}

fn linker_object_limit(request: &RenderRequest, conservative: bool) -> usize {
    if !conservative {
        return 3;
    }

    render_rulepack()
        .policy_for_kind(RendererFamilyKind::Linker)
        .and_then(|policy| conservative_limit(policy, request.profile))
        .unwrap_or(3)
}

fn conservative_limit(policy: &RendererFamilyPolicy, profile: RenderProfile) -> Option<usize> {
    policy
        .conservative_limits
        .as_ref()
        .map(|limits| match profile {
            RenderProfile::Verbose => limits.verbose,
            RenderProfile::Debug => limits.debug,
            RenderProfile::Default => limits.default,
            RenderProfile::Concise => limits.concise,
            RenderProfile::Ci => limits.ci,
            RenderProfile::RawFallback => limits.raw_fallback,
        })
}

fn push_index(indices: &mut Vec<usize>, index: usize) {
    if !indices.contains(&index) {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        DebugRefs, PathPolicy, RenderCapabilities, SourceExcerptPolicy, TypeDisplayPolicy,
        WarningVisibility,
    };
    use diag_core::{
        AnalysisOverlay, ArtifactKind, ArtifactStorage, CaptureArtifact, Confidence, ContextChain,
        DiagnosticDocument, LanguageMode, Location, MessageText, NodeCompleteness, Origin,
        Ownership, Phase, ProducerInfo, Provenance, ProvenanceSource, RunInfo, SemanticRole,
        Severity, SymbolContext, ToolInfo, WrapperSurface,
    };

    fn sample_request() -> RenderRequest {
        RenderRequest {
            document: DiagnosticDocument {
                document_id: "doc".to_string(),
                schema_version: "1".to_string(),
                document_completeness: DocumentCompleteness::Complete,
                producer: ProducerInfo {
                    name: "gcc-formed".to_string(),
                    version: "0.1.0".to_string(),
                    git_revision: None,
                    build_profile: None,
                    rulepack_version: None,
                },
                run: RunInfo {
                    invocation_id: "inv".to_string(),
                    invoked_as: Some("gcc-formed".to_string()),
                    argv_redacted: vec![
                        "gcc".to_string(),
                        "-c".to_string(),
                        "src/main.cpp".to_string(),
                    ],
                    cwd_display: Some("/tmp/project".to_string()),
                    exit_status: 1,
                    primary_tool: ToolInfo {
                        name: "gcc".to_string(),
                        version: Some("15.1.0".to_string()),
                        component: None,
                        vendor: Some("GNU".to_string()),
                    },
                    secondary_tools: Vec::new(),
                    language_mode: Some(LanguageMode::Cpp),
                    target_triple: None,
                    wrapper_mode: Some(WrapperSurface::Terminal),
                },
                captures: vec![CaptureArtifact {
                    id: "stderr.raw".to_string(),
                    kind: ArtifactKind::CompilerStderrText,
                    media_type: "text/plain".to_string(),
                    encoding: Some("utf-8".to_string()),
                    digest_sha256: None,
                    size_bytes: Some(12),
                    storage: ArtifactStorage::Inline,
                    inline_text: Some("stderr".to_string()),
                    external_ref: None,
                    produced_by: None,
                }],
                integrity_issues: Vec::new(),
                diagnostics: vec![sample_node("syntax")],
                document_analysis: None,
                fingerprints: None,
            },
            cascade_policy: diag_core::CascadePolicySnapshot::default(),
            profile: RenderProfile::Default,
            capabilities: RenderCapabilities {
                stream_kind: crate::StreamKind::Tty,
                width_columns: Some(100),
                ansi_color: false,
                unicode: false,
                hyperlinks: false,
                interactive: false,
            },
            cwd: Some("/tmp/project".into()),
            path_policy: PathPolicy::RelativeToCwd,
            warning_visibility: WarningVisibility::Auto,
            debug_refs: DebugRefs::None,
            type_display_policy: TypeDisplayPolicy::CompactSafe,
            source_excerpt_policy: SourceExcerptPolicy::Auto,
        }
    }

    fn sample_node(family: &str) -> DiagnosticNode {
        DiagnosticNode {
            id: format!("node-{family}"),
            origin: Origin::Gcc,
            phase: Phase::Semantic,
            severity: Severity::Error,
            semantic_role: SemanticRole::Root,
            message: MessageText {
                raw_text: "message".to_string(),
                normalized_text: None,
                locale: None,
            },
            locations: vec![
                Location::caret("src/main.cpp", 5, 7, diag_core::LocationRole::Primary)
                    .with_ownership(Ownership::User, "user_workspace"),
            ],
            children: Vec::new(),
            suggestions: Vec::new(),
            context_chains: Vec::new(),
            symbol_context: None,
            node_completeness: NodeCompleteness::Complete,
            provenance: Provenance {
                source: ProvenanceSource::Compiler,
                capture_refs: vec!["stderr.raw".to_string()],
            },
            analysis: Some(AnalysisOverlay {
                family: Some(family.to_string().into()),
                family_version: None,
                family_confidence: None,
                root_cause_score: None,
                actionability_score: None,
                user_code_priority: None,
                headline: Some("headline".into()),
                first_action_hint: Some("hint".into()),
                confidence: Some(Confidence::High.score()),
                preferred_primary_location_id: None,
                rule_id: Some("rule".into()),
                matched_conditions: vec!["matched=true".into()],
                suppression_reason: None,
                collapsed_child_ids: Vec::new(),
                collapsed_chain_ids: Vec::new(),
                group_ref: None,
                reasons: Vec::new(),
                policy_profile: None,
                producer_version: None,
            }),
            fingerprints: None,
        }
    }

    fn sample_linker_node(family: &str) -> DiagnosticNode {
        let mut node = sample_node(family);
        node.symbol_context = Some(SymbolContext {
            primary_symbol: Some("foo".to_string()),
            related_objects: vec![
                "obj/vendor.o".to_string(),
                "src/main.o".to_string(),
                "lib/helper.o".to_string(),
            ],
            archive: Some("libfoo.a".to_string()),
        });
        node
    }

    #[test]
    fn loads_checked_in_render_rulepack() {
        let rulepack = render_rulepack();
        assert_eq!(rulepack.rulepack_version, "phase1");
        assert!(std::ptr::eq(rulepack, checked_in_rulepack().render()));
        assert!(
            rulepack
                .policy_for_kind(RendererFamilyKind::Linker)
                .is_some_and(|policy| policy.match_prefix.as_deref() == Some("linker."))
        );
    }

    #[test]
    fn renderer_family_kind_uses_rulepack_matching_policy() {
        assert_eq!(
            renderer_family_kind(&sample_node("macro_include")),
            RendererFamilyKind::MacroInclude
        );
        assert_eq!(
            renderer_family_kind(&sample_node("preprocessor_directive")),
            RendererFamilyKind::Syntax
        );
        assert_eq!(
            renderer_family_kind(&sample_node("openmp")),
            RendererFamilyKind::Syntax
        );
        assert_eq!(
            renderer_family_kind(&sample_node("scope_declaration")),
            RendererFamilyKind::Syntax
        );
        assert_eq!(
            renderer_family_kind(&sample_node("redefinition")),
            RendererFamilyKind::Syntax
        );
        assert_eq!(
            renderer_family_kind(&sample_node("deleted_function")),
            RendererFamilyKind::Syntax
        );
        assert_eq!(
            renderer_family_kind(&sample_node("concepts_constraints")),
            RendererFamilyKind::Syntax
        );
        assert_eq!(
            renderer_family_kind(&sample_node("ranges_views")),
            RendererFamilyKind::Syntax
        );
        assert_eq!(
            renderer_family_kind(&sample_node("unused")),
            RendererFamilyKind::Syntax
        );
        assert_eq!(
            renderer_family_kind(&sample_node("return_type")),
            RendererFamilyKind::Syntax
        );
        assert_eq!(
            renderer_family_kind(&sample_node("fallthrough")),
            RendererFamilyKind::Syntax
        );
        assert_eq!(
            renderer_family_kind(&sample_node("sanitizer_buffer")),
            RendererFamilyKind::Syntax
        );
        assert_eq!(
            renderer_family_kind(&sample_node("format_string")),
            RendererFamilyKind::Syntax
        );
        assert_eq!(
            renderer_family_kind(&sample_node("uninitialized")),
            RendererFamilyKind::Syntax
        );
        assert_eq!(
            renderer_family_kind(&sample_node("overflow_arithmetic")),
            RendererFamilyKind::Syntax
        );
        assert_eq!(
            renderer_family_kind(&sample_node("enum_switch")),
            RendererFamilyKind::Syntax
        );
        assert_eq!(
            renderer_family_kind(&sample_node("analyzer")),
            RendererFamilyKind::Syntax
        );
        assert_eq!(
            renderer_family_kind(&sample_node("null_pointer")),
            RendererFamilyKind::Syntax
        );
        assert_eq!(
            renderer_family_kind(&sample_node("conversion_narrowing")),
            RendererFamilyKind::Syntax
        );
        assert_eq!(
            renderer_family_kind(&sample_node("const_qualifier")),
            RendererFamilyKind::Syntax
        );
        assert_eq!(
            renderer_family_kind(&sample_node("move_semantics")),
            RendererFamilyKind::Syntax
        );
        assert_eq!(
            renderer_family_kind(&sample_node("strict_aliasing")),
            RendererFamilyKind::Syntax
        );
        assert_eq!(
            renderer_family_kind(&sample_node("asm_inline")),
            RendererFamilyKind::Syntax
        );
        assert_eq!(
            renderer_family_kind(&sample_node("bit_field_packed")),
            RendererFamilyKind::Syntax
        );
        assert_eq!(
            renderer_family_kind(&sample_node("abi_alignment")),
            RendererFamilyKind::Syntax
        );
        assert_eq!(
            renderer_family_kind(&sample_node("thread_safety")),
            RendererFamilyKind::Syntax
        );
        assert_eq!(
            renderer_family_kind(&sample_node("storage_class")),
            RendererFamilyKind::Syntax
        );
        assert_eq!(
            renderer_family_kind(&sample_node("exception_handling")),
            RendererFamilyKind::Syntax
        );
        assert_eq!(
            renderer_family_kind(&sample_node("attribute")),
            RendererFamilyKind::Syntax
        );
        assert_eq!(
            renderer_family_kind(&sample_node("odr_inline_linkage")),
            RendererFamilyKind::Syntax
        );
        assert_eq!(
            renderer_family_kind(&sample_node("sizeof_allocation")),
            RendererFamilyKind::Syntax
        );
        assert_eq!(
            renderer_family_kind(&sample_node("pointer_reference")),
            RendererFamilyKind::Syntax
        );
        assert_eq!(
            renderer_family_kind(&sample_node("structured_binding")),
            RendererFamilyKind::Syntax
        );
        assert_eq!(
            renderer_family_kind(&sample_node("access_control")),
            RendererFamilyKind::Syntax
        );
        assert_eq!(
            renderer_family_kind(&sample_node("inheritance_virtual")),
            RendererFamilyKind::Syntax
        );
        assert_eq!(
            renderer_family_kind(&sample_node("constexpr")),
            RendererFamilyKind::Syntax
        );
        assert_eq!(
            renderer_family_kind(&sample_node("lambda_closure")),
            RendererFamilyKind::Syntax
        );
        assert_eq!(
            renderer_family_kind(&sample_node("lifetime_dangling")),
            RendererFamilyKind::Syntax
        );
        assert_eq!(
            renderer_family_kind(&sample_node("init_order")),
            RendererFamilyKind::Syntax
        );
        assert_eq!(
            renderer_family_kind(&sample_node("designated_init")),
            RendererFamilyKind::Syntax
        );
        assert_eq!(
            renderer_family_kind(&sample_node("coroutine")),
            RendererFamilyKind::Syntax
        );
        assert_eq!(
            renderer_family_kind(&sample_node("module_import")),
            RendererFamilyKind::Syntax
        );
        assert_eq!(
            renderer_family_kind(&sample_node("deprecated")),
            RendererFamilyKind::Syntax
        );
        assert_eq!(
            renderer_family_kind(&sample_node("pedantic_compliance")),
            RendererFamilyKind::Syntax
        );
        assert_eq!(
            renderer_family_kind(&sample_node("three_way_comparison")),
            RendererFamilyKind::Syntax
        );
        assert_eq!(
            renderer_family_kind(&sample_node("string_character")),
            RendererFamilyKind::Syntax
        );
        assert_eq!(
            renderer_family_kind(&sample_node("linker.undefined_reference")),
            RendererFamilyKind::Linker
        );
        assert_eq!(
            renderer_family_kind(&sample_node("linker.file_format_or_relocation")),
            RendererFamilyKind::Unknown
        );
    }

    #[test]
    fn conservative_useful_subset_respects_rulepack_family_flags() {
        let mut request = sample_request();
        request.document.run.primary_tool.version = Some("12.3.0".to_string());
        request.document.document_completeness = DocumentCompleteness::Partial;

        let mut template = sample_node("template");
        template.node_completeness = NodeCompleteness::Partial;
        template.provenance.source = ProvenanceSource::ResidualText;
        template
            .analysis
            .as_mut()
            .unwrap()
            .set_confidence_bucket(Confidence::Low);
        assert!(is_conservative_useful_subset_card(&request, &template));

        let mut macro_include = sample_node("macro_include");
        macro_include.node_completeness = NodeCompleteness::Partial;
        macro_include.provenance.source = ProvenanceSource::ResidualText;
        macro_include
            .analysis
            .as_mut()
            .unwrap()
            .set_confidence_bucket(Confidence::Low);
        assert!(!is_conservative_useful_subset_card(
            &request,
            &macro_include
        ));
    }

    #[test]
    fn conservative_limits_come_from_rulepack_policy() {
        let mut request = sample_request();
        request.profile = RenderProfile::Ci;

        assert_eq!(constrained_template_frames(&request, 20, true), 2);
        assert_eq!(constrained_candidate_notes(&request, 8, true), 1);
        assert_eq!(linker_object_limit(&request, true), 1);

        request.profile = RenderProfile::Debug;
        assert_eq!(constrained_template_frames(&request, 30, true), 6);
        assert_eq!(constrained_candidate_notes(&request, 20, true), 3);
        assert_eq!(linker_object_limit(&request, true), 2);
    }

    #[test]
    fn template_frontier_compaction_leads_with_first_user_owned_frame() {
        let request = sample_request();
        let mut node = sample_node("template");
        node.context_chains = vec![ContextChain {
            kind: ContextChainKind::TemplateInstantiation,
            frames: vec![
                ContextFrame {
                    label: "candidate 1: 'template<class T> Widget(T)'".to_string(),
                    path: Some("/usr/include/c++/15/widget".to_string()),
                    line: Some(18),
                    column: Some(5),
                },
                ContextFrame {
                    label: "candidate 2: 'template<class U> Widget(U)'".to_string(),
                    path: Some("/usr/include/c++/15/widget".to_string()),
                    line: Some(18),
                    column: Some(9),
                },
                ContextFrame {
                    label: "template argument deduction/substitution failed:".to_string(),
                    path: Some("/usr/include/c++/15/widget".to_string()),
                    line: Some(18),
                    column: Some(11),
                },
                ContextFrame {
                    label: "deduced conflicting types for parameter 'T' ('int' and 'const char*')"
                        .to_string(),
                    path: Some("src/main.cpp".to_string()),
                    line: Some(25),
                    column: Some(13),
                },
                ContextFrame {
                    label: "required from here".to_string(),
                    path: Some("src/app.cpp".to_string()),
                    line: Some(9),
                    column: Some(7),
                },
            ],
        }];

        let evidence = summarize_supporting_evidence(&request, &node);

        assert_eq!(evidence.context_lines[0], "while instantiating:");
        assert_eq!(
            evidence.context_lines[1],
            "  - src/main.cpp:25:13 deduced conflicting types for parameter 'T' ('int' and 'const char*')"
        );
        assert_eq!(
            evidence.context_lines[2],
            "  - src/app.cpp:9:7 required from here"
        );
        assert!(
            evidence
                .context_lines
                .iter()
                .any(|line| line.contains("omitted 2 internal template frames"))
        );
        assert!(
            !evidence
                .context_lines
                .iter()
                .any(|line| line.contains("candidate 2"))
        );
    }

    #[test]
    fn macro_frontier_compaction_shows_user_invocation_before_inner_expansion() {
        let request = sample_request();
        let mut node = sample_node("macro_include");
        node.context_chains = vec![ContextChain {
            kind: ContextChainKind::MacroExpansion,
            frames: vec![
                ContextFrame {
                    label: "in expansion of macro 'INNER_ACCESS'".to_string(),
                    path: Some("src/config.h".to_string()),
                    line: Some(2),
                    column: Some(29),
                },
                ContextFrame {
                    label: "in expansion of macro 'OUTER_ACCESS'".to_string(),
                    path: Some("src/wrapper.h".to_string()),
                    line: Some(7),
                    column: Some(11),
                },
                ContextFrame {
                    label: "in expansion of macro 'FETCH_VALUE'".to_string(),
                    path: Some("src/main.c".to_string()),
                    line: Some(9),
                    column: Some(12),
                },
            ],
        }];

        let evidence = summarize_supporting_evidence(&request, &node);

        assert_eq!(evidence.context_lines[0], "through macro expansion:");
        assert_eq!(
            evidence.context_lines[1],
            "  - src/main.c:9:12 in expansion of macro 'FETCH_VALUE'"
        );
        assert_eq!(
            evidence.context_lines[2],
            "  - src/wrapper.h:7:11 in expansion of macro 'OUTER_ACCESS'"
        );
    }

    #[test]
    fn include_frontier_compaction_keeps_user_boundary_visible() {
        let request = sample_request();
        let mut node = sample_node("macro_include");
        node.context_chains = vec![ContextChain {
            kind: ContextChainKind::Include,
            frames: vec![
                ContextFrame {
                    label: "In file included from /usr/include/project/detail.hpp:1,".to_string(),
                    path: Some("/usr/include/project/detail.hpp".to_string()),
                    line: Some(1),
                    column: Some(1),
                },
                ContextFrame {
                    label: "from src/wrapper.h:1:".to_string(),
                    path: Some("src/wrapper.h".to_string()),
                    line: Some(1),
                    column: Some(1),
                },
                ContextFrame {
                    label: "from src/main.c:2:".to_string(),
                    path: Some("src/main.c".to_string()),
                    line: Some(2),
                    column: Some(1),
                },
            ],
        }];

        let evidence = summarize_supporting_evidence(&request, &node);

        assert_eq!(evidence.context_lines[0], "from include chain:");
        assert!(
            evidence
                .context_lines
                .iter()
                .any(|line| { line.contains("/usr/include/project/detail.hpp") })
        );
        assert!(
            evidence
                .context_lines
                .iter()
                .any(|line| line.contains("src/main.c:2:1"))
        );
    }

    #[test]
    fn overload_candidate_flood_uses_candidate_specific_omission_notice() {
        let request = sample_request();
        let mut node = sample_node("type_overload");
        node.children = (1..=5)
            .map(|index| {
                let mut child = sample_node("type_overload");
                child.id = format!("candidate-{index}");
                child.severity = Severity::Note;
                child.semantic_role = SemanticRole::Candidate;
                child.locations = vec![
                    Location::caret(
                        format!("src/candidate_{index}.hpp"),
                        index,
                        3,
                        diag_core::LocationRole::Primary,
                    )
                    .with_ownership(Ownership::User, "user_workspace"),
                ];
                child.message.raw_text = format!("candidate {index}: 'void set_limit(value_type)'");
                child.children = Vec::new();
                child.context_chains = Vec::new();
                child.analysis = None;
                child
            })
            .collect();

        let evidence = summarize_supporting_evidence(&request, &node);

        assert!(
            evidence
                .context_lines
                .iter()
                .any(|line| line == "omitted 2 other overload candidates")
        );
    }

    #[test]
    fn linker_cannot_find_library_uses_shared_linker_policy() {
        let request = sample_request();
        let mut node = sample_linker_node("linker.cannot_find_library");
        node.symbol_context.as_mut().unwrap().primary_symbol = None;

        let evidence = summarize_supporting_evidence(&request, &node);

        assert_eq!(renderer_family_kind(&node), RendererFamilyKind::Linker);
        assert!(
            evidence
                .context_lines
                .iter()
                .any(|line| line == "linker: original linker diagnostics are preserved")
        );
        assert!(
            evidence
                .context_lines
                .iter()
                .any(|line| line == "linker: archive libfoo.a")
        );
    }

    #[test]
    fn excluded_file_format_family_uses_generic_evidence_path() {
        let request = sample_request();
        let mut node = sample_linker_node("linker.file_format_or_relocation");
        node.context_chains = vec![ContextChain {
            kind: ContextChainKind::LinkerResolution,
            frames: Vec::new(),
        }];

        let evidence = summarize_supporting_evidence(&request, &node);

        assert_eq!(renderer_family_kind(&node), RendererFamilyKind::Unknown);
        assert!(
            evidence
                .context_lines
                .iter()
                .any(|line| line == "linker: preserved")
        );
        assert!(
            !evidence
                .context_lines
                .iter()
                .any(|line| line.starts_with("linker: symbol `"))
        );
    }
}
