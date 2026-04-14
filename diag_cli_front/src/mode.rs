use crate::args::os_to_string;
use diag_backend_probe::{
    CapabilityProfile, ProbeResult, ProcessingPath, SupportLevel, VersionBand,
    capability_profile_for_major,
};
use diag_capture_runtime::ExecutionMode;
use diag_core::{FallbackReason, LanguageMode};
use diag_render::{DebugRefs, RenderCapabilities, RenderProfile, StreamKind};
use diag_trace::RetentionPolicy;
use std::collections::BTreeSet;
use std::env;
use std::ffi::OsString;
use std::io::IsTerminal;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ModeDecision {
    pub(crate) mode: ExecutionMode,
    pub(crate) fallback_reason: Option<FallbackReason>,
    pub(crate) decision_log: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CliCompatibilitySeam {
    version_band: VersionBand,
    support_level: SupportLevel,
    default_processing_path: ProcessingPath,
    allowed_processing_paths: BTreeSet<ProcessingPath>,
    sarif_diagnostics: bool,
    tty_color_control: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct OperatorGuidance {
    pub(crate) summary: &'static str,
    pub(crate) representative_limitations: &'static [&'static str],
    pub(crate) actionable_next_steps: &'static [&'static str],
    pub(crate) c_first_focus_areas: &'static [&'static str],
}

impl CliCompatibilitySeam {
    pub(crate) fn from_probe(backend: &ProbeResult) -> Self {
        Self::from_profile(backend.capability_profile())
    }

    pub(crate) fn from_version_band(version_band: VersionBand) -> Self {
        let representative_major = representative_major_for_band(version_band);
        Self::from_profile(capability_profile_for_major(representative_major))
    }

    fn from_profile(profile: CapabilityProfile) -> Self {
        Self {
            version_band: profile.version_band,
            support_level: profile.support_level,
            default_processing_path: profile.default_processing_path,
            allowed_processing_paths: profile.allowed_processing_paths,
            sarif_diagnostics: profile.sarif_diagnostics,
            tty_color_control: profile.tty_color_control,
        }
    }

    fn allows_processing_path(&self, path: ProcessingPath) -> bool {
        self.allowed_processing_paths.contains(&path)
    }

    fn select_processing_path(
        &self,
        mode: ExecutionMode,
        requested: Option<ProcessingPath>,
    ) -> Result<ProcessingPath, String> {
        if matches!(mode, ExecutionMode::Passthrough) {
            return Ok(ProcessingPath::Passthrough);
        }

        let selected = requested.unwrap_or(self.default_processing_path);
        if matches!(selected, ProcessingPath::Passthrough) {
            return Err("passthrough processing path requires passthrough mode".to_string());
        }
        if !self.allows_processing_path(selected) {
            return Err(format!(
                "requested processing path `{}` is not supported for this backend",
                processing_path_label(selected)
            ));
        }
        if matches!(mode, ExecutionMode::Shadow)
            && matches!(selected, ProcessingPath::SingleSinkStructured)
        {
            return Err(
                "single_sink_structured is only supported with render mode on this backend"
                    .to_string(),
            );
        }
        Ok(selected)
    }

    fn is_in_scope(&self) -> bool {
        matches!(self.support_level, SupportLevel::InScope)
    }

    pub(crate) fn support_level(&self) -> SupportLevel {
        self.support_level
    }

    pub(crate) fn should_inject_sarif(
        &self,
        mode: ExecutionMode,
        processing_path: ProcessingPath,
    ) -> bool {
        mode != ExecutionMode::Passthrough
            && self.sarif_diagnostics
            && matches!(processing_path, ProcessingPath::DualSinkStructured)
    }

    pub(crate) fn prefers_json_single_sink(&self) -> bool {
        matches!(self.version_band, VersionBand::Gcc9_12)
    }

    pub(crate) fn should_preserve_tty_color(
        &self,
        mode: ExecutionMode,
        processing_path: ProcessingPath,
        capabilities: &RenderCapabilities,
        forwarded_args: &[OsString],
    ) -> bool {
        mode == ExecutionMode::Render
            && !matches!(processing_path, ProcessingPath::Passthrough)
            && self.tty_color_control
            && matches!(capabilities.stream_kind, StreamKind::Tty)
            && capabilities.interactive
            && capabilities.ansi_color
            && !has_color_control_override(forwarded_args)
    }
}

