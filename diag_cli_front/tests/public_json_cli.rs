#![cfg(unix)]

use assert_cmd::Command;
use serde_json::Value;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

#[test]
fn public_json_writes_available_export_to_file() {
    let fixture = public_json_fixture("15.2.0");
    let public_json = fixture.temp.path().join("public.json");

    Command::cargo_bin("gcc-formed")
        .unwrap()
        .env("FORMED_BACKEND_GCC", &fixture.backend)
        .current_dir(fixture.temp.path())
        .arg("--formed-mode=render")
        .arg(format!("--formed-public-json={}", public_json.display()))
        .arg("-c")
        .arg(&fixture.source)
        .assert()
        .failure();

    let export = parse_json_file(&public_json);
    assert_eq!(
        export["kind"].as_str(),
        Some("gcc_formed_public_diagnostic_export")
    );
    assert_eq!(export["status"].as_str(), Some("available"));
    assert!(export["unavailable_reason"].is_null());
    assert_eq!(export["producer"]["name"].as_str(), Some("gcc-formed"));
    assert_eq!(
        export["invocation"]["primary_tool"]["name"].as_str(),
        Some("fake-gcc")
    );
    assert_eq!(export["execution"]["version_band"].as_str(), Some("gcc15"));
    assert_required_execution_fields(
        &export,
        "gcc15",
        "dual_sink_structured",
        "in_scope",
        &["dual_sink_structured", "passthrough"],
    );
    assert!(
        export["result"]["summary"]["diagnostic_count"]
            .as_u64()
            .is_some_and(|value| value >= 1)
    );
}

#[test]
fn public_json_writes_available_export_to_stdout() {
    let fixture = public_json_fixture("15.2.0");

    let assert = Command::cargo_bin("gcc-formed")
        .unwrap()
        .env("FORMED_BACKEND_GCC", &fixture.backend)
        .current_dir(fixture.temp.path())
        .arg("--formed-mode=render")
        .arg("--formed-public-json=stdout")
        .arg("-c")
        .arg(&fixture.source)
        .assert()
        .failure();

    let export: Value = serde_json::from_slice(&assert.get_output().stdout).unwrap();
    assert_eq!(
        export["kind"].as_str(),
        Some("gcc_formed_public_diagnostic_export")
    );
    assert_eq!(export["status"].as_str(), Some("available"));
    assert!(export["unavailable_reason"].is_null());
    assert_eq!(export["producer"]["name"].as_str(), Some("gcc-formed"));
    assert_eq!(
        export["invocation"]["primary_tool"]["name"].as_str(),
        Some("fake-gcc")
    );
    assert_eq!(export["execution"]["version_band"].as_str(), Some("gcc15"));
    assert_required_execution_fields(
        &export,
        "gcc15",
        "dual_sink_structured",
        "in_scope",
        &["dual_sink_structured", "passthrough"],
    );
    assert!(
        export["result"]["summary"]["error_count"]
            .as_u64()
            .is_some_and(|value| value >= 1)
    );
}

#[test]
fn public_json_keeps_required_execution_fields_across_representative_band_paths() {
    let cases = [
        (
            "15.2.0",
            Vec::<&str>::new(),
            "gcc15",
            "dual_sink_structured",
            vec!["dual_sink_structured", "passthrough"],
            Some("structured"),
            Some("none"),
        ),
        (
            "13.2.0",
            Vec::<&str>::new(),
            "gcc13_14",
            "native_text_capture",
            vec![
                "single_sink_structured",
                "native_text_capture",
                "passthrough",
            ],
            Some("residual_text"),
            Some("compatibility"),
        ),
        (
            "13.2.0",
            vec!["--formed-processing-path=single_sink_structured"],
            "gcc13_14",
            "single_sink_structured",
            vec![
                "single_sink_structured",
                "native_text_capture",
                "passthrough",
            ],
            Some("structured"),
            Some("none"),
        ),
        (
            "12.2.0",
            Vec::<&str>::new(),
            "gcc9_12",
            "native_text_capture",
            vec![
                "single_sink_structured",
                "native_text_capture",
                "passthrough",
            ],
            Some("residual_text"),
            Some("compatibility"),
        ),
        (
            "12.2.0",
            vec!["--formed-processing-path=single_sink_structured"],
            "gcc9_12",
            "single_sink_structured",
            vec![
                "single_sink_structured",
                "native_text_capture",
                "passthrough",
            ],
            Some("structured"),
            Some("none"),
        ),
    ];

    for (
        version,
        extra_args,
        expected_band,
        expected_path,
        expected_allowed_paths,
        expected_source_authority,
        expected_fallback_grade,
    ) in cases
    {
        let export = run_public_json_export(version, "render", &extra_args);

        assert_eq!(export["status"].as_str(), Some("available"));
        assert_required_execution_fields(
            &export,
            expected_band,
            expected_path,
            "in_scope",
            &expected_allowed_paths,
        );
        assert_eq!(
            export["execution"]["source_authority"].as_str(),
            expected_source_authority
        );
        assert_eq!(
            export["execution"]["fallback_grade"].as_str(),
            expected_fallback_grade
        );
        assert!(export["execution"]["fallback_reason"].is_null());
    }
}

