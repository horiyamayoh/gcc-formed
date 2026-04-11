mod snapshot;

pub use snapshot::{
    SnapshotComparison, SnapshotDiffKind, compare_snapshot_contents, normalize_snapshot_contents,
};

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
    pub version_band: String,
    pub support_level: String,
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
    pub first_action_max_line: Option<usize>,
    #[serde(default)]
    pub partial_notice_required: Option<bool>,
    #[serde(default)]
    pub raw_diagnostics_hint_required: Option<bool>,
    #[serde(default)]
    pub raw_sub_block_required: Option<bool>,
    #[serde(default)]
    pub low_confidence_notice_required: Option<bool>,
    #[serde(default)]
    pub path_first_required: Option<bool>,
    #[serde(default)]
    pub color_meaning_forbidden: Option<bool>,
    #[serde(default)]
    pub compaction_required_substrings: Vec<String>,
    #[serde(default)]
    pub compaction_forbidden_substrings: Vec<String>,
    #[serde(default)]
    pub required_substrings: Vec<String>,
    #[serde(default)]
    pub forbidden_substrings: Vec<String>,
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
    pub version_band: String,
    pub processing_path: String,
    pub support_level: String,
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

#[derive(Debug, Clone, Deserialize)]
struct RawFixtureInvoke {
    language: String,
    #[serde(default)]
    standard: Option<String>,
    target_compiler_family: String,
    #[serde(default)]
    version_band: Option<String>,
    #[serde(default)]
    support_level: Option<String>,
    #[serde(default)]
    required_support_tier: Option<String>,
    major_version_selector: String,
    argv: Vec<String>,
    cwd_policy: String,
    #[serde(default)]
    env_overrides: BTreeMap<String, String>,
    source_readability_expectation: String,
    linker_involvement: bool,
    expected_mode: String,
    canonical_path_policy: String,
}

#[derive(Debug, Clone, Deserialize)]
struct RawFixtureExpectations {
    schema_version: u32,
    fixture_id: String,
    #[serde(default)]
    version_band: Option<String>,
    #[serde(default)]
    processing_path: Option<String>,
    #[serde(default)]
    support_level: Option<String>,
    #[serde(default)]
    support_tier: Option<String>,
    expected_mode: String,
    #[serde(default)]
    family: Option<String>,
    #[serde(default)]
    semantic: Option<SemanticExpectations>,
    #[serde(default)]
    render: RenderExpectations,
    #[serde(default)]
    integrity: IntegrityExpectations,
    #[serde(default)]
    performance: PerformanceExpectations,
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

    pub fn declared_snapshot_root(&self) -> PathBuf {
        self.root
            .join("snapshots")
            .join(&self.expectations.version_band)
            .join(&self.expectations.processing_path)
    }

    pub fn legacy_snapshot_root(&self) -> PathBuf {
        self.root.join("snapshots").join("gcc15")
    }

    pub fn snapshot_root(&self) -> PathBuf {
        let declared = self.declared_snapshot_root();
        if declared.exists() {
            return declared;
        }

        let legacy = self.legacy_snapshot_root();
        if legacy.exists() {
            return legacy;
        }

        declared
    }

