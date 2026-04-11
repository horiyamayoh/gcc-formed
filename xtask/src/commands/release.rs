use clap::ValueEnum;
use diag_trace::{BuildManifest, ChecksumEntry, DEFAULT_PRODUCT_NAME, build_manifest_for_target};
use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::SHASUMS_SIGNATURE_FILE;
use crate::util::fs::copy_dir_recursive;

#[derive(Debug, Clone)]
pub(crate) struct PackageOptions {
    pub(crate) binary: PathBuf,
    pub(crate) debug_binary: Option<PathBuf>,
    pub(crate) target_triple: String,
    pub(crate) out_dir: PathBuf,
    pub(crate) release_channel: String,
    pub(crate) maturity_label: String,
    pub(crate) signing_private_key: Option<PathBuf>,
}

#[derive(Debug)]
pub(crate) struct PackageOutput {
    pub(crate) control_dir: PathBuf,
    pub(crate) primary_archive: PathBuf,
    pub(crate) debug_archive: PathBuf,
    pub(crate) source_archive: PathBuf,
    pub(crate) manifest_path: PathBuf,
    pub(crate) build_info_path: PathBuf,
    pub(crate) shasums_path: PathBuf,
    pub(crate) shasums_signature_path: Option<PathBuf>,
}

#[derive(Debug, Clone)]
pub(crate) struct VendorOptions {
    pub(crate) output_dir: PathBuf,
}

#[derive(Debug)]
pub(crate) struct VendorOutput {
    pub(crate) vendor_dir: PathBuf,
    pub(crate) vendor_hash: String,
}

