use assert_cmd::Command;
use predicates::prelude::*;
use serde_json::Value;
use std::collections::BTreeMap;
use std::env;
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
fn renders_with_fake_gcc13_backend_on_native_text_default_path() {
    let temp = fixture("13.3.0");
    let backend = temp.path().join("fake-gcc");
    let source = temp.path().join("main.c");
    let trace_root = temp.path().join("trace-root");

    Command::cargo_bin("gcc-formed")
        .unwrap()
        .env("FORMED_BACKEND_GCC", &backend)
        .env("FORMED_TRACE_DIR", &trace_root)
        .current_dir(temp.path())
        .arg("--formed-trace=always")
        .arg("-c")
        .arg(&source)
        .assert()
        .failure()
        .stderr(predicate::str::contains(expected_tier_b_native_text_notice()))
        .stderr(predicate::str::contains(
            "note: some compiler details were not fully structured; original diagnostics are preserved",
        ))
        .stderr(predicate::str::contains(
            "error: expected ';' before '}' token",
        ))
        .stderr(predicate::str::contains(
            "help: fix the first parser error at the user-owned location",
        ))
        .stderr(predicate::str::contains(
            "why: main.c:4:1: error: expected ';' before '}' token",
        ))
        .stderr(predicate::str::contains("showing a conservative wrapper view").not());

    let trace: Value =
        serde_json::from_str(&fs::read_to_string(trace_root.join("trace.json")).unwrap()).unwrap();
    assert_eq!(trace["selected_mode"], "render");
    assert_eq!(trace["wrapper_verdict"], "render_fallback");
    assert!(trace["fallback_reason"].is_null());
    assert_eq!(trace["environment_summary"]["version_band"], "gcc13_14");
    assert_eq!(
        trace["environment_summary"]["processing_path"],
        "native_text_capture"
    );
    assert_eq!(
        trace["parser_result_summary"]["status"].as_str(),
        Some("fallback")
    );
    assert_eq!(
        trace["parser_result_summary"]["document_completeness"].as_str(),
        Some("partial")
    );
    assert!(
        trace["decision_log"]
            .as_array()
            .unwrap()
            .iter()
            .any(|entry| entry.as_str() == Some("ingest_source_authority=residual_text"))
    );
    assert!(
        trace["decision_log"]
            .as_array()
            .unwrap()
            .iter()
            .any(|entry| entry.as_str() == Some("ingest_fallback_grade=compatibility"))
    );
}

#[test]
fn renders_with_explicit_single_sink_structured_on_fake_gcc13_backend() {
    let temp = fixture("13.3.0");
    let backend = temp.path().join("fake-gcc");
    let source = temp.path().join("main.c");
    let trace_root = temp.path().join("trace-root");

    Command::cargo_bin("gcc-formed")
        .unwrap()
        .env("FORMED_BACKEND_GCC", &backend)
        .env("FORMED_TRACE_DIR", &trace_root)
        .current_dir(temp.path())
        .arg("--formed-trace=always")
        .arg("--formed-processing-path=single_sink_structured")
        .arg("-c")
        .arg(&source)
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            expected_tier_b_single_sink_notice(),
        ))
        .stderr(predicate::str::contains("error: syntax error"))
        .stderr(predicate::str::contains(
            "help: fix the first parser error at the user-owned location",
        ))
        .stderr(predicate::str::contains("showing a conservative wrapper view").not());

    let trace: Value =
        serde_json::from_str(&fs::read_to_string(trace_root.join("trace.json")).unwrap()).unwrap();
    assert_eq!(trace["selected_mode"], "render");
    assert_eq!(trace["wrapper_verdict"], "rendered");
    assert!(trace["fallback_reason"].is_null());
    assert_eq!(
        trace["environment_summary"]["processing_path"],
        "single_sink_structured"
    );
    assert!(
        trace["environment_summary"]["injected_flags"]
            .as_array()
            .unwrap()
            .iter()
            .any(|flag| flag.as_str() == Some("-fdiagnostics-format=sarif-file"))
    );
    assert_eq!(
        trace["parser_result_summary"]["status"].as_str(),
        Some("parsed")
    );
    assert!(
        trace["decision_log"]
            .as_array()
            .unwrap()
            .iter()
            .any(|entry| entry.as_str() == Some("ingest_source_authority=structured"))
    );
    assert!(
        trace["decision_log"]
            .as_array()
            .unwrap()
            .iter()
            .any(|entry| entry.as_str() == Some("ingest_fallback_grade=none"))
    );
    assert!(!temp.path().join("source.sarif").exists());
}

