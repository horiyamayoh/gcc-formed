use crate::SnapshotSubset;
use crate::commands::corpus::{
    ReplayReport, build_replay_report, subset_name, write_replay_report,
};
use diag_backend_probe::{ProbeCache, ResolveRequest, SupportTier};
use diag_capture_runtime::{CaptureRequest, ExecutionMode, cleanup_capture, run_capture};
use diag_trace::{RetentionPolicy, WrapperPaths, build_target_triple};
use serde::{Deserialize, Serialize};
use serde_json::json;
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

#[derive(Debug, Clone)]
pub(crate) struct RcGateOptions {
    pub(crate) root: PathBuf,
    pub(crate) report_dir: PathBuf,
    pub(crate) issue_budget_report: PathBuf,
    pub(crate) fuzz_report: PathBuf,
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) struct RolloutMatrixCase {
    pub(crate) support_tier: String,
    pub(crate) requested_mode: Option<String>,
    pub(crate) hard_conflict: bool,
    pub(crate) selected_mode: String,
    pub(crate) fallback_reason: Option<String>,
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

    let issue_budget = load_issue_budget_evidence(&options.issue_budget_report)?;
    let normalized_issue_budget_path = options.report_dir.join("issue-budget-evidence.json");
    write_json(&normalized_issue_budget_path, &issue_budget)?;

    let fuzz = load_fuzz_evidence(&options.fuzz_report)?;
    let normalized_fuzz_path = options.report_dir.join("fuzz-evidence.json");
    write_json(&normalized_fuzz_path, &fuzz)?;

    let ux = load_ux_signoff_evidence(&options.ux_signoff_report)?;
    let normalized_ux_path = options.report_dir.join("ux-signoff-evidence.json");
    write_json(&normalized_ux_path, &ux)?;

    let checks = build_rc_gate_checks(
        &replay,
        &bench_report,
        &deterministic_report,
        &rollout_matrix_report,
        &issue_budget,
        &normalized_issue_budget_path,
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
            inject_sarif: backend.support_tier == SupportTier::A,
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
            inject_sarif: backend.support_tier == SupportTier::A,
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
        rollout_case("a", None, false, "render", None),
        rollout_case("a", Some("shadow"), false, "shadow", Some("shadow_mode")),
        rollout_case(
            "a",
            Some("passthrough"),
            false,
            "passthrough",
            Some("user_opt_out"),
        ),
        rollout_case(
            "a",
            Some("render"),
            true,
            "passthrough",
            Some("incompatible_sink"),
        ),
        rollout_case("b", None, false, "passthrough", Some("unsupported_tier")),
        rollout_case("b", Some("shadow"), false, "shadow", Some("shadow_mode")),
        rollout_case(
            "b",
            Some("render"),
            false,
            "passthrough",
            Some("unsupported_tier"),
        ),
        rollout_case("c", None, false, "passthrough", Some("unsupported_tier")),
    ]
}

fn rollout_case(
    support_tier: &str,
    requested_mode: Option<&str>,
    hard_conflict: bool,
    selected_mode: &str,
    fallback_reason: Option<&str>,
) -> RolloutMatrixCase {
    RolloutMatrixCase {
        support_tier: support_tier.to_string(),
        requested_mode: requested_mode.map(str::to_string),
        hard_conflict,
        selected_mode: selected_mode.to_string(),
        fallback_reason: fallback_reason.map(str::to_string),
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

fn build_rc_gate_checks(
    replay: &ReplayReport,
    bench: &BenchSmokeReport,
    deterministic: &DeterministicReplayReport,
    rollout: &RolloutMatrixReport,
    issue_budget: &IssueBudgetEvidence,
    issue_budget_path: &Path,
    fuzz: &FuzzEvidence,
    fuzz_path: &Path,
    ux: &UxSignoffEvidence,
    ux_path: &Path,
    allow_pending_manual_checks: bool,
) -> Vec<RcGateCheck> {
    let unexpected_fallback_rate = replay.metrics.unexpected_fallback_count as f64
        / replay.metrics.promoted_fixture_count.max(1) as f64;
    vec![
        RcGateCheck {
            id: "curated_corpus".to_string(),
            title: "Curated Corpus Pass 100%".to_string(),
            status: if replay.failures.is_empty() && replay.promoted_failed == 0 {
                GateStatus::Pass
            } else {
                GateStatus::Fail
            },
            summary: format!(
                "verified {}/{} promoted fixtures",
                replay.promoted_verified, replay.promoted_fixture_count
            ),
            blocker: replay.promoted_failed > 0,
            manual: false,
            evidence_path: Some(PathBuf::from("replay-report.json")),
        },
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
        manual_issue_budget_check(issue_budget, issue_budget_path, allow_pending_manual_checks),
        manual_fuzz_check(fuzz, fuzz_path, allow_pending_manual_checks),
        manual_ux_check(ux, ux_path, allow_pending_manual_checks),
    ]
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

fn manual_fuzz_check(
    fuzz: &FuzzEvidence,
    fuzz_path: &Path,
    allow_pending_manual_checks: bool,
) -> RcGateCheck {
    let status = match fuzz.status {
        ManualEvidenceStatus::Approved if fuzz.crash_count == 0 && fuzz.corpus_replay_passed => {
            GateStatus::Pass
        }
        ManualEvidenceStatus::Pending => GateStatus::Pending,
        _ => GateStatus::Fail,
    };
    RcGateCheck {
        id: "fuzz".to_string(),
        title: "Fuzz Crash 0".to_string(),
        status: status.clone(),
        summary: format!(
            "status={:?} crash_count={} corpus_replay_passed={}",
            fuzz.status, fuzz.crash_count, fuzz.corpus_replay_passed
        ),
        blocker: check_is_blocker(&status, true, allow_pending_manual_checks),
        manual: true,
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

fn load_fuzz_evidence(path: &Path) -> Result<FuzzEvidence, Box<dyn std::error::Error>> {
    if path.exists() {
        return Ok(serde_json::from_slice(&fs::read(path)?)?);
    }
    Ok(FuzzEvidence {
        schema_version: RC_GATE_SCHEMA_VERSION,
        release_candidate: "1.0.0-rc.N".to_string(),
        status: ManualEvidenceStatus::Pending,
        crash_count: 0,
        corpus_replay_passed: false,
        updated_at: "missing".to_string(),
        notes: vec![format!("missing manual evidence: {}", path.display())],
    })
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
    use crate::commands::corpus::{AcceptanceFixtureSummary, ReplayReport, acceptance_metrics_for};
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
            parse_time_ms,
            render_time_ms,
            verified: true,
        }
    }

    fn replay_report(fixtures: Vec<AcceptanceFixtureSummary>) -> ReplayReport {
        ReplayReport {
            family_counts: BTreeMap::new(),
            selected_family_counts: BTreeMap::new(),
            selected_fixture_count: fixtures.len(),
            promoted_fixture_count: fixtures.len(),
            promoted_verified: fixtures.len(),
            promoted_failed: 0,
            subset: "all".to_string(),
            metrics: acceptance_metrics_for(&fixtures),
            fixtures,
            failures: Vec::new(),
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
}
