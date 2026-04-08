use clap::{Parser, Subcommand, ValueEnum};
use diag_adapter_gcc::{ingest, producer_for_version, tool_for_backend};
use diag_core::{
    ArtifactKind, ArtifactStorage, CaptureArtifact, DiagnosticDocument, LanguageMode, RunInfo,
    SnapshotKind, WrapperSurface, snapshot_json,
};
use diag_enrich::enrich_document;
use diag_render::{
    DebugRefs, PathPolicy, RenderCapabilities, RenderProfile, RenderRequest, SourceExcerptPolicy,
    StreamKind, TypeDisplayPolicy, WarningVisibility, build_view_model, render,
};
use diag_testkit::{
    ExpectedFallback, Fixture, RenderProfileExpectations, discover, family_counts, validate_fixture,
};
use diag_trace::{BuildManifest, ChecksumEntry, DEFAULT_PRODUCT_NAME, build_manifest_for_target};
use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Instant;

const REPRESENTATIVE_FIXTURES: &[&str] = &[
    "c/syntax/case-01",
    "c/type/case-01",
    "cpp/overload/case-01",
    "cpp/template/case-01",
    "c/macro_include/case-01",
    "c/linker/case-01",
    "c/partial/case-01",
];

const SHASUMS_SIGNATURE_FILE: &str = "SHA256SUMS.sig";

#[derive(Debug, Parser)]
#[command(name = "xtask")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, Subcommand)]
enum Commands {
    Check,
    HermeticReleaseCheck {
        #[arg(long, default_value = "vendor")]
        vendor_dir: PathBuf,
        #[arg(long, default_value = "gcc-formed")]
        bin: String,
        #[arg(long)]
        target_triple: Option<String>,
    },
    InstallRelease {
        #[arg(long)]
        repository_root: PathBuf,
        #[arg(long)]
        target_triple: String,
        #[arg(long)]
        install_root: PathBuf,
        #[arg(long)]
        bin_dir: PathBuf,
        #[arg(long)]
        channel: Option<String>,
        #[arg(long)]
        version: Option<String>,
        #[arg(long)]
        expected_primary_sha256: Option<String>,
        #[arg(long)]
        expected_signing_key_id: Option<String>,
        #[arg(long)]
        expected_signing_public_key_sha256: Option<String>,
    },
    Install {
        #[arg(long)]
        control_dir: PathBuf,
        #[arg(long)]
        install_root: PathBuf,
        #[arg(long)]
        bin_dir: PathBuf,
        #[arg(long)]
        expected_signing_key_id: Option<String>,
        #[arg(long)]
        expected_signing_public_key_sha256: Option<String>,
    },
    Package {
        #[arg(long)]
        binary: PathBuf,
        #[arg(long)]
        debug_binary: Option<PathBuf>,
        #[arg(long)]
        target_triple: String,
        #[arg(long, default_value = "dist")]
        out_dir: PathBuf,
        #[arg(long, default_value = "stable")]
        release_channel: String,
        #[arg(long, default_value = "gcc15_primary")]
        support_tier: String,
        #[arg(long)]
        signing_private_key: Option<PathBuf>,
    },
    ReleasePromote {
        #[arg(long)]
        repository_root: PathBuf,
        #[arg(long)]
        target_triple: String,
        #[arg(long)]
        version: String,
        #[arg(long)]
        channel: String,
    },
    ReleasePublish {
        #[arg(long)]
        control_dir: PathBuf,
        #[arg(long)]
        repository_root: PathBuf,
    },
    ReleaseResolve {
        #[arg(long)]
        repository_root: PathBuf,
        #[arg(long)]
        target_triple: String,
        #[arg(long)]
        channel: Option<String>,
        #[arg(long)]
        version: Option<String>,
    },
    Rollback {
        #[arg(long)]
        install_root: PathBuf,
        #[arg(long)]
        bin_dir: PathBuf,
        #[arg(long)]
        version: String,
    },
    Uninstall {
        #[arg(long)]
        install_root: PathBuf,
        #[arg(long)]
        bin_dir: PathBuf,
        #[arg(long, value_enum)]
        mode: UninstallMode,
        #[arg(long)]
        version: Option<String>,
        #[arg(long)]
        state_root: Option<PathBuf>,
        #[arg(long)]
        purge_state: bool,
    },
    Vendor {
        #[arg(long, default_value = "vendor")]
        output_dir: PathBuf,
    },
    Replay {
        #[arg(long, default_value = "corpus")]
        root: PathBuf,
        #[arg(long)]
        fixture: Option<String>,
        #[arg(long)]
        family: Option<String>,
    },
    Snapshot {
        #[arg(long, default_value = "corpus")]
        root: PathBuf,
        #[arg(long)]
        fixture: Option<String>,
        #[arg(long)]
        family: Option<String>,
        #[arg(long, value_enum, default_value_t = SnapshotSubset::All)]
        subset: SnapshotSubset,
        #[arg(long)]
        check: bool,
        #[arg(long, default_value = "gcc:15")]
        docker_image: String,
    },
    BenchSmoke,
    SelfCheck,
}

#[derive(Debug, Clone, Copy, ValueEnum, PartialEq, Eq)]
enum SnapshotSubset {
    All,
    Representative,
}

#[derive(Debug)]
struct VerificationFailure {
    layer: String,
    fixture_id: String,
    summary: String,
}

#[derive(Debug)]
struct CapturedIngress {
    stderr_text: String,
    sarif_text: String,
}

#[derive(Debug, Clone)]
struct PackageOptions {
    binary: PathBuf,
    debug_binary: Option<PathBuf>,
    target_triple: String,
    out_dir: PathBuf,
    release_channel: String,
    support_tier: String,
    signing_private_key: Option<PathBuf>,
}

#[derive(Debug)]
struct PackageOutput {
    control_dir: PathBuf,
    primary_archive: PathBuf,
    debug_archive: PathBuf,
    source_archive: PathBuf,
    manifest_path: PathBuf,
    build_info_path: PathBuf,
    shasums_path: PathBuf,
    shasums_signature_path: Option<PathBuf>,
}

#[derive(Debug, Clone)]
struct VendorOptions {
    output_dir: PathBuf,
}

#[derive(Debug)]
struct VendorOutput {
    vendor_dir: PathBuf,
    vendor_hash: String,
}

#[derive(Debug, Clone)]
struct ReleasePublishOptions {
    control_dir: PathBuf,
    repository_root: PathBuf,
}

#[derive(Debug)]
struct ReleasePublishOutput {
    repository_root: PathBuf,
    target_triple: String,
    version: String,
    control_dir: PathBuf,
    release_metadata_path: PathBuf,
    primary_archive_sha256: String,
    signing_key_id: Option<String>,
    signing_public_key_sha256: Option<String>,
}

#[derive(Debug, Clone)]
struct ReleasePromoteOptions {
    repository_root: PathBuf,
    target_triple: String,
    version: String,
    channel: String,
}

#[derive(Debug)]
struct ReleasePromoteOutput {
    repository_root: PathBuf,
    target_triple: String,
    version: String,
    channel: String,
    channel_metadata_path: PathBuf,
    primary_archive_sha256: String,
}

#[derive(Debug, Clone)]
struct ReleaseResolveOptions {
    repository_root: PathBuf,
    target_triple: String,
    channel: Option<String>,
    version: Option<String>,
}

#[derive(Debug)]
struct ReleaseResolveOutput {
    repository_root: PathBuf,
    target_triple: String,
    requested_channel: Option<String>,
    resolved_version: String,
    control_dir: PathBuf,
    primary_archive: PathBuf,
    primary_archive_sha256: String,
    manifest_sha256: String,
    shasums_sha256: String,
    signing_key_id: Option<String>,
    signing_public_key_sha256: Option<String>,
    shasums_signature_path: Option<PathBuf>,
}

#[derive(Debug, Clone)]
struct HermeticReleaseOptions {
    vendor_dir: PathBuf,
    bin: String,
    target_triple: Option<String>,
}

#[derive(Debug)]
struct HermeticReleaseOutput {
    vendor_dir: PathBuf,
    vendor_hash: String,
    bin: String,
    target_triple: Option<String>,
    target_dir: PathBuf,
}

#[derive(Debug, Clone)]
struct InstallOptions {
    control_dir: PathBuf,
    install_root: PathBuf,
    bin_dir: PathBuf,
    expected_signing_key_id: Option<String>,
    expected_signing_public_key_sha256: Option<String>,
}

#[derive(Debug)]
struct InstallOutput {
    install_root: PathBuf,
    bin_dir: PathBuf,
    installed_version: String,
    previous_version: Option<String>,
    signing_key_id: Option<String>,
    signing_public_key_sha256: Option<String>,
    current_path: PathBuf,
}

#[derive(Debug, Clone)]
struct InstallReleaseOptions {
    repository_root: PathBuf,
    target_triple: String,
    install_root: PathBuf,
    bin_dir: PathBuf,
    channel: Option<String>,
    version: Option<String>,
    expected_primary_sha256: Option<String>,
    expected_signing_key_id: Option<String>,
    expected_signing_public_key_sha256: Option<String>,
}

#[derive(Debug)]
struct InstallReleaseOutput {
    install_root: PathBuf,
    bin_dir: PathBuf,
    installed_version: String,
    previous_version: Option<String>,
    requested_channel: Option<String>,
    resolved_version: String,
    primary_archive_sha256: String,
    signing_key_id: Option<String>,
    signing_public_key_sha256: Option<String>,
    current_path: PathBuf,
}

#[derive(Debug, Clone)]
struct RollbackOptions {
    install_root: PathBuf,
    bin_dir: PathBuf,
    version: String,
}

#[derive(Debug)]
struct RollbackOutput {
    install_root: PathBuf,
    active_version: String,
    current_path: PathBuf,
}

#[derive(Debug, Clone)]
struct UninstallOptions {
    install_root: PathBuf,
    bin_dir: PathBuf,
    mode: UninstallMode,
    version: Option<String>,
    state_root: Option<PathBuf>,
    purge_state: bool,
}

#[derive(Debug)]
struct UninstallOutput {
    install_root: PathBuf,
    removed_versions: Vec<String>,
    removed_launchers: Vec<String>,
    purged_state: bool,
}

#[derive(Debug, Clone, Copy, ValueEnum, PartialEq, Eq)]
enum UninstallMode {
    RemoveVersion,
    PurgeInstall,
}