    pub fn authoritative_structured_artifact_name(&self) -> Option<&'static str> {
        match self.expectations.processing_path.as_str() {
            "dual_sink_structured" => Some("diagnostics.sarif"),
            "single_sink_structured" if self.expectations.version_band == "gcc9_12" => {
                Some("diagnostics.json")
            }
            "single_sink_structured" => Some("diagnostics.sarif"),
            "native_text_capture" | "passthrough" => None,
            _ => None,
        }
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
        let mut required_artifacts = vec![
            "stderr.raw",
            "ir.facts.json",
            "ir.analysis.json",
            "view.default.json",
            "render.default.txt",
            "render.ci.txt",
        ];
        if let Some(artifact_name) = fixture.authoritative_structured_artifact_name() {
            required_artifacts.push(artifact_name);
        }
        for relative in required_artifacts {
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

    if fixture.invoke.version_band != fixture.expectations.version_band {
        return Err(FixtureError::Invalid(format!(
            "fixture {} invoke/expectations version_band mismatch: {} vs {}",
            fixture.fixture_id(),
            fixture.invoke.version_band,
            fixture.expectations.version_band
        )));
    }

    if fixture.invoke.support_level != fixture.expectations.support_level {
        return Err(FixtureError::Invalid(format!(
            "fixture {} invoke/expectations support_level mismatch: {} vs {}",
            fixture.fixture_id(),
            fixture.invoke.support_level,
            fixture.expectations.support_level
        )));
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
    let raw_invoke =
        serde_yaml::from_str::<RawFixtureInvoke>(&fs::read_to_string(root.join("invoke.yaml"))?)?;
    let invoke = normalize_invoke(raw_invoke)?;
    let raw_expectations = serde_yaml::from_str::<RawFixtureExpectations>(&fs::read_to_string(
        root.join("expectations.yaml"),
    )?)?;
    let expectations = normalize_expectations(raw_expectations, &invoke)?;
    let meta = serde_yaml::from_str::<FixtureMeta>(&fs::read_to_string(root.join("meta.yaml"))?)?;
    Ok(Fixture {
        root: root.to_path_buf(),
        invoke,
        expectations,
        meta,
    })
}

fn normalize_invoke(raw: RawFixtureInvoke) -> Result<FixtureInvoke, FixtureError> {
    let version_band = normalize_version_band(
        raw.version_band.as_deref(),
        raw.required_support_tier.as_deref(),
        Some(raw.major_version_selector.as_str()),
    )?;
    let support_level =
        normalize_support_level(raw.support_level.as_deref(), version_band.as_str())?;

    Ok(FixtureInvoke {
        language: raw.language,
        standard: raw.standard,
        target_compiler_family: raw.target_compiler_family,
        version_band,
        support_level,
        major_version_selector: raw.major_version_selector,
        argv: raw.argv,
        cwd_policy: raw.cwd_policy,
        env_overrides: raw.env_overrides,
        source_readability_expectation: raw.source_readability_expectation,
        linker_involvement: raw.linker_involvement,
        expected_mode: raw.expected_mode,
        canonical_path_policy: raw.canonical_path_policy,
    })
}

fn normalize_expectations(
    raw: RawFixtureExpectations,
    invoke: &FixtureInvoke,
) -> Result<FixtureExpectations, FixtureError> {
    let version_band = normalize_version_band(
        raw.version_band.as_deref(),
        raw.support_tier.as_deref(),
        Some(invoke.major_version_selector.as_str()),
    )?;
    let support_level =
        normalize_support_level(raw.support_level.as_deref(), version_band.as_str())?;
    let processing_path = normalize_processing_path(
        raw.processing_path.as_deref(),
        version_band.as_str(),
        raw.expected_mode.as_str(),
    )?;

    Ok(FixtureExpectations {
        schema_version: raw.schema_version,
        fixture_id: raw.fixture_id,
        version_band,
        processing_path,
        support_level,
        expected_mode: raw.expected_mode,
        family: raw.family,
        semantic: raw.semantic,
        render: raw.render,
        integrity: raw.integrity,
        performance: raw.performance,
    })
}

fn normalize_version_band(
    version_band: Option<&str>,
    legacy_support_tier: Option<&str>,
    major_version_selector: Option<&str>,
) -> Result<String, FixtureError> {
    if let Some(value) = version_band
        .or(legacy_support_tier)
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        return match value.to_ascii_lowercase().as_str() {
            "gcc15_plus" | "a" => Ok("gcc15_plus".to_string()),
            "gcc13_14" | "b" => Ok("gcc13_14".to_string()),
            "gcc9_12" | "c" => Ok("gcc9_12".to_string()),
            "unknown" => Ok("unknown".to_string()),
            _ => Err(FixtureError::Invalid(format!(
                "unsupported fixture version_band `{value}`"
            ))),
        };
    }

    match major_version_selector
        .and_then(|selector| selector.parse::<u32>().ok())
        .unwrap_or_default()
    {
        major if major >= 15 => Ok("gcc15_plus".to_string()),
        13 | 14 => Ok("gcc13_14".to_string()),
        9..=12 => Ok("gcc9_12".to_string()),
        _ => Ok("unknown".to_string()),
    }
}

fn normalize_support_level(
    support_level: Option<&str>,
    version_band: &str,
) -> Result<String, FixtureError> {
    if let Some(value) = support_level
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        return match value.to_ascii_lowercase().as_str() {
            "preview" => Ok("preview".to_string()),
            "experimental" => Ok("experimental".to_string()),
            "passthrough_only" => Ok("passthrough_only".to_string()),
            _ => Err(FixtureError::Invalid(format!(
                "unsupported fixture support_level `{value}`"
            ))),
        };
    }

    Ok(match version_band {
        "gcc15_plus" => "preview",
        "gcc13_14" | "gcc9_12" => "experimental",
        _ => "passthrough_only",
    }
    .to_string())
}

