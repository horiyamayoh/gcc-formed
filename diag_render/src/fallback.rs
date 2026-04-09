use crate::formatter::sanitize_display_line;
use crate::{DebugRefs, RenderRequest, RenderResult};
use diag_core::FallbackReason;

pub fn render_fallback(request: &RenderRequest, fallback_reason: FallbackReason) -> RenderResult {
    let mut lines = vec![
        "error: showing a conservative wrapper view; original compiler diagnostics are preserved"
            .to_string(),
    ];
    lines.push(format!(
        "note: fallback reason = {}",
        fallback_reason.as_str()
    ));
    lines.push("raw:".to_string());

    let mut raw_lines = Vec::new();
    for node in &request.document.diagnostics {
        for line in node.message.raw_text.lines() {
            raw_lines.push(line.to_string());
        }
    }
    if raw_lines.is_empty() {
        if let Some(stderr) = request
            .document
            .captures
            .iter()
            .find(|capture| capture.id == "stderr.raw")
            .and_then(|capture| capture.inline_text.as_ref())
        {
            raw_lines.extend(stderr.lines().map(ToOwned::to_owned));
        }
    }
    for line in raw_lines {
        lines.push(format!("  {}", sanitize_display_line(&line, false)));
    }
    if matches!(request.debug_refs, DebugRefs::TraceId) {
        lines.push(format!("trace: {}", request.document.run.invocation_id));
    }
    RenderResult {
        text: lines.join("\n"),
        used_analysis: false,
        used_fallback: true,
        fallback_reason: Some(fallback_reason),
        displayed_group_refs: request
            .document
            .diagnostics
            .iter()
            .map(|node| node.id.clone())
            .collect(),
        suppressed_group_count: 0,
        suppressed_warning_count: 0,
        truncation_occurred: false,
        render_issues: Vec::new(),
    }
}
