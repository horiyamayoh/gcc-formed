use crate::SnapshotSubset;
use diag_adapter_gcc::{ingest_with_reason, producer_for_version, tool_for_backend};
use diag_core::{
    ArtifactKind, ArtifactStorage, CaptureArtifact, DiagnosticDocument, FallbackReason,
    LanguageMode, Ownership, RunInfo, SnapshotKind, WrapperSurface, snapshot_json,
};
use diag_enrich::enrich_document;
use diag_render::{
    DebugRefs, PathPolicy, RenderCapabilities, RenderProfile, RenderRequest, SourceExcerptPolicy,
    StreamKind, TypeDisplayPolicy, WarningVisibility, build_view_model, render,
};
use diag_testkit::{
    ExpectedFallback, Fixture, RenderProfileExpectations, SnapshotDiffKind,
    compare_snapshot_contents, discover, family_counts, normalize_snapshot_contents,
    validate_fixture,
};
use serde::Serialize;
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;
use std::process::Command;
use std::time::Instant;

pub(crate) const REPRESENTATIVE_FIXTURES: &[&str] = &[
    "c/syntax/case-01",
    "c/syntax/case-02",
    "c/syntax/case-05",
    "c/macro_include/case-01",
    "c/macro_include/case-03",
    "c/macro_include/case-10",
    "cpp/template/case-01",
    "cpp/template/case-02",
    "cpp/template/case-05",
    "c/type/case-01",
    "cpp/overload/case-01",
    "cpp/overload/case-02",
    "c/linker/case-01",
    "c/linker/case-02",
    "c/linker/case-03",
];

pub(crate) const MINIMUM_CURATED_CORPUS_SIZE: usize = 80;
pub(crate) const MAXIMUM_CURATED_CORPUS_SIZE: usize = 120;

