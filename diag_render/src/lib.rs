mod excerpt;
mod fallback;
mod family;
mod formatter;
mod selector;
mod view_model;

use diag_core::{DiagnosticDocument, IntegrityIssue};
use serde::{Deserialize, Serialize};

pub use excerpt::ExcerptBlock;
pub use selector::select_groups;
pub use view_model::{RenderGroupCard, RenderSessionSummary, RenderViewModel};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RenderProfile {
    Default,
    Concise,
    Verbose,
    Ci,
    RawFallback,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StreamKind {
    Tty,
    Pipe,
    File,
    CiLog,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PathPolicy {
    ShortestUnambiguous,
    RelativeToCwd,
    Absolute,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WarningVisibility {
    Auto,
    ShowAll,
    SuppressAll,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DebugRefs {
    None,
    TraceId,
    CaptureRef,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TypeDisplayPolicy {
    Full,
    CompactSafe,
    RawFirst,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceExcerptPolicy {
    Auto,
    ForceOn,
    ForceOff,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RenderRequest {
    pub document: DiagnosticDocument,
    pub profile: RenderProfile,
    pub capabilities: RenderCapabilities,
    pub cwd: Option<std::path::PathBuf>,
    pub path_policy: PathPolicy,
    pub warning_visibility: WarningVisibility,
    pub debug_refs: DebugRefs,
    pub type_display_policy: TypeDisplayPolicy,
    pub source_excerpt_policy: SourceExcerptPolicy,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RenderCapabilities {
    pub stream_kind: StreamKind,
    pub width_columns: Option<usize>,
    pub ansi_color: bool,
    pub unicode: bool,
    pub hyperlinks: bool,
    pub interactive: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RenderResult {
    pub text: String,
    pub used_analysis: bool,
    pub used_fallback: bool,
    pub displayed_group_refs: Vec<String>,
    pub suppressed_group_count: usize,
    pub suppressed_warning_count: usize,
    pub truncation_occurred: bool,
    pub render_issues: Vec<IntegrityIssue>,
}

#[derive(Debug, thiserror::Error)]
pub enum RenderError {
    #[error("render failed")]
    Failed,
}

pub fn render(request: RenderRequest) -> Result<RenderResult, RenderError> {
    if matches!(request.profile, RenderProfile::RawFallback)
        || matches!(
            request.document.document_completeness,
            diag_core::DocumentCompleteness::Passthrough | diag_core::DocumentCompleteness::Failed
        )
    {
        return Ok(fallback::render_fallback(&request));
    }

    let selected = selector::select_groups(&request);
    if selected.cards.is_empty() {
        return Ok(fallback::render_fallback(&request));
    }
    let view_model = view_model::build(&request, selected.cards);
    Ok(formatter::emit(
        &request,
        view_model,
        selected.suppressed_warning_count,
    ))
}
