use diag_core::{
    ContextChainKind, DiagnosticDocument, DiagnosticNode, Ownership, fingerprint_for,
    normalize_message,
};

/// Fixed-width line bucket size used for same-file candidate prefiltering.
pub const PRIMARY_LINE_BUCKET_WIDTH: u32 = 8;

/// Canonical representative chosen for one logical group.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnchorSource {
    /// The group anchor came from the node primary location.
    PrimaryLocation,
    /// The group anchor came from the first template-instantiation frontier.
    TemplateFrontier,
    /// The group anchor came from the first macro-expansion frontier.
    MacroFrontier,
    /// The group anchor came from the first include frontier.
    IncludeFrontier,
    /// No path anchor was available; only symbol context remained.
    SymbolContext,
    /// Neither path nor symbol context was available.
    MessageOnly,
}

/// Stable anchor material used to key a logical group.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CanonicalAnchor {
    /// Source that won canonical anchor selection.
    pub source: AnchorSource,
    /// Normalized path key when a path anchor exists.
    pub path_key: Option<String>,
    /// 1-based line number when available.
    pub line: Option<u32>,
    /// 1-based column number when available.
    pub column: Option<u32>,
}

/// Deterministic key material derived from one top-level diagnostic.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct GroupKeySet {
    /// Normalized raw file identity of the group primary location.
    pub primary_file_key: Option<String>,
    /// Fixed-width bucket of the primary line number.
    pub primary_line_bucket: Option<u32>,
    /// Normalized translation-unit identity for the group.
    pub translation_unit_key: Option<String>,
    /// Stable origin/phase discriminator used by candidate prefiltering.
    pub origin_phase_key: String,
    /// Normalized symbol identity when linker or symbol facts are present.
    pub symbol_key: Option<String>,
    /// Family assigned during enrichment, or `unknown`.
    pub family_key: String,
    /// Effective ownership label for the canonical anchor.
    pub ownership_key: String,
    /// Compiler-message key normalized across structured/native-text paths.
    pub normalized_message_key: String,
    /// First template frontier key, if any.
    pub template_frontier_key: Option<String>,
    /// First macro frontier key, if any.
    pub macro_frontier_key: Option<String>,
    /// First include frontier key, if any.
    pub include_frontier_key: Option<String>,
    /// Stable top-level ordinal from `document.diagnostics`.
    pub ordinal_in_invocation: usize,
}

/// One deterministic logical group derived from one top-level diagnostic.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogicalGroup {
    /// Stable group id derived from canonical key material.
    pub group_ref: String,
    /// Optional hint already attached to the node analysis.
    pub hint_group_ref: Option<String>,
    /// Lead node id for this group.
    pub lead_node_id: String,
    /// Index into `DiagnosticDocument.diagnostics`.
    pub node_index: usize,
    /// Canonical anchor used for path/line keyed matching.
    pub canonical_anchor: CanonicalAnchor,
    /// Canonical key material for later relation scoring.
    pub keys: GroupKeySet,
}

/// Extract deterministic logical groups from the top-level diagnostics in a document.
pub fn extract_logical_groups(document: &DiagnosticDocument) -> Vec<LogicalGroup> {
    let mut seen_group_refs = std::collections::BTreeMap::<String, usize>::new();
    let mut groups = Vec::with_capacity(document.diagnostics.len());

    for (node_index, node) in document.diagnostics.iter().enumerate() {
        let keys = derive_group_keys(node, node_index);
        let canonical_anchor = derive_canonical_anchor(node, &keys);
        let base_group_ref = canonical_group_ref(&keys);
        let counter = seen_group_refs.entry(base_group_ref.clone()).or_default();
        *counter += 1;
        let group_ref = if *counter == 1 {
            base_group_ref
        } else {
            format!("{base_group_ref}-{}", counter)
        };

        groups.push(LogicalGroup {
            group_ref,
            hint_group_ref: hint_group_ref(node),
            lead_node_id: node.id.clone(),
            node_index,
            canonical_anchor,
            keys,
        });
    }

    groups
}

