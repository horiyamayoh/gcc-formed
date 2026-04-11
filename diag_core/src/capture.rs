use serde::{Deserialize, Serialize};

use crate::{ContextChainKind, ProvenanceSource, SuggestionApplicability, ToolInfo};

/// A raw artifact captured from the build (e.g. stderr text, SARIF blob).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CaptureArtifact {
    /// Unique identifier for this capture within the document.
    pub id: String,
    /// The kind of artifact captured.
    pub kind: ArtifactKind,
    /// MIME media type (e.g. `"text/plain"`, `"application/json"`).
    pub media_type: String,
    /// Character encoding (e.g. `"utf-8"`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub encoding: Option<String>,
    /// SHA-256 digest of the raw content bytes.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub digest_sha256: Option<String>,
    /// Size of the raw content in bytes.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size_bytes: Option<u64>,
    /// Where the artifact content is stored.
    pub storage: ArtifactStorage,
    /// Inline text content when `storage` is [`ArtifactStorage::Inline`].
    #[serde(skip_serializing_if = "Option::is_none")]
    pub inline_text: Option<String>,
    /// External reference path when `storage` is [`ArtifactStorage::ExternalRef`].
    #[serde(skip_serializing_if = "Option::is_none")]
    pub external_ref: Option<String>,
    /// The tool that produced this artifact, if known.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub produced_by: Option<ToolInfo>,
}

/// The type of a captured build artifact.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ArtifactKind {
    /// GCC SARIF JSON output.
    GccSarif,
    /// GCC JSON diagnostics output.
    GccJson,
    /// Raw compiler stderr text.
    CompilerStderrText,
    /// Raw linker stderr text.
    LinkerStderrText,
    /// Raw compiler stdout text.
    CompilerStdoutText,
    /// Wrapper internal trace data.
    WrapperTrace,
    /// A source-code snippet.
    SourceSnippet,
    /// Any other artifact type.
    Other,
}

/// Where the content of a [`CaptureArtifact`] is stored.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ArtifactStorage {
    /// Content is stored inline in the document.
    Inline,
    /// Content is at an external file or URI.
    ExternalRef,
    /// Content is not available.
    Unavailable,
}

/// A problem detected during document production (e.g. parse failure, normalization issue).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct IntegrityIssue {
    /// How severe this issue is.
    pub severity: IssueSeverity,
    /// Pipeline stage where the issue was detected.
    pub stage: IssueStage,
    /// Human-readable description of the issue.
    pub message: String,
    /// Provenance linking this issue to a capture artifact.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provenance: Option<Provenance>,
}

/// Severity of an [`IntegrityIssue`].
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum IssueSeverity {
    /// The issue is blocking and indicates data loss.
    Error,
    /// The issue is non-blocking but may affect accuracy.
    Warning,
    /// Informational notice.
    Info,
}

/// Pipeline stage where an [`IntegrityIssue`] was detected.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum IssueStage {
    /// Issue during artifact capture.
    Capture,
    /// Issue during parsing of captured data.
    Parse,
    /// Issue during normalisation into IR.
    Normalize,
    /// Issue during analysis/enrichment.
    Analyze,
    /// Issue during rendering.
    Render,
}

/// A suggested code fix for a diagnostic.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Suggestion {
    /// Human-readable description of the fix.
    pub label: String,
    /// How safe it is to apply this fix automatically.
    pub applicability: SuggestionApplicability,
    /// Text edits that implement the fix.
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub edits: Vec<TextEdit>,
}

/// A single text replacement in a source file.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TextEdit {
    /// File path to edit.
    pub path: String,
    /// 1-based start line of the region to replace.
    pub start_line: u32,
    /// 1-based start column of the region to replace.
    pub start_column: u32,
    /// 1-based end line of the region to replace.
    pub end_line: u32,
    /// 1-based end column of the region to replace.
    pub end_column: u32,
    /// Text to insert in place of the removed region.
    pub replacement: String,
}

/// A chain of contextual frames (e.g. include stack, template instantiation trace).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ContextChain {
    /// The kind of context this chain represents.
    pub kind: ContextChainKind,
    /// Ordered list of frames in the chain (outermost first).
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub frames: Vec<ContextFrame>,
}

/// A single frame in a [`ContextChain`].
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ContextFrame {
    /// Human-readable label for this frame.
    pub label: String,
    /// File path where this frame originates.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    /// Line number in the source file.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line: Option<u32>,
    /// Column number in the source file.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub column: Option<u32>,
}

/// Symbol information for linker-related diagnostics.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SymbolContext {
    /// The primary symbol involved (e.g. undefined reference target).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub primary_symbol: Option<String>,
    /// Other related object files or symbols.
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub related_objects: Vec<String>,
    /// Archive (`.a`) file containing the symbol, if applicable.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub archive: Option<String>,
}

/// Data lineage linking an IR element back to its captured artifact(s).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Provenance {
    /// What produced the data.
    pub source: ProvenanceSource,
    /// IDs of [`CaptureArtifact`]s this element was derived from.
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub capture_refs: Vec<String>,
}