#[derive(Debug, Clone)]
pub(crate) struct ReleasePublishOptions {
    pub(crate) control_dir: PathBuf,
    pub(crate) repository_root: PathBuf,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct ReleasePublishOutput {
    pub(crate) repository_root: PathBuf,
    pub(crate) target_triple: String,
    pub(crate) version: String,
    pub(crate) control_dir: PathBuf,
    pub(crate) release_metadata_path: PathBuf,
    pub(crate) primary_archive_sha256: String,
    pub(crate) signing_key_id: Option<String>,
    pub(crate) signing_public_key_sha256: Option<String>,
}

#[derive(Debug, Clone)]
pub(crate) struct ReleasePromoteOptions {
    pub(crate) repository_root: PathBuf,
    pub(crate) target_triple: String,
    pub(crate) version: String,
    pub(crate) channel: String,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct ReleasePromoteOutput {
    pub(crate) repository_root: PathBuf,
    pub(crate) target_triple: String,
    pub(crate) version: String,
    pub(crate) channel: String,
    pub(crate) channel_metadata_path: PathBuf,
    pub(crate) primary_archive_sha256: String,
}

#[derive(Debug, Clone)]
pub(crate) struct ReleaseResolveOptions {
    pub(crate) repository_root: PathBuf,
    pub(crate) target_triple: String,
    pub(crate) channel: Option<String>,
    pub(crate) version: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct ReleaseResolveOutput {
    pub(crate) repository_root: PathBuf,
    pub(crate) target_triple: String,
    pub(crate) requested_channel: Option<String>,
    pub(crate) resolved_version: String,
    pub(crate) control_dir: PathBuf,
    pub(crate) primary_archive: PathBuf,
    pub(crate) primary_archive_sha256: String,
    pub(crate) manifest_sha256: String,
    pub(crate) shasums_sha256: String,
    pub(crate) signing_key_id: Option<String>,
    pub(crate) signing_public_key_sha256: Option<String>,
    pub(crate) shasums_signature_path: Option<PathBuf>,
}

#[derive(Debug, Clone)]
pub(crate) struct HermeticReleaseOptions {
    pub(crate) vendor_dir: PathBuf,
    pub(crate) bin: String,
    pub(crate) target_triple: Option<String>,
}

#[derive(Debug)]
pub(crate) struct HermeticReleaseOutput {
    pub(crate) vendor_dir: PathBuf,
    pub(crate) vendor_hash: String,
    pub(crate) bin: String,
    pub(crate) target_triple: Option<String>,
    pub(crate) target_dir: PathBuf,
}

#[derive(Debug, Clone)]
pub(crate) struct InstallOptions {
    pub(crate) control_dir: PathBuf,
    pub(crate) install_root: PathBuf,
    pub(crate) bin_dir: PathBuf,
    pub(crate) expected_signing_key_id: Option<String>,
    pub(crate) expected_signing_public_key_sha256: Option<String>,
    pub(crate) dry_run: bool,
}

#[derive(Debug, Serialize)]
pub(crate) struct InstallOutput {
    pub(crate) install_root: PathBuf,
    pub(crate) bin_dir: PathBuf,
    pub(crate) installed_version: String,
    pub(crate) previous_version: Option<String>,
    pub(crate) signing_key_id: Option<String>,
    pub(crate) signing_public_key_sha256: Option<String>,
    pub(crate) current_path: PathBuf,
    pub(crate) dry_run: bool,
    pub(crate) planned_actions: Vec<PlannedAction>,
}

#[derive(Debug, Clone)]
pub(crate) struct InstallReleaseOptions {
    pub(crate) repository_root: PathBuf,
    pub(crate) target_triple: String,
    pub(crate) install_root: PathBuf,
    pub(crate) bin_dir: PathBuf,
    pub(crate) channel: Option<String>,
    pub(crate) version: Option<String>,
    pub(crate) expected_primary_sha256: Option<String>,
    pub(crate) expected_signing_key_id: Option<String>,
    pub(crate) expected_signing_public_key_sha256: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct InstallReleaseOutput {
    pub(crate) install_root: PathBuf,
    pub(crate) bin_dir: PathBuf,
    pub(crate) installed_version: String,
    pub(crate) previous_version: Option<String>,
    pub(crate) requested_channel: Option<String>,
    pub(crate) resolved_version: String,
    pub(crate) primary_archive_sha256: String,
    pub(crate) signing_key_id: Option<String>,
    pub(crate) signing_public_key_sha256: Option<String>,
    pub(crate) current_path: PathBuf,
}

#[derive(Debug, Clone)]
pub(crate) struct RollbackOptions {
    pub(crate) install_root: PathBuf,
    pub(crate) bin_dir: PathBuf,
    pub(crate) version: String,
    pub(crate) dry_run: bool,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct RollbackOutput {
    pub(crate) install_root: PathBuf,
    pub(crate) active_version: String,
    pub(crate) current_path: PathBuf,
    pub(crate) dry_run: bool,
    pub(crate) planned_actions: Vec<PlannedAction>,
}

#[derive(Debug, Clone)]
pub(crate) struct UninstallOptions {
    pub(crate) install_root: PathBuf,
    pub(crate) bin_dir: PathBuf,
    pub(crate) mode: UninstallMode,
    pub(crate) version: Option<String>,
    pub(crate) state_root: Option<PathBuf>,
    pub(crate) purge_state: bool,
    pub(crate) dry_run: bool,
}

#[derive(Debug, Serialize)]
pub(crate) struct UninstallOutput {
    pub(crate) install_root: PathBuf,
    pub(crate) removed_versions: Vec<String>,
    pub(crate) removed_launchers: Vec<String>,
    pub(crate) purged_state: bool,
    pub(crate) dry_run: bool,
    pub(crate) planned_actions: Vec<PlannedAction>,
}

#[derive(Debug, Clone, Copy, ValueEnum, PartialEq, Eq)]
pub(crate) enum UninstallMode {
    RemoveVersion,
    PurgeInstall,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub(crate) struct PlannedAction {
    pub(crate) action: String,
    pub(crate) path: PathBuf,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) target: Option<PathBuf>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) note: Option<String>,
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum ReleaseSelector<'a> {
    Channel(&'a str),
    Version(&'a str),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct PublishedRelease {
    pub(crate) product_name: String,
    pub(crate) product_version: String,
    pub(crate) target_triple: String,
    #[serde(alias = "support_tier")]
    pub(crate) maturity_label: String,
    pub(crate) artifact_release_channel: String,
    pub(crate) control_dir: String,
    pub(crate) primary_archive_path: String,
    pub(crate) primary_archive_sha256: String,
    pub(crate) debug_archive_path: String,
    pub(crate) debug_archive_sha256: String,
    pub(crate) source_archive_path: String,
    pub(crate) source_archive_sha256: String,
    pub(crate) manifest_path: String,
    pub(crate) manifest_sha256: String,
    pub(crate) build_info_path: String,
    pub(crate) build_info_sha256: String,
    pub(crate) shasums_path: String,
    pub(crate) shasums_sha256: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) shasums_signature_path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) shasums_signature_sha256: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) signing_key_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) signing_public_key_sha256: Option<String>,
    pub(crate) published_unix_ts: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct ReleaseChannelPointer {
    pub(crate) channel: String,
    pub(crate) target_triple: String,
    pub(crate) version: String,
    pub(crate) primary_archive_sha256: String,
    pub(crate) manifest_sha256: String,
    pub(crate) shasums_sha256: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) signing_key_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) signing_public_key_sha256: Option<String>,
    pub(crate) promoted_unix_ts: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct DetachedSignatureEnvelope {
    pub(crate) algorithm: String,
    pub(crate) key_id: String,
    pub(crate) public_key_hex: String,
    pub(crate) signed_path: String,
    pub(crate) signed_sha256: String,
    pub(crate) signature_hex: String,
}

#[derive(Debug, Clone)]
pub(crate) struct VerifiedSignature {
    pub(crate) key_id: String,
    pub(crate) public_key_sha256: String,
}

pub(crate) fn run_package(
    options: PackageOptions,
) -> Result<PackageOutput, Box<dyn std::error::Error>> {
    run_package_at(&workspace_root(), &options)
}

pub(crate) fn run_install(
    options: InstallOptions,
) -> Result<InstallOutput, Box<dyn std::error::Error>> {
    run_install_at(&std::env::current_dir()?, &options)
}

pub(crate) fn run_vendor(
    options: VendorOptions,
) -> Result<VendorOutput, Box<dyn std::error::Error>> {
    run_vendor_at(&workspace_root(), &options)
}

pub(crate) fn run_vendor_at(
    workspace_root: &Path,
    options: &VendorOptions,
) -> Result<VendorOutput, Box<dyn std::error::Error>> {
    let vendor_dir = resolve_workspace_path(workspace_root, &options.output_dir);
    if vendor_dir.exists() {
        fs::remove_dir_all(&vendor_dir)?;
    }
    fs::create_dir_all(&vendor_dir)?;
    let output = Command::new("cargo")
        .current_dir(workspace_root)
        .args([
            "vendor",
            "--quiet",
            "--versioned-dirs",
            vendor_dir
                .to_str()
                .ok_or("vendor directory path was not valid UTF-8")?,
        ])
        .output()?;
    if !output.status.success() {
        return Err(format!(
            "cargo vendor failed for {}: {}",
            vendor_dir.display(),
            String::from_utf8_lossy(&output.stderr)
        )
        .into());
    }
    let vendor_hash = hash_vendor_dir(&vendor_dir)?;
    Ok(VendorOutput {
        vendor_dir,
        vendor_hash,
    })
}

pub(crate) fn run_release_publish(
    options: ReleasePublishOptions,
) -> Result<ReleasePublishOutput, Box<dyn std::error::Error>> {
    run_release_publish_at(&std::env::current_dir()?, &options)
}

pub(crate) fn run_release_publish_at(
    base_dir: &Path,
    options: &ReleasePublishOptions,
) -> Result<ReleasePublishOutput, Box<dyn std::error::Error>> {
    let control_dir = canonicalize_existing_path(base_dir, &options.control_dir, "control dir")?;
    let repository_root = resolve_workspace_path(base_dir, &options.repository_root);
    let manifest = read_build_manifest(&control_dir.join("manifest.json"))?;
    verify_shasums(&control_dir, &control_dir.join("SHA256SUMS"))?;

    let version_name = version_dir_name(&manifest.product_version);
    let version_root = release_version_root(
        &repository_root,
        &manifest.artifact_target_triple,
        &manifest.product_version,
    );
    if version_root.exists() {
        return Err(format!(
            "immutable release version already published: {}",
            version_root.display()
        )
        .into());
    }
    let target_root = release_target_root(&repository_root, &manifest.artifact_target_triple);
    fs::create_dir_all(target_root.join("versions"))?;
    fs::create_dir_all(target_root.join("channels"))?;

    let copied_control_dir = version_root.join("control");
    copy_dir_recursive(&control_dir, &copied_control_dir)?;
    verify_shasums(&copied_control_dir, &copied_control_dir.join("SHA256SUMS"))?;

    let primary_archive = find_primary_archive(&copied_control_dir)?;
    let debug_archive = find_release_archive_by_suffix(&copied_control_dir, ".debug.tar.gz")?;
    let source_archive = find_release_archive_by_suffix(&copied_control_dir, "-source.tar.gz")?;
    let manifest_path = copied_control_dir.join("manifest.json");
    let build_info_path = copied_control_dir.join("build-info.txt");
    let shasums_path = copied_control_dir.join("SHA256SUMS");
    let signature_path = copied_control_dir.join(SHASUMS_SIGNATURE_FILE);
    let signature = read_optional_detached_signature(&signature_path)?;
    let verified_signature = if let Some(signature) = signature.as_ref() {
        Some(verify_detached_signature(
            &shasums_path,
            signature,
            Some(&signature.key_id),
            None,
        )?)
    } else {
        None
    };

    let release = PublishedRelease {
        product_name: manifest.product_name.clone(),
        product_version: manifest.product_version.clone(),
        target_triple: manifest.artifact_target_triple.clone(),
        maturity_label: manifest.maturity_label.clone(),
        artifact_release_channel: manifest.release_channel.clone(),
        control_dir: relative_display(&version_root, &copied_control_dir)?,
        primary_archive_path: relative_display(&version_root, &primary_archive)?,
        primary_archive_sha256: sha256_file(&primary_archive)?,
        debug_archive_path: relative_display(&version_root, &debug_archive)?,
        debug_archive_sha256: sha256_file(&debug_archive)?,
        source_archive_path: relative_display(&version_root, &source_archive)?,
        source_archive_sha256: sha256_file(&source_archive)?,
        manifest_path: relative_display(&version_root, &manifest_path)?,
        manifest_sha256: sha256_file(&manifest_path)?,
        build_info_path: relative_display(&version_root, &build_info_path)?,
        build_info_sha256: sha256_file(&build_info_path)?,
        shasums_path: relative_display(&version_root, &shasums_path)?,
        shasums_sha256: sha256_file(&shasums_path)?,
        shasums_signature_path: signature
            .as_ref()
            .map(|_| relative_display(&version_root, &signature_path))
            .transpose()?,
        shasums_signature_sha256: signature
            .as_ref()
            .map(|_| sha256_file(&signature_path))
            .transpose()?,
        signing_key_id: verified_signature
            .as_ref()
            .map(|signature| signature.key_id.clone()),
        signing_public_key_sha256: verified_signature
            .as_ref()
            .map(|signature| signature.public_key_sha256.clone()),
        published_unix_ts: unix_timestamp_secs()?,
    };
    let release_metadata_path = version_root.join("release.json");
    fs::write(&release_metadata_path, serde_json::to_vec_pretty(&release)?)?;

    Ok(ReleasePublishOutput {
        repository_root,
        target_triple: manifest.artifact_target_triple,
        version: version_name.trim_start_matches('v').to_string(),
        control_dir: copied_control_dir,
        release_metadata_path,
        primary_archive_sha256: release.primary_archive_sha256,
        signing_key_id: release.signing_key_id,
        signing_public_key_sha256: release.signing_public_key_sha256,
    })
}

pub(crate) fn run_release_promote(
    options: ReleasePromoteOptions,
) -> Result<ReleasePromoteOutput, Box<dyn std::error::Error>> {
    run_release_promote_at(&std::env::current_dir()?, &options)
}

pub(crate) fn run_release_promote_at(
    base_dir: &Path,
    options: &ReleasePromoteOptions,
) -> Result<ReleasePromoteOutput, Box<dyn std::error::Error>> {
    ensure_operations_channel(&options.channel)?;
    let repository_root = resolve_workspace_path(base_dir, &options.repository_root);
    let release =
        verify_published_release(&repository_root, &options.target_triple, &options.version)?;
    let channel_metadata_path = release_channel_root(&repository_root, &options.target_triple)
        .join(format!("{}.json", options.channel));
    fs::create_dir_all(
        channel_metadata_path
            .parent()
            .unwrap_or_else(|| Path::new(".")),
    )?;
    let pointer = ReleaseChannelPointer {
        channel: options.channel.clone(),
        target_triple: options.target_triple.clone(),
        version: release.product_version.clone(),
        primary_archive_sha256: release.primary_archive_sha256.clone(),
        manifest_sha256: release.manifest_sha256.clone(),
        shasums_sha256: release.shasums_sha256.clone(),
        signing_key_id: release.signing_key_id.clone(),
        signing_public_key_sha256: release.signing_public_key_sha256.clone(),
        promoted_unix_ts: unix_timestamp_secs()?,
    };
    fs::write(&channel_metadata_path, serde_json::to_vec_pretty(&pointer)?)?;
    Ok(ReleasePromoteOutput {
        repository_root,
        target_triple: options.target_triple.clone(),
        version: release.product_version,
        channel: options.channel.clone(),
        channel_metadata_path,
        primary_archive_sha256: release.primary_archive_sha256,
    })
}

pub(crate) fn run_release_resolve(
    options: ReleaseResolveOptions,
) -> Result<ReleaseResolveOutput, Box<dyn std::error::Error>> {
    run_release_resolve_at(&std::env::current_dir()?, &options)
}

pub(crate) fn run_release_resolve_at(
    base_dir: &Path,
    options: &ReleaseResolveOptions,
) -> Result<ReleaseResolveOutput, Box<dyn std::error::Error>> {
    let repository_root = resolve_workspace_path(base_dir, &options.repository_root);
    let selector = release_selector(options.channel.as_deref(), options.version.as_deref())?;
    let (requested_channel, release) =
        resolve_published_release(&repository_root, &options.target_triple, selector)?;
    let version_root = release_version_root(
        &repository_root,
        &options.target_triple,
        &release.product_version,
    );
    Ok(ReleaseResolveOutput {
        repository_root,
        target_triple: options.target_triple.clone(),
        requested_channel,
        resolved_version: release.product_version.clone(),
        control_dir: version_root.join(&release.control_dir),
        primary_archive: version_root.join(&release.primary_archive_path),
        primary_archive_sha256: release.primary_archive_sha256,
        manifest_sha256: release.manifest_sha256,
        shasums_sha256: release.shasums_sha256,
        signing_key_id: release.signing_key_id.clone(),
        signing_public_key_sha256: release.signing_public_key_sha256.clone(),
        shasums_signature_path: release
            .shasums_signature_path
            .as_ref()
            .map(|path| version_root.join(path)),
    })
}

pub(crate) fn run_install_release(
    options: InstallReleaseOptions,
) -> Result<InstallReleaseOutput, Box<dyn std::error::Error>> {
    run_install_release_at(&std::env::current_dir()?, &options)
}

pub(crate) fn run_install_release_at(
    base_dir: &Path,
    options: &InstallReleaseOptions,
) -> Result<InstallReleaseOutput, Box<dyn std::error::Error>> {
    let repository_root = resolve_workspace_path(base_dir, &options.repository_root);
    let selector = release_selector(options.channel.as_deref(), options.version.as_deref())?;
    let (requested_channel, release) =
        resolve_published_release(&repository_root, &options.target_triple, selector)?;
    if let Some(expected_sha) = options.expected_primary_sha256.as_deref()
        && release.primary_archive_sha256 != expected_sha
    {
        return Err(format!(
            "release checksum mismatch: expected {expected_sha}, got {}",
            release.primary_archive_sha256
        )
        .into());
    }
    let install = run_install_at(
        base_dir,
        &InstallOptions {
            control_dir: release_version_root(
                &repository_root,
                &options.target_triple,
                &release.product_version,
            )
            .join(&release.control_dir),
            install_root: options.install_root.clone(),
            bin_dir: options.bin_dir.clone(),
            expected_signing_key_id: options.expected_signing_key_id.clone(),
            expected_signing_public_key_sha256: options.expected_signing_public_key_sha256.clone(),
            dry_run: false,
        },
    )?;
    Ok(InstallReleaseOutput {
        install_root: install.install_root,
        bin_dir: install.bin_dir,
        installed_version: install.installed_version,
        previous_version: install.previous_version,
        requested_channel,
        resolved_version: release.product_version,
        primary_archive_sha256: release.primary_archive_sha256,
        signing_key_id: install.signing_key_id,
        signing_public_key_sha256: install.signing_public_key_sha256,
        current_path: install.current_path,
    })
}

pub(crate) fn run_hermetic_release_check(
    options: HermeticReleaseOptions,
) -> Result<HermeticReleaseOutput, Box<dyn std::error::Error>> {
    run_hermetic_release_check_at(&workspace_root(), &options)
}

pub(crate) fn run_hermetic_release_check_at(
    workspace_root: &Path,
    options: &HermeticReleaseOptions,
) -> Result<HermeticReleaseOutput, Box<dyn std::error::Error>> {
    let vendor_dir = canonicalize_existing_path(workspace_root, &options.vendor_dir, "vendor dir")?;
    let hermetic_target_dir = workspace_root.join("target/hermetic-release");
    let cargo_home = tempfile::Builder::new()
        .prefix("gcc-formed-cargo-home-")
        .tempdir_in(workspace_root)?;
    let config_path = cargo_home.path().join("config.toml");
    fs::write(
        &config_path,
        vendored_source_config(&vendor_dir, &hermetic_target_dir)?,
    )?;
    let status = Command::new("cargo")
        .current_dir(workspace_root)
        .arg("build")
        .arg("--locked")
        .arg("--offline")
        .arg("--release")
        .arg("--target-dir")
        .arg(&hermetic_target_dir)
        .args(
            options
                .target_triple
                .as_ref()
                .map(|target| vec!["--target".to_string(), target.clone()])
                .unwrap_or_default(),
        )
        .arg("--bin")
        .arg(&options.bin)
        .env("CARGO_HOME", cargo_home.path())
        .status()?;
    if !status.success() {
        return Err("hermetic release build failed".into());
    }
    Ok(HermeticReleaseOutput {
        vendor_dir: vendor_dir.clone(),
        vendor_hash: hash_vendor_dir(&vendor_dir)?,
        bin: options.bin.clone(),
        target_triple: options.target_triple.clone(),
        target_dir: hermetic_target_dir,
    })
}

pub(crate) fn run_install_at(
    base_dir: &Path,
    options: &InstallOptions,
) -> Result<InstallOutput, Box<dyn std::error::Error>> {
    let control_dir = canonicalize_existing_path(base_dir, &options.control_dir, "control dir")?;
    let install_root = resolve_workspace_path(base_dir, &options.install_root);
    let bin_dir = resolve_workspace_path(base_dir, &options.bin_dir);
    let control_manifest = read_build_manifest(&control_dir.join("manifest.json"))?;
    ensure_target_aware_install_root(&install_root, &control_manifest.artifact_target_triple)?;
    verify_shasums(&control_dir, &control_dir.join("SHA256SUMS"))?;
    let verified_signature = verify_release_signature_if_present(
        &control_dir.join("SHA256SUMS"),
        &control_dir.join(SHASUMS_SIGNATURE_FILE),
        options.expected_signing_key_id.as_deref(),
        options.expected_signing_public_key_sha256.as_deref(),
    )?;

    let archive_path = find_primary_archive(&control_dir)?;
    let install_parent = install_root
        .parent()
        .map(PathBuf::from)
        .unwrap_or_else(|| base_dir.to_path_buf());
    fs::create_dir_all(&install_parent)?;
    let staging = tempfile::Builder::new()
        .prefix("gcc-formed-install-")
        .tempdir_in(&install_parent)?;
    extract_tar_archive(&archive_path, staging.path())?;
    let extracted_root = extracted_payload_root(staging.path(), &archive_path)?;
    let staged_manifest = read_build_manifest(&extracted_root.join("manifest.json"))?;
    verify_manifest_alignment(&control_manifest, &staged_manifest)?;
    verify_payload_checksums(&extracted_root, &staged_manifest)?;
    run_staged_self_check(
        &extracted_root.join("bin/gcc-formed"),
        &install_root,
        staging.path(),
    )?;

    let version_name = version_dir_name(&staged_manifest.product_version);
    let version_root = install_root.join(&version_name);
    if version_root.exists() {
        return Err(format!("version already installed at {}", version_root.display()).into());
    }
    let previous_version = current_version_name(&install_root)?;
    let mut planned_actions = vec![
        planned_action(
            "create_dir",
            install_root.clone(),
            None,
            Some("ensure versioned install root exists".to_string()),
        ),
        planned_action(
            "move",
            version_root.clone(),
            Some(extracted_root.clone()),
            Some("promote validated payload into immutable version directory".to_string()),
        ),
        planned_action(
            "swap_symlink",
            install_root.join("current"),
            Some(PathBuf::from(&version_name)),
            Some("activate installed version".to_string()),
        ),
    ];
    planned_actions.extend(planned_launcher_actions(&bin_dir, &install_root)?);
    assert_binary_reports_version(
        &extracted_root.join("bin/gcc-formed"),
        &staged_manifest.product_version,
    )?;
    if !options.dry_run {
        fs::create_dir_all(&install_root)?;
        ensure_launcher_symlinks(&bin_dir, &install_root)?;
        fs::rename(&extracted_root, &version_root)?;
        swap_symlink(
            &install_root.join("current"),
            Path::new(&version_name),
            true,
        )?;
        assert_binary_reports_version(
            &version_root.join("bin/gcc-formed"),
            &staged_manifest.product_version,
        )?;
    }

    Ok(InstallOutput {
        install_root,
        bin_dir: bin_dir.clone(),
        installed_version: staged_manifest.product_version,
        previous_version,
        signing_key_id: verified_signature
            .as_ref()
            .map(|signature| signature.key_id.clone()),
        signing_public_key_sha256: verified_signature
            .as_ref()
            .map(|signature| signature.public_key_sha256.clone()),
        current_path: bin_dir.join("gcc-formed"),
        dry_run: options.dry_run,
        planned_actions,
    })
}

pub(crate) fn run_rollback(
    options: RollbackOptions,
) -> Result<RollbackOutput, Box<dyn std::error::Error>> {
    run_rollback_at(&std::env::current_dir()?, &options)
}

pub(crate) fn run_rollback_at(
    base_dir: &Path,
    options: &RollbackOptions,
) -> Result<RollbackOutput, Box<dyn std::error::Error>> {
    let install_root = resolve_workspace_path(base_dir, &options.install_root);
    let bin_dir = resolve_workspace_path(base_dir, &options.bin_dir);
    let version_name = version_dir_name(&options.version);
    let version_root = install_root.join(&version_name);
    if !version_root.exists() {
        return Err(format!("rollback target not installed: {}", version_root.display()).into());
    }
    assert_binary_reports_version(
        &version_root.join("bin/gcc-formed"),
        version_name.trim_start_matches('v'),
    )?;
    let mut planned_actions = planned_launcher_actions(&bin_dir, &install_root)?;
    planned_actions.push(planned_action(
        "swap_symlink",
        install_root.join("current"),
        Some(PathBuf::from(&version_name)),
        Some("switch active version".to_string()),
    ));
    if !options.dry_run {
        ensure_launcher_symlinks(&bin_dir, &install_root)?;
        swap_symlink(
            &install_root.join("current"),
            Path::new(&version_name),
            true,
        )?;
        assert_binary_reports_version(
            &bin_dir.join("gcc-formed"),
            version_name.trim_start_matches('v'),
        )?;
    }

    Ok(RollbackOutput {
        install_root,
        active_version: version_name.trim_start_matches('v').to_string(),
        current_path: bin_dir.join("gcc-formed"),
        dry_run: options.dry_run,
        planned_actions,
    })
}

pub(crate) fn run_uninstall(
    options: UninstallOptions,
) -> Result<UninstallOutput, Box<dyn std::error::Error>> {
    run_uninstall_at(&std::env::current_dir()?, &options)
}

pub(crate) fn run_uninstall_at(
    base_dir: &Path,
    options: &UninstallOptions,
) -> Result<UninstallOutput, Box<dyn std::error::Error>> {
    let install_root = resolve_workspace_path(base_dir, &options.install_root);
    let bin_dir = resolve_workspace_path(base_dir, &options.bin_dir);
    let state_root = options
        .state_root
        .as_ref()
        .map(|path| resolve_workspace_path(base_dir, path));
    if options.purge_state && !matches!(options.mode, UninstallMode::PurgeInstall) {
        return Err("--purge-state is only supported with purge-install".into());
    }

    let mut removed_versions = Vec::new();
    let mut removed_launchers = Vec::new();
    let mut planned_actions = Vec::new();

    match options.mode {
        UninstallMode::RemoveVersion => {
            let version = options
                .version
                .as_ref()
                .ok_or("remove-version requires --version")?;
            let version_name = version_dir_name(version);
            if current_version_name(&install_root)?.as_deref() == Some(version_name.as_str()) {
                return Err(format!(
                    "refusing to remove active version `{}`; rollback or purge-install first",
                    version_name
                )
                .into());
            }
            let version_root = install_root.join(&version_name);
            if !version_root.exists() {
                return Err(format!("version not installed: {}", version_root.display()).into());
            }
            planned_actions.push(planned_action(
                "remove_dir_all",
                version_root.clone(),
                None,
                Some("remove inactive installed version".to_string()),
            ));
            if !options.dry_run {
                fs::remove_dir_all(&version_root)?;
            }
            removed_versions.push(version_name.trim_start_matches('v').to_string());
        }
        UninstallMode::PurgeInstall => {
            for version_dir in installed_versions(&install_root)? {
                let version_path = install_root.join(&version_dir);
                planned_actions.push(planned_action(
                    "remove_dir_all",
                    version_path.clone(),
                    None,
                    Some("remove installed version".to_string()),
                ));
                if !options.dry_run {
                    fs::remove_dir_all(&version_path)?;
                }
                removed_versions.push(version_dir.trim_start_matches('v').to_string());
            }
            let current_link = install_root.join("current");
            if current_link.exists() || fs::symlink_metadata(&current_link).is_ok() {
                planned_actions.push(planned_action(
                    "remove_path",
                    current_link.clone(),
                    None,
                    Some("remove active-version symlink".to_string()),
                ));
                if !options.dry_run {
                    remove_path_if_exists(&current_link)?;
                }
            }
            let managed_launchers = managed_launcher_paths(&bin_dir, &install_root)?;
            removed_launchers = managed_launchers
                .iter()
                .map(|(launcher, _)| launcher.clone())
                .collect();
            for (_, path) in &managed_launchers {
                planned_actions.push(planned_action(
                    "remove_path",
                    path.clone(),
                    None,
                    Some("remove managed launcher".to_string()),
                ));
            }
            if !options.dry_run {
                removed_launchers = remove_managed_launchers(&bin_dir, &install_root)?;
            }
            if install_root.exists() && fs::read_dir(&install_root)?.next().is_none() {
                planned_actions.push(planned_action(
                    "remove_dir",
                    install_root.clone(),
                    None,
                    Some("remove empty install root".to_string()),
                ));
                if !options.dry_run {
                    fs::remove_dir(&install_root)?;
                }
            }
        }
    }

    let mut purged_state = false;
    if options.purge_state {
        let state_root = state_root.ok_or("--purge-state requires --state-root")?;
        if state_root.exists() {
            planned_actions.push(planned_action(
                "remove_dir_all",
                state_root.clone(),
                None,
                Some("purge persisted state root".to_string()),
            ));
            if !options.dry_run {
                fs::remove_dir_all(&state_root)?;
            }
        }
        purged_state = true;
    }

    Ok(UninstallOutput {
        install_root,
        removed_versions,
        removed_launchers,
        purged_state,
        dry_run: options.dry_run,
        planned_actions,
    })
}

pub(crate) fn run_package_at(
    workspace_root: &Path,
    options: &PackageOptions,
) -> Result<PackageOutput, Box<dyn std::error::Error>> {
    ensure_clean_git_tree(workspace_root)?;
    ensure_release_inputs(workspace_root)?;

    let binary = canonicalize_existing_path(workspace_root, &options.binary, "binary")?;
    let debug_binary = options
        .debug_binary
        .as_ref()
        .map(|path| canonicalize_existing_path(workspace_root, path, "debug binary"))
        .transpose()?
        .unwrap_or_else(|| binary.clone());

    let output_root = resolve_workspace_path(workspace_root, &options.out_dir);
    let artifact_slug = artifact_slug_for_target(&options.target_triple);
    let version = env!("CARGO_PKG_VERSION");
    let package_basename = format!("{DEFAULT_PRODUCT_NAME}-v{version}-{artifact_slug}");
    let debug_basename = format!("{package_basename}.debug");
    let source_basename = format!("{DEFAULT_PRODUCT_NAME}-v{version}-source");
    let control_dir = output_root.join(&package_basename);
    if control_dir.exists() {
        fs::remove_dir_all(&control_dir)?;
    }
    fs::create_dir_all(&control_dir)?;

    let lockfile_hash = sha256_file(&workspace_root.join("Cargo.lock"))?;
    let vendor_hash = hash_vendor_dir(&workspace_root.join("vendor"))?;
    let primary_manifest = build_manifest_for_target(
        lockfile_hash.clone(),
        vendor_hash.clone(),
        &options.target_triple,
        &options.maturity_label,
        &options.release_channel,
    );
    let debug_manifest = build_manifest_for_target(
        lockfile_hash,
        vendor_hash,
        &options.target_triple,
        &options.maturity_label,
        &options.release_channel,
    );

    let staging = tempfile::tempdir()?;
    let primary_root = staging.path().join(&package_basename);
    let debug_root = staging.path().join(&debug_basename);

    let primary_manifest = stage_release_payload(
        workspace_root,
        &primary_root,
        &binary,
        &primary_manifest,
        "primary",
    )?;
    let _debug_manifest = stage_release_payload(
        workspace_root,
        &debug_root,
        &debug_binary,
        &debug_manifest,
        "debug",
    )?;

    let manifest_path = control_dir.join("manifest.json");
    fs::write(
        &manifest_path,
        serde_json::to_vec_pretty(&primary_manifest)?,
    )?;

    let build_info_path = control_dir.join("build-info.txt");
    fs::copy(primary_root.join("build-info.txt"), &build_info_path)?;

    let primary_archive = control_dir.join(format!("{package_basename}.tar.gz"));
    create_tar_archive(staging.path(), &package_basename, &primary_archive)?;
    let debug_archive = control_dir.join(format!("{debug_basename}.tar.gz"));
    create_tar_archive(staging.path(), &debug_basename, &debug_archive)?;

    let source_archive = control_dir.join(format!("{source_basename}.tar.gz"));
    create_source_archive(workspace_root, &source_archive)?;

    let shasums_path = control_dir.join("SHA256SUMS");
    fs::write(
        &shasums_path,
        render_sha256sums(&[
            &primary_archive,
            &debug_archive,
            &source_archive,
            &manifest_path,
            &build_info_path,
        ])?,
    )?;
    let shasums_signature_path =
        if let Some(signing_private_key) = options.signing_private_key.as_ref() {
            Some(write_detached_signature(
                &shasums_path,
                &control_dir.join(SHASUMS_SIGNATURE_FILE),
                signing_private_key,
            )?)
        } else {
            None
        };

    Ok(PackageOutput {
        control_dir,
        primary_archive,
        debug_archive,
        source_archive,
        manifest_path,
        build_info_path,
        shasums_path,
        shasums_signature_path,
    })
}

pub(crate) fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .to_path_buf()
}

pub(crate) fn ensure_clean_git_tree(
    workspace_root: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let output = Command::new("git")
        .current_dir(workspace_root)
        .args(["status", "--porcelain"])
        .output()?;
    if !output.status.success() {
        return Err("failed to inspect git worktree state".into());
    }
    if !output.stdout.is_empty() {
        return Err("release packaging requires a clean git worktree".into());
    }
    Ok(())
}

pub(crate) fn ensure_release_inputs(
    workspace_root: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    for relative in [
        "README.md",
        "docs/releases/RELEASE-NOTES.md",
        "LICENSE",
        "NOTICE",
        "Cargo.lock",
    ] {
        let path = workspace_root.join(relative);
        if !path.exists() {
            return Err(format!("required release input missing: {}", path.display()).into());
        }
    }
    Ok(())
}

pub(crate) fn canonicalize_existing_path(
    workspace_root: &Path,
    path: &Path,
    label: &str,
) -> Result<PathBuf, Box<dyn std::error::Error>> {
    let resolved = resolve_workspace_path(workspace_root, path);
    if !resolved.exists() {
        return Err(format!("{label} does not exist: {}", resolved.display()).into());
    }
    Ok(fs::canonicalize(resolved)?)
}

pub(crate) fn resolve_workspace_path(workspace_root: &Path, path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        workspace_root.join(path)
    }
}

