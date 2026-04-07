use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FixtureInvoke {
    pub language: String,
    pub target_compiler_family: String,
    pub required_support_tier: String,
    pub major_version_selector: String,
    pub argv: Vec<String>,
    pub cwd_policy: String,
    pub env_overrides: BTreeMap<String, String>,
    pub source_readability_expectation: String,
    pub linker_involvement: bool,
    pub expected_mode: String,
    pub canonical_path_policy: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FixtureExpectations {
    pub schema_version: u32,
    pub fixture_id: String,
    pub support_tier: String,
    pub expected_mode: String,
    pub family: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FixtureMeta {
    pub tags: Vec<String>,
    pub ownership: String,
    pub provenance: String,
    pub reviewer: String,
}

#[derive(Debug, Clone)]
pub struct Fixture {
    pub root: PathBuf,
    pub invoke: FixtureInvoke,
    pub expectations: FixtureExpectations,
    pub meta: FixtureMeta,
}

#[derive(Debug, thiserror::Error)]
pub enum FixtureError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("yaml error: {0}")]
    Yaml(#[from] serde_yaml::Error),
    #[error("fixture layout invalid: {0}")]
    Invalid(String),
}

pub fn discover(root: &Path) -> Result<Vec<Fixture>, FixtureError> {
    let mut fixtures = Vec::new();
    walk(root, &mut fixtures)?;
    fixtures.sort_by(|left, right| left.root.cmp(&right.root));
    Ok(fixtures)
}

pub fn family_counts(fixtures: &[Fixture]) -> BTreeMap<String, usize> {
    let mut counts = BTreeMap::new();
    for fixture in fixtures {
        *counts
            .entry(fixture.expectations.family.clone())
            .or_insert(0) += 1;
    }
    counts
}

pub fn validate_fixture(fixture: &Fixture) -> Result<(), FixtureError> {
    for relative in [
        "src",
        "invoke.yaml",
        "expectations.yaml",
        "meta.yaml",
        "snapshots",
    ] {
        if !fixture.root.join(relative).exists() {
            return Err(FixtureError::Invalid(format!(
                "fixture {} missing {relative}",
                fixture.root.display()
            )));
        }
    }
    Ok(())
}

fn walk(root: &Path, fixtures: &mut Vec<Fixture>) -> Result<(), FixtureError> {
    if !root.exists() {
        return Ok(());
    }
    let mut entries = fs::read_dir(root)?
        .filter_map(Result::ok)
        .collect::<Vec<_>>();
    entries.sort_by_key(|entry| entry.path());
    let has_fixture_files = entries
        .iter()
        .any(|entry| entry.file_name() == "invoke.yaml");
    if has_fixture_files {
        fixtures.push(load_fixture(root)?);
        return Ok(());
    }
    for entry in entries {
        let path = entry.path();
        if path.is_dir() {
            walk(&path, fixtures)?;
        }
    }
    Ok(())
}

fn load_fixture(root: &Path) -> Result<Fixture, FixtureError> {
    let invoke =
        serde_yaml::from_str::<FixtureInvoke>(&fs::read_to_string(root.join("invoke.yaml"))?)?;
    let expectations = serde_yaml::from_str::<FixtureExpectations>(&fs::read_to_string(
        root.join("expectations.yaml"),
    )?)?;
    let meta = serde_yaml::from_str::<FixtureMeta>(&fs::read_to_string(root.join("meta.yaml"))?)?;
    Ok(Fixture {
        root: root.to_path_buf(),
        invoke,
        expectations,
        meta,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_fixture_families() {
        let fixtures = vec![
            Fixture {
                root: PathBuf::from("a"),
                invoke: FixtureInvoke {
                    language: "c".to_string(),
                    target_compiler_family: "gcc".to_string(),
                    required_support_tier: "A".to_string(),
                    major_version_selector: "15".to_string(),
                    argv: vec!["-c".to_string()],
                    cwd_policy: "fixture_root".to_string(),
                    env_overrides: BTreeMap::new(),
                    source_readability_expectation: "readable".to_string(),
                    linker_involvement: false,
                    expected_mode: "render".to_string(),
                    canonical_path_policy: "relative_to_cwd".to_string(),
                },
                expectations: FixtureExpectations {
                    schema_version: 1,
                    fixture_id: "a".to_string(),
                    support_tier: "A".to_string(),
                    expected_mode: "render".to_string(),
                    family: "syntax".to_string(),
                },
                meta: FixtureMeta {
                    tags: vec!["syntax".to_string()],
                    ownership: "curated".to_string(),
                    provenance: "manual".to_string(),
                    reviewer: "codex".to_string(),
                },
            },
            Fixture {
                root: PathBuf::from("b"),
                invoke: FixtureInvoke {
                    language: "c".to_string(),
                    target_compiler_family: "gcc".to_string(),
                    required_support_tier: "A".to_string(),
                    major_version_selector: "15".to_string(),
                    argv: vec!["-c".to_string()],
                    cwd_policy: "fixture_root".to_string(),
                    env_overrides: BTreeMap::new(),
                    source_readability_expectation: "readable".to_string(),
                    linker_involvement: false,
                    expected_mode: "render".to_string(),
                    canonical_path_policy: "relative_to_cwd".to_string(),
                },
                expectations: FixtureExpectations {
                    schema_version: 1,
                    fixture_id: "b".to_string(),
                    support_tier: "A".to_string(),
                    expected_mode: "render".to_string(),
                    family: "syntax".to_string(),
                },
                meta: FixtureMeta {
                    tags: vec!["syntax".to_string()],
                    ownership: "curated".to_string(),
                    provenance: "manual".to_string(),
                    reviewer: "codex".to_string(),
                },
            },
        ];
        let counts = family_counts(&fixtures);
        assert_eq!(counts.get("syntax"), Some(&2));
    }
}
