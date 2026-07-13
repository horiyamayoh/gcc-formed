use crate::args::PublicJsonSink;
use crate::error::CliError;
use crate::mode::is_compiler_introspection;
use diag_backend_probe::{ProbeResult, ProcessingPath};
use diag_capture_runtime::ExecutionMode;
use diag_public_export::{
    PublicDiagnosticExport, PublicExportContext, PublicExportInvocation, PublicExportProducer,
    PublicExportTool, PublicExportUnavailableReason, PublicPayloadIdentity, PublicReleaseIdentity,
    unavailable_export,
};
use std::ffi::OsString;
use std::fs;
use std::io::Write;

pub(crate) fn ensure_public_json_stdout_safe(
    sink: Option<&PublicJsonSink>,
    mode: ExecutionMode,
    forwarded_args: &[OsString],
) -> Result<(), CliError> {
    if !sink.is_some_and(PublicJsonSink::is_stdout) {
        return Ok(());
    }
    if matches!(mode, ExecutionMode::Passthrough)
        || is_compiler_introspection(forwarded_args)
        || compiler_stdout_may_be_used(forwarded_args)
    {
        return Err(CliError::Config(
            "public JSON export to stdout is unsafe for this invocation; use --formed-public-json=/path/to/file.json".to_string(),
        ));
    }
    Ok(())
}

pub(crate) fn write_public_json(
    sink: Option<&PublicJsonSink>,
    export: &PublicDiagnosticExport,
) -> Result<(), CliError> {
    let Some(sink) = sink else {
        return Ok(());
    };
    let payload = export.canonical_json()?;
    match sink {
        PublicJsonSink::Stdout => {
            let mut stdout = std::io::stdout().lock();
            stdout.write_all(payload.as_bytes())?;
            if !payload.ends_with('\n') {
                stdout.write_all(b"\n")?;
            }
        }
        PublicJsonSink::File(path) => {
            if let Some(parent) = path.parent()
                && !parent.as_os_str().is_empty()
            {
                fs::create_dir_all(parent)?;
            }
            fs::write(path, payload)?;
        }
    }
    Ok(())
}

pub(crate) fn export_context_for_unavailable(
    argv0: &str,
    backend: &ProbeResult,
    exit_status: i32,
    wrapper_mode: diag_core::WrapperSurface,
    processing_path: ProcessingPath,
    fallback_reason: Option<diag_core::FallbackReason>,
) -> PublicExportContext {
    PublicExportContext {
        producer: PublicExportProducer {
            name: env!("CARGO_PKG_NAME").to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            release_identity: None,
            payload_identity: None,
        },
        invocation: PublicExportInvocation {
            invocation_id: None,
            invoked_as: Some(argv0.to_string()),
            exit_status,
            primary_tool: Some(PublicExportTool {
                name: backend
                    .resolved_path
                    .file_name()
                    .and_then(|value| value.to_str())
                    .unwrap_or("gcc")
                    .to_string(),
                version: Some(backend.version_string.clone()),
                component: None,
                vendor: None,
            }),
            language_mode: None,
            wrapper_mode: Some(snake_case_label(&wrapper_mode)),
        },
        version_band: backend.version_band(),
        processing_path,
        support_level: backend.support_level(),
        allowed_processing_paths: backend
            .capability_profile()
            .allowed_processing_paths
            .iter()
            .copied()
            .collect(),
        source_authority: None,
        fallback_grade: None,
        fallback_reason,
    }
    .with_runtime_identity_from_install()
}

pub(crate) trait RuntimeExportIdentity {
    fn with_runtime_identity_from_install(self) -> Self;
}

impl RuntimeExportIdentity for PublicExportContext {
    fn with_runtime_identity_from_install(self) -> Self {
        let identity = diag_trace::current_runtime_identity();
        self.with_runtime_identity(
            identity
                .release_identity
                .map(|release| PublicReleaseIdentity {
                    version: release.version,
                    channel: release.channel,
                    attestation_source: release.attestation_source,
                }),
            PublicPayloadIdentity {
                product_version: identity.payload_identity.product_version,
                git_commit: identity.payload_identity.git_commit,
                primary_archive_sha256: identity.payload_identity.primary_archive_sha256,
            },
        )
    }
}

pub(crate) fn unavailable_export_with_reason(
    context: &PublicExportContext,
    reason: PublicExportUnavailableReason,
) -> PublicDiagnosticExport {
    unavailable_export(context, reason)
}

fn compiler_stdout_may_be_used(args: &[OsString]) -> bool {
    let mut expect_output_path = false;
    for arg in args {
        let value = arg.to_string_lossy();
        if expect_output_path {
            if value == "-" {
                return true;
            }
            expect_output_path = false;
            continue;
        }
        if matches!(value.as_ref(), "-E" | "-M" | "-MM") || value == "-o-" {
            return true;
        }
        if value == "-o" {
            expect_output_path = true;
        }
    }
    false
}

fn snake_case_label<T: serde::Serialize>(value: &T) -> String {
    serde_json::to_value(value)
        .ok()
        .and_then(|value| value.as_str().map(|value| value.to_string()))
        .unwrap_or_else(|| "unknown".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stdout_export_rejects_preprocess_and_passthrough_invocations() {
        let sink = PublicJsonSink::Stdout;
        assert!(
            ensure_public_json_stdout_safe(
                Some(&sink),
                ExecutionMode::Render,
                &[OsString::from("-E"), OsString::from("main.c")]
            )
            .is_err()
        );
        assert!(
            ensure_public_json_stdout_safe(Some(&sink), ExecutionMode::Passthrough, &[]).is_err()
        );
    }

    #[test]
    fn stdout_export_allows_normal_render_invocations() {
        let sink = PublicJsonSink::Stdout;
        assert!(
            ensure_public_json_stdout_safe(
                Some(&sink),
                ExecutionMode::Render,
                &[OsString::from("-c"), OsString::from("main.c")]
            )
            .is_ok()
        );
    }
}