/// Derive stable key material from one top-level diagnostic node.
pub fn derive_group_keys(node: &DiagnosticNode, ordinal_in_invocation: usize) -> GroupKeySet {
    let primary_location = node.primary_location();
    let primary_file_key = primary_location.map(|location| normalize_path_key(location.path_raw()));
    let primary_line_bucket = primary_location.map(|location| line_bucket(location.line()));
    let translation_unit_key = primary_file_key
        .as_deref()
        .map(translation_unit_key_from_path)
        .or_else(|| first_frontier_path_key(node).map(|path| translation_unit_key_from_path(&path)))
        .or_else(|| related_object_translation_unit_key(node));

    let template_frontier_key = frontier_key(node, ContextChainKind::TemplateInstantiation);
    let macro_frontier_key = frontier_key(node, ContextChainKind::MacroExpansion);
    let include_frontier_key = frontier_key(node, ContextChainKind::Include);

    GroupKeySet {
        primary_file_key,
        primary_line_bucket,
        translation_unit_key,
        origin_phase_key: format!("{}:{}", origin_key(node), phase_key(node)),
        symbol_key: symbol_key(node),
        family_key: family_key(node),
        ownership_key: ownership_key(node),
        normalized_message_key: normalized_message_key(node),
        template_frontier_key,
        macro_frontier_key,
        include_frontier_key,
        ordinal_in_invocation,
    }
}

/// Pick the canonical anchor for one top-level diagnostic node.
pub fn derive_canonical_anchor(node: &DiagnosticNode, keys: &GroupKeySet) -> CanonicalAnchor {
    if let Some(location) = node.primary_location() {
        return CanonicalAnchor {
            source: AnchorSource::PrimaryLocation,
            path_key: keys.primary_file_key.clone(),
            line: Some(location.line()),
            column: Some(location.column()),
        };
    }
    if let Some(anchor) = frontier_anchor(node, ContextChainKind::TemplateInstantiation) {
        return anchor;
    }
    if let Some(anchor) = frontier_anchor(node, ContextChainKind::MacroExpansion) {
        return anchor;
    }
    if let Some(anchor) = frontier_anchor(node, ContextChainKind::Include) {
        return anchor;
    }
    if keys.symbol_key.is_some() {
        return CanonicalAnchor {
            source: AnchorSource::SymbolContext,
            path_key: None,
            line: None,
            column: None,
        };
    }
    CanonicalAnchor {
        source: AnchorSource::MessageOnly,
        path_key: None,
        line: None,
        column: None,
    }
}

/// Compute the stable base group ref from canonical key material.
pub fn canonical_group_ref(keys: &GroupKeySet) -> String {
    let digest = fingerprint_for(&(
        keys.primary_file_key.clone(),
        keys.primary_line_bucket,
        keys.translation_unit_key.clone(),
        keys.origin_phase_key.clone(),
        keys.symbol_key.clone(),
        keys.family_key.clone(),
        keys.ownership_key.clone(),
        keys.normalized_message_key.clone(),
        keys.template_frontier_key.clone(),
        keys.macro_frontier_key.clone(),
        keys.include_frontier_key.clone(),
    ));
    format!("group-{}", &digest[..12])
}

fn hint_group_ref(node: &DiagnosticNode) -> Option<String> {
    node.analysis
        .as_ref()
        .and_then(|analysis| analysis.group_ref.as_deref())
        .map(str::trim)
        .filter(|group_ref| !group_ref.is_empty())
        .map(ToOwned::to_owned)
}

fn family_key(node: &DiagnosticNode) -> String {
    node.analysis
        .as_ref()
        .and_then(|analysis| analysis.family.as_deref())
        .map(str::trim)
        .filter(|family| !family.is_empty())
        .unwrap_or("unknown")
        .to_string()
}

fn ownership_key(node: &DiagnosticNode) -> String {
    match node
        .primary_location()
        .and_then(|location| location.ownership())
        .copied()
    {
        Some(Ownership::User) => "user",
        Some(Ownership::Vendor) => "vendor",
        Some(Ownership::System) => "system",
        Some(Ownership::Generated) => "generated",
        Some(Ownership::Tool) => "tool",
        Some(Ownership::Unknown) | None => "unknown",
    }
    .to_string()
}

