//! Classifies and extracts residual text from compiler stderr that was not
//! captured as structured diagnostics.

use diag_core::{
    AnalysisOverlay, Confidence, ContextChain, ContextChainKind, DiagnosticNode, Location,
    MessageText, NodeCompleteness, Origin, Phase, Provenance, ProvenanceSource, SemanticRole,
    Severity, SymbolContext,
};
use diag_rulepack::{
    CompilerResidualKind, CompilerResidualSeed, HeadlineStrategy, LinkerResidualSeed,
    ResidualRulepack, checked_in_rulepack,
};
use regex::Regex;
use std::collections::{BTreeMap, btree_map::Entry};

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

/// Classifies residual stderr lines into structured diagnostic nodes.
///
/// When `include_passthrough` is true, unclassified lines are emitted as a
/// single passthrough node; otherwise they are silently dropped.
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
            ingest_compiler_diagnostic_line(
                &mut compiler_nodes,
                &mut passthrough,
                &mut compiler_block,
                line,
                &capture,
            );
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

fn residual_rulepack() -> &'static ResidualRulepack {
    checked_in_rulepack().residual()
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
        if let Some(regex) = &matcher.regex
            && let Some(capture) = regex.captures(line)
        {
            let template_values = capture_template_values(regex, &capture);
            return Some(LinkerMatch {
                rule: matcher.rule,
                group_key: linker_group_key(matcher.rule, &template_values),
                template_values,
            });
        }
        if let Some(prefix) = &matcher.rule.match_prefix
            && line.starts_with(prefix)
        {
            return Some(LinkerMatch {
                rule: matcher.rule,
                group_key: linker_group_key(matcher.rule, &BTreeMap::new()),
                template_values: BTreeMap::new(),
            });
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
    let seed = compiler_residual_seed(&capture["message"]);
    let raw_lines = vec![line.to_string()];
    if seed.kind == CompilerResidualKind::Unknown {
        *current_block = Some(CompilerResidualBlock {
            node: None,
            raw_lines,
        });
    } else {
        *current_block = Some(CompilerResidualBlock {
            node: Some(compiler_diagnostic_node(
                compiler_nodes.len(),
                line,
                capture,
                seed,
            )),
            raw_lines,
        });
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
    seed: &CompilerResidualSeed,
) -> DiagnosticNode {
    let message = capture["message"].to_string();
    let severity = match &capture["severity"] {
        "fatal error" | "error" => Severity::Error,
        "warning" => Severity::Warning,
        "note" => Severity::Note,
        _ => Severity::Unknown,
    };

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
            family: Some(seed.family.clone().into()),
            family_version: None,
            family_confidence: None,
            root_cause_score: None,
            actionability_score: None,
            user_code_priority: None,
            headline: Some(compiler_headline(seed, &message).into()),
            first_action_hint: Some(seed.first_action_hint.clone().into()),
            confidence: Some(Confidence::Low.score()),
            preferred_primary_location_id: None,
            rule_id: Some(seed.rule_id.clone().into()),
            matched_conditions: vec![
                "residual_group=compiler_diagnostic".into(),
                format!("family={}", seed.family).into(),
            ],
            suppression_reason: None,
            collapsed_child_ids: Vec::new(),
            collapsed_chain_ids: Vec::new(),
            group_ref: None,
            reasons: Vec::new(),
            policy_profile: None,
            producer_version: None,
        }),
        fingerprints: None,
    }
}

