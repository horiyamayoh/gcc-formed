use diag_core::DiagnosticNode;

#[derive(Debug, Clone, Copy)]
struct HeadlineRule {
    family: &'static str,
    headline: &'static str,
}

const HEADLINE_RULES: &[HeadlineRule] = &[
    HeadlineRule {
        family: "syntax",
        headline: "syntax error",
    },
    HeadlineRule {
        family: "type_overload",
        headline: "type or overload mismatch",
    },
    HeadlineRule {
        family: "template",
        headline: "template instantiation failed",
    },
    HeadlineRule {
        family: "macro_include",
        headline: "error surfaced through macro/include context",
    },
    HeadlineRule {
        family: "linker",
        headline: "linker reported a failure",
    },
    HeadlineRule {
        family: "passthrough",
        headline: "showing conservative wrapper view",
    },
];

pub(crate) fn headline_for(node: &DiagnosticNode, family: &str) -> String {
    match family {
        "linker.undefined_reference" => node
            .symbol_context
            .as_ref()
            .and_then(|symbol| symbol.primary_symbol.clone())
            .map(|symbol| format!("undefined reference to `{symbol}`"))
            .unwrap_or_else(|| "undefined reference reported by linker".to_string()),
        "linker.multiple_definition" => node
            .symbol_context
            .as_ref()
            .and_then(|symbol| symbol.primary_symbol.clone())
            .map(|symbol| format!("multiple definition of `{symbol}`"))
            .unwrap_or_else(|| "duplicate symbol definition reported by linker".to_string()),
        _ => HEADLINE_RULES
            .iter()
            .find(|rule| {
                rule.family == family || (rule.family == "linker" && family.starts_with("linker."))
            })
            .map(|rule| {
                if rule.family == "linker" {
                    node.symbol_context
                        .as_ref()
                        .and_then(|symbol| symbol.primary_symbol.clone())
                        .map(|symbol| format!("linker failed to resolve `{symbol}`"))
                        .unwrap_or_else(|| rule.headline.to_string())
                } else {
                    rule.headline.to_string()
                }
            })
            .unwrap_or_else(|| {
                node.message
                    .raw_text
                    .lines()
                    .next()
                    .unwrap_or("diagnostic")
                    .to_string()
            }),
    }
}
