//! Shared type definitions for the gcc-formed diagnostic IR.

use ordered_float::OrderedFloat;
use serde::{Deserialize, Serialize};
use std::fmt::{Display, Formatter};

/// Semantic-version string for the current IR schema.
pub const IR_SPEC_VERSION: &str = "1.0.0-alpha.1";
/// Version tag for the adapter contract.
pub const ADAPTER_SPEC_VERSION: &str = "v1alpha";
/// Version tag for the renderer contract.
pub const RENDERER_SPEC_VERSION: &str = "v1alpha";
/// Numeric confidence/priority score in the range `0.0..=1.0`.
pub type Score = OrderedFloat<f32>;
/// Minimum score that maps to [`crate::DisclosureConfidence::Certain`].
pub const CONFIDENCE_CERTAIN_THRESHOLD: f32 = 0.85;
/// Minimum score that maps to [`crate::DisclosureConfidence::Likely`].
pub const CONFIDENCE_LIKELY_THRESHOLD: f32 = 0.60;
/// Minimum score that maps to [`crate::DisclosureConfidence::Possible`].
pub const CONFIDENCE_POSSIBLE_THRESHOLD: f32 = 0.35;

/// Controls which layers are included when producing a snapshot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SnapshotKind {
    /// Snapshot contains only compiler-emitted facts; analysis overlays are stripped.
    FactsOnly,
    /// Snapshot retains analysis overlays alongside facts.
    AnalysisIncluded,
}

/// Describes how completely a document was produced.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DocumentCompleteness {
    /// All diagnostics were fully parsed and normalised.
    Complete,
    /// Some diagnostics could not be fully resolved.
    Partial,
    /// Raw compiler output was passed through without processing.
    Passthrough,
    /// Document production failed entirely.
    Failed,
}

/// Reason the pipeline fell back to raw passthrough output.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[serde(rename_all = "snake_case")]
pub enum FallbackReason {
    /// Compiler version band is outside the current public contract.
    UnsupportedVersionBand,
    /// The output sink cannot render structured diagnostics.
    IncompatibleSink,
    /// Pipeline is running in shadow/observation mode only.
    ShadowMode,
    /// No SARIF artifact was found in the compiler output.
    SarifMissing,
    /// SARIF artifact was present but could not be parsed.
    SarifParseFailed,
    /// Only residual (unparsed) text remained after processing.
    ResidualOnly,
    /// Renderer confidence was too low to emit structured output.
    RendererLowConfidence,
    /// An unexpected internal error occurred.
    InternalError,
    /// Processing was aborted due to a timeout or budget limit.
    TimeoutOrBudget,
    /// User explicitly opted out of structured output.
    UserOptOut,
}

impl FallbackReason {
    /// Returns the `snake_case` string representation of this reason.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::UnsupportedVersionBand => "unsupported_version_band",
            Self::IncompatibleSink => "incompatible_sink",
            Self::ShadowMode => "shadow_mode",
            Self::SarifMissing => "sarif_missing",
            Self::SarifParseFailed => "sarif_parse_failed",
            Self::ResidualOnly => "residual_only",
            Self::RendererLowConfidence => "renderer_low_confidence",
            Self::InternalError => "internal_error",
            Self::TimeoutOrBudget => "timeout_or_budget",
            Self::UserOptOut => "user_opt_out",
        }
    }
}

impl Display for FallbackReason {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Indicates which structured data source was authoritative for the document.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SourceAuthority {
    /// Diagnostics came from a structured source (e.g. SARIF).
    Structured,
    /// Diagnostics were extracted from unstructured residual text.
    ResidualText,
    /// No authoritative source was available.
    None,
}

/// Classifies the degree of fallback applied during rendering.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum FallbackGrade {
    /// No fallback was necessary.
    None,
    /// Partial fallback for backward-compatibility.
    Compatibility,
    /// Full passthrough fallback to preserve output on failure.
    FailOpen,
}

/// The environment surface where the wrapper is running.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WrapperSurface {
    /// Interactive terminal session.
    Terminal,
    /// Continuous-integration environment.
    Ci,
    /// Editor/IDE integration.
    Editor,
    /// Trace-only mode; output is logged but not displayed.
    TraceOnly,
    /// Surface could not be determined.
    Unknown,
}

