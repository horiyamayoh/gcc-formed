//! Residual, render, and implementation types for the diagnostic pipeline.
//!
//! This module contains the residual text classification types, rendering
//! policy types, implementation blocks, and private helpers.

use crate::{ConfidencePolicyConfig, EnrichRulepack, FamilyRuleConfig};
use diag_core::Phase;
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Residual section types
// ---------------------------------------------------------------------------

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
    /// Preprocessor directive or include failure residual.
    Preprocess,
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

// ---------------------------------------------------------------------------
// Render section types
// ---------------------------------------------------------------------------

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

// ---------------------------------------------------------------------------
// impl blocks
// ---------------------------------------------------------------------------

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

// ---------------------------------------------------------------------------
// Private helpers
// ---------------------------------------------------------------------------

fn generic_family_text<'a>(entries: &'a [FamilyText], family: &str) -> Option<&'a FamilyText> {
    entries.iter().find(|entry| {
        entry.family == family || (entry.family == "linker" && family.starts_with("linker."))
    })
}
