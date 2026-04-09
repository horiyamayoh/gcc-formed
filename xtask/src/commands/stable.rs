use crate::commands::release::{
    InstallReleaseOptions, InstallReleaseOutput, PublishedRelease, ReleasePromoteOptions,
    ReleasePromoteOutput, ReleasePublishOptions, ReleasePublishOutput, ReleaseResolveOptions,
    ReleaseResolveOutput, RollbackOptions, RollbackOutput, current_version_name,
    installed_versions, read_release_channel_pointer, run_install_release_at,
    run_release_promote_at, run_release_publish_at, run_release_resolve_at, run_rollback_at,
    verify_published_release,
};
use serde::Serialize;
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

const STABLE_RELEASE_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone)]
pub(crate) struct StableReleaseOptions {
    pub(crate) control_dir: PathBuf,
    pub(crate) repository_root: PathBuf,
    pub(crate) target_triple: String,
    pub(crate) install_root: PathBuf,
    pub(crate) bin_dir: PathBuf,
    pub(crate) report_dir: PathBuf,
    pub(crate) rollback_baseline_version: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct StableChannelReport {
    pub(crate) channel: String,
    pub(crate) promote: ReleasePromoteOutput,
    pub(crate) resolve: ReleaseResolveOutput,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct NoRebuildEvidence {
    pub(crate) metadata_only_promotion: bool,
    pub(crate) published_primary_archive_sha256: String,
    pub(crate) published_manifest_sha256: String,
    pub(crate) published_shasums_sha256: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) signing_key_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) signing_public_key_sha256: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(crate) drift: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct RollbackDrillReport {
    pub(crate) baseline_version: String,
    pub(crate) candidate_version: String,
    pub(crate) install_root: PathBuf,
    pub(crate) bin_dir: PathBuf,
    pub(crate) baseline_release: ReleaseResolveOutput,
    pub(crate) candidate_release: ReleaseResolveOutput,
    pub(crate) baseline_install: InstallReleaseOutput,
    pub(crate) candidate_install: InstallReleaseOutput,
    pub(crate) rollback: RollbackOutput,
    pub(crate) installed_versions_before_rollback: Vec<String>,
    pub(crate) installed_versions_after_rollback: Vec<String>,
    pub(crate) pre_rollback_current_version: Option<String>,
    pub(crate) post_rollback_current_version: Option<String>,
    pub(crate) rollback_swap_symlink_count: usize,
    pub(crate) symlink_only_switch: bool,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct StableReleaseReport {
    pub(crate) schema_version: u32,
    pub(crate) generated_at_unix_seconds: u64,
    pub(crate) candidate_version: String,
    pub(crate) target_triple: String,
    pub(crate) control_dir: PathBuf,
    pub(crate) repository_root: PathBuf,
    pub(crate) report_dir: PathBuf,
    pub(crate) report_path: PathBuf,
    pub(crate) summary_path: PathBuf,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) previous_stable_version_before_promote: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) requested_rollback_baseline_version: Option<String>,
    pub(crate) release_publish: ReleasePublishOutput,
    pub(crate) published_release: PublishedRelease,
    pub(crate) canary: StableChannelReport,
    pub(crate) beta: StableChannelReport,
    pub(crate) stable: StableChannelReport,
    pub(crate) no_rebuild_evidence: NoRebuildEvidence,
    pub(crate) rollback_drill: RollbackDrillReport,
}

pub(crate) fn run_stable_release(
    options: StableReleaseOptions,
) -> Result<StableReleaseReport, Box<dyn std::error::Error>> {
    run_stable_release_at(&std::env::current_dir()?, &options)
}

