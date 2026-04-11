//! Probes compiler backends for version, capabilities, and support level.

use serde::{Deserialize, Serialize};
use std::collections::{BTreeSet, HashMap};
use std::env;
use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Kind of compiler driver being invoked.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DriverKind {
    /// GNU C compiler driver.
    Gcc,
    /// GNU C++ compiler driver.
    Gxx,
}

/// Tiered classification of backend support quality.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SupportTier {
    /// Full structured diagnostics support (GCC 15+).
    A,
    /// Partial structured diagnostics support (GCC 13-14).
    B,
    /// Minimal support, legacy versions only (GCC 9-12).
    C,
}

/// Broad version band grouping for capability mapping.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VersionBand {
    /// GCC 15 and newer.
    Gcc15Plus,
    /// GCC 13 through 14.
    Gcc13_14,
    /// GCC 9 through 12.
    Gcc9_12,
    /// Unrecognized or unsupported version.
    Unknown,
}

/// Functional support level for a given version band.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SupportLevel {
    /// Full feature set available as a preview.
    Preview,
    /// Partial feature set with known limitations.
    Experimental,
    /// Only unmodified passthrough execution is supported.
    PassthroughOnly,
}

/// Diagnostic processing strategy selected for an invocation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProcessingPath {
    /// Structured output alongside native text via dual-sink flags.
    DualSinkStructured,
    /// Structured output replaces native text via single-sink flags.
    SingleSinkStructured,
    /// Only native stderr text is captured.
    NativeTextCapture,
    /// No capture; the backend runs unmodified.
    Passthrough,
}

/// Describes the diagnostic capabilities of a specific backend version.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CapabilityProfile {
    /// Version band this profile was derived from.
    pub version_band: VersionBand,
    /// Functional support level for this version.
    pub support_level: SupportLevel,
    /// Whether native stderr text capture is available.
    pub native_text_capture: bool,
    /// Whether JSON diagnostic output is supported.
    pub json_diagnostics: bool,
    /// Whether SARIF diagnostic output is supported.
    pub sarif_diagnostics: bool,
    /// Whether dual-sink structured output is supported.
    pub dual_sink: bool,
    /// Whether TTY color control flags are honored.
    pub tty_color_control: bool,
    /// Whether caret diagnostic control is available.
    pub caret_control: bool,
    /// Whether parseable fix-it hints are supported.
    pub parseable_fixits: bool,
    /// Whether locale can be stabilized for reproducible output.
    pub locale_stabilization: bool,
    /// Recommended processing path for this version.
    pub default_processing_path: ProcessingPath,
    /// Set of processing paths this version can use.
    pub allowed_processing_paths: BTreeSet<ProcessingPath>,
}

/// Filesystem identity key used to cache probe results.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ProbeKey {
    /// Canonical path to the backend binary.
    pub realpath: PathBuf,
    /// Inode number (Unix) or zero (non-Unix).
    pub inode: u64,
    /// Last modification time in seconds since the Unix epoch.
    pub mtime_seconds: i64,
    /// File size in bytes.
    pub size_bytes: u64,
}

/// Result of probing a compiler backend for version and capabilities.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProbeResult {
    /// Backend name as originally requested.
    pub requested_backend: String,
    /// Canonical filesystem path to the resolved backend binary.
    pub resolved_path: PathBuf,
    /// Raw version string returned by the backend.
    pub version_string: String,
    /// Parsed major version number.
    pub major: u32,
    /// Parsed minor version number.
    pub minor: u32,
    /// Support tier derived from the major version.
    pub support_tier: SupportTier,
    /// Whether the backend is a C or C++ driver.
    pub driver_kind: DriverKind,
    /// Whether the backend supports `-fdiagnostics-add-output=sarif`.
    pub add_output_sarif_supported: bool,
    /// Filesystem identity key for cache invalidation.
    pub version_probe_key: ProbeKey,
}

impl ProbeResult {
    /// Returns the version band for this probe's major version.
    pub fn version_band(&self) -> VersionBand {
        version_band_for_major(self.major)
    }