fn normalized_message_key(node: &DiagnosticNode) -> String {
    if let Some(normalized) = node
        .message
        .normalized_text
        .as_deref()
        .map(str::trim)
        .filter(|normalized| !normalized.is_empty())
    {
        return normalized.to_string();
    }

    let first_line = node
        .message
        .raw_text
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .unwrap_or("");
    let without_severity_prefix = strip_severity_prefix(first_line);
    let linker_core = strip_linker_prefix(without_severity_prefix);
    let without_warning_option = strip_trailing_warning_option(linker_core);
    normalize_message(without_warning_option)
}

fn strip_severity_prefix(message: &str) -> &str {
    for severity in ["fatal error", "error", "warning", "note", "remark", "info"] {
        let marker = format!(": {severity}: ");
        if let Some((_, suffix)) = message.rsplit_once(&marker) {
            return suffix.trim();
        }
    }
    message
}

fn strip_trailing_warning_option(message: &str) -> &str {
    let Some(without_bracket) = message.strip_suffix(']') else {
        return message;
    };
    let Some((prefix, suffix)) = without_bracket.rsplit_once(" [") else {
        return message;
    };
    if suffix.starts_with("-W") || suffix.starts_with("-f") {
        prefix.trim_end()
    } else {
        message
    }
}

fn strip_linker_prefix(message: &str) -> &str {
    for marker in [
        "undefined reference to",
        "multiple definition of",
        "first defined here",
        "cannot find -l",
    ] {
        if let Some(start) = message.find(marker) {
            return message[start..].trim();
        }
    }
    message
}

fn symbol_key(node: &DiagnosticNode) -> Option<String> {
    node.symbol_context
        .as_ref()
        .and_then(|symbol_context| symbol_context.primary_symbol.as_deref())
        .map(normalize_symbol_key)
        .filter(|symbol| !symbol.is_empty())
        .or_else(|| extract_symbol_from_message(&node.message.raw_text))
}

fn normalize_symbol_key(symbol: &str) -> String {
    symbol
        .trim()
        .trim_matches('`')
        .trim_matches('\'')
        .trim()
        .to_string()
}

fn extract_symbol_from_message(raw_text: &str) -> Option<String> {
    let first_line = raw_text
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())?;
    if !(first_line.contains("undefined reference")
        || first_line.contains("multiple definition")
        || first_line.contains("first defined here"))
    {
        return None;
    }

    extract_quoted_segment(first_line)
        .map(normalize_symbol_key)
        .filter(|symbol| !symbol.is_empty())
}

fn extract_quoted_segment(message: &str) -> Option<&str> {
    for quote in ['`', '\''] {
        let start = message.find(quote)?;
        let suffix = &message[start + quote.len_utf8()..];
        let end = suffix.find(quote)?;
        if end > 0 {
            return Some(&suffix[..end]);
        }
    }
    None
}

fn frontier_key(node: &DiagnosticNode, kind: ContextChainKind) -> Option<String> {
    let (path, line, _) = first_frontier_frame(node, kind)?;
    Some(frontier_key_from_parts(&path, line))
}

fn first_frontier_path_key(node: &DiagnosticNode) -> Option<String> {
    for kind in [
        ContextChainKind::TemplateInstantiation,
        ContextChainKind::MacroExpansion,
        ContextChainKind::Include,
    ] {
        if let Some((path, _, _)) = first_frontier_frame(node, kind) {
            return Some(path);
        }
    }
    None
}

fn frontier_anchor(node: &DiagnosticNode, kind: ContextChainKind) -> Option<CanonicalAnchor> {
    let (path, line, column) = first_frontier_frame(node, kind.clone())?;
    Some(CanonicalAnchor {
        source: match kind {
            ContextChainKind::TemplateInstantiation => AnchorSource::TemplateFrontier,
            ContextChainKind::MacroExpansion => AnchorSource::MacroFrontier,
            ContextChainKind::Include => AnchorSource::IncludeFrontier,
            _ => return None,
        },
        path_key: Some(path),
        line,
        column,
    })
}

