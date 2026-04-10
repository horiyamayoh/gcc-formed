use crate::args::os_to_string;
use diag_backend_probe::{
    CapabilityProfile, ProbeResult, ProcessingPath, SupportLevel, SupportTier, VersionBand,
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
    support_tier: SupportTier,
    support_level: SupportLevel,
    default_processing_path: ProcessingPath,
    allowed_processing_paths: BTreeSet<ProcessingPath>,
    sarif_diagnostics: bool,
    tty_color_control: bool,
}

impl CliCompatibilitySeam {
    pub(crate) fn from_probe(backend: &ProbeResult) -> Self {
        Self::from_profile(backend.support_tier, backend.capability_profile())
    }

    #[cfg(test)]
    pub(crate) fn from_support_tier(tier: SupportTier) -> Self {
        let version_band = match tier {
            SupportTier::A => VersionBand::Gcc15Plus,
            SupportTier::B => VersionBand::Gcc13_14,
            SupportTier::C => VersionBand::Unknown,
        };
        Self::from_version_band(version_band)
    }

    pub(crate) fn from_version_band(version_band: VersionBand) -> Self {
        let representative_major = representative_major_for_band(version_band);
        let support_tier = match version_band {
            VersionBand::Gcc15Plus => SupportTier::A,
            VersionBand::Gcc13_14 => SupportTier::B,
            VersionBand::Gcc9_12 | VersionBand::Unknown => SupportTier::C,
        };
        Self::from_profile(
            support_tier,
            capability_profile_for_major(representative_major),
        )
    }

    fn from_profile(support_tier: SupportTier, profile: CapabilityProfile) -> Self {
        Self {
            version_band: profile.version_band,
            support_tier,
            support_level: profile.support_level,
            default_processing_path: profile.default_processing_path,
            allowed_processing_paths: profile.allowed_processing_paths,
            sarif_diagnostics: profile.sarif_diagnostics,
            tty_color_control: profile.tty_color_control,
        }
    }

    fn is_primary_structured(&self) -> bool {
        matches!(self.support_level, SupportLevel::Primary)
            && matches!(
                self.default_processing_path,
                ProcessingPath::DualSinkStructured
            )
    }

