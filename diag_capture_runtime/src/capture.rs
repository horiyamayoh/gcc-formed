//! Backend process spawning, stderr streaming, and capture orchestration.

use crate::artifact::{CaptureError, CaptureOutcome};
use crate::artifact_builder::{
    CapturedStderr, authoritative_structured_path, build_artifacts, build_capture_bundle,
    build_invocation_record, discover_structured_artifact, preserve_discovered_artifact,
    single_sink_structured_capture, snapshot_structured_artifacts, status_to_info,
    write_invocation_record,
};
use crate::policy::{
    CaptureRequest, NativeTextCapturePolicy, StructuredCapturePolicy, apply_child_env_policy,
    child_env_policy,
};
use crate::{STDERR_CAPTURE_BUFFER_BYTES, STDERR_CAPTURE_ID, STDERR_CAPTURE_PREVIEW_LIMIT_BYTES};
use diag_core::fingerprint_for;
use diag_trace::{secure_private_dir, secure_private_file, should_retain};
use std::ffi::OsString;
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;
use std::time::Instant;

/// Executes the backend compiler and captures diagnostic artifacts.
pub fn run_capture(request: &CaptureRequest) -> Result<CaptureOutcome, CaptureError> {
    let capture_started = Instant::now();
    let plan = request.capture_plan();
    request.paths.ensure_dirs()?;
    let temp_dir_path = unique_temp_dir(&request.paths.runtime_root)?;
    let stderr_spool_path = temp_dir_path.join(STDERR_CAPTURE_ID);
    let expected_structured_path =
        authoritative_structured_path(plan.structured_capture, &temp_dir_path);
    let injected_sarif_path = match plan.structured_capture {
        StructuredCapturePolicy::SarifFile => expected_structured_path.clone(),
        StructuredCapturePolicy::Disabled
        | StructuredCapturePolicy::SingleSinkSarifFile
        | StructuredCapturePolicy::SingleSinkJsonFile => None,
    };
    let single_sink_snapshot = if single_sink_structured_capture(plan.structured_capture) {
        Some(snapshot_structured_artifacts(
            &request.cwd,
            plan.structured_capture,
        )?)
    } else {
        None
    };

    let mut command = Command::new(request.backend.spawn_path());
    command.current_dir(&request.cwd);
    command.stdin(Stdio::inherit());
    command.stdout(Stdio::inherit());
    let env_policy = child_env_policy(&plan);
    apply_child_env_policy(&mut command, &env_policy);

    let mut final_args = request.args.clone();
    match plan.structured_capture {
        StructuredCapturePolicy::Disabled => {}
        StructuredCapturePolicy::SarifFile => {
            if let Some(sarif_path) = injected_sarif_path.as_ref() {
                final_args.push(OsString::from(format!(
                    "-fdiagnostics-add-output=sarif:version=2.1,file={}",
                    sarif_path.display()
                )));
            }
        }
        StructuredCapturePolicy::SingleSinkSarifFile => {
            final_args.push(OsString::from("-fdiagnostics-format=sarif-file"));
        }
        StructuredCapturePolicy::SingleSinkJsonFile => {
            final_args.push(OsString::from("-fdiagnostics-format=json-file"));
        }
    }
    let invocation_path = temp_dir_path.join("invocation.json");
    let spawn_args = request.backend.spawn_args(&final_args);
    write_invocation_record(
        &invocation_path,
        &build_invocation_record(
            request,
            &plan,
            &final_args,
            &spawn_args,
            injected_sarif_path.as_deref(),
            env_policy,
        ),
    )?;
    command.args(&spawn_args);

    let stderr_mode = match plan.native_text_capture {
        NativeTextCapturePolicy::Passthrough => Stdio::inherit(),
        NativeTextCapturePolicy::CaptureOnly | NativeTextCapturePolicy::TeeToParent => {
            Stdio::piped()
        }
    };
    command.stderr(stderr_mode);
    let mut child = command.spawn().map_err(|_| CaptureError::Spawn)?;

    let stderr_handle = match plan.native_text_capture {
        NativeTextCapturePolicy::Passthrough => None,
        NativeTextCapturePolicy::CaptureOnly => child
            .stderr
            .take()
            .map(|stderr| spawn_capture_reader(stderr, stderr_spool_path.clone())),
        NativeTextCapturePolicy::TeeToParent => child
            .stderr
            .take()
            .map(|stderr| spawn_tee_reader(stderr, stderr_spool_path.clone())),
    };

    let status = child.wait()?;
    if let Some(path) = injected_sarif_path.as_ref().filter(|path| path.exists()) {
        secure_private_file(path)?;
    }
    let stderr_capture = await_stderr_capture(stderr_handle, &stderr_spool_path)?;
    if stderr_capture.spool_path.exists() {
        secure_private_file(&stderr_capture.spool_path)?;
    }
    let stderr_bytes = stderr_capture.preview_bytes.clone();

    let single_sink_structured_path = if let Some(snapshot) = single_sink_snapshot.as_ref() {
        discover_structured_artifact(&request.cwd, snapshot, plan.structured_capture)?
            .map(|artifact| {
                preserve_discovered_artifact(&artifact, &temp_dir_path, plan.structured_capture)
            })
            .transpose()?
    } else {
        None
    };
    let structured_path = match plan.structured_capture {
        StructuredCapturePolicy::Disabled => None,
        StructuredCapturePolicy::SarifFile => injected_sarif_path,
        StructuredCapturePolicy::SingleSinkSarifFile
        | StructuredCapturePolicy::SingleSinkJsonFile => {
            single_sink_structured_path.or(expected_structured_path)
        }
    };

    let child_failed = !status.success();
    let retained = should_retain(plan.retention_policy, false, child_failed);
    let retained_trace_dir = if retained {
        let trace_dir = request.paths.trace_root.join(temp_dir_name(&temp_dir_path));
        if trace_dir.exists() {
            fs::remove_dir_all(&trace_dir)?;
        }
        fs::create_dir_all(&trace_dir)?;
        secure_private_dir(&trace_dir)?;
        if stderr_capture.total_bytes > 0 {
            let retained_stderr = trace_dir.join(STDERR_CAPTURE_ID);
            if stderr_capture.spool_path.exists() {
                fs::copy(&stderr_capture.spool_path, &retained_stderr)?;
            } else {
                fs::write(&retained_stderr, &stderr_bytes)?;
            }
            secure_private_file(&retained_stderr)?;
        }
        if let Some(structured) = structured_path.as_ref().filter(|path| path.exists()) {
            let retained_structured = trace_dir.join(
                structured
                    .file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or("diagnostics.bin"),
            );
            fs::copy(structured, &retained_structured)?;
            secure_private_file(&retained_structured)?;
        }
        if invocation_path.exists() {
            let retained_invocation = trace_dir.join("invocation.json");
            fs::copy(&invocation_path, &retained_invocation)?;
            secure_private_file(&retained_invocation)?;
        }
        Some(trace_dir)
    } else {
        None
    };

    let exit_status = status_to_info(status);
    let artifacts = build_artifacts(&stderr_capture, structured_path.as_ref(), &request.backend);
    let integrity_issues = stderr_capture.integrity_issues();
    let bundle = build_capture_bundle(
        request,
        &final_args,
        &spawn_args,
        &plan,
        &exit_status,
        &artifacts,
        &integrity_issues,
    );

    Ok(CaptureOutcome {
        exit_status,
        stderr_bytes,
        sarif_path: matches!(
            plan.structured_capture,
            StructuredCapturePolicy::SarifFile | StructuredCapturePolicy::SingleSinkSarifFile
        )
        .then_some(structured_path.clone())
        .flatten(),
        temp_dir: temp_dir_path,
        capture_duration_ms: capture_started.elapsed().as_millis() as u64,
        retained,
        retained_trace_dir,
        artifacts,
        bundle,
    })
}

