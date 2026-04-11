use diag_core::{ArtifactKind, CaptureArtifact, DiagnosticDocument, RunInfo};
use serde::{Deserialize, Serialize};
use std::path::Path;

/// Canonical snapshot comparison lives in the test harness so volatile-field
/// handling is centralized instead of being reimplemented per caller.
///
/// Allowed normalization is intentionally narrow:
/// - temporary object paths and random suffixes
/// - quote-style drift
/// - line/column drift in volatile text and SARIF regions
/// - non-semantic SARIF wrapper fields and transient capture metadata
///
/// Disallowed normalization is equally important:
/// - family, severity, ownership, phase, provenance source
/// - lead path identity
/// - fallback semantics
/// - rule-selected analysis content beyond the volatile text rules above

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SnapshotDiffKind {
    Exact,
    NormalizationOnly,
    Semantic,
    MissingExpected,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SnapshotComparison {
    pub diff_kind: SnapshotDiffKind,
    pub normalized_expected: String,
    pub normalized_actual: String,
}

impl SnapshotComparison {
    pub fn matches_after_normalization(&self) -> bool {
        matches!(
            self.diff_kind,
            SnapshotDiffKind::Exact | SnapshotDiffKind::NormalizationOnly
        )
    }
}

pub fn compare_snapshot_contents(
    path: &Path,
    expected: &str,
    actual: &str,
) -> Result<SnapshotComparison, String> {
    let normalized_expected = normalize_snapshot_contents(path, expected)?;
    let normalized_actual = normalize_snapshot_contents(path, actual)?;
    let diff_kind = if expected == actual {
        SnapshotDiffKind::Exact
    } else if normalized_expected == normalized_actual {
        SnapshotDiffKind::NormalizationOnly
    } else {
        SnapshotDiffKind::Semantic
    };
    Ok(SnapshotComparison {
        diff_kind,
        normalized_expected,
        normalized_actual,
    })
}

pub fn normalize_snapshot_contents(path: &Path, contents: &str) -> Result<String, String> {
    match path.file_name().and_then(|value| value.to_str()) {
        Some("diagnostics.sarif") => normalize_sarif_snapshot_contents(path, contents),
        Some("ir.facts.json") | Some("ir.analysis.json") => {
            normalize_ir_snapshot_contents(path, contents)
        }
        _ => Ok(normalize_snapshot_text(contents)),
    }
}

fn normalize_sarif_snapshot_contents(path: &Path, contents: &str) -> Result<String, String> {
    let value: serde_json::Value = serde_json::from_str(contents)
        .map_err(|error| format!("failed to parse {} as JSON: {error}", path.display()))?;
    let value = normalize_sarif_snapshot_value(value);
    diag_core::canonical_json(&value)
        .map_err(|error| format!("failed to canonicalize {}: {error}", path.display()))
}

fn normalize_ir_snapshot_contents(path: &Path, contents: &str) -> Result<String, String> {
    let mut document: DiagnosticDocument = serde_json::from_str(contents).map_err(|error| {
        format!(
            "failed to parse {} as diagnostic IR: {error}",
            path.display()
        )
    })?;
    normalize_diagnostic_document_for_snapshot_compare(&mut document);
    diag_core::canonical_json(&normalized_ir_snapshot_value(&document))
        .map_err(|error| format!("failed to canonicalize {}: {error}", path.display()))
}

fn normalize_snapshot_text(contents: &str) -> String {
    let contents = normalize_transient_object_paths(contents);
    let contents = normalize_gcc_quote_style(&contents);
    let contents = normalize_volatile_compiler_text(&contents);
    normalize_transient_line_numbers(&contents)
}

fn normalize_volatile_compiler_text(contents: &str) -> String {
    let contents = contents
        .replace("'{{'", "'{'")
        .replace("'}}'", "'}'")
        .replace("[-Werror=", "[-W")
        .replace("\"  candidate", "\"candidate")
        .replace("\"  template", "\"template")
        .replace("\"  deduced", "\"deduced");
    let contents = normalize_linker_offsets(&contents);
    let mut normalized = String::with_capacity(contents.len());
    for segment in contents.split_inclusive('\n') {
        let (line, newline) = segment
            .strip_suffix('\n')
            .map(|line| (line, "\n"))
            .unwrap_or((segment, ""));
        let line = normalize_candidate_line(line);
        if line.trim() == "cc1: all warnings being treated as errors"
            || is_candidate_count_line(&line)
        {
            continue;
        }
        let line = normalize_marker_line(&line);
        normalized.push_str(&line);
        normalized.push_str(newline);
    }
    normalized
}

fn normalize_linker_offsets(contents: &str) -> String {
    let mut normalized = String::with_capacity(contents.len());
    let bytes = contents.as_bytes();
    let mut index = 0;

    while index < bytes.len() {
        if bytes[index] == b'+'
            && index + 3 < bytes.len()
            && bytes[index + 1] == b'0'
            && bytes[index + 2] == b'x'
        {
            let mut digit_end = index + 3;
            while digit_end < bytes.len() && bytes[digit_end].is_ascii_hexdigit() {
                digit_end += 1;
            }
            if digit_end > index + 3 {
                normalized.push_str("+0x0");
                index = digit_end;
                continue;
            }
        }
        normalized.push(bytes[index] as char);
        index += 1;
    }

    normalized
}

fn normalize_candidate_line(line: &str) -> String {
    let mut normalized = String::with_capacity(line.len());
    let bytes = line.as_bytes();
    let mut index = 0;

    while index < bytes.len() {
        if line[index..].starts_with("candidate ") {
            let digit_start = index + "candidate ".len();
            let mut digit_end = digit_start;
            while digit_end < bytes.len() && bytes[digit_end].is_ascii_digit() {
                digit_end += 1;
            }
            if digit_end > digit_start && digit_end < bytes.len() && bytes[digit_end] == b':' {
                normalized.push_str("candidate:");
                index = digit_end + 1;
                continue;
            }
        }
        normalized.push(bytes[index] as char);
        index += 1;
    }

    let normalized = normalized
        .replace("note:   candidate", "note: candidate")
        .replace("note:   template", "note: template")
        .replace("note:   deduced", "note: deduced");
    if let Some(rest) = normalized.strip_prefix("  candidate") {
        return format!("candidate{rest}");
    }
    if let Some(rest) = normalized.strip_prefix("  template") {
        return format!("template{rest}");
    }
    if let Some(rest) = normalized.strip_prefix("  deduced") {
        return format!("deduced{rest}");
    }
    normalized
}

fn is_candidate_count_line(line: &str) -> bool {
    let trimmed = line.trim();
    if let Some(note_start) = trimmed.find("note: there are ") {
        let rest = &trimmed[note_start + "note: there are ".len()..];
        return rest.ends_with(" candidates");
    }
    trimmed.contains("note: there is 1 candidate")
}

fn normalize_marker_line(line: &str) -> String {
    let Some(pipe_index) = line.find('|') else {
        return line.to_string();
    };
    let after_pipe = &line[pipe_index + 1..];
    let trimmed = after_pipe.trim();
    if trimmed.is_empty() || !trimmed.chars().all(|ch| matches!(ch, '^' | '~')) {
        return line.to_string();
    }
    let marker_indent = after_pipe
        .chars()
        .take_while(|ch| *ch == ' ')
        .collect::<String>();
    format!("{}|{}^", &line[..pipe_index], marker_indent)
}

fn normalize_transient_object_paths(contents: &str) -> String {
    let mut normalized = String::with_capacity(contents.len());
    let mut remaining = contents;

    while let Some(start) = remaining.find("/tmp/") {
        normalized.push_str(&remaining[..start]);
        let candidate = &remaining[start..];
        let path_len = candidate
            .chars()
            .take_while(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '/' | '.' | '_' | '-'))
            .map(char::len_utf8)
            .sum::<usize>();
        let path = &candidate[..path_len];
        if path.starts_with("/tmp/") && path.ends_with(".o") {
            normalized.push_str("/tmp/<object>.o");
            remaining = &candidate[path_len..];
        } else {
            normalized.push_str("/tmp/");
            remaining = &candidate["/tmp/".len()..];
        }
    }

    normalized.push_str(remaining);
    normalized
}

