#[cfg(test)]
use crate::commands::corpus::confidence_label;
use crate::commands::corpus::{
    REPRESENTATIVE_FIXTURES, VerificationFailure, canonical_json_for_view_model,
    classify_snapshot_artifact_diff, collect_acceptance_fixture_summary, render_profile_from_name,
    render_request_for_fixture, replay_fixture_document, structured_artifact_spec_for_fixture,
    write_fixture_report_bundle,
};
use crate::util::fs::copy_dir_recursive;
use diag_core::{SnapshotKind, snapshot_json};
use diag_render::{build_view_model, render};
use diag_testkit::{Fixture, discover, validate_fixture};
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

const HUMAN_EVAL_SCHEMA_VERSION: u32 = 4;
const MINIMUM_TASK_STUDY_FIXTURE_COUNT: usize = 10;
const REQUIRED_TASK_STUDY_FAMILIES: &[&str] = &[
    "syntax",
    "macro_include",
    "template",
    "type",
    "overload",
    "linker",
];
const C_FIRST_TASK_CATEGORIES: &[&str] = &[
    "compile",
    "link",
    "include_path",
    "macro",
    "preprocessor",
    "honest_fallback",
];
const C_FIRST_TASK_FIXTURES: &[(&str, &[&str])] = &[
    ("c/syntax/case-11", &["compile"]),
    ("c/linker/case-11", &["link"]),
    ("c/macro_include/case-13", &["include_path", "macro"]),
    ("c/preprocessor_directive/case-01", &["preprocessor"]),
    ("c/macro_include/case-01", &["honest_fallback"]),
];