pub(crate) fn is_compiler_introspection(args: &[OsString]) -> bool {
    args.iter().any(|arg| {
        let value = os_to_string(arg);
        matches!(
            value.as_str(),
            "--help"
                | "--version"
                | "-###"
                | "-dumpmachine"
                | "-dumpversion"
                | "-dumpfullversion"
                | "-dumpspecs"
        ) || value.starts_with("-print-")
    })
}

pub(crate) fn has_hard_conflict(args: &[OsString]) -> bool {
    args.iter().any(|arg| {
        let value = os_to_string(arg);
        value.starts_with("-fdiagnostics-format=")
            || value.starts_with("-fdiagnostics-add-output=")
            || value.starts_with("-fdiagnostics-set-output=")
            || value == "-fdiagnostics-parseable-fixits"
            || value == "-fdiagnostics-generate-patch"
    })
}

pub(crate) fn has_color_control_override(args: &[OsString]) -> bool {
    args.iter().any(|arg| {
        let value = os_to_string(arg);
        value == "-fno-diagnostics-color"
            || value.starts_with("-fdiagnostics-color=")
            || value.starts_with("-fdiagnostics-color ")
    })
}

#[cfg(test)]
pub(crate) fn select_mode(
    version_band: VersionBand,
    requested: Option<ExecutionMode>,
    hard_conflict: bool,
) -> ModeDecision {
    let seam = CliCompatibilitySeam::from_version_band(version_band);
    select_mode_for_seam(&seam, requested, hard_conflict)
}

pub(crate) fn select_mode_for_seam(
    seam: &CliCompatibilitySeam,
    requested: Option<ExecutionMode>,
    hard_conflict: bool,
) -> ModeDecision {
    let mut decision_log = vec![
        format!("version_band={}", version_band_label(seam.version_band)),
        format!("support_level={}", support_level_label(seam.support_level)),
        format!(
            "default_processing_path={}",
            processing_path_label(seam.default_processing_path)
        ),
    ];
    if hard_conflict {
        decision_log.push("hard_conflict=diagnostic_sink_override".to_string());
        return ModeDecision {
            mode: ExecutionMode::Passthrough,
            fallback_reason: Some(FallbackReason::IncompatibleSink),
            decision_log,
        };
    }
    if let Some(ExecutionMode::Passthrough) = requested {
        decision_log.push("requested_mode=passthrough".to_string());
        return ModeDecision {
            mode: ExecutionMode::Passthrough,
            fallback_reason: Some(FallbackReason::UserOptOut),
            decision_log,
        };
    }
    let mode = if seam.is_in_scope() {
        let selected = requested.unwrap_or(ExecutionMode::Render);
        decision_log.push(format!("selected_mode={}", execution_mode_label(selected)));
        selected
    } else {
        decision_log.push("default_mode=passthrough_only".to_string());
        ExecutionMode::Passthrough
    };

    let fallback_reason = match mode {
        ExecutionMode::Passthrough => matches!(
            seam.version_band,
            VersionBand::Gcc16Plus | VersionBand::Unknown
        )
        .then_some(FallbackReason::UnsupportedVersionBand),
        ExecutionMode::Shadow => Some(FallbackReason::ShadowMode),
        ExecutionMode::Render => None,
    };

    ModeDecision {
        mode,
        fallback_reason,
        decision_log,
    }
}

pub(crate) fn select_processing_path_for_seam(
    seam: &CliCompatibilitySeam,
    decision: &ModeDecision,
    requested: Option<ProcessingPath>,
) -> Result<ProcessingPath, String> {
    seam.select_processing_path(decision.mode, requested)
}

#[cfg(test)]
pub(crate) fn compatibility_scope_notice(
    version_band: VersionBand,
    decision: &ModeDecision,
) -> Option<String> {
    let seam = CliCompatibilitySeam::from_version_band(version_band);
    compatibility_scope_notice_for_path(
        &seam,
        decision,
        select_processing_path_for_seam(&seam, decision, None)
            .expect("default processing path should remain valid for the seam"),
    )
}