fn normalize_gcc_quote_style(contents: &str) -> String {
    contents
        .chars()
        .map(|ch| match ch {
            '`' | '‘' | '’' => '\'',
            '“' | '”' => '"',
            _ => ch,
        })
        .collect()
}

fn normalize_transient_line_numbers(contents: &str) -> String {
    let contents = replace_number_after_marker(contents, "\"line\": ");
    let contents = replace_number_after_marker(&contents, "\"end_line\": ");
    let contents = replace_number_after_marker(&contents, "\"startLine\": ");
    let contents = replace_number_after_marker(&contents, "\"endLine\": ");
    let contents = normalize_gutter_line_numbers(&contents);
    normalize_colon_number_sequences(&contents)
}

fn replace_number_after_marker(contents: &str, marker: &str) -> String {
    let mut normalized = String::with_capacity(contents.len());
    let mut remaining = contents;

    while let Some(offset) = remaining.find(marker) {
        let marker_end = offset + marker.len();
        normalized.push_str(&remaining[..marker_end]);
        let tail = &remaining[marker_end..];
        let digit_len = tail
            .chars()
            .take_while(|ch| ch.is_ascii_digit())
            .map(char::len_utf8)
            .sum::<usize>();
        if digit_len == 0 {
            remaining = tail;
            continue;
        }
        normalized.push('1');
        remaining = &tail[digit_len..];
    }

    normalized.push_str(remaining);
    normalized
}

fn normalize_gutter_line_numbers(contents: &str) -> String {
    let mut normalized = String::with_capacity(contents.len());
    for segment in contents.split_inclusive('\n') {
        let (line, newline) = segment
            .strip_suffix('\n')
            .map(|line| (line, "\n"))
            .unwrap_or((segment, ""));
        let indent_len = line
            .chars()
            .take_while(|ch| matches!(ch, ' ' | '\t'))
            .map(char::len_utf8)
            .sum::<usize>();
        let rest = &line[indent_len..];
        let digit_len = rest
            .chars()
            .take_while(|ch| ch.is_ascii_digit())
            .map(char::len_utf8)
            .sum::<usize>();
        if digit_len > 0 && rest[digit_len..].starts_with(" |") {
            normalized.push_str(&line[..indent_len]);
            normalized.push('1');
            normalized.push_str(&rest[digit_len..]);
        } else {
            normalized.push_str(line);
        }
        normalized.push_str(newline);
    }
    normalized
}

