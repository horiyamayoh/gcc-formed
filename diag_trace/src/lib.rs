//! Trace envelope generation, build manifests, and secure file operations for
//! diagnostic artifacts.

use diag_core::{
    ADAPTER_SPEC_VERSION, DiagnosticDocument, FallbackReason, GroupCascadeRole,
    RENDERER_SPEC_VERSION, VisibilityFloor,
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// Default product name embedded in build manifests.
pub const DEFAULT_PRODUCT_NAME: &str = "gcc-formed";
/// Default maturity label for the current release cycle.
pub const DEFAULT_MATURITY_LABEL: &str = "v1beta";

/// Policy controlling when trace artifacts are retained on disk.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RetentionPolicy {
    /// Never retain trace artifacts.
    Never,
    /// Retain only when the wrapper itself fails.
    OnWrapperFailure,
    /// Retain when the wrapper or the child process fails.
    OnChildError,
    /// Always retain trace artifacts.
    Always,
}

/// Resolved filesystem paths used by the wrapper at runtime.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WrapperPaths {
    /// Path to the configuration file.
    pub config_path: PathBuf,
    /// Root directory for cached data.
    pub cache_root: PathBuf,
    /// Root directory for persistent state.
    pub state_root: PathBuf,
    /// Root directory for runtime-only files.
    pub runtime_root: PathBuf,
    /// Root directory for trace output.
    pub trace_root: PathBuf,
    /// Root directory for installed artifacts.
    pub install_root: PathBuf,
}

/// Build manifest describing the product binary and its build environment.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuildManifest {
    /// Product name (e.g. "gcc-formed").
    pub product_name: String,
    /// Semantic version of the product.
    pub product_version: String,
    /// Target triple the artifact was compiled for.
    pub artifact_target_triple: String,
    /// Operating system component of the target.
    pub artifact_os: String,
    /// CPU architecture component of the target.
    pub artifact_arch: String,
    /// C library family (e.g. "gnu", "musl").
    pub artifact_libc_family: String,
    /// Git commit hash at build time.
    pub git_commit: String,
    /// Cargo build profile (e.g. "release").
    pub build_profile: String,
    /// Rust compiler version used.
    pub rustc_version: String,
    /// Cargo version used.
    pub cargo_version: String,
    /// Timestamp when the build was produced.
    pub build_timestamp: String,
    /// SHA-256 hash of the lockfile.
    pub lockfile_hash: String,
    /// SHA-256 hash of the vendored dependencies.
    pub vendor_hash: String,
    /// IR specification version.
    pub ir_spec_version: String,
    /// Adapter specification version.
    pub adapter_spec_version: String,
    /// Renderer specification version.
    pub renderer_spec_version: String,
    /// Maturity label for this build (e.g. "v1beta").
    #[serde(alias = "support_tier_declaration")]
    pub maturity_label: String,
    /// Release channel (e.g. "stable", "dev").
    pub release_channel: String,
    /// Checksums for the packaged artifacts.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub checksums: Vec<ChecksumEntry>,
}

/// A single file checksum entry within a build manifest.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ChecksumEntry {
    /// Relative path of the artifact.
    pub path: String,
    /// SHA-256 hex digest.
    pub sha256: String,
    /// File size in bytes.
    pub size_bytes: u64,
}

/// Decomposed description of a Rust-style target triple.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TargetDescriptor {
    /// Full target triple string.
    pub target_triple: String,
    /// Operating system (e.g. "linux", "macos").
    pub os: String,
    /// CPU architecture (e.g. "`x86_64`", "`aarch64`").
    pub arch: String,
    /// C library family (e.g. "gnu", "musl").
    pub libc_family: String,
}

/// Top-level trace envelope written for each wrapper invocation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraceEnvelope {
    /// Unique identifier for this trace.
    pub trace_id: String,
    /// Processing mode that was selected (e.g. "render", "passthrough").
    pub selected_mode: String,
    /// Render profile that was selected (e.g. "default", "ci").
    pub selected_profile: String,
    /// Overall verdict from the wrapper.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wrapper_verdict: Option<String>,
    /// Version information summary.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version_summary: Option<TraceVersionSummary>,
    /// Environment discovery summary.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub environment_summary: Option<TraceEnvironmentSummary>,
    /// Terminal capability detection results.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub capabilities: Option<TraceCapabilities>,
    /// Timing measurements for the wrapper run.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timing: Option<TraceTiming>,
    /// Child process exit information.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub child_exit: Option<TraceChildExit>,
    /// Summary of parser results.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parser_result_summary: Option<TraceParserResultSummary>,
    /// Fingerprint summary for the diagnostic document.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fingerprint_summary: Option<TraceFingerprintSummary>,
    /// Redaction status of the trace.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub redaction_status: Option<TraceRedactionStatus>,
    /// Ordered log of internal decision points.
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub decision_log: Vec<String>,
    /// Suppressed-group explainability copied into the trace for review/debug.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cascade_explainability: Option<TraceCascadeExplainability>,
    /// Reason the wrapper fell back to passthrough, if applicable.
    pub fallback_reason: Option<FallbackReason>,
    /// Non-fatal warning messages emitted during the run.
    pub warning_messages: Vec<String>,
    /// References to artifacts produced by this trace.
    pub artifacts: Vec<TraceArtifactRef>,
}

/// Version information recorded in a trace envelope.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraceVersionSummary {
    /// Wrapper binary version.
    pub wrapper_version: String,
    /// Target triple the wrapper was built for.
    pub build_target_triple: String,
    /// IR specification version.
    pub ir_spec_version: String,
    /// Adapter specification version.
    pub adapter_spec_version: String,
    /// Renderer specification version.
    pub renderer_spec_version: String,
}