fn compiler_residual_seed(message: &str) -> &'static CompilerResidualSeed {
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
        .unwrap_or_else(|| residual_rulepack().compiler_seed(CompilerResidualKind::Unknown))
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
            analysis.family = Some(template_seed.family.clone().into());
            analysis.headline = Some(compiler_headline(template_seed, &message).into());
            analysis.first_action_hint = Some(template_seed.first_action_hint.clone().into());
            analysis.rule_id = Some(template_seed.rule_id.clone().into());
            if !analysis
                .matched_conditions
                .iter()
                .any(|condition| condition == "family=template")
            {
                analysis.matched_conditions.push("family=template".into());
            }
        }
    }

    let note_seed = compiler_residual_seed(&message);
    if note_seed.family == "concepts_constraints" {
        node.phase = note_seed.phase.clone();
        if let Some(analysis) = node.analysis.as_mut() {
            analysis.family = Some(note_seed.family.clone().into());
            analysis.headline = Some(compiler_headline(note_seed, &message).into());
            analysis.first_action_hint = Some(note_seed.first_action_hint.clone().into());
            analysis.rule_id = Some(note_seed.rule_id.clone().into());
            if !analysis
                .matched_conditions
                .iter()
                .any(|condition| condition == "family=concepts_constraints")
            {
                analysis
                    .matched_conditions
                    .push("family=concepts_constraints".into());
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
    Location::caret(
        capture["path"].to_string(),
        capture["line"].parse().unwrap_or(1),
        capture["column"].parse().unwrap_or(1),
        diag_core::LocationRole::Primary,
    )
}

fn group_to_node(
    index: usize,
    key: &str,
    rule: &LinkerResidualSeed,
    template_values: &BTreeMap<String, String>,
    lines: &[String],
) -> DiagnosticNode {
    let context_chains = grouped_context_chains(rule);
    DiagnosticNode {
        id: format!("residual-{index}"),
        origin: rule.origin.clone(),
        phase: rule.phase.clone(),
        severity: grouped_severity(lines),
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
                origin: rule.origin.clone(),
                phase: rule.phase.clone(),
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
                context_chains: context_chains.clone(),
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
        context_chains,
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
            family: Some(rule.family.clone().into()),
            family_version: None,
            family_confidence: None,
            root_cause_score: None,
            actionability_score: None,
            user_code_priority: None,
            headline: Some(render_template(&rule.headline_template, template_values).into()),
            first_action_hint: Some(rule.first_action_hint.clone().into()),
            confidence: Some(Confidence::Medium.score()),
            preferred_primary_location_id: None,
            rule_id: Some(rule.rule_id.clone().into()),
            matched_conditions: vec![format!("residual_group={key}").into()],
            suppression_reason: None,
            collapsed_child_ids: Vec::new(),
            collapsed_chain_ids: Vec::new(),
            group_ref: None,
            reasons: Vec::new(),
            policy_profile: None,
            producer_version: None,
        }),
        fingerprints: None,
    }
}

fn grouped_context_chains(rule: &LinkerResidualSeed) -> Vec<ContextChain> {
    if matches!(rule.phase, Phase::Link) {
        vec![ContextChain {
            kind: ContextChainKind::LinkerResolution,
            frames: Vec::new(),
        }]
    } else {
        Vec::new()
    }
}