fn normalize_colon_number_sequences(contents: &str) -> String {
    let mut normalized = String::with_capacity(contents.len());
    let bytes = contents.as_bytes();
    let mut index = 0;

    while index < bytes.len() {
        if bytes[index] == b':' {
            let digit_start = index + 1;
            let mut digit_end = digit_start;
            while digit_end < bytes.len() && bytes[digit_end].is_ascii_digit() {
                digit_end += 1;
            }
            if digit_end > digit_start && digit_end < bytes.len() && bytes[digit_end] == b':' {
                normalized.push(':');
                normalized.push('1');
                index = digit_end;
                continue;
            }
        }
        normalized.push(bytes[index] as char);
        index += 1;
    }

    normalized
}

fn normalize_sarif_snapshot_value(value: serde_json::Value) -> serde_json::Value {
    let mut normalized = serde_json::Map::new();
    if let Some(version) = value.get("version").cloned() {
        normalized.insert("version".to_string(), version);
    }
    if let Some(runs) = value.get("runs").and_then(serde_json::Value::as_array) {
        normalized.insert(
            "runs".to_string(),
            serde_json::Value::Array(
                runs.iter()
                    .map(|run| {
                        let mut normalized_run = serde_json::Map::new();
                        if let Some(results) =
                            run.get("results").and_then(serde_json::Value::as_array)
                        {
                            normalized_run.insert(
                                "results".to_string(),
                                serde_json::Value::Array(
                                    results.iter().map(normalize_sarif_result).collect(),
                                ),
                            );
                        }
                        serde_json::Value::Object(normalized_run)
                    })
                    .collect(),
            ),
        );
    }
    serde_json::Value::Object(normalized)
}

fn normalize_sarif_result(result: &serde_json::Value) -> serde_json::Value {
    let mut normalized = serde_json::Map::new();
    if let Some(level) = result.get("level").and_then(serde_json::Value::as_str) {
        normalized.insert(
            "level".to_string(),
            serde_json::Value::String(level.to_string()),
        );
    }
    if let Some(message) = result.get("message")
        && let Some(message) = normalize_sarif_message(message)
    {
        normalized.insert("message".to_string(), message);
    }
    serde_json::Value::Object(normalized)
}

fn normalize_sarif_message(message: &serde_json::Value) -> Option<serde_json::Value> {
    if let Some(text) = message.get("text").and_then(serde_json::Value::as_str) {
        return Some(serde_json::json!({ "text": normalize_snapshot_text(text) }));
    }
    message
        .get("markdown")
        .and_then(serde_json::Value::as_str)
        .map(|markdown| serde_json::json!({ "markdown": normalize_snapshot_text(markdown) }))
}

fn normalized_ir_snapshot_value(document: &DiagnosticDocument) -> serde_json::Value {
    let mut normalized = serde_json::Map::new();
    normalized.insert(
        "document_completeness".to_string(),
        serde_json::json!(document.document_completeness),
    );
    normalized.insert(
        "document_id".to_string(),
        serde_json::Value::String(document.document_id.clone()),
    );
    normalized.insert(
        "schema_version".to_string(),
        serde_json::Value::String(document.schema_version.clone()),
    );
    normalized.insert(
        "producer".to_string(),
        normalized_producer_info_value(&document.producer),
    );
    normalized.insert("run".to_string(), normalized_run_info_value(&document.run));
    if !document.captures.is_empty() {
        normalized.insert(
            "captures".to_string(),
            serde_json::Value::Array(
                document
                    .captures
                    .iter()
                    .map(normalized_capture_value)
                    .collect(),
            ),
        );
    }
    if !document.integrity_issues.is_empty() {
        normalized.insert(
            "integrity_issues".to_string(),
            serde_json::Value::Array(
                document
                    .integrity_issues
                    .iter()
                    .map(normalized_integrity_issue_value)
                    .collect(),
            ),
        );
    }
    if !document.diagnostics.is_empty() {
        normalized.insert(
            "diagnostics".to_string(),
            serde_json::Value::Array(
                document
                    .diagnostics
                    .iter()
                    .map(normalized_diagnostic_node_value)
                    .collect(),
            ),
        );
    }
    serde_json::Value::Object(normalized)
}

fn normalized_producer_info_value(producer: &diag_core::ProducerInfo) -> serde_json::Value {
    let mut normalized = serde_json::Map::new();
    normalized.insert(
        "name".to_string(),
        serde_json::Value::String(producer.name.clone()),
    );
    normalized.insert(
        "version".to_string(),
        serde_json::Value::String(producer.version.clone()),
    );
    if let Some(rulepack_version) = producer.rulepack_version.as_ref() {
        normalized.insert(
            "rulepack_version".to_string(),
            serde_json::Value::String(rulepack_version.clone()),
        );
    }
    serde_json::Value::Object(normalized)
}

fn normalized_run_info_value(run: &RunInfo) -> serde_json::Value {
    let mut normalized = serde_json::Map::new();
    normalized.insert(
        "exit_status".to_string(),
        serde_json::json!(run.exit_status),
    );
    normalized.insert(
        "invocation_id".to_string(),
        serde_json::Value::String(run.invocation_id.clone()),
    );
    if let Some(invoked_as) = run.invoked_as.as_ref() {
        normalized.insert(
            "invoked_as".to_string(),
            serde_json::Value::String(invoked_as.clone()),
        );
    }
    if !run.argv_redacted.is_empty() {
        normalized.insert(
            "argv_redacted".to_string(),
            serde_json::json!(run.argv_redacted),
        );
    }
    if let Some(cwd_display) = run.cwd_display.as_ref() {
        normalized.insert(
            "cwd_display".to_string(),
            serde_json::Value::String(cwd_display.clone()),
        );
    }
    normalized.insert(
        "primary_tool".to_string(),
        normalized_tool_info_value(&run.primary_tool),
    );
    if let Some(language_mode) = run.language_mode.as_ref() {
        normalized.insert(
            "language_mode".to_string(),
            serde_json::json!(language_mode),
        );
    }
    if let Some(target_triple) = run.target_triple.as_ref() {
        normalized.insert(
            "target_triple".to_string(),
            serde_json::Value::String(target_triple.clone()),
        );
    }
    if let Some(wrapper_mode) = run.wrapper_mode.as_ref() {
        normalized.insert("wrapper_mode".to_string(), serde_json::json!(wrapper_mode));
    }
    serde_json::Value::Object(normalized)
}

