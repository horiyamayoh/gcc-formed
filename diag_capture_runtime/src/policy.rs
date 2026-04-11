//! Capture policy types, plan derivation, and child-process environment management.

use crate::{STDERR_CAPTURE_ID, STDERR_CAPTURE_PREVIEW_LIMIT_BYTES};
use diag_backend_probe::ProcessingPath;
use diag_core::{IntegrityIssue, IssueSeverity, IssueStage, Provenance, ProvenanceSource};
use diag_trace::RetentionPolicy;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::env;
use std::ffi::OsString;
use std::process::Command;

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
    pub backend: diag_backend_probe::ProbeResult,
    /// Arguments to pass to the backend.
    pub args: Vec<OsString>,
    /// Working directory for the backend process.
    pub cwd: std::path::PathBuf,
    /// Requested execution mode.
    pub mode: ExecutionMode,
    /// Whether to tee stderr in passthrough mode.
    pub capture_passthrough_stderr: bool,
    /// Trace retention policy.
    pub retention: RetentionPolicy,
    /// Filesystem paths used by the wrapper runtime.
    pub paths: diag_trace::WrapperPaths,
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
        backend: diag_backend_probe::ProbeResult,
        args: Vec<OsString>,
        cwd: std::path::PathBuf,
        paths: diag_trace::WrapperPaths,
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

/// Returns the environment variable keys modified by the child env policy for the given mode.
pub fn trace_sanitized_env_keys(mode: ExecutionMode) -> Vec<String> {
    let policy = child_env_policy_for_mode(mode);
    let mut keys = policy.set.into_keys().collect::<Vec<_>>();
    keys.extend(policy.unset);
    keys
}

// ---------------------------------------------------------------------------
// pub(crate) helpers
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) struct ChildEnvPolicy {
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub(crate) set: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(crate) unset: Vec<String>,
}

pub(crate) fn effective_capture_plan(
    request: &CaptureRequest,
    mut plan: CapturePlan,
) -> CapturePlan {
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

pub(crate) fn runtime_passthrough_capture_policy(
    request: &CaptureRequest,
) -> NativeTextCapturePolicy {
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

pub(crate) fn has_hard_diagnostics_conflict(args: &[OsString]) -> bool {
    args.iter().any(|arg| {
        let value = arg.to_string_lossy();
        value.starts_with("-fdiagnostics-format=")
            || value.starts_with("-fdiagnostics-add-output=")
            || value.starts_with("-fdiagnostics-set-output=")
            || value == "-fdiagnostics-parseable-fixits"
            || value == "-fdiagnostics-generate-patch"
    })
}

pub(crate) fn has_color_control_override(args: &[OsString]) -> bool {
    args.iter().any(|arg| {
        let value = arg.to_string_lossy();
        value == "-fno-diagnostics-color"
            || value == "-fdiagnostics-color"
            || value.starts_with("-fdiagnostics-color=")
    })
}

pub(crate) fn child_env_policy(plan: &CapturePlan) -> ChildEnvPolicy {
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

pub(crate) fn child_env_policy_for_mode(mode: ExecutionMode) -> ChildEnvPolicy {
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

pub(crate) fn apply_child_env_policy(command: &mut Command, policy: &ChildEnvPolicy) {
    for (key, value) in &policy.set {
        command.env(key, value);
    }
    for key in &policy.unset {
        command.env_remove(key);
    }
}

pub(crate) fn child_env_policy_is_empty(policy: &ChildEnvPolicy) -> bool {
    policy.set.is_empty() && policy.unset.is_empty()
}

pub(crate) fn collect_wrapper_env() -> BTreeMap<String, String> {
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

/// Integrity issues arising from stderr capture truncation (used by [`crate::artifact::CapturedStderr`]).
pub(crate) fn stderr_truncation_issues(
    total_bytes: u64,
    truncated_bytes: u64,
) -> Vec<IntegrityIssue> {
    if truncated_bytes == 0 {
        return Vec::new();
    }

    vec![IntegrityIssue {
        severity: IssueSeverity::Warning,
        stage: IssueStage::Capture,
        message: format!(
            "stderr capture exceeded the in-memory cap of {} bytes; preserved {} bytes in spool storage and truncated {} bytes from inline processing",
            STDERR_CAPTURE_PREVIEW_LIMIT_BYTES, total_bytes, truncated_bytes
        ),
        provenance: Some(Provenance {
            source: ProvenanceSource::Policy,
            capture_refs: vec![STDERR_CAPTURE_ID.to_string()],
        }),
    }]
}