/// Environment discovery summary recorded in a trace envelope.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraceEnvironmentSummary {
    /// Path to the compiler backend binary.
    pub backend_path: PathBuf,
    /// Path to the configured launcher binary, if any.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub backend_launcher_path: Option<PathBuf>,
    /// Version string reported by the backend.
    pub backend_version: String,
    /// Detected GCC version band.
    pub version_band: String,
    /// Selected processing path.
    pub processing_path: String,
    /// Computed support level.
    pub support_level: String,
    /// Active backend topology kind.
    pub backend_topology_kind: String,
    /// Versioned topology policy identifier.
    pub backend_topology_policy_version: String,
    /// Extra flags injected by the wrapper.
    #[serde(default)]
    pub injected_flags: Vec<String>,
    /// Environment variable keys that were sanitized.
    #[serde(default)]
    pub sanitized_env_keys: Vec<String>,
    /// Paths to temporary artifacts created during the run.
    #[serde(default)]
    pub temp_artifact_paths: Vec<PathBuf>,
}

/// Terminal capability detection results.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraceCapabilities {
    /// Stream kind (e.g. "tty", "pipe").
    pub stream_kind: String,
    /// Terminal width in columns, if known.
    pub width_columns: Option<usize>,
    /// Whether ANSI color output is supported.
    pub ansi_color: bool,
    /// Whether Unicode output is supported.
    pub unicode: bool,
    /// Whether terminal hyperlinks are supported.
    pub hyperlinks: bool,
    /// Whether the stream is interactive.
    pub interactive: bool,
}

/// Timing measurements for a wrapper invocation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraceTiming {
    /// Time spent capturing compiler output, in milliseconds.
    pub capture_ms: u64,
    /// Time spent rendering diagnostics, in milliseconds.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub render_ms: Option<u64>,
    /// Total wall-clock time, in milliseconds.
    pub total_ms: u64,
}

/// Exit status of the child compiler process.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraceChildExit {
    /// Exit code, if the process exited normally.
    pub code: Option<i32>,
    /// Signal number, if the process was terminated by a signal.
    pub signal: Option<i32>,
    /// Whether the child exited successfully.
    pub success: bool,
}

/// Summary of the parser's output included in the trace.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraceParserResultSummary {
    /// Parser status (e.g. "ok", "error").
    pub status: String,
    /// Document completeness level reported by the parser.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub document_completeness: Option<String>,
    /// Number of diagnostic nodes produced.
    pub diagnostic_count: usize,
    /// Number of integrity issues found.
    pub integrity_issue_count: usize,
    /// Number of capture artifacts collected.
    pub capture_count: usize,
}

/// Fingerprint summary for deduplication and drift detection.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraceFingerprintSummary {
    /// Raw fingerprint hash.
    pub raw: String,
    /// Normalized fingerprint hash, if computed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub normalized: Option<String>,
    /// Family-level fingerprint hash, if computed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub family: Option<String>,
}

/// Redaction status of trace artifacts.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraceRedactionStatus {
    /// Redaction class applied.
    pub class: String,
    /// Whether the trace is restricted to local storage.
    pub local_only: bool,
    /// List of artifact IDs that were normalized for redaction.
    #[serde(default)]
    pub normalized_artifacts: Vec<String>,
}

/// Reference to an artifact produced during a trace.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraceArtifactRef {
    /// Artifact identifier.
    pub id: String,
    /// Filesystem path where the artifact was written.
    pub path: Option<PathBuf>,
}

/// Trace-visible explainability for cascade-suppressed groups.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TraceCascadeExplainability {
    /// Retained normalized analysis artifact, when it exists.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub analysis_artifact_id: Option<String>,
    /// Suppressed groups that should remain explainable in trace review.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub suppressed_groups: Vec<TraceSuppressedGroupExplainability>,
}

/// Trace-visible explainability for one suppressed cascade group.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TraceSuppressedGroupExplainability {
    /// Group reference used by cascade analysis.
    pub group_ref: String,
    /// Episode reference when present.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub episode_ref: Option<String>,
    /// Cascade role assigned by document analysis.
    pub role: GroupCascadeRole,
    /// Visibility floor assigned by document analysis.
    pub visibility_floor: VisibilityFloor,
    /// Best parent, when one exists.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub best_parent_group_ref: Option<String>,
    /// Evidence tags supporting the suppression decision.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub evidence_tags: Vec<String>,
    /// Capture refs that let reviewers reach raw provenance.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub provenance_capture_refs: Vec<String>,
}

/// Errors that can occur when writing traces or manifests.
#[derive(Debug, thiserror::Error)]
pub enum TraceError {
    /// An I/O error occurred.
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    /// A JSON serialization error occurred.
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),
}

impl WrapperPaths {
    /// Discovers wrapper paths from the current environment using XDG conventions.
    pub fn discover() -> Self {
        Self::from_env(
            |key| env::var_os(key),
            home_from_env(env::var_os("HOME")),
            env::temp_dir(),
            build_target_triple(),
        )
    }

