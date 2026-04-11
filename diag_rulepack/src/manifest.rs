//! Rulepack manifest types and loading logic.
//!
//! This module contains the manifest schema types, the [`LoadedRulepack`]
//! bundle, rulepack error types, and all loading/parsing functions.

use crate::validate::{
    hex_sha256, invalid_rulepack, validate_enrich_rulepack, validate_manifest,
    validate_render_rulepack,
};
use crate::validate_residual::validate_residual_rulepack;
use crate::{EnrichRulepack, RenderRulepack, ResidualRulepack};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

// ---------------------------------------------------------------------------
// Schema version constants
// ---------------------------------------------------------------------------

/// Schema version string expected in rulepack manifest files.
pub const RULEPACK_MANIFEST_SCHEMA_VERSION: &str = "diag_rulepack_manifest/v1alpha1";
/// Schema version string expected in enrichment rulepack sections.
pub const ENRICH_RULEPACK_SCHEMA_VERSION: &str = "diag_enrich_rulepack/v1alpha1";
/// Schema version string expected in residual rulepack sections.
pub const RESIDUAL_RULEPACK_SCHEMA_VERSION: &str = "diag_residual_rulepack/v1alpha1";
/// Schema version string expected in render rulepack sections.
pub const RENDER_RULEPACK_SCHEMA_VERSION: &str = "diag_render_rulepack/v1alpha1";
/// Rulepack version identifier for the checked-in (bundled) rule pack.
pub const CHECKED_IN_RULEPACK_VERSION: &str = "phase1";
/// Filename of the checked-in manifest on disk.
pub const CHECKED_IN_MANIFEST_FILE: &str = "diag_rulepack.manifest.phase1.json";

// ---------------------------------------------------------------------------
// Embedded raw bytes
// ---------------------------------------------------------------------------

#[cfg(test)]
pub(crate) const CHECKED_IN_SECTION_FILES: &[&str] = &[
    CHECKED_IN_MANIFEST_FILE,
    "enrich.rulepack.json",
    "residual.rulepack.json",
    "render.rulepack.json",
];

const CHECKED_IN_MANIFEST_RAW: &[u8] =
    include_bytes!("../../rules/diag_rulepack.manifest.phase1.json");
const CHECKED_IN_ENRICH_RAW: &[u8] = include_bytes!("../../rules/enrich.rulepack.json");
const CHECKED_IN_RESIDUAL_RAW: &[u8] = include_bytes!("../../rules/residual.rulepack.json");
const CHECKED_IN_RENDER_RAW: &[u8] = include_bytes!("../../rules/render.rulepack.json");

// ---------------------------------------------------------------------------
// Manifest types
// ---------------------------------------------------------------------------

/// Top-level manifest that lists the sections composing a rule pack.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RulepackManifest {
    /// Schema version identifier for this manifest format.
    pub schema_version: String,
    /// Version tag shared by every section in this rule pack.
    pub rulepack_version: String,
    /// Optional human-readable description.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Ordered list of section references (enrich, residual, render).
    pub sections: Vec<ManifestSection>,
}

/// A single section entry inside a [`RulepackManifest`].
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ManifestSection {
    /// Which section this entry represents (enrich, residual, or render).
    pub kind: SectionKind,
    /// Relative path to the section JSON file.
    pub path: String,
    /// Expected SHA-256 hex digest of the section file contents.
    pub sha256: String,
}

/// Discriminator for the three kinds of rulepack sections.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum SectionKind {
    /// Enrichment (family matching and confidence) section.
    Enrich,
    /// Residual text classification section.
    Residual,
    /// Rendering policy section.
    Render,
}

// ---------------------------------------------------------------------------
// LoadedRulepack
// ---------------------------------------------------------------------------

/// A fully validated, ready-to-use rule pack bundle containing all three sections.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoadedRulepack {
    manifest: RulepackManifest,
    enrich: EnrichRulepack,
    residual: ResidualRulepack,
    render: RenderRulepack,
}

