use crate::error::CliError;
use diag_backend_probe::ProcessingPath;
use diag_capture_runtime::ExecutionMode;
use diag_core::{CompressionLevel, SuppressedCountVisibility};
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
    pub(crate) launcher: Option<PathBuf>,
    pub(crate) trace: Option<RetentionPolicy>,
    pub(crate) trace_bundle: Option<TraceBundleSink>,
    pub(crate) debug_refs: Option<DebugRefs>,
    pub(crate) public_json: Option<PublicJsonSink>,
    pub(crate) cascade_compression_level: Option<CompressionLevel>,
    pub(crate) cascade_suppress_likelihood_threshold: Option<f32>,
    pub(crate) cascade_summary_likelihood_threshold: Option<f32>,
    pub(crate) cascade_min_parent_margin: Option<f32>,
    pub(crate) cascade_max_expanded_independent_roots: Option<usize>,
    pub(crate) cascade_show_suppressed_count: Option<SuppressedCountVisibility>,
    pub(crate) introspection: Option<WrapperIntrospection>,
    pub(crate) forwarded_args: Vec<OsString>,
}

impl ParsedArgs {
    pub(crate) fn parse(args: Vec<OsString>) -> Result<Self, CliError> {
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
            } else if let Some(path) = value.strip_prefix("--formed-backend-launcher=") {
                parsed.launcher = Some(PathBuf::from(path));
            } else if let Some(policy) = value.strip_prefix("--formed-trace=") {
                parsed.trace = Some(parse_retention_policy(policy)?);
            } else if value == "--formed-trace-bundle" {
                parsed.trace_bundle = Some(TraceBundleSink::Auto);
            } else if let Some(path) = value.strip_prefix("--formed-trace-bundle=") {
                parsed.trace_bundle = Some(parse_trace_bundle_sink(path));
            } else if let Some(debug_refs) = value.strip_prefix("--formed-debug-refs=") {
                parsed.debug_refs = Some(parse_debug_refs(debug_refs)?);
            } else if let Some(sink) = value.strip_prefix("--formed-public-json=") {
                parsed.public_json = Some(parse_public_json_sink(sink));
            } else if let Some(level) = value.strip_prefix("--formed-cascade-level=") {
                parsed.cascade_compression_level = Some(parse_compression_level(level)?);
            } else if let Some(threshold) =
                value.strip_prefix("--formed-cascade-suppress-threshold=")
            {
                parsed.cascade_suppress_likelihood_threshold =
                    Some(parse_probability("cascade suppress threshold", threshold)?);
            } else if let Some(threshold) =
                value.strip_prefix("--formed-cascade-summary-threshold=")
            {
                parsed.cascade_summary_likelihood_threshold =
                    Some(parse_probability("cascade summary threshold", threshold)?);
            } else if let Some(margin) = value.strip_prefix("--formed-cascade-min-parent-margin=") {
                parsed.cascade_min_parent_margin =
                    Some(parse_probability("cascade min parent margin", margin)?);
            } else if let Some(limit) =
                value.strip_prefix("--formed-cascade-max-expanded-independent-roots=")
            {
                parsed.cascade_max_expanded_independent_roots = Some(parse_usize(
                    "cascade max expanded independent roots",
                    limit,
                )?);
            } else if let Some(limit) =
                value.strip_prefix("--formed-max-expanded-independent-roots=")
            {
                parsed.cascade_max_expanded_independent_roots = Some(parse_usize(
                    "cascade max expanded independent roots",
                    limit,
                )?);
            } else if let Some(mode) = value.strip_prefix("--formed-cascade-show-suppressed-count=")
            {
                parsed.cascade_show_suppressed_count =
                    Some(parse_suppressed_count_visibility(mode)?);
            } else if let Some(mode) = value.strip_prefix("--formed-show-suppressed-count=") {
                parsed.cascade_show_suppressed_count =
                    Some(parse_suppressed_count_visibility(mode)?);
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum PublicJsonSink {
    Stdout,
    File(PathBuf),
}

impl PublicJsonSink {
    pub(crate) fn is_stdout(&self) -> bool {
        matches!(self, Self::Stdout)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum TraceBundleSink {
    Auto,
    File(PathBuf),
}

pub(crate) fn parse_mode(value: &str) -> Result<ExecutionMode, CliError> {
    match value {
        "render" => Ok(ExecutionMode::Render),
        "shadow" => Ok(ExecutionMode::Shadow),
        "passthrough" => Ok(ExecutionMode::Passthrough),
        _ => Err(CliError::Config(format!("unsupported mode: {value}"))),
    }
}

pub(crate) fn parse_processing_path(value: &str) -> Result<ProcessingPath, CliError> {
    match value {
        "dual_sink_structured" => Ok(ProcessingPath::DualSinkStructured),
        "single_sink_structured" => Ok(ProcessingPath::SingleSinkStructured),
        "native_text_capture" => Ok(ProcessingPath::NativeTextCapture),
        "passthrough" => Ok(ProcessingPath::Passthrough),
        _ => Err(CliError::Config(format!(
            "unsupported processing path: {value}"
        ))),
    }
}

pub(crate) fn parse_profile(value: &str) -> Result<RenderProfile, CliError> {
    match value {
        "default" => Ok(RenderProfile::Default),
        "concise" => Ok(RenderProfile::Concise),
        "verbose" => Ok(RenderProfile::Verbose),
        "debug" => Ok(RenderProfile::Debug),
        "ci" => Ok(RenderProfile::Ci),
        "raw_fallback" => Ok(RenderProfile::RawFallback),
        _ => Err(CliError::Config(format!("unsupported profile: {value}"))),
    }
}

pub(crate) fn parse_retention_policy(value: &str) -> Result<RetentionPolicy, CliError> {
    match value {
        "never" => Ok(RetentionPolicy::Never),
        "on-wrapper-failure" => Ok(RetentionPolicy::OnWrapperFailure),
        "on-child-error" => Ok(RetentionPolicy::OnChildError),
        "always" => Ok(RetentionPolicy::Always),
        _ => Err(CliError::Config(format!(
            "unsupported trace policy: {value}"
        ))),
    }
}

pub(crate) fn parse_debug_refs(value: &str) -> Result<DebugRefs, CliError> {
    match value {
        "none" => Ok(DebugRefs::None),
        "trace_id" => Ok(DebugRefs::TraceId),
        "capture_ref" => Ok(DebugRefs::CaptureRef),
        _ => Err(CliError::Config(format!(
            "unsupported debug ref mode: {value}"
        ))),
    }
}

pub(crate) fn parse_public_json_sink(value: &str) -> PublicJsonSink {
    if value == "-" || value == "stdout" {
        PublicJsonSink::Stdout
    } else {
        PublicJsonSink::File(PathBuf::from(value))
    }
}

pub(crate) fn parse_trace_bundle_sink(value: &str) -> TraceBundleSink {
    if value == "auto" {
        TraceBundleSink::Auto
    } else {
        TraceBundleSink::File(PathBuf::from(value))
    }
}

pub(crate) fn parse_compression_level(value: &str) -> Result<CompressionLevel, CliError> {
    match value {
        "off" => Ok(CompressionLevel::Off),
        "conservative" => Ok(CompressionLevel::Conservative),
        "balanced" => Ok(CompressionLevel::Balanced),
        "aggressive" => Ok(CompressionLevel::Aggressive),
        _ => Err(CliError::Config(format!(
            "unsupported cascade level: {value}"
        ))),
    }
}

pub(crate) fn parse_suppressed_count_visibility(
    value: &str,
) -> Result<SuppressedCountVisibility, CliError> {
    match value {
        "auto" => Ok(SuppressedCountVisibility::Auto),
        "always" => Ok(SuppressedCountVisibility::Always),
        "never" => Ok(SuppressedCountVisibility::Never),
        _ => Err(CliError::Config(format!(
            "unsupported suppressed count visibility: {value}"
        ))),
    }
}

pub(crate) fn parse_probability(label: &str, value: &str) -> Result<f32, CliError> {
    let parsed = value
        .parse::<f32>()
        .map_err(|_| CliError::Config(format!("invalid {label}: {value}")))?;
    if !parsed.is_finite() || !(0.0..=1.0).contains(&parsed) {
        return Err(CliError::Config(format!(
            "{label} must be within 0.0..=1.0: {value}"
        )));
    }
    Ok(parsed)
}

pub(crate) fn parse_usize(label: &str, value: &str) -> Result<usize, CliError> {
    value
        .parse::<usize>()
        .map_err(|_| CliError::Config(format!("invalid {label}: {value}")))
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
            OsString::from("--formed-backend-launcher=/usr/bin/ccache"),
            OsString::from("--formed-trace=always"),
            OsString::from("--formed-trace-bundle=artifacts/case.trace-bundle.tar.gz"),
            OsString::from("--formed-debug-refs=trace_id"),
            OsString::from("--formed-public-json=out.json"),
            OsString::from("--formed-cascade-level=balanced"),
            OsString::from("--formed-cascade-suppress-threshold=0.81"),
            OsString::from("--formed-cascade-summary-threshold=0.61"),
            OsString::from("--formed-cascade-min-parent-margin=0.14"),
            OsString::from("--formed-cascade-max-expanded-independent-roots=3"),
            OsString::from("--formed-cascade-show-suppressed-count=always"),
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
        assert_eq!(parsed.launcher, Some(PathBuf::from("/usr/bin/ccache")));
        assert_eq!(parsed.trace, Some(RetentionPolicy::Always));
        assert_eq!(
            parsed.trace_bundle,
            Some(TraceBundleSink::File(PathBuf::from(
                "artifacts/case.trace-bundle.tar.gz"
            )))
        );
        assert_eq!(parsed.debug_refs, Some(DebugRefs::TraceId));
        assert_eq!(
            parsed.public_json,
            Some(PublicJsonSink::File(PathBuf::from("out.json")))
        );
        assert_eq!(
            parsed.cascade_compression_level,
            Some(CompressionLevel::Balanced)
        );
        assert_eq!(parsed.cascade_suppress_likelihood_threshold, Some(0.81));
        assert_eq!(parsed.cascade_summary_likelihood_threshold, Some(0.61));
        assert_eq!(parsed.cascade_min_parent_margin, Some(0.14));
        assert_eq!(parsed.cascade_max_expanded_independent_roots, Some(3));
        assert_eq!(
            parsed.cascade_show_suppressed_count,
            Some(SuppressedCountVisibility::Always)
        );
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

    #[test]
    fn parses_debug_profile_flag() {
        let parsed = ParsedArgs::parse(vec![
            OsString::from("gcc-formed"),
            OsString::from("--formed-profile=debug"),
            OsString::from("main.c"),
        ])
        .expect("parsed args");

        assert_eq!(parsed.profile, Some(RenderProfile::Debug));
        assert_eq!(parsed.forwarded_args, vec![OsString::from("main.c")]);
    }

    #[test]
    fn accepts_compatibility_aliases_for_long_cascade_flags() {
        let parsed = ParsedArgs::parse(vec![
            OsString::from("gcc-formed"),
            OsString::from("--formed-max-expanded-independent-roots=4"),
            OsString::from("--formed-show-suppressed-count=never"),
        ])
        .expect("parsed args");

        assert_eq!(parsed.cascade_max_expanded_independent_roots, Some(4));
        assert_eq!(
            parsed.cascade_show_suppressed_count,
            Some(SuppressedCountVisibility::Never)
        );
    }

    #[test]
    fn parses_auto_trace_bundle_flag() {
        let parsed = ParsedArgs::parse(vec![
            OsString::from("gcc-formed"),
            OsString::from("--formed-trace-bundle"),
            OsString::from("-c"),
            OsString::from("main.c"),
        ])
        .expect("parsed args");

        assert_eq!(parsed.trace_bundle, Some(TraceBundleSink::Auto));
        assert_eq!(
            parsed.forwarded_args,
            vec![OsString::from("-c"), OsString::from("main.c")]
        );
    }

    #[test]
    fn rejects_invalid_cascade_threshold() {
        let error = ParsedArgs::parse(vec![
            OsString::from("gcc-formed"),
            OsString::from("--formed-cascade-summary-threshold=1.2"),
        ])
        .unwrap_err();

        assert!(error.to_string().contains("cascade summary threshold"));
    }

    #[test]
    fn parses_public_json_stdout_sink() {
        let parsed = ParsedArgs::parse(vec![
            OsString::from("gcc-formed"),
            OsString::from("--formed-public-json=-"),
        ])
        .expect("parsed args");

        assert_eq!(parsed.public_json, Some(PublicJsonSink::Stdout));
    }
}
