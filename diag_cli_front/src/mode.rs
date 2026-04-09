use crate::args::os_to_string;
use diag_backend_probe::SupportTier;
use diag_capture_runtime::ExecutionMode;
use diag_core::FallbackReason;
use diag_render::DebugRefs;
use diag_trace::RetentionPolicy;
use std::ffi::OsString;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ModeDecision {
    pub(crate) mode: ExecutionMode,
    pub(crate) fallback_reason: Option<FallbackReason>,
    pub(crate) decision_log: Vec<String>,
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

pub(crate) fn select_mode(
    tier: SupportTier,
    requested: Option<ExecutionMode>,
    hard_conflict: bool,
) -> ModeDecision {
    let mut decision_log = vec![format!("support_tier={:?}", tier).to_lowercase()];
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
    let mode = match tier {
        SupportTier::A => {
            decision_log.push(format!(
                "tier_a_mode={}",
                format!("{:?}", requested.unwrap_or(ExecutionMode::Render)).to_lowercase()
            ));
            requested.unwrap_or(ExecutionMode::Render)
        }
        SupportTier::B => match requested {
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
        SupportTier::C => {
            decision_log.push("tier_c_mode=passthrough_only".to_string());
            ExecutionMode::Passthrough
        }
    };

    let fallback_reason = match mode {
        ExecutionMode::Passthrough => match tier {
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

pub(crate) fn compatibility_scope_notice(
    tier: SupportTier,
    decision: &ModeDecision,
) -> Option<&'static str> {
    match tier {
        SupportTier::A => None,
        SupportTier::B => match decision.mode {
            ExecutionMode::Shadow => Some(
                "gcc-formed: GCC 13/14 is running in compatibility mode; only conservative shadow capture is supported and enhanced render output is not guaranteed.",
            ),
            ExecutionMode::Passthrough => Some(
                "gcc-formed: GCC 13/14 is running in compatibility mode; enhanced render output is not guaranteed and conservative raw diagnostics will be preserved.",
            ),
            ExecutionMode::Render => None,
        },
        SupportTier::C => Some(
            "gcc-formed: this compiler version is outside the first-release support scope; conservative passthrough output will be used.",
        ),
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
                "gcc-formed: GCC 13/14 is running in compatibility mode; enhanced render output is not guaranteed and conservative raw diagnostics will be preserved."
            )
        );
    }

    #[test]
    fn announces_out_of_scope_tier_c_passthrough() {
        let decision = select_mode(SupportTier::C, None, false);
        assert_eq!(
            compatibility_scope_notice(SupportTier::C, &decision),
            Some(
                "gcc-formed: this compiler version is outside the first-release support scope; conservative passthrough output will be used."
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
}
