//! Captures diagnostic artifacts from compiler invocations, manages temporary files and cleanup.

use diag_backend_probe::{ProbeResult, ProcessingPath};
use diag_core::{
    ArtifactKind, ArtifactStorage, CaptureArtifact, IntegrityIssue, IssueSeverity, IssueStage,
    Provenance, ProvenanceSource, ToolInfo, fingerprint_for,
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

const STDERR_CAPTURE_BUFFER_BYTES: usize = 4096;
const STDERR_CAPTURE_PREVIEW_LIMIT_BYTES: usize = 1024 * 1024;
const STDERR_CAPTURE_ID: &str = "stderr.raw";

/// How the wrapper executes the backend compiler.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionMode {
    /// Capture and re-render diagnostics through the wrapper pipeline.
    Render,
    /// Run the backend and tee stderr while capturing artifacts.
    Shadow,
    /// Pass execution directly to the backend without modification.
    Passthrough,
}

/// Policy for capturing structured diagnostic output.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StructuredCapturePolicy {
    /// No structured capture.
    Disabled,
    /// Dual-sink SARIF via `-fdiagnostics-add-output`.
    SarifFile,
    /// Single-sink SARIF via `-fdiagnostics-format=sarif-file`.
    SingleSinkSarifFile,
    /// Single-sink JSON via `-fdiagnostics-format=json-file`.
    SingleSinkJsonFile,
}

/// Policy for handling native stderr text from the backend.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NativeTextCapturePolicy {
    /// Forward stderr to the parent process without capture.
    Passthrough,
    /// Capture stderr silently without forwarding.
    CaptureOnly,
    /// Capture stderr and simultaneously forward to the parent.
    TeeToParent,
}

/// How the child process locale environment is managed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LocaleHandling {
    /// Keep the inherited locale unchanged.
    Preserve,
    /// Set `LC_MESSAGES=C` for stable English diagnostics.
    ForceMessagesC,
}

/// Fully resolved plan describing how a capture invocation will proceed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct CapturePlan {
    /// Execution mode for the backend invocation.
    pub execution_mode: ExecutionMode,
    /// Diagnostic processing strategy in effect.
    pub processing_path: ProcessingPath,
    /// Structured diagnostic capture policy.
    pub structured_capture: StructuredCapturePolicy,
    /// Native stderr text handling policy.
    pub native_text_capture: NativeTextCapturePolicy,
    /// Whether to inject color-always flags for native output.
    pub preserve_native_color: bool,
    /// Locale management for the child process.
    pub locale_handling: LocaleHandling,
    /// Trace retention policy after capture completes.
    pub retention_policy: RetentionPolicy,
}

/// Input parameters for a single diagnostic capture invocation.
#[derive(Debug, Clone)]
pub struct CaptureRequest {
    /// Probed backend to invoke.
    pub backend: ProbeResult,
    /// Arguments to pass to the backend.
    pub args: Vec<OsString>,
    /// Working directory for the backend process.
    pub cwd: PathBuf,
    /// Requested execution mode.
    pub mode: ExecutionMode,
    /// Whether to tee stderr in passthrough mode.
    pub capture_passthrough_stderr: bool,
    /// Trace retention policy.
    pub retention: RetentionPolicy,
    /// Filesystem paths used by the wrapper runtime.
    pub paths: WrapperPaths,
    /// Structured capture policy to apply.
    pub structured_capture: StructuredCapturePolicy,
    /// Whether to inject color-always flags.
    pub preserve_native_color: bool,
}

impl CaptureRequest {
    /// Derives the effective capture plan from this request.
    pub fn capture_plan(&self) -> CapturePlan {
        effective_capture_plan(
            self,
            CapturePlan {
                execution_mode: self.mode,
                processing_path: match self.structured_capture {
                    StructuredCapturePolicy::SarifFile => ProcessingPath::DualSinkStructured,
                    StructuredCapturePolicy::SingleSinkSarifFile
                    | StructuredCapturePolicy::SingleSinkJsonFile => {
                        ProcessingPath::SingleSinkStructured
                    }
                    StructuredCapturePolicy::Disabled => match self.mode {
                        ExecutionMode::Passthrough => ProcessingPath::Passthrough,
                        _ => ProcessingPath::NativeTextCapture,
                    },
                },
                structured_capture: self.structured_capture,
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
            },
        )
    }

