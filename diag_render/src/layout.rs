use crate::presentation::{LocationPlacement, RenderSemanticSlot, SemanticSlotId};
use crate::theme::ThemePolicy;
use crate::view_model::RenderGroupCard;
use crate::{RenderProfile, RenderRequest};

const DEFAULT_INLINE_LOCATION_SOFT_LIMIT: usize = 100;
const MIN_EVIDENCE_LABEL_WIDTH: usize = 4;

#[derive(Debug, Clone, Copy)]
pub(crate) struct LayoutProfile {
    path_first_primary_line: bool,
    show_location_line: bool,
    raw_block_indent: &'static str,
    ansi_color: bool,
    inline_location_soft_limit: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LocationHost {
    Header,
    Evidence,
    Excerpt,
    Dedicated,
    None,
}

impl LayoutProfile {
    pub(crate) fn for_request(request: &RenderRequest) -> Self {
        match request.profile {
            RenderProfile::Ci => Self {
                path_first_primary_line: true,
                show_location_line: false,
                raw_block_indent: "  ",
                ansi_color: request.capabilities.ansi_color,
                inline_location_soft_limit: request
                    .capabilities
                    .width_columns
                    .map(|width| width.min(DEFAULT_INLINE_LOCATION_SOFT_LIMIT))
                    .unwrap_or(DEFAULT_INLINE_LOCATION_SOFT_LIMIT),
            },
            _ => Self {
                path_first_primary_line: false,
                show_location_line: true,
                raw_block_indent: "  ",
                ansi_color: request.capabilities.ansi_color,
                inline_location_soft_limit: request
                    .capabilities
                    .width_columns
                    .map(|width| width.min(DEFAULT_INLINE_LOCATION_SOFT_LIMIT))
                    .unwrap_or(DEFAULT_INLINE_LOCATION_SOFT_LIMIT),
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

    fn primary_line(
        &self,
        theme: &ThemePolicy,
        card: &RenderGroupCard,
        location_host: LocationHost,
    ) -> String {
        let show_header_location = matches!(location_host, LocationHost::Header);
        if card.semantic_card.presentation.subject_first_header {
            if self.path_first_primary_line && card.canonical_location.is_some() {
                return self.render_subject_first_header(
                    theme,
                    card,
                    &card.semantic_card.presentation.header.ci_path_first_format,
                    true,
                );
            }
            let mut line = self.render_subject_first_header(
                theme,
                card,
                &card.semantic_card.presentation.header.interactive_format,
                false,
            );
            if show_header_location {
                self.append_inline_location_suffix(theme, card, &mut line);
            }
            return line;
        }

        if self.path_first_primary_line {
            return card
                .canonical_location
                .as_ref()
                .map(|location| {
                    format!(
                        "{}: {}: {}",
                        theme.inline(location),
                        self.style_severity(&card.severity),
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
                        "{}{}: {}",
                        prefix,
                        self.style_severity(&card.severity),
                        theme.inline(&card.title)
                    )
                });
        }

        let mut line = format!(
            "{}: {}",
            self.style_severity(&card.severity),
            theme.inline(&card.title)
        );
        if show_header_location {
            self.append_inline_location_suffix(theme, card, &mut line);
        }
        line
    }

    fn location_host(&self, card: &RenderGroupCard) -> LocationHost {
        let Some(_) = card.canonical_location.as_ref() else {
            return LocationHost::None;
        };
        if self.path_first_primary_line {
            return LocationHost::Header;
        }

        let policy = &card.semantic_card.presentation.location_policy;
        let mut placements = Vec::with_capacity(policy.fallback_order.len() + 1);
        placements.push(policy.default_placement);
        placements.extend(policy.fallback_order.iter().copied());

        for placement in placements {
            match placement {
                LocationPlacement::InlineSuffix => {
                    if self.can_host_location_in_header(card) {
                        return LocationHost::Header;
                    }
                    if self.has_evidence_host(card) {
                        return LocationHost::Evidence;
                    }
                    if !card.excerpts.is_empty() {
                        return LocationHost::Excerpt;
                    }
                }
                LocationPlacement::HeaderSuffix => {
                    if self.can_host_location_in_header(card) {
                        return LocationHost::Header;
                    }
                }
                LocationPlacement::EvidenceSuffix => {
                    if self.has_evidence_host(card) {
                        return LocationHost::Evidence;
                    }
                }
                LocationPlacement::ExcerptHeader => {
                    if !card.excerpts.is_empty() {
                        return LocationHost::Excerpt;
                    }
                }
                LocationPlacement::DedicatedLine => {
                    if self.show_location_line {
                        return LocationHost::Dedicated;
                    }
                }
                LocationPlacement::None => continue,
            }
        }

        if self.can_host_location_in_header(card) {
            LocationHost::Header
        } else if self.has_evidence_host(card) {
            LocationHost::Evidence
        } else if !card.excerpts.is_empty() {
            LocationHost::Excerpt
        } else if card.semantic_card.presentation.subject_first_header {
            LocationHost::Header
        } else if self.show_location_line {
            LocationHost::Dedicated
        } else {
            LocationHost::Header
        }
    }

    fn can_host_location_in_header(&self, card: &RenderGroupCard) -> bool {
        if self.path_first_primary_line {
            return true;
        }
        if card.canonical_location.is_none() {
            return false;
        }
        self.header_len_without_location(card) + self.inline_location_suffix_len(card)
            <= self.effective_inline_location_soft_limit(card)
    }

    fn has_evidence_host(&self, card: &RenderGroupCard) -> bool {
        card.semantic_card
            .slot_text(SemanticSlotId::FirstAction)
            .or(card.first_action.as_deref())
            .is_some()
            || !rendered_semantic_slots(card).is_empty()
            || card.semantic_card.slot_text(SemanticSlotId::Raw).is_some()
            || !card.semantic_card.presentation.subject_first_header
    }

    fn style_severity(&self, severity: &str) -> String {
        let ansi = match severity {
            "fatal" | "error" => Some("1;31"),
            "warning" => Some("1;33"),
            "note" => Some("1;36"),
            _ => None,
        };
        self.style_segment(severity, ansi)
    }

    fn style_family_tag(&self, family_tag: &str) -> String {
        self.style_segment(family_tag, Some("2"))
    }

    fn style_evidence_label(&self, label: &str, width: usize) -> String {
        self.style_segment(&format!("{label:width$}:", width = width), Some("36"))
    }

    fn style_segment(&self, text: &str, ansi: Option<&str>) -> String {
        if !self.ansi_color {
            return text.to_string();
        }
        let Some(ansi) = ansi else {
            return text.to_string();
        };
        format!("\u{1b}[{ansi}m{text}\u{1b}[0m")
    }

    fn render_subject_first_header(
        &self,
        theme: &ThemePolicy,
        card: &RenderGroupCard,
        template: &str,
        include_location: bool,
    ) -> String {
        let family = self.subject_first_family(card);
        let family_text = theme.inline(family);
        let family_tag = self.style_family_tag(&format!("[{}]", family_text));
        let subject = theme.inline(&card.title);
        self.render_header_template(
            template,
            &self.style_severity(&card.severity),
            &family_text,
            &family_tag,
            &subject,
            include_location
                .then_some(card.canonical_location.as_ref())
                .flatten()
                .map(|location| theme.inline(location)),
        )
    }

    fn render_header_template(
        &self,
        template: &str,
        severity: &str,
        family: &str,
        family_tag: &str,
        subject: &str,
        location: Option<String>,
    ) -> String {
        let mut rendered = template.replace("[{family}]", family_tag);
        rendered = rendered.replace("{severity}", severity);
        rendered = rendered.replace("{family}", family);
        rendered = rendered.replace("{subject}", subject);
        if let Some(location) = location {
            rendered.replace("{location}", &location)
        } else {
            rendered.replace("{location}", "")
        }
    }

    fn subject_first_family<'a>(&self, card: &'a RenderGroupCard) -> &'a str {
        card.semantic_card
            .display_family
            .as_deref()
            .or(card.family.as_deref())
            .unwrap_or(
                card.semantic_card
                    .presentation
                    .header
                    .unknown_family
                    .as_str(),
            )
    }

    fn append_inline_location_suffix(
        &self,
        theme: &ThemePolicy,
        card: &RenderGroupCard,
        line: &mut String,
    ) {
        if let Some(suffix) = self.inline_location_suffix(theme, card) {
            line.push_str(&suffix);
        }
    }

    fn inline_location_suffix(
        &self,
        theme: &ThemePolicy,
        card: &RenderGroupCard,
    ) -> Option<String> {
        let location = card.canonical_location.as_ref()?;
        Some(
            self.format_location_suffix(
                &card
                    .semantic_card
                    .presentation
                    .location_policy
                    .inline_suffix_format,
                &theme.inline(location),
            ),
        )
    }

    fn inline_location_suffix_len(&self, card: &RenderGroupCard) -> usize {
        let Some(location) = card.canonical_location.as_ref() else {
            return 0;
        };
        self.format_location_suffix(
            &card
                .semantic_card
                .presentation
                .location_policy
                .inline_suffix_format,
            location,
        )
        .chars()
        .count()
    }

    fn effective_inline_location_soft_limit(&self, card: &RenderGroupCard) -> usize {
        self.inline_location_soft_limit.min(
            card.semantic_card
                .presentation
                .location_policy
                .width_soft_limit,
        )
    }

    fn header_len_without_location(&self, card: &RenderGroupCard) -> usize {
        if card.semantic_card.presentation.subject_first_header {
            return self
                .render_header_template(
                    &card.semantic_card.presentation.header.interactive_format,
                    &card.severity,
                    self.subject_first_family(card),
                    &format!("[{}]", self.subject_first_family(card)),
                    &card.title,
                    None,
                )
                .chars()
                .count();
        }

        card.severity.chars().count() + ": ".len() + card.title.chars().count()
    }

    fn format_location_suffix(&self, template: &str, location: &str) -> String {
        if template.contains("{location}") {
            template.replace("{location}", location)
        } else {
            format!("{template}{location}")
        }
    }
}

struct LegacyPresentationAdapter<'a> {
    layout: &'a LayoutProfile,
    theme: &'a ThemePolicy,
    card: &'a RenderGroupCard,
}

struct EvidenceEntry<'a> {
    label: &'a str,
    value: &'a str,
    raw: bool,
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
        let location_host = self.layout.location_host(self.card);
        lines.push(
            self.layout
                .primary_line(self.theme, self.card, location_host),
        );
        if matches!(location_host, LocationHost::Dedicated)
            && let Some(location) = self.card.canonical_location.as_ref()
        {
            lines.push(format!("--> {}", self.theme.inline(location)));
        }
        if let Some(confidence_notice) = self.card.confidence_notice.as_ref() {
            lines.push(confidence_notice.clone());
        }
        if self.card.semantic_card.presentation.subject_first_header {
            self.render_subject_first_evidence(lines, location_host);
        } else {
            self.render_legacy_evidence(lines, location_host);
        }
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

