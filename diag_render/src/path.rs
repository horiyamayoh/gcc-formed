use crate::{PathPolicy, RenderRequest};
use diag_core::{DiagnosticNode, Location};
use std::path::{Path, PathBuf};

pub(crate) fn format_location(request: &RenderRequest, location: &Location) -> String {
    format!(
        "{}:{}:{}",
        display_path(request, location.path_raw()),
        location.line(),
        location.column()
    )
}

pub(crate) fn resolved_path(request: &RenderRequest, raw_path: &str) -> PathBuf {
    let path = Path::new(raw_path);
    if path.is_absolute() || preserves_literal_path(raw_path) {
        return PathBuf::from(raw_path);
    }
    request
        .cwd
        .as_ref()
        .map(|cwd| cwd.join(path))
        .unwrap_or_else(|| PathBuf::from(raw_path))
}

fn display_path(request: &RenderRequest, raw_path: &str) -> String {
    match request.path_policy {
        PathPolicy::Absolute => absolute_display_path(request, raw_path),
        PathPolicy::RelativeToCwd => {
            relative_display_path(request, raw_path).unwrap_or_else(|| raw_path.to_string())
        }
        PathPolicy::ShortestUnambiguous => shortest_unambiguous_display_path(request, raw_path),
    }
}

fn shortest_unambiguous_display_path(request: &RenderRequest, raw_path: &str) -> String {
    let Some(target_relative) = relative_display_path(request, raw_path) else {
        return absolute_display_path(request, raw_path);
    };
    let target_components = path_components(&target_relative);
    if target_components.is_empty() {
        return target_relative;
    }
    let relative_candidates = collect_relative_candidates(request);
    for depth in 1..=target_components.len() {
        let suffix = &target_components[target_components.len() - depth..];
        let unique = relative_candidates.iter().all(|candidate| {
            candidate == &target_components || !ends_with_components(candidate, suffix)
        });
        if unique {
            return join_components(suffix);
        }
    }
    target_relative
}

fn collect_relative_candidates(request: &RenderRequest) -> Vec<Vec<String>> {
    let mut candidates = Vec::new();
    for diagnostic in &request.document.diagnostics {
        collect_node_relative_candidates(request, diagnostic, &mut candidates);
    }
    candidates
}

fn collect_node_relative_candidates(
    request: &RenderRequest,
    node: &DiagnosticNode,
    candidates: &mut Vec<Vec<String>>,
) {
    for location in &node.locations {
        if let Some(relative) = relative_display_path(request, location.path_raw()) {
            candidates.push(path_components(&relative));
        }
    }
    for child in &node.children {
        collect_node_relative_candidates(request, child, candidates);
    }
}

fn relative_display_path(request: &RenderRequest, raw_path: &str) -> Option<String> {
    if preserves_literal_path(raw_path) {
        return None;
    }
    let path = Path::new(raw_path);
    if path.is_absolute() {
        return request
            .cwd
            .as_ref()
            .and_then(|cwd| path.strip_prefix(cwd).ok())
            .map(|relative| relative.display().to_string());
    }
    Some(raw_path.to_string())
}

fn absolute_display_path(request: &RenderRequest, raw_path: &str) -> String {
    if preserves_literal_path(raw_path) {
        return raw_path.to_string();
    }
    let path = Path::new(raw_path);
    if path.is_absolute() {
        path.display().to_string()
    } else {
        request
            .cwd
            .as_ref()
            .map(|cwd| cwd.join(path).display().to_string())
            .unwrap_or_else(|| raw_path.to_string())
    }
}

fn preserves_literal_path(raw_path: &str) -> bool {
    raw_path.starts_with("file://")
        || raw_path.contains("://")
        || raw_path.contains(":\\")
        || (raw_path.starts_with('<') && raw_path.ends_with('>'))
}

fn path_components(path: &str) -> Vec<String> {
    Path::new(path)
        .components()
        .map(|component| component.as_os_str().to_string_lossy().into_owned())
        .collect()
}

fn ends_with_components(candidate: &[String], suffix: &[String]) -> bool {
    candidate.len() >= suffix.len() && candidate[candidate.len() - suffix.len()..] == *suffix
}

fn join_components(components: &[String]) -> String {
    let mut path = PathBuf::new();
    for component in components {
        path.push(component);
    }
    path.display().to_string()
}
