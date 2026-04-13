use diag_backend_probe::{ProcessingPath, SupportLevel, VersionBand, capability_profile_for_major};
use diag_core::{
    Confidence, DiagnosticDocument, DiagnosticNode, FallbackGrade, FallbackReason, Ownership,
    Suggestion,
};
use serde::{Deserialize, Serialize};

pub const PUBLIC_EXPORT_KIND: &str = "gcc_formed_public_diagnostic_export";
pub const PUBLIC_EXPORT_SCHEMA_VERSION: &str = "2.0.0-alpha.1";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PublicDiagnosticExport {
    pub schema_version: String,
    pub kind: String,
    pub status: PublicExportStatus,
    pub producer: PublicExportProducer,
    pub invocation: PublicExportInvocation,
    pub execution: PublicExportExecution,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<PublicDiagnosticResult>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unavailable_reason: Option<PublicExportUnavailableReason>,
}

impl PublicDiagnosticExport {
    pub fn canonical_json(&self) -> Result<String, serde_json::Error> {
        diag_core::canonical_json(self)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PublicExportStatus {
    Available,
    Unavailable,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PublicExportUnavailableReason {
    IntrospectionLike,
    PassthroughMode,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PublicExportProducer {
    pub name: String,
    pub version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PublicExportInvocation {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub invocation_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub invoked_as: Option<String>,
    pub exit_status: i32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub primary_tool: Option<PublicExportTool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub language_mode: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wrapper_mode: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PublicExportTool {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub component: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vendor: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PublicExportExecution {
    pub version_band: String,
    pub processing_path: String,
    pub support_level: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub allowed_processing_paths: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_authority: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fallback_grade: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fallback_reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub document_completeness: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PublicDiagnosticResult {
    pub summary: PublicDiagnosticSummary,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub diagnostics: Vec<PublicDiagnostic>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PublicDiagnosticSummary {
    pub diagnostic_count: usize,
    pub error_count: usize,
    pub warning_count: usize,
    pub note_count: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub independent_root_count: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dependent_follow_on_count: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duplicate_count: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub uncertain_count: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PublicDiagnostic {
    pub severity: String,
    pub phase: String,
    pub semantic_role: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub family: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub headline: Option<String>,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub first_action: Option<String>,
    pub confidence: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub primary_location: Option<PublicDiagnosticLocation>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub provenance_capture_refs: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub suggestions: Vec<PublicDiagnosticSuggestion>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub related_diagnostics: Vec<PublicDiagnostic>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PublicDiagnosticLocation {
    pub path: String,
    pub line: u32,
    pub column: u32,
    pub role: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ownership: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PublicDiagnosticSuggestion {
    pub label: String,
    pub applicability: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub edits: Vec<PublicDiagnosticTextEdit>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PublicDiagnosticTextEdit {
    pub path: String,
    pub start_line: u32,
    pub start_column: u32,
    pub end_line: u32,
    pub end_column: u32,
    pub replacement: String,
}

#[derive(Debug, Clone)]
pub struct PublicExportContext {
    pub producer: PublicExportProducer,
    pub invocation: PublicExportInvocation,
    pub version_band: VersionBand,
    pub processing_path: ProcessingPath,
    pub support_level: SupportLevel,
    pub allowed_processing_paths: Vec<ProcessingPath>,
    pub source_authority: Option<diag_core::SourceAuthority>,
    pub fallback_grade: Option<FallbackGrade>,
    pub fallback_reason: Option<FallbackReason>,
}

impl PublicExportContext {
    pub fn from_document(
        document: &DiagnosticDocument,
        version_band: VersionBand,
        processing_path: ProcessingPath,
        support_level: SupportLevel,
        source_authority: diag_core::SourceAuthority,
        fallback_grade: FallbackGrade,
        fallback_reason: Option<FallbackReason>,
    ) -> Self {
        Self {
            producer: PublicExportProducer {
                name: document.producer.name.clone(),
                version: document.producer.version.clone(),
            },
            invocation: PublicExportInvocation {
                invocation_id: Some(document.run.invocation_id.clone()),
                invoked_as: document.run.invoked_as.clone(),
                exit_status: document.run.exit_status,
                primary_tool: Some(PublicExportTool {
                    name: document.run.primary_tool.name.clone(),
                    version: document.run.primary_tool.version.clone(),
                    component: document.run.primary_tool.component.clone(),
                    vendor: document.run.primary_tool.vendor.clone(),
                }),
                language_mode: document.run.language_mode.clone().map(label),
                wrapper_mode: document.run.wrapper_mode.clone().map(label),
            },
            version_band,
            processing_path,
            support_level,
            allowed_processing_paths: default_allowed_processing_paths_for_version_band(
                version_band,
            ),
            source_authority: Some(source_authority),
            fallback_grade: Some(fallback_grade),
            fallback_reason,
        }
    }

    pub fn with_allowed_processing_paths<I>(mut self, paths: I) -> Self
    where
        I: IntoIterator<Item = ProcessingPath>,
    {
        self.allowed_processing_paths = paths.into_iter().collect();
        self
    }
}

pub fn export_from_document(
    document: &DiagnosticDocument,
    context: &PublicExportContext,
) -> PublicDiagnosticExport {
    PublicDiagnosticExport {
        schema_version: PUBLIC_EXPORT_SCHEMA_VERSION.to_string(),
        kind: PUBLIC_EXPORT_KIND.to_string(),
        status: PublicExportStatus::Available,
        producer: context.producer.clone(),
        invocation: context.invocation.clone(),
        execution: PublicExportExecution {
            version_band: label(context.version_band),
            processing_path: label(context.processing_path),
            support_level: label(context.support_level),
            allowed_processing_paths: context
                .allowed_processing_paths
                .iter()
                .copied()
                .map(label)
                .collect(),
            source_authority: context.source_authority.map(label),
            fallback_grade: context.fallback_grade.map(label),
            fallback_reason: context.fallback_reason.map(label),
            document_completeness: Some(label(document.document_completeness.clone())),
        },
        result: Some(PublicDiagnosticResult {
            summary: summary_from_document(document),
            diagnostics: document
                .diagnostics
                .iter()
                .map(public_diagnostic_from_node)
                .collect(),
        }),
        unavailable_reason: None,
    }
}

pub fn unavailable_export(
    context: &PublicExportContext,
    reason: PublicExportUnavailableReason,
) -> PublicDiagnosticExport {
    PublicDiagnosticExport {
        schema_version: PUBLIC_EXPORT_SCHEMA_VERSION.to_string(),
        kind: PUBLIC_EXPORT_KIND.to_string(),
        status: PublicExportStatus::Unavailable,
        producer: context.producer.clone(),
        invocation: context.invocation.clone(),
        execution: PublicExportExecution {
            version_band: label(context.version_band),
            processing_path: label(context.processing_path),
            support_level: label(context.support_level),
            allowed_processing_paths: context
                .allowed_processing_paths
                .iter()
                .copied()
                .map(label)
                .collect(),
            source_authority: context.source_authority.map(label),
            fallback_grade: context.fallback_grade.map(label),
            fallback_reason: context.fallback_reason.map(label),
            document_completeness: None,
        },
        result: None,
        unavailable_reason: Some(reason),
    }
}

fn summary_from_document(document: &DiagnosticDocument) -> PublicDiagnosticSummary {
    let mut counts = SeverityCounts::default();
    for diagnostic in &document.diagnostics {
        counts.visit(diagnostic);
    }
    let stats = document
        .document_analysis
        .as_ref()
        .map(|analysis| &analysis.stats);
    PublicDiagnosticSummary {
        diagnostic_count: counts.total,
        error_count: counts.error_count,
        warning_count: counts.warning_count,
        note_count: counts.note_count,
        independent_root_count: stats.map(|stats| stats.independent_root_count),
        dependent_follow_on_count: stats.map(|stats| stats.dependent_follow_on_count),
        duplicate_count: stats.map(|stats| stats.duplicate_count),
        uncertain_count: stats.map(|stats| stats.uncertain_count),
    }
}

fn public_diagnostic_from_node(node: &DiagnosticNode) -> PublicDiagnostic {
    PublicDiagnostic {
        severity: node.severity.to_string(),
        phase: node.phase.to_string(),
        semantic_role: label(node.semantic_role.clone()),
        family: node
            .analysis
            .as_ref()
            .and_then(|analysis| analysis.family.as_ref().map(ToString::to_string)),
        headline: node
            .analysis
            .as_ref()
            .and_then(|analysis| analysis.headline.as_ref().map(ToString::to_string)),
        message: node.message.raw_text.clone(),
        first_action: node
            .analysis
            .as_ref()
            .and_then(|analysis| analysis.first_action_hint.as_ref().map(ToString::to_string)),
        confidence: node
            .analysis
            .as_ref()
            .map(|analysis| Confidence::from_score(analysis.confidence))
            .map(label)
            .unwrap_or_else(|| "unknown".to_string()),
        primary_location: node.primary_location().map(public_location_from_location),
        provenance_capture_refs: node.provenance.capture_refs.clone(),
        suggestions: node
            .suggestions
            .iter()
            .map(public_suggestion_from_suggestion)
            .collect(),
        related_diagnostics: node
            .children
            .iter()
            .map(public_diagnostic_from_node)
            .collect(),
    }
}

fn public_location_from_location(location: &diag_core::Location) -> PublicDiagnosticLocation {
    PublicDiagnosticLocation {
        path: location.path_raw().to_string(),
        line: location.line(),
        column: location.column(),
        role: label(location.role),
        ownership: location.ownership().map(|ownership| match ownership {
            Ownership::User => "user".to_string(),
            Ownership::Vendor => "vendor".to_string(),
            Ownership::System => "system".to_string(),
            Ownership::Generated => "generated".to_string(),
            Ownership::Tool => "tool".to_string(),
            Ownership::Unknown => "unknown".to_string(),
        }),
    }
}

fn public_suggestion_from_suggestion(suggestion: &Suggestion) -> PublicDiagnosticSuggestion {
    PublicDiagnosticSuggestion {
        label: suggestion.label.clone(),
        applicability: label(suggestion.applicability.clone()),
        edits: suggestion
            .edits
            .iter()
            .map(|edit| PublicDiagnosticTextEdit {
                path: edit.path.clone(),
                start_line: edit.start_line,
                start_column: edit.start_column,
                end_line: edit.end_line,
                end_column: edit.end_column,
                replacement: edit.replacement.clone(),
            })
            .collect(),
    }
}

#[derive(Default)]
struct SeverityCounts {
    total: usize,
    error_count: usize,
    warning_count: usize,
    note_count: usize,
}

impl SeverityCounts {
    fn visit(&mut self, node: &DiagnosticNode) {
        self.total += 1;
        match node.severity {
            diag_core::Severity::Fatal | diag_core::Severity::Error => self.error_count += 1,
            diag_core::Severity::Warning => self.warning_count += 1,
            diag_core::Severity::Note => self.note_count += 1,
            _ => {}
        }
        for child in &node.children {
            self.visit(child);
        }
    }
}

pub fn normalize_export_for_snapshot_compare(export: &mut PublicDiagnosticExport) {
    export.schema_version = PUBLIC_EXPORT_SCHEMA_VERSION.to_string();
    export.producer.version = "<normalized>".to_string();
    export.invocation.invocation_id = export
        .invocation
        .invocation_id
        .as_ref()
        .map(|_| "<invocation>".to_string());
    if let Some(primary_tool) = export.invocation.primary_tool.as_mut() {
        primary_tool.version = None;
    }
}

pub fn schema_shape_fingerprint(export: &PublicDiagnosticExport) -> String {
    diag_core::fingerprint_for(&schema_shape_value(
        serde_json::to_value(export).unwrap_or(serde_json::Value::Null),
    ))
}

fn schema_shape_value(value: serde_json::Value) -> serde_json::Value {
    match value {
        serde_json::Value::Null => serde_json::Value::String("null".to_string()),
        serde_json::Value::Bool(_) => serde_json::Value::String("bool".to_string()),
        serde_json::Value::Number(_) => serde_json::Value::String("number".to_string()),
        serde_json::Value::String(_) => serde_json::Value::String("string".to_string()),
        serde_json::Value::Array(values) => {
            serde_json::Value::Array(values.into_iter().map(schema_shape_value).collect())
        }
        serde_json::Value::Object(object) => serde_json::Value::Object(
            object
                .into_iter()
                .map(|(key, value)| (key, schema_shape_value(value)))
                .collect(),
        ),
    }
}

fn label<T: Serialize>(value: T) -> String {
    serde_json::to_value(value)
        .ok()
        .and_then(|value| value.as_str().map(|value| value.to_string()))
        .unwrap_or_else(|| "unknown".to_string())
}

fn default_allowed_processing_paths_for_version_band(
    version_band: VersionBand,
) -> Vec<ProcessingPath> {
    capability_profile_for_major(representative_major_for_band(version_band))
        .allowed_processing_paths
        .into_iter()
        .collect()
}

fn representative_major_for_band(version_band: VersionBand) -> u32 {
    match version_band {
        VersionBand::Gcc16Plus => 16,
        VersionBand::Gcc15 => 15,
        VersionBand::Gcc13_14 => 13,
        VersionBand::Gcc9_12 => 9,
        VersionBand::Unknown => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use diag_core::{
        DocumentCompleteness, LanguageMode, Location, LocationRole, MessageText, NodeCompleteness,
        Origin, Phase, ProducerInfo, Provenance, ProvenanceSource, SemanticRole, Severity,
        ToolInfo, WrapperSurface,
    };

    const EXPECTED_PUBLIC_SCHEMA_SHAPE_FINGERPRINT: &str =
        "ce5b18957f4a1d52853416ac7764f101d9469a76e10ffcaea37dc2cd06e06325";

    fn sample_document() -> DiagnosticDocument {
        DiagnosticDocument {
            document_id: "doc-1".to_string(),
            schema_version: diag_core::IR_SPEC_VERSION.to_string(),
            document_completeness: DocumentCompleteness::Partial,
            producer: ProducerInfo {
                name: "gcc-formed".to_string(),
                version: "0.2.0-beta.1".to_string(),
                git_revision: None,
                build_profile: None,
                rulepack_version: None,
            },
            run: diag_core::RunInfo {
                invocation_id: "inv-1".to_string(),
                invoked_as: Some("gcc-formed".to_string()),
                argv_redacted: vec!["gcc".to_string(), "-c".to_string(), "main.c".to_string()],
                cwd_display: None,
                exit_status: 1,
                primary_tool: ToolInfo {
                    name: "gcc".to_string(),
                    version: Some("15.2.0".to_string()),
                    component: None,
                    vendor: Some("GNU".to_string()),
                },
                secondary_tools: Vec::new(),
                language_mode: Some(LanguageMode::C),
                target_triple: None,
                wrapper_mode: Some(WrapperSurface::Terminal),
            },
            captures: Vec::new(),
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
                        .with_ownership(Ownership::User, "user"),
                ],
                children: Vec::new(),
                suggestions: Vec::new(),
                context_chains: Vec::new(),
                symbol_context: None,
                node_completeness: NodeCompleteness::Partial,
                provenance: Provenance {
                    source: ProvenanceSource::ResidualText,
                    capture_refs: vec!["stderr.raw".to_string()],
                },
                analysis: Some(diag_core::AnalysisOverlay {
                    family: Some("syntax".into()),
                    family_version: None,
                    family_confidence: None,
                    root_cause_score: None,
                    actionability_score: None,
                    user_code_priority: None,
                    headline: Some("syntax error".into()),
                    first_action_hint: Some("insert the missing semicolon".into()),
                    confidence: Some(Confidence::High.score()),
                    preferred_primary_location_id: None,
                    rule_id: None,
                    matched_conditions: Vec::new(),
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
            document_analysis: Some(diag_core::DocumentAnalysis {
                policy_profile: Some("default-aggressive".to_string()),
                producer_version: Some("0.2.0-beta.1".to_string()),
                episode_graph: diag_core::EpisodeGraph::default(),
                group_analysis: Vec::new(),
                stats: diag_core::CascadeStats {
                    independent_root_count: 1,
                    dependent_follow_on_count: 0,
                    duplicate_count: 0,
                    uncertain_count: 0,
                },
            }),
            fingerprints: None,
        }
    }

    fn sample_related_public_diagnostic() -> PublicDiagnostic {
        PublicDiagnostic {
            severity: "note".to_string(),
            phase: "parse".to_string(),
            semantic_role: "follow_on".to_string(),
            family: Some("syntax".to_string()),
            headline: Some("related syntax detail".to_string()),
            message: "insert ';' before '}' token".to_string(),
            first_action: Some("inspect the preceding declaration".to_string()),
            confidence: "medium".to_string(),
            primary_location: Some(PublicDiagnosticLocation {
                path: "src/main.c".to_string(),
                line: 4,
                column: 1,
                role: "primary".to_string(),
                ownership: Some("user".to_string()),
            }),
            provenance_capture_refs: vec!["stderr.raw".to_string()],
            suggestions: Vec::new(),
            related_diagnostics: Vec::new(),
        }
    }

    fn representative_available_export() -> PublicDiagnosticExport {
        PublicDiagnosticExport {
            schema_version: PUBLIC_EXPORT_SCHEMA_VERSION.to_string(),
            kind: PUBLIC_EXPORT_KIND.to_string(),
            status: PublicExportStatus::Available,
            producer: PublicExportProducer {
                name: "gcc-formed".to_string(),
                version: "0.2.0-beta.1".to_string(),
            },
            invocation: PublicExportInvocation {
                invocation_id: Some("inv-1".to_string()),
                invoked_as: Some("gcc-formed".to_string()),
                exit_status: 1,
                primary_tool: Some(PublicExportTool {
                    name: "gcc".to_string(),
                    version: Some("15.2.0".to_string()),
                    component: Some("driver".to_string()),
                    vendor: Some("GNU".to_string()),
                }),
                language_mode: Some("c".to_string()),
                wrapper_mode: Some("terminal".to_string()),
            },
            execution: PublicExportExecution {
                version_band: "gcc15".to_string(),
                processing_path: "dual_sink_structured".to_string(),
                support_level: "in_scope".to_string(),
                allowed_processing_paths: vec![
                    "dual_sink_structured".to_string(),
                    "passthrough".to_string(),
                ],
                source_authority: Some("structured".to_string()),
                fallback_grade: Some("none".to_string()),
                fallback_reason: Some("shadow_mode".to_string()),
                document_completeness: Some("complete".to_string()),
            },
            result: Some(PublicDiagnosticResult {
                summary: PublicDiagnosticSummary {
                    diagnostic_count: 2,
                    error_count: 1,
                    warning_count: 0,
                    note_count: 1,
                    independent_root_count: Some(1),
                    dependent_follow_on_count: Some(1),
                    duplicate_count: Some(0),
                    uncertain_count: Some(0),
                },
                diagnostics: vec![PublicDiagnostic {
                    severity: "error".to_string(),
                    phase: "parse".to_string(),
                    semantic_role: "root".to_string(),
                    family: Some("syntax".to_string()),
                    headline: Some("syntax error".to_string()),
                    message: "expected ';' before '}' token".to_string(),
                    first_action: Some("insert the missing semicolon".to_string()),
                    confidence: "high".to_string(),
                    primary_location: Some(PublicDiagnosticLocation {
                        path: "src/main.c".to_string(),
                        line: 4,
                        column: 1,
                        role: "primary".to_string(),
                        ownership: Some("user".to_string()),
                    }),
                    provenance_capture_refs: vec!["stderr.raw".to_string()],
                    suggestions: vec![PublicDiagnosticSuggestion {
                        label: "insert ';'".to_string(),
                        applicability: "machine_applicable".to_string(),
                        edits: vec![PublicDiagnosticTextEdit {
                            path: "src/main.c".to_string(),
                            start_line: 4,
                            start_column: 1,
                            end_line: 4,
                            end_column: 1,
                            replacement: ";".to_string(),
                        }],
                    }],
                    related_diagnostics: vec![sample_related_public_diagnostic()],
                }],
            }),
            unavailable_reason: None,
        }
    }

    fn representative_unavailable_export() -> PublicDiagnosticExport {
        PublicDiagnosticExport {
            schema_version: PUBLIC_EXPORT_SCHEMA_VERSION.to_string(),
            kind: PUBLIC_EXPORT_KIND.to_string(),
            status: PublicExportStatus::Unavailable,
            producer: PublicExportProducer {
                name: "gcc-formed".to_string(),
                version: "0.2.0-beta.1".to_string(),
            },
            invocation: PublicExportInvocation {
                invocation_id: Some("inv-2".to_string()),
                invoked_as: Some("gcc-formed".to_string()),
                exit_status: 0,
                primary_tool: Some(PublicExportTool {
                    name: "gcc".to_string(),
                    version: Some("15.2.0".to_string()),
                    component: Some("driver".to_string()),
                    vendor: Some("GNU".to_string()),
                }),
                language_mode: Some("c".to_string()),
                wrapper_mode: Some("terminal".to_string()),
            },
            execution: PublicExportExecution {
                version_band: "gcc15".to_string(),
                processing_path: "passthrough".to_string(),
                support_level: "in_scope".to_string(),
                allowed_processing_paths: vec![
                    "dual_sink_structured".to_string(),
                    "passthrough".to_string(),
                ],
                source_authority: Some("structured".to_string()),
                fallback_grade: Some("none".to_string()),
                fallback_reason: Some("incompatible_sink".to_string()),
                document_completeness: Some("passthrough".to_string()),
            },
            result: None,
            unavailable_reason: Some(PublicExportUnavailableReason::PassthroughMode),
        }
    }

    fn sample_export() -> PublicDiagnosticExport {
        let document = sample_document();
        let context = PublicExportContext::from_document(
            &document,
            VersionBand::Gcc13_14,
            ProcessingPath::NativeTextCapture,
            SupportLevel::InScope,
            diag_core::SourceAuthority::ResidualText,
            FallbackGrade::Compatibility,
            None,
        );
        export_from_document(&document, &context)
    }

    #[test]
    fn export_projection_is_deterministic() {
        let document = sample_document();
        let context = PublicExportContext::from_document(
            &document,
            VersionBand::Gcc13_14,
            ProcessingPath::NativeTextCapture,
            SupportLevel::InScope,
            diag_core::SourceAuthority::ResidualText,
            FallbackGrade::Compatibility,
            None,
        );
        let export = export_from_document(&document, &context);

        assert_eq!(export.status, PublicExportStatus::Available);
        assert_eq!(export.execution.version_band, "gcc13_14");
        assert_eq!(
            export
                .result
                .as_ref()
                .unwrap()
                .diagnostics
                .first()
                .unwrap()
                .headline
                .as_deref(),
            Some("syntax error")
        );
        assert_eq!(
            export
                .result
                .as_ref()
                .unwrap()
                .diagnostics
                .first()
                .unwrap()
                .provenance_capture_refs,
            vec!["stderr.raw".to_string()]
        );
        assert_eq!(
            export.canonical_json().unwrap(),
            export.canonical_json().unwrap()
        );
    }

    #[test]
    fn schema_shape_fingerprint_ignores_scalar_value_changes() {
        let export = sample_export();
        let fingerprint = schema_shape_fingerprint(&export);

        let mut mutated = export.clone();
        mutated.schema_version = "9.9.9".to_string();
        mutated.producer.version = "other-version".to_string();
        mutated.invocation.invocation_id = Some("inv-999".to_string());
        mutated.execution.support_level = "in_scope".to_string();
        mutated.execution.document_completeness = Some("complete".to_string());

        assert_eq!(fingerprint, schema_shape_fingerprint(&mutated));
    }

    #[test]
    fn schema_shape_fingerprint_changes_when_structure_changes() {
        let export = sample_export();
        let fingerprint = schema_shape_fingerprint(&export);

        let mut mutated = export.clone();
        mutated.invocation.primary_tool = None;

        assert_ne!(fingerprint, schema_shape_fingerprint(&mutated));
    }

    #[test]
    fn unavailable_export_uses_explicit_reason() {
        let context = PublicExportContext {
            producer: PublicExportProducer {
                name: "gcc-formed".to_string(),
                version: "0.2.0-beta.1".to_string(),
            },
            invocation: PublicExportInvocation {
                invocation_id: None,
                invoked_as: Some("gcc-formed".to_string()),
                exit_status: 0,
                primary_tool: Some(PublicExportTool {
                    name: "gcc".to_string(),
                    version: Some("15.2.0".to_string()),
                    component: None,
                    vendor: Some("GNU".to_string()),
                }),
                language_mode: Some("c".to_string()),
                wrapper_mode: Some("terminal".to_string()),
            },
            version_band: VersionBand::Gcc15,
            processing_path: ProcessingPath::Passthrough,
            support_level: SupportLevel::InScope,
            allowed_processing_paths: vec![
                ProcessingPath::DualSinkStructured,
                ProcessingPath::Passthrough,
            ],
            source_authority: None,
            fallback_grade: None,
            fallback_reason: Some(FallbackReason::UserOptOut),
        };

        let export = unavailable_export(&context, PublicExportUnavailableReason::PassthroughMode);

        assert_eq!(export.status, PublicExportStatus::Unavailable);
        assert_eq!(
            export.unavailable_reason,
            Some(PublicExportUnavailableReason::PassthroughMode)
        );
        assert!(export.result.is_none());
    }

    #[test]
    fn snapshot_normalization_strips_volatile_fields() {
        let document = sample_document();
        let context = PublicExportContext::from_document(
            &document,
            VersionBand::Gcc15,
            ProcessingPath::DualSinkStructured,
            SupportLevel::InScope,
            diag_core::SourceAuthority::Structured,
            FallbackGrade::None,
            None,
        );
        let mut export = export_from_document(&document, &context);
        normalize_export_for_snapshot_compare(&mut export);

        assert_eq!(export.producer.version, "<normalized>");
        assert_eq!(
            export.invocation.invocation_id.as_deref(),
            Some("<invocation>")
        );
        assert!(
            export
                .invocation
                .primary_tool
                .as_ref()
                .unwrap()
                .version
                .is_none()
        );
    }

    #[test]
    fn public_schema_shape_fingerprint_is_stable() {
        let representative_shapes = vec![
            representative_available_export(),
            representative_unavailable_export(),
        ]
        .into_iter()
        .map(|export| schema_shape_value(serde_json::to_value(export).unwrap()))
        .collect::<Vec<_>>();

        assert_eq!(
            diag_core::fingerprint_for(&representative_shapes),
            EXPECTED_PUBLIC_SCHEMA_SHAPE_FINGERPRINT
        );
    }

    #[test]
    fn custom_allowed_processing_paths_override_band_defaults() {
        let document = sample_document();
        let context = PublicExportContext::from_document(
            &document,
            VersionBand::Gcc15,
            ProcessingPath::NativeTextCapture,
            SupportLevel::InScope,
            diag_core::SourceAuthority::ResidualText,
            FallbackGrade::Compatibility,
            None,
        )
        .with_allowed_processing_paths([
            ProcessingPath::NativeTextCapture,
            ProcessingPath::SingleSinkStructured,
            ProcessingPath::Passthrough,
        ]);

        let export = export_from_document(&document, &context);

        assert_eq!(
            export.execution.allowed_processing_paths,
            vec![
                "native_text_capture".to_string(),
                "single_sink_structured".to_string(),
                "passthrough".to_string(),
            ]
        );
    }

    #[test]
    fn representative_band_exports_keep_required_execution_fields() {
        let document = sample_document();
        let contexts = [
            PublicExportContext::from_document(
                &document,
                VersionBand::Gcc15,
                ProcessingPath::DualSinkStructured,
                SupportLevel::InScope,
                diag_core::SourceAuthority::Structured,
                FallbackGrade::None,
                None,
            ),
            PublicExportContext::from_document(
                &document,
                VersionBand::Gcc13_14,
                ProcessingPath::NativeTextCapture,
                SupportLevel::InScope,
                diag_core::SourceAuthority::ResidualText,
                FallbackGrade::Compatibility,
                None,
            ),
            PublicExportContext::from_document(
                &document,
                VersionBand::Gcc9_12,
                ProcessingPath::SingleSinkStructured,
                SupportLevel::InScope,
                diag_core::SourceAuthority::Structured,
                FallbackGrade::None,
                None,
            ),
        ];

        let exports = contexts
            .iter()
            .map(|context| export_from_document(&document, context))
            .collect::<Vec<_>>();

        assert_eq!(exports[0].execution.version_band, "gcc15");
        assert_eq!(exports[1].execution.version_band, "gcc13_14");
        assert_eq!(exports[2].execution.version_band, "gcc9_12");
        for export in exports {
            assert!(!export.execution.version_band.is_empty());
            assert!(!export.execution.processing_path.is_empty());
            assert!(!export.execution.support_level.is_empty());
            assert!(!export.execution.allowed_processing_paths.is_empty());
            assert!(export.execution.source_authority.is_some());
            assert!(export.execution.fallback_grade.is_some());
        }
    }
}
