use diag_backend_probe::{ProbeResult, ProcessingPath};
use diag_core::{
    ArtifactKind, ArtifactStorage, CaptureArtifact, IntegrityIssue, ToolInfo, fingerprint_for,
};
use diag_trace::{
    RetentionPolicy, WrapperPaths, secure_private_dir, secure_private_file, should_retain,
};
use serde::{Deserialize, Serialize};
use std::borrow::Cow;
use std::collections::BTreeMap;
use std::env;
use std::ffi::OsString;
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus, Stdio};
use std::thread;
use std::time::Instant;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionMode {
    Render,
    Shadow,
    Passthrough,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StructuredCapturePolicy {
    Disabled,
    SarifFile,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NativeTextCapturePolicy {
    Passthrough,
    CaptureOnly,
    TeeToParent,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LocaleHandling {
    Preserve,
    ForceMessagesC,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct CapturePlan {
    pub execution_mode: ExecutionMode,
    pub processing_path: ProcessingPath,
    pub structured_capture: StructuredCapturePolicy,
    pub native_text_capture: NativeTextCapturePolicy,
    pub preserve_native_color: bool,
    pub locale_handling: LocaleHandling,
    pub retention_policy: RetentionPolicy,
}

#[derive(Debug, Clone)]
pub struct CaptureRequest {
    pub backend: ProbeResult,
    pub args: Vec<OsString>,
    pub cwd: PathBuf,
    pub mode: ExecutionMode,
    pub capture_passthrough_stderr: bool,
    pub retention: RetentionPolicy,
    pub paths: WrapperPaths,
    pub inject_sarif: bool,
    pub preserve_native_color: bool,
}

impl CaptureRequest {
    pub fn capture_plan(&self) -> CapturePlan {
        CapturePlan {
            execution_mode: self.mode,
            processing_path: match self.mode {
                ExecutionMode::Passthrough => ProcessingPath::Passthrough,
                _ if self.inject_sarif => ProcessingPath::DualSinkStructured,
                _ => ProcessingPath::NativeTextCapture,
            },
            structured_capture: if self.inject_sarif {
                StructuredCapturePolicy::SarifFile
            } else {
                StructuredCapturePolicy::Disabled
            },
            native_text_capture: match self.mode {
                ExecutionMode::Passthrough if self.capture_passthrough_stderr => {
                    NativeTextCapturePolicy::TeeToParent
                }
                ExecutionMode::Passthrough => NativeTextCapturePolicy::Passthrough,
                ExecutionMode::Render => NativeTextCapturePolicy::CaptureOnly,
                ExecutionMode::Shadow => NativeTextCapturePolicy::TeeToParent,
            },
            preserve_native_color: self.preserve_native_color,
            locale_handling: if matches!(self.mode, ExecutionMode::Render) {
                LocaleHandling::ForceMessagesC
            } else {
                LocaleHandling::Preserve
            },
            retention_policy: self.retention,
        }
    }

    pub fn from_plan(
        backend: ProbeResult,
        args: Vec<OsString>,
        cwd: PathBuf,
        paths: WrapperPaths,
        plan: CapturePlan,
    ) -> Self {
        Self {
            backend,
            args,
            cwd,
            mode: plan.execution_mode,
            capture_passthrough_stderr: matches!(
                (plan.execution_mode, plan.native_text_capture),
                (
                    ExecutionMode::Passthrough,
                    NativeTextCapturePolicy::TeeToParent
                )
            ),
            retention: plan.retention_policy,
            paths,
            inject_sarif: matches!(plan.structured_capture, StructuredCapturePolicy::SarifFile),
            preserve_native_color: plan.preserve_native_color,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ExitStatusInfo {
    pub code: Option<i32>,
    pub signal: Option<i32>,
    pub success: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CaptureInvocation {
    pub backend_path: String,
    pub argv: Vec<String>,
    pub argv_hash: String,
    pub cwd: String,
    pub selected_mode: ExecutionMode,
    pub processing_path: ProcessingPath,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CaptureBundle {
    pub plan: CapturePlan,
    pub invocation: CaptureInvocation,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub raw_text_artifacts: Vec<CaptureArtifact>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub structured_artifacts: Vec<CaptureArtifact>,
    pub exit_status: ExitStatusInfo,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub integrity_issues: Vec<IntegrityIssue>,
}

impl CaptureBundle {
    pub fn capture_artifacts(&self) -> Vec<CaptureArtifact> {
        let mut artifacts =
            Vec::with_capacity(self.raw_text_artifacts.len() + self.structured_artifacts.len());
        artifacts.extend(self.raw_text_artifacts.clone());
        artifacts.extend(self.structured_artifacts.clone());
        artifacts
    }

    pub fn stderr_text(&self) -> Option<&str> {
        self.raw_text_artifacts.iter().find_map(|artifact| {
            matches!(
                artifact.kind,
                ArtifactKind::CompilerStderrText | ArtifactKind::LinkerStderrText
            )
            .then(|| artifact.inline_text.as_deref())
            .flatten()
        })
    }

    pub fn authoritative_sarif_path(&self, temp_dir: &Path) -> Option<PathBuf> {
        match self.plan.structured_capture {
            StructuredCapturePolicy::Disabled => None,
            StructuredCapturePolicy::SarifFile => Some(temp_dir.join("diagnostics.sarif")),
        }
    }

    pub fn injected_flags(&self, temp_dir: &Path) -> Vec<String> {
        let mut flags = Vec::new();
        if let Some(path) = self.authoritative_sarif_path(temp_dir) {
            flags.push(format!(
                "-fdiagnostics-add-output=sarif:version=2.1,file={}",
                sanitize_sarif_path(&path)
            ));
        }
        if self.plan.preserve_native_color {
            flags.push("-fdiagnostics-color=always".to_string());
        }
        flags
    }

    pub fn temp_artifact_paths(&self, temp_dir: &Path) -> Vec<PathBuf> {
        let mut paths = vec![temp_dir.to_path_buf(), temp_dir.join("invocation.json")];
        if let Some(path) = self.authoritative_sarif_path(temp_dir) {
            paths.push(path);
        }
        paths
    }
}

#[derive(Debug)]
pub struct CaptureOutcome {
    pub exit_status: ExitStatusInfo,
    pub stderr_bytes: Vec<u8>,
    pub sarif_path: Option<PathBuf>,
    pub temp_dir: PathBuf,
    pub capture_duration_ms: u64,
    pub retained: bool,
    pub retained_trace_dir: Option<PathBuf>,
    pub artifacts: Vec<CaptureArtifact>,
    pub bundle: CaptureBundle,
}

impl CaptureOutcome {
    pub fn capture_artifacts(&self) -> Vec<CaptureArtifact> {
        self.bundle.capture_artifacts()
    }

    pub fn stderr_text(&self) -> Cow<'_, str> {
        self.bundle
            .stderr_text()
            .map(Cow::Borrowed)
            .unwrap_or_else(|| String::from_utf8_lossy(&self.stderr_bytes))
    }

    pub fn authoritative_sarif_path(&self) -> Option<PathBuf> {
        self.bundle.authoritative_sarif_path(&self.temp_dir)
    }

    pub fn processing_path(&self) -> ProcessingPath {
        self.bundle.plan.processing_path
    }

    pub fn sanitized_env_keys(&self) -> Vec<String> {
        trace_sanitized_env_keys(self.bundle.plan.execution_mode)
    }

    pub fn injected_flags(&self) -> Vec<String> {
        self.bundle.injected_flags(&self.temp_dir)
    }

    pub fn temp_artifact_paths(&self) -> Vec<PathBuf> {
        self.bundle.temp_artifact_paths(&self.temp_dir)
    }
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
    argv_hash: String,
    normalized_invocation: NormalizedInvocation,
    redaction_class: String,
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct NormalizedInvocation {
    arg_count: usize,
    input_count: usize,
    compile_only: bool,
    preprocess_only: bool,
    assemble_only: bool,
    output_requested: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    language_override: Option<String>,
    include_path_count: usize,
    define_count: usize,
    diagnostics_flag_count: usize,
    injected_flag_count: usize,
}

pub fn run_capture(request: &CaptureRequest) -> Result<CaptureOutcome, CaptureError> {
    let capture_started = Instant::now();
    let plan = request.capture_plan();
    request.paths.ensure_dirs()?;
    let temp_dir_path = unique_temp_dir(&request.paths.runtime_root)?;
    let sarif_path = match plan.structured_capture {
        StructuredCapturePolicy::Disabled => None,
        StructuredCapturePolicy::SarifFile => Some(temp_dir_path.join("diagnostics.sarif")),
    };

    let mut command = Command::new(&request.backend.resolved_path);
    command.current_dir(&request.cwd);
    command.stdin(Stdio::inherit());
    command.stdout(Stdio::inherit());
    let child_env_policy = child_env_policy(&plan);
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
    command.args(&final_args);

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
        NativeTextCapturePolicy::CaptureOnly => child.stderr.take().map(|stderr| {
            thread::spawn(move || -> Result<Vec<u8>, std::io::Error> {
                let mut reader = stderr;
                let mut buffer = Vec::new();
                reader.read_to_end(&mut buffer)?;
                Ok(buffer)
            })
        }),
        NativeTextCapturePolicy::TeeToParent => child.stderr.take().map(spawn_tee_reader),
    };

    let status = child.wait()?;
    if let Some(path) = sarif_path.as_ref().filter(|path| path.exists()) {
        secure_private_file(path)?;
    }
    let stderr_bytes = match stderr_handle {
        Some(handle) => handle
            .join()
            .unwrap_or_else(|_| Ok(Vec::new()))
            .unwrap_or_default(),
        None => Vec::new(),
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
        let retained_stderr = trace_dir.join("stderr.raw");
        fs::write(&retained_stderr, &stderr_bytes)?;
        secure_private_file(&retained_stderr)?;
        if let Some(sarif) = sarif_path.as_ref().filter(|path| path.exists()) {
            let retained_sarif = trace_dir.join("diagnostics.sarif");
            fs::copy(sarif, &retained_sarif)?;
            secure_private_file(&retained_sarif)?;
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
    let artifacts = build_artifacts(&stderr_bytes, sarif_path.as_ref(), &request.backend);
    let bundle = build_capture_bundle(request, &final_args, &plan, &exit_status, &artifacts);

    Ok(CaptureOutcome {
        exit_status,
        stderr_bytes,
        sarif_path,
        temp_dir: temp_dir_path,
        capture_duration_ms: capture_started.elapsed().as_millis() as u64,
        retained,
        retained_trace_dir,
        artifacts,
        bundle,
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

fn spawn_tee_reader(
    stderr: std::process::ChildStderr,
) -> thread::JoinHandle<Result<Vec<u8>, std::io::Error>> {
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

fn build_capture_bundle(
    request: &CaptureRequest,
    final_args: &[OsString],
    plan: &CapturePlan,
    exit_status: &ExitStatusInfo,
    artifacts: &[CaptureArtifact],
) -> CaptureBundle {
    CaptureBundle {
        plan: *plan,
        invocation: build_capture_invocation(request, final_args, plan),
        raw_text_artifacts: artifacts
            .iter()
            .filter(|artifact| {
                matches!(
                    artifact.kind,
                    ArtifactKind::CompilerStderrText
                        | ArtifactKind::LinkerStderrText
                        | ArtifactKind::CompilerStdoutText
                )
            })
            .cloned()
            .collect(),
        structured_artifacts: artifacts
            .iter()
            .filter(|artifact| {
                matches!(
                    artifact.kind,
                    ArtifactKind::GccSarif | ArtifactKind::GccJson
                )
            })
            .cloned()
            .collect(),
        exit_status: exit_status.clone(),
        integrity_issues: Vec::new(),
    }
}

fn build_capture_invocation(
    request: &CaptureRequest,
    final_args: &[OsString],
    plan: &CapturePlan,
) -> CaptureInvocation {
    let argv = final_args
        .iter()
        .map(|arg| arg.to_string_lossy().into_owned())
        .collect::<Vec<_>>();
    CaptureInvocation {
        backend_path: request.backend.resolved_path.display().to_string(),
        argv_hash: fingerprint_for(&argv),
        argv,
        cwd: request.cwd.display().to_string(),
        selected_mode: plan.execution_mode,
        processing_path: plan.processing_path,
    }
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
    secure_private_dir(&path)?;
    Ok(path)
}

fn build_invocation_record(
    request: &CaptureRequest,
    final_args: &[OsString],
    sarif_path: Option<&Path>,
    child_env_policy: ChildEnvPolicy,
) -> InvocationRecord {
    let argv = final_args
        .iter()
        .map(|arg| arg.to_string_lossy().into_owned())
        .collect::<Vec<_>>();
    InvocationRecord {
        backend_path: request.backend.resolved_path.display().to_string(),
        argv_hash: fingerprint_for(&argv),
        normalized_invocation: normalize_invocation(&argv),
        redaction_class: "restricted".to_string(),
        argv,
        selected_mode: request.mode,
        cwd: request.cwd.display().to_string(),
        sarif_path: sarif_path.map(|path| path.display().to_string()),
        child_env_policy,
        wrapper_env: collect_wrapper_env(),
    }
}

fn normalize_invocation(argv: &[String]) -> NormalizedInvocation {
    let mut input_count = 0;
    let mut compile_only = false;
    let mut preprocess_only = false;
    let mut assemble_only = false;
    let mut output_requested = false;
    let mut language_override = None;
    let mut include_path_count = 0;
    let mut define_count = 0;
    let mut diagnostics_flag_count = 0;
    let mut injected_flag_count = 0;
    let mut expect_output_path = false;
    let mut expect_language = false;

    for arg in argv {
        if expect_output_path {
            expect_output_path = false;
            continue;
        }
        if expect_language {
            language_override = Some(arg.clone());
            expect_language = false;
            continue;
        }

        match arg.as_str() {
            "-c" => compile_only = true,
            "-E" => preprocess_only = true,
            "-S" => assemble_only = true,
            "-o" => {
                output_requested = true;
                expect_output_path = true;
            }
            "-x" => expect_language = true,
            _ if arg.starts_with("-I") => include_path_count += 1,
            _ if arg.starts_with("-D") => define_count += 1,
            _ if arg.starts_with("-fdiagnostics-") => {
                diagnostics_flag_count += 1;
                if arg.starts_with("-fdiagnostics-add-output=sarif:version=2.1,file=") {
                    injected_flag_count += 1;
                }
            }
            _ if arg.starts_with('-') => {}
            _ => input_count += 1,
        }
    }

    NormalizedInvocation {
        arg_count: argv.len(),
        input_count,
        compile_only,
        preprocess_only,
        assemble_only,
        output_requested,
        language_override,
        include_path_count,
        define_count,
        diagnostics_flag_count,
        injected_flag_count,
    }
}

fn child_env_policy(plan: &CapturePlan) -> ChildEnvPolicy {
    let mut policy = ChildEnvPolicy::default();
    if matches!(plan.locale_handling, LocaleHandling::ForceMessagesC) {
        policy
            .set
            .insert("LC_MESSAGES".to_string(), "C".to_string());
    }
    if matches!(
        plan.execution_mode,
        ExecutionMode::Render | ExecutionMode::Shadow
    ) {
        policy.unset = vec![
            "EXPERIMENTAL_SARIF_SOCKET".to_string(),
            "GCC_DIAGNOSTICS_LOG".to_string(),
            "GCC_EXTRA_DIAGNOSTIC_OUTPUT".to_string(),
        ];
    }
    policy
}

fn child_env_policy_for_mode(mode: ExecutionMode) -> ChildEnvPolicy {
    child_env_policy(&CapturePlan {
        execution_mode: mode,
        processing_path: match mode {
            ExecutionMode::Passthrough => ProcessingPath::Passthrough,
            ExecutionMode::Render => ProcessingPath::DualSinkStructured,
            ExecutionMode::Shadow => ProcessingPath::NativeTextCapture,
        },
        structured_capture: if matches!(mode, ExecutionMode::Passthrough) {
            StructuredCapturePolicy::Disabled
        } else {
            StructuredCapturePolicy::SarifFile
        },
        native_text_capture: match mode {
            ExecutionMode::Passthrough => NativeTextCapturePolicy::Passthrough,
            ExecutionMode::Render => NativeTextCapturePolicy::CaptureOnly,
            ExecutionMode::Shadow => NativeTextCapturePolicy::TeeToParent,
        },
        preserve_native_color: false,
        locale_handling: if matches!(mode, ExecutionMode::Render) {
            LocaleHandling::ForceMessagesC
        } else {
            LocaleHandling::Preserve
        },
        retention_policy: RetentionPolicy::Never,
    })
}

pub fn trace_sanitized_env_keys(mode: ExecutionMode) -> Vec<String> {
    let policy = child_env_policy_for_mode(mode);
    let mut keys = policy.set.into_keys().collect::<Vec<_>>();
    keys.extend(policy.unset);
    keys
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
    fs::write(path, json)?;
    secure_private_file(path)
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

    fn fake_paths() -> WrapperPaths {
        WrapperPaths {
            config_path: PathBuf::from("/tmp/config.toml"),
            cache_root: PathBuf::from("/tmp/cache"),
            state_root: PathBuf::from("/tmp/state"),
            runtime_root: PathBuf::from("/tmp/runtime"),
            trace_root: PathBuf::from("/tmp/traces"),
            install_root: PathBuf::from("/tmp/install"),
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
            capture_passthrough_stderr: false,
            retention: RetentionPolicy::Always,
            paths: fake_paths(),
            inject_sarif: true,
            preserve_native_color: true,
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
            child_env_policy(&request.capture_plan()),
        );

        assert!(request.capture_plan().preserve_native_color);
        assert_eq!(record.selected_mode, ExecutionMode::Render);
        assert_eq!(record.backend_path, "/usr/bin/gcc");
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
            retention: RetentionPolicy::OnWrapperFailure,
            paths: fake_paths(),
            inject_sarif: true,
            preserve_native_color: false,
        };
        let render_plan = render_request.capture_plan();
        assert_eq!(render_plan.execution_mode, ExecutionMode::Render);
        assert_eq!(
            render_plan.processing_path,
            ProcessingPath::DualSinkStructured
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
            RetentionPolicy::OnWrapperFailure
        );

        let passthrough_request = CaptureRequest {
            backend: fake_probe(),
            args: Vec::new(),
            cwd: PathBuf::from("/tmp/project"),
            mode: ExecutionMode::Passthrough,
            capture_passthrough_stderr: true,
            retention: RetentionPolicy::Always,
            paths: fake_paths(),
            inject_sarif: false,
            preserve_native_color: false,
        };
        let passthrough_plan = passthrough_request.capture_plan();
        assert_eq!(
            passthrough_plan.processing_path,
            ProcessingPath::Passthrough
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
        assert_eq!(passthrough_plan.retention_policy, RetentionPolicy::Always);
    }

    #[test]
    fn injected_flags_preserve_native_color_when_requested() {
        let bundle = CaptureBundle {
            plan: CapturePlan {
                execution_mode: ExecutionMode::Render,
                processing_path: ProcessingPath::NativeTextCapture,
                structured_capture: StructuredCapturePolicy::Disabled,
                native_text_capture: NativeTextCapturePolicy::CaptureOnly,
                preserve_native_color: true,
                locale_handling: LocaleHandling::ForceMessagesC,
                retention_policy: RetentionPolicy::Always,
            },
            invocation: CaptureInvocation {
                backend_path: "/usr/bin/gcc".to_string(),
                argv: Vec::new(),
                argv_hash: "hash".to_string(),
                cwd: "/tmp/project".to_string(),
                selected_mode: ExecutionMode::Render,
                processing_path: ProcessingPath::NativeTextCapture,
            },
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
            bundle.injected_flags(Path::new("/tmp/runtime")),
            vec!["-fdiagnostics-color=always".to_string()]
        );
    }

    #[test]
    fn capture_bundle_groups_raw_text_and_structured_artifacts() {
        let request = CaptureRequest {
            backend: fake_probe(),
            args: vec![OsString::from("-c"), OsString::from("main.c")],
            cwd: PathBuf::from("/tmp/project"),
            mode: ExecutionMode::Render,
            capture_passthrough_stderr: false,
            retention: RetentionPolicy::Always,
            paths: fake_paths(),
            inject_sarif: true,
            preserve_native_color: false,
        };
        let plan = request.capture_plan();
        let sarif_path = PathBuf::from("/tmp/runtime/diagnostics.sarif");
        let artifacts = build_artifacts(b"stderr", Some(&sarif_path), &request.backend);
        let exit_status = ExitStatusInfo {
            code: Some(1),
            signal: None,
            success: false,
        };

        let bundle = build_capture_bundle(
            &request,
            &[
                OsString::from("-c"),
                OsString::from("main.c"),
                OsString::from(
                    "-fdiagnostics-add-output=sarif:version=2.1,file=/tmp/runtime/diagnostics.sarif",
                ),
            ],
            &plan,
            &exit_status,
            &artifacts,
        );

        assert_eq!(bundle.invocation.backend_path, "/usr/bin/gcc");
        assert_eq!(bundle.invocation.selected_mode, ExecutionMode::Render);
        assert_eq!(
            bundle.invocation.processing_path,
            ProcessingPath::DualSinkStructured
        );
        assert_eq!(bundle.raw_text_artifacts.len(), 1);
        assert_eq!(bundle.raw_text_artifacts[0].id, "stderr.raw");
        assert_eq!(bundle.structured_artifacts.len(), 1);
        assert_eq!(bundle.structured_artifacts[0].id, "diagnostics.sarif");
        assert_eq!(bundle.exit_status, exit_status);
        assert!(bundle.integrity_issues.is_empty());
    }

    #[test]
    fn bundle_helpers_preserve_injected_flag_and_temp_paths_when_sarif_is_missing() {
        let bundle = CaptureBundle {
            plan: CapturePlan {
                execution_mode: ExecutionMode::Render,
                processing_path: ProcessingPath::DualSinkStructured,
                structured_capture: StructuredCapturePolicy::SarifFile,
                native_text_capture: NativeTextCapturePolicy::CaptureOnly,
                preserve_native_color: true,
                locale_handling: LocaleHandling::ForceMessagesC,
                retention_policy: RetentionPolicy::Always,
            },
            invocation: CaptureInvocation {
                backend_path: "/usr/bin/gcc".to_string(),
                argv: vec!["-c".to_string(), "main.c".to_string()],
                argv_hash: "hash".to_string(),
                cwd: "/tmp/project".to_string(),
                selected_mode: ExecutionMode::Render,
                processing_path: ProcessingPath::DualSinkStructured,
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
        let normalized = normalize_invocation(&[
            "-c".to_string(),
            "-Iinclude".to_string(),
            "-DDEBUG=1".to_string(),
            "-o".to_string(),
            "main.o".to_string(),
            "-x".to_string(),
            "c++".to_string(),
            "main.cc".to_string(),
        ]);

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
}
