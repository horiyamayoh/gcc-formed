//! Rule pack definitions and loading for the diagnostic pipeline.
//!
//! This crate defines configuration types for enrichment rules, residual text
//! classification rules, and rendering policies. Rule packs are loaded from
//! JSON manifests that reference versioned, SHA-256-verified section files.
//!
//! Key types:
//! - [`LoadedRulepack`] -- a fully validated, ready-to-use rule pack bundle.
//! - [`EnrichRulepack`] -- family-level match rules and confidence policies.
//! - [`ResidualRulepack`] -- wording templates and residual classification seeds.
//! - [`RenderRulepack`] -- per-family rendering policies and profile limits.

use diag_core::Phase;
use regex::Regex;
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::sync::OnceLock;

/// Schema version string expected in rulepack manifest files.
pub const RULEPACK_MANIFEST_SCHEMA_VERSION: &str = "diag_rulepack_manifest/v1alpha1";
/// Schema version string expected in enrichment rulepack sections.
pub const ENRICH_RULEPACK_SCHEMA_VERSION: &str = "diag_enrich_rulepack/v1alpha1";
/// Schema version string expected in residual rulepack sections.
pub const RESIDUAL_RULEPACK_SCHEMA_VERSION: &str = "diag_residual_rulepack/v1alpha1";
/// Schema version string expected in render rulepack sections.
pub const RENDER_RULEPACK_SCHEMA_VERSION: &str = "diag_render_rulepack/v1alpha1";
/// Rulepack version identifier for the checked-in (bundled) rule pack.
pub const CHECKED_IN_RULEPACK_VERSION: &str = "phase1";
/// Filename of the checked-in manifest on disk.
pub const CHECKED_IN_MANIFEST_FILE: &str = "diag_rulepack.manifest.phase1.json";

#[cfg(test)]
const CHECKED_IN_SECTION_FILES: &[&str] = &[
    CHECKED_IN_MANIFEST_FILE,
    "enrich.rulepack.json",
    "residual.rulepack.json",
    "render.rulepack.json",
];

const CHECKED_IN_MANIFEST_RAW: &[u8] =
    include_bytes!("../../rules/diag_rulepack.manifest.phase1.json");
const CHECKED_IN_ENRICH_RAW: &[u8] = include_bytes!("../../rules/enrich.rulepack.json");
const CHECKED_IN_RESIDUAL_RAW: &[u8] = include_bytes!("../../rules/residual.rulepack.json");
const CHECKED_IN_RENDER_RAW: &[u8] = include_bytes!("../../rules/render.rulepack.json");

/// Top-level manifest that lists the sections composing a rule pack.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RulepackManifest {
    /// Schema version identifier for this manifest format.
    pub schema_version: String,
    /// Version tag shared by every section in this rule pack.
    pub rulepack_version: String,
    /// Optional human-readable description.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Ordered list of section references (enrich, residual, render).
    pub sections: Vec<ManifestSection>,
}

/// A single section entry inside a [`RulepackManifest`].
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ManifestSection {
    /// Which section this entry represents (enrich, residual, or render).
    pub kind: SectionKind,
    /// Relative path to the section JSON file.
    pub path: String,
    /// Expected SHA-256 hex digest of the section file contents.
    pub sha256: String,
}

/// Discriminator for the three kinds of rulepack sections.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum SectionKind {
    /// Enrichment (family matching and confidence) section.
    Enrich,
    /// Residual text classification section.
    Residual,
    /// Rendering policy section.
    Render,
}

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

/// Residual rulepack defining wording templates and residual classification seeds.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ResidualRulepack {
    /// Schema version identifier for this residual rulepack format.
    pub schema_version: String,
    /// Version tag matching the parent manifest.
    pub rulepack_version: String,
    /// Wording templates for headlines and action hints.
    pub wording: WordingSection,
    /// Residual text classification seeds for compiler and linker output.
    pub residual: ResidualSection,
}

/// Wording templates for diagnostic headlines and action hints.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WordingSection {
    /// Strategy for generating a headline when no specific wording exists.
    pub default_headline_strategy: HeadlineFallbackStrategy,
    /// Default action hint shown when no family-specific hint exists.
    pub default_action_hint: String,
    /// Template for linker symbol headlines with a `{symbol}` placeholder.
    pub generic_linker_symbol_headline_template: String,
    /// Generic headlines keyed by family.
    pub generic_headlines: Vec<FamilyText>,
    /// Generic action hints keyed by family.
    pub generic_action_hints: Vec<FamilyText>,
    /// Family-specific wording overrides with full headline templates.
    pub specific_overrides: Vec<SpecificWordingOverride>,
}

/// A family-keyed text entry (used for generic headlines and action hints).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FamilyText {
    /// Family this text applies to.
    pub family: String,
    /// The text content (headline or action hint).
    pub text: String,
}

/// Family-specific wording override with full headline and action hint templates.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SpecificWordingOverride {
    /// Family this override applies to.
    pub family: String,
    /// Headline template (may contain `{symbol}` placeholder).
    pub headline_template: String,
    /// Headline used when no symbol is available.
    pub headline_without_symbol: String,
    /// Action hint shown as the first suggestion.
    pub first_action_hint: String,
}

/// Strategy for generating a headline when no specific wording exists.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum HeadlineFallbackStrategy {
    /// Use the first line of the diagnostic message as the headline.
    FirstMessageLine,
}

/// Container for all residual classification seeds.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ResidualSection {
    /// Seeds for classifying residual compiler diagnostics.
    pub compiler_groups: Vec<CompilerResidualSeed>,
    /// Rules for recognizing compiler note patterns.
    pub compiler_note_rules: CompilerNoteRules,
    /// Seeds for classifying residual linker diagnostics.
    pub linker_groups: Vec<LinkerResidualSeed>,
    /// Seed for unrecognized passthrough diagnostics.
    pub passthrough: PassthroughResidualSeed,
}

/// Kind of residual compiler diagnostic.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum CompilerResidualKind {
    /// Syntax error residual.
    Syntax,
    /// Template instantiation failure residual.
    Template,
    /// Type mismatch or overload resolution failure residual.
    TypeOverload,
    /// Unrecognized compiler diagnostic residual.
    Unknown,
}

/// Strategy for producing a residual seed headline.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum HeadlineStrategy {
    /// Use a fixed, pre-defined headline string.
    FixedText,
    /// Pass through the original diagnostic message as the headline.
    MessagePassthrough,
}

