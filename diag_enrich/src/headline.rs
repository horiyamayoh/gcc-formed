use diag_core::DiagnosticNode;
use diag_rulepack::{
    FamilyText, HeadlineFallbackStrategy, ResidualRulepack, SpecificWordingOverride,
    checked_in_rulepack,
};

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
    wording_rulepack().generic_action_hint(family)
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

fn wording_rulepack() -> &'static ResidualRulepack {
    checked_in_rulepack().residual()
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
        return analysis.headline.as_ref().map(|c| c.clone().into_owned());
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
        assert!(std::ptr::eq(rulepack, checked_in_rulepack().residual()));
        assert_eq!(
            generic_action_hint_rule("syntax"),
            Some("fix the first parser error at the user-owned location")
        );
        assert!(specific_wording_override("linker.undefined_reference").is_some());
    }
}