pub(crate) fn compatibility_scope_notice_for_path(
    seam: &CliCompatibilitySeam,
    decision: &ModeDecision,
    processing_path: ProcessingPath,
) -> Option<String> {
    let context = notice_context(seam, decision, processing_path);

    if !seam.is_in_scope() {
        return Some(format!(
            "{context}; this compiler version is outside the current GCC 9-15 contract and conservative raw diagnostics will be preserved; operator next step=use raw gcc/g++ or --formed-mode=passthrough until an in-scope VersionBand is confirmed."
        ));
    }

    match (decision.mode, processing_path, decision.fallback_reason) {
        (ExecutionMode::Shadow, _, _) => Some(format!(
            "{context}; shadow capture is active under the shared GCC 9-15 in-scope contract and emits capability-specific debug metadata without changing the public contract."
        )),
        (ExecutionMode::Passthrough, _, Some(FallbackReason::UserOptOut)) => Some(format!(
            "{context}; wrapper enrichment was bypassed and conservative raw diagnostics will be preserved."
        )),
        (ExecutionMode::Passthrough, _, _) => Some(format!(
            "{context}; wrapper enrichment was bypassed and conservative raw diagnostics will be preserved."
        )),
        (ExecutionMode::Render, ProcessingPath::SingleSinkStructured, _) => Some(format!(
            "{context}; explicit structured capture is active and same-run native diagnostics may not be preserved on this backend capability profile."
        )),
        _ => None,
    }
}

pub(crate) fn operator_guidance_for_seam(seam: &CliCompatibilitySeam) -> OperatorGuidance {
    if !seam.is_in_scope() {
        return OperatorGuidance {
            summary: "operator next step=use raw gcc/g++ or --formed-mode=passthrough until an in-scope VersionBand is confirmed.",
            representative_limitations: &[
                "this compiler version is outside the current GCC 9-15 contract.",
                "conservative raw diagnostics will be preserved.",
            ],
            actionable_next_steps: &[
                "Use raw gcc/g++ for production builds until an in-scope VersionBand is confirmed.",
                "Use --formed-mode=passthrough when you need the wrapper path for triage only.",
            ],
            c_first_focus_areas: &[],
        };
    }

    if matches!(
        seam.default_processing_path,
        ProcessingPath::DualSinkStructured
    ) {
        return OperatorGuidance {
            summary: "operator next step=keep direct CC/CXX replacement, and keep at most one wrapper-owned backend launcher behind the wrapper.",
            representative_limitations: &[
                "dual_sink_structured is the default capture path on this backend capability profile.",
                "Launcher stacks in front of the wrapper are still outside the current beta contract.",
            ],
            actionable_next_steps: &[
                "Keep direct CC/CXX replacement as the default insertion shape.",
                "If you need one cache or remote-exec launcher, keep it behind the wrapper.",
            ],
            c_first_focus_areas: &["compile", "type", "macro_include", "linker"],
        };
    }

    match seam.version_band {
        VersionBand::Gcc9_12 => OperatorGuidance {
            summary: "operator next step=for C-first Make / CMake builds, set CC=gcc-formed and CXX=g++-formed; keep at most one wrapper-owned backend launcher behind the wrapper; select single_sink_structured only when you need explicit machine-readable structured capture for that run; and fall back to raw gcc/g++ or --formed-mode=passthrough if the topology is not proven.",
            representative_limitations: &[
                "native_text_capture is the default capture path on this backend capability profile.",
                "single_sink_structured remains available as an explicit structured capture path; the artifact format stays capability-specific.",
                "same-run native diagnostics may not be preserved when explicit structured capture is active.",
            ],
            actionable_next_steps: &[
                "Set CC=gcc-formed and CXX=g++-formed for direct Make / CMake insertion.",
                "Keep the backend default path unless you explicitly need machine-readable structured capture for that run.",
                "Use raw gcc/g++ or --formed-mode=passthrough if the topology is not proven.",
            ],
            c_first_focus_areas: &[
                "compile",
                "type",
                "link",
                "include_path",
                "macro",
                "preprocessor",
            ],
        },
        VersionBand::Gcc15 | VersionBand::Gcc13_14 => OperatorGuidance {
            summary: "operator next step=for C-first Make / CMake builds, set CC=gcc-formed and CXX=g++-formed; keep at most one wrapper-owned backend launcher behind the wrapper; select single_sink_structured only when you need explicit machine-readable structured capture for that run; and fall back to raw gcc/g++ or --formed-mode=passthrough if the topology is not proven.",
            representative_limitations: &[
                "native_text_capture is the default capture path on this backend capability profile.",
                "single_sink_structured remains available as an explicit structured capture path.",
                "same-run native diagnostics may not be preserved when explicit structured capture is active.",
            ],
            actionable_next_steps: &[
                "Set CC=gcc-formed and CXX=g++-formed for direct Make / CMake insertion.",
                "Keep the backend default path unless you explicitly need machine-readable structured capture for that run.",
                "Use raw gcc/g++ or --formed-mode=passthrough if the topology is not proven.",
            ],
            c_first_focus_areas: &["compile", "link", "include_path", "macro", "preprocessor"],
        },
        VersionBand::Gcc16Plus | VersionBand::Unknown => unreachable!("handled above"),
    }
}

