use diag_core::ContextChainKind;

pub(crate) fn option_context_kind(option: &str) -> Option<ContextChainKind> {
    if option.starts_with("-Wtemplate-") {
        Some(ContextChainKind::TemplateInstantiation)
    } else if option == "-fanalyzer" || option.starts_with("-Wanalyzer-") {
        Some(ContextChainKind::AnalyzerPath)
    } else if option.starts_with("-Wmacro-") {
        Some(ContextChainKind::MacroExpansion)
    } else {
        None
    }
}

pub(crate) fn text_context_kinds(message: &str) -> Vec<ContextChainKind> {
    let mut kinds = Vec::new();
    if message_has_template_context(message) {
        kinds.push(ContextChainKind::TemplateInstantiation);
    }
    if message_has_macro_context(message) {
        kinds.push(ContextChainKind::MacroExpansion);
    }
    if message_has_include_context(message) {
        kinds.push(ContextChainKind::Include);
    }
    kinds
}

pub(crate) fn metadata_context_kinds(seed: &str) -> Vec<ContextChainKind> {
    let lowered = seed.trim().to_lowercase();
    let mut kinds = Vec::new();
    if lowered.is_empty() {
        return kinds;
    }
    if lowered.contains("template")
        || lowered.contains("instantiat")
        || lowered.contains("substitution")
    {
        kinds.push(ContextChainKind::TemplateInstantiation);
    }
    if lowered.contains("analyzer") {
        kinds.push(ContextChainKind::AnalyzerPath);
    }
    if lowered.contains("macro") {
        kinds.push(ContextChainKind::MacroExpansion);
    }
    if lowered.contains("include") || lowered.contains("header") {
        kinds.push(ContextChainKind::Include);
    }
    kinds
}

pub(crate) fn extend_unique_context_kinds(
    kinds: &mut Vec<ContextChainKind>,
    additional: Vec<ContextChainKind>,
) {
    for kind in additional {
        if !kinds.contains(&kind) {
            kinds.push(kind);
        }
    }
}

pub(crate) fn message_has_template_context(message: &str) -> bool {
    let lowered = message.to_lowercase();
    lowered.contains("template")
        || lowered.contains("required from")
        || lowered.contains("required by substitution")
        || lowered.contains("deduction/substitution")
        || lowered.contains("deduced conflicting")
        || lowered.contains("in instantiation of")
        || lowered.contains("in substitution of")
}

pub(crate) fn message_has_macro_context(message: &str) -> bool {
    message.to_lowercase().contains("macro")
}

pub(crate) fn message_has_include_context(message: &str) -> bool {
    let lowered = message.to_lowercase();
    if !lowered.contains("include") {
        return false;
    }
    if lowered.contains("does not include")
        || lowered.contains("did not include")
        || lowered.contains("not included")
    {
        return false;
    }

    let trimmed = message.trim_start();
    trimmed.starts_with("In file included from ")
        || trimmed.starts_with("from ")
        || lowered.contains("included from")
        || lowered.contains("#include")
        || lowered.contains("including file")
}
