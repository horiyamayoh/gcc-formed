use crate::args::{
    parse_debug_refs, parse_mode, parse_processing_path, parse_profile, parse_retention_policy,
};
use crate::error::CliError;
use diag_backend_probe::ProcessingPath;
use diag_capture_runtime::ExecutionMode;
use diag_render::{DebugRefs, PathPolicy, RenderProfile};
use diag_trace::{RetentionPolicy, WrapperPaths};
use serde::Deserialize;
use std::env;
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Default, Deserialize)]
pub(crate) struct ConfigFile {
    #[serde(default)]
    pub(crate) schema_version: Option<u32>,
    #[serde(default)]
    pub(crate) backend: BackendSection,
    #[serde(default)]
    pub(crate) runtime: RuntimeSection,
    #[serde(default)]
    pub(crate) render: RenderSection,
    #[serde(default)]
    pub(crate) trace: TraceSection,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub(crate) struct BackendSection {
    #[serde(default)]
    pub(crate) gcc: Option<PathBuf>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub(crate) struct RuntimeSection {
    #[serde(default, deserialize_with = "deserialize_optional_mode")]
    pub(crate) mode: Option<ExecutionMode>,
    #[serde(default, deserialize_with = "deserialize_optional_processing_path")]
    pub(crate) processing_path: Option<ProcessingPath>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub(crate) struct RenderSection {
    #[serde(default, deserialize_with = "deserialize_optional_profile")]
    pub(crate) profile: Option<RenderProfile>,
    #[serde(default, deserialize_with = "deserialize_optional_path_policy")]
    pub(crate) path_policy: Option<PathPolicy>,
    #[serde(default, deserialize_with = "deserialize_optional_debug_refs")]
    pub(crate) debug_refs: Option<DebugRefs>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub(crate) struct TraceSection {
    #[serde(default, deserialize_with = "deserialize_optional_retention")]
    pub(crate) retention_policy: Option<RetentionPolicy>,
}

impl ConfigFile {
    pub(crate) fn load(paths: &WrapperPaths) -> Result<Self, CliError> {
        Self::load_from_paths(admin_config_paths(), Some(&paths.config_path))
    }

    fn load_from_paths<I, P>(admin_paths: I, user_path: Option<P>) -> Result<Self, CliError>
    where
        I: IntoIterator<Item = PathBuf>,
        P: AsRef<Path>,
    {
        let mut merged = ConfigFile::default();
        if let Some(admin) = admin_paths.into_iter().find(|path| path.exists()) {
            merged = merge_config(merged, load_config_file(&admin)?);
        }
        if let Some(user_path) = user_path
            && user_path.as_ref().exists()
        {
            merged = merge_config(merged, load_config_file(user_path.as_ref())?);
        }
        Ok(merged)
    }
}

fn admin_config_paths() -> Vec<PathBuf> {
    admin_config_paths_from(env::var_os("XDG_CONFIG_DIRS"))
}

fn admin_config_paths_from(raw_xdg_config_dirs: Option<OsString>) -> Vec<PathBuf> {
    let dirs = raw_xdg_config_dirs
        .as_deref()
        .filter(|value| !value.is_empty())
        .map(|value| {
            env::split_paths(value)
                .filter(|path| !path.as_os_str().is_empty())
                .collect::<Vec<_>>()
        })
        .filter(|paths| !paths.is_empty())
        .unwrap_or_else(|| vec![PathBuf::from("/etc/xdg")]);
    dirs.into_iter()
        .map(|dir| dir.join("cc-formed").join("config.toml"))
        .collect()
}

fn load_config_file(path: &Path) -> Result<ConfigFile, CliError> {
    toml::from_str(&fs::read_to_string(path)?).map_err(|e| CliError::Config(e.to_string()))
}

fn merge_config(base: ConfigFile, overlay: ConfigFile) -> ConfigFile {
    ConfigFile {
        schema_version: overlay.schema_version.or(base.schema_version),
        backend: BackendSection {
            gcc: overlay.backend.gcc.or(base.backend.gcc),
        },
        runtime: RuntimeSection {
            mode: overlay.runtime.mode.or(base.runtime.mode),
            processing_path: overlay
                .runtime
                .processing_path
                .or(base.runtime.processing_path),
        },
        render: RenderSection {
            profile: overlay.render.profile.or(base.render.profile),
            path_policy: overlay.render.path_policy.or(base.render.path_policy),
            debug_refs: overlay.render.debug_refs.or(base.render.debug_refs),
        },
        trace: TraceSection {
            retention_policy: overlay
                .trace
                .retention_policy
                .or(base.trace.retention_policy),
        },
    }
}

fn deserialize_optional_mode<'de, D>(deserializer: D) -> Result<Option<ExecutionMode>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = Option::<String>::deserialize(deserializer)?;
    value
        .map(|value| parse_mode(&value).map_err(serde::de::Error::custom))
        .transpose()
}

fn deserialize_optional_processing_path<'de, D>(
    deserializer: D,
) -> Result<Option<ProcessingPath>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = Option::<String>::deserialize(deserializer)?;
    value
        .map(|value| parse_processing_path(&value).map_err(serde::de::Error::custom))
        .transpose()
}

fn deserialize_optional_profile<'de, D>(deserializer: D) -> Result<Option<RenderProfile>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = Option::<String>::deserialize(deserializer)?;
    value
        .map(|value| parse_profile(&value).map_err(serde::de::Error::custom))
        .transpose()
}

