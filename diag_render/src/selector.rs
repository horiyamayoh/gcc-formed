use crate::budget::{WarningFailureMode, budget_for};
use crate::{RenderProfile, RenderRequest, WarningVisibility};
use diag_core::{Confidence, DiagnosticNode, Ownership, Phase, SemanticRole, Severity};

#[derive(Debug)]
pub struct Selection {
    pub cards: Vec<DiagnosticNode>,
    pub suppressed_warning_count: usize,
}

pub fn select_groups(request: &RenderRequest) -> Selection {
    let budget = budget_for(request.profile);
    let mut diagnostics = request.document.diagnostics.clone();
    diagnostics.sort_by(|left, right| {
        sort_key(right)
            .cmp(&sort_key(left))
            .then_with(|| left.id.cmp(&right.id))
    });
    let has_failure = diagnostics
        .iter()
        .any(|node| matches!(node.severity, Severity::Fatal | Severity::Error));

    let mut suppressed_warning_count = 0;
    if has_failure
        && should_filter_warnings(request.warning_visibility, budget.warning_failure_mode)
    {
        diagnostics.retain(|node| {
            if matches!(node.severity, Severity::Warning) {
                suppressed_warning_count += 1;
                false
            } else {
                true
            }
        });
    }

    let expanded_groups = match request.profile {
        RenderProfile::Default if !has_failure => 2,
        _ => budget.expanded_groups,
    };
    let expanded = diagnostics.into_iter().take(expanded_groups).collect();
    Selection {
        cards: expanded,
        suppressed_warning_count,
    }
}

fn should_filter_warnings(
    visibility: WarningVisibility,
    warning_failure_mode: WarningFailureMode,
) -> bool {
    match visibility {
        WarningVisibility::ShowAll => false,
        WarningVisibility::SuppressAll => true,
        WarningVisibility::Auto => !matches!(warning_failure_mode, WarningFailureMode::Show),
    }
}

fn sort_key(node: &DiagnosticNode) -> (u8, u8, u8, u8, u8, u8, usize) {
    (
        severity_rank(&node.severity),
        ownership_rank(best_ownership(node)),
        confidence_rank(
            node.analysis
                .as_ref()
                .and_then(|analysis| analysis.confidence.as_ref()),
        ),
        phase_rank(&node.phase),
        semantic_role_rank(&node.semantic_role),
        specificity_rank(node),
        std::cmp::Reverse(node.message.raw_text.len()).0,
    )
}

fn severity_rank(severity: &Severity) -> u8 {
    match severity {
        Severity::Fatal => 7,
        Severity::Error => 6,
        Severity::Warning => 5,
        Severity::Note => 4,
        Severity::Remark => 3,
        Severity::Info => 2,
        Severity::Debug => 1,
        Severity::Unknown => 0,
    }
}

fn ownership_rank(ownership: Option<&Ownership>) -> u8 {
    match ownership {
        Some(Ownership::User) => 4,
        Some(Ownership::Vendor) => 3,
        Some(Ownership::Generated) => 2,
        Some(Ownership::System) => 1,
        _ => 0,
    }
}

fn best_ownership(node: &DiagnosticNode) -> Option<&Ownership> {
    node.primary_location()
        .and_then(|location| location.ownership.as_ref())
        .or_else(|| {
            node.locations
                .iter()
                .filter_map(|location| location.ownership.as_ref())
                .max_by_key(|ownership| ownership_rank(Some(*ownership)))
        })
}

fn confidence_rank(confidence: Option<&Confidence>) -> u8 {
    match confidence {
        Some(Confidence::High) => 4,
        Some(Confidence::Medium) => 3,
        Some(Confidence::Low) => 2,
        Some(Confidence::Unknown) | None => 1,
    }
}

fn phase_rank(phase: &Phase) -> u8 {
    match phase {
        Phase::Parse => 9,
        Phase::Semantic => 8,
        Phase::Instantiate => 7,
        Phase::Constraints => 6,
        Phase::Analyze => 5,
        Phase::Codegen => 4,
        Phase::Assemble => 3,
        Phase::Link => 2,
        Phase::Driver | Phase::Preprocess | Phase::Optimize | Phase::Archive | Phase::Unknown => 1,
    }
}

fn specificity_rank(node: &DiagnosticNode) -> u8 {
    let family_rank = node
        .analysis
        .as_ref()
        .and_then(|analysis| analysis.family.as_deref())
        .map(|family| match family {
            "unknown" | "passthrough" | "linker.file_format_or_relocation" => 0,
            family if family.starts_with("linker.") => 3,
            _ => 2,
        })
        .unwrap_or(0);
    let symbol_rank = u8::from(node.symbol_context.is_some());
    let first_action_rank = node
        .analysis
        .as_ref()
        .and_then(|analysis| analysis.first_action_hint.as_ref())
        .map(|hint| !hint.trim().is_empty())
        .unwrap_or(false);
    family_rank + symbol_rank + u8::from(first_action_rank)
}

fn semantic_role_rank(role: &SemanticRole) -> u8 {
    match role {
        SemanticRole::Root => 7,
        SemanticRole::Summary => 6,
        SemanticRole::Help => 5,
        SemanticRole::Supporting => 4,
        SemanticRole::Candidate => 3,
        SemanticRole::PathEvent => 2,
        SemanticRole::Passthrough => 1,
        SemanticRole::Unknown => 0,
    }
}