pub(crate) fn release_target_root(repository_root: &Path, target_triple: &str) -> PathBuf {
    repository_root.join("targets").join(target_triple)
}

pub(crate) fn release_channel_root(repository_root: &Path, target_triple: &str) -> PathBuf {
    release_target_root(repository_root, target_triple).join("channels")
}

pub(crate) fn release_version_root(
    repository_root: &Path,
    target_triple: &str,
    version: &str,
) -> PathBuf {
    release_target_root(repository_root, target_triple)
        .join("versions")
        .join(version_dir_name(version))
}

pub(crate) fn release_selector<'a>(
    channel: Option<&'a str>,
    version: Option<&'a str>,
) -> Result<ReleaseSelector<'a>, Box<dyn std::error::Error>> {
    match (channel, version) {
        (Some(_), Some(_)) => Err("specify either --channel or --version, not both".into()),
        (Some(channel), None) => {
            ensure_operations_channel(channel)?;
            Ok(ReleaseSelector::Channel(channel))
        }
        (None, Some(version)) => Ok(ReleaseSelector::Version(version)),
        (None, None) => Err("specify one of --channel or --version".into()),
    }
}

pub(crate) fn ensure_operations_channel(channel: &str) -> Result<(), Box<dyn std::error::Error>> {
    match channel {
        "canary" | "beta" | "stable" => Ok(()),
        _ => Err(format!("unsupported operations channel: {channel}").into()),
    }
}