#[test]
fn shadows_with_fake_gcc13_backend_and_honest_notice() {
    let temp = fixture("13.3.0");
    let backend = temp.path().join("fake-gcc");
    let source = temp.path().join("main.c");
    let trace_root = temp.path().join("trace-root");

    Command::cargo_bin("gcc-formed")
        .unwrap()
        .env("FORMED_BACKEND_GCC", &backend)
        .env("FORMED_TRACE_DIR", &trace_root)
        .current_dir(temp.path())
        .arg("--formed-trace=always")
        .arg("--formed-mode=shadow")
        .arg("-c")
        .arg(&source)
        .assert()
        .failure()
        .stderr(predicate::str::contains(expected_tier_b_shadow_notice()))
        .stderr(predicate::str::contains(
            "main.c:4:1: error: expected ';' before '}' token",
        ))
        .stderr(predicate::str::contains("error: syntax error").not());

    let trace: Value =
        serde_json::from_str(&fs::read_to_string(trace_root.join("trace.json")).unwrap()).unwrap();
    assert_eq!(trace["selected_mode"], "shadow");
    assert_eq!(trace["support_tier"], "b");
    assert_eq!(trace["wrapper_verdict"], "shadow_observed");
    assert_eq!(trace["environment_summary"]["version_band"], "gcc13_14");
    assert_eq!(
        trace["environment_summary"]["processing_path"],
        "native_text_capture"
    );
    assert_eq!(
        trace["environment_summary"]["support_level"],
        "experimental"
    );
    assert_eq!(trace["fallback_reason"], "shadow_mode");
    assert_eq!(
        trace["parser_result_summary"]["status"].as_str(),
        Some("fallback")
    );
    assert!(
        trace["decision_log"]
            .as_array()
            .unwrap()
            .iter()
            .any(|entry| entry.as_str() == Some("ingest_source_authority=residual_text"))
    );
    assert!(
        trace["decision_log"]
            .as_array()
            .unwrap()
            .iter()
            .any(|entry| entry.as_str() == Some("ingest_fallback_grade=compatibility"))
    );
}

#[test]
fn renders_with_fake_gcc12_backend_on_native_text_default_path() {
    let temp = fixture("12.2.0");
    let backend = temp.path().join("fake-gcc");
    let source = temp.path().join("main.c");
    let trace_root = temp.path().join("trace-root");

    Command::cargo_bin("gcc-formed")
        .unwrap()
        .env("FORMED_BACKEND_GCC", &backend)
        .env("FORMED_TRACE_DIR", &trace_root)
        .current_dir(temp.path())
        .arg("--formed-trace=always")
        .arg("-c")
        .arg(&source)
        .assert()
        .failure()
        .stderr(predicate::str::contains(expected_tier_c_native_text_notice()))
        .stderr(predicate::str::contains(
            "note: some compiler details were not fully structured; original diagnostics are preserved",
        ))
        .stderr(predicate::str::contains(
            "main.c:4:1: error: expected ';' before '}' token",
        ))
        .stderr(predicate::str::contains(
            "help: fix the first parser error at the user-owned location",
        ))
        .stderr(predicate::str::contains("showing a conservative wrapper view").not());

    let trace: Value =
        serde_json::from_str(&fs::read_to_string(trace_root.join("trace.json")).unwrap()).unwrap();
    assert_eq!(trace["selected_mode"], "render");
    assert_eq!(trace["support_tier"], "c");
    assert_eq!(trace["wrapper_verdict"], "render_fallback");
    assert_eq!(trace["environment_summary"]["version_band"], "gcc9_12");
    assert_eq!(
        trace["environment_summary"]["processing_path"],
        "native_text_capture"
    );
    assert_eq!(
        trace["environment_summary"]["support_level"],
        "experimental"
    );
    assert!(trace["fallback_reason"].is_null());
    assert_eq!(
        trace["parser_result_summary"]["status"].as_str(),
        Some("fallback")
    );
    assert_eq!(
        trace["parser_result_summary"]["document_completeness"].as_str(),
        Some("partial")
    );
    assert!(
        trace["decision_log"]
            .as_array()
            .unwrap()
            .iter()
            .any(|entry| entry.as_str() == Some("ingest_source_authority=residual_text"))
    );
    assert!(
        trace["decision_log"]
            .as_array()
            .unwrap()
            .iter()
            .any(|entry| entry.as_str() == Some("ingest_fallback_grade=compatibility"))
    );
}

