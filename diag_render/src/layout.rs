use crate::presentation::SemanticSlotId;
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
        LegacyPresentationAdapter::new(self, theme, card).render(lines);
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

struct LegacyPresentationAdapter<'a> {
    layout: &'a LayoutProfile,
    theme: &'a ThemePolicy,
    card: &'a RenderGroupCard,
}

impl<'a> LegacyPresentationAdapter<'a> {
    fn new(layout: &'a LayoutProfile, theme: &'a ThemePolicy, card: &'a RenderGroupCard) -> Self {
        Self {
            layout,
            theme,
            card,
        }
    }

    fn render(&self, lines: &mut Vec<String>) {
        lines.push(self.layout.primary_line(self.theme, self.card));
        if self.layout.show_location_line
            && let Some(location) = self.card.canonical_location.as_ref()
        {
            lines.push(format!("--> {}", self.theme.inline(location)));
        }
        if let Some(confidence_notice) = self.card.confidence_notice.as_ref() {
            lines.push(confidence_notice.clone());
        }
        if let Some(first_action) = self
            .card
            .semantic_card
            .slot_text(SemanticSlotId::FirstAction)
            .or(self.card.first_action.as_deref())
        {
            lines.push(format!("help: {}", self.theme.inline(first_action)));
        }
        let why_label = self
            .card
            .semantic_card
            .slot_label(SemanticSlotId::WhyRaw)
            .unwrap_or("why");
        let why_text = self
            .card
            .semantic_card
            .slot_text(SemanticSlotId::WhyRaw)
            .unwrap_or(&self.card.raw_message);
        lines.push(format!("{why_label}: {}", self.theme.raw(why_text)));
        for excerpt in &self.card.excerpts {
            lines.push(format!("| {}", self.theme.inline(&excerpt.location)));
            for source in &excerpt.lines {
                lines.push(format!("| {}", self.theme.inline(source)));
            }
            for annotation in &excerpt.annotations {
                lines.push(format!("| {}", self.theme.inline(annotation)));
            }
        }
        for context in &self.card.context_lines {
            lines.push(self.theme.inline(context));
        }
        for note in &self.card.child_notes {
            lines.push(format!("note: {}", self.theme.inline(note)));
        }
        for notice in &self.card.collapsed_notices {
            lines.push(format!("note: {}", self.theme.inline(notice)));
        }
        for suggestion in &self.card.suggestions {
            lines.push(format!(
                "{}: {}",
                suggestion.label,
                self.theme.inline(&suggestion.text)
            ));
            for patch_line in &suggestion.inline_patch {
                lines.push(format!("  {}", self.theme.inline(patch_line)));
            }
        }
        if !self.card.raw_sub_block.is_empty() {
            lines.push(self.card.raw_block_label.clone());
            for raw_line in &self.card.raw_sub_block {
                lines.push(format!(
                    "{}{}",
                    self.layout.raw_block_indent,
                    self.theme.raw(raw_line)
                ));
            }
        }
    }
}
