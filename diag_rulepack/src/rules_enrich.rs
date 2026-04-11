//! Enrichment rule type definitions for the diagnostic pipeline.
//!
//! This module contains the configuration types for enrichment rules,
//! including family match rules and confidence policies.

use diag_core::Phase;
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Enrich section types
// ---------------------------------------------------------------------------

/// Enrichment rulepack defining family match rules and confidence policies.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EnrichRulepack {
    /// Schema version identifier for this enrichment rulepack format.
    pub schema_version: String,
    /// Version tag matching the parent manifest.
    pub rulepack_version: String,
    /// Rule ID used for ingress-specific overrides.
    pub ingress_specific_override_rule_id: String,
    /// Fallback rule applied when no family matches (unknown diagnostics).
    pub unknown_fallback: FallbackRuleConfig,
    /// Fallback rule applied to passthrough diagnostics.
    pub passthrough_fallback: FallbackRuleConfig,
    /// Ordered list of family match rules evaluated during enrichment.
    pub rules: Vec<FamilyRuleConfig>,
    /// Default confidence policy used when no family-specific policy exists.
    pub default_confidence_policy: ConfidencePolicyConfig,
    /// Family-specific confidence policies.
    pub confidence_policies: Vec<ConfidencePolicyConfig>,
}

/// Configuration for an unknown or passthrough fallback rule.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FallbackRuleConfig {
    /// Diagnostic family assigned by this fallback.
    pub family: String,
    /// Unique rule identifier for tracing.
    pub rule_id: String,
    /// Conditions that were matched when this fallback was selected.
    pub matched_conditions: Vec<String>,
    /// Human-readable reason why the diagnostic was suppressed.
    pub suppression_reason: String,
}

/// Match rule that assigns a diagnostic to a specific family.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FamilyRuleConfig {
    /// Unique rule identifier for tracing and auditing.
    pub rule_id: String,
    /// Target family name this rule assigns on match.
    pub family: String,
    /// Strategy controlling how message text is matched.
    pub match_strategy: MatchStrategyConfig,
    /// Term groups matched against the primary diagnostic message.
    #[serde(default)]
    pub message_groups: Vec<TermGroupConfig>,
    /// Term groups matched against child diagnostic messages.
    #[serde(default)]
    pub child_message_groups: Vec<TermGroupConfig>,
    /// Terms matched against candidate child messages.
    #[serde(default)]
    pub candidate_child_terms: Vec<String>,
    /// Context conditions (e.g. template instantiation, macro expansion).
    #[serde(default)]
    pub contexts: Vec<ContextConditionConfig>,
    /// Child note conditions (e.g. template context, macro expansion).
    #[serde(default)]
    pub child_notes: Vec<ChildNoteConditionConfig>,
    /// Optional regex condition matched against symbol context.
    #[serde(default)]
    pub symbol_context_condition: Option<String>,
    /// Optional regex condition matched against candidate children.
    #[serde(default)]
    pub candidate_child_condition: Option<String>,
    /// Optional regex condition matched against the semantic role.
    #[serde(default)]
    pub semantic_role_condition: Option<String>,
    /// Phase annotations applied when this rule matches.
    #[serde(default)]
    pub phase_annotations: Vec<PhaseAnnotationConfig>,
}

/// Strategy that determines how a family rule matches diagnostic messages.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MatchStrategyConfig {
    /// Match using structured fields first, falling back to message text.
    StructuredOrMessage,
    /// Match using phase information first, falling back to message text.
    PhaseOrMessage,
    /// Match using semantic role classification.
    SemanticRole,
}

/// A named group of search terms sharing a common prefix.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TermGroupConfig {
    /// Prefix label identifying this term group.
    pub prefix: String,
    /// Individual search terms within this group.
    pub terms: Vec<String>,
}

/// A context condition checked during family rule evaluation.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ContextConditionConfig {
    /// Kind of context this condition targets.
    pub kind: ContextConditionKind,
    /// Regex pattern matched against the context text.
    pub condition: String,
}