#[derive(Debug, Clone)]
struct CurrentFixtureVocabulary {
    version_band: String,
    support_level: String,
    processing_path: String,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct HumanEvalFixtureReport {
    pub(crate) fixture_id: String,
    pub(crate) family_key: String,
    pub(crate) language_key: String,
    pub(crate) title: Option<String>,
    pub(crate) version_band: String,
    pub(crate) support_level: String,
    pub(crate) processing_path: String,
    pub(crate) expected_mode: String,
    pub(crate) tags: Vec<String>,
    pub(crate) expert_review: bool,
    pub(crate) task_study: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(crate) c_first_categories: Vec<String>,
    pub(crate) used_fallback: bool,
    pub(crate) lead_confidence: String,
    pub(crate) rendered_first_action_line: Option<usize>,
    pub(crate) diagnostic_compression_ratio: Option<f64>,
    pub(crate) source_dir: String,
    pub(crate) invoke_path: String,
    pub(crate) expectations_path: String,
    pub(crate) meta_path: String,
    pub(crate) raw_gcc_path: String,
    pub(crate) gcc_formed_default_path: String,
    pub(crate) gcc_formed_ci_path: String,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct HumanEvalTaskStudyRow {
    pub(crate) participant_group: String,
    pub(crate) sequence_index: usize,
    pub(crate) fixture_id: String,
    pub(crate) family_key: String,
    pub(crate) title: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(crate) c_first_categories: Vec<String>,
    pub(crate) first_interface: String,
    pub(crate) second_interface: String,
    pub(crate) raw_gcc_path: String,
    pub(crate) gcc_formed_path: String,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct HumanEvalCFirstTaskReport {
    pub(crate) fixture_id: String,
    pub(crate) categories: Vec<String>,
    pub(crate) version_band: String,
    pub(crate) processing_path: String,
    pub(crate) raw_gcc_path: String,
    pub(crate) gcc_formed_path: String,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct HumanEvalKitReport {
    pub(crate) schema_version: u32,
    pub(crate) generated_at_unix_seconds: u64,
    pub(crate) root: PathBuf,
    pub(crate) report_dir: PathBuf,
    pub(crate) expert_review_fixture_count: usize,
    pub(crate) task_study_fixture_count: usize,
    pub(crate) family_counts: BTreeMap<String, usize>,
    pub(crate) covered_required_families: Vec<String>,
    pub(crate) missing_required_families: Vec<String>,
    pub(crate) c_first_task_fixture_count: usize,
    pub(crate) covered_c_first_categories: Vec<String>,
    pub(crate) missing_c_first_categories: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(crate) c_first_tasks: Vec<HumanEvalCFirstTaskReport>,
    pub(crate) fixtures: Vec<HumanEvalFixtureReport>,
    pub(crate) task_study_matrix: Vec<HumanEvalTaskStudyRow>,
}

pub(crate) fn run_human_eval_kit(
    root: &Path,
    report_dir: &Path,
) -> Result<HumanEvalKitReport, Box<dyn std::error::Error>> {
    fs::create_dir_all(report_dir)?;

    let fixtures = discover(root)?;
    for fixture in &fixtures {
        validate_fixture(fixture)?;
    }

    let selected = select_human_eval_fixtures(&fixtures);
    if selected.is_empty() {
        return Err("no representative promoted fixtures available for human evaluation".into());
    }

    let mut fixture_reports = Vec::new();
    let mut family_counts = BTreeMap::new();
    for fixture in selected {
        let report = build_fixture_report_bundle(fixture, report_dir)?;
        *family_counts.entry(report.family_key.clone()).or_insert(0) += 1;
        fixture_reports.push(report);
    }

    let task_study_matrix = build_task_study_matrix(&fixture_reports);
    let covered_required_families = covered_required_families(&fixture_reports);
    let missing_required_families = missing_required_families(&fixture_reports);
    let c_first_tasks = collect_c_first_tasks(&fixture_reports);
    let covered_c_first_categories = covered_c_first_categories(&fixture_reports);
    let missing_c_first_categories = missing_c_first_categories(&fixture_reports);

    let report = HumanEvalKitReport {
        schema_version: HUMAN_EVAL_SCHEMA_VERSION,
        generated_at_unix_seconds: unix_now_seconds(),
        root: root.to_path_buf(),
        report_dir: report_dir.to_path_buf(),
        expert_review_fixture_count: fixture_reports.len(),
        task_study_fixture_count: fixture_reports
            .iter()
            .filter(|fixture| fixture.task_study)
            .count(),
        family_counts,
        covered_required_families,
        missing_required_families,
        c_first_task_fixture_count: c_first_tasks.len(),
        covered_c_first_categories,
        missing_c_first_categories,
        c_first_tasks,
        fixtures: fixture_reports,
        task_study_matrix,
    };

    fs::write(
        report_dir.join("human-eval-report.json"),
        serde_json::to_vec_pretty(&report)?,
    )?;
    fs::write(report_dir.join("README.md"), build_bundle_readme(&report))?;
    fs::write(
        report_dir.join("expert-review-sheet.csv"),
        build_expert_review_sheet(&report),
    )?;
    fs::write(
        report_dir.join("task-study-sheet.csv"),
        build_task_study_sheet(&report),
    )?;
    fs::write(
        report_dir.join("counterbalance.csv"),
        build_counterbalance_csv(&report),
    )?;
    fs::write(
        report_dir.join("metrics-manual-eval.template.json"),
        serde_json::to_vec_pretty(&metrics_manual_template(report_dir))?,
    )?;
    fs::write(
        report_dir.join("ux-signoff.template.json"),
        serde_json::to_vec_pretty(&ux_signoff_template(report_dir))?,
    )?;
    fs::write(
        report_dir.join("bundle-files.json"),
        serde_json::to_vec_pretty(&serde_json::json!({
            "schema_version": HUMAN_EVAL_SCHEMA_VERSION,
            "bundle_files": [
                "README.md",
                "human-eval-report.json",
                "expert-review-sheet.csv",
                "task-study-sheet.csv",
                "counterbalance.csv",
                "metrics-manual-eval.template.json",
                "ux-signoff.template.json"
            ],
        }))?,
    )?;

    Ok(report)
}

pub(crate) fn human_eval_kit_is_complete(report: &HumanEvalKitReport) -> bool {
    report.task_study_fixture_count >= MINIMUM_TASK_STUDY_FIXTURE_COUNT
        && report.missing_required_families.is_empty()
        && report.missing_c_first_categories.is_empty()
}

fn select_human_eval_fixtures(fixtures: &[Fixture]) -> Vec<&Fixture> {
    let promoted_by_id = fixtures
        .iter()
        .filter(|fixture| fixture.is_promoted())
        .map(|fixture| (fixture.fixture_id(), fixture))
        .collect::<BTreeMap<_, _>>();
    let mut selected = Vec::new();
    let mut seen = BTreeSet::new();

    for fixture_id in REPRESENTATIVE_FIXTURES {
        if let Some(fixture) = promoted_by_id.get(fixture_id) {
            selected.push(*fixture);
            seen.insert((*fixture_id).to_string());
        }
    }

    let mut extras = fixtures
        .iter()
        .filter(|fixture| fixture.is_promoted())
        .filter(|fixture| fixture.meta.tags.iter().any(|tag| tag == "representative"))
        .filter(|fixture| !seen.contains(fixture.fixture_id()))
        .collect::<Vec<_>>();
    extras.sort_by(|left, right| left.fixture_id().cmp(right.fixture_id()));
    selected.extend(extras);
    selected
}

fn build_fixture_report_bundle(
    fixture: &Fixture,
    report_dir: &Path,
) -> Result<HumanEvalFixtureReport, Box<dyn std::error::Error>> {
    let vocabulary = current_fixture_vocabulary(fixture);
    let summary = collect_acceptance_fixture_summary(fixture, None)
        .map_err(|failure| verification_failure_message("collect summary", &failure))?;
    let artifacts = collect_human_eval_artifacts(fixture)?;
    let mut artifact_diffs = Vec::new();
    for (relative, contents) in &artifacts {
        let (diff, _) = classify_snapshot_artifact_diff(
            fixture,
            relative,
            &fixture.snapshot_root().join(relative),
            contents,
            false,
        )
        .map_err(|failure| verification_failure_message("classify artifact diff", &failure))?;
        artifact_diffs.push(diff);
    }

    write_fixture_report_bundle(report_dir, fixture, &artifacts, &artifact_diffs)
        .map_err(|error| format!("write fixture bundle {}: {error}", fixture.fixture_id()))?;
    copy_fixture_inputs(fixture, report_dir)?;

    let fixture_dir = report_dir.join("fixtures").join(fixture.fixture_id());
    fs::write(
        fixture_dir.join("review-context.json"),
        serde_json::to_vec_pretty(&serde_json::json!({
            "fixture_id": fixture.fixture_id(),
            "family_key": fixture.family_key(),
            "language_key": fixture.language_key(),
            "title": fixture.meta.title.clone(),
            "version_band": vocabulary.version_band.clone(),
            "support_level": vocabulary.support_level.clone(),
            "processing_path": vocabulary.processing_path.clone(),
            "expected_mode": fixture.expectations.expected_mode.clone(),
            "tags": fixture.meta.tags.clone(),
            "acceptance_summary": summary,
        }))?,
    )?;

    let family_key = fixture.family_key();
    let c_first_categories = c_first_categories_for_fixture(fixture.fixture_id());
    Ok(HumanEvalFixtureReport {
        fixture_id: fixture.fixture_id().to_string(),
        family_key: family_key.clone(),
        language_key: fixture.language_key(),
        title: fixture.meta.title.clone(),
        version_band: vocabulary.version_band,
        support_level: vocabulary.support_level,
        processing_path: vocabulary.processing_path,
        expected_mode: fixture.expectations.expected_mode.clone(),
        tags: fixture.meta.tags.clone(),
        expert_review: true,
        task_study: REQUIRED_TASK_STUDY_FAMILIES.contains(&family_key.as_str())
            || !c_first_categories.is_empty(),
        c_first_categories,
        used_fallback: summary.used_fallback,
        lead_confidence: summary.lead_confidence,
        rendered_first_action_line: summary.rendered_first_action_line,
        diagnostic_compression_ratio: summary.diagnostic_compression_ratio,
        source_dir: format!("fixtures/{}/input/src", fixture.fixture_id()),
        invoke_path: format!("fixtures/{}/input/invoke.yaml", fixture.fixture_id()),
        expectations_path: format!("fixtures/{}/input/expectations.yaml", fixture.fixture_id()),
        meta_path: format!("fixtures/{}/input/meta.yaml", fixture.fixture_id()),
        raw_gcc_path: format!("fixtures/{}/actual/stderr.raw", fixture.fixture_id()),
        gcc_formed_default_path: format!(
            "fixtures/{}/actual/render.default.txt",
            fixture.fixture_id()
        ),
        gcc_formed_ci_path: format!("fixtures/{}/actual/render.ci.txt", fixture.fixture_id()),
    })
}

fn current_fixture_vocabulary(fixture: &Fixture) -> CurrentFixtureVocabulary {
    CurrentFixtureVocabulary {
        version_band: fixture.invoke.version_band.clone(),
        support_level: fixture.invoke.support_level.clone(),
        processing_path: fixture.expectations.processing_path.clone(),
    }
}

fn collect_human_eval_artifacts(
    fixture: &Fixture,
) -> Result<BTreeMap<String, String>, Box<dyn std::error::Error>> {
    let replay = replay_fixture_document(fixture)?;
    replay
        .document
        .validate()
        .map_err(|error| error.errors.join("; "))?;

    let snapshot_root = fixture.snapshot_root();
    let mut artifacts = BTreeMap::new();
    for relative in copied_snapshot_artifact_names(fixture) {
        artifacts.insert(
            relative.clone(),
            fs::read_to_string(snapshot_root.join(&relative))?,
        );
    }
    artifacts.insert(
        "ir.facts.json".to_string(),
        snapshot_json(&replay.document, SnapshotKind::FactsOnly)?,
    );
    artifacts.insert(
        "ir.analysis.json".to_string(),
        snapshot_json(&replay.document, SnapshotKind::AnalysisIncluded)?,
    );

    for profile_name in selected_profile_names(fixture) {
        let profile = render_profile_from_name(profile_name)
            .ok_or_else(|| format!("unsupported render profile `{profile_name}`"))?;
        let request = render_request_for_fixture(fixture, &replay.document, profile);
        let view_model = build_view_model(&request);
        let render_result = render(request)?;
        artifacts.insert(
            format!("view.{profile_name}.json"),
            canonical_json_for_view_model(view_model.as_ref())?,
        );
        artifacts.insert(format!("render.{profile_name}.txt"), render_result.text);
    }

    Ok(artifacts)
}

fn copied_snapshot_artifact_names(fixture: &Fixture) -> Vec<String> {
    let mut names = vec!["stderr.raw".to_string()];
    if let Some(spec) = structured_artifact_spec_for_fixture(fixture) {
        names.push(spec.file_name.to_string());
    }
    names
}

fn selected_profile_names(fixture: &Fixture) -> Vec<&'static str> {
    let mut names = vec!["default", "ci"];
    for (name, _) in fixture.expectations.render.named_profiles() {
        if !names.contains(&name) {
            names.push(name);
        }
    }
    names
}

fn copy_fixture_inputs(
    fixture: &Fixture,
    report_dir: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let input_dir = report_dir
        .join("fixtures")
        .join(fixture.fixture_id())
        .join("input");
    copy_dir_recursive(&fixture.root.join("src"), &input_dir.join("src"))?;
    for relative in ["invoke.yaml", "expectations.yaml", "meta.yaml"] {
        fs::copy(fixture.root.join(relative), input_dir.join(relative))?;
    }
    Ok(())
}

fn build_task_study_matrix(fixtures: &[HumanEvalFixtureReport]) -> Vec<HumanEvalTaskStudyRow> {
    let task_fixtures = fixtures
        .iter()
        .filter(|fixture| fixture.task_study)
        .collect::<Vec<_>>();
    let mut rows = Vec::new();
    for (sequence_index, fixture) in task_fixtures.iter().enumerate() {
        let formed_first = sequence_index % 2 == 0;
        rows.push(task_study_row(
            "A",
            sequence_index + 1,
            fixture,
            formed_first,
        ));
        rows.push(task_study_row(
            "B",
            sequence_index + 1,
            fixture,
            !formed_first,
        ));
    }
    rows
}

fn task_study_row(
    participant_group: &str,
    sequence_index: usize,
    fixture: &HumanEvalFixtureReport,
    formed_first: bool,
) -> HumanEvalTaskStudyRow {
    HumanEvalTaskStudyRow {
        participant_group: participant_group.to_string(),
        sequence_index,
        fixture_id: fixture.fixture_id.clone(),
        family_key: fixture.family_key.clone(),
        title: fixture
            .title
            .clone()
            .unwrap_or_else(|| fixture.fixture_id.clone()),
        c_first_categories: fixture.c_first_categories.clone(),
        first_interface: if formed_first {
            "gcc_formed".to_string()
        } else {
            "raw_gcc".to_string()
        },
        second_interface: if formed_first {
            "raw_gcc".to_string()
        } else {
            "gcc_formed".to_string()
        },
        raw_gcc_path: fixture.raw_gcc_path.clone(),
        gcc_formed_path: fixture.gcc_formed_default_path.clone(),
    }
}

fn c_first_categories_for_fixture(fixture_id: &str) -> Vec<String> {
    C_FIRST_TASK_FIXTURES
        .iter()
        .find_map(|(candidate, categories)| {
            if *candidate == fixture_id {
                Some(
                    categories
                        .iter()
                        .map(|category| (*category).to_string())
                        .collect(),
                )
            } else {
                None
            }
        })
        .unwrap_or_default()
}

fn covered_required_families(fixtures: &[HumanEvalFixtureReport]) -> Vec<String> {
    REQUIRED_TASK_STUDY_FAMILIES
        .iter()
        .filter(|family| {
            fixtures
                .iter()
                .any(|fixture| fixture.task_study && fixture.family_key == **family)
        })
        .map(|family| (*family).to_string())
        .collect()
}

fn covered_c_first_categories(fixtures: &[HumanEvalFixtureReport]) -> Vec<String> {
    C_FIRST_TASK_CATEGORIES
        .iter()
        .filter(|category| {
            fixtures.iter().any(|fixture| {
                fixture
                    .c_first_categories
                    .iter()
                    .any(|item| item == **category)
            })
        })
        .map(|category| (*category).to_string())
        .collect()
}

fn missing_c_first_categories(fixtures: &[HumanEvalFixtureReport]) -> Vec<String> {
    C_FIRST_TASK_CATEGORIES
        .iter()
        .filter(|category| {
            !fixtures.iter().any(|fixture| {
                fixture
                    .c_first_categories
                    .iter()
                    .any(|item| item == **category)
            })
        })
        .map(|category| (*category).to_string())
        .collect()
}

fn collect_c_first_tasks(fixtures: &[HumanEvalFixtureReport]) -> Vec<HumanEvalCFirstTaskReport> {
    fixtures
        .iter()
        .filter(|fixture| !fixture.c_first_categories.is_empty())
        .map(|fixture| HumanEvalCFirstTaskReport {
            fixture_id: fixture.fixture_id.clone(),
            categories: fixture.c_first_categories.clone(),
            version_band: fixture.version_band.clone(),
            processing_path: fixture.processing_path.clone(),
            raw_gcc_path: fixture.raw_gcc_path.clone(),
            gcc_formed_path: fixture.gcc_formed_default_path.clone(),
        })
        .collect()
}

fn missing_required_families(fixtures: &[HumanEvalFixtureReport]) -> Vec<String> {
    REQUIRED_TASK_STUDY_FAMILIES
        .iter()
        .filter(|family| {
            !fixtures
                .iter()
                .any(|fixture| fixture.task_study && fixture.family_key == **family)
        })
        .map(|family| (*family).to_string())
        .collect()
}

fn build_bundle_readme(report: &HumanEvalKitReport) -> String {
    let mut text = String::new();
    let _ = writeln!(&mut text, "# Human Evaluation Kit");
    let _ = writeln!(&mut text);
    let _ = writeln!(
        &mut text,
        "This bundle is the repeatable RC review packet for expert fixture review and task-based UX study."
    );
    let _ = writeln!(&mut text);
    let _ = writeln!(
        &mut text,
        "- Expert review fixtures: `{}`",
        report.expert_review_fixture_count
    );
    let _ = writeln!(
        &mut text,
        "- Task study fixtures: `{}`",
        report.task_study_fixture_count
    );
    let _ = writeln!(
        &mut text,
        "- Covered required families: `{}`",
        report.covered_required_families.join(", ")
    );
    if report.missing_required_families.is_empty() {
        let _ = writeln!(&mut text, "- Missing required families: none");
    } else {
        let _ = writeln!(
            &mut text,
            "- Missing required families: `{}`",
            report.missing_required_families.join(", ")
        );
    }
    let _ = writeln!(
        &mut text,
        "- C-first task fixtures: `{}`",
        report.c_first_task_fixture_count
    );
    let _ = writeln!(
        &mut text,
        "- Covered C-first categories: `{}`",
        report.covered_c_first_categories.join(", ")
    );
    if report.missing_c_first_categories.is_empty() {
        let _ = writeln!(&mut text, "- Missing C-first categories: none");
    } else {
        let _ = writeln!(
            &mut text,
            "- Missing C-first categories: `{}`",
            report.missing_c_first_categories.join(", ")
        );
    }
    let _ = writeln!(&mut text);
    let _ = writeln!(&mut text, "## Procedure");
    let _ = writeln!(&mut text);
    let _ = writeln!(
        &mut text,
        "1. Use `expert-review-sheet.csv` for fixture-by-fixture expert review. Inspect `fixtures/<fixture>/actual/render.default.txt`, `render.ci.txt`, `stderr.raw`, the authoritative structured artifact when present, `ir.analysis.json`, and the copied source tree under `input/src/`."
    );
    let _ = writeln!(
        &mut text,
        "2. Use `counterbalance.csv` to assign participant group A or B. Then fill `task-study-sheet.csv` while comparing `raw_gcc_path` and `gcc_formed_path` for each fixture."
    );
    let _ = writeln!(
        &mut text,
        "   The C-first operator packet must cover `compile`, `link`, `include_path`, `macro`, `preprocessor`, and `honest_fallback` before RC sign-off."
    );
    let _ = writeln!(
        &mut text,
        "3. After the study, summarize TRC, TFAH, first-fix success delta, and high-confidence mislead rate into `metrics-manual-eval.template.json`, then copy the result into `eval/rc/metrics-manual-eval.json`."
    );
    let _ = writeln!(
        &mut text,
        "4. Record the expert review verdict and reviewer list into `ux-signoff.template.json`, then copy the result into `eval/rc/ux-signoff.json`."
    );
    let _ = writeln!(&mut text);
    let _ = writeln!(&mut text, "## Expert Review Checklist");
    let _ = writeln!(&mut text);
    let _ = writeln!(&mut text, "- lead group is close to the root cause");
    let _ = writeln!(
        &mut text,
        "- first action is visible within the first screenful"
    );
    let _ = writeln!(
        &mut text,
        "- omission notice is honest when information is partial"
    );
    let _ = writeln!(
        &mut text,
        "- template / macro / include / linker compression is appropriate"
    );
    let _ = writeln!(&mut text, "- confidence wording is appropriate");
    let _ = writeln!(&mut text, "- raw facts remain reachable");
    let _ = writeln!(&mut text, "- CI render stays path-first");
    let _ = writeln!(&mut text, "- noise is not worse than raw GCC");
    let _ = writeln!(&mut text);
    let _ = writeln!(&mut text, "## Aggregation Notes");
    let _ = writeln!(&mut text);
    let _ = writeln!(
        &mut text,
        "- `TRC improvement % = 100 * (median(raw_trc_seconds) - median(gcc_formed_trc_seconds)) / median(raw_trc_seconds)`"
    );
    let _ = writeln!(
        &mut text,
        "- `TFAH improvement % = 100 * (median(raw_ttfah_seconds) - median(gcc_formed_ttfah_seconds)) / median(raw_ttfah_seconds)`"
    );
    let _ = writeln!(
        &mut text,
        "- `first-fix success delta points = gcc_formed_success_rate - raw_gcc_success_rate`"
    );
    let _ = writeln!(
        &mut text,
        "- `high-confidence mislead rate = mislead_count / reviewed_high_confidence_cases`"
    );
    text
}

fn build_expert_review_sheet(report: &HumanEvalKitReport) -> String {
    let mut csv = String::new();
    csv.push_str(
        "fixture_id,family_key,title,render_default_path,render_ci_path,raw_gcc_path,lead_group_root_cause,first_action_first_screenful,omission_notice_honest,compression_reasonable,confidence_appropriate,raw_facts_accessible,ci_path_first,noise_not_worse_than_raw,overall_verdict,notes\n",
    );
    for fixture in &report.fixtures {
        csv.push_str(&csv_line(&[
            fixture.fixture_id.clone(),
            fixture.family_key.clone(),
            fixture.title.clone().unwrap_or_default(),
            fixture.gcc_formed_default_path.clone(),
            fixture.gcc_formed_ci_path.clone(),
            fixture.raw_gcc_path.clone(),
            String::new(),
            String::new(),
            String::new(),
            String::new(),
            String::new(),
            String::new(),
            String::new(),
            String::new(),
            String::new(),
            String::new(),
        ]));
    }
    csv
}

fn build_task_study_sheet(report: &HumanEvalKitReport) -> String {
    let mut csv = String::new();
    csv.push_str(
        "participant_id,participant_group,sequence_index,fixture_id,family_key,title,c_first_categories,first_interface,second_interface,raw_gcc_path,gcc_formed_path,raw_ttfah_seconds,gcc_formed_ttfah_seconds,raw_trc_seconds,gcc_formed_trc_seconds,raw_first_fix_success,gcc_formed_first_fix_success,high_confidence_case,mislead_observed,notes\n",
    );
    for row in &report.task_study_matrix {
        csv.push_str(&csv_line(&[
            String::new(),
            row.participant_group.clone(),
            row.sequence_index.to_string(),
            row.fixture_id.clone(),
            row.family_key.clone(),
            row.title.clone(),
            row.c_first_categories.join("|"),
            row.first_interface.clone(),
            row.second_interface.clone(),
            row.raw_gcc_path.clone(),
            row.gcc_formed_path.clone(),
            String::new(),
            String::new(),
            String::new(),
            String::new(),
            String::new(),
            String::new(),
            String::new(),
            String::new(),
            String::new(),
        ]));
    }
    csv
}

fn build_counterbalance_csv(report: &HumanEvalKitReport) -> String {
    let mut csv = String::new();
    csv.push_str(
        "participant_group,sequence_index,fixture_id,family_key,title,c_first_categories,first_interface,second_interface,raw_gcc_path,gcc_formed_path\n",
    );
    for row in &report.task_study_matrix {
        csv.push_str(&csv_line(&[
            row.participant_group.clone(),
            row.sequence_index.to_string(),
            row.fixture_id.clone(),
            row.family_key.clone(),
            row.title.clone(),
            row.c_first_categories.join("|"),
            row.first_interface.clone(),
            row.second_interface.clone(),
            row.raw_gcc_path.clone(),
            row.gcc_formed_path.clone(),
        ]));
    }
    csv
}

fn metrics_manual_template(report_dir: &Path) -> serde_json::Value {
    serde_json::json!({
        "schema_version": HUMAN_EVAL_SCHEMA_VERSION,
        "release_candidate": "1.0.0-rc.N",
        "status": "pending",
        "reviewed_fixture_count": 0,
        "high_confidence_mislead_rate": serde_json::Value::Null,
        "trc_improvement_percent": serde_json::Value::Null,
        "tfah_improvement_percent": serde_json::Value::Null,
        "first_fix_success_delta_points": serde_json::Value::Null,
        "updated_at": "fill-me",
        "reviewers": [],
        "notes": [
            format!(
                "Populate this file after filling `{}` and aggregating the paired raw GCC vs gcc-formed study results.",
                report_dir.join("task-study-sheet.csv").display()
            )
        ]
    })
}

fn ux_signoff_template(report_dir: &Path) -> serde_json::Value {
    serde_json::json!({
        "schema_version": HUMAN_EVAL_SCHEMA_VERSION,
        "release_candidate": "1.0.0-rc.N",
        "status": "pending",
        "updated_at": "fill-me",
        "reviewers": [],
        "notes": [
            format!(
                "Populate this file after reviewers complete `{}` and confirm the RC expert review verdict.",
                report_dir.join("expert-review-sheet.csv").display()
            )
        ]
    })
}

fn verification_failure_message(stage: &str, failure: &VerificationFailure) -> String {
    format!(
        "{stage} for {} failed at {}: {}",
        failure.fixture_id, failure.layer, failure.summary
    )
}

fn csv_line(fields: &[String]) -> String {
    let mut line = String::new();
    for (index, field) in fields.iter().enumerate() {
        if index > 0 {
            line.push(',');
        }
        line.push_str(&csv_escape(field));
    }
    line.push('\n');
    line
}

fn csv_escape(value: &str) -> String {
    if value.contains([',', '"', '\n']) {
        format!("\"{}\"", value.replace('"', "\"\""))
    } else {
        value.to_string()
    }
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

    fn sample_fixture(fixture_id: &str, family_key: &str, title: &str) -> HumanEvalFixtureReport {
        let c_first_categories = c_first_categories_for_fixture(fixture_id);
        HumanEvalFixtureReport {
            fixture_id: fixture_id.to_string(),
            family_key: family_key.to_string(),
            language_key: "c".to_string(),
            title: Some(title.to_string()),
            version_band: "gcc15_plus".to_string(),
            support_level: "preview".to_string(),
            processing_path: "dual_sink_structured".to_string(),
            expected_mode: "render".to_string(),
            tags: vec!["representative".to_string()],
            expert_review: true,
            task_study: REQUIRED_TASK_STUDY_FAMILIES.contains(&family_key)
                || !c_first_categories.is_empty(),
            c_first_categories,
            used_fallback: false,
            lead_confidence: confidence_label(diag_core::Confidence::High).to_string(),
            rendered_first_action_line: Some(2),
            diagnostic_compression_ratio: Some(2.0),
            source_dir: format!("fixtures/{fixture_id}/input/src"),
            invoke_path: format!("fixtures/{fixture_id}/input/invoke.yaml"),
            expectations_path: format!("fixtures/{fixture_id}/input/expectations.yaml"),
            meta_path: format!("fixtures/{fixture_id}/input/meta.yaml"),
            raw_gcc_path: format!("fixtures/{fixture_id}/actual/stderr.raw"),
            gcc_formed_default_path: format!("fixtures/{fixture_id}/actual/render.default.txt"),
            gcc_formed_ci_path: format!("fixtures/{fixture_id}/actual/render.ci.txt"),
        }
    }

    fn sample_input_fixture(
        version_band: &str,
        support_level: &str,
        processing_path: &str,
        tags: Vec<String>,
    ) -> Fixture {
        Fixture {
            root: PathBuf::from("corpus/c/syntax/case-01"),
            invoke: diag_testkit::FixtureInvoke {
                language: "c".to_string(),
                standard: None,
                target_compiler_family: "gcc".to_string(),
                version_band: version_band.to_string(),
                support_level: support_level.to_string(),
                major_version_selector: "15".to_string(),
                argv: vec!["-c".to_string()],
                cwd_policy: "fixture_root".to_string(),
                env_overrides: BTreeMap::new(),
                source_readability_expectation: "readable".to_string(),
                linker_involvement: false,
                expected_mode: "render".to_string(),
                canonical_path_policy: "relative_to_cwd".to_string(),
            },
            expectations: diag_testkit::FixtureExpectations {
                schema_version: 1,
                fixture_id: "c/syntax/case-01".to_string(),
                version_band: version_band.to_string(),
                processing_path: processing_path.to_string(),
                support_level: support_level.to_string(),
                expected_mode: "render".to_string(),
                family: Some("syntax".to_string()),
                semantic: None,
                render: diag_testkit::RenderExpectations::default(),
                cascade: diag_testkit::CascadeExpectations::default(),
                integrity: diag_testkit::IntegrityExpectations::default(),
                performance: diag_testkit::PerformanceExpectations::default(),
            },
            meta: diag_testkit::FixtureMeta {
                corpus_id: None,
                title: None,
                tags,
                ownership: Some("curated".to_string()),
                provenance: Some("manual".to_string()),
                reviewer: Some("codex".to_string()),
                redaction_class: None,
                owner_team: None,
                last_reviewed: None,
                reviewers: Vec::new(),
                promotion_status: None,
                known_version_drift_notes: Vec::new(),
            },
        }
    }

    #[test]
    fn current_fixture_vocabulary_uses_current_labels() {
        let fixture = sample_input_fixture(
            "gcc13_14",
            "experimental",
            "single_sink_structured",
            vec![
                "representative".to_string(),
                "processing_path:single_sink_structured".to_string(),
            ],
        );

        let vocabulary = current_fixture_vocabulary(&fixture);

        assert_eq!(vocabulary.version_band, "gcc13_14");
        assert_eq!(vocabulary.support_level, "experimental");
        assert_eq!(vocabulary.processing_path, "single_sink_structured");
    }

    #[test]
    fn current_fixture_vocabulary_uses_expectations_processing_path() {
        let fixture = sample_input_fixture(
            "gcc9_12",
            "experimental",
            "native_text_capture",
            vec!["representative".to_string()],
        );

        let vocabulary = current_fixture_vocabulary(&fixture);

        assert_eq!(vocabulary.version_band, "gcc9_12");
        assert_eq!(vocabulary.support_level, "experimental");
        assert_eq!(vocabulary.processing_path, "native_text_capture");
    }

    #[test]
    fn human_eval_report_serializes_current_fixture_vocabulary_only() {
        let report = sample_fixture("c/syntax/case-01", "syntax", "syntax");
        let value = serde_json::to_value(&report).unwrap();

        assert_eq!(value["version_band"], "gcc15_plus");
        assert_eq!(value["support_level"], "preview");
        assert_eq!(value["processing_path"], "dual_sink_structured");
        assert!(value.get("support_tier").is_none());
    }

    #[test]
    fn copied_snapshot_artifact_names_omit_structured_capture_for_native_text() {
        let fixture = sample_input_fixture(
            "gcc13_14",
            "experimental",
            "native_text_capture",
            vec!["representative".to_string()],
        );

        assert_eq!(
            copied_snapshot_artifact_names(&fixture),
            vec!["stderr.raw".to_string()]
        );
    }

    #[test]
    fn copied_snapshot_artifact_names_use_json_for_band_c_single_sink() {
        let fixture = sample_input_fixture(
            "gcc9_12",
            "experimental",
            "single_sink_structured",
            vec![
                "representative".to_string(),
                "processing_path:single_sink_structured".to_string(),
            ],
        );

        assert_eq!(
            copied_snapshot_artifact_names(&fixture),
            vec!["stderr.raw".to_string(), "diagnostics.json".to_string()]
        );
    }

    #[test]
    fn task_study_matrix_counterbalances_interfaces() {
        let fixtures = vec![
            sample_fixture("c/syntax/case-01", "syntax", "syntax"),
            sample_fixture("c/macro_include/case-01", "macro_include", "macro"),
        ];

        let rows = build_task_study_matrix(&fixtures);

        assert_eq!(rows.len(), 4);
        assert_eq!(rows[0].participant_group, "A");
        assert_eq!(rows[0].first_interface, "gcc_formed");
        assert_eq!(rows[1].participant_group, "B");
        assert_eq!(rows[1].first_interface, "raw_gcc");
        assert_eq!(rows[2].sequence_index, 2);
        assert_eq!(rows[2].first_interface, "raw_gcc");
        assert_eq!(rows[3].first_interface, "gcc_formed");
    }

    #[test]
    fn c_first_categories_cover_expected_fixture_map() {
        assert_eq!(
            c_first_categories_for_fixture("c/macro_include/case-13"),
            vec!["include_path".to_string(), "macro".to_string()]
        );
        assert_eq!(
            c_first_categories_for_fixture("c/macro_include/case-01"),
            vec!["honest_fallback".to_string()]
        );
        assert_eq!(
            c_first_categories_for_fixture("c/preprocessor_directive/case-01"),
            vec!["preprocessor".to_string()]
        );
        assert!(c_first_categories_for_fixture("c/syntax/case-01").is_empty());
    }

    #[test]
    fn c_first_task_study_fixtures_are_marked_for_human_eval() {
        let fixtures = [
            sample_fixture("c/syntax/case-11", "syntax", "compile"),
            sample_fixture("c/linker/case-11", "linker", "link"),
            sample_fixture("c/macro_include/case-13", "macro_include", "macro"),
            sample_fixture(
                "c/preprocessor_directive/case-01",
                "preprocessor_directive",
                "preprocessor",
            ),
            sample_fixture("c/macro_include/case-01", "macro_include", "fallback"),
        ];

        let c_first_task_study_ids = fixtures
            .iter()
            .filter(|fixture| fixture.task_study)
            .map(|fixture| fixture.fixture_id.as_str())
            .collect::<Vec<_>>();

        assert_eq!(c_first_task_study_ids.len(), 5);
        assert!(c_first_task_study_ids.contains(&"c/syntax/case-11"));
        assert!(c_first_task_study_ids.contains(&"c/linker/case-11"));
        assert!(c_first_task_study_ids.contains(&"c/macro_include/case-13"));
        assert!(c_first_task_study_ids.contains(&"c/macro_include/case-01"));
        assert!(c_first_task_study_ids.contains(&"c/preprocessor_directive/case-01"));
    }

    #[test]
    fn completeness_requires_required_families_and_minimum_case_count() {
        let mut fixtures = Vec::new();
        for family in REQUIRED_TASK_STUDY_FAMILIES {
            fixtures.push(sample_fixture(
                &format!("fixture/{family}/01"),
                family,
                family,
            ));
        }
        fixtures.extend([
            sample_fixture("fixture/extra/01", "syntax", "extra 1"),
            sample_fixture("fixture/extra/02", "template", "extra 2"),
            sample_fixture("fixture/extra/03", "linker", "extra 3"),
            sample_fixture("fixture/extra/04", "macro_include", "extra 4"),
        ]);
        let report = HumanEvalKitReport {
            schema_version: HUMAN_EVAL_SCHEMA_VERSION,
            generated_at_unix_seconds: 0,
            root: PathBuf::from("corpus"),
            report_dir: PathBuf::from("target/human-eval"),
            expert_review_fixture_count: fixtures.len(),
            task_study_fixture_count: fixtures.len(),
            family_counts: BTreeMap::new(),
            covered_required_families: covered_required_families(&fixtures),
            missing_required_families: missing_required_families(&fixtures),
            c_first_task_fixture_count: 5,
            covered_c_first_categories: C_FIRST_TASK_CATEGORIES
                .iter()
                .map(|category| (*category).to_string())
                .collect(),
            missing_c_first_categories: Vec::new(),
            c_first_tasks: Vec::new(),
            fixtures,
            task_study_matrix: Vec::new(),
        };

        assert!(human_eval_kit_is_complete(&report));
    }
}