    fn render_subject_first_evidence(&self, lines: &mut Vec<String>, location_host: LocationHost) {
        let evidence = self.evidence_entries();
        if evidence.is_empty() {
            return;
        }
        let label_width = self.configured_label_width(&evidence);
        let mut appended_location = false;
        for entry in evidence {
            let value = if entry.raw {
                self.theme.raw(entry.value)
            } else {
                self.theme.inline(entry.value)
            };
            let mut line = format!(
                "{} {}",
                self.layout.style_evidence_label(entry.label, label_width),
                value
            );
            self.append_inline_location_once(&mut line, location_host, &mut appended_location);
            lines.push(line);
        }
    }

    fn render_legacy_evidence(&self, lines: &mut Vec<String>, location_host: LocationHost) {
        let mut appended_location = false;
        if let Some(first_action) = self
            .card
            .semantic_card
            .slot_text(SemanticSlotId::FirstAction)
            .or(self.card.first_action.as_deref())
        {
            let mut line = format!(
                "{} {}",
                self.layout
                    .style_evidence_label("help", "help".chars().count()),
                self.theme.inline(first_action)
            );
            self.append_inline_location_once(&mut line, location_host, &mut appended_location);
            lines.push(line);
        }
        self.render_legacy_semantic_evidence(lines, location_host, &mut appended_location);
        if let Some(why_text) = self.card.semantic_card.slot_text(SemanticSlotId::Raw) {
            let why_label = self
                .card
                .semantic_card
                .slot_label(SemanticSlotId::Raw)
                .unwrap_or("why");
            let mut line = format!(
                "{} {}",
                self.layout
                    .style_evidence_label(why_label, why_label.chars().count()),
                self.theme.raw(why_text)
            );
            self.append_inline_location_once(&mut line, location_host, &mut appended_location);
            lines.push(line);
        } else {
            let mut line = format!(
                "{} {}",
                self.layout
                    .style_evidence_label("why", "why".chars().count()),
                self.theme.raw(&self.card.raw_message)
            );
            self.append_inline_location_once(&mut line, location_host, &mut appended_location);
            lines.push(line);
        }
    }

