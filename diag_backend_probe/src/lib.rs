use serde::{Deserialize, Serialize};
use std::collections::{BTreeSet, HashMap};
use std::env;
use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DriverKind {
    Gcc,
    Gxx,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SupportTier {
    A,
    B,
    C,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VersionBand {
    Gcc15Plus,
    Gcc13_14,
    Gcc9_12,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SupportLevel {
    Primary,
    Supported,
    Conservative,
    Experimental,
    Unsupported,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProcessingPath {
    DualSinkStructured,
    SingleSinkStructured,
    NativeTextCapture,
    Passthrough,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CapabilityProfile {
    pub version_band: VersionBand,
    pub support_level: SupportLevel,
    pub native_text_capture: bool,
    pub json_diagnostics: bool,
    pub sarif_diagnostics: bool,
    pub dual_sink: bool,
    pub tty_color_control: bool,
    pub caret_control: bool,
    pub parseable_fixits: bool,
    pub locale_stabilization: bool,
    pub default_processing_path: ProcessingPath,
    pub allowed_processing_paths: BTreeSet<ProcessingPath>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ProbeKey {
    pub realpath: PathBuf,
    pub inode: u64,
    pub mtime_seconds: i64,
    pub size_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProbeResult {
    pub requested_backend: String,
    pub resolved_path: PathBuf,
    pub version_string: String,
    pub major: u32,
    pub minor: u32,
    pub support_tier: SupportTier,
    pub driver_kind: DriverKind,
    pub add_output_sarif_supported: bool,
    pub version_probe_key: ProbeKey,
}

impl ProbeResult {
    pub fn version_band(&self) -> VersionBand {
        version_band_for_major(self.major)
    }

    pub fn support_level(&self) -> SupportLevel {
        support_level_for_version_band(self.version_band(), self.support_tier)
    }

    pub fn default_processing_path(&self) -> ProcessingPath {
        default_processing_path_for_version_band(self.version_band(), self.support_tier)
    }

    pub fn capability_profile(&self) -> CapabilityProfile {
        capability_profile_for_probe(self)
    }
}

#[derive(Debug, Clone)]
pub struct ResolveRequest {
    pub explicit_backend: Option<PathBuf>,
    pub env_backend: Option<PathBuf>,
    pub invoked_as: String,
}

#[derive(Debug, Default)]
pub struct ProbeCache {
    entries: HashMap<ProbeKey, ProbeResult>,
}

#[derive(Debug, thiserror::Error)]
pub enum ProbeError {
    #[error("backend compiler was not found: {0}")]
    NotFound(String),
    #[error("failed to inspect backend metadata for {path}: {source}")]
    Metadata {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("failed to probe backend version for {path}: {source}")]
    VersionProbe {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("backend version output was not parseable: {0}")]
    UnparseableVersion(String),
}

impl ProbeCache {
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

pub fn default_backend_name(invoked_as: &str) -> &'static str {
    if invoked_as.contains("g++") || invoked_as.contains("c++") {
        "g++"
    } else {
        "gcc"
    }
}

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

pub fn support_tier_for_major(major: u32) -> SupportTier {
    match major {
        15.. => SupportTier::A,
        13 | 14 => SupportTier::B,
        _ => SupportTier::C,
    }
}

pub fn version_band_for_major(major: u32) -> VersionBand {
    match major {
        15.. => VersionBand::Gcc15Plus,
        13 | 14 => VersionBand::Gcc13_14,
        9..=12 => VersionBand::Gcc9_12,
        _ => VersionBand::Unknown,
    }
}

pub fn support_level_for_tier(tier: SupportTier) -> SupportLevel {
    match tier {
        SupportTier::A => SupportLevel::Primary,
        SupportTier::B => SupportLevel::Conservative,
        SupportTier::C => SupportLevel::Unsupported,
    }
}

fn support_level_for_version_band(
    version_band: VersionBand,
    tier: SupportTier,
) -> SupportLevel {
    match version_band {
        VersionBand::Gcc9_12 => SupportLevel::Experimental,
        VersionBand::Gcc15Plus | VersionBand::Gcc13_14 => support_level_for_tier(tier),
        VersionBand::Unknown => support_level_for_tier(tier),
    }
}

pub fn default_processing_path_for_tier(tier: SupportTier) -> ProcessingPath {
    match tier {
        SupportTier::A => ProcessingPath::DualSinkStructured,
        SupportTier::B => ProcessingPath::NativeTextCapture,
        SupportTier::C => ProcessingPath::Passthrough,
    }
}

fn default_processing_path_for_version_band(
    version_band: VersionBand,
    tier: SupportTier,
) -> ProcessingPath {
    match version_band {
        VersionBand::Gcc9_12 => ProcessingPath::NativeTextCapture,
        VersionBand::Gcc15Plus | VersionBand::Gcc13_14 => {
            default_processing_path_for_tier(tier)
        }
        VersionBand::Unknown => default_processing_path_for_tier(tier),
    }
}

pub fn capability_profile_for_major(major: u32) -> CapabilityProfile {
    capability_profile_for_compatibility(
        version_band_for_major(major),
        support_tier_for_major(major),
        major >= 15,
    )
}

pub fn capability_profile_for_probe(probe: &ProbeResult) -> CapabilityProfile {
    capability_profile_for_compatibility(
        probe.version_band(),
        probe.support_tier,
        probe.add_output_sarif_supported,
    )
}

fn capability_profile_for_compatibility(
    version_band: VersionBand,
    support_tier: SupportTier,
    add_output_sarif_supported: bool,
) -> CapabilityProfile {
    // Keep the new vocabulary behind a probe-local compatibility seam until a
    // later issue installs dedicated path/capability probing.
    let sarif_diagnostics = add_output_sarif_supported;
    let dual_sink = sarif_diagnostics;

    match version_band {
        VersionBand::Gcc15Plus => CapabilityProfile {
            version_band,
            support_level: support_level_for_tier(support_tier),
            native_text_capture: true,
            json_diagnostics: false,
            sarif_diagnostics,
            dual_sink,
            tty_color_control: true,
            caret_control: false,
            parseable_fixits: false,
            locale_stabilization: false,
            default_processing_path: default_processing_path_for_tier(support_tier),
            allowed_processing_paths: BTreeSet::from([
                ProcessingPath::DualSinkStructured,
                ProcessingPath::Passthrough,
            ]),
        },
        VersionBand::Gcc13_14 => CapabilityProfile {
            version_band,
            support_level: support_level_for_tier(support_tier),
            native_text_capture: true,
            json_diagnostics: false,
            sarif_diagnostics,
            dual_sink,
            tty_color_control: false,
            caret_control: false,
            parseable_fixits: false,
            locale_stabilization: false,
            default_processing_path: default_processing_path_for_tier(support_tier),
            allowed_processing_paths: BTreeSet::from([
                ProcessingPath::NativeTextCapture,
                ProcessingPath::SingleSinkStructured,
                ProcessingPath::Passthrough,
            ]),
        },
        VersionBand::Gcc9_12 => CapabilityProfile {
            version_band,
            support_level: SupportLevel::Experimental,
            native_text_capture: true,
            json_diagnostics: false,
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
            support_level: support_level_for_tier(support_tier),
            native_text_capture: true,
            json_diagnostics: false,
            sarif_diagnostics,
            dual_sink,
            tty_color_control: false,
            caret_control: false,
            parseable_fixits: false,
            locale_stabilization: false,
            default_processing_path: default_processing_path_for_tier(support_tier),
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
    fn maps_support_levels_and_default_processing_paths_from_legacy_tiers() {
        assert_eq!(
            support_level_for_tier(SupportTier::A),
            SupportLevel::Primary
        );
        assert_eq!(
            support_level_for_tier(SupportTier::B),
            SupportLevel::Conservative
        );
        assert_eq!(
            support_level_for_tier(SupportTier::C),
            SupportLevel::Unsupported
        );
        assert_eq!(
            default_processing_path_for_tier(SupportTier::A),
            ProcessingPath::DualSinkStructured
        );
        assert_eq!(
            default_processing_path_for_tier(SupportTier::B),
            ProcessingPath::NativeTextCapture
        );
    }

    #[test]
    fn maps_band_specific_support_levels_and_default_processing_paths() {
        assert_eq!(
            support_level_for_version_band(VersionBand::Gcc9_12, SupportTier::C),
            SupportLevel::Experimental
        );
        assert_eq!(
            support_level_for_version_band(VersionBand::Unknown, SupportTier::C),
            SupportLevel::Unsupported
        );
        assert_eq!(
            default_processing_path_for_version_band(VersionBand::Gcc9_12, SupportTier::C),
            ProcessingPath::NativeTextCapture
        );
        assert_eq!(
            default_processing_path_for_version_band(VersionBand::Unknown, SupportTier::C),
            ProcessingPath::Passthrough
        );
    }

    #[test]
    fn builds_capability_profiles_without_mutating_legacy_surface() {
        let gcc15 = capability_profile_for_major(15);
        assert_eq!(gcc15.version_band, VersionBand::Gcc15Plus);
        assert_eq!(gcc15.support_level, SupportLevel::Primary);
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
        assert_eq!(gcc13.support_level, SupportLevel::Conservative);
        assert!(!gcc13.json_diagnostics);
        assert!(!gcc13.sarif_diagnostics);
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
        assert!(gcc12.allowed_processing_paths.contains(&ProcessingPath::Passthrough));

        let gcc8 = capability_profile_for_major(8);
        assert_eq!(gcc8.version_band, VersionBand::Unknown);
        assert_eq!(gcc8.support_level, SupportLevel::Unsupported);
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
        assert_eq!(probe.support_level(), SupportLevel::Primary);
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
    fn parses_version_line() {
        let parsed = parse_version("gcc (Ubuntu 15.1.0-1) 15.1.0").unwrap();
        assert_eq!(parsed, (15, 1));
    }
}
