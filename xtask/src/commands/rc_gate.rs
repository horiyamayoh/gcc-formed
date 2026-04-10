use crate::SnapshotSubset;
use crate::commands::corpus::{
    NativeParityReport, ReplayReport, build_replay_report, subset_name, write_replay_report,
};
use crate::commands::fuzz::{FuzzSmokeReport, FuzzSmokeStatus, run_fuzz_smoke};
use crate::commands::human_eval::{
    HumanEvalKitReport, human_eval_kit_is_complete, run_human_eval_kit,
};
use diag_backend_probe::{ProbeCache, ResolveRequest, SupportTier};
use diag_capture_runtime::{CaptureRequest, ExecutionMode, cleanup_capture, run_capture};
use diag_trace::{RetentionPolicy, WrapperPaths, build_target_triple};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::BTreeMap;
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

const RC_GATE_SCHEMA_VERSION: u32 = 1;
const SUCCESS_PATH_P95_TARGET_MS: u64 = 40;
const SIMPLE_FAILURE_P95_TARGET_MS: u64 = 80;
const TEMPLATE_HEAVY_P95_TARGET_MS: u64 = 250;
const UNEXPECTED_FALLBACK_RATE_TARGET: f64 = 0.001;
const SUCCESS_PATH_BENCH_SAMPLES: usize = 20;
const SUCCESS_PATH_WARMUP_RUNS: usize = 2;
const REQUIRED_METRIC_FAMILIES: &[&str] = &[
    "syntax",
    "macro_include",
    "template",
    "type",
    "overload",
    "linker",
];
const FIRST_ACTION_SCREENFUL_LINE: usize = 8;