    fn from_env<F>(get_var: F, home: PathBuf, temp_dir: PathBuf, target_triple: &str) -> Self
    where
        F: Fn(&str) -> Option<OsString>,
    {
        let env_path = |key: &str| {
            get_var(key)
                .filter(|value| !value.is_empty())
                .map(PathBuf::from)
        };

        let config_home = env_path("XDG_CONFIG_HOME").unwrap_or_else(|| home.join(".config"));
        let cache_home = env_path("XDG_CACHE_HOME").unwrap_or_else(|| home.join(".cache"));
        let state_home =
            env_path("XDG_STATE_HOME").unwrap_or_else(|| home.join(".local").join("state"));
        let runtime_home =
            env_path("XDG_RUNTIME_DIR").unwrap_or_else(|| temp_dir.join("cc-formed-runtime"));

        let config_path = env_path("FORMED_CONFIG_FILE")
            .or_else(|| env_path("FORMED_CONFIG_DIR").map(|dir| dir.join("config.toml")))
            .unwrap_or_else(|| config_home.join("cc-formed").join("config.toml"));
        let cache_root =
            env_path("FORMED_CACHE_DIR").unwrap_or_else(|| cache_home.join("cc-formed"));
        let state_root =
            env_path("FORMED_STATE_DIR").unwrap_or_else(|| state_home.join("cc-formed"));
        let runtime_root =
            env_path("FORMED_RUNTIME_DIR").unwrap_or_else(|| runtime_home.join("cc-formed"));
        let trace_root = env_path("FORMED_TRACE_DIR").unwrap_or_else(|| state_root.join("traces"));
        let install_root = env_path("FORMED_INSTALL_ROOT").unwrap_or_else(|| {
            home.join(".local")
                .join("opt")
                .join("cc-formed")
                .join(target_triple)
        });

        Self {
            config_path,
            cache_root,
            state_root,
            runtime_root,
            trace_root,
            install_root,
        }
    }

    /// Creates all required directories and secures private ones.
    pub fn ensure_dirs(&self) -> Result<(), std::io::Error> {
        for dir in [
            &self.cache_root,
            &self.state_root,
            &self.runtime_root,
            &self.trace_root,
        ] {
            fs::create_dir_all(dir)?;
        }
        secure_private_dir(&self.state_root)?;
        secure_private_dir(&self.runtime_root)?;
        secure_private_dir(&self.trace_root)?;
        Ok(())
    }
}

fn home_from_env(home: Option<OsString>) -> PathBuf {
    home.filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
}

/// Returns the compile-time target triple, or `"unknown-target"` if not set.
pub fn build_target_triple() -> &'static str {
    option_env!("FORMED_TARGET").unwrap_or("unknown-target")
}

/// Sets directory permissions to owner-only (0700) on Unix.
#[cfg(unix)]
pub fn secure_private_dir(path: &Path) -> Result<(), std::io::Error> {
    use std::os::unix::fs::PermissionsExt;

    let metadata = fs::metadata(path)?;
    let mut permissions = metadata.permissions();
    permissions.set_mode(0o700);
    fs::set_permissions(path, permissions)
}

/// No-op on non-Unix platforms.
#[cfg(not(unix))]
pub fn secure_private_dir(_path: &Path) -> Result<(), std::io::Error> {
    Ok(())
}

/// Sets file permissions to owner-only (0600) on Unix.
#[cfg(unix)]
pub fn secure_private_file(path: &Path) -> Result<(), std::io::Error> {
    use std::os::unix::fs::PermissionsExt;

    let metadata = fs::metadata(path)?;
    let mut permissions = metadata.permissions();
    permissions.set_mode(0o600);
    fs::set_permissions(path, permissions)
}

/// No-op on non-Unix platforms.
#[cfg(not(unix))]
pub fn secure_private_file(_path: &Path) -> Result<(), std::io::Error> {
    Ok(())
}

/// Generates a unique trace identifier based on the current system time.
pub fn trace_id() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default();
    format!("trace-{nanos}")
}

/// Returns `true` if the given retention policy requires keeping the trace.
pub fn should_retain(policy: RetentionPolicy, wrapper_failed: bool, child_failed: bool) -> bool {
    match policy {
        RetentionPolicy::Never => false,
        RetentionPolicy::OnWrapperFailure => wrapper_failed,
        RetentionPolicy::OnChildError => wrapper_failed || child_failed,
        RetentionPolicy::Always => true,
    }
}

/// Writes a trace envelope to the configured trace root, returning the output path.
pub fn write_trace(
    paths: &WrapperPaths,
    trace: &TraceEnvelope,
    trace_name: &str,
) -> Result<PathBuf, TraceError> {
    paths.ensure_dirs()?;
    let path = paths.trace_root.join(trace_name);
    write_trace_at(&path, trace)?;
    Ok(path)
}

/// Writes a trace envelope to an explicit path, creating parent directories as needed.
pub fn write_trace_at(path: &Path, trace: &TraceEnvelope) -> Result<(), TraceError> {
    if let Some(parent) = path.parent() {
        let parent_existed = parent.exists();
        fs::create_dir_all(parent)?;
        if !parent_existed {
            secure_private_dir(parent)?;
        }
    }
    fs::write(path, serde_json::to_vec_pretty(trace)?)?;
    secure_private_file(path)?;
    Ok(())
}

/// Builds trace-visible explainability for suppressed cascade groups.
pub fn trace_cascade_explainability_from_document(
    document: &DiagnosticDocument,
    analysis_artifact_id: Option<&str>,
) -> Option<TraceCascadeExplainability> {
    let document_analysis = document.document_analysis.as_ref()?;
    let provenance_capture_refs = provenance_capture_refs_by_group(document);
    let suppressed_groups = document_analysis
        .group_analysis
        .iter()
        .filter(|group| {
            matches!(
                group.role,
                GroupCascadeRole::FollowOn | GroupCascadeRole::Duplicate
            )
        })
        .map(|group| TraceSuppressedGroupExplainability {
            group_ref: group.group_ref.clone(),
            episode_ref: group.episode_ref.clone(),
            role: group.role,
            visibility_floor: group.visibility_floor,
            best_parent_group_ref: group.best_parent_group_ref.clone(),
            evidence_tags: group.evidence_tags.clone(),
            provenance_capture_refs: provenance_capture_refs
                .get(group.group_ref.as_str())
                .map(|refs| refs.iter().cloned().collect())
                .unwrap_or_default(),
        })
        .collect::<Vec<_>>();

    if suppressed_groups.is_empty() && analysis_artifact_id.is_none() {
        return None;
    }

    Some(TraceCascadeExplainability {
        analysis_artifact_id: analysis_artifact_id.map(ToOwned::to_owned),
        suppressed_groups,
    })
}

