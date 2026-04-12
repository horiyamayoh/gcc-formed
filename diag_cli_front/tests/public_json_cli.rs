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
    assert_eq!(
        export["execution"]["version_band"].as_str(),
        Some("gcc15_plus")
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
    assert_eq!(
        export["execution"]["version_band"].as_str(),
        Some("gcc15_plus")
    );
    assert!(
        export["result"]["summary"]["error_count"]
            .as_u64()
            .is_some_and(|value| value >= 1)
    );
}

#[test]
fn public_json_writes_unavailable_export_for_passthrough_mode() {
    let fixture = public_json_fixture("15.2.0");
    let public_json = fixture.temp.path().join("public.json");

    Command::cargo_bin("gcc-formed")
        .unwrap()
        .env("FORMED_BACKEND_GCC", &fixture.backend)
        .current_dir(fixture.temp.path())
        .arg("--formed-mode=passthrough")
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
    assert_eq!(export["status"].as_str(), Some("unavailable"));
    assert_eq!(
        export["unavailable_reason"].as_str(),
        Some("passthrough_mode")
    );
    assert!(export["result"].is_null());
    assert_eq!(
        export["execution"]["processing_path"].as_str(),
        Some("passthrough")
    );
    assert_eq!(export["invocation"]["exit_status"].as_i64(), Some(1));
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
sarif=""
for arg in "$@"; do
  case "$arg" in
    -fdiagnostics-add-output=sarif:version=2.1,file=*)
      sarif="${{arg#-fdiagnostics-add-output=sarif:version=2.1,file=}}"
      ;;
    -fdiagnostics-format=sarif-file)
      sarif="source.sarif"
      ;;
    -fdiagnostics-format=json-file)
      sarif="source.json"
      ;;
  esac
done
if [[ -n "$sarif" ]]; then
  cat >"$sarif" <<'JSON'
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
