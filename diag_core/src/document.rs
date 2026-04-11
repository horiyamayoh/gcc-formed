use serde::{Deserialize, Serialize};

use crate::{
    AnalysisOverlay, CaptureArtifact, ContextChain, DocumentCompleteness, FingerprintSet,
    IntegrityIssue, LanguageMode, Location, LocationRole, NodeCompleteness, Origin, Phase,
    Provenance, SemanticRole, Severity, Suggestion, SymbolContext, WrapperSurface,
};

/// Identifies the software that produced this diagnostic document.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProducerInfo {
    /// Human-readable producer name (e.g. `"gcc-formed"`).
    pub name: String,
    /// Semantic version of the producer.
    pub version: String,
    /// Git revision hash at build time, if available.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub git_revision: Option<String>,
    /// Build profile (e.g. `"release"`, `"debug"`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub build_profile: Option<String>,
    /// Version of the rule-pack used for enrichment, if any.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rulepack_version: Option<String>,
}

/// Metadata about the compiler invocation that was captured.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RunInfo {
    /// Unique identifier for this invocation.
    pub invocation_id: String,
    /// The command name the user typed (e.g. `"gcc"`, `"cc"`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub invoked_as: Option<String>,
    /// Redacted copy of the argument vector.
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub argv_redacted: Vec<String>,
    /// Working directory at invocation time (display form).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cwd_display: Option<String>,
    /// Process exit status code.
    pub exit_status: i32,
    /// The primary compiler/linker tool that ran.
    pub primary_tool: ToolInfo,
    /// Any secondary tools involved in the build step.
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub secondary_tools: Vec<ToolInfo>,
    /// Source language mode detected or specified.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub language_mode: Option<LanguageMode>,
    /// Target triple (e.g. `"x86_64-unknown-linux-gnu"`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_triple: Option<String>,
    /// Surface the wrapper was running in.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wrapper_mode: Option<WrapperSurface>,
}

/// Describes a single tool in the build pipeline.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ToolInfo {
    /// Tool name (e.g. `"gcc"`, `"ld"`).
    pub name: String,
    /// Tool version string.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    /// Sub-component within the tool (e.g. `"cc1"`, `"cc1plus"`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub component: Option<String>,
    /// Vendor of the tool (e.g. `"GNU"`, `"LLVM"`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vendor: Option<String>,
}

/// Top-level IR envelope for a single compiler invocation.
///
/// Contains producer metadata, run information, captured artifacts,
/// integrity issues, and the tree of [`DiagnosticNode`]s.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DiagnosticDocument {
    /// Unique document identifier.
    pub document_id: String,
    /// Semantic version of the IR schema this document conforms to.
    pub schema_version: String,
    /// How completely the document was produced.
    pub document_completeness: DocumentCompleteness,
    /// Information about the software that produced this document.
    pub producer: ProducerInfo,
    /// Metadata about the captured compiler invocation.
    pub run: RunInfo,
    /// Raw artifacts captured during the build (stderr, SARIF, etc.).
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub captures: Vec<CaptureArtifact>,
    /// Issues detected during document production (parse errors, etc.).
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub integrity_issues: Vec<IntegrityIssue>,
    /// Top-level diagnostic nodes extracted from compiler output.
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub diagnostics: Vec<DiagnosticNode>,
    /// Document-level fingerprints for drift detection.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fingerprints: Option<FingerprintSet>,
}

/// The message text of a diagnostic, with optional normalised form.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MessageText {
    /// Original message text as emitted by the compiler.
    pub raw_text: String,
    /// Normalised form with numbers replaced by `<n>`, etc.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub normalized_text: Option<String>,
    /// Locale/language of the message (e.g. `"C"`, `"en_US"`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub locale: Option<String>,
}

/// A single diagnostic extracted from compiler output.
///
/// Nodes form a tree: a root node may have child supporting notes,
/// candidates, and path events.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DiagnosticNode {
    /// Unique node identifier within the document.
    pub id: String,
    /// Which tool originally emitted this diagnostic.
    pub origin: Origin,
    /// Compiler phase that produced this diagnostic.
    pub phase: Phase,
    /// Severity level (error, warning, note, etc.).
    pub severity: Severity,
    /// Semantic role of this node in the diagnostic tree.
    pub semantic_role: SemanticRole,
    /// The diagnostic message text.
    pub message: MessageText,
    /// Source-code locations associated with this diagnostic.
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub locations: Vec<Location>,
    /// Child diagnostic nodes (supporting notes, candidates, etc.).
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub children: Vec<DiagnosticNode>,
    /// Suggested fixes for this diagnostic.
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub suggestions: Vec<Suggestion>,
    /// Chains of contextual frames (include stacks, template instantiations, etc.).
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub context_chains: Vec<ContextChain>,
    /// Symbol information related to this diagnostic (e.g. linker symbols).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub symbol_context: Option<SymbolContext>,
    /// How completely this node was parsed.
    pub node_completeness: NodeCompleteness,
    /// Data lineage back to the captured artifact.
    pub provenance: Provenance,
    /// Optional enrichment-stage analysis overlay.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub analysis: Option<AnalysisOverlay>,
    /// Node-level fingerprints for drift detection.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fingerprints: Option<FingerprintSet>,
}

impl DiagnosticNode {
    /// Returns the preferred primary location, respecting the analysis overlay override.
    ///
    /// Falls back to the first location with [`LocationRole::Primary`], then the first
    /// location in the list.
    pub fn primary_location(&self) -> Option<&Location> {
        if let Some(preferred_id) = self
            .analysis
            .as_ref()
            .and_then(|analysis| analysis.preferred_primary_location_id.as_deref())
            && let Some(location) = self
                .locations
                .iter()
                .find(|location| location.id == preferred_id)
        {
            return Some(location);
        }
        self.locations
            .iter()
            .find(|location| matches!(location.role, LocationRole::Primary))
            .or_else(|| self.locations.first())
    }
}