fn normalized_tool_info_value(tool: &diag_core::ToolInfo) -> serde_json::Value {
    let mut normalized = serde_json::Map::new();
    normalized.insert(
        "name".to_string(),
        serde_json::Value::String(tool.name.clone()),
    );
    if let Some(vendor) = tool.vendor.as_ref() {
        normalized.insert(
            "vendor".to_string(),
            serde_json::Value::String(vendor.clone()),
        );
    }
    serde_json::Value::Object(normalized)
}

fn normalized_capture_value(capture: &CaptureArtifact) -> serde_json::Value {
    let mut normalized = serde_json::Map::new();
    normalized.insert(
        "id".to_string(),
        serde_json::Value::String(capture.id.clone()),
    );
    normalized.insert("kind".to_string(), serde_json::json!(capture.kind));
    normalized.insert("storage".to_string(), serde_json::json!(capture.storage));
    if let Some(inline_text) = capture.inline_text.as_ref() {
        normalized.insert(
            "inline_text".to_string(),
            serde_json::Value::String(inline_text.clone()),
        );
    }
    if let Some(external_ref) = capture.external_ref.as_ref() {
        normalized.insert(
            "external_ref".to_string(),
            serde_json::Value::String(external_ref.clone()),
        );
    }
    serde_json::Value::Object(normalized)
}

fn normalized_integrity_issue_value(issue: &diag_core::IntegrityIssue) -> serde_json::Value {
    let mut normalized = serde_json::Map::new();
    normalized.insert(
        "message".to_string(),
        serde_json::Value::String(issue.message.clone()),
    );
    normalized.insert("severity".to_string(), serde_json::json!(issue.severity));
    normalized.insert("stage".to_string(), serde_json::json!(issue.stage));
    if let Some(provenance) = issue.provenance.as_ref() {
        normalized.insert(
            "provenance".to_string(),
            normalized_provenance_value(provenance),
        );
    }
    serde_json::Value::Object(normalized)
}

fn normalized_diagnostic_node_value(node: &diag_core::DiagnosticNode) -> serde_json::Value {
    let mut normalized = serde_json::Map::new();
    if let Some(analysis) = node.analysis.as_ref() {
        normalized.insert("analysis".to_string(), normalized_analysis_value(analysis));
    }
    if !node.children.is_empty() {
        normalized.insert(
            "children".to_string(),
            serde_json::Value::Array(
                node.children
                    .iter()
                    .map(normalized_diagnostic_node_value)
                    .collect(),
            ),
        );
    }
    if !node.context_chains.is_empty() {
        normalized.insert(
            "context_chains".to_string(),
            serde_json::Value::Array(
                node.context_chains
                    .iter()
                    .map(normalized_context_chain_value)
                    .collect(),
            ),
        );
    }
    if matches!(node.semantic_role, diag_core::SemanticRole::Root) {
        normalized.insert("id".to_string(), serde_json::Value::String(node.id.clone()));
    }
    if !node.locations.is_empty() {
        normalized.insert(
            "locations".to_string(),
            serde_json::Value::Array(
                node.locations
                    .iter()
                    .map(normalized_location_value)
                    .collect(),
            ),
        );
    }
    normalized.insert(
        "message".to_string(),
        normalized_message_text_value(&node.message),
    );
    normalized.insert(
        "node_completeness".to_string(),
        serde_json::json!(node.node_completeness),
    );
    normalized.insert("origin".to_string(), serde_json::json!(node.origin));
    normalized.insert("phase".to_string(), serde_json::json!(node.phase));
    normalized.insert(
        "provenance".to_string(),
        normalized_provenance_value(&node.provenance),
    );
    normalized.insert(
        "semantic_role".to_string(),
        serde_json::json!(node.semantic_role),
    );
    normalized.insert("severity".to_string(), serde_json::json!(node.severity));
    serde_json::Value::Object(normalized)
}

fn normalized_analysis_value(analysis: &diag_core::AnalysisOverlay) -> serde_json::Value {
    let mut normalized = serde_json::Map::new();
    if let Some(confidence) = analysis.confidence.as_ref() {
        normalized.insert("confidence".to_string(), serde_json::json!(confidence));
    }
    if let Some(family) = analysis.family.as_ref() {
        normalized.insert(
            "family".to_string(),
            serde_json::Value::String(family.clone()),
        );
    }
    if let Some(first_action_hint) = analysis.first_action_hint.as_ref() {
        normalized.insert(
            "first_action_hint".to_string(),
            serde_json::Value::String(normalize_snapshot_text(first_action_hint)),
        );
    }
    if let Some(headline) = analysis.headline.as_ref() {
        normalized.insert(
            "headline".to_string(),
            serde_json::Value::String(normalize_snapshot_text(headline)),
        );
    }
    if let Some(rule_id) = analysis.rule_id.as_ref() {
        normalized.insert(
            "rule_id".to_string(),
            serde_json::Value::String(rule_id.clone()),
        );
    }
    if !analysis.matched_conditions.is_empty() {
        normalized.insert(
            "matched_conditions".to_string(),
            serde_json::Value::Array(
                analysis
                    .matched_conditions
                    .iter()
                    .map(|condition| serde_json::Value::String(condition.clone()))
                    .collect(),
            ),
        );
    }
    if let Some(suppression_reason) = analysis.suppression_reason.as_ref() {
        normalized.insert(
            "suppression_reason".to_string(),
            serde_json::Value::String(suppression_reason.clone()),
        );
    }
    serde_json::Value::Object(normalized)
}

