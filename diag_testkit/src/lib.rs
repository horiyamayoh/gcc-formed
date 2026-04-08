use diag_core::{Confidence, Severity};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FixtureInvoke {
    pub language: String,
    #[serde(default)]
    pub standard: Option<String>,
    pub target_compiler_family: String,
    pub required_support_tier: String,
    pub major_version_selector: String,
    pub argv: Vec<String>,
    pub cwd_policy: String,
    #[serde(default)]
    pub env_overrides: BTreeMap<String, String>,
    pub source_readability_expectation: String,
    pub linker_involvement: bool,
    pub expected_mode: String,
    pub canonical_path_policy: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ExpectedFallback {
    Allowed,
    Forbidden,
    Required,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ExpectedPrimaryLocation {
    pub path: String,
    pub line: u32,
    #[serde(default)]
    pub column: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SemanticExpectations {
    pub family: String,
    pub severity: Severity,
    #[serde(default)]
    pub lead_group_any_of: Vec<String>,
    #[serde(default)]
    pub primary_locations: Vec<ExpectedPrimaryLocation>,
    #[serde(default)]
    pub primary_location_user_owned_required: bool,
    #[serde(default)]
    pub first_action_required: bool,
    #[serde(default)]
    pub raw_provenance_required: bool,
    #[serde(default)]
    pub fallback: Option<ExpectedFallback>,
    #[serde(default)]
    pub confidence_min: Option<Confidence>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct RenderProfileExpectations {
    #[serde(default)]
    pub omission_notice_required: Option<bool>,
    #[serde(default)]
    pub first_screenful_max_lines: Option<usize>,
    #[serde(default)]
    pub path_first_required: Option<bool>,
    #[serde(default)]
    pub color_meaning_forbidden: Option<bool>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct RenderExpectations {
    #[serde(default)]
    pub default: Option<RenderProfileExpectations>,
    #[serde(default)]
    pub concise: Option<RenderProfileExpectations>,
    #[serde(default)]
    pub verbose: Option<RenderProfileExpectations>,
    #[serde(default)]
    pub ci: Option<RenderProfileExpectations>,
    #[serde(default)]
    pub raw_fallback: Option<RenderProfileExpectations>,
}

impl RenderExpectations {
    pub fn named_profiles(&self) -> Vec<(&'static str, &RenderProfileExpectations)> {
        let mut profiles = Vec::new();
        if let Some(expectations) = self.default.as_ref() {
            profiles.push(("default", expectations));
        }
        if let Some(expectations) = self.concise.as_ref() {
            profiles.push(("concise", expectations));
        }
        if let Some(expectations) = self.verbose.as_ref() {
            profiles.push(("verbose", expectations));
        }
        if let Some(expectations) = self.ci.as_ref() {
            profiles.push(("ci", expectations));
        }
        if let Some(expectations) = self.raw_fallback.as_ref() {
            profiles.push(("raw_fallback", expectations));
        }
        profiles
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct IntegrityExpectations {
    #[serde(default)]
    pub allowed_issue_codes: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct PerformanceExpectations {
    #[serde(default)]
    pub parse_time_ms_max: Option<u64>,
    #[serde(default)]
    pub render_time_ms_max: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FixtureExpectations {
    pub schema_version: u32,
    pub fixture_id: String,
    pub support_tier: String,
    pub expected_mode: String,
    #[serde(default)]
    pub family: Option<String>,
    #[serde(default)]
    pub semantic: Option<SemanticExpectations>,
    #[serde(default)]
    pub render: RenderExpectations,
    #[serde(default)]
    pub integrity: IntegrityExpectations,
    #[serde(default)]
    pub performance: PerformanceExpectations,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FixtureMeta {
    #[serde(default)]
    pub corpus_id: Option<String>,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub ownership: Option<String>,
    #[serde(default)]
    pub provenance: Option<String>,
    #[serde(default)]
    pub reviewer: Option<String>,
    #[serde(default)]
    pub redaction_class: Option<String>,
    #[serde(default)]
    pub owner_team: Option<String>,
    #[serde(default)]
    pub last_reviewed: Option<String>,
    #[serde(default)]
    pub reviewers: Vec<String>,
    #[serde(default)]
    pub promotion_status: Option<String>,
    #[serde(default)]
    pub known_version_drift_notes: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct Fixture {
    pub root: PathBuf,
    pub invoke: FixtureInvoke,
    pub expectations: FixtureExpectations,
    pub meta: FixtureMeta,
}

impl Fixture {
    pub fn fixture_id(&self) -> &str {
        &self.expectations.fixture_id
    }

    pub fn family_key(&self) -> String {
        self.root
            .parent()
            .and_then(Path::file_name)
            .and_then(|value| value.to_str())
            .unwrap_or("unknown")
            .to_string()
    }

    pub fn language_key(&self) -> String {
        self.root
            .parent()
            .and_then(Path::parent)
            .and_then(Path::file_name)
            .and_then(|value| value.to_str())
            .unwrap_or("unknown")
            .to_string()
    }

    pub fn snapshot_root(&self) -> PathBuf {
        self.root.join("snapshots").join("gcc15")
    }

    pub fn is_promoted(&self) -> bool {
        self.expectations.semantic.is_some()
    }

    pub fn has_snapshot_artifacts(&self) -> bool {
        self.snapshot_root().join("ir.facts.json").exists()
    }
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
        *counts.entry(fixture.family_key()).or_insert(0) += 1;
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

    if fixture.is_promoted() {
        let semantic = fixture.expectations.semantic.as_ref().ok_or_else(|| {
            FixtureError::Invalid("promoted fixture missing semantic block".to_string())
        })?;
        if semantic.family.trim().is_empty() {
            return Err(FixtureError::Invalid(format!(
                "fixture {} semantic.family must be non-empty",
                fixture.fixture_id()
            )));
        }
        let snapshot_root = fixture.snapshot_root();
        for relative in [
            "stderr.raw",
            "diagnostics.sarif",
            "ir.facts.json",
            "ir.analysis.json",
            "view.default.json",
            "render.default.txt",
            "render.ci.txt",
        ] {
            if !snapshot_root.join(relative).exists() {
                return Err(FixtureError::Invalid(format!(
                    "promoted fixture {} missing {}",
                    fixture.fixture_id(),
                    snapshot_root.join(relative).display()
                )));
            }
        }
        for (profile, _) in fixture.expectations.render.named_profiles() {
            let view = snapshot_root.join(format!("view.{profile}.json"));
            let render = snapshot_root.join(format!("render.{profile}.txt"));
            if !view.exists() {
                return Err(FixtureError::Invalid(format!(
                    "promoted fixture {} missing {}",
                    fixture.fixture_id(),
                    view.display()
                )));
            }
            if !render.exists() {
                return Err(FixtureError::Invalid(format!(
                    "promoted fixture {} missing {}",
                    fixture.fixture_id(),
                    render.display()
                )));
            }
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
    fn counts_fixture_families_from_path() {
        let fixtures = vec![
            Fixture {
                root: PathBuf::from("corpus/c/syntax/case-01"),
                invoke: FixtureInvoke {
                    language: "c".to_string(),
                    standard: None,
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
                    fixture_id: "c/syntax/case-01".to_string(),
                    support_tier: "A".to_string(),
                    expected_mode: "render".to_string(),
                    family: Some("syntax".to_string()),
                    semantic: None,
                    render: RenderExpectations::default(),
                    integrity: IntegrityExpectations::default(),
                    performance: PerformanceExpectations::default(),
                },
                meta: FixtureMeta {
                    corpus_id: None,
                    title: None,
                    tags: vec!["syntax".to_string()],
                    ownership: Some("curated".to_string()),
                    provenance: Some("manual".to_string()),
                    reviewer: Some("codex".to_string()),
                    redaction_class: None,
                    owner_team: None,
                    last_reviewed: None,
                    reviewers: Vec::new(),
                    promotion_status: None,
                    known_version_drift_notes: Vec::new(),
                },
            },
            Fixture {
                root: PathBuf::from("corpus/c/syntax/case-02"),
                invoke: FixtureInvoke {
                    language: "c".to_string(),
                    standard: None,
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
                    fixture_id: "c/syntax/case-02".to_string(),
                    support_tier: "A".to_string(),
                    expected_mode: "render".to_string(),
                    family: Some("syntax".to_string()),
                    semantic: None,
                    render: RenderExpectations::default(),
                    integrity: IntegrityExpectations::default(),
                    performance: PerformanceExpectations::default(),
                },
                meta: FixtureMeta {
                    corpus_id: None,
                    title: None,
                    tags: vec!["syntax".to_string()],
                    ownership: Some("curated".to_string()),
                    provenance: Some("manual".to_string()),
                    reviewer: Some("codex".to_string()),
                    redaction_class: None,
                    owner_team: None,
                    last_reviewed: None,
                    reviewers: Vec::new(),
                    promotion_status: None,
                    known_version_drift_notes: Vec::new(),
                },
            },
        ];
        let counts = family_counts(&fixtures);
        assert_eq!(counts.get("syntax"), Some(&2));
    }

    #[test]
    fn promoted_fixture_requires_semantic_snapshots() {
        let tempdir = tempfile::tempdir().unwrap();
        let root = tempdir.path().join("corpus/c/syntax/case-01");
        fs::create_dir_all(root.join("src")).unwrap();
        fs::create_dir_all(root.join("snapshots/gcc15")).unwrap();
        fs::write(root.join("src/main.c"), "int main(void) { return 0; }\n").unwrap();
        fs::write(
            root.join("invoke.yaml"),
            r#"
language: c
standard: c11
target_compiler_family: gcc
required_support_tier: A
major_version_selector: "15"
argv: ["-c", "src/main.c"]
cwd_policy: fixture_root
env_overrides: { LC_MESSAGES: C }
source_readability_expectation: readable
linker_involvement: false
expected_mode: render
canonical_path_policy: relative_to_cwd
"#,
        )
        .unwrap();
        fs::write(
            root.join("expectations.yaml"),
            r#"
schema_version: 1
fixture_id: c/syntax/case-01
support_tier: A
expected_mode: render
semantic:
  family: syntax
  severity: error
render:
  default:
    first_screenful_max_lines: 24
"#,
        )
        .unwrap();
        fs::write(
            root.join("meta.yaml"),
            r#"
corpus_id: c/syntax/case-01
title: syntax representative
tags: [syntax]
"#,
        )
        .unwrap();

        let fixture = discover(tempdir.path()).unwrap().pop().unwrap();
        let error = validate_fixture(&fixture).unwrap_err().to_string();
        assert!(error.contains("promoted fixture"));
        assert!(error.contains("missing"));
    }
}