/// Seed for classifying a residual compiler diagnostic into a group.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CompilerResidualSeed {
    /// Residual kind this seed targets.
    pub kind: CompilerResidualKind,
    /// Family name assigned to matched diagnostics.
    pub family: String,
    /// Compiler phase this seed applies to.
    pub phase: Phase,
    /// Unique rule identifier for tracing.
    pub rule_id: String,
    /// Strategy for producing the headline.
    pub headline_strategy: HeadlineStrategy,
    /// Fixed headline text (required when `headline_strategy` is `FixedText`).
    pub headline: Option<String>,
    /// Action hint shown as the first suggestion.
    pub first_action_hint: String,
    /// Patterns; if any matches the message, this seed applies.
    #[serde(default)]
    pub match_any: Vec<String>,
}

/// Rules for recognizing compiler note patterns (template context, candidates).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CompilerNoteRules {
    /// Patterns matching template context notes.
    #[serde(default)]
    pub template_context_any: Vec<String>,
    /// Substrings that identify candidate notes.
    #[serde(default)]
    pub candidate_contains: Vec<String>,
    /// Prefix for numbered candidate notes (e.g. "candidate #").
    pub candidate_numbered_prefix: String,
}

/// Kind of residual linker diagnostic.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum LinkerResidualKind {
    /// Undefined reference to a symbol.
    UndefinedReference,
    /// Multiple definitions of a symbol.
    MultipleDefinition,
    /// Library not found (`-l` flag).
    CannotFindLibrary,
    /// File format or relocation error.
    FileFormatOrRelocation,
    /// `collect2` summary line.
    Collect2Summary,
    /// Assembler error passed through the driver.
    AssemblerError,
    /// Fatal driver error.
    DriverFatal,
    /// Internal compiler error (ICE) banner line.
    InternalCompilerErrorBanner,
}

/// Seed for classifying a residual linker diagnostic into a group.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LinkerResidualSeed {
    /// Residual kind this seed targets.
    pub kind: LinkerResidualKind,
    /// Family name assigned to matched diagnostics.
    pub family: String,
    /// Origin tool that produced this diagnostic.
    pub origin: diag_core::Origin,
    /// Compiler phase this seed applies to.
    pub phase: Phase,
    /// Unique rule identifier for tracing.
    pub rule_id: String,
    /// Static group key (mutually exclusive with `group_key_template`).
    #[serde(default)]
    pub group_key: Option<String>,
    /// Template for generating a group key (mutually exclusive with `group_key`).
    #[serde(default)]
    pub group_key_template: Option<String>,
    /// Regex pattern matched against the diagnostic message.
    #[serde(default)]
    pub match_regex: Option<String>,
    /// Prefix matched against the diagnostic message.
    #[serde(default)]
    pub match_prefix: Option<String>,
    /// Whether the match requires a colon separator in the message.
    #[serde(default)]
    pub requires_colon: bool,
    /// Regex capture group for extracting the symbol name.
    #[serde(default)]
    pub symbol_capture: Option<String>,
    /// Template for generating the headline (may contain `{symbol}`).
    pub headline_template: String,
    /// Action hint shown as the first suggestion.
    pub first_action_hint: String,
}

/// Seed for the passthrough residual bucket (diagnostics not matching any rule).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PassthroughResidualSeed {
    /// Family name assigned to passthrough diagnostics.
    pub family: String,
    /// Compiler phase for passthrough diagnostics.
    pub phase: Phase,
    /// Unique rule identifier for tracing.
    pub rule_id: String,
    /// Default headline for passthrough diagnostics.
    pub headline: String,
    /// Action hint shown as the first suggestion.
    pub first_action_hint: String,
}

/// Render rulepack defining per-family rendering policies and profile limits.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RenderRulepack {
    /// Schema version identifier for this render rulepack format.
    pub schema_version: String,
    /// Version tag matching the parent manifest.
    pub rulepack_version: String,
    /// Ordered list of family rendering policies.
    pub family_policies: Vec<RendererFamilyPolicy>,
}

/// Discriminator for renderer family policy groupings.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum RendererFamilyKind {
    /// Catch-all for unrecognized families.
    Unknown,
    /// Syntax error family.
    Syntax,
    /// Template instantiation family.
    Template,
    /// Macro or include family.
    MacroInclude,
    /// Type mismatch or overload resolution family.
    TypeOverload,
    /// Linker diagnostic family.
    Linker,
}

/// Rendering policy for a specific family kind.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RendererFamilyPolicy {
    /// Which family grouping this policy governs.
    pub kind: RendererFamilyKind,
    /// Exact family name to match (mutually exclusive with `match_prefix`).
    #[serde(default)]
    pub match_exact: Option<String>,
    /// Family name prefix to match (mutually exclusive with `match_exact`).
    #[serde(default)]
    pub match_prefix: Option<String>,
    /// Family name to exclude even if the prefix matches.
    #[serde(default)]
    pub exclude_exact: Option<String>,
    /// Specificity rank for tie-breaking when multiple policies match.
    pub specificity_rank: u8,
    /// Whether Band-C conservative mode considers this family useful.
    pub band_c_conservative_useful_subset: bool,
    /// Optional per-profile child note display limits.
    #[serde(default)]
    pub conservative_limits: Option<ProfileLimitPolicy>,
}

/// Per-profile maximum child note counts for conservative rendering.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProfileLimitPolicy {
    /// Limit for the verbose output profile.
    pub verbose: usize,
    /// Limit for the debug output profile.
    pub debug: usize,
    /// Limit for the default output profile.
    pub default: usize,
    /// Limit for the concise output profile.
    pub concise: usize,
    /// Limit for the CI output profile.
    pub ci: usize,
    /// Fallback limit used for raw output.
    pub raw_fallback: usize,
}

/// A fully validated, ready-to-use rule pack bundle containing all three sections.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoadedRulepack {
    manifest: RulepackManifest,
    enrich: EnrichRulepack,
    residual: ResidualRulepack,
    render: RenderRulepack,
}

impl LoadedRulepack {
    /// Returns the rulepack version string from the manifest.
    pub fn version(&self) -> &str {
        &self.manifest.rulepack_version
    }

    /// Returns the manifest metadata for this rule pack.
    pub fn manifest(&self) -> &RulepackManifest {
        &self.manifest
    }

    /// Returns the enrichment rulepack section.
    pub fn enrich(&self) -> &EnrichRulepack {
        &self.enrich
    }

    /// Returns the residual rulepack section.
    pub fn residual(&self) -> &ResidualRulepack {
        &self.residual
    }

    /// Returns the render rulepack section.
    pub fn render(&self) -> &RenderRulepack {
        &self.render
    }
}

impl EnrichRulepack {
    /// Looks up the family rule for the given family name.
    ///
    /// # Panics
    ///
    /// Panics if the family rule is not found. This is a fail-fast invariant:
    /// the checked-in enrich rulepack is expected to be complete, so a missing
    /// family indicates a configuration error that must be fixed before release.
    pub fn rule(&self, family: &str) -> &FamilyRuleConfig {
        self.rules
            .iter()
            .find(|rule| rule.family == family)
            .unwrap_or_else(|| {
                panic!("missing family rule in checked-in enrich rulepack: {family}")
            })
    }