/// Source language being compiled.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LanguageMode {
    /// C language.
    C,
    /// C++ language.
    Cpp,
    /// Objective-C language.
    Objc,
    /// Objective-C++ language.
    Objcpp,
    /// Language could not be determined.
    Unknown,
}

/// The tool that originally emitted a diagnostic.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Origin {
    /// GCC compiler front-end.
    Gcc,
    /// Clang compiler front-end.
    Clang,
    /// System linker.
    Linker,
    /// Compiler driver (orchestration layer).
    Driver,
    /// The gcc-formed wrapper itself.
    Wrapper,
    /// An external third-party tool.
    ExternalTool,
    /// Origin could not be determined.
    Unknown,
}

/// Compiler phase that produced a diagnostic.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Phase {
    /// Driver orchestration phase.
    Driver,
    /// Preprocessing phase.
    Preprocess,
    /// Parsing phase.
    Parse,
    /// Semantic analysis phase.
    Semantic,
    /// Template/generic instantiation phase.
    Instantiate,
    /// Constraint-checking phase (e.g. C++20 concepts).
    Constraints,
    /// Static analysis phase.
    Analyze,
    /// Optimization phase.
    Optimize,
    /// Code generation phase.
    Codegen,
    /// Assembly phase.
    Assemble,
    /// Linking phase.
    Link,
    /// Archiving phase (static library creation).
    Archive,
    /// Phase could not be determined.
    Unknown,
}

/// Diagnostic severity level, ordered from most to least severe.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    /// Unrecoverable error that terminates compilation.
    Fatal,
    /// Compilation error.
    Error,
    /// Compiler warning.
    Warning,
    /// Informational note attached to another diagnostic.
    Note,
    /// Optimisation or analysis remark.
    Remark,
    /// General informational message.
    Info,
    /// Debug-level diagnostic (typically suppressed).
    Debug,
    /// Severity could not be determined.
    Unknown,
}

/// Semantic role of a [`crate::DiagnosticNode`] within the diagnostic tree.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SemanticRole {
    /// Top-level root diagnostic.
    Root,
    /// Supporting child note.
    Supporting,
    /// Help/hint message.
    Help,
    /// Overload or template candidate.
    Candidate,
    /// Event along a static-analysis path.
    PathEvent,
    /// Summary node grouping multiple diagnostics.
    Summary,
    /// Unprocessed passthrough node.
    Passthrough,
    /// Role could not be determined.
    Unknown,
}

/// Role that a [`crate::Location`] plays for its diagnostic.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LocationRole {
    /// Primary location where the error occurred.
    Primary,
    /// Secondary supporting location.
    Secondary,
    /// Related but non-essential location.
    Related,
    /// Contextual location (e.g. enclosing scope).
    Context,
    /// Target location for a suggested edit.
    EditTarget,
    /// Location where a symbol is referenced.
    SymbolReference,
    /// Location where a symbol is defined.
    SymbolDefinition,
    /// Any other role.
    Other,
}

/// How a [`crate::Location`] was derived from compiler output.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LocationSourceKind {
    /// Single caret position.
    Caret,
    /// Span covering a range of source text.
    Range,
    /// Token-level location.
    Token,
    /// Insertion point for a fix-it.
    Insertion,
    /// Macro or template expansion location.
    Expansion,
    /// Compiler-generated (no real source).
    Generated,
    /// Any other derivation.
    Other,
}

/// Style of a file path string.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PathStyle {
    /// POSIX-style path with `/` separators.
    Posix,
    /// Windows-style path with `\` separators.
    Windows,
    /// URI-style path (e.g. `file:///...`).
    Uri,
    /// Virtual/synthetic path (e.g. `<built-in>`).
    Virtual,
    /// Style could not be determined.
    Unknown,
}

/// Whether a file path is absolute, relative, virtual, or generated.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PathKind {
    /// Fully-qualified absolute path.
    Absolute,
    /// Relative path (to some working directory).
    Relative,
    /// Virtual path (e.g. `<built-in>`).
    Virtual,
    /// Path for generated/temporary content.
    Generated,
    /// Kind could not be determined.
    Unknown,
}

