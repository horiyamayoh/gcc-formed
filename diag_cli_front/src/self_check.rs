use crate::args::WrapperIntrospection;
use diag_backend_probe::{ProbeCache, ResolveRequest};
use diag_trace::{
    BuildManifest, WrapperPaths, build_target_triple, default_build_manifest, trace_id,
};
use serde_json::json;
use std::env;
use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};

pub(crate) fn handle_wrapper_introspection(
    command: WrapperIntrospection,
    paths: &WrapperPaths,
) -> Result<i32, Box<dyn std::error::Error>> {
    match command {
        WrapperIntrospection::Version => {
            println!("{}", env!("CARGO_PKG_VERSION"));
        }
        WrapperIntrospection::VersionVerbose => {
            let manifest = build_manifest()?;
            println!("product version: {}", manifest.product_version);
            println!("target triple: {}", manifest.artifact_target_triple);
            println!("git commit: {}", manifest.git_commit);
            println!("build profile: {}", manifest.build_profile);
            println!("rustc version: {}", manifest.rustc_version);
            println!("cargo version: {}", manifest.cargo_version);
            println!("build timestamp: {}", manifest.build_timestamp);
            println!("support tier: {}", manifest.support_tier_declaration);
            println!("IR spec version: {}", manifest.ir_spec_version);
            println!("adapter spec version: {}", manifest.adapter_spec_version);
            println!("renderer spec version: {}", manifest.renderer_spec_version);
            println!("install root: {}", paths.install_root.display());
            println!("config path: {}", paths.config_path.display());
        }
        WrapperIntrospection::PrintPaths => {
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({
                    "config_path": paths.config_path,
                    "cache_root": paths.cache_root,
                    "state_root": paths.state_root,
                    "runtime_root": paths.runtime_root,
                    "trace_root": paths.trace_root,
                    "install_root": paths.install_root,
                }))?
            );
        }
        WrapperIntrospection::SelfCheck => {
            println!("{}", serde_json::to_string_pretty(&self_check(paths)?)?);
        }
        WrapperIntrospection::DumpBuildManifest => {
            println!("{}", serde_json::to_string_pretty(&build_manifest()?)?);
        }
    }
    Ok(0)
}

fn build_manifest() -> Result<BuildManifest, Box<dyn std::error::Error>> {
    let workspace_root = workspace_root();
    let lockfile_hash = hash_file(&workspace_root.join("Cargo.lock"))?;
    let vendor_hash = hash_vendor(&workspace_root.join("vendor"))?;
    Ok(default_build_manifest(lockfile_hash, vendor_hash))
}

fn self_check(paths: &WrapperPaths) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
    let manifest = build_manifest()?;
    let mut cache = ProbeCache::default();
    let backend = cache.get_or_probe(ResolveRequest {
        explicit_backend: None,
        env_backend: env::var_os("FORMED_BACKEND_GCC").map(PathBuf::from),
        invoked_as: "gcc-formed".to_string(),
    })?;

    paths.ensure_dirs()?;
    let state_access = probe_write_access(&paths.state_root);
    let runtime_access = probe_write_access(&paths.runtime_root);
    let install_probe_root = existing_probe_root(&paths.install_root);
    let install_access = probe_write_access(&install_probe_root);
    let target_matches_build = manifest.artifact_target_triple == build_target_triple();
    let install_root_includes_target =
        install_root_includes_target(&paths.install_root, &manifest.artifact_target_triple);
    let separated_roots = paths_are_separated(paths);
    let mut warnings = Vec::new();
    if !target_matches_build {
        warnings.push("build target triple diverges from embedded manifest".to_string());
    }
    if !install_root_includes_target {
        warnings.push("install root does not include target triple".to_string());
    }
    if !separated_roots {
        warnings.push("install root overlaps mutable state paths".to_string());
    }
    if state_access.is_err() {
        warnings.push("state root is not writable".to_string());
    }
    if runtime_access.is_err() {
        warnings.push("runtime root is not writable".to_string());
    }
    if install_access.is_err() {
        warnings.push("install root probe ancestor is not writable".to_string());
    }

    Ok(json!({
        "binary": "ok",
        "manifest": {
            "target_triple": manifest.artifact_target_triple,
            "target_triple_matches_build": target_matches_build,
            "support_tier": manifest.support_tier_declaration,
        },
        "paths": {
            "config_path": paths.config_path,
            "cache_root": paths.cache_root,
            "state_root": paths.state_root,
            "runtime_root": paths.runtime_root,
            "trace_root": paths.trace_root,
            "install_root": paths.install_root,
            "install_probe_root": install_probe_root,
            "state_root_access": access_label(&state_access),
            "runtime_root_access": access_label(&runtime_access),
            "install_root_access": access_label(&install_access),
            "install_root_includes_target_triple": install_root_includes_target,
            "separated_from_install_root": separated_roots,
        },
        "backend": {
            "path": backend.resolved_path,
            "version": backend.version_string,
            "support_tier": format!("{:?}", backend.support_tier).to_lowercase(),
        },
        "warnings": warnings,
    }))
}