#[cfg(test)]
pub(crate) fn operator_guidance_for_version_band(version_band: VersionBand) -> OperatorGuidance {
    let seam = CliCompatibilitySeam::from_version_band(version_band);
    operator_guidance_for_seam(&seam)
}

fn representative_major_for_band(version_band: VersionBand) -> u32 {
    match version_band {
        VersionBand::Gcc16Plus => 16,
        VersionBand::Gcc15 => 15,
        VersionBand::Gcc13_14 => 13,
        VersionBand::Gcc9_12 => 9,
        VersionBand::Unknown => 0,
    }
}

fn processing_path_label(path: ProcessingPath) -> &'static str {
    match path {
        ProcessingPath::DualSinkStructured => "dual_sink_structured",
        ProcessingPath::SingleSinkStructured => "single_sink_structured",
        ProcessingPath::NativeTextCapture => "native_text_capture",
        ProcessingPath::Passthrough => "passthrough",
    }
}

fn version_band_label(version_band: VersionBand) -> &'static str {
    match version_band {
        VersionBand::Gcc16Plus => "gcc16_plus",
        VersionBand::Gcc15 => "gcc15",
        VersionBand::Gcc13_14 => "gcc13_14",
        VersionBand::Gcc9_12 => "gcc9_12",
        VersionBand::Unknown => "unknown",
    }
}

fn support_level_label(level: SupportLevel) -> &'static str {
    match level {
        SupportLevel::InScope => "in_scope",
        SupportLevel::PassthroughOnly => "passthrough_only",
    }
}

pub(crate) fn execution_mode_label(mode: ExecutionMode) -> &'static str {
    match mode {
        ExecutionMode::Render => "render",
        ExecutionMode::Shadow => "shadow",
        ExecutionMode::Passthrough => "passthrough",
    }
}

pub(crate) fn fallback_reason_label(reason: FallbackReason) -> &'static str {
    match reason {
        FallbackReason::UnsupportedVersionBand => "unsupported_version_band",
        FallbackReason::IncompatibleSink => "incompatible_sink",
        FallbackReason::UserOptOut => "user_opt_out",
        FallbackReason::ShadowMode => "shadow_mode",
        FallbackReason::SarifMissing => "sarif_missing",
        FallbackReason::SarifParseFailed => "sarif_parse_failed",
        FallbackReason::ResidualOnly => "residual_only",
        FallbackReason::RendererLowConfidence => "renderer_low_confidence",
        FallbackReason::InternalError => "internal_error",
        FallbackReason::TimeoutOrBudget => "timeout_or_budget",
    }
}

fn notice_context(
    seam: &CliCompatibilitySeam,
    decision: &ModeDecision,
    processing_path: ProcessingPath,
) -> String {
    let mut parts = vec![
        format!("version band={}", version_band_label(seam.version_band)),
        format!("support level={}", support_level_label(seam.support_level)),
        format!("selected mode={}", execution_mode_label(decision.mode)),
        format!("processing path={}", processing_path_label(processing_path)),
    ];

    if let Some(reason) = decision.fallback_reason {
        parts.push(format!("fallback reason={}", fallback_reason_label(reason)));
    }

    format!("gcc-formed: {}", parts.join("; "))
}

pub(crate) fn should_capture_passthrough_stderr(
    retention_policy: RetentionPolicy,
    debug_refs: DebugRefs,
) -> bool {
    matches!(
        retention_policy,
        RetentionPolicy::OnChildError | RetentionPolicy::Always
    ) || matches!(debug_refs, DebugRefs::CaptureRef)
}

pub(crate) fn detect_capabilities() -> RenderCapabilities {
    let stderr = std::io::stderr();
    let is_terminal = stderr.is_terminal();
    let width = env::var("COLUMNS")
        .ok()
        .and_then(|value| value.parse().ok())
        .or(Some(100));
    RenderCapabilities {
        stream_kind: if is_ci() {
            StreamKind::CiLog
        } else if is_terminal {
            StreamKind::Tty
        } else {
            StreamKind::Pipe
        },
        width_columns: width,
        ansi_color: is_terminal,
        unicode: false,
        hyperlinks: false,
        interactive: is_terminal,
    }
}