fn normalized_context_chain_value(chain: &diag_core::ContextChain) -> serde_json::Value {
    let mut normalized = serde_json::Map::new();
    if !chain.frames.is_empty() {
        normalized.insert(
            "frames".to_string(),
            serde_json::Value::Array(
                chain
                    .frames
                    .iter()
                    .map(normalized_context_frame_value)
                    .collect(),
            ),
        );
    }
    normalized.insert("kind".to_string(), serde_json::json!(chain.kind));
    serde_json::Value::Object(normalized)
}

fn normalized_context_frame_value(frame: &diag_core::ContextFrame) -> serde_json::Value {
    let mut normalized = serde_json::Map::new();
    normalized.insert(
        "label".to_string(),
        serde_json::Value::String(frame.label.clone()),
    );
    if let Some(path) = frame.path.as_ref() {
        normalized.insert("path".to_string(), serde_json::Value::String(path.clone()));
    }
    serde_json::Value::Object(normalized)
}

fn normalized_location_value(location: &diag_core::Location) -> serde_json::Value {
    let mut normalized = serde_json::Map::new();
    if let Some(ownership) = location.ownership() {
        normalized.insert("ownership".to_string(), serde_json::json!(ownership));
    }
    normalized.insert(
        "path".to_string(),
        serde_json::Value::String(location.path_raw().to_string()),
    );
    serde_json::Value::Object(normalized)
}

fn normalized_message_text_value(message: &diag_core::MessageText) -> serde_json::Value {
    serde_json::json!({
        "raw_text": message.raw_text,
    })
}

fn normalized_provenance_value(provenance: &diag_core::Provenance) -> serde_json::Value {
    let mut normalized = serde_json::Map::new();
    if !provenance.capture_refs.is_empty() {
        normalized.insert(
            "capture_refs".to_string(),
            serde_json::json!(provenance.capture_refs),
        );
    }
    normalized.insert("source".to_string(), serde_json::json!(provenance.source));
    serde_json::Value::Object(normalized)
}

fn normalize_diagnostic_document_for_snapshot_compare(document: &mut DiagnosticDocument) {
    document.document_id = normalize_transient_object_paths(&document.document_id);
    document.producer.name = normalize_transient_object_paths(&document.producer.name);
    document.producer.version = normalize_transient_object_paths(&document.producer.version);
    if let Some(git_revision) = document.producer.git_revision.as_mut() {
        *git_revision = normalize_transient_object_paths(git_revision);
    }
    if let Some(build_profile) = document.producer.build_profile.as_mut() {
        *build_profile = normalize_transient_object_paths(build_profile);
    }
    if let Some(rulepack_version) = document.producer.rulepack_version.as_mut() {
        *rulepack_version = normalize_transient_object_paths(rulepack_version);
    }
    normalize_run_info_for_snapshot_compare(&mut document.run);
    for capture in &mut document.captures {
        normalize_capture_for_snapshot_compare(capture);
    }
    for issue in &mut document.integrity_issues {
        issue.message = normalize_snapshot_text(&issue.message);
    }
    for diagnostic in &mut document.diagnostics {
        normalize_diagnostic_node_for_snapshot_compare(diagnostic);
    }
    document.fingerprints = None;
    document.refresh_fingerprints();
    document.fingerprints = None;
}

fn normalize_run_info_for_snapshot_compare(run: &mut RunInfo) {
    run.invocation_id = normalize_transient_object_paths(&run.invocation_id);
    if let Some(invoked_as) = run.invoked_as.as_mut() {
        *invoked_as = normalize_transient_object_paths(invoked_as);
    }
    for arg in &mut run.argv_redacted {
        *arg = normalize_transient_object_paths(arg);
    }
    if let Some(cwd_display) = run.cwd_display.as_mut() {
        *cwd_display = normalize_transient_object_paths(cwd_display);
    }
    normalize_tool_info_for_snapshot_compare(&mut run.primary_tool);
    for tool in &mut run.secondary_tools {
        normalize_tool_info_for_snapshot_compare(tool);
    }
    if let Some(target_triple) = run.target_triple.as_mut() {
        *target_triple = normalize_transient_object_paths(target_triple);
    }
}

fn normalize_tool_info_for_snapshot_compare(tool: &mut diag_core::ToolInfo) {
    tool.name = normalize_transient_object_paths(&tool.name);
    if let Some(version) = tool.version.as_mut() {
        *version = normalize_transient_object_paths(version);
    }
    if let Some(component) = tool.component.as_mut() {
        *component = normalize_transient_object_paths(component);
    }
    if let Some(vendor) = tool.vendor.as_mut() {
        *vendor = normalize_transient_object_paths(vendor);
    }
}