pub(crate) fn relative_display(
    root: &Path,
    path: &Path,
) -> Result<String, Box<dyn std::error::Error>> {
    Ok(path
        .strip_prefix(root)
        .map_err(|_| format!("{} is not under {}", path.display(), root.display()))?
        .display()
        .to_string()
        .replace('\\', "/"))
}

pub(crate) fn unix_timestamp_secs() -> Result<u64, Box<dyn std::error::Error>> {
    Ok(std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_secs())
}

pub(crate) fn artifact_slug_for_target(target_triple: &str) -> String {
    let descriptor = diag_trace::describe_target(target_triple);
    format!(
        "{}-{}-{}",
        descriptor.os, descriptor.arch, descriptor.libc_family
    )
}

pub(crate) fn stage_release_payload(
    workspace_root: &Path,
    stage_root: &Path,
    binary: &Path,
    manifest_template: &BuildManifest,
    archive_role: &str,
) -> Result<BuildManifest, Box<dyn std::error::Error>> {
    fs::create_dir_all(stage_root.join("bin"))?;
    fs::create_dir_all(stage_root.join("share/doc/gcc-formed"))?;
    fs::create_dir_all(stage_root.join("share/licenses/gcc-formed"))?;

    copy_release_file(binary, &stage_root.join("bin/gcc-formed"))?;
    copy_release_file(binary, &stage_root.join("bin/g++-formed"))?;
    copy_release_file(
        &workspace_root.join("README.md"),
        &stage_root.join("share/doc/gcc-formed/README.md"),
    )?;
    copy_release_file(
        &workspace_root.join("docs/releases/RELEASE-NOTES.md"),
        &stage_root.join("share/doc/gcc-formed/docs/releases/RELEASE-NOTES.md"),
    )?;
    copy_release_file(
        &workspace_root.join("LICENSE"),
        &stage_root.join("share/licenses/gcc-formed/LICENSE"),
    )?;
    copy_release_file(
        &workspace_root.join("NOTICE"),
        &stage_root.join("share/licenses/gcc-formed/NOTICE"),
    )?;

    let build_info_path = stage_root.join("build-info.txt");
    fs::write(
        &build_info_path,
        render_build_info(manifest_template, archive_role, binary),
    )?;

    let mut manifest = manifest_template.clone();
    manifest.checksums = payload_checksums(stage_root)?;
    fs::write(
        stage_root.join("manifest.json"),
        serde_json::to_vec_pretty(&manifest)?,
    )?;

    Ok(manifest)
}