#[test]
fn renders_with_explicit_single_sink_structured_json_on_fake_gcc12_backend() {
    let temp = fixture("12.2.0");
    let backend = temp.path().join("fake-gcc");
    let source = temp.path().join("main.c");
    let trace_root = temp.path().join("trace-root");

    Command::cargo_bin("gcc-formed")
        .unwrap()
        .env("FORMED_BACKEND_GCC", &backend)
        .env("FORMED_TRACE_DIR", &trace_root)
        .current_dir(temp.path())
        .arg("--formed-trace=always")
        .arg("--formed-processing-path=single_sink_structured")
        .arg("-c")
        .arg(&source)
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            expected_tier_c_single_sink_notice(),
        ))
        .stderr(predicate::str::contains("error: syntax error"))
        .stderr(predicate::str::contains(
            "help: fix the first parser error at the user-owned location",
        ))
        .stderr(predicate::str::contains("showing a conservative wrapper view").not());

    let trace: Value =
        serde_json::from_str(&fs::read_to_string(trace_root.join("trace.json")).unwrap()).unwrap();
    assert_eq!(trace["selected_mode"], "render");
    assert_eq!(trace["support_tier"], "c");
    assert_eq!(trace["wrapper_verdict"], "rendered");
    assert!(trace["fallback_reason"].is_null());
    assert_eq!(trace["environment_summary"]["version_band"], "gcc9_12");
    assert_eq!(
        trace["environment_summary"]["processing_path"],
        "single_sink_structured"
    );
    assert!(
        trace["environment_summary"]["injected_flags"]
            .as_array()
            .unwrap()
            .iter()
            .any(|flag| flag.as_str() == Some("-fdiagnostics-format=json-file"))
    );
    assert!(
        trace["environment_summary"]["temp_artifact_paths"]
            .as_array()
            .unwrap()
            .iter()
            .any(|path| path
                .as_str()
                .is_some_and(|path| path.ends_with("/diagnostics.json")))
    );
    assert_eq!(
        trace["parser_result_summary"]["status"].as_str(),
        Some("parsed")
    );
    assert!(
        trace["decision_log"]
            .as_array()
            .unwrap()
            .iter()
            .any(|entry| entry.as_str() == Some("ingest_source_authority=structured"))
    );
    assert!(
        trace["decision_log"]
            .as_array()
            .unwrap()
            .iter()
            .any(|entry| entry.as_str() == Some("ingest_fallback_grade=none"))
    );
    assert!(!temp.path().join("source.json").exists());
}

#[test]
fn missing_single_sink_json_falls_back_honestly_on_fake_gcc12_backend() {
    let temp = fixture_with_sarif_mode("12.2.0", "missing");
    let backend = temp.path().join("fake-gcc");
    let source = temp.path().join("main.c");
    let trace_root = temp.path().join("trace-root");

    Command::cargo_bin("gcc-formed")
        .unwrap()
        .env("FORMED_BACKEND_GCC", &backend)
        .env("FORMED_TRACE_DIR", &trace_root)
        .current_dir(temp.path())
        .arg("--formed-trace=always")
        .arg("--formed-processing-path=single_sink_structured")
        .arg("-c")
        .arg(&source)
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            expected_tier_c_single_sink_notice(),
        ))
        .stderr(predicate::str::contains(
            "note: some compiler details were not fully structured; original diagnostics are preserved",
        ))
        .stderr(predicate::str::contains(
            "main.c:4:1: error: expected ';' before '}' token",
        ));

    let trace: Value =
        serde_json::from_str(&fs::read_to_string(trace_root.join("trace.json")).unwrap()).unwrap();
    assert_eq!(trace["selected_mode"], "render");
    assert_eq!(trace["wrapper_verdict"], "render_fallback");
    assert_eq!(
        trace["environment_summary"]["processing_path"],
        "single_sink_structured"
    );
    assert!(
        trace["environment_summary"]["injected_flags"]
            .as_array()
            .unwrap()
            .iter()
            .any(|flag| flag.as_str() == Some("-fdiagnostics-format=json-file"))
    );
    assert_eq!(
        trace["parser_result_summary"]["status"].as_str(),
        Some("fallback")
    );
    assert!(
        trace["decision_log"]
            .as_array()
            .unwrap()
            .iter()
            .any(|entry| entry.as_str() == Some("ingest_source_authority=residual_text"))
    );
    assert!(
        trace["decision_log"]
            .as_array()
            .unwrap()
            .iter()
            .any(|entry| entry.as_str() == Some("ingest_fallback_grade=fail_open"))
    );
    assert!(
        trace["warning_messages"]
            .as_array()
            .unwrap()
            .iter()
            .any(|message| message.as_str().is_some_and(
                |message| message.contains("expected structured GCC JSON was not produced")
            ))
    );
}

#[test]
fn renders_with_fake_gcc12_type_overload_useful_subset() {
    let temp = fixture_with_stderr(
        "12.2.0",
        "\
main.cpp:5:7: error: no matching function for call to 'takes(int)'\n\
main.cpp:2:6: note: candidate 1: 'void takes(int, int)'\n",
    );
    let backend = temp.path().join("fake-gcc");
    let source = temp.path().join("main.cpp");
    let trace_root = temp.path().join("trace-root");

    Command::cargo_bin("gcc-formed")
        .unwrap()
        .env("FORMED_BACKEND_GCC", &backend)
        .env("FORMED_TRACE_DIR", &trace_root)
        .current_dir(temp.path())
        .arg("--formed-trace=always")
        .arg("-c")
        .arg(&source)
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            expected_tier_c_native_text_notice(),
        ))
        .stderr(predicate::str::contains("type or overload mismatch"))
        .stderr(predicate::str::contains(
            "compare the expected type and actual argument at the call site",
        ))
        .stderr(predicate::str::contains("showing a conservative wrapper view").not());

    let trace: Value =
        serde_json::from_str(&fs::read_to_string(trace_root.join("trace.json")).unwrap()).unwrap();
    assert_eq!(trace["selected_mode"], "render");
    assert_eq!(trace["wrapper_verdict"], "render_fallback");
    assert_eq!(trace["environment_summary"]["version_band"], "gcc9_12");
    assert_eq!(
        trace["environment_summary"]["processing_path"],
        "native_text_capture"
    );
    assert!(trace["fallback_reason"].is_null());
    assert_eq!(
        trace["parser_result_summary"]["status"].as_str(),
        Some("fallback")
    );
    assert_eq!(
        trace["parser_result_summary"]["document_completeness"].as_str(),
        Some("partial")
    );
    assert!(
        trace["decision_log"]
            .as_array()
            .unwrap()
            .iter()
            .any(|entry| entry.as_str() == Some("ingest_source_authority=residual_text"))
    );
    assert!(
        trace["decision_log"]
            .as_array()
            .unwrap()
            .iter()
            .any(|entry| entry.as_str() == Some("ingest_fallback_grade=compatibility"))
    );
}