fn normalize_capture_for_snapshot_compare(capture: &mut CaptureArtifact) {
    capture.id = normalize_transient_object_paths(&capture.id);
    capture.media_type = normalize_transient_object_paths(&capture.media_type);
    if let Some(encoding) = capture.encoding.as_mut() {
        *encoding = normalize_transient_object_paths(encoding);
    }
    if let Some(digest_sha256) = capture.digest_sha256.as_mut() {
        *digest_sha256 = normalize_transient_object_paths(digest_sha256);
    }
    if let Some(inline_text) = capture.inline_text.as_mut() {
        *inline_text = normalize_snapshot_text(inline_text);
        capture.size_bytes = Some(inline_text.len() as u64);
    }
    if let Some(external_ref) = capture.external_ref.as_mut() {
        *external_ref = normalize_transient_object_paths(external_ref);
    }
    if matches!(capture.kind, ArtifactKind::GccSarif) {
        capture.size_bytes = None;
    }
    if let Some(produced_by) = capture.produced_by.as_mut() {
        normalize_tool_info_for_snapshot_compare(produced_by);
    }
}

fn normalize_diagnostic_node_for_snapshot_compare(node: &mut diag_core::DiagnosticNode) {
    node.id = normalize_transient_object_paths(&node.id);
    normalize_message_text_for_snapshot_compare(&mut node.message);
    for location in &mut node.locations {
        normalize_location_for_snapshot_compare(location);
    }
    node.suggestions.clear();
    node.symbol_context = None;
    for child in &mut node.children {
        normalize_diagnostic_node_for_snapshot_compare(child);
    }
    for frame in node
        .context_chains
        .iter_mut()
        .flat_map(|chain| &mut chain.frames)
    {
        frame.label = normalize_snapshot_text(&frame.label);
        if let Some(path) = frame.path.as_mut() {
            *path = normalize_transient_object_paths(path);
        }
    }
    node.fingerprints = None;
}

fn normalize_message_text_for_snapshot_compare(message: &mut diag_core::MessageText) {
    message.raw_text = normalize_snapshot_text(&message.raw_text);
    if let Some(normalized_text) = message.normalized_text.as_mut() {
        *normalized_text = normalize_snapshot_text(normalized_text);
    }
    if let Some(locale) = message.locale.as_mut() {
        *locale = normalize_transient_object_paths(locale);
    }
}

fn normalize_location_for_snapshot_compare(location: &mut diag_core::Location) {
    location.file.path_raw = normalize_transient_object_paths(location.path_raw());
    if let Some(display_path) = location.file.display_path.as_mut() {
        *display_path = normalize_transient_object_paths(display_path);
    }
}

#[cfg(test)]
mod tests {
    use super::{SnapshotDiffKind, compare_snapshot_contents, normalize_snapshot_contents};
    use std::path::Path;

