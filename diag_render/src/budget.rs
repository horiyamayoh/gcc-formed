use crate::RenderProfile;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WarningFailureMode {
    Summarize,
    Suppress,
    Show,
}

#[derive(Debug, Clone, Copy)]
pub struct DisplayBudget {
    pub expanded_groups: usize,
    pub first_screenful_max_lines: usize,
    pub source_excerpts: usize,
    pub template_frames: usize,
    pub macro_include_frames: usize,
    pub candidate_notes: usize,
    pub warning_failure_mode: WarningFailureMode,
}

#[derive(Debug, Clone, Copy)]
pub struct DisclosurePolicy {
    pub partial_document_notice: &'static str,
    pub low_confidence_notice: &'static str,
    pub raw_diagnostics_hint: &'static str,
    pub truncation_notice: &'static str,
    pub raw_block_label: &'static str,
    pub suppressed_warning_notice: &'static str,
    pub raw_sub_block_lines: usize,
}

#[derive(Debug, Clone, Copy)]
pub struct RenderPolicy {
    pub budget: DisplayBudget,
    pub disclosure: DisclosurePolicy,
}

pub fn render_policy(profile: RenderProfile) -> RenderPolicy {
    RenderPolicy {
        budget: budget_for(profile),
        disclosure: disclosure_policy_for(profile),
    }
}

pub fn budget_for(profile: RenderProfile) -> DisplayBudget {
    match profile {
        RenderProfile::Default => DisplayBudget {
            expanded_groups: 1,
            first_screenful_max_lines: 28,
            source_excerpts: 2,
            template_frames: 5,
            macro_include_frames: 4,
            candidate_notes: 3,
            warning_failure_mode: WarningFailureMode::Summarize,
        },
        RenderProfile::Concise => DisplayBudget {
            expanded_groups: 1,
            first_screenful_max_lines: 14,
            source_excerpts: 1,
            template_frames: 3,
            macro_include_frames: 2,
            candidate_notes: 2,
            warning_failure_mode: WarningFailureMode::Suppress,
        },
        RenderProfile::Verbose => DisplayBudget {
            expanded_groups: usize::MAX,
            first_screenful_max_lines: 80,
            source_excerpts: 6,
            template_frames: 20,
            macro_include_frames: 12,
            candidate_notes: 10,
            warning_failure_mode: WarningFailureMode::Show,
        },
        RenderProfile::Ci => DisplayBudget {
            expanded_groups: 1,
            first_screenful_max_lines: 16,
            source_excerpts: 1,
            template_frames: 3,
            macro_include_frames: 2,
            candidate_notes: 2,
            warning_failure_mode: WarningFailureMode::Summarize,
        },
        RenderProfile::RawFallback => DisplayBudget {
            expanded_groups: 0,
            first_screenful_max_lines: usize::MAX,
            source_excerpts: 0,
            template_frames: 0,
            macro_include_frames: 0,
            candidate_notes: 0,
            warning_failure_mode: WarningFailureMode::Show,
        },
    }
}

pub fn disclosure_policy_for(profile: RenderProfile) -> DisclosurePolicy {
    DisclosurePolicy {
        partial_document_notice: "note: some compiler details were not fully structured; original diagnostics are preserved",
        low_confidence_notice: "note: wrapper confidence is low; verify against the preserved raw diagnostics",
        raw_diagnostics_hint: "raw: rerun with --formed-profile=raw_fallback to inspect the original compiler output",
        truncation_notice: "note: omitted additional details; rerun with --formed-profile=verbose",
        raw_block_label: "raw:",
        suppressed_warning_notice: "note: suppressed {count} warning(s) while focusing on the failing group",
        raw_sub_block_lines: match profile {
            RenderProfile::Verbose => 4,
            RenderProfile::Default => 2,
            RenderProfile::Concise | RenderProfile::Ci => 1,
            RenderProfile::RawFallback => 0,
        },
    }
}