    /// Returns the confidence policy for the given family, falling back to
    /// the generic `"linker"` policy for `linker.*` families, then to the
    /// default confidence policy.
    pub fn confidence_policy_for(&self, family: &str) -> &ConfidencePolicyConfig {
        self.confidence_policies
            .iter()
            .find(|policy| policy.family.as_deref() == Some(family))
            .or_else(|| {
                family
                    .starts_with("linker.")
                    .then(|| {
                        self.confidence_policies
                            .iter()
                            .find(|policy| policy.family.as_deref() == Some("linker"))
                    })
                    .flatten()
            })
            .unwrap_or(&self.default_confidence_policy)
    }
}

impl ResidualRulepack {
    /// Returns the compiler residual seed for the given kind.
    ///
    /// # Panics
    ///
    /// Panics if the seed for the requested kind is not found. This is a
    /// fail-fast configuration invariant: the checked-in residual rulepack
    /// must contain seeds for every [`CompilerResidualKind`].
    pub fn compiler_seed(&self, kind: CompilerResidualKind) -> &CompilerResidualSeed {
        self.residual
            .compiler_groups
            .iter()
            .find(|entry| entry.kind == kind)
            .unwrap_or_else(|| panic!("missing compiler residual seed for {kind:?}"))
    }

    /// Returns the generic headline text for the given family, if one exists.
    pub fn generic_headline(&self, family: &str) -> Option<&str> {
        generic_family_text(&self.wording.generic_headlines, family)
            .map(|entry| entry.text.as_str())
    }

    /// Returns the generic action hint for the given family, if one exists.
    pub fn generic_action_hint(&self, family: &str) -> Option<&str> {
        generic_family_text(&self.wording.generic_action_hints, family)
            .map(|entry| entry.text.as_str())
    }

    /// Returns the specific wording override for the given family, if one exists.
    pub fn specific_wording_override(&self, family: &str) -> Option<&SpecificWordingOverride> {
        self.wording
            .specific_overrides
            .iter()
            .find(|entry| entry.family == family)
    }
}

impl RenderRulepack {
    /// Returns the first rendering policy whose match criteria include `family`.
    pub fn policy_for_family(&self, family: &str) -> Option<&RendererFamilyPolicy> {
        self.family_policies
            .iter()
            .find(|policy| policy.matches(family))
    }

    /// Returns the rendering policy for the given [`RendererFamilyKind`].
    pub fn policy_for_kind(&self, kind: RendererFamilyKind) -> Option<&RendererFamilyPolicy> {
        self.family_policies
            .iter()
            .find(|policy| policy.kind == kind)
    }
}

impl RendererFamilyPolicy {
    /// Returns `true` if this policy matches the given family name.
    pub fn matches(&self, family: &str) -> bool {
        if self.exclude_exact.as_deref() == Some(family) {
            return false;
        }

        self.match_exact.as_deref() == Some(family)
            || self
                .match_prefix
                .as_deref()
                .is_some_and(|prefix| family.starts_with(prefix))
    }
}

/// Errors that can occur when loading or validating a rulepack.
#[derive(Debug, thiserror::Error)]
pub enum RulepackError {
    /// A section file could not be read from disk.
    #[error("failed to read {path}: {source}")]
    ReadFile {
        /// Path to the file that could not be read.
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    /// A section file contained invalid JSON.
    #[error("failed to parse JSON in {path}: {source}")]
    ParseJson {
        /// Path to the file with the parse error.
        path: PathBuf,
        #[source]
        source: serde_json::Error,
    },
    /// The SHA-256 digest of a section file did not match the manifest.
    #[error("rulepack digest mismatch for {path}: expected {expected}, got {actual}")]
    DigestMismatch {
        /// Path to the mismatched section file.
        path: PathBuf,
        /// Digest recorded in the manifest.
        expected: String,
        /// Digest computed from the actual file contents.
        actual: String,
    },
    /// A rulepack failed structural or semantic validation.
    #[error("invalid rulepack at {path}: {message}")]
    InvalidRulepack {
        /// Path to the invalid rulepack file.
        path: PathBuf,
        /// Human-readable description of the validation failure.
        message: String,
    },
}

static CHECKED_IN_RULEPACK: OnceLock<LoadedRulepack> = OnceLock::new();

/// Returns the on-disk path to the checked-in rules directory.
pub fn checked_in_rules_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("workspace root")
        .join("rules")
}

/// Returns the on-disk path to the checked-in manifest file.
pub fn checked_in_manifest_path() -> PathBuf {
    checked_in_rules_dir().join(CHECKED_IN_MANIFEST_FILE)
}

/// Returns a reference to the lazily-initialized, embedded checked-in rulepack.
///
/// The rulepack is parsed and validated on first access and cached for the
/// lifetime of the process.
///
/// # Panics
///
/// Panics if the embedded checked-in rulepack fails validation. This is a
/// fail-fast invariant ensuring the bundled rulepack is always valid; a panic
/// here indicates a build-time data error that must be fixed before release.
pub fn checked_in_rulepack() -> &'static LoadedRulepack {
    CHECKED_IN_RULEPACK.get_or_init(|| {
        load_embedded_rulepack().unwrap_or_else(|error| {
            panic!("checked-in diag_rulepack must validate at runtime: {error}");
        })
    })
}

/// Returns the version string of the checked-in rulepack.
pub fn checked_in_rulepack_version() -> &'static str {
    checked_in_rulepack().version()
}

/// Clones and returns the checked-in rulepack.
pub fn load_checked_in_rulepack() -> Result<LoadedRulepack, RulepackError> {
    Ok(checked_in_rulepack().clone())
}

/// Loads a rulepack from an on-disk manifest file, verifying digests and
/// validating all sections.
pub fn load_rulepack_from_manifest(
    manifest_path: impl AsRef<Path>,
) -> Result<LoadedRulepack, RulepackError> {
    let manifest_path = manifest_path.as_ref().to_path_buf();
    let manifest_raw = read_raw_file(&manifest_path)?;
    load_rulepack_from_raw(&manifest_path, &manifest_raw, |section_path| {
        read_raw_file(section_path)
    })
}

fn load_embedded_rulepack() -> Result<LoadedRulepack, RulepackError> {
    load_rulepack_from_raw(
        Path::new(CHECKED_IN_MANIFEST_FILE),
        CHECKED_IN_MANIFEST_RAW,
        |section_path| {
            embedded_section_raw(section_path.to_str().unwrap_or_default())
                .map(|raw| raw.to_vec())
                .ok_or_else(|| invalid_rulepack(section_path, "embedded section not found"))
        },
    )
}