    #[test]
    fn normalizes_sarif_snapshots_before_compare() {
        let expected = r#"{
  "version":"2.1.0",
  "runs":[
    {
      "results":[
        {
          "level":"error",
          "ruleId":"error",
          "message":{"text":"link failed for ‘/tmp/helper.o’ and ‘/tmp/main.o’"},
          "locations":[
            {
              "physicalLocation":{
                "artifactLocation":{"uri":"src/main.c"},
                "region":{"startLine":2,"startColumn":25}
              }
            }
          ]
        }
      ]
    }
  ]
}"#;
        let actual = r#"{
  "$schema": "https://docs.oasis-open.org/sarif/sarif/v2.1.0/errata01/os/schemas/sarif-schema-2.1.0.json",
  "runs": [
    {
      "artifacts": [
        {
          "location": {
            "uri": "src/main.c"
          }
        }
      ],
      "results": [
        {
          "level": "error",
          "locations": [
            {
              "id": 0,
              "physicalLocation": {
                "artifactLocation": {
                  "uri": "src/main.c",
                  "uriBaseId": "%SRCROOT%"
                },
                "region": {
                  "startLine": 2,
                  "startColumn": 25
                }
              }
            }
          ],
          "message": {
            "text": "link failed for '/tmp/cc123456.o' and '/tmp/cc654321.o'"
          }
        }
      ]
    }
  ],
  "version": "2.1.0"
}"#;

        let normalized_expected =
            normalize_snapshot_contents(Path::new("diagnostics.sarif"), expected).unwrap();
        let normalized_actual =
            normalize_snapshot_contents(Path::new("diagnostics.sarif"), actual).unwrap();

        assert_eq!(normalized_expected, normalized_actual);
    }

    #[test]
    fn normalizes_ir_snapshots_before_compare() {
        let expected = r#"{
  "captures": [
    {
      "id": "stderr.raw",
      "inline_text": "/usr/bin/ld: /tmp/helper.o: in function `duplicate':\nhelper.c:(.text+0x0): multiple definition of `duplicate'; /tmp/main.o:main.c:(.text+0x0): first defined here\ncollect2: error: ld returned 1 exit status\n",
      "kind": "compiler_stderr_text",
      "media_type": "text/plain",
      "size_bytes": 205,
      "storage": "inline"
    },
    {
      "external_ref": "<capture:diagnostics.sarif>",
      "id": "diagnostics.sarif",
      "kind": "gcc_sarif",
      "media_type": "application/sarif+json",
      "size_bytes": 44,
      "storage": "external_ref"
    }
  ],
  "diagnostics": [
    {
      "analysis": {
        "family": "linker.multiple_definition"
      },
      "fingerprints": {
        "family": "expected-family",
        "raw": "expected-raw",
        "structural": "expected-structural"
      },
      "id": "residual-1",
      "message": {
        "raw_text": "helper.c:(.text+0x0): multiple definition of ‘duplicate’; /tmp/main.o:main.c:(.text+0x0): first defined here"
      },
      "node_completeness": "partial",
      "origin": "linker",
      "phase": "link",
      "provenance": {
        "capture_refs": [
          "stderr.raw"
        ],
        "source": "residual_text"
      },
      "semantic_role": "root",
      "severity": "error"
    }
  ],
  "document_completeness": "partial",
  "document_id": "<document>",
  "fingerprints": {
    "family": "expected-document-family",
    "raw": "expected-document-raw",
    "structural": "expected-document-structural"
  },
  "producer": {
    "name": "gcc-formed",
    "version": "<normalized>"
  },
  "run": {
    "exit_status": 1,
    "invocation_id": "<invocation>",
    "primary_tool": {
      "name": "gcc",
      "vendor": "GNU"
    }
  },
  "schema_version": "1.0.0-alpha.1"
}"#;
        let actual = r#"{
  "captures": [
    {
      "id": "stderr.raw",
      "inline_text": "/usr/bin/ld: /tmp/cc123456.o: in function `duplicate':\nhelper.c:(.text+0x0): multiple definition of `duplicate'; /tmp/cc654321.o:main.c:(.text+0x0): first defined here\ncollect2: error: ld returned 1 exit status\n",
      "kind": "compiler_stderr_text",
      "media_type": "text/plain",
      "size_bytes": 211,
      "storage": "inline"
    },
    {
      "external_ref": "<capture:diagnostics.sarif>",
      "id": "diagnostics.sarif",
      "kind": "gcc_sarif",
      "media_type": "application/sarif+json",
      "size_bytes": 987,
      "storage": "external_ref"
    }
  ],
  "diagnostics": [
    {
      "analysis": {
        "family": "linker.multiple_definition"
      },
      "fingerprints": {
        "family": "actual-family",
        "raw": "actual-raw",
        "structural": "actual-structural"
      },
      "id": "residual-1",
      "message": {
        "raw_text": "helper.c:(.text+0x0): multiple definition of 'duplicate'; /tmp/cc654321.o:main.c:(.text+0x0): first defined here"
      },
      "node_completeness": "partial",
      "origin": "linker",
      "phase": "link",
      "provenance": {
        "capture_refs": [
          "stderr.raw"
        ],
        "source": "residual_text"
      },
      "semantic_role": "root",
      "severity": "error"
    }
  ],
  "document_completeness": "partial",
  "document_id": "<document>",
  "fingerprints": {
    "family": "actual-document-family",
    "raw": "actual-document-raw",
    "structural": "actual-document-structural"
  },
  "producer": {
    "name": "gcc-formed",
    "version": "<normalized>"
  },
  "run": {
    "exit_status": 1,
    "invocation_id": "<invocation>",
    "primary_tool": {
      "name": "gcc",
      "vendor": "GNU"
    }
  },
  "schema_version": "1.0.0-alpha.1"
}"#;

        let normalized_expected =
            normalize_snapshot_contents(Path::new("ir.analysis.json"), expected).unwrap();
        let normalized_actual =
            normalize_snapshot_contents(Path::new("ir.analysis.json"), actual).unwrap();

        assert_eq!(normalized_expected, normalized_actual);
    }

    #[test]
    fn normalizes_ir_location_span_drift_before_compare() {
        let expected = r#"{
  "captures": [],
  "diagnostics": [
    {
      "analysis": {
        "confidence": "high",
        "family": "macro_include",
        "first_action_hint": "inspect the user-owned include edge or macro invocation that triggers the error",
        "headline": "error surfaced through macro/include context"
      },
      "context_chains": [
        {
          "frames": [
            {
              "column": 25,
              "label": "src/main.c:2:25: note: in expansion of macro `CALL_BAD'",
              "line": 2,
              "path": "src/main.c"
            }
          ],
          "kind": "macro_expansion"
        }
      ],
      "fingerprints": {
        "family": "expected-family",
        "raw": "expected-raw",
        "structural": "expected-structural"
      },
      "id": "sarif-0-0",
      "locations": [
        {
          "column": 25,
          "line": 2,
          "ownership": "user",
          "path": "src/main.c"
        }
      ],
      "message": {
        "raw_text": "`missing_symbol' undeclared"
      },
      "node_completeness": "complete",
      "origin": "gcc",
      "phase": "semantic",
      "provenance": {
        "capture_refs": [
          "diagnostics.sarif"
        ],
        "source": "compiler"
      },
      "semantic_role": "root",
      "severity": "error"
    }
  ],
  "document_completeness": "complete",
  "document_id": "<document>",
  "fingerprints": {
    "family": "expected-document-family",
    "raw": "expected-document-raw",
    "structural": "expected-document-structural"
  },
  "producer": {
    "name": "gcc-formed",
    "rulepack_version": "phase1",
    "version": "<normalized>"
  },
  "run": {
    "argv_redacted": [
      "gcc",
      "src/main.c"
    ],
    "cwd_display": "<cwd>",
    "exit_status": 1,
    "invocation_id": "<invocation>",
    "invoked_as": "gcc-formed",
    "language_mode": "c",
    "primary_tool": {
      "name": "gcc",
      "vendor": "GNU"
    },
    "target_triple": "x86_64-unknown-linux-gnu",
    "wrapper_mode": "terminal"
  },
  "schema_version": "1.0.0-alpha.1"
}"#;
        let actual = r#"{
  "captures": [],
  "diagnostics": [
    {
      "analysis": {
        "confidence": "high",
        "family": "macro_include",
        "first_action_hint": "inspect the user-owned include edge or macro invocation that triggers the error",
        "headline": "error surfaced through macro/include context"
      },
      "context_chains": [
        {
          "frames": [
            {
              "column": 41,
              "label": "src/main.c:5:41: note: in expansion of macro ‘CALL_BAD’",
              "line": 5,
              "path": "src/main.c"
            }
          ],
          "kind": "macro_expansion"
        }
      ],
      "fingerprints": {
        "family": "actual-family",
        "raw": "actual-raw",
        "structural": "actual-structural"
      },
      "id": "sarif-0-0",
      "locations": [
        {
          "column": 41,
          "end_column": 42,
          "end_line": 5,
          "line": 5,
          "ownership": "user",
          "path": "src/main.c"
        }
      ],
      "message": {
        "raw_text": "‘missing_symbol’ undeclared"
      },
      "node_completeness": "complete",
      "origin": "gcc",
      "phase": "semantic",
      "provenance": {
        "capture_refs": [
          "diagnostics.sarif"
        ],
        "source": "compiler"
      },
      "semantic_role": "root",
      "severity": "error"
    }
  ],
  "document_completeness": "complete",
  "document_id": "<document>",
  "fingerprints": {
    "family": "actual-document-family",
    "raw": "actual-document-raw",
    "structural": "actual-document-structural"
  },
  "producer": {
    "name": "gcc-formed",
    "rulepack_version": "phase1",
    "version": "<normalized>"
  },
  "run": {
    "argv_redacted": [
      "gcc",
      "src/main.c"
    ],
    "cwd_display": "<cwd>",
    "exit_status": 1,
    "invocation_id": "<invocation>",
    "invoked_as": "gcc-formed",
    "language_mode": "c",
    "primary_tool": {
      "name": "gcc",
      "vendor": "GNU"
    },
    "target_triple": "x86_64-unknown-linux-gnu",
    "wrapper_mode": "terminal"
  },
  "schema_version": "1.0.0-alpha.1"
}"#;

        let normalized_expected =
            normalize_snapshot_contents(Path::new("ir.analysis.json"), expected).unwrap();
        let normalized_actual =
            normalize_snapshot_contents(Path::new("ir.analysis.json"), actual).unwrap();

        assert_eq!(normalized_expected, normalized_actual);
    }

    #[test]
    fn normalizes_transient_line_numbers_before_compare() {
        let expected = "src/main.c:2:25: note: in expansion of macro 'CALL_BAD'\n    2 | int main(void) { return CALL_BAD(); }\n";
        let actual = "src/main.c:3:25: note: in expansion of macro 'CALL_BAD'\n    3 | int main(void) { return CALL_BAD(); }\n";

        let normalized_expected =
            normalize_snapshot_contents(Path::new("stderr.raw"), expected).unwrap();
        let normalized_actual =
            normalize_snapshot_contents(Path::new("stderr.raw"), actual).unwrap();

        assert_eq!(normalized_expected, normalized_actual);
    }

    #[test]
    fn normalizes_transient_column_numbers_in_location_headers() {
        let expected = "src/main.c:2:25: error: incompatible types\n";
        let actual = "src/main.c:5:41: error: incompatible types\n";

        let normalized_expected =
            normalize_snapshot_contents(Path::new("stderr.raw"), expected).unwrap();
        let normalized_actual =
            normalize_snapshot_contents(Path::new("stderr.raw"), actual).unwrap();

        assert_eq!(normalized_expected, normalized_actual);
    }

    #[test]
    fn normalizes_volatile_compiler_text_patterns() {
        let expected = "src/main.cpp:4:5: note: candidate: 'void takes(int, int)'\n    4 |     takes(1);\n      |     ^~~~~\ncc1: all warnings being treated as errors\n/usr/bin/ld: /tmp/main.o: in function 'main':\nmain.c:(.text+0x9): undefined reference to 'missing_symbol'\n";
        let actual = "src/main.cpp:7:9: note: there are 2 candidates\nsrc/main.cpp:7:9: note: candidate 1: 'void takes(int, int)'\n    7 |     takes(1);\n      |     ~~~~~^~~\n/usr/bin/ld: /tmp/cc123456.o: in function 'main':\nmain.c:(.text+0x5): undefined reference to 'missing_symbol'\n";

        let normalized_expected =
            normalize_snapshot_contents(Path::new("stderr.raw"), expected).unwrap();
        let normalized_actual =
            normalize_snapshot_contents(Path::new("stderr.raw"), actual).unwrap();

        assert_eq!(normalized_expected, normalized_actual);
    }

    #[test]
    fn classifies_normalization_only_drift_separately() {
        let expected = "src/main.c:2:25: error: incompatible types\n";
        let actual = "src/main.c:5:41: error: incompatible types\n";

        let comparison =
            compare_snapshot_contents(Path::new("stderr.raw"), expected, actual).unwrap();

        assert_eq!(comparison.diff_kind, SnapshotDiffKind::NormalizationOnly);
        assert!(comparison.matches_after_normalization());
    }

    #[test]
    fn classifies_semantic_snapshot_drift() {
        let expected = "src/main.c:2:25: error: incompatible types\n";
        let actual = "src/main.c:2:25: error: undefined reference to missing_symbol\n";

        let comparison =
            compare_snapshot_contents(Path::new("stderr.raw"), expected, actual).unwrap();

        assert_eq!(comparison.diff_kind, SnapshotDiffKind::Semantic);
        assert!(!comparison.matches_after_normalization());
    }
}
