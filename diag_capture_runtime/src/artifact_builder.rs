//! Builder helpers and internal types for constructing capture artifacts and invocation records.

use crate::STDERR_CAPTURE_ID;
use crate::artifact::{CaptureBundle, CaptureInvocation, ExitStatusInfo};
use crate::policy::{
    CapturePlan, CaptureRequest, ChildEnvPolicy, ExecutionMode, StructuredCapturePolicy,
    child_env_policy_is_empty, collect_wrapper_env,
};
use diag_backend_probe::ProbeResult;
use diag_core::{
    ArtifactKind, ArtifactStorage, CaptureArtifact, IntegrityIssue, ToolInfo, fingerprint_for,
};
use diag_trace::secure_private_file;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) struct InvocationRecord {
    pub(crate) backend_path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) launcher_path: Option<String>,
    pub(crate) spawn_path: String,
    pub(crate) argv: Vec<String>,
    pub(crate) spawn_argv: Vec<String>,
    pub(crate) argv_hash: String,
    pub(crate) normalized_invocation: NormalizedInvocation,
    pub(crate) redaction_class: String,
    pub(crate) selected_mode: ExecutionMode,
    pub(crate) cwd: String,
    pub(crate) sarif_path: Option<String>,
    #[serde(skip_serializing_if = "child_env_policy_is_empty", default)]
    pub(crate) child_env_policy: ChildEnvPolicy,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub(crate) wrapper_env: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) struct NormalizedInvocation {
    pub(crate) arg_count: usize,
    pub(crate) input_count: usize,
    pub(crate) compile_only: bool,
    pub(crate) preprocess_only: bool,
    pub(crate) assemble_only: bool,
    pub(crate) output_requested: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) language_override: Option<String>,
    pub(crate) include_path_count: usize,
    pub(crate) define_count: usize,
    pub(crate) diagnostics_flag_count: usize,
    pub(crate) injected_flag_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct ArtifactFingerprint {
    pub(crate) modified_ns: u128,
    pub(crate) size_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DiscoveredStructuredArtifact {
    pub(crate) path: PathBuf,
    pub(crate) existed_before: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CapturedStderr {
    pub(crate) preview_bytes: Vec<u8>,
    pub(crate) total_bytes: u64,
    pub(crate) truncated_bytes: u64,
    pub(crate) spool_path: PathBuf,
}

impl CapturedStderr {
    pub(crate) fn empty(spool_path: PathBuf) -> Self {
        Self {
            preview_bytes: Vec::new(),
            total_bytes: 0,
            truncated_bytes: 0,
            spool_path,
        }
    }

    #[cfg(test)]
    pub(crate) fn truncated(&self) -> bool {
        self.truncated_bytes > 0
    }

    pub(crate) fn integrity_issues(&self) -> Vec<IntegrityIssue> {
        crate::policy::stderr_truncation_issues(self.total_bytes, self.truncated_bytes)
    }
}

pub(crate) fn authoritative_structured_path(
    policy: StructuredCapturePolicy,
    temp_dir: &Path,
) -> Option<PathBuf> {
    structured_artifact_file_name(policy).map(|file_name| temp_dir.join(file_name))
}

pub(crate) fn structured_artifact_file_name(
    policy: StructuredCapturePolicy,
) -> Option<&'static str> {
    match policy {
        StructuredCapturePolicy::Disabled => None,
        StructuredCapturePolicy::SarifFile | StructuredCapturePolicy::SingleSinkSarifFile => {
            Some("diagnostics.sarif")
        }
        StructuredCapturePolicy::SingleSinkJsonFile => Some("diagnostics.json"),
    }
}

pub(crate) fn structured_artifact_extension(
    policy: StructuredCapturePolicy,
) -> Option<&'static str> {
    match policy {
        StructuredCapturePolicy::Disabled => None,
        StructuredCapturePolicy::SarifFile | StructuredCapturePolicy::SingleSinkSarifFile => {
            Some("sarif")
        }
        StructuredCapturePolicy::SingleSinkJsonFile => Some("json"),
    }
}

pub(crate) fn single_sink_structured_capture(policy: StructuredCapturePolicy) -> bool {
    matches!(
        policy,
        StructuredCapturePolicy::SingleSinkSarifFile | StructuredCapturePolicy::SingleSinkJsonFile
    )
}