#[test]
fn public_json_writes_unavailable_export_for_passthrough_mode() {
    let export = run_public_json_export("15.2.0", "passthrough", &[]);
    assert_eq!(
        export["kind"].as_str(),
        Some("gcc_formed_public_diagnostic_export")
    );
    assert_eq!(export["status"].as_str(), Some("unavailable"));
    assert_eq!(
        export["unavailable_reason"].as_str(),
        Some("passthrough_mode")
    );
    assert!(export["result"].is_null());
    assert_required_execution_fields(
        &export,
        "gcc15",
        "passthrough",
        "in_scope",
        &["dual_sink_structured", "passthrough"],
    );
    assert_eq!(export["invocation"]["exit_status"].as_i64(), Some(1));
    assert!(export["execution"]["source_authority"].is_null());
    assert!(export["execution"]["fallback_grade"].is_null());
}

struct PublicJsonFixture {
    temp: TempDir,
    backend: PathBuf,
    source: PathBuf,
}

fn public_json_fixture(version: &str) -> PublicJsonFixture {
    let temp = tempfile::tempdir().unwrap();
    let backend = temp.path().join("fake-gcc");
    let source = temp.path().join("main.c");
    fs::write(&source, "int main(void) { return 0 }\n").unwrap();
    write_executable_script(&backend, &fake_backend_script(version));

    PublicJsonFixture {
        temp,
        backend,
        source,
    }
}

fn fake_backend_script(version: &str) -> String {
    format!(
        r#"#!/usr/bin/env bash
set -euo pipefail
if [[ "${{1:-}}" == "--version" ]]; then
  echo "gcc (Fake) {version}"
  exit 0
fi
structured_path=""
structured_format=""
for arg in "$@"; do
  case "$arg" in
    -fdiagnostics-add-output=sarif:version=2.1,file=*)
      structured_path="${{arg#-fdiagnostics-add-output=sarif:version=2.1,file=}}"
      structured_format="sarif"
      ;;
    -fdiagnostics-format=sarif-file)
      structured_path="source.sarif"
      structured_format="sarif"
      ;;
    -fdiagnostics-format=json-file)
      structured_path="source.json"
      structured_format="json"
      ;;
  esac
done
if [[ -n "$structured_path" && "$structured_format" == "sarif" ]]; then
  cat >"$structured_path" <<'JSON'
{{
  "version":"2.1.0",
  "runs":[
    {{
      "results":[
        {{
          "level":"error",
          "message":{{"text":"expected ';' before '}}' token"}},
          "locations":[
            {{
              "physicalLocation":{{
                "artifactLocation":{{"uri":"main.c"}},
                "region":{{"startLine":4,"startColumn":1}}
              }}
            }}
          ]
        }}
      ]
    }}
  ]
}}
JSON
fi
if [[ -n "$structured_path" && "$structured_format" == "json" ]]; then
  cat >"$structured_path" <<'JSON'
[
  {{
    "kind":"error",
    "message":"expected ';' before '}}' token",
    "locations":[
      {{
        "caret":{{"file":"main.c","line":4,"column":1}}
      }}
    ]
  }}
]
JSON
fi
printf '%s\n' "main.c:4:1: error: expected ';' before '}}' token" >&2
exit 1
"#,
        version = version,
    )
}

fn write_executable_script(path: &Path, script: &str) {
    fs::write(path, script).unwrap();
    let mut permissions = fs::metadata(path).unwrap().permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(path, permissions).unwrap();
}

fn parse_json_file(path: &Path) -> Value {
    serde_json::from_str(&fs::read_to_string(path).unwrap()).unwrap()
}

fn run_public_json_export(version: &str, mode: &str, extra_args: &[&str]) -> Value {
    let fixture = public_json_fixture(version);
    let public_json = fixture.temp.path().join("public.json");
    let mut command = Command::cargo_bin("gcc-formed").unwrap();
    command
        .env("FORMED_BACKEND_GCC", &fixture.backend)
        .current_dir(fixture.temp.path())
        .arg(format!("--formed-mode={mode}"))
        .arg(format!("--formed-public-json={}", public_json.display()));
    for arg in extra_args {
        command.arg(arg);
    }
    command.arg("-c").arg(&fixture.source).assert().failure();
    parse_json_file(&public_json)
}

fn assert_required_execution_fields(
    export: &Value,
    expected_band: &str,
    expected_path: &str,
    expected_support_level: &str,
    expected_allowed_paths: &[&str],
) {
    assert_eq!(
        export["execution"]["version_band"].as_str(),
        Some(expected_band)
    );
    assert_eq!(
        export["execution"]["processing_path"].as_str(),
        Some(expected_path)
    );
    assert_eq!(
        export["execution"]["support_level"].as_str(),
        Some(expected_support_level)
    );
    assert_eq!(
        export["execution"]["allowed_processing_paths"],
        serde_json::json!(expected_allowed_paths)
    );
}
