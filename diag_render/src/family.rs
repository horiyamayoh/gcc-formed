use crate::RenderProfile;
use diag_core::{ContextChainKind, DiagnosticNode};

pub fn summarize_context(node: &DiagnosticNode, profile: RenderProfile) -> Vec<String> {
    let limit = match profile {
        RenderProfile::Verbose => usize::MAX,
        _ => 3,
    };
    let mut lines = Vec::new();
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
            push_unique(&mut lines, format!("{label}: preserved"));
            continue;
        }
        for frame in chain.frames.iter().take(limit) {
            let mut rendered = format!("{label}: {}", frame.label);
            if let Some(path) = frame.path.as_ref() {
                rendered.push_str(&format!(" @ {path}"));
            }
            push_unique(&mut lines, rendered);
        }
        if chain.frames.len() > limit {
            push_unique(
                &mut lines,
                format!("omitted {} {} frames", chain.frames.len() - limit, label),
            );
        }
    }
    if lines.is_empty() {
        if let Some(family) = node
            .analysis
            .as_ref()
            .and_then(|analysis| analysis.family.as_ref())
        {
            if family == "template" {
                push_unique(&mut lines, "template: preserved".to_string());
            } else if family == "macro_include" {
                push_unique(&mut lines, "macro/include: preserved".to_string());
            } else if family == "linker" || family.starts_with("linker.") {
                push_unique(&mut lines, "linker: preserved".to_string());
            }
        }
    }
    lines
}

fn push_unique(lines: &mut Vec<String>, line: String) {
    if !lines.iter().any(|existing| existing == &line) {
        lines.push(line);
    }
}
