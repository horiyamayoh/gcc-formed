use crate::args::{parse_debug_refs, parse_mode, parse_profile, parse_retention_policy};
use diag_capture_runtime::ExecutionMode;
use diag_render::{DebugRefs, PathPolicy, RenderProfile};
use diag_trace::{RetentionPolicy, WrapperPaths};
use serde::Deserialize;
use std::env;
use std::fs;
use std::path::PathBuf;

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
    pub(crate) fn load(paths: &WrapperPaths) -> Result<Self, Box<dyn std::error::Error>> {
        let mut merged = ConfigFile::default();
        if let Some(admin) = admin_config_path() {
            if admin.exists() {
                merged = merge_config(merged, toml::from_str(&fs::read_to_string(admin)?)?);
            }
        }
        if paths.config_path.exists() {
            merged = merge_config(
                merged,
                toml::from_str(&fs::read_to_string(&paths.config_path)?)?,
            );
        }
        Ok(merged)
    }
}

fn admin_config_path() -> Option<PathBuf> {
    let dirs = env::var_os("XDG_CONFIG_DIRS")
        .map(|value| env::split_paths(&value).collect::<Vec<_>>())
        .unwrap_or_else(|| vec![PathBuf::from("/etc/xdg")]);
    dirs.into_iter()
        .next()
        .map(|dir| dir.join("cc-formed").join("config.toml"))
}

fn merge_config(base: ConfigFile, overlay: ConfigFile) -> ConfigFile {
    ConfigFile {
        schema_version: overlay.schema_version.or(base.schema_version),
        backend: BackendSection {
            gcc: overlay.backend.gcc.or(base.backend.gcc),
        },
        runtime: RuntimeSection {
            mode: overlay.runtime.mode.or(base.runtime.mode),
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

    #[test]
    fn overlay_config_overrides_matching_fields_only() {
        let base = ConfigFile {
            schema_version: Some(1),
            backend: BackendSection {
                gcc: Some(PathBuf::from("/usr/bin/gcc")),
            },
            runtime: RuntimeSection {
                mode: Some(ExecutionMode::Shadow),
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
        assert_eq!(merged.render.profile, Some(RenderProfile::Verbose));
        assert_eq!(merged.render.path_policy, Some(PathPolicy::RelativeToCwd));
        assert_eq!(merged.render.debug_refs, Some(DebugRefs::CaptureRef));
        assert_eq!(merged.trace.retention_policy, Some(RetentionPolicy::Always));
    }
}