fn load_rulepack_from_raw<F>(
    manifest_path: &Path,
    manifest_raw: &[u8],
    mut read_section: F,
) -> Result<LoadedRulepack, RulepackError>
where
    F: FnMut(&Path) -> Result<Vec<u8>, RulepackError>,
{
    let manifest: RulepackManifest = parse_json(manifest_path, manifest_raw)?;
    validate_manifest(&manifest, manifest_path)?;

    let manifest_dir = manifest_path.parent().unwrap_or(Path::new("."));
    let mut enrich = None;
    let mut residual = None;
    let mut render = None;

    for section in &manifest.sections {
        let section_path = manifest_dir.join(&section.path);
        let raw = read_section(&section_path)?;
        let actual_digest = hex_sha256(&raw);
        if actual_digest != section.sha256 {
            return Err(RulepackError::DigestMismatch {
                path: section_path,
                expected: section.sha256.clone(),
                actual: actual_digest,
            });
        }

        match section.kind {
            SectionKind::Enrich => {
                let parsed: EnrichRulepack = parse_json(&section_path, &raw)?;
                validate_enrich_rulepack(&parsed, &section_path, &manifest.rulepack_version)?;
                enrich = Some(parsed);
            }
            SectionKind::Residual => {
                let parsed: ResidualRulepack = parse_json(&section_path, &raw)?;
                validate_residual_rulepack(&parsed, &section_path, &manifest.rulepack_version)?;
                residual = Some(parsed);
            }
            SectionKind::Render => {
                let parsed: RenderRulepack = parse_json(&section_path, &raw)?;
                validate_render_rulepack(&parsed, &section_path, &manifest.rulepack_version)?;
                render = Some(parsed);
            }
        }
    }

    Ok(LoadedRulepack {
        manifest,
        enrich: enrich.ok_or_else(|| {
            invalid_rulepack(manifest_path, "manifest did not resolve an enrich section")
        })?,
        residual: residual.ok_or_else(|| {
            invalid_rulepack(manifest_path, "manifest did not resolve a residual section")
        })?,
        render: render.ok_or_else(|| {
            invalid_rulepack(manifest_path, "manifest did not resolve a render section")
        })?,
    })
}

fn embedded_section_raw(path: &str) -> Option<&'static [u8]> {
    match path {
        "enrich.rulepack.json" => Some(CHECKED_IN_ENRICH_RAW),
        "residual.rulepack.json" => Some(CHECKED_IN_RESIDUAL_RAW),
        "render.rulepack.json" => Some(CHECKED_IN_RENDER_RAW),
        _ => None,
    }
}

fn read_raw_file(path: &Path) -> Result<Vec<u8>, RulepackError> {
    fs::read(path).map_err(|source| RulepackError::ReadFile {
        path: path.to_path_buf(),
        source,
    })
}

fn parse_json<T: DeserializeOwned>(path: &Path, raw: &[u8]) -> Result<T, RulepackError> {
    serde_json::from_slice(raw).map_err(|source| RulepackError::ParseJson {
        path: path.to_path_buf(),
        source,
    })
}

fn validate_manifest(manifest: &RulepackManifest, path: &Path) -> Result<(), RulepackError> {
    ensure_schema_version(
        &manifest.schema_version,
        RULEPACK_MANIFEST_SCHEMA_VERSION,
        path,
    )?;
    ensure_version_id(&manifest.rulepack_version, path, "rulepack_version")?;
    if manifest.sections.is_empty() {
        return Err(invalid_rulepack(
            path,
            "manifest must include at least one section",
        ));
    }

    let mut section_kinds = BTreeSet::new();
    let mut section_paths = BTreeSet::new();
    for section in &manifest.sections {
        if !section_kinds.insert(section.kind) {
            return Err(invalid_rulepack(
                path,
                "manifest contains duplicate section kinds",
            ));
        }
        if !section_paths.insert(section.path.as_str()) {
            return Err(invalid_rulepack(
                path,
                "manifest contains duplicate section paths",
            ));
        }
        ensure_relative_json_path(&section.path, path)?;
        ensure_sha256_hex(&section.sha256, path, &section.path)?;
    }
    Ok(())
}

fn validate_enrich_rulepack(
    rulepack: &EnrichRulepack,
    path: &Path,
    expected_version: &str,
) -> Result<(), RulepackError> {
    validate_section_header(
        &rulepack.schema_version,
        &rulepack.rulepack_version,
        ENRICH_RULEPACK_SCHEMA_VERSION,
        path,
        expected_version,
    )?;
    ensure_non_empty(
        &rulepack.ingress_specific_override_rule_id,
        path,
        "ingress_specific_override_rule_id",
    )?;
    ensure_fallback_rule(&rulepack.unknown_fallback, path, "unknown_fallback")?;
    ensure_fallback_rule(&rulepack.passthrough_fallback, path, "passthrough_fallback")?;
    if rulepack.rules.is_empty() {
        return Err(invalid_rulepack(
            path,
            "checked-in enrich rulepack must define at least one family rule",
        ));
    }

    let mut families = BTreeSet::new();
    let mut rule_ids = BTreeSet::new();
    for rule in &rulepack.rules {
        if !families.insert(rule.family.as_str()) {
            return Err(invalid_rulepack(
                path,
                format!(
                    "duplicate family rule in checked-in enrich rulepack: {}",
                    rule.family
                ),
            ));
        }
        if !rule_ids.insert(rule.rule_id.as_str()) {
            return Err(invalid_rulepack(
                path,
                format!(
                    "duplicate rule id in checked-in enrich rulepack: {}",
                    rule.rule_id
                ),
            ));
        }
        ensure_non_empty(&rule.rule_id, path, "rule.rule_id")?;
        ensure_non_empty(&rule.family, path, "rule.family")?;
        ensure_non_empty_groups(&rule.message_groups, path, "rule.message_groups")?;
        ensure_non_empty_groups(
            &rule.child_message_groups,
            path,
            "rule.child_message_groups",
        )?;
        ensure_non_empty_strings(
            &rule.candidate_child_terms,
            path,
            "rule.candidate_child_terms",
        )?;
        ensure_conditions_non_empty(&rule.contexts, path, "rule.contexts")?;
        ensure_conditions_non_empty(&rule.child_notes, path, "rule.child_notes")?;
        if let Some(condition) = &rule.symbol_context_condition {
            ensure_non_empty(condition, path, "rule.symbol_context_condition")?;
        }
        if let Some(condition) = &rule.candidate_child_condition {
            ensure_non_empty(condition, path, "rule.candidate_child_condition")?;
        }
        if let Some(condition) = &rule.semantic_role_condition {
            ensure_non_empty(condition, path, "rule.semantic_role_condition")?;
        }
        for annotation in &rule.phase_annotations {
            ensure_non_empty(
                &annotation.condition,
                path,
                "rule.phase_annotations.condition",
            )?;
        }
    }

    let mut confidence_families = BTreeSet::new();
    for policy in &rulepack.confidence_policies {
        let family = policy.family.as_deref().ok_or_else(|| {
            invalid_rulepack(path, "family confidence policies must name a family")
        })?;
        if !confidence_families.insert(family) {
            return Err(invalid_rulepack(
                path,
                format!("duplicate confidence policy in checked-in enrich rulepack: {family}"),
            ));
        }
        validate_confidence_policy(policy, path)?;
    }
    validate_confidence_policy(&rulepack.default_confidence_policy, path)?;

    for family in [
        "syntax",
        "type_overload",
        "template",
        "macro_include",
        "linker",
    ] {
        if !rulepack
            .confidence_policies
            .iter()
            .any(|policy| policy.family.as_deref() == Some(family))
        {
            return Err(invalid_rulepack(
                path,
                format!("missing confidence policy in checked-in enrich rulepack: {family}"),
            ));
        }
    }
    Ok(())
}

