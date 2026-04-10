use super::*;
use diag_core::FallbackReason;
use diag_testkit::normalize_snapshot_contents;
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

fn acceptance_summary(
    fixture_id: &str,
    expected_family: Option<&str>,
    actual_family: &str,
) -> AcceptanceFixtureSummary {
    AcceptanceFixtureSummary {
        fixture_id: fixture_id.to_string(),
        family_key: "syntax".to_string(),
        title: None,
        support_band: "gcc15_plus".to_string(),
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

    let error = enforce_minimum_corpus_shape(121, &counts).unwrap_err();
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
            support_tier: "gcc15_primary".to_string(),
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
    assert_eq!(manifest.support_tier_declaration, "gcc15_primary");
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
            support_tier: "gcc15_primary".to_string(),
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
            support_tier: "gcc15_primary".to_string(),
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
            support_tier: "gcc15_primary".to_string(),
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
            support_tier: "gcc15_primary".to_string(),
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
            support_tier: "gcc15_primary".to_string(),
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
            support_tier: "gcc15_primary".to_string(),
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
            support_tier: "gcc15_primary".to_string(),
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
            support_tier: "gcc15_primary".to_string(),
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
            support_tier: "gcc15_primary".to_string(),
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
            support_tier: "gcc15_primary".to_string(),
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
            support_tier: "gcc15_primary".to_string(),
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
            support_tier: "gcc15_primary".to_string(),
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
            support_tier: "gcc15_primary".to_string(),
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
            support_tier: "gcc15_primary".to_string(),
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