#[test]
fn fails_open_with_fake_gcc12_opaque_native_text_residual() {
    let temp = fixture_with_stderr(
        "12.2.0",
        "\
main.c:4:1: error: opaque compiler wording here\n\
main.c:4:1: note: extra opaque detail\n",
    );
    let backend = temp.path().join("fake-gcc");
    let source = temp.path().join("main.c");
    let trace_root = temp.path().join("trace-root");

    Command::cargo_bin("gcc-formed")
        .unwrap()
        .env("FORMED_BACKEND_GCC", &backend)
        .env("FORMED_TRACE_DIR", &trace_root)
        .current_dir(temp.path())
        .arg("--formed-trace=always")
        .arg("-c")
        .arg(&source)
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            expected_tier_c_native_text_notice(),
        ))
        .stderr(predicate::str::contains(
            "error: showing a conservative wrapper view",
        ))
        .stderr(predicate::str::contains("opaque compiler wording here"));

    let trace: Value =
        serde_json::from_str(&fs::read_to_string(trace_root.join("trace.json")).unwrap()).unwrap();
    assert_eq!(trace["selected_mode"], "render");
    assert_eq!(trace["wrapper_verdict"], "render_fallback");
    assert_eq!(trace["environment_summary"]["version_band"], "gcc9_12");
    assert_eq!(trace["fallback_reason"], "residual_only");
    assert_eq!(
        trace["parser_result_summary"]["status"].as_str(),
        Some("fallback")
    );
    assert_eq!(
        trace["parser_result_summary"]["document_completeness"].as_str(),
        Some("passthrough")
    );
    assert!(
        trace["decision_log"]
            .as_array()
            .unwrap()
            .iter()
            .any(|entry| entry.as_str() == Some("ingest_source_authority=residual_text"))
    );
    assert!(
        trace["decision_log"]
            .as_array()
            .unwrap()
            .iter()
            .any(|entry| entry.as_str() == Some("ingest_fallback_grade=fail_open"))
    );
}