fn deserialize_optional_path_policy<'de, D>(deserializer: D) -> Result<Option<PathPolicy>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = Option::<String>::deserialize(deserializer)?;
    match value.as_deref() {
        None => Ok(None),
        Some("shortest_unambiguous") => Ok(Some(PathPolicy::ShortestUnambiguous)),
        Some("relative_to_cwd") => Ok(Some(PathPolicy::RelativeToCwd)),
        Some("absolute") => Ok(Some(PathPolicy::Absolute)),
        Some(other) => Err(serde::de::Error::custom(format!(
            "unsupported path policy: {other}"
        ))),
    }
}

fn deserialize_optional_debug_refs<'de, D>(deserializer: D) -> Result<Option<DebugRefs>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = Option::<String>::deserialize(deserializer)?;
    value
        .map(|value| parse_debug_refs(&value).map_err(serde::de::Error::custom))
        .transpose()
}

fn deserialize_optional_retention<'de, D>(
    deserializer: D,
) -> Result<Option<RetentionPolicy>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = Option::<String>::deserialize(deserializer)?;
    value
        .map(|value| parse_retention_policy(&value).map_err(serde::de::Error::custom))
        .transpose()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn overlay_config_overrides_matching_fields_only() {
        let base = ConfigFile {
            schema_version: Some(1),
            backend: BackendSection {
                gcc: Some(PathBuf::from("/usr/bin/gcc")),
            },
            runtime: RuntimeSection {
                mode: Some(ExecutionMode::Shadow),
                processing_path: Some(ProcessingPath::NativeTextCapture),
            },
            render: RenderSection {
                profile: Some(RenderProfile::Verbose),
                path_policy: Some(PathPolicy::Absolute),
                debug_refs: Some(DebugRefs::CaptureRef),
            },
            trace: TraceSection {
                retention_policy: Some(RetentionPolicy::OnChildError),
            },
        };
        let overlay = ConfigFile {
            schema_version: None,
            backend: BackendSection { gcc: None },
            runtime: RuntimeSection {
                mode: Some(ExecutionMode::Render),
                processing_path: Some(ProcessingPath::SingleSinkStructured),
            },
            render: RenderSection {
                profile: None,
                path_policy: Some(PathPolicy::RelativeToCwd),
                debug_refs: None,
            },
            trace: TraceSection {
                retention_policy: Some(RetentionPolicy::Always),
            },
        };

        let merged = merge_config(base, overlay);
        assert_eq!(merged.schema_version, Some(1));
        assert_eq!(merged.backend.gcc, Some(PathBuf::from("/usr/bin/gcc")));
        assert_eq!(merged.runtime.mode, Some(ExecutionMode::Render));
        assert_eq!(
            merged.runtime.processing_path,
            Some(ProcessingPath::SingleSinkStructured)
        );
        assert_eq!(merged.render.profile, Some(RenderProfile::Verbose));
        assert_eq!(merged.render.path_policy, Some(PathPolicy::RelativeToCwd));
        assert_eq!(merged.render.debug_refs, Some(DebugRefs::CaptureRef));
        assert_eq!(merged.trace.retention_policy, Some(RetentionPolicy::Always));
    }

    #[test]
    fn load_uses_first_existing_admin_config_candidate() {
        let temp = tempdir().unwrap();
        let missing_admin = temp
            .path()
            .join("etc-a")
            .join("cc-formed")
            .join("config.toml");
        let fallback_admin = temp
            .path()
            .join("etc-b")
            .join("cc-formed")
            .join("config.toml");
        fs::create_dir_all(fallback_admin.parent().unwrap()).unwrap();
        fs::write(
            &fallback_admin,
            r#"
                [backend]
                gcc = "/opt/gcc-from-second"
            "#,
        )
        .unwrap();

        let loaded =
            ConfigFile::load_from_paths([missing_admin, fallback_admin], Option::<&Path>::None)
                .unwrap();

        assert_eq!(
            loaded.backend.gcc,
            Some(PathBuf::from("/opt/gcc-from-second"))
        );
    }

    #[test]
    fn user_config_overrides_first_existing_admin_config() {
        let temp = tempdir().unwrap();
        let admin = temp
            .path()
            .join("etc")
            .join("cc-formed")
            .join("config.toml");
        let user = temp.path().join("user-config.toml");
        fs::create_dir_all(admin.parent().unwrap()).unwrap();
        fs::write(
            &admin,
            r#"
                [backend]
                gcc = "/opt/gcc-from-admin"

                [runtime]
                mode = "shadow"
            "#,
        )
        .unwrap();
        fs::write(
            &user,
            r#"
                [runtime]
                mode = "render"
            "#,
        )
        .unwrap();

        let loaded = ConfigFile::load_from_paths([admin], Some(&user)).unwrap();

        assert_eq!(
            loaded.backend.gcc,
            Some(PathBuf::from("/opt/gcc-from-admin"))
        );
        assert_eq!(loaded.runtime.mode, Some(ExecutionMode::Render));
    }

    #[test]
    fn admin_config_paths_fall_back_to_default_when_xdg_config_dirs_is_empty() {
        let paths = admin_config_paths_from(Some(OsString::new()));

        assert_eq!(paths, vec![PathBuf::from("/etc/xdg/cc-formed/config.toml")]);
    }

    #[test]
    fn admin_config_paths_ignore_empty_candidates_in_xdg_config_dirs() {
        let separator = if cfg!(windows) { ";" } else { ":" };
        let raw = OsString::from(format!(
            "{separator}/opt/xdg-a{separator}/opt/xdg-b{separator}"
        ));

        let paths = admin_config_paths_from(Some(raw));

        assert_eq!(
            paths,
            vec![
                PathBuf::from("/opt/xdg-a/cc-formed/config.toml"),
                PathBuf::from("/opt/xdg-b/cc-formed/config.toml"),
            ]
        );
    }
}
