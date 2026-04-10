use diag_core::{
    AnalysisOverlay, Confidence, ContextChain, ContextChainKind, DiagnosticNode, Location,
    MessageText, NodeCompleteness, Origin, Phase, Provenance, ProvenanceSource, SemanticRole,
    Severity, SymbolContext,
};
use regex::Regex;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CompilerResidualKind {
    Syntax,
    TypeOverload,
    Template,
    Unknown,
}

#[derive(Debug)]
struct CompilerResidualBlock {
    node: Option<DiagnosticNode>,
    raw_lines: Vec<String>,
}

pub fn classify(stderr: &str, include_passthrough: bool) -> Vec<DiagnosticNode> {
    let mut grouped = BTreeMap::<String, Vec<String>>::new();
    let mut compiler_nodes = Vec::new();
    let mut passthrough = Vec::new();
    let mut compiler_block = None::<CompilerResidualBlock>;
    let undefined_reference =
        Regex::new(r"undefined reference to [`'](?P<symbol>[^`']+)[`']").expect("regex");
    let multiple_definition =
        Regex::new(r"multiple definition of [`'](?P<symbol>[^`']+)[`']").expect("regex");
    let cannot_find = Regex::new(r"cannot find -l(?P<library>\S+)").expect("regex");
    let file_format =
        Regex::new(r"(file format not recognized|relocation truncated)").expect("regex");
    let assembler = Regex::new(r"(?i)(^as:|^assembler:|assembler messages)").expect("regex");
    let compiler_diagnostic = Regex::new(
        r"^(?P<path>[[:alnum:]_./+-]+):(?P<line>\d+):(?P<column>\d+): (?P<severity>fatal error|error|warning|note): (?P<message>.+)$",
    )
    .expect("regex");

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
        if let Some(capture) = undefined_reference.captures(line) {
            let symbol = capture["symbol"].to_string();
            grouped
                .entry(format!("undefined:{symbol}"))
                .or_default()
                .push(line.to_string());
            continue;
        }
        if let Some(capture) = multiple_definition.captures(line) {
            let symbol = capture["symbol"].to_string();
            grouped
                .entry(format!("multiple:{symbol}"))
                .or_default()
                .push(line.to_string());
            continue;
        }
        if let Some(capture) = cannot_find.captures(line) {
            let library = capture["library"].to_string();
            grouped
                .entry(format!("library:{library}"))
                .or_default()
                .push(line.to_string());
            continue;
        }
        if file_format.is_match(line) {
            grouped
                .entry("file-format".to_string())
                .or_default()
                .push(line.to_string());
            continue;
        }
        if line.starts_with("collect2: error:") {
            grouped
                .entry("collect2".to_string())
                .or_default()
                .push(line.to_string());
            continue;
        }
        if assembler.is_match(line) && line.contains(':') {
            grouped
                .entry("assembler".to_string())
                .or_default()
                .push(line.to_string());
            continue;
        }
        passthrough.push(line.to_string());
    }
    flush_compiler_block(&mut compiler_nodes, &mut passthrough, &mut compiler_block);

    let mut nodes = compiler_nodes;
    let grouped_base_index = nodes.len();
    for (index, (key, lines)) in grouped.into_iter().enumerate() {
        nodes.push(group_to_node(grouped_base_index + index, &key, &lines));
    }
    if include_passthrough && !passthrough.is_empty() {
        nodes.push(DiagnosticNode {
            id: "residual-passthrough".to_string(),
            origin: Origin::ExternalTool,
            phase: Phase::Link,
            severity: Severity::Error,
            semantic_role: SemanticRole::Passthrough,
            message: MessageText {
                raw_text: passthrough.join("\n"),
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
                family: Some("passthrough".to_string()),
                headline: Some("unclassified residual diagnostics".to_string()),
                first_action_hint: Some(
                    "inspect the preserved compiler stderr for details".to_string(),
                ),
                confidence: Some(Confidence::Low),
                rule_id: Some("rule.residual.passthrough".to_string()),
                matched_conditions: vec!["residual_group=passthrough".to_string()],
                suppression_reason: None,
                collapsed_child_ids: Vec::new(),
                collapsed_chain_ids: Vec::new(),
            }),
            fingerprints: None,
        });
    }
    nodes
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
    let (family, phase, headline, first_action_hint, rule_id, matched_conditions) =
        compiler_diagnostic_seed(kind, &message);

    DiagnosticNode {
        id: format!("residual-compiler-{index}"),
        origin: Origin::Gcc,
        phase,
        severity,
        semantic_role: SemanticRole::Root,
        message: MessageText {
            raw_text: line.to_string(),
            normalized_text: None,
            locale: None,
        },
        locations: vec![Location {
            path: capture["path"].to_string(),
            line: capture["line"].parse().unwrap_or(1),
            column: capture["column"].parse().unwrap_or(1),
            end_line: None,
            end_column: None,
            display_path: None,
            ownership: None,
        }],
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
            family: Some(family),
            headline: Some(headline),
            first_action_hint: Some(first_action_hint),
            confidence: Some(Confidence::Low),
            rule_id: Some(rule_id),
            matched_conditions,
            suppression_reason: None,
            collapsed_child_ids: Vec::new(),
            collapsed_chain_ids: Vec::new(),
        }),
        fingerprints: None,
    }
}

