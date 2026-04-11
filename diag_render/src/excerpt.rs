use crate::budget::budget_for;
use crate::{RenderProfile, RenderRequest, SourceExcerptPolicy};
use diag_core::{DiagnosticNode, Ownership};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

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
    let mut locations = node.locations.iter().enumerate().collect::<Vec<_>>();
    locations.sort_by(|left, right| {
        excerpt_rank(request, right.1)
            .cmp(&excerpt_rank(request, left.1))
            .then_with(|| left.0.cmp(&right.0))
    });
    locations
        .iter()
        .take(limit)
        .filter_map(|(_, location)| {
            let resolved_path = if std::path::Path::new(location.path_raw()).is_relative() {
                request
                    .cwd
                    .as_ref()
                    .map(|cwd| cwd.join(location.path_raw()))
                    .unwrap_or_else(|| std::path::PathBuf::from(location.path_raw()))
            } else {
                std::path::PathBuf::from(location.path_raw())
            };
            let content = fs::read_to_string(&resolved_path).ok()?;
            let line_index = usize::try_from(location.line().saturating_sub(1)).ok()?;
            let source_line = content.lines().nth(line_index)?.to_string();
            Some(ExcerptBlock {
                location: format!(
                    "{}:{}:{}",
                    location.path_raw(),
                    location.line(),
                    location.column()
                ),
                lines: vec![source_line],
            })
        })
        .collect()
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
