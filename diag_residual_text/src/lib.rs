use diag_core::{
    AnalysisOverlay, Confidence, ContextChain, ContextChainKind, DiagnosticNode, Location,
    MessageText, NodeCompleteness, Origin, Phase, Provenance, ProvenanceSource, SemanticRole,
    Severity, SymbolContext,
};
use regex::Regex;
use serde::Deserialize;
use std::collections::{BTreeMap, BTreeSet, btree_map::Entry};
use std::sync::OnceLock;

const RESIDUAL_RULEPACK_JSON: &str = include_str!("../../rules/residual.rulepack.json");
const RESIDUAL_RULEPACK_SCHEMA_VERSION: &str = "diag_residual_rulepack/v1alpha1";

static RESIDUAL_RULEPACK: OnceLock<ResidualRulepackRoot> = OnceLock::new();

#[derive(Debug, Deserialize)]
struct ResidualRulepackRoot {
    schema_version: String,
    rulepack_version: String,
    residual: ResidualSection,
}

#[derive(Debug, Deserialize)]
struct ResidualSection {
    compiler_groups: Vec<CompilerResidualSeed>,
    compiler_note_rules: CompilerNoteRules,
    linker_groups: Vec<LinkerResidualSeed>,
    passthrough: PassthroughResidualSeed,
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
enum CompilerResidualKind {
    Syntax,
    Template,
    TypeOverload,
    Unknown,
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum HeadlineStrategy {
    FixedText,
    MessagePassthrough,
}

#[derive(Debug, Deserialize)]
struct CompilerResidualSeed {
    kind: CompilerResidualKind,
    family: String,
    phase: Phase,
    rule_id: String,
    headline_strategy: HeadlineStrategy,
    headline: Option<String>,
    first_action_hint: String,
    #[serde(default)]
    match_any: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct CompilerNoteRules {
    #[serde(default)]
    template_context_any: Vec<String>,
    #[serde(default)]
    candidate_contains: Vec<String>,
    candidate_numbered_prefix: String,
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
enum LinkerResidualKind {
    UndefinedReference,
    MultipleDefinition,
    CannotFindLibrary,
    FileFormatOrRelocation,
    Collect2Error,
    AssemblerError,
}

#[derive(Debug, Deserialize)]
struct LinkerResidualSeed {
    kind: LinkerResidualKind,
    family: String,
    origin: Origin,
    phase: Phase,
    rule_id: String,
    #[serde(default)]
    group_key: Option<String>,
    #[serde(default)]
    group_key_template: Option<String>,
    #[serde(default)]
    match_regex: Option<String>,
    #[serde(default)]
    match_prefix: Option<String>,
    #[serde(default)]
    requires_colon: bool,
    #[serde(default)]
    symbol_capture: Option<String>,
    headline_template: String,
    first_action_hint: String,
}

#[derive(Debug, Deserialize)]
struct PassthroughResidualSeed {
    family: String,
    phase: Phase,
    rule_id: String,
    headline: String,
    first_action_hint: String,
}

#[derive(Debug)]
struct CompilerResidualBlock {
    node: Option<DiagnosticNode>,
    raw_lines: Vec<String>,
}

#[derive(Debug)]
struct GroupedResidual {
    rule: &'static LinkerResidualSeed,
    template_values: BTreeMap<String, String>,
    lines: Vec<String>,
}

#[derive(Debug)]
struct CompiledLinkerSeed {
    rule: &'static LinkerResidualSeed,
    regex: Option<Regex>,
}

#[derive(Debug)]
struct LinkerMatch {
    rule: &'static LinkerResidualSeed,
    group_key: String,
    template_values: BTreeMap<String, String>,
}

pub fn classify(stderr: &str, include_passthrough: bool) -> Vec<DiagnosticNode> {
    let mut grouped = BTreeMap::<String, GroupedResidual>::new();
    let mut compiler_nodes = Vec::new();
    let mut passthrough = Vec::new();
    let mut compiler_block = None::<CompilerResidualBlock>;
    let compiler_diagnostic = Regex::new(
        r"^(?P<path>[[:alnum:]_./+-]+):(?P<line>\d+):(?P<column>\d+): (?P<severity>fatal error|error|warning|note): (?P<message>.+)$",
    )
    .expect("regex");
    let linker_matchers = compiled_linker_seeds();

    for line in stderr.lines().filter(|line| !line.trim().is_empty()) {
        if let Some(capture) = compiler_diagnostic.captures(line) {
            if include_passthrough {
                ingest_compiler_diagnostic_line(
                    &mut compiler_nodes,
                    &mut passthrough,
                    &mut compiler_block,
                    line,
                    &capture,
                );
            }
            continue;
        }

        flush_compiler_block(&mut compiler_nodes, &mut passthrough, &mut compiler_block);

        if let Some(linker_match) = match_linker_group(line, &linker_matchers) {
            match grouped.entry(linker_match.group_key) {
                Entry::Occupied(mut entry) => {
                    entry.get_mut().lines.push(line.to_string());
                }
                Entry::Vacant(entry) => {
                    entry.insert(GroupedResidual {
                        rule: linker_match.rule,
                        template_values: linker_match.template_values,
                        lines: vec![line.to_string()],
                    });
                }
            }
            continue;
        }

        passthrough.push(line.to_string());
    }
    flush_compiler_block(&mut compiler_nodes, &mut passthrough, &mut compiler_block);

    let mut nodes = compiler_nodes;
    let grouped_base_index = nodes.len();
    for (index, (key, group)) in grouped.into_iter().enumerate() {
        nodes.push(group_to_node(
            grouped_base_index + index,
            &key,
            group.rule,
            &group.template_values,
            &group.lines,
        ));
    }
    if include_passthrough && !passthrough.is_empty() {
        nodes.push(passthrough_node(&passthrough));
    }
    nodes
}

fn residual_rulepack() -> &'static ResidualRulepackRoot {
    RESIDUAL_RULEPACK.get_or_init(load_residual_rulepack)
}

fn load_residual_rulepack() -> ResidualRulepackRoot {
    let rulepack: ResidualRulepackRoot = serde_json::from_str(RESIDUAL_RULEPACK_JSON)
        .expect("checked-in residual.rulepack.json must parse");
    rulepack.validate();
    rulepack
}

impl ResidualRulepackRoot {
    fn validate(&self) {
        assert_eq!(
            self.schema_version, RESIDUAL_RULEPACK_SCHEMA_VERSION,
            "checked-in residual rulepack schema_version drifted"
        );
        assert!(
            !self.rulepack_version.trim().is_empty(),
            "checked-in residual rulepack_version must be non-empty"
        );

        let mut compiler_kinds = BTreeSet::new();
        for entry in &self.residual.compiler_groups {
            assert!(
                compiler_kinds.insert(entry.kind),
                "duplicate compiler residual kind in checked-in residual rulepack"
            );
            assert!(
                !entry.family.trim().is_empty(),
                "compiler residual family must be non-empty"
            );
            assert!(
                !entry.rule_id.trim().is_empty(),
                "compiler residual rule_id must be non-empty"
            );
            assert!(
                !entry.first_action_hint.trim().is_empty(),
                "compiler residual first_action_hint must be non-empty"
            );
            if matches!(entry.headline_strategy, HeadlineStrategy::FixedText) {
                assert!(
                    entry
                        .headline
                        .as_deref()
                        .is_some_and(|headline| !headline.trim().is_empty()),
                    "fixed_text compiler residual seeds must include headline"
                );
            }
        }
        assert!(
            compiler_kinds.contains(&CompilerResidualKind::Unknown),
            "checked-in residual rulepack must include unknown compiler seed"
        );

        assert!(
            !self
                .residual
                .compiler_note_rules
                .candidate_numbered_prefix
                .trim()
                .is_empty(),
            "compiler_note_rules.candidate_numbered_prefix must be non-empty"
        );

        let mut linker_kinds = BTreeSet::new();
        for entry in &self.residual.linker_groups {
            assert!(
                linker_kinds.insert(entry.kind),
                "duplicate linker residual kind in checked-in residual rulepack"
            );
            assert!(
                !entry.family.trim().is_empty(),
                "linker residual family must be non-empty"
            );
            assert!(
                !entry.rule_id.trim().is_empty(),
                "linker residual rule_id must be non-empty"
            );
            assert!(
                !entry.headline_template.trim().is_empty(),
                "linker residual headline_template must be non-empty"
            );
            assert!(
                !entry.first_action_hint.trim().is_empty(),
                "linker residual first_action_hint must be non-empty"
            );
            assert!(
                entry.group_key.is_some() ^ entry.group_key_template.is_some(),
                "linker residual rules must set exactly one of group_key/group_key_template"
            );
            assert!(
                entry.match_regex.is_some() || entry.match_prefix.is_some(),
                "linker residual rules must set match_regex or match_prefix"
            );
            if let Some(pattern) = &entry.match_regex {
                Regex::new(pattern).unwrap_or_else(|error| {
                    panic!("invalid linker residual regex `{pattern}`: {error}")
                });
            }
        }

        assert!(
            !self.residual.passthrough.family.trim().is_empty(),
            "passthrough family must be non-empty"
        );
        assert!(
            !self.residual.passthrough.rule_id.trim().is_empty(),
            "passthrough rule_id must be non-empty"
        );
        assert!(
            !self.residual.passthrough.headline.trim().is_empty(),
            "passthrough headline must be non-empty"
        );
        assert!(
            !self
                .residual
                .passthrough
                .first_action_hint
                .trim()
                .is_empty(),
            "passthrough first_action_hint must be non-empty"
        );
    }