#[derive(Debug, Clone, Copy)]
enum ReleaseSelector<'a> {
    Channel(&'a str),
    Version(&'a str),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PublishedRelease {
    product_name: String,
    product_version: String,
    target_triple: String,
    support_tier: String,
    artifact_release_channel: String,
    control_dir: String,
    primary_archive_path: String,
    primary_archive_sha256: String,
    debug_archive_path: String,
    debug_archive_sha256: String,
    source_archive_path: String,
    source_archive_sha256: String,
    manifest_path: String,
    manifest_sha256: String,
    build_info_path: String,
    build_info_sha256: String,
    shasums_path: String,
    shasums_sha256: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    shasums_signature_path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    shasums_signature_sha256: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    signing_key_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    signing_public_key_sha256: Option<String>,
    published_unix_ts: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ReleaseChannelPointer {
    channel: String,
    target_triple: String,
    version: String,
    primary_archive_sha256: String,
    manifest_sha256: String,
    shasums_sha256: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    signing_key_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    signing_public_key_sha256: Option<String>,
    promoted_unix_ts: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct DetachedSignatureEnvelope {
    algorithm: String,
    key_id: String,
    public_key_hex: String,
    signed_path: String,
    signed_sha256: String,
    signature_hex: String,
}

#[derive(Debug, Clone)]
struct VerifiedSignature {
    key_id: String,
    public_key_sha256: String,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();
    match cli.command {
        Commands::Check => {
            run("cargo", &["fmt", "--check"])?;
            run("cargo", &["test", "--workspace"])?;
        }
        Commands::HermeticReleaseCheck {
            vendor_dir,
            bin,
            target_triple,
        } => {
            let check = run_hermetic_release_check(HermeticReleaseOptions {
                vendor_dir,
                bin,
                target_triple,
            })?;
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({
                    "vendor_dir": check.vendor_dir,
                    "vendor_hash": check.vendor_hash,
                    "bin": check.bin,
                    "target_triple": check.target_triple,
                    "target_dir": check.target_dir,
                }))?
            );
        }
        Commands::InstallRelease {
            repository_root,
            target_triple,
            install_root,
            bin_dir,
            channel,
            version,
            expected_primary_sha256,
            expected_signing_key_id,
            expected_signing_public_key_sha256,
        } => {
            let install = run_install_release(InstallReleaseOptions {
                repository_root,
                target_triple,
                install_root,
                bin_dir,
                channel,
                version,
                expected_primary_sha256,
                expected_signing_key_id,
                expected_signing_public_key_sha256,
            })?;
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({
                    "install_root": install.install_root,
                    "bin_dir": install.bin_dir,
                    "installed_version": install.installed_version,
                    "previous_version": install.previous_version,
                    "requested_channel": install.requested_channel,
                    "resolved_version": install.resolved_version,
                    "primary_archive_sha256": install.primary_archive_sha256,
                    "signing_key_id": install.signing_key_id,
                    "signing_public_key_sha256": install.signing_public_key_sha256,
                    "current_path": install.current_path,
                }))?
            );
        }
        Commands::Install {
            control_dir,
            install_root,
            bin_dir,
            expected_signing_key_id,
            expected_signing_public_key_sha256,
        } => {
            let install = run_install(InstallOptions {
                control_dir,
                install_root,
                bin_dir,
                expected_signing_key_id,
                expected_signing_public_key_sha256,
            })?;
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({
                    "install_root": install.install_root,
                    "bin_dir": install.bin_dir,
                    "installed_version": install.installed_version,
                    "previous_version": install.previous_version,
                    "signing_key_id": install.signing_key_id,
                    "signing_public_key_sha256": install.signing_public_key_sha256,
                    "current_path": install.current_path,
                }))?
            );
        }
        Commands::Package {
            binary,
            debug_binary,
            target_triple,
            out_dir,
            release_channel,
            support_tier,
            signing_private_key,
        } => {
            let package = run_package(PackageOptions {
                binary,
                debug_binary,
                target_triple,
                out_dir,
                release_channel,
                support_tier,
                signing_private_key,
            })?;
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({
                    "control_dir": package.control_dir,
                    "primary_archive": package.primary_archive,
                    "debug_archive": package.debug_archive,
                    "source_archive": package.source_archive,
                    "manifest_path": package.manifest_path,
                    "build_info_path": package.build_info_path,
                    "shasums_path": package.shasums_path,
                    "shasums_signature_path": package.shasums_signature_path,
                }))?
            );
        }
        Commands::ReleasePromote {
            repository_root,
            target_triple,
            version,
            channel,
        } => {
            let promote = run_release_promote(ReleasePromoteOptions {
                repository_root,
                target_triple,
                version,
                channel,
            })?;
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({
                    "repository_root": promote.repository_root,
                    "target_triple": promote.target_triple,
                    "version": promote.version,
                    "channel": promote.channel,
                    "channel_metadata_path": promote.channel_metadata_path,
                    "primary_archive_sha256": promote.primary_archive_sha256,
                }))?
            );
        }
        Commands::ReleasePublish {
            control_dir,
            repository_root,
        } => {
            let publish = run_release_publish(ReleasePublishOptions {
                control_dir,
                repository_root,
            })?;
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({
                    "repository_root": publish.repository_root,
                    "target_triple": publish.target_triple,
                    "version": publish.version,
                    "control_dir": publish.control_dir,
                    "release_metadata_path": publish.release_metadata_path,
                    "primary_archive_sha256": publish.primary_archive_sha256,
                    "signing_key_id": publish.signing_key_id,
                    "signing_public_key_sha256": publish.signing_public_key_sha256,
                }))?
            );
        }
        Commands::ReleaseResolve {
            repository_root,
            target_triple,
            channel,
            version,
        } => {
            let release = run_release_resolve(ReleaseResolveOptions {
                repository_root,
                target_triple,
                channel,
                version,
            })?;
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({
                    "repository_root": release.repository_root,
                    "target_triple": release.target_triple,
                    "requested_channel": release.requested_channel,
                    "resolved_version": release.resolved_version,
                    "control_dir": release.control_dir,
                    "primary_archive": release.primary_archive,
                    "primary_archive_sha256": release.primary_archive_sha256,
                    "manifest_sha256": release.manifest_sha256,
                    "shasums_sha256": release.shasums_sha256,
                    "signing_key_id": release.signing_key_id,
                    "signing_public_key_sha256": release.signing_public_key_sha256,
                    "shasums_signature_path": release.shasums_signature_path,
                }))?
            );
        }
        Commands::Rollback {
            install_root,
            bin_dir,
            version,
        } => {
            let rollback = run_rollback(RollbackOptions {
                install_root,
                bin_dir,
                version,
            })?;
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({
                    "install_root": rollback.install_root,
                    "active_version": rollback.active_version,
                    "current_path": rollback.current_path,
                }))?
            );
        }
        Commands::Uninstall {
            install_root,
            bin_dir,
            mode,
            version,
            state_root,
            purge_state,
        } => {
            let uninstall = run_uninstall(UninstallOptions {
                install_root,
                bin_dir,
                mode,
                version,
                state_root,
                purge_state,
            })?;
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({
                    "install_root": uninstall.install_root,
                    "removed_versions": uninstall.removed_versions,
                    "removed_launchers": uninstall.removed_launchers,
                    "purged_state": uninstall.purged_state,
                }))?
            );
        }
        Commands::Vendor { output_dir } => {
            let vendor = run_vendor(VendorOptions { output_dir })?;
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({
                    "vendor_dir": vendor.vendor_dir,
                    "vendor_hash": vendor.vendor_hash,
                }))?
            );
        }
        Commands::Replay {
            root,
            fixture,
            family,
        } => run_replay(&root, fixture.as_deref(), family.as_deref())?,
        Commands::Snapshot {
            root,
            fixture,
            family,
            subset,
            check,
            docker_image,
        } => run_snapshot(
            &root,
            fixture.as_deref(),
            family.as_deref(),
            subset,
            check,
            &docker_image,
        )?,
        Commands::BenchSmoke => {
            println!(
                "{}",
                serde_json::json!({
                    "success_path_p95_ms_target": 40,
                    "simple_failure_p95_ms_target": 80,
                    "template_heavy_p95_ms_target": 250
                })
            );
        }
        Commands::SelfCheck => {
            println!(
                "{}",
                serde_json::json!({
                    "workspace": "ok",
                    "toolchain": "managed via rust-toolchain.toml",
                    "corpus_root": "corpus"
                })
            );
        }
    }
    Ok(())
}

fn run_replay(
    root: &Path,
    fixture_filter: Option<&str>,
    family_filter: Option<&str>,
) -> Result<(), Box<dyn std::error::Error>> {
    let fixtures = discover(root)?;
    for fixture in &fixtures {
        validate_fixture(fixture)?;
    }
    let counts = family_counts(&fixtures);
    enforce_minimum_family_counts(&counts)?;
    let selected = select_fixtures(
        &fixtures,
        fixture_filter,
        family_filter,
        SnapshotSubset::All,
    );
    if selected.is_empty() {
        return Err("no fixtures matched replay selection".into());
    }

    let mut failures = Vec::new();
    let mut promoted_verified = 0usize;
    for fixture in &selected {
        if fixture.is_promoted() {
            match verify_promoted_fixture(fixture) {
                Ok(_) => promoted_verified += 1,
                Err(failure) => failures.push(failure),
            }
        }
    }

    if !failures.is_empty() {
        report_failures("replay", &failures);
        return Err("replay verification failed".into());
    }

    println!(
        "{}",
        serde_json::to_string_pretty(&serde_json::json!({
            "family_counts": counts,
            "selected_fixture_count": selected.len(),
            "promoted_verified": promoted_verified,
            "mode": "replay"
        }))?
    );
    Ok(())
}

fn run_snapshot(
    root: &Path,
    fixture_filter: Option<&str>,
    family_filter: Option<&str>,
    subset: SnapshotSubset,
    check: bool,
    docker_image: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let fixtures = discover(root)?;
    let selected = select_fixtures(&fixtures, fixture_filter, family_filter, subset);
    if selected.is_empty() {
        return Err("no fixtures matched snapshot selection".into());
    }

    let promoted = selected
        .iter()
        .copied()
        .filter(|fixture| fixture.is_promoted())
        .collect::<Vec<_>>();
    if promoted.is_empty() {
        return Err("snapshot selection did not include any promoted fixtures".into());
    }

    let mut failures = Vec::new();
    let mut updated = 0usize;
    for fixture in promoted {
        if let Err(error) = validate_snapshot_inputs(fixture) {
            failures.push(VerificationFailure {
                layer: "fixture_layout".to_string(),
                fixture_id: fixture.fixture_id().to_string(),
                summary: error.to_string(),
            });
            continue;
        }
        match materialize_fixture_snapshots(fixture, docker_image, check) {
            Ok(count) => updated += count,
            Err(failure) => failures.push(failure),
        }
    }

    if !failures.is_empty() {
        report_failures("snapshot", &failures);
        return Err("snapshot update/check failed".into());
    }

    println!(
        "{}",
        serde_json::to_string_pretty(&serde_json::json!({
            "selected_fixture_count": selected.len(),
            "promoted_fixture_count": updated,
            "check_only": check,
            "subset": match subset {
                SnapshotSubset::All => "all",
                SnapshotSubset::Representative => "representative",
            },
            "docker_image": docker_image
        }))?
    );
    Ok(())
}

fn verify_promoted_fixture(fixture: &Fixture) -> Result<(), VerificationFailure> {
    let replay = replay_fixture_document(fixture).map_err(|error| VerificationFailure {
        layer: "ingest".to_string(),
        fixture_id: fixture.fixture_id().to_string(),
        summary: error.to_string(),
    })?;
    replay
        .document
        .validate()
        .map_err(|error| VerificationFailure {
            layer: "schema_validation".to_string(),
            fixture_id: fixture.fixture_id().to_string(),
            summary: error.errors.join("; "),
        })?;

    compare_snapshot_file(
        fixture,
        "ir.facts",
        &fixture.snapshot_root().join("ir.facts.json"),
        &snapshot_json(&replay.document, SnapshotKind::FactsOnly).map_err(|error| {
            VerificationFailure {
                layer: "ir.facts".to_string(),
                fixture_id: fixture.fixture_id().to_string(),
                summary: error.to_string(),
            }
        })?,
    )?;
    compare_snapshot_file(
        fixture,
        "ir.analysis",
        &fixture.snapshot_root().join("ir.analysis.json"),
        &snapshot_json(&replay.document, SnapshotKind::AnalysisIncluded).map_err(|error| {
            VerificationFailure {
                layer: "ir.analysis".to_string(),
                fixture_id: fixture.fixture_id().to_string(),
                summary: error.to_string(),
            }
        })?,
    )?;

    let default_request =
        render_request_for_fixture(fixture, &replay.document, RenderProfile::Default);
    let default_render_start = Instant::now();
    let default_render_result =
        render(default_request.clone()).map_err(|error| VerificationFailure {
            layer: "render.default".to_string(),
            fixture_id: fixture.fixture_id().to_string(),
            summary: error.to_string(),
        })?;
    let default_render_time_ms = elapsed_ms(default_render_start);
    let default_view_model = build_view_model(&default_request);
    let lead_node = lead_node_for_document(
        &replay.document,
        &default_render_result.displayed_group_refs,
    )
    .ok_or_else(|| VerificationFailure {
        layer: "semantic".to_string(),
        fixture_id: fixture.fixture_id().to_string(),
        summary: "default render produced no lead diagnostic".to_string(),
    })?;
    verify_semantic_expectations(fixture, &replay.document, lead_node, &default_render_result)?;

    for (profile_name, expectations) in fixture.expectations.render.named_profiles() {
        let profile =
            render_profile_from_name(profile_name).ok_or_else(|| VerificationFailure {
                layer: "render".to_string(),
                fixture_id: fixture.fixture_id().to_string(),
                summary: format!("unknown snapshot profile `{profile_name}`"),
            })?;
        let request = render_request_for_fixture(fixture, &replay.document, profile);
        let view_model = if matches!(profile, RenderProfile::Default) {
            default_view_model.clone()
        } else {
            build_view_model(&request)
        };
        let render_result = if matches!(profile, RenderProfile::Default) {
            default_render_result.clone()
        } else {
            render(request.clone()).map_err(|error| VerificationFailure {
                layer: format!("render.{profile_name}"),
                fixture_id: fixture.fixture_id().to_string(),
                summary: error.to_string(),
            })?
        };

        compare_snapshot_file(
            fixture,
            &format!("view.{profile_name}"),
            &fixture
                .snapshot_root()
                .join(format!("view.{profile_name}.json")),
            &canonical_json_for_view_model(view_model.as_ref()).map_err(|error| {
                VerificationFailure {
                    layer: format!("view.{profile_name}"),
                    fixture_id: fixture.fixture_id().to_string(),
                    summary: error.to_string(),
                }
            })?,
        )?;
        compare_snapshot_file(
            fixture,
            &format!("render.{profile_name}"),
            &fixture
                .snapshot_root()
                .join(format!("render.{profile_name}.txt")),
            &render_result.text,
        )?;
        verify_render_expectations(
            fixture,
            profile_name,
            expectations,
            &render_result.text,
            lead_node
                .primary_location()
                .map(|location| location.path.as_str()),
        )?;
    }

    if let Some(perf) = fixture.expectations.performance.parse_time_ms_max {
        if replay.parse_time_ms > perf {
            return Err(VerificationFailure {
                layer: "performance.parse".to_string(),
                fixture_id: fixture.fixture_id().to_string(),
                summary: format!("parse time {}ms exceeded {}ms", replay.parse_time_ms, perf),
            });
        }
    }
    if let Some(perf) = fixture.expectations.performance.render_time_ms_max {
        if default_render_time_ms > perf {
            return Err(VerificationFailure {
                layer: "performance.render".to_string(),
                fixture_id: fixture.fixture_id().to_string(),
                summary: format!(
                    "default render time {}ms exceeded {}ms",
                    default_render_time_ms, perf
                ),
            });
        }
    }

    Ok(())
}

