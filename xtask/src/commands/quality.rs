use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug)]
pub(crate) struct QualityReportOptions {
    pub(crate) root: PathBuf,
    pub(crate) format: String,
}

#[derive(Debug, Deserialize)]
struct CoverageInput {
    fixtures: Vec<CoverageFixture>,
}

#[derive(Debug, Deserialize)]
struct CoverageFixture {
    fixture_id: String,
    language: Option<String>,
    diagnostic_shape: Option<String>,
    oracle_repair_unit_count: usize,
    formed_visible_block_count: Option<usize>,
    #[serde(default)]
    formed_visible_repair_unit_count: Option<usize>,
    #[serde(default)]
    formed_total_repair_unit_count: Option<usize>,
    #[serde(default)]
    hidden_independent_evidence_count: Option<usize>,
    #[serde(default)]
    orphan_hidden_evidence_count: Option<usize>,
    #[serde(default)]
    raw_fact_coverage_numerator: Option<usize>,
    #[serde(default)]
    raw_fact_coverage_denominator: Option<usize>,
}

#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub(crate) struct QualityReport {
    schema_version: u32,
    records: Vec<QualityMetricRecord>,
    totals: QualityTotals,
    blockers: Vec<String>,
    runtime_uses_oracle_recompile: bool,
}

#[derive(Debug, Serialize, Deserialize, PartialEq)]
struct QualityMetricRecord {
    fixture_id: String,
    language: String,
    diagnostic_shape: String,
    oracle_repair_unit_count: usize,
    visible_repair_unit_count: Option<usize>,
    displayed_block_count: Option<usize>,
    count_drift: Option<isize>,
    false_split_count: usize,
    false_merge_count: usize,
    observable_unit_recall: Option<f64>,
    visible_unit_precision: Option<f64>,
    hidden_independent_evidence_count: usize,
    orphan_hidden_evidence_count: usize,
    raw_fact_coverage: Option<f64>,
    grounded_action_precision: Option<f64>,
    native_source_emphasis_parity: Option<bool>,
    legacy_partition_diff_available: bool,
}

#[derive(Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
struct QualityTotals {
    fixture_count: usize,
    exact_count_fixture_count: usize,
    false_split_count: usize,
    false_merge_count: usize,
    displayed_count_drift_count: usize,
    hidden_independent_evidence_count: usize,
    orphan_hidden_evidence_count: usize,
    silent_fact_loss_count: usize,
}

pub(crate) fn run_quality_report(
    options: QualityReportOptions,
) -> Result<(), Box<dyn std::error::Error>> {
    if options.format != "json" {
        return Err("quality-report currently supports --format json".into());
    }
    let report = build_quality_report(&options.root)?;
    println!("{}", serde_json::to_string_pretty(&report)?);
    if !report.blockers.is_empty() {
        return Err(format!("quality blockers: {}", report.blockers.join("; ")).into());
    }
    Ok(())
}

