use criterion::{Criterion, black_box, criterion_group, criterion_main};

use diag_adapter_gcc::{IngestPolicy, ingest_bundle, producer_for_version, tool_for_backend};
use diag_backend_probe::ProcessingPath;
use diag_capture_runtime::{
    CaptureBundle, CaptureInvocation, CapturePlan, ExecutionMode, ExitStatusInfo, LocaleHandling,
    NativeTextCapturePolicy, StructuredCapturePolicy,
};
use diag_core::{
    ArtifactKind, ArtifactStorage, CaptureArtifact, LanguageMode, RunInfo, WrapperSurface,
};
use diag_trace::RetentionPolicy;

fn base_run_info() -> RunInfo {
    RunInfo {
        invocation_id: "bench-inv".to_string(),
        invoked_as: Some("gcc-formed".to_string()),
        argv_redacted: vec!["gcc".to_string(), "-c".to_string(), "main.c".to_string()],
        cwd_display: None,
        exit_status: 1,
        primary_tool: tool_for_backend("gcc", Some("15.2.0".to_string())),
        secondary_tools: Vec::new(),
        language_mode: Some(LanguageMode::C),
        target_triple: None,
        wrapper_mode: Some(WrapperSurface::Terminal),
    }
}

fn build_residual_bundle(stderr_text: &str) -> CaptureBundle {
    let run = base_run_info();
    let primary_tool = run.primary_tool.clone();
    CaptureBundle {
        plan: CapturePlan {
            execution_mode: ExecutionMode::Passthrough,
            processing_path: ProcessingPath::Passthrough,
            structured_capture: StructuredCapturePolicy::Disabled,
            native_text_capture: NativeTextCapturePolicy::Passthrough,
            preserve_native_color: false,
            locale_handling: LocaleHandling::Preserve,
            retention_policy: RetentionPolicy::Never,
        },
        invocation: CaptureInvocation {
            backend_path: run.primary_tool.name.clone(),
            launcher_path: None,
            spawn_path: run.primary_tool.name.clone(),
            argv: run.argv_redacted.clone(),
            spawn_argv: run.argv_redacted.clone(),
            argv_hash: diag_core::fingerprint_for(&run.argv_redacted),
            cwd: String::new(),
            selected_mode: ExecutionMode::Passthrough,
            processing_path: ProcessingPath::Passthrough,
        },
        raw_text_artifacts: vec![CaptureArtifact {
            id: "stderr.raw".to_string(),
            kind: ArtifactKind::CompilerStderrText,
            media_type: "text/plain".to_string(),
            encoding: Some("utf-8".to_string()),
            digest_sha256: None,
            size_bytes: Some(stderr_text.len() as u64),
            storage: ArtifactStorage::Inline,
            inline_text: Some(stderr_text.to_string()),
            external_ref: None,
            produced_by: Some(primary_tool),
        }],
        structured_artifacts: Vec::new(),
        exit_status: ExitStatusInfo {
            code: Some(1),
            signal: None,
            success: false,
        },
        integrity_issues: Vec::new(),
    }
}

const MIXED_STDERR: &str = "\
src/config_a.h:1:23: error: first missing symbol\n\
src/main.c:3:25: note: in expansion of macro 'FETCH_A'\n\
src/config_b.h:2:11: error: second missing symbol\n\
src/other.c:8:9: note: in expansion of macro 'FETCH_B'\n\
main.cpp:5:7: error: no matching function for call to 'takes(int)'\n\
main.cpp:2:6: note: candidate 1: 'void takes(int, int)'\n";

fn bench_ingest_bundle(c: &mut Criterion) {
    let bundle = build_residual_bundle(MIXED_STDERR);
    c.bench_function("ingest_bundle_residual", |b| {
        b.iter(|| {
            let policy = IngestPolicy {
                producer: producer_for_version("0.1.0"),
                run: base_run_info(),
            };
            ingest_bundle(black_box(&bundle), black_box(policy)).unwrap()
        });
    });
}

criterion_group!(benches, bench_ingest_bundle);
criterion_main!(benches);