pub(crate) fn run_stable_release_at(
    base_dir: &Path,
    options: &StableReleaseOptions,
) -> Result<StableReleaseReport, Box<dyn std::error::Error>> {
    let control_dir = resolve_path(base_dir, &options.control_dir);
    let repository_root = resolve_path(base_dir, &options.repository_root);
    let install_root = resolve_path(base_dir, &options.install_root);
    let bin_dir = resolve_path(base_dir, &options.bin_dir);
    let report_dir = resolve_path(base_dir, &options.report_dir);
    fs::create_dir_all(&report_dir)?;

    let previous_stable_version_before_promote =
        read_release_channel_pointer(&repository_root, &options.target_triple, "stable")
            .ok()
            .map(|pointer| pointer.version);

    let release_publish = run_release_publish_at(
        base_dir,
        &ReleasePublishOptions {
            control_dir: control_dir.clone(),
            repository_root: repository_root.clone(),
        },
    )?;
    let candidate_version = release_publish.version.clone();
    let published_release =
        verify_published_release(&repository_root, &options.target_triple, &candidate_version)?;

    let canary = promote_and_resolve(
        base_dir,
        &repository_root,
        &options.target_triple,
        &candidate_version,
        "canary",
    )?;
    let beta = promote_and_resolve(
        base_dir,
        &repository_root,
        &options.target_triple,
        &candidate_version,
        "beta",
    )?;
    let stable = promote_and_resolve(
        base_dir,
        &repository_root,
        &options.target_triple,
        &candidate_version,
        "stable",
    )?;

    let no_rebuild_evidence = build_no_rebuild_evidence(
        &release_publish,
        &published_release,
        [&canary, &beta, &stable],
    );
    if !no_rebuild_evidence.metadata_only_promotion {
        return Err(format!(
            "stable release promotion drifted from published bits: {}",
            no_rebuild_evidence.drift.join("; ")
        )
        .into());
    }

    let rollback_baseline_version = determine_rollback_baseline(
        options.rollback_baseline_version.as_deref(),
        previous_stable_version_before_promote.as_deref(),
        &candidate_version,
    )?;
    ensure_empty_drill_roots(&install_root, &bin_dir)?;
    let rollback_drill = run_rollback_drill(
        base_dir,
        &repository_root,
        &options.target_triple,
        &install_root,
        &bin_dir,
        &rollback_baseline_version,
        &stable.resolve,
    )?;

    let report_path = report_dir.join("stable-release-report.json");
    let summary_path = report_dir.join("stable-release-summary.md");
    let report = StableReleaseReport {
        schema_version: STABLE_RELEASE_SCHEMA_VERSION,
        generated_at_unix_seconds: unix_now_seconds(),
        candidate_version,
        target_triple: options.target_triple.clone(),
        control_dir,
        repository_root,
        report_dir: report_dir.clone(),
        report_path: report_path.clone(),
        summary_path: summary_path.clone(),
        previous_stable_version_before_promote,
        requested_rollback_baseline_version: options.rollback_baseline_version.clone(),
        release_publish,
        published_release,
        canary,
        beta,
        stable,
        no_rebuild_evidence,
        rollback_drill,
    };

    fs::write(&report_path, serde_json::to_vec_pretty(&report)?)?;
    fs::write(
        report_dir.join("promotion-evidence.json"),
        serde_json::to_vec_pretty(&report.no_rebuild_evidence)?,
    )?;
    fs::write(
        report_dir.join("rollback-drill.json"),
        serde_json::to_vec_pretty(&report.rollback_drill)?,
    )?;
    fs::write(&summary_path, build_summary(&report))?;

    Ok(report)
}

fn promote_and_resolve(
    base_dir: &Path,
    repository_root: &Path,
    target_triple: &str,
    version: &str,
    channel: &str,
) -> Result<StableChannelReport, Box<dyn std::error::Error>> {
    let promote = run_release_promote_at(
        base_dir,
        &ReleasePromoteOptions {
            repository_root: repository_root.to_path_buf(),
            target_triple: target_triple.to_string(),
            version: version.to_string(),
            channel: channel.to_string(),
        },
    )?;
    let resolve = run_release_resolve_at(
        base_dir,
        &ReleaseResolveOptions {
            repository_root: repository_root.to_path_buf(),
            target_triple: target_triple.to_string(),
            channel: Some(channel.to_string()),
            version: None,
        },
    )?;
    Ok(StableChannelReport {
        channel: channel.to_string(),
        promote,
        resolve,
    })
}