    /// Returns the support level for this probe's version band.
    pub fn support_level(&self) -> SupportLevel {
        support_level_for_version_band(self.version_band())
    }

    /// Returns the default processing path for this probe's version band.
    pub fn default_processing_path(&self) -> ProcessingPath {
        default_processing_path_for_version_band(self.version_band())
    }

    /// Builds a full capability profile from this probe result.
    pub fn capability_profile(&self) -> CapabilityProfile {
        capability_profile_for_probe(self)
    }
}

/// Parameters for resolving a backend compiler path.
#[derive(Debug, Clone)]
pub struct ResolveRequest {
    /// Explicitly configured backend path, if any.
    pub explicit_backend: Option<PathBuf>,
    /// Backend path from an environment variable, if any.
    pub env_backend: Option<PathBuf>,
    /// Name the wrapper was invoked as (e.g. `gcc-formed`).
    pub invoked_as: String,
}

/// In-memory cache of probe results keyed by filesystem identity.
#[derive(Debug, Default)]
pub struct ProbeCache {
    entries: HashMap<ProbeKey, ProbeResult>,
}

/// Errors that can occur during backend probing.
#[derive(Debug, thiserror::Error)]
pub enum ProbeError {
    /// The requested backend binary could not be found on disk or in PATH.
    #[error("backend compiler was not found: {0}")]
    NotFound(String),
    /// Filesystem metadata for the backend binary could not be read.
    #[error("failed to inspect backend metadata for {path}: {source}")]
    Metadata {
        /// Path that was inspected.
        path: PathBuf,
        #[source]
        /// Underlying I/O error.
        source: std::io::Error,
    },
    /// Running `--version` on the backend failed.
    #[error("failed to probe backend version for {path}: {source}")]
    VersionProbe {
        /// Path to the backend that was probed.
        path: PathBuf,
        #[source]
        /// Underlying I/O error.
        source: std::io::Error,
    },
    /// The version string could not be parsed into major/minor components.
    #[error("backend version output was not parseable: {0}")]
    UnparseableVersion(String),
}

impl ProbeCache {
    /// Returns a cached probe result or performs a fresh probe.
    pub fn get_or_probe(&mut self, request: ResolveRequest) -> Result<ProbeResult, ProbeError> {
        let path = resolve_backend(&request)?;
        let key = probe_key(&path)?;
        if let Some(cached) = self.entries.get(&key) {
            return Ok(cached.clone());
        }
        let result = probe_backend(&path, request.invoked_as)?;
        self.entries.insert(key, result.clone());
        Ok(result)
    }
}

/// Resolves the backend compiler path from explicit, env, or PATH lookup.
pub fn resolve_backend(request: &ResolveRequest) -> Result<PathBuf, ProbeError> {
    if let Some(explicit) = request.explicit_backend.as_ref() {
        return canonicalize_candidate(explicit);
    }
    if let Some(from_env) = request.env_backend.as_ref() {
        return canonicalize_candidate(from_env);
    }
    let default = default_backend_name(&request.invoked_as);
    find_in_path(default).ok_or_else(|| ProbeError::NotFound(default.to_string()))
}

/// Returns the default backend binary name based on how the wrapper was invoked.
pub fn default_backend_name(invoked_as: &str) -> &'static str {
    if invoked_as.contains("g++") || invoked_as.contains("c++") {
        "g++"
    } else {
        "gcc"
    }
}

/// Probes the backend at `path` for version info and builds a [`ProbeResult`].
pub fn probe_backend(path: &Path, invoked_as: String) -> Result<ProbeResult, ProbeError> {
    let output = Command::new(path)
        .arg("--version")
        .output()
        .map_err(|source| ProbeError::VersionProbe {
            path: path.to_path_buf(),
            source,
        })?;
    let version_string = String::from_utf8_lossy(&output.stdout)
        .lines()
        .next()
        .unwrap_or_default()
        .trim()
        .to_string();
    let (major, minor) = parse_version(&version_string)?;
    let key = probe_key(path)?;
    Ok(ProbeResult {
        requested_backend: invoked_as,
        resolved_path: path.to_path_buf(),
        version_string,
        major,
        minor,
        support_tier: support_tier_for_major(major),
        driver_kind: driver_kind_for_path(path),
        add_output_sarif_supported: major >= 15,
        version_probe_key: key,
    })
}

