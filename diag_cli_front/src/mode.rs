use crate::args::os_to_string;
use diag_backend_probe::{
    CapabilityProfile, ProbeResult, ProcessingPath, SupportLevel, SupportTier,
    default_processing_path_for_tier, support_level_for_tier,
};
use diag_capture_runtime::ExecutionMode;
use diag_core::{FallbackReason, LanguageMode};
use diag_render::{DebugRefs, RenderCapabilities, RenderProfile, StreamKind};
use diag_trace::RetentionPolicy;
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
    support_tier: SupportTier,
    support_level: SupportLevel,
    default_processing_path: ProcessingPath,
    sarif_diagnostics: bool,
}

impl CliCompatibilitySeam {
    pub(crate) fn from_probe(backend: &ProbeResult) -> Self {
        Self::from_profile(backend.support_tier, backend.capability_profile())
    }

    pub(crate) fn from_support_tier(tier: SupportTier) -> Self {
        Self {
            support_tier: tier,
            support_level: support_level_for_tier(tier),
            default_processing_path: default_processing_path_for_tier(tier),
            sarif_diagnostics: matches!(
                default_processing_path_for_tier(tier),
                ProcessingPath::DualSinkStructured
            ),
        }
    }

    fn from_profile(support_tier: SupportTier, profile: CapabilityProfile) -> Self {
        Self {
            support_tier,
            support_level: profile.support_level,
            default_processing_path: profile.default_processing_path,
            sarif_diagnostics: profile.sarif_diagnostics,
        }
    }

    fn is_primary_structured(&self) -> bool {
        matches!(self.support_level, SupportLevel::Primary)
            && matches!(
                self.default_processing_path,
                ProcessingPath::DualSinkStructured
            )
    }

    fn is_shadow_compatibility(&self) -> bool {
        matches!(self.support_tier, SupportTier::B)
            && matches!(self.support_level, SupportLevel::Conservative)
            && matches!(self.default_processing_path, ProcessingPath::Passthrough)
    }

    pub(crate) fn should_inject_sarif(&self, mode: ExecutionMode) -> bool {
        mode != ExecutionMode::Passthrough && self.sarif_diagnostics && self.is_primary_structured()
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
        _ if seam.is_shadow_compatibility() => match requested {
            Some(ExecutionMode::Shadow) => {
                decision_log.push("tier_b_mode=shadow_raw_only".to_string());
                ExecutionMode::Shadow
            }
            Some(ExecutionMode::Render) => {
                decision_log.push("tier_b_render_unsupported=passthrough".to_string());
                ExecutionMode::Passthrough
            }
            None => {
                decision_log.push("tier_b_default=passthrough".to_string());
                ExecutionMode::Passthrough
            }
            Some(ExecutionMode::Passthrough) => ExecutionMode::Passthrough,
        },
        _ => {
            decision_log.push("tier_c_mode=passthrough_only".to_string());
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

#[cfg(test)]
pub(crate) fn compatibility_scope_notice(
    tier: SupportTier,
    decision: &ModeDecision,
) -> Option<&'static str> {
    let seam = CliCompatibilitySeam::from_support_tier(tier);
    compatibility_scope_notice_for_seam(&seam, decision)
}

pub(crate) fn compatibility_scope_notice_for_seam(
    seam: &CliCompatibilitySeam,
    decision: &ModeDecision,
) -> Option<&'static str> {
    match seam.support_tier {
        SupportTier::A => None,
        SupportTier::B => match decision.mode {
            ExecutionMode::Shadow => Some(
                "gcc-formed: support tier=b compatibility-only path (GCC 13/14); selected mode=shadow; fallback reason=shadow_mode; conservative shadow capture is enabled and enhanced render output is not guaranteed.",
            ),
            ExecutionMode::Passthrough => Some(
                "gcc-formed: support tier=b compatibility-only path (GCC 13/14); selected mode=passthrough; fallback reason=unsupported_tier; enhanced render output is not guaranteed and conservative raw diagnostics will be preserved.",
            ),
            ExecutionMode::Render => None,
        },
        SupportTier::C => Some(
            "gcc-formed: support tier=c out-of-scope compatibility path; selected mode=passthrough; fallback reason=unsupported_tier; this compiler version is outside the first-release support scope and conservative raw diagnostics will be preserved.",
        ),
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
                .any(|entry| entry == "tier_b_mode=shadow_raw_only")
        );
    }

    #[test]
    fn announces_tier_b_compatibility_passthrough() {
        let decision = select_mode(SupportTier::B, None, false);
        assert_eq!(
            compatibility_scope_notice(SupportTier::B, &decision),
            Some(
                "gcc-formed: support tier=b compatibility-only path (GCC 13/14); selected mode=passthrough; fallback reason=unsupported_tier; enhanced render output is not guaranteed and conservative raw diagnostics will be preserved."
            )
        );
    }

    #[test]
    fn announces_tier_b_compatibility_shadow() {
        let decision = select_mode(SupportTier::B, Some(ExecutionMode::Shadow), false);
        assert_eq!(
            compatibility_scope_notice(SupportTier::B, &decision),
            Some(
                "gcc-formed: support tier=b compatibility-only path (GCC 13/14); selected mode=shadow; fallback reason=shadow_mode; conservative shadow capture is enabled and enhanced render output is not guaranteed."
            )
        );
    }

    #[test]
    fn announces_out_of_scope_tier_c_passthrough() {
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
}