    fn render_legacy_semantic_evidence(
        &self,
        lines: &mut Vec<String>,
        location_host: LocationHost,
        appended_location: &mut bool,
    ) {
        let mut rendered_slots = rendered_semantic_slots(self.card);
        if rendered_slots.is_empty() {
            return;
        }
        let label_width = rendered_slots
            .iter()
            .filter_map(|slot| slot.label.as_deref())
            .map(str::len)
            .max()
            .unwrap_or(0);
        for slot in rendered_slots.drain(..) {
            let label = slot
                .label
                .as_deref()
                .unwrap_or_else(|| slot.slot.stable_id());
            let mut line = format!(
                "{} {}",
                self.layout.style_evidence_label(label, label_width),
                self.theme.inline(&slot.value)
            );
            self.append_inline_location_once(&mut line, location_host, appended_location);
            lines.push(line);
        }
    }

    fn evidence_entries(&self) -> Vec<EvidenceEntry<'a>> {
        let mut entries = Vec::new();
        if let Some(first_action) = self
            .card
            .semantic_card
            .slot_text(SemanticSlotId::FirstAction)
            .or(self.card.first_action.as_deref())
        {
            let label = self
                .card
                .semantic_card
                .slot_label(SemanticSlotId::FirstAction)
                .unwrap_or("help");
            entries.push(EvidenceEntry {
                label,
                value: first_action,
                raw: false,
            });
        }
        for slot in rendered_semantic_slots(self.card) {
            let label = slot
                .label
                .as_deref()
                .unwrap_or_else(|| slot.slot.stable_id());
            entries.push(EvidenceEntry {
                label,
                value: &slot.value,
                raw: false,
            });
        }
        if let Some(why_text) = self.card.semantic_card.slot_text(SemanticSlotId::Raw) {
            let why_label = self
                .card
                .semantic_card
                .slot_label(SemanticSlotId::Raw)
                .unwrap_or("why");
            entries.push(EvidenceEntry {
                label: why_label,
                value: why_text,
                raw: true,
            });
        } else if !self.card.semantic_card.presentation.subject_first_header {
            entries.push(EvidenceEntry {
                label: "why",
                value: &self.card.raw_message,
                raw: true,
            });
        }
        entries
    }

    fn configured_label_width(&self, evidence: &[EvidenceEntry<'_>]) -> usize {
        self.card
            .semantic_card
            .presentation
            .evidence_label_width
            .max(
                evidence
                    .iter()
                    .map(|entry| entry.label.chars().count())
                    .max()
                    .unwrap_or(0),
            )
            .max(MIN_EVIDENCE_LABEL_WIDTH)
    }

    fn append_inline_location_once(
        &self,
        line: &mut String,
        location_host: LocationHost,
        appended_location: &mut bool,
    ) {
        if matches!(location_host, LocationHost::Evidence)
            && !*appended_location
            && let Some(suffix) = self.layout.inline_location_suffix(self.theme, self.card)
        {
            line.push_str(&suffix);
            *appended_location = true;
        }
    }
}

fn rendered_semantic_slots(card: &RenderGroupCard) -> Vec<&RenderSemanticSlot> {
    card.semantic_card
        .slots
        .iter()
        .filter(|slot| slot.slot != SemanticSlotId::FirstAction && slot.slot != SemanticSlotId::Raw)
        .collect()
}
