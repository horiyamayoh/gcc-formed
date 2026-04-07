use diag_backend_probe::ProbeResult;
use diag_core::{ArtifactKind, ArtifactStorage, CaptureArtifact, ToolInfo};
use diag_trace::{RetentionPolicy, WrapperPaths, should_retain};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::env;
use std::ffi::OsString;
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus, Stdio};
use std::thread;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionMode {
    Render,
    Shadow,
    Passthrough,
}

#[derive(Debug, Clone)]
pub struct CaptureRequest {
    pub backend: ProbeResult,
    pub args: Vec<OsString>,
    pub cwd: PathBuf,
    pub mode: ExecutionMode,
    pub retention: RetentionPolicy,
    pub paths: WrapperPaths,
    pub inject_sarif: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExitStatusInfo {
    pub code: Option<i32>,
    pub signal: Option<i32>,
    pub success: bool,
}

#[derive(Debug)]
pub struct CaptureOutcome {
    pub exit_status: ExitStatusInfo,
    pub stderr_bytes: Vec<u8>,
    pub sarif_path: Option<PathBuf>,
    pub temp_dir: PathBuf,
    pub retained: bool,
    pub retained_trace_dir: Option<PathBuf>,
    pub artifacts: Vec<CaptureArtifact>,
}

#[derive(Debug, thiserror::Error)]
pub enum CaptureError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("failed to spawn backend command")]
    Spawn,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct InvocationRecord {
    backend_path: String,
    argv: Vec<String>,
    selected_mode: ExecutionMode,
    cwd: String,
    sarif_path: Option<String>,
    #[serde(skip_serializing_if = "child_env_policy_is_empty", default)]
    child_env_policy: ChildEnvPolicy,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    wrapper_env: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
struct ChildEnvPolicy {
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    set: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    unset: Vec<String>,
}

pub fn run_capture(request: &CaptureRequest) -> Result<CaptureOutcome, CaptureError> {
    request.paths.ensure_dirs()?;
    let temp_dir_path = unique_temp_dir(&request.paths.runtime_root)?;
    let sarif_path = if request.inject_sarif {
        Some(temp_dir_path.join("diagnostics.sarif"))
    } else {
        None
    };

    let mut command = Command::new(&request.backend.resolved_path);
    command.current_dir(&request.cwd);
    command.stdin(Stdio::inherit());
    command.stdout(Stdio::inherit());
    let child_env_policy = child_env_policy(request.mode);
    apply_child_env_policy(&mut command, &child_env_policy);

    let mut final_args = request.args.clone();
    if let Some(sarif_path) = sarif_path.as_ref() {
        final_args.push(OsString::from(format!(
            "-fdiagnostics-add-output=sarif:version=2.1,file={}",
            sanitize_sarif_path(sarif_path)
        )));
    }
    let invocation_path = temp_dir_path.join("invocation.json");
    write_invocation_record(
        &invocation_path,
        &build_invocation_record(
            request,
            &final_args,
            sarif_path.as_deref(),
            child_env_policy,
        ),
    )?;
    command.args(final_args);

    let stderr_mode = match request.mode {
        ExecutionMode::Passthrough => Stdio::inherit(),
        ExecutionMode::Render | ExecutionMode::Shadow => Stdio::piped(),
    };
    command.stderr(stderr_mode);
    let mut child = command.spawn().map_err(|_| CaptureError::Spawn)?;

    let stderr_handle = match request.mode {
        ExecutionMode::Passthrough => None,
        ExecutionMode::Render => child.stderr.take().map(|stderr| {
            thread::spawn(move || -> Result<Vec<u8>, std::io::Error> {
                let mut reader = stderr;
                let mut buffer = Vec::new();
                reader.read_to_end(&mut buffer)?;
                Ok(buffer)
            })
        }),
        ExecutionMode::Shadow => child.stderr.take().map(|stderr| {
            thread::spawn(move || -> Result<Vec<u8>, std::io::Error> {
                let mut reader = stderr;
                let mut buffer = [0_u8; 4096];
                let mut captured = Vec::new();
                let mut tee = std::io::stderr().lock();
                loop {
                    let read = reader.read(&mut buffer)?;
                    if read == 0 {
                        break;
                    }
                    tee.write_all(&buffer[..read])?;
                    tee.flush()?;
                    captured.extend_from_slice(&buffer[..read]);
                }
                Ok(captured)
            })
        }),
    };

    let status = child.wait()?;
    let stderr_bytes = match stderr_handle {
        Some(handle) => handle
            .join()
            .unwrap_or_else(|_| Ok(Vec::new()))
            .unwrap_or_default(),
        None => Vec::new(),
    };

    let child_failed = !status.success();
    let retained = should_retain(request.retention, false, child_failed);
    let retained_trace_dir = if retained {
        let trace_dir = request.paths.trace_root.join(temp_dir_name(&temp_dir_path));
        if trace_dir.exists() {
            fs::remove_dir_all(&trace_dir)?;
        }
        fs::create_dir_all(&trace_dir)?;
        fs::write(trace_dir.join("stderr.raw"), &stderr_bytes)?;
        if let Some(sarif) = sarif_path.as_ref().filter(|path| path.exists()) {
            fs::copy(sarif, trace_dir.join("diagnostics.sarif"))?;
        }
        if invocation_path.exists() {
            fs::copy(&invocation_path, trace_dir.join("invocation.json"))?;
        }
        Some(trace_dir)
    } else {
        None
    };

    let artifacts = build_artifacts(&stderr_bytes, sarif_path.as_ref(), &request.backend);

    Ok(CaptureOutcome {
        exit_status: status_to_info(status),
        stderr_bytes,
        sarif_path,
        temp_dir: temp_dir_path,
        retained,
        retained_trace_dir,
        artifacts,
    })
}

pub fn cleanup_capture(outcome: &CaptureOutcome) -> Result<(), std::io::Error> {
    if outcome.retained {
        return Ok(());
    }
    if outcome.temp_dir.exists() {
        fs::remove_dir_all(&outcome.temp_dir)?;
    }
    Ok(())
}

fn build_artifacts(
    stderr_bytes: &[u8],
    sarif_path: Option<&PathBuf>,
    backend: &ProbeResult,
) -> Vec<CaptureArtifact> {
    let mut artifacts = Vec::new();
    if !stderr_bytes.is_empty() {
        artifacts.push(CaptureArtifact {
            id: "stderr.raw".to_string(),
            kind: ArtifactKind::CompilerStderrText,
            media_type: "text/plain".to_string(),
            encoding: Some("utf-8".to_string()),
            digest_sha256: None,
            size_bytes: Some(stderr_bytes.len() as u64),
            storage: ArtifactStorage::Inline,
            inline_text: Some(String::from_utf8_lossy(stderr_bytes).to_string()),
            external_ref: None,
            produced_by: Some(tool_info(backend)),
        });
    }
    if let Some(path) = sarif_path {
        artifacts.push(CaptureArtifact {
            id: "diagnostics.sarif".to_string(),
            kind: ArtifactKind::GccSarif,
            media_type: "application/sarif+json".to_string(),
            encoding: Some("utf-8".to_string()),
            digest_sha256: None,
            size_bytes: fs::metadata(path).ok().map(|metadata| metadata.len()),
            storage: if path.exists() {
                ArtifactStorage::ExternalRef
            } else {
                ArtifactStorage::Unavailable
            },
            inline_text: None,
            external_ref: if path.exists() {
                Some(path.display().to_string())
            } else {
                None
            },
            produced_by: Some(tool_info(backend)),
        });
    }
    artifacts
}

fn tool_info(backend: &ProbeResult) -> ToolInfo {
    ToolInfo {
        name: backend
            .resolved_path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("gcc")
            .to_string(),
        version: Some(backend.version_string.clone()),
        component: None,
        vendor: Some("GNU".to_string()),
    }
}

fn sanitize_sarif_path(path: &Path) -> String {
    path.display()
        .to_string()
        .replace(',', "_")
        .replace('=', "_")
        .replace(' ', "_")
}

fn temp_dir_name(path: &Path) -> String {
    path.file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("trace")
        .to_string()
}

fn unique_temp_dir(root: &Path) -> Result<PathBuf, std::io::Error> {
    let unique = format!(
        "formed-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or_default()
    );
    let path = root.join(unique);
    fs::create_dir_all(&path)?;
    Ok(path)
}

fn build_invocation_record(
    request: &CaptureRequest,
    final_args: &[OsString],
    sarif_path: Option<&Path>,
    child_env_policy: ChildEnvPolicy,
) -> InvocationRecord {
    InvocationRecord {
        backend_path: request.backend.resolved_path.display().to_string(),
        argv: final_args
            .iter()
            .map(|arg| arg.to_string_lossy().into_owned())
            .collect(),
        selected_mode: request.mode,
        cwd: request.cwd.display().to_string(),
        sarif_path: sarif_path.map(|path| path.display().to_string()),
        child_env_policy,
        wrapper_env: collect_wrapper_env(),
    }
}

fn child_env_policy(mode: ExecutionMode) -> ChildEnvPolicy {
    let mut policy = ChildEnvPolicy::default();
    if matches!(mode, ExecutionMode::Render) {
        policy
            .set
            .insert("LC_MESSAGES".to_string(), "C".to_string());
    }
    if matches!(mode, ExecutionMode::Render | ExecutionMode::Shadow) {
        policy.unset = vec![
            "EXPERIMENTAL_SARIF_SOCKET".to_string(),
            "GCC_DIAGNOSTICS_LOG".to_string(),
            "GCC_EXTRA_DIAGNOSTIC_OUTPUT".to_string(),
        ];
    }
    policy
}

fn apply_child_env_policy(command: &mut Command, policy: &ChildEnvPolicy) {
    for (key, value) in &policy.set {
        command.env(key, value);
    }
    for key in &policy.unset {
        command.env_remove(key);
    }
}

fn child_env_policy_is_empty(policy: &ChildEnvPolicy) -> bool {
    policy.set.is_empty() && policy.unset.is_empty()
}

fn collect_wrapper_env() -> BTreeMap<String, String> {
    const KEYS: &[&str] = &[
        "FORMED_BACKEND_GCC",
        "FORMED_CACHE_DIR",
        "FORMED_CONFIG_DIR",
        "FORMED_CONFIG_FILE",
        "FORMED_INSTALL_ROOT",
        "FORMED_RUNTIME_DIR",
        "FORMED_STATE_DIR",
        "FORMED_TRACE_DIR",
    ];

    let mut env_subset = BTreeMap::new();
    for key in KEYS {
        if let Some(value) = env::var_os(key) {
            env_subset.insert((*key).to_string(), value.to_string_lossy().into_owned());
        }
    }
    env_subset
}

fn write_invocation_record(path: &Path, record: &InvocationRecord) -> Result<(), std::io::Error> {
    let json = serde_json::to_vec_pretty(record).map_err(std::io::Error::other)?;
    fs::write(path, json)
}

fn status_to_info(status: ExitStatus) -> ExitStatusInfo {
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        ExitStatusInfo {
            code: status.code(),
            signal: status.signal(),
            success: status.success(),
        }
    }
    #[cfg(not(unix))]
    {
        ExitStatusInfo {
            code: status.code(),
            signal: None,
            success: status.success(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use diag_backend_probe::{DriverKind, ProbeKey, SupportTier};
    use std::path::PathBuf;

    fn fake_probe() -> ProbeResult {
        ProbeResult {
            requested_backend: "gcc-formed".to_string(),
            resolved_path: PathBuf::from("/usr/bin/gcc"),
            version_string: "gcc (GCC) 15.1.0".to_string(),
            major: 15,
            minor: 1,
            support_tier: SupportTier::A,
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

    #[test]
    fn sanitizes_sarif_path() {
        let sanitized = sanitize_sarif_path(Path::new("/tmp/a,b=c d.sarif"));
        assert!(!sanitized.contains(','));
        assert!(!sanitized.contains('='));
        assert!(!sanitized.contains(' '));
    }

    #[test]
    fn creates_inline_stderr_artifact() {
        let artifacts = build_artifacts(b"stderr", None, &fake_probe());
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
            args: vec![OsString::from("-c"), OsString::from("src/main.c")],
            cwd: PathBuf::from("/tmp/project"),
            mode: ExecutionMode::Render,
            retention: RetentionPolicy::Always,
            paths: WrapperPaths {
                config_path: PathBuf::from("/tmp/config.toml"),
                cache_root: PathBuf::from("/tmp/cache"),
                state_root: PathBuf::from("/tmp/state"),
                runtime_root: PathBuf::from("/tmp/runtime"),
                trace_root: PathBuf::from("/tmp/traces"),
                install_root: PathBuf::from("/tmp/install"),
            },
            inject_sarif: true,
        };

        let record = build_invocation_record(
            &request,
            &[
                OsString::from("-c"),
                OsString::from("src/main.c"),
                OsString::from(
                    "-fdiagnostics-add-output=sarif:version=2.1,file=/tmp/runtime/diagnostics.sarif",
                ),
            ],
            Some(Path::new("/tmp/runtime/diagnostics.sarif")),
            child_env_policy(ExecutionMode::Render),
        );

        assert_eq!(record.selected_mode, ExecutionMode::Render);
        assert_eq!(record.backend_path, "/usr/bin/gcc");
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
        let policy = child_env_policy(ExecutionMode::Render);
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
        let policy = child_env_policy(ExecutionMode::Shadow);
        assert!(policy.set.is_empty());
        assert_eq!(policy.unset.len(), 3);
    }

    #[test]
    fn passthrough_mode_preserves_environment() {
        let policy = child_env_policy(ExecutionMode::Passthrough);
        assert!(child_env_policy_is_empty(&policy));
    }
}