fn build_no_rebuild_evidence(
    release_publish: &ReleasePublishOutput,
    published_release: &PublishedRelease,
    channels: [&StableChannelReport; 3],
) -> NoRebuildEvidence {
    let mut drift = Vec::new();
    for channel in channels {
        if channel.promote.version != release_publish.version {
            drift.push(format!(
                "{} version mismatch: expected {}, got {}",
                channel.channel, release_publish.version, channel.promote.version
            ));
        }
        if channel.promote.primary_archive_sha256 != published_release.primary_archive_sha256 {
            drift.push(format!(
                "{} primary checksum mismatch: expected {}, got {}",
                channel.channel,
                published_release.primary_archive_sha256,
                channel.promote.primary_archive_sha256
            ));
        }
        if channel.resolve.resolved_version != release_publish.version {
            drift.push(format!(
                "{} resolve version mismatch: expected {}, got {}",
                channel.channel, release_publish.version, channel.resolve.resolved_version
            ));
        }
        if channel.resolve.primary_archive_sha256 != published_release.primary_archive_sha256 {
            drift.push(format!(
                "{} resolve primary checksum mismatch: expected {}, got {}",
                channel.channel,
                published_release.primary_archive_sha256,
                channel.resolve.primary_archive_sha256
            ));
        }
        if channel.resolve.manifest_sha256 != published_release.manifest_sha256 {
            drift.push(format!(
                "{} resolve manifest checksum mismatch: expected {}, got {}",
                channel.channel, published_release.manifest_sha256, channel.resolve.manifest_sha256
            ));
        }
        if channel.resolve.shasums_sha256 != published_release.shasums_sha256 {
            drift.push(format!(
                "{} resolve shasums checksum mismatch: expected {}, got {}",
                channel.channel, published_release.shasums_sha256, channel.resolve.shasums_sha256
            ));
        }
        if channel.resolve.signing_key_id != published_release.signing_key_id {
            drift.push(format!(
                "{} signing key mismatch: expected {:?}, got {:?}",
                channel.channel, published_release.signing_key_id, channel.resolve.signing_key_id
            ));
        }
        if channel.resolve.signing_public_key_sha256 != published_release.signing_public_key_sha256
        {
            drift.push(format!(
                "{} signing public key mismatch: expected {:?}, got {:?}",
                channel.channel,
                published_release.signing_public_key_sha256,
                channel.resolve.signing_public_key_sha256
            ));
        }
    }

    NoRebuildEvidence {
        metadata_only_promotion: drift.is_empty(),
        published_primary_archive_sha256: published_release.primary_archive_sha256.clone(),
        published_manifest_sha256: published_release.manifest_sha256.clone(),
        published_shasums_sha256: published_release.shasums_sha256.clone(),
        signing_key_id: published_release.signing_key_id.clone(),
        signing_public_key_sha256: published_release.signing_public_key_sha256.clone(),
        drift,
    }
}

fn determine_rollback_baseline(
    requested_baseline_version: Option<&str>,
    previous_stable_version: Option<&str>,
    candidate_version: &str,
) -> Result<String, Box<dyn std::error::Error>> {
    let baseline = requested_baseline_version
        .map(str::to_string)
        .or_else(|| {
            previous_stable_version.and_then(|version| {
                if version == candidate_version {
                    None
                } else {
                    Some(version.to_string())
                }
            })
        })
        .ok_or_else(|| {
            "stable release automation requires --rollback-baseline-version or an existing stable channel baseline".to_string()
        })?;
    if baseline == candidate_version {
        return Err(format!(
            "rollback baseline version must differ from candidate version: {candidate_version}"
        )
        .into());
    }
    Ok(baseline)
}