fn provenance_capture_refs_by_group(
    document: &DiagnosticDocument,
) -> BTreeMap<&str, BTreeSet<String>> {
    let mut by_group_ref = BTreeMap::new();
    for node in &document.diagnostics {
        let group_ref = node
            .analysis
            .as_ref()
            .and_then(|analysis| analysis.group_ref.as_deref())
            .map(str::trim)
            .filter(|group_ref| !group_ref.is_empty())
            .unwrap_or(node.id.as_str());
        let entry = by_group_ref.entry(group_ref).or_insert_with(BTreeSet::new);
        entry.extend(node.provenance.capture_refs.iter().cloned());
        for location in &node.locations {
            if let Some(provenance) = location.provenance_override.as_ref() {
                entry.extend(provenance.capture_refs.iter().cloned());
            }
        }
    }
    by_group_ref
}

/// Writes a build manifest as pretty-printed JSON and secures the file.
pub fn write_manifest(path: &Path, manifest: &BuildManifest) -> Result<(), TraceError> {
    if let Some(parent) = path.parent() {
        let parent_existed = parent.exists();
        fs::create_dir_all(parent)?;
        if !parent_existed {
            secure_private_dir(parent)?;
        }
    }
    fs::write(path, serde_json::to_vec_pretty(manifest)?)?;
    secure_private_file(path)?;
    Ok(())
}

/// Parses a target triple string into a [`TargetDescriptor`].
pub fn describe_target(target_triple: &str) -> TargetDescriptor {
    let segments = target_triple.split('-').collect::<Vec<_>>();
    let arch = segments.first().copied().unwrap_or("unknown").to_string();
    let os = if segments.contains(&"linux") {
        "linux"
    } else if segments.contains(&"darwin") {
        "macos"
    } else if segments.contains(&"windows") {
        "windows"
    } else {
        "unknown"
    }
    .to_string();
    let libc_family = if segments.contains(&"musl") {
        "musl"
    } else if segments
        .iter()
        .any(|segment| *segment == "gnu" || segment.starts_with("gnu"))
    {
        "gnu"
    } else if os == "macos" || os == "windows" {
        "none"
    } else {
        "unknown"
    }
    .to_string();

    TargetDescriptor {
        target_triple: target_triple.to_string(),
        os,
        arch,
        libc_family,
    }
}

/// Builds a [`BuildManifest`] for the given target triple and release metadata.
pub fn build_manifest_for_target(
    lockfile_hash: String,
    vendor_hash: String,
    target_triple: &str,
    maturity_label: &str,
    release_channel: &str,
) -> BuildManifest {
    let descriptor = describe_target(target_triple);
    BuildManifest {
        product_name: DEFAULT_PRODUCT_NAME.to_string(),
        product_version: env!("CARGO_PKG_VERSION").to_string(),
        artifact_target_triple: descriptor.target_triple,
        artifact_os: descriptor.os,
        artifact_arch: descriptor.arch,
        artifact_libc_family: descriptor.libc_family,
        git_commit: option_env!("FORMED_GIT_COMMIT")
            .unwrap_or("unknown")
            .to_string(),
        build_profile: option_env!("FORMED_BUILD_PROFILE")
            .unwrap_or("dev")
            .to_string(),
        rustc_version: option_env!("FORMED_RUSTC_VERSION")
            .unwrap_or("unknown")
            .to_string(),
        cargo_version: option_env!("FORMED_CARGO_VERSION")
            .unwrap_or("unknown")
            .to_string(),
        build_timestamp: option_env!("FORMED_BUILD_TIMESTAMP")
            .unwrap_or("unknown")
            .to_string(),
        lockfile_hash,
        vendor_hash,
        ir_spec_version: diag_core::IR_SPEC_VERSION.to_string(),
        adapter_spec_version: ADAPTER_SPEC_VERSION.to_string(),
        renderer_spec_version: RENDERER_SPEC_VERSION.to_string(),
        maturity_label: maturity_label.to_string(),
        release_channel: release_channel.to_string(),
        checksums: Vec::new(),
    }
}