#[cfg(unix)]
#[test]
fn compiler_introspection_passthrough_preserves_signal_exit_code() {
    let temp = tempfile::tempdir().unwrap();
    let backend = temp.path().join("fake-gcc");
    fs::write(
        &backend,
        r#"#!/usr/bin/env bash
set -euo pipefail
count_file="$(dirname "$0")/version-count"
if [[ "${1:-}" == "--version" ]]; then
  if [[ ! -f "$count_file" ]]; then
    echo "gcc (Fake) 15.2.0"
    : >"$count_file"
    exit 0
  fi
  kill -s TERM $$
fi
exit 0
"#,
    )
    .unwrap();
    let mut permissions = fs::metadata(&backend).unwrap().permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&backend, permissions).unwrap();

    Command::cargo_bin("gcc-formed")
        .unwrap()
        .env("FORMED_BACKEND_GCC", &backend)
        .current_dir(temp.path())
        .arg("--version")
        .assert()
        .failure()
        .code(143);
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
    assert_eq!(trace["wrapper_verdict"], "rendered");
    assert!(trace["fallback_reason"].is_null());
    assert_eq!(
        trace["version_summary"]["wrapper_version"].as_str(),
        Some(env!("CARGO_PKG_VERSION"))
    );
    assert_eq!(
        trace["version_summary"]["adapter_spec_version"].as_str(),
        Some(diag_core::ADAPTER_SPEC_VERSION)
    );
    assert_eq!(
        trace["environment_summary"]["backend_version"].as_str(),
        Some("gcc (Fake) 15.2.0")
    );
    assert_eq!(trace["environment_summary"]["version_band"], "gcc15_plus");
    assert_eq!(
        trace["environment_summary"]["processing_path"],
        "dual_sink_structured"
    );
    assert_eq!(trace["environment_summary"]["support_level"], "preview");
    assert!(
        trace["environment_summary"]["injected_flags"]
            .as_array()
            .unwrap()
            .iter()
            .any(|flag| flag.as_str().is_some_and(
                |flag| flag.starts_with("-fdiagnostics-add-output=sarif:version=2.1,file=")
            ))
    );
    assert!(
        !trace["environment_summary"]["injected_flags"]
            .as_array()
            .unwrap()
            .iter()
            .any(|flag| flag.as_str() == Some("-fdiagnostics-color=always"))
    );
    assert!(
        trace["environment_summary"]["sanitized_env_keys"]
            .as_array()
            .unwrap()
            .iter()
            .any(|key| key.as_str() == Some("LC_MESSAGES"))
    );
    assert!(
        trace["environment_summary"]["temp_artifact_paths"]
            .as_array()
            .unwrap()
            .iter()
            .any(|path| path
                .as_str()
                .is_some_and(|path| path.contains("runtime-root/formed-")))
    );
    assert_eq!(
        trace["capabilities"]["stream_kind"],
        expected_non_tty_stream_kind()
    );
    assert_eq!(trace["capabilities"]["ansi_color"], false);
    assert!(trace["timing"]["capture_ms"].as_u64().is_some());
    assert!(trace["timing"]["render_ms"].as_u64().is_some());
    assert!(trace["timing"]["total_ms"].as_u64().is_some());
    assert_eq!(trace["child_exit"]["code"].as_i64(), Some(1));
    assert!(trace["child_exit"]["signal"].is_null());
    assert_eq!(trace["child_exit"]["success"].as_bool(), Some(false));
    assert_eq!(
        trace["fingerprint_summary"]["raw"].as_str().map(str::len),
        Some(64)
    );
    assert_eq!(
        trace["fingerprint_summary"]["normalized"]
            .as_str()
            .map(str::len),
        Some(64)
    );
    assert_eq!(
        trace["fingerprint_summary"]["family"]
            .as_str()
            .map(str::len),
        Some(64)
    );
    assert_eq!(trace["redaction_status"]["class"], "restricted");
    assert_eq!(
        trace["redaction_status"]["local_only"].as_bool(),
        Some(true)
    );
    assert!(
        trace["redaction_status"]["normalized_artifacts"]
            .as_array()
            .unwrap()
            .iter()
            .any(|artifact| artifact.as_str() == Some("ir.analysis.json"))
    );
    assert_eq!(
        trace["parser_result_summary"]["status"].as_str(),
        Some("parsed")
    );
    assert_eq!(
        trace["parser_result_summary"]["document_completeness"].as_str(),
        Some("complete")
    );
    assert!(
        trace["parser_result_summary"]["diagnostic_count"]
            .as_u64()
            .unwrap()
            >= 1
    );
    assert_eq!(
        trace["parser_result_summary"]["capture_count"].as_u64(),
        Some(2)
    );
    assert!(
        trace["decision_log"]
            .as_array()
            .unwrap()
            .iter()
            .any(|entry| entry.as_str() == Some("selected_mode=render"))
    );
    assert!(
        trace["decision_log"]
            .as_array()
            .unwrap()
            .iter()
            .any(|entry| entry.as_str() == Some("ingest_source_authority=structured"))
    );
    assert!(
        trace["decision_log"]
            .as_array()
            .unwrap()
            .iter()
            .any(|entry| entry.as_str() == Some("ingest_fallback_grade=none"))
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
            .any(|artifact| artifact["id"].as_str() == Some("ir.analysis.json"))
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
    assert!(retained_dir.join("ir.analysis.json").exists());
    assert!(retained_dir.join("trace.json").exists());
    #[cfg(unix)]
    {
        assert_private_dir(&retained_dir);
        assert_private_file(&trace_root.join("trace.json"));
        assert_private_file(&retained_dir.join("stderr.raw"));
        assert_private_file(&retained_dir.join("diagnostics.sarif"));
        assert_private_file(&retained_dir.join("invocation.json"));
        assert_private_file(&retained_dir.join("ir.analysis.json"));
        assert_private_file(&retained_dir.join("trace.json"));

        let runtime_temp_dir = fs::read_dir(&runtime_root)
            .unwrap()
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .find(|path| path.is_dir())
            .unwrap();
        assert_private_dir(&runtime_temp_dir);
        assert_private_file(&runtime_temp_dir.join("diagnostics.sarif"));
        assert_private_file(&runtime_temp_dir.join("invocation.json"));
    }

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
    assert_eq!(invocation["redaction_class"].as_str(), Some("restricted"));
    assert_eq!(invocation["argv_hash"].as_str().map(str::len), Some(64));
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
    assert_eq!(
        invocation["normalized_invocation"]["arg_count"].as_u64(),
        Some(3)
    );
    assert_eq!(
        invocation["normalized_invocation"]["input_count"].as_u64(),
        Some(1)
    );
    assert_eq!(
        invocation["normalized_invocation"]["compile_only"].as_bool(),
        Some(true)
    );
    assert_eq!(
        invocation["normalized_invocation"]["injected_flag_count"].as_u64(),
        Some(1)
    );

    let retained_trace: Value =
        serde_json::from_str(&fs::read_to_string(retained_dir.join("trace.json")).unwrap())
            .unwrap();
    assert_eq!(retained_trace["selected_mode"], "render");
    assert_eq!(
        retained_trace["capabilities"]["stream_kind"],
        expected_non_tty_stream_kind()
    );

    let normalized_ir: Value =
        serde_json::from_str(&fs::read_to_string(retained_dir.join("ir.analysis.json")).unwrap())
            .unwrap();
    assert_eq!(normalized_ir["document_id"].as_str(), Some("<document>"));
    assert_eq!(
        normalized_ir["run"]["invocation_id"].as_str(),
        Some("<invocation>")
    );
    assert_eq!(normalized_ir["run"]["cwd_display"].as_str(), Some("<cwd>"));
    assert_eq!(
        normalized_ir["producer"]["version"].as_str(),
        Some("<normalized>")
    );
    assert!(normalized_ir["run"]["primary_tool"]["version"].is_null());
    assert_eq!(
        normalized_ir["captures"][1]["external_ref"].as_str(),
        Some("<capture:diagnostics.sarif>")
    );
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
    assert!(report["backend"]["support_tier"].is_null());
    assert_eq!(report["backend"]["version_band"], "gcc15_plus");
    assert_eq!(report["backend"]["processing_path"], "dual_sink_structured");
    assert_eq!(report["backend"]["support_level"], "preview");
    let rollout_cases = report["rollout_matrix"]["cases"].as_array().unwrap();
    assert!(rollout_cases.iter().any(|case| {
        case["version_band"] == "gcc15_plus"
            && case["requested_mode"].is_null()
            && case["selected_mode"] == "render"
            && case["processing_path"] == "dual_sink_structured"
            && case["support_level"] == "preview"
    }));
    assert!(rollout_cases.iter().any(|case| {
        case["version_band"] == "gcc13_14"
            && case["requested_mode"] == "shadow"
            && case["selected_mode"] == "shadow"
            && case["processing_path"] == "native_text_capture"
            && case["support_level"] == "experimental"
            && case["fallback_reason"] == "shadow_mode"
    }));
    assert!(report["warnings"].as_array().unwrap().is_empty());
}

