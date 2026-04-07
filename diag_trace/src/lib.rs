use diag_core::{ADAPTER_SPEC_VERSION, RENDERER_SPEC_VERSION};
use serde::{Deserialize, Serialize};
use std::env;
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

pub const DEFAULT_PRODUCT_NAME: &str = "gcc-formed";

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
    pub support_tier_declaration: String,
    pub release_channel: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraceEnvelope {
    pub trace_id: String,
    pub selected_mode: String,
    pub selected_profile: String,
    pub support_tier: String,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub decision_log: Vec<String>,
    pub fallback_reason: Option<String>,
    pub warning_messages: Vec<String>,
    pub artifacts: Vec<TraceArtifactRef>,
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
fn secure_private_dir(path: &Path) -> Result<(), std::io::Error> {
    use std::os::unix::fs::PermissionsExt;

    let metadata = fs::metadata(path)?;
    let mut permissions = metadata.permissions();
    permissions.set_mode(0o700);
    fs::set_permissions(path, permissions)
}

#[cfg(not(unix))]
fn secure_private_dir(_path: &Path) -> Result<(), std::io::Error> {
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
    fs::write(&path, serde_json::to_vec_pretty(trace)?)?;
    Ok(path)
}

pub fn write_manifest(path: &Path, manifest: &BuildManifest) -> Result<(), TraceError> {
    fs::write(path, serde_json::to_vec_pretty(manifest)?)?;
    Ok(())
}

pub fn default_build_manifest(lockfile_hash: String, vendor_hash: String) -> BuildManifest {
    BuildManifest {
        product_name: DEFAULT_PRODUCT_NAME.to_string(),
        product_version: env!("CARGO_PKG_VERSION").to_string(),
        artifact_target_triple: option_env!("FORMED_TARGET")
            .unwrap_or("unknown-target")
            .to_string(),
        artifact_os: env::consts::OS.to_string(),
        artifact_arch: env::consts::ARCH.to_string(),
        artifact_libc_family: if env::consts::OS == "linux" {
            "gnu".to_string()
        } else {
            "unknown".to_string()
        },
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
        support_tier_declaration: "gcc15_primary".to_string(),
        release_channel: option_env!("FORMED_RELEASE_CHANNEL")
            .unwrap_or("dev")
            .to_string(),
    }
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
}
