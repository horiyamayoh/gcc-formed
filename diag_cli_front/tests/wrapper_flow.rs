use assert_cmd::Command;
use predicates::prelude::*;
use serde_json::Value;
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

#[test]
fn retains_trace_bundle_with_invocation_record_and_decision_log() {
    let temp = fixture("15.2.0");
    let backend = temp.path().join("fake-gcc");
    let source = temp.path().join("main.c");
    let trace_root = temp.path().join("trace-root");
    let state_root = temp.path().join("state-root");
    let runtime_root = temp.path().join("runtime-root");

    Command::cargo_bin("gcc-formed")
        .unwrap()
        .env("FORMED_BACKEND_GCC", &backend)
        .env("FORMED_TRACE_DIR", &trace_root)
        .env("FORMED_STATE_DIR", &state_root)
        .env("FORMED_RUNTIME_DIR", &runtime_root)
        .current_dir(temp.path())
        .arg("--formed-trace=always")
        .arg("-c")
        .arg(&source)
        .assert()
        .failure();

    let trace: Value =
        serde_json::from_str(&fs::read_to_string(trace_root.join("trace.json")).unwrap()).unwrap();
    assert_eq!(trace["selected_mode"], "render");
    assert!(trace["fallback_reason"].is_null());
    assert!(
        trace["decision_log"]
            .as_array()
            .unwrap()
            .iter()
            .any(|entry| entry.as_str() == Some("tier_a_mode=render"))
    );
    assert!(
        trace["artifacts"]
            .as_array()
            .unwrap()
            .iter()
            .any(|artifact| artifact["id"].as_str() == Some("invocation.json"))
    );

    let retained_dir = fs::read_dir(&trace_root)
        .unwrap()
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .find(|path| path.is_dir())
        .unwrap();
    assert!(retained_dir.join("stderr.raw").exists());
    assert!(retained_dir.join("diagnostics.sarif").exists());
    assert!(retained_dir.join("invocation.json").exists());

    let invocation: Value =
        serde_json::from_str(&fs::read_to_string(retained_dir.join("invocation.json")).unwrap())
            .unwrap();
    let expected_backend_path = fs::canonicalize(&backend).unwrap().display().to_string();
    let expected_cwd = temp.path().display().to_string();
    assert_eq!(invocation["selected_mode"], "render");
    assert_eq!(
        invocation["backend_path"].as_str(),
        Some(expected_backend_path.as_str())
    );
    assert_eq!(invocation["cwd"].as_str(), Some(expected_cwd.as_str()));
    assert!(
        invocation["argv"]
            .as_array()
            .unwrap()
            .iter()
            .any(|arg| arg.as_str() == Some("-c"))
    );
    assert!(invocation["argv"].as_array().unwrap().iter().any(|arg| {
        arg.as_str()
            .is_some_and(|arg| arg.starts_with("-fdiagnostics-add-output=sarif:version=2.1,file="))
    }));
}

#[test]
fn self_check_reports_target_aware_paths_and_backend_status() {
    let temp = fixture("15.2.0");
    let backend = temp.path().join("fake-gcc");
    let home = temp.path().join("home");
    let config_home = temp.path().join("xdg-config");
    let cache_home = temp.path().join("xdg-cache");
    let state_home = temp.path().join("xdg-state");
    let runtime_dir = temp.path().join("xdg-runtime");
    fs::create_dir_all(&home).unwrap();

    let assert = Command::cargo_bin("gcc-formed")
        .unwrap()
        .env("HOME", &home)
        .env("XDG_CONFIG_HOME", &config_home)
        .env("XDG_CACHE_HOME", &cache_home)
        .env("XDG_STATE_HOME", &state_home)
        .env("XDG_RUNTIME_DIR", &runtime_dir)
        .env("FORMED_BACKEND_GCC", &backend)
        .env_remove("FORMED_INSTALL_ROOT")
        .env_remove("FORMED_CONFIG_FILE")
        .env_remove("FORMED_CONFIG_DIR")
        .env_remove("FORMED_CACHE_DIR")
        .env_remove("FORMED_STATE_DIR")
        .env_remove("FORMED_RUNTIME_DIR")
        .env_remove("FORMED_TRACE_DIR")
        .current_dir(temp.path())
        .arg("--formed-self-check")
        .assert()
        .success();

    let report: Value = serde_json::from_slice(&assert.get_output().stdout).unwrap();
    let target_triple = report["manifest"]["target_triple"].as_str().unwrap();
    let expected_backend_path = fs::canonicalize(&backend).unwrap().display().to_string();
    let expected_config_path = config_home
        .join("cc-formed/config.toml")
        .display()
        .to_string();

    assert_eq!(report["binary"], "ok");
    assert_eq!(report["manifest"]["target_triple_matches_build"], true);
    assert_eq!(report["paths"]["state_root_access"], "ok");
    assert_eq!(report["paths"]["runtime_root_access"], "ok");
    assert_eq!(report["paths"]["install_root_access"], "ok");
    assert_eq!(report["paths"]["install_root_includes_target_triple"], true);
    assert_eq!(report["paths"]["separated_from_install_root"], true);
    assert_eq!(
        report["paths"]["config_path"].as_str(),
        Some(expected_config_path.as_str())
    );
    assert!(
        report["paths"]["install_root"]
            .as_str()
            .unwrap()
            .ends_with(target_triple)
    );
    assert_eq!(
        report["backend"]["path"].as_str(),
        Some(expected_backend_path.as_str())
    );
    assert_eq!(report["backend"]["support_tier"], "a");
    assert!(report["warnings"].as_array().unwrap().is_empty());
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