fn validate_residual_rulepack(
    rulepack: &ResidualRulepack,
    path: &Path,
    expected_version: &str,
) -> Result<(), RulepackError> {
    validate_section_header(
        &rulepack.schema_version,
        &rulepack.rulepack_version,
        RESIDUAL_RULEPACK_SCHEMA_VERSION,
        path,
        expected_version,
    )?;
    ensure_non_empty(
        &rulepack.wording.default_action_hint,
        path,
        "wording.default_action_hint",
    )?;
    ensure_non_empty(
        &rulepack.wording.generic_linker_symbol_headline_template,
        path,
        "wording.generic_linker_symbol_headline_template",
    )?;

    let mut headline_families = BTreeSet::new();
    let mut action_families = BTreeSet::new();
    let mut specific_families = BTreeSet::new();
    for entry in &rulepack.wording.generic_headlines {
        ensure_non_empty(&entry.family, path, "wording.generic_headlines.family")?;
        ensure_non_empty(&entry.text, path, "wording.generic_headlines.text")?;
        if !headline_families.insert(entry.family.as_str()) {
            return Err(invalid_rulepack(
                path,
                format!(
                    "duplicate generic headline family in residual rulepack: {}",
                    entry.family
                ),
            ));
        }
    }
    for entry in &rulepack.wording.generic_action_hints {
        ensure_non_empty(&entry.family, path, "wording.generic_action_hints.family")?;
        ensure_non_empty(&entry.text, path, "wording.generic_action_hints.text")?;
        if !action_families.insert(entry.family.as_str()) {
            return Err(invalid_rulepack(
                path,
                format!(
                    "duplicate generic action family in residual rulepack: {}",
                    entry.family
                ),
            ));
        }
    }
    if headline_families != action_families {
        return Err(invalid_rulepack(
            path,
            "generic headline/action family sets must stay aligned",
        ));
    }
    for entry in &rulepack.wording.specific_overrides {
        ensure_non_empty(&entry.family, path, "wording.specific_overrides.family")?;
        ensure_non_empty(
            &entry.headline_template,
            path,
            "wording.specific_overrides.headline_template",
        )?;
        ensure_non_empty(
            &entry.headline_without_symbol,
            path,
            "wording.specific_overrides.headline_without_symbol",
        )?;
        ensure_non_empty(
            &entry.first_action_hint,
            path,
            "wording.specific_overrides.first_action_hint",
        )?;
        if !specific_families.insert(entry.family.as_str()) {
            return Err(invalid_rulepack(
                path,
                format!(
                    "duplicate specific wording family in residual rulepack: {}",
                    entry.family
                ),
            ));
        }
    }

    let mut compiler_kinds = BTreeSet::new();
    for entry in &rulepack.residual.compiler_groups {
        if !compiler_kinds.insert(entry.kind) {
            return Err(invalid_rulepack(
                path,
                "duplicate compiler residual kind in checked-in residual rulepack",
            ));
        }
        ensure_non_empty(&entry.family, path, "compiler_group.family")?;
        ensure_non_empty(&entry.rule_id, path, "compiler_group.rule_id")?;
        ensure_non_empty(
            &entry.first_action_hint,
            path,
            "compiler_group.first_action_hint",
        )?;
        ensure_non_empty_strings(&entry.match_any, path, "compiler_group.match_any")?;
        if matches!(entry.headline_strategy, HeadlineStrategy::FixedText) {
            let Some(headline) = entry.headline.as_deref() else {
                return Err(invalid_rulepack(
                    path,
                    "fixed_text compiler residual seeds must include headline",
                ));
            };
            ensure_non_empty(headline, path, "compiler_group.headline")?;
        }
    }
    if !compiler_kinds.contains(&CompilerResidualKind::Unknown) {
        return Err(invalid_rulepack(
            path,
            "checked-in residual rulepack must include unknown compiler seed",
        ));
    }

    ensure_non_empty(
        &rulepack
            .residual
            .compiler_note_rules
            .candidate_numbered_prefix,
        path,
        "compiler_note_rules.candidate_numbered_prefix",
    )?;
    ensure_non_empty_strings(
        &rulepack.residual.compiler_note_rules.template_context_any,
        path,
        "compiler_note_rules.template_context_any",
    )?;
    ensure_non_empty_strings(
        &rulepack.residual.compiler_note_rules.candidate_contains,
        path,
        "compiler_note_rules.candidate_contains",
    )?;

    let mut linker_rules = BTreeMap::new();
    for entry in &rulepack.residual.linker_groups {
        if linker_rules.insert(entry.kind, entry).is_some() {
            return Err(invalid_rulepack(
                path,
                "duplicate linker residual kind in checked-in residual rulepack",
            ));
        }
        ensure_non_empty(&entry.family, path, "linker_group.family")?;
        ensure_non_empty(&entry.rule_id, path, "linker_group.rule_id")?;
        ensure_non_empty(
            &entry.headline_template,
            path,
            "linker_group.headline_template",
        )?;
        ensure_non_empty(
            &entry.first_action_hint,
            path,
            "linker_group.first_action_hint",
        )?;
        if entry.group_key.is_some() == entry.group_key_template.is_some() {
            return Err(invalid_rulepack(
                path,
                "linker residual rules must set exactly one of group_key/group_key_template",
            ));
        }
        if entry.match_regex.is_none() && entry.match_prefix.is_none() {
            return Err(invalid_rulepack(
                path,
                "linker residual rules must set match_regex or match_prefix",
            ));
        }
        if let Some(group_key) = &entry.group_key {
            ensure_non_empty(group_key, path, "linker_group.group_key")?;
        }
        if let Some(group_key_template) = &entry.group_key_template {
            ensure_non_empty(group_key_template, path, "linker_group.group_key_template")?;
        }
        if let Some(pattern) = &entry.match_regex {
            ensure_non_empty(pattern, path, "linker_group.match_regex")?;
            Regex::new(pattern).map_err(|error| {
                invalid_rulepack(
                    path,
                    format!("invalid linker residual regex `{pattern}`: {error}"),
                )
            })?;
        }
        if let Some(prefix) = &entry.match_prefix {
            ensure_non_empty(prefix, path, "linker_group.match_prefix")?;
        }
        if let Some(symbol_capture) = &entry.symbol_capture {
            ensure_non_empty(symbol_capture, path, "linker_group.symbol_capture")?;
        }
    }
    for kind in [
        LinkerResidualKind::UndefinedReference,
        LinkerResidualKind::MultipleDefinition,
        LinkerResidualKind::CannotFindLibrary,
        LinkerResidualKind::FileFormatOrRelocation,
        LinkerResidualKind::Collect2Summary,
        LinkerResidualKind::AssemblerError,
        LinkerResidualKind::DriverFatal,
        LinkerResidualKind::InternalCompilerErrorBanner,
    ] {
        if !linker_rules.contains_key(&kind) {
            return Err(invalid_rulepack(
                path,
                format!("missing grouped residual kind in checked-in residual rulepack: {kind:?}"),
            ));
        }
    }
    ensure_grouped_residual_contract(
        linker_rules[&LinkerResidualKind::UndefinedReference],
        "linker.undefined_reference",
        diag_core::Origin::Linker,
        Phase::Link,
        path,
    )?;
    ensure_grouped_residual_contract(
        linker_rules[&LinkerResidualKind::MultipleDefinition],
        "linker.multiple_definition",
        diag_core::Origin::Linker,
        Phase::Link,
        path,
    )?;
    ensure_grouped_residual_contract(
        linker_rules[&LinkerResidualKind::CannotFindLibrary],
        "linker.cannot_find_library",
        diag_core::Origin::Linker,
        Phase::Link,
        path,
    )?;
    ensure_grouped_residual_contract(
        linker_rules[&LinkerResidualKind::FileFormatOrRelocation],
        "linker.file_format_or_relocation",
        diag_core::Origin::Linker,
        Phase::Link,
        path,
    )?;
    ensure_grouped_residual_contract(
        linker_rules[&LinkerResidualKind::Collect2Summary],
        "collect2_summary",
        diag_core::Origin::Driver,
        Phase::Link,
        path,
    )?;
    ensure_grouped_residual_contract(
        linker_rules[&LinkerResidualKind::AssemblerError],
        "assembler_error",
        diag_core::Origin::ExternalTool,
        Phase::Assemble,
        path,
    )?;
    ensure_grouped_residual_contract(
        linker_rules[&LinkerResidualKind::DriverFatal],
        "driver_fatal",
        diag_core::Origin::Driver,
        Phase::Driver,
        path,
    )?;
    ensure_grouped_residual_contract(
        linker_rules[&LinkerResidualKind::InternalCompilerErrorBanner],
        "internal_compiler_error_banner",
        diag_core::Origin::Gcc,
        Phase::Unknown,
        path,
    )?;

    ensure_non_empty(
        &rulepack.residual.passthrough.family,
        path,
        "passthrough.family",
    )?;
    ensure_non_empty(
        &rulepack.residual.passthrough.rule_id,
        path,
        "passthrough.rule_id",
    )?;
    ensure_non_empty(
        &rulepack.residual.passthrough.headline,
        path,
        "passthrough.headline",
    )?;
    ensure_non_empty(
        &rulepack.residual.passthrough.first_action_hint,
        path,
        "passthrough.first_action_hint",
    )?;
    Ok(())
}

