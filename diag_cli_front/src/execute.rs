use crate::args::ParsedArgs;
use crate::backend::build_execution_plan;
use crate::config::ConfigFile;
use crate::error::CliError;
use crate::mode::is_compiler_introspection;
use crate::render::{
    CommonTraceContext, IngestTraceMetadata, PassthroughTraceWriteRequest, TraceWriteRequest,
    argv_for_trace, build_language_mode, build_primary_tool, maybe_write_passthrough_trace,
    maybe_write_trace, wrapper_surface,
};
use crate::self_check::handle_wrapper_introspection;
use diag_adapter_contract::{DiagnosticAdapter, IngestPolicy, IngestReport};
use diag_adapter_gcc::{AdapterError, GccAdapter, producer_for_version};
use diag_backend_probe::ProbeCache;
use diag_capture_runtime::{
    CaptureBundle, ExecutionMode, ExitStatusInfo, cleanup_capture, run_capture,
};
use diag_cascade::{CascadeContext, CascadeReport, DocumentAnalyzer, SafeDocumentAnalyzer};
use diag_core::{CascadePolicySnapshot, DiagnosticDocument, RunInfo};
use diag_enrich::enrich_document;
use diag_render::{
    PathPolicy, RenderRequest, SourceExcerptPolicy, TypeDisplayPolicy, WarningVisibility, render,
};
use diag_trace::{WrapperPaths, trace_id};
use std::env;
use std::ffi::OsString;
use std::io::Write;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::Path;
use std::process::{Command, ExitCode};
use std::time::Instant;

pub(crate) fn entrypoint() -> ExitCode {
    match real_main() {
        Ok(code) => ExitCode::from(code as u8),
        Err(error) => {
            eprintln!("gcc-formed: {error}");
            ExitCode::from(1)
        }
    }
}

fn real_main() -> Result<i32, CliError> {
    let wrapper_started = Instant::now();
    let argv0 = env::args()
        .next()
        .unwrap_or_else(|| "gcc-formed".to_string());
    let parsed = ParsedArgs::parse(env::args_os().collect())?;
    let paths = WrapperPaths::discover();
    let config = ConfigFile::load(&paths)?;

    if let Some(command) = parsed.introspection {
        return handle_wrapper_introspection(command, &paths);
    }

    let cascade_policy = config.resolve_cascade_policy(&parsed);
    let mut cache = ProbeCache::default();
    let plan = build_execution_plan(&argv0, &parsed, &config, &mut cache)?;

    if is_compiler_introspection(&parsed.forwarded_args) {
        return passthrough_inherit(&plan.backend, &parsed.forwarded_args, &env::current_dir()?);
    }

    if let Some(note) = plan.scope_notice {
        eprintln!("{note}");
    }

    let cwd = env::current_dir()?;
    let capture = run_capture(&plan.capture_request(&paths, &parsed, &cwd))?;
    let exit_code = exit_code_from_status(&capture.exit_status);
    let trace_context = |total_duration_ms| CommonTraceContext {
        paths: &paths,
        capture: &capture,
        parsed: &parsed,
        backend: &plan.backend,
        mode_decision: &plan.mode_decision,
        profile: plan.profile,
        cascade_policy: &cascade_policy,
        capabilities: &plan.capabilities,
        total_duration_ms,
    };

    if matches!(plan.mode(), ExecutionMode::Passthrough) {
        maybe_write_passthrough_trace(PassthroughTraceWriteRequest {
            common: trace_context(wrapper_started.elapsed().as_millis() as u64),
        })?;
        cleanup_capture(&capture)?;
        return Ok(exit_code);
    }

    let run_info = RunInfo {
        invocation_id: trace_id(),
        invoked_as: Some(argv0.clone()),
        argv_redacted: argv_for_trace(&parsed),
        cwd_display: Some(cwd.display().to_string()),
        exit_status: exit_code,
        primary_tool: build_primary_tool(&plan.backend),
        secondary_tools: Vec::new(),
        language_mode: Some(build_language_mode(&argv0)),
        target_triple: None,
        wrapper_mode: Some(wrapper_surface()),
    };
    let ingest_report = ingest_bundle(
        &GccAdapter,
        &capture.bundle,
        IngestPolicy {
            producer: producer_for_version(env!("CARGO_PKG_VERSION")),
            run: run_info,
        },
    )?;
    let ingest_trace = IngestTraceMetadata {
        source_authority: ingest_report.source_authority,
        fallback_grade: ingest_report.fallback_grade,
        fallback_reason: ingest_report.fallback_reason,
    };
    let mut document = ingest_report.document;
    document.captures = capture.capture_artifacts();
    enrich_document(&mut document, &cwd);
    let cascade_context = CascadeContext {
        version_band: plan.backend.version_band(),
        processing_path: capture.processing_path(),
        source_authority: ingest_trace.source_authority,
        fallback_grade: ingest_trace.fallback_grade,
        cwd: cwd.clone(),
    };
    let _ = run_cascade_analysis(
        &SafeDocumentAnalyzer,
        &mut document,
        &cascade_context,
        &cascade_policy,
    );

    if matches!(plan.mode(), ExecutionMode::Shadow) {
        maybe_write_trace(TraceWriteRequest {
            common: trace_context(wrapper_started.elapsed().as_millis() as u64),
            document: &document,
            ingest_trace,
            fallback_reason: plan
                .mode_decision
                .fallback_reason
                .or(ingest_trace.fallback_reason),
            render_duration_ms: None,
        })?;
        cleanup_capture(&capture)?;
        return Ok(exit_code);
    }

    let render_started = Instant::now();
    let render_result = render(RenderRequest {
        document: document.clone(),
        cascade_policy: cascade_policy.clone(),
        profile: plan.profile,
        capabilities: plan.capabilities.clone(),
        cwd: Some(cwd),
        path_policy: config
            .render
            .path_policy
            .unwrap_or(PathPolicy::ShortestUnambiguous),
        warning_visibility: WarningVisibility::Auto,
        debug_refs: plan.debug_refs,
        type_display_policy: TypeDisplayPolicy::CompactSafe,
        source_excerpt_policy: SourceExcerptPolicy::Auto,
    })?;
    let effective_fallback_reason = plan
        .mode_decision
        .fallback_reason
        .or(ingest_trace.fallback_reason)
        .or(render_result.fallback_reason);
    let render_duration_ms = render_started.elapsed().as_millis() as u64;
    let mut stderr = std::io::stderr().lock();
    stderr.write_all(render_result.text.as_bytes())?;
    if !render_result.text.ends_with('\n') {
        stderr.write_all(b"\n")?;
    }

    maybe_write_trace(TraceWriteRequest {
        common: trace_context(wrapper_started.elapsed().as_millis() as u64),
        document: &document,
        ingest_trace,
        fallback_reason: effective_fallback_reason,
        render_duration_ms: Some(render_duration_ms),
    })?;
    cleanup_capture(&capture)?;
    Ok(exit_code)
}

