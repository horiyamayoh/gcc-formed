use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

const SCHEMA_VERSION: u32 = 1;
const MINIMUM_TRIALS: usize = 360;
const MINIMUM_FAMILIES: usize = 120;
const MINIMUM_PER_CONDITION: usize = 120;
const REQUIRED_MATRIX_CELLS: &[(&str, &str)] = &[
    ("gcc15", "dual_sink_structured"),
    ("gcc13_14", "native_text_capture"),
    ("gcc13_14", "single_sink_structured"),
    ("gcc9_12", "native_text_capture"),
    ("gcc9_12", "single_sink_structured"),
];

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct AgentOutputQualityReport {
    pub(crate) schema_version: u32,
    pub(crate) verdict: String,
    pub(crate) candidate_sha: String,
    pub(crate) protocol_sha256: String,
    pub(crate) analysis_plan_sha256: String,
    pub(crate) model_agent_tool_manifest_sha256: String,
    pub(crate) no_subagent_attestation_sha256: String,
    pub(crate) toolchain_sha256: String,
    pub(crate) corpus_manifest_sha256: String,
    pub(crate) started_trials: usize,
    pub(crate) valid_trials: usize,
    pub(crate) condition_counts: BTreeMap<String, usize>,
    pub(crate) semantic_family_count: usize,
    pub(crate) excluded_trials: usize,
    #[serde(default)]
    pub(crate) missing_scores: Vec<String>,
    pub(crate) invalid_final_schema_trials: usize,
    #[serde(default)]
    pub(crate) margin_failures: Vec<String>,
    pub(crate) improvement_requirement_passed: bool,
    pub(crate) fidelity_status: String,
    pub(crate) integrity_status: String,
    pub(crate) human_readable_contract_status: String,
    pub(crate) trial_artifact_merkle_root: String,
    pub(crate) condition_key_commitment: String,
    pub(crate) claim_boundary: String,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct AgentOutputQualityValidation {
    pub(crate) schema_version: u32,
    pub(crate) status: String,
    pub(crate) report_path: PathBuf,
    pub(crate) blockers: Vec<String>,
    pub(crate) report: AgentOutputQualityReport,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct OutputQualityMatrixFixture {
    fixture_id: String,
    version_band: String,
    processing_path: String,
    default_first_action_line: Option<usize>,
    ci_first_action_line: Option<usize>,
    displayed_block_count: usize,
    source_or_location_visible: bool,
    disclosure_visible: bool,
    group_identity_preserved: bool,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct OutputQualityMatrixReport {
    schema_version: u32,
    candidate_preset: String,
    pub(crate) status: String,
    required_cells: Vec<String>,
    covered_cells: Vec<String>,
    missing_cells: Vec<String>,
    failures: Vec<String>,
    fixtures: Vec<OutputQualityMatrixFixture>,
    claim_boundary: String,
}

pub(crate) fn run_output_quality_matrix(
    root: &Path,
    output: &Path,
) -> Result<OutputQualityMatrixReport, Box<dyn std::error::Error>> {
    use crate::commands::corpus::{render_request_for_fixture, replay_fixture_document};
    use diag_render::{
        RenderProfile, ResolvedPresentationPolicy, render, render_with_presentation_policy,
    };

    let fixtures = diag_testkit::discover(root)?;
    let mut covered = std::collections::BTreeSet::new();
    let mut failures = Vec::new();
    let mut records = Vec::new();
    for fixture in fixtures.iter().filter(|fixture| fixture.is_promoted()) {
        let band = fixture.expectations.version_band.as_str();
        let path = fixture.expectations.processing_path.as_str();
        if !REQUIRED_MATRIX_CELLS.contains(&(band, path)) {
            continue;
        }
        let replay = match replay_fixture_document(fixture) {
            Ok(replay) => replay,
            Err(error) => {
                failures.push(format!("{}: replay failed: {error}", fixture.fixture_id()));
                continue;
            }
        };
        let mut policy = ResolvedPresentationPolicy::subject_blocks_v2();
        policy.preset_id = "repair_units_hybrid_v1".to_string();
        let default_request =
            render_request_for_fixture(fixture, &replay.document, RenderProfile::Default);
        let ci_request = render_request_for_fixture(fixture, &replay.document, RenderProfile::Ci);
        let current = render(default_request.clone())?;
        let candidate = render_with_presentation_policy(default_request, &policy)?;
        let candidate_ci = render_with_presentation_policy(ci_request, &policy)?;
        let group_identity_preserved =
            current.displayed_group_refs == candidate.displayed_group_refs;
        let explicit_disclosure = candidate
            .text
            .contains("details: --formed-explain | raw: --formed-raw")
            && candidate_ci
                .text
                .contains("details: --formed-explain | raw: --formed-raw");
        let raw_already_visible = candidate.text == current.text;
        let disclosure_visible = explicit_disclosure || raw_already_visible;
        let source_or_location_visible = candidate.text.contains("-->")
            || candidate.text.lines().take(12).any(|line| {
                line.contains(".c:")
                    || line.contains(".cpp:")
                    || line.contains("symbol")
                    || line.contains("from")
                    || line.contains("library")
                    || line.contains("cannot find")
            });
        let default_first_action_line = first_action_line(&candidate.text);
        let ci_first_action_line = first_action_line(&candidate_ci.text);
        if !group_identity_preserved {
            failures.push(format!(
                "{}: candidate changed displayed RepairUnit identity",
                fixture.fixture_id()
            ));
        }
        if !disclosure_visible {
            failures.push(format!(
                "{}: candidate lacks one-step raw/explain disclosure",
                fixture.fixture_id()
            ));
        }
        if !source_or_location_visible {
            failures.push(format!(
                "{}: candidate lacks source/location/symbol anchor",
                fixture.fixture_id()
            ));
        }
        if default_first_action_line.is_none_or(|line| line > 12)
            || ci_first_action_line.is_none_or(|line| line > 12)
        {
            failures.push(format!(
                "{}: first actionable evidence is outside the 12-line qualification budget",
                fixture.fixture_id()
            ));
        }
        covered.insert(format!("{band}/{path}"));
        records.push(OutputQualityMatrixFixture {
            fixture_id: fixture.fixture_id().to_string(),
            version_band: band.to_string(),
            processing_path: path.to_string(),
            default_first_action_line,
            ci_first_action_line,
            displayed_block_count: candidate.displayed_group_refs.len(),
            source_or_location_visible,
            disclosure_visible,
            group_identity_preserved,
        });
    }
    let required_cells = REQUIRED_MATRIX_CELLS
        .iter()
        .map(|(band, path)| format!("{band}/{path}"))
        .collect::<Vec<_>>();
    let missing_cells = required_cells
        .iter()
        .filter(|cell| !covered.contains(*cell))
        .cloned()
        .collect::<Vec<_>>();
    let report = OutputQualityMatrixReport {
        schema_version: SCHEMA_VERSION,
        candidate_preset: "repair_units_hybrid_v1".to_string(),
        status: if missing_cells.is_empty() && failures.is_empty() {
            "pass".to_string()
        } else {
            "fail".to_string()
        },
        required_cells,
        covered_cells: covered.into_iter().collect(),
        missing_cells,
        failures,
        fixtures: records,
        claim_boundary: "deterministic candidate rendering over retained real-compiler matrix evidence; not a human behavioral study".to_string(),
    };
    if let Some(parent) = output.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(output, serde_json::to_vec_pretty(&report)?)?;
    Ok(report)
}

fn first_action_line(text: &str) -> Option<usize> {
    text.lines()
        .position(|line| {
            let line = line.to_ascii_lowercase();
            line.contains("error")
                || line.contains("warning")
                || line.contains("fatal")
                || line.contains("help:")
                || line.contains("undefined reference")
                || line.contains("not declared")
        })
        .map(|index| index + 1)
}

pub(crate) fn load_and_validate_agent_output_quality(
    report_path: &Path,
) -> Result<AgentOutputQualityValidation, Box<dyn std::error::Error>> {
    let bytes = fs::read(report_path).map_err(|error| {
        format!(
            "required single-agent output-quality report {} could not be read: {error}",
            report_path.display()
        )
    })?;
    let report: AgentOutputQualityReport = serde_json::from_slice(&bytes).map_err(|error| {
        format!(
            "required single-agent output-quality report {} is invalid JSON: {error}",
            report_path.display()
        )
    })?;
    let mut blockers = Vec::new();
    if report.schema_version != SCHEMA_VERSION {
        blockers.push(format!(
            "schema_version={} expected={SCHEMA_VERSION}",
            report.schema_version
        ));
    }
    if report.verdict != "pass" {
        blockers.push(format!(
            "qualification verdict is {:?}, not pass",
            report.verdict
        ));
    }
    if report.started_trials < MINIMUM_TRIALS {
        blockers.push(format!(
            "started_trials={} minimum={MINIMUM_TRIALS}",
            report.started_trials
        ));
    }
    if report.valid_trials < MINIMUM_TRIALS {
        blockers.push(format!(
            "valid_trials={} minimum={MINIMUM_TRIALS}",
            report.valid_trials
        ));
    }
    if report.semantic_family_count < MINIMUM_FAMILIES {
        blockers.push(format!(
            "semantic_family_count={} minimum={MINIMUM_FAMILIES}",
            report.semantic_family_count
        ));
    }
    for condition in ["native_gcc", "current_default", "candidate"] {
        let count = report.condition_counts.get(condition).copied().unwrap_or(0);
        if count < MINIMUM_PER_CONDITION {
            blockers.push(format!(
                "condition {condition} count={count} minimum={MINIMUM_PER_CONDITION}"
            ));
        }
    }
    if !report.missing_scores.is_empty() {
        blockers.push(format!(
            "{} trial scores are missing",
            report.missing_scores.len()
        ));
    }
    if report.invalid_final_schema_trials != 0 {
        blockers.push(format!(
            "invalid_final_schema_trials={} expected=0",
            report.invalid_final_schema_trials
        ));
    }
    if !report.margin_failures.is_empty() {
        blockers.push(format!(
            "{} preregistered margins failed",
            report.margin_failures.len()
        ));
    }
    if !report.improvement_requirement_passed {
        blockers.push("preregistered improvement requirement did not pass".to_string());
    }
    if report.fidelity_status != "pass" {
        blockers.push(format!(
            "fidelity_status={:?} expected=pass",
            report.fidelity_status
        ));
    }
    if report.integrity_status != "pass" {
        blockers.push(format!(
            "integrity_status={:?} expected=pass",
            report.integrity_status
        ));
    }
    if report.human_readable_contract_status != "pass" {
        blockers.push(format!(
            "human_readable_contract_status={:?} expected=pass",
            report.human_readable_contract_status
        ));
    }
    if !matches!(report.candidate_sha.len(), 40 | 64)
        || !report
            .candidate_sha
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit())
    {
        blockers.push("candidate_sha is not a 40- or 64-character hex commit id".to_string());
    }
    for (name, value) in [
        ("protocol_sha256", report.protocol_sha256.as_str()),
        ("analysis_plan_sha256", report.analysis_plan_sha256.as_str()),
        (
            "model_agent_tool_manifest_sha256",
            report.model_agent_tool_manifest_sha256.as_str(),
        ),
        (
            "no_subagent_attestation_sha256",
            report.no_subagent_attestation_sha256.as_str(),
        ),
        ("toolchain_sha256", report.toolchain_sha256.as_str()),
        (
            "corpus_manifest_sha256",
            report.corpus_manifest_sha256.as_str(),
        ),
        (
            "trial_artifact_merkle_root",
            report.trial_artifact_merkle_root.as_str(),
        ),
        (
            "condition_key_commitment",
            report.condition_key_commitment.as_str(),
        ),
    ] {
        if value.len() != 64 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            blockers.push(format!("{name} is not a 64-character hex digest"));
        }
    }
    if !report
        .claim_boundary
        .contains("no human behavioral-study claim")
    {
        blockers.push("claim boundary does not explicitly reject a human-study claim".to_string());
    }
    Ok(AgentOutputQualityValidation {
        schema_version: SCHEMA_VERSION,
        status: if blockers.is_empty() {
            "pass".to_string()
        } else {
            "fail".to_string()
        },
        report_path: report_path.to_path_buf(),
        blockers,
        report,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn passing_report() -> AgentOutputQualityReport {
        AgentOutputQualityReport {
            schema_version: 1,
            verdict: "pass".to_string(),
            candidate_sha: "a".repeat(64),
            protocol_sha256: "b".repeat(64),
            analysis_plan_sha256: "c".repeat(64),
            model_agent_tool_manifest_sha256: "d".repeat(64),
            no_subagent_attestation_sha256: "2".repeat(64),
            toolchain_sha256: "1".repeat(64),
            corpus_manifest_sha256: "e".repeat(64),
            started_trials: 360,
            valid_trials: 360,
            condition_counts: BTreeMap::from([
                ("native_gcc".to_string(), 120),
                ("current_default".to_string(), 120),
                ("candidate".to_string(), 120),
            ]),
            semantic_family_count: 120,
            excluded_trials: 0,
            missing_scores: Vec::new(),
            invalid_final_schema_trials: 0,
            margin_failures: Vec::new(),
            improvement_requirement_passed: true,
            fidelity_status: "pass".to_string(),
            integrity_status: "pass".to_string(),
            human_readable_contract_status: "pass".to_string(),
            trial_artifact_merkle_root: "f".repeat(64),
            condition_key_commitment: "0".repeat(64),
            claim_boundary: "coding-agent evidence; no human behavioral-study claim".to_string(),
        }
    }

    #[test]
    fn complete_passing_packet_is_accepted() {
        let directory = tempdir().unwrap();
        let path = directory.path().join("qualification-report.json");
        fs::write(&path, serde_json::to_vec(&passing_report()).unwrap()).unwrap();

        let validation = load_and_validate_agent_output_quality(&path).unwrap();

        assert_eq!(validation.status, "pass");
        assert!(validation.blockers.is_empty());
    }

    #[test]
    fn full_length_git_sha_is_accepted_as_candidate_identity() {
        let directory = tempdir().unwrap();
        let path = directory.path().join("qualification-report.json");
        let mut report = passing_report();
        report.candidate_sha = "a".repeat(40);
        fs::write(&path, serde_json::to_vec(&report).unwrap()).unwrap();

        let validation = load_and_validate_agent_output_quality(&path).unwrap();

        assert_eq!(validation.status, "pass");
    }

    #[test]
    fn inconclusive_or_underpowered_packet_is_rejected() {
        let directory = tempdir().unwrap();
        let path = directory.path().join("qualification-report.json");
        let mut report = passing_report();
        report.verdict = "inconclusive".to_string();
        report.valid_trials = 359;
        fs::write(&path, serde_json::to_vec(&report).unwrap()).unwrap();

        let validation = load_and_validate_agent_output_quality(&path).unwrap();

        assert_eq!(validation.status, "fail");
        assert!(
            validation
                .blockers
                .iter()
                .any(|item| item.contains("verdict"))
        );
        assert!(
            validation
                .blockers
                .iter()
                .any(|item| item.contains("valid_trials"))
        );
    }
}
