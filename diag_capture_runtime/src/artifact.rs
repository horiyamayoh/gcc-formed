//! Data types and builders for capture artifacts, invocation records, and bundles.

use crate::artifact_builder::authoritative_structured_path;
use crate::policy::{CapturePlan, ExecutionMode, StructuredCapturePolicy};
use diag_backend_probe::ProcessingPath;
use diag_core::{ArtifactKind, CaptureArtifact, IntegrityIssue};
use serde::{Deserialize, Serialize};
use std::borrow::Cow;
use std::path::{Path, PathBuf};

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
        crate::trace_sanitized_env_keys(self.bundle.plan.execution_mode)
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