pub(crate) fn copy_release_file(from: &Path, to: &Path) -> Result<(), Box<dyn std::error::Error>> {
    if let Some(parent) = to.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::copy(from, to)?;
    let permissions = fs::metadata(from)?.permissions();
    fs::set_permissions(to, permissions)?;
    Ok(())
}

pub(crate) fn render_build_info(
    manifest: &BuildManifest,
    archive_role: &str,
    binary: &Path,
) -> String {
    let mut text = String::new();
    let _ = writeln!(&mut text, "product: {}", manifest.product_name);
    let _ = writeln!(&mut text, "version: {}", manifest.product_version);
    let _ = writeln!(
        &mut text,
        "artifact target triple: {}",
        manifest.artifact_target_triple
    );
    let _ = writeln!(
        &mut text,
        "artifact platform: {}/{}/{}",
        manifest.artifact_os, manifest.artifact_arch, manifest.artifact_libc_family
    );
    let _ = writeln!(&mut text, "git commit: {}", manifest.git_commit);
    let _ = writeln!(&mut text, "build profile: {}", manifest.build_profile);
    let _ = writeln!(&mut text, "rustc: {}", manifest.rustc_version);
    let _ = writeln!(&mut text, "cargo: {}", manifest.cargo_version);
    let _ = writeln!(&mut text, "build timestamp: {}", manifest.build_timestamp);
    let _ = writeln!(&mut text, "maturity label: {}", manifest.maturity_label);
    let _ = writeln!(&mut text, "release channel: {}", manifest.release_channel);
    let _ = writeln!(&mut text, "archive role: {archive_role}");
    let _ = writeln!(&mut text, "binary source: {}", binary.display());
    let _ = writeln!(&mut text, "lockfile hash: {}", manifest.lockfile_hash);
    let _ = writeln!(&mut text, "vendor hash: {}", manifest.vendor_hash);
    text
}

pub(crate) fn payload_checksums(
    stage_root: &Path,
) -> Result<Vec<ChecksumEntry>, Box<dyn std::error::Error>> {
    let payload_paths = [
        "bin/gcc-formed",
        "bin/g++-formed",
        "share/doc/gcc-formed/README.md",
        "share/doc/gcc-formed/docs/releases/RELEASE-NOTES.md",
        "share/licenses/gcc-formed/LICENSE",
        "share/licenses/gcc-formed/NOTICE",
        "build-info.txt",
    ];
    let mut checksums = Vec::new();
    for relative in payload_paths {
        let path = stage_root.join(relative);
        checksums.push(ChecksumEntry {
            path: relative.to_string(),
            sha256: sha256_file(&path)?,
            size_bytes: fs::metadata(path)?.len(),
        });
    }
    Ok(checksums)
}

pub(crate) fn hash_vendor_dir(path: &Path) -> Result<String, Box<dyn std::error::Error>> {
    if !path.exists() {
        return Ok("vendor-missing".to_string());
    }
    let mut entries = Vec::new();
    collect_paths_for_hash(path, path, &mut entries)?;
    entries.sort();
    Ok(sha256_bytes(entries.join("\n").as_bytes()))
}

pub(crate) fn collect_paths_for_hash(
    root: &Path,
    path: &Path,
    entries: &mut Vec<String>,
) -> Result<(), Box<dyn std::error::Error>> {
    for entry in fs::read_dir(path)? {
        let entry = entry?;
        let child = entry.path();
        if child.is_dir() {
            collect_paths_for_hash(root, &child, entries)?;
        } else {
            let relative = child
                .strip_prefix(root)
                .unwrap_or(&child)
                .display()
                .to_string();
            entries.push(format!("{relative}:{}", sha256_file(&child)?));
        }
    }
    Ok(())
}

pub(crate) fn sha256_file(path: &Path) -> Result<String, Box<dyn std::error::Error>> {
    let bytes = fs::read(path)?;
    Ok(sha256_bytes(&bytes))
}

pub(crate) fn sha256_bytes(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}