/// Builds a [`BuildManifest`] using the compile-time target triple and default metadata.
pub fn default_build_manifest(lockfile_hash: String, vendor_hash: String) -> BuildManifest {
    build_manifest_for_target(
        lockfile_hash,
        vendor_hash,
        build_target_triple(),
        DEFAULT_MATURITY_LABEL,
        option_env!("FORMED_RELEASE_CHANNEL").unwrap_or("dev"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;
    use std::ffi::OsString;
    use std::path::PathBuf;

    #[test]
    fn retention_policy_matches_wrapper_expectations() {
        assert!(!should_retain(RetentionPolicy::Never, true, true));
        assert!(should_retain(
            RetentionPolicy::OnWrapperFailure,
            true,
            false
        ));
        assert!(should_retain(RetentionPolicy::OnChildError, false, true));
        assert!(should_retain(RetentionPolicy::Always, false, false));
    }

    #[test]
    fn retention_policy_truth_table_is_exhaustive() {
        let cases = [
            (RetentionPolicy::Never, false, false, false),
            (RetentionPolicy::Never, true, false, false),
            (RetentionPolicy::Never, false, true, false),
            (RetentionPolicy::OnWrapperFailure, false, false, false),
            (RetentionPolicy::OnWrapperFailure, true, false, true),
            (RetentionPolicy::OnWrapperFailure, false, true, false),
            (RetentionPolicy::OnChildError, false, false, false),
            (RetentionPolicy::OnChildError, true, false, true),
            (RetentionPolicy::OnChildError, false, true, true),
            (RetentionPolicy::Always, false, false, true),
            (RetentionPolicy::Always, true, false, true),
            (RetentionPolicy::Always, false, true, true),
        ];

        for (policy, wrapper_failed, child_failed, expected) in cases {
            assert_eq!(
                should_retain(policy, wrapper_failed, child_failed),
                expected,
                "policy={policy:?} wrapper_failed={wrapper_failed} child_failed={child_failed}"
            );
        }
    }

    #[test]
    fn trace_envelope_round_trips_through_json() {
        let trace = TraceEnvelope {
            trace_id: "trace-1".to_string(),
            selected_mode: "render".to_string(),
            selected_profile: "default".to_string(),
            wrapper_verdict: Some("fallback".to_string()),
            version_summary: Some(TraceVersionSummary {
                wrapper_version: "0.2.0-beta.1".to_string(),
                build_target_triple: "x86_64-unknown-linux-musl".to_string(),
                ir_spec_version: "v1alpha".to_string(),
                adapter_spec_version: "v1alpha".to_string(),
                renderer_spec_version: "v1alpha".to_string(),
            }),
            environment_summary: Some(TraceEnvironmentSummary {
                backend_path: PathBuf::from("/usr/bin/gcc"),
                backend_launcher_path: Some(PathBuf::from("/usr/bin/ccache")),
                backend_version: "gcc (GCC) 15.2.0".to_string(),
                version_band: "GCC15+".to_string(),
                processing_path: "DualSinkStructured".to_string(),
                support_level: "Preview".to_string(),
                backend_topology_kind: "single_backend_launcher".to_string(),
                backend_topology_policy_version: "v1beta-topology-2026-04-12".to_string(),
                injected_flags: vec!["-fdiagnostics-add-output=sarif".to_string()],
                sanitized_env_keys: vec!["HOME".to_string()],
                temp_artifact_paths: vec![PathBuf::from("/tmp/diag.sarif")],
            }),
            capabilities: Some(TraceCapabilities {
                stream_kind: "tty".to_string(),
                width_columns: Some(120),
                ansi_color: true,
                unicode: true,
                hyperlinks: false,
                interactive: true,
            }),
            timing: Some(TraceTiming {
                capture_ms: 4,
                render_ms: Some(2),
                total_ms: 6,
            }),
            child_exit: Some(TraceChildExit {
                code: Some(1),
                signal: None,
                success: false,
            }),
            parser_result_summary: Some(TraceParserResultSummary {
                status: "ok".to_string(),
                document_completeness: Some("partial".to_string()),
                diagnostic_count: 2,
                integrity_issue_count: 1,
                capture_count: 3,
            }),
            fingerprint_summary: Some(TraceFingerprintSummary {
                raw: "raw-fp".to_string(),
                normalized: Some("normalized-fp".to_string()),
                family: Some("family-fp".to_string()),
            }),
            redaction_status: Some(TraceRedactionStatus {
                class: "sanitized".to_string(),
                local_only: true,
                normalized_artifacts: vec!["stderr.raw".to_string()],
            }),
            decision_log: vec!["selected_dual_sink".to_string()],
            cascade_explainability: Some(TraceCascadeExplainability {
                analysis_artifact_id: Some("ir.analysis.json".to_string()),
                suppressed_groups: vec![TraceSuppressedGroupExplainability {
                    group_ref: "group-follow".to_string(),
                    episode_ref: Some("episode-1".to_string()),
                    role: GroupCascadeRole::FollowOn,
                    visibility_floor: VisibilityFloor::HiddenAllowed,
                    best_parent_group_ref: Some("group-root".to_string()),
                    evidence_tags: vec!["cascade".to_string()],
                    provenance_capture_refs: vec!["stderr.raw".to_string()],
                }],
            }),
            fallback_reason: Some(FallbackReason::ResidualOnly),
            warning_messages: vec!["kept raw stderr".to_string()],
            artifacts: vec![TraceArtifactRef {
                id: "stderr.raw".to_string(),
                path: Some(PathBuf::from("/tmp/stderr.raw")),
            }],
        };

        let encoded = serde_json::to_value(&trace).unwrap();
        let decoded: TraceEnvelope = serde_json::from_value(encoded.clone()).unwrap();

        assert_eq!(serde_json::to_value(&decoded).unwrap(), encoded);
    }

    #[test]
    fn trace_cascade_explainability_tracks_suppressed_groups_and_raw_provenance() {
        let document = diag_core::DiagnosticDocument {
            document_id: "doc-1".to_string(),
            schema_version: diag_core::IR_SPEC_VERSION.to_string(),
            document_completeness: diag_core::DocumentCompleteness::Complete,
            producer: diag_core::ProducerInfo {
                name: "gcc-formed".to_string(),
                version: "0.0.0-test".to_string(),
                git_revision: None,
                build_profile: None,
                rulepack_version: None,
            },
            run: diag_core::RunInfo {
                invocation_id: "trace-1".to_string(),
                invoked_as: Some("gcc-formed".to_string()),
                argv_redacted: Vec::new(),
                cwd_display: None,
                exit_status: 1,
                primary_tool: diag_core::ToolInfo {
                    name: "gcc".to_string(),
                    version: Some("15.2.0".to_string()),
                    component: None,
                    vendor: None,
                },
                secondary_tools: Vec::new(),
                language_mode: None,
                target_triple: None,
                wrapper_mode: Some(diag_core::WrapperSurface::Terminal),
            },
            captures: Vec::new(),
            integrity_issues: Vec::new(),
            diagnostics: vec![
                diag_core::DiagnosticNode {
                    id: "root".to_string(),
                    origin: diag_core::Origin::Gcc,
                    phase: diag_core::Phase::Parse,
                    severity: diag_core::Severity::Error,
                    semantic_role: diag_core::SemanticRole::Root,
                    message: diag_core::MessageText {
                        raw_text: "primary failure".to_string(),
                        normalized_text: None,
                        locale: None,
                    },
                    locations: vec![diag_core::Location::caret(
                        "src/main.c",
                        2,
                        1,
                        diag_core::LocationRole::Primary,
                    )],
                    children: Vec::new(),
                    suggestions: Vec::new(),
                    context_chains: Vec::new(),
                    symbol_context: None,
                    node_completeness: diag_core::NodeCompleteness::Complete,
                    provenance: diag_core::Provenance {
                        source: diag_core::ProvenanceSource::Compiler,
                        capture_refs: vec!["stderr.raw".to_string()],
                    },
                    analysis: Some(diag_core::AnalysisOverlay {
                        family: Some("syntax".into()),
                        family_version: None,
                        family_confidence: None,
                        root_cause_score: None,
                        actionability_score: None,
                        user_code_priority: None,
                        headline: Some("syntax error".into()),
                        first_action_hint: None,
                        confidence: None,
                        preferred_primary_location_id: None,
                        rule_id: None,
                        matched_conditions: Vec::new(),
                        suppression_reason: None,
                        collapsed_child_ids: Vec::new(),
                        collapsed_chain_ids: Vec::new(),
                        group_ref: Some("group-root".to_string()),
                        reasons: Vec::new(),
                        policy_profile: None,
                        producer_version: None,
                    }),
                    fingerprints: None,
                },
                diag_core::DiagnosticNode {
                    id: "follow".to_string(),
                    origin: diag_core::Origin::Gcc,
                    phase: diag_core::Phase::Parse,
                    severity: diag_core::Severity::Error,
                    semantic_role: diag_core::SemanticRole::Supporting,
                    message: diag_core::MessageText {
                        raw_text: "follow-on failure".to_string(),
                        normalized_text: None,
                        locale: None,
                    },
                    locations: vec![diag_core::Location::caret(
                        "src/main.c",
                        3,
                        1,
                        diag_core::LocationRole::Primary,
                    )],
                    children: Vec::new(),
                    suggestions: Vec::new(),
                    context_chains: Vec::new(),
                    symbol_context: None,
                    node_completeness: diag_core::NodeCompleteness::Complete,
                    provenance: diag_core::Provenance {
                        source: diag_core::ProvenanceSource::ResidualText,
                        capture_refs: vec![
                            "stderr.raw".to_string(),
                            "diagnostics.sarif".to_string(),
                        ],
                    },
                    analysis: Some(diag_core::AnalysisOverlay {
                        family: Some("syntax".into()),
                        family_version: None,
                        family_confidence: None,
                        root_cause_score: None,
                        actionability_score: None,
                        user_code_priority: None,
                        headline: Some("follow-on failure".into()),
                        first_action_hint: None,
                        confidence: None,
                        preferred_primary_location_id: None,
                        rule_id: None,
                        matched_conditions: Vec::new(),
                        suppression_reason: None,
                        collapsed_child_ids: Vec::new(),
                        collapsed_chain_ids: Vec::new(),
                        group_ref: Some("group-follow".to_string()),
                        reasons: Vec::new(),
                        policy_profile: None,
                        producer_version: None,
                    }),
                    fingerprints: None,
                },
            ],
            document_analysis: Some(diag_core::DocumentAnalysis {
                policy_profile: Some("default-aggressive".to_string()),
                producer_version: Some("test".to_string()),
                episode_graph: diag_core::EpisodeGraph {
                    episodes: vec![diag_core::DiagnosticEpisode {
                        episode_ref: "episode-1".to_string(),
                        lead_group_ref: "group-root".to_string(),
                        member_group_refs: vec![
                            "group-root".to_string(),
                            "group-follow".to_string(),
                        ],
                        family: Some("syntax".to_string()),
                        lead_root_score: Some(0.97.into()),
                        confidence: Some(0.91.into()),
                    }],
                    relations: Vec::new(),
                },
                group_analysis: vec![
                    diag_core::GroupCascadeAnalysis {
                        group_ref: "group-root".to_string(),
                        episode_ref: Some("episode-1".to_string()),
                        role: GroupCascadeRole::LeadRoot,
                        best_parent_group_ref: None,
                        root_score: Some(0.97.into()),
                        independence_score: Some(0.94.into()),
                        suppress_likelihood: Some(0.08.into()),
                        summary_likelihood: Some(0.12.into()),
                        visibility_floor: VisibilityFloor::NeverHidden,
                        evidence_tags: vec!["user_owned_primary".to_string()],
                    },
                    diag_core::GroupCascadeAnalysis {
                        group_ref: "group-follow".to_string(),
                        episode_ref: Some("episode-1".to_string()),
                        role: GroupCascadeRole::FollowOn,
                        best_parent_group_ref: Some("group-root".to_string()),
                        root_score: Some(0.18.into()),
                        independence_score: Some(0.12.into()),
                        suppress_likelihood: Some(0.89.into()),
                        summary_likelihood: Some(0.76.into()),
                        visibility_floor: VisibilityFloor::HiddenAllowed,
                        evidence_tags: vec![
                            "cascade".to_string(),
                            "shared_primary_file".to_string(),
                        ],
                    },
                ],
                stats: diag_core::CascadeStats {
                    independent_root_count: 1,
                    dependent_follow_on_count: 1,
                    duplicate_count: 0,
                    uncertain_count: 0,
                },
            }),
            fingerprints: None,
        };

        let explainability =
            trace_cascade_explainability_from_document(&document, Some("ir.analysis.json"))
                .unwrap();
        assert_eq!(
            explainability.analysis_artifact_id.as_deref(),
            Some("ir.analysis.json")
        );
        assert_eq!(explainability.suppressed_groups.len(), 1);
        assert_eq!(
            explainability.suppressed_groups[0],
            TraceSuppressedGroupExplainability {
                group_ref: "group-follow".to_string(),
                episode_ref: Some("episode-1".to_string()),
                role: GroupCascadeRole::FollowOn,
                visibility_floor: VisibilityFloor::HiddenAllowed,
                best_parent_group_ref: Some("group-root".to_string()),
                evidence_tags: vec!["cascade".to_string(), "shared_primary_file".to_string()],
                provenance_capture_refs: vec![
                    "diagnostics.sarif".to_string(),
                    "stderr.raw".to_string()
                ],
            }
        );
    }

    #[test]
    fn discovers_xdg_paths_and_target_aware_install_root() {
        let env = BTreeMap::from([
            ("XDG_CONFIG_HOME".to_string(), OsString::from("/xdg/config")),
            ("XDG_CACHE_HOME".to_string(), OsString::from("/xdg/cache")),
            ("XDG_STATE_HOME".to_string(), OsString::from("/xdg/state")),
        ]);
        let paths = WrapperPaths::from_env(
            |key| env.get(key).cloned(),
            PathBuf::from("/home/tester"),
            PathBuf::from("/tmp"),
            "x86_64-unknown-linux-musl",
        );

        assert_eq!(
            paths.config_path,
            PathBuf::from("/xdg/config/cc-formed/config.toml")
        );
        assert_eq!(paths.cache_root, PathBuf::from("/xdg/cache/cc-formed"));
        assert_eq!(paths.state_root, PathBuf::from("/xdg/state/cc-formed"));
        assert_eq!(
            paths.runtime_root,
            PathBuf::from("/tmp/cc-formed-runtime/cc-formed")
        );
        assert_eq!(
            paths.install_root,
            PathBuf::from("/home/tester/.local/opt/cc-formed/x86_64-unknown-linux-musl")
        );
    }

    #[test]
    fn empty_env_overrides_fall_back_to_default_wrapper_paths() {
        let env = BTreeMap::from([
            ("XDG_CONFIG_HOME".to_string(), OsString::new()),
            ("XDG_CACHE_HOME".to_string(), OsString::new()),
            ("XDG_STATE_HOME".to_string(), OsString::new()),
            ("XDG_RUNTIME_DIR".to_string(), OsString::new()),
            ("FORMED_CONFIG_FILE".to_string(), OsString::new()),
            ("FORMED_CONFIG_DIR".to_string(), OsString::new()),
            ("FORMED_CACHE_DIR".to_string(), OsString::new()),
            ("FORMED_STATE_DIR".to_string(), OsString::new()),
            ("FORMED_RUNTIME_DIR".to_string(), OsString::new()),
            ("FORMED_TRACE_DIR".to_string(), OsString::new()),
            ("FORMED_INSTALL_ROOT".to_string(), OsString::new()),
        ]);
        let paths = WrapperPaths::from_env(
            |key| env.get(key).cloned(),
            PathBuf::from("/home/tester"),
            PathBuf::from("/tmp"),
            "x86_64-unknown-linux-gnu",
        );

        assert_eq!(
            paths.config_path,
            PathBuf::from("/home/tester/.config/cc-formed/config.toml")
        );
        assert_eq!(
            paths.cache_root,
            PathBuf::from("/home/tester/.cache/cc-formed")
        );
        assert_eq!(
            paths.state_root,
            PathBuf::from("/home/tester/.local/state/cc-formed")
        );
        assert_eq!(
            paths.runtime_root,
            PathBuf::from("/tmp/cc-formed-runtime/cc-formed")
        );
        assert_eq!(
            paths.trace_root,
            PathBuf::from("/home/tester/.local/state/cc-formed/traces")
        );
        assert_eq!(
            paths.install_root,
            PathBuf::from("/home/tester/.local/opt/cc-formed/x86_64-unknown-linux-gnu")
        );
    }

    #[test]
    fn formed_overrides_take_precedence() {
        let env = BTreeMap::from([
            (
                "FORMED_CONFIG_FILE".to_string(),
                OsString::from("/custom/config.toml"),
            ),
            (
                "FORMED_CACHE_DIR".to_string(),
                OsString::from("/custom/cache-root"),
            ),
            (
                "FORMED_STATE_DIR".to_string(),
                OsString::from("/custom/state-root"),
            ),
            (
                "FORMED_RUNTIME_DIR".to_string(),
                OsString::from("/custom/runtime-root"),
            ),
            (
                "FORMED_TRACE_DIR".to_string(),
                OsString::from("/custom/trace-root"),
            ),
            (
                "FORMED_INSTALL_ROOT".to_string(),
                OsString::from("/custom/install-root"),
            ),
        ]);
        let paths = WrapperPaths::from_env(
            |key| env.get(key).cloned(),
            PathBuf::from("/home/tester"),
            PathBuf::from("/tmp"),
            "x86_64-unknown-linux-gnu",
        );

        assert_eq!(paths.config_path, PathBuf::from("/custom/config.toml"));
        assert_eq!(paths.cache_root, PathBuf::from("/custom/cache-root"));
        assert_eq!(paths.state_root, PathBuf::from("/custom/state-root"));
        assert_eq!(paths.runtime_root, PathBuf::from("/custom/runtime-root"));
        assert_eq!(paths.trace_root, PathBuf::from("/custom/trace-root"));
        assert_eq!(paths.install_root, PathBuf::from("/custom/install-root"));
    }

    #[test]
    fn empty_home_env_uses_current_directory_fallback() {
        let home = home_from_env(Some(OsString::new()));

        assert_eq!(home, PathBuf::from("."));
    }

    #[cfg(unix)]
    #[test]
    fn writes_private_trace_and_manifest_files() {
        use std::os::unix::fs::PermissionsExt;

        let temp = std::env::temp_dir().join(format!(
            "diag-trace-test-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&temp).unwrap();
        let trace_path = temp.join("trace.json");
        let manifest_path = temp.join("manifest.json");
        let trace = TraceEnvelope {
            trace_id: "trace-1".to_string(),
            selected_mode: "render".to_string(),
            selected_profile: "default".to_string(),
            wrapper_verdict: None,
            version_summary: None,
            environment_summary: None,
            capabilities: None,
            timing: None,
            child_exit: None,
            parser_result_summary: None,
            fingerprint_summary: None,
            redaction_status: None,
            decision_log: Vec::new(),
            cascade_explainability: None,
            fallback_reason: None,
            warning_messages: Vec::new(),
            artifacts: Vec::new(),
        };

        write_trace_at(&trace_path, &trace).unwrap();
        write_manifest(
            &manifest_path,
            &default_build_manifest("lock".to_string(), "vendor".to_string()),
        )
        .unwrap();

        assert_eq!(
            fs::metadata(&trace_path).unwrap().permissions().mode() & 0o777,
            0o600
        );
        assert_eq!(
            fs::metadata(&manifest_path).unwrap().permissions().mode() & 0o777,
            0o600
        );
        fs::remove_dir_all(temp).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn preserves_existing_parent_directory_permissions_when_writing_trace() {
        use std::os::unix::fs::PermissionsExt;

        let temp = std::env::temp_dir().join(format!(
            "diag-trace-test-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let parent = temp.join("existing-parent");
        fs::create_dir_all(&parent).unwrap();
        fs::set_permissions(&parent, fs::Permissions::from_mode(0o755)).unwrap();

        let trace_path = parent.join("trace.json");
        let trace = TraceEnvelope {
            trace_id: "trace-2".to_string(),
            selected_mode: "render".to_string(),
            selected_profile: "default".to_string(),
            wrapper_verdict: None,
            version_summary: None,
            environment_summary: None,
            capabilities: None,
            timing: None,
            child_exit: None,
            parser_result_summary: None,
            fingerprint_summary: None,
            redaction_status: None,
            decision_log: Vec::new(),
            cascade_explainability: None,
            fallback_reason: None,
            warning_messages: Vec::new(),
            artifacts: Vec::new(),
        };

        write_trace_at(&trace_path, &trace).unwrap();

        assert_eq!(
            fs::metadata(&parent).unwrap().permissions().mode() & 0o777,
            0o755
        );
        assert_eq!(
            fs::metadata(&trace_path).unwrap().permissions().mode() & 0o777,
            0o600
        );
        fs::remove_dir_all(temp).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn write_manifest_creates_missing_parent_directories() {
        use std::os::unix::fs::PermissionsExt;

        let temp = std::env::temp_dir().join(format!(
            "diag-trace-test-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let manifest_path = temp.join("nested/control/manifest.json");

        write_manifest(
            &manifest_path,
            &default_build_manifest("lock".to_string(), "vendor".to_string()),
        )
        .unwrap();

        let parent = manifest_path.parent().unwrap();
        assert!(manifest_path.exists());
        assert_eq!(
            fs::metadata(parent).unwrap().permissions().mode() & 0o777,
            0o700
        );
        assert_eq!(
            fs::metadata(&manifest_path).unwrap().permissions().mode() & 0o777,
            0o600
        );
        fs::remove_dir_all(temp).unwrap();
    }

    #[test]
    fn target_descriptor_tracks_linux_libc_family() {
        let musl = describe_target("x86_64-unknown-linux-musl");
        assert_eq!(musl.os, "linux");
        assert_eq!(musl.arch, "x86_64");
        assert_eq!(musl.libc_family, "musl");

        let gnu = describe_target("aarch64-unknown-linux-gnu");
        assert_eq!(gnu.os, "linux");
        assert_eq!(gnu.arch, "aarch64");
        assert_eq!(gnu.libc_family, "gnu");
    }

    #[test]
    fn target_descriptor_handles_non_linux_and_unknown_targets() {
        let macos = describe_target("aarch64-apple-darwin");
        assert_eq!(macos.os, "macos");
        assert_eq!(macos.libc_family, "none");

        let windows = describe_target("x86_64-pc-windows-msvc");
        assert_eq!(windows.os, "windows");
        assert_eq!(windows.libc_family, "none");

        let unknown = describe_target("wasm32-unknown-unknown");
        assert_eq!(unknown.os, "unknown");
        assert_eq!(unknown.libc_family, "unknown");
    }

    #[test]
    fn build_manifest_for_target_infers_artifact_metadata() {
        let manifest = build_manifest_for_target(
            "lock".to_string(),
            "vendor".to_string(),
            "x86_64-unknown-linux-musl",
            DEFAULT_MATURITY_LABEL,
            "stable",
        );

        assert_eq!(manifest.artifact_target_triple, "x86_64-unknown-linux-musl");
        assert_eq!(manifest.artifact_os, "linux");
        assert_eq!(manifest.artifact_arch, "x86_64");
        assert_eq!(manifest.artifact_libc_family, "musl");
        assert_eq!(manifest.maturity_label, DEFAULT_MATURITY_LABEL);
        assert_eq!(manifest.release_channel, "stable");
        assert!(manifest.checksums.is_empty());
    }

    #[test]
    fn build_manifest_accepts_legacy_support_tier_alias() {
        let mut manifest_json = serde_json::to_value(default_build_manifest(
            "lock".to_string(),
            "vendor".to_string(),
        ))
        .unwrap();
        let object = manifest_json.as_object_mut().unwrap();
        let maturity_label = object.remove("maturity_label").unwrap();
        object.insert("support_tier_declaration".to_string(), maturity_label);

        let manifest: BuildManifest = serde_json::from_value(manifest_json).unwrap();

        assert_eq!(manifest.maturity_label, DEFAULT_MATURITY_LABEL);
        assert_eq!(manifest.release_channel, "dev");
    }
}
