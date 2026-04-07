use assert_cmd::Command;
use predicates::prelude::*;
use serde_json::Value;
use std::collections::BTreeMap;
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
    assert_eq!(trace["capabilities"]["stream_kind"], "pipe");
    assert_eq!(trace["capabilities"]["ansi_color"], false);
    assert!(trace["timing"]["capture_ms"].as_u64().is_some());
    assert!(trace["timing"]["render_ms"].as_u64().is_some());
    assert!(trace["timing"]["total_ms"].as_u64().is_some());
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
    assert!(
        trace["artifacts"]
            .as_array()
            .unwrap()
            .iter()
            .any(|artifact| artifact["id"].as_str() == Some("trace.json"))
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
    assert!(retained_dir.join("trace.json").exists());

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

    let retained_trace: Value =
        serde_json::from_str(&fs::read_to_string(retained_dir.join("trace.json")).unwrap())
            .unwrap();
    assert_eq!(retained_trace["selected_mode"], "render");
    assert_eq!(retained_trace["capabilities"]["stream_kind"], "pipe");
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

#[test]
fn render_mode_sanitizes_child_diagnostic_environment() {
    let temp = fixture("15.2.0");
    let backend = temp.path().join("fake-gcc");
    let source = temp.path().join("main.c");
    let trace_root = temp.path().join("trace-root");
    let runtime_root = temp.path().join("runtime-root");
    let env_dump = temp.path().join("child-env.txt");

    Command::cargo_bin("gcc-formed")
        .unwrap()
        .env("FORMED_BACKEND_GCC", &backend)
        .env("FORMED_TRACE_DIR", &trace_root)
        .env("FORMED_RUNTIME_DIR", &runtime_root)
        .env("FORMED_TEST_ENV_DUMP", &env_dump)
        .env("LC_ALL", "ja_JP.UTF-8")
        .env("LC_MESSAGES", "ja_JP.UTF-8")
        .env("LC_CTYPE", "en_US.UTF-8")
        .env("GCC_DIAGNOSTICS_LOG", "/tmp/diag.log")
        .env("GCC_EXTRA_DIAGNOSTIC_OUTPUT", "fixits")
        .env("EXPERIMENTAL_SARIF_SOCKET", "/tmp/sarif.sock")
        .current_dir(temp.path())
        .arg("--formed-trace=always")
        .arg("-c")
        .arg(&source)
        .assert()
        .failure();

    let env_dump = parse_env_dump(&fs::read_to_string(&env_dump).unwrap());
    assert_eq!(
        env_dump.get("LC_ALL").map(String::as_str),
        Some("ja_JP.UTF-8")
    );
    assert_eq!(env_dump.get("LC_MESSAGES").map(String::as_str), Some("C"));
    assert_eq!(
        env_dump.get("LC_CTYPE").map(String::as_str),
        Some("en_US.UTF-8")
    );
    assert_eq!(
        env_dump.get("GCC_DIAGNOSTICS_LOG").map(String::as_str),
        Some("__unset__")
    );
    assert_eq!(
        env_dump
            .get("GCC_EXTRA_DIAGNOSTIC_OUTPUT")
            .map(String::as_str),
        Some("__unset__")
    );
    assert_eq!(
        env_dump
            .get("EXPERIMENTAL_SARIF_SOCKET")
            .map(String::as_str),
        Some("__unset__")
    );

    let retained_dir = fs::read_dir(&trace_root)
        .unwrap()
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .find(|path| path.is_dir())
        .unwrap();
    let invocation: Value =
        serde_json::from_str(&fs::read_to_string(retained_dir.join("invocation.json")).unwrap())
            .unwrap();
    assert_eq!(
        invocation["child_env_policy"]["set"]["LC_MESSAGES"].as_str(),
        Some("C")
    );
    assert!(
        invocation["child_env_policy"]["unset"]
            .as_array()
            .unwrap()
            .iter()
            .any(|value| value.as_str() == Some("GCC_DIAGNOSTICS_LOG"))
    );
}

fn parse_env_dump(contents: &str) -> BTreeMap<String, String> {
    contents
        .lines()
        .filter_map(|line| line.split_once('='))
        .map(|(key, value)| (key.to_string(), value.to_string()))
        .collect()
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
if [[ -n "${{FORMED_TEST_ENV_DUMP:-}}" ]]; then
  {{
    printf 'LC_ALL=%s\n' "${{LC_ALL-__unset__}}"
    printf 'LC_MESSAGES=%s\n' "${{LC_MESSAGES-__unset__}}"
    printf 'LC_CTYPE=%s\n' "${{LC_CTYPE-__unset__}}"
    printf 'GCC_DIAGNOSTICS_LOG=%s\n' "${{GCC_DIAGNOSTICS_LOG-__unset__}}"
    printf 'GCC_EXTRA_DIAGNOSTIC_OUTPUT=%s\n' "${{GCC_EXTRA_DIAGNOSTIC_OUTPUT-__unset__}}"
    printf 'EXPERIMENTAL_SARIF_SOCKET=%s\n' "${{EXPERIMENTAL_SARIF_SOCKET-__unset__}}"
  }} >"${{FORMED_TEST_ENV_DUMP}}"
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