/// Removes temporary capture files unless they were retained for tracing.
pub fn cleanup_capture(outcome: &CaptureOutcome) -> Result<(), std::io::Error> {
    if outcome.retained {
        return Ok(());
    }
    if outcome.temp_dir.exists() {
        fs::remove_dir_all(&outcome.temp_dir)?;
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// pub(crate) helpers
// ---------------------------------------------------------------------------

pub(crate) fn capture_stderr_stream(
    reader: &mut impl Read,
    spool_path: &Path,
    mut tee: Option<&mut dyn Write>,
) -> Result<CapturedStderr, std::io::Error> {
    let mut spool = fs::File::create(spool_path)?;
    let mut preview_bytes = Vec::new();
    let mut total_bytes = 0_u64;
    let mut buffer = [0_u8; STDERR_CAPTURE_BUFFER_BYTES];

    loop {
        let read = reader.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        let chunk = &buffer[..read];
        spool.write_all(chunk)?;
        if let Some(tee) = tee.as_mut() {
            tee.write_all(chunk)?;
            tee.flush()?;
        }
        total_bytes += read as u64;
        let remaining_preview =
            STDERR_CAPTURE_PREVIEW_LIMIT_BYTES.saturating_sub(preview_bytes.len());
        if remaining_preview > 0 {
            let preview_len = remaining_preview.min(read);
            preview_bytes.extend_from_slice(&chunk[..preview_len]);
        }
    }
    spool.flush()?;

    Ok(CapturedStderr {
        truncated_bytes: total_bytes.saturating_sub(preview_bytes.len() as u64),
        preview_bytes,
        total_bytes,
        spool_path: spool_path.to_path_buf(),
    })
}

pub(crate) fn await_stderr_capture(
    stderr_handle: Option<thread::JoinHandle<Result<CapturedStderr, std::io::Error>>>,
    stderr_spool_path: &Path,
) -> Result<CapturedStderr, CaptureError> {
    match stderr_handle {
        Some(handle) => match handle.join() {
            Ok(Ok(captured)) => Ok(captured),
            Ok(Err(error)) => Err(CaptureError::StderrCapture(error)),
            Err(_) => Err(CaptureError::StderrCaptureThreadPanicked),
        },
        None => Ok(CapturedStderr::empty(stderr_spool_path.to_path_buf())),
    }
}

pub(crate) fn spawn_capture_reader(
    stderr: std::process::ChildStderr,
    spool_path: PathBuf,
) -> thread::JoinHandle<Result<CapturedStderr, std::io::Error>> {
    thread::spawn(move || -> Result<CapturedStderr, std::io::Error> {
        let mut reader = stderr;
        capture_stderr_stream(&mut reader, &spool_path, None)
    })
}

pub(crate) fn spawn_tee_reader(
    stderr: std::process::ChildStderr,
    spool_path: PathBuf,
) -> thread::JoinHandle<Result<CapturedStderr, std::io::Error>> {
    thread::spawn(move || -> Result<CapturedStderr, std::io::Error> {
        let mut reader = stderr;
        let mut tee = std::io::stderr().lock();
        capture_stderr_stream(&mut reader, &spool_path, Some(&mut tee))
    })
}

pub(crate) fn path_is_safe_for_gcc_output(path: &Path) -> bool {
    path.to_string_lossy()
        .chars()
        .all(|ch| !matches!(ch, ',' | '=' | ' ') && !ch.is_control())
}

pub(crate) fn safe_runtime_fallback_bases() -> Vec<PathBuf> {
    let mut bases = vec![std::env::temp_dir().join("cc-formed-runtime")];
    #[cfg(unix)]
    {
        let unix_tmp = PathBuf::from("/tmp/cc-formed-runtime");
        if !bases.iter().any(|base| base == &unix_tmp) {
            bases.push(unix_tmp);
        }
    }
    bases
}

pub(crate) fn safe_runtime_root(root: &Path) -> Result<PathBuf, std::io::Error> {
    if path_is_safe_for_gcc_output(root) {
        return Ok(root.to_path_buf());
    }

    let root_fingerprint = fingerprint_for(&[root.display().to_string()]);
    let suffix = &root_fingerprint[..12];
    for base in safe_runtime_fallback_bases() {
        let candidate = base.join(format!("cc-formed-safe-{suffix}"));
        if path_is_safe_for_gcc_output(&candidate) {
            return Ok(candidate);
        }
    }

    Err(std::io::Error::new(
        std::io::ErrorKind::InvalidInput,
        format!(
            "could not derive a safe diagnostics runtime root from {}",
            root.display()
        ),
    ))
}

pub(crate) fn temp_dir_name(path: &Path) -> String {
    path.file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("trace")
        .to_string()
}

pub(crate) fn unique_temp_dir(root: &Path) -> Result<PathBuf, std::io::Error> {
    let safe_root = safe_runtime_root(root)?;
    fs::create_dir_all(&safe_root)?;
    secure_private_dir(&safe_root)?;
    let unique = format!(
        "formed-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or_default()
    );
    let path = safe_root.join(unique);
    fs::create_dir_all(&path)?;
    secure_private_dir(&path)?;
    Ok(path)
}