fn expected_non_tty_stream_kind() -> &'static str {
    if env::var_os("CI").is_some() {
        "cilog"
    } else {
        "pipe"
    }
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

#[test]
fn hard_conflict_passthrough_still_emits_trace_bundle() {
    let temp = fixture("15.2.0");
    let backend = temp.path().join("fake-gcc");
    let source = temp.path().join("main.c");
    let trace_root = temp.path().join("trace-root");
    let runtime_root = temp.path().join("runtime-root");

    Command::cargo_bin("gcc-formed")
        .unwrap()
        .env("FORMED_BACKEND_GCC", &backend)
        .env("FORMED_TRACE_DIR", &trace_root)
        .env("FORMED_RUNTIME_DIR", &runtime_root)
        .current_dir(temp.path())
        .arg("--formed-trace=always")
        .arg("-fdiagnostics-format=text")
        .arg("-c")
        .arg(&source)
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "main.c:4:1: error: expected ';' before '}' token",
        ))
        .stderr(predicate::str::contains("help:").not());

    let trace: Value =
        serde_json::from_str(&fs::read_to_string(trace_root.join("trace.json")).unwrap()).unwrap();
    assert_eq!(trace["selected_mode"], "passthrough");
    assert_eq!(trace["wrapper_verdict"], "passthrough_fallback");
    assert_eq!(trace["fallback_reason"], "incompatible_sink");
    assert_eq!(
        trace["environment_summary"]["backend_version"].as_str(),
        Some("gcc (Fake) 15.2.0")
    );
    assert_eq!(trace["environment_summary"]["version_band"], "gcc15_plus");
    assert_eq!(
        trace["environment_summary"]["processing_path"],
        "passthrough"
    );
    assert_eq!(trace["environment_summary"]["support_level"], "preview");
    assert!(
        trace["environment_summary"]["injected_flags"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert!(
        trace["environment_summary"]["sanitized_env_keys"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert!(trace["timing"]["capture_ms"].as_u64().is_some());
    assert!(trace["timing"]["render_ms"].is_null());
    assert_eq!(trace["child_exit"]["code"].as_i64(), Some(1));
    assert!(trace["child_exit"]["signal"].is_null());
    assert_eq!(
        trace["fingerprint_summary"]["raw"].as_str().map(str::len),
        Some(64)
    );
    assert!(trace["fingerprint_summary"]["normalized"].is_null());
    assert!(trace["fingerprint_summary"]["family"].is_null());
    assert_eq!(trace["redaction_status"]["class"], "restricted");
    assert_eq!(
        trace["redaction_status"]["local_only"].as_bool(),
        Some(true)
    );
    assert!(
        trace["redaction_status"]["normalized_artifacts"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        trace["parser_result_summary"]["status"].as_str(),
        Some("skipped")
    );
    assert!(trace["parser_result_summary"]["document_completeness"].is_null());
    assert_eq!(
        trace["parser_result_summary"]["capture_count"].as_u64(),
        Some(1)
    );
    assert!(
        trace["decision_log"]
            .as_array()
            .unwrap()
            .iter()
            .any(|entry| entry.as_str() == Some("hard_conflict=diagnostic_sink_override"))
    );
    assert!(
        trace["artifacts"]
            .as_array()
            .unwrap()
            .iter()
            .any(|artifact| artifact["id"].as_str() == Some("stderr.raw"))
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
            .all(|artifact| artifact["id"].as_str() != Some("ir.analysis.json"))
    );

    let retained_dir = fs::read_dir(&trace_root)
        .unwrap()
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .find(|path| path.is_dir())
        .unwrap();
    assert!(retained_dir.join("stderr.raw").exists());
    assert!(retained_dir.join("invocation.json").exists());
    assert!(retained_dir.join("trace.json").exists());
    assert!(!retained_dir.join("ir.analysis.json").exists());
    assert!(
        fs::read_to_string(retained_dir.join("stderr.raw"))
            .unwrap()
            .contains("main.c:4:1: error: expected ';' before '}' token")
    );

    let invocation: Value =
        serde_json::from_str(&fs::read_to_string(retained_dir.join("invocation.json")).unwrap())
            .unwrap();
    assert_eq!(invocation["selected_mode"], "passthrough");
    assert_eq!(invocation["redaction_class"].as_str(), Some("restricted"));
    assert_eq!(
        invocation["normalized_invocation"]["injected_flag_count"].as_u64(),
        Some(0)
    );
}

#[test]
fn missing_sarif_falls_back_with_reason_coded_trace() {
    let temp = fixture_with_sarif_mode("15.2.0", "missing");
    let backend = temp.path().join("fake-gcc");
    let source = temp.path().join("main.c");
    let trace_root = temp.path().join("trace-root");

    Command::cargo_bin("gcc-formed")
        .unwrap()
        .env("FORMED_BACKEND_GCC", &backend)
        .env("FORMED_TRACE_DIR", &trace_root)
        .current_dir(temp.path())
        .arg("--formed-trace=always")
        .arg("-c")
        .arg(&source)
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "note: some compiler details were not fully structured; original diagnostics are preserved",
        ))
        .stderr(predicate::str::contains(
            "main.c:4:1: error: expected ';' before '}' token",
        ));

    let trace: Value =
        serde_json::from_str(&fs::read_to_string(trace_root.join("trace.json")).unwrap()).unwrap();
    assert_eq!(trace["selected_mode"], "render");
    assert_eq!(trace["wrapper_verdict"], "render_fallback");
    assert_eq!(trace["fallback_reason"], "sarif_missing");
    assert_eq!(
        trace["parser_result_summary"]["status"].as_str(),
        Some("fallback")
    );
    assert_eq!(
        trace["parser_result_summary"]["document_completeness"].as_str(),
        Some("partial")
    );
    assert!(
        trace["decision_log"]
            .as_array()
            .unwrap()
            .iter()
            .any(|entry| entry.as_str() == Some("ingest_source_authority=residual_text"))
    );
    assert!(
        trace["decision_log"]
            .as_array()
            .unwrap()
            .iter()
            .any(|entry| entry.as_str() == Some("ingest_fallback_grade=fail_open"))
    );
    assert!(
        trace["warning_messages"]
            .as_array()
            .unwrap()
            .iter()
            .any(|message| message
                .as_str()
                .is_some_and(|message| message.contains("authoritative SARIF was not produced")))
    );
}

#[test]
fn invalid_sarif_falls_back_with_reason_coded_trace() {
    let temp = fixture_with_sarif_mode("15.2.0", "invalid");
    let backend = temp.path().join("fake-gcc");
    let source = temp.path().join("main.c");
    let trace_root = temp.path().join("trace-root");

    Command::cargo_bin("gcc-formed")
        .unwrap()
        .env("FORMED_BACKEND_GCC", &backend)
        .env("FORMED_TRACE_DIR", &trace_root)
        .current_dir(temp.path())
        .arg("--formed-trace=always")
        .arg("-c")
        .arg(&source)
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "error: showing a conservative wrapper view",
        ))
        .stderr(predicate::str::contains(
            "main.c:4:1: error: expected ';' before '}' token",
        ));

    let trace: Value =
        serde_json::from_str(&fs::read_to_string(trace_root.join("trace.json")).unwrap()).unwrap();
    assert_eq!(trace["selected_mode"], "render");
    assert_eq!(trace["wrapper_verdict"], "render_fallback");
    assert_eq!(trace["fallback_reason"], "sarif_parse_failed");
    assert_eq!(
        trace["parser_result_summary"]["status"].as_str(),
        Some("fallback")
    );
    assert_eq!(
        trace["parser_result_summary"]["document_completeness"].as_str(),
        Some("failed")
    );
    assert!(
        trace["decision_log"]
            .as_array()
            .unwrap()
            .iter()
            .any(|entry| entry.as_str() == Some("ingest_source_authority=residual_text"))
    );
    assert!(
        trace["decision_log"]
            .as_array()
            .unwrap()
            .iter()
            .any(|entry| entry.as_str() == Some("ingest_fallback_grade=fail_open"))
    );
    assert!(
        trace["warning_messages"]
            .as_array()
            .unwrap()
            .iter()
            .any(|message| message
                .as_str()
                .is_some_and(|message| message.contains("failed to parse authoritative SARIF")))
    );
}