fn materialize_fixture_snapshots(
    fixture: &Fixture,
    docker_image: &str,
    check: bool,
) -> Result<usize, VerificationFailure> {
    let captured = if std::env::var_os("FORMED_SNAPSHOT_USE_EXISTING_INGRESS").is_some() {
        load_existing_ingress(fixture)?
    } else {
        capture_fixture_ingress(fixture, docker_image)?
    };
    let tempdir = tempfile::tempdir().map_err(|error| VerificationFailure {
        layer: "snapshot".to_string(),
        fixture_id: fixture.fixture_id().to_string(),
        summary: error.to_string(),
    })?;
    let temp_root = tempdir.path();
    fs::write(temp_root.join("stderr.raw"), &captured.stderr_text).map_err(|error| {
        VerificationFailure {
            layer: "snapshot".to_string(),
            fixture_id: fixture.fixture_id().to_string(),
            summary: error.to_string(),
        }
    })?;
    fs::write(temp_root.join("diagnostics.sarif"), &captured.sarif_text).map_err(|error| {
        VerificationFailure {
            layer: "snapshot".to_string(),
            fixture_id: fixture.fixture_id().to_string(),
            summary: error.to_string(),
        }
    })?;

    let document = replay_document_from_ingress(
        fixture,
        &captured.stderr_text,
        temp_root.join("diagnostics.sarif").as_path(),
    )
    .map_err(|error| VerificationFailure {
        layer: "snapshot".to_string(),
        fixture_id: fixture.fixture_id().to_string(),
        summary: error.to_string(),
    })?;
    document.validate().map_err(|error| VerificationFailure {
        layer: "snapshot".to_string(),
        fixture_id: fixture.fixture_id().to_string(),
        summary: error.errors.join("; "),
    })?;

    let snapshot_root = fixture.snapshot_root();
    fs::create_dir_all(&snapshot_root).map_err(|error| VerificationFailure {
        layer: "snapshot".to_string(),
        fixture_id: fixture.fixture_id().to_string(),
        summary: error.to_string(),
    })?;

    let mut artifacts = BTreeMap::new();
    artifacts.insert("stderr.raw".to_string(), captured.stderr_text.clone());
    artifacts.insert("diagnostics.sarif".to_string(), captured.sarif_text.clone());
    artifacts.insert(
        "ir.facts.json".to_string(),
        snapshot_json(&document, SnapshotKind::FactsOnly).map_err(|error| VerificationFailure {
            layer: "snapshot".to_string(),
            fixture_id: fixture.fixture_id().to_string(),
            summary: error.to_string(),
        })?,
    );
    artifacts.insert(
        "ir.analysis.json".to_string(),
        snapshot_json(&document, SnapshotKind::AnalysisIncluded).map_err(|error| {
            VerificationFailure {
                layer: "snapshot".to_string(),
                fixture_id: fixture.fixture_id().to_string(),
                summary: error.to_string(),
            }
        })?,
    );

    for (profile_name, _) in fixture.expectations.render.named_profiles() {
        let profile =
            render_profile_from_name(profile_name).ok_or_else(|| VerificationFailure {
                layer: "snapshot".to_string(),
                fixture_id: fixture.fixture_id().to_string(),
                summary: format!("unknown snapshot profile `{profile_name}`"),
            })?;
        let request = render_request_for_fixture(fixture, &document, profile);
        let view_model = build_view_model(&request);
        let render_result = render(request).map_err(|error| VerificationFailure {
            layer: format!("render.{profile_name}"),
            fixture_id: fixture.fixture_id().to_string(),
            summary: error.to_string(),
        })?;
        artifacts.insert(
            format!("view.{profile_name}.json"),
            canonical_json_for_view_model(view_model.as_ref()).map_err(|error| {
                VerificationFailure {
                    layer: format!("view.{profile_name}"),
                    fixture_id: fixture.fixture_id().to_string(),
                    summary: error.to_string(),
                }
            })?,
        );
        artifacts.insert(format!("render.{profile_name}.txt"), render_result.text);
    }

    for (relative, contents) in artifacts {
        let path = snapshot_root.join(relative);
        if check {
            compare_snapshot_file(
                fixture,
                path.file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or("snapshot"),
                &path,
                &contents,
            )?;
        } else {
            fs::write(&path, contents).map_err(|error| VerificationFailure {
                layer: "snapshot_write".to_string(),
                fixture_id: fixture.fixture_id().to_string(),
                summary: format!("{}: {error}", path.display()),
            })?;
        }
    }

    Ok(1)
}

fn load_existing_ingress(fixture: &Fixture) -> Result<CapturedIngress, VerificationFailure> {
    let snapshot_root = fixture.snapshot_root();
    let stderr_path = snapshot_root.join("stderr.raw");
    let sarif_path = snapshot_root.join("diagnostics.sarif");
    let stderr_text = fs::read_to_string(&stderr_path).map_err(|error| VerificationFailure {
        layer: "capture".to_string(),
        fixture_id: fixture.fixture_id().to_string(),
        summary: format!("failed to read {}: {error}", stderr_path.display()),
    })?;
    let sarif_text = fs::read_to_string(&sarif_path).map_err(|error| VerificationFailure {
        layer: "capture".to_string(),
        fixture_id: fixture.fixture_id().to_string(),
        summary: format!("failed to read {}: {error}", sarif_path.display()),
    })?;
    Ok(CapturedIngress {
        stderr_text,
        sarif_text,
    })
}

fn replay_fixture_document(
    fixture: &Fixture,
) -> Result<ReplayOutcomeAndDocument, Box<dyn std::error::Error>> {
    let snapshot_root = fixture.snapshot_root();
    let stderr_text = fs::read_to_string(snapshot_root.join("stderr.raw"))?;
    let parse_start = Instant::now();
    let document = replay_document_from_ingress(
        fixture,
        &stderr_text,
        snapshot_root.join("diagnostics.sarif").as_path(),
    )?;
    Ok(ReplayOutcomeAndDocument {
        document,
        parse_time_ms: elapsed_ms(parse_start),
    })
}

#[derive(Debug)]
struct ReplayOutcomeAndDocument {
    document: DiagnosticDocument,
    parse_time_ms: u64,
}

fn replay_document_from_ingress(
    fixture: &Fixture,
    stderr_text: &str,
    sarif_path: &Path,
) -> Result<DiagnosticDocument, Box<dyn std::error::Error>> {
    let run_info = run_info_for_fixture(fixture);
    let mut document = ingest(
        Some(sarif_path),
        stderr_text,
        producer_for_version("snapshot"),
        run_info,
    )?;
    document.captures = capture_artifacts_for_fixture(fixture, stderr_text, sarif_path)?;
    enrich_document(&mut document, &fixture.root);
    Ok(document)
}

fn run_info_for_fixture(fixture: &Fixture) -> RunInfo {
    let compiler = compiler_binary_for_fixture(fixture);
    let mut argv = vec![compiler.to_string()];
    if let Some(standard) = fixture.invoke.standard.as_ref() {
        argv.push(format!("-std={standard}"));
    }
    argv.extend(fixture.invoke.argv.iter().cloned());

    RunInfo {
        invocation_id: format!("fixture-{}", fixture.fixture_id().replace('/', "-")),
        invoked_as: Some("gcc-formed".to_string()),
        argv_redacted: argv,
        cwd_display: Some(fixture.root.display().to_string()),
        exit_status: 1,
        primary_tool: tool_for_backend(
            compiler,
            Some(format!("{}.x", fixture.invoke.major_version_selector)),
        ),
        secondary_tools: Vec::new(),
        language_mode: Some(language_mode_for_fixture(fixture)),
        target_triple: Some("x86_64-unknown-linux-gnu".to_string()),
        wrapper_mode: Some(WrapperSurface::Terminal),
    }
}

fn capture_artifacts_for_fixture(
    fixture: &Fixture,
    stderr_text: &str,
    sarif_path: &Path,
) -> Result<Vec<CaptureArtifact>, Box<dyn std::error::Error>> {
    let compiler = tool_for_backend(
        compiler_binary_for_fixture(fixture),
        Some(format!("{}.x", fixture.invoke.major_version_selector)),
    );
    let mut captures = vec![CaptureArtifact {
        id: "stderr.raw".to_string(),
        kind: ArtifactKind::CompilerStderrText,
        media_type: "text/plain".to_string(),
        encoding: Some("utf-8".to_string()),
        digest_sha256: None,
        size_bytes: Some(stderr_text.len() as u64),
        storage: ArtifactStorage::Inline,
        inline_text: Some(stderr_text.to_string()),
        external_ref: None,
        produced_by: Some(compiler.clone()),
    }];
    captures.push(CaptureArtifact {
        id: "diagnostics.sarif".to_string(),
        kind: ArtifactKind::GccSarif,
        media_type: "application/sarif+json".to_string(),
        encoding: Some("utf-8".to_string()),
        digest_sha256: None,
        size_bytes: Some(fs::metadata(sarif_path)?.len()),
        storage: ArtifactStorage::ExternalRef,
        inline_text: None,
        external_ref: Some(sarif_path.display().to_string()),
        produced_by: Some(compiler),
    });
    Ok(captures)
}

fn render_request_for_fixture(
    fixture: &Fixture,
    document: &DiagnosticDocument,
    profile: RenderProfile,
) -> RenderRequest {
    RenderRequest {
        document: document.clone(),
        profile,
        capabilities: RenderCapabilities {
            stream_kind: if matches!(profile, RenderProfile::Ci) {
                StreamKind::CiLog
            } else {
                StreamKind::Pipe
            },
            width_columns: Some(100),
            ansi_color: false,
            unicode: false,
            hyperlinks: false,
            interactive: false,
        },
        cwd: Some(fixture.root.clone()),
        path_policy: PathPolicy::RelativeToCwd,
        warning_visibility: WarningVisibility::Auto,
        debug_refs: DebugRefs::None,
        type_display_policy: TypeDisplayPolicy::CompactSafe,
        source_excerpt_policy: SourceExcerptPolicy::Auto,
    }
}