pub(crate) fn create_tar_archive(
    root: &Path,
    directory_name: &str,
    output_path: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let status = Command::new("tar")
        .current_dir(root)
        .arg("-czf")
        .arg(output_path)
        .arg(directory_name)
        .status()?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("failed to create tar archive {}", output_path.display()).into())
    }
}

pub(crate) fn create_source_archive(
    workspace_root: &Path,
    output_path: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let status = Command::new("git")
        .current_dir(workspace_root)
        .arg("archive")
        .arg("--format=tar.gz")
        .arg(format!("--output={}", output_path.display()))
        .arg("HEAD")
        .status()?;
    if status.success() {
        Ok(())
    } else {
        Err(format!(
            "failed to create source archive at {}",
            output_path.display()
        )
        .into())
    }
}

pub(crate) fn render_sha256sums(paths: &[&Path]) -> Result<String, Box<dyn std::error::Error>> {
    let mut lines = Vec::new();
    for path in paths {
        lines.push(format!(
            "{}  {}",
            sha256_file(path)?,
            path.file_name()
                .and_then(|value| value.to_str())
                .unwrap_or_default()
        ));
    }
    lines.sort();
    Ok(lines.join("\n") + "\n")
}

pub(crate) fn write_detached_signature(
    signed_path: &Path,
    signature_path: &Path,
    signing_private_key_path: &Path,
) -> Result<PathBuf, Box<dyn std::error::Error>> {
    let signing_key = read_signing_key(signing_private_key_path)?;
    let signed_bytes = fs::read(signed_path)?;
    let verifying_key = signing_key.verifying_key();
    let envelope = DetachedSignatureEnvelope {
        algorithm: "ed25519".to_string(),
        key_id: signing_key_id(&verifying_key),
        public_key_hex: encode_hex(&verifying_key.to_bytes()),
        signed_path: signed_path
            .file_name()
            .and_then(|value| value.to_str())
            .ok_or("signed path filename was not valid UTF-8")?
            .to_string(),
        signed_sha256: sha256_bytes(&signed_bytes),
        signature_hex: encode_hex(&signing_key.sign(&signed_bytes).to_bytes()),
    };
    fs::write(signature_path, serde_json::to_vec_pretty(&envelope)?)?;
    Ok(signature_path.to_path_buf())
}

pub(crate) fn verify_release_signature_if_present(
    signed_path: &Path,
    signature_path: &Path,
    expected_signing_key_id: Option<&str>,
    expected_signing_public_key_sha256: Option<&str>,
) -> Result<Option<VerifiedSignature>, Box<dyn std::error::Error>> {
    let signature = read_optional_detached_signature(signature_path)?;
    match (
        signature,
        expected_signing_key_id,
        expected_signing_public_key_sha256,
    ) {
        (None, None, None) => Ok(None),
        (None, expected_key_id, expected_public_key_sha256) => Err(format!(
            "expected detached signature for {}{}{}",
            signed_path.display(),
            expected_key_id
                .map(|value| format!(" with signing key `{value}`"))
                .unwrap_or_default(),
            expected_public_key_sha256
                .map(|value| format!(" and trusted public key sha256 `{value}`"))
                .unwrap_or_default()
        )
        .into()),
        (Some(envelope), expected_key_id, expected_public_key_sha256) => {
            Ok(Some(verify_detached_signature(
                signed_path,
                &envelope,
                expected_key_id,
                expected_public_key_sha256,
            )?))
        }
    }
}

pub(crate) fn read_optional_detached_signature(
    signature_path: &Path,
) -> Result<Option<DetachedSignatureEnvelope>, Box<dyn std::error::Error>> {
    if signature_path.exists() {
        Ok(Some(read_json_file(signature_path)?))
    } else {
        Ok(None)
    }
}

pub(crate) fn verify_detached_signature(
    signed_path: &Path,
    envelope: &DetachedSignatureEnvelope,
    expected_key_id: Option<&str>,
    expected_public_key_sha256: Option<&str>,
) -> Result<VerifiedSignature, Box<dyn std::error::Error>> {
    if envelope.algorithm != "ed25519" {
        return Err(format!("unsupported signature algorithm: {}", envelope.algorithm).into());
    }
    let expected_name = signed_path
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or("signed path filename was not valid UTF-8")?;
    if envelope.signed_path != expected_name {
        return Err(format!(
            "detached signature targets `{}` but expected `{expected_name}`",
            envelope.signed_path
        )
        .into());
    }
    if let Some(expected_key_id) = expected_key_id
        && envelope.key_id != expected_key_id
    {
        return Err(format!(
            "detached signature key mismatch: expected {expected_key_id}, got {}",
            envelope.key_id
        )
        .into());
    }
    let public_key_bytes = decode_hex(&envelope.public_key_hex, "public key")?;
    let public_key_bytes: [u8; 32] = public_key_bytes
        .try_into()
        .map_err(|_| "public key must contain exactly 32 bytes")?;
    let verifying_key = VerifyingKey::from_bytes(&public_key_bytes)?;
    let derived_key_id = signing_key_id(&verifying_key);
    if derived_key_id != envelope.key_id {
        return Err(format!(
            "detached signature key id mismatch: expected {}, got {derived_key_id}",
            envelope.key_id
        )
        .into());
    }
    let public_key_sha256 = signing_public_key_sha256(&verifying_key);
    if let Some(expected_public_key_sha256) = expected_public_key_sha256
        && public_key_sha256 != expected_public_key_sha256
    {
        return Err(format!(
            "detached signature public key mismatch: expected {expected_public_key_sha256}, got {public_key_sha256}"
        )
        .into());
    }
    let signed_bytes = fs::read(signed_path)?;
    let signed_sha256 = sha256_bytes(&signed_bytes);
    if signed_sha256 != envelope.signed_sha256 {
        return Err(format!(
            "detached signature digest mismatch for {}: expected {}, got {signed_sha256}",
            signed_path.display(),
            envelope.signed_sha256
        )
        .into());
    }
    let signature_bytes = decode_hex(&envelope.signature_hex, "signature")?;
    let signature = Signature::from_slice(&signature_bytes)?;
    verifying_key.verify(&signed_bytes, &signature)?;
    Ok(VerifiedSignature {
        key_id: envelope.key_id.clone(),
        public_key_sha256,
    })
}

pub(crate) fn read_signing_key(path: &Path) -> Result<SigningKey, Box<dyn std::error::Error>> {
    let bytes = decode_hex(&fs::read_to_string(path)?, "private key")?;
    let bytes: [u8; 32] = bytes
        .try_into()
        .map_err(|_| "private key must contain exactly 32 bytes")?;
    Ok(SigningKey::from_bytes(&bytes))
}

pub(crate) fn signing_key_id(verifying_key: &VerifyingKey) -> String {
    format!("ed25519:{}", &sha256_bytes(&verifying_key.to_bytes())[..16])
}

pub(crate) fn signing_public_key_sha256(verifying_key: &VerifyingKey) -> String {
    sha256_bytes(&verifying_key.to_bytes())
}

pub(crate) fn decode_hex(value: &str, label: &str) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let normalized = value.trim();
    if !normalized.len().is_multiple_of(2) {
        return Err(format!("{label} hex must contain an even number of characters").into());
    }
    let mut bytes = Vec::with_capacity(normalized.len() / 2);
    let chars = normalized.as_bytes().chunks_exact(2);
    for chunk in chars {
        let pair = std::str::from_utf8(chunk)?;
        bytes
            .push(u8::from_str_radix(pair, 16).map_err(|error| {
                format!("{label} hex contained invalid byte `{pair}`: {error}")
            })?);
    }
    Ok(bytes)
}

pub(crate) fn encode_hex(bytes: &[u8]) -> String {
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        let _ = write!(&mut output, "{byte:02x}");
    }
    output
}

pub(crate) fn read_json_file<T>(path: &Path) -> Result<T, Box<dyn std::error::Error>>
where
    T: for<'de> Deserialize<'de>,
{
    Ok(serde_json::from_str(&fs::read_to_string(path)?)?)
}

pub(crate) fn read_published_release(
    version_root: &Path,
) -> Result<PublishedRelease, Box<dyn std::error::Error>> {
    read_json_file(&version_root.join("release.json"))
}

pub(crate) fn read_release_channel_pointer(
    repository_root: &Path,
    target_triple: &str,
    channel: &str,
) -> Result<ReleaseChannelPointer, Box<dyn std::error::Error>> {
    read_json_file(
        &release_channel_root(repository_root, target_triple).join(format!("{channel}.json")),
    )
}

