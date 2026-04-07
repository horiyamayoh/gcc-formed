use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, HashSet};
use std::fmt::{Display, Formatter};

pub const IR_SPEC_VERSION: &str = "1.0.0-alpha.1";
pub const ADAPTER_SPEC_VERSION: &str = "v1alpha";
pub const RENDERER_SPEC_VERSION: &str = "v1alpha";

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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Location {
    pub path: String,
    pub line: u32,
    pub column: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end_line: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end_column: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ownership: Option<Ownership>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Ownership {
    User,
    Vendor,
    System,
    Generated,
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
    pub headline: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub first_action_hint: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub confidence: Option<Confidence>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub collapsed_child_ids: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub collapsed_chain_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum Confidence {
    High,
    Medium,
    Low,
    Unknown,
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
        for node in &self.diagnostics {
            validate_node(node, &mut node_ids, &mut errors, true);
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
        self.locations.first()
    }
}

fn validate_node(
    node: &DiagnosticNode,
    node_ids: &mut HashSet<String>,
    errors: &mut Vec<String>,
    top_level: bool,
) {
    if !node_ids.insert(node.id.clone()) {
        errors.push(format!("duplicate node id: {}", node.id));
    }
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
        validate_node(child, node_ids, errors, false);
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
                .and_then(|location| location.ownership.as_ref())
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
                locations: vec![Location {
                    path: "src/main.c".to_string(),
                    line: 4,
                    column: 1,
                    end_line: None,
                    end_column: None,
                    display_path: None,
                    ownership: Some(Ownership::User),
                }],
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
                    headline: Some("syntax error".to_string()),
                    first_action_hint: Some("insert the missing semicolon".to_string()),
                    confidence: Some(Confidence::High),
                    collapsed_child_ids: Vec::new(),
                    collapsed_chain_ids: Vec::new(),
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
}
