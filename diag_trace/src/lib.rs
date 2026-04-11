use diag_core::{ADAPTER_SPEC_VERSION, FallbackReason, RENDERER_SPEC_VERSION};
use serde::{Deserialize, Serialize};
use std::env;
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

pub const DEFAULT_PRODUCT_NAME: &str = "gcc-formed";
pub const DEFAULT_MATURITY_LABEL: &str = "v1beta";

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RetentionPolicy {
    Never,
    OnWrapperFailure,
    OnChildError,
    Always,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WrapperPaths {
    pub config_path: PathBuf,
    pub cache_root: PathBuf,
    pub state_root: PathBuf,
    pub runtime_root: PathBuf,
    pub trace_root: PathBuf,
    pub install_root: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuildManifest {
    pub product_name: String,
    pub product_version: String,
    pub artifact_target_triple: String,
    pub artifact_os: String,
    pub artifact_arch: String,
    pub artifact_libc_family: String,
    pub git_commit: String,
    pub build_profile: String,
    pub rustc_version: String,
    pub cargo_version: String,
    pub build_timestamp: String,
    pub lockfile_hash: String,
    pub vendor_hash: String,
    pub ir_spec_version: String,
    pub adapter_spec_version: String,
    pub renderer_spec_version: String,
    #[serde(alias = "support_tier_declaration")]
    pub maturity_label: String,
    pub release_channel: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub checksums: Vec<ChecksumEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ChecksumEntry {
    pub path: String,
    pub sha256: String,
    pub size_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TargetDescriptor {
    pub target_triple: String,
    pub os: String,
    pub arch: String,
    pub libc_family: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraceEnvelope {
    pub trace_id: String,
    pub selected_mode: String,
    pub selected_profile: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wrapper_verdict: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version_summary: Option<TraceVersionSummary>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub environment_summary: Option<TraceEnvironmentSummary>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub capabilities: Option<TraceCapabilities>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timing: Option<TraceTiming>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub child_exit: Option<TraceChildExit>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parser_result_summary: Option<TraceParserResultSummary>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fingerprint_summary: Option<TraceFingerprintSummary>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub redaction_status: Option<TraceRedactionStatus>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub decision_log: Vec<String>,
    pub fallback_reason: Option<FallbackReason>,
    pub warning_messages: Vec<String>,
    pub artifacts: Vec<TraceArtifactRef>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraceVersionSummary {
    pub wrapper_version: String,
    pub build_target_triple: String,
    pub ir_spec_version: String,
    pub adapter_spec_version: String,
    pub renderer_spec_version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraceEnvironmentSummary {
    pub backend_path: PathBuf,
    pub backend_version: String,
    pub version_band: String,
    pub processing_path: String,
    pub support_level: String,
    #[serde(default)]
    pub injected_flags: Vec<String>,
    #[serde(default)]
    pub sanitized_env_keys: Vec<String>,
    #[serde(default)]
    pub temp_artifact_paths: Vec<PathBuf>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraceCapabilities {
    pub stream_kind: String,
    pub width_columns: Option<usize>,
    pub ansi_color: bool,
    pub unicode: bool,
    pub hyperlinks: bool,
    pub interactive: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraceTiming {
    pub capture_ms: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub render_ms: Option<u64>,
    pub total_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraceChildExit {
    pub code: Option<i32>,
    pub signal: Option<i32>,
    pub success: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraceParserResultSummary {
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub document_completeness: Option<String>,
    pub diagnostic_count: usize,
    pub integrity_issue_count: usize,
    pub capture_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraceFingerprintSummary {
    pub raw: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub normalized: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub family: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraceRedactionStatus {
    pub class: String,
    pub local_only: bool,
    #[serde(default)]
    pub normalized_artifacts: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraceArtifactRef {
    pub id: String,
    pub path: Option<PathBuf>,
}

#[derive(Debug, thiserror::Error)]
pub enum TraceError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),
}

impl WrapperPaths {
    pub fn discover() -> Self {
        Self::from_env(
            |key| env::var_os(key),
            env::var_os("HOME")
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from(".")),
            env::temp_dir(),
            build_target_triple(),
        )
    }

    fn from_env<F>(get_var: F, home: PathBuf, temp_dir: PathBuf, target_triple: &str) -> Self
    where
        F: Fn(&str) -> Option<OsString>,
    {
        let config_home = get_var("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| home.join(".config"));
        let cache_home = get_var("XDG_CACHE_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| home.join(".cache"));
        let state_home = get_var("XDG_STATE_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| home.join(".local").join("state"));
        let runtime_home = get_var("XDG_RUNTIME_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|| temp_dir.join("cc-formed-runtime"));

        let config_path = get_var("FORMED_CONFIG_FILE")
            .map(PathBuf::from)
            .or_else(|| {
                get_var("FORMED_CONFIG_DIR")
                    .map(PathBuf::from)
                    .map(|dir| dir.join("config.toml"))
            })
            .unwrap_or_else(|| config_home.join("cc-formed").join("config.toml"));
        let cache_root = get_var("FORMED_CACHE_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|| cache_home.join("cc-formed"));
        let state_root = get_var("FORMED_STATE_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|| state_home.join("cc-formed"));
        let runtime_root = get_var("FORMED_RUNTIME_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|| runtime_home.join("cc-formed"));
        let trace_root = get_var("FORMED_TRACE_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|| state_root.join("traces"));
        let install_root = get_var("FORMED_INSTALL_ROOT")
            .map(PathBuf::from)
            .unwrap_or_else(|| {
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

pub fn build_target_triple() -> &'static str {
    option_env!("FORMED_TARGET").unwrap_or("unknown-target")
}

#[cfg(unix)]
pub fn secure_private_dir(path: &Path) -> Result<(), std::io::Error> {
    use std::os::unix::fs::PermissionsExt;

    let metadata = fs::metadata(path)?;
    let mut permissions = metadata.permissions();
    permissions.set_mode(0o700);
    fs::set_permissions(path, permissions)
}

#[cfg(not(unix))]
pub fn secure_private_dir(_path: &Path) -> Result<(), std::io::Error> {
    Ok(())
}

#[cfg(unix)]
pub fn secure_private_file(path: &Path) -> Result<(), std::io::Error> {
    use std::os::unix::fs::PermissionsExt;

    let metadata = fs::metadata(path)?;
    let mut permissions = metadata.permissions();
    permissions.set_mode(0o600);
    fs::set_permissions(path, permissions)
}

#[cfg(not(unix))]
pub fn secure_private_file(_path: &Path) -> Result<(), std::io::Error> {
    Ok(())
}

pub fn trace_id() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default();
    format!("trace-{nanos}")
}

pub fn should_retain(policy: RetentionPolicy, wrapper_failed: bool, child_failed: bool) -> bool {
    match policy {
        RetentionPolicy::Never => false,
        RetentionPolicy::OnWrapperFailure => wrapper_failed,
        RetentionPolicy::OnChildError => wrapper_failed || child_failed,
        RetentionPolicy::Always => true,
    }
}

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

pub fn write_trace_at(path: &Path, trace: &TraceEnvelope) -> Result<(), TraceError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
        secure_private_dir(parent)?;
    }
    fs::write(path, serde_json::to_vec_pretty(trace)?)?;
    secure_private_file(path)?;
    Ok(())
}

pub fn write_manifest(path: &Path, manifest: &BuildManifest) -> Result<(), TraceError> {
    fs::write(path, serde_json::to_vec_pretty(manifest)?)?;
    secure_private_file(path)?;
    Ok(())
}

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
}
