use serde::{Deserialize, Serialize};
use std::collections::HashMap;
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
    fn parses_version_line() {
        let parsed = parse_version("gcc (Ubuntu 15.1.0-1) 15.1.0").unwrap();
        assert_eq!(parsed, (15, 1));
    }
}