    fn compiler_seed(&self, kind: CompilerResidualKind) -> &CompilerResidualSeed {
        self.residual
            .compiler_groups
            .iter()
            .find(|entry| entry.kind == kind)
            .unwrap_or_else(|| panic!("missing compiler residual seed for {kind:?}"))
    }
}

fn compiled_linker_seeds() -> Vec<CompiledLinkerSeed> {
    residual_rulepack()
        .residual
        .linker_groups
        .iter()
        .map(|rule| CompiledLinkerSeed {
            rule,
            regex: rule
                .match_regex
                .as_ref()
                .map(|pattern| Regex::new(pattern).expect("validated linker residual regex")),
        })
        .collect()
}

fn match_linker_group(line: &str, matchers: &[CompiledLinkerSeed]) -> Option<LinkerMatch> {
    for matcher in matchers {
        if matcher.rule.requires_colon && !line.contains(':') {
            continue;
        }
        if let Some(regex) = &matcher.regex {
            if let Some(capture) = regex.captures(line) {
                let template_values = capture_template_values(regex, &capture);
                return Some(LinkerMatch {
                    rule: matcher.rule,
                    group_key: linker_group_key(matcher.rule, &template_values),
                    template_values,
                });
            }
        }
        if let Some(prefix) = &matcher.rule.match_prefix {
            if line.starts_with(prefix) {
                return Some(LinkerMatch {
                    rule: matcher.rule,
                    group_key: linker_group_key(matcher.rule, &BTreeMap::new()),
                    template_values: BTreeMap::new(),
                });
            }
        }
    }
    None
}

fn capture_template_values(
    regex: &Regex,
    capture: &regex::Captures<'_>,
) -> BTreeMap<String, String> {
    let mut values = BTreeMap::new();
    for name in regex.capture_names().flatten() {
        if let Some(value) = capture.name(name) {
            values.insert(name.to_string(), value.as_str().to_string());
        }
    }
    values
}

fn linker_group_key(rule: &LinkerResidualSeed, values: &BTreeMap<String, String>) -> String {
    rule.group_key
        .as_deref()
        .map(ToString::to_string)
        .unwrap_or_else(|| render_template(&rule.group_key_template.clone().unwrap(), values))
}

fn ingest_compiler_diagnostic_line(
    compiler_nodes: &mut Vec<DiagnosticNode>,
    passthrough: &mut Vec<String>,
    current_block: &mut Option<CompilerResidualBlock>,
    line: &str,
    capture: &regex::Captures<'_>,
) {
    let severity_label = &capture["severity"];
    if severity_label == "note" {
        if let Some(block) = current_block.as_mut() {
            block.raw_lines.push(line.to_string());
            if let Some(node) = block.node.as_mut() {
                attach_compiler_note(node, line, capture);
                return;
            }
        } else {
            passthrough.push(line.to_string());
            return;
        }
        return;
    }

    flush_compiler_block(compiler_nodes, passthrough, current_block);
    let kind = compiler_residual_kind(&capture["message"]);
    let raw_lines = vec![line.to_string()];
    match kind {
        CompilerResidualKind::Unknown => {
            *current_block = Some(CompilerResidualBlock {
                node: None,
                raw_lines,
            });
        }
        _ => {
            *current_block = Some(CompilerResidualBlock {
                node: Some(compiler_diagnostic_node(
                    compiler_nodes.len(),
                    line,
                    capture,
                    kind,
                )),
                raw_lines,
            });
        }
    }
}

fn flush_compiler_block(
    compiler_nodes: &mut Vec<DiagnosticNode>,
    passthrough: &mut Vec<String>,
    current_block: &mut Option<CompilerResidualBlock>,
) {
    let Some(block) = current_block.take() else {
        return;
    };
    if let Some(node) = block.node {
        compiler_nodes.push(node);
    } else {
        passthrough.extend(block.raw_lines);
    }
}

fn compiler_diagnostic_node(
    index: usize,
    line: &str,
    capture: &regex::Captures<'_>,
    kind: CompilerResidualKind,
) -> DiagnosticNode {
    let message = capture["message"].to_string();
    let severity = match &capture["severity"] {
        "fatal error" | "error" => Severity::Error,
        "warning" => Severity::Warning,
        "note" => Severity::Note,
        _ => Severity::Unknown,
    };
    let seed = residual_rulepack().compiler_seed(kind);

    DiagnosticNode {
        id: format!("residual-compiler-{index}"),
        origin: Origin::Gcc,
        phase: seed.phase.clone(),
        severity,
        semantic_role: SemanticRole::Root,
        message: MessageText {
            raw_text: line.to_string(),
            normalized_text: None,
            locale: None,
        },
        locations: vec![compiler_location(capture)],
        children: Vec::new(),
        suggestions: Vec::new(),
        context_chains: Vec::new(),
        symbol_context: None,
        node_completeness: NodeCompleteness::Partial,
        provenance: Provenance {
            source: ProvenanceSource::ResidualText,
            capture_refs: vec!["stderr.raw".to_string()],
        },
        analysis: Some(AnalysisOverlay {
            family: Some(seed.family.clone()),
            headline: Some(compiler_headline(seed, &message)),
            first_action_hint: Some(seed.first_action_hint.clone()),
            confidence: Some(Confidence::Low),
            rule_id: Some(seed.rule_id.clone()),
            matched_conditions: vec![
                "residual_group=compiler_diagnostic".to_string(),
                format!("family={}", seed.family),
            ],
            suppression_reason: None,
            collapsed_child_ids: Vec::new(),
            collapsed_chain_ids: Vec::new(),
        }),
        fingerprints: None,
    }
}

fn compiler_residual_kind(message: &str) -> CompilerResidualKind {
    let lowered = message.to_lowercase();
    residual_rulepack()
        .residual
        .compiler_groups
        .iter()
        .find(|seed| {
            seed.kind != CompilerResidualKind::Unknown
                && seed
                    .match_any
                    .iter()
                    .any(|needle| lowered.contains(needle.as_str()))
        })
        .map(|seed| seed.kind)
        .unwrap_or(CompilerResidualKind::Unknown)
}

fn compiler_headline(seed: &CompilerResidualSeed, message: &str) -> String {
    match seed.headline_strategy {
        HeadlineStrategy::FixedText => seed
            .headline
            .clone()
            .expect("validated fixed_text compiler residual headline"),
        HeadlineStrategy::MessagePassthrough => message.to_string(),
    }
}

fn attach_compiler_note(node: &mut DiagnosticNode, line: &str, capture: &regex::Captures<'_>) {
    let message = capture["message"].to_string();
    let lowered = message.to_lowercase();
    let role = if is_candidate_message(&lowered) {
        SemanticRole::Candidate
    } else {
        SemanticRole::Supporting
    };
    let phase = if is_template_context_message(&lowered) {
        Phase::Instantiate
    } else {
        node.phase.clone()
    };

    if is_template_context_message(&lowered) {
        let template_seed = residual_rulepack().compiler_seed(CompilerResidualKind::Template);
        node.phase = template_seed.phase.clone();
        push_context_chain(node, line, capture);
        if let Some(analysis) = node.analysis.as_mut() {
            analysis.family = Some(template_seed.family.clone());
            analysis.headline = Some(compiler_headline(template_seed, &message));
            analysis.first_action_hint = Some(template_seed.first_action_hint.clone());
            analysis.rule_id = Some(template_seed.rule_id.clone());
            if !analysis
                .matched_conditions
                .iter()
                .any(|condition| condition == "family=template")
            {
                analysis
                    .matched_conditions
                    .push("family=template".to_string());
            }
        }
    }

    node.children.push(DiagnosticNode {
        id: format!("{}-child-{}", node.id, node.children.len() + 1),
        origin: Origin::Gcc,
        phase,
        severity: Severity::Note,
        semantic_role: role,
        message: MessageText {
            raw_text: line.to_string(),
            normalized_text: None,
            locale: None,
        },
        locations: vec![compiler_location(capture)],
        children: Vec::new(),
        suggestions: Vec::new(),
        context_chains: Vec::new(),
        symbol_context: None,
        node_completeness: NodeCompleteness::Partial,
        provenance: Provenance {
            source: ProvenanceSource::ResidualText,
            capture_refs: vec!["stderr.raw".to_string()],
        },
        analysis: None,
        fingerprints: None,
    });
}

fn push_context_chain(node: &mut DiagnosticNode, line: &str, capture: &regex::Captures<'_>) {
    let frame = diag_core::ContextFrame {
        label: capture["message"].to_string(),
        path: Some(capture["path"].to_string()),
        line: Some(capture["line"].parse().unwrap_or(1)),
        column: Some(capture["column"].parse().unwrap_or(1)),
    };
    if let Some(existing) = node
        .context_chains
        .iter_mut()
        .find(|chain| matches!(chain.kind, ContextChainKind::TemplateInstantiation))
    {
        existing.frames.push(frame);
    } else {
        node.context_chains.push(ContextChain {
            kind: ContextChainKind::TemplateInstantiation,
            frames: vec![frame],
        });
    }
    if !node
        .message
        .raw_text
        .lines()
        .any(|existing| existing == line)
    {
        node.message.raw_text.push('\n');
        node.message.raw_text.push_str(line);
    }
}

fn is_template_context_message(message: &str) -> bool {
    residual_rulepack()
        .residual
        .compiler_note_rules
        .template_context_any
        .iter()
        .any(|needle| message.contains(needle))
}

fn is_candidate_message(message: &str) -> bool {
    residual_rulepack()
        .residual
        .compiler_note_rules
        .candidate_contains
        .iter()
        .any(|needle| message.contains(needle))
        || is_numbered_candidate_message(
            message,
            &residual_rulepack()
                .residual
                .compiler_note_rules
                .candidate_numbered_prefix,
        )
}

fn is_numbered_candidate_message(message: &str, prefix: &str) -> bool {
    let Some(rest) = message.trim().strip_prefix(prefix) else {
        return false;
    };
    let digit_len = rest.chars().take_while(|ch| ch.is_ascii_digit()).count();
    digit_len > 0 && rest[digit_len..].starts_with(':')
}

fn compiler_location(capture: &regex::Captures<'_>) -> Location {
    Location {
        path: capture["path"].to_string(),
        line: capture["line"].parse().unwrap_or(1),
        column: capture["column"].parse().unwrap_or(1),
        end_line: None,
        end_column: None,
        display_path: None,
        ownership: None,
    }
}

fn group_to_node(
    index: usize,
    key: &str,
    rule: &LinkerResidualSeed,
    template_values: &BTreeMap<String, String>,
    lines: &[String],
) -> DiagnosticNode {
    DiagnosticNode {
        id: format!("residual-{index}"),
        origin: rule.origin.clone(),
        phase: rule.phase.clone(),
        severity: Severity::Error,
        semantic_role: SemanticRole::Root,
        message: MessageText {
            raw_text: lines.join("\n"),
            normalized_text: None,
            locale: None,
        },
        locations: parse_locations(lines),
        children: lines
            .iter()
            .enumerate()
            .skip(1)
            .map(|(child_index, line)| DiagnosticNode {
                id: format!("residual-{index}-child-{child_index}"),
                origin: Origin::Linker,
                phase: Phase::Link,
                severity: Severity::Note,
                semantic_role: SemanticRole::Supporting,
                message: MessageText {
                    raw_text: line.clone(),
                    normalized_text: None,
                    locale: None,
                },
                locations: Vec::new(),
                children: Vec::new(),
                suggestions: Vec::new(),
                context_chains: vec![ContextChain {
                    kind: ContextChainKind::LinkerResolution,
                    frames: Vec::new(),
                }],
                symbol_context: None,
                node_completeness: NodeCompleteness::Partial,
                provenance: Provenance {
                    source: ProvenanceSource::ResidualText,
                    capture_refs: vec!["stderr.raw".to_string()],
                },
                analysis: None,
                fingerprints: None,
            })
            .collect(),
        suggestions: Vec::new(),
        context_chains: vec![ContextChain {
            kind: ContextChainKind::LinkerResolution,
            frames: Vec::new(),
        }],
        symbol_context: rule
            .symbol_capture
            .as_ref()
            .and_then(|capture_name| template_values.get(capture_name))
            .map(|symbol| SymbolContext {
                primary_symbol: Some(symbol.clone()),
                related_objects: Vec::new(),
                archive: None,
            }),
        node_completeness: NodeCompleteness::Partial,
        provenance: Provenance {
            source: ProvenanceSource::ResidualText,
            capture_refs: vec!["stderr.raw".to_string()],
        },
        analysis: Some(AnalysisOverlay {
            family: Some(rule.family.clone()),
            headline: Some(render_template(&rule.headline_template, template_values)),
            first_action_hint: Some(rule.first_action_hint.clone()),
            confidence: Some(Confidence::Medium),
            rule_id: Some(rule.rule_id.clone()),
            matched_conditions: vec![format!("residual_group={key}")],
            suppression_reason: None,
            collapsed_child_ids: Vec::new(),
            collapsed_chain_ids: Vec::new(),
        }),
        fingerprints: None,
    }
}

fn passthrough_node(lines: &[String]) -> DiagnosticNode {
    let passthrough = &residual_rulepack().residual.passthrough;
    DiagnosticNode {
        id: "residual-passthrough".to_string(),
        origin: Origin::ExternalTool,
        phase: passthrough.phase.clone(),
        severity: Severity::Error,
        semantic_role: SemanticRole::Passthrough,
        message: MessageText {
            raw_text: lines.join("\n"),
            normalized_text: None,
            locale: None,
        },
        locations: Vec::new(),
        children: Vec::new(),
        suggestions: Vec::new(),
        context_chains: Vec::new(),
        symbol_context: None,
        node_completeness: NodeCompleteness::Passthrough,
        provenance: Provenance {
            source: ProvenanceSource::ResidualText,
            capture_refs: vec!["stderr.raw".to_string()],
        },
        analysis: Some(AnalysisOverlay {
            family: Some(passthrough.family.clone()),
            headline: Some(passthrough.headline.clone()),
            first_action_hint: Some(passthrough.first_action_hint.clone()),
            confidence: Some(Confidence::Low),
            rule_id: Some(passthrough.rule_id.clone()),
            matched_conditions: vec!["residual_group=passthrough".to_string()],
            suppression_reason: None,
            collapsed_child_ids: Vec::new(),
            collapsed_chain_ids: Vec::new(),
        }),
        fingerprints: None,
    }
}

fn render_template(template: &str, values: &BTreeMap<String, String>) -> String {
    let mut rendered = template.to_string();
    for (key, value) in values {
        rendered = rendered.replace(&format!("{{{key}}}"), value);
    }
    rendered
}

fn parse_locations(lines: &[String]) -> Vec<Location> {
    let location_re = Regex::new(r"(?P<path>[[:alnum:]_./+-]+):(?P<line>\d+)(?::(?P<column>\d+))?")
        .expect("regex");
    lines
        .iter()
        .filter_map(|line| {
            let capture = location_re.captures(line)?;
            Some(Location {
                path: capture["path"].to_string(),
                line: capture["line"].parse().ok()?,
                column: capture
                    .name("column")
                    .and_then(|match_| match_.as_str().parse().ok())
                    .unwrap_or(1),
                end_line: None,
                end_column: None,
                display_path: None,
                ownership: None,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loads_checked_in_residual_rulepack() {
        let rulepack = residual_rulepack();
        assert_eq!(rulepack.rulepack_version, "phase1");
        assert_eq!(
            rulepack
                .compiler_seed(CompilerResidualKind::Template)
                .headline
                .as_deref(),
            Some("template instantiation failed")
        );
        assert!(
            rulepack
                .residual
                .linker_groups
                .iter()
                .any(|entry| entry.kind == LinkerResidualKind::Collect2Error)
        );
    }

    #[test]
    fn classifies_simple_compiler_error_as_renderable_node() {
        let nodes = classify("main.c:4:1: error: expected ';' before '}' token\n", true);
        assert_eq!(nodes.len(), 1);
        assert_eq!(nodes[0].severity, Severity::Error);
        assert_eq!(nodes[0].phase, Phase::Parse);
        assert_eq!(nodes[0].node_completeness, NodeCompleteness::Partial);
        assert_eq!(nodes[0].locations[0].path, "main.c");
        assert_eq!(
            nodes[0]
                .analysis
                .as_ref()
                .and_then(|analysis| analysis.family.as_deref()),
            Some("syntax")
        );
    }

    #[test]
    fn keeps_unclassified_lines_in_passthrough_bucket() {
        let nodes = classify("totally unstructured compiler output\n", true);
        assert_eq!(nodes.len(), 1);
        assert_eq!(nodes[0].semantic_role, SemanticRole::Passthrough);
        assert_eq!(nodes[0].node_completeness, NodeCompleteness::Passthrough);
    }

    #[test]
    fn keeps_unclassified_compiler_diagnostics_in_passthrough_bucket() {
        let stderr = "\
main.c:4:1: error: unsupported compiler wording here\n\
main.c:4:1: note: extra opaque detail\n";
        let nodes = classify(stderr, true);
        assert_eq!(nodes.len(), 1);
        assert_eq!(nodes[0].semantic_role, SemanticRole::Passthrough);
        assert!(
            nodes[0]
                .message
                .raw_text
                .contains("unsupported compiler wording")
        );
        assert!(nodes[0].message.raw_text.contains("extra opaque detail"));
    }

    #[test]
    fn groups_type_overload_candidates_under_one_root() {
        let stderr = "\
main.cpp:5:7: error: no matching function for call to 'takes(int)'\n\
main.cpp:2:6: note: candidate 1: 'void takes(int, int)'\n";
        let nodes = classify(stderr, true);
        assert_eq!(nodes.len(), 1);
        assert_eq!(
            nodes[0]
                .analysis
                .as_ref()
                .and_then(|analysis| analysis.family.as_deref()),
            Some("type_overload")
        );
        assert_eq!(nodes[0].children.len(), 1);
        assert_eq!(nodes[0].children[0].semantic_role, SemanticRole::Candidate);
    }

    #[test]
    fn groups_basic_template_context_under_one_root() {
        let stderr = "\
main.cpp:8:15: error: no matching function for call to 'expect_ptr(int&)'\n\
main.cpp:3:7: note: template argument deduction/substitution failed:\n\
main.cpp:8:15: note:   required from here\n";
        let nodes = classify(stderr, true);
        assert_eq!(nodes.len(), 1);
        assert_eq!(nodes[0].phase, Phase::Instantiate);
        assert_eq!(
            nodes[0]
                .analysis
                .as_ref()
                .and_then(|analysis| analysis.family.as_deref()),
            Some("template")
        );
        assert!(
            nodes[0]
                .context_chains
                .iter()
                .any(|chain| matches!(chain.kind, ContextChainKind::TemplateInstantiation))
        );
        assert_eq!(nodes[0].children.len(), 2);
    }

    #[test]
    fn groups_undefined_reference_residuals() {
        let stderr = "/usr/bin/ld: main.o: in function `main':\nmain.c:(.text+0x15): undefined reference to `foo'";
        let nodes = classify(stderr, true);
        assert!(nodes.iter().any(|node| {
            node.analysis
                .as_ref()
                .and_then(|analysis| analysis.family.as_deref())
                == Some("linker.undefined_reference")
        }));
    }
}