fn compiler_residual_kind(message: &str) -> CompilerResidualKind {
    let lowered = message.to_lowercase();
    if lowered.contains("expected ") || lowered.contains(" before ") || lowered.contains(" after ")
    {
        CompilerResidualKind::Syntax
    } else if lowered.contains("template")
        || lowered.contains("deduction/substitution")
        || lowered.contains("deduced conflicting")
    {
        CompilerResidualKind::Template
    } else if lowered.contains("cannot convert")
        || lowered.contains("no matching")
        || lowered.contains("invalid conversion")
        || lowered.contains("incompatible type")
        || lowered.contains("passing argument")
    {
        CompilerResidualKind::TypeOverload
    } else {
        CompilerResidualKind::Unknown
    }
}

fn compiler_diagnostic_seed(
    kind: CompilerResidualKind,
    message: &str,
) -> (String, Phase, String, String, String, Vec<String>) {
    match kind {
        CompilerResidualKind::Syntax => (
            "syntax".to_string(),
            Phase::Parse,
            message.to_string(),
            "verify the first compiler-reported location against the preserved raw diagnostics"
                .to_string(),
            "rule.residual.compiler_line".to_string(),
            vec![
                "residual_group=compiler_diagnostic".to_string(),
                "family=syntax".to_string(),
            ],
        ),
        CompilerResidualKind::TypeOverload => (
            "type_overload".to_string(),
            Phase::Semantic,
            "type or overload mismatch".to_string(),
            "compare the expected type and actual argument at the call site".to_string(),
            "rule.residual.compiler_type_overload".to_string(),
            vec![
                "residual_group=compiler_diagnostic".to_string(),
                "family=type_overload".to_string(),
            ],
        ),
        CompilerResidualKind::Template => (
            "template".to_string(),
            Phase::Instantiate,
            "template instantiation failed".to_string(),
            "start from the first user-owned template frame and match template arguments"
                .to_string(),
            "rule.residual.compiler_template".to_string(),
            vec![
                "residual_group=compiler_diagnostic".to_string(),
                "family=template".to_string(),
            ],
        ),
        CompilerResidualKind::Unknown => (
            "compiler.residual".to_string(),
            Phase::Semantic,
            message.to_string(),
            "inspect the preserved raw diagnostics for the first corrective action".to_string(),
            "rule.residual.compiler_unknown".to_string(),
            vec![
                "residual_group=compiler_diagnostic".to_string(),
                "family=compiler.residual".to_string(),
            ],
        ),
    }
}

