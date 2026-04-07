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
            lines.push(format!("{label}: preserved"));
            continue;
        }
        for frame in chain.frames.iter().take(limit) {
            let mut rendered = format!("{label}: {}", frame.label);
            if let Some(path) = frame.path.as_ref() {
                rendered.push_str(&format!(" @ {path}"));
            }
            lines.push(rendered);
        }
        if chain.frames.len() > limit {
            lines.push(format!(
                "omitted {} {} frames",
                chain.frames.len() - limit,
                label
            ));
        }
    }
    if lines.is_empty() {
        if let Some(family) = node
            .analysis
            .as_ref()
            .and_then(|analysis| analysis.family.as_ref())
        {
            if family == "template" {
                lines.push("template: preserved".to_string());
            } else if family == "macro_include" {
                lines.push("macro/include: preserved".to_string());
            } else if family == "linker" {
                lines.push("linker: preserved".to_string());
            }
        }
    }
    lines
}