fn invoke_adapter<A: DiagnosticAdapter>(
    adapter: &A,
    bundle: &CaptureBundle,
    policy: IngestPolicy,
) -> Result<IngestReport, A::Error> {
    adapter.ingest(bundle, policy)
}

fn ingest_bundle<A: DiagnosticAdapter<Error = AdapterError>>(
    adapter: &A,
    bundle: &CaptureBundle,
    policy: IngestPolicy,
) -> Result<IngestReport, CliError> {
    Ok(invoke_adapter(adapter, bundle, policy)?)
}

fn run_cascade_analysis<A: DocumentAnalyzer>(
    analyzer: &A,
    document: &mut DiagnosticDocument,
    context: &CascadeContext,
    policy: &CascadePolicySnapshot,
) -> Option<CascadeReport> {
    match catch_unwind(AssertUnwindSafe(|| {
        analyzer.analyze_document(document, context, policy)
    })) {
        Ok(Ok(report)) => Some(report),
        Ok(Err(_)) | Err(_) => {
            document.document_analysis = None;
            None
        }
    }
}

fn passthrough_inherit(
    backend: &diag_backend_probe::ProbeResult,
    forwarded_args: &[OsString],
    cwd: &Path,
) -> Result<i32, CliError> {
    let status = Command::new(backend.spawn_path())
        .current_dir(cwd)
        .args(backend.spawn_args(forwarded_args))
        .stdin(std::process::Stdio::inherit())
        .stdout(std::process::Stdio::inherit())
        .stderr(std::process::Stdio::inherit())
        .status()?;
    Ok(exit_code_from_process_status(&status))
}

pub(crate) fn exit_code_from_status(status: &ExitStatusInfo) -> i32 {
    status
        .code
        .or_else(|| status.signal.map(|signal| 128 + signal))
        .unwrap_or(1)
}