pub(crate) fn structured_artifact_metadata(
    path: &Path,
) -> Option<(&'static str, ArtifactKind, &'static str)> {
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

pub(crate) fn build_artifacts(
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

pub(crate) fn snapshot_structured_artifacts(
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

pub(crate) fn artifact_fingerprint(metadata: &fs::Metadata) -> ArtifactFingerprint {
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

pub(crate) fn discover_structured_artifact(
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

pub(crate) fn preserve_discovered_artifact(
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

pub(crate) fn build_capture_bundle(
    request: &CaptureRequest,
    final_args: &[OsString],
    spawn_args: &[OsString],
    plan: &CapturePlan,
    exit_status: &ExitStatusInfo,
    artifacts: &[CaptureArtifact],
    integrity_issues: &[IntegrityIssue],
) -> CaptureBundle {
    CaptureBundle {
        plan: *plan,
        invocation: build_capture_invocation(request, final_args, spawn_args, plan),
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

pub(crate) fn build_capture_invocation(
    request: &CaptureRequest,
    final_args: &[OsString],
    spawn_args: &[OsString],
    plan: &CapturePlan,
) -> CaptureInvocation {
    let argv = final_args
        .iter()
        .map(|arg| arg.to_string_lossy().into_owned())
        .collect::<Vec<_>>();
    let spawn_argv = spawn_args
        .iter()
        .map(|arg| arg.to_string_lossy().into_owned())
        .collect::<Vec<_>>();
    CaptureInvocation {
        backend_path: request.backend.resolved_path.display().to_string(),
        launcher_path: request
            .backend
            .execution_topology
            .launcher_path
            .as_ref()
            .map(|path| path.display().to_string()),
        spawn_path: request.backend.spawn_path().display().to_string(),
        argv_hash: fingerprint_for(&argv),
        argv,
        spawn_argv,
        cwd: request.cwd.display().to_string(),
        selected_mode: plan.execution_mode,
        processing_path: plan.processing_path,
    }
}

pub(crate) fn tool_info(backend: &ProbeResult) -> ToolInfo {
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

pub(crate) fn build_invocation_record(
    request: &CaptureRequest,
    plan: &CapturePlan,
    final_args: &[OsString],
    spawn_args: &[OsString],
    sarif_path: Option<&Path>,
    child_env_policy: ChildEnvPolicy,
) -> InvocationRecord {
    let argv = final_args
        .iter()
        .map(|arg| arg.to_string_lossy().into_owned())
        .collect::<Vec<_>>();
    let spawn_argv = spawn_args
        .iter()
        .map(|arg| arg.to_string_lossy().into_owned())
        .collect::<Vec<_>>();
    InvocationRecord {
        backend_path: request.backend.resolved_path.display().to_string(),
        launcher_path: request
            .backend
            .execution_topology
            .launcher_path
            .as_ref()
            .map(|path| path.display().to_string()),
        spawn_path: request.backend.spawn_path().display().to_string(),
        argv_hash: fingerprint_for(&argv),
        normalized_invocation: normalize_invocation(&argv, request.args.len()),
        redaction_class: "restricted".to_string(),
        argv,
        spawn_argv,
        selected_mode: plan.execution_mode,
        cwd: request.cwd.display().to_string(),
        sarif_path: sarif_path.map(|path| path.display().to_string()),
        child_env_policy,
        wrapper_env: collect_wrapper_env(),
    }
}

pub(crate) fn normalize_invocation(argv: &[String], user_arg_count: usize) -> NormalizedInvocation {
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
    let mut skip_next_option_value = false;

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
        if skip_next_option_value {
            skip_next_option_value = false;
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
            "-I" => {
                include_path_count += 1;
                skip_next_option_value = true;
            }
            "-D" => {
                define_count += 1;
                skip_next_option_value = true;
            }
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
            _ if takes_separate_option_value(arg) => skip_next_option_value = true,
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

fn takes_separate_option_value(arg: &str) -> bool {
    matches!(
        arg,
        "-U" | "-include"
            | "-imacros"
            | "-iquote"
            | "-isystem"
            | "-idirafter"
            | "-iprefix"
            | "-iwithprefix"
            | "-iwithprefixbefore"
            | "-isysroot"
            | "--sysroot"
            | "-MF"
            | "-MT"
            | "-MQ"
            | "-L"
            | "-B"
            | "-specs"
            | "-wrapper"
            | "-Xassembler"
            | "-Xpreprocessor"
            | "-Xlinker"
            | "-Xclang"
            | "-dumpdir"
            | "-dumpbase"
            | "-dumpbase-ext"
    )
}

pub(crate) fn write_invocation_record(
    path: &Path,
    record: &InvocationRecord,
) -> Result<(), std::io::Error> {
    let json = serde_json::to_vec_pretty(record).map_err(std::io::Error::other)?;
    fs::write(path, json)?;
    secure_private_file(path)
}

pub(crate) fn status_to_info(status: std::process::ExitStatus) -> ExitStatusInfo {
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