/// Kind of context that a [`ContextConditionConfig`] applies to.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ContextConditionKind {
    /// C++ template instantiation context.
    TemplateInstantiation,
    /// Preprocessor macro expansion context.
    MacroExpansion,
    /// Header include context.
    Include,
    /// Linker symbol resolution context.
    LinkerResolution,
}

/// A child note condition checked during family rule evaluation.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ChildNoteConditionConfig {
    /// Kind of child note this condition targets.
    pub kind: ChildNoteConditionKind,
    /// Regex pattern matched against the child note text.
    pub condition: String,
}

/// Kind of child note that a [`ChildNoteConditionConfig`] applies to.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ChildNoteConditionKind {
    /// Template context child note.
    TemplateContext,
    /// Macro expansion child note.
    MacroExpansion,
    /// Include chain child note.
    Include,
}

/// A phase annotation attached to a family rule.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PhaseAnnotationConfig {
    /// Compiler phase this annotation targets.
    pub phase: Phase,
    /// Regex condition that must match for the annotation to apply.
    pub condition: String,
    /// When during rule evaluation this annotation is checked.
    pub when: PhaseAnnotationWhen,
}

/// When a phase annotation condition is evaluated.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PhaseAnnotationWhen {
    /// Evaluate after the family rule has matched.
    RuleMatched,
    /// Evaluate against the primary message terms.
    MessageTerms,
    /// Evaluate against the primary message or candidate children.
    MessageOrCandidate,
}

/// Policy determining confidence level assignment for a family.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ConfidencePolicyConfig {
    /// Family this policy applies to, or `None` for the default policy.
    #[serde(default)]
    pub family: Option<String>,
    /// If set, always assign this fixed confidence level.
    #[serde(default)]
    pub fixed: Option<ConfidenceLevelConfig>,
    /// Clauses; if any clause matches, confidence is high.
    #[serde(default)]
    pub high_when_any: Vec<ConfidenceClauseConfig>,
    /// Clauses; if any clause matches, confidence is medium.
    #[serde(default)]
    pub medium_when_any: Vec<ConfidenceClauseConfig>,
    /// Confidence level used when no clause matches.
    pub default_confidence: ConfidenceLevelConfig,
}

/// A conjunction of signals; all must be present for the clause to match.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ConfidenceClauseConfig {
    /// Signals that must all be present for this clause to match.
    pub all: Vec<ConfidenceSignal>,
}

/// An individual signal used in confidence level evaluation.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ConfidenceSignal {
    /// Diagnostic location is in user-owned source.
    UserOwnedLocation,
    /// Primary ownership belongs to the user.
    PrimaryOwnershipUser,
    /// Phase is parsing.
    PhaseParse,
    /// Phase is semantic analysis.
    PhaseSemantic,
    /// Phase is template instantiation.
    PhaseInstantiate,
    /// Phase is linking.
    PhaseLink,
    /// Template context is present.
    TemplateContext,
    /// Macro expansion context is present.
    MacroContext,
    /// Include context is present.
    IncludeContext,
    /// Linker resolution context is present.
    LinkerContext,
    /// Symbol context is present.
    SymbolContext,
    /// Candidate child note is present.
    CandidateChild,
    /// Template child note is present.
    TemplateChild,
    /// Macro child note is present.
    MacroChild,
    /// Include child note is present.
    IncludeChild,
    /// Lexical (message-text) signal matched.
    LexicalSignal,
    /// Structured (non-text) signal matched.
    StructuredSignal,
    /// Diagnostic already belongs to a specific family.
    ExistingSpecificFamily,
}

/// Discrete confidence level assigned to a classified diagnostic.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ConfidenceLevelConfig {
    /// High confidence in the classification.
    High,
    /// Medium confidence in the classification.
    Medium,
    /// Low confidence in the classification.
    Low,
}

impl From<ConfidenceLevelConfig> for diag_core::Confidence {
    fn from(value: ConfidenceLevelConfig) -> Self {
        match value {
            ConfidenceLevelConfig::High => diag_core::Confidence::High,
            ConfidenceLevelConfig::Medium => diag_core::Confidence::Medium,
            ConfidenceLevelConfig::Low => diag_core::Confidence::Low,
        }
    }
}