fn parse_env_dump(contents: &str) -> BTreeMap<String, String> {
    contents
        .lines()
        .filter_map(|line| line.split_once('='))
        .map(|(key, value)| (key.to_string(), value.to_string()))
        .collect()
}

fn expected_tier_b_native_text_notice() -> &'static str {
    "gcc-formed: version band=gcc13_14 support level=experimental default processing path=native_text_capture; selected mode=render; fallback reason=none; native-text capture is the default and explicit single_sink_structured selection remains opt-in."
}

fn expected_tier_b_single_sink_notice() -> &'static str {
    "gcc-formed: version band=gcc13_14 support level=experimental default processing path=native_text_capture; selected mode=render; processing path=single_sink_structured; explicit structured capture is active and raw native diagnostics may not be preserved in the same run."
}

fn expected_tier_b_shadow_notice() -> &'static str {
    "gcc-formed: version band=gcc13_14 support level=experimental default processing path=native_text_capture; selected mode=shadow; fallback reason=shadow_mode; conservative native-text shadow capture is enabled and explicit single_sink_structured selection remains opt-in."
}

fn expected_tier_c_native_text_notice() -> &'static str {
    "gcc-formed: version band=gcc9_12 support level=experimental default processing path=native_text_capture; selected mode=render; fallback reason=none; native-text capture is the default and explicit single_sink_structured JSON selection remains opt-in."
}

