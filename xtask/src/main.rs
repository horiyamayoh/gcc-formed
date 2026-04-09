mod commands;
mod util;

use crate::commands::{corpus::*, fuzz::*, human_eval::*, rc_gate::*, release::*, stable::*};
use crate::util::process::run;
use clap::{Parser, Subcommand, ValueEnum};
#[cfg(test)]
use diag_trace::{BuildManifest, DEFAULT_PRODUCT_NAME};
use std::path::PathBuf;

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
        #[arg(long)]
        dry_run: bool,
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
        #[arg(long)]
        dry_run: bool,
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
        #[arg(long)]
        dry_run: bool,
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
        #[arg(long, value_enum, default_value_t = SnapshotSubset::All)]
        subset: SnapshotSubset,
        #[arg(long)]
        report_dir: Option<PathBuf>,
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
        #[arg(long)]
        report_dir: Option<PathBuf>,
    },
    BenchSmoke {
        #[arg(long, default_value = "corpus")]
        root: PathBuf,
        #[arg(long, value_enum, default_value_t = SnapshotSubset::Representative)]
        subset: SnapshotSubset,
        #[arg(long)]
        report_dir: Option<PathBuf>,
    },
    FuzzSmoke {
        #[arg(long, default_value = "fuzz")]
        root: PathBuf,
        #[arg(long)]
        report_dir: Option<PathBuf>,
    },
    HumanEvalKit {
        #[arg(long, default_value = "corpus")]
        root: PathBuf,
        #[arg(long, default_value = "target/human-eval")]
        report_dir: PathBuf,
    },
    RcGate {
        #[arg(long, default_value = "corpus")]
        root: PathBuf,
        #[arg(long, default_value = "target/rc-gate")]
        report_dir: PathBuf,
        #[arg(long, default_value = "eval/rc/metrics-manual-eval.json")]
        metrics_manual_report: PathBuf,
        #[arg(long, default_value = "eval/rc/issue-budget.json")]
        issue_budget_report: PathBuf,
        #[arg(long, default_value = "fuzz")]
        fuzz_root: PathBuf,
        #[arg(long)]
        fuzz_report: Option<PathBuf>,
        #[arg(long, default_value = "eval/rc/ux-signoff.json")]
        ux_signoff_report: PathBuf,
        #[arg(long)]
        allow_pending_manual_checks: bool,
    },
    StableRelease {
        #[arg(long)]
        control_dir: PathBuf,
        #[arg(long)]
        repository_root: PathBuf,
        #[arg(long)]
        target_triple: String,
        #[arg(long)]
        install_root: PathBuf,
        #[arg(long)]
        bin_dir: PathBuf,
        #[arg(long, default_value = "target/stable-release")]
        report_dir: PathBuf,
        #[arg(long)]
        rollback_baseline_version: Option<String>,
    },
    SelfCheck,
}

#[derive(Debug, Clone, Copy, ValueEnum, PartialEq, Eq)]
enum SnapshotSubset {
    All,
    Representative,
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
            dry_run,
        } => {
            let install = run_install(InstallOptions {
                control_dir,
                install_root,
                bin_dir,
                expected_signing_key_id,
                expected_signing_public_key_sha256,
                dry_run,
            })?;
            println!("{}", serde_json::to_string_pretty(&install)?);
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
            dry_run,
        } => {
            let rollback = run_rollback(RollbackOptions {
                install_root,
                bin_dir,
                version,
                dry_run,
            })?;
            println!("{}", serde_json::to_string_pretty(&rollback)?);
        }
        Commands::Uninstall {
            install_root,
            bin_dir,
            mode,
            version,
            state_root,
            purge_state,
            dry_run,
        } => {
            let uninstall = run_uninstall(UninstallOptions {
                install_root,
                bin_dir,
                mode,
                version,
                state_root,
                purge_state,
                dry_run,
            })?;
            println!("{}", serde_json::to_string_pretty(&uninstall)?);
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
            subset,
            report_dir,
        } => run_replay(
            &root,
            fixture.as_deref(),
            family.as_deref(),
            subset,
            report_dir.as_deref(),
        )?,
        Commands::Snapshot {
            root,
            fixture,
            family,
            subset,
            check,
            docker_image,
            report_dir,
        } => run_snapshot(
            &root,
            fixture.as_deref(),
            family.as_deref(),
            subset,
            check,
            &docker_image,
            report_dir.as_deref(),
        )?,
        Commands::BenchSmoke {
            root,
            subset,
            report_dir,
        } => {
            let report = run_bench_smoke(&root, subset, report_dir.as_deref())?;
            println!("{}", serde_json::to_string_pretty(&report)?);
            if report.overall_status == GateStatus::Fail {
                return Err("bench smoke budgets failed".into());
            }
        }
        Commands::FuzzSmoke { root, report_dir } => {
            let report = run_fuzz_smoke(&root, report_dir.as_deref())?;
            println!("{}", serde_json::to_string_pretty(&report)?);
            if report.overall_status == FuzzSmokeStatus::Fail {
                return Err("fuzz smoke failed".into());
            }
        }
        Commands::HumanEvalKit { root, report_dir } => {
            let report = run_human_eval_kit(&root, &report_dir)?;
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({
                    "root": report.root,
                    "report_dir": report.report_dir,
                    "expert_review_fixture_count": report.expert_review_fixture_count,
                    "task_study_fixture_count": report.task_study_fixture_count,
                    "family_counts": report.family_counts,
                    "covered_required_families": report.covered_required_families,
                    "missing_required_families": report.missing_required_families,
                }))?
            );
        }
        Commands::RcGate {
            root,
            report_dir,
            metrics_manual_report,
            issue_budget_report,
            fuzz_root,
            fuzz_report,
            ux_signoff_report,
            allow_pending_manual_checks,
        } => {
            let report = run_rc_gate(RcGateOptions {
                root,
                report_dir,
                metrics_manual_report,
                issue_budget_report,
                fuzz_root,
                fuzz_report,
                ux_signoff_report,
                allow_pending_manual_checks,
            })?;
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({
                    "overall_status": report.overall_status,
                    "blocker_count": report.blockers.len(),
                    "report_dir": report.report_dir,
                }))?
            );
            if report.overall_status == GateStatus::Fail {
                return Err("release candidate gate failed".into());
            }
        }
        Commands::StableRelease {
            control_dir,
            repository_root,
            target_triple,
            install_root,
            bin_dir,
            report_dir,
            rollback_baseline_version,
        } => {
            let report = run_stable_release(StableReleaseOptions {
                control_dir,
                repository_root,
                target_triple,
                install_root,
                bin_dir,
                report_dir,
                rollback_baseline_version,
            })?;
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({
                    "candidate_version": report.candidate_version,
                    "rollback_baseline_version": report.rollback_drill.baseline_version,
                    "metadata_only_promotion": report.no_rebuild_evidence.metadata_only_promotion,
                    "report_dir": report.report_dir,
                    "report_path": report.report_path,
                    "summary_path": report.summary_path,
                }))?
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

#[cfg(test)]
mod tests;