fn build_quality_report(root: &Path) -> Result<QualityReport, Box<dyn std::error::Error>> {
    let input: CoverageInput =
        serde_json::from_slice(&fs::read(root.join("repair-unit-coverage.json"))?)?;
    let mut totals = QualityTotals::default();
    let mut blockers = Vec::new();
    let mut records = Vec::new();
    for fixture in input.fixtures {
        let displayed = fixture.formed_visible_block_count;
        let visible = fixture.formed_visible_repair_unit_count;
        let false_split = displayed
            .map(|count| count.saturating_sub(fixture.oracle_repair_unit_count))
            .unwrap_or(0);
        let false_merge = displayed
            .map(|count| fixture.oracle_repair_unit_count.saturating_sub(count))
            .unwrap_or(0);
        let drift =
            displayed.map(|count| count as isize - fixture.oracle_repair_unit_count as isize);
        let overlap = displayed
            .map(|count| count.min(fixture.oracle_repair_unit_count))
            .unwrap_or(0);
        let recall = (fixture.oracle_repair_unit_count > 0 && displayed.is_some())
            .then(|| overlap as f64 / fixture.oracle_repair_unit_count as f64);
        let precision = displayed
            .filter(|count| *count > 0)
            .map(|count| overlap as f64 / count as f64);
        totals.fixture_count += 1;
        totals.false_split_count += false_split;
        totals.false_merge_count += false_merge;
        if drift == Some(0) {
            totals.exact_count_fixture_count += 1;
        } else {
            totals.displayed_count_drift_count += 1;
            blockers.push(format!("{} count drift {:?}", fixture.fixture_id, drift));
        }
        if visible != displayed {
            blockers.push(format!(
                "{} displayed/visible mismatch {:?}/{:?}",
                fixture.fixture_id, displayed, visible
            ));
        }
        let hidden_independent = fixture.hidden_independent_evidence_count.unwrap_or(0);
        let orphan_hidden = fixture.orphan_hidden_evidence_count.unwrap_or(0);
        if fixture.hidden_independent_evidence_count.is_none()
            || fixture.orphan_hidden_evidence_count.is_none()
        {
            blockers.push(format!(
                "{} missing visibility evidence",
                fixture.fixture_id
            ));
        }
        if hidden_independent > 0 {
            blockers.push(format!(
                "{} hidden independent evidence {}",
                fixture.fixture_id, hidden_independent
            ));
        }
        if orphan_hidden > 0 {
            blockers.push(format!(
                "{} orphan hidden evidence {}",
                fixture.fixture_id, orphan_hidden
            ));
        }
        let raw_fact_coverage = fixture
            .raw_fact_coverage_denominator
            .and_then(|denominator| {
                fixture.raw_fact_coverage_numerator.map(|numerator| {
                    if denominator == 0 {
                        1.0
                    } else {
                        numerator as f64 / denominator as f64
                    }
                })
            });
        if raw_fact_coverage != Some(1.0) {
            blockers.push(format!(
                "{} raw fact coverage {:?}",
                fixture.fixture_id, raw_fact_coverage
            ));
            totals.silent_fact_loss_count += 1;
        }
        totals.hidden_independent_evidence_count += hidden_independent;
        totals.orphan_hidden_evidence_count += orphan_hidden;
        records.push(QualityMetricRecord {
            fixture_id: fixture.fixture_id,
            language: fixture.language.unwrap_or_else(|| "unknown".into()),
            diagnostic_shape: fixture.diagnostic_shape.unwrap_or_else(|| "unknown".into()),
            oracle_repair_unit_count: fixture.oracle_repair_unit_count,
            visible_repair_unit_count: visible,
            displayed_block_count: displayed,
            count_drift: drift,
            false_split_count: false_split,
            false_merge_count: false_merge,
            observable_unit_recall: recall,
            visible_unit_precision: precision,
            hidden_independent_evidence_count: hidden_independent,
            orphan_hidden_evidence_count: orphan_hidden,
            raw_fact_coverage,
            grounded_action_precision: None,
            native_source_emphasis_parity: None,
            legacy_partition_diff_available: fixture.formed_total_repair_unit_count.is_some(),
        });
    }
    Ok(QualityReport {
        schema_version: 1,
        records,
        totals,
        blockers,
        runtime_uses_oracle_recompile: false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn distinguishes_false_split_and_false_merge() {
        let temp = tempfile::tempdir().unwrap();
        fs::write(
            temp.path().join("repair-unit-coverage.json"),
            r#"{"fixtures":[
              {"fixture_id":"split","language":"c","diagnostic_shape":"x","oracle_repair_unit_count":1,"formed_visible_block_count":2,"formed_visible_repair_unit_count":2,"formed_total_repair_unit_count":2,"hidden_independent_evidence_count":0,"orphan_hidden_evidence_count":0,"raw_fact_coverage_numerator":2,"raw_fact_coverage_denominator":2},
              {"fixture_id":"merge","language":"cpp","diagnostic_shape":"y","oracle_repair_unit_count":3,"formed_visible_block_count":2,"formed_visible_repair_unit_count":2,"formed_total_repair_unit_count":2,"hidden_independent_evidence_count":0,"orphan_hidden_evidence_count":0,"raw_fact_coverage_numerator":2,"raw_fact_coverage_denominator":2}
            ]}"#,
        )
        .unwrap();
        let report = build_quality_report(temp.path()).unwrap();
        assert_eq!(report.totals.false_split_count, 1);
        assert_eq!(report.totals.false_merge_count, 1);
        assert_eq!(report.blockers.len(), 2);
    }
}
