use diag_backend_probe::ProcessingPath;
use diag_capture_runtime::ExecutionMode;
use diag_render::{DebugRefs, RenderProfile};
use diag_trace::RetentionPolicy;
use std::ffi::OsString;
use std::path::PathBuf;

#[derive(Debug, Clone, Default)]
pub(crate) struct ParsedArgs {
    pub(crate) mode: Option<ExecutionMode>,
    pub(crate) processing_path: Option<ProcessingPath>,
    pub(crate) profile: Option<RenderProfile>,
    pub(crate) backend: Option<PathBuf>,
    pub(crate) trace: Option<RetentionPolicy>,
    pub(crate) debug_refs: Option<DebugRefs>,
    pub(crate) introspection: Option<WrapperIntrospection>,
    pub(crate) forwarded_args: Vec<OsString>,
}

impl ParsedArgs {
    pub(crate) fn parse(args: Vec<OsString>) -> Result<Self, Box<dyn std::error::Error>> {
        let mut parsed = ParsedArgs::default();
        for arg in args.into_iter().skip(1) {
            let value = arg.to_string_lossy();
            if let Some(mode) = value.strip_prefix("--formed-mode=") {
                parsed.mode = Some(parse_mode(mode)?);
            } else if let Some(path) = value.strip_prefix("--formed-processing-path=") {
                parsed.processing_path = Some(parse_processing_path(path)?);
            } else if let Some(profile) = value.strip_prefix("--formed-profile=") {
                parsed.profile = Some(parse_profile(profile)?);
            } else if let Some(path) = value.strip_prefix("--formed-backend-gcc=") {
                parsed.backend = Some(PathBuf::from(path));
            } else if let Some(policy) = value.strip_prefix("--formed-trace=") {
                parsed.trace = Some(parse_retention_policy(policy)?);
            } else if let Some(debug_refs) = value.strip_prefix("--formed-debug-refs=") {
                parsed.debug_refs = Some(parse_debug_refs(debug_refs)?);
            } else if value == "--formed-version" {
                parsed.introspection = Some(WrapperIntrospection::Version);
            } else if value == "--formed-version=verbose" {
                parsed.introspection = Some(WrapperIntrospection::VersionVerbose);
            } else if value == "--formed-print-paths" {
                parsed.introspection = Some(WrapperIntrospection::PrintPaths);
            } else if value == "--formed-self-check" {
                parsed.introspection = Some(WrapperIntrospection::SelfCheck);
            } else if value == "--formed-dump-build-manifest" {
                parsed.introspection = Some(WrapperIntrospection::DumpBuildManifest);
            } else {
                parsed.forwarded_args.push(arg);
            }
        }
        Ok(parsed)
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum WrapperIntrospection {
    Version,
    VersionVerbose,
    PrintPaths,
    SelfCheck,
    DumpBuildManifest,
}

pub(crate) fn parse_mode(value: &str) -> Result<ExecutionMode, Box<dyn std::error::Error>> {
    match value {
        "render" => Ok(ExecutionMode::Render),
        "shadow" => Ok(ExecutionMode::Shadow),
        "passthrough" => Ok(ExecutionMode::Passthrough),
        _ => Err(format!("unsupported mode: {value}").into()),
    }
}

pub(crate) fn parse_processing_path(
    value: &str,
) -> Result<ProcessingPath, Box<dyn std::error::Error>> {
    match value {
        "dual_sink_structured" => Ok(ProcessingPath::DualSinkStructured),
        "single_sink_structured" => Ok(ProcessingPath::SingleSinkStructured),
        "native_text_capture" => Ok(ProcessingPath::NativeTextCapture),
        "passthrough" => Ok(ProcessingPath::Passthrough),
        _ => Err(format!("unsupported processing path: {value}").into()),
    }
}

pub(crate) fn parse_profile(value: &str) -> Result<RenderProfile, Box<dyn std::error::Error>> {
    match value {
        "default" => Ok(RenderProfile::Default),
        "concise" => Ok(RenderProfile::Concise),
        "verbose" => Ok(RenderProfile::Verbose),
        "ci" => Ok(RenderProfile::Ci),
        "raw_fallback" => Ok(RenderProfile::RawFallback),
        _ => Err(format!("unsupported profile: {value}").into()),
    }
}

pub(crate) fn parse_retention_policy(
    value: &str,
) -> Result<RetentionPolicy, Box<dyn std::error::Error>> {
    match value {
        "never" => Ok(RetentionPolicy::Never),
        "on-wrapper-failure" => Ok(RetentionPolicy::OnWrapperFailure),
        "on-child-error" => Ok(RetentionPolicy::OnChildError),
        "always" => Ok(RetentionPolicy::Always),
        _ => Err(format!("unsupported trace policy: {value}").into()),
    }
}

pub(crate) fn parse_debug_refs(value: &str) -> Result<DebugRefs, Box<dyn std::error::Error>> {
    match value {
        "none" => Ok(DebugRefs::None),
        "trace_id" => Ok(DebugRefs::TraceId),
        "capture_ref" => Ok(DebugRefs::CaptureRef),
        _ => Err(format!("unsupported debug ref mode: {value}").into()),
    }
}

pub(crate) fn os_to_string(value: &OsString) -> String {
    value.to_string_lossy().into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_wrapper_flags_without_forwarding_them() {
        let parsed = ParsedArgs::parse(vec![
            OsString::from("gcc-formed"),
            OsString::from("--formed-mode=shadow"),
            OsString::from("--formed-processing-path=single_sink_structured"),
            OsString::from("--formed-profile=ci"),
            OsString::from("--formed-trace=always"),
            OsString::from("--formed-debug-refs=trace_id"),
            OsString::from("-c"),
            OsString::from("main.c"),
        ])
        .expect("parsed args");

        assert_eq!(parsed.mode, Some(ExecutionMode::Shadow));
        assert_eq!(
            parsed.processing_path,
            Some(ProcessingPath::SingleSinkStructured)
        );
        assert_eq!(parsed.profile, Some(RenderProfile::Ci));
        assert_eq!(parsed.trace, Some(RetentionPolicy::Always));
        assert_eq!(parsed.debug_refs, Some(DebugRefs::TraceId));
        assert_eq!(
            parsed.forwarded_args,
            vec![OsString::from("-c"), OsString::from("main.c")]
        );
    }

    #[test]
    fn keeps_last_introspection_flag() {
        let parsed = ParsedArgs::parse(vec![
            OsString::from("gcc-formed"),
            OsString::from("--formed-version"),
            OsString::from("--formed-self-check"),
        ])
        .expect("parsed args");

        assert!(matches!(
            parsed.introspection,
            Some(WrapperIntrospection::SelfCheck)
        ));
        assert!(parsed.forwarded_args.is_empty());
    }
}
