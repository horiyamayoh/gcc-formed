use crate::{RenderProfile, RenderRequest, WarningVisibility};
use diag_core::{DiagnosticNode, Ownership, Severity};

#[derive(Debug)]
pub struct Selection {
    pub cards: Vec<DiagnosticNode>,
    pub suppressed_warning_count: usize,
}

pub fn select_groups(request: &RenderRequest) -> Selection {
    let mut diagnostics = request.document.diagnostics.clone();
    diagnostics.sort_by(|left, right| sort_key(right).cmp(&sort_key(left)));
    let has_failure = diagnostics
        .iter()
        .any(|node| matches!(node.severity, Severity::Fatal | Severity::Error));

    let mut suppressed_warning_count = 0;
    if has_failure
        && matches!(
            request.warning_visibility,
            WarningVisibility::Auto | WarningVisibility::SuppressAll
        )
        && !matches!(request.profile, RenderProfile::Verbose)
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

    let expanded = match request.profile {
        RenderProfile::Verbose => diagnostics,
        _ => diagnostics.into_iter().take(1).collect(),
    };
    Selection {
        cards: expanded,
        suppressed_warning_count,
    }
}

fn sort_key(node: &DiagnosticNode) -> (u8, u8, usize) {
    (
        severity_rank(&node.severity),
        ownership_rank(
            node.primary_location()
                .and_then(|location| location.ownership.as_ref()),
        ),
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
