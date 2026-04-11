//! Stderr context augmentation for structured diagnostic documents.

use diag_core::{ContextChain, ContextChainKind, DiagnosticDocument, DiagnosticNode, Location};

#[derive(Debug, Clone, PartialEq, Eq)]
struct StderrContextBlock {
    include_frames: Vec<diag_core::ContextFrame>,
    macro_frames: Vec<diag_core::ContextFrame>,
    location_hints: Vec<StderrLocationHint>,
    message_hint: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct StderrLocationHint {
    path: String,
    line: u32,
    column: Option<u32>,
    priority: u8,
}

pub(crate) fn augment_context_chains_from_stderr(
    document: &mut DiagnosticDocument,
    stderr_text: &str,
) {
    let blocks = parse_stderr_context_blocks(stderr_text);
    for block in blocks {
        let Some(target_index) = select_context_target(&document.diagnostics, &block) else {
            continue;
        };
        let target = &mut document.diagnostics[target_index];
        if !block.include_frames.is_empty() {
            push_chain_frames(target, ContextChainKind::Include, block.include_frames);
        }
        if !block.macro_frames.is_empty() {
            push_chain_frames(target, ContextChainKind::MacroExpansion, block.macro_frames);
        }
    }
}

fn parse_stderr_context_blocks(stderr_text: &str) -> Vec<StderrContextBlock> {
    let mut blocks = Vec::new();
    let mut pending_include_frames = Vec::new();
    let mut current_block: Option<StderrContextBlock> = None;

    for line in stderr_text.lines() {
        let trimmed = line.trim_start();
        if let Some(frame) = parse_include_frame(trimmed) {
            if let Some(block) = current_block.take().filter(stderr_block_has_frames) {
                blocks.push(block);
            }
            pending_include_frames.push(frame);
            continue;
        }

        if let Some((hint, message_hint)) = parse_root_diagnostic_hint(trimmed) {
            if let Some(block) = current_block.take().filter(stderr_block_has_frames) {
                blocks.push(block);
            }
            current_block = Some(StderrContextBlock {
                include_frames: std::mem::take(&mut pending_include_frames),
                macro_frames: Vec::new(),
                location_hints: vec![hint],
                message_hint: Some(message_hint),
            });
            continue;
        }

        if let Some(frame) = parse_macro_expansion_frame(trimmed)
            && let Some(block) = current_block.as_mut()
        {
            if let Some(hint) = location_hint_from_frame(&frame, 2) {
                block.location_hints.push(hint);
            }
            block.macro_frames.push(frame);
        }
    }

    if let Some(block) = current_block.filter(stderr_block_has_frames) {
        blocks.push(block);
    }

    blocks
}

fn stderr_block_has_frames(block: &StderrContextBlock) -> bool {
    !block.include_frames.is_empty() || !block.macro_frames.is_empty()
}

fn parse_root_diagnostic_hint(line: &str) -> Option<(StderrLocationHint, String)> {
    let path = parse_path_prefix(line)?;
    let line_number = parse_line_prefix(line)?;
    let message = parse_root_diagnostic_message(line)?;
    Some((
        StderrLocationHint {
            path,
            line: line_number,
            column: parse_column_prefix(line),
            priority: 1,
        },
        message,
    ))
}

fn parse_root_diagnostic_message(line: &str) -> Option<String> {
    for marker in [": fatal error: ", ": error: ", ": warning: "] {
        if let Some((_, message)) = line.split_once(marker) {
            return Some(message.trim().to_string());
        }
    }
    None
}

fn parse_macro_expansion_frame(line: &str) -> Option<diag_core::ContextFrame> {
    line.contains("in expansion of macro")
        .then(|| diag_core::ContextFrame {
            label: line.to_string(),
            path: parse_path_prefix(line),
            line: parse_line_prefix(line),
            column: parse_column_prefix(line),
        })
}

fn location_hint_from_frame(
    frame: &diag_core::ContextFrame,
    priority: u8,
) -> Option<StderrLocationHint> {
    Some(StderrLocationHint {
        path: frame.path.clone()?,
        line: frame.line?,
        column: frame.column,
        priority,
    })
}

fn select_context_target(
    diagnostics: &[DiagnosticNode],
    block: &StderrContextBlock,
) -> Option<usize> {
    let best = diagnostics
        .iter()
        .enumerate()
        .filter_map(|(index, node)| {
            let score = score_context_block_for_node(block, node);
            (score > 0).then_some((index, score))
        })
        .max_by_key(|(_, score)| *score)
        .map(|(index, _)| index);

    best.or_else(|| (diagnostics.len() == 1 && stderr_block_has_frames(block)).then_some(0))
}

fn score_context_block_for_node(block: &StderrContextBlock, node: &DiagnosticNode) -> i32 {
    let location_score = node
        .primary_location()
        .map(|primary| {
            block
                .location_hints
                .iter()
                .filter_map(|hint| score_location_hint(primary, hint))
                .max()
                .unwrap_or(0)
        })
        .unwrap_or(0);

    location_score + score_message_hint(block.message_hint.as_deref(), &node.message.raw_text)
}

fn score_location_hint(location: &Location, hint: &StderrLocationHint) -> Option<i32> {
    if location.path_raw() != hint.path || location.line() != hint.line {
        return None;
    }

    let mut score = 100 + (hint.priority as i32 * 25);
    if hint.column.is_some() && hint.column == Some(location.column()) {
        score += 20;
    }
    Some(score)
}

fn score_message_hint(message_hint: Option<&str>, node_message: &str) -> i32 {
    let Some(message_hint) = message_hint else {
        return 0;
    };
    let hint = message_hint.trim().to_lowercase();
    let node = node_message.trim().to_lowercase();
    if hint.is_empty() || node.is_empty() {
        return 0;
    }
    if node.contains(&hint) || hint.contains(&node) {
        30
    } else {
        0
    }
}

fn parse_include_frame(line: &str) -> Option<diag_core::ContextFrame> {
    let prefix = if let Some(value) = line.strip_prefix("In file included from ") {
        value
    } else {
        line.strip_prefix("from ")?
    };
    let (path, line_number) = split_path_line(prefix)?;
    Some(diag_core::ContextFrame {
        label: line.to_string(),
        path: Some(path.to_string()),
        line: Some(line_number),
        column: None,
    })
}

fn split_path_line(value: &str) -> Option<(&str, u32)> {
    let separator = value.rfind(':')?;
    let path = value[..separator].trim_end_matches(',').trim();
    let remainder = value[separator + 1..]
        .trim_end_matches(',')
        .trim_end_matches(':')
        .trim();
    Some((path, remainder.parse().ok()?))
}

fn parse_path_prefix(line: &str) -> Option<String> {
    let first = line.split(':').next()?;
    if first.is_empty() || first.contains(' ') {
        None
    } else {
        Some(first.to_string())
    }
}

fn parse_line_prefix(line: &str) -> Option<u32> {
    let mut parts = line.split(':');
    parts.next()?;
    parts.next()?.parse().ok()
}

fn parse_column_prefix(line: &str) -> Option<u32> {
    let mut parts = line.split(':');
    parts.next()?;
    parts.next()?;
    parts.next()?.parse().ok()
}

fn push_chain_frames(
    node: &mut DiagnosticNode,
    kind: ContextChainKind,
    mut frames: Vec<diag_core::ContextFrame>,
) {
    if let Some(existing) = node
        .context_chains
        .iter_mut()
        .find(|chain| chain.kind == kind)
    {
        existing.frames.append(&mut frames);
    } else {
        node.context_chains.push(ContextChain { kind, frames });
    }
}