fn ensure_grouped_residual_contract(
    entry: &LinkerResidualSeed,
    expected_family: &str,
    expected_origin: diag_core::Origin,
    expected_phase: Phase,
    path: &Path,
) -> Result<(), RulepackError> {
    if entry.family != expected_family {
        return Err(invalid_rulepack(
            path,
            format!(
                "grouped residual kind {:?} must use family `{expected_family}`, got `{}`",
                entry.kind, entry.family
            ),
        ));
    }
    if entry.origin != expected_origin || entry.phase != expected_phase {
        return Err(invalid_rulepack(
            path,
            format!(
                "grouped residual kind {:?} must use origin `{:?}` and phase `{:?}`",
                entry.kind, expected_origin, expected_phase
            ),
        ));
    }
    Ok(())
}

fn validate_render_rulepack(
    rulepack: &RenderRulepack,
    path: &Path,
    expected_version: &str,
) -> Result<(), RulepackError> {
    validate_section_header(
        &rulepack.schema_version,
        &rulepack.rulepack_version,
        RENDER_RULEPACK_SCHEMA_VERSION,
        path,
        expected_version,
    )?;
    if rulepack.family_policies.is_empty() {
        return Err(invalid_rulepack(
            path,
            "checked-in render rulepack must define family_policies",
        ));
    }

    let mut seen_kinds = BTreeSet::new();
    for policy in &rulepack.family_policies {
        if policy.kind == RendererFamilyKind::Unknown {
            return Err(invalid_rulepack(
                path,
                "checked-in render rulepack must not define unknown family policies",
            ));
        }
        if !seen_kinds.insert(policy.kind) {
            return Err(invalid_rulepack(
                path,
                format!(
                    "duplicate renderer family policy in checked-in render rulepack: {:?}",
                    policy.kind
                ),
            ));
        }
        if policy.match_exact.is_some() == policy.match_prefix.is_some() {
            return Err(invalid_rulepack(
                path,
                "renderer family policy must set exactly one of match_exact/match_prefix",
            ));
        }
        if let Some(match_exact) = policy.match_exact.as_deref() {
            ensure_non_empty(match_exact, path, "render.match_exact")?;
        }
        if let Some(match_prefix) = policy.match_prefix.as_deref() {
            ensure_non_empty(match_prefix, path, "render.match_prefix")?;
        }
        if let Some(exclude_exact) = policy.exclude_exact.as_deref() {
            ensure_non_empty(exclude_exact, path, "render.exclude_exact")?;
        }
    }
    Ok(())
}

fn validate_section_header(
    schema_version: &str,
    rulepack_version: &str,
    expected_schema: &str,
    path: &Path,
    expected_version: &str,
) -> Result<(), RulepackError> {
    ensure_schema_version(schema_version, expected_schema, path)?;
    ensure_version_id(rulepack_version, path, "rulepack_version")?;
    if rulepack_version != expected_version {
        return Err(invalid_rulepack(
            path,
            format!(
                "section rulepack_version {} does not match manifest {}",
                rulepack_version, expected_version
            ),
        ));
    }
    Ok(())
}

fn generic_family_text<'a>(entries: &'a [FamilyText], family: &str) -> Option<&'a FamilyText> {
    entries.iter().find(|entry| {
        entry.family == family || (entry.family == "linker" && family.starts_with("linker."))
    })
}

