use crate::RenderProfile;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WarningFailureMode {
    Summarize,
    Suppress,
    Show,
}

#[derive(Debug, Clone, Copy)]
pub struct ProfileBudget {
    pub expanded_groups: usize,
    pub hard_max_lines: usize,
    pub source_excerpts: usize,
    pub template_frames: usize,
    pub macro_include_frames: usize,
    pub candidate_notes: usize,
    pub warning_failure_mode: WarningFailureMode,
}

pub fn budget_for(profile: RenderProfile) -> ProfileBudget {
    match profile {
        RenderProfile::Default => ProfileBudget {
            expanded_groups: 1,
            hard_max_lines: 28,
            source_excerpts: 2,
            template_frames: 5,
            macro_include_frames: 4,
            candidate_notes: 3,
            warning_failure_mode: WarningFailureMode::Summarize,
        },
        RenderProfile::Concise => ProfileBudget {
            expanded_groups: 1,
            hard_max_lines: 14,
            source_excerpts: 1,
            template_frames: 3,
            macro_include_frames: 2,
            candidate_notes: 2,
            warning_failure_mode: WarningFailureMode::Suppress,
        },
        RenderProfile::Verbose => ProfileBudget {
            expanded_groups: usize::MAX,
            hard_max_lines: 80,
            source_excerpts: 6,
            template_frames: 20,
            macro_include_frames: 12,
            candidate_notes: 10,
            warning_failure_mode: WarningFailureMode::Show,
        },
        RenderProfile::Ci => ProfileBudget {
            expanded_groups: 1,
            hard_max_lines: 16,
            source_excerpts: 1,
            template_frames: 3,
            macro_include_frames: 2,
            candidate_notes: 2,
            warning_failure_mode: WarningFailureMode::Summarize,
        },
        RenderProfile::RawFallback => ProfileBudget {
            expanded_groups: 0,
            hard_max_lines: usize::MAX,
            source_excerpts: 0,
            template_frames: 0,
            macro_include_frames: 0,
            candidate_notes: 0,
            warning_failure_mode: WarningFailureMode::Show,
        },
    }
}
