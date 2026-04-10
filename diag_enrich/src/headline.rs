use diag_core::DiagnosticNode;
use serde::Deserialize;
use std::collections::BTreeSet;
use std::sync::OnceLock;

const WORDING_RULEPACK_JSON: &str = include_str!("../../rules/residual.rulepack.json");
const WORDING_RULEPACK_SCHEMA_VERSION: &str = "diag_residual_rulepack/v1alpha1";

static WORDING_RULEPACK: OnceLock<WordingRulepackRoot> = OnceLock::new();

#[derive(Debug, Deserialize)]
struct WordingRulepackRoot {
    schema_version: String,
    rulepack_version: String,
    wording: WordingSection,
}

#[derive(Debug, Deserialize)]
struct WordingSection {
    default_headline_strategy: HeadlineFallbackStrategy,
    default_action_hint: String,
    generic_linker_symbol_headline_template: String,
    generic_headlines: Vec<FamilyText>,
    generic_action_hints: Vec<FamilyText>,
    specific_overrides: Vec<SpecificWordingOverride>,
}

#[derive(Debug, Deserialize)]
struct FamilyText {
    family: String,
    text: String,
}

#[derive(Debug, Deserialize)]
pub(crate) struct SpecificWordingOverride {
    family: String,
    headline_template: String,
    headline_without_symbol: String,
    first_action_hint: String,
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum HeadlineFallbackStrategy {
    FirstMessageLine,
}

pub(crate) fn headline_for(node: &DiagnosticNode, family: &str) -> String {
    if let Some(override_rule) = specific_wording_override(family) {
        return render_specific_headline(node, override_rule);
    }

    if family.contains('.') {
        preserved_specific_headline(node, family).unwrap_or_else(|| generic_headline(node, family))
    } else {
        generic_headline(node, family)
    }
}

pub(crate) fn generic_action_hint_rule(family: &str) -> Option<&'static str> {
    generic_family_text(&wording_rulepack().wording.generic_action_hints, family)
        .map(|entry| entry.text.as_str())
}

pub(crate) fn specific_action_hint_rule(family: &str) -> Option<&'static str> {
    specific_wording_override(family).map(|entry| entry.first_action_hint.as_str())
}

pub(crate) fn specific_wording_override(family: &str) -> Option<&'static SpecificWordingOverride> {
    wording_rulepack()
        .wording
        .specific_overrides
        .iter()
        .find(|entry| entry.family == family)
}

pub(crate) fn default_action_hint() -> &'static str {
    wording_rulepack().wording.default_action_hint.as_str()
}

fn wording_rulepack() -> &'static WordingRulepackRoot {
    WORDING_RULEPACK.get_or_init(load_wording_rulepack)
}

fn load_wording_rulepack() -> WordingRulepackRoot {
    let rulepack: WordingRulepackRoot = serde_json::from_str(WORDING_RULEPACK_JSON)
        .expect("checked-in residual.rulepack.json must parse");
    rulepack.validate();
    rulepack
}

impl WordingRulepackRoot {
    fn validate(&self) {
        assert_eq!(
            self.schema_version, WORDING_RULEPACK_SCHEMA_VERSION,
            "checked-in residual rulepack schema_version drifted"
        );
        assert!(
            !self.rulepack_version.trim().is_empty(),
            "checked-in residual rulepack_version must be non-empty"
        );

        let mut headline_families = BTreeSet::new();
        let mut action_families = BTreeSet::new();
        let mut specific_families = BTreeSet::new();

        for entry in &self.wording.generic_headlines {
            assert!(
                headline_families.insert(entry.family.as_str()),
                "duplicate generic headline family in residual rulepack: {}",
                entry.family
            );
        }
        for entry in &self.wording.generic_action_hints {
            assert!(
                action_families.insert(entry.family.as_str()),
                "duplicate generic action family in residual rulepack: {}",
                entry.family
            );
        }
        for entry in &self.wording.specific_overrides {
            assert!(
                specific_families.insert(entry.family.as_str()),
                "duplicate specific wording family in residual rulepack: {}",
                entry.family
            );
        }

        assert_eq!(
            headline_families, action_families,
            "generic headline/action family sets must stay aligned"
        );
    }
}

fn render_specific_headline(node: &DiagnosticNode, rule: &SpecificWordingOverride) -> String {
    node.symbol_context
        .as_ref()
        .and_then(|symbol| symbol.primary_symbol.as_deref())
        .map(|symbol| render_template(&rule.headline_template, "symbol", symbol))
        .unwrap_or_else(|| rule.headline_without_symbol.clone())
}

fn generic_headline(node: &DiagnosticNode, family: &str) -> String {
    generic_family_text(&wording_rulepack().wording.generic_headlines, family)
        .map(|entry| {
            if family == "linker" || family.starts_with("linker.") {
                node.symbol_context
                    .as_ref()
                    .and_then(|symbol| symbol.primary_symbol.as_deref())
                    .map(|symbol| {
                        render_template(
                            &wording_rulepack()
                                .wording
                                .generic_linker_symbol_headline_template,
                            "symbol",
                            symbol,
                        )
                    })
                    .unwrap_or_else(|| entry.text.clone())
            } else {
                entry.text.clone()
            }
        })
        .unwrap_or_else(|| default_headline(node))
}

fn generic_family_text(
    entries: &'static [FamilyText],
    family: &str,
) -> Option<&'static FamilyText> {
    entries.iter().find(|entry| {
        entry.family == family || (entry.family == "linker" && family.starts_with("linker."))
    })
}

fn default_headline(node: &DiagnosticNode) -> String {
    match wording_rulepack().wording.default_headline_strategy {
        HeadlineFallbackStrategy::FirstMessageLine => node
            .message
            .raw_text
            .lines()
            .next()
            .unwrap_or("diagnostic")
            .to_string(),
    }
}

fn preserved_specific_headline(node: &DiagnosticNode, family: &str) -> Option<String> {
    let analysis = node.analysis.as_ref()?;
    let existing_family = analysis.family.as_deref()?;
    if existing_family == family && specific_wording_override(family).is_none() {
        return analysis.headline.clone();
    }
    None
}

fn render_template(template: &str, key: &str, value: &str) -> String {
    template.replace(&format!("{{{key}}}"), value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loads_checked_in_wording_rulepack() {
        let rulepack = wording_rulepack();
        assert_eq!(rulepack.rulepack_version, "phase1");
        assert_eq!(
            generic_action_hint_rule("syntax"),
            Some("fix the first parser error at the user-owned location")
        );
        assert!(specific_wording_override("linker.undefined_reference").is_some());
    }
}
