use crate::theme::sanitize_display_line;
use crate::{DebugRefs, RenderProfile, RenderRequest, RenderResult};
use diag_core::FallbackReason;
use std::fs;

pub fn render_fallback(request: &RenderRequest, fallback_reason: FallbackReason) -> RenderResult {
    let text = if renders_quiet_passthrough(request, fallback_reason) {
        render_quiet_passthrough(request)
    } else {
        render_annotated_fallback(request, fallback_reason)
    };

    RenderResult {
        text,
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

fn renders_quiet_passthrough(request: &RenderRequest, fallback_reason: FallbackReason) -> bool {
    matches!(fallback_reason, FallbackReason::ResidualOnly)
        && !matches!(
            request.profile,
            RenderProfile::Verbose | RenderProfile::Debug | RenderProfile::RawFallback
        )
}

fn render_quiet_passthrough(request: &RenderRequest) -> String {
    let mut text = preserved_stderr_text(request).unwrap_or_else(|| {
        reconstructed_diagnostic_lines(request)
            .into_iter()
            .map(|line| sanitize_display_line(&line, false))
            .collect::<Vec<_>>()
            .join("\n")
    });
    append_debug_refs(request, &mut text);
    text
}

fn render_annotated_fallback(request: &RenderRequest, fallback_reason: FallbackReason) -> String {
    let mut lines = vec![
        "error: showing a conservative wrapper view; original compiler diagnostics are preserved"
            .to_string(),
    ];
    lines.push(format!(
        "note: fallback reason = {}",
        fallback_reason.as_str()
    ));
    lines.push("raw:".to_string());

    let (raw_lines, reconstructed) = if let Some(stderr_lines) = preserved_stderr_lines(request) {
        (stderr_lines, false)
    } else {
        (reconstructed_diagnostic_lines(request), true)
    };
    if reconstructed {
        lines.push(
            "note: raw stderr capture is unavailable; showing reconstructed diagnostic messages"
                .to_string(),
        );
    }
    for line in raw_lines {
        lines.push(format!("  {}", sanitize_display_line(&line, false)));
    }
    let mut text = lines.join("\n");
    append_debug_refs(request, &mut text);
    text
}

fn preserved_stderr_text(request: &RenderRequest) -> Option<String> {
    let capture = request
        .document
        .captures
        .iter()
        .find(|capture| capture.id == "stderr.raw")?;
    let text = if let Some(text) = capture.inline_text.as_ref() {
        Some(text.clone())
    } else {
        capture.external_ref.as_ref().and_then(|path| {
            fs::read(path)
                .ok()
                .map(|bytes| String::from_utf8_lossy(&bytes).into_owned())
        })
    }?;
    (!text.is_empty()).then_some(text)
}

fn preserved_stderr_lines(request: &RenderRequest) -> Option<Vec<String>> {
    preserved_stderr_text(request).map(|text| text.lines().map(ToOwned::to_owned).collect())
}

fn reconstructed_diagnostic_lines(request: &RenderRequest) -> Vec<String> {
    let mut raw_lines = Vec::new();
    for node in &request.document.diagnostics {
        for line in node.message.raw_text.lines() {
            raw_lines.push(line.to_string());
        }
    }
    raw_lines
}

fn append_debug_refs(request: &RenderRequest, text: &mut String) {
    match request.debug_refs {
        DebugRefs::None => {}
        DebugRefs::TraceId => append_line(
            text,
            format!("trace: {}", request.document.run.invocation_id),
        ),
        DebugRefs::CaptureRef => {
            let capture_ids = request
                .document
                .captures
                .iter()
                .map(|capture| capture.id.clone())
                .collect::<Vec<_>>()
                .join(", ");
            if !capture_ids.is_empty() {
                append_line(text, format!("captures: {capture_ids}"));
            }
        }
    }
}

fn append_line(text: &mut String, line: String) {
    if !text.is_empty() && !text.ends_with('\n') {
        text.push('\n');
    }
    text.push_str(&line);
}