    fn is_native_text_default(&self) -> bool {
        matches!(
            self.support_level,
            SupportLevel::Conservative | SupportLevel::Experimental
        ) && matches!(
            self.default_processing_path,
            ProcessingPath::NativeTextCapture
        )
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

    pub(crate) fn should_inject_sarif(&self, mode: ExecutionMode) -> bool {
        mode != ExecutionMode::Passthrough && self.sarif_diagnostics && self.is_primary_structured()
    }

    pub(crate) fn prefers_json_single_sink(&self) -> bool {
        matches!(self.version_band, VersionBand::Gcc9_12)
    }

    pub(crate) fn should_preserve_tty_color(
        &self,
        mode: ExecutionMode,
        capabilities: &RenderCapabilities,
        forwarded_args: &[OsString],
    ) -> bool {
        mode == ExecutionMode::Render
            && self.is_primary_structured()
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
            "--help" | "--version" | "-dumpmachine" | "-dumpversion" | "-dumpfullversion" | "-###"
        ) || value.starts_with("-dump")
            || value.starts_with("-print-")
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
    tier: SupportTier,
    requested: Option<ExecutionMode>,
    hard_conflict: bool,
) -> ModeDecision {
    let seam = CliCompatibilitySeam::from_support_tier(tier);
    select_mode_for_seam(&seam, requested, hard_conflict)
}

pub(crate) fn select_mode_for_seam(
    seam: &CliCompatibilitySeam,
    requested: Option<ExecutionMode>,
    hard_conflict: bool,
) -> ModeDecision {
    let mut decision_log = vec![format!("support_tier={:?}", seam.support_tier).to_lowercase()];
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
    let mode = match seam.support_tier {
        _ if seam.is_primary_structured() => {
            decision_log.push(format!(
                "tier_a_mode={}",
                format!("{:?}", requested.unwrap_or(ExecutionMode::Render)).to_lowercase()
            ));
            requested.unwrap_or(ExecutionMode::Render)
        }
        _ if seam.is_native_text_default() => match requested {
            Some(ExecutionMode::Shadow) => {
                decision_log.push(format!(
                    "{}_mode=shadow_native_text",
                    native_text_log_prefix(seam.version_band)
                ));
                ExecutionMode::Shadow
            }
            Some(ExecutionMode::Render) => {
                decision_log.push(format!(
                    "{}_requested_render=native_text",
                    native_text_log_prefix(seam.version_band)
                ));
                ExecutionMode::Render
            }
            None => {
                decision_log.push(format!(
                    "{}_default=native_text",
                    native_text_log_prefix(seam.version_band)
                ));
                ExecutionMode::Render
            }
            Some(ExecutionMode::Passthrough) => ExecutionMode::Passthrough,
        },
        _ => {
            decision_log.push(format!(
                "tier_{}_mode=passthrough_only",
                support_tier_label(seam.support_tier)
            ));
            ExecutionMode::Passthrough
        }
    };

    let fallback_reason = match mode {
        ExecutionMode::Passthrough => match seam.support_tier {
            SupportTier::A => None,
            SupportTier::B | SupportTier::C => Some(FallbackReason::UnsupportedTier),
        },
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
) -> ProcessingPath {
    seam.select_processing_path(decision.mode, requested)
        .unwrap_or_else(|_| match decision.mode {
            ExecutionMode::Passthrough => ProcessingPath::Passthrough,
            ExecutionMode::Shadow => {
                if seam.is_primary_structured() {
                    ProcessingPath::DualSinkStructured
                } else {
                    ProcessingPath::NativeTextCapture
                }
            }
            ExecutionMode::Render if seam.is_primary_structured() => {
                ProcessingPath::DualSinkStructured
            }
            ExecutionMode::Render => seam.default_processing_path,
        })
}

#[cfg(test)]
pub(crate) fn compatibility_scope_notice(
    tier: SupportTier,
    decision: &ModeDecision,
) -> Option<&'static str> {
    let seam = CliCompatibilitySeam::from_support_tier(tier);
    compatibility_scope_notice_for_path(
        &seam,
        decision,
        select_processing_path_for_seam(&seam, decision, None),
    )
}

pub(crate) fn compatibility_scope_notice_for_path(
    seam: &CliCompatibilitySeam,
    decision: &ModeDecision,
    processing_path: ProcessingPath,
) -> Option<&'static str> {
    match seam.version_band {
        VersionBand::Gcc15Plus => None,
        VersionBand::Gcc13_14 => match (decision.mode, processing_path, decision.fallback_reason) {
            (ExecutionMode::Shadow, _, _) => Some(
                "gcc-formed: support tier=b native-text default path (GCC 13/14); selected mode=shadow; fallback reason=shadow_mode; conservative native-text shadow capture is enabled and explicit single-sink structured selection remains opt-in.",
            ),
            (ExecutionMode::Passthrough, _, Some(FallbackReason::UserOptOut)) => Some(
                "gcc-formed: support tier=b native-text default path (GCC 13/14); selected mode=passthrough; fallback reason=user_opt_out; native-text render was bypassed and conservative raw diagnostics will be preserved.",
            ),
            (ExecutionMode::Passthrough, _, _) => Some(
                "gcc-formed: support tier=b native-text default path (GCC 13/14); selected mode=passthrough; fallback reason=incompatible_sink; enhanced capture was bypassed and conservative raw diagnostics will be preserved.",
            ),
            (ExecutionMode::Render, ProcessingPath::SingleSinkStructured, _) => Some(
                "gcc-formed: support tier=b native-text default path (GCC 13/14); selected mode=render; processing path=single_sink_structured; explicit structured capture is active and raw native diagnostics may not be preserved in the same run.",
            ),
            (ExecutionMode::Render, _, _) => Some(
                "gcc-formed: support tier=b native-text default path (GCC 13/14); selected mode=render; fallback reason=none; native-text capture is the default and explicit single-sink structured selection remains opt-in.",
            ),
        },
        VersionBand::Gcc9_12 => match (decision.mode, processing_path, decision.fallback_reason) {
            (ExecutionMode::Shadow, _, _) => Some(
                "gcc-formed: support tier=c experimental native-text default path (GCC 9-12); selected mode=shadow; fallback reason=shadow_mode; conservative native-text shadow capture is enabled and explicit single-sink structured JSON selection remains opt-in.",
            ),
            (ExecutionMode::Passthrough, _, Some(FallbackReason::UserOptOut)) => Some(
                "gcc-formed: support tier=c experimental native-text default path (GCC 9-12); selected mode=passthrough; fallback reason=user_opt_out; native-text render was bypassed and conservative raw diagnostics will be preserved.",
            ),
            (ExecutionMode::Passthrough, _, _) => Some(
                "gcc-formed: support tier=c experimental native-text default path (GCC 9-12); selected mode=passthrough; fallback reason=incompatible_sink; enhanced capture was bypassed and conservative raw diagnostics will be preserved.",
            ),
            (ExecutionMode::Render, ProcessingPath::SingleSinkStructured, _) => Some(
                "gcc-formed: support tier=c experimental native-text default path (GCC 9-12); selected mode=render; processing path=single_sink_structured; explicit structured JSON capture is active and raw native diagnostics may not be preserved in the same run.",
            ),
            (ExecutionMode::Render, _, _) => Some(
                "gcc-formed: support tier=c experimental native-text default path (GCC 9-12); selected mode=render; fallback reason=none; native-text capture is the default and explicit single-sink structured JSON selection remains opt-in.",
            ),
        },
        VersionBand::Unknown => Some(
            "gcc-formed: support tier=c out-of-scope compatibility path; selected mode=passthrough; fallback reason=unsupported_tier; this compiler version is outside the first-release support scope and conservative raw diagnostics will be preserved.",
        ),
    }
}