fn grouped_severity(lines: &[String]) -> Severity {
    if lines
        .iter()
        .any(|line| line.to_ascii_lowercase().contains("fatal error"))
    {
        Severity::Fatal
    } else {
        Severity::Error
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
            family: Some(passthrough.family.clone().into()),
            family_version: None,
            family_confidence: None,
            root_cause_score: None,
            actionability_score: None,
            user_code_priority: None,
            headline: Some(passthrough.headline.clone().into()),
            first_action_hint: Some(passthrough.first_action_hint.clone().into()),
            confidence: Some(Confidence::Low.score()),
            preferred_primary_location_id: None,
            rule_id: Some(passthrough.rule_id.clone().into()),
            matched_conditions: vec!["residual_group=passthrough".into()],
            suppression_reason: None,
            collapsed_child_ids: Vec::new(),
            collapsed_chain_ids: Vec::new(),
            group_ref: None,
            reasons: Vec::new(),
            policy_profile: None,
            producer_version: None,
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
            Some(Location::caret(
                capture["path"].to_string(),
                capture["line"].parse().ok()?,
                capture
                    .name("column")
                    .and_then(|match_| match_.as_str().parse().ok())
                    .unwrap_or(1),
                diag_core::LocationRole::Primary,
            ))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use diag_rulepack::LinkerResidualKind;

    #[test]
    fn loads_checked_in_residual_rulepack() {
        let rulepack = residual_rulepack();
        assert_eq!(rulepack.rulepack_version, "phase1");
        assert!(std::ptr::eq(rulepack, checked_in_rulepack().residual()));
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
                .any(|entry| entry.kind == LinkerResidualKind::Collect2Summary)
        );
        assert!(
            rulepack
                .residual
                .linker_groups
                .iter()
                .any(|entry| entry.kind == LinkerResidualKind::DriverFatal)
        );
    }

    #[test]
    fn classifies_simple_compiler_error_as_renderable_node() {
        let nodes = classify("main.c:4:1: error: expected ';' before '}' token\n", true);
        assert_eq!(nodes.len(), 1);
        assert_eq!(nodes[0].severity, Severity::Error);
        assert_eq!(nodes[0].phase, Phase::Parse);
        assert_eq!(nodes[0].node_completeness, NodeCompleteness::Partial);
        assert_eq!(nodes[0].locations[0].path_raw(), "main.c");
        assert_eq!(
            nodes[0]
                .analysis
                .as_ref()
                .and_then(|analysis| analysis.family.as_deref()),
            Some("syntax")
        );
    }

    #[test]
    fn classifies_preprocessor_directives_as_preprocess_phase() {
        let nodes = classify("src/config.h:3:2: error: #error stop here\n", true);
        assert_eq!(nodes.len(), 1);
        assert_eq!(nodes[0].phase, Phase::Preprocess);
        assert_eq!(
            nodes[0]
                .analysis
                .as_ref()
                .and_then(|analysis| analysis.family.as_deref()),
            Some("preprocessor_directive")
        );
    }

    #[test]
    fn classifies_scope_declaration_compiler_error() {
        let nodes = classify(
            "main.cpp:2:12: error: 'missing_value' was not declared in this scope\n",
            true,
        );

        assert_eq!(nodes.len(), 1);
        assert_eq!(nodes[0].phase, Phase::Semantic);
        assert_eq!(
            nodes[0]
                .analysis
                .as_ref()
                .and_then(|analysis| analysis.family.as_deref()),
            Some("scope_declaration")
        );
        assert_eq!(
            nodes[0]
                .analysis
                .as_ref()
                .and_then(|analysis| analysis.headline.as_deref()),
            Some("identifier not found in scope")
        );
    }

    #[test]
    fn classifies_redefinition_compiler_error() {
        let stderr = "\
main.c:2:5: error: redefinition of 'counter'\n\
main.c:1:5: note: previous definition of 'counter' with type 'int'\n";
        let nodes = classify(stderr, true);

        assert_eq!(nodes.len(), 1);
        assert_eq!(nodes[0].phase, Phase::Semantic);
        assert_eq!(
            nodes[0]
                .analysis
                .as_ref()
                .and_then(|analysis| analysis.family.as_deref()),
            Some("redefinition")
        );
        assert_eq!(nodes[0].children.len(), 1);
    }

    #[test]
    fn classifies_deleted_function_compiler_error() {
        let stderr = "\
main.cpp:10:9: error: use of deleted function 'NoCopy::NoCopy(const NoCopy&)'\n\
main.cpp:3:5: note: declared here\n";
        let nodes = classify(stderr, true);

        assert_eq!(nodes.len(), 1);
        assert_eq!(nodes[0].phase, Phase::Semantic);
        assert_eq!(
            nodes[0]
                .analysis
                .as_ref()
                .and_then(|analysis| analysis.family.as_deref()),
            Some("deleted_function")
        );
        assert_eq!(
            nodes[0]
                .analysis
                .as_ref()
                .and_then(|analysis| analysis.headline.as_deref()),
            Some("use of a deleted or unavailable function")
        );
    }

    #[test]
    fn classifies_concepts_constraints_compiler_error() {
        let stderr = "\
main.cpp:7:19: error: no matching function for call to 'consume(int)'\n\
main.cpp:2:9: note: constraints not satisfied\n";
        let nodes = classify(stderr, true);

        assert_eq!(nodes.len(), 1);
        assert_eq!(nodes[0].phase, Phase::Constraints);
        assert_eq!(
            nodes[0]
                .analysis
                .as_ref()
                .and_then(|analysis| analysis.family.as_deref()),
            Some("concepts_constraints")
        );
        assert_eq!(nodes[0].children.len(), 1);
    }

    #[test]
    fn classifies_unused_compiler_warning() {
        let stderr = "main.c:2:9: warning: unused variable 'temporary' [-Wunused-variable]\n";
        let nodes = classify(stderr, true);

        assert_eq!(nodes.len(), 1);
        assert_eq!(nodes[0].severity, Severity::Warning);
        assert_eq!(nodes[0].phase, Phase::Semantic);
        assert_eq!(
            nodes[0]
                .analysis
                .as_ref()
                .and_then(|analysis| analysis.family.as_deref()),
            Some("unused")
        );
        assert_eq!(
            nodes[0]
                .analysis
                .as_ref()
                .and_then(|analysis| analysis.headline.as_deref()),
            Some("unused declaration detected")
        );
    }

    #[test]
    fn classifies_return_type_compiler_warning() {
        let stderr =
            "main.c:3:1: warning: control reaches end of non-void function [-Wreturn-type]\n";
        let nodes = classify(stderr, true);

        assert_eq!(nodes.len(), 1);
        assert_eq!(nodes[0].severity, Severity::Warning);
        assert_eq!(nodes[0].phase, Phase::Semantic);
        assert_eq!(
            nodes[0]
                .analysis
                .as_ref()
                .and_then(|analysis| analysis.family.as_deref()),
            Some("return_type")
        );
    }

    #[test]
    fn classifies_format_string_compiler_warning() {
        let stderr = "main.c:4:12: warning: format '%d' expects argument of type 'int', but argument 2 has type 'char *' [-Wformat=]\n";
        let nodes = classify(stderr, true);

        assert_eq!(nodes.len(), 1);
        assert_eq!(nodes[0].severity, Severity::Warning);
        assert_eq!(nodes[0].phase, Phase::Semantic);
        assert_eq!(
            nodes[0]
                .analysis
                .as_ref()
                .and_then(|analysis| analysis.family.as_deref()),
            Some("format_string")
        );
    }

    #[test]
    fn classifies_analyzer_compiler_warning() {
        let stderr =
            "main.c:6:5: warning: double-'free' of 'ptr' [CWE-415] [-Wanalyzer-double-free]\n";
        let nodes = classify(stderr, true);

        assert_eq!(nodes.len(), 1);
        assert_eq!(nodes[0].severity, Severity::Warning);
        assert_eq!(nodes[0].phase, Phase::Analyze);
        assert_eq!(
            nodes[0]
                .analysis
                .as_ref()
                .and_then(|analysis| analysis.family.as_deref()),
            Some("analyzer")
        );
    }

    #[test]
    fn classifies_uninitialized_compiler_warning() {
        let stderr =
            "main.c:5:12: warning: 'value' may be used uninitialized [-Wmaybe-uninitialized]\n";
        let nodes = classify(stderr, true);

        assert_eq!(nodes.len(), 1);
        assert_eq!(nodes[0].severity, Severity::Warning);
        assert_eq!(nodes[0].phase, Phase::Semantic);
        assert_eq!(
            nodes[0]
                .analysis
                .as_ref()
                .and_then(|analysis| analysis.family.as_deref()),
            Some("uninitialized")
        );
    }

    #[test]
    fn classifies_conversion_narrowing_compiler_warning() {
        let stderr = "main.c:2:17: warning: comparison of integer expressions of different signedness: 'int' and 'unsigned int' [-Wsign-compare]\n";
        let nodes = classify(stderr, true);

        assert_eq!(nodes.len(), 1);
        assert_eq!(nodes[0].severity, Severity::Warning);
        assert_eq!(nodes[0].phase, Phase::Semantic);
        assert_eq!(
            nodes[0]
                .analysis
                .as_ref()
                .and_then(|analysis| analysis.family.as_deref()),
            Some("conversion_narrowing")
        );
    }

    #[test]
    fn classifies_const_qualifier_compiler_warning() {
        let stderr = "main.c:7:18: warning: passing argument 1 of 'takes' discards 'const' qualifier from pointer target type [-Wdiscarded-qualifiers]\n";
        let nodes = classify(stderr, true);

        assert_eq!(nodes.len(), 1);
        assert_eq!(nodes[0].severity, Severity::Warning);
        assert_eq!(nodes[0].phase, Phase::Semantic);
        assert_eq!(
            nodes[0]
                .analysis
                .as_ref()
                .and_then(|analysis| analysis.family.as_deref()),
            Some("const_qualifier")
        );
    }

    #[test]
    fn classifies_pointer_reference_compiler_error() {
        let stderr = "\
main.cpp:5:16: error: invalid use of incomplete type 'struct Node'\n\
main.cpp:1:8: note: forward declaration of 'struct Node'\n";
        let nodes = classify(stderr, true);

        assert_eq!(nodes.len(), 1);
        assert_eq!(nodes[0].severity, Severity::Error);
        assert_eq!(nodes[0].phase, Phase::Semantic);
        assert_eq!(
            nodes[0]
                .analysis
                .as_ref()
                .and_then(|analysis| analysis.family.as_deref()),
            Some("pointer_reference")
        );
        assert_eq!(nodes[0].children.len(), 1);
    }

    #[test]
    fn classifies_access_control_compiler_error() {
        let stderr = "\
access.cpp:8:20: error: 'int Counter::value' is private within this context\n\
access.cpp:3:9: note: declared private here\n";
        let nodes = classify(stderr, true);

        assert_eq!(nodes.len(), 1);
        assert_eq!(nodes[0].severity, Severity::Error);
        assert_eq!(nodes[0].phase, Phase::Semantic);
        assert_eq!(
            nodes[0]
                .analysis
                .as_ref()
                .and_then(|analysis| analysis.family.as_deref()),
            Some("access_control")
        );
        assert_eq!(nodes[0].children.len(), 1);
    }

    #[test]
    fn classifies_coroutine_compiler_error() {
        let stderr = "main.cpp:5:5: error: unable to find the promise type for this coroutine\n";
        let nodes = classify(stderr, true);

        assert_eq!(nodes.len(), 1);
        assert_eq!(nodes[0].severity, Severity::Error);
        assert_eq!(nodes[0].phase, Phase::Semantic);
        assert_eq!(
            nodes[0]
                .analysis
                .as_ref()
                .and_then(|analysis| analysis.family.as_deref()),
            Some("coroutine")
        );
    }

    #[test]
    fn classifies_module_import_compiler_error() {
        let stderr =
            "main.cpp:1:8: error: failed to read compiled module: No such file or directory\n";
        let nodes = classify(stderr, true);

        assert_eq!(nodes.len(), 1);
        assert_eq!(nodes[0].severity, Severity::Error);
        assert_eq!(nodes[0].phase, Phase::Semantic);
        assert_eq!(
            nodes[0]
                .analysis
                .as_ref()
                .and_then(|analysis| analysis.family.as_deref()),
            Some("module_import")
        );
    }

    #[test]
    fn classifies_deprecated_compiler_warning() {
        let stderr = "main.cpp:4:19: warning: 'int old_api()' is deprecated: use new_api [-Wdeprecated-declarations]\n";
        let nodes = classify(stderr, true);

        assert_eq!(nodes.len(), 1);
        assert_eq!(nodes[0].severity, Severity::Warning);
        assert_eq!(nodes[0].phase, Phase::Semantic);
        assert_eq!(
            nodes[0]
                .analysis
                .as_ref()
                .and_then(|analysis| analysis.family.as_deref()),
            Some("deprecated")
        );
    }

    #[test]
    fn classifies_inheritance_virtual_compiler_error() {
        let stderr = "\
main.cpp:7:13: error: cannot declare variable 'value' to be of abstract type 'Derived'\n\
main.cpp:3:18: note:   because the following virtual functions are pure within 'Derived':\n";
        let nodes = classify(stderr, true);

        assert_eq!(nodes.len(), 1);
        assert_eq!(nodes[0].severity, Severity::Error);
        assert_eq!(nodes[0].phase, Phase::Semantic);
        assert_eq!(
            nodes[0]
                .analysis
                .as_ref()
                .and_then(|analysis| analysis.family.as_deref()),
            Some("inheritance_virtual")
        );
        assert_eq!(nodes[0].children.len(), 1);
    }

    #[test]
    fn classifies_constexpr_compiler_error() {
        let stderr = "main.cpp:1:19: error: static assertion failed: int size mismatch\n";
        let nodes = classify(stderr, true);

        assert_eq!(nodes.len(), 1);
        assert_eq!(nodes[0].severity, Severity::Error);
        assert_eq!(nodes[0].phase, Phase::Semantic);
        assert_eq!(
            nodes[0]
                .analysis
                .as_ref()
                .and_then(|analysis| analysis.family.as_deref()),
            Some("constexpr")
        );
    }

    #[test]
    fn classifies_lambda_closure_compiler_error() {
        let stderr = "\
main.cpp:3:27: error: 'value' is not captured\n\
main.cpp:3:24: note: the lambda has no capture-default\n";
        let nodes = classify(stderr, true);

        assert_eq!(nodes.len(), 1);
        assert_eq!(nodes[0].severity, Severity::Error);
        assert_eq!(nodes[0].phase, Phase::Semantic);
        assert_eq!(
            nodes[0]
                .analysis
                .as_ref()
                .and_then(|analysis| analysis.family.as_deref()),
            Some("lambda_closure")
        );
        assert_eq!(nodes[0].children.len(), 1);
    }

    #[test]
    fn classifies_lifetime_dangling_compiler_warning() {
        let stderr = "main.cpp:3:12: warning: address of local variable 'value' returned [-Wreturn-local-addr]\n";
        let nodes = classify(stderr, true);

        assert_eq!(nodes.len(), 1);
        assert_eq!(nodes[0].severity, Severity::Warning);
        assert_eq!(nodes[0].phase, Phase::Semantic);
        assert_eq!(
            nodes[0]
                .analysis
                .as_ref()
                .and_then(|analysis| analysis.family.as_deref()),
            Some("lifetime_dangling")
        );
    }

    #[test]
    fn classifies_init_order_compiler_warning() {
        let stderr = "main.cpp:4:5: warning: 'Example::value' will be initialized after 'int Example::count' [-Wreorder]\n";
        let nodes = classify(stderr, true);

        assert_eq!(nodes.len(), 1);
        assert_eq!(nodes[0].severity, Severity::Warning);
        assert_eq!(nodes[0].phase, Phase::Semantic);
        assert_eq!(
            nodes[0]
                .analysis
                .as_ref()
                .and_then(|analysis| analysis.family.as_deref()),
            Some("init_order")
        );
    }

    #[test]
    fn keeps_structured_compiler_residuals_when_passthrough_is_disabled() {
        let stderr = "\
main.cpp:5:7: error: no matching function for call to 'takes(int)'\n\
main.cpp:2:6: note: candidate 1: 'void takes(int, int)'\n";
        let nodes = classify(stderr, false);

        assert_eq!(nodes.len(), 1);
        assert_eq!(nodes[0].semantic_role, SemanticRole::Root);
        assert_eq!(nodes[0].node_completeness, NodeCompleteness::Partial);
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
    fn suppresses_only_passthrough_emission_when_disabled() {
        let stderr = "\
main.c:4:1: error: expected ';' before '}' token\n\
totally unstructured compiler output\n";

        let with_passthrough = classify(stderr, true);
        let without_passthrough = classify(stderr, false);

        assert_eq!(with_passthrough.len(), 2);
        assert!(with_passthrough.iter().any(|node| {
            node.analysis
                .as_ref()
                .and_then(|analysis| analysis.family.as_deref())
                == Some("syntax")
        }));
        assert!(with_passthrough.iter().any(|node| {
            matches!(node.semantic_role, SemanticRole::Passthrough)
                && node
                    .message
                    .raw_text
                    .contains("totally unstructured compiler output")
        }));

        assert_eq!(without_passthrough.len(), 1);
        assert_eq!(without_passthrough[0].semantic_role, SemanticRole::Root);
        assert!(
            !without_passthrough
                .iter()
                .any(|node| matches!(node.semantic_role, SemanticRole::Passthrough))
        );
        assert_eq!(
            without_passthrough[0]
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

    #[test]
    fn groups_collect2_residuals_as_driver_summary() {
        let stderr = "collect2: error: ld returned 1 exit status";
        let nodes = classify(stderr, true);
        assert_eq!(nodes.len(), 1);
        assert_eq!(nodes[0].origin, Origin::Driver);
        assert_eq!(nodes[0].phase, Phase::Link);
        let analysis = nodes[0].analysis.as_ref().unwrap();
        assert_eq!(analysis.family.as_deref(), Some("collect2_summary"));
        assert_eq!(
            analysis.headline.as_deref(),
            Some("driver reported a linker failure summary")
        );
        assert!(
            analysis
                .matched_conditions
                .iter()
                .any(|condition| condition == "residual_group=collect2")
        );
    }

    #[test]
    fn groups_driver_fatal_residuals() {
        let stderr = "gcc: fatal error: cannot execute 'cc1': execvp: No such file or directory";
        let nodes = classify(stderr, true);
        assert_eq!(nodes.len(), 1);
        assert_eq!(nodes[0].origin, Origin::Driver);
        assert_eq!(nodes[0].phase, Phase::Driver);
        assert_eq!(nodes[0].severity, Severity::Fatal);
        assert_eq!(
            nodes[0]
                .analysis
                .as_ref()
                .and_then(|analysis| analysis.family.as_deref()),
            Some("driver_fatal")
        );
    }

    #[test]
    fn groups_internal_compiler_error_banners() {
        let stderr = "cc1plus: internal compiler error: Segmentation fault";
        let nodes = classify(stderr, true);
        assert_eq!(nodes.len(), 1);
        assert_eq!(nodes[0].origin, Origin::Gcc);
        assert_eq!(nodes[0].phase, Phase::Unknown);
        assert_eq!(
            nodes[0]
                .analysis
                .as_ref()
                .and_then(|analysis| analysis.family.as_deref()),
            Some("internal_compiler_error_banner")
        );
    }

    #[test]
    fn grouped_non_linker_children_preserve_rule_origin_and_phase() {
        let stderr = "\
as: unrecognized option '--gdwarf-99'\n\
as: fatal error: Killed signal terminated program as\n";
        let nodes = classify(stderr, true);
        assert_eq!(nodes.len(), 1);
        assert_eq!(nodes[0].origin, Origin::ExternalTool);
        assert_eq!(nodes[0].phase, Phase::Assemble);
        assert_eq!(nodes[0].children.len(), 1);
        assert_eq!(nodes[0].children[0].origin, Origin::ExternalTool);
        assert_eq!(nodes[0].children[0].phase, Phase::Assemble);
        assert!(nodes[0].context_chains.is_empty());
        assert!(nodes[0].children[0].context_chains.is_empty());
    }

    #[test]
    fn groups_cannot_find_library_residuals() {
        let stderr = "/usr/bin/ld: cannot find -lssl";
        let nodes = classify(stderr, true);
        assert_eq!(nodes.len(), 1);
        let analysis = nodes[0].analysis.as_ref().unwrap();
        assert_eq!(
            analysis.family.as_deref(),
            Some("linker.cannot_find_library")
        );
        assert_eq!(
            analysis.headline.as_deref(),
            Some("cannot find library `-lssl`")
        );
    }

    #[test]
    fn groups_multiple_definition_residuals() {
        let stderr = "/usr/bin/ld: util.o:(.text+0x0): multiple definition of `foo'";
        let nodes = classify(stderr, true);
        assert_eq!(nodes.len(), 1);
        let analysis = nodes[0].analysis.as_ref().unwrap();
        assert_eq!(
            analysis.family.as_deref(),
            Some("linker.multiple_definition")
        );
        assert_eq!(
            analysis.headline.as_deref(),
            Some("multiple definition of `foo`")
        );
    }

    #[test]
    fn groups_file_format_residuals() {
        let stderr =
            "/usr/bin/ld: archive.a: file format not recognized; treating as linker script";
        let nodes = classify(stderr, true);
        assert_eq!(nodes.len(), 1);
        let analysis = nodes[0].analysis.as_ref().unwrap();
        assert_eq!(
            analysis.family.as_deref(),
            Some("linker.file_format_or_relocation")
        );
        assert_eq!(
            analysis.headline.as_deref(),
            Some("linker file format or relocation failure")
        );
        assert!(
            analysis
                .matched_conditions
                .iter()
                .any(|condition| condition == "residual_group=file-format")
        );
    }

    #[test]
    fn empty_input_produces_no_nodes() {
        assert!(classify("", true).is_empty());
        assert!(classify("\n\n", false).is_empty());
    }

    #[test]
    fn interleaved_compiler_and_linker_output_preserves_each_family() {
        let stderr = "\
main.c:4:1: error: expected ';' before '}' token\n\
/usr/bin/ld: main.o: in function `main':\n\
main.c:(.text+0x15): undefined reference to `foo'\n\
collect2: error: ld returned 1 exit status\n";
        let nodes = classify(stderr, true);

        assert!(nodes.iter().any(|node| {
            node.analysis
                .as_ref()
                .and_then(|analysis| analysis.family.as_deref())
                == Some("syntax")
        }));
        assert!(nodes.iter().any(|node| {
            node.analysis
                .as_ref()
                .and_then(|analysis| analysis.family.as_deref())
                == Some("linker.undefined_reference")
        }));
        assert!(nodes.iter().any(|node| {
            node.analysis
                .as_ref()
                .and_then(|analysis| analysis.family.as_deref())
                == Some("collect2_summary")
        }));
    }

    #[test]
    fn malformed_compiler_lines_without_column_become_passthrough() {
        let stderr = "main.c:4: error: expected ';' before '}' token\n";
        let nodes = classify(stderr, true);

        assert_eq!(nodes.len(), 1);
        assert_eq!(nodes[0].semantic_role, SemanticRole::Passthrough);
        assert!(
            nodes[0]
                .message
                .raw_text
                .contains("expected ';' before '}' token")
        );
    }

    #[test]
    fn preserves_unicode_passthrough_text() {
        let stderr = "外部ツール: エラー: 未定義シンボル μ_result\n";
        let nodes = classify(stderr, true);

        assert_eq!(nodes.len(), 1);
        assert_eq!(nodes[0].semantic_role, SemanticRole::Passthrough);
        assert_eq!(
            nodes[0].message.raw_text,
            "外部ツール: エラー: 未定義シンボル μ_result"
        );
    }
}