#[derive(Debug, Clone)]
pub(crate) struct RcGateOptions {
    pub(crate) root: PathBuf,
    pub(crate) report_dir: PathBuf,
    pub(crate) metrics_manual_report: PathBuf,
    pub(crate) issue_budget_report: PathBuf,
    pub(crate) fuzz_root: PathBuf,
    pub(crate) fuzz_report: Option<PathBuf>,
    pub(crate) ux_signoff_report: PathBuf,
    pub(crate) allow_pending_manual_checks: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum GateStatus {
    Pass,
    Fail,
    Pending,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct SlowFixtureReport {
    pub(crate) fixture_id: String,
    pub(crate) family_key: String,
    pub(crate) postprocess_ms: u64,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct BenchScenarioReport {
    pub(crate) scenario: String,
    pub(crate) status: GateStatus,
    pub(crate) metric: String,
    pub(crate) target_ms: u64,
    pub(crate) p95_ms: Option<u64>,
    pub(crate) sample_count: usize,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(crate) samples_ms: Vec<u64>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(crate) slowest_fixtures: Vec<SlowFixtureReport>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(crate) notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct BenchSmokeReport {
    pub(crate) schema_version: u32,
    pub(crate) subset: String,
    pub(crate) overall_status: GateStatus,
    pub(crate) blockers: Vec<String>,
    pub(crate) success_path: BenchScenarioReport,
    pub(crate) simple_failure: BenchScenarioReport,
    pub(crate) template_heavy_failure: BenchScenarioReport,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ManualEvidenceStatus {
    Approved,
    Pending,
    Rejected,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct IssueBudgetEvidence {
    pub(crate) schema_version: u32,
    pub(crate) release_candidate: String,
    pub(crate) status: ManualEvidenceStatus,
    pub(crate) open_p0: usize,
    pub(crate) open_p1: usize,
    pub(crate) updated_at: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(crate) notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct FuzzEvidence {
    pub(crate) schema_version: u32,
    pub(crate) release_candidate: String,
    pub(crate) status: ManualEvidenceStatus,
    pub(crate) crash_count: usize,
    pub(crate) corpus_replay_passed: bool,
    pub(crate) updated_at: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(crate) notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct UxSignoffEvidence {
    pub(crate) schema_version: u32,
    pub(crate) release_candidate: String,
    pub(crate) status: ManualEvidenceStatus,
    pub(crate) updated_at: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(crate) reviewers: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(crate) notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct ManualMetricsEvaluation {
    pub(crate) schema_version: u32,
    pub(crate) release_candidate: String,
    pub(crate) status: ManualEvidenceStatus,
    pub(crate) reviewed_fixture_count: usize,
    pub(crate) high_confidence_mislead_rate: Option<f64>,
    pub(crate) trc_improvement_percent: Option<f64>,
    pub(crate) tfah_improvement_percent: Option<f64>,
    pub(crate) first_fix_success_delta_points: Option<f64>,
    pub(crate) updated_at: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(crate) reviewers: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(crate) notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) struct RolloutMatrixCase {
    pub(crate) support_tier: String,
    pub(crate) requested_mode: Option<String>,
    pub(crate) hard_conflict: bool,
    pub(crate) selected_mode: String,
    pub(crate) fallback_reason: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) scope_notice: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct RolloutMatrixMismatch {
    pub(crate) key: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) expected: Option<RolloutMatrixCase>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) observed: Option<RolloutMatrixCase>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct RolloutMatrixReport {
    pub(crate) status: GateStatus,
    pub(crate) command: String,
    pub(crate) expected_case_count: usize,
    pub(crate) observed_case_count: usize,
    pub(crate) matched_case_count: usize,
    pub(crate) mismatches: Vec<RolloutMatrixMismatch>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct DeterministicReplayReport {
    pub(crate) status: GateStatus,
    pub(crate) subset: String,
    pub(crate) first_hash: String,
    pub(crate) second_hash: String,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct RcGateCheck {
    pub(crate) id: String,
    pub(crate) title: String,
    pub(crate) status: GateStatus,
    pub(crate) summary: String,
    pub(crate) blocker: bool,
    pub(crate) manual: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) evidence_path: Option<PathBuf>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct RcGateBlocker {
    pub(crate) id: String,
    pub(crate) summary: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) evidence_path: Option<PathBuf>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct RcGateReport {
    pub(crate) schema_version: u32,
    pub(crate) generated_at_unix_seconds: u64,
    pub(crate) root: PathBuf,
    pub(crate) report_dir: PathBuf,
    pub(crate) allow_pending_manual_checks: bool,
    pub(crate) overall_status: GateStatus,
    pub(crate) blockers: Vec<RcGateBlocker>,
    pub(crate) checks: Vec<RcGateCheck>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct FallbackMetricsReport {
    pub(crate) fallback_rate: f64,
    pub(crate) unexpected_fallback_rate: f64,
    pub(crate) fallback_reason_counts: BTreeMap<String, usize>,
    pub(crate) unexpected_fallback_reason_counts: BTreeMap<String, usize>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct HighConfidenceMetricsReport {
    pub(crate) high_confidence_fixture_count: usize,
    pub(crate) automated_family_mismatch_count: usize,
    pub(crate) automated_family_mismatch_rate: f64,
    pub(crate) manual_status: ManualEvidenceStatus,
    pub(crate) manual_reviewed_fixture_count: usize,
    pub(crate) manual_high_confidence_mislead_rate: Option<f64>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct CompressionMetricsReport {
    pub(crate) comparable_fixture_count: usize,
    pub(crate) median_ratio: Option<f64>,
    pub(crate) min_ratio: Option<f64>,
    pub(crate) max_ratio: Option<f64>,
    pub(crate) family_median_ratios: BTreeMap<String, f64>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct FirstActionMetricsReport {
    pub(crate) rendered_hint_fixture_count: usize,
    pub(crate) median_line: Option<usize>,
    pub(crate) max_line: Option<usize>,
    pub(crate) within_first_screenful_count: usize,
    pub(crate) within_first_screenful_rate: Option<f64>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct PerformanceMetricsReport {
    pub(crate) success_path_p95_overhead_ms: Option<u64>,
    pub(crate) simple_failure_p95_postprocess_ms: Option<u64>,
    pub(crate) template_heavy_p95_postprocess_ms: Option<u64>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct FamilyCoverageMetricsReport {
    pub(crate) observed_family_counts: BTreeMap<String, usize>,
    pub(crate) required_family_count: usize,
    pub(crate) covered_required_family_count: usize,
    pub(crate) missing_required_families: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct CompatibilityVsPrimaryMetricsReport {
    pub(crate) rollout_matrix_status: GateStatus,
    pub(crate) tier_a_default_mode: Option<String>,
    pub(crate) tier_b_default_mode: Option<String>,
    pub(crate) tier_c_default_mode: Option<String>,
    pub(crate) tier_b_requested_render_mode: Option<String>,
    pub(crate) tier_b_requested_shadow_mode: Option<String>,
    pub(crate) primary_enhanced_default: bool,
    pub(crate) compatibility_default_is_conservative: bool,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct RawGccComparisonMetricsReport {
    pub(crate) compression_ratio: CompressionMetricsReport,
    pub(crate) first_action_hint: FirstActionMetricsReport,
    pub(crate) manual_eval_status: ManualEvidenceStatus,
    pub(crate) manual_trc_improvement_percent: Option<f64>,
    pub(crate) manual_tfah_improvement_percent: Option<f64>,
    pub(crate) manual_first_fix_success_delta_points: Option<f64>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct NativeParityMetricsReport {
    pub(crate) covered_dimensions: BTreeMap<String, usize>,
    pub(crate) failure_counts_by_dimension: BTreeMap<String, usize>,
    pub(crate) failing_fixture_count: usize,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct RcMetricsReport {
    pub(crate) schema_version: u32,
    pub(crate) generated_at_unix_seconds: u64,
    pub(crate) fallback: FallbackMetricsReport,
    pub(crate) high_confidence: HighConfidenceMetricsReport,
    pub(crate) native_parity: NativeParityMetricsReport,
    pub(crate) raw_gcc_comparison: RawGccComparisonMetricsReport,
    pub(crate) performance: PerformanceMetricsReport,
    pub(crate) family_coverage: FamilyCoverageMetricsReport,
    pub(crate) compatibility_vs_primary: CompatibilityVsPrimaryMetricsReport,
    pub(crate) manual_evidence_path: PathBuf,
    pub(crate) manual_evidence: ManualMetricsEvaluation,
}

pub(crate) fn run_bench_smoke(
    root: &Path,
    subset: SnapshotSubset,
    report_dir: Option<&Path>,
) -> Result<BenchSmokeReport, Box<dyn std::error::Error>> {
    let replay = build_replay_report(root, None, None, subset, None)?;
    let report = bench_smoke_report_from_replay(&replay, subset)?;
    if let Some(report_dir) = report_dir {
        fs::create_dir_all(report_dir)?;
        fs::write(
            report_dir.join("bench-smoke-report.json"),
            serde_json::to_vec_pretty(&report)?,
        )?;
    }
    Ok(report)
}

pub(crate) fn run_rc_gate(
    options: RcGateOptions,
) -> Result<RcGateReport, Box<dyn std::error::Error>> {
    fs::create_dir_all(&options.report_dir)?;

    let replay = build_replay_report(
        &options.root,
        None,
        None,
        SnapshotSubset::All,
        Some(&options.report_dir),
    )?;
    write_replay_report(&options.report_dir, &replay)?;

    let bench_report = bench_smoke_report_from_replay(&replay, SnapshotSubset::All)?;
    write_json(
        &options.report_dir.join("bench-smoke-report.json"),
        &bench_report,
    )?;

    let deterministic_report = build_deterministic_replay_report(&options.root)?;
    write_json(
        &options.report_dir.join("deterministic-replay-report.json"),
        &deterministic_report,
    )?;

    let rollout_matrix_report = build_rollout_matrix_report()?;
    write_json(
        &options.report_dir.join("rollout-matrix-report.json"),
        &rollout_matrix_report,
    )?;

    let human_eval_report_dir = options.report_dir.join("human-eval");
    let human_eval = run_human_eval_kit(&options.root, &human_eval_report_dir)?;

    let manual_metrics = load_manual_metrics_evidence(&options.metrics_manual_report)?;
    let normalized_manual_metrics_path = options.report_dir.join("metrics-manual-eval.json");
    write_json(&normalized_manual_metrics_path, &manual_metrics)?;

    let metrics_report = build_metrics_report(
        &replay,
        &bench_report,
        &rollout_matrix_report,
        &manual_metrics,
        &normalized_manual_metrics_path,
    );
    write_json(
        &options.report_dir.join("metrics-report.json"),
        &metrics_report,
    )?;

    let issue_budget = load_issue_budget_evidence(&options.issue_budget_report)?;
    let normalized_issue_budget_path = options.report_dir.join("issue-budget-evidence.json");
    write_json(&normalized_issue_budget_path, &issue_budget)?;

    let fuzz_smoke = run_fuzz_smoke(&options.fuzz_root, Some(&options.report_dir))?;
    let fuzz = fuzz_evidence_from_report(&fuzz_smoke);
    let normalized_fuzz_path = options.report_dir.join("fuzz-evidence.json");
    write_json(&normalized_fuzz_path, &fuzz)?;
    if let Some(path) = options.fuzz_report.as_ref() {
        write_json(path, &fuzz)?;
    }

    let ux = load_ux_signoff_evidence(&options.ux_signoff_report)?;
    let normalized_ux_path = options.report_dir.join("ux-signoff-evidence.json");
    write_json(&normalized_ux_path, &ux)?;

    let checks = build_rc_gate_checks(
        &replay,
        &bench_report,
        &deterministic_report,
        &rollout_matrix_report,
        &human_eval,
        &metrics_report,
        &issue_budget,
        &normalized_issue_budget_path,
        &fuzz_smoke,
        &fuzz,
        &normalized_fuzz_path,
        &ux,
        &normalized_ux_path,
        options.allow_pending_manual_checks,
    );
    let blockers = blockers_for_checks(&checks);
    let overall_status = if blockers.is_empty() {
        GateStatus::Pass
    } else {
        GateStatus::Fail
    };
    let report = RcGateReport {
        schema_version: RC_GATE_SCHEMA_VERSION,
        generated_at_unix_seconds: unix_now_seconds(),
        root: options.root,
        report_dir: options.report_dir.clone(),
        allow_pending_manual_checks: options.allow_pending_manual_checks,
        overall_status,
        blockers,
        checks,
    };

    write_json(&options.report_dir.join("rc-gate-report.json"), &report)?;
    fs::write(
        options.report_dir.join("rc-gate-summary.md"),
        build_rc_gate_summary(&report),
    )?;

    Ok(report)
}

pub(crate) fn bench_smoke_report_from_replay(
    replay: &ReplayReport,
    subset: SnapshotSubset,
) -> Result<BenchSmokeReport, Box<dyn std::error::Error>> {
    let success_path = measure_success_path_overhead()?;
    let simple_failure = failure_scenario_report(
        replay,
        "simple_failure",
        SIMPLE_FAILURE_P95_TARGET_MS,
        |family| !is_template_heavy_family(family),
    );
    let template_heavy_failure = failure_scenario_report(
        replay,
        "template_heavy_failure",
        TEMPLATE_HEAVY_P95_TARGET_MS,
        is_template_heavy_family,
    );

    let mut blockers = Vec::new();
    for scenario in [&success_path, &simple_failure, &template_heavy_failure] {
        if scenario.status == GateStatus::Fail {
            blockers.push(format!(
                "{} exceeded {}ms budget",
                scenario.scenario, scenario.target_ms
            ));
        }
    }
    let overall_status = if blockers.is_empty() {
        GateStatus::Pass
    } else {
        GateStatus::Fail
    };

    Ok(BenchSmokeReport {
        schema_version: RC_GATE_SCHEMA_VERSION,
        subset: subset_name(subset).to_string(),
        overall_status,
        blockers,
        success_path,
        simple_failure,
        template_heavy_failure,
    })
}

fn failure_scenario_report<F>(
    replay: &ReplayReport,
    scenario: &str,
    target_ms: u64,
    predicate: F,
) -> BenchScenarioReport
where
    F: Fn(&str) -> bool,
{
    let mut samples = Vec::new();
    let mut slowest = Vec::new();
    for fixture in &replay.fixtures {
        if !predicate(&fixture.family_key) {
            continue;
        }
        let postprocess_ms = fixture.parse_time_ms.saturating_add(fixture.render_time_ms);
        samples.push(postprocess_ms);
        slowest.push(SlowFixtureReport {
            fixture_id: fixture.fixture_id.clone(),
            family_key: fixture.family_key.clone(),
            postprocess_ms,
        });
    }
    slowest.sort_by(|left, right| right.postprocess_ms.cmp(&left.postprocess_ms));
    slowest.truncate(3);

    let p95_ms = percentile_95(&samples);
    let status = if samples.is_empty() {
        GateStatus::Fail
    } else if p95_ms.unwrap_or_default() <= target_ms {
        GateStatus::Pass
    } else {
        GateStatus::Fail
    };
    let notes = if samples.is_empty() {
        vec!["no promoted fixtures matched the scenario".to_string()]
    } else {
        Vec::new()
    };

    BenchScenarioReport {
        scenario: scenario.to_string(),
        status,
        metric: "p95_postprocess_ms".to_string(),
        target_ms,
        p95_ms,
        sample_count: samples.len(),
        samples_ms: samples,
        slowest_fixtures: slowest,
        notes,
    }
}

fn measure_success_path_overhead() -> Result<BenchScenarioReport, Box<dyn std::error::Error>> {
    let sandbox = tempfile::tempdir()?;
    let source = sandbox.path().join("success.c");
    fs::write(&source, "int main(void) { return 0; }\n")?;

    let mut probe_cache = ProbeCache::default();
    let backend = probe_cache.get_or_probe(ResolveRequest {
        explicit_backend: None,
        env_backend: None,
        invoked_as: "gcc-formed".to_string(),
    })?;

    let args = vec![
        OsString::from("-fsyntax-only"),
        OsString::from("-x"),
        OsString::from("c"),
        source.as_os_str().to_os_string(),
    ];
    let paths = WrapperPaths {
        config_path: sandbox.path().join("config/config.toml"),
        cache_root: sandbox.path().join("cache"),
        state_root: sandbox.path().join("state"),
        runtime_root: sandbox.path().join("runtime"),
        trace_root: sandbox.path().join("state/traces"),
        install_root: sandbox.path().join("install").join(build_target_triple()),
    };

    for _ in 0..SUCCESS_PATH_WARMUP_RUNS {
        let _ = measure_direct_backend_duration(&backend.resolved_path, sandbox.path(), &args)?;
        let outcome = run_capture(&CaptureRequest {
            backend: backend.clone(),
            args: args.clone(),
            cwd: sandbox.path().to_path_buf(),
            mode: default_execution_mode_for(backend.support_tier),
            capture_passthrough_stderr: false,
            retention: RetentionPolicy::Never,
            paths: paths.clone(),
            structured_capture: if backend.support_tier == SupportTier::A {
                diag_capture_runtime::StructuredCapturePolicy::SarifFile
            } else {
                diag_capture_runtime::StructuredCapturePolicy::Disabled
            },
            preserve_native_color: false,
        })?;
        cleanup_capture(&outcome)?;
    }

    let mut overhead_samples = Vec::new();
    let mut direct_samples = Vec::new();
    let mut wrapper_samples = Vec::new();
    for _ in 0..SUCCESS_PATH_BENCH_SAMPLES {
        let direct_ms =
            measure_direct_backend_duration(&backend.resolved_path, sandbox.path(), &args)?;
        direct_samples.push(direct_ms);
        let outcome = run_capture(&CaptureRequest {
            backend: backend.clone(),
            args: args.clone(),
            cwd: sandbox.path().to_path_buf(),
            mode: default_execution_mode_for(backend.support_tier),
            capture_passthrough_stderr: false,
            retention: RetentionPolicy::Never,
            paths: paths.clone(),
            structured_capture: if backend.support_tier == SupportTier::A {
                diag_capture_runtime::StructuredCapturePolicy::SarifFile
            } else {
                diag_capture_runtime::StructuredCapturePolicy::Disabled
            },
            preserve_native_color: false,
        })?;
        let wrapper_ms = outcome.capture_duration_ms;
        wrapper_samples.push(wrapper_ms);
        overhead_samples.push(wrapper_ms.saturating_sub(direct_ms));
        cleanup_capture(&outcome)?;
    }

    let p95_ms = percentile_95(&overhead_samples);
    let mut notes = Vec::new();
    notes.push(format!(
        "backend={} ({:?})",
        backend.resolved_path.display(),
        backend.support_tier
    ));
    notes.push(format!(
        "direct_p95_ms={}",
        percentile_95(&direct_samples).unwrap_or_default()
    ));
    notes.push(format!(
        "wrapper_p95_ms={}",
        percentile_95(&wrapper_samples).unwrap_or_default()
    ));

    Ok(BenchScenarioReport {
        scenario: "success_path".to_string(),
        status: if p95_ms.unwrap_or_default() <= SUCCESS_PATH_P95_TARGET_MS {
            GateStatus::Pass
        } else {
            GateStatus::Fail
        },
        metric: "p95_wrapper_overhead_ms".to_string(),
        target_ms: SUCCESS_PATH_P95_TARGET_MS,
        p95_ms,
        sample_count: overhead_samples.len(),
        samples_ms: overhead_samples,
        slowest_fixtures: Vec::new(),
        notes,
    })
}

fn measure_direct_backend_duration(
    backend_path: &Path,
    cwd: &Path,
    args: &[OsString],
) -> Result<u64, Box<dyn std::error::Error>> {
    let started = Instant::now();
    let status = Command::new(backend_path)
        .current_dir(cwd)
        .args(args)
        .status()?;
    if !status.success() {
        return Err(format!(
            "direct backend success-path smoke failed: {}",
            backend_path.display()
        )
        .into());
    }
    Ok(started.elapsed().as_millis() as u64)
}

fn build_rollout_matrix_report() -> Result<RolloutMatrixReport, Box<dyn std::error::Error>> {
    let repo_root = repo_root();
    let command = "cargo run -q -p diag_cli_front --bin gcc-formed -- --formed-self-check";
    let output = Command::new("cargo")
        .current_dir(&repo_root)
        .args([
            "run",
            "-q",
            "-p",
            "diag_cli_front",
            "--bin",
            "gcc-formed",
            "--",
            "--formed-self-check",
        ])
        .output()?;
    if !output.status.success() {
        return Err(format!(
            "failed to capture rollout matrix via self-check: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        )
        .into());
    }
    let payload: serde_json::Value = serde_json::from_slice(&output.stdout)?;
    let observed_cases: Vec<RolloutMatrixCase> = serde_json::from_value(
        payload
            .get("rollout_matrix")
            .and_then(|matrix| matrix.get("cases"))
            .cloned()
            .ok_or("self-check output missing rollout_matrix.cases")?,
    )?;
    Ok(compare_rollout_matrix_cases(command, &observed_cases))
}

fn compare_rollout_matrix_cases(
    command: &str,
    observed_cases: &[RolloutMatrixCase],
) -> RolloutMatrixReport {
    let expected_cases = expected_rollout_matrix_cases();
    let mut mismatches = Vec::new();
    for expected in &expected_cases {
        let observed = observed_cases
            .iter()
            .find(|candidate| rollout_case_key(candidate) == rollout_case_key(expected));
        if observed != Some(expected) {
            mismatches.push(RolloutMatrixMismatch {
                key: rollout_case_key(expected),
                expected: Some(expected.clone()),
                observed: observed.cloned(),
            });
        }
    }
    for observed in observed_cases {
        if expected_cases
            .iter()
            .all(|candidate| rollout_case_key(candidate) != rollout_case_key(observed))
        {
            mismatches.push(RolloutMatrixMismatch {
                key: rollout_case_key(observed),
                expected: None,
                observed: Some(observed.clone()),
            });
        }
    }

    RolloutMatrixReport {
        status: if mismatches.is_empty() {
            GateStatus::Pass
        } else {
            GateStatus::Fail
        },
        command: command.to_string(),
        expected_case_count: expected_cases.len(),
        observed_case_count: observed_cases.len(),
        matched_case_count: expected_cases.len().saturating_sub(mismatches.len()),
        mismatches,
    }
}

fn expected_rollout_matrix_cases() -> Vec<RolloutMatrixCase> {
    vec![
        rollout_case("a", None, false, "render", None, None),
        rollout_case(
            "a",
            Some("shadow"),
            false,
            "shadow",
            Some("shadow_mode"),
            None,
        ),
        rollout_case(
            "a",
            Some("passthrough"),
            false,
            "passthrough",
            Some("user_opt_out"),
            None,
        ),
        rollout_case(
            "a",
            Some("render"),
            true,
            "passthrough",
            Some("incompatible_sink"),
            None,
        ),
        rollout_case(
            "b",
            None,
            false,
            "passthrough",
            Some("unsupported_tier"),
            Some(
                "gcc-formed: support tier=b compatibility-only path (GCC 13/14); selected mode=passthrough; fallback reason=unsupported_tier; enhanced render output is not guaranteed and conservative raw diagnostics will be preserved.",
            ),
        ),
        rollout_case(
            "b",
            Some("shadow"),
            false,
            "shadow",
            Some("shadow_mode"),
            Some(
                "gcc-formed: support tier=b compatibility-only path (GCC 13/14); selected mode=shadow; fallback reason=shadow_mode; conservative shadow capture is enabled and enhanced render output is not guaranteed.",
            ),
        ),
        rollout_case(
            "b",
            Some("render"),
            false,
            "passthrough",
            Some("unsupported_tier"),
            Some(
                "gcc-formed: support tier=b compatibility-only path (GCC 13/14); selected mode=passthrough; fallback reason=unsupported_tier; enhanced render output is not guaranteed and conservative raw diagnostics will be preserved.",
            ),
        ),
        rollout_case(
            "c",
            None,
            false,
            "passthrough",
            Some("unsupported_tier"),
            Some(
                "gcc-formed: support tier=c out-of-scope compatibility path; selected mode=passthrough; fallback reason=unsupported_tier; this compiler version is outside the first-release support scope and conservative raw diagnostics will be preserved.",
            ),
        ),
    ]
}

fn rollout_case(
    support_tier: &str,
    requested_mode: Option<&str>,
    hard_conflict: bool,
    selected_mode: &str,
    fallback_reason: Option<&str>,
    scope_notice: Option<&str>,
) -> RolloutMatrixCase {
    RolloutMatrixCase {
        support_tier: support_tier.to_string(),
        requested_mode: requested_mode.map(str::to_string),
        hard_conflict,
        selected_mode: selected_mode.to_string(),
        fallback_reason: fallback_reason.map(str::to_string),
        scope_notice: scope_notice.map(str::to_string),
    }
}

fn rollout_case_key(case: &RolloutMatrixCase) -> String {
    format!(
        "{}:{}:{}",
        case.support_tier,
        case.requested_mode.as_deref().unwrap_or("default"),
        case.hard_conflict
    )
}

fn build_deterministic_replay_report(
    root: &Path,
) -> Result<DeterministicReplayReport, Box<dyn std::error::Error>> {
    let first = build_replay_report(root, None, None, SnapshotSubset::All, None)?;
    let second = build_replay_report(root, None, None, SnapshotSubset::All, None)?;
    let first_hash = diag_core::fingerprint_for(&canonical_deterministic_replay(&first)?);
    let second_hash = diag_core::fingerprint_for(&canonical_deterministic_replay(&second)?);
    Ok(DeterministicReplayReport {
        status: if first_hash == second_hash {
            GateStatus::Pass
        } else {
            GateStatus::Fail
        },
        subset: subset_name(SnapshotSubset::All).to_string(),
        first_hash,
        second_hash,
    })
}

fn canonical_deterministic_replay(
    report: &ReplayReport,
) -> Result<String, Box<dyn std::error::Error>> {
    let mut value = serde_json::to_value(report)?;
    for fixture in value
        .get_mut("fixtures")
        .and_then(|fixtures| fixtures.as_array_mut())
        .into_iter()
        .flatten()
    {
        if let Some(object) = fixture.as_object_mut() {
            object.insert("parse_time_ms".to_string(), json!(0));
            object.insert("render_time_ms".to_string(), json!(0));
        }
    }
    Ok(diag_core::canonical_json(&value)?)
}

fn build_metrics_report(
    replay: &ReplayReport,
    bench: &BenchSmokeReport,
    rollout: &RolloutMatrixReport,
    manual_metrics: &ManualMetricsEvaluation,
    manual_metrics_path: &Path,
) -> RcMetricsReport {
    let fallback = FallbackMetricsReport {
        fallback_rate: replay.metrics.fallback_rate,
        unexpected_fallback_rate: replay.metrics.unexpected_fallback_count as f64
            / replay.metrics.promoted_fixture_count.max(1) as f64,
        fallback_reason_counts: replay.metrics.fallback_reason_counts.clone(),
        unexpected_fallback_reason_counts: replay.metrics.unexpected_fallback_reason_counts.clone(),
    };

    let high_confidence_fixture_count = replay
        .fixtures
        .iter()
        .filter(|fixture| fixture.high_confidence)
        .count();
    let automated_family_mismatch_count = replay
        .fixtures
        .iter()
        .filter(|fixture| fixture.high_confidence && !fixture.family_match)
        .count();
    let automated_family_mismatch_rate =
        automated_family_mismatch_count as f64 / high_confidence_fixture_count.max(1) as f64;

    let compression = compression_metrics_from_replay(replay);
    let first_action = first_action_metrics_from_replay(replay);
    let family_coverage = family_coverage_metrics_from_replay(replay);
    let compatibility_vs_primary = compatibility_metrics_from_rollout(rollout);

    RcMetricsReport {
        schema_version: RC_GATE_SCHEMA_VERSION,
        generated_at_unix_seconds: unix_now_seconds(),
        fallback,
        high_confidence: HighConfidenceMetricsReport {
            high_confidence_fixture_count,
            automated_family_mismatch_count,
            automated_family_mismatch_rate,
            manual_status: manual_metrics.status.clone(),
            manual_reviewed_fixture_count: manual_metrics.reviewed_fixture_count,
            manual_high_confidence_mislead_rate: manual_metrics.high_confidence_mislead_rate,
        },
        native_parity: native_parity_metrics_from_report(&replay.native_parity),
        raw_gcc_comparison: RawGccComparisonMetricsReport {
            compression_ratio: compression,
            first_action_hint: first_action,
            manual_eval_status: manual_metrics.status.clone(),
            manual_trc_improvement_percent: manual_metrics.trc_improvement_percent,
            manual_tfah_improvement_percent: manual_metrics.tfah_improvement_percent,
            manual_first_fix_success_delta_points: manual_metrics.first_fix_success_delta_points,
        },
        performance: PerformanceMetricsReport {
            success_path_p95_overhead_ms: bench.success_path.p95_ms,
            simple_failure_p95_postprocess_ms: bench.simple_failure.p95_ms,
            template_heavy_p95_postprocess_ms: bench.template_heavy_failure.p95_ms,
        },
        family_coverage,
        compatibility_vs_primary,
        manual_evidence_path: relative_report_evidence_path(manual_metrics_path),
        manual_evidence: manual_metrics.clone(),
    }
}

fn native_parity_metrics_from_report(report: &NativeParityReport) -> NativeParityMetricsReport {
    NativeParityMetricsReport {
        covered_dimensions: report.covered_dimensions.clone(),
        failure_counts_by_dimension: report.failure_counts_by_dimension.clone(),
        failing_fixture_count: report.failing_fixtures.len(),
    }
}

fn compression_metrics_from_replay(replay: &ReplayReport) -> CompressionMetricsReport {
    let ratios = replay
        .fixtures
        .iter()
        .filter_map(|fixture| fixture.diagnostic_compression_ratio)
        .collect::<Vec<_>>();
    let mut ratios_by_family: BTreeMap<String, Vec<f64>> = BTreeMap::new();
    for fixture in &replay.fixtures {
        if let Some(ratio) = fixture.diagnostic_compression_ratio {
            ratios_by_family
                .entry(fixture.family_key.clone())
                .or_default()
                .push(ratio);
        }
    }
    let family_median_ratios = ratios_by_family
        .into_iter()
        .filter_map(|(family, values)| median_f64(&values).map(|median| (family, median)))
        .collect();

    CompressionMetricsReport {
        comparable_fixture_count: ratios.len(),
        median_ratio: median_f64(&ratios),
        min_ratio: ratios.iter().cloned().reduce(f64::min),
        max_ratio: ratios.iter().cloned().reduce(f64::max),
        family_median_ratios,
    }
}

fn first_action_metrics_from_replay(replay: &ReplayReport) -> FirstActionMetricsReport {
    let rendered_lines = replay
        .fixtures
        .iter()
        .filter_map(|fixture| fixture.rendered_first_action_line)
        .collect::<Vec<_>>();
    let within_first_screenful_count = rendered_lines
        .iter()
        .filter(|line| **line <= FIRST_ACTION_SCREENFUL_LINE)
        .count();
    FirstActionMetricsReport {
        rendered_hint_fixture_count: rendered_lines.len(),
        median_line: median_usize(&rendered_lines),
        max_line: rendered_lines.iter().copied().max(),
        within_first_screenful_count,
        within_first_screenful_rate: (!rendered_lines.is_empty())
            .then_some(within_first_screenful_count as f64 / rendered_lines.len() as f64),
    }
}

fn family_coverage_metrics_from_replay(replay: &ReplayReport) -> FamilyCoverageMetricsReport {
    let missing_required_families = REQUIRED_METRIC_FAMILIES
        .iter()
        .filter(|family| !replay.selected_family_counts.contains_key(**family))
        .map(|family| (*family).to_string())
        .collect::<Vec<_>>();
    FamilyCoverageMetricsReport {
        observed_family_counts: replay.selected_family_counts.clone(),
        required_family_count: REQUIRED_METRIC_FAMILIES.len(),
        covered_required_family_count: REQUIRED_METRIC_FAMILIES
            .iter()
            .filter(|family| replay.selected_family_counts.contains_key(**family))
            .count(),
        missing_required_families,
    }
}

fn compatibility_metrics_from_rollout(
    rollout: &RolloutMatrixReport,
) -> CompatibilityVsPrimaryMetricsReport {
    let expected_cases = expected_rollout_matrix_cases();
    let lookup = |support_tier: &str, requested_mode: Option<&str>, hard_conflict: bool| {
        expected_cases
            .iter()
            .find(|case| {
                case.support_tier == support_tier
                    && case.requested_mode.as_deref() == requested_mode
                    && case.hard_conflict == hard_conflict
            })
            .map(|case| case.selected_mode.clone())
    };
    let tier_a_default_mode = lookup("a", None, false);
    let tier_b_default_mode = lookup("b", None, false);
    let tier_c_default_mode = lookup("c", None, false);
    let tier_b_requested_render_mode = lookup("b", Some("render"), false);
    let tier_b_requested_shadow_mode = lookup("b", Some("shadow"), false);
    CompatibilityVsPrimaryMetricsReport {
        rollout_matrix_status: rollout.status.clone(),
        primary_enhanced_default: tier_a_default_mode.as_deref() == Some("render"),
        compatibility_default_is_conservative: tier_b_default_mode.as_deref()
            == Some("passthrough")
            && tier_c_default_mode.as_deref() == Some("passthrough"),
        tier_a_default_mode,
        tier_b_default_mode,
        tier_c_default_mode,
        tier_b_requested_render_mode,
        tier_b_requested_shadow_mode,
    }
}

fn build_rc_gate_checks(
    replay: &ReplayReport,
    bench: &BenchSmokeReport,
    deterministic: &DeterministicReplayReport,
    rollout: &RolloutMatrixReport,
    human_eval: &HumanEvalKitReport,
    metrics: &RcMetricsReport,
    issue_budget: &IssueBudgetEvidence,
    issue_budget_path: &Path,
    fuzz_smoke: &FuzzSmokeReport,
    fuzz: &FuzzEvidence,
    fuzz_path: &Path,
    ux: &UxSignoffEvidence,
    ux_path: &Path,
    allow_pending_manual_checks: bool,
) -> Vec<RcGateCheck> {
    let unexpected_fallback_rate = replay.metrics.unexpected_fallback_count as f64
        / replay.metrics.promoted_fixture_count.max(1) as f64;
    vec![
        curated_corpus_check(replay),
        RcGateCheck {
            id: "rollout_matrix".to_string(),
            title: "Rollout Mode Matrix Pass 100%".to_string(),
            status: rollout.status.clone(),
            summary: format!(
                "matched {}/{} expected cases",
                rollout.matched_case_count, rollout.expected_case_count
            ),
            blocker: rollout.status == GateStatus::Fail,
            manual: false,
            evidence_path: Some(PathBuf::from("rollout-matrix-report.json")),
        },
        RcGateCheck {
            id: "unexpected_fallback".to_string(),
            title: "Unexpected Fallback Threshold".to_string(),
            status: if replay.metrics.unexpected_fallback_count == 0
                && unexpected_fallback_rate < UNEXPECTED_FALLBACK_RATE_TARGET
            {
                GateStatus::Pass
            } else {
                GateStatus::Fail
            },
            summary: format!(
                "unexpected fallback count={} rate={:.4}",
                replay.metrics.unexpected_fallback_count, unexpected_fallback_rate
            ),
            blocker: replay.metrics.unexpected_fallback_count > 0,
            manual: false,
            evidence_path: Some(PathBuf::from("replay-report.json")),
        },
        RcGateCheck {
            id: "native_parity_stop_ship".to_string(),
            title: "Native Parity Stop-Ship".to_string(),
            status: if replay.native_parity.failing_fixtures.is_empty() {
                GateStatus::Pass
            } else {
                GateStatus::Fail
            },
            summary: format!(
                "coverage={} failure_counts={}",
                replay.native_parity.covered_dimensions.len(),
                serde_json::to_string(&replay.native_parity.failure_counts_by_dimension)
                    .unwrap_or_else(|_| "{}".to_string())
            ),
            blocker: !replay.native_parity.failing_fixtures.is_empty(),
            manual: false,
            evidence_path: Some(PathBuf::from("native-parity-report.json")),
        },
        RcGateCheck {
            id: "benchmark_budget".to_string(),
            title: "Benchmark Budgets".to_string(),
            status: bench.overall_status.clone(),
            summary: format!(
                "success_path={:?}, simple_failure={:?}, template_heavy={:?}",
                bench.success_path.status,
                bench.simple_failure.status,
                bench.template_heavy_failure.status
            ),
            blocker: bench.overall_status == GateStatus::Fail,
            manual: false,
            evidence_path: Some(PathBuf::from("bench-smoke-report.json")),
        },
        RcGateCheck {
            id: "deterministic_replay".to_string(),
            title: "Deterministic Replay".to_string(),
            status: deterministic.status.clone(),
            summary: format!(
                "first_hash={} second_hash={}",
                deterministic.first_hash, deterministic.second_hash
            ),
            blocker: deterministic.status == GateStatus::Fail,
            manual: false,
            evidence_path: Some(PathBuf::from("deterministic-replay-report.json")),
        },
        human_eval_kit_check(human_eval),
        manual_metrics_check(metrics, allow_pending_manual_checks),
        manual_issue_budget_check(issue_budget, issue_budget_path, allow_pending_manual_checks),
        fuzz_check(fuzz_smoke, fuzz, fuzz_path),
        manual_ux_check(ux, ux_path, allow_pending_manual_checks),
    ]
}

fn curated_corpus_check(replay: &ReplayReport) -> RcGateCheck {
    let status = if replay.failures.is_empty() && replay.promoted_failed == 0 {
        GateStatus::Pass
    } else {
        GateStatus::Fail
    };
    RcGateCheck {
        id: "curated_corpus".to_string(),
        title: "Curated Corpus Pass 100%".to_string(),
        status: status.clone(),
        summary: curated_corpus_summary(replay),
        blocker: status == GateStatus::Fail,
        manual: false,
        evidence_path: Some(PathBuf::from("replay-report.json")),
    }
}

fn curated_corpus_summary(replay: &ReplayReport) -> String {
    let mut highlights = Vec::new();
    for cell in replay
        .coverage
        .missing_required_band_path_surfaces
        .iter()
        .take(3)
    {
        highlights.push(format!("matrix_hole={cell}"));
    }
    for cell in replay.coverage.missing_required_band_paths.iter().take(3) {
        highlights.push(format!("matrix_path_hole={cell}"));
    }
    let remaining_slots = 3usize.saturating_sub(highlights.len());
    for failure in replay
        .failures
        .iter()
        .filter(|failure| {
            !matches!(
                failure.layer.as_str(),
                "coverage.band_path_surface" | "coverage.band_path"
            )
        })
        .take(remaining_slots)
    {
        highlights.push(replay_failure_highlight(replay, failure));
    }

    if highlights.is_empty() {
        format!(
            "verified {}/{} promoted fixtures",
            replay.promoted_verified, replay.promoted_fixture_count
        )
    } else {
        format!(
            "verified {}/{} promoted fixtures; blockers={}",
            replay.promoted_verified,
            replay.promoted_fixture_count,
            highlights.join("; ")
        )
    }
}

fn replay_failure_highlight(
    replay: &ReplayReport,
    failure: &crate::commands::corpus::VerificationFailure,
) -> String {
    let fixture = replay
        .fixtures
        .iter()
        .find(|fixture| fixture.fixture_id == failure.fixture_id);
    let support_band = fixture
        .map(|fixture| fixture.support_band.as_str())
        .unwrap_or("unknown_band");
    let processing_path = fixture
        .map(|fixture| fixture.processing_path.as_str())
        .unwrap_or("unknown_path");
    let concern = replay_failure_concern(&failure.layer);
    match replay_failure_surface(&failure.layer) {
        Some(surface) => format!(
            "{support_band}/{processing_path}/{surface} concern={concern} fixture={} {}",
            failure.fixture_id, failure.summary
        ),
        None => format!(
            "{support_band}/{processing_path} concern={concern} fixture={} {}",
            failure.fixture_id, failure.summary
        ),
    }
}

fn replay_failure_surface(layer: &str) -> Option<&str> {
    let mut parts = layer.split('.');
    match (parts.next(), parts.next(), parts.next()) {
        (Some("render"), Some(surface), Some(_)) => Some(surface),
        _ => None,
    }
}

fn replay_failure_concern(layer: &str) -> String {
    if layer.ends_with(".ansi") {
        return "color_meaning".to_string();
    }
    if layer.ends_with(".line_budget") {
        return "line_budget".to_string();
    }
    if layer.ends_with(".first_action_visibility") {
        return "first_action_visibility".to_string();
    }
    if layer.ends_with(".omission_notice")
        || layer.ends_with(".partial_notice")
        || layer.ends_with(".raw_disclosure")
        || layer.ends_with(".raw_sub_block")
        || layer.ends_with(".low_confidence_notice")
    {
        return "disclosure_honesty".to_string();
    }
    if layer.ends_with(".compaction") {
        return "compaction".to_string();
    }
    if let Some(rest) = layer.strip_prefix("render.") {
        let mut parts = rest.split('.');
        let _surface = parts.next();
        if let Some(concern) = parts.next() {
            return concern.to_string();
        }
    }
    layer.to_string()
}

fn human_eval_kit_check(report: &HumanEvalKitReport) -> RcGateCheck {
    let status = if human_eval_kit_is_complete(report) {
        GateStatus::Pass
    } else {
        GateStatus::Fail
    };
    RcGateCheck {
        id: "human_eval_kit".to_string(),
        title: "Human Evaluation Kit".to_string(),
        status: status.clone(),
        summary: format!(
            "expert_fixtures={} task_study_fixtures={} missing_required_families={}",
            report.expert_review_fixture_count,
            report.task_study_fixture_count,
            if report.missing_required_families.is_empty() {
                "none".to_string()
            } else {
                report.missing_required_families.join(",")
            }
        ),
        blocker: status == GateStatus::Fail,
        manual: false,
        evidence_path: Some(PathBuf::from("human-eval/human-eval-report.json")),
    }
}

fn manual_metrics_check(
    metrics: &RcMetricsReport,
    allow_pending_manual_checks: bool,
) -> RcGateCheck {
    let manual = &metrics.manual_evidence;
    let status = match manual.status {
        ManualEvidenceStatus::Approved if manual_metrics_fields_complete(manual) => {
            GateStatus::Pass
        }
        ManualEvidenceStatus::Pending => GateStatus::Pending,
        _ => GateStatus::Fail,
    };
    RcGateCheck {
        id: "metrics_packet".to_string(),
        title: "RC Metrics Packet".to_string(),
        status: status.clone(),
        summary: format!(
            "status={:?} compression_median={:?} first_action_median={:?} overhead_p95_ms={:?}",
            manual.status,
            metrics.raw_gcc_comparison.compression_ratio.median_ratio,
            metrics.raw_gcc_comparison.first_action_hint.median_line,
            metrics.performance.success_path_p95_overhead_ms
        ),
        blocker: check_is_blocker(&status, true, allow_pending_manual_checks),
        manual: true,
        evidence_path: Some(PathBuf::from("metrics-report.json")),
    }
}

fn manual_metrics_fields_complete(manual: &ManualMetricsEvaluation) -> bool {
    manual.high_confidence_mislead_rate.is_some()
        && manual.trc_improvement_percent.is_some()
        && manual.tfah_improvement_percent.is_some()
        && manual.first_fix_success_delta_points.is_some()
}

fn manual_issue_budget_check(
    issue_budget: &IssueBudgetEvidence,
    issue_budget_path: &Path,
    allow_pending_manual_checks: bool,
) -> RcGateCheck {
    let status = match issue_budget.status {
        ManualEvidenceStatus::Approved
            if issue_budget.open_p0 == 0 && issue_budget.open_p1 == 0 =>
        {
            GateStatus::Pass
        }
        ManualEvidenceStatus::Pending => GateStatus::Pending,
        _ => GateStatus::Fail,
    };
    RcGateCheck {
        id: "issue_budget".to_string(),
        title: "P0/P1 Open Bug Budget".to_string(),
        status: status.clone(),
        summary: format!(
            "status={:?} open_p0={} open_p1={}",
            issue_budget.status, issue_budget.open_p0, issue_budget.open_p1
        ),
        blocker: check_is_blocker(&status, true, allow_pending_manual_checks),
        manual: true,
        evidence_path: Some(relative_report_evidence_path(issue_budget_path)),
    }
}

fn fuzz_check(fuzz_smoke: &FuzzSmokeReport, fuzz: &FuzzEvidence, fuzz_path: &Path) -> RcGateCheck {
    let status = if fuzz_smoke.overall_status == FuzzSmokeStatus::Pass
        && fuzz.crash_count == 0
        && fuzz.corpus_replay_passed
    {
        GateStatus::Pass
    } else {
        GateStatus::Fail
    };
    RcGateCheck {
        id: "fuzz".to_string(),
        title: "Fuzz Crash 0".to_string(),
        status: status.clone(),
        summary: format!(
            "status={:?} crash_count={} corpus_replay_passed={} seed_cases={} failed_cases={} budget_violations={}",
            fuzz.status,
            fuzz.crash_count,
            fuzz.corpus_replay_passed,
            fuzz_smoke.case_count,
            fuzz_smoke.failed_case_count,
            fuzz_smoke.budget_violation_count
        ),
        blocker: status == GateStatus::Fail,
        manual: false,
        evidence_path: Some(relative_report_evidence_path(fuzz_path)),
    }
}

fn manual_ux_check(
    ux: &UxSignoffEvidence,
    ux_path: &Path,
    allow_pending_manual_checks: bool,
) -> RcGateCheck {
    let status = match ux.status {
        ManualEvidenceStatus::Approved => GateStatus::Pass,
        ManualEvidenceStatus::Pending => GateStatus::Pending,
        ManualEvidenceStatus::Rejected => GateStatus::Fail,
    };
    RcGateCheck {
        id: "ux_review".to_string(),
        title: "UX Review Sign-off".to_string(),
        status: status.clone(),
        summary: format!(
            "status={:?} reviewers={}",
            ux.status,
            ux.reviewers.join(",")
        ),
        blocker: check_is_blocker(&status, true, allow_pending_manual_checks),
        manual: true,
        evidence_path: Some(relative_report_evidence_path(ux_path)),
    }
}

fn check_is_blocker(status: &GateStatus, manual: bool, allow_pending_manual_checks: bool) -> bool {
    match status {
        GateStatus::Pass => false,
        GateStatus::Fail => true,
        GateStatus::Pending => manual && !allow_pending_manual_checks,
    }
}

fn blockers_for_checks(checks: &[RcGateCheck]) -> Vec<RcGateBlocker> {
    checks
        .iter()
        .filter(|check| check.blocker)
        .map(|check| RcGateBlocker {
            id: check.id.clone(),
            summary: format!("{}: {}", check.title, check.summary),
            evidence_path: check.evidence_path.clone(),
        })
        .collect()
}

fn build_rc_gate_summary(report: &RcGateReport) -> String {
    let mut lines = vec![
        "# RC Gate Summary".to_string(),
        String::new(),
        format!("- Overall status: `{:?}`", report.overall_status),
        format!(
            "- Pending manual evidence allowed: `{}`",
            report.allow_pending_manual_checks
        ),
        format!("- Blocker count: `{}`", report.blockers.len()),
    ];
    if report.blockers.is_empty() {
        lines.push("- Ship blockers: none".to_string());
    } else {
        lines.push("- Ship blockers:".to_string());
        for blocker in &report.blockers {
            lines.push(format!("  - `{}` {}", blocker.id, blocker.summary));
        }
    }
    lines.extend(
        [
            String::new(),
            "| Check | Status | Summary |".to_string(),
            "| --- | --- | --- |".to_string(),
        ]
        .into_iter(),
    );
    for check in &report.checks {
        lines.push(format!(
            "| {} | `{:?}` | {} |",
            check.title, check.status, check.summary
        ));
    }
    lines.join("\n") + "\n"
}

fn load_issue_budget_evidence(
    path: &Path,
) -> Result<IssueBudgetEvidence, Box<dyn std::error::Error>> {
    if path.exists() {
        return Ok(serde_json::from_slice(&fs::read(path)?)?);
    }
    Ok(IssueBudgetEvidence {
        schema_version: RC_GATE_SCHEMA_VERSION,
        release_candidate: "1.0.0-rc.N".to_string(),
        status: ManualEvidenceStatus::Pending,
        open_p0: 0,
        open_p1: 0,
        updated_at: "missing".to_string(),
        notes: vec![format!("missing manual evidence: {}", path.display())],
    })
}

fn fuzz_evidence_from_report(report: &FuzzSmokeReport) -> FuzzEvidence {
    let mut notes = report
        .cases
        .iter()
        .filter(|case| case.status != crate::commands::fuzz::FuzzCaseStatus::Pass)
        .map(|case| format!("{}: {}", case.id, case.summary))
        .take(5)
        .collect::<Vec<_>>();
    if notes.is_empty() {
        notes.push(format!(
            "seed suite passed with {} cases",
            report.case_count
        ));
    }
    FuzzEvidence {
        schema_version: RC_GATE_SCHEMA_VERSION,
        release_candidate: "1.0.0-rc.N".to_string(),
        status: if report.overall_status == FuzzSmokeStatus::Pass {
            ManualEvidenceStatus::Approved
        } else {
            ManualEvidenceStatus::Rejected
        },
        crash_count: report.crash_count,
        corpus_replay_passed: report.corpus_replay_passed,
        updated_at: unix_now_seconds().to_string(),
        notes,
    }
}

fn load_ux_signoff_evidence(path: &Path) -> Result<UxSignoffEvidence, Box<dyn std::error::Error>> {
    if path.exists() {
        return Ok(serde_json::from_slice(&fs::read(path)?)?);
    }
    Ok(UxSignoffEvidence {
        schema_version: RC_GATE_SCHEMA_VERSION,
        release_candidate: "1.0.0-rc.N".to_string(),
        status: ManualEvidenceStatus::Pending,
        updated_at: "missing".to_string(),
        reviewers: Vec::new(),
        notes: vec![format!("missing manual evidence: {}", path.display())],
    })
}

fn load_manual_metrics_evidence(
    path: &Path,
) -> Result<ManualMetricsEvaluation, Box<dyn std::error::Error>> {
    if path.exists() {
        return Ok(serde_json::from_slice(&fs::read(path)?)?);
    }
    Ok(ManualMetricsEvaluation {
        schema_version: RC_GATE_SCHEMA_VERSION,
        release_candidate: "1.0.0-rc.N".to_string(),
        status: ManualEvidenceStatus::Pending,
        reviewed_fixture_count: 0,
        high_confidence_mislead_rate: None,
        trc_improvement_percent: None,
        tfah_improvement_percent: None,
        first_fix_success_delta_points: None,
        updated_at: "missing".to_string(),
        reviewers: Vec::new(),
        notes: vec![format!("missing manual evidence: {}", path.display())],
    })
}

fn write_json<T: Serialize>(path: &Path, value: &T) -> Result<(), Box<dyn std::error::Error>> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, serde_json::to_vec_pretty(value)?)?;
    Ok(())
}

fn percentile_95(samples: &[u64]) -> Option<u64> {
    if samples.is_empty() {
        return None;
    }
    let mut ordered = samples.to_vec();
    ordered.sort_unstable();
    let index = ((ordered.len() * 95).saturating_sub(1)) / 100;
    ordered.get(index).copied()
}

fn median_usize(samples: &[usize]) -> Option<usize> {
    if samples.is_empty() {
        return None;
    }
    let mut ordered = samples.to_vec();
    ordered.sort_unstable();
    ordered.get(ordered.len() / 2).copied()
}

fn median_f64(samples: &[f64]) -> Option<f64> {
    if samples.is_empty() {
        return None;
    }
    let mut ordered = samples.to_vec();
    ordered.sort_by(|left, right| left.total_cmp(right));
    ordered.get(ordered.len() / 2).copied()
}

fn is_template_heavy_family(family: &str) -> bool {
    matches!(family, "template" | "overload" | "linker")
}

fn default_execution_mode_for(tier: SupportTier) -> ExecutionMode {
    match tier {
        SupportTier::A => ExecutionMode::Render,
        SupportTier::B | SupportTier::C => ExecutionMode::Passthrough,
    }
}

fn relative_report_evidence_path(path: &Path) -> PathBuf {
    path.file_name()
        .map(PathBuf::from)
        .unwrap_or_else(|| path.to_path_buf())
}

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .to_path_buf()
}

fn unix_now_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::corpus::{
        AcceptanceFixtureSummary, FixtureCoverageReport, NativeParityReport, ReplayReport,
        VerificationFailure, acceptance_metrics_for,
    };
    use std::collections::BTreeMap;

    fn fixture_summary(
        fixture_id: &str,
        family_key: &str,
        parse_time_ms: u64,
        render_time_ms: u64,
    ) -> AcceptanceFixtureSummary {
        AcceptanceFixtureSummary {
            fixture_id: fixture_id.to_string(),
            family_key: family_key.to_string(),
            title: None,
            support_band: "gcc15_plus".to_string(),
            processing_path: "dual_sink_structured".to_string(),
            fallback_contract: "bounded_render".to_string(),
            expected_family: Some(family_key.to_string()),
            actual_family: family_key.to_string(),
            family_match: true,
            used_fallback: false,
            fallback_reason: None,
            fallback_forbidden: false,
            unexpected_fallback: false,
            primary_location_path: Some("src/main.c".to_string()),
            primary_location_user_owned_required: false,
            primary_location_user_owned: true,
            missing_required_primary_location: false,
            first_action_required: false,
            first_action_present: true,
            missing_required_first_action: false,
            headline_rewritten: true,
            lead_confidence: "high".to_string(),
            high_confidence: true,
            rendered_first_action_line: Some(3),
            omission_notice_present: false,
            partial_notice_present: false,
            raw_diagnostics_hint_present: true,
            raw_sub_block_present: false,
            low_confidence_notice_present: false,
            within_first_screenful_budget: true,
            first_action_within_budget: None,
            native_parity_dimensions: Vec::new(),
            raw_line_count: 12,
            rendered_line_count: 6,
            diagnostic_compression_ratio: Some(2.0),
            parse_time_ms,
            render_time_ms,
            verified: true,
        }
    }

    fn replay_report(fixtures: Vec<AcceptanceFixtureSummary>) -> ReplayReport {
        ReplayReport {
            family_counts: BTreeMap::new(),
            selected_family_counts: BTreeMap::new(),
            coverage: FixtureCoverageReport::default(),
            selected_fixture_count: fixtures.len(),
            promoted_fixture_count: fixtures.len(),
            promoted_verified: fixtures.len(),
            promoted_failed: 0,
            subset: "all".to_string(),
            metrics: acceptance_metrics_for(&fixtures),
            native_parity: NativeParityReport::default(),
            fixtures,
            failures: Vec::new(),
        }
    }

    fn bench_report() -> BenchSmokeReport {
        BenchSmokeReport {
            schema_version: RC_GATE_SCHEMA_VERSION,
            subset: "all".to_string(),
            overall_status: GateStatus::Pass,
            blockers: Vec::new(),
            success_path: BenchScenarioReport {
                scenario: "success_path".to_string(),
                status: GateStatus::Pass,
                metric: "p95_wrapper_overhead_ms".to_string(),
                target_ms: SUCCESS_PATH_P95_TARGET_MS,
                p95_ms: Some(3),
                sample_count: 5,
                samples_ms: vec![1, 2, 3, 3, 3],
                slowest_fixtures: Vec::new(),
                notes: Vec::new(),
            },
            simple_failure: BenchScenarioReport {
                scenario: "simple_failure".to_string(),
                status: GateStatus::Pass,
                metric: "p95_postprocess_ms".to_string(),
                target_ms: SIMPLE_FAILURE_P95_TARGET_MS,
                p95_ms: Some(18),
                sample_count: 5,
                samples_ms: vec![10, 12, 15, 18, 18],
                slowest_fixtures: Vec::new(),
                notes: Vec::new(),
            },
            template_heavy_failure: BenchScenarioReport {
                scenario: "template_heavy_failure".to_string(),
                status: GateStatus::Pass,
                metric: "p95_postprocess_ms".to_string(),
                target_ms: TEMPLATE_HEAVY_P95_TARGET_MS,
                p95_ms: Some(40),
                sample_count: 5,
                samples_ms: vec![20, 24, 30, 40, 40],
                slowest_fixtures: Vec::new(),
                notes: Vec::new(),
            },
        }
    }

    fn manual_metrics(status: ManualEvidenceStatus) -> ManualMetricsEvaluation {
        ManualMetricsEvaluation {
            schema_version: RC_GATE_SCHEMA_VERSION,
            release_candidate: "1.0.0-rc.1".to_string(),
            status,
            reviewed_fixture_count: 10,
            high_confidence_mislead_rate: Some(0.01),
            trc_improvement_percent: Some(35.0),
            tfah_improvement_percent: Some(50.0),
            first_fix_success_delta_points: Some(20.0),
            updated_at: "2026-04-09".to_string(),
            reviewers: vec!["reviewer".to_string()],
            notes: Vec::new(),
        }
    }

    #[test]
    fn benchmark_groups_template_heavy_families_separately() {
        let replay = replay_report(vec![
            fixture_summary("c/syntax/case-01", "syntax", 20, 10),
            fixture_summary("cpp/template/case-01", "template", 120, 40),
            fixture_summary("c/linker/case-01", "linker", 100, 60),
        ]);

        let simple = failure_scenario_report(
            &replay,
            "simple_failure",
            SIMPLE_FAILURE_P95_TARGET_MS,
            |family| !is_template_heavy_family(family),
        );
        let heavy = failure_scenario_report(
            &replay,
            "template_heavy_failure",
            TEMPLATE_HEAVY_P95_TARGET_MS,
            is_template_heavy_family,
        );

        assert_eq!(simple.sample_count, 1);
        assert_eq!(simple.p95_ms, Some(30));
        assert_eq!(heavy.sample_count, 2);
        assert_eq!(heavy.p95_ms, Some(160));
    }

    #[test]
    fn percentile_95_uses_stable_ordering() {
        assert_eq!(percentile_95(&[]), None);
        assert_eq!(percentile_95(&[10]), Some(10));
        assert_eq!(percentile_95(&[10, 20, 30, 40, 50]), Some(50));
    }

    #[test]
    fn deterministic_replay_projection_ignores_timing_fields() {
        let mut left = replay_report(vec![fixture_summary("a", "syntax", 10, 5)]);
        let mut right = replay_report(vec![fixture_summary("a", "syntax", 99, 77)]);
        left.metrics = acceptance_metrics_for(&left.fixtures);
        right.metrics = acceptance_metrics_for(&right.fixtures);

        let left_projection = canonical_deterministic_replay(&left).unwrap();
        let right_projection = canonical_deterministic_replay(&right).unwrap();

        assert_eq!(left_projection, right_projection);
    }

    #[test]
    fn rollout_matrix_comparison_flags_drift() {
        let mut observed = expected_rollout_matrix_cases();
        observed[0].selected_mode = "shadow".to_string();

        let report = compare_rollout_matrix_cases("cargo run ...", &observed);

        assert_eq!(report.status, GateStatus::Fail);
        assert_eq!(report.mismatches.len(), 1);
    }

    #[test]
    fn pending_manual_checks_stop_strict_rc_gate() {
        let check = manual_issue_budget_check(
            &IssueBudgetEvidence {
                schema_version: RC_GATE_SCHEMA_VERSION,
                release_candidate: "1.0.0-rc.1".to_string(),
                status: ManualEvidenceStatus::Pending,
                open_p0: 0,
                open_p1: 0,
                updated_at: "2026-04-09".to_string(),
                notes: Vec::new(),
            },
            Path::new("issue-budget.json"),
            false,
        );

        assert_eq!(check.status, GateStatus::Pending);
        assert!(check.blocker);
    }

    #[test]
    fn metrics_report_summarizes_corpus_kpis() {
        let mut fixtures = vec![
            fixture_summary("c/syntax/case-01", "syntax", 20, 10),
            fixture_summary("cpp/template/case-01", "template", 40, 20),
            fixture_summary("c/linker/case-01", "linker", 30, 10),
        ];
        fixtures[0].rendered_first_action_line = Some(3);
        fixtures[0].diagnostic_compression_ratio = Some(2.0);
        fixtures[1].rendered_first_action_line = Some(5);
        fixtures[1].diagnostic_compression_ratio = Some(3.0);
        fixtures[2].rendered_first_action_line = None;
        fixtures[2].diagnostic_compression_ratio = Some(1.5);
        fixtures[2].high_confidence = false;
        fixtures[2].lead_confidence = "medium".to_string();

        let mut replay = replay_report(fixtures);
        replay.selected_family_counts = BTreeMap::from([
            ("syntax".to_string(), 1),
            ("template".to_string(), 1),
            ("linker".to_string(), 1),
        ]);
        let rollout =
            compare_rollout_matrix_cases("cargo run ...", &expected_rollout_matrix_cases());
        let report = build_metrics_report(
            &replay,
            &bench_report(),
            &rollout,
            &manual_metrics(ManualEvidenceStatus::Pending),
            Path::new("metrics-manual-eval.json"),
        );

        assert_eq!(
            report.raw_gcc_comparison.compression_ratio.median_ratio,
            Some(2.0)
        );
        assert_eq!(
            report.raw_gcc_comparison.first_action_hint.median_line,
            Some(5)
        );
        assert_eq!(report.performance.success_path_p95_overhead_ms, Some(3));
        assert_eq!(
            report.family_coverage.missing_required_families,
            vec![
                "macro_include".to_string(),
                "type".to_string(),
                "overload".to_string()
            ]
        );
        assert!(report.compatibility_vs_primary.primary_enhanced_default);
        assert!(
            report
                .compatibility_vs_primary
                .compatibility_default_is_conservative
        );
    }

    #[test]
    fn approved_metrics_packet_requires_all_manual_fields() {
        let replay = replay_report(vec![fixture_summary("c/syntax/case-01", "syntax", 20, 10)]);
        let rollout =
            compare_rollout_matrix_cases("cargo run ...", &expected_rollout_matrix_cases());
        let mut manual = manual_metrics(ManualEvidenceStatus::Approved);
        manual.tfah_improvement_percent = None;
        let report = build_metrics_report(
            &replay,
            &bench_report(),
            &rollout,
            &manual,
            Path::new("metrics-manual-eval.json"),
        );

        let check = manual_metrics_check(&report, false);

        assert_eq!(check.status, GateStatus::Fail);
        assert!(check.blocker);
    }

    #[test]
    fn incomplete_human_eval_kit_blocks_rc_gate() {
        let report = HumanEvalKitReport {
            schema_version: 1,
            generated_at_unix_seconds: 0,
            root: PathBuf::from("corpus"),
            report_dir: PathBuf::from("target/human-eval"),
            expert_review_fixture_count: 3,
            task_study_fixture_count: 3,
            family_counts: BTreeMap::new(),
            covered_required_families: vec!["syntax".to_string()],
            missing_required_families: vec!["template".to_string()],
            fixtures: Vec::new(),
            task_study_matrix: Vec::new(),
        };

        let check = human_eval_kit_check(&report);

        assert_eq!(check.status, GateStatus::Fail);
        assert!(check.blocker);
    }

    #[test]
    fn curated_corpus_check_blocks_path_aware_matrix_holes() {
        let mut replay = replay_report(vec![fixture_summary("c/syntax/case-01", "syntax", 20, 10)]);
        replay.coverage.missing_required_band_path_surfaces =
            vec!["gcc13_14/native_text_capture/ci".to_string()];
        replay.failures.push(VerificationFailure {
            layer: "coverage.band_path_surface".to_string(),
            fixture_id: "corpus".to_string(),
            summary: "representative coverage missing required band/path/surface combinations: gcc13_14/native_text_capture/ci".to_string(),
        });

        let check = curated_corpus_check(&replay);

        assert_eq!(check.status, GateStatus::Fail);
        assert!(check.blocker);
        assert!(check.summary.contains("gcc13_14/native_text_capture/ci"));
    }

    #[test]
    fn curated_corpus_check_surfaces_band_path_surface_and_concern() {
        let mut replay = replay_report(vec![fixture_summary("c/syntax/case-09", "syntax", 20, 10)]);
        replay.fixtures[0].support_band = "gcc9_12".to_string();
        replay.fixtures[0].processing_path = "native_text_capture".to_string();
        replay.failures.push(VerificationFailure {
            layer: "render.ci.line_budget".to_string(),
            fixture_id: "c/syntax/case-09".to_string(),
            summary: "rendered 20 lines, budget is 14".to_string(),
        });

        let check = curated_corpus_check(&replay);

        assert_eq!(check.status, GateStatus::Fail);
        assert!(check.blocker);
        assert!(check.summary.contains("gcc9_12/native_text_capture/ci"));
        assert!(check.summary.contains("line_budget"));
    }
}