impl LoadedRulepack {
    /// Returns the rulepack version string from the manifest.
    pub fn version(&self) -> &str {
        &self.manifest.rulepack_version
    }

    /// Returns the manifest metadata for this rule pack.
    pub fn manifest(&self) -> &RulepackManifest {
        &self.manifest
    }

    /// Returns the enrichment rulepack section.
    pub fn enrich(&self) -> &EnrichRulepack {
        &self.enrich
    }

    /// Returns the residual rulepack section.
    pub fn residual(&self) -> &ResidualRulepack {
        &self.residual
    }

    /// Returns the render rulepack section.
    pub fn render(&self) -> &RenderRulepack {
        &self.render
    }
}

// ---------------------------------------------------------------------------
// RulepackError
// ---------------------------------------------------------------------------

/// Errors that can occur when loading or validating a rulepack.
#[derive(Debug, thiserror::Error)]
pub enum RulepackError {
    /// A section file could not be read from disk.
    #[error("failed to read {path}: {source}")]
    ReadFile {
        /// Path to the file that could not be read.
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    /// A section file contained invalid JSON.
    #[error("failed to parse JSON in {path}: {source}")]
    ParseJson {
        /// Path to the file with the parse error.
        path: PathBuf,
        #[source]
        source: serde_json::Error,
    },
    /// The SHA-256 digest of a section file did not match the manifest.
    #[error("rulepack digest mismatch for {path}: expected {expected}, got {actual}")]
    DigestMismatch {
        /// Path to the mismatched section file.
        path: PathBuf,
        /// Digest recorded in the manifest.
        expected: String,
        /// Digest computed from the actual file contents.
        actual: String,
    },
    /// A rulepack failed structural or semantic validation.
    #[error("invalid rulepack at {path}: {message}")]
    InvalidRulepack {
        /// Path to the invalid rulepack file.
        path: PathBuf,
        /// Human-readable description of the validation failure.
        message: String,
    },
}

// ---------------------------------------------------------------------------
// Public loading API
// ---------------------------------------------------------------------------

static CHECKED_IN_RULEPACK: OnceLock<LoadedRulepack> = OnceLock::new();

/// Returns the on-disk path to the checked-in rules directory.
pub fn checked_in_rules_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("workspace root")
        .join("rules")
}

/// Returns the on-disk path to the checked-in manifest file.
pub fn checked_in_manifest_path() -> PathBuf {
    checked_in_rules_dir().join(CHECKED_IN_MANIFEST_FILE)
}

/// Returns a reference to the lazily-initialized, embedded checked-in rulepack.
///
/// The rulepack is parsed and validated on first access and cached for the
/// lifetime of the process.
///
/// # Panics
///
/// Panics if the embedded checked-in rulepack fails validation. This is a
/// fail-fast invariant ensuring the bundled rulepack is always valid; a panic
/// here indicates a build-time data error that must be fixed before release.
pub fn checked_in_rulepack() -> &'static LoadedRulepack {
    CHECKED_IN_RULEPACK.get_or_init(|| {
        load_embedded_rulepack().unwrap_or_else(|error| {
            panic!("checked-in diag_rulepack must validate at runtime: {error}");
        })
    })
}

/// Returns the version string of the checked-in rulepack.
pub fn checked_in_rulepack_version() -> &'static str {
    checked_in_rulepack().version()
}

/// Clones and returns the checked-in rulepack.
pub fn load_checked_in_rulepack() -> Result<LoadedRulepack, RulepackError> {
    Ok(checked_in_rulepack().clone())
}

/// Loads a rulepack from an on-disk manifest file, verifying digests and
/// validating all sections.
pub fn load_rulepack_from_manifest(
    manifest_path: impl AsRef<Path>,
) -> Result<LoadedRulepack, RulepackError> {
    let manifest_path = manifest_path.as_ref().to_path_buf();
    let manifest_raw = read_raw_file(&manifest_path)?;
    load_rulepack_from_raw(&manifest_path, &manifest_raw, |section_path| {
        read_raw_file(section_path)
    })
}