fn ensure_fallback_rule(
    fallback: &FallbackRuleConfig,
    path: &Path,
    label: &str,
) -> Result<(), RulepackError> {
    ensure_non_empty(&fallback.family, path, &format!("{label}.family"))?;
    ensure_non_empty(&fallback.rule_id, path, &format!("{label}.rule_id"))?;
    ensure_non_empty(
        &fallback.suppression_reason,
        path,
        &format!("{label}.suppression_reason"),
    )?;
    ensure_non_empty_strings(
        &fallback.matched_conditions,
        path,
        &format!("{label}.matched_conditions"),
    )?;
    Ok(())
}

fn validate_confidence_policy(
    policy: &ConfidencePolicyConfig,
    path: &Path,
) -> Result<(), RulepackError> {
    for clause in policy
        .high_when_any
        .iter()
        .chain(policy.medium_when_any.iter())
    {
        if clause.all.is_empty() {
            return Err(invalid_rulepack(
                path,
                "confidence clauses must include at least one signal",
            ));
        }
    }
    Ok(())
}

fn ensure_non_empty_groups(
    groups: &[TermGroupConfig],
    path: &Path,
    label: &str,
) -> Result<(), RulepackError> {
    for group in groups {
        ensure_non_empty(&group.prefix, path, &format!("{label}.prefix"))?;
        ensure_non_empty_strings(&group.terms, path, &format!("{label}.terms"))?;
    }
    Ok(())
}

fn ensure_conditions_non_empty<T>(
    conditions: &[T],
    path: &Path,
    label: &str,
) -> Result<(), RulepackError>
where
    T: ConditionField,
{
    for condition in conditions {
        ensure_non_empty(condition.condition(), path, label)?;
    }
    Ok(())
}

trait ConditionField {
    fn condition(&self) -> &str;
}

impl ConditionField for ContextConditionConfig {
    fn condition(&self) -> &str {
        &self.condition
    }
}

impl ConditionField for ChildNoteConditionConfig {
    fn condition(&self) -> &str {
        &self.condition
    }
}

fn ensure_non_empty_strings(
    values: &[String],
    path: &Path,
    label: &str,
) -> Result<(), RulepackError> {
    for value in values {
        ensure_non_empty(value, path, label)?;
    }
    Ok(())
}

fn ensure_schema_version(actual: &str, expected: &str, path: &Path) -> Result<(), RulepackError> {
    if actual == expected {
        Ok(())
    } else {
        Err(invalid_rulepack(
            path,
            format!("expected schema_version {expected}, got {actual}"),
        ))
    }
}

fn ensure_version_id(value: &str, path: &Path, field: &str) -> Result<(), RulepackError> {
    ensure_non_empty(value, path, field)?;
    let mut chars = value.chars();
    let Some(first) = chars.next() else {
        unreachable!("ensure_non_empty returned ok for an empty string");
    };
    if !first.is_ascii_lowercase()
        || !chars.all(|ch| {
            ch.is_ascii_lowercase() || ch.is_ascii_digit() || matches!(ch, '.' | '_' | '-')
        })
    {
        return Err(invalid_rulepack(
            path,
            format!(
                "{field} must start with a lowercase ASCII letter and contain only lowercase ASCII letters, digits, '.', '_' or '-'"
            ),
        ));
    }
    Ok(())
}

fn ensure_non_empty(value: &str, path: &Path, field: &str) -> Result<(), RulepackError> {
    if value.trim().is_empty() {
        Err(invalid_rulepack(path, format!("{field} must be non-empty")))
    } else {
        Ok(())
    }
}

fn ensure_relative_json_path(value: &str, path: &Path) -> Result<(), RulepackError> {
    if value.trim().is_empty() {
        return Err(invalid_rulepack(path, "section paths must be non-empty"));
    }
    let section_path = Path::new(value);
    if section_path.is_absolute() {
        return Err(invalid_rulepack(path, "section paths must be relative"));
    }
    for component in section_path.components() {
        if !matches!(component, Component::Normal(_)) {
            return Err(invalid_rulepack(
                path,
                "section paths must be normalized relative JSON paths",
            ));
        }
    }
    if section_path.extension().and_then(|ext| ext.to_str()) != Some("json") {
        return Err(invalid_rulepack(
            path,
            "section paths must reference JSON files",
        ));
    }
    Ok(())
}

fn ensure_sha256_hex(value: &str, path: &Path, label: &str) -> Result<(), RulepackError> {
    if value.len() == 64 && value.chars().all(|ch| ch.is_ascii_hexdigit()) {
        Ok(())
    } else {
        Err(invalid_rulepack(
            path,
            format!("{label} must be a 64-character SHA-256 hex digest"),
        ))
    }
}

fn hex_sha256(raw: &[u8]) -> String {
    let digest = Sha256::digest(raw);
    let mut rendered = String::with_capacity(digest.len() * 2);
    for byte in digest {
        rendered.push_str(&format!("{byte:02x}"));
    }
    rendered
}

