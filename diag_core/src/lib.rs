use ordered_float::OrderedFloat;
use regex::Regex;
use semver::Version;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::{Map, Value};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, HashSet};
use std::fmt::{Display, Formatter};

pub const IR_SPEC_VERSION: &str = "1.0.0-alpha.1";
pub const ADAPTER_SPEC_VERSION: &str = "v1alpha";
pub const RENDERER_SPEC_VERSION: &str = "v1alpha";
pub type Score = OrderedFloat<f32>;
pub const CONFIDENCE_CERTAIN_THRESHOLD: f32 = 0.85;
pub const CONFIDENCE_LIKELY_THRESHOLD: f32 = 0.60;
pub const CONFIDENCE_POSSIBLE_THRESHOLD: f32 = 0.35;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SnapshotKind {
    FactsOnly,
    AnalysisIncluded,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DocumentCompleteness {
    Complete,
    Partial,
    Passthrough,
    Failed,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[serde(rename_all = "snake_case")]
pub enum FallbackReason {
    UnsupportedTier,
    IncompatibleSink,
    ShadowMode,
    SarifMissing,
    SarifParseFailed,
    ResidualOnly,
    RendererLowConfidence,
    InternalError,
    TimeoutOrBudget,
    UserOptOut,
}

impl FallbackReason {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::UnsupportedTier => "unsupported_tier",
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

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SourceAuthority {
    Structured,
    ResidualText,
    None,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum FallbackGrade {
    None,
    Compatibility,
    FailOpen,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WrapperSurface {
    Terminal,
    Ci,
    Editor,
    TraceOnly,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProducerInfo {
    pub name: String,
    pub version: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub git_revision: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub build_profile: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rulepack_version: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RunInfo {
    pub invocation_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub invoked_as: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub argv_redacted: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cwd_display: Option<String>,
    pub exit_status: i32,
    pub primary_tool: ToolInfo,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub secondary_tools: Vec<ToolInfo>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub language_mode: Option<LanguageMode>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_triple: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wrapper_mode: Option<WrapperSurface>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ToolInfo {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub component: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vendor: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LanguageMode {
    C,
    Cpp,
    Objc,
    Objcpp,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DiagnosticDocument {
    pub document_id: String,
    pub schema_version: String,
    pub document_completeness: DocumentCompleteness,
    pub producer: ProducerInfo,
    pub run: RunInfo,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub captures: Vec<CaptureArtifact>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub integrity_issues: Vec<IntegrityIssue>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub diagnostics: Vec<DiagnosticNode>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fingerprints: Option<FingerprintSet>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CaptureArtifact {
    pub id: String,
    pub kind: ArtifactKind,
    pub media_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub encoding: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub digest_sha256: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size_bytes: Option<u64>,
    pub storage: ArtifactStorage,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub inline_text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub external_ref: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub produced_by: Option<ToolInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ArtifactKind {
    GccSarif,
    GccJson,
    CompilerStderrText,
    LinkerStderrText,
    CompilerStdoutText,
    WrapperTrace,
    SourceSnippet,
    Other,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ArtifactStorage {
    Inline,
    ExternalRef,
    Unavailable,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct IntegrityIssue {
    pub severity: IssueSeverity,
    pub stage: IssueStage,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provenance: Option<Provenance>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum IssueSeverity {
    Error,
    Warning,
    Info,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum IssueStage {
    Capture,
    Parse,
    Normalize,
    Analyze,
    Render,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DiagnosticNode {
    pub id: String,
    pub origin: Origin,
    pub phase: Phase,
    pub severity: Severity,
    pub semantic_role: SemanticRole,
    pub message: MessageText,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub locations: Vec<Location>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub children: Vec<DiagnosticNode>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub suggestions: Vec<Suggestion>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub context_chains: Vec<ContextChain>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub symbol_context: Option<SymbolContext>,
    pub node_completeness: NodeCompleteness,
    pub provenance: Provenance,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub analysis: Option<AnalysisOverlay>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fingerprints: Option<FingerprintSet>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Origin {
    Gcc,
    Clang,
    Linker,
    Driver,
    Wrapper,
    ExternalTool,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Phase {
    Driver,
    Preprocess,
    Parse,
    Semantic,
    Instantiate,
    Constraints,
    Analyze,
    Optimize,
    Codegen,
    Assemble,
    Link,
    Archive,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    Fatal,
    Error,
    Warning,
    Note,
    Remark,
    Info,
    Debug,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SemanticRole {
    Root,
    Supporting,
    Help,
    Candidate,
    PathEvent,
    Summary,
    Passthrough,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MessageText {
    pub raw_text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub normalized_text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub locale: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Location {
    pub id: String,
    pub file: FileRef,
    pub anchor: Option<SourcePoint>,
    pub range: Option<SourceRange>,
    pub role: LocationRole,
    pub source_kind: LocationSourceKind,
    pub label: Option<String>,
    pub ownership_override: Option<OwnershipInfo>,
    pub provenance_override: Option<Provenance>,
    pub source_excerpt_ref: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FileRef {
    pub path_raw: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub uri: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path_style: Option<PathStyle>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path_kind: Option<PathKind>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ownership: Option<OwnershipInfo>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exists_at_capture: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct OwnershipInfo {
    pub owner: Ownership,
    pub reason: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub confidence: Option<Score>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SourcePoint {
    pub line: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub column_origin: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub column_byte: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub column_display: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub column_native: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub column_native_unit: Option<ColumnUnit>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SourceRange {
    pub start: SourcePoint,
    pub end: SourcePoint,
    pub boundary_semantics: BoundarySemantics,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LocationRole {
    Primary,
    Secondary,
    Related,
    Context,
    EditTarget,
    SymbolReference,
    SymbolDefinition,
    Other,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LocationSourceKind {
    Caret,
    Range,
    Token,
    Insertion,
    Expansion,
    Generated,
    Other,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PathStyle {
    Posix,
    Windows,
    Uri,
    Virtual,
    Unknown,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PathKind {
    Absolute,
    Relative,
    Virtual,
    Generated,
    Unknown,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ColumnUnit {
    Byte,
    Display,
    Utf16CodeUnit,
    UnicodeScalar,
    Unknown,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum BoundarySemantics {
    HalfOpen,
    InclusiveEnd,
    Point,
    Unknown,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Ownership {
    User,
    Vendor,
    System,
    Generated,
    Tool,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Suggestion {
    pub label: String,
    pub applicability: SuggestionApplicability,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub edits: Vec<TextEdit>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SuggestionApplicability {
    MachineApplicable,
    MaybeIncorrect,
    Manual,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TextEdit {
    pub path: String,
    pub start_line: u32,
    pub start_column: u32,
    pub end_line: u32,
    pub end_column: u32,
    pub replacement: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ContextChain {
    pub kind: ContextChainKind,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub frames: Vec<ContextFrame>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ContextChainKind {
    Include,
    MacroExpansion,
    TemplateInstantiation,
    LinkerResolution,
    AnalyzerPath,
    Other,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ContextFrame {
    pub label: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub column: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SymbolContext {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub primary_symbol: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub related_objects: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub archive: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum NodeCompleteness {
    Complete,
    Partial,
    Passthrough,
    Synthesized,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Provenance {
    pub source: ProvenanceSource,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub capture_refs: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ProvenanceSource {
    Compiler,
    Linker,
    WrapperGenerated,
    ResidualText,
    Policy,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AnalysisOverlay {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub family: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub family_version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub family_confidence: Option<Score>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub root_cause_score: Option<Score>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub actionability_score: Option<Score>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_code_priority: Option<Score>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub headline: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub first_action_hint: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(default, deserialize_with = "deserialize_confidence_score_opt")]
    pub confidence: Option<Score>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub preferred_primary_location_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rule_id: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub matched_conditions: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub suppression_reason: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub collapsed_child_ids: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub collapsed_chain_ids: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub group_ref: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub reasons: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub policy_profile: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub producer_version: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum Confidence {
    High,
    Medium,
    Low,
    Unknown,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DisclosureConfidence {
    Certain,
    Likely,
    Possible,
    Hidden,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FingerprintSet {
    pub raw: String,
    pub structural: String,
    pub family: String,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("document validation failed")]
pub struct ValidationErrors {
    pub errors: Vec<String>,
}

impl Confidence {
    pub fn score(self) -> Score {
        OrderedFloat(match self {
            Self::High => 0.9,
            Self::Medium => 0.65,
            Self::Low => 0.35,
            Self::Unknown => 0.0,
        })
    }

    pub fn from_score(score: Option<Score>) -> Self {
        match DisclosureConfidence::from_score(score) {
            DisclosureConfidence::Certain => Self::High,
            DisclosureConfidence::Likely => Self::Medium,
            DisclosureConfidence::Possible => Self::Low,
            DisclosureConfidence::Hidden => Self::Unknown,
        }
    }
}

impl DisclosureConfidence {
    pub fn from_score(score: Option<Score>) -> Self {
        let Some(score) = score else {
            return Self::Hidden;
        };
        let score = score.into_inner();
        if score >= CONFIDENCE_CERTAIN_THRESHOLD {
            Self::Certain
        } else if score >= CONFIDENCE_LIKELY_THRESHOLD {
            Self::Likely
        } else if score >= CONFIDENCE_POSSIBLE_THRESHOLD {
            Self::Possible
        } else {
            Self::Hidden
        }
    }

    pub fn allows_analysis_title(self) -> bool {
        matches!(self, Self::Certain | Self::Likely)
    }

    pub fn allows_first_action(self) -> bool {
        matches!(self, Self::Certain | Self::Likely)
    }

    pub fn requires_low_confidence_notice(self) -> bool {
        matches!(self, Self::Possible | Self::Hidden)
    }
}

impl OwnershipInfo {
    pub fn new(owner: Ownership, reason: impl Into<String>) -> Self {
        Self {
            owner,
            reason: reason.into(),
            confidence: None,
        }
    }
}

impl FileRef {
    pub fn new(path_raw: impl Into<String>) -> Self {
        let path_raw = path_raw.into();
        let (path_style, path_kind) = infer_path_metadata(&path_raw);
        Self {
            path_raw,
            display_path: None,
            uri: None,
            path_style: Some(path_style),
            path_kind: Some(path_kind),
            ownership: None,
            exists_at_capture: None,
        }
    }
}

impl SourcePoint {
    pub fn new(line: u32, column: u32) -> Self {
        Self {
            line,
            column_origin: Some(1),
            column_byte: None,
            column_display: Some(column),
            column_native: Some(column),
            column_native_unit: Some(ColumnUnit::Display),
        }
    }
}

impl Location {
    pub fn caret(path: impl Into<String>, line: u32, column: u32, role: LocationRole) -> Self {
        let path = path.into();
        let anchor = SourcePoint::new(line, column);
        Self {
            id: synthetic_location_id(&path, &anchor, None),
            file: FileRef::new(path),
            anchor: Some(anchor),
            range: None,
            role,
            source_kind: LocationSourceKind::Caret,
            label: None,
            ownership_override: None,
            provenance_override: None,
            source_excerpt_ref: None,
        }
    }

    pub fn with_range_end(
        mut self,
        end_line: u32,
        end_column: u32,
        boundary_semantics: BoundarySemantics,
    ) -> Self {
        let start = self
            .anchor
            .clone()
            .unwrap_or_else(|| SourcePoint::new(end_line, end_column));
        let end = SourcePoint::new(end_line, end_column);
        self.id = synthetic_location_id(&self.file.path_raw, &start, Some(&end));
        self.range = Some(SourceRange {
            start,
            end,
            boundary_semantics,
        });
        self.source_kind = LocationSourceKind::Range;
        self
    }

    pub fn with_display_path(mut self, display_path: impl Into<String>) -> Self {
        self.file.display_path = Some(display_path.into());
        self
    }

    pub fn with_ownership(mut self, owner: Ownership, reason: impl Into<String>) -> Self {
        self.file.ownership = Some(OwnershipInfo::new(owner, reason));
        self
    }

    pub fn set_path_raw(&mut self, path: impl Into<String>) {
        self.file.path_raw = path.into();
        let start = self
            .anchor
            .as_ref()
            .or_else(|| self.range.as_ref().map(|range| &range.start));
        let end = self.range.as_ref().map(|range| &range.end);
        if let Some(start) = start {
            self.id = synthetic_location_id(&self.file.path_raw, start, end);
        }
    }

    pub fn set_anchor(&mut self, line: u32, column: u32) {
        let anchor = SourcePoint::new(line, column);
        self.anchor = Some(anchor.clone());
        if let Some(range) = self.range.as_mut() {
            range.start = anchor.clone();
        }
        self.id = synthetic_location_id(
            &self.file.path_raw,
            &anchor,
            self.range.as_ref().map(|range| &range.end),
        );
    }

    pub fn set_ownership(&mut self, owner: Ownership, reason: impl Into<String>) {
        self.file.ownership = Some(OwnershipInfo::new(owner, reason));
    }

    pub fn path_raw(&self) -> &str {
        &self.file.path_raw
    }

    pub fn display_path(&self) -> &str {
        self.file
            .display_path
            .as_deref()
            .unwrap_or(&self.file.path_raw)
    }

    pub fn line(&self) -> u32 {
        self.anchor
            .as_ref()
            .map(|point| point.line)
            .or_else(|| self.range.as_ref().map(|range| range.start.line))
            .unwrap_or(1)
    }

    pub fn column(&self) -> u32 {
        self.anchor
            .as_ref()
            .and_then(source_point_column)
            .or_else(|| {
                self.range
                    .as_ref()
                    .and_then(|range| source_point_column(&range.start))
            })
            .unwrap_or(1)
    }

    pub fn end_line(&self) -> Option<u32> {
        self.range.as_ref().map(|range| range.end.line)
    }

    pub fn end_column(&self) -> Option<u32> {
        self.range
            .as_ref()
            .and_then(|range| source_point_column(&range.end))
    }

    pub fn ownership(&self) -> Option<&Ownership> {
        self.ownership_override
            .as_ref()
            .map(|info| &info.owner)
            .or_else(|| self.file.ownership.as_ref().map(|info| &info.owner))
    }
}

impl AnalysisOverlay {
    pub fn set_confidence_score(&mut self, score: f32) {
        self.confidence = Some(OrderedFloat(score));
    }

    pub fn set_confidence_bucket(&mut self, confidence: Confidence) {
        self.confidence = Some(confidence.score());
    }

    pub fn confidence_score(&self) -> Option<Score> {
        self.confidence
    }

    pub fn confidence_bucket(&self) -> Option<Confidence> {
        self.confidence
            .map(|score| Confidence::from_score(Some(score)))
    }

    pub fn disclosure_confidence(&self) -> DisclosureConfidence {
        DisclosureConfidence::from_score(self.confidence)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct LocationCurrent {
    pub id: String,
    pub file: FileRef,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub anchor: Option<SourcePoint>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub range: Option<SourceRange>,
    pub role: LocationRole,
    pub source_kind: LocationSourceKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ownership_override: Option<OwnershipInfo>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provenance_override: Option<Provenance>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_excerpt_ref: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct LocationLegacy {
    pub path: String,
    pub line: u32,
    pub column: u32,
    #[serde(default)]
    pub end_line: Option<u32>,
    #[serde(default)]
    pub end_column: Option<u32>,
    #[serde(default)]
    pub display_path: Option<String>,
    #[serde(default)]
    pub ownership: Option<Ownership>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
enum LocationWire {
    Current(Box<LocationCurrent>),
    Legacy(LocationLegacy),
}

impl From<Location> for LocationCurrent {
    fn from(location: Location) -> Self {
        Self {
            id: location.id,
            file: location.file,
            anchor: location.anchor,
            range: location.range,
            role: location.role,
            source_kind: location.source_kind,
            label: location.label,
            ownership_override: location.ownership_override,
            provenance_override: location.provenance_override,
            source_excerpt_ref: location.source_excerpt_ref,
        }
    }
}

impl From<LocationCurrent> for Location {
    fn from(location: LocationCurrent) -> Self {
        Self {
            id: location.id,
            file: location.file,
            anchor: location.anchor,
            range: location.range,
            role: location.role,
            source_kind: location.source_kind,
            label: location.label,
            ownership_override: location.ownership_override,
            provenance_override: location.provenance_override,
            source_excerpt_ref: location.source_excerpt_ref,
        }
    }
}

impl From<LocationLegacy> for Location {
    fn from(location: LocationLegacy) -> Self {
        let mut converted = Location::caret(
            location.path,
            location.line,
            location.column,
            LocationRole::Primary,
        );
        if let Some(display_path) = location.display_path {
            converted = converted.with_display_path(display_path);
        }
        if let Some(owner) = location.ownership {
            converted = converted.with_ownership(owner, ownership_reason_key(owner));
        }
        if let (Some(end_line), Some(end_column)) = (location.end_line, location.end_column) {
            converted = converted.with_range_end(end_line, end_column, BoundarySemantics::Unknown);
        }
        converted
    }
}

impl From<LocationWire> for Location {
    fn from(location: LocationWire) -> Self {
        match location {
            LocationWire::Current(location) => (*location).into(),
            LocationWire::Legacy(location) => location.into(),
        }
    }
}

impl Serialize for Location {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        LocationCurrent::from(self.clone()).serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for Location {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let wire = LocationWire::deserialize(deserializer)?;
        Ok(wire.into())
    }
}

fn deserialize_confidence_score_opt<'de, D>(deserializer: D) -> Result<Option<Score>, D::Error>
where
    D: Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum ConfidenceWire {
        Score(f32),
        Bucket(Confidence),
    }

    let confidence = Option::<ConfidenceWire>::deserialize(deserializer)?;
    Ok(confidence.map(|confidence| match confidence {
        ConfidenceWire::Score(score) => OrderedFloat(score),
        ConfidenceWire::Bucket(bucket) => bucket.score(),
    }))
}

fn ownership_reason_key(owner: Ownership) -> &'static str {
    match owner {
        Ownership::User => "user_workspace",
        Ownership::Vendor => "vendor_path",
        Ownership::System => "system_path",
        Ownership::Generated => "generated_path",
        Ownership::Tool => "tool_generated",
        Ownership::Unknown => "unknown",
    }
}

fn source_point_column(point: &SourcePoint) -> Option<u32> {
    point
        .column_display
        .or(point.column_native)
        .or(point.column_byte)
}

fn infer_path_metadata(path: &str) -> (PathStyle, PathKind) {
    if path.starts_with("file://") {
        return (PathStyle::Uri, PathKind::Absolute);
    }
    if path.starts_with('/') {
        return (PathStyle::Posix, PathKind::Absolute);
    }
    if path.contains(":\\") {
        return (PathStyle::Windows, PathKind::Absolute);
    }
    if path.starts_with('<') && path.ends_with('>') {
        return (PathStyle::Virtual, PathKind::Virtual);
    }
    (PathStyle::Posix, PathKind::Relative)
}

fn synthetic_location_id(path: &str, start: &SourcePoint, end: Option<&SourcePoint>) -> String {
    let end = end.unwrap_or(start);
    format!(
        "loc:{}:{}:{}:{}:{}",
        path,
        start.line,
        source_point_column(start).unwrap_or(1),
        end.line,
        source_point_column(end).unwrap_or(source_point_column(start).unwrap_or(1))
    )
}

impl DiagnosticDocument {
    pub fn validate(&self) -> Result<(), ValidationErrors> {
        let mut errors = Vec::new();
        let mut capture_ids = HashSet::new();
        let mut node_ids = HashSet::new();

        if self.document_id.trim().is_empty() {
            errors.push("document_id must be non-empty".to_string());
        }
        if self.schema_version.trim().is_empty() {
            errors.push("schema_version must be non-empty".to_string());
        } else if Version::parse(self.schema_version.trim()).is_err() {
            errors.push(format!(
                "schema_version {} must be parseable semver",
                self.schema_version
            ));
        }
        if self.diagnostics.is_empty()
            && !matches!(
                self.document_completeness,
                DocumentCompleteness::Failed | DocumentCompleteness::Passthrough
            )
        {
            errors.push(
                "diagnostics may be empty only for failed or passthrough documents".to_string(),
            );
        }
        for capture in &self.captures {
            if !capture_ids.insert(capture.id.clone()) {
                errors.push(format!("duplicate capture id: {}", capture.id));
            }
            if matches!(capture.storage, ArtifactStorage::Inline) && capture.inline_text.is_none() {
                errors.push(format!("inline capture {} missing inline_text", capture.id));
            }
            if matches!(capture.storage, ArtifactStorage::ExternalRef)
                && capture.external_ref.is_none()
            {
                errors.push(format!(
                    "external_ref capture {} missing external_ref",
                    capture.id
                ));
            }
        }
        for (index, issue) in self.integrity_issues.iter().enumerate() {
            validate_integrity_issue(issue, index, &capture_ids, &mut errors);
        }
        for node in &self.diagnostics {
            validate_node(node, &capture_ids, &mut node_ids, &mut errors, true);
        }
        if errors.is_empty() {
            Ok(())
        } else {
            Err(ValidationErrors { errors })
        }
    }

    pub fn refresh_fingerprints(&mut self) {
        for node in &mut self.diagnostics {
            refresh_node_fingerprints(node);
        }
        self.fingerprints = None;
        self.fingerprints = Some(FingerprintSet {
            raw: fingerprint_for(&self.diagnostics),
            structural: fingerprint_for(&canonical_snapshot_value(self)),
            family: fingerprint_for(
                &self
                    .diagnostics
                    .iter()
                    .map(|node| {
                        node.analysis
                            .as_ref()
                            .and_then(|analysis| analysis.family.clone())
                            .unwrap_or_else(|| "unknown".to_string())
                    })
                    .collect::<Vec<_>>(),
            ),
        });
    }

    pub fn canonical_json(&self) -> Result<String, serde_json::Error> {
        canonical_json(self)
    }
}

impl DiagnosticNode {
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

fn validate_node(
    node: &DiagnosticNode,
    capture_ids: &HashSet<String>,
    node_ids: &mut HashSet<String>,
    errors: &mut Vec<String>,
    top_level: bool,
) {
    if !node_ids.insert(node.id.clone()) {
        errors.push(format!("duplicate node id: {}", node.id));
    }
    validate_provenance(
        &format!("node {} provenance", node.id),
        &node.provenance,
        capture_ids,
        errors,
    );
    if node.message.raw_text.trim().is_empty() {
        errors.push(format!("node {} missing raw_text", node.id));
    }
    if matches!(node.node_completeness, NodeCompleteness::Passthrough)
        && node.provenance.capture_refs.is_empty()
    {
        errors.push(format!(
            "node {} is passthrough but provenance.capture_refs is empty",
            node.id
        ));
    }
    if top_level
        && !matches!(
            node.semantic_role,
            SemanticRole::Root | SemanticRole::Summary | SemanticRole::Passthrough
        )
    {
        errors.push(format!(
            "top-level node {} must be root, summary, or passthrough",
            node.id
        ));
    }
    for child in &node.children {
        if matches!(child.semantic_role, SemanticRole::Root) {
            errors.push(format!(
                "child node {} must not have semantic_role=root",
                child.id
            ));
        }
        validate_node(child, capture_ids, node_ids, errors, false);
    }
    if matches!(node.node_completeness, NodeCompleteness::Synthesized)
        && !matches!(
            node.provenance.source,
            ProvenanceSource::WrapperGenerated | ProvenanceSource::Policy
        )
    {
        errors.push(format!(
            "node {} is synthesized but provenance.source is not wrapper_generated or policy",
            node.id
        ));
    }
    if matches!(
        node.phase,
        Phase::Parse | Phase::Semantic | Phase::Instantiate
    ) && node.locations.is_empty()
        && matches!(node.node_completeness, NodeCompleteness::Complete)
    {
        errors.push(format!(
            "node {} is complete in parse/semantic/instantiate phase but has no locations",
            node.id
        ));
    }
    let child_ids = descendant_node_ids(node);
    if let Some(analysis) = node.analysis.as_ref() {
        for (label, score) in [
            ("family_confidence", analysis.family_confidence),
            ("root_cause_score", analysis.root_cause_score),
            ("actionability_score", analysis.actionability_score),
            ("user_code_priority", analysis.user_code_priority),
            ("confidence", analysis.confidence),
        ] {
            if let Some(score) = score
                && !(0.0..=1.0).contains(&score.into_inner())
            {
                errors.push(format!(
                    "node {} analysis {} must be within 0.0..=1.0",
                    node.id, label
                ));
            }
        }
        if let Some(preferred_id) = analysis.preferred_primary_location_id.as_deref()
            && !node
                .locations
                .iter()
                .any(|location| location.id == preferred_id)
        {
            errors.push(format!(
                "node {} preferred_primary_location_id {} does not exist",
                node.id, preferred_id
            ));
        }
        for child_id in &analysis.collapsed_child_ids {
            if !child_ids.contains(child_id) {
                errors.push(format!(
                    "node {} collapsed_child_id {} does not reference a descendant",
                    node.id, child_id
                ));
            }
        }
    }
    for location in &node.locations {
        validate_location(node, location, capture_ids, errors);
    }
}

fn validate_integrity_issue(
    issue: &IntegrityIssue,
    index: usize,
    capture_ids: &HashSet<String>,
    errors: &mut Vec<String>,
) {
    if let Some(provenance) = issue.provenance.as_ref() {
        validate_provenance(
            &format!("integrity_issue[{index}] provenance"),
            provenance,
            capture_ids,
            errors,
        );
    }
}

fn validate_location(
    node: &DiagnosticNode,
    location: &Location,
    capture_ids: &HashSet<String>,
    errors: &mut Vec<String>,
) {
    if location.anchor.is_none() && location.range.is_none() {
        errors.push(format!(
            "node {} location {} must have anchor or range",
            node.id, location.id
        ));
    }
    if let Some(anchor) = location.anchor.as_ref()
        && anchor.line < 1
    {
        errors.push(format!(
            "node {} location {} anchor line must be >= 1",
            node.id, location.id
        ));
    }
    if let Some(range) = location.range.as_ref() {
        if range.start.line < 1 {
            errors.push(format!(
                "node {} location {} range.start line must be >= 1",
                node.id, location.id
            ));
        }
        if range.end.line < 1 {
            errors.push(format!(
                "node {} location {} range.end line must be >= 1",
                node.id, location.id
            ));
        }
    }
    if let Some(provenance) = location.provenance_override.as_ref() {
        validate_provenance(
            &format!(
                "node {} location {} provenance_override",
                node.id, location.id
            ),
            provenance,
            capture_ids,
            errors,
        );
    }
}

fn validate_provenance(
    scope: &str,
    provenance: &Provenance,
    capture_ids: &HashSet<String>,
    errors: &mut Vec<String>,
) {
    for capture_ref in &provenance.capture_refs {
        if !capture_ids.contains(capture_ref) {
            errors.push(format!(
                "{scope} references missing capture {}",
                capture_ref
            ));
        }
    }
}

fn descendant_node_ids(node: &DiagnosticNode) -> HashSet<String> {
    let mut ids = HashSet::new();
    collect_descendant_node_ids(node, &mut ids);
    ids
}

fn collect_descendant_node_ids(node: &DiagnosticNode, ids: &mut HashSet<String>) {
    for child in &node.children {
        ids.insert(child.id.clone());
        collect_descendant_node_ids(child, ids);
    }
}

fn refresh_node_fingerprints(node: &mut DiagnosticNode) {
    for child in &mut node.children {
        refresh_node_fingerprints(child);
    }
    node.fingerprints = None;
    let family_seed = node
        .analysis
        .as_ref()
        .and_then(|analysis| analysis.family.as_deref())
        .unwrap_or("unknown");
    node.fingerprints = Some(FingerprintSet {
        raw: fingerprint_for(&node.message.raw_text),
        structural: fingerprint_for(&canonical_snapshot_value(node)),
        family: fingerprint_for(&format!(
            "{}:{}:{}:{}",
            family_seed,
            normalize_message(&node.message.raw_text),
            node.phase,
            node.primary_location()
                .and_then(Location::ownership)
                .map(Ownership::to_string)
                .unwrap_or_else(|| "unknown".to_string())
        )),
    });
}

pub fn canonical_json<T: Serialize>(value: &T) -> Result<String, serde_json::Error> {
    let value = canonical_snapshot_value(value);
    serde_json::to_string_pretty(&value)
}

pub fn canonical_snapshot_value<T: Serialize>(value: &T) -> Value {
    match serde_json::to_value(value) {
        Ok(value) => sort_value(value),
        Err(error) => Value::String(format!("serialization_error:{error}")),
    }
}

pub fn normalize_for_snapshot(document: &DiagnosticDocument) -> DiagnosticDocument {
    normalize_for_snapshot_kind(document, SnapshotKind::AnalysisIncluded)
}

pub fn normalize_for_snapshot_kind(
    document: &DiagnosticDocument,
    kind: SnapshotKind,
) -> DiagnosticDocument {
    let mut copy = document.clone();
    copy.document_id = "<document>".to_string();
    copy.schema_version = IR_SPEC_VERSION.to_string();
    copy.producer.version = "<normalized>".to_string();
    copy.producer.git_revision = None;
    copy.producer.build_profile = None;
    copy.run.invocation_id = "<invocation>".to_string();
    if let Some(cwd) = copy.run.cwd_display.as_mut() {
        *cwd = "<cwd>".to_string();
    }
    copy.run.primary_tool.version = None;
    for tool in &mut copy.run.secondary_tools {
        tool.version = None;
    }
    for capture in &mut copy.captures {
        if capture.external_ref.is_some() {
            capture.external_ref = Some(format!("<capture:{}>", capture.id));
        }
        capture.digest_sha256 = None;
        if let Some(tool) = capture.produced_by.as_mut() {
            tool.version = None;
        }
    }
    if matches!(kind, SnapshotKind::FactsOnly) {
        for diagnostic in &mut copy.diagnostics {
            strip_analysis(diagnostic);
        }
    }
    copy.refresh_fingerprints();
    copy
}

pub fn snapshot_json(
    document: &DiagnosticDocument,
    kind: SnapshotKind,
) -> Result<String, serde_json::Error> {
    canonical_json(&normalize_for_snapshot_kind(document, kind))
}

pub fn normalize_message(message: &str) -> String {
    let number_re = Regex::new(r"\d+").expect("compile-time regex");
    number_re.replace_all(message, "<n>").into_owned()
}

pub fn fingerprint_for<T: Serialize>(value: &T) -> String {
    let canonical = canonical_snapshot_value(value);
    let payload = serde_json::to_vec(&canonical).unwrap_or_default();
    let mut hasher = Sha256::new();
    hasher.update(payload);
    format!("{:x}", hasher.finalize())
}

fn strip_analysis(node: &mut DiagnosticNode) {
    node.analysis = None;
    for child in &mut node.children {
        strip_analysis(child);
    }
}

fn sort_value(value: Value) -> Value {
    match value {
        Value::Array(values) => Value::Array(values.into_iter().map(sort_value).collect()),
        Value::Object(object) => {
            let mut sorted = BTreeMap::new();
            for (key, value) in object {
                sorted.insert(key, sort_value(value));
            }
            let mut result = Map::new();
            for (key, value) in sorted {
                result.insert(key, value);
            }
            Value::Object(result)
        }
        other => other,
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_document() -> DiagnosticDocument {
        DiagnosticDocument {
            document_id: "doc-1".to_string(),
            schema_version: IR_SPEC_VERSION.to_string(),
            document_completeness: DocumentCompleteness::Complete,
            producer: ProducerInfo {
                name: "gcc-formed".to_string(),
                version: "0.1.0".to_string(),
                git_revision: None,
                build_profile: Some("test".to_string()),
                rulepack_version: None,
            },
            run: RunInfo {
                invocation_id: "inv-1".to_string(),
                invoked_as: Some("gcc-formed".to_string()),
                argv_redacted: vec!["gcc".to_string(), "-c".to_string(), "main.c".to_string()],
                cwd_display: Some("/tmp/project".to_string()),
                exit_status: 1,
                primary_tool: ToolInfo {
                    name: "gcc".to_string(),
                    version: Some("15.1.0".to_string()),
                    component: None,
                    vendor: Some("GNU".to_string()),
                },
                secondary_tools: Vec::new(),
                language_mode: Some(LanguageMode::C),
                target_triple: Some("x86_64-unknown-linux-gnu".to_string()),
                wrapper_mode: Some(WrapperSurface::Terminal),
            },
            captures: vec![CaptureArtifact {
                id: "stderr.raw".to_string(),
                kind: ArtifactKind::CompilerStderrText,
                media_type: "text/plain".to_string(),
                encoding: Some("utf-8".to_string()),
                digest_sha256: Some("deadbeef".to_string()),
                size_bytes: Some(12),
                storage: ArtifactStorage::Inline,
                inline_text: Some("main.c:1:1".to_string()),
                external_ref: None,
                produced_by: None,
            }],
            integrity_issues: Vec::new(),
            diagnostics: vec![DiagnosticNode {
                id: "root-1".to_string(),
                origin: Origin::Gcc,
                phase: Phase::Parse,
                severity: Severity::Error,
                semantic_role: SemanticRole::Root,
                message: MessageText {
                    raw_text: "expected ';' before '}' token".to_string(),
                    normalized_text: None,
                    locale: Some("C".to_string()),
                },
                locations: vec![
                    Location::caret("src/main.c", 4, 1, LocationRole::Primary)
                        .with_ownership(Ownership::User, ownership_reason_key(Ownership::User)),
                ],
                children: Vec::new(),
                suggestions: Vec::new(),
                context_chains: Vec::new(),
                symbol_context: None,
                node_completeness: NodeCompleteness::Complete,
                provenance: Provenance {
                    source: ProvenanceSource::Compiler,
                    capture_refs: vec!["stderr.raw".to_string()],
                },
                analysis: Some(AnalysisOverlay {
                    family: Some("syntax".to_string()),
                    family_version: None,
                    family_confidence: None,
                    root_cause_score: None,
                    actionability_score: None,
                    user_code_priority: None,
                    headline: Some("syntax error".to_string()),
                    first_action_hint: Some("insert the missing semicolon".to_string()),
                    confidence: Some(Confidence::High.score()),
                    preferred_primary_location_id: Some("loc:src/main.c:4:1:4:1".to_string()),
                    rule_id: Some("rule.syntax.expected_or_before".to_string()),
                    matched_conditions: vec!["message_contains=expected".to_string()],
                    suppression_reason: None,
                    collapsed_child_ids: Vec::new(),
                    collapsed_chain_ids: Vec::new(),
                    group_ref: None,
                    reasons: Vec::new(),
                    policy_profile: None,
                    producer_version: None,
                }),
                fingerprints: None,
            }],
            fingerprints: None,
        }
    }

    #[test]
    fn validates_and_fingerprints_document() {
        let mut document = sample_document();
        assert!(document.validate().is_ok());
        document.refresh_fingerprints();
        assert!(document.fingerprints.is_some());
        assert!(document.diagnostics[0].fingerprints.is_some());
    }

    #[test]
    fn canonical_json_is_deterministic() {
        let document = sample_document();
        let left = document.canonical_json().unwrap();
        let right = document.canonical_json().unwrap();
        assert_eq!(left, right);
    }

    #[test]
    fn snapshot_variants_are_deterministic() {
        let mut document = sample_document();
        document.refresh_fingerprints();

        let facts_left = snapshot_json(&document, SnapshotKind::FactsOnly).unwrap();
        let facts_right = snapshot_json(&document, SnapshotKind::FactsOnly).unwrap();
        let analysis = snapshot_json(&document, SnapshotKind::AnalysisIncluded).unwrap();

        assert_eq!(facts_left, facts_right);
        assert!(facts_left.contains("<document>"));
        assert!(!facts_left.contains("syntax error"));
        assert!(analysis.contains("syntax error"));
    }

    #[test]
    fn rejects_duplicate_node_ids() {
        let mut document = sample_document();
        let duplicate = document.diagnostics[0].clone();
        document.diagnostics.push(duplicate);
        let errors = document.validate().unwrap_err();
        assert!(
            errors
                .errors
                .iter()
                .any(|error| error.contains("duplicate node id"))
        );
    }

    #[test]
    fn prefers_analysis_primary_location_id() {
        let mut document = sample_document();
        document.diagnostics[0].locations.push(Location::caret(
            "src/secondary.c",
            8,
            3,
            LocationRole::Primary,
        ));
        document.diagnostics[0]
            .analysis
            .as_mut()
            .unwrap()
            .preferred_primary_location_id = Some("loc:src/secondary.c:8:3:8:3".to_string());

        let location = document.diagnostics[0].primary_location().unwrap();

        assert_eq!(location.path_raw(), "src/secondary.c");
    }

    #[test]
    fn rejects_missing_preferred_primary_location() {
        let mut document = sample_document();
        document.diagnostics[0]
            .analysis
            .as_mut()
            .unwrap()
            .preferred_primary_location_id = Some("missing".to_string());

        let errors = document.validate().unwrap_err();

        assert!(
            errors
                .errors
                .iter()
                .any(|error| error.contains("preferred_primary_location_id"))
        );
    }

    #[test]
    fn rejects_unparseable_schema_version() {
        let mut document = sample_document();
        document.schema_version = "v1alpha".to_string();

        let errors = document.validate().unwrap_err();

        assert!(
            errors
                .errors
                .iter()
                .any(|error| error.contains("schema_version v1alpha must be parseable semver"))
        );
    }

    #[test]
    fn rejects_missing_capture_refs_across_document_scopes() {
        let mut document = sample_document();
        document.diagnostics[0].provenance.capture_refs = vec!["missing-node".to_string()];
        document.diagnostics[0].locations[0].provenance_override = Some(Provenance {
            source: ProvenanceSource::Policy,
            capture_refs: vec!["missing-location".to_string()],
        });
        document.integrity_issues.push(IntegrityIssue {
            severity: IssueSeverity::Warning,
            stage: IssueStage::Normalize,
            message: "capture drift".to_string(),
            provenance: Some(Provenance {
                source: ProvenanceSource::ResidualText,
                capture_refs: vec!["missing-issue".to_string()],
            }),
        });

        let errors = document.validate().unwrap_err();

        assert!(errors.errors.iter().any(|error| {
            error.contains("node root-1 provenance references missing capture missing-node")
        }));
        assert!(
            errors
                .errors
                .iter()
                .any(|error| error.contains("node root-1 location loc:src/main.c:4:1:4:1 provenance_override references missing capture missing-location"))
        );
        assert!(errors.errors.iter().any(|error| {
            error.contains("integrity_issue[0] provenance references missing capture missing-issue")
        }));
    }

    #[test]
    fn rejects_invalid_location_integrity() {
        let mut document = sample_document();
        document.diagnostics[0].locations[0]
            .anchor
            .as_mut()
            .unwrap()
            .line = 0;
        document.diagnostics[0].locations.push(Location {
            id: "loc:missing".to_string(),
            file: FileRef::new("src/missing.c"),
            anchor: None,
            range: None,
            role: LocationRole::Secondary,
            source_kind: LocationSourceKind::Other,
            label: None,
            ownership_override: None,
            provenance_override: None,
            source_excerpt_ref: None,
        });

        let errors = document.validate().unwrap_err();

        assert!(
            errors
                .errors
                .iter()
                .any(|error| error.contains("anchor line must be >= 1"))
        );
        assert!(
            errors
                .errors
                .iter()
                .any(|error| error.contains("must have anchor or range"))
        );
    }

    #[test]
    fn rejects_synthesized_nodes_with_non_wrapper_provenance() {
        let mut document = sample_document();
        document.diagnostics[0].node_completeness = NodeCompleteness::Synthesized;
        document.diagnostics[0].provenance.source = ProvenanceSource::Compiler;

        let errors = document.validate().unwrap_err();

        assert!(
            errors.errors.iter().any(|error| error.contains(
                "is synthesized but provenance.source is not wrapper_generated or policy"
            ))
        );
    }

    #[test]
    fn rejects_collapsed_child_ids_that_are_not_descendants() {
        let mut document = sample_document();
        document.diagnostics[0]
            .analysis
            .as_mut()
            .unwrap()
            .collapsed_child_ids = vec!["missing-child".to_string()];

        let errors = document.validate().unwrap_err();

        assert!(errors.errors.iter().any(|error| {
            error.contains("collapsed_child_id missing-child does not reference a descendant")
        }));
    }

    #[test]
    fn confidence_thresholds_follow_renderer_contract() {
        assert_eq!(
            DisclosureConfidence::from_score(Some(OrderedFloat(0.85))),
            DisclosureConfidence::Certain
        );
        assert_eq!(
            DisclosureConfidence::from_score(Some(OrderedFloat(0.84))),
            DisclosureConfidence::Likely
        );
        assert_eq!(
            DisclosureConfidence::from_score(Some(OrderedFloat(0.60))),
            DisclosureConfidence::Likely
        );
        assert_eq!(
            DisclosureConfidence::from_score(Some(OrderedFloat(0.59))),
            DisclosureConfidence::Possible
        );
        assert_eq!(
            DisclosureConfidence::from_score(Some(OrderedFloat(0.35))),
            DisclosureConfidence::Possible
        );
        assert_eq!(
            DisclosureConfidence::from_score(Some(OrderedFloat(0.34))),
            DisclosureConfidence::Hidden
        );
        assert_eq!(
            Confidence::from_score(Some(OrderedFloat(0.84))),
            Confidence::Medium
        );
        assert_eq!(
            Confidence::from_score(Some(OrderedFloat(0.34))),
            Confidence::Unknown
        );
    }
}
