//! Captures diagnostic artifacts from compiler invocations, manages temporary files and cleanup.

mod artifact;
mod artifact_builder;
mod capture;
mod policy;

pub use artifact::*;
pub use capture::{cleanup_capture, run_capture};
pub use policy::*;

pub(crate) const STDERR_CAPTURE_BUFFER_BYTES: usize = 4096;
pub(crate) const STDERR_CAPTURE_PREVIEW_LIMIT_BYTES: usize = 1024 * 1024;
pub(crate) const STDERR_CAPTURE_ID: &str = "stderr.raw";

#[cfg(test)]
mod tests {
    use super::*;
    use crate::artifact_builder::{
        CapturedStderr, build_artifacts, build_capture_bundle, build_invocation_record,
        normalize_invocation, tool_info,
    };
    use crate::capture::{
        await_stderr_capture, capture_stderr_stream, path_is_safe_for_gcc_output, unique_temp_dir,
    };
    use crate::policy::{child_env_policy, child_env_policy_for_mode, child_env_policy_is_empty};
    use diag_backend_probe::{
        ActiveBackendTopology, BACKEND_TOPOLOGY_POLICY_VERSION, BackendTopologyDisposition,
        BackendTopologyKind, DriverKind, ProbeKey,
    };
    use diag_core::{ArtifactKind, ArtifactStorage, CaptureArtifact, fingerprint_for};
    use std::collections::BTreeSet;
    use std::io::Cursor;
    use std::path::PathBuf;
    use std::thread;

    fn fake_probe() -> diag_backend_probe::ProbeResult {
        diag_backend_probe::ProbeResult {
            requested_backend: "gcc-formed".to_string(),
            resolved_path: PathBuf::from("/usr/bin/gcc"),
            execution_topology: ActiveBackendTopology {
                policy_version: BACKEND_TOPOLOGY_POLICY_VERSION.to_string(),
                kind: BackendTopologyKind::Direct,
                launcher_path: None,
                disposition: BackendTopologyDisposition::Supported,
            },
            version_string: "gcc (GCC) 15.1.0".to_string(),
            major: 15,
            minor: 1,
            driver_kind: DriverKind::Gcc,
            add_output_sarif_supported: true,
            version_probe_key: ProbeKey {
                realpath: PathBuf::from("/usr/bin/gcc"),
                inode: 1,
                mtime_seconds: 0,
                size_bytes: 1,
            },
        }
    }

    fn fake_paths() -> diag_trace::WrapperPaths {
        diag_trace::WrapperPaths {
            config_path: PathBuf::from("/tmp/config.toml"),
            cache_root: PathBuf::from("/tmp/cache"),
            state_root: PathBuf::from("/tmp/state"),
            runtime_root: PathBuf::from("/tmp/runtime"),
            trace_root: PathBuf::from("/tmp/traces"),
            install_root: PathBuf::from("/tmp/install"),
        }
    }

    fn empty_invocation(processing_path: diag_backend_probe::ProcessingPath) -> CaptureInvocation {
        CaptureInvocation {
            backend_path: "/usr/bin/gcc".to_string(),
            launcher_path: None,
            spawn_path: "/usr/bin/gcc".to_string(),
            argv: Vec::new(),
            spawn_argv: Vec::new(),
            argv_hash: "hash".to_string(),
            cwd: "/tmp/project".to_string(),
            selected_mode: ExecutionMode::Render,
            processing_path,
        }
    }

    fn captured_stderr(bytes: &[u8]) -> CapturedStderr {
        CapturedStderr {
            preview_bytes: bytes.to_vec(),
            total_bytes: bytes.len() as u64,
            truncated_bytes: 0,
            spool_path: PathBuf::from("/tmp/runtime/stderr.raw"),
        }
    }

    #[test]
    fn path_safety_helper_rejects_unsafe_runtime_roots() {
        assert!(path_is_safe_for_gcc_output(std::path::Path::new(
            "/tmp/cc-formed-runtime/formed-123/diagnostics.sarif"
        )));
        assert!(!path_is_safe_for_gcc_output(std::path::Path::new(
            "/tmp/runtime,root=unsafe path/formed-123/diagnostics.sarif"
        )));
        assert!(!path_is_safe_for_gcc_output(std::path::Path::new(
            "/tmp/runtime=root/formed-123/diagnostics.sarif"
        )));
    }