fn normalize_processing_path(
    processing_path: Option<&str>,
    version_band: &str,
    expected_mode: &str,
) -> Result<String, FixtureError> {
    if let Some(value) = processing_path
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        return match value.to_ascii_lowercase().as_str() {
            "dual_sink_structured" => Ok("dual_sink_structured".to_string()),
            "single_sink_structured" => Ok("single_sink_structured".to_string()),
            "native_text_capture" => Ok("native_text_capture".to_string()),
            "passthrough" => Ok("passthrough".to_string()),
            _ => Err(FixtureError::Invalid(format!(
                "unsupported fixture processing_path `{value}`"
            ))),
        };
    }

    Ok(
        if expected_mode == "passthrough" || version_band == "unknown" {
            "passthrough"
        } else if version_band == "gcc15_plus" {
            "dual_sink_structured"
        } else {
            "native_text_capture"
        }
        .to_string(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    struct PromotedFixtureSpec<'a> {
        fixture_id: &'a str,
        language: &'a str,
        source_name: &'a str,
        version_band: &'a str,
        support_level: &'a str,
        major_version_selector: &'a str,
        processing_path: &'a str,
        snapshot_layout: &'a str,
    }

    fn write_promoted_fixture(tempdir: &TempDir, spec: PromotedFixtureSpec<'_>) -> PathBuf {
        let root = tempdir.path().join(spec.fixture_id);
        fs::create_dir_all(root.join("src")).unwrap();
        fs::create_dir_all(root.join(spec.snapshot_layout)).unwrap();
        fs::write(
            root.join("src").join(spec.source_name),
            "int main(void) { return 0; }\n",
        )
        .unwrap();
        fs::write(
            root.join("invoke.yaml"),
            format!(
                r#"
language: {language}
standard: c11
target_compiler_family: gcc
version_band: {version_band}
support_level: {support_level}
major_version_selector: "{major_version_selector}"
argv: ["-c", "src/{source_name}"]
cwd_policy: fixture_root
env_overrides: {{ LC_MESSAGES: C }}
source_readability_expectation: readable
linker_involvement: false
expected_mode: render
canonical_path_policy: relative_to_cwd
"#,
                language = spec.language,
                version_band = spec.version_band,
                support_level = spec.support_level,
                major_version_selector = spec.major_version_selector,
                source_name = spec.source_name,
            ),
        )
        .unwrap();
        fs::write(
            root.join("expectations.yaml"),
            format!(
                r#"
schema_version: 1
fixture_id: {fixture_id}
version_band: {version_band}
processing_path: {processing_path}
support_level: {support_level}
expected_mode: render
semantic:
  family: syntax
  severity: error
render:
  default:
    first_screenful_max_lines: 24
"#,
                fixture_id = spec.fixture_id,
                version_band = spec.version_band,
                processing_path = spec.processing_path,
                support_level = spec.support_level,
            ),
        )
        .unwrap();
        fs::write(
            root.join("meta.yaml"),
            format!(
                r#"
corpus_id: {fixture_id}
title: promoted fixture
tags: [syntax]
"#,
                fixture_id = spec.fixture_id,
            ),
        )
        .unwrap();
        root
    }

    fn write_required_promoted_artifacts(snapshot_root: &Path) {
        for relative in [
            "stderr.raw",
            "ir.facts.json",
            "ir.analysis.json",
            "view.default.json",
            "render.default.txt",
            "render.ci.txt",
        ] {
            fs::write(snapshot_root.join(relative), "{}\n").unwrap();
        }
    }

    #[test]
    fn counts_fixture_families_from_path() {
        let fixtures = vec![
            Fixture {
                root: PathBuf::from("corpus/c/syntax/case-01"),
                invoke: FixtureInvoke {
                    language: "c".to_string(),
                    standard: None,
                    target_compiler_family: "gcc".to_string(),
                    version_band: "gcc15_plus".to_string(),
                    support_level: "preview".to_string(),
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
                    version_band: "gcc15_plus".to_string(),
                    processing_path: "dual_sink_structured".to_string(),
                    support_level: "preview".to_string(),
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
                    version_band: "gcc15_plus".to_string(),
                    support_level: "preview".to_string(),
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
                    version_band: "gcc15_plus".to_string(),
                    processing_path: "dual_sink_structured".to_string(),
                    support_level: "preview".to_string(),
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
        let root = write_promoted_fixture(
            &tempdir,
            PromotedFixtureSpec {
                fixture_id: "corpus/c/syntax/case-01",
                language: "c",
                source_name: "main.c",
                version_band: "gcc15_plus",
                support_level: "preview",
                major_version_selector: "15",
                processing_path: "dual_sink_structured",
                snapshot_layout: "snapshots/gcc15_plus/dual_sink_structured",
            },
        );

        let fixture = discover(tempdir.path()).unwrap().pop().unwrap();
        assert_eq!(
            fixture.snapshot_root(),
            root.join("snapshots/gcc15_plus/dual_sink_structured")
        );
        let error = validate_fixture(&fixture).unwrap_err().to_string();
        assert!(error.contains("promoted fixture"));
        assert!(error.contains("missing"));
    }

    #[test]
    fn legacy_tier_fields_normalize_to_current_fixture_vocabulary() {
        let tempdir = tempfile::tempdir().unwrap();
        let root = tempdir.path().join("corpus/c/syntax/case-09");
        fs::create_dir_all(root.join("src")).unwrap();
        fs::create_dir_all(root.join("snapshots/gcc15")).unwrap();
        fs::write(root.join("src/main.c"), "int main(void) { return 0; }\n").unwrap();
        fs::write(
            root.join("invoke.yaml"),
            r#"
language: c
standard: c11
target_compiler_family: gcc
required_support_tier: C
major_version_selector: "12"
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
fixture_id: c/syntax/case-09
support_tier: C
expected_mode: render
"#,
        )
        .unwrap();
        fs::write(
            root.join("meta.yaml"),
            r#"
corpus_id: c/syntax/case-09
title: legacy tier fixture
tags: [syntax]
"#,
        )
        .unwrap();

        let fixture = discover(tempdir.path()).unwrap().pop().unwrap();
        assert_eq!(fixture.snapshot_root(), root.join("snapshots/gcc15"));
        assert_eq!(fixture.invoke.version_band, "gcc9_12");
        assert_eq!(fixture.invoke.support_level, "experimental");
        assert_eq!(fixture.expectations.version_band, "gcc9_12");
        assert_eq!(fixture.expectations.support_level, "experimental");
        assert_eq!(fixture.expectations.processing_path, "native_text_capture");
    }

    #[test]
    fn promoted_native_text_fixture_allows_structured_artifact_to_be_absent() {
        let tempdir = tempfile::tempdir().unwrap();
        let root = write_promoted_fixture(
            &tempdir,
            PromotedFixtureSpec {
                fixture_id: "corpus/c/partial/case-07",
                language: "c",
                source_name: "main.c",
                version_band: "gcc13_14",
                support_level: "experimental",
                major_version_selector: "14",
                processing_path: "native_text_capture",
                snapshot_layout: "snapshots/gcc13_14/native_text_capture",
            },
        );
        let snapshot_root = root.join("snapshots/gcc13_14/native_text_capture");
        write_required_promoted_artifacts(&snapshot_root);

        let fixture = discover(tempdir.path()).unwrap().pop().unwrap();
        assert_eq!(fixture.snapshot_root(), snapshot_root);
        assert_eq!(fixture.authoritative_structured_artifact_name(), None);
        validate_fixture(&fixture).unwrap();
    }

    #[test]
    fn promoted_band_c_single_sink_fixture_requires_json_artifact() {
        let tempdir = tempfile::tempdir().unwrap();
        let root = write_promoted_fixture(
            &tempdir,
            PromotedFixtureSpec {
                fixture_id: "corpus/cpp/overload/case-07",
                language: "cpp",
                source_name: "main.cpp",
                version_band: "gcc9_12",
                support_level: "experimental",
                major_version_selector: "12",
                processing_path: "single_sink_structured",
                snapshot_layout: "snapshots/gcc9_12/single_sink_structured",
            },
        );
        let snapshot_root = root.join("snapshots/gcc9_12/single_sink_structured");
        write_required_promoted_artifacts(&snapshot_root);

        let fixture = discover(tempdir.path()).unwrap().pop().unwrap();
        assert_eq!(fixture.snapshot_root(), snapshot_root);
        assert_eq!(
            fixture.authoritative_structured_artifact_name(),
            Some("diagnostics.json")
        );
        let error = validate_fixture(&fixture).unwrap_err().to_string();
        assert!(error.contains("diagnostics.json"));

        fs::write(snapshot_root.join("diagnostics.json"), "{}\n").unwrap();
        validate_fixture(&fixture).unwrap();
    }
}
