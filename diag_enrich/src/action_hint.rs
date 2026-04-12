use crate::headline::{
    default_action_hint, generic_action_hint_rule, preserves_existing_ingress_wording,
    specific_action_hint_rule, specific_wording_override,
};
use diag_core::DiagnosticNode;

pub(crate) fn action_hint_for(node: &DiagnosticNode, family: &str) -> String {
    if let Some(action_hint) = specific_action_hint_rule(family) {
        return action_hint.to_string();
    }

    if preserves_existing_ingress_wording(family) {
        preserved_specific_action_hint(node, family).unwrap_or_else(|| generic_action_hint(family))
    } else {
        generic_action_hint(family)
    }
}

fn generic_action_hint(family: &str) -> String {
    generic_action_hint_rule(family)
        .map(ToString::to_string)
        .unwrap_or_else(|| default_action_hint().to_string())
}

fn preserved_specific_action_hint(node: &DiagnosticNode, family: &str) -> Option<String> {
    let analysis = node.analysis.as_ref()?;
    let existing_family = analysis.family.as_deref()?;
    if existing_family == family && specific_wording_override(family).is_none() {
        return analysis
            .first_action_hint
            .as_ref()
            .map(|c| c.clone().into_owned());
    }
    None
}