fn representative_major_for_band(version_band: VersionBand) -> u32 {
    match version_band {
        VersionBand::Gcc15Plus => 15,
        VersionBand::Gcc13_14 => 13,
        VersionBand::Gcc9_12 => 9,
        VersionBand::Unknown => 0,
    }
}

fn native_text_log_prefix(version_band: VersionBand) -> &'static str {
    match version_band {
        VersionBand::Gcc13_14 => "tier_b",
        VersionBand::Gcc9_12 => "tier_c",
        VersionBand::Gcc15Plus | VersionBand::Unknown => "compat",
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

pub(crate) fn support_tier_label(tier: SupportTier) -> &'static str {
    match tier {
        SupportTier::A => "a",
        SupportTier::B => "b",
        SupportTier::C => "c",
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
        FallbackReason::UnsupportedTier => "unsupported_tier",
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

pub(crate) fn is_ci() -> bool {
    env::var_os("CI").is_some()
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
        let decision = select_mode(SupportTier::A, None, true);
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
        let decision = select_mode(SupportTier::B, Some(ExecutionMode::Shadow), false);
        assert_eq!(decision.mode, ExecutionMode::Shadow);
        assert_eq!(decision.fallback_reason, Some(FallbackReason::ShadowMode));
        assert!(
            decision
                .decision_log
                .iter()
                .any(|entry| entry == "tier_b_mode=shadow_native_text")
        );
    }

    #[test]
    fn selects_tier_b_native_text_render_by_default() {
        let decision = select_mode(SupportTier::B, None, false);
        assert_eq!(decision.mode, ExecutionMode::Render);
        assert_eq!(decision.fallback_reason, None);
        assert!(
            decision
                .decision_log
                .iter()
                .any(|entry| entry == "tier_b_default=native_text")
        );
    }

    #[test]
    fn announces_tier_b_native_text_default_render() {
        let decision = select_mode(SupportTier::B, None, false);
        assert_eq!(
            compatibility_scope_notice(SupportTier::B, &decision),
            Some(
                "gcc-formed: support tier=b native-text default path (GCC 13/14); selected mode=render; fallback reason=none; native-text capture is the default and explicit single-sink structured selection remains opt-in."
            )
        );
    }

    #[test]
    fn announces_tier_b_compatibility_shadow() {
        let decision = select_mode(SupportTier::B, Some(ExecutionMode::Shadow), false);
        assert_eq!(
            compatibility_scope_notice(SupportTier::B, &decision),
            Some(
                "gcc-formed: support tier=b native-text default path (GCC 13/14); selected mode=shadow; fallback reason=shadow_mode; conservative native-text shadow capture is enabled and explicit single-sink structured selection remains opt-in."
            )
        );
    }

    #[test]
    fn selects_single_sink_structured_when_requested_for_tier_b_render() {
        let seam = CliCompatibilitySeam::from_support_tier(SupportTier::B);
        let decision = select_mode_for_seam(&seam, None, false);
        assert_eq!(
            select_processing_path_for_seam(
                &seam,
                &decision,
                Some(ProcessingPath::SingleSinkStructured)
            ),
            ProcessingPath::SingleSinkStructured
        );
    }

    #[test]
    fn announces_single_sink_structured_tradeoff_for_tier_b() {
        let seam = CliCompatibilitySeam::from_support_tier(SupportTier::B);
        let decision = select_mode_for_seam(&seam, None, false);
        assert_eq!(
            compatibility_scope_notice_for_path(
                &seam,
                &decision,
                ProcessingPath::SingleSinkStructured
            ),
            Some(
                "gcc-formed: support tier=b native-text default path (GCC 13/14); selected mode=render; processing path=single_sink_structured; explicit structured capture is active and raw native diagnostics may not be preserved in the same run."
            )
        );
    }

    #[test]
    fn selects_band_c_native_text_render_by_default() {
        let seam = CliCompatibilitySeam::from_version_band(VersionBand::Gcc9_12);
        let decision = select_mode_for_seam(&seam, None, false);
        assert_eq!(decision.mode, ExecutionMode::Render);
        assert_eq!(decision.fallback_reason, None);
        assert!(
            decision
                .decision_log
                .iter()
                .any(|entry| entry == "tier_c_default=native_text")
        );
    }

    #[test]
    fn announces_band_c_native_text_default_render() {
        let seam = CliCompatibilitySeam::from_version_band(VersionBand::Gcc9_12);
        let decision = select_mode_for_seam(&seam, None, false);
        assert_eq!(
            compatibility_scope_notice_for_path(
                &seam,
                &decision,
                ProcessingPath::NativeTextCapture
            ),
            Some(
                "gcc-formed: support tier=c experimental native-text default path (GCC 9-12); selected mode=render; fallback reason=none; native-text capture is the default and explicit single-sink structured JSON selection remains opt-in."
            )
        );
    }

    #[test]
    fn announces_single_sink_structured_tradeoff_for_band_c() {
        let seam = CliCompatibilitySeam::from_version_band(VersionBand::Gcc9_12);
        let decision = select_mode_for_seam(&seam, None, false);
        assert_eq!(
            compatibility_scope_notice_for_path(
                &seam,
                &decision,
                ProcessingPath::SingleSinkStructured
            ),
            Some(
                "gcc-formed: support tier=c experimental native-text default path (GCC 9-12); selected mode=render; processing path=single_sink_structured; explicit structured JSON capture is active and raw native diagnostics may not be preserved in the same run."
            )
        );
    }

    #[test]
    fn announces_out_of_scope_unknown_passthrough() {
        let decision = select_mode(SupportTier::C, None, false);
        assert_eq!(
            compatibility_scope_notice(SupportTier::C, &decision),
            Some(
                "gcc-formed: support tier=c out-of-scope compatibility path; selected mode=passthrough; fallback reason=unsupported_tier; this compiler version is outside the first-release support scope and conservative raw diagnostics will be preserved."
            )
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
    fn probe_vocabulary_seam_preserves_primary_structured_render() {
        let seam = CliCompatibilitySeam::from_probe(&ProbeResult {
            requested_backend: "gcc-formed".to_string(),
            resolved_path: "/tmp/fake-gcc".into(),
            version_string: "gcc (Fake) 15.2.0".to_string(),
            major: 15,
            minor: 2,
            support_tier: SupportTier::A,
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
        assert!(seam.should_inject_sarif(decision.mode));
        assert!(seam.should_preserve_tty_color(
            decision.mode,
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
    fn probe_vocabulary_seam_preserves_tier_b_shadow_escape_hatch() {
        let seam = CliCompatibilitySeam::from_probe(&ProbeResult {
            requested_backend: "gcc-formed".to_string(),
            resolved_path: "/tmp/fake-gcc".into(),
            version_string: "gcc (Fake) 13.3.0".to_string(),
            major: 13,
            minor: 3,
            support_tier: SupportTier::B,
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
        assert!(!seam.should_inject_sarif(decision.mode));
    }

    #[test]
    fn tty_color_preservation_requires_tty_and_no_user_override() {
        let seam = CliCompatibilitySeam::from_support_tier(SupportTier::A);
        let tty_capabilities = RenderCapabilities {
            stream_kind: StreamKind::Tty,
            width_columns: Some(100),
            ansi_color: true,
            unicode: false,
            hyperlinks: false,
            interactive: true,
        };

        assert!(seam.should_preserve_tty_color(ExecutionMode::Render, &tty_capabilities, &[]));
        assert!(!seam.should_preserve_tty_color(
            ExecutionMode::Render,
            &RenderCapabilities {
                stream_kind: StreamKind::Pipe,
                ..tty_capabilities
            },
            &[]
        ));
        assert!(!seam.should_preserve_tty_color(
            ExecutionMode::Render,
            &tty_capabilities,
            &[OsString::from("-fdiagnostics-color=never")]
        ));
    }
}