    /// Constructs a request from an already-resolved capture plan.
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
            structured_capture: plan.structured_capture,
            preserve_native_color: plan.preserve_native_color,
        }
    }
}

/// Portable representation of a child process exit status.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ExitStatusInfo {
    /// Exit code, if the process exited normally.
    pub code: Option<i32>,
    /// Signal number, if the process was terminated by a signal.
    pub signal: Option<i32>,
    /// Whether the process exited successfully.
    pub success: bool,
}

/// Metadata describing the backend invocation that was executed.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CaptureInvocation {
    /// Filesystem path to the backend binary.
    pub backend_path: String,
    /// Full argument vector passed to the backend.
    pub argv: Vec<String>,
    /// Fingerprint hash of the argument vector.
    pub argv_hash: String,
    /// Working directory used for the invocation.
    pub cwd: String,
    /// Execution mode that was in effect.
    pub selected_mode: ExecutionMode,
    /// Processing path that was in effect.
    pub processing_path: ProcessingPath,
}

/// Serializable bundle of all capture results for a single invocation.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CaptureBundle {
    /// The capture plan that governed this invocation.
    pub plan: CapturePlan,
    /// Metadata about the backend invocation.
    pub invocation: CaptureInvocation,
    /// Raw stderr text artifacts captured from the backend.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub raw_text_artifacts: Vec<CaptureArtifact>,
    /// Structured diagnostic artifacts (SARIF/JSON) captured from the backend.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub structured_artifacts: Vec<CaptureArtifact>,
    /// Exit status of the backend process.
    pub exit_status: ExitStatusInfo,
    /// Integrity issues detected during capture.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub integrity_issues: Vec<IntegrityIssue>,
}

impl CaptureBundle {
    /// Returns all artifacts (raw text and structured) combined.
    pub fn capture_artifacts(&self) -> Vec<CaptureArtifact> {
        let mut artifacts =
            Vec::with_capacity(self.raw_text_artifacts.len() + self.structured_artifacts.len());
        artifacts.extend(self.raw_text_artifacts.clone());
        artifacts.extend(self.structured_artifacts.clone());
        artifacts
    }

    /// Returns the inline stderr text from raw text artifacts, if present.
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

    /// Returns the expected SARIF output path within the temp directory.
    pub fn authoritative_sarif_path(&self, temp_dir: &Path) -> Option<PathBuf> {
        matches!(
            self.plan.structured_capture,
            StructuredCapturePolicy::SarifFile | StructuredCapturePolicy::SingleSinkSarifFile
        )
        .then(|| temp_dir.join("diagnostics.sarif"))
    }

    /// Returns the diagnostic flags that were injected into the backend invocation.
    pub fn injected_flags(&self, temp_dir: &Path) -> Vec<String> {
        let mut flags = Vec::new();
        match self.plan.structured_capture {
            StructuredCapturePolicy::Disabled => {}
            StructuredCapturePolicy::SarifFile => {
                if let Some(path) = self.authoritative_sarif_path(temp_dir) {
                    flags.push(format!(
                        "-fdiagnostics-add-output=sarif:version=2.1,file={}",
                        path.display()
                    ));
                }
            }
            StructuredCapturePolicy::SingleSinkSarifFile => {
                flags.push("-fdiagnostics-format=sarif-file".to_string());
            }
            StructuredCapturePolicy::SingleSinkJsonFile => {
                flags.push("-fdiagnostics-format=json-file".to_string());
            }
        }
        if self.plan.preserve_native_color {
            flags.push("-fdiagnostics-color=always".to_string());
        }
        flags
    }

    /// Returns the list of temporary artifact paths created during capture.
    pub fn temp_artifact_paths(&self, temp_dir: &Path) -> Vec<PathBuf> {
        let mut paths = vec![temp_dir.to_path_buf(), temp_dir.join("invocation.json")];
        if let Some(path) = authoritative_structured_path(self.plan.structured_capture, temp_dir) {
            paths.push(path);
        }
        paths
    }
}