fn verify_semantic_expectations(
    fixture: &Fixture,
    document: &DiagnosticDocument,
    lead_node: &diag_core::DiagnosticNode,
    default_render_result: &diag_render::RenderResult,
) -> Result<(), VerificationFailure> {
    let semantic = fixture
        .expectations
        .semantic
        .as_ref()
        .ok_or_else(|| VerificationFailure {
            layer: "semantic".to_string(),
            fixture_id: fixture.fixture_id().to_string(),
            summary: "promoted fixture missing semantic expectations".to_string(),
        })?;

    let actual_family = lead_node
        .analysis
        .as_ref()
        .and_then(|analysis| analysis.family.as_deref())
        .unwrap_or("unknown");
    if actual_family != semantic.family {
        return Err(VerificationFailure {
            layer: "semantic.family".to_string(),
            fixture_id: fixture.fixture_id().to_string(),
            summary: format!("expected `{}`, got `{actual_family}`", semantic.family),
        });
    }

    if lead_node.severity != semantic.severity {
        return Err(VerificationFailure {
            layer: "semantic.severity".to_string(),
            fixture_id: fixture.fixture_id().to_string(),
            summary: format!(
                "expected `{}`, got `{}`",
                semantic.severity, lead_node.severity
            ),
        });
    }

    if !semantic.lead_group_any_of.is_empty()
        && !semantic
            .lead_group_any_of
            .iter()
            .any(|group_id| group_id == &lead_node.id)
    {
        return Err(VerificationFailure {
            layer: "semantic.lead_group".to_string(),
            fixture_id: fixture.fixture_id().to_string(),
            summary: format!(
                "lead group `{}` not in allowed set [{}]",
                lead_node.id,
                semantic.lead_group_any_of.join(", ")
            ),
        });
    }

    for expected in &semantic.primary_locations {
        let found = lead_node.locations.iter().any(|location| {
            location.path == expected.path
                && location.line == expected.line
                && expected
                    .column
                    .map(|column| column == location.column)
                    .unwrap_or(true)
        });
        if !found {
            return Err(VerificationFailure {
                layer: "semantic.primary_locations".to_string(),
                fixture_id: fixture.fixture_id().to_string(),
                summary: format!(
                    "lead diagnostic did not include expected location {}:{}",
                    expected.path, expected.line
                ),
            });
        }
    }

    if semantic.first_action_required
        && lead_node
            .analysis
            .as_ref()
            .and_then(|analysis| analysis.first_action_hint.as_ref())
            .map(|value| value.trim().is_empty())
            .unwrap_or(true)
    {
        return Err(VerificationFailure {
            layer: "semantic.first_action".to_string(),
            fixture_id: fixture.fixture_id().to_string(),
            summary: "lead diagnostic did not expose a first_action_hint".to_string(),
        });
    }

    if semantic.raw_provenance_required {
        let has_stderr_capture = document
            .captures
            .iter()
            .any(|capture| capture.id == "stderr.raw");
        if !has_stderr_capture || lead_node.provenance.capture_refs.is_empty() {
            return Err(VerificationFailure {
                layer: "semantic.raw_provenance".to_string(),
                fixture_id: fixture.fixture_id().to_string(),
                summary: "raw provenance was not preserved".to_string(),
            });
        }
    }

    if let Some(fallback) = semantic.fallback {
        match fallback {
            ExpectedFallback::Allowed => {}
            ExpectedFallback::Forbidden if default_render_result.used_fallback => {
                return Err(VerificationFailure {
                    layer: "semantic.fallback".to_string(),
                    fixture_id: fixture.fixture_id().to_string(),
                    summary: "default profile unexpectedly used fallback".to_string(),
                });
            }
            ExpectedFallback::Required if !default_render_result.used_fallback => {
                return Err(VerificationFailure {
                    layer: "semantic.fallback".to_string(),
                    fixture_id: fixture.fixture_id().to_string(),
                    summary: "default profile did not use required fallback".to_string(),
                });
            }
            _ => {}
        }
    }

    if let Some(confidence_min) = semantic.confidence_min.as_ref() {
        let actual = lead_node
            .analysis
            .as_ref()
            .and_then(|analysis| analysis.confidence.as_ref())
            .cloned()
            .unwrap_or(diag_core::Confidence::Unknown);
        if confidence_rank(&actual) < confidence_rank(confidence_min) {
            return Err(VerificationFailure {
                layer: "semantic.confidence".to_string(),
                fixture_id: fixture.fixture_id().to_string(),
                summary: format!("expected confidence >= {confidence_min:?}, got {actual:?}"),
            });
        }
    }

    Ok(())
}

fn verify_render_expectations(
    fixture: &Fixture,
    profile_name: &str,
    expectations: &RenderProfileExpectations,
    text: &str,
    lead_path: Option<&str>,
) -> Result<(), VerificationFailure> {
    if expectations.omission_notice_required == Some(true) && !text.contains("omitted") {
        return Err(VerificationFailure {
            layer: format!("render.{profile_name}.omission_notice"),
            fixture_id: fixture.fixture_id().to_string(),
            summary: "required omission notice was missing".to_string(),
        });
    }
    if expectations.omission_notice_required == Some(false) && text.contains("omitted") {
        return Err(VerificationFailure {
            layer: format!("render.{profile_name}.omission_notice"),
            fixture_id: fixture.fixture_id().to_string(),
            summary: "unexpected omission notice was present".to_string(),
        });
    }
    if let Some(max_lines) = expectations.first_screenful_max_lines {
        let lines = text.lines().count();
        if lines > max_lines {
            return Err(VerificationFailure {
                layer: format!("render.{profile_name}.line_budget"),
                fixture_id: fixture.fixture_id().to_string(),
                summary: format!("rendered {lines} lines, budget is {max_lines}"),
            });
        }
    }
    if expectations.path_first_required == Some(true) {
        let first_line = text.lines().next().unwrap_or_default();
        let lead_path = lead_path.unwrap_or_default();
        if !first_line.starts_with(lead_path) {
            return Err(VerificationFailure {
                layer: format!("render.{profile_name}.path_first"),
                fixture_id: fixture.fixture_id().to_string(),
                summary: format!("first line was not path-first: `{first_line}`"),
            });
        }
    }
    if expectations.color_meaning_forbidden == Some(true) && text.contains('\u{1b}') {
        return Err(VerificationFailure {
            layer: format!("render.{profile_name}.ansi"),
            fixture_id: fixture.fixture_id().to_string(),
            summary: "render output used ANSI escapes".to_string(),
        });
    }
    Ok(())
}

fn compare_snapshot_file(
    fixture: &Fixture,
    layer: &str,
    path: &Path,
    actual: &str,
) -> Result<(), VerificationFailure> {
    let expected = fs::read_to_string(path).map_err(|error| VerificationFailure {
        layer: layer.to_string(),
        fixture_id: fixture.fixture_id().to_string(),
        summary: format!("failed to read {}: {error}", path.display()),
    })?;
    let expected =
        normalize_snapshot_contents(path, &expected).map_err(|summary| VerificationFailure {
            layer: layer.to_string(),
            fixture_id: fixture.fixture_id().to_string(),
            summary,
        })?;
    let actual =
        normalize_snapshot_contents(path, actual).map_err(|summary| VerificationFailure {
            layer: layer.to_string(),
            fixture_id: fixture.fixture_id().to_string(),
            summary,
        })?;
    if expected == actual {
        return Ok(());
    }
    Err(VerificationFailure {
        layer: layer.to_string(),
        fixture_id: fixture.fixture_id().to_string(),
        summary: first_diff_summary(&expected, &actual),
    })
}

fn normalize_snapshot_contents(path: &Path, contents: &str) -> Result<String, String> {
    let contents = normalize_transient_object_paths(contents);
    if path.file_name().and_then(|value| value.to_str()) != Some("diagnostics.sarif") {
        return Ok(contents);
    }

    let value: serde_json::Value = serde_json::from_str(&contents)
        .map_err(|error| format!("failed to parse {} as JSON: {error}", path.display()))?;
    let value = normalize_sarif_snapshot_value(value);
    diag_core::canonical_json(&value)
        .map_err(|error| format!("failed to canonicalize {}: {error}", path.display()))
}

fn normalize_transient_object_paths(contents: &str) -> String {
    let mut normalized = String::with_capacity(contents.len());
    let mut remaining = contents;

    while let Some(start) = remaining.find("/tmp/") {
        normalized.push_str(&remaining[..start]);
        let candidate = &remaining[start..];
        let path_len = candidate
            .chars()
            .take_while(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '/' | '.' | '_' | '-'))
            .map(char::len_utf8)
            .sum::<usize>();
        let path = &candidate[..path_len];
        if path.starts_with("/tmp/") && path.ends_with(".o") {
            normalized.push_str("/tmp/<object>.o");
            remaining = &candidate[path_len..];
        } else {
            normalized.push_str("/tmp/");
            remaining = &candidate["/tmp/".len()..];
        }
    }

    normalized.push_str(remaining);
    normalized
}

fn normalize_sarif_snapshot_value(value: serde_json::Value) -> serde_json::Value {
    let mut normalized = serde_json::Map::new();
    if let Some(version) = value.get("version").cloned() {
        normalized.insert("version".to_string(), version);
    }
    if let Some(runs) = value.get("runs").and_then(serde_json::Value::as_array) {
        normalized.insert(
            "runs".to_string(),
            serde_json::Value::Array(
                runs.iter()
                    .map(|run| {
                        let mut normalized_run = serde_json::Map::new();
                        if let Some(results) = run.get("results").cloned() {
                            normalized_run.insert("results".to_string(), results);
                        }
                        serde_json::Value::Object(normalized_run)
                    })
                    .collect(),
            ),
        );
    }
    serde_json::Value::Object(normalized)
}

fn canonical_json_for_view_model(
    view_model: Option<&diag_render::RenderViewModel>,
) -> Result<String, serde_json::Error> {
    match view_model {
        Some(model) => diag_core::canonical_json(model),
        None => diag_core::canonical_json(&serde_json::Value::Null),
    }
}

fn render_profile_from_name(name: &str) -> Option<RenderProfile> {
    match name {
        "default" => Some(RenderProfile::Default),
        "concise" => Some(RenderProfile::Concise),
        "verbose" => Some(RenderProfile::Verbose),
        "ci" => Some(RenderProfile::Ci),
        "raw_fallback" => Some(RenderProfile::RawFallback),
        _ => None,
    }
}

fn lead_node_for_document<'a>(
    document: &'a DiagnosticDocument,
    displayed_group_refs: &[String],
) -> Option<&'a diag_core::DiagnosticNode> {
    let lead_id = displayed_group_refs.first()?;
    document.diagnostics.iter().find(|node| &node.id == lead_id)
}

fn confidence_rank(confidence: &diag_core::Confidence) -> u8 {
    match confidence {
        diag_core::Confidence::High => 4,
        diag_core::Confidence::Medium => 3,
        diag_core::Confidence::Low => 2,
        diag_core::Confidence::Unknown => 1,
    }
}

fn select_fixtures<'a>(
    fixtures: &'a [Fixture],
    fixture_filter: Option<&str>,
    family_filter: Option<&str>,
    subset: SnapshotSubset,
) -> Vec<&'a Fixture> {
    fixtures
        .iter()
        .filter(|fixture| {
            fixture_filter
                .map(|needle| fixture.fixture_id() == needle)
                .unwrap_or(true)
        })
        .filter(|fixture| {
            family_filter
                .map(|needle| fixture.family_key() == needle)
                .unwrap_or(true)
        })
        .filter(|fixture| match subset {
            SnapshotSubset::All => true,
            SnapshotSubset::Representative => {
                REPRESENTATIVE_FIXTURES.contains(&fixture.fixture_id())
            }
        })
        .collect()
}

fn validate_snapshot_inputs(fixture: &Fixture) -> Result<(), Box<dyn std::error::Error>> {
    for relative in [
        "src",
        "invoke.yaml",
        "expectations.yaml",
        "meta.yaml",
        "snapshots",
    ] {
        if !fixture.root.join(relative).exists() {
            return Err(format!(
                "fixture {} missing {}",
                fixture.fixture_id(),
                fixture.root.join(relative).display()
            )
            .into());
        }
    }
    if !fixture.is_promoted() {
        return Err(format!("fixture {} is not promoted", fixture.fixture_id()).into());
    }
    Ok(())
}

fn capture_fixture_ingress(
    fixture: &Fixture,
    docker_image: &str,
) -> Result<CapturedIngress, VerificationFailure> {
    let sandbox = tempfile::tempdir().map_err(|error| VerificationFailure {
        layer: "capture".to_string(),
        fixture_id: fixture.fixture_id().to_string(),
        summary: error.to_string(),
    })?;
    copy_dir_recursive(&fixture.root.join("src"), &sandbox.path().join("src")).map_err(
        |error| VerificationFailure {
            layer: "capture".to_string(),
            fixture_id: fixture.fixture_id().to_string(),
            summary: error.to_string(),
        },
    )?;

    let compiler = compiler_binary_for_fixture(fixture);
    let mut shell_args = vec![compiler.to_string()];
    if let Some(standard) = fixture.invoke.standard.as_ref() {
        shell_args.push(format!("-std={standard}"));
    }
    shell_args.extend(fixture.invoke.argv.iter().cloned());
    shell_args
        .push("-fdiagnostics-add-output=sarif:version=2.1,file=diagnostics.sarif".to_string());
    let command_line = format!(
        "set -euo pipefail; {} 1>stdout.raw 2>stderr.raw || true",
        shell_args
            .iter()
            .map(|arg| shell_quote(arg))
            .collect::<Vec<_>>()
            .join(" ")
    );

    let mut command = Command::new("docker");
    command
        .arg("run")
        .arg("--rm")
        .arg("-v")
        .arg(format!("{}:/workspace", sandbox.path().display()))
        .arg("-w")
        .arg("/workspace")
        .arg("-e")
        .arg("LC_MESSAGES=C");
    for (key, value) in &fixture.invoke.env_overrides {
        command.arg("-e").arg(format!("{key}={value}"));
    }
    command
        .arg(docker_image)
        .arg("bash")
        .arg("-lc")
        .arg(command_line);
    let output = command.output().map_err(|error| VerificationFailure {
        layer: "capture".to_string(),
        fixture_id: fixture.fixture_id().to_string(),
        summary: format!("failed to run docker: {error}"),
    })?;
    if !output.status.success() {
        return Err(VerificationFailure {
            layer: "capture".to_string(),
            fixture_id: fixture.fixture_id().to_string(),
            summary: format!(
                "docker invocation failed: {}",
                String::from_utf8_lossy(&output.stderr)
            ),
        });
    }

    let stderr_path = sandbox.path().join("stderr.raw");
    let sarif_path = sandbox.path().join("diagnostics.sarif");
    let stderr_text = fs::read_to_string(&stderr_path).map_err(|error| VerificationFailure {
        layer: "capture".to_string(),
        fixture_id: fixture.fixture_id().to_string(),
        summary: format!("failed to read {}: {error}", stderr_path.display()),
    })?;
    let sarif_text = fs::read_to_string(&sarif_path).map_err(|error| VerificationFailure {
        layer: "capture".to_string(),
        fixture_id: fixture.fixture_id().to_string(),
        summary: format!("failed to read {}: {error}", sarif_path.display()),
    })?;
    Ok(CapturedIngress {
        stderr_text,
        sarif_text,
    })
}