#[derive(Debug, Clone, Serialize)]
pub(crate) struct VerificationFailure {
    pub(crate) layer: String,
    pub(crate) fixture_id: String,
    pub(crate) summary: String,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct AcceptanceFixtureSummary {
    pub(crate) fixture_id: String,
    pub(crate) family_key: String,
    pub(crate) title: Option<String>,
    pub(crate) expected_family: Option<String>,
    pub(crate) actual_family: String,
    pub(crate) family_match: bool,
    pub(crate) used_fallback: bool,
    pub(crate) fallback_reason: Option<FallbackReason>,
    pub(crate) fallback_forbidden: bool,
    pub(crate) unexpected_fallback: bool,
    pub(crate) primary_location_path: Option<String>,
    pub(crate) primary_location_user_owned_required: bool,
    pub(crate) primary_location_user_owned: bool,
    pub(crate) missing_required_primary_location: bool,
    pub(crate) first_action_required: bool,
    pub(crate) first_action_present: bool,
    pub(crate) missing_required_first_action: bool,
    pub(crate) headline_rewritten: bool,
    pub(crate) parse_time_ms: u64,
    pub(crate) render_time_ms: u64,
    pub(crate) verified: bool,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct AcceptanceMetrics {
    pub(crate) promoted_fixture_count: usize,
    pub(crate) fallback_used_count: usize,
    pub(crate) fallback_forbidden_count: usize,
    pub(crate) unexpected_fallback_count: usize,
    pub(crate) fallback_reason_counts: BTreeMap<String, usize>,
    pub(crate) unexpected_fallback_reason_counts: BTreeMap<String, usize>,
    pub(crate) primary_location_user_owned_required_count: usize,
    pub(crate) primary_location_user_owned_count: usize,
    pub(crate) missing_required_primary_location_count: usize,
    pub(crate) first_action_required_count: usize,
    pub(crate) first_action_present_count: usize,
    pub(crate) missing_required_first_action_count: usize,
    pub(crate) headline_rewritten_count: usize,
    pub(crate) family_expected_count: usize,
    pub(crate) family_match_count: usize,
    pub(crate) fallback_rate: f64,
    pub(crate) primary_location_user_owned_rate: f64,
    pub(crate) first_action_present_rate: f64,
    pub(crate) headline_rewritten_rate: f64,
    pub(crate) family_match_rate: f64,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct ReplayReport {
    pub(crate) family_counts: BTreeMap<String, usize>,
    pub(crate) selected_family_counts: BTreeMap<String, usize>,
    pub(crate) selected_fixture_count: usize,
    pub(crate) promoted_fixture_count: usize,
    pub(crate) promoted_verified: usize,
    pub(crate) promoted_failed: usize,
    pub(crate) subset: String,
    pub(crate) metrics: AcceptanceMetrics,
    pub(crate) fixtures: Vec<AcceptanceFixtureSummary>,
    pub(crate) failures: Vec<VerificationFailure>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct SnapshotReport {
    pub(crate) selected_fixture_count: usize,
    pub(crate) promoted_fixture_count: usize,
    pub(crate) successful_fixture_count: usize,
    pub(crate) check_only: bool,
    pub(crate) subset: String,
    pub(crate) docker_image: String,
    pub(crate) drift_metrics: SnapshotDriftMetrics,
    pub(crate) fallback_reason_counts: BTreeMap<String, usize>,
    pub(crate) fixtures: Vec<SnapshotFixtureReport>,
    pub(crate) failures: Vec<VerificationFailure>,
}

#[derive(Debug, Clone, Default, Serialize)]
pub(crate) struct SnapshotDriftMetrics {
    pub(crate) exact_count: usize,
    pub(crate) normalization_only_count: usize,
    pub(crate) semantic_count: usize,
    pub(crate) missing_expected_count: usize,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct SnapshotArtifactDiff {
    pub(crate) path: String,
    pub(crate) diff_kind: SnapshotDiffKind,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct SnapshotFixtureReport {
    pub(crate) fixture_id: String,
    pub(crate) family_key: String,
    pub(crate) fallback_reason: Option<FallbackReason>,
    pub(crate) artifact_diffs: Vec<SnapshotArtifactDiff>,
}

#[derive(Debug)]
pub(crate) struct SnapshotFixtureOutcome {
    pub(crate) report: SnapshotFixtureReport,
    pub(crate) check_failure: Option<VerificationFailure>,
}

#[derive(Debug)]
pub(crate) struct CapturedIngress {
    pub(crate) stderr_text: String,
    pub(crate) sarif_text: String,
}

pub(crate) fn run_replay(
    root: &Path,
    fixture_filter: Option<&str>,
    family_filter: Option<&str>,
    subset: SnapshotSubset,
    report_dir: Option<&Path>,
) -> Result<(), Box<dyn std::error::Error>> {
    let report = build_replay_report(root, fixture_filter, family_filter, subset, report_dir)?;
    if !report.failures.is_empty() {
        report_failures("replay", &report.failures);
        return Err("replay verification failed".into());
    }

    println!(
        "{}",
        serde_json::to_string_pretty(&serde_json::json!({
            "family_counts": report.family_counts,
            "selected_family_counts": report.selected_family_counts,
            "selected_fixture_count": report.selected_fixture_count,
            "promoted_verified": report.promoted_verified,
            "promoted_fixture_count": report.promoted_fixture_count,
            "subset": report.subset,
            "metrics": report.metrics,
            "mode": "replay"
        }))?
    );
    Ok(())
}

pub(crate) fn build_replay_report(
    root: &Path,
    fixture_filter: Option<&str>,
    family_filter: Option<&str>,
    subset: SnapshotSubset,
    report_dir: Option<&Path>,
) -> Result<ReplayReport, Box<dyn std::error::Error>> {
    let fixtures = discover(root)?;
    for fixture in &fixtures {
        validate_fixture(fixture)?;
    }
    let counts = family_counts(&fixtures);
    enforce_minimum_corpus_shape(fixtures.len(), &counts)?;
    let selected = select_fixtures(&fixtures, fixture_filter, family_filter, subset);
    if selected.is_empty() {
        return Err("no fixtures matched replay selection".into());
    }
    let selected_family_counts = family_counts_for_selected(&selected);

    let mut failures = Vec::new();
    let mut promoted_verified = 0usize;
    let mut summaries = Vec::new();
    for fixture in &selected {
        if fixture.is_promoted() {
            match collect_acceptance_fixture_summary(fixture, report_dir) {
                Ok(mut summary) => match verify_promoted_fixture(fixture) {
                    Ok(_) => {
                        summary.verified = true;
                        promoted_verified += 1;
                        summaries.push(summary);
                    }
                    Err(failure) => {
                        summary.verified = false;
                        summaries.push(summary);
                        failures.push(failure);
                    }
                },
                Err(failure) => failures.push(failure),
            }
        }
    }

    let report = ReplayReport {
        family_counts: counts.clone(),
        selected_family_counts,
        selected_fixture_count: selected.len(),
        promoted_fixture_count: summaries.len(),
        promoted_verified,
        promoted_failed: summaries.len().saturating_sub(promoted_verified),
        subset: subset_name(subset).to_string(),
        metrics: acceptance_metrics_for(&summaries),
        fixtures: summaries,
        failures: failures.clone(),
    };
    if let Some(report_dir) = report_dir {
        write_replay_report(report_dir, &report)?;
    }
    Ok(report)
}

pub(crate) fn run_snapshot(
    root: &Path,
    fixture_filter: Option<&str>,
    family_filter: Option<&str>,
    subset: SnapshotSubset,
    check: bool,
    docker_image: &str,
    report_dir: Option<&Path>,
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
    let promoted_count = promoted.len();

    let mut failures = Vec::new();
    let mut updated = 0usize;
    let mut fixture_reports = Vec::new();
    for fixture in promoted {
        if let Err(error) = validate_snapshot_inputs(fixture) {
            failures.push(VerificationFailure {
                layer: "fixture_layout".to_string(),
                fixture_id: fixture.fixture_id().to_string(),
                summary: error.to_string(),
            });
            continue;
        }
        match materialize_fixture_snapshots(fixture, docker_image, check, report_dir) {
            Ok(outcome) => {
                updated += 1;
                if let Some(failure) = outcome.check_failure {
                    failures.push(failure);
                }
                fixture_reports.push(outcome.report);
            }
            Err(failure) => failures.push(failure),
        }
    }

    let report = SnapshotReport {
        selected_fixture_count: selected.len(),
        promoted_fixture_count: promoted_count,
        successful_fixture_count: fixture_reports.len(),
        check_only: check,
        subset: subset_name(subset).to_string(),
        docker_image: docker_image.to_string(),
        drift_metrics: snapshot_drift_metrics_for(&fixture_reports),
        fallback_reason_counts: count_snapshot_fallback_reasons(&fixture_reports),
        fixtures: fixture_reports,
        failures: failures.clone(),
    };
    if let Some(report_dir) = report_dir {
        write_snapshot_report(report_dir, &report)?;
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
            "subset": report.subset,
            "docker_image": docker_image,
            "drift_metrics": report.drift_metrics,
            "fallback_reason_counts": report.fallback_reason_counts
        }))?
    );
    Ok(())
}

pub(crate) fn collect_acceptance_fixture_summary(
    fixture: &Fixture,
    report_dir: Option<&Path>,
) -> Result<AcceptanceFixtureSummary, VerificationFailure> {
    let semantic = fixture
        .expectations
        .semantic
        .as_ref()
        .ok_or_else(|| VerificationFailure {
            layer: "semantic".to_string(),
            fixture_id: fixture.fixture_id().to_string(),
            summary: "promoted fixture missing semantic expectations".to_string(),
        })?;
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

    let default_request =
        render_request_for_fixture(fixture, &replay.document, RenderProfile::Default);
    let default_view_model = build_view_model(&default_request);
    let render_started = Instant::now();
    let default_render_result =
        render(default_request.clone()).map_err(|error| VerificationFailure {
            layer: "render.default".to_string(),
            fixture_id: fixture.fixture_id().to_string(),
            summary: error.to_string(),
        })?;
    let effective_fallback_reason = replay
        .fallback_reason
        .or(default_render_result.fallback_reason);
    let render_time_ms = elapsed_ms(render_started);
    let lead_node = lead_node_for_document(
        &replay.document,
        &default_render_result.displayed_group_refs,
    )
    .ok_or_else(|| VerificationFailure {
        layer: "semantic".to_string(),
        fixture_id: fixture.fixture_id().to_string(),
        summary: "default render produced no lead diagnostic".to_string(),
    })?;

    let actual_family = lead_node
        .analysis
        .as_ref()
        .and_then(|analysis| analysis.family.as_deref())
        .unwrap_or("unknown")
        .to_string();
    let family_match = semantic.family == actual_family;
    let raw_headline = lead_node
        .message
        .raw_text
        .lines()
        .next()
        .unwrap_or_default()
        .trim()
        .to_string();
    let analyzed_headline = lead_node
        .analysis
        .as_ref()
        .and_then(|analysis| analysis.headline.as_deref())
        .unwrap_or_default()
        .trim()
        .to_string();
    let first_action_present = lead_node
        .analysis
        .as_ref()
        .and_then(|analysis| analysis.first_action_hint.as_ref())
        .map(|value| !value.trim().is_empty())
        .unwrap_or(false);
    let primary_location = lead_node.primary_location();
    let primary_location_user_owned = primary_location
        .and_then(|location| location.ownership.as_ref())
        .is_some_and(|ownership| *ownership == Ownership::User);
    let fallback_forbidden = semantic.fallback == Some(ExpectedFallback::Forbidden);
    let unexpected_fallback = fallback_forbidden && default_render_result.used_fallback;
    let primary_location_user_owned_required = semantic.primary_location_user_owned_required;
    let missing_required_primary_location =
        primary_location_user_owned_required && !primary_location_user_owned;
    let first_action_required = semantic.first_action_required;
    let missing_required_first_action = first_action_required && !first_action_present;

    if let Some(report_dir) = report_dir {
        let mut artifacts = BTreeMap::new();
        let snapshot_root = fixture.snapshot_root();
        artifacts.insert(
            "stderr.raw".to_string(),
            fs::read_to_string(snapshot_root.join("stderr.raw")).map_err(|error| {
                VerificationFailure {
                    layer: "report.stderr".to_string(),
                    fixture_id: fixture.fixture_id().to_string(),
                    summary: error.to_string(),
                }
            })?,
        );
        artifacts.insert(
            "diagnostics.sarif".to_string(),
            fs::read_to_string(snapshot_root.join("diagnostics.sarif")).map_err(|error| {
                VerificationFailure {
                    layer: "report.sarif".to_string(),
                    fixture_id: fixture.fixture_id().to_string(),
                    summary: error.to_string(),
                }
            })?,
        );
        artifacts.insert(
            "ir.facts.json".to_string(),
            snapshot_json(&replay.document, SnapshotKind::FactsOnly).map_err(|error| {
                VerificationFailure {
                    layer: "report.ir.facts".to_string(),
                    fixture_id: fixture.fixture_id().to_string(),
                    summary: error.to_string(),
                }
            })?,
        );
        artifacts.insert(
            "ir.analysis.json".to_string(),
            snapshot_json(&replay.document, SnapshotKind::AnalysisIncluded).map_err(|error| {
                VerificationFailure {
                    layer: "report.ir.analysis".to_string(),
                    fixture_id: fixture.fixture_id().to_string(),
                    summary: error.to_string(),
                }
            })?,
        );
        artifacts.insert(
            "view.default.json".to_string(),
            canonical_json_for_view_model(default_view_model.as_ref()).map_err(|error| {
                VerificationFailure {
                    layer: "report.view.default".to_string(),
                    fixture_id: fixture.fixture_id().to_string(),
                    summary: error.to_string(),
                }
            })?,
        );
        artifacts.insert(
            "render.default.txt".to_string(),
            default_render_result.text.clone(),
        );
        let mut artifact_diffs = Vec::new();
        for (relative, contents) in &artifacts {
            let path = fixture.snapshot_root().join(relative);
            let (diff, _) =
                classify_snapshot_artifact_diff(fixture, relative, &path, contents, false)?;
            artifact_diffs.push(diff);
        }
        write_fixture_report_bundle(report_dir, fixture, &artifacts, &artifact_diffs).map_err(
            |error| VerificationFailure {
                layer: "report.bundle".to_string(),
                fixture_id: fixture.fixture_id().to_string(),
                summary: error,
            },
        )?;
    }

    Ok(AcceptanceFixtureSummary {
        fixture_id: fixture.fixture_id().to_string(),
        family_key: fixture.family_key(),
        title: fixture.meta.title.clone(),
        expected_family: Some(semantic.family.clone()),
        actual_family,
        family_match,
        used_fallback: default_render_result.used_fallback,
        fallback_reason: effective_fallback_reason,
        fallback_forbidden,
        unexpected_fallback,
        primary_location_path: primary_location.map(|location| location.path.clone()),
        primary_location_user_owned_required,
        primary_location_user_owned,
        missing_required_primary_location,
        first_action_required,
        first_action_present,
        missing_required_first_action,
        headline_rewritten: !analyzed_headline.is_empty() && analyzed_headline != raw_headline,
        parse_time_ms: replay.parse_time_ms,
        render_time_ms,
        verified: false,
    })
}

pub(crate) fn verify_promoted_fixture(fixture: &Fixture) -> Result<(), VerificationFailure> {
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

    verify_snapshot_file(
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
    verify_snapshot_file(
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

        verify_snapshot_file(
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
        verify_snapshot_file(
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

pub(crate) fn materialize_fixture_snapshots(
    fixture: &Fixture,
    docker_image: &str,
    check: bool,
    report_dir: Option<&Path>,
) -> Result<SnapshotFixtureOutcome, VerificationFailure> {
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

    let replay = replay_document_from_ingress(
        fixture,
        &captured.stderr_text,
        temp_root.join("diagnostics.sarif").as_path(),
    )
    .map_err(|error| VerificationFailure {
        layer: "snapshot".to_string(),
        fixture_id: fixture.fixture_id().to_string(),
        summary: error.to_string(),
    })?;
    replay
        .document
        .validate()
        .map_err(|error| VerificationFailure {
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
        snapshot_json(&replay.document, SnapshotKind::FactsOnly).map_err(|error| {
            VerificationFailure {
                layer: "snapshot".to_string(),
                fixture_id: fixture.fixture_id().to_string(),
                summary: error.to_string(),
            }
        })?,
    );
    artifacts.insert(
        "ir.analysis.json".to_string(),
        snapshot_json(&replay.document, SnapshotKind::AnalysisIncluded).map_err(|error| {
            VerificationFailure {
                layer: "snapshot".to_string(),
                fixture_id: fixture.fixture_id().to_string(),
                summary: error.to_string(),
            }
        })?,
    );

    let mut effective_fallback_reason = replay.fallback_reason;
    for (profile_name, _) in fixture.expectations.render.named_profiles() {
        let profile =
            render_profile_from_name(profile_name).ok_or_else(|| VerificationFailure {
                layer: "snapshot".to_string(),
                fixture_id: fixture.fixture_id().to_string(),
                summary: format!("unknown snapshot profile `{profile_name}`"),
            })?;
        let request = render_request_for_fixture(fixture, &replay.document, profile);
        let view_model = build_view_model(&request);
        let render_result = render(request).map_err(|error| VerificationFailure {
            layer: format!("render.{profile_name}"),
            fixture_id: fixture.fixture_id().to_string(),
            summary: error.to_string(),
        })?;
        if matches!(profile, RenderProfile::Default) {
            effective_fallback_reason = effective_fallback_reason.or(render_result.fallback_reason);
        }
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

    let mut artifact_diffs = Vec::new();
    let mut pending_failure = None;
    for (relative, contents) in &artifacts {
        let path = snapshot_root.join(relative);
        let (diff, failure) =
            classify_snapshot_artifact_diff(fixture, relative, &path, contents, check)?;
        artifact_diffs.push(diff);
        if pending_failure.is_none() {
            pending_failure = failure;
        }
    }

    if let Some(report_dir) = report_dir {
        write_fixture_report_bundle(report_dir, fixture, &artifacts, &artifact_diffs).map_err(
            |error| VerificationFailure {
                layer: "report.bundle".to_string(),
                fixture_id: fixture.fixture_id().to_string(),
                summary: error,
            },
        )?;
    }

    for (relative, contents) in artifacts {
        let path = snapshot_root.join(relative);
        if !check {
            fs::write(&path, contents).map_err(|error| VerificationFailure {
                layer: "snapshot_write".to_string(),
                fixture_id: fixture.fixture_id().to_string(),
                summary: format!("{}: {error}", path.display()),
            })?;
        }
    }

    Ok(SnapshotFixtureOutcome {
        report: SnapshotFixtureReport {
            fixture_id: fixture.fixture_id().to_string(),
            family_key: fixture.family_key(),
            fallback_reason: effective_fallback_reason,
            artifact_diffs,
        },
        check_failure: pending_failure,
    })
}

pub(crate) fn load_existing_ingress(
    fixture: &Fixture,
) -> Result<CapturedIngress, VerificationFailure> {
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

pub(crate) fn replay_fixture_document(
    fixture: &Fixture,
) -> Result<ReplayOutcomeAndDocument, Box<dyn std::error::Error>> {
    let snapshot_root = fixture.snapshot_root();
    let stderr_text = fs::read_to_string(snapshot_root.join("stderr.raw"))?;
    let parse_start = Instant::now();
    let replay = replay_document_from_ingress(
        fixture,
        &stderr_text,
        snapshot_root.join("diagnostics.sarif").as_path(),
    )?;
    Ok(ReplayOutcomeAndDocument {
        parse_time_ms: elapsed_ms(parse_start),
        ..replay
    })
}

#[derive(Debug)]
pub(crate) struct ReplayOutcomeAndDocument {
    pub(crate) document: DiagnosticDocument,
    pub(crate) fallback_reason: Option<FallbackReason>,
    pub(crate) parse_time_ms: u64,
}

pub(crate) fn replay_document_from_ingress(
    fixture: &Fixture,
    stderr_text: &str,
    sarif_path: &Path,
) -> Result<ReplayOutcomeAndDocument, Box<dyn std::error::Error>> {
    let run_info = run_info_for_fixture(fixture);
    let ingest = ingest_with_reason(
        Some(sarif_path),
        stderr_text,
        producer_for_version("snapshot"),
        run_info,
    )?;
    let mut document = ingest.document;
    document.captures = capture_artifacts_for_fixture(fixture, stderr_text, sarif_path)?;
    enrich_document(&mut document, &fixture.root);
    Ok(ReplayOutcomeAndDocument {
        document,
        fallback_reason: ingest.fallback_reason,
        parse_time_ms: 0,
    })
}

pub(crate) fn run_info_for_fixture(fixture: &Fixture) -> RunInfo {
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

pub(crate) fn capture_artifacts_for_fixture(
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

pub(crate) fn render_request_for_fixture(
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

pub(crate) fn verify_semantic_expectations(
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

    if semantic.primary_location_user_owned_required
        && !lead_node
            .primary_location()
            .and_then(|location| location.ownership.as_ref())
            .is_some_and(|ownership| *ownership == Ownership::User)
    {
        return Err(VerificationFailure {
            layer: "semantic.primary_location_ownership".to_string(),
            fixture_id: fixture.fixture_id().to_string(),
            summary: "lead diagnostic primary location was not user-owned".to_string(),
        });
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

pub(crate) fn verify_render_expectations(
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
    for required in &expectations.required_substrings {
        if !text.contains(required) {
            return Err(VerificationFailure {
                layer: format!("render.{profile_name}.required_substrings"),
                fixture_id: fixture.fixture_id().to_string(),
                summary: format!("required substring missing: `{required}`"),
            });
        }
    }
    for forbidden in &expectations.forbidden_substrings {
        if text.contains(forbidden) {
            return Err(VerificationFailure {
                layer: format!("render.{profile_name}.forbidden_substrings"),
                fixture_id: fixture.fixture_id().to_string(),
                summary: format!("forbidden substring present: `{forbidden}`"),
            });
        }
    }
    Ok(())
}

pub(crate) fn classify_snapshot_artifact_diff(
    fixture: &Fixture,
    relative: &str,
    path: &Path,
    actual: &str,
    check: bool,
) -> Result<(SnapshotArtifactDiff, Option<VerificationFailure>), VerificationFailure> {
    let diff_path = relative.to_string();
    if !path.exists() {
        let diff = SnapshotArtifactDiff {
            path: diff_path,
            diff_kind: SnapshotDiffKind::MissingExpected,
        };
        let failure = check.then(|| VerificationFailure {
            layer: relative.to_string(),
            fixture_id: fixture.fixture_id().to_string(),
            summary: format!("missing expected snapshot {}", path.display()),
        });
        return Ok((diff, failure));
    }

    let expected = fs::read_to_string(path).map_err(|error| VerificationFailure {
        layer: relative.to_string(),
        fixture_id: fixture.fixture_id().to_string(),
        summary: format!("failed to read {}: {error}", path.display()),
    })?;
    let comparison = compare_snapshot_contents(path, &expected, actual).map_err(|summary| {
        VerificationFailure {
            layer: relative.to_string(),
            fixture_id: fixture.fixture_id().to_string(),
            summary,
        }
    })?;
    let diff = SnapshotArtifactDiff {
        path: diff_path,
        diff_kind: comparison.diff_kind,
    };
    let failure = if check && !comparison.matches_after_normalization() {
        Some(VerificationFailure {
            layer: relative.to_string(),
            fixture_id: fixture.fixture_id().to_string(),
            summary: format!(
                "semantic diff after normalization: {}",
                first_diff_summary(
                    &comparison.normalized_expected,
                    &comparison.normalized_actual
                )
            ),
        })
    } else {
        None
    };
    Ok((diff, failure))
}

pub(crate) fn verify_snapshot_file(
    fixture: &Fixture,
    layer: &str,
    path: &Path,
    actual: &str,
) -> Result<(), VerificationFailure> {
    if !path.exists() {
        return Err(VerificationFailure {
            layer: layer.to_string(),
            fixture_id: fixture.fixture_id().to_string(),
            summary: format!("missing expected snapshot {}", path.display()),
        });
    }
    let expected = fs::read_to_string(path).map_err(|error| VerificationFailure {
        layer: layer.to_string(),
        fixture_id: fixture.fixture_id().to_string(),
        summary: format!("failed to read {}: {error}", path.display()),
    })?;
    let comparison = compare_snapshot_contents(path, &expected, actual).map_err(|summary| {
        VerificationFailure {
            layer: layer.to_string(),
            fixture_id: fixture.fixture_id().to_string(),
            summary,
        }
    })?;
    if comparison.matches_after_normalization() {
        return Ok(());
    }
    Err(VerificationFailure {
        layer: layer.to_string(),
        fixture_id: fixture.fixture_id().to_string(),
        summary: format!(
            "semantic diff after normalization: {}",
            first_diff_summary(
                &comparison.normalized_expected,
                &comparison.normalized_actual
            )
        ),
    })
}

pub(crate) fn canonical_json_for_view_model(
    view_model: Option<&diag_render::RenderViewModel>,
) -> Result<String, serde_json::Error> {
    match view_model {
        Some(model) => diag_core::canonical_json(model),
        None => diag_core::canonical_json(&serde_json::Value::Null),
    }
}

pub(crate) fn render_profile_from_name(name: &str) -> Option<RenderProfile> {
    match name {
        "default" => Some(RenderProfile::Default),
        "concise" => Some(RenderProfile::Concise),
        "verbose" => Some(RenderProfile::Verbose),
        "ci" => Some(RenderProfile::Ci),
        "raw_fallback" => Some(RenderProfile::RawFallback),
        _ => None,
    }
}

pub(crate) fn lead_node_for_document<'a>(
    document: &'a DiagnosticDocument,
    displayed_group_refs: &[String],
) -> Option<&'a diag_core::DiagnosticNode> {
    let lead_id = displayed_group_refs.first()?;
    document.diagnostics.iter().find(|node| &node.id == lead_id)
}

pub(crate) fn confidence_rank(confidence: &diag_core::Confidence) -> u8 {
    match confidence {
        diag_core::Confidence::High => 4,
        diag_core::Confidence::Medium => 3,
        diag_core::Confidence::Low => 2,
        diag_core::Confidence::Unknown => 1,
    }
}

pub(crate) fn select_fixtures<'a>(
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

pub(crate) fn family_counts_for_selected(fixtures: &[&Fixture]) -> BTreeMap<String, usize> {
    let mut counts = BTreeMap::new();
    for fixture in fixtures {
        *counts.entry(fixture.family_key()).or_insert(0) += 1;
    }
    counts
}

pub(crate) fn subset_name(subset: SnapshotSubset) -> &'static str {
    match subset {
        SnapshotSubset::All => "all",
        SnapshotSubset::Representative => "representative",
    }
}

pub(crate) fn acceptance_metrics_for(fixtures: &[AcceptanceFixtureSummary]) -> AcceptanceMetrics {
    let promoted_fixture_count = fixtures.len();
    let fallback_used_count = fixtures
        .iter()
        .filter(|fixture| fixture.used_fallback)
        .count();
    let fallback_forbidden_count = fixtures
        .iter()
        .filter(|fixture| fixture.fallback_forbidden)
        .count();
    let unexpected_fallback_count = fixtures
        .iter()
        .filter(|fixture| fixture.unexpected_fallback)
        .count();
    let fallback_reason_counts = count_fallback_reasons(fixtures.iter());
    let unexpected_fallback_reason_counts = count_fallback_reasons(
        fixtures
            .iter()
            .filter(|fixture| fixture.unexpected_fallback),
    );
    let primary_location_user_owned_required_count = fixtures
        .iter()
        .filter(|fixture| fixture.primary_location_user_owned_required)
        .count();
    let primary_location_user_owned_count = fixtures
        .iter()
        .filter(|fixture| {
            fixture.primary_location_user_owned_required && fixture.primary_location_user_owned
        })
        .count();
    let missing_required_primary_location_count = fixtures
        .iter()
        .filter(|fixture| fixture.missing_required_primary_location)
        .count();
    let first_action_required_count = fixtures
        .iter()
        .filter(|fixture| fixture.first_action_required)
        .count();
    let first_action_present_count = fixtures
        .iter()
        .filter(|fixture| fixture.first_action_required && fixture.first_action_present)
        .count();
    let missing_required_first_action_count = fixtures
        .iter()
        .filter(|fixture| fixture.missing_required_first_action)
        .count();
    let headline_rewritten_count = fixtures
        .iter()
        .filter(|fixture| fixture.headline_rewritten)
        .count();
    let family_expected_count = fixtures
        .iter()
        .filter(|fixture| fixture.expected_family.is_some())
        .count();
    let family_match_count = fixtures
        .iter()
        .filter(|fixture| fixture.family_match)
        .count();

    AcceptanceMetrics {
        promoted_fixture_count,
        fallback_used_count,
        fallback_forbidden_count,
        unexpected_fallback_count,
        fallback_reason_counts,
        unexpected_fallback_reason_counts,
        primary_location_user_owned_required_count,
        primary_location_user_owned_count,
        missing_required_primary_location_count,
        first_action_required_count,
        first_action_present_count,
        missing_required_first_action_count,
        headline_rewritten_count,
        family_expected_count,
        family_match_count,
        fallback_rate: ratio(unexpected_fallback_count, fallback_forbidden_count),
        primary_location_user_owned_rate: ratio(
            primary_location_user_owned_count,
            primary_location_user_owned_required_count,
        ),
        first_action_present_rate: ratio(first_action_present_count, first_action_required_count),
        headline_rewritten_rate: ratio(headline_rewritten_count, promoted_fixture_count),
        family_match_rate: ratio(family_match_count, family_expected_count),
    }
}

pub(crate) fn count_fallback_reasons<'a>(
    fixtures: impl IntoIterator<Item = &'a AcceptanceFixtureSummary>,
) -> BTreeMap<String, usize> {
    let mut counts = BTreeMap::new();
    for fixture in fixtures {
        if let Some(reason) = fixture.fallback_reason {
            *counts.entry(reason.to_string()).or_insert(0) += 1;
        }
    }
    counts
}

pub(crate) fn ratio(numerator: usize, denominator: usize) -> f64 {
    if denominator == 0 {
        0.0
    } else {
        numerator as f64 / denominator as f64
    }
}

pub(crate) fn snapshot_drift_metrics_for(
    fixtures: &[SnapshotFixtureReport],
) -> SnapshotDriftMetrics {
    let mut metrics = SnapshotDriftMetrics::default();
    for fixture in fixtures {
        for artifact in &fixture.artifact_diffs {
            match artifact.diff_kind {
                SnapshotDiffKind::Exact => metrics.exact_count += 1,
                SnapshotDiffKind::NormalizationOnly => metrics.normalization_only_count += 1,
                SnapshotDiffKind::Semantic => metrics.semantic_count += 1,
                SnapshotDiffKind::MissingExpected => metrics.missing_expected_count += 1,
            }
        }
    }
    metrics
}

pub(crate) fn count_snapshot_fallback_reasons(
    fixtures: &[SnapshotFixtureReport],
) -> BTreeMap<String, usize> {
    let mut counts = BTreeMap::new();
    for fixture in fixtures {
        if let Some(reason) = fixture.fallback_reason {
            *counts.entry(reason.to_string()).or_insert(0) += 1;
        }
    }
    counts
}

pub(crate) fn write_replay_report(
    report_dir: &Path,
    report: &ReplayReport,
) -> Result<(), Box<dyn std::error::Error>> {
    fs::create_dir_all(report_dir)?;
    fs::write(
        report_dir.join("replay-report.json"),
        serde_json::to_vec_pretty(report)?,
    )?;
    Ok(())
}

pub(crate) fn write_snapshot_report(
    report_dir: &Path,
    report: &SnapshotReport,
) -> Result<(), Box<dyn std::error::Error>> {
    fs::create_dir_all(report_dir)?;
    fs::write(
        report_dir.join("snapshot-report.json"),
        serde_json::to_vec_pretty(report)?,
    )?;
    Ok(())
}

pub(crate) fn write_fixture_report_bundle(
    report_dir: &Path,
    fixture: &Fixture,
    actual_artifacts: &BTreeMap<String, String>,
    artifact_diffs: &[SnapshotArtifactDiff],
) -> Result<(), String> {
    let fixture_dir = report_dir.join("fixtures").join(fixture.fixture_id());
    let actual_dir = fixture_dir.join("actual");
    let actual_normalized_dir = fixture_dir.join("actual-normalized");
    let expected_dir = fixture_dir.join("expected");
    let expected_normalized_dir = fixture_dir.join("expected-normalized");
    fs::create_dir_all(&actual_dir).map_err(|error| error.to_string())?;
    fs::create_dir_all(&actual_normalized_dir).map_err(|error| error.to_string())?;

    for (relative, contents) in actual_artifacts {
        let actual_path = actual_dir.join(relative);
        if let Some(parent) = actual_path.parent() {
            fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }
        fs::write(&actual_path, contents).map_err(|error| error.to_string())?;

        let normalized_actual = normalize_snapshot_contents(Path::new(relative), contents)
            .map_err(|error| {
                format!("failed to normalize actual report artifact `{relative}`: {error}")
            })?;
        let normalized_actual_path = actual_normalized_dir.join(relative);
        if let Some(parent) = normalized_actual_path.parent() {
            fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }
        fs::write(&normalized_actual_path, normalized_actual).map_err(|error| error.to_string())?;

        let expected_path = fixture.snapshot_root().join(relative);
        if expected_path.exists() {
            let expected_contents =
                fs::read_to_string(&expected_path).map_err(|error| error.to_string())?;
            let report_expected_path = expected_dir.join(relative);
            if let Some(parent) = report_expected_path.parent() {
                fs::create_dir_all(parent).map_err(|error| error.to_string())?;
            }
            fs::write(&report_expected_path, &expected_contents)
                .map_err(|error| error.to_string())?;

            let normalized_expected =
                normalize_snapshot_contents(&expected_path, &expected_contents).map_err(
                    |error| {
                        format!(
                            "failed to normalize expected report artifact `{}`: {error}",
                            expected_path.display()
                        )
                    },
                )?;
            let normalized_expected_path = expected_normalized_dir.join(relative);
            if let Some(parent) = normalized_expected_path.parent() {
                fs::create_dir_all(parent).map_err(|error| error.to_string())?;
            }
            fs::write(&normalized_expected_path, normalized_expected)
                .map_err(|error| error.to_string())?;
        }
    }

    fs::write(
        fixture_dir.join("fixture.json"),
        serde_json::to_vec_pretty(&serde_json::json!({
            "fixture_id": fixture.fixture_id(),
            "family": fixture.family_key(),
            "title": fixture.meta.title.clone(),
            "artifact_diffs": artifact_diffs,
        }))
        .map_err(|error| error.to_string())?,
    )
    .map_err(|error| error.to_string())?;

    fs::write(
        fixture_dir.join("comparisons.json"),
        serde_json::to_vec_pretty(artifact_diffs).map_err(|error| error.to_string())?,
    )
    .map_err(|error| error.to_string())?;

    Ok(())
}

pub(crate) fn validate_snapshot_inputs(
    fixture: &Fixture,
) -> Result<(), Box<dyn std::error::Error>> {
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

pub(crate) fn capture_fixture_ingress(
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

pub(crate) fn compiler_binary_for_fixture(fixture: &Fixture) -> &'static str {
    match fixture.invoke.language.as_str() {
        "cpp" | "cxx" => "g++",
        _ => "gcc",
    }
}

pub(crate) fn language_mode_for_fixture(fixture: &Fixture) -> LanguageMode {
    match fixture.invoke.language.as_str() {
        "cpp" | "cxx" => LanguageMode::Cpp,
        _ => LanguageMode::C,
    }
}

pub(crate) fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\"'\"'"))
}

pub(crate) fn copy_dir_recursive(from: &Path, to: &Path) -> Result<(), std::io::Error> {
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

pub(crate) fn report_failures(mode: &str, failures: &[VerificationFailure]) {
    eprintln!("mode: {mode}");
    eprintln!("failed fixture count: {}", failures.len());
    if let Some(first) = failures.first() {
        eprintln!("failed layer: {}", first.layer);
        eprintln!("first failed fixture: {}", first.fixture_id);
        eprintln!("first diff summary: {}", first.summary);
    }
}

pub(crate) fn first_diff_summary(expected: &str, actual: &str) -> String {
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

pub(crate) fn elapsed_ms(start: Instant) -> u64 {
    start.elapsed().as_millis() as u64
}

pub(crate) fn enforce_minimum_corpus_shape(
    fixture_count: usize,
    counts: &std::collections::BTreeMap<String, usize>,
) -> Result<(), Box<dyn std::error::Error>> {
    if fixture_count < MINIMUM_CURATED_CORPUS_SIZE {
        return Err(format!(
            "curated corpus below beta bar: expected >= {MINIMUM_CURATED_CORPUS_SIZE} fixtures, got {fixture_count}"
        )
        .into());
    }
    if fixture_count > MAXIMUM_CURATED_CORPUS_SIZE {
        return Err(format!(
            "curated corpus exceeded beta bar: expected <= {MAXIMUM_CURATED_CORPUS_SIZE} fixtures, got {fixture_count}"
        )
        .into());
    }
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
