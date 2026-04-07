use crate::{RenderProfile, RenderRequest, SourceExcerptPolicy};
use diag_core::DiagnosticNode;
use std::fs;

#[derive(Debug, Clone)]
pub struct ExcerptBlock {
    pub location: String,
    pub lines: Vec<String>,
}

pub fn load_excerpt(request: &RenderRequest, node: &DiagnosticNode) -> Vec<ExcerptBlock> {
    if matches!(request.source_excerpt_policy, SourceExcerptPolicy::ForceOff) {
        return Vec::new();
    }
    let limit = match request.profile {
        RenderProfile::Verbose => 3,
        _ => 1,
    };
    node.locations
        .iter()
        .take(limit)
        .filter_map(|location| {
            let content = fs::read_to_string(&location.path).ok()?;
            let line_index = usize::try_from(location.line.saturating_sub(1)).ok()?;
            let source_line = content.lines().nth(line_index)?.to_string();
            Some(ExcerptBlock {
                location: format!("{}:{}:{}", location.path, location.line, location.column),
                lines: vec![source_line],
            })
        })
        .collect()
}