    #[cfg(unix)]
    fn make_executable(path: &std::path::Path) {
        use std::os::unix::fs::PermissionsExt;

        let mut permissions = std::fs::metadata(path).unwrap().permissions();
        permissions.set_mode(0o755);
        std::fs::set_permissions(path, permissions).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn run_capture_uses_preselected_safe_sarif_path_for_unsafe_runtime_root() {
        let temp = tempfile::tempdir().unwrap();
        let backend = temp.path().join("fake-gcc");
        let observed_sarif_path = temp.path().join("observed-sarif-path.txt");
        let runtime_root = temp.path().join("runtime,root=unsafe path");
        let cwd = temp.path().join("cwd");
        std::fs::create_dir_all(&cwd).unwrap();

        let script = format!(
            r#"#!/usr/bin/env bash
set -euo pipefail
if [[ "${{1:-}}" == "--version" ]]; then
  echo "gcc (Fake) 15.2.0"
  exit 0
fi
sarif=""
for arg in "$@"; do
  if [[ "$arg" == -fdiagnostics-add-output=sarif:version=2.1,file=* ]]; then
    sarif="${{arg#-fdiagnostics-add-output=sarif:version=2.1,file=}}"
  fi
done
printf '%s' "$sarif" > "{}"
if [[ -n "$sarif" ]]; then
  cat > "$sarif" <<'SARIF'
{{"version":"2.1.0","runs":[]}}
SARIF
fi
printf '%s\n' 'main.c:1:1: error: synthetic failure' >&2
exit 1
"#,
            observed_sarif_path.display()
        );
        std::fs::write(&backend, script).unwrap();
        make_executable(&backend);

        let request = CaptureRequest {
            backend: diag_backend_probe::ProbeResult {
                resolved_path: backend.clone(),
                execution_topology: ActiveBackendTopology {
                    policy_version: BACKEND_TOPOLOGY_POLICY_VERSION.to_string(),
                    kind: BackendTopologyKind::Direct,
                    launcher_path: None,
                    disposition: BackendTopologyDisposition::Supported,
                },
                version_string: "gcc (Fake) 15.2.0".to_string(),
                ..fake_probe()
            },
            args: vec![
                std::ffi::OsString::from("-c"),
                std::ffi::OsString::from("main.c"),
            ],
            cwd,
            mode: ExecutionMode::Render,
            capture_passthrough_stderr: false,
            retention: diag_trace::RetentionPolicy::Never,
            paths: diag_trace::WrapperPaths {
                config_path: temp.path().join("config.toml"),
                cache_root: temp.path().join("cache-root"),
                state_root: temp.path().join("state-root"),
                runtime_root: runtime_root.clone(),
                trace_root: temp.path().join("trace-root"),
                install_root: temp.path().join("install-root"),
            },
            structured_capture: StructuredCapturePolicy::SarifFile,
            preserve_native_color: false,
        };

        let output = run_capture(&request).unwrap();
        let sarif_path = output.sarif_path.clone().unwrap();
        let injected_sarif_arg = output
            .bundle
            .invocation
            .argv
            .iter()
            .find_map(|arg| arg.strip_prefix("-fdiagnostics-add-output=sarif:version=2.1,file="))
            .unwrap();

        assert_eq!(sarif_path, output.temp_dir.join("diagnostics.sarif"));
        assert!(sarif_path.exists());
        assert!(!output.temp_dir.starts_with(&runtime_root));
        assert!(path_is_safe_for_gcc_output(&output.temp_dir));
        assert_eq!(injected_sarif_arg, sarif_path.display().to_string());
        assert_eq!(
            std::fs::read_to_string(&observed_sarif_path).unwrap(),
            sarif_path.display().to_string()
        );

        cleanup_capture(&output).unwrap();
        assert!(!output.temp_dir.exists());
    }

    #[test]
    fn creates_inline_stderr_artifact() {
        let artifacts = build_artifacts(&captured_stderr(b"stderr"), None, &fake_probe());
        assert_eq!(artifacts[0].id, "stderr.raw");
        assert!(
            artifacts[0]
                .inline_text
                .as_deref()
                .unwrap()
                .contains("stderr")
        );
    }

    #[test]
    fn builds_invocation_record_with_selected_mode_and_sarif() {
        let request = CaptureRequest {
            backend: fake_probe(),
            args: vec![
                std::ffi::OsString::from("-c"),
                std::ffi::OsString::from("src/main.c"),
            ],
            cwd: PathBuf::from("/tmp/project"),
            mode: ExecutionMode::Render,
            capture_passthrough_stderr: false,
            retention: diag_trace::RetentionPolicy::Always,
            paths: fake_paths(),
            structured_capture: StructuredCapturePolicy::SarifFile,
            preserve_native_color: true,
        };

        let record = build_invocation_record(
            &request,
            &request.capture_plan(),
            &[
                std::ffi::OsString::from("-c"),
                std::ffi::OsString::from("src/main.c"),
                std::ffi::OsString::from(
                    "-fdiagnostics-add-output=sarif:version=2.1,file=/tmp/runtime/diagnostics.sarif",
                ),
            ],
            &[
                std::ffi::OsString::from("-c"),
                std::ffi::OsString::from("src/main.c"),
                std::ffi::OsString::from(
                    "-fdiagnostics-add-output=sarif:version=2.1,file=/tmp/runtime/diagnostics.sarif",
                ),
            ],
            Some(std::path::Path::new("/tmp/runtime/diagnostics.sarif")),
            child_env_policy(&request.capture_plan()),
        );

        assert!(request.capture_plan().preserve_native_color);
        assert_eq!(record.selected_mode, ExecutionMode::Render);
        assert_eq!(record.backend_path, "/usr/bin/gcc");
        assert_eq!(record.spawn_path, "/usr/bin/gcc");
        assert_eq!(record.argv_hash, fingerprint_for(&record.argv));
        assert_eq!(record.redaction_class, "restricted");
        assert_eq!(record.cwd, "/tmp/project");
        assert_eq!(
            record.sarif_path.as_deref(),
            Some("/tmp/runtime/diagnostics.sarif")
        );
        assert!(record.argv.iter().any(|arg| arg == "-c"));
        assert!(
            record
                .argv
                .iter()
                .any(|arg| arg.starts_with("-fdiagnostics-add-output=sarif:version=2.1,file="))
        );
        assert_eq!(record.normalized_invocation.arg_count, 3);
        assert_eq!(record.normalized_invocation.input_count, 1);
        assert!(record.normalized_invocation.compile_only);
        assert_eq!(record.normalized_invocation.injected_flag_count, 1);
        assert_eq!(record.normalized_invocation.diagnostics_flag_count, 1);
        assert_eq!(
            record
                .child_env_policy
                .set
                .get("LC_MESSAGES")
                .map(String::as_str),
            Some("C")
        );
    }

    #[test]
    fn render_mode_sets_locale_and_unsets_conflicting_diagnostic_env() {
        let policy = child_env_policy_for_mode(ExecutionMode::Render);
        assert_eq!(policy.set.get("LC_MESSAGES").map(String::as_str), Some("C"));
        assert!(policy.unset.iter().any(|key| key == "GCC_DIAGNOSTICS_LOG"));
        assert!(
            policy
                .unset
                .iter()
                .any(|key| key == "GCC_EXTRA_DIAGNOSTIC_OUTPUT")
        );
        assert!(
            policy
                .unset
                .iter()
                .any(|key| key == "EXPERIMENTAL_SARIF_SOCKET")
        );
    }

    #[test]
    fn shadow_mode_only_unsets_conflicting_diagnostic_env() {
        let policy = child_env_policy_for_mode(ExecutionMode::Shadow);
        assert!(policy.set.is_empty());
        assert_eq!(policy.unset.len(), 3);
    }

    #[test]
    fn passthrough_mode_preserves_environment() {
        let policy = child_env_policy_for_mode(ExecutionMode::Passthrough);
        assert!(child_env_policy_is_empty(&policy));
    }

    #[test]
    fn capture_plan_derives_current_render_and_passthrough_policies() {
        let render_request = CaptureRequest {
            backend: fake_probe(),
            args: Vec::new(),
            cwd: PathBuf::from("/tmp/project"),
            mode: ExecutionMode::Render,
            capture_passthrough_stderr: false,
            retention: diag_trace::RetentionPolicy::OnWrapperFailure,
            paths: fake_paths(),
            structured_capture: StructuredCapturePolicy::SarifFile,
            preserve_native_color: false,
        };
        let render_plan = render_request.capture_plan();
        assert_eq!(render_plan.execution_mode, ExecutionMode::Render);
        assert_eq!(
            render_plan.processing_path,
            diag_backend_probe::ProcessingPath::DualSinkStructured
        );
        assert_eq!(
            render_plan.structured_capture,
            StructuredCapturePolicy::SarifFile
        );
        assert_eq!(
            render_plan.native_text_capture,
            NativeTextCapturePolicy::CaptureOnly
        );
        assert_eq!(render_plan.locale_handling, LocaleHandling::ForceMessagesC);
        assert_eq!(
            render_plan.retention_policy,
            diag_trace::RetentionPolicy::OnWrapperFailure
        );

        let passthrough_request = CaptureRequest {
            backend: fake_probe(),
            args: Vec::new(),
            cwd: PathBuf::from("/tmp/project"),
            mode: ExecutionMode::Passthrough,
            capture_passthrough_stderr: true,
            retention: diag_trace::RetentionPolicy::Always,
            paths: fake_paths(),
            structured_capture: StructuredCapturePolicy::Disabled,
            preserve_native_color: false,
        };
        let passthrough_plan = passthrough_request.capture_plan();
        assert_eq!(
            passthrough_plan.processing_path,
            diag_backend_probe::ProcessingPath::Passthrough
        );
        assert_eq!(
            passthrough_plan.structured_capture,
            StructuredCapturePolicy::Disabled
        );
        assert_eq!(
            passthrough_plan.native_text_capture,
            NativeTextCapturePolicy::TeeToParent
        );
        assert_eq!(passthrough_plan.locale_handling, LocaleHandling::Preserve);
        assert_eq!(
            passthrough_plan.retention_policy,
            diag_trace::RetentionPolicy::Always
        );
    }

    #[test]
    fn capture_plan_passthroughs_on_user_diagnostics_sink_conflict() {
        let request = CaptureRequest {
            backend: fake_probe(),
            args: vec![
                std::ffi::OsString::from("-c"),
                std::ffi::OsString::from("main.c"),
                std::ffi::OsString::from("-fdiagnostics-format=sarif-file"),
            ],
            cwd: PathBuf::from("/tmp/project"),
            mode: ExecutionMode::Render,
            capture_passthrough_stderr: false,
            retention: diag_trace::RetentionPolicy::OnWrapperFailure,
            paths: fake_paths(),
            structured_capture: StructuredCapturePolicy::SarifFile,
            preserve_native_color: true,
        };

        let plan = request.capture_plan();
        assert_eq!(plan.execution_mode, ExecutionMode::Passthrough);
        assert_eq!(
            plan.processing_path,
            diag_backend_probe::ProcessingPath::Passthrough
        );
        assert_eq!(plan.structured_capture, StructuredCapturePolicy::Disabled);
        assert_eq!(
            plan.native_text_capture,
            NativeTextCapturePolicy::Passthrough
        );
        assert!(!plan.preserve_native_color);
        assert_eq!(plan.locale_handling, LocaleHandling::Preserve);
    }

    #[test]
    fn capture_plan_disables_color_injection_when_user_overrides_color() {
        let request = CaptureRequest {
            backend: fake_probe(),
            args: vec![
                std::ffi::OsString::from("-c"),
                std::ffi::OsString::from("main.c"),
                std::ffi::OsString::from("-fdiagnostics-color=never"),
            ],
            cwd: PathBuf::from("/tmp/project"),
            mode: ExecutionMode::Render,
            capture_passthrough_stderr: false,
            retention: diag_trace::RetentionPolicy::Always,
            paths: fake_paths(),
            structured_capture: StructuredCapturePolicy::Disabled,
            preserve_native_color: true,
        };

        let plan = request.capture_plan();
        assert_eq!(plan.execution_mode, ExecutionMode::Render);
        assert_eq!(
            plan.processing_path,
            diag_backend_probe::ProcessingPath::NativeTextCapture
        );
        assert!(!plan.preserve_native_color);
    }

    #[test]
    fn injected_flags_preserve_native_color_when_requested() {
        let bundle = CaptureBundle {
            plan: CapturePlan {
                execution_mode: ExecutionMode::Render,
                processing_path: diag_backend_probe::ProcessingPath::NativeTextCapture,
                structured_capture: StructuredCapturePolicy::Disabled,
                native_text_capture: NativeTextCapturePolicy::CaptureOnly,
                preserve_native_color: true,
                locale_handling: LocaleHandling::ForceMessagesC,
                retention_policy: diag_trace::RetentionPolicy::Always,
            },
            invocation: empty_invocation(diag_backend_probe::ProcessingPath::NativeTextCapture),
            raw_text_artifacts: Vec::new(),
            structured_artifacts: Vec::new(),
            exit_status: ExitStatusInfo {
                code: Some(1),
                signal: None,
                success: false,
            },
            integrity_issues: Vec::new(),
        };

        assert_eq!(
            bundle.injected_flags(std::path::Path::new("/tmp/runtime")),
            vec!["-fdiagnostics-color=always".to_string()]
        );
    }

    #[test]
    fn single_sink_capture_plan_uses_explicit_structured_path() {
        let request = CaptureRequest {
            backend: fake_probe(),
            args: Vec::new(),
            cwd: PathBuf::from("/tmp/project"),
            mode: ExecutionMode::Render,
            capture_passthrough_stderr: false,
            retention: diag_trace::RetentionPolicy::OnWrapperFailure,
            paths: fake_paths(),
            structured_capture: StructuredCapturePolicy::SingleSinkSarifFile,
            preserve_native_color: false,
        };

        let plan = request.capture_plan();
        assert_eq!(
            plan.processing_path,
            diag_backend_probe::ProcessingPath::SingleSinkStructured
        );
        assert_eq!(
            plan.structured_capture,
            StructuredCapturePolicy::SingleSinkSarifFile
        );
        let bundle = CaptureBundle {
            plan,
            invocation: empty_invocation(diag_backend_probe::ProcessingPath::SingleSinkStructured),
            raw_text_artifacts: Vec::new(),
            structured_artifacts: Vec::new(),
            exit_status: ExitStatusInfo {
                code: Some(1),
                signal: None,
                success: false,
            },
            integrity_issues: Vec::new(),
        };
        assert_eq!(
            bundle.injected_flags(std::path::Path::new("/tmp/runtime")),
            vec!["-fdiagnostics-format=sarif-file".to_string()]
        );
    }

    #[test]
    fn single_sink_json_capture_plan_uses_explicit_structured_path() {
        let request = CaptureRequest {
            backend: fake_probe(),
            args: Vec::new(),
            cwd: PathBuf::from("/tmp/project"),
            mode: ExecutionMode::Render,
            capture_passthrough_stderr: false,
            retention: diag_trace::RetentionPolicy::OnWrapperFailure,
            paths: fake_paths(),
            structured_capture: StructuredCapturePolicy::SingleSinkJsonFile,
            preserve_native_color: false,
        };

        let plan = request.capture_plan();
        assert_eq!(
            plan.processing_path,
            diag_backend_probe::ProcessingPath::SingleSinkStructured
        );
        assert_eq!(
            plan.structured_capture,
            StructuredCapturePolicy::SingleSinkJsonFile
        );
        let bundle = CaptureBundle {
            plan,
            invocation: empty_invocation(diag_backend_probe::ProcessingPath::SingleSinkStructured),
            raw_text_artifacts: Vec::new(),
            structured_artifacts: Vec::new(),
            exit_status: ExitStatusInfo {
                code: Some(1),
                signal: None,
                success: false,
            },
            integrity_issues: Vec::new(),
        };
        assert_eq!(
            bundle.injected_flags(std::path::Path::new("/tmp/runtime")),
            vec!["-fdiagnostics-format=json-file".to_string()]
        );
        assert_eq!(
            bundle.temp_artifact_paths(std::path::Path::new("/tmp/runtime")),
            vec![
                PathBuf::from("/tmp/runtime"),
                PathBuf::from("/tmp/runtime/invocation.json"),
                PathBuf::from("/tmp/runtime/diagnostics.json"),
            ]
        );
    }

    #[test]
    fn capture_bundle_groups_raw_text_and_structured_artifacts() {
        let request = CaptureRequest {
            backend: fake_probe(),
            args: vec![
                std::ffi::OsString::from("-c"),
                std::ffi::OsString::from("main.c"),
            ],
            cwd: PathBuf::from("/tmp/project"),
            mode: ExecutionMode::Render,
            capture_passthrough_stderr: false,
            retention: diag_trace::RetentionPolicy::Always,
            paths: fake_paths(),
            structured_capture: StructuredCapturePolicy::SarifFile,
            preserve_native_color: false,
        };
        let plan = request.capture_plan();
        let sarif_path = PathBuf::from("/tmp/runtime/diagnostics.sarif");
        let artifacts = build_artifacts(
            &captured_stderr(b"stderr"),
            Some(&sarif_path),
            &request.backend,
        );
        let exit_status = ExitStatusInfo {
            code: Some(1),
            signal: None,
            success: false,
        };

        let bundle = build_capture_bundle(
            &request,
            &[
                std::ffi::OsString::from("-c"),
                std::ffi::OsString::from("main.c"),
                std::ffi::OsString::from(
                    "-fdiagnostics-add-output=sarif:version=2.1,file=/tmp/runtime/diagnostics.sarif",
                ),
            ],
            &[
                std::ffi::OsString::from("-c"),
                std::ffi::OsString::from("main.c"),
                std::ffi::OsString::from(
                    "-fdiagnostics-add-output=sarif:version=2.1,file=/tmp/runtime/diagnostics.sarif",
                ),
            ],
            &plan,
            &exit_status,
            &artifacts,
            &[],
        );

        assert_eq!(bundle.invocation.backend_path, "/usr/bin/gcc");
        assert_eq!(bundle.invocation.selected_mode, ExecutionMode::Render);
        assert_eq!(
            bundle.invocation.processing_path,
            diag_backend_probe::ProcessingPath::DualSinkStructured
        );
        assert_eq!(bundle.raw_text_artifacts.len(), 1);
        assert_eq!(bundle.raw_text_artifacts[0].id, "stderr.raw");
        assert_eq!(bundle.structured_artifacts.len(), 1);
        assert_eq!(bundle.structured_artifacts[0].id, "diagnostics.sarif");
        assert_eq!(bundle.exit_status, exit_status);
        assert!(bundle.integrity_issues.is_empty());
    }

    #[test]
    fn capture_bundle_groups_raw_text_and_gcc_json_artifacts() {
        let request = CaptureRequest {
            backend: fake_probe(),
            args: vec![
                std::ffi::OsString::from("-c"),
                std::ffi::OsString::from("main.c"),
            ],
            cwd: PathBuf::from("/tmp/project"),
            mode: ExecutionMode::Render,
            capture_passthrough_stderr: false,
            retention: diag_trace::RetentionPolicy::Always,
            paths: fake_paths(),
            structured_capture: StructuredCapturePolicy::SingleSinkJsonFile,
            preserve_native_color: false,
        };
        let plan = request.capture_plan();
        let json_path = PathBuf::from("/tmp/runtime/diagnostics.json");
        let artifacts = build_artifacts(
            &captured_stderr(b"stderr"),
            Some(&json_path),
            &request.backend,
        );
        let exit_status = ExitStatusInfo {
            code: Some(1),
            signal: None,
            success: false,
        };

        let bundle = build_capture_bundle(
            &request,
            &[
                std::ffi::OsString::from("-c"),
                std::ffi::OsString::from("main.c"),
                std::ffi::OsString::from("-fdiagnostics-format=json-file"),
            ],
            &[
                std::ffi::OsString::from("-c"),
                std::ffi::OsString::from("main.c"),
                std::ffi::OsString::from("-fdiagnostics-format=json-file"),
            ],
            &plan,
            &exit_status,
            &artifacts,
            &[],
        );

        assert_eq!(
            bundle.invocation.processing_path,
            diag_backend_probe::ProcessingPath::SingleSinkStructured
        );
        assert_eq!(bundle.raw_text_artifacts.len(), 1);
        assert_eq!(bundle.raw_text_artifacts[0].id, "stderr.raw");
        assert_eq!(bundle.structured_artifacts.len(), 1);
        assert_eq!(bundle.structured_artifacts[0].id, "diagnostics.json");
        assert_eq!(bundle.structured_artifacts[0].kind, ArtifactKind::GccJson);
        assert_eq!(bundle.exit_status, exit_status);
        assert!(bundle.integrity_issues.is_empty());
    }

    #[test]
    fn capture_bundle_surfaces_stderr_truncation_issue() {
        let request = CaptureRequest {
            backend: fake_probe(),
            args: vec![
                std::ffi::OsString::from("-c"),
                std::ffi::OsString::from("main.c"),
            ],
            cwd: PathBuf::from("/tmp/project"),
            mode: ExecutionMode::Render,
            capture_passthrough_stderr: false,
            retention: diag_trace::RetentionPolicy::Always,
            paths: fake_paths(),
            structured_capture: StructuredCapturePolicy::Disabled,
            preserve_native_color: false,
        };
        let plan = request.capture_plan();
        let captured = CapturedStderr {
            preview_bytes: b"stderr-preview".to_vec(),
            total_bytes: (STDERR_CAPTURE_PREVIEW_LIMIT_BYTES + 128) as u64,
            truncated_bytes: 128,
            spool_path: PathBuf::from("/tmp/runtime/stderr.raw"),
        };
        let artifacts = build_artifacts(&captured, None, &request.backend);
        let exit_status = ExitStatusInfo {
            code: Some(1),
            signal: None,
            success: false,
        };

        let bundle = build_capture_bundle(
            &request,
            &[
                std::ffi::OsString::from("-c"),
                std::ffi::OsString::from("main.c"),
            ],
            &[
                std::ffi::OsString::from("-c"),
                std::ffi::OsString::from("main.c"),
            ],
            &plan,
            &exit_status,
            &artifacts,
            &captured.integrity_issues(),
        );

        assert_eq!(bundle.integrity_issues.len(), 1);
        assert_eq!(
            bundle.integrity_issues[0].stage,
            diag_core::IssueStage::Capture
        );
        assert!(bundle.integrity_issues[0].message.contains("truncated"));
    }

    #[test]
    fn capture_stderr_stream_truncates_large_template_flood_and_reports_integrity_issue() {
        let temp = tempfile::tempdir().unwrap();
        let spool_path = temp.path().join("stderr.raw");
        let line = "template instantiation depth exceeded while substituting std::vector<std::tuple<int, long, double>>\n";
        let repeats = (STDERR_CAPTURE_PREVIEW_LIMIT_BYTES / line.len()) + 64;
        let payload = line.repeat(repeats);

        let mut cursor = Cursor::new(payload.as_bytes());
        let captured = capture_stderr_stream(&mut cursor, &spool_path, None).unwrap();

        assert_eq!(
            captured.preview_bytes.len(),
            STDERR_CAPTURE_PREVIEW_LIMIT_BYTES
        );
        assert_eq!(captured.total_bytes, payload.len() as u64);
        assert!(captured.truncated());
        assert_eq!(
            std::fs::metadata(&spool_path).unwrap().len(),
            payload.len() as u64
        );
        let issues = captured.integrity_issues();
        assert_eq!(issues.len(), 1);
        assert!(
            issues[0]
                .message
                .contains("stderr capture exceeded the in-memory cap")
        );
        assert_eq!(issues[0].stage, diag_core::IssueStage::Capture);
    }

    #[test]
    fn capture_stderr_stream_truncates_large_linker_flood_and_tees_full_output() {
        let temp = tempfile::tempdir().unwrap();
        let spool_path = temp.path().join("stderr.raw");
        let line = "/usr/bin/ld: libhuge.a(object.o): undefined reference to `long_missing_symbol_name_for_linker_flood`\n";
        let repeats = (STDERR_CAPTURE_PREVIEW_LIMIT_BYTES / line.len()) + 32;
        let payload = line.repeat(repeats);
        let mut tee_bytes = Vec::new();

        let mut cursor = Cursor::new(payload.as_bytes());
        let captured =
            capture_stderr_stream(&mut cursor, &spool_path, Some(&mut tee_bytes)).unwrap();

        assert_eq!(tee_bytes, payload.as_bytes());
        assert_eq!(
            captured.preview_bytes.len(),
            STDERR_CAPTURE_PREVIEW_LIMIT_BYTES
        );
        assert_eq!(captured.total_bytes, payload.len() as u64);
        assert!(captured.truncated());
    }

    #[test]
    fn await_stderr_capture_propagates_reader_io_error() {
        let temp = tempfile::tempdir().unwrap();
        let spool_path = temp.path().join("stderr.raw");
        let handle = thread::spawn(|| -> Result<CapturedStderr, std::io::Error> {
            Err(std::io::Error::other("synthetic stderr capture failure"))
        });

        let error = await_stderr_capture(Some(handle), &spool_path).unwrap_err();

        match error {
            CaptureError::StderrCapture(source) => {
                assert_eq!(source.kind(), std::io::ErrorKind::Other);
                assert_eq!(source.to_string(), "synthetic stderr capture failure");
            }
            other => panic!("expected stderr capture error, got {other:?}"),
        }
    }

    #[test]
    fn await_stderr_capture_propagates_reader_panic() {
        let temp = tempfile::tempdir().unwrap();
        let spool_path = temp.path().join("stderr.raw");
        let handle = thread::spawn(|| -> Result<CapturedStderr, std::io::Error> {
            panic!("synthetic stderr capture panic");
        });

        let error = await_stderr_capture(Some(handle), &spool_path).unwrap_err();

        assert!(matches!(error, CaptureError::StderrCaptureThreadPanicked));
    }

    #[test]
    fn bundle_helpers_preserve_injected_flag_and_temp_paths_when_sarif_is_missing() {
        let bundle = CaptureBundle {
            plan: CapturePlan {
                execution_mode: ExecutionMode::Render,
                processing_path: diag_backend_probe::ProcessingPath::DualSinkStructured,
                structured_capture: StructuredCapturePolicy::SarifFile,
                native_text_capture: NativeTextCapturePolicy::CaptureOnly,
                preserve_native_color: true,
                locale_handling: LocaleHandling::ForceMessagesC,
                retention_policy: diag_trace::RetentionPolicy::Always,
            },
            invocation: CaptureInvocation {
                backend_path: "/usr/bin/gcc".to_string(),
                launcher_path: None,
                spawn_path: "/usr/bin/gcc".to_string(),
                argv: vec!["-c".to_string(), "main.c".to_string()],
                spawn_argv: vec!["-c".to_string(), "main.c".to_string()],
                argv_hash: "hash".to_string(),
                cwd: "/tmp/project".to_string(),
                selected_mode: ExecutionMode::Render,
                processing_path: diag_backend_probe::ProcessingPath::DualSinkStructured,
            },
            raw_text_artifacts: vec![CaptureArtifact {
                id: "stderr.raw".to_string(),
                kind: ArtifactKind::CompilerStderrText,
                media_type: "text/plain".to_string(),
                encoding: Some("utf-8".to_string()),
                digest_sha256: None,
                size_bytes: Some(6),
                storage: ArtifactStorage::Inline,
                inline_text: Some("stderr".to_string()),
                external_ref: None,
                produced_by: Some(tool_info(&fake_probe())),
            }],
            structured_artifacts: vec![CaptureArtifact {
                id: "diagnostics.sarif".to_string(),
                kind: ArtifactKind::GccSarif,
                media_type: "application/sarif+json".to_string(),
                encoding: Some("utf-8".to_string()),
                digest_sha256: None,
                size_bytes: None,
                storage: ArtifactStorage::Unavailable,
                inline_text: None,
                external_ref: None,
                produced_by: Some(tool_info(&fake_probe())),
            }],
            exit_status: ExitStatusInfo {
                code: Some(1),
                signal: None,
                success: false,
            },
            integrity_issues: Vec::new(),
        };

        let temp_dir = PathBuf::from("/tmp/runtime/formed-123");
        assert_eq!(
            bundle.authoritative_sarif_path(&temp_dir),
            Some(temp_dir.join("diagnostics.sarif"))
        );
        assert_eq!(
            bundle.injected_flags(&temp_dir),
            vec![
                format!(
                    "-fdiagnostics-add-output=sarif:version=2.1,file={}",
                    temp_dir.join("diagnostics.sarif").display()
                ),
                "-fdiagnostics-color=always".to_string(),
            ]
        );
        assert_eq!(
            bundle.temp_artifact_paths(&temp_dir),
            vec![
                temp_dir.clone(),
                temp_dir.join("invocation.json"),
                temp_dir.join("diagnostics.sarif"),
            ]
        );
        assert_eq!(bundle.stderr_text(), Some("stderr"));
        assert_eq!(bundle.capture_artifacts().len(), 2);
    }

    #[test]
    fn bundle_helpers_preserve_injected_flag_and_temp_paths_when_json_is_missing() {
        let bundle = CaptureBundle {
            plan: CapturePlan {
                execution_mode: ExecutionMode::Render,
                processing_path: diag_backend_probe::ProcessingPath::SingleSinkStructured,
                structured_capture: StructuredCapturePolicy::SingleSinkJsonFile,
                native_text_capture: NativeTextCapturePolicy::CaptureOnly,
                preserve_native_color: false,
                locale_handling: LocaleHandling::ForceMessagesC,
                retention_policy: diag_trace::RetentionPolicy::Always,
            },
            invocation: CaptureInvocation {
                backend_path: "/usr/bin/gcc".to_string(),
                launcher_path: None,
                spawn_path: "/usr/bin/gcc".to_string(),
                argv: vec!["-c".to_string(), "main.c".to_string()],
                spawn_argv: vec!["-c".to_string(), "main.c".to_string()],
                argv_hash: "hash".to_string(),
                cwd: "/tmp/project".to_string(),
                selected_mode: ExecutionMode::Render,
                processing_path: diag_backend_probe::ProcessingPath::SingleSinkStructured,
            },
            raw_text_artifacts: vec![CaptureArtifact {
                id: "stderr.raw".to_string(),
                kind: ArtifactKind::CompilerStderrText,
                media_type: "text/plain".to_string(),
                encoding: Some("utf-8".to_string()),
                digest_sha256: None,
                size_bytes: Some(6),
                storage: ArtifactStorage::Inline,
                inline_text: Some("stderr".to_string()),
                external_ref: None,
                produced_by: Some(tool_info(&fake_probe())),
            }],
            structured_artifacts: vec![CaptureArtifact {
                id: "diagnostics.json".to_string(),
                kind: ArtifactKind::GccJson,
                media_type: "application/json".to_string(),
                encoding: Some("utf-8".to_string()),
                digest_sha256: None,
                size_bytes: None,
                storage: ArtifactStorage::Unavailable,
                inline_text: None,
                external_ref: None,
                produced_by: Some(tool_info(&fake_probe())),
            }],
            exit_status: ExitStatusInfo {
                code: Some(1),
                signal: None,
                success: false,
            },
            integrity_issues: Vec::new(),
        };

        let temp_dir = PathBuf::from("/tmp/runtime/formed-123");
        assert_eq!(bundle.authoritative_sarif_path(&temp_dir), None);
        assert_eq!(
            bundle.injected_flags(&temp_dir),
            vec!["-fdiagnostics-format=json-file".to_string()]
        );
        assert_eq!(
            bundle.temp_artifact_paths(&temp_dir),
            vec![
                temp_dir.clone(),
                temp_dir.join("invocation.json"),
                temp_dir.join("diagnostics.json"),
            ]
        );
        assert_eq!(bundle.stderr_text(), Some("stderr"));
        assert_eq!(bundle.capture_artifacts().len(), 2);
    }

    #[test]
    fn invocation_record_honestly_reports_runtime_passthrough_conflict() {
        let request = CaptureRequest {
            backend: fake_probe(),
            args: vec![
                std::ffi::OsString::from("-c"),
                std::ffi::OsString::from("main.c"),
                std::ffi::OsString::from("-fdiagnostics-format=sarif-file"),
            ],
            cwd: PathBuf::from("/tmp/project"),
            mode: ExecutionMode::Render,
            capture_passthrough_stderr: false,
            retention: diag_trace::RetentionPolicy::OnWrapperFailure,
            paths: fake_paths(),
            structured_capture: StructuredCapturePolicy::SarifFile,
            preserve_native_color: true,
        };
        let plan = request.capture_plan();
        let final_args = request.args.clone();

        let record = build_invocation_record(
            &request,
            &plan,
            &final_args,
            &final_args,
            None,
            child_env_policy(&plan),
        );

        assert_eq!(record.selected_mode, ExecutionMode::Passthrough);
        assert_eq!(record.sarif_path, None);
        assert_eq!(record.normalized_invocation.diagnostics_flag_count, 1);
        assert_eq!(record.normalized_invocation.injected_flag_count, 0);
    }

    #[test]
    fn trace_sanitized_env_keys_follow_child_policy() {
        let render_keys = trace_sanitized_env_keys(ExecutionMode::Render);
        assert!(render_keys.iter().any(|key| key == "LC_MESSAGES"));
        assert!(render_keys.iter().any(|key| key == "GCC_DIAGNOSTICS_LOG"));

        let shadow_keys = trace_sanitized_env_keys(ExecutionMode::Shadow);
        assert!(!shadow_keys.iter().any(|key| key == "LC_MESSAGES"));
        assert!(shadow_keys.iter().any(|key| key == "GCC_DIAGNOSTICS_LOG"));

        let passthrough_keys = trace_sanitized_env_keys(ExecutionMode::Passthrough);
        assert!(passthrough_keys.is_empty());
    }

    #[test]
    fn normalizes_invocation_shape_for_trace_harvesting() {
        let normalized = normalize_invocation(
            &[
                "-c".to_string(),
                "-Iinclude".to_string(),
                "-DDEBUG=1".to_string(),
                "-o".to_string(),
                "main.o".to_string(),
                "-x".to_string(),
                "c++".to_string(),
                "main.cc".to_string(),
            ],
            8,
        );

        assert_eq!(normalized.arg_count, 8);
        assert_eq!(normalized.input_count, 1);
        assert!(normalized.compile_only);
        assert!(!normalized.preprocess_only);
        assert!(!normalized.assemble_only);
        assert!(normalized.output_requested);
        assert_eq!(normalized.language_override.as_deref(), Some("c++"));
        assert_eq!(normalized.include_path_count, 1);
        assert_eq!(normalized.define_count, 1);
        assert_eq!(normalized.diagnostics_flag_count, 0);
        assert_eq!(normalized.injected_flag_count, 0);
    }

    #[test]
    fn separated_option_values_do_not_count_as_inputs() {
        let normalized = normalize_invocation(
            &[
                "-c".to_string(),
                "-I".to_string(),
                "include".to_string(),
                "-D".to_string(),
                "DEBUG=1".to_string(),
                "-include".to_string(),
                "config.h".to_string(),
                "-MF".to_string(),
                "deps.d".to_string(),
                "main.cc".to_string(),
            ],
            10,
        );

        assert_eq!(normalized.input_count, 1);
        assert_eq!(normalized.include_path_count, 1);
        assert_eq!(normalized.define_count, 1);
        assert!(normalized.compile_only);
    }

    #[test]
    fn invocation_record_preserves_response_file_tokens_and_depfile_options_verbatim() {
        let request = CaptureRequest {
            backend: fake_probe(),
            args: vec![
                std::ffi::OsString::from("@build.rsp"),
                std::ffi::OsString::from("-E"),
                std::ffi::OsString::from("-MF"),
                std::ffi::OsString::from("deps.d"),
                std::ffi::OsString::from("main.c"),
            ],
            cwd: PathBuf::from("/tmp/project"),
            mode: ExecutionMode::Passthrough,
            capture_passthrough_stderr: false,
            retention: diag_trace::RetentionPolicy::Never,
            paths: fake_paths(),
            structured_capture: StructuredCapturePolicy::Disabled,
            preserve_native_color: false,
        };
        let plan = request.capture_plan();
        let final_args = request.args.clone();

        let record = build_invocation_record(
            &request,
            &plan,
            &final_args,
            &final_args,
            None,
            child_env_policy(&plan),
        );

        assert_eq!(
            record.argv,
            vec![
                "@build.rsp".to_string(),
                "-E".to_string(),
                "-MF".to_string(),
                "deps.d".to_string(),
                "main.c".to_string(),
            ]
        );
        assert_eq!(record.normalized_invocation.arg_count, 5);
        assert!(record.normalized_invocation.preprocess_only);
        assert_eq!(record.normalized_invocation.input_count, 2);
        assert_eq!(record.normalized_invocation.diagnostics_flag_count, 0);
    }

    #[test]
    fn unique_temp_dir_remains_unique_under_parallel_calls() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("runtime-root");
        let handles = (0..16)
            .map(|_| {
                let root = root.clone();
                thread::spawn(move || unique_temp_dir(&root).unwrap())
            })
            .collect::<Vec<_>>();

        let mut allocated = BTreeSet::new();
        for handle in handles {
            let path = handle.join().unwrap();
            assert!(path.exists());
            assert!(allocated.insert(path.file_name().unwrap().to_string_lossy().to_string()));
        }
        assert_eq!(allocated.len(), 16);
    }

    #[test]
    fn normalizes_single_sink_flag_as_injected_diagnostic_flag() {
        let normalized = normalize_invocation(
            &[
                "-c".to_string(),
                "main.c".to_string(),
                "-fdiagnostics-format=sarif-file".to_string(),
            ],
            2,
        );

        assert_eq!(normalized.diagnostics_flag_count, 1);
        assert_eq!(normalized.injected_flag_count, 1);
    }

    #[test]
    fn normalizes_json_single_sink_flag_as_injected_diagnostic_flag() {
        let normalized = normalize_invocation(
            &[
                "-c".to_string(),
                "main.c".to_string(),
                "-fdiagnostics-format=json-file".to_string(),
            ],
            2,
        );

        assert_eq!(normalized.diagnostics_flag_count, 1);
        assert_eq!(normalized.injected_flag_count, 1);
    }

    #[test]
    fn user_supplied_single_sink_flag_is_not_counted_as_injected() {
        let normalized = normalize_invocation(
            &[
                "-c".to_string(),
                "main.c".to_string(),
                "-fdiagnostics-format=sarif-file".to_string(),
            ],
            3,
        );

        assert_eq!(normalized.diagnostics_flag_count, 1);
        assert_eq!(normalized.injected_flag_count, 0);
    }
}