fn ensure_empty_drill_roots(
    install_root: &Path,
    bin_dir: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    if current_version_name(install_root)?.is_some()
        || !installed_versions(install_root)?.is_empty()
    {
        return Err(format!(
            "stable release drill install root must start empty: {}",
            install_root.display()
        )
        .into());
    }
    if bin_dir.exists() && fs::read_dir(bin_dir)?.next().is_some() {
        return Err(format!(
            "stable release drill bin dir must start empty: {}",
            bin_dir.display()
        )
        .into());
    }
    Ok(())
}

fn run_rollback_drill(
    base_dir: &Path,
    repository_root: &Path,
    target_triple: &str,
    install_root: &Path,
    bin_dir: &Path,
    baseline_version: &str,
    candidate_release: &ReleaseResolveOutput,
) -> Result<RollbackDrillReport, Box<dyn std::error::Error>> {
    let baseline_release = run_release_resolve_at(
        base_dir,
        &ReleaseResolveOptions {
            repository_root: repository_root.to_path_buf(),
            target_triple: target_triple.to_string(),
            channel: None,
            version: Some(baseline_version.to_string()),
        },
    )?;
    let baseline_install = run_install_release_at(
        base_dir,
        &InstallReleaseOptions {
            repository_root: repository_root.to_path_buf(),
            target_triple: target_triple.to_string(),
            install_root: install_root.to_path_buf(),
            bin_dir: bin_dir.to_path_buf(),
            channel: None,
            version: Some(baseline_version.to_string()),
            expected_primary_sha256: Some(baseline_release.primary_archive_sha256.clone()),
            expected_signing_key_id: baseline_release.signing_key_id.clone(),
            expected_signing_public_key_sha256: baseline_release.signing_public_key_sha256.clone(),
        },
    )?;
    let candidate_install = run_install_release_at(
        base_dir,
        &InstallReleaseOptions {
            repository_root: repository_root.to_path_buf(),
            target_triple: target_triple.to_string(),
            install_root: install_root.to_path_buf(),
            bin_dir: bin_dir.to_path_buf(),
            channel: None,
            version: Some(candidate_release.resolved_version.clone()),
            expected_primary_sha256: Some(candidate_release.primary_archive_sha256.clone()),
            expected_signing_key_id: candidate_release.signing_key_id.clone(),
            expected_signing_public_key_sha256: candidate_release.signing_public_key_sha256.clone(),
        },
    )?;
    let installed_versions_before_rollback = installed_versions(install_root)?;
    let pre_rollback_current_version =
        current_version_name(install_root)?.map(|version| strip_version_prefix(&version));
    let rollback = run_rollback_at(
        base_dir,
        &RollbackOptions {
            install_root: install_root.to_path_buf(),
            bin_dir: bin_dir.to_path_buf(),
            version: baseline_version.to_string(),
            dry_run: false,
        },
    )?;
    let installed_versions_after_rollback = installed_versions(install_root)?;
    let post_rollback_current_version =
        current_version_name(install_root)?.map(|version| strip_version_prefix(&version));
    let rollback_swap_symlink_count = rollback
        .planned_actions
        .iter()
        .filter(|action| action.action == "swap_symlink")
        .count();
    let symlink_only_switch = rollback_swap_symlink_count == 1
        && rollback.planned_actions.len() == 1
        && rollback
            .planned_actions
            .first()
            .is_some_and(|action| action.path == install_root.join("current"));
    if !symlink_only_switch {
        return Err(format!(
            "rollback drill expected exactly one current symlink swap, saw {:?}",
            rollback.planned_actions
        )
        .into());
    }

    Ok(RollbackDrillReport {
        baseline_version: baseline_version.to_string(),
        candidate_version: candidate_release.resolved_version.clone(),
        install_root: install_root.to_path_buf(),
        bin_dir: bin_dir.to_path_buf(),
        baseline_release,
        candidate_release: candidate_release.clone(),
        baseline_install,
        candidate_install,
        rollback,
        installed_versions_before_rollback,
        installed_versions_after_rollback,
        pre_rollback_current_version,
        post_rollback_current_version,
        rollback_swap_symlink_count,
        symlink_only_switch,
    })
}

