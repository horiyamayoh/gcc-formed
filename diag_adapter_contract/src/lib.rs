//! Shared adapter contract for compiler-specific diagnostic ingestion.
//!
//! This crate defines the adapter-facing trait and the shared ingest request
//! and response types used by CLI and adapter implementations.

use diag_capture_runtime::CaptureBundle;
use diag_core::{
    Confidence, DiagnosticDocument, FallbackGrade, FallbackReason, IntegrityIssue, Origin,
    ProducerInfo, RunInfo, SourceAuthority,
};

/// Runtime context passed into a diagnostic adapter ingest call.
#[derive(Debug, Clone)]
pub struct IngestPolicy {
    /// Metadata about the tool that produced the diagnostic document.
    pub producer: ProducerInfo,
    /// Runtime context for the compiler invocation being ingested.
    pub run: RunInfo,
}

/// Full ingest report returned by an adapter implementation.
#[derive(Debug, Clone)]
pub struct IngestReport {
    /// The converted diagnostic document.
    pub document: DiagnosticDocument,
    /// Whether the document was derived from structured or residual input.
    pub source_authority: SourceAuthority,
    /// Upper bound on the confidence of diagnostics in this report.
    pub confidence_ceiling: Confidence,
    /// Degree to which fallback processing was applied.
    pub fallback_grade: FallbackGrade,
    /// Integrity issues encountered during ingestion.
    pub warnings: Vec<IntegrityIssue>,
    /// If the adapter fell back to a non-structured path, the reason why.
    pub fallback_reason: Option<FallbackReason>,
}

/// Common interface for compiler-specific diagnostic adapters.
pub trait DiagnosticAdapter {
    /// Adapter-specific ingest error.
    type Error: std::error::Error;

    /// Convert a capture bundle into an ingest report.
    fn ingest(
        &self,
        bundle: &CaptureBundle,
        policy: IngestPolicy,
    ) -> Result<IngestReport, Self::Error>;

    /// Return the diagnostic origins this adapter can emit.
    fn supported_origins(&self) -> &[Origin];
}

#[cfg(test)]
mod tests {
    use super::*;
    use diag_backend_probe::ProcessingPath;
    use diag_capture_runtime::{
        CaptureBundle, CaptureInvocation, CapturePlan, ExecutionMode, ExitStatusInfo,
        LocaleHandling, NativeTextCapturePolicy, StructuredCapturePolicy,
    };
    use diag_core::{
        DocumentCompleteness, LanguageMode, ProducerInfo, RunInfo, ToolInfo, WrapperSurface,
    };
    use diag_trace::RetentionPolicy;
    use std::error::Error;
    use std::fmt::{Display, Formatter};

    #[derive(Debug)]
    struct DummyError;

    impl Display for DummyError {
        fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
            write!(f, "dummy adapter error")
        }
    }

    impl Error for DummyError {}

    struct DummyAdapter;

    impl DiagnosticAdapter for DummyAdapter {
        type Error = DummyError;

        fn ingest(
            &self,
            _bundle: &CaptureBundle,
            policy: IngestPolicy,
        ) -> Result<IngestReport, Self::Error> {
            Ok(IngestReport {
                document: DiagnosticDocument {
                    document_id: "dummy-doc".to_string(),
                    schema_version: diag_core::IR_SPEC_VERSION.to_string(),
                    document_completeness: DocumentCompleteness::Complete,
                    producer: policy.producer,
                    run: policy.run,
                    captures: Vec::new(),
                    integrity_issues: Vec::new(),
                    diagnostics: Vec::new(),
                    document_analysis: None,
                    fingerprints: None,
                },
                source_authority: SourceAuthority::Structured,
                confidence_ceiling: Confidence::Low,
                fallback_grade: FallbackGrade::None,
                warnings: Vec::new(),
                fallback_reason: None,
            })
        }

        fn supported_origins(&self) -> &[Origin] {
            &[Origin::Clang]
        }
    }

    fn generic_ingest<A: DiagnosticAdapter>(
        adapter: &A,
        bundle: &CaptureBundle,
        policy: IngestPolicy,
    ) -> Result<IngestReport, A::Error> {
        adapter.ingest(bundle, policy)
    }

    fn sample_bundle() -> CaptureBundle {
        CaptureBundle {
            plan: CapturePlan {
                execution_mode: ExecutionMode::Render,
                processing_path: ProcessingPath::NativeTextCapture,
                structured_capture: StructuredCapturePolicy::Disabled,
                native_text_capture: NativeTextCapturePolicy::CaptureOnly,
                preserve_native_color: false,
                locale_handling: LocaleHandling::ForceMessagesC,
                retention_policy: RetentionPolicy::Never,
            },
            invocation: CaptureInvocation {
                backend_path: "/usr/bin/clang".to_string(),
                launcher_path: None,
                spawn_path: "/usr/bin/clang".to_string(),
                argv: vec!["clang".to_string(), "-c".to_string(), "main.c".to_string()],
                spawn_argv: vec!["clang".to_string(), "-c".to_string(), "main.c".to_string()],
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
        }
    }

    fn sample_policy() -> IngestPolicy {
        IngestPolicy {
            producer: ProducerInfo {
                name: "gcc-formed".to_string(),
                version: "0.2.0-beta.1".to_string(),
                git_revision: None,
                build_profile: Some("test".to_string()),
                rulepack_version: Some("phase1".to_string()),
            },
            run: RunInfo {
                invocation_id: "invocation".to_string(),
                invoked_as: Some("cc-formed".to_string()),
                argv_redacted: vec!["clang".to_string(), "-c".to_string(), "main.c".to_string()],
                cwd_display: Some("/tmp/project".to_string()),
                exit_status: 1,
                primary_tool: ToolInfo {
                    name: "clang".to_string(),
                    version: Some("18.1.0".to_string()),
                    component: None,
                    vendor: Some("LLVM".to_string()),
                },
                secondary_tools: Vec::new(),
                language_mode: Some(LanguageMode::C),
                target_triple: None,
                wrapper_mode: Some(WrapperSurface::Terminal),
            },
        }
    }

    #[test]
    fn dummy_adapter_satisfies_contract() {
        let adapter = DummyAdapter;
        let report = generic_ingest(&adapter, &sample_bundle(), sample_policy()).unwrap();

        assert_eq!(adapter.supported_origins(), &[Origin::Clang]);
        assert_eq!(report.source_authority, SourceAuthority::Structured);
        assert_eq!(report.document.document_id, "dummy-doc");
    }
}