fn first_frontier_frame(
    node: &DiagnosticNode,
    kind: ContextChainKind,
) -> Option<(String, Option<u32>, Option<u32>)> {
    node.context_chains
        .iter()
        .find(|chain| chain.kind == kind)
        .and_then(|chain| {
            chain.frames.iter().find_map(|frame| {
                frame.path.as_deref().map(|path| {
                    (
                        normalize_path_key(path),
                        frame.line.filter(|line| *line >= 1),
                        frame.column.filter(|column| *column >= 1),
                    )
                })
            })
        })
}

fn frontier_key_from_parts(path: &str, line: Option<u32>) -> String {
    match line {
        Some(line) => format!("{path}:{line}"),
        None => path.to_string(),
    }
}

fn related_object_translation_unit_key(node: &DiagnosticNode) -> Option<String> {
    let symbol_context = node.symbol_context.as_ref()?;
    let mut related_objects = symbol_context.related_objects.clone();
    related_objects.sort();
    related_objects
        .into_iter()
        .find(|path| !path.trim().is_empty())
        .map(|path| translation_unit_key_from_path(&normalize_path_key(&path)))
}

fn translation_unit_key_from_path(path: &str) -> String {
    let Some((directory, filename)) = path.rsplit_once('/') else {
        return strip_filename_extension(path).to_string();
    };
    let stem = strip_filename_extension(filename);
    if directory.is_empty() {
        stem.to_string()
    } else {
        format!("{directory}/{stem}")
    }
}

fn strip_filename_extension(filename: &str) -> &str {
    match filename.rsplit_once('.') {
        Some((stem, extension)) if !stem.is_empty() && !extension.is_empty() => stem,
        _ => filename,
    }
}

fn origin_key(node: &DiagnosticNode) -> &'static str {
    match node.origin {
        diag_core::Origin::Gcc => "gcc",
        diag_core::Origin::Clang => "clang",
        diag_core::Origin::Linker => "linker",
        diag_core::Origin::Driver => "driver",
        diag_core::Origin::Wrapper => "wrapper",
        diag_core::Origin::ExternalTool => "external_tool",
        diag_core::Origin::Unknown => "unknown",
    }
}

fn phase_key(node: &DiagnosticNode) -> &'static str {
    match node.phase {
        diag_core::Phase::Driver => "driver",
        diag_core::Phase::Preprocess => "preprocess",
        diag_core::Phase::Parse => "parse",
        diag_core::Phase::Semantic => "semantic",
        diag_core::Phase::Instantiate => "instantiate",
        diag_core::Phase::Constraints => "constraints",
        diag_core::Phase::Analyze => "analyze",
        diag_core::Phase::Optimize => "optimize",
        diag_core::Phase::Codegen => "codegen",
        diag_core::Phase::Assemble => "assemble",
        diag_core::Phase::Link => "link",
        diag_core::Phase::Archive => "archive",
        diag_core::Phase::Unknown => "unknown",
    }
}

fn line_bucket(line: u32) -> u32 {
    (line.saturating_sub(1)) / PRIMARY_LINE_BUCKET_WIDTH
}

fn normalize_path_key(path: &str) -> String {
    let replaced = path.trim().replace('\\', "/");
    let absolute = replaced.starts_with('/');
    let mut parts = Vec::new();

    for segment in replaced.split('/') {
        match segment {
            "" | "." => {}
            ".." => {
                if let Some(last) = parts.last()
                    && last != ".."
                {
                    parts.pop();
                } else if !absolute {
                    parts.push("..".to_string());
                }
            }
            other => parts.push(other.to_string()),
        }
    }

    let joined = parts.join("/");
    match (absolute, joined.is_empty()) {
        (true, true) => "/".to_string(),
        (true, false) => format!("/{joined}"),
        (false, true) => ".".to_string(),
        (false, false) => joined,
    }
}
