//! Probes compiler backends for version, capabilities, and support level.

use serde::{Deserialize, Serialize};
use std::collections::{BTreeSet, HashMap};
use std::env;
use std::ffi::{OsStr, OsString};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Versioned policy identifier for supported backend launcher/compiler topologies.
pub const BACKEND_TOPOLOGY_POLICY_VERSION: &str = "v1beta-topology-2026-04-12";

/// Kind of compiler driver being invoked.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DriverKind {
    /// GNU C compiler driver.
    Gcc,
    /// GNU C++ compiler driver.
    Gxx,
}

/// Broad version band grouping for capability mapping.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VersionBand {
    /// GCC 16 and newer, currently outside the in-scope parity contract.
    Gcc16Plus,
    /// GCC 15.
    Gcc15,
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
    /// Backend is inside the GCC 9-15 public parity contract.
    InScope,
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

/// Concrete execution shape used to reach the compiler backend.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BackendTopologyKind {
    /// The wrapper executes the compiler directly.
    Direct,
    /// The wrapper executes one launcher, which then invokes the compiler.
    SingleBackendLauncher,
}

/// Public classification for a known backend topology class.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BackendTopologyDisposition {
    /// The topology is supported in the current beta contract.
    Supported,
    /// The topology is intentionally unsupported in the current beta contract.
    Unsupported,
    /// The topology may be explored later but is not part of the current beta contract.
    NotYetSupported,
}

/// Machine-readable entry in the current backend-topology policy.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BackendTopologyPolicyEntry {
    /// Stable identifier for the topology class.
    pub topology: String,
    /// Current disposition for the topology class.
    pub disposition: BackendTopologyDisposition,
    /// Short user-facing explanation.
    pub note: String,
}

/// Active backend topology for one resolved invocation path.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActiveBackendTopology {
    /// Versioned policy identifier that governs this topology.
    pub policy_version: String,
    /// Kind of topology selected for this invocation.
    pub kind: BackendTopologyKind,
    /// Canonical launcher path when `kind=single_backend_launcher`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub launcher_path: Option<PathBuf>,
    /// Current policy disposition for this active topology.
    pub disposition: BackendTopologyDisposition,
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
    /// Recommended processing path for this resolved capability profile.
    pub default_processing_path: ProcessingPath,
    /// Set of processing paths this resolved capability profile can use.
    pub allowed_processing_paths: BTreeSet<ProcessingPath>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct CapabilityFacts {
    support_level: SupportLevel,
    native_text_capture: bool,
    json_diagnostics: bool,
    sarif_diagnostics: bool,
    dual_sink: bool,
    tty_color_control: bool,
    caret_control: bool,
    parseable_fixits: bool,
    locale_stabilization: bool,
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
    /// Active launcher/compiler topology for this invocation.
    pub execution_topology: ActiveBackendTopology,
    /// Raw version string returned by the backend.
    pub version_string: String,
    /// Parsed major version number.
    pub major: u32,
    /// Parsed minor version number.
    pub minor: u32,
    /// Whether the backend is a C or C++ driver.
    pub driver_kind: DriverKind,
    /// Whether the backend supports `-fdiagnostics-add-output=sarif`.
    pub add_output_sarif_supported: bool,
    /// Filesystem identity key for cache invalidation.
    pub version_probe_key: ProbeKey,
}

impl ProbeResult {
    /// Returns the path that should be executed for child invocations.
    pub fn spawn_path(&self) -> &Path {
        self.execution_topology
            .launcher_path
            .as_deref()
            .unwrap_or(&self.resolved_path)
    }

    /// Returns the compiler-facing argument vector for the spawned child.
    pub fn spawn_args(&self, compiler_args: &[OsString]) -> Vec<OsString> {
        let mut argv = Vec::with_capacity(
            compiler_args.len() + usize::from(self.execution_topology.launcher_path.is_some()),
        );
        if self.execution_topology.launcher_path.is_some() {
            argv.push(self.resolved_path.as_os_str().to_os_string());
        }
        argv.extend(compiler_args.iter().cloned());
        argv
    }

    /// Returns the version band for this probe's major version.
    pub fn version_band(&self) -> VersionBand {
        version_band_for_major(self.major)
    }

    /// Returns the support level for this probe's version band.
    pub fn support_level(&self) -> SupportLevel {
        support_level_for_version_band(self.version_band())
    }