fn invalid_rulepack(path: &Path, message: impl Into<String>) -> RulepackError {
    RulepackError::InvalidRulepack {
        path: path.to_path_buf(),
        message: message.into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn copy_checked_in_rulepack(temp_dir: &TempDir) -> PathBuf {
        for file_name in CHECKED_IN_SECTION_FILES {
            fs::copy(
                checked_in_rules_dir().join(file_name),
                temp_dir.path().join(file_name),
            )
            .unwrap();
        }
        temp_dir.path().join(CHECKED_IN_MANIFEST_FILE)
    }

    #[test]
    fn loads_checked_in_phase1_rulepack() {
        let rulepack = checked_in_rulepack();
        assert_eq!(rulepack.version(), CHECKED_IN_RULEPACK_VERSION);
        assert_eq!(rulepack.manifest().sections.len(), 3);
        assert_eq!(
            rulepack.enrich().rule("syntax").rule_id,
            "rule.family.syntax.phase_or_message"
        );
        assert_eq!(
            rulepack
                .residual()
                .compiler_seed(CompilerResidualKind::Template)
                .headline
                .as_deref(),
            Some("template instantiation failed")
        );
        assert!(
            rulepack
                .render()
                .policy_for_kind(RendererFamilyKind::Linker)
                .is_some()
        );
        assert!(
            rulepack
                .residual()
                .residual
                .linker_groups
                .iter()
                .any(|entry| entry.kind == LinkerResidualKind::DriverFatal)
        );
        assert!(
            rulepack
                .residual()
                .residual
                .linker_groups
                .iter()
                .any(|entry| entry.kind == LinkerResidualKind::Collect2Summary)
        );
    }

    #[test]
    fn on_disk_loader_matches_embedded_rulepack() {
        let temp_dir = TempDir::new().unwrap();
        let manifest_path = copy_checked_in_rulepack(&temp_dir);
        let loaded = load_rulepack_from_manifest(manifest_path).unwrap();
        assert_eq!(loaded, checked_in_rulepack().clone());
    }

    #[test]
    fn rejects_section_digest_mismatch() {
        let temp_dir = TempDir::new().unwrap();
        let manifest_path = copy_checked_in_rulepack(&temp_dir);
        let mut manifest: RulepackManifest =
            serde_json::from_slice(&fs::read(&manifest_path).unwrap()).unwrap();
        manifest.sections[0].sha256 = "0".repeat(64);
        fs::write(
            &manifest_path,
            serde_json::to_vec_pretty(&manifest).unwrap(),
        )
        .unwrap();

        let error = load_rulepack_from_manifest(&manifest_path).unwrap_err();
        assert!(matches!(error, RulepackError::DigestMismatch { .. }));
    }

    #[test]
    fn rejects_mixed_section_version_ids() {
        let temp_dir = TempDir::new().unwrap();
        let manifest_path = copy_checked_in_rulepack(&temp_dir);
        let residual_path = temp_dir.path().join("residual.rulepack.json");
        let mut residual: ResidualRulepack =
            serde_json::from_slice(&fs::read(&residual_path).unwrap()).unwrap();
        residual.rulepack_version = "phase0".to_string();
        let residual_raw = serde_json::to_vec_pretty(&residual).unwrap();
        fs::write(&residual_path, &residual_raw).unwrap();

        let mut manifest: RulepackManifest =
            serde_json::from_slice(&fs::read(&manifest_path).unwrap()).unwrap();
        manifest
            .sections
            .iter_mut()
            .find(|section| section.path == "residual.rulepack.json")
            .unwrap()
            .sha256 = hex_sha256(&residual_raw);
        fs::write(
            &manifest_path,
            serde_json::to_vec_pretty(&manifest).unwrap(),
        )
        .unwrap();

        let error = load_rulepack_from_manifest(&manifest_path).unwrap_err();
        match error {
            RulepackError::InvalidRulepack { message, .. } => {
                assert!(message.contains("does not match manifest"));
            }
            other => panic!("unexpected error: {other:?}"),
        }
    }

    #[test]
    fn rejects_invalid_manifest_version_id() {
        let temp_dir = TempDir::new().unwrap();
        let manifest_path = copy_checked_in_rulepack(&temp_dir);
        let mut manifest: RulepackManifest =
            serde_json::from_slice(&fs::read(&manifest_path).unwrap()).unwrap();
        manifest.rulepack_version = "Phase 1".to_string();
        fs::write(
            &manifest_path,
            serde_json::to_vec_pretty(&manifest).unwrap(),
        )
        .unwrap();

        let error = load_rulepack_from_manifest(&manifest_path).unwrap_err();
        match error {
            RulepackError::InvalidRulepack { message, .. } => {
                assert!(
                    message.contains("rulepack_version must start with a lowercase ASCII letter")
                );
            }
            other => panic!("unexpected error: {other:?}"),
        }
    }

    #[test]
    fn rejects_non_normalized_section_paths() {
        let temp_dir = TempDir::new().unwrap();
        let manifest_path = copy_checked_in_rulepack(&temp_dir);
        let mut manifest: RulepackManifest =
            serde_json::from_slice(&fs::read(&manifest_path).unwrap()).unwrap();
        manifest.sections[0].path = "./enrich.rulepack.json".to_string();
        fs::write(
            &manifest_path,
            serde_json::to_vec_pretty(&manifest).unwrap(),
        )
        .unwrap();

        let error = load_rulepack_from_manifest(&manifest_path).unwrap_err();
        match error {
            RulepackError::InvalidRulepack { message, .. } => {
                assert!(message.contains("normalized relative JSON paths"));
            }
            other => panic!("unexpected error: {other:?}"),
        }
    }

    #[test]
    fn rejects_missing_required_grouped_residual_kind() {
        let temp_dir = TempDir::new().unwrap();
        let manifest_path = copy_checked_in_rulepack(&temp_dir);
        let residual_path = temp_dir.path().join("residual.rulepack.json");
        let mut residual: ResidualRulepack =
            serde_json::from_slice(&fs::read(&residual_path).unwrap()).unwrap();
        residual
            .residual
            .linker_groups
            .retain(|entry| entry.kind != LinkerResidualKind::DriverFatal);
        let residual_raw = serde_json::to_vec_pretty(&residual).unwrap();
        fs::write(&residual_path, &residual_raw).unwrap();

        let mut manifest: RulepackManifest =
            serde_json::from_slice(&fs::read(&manifest_path).unwrap()).unwrap();
        manifest
            .sections
            .iter_mut()
            .find(|section| section.path == "residual.rulepack.json")
            .unwrap()
            .sha256 = hex_sha256(&residual_raw);
        fs::write(
            &manifest_path,
            serde_json::to_vec_pretty(&manifest).unwrap(),
        )
        .unwrap();

        let error = load_rulepack_from_manifest(&manifest_path).unwrap_err();
        match error {
            RulepackError::InvalidRulepack { message, .. } => {
                assert!(message.contains("missing grouped residual kind"));
                assert!(message.contains("DriverFatal"));
            }
            other => panic!("unexpected error: {other:?}"),
        }
    }

    #[test]
    fn rejects_collect2_summary_with_linker_origin() {
        let temp_dir = TempDir::new().unwrap();
        let manifest_path = copy_checked_in_rulepack(&temp_dir);
        let residual_path = temp_dir.path().join("residual.rulepack.json");
        let mut residual: ResidualRulepack =
            serde_json::from_slice(&fs::read(&residual_path).unwrap()).unwrap();
        residual
            .residual
            .linker_groups
            .iter_mut()
            .find(|entry| entry.kind == LinkerResidualKind::Collect2Summary)
            .unwrap()
            .origin = diag_core::Origin::Linker;
        let residual_raw = serde_json::to_vec_pretty(&residual).unwrap();
        fs::write(&residual_path, &residual_raw).unwrap();

        let mut manifest: RulepackManifest =
            serde_json::from_slice(&fs::read(&manifest_path).unwrap()).unwrap();
        manifest
            .sections
            .iter_mut()
            .find(|section| section.path == "residual.rulepack.json")
            .unwrap()
            .sha256 = hex_sha256(&residual_raw);
        fs::write(
            &manifest_path,
            serde_json::to_vec_pretty(&manifest).unwrap(),
        )
        .unwrap();

        let error = load_rulepack_from_manifest(&manifest_path).unwrap_err();
        match error {
            RulepackError::InvalidRulepack { message, .. } => {
                assert!(message.contains("Collect2Summary"));
                assert!(message.contains("origin `Driver` and phase `Link`"));
            }
            other => panic!("unexpected error: {other:?}"),
        }
    }
}
