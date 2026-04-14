use crate::args::WrapperIntrospection;
use crate::backend::{env_backend_override, env_launcher_override};
use crate::error::CliError;
use crate::mode::{
    CliCompatibilitySeam, compatibility_scope_notice_for_path, execution_mode_label,
    fallback_reason_label, operator_guidance_for_seam, select_mode_for_seam,
    select_processing_path_for_seam,
};
use diag_backend_probe::{
    ProbeCache, ProcessingPath, ResolveRequest, VersionBand, backend_topology_policy,
};
use diag_capture_runtime::ExecutionMode;
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
) -> Result<i32, CliError> {
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
            println!("maturity label: {}", manifest.maturity_label);
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

fn build_manifest() -> Result<BuildManifest, CliError> {
    let workspace_root = workspace_root();
    let lockfile_hash = hash_file(&workspace_root.join("Cargo.lock"))?;
    let vendor_hash = hash_vendor(&workspace_root.join("vendor"))?;
    Ok(default_build_manifest(lockfile_hash, vendor_hash))
}

fn self_check(paths: &WrapperPaths) -> Result<serde_json::Value, CliError> {
    let manifest = build_manifest()?;
    let mut cache = ProbeCache::default();
    let backend = cache
        .get_or_probe(ResolveRequest {
            cli_backend: None,
            env_backend: env_backend_override(),
            config_backend: None,
            cli_launcher: None,
            env_launcher: env_launcher_override(),
            config_launcher: None,
            invoked_as: "gcc-formed".to_string(),
            wrapper_path: env::current_exe().ok(),
        })
        .map_err(|e| CliError::Backend(e.to_string()))?;
    let operator_guidance = operator_guidance_for_seam(&CliCompatibilitySeam::from_probe(&backend));

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
        "shared_contract": shared_contract_summary(),
        "manifest": {
            "target_triple": manifest.artifact_target_triple,
            "target_triple_matches_build": target_matches_build,
            "maturity_label": manifest.maturity_label,
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
            "launcher_path": backend.execution_topology.launcher_path,
            "version": backend.version_string,
            "version_band": snake_case_label(&backend.version_band()),
            "default_processing_path": snake_case_label(&backend.default_processing_path()),
            "allowed_processing_paths": backend
                .capability_profile()
                .allowed_processing_paths
                .iter()
                .map(snake_case_label)
                .collect::<Vec<_>>(),
            "support_level": snake_case_label(&backend.support_level()),
            "topology_kind": snake_case_label(&backend.execution_topology.kind),
            "topology_policy_version": backend.execution_topology.policy_version,
            "topology_disposition": snake_case_label(&backend.execution_topology.disposition),
            "topology_policy": backend_topology_policy(),
        },
        "operator_guidance": {
            "summary": operator_guidance.summary,
            "representative_limitations": operator_guidance.representative_limitations,
            "actionable_next_steps": operator_guidance.actionable_next_steps,
            "c_first_focus_areas": operator_guidance.c_first_focus_areas,
        },
        "rollout_matrix": {
            "schema_version": 2,
            "cases": rollout_matrix_cases(),
        },
        "warnings": warnings,
    }))
}

fn shared_contract_summary() -> serde_json::Value {
    json!({
        "status": "shared_gcc_9_15_contract",
        "in_scope_version_bands": ["gcc15", "gcc13_14", "gcc9_12"],
        "support_level_labels": ["in_scope", "passthrough_only"],
        "observability_metadata": ["version_band", "processing_path"],
        "parity_note": "VersionBand and ProcessingPath remain observability metadata only inside the shared GCC 9-15 public contract.",
    })
}