pub(crate) fn verify_published_release(
    repository_root: &Path,
    target_triple: &str,
    version: &str,
) -> Result<PublishedRelease, Box<dyn std::error::Error>> {
    let version_root = release_version_root(repository_root, target_triple, version);
    if !version_root.exists() {
        return Err(format!("published version not found: {}", version_root.display()).into());
    }
    let release = read_published_release(&version_root)?;
    if release.target_triple != target_triple {
        return Err(format!(
            "published release target mismatch: expected {target_triple}, got {}",
            release.target_triple
        )
        .into());
    }
    let control_dir = version_root.join(&release.control_dir);
    verify_shasums(&control_dir, &version_root.join(&release.shasums_path))?;

    let primary_archive = version_root.join(&release.primary_archive_path);
    let manifest_path = version_root.join(&release.manifest_path);
    let shasums_path = version_root.join(&release.shasums_path);
    if sha256_file(&primary_archive)? != release.primary_archive_sha256 {
        return Err(format!(
            "published primary archive checksum drifted: {}",
            primary_archive.display()
        )
        .into());
    }
    if sha256_file(&manifest_path)? != release.manifest_sha256 {
        return Err(format!(
            "published manifest checksum drifted: {}",
            manifest_path.display()
        )
        .into());
    }
    if sha256_file(&shasums_path)? != release.shasums_sha256 {
        return Err(format!(
            "published shasums checksum drifted: {}",
            shasums_path.display()
        )
        .into());
    }
    match (
        release.shasums_signature_path.as_ref(),
        release.shasums_signature_sha256.as_ref(),
    ) {
        (Some(signature_path), Some(signature_sha256)) => {
            let signature_path = version_root.join(signature_path);
            let actual_sha256 = sha256_file(&signature_path)?;
            if &actual_sha256 != signature_sha256 {
                return Err(format!(
                    "published signature checksum drifted: {}",
                    signature_path.display()
                )
                .into());
            }
            let signature = read_json_file::<DetachedSignatureEnvelope>(&signature_path)?;
            verify_detached_signature(
                &shasums_path,
                &signature,
                release.signing_key_id.as_deref(),
                release.signing_public_key_sha256.as_deref(),
            )?;
        }
        (None, None) => {}
        _ => {
            return Err("published release signature metadata was incomplete".into());
        }
    }
    Ok(release)
}

pub(crate) fn resolve_published_release(
    repository_root: &Path,
    target_triple: &str,
    selector: ReleaseSelector<'_>,
) -> Result<(Option<String>, PublishedRelease), Box<dyn std::error::Error>> {
    match selector {
        ReleaseSelector::Version(version) => Ok((
            None,
            verify_published_release(repository_root, target_triple, version)?,
        )),
        ReleaseSelector::Channel(channel) => {
            let pointer = read_release_channel_pointer(repository_root, target_triple, channel)?;
            if pointer.target_triple != target_triple {
                return Err(format!(
                    "channel target mismatch: expected {target_triple}, got {}",
                    pointer.target_triple
                )
                .into());
            }
            if pointer.channel != channel {
                return Err(format!(
                    "channel pointer mismatch: expected {channel}, got {}",
                    pointer.channel
                )
                .into());
            }
            let release =
                verify_published_release(repository_root, target_triple, &pointer.version)?;
            if pointer.primary_archive_sha256 != release.primary_archive_sha256 {
                return Err(format!(
                    "channel pointer primary checksum mismatch for {}",
                    pointer.channel
                )
                .into());
            }
            if pointer.manifest_sha256 != release.manifest_sha256 {
                return Err(format!(
                    "channel pointer manifest checksum mismatch for {}",
                    pointer.channel
                )
                .into());
            }
            if pointer.shasums_sha256 != release.shasums_sha256 {
                return Err(format!(
                    "channel pointer shasums checksum mismatch for {}",
                    pointer.channel
                )
                .into());
            }
            if pointer.signing_key_id != release.signing_key_id {
                return Err(format!(
                    "channel pointer signing key mismatch for {}",
                    pointer.channel
                )
                .into());
            }
            if pointer.signing_public_key_sha256 != release.signing_public_key_sha256 {
                return Err(format!(
                    "channel pointer signing public key mismatch for {}",
                    pointer.channel
                )
                .into());
            }
            Ok((Some(channel.to_string()), release))
        }
    }
}

pub(crate) fn vendored_source_config(
    vendor_dir: &Path,
    target_dir: &Path,
) -> Result<String, Box<dyn std::error::Error>> {
    let vendor_dir = vendor_dir
        .to_str()
        .ok_or("vendor directory path was not valid UTF-8")?;
    let target_dir = target_dir
        .to_str()
        .ok_or("target directory path was not valid UTF-8")?;
    let vendor_dir = vendor_dir.replace('\\', "\\\\");
    let target_dir = target_dir.replace('\\', "\\\\");
    Ok(format!(
        "[source.crates-io]\nreplace-with = \"vendored-sources\"\n\n[source.vendored-sources]\ndirectory = \"{vendor_dir}\"\n\n[build]\ntarget-dir = \"{target_dir}\"\n"
    ))
}

pub(crate) fn read_build_manifest(
    path: &Path,
) -> Result<BuildManifest, Box<dyn std::error::Error>> {
    Ok(serde_json::from_str(&fs::read_to_string(path)?)?)
}

pub(crate) fn ensure_target_aware_install_root(
    install_root: &Path,
    target_triple: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let includes_target = install_root
        .components()
        .any(|component| component.as_os_str() == target_triple);
    if includes_target {
        Ok(())
    } else {
        Err(format!(
            "install root must include target triple `{target_triple}`: {}",
            install_root.display()
        )
        .into())
    }
}

pub(crate) fn verify_shasums(
    control_dir: &Path,
    shasums_path: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let contents = fs::read_to_string(shasums_path)?;
    if contents.trim().is_empty() {
        return Err(format!("checksum file is empty: {}", shasums_path.display()).into());
    }
    for line in contents.lines().filter(|line| !line.trim().is_empty()) {
        let (sha256, relative) = line
            .split_once("  ")
            .ok_or_else(|| format!("invalid SHA256SUMS line `{line}`"))?;
        let artifact_path = control_dir.join(relative);
        if !artifact_path.exists() {
            return Err(format!(
                "checksum entry references missing file: {}",
                artifact_path.display()
            )
            .into());
        }
        let actual = sha256_file(&artifact_path)?;
        if actual != sha256 {
            return Err(format!(
                "checksum mismatch for {}: expected {sha256}, got {actual}",
                artifact_path.display()
            )
            .into());
        }
    }
    Ok(())
}

pub(crate) fn find_primary_archive(
    control_dir: &Path,
) -> Result<PathBuf, Box<dyn std::error::Error>> {
    let mut archives = fs::read_dir(control_dir)?
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| {
            path.file_name()
                .and_then(|value| value.to_str())
                .map(|name| {
                    name.ends_with(".tar.gz")
                        && !name.ends_with(".debug.tar.gz")
                        && !name.ends_with("-source.tar.gz")
                })
                .unwrap_or(false)
        })
        .collect::<Vec<_>>();
    archives.sort();
    match archives.as_slice() {
        [archive] => Ok(archive.clone()),
        [] => Err(format!(
            "control dir did not contain a primary archive: {}",
            control_dir.display()
        )
        .into()),
        _ => Err(format!(
            "control dir contained multiple primary archives: {}",
            control_dir.display()
        )
        .into()),
    }
}

pub(crate) fn find_release_archive_by_suffix(
    control_dir: &Path,
    suffix: &str,
) -> Result<PathBuf, Box<dyn std::error::Error>> {
    let mut archives = fs::read_dir(control_dir)?
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| {
            path.file_name()
                .and_then(|value| value.to_str())
                .map(|name| name.ends_with(suffix))
                .unwrap_or(false)
        })
        .collect::<Vec<_>>();
    archives.sort();
    match archives.as_slice() {
        [archive] => Ok(archive.clone()),
        [] => Err(format!(
            "control dir did not contain an archive ending with `{suffix}`: {}",
            control_dir.display()
        )
        .into()),
        _ => Err(format!(
            "control dir contained multiple archives ending with `{suffix}`: {}",
            control_dir.display()
        )
        .into()),
    }
}

pub(crate) fn extract_tar_archive(
    archive_path: &Path,
    destination: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let status = Command::new("tar")
        .arg("-xzf")
        .arg(archive_path)
        .arg("-C")
        .arg(destination)
        .status()?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("failed to extract archive {}", archive_path.display()).into())
    }
}

pub(crate) fn extracted_payload_root(
    staging_root: &Path,
    archive_path: &Path,
) -> Result<PathBuf, Box<dyn std::error::Error>> {
    let archive_name = archive_path
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or("archive filename was not valid UTF-8")?;
    let root_name = archive_name
        .strip_suffix(".tar.gz")
        .ok_or("archive must end with .tar.gz")?;
    let root = staging_root.join(root_name);
    if root.exists() {
        Ok(root)
    } else {
        Err(format!(
            "archive extraction did not materialize expected root {}",
            root.display()
        )
        .into())
    }
}

pub(crate) fn verify_manifest_alignment(
    control_manifest: &BuildManifest,
    staged_manifest: &BuildManifest,
) -> Result<(), Box<dyn std::error::Error>> {
    if control_manifest.product_version != staged_manifest.product_version {
        return Err(format!(
            "manifest version mismatch: control {}, staged {}",
            control_manifest.product_version, staged_manifest.product_version
        )
        .into());
    }
    if control_manifest.artifact_target_triple != staged_manifest.artifact_target_triple {
        return Err(format!(
            "manifest target mismatch: control {}, staged {}",
            control_manifest.artifact_target_triple, staged_manifest.artifact_target_triple
        )
        .into());
    }
    Ok(())
}

