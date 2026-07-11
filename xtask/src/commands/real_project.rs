use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::fs;
use std::path::PathBuf;

#[derive(Debug)]
pub(crate) struct RealProjectVerifyOptions {
    pub(crate) root: PathBuf,
}

#[derive(Debug, Deserialize)]
struct Manifest {
    schema_version: u32,
    scenario_count: usize,
    projects: Vec<Project>,
}

#[derive(Debug, Deserialize)]
struct Project {
    project_id: String,
    build_system: String,
    language: String,
    phase: String,
    scenario_family: String,
    license: String,
    provenance: String,
    redaction: String,
    reviewer: String,
    repair_owner: String,
    network_required: bool,
    scenarios: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct Scenario {
    fixture_id: String,
    project_id: String,
    build_system: String,
    language: String,
    phase: String,
    scenario_family: String,
    multi_invocation: bool,
    invocation_boundary: String,
    license: String,
    provenance: String,
    redaction: String,
    reviewer: String,
    repair_owner: String,
    network_required: bool,
    promotion_status: String,
    unknown_unresolved_retention: String,
}

#[derive(Debug, Deserialize)]
struct InvocationManifest {
    parallel_capture: bool,
    attribution_precedes_repair_unit_inference: bool,
    invocations: Vec<Invocation>,
}

#[derive(Debug, Deserialize)]
struct Invocation {
    invocation_id: String,
    translation_unit: String,
    argv: Vec<String>,
    stderr_span_ref: String,
}

#[derive(Debug, Serialize)]
struct VerificationReport {
    schema_version: u32,
    project_shape_count: usize,
    scenario_count: usize,
    multi_invocation_scenario_count: usize,
    languages: BTreeSet<String>,
    build_systems: BTreeSet<String>,
    phases: BTreeSet<String>,
    license_audit_passed: bool,
    provenance_audit_passed: bool,
    redaction_audit_passed: bool,
    invocation_boundary_audit_passed: bool,
    network_required: bool,
    promoted_fixture_count: usize,
    oracle_artifact_count: usize,
    differential_coverage_report: String,
    raw_gcc_diagnostic_line_count: usize,
    current_default_subject_block_count: usize,
    repair_unit_visible_count: usize,
    exact_count_fixture_count: usize,
    fidelity_fact_coverage: f64,
    diagnostics_per_subject_block: f64,
    verdict: String,
}

pub(crate) fn verify_real_project_corpus(
    options: RealProjectVerifyOptions,
) -> Result<(), Box<dyn std::error::Error>> {
    let manifest: Manifest =
        serde_json::from_slice(&fs::read(options.root.join("manifest.json"))?)?;
    if manifest.schema_version != 1 {
        return Err("unsupported real-project manifest schema".into());
    }
    let mut languages = BTreeSet::new();
    let mut build_systems = BTreeSet::new();
    let mut phases = BTreeSet::new();
    let mut scenario_families = BTreeSet::new();
    let mut multi = 0usize;
    let mut promoted = 0usize;
    let mut oracle_artifacts = 0usize;
    let mut seen = BTreeSet::new();
    for project in &manifest.projects {
        require_metadata(project)?;
        for fixture_id in &project.scenarios {
            if !seen.insert(fixture_id.clone()) {
                return Err(format!("duplicate real-project fixture {fixture_id}").into());
            }
            let fixture = options.root.join(fixture_id);
            let scenario: Scenario =
                serde_json::from_slice(&fs::read(fixture.join("scenario.json"))?)?;
            validate_scenario(project, fixture_id, &scenario)?;
            let invocations: InvocationManifest =
                serde_json::from_slice(&fs::read(fixture.join("invocations.json"))?)?;
            validate_invocations(fixture_id, &scenario, &invocations)?;
            if !fixture.join("repair-oracle.toml").is_file()
                || !fixture.join("causal-map.json").is_file()
            {
                return Err(format!("{fixture_id} is missing canonical oracle artifacts").into());
            }
            oracle_artifacts += 1;
            multi += usize::from(scenario.multi_invocation);
            promoted += usize::from(scenario.promotion_status == "reviewed");
            languages.insert(scenario.language);
            build_systems.insert(scenario.build_system);
            phases.insert(scenario.phase);
            scenario_families.insert(scenario.scenario_family);
        }
    }
    if manifest.projects.len() < 8 || manifest.scenario_count < 40 || seen.len() < 40 {
        return Err("real-project corpus requires 8 project shapes and 40 scenarios".into());
    }
    if multi < 10 {
        return Err("real-project corpus requires 10 multi-invocation scenarios".into());
    }
    for required in ["c", "cpp"] {
        if !languages.contains(required) {
            return Err(format!("missing language {required}").into());
        }
    }
    for required in ["make", "cmake", "direct"] {
        if !build_systems.contains(required) {
            return Err(format!("missing build system {required}").into());
        }
    }
    for required in ["compile", "link"] {
        if !phases.contains(required) {
            return Err(format!("missing phase {required}").into());
        }
    }
    for required in [
        "generated_config_header",
        "repeated_across_translation_units",
        "parallel_independent_files",
        "macro_heavy_c",
        "template_heavy_cpp",
        "system_header_frontier",
        "missing_library_duplicate_symbol_link_order",
        "warning_as_error",
        "non_utf8_path_terminal_edge",
        "ccache_style_launcher",
    ] {
        if !scenario_families.contains(required) {
            return Err(format!("missing real-project scenario family {required}").into());
        }
    }
    let coverage_path = options.root.join("repair-unit-coverage.json");
    if !coverage_path.is_file() {
        return Err("missing differential repair-unit coverage report".into());
    }
    let coverage: serde_json::Value = serde_json::from_slice(&fs::read(&coverage_path)?)?;
    let coverage_fixtures = coverage
        .get("fixtures")
        .and_then(serde_json::Value::as_array)
        .ok_or("invalid differential repair-unit coverage report")?;
    let sum = |field: &str| {
        coverage_fixtures
            .iter()
            .filter_map(|fixture| fixture.get(field).and_then(serde_json::Value::as_u64))
            .sum::<u64>() as usize
    };
    let raw_lines = sum("raw_evidence_count");
    let default_blocks = sum("formed_visible_block_count");
    let visible_units = sum("formed_visible_repair_unit_count");
    let raw_numerator = sum("raw_fact_coverage_numerator");
    let raw_denominator = sum("raw_fact_coverage_denominator");
    let exact_count = coverage_fixtures
        .iter()
        .filter(|fixture| {
            fixture
                .get("baseline_classification")
                .and_then(serde_json::Value::as_str)
                == Some("exact")
        })
        .count();
    let report = VerificationReport {
        schema_version: 1,
        project_shape_count: manifest.projects.len(),
        scenario_count: seen.len(),
        multi_invocation_scenario_count: multi,
        languages,
        build_systems,
        phases,
        license_audit_passed: true,
        provenance_audit_passed: true,
        redaction_audit_passed: true,
        invocation_boundary_audit_passed: true,
        network_required: false,
        promoted_fixture_count: promoted,
        oracle_artifact_count: oracle_artifacts,
        differential_coverage_report: "repair-unit-coverage.json".into(),
        raw_gcc_diagnostic_line_count: raw_lines,
        current_default_subject_block_count: default_blocks,
        repair_unit_visible_count: visible_units,
        exact_count_fixture_count: exact_count,
        fidelity_fact_coverage: if raw_denominator == 0 {
            1.0
        } else {
            raw_numerator as f64 / raw_denominator as f64
        },
        diagnostics_per_subject_block: if default_blocks == 0 {
            0.0
        } else {
            raw_lines as f64 / default_blocks as f64
        },
        verdict: "pass".into(),
    };
    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(())
}

fn require_metadata(project: &Project) -> Result<(), Box<dyn std::error::Error>> {
    if project.project_id.is_empty()
        || project.build_system.is_empty()
        || project.language.is_empty()
        || project.phase.is_empty()
        || project.scenario_family.is_empty()
        || project.license.is_empty()
        || project.provenance.is_empty()
        || project.redaction.is_empty()
        || project.reviewer.is_empty()
        || project.repair_owner.is_empty()
        || project.network_required
    {
        return Err(format!("invalid project metadata for {}", project.project_id).into());
    }
    Ok(())
}

fn validate_scenario(
    project: &Project,
    fixture_id: &str,
    scenario: &Scenario,
) -> Result<(), Box<dyn std::error::Error>> {
    if scenario.fixture_id != fixture_id
        || scenario.project_id != project.project_id
        || scenario.build_system != project.build_system
        || scenario.language != project.language
        || scenario.phase != project.phase
        || scenario.scenario_family.is_empty()
        || scenario.license != project.license
        || scenario.provenance.is_empty()
        || scenario.redaction.is_empty()
        || scenario.reviewer.is_empty()
        || scenario.repair_owner.is_empty()
        || scenario.invocation_boundary.is_empty()
        || scenario.network_required
        || scenario.promotion_status != "reviewed"
        || scenario.unknown_unresolved_retention != "retain_with_raw_capture"
    {
        return Err(format!("invalid scenario metadata for {fixture_id}").into());
    }
    Ok(())
}

fn validate_invocations(
    fixture_id: &str,
    scenario: &Scenario,
    manifest: &InvocationManifest,
) -> Result<(), Box<dyn std::error::Error>> {
    let unique_ids = manifest
        .invocations
        .iter()
        .map(|invocation| invocation.invocation_id.as_str())
        .collect::<BTreeSet<_>>();
    let complete = manifest.invocations.iter().all(|invocation| {
        !invocation.translation_unit.is_empty()
            && !invocation.argv.is_empty()
            && !invocation.stderr_span_ref.is_empty()
    });
    if !manifest.attribution_precedes_repair_unit_inference
        || manifest.parallel_capture != scenario.multi_invocation
        || unique_ids.len() != manifest.invocations.len()
        || (scenario.multi_invocation && manifest.invocations.len() < 2)
        || !complete
    {
        return Err(format!("invalid invocation attribution for {fixture_id}").into());
    }
    Ok(())
}