/// Unit used for column numbers in a [`crate::SourcePoint`].
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ColumnUnit {
    /// Byte offset within the line.
    Byte,
    /// Display-width column (tab-expanded, wide-char aware).
    Display,
    /// UTF-16 code-unit offset (LSP convention).
    Utf16CodeUnit,
    /// Unicode scalar value offset.
    UnicodeScalar,
    /// Unit could not be determined.
    Unknown,
}

/// End-point semantics for a [`crate::SourceRange`].
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum BoundarySemantics {
    /// End is exclusive (half-open interval).
    HalfOpen,
    /// End is inclusive (closed interval).
    InclusiveEnd,
    /// Range collapses to a single point.
    Point,
    /// Semantics could not be determined.
    Unknown,
}

/// Who owns a source file (used for prioritising user-actionable diagnostics).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Ownership {
    /// File belongs to the user's project.
    User,
    /// File comes from a third-party/vendor dependency.
    Vendor,
    /// System header or library.
    System,
    /// File was generated (e.g. by a code generator).
    Generated,
    /// File was produced by a build tool.
    Tool,
    /// Ownership could not be determined.
    Unknown,
}

/// How confidently a [`crate::Suggestion`] can be applied automatically.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SuggestionApplicability {
    /// Fix can be safely applied by a tool.
    MachineApplicable,
    /// Fix is likely correct but should be reviewed.
    MaybeIncorrect,
    /// Fix requires manual human intervention.
    Manual,
}

/// The type of context represented by a [`crate::ContextChain`].
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ContextChainKind {
    /// `#include` file inclusion stack.
    Include,
    /// Macro expansion trace.
    MacroExpansion,
    /// C++ template instantiation backtrace.
    TemplateInstantiation,
    /// Linker symbol resolution chain.
    LinkerResolution,
    /// Static-analyzer path trace.
    AnalyzerPath,
    /// Any other context chain type.
    Other,
}

/// How completely a [`crate::DiagnosticNode`] was parsed from the source data.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum NodeCompleteness {
    /// All fields were fully populated from structured data.
    Complete,
    /// Some fields are missing or approximate.
    Partial,
    /// Node is a raw passthrough of unparsed text.
    Passthrough,
    /// Node was synthesised by the wrapper (not from compiler output).
    Synthesized,
}

/// Origin of the data behind a [`crate::Provenance`].
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ProvenanceSource {
    /// Data came from compiler structured output (SARIF, JSON).
    Compiler,
    /// Data came from linker output.
    Linker,
    /// Data was generated by the wrapper itself.
    WrapperGenerated,
    /// Data was extracted from residual unstructured text.
    ResidualText,
    /// Data was injected by policy rules.
    Policy,
    /// Source is unknown.
    Unknown,
}

impl Display for Severity {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        let value = match self {
            Severity::Fatal => "fatal",
            Severity::Error => "error",
            Severity::Warning => "warning",
            Severity::Note => "note",
            Severity::Remark => "remark",
            Severity::Info => "info",
            Severity::Debug => "debug",
            Severity::Unknown => "unknown",
        };
        formatter.write_str(value)
    }
}

impl Display for Ownership {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        let value = match self {
            Ownership::User => "user",
            Ownership::Vendor => "vendor",
            Ownership::System => "system",
            Ownership::Generated => "generated",
            Ownership::Tool => "tool",
            Ownership::Unknown => "unknown",
        };
        formatter.write_str(value)
    }
}

impl Display for Phase {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        let value = match self {
            Phase::Driver => "driver",
            Phase::Preprocess => "preprocess",
            Phase::Parse => "parse",
            Phase::Semantic => "semantic",
            Phase::Instantiate => "instantiate",
            Phase::Constraints => "constraints",
            Phase::Analyze => "analyze",
            Phase::Optimize => "optimize",
            Phase::Codegen => "codegen",
            Phase::Assemble => "assemble",
            Phase::Link => "link",
            Phase::Archive => "archive",
            Phase::Unknown => "unknown",
        };
        formatter.write_str(value)
    }
}