fn hash_file(path: &Path) -> Result<String, Box<dyn std::error::Error>> {
    let contents = fs::read_to_string(path).unwrap_or_default();
    Ok(diag_core::fingerprint_for(&contents))
}

fn hash_vendor(path: &Path) -> Result<String, Box<dyn std::error::Error>> {
    if !path.exists() {
        return Ok("vendor-missing".to_string());
    }
    let mut entries = Vec::new();
    collect_paths(path, &mut entries)?;
    Ok(diag_core::fingerprint_for(&entries))
}

fn collect_paths(path: &Path, entries: &mut Vec<String>) -> Result<(), Box<dyn std::error::Error>> {
    for entry in fs::read_dir(path)? {
        let entry = entry?;
        let child = entry.path();
        if child.is_dir() {
            collect_paths(&child, entries)?;
        } else {
            entries.push(child.display().to_string());
        }
    }
    Ok(())
}

fn install_root_includes_target(install_root: &Path, target_triple: &str) -> bool {
    if target_triple == "unknown-target" {
        return true;
    }
    install_root
        .components()
        .any(|component| component.as_os_str() == OsStr::new(target_triple))
}

fn existing_probe_root(path: &Path) -> PathBuf {
    let mut current = path;
    loop {
        if current.exists() {
            return current.to_path_buf();
        }
        if let Some(parent) = current.parent() {
            current = parent;
        } else {
            return path.to_path_buf();
        }
    }
}

fn probe_write_access(path: &Path) -> Result<(), std::io::Error> {
    fs::create_dir_all(path)?;
    let probe = path.join(format!(
        ".formed-self-check-{}-{}",
        std::process::id(),
        trace_id()
    ));
    fs::write(&probe, b"ok")?;
    fs::remove_file(probe)?;
    Ok(())
}

fn access_label(result: &Result<(), std::io::Error>) -> &'static str {
    if result.is_ok() { "ok" } else { "error" }
}

fn paths_are_separated(paths: &WrapperPaths) -> bool {
    let install_root = &paths.install_root;
    for mutable_root in [
        &paths.cache_root,
        &paths.state_root,
        &paths.runtime_root,
        &paths.trace_root,
    ] {
        if mutable_root.starts_with(install_root) || install_root.starts_with(mutable_root) {
            return false;
        }
    }
    true
}

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .to_path_buf()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn install_root_check_requires_target_component() {
        assert!(install_root_includes_target(
            Path::new("/home/test/.local/opt/cc-formed/x86_64-unknown-linux-musl"),
            "x86_64-unknown-linux-musl"
        ));
        assert!(!install_root_includes_target(
            Path::new("/home/test/.local/opt/cc-formed"),
            "x86_64-unknown-linux-musl"
        ));
    }

    #[test]
    fn mutable_paths_must_not_overlap_install_root() {
        let separated = WrapperPaths {
            config_path: PathBuf::from("/cfg/config.toml"),
            cache_root: PathBuf::from("/cache"),
            state_root: PathBuf::from("/state"),
            runtime_root: PathBuf::from("/runtime"),
            trace_root: PathBuf::from("/state/traces"),
            install_root: PathBuf::from("/opt/cc-formed/x86_64-unknown-linux-musl"),
        };
        let overlapping = WrapperPaths {
            config_path: PathBuf::from("/cfg/config.toml"),
            cache_root: PathBuf::from("/opt/cc-formed/cache"),
            state_root: PathBuf::from("/state"),
            runtime_root: PathBuf::from("/runtime"),
            trace_root: PathBuf::from("/state/traces"),
            install_root: PathBuf::from("/opt/cc-formed"),
        };

        assert!(paths_are_separated(&separated));
        assert!(!paths_are_separated(&overlapping));
    }
}