fn rollout_matrix_cases() -> Vec<serde_json::Value> {
    [
        (VersionBand::Gcc15, None, None, false),
        (VersionBand::Gcc15, Some(ExecutionMode::Shadow), None, false),
        (VersionBand::Gcc15, Some(ExecutionMode::Passthrough), None, false),
        (VersionBand::Gcc15, Some(ExecutionMode::Render), None, true),
        (VersionBand::Gcc13_14, None, None, false),
        (VersionBand::Gcc13_14, Some(ExecutionMode::Shadow), None, false),
        (VersionBand::Gcc13_14, Some(ExecutionMode::Render), None, false),
        (
            VersionBand::Gcc13_14,
            Some(ExecutionMode::Render),
            Some(ProcessingPath::SingleSinkStructured),
            false,
        ),
        (VersionBand::Gcc13_14, Some(ExecutionMode::Passthrough), None, false),
        (VersionBand::Gcc9_12, None, None, false),
        (VersionBand::Gcc9_12, Some(ExecutionMode::Shadow), None, false),
        (
            VersionBand::Gcc9_12,
            Some(ExecutionMode::Render),
            Some(ProcessingPath::SingleSinkStructured),
            false,
        ),
        (VersionBand::Gcc9_12, Some(ExecutionMode::Passthrough), None, false),
        (VersionBand::Gcc16Plus, None, None, false),
        (VersionBand::Unknown, None, None, false),
    ]
    .into_iter()
    .map(|(version_band, requested_mode, requested_processing_path, hard_conflict)| {
        let compatibility_seam = CliCompatibilitySeam::from_version_band(version_band);
        let decision = select_mode_for_seam(&compatibility_seam, requested_mode, hard_conflict);
        let processing_path = select_processing_path_for_seam(
            &compatibility_seam,
            &decision,
            requested_processing_path,
        )
        .expect("rollout matrix only includes supported processing path requests");
        json!({
            "version_band": snake_case_label(&version_band),
            "requested_mode": requested_mode.map(execution_mode_label),
            "requested_processing_path": requested_processing_path.map(|path| snake_case_label(&path)),
            "hard_conflict": hard_conflict,
            "selected_mode": execution_mode_label(decision.mode),
            "processing_path": snake_case_label(&processing_path),
            "support_level": snake_case_label(&compatibility_seam.support_level()),
            "fallback_reason": decision.fallback_reason.map(fallback_reason_label),
            "scope_notice": compatibility_scope_notice_for_path(
                &compatibility_seam,
                &decision,
                processing_path,
            ),
        })
    })
    .collect()
}

fn snake_case_label<T: serde::Serialize>(value: &T) -> String {
    serde_json::to_value(value)
        .ok()
        .and_then(|value| value.as_str().map(|value| value.to_string()))
        .unwrap_or_else(|| "unknown".to_string())
}

fn hash_file(path: &Path) -> Result<String, CliError> {
    let contents = fs::read_to_string(path)?;
    Ok(diag_core::fingerprint_for(&contents))
}

fn hash_vendor(path: &Path) -> Result<String, CliError> {
    if !path.exists() {
        return Ok("vendor-missing".to_string());
    }
    let mut entries = Vec::new();
    collect_paths(path, &mut entries)?;
    Ok(diag_core::fingerprint_for(&entries))
}

