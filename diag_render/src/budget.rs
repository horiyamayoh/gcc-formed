use crate::RenderProfile;

/// How warnings are handled when a fatal or error diagnostic is present.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WarningFailureMode {
    /// Show warnings as a summary count line.
    Summarize,
    /// Hide warnings entirely.
    Suppress,
    /// Show warnings in full alongside errors.
    Show,
}

/// Numeric limits that control how much detail the renderer emits.
#[derive(Debug, Clone, Copy)]
pub struct DisplayBudget {
    /// Maximum number of diagnostic groups rendered in full.
    pub expanded_groups: usize,
    /// Maximum session output lines before truncation in legacy/session-global paths.
    pub first_screenful_max_lines: usize,
    /// Target output lines per rendered diagnostic block before degradation begins.
    pub target_lines_per_block: usize,
    /// Hard maximum output lines per rendered diagnostic block.
    pub hard_max_lines_per_block: usize,
    /// Maximum source code excerpt blocks per card.
    pub source_excerpts: usize,
    /// Maximum template instantiation frames shown.
    pub template_frames: usize,
    /// Maximum macro/include chain frames shown.
    pub macro_include_frames: usize,
    /// Maximum overload candidate notes shown.
    pub candidate_notes: usize,
    /// How warnings are handled when errors are present.
    pub warning_failure_mode: WarningFailureMode,
}

/// Static text and limits that govern disclosure notices and honesty labels.
#[derive(Debug, Clone, Copy)]
pub struct DisclosurePolicy {
    /// Notice shown when the document is only partially structured.
    pub partial_document_notice: &'static str,
    /// Notice shown when analysis confidence is low.
    pub low_confidence_notice: &'static str,
    /// Hint directing the user to the raw fallback profile.
    pub raw_diagnostics_hint: &'static str,
    /// Notice shown when output was truncated.
    pub truncation_notice: &'static str,
    /// Notice shown when a single diagnostic block was truncated locally.
    pub block_truncation_notice: &'static str,
    /// Label preceding the raw compiler excerpt block.
    pub raw_block_label: &'static str,
    /// Notice shown when warnings were suppressed.
    pub suppressed_warning_notice: &'static str,
    /// Maximum lines in the raw sub-block per card.
    pub raw_sub_block_lines: usize,
}

/// Combined budget and disclosure settings for a render profile.
#[derive(Debug, Clone, Copy)]
pub struct RenderPolicy {
    /// Numeric display limits.
    pub budget: DisplayBudget,
    /// Disclosure and honesty label configuration.
    pub disclosure: DisclosurePolicy,
}

/// Returns the combined render policy for the given profile.
pub fn render_policy(profile: RenderProfile) -> RenderPolicy {
    RenderPolicy {
        budget: budget_for(profile),
        disclosure: disclosure_policy_for(profile),
    }
}

/// Returns the display budget for the given render profile.
pub fn budget_for(profile: RenderProfile) -> DisplayBudget {
    match profile {
        RenderProfile::Default => DisplayBudget {
            expanded_groups: 1,
            first_screenful_max_lines: 28,
            target_lines_per_block: 18,
            hard_max_lines_per_block: 28,
            source_excerpts: 2,
            template_frames: 5,
            macro_include_frames: 4,
            candidate_notes: 3,
            warning_failure_mode: WarningFailureMode::Summarize,
        },
        RenderProfile::Concise => DisplayBudget {
            expanded_groups: 1,
            first_screenful_max_lines: 14,
            target_lines_per_block: 10,
            hard_max_lines_per_block: 14,
            source_excerpts: 1,
            template_frames: 3,
            macro_include_frames: 2,
            candidate_notes: 2,
            warning_failure_mode: WarningFailureMode::Suppress,
        },
        RenderProfile::Verbose => DisplayBudget {
            expanded_groups: usize::MAX,
            first_screenful_max_lines: 80,
            target_lines_per_block: 40,
            hard_max_lines_per_block: 80,
            source_excerpts: 6,
            template_frames: 20,
            macro_include_frames: 12,
            candidate_notes: 10,
            warning_failure_mode: WarningFailureMode::Show,
        },
        RenderProfile::Debug => DisplayBudget {
            expanded_groups: usize::MAX,
            first_screenful_max_lines: 120,
            target_lines_per_block: 60,
            hard_max_lines_per_block: 120,
            source_excerpts: 8,
            template_frames: 30,
            macro_include_frames: 20,
            candidate_notes: 20,
            warning_failure_mode: WarningFailureMode::Show,
        },
        RenderProfile::Ci => DisplayBudget {
            expanded_groups: 1,
            first_screenful_max_lines: 16,
            target_lines_per_block: 12,
            hard_max_lines_per_block: 16,
            source_excerpts: 1,
            template_frames: 3,
            macro_include_frames: 2,
            candidate_notes: 2,
            warning_failure_mode: WarningFailureMode::Summarize,
        },
        RenderProfile::RawFallback => DisplayBudget {
            expanded_groups: 0,
            first_screenful_max_lines: usize::MAX,
            target_lines_per_block: 0,
            hard_max_lines_per_block: usize::MAX,
            source_excerpts: 0,
            template_frames: 0,
            macro_include_frames: 0,
            candidate_notes: 0,
            warning_failure_mode: WarningFailureMode::Show,
        },
    }
}

/// Returns the disclosure policy for the given render profile.
pub fn disclosure_policy_for(profile: RenderProfile) -> DisclosurePolicy {
    DisclosurePolicy {
        partial_document_notice: "note: some compiler details were not fully structured; original diagnostics are preserved",
        low_confidence_notice: "note: wrapper confidence is low; verify against the preserved raw diagnostics",
        raw_diagnostics_hint: "raw: rerun with --formed-profile=raw_fallback to inspect the original compiler output",
        truncation_notice: match profile {
            RenderProfile::Debug => {
                "note: omitted additional details even under --formed-profile=debug; inspect the preserved raw diagnostics"
            }
            _ => "note: omitted additional details; rerun with --formed-profile=verbose",
        },
        block_truncation_notice: match profile {
            RenderProfile::Debug => {
                "note: omitted additional details from this diagnostic block even under --formed-profile=debug"
            }
            _ => "note: omitted additional details from this diagnostic block",
        },
        raw_block_label: "raw:",
        suppressed_warning_notice: "note: suppressed {count} warning(s) while focusing on the failing group",
        raw_sub_block_lines: match profile {
            RenderProfile::Verbose => 4,
            RenderProfile::Debug => 6,
            RenderProfile::Default => 2,
            RenderProfile::Concise | RenderProfile::Ci => 1,
            RenderProfile::RawFallback => 0,
        },
    }
}
