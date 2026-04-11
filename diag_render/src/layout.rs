use crate::theme::ThemePolicy;
use crate::view_model::RenderGroupCard;
use crate::{RenderProfile, RenderRequest};

#[derive(Debug, Clone, Copy)]
pub(crate) struct LayoutProfile {
    path_first_primary_line: bool,
    show_location_line: bool,
    raw_block_indent: &'static str,
}

impl LayoutProfile {
    pub(crate) fn for_request(request: &RenderRequest) -> Self {
        match request.profile {
            RenderProfile::Ci => Self {
                path_first_primary_line: true,
                show_location_line: false,
                raw_block_indent: "  ",
            },
            _ => Self {
                path_first_primary_line: false,
                show_location_line: true,
                raw_block_indent: "  ",
            },
        }
    }

    pub(crate) fn render_card(
        &self,
        theme: &ThemePolicy,
        card: &RenderGroupCard,
        lines: &mut Vec<String>,
    ) {
        lines.push(self.primary_line(theme, card));
        if self.show_location_line
            && let Some(location) = card.canonical_location.as_ref()
        {
            lines.push(format!("--> {}", theme.inline(location)));
        }
        if let Some(confidence_notice) = card.confidence_notice.as_ref() {
            lines.push(confidence_notice.clone());
        }
        if let Some(first_action) = card.first_action.as_ref() {
            lines.push(format!("help: {}", theme.inline(first_action)));
        }
        lines.push(format!("why: {}", theme.raw(&card.raw_message)));
        for excerpt in &card.excerpts {
            lines.push(format!("| {}", theme.inline(&excerpt.location)));
            for source in &excerpt.lines {
                lines.push(format!("| {}", theme.inline(source)));
            }
        }
        for context in &card.context_lines {
            lines.push(theme.inline(context));
        }
        for note in &card.child_notes {
            lines.push(format!("note: {}", theme.inline(note)));
        }
        for notice in &card.collapsed_notices {
            lines.push(format!("note: {}", theme.inline(notice)));
        }
        if !card.raw_sub_block.is_empty() {
            lines.push(card.raw_block_label.clone());
            for raw_line in &card.raw_sub_block {
                lines.push(format!("{}{}", self.raw_block_indent, theme.raw(raw_line)));
            }
        }
    }

    fn primary_line(&self, theme: &ThemePolicy, card: &RenderGroupCard) -> String {
        if self.path_first_primary_line {
            return card
                .canonical_location
                .as_ref()
                .map(|location| {
                    format!(
                        "{}: {}: {}",
                        theme.inline(location),
                        card.severity,
                        theme.inline(&card.title)
                    )
                })
                .unwrap_or_else(|| {
                    let prefix = card
                        .family
                        .as_deref()
                        .filter(|family| family.starts_with("linker"))
                        .map(|_| "linker: ".to_string())
                        .unwrap_or_default();
                    format!(
                        "{}{severity}: {title}",
                        prefix,
                        severity = card.severity,
                        title = theme.inline(&card.title)
                    )
                });
        }

        format!("{}: {}", card.severity, theme.inline(&card.title))
    }
}