fn attach_compiler_note(node: &mut DiagnosticNode, line: &str, capture: &regex::Captures<'_>) {
    let message = capture["message"].to_string();
    let lowered = message.to_lowercase();
    let role = if lowered.contains("candidate:") || is_numbered_candidate_message(&lowered) {
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
        node.phase = Phase::Instantiate;
        push_context_chain(node, line, capture);
        if let Some(analysis) = node.analysis.as_mut() {
            analysis.family = Some("template".to_string());
            analysis.headline = Some("template instantiation failed".to_string());
            analysis.first_action_hint = Some(
                "start from the first user-owned template frame and match template arguments"
                    .to_string(),
            );
            analysis.rule_id = Some("rule.residual.compiler_template".to_string());
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
    message.contains("template")
        || message.contains("required from")
        || message.contains("required by substitution")
        || message.contains("deduction/substitution")
}

fn is_numbered_candidate_message(message: &str) -> bool {
    let Some(rest) = message.trim().strip_prefix("candidate ") else {
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

fn group_to_node(index: usize, key: &str, lines: &[String]) -> DiagnosticNode {
    let (headline, first_action_hint, symbol_context, family) =
        if let Some(symbol) = key.strip_prefix("undefined:") {
            (
                format!("undefined reference to `{symbol}`"),
                "define the missing symbol or link the object/library that provides it".to_string(),
                Some(SymbolContext {
                    primary_symbol: Some(symbol.to_string()),
                    related_objects: Vec::new(),
                    archive: None,
                }),
                "linker.undefined_reference".to_string(),
            )
        } else if let Some(symbol) = key.strip_prefix("multiple:") {
            (
            format!("multiple definition of `{symbol}`"),
            "remove the duplicate definition or make the symbol `static`/`inline` as appropriate"
                .to_string(),
            Some(SymbolContext {
                primary_symbol: Some(symbol.to_string()),
                related_objects: Vec::new(),
                archive: None,
            }),
            "linker.multiple_definition".to_string(),
        )
        } else if let Some(library) = key.strip_prefix("library:") {
            (
                format!("cannot find library `-l{library}`"),
                "check the library search path and whether the archive is installed".to_string(),
                None,
                "linker.cannot_find_library".to_string(),
            )
        } else if key == "assembler" {
            (
                "assembler reported an error".to_string(),
                "inspect the assembly-related stderr lines and the referenced source location"
                    .to_string(),
                None,
                "assembler.error".to_string(),
            )
        } else {
            (
                "linker file format or relocation failure".to_string(),
                "check object compatibility, target triple, and archive contents".to_string(),
                None,
                "linker.file_format_or_relocation".to_string(),
            )
        };

    DiagnosticNode {
        id: format!("residual-{index}"),
        origin: if family.starts_with("assembler") {
            Origin::ExternalTool
        } else {
            Origin::Linker
        },
        phase: if family.starts_with("assembler") {
            Phase::Assemble
        } else {
            Phase::Link
        },
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
        symbol_context,
        node_completeness: NodeCompleteness::Partial,
        provenance: Provenance {
            source: ProvenanceSource::ResidualText,
            capture_refs: vec!["stderr.raw".to_string()],
        },
        analysis: Some(AnalysisOverlay {
            family: Some(family),
            headline: Some(headline),
            first_action_hint: Some(first_action_hint),
            confidence: Some(Confidence::Medium),
            rule_id: Some("rule.residual.linker_group".to_string()),
            matched_conditions: vec![format!("residual_group={key}")],
            suppression_reason: None,
            collapsed_child_ids: Vec::new(),
            collapsed_chain_ids: Vec::new(),
        }),
        fingerprints: None,
    }
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
                .and_then(|a| a.family.clone())
                .as_deref()
                == Some("linker.undefined_reference")
        }));
    }
}