pub(crate) fn verify_payload_checksums(
    stage_root: &Path,
    manifest: &BuildManifest,
) -> Result<(), Box<dyn std::error::Error>> {
    for entry in &manifest.checksums {
        let path = stage_root.join(&entry.path);
        if !path.exists() {
            return Err(format!(
                "manifest checksum references missing payload {}",
                path.display()
            )
            .into());
        }
        let actual_sha = sha256_file(&path)?;
        if actual_sha != entry.sha256 {
            return Err(format!(
                "payload checksum mismatch for {}: expected {}, got {}",
                path.display(),
                entry.sha256,
                actual_sha
            )
            .into());
        }
        let size = fs::metadata(&path)?.len();
        if size != entry.size_bytes {
            return Err(format!(
                "payload size mismatch for {}: expected {}, got {}",
                path.display(),
                entry.size_bytes,
                size
            )
            .into());
        }
    }
    Ok(())
}

pub(crate) fn run_staged_self_check(
    binary_path: &Path,
    install_root: &Path,
    staging_root: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let check_root = staging_root.join("self-check");
    let config_dir = check_root.join("config");
    let cache_dir = check_root.join("cache");
    let state_dir = check_root.join("state");
    let runtime_dir = check_root.join("runtime");
    let trace_dir = check_root.join("trace");
    fs::create_dir_all(&check_root)?;
    let output = Command::new(binary_path)
        .arg("--formed-self-check")
        .env("FORMED_INSTALL_ROOT", install_root)
        .env("FORMED_CONFIG_FILE", config_dir.join("config.toml"))
        .env("FORMED_CACHE_DIR", &cache_dir)
        .env("FORMED_STATE_DIR", &state_dir)
        .env("FORMED_RUNTIME_DIR", &runtime_dir)
        .env("FORMED_TRACE_DIR", &trace_dir)
        .output()?;
    if output.status.success() {
        Ok(())
    } else {
        Err(format!(
            "staged self-check failed: {}",
            String::from_utf8_lossy(&output.stderr)
        )
        .into())
    }
}

pub(crate) fn version_dir_name(version: &str) -> String {
    if version.starts_with('v') {
        version.to_string()
    } else {
        format!("v{version}")
    }
}

pub(crate) fn planned_action(
    action: impl Into<String>,
    path: PathBuf,
    target: Option<PathBuf>,
    note: Option<String>,
) -> PlannedAction {
    PlannedAction {
        action: action.into(),
        path,
        target,
        note,
    }
}

pub(crate) fn launcher_specs(bin_dir: &Path, install_root: &Path) -> [(PathBuf, PathBuf); 2] {
    [
        (
            bin_dir.join("gcc-formed"),
            install_root.join("current/bin/gcc-formed"),
        ),
        (
            bin_dir.join("g++-formed"),
            install_root.join("current/bin/g++-formed"),
        ),
    ]
}

pub(crate) fn planned_launcher_actions(
    bin_dir: &Path,
    install_root: &Path,
) -> Result<Vec<PlannedAction>, Box<dyn std::error::Error>> {
    let mut actions = Vec::new();
    if fs::metadata(bin_dir).is_err() {
        actions.push(planned_action(
            "create_dir",
            bin_dir.to_path_buf(),
            None,
            Some("ensure launcher directory exists".to_string()),
        ));
    }
    for (link_path, target) in launcher_specs(bin_dir, install_root) {
        if launcher_needs_refresh(&link_path, &target)? {
            actions.push(planned_action(
                "swap_symlink",
                link_path,
                Some(target),
                Some("refresh managed launcher".to_string()),
            ));
        }
    }
    Ok(actions)
}

pub(crate) fn assert_binary_reports_version(
    binary_path: &Path,
    expected_version: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let output = Command::new(binary_path).arg("--formed-version").output()?;
    if !output.status.success() {
        return Err(format!("binary failed to report version: {}", binary_path.display()).into());
    }
    let actual = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let expected = expected_version.trim_start_matches('v');
    if actual == expected {
        Ok(())
    } else {
        Err(format!(
            "binary version mismatch for {}: expected {}, got {}",
            binary_path.display(),
            expected,
            actual
        )
        .into())
    }
}

pub(crate) fn current_version_name(
    install_root: &Path,
) -> Result<Option<String>, Box<dyn std::error::Error>> {
    let current_link = install_root.join("current");
    if fs::symlink_metadata(&current_link).is_err() {
        return Ok(None);
    }
    let target = fs::read_link(&current_link)?;
    Ok(target
        .file_name()
        .and_then(|value| value.to_str())
        .map(|value| value.to_string()))
}

pub(crate) fn installed_versions(
    install_root: &Path,
) -> Result<Vec<String>, Box<dyn std::error::Error>> {
    if !install_root.exists() {
        return Ok(Vec::new());
    }
    let mut versions = fs::read_dir(install_root)?
        .filter_map(|entry| entry.ok())
        .filter_map(|entry| {
            let path = entry.path();
            let name = path.file_name()?.to_str()?.to_string();
            if path.is_dir() && name.starts_with('v') {
                Some(name)
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    versions.sort();
    Ok(versions)
}

pub(crate) fn ensure_launcher_symlinks(
    bin_dir: &Path,
    install_root: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    fs::create_dir_all(bin_dir)?;
    for (link_path, target) in launcher_specs(bin_dir, install_root) {
        if launcher_needs_refresh(&link_path, &target)? {
            swap_symlink(&link_path, &target, false)?;
        }
    }
    Ok(())
}

pub(crate) fn launcher_needs_refresh(
    launcher_path: &Path,
    expected_target: &Path,
) -> Result<bool, Box<dyn std::error::Error>> {
    let metadata = match fs::symlink_metadata(launcher_path) {
        Ok(metadata) => metadata,
        Err(_) => return Ok(true),
    };
    if !metadata.file_type().is_symlink() {
        return Ok(true);
    }
    let target = fs::read_link(launcher_path)?;
    let resolved = if target.is_absolute() {
        target
    } else {
        launcher_path
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .join(target)
    };
    Ok(resolved != expected_target)
}

pub(crate) fn remove_managed_launchers(
    bin_dir: &Path,
    install_root: &Path,
) -> Result<Vec<String>, Box<dyn std::error::Error>> {
    let managed = managed_launcher_paths(bin_dir, install_root)?;
    let mut removed = Vec::new();
    for (launcher, path) in managed {
        remove_path_if_exists(&path)?;
        removed.push(launcher);
    }
    Ok(removed)
}

pub(crate) fn managed_launcher_paths(
    bin_dir: &Path,
    install_root: &Path,
) -> Result<Vec<(String, PathBuf)>, Box<dyn std::error::Error>> {
    let mut managed = Vec::new();
    for launcher in ["gcc-formed", "g++-formed"] {
        let path = bin_dir.join(launcher);
        if fs::symlink_metadata(&path).is_err() {
            continue;
        }
        if launcher_is_managed(&path, install_root)? {
            managed.push((launcher.to_string(), path));
        }
    }
    Ok(managed)
}

pub(crate) fn launcher_is_managed(
    launcher_path: &Path,
    install_root: &Path,
) -> Result<bool, Box<dyn std::error::Error>> {
    let target = fs::read_link(launcher_path)?;
    let resolved = if target.is_absolute() {
        target
    } else {
        launcher_path
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .join(target)
    };
    Ok(resolved.starts_with(install_root))
}

pub(crate) fn remove_path_if_exists(path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    if let Ok(metadata) = fs::symlink_metadata(path) {
        if metadata.is_dir() && !metadata.file_type().is_symlink() {
            fs::remove_dir_all(path)?;
        } else {
            fs::remove_file(path)?;
        }
    }
    Ok(())
}

pub(crate) fn swap_symlink(
    link_path: &Path,
    target: &Path,
    target_is_dir: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    if let Some(parent) = link_path.parent() {
        fs::create_dir_all(parent)?;
    }
    let link_name = link_path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("link");
    let temp_link = link_path
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join(format!(".{link_name}.tmp"));
    remove_path_if_exists(&temp_link)?;
    create_symlink(target, &temp_link, target_is_dir)?;
    match fs::rename(&temp_link, link_path) {
        Ok(()) => Ok(()),
        Err(_) => {
            remove_path_if_exists(link_path)?;
            fs::rename(&temp_link, link_path)?;
            Ok(())
        }
    }
}

#[cfg(unix)]
pub(crate) fn create_symlink(
    target: &Path,
    link_path: &Path,
    _target_is_dir: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    std::os::unix::fs::symlink(target, link_path)?;
    Ok(())
}

#[cfg(windows)]
pub(crate) fn create_symlink(
    target: &Path,
    link_path: &Path,
    target_is_dir: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    if target_is_dir {
        std::os::windows::fs::symlink_dir(target, link_path)?;
    } else {
        std::os::windows::fs::symlink_file(target, link_path)?;
    }
    Ok(())
}

#[cfg(not(any(unix, windows)))]
pub(crate) fn create_symlink(
    _target: &Path,
    _link_path: &Path,
    _target_is_dir: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    Err("symlink operations are unsupported on this platform".into())
}