fn collect_paths(path: &Path, entries: &mut Vec<String>) -> Result<(), CliError> {
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

    #[test]
    fn rollout_matrix_covers_in_scope_and_out_of_scope_modes() {
        let cases = rollout_matrix_cases();
        assert!(cases.iter().any(|case| {
            case["version_band"] == "gcc15"
                && case["requested_mode"].is_null()
                && case["selected_mode"] == "render"
                && case["processing_path"] == "dual_sink_structured"
                && case["support_level"] == "in_scope"
                && case["scope_notice"].is_null()
        }));
        assert!(cases.iter().any(|case| {
            case["version_band"] == "gcc13_14"
                && case["requested_mode"].is_null()
                && case["selected_mode"] == "render"
                && case["processing_path"] == "native_text_capture"
                && case["support_level"] == "in_scope"
                && case["fallback_reason"].is_null()
                && case["scope_notice"].is_null()
        }));
        assert!(cases.iter().any(|case| {
            case["version_band"] == "gcc13_14"
                && case["requested_mode"] == "shadow"
                && case["selected_mode"] == "shadow"
                && case["processing_path"] == "native_text_capture"
                && case["support_level"] == "in_scope"
                && case["fallback_reason"] == "shadow_mode"
                && case["scope_notice"]
                    == "gcc-formed: version band=gcc13_14; support level=in_scope; selected mode=shadow; processing path=native_text_capture; fallback reason=shadow_mode; shadow capture is active under the shared GCC 9-15 in-scope contract and emits capability-specific debug metadata without changing the public contract."
        }));
        assert!(cases.iter().any(|case| {
            case["version_band"] == "gcc13_14"
                && case["requested_mode"] == "passthrough"
                && case["selected_mode"] == "passthrough"
                && case["processing_path"] == "passthrough"
                && case["support_level"] == "in_scope"
                && case["fallback_reason"] == "user_opt_out"
                && case["scope_notice"]
                    == "gcc-formed: version band=gcc13_14; support level=in_scope; selected mode=passthrough; processing path=passthrough; fallback reason=user_opt_out; wrapper enrichment was bypassed and conservative raw diagnostics will be preserved."
        }));
        assert!(cases.iter().any(|case| {
            case["version_band"] == "gcc13_14"
                && case["requested_mode"] == "render"
                && case["requested_processing_path"] == "single_sink_structured"
                && case["selected_mode"] == "render"
                && case["processing_path"] == "single_sink_structured"
                && case["support_level"] == "in_scope"
                && case["fallback_reason"].is_null()
                && case["scope_notice"]
                    == "gcc-formed: version band=gcc13_14; support level=in_scope; selected mode=render; processing path=single_sink_structured; explicit structured capture is active and same-run native diagnostics may not be preserved on this backend capability profile."
        }));
        assert!(cases.iter().any(|case| {
            case["version_band"] == "gcc9_12"
                && case["requested_mode"].is_null()
                && case["selected_mode"] == "render"
                && case["processing_path"] == "native_text_capture"
                && case["support_level"] == "in_scope"
                && case["fallback_reason"].is_null()
                && case["scope_notice"].is_null()
        }));
        assert!(cases.iter().any(|case| {
            case["version_band"] == "gcc9_12"
                && case["requested_mode"] == "shadow"
                && case["selected_mode"] == "shadow"
                && case["processing_path"] == "native_text_capture"
                && case["support_level"] == "in_scope"
                && case["fallback_reason"] == "shadow_mode"
                && case["scope_notice"]
                    == "gcc-formed: version band=gcc9_12; support level=in_scope; selected mode=shadow; processing path=native_text_capture; fallback reason=shadow_mode; shadow capture is active under the shared GCC 9-15 in-scope contract and emits capability-specific debug metadata without changing the public contract."
        }));
        assert!(cases.iter().any(|case| {
            case["version_band"] == "gcc9_12"
                && case["requested_mode"] == "render"
                && case["requested_processing_path"] == "single_sink_structured"
                && case["selected_mode"] == "render"
                && case["processing_path"] == "single_sink_structured"
                && case["support_level"] == "in_scope"
                && case["fallback_reason"].is_null()
                && case["scope_notice"]
                    == "gcc-formed: version band=gcc9_12; support level=in_scope; selected mode=render; processing path=single_sink_structured; explicit structured capture is active and same-run native diagnostics may not be preserved on this backend capability profile."
        }));
        assert!(cases.iter().any(|case| {
            case["version_band"] == "gcc9_12"
                && case["requested_mode"] == "passthrough"
                && case["selected_mode"] == "passthrough"
                && case["processing_path"] == "passthrough"
                && case["support_level"] == "in_scope"
                && case["fallback_reason"] == "user_opt_out"
                && case["scope_notice"]
                    == "gcc-formed: version band=gcc9_12; support level=in_scope; selected mode=passthrough; processing path=passthrough; fallback reason=user_opt_out; wrapper enrichment was bypassed and conservative raw diagnostics will be preserved."
        }));
        assert!(cases.iter().any(|case| {
            case["version_band"] == "gcc16_plus"
                && case["requested_mode"].is_null()
                && case["selected_mode"] == "passthrough"
                && case["processing_path"] == "passthrough"
                && case["support_level"] == "passthrough_only"
                && case["fallback_reason"] == "unsupported_version_band"
                && case["scope_notice"]
                    == "gcc-formed: version band=gcc16_plus; support level=passthrough_only; selected mode=passthrough; processing path=passthrough; fallback reason=unsupported_version_band; this compiler version is outside the current GCC 9-15 contract and conservative raw diagnostics will be preserved; operator next step=use raw gcc/g++ or --formed-mode=passthrough until an in-scope VersionBand is confirmed."
        }));
        assert!(cases.iter().any(|case| {
            case["version_band"] == "unknown"
                && case["requested_mode"].is_null()
                && case["selected_mode"] == "passthrough"
                && case["processing_path"] == "passthrough"
                && case["support_level"] == "passthrough_only"
                && case["fallback_reason"] == "unsupported_version_band"
                && case["scope_notice"]
                    == "gcc-formed: version band=unknown; support level=passthrough_only; selected mode=passthrough; processing path=passthrough; fallback reason=unsupported_version_band; this compiler version is outside the current GCC 9-15 contract and conservative raw diagnostics will be preserved; operator next step=use raw gcc/g++ or --formed-mode=passthrough until an in-scope VersionBand is confirmed."
        }));
    }

    #[test]
    fn shared_contract_summary_marks_band_and_path_as_observability_only() {
        let summary = shared_contract_summary();

        assert_eq!(summary["status"], "shared_gcc_9_15_contract");
        assert_eq!(
            summary["in_scope_version_bands"],
            serde_json::json!(["gcc15", "gcc13_14", "gcc9_12"])
        );
        assert_eq!(
            summary["support_level_labels"],
            serde_json::json!(["in_scope", "passthrough_only"])
        );
        assert_eq!(
            summary["observability_metadata"],
            serde_json::json!(["version_band", "processing_path"])
        );
        assert!(
            summary["parity_note"]
                .as_str()
                .is_some_and(|note| note.contains("observability metadata only"))
        );
    }

    #[test]
    fn hash_file_errors_for_missing_inputs() {
        let missing = Path::new("/definitely/missing/Cargo.lock");

        assert!(matches!(hash_file(missing), Err(CliError::Io(_))));
    }
}
