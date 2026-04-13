use super::*;
use diag_backend_probe::support_level_for_version_band;
use diag_core::{DocumentCompleteness, FallbackReason, Origin, Phase, ProvenanceSource};
use diag_public_export::{PublicExportContext, PublicExportStatus, export_from_document};
use diag_render::{RenderProfile, build_view_model, render};
use diag_testkit::{discover, normalize_snapshot_contents};
use serde_yaml::Value as YamlValue;
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;
use std::process::Command;
use std::sync::{Mutex, OnceLock};

fn release_test_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

fn write_file(path: &Path, contents: &[u8]) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(path, contents).unwrap();
}

#[cfg(unix)]
fn make_executable(path: &Path) {
    use std::os::unix::fs::PermissionsExt;

    let mut permissions = fs::metadata(path).unwrap().permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(path, permissions).unwrap();
}

#[cfg(not(unix))]
fn make_executable(_path: &Path) {}

fn run_command(root: &Path, binary: &str, args: &[&str]) {
    let output = Command::new(binary)
        .current_dir(root)
        .args(args)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{binary} {} failed: {}",
        args.join(" "),
        String::from_utf8_lossy(&output.stderr)
    );
}

fn corpus_fixture(fixture_id: &str) -> diag_testkit::Fixture {
    let repo_root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("xtask crate should live under the repo root");
    discover(&repo_root.join("corpus"))
        .unwrap()
        .into_iter()
        .find(|fixture| fixture.fixture_id() == fixture_id)
        .unwrap_or_else(|| panic!("fixture `{fixture_id}` not found"))
}

fn fixture_with_snapshot(
    fixture_id: &str,
    version_band: &str,
    processing_path: &str,
    major_version_selector: &str,
) -> diag_testkit::Fixture {
    let mut fixture = corpus_fixture(fixture_id);
    fixture.invoke.version_band = version_band.to_string();
    fixture.invoke.support_level = "in_scope".to_string();
    fixture.invoke.major_version_selector = major_version_selector.to_string();
    fixture.expectations.version_band = version_band.to_string();
    fixture.expectations.processing_path = processing_path.to_string();
    fixture.expectations.support_level = "in_scope".to_string();
    fixture
}

fn public_export_for_fixture(
    fixture: &diag_testkit::Fixture,
    replay: &ReplayOutcomeAndDocument,
) -> diag_public_export::PublicDiagnosticExport {
    export_from_document(
        &replay.document,
        &PublicExportContext::from_document(
            &replay.document,
            fixture_support_band(fixture),
            fixture_processing_path(fixture),
            support_level_for_version_band(fixture_support_band(fixture)),
            replay.source_authority,
            replay.fallback_grade,
            replay.fallback_reason,
        ),
    )
}

fn meta_yaml_for_fixture(fixture: &diag_testkit::Fixture) -> YamlValue {
    let raw = fs::read_to_string(fixture.root.join("meta.yaml")).unwrap();
    serde_yaml::from_str::<YamlValue>(&raw).unwrap()
}

fn matrix_applicability_for_fixture(fixture: &diag_testkit::Fixture) -> YamlValue {
    meta_yaml_for_fixture(fixture)
        .get("matrix_applicability")
        .cloned()
        .expect("expected matrix_applicability block")
}

fn older_band_applicability_for_fixture(fixture: &diag_testkit::Fixture) -> YamlValue {
    meta_yaml_for_fixture(fixture)
        .get("older_band_applicability")
        .cloned()
        .expect("expected older_band_applicability block")
}

fn older_band_applicability_cell<'a>(
    applicability: &'a YamlValue,
    version_band: &str,
    processing_path: &str,
) -> &'a YamlValue {
    applicability
        .get(version_band)
        .and_then(|band| band.get(processing_path))
        .unwrap_or_else(|| {
            panic!("expected older_band_applicability.{version_band}.{processing_path} entry")
        })
}

