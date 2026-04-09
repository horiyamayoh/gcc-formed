use diag_core::DiagnosticNode;

#[derive(Debug, Clone, Copy)]
struct ActionHintRule {
    family: &'static str,
    first_action_hint: &'static str,
}

const ACTION_HINT_RULES: &[ActionHintRule] = &[
    ActionHintRule {
        family: "syntax",
        first_action_hint: "fix the first parser error at the user-owned location",
    },
    ActionHintRule {
        family: "type_overload",
        first_action_hint: "compare the expected type and actual argument at the call site",
    },
    ActionHintRule {
        family: "template",
        first_action_hint: "start from the first user-owned template frame and match template arguments",
    },
    ActionHintRule {
        family: "macro_include",
        first_action_hint: "inspect the user-owned macro invocation or include edge that reaches the failing line",
    },
    ActionHintRule {
        family: "linker",
        first_action_hint: "check the missing/duplicate symbol and the object or library inputs",
    },
    ActionHintRule {
        family: "passthrough",
        first_action_hint: "inspect the preserved raw diagnostics for the first corrective action",
    },
];

pub(crate) fn action_hint_for(node: &DiagnosticNode, family: &str) -> String {
    match family {
        "linker.undefined_reference" => {
            "define the missing symbol or link the object/library that provides it".to_string()
        }
        "linker.multiple_definition" => {
            "remove the duplicate definition or make the symbol internal to one translation unit"
                .to_string()
        }
        _ if family.contains('.') => preserved_specific_action_hint(node, family)
            .unwrap_or_else(|| generic_action_hint(family)),
        _ => generic_action_hint(family),
    }
}

fn generic_action_hint(family: &str) -> String {
    ACTION_HINT_RULES
        .iter()
        .find(|rule| {
            rule.family == family || (rule.family == "linker" && family.starts_with("linker."))
        })
        .map(|rule| rule.first_action_hint.to_string())
        .unwrap_or_else(|| {
            "inspect the preserved raw diagnostics for the first corrective action".to_string()
        })
}

fn preserved_specific_action_hint(node: &DiagnosticNode, family: &str) -> Option<String> {
    let analysis = node.analysis.as_ref()?;
    let existing_family = analysis.family.as_deref()?;
    if existing_family == family && !has_local_specific_override(family) {
        return analysis.first_action_hint.clone();
    }
    None
}

fn has_local_specific_override(family: &str) -> bool {
    matches!(
        family,
        "linker.undefined_reference" | "linker.multiple_definition"
    )
}