// ---------------------------------------------------------------------------
// Private loading helpers
// ---------------------------------------------------------------------------

fn load_embedded_rulepack() -> Result<LoadedRulepack, RulepackError> {
    load_rulepack_from_raw(
        Path::new(CHECKED_IN_MANIFEST_FILE),
        CHECKED_IN_MANIFEST_RAW,
        |section_path| {
            embedded_section_raw(section_path.to_str().unwrap_or_default())
                .map(|raw| raw.to_vec())
                .ok_or_else(|| invalid_rulepack(section_path, "embedded section not found"))
        },
    )
}

fn load_rulepack_from_raw<F>(
    manifest_path: &Path,
    manifest_raw: &[u8],
    mut read_section: F,
) -> Result<LoadedRulepack, RulepackError>
where
    F: FnMut(&Path) -> Result<Vec<u8>, RulepackError>,
{
    let manifest: RulepackManifest = parse_json(manifest_path, manifest_raw)?;
    validate_manifest(&manifest, manifest_path)?;

    let manifest_dir = manifest_path.parent().unwrap_or(Path::new("."));
    let mut enrich = None;
    let mut residual = None;
    let mut render = None;

    for section in &manifest.sections {
        let section_path = manifest_dir.join(&section.path);
        let raw = read_section(&section_path)?;
        let actual_digest = hex_sha256(&raw);
        if actual_digest != section.sha256 {
            return Err(RulepackError::DigestMismatch {
                path: section_path,
                expected: section.sha256.clone(),
                actual: actual_digest,
            });
        }

        match section.kind {
            SectionKind::Enrich => {
                let parsed: EnrichRulepack = parse_json(&section_path, &raw)?;
                validate_enrich_rulepack(&parsed, &section_path, &manifest.rulepack_version)?;
                enrich = Some(parsed);
            }
            SectionKind::Residual => {
                let parsed: ResidualRulepack = parse_json(&section_path, &raw)?;
                validate_residual_rulepack(&parsed, &section_path, &manifest.rulepack_version)?;
                residual = Some(parsed);
            }
            SectionKind::Render => {
                let parsed: RenderRulepack = parse_json(&section_path, &raw)?;
                validate_render_rulepack(&parsed, &section_path, &manifest.rulepack_version)?;
                render = Some(parsed);
            }
        }
    }

    Ok(LoadedRulepack {
        manifest,
        enrich: enrich.ok_or_else(|| {
            invalid_rulepack(manifest_path, "manifest did not resolve an enrich section")
        })?,
        residual: residual.ok_or_else(|| {
            invalid_rulepack(manifest_path, "manifest did not resolve a residual section")
        })?,
        render: render.ok_or_else(|| {
            invalid_rulepack(manifest_path, "manifest did not resolve a render section")
        })?,
    })
}

fn embedded_section_raw(path: &str) -> Option<&'static [u8]> {
    match path {
        "enrich.rulepack.json" => Some(CHECKED_IN_ENRICH_RAW),
        "residual.rulepack.json" => Some(CHECKED_IN_RESIDUAL_RAW),
        "render.rulepack.json" => Some(CHECKED_IN_RENDER_RAW),
        _ => None,
    }
}

fn read_raw_file(path: &Path) -> Result<Vec<u8>, RulepackError> {
    fs::read(path).map_err(|source| RulepackError::ReadFile {
        path: path.to_path_buf(),
        source,
    })
}

fn parse_json<T: DeserializeOwned>(path: &Path, raw: &[u8]) -> Result<T, RulepackError> {
    serde_json::from_slice(raw).map_err(|source| RulepackError::ParseJson {
        path: path.to_path_buf(),
        source,
    })
}