pub(crate) fn detect_profile_from_capabilities(capabilities: &RenderCapabilities) -> RenderProfile {
    match capabilities.stream_kind {
        StreamKind::CiLog => RenderProfile::Ci,
        StreamKind::Tty if capabilities.interactive => RenderProfile::Default,
        _ => RenderProfile::Concise,
    }
}

fn ci_env_is_enabled(raw: Option<OsString>) -> bool {
    raw.is_some_and(|value| !value.is_empty())
}

pub(crate) fn is_ci() -> bool {
    ci_env_is_enabled(env::var_os("CI"))
}

pub(crate) fn language_mode_from_invocation(invoked_as: &str) -> LanguageMode {
    if invoked_as.contains("g++") || invoked_as.contains("c++") {
        LanguageMode::Cpp
    } else {
        LanguageMode::C
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selects_passthrough_with_reason_for_hard_conflict() {
        let decision = select_mode(VersionBand::Gcc15, None, true);
        assert_eq!(decision.mode, ExecutionMode::Passthrough);
        assert_eq!(
            decision.fallback_reason,
            Some(FallbackReason::IncompatibleSink)
        );
        assert!(
            decision
                .decision_log
                .iter()
                .any(|entry| entry == "hard_conflict=diagnostic_sink_override")
        );
    }

    #[test]
    fn annotates_shadow_mode_with_reason() {
        let decision = select_mode(VersionBand::Gcc13_14, Some(ExecutionMode::Shadow), false);
        assert_eq!(decision.mode, ExecutionMode::Shadow);
        assert_eq!(decision.fallback_reason, Some(FallbackReason::ShadowMode));
        assert!(
            decision
                .decision_log
                .iter()
                .any(|entry| entry == "selected_mode=shadow")
        );
    }

    #[test]
    fn selects_in_scope_render_by_default() {
        let decision = select_mode(VersionBand::Gcc13_14, None, false);
        assert_eq!(decision.mode, ExecutionMode::Render);
        assert_eq!(decision.fallback_reason, None);
        assert!(
            decision
                .decision_log
                .iter()
                .any(|entry| entry == "selected_mode=render")
        );
    }

    #[test]
    fn omits_notice_for_default_in_scope_render() {
        let decision = select_mode(VersionBand::Gcc13_14, None, false);
        assert_eq!(
            compatibility_scope_notice(VersionBand::Gcc13_14, &decision),
            None
        );
    }

    #[test]
    fn announces_in_scope_shadow_mode() {
        let decision = select_mode(VersionBand::Gcc13_14, Some(ExecutionMode::Shadow), false);
        assert_eq!(
            compatibility_scope_notice(VersionBand::Gcc13_14, &decision),
            Some(
                "gcc-formed: version band=gcc13_14; support level=in_scope; selected mode=shadow; processing path=native_text_capture; fallback reason=shadow_mode; shadow capture is active under the shared GCC 9-15 in-scope contract and emits capability-specific debug metadata without changing the public contract.".to_string()
            )
        );
    }

    #[test]
    fn selects_single_sink_structured_when_requested_for_gcc13_render() {
        let seam = CliCompatibilitySeam::from_version_band(VersionBand::Gcc13_14);
        let decision = select_mode_for_seam(&seam, None, false);
        assert_eq!(
            select_processing_path_for_seam(
                &seam,
                &decision,
                Some(ProcessingPath::SingleSinkStructured)
            )
            .unwrap(),
            ProcessingPath::SingleSinkStructured
        );
    }

    #[test]
    fn announces_single_sink_structured_tradeoff_for_gcc13_render() {
        let seam = CliCompatibilitySeam::from_version_band(VersionBand::Gcc13_14);
        let decision = select_mode_for_seam(&seam, None, false);
        assert_eq!(
            compatibility_scope_notice_for_path(
                &seam,
                &decision,
                ProcessingPath::SingleSinkStructured
            ),
            Some(
                "gcc-formed: version band=gcc13_14; support level=in_scope; selected mode=render; processing path=single_sink_structured; explicit structured capture is active and same-run native diagnostics may not be preserved on this backend capability profile.".to_string()
            )
        );
    }

    #[test]
    fn selects_gcc9_native_text_render_by_default() {
        let seam = CliCompatibilitySeam::from_version_band(VersionBand::Gcc9_12);
        let decision = select_mode_for_seam(&seam, None, false);
        assert_eq!(decision.mode, ExecutionMode::Render);
        assert_eq!(decision.fallback_reason, None);
        assert!(
            decision
                .decision_log
                .iter()
                .any(|entry| entry == "selected_mode=render")
        );
    }

    #[test]
    fn omits_notice_for_default_gcc9_render() {
        let seam = CliCompatibilitySeam::from_version_band(VersionBand::Gcc9_12);
        let decision = select_mode_for_seam(&seam, None, false);
        assert_eq!(
            compatibility_scope_notice_for_path(
                &seam,
                &decision,
                ProcessingPath::NativeTextCapture
            ),
            None
        );
    }

    #[test]
    fn announces_single_sink_structured_tradeoff_for_gcc9_render() {
        let seam = CliCompatibilitySeam::from_version_band(VersionBand::Gcc9_12);
        let decision = select_mode_for_seam(&seam, None, false);
        assert_eq!(
            compatibility_scope_notice_for_path(
                &seam,
                &decision,
                ProcessingPath::SingleSinkStructured
            ),
            Some(
                "gcc-formed: version band=gcc9_12; support level=in_scope; selected mode=render; processing path=single_sink_structured; explicit structured capture is active and same-run native diagnostics may not be preserved on this backend capability profile.".to_string()
            )
        );
    }

    #[test]
    fn older_in_scope_operator_guidance_is_path_first() {
        let gcc13 = operator_guidance_for_version_band(VersionBand::Gcc13_14);
        let gcc9 = operator_guidance_for_version_band(VersionBand::Gcc9_12);

        assert_eq!(gcc13.summary, gcc9.summary);
        assert!(gcc13.summary.contains("select single_sink_structured only when you need explicit machine-readable structured capture for that run"));
        assert!(!gcc13.summary.contains("ordinary runs"));
        assert!(
            gcc13
                .actionable_next_steps
                .iter()
                .any(|step| step.contains("Keep the backend default path unless you explicitly need machine-readable structured capture"))
        );
        assert!(
            gcc9.representative_limitations
                .iter()
                .any(|limit| limit.contains("artifact format stays capability-specific"))
        );
    }

    #[test]
    fn announces_out_of_scope_unknown_passthrough() {
        let decision = select_mode(VersionBand::Unknown, None, false);
        assert_eq!(
            compatibility_scope_notice(VersionBand::Unknown, &decision),
            Some(
                "gcc-formed: version band=unknown; support level=passthrough_only; selected mode=passthrough; processing path=passthrough; fallback reason=unsupported_version_band; this compiler version is outside the current GCC 9-15 contract and conservative raw diagnostics will be preserved; operator next step=use raw gcc/g++ or --formed-mode=passthrough until an in-scope VersionBand is confirmed.".to_string()
            )
        );
    }

    #[test]
    fn probe_vocabulary_seam_rejects_dual_sink_when_gcc15_dual_sink_is_unavailable() {
        let seam = CliCompatibilitySeam::from_probe(&ProbeResult {
            requested_backend: "gcc-formed".to_string(),
            resolved_path: "/tmp/fake-gcc".into(),
            execution_topology: diag_backend_probe::ActiveBackendTopology {
                policy_version: diag_backend_probe::BACKEND_TOPOLOGY_POLICY_VERSION.to_string(),
                kind: diag_backend_probe::BackendTopologyKind::Direct,
                launcher_path: None,
                disposition: diag_backend_probe::BackendTopologyDisposition::Supported,
            },
            version_string: "gcc (Fake) 15.2.0".to_string(),
            major: 15,
            minor: 2,
            driver_kind: diag_backend_probe::DriverKind::Gcc,
            add_output_sarif_supported: false,
            version_probe_key: diag_backend_probe::ProbeKey {
                realpath: "/tmp/fake-gcc".into(),
                inode: 2,
                mtime_seconds: 1,
                size_bytes: 1,
            },
        });

        let decision = select_mode_for_seam(&seam, None, false);
        assert_eq!(decision.mode, ExecutionMode::Render);
        assert_eq!(
            select_processing_path_for_seam(&seam, &decision, None).unwrap(),
            ProcessingPath::NativeTextCapture
        );
        assert_eq!(
            select_processing_path_for_seam(
                &seam,
                &decision,
                Some(ProcessingPath::SingleSinkStructured)
            )
            .unwrap(),
            ProcessingPath::SingleSinkStructured
        );
        let error = select_processing_path_for_seam(
            &seam,
            &decision,
            Some(ProcessingPath::DualSinkStructured),
        )
        .unwrap_err();
        assert!(
            error.contains("requested processing path `dual_sink_structured` is not supported")
        );
    }

    #[test]
    fn operator_guidance_uses_probe_capabilities_when_gcc15_dual_sink_is_unavailable() {
        let seam = CliCompatibilitySeam::from_probe(&ProbeResult {
            requested_backend: "gcc-formed".to_string(),
            resolved_path: "/tmp/fake-gcc".into(),
            execution_topology: diag_backend_probe::ActiveBackendTopology {
                policy_version: diag_backend_probe::BACKEND_TOPOLOGY_POLICY_VERSION.to_string(),
                kind: diag_backend_probe::BackendTopologyKind::Direct,
                launcher_path: None,
                disposition: diag_backend_probe::BackendTopologyDisposition::Supported,
            },
            version_string: "gcc (Fake) 15.2.0".to_string(),
            major: 15,
            minor: 2,
            driver_kind: diag_backend_probe::DriverKind::Gcc,
            add_output_sarif_supported: false,
            version_probe_key: diag_backend_probe::ProbeKey {
                realpath: "/tmp/fake-gcc".into(),
                inode: 2,
                mtime_seconds: 1,
                size_bytes: 1,
            },
        });

        let guidance = operator_guidance_for_seam(&seam);

        assert_eq!(
            guidance.summary,
            "operator next step=for C-first Make / CMake builds, set CC=gcc-formed and CXX=g++-formed; keep at most one wrapper-owned backend launcher behind the wrapper; select single_sink_structured only when you need explicit machine-readable structured capture for that run; and fall back to raw gcc/g++ or --formed-mode=passthrough if the topology is not proven."
        );
        assert_eq!(
            guidance.representative_limitations[0],
            "native_text_capture is the default capture path on this backend capability profile."
        );
        assert_eq!(
            guidance.actionable_next_steps[1],
            "Keep the backend default path unless you explicitly need machine-readable structured capture for that run."
        );
    }

    #[test]
    fn captures_passthrough_stderr_only_when_requested() {
        assert!(should_capture_passthrough_stderr(
            RetentionPolicy::Always,
            DebugRefs::None
        ));
        assert!(should_capture_passthrough_stderr(
            RetentionPolicy::OnChildError,
            DebugRefs::None
        ));
        assert!(should_capture_passthrough_stderr(
            RetentionPolicy::Never,
            DebugRefs::CaptureRef
        ));
        assert!(!should_capture_passthrough_stderr(
            RetentionPolicy::Never,
            DebugRefs::None
        ));
        assert!(!should_capture_passthrough_stderr(
            RetentionPolicy::OnWrapperFailure,
            DebugRefs::TraceId
        ));
    }

    #[test]
    fn ci_profile_follows_capabilities() {
        let capabilities = RenderCapabilities {
            stream_kind: StreamKind::CiLog,
            width_columns: Some(120),
            ansi_color: false,
            unicode: false,
            hyperlinks: false,
            interactive: false,
        };
        assert_eq!(
            detect_profile_from_capabilities(&capabilities),
            RenderProfile::Ci
        );
    }

    #[test]
    fn empty_ci_env_does_not_enable_ci_mode() {
        assert!(ci_env_is_enabled(Some(OsString::from("1"))));
        assert!(!ci_env_is_enabled(Some(OsString::new())));
        assert!(!ci_env_is_enabled(None));
    }

    #[test]
    fn compiler_introspection_is_limited_to_explicit_dump_allowlist() {
        assert!(is_compiler_introspection(&[OsString::from("-dumpmachine")]));
        assert!(is_compiler_introspection(&[OsString::from("-dumpspecs")]));
        assert!(!is_compiler_introspection(&[
            OsString::from("-c"),
            OsString::from("main.c"),
            OsString::from("-dumpdir"),
            OsString::from("tmp/"),
            OsString::from("-dumpbase"),
            OsString::from("main.c"),
            OsString::from("-dumpbase-ext"),
            OsString::from(".c"),
        ]));
    }

    #[test]
    fn probe_vocabulary_seam_preserves_in_scope_structured_render() {
        let seam = CliCompatibilitySeam::from_probe(&ProbeResult {
            requested_backend: "gcc-formed".to_string(),
            resolved_path: "/tmp/fake-gcc".into(),
            execution_topology: diag_backend_probe::ActiveBackendTopology {
                policy_version: diag_backend_probe::BACKEND_TOPOLOGY_POLICY_VERSION.to_string(),
                kind: diag_backend_probe::BackendTopologyKind::Direct,
                launcher_path: None,
                disposition: diag_backend_probe::BackendTopologyDisposition::Supported,
            },
            version_string: "gcc (Fake) 15.2.0".to_string(),
            major: 15,
            minor: 2,
            driver_kind: diag_backend_probe::DriverKind::Gcc,
            add_output_sarif_supported: true,
            version_probe_key: diag_backend_probe::ProbeKey {
                realpath: "/tmp/fake-gcc".into(),
                inode: 1,
                mtime_seconds: 1,
                size_bytes: 1,
            },
        });

        let decision = select_mode_for_seam(&seam, None, false);
        assert_eq!(decision.mode, ExecutionMode::Render);
        assert_eq!(decision.fallback_reason, None);
        assert!(seam.should_inject_sarif(decision.mode, ProcessingPath::DualSinkStructured));
        assert!(seam.should_preserve_tty_color(
            decision.mode,
            ProcessingPath::DualSinkStructured,
            &RenderCapabilities {
                stream_kind: StreamKind::Tty,
                width_columns: Some(100),
                ansi_color: true,
                unicode: false,
                hyperlinks: false,
                interactive: true,
            },
            &[]
        ));
    }

    #[test]
    fn probe_vocabulary_seam_preserves_gcc13_shadow_escape_hatch() {
        let seam = CliCompatibilitySeam::from_probe(&ProbeResult {
            requested_backend: "gcc-formed".to_string(),
            resolved_path: "/tmp/fake-gcc".into(),
            execution_topology: diag_backend_probe::ActiveBackendTopology {
                policy_version: diag_backend_probe::BACKEND_TOPOLOGY_POLICY_VERSION.to_string(),
                kind: diag_backend_probe::BackendTopologyKind::Direct,
                launcher_path: None,
                disposition: diag_backend_probe::BackendTopologyDisposition::Supported,
            },
            version_string: "gcc (Fake) 13.3.0".to_string(),
            major: 13,
            minor: 3,
            driver_kind: diag_backend_probe::DriverKind::Gcc,
            add_output_sarif_supported: false,
            version_probe_key: diag_backend_probe::ProbeKey {
                realpath: "/tmp/fake-gcc".into(),
                inode: 1,
                mtime_seconds: 1,
                size_bytes: 1,
            },
        });

        let decision = select_mode_for_seam(&seam, Some(ExecutionMode::Shadow), false);
        assert_eq!(decision.mode, ExecutionMode::Shadow);
        assert_eq!(decision.fallback_reason, Some(FallbackReason::ShadowMode));
        assert!(!seam.should_inject_sarif(decision.mode, ProcessingPath::NativeTextCapture));
    }

    #[test]
    fn tty_color_preservation_requires_tty_and_no_user_override() {
        let seam = CliCompatibilitySeam::from_version_band(VersionBand::Gcc13_14);
        let tty_capabilities = RenderCapabilities {
            stream_kind: StreamKind::Tty,
            width_columns: Some(100),
            ansi_color: true,
            unicode: false,
            hyperlinks: false,
            interactive: true,
        };

        assert!(seam.should_preserve_tty_color(
            ExecutionMode::Render,
            ProcessingPath::NativeTextCapture,
            &tty_capabilities,
            &[]
        ));
        assert!(!seam.should_preserve_tty_color(
            ExecutionMode::Render,
            ProcessingPath::NativeTextCapture,
            &RenderCapabilities {
                stream_kind: StreamKind::Pipe,
                ..tty_capabilities
            },
            &[]
        ));
        assert!(!seam.should_preserve_tty_color(
            ExecutionMode::Render,
            ProcessingPath::NativeTextCapture,
            &tty_capabilities,
            &[OsString::from("-fdiagnostics-color=never")]
        ));
    }

    #[test]
    fn tty_color_preservation_applies_to_native_text_and_single_sink_paths() {
        let seam = CliCompatibilitySeam::from_version_band(VersionBand::Gcc9_12);
        let tty_capabilities = RenderCapabilities {
            stream_kind: StreamKind::Tty,
            width_columns: Some(100),
            ansi_color: true,
            unicode: false,
            hyperlinks: false,
            interactive: true,
        };

        assert!(seam.should_preserve_tty_color(
            ExecutionMode::Render,
            ProcessingPath::NativeTextCapture,
            &tty_capabilities,
            &[]
        ));
        assert!(seam.should_preserve_tty_color(
            ExecutionMode::Render,
            ProcessingPath::SingleSinkStructured,
            &tty_capabilities,
            &[]
        ));
        assert!(!seam.should_preserve_tty_color(
            ExecutionMode::Passthrough,
            ProcessingPath::Passthrough,
            &tty_capabilities,
            &[]
        ));
    }
}
