use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use tempfile::TempDir;

#[test]
fn renders_with_fake_gcc15_backend() {
    let temp = fixture("15.2.0");
    let backend = temp.path().join("fake-gcc");
    let source = temp.path().join("main.c");

    Command::cargo_bin("gcc-formed")
        .unwrap()
        .env("FORMED_BACKEND_GCC", &backend)
        .current_dir(temp.path())
        .arg("-c")
        .arg(&source)
        .assert()
        .failure()
        .stderr(predicate::str::contains("error: syntax error"))
        .stderr(predicate::str::contains("help: fix the first parser error"));
}

#[test]
fn falls_back_to_passthrough_with_fake_gcc13_backend() {
    let temp = fixture("13.3.0");
    let backend = temp.path().join("fake-gcc");
    let source = temp.path().join("main.c");

    Command::cargo_bin("gcc-formed")
        .unwrap()
        .env("FORMED_BACKEND_GCC", &backend)
        .current_dir(temp.path())
        .arg("-c")
        .arg(&source)
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "main.c:4:1: error: expected ';' before '}' token",
        ))
        .stderr(predicate::str::contains("help:").not());
}

fn fixture(version: &str) -> TempDir {
    let temp = tempfile::tempdir().unwrap();
    fs::write(temp.path().join("main.c"), "int main(void) { return 0 }\n").unwrap();
    let script = format!(
        r#"#!/usr/bin/env bash
set -euo pipefail
if [[ "${{1:-}}" == "--version" ]]; then
  echo "gcc (Fake) {version}"
  exit 0
fi
sarif=""
for arg in "$@"; do
  if [[ "$arg" == -fdiagnostics-add-output=sarif:version=2.1,file=* ]]; then
    sarif="${{arg#-fdiagnostics-add-output=sarif:version=2.1,file=}}"
  fi
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
echo "main.c:4:1: error: expected ';' before '}}' token" >&2
exit 1
"#
    );
    let backend = temp.path().join("fake-gcc");
    fs::write(&backend, script).unwrap();
    let mut permissions = fs::metadata(&backend).unwrap().permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&backend, permissions).unwrap();
    temp
}
