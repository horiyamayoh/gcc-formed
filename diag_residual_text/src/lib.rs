use diag_core::{
    AnalysisOverlay, Confidence, ContextChain, ContextChainKind, DiagnosticNode, Location,
    MessageText, NodeCompleteness, Origin, Phase, Provenance, ProvenanceSource, SemanticRole,
    Severity, SymbolContext,
};
use regex::Regex;
use std::collections::BTreeMap;

pub fn classify(stderr: &str) -> Vec<DiagnosticNode> {
    let mut grouped = BTreeMap::<String, Vec<String>>::new();
    let mut passthrough = Vec::new();
    let undefined_reference =
        Regex::new(r"undefined reference to [`'](?P<symbol>[^`']+)[`']").expect("regex");
    let multiple_definition =
        Regex::new(r"multiple definition of [`'](?P<symbol>[^`']+)[`']").expect("regex");
    let cannot_find = Regex::new(r"cannot find -l(?P<library>\S+)").expect("regex");
    let file_format =
        Regex::new(r"(file format not recognized|relocation truncated)").expect("regex");
    let assembler = Regex::new(r"(?i)(^as:|^assembler:|assembler messages)").expect("regex");

    for line in stderr.lines().filter(|line| !line.trim().is_empty()) {
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

    let mut nodes = Vec::new();
    for (index, (key, lines)) in grouped.into_iter().enumerate() {
        nodes.push(group_to_node(index, &key, &lines));
    }
    if !passthrough.is_empty() {
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
                collapsed_child_ids: Vec::new(),
                collapsed_chain_ids: Vec::new(),
            }),
            fingerprints: None,
        });
    }
    nodes
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
    fn groups_undefined_reference_residuals() {
        let stderr = "/usr/bin/ld: main.o: in function `main':\nmain.c:(.text+0x15): undefined reference to `foo'";
        let nodes = classify(stderr);
        assert!(nodes.iter().any(|node| {
            node.analysis
                .as_ref()
                .and_then(|a| a.family.clone())
                .as_deref()
                == Some("linker.undefined_reference")
        }));
    }
}
