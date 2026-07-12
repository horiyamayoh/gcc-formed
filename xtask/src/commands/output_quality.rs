use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

const SCHEMA_VERSION: u32 = 1;
const MINIMUM_TRIALS: usize = 360;
const MINIMUM_FAMILIES: usize = 120;
const MINIMUM_PER_CONDITION: usize = 120;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct AgentOutputQualityReport {
    pub(crate) schema_version: u32,
    pub(crate) verdict: String,
    pub(crate) candidate_sha: String,
    pub(crate) protocol_sha256: String,
    pub(crate) analysis_plan_sha256: String,
    pub(crate) model_agent_tool_manifest_sha256: String,
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
    if report.human_readable_contract_status != "pass" {
        blockers.push(format!(
            "human_readable_contract_status={:?} expected=pass",
            report.human_readable_contract_status
        ));
    }
    for (name, value) in [
        ("candidate_sha", report.candidate_sha.as_str()),
        ("protocol_sha256", report.protocol_sha256.as_str()),
        ("analysis_plan_sha256", report.analysis_plan_sha256.as_str()),
        (
            "model_agent_tool_manifest_sha256",
            report.model_agent_tool_manifest_sha256.as_str(),
        ),
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