fn exit_code_from_process_status(status: &std::process::ExitStatus) -> i32 {
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;

        status
            .code()
            .or_else(|| status.signal().map(|signal| 128 + signal))
            .unwrap_or(1)
    }
    #[cfg(not(unix))]
    {
        status.code().unwrap_or(1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use diag_backend_probe::{ProcessingPath, VersionBand};
    use diag_capture_runtime::{
        CaptureInvocation, CapturePlan, LocaleHandling, NativeTextCapturePolicy,
        StructuredCapturePolicy,
    };
    use diag_cascade::{CascadeError, CascadeReport, DocumentAnalyzer};
    use diag_core::{
        CompressionLevel, Confidence, DiagnosticDocument, DiagnosticNode, DocumentAnalysis,
        DocumentCompleteness, FallbackGrade, LanguageMode, Location, LocationRole, MessageText,
        NodeCompleteness, Origin, Ownership, Phase, ProducerInfo, Provenance, ProvenanceSource,
        SemanticRole, Severity, SourceAuthority, ToolInfo, WrapperSurface,
    };
    use diag_render::{
        DebugRefs, PathPolicy, RenderCapabilities, RenderProfile, RenderRequest,
        SourceExcerptPolicy, StreamKind, TypeDisplayPolicy, WarningVisibility, render,
    };
    use diag_trace::RetentionPolicy;
    use std::error::Error;
    use std::fmt::{Display, Formatter};
    use std::path::PathBuf;

    #[test]
    fn signal_exit_status_uses_conventional_code() {
        let status = ExitStatusInfo {
            code: None,
            signal: Some(15),
            success: false,
        };
        assert_eq!(exit_code_from_status(&status), 143);
    }

    #[derive(Debug)]
    struct DummyAdapterError;

    impl Display for DummyAdapterError {
        fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
            write!(f, "dummy adapter error")
        }
    }

    impl Error for DummyAdapterError {}

    struct DummyAdapter;

    impl DiagnosticAdapter for DummyAdapter {
        type Error = DummyAdapterError;

        fn ingest(
            &self,
            _bundle: &CaptureBundle,
            policy: IngestPolicy,
        ) -> Result<IngestReport, Self::Error> {
            Ok(IngestReport {
                document: DiagnosticDocument {
                    document_id: "dummy-cli-doc".to_string(),
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

    fn sample_document(document_analysis: Option<DocumentAnalysis>) -> DiagnosticDocument {
        DiagnosticDocument {
            document_id: "dummy-cli-doc".to_string(),
            schema_version: diag_core::IR_SPEC_VERSION.to_string(),
            document_completeness: DocumentCompleteness::Complete,
            producer: ProducerInfo {
                name: "gcc-formed".to_string(),
                version: "0.2.0-beta.1".to_string(),
                git_revision: None,
                build_profile: Some("test".to_string()),
                rulepack_version: Some("phase1".to_string()),
            },
            run: sample_policy().run,
            captures: Vec::new(),
            integrity_issues: Vec::new(),
            diagnostics: vec![DiagnosticNode {
                id: "root".to_string(),
                origin: Origin::Gcc,
                phase: Phase::Parse,
                severity: Severity::Error,
                semantic_role: SemanticRole::Root,
                message: MessageText {
                    raw_text: "expected ';' before '}' token".to_string(),
                    normalized_text: None,
                    locale: None,
                },
                locations: vec![
                    Location::caret("src/main.c", 2, 13, LocationRole::Primary)
                        .with_ownership(Ownership::User, "user_workspace"),
                ],
                children: Vec::new(),
                suggestions: Vec::new(),
                context_chains: Vec::new(),
                symbol_context: None,
                node_completeness: NodeCompleteness::Complete,
                provenance: Provenance {
                    source: ProvenanceSource::Compiler,
                    capture_refs: vec!["stderr.raw".to_string()],
                },
                analysis: None,
                fingerprints: None,
            }],
            document_analysis,
            fingerprints: None,
        }
    }

    fn sample_cascade_context() -> CascadeContext {
        CascadeContext {
            version_band: VersionBand::Gcc15Plus,
            processing_path: ProcessingPath::DualSinkStructured,
            source_authority: SourceAuthority::Structured,
            fallback_grade: FallbackGrade::None,
            cwd: PathBuf::from("/tmp/project"),
        }
    }

    fn sample_render_request(document: DiagnosticDocument) -> RenderRequest {
        RenderRequest {
            document,
            cascade_policy: CascadePolicySnapshot::default(),
            profile: RenderProfile::Default,
            capabilities: RenderCapabilities {
                stream_kind: StreamKind::Pipe,
                width_columns: Some(100),
                ansi_color: false,
                unicode: false,
                hyperlinks: false,
                interactive: false,
            },
            cwd: Some(PathBuf::from("/tmp/project")),
            path_policy: PathPolicy::RelativeToCwd,
            warning_visibility: WarningVisibility::Auto,
            debug_refs: DebugRefs::None,
            type_display_policy: TypeDisplayPolicy::CompactSafe,
            source_excerpt_policy: SourceExcerptPolicy::ForceOff,
        }
    }

    struct SuccessAnalyzer;

    impl DocumentAnalyzer for SuccessAnalyzer {
        fn analyze_document(
            &self,
            document: &mut DiagnosticDocument,
            _context: &CascadeContext,
            _policy: &CascadePolicySnapshot,
        ) -> Result<CascadeReport, CascadeError> {
            document.document_analysis = Some(DocumentAnalysis::default());
            Ok(CascadeReport {
                document_analysis_present: true,
            })
        }
    }

    struct ErrorAnalyzer;

    impl DocumentAnalyzer for ErrorAnalyzer {
        fn analyze_document(
            &self,
            _document: &mut DiagnosticDocument,
            _context: &CascadeContext,
            _policy: &CascadePolicySnapshot,
        ) -> Result<CascadeReport, CascadeError> {
            Err(CascadeError::Internal {
                reason: "synthetic failure".to_string(),
            })
        }
    }

    struct PanicAnalyzer;

    impl DocumentAnalyzer for PanicAnalyzer {
        fn analyze_document(
            &self,
            document: &mut DiagnosticDocument,
            _context: &CascadeContext,
            _policy: &CascadePolicySnapshot,
        ) -> Result<CascadeReport, CascadeError> {
            document.document_analysis = Some(DocumentAnalysis::default());
            panic!("synthetic panic")
        }
    }

    #[test]
    fn generic_adapter_helper_accepts_dummy_adapter() {
        let adapter = DummyAdapter;
        let report = invoke_adapter(&adapter, &sample_bundle(), sample_policy()).unwrap();

        assert_eq!(adapter.supported_origins(), &[Origin::Clang]);
        assert_eq!(report.document.document_id, "dummy-cli-doc");
        assert_eq!(report.source_authority, SourceAuthority::Structured);
    }

    #[test]
    fn run_cascade_analysis_preserves_successful_document_analysis() {
        let mut document = sample_document(None);

        let report = run_cascade_analysis(
            &SuccessAnalyzer,
            &mut document,
            &sample_cascade_context(),
            &CascadePolicySnapshot::default(),
        );

        assert_eq!(
            report,
            Some(CascadeReport {
                document_analysis_present: true,
            })
        );
        assert_eq!(
            document.document_analysis,
            Some(DocumentAnalysis::default())
        );
    }

    #[test]
    fn safe_document_analyzer_materializes_document_analysis_from_resolved_policy() {
        let mut document = sample_document(None);
        let policy = CascadePolicySnapshot {
            compression_level: CompressionLevel::Off,
            ..CascadePolicySnapshot::default()
        };

        let report = run_cascade_analysis(
            &SafeDocumentAnalyzer,
            &mut document,
            &sample_cascade_context(),
            &policy,
        );

        assert_eq!(
            report,
            Some(CascadeReport {
                document_analysis_present: true,
            })
        );
        assert_eq!(
            document
                .document_analysis
                .as_ref()
                .and_then(|analysis| analysis.policy_profile.as_deref()),
            Some("default-off")
        );
    }

    #[test]
    fn run_cascade_analysis_fails_open_on_error() {
        let mut document = sample_document(Some(DocumentAnalysis::default()));

        let report = run_cascade_analysis(
            &ErrorAnalyzer,
            &mut document,
            &sample_cascade_context(),
            &CascadePolicySnapshot::default(),
        );

        assert!(report.is_none());
        assert!(document.document_analysis.is_none());
    }

    #[test]
    fn run_cascade_analysis_fails_open_on_panic() {
        let mut document = sample_document(Some(DocumentAnalysis::default()));

        let report = run_cascade_analysis(
            &PanicAnalyzer,
            &mut document,
            &sample_cascade_context(),
            &CascadePolicySnapshot::default(),
        );

        assert!(report.is_none());
        assert!(document.document_analysis.is_none());
    }

    #[test]
    fn render_succeeds_when_document_analysis_is_none_after_fail_open() {
        let mut document = sample_document(Some(DocumentAnalysis::default()));

        let report = run_cascade_analysis(
            &ErrorAnalyzer,
            &mut document,
            &sample_cascade_context(),
            &CascadePolicySnapshot::default(),
        );
        assert!(report.is_none());
        assert!(document.document_analysis.is_none());

        let render_result = render(sample_render_request(document)).unwrap();
        assert!(!render_result.text.is_empty());
    }
}
