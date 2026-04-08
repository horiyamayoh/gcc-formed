use crate::budget::budget_for;
use crate::{RenderProfile, RenderRequest, SourceExcerptPolicy};
use diag_core::DiagnosticNode;
use serde::{Deserialize, Serialize};
use std::fs;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExcerptBlock {
    pub location: String,
    pub lines: Vec<String>,
}

pub fn load_excerpt(request: &RenderRequest, node: &DiagnosticNode) -> Vec<ExcerptBlock> {
    if matches!(request.source_excerpt_policy, SourceExcerptPolicy::ForceOff) {
        return Vec::new();
    }
    let limit = match request.profile {
        RenderProfile::RawFallback => 0,
        _ => budget_for(request.profile).source_excerpts,
    };
    node.locations
        .iter()
        .take(limit)
        .filter_map(|location| {
            let resolved_path = if std::path::Path::new(&location.path).is_relative() {
                request
                    .cwd
                    .as_ref()
                    .map(|cwd| cwd.join(&location.path))
                    .unwrap_or_else(|| std::path::PathBuf::from(&location.path))
            } else {
                std::path::PathBuf::from(&location.path)
            };
            let content = fs::read_to_string(&resolved_path).ok()?;
            let line_index = usize::try_from(location.line.saturating_sub(1)).ok()?;
            let source_line = content.lines().nth(line_index)?.to_string();
            Some(ExcerptBlock {
                location: format!("{}:{}:{}", location.path, location.line, location.column),
                lines: vec![source_line],
            })
        })
        .collect()
}