fn yaml_string_sequence(node: Option<&YamlValue>) -> Vec<String> {
    node.and_then(YamlValue::as_sequence)
        .map(|values| {
            values
                .iter()
                .filter_map(YamlValue::as_str)
                .map(str::to_string)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default()
}

fn assert_representative_fixture_matches_band_and_path(
    fixture_id: &str,
    expected_band: diag_backend_probe::VersionBand,
    expected_path: diag_backend_probe::ProcessingPath,
) {
    let fixture = corpus_fixture(fixture_id);
    let meta = meta_yaml_for_fixture(&fixture);
    let tags = yaml_string_sequence(meta.get("tags"));

    assert_eq!(
        fixture_support_band(&fixture),
        expected_band,
        "{fixture_id} should match the expected representative band",
    );
    assert_eq!(
        fixture_processing_path(&fixture),
        expected_path,
        "{fixture_id} should match the expected representative processing path",
    );
    assert!(
        tags.iter().any(|tag| tag == "representative"),
        "{fixture_id} should remain tagged as representative",
    );
    assert!(
        meta.get("matrix_applicability").is_some(),
        "{fixture_id} should keep matrix_applicability metadata",
    );
}

fn assert_fixture_does_not_claim_older_band_representative_cells(
    fixture_id: &str,
    meta: &YamlValue,
) {
    let tags = meta
        .get("tags")
        .and_then(YamlValue::as_sequence)
        .map(|tags| {
            tags.iter()
                .filter_map(YamlValue::as_str)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    for forbidden_tag in [
        "band:gcc13_14",
        "band:gcc9_12",
        "processing_path:native_text_capture",
        "processing_path:single_sink_structured",
    ] {
        assert!(
            !tags.contains(&forbidden_tag),
            "{fixture_id} should not claim older-band representative tag {forbidden_tag}",
        );
    }

    if let Some(matrix) = meta.get("matrix_applicability") {
        assert_eq!(
            matrix.get("version_band").and_then(YamlValue::as_str),
            Some("gcc15"),
            "{fixture_id} should only keep representative matrix_applicability for GCC15",
        );
        assert_eq!(
            matrix.get("processing_path").and_then(YamlValue::as_str),
            Some("dual_sink_structured"),
            "{fixture_id} should only keep representative matrix_applicability for GCC15 dual_sink_structured",
        );
    }
}

fn assert_emitted_family_replay_contract(
    fixture: &diag_testkit::Fixture,
    expected_family: &str,
    expect_residual_only_passthrough: bool,
) {
    let fixture_id = fixture.fixture_id().to_string();
    let semantic = fixture.expectations.semantic.as_ref().unwrap();
    let replay = replay_fixture_document(fixture).unwrap();
    let request = render_request_for_fixture(fixture, &replay.document, RenderProfile::Default);
    let render_result = render(request).unwrap();
    let lead_node =
        lead_node_for_document(&replay.document, &render_result.displayed_group_refs).unwrap();

    assert_eq!(
        lead_node
            .analysis
            .as_ref()
            .and_then(|analysis| analysis.family.as_deref()),
        Some(expected_family),
        "{fixture_id} should keep {expected_family} as the lead family",
    );

    if semantic.first_action_required {
        assert!(
            lead_node
                .analysis
                .as_ref()
                .and_then(|analysis| analysis.first_action_hint.as_deref())
                .is_some_and(|hint| !hint.trim().is_empty()),
            "{fixture_id} should keep a lead first_action_hint",
        );
    }

    if let Some(max_line) = fixture
        .expectations
        .render
        .default
        .as_ref()
        .and_then(|expectations| expectations.first_action_max_line)
    {
        let line = first_help_line(&render_result.text).expect("expected help line");
        assert!(
            line <= max_line,
            "{fixture_id} should keep help within line {max_line}, got {line}",
        );
    }

    if fixture
        .expectations
        .render
        .default
        .as_ref()
        .and_then(|expectations| expectations.partial_notice_required)
        == Some(true)
    {
        assert!(
            contains_partial_notice(&render_result.text),
            "{fixture_id} should keep the partial notice visible",
        );
    }

    if fixture
        .expectations
        .render
        .default
        .as_ref()
        .and_then(|expectations| expectations.raw_diagnostics_hint_required)
        == Some(true)
    {
        assert!(
            contains_raw_diagnostics_hint(&render_result.text),
            "{fixture_id} should keep the raw diagnostics hint visible",
        );
    }

    if fixture
        .expectations
        .render
        .default
        .as_ref()
        .and_then(|expectations| expectations.raw_sub_block_required)
        == Some(true)
    {
        assert!(
            contains_raw_sub_block(&render_result.text),
            "{fixture_id} should keep the raw diagnostics sub-block visible",
        );
    }

    if fixture
        .expectations
        .render
        .default
        .as_ref()
        .and_then(|expectations| expectations.low_confidence_notice_required)
        == Some(true)
    {
        assert!(
            render_result.text.contains("confidence is limited"),
            "{fixture_id} should keep the low-confidence notice visible",
        );
    }

    let export = public_export_for_fixture(fixture, &replay);
    assert_eq!(export.status, PublicExportStatus::Available);
    assert_eq!(
        export.execution.version_band,
        fixture.expectations.version_band
    );
    assert_eq!(
        export.execution.processing_path,
        fixture.expectations.processing_path
    );
    assert!(
        export
            .execution
            .allowed_processing_paths
            .contains(&fixture.expectations.processing_path),
        "{fixture_id} should list its processing path in allowed_processing_paths",
    );

    if fixture.expectations.version_band != "gcc15" {
        assert!(
            export
                .execution
                .allowed_processing_paths
                .contains(&"passthrough".to_string()),
            "{fixture_id} should keep passthrough in allowed_processing_paths",
        );
    }

    let diagnostics = &export.result.as_ref().unwrap().diagnostics;
    assert!(
        !diagnostics.is_empty(),
        "{fixture_id} should export at least one diagnostic",
    );
    let matching_diagnostics = diagnostics
        .iter()
        .filter(|diagnostic| {
            diagnostic
                .family
                .as_deref()
                .is_some_and(|family| family == expected_family)
        })
        .collect::<Vec<_>>();
    assert!(
        !matching_diagnostics.is_empty(),
        "{fixture_id} should export at least one diagnostic tagged as {expected_family}",
    );
    assert!(
        matching_diagnostics.iter().any(|diagnostic| {
            diagnostic
                .headline
                .as_deref()
                .is_some_and(|headline| !headline.trim().is_empty())
        }),
        "{fixture_id} should export a non-empty headline for {expected_family}",
    );

    if semantic.first_action_required {
        assert!(
            matching_diagnostics.iter().any(|diagnostic| {
                diagnostic
                    .first_action
                    .as_deref()
                    .is_some_and(|action| !action.trim().is_empty())
            }),
            "{fixture_id} should export a non-empty first_action for {expected_family}",
        );
    }

    if semantic.raw_provenance_required {
        assert!(
            matching_diagnostics
                .iter()
                .any(|diagnostic| !diagnostic.provenance_capture_refs.is_empty()),
            "{fixture_id} should keep provenance_capture_refs on exported diagnostics for {expected_family}",
        );
    }

    if expect_residual_only_passthrough {
        assert_eq!(
            export.execution.document_completeness.as_deref(),
            Some("passthrough"),
            "{fixture_id} should preserve passthrough document completeness for the nearest emitted proof",
        );
        assert_eq!(
            export.execution.fallback_reason.as_deref(),
            Some("residual_only"),
            "{fixture_id} should record residual_only for the nearest emitted proof",
        );
        assert!(
            render_result.text.contains("failed to read compiled module"),
            "{fixture_id} should preserve the native module-import diagnostic text",
        );
    } else {
        assert_ne!(
            export.execution.fallback_reason.as_deref(),
            Some("residual_only"),
            "{fixture_id} should not regress to residual_only fallback",
        );
    }
}

fn acceptance_summary(
    fixture_id: &str,
    expected_family: Option<&str>,
    actual_family: &str,
) -> AcceptanceFixtureSummary {
    AcceptanceFixtureSummary {
        fixture_id: fixture_id.to_string(),
        family_key: "syntax".to_string(),
        title: None,
        support_band: "gcc15".to_string(),
        processing_path: "dual_sink_structured".to_string(),
        fallback_contract: "bounded_render".to_string(),
        expected_family: expected_family.map(str::to_string),
        actual_family: actual_family.to_string(),
        family_match: expected_family
            .map(|expected| expected == actual_family)
            .unwrap_or(false),
        used_fallback: false,
        fallback_reason: None,
        fallback_forbidden: false,
        unexpected_fallback: false,
        primary_location_path: Some("src/main.c".to_string()),
        primary_location_user_owned_required: false,
        primary_location_user_owned: false,
        missing_required_primary_location: false,
        first_action_required: false,
        first_action_present: false,
        missing_required_first_action: false,
        headline_rewritten: true,
        lead_confidence: "high".to_string(),
        high_confidence: true,
        rendered_first_action_line: Some(2),
        omission_notice_present: false,
        partial_notice_present: false,
        raw_diagnostics_hint_present: true,
        raw_sub_block_present: false,
        low_confidence_notice_present: false,
        within_first_screenful_budget: true,
        first_action_within_budget: None,
        native_parity_dimensions: Vec::new(),
        raw_line_count: 14,
        rendered_line_count: 7,
        diagnostic_compression_ratio: Some(2.0),
        parse_time_ms: 12,
        render_time_ms: 7,
        cascade_analysis_present: true,
        cascade_independent_episode_count: 1,
        cascade_independent_root_count: 1,
        cascade_dependent_follow_on_count: 0,
        cascade_duplicate_count: 0,
        cascade_uncertain_count: 0,
        default_summary_only_group_count: 0,
        default_hidden_group_count: 0,
        default_suppressed_group_count: 0,
        anti_collision: false,
        anti_collision_scenarios: Vec::new(),
        anti_collision_independent_root_total_count: 0,
        anti_collision_independent_root_recalled_count: 0,
        anti_collision_false_hidden_suppression_count: 0,
        anti_collision_hidden_independent_root_refs: Vec::new(),
        anti_collision_hidden_visibility_protected_refs: Vec::new(),
        verified: true,
    }
}

fn fake_wrapper_script(version: &str) -> String {
    format!(
        "#!/bin/sh\nif [ \"$1\" = \"--formed-version\" ]; then\n  printf '%s\\n' \"{version}\"\nelif [ \"$1\" = \"--formed-self-check\" ]; then\n  printf '%s\\n' '{{\"binary\":\"ok\"}}'\nelse\n  printf '%s\\n' \"packaged-{version}\"\nfi\n"
    )
}

fn test_signing_private_key_hex() -> &'static str {
    "000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f"
}

fn write_signing_private_key(path: &Path) {
    write_file(
        path,
        format!("{}\n", test_signing_private_key_hex()).as_bytes(),
    );
}

fn rewrite_packaged_fixture_version(
    package: &PackageOutput,
    signing_private_key: &Path,
    version: &str,
) {
    let mut control_manifest = read_build_manifest(&package.manifest_path).unwrap();
    control_manifest.product_version = version.to_string();
    write_file(
        &package.manifest_path,
        serde_json::to_vec_pretty(&control_manifest)
            .unwrap()
            .as_slice(),
    );

    let build_info = fs::read_to_string(&package.build_info_path).unwrap();
    let rewritten_build_info = build_info
        .lines()
        .map(|line| {
            if line.starts_with("version: ") {
                format!("version: {version}")
            } else {
                line.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
        + "\n";
    write_file(&package.build_info_path, rewritten_build_info.as_bytes());

    let primary_archive = find_primary_archive(&package.control_dir).unwrap();
    let staging = tempfile::tempdir().unwrap();
    extract_tar_archive(&primary_archive, staging.path()).unwrap();
    let extracted_root = extracted_payload_root(staging.path(), &primary_archive).unwrap();
    let staged_manifest_path = extracted_root.join("manifest.json");
    let mut staged_manifest = read_build_manifest(&staged_manifest_path).unwrap();
    staged_manifest.product_version = version.to_string();
    let staged_build_info_path = extracted_root.join("build-info.txt");
    let staged_build_info = fs::read_to_string(&staged_build_info_path).unwrap();
    let rewritten_staged_build_info = staged_build_info
        .lines()
        .map(|line| {
            if line.starts_with("version: ") {
                format!("version: {version}")
            } else {
                line.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
        + "\n";
    write_file(
        &staged_build_info_path,
        rewritten_staged_build_info.as_bytes(),
    );
    staged_manifest.checksums = payload_checksums(&extracted_root).unwrap();
    write_file(
        &staged_manifest_path,
        serde_json::to_vec_pretty(&staged_manifest)
            .unwrap()
            .as_slice(),
    );
    let root_name = extracted_root
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap()
        .to_string();
    create_tar_archive(staging.path(), &root_name, &primary_archive).unwrap();

    let shasums = render_sha256sums(&[
        &package.primary_archive,
        &package.debug_archive,
        &package.source_archive,
        &package.manifest_path,
        &package.build_info_path,
    ])
    .unwrap();
    write_file(&package.shasums_path, shasums.as_bytes());
    if let Some(signature_path) = &package.shasums_signature_path {
        let _ =
            write_detached_signature(&package.shasums_path, signature_path, signing_private_key)
                .unwrap();
    }
}

fn test_signing_public_key_sha256() -> String {
    let sandbox = tempfile::tempdir().unwrap();
    let path = sandbox.path().join("release-signing.key");
    write_signing_private_key(&path);
    signing_public_key_sha256(&read_signing_key(&path).unwrap().verifying_key())
}

fn current_release_fixture_version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

fn current_release_fixture_version_name() -> String {
    format!("v{}", current_release_fixture_version())
}

#[test]
fn ci_gate_cli_parses_nightly_lane_and_report_dir() {
    let cli = Cli::try_parse_from([
        "xtask",
        "ci-gate",
        "--workflow",
        "nightly",
        "--matrix-lane",
        "gcc15",
        "--report-dir",
        "target/local-gates/nightly-smoke",
    ])
    .unwrap();

    match cli.command {
        Commands::CiGate {
            workflow,
            report_dir,
            matrix_lane,
        } => {
            assert_eq!(workflow, CiWorkflow::Nightly);
            assert_eq!(
                report_dir,
                Some(Path::new("target/local-gates/nightly-smoke").to_path_buf())
            );
            assert_eq!(matrix_lane, Some(CiMatrixLane::Gcc15));
        }
        command => panic!("unexpected command: {command:?}"),
    }
}

#[test]
fn ci_gate_command_builder_rejects_matrix_lane_for_pr() {
    let error = build_ci_gate_command(CiWorkflow::Pr, None, Some(CiMatrixLane::Gcc15)).unwrap_err();
    assert!(error.to_string().contains("`nightly`"));
}

#[test]
fn ci_gate_command_builder_points_to_local_runner() {
    let command = build_ci_gate_command(
        CiWorkflow::Nightly,
        Some(&Path::new("target/local-gates/nightly").to_path_buf()),
        Some(CiMatrixLane::Gcc12),
    )
    .unwrap();
    assert_eq!(command.program, "python3");
    assert!(command.args[0].ends_with("ci/run_local_gate.py"));
    assert_eq!(
        command.args[1..],
        [
            "--workflow",
            "nightly",
            "--report-dir",
            "target/local-gates/nightly",
            "--matrix-lane",
            "gcc12",
        ]
    );
}

fn init_release_repo(version: &str) -> (tempfile::TempDir, PathBuf, PathBuf) {
    let sandbox = tempfile::tempdir().unwrap();
    let repo_root = sandbox.path().join("repo");
    let binary_root = sandbox.path().join("binary");
    fs::create_dir_all(&repo_root).unwrap();
    fs::create_dir_all(&binary_root).unwrap();

    write_file(&repo_root.join(".gitignore"), b"/dist\n");
    write_file(&repo_root.join("README.md"), b"# gcc-formed\n");
    write_file(
        &repo_root.join("docs/releases/RELEASE-NOTES.md"),
        b"# Release Notes\n\n- Initial release packaging smoke fixture.\n",
    );
    write_file(&repo_root.join("LICENSE"), b"Apache-2.0\n");
    write_file(&repo_root.join("NOTICE"), b"gcc-formed notice\n");
    write_file(&repo_root.join("Cargo.lock"), b"version = 3\n");
    write_file(&repo_root.join("src/main.rs"), b"fn main() {}\n");

    let binary_path = binary_root.join("gcc-formed");
    write_file(&binary_path, fake_wrapper_script(version).as_bytes());
    make_executable(&binary_path);

    run_command(&repo_root, "git", &["init", "-q", "-b", "main"]);
    run_command(
        &repo_root,
        "git",
        &["config", "user.email", "ci@example.com"],
    );
    run_command(&repo_root, "git", &["config", "user.name", "CI"]);
    run_command(&repo_root, "git", &["add", "."]);
    run_command(&repo_root, "git", &["commit", "-q", "-m", "initial"]);

    (sandbox, repo_root, binary_path)
}

fn init_minimal_cargo_project() -> (tempfile::TempDir, PathBuf) {
    let sandbox = tempfile::tempdir().unwrap();
    let root = sandbox.path().join("mini");
    fs::create_dir_all(root.join(".cargo")).unwrap();
    fs::create_dir_all(root.join("src")).unwrap();
    write_file(
        &root.join(".cargo/config.toml"),
        b"[build]\ntarget-dir = \"target\"\n",
    );
    write_file(
        &root.join("Cargo.toml"),
        b"[package]\nname = \"mini\"\nversion = \"0.1.0\"\nedition = \"2024\"\n\n[dependencies]\n",
    );
    write_file(&root.join("src/main.rs"), b"fn main() {}\n");
    run_command(&root, "cargo", &["generate-lockfile", "--offline"]);
    (sandbox, root)
}

#[test]
fn normalizes_sarif_snapshots_before_compare() {
    let expected = r#"{
  "version":"2.1.0",
  "runs":[
{
  "results":[
    {
      "level":"error",
      "ruleId":"error",
      "message":{"text":"link failed for ‘/tmp/helper.o’ and ‘/tmp/main.o’"},
      "locations":[
        {
          "physicalLocation":{
            "artifactLocation":{"uri":"src/main.c"},
            "region":{"startLine":2,"startColumn":25}
          }
        }
      ]
    }
  ]
}
  ]
}"#;
    let actual = r#"{
  "$schema": "https://docs.oasis-open.org/sarif/sarif/v2.1.0/errata01/os/schemas/sarif-schema-2.1.0.json",
  "runs": [
{
  "artifacts": [
    {
      "location": {
        "uri": "src/main.c"
      }
    }
  ],
  "results": [
    {
      "level": "error",
      "locations": [
        {
          "id": 0,
          "physicalLocation": {
            "artifactLocation": {
              "uri": "src/main.c",
              "uriBaseId": "%SRCROOT%"
            },
            "region": {
              "startLine": 2,
              "startColumn": 25
            }
          }
        }
      ],
      "message": {
        "text": "link failed for '/tmp/cc123456.o' and '/tmp/cc654321.o'"
      }
    }
  ]
}
  ],
  "version": "2.1.0"
}"#;

    let normalized_expected =
        normalize_snapshot_contents(Path::new("diagnostics.sarif"), expected).unwrap();
    let normalized_actual =
        normalize_snapshot_contents(Path::new("diagnostics.sarif"), actual).unwrap();

    assert_eq!(normalized_expected, normalized_actual);
}

#[test]
fn normalizes_ir_snapshots_before_compare() {
    let expected = r#"{
  "captures": [
{
  "id": "stderr.raw",
  "inline_text": "/usr/bin/ld: /tmp/helper.o: in function `duplicate':\nhelper.c:(.text+0x0): multiple definition of `duplicate'; /tmp/main.o:main.c:(.text+0x0): first defined here\ncollect2: error: ld returned 1 exit status\n",
  "kind": "compiler_stderr_text",
  "media_type": "text/plain",
  "size_bytes": 205,
  "storage": "inline"
},
{
  "external_ref": "<capture:diagnostics.sarif>",
  "id": "diagnostics.sarif",
  "kind": "gcc_sarif",
  "media_type": "application/sarif+json",
  "size_bytes": 44,
  "storage": "external_ref"
}
  ],
  "diagnostics": [
{
  "analysis": {
    "family": "linker.multiple_definition"
  },
  "fingerprints": {
    "family": "expected-family",
    "raw": "expected-raw",
    "structural": "expected-structural"
  },
  "id": "residual-1",
  "message": {
    "raw_text": "helper.c:(.text+0x0): multiple definition of ‘duplicate’; /tmp/main.o:main.c:(.text+0x0): first defined here"
  },
  "node_completeness": "partial",
  "origin": "linker",
  "phase": "link",
  "provenance": {
    "capture_refs": [
      "stderr.raw"
    ],
    "source": "residual_text"
  },
  "semantic_role": "root",
  "severity": "error"
}
  ],
  "document_completeness": "partial",
  "document_id": "<document>",
  "fingerprints": {
"family": "expected-document-family",
"raw": "expected-document-raw",
"structural": "expected-document-structural"
  },
  "producer": {
"name": "gcc-formed",
"version": "<normalized>"
  },
  "run": {
"exit_status": 1,
"invocation_id": "<invocation>",
"primary_tool": {
  "name": "gcc",
  "vendor": "GNU"
}
  },
  "schema_version": "1.0.0-alpha.1"
}"#;
    let actual = r#"{
  "captures": [
{
  "id": "stderr.raw",
  "inline_text": "/usr/bin/ld: /tmp/cc123456.o: in function `duplicate':\nhelper.c:(.text+0x0): multiple definition of `duplicate'; /tmp/cc654321.o:main.c:(.text+0x0): first defined here\ncollect2: error: ld returned 1 exit status\n",
  "kind": "compiler_stderr_text",
  "media_type": "text/plain",
  "size_bytes": 211,
  "storage": "inline"
},
{
  "external_ref": "<capture:diagnostics.sarif>",
  "id": "diagnostics.sarif",
  "kind": "gcc_sarif",
  "media_type": "application/sarif+json",
  "size_bytes": 987,
  "storage": "external_ref"
}
  ],
  "diagnostics": [
{
  "analysis": {
    "family": "linker.multiple_definition"
  },
  "fingerprints": {
    "family": "actual-family",
    "raw": "actual-raw",
    "structural": "actual-structural"
  },
  "id": "residual-1",
  "message": {
    "raw_text": "helper.c:(.text+0x0): multiple definition of 'duplicate'; /tmp/cc654321.o:main.c:(.text+0x0): first defined here"
  },
  "node_completeness": "partial",
  "origin": "linker",
  "phase": "link",
  "provenance": {
    "capture_refs": [
      "stderr.raw"
    ],
    "source": "residual_text"
  },
  "semantic_role": "root",
  "severity": "error"
}
  ],
  "document_completeness": "partial",
  "document_id": "<document>",
  "fingerprints": {
"family": "actual-document-family",
"raw": "actual-document-raw",
"structural": "actual-document-structural"
  },
  "producer": {
"name": "gcc-formed",
"version": "<normalized>"
  },
  "run": {
"exit_status": 1,
"invocation_id": "<invocation>",
"primary_tool": {
  "name": "gcc",
  "vendor": "GNU"
}
  },
  "schema_version": "1.0.0-alpha.1"
}"#;

    let normalized_expected =
        normalize_snapshot_contents(Path::new("ir.analysis.json"), expected).unwrap();
    let normalized_actual =
        normalize_snapshot_contents(Path::new("ir.analysis.json"), actual).unwrap();

    assert_eq!(normalized_expected, normalized_actual);
}

#[test]
fn normalizes_ir_location_span_drift_before_compare() {
    let expected = r#"{
  "captures": [],
  "diagnostics": [
{
  "analysis": {
    "confidence": "high",
    "family": "macro_include",
    "first_action_hint": "inspect the user-owned include edge or macro invocation that triggers the error",
    "headline": "error surfaced through macro/include context"
  },
  "context_chains": [
    {
      "frames": [
        {
          "column": 25,
          "label": "src/main.c:2:25: note: in expansion of macro `CALL_BAD'",
          "line": 2,
          "path": "src/main.c"
        }
      ],
      "kind": "macro_expansion"
    }
  ],
  "fingerprints": {
    "family": "expected-family",
    "raw": "expected-raw",
    "structural": "expected-structural"
  },
  "id": "sarif-0-0",
  "locations": [
    {
      "column": 25,
      "line": 2,
      "ownership": "user",
      "path": "src/main.c"
    }
  ],
  "message": {
    "raw_text": "`missing_symbol' undeclared"
  },
  "node_completeness": "complete",
  "origin": "gcc",
  "phase": "semantic",
  "provenance": {
    "capture_refs": [
      "diagnostics.sarif"
    ],
    "source": "compiler"
  },
  "semantic_role": "root",
  "severity": "error"
}

  ],
  "document_completeness": "complete",
  "document_id": "<document>",
  "fingerprints": {
"family": "expected-document-family",
"raw": "expected-document-raw",
"structural": "expected-document-structural"
  },
  "producer": {
"name": "gcc-formed",
"rulepack_version": "phase1",
"version": "<normalized>"
  },
  "run": {
"argv_redacted": [
  "gcc",
  "src/main.c"
],
"cwd_display": "<cwd>",
"exit_status": 1,
"invocation_id": "<invocation>",
"invoked_as": "gcc-formed",
"language_mode": "c",
"primary_tool": {
  "name": "gcc",
  "vendor": "GNU"
},
"target_triple": "x86_64-unknown-linux-gnu",
"wrapper_mode": "terminal"
  },
  "schema_version": "1.0.0-alpha.1"
}"#;
    let actual = r#"{
  "captures": [],
  "diagnostics": [
{
  "analysis": {
    "confidence": "high",
    "family": "macro_include",
    "first_action_hint": "inspect the user-owned include edge or macro invocation that triggers the error",
    "headline": "error surfaced through macro/include context"
  },
  "context_chains": [
    {
      "frames": [
        {
          "column": 41,
          "label": "src/main.c:5:41: note: in expansion of macro ‘CALL_BAD’",
          "line": 5,
          "path": "src/main.c"
        }
      ],
      "kind": "macro_expansion"
    }
  ],
  "fingerprints": {
    "family": "actual-family",
    "raw": "actual-raw",
    "structural": "actual-structural"
  },
  "id": "sarif-0-0",
  "locations": [
    {
      "column": 41,
      "end_column": 42,
      "end_line": 5,
      "line": 5,
      "ownership": "user",
      "path": "src/main.c"
    }
  ],
  "message": {
    "raw_text": "‘missing_symbol’ undeclared"
  },
  "node_completeness": "complete",
  "origin": "gcc",
  "phase": "semantic",
  "provenance": {
    "capture_refs": [
      "diagnostics.sarif"
    ],
    "source": "compiler"
  },
  "semantic_role": "root",
  "severity": "error"
}
  ],
  "document_completeness": "complete",
  "document_id": "<document>",
  "fingerprints": {
"family": "actual-document-family",
"raw": "actual-document-raw",
"structural": "actual-document-structural"
  },
  "producer": {
"name": "gcc-formed",
"rulepack_version": "phase1",
"version": "<normalized>"
  },
  "run": {
"argv_redacted": [
  "gcc",
  "src/main.c"
],
"cwd_display": "<cwd>",
"exit_status": 1,
"invocation_id": "<invocation>",
"invoked_as": "gcc-formed",
"language_mode": "c",
"primary_tool": {
  "name": "gcc",
  "vendor": "GNU"
},
"target_triple": "x86_64-unknown-linux-gnu",
"wrapper_mode": "terminal"
  },
  "schema_version": "1.0.0-alpha.1"
}"#;

    let normalized_expected =
        normalize_snapshot_contents(Path::new("ir.analysis.json"), expected).unwrap();
    let normalized_actual =
        normalize_snapshot_contents(Path::new("ir.analysis.json"), actual).unwrap();

    assert_eq!(normalized_expected, normalized_actual);
}

#[test]
fn replay_fixture_keeps_collect2_as_driver_summary_without_changing_lead_family() {
    let fixture = corpus_fixture("c/linker/case-12");
    let replay = replay_fixture_document(&fixture).unwrap();
    let request = render_request_for_fixture(&fixture, &replay.document, RenderProfile::Default);
    let render_result = render(request).unwrap();
    let lead_node =
        lead_node_for_document(&replay.document, &render_result.displayed_group_refs).unwrap();

    let collect2_node = replay
        .document
        .diagnostics
        .iter()
        .find(|node| {
            node.message
                .raw_text
                .contains("collect2: error: ld returned 1 exit status")
        })
        .unwrap();
    assert_eq!(collect2_node.origin, Origin::Driver);
    assert_eq!(collect2_node.phase, Phase::Link);

    let undefined_reference_node = replay
        .document
        .diagnostics
        .iter()
        .find(|node| {
            node.analysis
                .as_ref()
                .and_then(|analysis| analysis.family.as_deref())
                == Some("linker.undefined_reference")
        })
        .unwrap();
    assert_eq!(undefined_reference_node.origin, Origin::Linker);

    assert_eq!(
        lead_node
            .analysis
            .as_ref()
            .and_then(|analysis| analysis.family.as_deref()),
        Some("linker.undefined_reference")
    );
}

#[test]
fn asm_inline_case_01_prefers_the_failing_error_in_verbose_view() {
    let fixture = corpus_fixture("c/asm_inline/case-01");
    let replay = replay_fixture_document(&fixture).unwrap();
    let request = render_request_for_fixture(&fixture, &replay.document, RenderProfile::Verbose);
    let view = build_view_model(&request).unwrap();

    assert_eq!(view.cards.len(), 2);
    assert_eq!(view.cards[0].severity, "error");
    assert_eq!(view.cards[0].raw_message, "impossible constraint in ‘asm’");
    assert_eq!(view.cards[1].severity, "warning");
    assert_eq!(
        view.cards[1].raw_message,
        "‘asm’ operand 0 probably does not match constraints"
    );

    let render_result = render(request).unwrap();
    assert_eq!(
        render_result.displayed_group_refs,
        vec![
            "group-8de1e6e83fb3".to_string(),
            "group-d48624c5088d".to_string()
        ]
    );
}

#[test]
fn init_order_case_01_keeps_declaration_order_evidence_expanded_and_initializer_site_summary_only()
{
    let fixture = corpus_fixture("cpp/init_order/case-01");
    let replay = replay_fixture_document(&fixture).unwrap();
    let visible_group_refs = vec![
        "group-4fe83b216034".to_string(),
        "group-47fde0da878b".to_string(),
    ];
    let summary_only_group_refs = vec!["group-59815819bad1".to_string()];

    for profile in [
        RenderProfile::Default,
        RenderProfile::Concise,
        RenderProfile::Ci,
    ] {
        let request = render_request_for_fixture(&fixture, &replay.document, profile);
        let view = build_view_model(&request).unwrap();
        assert_eq!(
            view.cards
                .iter()
                .map(|card| card.group_id.clone())
                .collect::<Vec<_>>(),
            visible_group_refs,
            "profile {:?} should keep declaration-order evidence expanded",
            profile
        );
        assert_eq!(
            view.summary_only_groups
                .iter()
                .map(|group| group.group_id.clone())
                .collect::<Vec<_>>(),
            summary_only_group_refs,
            "profile {:?} should keep `when initialized here` summary-only",
            profile
        );
        assert_eq!(view.summary_only_groups[0].title, "  when initialized here");

        let render_result = render(request).unwrap();
        assert_eq!(
            render_result.displayed_group_refs, visible_group_refs,
            "profile {:?} should render the same expanded card order",
            profile
        );
        assert_eq!(
            render_result.suppressed_group_count, 1,
            "profile {:?} should count exactly one suppressed summary-only group",
            profile
        );
    }

    let request = render_request_for_fixture(&fixture, &replay.document, RenderProfile::Verbose);
    let view = build_view_model(&request).unwrap();
    let verbose_group_refs = vec![
        "group-4fe83b216034".to_string(),
        "group-47fde0da878b".to_string(),
        "group-59815819bad1".to_string(),
    ];
    assert_eq!(
        view.cards
            .iter()
            .map(|card| card.group_id.clone())
            .collect::<Vec<_>>(),
        verbose_group_refs,
        "verbose should expand the initializer site as well"
    );
    assert!(view.summary_only_groups.is_empty());

    let render_result = render(request).unwrap();
    assert_eq!(render_result.displayed_group_refs, verbose_group_refs);
    assert_eq!(render_result.suppressed_group_count, 0);
}

#[test]
fn replay_fixture_preserves_lead_family_across_structured_and_residual_root_seams() {
    for fixture_id in ["c/syntax/case-10", "c/type/case-12", "cpp/overload/case-08"] {
        let fixture = corpus_fixture(fixture_id);
        let replay = replay_fixture_document(&fixture).unwrap();
        let request =
            render_request_for_fixture(&fixture, &replay.document, RenderProfile::Default);
        let render_result = render(request).unwrap();
        let lead_node =
            lead_node_for_document(&replay.document, &render_result.displayed_group_refs).unwrap();
        let has_residual_root = replay.document.diagnostics.iter().any(|node| {
            matches!(node.provenance.source, ProvenanceSource::ResidualText)
                && matches!(node.semantic_role, diag_core::SemanticRole::Root)
        });

        assert_eq!(
            replay.document.document_completeness,
            if has_residual_root {
                DocumentCompleteness::Partial
            } else {
                DocumentCompleteness::Complete
            }
        );
        assert!(replay.document.diagnostics.iter().any(|node| {
            matches!(node.provenance.source, ProvenanceSource::Compiler)
                && matches!(node.semantic_role, diag_core::SemanticRole::Root)
        }));
        assert_eq!(
            lead_node
                .analysis
                .as_ref()
                .and_then(|analysis| analysis.family.as_deref()),
            fixture
                .expectations
                .semantic
                .as_ref()
                .map(|semantic| semantic.family.as_str())
        );
    }
}

#[test]
fn macro_include_view_model_keeps_excerpt_lines_and_annotations() {
    for fixture_id in ["c/macro_include/case-11", "c/macro_include/case-12"] {
        let fixture = corpus_fixture(fixture_id);
        let replay = replay_fixture_document(&fixture).unwrap();
        let request =
            render_request_for_fixture(&fixture, &replay.document, RenderProfile::Default);
        let view = build_view_model(&request).unwrap();

        let excerpt = view.cards[0].excerpts.first().unwrap();
        assert!(
            !excerpt.lines.is_empty(),
            "{fixture_id} should keep source lines"
        );
        assert!(
            !excerpt.annotations.is_empty(),
            "{fixture_id} should keep caret annotations"
        );
    }
}

#[test]
fn normalizes_transient_line_numbers_before_compare() {
    let expected = "src/main.c:2:25: note: in expansion of macro 'CALL_BAD'\n    2 | int main(void) { return CALL_BAD(); }\n";
    let actual = "src/main.c:3:25: note: in expansion of macro 'CALL_BAD'\n    3 | int main(void) { return CALL_BAD(); }\n";

    let normalized_expected =
        normalize_snapshot_contents(Path::new("stderr.raw"), expected).unwrap();
    let normalized_actual = normalize_snapshot_contents(Path::new("stderr.raw"), actual).unwrap();

    assert_eq!(normalized_expected, normalized_actual);
}

#[test]
fn normalizes_transient_column_numbers_in_location_headers() {
    let expected = "src/main.c:2:25: error: incompatible types\n";
    let actual = "src/main.c:5:41: error: incompatible types\n";

    let normalized_expected =
        normalize_snapshot_contents(Path::new("stderr.raw"), expected).unwrap();
    let normalized_actual = normalize_snapshot_contents(Path::new("stderr.raw"), actual).unwrap();

    assert_eq!(normalized_expected, normalized_actual);
}

#[test]
fn normalizes_volatile_compiler_text_patterns() {
    let expected = "src/main.cpp:4:5: note: candidate: 'void takes(int, int)'\n    4 |     takes(1);\n      |     ^~~~~\ncc1: all warnings being treated as errors\n/usr/bin/ld: /tmp/main.o: in function 'main':\nmain.c:(.text+0x9): undefined reference to 'missing_symbol'\n";
    let actual = "src/main.cpp:7:9: note: there are 2 candidates\nsrc/main.cpp:7:9: note: candidate 1: 'void takes(int, int)'\n    7 |     takes(1);\n      |     ~~~~~^~~\n/usr/bin/ld: /tmp/cc123456.o: in function 'main':\nmain.c:(.text+0x5): undefined reference to 'missing_symbol'\n";

    let normalized_expected =
        normalize_snapshot_contents(Path::new("stderr.raw"), expected).unwrap();
    let normalized_actual = normalize_snapshot_contents(Path::new("stderr.raw"), actual).unwrap();

    assert_eq!(normalized_expected, normalized_actual);
}

#[test]
fn acceptance_metrics_use_expectation_denominators() {
    let mut required = acceptance_summary("c/syntax/case-01", Some("syntax"), "syntax");
    required.fallback_forbidden = true;
    required.primary_location_user_owned_required = true;
    required.primary_location_user_owned = true;
    required.first_action_required = true;
    required.first_action_present = true;

    let mut advisory_only = acceptance_summary("c/partial/case-01", None, "linker");
    advisory_only.used_fallback = true;
    advisory_only.fallback_reason = Some(FallbackReason::ResidualOnly);
    advisory_only.headline_rewritten = false;

    let metrics = acceptance_metrics_for(&[required, advisory_only]);

    assert_eq!(metrics.promoted_fixture_count, 2);
    assert_eq!(metrics.fallback_used_count, 1);
    assert_eq!(metrics.fallback_forbidden_count, 1);
    assert_eq!(metrics.unexpected_fallback_count, 0);
    assert_eq!(
        metrics
            .fallback_reason_counts
            .get(FallbackReason::ResidualOnly.as_str()),
        Some(&1)
    );
    assert!(metrics.unexpected_fallback_reason_counts.is_empty());
    assert_eq!(metrics.primary_location_user_owned_required_count, 1);
    assert_eq!(metrics.primary_location_user_owned_count, 1);
    assert_eq!(metrics.first_action_required_count, 1);
    assert_eq!(metrics.first_action_present_count, 1);
    assert_eq!(metrics.family_expected_count, 1);
    assert_eq!(metrics.family_match_count, 1);
    assert_eq!(metrics.fallback_rate, 0.0);
    assert_eq!(metrics.primary_location_user_owned_rate, 1.0);
    assert_eq!(metrics.first_action_present_rate, 1.0);
    assert_eq!(metrics.family_match_rate, 1.0);
    assert_eq!(metrics.headline_rewritten_rate, 0.5);
}

#[test]
fn older_band_core_parser_path_anchors_declare_matrix_applicability() {
    let cases = [
        (
            "c/preprocessor_directive/case-01",
            vec![
                FixtureSurface::Default,
                FixtureSurface::Ci,
                FixtureSurface::Debug,
            ],
            "GCC13-14 single_sink_structured preprocessor anchor",
        ),
        (
            "c/syntax/case-11",
            vec![FixtureSurface::Default, FixtureSurface::Ci],
            "GCC9-12 single_sink_structured parser/fix-it anchor",
        ),
    ];

    for (fixture_id, expected_surfaces, note_substring) in cases {
        let fixture = corpus_fixture(fixture_id);
        assert_eq!(
            declared_matrix_applicability_surfaces(&fixture).as_deref(),
            Some(expected_surfaces.as_slice()),
            "{fixture_id} should declare the expected matrix_applicability surfaces",
        );

        let matrix = matrix_applicability_for_fixture(&fixture);
        let note = matrix
            .get("note")
            .and_then(YamlValue::as_str)
            .unwrap_or_default();
        assert!(
            note.contains(note_substring),
            "{fixture_id} should carry a path-scoped applicability note",
        );
    }
}

#[test]
fn older_band_core_parser_fallback_replays_keep_first_action_disclosure_and_public_export_shape() {
    for fixture_id in [
        "c/preprocessor_directive/case-01",
        "c/macro_include/case-13",
        "c/partial/case-07",
        "c/syntax/case-11",
        "c/syntax/case-12",
        "c/type/case-11",
        "c/linker/case-11",
    ] {
        let fixture = corpus_fixture(fixture_id);
        let semantic = fixture.expectations.semantic.as_ref().unwrap();
        let replay = replay_fixture_document(&fixture).unwrap();
        let request =
            render_request_for_fixture(&fixture, &replay.document, RenderProfile::Default);
        let render_result = render(request).unwrap();
        let lead_node =
            lead_node_for_document(&replay.document, &render_result.displayed_group_refs).unwrap();

        if semantic.first_action_required {
            assert!(
                lead_node
                    .analysis
                    .as_ref()
                    .and_then(|analysis| analysis.first_action_hint.as_deref())
                    .is_some_and(|hint| !hint.trim().is_empty()),
                "{fixture_id} should keep a lead first_action_hint",
            );
        }

        if let Some(max_line) = fixture
            .expectations
            .render
            .default
            .as_ref()
            .and_then(|expectations| expectations.first_action_max_line)
        {
            let line = first_help_line(&render_result.text).expect("expected help line");
            assert!(
                line <= max_line,
                "{fixture_id} should keep help within line {max_line}, got {line}",
            );
        }

        if fixture
            .expectations
            .render
            .default
            .as_ref()
            .and_then(|expectations| expectations.partial_notice_required)
            == Some(true)
        {
            assert!(
                contains_partial_notice(&render_result.text),
                "{fixture_id} should keep the partial notice visible",
            );
        }

        if fixture
            .expectations
            .render
            .default
            .as_ref()
            .and_then(|expectations| expectations.raw_diagnostics_hint_required)
            == Some(true)
        {
            assert!(
                contains_raw_diagnostics_hint(&render_result.text),
                "{fixture_id} should keep the raw diagnostics hint visible",
            );
        }

        if fixture
            .expectations
            .render
            .default
            .as_ref()
            .and_then(|expectations| expectations.raw_sub_block_required)
            == Some(true)
        {
            assert!(
                contains_raw_sub_block(&render_result.text),
                "{fixture_id} should keep the raw diagnostics sub-block visible",
            );
        }

        let export = public_export_for_fixture(&fixture, &replay);
        assert_eq!(export.status, PublicExportStatus::Available);
        assert_eq!(
            export.execution.version_band,
            fixture.expectations.version_band
        );
        assert_eq!(
            export.execution.processing_path,
            fixture.expectations.processing_path
        );
        assert!(
            export
                .execution
                .allowed_processing_paths
                .contains(&fixture.expectations.processing_path),
            "{fixture_id} should list its processing path in allowed_processing_paths",
        );

        let public_diag = export
            .result
            .as_ref()
            .unwrap()
            .diagnostics
            .iter()
            .find(|diagnostic| diagnostic.family.as_deref() == Some(semantic.family.as_str()))
            .unwrap_or_else(|| panic!("{fixture_id} should export family {}", semantic.family));
        assert!(
            public_diag
                .headline
                .as_deref()
                .is_some_and(|headline| !headline.trim().is_empty()),
            "{fixture_id} should export a non-empty headline for {}",
            semantic.family,
        );
        if semantic.first_action_required {
            assert!(
                public_diag
                    .first_action
                    .as_deref()
                    .is_some_and(|action| !action.trim().is_empty()),
                "{fixture_id} should export a non-empty first_action for {}",
                semantic.family,
            );
        }
        if semantic.raw_provenance_required {
            assert!(
                !public_diag.provenance_capture_refs.is_empty(),
                "{fixture_id} should keep capture refs on the exported diagnostic",
            );
        }
    }
}

#[test]
fn older_band_honest_fallback_anchor_keeps_disclosure_and_public_export_context() {
    let fixture = corpus_fixture("c/macro_include/case-01");
    let replay = replay_fixture_document(&fixture).unwrap();
    let request = render_request_for_fixture(&fixture, &replay.document, RenderProfile::Default);
    let render_result = render(request).unwrap();
    let export = public_export_for_fixture(&fixture, &replay);

    assert_eq!(export.status, PublicExportStatus::Available);
    assert_eq!(
        export.execution.version_band,
        fixture.expectations.version_band
    );
    assert_eq!(
        export.execution.processing_path,
        fixture.expectations.processing_path
    );
    assert_eq!(
        export.execution.fallback_grade.as_deref(),
        Some("fail_open")
    );
    assert_eq!(
        export.execution.fallback_reason.as_deref(),
        Some("residual_only")
    );
    assert!(
        export
            .execution
            .allowed_processing_paths
            .contains(&fixture.expectations.processing_path),
        "honest-fallback anchor should keep its processing path in allowed_processing_paths",
    );
    assert!(
        render_result
            .text
            .contains("In file included from src/wrapper.h:1,"),
        "honest-fallback anchor should preserve the include-chain disclosure",
    );
    assert!(
        render_result
            .text
            .contains("note: in expansion of macro 'CALL_BAD'"),
        "honest-fallback anchor should preserve the macro-expansion disclosure",
    );
    assert!(
        export
            .result
            .as_ref()
            .unwrap()
            .diagnostics
            .iter()
            .any(|diagnostic| {
                diagnostic
                    .first_action
                    .as_deref()
                    .is_some_and(|action| !action.trim().is_empty())
            }),
        "honest-fallback anchor should still export actionable first-action text",
    );
}

#[test]
fn prp07_c_family_fixtures_declare_explicit_older_band_applicability_inventory() {
    let generic_fixtures = [
        "c/format_string/case-01",
        "c/const_qualifier/case-01",
        "c/conversion_narrowing/case-01",
        "c/fallthrough/case-01",
        "c/storage_class/case-01",
        "c/string_character/case-01",
        "c/strict_aliasing/case-01",
        "c/sizeof_allocation/case-01",
        "c/null_pointer/case-01",
        "c/redefinition/case-01",
        "c/return_type/case-01",
        "c/uninitialized/case-01",
        "c/unused/case-01",
        "c/abi_alignment/case-01",
        "c/asm_inline/case-01",
        "c/bit_field_packed/case-01",
        "c/odr_inline_linkage/case-01",
        "c/overflow_arithmetic/case-01",
        "c/pedantic_compliance/case-01",
        "c/sanitizer_buffer/case-01",
    ];
    let applicability_gap_fixtures = ["c/openmp/case-01", "c/analyzer/case-01"];
    let path_gap_fixtures = [
        "c/path/case-01",
        "c/path/case-02",
        "c/path/case-03",
        "c/path/case-04",
        "c/path/case-05",
        "c/path/case-06",
    ];

    for fixture_id in generic_fixtures {
        let fixture = corpus_fixture(fixture_id);
        let meta = meta_yaml_for_fixture(&fixture);
        assert_fixture_does_not_claim_older_band_representative_cells(fixture_id, &meta);
        let applicability = older_band_applicability_for_fixture(&fixture);
        assert_eq!(
            applicability
                .get("shared_contract_when_emitted")
                .and_then(YamlValue::as_bool),
            Some(true),
            "{fixture_id} should declare shared_contract_when_emitted",
        );

        for (version_band, processing_path) in [
            ("gcc13_14", "native_text_capture"),
            ("gcc13_14", "single_sink_structured"),
            ("gcc9_12", "native_text_capture"),
            ("gcc9_12", "single_sink_structured"),
        ] {
            let cell = older_band_applicability_cell(&applicability, version_band, processing_path);
            assert_eq!(
                cell.get("status").and_then(YamlValue::as_str),
                Some("missing_representative_evidence"),
                "{fixture_id} should mark {version_band}/{processing_path} as missing evidence",
            );
            let note = cell
                .get("note")
                .and_then(YamlValue::as_str)
                .unwrap_or_default();
            assert!(
                note.contains(
                    "Do not infer older-band coverage from the GCC15 dual_sink_structured fixture."
                ),
                "{fixture_id} should keep an explicit non-inference note for {version_band}/{processing_path}",
            );
        }
    }

    for fixture_id in applicability_gap_fixtures {
        let fixture = corpus_fixture(fixture_id);
        let meta = meta_yaml_for_fixture(&fixture);
        assert_fixture_does_not_claim_older_band_representative_cells(fixture_id, &meta);
        let applicability = older_band_applicability_for_fixture(&fixture);
        assert_eq!(
            applicability
                .get("shared_contract_when_emitted")
                .and_then(YamlValue::as_bool),
            Some(true),
            "{fixture_id} should declare shared_contract_when_emitted",
        );
        for (version_band, processing_path) in [
            ("gcc13_14", "native_text_capture"),
            ("gcc13_14", "single_sink_structured"),
            ("gcc9_12", "native_text_capture"),
            ("gcc9_12", "single_sink_structured"),
        ] {
            let note = older_band_applicability_cell(&applicability, version_band, processing_path)
                .get("note")
                .and_then(YamlValue::as_str)
                .unwrap_or_default();
            assert!(
                note.contains("applicability gap"),
                "{fixture_id} should describe {version_band}/{processing_path} as an applicability gap",
            );
            assert!(
                note.contains("GCC15 dual_sink_structured fixture"),
                "{fixture_id} should keep the non-inference note tied to the GCC15 fixture",
            );
        }
    }

    for fixture_id in path_gap_fixtures {
        let fixture = corpus_fixture(fixture_id);
        let meta = meta_yaml_for_fixture(&fixture);
        assert_fixture_does_not_claim_older_band_representative_cells(fixture_id, &meta);
        let applicability = older_band_applicability_for_fixture(&fixture);
        assert_eq!(
            applicability
                .get("shared_contract_when_emitted")
                .and_then(YamlValue::as_bool),
            Some(true),
            "{fixture_id} should declare shared_contract_when_emitted",
        );
        for (version_band, processing_path) in [
            ("gcc13_14", "native_text_capture"),
            ("gcc13_14", "single_sink_structured"),
            ("gcc9_12", "native_text_capture"),
            ("gcc9_12", "single_sink_structured"),
        ] {
            let note = older_band_applicability_cell(&applicability, version_band, processing_path)
                .get("note")
                .and_then(YamlValue::as_str)
                .unwrap_or_default();
            assert!(
                note.contains("applicability gap"),
                "{fixture_id} should describe {version_band}/{processing_path} as an applicability gap",
            );
            assert!(
                note.contains("legacy-root fixture"),
                "{fixture_id} should keep the legacy-root caveat for {version_band}/{processing_path}",
            );
        }
    }
}

#[test]
fn prp07_c_emitted_family_replays_keep_shared_render_and_public_export_contract() {
    for fixture_id in [
        "c/format_string/case-01",
        "c/const_qualifier/case-01",
        "c/conversion_narrowing/case-01",
        "c/fallthrough/case-01",
        "c/storage_class/case-01",
        "c/string_character/case-01",
        "c/strict_aliasing/case-01",
        "c/sizeof_allocation/case-01",
        "c/null_pointer/case-01",
        "c/redefinition/case-01",
        "c/return_type/case-01",
        "c/uninitialized/case-01",
        "c/unused/case-01",
        "c/abi_alignment/case-01",
        "c/asm_inline/case-01",
        "c/bit_field_packed/case-01",
        "c/odr_inline_linkage/case-01",
        "c/overflow_arithmetic/case-01",
        "c/pedantic_compliance/case-01",
        "c/sanitizer_buffer/case-01",
        "c/openmp/case-01",
        "c/analyzer/case-01",
    ] {
        let fixture = corpus_fixture(fixture_id);
        let semantic = fixture.expectations.semantic.as_ref().unwrap();
        let replay = replay_fixture_document(&fixture).unwrap();
        let request =
            render_request_for_fixture(&fixture, &replay.document, RenderProfile::Default);
        let render_result = render(request).unwrap();
        let lead_node =
            lead_node_for_document(&replay.document, &render_result.displayed_group_refs).unwrap();

        assert_eq!(
            lead_node
                .analysis
                .as_ref()
                .and_then(|analysis| analysis.family.as_deref()),
            Some(semantic.family.as_str()),
            "{fixture_id} should keep {0} as the lead family",
            semantic.family,
        );

        if semantic.first_action_required {
            assert!(
                lead_node
                    .analysis
                    .as_ref()
                    .and_then(|analysis| analysis.first_action_hint.as_deref())
                    .is_some_and(|hint| !hint.trim().is_empty()),
                "{fixture_id} should keep a lead first_action_hint",
            );
        }

        if let Some(max_line) = fixture
            .expectations
            .render
            .default
            .as_ref()
            .and_then(|expectations| expectations.first_action_max_line)
        {
            let line = first_help_line(&render_result.text).expect("expected help line");
            assert!(
                line <= max_line,
                "{fixture_id} should keep help within line {max_line}, got {line}",
            );
        }

        if fixture
            .expectations
            .render
            .default
            .as_ref()
            .and_then(|expectations| expectations.partial_notice_required)
            == Some(true)
        {
            assert!(
                contains_partial_notice(&render_result.text),
                "{fixture_id} should keep the partial notice visible",
            );
        }

        if fixture
            .expectations
            .render
            .default
            .as_ref()
            .and_then(|expectations| expectations.raw_diagnostics_hint_required)
            == Some(true)
        {
            assert!(
                contains_raw_diagnostics_hint(&render_result.text),
                "{fixture_id} should keep the raw diagnostics hint visible",
            );
        }

        if fixture
            .expectations
            .render
            .default
            .as_ref()
            .and_then(|expectations| expectations.raw_sub_block_required)
            == Some(true)
        {
            assert!(
                contains_raw_sub_block(&render_result.text),
                "{fixture_id} should keep the raw diagnostics sub-block visible",
            );
        }

        if fixture
            .expectations
            .render
            .default
            .as_ref()
            .and_then(|expectations| expectations.low_confidence_notice_required)
            == Some(true)
        {
            assert!(
                render_result.text.contains("confidence is limited"),
                "{fixture_id} should keep the low-confidence notice visible",
            );
        }

        let export = public_export_for_fixture(&fixture, &replay);
        assert_eq!(export.status, PublicExportStatus::Available);
        assert_eq!(
            export.execution.version_band,
            fixture.expectations.version_band
        );
        assert_eq!(
            export.execution.processing_path,
            fixture.expectations.processing_path
        );
        assert!(
            export
                .execution
                .allowed_processing_paths
                .contains(&fixture.expectations.processing_path),
            "{fixture_id} should list its processing path in allowed_processing_paths",
        );

        let diagnostics = &export.result.as_ref().unwrap().diagnostics;
        assert!(
            !diagnostics.is_empty(),
            "{fixture_id} should export at least one diagnostic",
        );
        assert!(
            diagnostics.iter().any(|diagnostic| {
                diagnostic
                    .family
                    .as_deref()
                    .is_some_and(|family| family == semantic.family.as_str())
            }),
            "{fixture_id} should export at least one diagnostic tagged as {}",
            semantic.family,
        );
        assert!(
            diagnostics.iter().any(|diagnostic| {
                diagnostic
                    .headline
                    .as_deref()
                    .is_some_and(|headline| !headline.trim().is_empty())
            }),
            "{fixture_id} should export a non-empty headline",
        );
        if semantic.first_action_required {
            assert!(
                diagnostics.iter().any(|diagnostic| {
                    diagnostic
                        .first_action
                        .as_deref()
                        .is_some_and(|action| !action.trim().is_empty())
                }),
                "{fixture_id} should export at least one non-empty first_action",
            );
        }
        if semantic.raw_provenance_required {
            assert!(
                diagnostics
                    .iter()
                    .any(|diagnostic| !diagnostic.provenance_capture_refs.is_empty()),
                "{fixture_id} should keep provenance_capture_refs on at least one exported diagnostic",
            );
        }
    }
}

#[test]
fn prp08_cpp_family_anchors_declare_older_band_inventory_and_shared_contract_proof() {
    let gcc15_only_core_fixtures = [
        "cpp/access_control/case-01",
        "cpp/deleted_function/case-01",
        "cpp/deprecated/case-01",
        "cpp/enum_switch/case-01",
        "cpp/exception_handling/case-01",
        "cpp/inheritance_virtual/case-01",
        "cpp/init_order/case-01",
        "cpp/move_semantics/case-01",
        "cpp/pointer_reference/case-01",
        "cpp/scope_declaration/case-01",
    ];

    for fixture_id in gcc15_only_core_fixtures {
        let fixture = corpus_fixture(fixture_id);
        let meta = meta_yaml_for_fixture(&fixture);
        assert_fixture_does_not_claim_older_band_representative_cells(fixture_id, &meta);
        let applicability = older_band_applicability_for_fixture(&fixture);
        assert_eq!(
            applicability
                .get("shared_contract_when_emitted")
                .and_then(YamlValue::as_bool),
            Some(true),
            "{fixture_id} should declare shared_contract_when_emitted",
        );

        for (version_band, processing_path) in [
            ("gcc13_14", "native_text_capture"),
            ("gcc13_14", "single_sink_structured"),
            ("gcc9_12", "native_text_capture"),
            ("gcc9_12", "single_sink_structured"),
        ] {
            let cell = older_band_applicability_cell(&applicability, version_band, processing_path);
            assert_eq!(
                cell.get("status").and_then(YamlValue::as_str),
                Some("missing_representative_evidence"),
                "{fixture_id} should mark {version_band}/{processing_path} as missing evidence",
            );
            let note = cell
                .get("note")
                .and_then(YamlValue::as_str)
                .unwrap_or_default();
            assert!(
                note.contains("Do not infer")
                    && note.contains("GCC15 dual_sink_structured fixture"),
                "{fixture_id} should keep a non-inference note for {version_band}/{processing_path}",
            );
        }
    }

    let overload_anchor = corpus_fixture("cpp/overload/case-01");
    let overload_meta = meta_yaml_for_fixture(&overload_anchor);
    assert_fixture_does_not_claim_older_band_representative_cells(
        "cpp/overload/case-01",
        &overload_meta,
    );
    let overload_applicability = older_band_applicability_for_fixture(&overload_anchor);
    assert_eq!(
        overload_applicability
            .get("shared_contract_when_emitted")
            .and_then(YamlValue::as_bool),
        Some(true),
        "cpp/overload/case-01 should declare shared_contract_when_emitted",
    );
    for (version_band, processing_path) in [
        ("gcc13_14", "native_text_capture"),
        ("gcc13_14", "single_sink_structured"),
        ("gcc9_12", "native_text_capture"),
    ] {
        let cell =
            older_band_applicability_cell(&overload_applicability, version_band, processing_path);
        assert_eq!(
            cell.get("status").and_then(YamlValue::as_str),
            Some("missing_representative_evidence"),
            "cpp/overload/case-01 should mark {version_band}/{processing_path} as missing evidence",
        );
        let note = cell
            .get("note")
            .and_then(YamlValue::as_str)
            .unwrap_or_default();
        assert!(
            note.contains("Do not infer") && note.contains("GCC15 dual_sink_structured fixture"),
            "cpp/overload/case-01 should keep a non-inference note for {version_band}/{processing_path}",
        );
    }
    let overload_single_sink =
        older_band_applicability_cell(&overload_applicability, "gcc9_12", "single_sink_structured");
    assert_eq!(
        overload_single_sink
            .get("status")
            .and_then(YamlValue::as_str),
        Some("representative_evidence"),
        "cpp/overload/case-01 should mark gcc9_12/single_sink_structured as representative evidence",
    );
    assert_eq!(
        yaml_string_sequence(overload_single_sink.get("representative_fixtures")),
        vec!["cpp/overload/case-07".to_string()],
        "cpp/overload/case-01 should point at the checked-in overload representative proof fixture",
    );
    let overload_note = overload_single_sink
        .get("note")
        .and_then(YamlValue::as_str)
        .unwrap_or_default();
    assert!(
        overload_note.contains("shared overload contract")
            && overload_note.contains("lower guarantee"),
        "cpp/overload/case-01 should describe representative proof as shared-contract evidence",
    );
    assert_representative_fixture_matches_band_and_path(
        "cpp/overload/case-07",
        diag_backend_probe::VersionBand::Gcc9_12,
        diag_backend_probe::ProcessingPath::SingleSinkStructured,
    );

    let template_anchor = corpus_fixture("cpp/template/case-01");
    let template_meta = meta_yaml_for_fixture(&template_anchor);
    assert_fixture_does_not_claim_older_band_representative_cells(
        "cpp/template/case-01",
        &template_meta,
    );
    let template_applicability = older_band_applicability_for_fixture(&template_anchor);
    assert_eq!(
        template_applicability
            .get("shared_contract_when_emitted")
            .and_then(YamlValue::as_bool),
        Some(true),
        "cpp/template/case-01 should declare shared_contract_when_emitted",
    );
    for (version_band, processing_path) in [
        ("gcc13_14", "native_text_capture"),
        ("gcc9_12", "native_text_capture"),
    ] {
        let cell =
            older_band_applicability_cell(&template_applicability, version_band, processing_path);
        assert_eq!(
            cell.get("status").and_then(YamlValue::as_str),
            Some("missing_representative_evidence"),
            "cpp/template/case-01 should mark {version_band}/{processing_path} as missing evidence",
        );
        let note = cell
            .get("note")
            .and_then(YamlValue::as_str)
            .unwrap_or_default();
        assert!(
            note.contains("Do not infer") && note.contains("GCC15 dual_sink_structured fixture"),
            "cpp/template/case-01 should keep a non-inference note for {version_band}/{processing_path}",
        );
    }
    let template_gcc13_single = older_band_applicability_cell(
        &template_applicability,
        "gcc13_14",
        "single_sink_structured",
    );
    assert_eq!(
        template_gcc13_single
            .get("status")
            .and_then(YamlValue::as_str),
        Some("representative_evidence"),
        "cpp/template/case-01 should mark gcc13_14/single_sink_structured as representative evidence",
    );
    assert_eq!(
        yaml_string_sequence(template_gcc13_single.get("representative_fixtures")),
        vec![
            "cpp/template/case-13".to_string(),
            "cpp/template/case-15".to_string(),
        ],
        "cpp/template/case-01 should point at the checked-in GCC13-14 template proof fixtures",
    );
    let template_gcc13_note = template_gcc13_single
        .get("note")
        .and_then(YamlValue::as_str)
        .unwrap_or_default();
    assert!(
        template_gcc13_note.contains("shared template contract")
            && template_gcc13_note.contains("lower guarantee"),
        "cpp/template/case-01 should describe GCC13-14 template proof as shared-contract evidence",
    );
    let template_gcc9_single =
        older_band_applicability_cell(&template_applicability, "gcc9_12", "single_sink_structured");
    assert_eq!(
        template_gcc9_single
            .get("status")
            .and_then(YamlValue::as_str),
        Some("representative_evidence"),
        "cpp/template/case-01 should mark gcc9_12/single_sink_structured as representative evidence",
    );
    assert_eq!(
        yaml_string_sequence(template_gcc9_single.get("representative_fixtures")),
        vec!["cpp/template/case-14".to_string()],
        "cpp/template/case-01 should point at the checked-in GCC9-12 template proof fixture",
    );
    let template_gcc9_note = template_gcc9_single
        .get("note")
        .and_then(YamlValue::as_str)
        .unwrap_or_default();
    assert!(
        template_gcc9_note.contains("shared template contract")
            && template_gcc9_note.contains("lower guarantee"),
        "cpp/template/case-01 should describe GCC9-12 template proof as shared-contract evidence",
    );
    for fixture_id in ["cpp/template/case-13", "cpp/template/case-15"] {
        assert_representative_fixture_matches_band_and_path(
            fixture_id,
            diag_backend_probe::VersionBand::Gcc13_14,
            diag_backend_probe::ProcessingPath::SingleSinkStructured,
        );
    }
    assert_representative_fixture_matches_band_and_path(
        "cpp/template/case-14",
        diag_backend_probe::VersionBand::Gcc9_12,
        diag_backend_probe::ProcessingPath::SingleSinkStructured,
    );
}

#[test]
fn prp08_cpp_replays_keep_shared_render_and_public_export_contract() {
    for fixture_id in [
        "cpp/access_control/case-01",
        "cpp/deleted_function/case-01",
        "cpp/deprecated/case-01",
        "cpp/enum_switch/case-01",
        "cpp/exception_handling/case-01",
        "cpp/inheritance_virtual/case-01",
        "cpp/init_order/case-01",
        "cpp/move_semantics/case-01",
        "cpp/pointer_reference/case-01",
        "cpp/scope_declaration/case-01",
        "cpp/overload/case-01",
        "cpp/overload/case-07",
        "cpp/template/case-01",
        "cpp/template/case-13",
        "cpp/template/case-14",
        "cpp/template/case-15",
    ] {
        let fixture = corpus_fixture(fixture_id);
        let semantic = fixture.expectations.semantic.as_ref().unwrap();
        let replay = replay_fixture_document(&fixture).unwrap();
        let request =
            render_request_for_fixture(&fixture, &replay.document, RenderProfile::Default);
        let render_result = render(request).unwrap();
        let lead_node =
            lead_node_for_document(&replay.document, &render_result.displayed_group_refs).unwrap();

        assert_eq!(
            lead_node
                .analysis
                .as_ref()
                .and_then(|analysis| analysis.family.as_deref()),
            Some(semantic.family.as_str()),
            "{fixture_id} should keep {0} as the lead family",
            semantic.family,
        );

        if semantic.first_action_required {
            assert!(
                lead_node
                    .analysis
                    .as_ref()
                    .and_then(|analysis| analysis.first_action_hint.as_deref())
                    .is_some_and(|hint| !hint.trim().is_empty()),
                "{fixture_id} should keep a lead first_action_hint",
            );
        }

        if let Some(max_line) = fixture
            .expectations
            .render
            .default
            .as_ref()
            .and_then(|expectations| expectations.first_action_max_line)
        {
            let line = first_help_line(&render_result.text).expect("expected help line");
            assert!(
                line <= max_line,
                "{fixture_id} should keep help within line {max_line}, got {line}",
            );
        }

        if fixture
            .expectations
            .render
            .default
            .as_ref()
            .and_then(|expectations| expectations.partial_notice_required)
            == Some(true)
        {
            assert!(
                contains_partial_notice(&render_result.text),
                "{fixture_id} should keep the partial notice visible",
            );
        }

        if fixture
            .expectations
            .render
            .default
            .as_ref()
            .and_then(|expectations| expectations.raw_diagnostics_hint_required)
            == Some(true)
        {
            assert!(
                contains_raw_diagnostics_hint(&render_result.text),
                "{fixture_id} should keep the raw diagnostics hint visible",
            );
        }

        if fixture
            .expectations
            .render
            .default
            .as_ref()
            .and_then(|expectations| expectations.raw_sub_block_required)
            == Some(true)
        {
            assert!(
                contains_raw_sub_block(&render_result.text),
                "{fixture_id} should keep the raw diagnostics sub-block visible",
            );
        }

        let export = public_export_for_fixture(&fixture, &replay);
        assert_eq!(export.status, PublicExportStatus::Available);
        assert_eq!(
            export.execution.version_band,
            fixture.expectations.version_band
        );
        assert_eq!(
            export.execution.processing_path,
            fixture.expectations.processing_path
        );
        assert!(
            export
                .execution
                .allowed_processing_paths
                .contains(&fixture.expectations.processing_path),
            "{fixture_id} should list its processing path in allowed_processing_paths",
        );

        if matches!(
            fixture_id,
            "cpp/overload/case-07"
                | "cpp/template/case-13"
                | "cpp/template/case-14"
                | "cpp/template/case-15"
        ) {
            assert!(
                export
                    .execution
                    .allowed_processing_paths
                    .contains(&"native_text_capture".to_string()),
                "{fixture_id} should keep native_text_capture in allowed_processing_paths for older-band shared-contract proof",
            );
            assert!(
                export
                    .execution
                    .allowed_processing_paths
                    .contains(&"passthrough".to_string()),
                "{fixture_id} should keep passthrough in allowed_processing_paths for older-band shared-contract proof",
            );
        }

        let diagnostics = &export.result.as_ref().unwrap().diagnostics;
        assert!(
            !diagnostics.is_empty(),
            "{fixture_id} should export at least one diagnostic",
        );
        let matching_diagnostics = diagnostics
            .iter()
            .filter(|diagnostic| {
                diagnostic
                    .family
                    .as_deref()
                    .is_some_and(|family| family == semantic.family.as_str())
            })
            .collect::<Vec<_>>();
        assert!(
            !matching_diagnostics.is_empty(),
            "{fixture_id} should export at least one diagnostic tagged as {}",
            semantic.family,
        );
        assert!(
            matching_diagnostics.iter().any(|diagnostic| {
                diagnostic
                    .headline
                    .as_deref()
                    .is_some_and(|headline| !headline.trim().is_empty())
            }),
            "{fixture_id} should export a non-empty headline for {}",
            semantic.family,
        );
        if semantic.first_action_required {
            assert!(
                matching_diagnostics.iter().any(|diagnostic| {
                    diagnostic
                        .first_action
                        .as_deref()
                        .is_some_and(|action| !action.trim().is_empty())
                }),
                "{fixture_id} should export a non-empty first_action for {}",
                semantic.family,
            );
        }
        if semantic.raw_provenance_required {
            assert!(
                matching_diagnostics
                    .iter()
                    .any(|diagnostic| !diagnostic.provenance_capture_refs.is_empty()),
                "{fixture_id} should keep provenance_capture_refs on exported diagnostics for {}",
                semantic.family,
            );
        }
    }
}

#[test]
fn prp09_modern_cpp_anchors_declare_gcc13_matrix_and_explicit_gcc9_12_applicability_inventory() {
    let fixtures = [
        (
            "cpp/constexpr/case-01",
            "native_text_capture",
            "The compiler can emit this family on GCC9-12",
        ),
        (
            "cpp/lambda_closure/case-01",
            "native_text_capture",
            "The compiler can emit this family on GCC9-12",
        ),
        (
            "cpp/lifetime_dangling/case-01",
            "native_text_capture",
            "The compiler can emit this family on GCC9-12",
        ),
        (
            "cpp/structured_binding/case-01",
            "native_text_capture",
            "The compiler can emit this family on GCC9-12",
        ),
        (
            "cpp/designated_init/case-01",
            "native_text_capture",
            "The compiler can emit this family on GCC9-12",
        ),
        (
            "cpp/three_way_comparison/case-01",
            "single_sink_structured",
            "older front ends are unavailable for part of the band",
        ),
        (
            "cpp/concepts_constraints/case-01",
            "single_sink_structured",
            "older front ends are unavailable for part of the band",
        ),
        (
            "cpp/coroutine/case-01",
            "single_sink_structured",
            "older front ends are unavailable for part of the band",
        ),
        (
            "cpp/module_import/case-01",
            "native_text_capture",
            "GCC9-GCC10 front ends are unavailable",
        ),
        (
            "cpp/ranges_views/case-01",
            "single_sink_structured",
            "older front ends are unavailable for part of the band",
        ),
    ];

    for (fixture_id, processing_path, note_needle) in fixtures {
        let fixture = corpus_fixture(fixture_id);
        let meta = meta_yaml_for_fixture(&fixture);
        let tags = yaml_string_sequence(meta.get("tags"));

        assert_eq!(
            fixture.expectations.version_band,
            "gcc13_14",
            "{fixture_id} should be promoted as a gcc13_14 representative anchor",
        );
        assert_eq!(
            fixture.expectations.processing_path,
            processing_path,
            "{fixture_id} should declare the expected representative processing path",
        );
        for required_tag in [
            "beta-bar",
            "representative",
            "band:gcc13_14",
            "surface:default",
            "surface:ci",
            "fallback_contract:bounded_render",
        ] {
            assert!(
                tags.iter().any(|tag| tag == required_tag),
                "{fixture_id} should keep tag {required_tag}",
            );
        }
        assert!(
            tags.iter()
                .any(|tag| tag == format!("processing_path:{processing_path}").as_str()),
            "{fixture_id} should keep the processing-path representative tag",
        );

        let matrix = matrix_applicability_for_fixture(&fixture);
        assert_eq!(
            matrix.get("version_band").and_then(YamlValue::as_str),
            Some("gcc13_14"),
            "{fixture_id} should declare gcc13_14 matrix applicability",
        );
        assert_eq!(
            matrix.get("processing_path").and_then(YamlValue::as_str),
            Some(processing_path),
            "{fixture_id} should declare the expected matrix processing path",
        );
        assert_eq!(
            yaml_string_sequence(matrix.get("surfaces")),
            vec!["default".to_string(), "ci".to_string()],
            "{fixture_id} should declare default/ci as the checked-in stop-ship surfaces",
        );
        let matrix_note = matrix
            .get("note")
            .and_then(YamlValue::as_str)
            .unwrap_or_default();
        assert!(
            matrix_note.contains("debug surface is intentionally omitted"),
            "{fixture_id} should explain the missing debug surface",
        );

        let applicability = older_band_applicability_for_fixture(&fixture);
        assert_eq!(
            applicability
                .get("shared_contract_when_emitted")
                .and_then(YamlValue::as_bool),
            Some(true),
            "{fixture_id} should declare shared_contract_when_emitted",
        );
        for older_path in ["native_text_capture", "single_sink_structured"] {
            let cell = older_band_applicability_cell(&applicability, "gcc9_12", older_path);
            assert_eq!(
                cell.get("status").and_then(YamlValue::as_str),
                Some("missing_representative_evidence"),
                "{fixture_id} should mark gcc9_12/{older_path} as missing representative evidence",
            );
            let note = cell
                .get("note")
                .and_then(YamlValue::as_str)
                .unwrap_or_default();
            assert!(
                note.contains(note_needle),
                "{fixture_id} should explain gcc9_12/{older_path} with the expected applicability note",
            );
            let normalized_note = note.to_ascii_lowercase();
            assert!(
                normalized_note.contains("do not infer"),
                "{fixture_id} should keep the explicit non-inference note for gcc9_12/{older_path}",
            );
        }

        let primary_snapshot_root = fixture.snapshot_root();
        assert!(
            primary_snapshot_root.join("public.export.json").exists(),
            "{fixture_id} should keep a checked-in gcc13_14 representative public export snapshot",
        );

        let gcc15_companion =
            fixture_with_snapshot(fixture_id, "gcc15", "dual_sink_structured", "15");
        assert!(
            gcc15_companion.snapshot_root().join("public.export.json").exists(),
            "{fixture_id} should keep a checked-in gcc15 companion public export snapshot",
        );
    }
}

#[test]
fn prp09_modern_cpp_replays_keep_shared_render_and_public_export_contract() {
    let fixtures = [
        (
            "cpp/constexpr/case-01",
            "constexpr",
            false,
            diag_backend_probe::ProcessingPath::NativeTextCapture,
        ),
        (
            "cpp/lambda_closure/case-01",
            "lambda_closure",
            false,
            diag_backend_probe::ProcessingPath::NativeTextCapture,
        ),
        (
            "cpp/lifetime_dangling/case-01",
            "lifetime_dangling",
            false,
            diag_backend_probe::ProcessingPath::NativeTextCapture,
        ),
        (
            "cpp/structured_binding/case-01",
            "structured_binding",
            false,
            diag_backend_probe::ProcessingPath::NativeTextCapture,
        ),
        (
            "cpp/designated_init/case-01",
            "designated_init",
            false,
            diag_backend_probe::ProcessingPath::NativeTextCapture,
        ),
        (
            "cpp/three_way_comparison/case-01",
            "three_way_comparison",
            false,
            diag_backend_probe::ProcessingPath::SingleSinkStructured,
        ),
        (
            "cpp/concepts_constraints/case-01",
            "concepts_constraints",
            false,
            diag_backend_probe::ProcessingPath::SingleSinkStructured,
        ),
        (
            "cpp/coroutine/case-01",
            "coroutine",
            false,
            diag_backend_probe::ProcessingPath::SingleSinkStructured,
        ),
        (
            "cpp/module_import/case-01",
            "module_import",
            true,
            diag_backend_probe::ProcessingPath::NativeTextCapture,
        ),
        (
            "cpp/ranges_views/case-01",
            "ranges_views",
            false,
            diag_backend_probe::ProcessingPath::SingleSinkStructured,
        ),
    ];

    for (fixture_id, expected_family, expect_residual_only_passthrough, expected_path) in fixtures {
        let fixture = corpus_fixture(fixture_id);
        assert_eq!(
            fixture_processing_path(&fixture),
            expected_path,
            "{fixture_id} should keep the expected gcc13_14 representative path",
        );
        assert_emitted_family_replay_contract(
            &fixture,
            expected_family,
            expect_residual_only_passthrough,
        );

        let gcc15_companion =
            fixture_with_snapshot(fixture_id, "gcc15", "dual_sink_structured", "15");
        assert_eq!(
            fixture_processing_path(&gcc15_companion),
            diag_backend_probe::ProcessingPath::DualSinkStructured,
            "{fixture_id} should keep a gcc15 dual-sink companion snapshot",
        );
        assert_emitted_family_replay_contract(&gcc15_companion, expected_family, false);
    }
}

#[test]
fn snapshot_reports_count_reason_coded_fallbacks() {
    let fixtures = vec![
        SnapshotFixtureReport {
            fixture_id: "c/partial/case-01".to_string(),
            family_key: "partial".to_string(),
            fallback_reason: Some(FallbackReason::ResidualOnly),
            artifact_diffs: Vec::new(),
        },
        SnapshotFixtureReport {
            fixture_id: "c/syntax/case-01".to_string(),
            family_key: "syntax".to_string(),
            fallback_reason: Some(FallbackReason::SarifMissing),
            artifact_diffs: Vec::new(),
        },
        SnapshotFixtureReport {
            fixture_id: "c/syntax/case-02".to_string(),
            family_key: "syntax".to_string(),
            fallback_reason: Some(FallbackReason::SarifMissing),
            artifact_diffs: Vec::new(),
        },
    ];

    let counts = count_snapshot_fallback_reasons(&fixtures);
    assert_eq!(counts.get(FallbackReason::ResidualOnly.as_str()), Some(&1));
    assert_eq!(counts.get(FallbackReason::SarifMissing.as_str()), Some(&2));
}

#[test]
fn curated_corpus_shape_rejects_fixture_count_below_beta_bar() {
    let counts = BTreeMap::from([
        ("syntax".to_string(), 8),
        ("type".to_string(), 10),
        ("overload".to_string(), 6),
        ("template".to_string(), 12),
        ("macro_include".to_string(), 10),
        ("linker".to_string(), 10),
        ("partial".to_string(), 6),
        ("path".to_string(), 6),
    ]);

    let error = enforce_minimum_corpus_shape(79, &counts).unwrap_err();
    assert!(error.to_string().contains("curated corpus below beta bar"));
}

#[test]
fn curated_corpus_shape_rejects_fixture_count_above_beta_bar() {
    let counts = BTreeMap::from([
        ("syntax".to_string(), 12),
        ("type".to_string(), 14),
        ("overload".to_string(), 10),
        ("template".to_string(), 18),
        ("macro_include".to_string(), 14),
        ("linker".to_string(), 14),
        ("partial".to_string(), 8),
        ("path".to_string(), 10),
    ]);

    let error = enforce_minimum_corpus_shape(130, &counts).unwrap_err();
    assert!(
        error
            .to_string()
            .contains("curated corpus exceeded beta bar")
    );
}

#[test]
fn curated_corpus_shape_rejects_family_quota_regressions() {
    let counts = BTreeMap::from([
        ("syntax".to_string(), 8),
        ("type".to_string(), 9),
        ("overload".to_string(), 6),
        ("template".to_string(), 12),
        ("macro_include".to_string(), 10),
        ("linker".to_string(), 10),
        ("partial".to_string(), 6),
        ("path".to_string(), 6),
    ]);

    let error = enforce_minimum_corpus_shape(80, &counts).unwrap_err();
    assert!(
        error
            .to_string()
            .contains("family `type` below minimum fixture count")
    );
}

#[test]
fn package_smoke_emits_release_artifacts() {
    let _guard = release_test_lock().lock().unwrap();
    let version = current_release_fixture_version();
    let (_sandbox, repo_root, binary_path) = init_release_repo(version);
    let package = run_package_at(
        &repo_root,
        &PackageOptions {
            binary: binary_path.clone(),
            debug_binary: None,
            target_triple: "x86_64-unknown-linux-musl".to_string(),
            out_dir: PathBuf::from("dist"),
            release_channel: "stable".to_string(),
            maturity_label: "v1beta".to_string(),
            signing_private_key: None,
        },
    )
    .unwrap();

    assert!(package.primary_archive.exists());
    assert!(package.debug_archive.exists());
    assert!(package.source_archive.exists());
    assert!(package.manifest_path.exists());
    assert!(package.build_info_path.exists());
    assert!(package.shasums_path.exists());

    let manifest =
        serde_json::from_str::<BuildManifest>(&fs::read_to_string(&package.manifest_path).unwrap())
            .unwrap();
    assert_eq!(manifest.product_name, DEFAULT_PRODUCT_NAME);
    assert_eq!(manifest.artifact_target_triple, "x86_64-unknown-linux-musl");
    assert_eq!(manifest.artifact_libc_family, "musl");
    assert_eq!(manifest.release_channel, "stable");
    assert_eq!(manifest.maturity_label, "v1beta");
    assert_eq!(manifest.checksums.len(), 7);
    assert!(
        manifest
            .checksums
            .iter()
            .any(|entry| entry.path == "bin/gcc-formed")
    );

    let shasums = fs::read_to_string(&package.shasums_path).unwrap();
    assert!(
        shasums.contains(
            package
                .primary_archive
                .file_name()
                .and_then(|value| value.to_str())
                .unwrap()
        )
    );
    assert!(
        shasums.contains(
            package
                .source_archive
                .file_name()
                .and_then(|value| value.to_str())
                .unwrap()
        )
    );

    let output = Command::new("tar")
        .args(["-tzf", &package.primary_archive.display().to_string()])
        .output()
        .unwrap();
    assert!(output.status.success());
    let listing = String::from_utf8(output.stdout).unwrap();
    assert!(listing.contains("bin/gcc-formed"));
    assert!(listing.contains("bin/g++-formed"));
    assert!(listing.contains("manifest.json"));
    assert!(listing.contains("build-info.txt"));
    assert!(listing.contains("share/doc/gcc-formed/README.md"));
    assert!(listing.contains("share/licenses/gcc-formed/LICENSE"));
}

#[test]
fn package_rejects_dirty_worktree() {
    let _guard = release_test_lock().lock().unwrap();
    let version = current_release_fixture_version();
    let (_sandbox, repo_root, binary_path) = init_release_repo(version);
    write_file(&repo_root.join("dirty.txt"), b"untracked\n");

    let error = run_package_at(
        &repo_root,
        &PackageOptions {
            binary: binary_path,
            debug_binary: None,
            target_triple: "x86_64-unknown-linux-gnu".to_string(),
            out_dir: PathBuf::from("dist"),
            release_channel: "stable".to_string(),
            maturity_label: "v1beta".to_string(),
            signing_private_key: None,
        },
    )
    .unwrap_err();
    assert!(error.to_string().contains("clean git worktree"));
}

#[test]
fn package_requires_release_documents() {
    let _guard = release_test_lock().lock().unwrap();
    let version = current_release_fixture_version();
    let (_sandbox, repo_root, binary_path) = init_release_repo(version);
    fs::remove_file(repo_root.join("NOTICE")).unwrap();
    run_command(&repo_root, "git", &["add", "-u"]);
    run_command(&repo_root, "git", &["commit", "-q", "-m", "remove notice"]);

    let error = run_package_at(
        &repo_root,
        &PackageOptions {
            binary: binary_path,
            debug_binary: None,
            target_triple: "x86_64-unknown-linux-gnu".to_string(),
            out_dir: PathBuf::from("dist"),
            release_channel: "stable".to_string(),
            maturity_label: "v1beta".to_string(),
            signing_private_key: None,
        },
    )
    .unwrap_err();
    assert!(error.to_string().contains("required release input missing"));
}

#[test]
fn artifact_slug_is_platform_focused() {
    assert_eq!(
        artifact_slug_for_target("x86_64-unknown-linux-musl"),
        "linux-x86_64-musl"
    );
    assert_eq!(
        artifact_slug_for_target("aarch64-unknown-linux-gnu"),
        "linux-aarch64-gnu"
    );
}

#[test]
fn vendored_source_config_replaces_crates_io() {
    let _guard = release_test_lock().lock().unwrap();
    let config = vendored_source_config(
        Path::new("/tmp/vendor"),
        Path::new("/tmp/target/hermetic-release"),
    )
    .unwrap();
    assert!(config.contains("[source.crates-io]"));
    assert!(config.contains("replace-with = \"vendored-sources\""));
    assert!(config.contains("directory = \"/tmp/vendor\""));
    assert!(config.contains("target-dir = \"/tmp/target/hermetic-release\""));
}

#[test]
fn vendor_and_hermetic_release_check_work_for_minimal_project() {
    let _guard = release_test_lock().lock().unwrap();
    let (_sandbox, root) = init_minimal_cargo_project();
    let vendor = run_vendor_at(
        &root,
        &VendorOptions {
            output_dir: PathBuf::from("vendor"),
        },
    )
    .unwrap();
    assert!(vendor.vendor_dir.exists());
    assert_ne!(vendor.vendor_hash, "vendor-missing");

    let hermetic = run_hermetic_release_check_at(
        &root,
        &HermeticReleaseOptions {
            vendor_dir: PathBuf::from("vendor"),
            bin: "mini".to_string(),
            target_triple: None,
        },
    )
    .unwrap();
    assert_eq!(hermetic.bin, "mini");
    assert_eq!(hermetic.vendor_hash, vendor.vendor_hash);
    assert_eq!(hermetic.target_triple, None);
    assert!(hermetic.target_dir.join("release/mini").exists());
}

#[test]
fn hermetic_release_check_supports_musl_target_for_minimal_project() {
    let _guard = release_test_lock().lock().unwrap();
    let (_sandbox, root) = init_minimal_cargo_project();
    run_vendor_at(
        &root,
        &VendorOptions {
            output_dir: PathBuf::from("vendor"),
        },
    )
    .unwrap();

    let hermetic = run_hermetic_release_check_at(
        &root,
        &HermeticReleaseOptions {
            vendor_dir: PathBuf::from("vendor"),
            bin: "mini".to_string(),
            target_triple: Some("x86_64-unknown-linux-musl".to_string()),
        },
    )
    .unwrap();

    assert_eq!(
        hermetic.target_triple.as_deref(),
        Some("x86_64-unknown-linux-musl")
    );
    assert!(
        hermetic
            .target_dir
            .join("x86_64-unknown-linux-musl/release/mini")
            .exists()
    );
}

#[test]
fn install_smoke_verifies_archive_and_creates_current_symlink() {
    let _guard = release_test_lock().lock().unwrap();
    let version = current_release_fixture_version();
    let version_name = current_release_fixture_version_name();
    let (sandbox, repo_root, binary_path) = init_release_repo(version);
    let package = run_package_at(
        &repo_root,
        &PackageOptions {
            binary: binary_path,
            debug_binary: None,
            target_triple: "x86_64-unknown-linux-gnu".to_string(),
            out_dir: PathBuf::from("dist"),
            release_channel: "stable".to_string(),
            maturity_label: "v1beta".to_string(),
            signing_private_key: None,
        },
    )
    .unwrap();
    let install_root = sandbox
        .path()
        .join("install")
        .join("x86_64-unknown-linux-gnu");
    let bin_dir = sandbox.path().join("bin");
    let install = run_install_at(
        &repo_root,
        &InstallOptions {
            control_dir: package.control_dir.clone(),
            install_root: install_root.clone(),
            bin_dir: bin_dir.clone(),
            expected_signing_key_id: None,
            expected_signing_public_key_sha256: None,
            dry_run: false,
        },
    )
    .unwrap();

    assert_eq!(install.installed_version, version);
    assert_eq!(install.previous_version, None);
    assert_eq!(
        current_version_name(&install_root).unwrap().as_deref(),
        Some(version_name.as_str())
    );
    assert_binary_reports_version(&bin_dir.join("gcc-formed"), version).unwrap();
    assert!(
        install_root
            .join(&version_name)
            .join("bin/gcc-formed")
            .exists()
    );
    assert!(launcher_is_managed(&bin_dir.join("gcc-formed"), &install_root).unwrap());
}

#[test]
fn install_dry_run_reports_actions_without_mutating_install_layout() {
    let _guard = release_test_lock().lock().unwrap();
    let version = current_release_fixture_version();
    let version_name = current_release_fixture_version_name();
    let (sandbox, repo_root, binary_path) = init_release_repo(version);
    let package = run_package_at(
        &repo_root,
        &PackageOptions {
            binary: binary_path,
            debug_binary: None,
            target_triple: "x86_64-unknown-linux-gnu".to_string(),
            out_dir: PathBuf::from("dist"),
            release_channel: "stable".to_string(),
            maturity_label: "v1beta".to_string(),
            signing_private_key: None,
        },
    )
    .unwrap();
    let install_root = sandbox
        .path()
        .join("install")
        .join("x86_64-unknown-linux-gnu");
    let bin_dir = sandbox.path().join("bin");

    let install = run_install_at(
        &repo_root,
        &InstallOptions {
            control_dir: package.control_dir,
            install_root: install_root.clone(),
            bin_dir: bin_dir.clone(),
            expected_signing_key_id: None,
            expected_signing_public_key_sha256: None,
            dry_run: true,
        },
    )
    .unwrap();

    assert!(install.dry_run);
    assert_eq!(install.installed_version, version);
    assert_eq!(
        install
            .planned_actions
            .iter()
            .map(|action| action.action.as_str())
            .collect::<Vec<_>>(),
        vec![
            "create_dir",
            "move",
            "swap_symlink",
            "create_dir",
            "swap_symlink",
            "swap_symlink",
        ]
    );
    assert!(!install_root.join(&version_name).exists());
    assert_eq!(current_version_name(&install_root).unwrap(), None);
    assert!(fs::symlink_metadata(bin_dir.join("gcc-formed")).is_err());
}

#[test]
fn install_rejects_control_dir_with_bad_checksums() {
    let _guard = release_test_lock().lock().unwrap();
    let version = current_release_fixture_version();
    let (sandbox, repo_root, binary_path) = init_release_repo(version);
    let package = run_package_at(
        &repo_root,
        &PackageOptions {
            binary: binary_path,
            debug_binary: None,
            target_triple: "x86_64-unknown-linux-gnu".to_string(),
            out_dir: PathBuf::from("dist"),
            release_channel: "stable".to_string(),
            maturity_label: "v1beta".to_string(),
            signing_private_key: None,
        },
    )
    .unwrap();
    write_file(&package.shasums_path, b"deadbeef  broken\n");

    let error = run_install_at(
        &repo_root,
        &InstallOptions {
            control_dir: package.control_dir,
            install_root: sandbox
                .path()
                .join("install")
                .join("x86_64-unknown-linux-gnu"),
            bin_dir: sandbox.path().join("bin"),
            expected_signing_key_id: None,
            expected_signing_public_key_sha256: None,
            dry_run: false,
        },
    )
    .unwrap_err();
    assert!(error.to_string().contains("references missing file"));
}

#[test]
fn signed_package_supports_pinned_signature_verification_and_system_wide_layout() {
    let _guard = release_test_lock().lock().unwrap();
    let version = current_release_fixture_version();
    let version_name = current_release_fixture_version_name();
    let (sandbox, repo_root, binary_path) = init_release_repo(version);
    let signing_private_key = sandbox.path().join("release-signing.key");
    write_signing_private_key(&signing_private_key);
    let package = run_package_at(
        &repo_root,
        &PackageOptions {
            binary: binary_path,
            debug_binary: None,
            target_triple: "x86_64-unknown-linux-musl".to_string(),
            out_dir: PathBuf::from("dist"),
            release_channel: "stable".to_string(),
            maturity_label: "v1beta".to_string(),
            signing_private_key: Some(signing_private_key),
        },
    )
    .unwrap();
    let signature = read_json_file::<DetachedSignatureEnvelope>(
        package
            .shasums_signature_path
            .as_deref()
            .expect("signature path missing"),
    )
    .unwrap();
    let trusted_public_key_sha256 = test_signing_public_key_sha256();
    let system_root = sandbox.path().join("system-root");
    let install_root = system_root
        .join("opt/cc-formed")
        .join("x86_64-unknown-linux-musl");
    let bin_dir = system_root.join("usr/local/bin");

    let install = run_install_at(
        &repo_root,
        &InstallOptions {
            control_dir: package.control_dir,
            install_root: install_root.clone(),
            bin_dir: bin_dir.clone(),
            expected_signing_key_id: Some(signature.key_id.clone()),
            expected_signing_public_key_sha256: Some(trusted_public_key_sha256.clone()),
            dry_run: false,
        },
    )
    .unwrap();

    assert_eq!(
        install.signing_key_id.as_deref(),
        Some(signature.key_id.as_str())
    );
    assert_eq!(
        install.signing_public_key_sha256.as_deref(),
        Some(trusted_public_key_sha256.as_str())
    );
    assert_eq!(install.installed_version, version);
    assert_eq!(
        current_version_name(&install_root).unwrap().as_deref(),
        Some(version_name.as_str())
    );
    assert_binary_reports_version(&bin_dir.join("gcc-formed"), version).unwrap();
    assert!(bin_dir.join("gcc-formed").exists());
    assert!(launcher_is_managed(&bin_dir.join("gcc-formed"), &install_root).unwrap());
}

#[test]
fn install_rejects_signed_release_with_wrong_key_id() {
    let _guard = release_test_lock().lock().unwrap();
    let version = current_release_fixture_version();
    let (sandbox, repo_root, binary_path) = init_release_repo(version);
    let signing_private_key = sandbox.path().join("release-signing.key");
    write_signing_private_key(&signing_private_key);
    let package = run_package_at(
        &repo_root,
        &PackageOptions {
            binary: binary_path,
            debug_binary: None,
            target_triple: "x86_64-unknown-linux-gnu".to_string(),
            out_dir: PathBuf::from("dist"),
            release_channel: "stable".to_string(),
            maturity_label: "v1beta".to_string(),
            signing_private_key: Some(signing_private_key),
        },
    )
    .unwrap();

    let error = run_install_at(
        &repo_root,
        &InstallOptions {
            control_dir: package.control_dir,
            install_root: sandbox
                .path()
                .join("install")
                .join("x86_64-unknown-linux-gnu"),
            bin_dir: sandbox.path().join("bin"),
            expected_signing_key_id: Some("ed25519:deadbeefdeadbeef".to_string()),
            expected_signing_public_key_sha256: None,
            dry_run: false,
        },
    )
    .unwrap_err();

    assert!(
        error
            .to_string()
            .contains("detached signature key mismatch")
    );
}

#[test]
fn install_rejects_signed_release_with_wrong_public_key_sha() {
    let _guard = release_test_lock().lock().unwrap();
    let version = current_release_fixture_version();
    let (sandbox, repo_root, binary_path) = init_release_repo(version);
    let signing_private_key = sandbox.path().join("release-signing.key");
    write_signing_private_key(&signing_private_key);
    let package = run_package_at(
        &repo_root,
        &PackageOptions {
            binary: binary_path,
            debug_binary: None,
            target_triple: "x86_64-unknown-linux-gnu".to_string(),
            out_dir: PathBuf::from("dist"),
            release_channel: "stable".to_string(),
            maturity_label: "v1beta".to_string(),
            signing_private_key: Some(signing_private_key),
        },
    )
    .unwrap();

    let error = run_install_at(
        &repo_root,
        &InstallOptions {
            control_dir: package.control_dir,
            install_root: sandbox
                .path()
                .join("install")
                .join("x86_64-unknown-linux-gnu"),
            bin_dir: sandbox.path().join("bin"),
            expected_signing_key_id: None,
            expected_signing_public_key_sha256: Some("deadbeef".to_string()),
            dry_run: false,
        },
    )
    .unwrap_err();

    assert!(
        error
            .to_string()
            .contains("detached signature public key mismatch")
    );
}

#[test]
fn release_publish_promote_and_resolve_keep_same_bits() {
    let _guard = release_test_lock().lock().unwrap();
    let version = current_release_fixture_version();
    let (sandbox, repo_root, binary_path) = init_release_repo(version);
    let signing_private_key = sandbox.path().join("release-signing.key");
    write_signing_private_key(&signing_private_key);
    let package = run_package_at(
        &repo_root,
        &PackageOptions {
            binary: binary_path,
            debug_binary: None,
            target_triple: "x86_64-unknown-linux-gnu".to_string(),
            out_dir: PathBuf::from("dist"),
            release_channel: "stable".to_string(),
            maturity_label: "v1beta".to_string(),
            signing_private_key: Some(signing_private_key),
        },
    )
    .unwrap();
    let repository_root = sandbox.path().join("release-repo");

    let publish = run_release_publish_at(
        &repo_root,
        &ReleasePublishOptions {
            control_dir: package.control_dir.clone(),
            repository_root: repository_root.clone(),
        },
    )
    .unwrap();
    let canary = run_release_promote_at(
        &repo_root,
        &ReleasePromoteOptions {
            repository_root: repository_root.clone(),
            target_triple: "x86_64-unknown-linux-gnu".to_string(),
            version: version.to_string(),
            channel: "canary".to_string(),
        },
    )
    .unwrap();
    let stable = run_release_promote_at(
        &repo_root,
        &ReleasePromoteOptions {
            repository_root: repository_root.clone(),
            target_triple: "x86_64-unknown-linux-gnu".to_string(),
            version: version.to_string(),
            channel: "stable".to_string(),
        },
    )
    .unwrap();
    let resolved = run_release_resolve_at(
        &repo_root,
        &ReleaseResolveOptions {
            repository_root: repository_root.clone(),
            target_triple: "x86_64-unknown-linux-gnu".to_string(),
            channel: Some("stable".to_string()),
            version: None,
        },
    )
    .unwrap();

    let published = read_published_release(&release_version_root(
        &repository_root,
        "x86_64-unknown-linux-gnu",
        version,
    ))
    .unwrap();
    let stable_pointer =
        read_release_channel_pointer(&repository_root, "x86_64-unknown-linux-gnu", "stable")
            .unwrap();

    assert_eq!(publish.version, version);
    assert!(publish.signing_key_id.is_some());
    assert!(publish.signing_public_key_sha256.is_some());
    assert_eq!(
        canary.primary_archive_sha256,
        publish.primary_archive_sha256
    );
    assert_eq!(
        stable.primary_archive_sha256,
        publish.primary_archive_sha256
    );
    assert_eq!(resolved.resolved_version, version);
    assert_eq!(
        resolved.primary_archive_sha256,
        publish.primary_archive_sha256
    );
    assert_eq!(
        published.primary_archive_sha256,
        publish.primary_archive_sha256
    );
    assert_eq!(stable_pointer.version, version);
    assert_eq!(
        stable_pointer.primary_archive_sha256,
        published.primary_archive_sha256
    );
    assert_eq!(stable_pointer.signing_key_id, publish.signing_key_id);
    assert_eq!(
        stable_pointer.signing_public_key_sha256,
        publish.signing_public_key_sha256
    );
    assert_eq!(resolved.signing_key_id, publish.signing_key_id);
    assert_eq!(
        resolved.signing_public_key_sha256,
        publish.signing_public_key_sha256
    );
    assert!(
        resolved
            .shasums_signature_path
            .as_ref()
            .is_some_and(|path| path.exists())
    );
    assert!(resolved.control_dir.exists());
    assert!(resolved.primary_archive.exists());
}

#[test]
fn install_release_supports_exact_version_and_checksum_pin() {
    let _guard = release_test_lock().lock().unwrap();
    let version = current_release_fixture_version();
    let version_name = current_release_fixture_version_name();
    let (sandbox, repo_root, binary_path) = init_release_repo(version);
    let signing_private_key = sandbox.path().join("release-signing.key");
    write_signing_private_key(&signing_private_key);
    let package = run_package_at(
        &repo_root,
        &PackageOptions {
            binary: binary_path,
            debug_binary: None,
            target_triple: "x86_64-unknown-linux-gnu".to_string(),
            out_dir: PathBuf::from("dist"),
            release_channel: "stable".to_string(),
            maturity_label: "v1beta".to_string(),
            signing_private_key: Some(signing_private_key),
        },
    )
    .unwrap();
    let repository_root = sandbox.path().join("release-repo");
    run_release_publish_at(
        &repo_root,
        &ReleasePublishOptions {
            control_dir: package.control_dir,
            repository_root: repository_root.clone(),
        },
    )
    .unwrap();
    run_release_promote_at(
        &repo_root,
        &ReleasePromoteOptions {
            repository_root: repository_root.clone(),
            target_triple: "x86_64-unknown-linux-gnu".to_string(),
            version: version.to_string(),
            channel: "stable".to_string(),
        },
    )
    .unwrap();
    let resolved = run_release_resolve_at(
        &repo_root,
        &ReleaseResolveOptions {
            repository_root: repository_root.clone(),
            target_triple: "x86_64-unknown-linux-gnu".to_string(),
            channel: Some("stable".to_string()),
            version: None,
        },
    )
    .unwrap();

    let install_root = sandbox
        .path()
        .join("install")
        .join("x86_64-unknown-linux-gnu");
    let bin_dir = sandbox.path().join("bin");
    let install = run_install_release_at(
        &repo_root,
        &InstallReleaseOptions {
            repository_root: repository_root.clone(),
            target_triple: "x86_64-unknown-linux-gnu".to_string(),
            install_root: install_root.clone(),
            bin_dir: bin_dir.clone(),
            channel: None,
            version: Some(version.to_string()),
            expected_primary_sha256: Some(resolved.primary_archive_sha256.clone()),
            expected_signing_key_id: resolved.signing_key_id.clone(),
            expected_signing_public_key_sha256: resolved.signing_public_key_sha256.clone(),
        },
    )
    .unwrap();

    assert_eq!(install.requested_channel, None);
    assert_eq!(install.resolved_version, version);
    assert_eq!(install.installed_version, version);
    assert_eq!(
        install.primary_archive_sha256,
        resolved.primary_archive_sha256
    );
    assert_eq!(install.signing_key_id, resolved.signing_key_id);
    assert_eq!(
        install.signing_public_key_sha256,
        resolved.signing_public_key_sha256
    );
    assert_eq!(
        current_version_name(&install_root).unwrap().as_deref(),
        Some(version_name.as_str())
    );
    assert_binary_reports_version(&bin_dir.join("gcc-formed"), version).unwrap();
}

#[test]
fn install_release_from_channel_reports_exact_installed_version() {
    let _guard = release_test_lock().lock().unwrap();
    let version = current_release_fixture_version();
    let (sandbox, repo_root, binary_path) = init_release_repo(version);
    let signing_private_key = sandbox.path().join("release-signing.key");
    write_signing_private_key(&signing_private_key);
    let package = run_package_at(
        &repo_root,
        &PackageOptions {
            binary: binary_path,
            debug_binary: None,
            target_triple: "x86_64-unknown-linux-gnu".to_string(),
            out_dir: PathBuf::from("dist"),
            release_channel: "stable".to_string(),
            maturity_label: "v1beta".to_string(),
            signing_private_key: Some(signing_private_key),
        },
    )
    .unwrap();
    let repository_root = sandbox.path().join("release-repo");
    run_release_publish_at(
        &repo_root,
        &ReleasePublishOptions {
            control_dir: package.control_dir,
            repository_root: repository_root.clone(),
        },
    )
    .unwrap();
    run_release_promote_at(
        &repo_root,
        &ReleasePromoteOptions {
            repository_root: repository_root.clone(),
            target_triple: "x86_64-unknown-linux-gnu".to_string(),
            version: version.to_string(),
            channel: "stable".to_string(),
        },
    )
    .unwrap();

    let stable_pointer =
        read_release_channel_pointer(&repository_root, "x86_64-unknown-linux-gnu", "stable")
            .unwrap();
    let install = run_install_release_at(
        &repo_root,
        &InstallReleaseOptions {
            repository_root: repository_root.clone(),
            target_triple: "x86_64-unknown-linux-gnu".to_string(),
            install_root: sandbox
                .path()
                .join("channel-install")
                .join("x86_64-unknown-linux-gnu"),
            bin_dir: sandbox.path().join("channel-bin"),
            channel: Some("stable".to_string()),
            version: None,
            expected_primary_sha256: None,
            expected_signing_key_id: stable_pointer.signing_key_id.clone(),
            expected_signing_public_key_sha256: stable_pointer.signing_public_key_sha256.clone(),
        },
    )
    .unwrap();

    assert_eq!(install.requested_channel.as_deref(), Some("stable"));
    assert_eq!(install.resolved_version, version);
    assert_eq!(install.installed_version, version);
}

#[test]
fn install_release_rejects_mismatched_pinned_checksum() {
    let _guard = release_test_lock().lock().unwrap();
    let version = current_release_fixture_version();
    let (sandbox, repo_root, binary_path) = init_release_repo(version);
    let signing_private_key = sandbox.path().join("release-signing.key");
    write_signing_private_key(&signing_private_key);
    let package = run_package_at(
        &repo_root,
        &PackageOptions {
            binary: binary_path,
            debug_binary: None,
            target_triple: "x86_64-unknown-linux-gnu".to_string(),
            out_dir: PathBuf::from("dist"),
            release_channel: "stable".to_string(),
            maturity_label: "v1beta".to_string(),
            signing_private_key: Some(signing_private_key),
        },
    )
    .unwrap();
    let repository_root = sandbox.path().join("release-repo");
    run_release_publish_at(
        &repo_root,
        &ReleasePublishOptions {
            control_dir: package.control_dir,
            repository_root: repository_root.clone(),
        },
    )
    .unwrap();

    let error = run_install_release_at(
        &repo_root,
        &InstallReleaseOptions {
            repository_root,
            target_triple: "x86_64-unknown-linux-gnu".to_string(),
            install_root: sandbox
                .path()
                .join("install")
                .join("x86_64-unknown-linux-gnu"),
            bin_dir: sandbox.path().join("bin"),
            channel: None,
            version: Some(version.to_string()),
            expected_primary_sha256: Some("deadbeef".to_string()),
            expected_signing_key_id: None,
            expected_signing_public_key_sha256: None,
        },
    )
    .unwrap_err();

    assert!(error.to_string().contains("release checksum mismatch"));
}

#[test]
fn stable_release_report_proves_metadata_only_promotion_and_single_symlink_rollback() {
    let _guard = release_test_lock().lock().unwrap();
    let baseline_version = "0.1.0";
    let candidate_version = current_release_fixture_version();

    let (baseline_sandbox, baseline_repo_root, baseline_binary_path) =
        init_release_repo(baseline_version);
    let baseline_signing_private_key = baseline_sandbox.path().join("release-signing.key");
    write_signing_private_key(&baseline_signing_private_key);
    let baseline_package = run_package_at(
        &baseline_repo_root,
        &PackageOptions {
            binary: baseline_binary_path,
            debug_binary: None,
            target_triple: "x86_64-unknown-linux-gnu".to_string(),
            out_dir: PathBuf::from("dist"),
            release_channel: "stable".to_string(),
            maturity_label: "v1beta".to_string(),
            signing_private_key: Some(baseline_signing_private_key.clone()),
        },
    )
    .unwrap();
    rewrite_packaged_fixture_version(
        &baseline_package,
        &baseline_signing_private_key,
        baseline_version,
    );
    let repository_root = baseline_sandbox.path().join("release-repo");
    run_release_publish_at(
        &baseline_repo_root,
        &ReleasePublishOptions {
            control_dir: baseline_package.control_dir,
            repository_root: repository_root.clone(),
        },
    )
    .unwrap();
    run_release_promote_at(
        &baseline_repo_root,
        &ReleasePromoteOptions {
            repository_root: repository_root.clone(),
            target_triple: "x86_64-unknown-linux-gnu".to_string(),
            version: baseline_version.to_string(),
            channel: "stable".to_string(),
        },
    )
    .unwrap();

    let (candidate_sandbox, candidate_repo_root, candidate_binary_path) =
        init_release_repo(candidate_version);
    let candidate_signing_private_key = candidate_sandbox.path().join("release-signing.key");
    write_signing_private_key(&candidate_signing_private_key);
    let candidate_package = run_package_at(
        &candidate_repo_root,
        &PackageOptions {
            binary: candidate_binary_path,
            debug_binary: None,
            target_triple: "x86_64-unknown-linux-gnu".to_string(),
            out_dir: PathBuf::from("dist"),
            release_channel: "stable".to_string(),
            maturity_label: "v1beta".to_string(),
            signing_private_key: Some(candidate_signing_private_key),
        },
    )
    .unwrap();

    let report = run_stable_release_at(
        &candidate_repo_root,
        &StableReleaseOptions {
            control_dir: candidate_package.control_dir,
            repository_root: repository_root.clone(),
            target_triple: "x86_64-unknown-linux-gnu".to_string(),
            install_root: candidate_sandbox
                .path()
                .join("stable-install")
                .join("x86_64-unknown-linux-gnu"),
            bin_dir: candidate_sandbox.path().join("stable-bin"),
            report_dir: candidate_sandbox.path().join("stable-report"),
            rollback_baseline_version: Some(baseline_version.to_string()),
        },
    )
    .unwrap();

    assert_eq!(
        report.previous_stable_version_before_promote.as_deref(),
        Some(baseline_version)
    );
    assert!(report.no_rebuild_evidence.metadata_only_promotion);
    assert_eq!(report.canary.resolve.resolved_version, candidate_version);
    assert_eq!(report.beta.resolve.resolved_version, candidate_version);
    assert_eq!(report.stable.resolve.resolved_version, candidate_version);
    assert_eq!(report.rollback_drill.baseline_version, baseline_version);
    assert_eq!(report.rollback_drill.candidate_version, candidate_version);
    assert_eq!(
        report
            .rollback_drill
            .pre_rollback_current_version
            .as_deref(),
        Some(candidate_version)
    );
    assert_eq!(
        report
            .rollback_drill
            .post_rollback_current_version
            .as_deref(),
        Some(baseline_version)
    );
    assert_eq!(report.rollback_drill.rollback_swap_symlink_count, 1);
    assert!(report.rollback_drill.symlink_only_switch);
    assert_eq!(
        current_version_name(&report.rollback_drill.install_root)
            .unwrap()
            .as_deref(),
        Some("v0.1.0")
    );
    assert!(report.report_path.exists());
    assert!(report.summary_path.exists());
    assert!(
        candidate_sandbox
            .path()
            .join("stable-report/rollback-drill.json")
            .exists()
    );
    assert!(
        candidate_sandbox
            .path()
            .join("stable-report/promotion-evidence.json")
            .exists()
    );
}

#[test]
fn rollback_switches_current_symlink_to_requested_version() {
    let _guard = release_test_lock().lock().unwrap();
    let sandbox = tempfile::tempdir().unwrap();
    let install_root = sandbox
        .path()
        .join("install")
        .join("x86_64-unknown-linux-gnu");
    let bin_dir = sandbox.path().join("bin");
    let v1 = install_root.join("v0.1.0/bin/gcc-formed");
    let v2 = install_root.join("v0.1.1/bin/gcc-formed");
    write_file(&v1, fake_wrapper_script("0.1.0").as_bytes());
    write_file(
        &install_root.join("v0.1.0/bin/g++-formed"),
        fake_wrapper_script("0.1.0").as_bytes(),
    );
    write_file(&v2, fake_wrapper_script("0.1.1").as_bytes());
    write_file(
        &install_root.join("v0.1.1/bin/g++-formed"),
        fake_wrapper_script("0.1.1").as_bytes(),
    );
    make_executable(&v1);
    make_executable(&install_root.join("v0.1.0/bin/g++-formed"));
    make_executable(&v2);
    make_executable(&install_root.join("v0.1.1/bin/g++-formed"));
    ensure_launcher_symlinks(&bin_dir, &install_root).unwrap();
    swap_symlink(&install_root.join("current"), Path::new("v0.1.1"), true).unwrap();

    let rollback = run_rollback_at(
        sandbox.path(),
        &RollbackOptions {
            install_root: install_root.clone(),
            bin_dir: bin_dir.clone(),
            version: "0.1.0".to_string(),
            dry_run: false,
        },
    )
    .unwrap();

    assert_eq!(rollback.active_version, "0.1.0");
    assert_eq!(rollback.planned_actions.len(), 1);
    assert_eq!(rollback.planned_actions[0].action, "swap_symlink");
    assert_eq!(
        rollback.planned_actions[0].path,
        install_root.join("current")
    );
    assert_eq!(
        current_version_name(&install_root).unwrap().as_deref(),
        Some("v0.1.0")
    );
    assert_binary_reports_version(&bin_dir.join("gcc-formed"), "0.1.0").unwrap();
}

#[test]
fn rollback_dry_run_reports_actions_without_switching_current_symlink() {
    let _guard = release_test_lock().lock().unwrap();
    let sandbox = tempfile::tempdir().unwrap();
    let install_root = sandbox
        .path()
        .join("install")
        .join("x86_64-unknown-linux-gnu");
    let bin_dir = sandbox.path().join("bin");
    let v1 = install_root.join("v0.1.0/bin/gcc-formed");
    let v2 = install_root.join("v0.1.1/bin/gcc-formed");
    write_file(&v1, fake_wrapper_script("0.1.0").as_bytes());
    write_file(
        &install_root.join("v0.1.0/bin/g++-formed"),
        fake_wrapper_script("0.1.0").as_bytes(),
    );
    write_file(&v2, fake_wrapper_script("0.1.1").as_bytes());
    write_file(
        &install_root.join("v0.1.1/bin/g++-formed"),
        fake_wrapper_script("0.1.1").as_bytes(),
    );
    make_executable(&v1);
    make_executable(&install_root.join("v0.1.0/bin/g++-formed"));
    make_executable(&v2);
    make_executable(&install_root.join("v0.1.1/bin/g++-formed"));
    ensure_launcher_symlinks(&bin_dir, &install_root).unwrap();
    swap_symlink(&install_root.join("current"), Path::new("v0.1.1"), true).unwrap();

    let rollback = run_rollback_at(
        sandbox.path(),
        &RollbackOptions {
            install_root: install_root.clone(),
            bin_dir: bin_dir.clone(),
            version: "0.1.0".to_string(),
            dry_run: true,
        },
    )
    .unwrap();

    assert!(rollback.dry_run);
    assert_eq!(rollback.active_version, "0.1.0");
    assert_eq!(rollback.planned_actions.len(), 1);
    assert_eq!(rollback.planned_actions[0].action, "swap_symlink");
    assert_eq!(
        rollback.planned_actions[0].path,
        install_root.join("current")
    );
    assert_eq!(
        current_version_name(&install_root).unwrap().as_deref(),
        Some("v0.1.1")
    );
    assert_binary_reports_version(&bin_dir.join("gcc-formed"), "0.1.1").unwrap();
}

#[test]
fn purge_uninstall_removes_install_bits_without_touching_state() {
    let _guard = release_test_lock().lock().unwrap();
    let sandbox = tempfile::tempdir().unwrap();
    let install_root = sandbox
        .path()
        .join("install")
        .join("x86_64-unknown-linux-gnu");
    let bin_dir = sandbox.path().join("bin");
    let state_root = sandbox.path().join("state");
    write_file(
        &install_root.join("v0.1.0/bin/gcc-formed"),
        fake_wrapper_script("0.1.0").as_bytes(),
    );
    write_file(
        &install_root.join("v0.1.0/bin/g++-formed"),
        fake_wrapper_script("0.1.0").as_bytes(),
    );
    make_executable(&install_root.join("v0.1.0/bin/gcc-formed"));
    make_executable(&install_root.join("v0.1.0/bin/g++-formed"));
    ensure_launcher_symlinks(&bin_dir, &install_root).unwrap();
    swap_symlink(&install_root.join("current"), Path::new("v0.1.0"), true).unwrap();
    write_file(&state_root.join("trace.json"), b"keep me\n");

    let uninstall = run_uninstall_at(
        sandbox.path(),
        &UninstallOptions {
            install_root: install_root.clone(),
            bin_dir: bin_dir.clone(),
            mode: UninstallMode::PurgeInstall,
            version: None,
            state_root: Some(state_root.clone()),
            purge_state: false,
            dry_run: false,
        },
    )
    .unwrap();

    assert_eq!(uninstall.removed_versions, vec!["0.1.0".to_string()]);
    assert!(
        uninstall
            .removed_launchers
            .contains(&"gcc-formed".to_string())
    );
    assert!(!install_root.exists());
    assert!(state_root.exists());
}

#[test]
fn purge_uninstall_dry_run_reports_targets_without_removing_files() {
    let _guard = release_test_lock().lock().unwrap();
    let sandbox = tempfile::tempdir().unwrap();
    let install_root = sandbox
        .path()
        .join("install")
        .join("x86_64-unknown-linux-gnu");
    let bin_dir = sandbox.path().join("bin");
    let state_root = sandbox.path().join("state");
    write_file(
        &install_root.join("v0.1.0/bin/gcc-formed"),
        fake_wrapper_script("0.1.0").as_bytes(),
    );
    write_file(
        &install_root.join("v0.1.0/bin/g++-formed"),
        fake_wrapper_script("0.1.0").as_bytes(),
    );
    make_executable(&install_root.join("v0.1.0/bin/gcc-formed"));
    make_executable(&install_root.join("v0.1.0/bin/g++-formed"));
    ensure_launcher_symlinks(&bin_dir, &install_root).unwrap();
    swap_symlink(&install_root.join("current"), Path::new("v0.1.0"), true).unwrap();
    write_file(&state_root.join("trace.json"), b"keep me\n");

    let uninstall = run_uninstall_at(
        sandbox.path(),
        &UninstallOptions {
            install_root: install_root.clone(),
            bin_dir: bin_dir.clone(),
            mode: UninstallMode::PurgeInstall,
            version: None,
            state_root: Some(state_root.clone()),
            purge_state: true,
            dry_run: true,
        },
    )
    .unwrap();

    assert!(uninstall.dry_run);
    assert_eq!(uninstall.removed_versions, vec!["0.1.0".to_string()]);
    assert!(
        uninstall
            .removed_launchers
            .contains(&"gcc-formed".to_string())
    );
    assert!(install_root.exists());
    assert!(state_root.exists());
    assert_eq!(
        current_version_name(&install_root).unwrap().as_deref(),
        Some("v0.1.0")
    );
    assert!(
        uninstall
            .planned_actions
            .iter()
            .any(|action| action.path == state_root)
    );
}