/// Complete outcome of a capture invocation, including artifacts and metadata.
#[derive(Debug)]
pub struct CaptureOutcome {
    /// Exit status of the backend process.
    pub exit_status: ExitStatusInfo,
    /// Raw stderr bytes captured from the backend.
    pub stderr_bytes: Vec<u8>,
    /// Path to the SARIF file, if one was produced.
    pub sarif_path: Option<PathBuf>,
    /// Temporary directory used for this capture session.
    pub temp_dir: PathBuf,
    /// Wall-clock capture duration in milliseconds.
    pub capture_duration_ms: u64,
    /// Whether trace artifacts were retained on disk.
    pub retained: bool,
    /// Directory where retained traces were stored, if any.
    pub retained_trace_dir: Option<PathBuf>,
    /// All artifacts produced during capture.
    pub artifacts: Vec<CaptureArtifact>,
    /// Serializable bundle summarizing the capture.
    pub bundle: CaptureBundle,
}

impl CaptureOutcome {
    /// Returns all capture artifacts from the bundle.
    pub fn capture_artifacts(&self) -> Vec<CaptureArtifact> {
        self.bundle.capture_artifacts()
    }

    /// Returns the captured stderr as a string, lossy-decoding if needed.
    pub fn stderr_text(&self) -> Cow<'_, str> {
        self.bundle
            .stderr_text()
            .map(Cow::Borrowed)
            .unwrap_or_else(|| String::from_utf8_lossy(&self.stderr_bytes))
    }

    /// Returns the expected SARIF output path for this outcome.
    pub fn authoritative_sarif_path(&self) -> Option<PathBuf> {
        self.bundle.authoritative_sarif_path(&self.temp_dir)
    }

    /// Returns the processing path that was used.
    pub fn processing_path(&self) -> ProcessingPath {
        self.bundle.plan.processing_path
    }

    /// Returns environment variable keys that were set or unset for the child.
    pub fn sanitized_env_keys(&self) -> Vec<String> {
        trace_sanitized_env_keys(self.bundle.plan.execution_mode)
    }

    /// Returns the diagnostic flags injected into the backend invocation.
    pub fn injected_flags(&self) -> Vec<String> {
        self.bundle.injected_flags(&self.temp_dir)
    }

    /// Returns temporary artifact paths created during capture.
    pub fn temp_artifact_paths(&self) -> Vec<PathBuf> {
        self.bundle.temp_artifact_paths(&self.temp_dir)
    }
}