fn compiler_binary_for_fixture(fixture: &Fixture) -> &'static str {
    match fixture.invoke.language.as_str() {
        "cpp" | "cxx" => "g++",
        _ => "gcc",
    }
}

fn language_mode_for_fixture(fixture: &Fixture) -> LanguageMode {
    match fixture.invoke.language.as_str() {
        "cpp" | "cxx" => LanguageMode::Cpp,
        _ => LanguageMode::C,
    }
}

fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\"'\"'"))
}

fn copy_dir_recursive(from: &Path, to: &Path) -> Result<(), std::io::Error> {
    fs::create_dir_all(to)?;
    for entry in fs::read_dir(from)? {
        let entry = entry?;
        let source = entry.path();
        let destination = to.join(entry.file_name());
        if source.is_dir() {
            copy_dir_recursive(&source, &destination)?;
        } else {
            fs::copy(source, destination)?;
        }
    }
    Ok(())
}

fn run_package(options: PackageOptions) -> Result<PackageOutput, Box<dyn std::error::Error>> {
    run_package_at(&workspace_root(), &options)
}

fn run_install(options: InstallOptions) -> Result<InstallOutput, Box<dyn std::error::Error>> {
    run_install_at(&std::env::current_dir()?, &options)
}

fn run_vendor(options: VendorOptions) -> Result<VendorOutput, Box<dyn std::error::Error>> {
    run_vendor_at(&workspace_root(), &options)
}