/// Maps a GCC major version to its support tier.
pub fn support_tier_for_major(major: u32) -> SupportTier {
    match major {
        15.. => SupportTier::A,
        13 | 14 => SupportTier::B,
        _ => SupportTier::C,
    }
}

/// Maps a GCC major version to its version band.
pub fn version_band_for_major(major: u32) -> VersionBand {
    match major {
        15.. => VersionBand::Gcc15Plus,
        13 | 14 => VersionBand::Gcc13_14,
        9..=12 => VersionBand::Gcc9_12,
        _ => VersionBand::Unknown,
    }
}

/// Returns the support level for a given version band.
pub fn support_level_for_version_band(version_band: VersionBand) -> SupportLevel {
    match version_band {
        VersionBand::Gcc15Plus => SupportLevel::Preview,
        VersionBand::Gcc13_14 | VersionBand::Gcc9_12 => SupportLevel::Experimental,
        VersionBand::Unknown => SupportLevel::PassthroughOnly,
    }
}

/// Returns the default processing path for a given version band.
pub fn default_processing_path_for_version_band(version_band: VersionBand) -> ProcessingPath {
    match version_band {
        VersionBand::Gcc15Plus => ProcessingPath::DualSinkStructured,
        VersionBand::Gcc13_14 | VersionBand::Gcc9_12 => ProcessingPath::NativeTextCapture,
        VersionBand::Unknown => ProcessingPath::Passthrough,
    }
}

/// Builds a capability profile from a GCC major version number.
pub fn capability_profile_for_major(major: u32) -> CapabilityProfile {
    capability_profile_for_version_band(version_band_for_major(major), major >= 15)
}

/// Builds a capability profile from a completed probe result.
pub fn capability_profile_for_probe(probe: &ProbeResult) -> CapabilityProfile {
    capability_profile_for_version_band(probe.version_band(), probe.add_output_sarif_supported)
}

fn capability_profile_for_version_band(
    version_band: VersionBand,
    add_output_sarif_supported: bool,
) -> CapabilityProfile {
    // Keep the new vocabulary behind a probe-local compatibility seam until a
    // later issue installs dedicated path/capability probing.
    let sarif_diagnostics = matches!(version_band, VersionBand::Gcc15Plus | VersionBand::Gcc13_14);
    let dual_sink = matches!(version_band, VersionBand::Gcc15Plus) && add_output_sarif_supported;

    match version_band {
        VersionBand::Gcc15Plus => CapabilityProfile {
            version_band,
            support_level: support_level_for_version_band(version_band),
            native_text_capture: true,
            json_diagnostics: false,
            sarif_diagnostics,
            dual_sink,
            tty_color_control: true,
            caret_control: false,
            parseable_fixits: false,
            locale_stabilization: false,
            default_processing_path: default_processing_path_for_version_band(version_band),
            allowed_processing_paths: BTreeSet::from([
                ProcessingPath::DualSinkStructured,
                ProcessingPath::Passthrough,
            ]),
        },
        VersionBand::Gcc13_14 => CapabilityProfile {
            version_band,
            support_level: support_level_for_version_band(version_band),
            native_text_capture: true,
            json_diagnostics: false,
            sarif_diagnostics,
            dual_sink,
            tty_color_control: false,
            caret_control: false,
            parseable_fixits: false,
            locale_stabilization: false,
            default_processing_path: default_processing_path_for_version_band(version_band),
            allowed_processing_paths: BTreeSet::from([
                ProcessingPath::NativeTextCapture,
                ProcessingPath::SingleSinkStructured,
                ProcessingPath::Passthrough,
            ]),
        },
        VersionBand::Gcc9_12 => CapabilityProfile {
            version_band,
            support_level: support_level_for_version_band(version_band),
            native_text_capture: true,
            json_diagnostics: true,
            sarif_diagnostics,
            dual_sink,
            tty_color_control: false,
            caret_control: false,
            parseable_fixits: false,
            locale_stabilization: false,
            default_processing_path: ProcessingPath::NativeTextCapture,
            allowed_processing_paths: BTreeSet::from([
                ProcessingPath::NativeTextCapture,
                ProcessingPath::SingleSinkStructured,
                ProcessingPath::Passthrough,
            ]),
        },
        VersionBand::Unknown => CapabilityProfile {
            version_band,
            support_level: support_level_for_version_band(version_band),
            native_text_capture: true,
            json_diagnostics: false,
            sarif_diagnostics,
            dual_sink,
            tty_color_control: false,
            caret_control: false,
            parseable_fixits: false,
            locale_stabilization: false,
            default_processing_path: default_processing_path_for_version_band(version_band),
            allowed_processing_paths: BTreeSet::from([ProcessingPath::Passthrough]),
        },
    }
}