    /// Returns the default processing path resolved from this probe's capability facts.
    pub fn default_processing_path(&self) -> ProcessingPath {
        self.capability_profile().default_processing_path
    }

    /// Builds a full capability profile from this probe result.
    pub fn capability_profile(&self) -> CapabilityProfile {
        capability_profile_for_probe(self)
    }
}

/// Parameters for resolving a backend compiler path.
#[derive(Debug, Clone)]
pub struct ResolveRequest {
    /// Backend path from wrapper-owned CLI flags, if any.
    pub cli_backend: Option<PathBuf>,
    /// Backend path from an environment variable, if any.
    pub env_backend: Option<PathBuf>,
    /// Backend path from config files, if any.
    pub config_backend: Option<PathBuf>,
    /// Launcher path from wrapper-owned CLI flags, if any.
    pub cli_launcher: Option<PathBuf>,
    /// Launcher path from an environment variable, if any.
    pub env_launcher: Option<PathBuf>,
    /// Launcher path from config files, if any.
    pub config_launcher: Option<PathBuf>,
    /// Name the wrapper was invoked as (e.g. `gcc-formed`).
    pub invoked_as: String,
    /// Wrapper binary path for recursion detection, if available.
    pub wrapper_path: Option<PathBuf>,
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
    /// The requested launcher executable could not be found.
    #[error("backend launcher was not found or is not a single executable path: {0}")]
    LauncherNotFound(String),
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
    /// The requested launcher/compiler topology is outside the current beta contract.
    #[error("unsupported backend topology: {0}")]
    UnsupportedTopology(String),
    /// The requested launcher/compiler topology would recurse back into the wrapper.
    #[error("recursive backend topology is not allowed: {0}")]
    RecursiveTopology(String),
}

impl ProbeCache {
    /// Returns a cached probe result or performs a fresh probe.
    pub fn get_or_probe(&mut self, request: ResolveRequest) -> Result<ProbeResult, ProbeError> {
        let resolved = resolve_backend(&request)?;
        let key = probe_key(&resolved.compiler_path)?;
        if let Some(cached) = self.entries.get(&key) {
            let mut cached = cached.clone();
            cached.requested_backend = request.invoked_as;
            cached.execution_topology = resolved.execution_topology;
            return Ok(cached);
        }
        let mut result = probe_backend(&resolved.compiler_path, request.invoked_as)?;
        self.entries.insert(key, result.clone());
        result.execution_topology = resolved.execution_topology;
        Ok(result)
    }
}

/// Resolves the backend compiler path from explicit, env, or PATH lookup.
pub fn resolve_backend(request: &ResolveRequest) -> Result<ResolvedBackend, ProbeError> {
    let compiler_path = if let Some(candidate) = request
        .cli_backend
        .as_ref()
        .or(request.env_backend.as_ref())
        .or(request.config_backend.as_ref())
    {
        canonicalize_candidate(candidate)?
    } else {
        let default = default_backend_name(&request.invoked_as);
        find_in_path(default).ok_or_else(|| ProbeError::NotFound(default.to_string()))?
    };
    let launcher_path = request
        .cli_launcher
        .as_ref()
        .or(request.env_launcher.as_ref())
        .or(request.config_launcher.as_ref())
        .map(|candidate| canonicalize_launcher_candidate(candidate))
        .transpose()?;

    validate_backend_topology(
        &compiler_path,
        launcher_path.as_deref(),
        request.wrapper_path.as_deref(),
    )?;

    Ok(ResolvedBackend {
        compiler_path: compiler_path.clone(),
        execution_topology: ActiveBackendTopology {
            policy_version: BACKEND_TOPOLOGY_POLICY_VERSION.to_string(),
            kind: if launcher_path.is_some() {
                BackendTopologyKind::SingleBackendLauncher
            } else {
                BackendTopologyKind::Direct
            },
            launcher_path,
            disposition: BackendTopologyDisposition::Supported,
        },
    })
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
    if !output.status.success() {
        return Err(ProbeError::VersionProbe {
            path: path.to_path_buf(),
            source: std::io::Error::other(format!("--version exited with {}", output.status)),
        });
    }
    let version_string = String::from_utf8_lossy(&output.stdout)
        .lines()
        .next()
        .unwrap_or_default()
        .trim()
        .to_string();
    let (major, minor) = parse_version(&version_string)?;
    let key = probe_key(path)?;
    let add_output_sarif_supported = probe_add_output_sarif_support(path, major);
    Ok(ProbeResult {
        requested_backend: invoked_as,
        resolved_path: path.to_path_buf(),
        execution_topology: ActiveBackendTopology {
            policy_version: BACKEND_TOPOLOGY_POLICY_VERSION.to_string(),
            kind: BackendTopologyKind::Direct,
            launcher_path: None,
            disposition: BackendTopologyDisposition::Supported,
        },
        version_string,
        major,
        minor,
        driver_kind: driver_kind_for_path(path),
        add_output_sarif_supported,
        version_probe_key: key,
    })
}