fn run_vendor_at(
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

fn run_release_publish(
    options: ReleasePublishOptions,
) -> Result<ReleasePublishOutput, Box<dyn std::error::Error>> {
    run_release_publish_at(&std::env::current_dir()?, &options)
}

fn run_release_publish_at(
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
        support_tier: manifest.support_tier_declaration.clone(),
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

fn run_release_promote(
    options: ReleasePromoteOptions,
) -> Result<ReleasePromoteOutput, Box<dyn std::error::Error>> {
    run_release_promote_at(&std::env::current_dir()?, &options)
}

fn run_release_promote_at(
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

fn run_release_resolve(
    options: ReleaseResolveOptions,
) -> Result<ReleaseResolveOutput, Box<dyn std::error::Error>> {
    run_release_resolve_at(&std::env::current_dir()?, &options)
}

fn run_release_resolve_at(
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

fn run_install_release(
    options: InstallReleaseOptions,
) -> Result<InstallReleaseOutput, Box<dyn std::error::Error>> {
    run_install_release_at(&std::env::current_dir()?, &options)
}

fn run_install_release_at(
    base_dir: &Path,
    options: &InstallReleaseOptions,
) -> Result<InstallReleaseOutput, Box<dyn std::error::Error>> {
    let repository_root = resolve_workspace_path(base_dir, &options.repository_root);
    let selector = release_selector(options.channel.as_deref(), options.version.as_deref())?;
    let (requested_channel, release) =
        resolve_published_release(&repository_root, &options.target_triple, selector)?;
    if let Some(expected_sha) = options.expected_primary_sha256.as_deref() {
        if release.primary_archive_sha256 != expected_sha {
            return Err(format!(
                "release checksum mismatch: expected {expected_sha}, got {}",
                release.primary_archive_sha256
            )
            .into());
        }
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

fn run_hermetic_release_check(
    options: HermeticReleaseOptions,
) -> Result<HermeticReleaseOutput, Box<dyn std::error::Error>> {
    run_hermetic_release_check_at(&workspace_root(), &options)
}

fn run_hermetic_release_check_at(
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

fn run_install_at(
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
    fs::create_dir_all(&install_root)?;
    let previous_version = current_version_name(&install_root)?;
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
    })
}

fn run_rollback(options: RollbackOptions) -> Result<RollbackOutput, Box<dyn std::error::Error>> {
    run_rollback_at(&std::env::current_dir()?, &options)
}

fn run_rollback_at(
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
    ensure_launcher_symlinks(&bin_dir, &install_root)?;
    assert_binary_reports_version(
        &version_root.join("bin/gcc-formed"),
        version_name.trim_start_matches('v'),
    )?;
    swap_symlink(
        &install_root.join("current"),
        Path::new(&version_name),
        true,
    )?;
    assert_binary_reports_version(
        &bin_dir.join("gcc-formed"),
        version_name.trim_start_matches('v'),
    )?;

    Ok(RollbackOutput {
        install_root,
        active_version: version_name.trim_start_matches('v').to_string(),
        current_path: bin_dir.join("gcc-formed"),
    })
}

fn run_uninstall(options: UninstallOptions) -> Result<UninstallOutput, Box<dyn std::error::Error>> {
    run_uninstall_at(&std::env::current_dir()?, &options)
}

fn run_uninstall_at(
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
            fs::remove_dir_all(&version_root)?;
            removed_versions.push(version_name.trim_start_matches('v').to_string());
        }
        UninstallMode::PurgeInstall => {
            for version_dir in installed_versions(&install_root)? {
                fs::remove_dir_all(install_root.join(&version_dir))?;
                removed_versions.push(version_dir.trim_start_matches('v').to_string());
            }
            let current_link = install_root.join("current");
            remove_path_if_exists(&current_link)?;
            removed_launchers = remove_managed_launchers(&bin_dir, &install_root)?;
            if install_root.exists() && fs::read_dir(&install_root)?.next().is_none() {
                fs::remove_dir(&install_root)?;
            }
        }
    }

    let mut purged_state = false;
    if options.purge_state {
        let state_root = state_root.ok_or("--purge-state requires --state-root")?;
        if state_root.exists() {
            fs::remove_dir_all(&state_root)?;
        }
        purged_state = true;
    }

    Ok(UninstallOutput {
        install_root,
        removed_versions,
        removed_launchers,
        purged_state,
    })
}

fn run_package_at(
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
        &options.support_tier,
        &options.release_channel,
    );
    let debug_manifest = build_manifest_for_target(
        lockfile_hash,
        vendor_hash,
        &options.target_triple,
        &options.support_tier,
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

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .to_path_buf()
}

fn ensure_clean_git_tree(workspace_root: &Path) -> Result<(), Box<dyn std::error::Error>> {
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

fn ensure_release_inputs(workspace_root: &Path) -> Result<(), Box<dyn std::error::Error>> {
    for relative in [
        "README.md",
        "RELEASE-NOTES.md",
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

fn canonicalize_existing_path(
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

fn resolve_workspace_path(workspace_root: &Path, path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        workspace_root.join(path)
    }
}

fn release_target_root(repository_root: &Path, target_triple: &str) -> PathBuf {
    repository_root.join("targets").join(target_triple)
}

fn release_channel_root(repository_root: &Path, target_triple: &str) -> PathBuf {
    release_target_root(repository_root, target_triple).join("channels")
}

fn release_version_root(repository_root: &Path, target_triple: &str, version: &str) -> PathBuf {
    release_target_root(repository_root, target_triple)
        .join("versions")
        .join(version_dir_name(version))
}

fn release_selector<'a>(
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

fn ensure_operations_channel(channel: &str) -> Result<(), Box<dyn std::error::Error>> {
    match channel {
        "canary" | "beta" | "stable" => Ok(()),
        _ => Err(format!("unsupported operations channel: {channel}").into()),
    }
}

fn relative_display(root: &Path, path: &Path) -> Result<String, Box<dyn std::error::Error>> {
    Ok(path
        .strip_prefix(root)
        .map_err(|_| format!("{} is not under {}", path.display(), root.display()))?
        .display()
        .to_string()
        .replace('\\', "/"))
}

fn unix_timestamp_secs() -> Result<u64, Box<dyn std::error::Error>> {
    Ok(std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_secs())
}

fn artifact_slug_for_target(target_triple: &str) -> String {
    let descriptor = diag_trace::describe_target(target_triple);
    format!(
        "{}-{}-{}",
        descriptor.os, descriptor.arch, descriptor.libc_family
    )
}

fn stage_release_payload(
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
        &workspace_root.join("RELEASE-NOTES.md"),
        &stage_root.join("share/doc/gcc-formed/RELEASE-NOTES.md"),
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

fn copy_release_file(from: &Path, to: &Path) -> Result<(), Box<dyn std::error::Error>> {
    if let Some(parent) = to.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::copy(from, to)?;
    let permissions = fs::metadata(from)?.permissions();
    fs::set_permissions(to, permissions)?;
    Ok(())
}

fn render_build_info(manifest: &BuildManifest, archive_role: &str, binary: &Path) -> String {
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
    let _ = writeln!(
        &mut text,
        "support tier: {}",
        manifest.support_tier_declaration
    );
    let _ = writeln!(&mut text, "release channel: {}", manifest.release_channel);
    let _ = writeln!(&mut text, "archive role: {archive_role}");
    let _ = writeln!(&mut text, "binary source: {}", binary.display());
    let _ = writeln!(&mut text, "lockfile hash: {}", manifest.lockfile_hash);
    let _ = writeln!(&mut text, "vendor hash: {}", manifest.vendor_hash);
    text
}

fn payload_checksums(stage_root: &Path) -> Result<Vec<ChecksumEntry>, Box<dyn std::error::Error>> {
    let payload_paths = [
        "bin/gcc-formed",
        "bin/g++-formed",
        "share/doc/gcc-formed/README.md",
        "share/doc/gcc-formed/RELEASE-NOTES.md",
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

fn hash_vendor_dir(path: &Path) -> Result<String, Box<dyn std::error::Error>> {
    if !path.exists() {
        return Ok("vendor-missing".to_string());
    }
    let mut entries = Vec::new();
    collect_paths_for_hash(path, path, &mut entries)?;
    entries.sort();
    Ok(sha256_bytes(entries.join("\n").as_bytes()))
}

fn collect_paths_for_hash(
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

fn sha256_file(path: &Path) -> Result<String, Box<dyn std::error::Error>> {
    let bytes = fs::read(path)?;
    Ok(sha256_bytes(&bytes))
}

fn sha256_bytes(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}

fn create_tar_archive(
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

fn create_source_archive(
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

fn render_sha256sums(paths: &[&Path]) -> Result<String, Box<dyn std::error::Error>> {
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

fn write_detached_signature(
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

fn verify_release_signature_if_present(
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

fn read_optional_detached_signature(
    signature_path: &Path,
) -> Result<Option<DetachedSignatureEnvelope>, Box<dyn std::error::Error>> {
    if signature_path.exists() {
        Ok(Some(read_json_file(signature_path)?))
    } else {
        Ok(None)
    }
}

fn verify_detached_signature(
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
    if let Some(expected_key_id) = expected_key_id {
        if envelope.key_id != expected_key_id {
            return Err(format!(
                "detached signature key mismatch: expected {expected_key_id}, got {}",
                envelope.key_id
            )
            .into());
        }
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
    if let Some(expected_public_key_sha256) = expected_public_key_sha256 {
        if public_key_sha256 != expected_public_key_sha256 {
            return Err(format!(
                "detached signature public key mismatch: expected {expected_public_key_sha256}, got {public_key_sha256}"
            )
            .into());
        }
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

fn read_signing_key(path: &Path) -> Result<SigningKey, Box<dyn std::error::Error>> {
    let bytes = decode_hex(&fs::read_to_string(path)?, "private key")?;
    let bytes: [u8; 32] = bytes
        .try_into()
        .map_err(|_| "private key must contain exactly 32 bytes")?;
    Ok(SigningKey::from_bytes(&bytes))
}

fn signing_key_id(verifying_key: &VerifyingKey) -> String {
    format!("ed25519:{}", &sha256_bytes(&verifying_key.to_bytes())[..16])
}

fn signing_public_key_sha256(verifying_key: &VerifyingKey) -> String {
    sha256_bytes(&verifying_key.to_bytes())
}

fn decode_hex(value: &str, label: &str) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let normalized = value.trim();
    if normalized.len() % 2 != 0 {
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

fn encode_hex(bytes: &[u8]) -> String {
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        let _ = write!(&mut output, "{byte:02x}");
    }
    output
}

fn read_json_file<T>(path: &Path) -> Result<T, Box<dyn std::error::Error>>
where
    T: for<'de> Deserialize<'de>,
{
    Ok(serde_json::from_str(&fs::read_to_string(path)?)?)
}

fn read_published_release(
    version_root: &Path,
) -> Result<PublishedRelease, Box<dyn std::error::Error>> {
    read_json_file(&version_root.join("release.json"))
}

fn read_release_channel_pointer(
    repository_root: &Path,
    target_triple: &str,
    channel: &str,
) -> Result<ReleaseChannelPointer, Box<dyn std::error::Error>> {
    read_json_file(
        &release_channel_root(repository_root, target_triple).join(format!("{channel}.json")),
    )
}

fn verify_published_release(
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

fn resolve_published_release(
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

fn vendored_source_config(
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

fn read_build_manifest(path: &Path) -> Result<BuildManifest, Box<dyn std::error::Error>> {
    Ok(serde_json::from_str(&fs::read_to_string(path)?)?)
}

fn ensure_target_aware_install_root(
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

fn verify_shasums(
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

fn find_primary_archive(control_dir: &Path) -> Result<PathBuf, Box<dyn std::error::Error>> {
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

fn find_release_archive_by_suffix(
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

fn extract_tar_archive(
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

fn extracted_payload_root(
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

fn verify_manifest_alignment(
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

fn verify_payload_checksums(
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

fn run_staged_self_check(
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

fn version_dir_name(version: &str) -> String {
    if version.starts_with('v') {
        version.to_string()
    } else {
        format!("v{version}")
    }
}

fn assert_binary_reports_version(
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

fn current_version_name(install_root: &Path) -> Result<Option<String>, Box<dyn std::error::Error>> {
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

fn installed_versions(install_root: &Path) -> Result<Vec<String>, Box<dyn std::error::Error>> {
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

fn ensure_launcher_symlinks(
    bin_dir: &Path,
    install_root: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    fs::create_dir_all(bin_dir)?;
    swap_symlink(
        &bin_dir.join("gcc-formed"),
        &install_root.join("current/bin/gcc-formed"),
        false,
    )?;
    swap_symlink(
        &bin_dir.join("g++-formed"),
        &install_root.join("current/bin/g++-formed"),
        false,
    )?;
    Ok(())
}

fn remove_managed_launchers(
    bin_dir: &Path,
    install_root: &Path,
) -> Result<Vec<String>, Box<dyn std::error::Error>> {
    let mut removed = Vec::new();
    for launcher in ["gcc-formed", "g++-formed"] {
        let path = bin_dir.join(launcher);
        if fs::symlink_metadata(&path).is_err() {
            continue;
        }
        if launcher_is_managed(&path, install_root)? {
            remove_path_if_exists(&path)?;
            removed.push(launcher.to_string());
        }
    }
    Ok(removed)
}

fn launcher_is_managed(
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

fn remove_path_if_exists(path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    if let Ok(metadata) = fs::symlink_metadata(path) {
        if metadata.is_dir() && !metadata.file_type().is_symlink() {
            fs::remove_dir_all(path)?;
        } else {
            fs::remove_file(path)?;
        }
    }
    Ok(())
}

fn swap_symlink(
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
fn create_symlink(
    target: &Path,
    link_path: &Path,
    _target_is_dir: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    std::os::unix::fs::symlink(target, link_path)?;
    Ok(())
}

#[cfg(windows)]
fn create_symlink(
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
fn create_symlink(
    _target: &Path,
    _link_path: &Path,
    _target_is_dir: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    Err("symlink operations are unsupported on this platform".into())
}

fn report_failures(mode: &str, failures: &[VerificationFailure]) {
    eprintln!("mode: {mode}");
    eprintln!("failed fixture count: {}", failures.len());
    if let Some(first) = failures.first() {
        eprintln!("failed layer: {}", first.layer);
        eprintln!("first failed fixture: {}", first.fixture_id);
        eprintln!("first diff summary: {}", first.summary);
    }
}

fn first_diff_summary(expected: &str, actual: &str) -> String {
    for (index, (left, right)) in expected.lines().zip(actual.lines()).enumerate() {
        if left != right {
            return format!("line {} expected `{}` but got `{}`", index + 1, left, right);
        }
    }
    let expected_lines = expected.lines().count();
    let actual_lines = actual.lines().count();
    if expected_lines != actual_lines {
        format!(
            "line count changed: expected {} lines, got {} lines",
            expected_lines, actual_lines
        )
    } else {
        "snapshot content changed".to_string()
    }
}

fn elapsed_ms(start: Instant) -> u64 {
    start.elapsed().as_millis() as u64
}

fn run(binary: &str, args: &[&str]) -> Result<(), Box<dyn std::error::Error>> {
    let status = Command::new(binary).args(args).status()?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("{binary} {} failed", args.join(" ")).into())
    }
}

fn enforce_minimum_family_counts(
    counts: &std::collections::BTreeMap<String, usize>,
) -> Result<(), Box<dyn std::error::Error>> {
    let minimums = [
        ("syntax", 8_usize),
        ("type", 10),
        ("overload", 6),
        ("template", 12),
        ("macro_include", 10),
        ("linker", 10),
        ("partial", 6),
        ("path", 6),
    ];
    for (family, minimum) in minimums {
        let actual = counts.get(family).copied().unwrap_or_default();
        if actual < minimum {
            return Err(format!(
                "family `{family}` below minimum fixture count: expected >= {minimum}, got {actual}"
            )
            .into());
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_file(path: &Path, contents: &[u8]) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(path, contents).unwrap();
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

    fn run_command(root: &Path, binary: &str, args: &[&str]) {
        let output = Command::new(binary)
            .current_dir(root)
            .args(args)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{binary} {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr)
        );
    }

    fn fake_wrapper_script(version: &str) -> String {
        format!(
            "#!/bin/sh\nif [ \"$1\" = \"--formed-version\" ]; then\n  printf '%s\\n' \"{version}\"\nelif [ \"$1\" = \"--formed-self-check\" ]; then\n  printf '%s\\n' '{{\"binary\":\"ok\"}}'\nelse\n  printf '%s\\n' \"packaged-{version}\"\nfi\n"
        )
    }

    fn test_signing_private_key_hex() -> &'static str {
        "000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f"
    }

    fn write_signing_private_key(path: &Path) {
        write_file(
            path,
            format!("{}\n", test_signing_private_key_hex()).as_bytes(),
        );
    }

    fn test_signing_public_key_sha256() -> String {
        let sandbox = tempfile::tempdir().unwrap();
        let path = sandbox.path().join("release-signing.key");
        write_signing_private_key(&path);
        signing_public_key_sha256(&read_signing_key(&path).unwrap().verifying_key())
    }

    fn init_release_repo(version: &str) -> (tempfile::TempDir, PathBuf, PathBuf) {
        let sandbox = tempfile::tempdir().unwrap();
        let repo_root = sandbox.path().join("repo");
        let binary_root = sandbox.path().join("binary");
        fs::create_dir_all(&repo_root).unwrap();
        fs::create_dir_all(&binary_root).unwrap();

        write_file(&repo_root.join(".gitignore"), b"/dist\n");
        write_file(&repo_root.join("README.md"), b"# gcc-formed\n");
        write_file(
            &repo_root.join("RELEASE-NOTES.md"),
            b"# Release Notes\n\n- Initial release packaging smoke fixture.\n",
        );
        write_file(&repo_root.join("LICENSE"), b"Apache-2.0\n");
        write_file(&repo_root.join("NOTICE"), b"gcc-formed notice\n");
        write_file(&repo_root.join("Cargo.lock"), b"version = 3\n");
        write_file(&repo_root.join("src/main.rs"), b"fn main() {}\n");

        let binary_path = binary_root.join("gcc-formed");
        write_file(&binary_path, fake_wrapper_script(version).as_bytes());
        make_executable(&binary_path);

        run_command(&repo_root, "git", &["init", "-q", "-b", "main"]);
        run_command(
            &repo_root,
            "git",
            &["config", "user.email", "ci@example.com"],
        );
        run_command(&repo_root, "git", &["config", "user.name", "CI"]);
        run_command(&repo_root, "git", &["add", "."]);
        run_command(&repo_root, "git", &["commit", "-q", "-m", "initial"]);

        (sandbox, repo_root, binary_path)
    }

    fn init_minimal_cargo_project() -> (tempfile::TempDir, PathBuf) {
        let sandbox = tempfile::tempdir().unwrap();
        let root = sandbox.path().join("mini");
        fs::create_dir_all(root.join(".cargo")).unwrap();
        fs::create_dir_all(root.join("src")).unwrap();
        write_file(
            &root.join(".cargo/config.toml"),
            b"[build]\ntarget-dir = \"target\"\n",
        );
        write_file(
            &root.join("Cargo.toml"),
            b"[package]\nname = \"mini\"\nversion = \"0.1.0\"\nedition = \"2024\"\n\n[dependencies]\n",
        );
        write_file(&root.join("src/main.rs"), b"fn main() {}\n");
        run_command(&root, "cargo", &["generate-lockfile", "--offline"]);
        (sandbox, root)
    }

    #[test]
    fn normalizes_sarif_snapshots_before_compare() {
        let expected = r#"{"version":"2.1.0","runs":[{"results":[{"message":{"text":"link failed for /tmp/helper.o and /tmp/main.o"}}]}]}"#;
        let actual = r#"{
  "$schema": "https://docs.oasis-open.org/sarif/sarif/v2.1.0/errata01/os/schemas/sarif-schema-2.1.0.json",
  "runs": [
    {
      "artifacts": [
        {
          "location": {
            "uri": "src/main.c"
          }
        }
      ],
      "results": [
        {
          "message": {
            "text": "link failed for /tmp/cc123456.o and /tmp/cc654321.o"
          }
        }
      ]
    }
  ],
  "version": "2.1.0"
}"#;

        let normalized_expected =
            normalize_snapshot_contents(Path::new("diagnostics.sarif"), expected).unwrap();
        let normalized_actual =
            normalize_snapshot_contents(Path::new("diagnostics.sarif"), actual).unwrap();

        assert_eq!(normalized_expected, normalized_actual);
    }

    #[test]
    fn package_smoke_emits_release_artifacts() {
        let (_sandbox, repo_root, binary_path) = init_release_repo("0.1.0");
        let package = run_package_at(
            &repo_root,
            &PackageOptions {
                binary: binary_path.clone(),
                debug_binary: None,
                target_triple: "x86_64-unknown-linux-musl".to_string(),
                out_dir: PathBuf::from("dist"),
                release_channel: "stable".to_string(),
                support_tier: "gcc15_primary".to_string(),
                signing_private_key: None,
            },
        )
        .unwrap();

        assert!(package.primary_archive.exists());
        assert!(package.debug_archive.exists());
        assert!(package.source_archive.exists());
        assert!(package.manifest_path.exists());
        assert!(package.build_info_path.exists());
        assert!(package.shasums_path.exists());

        let manifest = serde_json::from_str::<BuildManifest>(
            &fs::read_to_string(&package.manifest_path).unwrap(),
        )
        .unwrap();
        assert_eq!(manifest.product_name, DEFAULT_PRODUCT_NAME);
        assert_eq!(manifest.artifact_target_triple, "x86_64-unknown-linux-musl");
        assert_eq!(manifest.artifact_libc_family, "musl");
        assert_eq!(manifest.release_channel, "stable");
        assert_eq!(manifest.support_tier_declaration, "gcc15_primary");
        assert_eq!(manifest.checksums.len(), 7);
        assert!(
            manifest
                .checksums
                .iter()
                .any(|entry| entry.path == "bin/gcc-formed")
        );

        let shasums = fs::read_to_string(&package.shasums_path).unwrap();
        assert!(
            shasums.contains(
                package
                    .primary_archive
                    .file_name()
                    .and_then(|value| value.to_str())
                    .unwrap()
            )
        );
        assert!(
            shasums.contains(
                package
                    .source_archive
                    .file_name()
                    .and_then(|value| value.to_str())
                    .unwrap()
            )
        );

        let output = Command::new("tar")
            .args(["-tzf", &package.primary_archive.display().to_string()])
            .output()
            .unwrap();
        assert!(output.status.success());
        let listing = String::from_utf8(output.stdout).unwrap();
        assert!(listing.contains("bin/gcc-formed"));
        assert!(listing.contains("bin/g++-formed"));
        assert!(listing.contains("manifest.json"));
        assert!(listing.contains("build-info.txt"));
        assert!(listing.contains("share/doc/gcc-formed/README.md"));
        assert!(listing.contains("share/licenses/gcc-formed/LICENSE"));
    }

    #[test]
    fn package_rejects_dirty_worktree() {
        let (_sandbox, repo_root, binary_path) = init_release_repo("0.1.0");
        write_file(&repo_root.join("dirty.txt"), b"untracked\n");

        let error = run_package_at(
            &repo_root,
            &PackageOptions {
                binary: binary_path,
                debug_binary: None,
                target_triple: "x86_64-unknown-linux-gnu".to_string(),
                out_dir: PathBuf::from("dist"),
                release_channel: "stable".to_string(),
                support_tier: "gcc15_primary".to_string(),
                signing_private_key: None,
            },
        )
        .unwrap_err();
        assert!(error.to_string().contains("clean git worktree"));
    }

    #[test]
    fn package_requires_release_documents() {
        let (_sandbox, repo_root, binary_path) = init_release_repo("0.1.0");
        fs::remove_file(repo_root.join("NOTICE")).unwrap();
        run_command(&repo_root, "git", &["add", "-u"]);
        run_command(&repo_root, "git", &["commit", "-q", "-m", "remove notice"]);

        let error = run_package_at(
            &repo_root,
            &PackageOptions {
                binary: binary_path,
                debug_binary: None,
                target_triple: "x86_64-unknown-linux-gnu".to_string(),
                out_dir: PathBuf::from("dist"),
                release_channel: "stable".to_string(),
                support_tier: "gcc15_primary".to_string(),
                signing_private_key: None,
            },
        )
        .unwrap_err();
        assert!(error.to_string().contains("required release input missing"));
    }

    #[test]
    fn artifact_slug_is_platform_focused() {
        assert_eq!(
            artifact_slug_for_target("x86_64-unknown-linux-musl"),
            "linux-x86_64-musl"
        );
        assert_eq!(
            artifact_slug_for_target("aarch64-unknown-linux-gnu"),
            "linux-aarch64-gnu"
        );
    }

    #[test]
    fn vendored_source_config_replaces_crates_io() {
        let config = vendored_source_config(
            Path::new("/tmp/vendor"),
            Path::new("/tmp/target/hermetic-release"),
        )
        .unwrap();
        assert!(config.contains("[source.crates-io]"));
        assert!(config.contains("replace-with = \"vendored-sources\""));
        assert!(config.contains("directory = \"/tmp/vendor\""));
        assert!(config.contains("target-dir = \"/tmp/target/hermetic-release\""));
    }

    #[test]
    fn vendor_and_hermetic_release_check_work_for_minimal_project() {
        let (_sandbox, root) = init_minimal_cargo_project();
        let vendor = run_vendor_at(
            &root,
            &VendorOptions {
                output_dir: PathBuf::from("vendor"),
            },
        )
        .unwrap();
        assert!(vendor.vendor_dir.exists());
        assert_ne!(vendor.vendor_hash, "vendor-missing");

        let hermetic = run_hermetic_release_check_at(
            &root,
            &HermeticReleaseOptions {
                vendor_dir: PathBuf::from("vendor"),
                bin: "mini".to_string(),
                target_triple: None,
            },
        )
        .unwrap();
        assert_eq!(hermetic.bin, "mini");
        assert_eq!(hermetic.vendor_hash, vendor.vendor_hash);
        assert_eq!(hermetic.target_triple, None);
        assert!(hermetic.target_dir.join("release/mini").exists());
    }

    #[test]
    fn hermetic_release_check_supports_musl_target_for_minimal_project() {
        let (_sandbox, root) = init_minimal_cargo_project();
        run_vendor_at(
            &root,
            &VendorOptions {
                output_dir: PathBuf::from("vendor"),
            },
        )
        .unwrap();

        let hermetic = run_hermetic_release_check_at(
            &root,
            &HermeticReleaseOptions {
                vendor_dir: PathBuf::from("vendor"),
                bin: "mini".to_string(),
                target_triple: Some("x86_64-unknown-linux-musl".to_string()),
            },
        )
        .unwrap();

        assert_eq!(
            hermetic.target_triple.as_deref(),
            Some("x86_64-unknown-linux-musl")
        );
        assert!(
            hermetic
                .target_dir
                .join("x86_64-unknown-linux-musl/release/mini")
                .exists()
        );
    }

    #[test]
    fn install_smoke_verifies_archive_and_creates_current_symlink() {
        let (sandbox, repo_root, binary_path) = init_release_repo("0.1.0");
        let package = run_package_at(
            &repo_root,
            &PackageOptions {
                binary: binary_path,
                debug_binary: None,
                target_triple: "x86_64-unknown-linux-gnu".to_string(),
                out_dir: PathBuf::from("dist"),
                release_channel: "stable".to_string(),
                support_tier: "gcc15_primary".to_string(),
                signing_private_key: None,
            },
        )
        .unwrap();
        let install_root = sandbox
            .path()
            .join("install")
            .join("x86_64-unknown-linux-gnu");
        let bin_dir = sandbox.path().join("bin");
        let install = run_install_at(
            &repo_root,
            &InstallOptions {
                control_dir: package.control_dir.clone(),
                install_root: install_root.clone(),
                bin_dir: bin_dir.clone(),
                expected_signing_key_id: None,
                expected_signing_public_key_sha256: None,
            },
        )
        .unwrap();

        assert_eq!(install.installed_version, "0.1.0");
        assert_eq!(install.previous_version, None);
        assert_eq!(
            current_version_name(&install_root).unwrap().as_deref(),
            Some("v0.1.0")
        );
        assert_binary_reports_version(&bin_dir.join("gcc-formed"), "0.1.0").unwrap();
        assert!(install_root.join("v0.1.0/bin/gcc-formed").exists());
        assert!(launcher_is_managed(&bin_dir.join("gcc-formed"), &install_root).unwrap());
    }

    #[test]
    fn install_rejects_control_dir_with_bad_checksums() {
        let (sandbox, repo_root, binary_path) = init_release_repo("0.1.0");
        let package = run_package_at(
            &repo_root,
            &PackageOptions {
                binary: binary_path,
                debug_binary: None,
                target_triple: "x86_64-unknown-linux-gnu".to_string(),
                out_dir: PathBuf::from("dist"),
                release_channel: "stable".to_string(),
                support_tier: "gcc15_primary".to_string(),
                signing_private_key: None,
            },
        )
        .unwrap();
        write_file(&package.shasums_path, b"deadbeef  broken\n");

        let error = run_install_at(
            &repo_root,
            &InstallOptions {
                control_dir: package.control_dir,
                install_root: sandbox
                    .path()
                    .join("install")
                    .join("x86_64-unknown-linux-gnu"),
                bin_dir: sandbox.path().join("bin"),
                expected_signing_key_id: None,
                expected_signing_public_key_sha256: None,
            },
        )
        .unwrap_err();
        assert!(error.to_string().contains("references missing file"));
    }

    #[test]
    fn signed_package_supports_pinned_signature_verification_and_system_wide_layout() {
        let (sandbox, repo_root, binary_path) = init_release_repo("0.1.0");
        let signing_private_key = sandbox.path().join("release-signing.key");
        write_signing_private_key(&signing_private_key);
        let package = run_package_at(
            &repo_root,
            &PackageOptions {
                binary: binary_path,
                debug_binary: None,
                target_triple: "x86_64-unknown-linux-musl".to_string(),
                out_dir: PathBuf::from("dist"),
                release_channel: "stable".to_string(),
                support_tier: "gcc15_primary".to_string(),
                signing_private_key: Some(signing_private_key),
            },
        )
        .unwrap();
        let signature = read_json_file::<DetachedSignatureEnvelope>(
            package
                .shasums_signature_path
                .as_deref()
                .expect("signature path missing"),
        )
        .unwrap();
        let trusted_public_key_sha256 = test_signing_public_key_sha256();
        let system_root = sandbox.path().join("system-root");
        let install_root = system_root
            .join("opt/cc-formed")
            .join("x86_64-unknown-linux-musl");
        let bin_dir = system_root.join("usr/local/bin");

        let install = run_install_at(
            &repo_root,
            &InstallOptions {
                control_dir: package.control_dir,
                install_root: install_root.clone(),
                bin_dir: bin_dir.clone(),
                expected_signing_key_id: Some(signature.key_id.clone()),
                expected_signing_public_key_sha256: Some(trusted_public_key_sha256.clone()),
            },
        )
        .unwrap();

        assert_eq!(
            install.signing_key_id.as_deref(),
            Some(signature.key_id.as_str())
        );
        assert_eq!(
            install.signing_public_key_sha256.as_deref(),
            Some(trusted_public_key_sha256.as_str())
        );
        assert_eq!(install.installed_version, "0.1.0");
        assert_eq!(
            current_version_name(&install_root).unwrap().as_deref(),
            Some("v0.1.0")
        );
        assert_binary_reports_version(&bin_dir.join("gcc-formed"), "0.1.0").unwrap();
        assert!(bin_dir.join("gcc-formed").exists());
        assert!(launcher_is_managed(&bin_dir.join("gcc-formed"), &install_root).unwrap());
    }

    #[test]
    fn install_rejects_signed_release_with_wrong_key_id() {
        let (sandbox, repo_root, binary_path) = init_release_repo("0.1.0");
        let signing_private_key = sandbox.path().join("release-signing.key");
        write_signing_private_key(&signing_private_key);
        let package = run_package_at(
            &repo_root,
            &PackageOptions {
                binary: binary_path,
                debug_binary: None,
                target_triple: "x86_64-unknown-linux-gnu".to_string(),
                out_dir: PathBuf::from("dist"),
                release_channel: "stable".to_string(),
                support_tier: "gcc15_primary".to_string(),
                signing_private_key: Some(signing_private_key),
            },
        )
        .unwrap();

        let error = run_install_at(
            &repo_root,
            &InstallOptions {
                control_dir: package.control_dir,
                install_root: sandbox
                    .path()
                    .join("install")
                    .join("x86_64-unknown-linux-gnu"),
                bin_dir: sandbox.path().join("bin"),
                expected_signing_key_id: Some("ed25519:deadbeefdeadbeef".to_string()),
                expected_signing_public_key_sha256: None,
            },
        )
        .unwrap_err();

        assert!(
            error
                .to_string()
                .contains("detached signature key mismatch")
        );
    }

    #[test]
    fn install_rejects_signed_release_with_wrong_public_key_sha() {
        let (sandbox, repo_root, binary_path) = init_release_repo("0.1.0");
        let signing_private_key = sandbox.path().join("release-signing.key");
        write_signing_private_key(&signing_private_key);
        let package = run_package_at(
            &repo_root,
            &PackageOptions {
                binary: binary_path,
                debug_binary: None,
                target_triple: "x86_64-unknown-linux-gnu".to_string(),
                out_dir: PathBuf::from("dist"),
                release_channel: "stable".to_string(),
                support_tier: "gcc15_primary".to_string(),
                signing_private_key: Some(signing_private_key),
            },
        )
        .unwrap();

        let error = run_install_at(
            &repo_root,
            &InstallOptions {
                control_dir: package.control_dir,
                install_root: sandbox
                    .path()
                    .join("install")
                    .join("x86_64-unknown-linux-gnu"),
                bin_dir: sandbox.path().join("bin"),
                expected_signing_key_id: None,
                expected_signing_public_key_sha256: Some("deadbeef".to_string()),
            },
        )
        .unwrap_err();

        assert!(
            error
                .to_string()
                .contains("detached signature public key mismatch")
        );
    }

    #[test]
    fn release_publish_promote_and_resolve_keep_same_bits() {
        let (sandbox, repo_root, binary_path) = init_release_repo("0.1.0");
        let signing_private_key = sandbox.path().join("release-signing.key");
        write_signing_private_key(&signing_private_key);
        let package = run_package_at(
            &repo_root,
            &PackageOptions {
                binary: binary_path,
                debug_binary: None,
                target_triple: "x86_64-unknown-linux-gnu".to_string(),
                out_dir: PathBuf::from("dist"),
                release_channel: "stable".to_string(),
                support_tier: "gcc15_primary".to_string(),
                signing_private_key: Some(signing_private_key),
            },
        )
        .unwrap();
        let repository_root = sandbox.path().join("release-repo");

        let publish = run_release_publish_at(
            &repo_root,
            &ReleasePublishOptions {
                control_dir: package.control_dir.clone(),
                repository_root: repository_root.clone(),
            },
        )
        .unwrap();
        let canary = run_release_promote_at(
            &repo_root,
            &ReleasePromoteOptions {
                repository_root: repository_root.clone(),
                target_triple: "x86_64-unknown-linux-gnu".to_string(),
                version: "0.1.0".to_string(),
                channel: "canary".to_string(),
            },
        )
        .unwrap();
        let stable = run_release_promote_at(
            &repo_root,
            &ReleasePromoteOptions {
                repository_root: repository_root.clone(),
                target_triple: "x86_64-unknown-linux-gnu".to_string(),
                version: "0.1.0".to_string(),
                channel: "stable".to_string(),
            },
        )
        .unwrap();
        let resolved = run_release_resolve_at(
            &repo_root,
            &ReleaseResolveOptions {
                repository_root: repository_root.clone(),
                target_triple: "x86_64-unknown-linux-gnu".to_string(),
                channel: Some("stable".to_string()),
                version: None,
            },
        )
        .unwrap();

        let published = read_published_release(&release_version_root(
            &repository_root,
            "x86_64-unknown-linux-gnu",
            "0.1.0",
        ))
        .unwrap();
        let stable_pointer =
            read_release_channel_pointer(&repository_root, "x86_64-unknown-linux-gnu", "stable")
                .unwrap();

        assert_eq!(publish.version, "0.1.0");
        assert!(publish.signing_key_id.is_some());
        assert!(publish.signing_public_key_sha256.is_some());
        assert_eq!(
            canary.primary_archive_sha256,
            publish.primary_archive_sha256
        );
        assert_eq!(
            stable.primary_archive_sha256,
            publish.primary_archive_sha256
        );
        assert_eq!(resolved.resolved_version, "0.1.0");
        assert_eq!(
            resolved.primary_archive_sha256,
            publish.primary_archive_sha256
        );
        assert_eq!(
            published.primary_archive_sha256,
            publish.primary_archive_sha256
        );
        assert_eq!(stable_pointer.version, "0.1.0");
        assert_eq!(
            stable_pointer.primary_archive_sha256,
            published.primary_archive_sha256
        );
        assert_eq!(stable_pointer.signing_key_id, publish.signing_key_id);
        assert_eq!(
            stable_pointer.signing_public_key_sha256,
            publish.signing_public_key_sha256
        );
        assert_eq!(resolved.signing_key_id, publish.signing_key_id);
        assert_eq!(
            resolved.signing_public_key_sha256,
            publish.signing_public_key_sha256
        );
        assert!(
            resolved
                .shasums_signature_path
                .as_ref()
                .is_some_and(|path| path.exists())
        );
        assert!(resolved.control_dir.exists());
        assert!(resolved.primary_archive.exists());
    }

    #[test]
    fn install_release_supports_exact_version_and_checksum_pin() {
        let (sandbox, repo_root, binary_path) = init_release_repo("0.1.0");
        let signing_private_key = sandbox.path().join("release-signing.key");
        write_signing_private_key(&signing_private_key);
        let package = run_package_at(
            &repo_root,
            &PackageOptions {
                binary: binary_path,
                debug_binary: None,
                target_triple: "x86_64-unknown-linux-gnu".to_string(),
                out_dir: PathBuf::from("dist"),
                release_channel: "stable".to_string(),
                support_tier: "gcc15_primary".to_string(),
                signing_private_key: Some(signing_private_key),
            },
        )
        .unwrap();
        let repository_root = sandbox.path().join("release-repo");
        run_release_publish_at(
            &repo_root,
            &ReleasePublishOptions {
                control_dir: package.control_dir,
                repository_root: repository_root.clone(),
            },
        )
        .unwrap();
        run_release_promote_at(
            &repo_root,
            &ReleasePromoteOptions {
                repository_root: repository_root.clone(),
                target_triple: "x86_64-unknown-linux-gnu".to_string(),
                version: "0.1.0".to_string(),
                channel: "stable".to_string(),
            },
        )
        .unwrap();
        let resolved = run_release_resolve_at(
            &repo_root,
            &ReleaseResolveOptions {
                repository_root: repository_root.clone(),
                target_triple: "x86_64-unknown-linux-gnu".to_string(),
                channel: Some("stable".to_string()),
                version: None,
            },
        )
        .unwrap();

        let install_root = sandbox
            .path()
            .join("install")
            .join("x86_64-unknown-linux-gnu");
        let bin_dir = sandbox.path().join("bin");
        let install = run_install_release_at(
            &repo_root,
            &InstallReleaseOptions {
                repository_root: repository_root.clone(),
                target_triple: "x86_64-unknown-linux-gnu".to_string(),
                install_root: install_root.clone(),
                bin_dir: bin_dir.clone(),
                channel: None,
                version: Some("0.1.0".to_string()),
                expected_primary_sha256: Some(resolved.primary_archive_sha256.clone()),
                expected_signing_key_id: resolved.signing_key_id.clone(),
                expected_signing_public_key_sha256: resolved.signing_public_key_sha256.clone(),
            },
        )
        .unwrap();

        assert_eq!(install.requested_channel, None);
        assert_eq!(install.resolved_version, "0.1.0");
        assert_eq!(install.installed_version, "0.1.0");
        assert_eq!(
            install.primary_archive_sha256,
            resolved.primary_archive_sha256
        );
        assert_eq!(install.signing_key_id, resolved.signing_key_id);
        assert_eq!(
            install.signing_public_key_sha256,
            resolved.signing_public_key_sha256
        );
        assert_eq!(
            current_version_name(&install_root).unwrap().as_deref(),
            Some("v0.1.0")
        );
        assert_binary_reports_version(&bin_dir.join("gcc-formed"), "0.1.0").unwrap();
    }

    #[test]
    fn install_release_from_channel_reports_exact_installed_version() {
        let (sandbox, repo_root, binary_path) = init_release_repo("0.1.0");
        let signing_private_key = sandbox.path().join("release-signing.key");
        write_signing_private_key(&signing_private_key);
        let package = run_package_at(
            &repo_root,
            &PackageOptions {
                binary: binary_path,
                debug_binary: None,
                target_triple: "x86_64-unknown-linux-gnu".to_string(),
                out_dir: PathBuf::from("dist"),
                release_channel: "stable".to_string(),
                support_tier: "gcc15_primary".to_string(),
                signing_private_key: Some(signing_private_key),
            },
        )
        .unwrap();
        let repository_root = sandbox.path().join("release-repo");
        run_release_publish_at(
            &repo_root,
            &ReleasePublishOptions {
                control_dir: package.control_dir,
                repository_root: repository_root.clone(),
            },
        )
        .unwrap();
        run_release_promote_at(
            &repo_root,
            &ReleasePromoteOptions {
                repository_root: repository_root.clone(),
                target_triple: "x86_64-unknown-linux-gnu".to_string(),
                version: "0.1.0".to_string(),
                channel: "stable".to_string(),
            },
        )
        .unwrap();

        let stable_pointer =
            read_release_channel_pointer(&repository_root, "x86_64-unknown-linux-gnu", "stable")
                .unwrap();
        let install = run_install_release_at(
            &repo_root,
            &InstallReleaseOptions {
                repository_root: repository_root.clone(),
                target_triple: "x86_64-unknown-linux-gnu".to_string(),
                install_root: sandbox
                    .path()
                    .join("channel-install")
                    .join("x86_64-unknown-linux-gnu"),
                bin_dir: sandbox.path().join("channel-bin"),
                channel: Some("stable".to_string()),
                version: None,
                expected_primary_sha256: None,
                expected_signing_key_id: stable_pointer.signing_key_id.clone(),
                expected_signing_public_key_sha256: stable_pointer
                    .signing_public_key_sha256
                    .clone(),
            },
        )
        .unwrap();

        assert_eq!(install.requested_channel.as_deref(), Some("stable"));
        assert_eq!(install.resolved_version, "0.1.0");
        assert_eq!(install.installed_version, "0.1.0");
    }

    #[test]
    fn install_release_rejects_mismatched_pinned_checksum() {
        let (sandbox, repo_root, binary_path) = init_release_repo("0.1.0");
        let signing_private_key = sandbox.path().join("release-signing.key");
        write_signing_private_key(&signing_private_key);
        let package = run_package_at(
            &repo_root,
            &PackageOptions {
                binary: binary_path,
                debug_binary: None,
                target_triple: "x86_64-unknown-linux-gnu".to_string(),
                out_dir: PathBuf::from("dist"),
                release_channel: "stable".to_string(),
                support_tier: "gcc15_primary".to_string(),
                signing_private_key: Some(signing_private_key),
            },
        )
        .unwrap();
        let repository_root = sandbox.path().join("release-repo");
        run_release_publish_at(
            &repo_root,
            &ReleasePublishOptions {
                control_dir: package.control_dir,
                repository_root: repository_root.clone(),
            },
        )
        .unwrap();

        let error = run_install_release_at(
            &repo_root,
            &InstallReleaseOptions {
                repository_root,
                target_triple: "x86_64-unknown-linux-gnu".to_string(),
                install_root: sandbox
                    .path()
                    .join("install")
                    .join("x86_64-unknown-linux-gnu"),
                bin_dir: sandbox.path().join("bin"),
                channel: None,
                version: Some("0.1.0".to_string()),
                expected_primary_sha256: Some("deadbeef".to_string()),
                expected_signing_key_id: None,
                expected_signing_public_key_sha256: None,
            },
        )
        .unwrap_err();

        assert!(error.to_string().contains("release checksum mismatch"));
    }

    #[test]
    fn rollback_switches_current_symlink_to_requested_version() {
        let sandbox = tempfile::tempdir().unwrap();
        let install_root = sandbox
            .path()
            .join("install")
            .join("x86_64-unknown-linux-gnu");
        let bin_dir = sandbox.path().join("bin");
        let v1 = install_root.join("v0.1.0/bin/gcc-formed");
        let v2 = install_root.join("v0.1.1/bin/gcc-formed");
        write_file(&v1, fake_wrapper_script("0.1.0").as_bytes());
        write_file(
            &install_root.join("v0.1.0/bin/g++-formed"),
            fake_wrapper_script("0.1.0").as_bytes(),
        );
        write_file(&v2, fake_wrapper_script("0.1.1").as_bytes());
        write_file(
            &install_root.join("v0.1.1/bin/g++-formed"),
            fake_wrapper_script("0.1.1").as_bytes(),
        );
        make_executable(&v1);
        make_executable(&install_root.join("v0.1.0/bin/g++-formed"));
        make_executable(&v2);
        make_executable(&install_root.join("v0.1.1/bin/g++-formed"));
        ensure_launcher_symlinks(&bin_dir, &install_root).unwrap();
        swap_symlink(&install_root.join("current"), Path::new("v0.1.1"), true).unwrap();

        let rollback = run_rollback_at(
            sandbox.path(),
            &RollbackOptions {
                install_root: install_root.clone(),
                bin_dir: bin_dir.clone(),
                version: "0.1.0".to_string(),
            },
        )
        .unwrap();

        assert_eq!(rollback.active_version, "0.1.0");
        assert_eq!(
            current_version_name(&install_root).unwrap().as_deref(),
            Some("v0.1.0")
        );
        assert_binary_reports_version(&bin_dir.join("gcc-formed"), "0.1.0").unwrap();
    }

    #[test]
    fn purge_uninstall_removes_install_bits_without_touching_state() {
        let sandbox = tempfile::tempdir().unwrap();
        let install_root = sandbox
            .path()
            .join("install")
            .join("x86_64-unknown-linux-gnu");
        let bin_dir = sandbox.path().join("bin");
        let state_root = sandbox.path().join("state");
        write_file(
            &install_root.join("v0.1.0/bin/gcc-formed"),
            fake_wrapper_script("0.1.0").as_bytes(),
        );
        write_file(
            &install_root.join("v0.1.0/bin/g++-formed"),
            fake_wrapper_script("0.1.0").as_bytes(),
        );
        make_executable(&install_root.join("v0.1.0/bin/gcc-formed"));
        make_executable(&install_root.join("v0.1.0/bin/g++-formed"));
        ensure_launcher_symlinks(&bin_dir, &install_root).unwrap();
        swap_symlink(&install_root.join("current"), Path::new("v0.1.0"), true).unwrap();
        write_file(&state_root.join("trace.json"), b"keep me\n");

        let uninstall = run_uninstall_at(
            sandbox.path(),
            &UninstallOptions {
                install_root: install_root.clone(),
                bin_dir: bin_dir.clone(),
                mode: UninstallMode::PurgeInstall,
                version: None,
                state_root: Some(state_root.clone()),
                purge_state: false,
            },
        )
        .unwrap();

        assert_eq!(uninstall.removed_versions, vec!["0.1.0".to_string()]);
        assert!(
            uninstall
                .removed_launchers
                .contains(&"gcc-formed".to_string())
        );
        assert!(!install_root.exists());
        assert!(state_root.exists());
    }
}