fn parse_version(line: &str) -> Result<(u32, u32), ProbeError> {
    for token in line.split_whitespace() {
        let parts = token.split('.').collect::<Vec<_>>();
        if parts.len() < 2 {
            continue;
        }
        let maybe_major = parts[0].parse::<u32>();
        let maybe_minor = parts[1].parse::<u32>();
        if let (Ok(major), Ok(minor)) = (maybe_major, maybe_minor) {
            return Ok((major, minor));
        }
    }
    Err(ProbeError::UnparseableVersion(line.to_string()))
}

fn driver_kind_for_path(path: &Path) -> DriverKind {
    let name = path.file_name().and_then(OsStr::to_str).unwrap_or_default();
    if name.contains("++") || name.contains("gpp") {
        DriverKind::Gxx
    } else {
        DriverKind::Gcc
    }
}

fn probe_key(path: &Path) -> Result<ProbeKey, ProbeError> {
    let metadata = fs::metadata(path).map_err(|source| ProbeError::Metadata {
        path: path.to_path_buf(),
        source,
    })?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        Ok(ProbeKey {
            realpath: path.to_path_buf(),
            inode: metadata.ino(),
            mtime_seconds: metadata.mtime(),
            size_bytes: metadata.size(),
        })
    }
    #[cfg(not(unix))]
    {
        let modified = metadata
            .modified()
            .ok()
            .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|duration| duration.as_secs() as i64)
            .unwrap_or(0);
        Ok(ProbeKey {
            realpath: path.to_path_buf(),
            inode: 0,
            mtime_seconds: modified,
            size_bytes: metadata.len(),
        })
    }
}

fn find_in_path(binary: &str) -> Option<PathBuf> {
    let path = env::var_os("PATH")?;
    env::split_paths(&path)
        .map(|dir| dir.join(binary))
        .find(|candidate| candidate.exists())
        .and_then(|candidate| canonicalize_candidate(&candidate).ok())
}