/// Errors that can occur during a capture invocation.
#[derive(Debug, thiserror::Error)]
pub enum CaptureError {
    /// An I/O error occurred during capture.
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    /// The backend process could not be spawned.
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

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct ArtifactFingerprint {
    modified_ns: u128,
    size_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct DiscoveredStructuredArtifact {
    path: PathBuf,
    existed_before: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct CapturedStderr {
    preview_bytes: Vec<u8>,
    total_bytes: u64,
    truncated_bytes: u64,
    spool_path: PathBuf,
}

impl CapturedStderr {
    fn empty(spool_path: PathBuf) -> Self {
        Self {
            preview_bytes: Vec::new(),
            total_bytes: 0,
            truncated_bytes: 0,
            spool_path,
        }
    }

    fn truncated(&self) -> bool {
        self.truncated_bytes > 0
    }

    fn integrity_issues(&self) -> Vec<IntegrityIssue> {
        if !self.truncated() {
            return Vec::new();
        }

        vec![IntegrityIssue {
            severity: IssueSeverity::Warning,
            stage: IssueStage::Capture,
            message: format!(
                "stderr capture exceeded the in-memory cap of {} bytes; preserved {} bytes in spool storage and truncated {} bytes from inline processing",
                STDERR_CAPTURE_PREVIEW_LIMIT_BYTES, self.total_bytes, self.truncated_bytes
            ),
            provenance: Some(Provenance {
                source: ProvenanceSource::Policy,
                capture_refs: vec![STDERR_CAPTURE_ID.to_string()],
            }),
        }]
    }
}

fn authoritative_structured_path(
    policy: StructuredCapturePolicy,
    temp_dir: &Path,
) -> Option<PathBuf> {
    structured_artifact_file_name(policy).map(|file_name| temp_dir.join(file_name))
}

fn structured_artifact_file_name(policy: StructuredCapturePolicy) -> Option<&'static str> {
    match policy {
        StructuredCapturePolicy::Disabled => None,
        StructuredCapturePolicy::SarifFile | StructuredCapturePolicy::SingleSinkSarifFile => {
            Some("diagnostics.sarif")
        }
        StructuredCapturePolicy::SingleSinkJsonFile => Some("diagnostics.json"),
    }
}

fn structured_artifact_extension(policy: StructuredCapturePolicy) -> Option<&'static str> {
    match policy {
        StructuredCapturePolicy::Disabled => None,
        StructuredCapturePolicy::SarifFile | StructuredCapturePolicy::SingleSinkSarifFile => {
            Some("sarif")
        }
        StructuredCapturePolicy::SingleSinkJsonFile => Some("json"),
    }
}

fn single_sink_structured_capture(policy: StructuredCapturePolicy) -> bool {
    matches!(
        policy,
        StructuredCapturePolicy::SingleSinkSarifFile | StructuredCapturePolicy::SingleSinkJsonFile
    )
}

fn effective_capture_plan(request: &CaptureRequest, mut plan: CapturePlan) -> CapturePlan {
    if has_hard_diagnostics_conflict(&request.args) {
        plan.execution_mode = ExecutionMode::Passthrough;
        plan.processing_path = ProcessingPath::Passthrough;
        plan.structured_capture = StructuredCapturePolicy::Disabled;
        plan.native_text_capture = runtime_passthrough_capture_policy(request);
        plan.preserve_native_color = false;
        plan.locale_handling = LocaleHandling::Preserve;
        return plan;
    }
    if has_color_control_override(&request.args) {
        plan.preserve_native_color = false;
    }
    plan
}

fn runtime_passthrough_capture_policy(request: &CaptureRequest) -> NativeTextCapturePolicy {
    if request.capture_passthrough_stderr
        || matches!(
            request.retention,
            RetentionPolicy::OnChildError | RetentionPolicy::Always
        )
    {
        NativeTextCapturePolicy::TeeToParent
    } else {
        NativeTextCapturePolicy::Passthrough
    }
}

fn has_hard_diagnostics_conflict(args: &[OsString]) -> bool {
    args.iter().any(|arg| {
        let value = arg.to_string_lossy();
        value.starts_with("-fdiagnostics-format=")
            || value.starts_with("-fdiagnostics-add-output=")
            || value.starts_with("-fdiagnostics-set-output=")
            || value == "-fdiagnostics-parseable-fixits"
            || value == "-fdiagnostics-generate-patch"
    })
}

fn has_color_control_override(args: &[OsString]) -> bool {
    args.iter().any(|arg| {
        let value = arg.to_string_lossy();
        value == "-fno-diagnostics-color"
            || value == "-fdiagnostics-color"
            || value.starts_with("-fdiagnostics-color=")
    })
}

fn capture_stderr_stream(
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

fn spawn_capture_reader(
    stderr: std::process::ChildStderr,
    spool_path: PathBuf,
) -> thread::JoinHandle<Result<CapturedStderr, std::io::Error>> {
    thread::spawn(move || -> Result<CapturedStderr, std::io::Error> {
        let mut reader = stderr;
        capture_stderr_stream(&mut reader, &spool_path, None)
    })
}

fn structured_artifact_metadata(path: &Path) -> Option<(&'static str, ArtifactKind, &'static str)> {
    match path.file_name().and_then(|name| name.to_str()) {
        Some("diagnostics.sarif") => Some((
            "diagnostics.sarif",
            ArtifactKind::GccSarif,
            "application/sarif+json",
        )),
        Some("diagnostics.json") => Some((
            "diagnostics.json",
            ArtifactKind::GccJson,
            "application/json",
        )),
        _ => None,
    }
}

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

    let mut command = Command::new(&request.backend.resolved_path);
    command.current_dir(&request.cwd);
    command.stdin(Stdio::inherit());
    command.stdout(Stdio::inherit());
    let child_env_policy = child_env_policy(&plan);
    apply_child_env_policy(&mut command, &child_env_policy);

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
    write_invocation_record(
        &invocation_path,
        &build_invocation_record(
            request,
            &plan,
            &final_args,
            injected_sarif_path.as_deref(),
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
    let stderr_capture = match stderr_handle {
        Some(handle) => handle
            .join()
            .unwrap_or_else(|_| Ok(CapturedStderr::empty(stderr_spool_path.clone())))
            .unwrap_or_else(|_| CapturedStderr::empty(stderr_spool_path.clone())),
        None => CapturedStderr::empty(stderr_spool_path.clone()),
    };
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

fn spawn_tee_reader(
    stderr: std::process::ChildStderr,
    spool_path: PathBuf,
) -> thread::JoinHandle<Result<CapturedStderr, std::io::Error>> {
    thread::spawn(move || -> Result<CapturedStderr, std::io::Error> {
        let mut reader = stderr;
        let mut tee = std::io::stderr().lock();
        capture_stderr_stream(&mut reader, &spool_path, Some(&mut tee))
    })
}

fn build_artifacts(
    stderr_capture: &CapturedStderr,
    structured_path: Option<&PathBuf>,
    backend: &ProbeResult,
) -> Vec<CaptureArtifact> {
    let mut artifacts = Vec::new();
    if stderr_capture.total_bytes > 0 {
        artifacts.push(CaptureArtifact {
            id: STDERR_CAPTURE_ID.to_string(),
            kind: ArtifactKind::CompilerStderrText,
            media_type: "text/plain".to_string(),
            encoding: Some("utf-8".to_string()),
            digest_sha256: None,
            size_bytes: Some(stderr_capture.total_bytes),
            storage: ArtifactStorage::Inline,
            inline_text: Some(String::from_utf8_lossy(&stderr_capture.preview_bytes).to_string()),
            external_ref: None,
            produced_by: Some(tool_info(backend)),
        });
    }
    if let Some(path) = structured_path {
        let Some((id, kind, media_type)) = structured_artifact_metadata(path) else {
            return artifacts;
        };
        artifacts.push(CaptureArtifact {
            id: id.to_string(),
            kind,
            media_type: media_type.to_string(),
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

fn snapshot_structured_artifacts(
    dir: &Path,
    policy: StructuredCapturePolicy,
) -> Result<BTreeMap<PathBuf, ArtifactFingerprint>, std::io::Error> {
    let Some(extension) = structured_artifact_extension(policy) else {
        return Ok(BTreeMap::new());
    };
    let mut entries = BTreeMap::new();
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) != Some(extension) {
            continue;
        }
        let metadata = entry.metadata()?;
        if !metadata.is_file() {
            continue;
        }
        entries.insert(path, artifact_fingerprint(&metadata));
    }
    Ok(entries)
}

fn artifact_fingerprint(metadata: &fs::Metadata) -> ArtifactFingerprint {
    let modified_ns = metadata
        .modified()
        .ok()
        .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|duration| duration.as_nanos())
        .unwrap_or_default();
    ArtifactFingerprint {
        modified_ns,
        size_bytes: metadata.len(),
    }
}

fn discover_structured_artifact(
    dir: &Path,
    before: &BTreeMap<PathBuf, ArtifactFingerprint>,
    policy: StructuredCapturePolicy,
) -> Result<Option<DiscoveredStructuredArtifact>, std::io::Error> {
    let after = snapshot_structured_artifacts(dir, policy)?;
    let mut changed = after
        .into_iter()
        .filter_map(|(path, fingerprint)| {
            let existed_before = before.contains_key(&path);
            (before.get(&path) != Some(&fingerprint)).then_some((
                fingerprint.modified_ns,
                path.clone(),
                DiscoveredStructuredArtifact {
                    path,
                    existed_before,
                },
            ))
        })
        .collect::<Vec<_>>();
    changed.sort_by(|left, right| right.0.cmp(&left.0).then_with(|| right.1.cmp(&left.1)));
    Ok(changed.into_iter().map(|(_, _, artifact)| artifact).next())
}

fn preserve_discovered_artifact(
    artifact: &DiscoveredStructuredArtifact,
    temp_dir: &Path,
    policy: StructuredCapturePolicy,
) -> Result<PathBuf, std::io::Error> {
    let preserved_path = authoritative_structured_path(policy, temp_dir)
        .unwrap_or_else(|| temp_dir.join("diagnostics.bin"));
    fs::copy(&artifact.path, &preserved_path)?;
    secure_private_file(&preserved_path)?;
    if !artifact.existed_before {
        let _ = fs::remove_file(&artifact.path);
    }
    Ok(preserved_path)
}

fn build_capture_bundle(
    request: &CaptureRequest,
    final_args: &[OsString],
    plan: &CapturePlan,
    exit_status: &ExitStatusInfo,
    artifacts: &[CaptureArtifact],
    integrity_issues: &[IntegrityIssue],
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
        integrity_issues: integrity_issues.to_vec(),
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

fn path_is_safe_for_gcc_output(path: &Path) -> bool {
    path.to_string_lossy()
        .chars()
        .all(|ch| !matches!(ch, ',' | '=' | ' ') && !ch.is_control())
}

fn safe_runtime_fallback_bases() -> Vec<PathBuf> {
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

fn safe_runtime_root(root: &Path) -> Result<PathBuf, std::io::Error> {
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

fn temp_dir_name(path: &Path) -> String {
    path.file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("trace")
        .to_string()
}

fn unique_temp_dir(root: &Path) -> Result<PathBuf, std::io::Error> {
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

fn build_invocation_record(
    request: &CaptureRequest,
    plan: &CapturePlan,
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
        normalized_invocation: normalize_invocation(&argv, request.args.len()),
        redaction_class: "restricted".to_string(),
        argv,
        selected_mode: plan.execution_mode,
        cwd: request.cwd.display().to_string(),
        sarif_path: sarif_path.map(|path| path.display().to_string()),
        child_env_policy,
        wrapper_env: collect_wrapper_env(),
    }
}

fn normalize_invocation(argv: &[String], user_arg_count: usize) -> NormalizedInvocation {
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

    for (index, arg) in argv.iter().enumerate() {
        let wrapper_owned = index >= user_arg_count;
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
                if wrapper_owned
                    && (arg.starts_with("-fdiagnostics-add-output=sarif:version=2.1,file=")
                        || arg == "-fdiagnostics-format=sarif-file"
                        || arg == "-fdiagnostics-format=json-file")
                {
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

/// Returns the environment variable keys modified by the child env policy for the given mode.
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
    use std::io::Cursor;
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
        assert!(path_is_safe_for_gcc_output(Path::new(
            "/tmp/cc-formed-runtime/formed-123/diagnostics.sarif"
        )));
        assert!(!path_is_safe_for_gcc_output(Path::new(
            "/tmp/runtime,root=unsafe path/formed-123/diagnostics.sarif"
        )));
        assert!(!path_is_safe_for_gcc_output(Path::new(
            "/tmp/runtime=root/formed-123/diagnostics.sarif"
        )));
    }

    #[cfg(unix)]
    fn make_executable(path: &Path) {
        use std::os::unix::fs::PermissionsExt;

        let mut permissions = fs::metadata(path).unwrap().permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(path, permissions).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn run_capture_uses_preselected_safe_sarif_path_for_unsafe_runtime_root() {
        let temp = tempfile::tempdir().unwrap();
        let backend = temp.path().join("fake-gcc");
        let observed_sarif_path = temp.path().join("observed-sarif-path.txt");
        let runtime_root = temp.path().join("runtime,root=unsafe path");
        let cwd = temp.path().join("cwd");
        fs::create_dir_all(&cwd).unwrap();

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
        fs::write(&backend, script).unwrap();
        make_executable(&backend);

        let request = CaptureRequest {
            backend: ProbeResult {
                resolved_path: backend.clone(),
                version_string: "gcc (Fake) 15.2.0".to_string(),
                ..fake_probe()
            },
            args: vec![OsString::from("-c"), OsString::from("main.c")],
            cwd,
            mode: ExecutionMode::Render,
            capture_passthrough_stderr: false,
            retention: RetentionPolicy::Never,
            paths: WrapperPaths {
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
            fs::read_to_string(&observed_sarif_path).unwrap(),
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
            args: vec![OsString::from("-c"), OsString::from("src/main.c")],
            cwd: PathBuf::from("/tmp/project"),
            mode: ExecutionMode::Render,
            capture_passthrough_stderr: false,
            retention: RetentionPolicy::Always,
            paths: fake_paths(),
            structured_capture: StructuredCapturePolicy::SarifFile,
            preserve_native_color: true,
        };

        let record = build_invocation_record(
            &request,
            &request.capture_plan(),
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
            structured_capture: StructuredCapturePolicy::SarifFile,
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
            structured_capture: StructuredCapturePolicy::Disabled,
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
    fn capture_plan_passthroughs_on_user_diagnostics_sink_conflict() {
        let request = CaptureRequest {
            backend: fake_probe(),
            args: vec![
                OsString::from("-c"),
                OsString::from("main.c"),
                OsString::from("-fdiagnostics-format=sarif-file"),
            ],
            cwd: PathBuf::from("/tmp/project"),
            mode: ExecutionMode::Render,
            capture_passthrough_stderr: false,
            retention: RetentionPolicy::OnWrapperFailure,
            paths: fake_paths(),
            structured_capture: StructuredCapturePolicy::SarifFile,
            preserve_native_color: true,
        };

        let plan = request.capture_plan();
        assert_eq!(plan.execution_mode, ExecutionMode::Passthrough);
        assert_eq!(plan.processing_path, ProcessingPath::Passthrough);
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
                OsString::from("-c"),
                OsString::from("main.c"),
                OsString::from("-fdiagnostics-color=never"),
            ],
            cwd: PathBuf::from("/tmp/project"),
            mode: ExecutionMode::Render,
            capture_passthrough_stderr: false,
            retention: RetentionPolicy::Always,
            paths: fake_paths(),
            structured_capture: StructuredCapturePolicy::Disabled,
            preserve_native_color: true,
        };

        let plan = request.capture_plan();
        assert_eq!(plan.execution_mode, ExecutionMode::Render);
        assert_eq!(plan.processing_path, ProcessingPath::NativeTextCapture);
        assert!(!plan.preserve_native_color);
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
    fn single_sink_capture_plan_uses_explicit_structured_path() {
        let request = CaptureRequest {
            backend: fake_probe(),
            args: Vec::new(),
            cwd: PathBuf::from("/tmp/project"),
            mode: ExecutionMode::Render,
            capture_passthrough_stderr: false,
            retention: RetentionPolicy::OnWrapperFailure,
            paths: fake_paths(),
            structured_capture: StructuredCapturePolicy::SingleSinkSarifFile,
            preserve_native_color: false,
        };

        let plan = request.capture_plan();
        assert_eq!(plan.processing_path, ProcessingPath::SingleSinkStructured);
        assert_eq!(
            plan.structured_capture,
            StructuredCapturePolicy::SingleSinkSarifFile
        );
        let bundle = CaptureBundle {
            plan,
            invocation: CaptureInvocation {
                backend_path: "/usr/bin/gcc".to_string(),
                argv: Vec::new(),
                argv_hash: "hash".to_string(),
                cwd: "/tmp/project".to_string(),
                selected_mode: ExecutionMode::Render,
                processing_path: ProcessingPath::SingleSinkStructured,
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
            retention: RetentionPolicy::OnWrapperFailure,
            paths: fake_paths(),
            structured_capture: StructuredCapturePolicy::SingleSinkJsonFile,
            preserve_native_color: false,
        };

        let plan = request.capture_plan();
        assert_eq!(plan.processing_path, ProcessingPath::SingleSinkStructured);
        assert_eq!(
            plan.structured_capture,
            StructuredCapturePolicy::SingleSinkJsonFile
        );
        let bundle = CaptureBundle {
            plan,
            invocation: CaptureInvocation {
                backend_path: "/usr/bin/gcc".to_string(),
                argv: Vec::new(),
                argv_hash: "hash".to_string(),
                cwd: "/tmp/project".to_string(),
                selected_mode: ExecutionMode::Render,
                processing_path: ProcessingPath::SingleSinkStructured,
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
            vec!["-fdiagnostics-format=json-file".to_string()]
        );
        assert_eq!(
            bundle.temp_artifact_paths(Path::new("/tmp/runtime")),
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
            args: vec![OsString::from("-c"), OsString::from("main.c")],
            cwd: PathBuf::from("/tmp/project"),
            mode: ExecutionMode::Render,
            capture_passthrough_stderr: false,
            retention: RetentionPolicy::Always,
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
                OsString::from("-c"),
                OsString::from("main.c"),
                OsString::from(
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
    fn capture_bundle_groups_raw_text_and_gcc_json_artifacts() {
        let request = CaptureRequest {
            backend: fake_probe(),
            args: vec![OsString::from("-c"), OsString::from("main.c")],
            cwd: PathBuf::from("/tmp/project"),
            mode: ExecutionMode::Render,
            capture_passthrough_stderr: false,
            retention: RetentionPolicy::Always,
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
                OsString::from("-c"),
                OsString::from("main.c"),
                OsString::from("-fdiagnostics-format=json-file"),
            ],
            &plan,
            &exit_status,
            &artifacts,
            &[],
        );

        assert_eq!(
            bundle.invocation.processing_path,
            ProcessingPath::SingleSinkStructured
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
            args: vec![OsString::from("-c"), OsString::from("main.c")],
            cwd: PathBuf::from("/tmp/project"),
            mode: ExecutionMode::Render,
            capture_passthrough_stderr: false,
            retention: RetentionPolicy::Always,
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
            &[OsString::from("-c"), OsString::from("main.c")],
            &plan,
            &exit_status,
            &artifacts,
            &captured.integrity_issues(),
        );

        assert_eq!(bundle.integrity_issues.len(), 1);
        assert_eq!(bundle.integrity_issues[0].stage, IssueStage::Capture);
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
            fs::metadata(&spool_path).unwrap().len(),
            payload.len() as u64
        );
        let issues = captured.integrity_issues();
        assert_eq!(issues.len(), 1);
        assert!(
            issues[0]
                .message
                .contains("stderr capture exceeded the in-memory cap")
        );
        assert_eq!(issues[0].stage, IssueStage::Capture);
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
    fn bundle_helpers_preserve_injected_flag_and_temp_paths_when_json_is_missing() {
        let bundle = CaptureBundle {
            plan: CapturePlan {
                execution_mode: ExecutionMode::Render,
                processing_path: ProcessingPath::SingleSinkStructured,
                structured_capture: StructuredCapturePolicy::SingleSinkJsonFile,
                native_text_capture: NativeTextCapturePolicy::CaptureOnly,
                preserve_native_color: false,
                locale_handling: LocaleHandling::ForceMessagesC,
                retention_policy: RetentionPolicy::Always,
            },
            invocation: CaptureInvocation {
                backend_path: "/usr/bin/gcc".to_string(),
                argv: vec!["-c".to_string(), "main.c".to_string()],
                argv_hash: "hash".to_string(),
                cwd: "/tmp/project".to_string(),
                selected_mode: ExecutionMode::Render,
                processing_path: ProcessingPath::SingleSinkStructured,
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
                OsString::from("-c"),
                OsString::from("main.c"),
                OsString::from("-fdiagnostics-format=sarif-file"),
            ],
            cwd: PathBuf::from("/tmp/project"),
            mode: ExecutionMode::Render,
            capture_passthrough_stderr: false,
            retention: RetentionPolicy::OnWrapperFailure,
            paths: fake_paths(),
            structured_capture: StructuredCapturePolicy::SarifFile,
            preserve_native_color: true,
        };
        let plan = request.capture_plan();
        let final_args = request.args.clone();

        let record =
            build_invocation_record(&request, &plan, &final_args, None, child_env_policy(&plan));

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