fn build_summary(report: &StableReleaseReport) -> String {
    let mut markdown = String::new();
    let _ = writeln!(&mut markdown, "# Stable Release Summary");
    let _ = writeln!(&mut markdown);
    let _ = writeln!(
        &mut markdown,
        "- candidate version: `{}`",
        report.candidate_version
    );
    let _ = writeln!(
        &mut markdown,
        "- rollback baseline version: `{}`",
        report.rollback_drill.baseline_version
    );
    let _ = writeln!(&mut markdown, "- target triple: `{}`", report.target_triple);
    let _ = writeln!(
        &mut markdown,
        "- metadata-only promotion: `{}`",
        report.no_rebuild_evidence.metadata_only_promotion
    );
    let _ = writeln!(
        &mut markdown,
        "- published primary sha256: `{}`",
        report.no_rebuild_evidence.published_primary_archive_sha256
    );
    let _ = writeln!(
        &mut markdown,
        "- published manifest sha256: `{}`",
        report.no_rebuild_evidence.published_manifest_sha256
    );
    let _ = writeln!(
        &mut markdown,
        "- published shasums sha256: `{}`",
        report.no_rebuild_evidence.published_shasums_sha256
    );
    if let Some(key_id) = &report.no_rebuild_evidence.signing_key_id {
        let _ = writeln!(&mut markdown, "- signing key id: `{key_id}`");
    }
    if let Some(public_key_sha256) = &report.no_rebuild_evidence.signing_public_key_sha256 {
        let _ = writeln!(
            &mut markdown,
            "- signing public key sha256: `{public_key_sha256}`"
        );
    }
    let _ = writeln!(&mut markdown);
    let _ = writeln!(&mut markdown, "## Channel Resolves");
    let _ = writeln!(
        &mut markdown,
        "- canary: `{}` / `{}`",
        report.canary.resolve.resolved_version, report.canary.resolve.primary_archive_sha256
    );
    let _ = writeln!(
        &mut markdown,
        "- beta: `{}` / `{}`",
        report.beta.resolve.resolved_version, report.beta.resolve.primary_archive_sha256
    );
    let _ = writeln!(
        &mut markdown,
        "- stable: `{}` / `{}`",
        report.stable.resolve.resolved_version, report.stable.resolve.primary_archive_sha256
    );
    let _ = writeln!(&mut markdown);
    let _ = writeln!(&mut markdown, "## Rollback Drill");
    let _ = writeln!(
        &mut markdown,
        "- pre-rollback current version: `{}`",
        report
            .rollback_drill
            .pre_rollback_current_version
            .as_deref()
            .unwrap_or("unknown")
    );
    let _ = writeln!(
        &mut markdown,
        "- post-rollback current version: `{}`",
        report
            .rollback_drill
            .post_rollback_current_version
            .as_deref()
            .unwrap_or("unknown")
    );
    let _ = writeln!(
        &mut markdown,
        "- swap_symlink operations during rollback: `{}`",
        report.rollback_drill.rollback_swap_symlink_count
    );
    let _ = writeln!(
        &mut markdown,
        "- symlink-only switch: `{}`",
        report.rollback_drill.symlink_only_switch
    );
    markdown
}

fn resolve_path(base_dir: &Path, path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        base_dir.join(path)
    }
}

fn strip_version_prefix(version: &str) -> String {
    version.trim_start_matches('v').to_string()
}

fn unix_now_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}