fn canonicalize_candidate(path: &Path) -> Result<PathBuf, ProbeError> {
    fs::canonicalize(path).map_err(|_| ProbeError::NotFound(path.display().to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn picks_cplusplus_driver_from_invocation_name() {
        assert_eq!(default_backend_name("g++-formed"), "g++");
        assert_eq!(default_backend_name("gcc-formed"), "gcc");
    }

    #[test]
    fn maps_support_tiers() {
        assert_eq!(support_tier_for_major(15), SupportTier::A);
        assert_eq!(support_tier_for_major(13), SupportTier::B);
        assert_eq!(support_tier_for_major(12), SupportTier::C);
    }

    #[test]
    fn maps_version_bands() {
        assert_eq!(version_band_for_major(15), VersionBand::Gcc15Plus);
        assert_eq!(version_band_for_major(13), VersionBand::Gcc13_14);
        assert_eq!(version_band_for_major(9), VersionBand::Gcc9_12);
        assert_eq!(version_band_for_major(8), VersionBand::Unknown);
    }

    #[test]
    fn maps_support_levels_and_default_processing_paths_from_version_bands() {
        assert_eq!(
            support_level_for_version_band(VersionBand::Gcc15Plus),
            SupportLevel::Preview
        );
        assert_eq!(
            support_level_for_version_band(VersionBand::Gcc13_14),
            SupportLevel::Experimental
        );
        assert_eq!(
            support_level_for_version_band(VersionBand::Unknown),
            SupportLevel::PassthroughOnly
        );
        assert_eq!(
            default_processing_path_for_version_band(VersionBand::Gcc15Plus),
            ProcessingPath::DualSinkStructured
        );
        assert_eq!(
            default_processing_path_for_version_band(VersionBand::Gcc13_14),
            ProcessingPath::NativeTextCapture
        );
    }

    #[test]
    fn maps_band_specific_support_levels_and_default_processing_paths() {
        assert_eq!(
            support_level_for_version_band(VersionBand::Gcc9_12),
            SupportLevel::Experimental
        );
        assert_eq!(
            support_level_for_version_band(VersionBand::Unknown),
            SupportLevel::PassthroughOnly
        );
        assert_eq!(
            default_processing_path_for_version_band(VersionBand::Gcc9_12),
            ProcessingPath::NativeTextCapture
        );
        assert_eq!(
            default_processing_path_for_version_band(VersionBand::Unknown),
            ProcessingPath::Passthrough
        );
    }

    #[test]
    fn builds_capability_profiles_without_mutating_legacy_surface() {
        let gcc15 = capability_profile_for_major(15);
        assert_eq!(gcc15.version_band, VersionBand::Gcc15Plus);
        assert_eq!(gcc15.support_level, SupportLevel::Preview);
        assert!(gcc15.sarif_diagnostics);
        assert!(gcc15.dual_sink);
        assert!(gcc15.tty_color_control);
        assert!(!gcc15.json_diagnostics);
        assert!(
            gcc15
                .allowed_processing_paths
                .contains(&ProcessingPath::DualSinkStructured)
        );

        let gcc13 = capability_profile_for_major(13);
        assert_eq!(gcc13.version_band, VersionBand::Gcc13_14);
        assert_eq!(gcc13.support_level, SupportLevel::Experimental);
        assert!(!gcc13.json_diagnostics);
        assert!(gcc13.sarif_diagnostics);
        assert!(!gcc13.dual_sink);
        assert!(!gcc13.tty_color_control);
        assert_eq!(
            gcc13.default_processing_path,
            ProcessingPath::NativeTextCapture
        );
        assert!(
            gcc13
                .allowed_processing_paths
                .contains(&ProcessingPath::SingleSinkStructured)
        );
        assert!(
            gcc13
                .allowed_processing_paths
                .contains(&ProcessingPath::NativeTextCapture)
        );

        let gcc12 = capability_profile_for_major(12);
        assert_eq!(gcc12.version_band, VersionBand::Gcc9_12);
        assert_eq!(gcc12.support_level, SupportLevel::Experimental);
        assert!(gcc12.json_diagnostics);
        assert_eq!(
            gcc12.default_processing_path,
            ProcessingPath::NativeTextCapture
        );
        assert!(
            gcc12
                .allowed_processing_paths
                .contains(&ProcessingPath::SingleSinkStructured)
        );
        assert!(
            gcc12
                .allowed_processing_paths
                .contains(&ProcessingPath::NativeTextCapture)
        );
        assert!(
            gcc12
                .allowed_processing_paths
                .contains(&ProcessingPath::Passthrough)
        );
        assert!(!gcc12.sarif_diagnostics);
        assert!(!gcc12.dual_sink);

        let gcc8 = capability_profile_for_major(8);
        assert_eq!(gcc8.version_band, VersionBand::Unknown);
        assert_eq!(gcc8.support_level, SupportLevel::PassthroughOnly);
        assert_eq!(gcc8.default_processing_path, ProcessingPath::Passthrough);
        assert_eq!(
            gcc8.allowed_processing_paths,
            BTreeSet::from([ProcessingPath::Passthrough])
        );
    }

    #[test]
    fn exposes_probe_result_accessors_additively() {
        let probe = ProbeResult {
            requested_backend: "gcc-formed".to_string(),
            resolved_path: PathBuf::from("/usr/bin/gcc"),
            version_string: "gcc (GCC) 15.1.0".to_string(),
            major: 15,
            minor: 1,
            support_tier: SupportTier::A,
            driver_kind: DriverKind::Gcc,
            add_output_sarif_supported: true,
            version_probe_key: ProbeKey {
                realpath: PathBuf::from("/usr/bin/gcc"),
                inode: 1,
                mtime_seconds: 0,
                size_bytes: 1,
            },
        };

        assert_eq!(probe.version_band(), VersionBand::Gcc15Plus);
        assert_eq!(probe.support_level(), SupportLevel::Preview);
        assert_eq!(
            probe.default_processing_path(),
            ProcessingPath::DualSinkStructured
        );
        assert!(probe.capability_profile().dual_sink);
        assert_eq!(probe.support_tier, SupportTier::A);
        assert!(probe.add_output_sarif_supported);
    }

    #[test]
    fn probe_result_accessors_reflect_band_c_contract() {
        let probe = ProbeResult {
            requested_backend: "gcc-formed".to_string(),
            resolved_path: PathBuf::from("/usr/bin/gcc-12"),
            version_string: "gcc (GCC) 12.3.0".to_string(),
            major: 12,
            minor: 3,
            support_tier: SupportTier::C,
            driver_kind: DriverKind::Gcc,
            add_output_sarif_supported: false,
            version_probe_key: ProbeKey {
                realpath: PathBuf::from("/usr/bin/gcc-12"),
                inode: 2,
                mtime_seconds: 0,
                size_bytes: 2,
            },
        };

        assert_eq!(probe.version_band(), VersionBand::Gcc9_12);
        assert_eq!(probe.support_level(), SupportLevel::Experimental);
        assert_eq!(
            probe.default_processing_path(),
            ProcessingPath::NativeTextCapture
        );
        assert_eq!(
            probe.capability_profile().allowed_processing_paths,
            BTreeSet::from([
                ProcessingPath::NativeTextCapture,
                ProcessingPath::SingleSinkStructured,
                ProcessingPath::Passthrough,
            ])
        );
    }

    #[test]
    fn band_b_capability_profile_distinguishes_single_sink_sarif_from_dual_sink() {
        let probe = ProbeResult {
            requested_backend: "gcc-formed".to_string(),
            resolved_path: PathBuf::from("/usr/bin/gcc-13"),
            version_string: "gcc (GCC) 13.3.0".to_string(),
            major: 13,
            minor: 3,
            support_tier: SupportTier::B,
            driver_kind: DriverKind::Gcc,
            add_output_sarif_supported: false,
            version_probe_key: ProbeKey {
                realpath: PathBuf::from("/usr/bin/gcc-13"),
                inode: 3,
                mtime_seconds: 0,
                size_bytes: 3,
            },
        };

        let profile = probe.capability_profile();
        assert_eq!(profile.version_band, VersionBand::Gcc13_14);
        assert_eq!(
            profile.default_processing_path,
            ProcessingPath::NativeTextCapture
        );
        assert!(profile.sarif_diagnostics);
        assert!(!profile.dual_sink);
        assert!(!probe.add_output_sarif_supported);
        assert_eq!(
            profile.allowed_processing_paths,
            BTreeSet::from([
                ProcessingPath::NativeTextCapture,
                ProcessingPath::SingleSinkStructured,
                ProcessingPath::Passthrough,
            ])
        );
    }

    #[test]
    fn band_c_capability_profile_keeps_json_single_sink_without_sarif() {
        let profile = capability_profile_for_major(11);
        assert_eq!(profile.version_band, VersionBand::Gcc9_12);
        assert!(profile.json_diagnostics);
        assert!(!profile.sarif_diagnostics);
        assert!(!profile.dual_sink);
        assert!(
            profile
                .allowed_processing_paths
                .contains(&ProcessingPath::SingleSinkStructured)
        );
    }

    #[test]
    fn parses_version_line() {
        let parsed = parse_version("gcc (Ubuntu 15.1.0-1) 15.1.0").unwrap();
        assert_eq!(parsed, (15, 1));
    }
}
