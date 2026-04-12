use diag_core::DiagnosticNode;
use diag_rulepack::{
    FamilyText, HeadlineFallbackStrategy, ResidualRulepack, SpecificWordingOverride,
    checked_in_rulepack,
};
use std::collections::{HashMap, HashSet};
use std::sync::OnceLock;

pub(crate) fn headline_for(node: &DiagnosticNode, family: &str) -> String {
    if let Some(override_rule) = specific_wording_override(family) {
        return render_specific_headline(node, override_rule);
    }

    if preserves_existing_ingress_wording(family) {
        preserved_specific_headline(node, family).unwrap_or_else(|| generic_headline(node, family))
    } else {
        generic_headline(node, family)
    }
}

pub(crate) fn generic_action_hint_rule(family: &str) -> Option<&'static str> {
    generic_action_hints().get(family).copied().or_else(|| {
        if family.starts_with("linker.") && family != "linker" {
            generic_action_hints().get("linker").copied()
        } else {
            None
        }
    })
}

pub(crate) fn specific_action_hint_rule(family: &str) -> Option<&'static str> {
    specific_wording_override(family).map(|entry| entry.first_action_hint.as_str())
}

pub(crate) fn specific_wording_override(family: &str) -> Option<&'static SpecificWordingOverride> {
    specific_wording_overrides().get(family).copied()
}

pub(crate) fn default_action_hint() -> &'static str {
    wording_rulepack().wording.default_action_hint.as_str()
}

fn wording_rulepack() -> &'static ResidualRulepack {
    checked_in_rulepack().residual()
}

pub(crate) fn preserves_existing_ingress_wording(family: &str) -> bool {
    family.contains('.') || !is_broad_enrich_family(family)
}

pub(crate) fn is_broad_enrich_family(family: &str) -> bool {
    broad_enrich_families().contains(family)
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
    let exact = generic_family_texts(entries).get(family).copied();
    exact.or_else(|| {
        if family.starts_with("linker.") && family != "linker" {
            generic_family_texts(entries).get("linker").copied()
        } else {
            None
        }
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
        return analysis.headline.as_ref().map(|c| c.clone().into_owned());
    }
    None
}

fn broad_enrich_families() -> &'static HashSet<&'static str> {
    static BROAD_FAMILIES: OnceLock<HashSet<&'static str>> = OnceLock::new();
    BROAD_FAMILIES.get_or_init(|| {
        let rulepack = checked_in_rulepack().enrich();
        let mut families = HashSet::with_capacity(rulepack.rules.len() + 2);
        for rule in &rulepack.rules {
            families.insert(rule.family.as_str());
        }
        families.insert(rulepack.unknown_fallback.family.as_str());
        families.insert(rulepack.passthrough_fallback.family.as_str());
        families
    })
}

fn specific_wording_overrides() -> &'static HashMap<&'static str, &'static SpecificWordingOverride>
{
    static SPECIFIC_OVERRIDES: OnceLock<HashMap<&'static str, &'static SpecificWordingOverride>> =
        OnceLock::new();
    SPECIFIC_OVERRIDES.get_or_init(|| {
        wording_rulepack()
            .wording
            .specific_overrides
            .iter()
            .map(|entry| (entry.family.as_str(), entry))
            .collect()
    })
}

fn generic_action_hints() -> &'static HashMap<&'static str, &'static str> {
    static GENERIC_ACTION_HINTS: OnceLock<HashMap<&'static str, &'static str>> = OnceLock::new();
    GENERIC_ACTION_HINTS.get_or_init(|| {
        wording_rulepack()
            .wording
            .generic_action_hints
            .iter()
            .map(|entry| (entry.family.as_str(), entry.text.as_str()))
            .collect()
    })
}

fn generic_family_texts(
    entries: &'static [FamilyText],
) -> &'static HashMap<&'static str, &'static FamilyText> {
    static GENERIC_HEADLINES: OnceLock<HashMap<&'static str, &'static FamilyText>> =
        OnceLock::new();
    match entries.as_ptr() == wording_rulepack().wording.generic_headlines.as_ptr() {
        true => GENERIC_HEADLINES.get_or_init(|| {
            wording_rulepack()
                .wording
                .generic_headlines
                .iter()
                .map(|entry| (entry.family.as_str(), entry))
                .collect()
        }),
        false => {
            unreachable!("generic_family_texts is only used with checked-in generic headlines")
        }
    }
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
        assert!(std::ptr::eq(rulepack, checked_in_rulepack().residual()));
        assert_eq!(
            generic_action_hint_rule("syntax"),
            Some("fix the first parser error at the user-owned location")
        );
        assert!(specific_wording_override("linker.undefined_reference").is_some());
    }
}
