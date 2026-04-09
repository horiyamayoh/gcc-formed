use diag_core::Ownership;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
struct OwnershipRule {
    ownership: Ownership,
    path_contains_any: &'static [&'static str],
    path_suffixes: &'static [&'static str],
}

const OWNERSHIP_RULES: &[OwnershipRule] = &[
    OwnershipRule {
        ownership: Ownership::System,
        path_contains_any: &["/usr/include", "/usr/lib", "/opt/homebrew"],
        path_suffixes: &[],
    },
    OwnershipRule {
        ownership: Ownership::Vendor,
        path_contains_any: &["/vendor/", "/third_party/", "/external/"],
        path_suffixes: &[],
    },
    OwnershipRule {
        ownership: Ownership::Generated,
        path_contains_any: &["/generated/", "/build/"],
        path_suffixes: &[".generated.h", ".generated.hpp"],
    },
];

pub(crate) fn classify_ownership(path: &str, cwd: &Path) -> Ownership {
    let path = PathBuf::from(path);
    let rendered = path.display().to_string();

    if rendered.is_empty() {
        return Ownership::Unknown;
    }
    if let Some(ownership) = matching_rule(&rendered) {
        return ownership;
    }
    if path.is_relative() || path.starts_with(cwd) {
        return Ownership::User;
    }
    Ownership::Unknown
}

fn matching_rule(rendered: &str) -> Option<Ownership> {
    let normalized = if rendered.starts_with('/') {
        rendered.to_string()
    } else {
        format!("/{rendered}")
    };
    OWNERSHIP_RULES.iter().find_map(|rule| {
        if contains_any(&normalized, rule.path_contains_any)
            || ends_with_any(&normalized, rule.path_suffixes)
        {
            Some(rule.ownership.clone())
        } else {
            None
        }
    })
}

fn contains_any(haystack: &str, needles: &[&str]) -> bool {
    needles.iter().any(|needle| haystack.contains(needle))
}

fn ends_with_any(haystack: &str, suffixes: &[&str]) -> bool {
    suffixes.iter().any(|suffix| haystack.ends_with(suffix))
}