fn expected_tier_c_single_sink_notice() -> &'static str {
    "gcc-formed: version band=gcc9_12 support level=experimental default processing path=native_text_capture; selected mode=render; processing path=single_sink_structured; explicit structured JSON capture is active and raw native diagnostics may not be preserved in the same run."
}

#[cfg(unix)]
fn assert_private_dir(path: &std::path::Path) {
    assert_eq!(
        fs::metadata(path).unwrap().permissions().mode() & 0o777,
        0o700
    );
}

#[cfg(unix)]
fn assert_private_file(path: &std::path::Path) {
    assert_eq!(
        fs::metadata(path).unwrap().permissions().mode() & 0o777,
        0o600
    );
}

fn fixture(version: &str) -> TempDir {
    fixture_with_sarif_mode(version, "valid")
}

fn fixture_with_stderr(version: &str, stderr: &str) -> TempDir {
    fixture_with_sarif_mode_and_stderr(version, "valid", stderr)
}

fn fixture_with_sarif_mode(version: &str, sarif_mode: &str) -> TempDir {
    fixture_with_sarif_mode_and_stderr(
        version,
        sarif_mode,
        "main.c:4:1: error: expected ';' before '}' token\n",
    )
}

fn fixture_with_sarif_mode_and_stderr(version: &str, sarif_mode: &str, stderr: &str) -> TempDir {
    let temp = tempfile::tempdir().unwrap();
    fs::write(temp.path().join("main.c"), "int main(void) { return 0 }\n").unwrap();
    fs::write(temp.path().join("main.cpp"), "int main() { return 0; }\n").unwrap();
    fs::write(temp.path().join("stderr.txt"), stderr).unwrap();
    let script = format!(
        r#"#!/usr/bin/env bash
set -euo pipefail
if [[ "${{1:-}}" == "--version" ]]; then
  echo "gcc (Fake) {version}"
  exit 0
fi
	sarif=""
	structured_kind=""
	for arg in "$@"; do
	  if [[ "$arg" == -fdiagnostics-add-output=sarif:version=2.1,file=* ]]; then
	    sarif="${{arg#-fdiagnostics-add-output=sarif:version=2.1,file=}}"
	    structured_kind="sarif"
	  elif [[ "$arg" == "-fdiagnostics-format=sarif-file" ]]; then
	    sarif="source.sarif"
	    structured_kind="sarif"
	  elif [[ "$arg" == "-fdiagnostics-format=json-file" ]]; then
	    sarif="source.json"
	    structured_kind="json"
	  fi
	done
if [[ -n "$sarif" ]]; then
  case "{sarif_mode}" in
    valid)
      if [[ "$structured_kind" == "json" ]]; then
        cat >"$sarif" <<'JSON'
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
      else
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
      ;;
    invalid)
      if [[ "$structured_kind" == "json" ]]; then
        printf '%s\n' '[' >"$sarif"
      else
        printf '%s\n' '{{"version":' >"$sarif"
      fi
      ;;
    missing)
      ;;
  esac
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
	if [[ "$sarif" != "source.sarif" && "$sarif" != "source.json" || "{sarif_mode}" != "valid" ]]; then
	  cat "$(dirname "$0")/stderr.txt" >&2
	fi
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