fn probe_add_output_sarif_support(path: &Path, major: u32) -> bool {
    let output = match Command::new(path).arg("--help=common").output() {
        Ok(output) => output,
        Err(_) => return major == 15,
    };

    if !output.status.success() {
        return major == 15;
    }

    let help = format!(
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    if help.contains("-fdiagnostics-add-output=") || help.contains("-fdiagnostics-add-output ") {
        return true;
    }

    if help.contains("-fdiagnostics-format=") || help.contains("-fdiagnostics-") {
        return false;
    }

    major == 15
}

/// One row from the current backend-topology support policy.
pub fn backend_topology_policy() -> Vec<BackendTopologyPolicyEntry> {
    vec![
        BackendTopologyPolicyEntry {
            topology: "direct_compiler".to_string(),
            disposition: BackendTopologyDisposition::Supported,
            note: "wrapper executes a concrete GCC/G++ binary directly".to_string(),
        },
        BackendTopologyPolicyEntry {
            topology: "single_backend_launcher".to_string(),
            disposition: BackendTopologyDisposition::Supported,
            note: "wrapper executes one launcher and passes one concrete compiler path as argv[1]".to_string(),
        },
        BackendTopologyPolicyEntry {
            topology: "launcher_before_wrapper".to_string(),
            disposition: BackendTopologyDisposition::Unsupported,
            note: "build-system-managed compiler launchers before gcc-formed are outside the current beta contract".to_string(),
        },
        BackendTopologyPolicyEntry {
            topology: "multi_launcher_chain".to_string(),
            disposition: BackendTopologyDisposition::Unsupported,
            note: "stacked launchers are not supported; configure at most one launcher executable".to_string(),
        },
        BackendTopologyPolicyEntry {
            topology: "shell_command_launcher".to_string(),
            disposition: BackendTopologyDisposition::Unsupported,
            note: "launcher configuration must be one executable path, not a shell string".to_string(),
        },
        BackendTopologyPolicyEntry {
            topology: "launcher_alias_backend".to_string(),
            disposition: BackendTopologyDisposition::Unsupported,
            note: "backend compiler must resolve to a concrete GCC/G++ binary, not a launcher alias directory entry".to_string(),
        },
    ]
}

/// Maps a GCC major version to its version band.
pub fn version_band_for_major(major: u32) -> VersionBand {
    match major {
        16.. => VersionBand::Gcc16Plus,
        15 => VersionBand::Gcc15,
        13 | 14 => VersionBand::Gcc13_14,
        9..=12 => VersionBand::Gcc9_12,
        _ => VersionBand::Unknown,
    }
}

/// Returns the support level for a given version band.
pub fn support_level_for_version_band(version_band: VersionBand) -> SupportLevel {
    match version_band {
        VersionBand::Gcc15 | VersionBand::Gcc13_14 | VersionBand::Gcc9_12 => SupportLevel::InScope,
        VersionBand::Gcc16Plus | VersionBand::Unknown => SupportLevel::PassthroughOnly,
    }
}

/// Returns the default processing path for a given version band.
pub fn default_processing_path_for_version_band(version_band: VersionBand) -> ProcessingPath {
    let representative_capabilities =
        capability_facts_for_version_band(version_band, matches!(version_band, VersionBand::Gcc15));
    default_processing_path_for_capability_facts(representative_capabilities)
}

/// Builds a capability profile from a GCC major version number.
pub fn capability_profile_for_major(major: u32) -> CapabilityProfile {
    let version_band = version_band_for_major(major);
    capability_profile_for_version_band(version_band, matches!(version_band, VersionBand::Gcc15))
}

/// Builds a capability profile from a completed probe result.
pub fn capability_profile_for_probe(probe: &ProbeResult) -> CapabilityProfile {
    capability_profile_for_version_band(probe.version_band(), probe.add_output_sarif_supported)
}

fn capability_profile_for_version_band(
    version_band: VersionBand,
    add_output_sarif_supported: bool,
) -> CapabilityProfile {
    let capabilities = capability_facts_for_version_band(version_band, add_output_sarif_supported);

    CapabilityProfile {
        version_band,
        support_level: capabilities.support_level,
        native_text_capture: capabilities.native_text_capture,
        json_diagnostics: capabilities.json_diagnostics,
        sarif_diagnostics: capabilities.sarif_diagnostics,
        dual_sink: capabilities.dual_sink,
        tty_color_control: capabilities.tty_color_control,
        caret_control: capabilities.caret_control,
        parseable_fixits: capabilities.parseable_fixits,
        locale_stabilization: capabilities.locale_stabilization,
        default_processing_path: default_processing_path_for_capability_facts(capabilities),
        allowed_processing_paths: allowed_processing_paths_for_capability_facts(capabilities),
    }
}

fn capability_facts_for_version_band(
    version_band: VersionBand,
    add_output_sarif_supported: bool,
) -> CapabilityFacts {
    match version_band {
        VersionBand::Gcc15 => CapabilityFacts {
            support_level: support_level_for_version_band(version_band),
            native_text_capture: true,
            json_diagnostics: false,
            sarif_diagnostics: true,
            dual_sink: add_output_sarif_supported,
            tty_color_control: true,
            caret_control: false,
            parseable_fixits: false,
            locale_stabilization: false,
        },
        VersionBand::Gcc13_14 => CapabilityFacts {
            support_level: support_level_for_version_band(version_band),
            native_text_capture: true,
            json_diagnostics: false,
            sarif_diagnostics: true,
            dual_sink: false,
            tty_color_control: true,
            caret_control: false,
            parseable_fixits: false,
            locale_stabilization: false,
        },
        VersionBand::Gcc9_12 => CapabilityFacts {
            support_level: support_level_for_version_band(version_band),
            native_text_capture: true,
            json_diagnostics: true,
            sarif_diagnostics: false,
            dual_sink: false,
            tty_color_control: true,
            caret_control: false,
            parseable_fixits: false,
            locale_stabilization: false,
        },
        VersionBand::Gcc16Plus | VersionBand::Unknown => CapabilityFacts {
            support_level: support_level_for_version_band(version_band),
            native_text_capture: false,
            json_diagnostics: false,
            sarif_diagnostics: false,
            dual_sink: false,
            tty_color_control: false,
            caret_control: false,
            parseable_fixits: false,
            locale_stabilization: false,
        },
    }
}

fn default_processing_path_for_capability_facts(capabilities: CapabilityFacts) -> ProcessingPath {
    if matches!(capabilities.support_level, SupportLevel::PassthroughOnly) {
        return ProcessingPath::Passthrough;
    }

    if capabilities.dual_sink {
        return ProcessingPath::DualSinkStructured;
    }

    if capabilities.native_text_capture {
        return ProcessingPath::NativeTextCapture;
    }

    if capabilities.sarif_diagnostics || capabilities.json_diagnostics {
        return ProcessingPath::SingleSinkStructured;
    }

    ProcessingPath::Passthrough
}

fn allowed_processing_paths_for_capability_facts(
    capabilities: CapabilityFacts,
) -> BTreeSet<ProcessingPath> {
    if matches!(capabilities.support_level, SupportLevel::PassthroughOnly) {
        return BTreeSet::from([ProcessingPath::Passthrough]);
    }

    let mut paths = BTreeSet::new();

    if capabilities.dual_sink {
        // Prefer the safest same-run structured path when dual-sink is available.
        paths.insert(ProcessingPath::DualSinkStructured);
    } else {
        if capabilities.native_text_capture {
            paths.insert(ProcessingPath::NativeTextCapture);
        }
        if capabilities.sarif_diagnostics || capabilities.json_diagnostics {
            paths.insert(ProcessingPath::SingleSinkStructured);
        }
    }

    paths.insert(ProcessingPath::Passthrough);
    paths
}

fn parse_version(line: &str) -> Result<(u32, u32), ProbeError> {
    let stripped = strip_parenthesized_segments(line);
    for token in stripped.split_whitespace() {
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

fn strip_parenthesized_segments(line: &str) -> String {
    let mut stripped = String::with_capacity(line.len());
    let mut depth = 0u32;

    for ch in line.chars() {
        match ch {
            '(' => {
                depth = depth.saturating_add(1);
                stripped.push(' ');
            }
            ')' if depth > 0 => {
                depth -= 1;
                stripped.push(' ');
            }
            _ if depth == 0 => stripped.push(ch),
            _ => stripped.push(' '),
        }
    }

    stripped
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
    find_in_path_with_path(binary, &path)
}

fn find_in_path_with_path(binary: &str, path: &OsStr) -> Option<PathBuf> {
    env::split_paths(path)
        .map(|dir| dir.join(binary))
        .filter(|candidate| is_runnable_candidate(candidate))
        .find_map(|candidate| canonicalize_candidate(&candidate).ok())
}

fn is_runnable_candidate(path: &Path) -> bool {
    let Ok(metadata) = fs::metadata(path) else {
        return false;
    };
    if !metadata.is_file() {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        metadata.permissions().mode() & 0o111 != 0
    }
    #[cfg(not(unix))]
    {
        true
    }
}

fn canonicalize_candidate(path: &Path) -> Result<PathBuf, ProbeError> {
    fs::canonicalize(path).map_err(|_| ProbeError::NotFound(path.display().to_string()))
}

fn canonicalize_launcher_candidate(path: &Path) -> Result<PathBuf, ProbeError> {
    fs::canonicalize(path).map_err(|_| ProbeError::LauncherNotFound(path.display().to_string()))
}

fn validate_backend_topology(
    compiler_path: &Path,
    launcher_path: Option<&Path>,
    wrapper_path: Option<&Path>,
) -> Result<(), ProbeError> {
    let wrapper_path = wrapper_path
        .map(fs::canonicalize)
        .transpose()
        .map_err(|_| {
            ProbeError::RecursiveTopology("failed to canonicalize wrapper path".to_string())
        })?;
    if let Some(wrapper_path) = wrapper_path.as_deref() {
        if compiler_path == wrapper_path {
            return Err(ProbeError::RecursiveTopology(
                "backend compiler resolves to the wrapper binary".to_string(),
            ));
        }
        if launcher_path.is_some_and(|launcher| launcher == wrapper_path) {
            return Err(ProbeError::RecursiveTopology(
                "backend launcher resolves to the wrapper binary".to_string(),
            ));
        }
    }
    if launcher_path.is_some_and(|launcher| launcher == compiler_path) {
        return Err(ProbeError::UnsupportedTopology(
            "launcher and backend compiler resolve to the same executable".to_string(),
        ));
    }
    Ok(())
}

/// Resolved launcher/compiler paths for one invocation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedBackend {
    /// Canonical path to the resolved compiler binary.
    pub compiler_path: PathBuf,
    /// Active launcher/compiler topology for this invocation.
    pub execution_topology: ActiveBackendTopology,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::Path;

    fn direct_topology() -> ActiveBackendTopology {
        ActiveBackendTopology {
            policy_version: BACKEND_TOPOLOGY_POLICY_VERSION.to_string(),
            kind: BackendTopologyKind::Direct,
            launcher_path: None,
            disposition: BackendTopologyDisposition::Supported,
        }
    }

    #[test]
    fn picks_cplusplus_driver_from_invocation_name() {
        assert_eq!(default_backend_name("g++-formed"), "g++");
        assert_eq!(default_backend_name("gcc-formed"), "gcc");
    }

    #[test]
    fn maps_version_bands() {
        assert_eq!(version_band_for_major(16), VersionBand::Gcc16Plus);
        assert_eq!(version_band_for_major(15), VersionBand::Gcc15);
        assert_eq!(version_band_for_major(13), VersionBand::Gcc13_14);
        assert_eq!(version_band_for_major(9), VersionBand::Gcc9_12);
        assert_eq!(version_band_for_major(8), VersionBand::Unknown);
    }

    #[test]
    fn maps_support_levels_and_default_processing_paths_from_version_bands() {
        assert_eq!(
            support_level_for_version_band(VersionBand::Gcc15),
            SupportLevel::InScope
        );
        assert_eq!(
            support_level_for_version_band(VersionBand::Gcc13_14),
            SupportLevel::InScope
        );
        assert_eq!(
            support_level_for_version_band(VersionBand::Gcc16Plus),
            SupportLevel::PassthroughOnly
        );
        assert_eq!(
            support_level_for_version_band(VersionBand::Unknown),
            SupportLevel::PassthroughOnly
        );
        assert_eq!(
            default_processing_path_for_version_band(VersionBand::Gcc15),
            ProcessingPath::DualSinkStructured
        );
        assert_eq!(
            default_processing_path_for_version_band(VersionBand::Gcc13_14),
            ProcessingPath::NativeTextCapture
        );
        assert_eq!(
            default_processing_path_for_version_band(VersionBand::Gcc16Plus),
            ProcessingPath::Passthrough
        );
    }

    #[test]
    fn maps_band_specific_support_levels_and_default_processing_paths() {
        assert_eq!(
            support_level_for_version_band(VersionBand::Gcc9_12),
            SupportLevel::InScope
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
        assert_eq!(gcc15.version_band, VersionBand::Gcc15);
        assert_eq!(gcc15.support_level, SupportLevel::InScope);
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
        assert_eq!(gcc13.support_level, SupportLevel::InScope);
        assert!(!gcc13.json_diagnostics);
        assert!(gcc13.sarif_diagnostics);
        assert!(!gcc13.dual_sink);
        assert!(gcc13.tty_color_control);
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
        assert_eq!(gcc12.support_level, SupportLevel::InScope);
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
            execution_topology: direct_topology(),
            version_string: "gcc (GCC) 15.1.0".to_string(),
            major: 15,
            minor: 1,
            driver_kind: DriverKind::Gcc,
            add_output_sarif_supported: true,
            version_probe_key: ProbeKey {
                realpath: PathBuf::from("/usr/bin/gcc"),
                inode: 1,
                mtime_seconds: 0,
                size_bytes: 1,
            },
        };

        assert_eq!(probe.version_band(), VersionBand::Gcc15);
        assert_eq!(probe.support_level(), SupportLevel::InScope);
        assert_eq!(
            probe.default_processing_path(),
            ProcessingPath::DualSinkStructured
        );
        assert!(probe.capability_profile().dual_sink);
        assert!(probe.add_output_sarif_supported);
    }

    #[test]
    fn probe_result_default_path_follows_resolved_capability_facts() {
        let probe = ProbeResult {
            requested_backend: "gcc-formed".to_string(),
            resolved_path: PathBuf::from("/usr/bin/gcc-15"),
            execution_topology: direct_topology(),
            version_string: "gcc (GCC) 15.1.0".to_string(),
            major: 15,
            minor: 1,
            driver_kind: DriverKind::Gcc,
            add_output_sarif_supported: false,
            version_probe_key: ProbeKey {
                realpath: PathBuf::from("/usr/bin/gcc-15"),
                inode: 4,
                mtime_seconds: 0,
                size_bytes: 4,
            },
        };

        let profile = probe.capability_profile();
        assert_eq!(probe.version_band(), VersionBand::Gcc15);
        assert_eq!(
            probe.default_processing_path(),
            ProcessingPath::NativeTextCapture
        );
        assert_eq!(
            profile.default_processing_path,
            ProcessingPath::NativeTextCapture
        );
        assert_eq!(
            profile.allowed_processing_paths,
            BTreeSet::from([
                ProcessingPath::NativeTextCapture,
                ProcessingPath::SingleSinkStructured,
                ProcessingPath::Passthrough,
            ])
        );
        assert!(!profile.dual_sink);
        assert!(profile.sarif_diagnostics);
    }

    #[test]
    fn probe_result_accessors_reflect_band_c_contract() {
        let probe = ProbeResult {
            requested_backend: "gcc-formed".to_string(),
            resolved_path: PathBuf::from("/usr/bin/gcc-12"),
            execution_topology: direct_topology(),
            version_string: "gcc (GCC) 12.3.0".to_string(),
            major: 12,
            minor: 3,
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
        assert_eq!(probe.support_level(), SupportLevel::InScope);
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
            execution_topology: direct_topology(),
            version_string: "gcc (GCC) 13.3.0".to_string(),
            major: 13,
            minor: 3,
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

    #[test]
    fn parses_version_from_unexpected_but_supported_formats() {
        let arm = parse_version("arm-none-eabi-gcc (Arm GNU Toolchain) 13.2.1 20231009").unwrap();
        let cplusplus = parse_version("g++ (GCC) 14.2.0 20250215 (Red Hat 14.2.0-3)").unwrap();

        assert_eq!(arm, (13, 2));
        assert_eq!(cplusplus, (14, 2));
    }

    #[test]
    fn ignores_parenthesized_vendor_version_tokens() {
        let parsed = parse_version("gcc (Custom bundle 99.1 build) 13.2.0").unwrap();

        assert_eq!(parsed, (13, 2));
    }

    #[test]
    fn rejects_empty_partial_and_malformed_version_lines() {
        for line in ["", "gcc", "gcc version 15", "gcc version fifteen.point.two"] {
            assert!(
                matches!(parse_version(line), Err(ProbeError::UnparseableVersion(value)) if value == line),
                "expected unparseable version for {line:?}"
            );
        }
    }

    #[cfg(unix)]
    #[test]
    fn probe_backend_rejects_non_zero_version_exit_even_with_parseable_stdout() {
        let temp = tempfile::tempdir().unwrap();
        let backend = temp.path().join("fake-gcc");
        fs::write(
            &backend,
            r#"#!/bin/sh
if [ "${1:-}" = "--version" ]; then
  printf '%s\n' 'gcc (Fake) 15.2.0'
  exit 1
fi
exit 0
"#,
        )
        .unwrap();
        make_executable(&backend);

        let error = probe_backend(&backend, "gcc-formed".to_string()).unwrap_err();

        match error {
            ProbeError::VersionProbe { path, source } => {
                assert_eq!(path, backend);
                assert!(source.to_string().contains("exit"));
            }
            other => panic!("expected version probe failure, got {other:?}"),
        }
    }

    #[cfg(unix)]
    #[test]
    fn probe_backend_uses_help_output_to_confirm_dual_sink_support() {
        let temp = tempfile::tempdir().unwrap();
        let backend = temp.path().join("fake-gcc");
        fs::write(
            &backend,
            r#"#!/bin/sh
if [ "${1:-}" = "--version" ]; then
  printf '%s\n' 'gcc (Fake) 15.2.0'
  exit 0
fi
if [ "${1:-}" = "--help=common" ]; then
  printf '%s\n' '  -fdiagnostics-add-output=[sarif:version=2.1,file=PATH]'
  exit 0
fi
exit 0
"#,
        )
        .unwrap();
        make_executable(&backend);

        let probe = probe_backend(&backend, "gcc-formed".to_string()).unwrap();

        assert!(probe.add_output_sarif_supported);
        assert_eq!(
            probe.default_processing_path(),
            ProcessingPath::DualSinkStructured
        );
    }

    #[cfg(unix)]
    #[test]
    fn probe_backend_can_override_gcc15_dual_sink_assumption_from_help_output() {
        let temp = tempfile::tempdir().unwrap();
        let backend = temp.path().join("fake-gcc");
        fs::write(
            &backend,
            r#"#!/bin/sh
if [ "${1:-}" = "--version" ]; then
  printf '%s\n' 'gcc (Fake) 15.2.0'
  exit 0
fi
if [ "${1:-}" = "--help=common" ]; then
  printf '%s\n' '  -fdiagnostics-format=[text|sarif-file|json-file]'
  exit 0
fi
exit 0
"#,
        )
        .unwrap();
        make_executable(&backend);

        let probe = probe_backend(&backend, "gcc-formed".to_string()).unwrap();

        assert!(!probe.add_output_sarif_supported);
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

    #[cfg(unix)]
    #[test]
    fn resolve_backend_rejects_symlink_loop_for_launcher_topology_cycles() {
        use std::os::unix::fs::symlink;

        let temp = tempfile::tempdir().unwrap();
        let launcher = temp.path().join("gcc-launcher");
        let loopback = temp.path().join("gcc-launcher.loop");
        let compiler = temp.path().join("real-gcc");

        fs::write(&compiler, "#!/bin/sh\nexit 0\n").unwrap();
        make_executable(&compiler);
        symlink(&loopback, &launcher).unwrap();
        symlink(&launcher, &loopback).unwrap();

        let request = ResolveRequest {
            cli_backend: Some(compiler),
            env_backend: None,
            config_backend: None,
            cli_launcher: Some(launcher.clone()),
            env_launcher: None,
            config_launcher: None,
            invoked_as: "gcc-formed".to_string(),
            wrapper_path: None,
        };

        let error = resolve_backend(&request).unwrap_err();
        match error {
            ProbeError::LauncherNotFound(path) => {
                assert!(path.contains("gcc-launcher"));
            }
            other => panic!("expected loop rejection, got {other:?}"),
        }
    }

    #[test]
    fn find_in_path_skips_directory_candidates() {
        let temp = tempfile::tempdir().unwrap();
        let blocked_dir = temp.path().join("blocked");
        let valid_dir = temp.path().join("valid");
        fs::create_dir_all(blocked_dir.join("gcc")).unwrap();
        fs::create_dir_all(&valid_dir).unwrap();
        let backend = valid_dir.join("gcc");
        fs::write(&backend, "").unwrap();
        make_executable(&backend);

        let path = env::join_paths([blocked_dir.as_path(), valid_dir.as_path()]).unwrap();

        assert_eq!(
            find_in_path_with_path("gcc", &path),
            Some(backend.canonicalize().unwrap())
        );
    }

    #[cfg(unix)]
    #[test]
    fn find_in_path_skips_non_executable_file_candidates() {
        use std::os::unix::fs::PermissionsExt;

        let temp = tempfile::tempdir().unwrap();
        let blocked_dir = temp.path().join("blocked");
        let valid_dir = temp.path().join("valid");
        fs::create_dir_all(&blocked_dir).unwrap();
        fs::create_dir_all(&valid_dir).unwrap();
        let blocked = blocked_dir.join("gcc");
        let backend = valid_dir.join("gcc");
        fs::write(&blocked, "#!/bin/sh\n").unwrap();
        fs::write(&backend, "#!/bin/sh\n").unwrap();
        fs::set_permissions(&blocked, fs::Permissions::from_mode(0o644)).unwrap();
        make_executable(&backend);

        let path = env::join_paths([blocked_dir.as_path(), valid_dir.as_path()]).unwrap();

        assert_eq!(
            find_in_path_with_path("gcc", &path),
            Some(backend.canonicalize().unwrap())
        );
    }

    #[test]
    fn maps_version_band_boundaries_for_all_supported_cutoffs() {
        let cases = [
            (8, VersionBand::Unknown),
            (9, VersionBand::Gcc9_12),
            (12, VersionBand::Gcc9_12),
            (13, VersionBand::Gcc13_14),
            (14, VersionBand::Gcc13_14),
            (15, VersionBand::Gcc15),
            (16, VersionBand::Gcc16Plus),
        ];

        for (major, expected_band) in cases {
            assert_eq!(version_band_for_major(major), expected_band);
        }
    }

    #[test]
    fn capability_profiles_follow_band_specific_contracts() {
        let gcc9 = capability_profile_for_major(9);
        assert_eq!(gcc9.version_band, VersionBand::Gcc9_12);
        assert_eq!(gcc9.support_level, SupportLevel::InScope);
        assert!(gcc9.native_text_capture);
        assert!(gcc9.json_diagnostics);
        assert!(!gcc9.sarif_diagnostics);
        assert!(gcc9.tty_color_control);
        assert_eq!(
            gcc9.allowed_processing_paths,
            BTreeSet::from([
                ProcessingPath::NativeTextCapture,
                ProcessingPath::SingleSinkStructured,
                ProcessingPath::Passthrough,
            ])
        );

        let gcc14 = capability_profile_for_major(14);
        assert_eq!(gcc14.version_band, VersionBand::Gcc13_14);
        assert_eq!(gcc14.support_level, SupportLevel::InScope);
        assert!(gcc14.native_text_capture);
        assert!(!gcc14.json_diagnostics);
        assert!(gcc14.sarif_diagnostics);
        assert!(!gcc14.dual_sink);
        assert!(gcc14.tty_color_control);
        assert_eq!(
            gcc14.allowed_processing_paths,
            BTreeSet::from([
                ProcessingPath::NativeTextCapture,
                ProcessingPath::SingleSinkStructured,
                ProcessingPath::Passthrough,
            ])
        );

        let gcc16 = capability_profile_for_major(16);
        assert_eq!(gcc16.version_band, VersionBand::Gcc16Plus);
        assert_eq!(gcc16.support_level, SupportLevel::PassthroughOnly);
        assert!(!gcc16.sarif_diagnostics);
        assert!(!gcc16.dual_sink);
        assert_eq!(
            gcc16.allowed_processing_paths,
            BTreeSet::from([ProcessingPath::Passthrough])
        );

        let unknown = capability_profile_for_major(7);
        assert_eq!(unknown.version_band, VersionBand::Unknown);
        assert_eq!(unknown.support_level, SupportLevel::PassthroughOnly);
        assert_eq!(
            unknown.allowed_processing_paths,
            BTreeSet::from([ProcessingPath::Passthrough])
        );
    }

    #[cfg(unix)]
    fn make_executable(path: &Path) {
        use std::os::unix::fs::PermissionsExt;

        let mut permissions = fs::metadata(path).unwrap().permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(path, permissions).unwrap();
    }

    #[cfg(not(unix))]
    fn make_executable(_path: &Path) {}
}
