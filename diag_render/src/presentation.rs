use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

const GENERIC_TEMPLATE_ID: &str = "generic_block";
const LEGACY_DEFAULT_TEMPLATE_ID: &str = "legacy_primary_block";
const SUBJECT_BLOCKS_DEFAULT_TEMPLATE_ID: &str = "generic_block";

/// Stable semantic slot identifiers used by Presentation V2.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SemanticSlotId {
    FirstAction,
    Want,
    Got,
    Via,
    Name,
    Use,
    Need,
    From,
    Near,
    Now,
    Prev,
    Symbol,
    Archive,
    WhyRaw,
}

impl SemanticSlotId {
    /// Returns the stable `snake_case` ID used across templates and adapters.
    pub fn stable_id(self) -> &'static str {
        match self {
            Self::FirstAction => "first_action",
            Self::Want => "want",
            Self::Got => "got",
            Self::Via => "via",
            Self::Name => "name",
            Self::Use => "use",
            Self::Need => "need",
            Self::From => "from",
            Self::Near => "near",
            Self::Now => "now",
            Self::Prev => "prev",
            Self::Symbol => "symbol",
            Self::Archive => "archive",
            Self::WhyRaw => "why_raw",
        }
    }
}

/// Session-level presentation mode for visible diagnostic groups.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionMode {
    AllVisibleBlocks,
    LeadPlusSummary,
    CappedBlocks,
}

/// Preferred host location for rendered locations.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LocationPlacement {
    InlineSuffix,
    HeaderSuffix,
    EvidenceSuffix,
    ExcerptHeader,
    DedicatedLine,
    None,
}

/// Resolved location placement policy for a preset.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResolvedLocationPolicy {
    pub default_placement: LocationPlacement,
    #[serde(default)]
    pub fallback_order: Vec<LocationPlacement>,
}

/// One logical line inside a resolved presentation template.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResolvedTemplateLine {
    pub slot: SemanticSlotId,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub suffix_slot: Option<String>,
    #[serde(default)]
    pub optional: bool,
}

/// A checked and normalized template definition.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResolvedTemplate {
    pub id: String,
    #[serde(default)]
    pub core: Vec<ResolvedTemplateLine>,
}

/// Family-to-presentation mapping after config resolution.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResolvedFamilyPresentation {
    pub matcher: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display_family: Option<String>,
    pub template_id: String,
}

/// The effective presentation decision for a single rendered card.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResolvedCardPresentation {
    pub template_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display_family: Option<String>,
    #[serde(default)]
    pub subject_first_header: bool,
    #[serde(default = "default_card_location_policy")]
    pub location_policy: ResolvedLocationPolicy,
    #[serde(default)]
    pub fell_back_to_generic_template: bool,
}

/// Resolved, render-ready presentation policy accepted by the render layer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResolvedPresentationPolicy {
    pub preset_id: String,
    pub session_mode: SessionMode,
    pub location_policy: ResolvedLocationPolicy,
    #[serde(default)]
    pub label_catalog: BTreeMap<String, String>,
    #[serde(default)]
    pub templates: BTreeMap<String, ResolvedTemplate>,
    #[serde(default)]
    pub family_mappings: Vec<ResolvedFamilyPresentation>,
    #[serde(default = "default_template_id")]
    pub default_template_id: String,
    #[serde(default = "generic_template_id")]
    pub generic_template_id: String,
    #[serde(default)]
    pub warnings: Vec<String>,
    #[serde(default)]
    pub fell_back_to_default: bool,
}

impl Default for ResolvedPresentationPolicy {
    fn default() -> Self {
        Self::legacy_v1()
    }
}

impl ResolvedPresentationPolicy {
    /// Built-in legacy-compatible preset used by current render defaults.
    pub fn legacy_v1() -> Self {
        let templates = BTreeMap::from([
            (
                GENERIC_TEMPLATE_ID.to_string(),
                ResolvedTemplate {
                    id: GENERIC_TEMPLATE_ID.to_string(),
                    core: vec![ResolvedTemplateLine {
                        slot: SemanticSlotId::WhyRaw,
                        label: Some("why".to_string()),
                        suffix_slot: None,
                        optional: false,
                    }],
                },
            ),
            (
                LEGACY_DEFAULT_TEMPLATE_ID.to_string(),
                ResolvedTemplate {
                    id: LEGACY_DEFAULT_TEMPLATE_ID.to_string(),
                    core: vec![
                        ResolvedTemplateLine {
                            slot: SemanticSlotId::FirstAction,
                            label: Some("help".to_string()),
                            suffix_slot: None,
                            optional: true,
                        },
                        ResolvedTemplateLine {
                            slot: SemanticSlotId::WhyRaw,
                            label: Some("why".to_string()),
                            suffix_slot: None,
                            optional: false,
                        },
                    ],
                },
            ),
            (
                "legacy_linker_block".to_string(),
                ResolvedTemplate {
                    id: "legacy_linker_block".to_string(),
                    core: vec![ResolvedTemplateLine {
                        slot: SemanticSlotId::WhyRaw,
                        label: Some("why".to_string()),
                        suffix_slot: None,
                        optional: false,
                    }],
                },
            ),
        ]);

        Self {
            preset_id: "legacy_v1".to_string(),
            session_mode: SessionMode::LeadPlusSummary,
            location_policy: ResolvedLocationPolicy {
                default_placement: LocationPlacement::DedicatedLine,
                fallback_order: vec![
                    LocationPlacement::DedicatedLine,
                    LocationPlacement::ExcerptHeader,
                    LocationPlacement::None,
                ],
            },
            label_catalog: builtin_label_catalog(),
            templates,
            family_mappings: vec![ResolvedFamilyPresentation {
                matcher: "prefix:linker.".to_string(),
                display_family: Some("linker".to_string()),
                template_id: "legacy_linker_block".to_string(),
            }],
            default_template_id: LEGACY_DEFAULT_TEMPLATE_ID.to_string(),
            generic_template_id: GENERIC_TEMPLATE_ID.to_string(),
            warnings: Vec::new(),
            fell_back_to_default: false,
        }
    }

    /// Built-in subject-first preset skeleton. The generic path remains fail-open.
    pub fn subject_blocks_v1() -> Self {
        let templates = BTreeMap::from([
            (
                GENERIC_TEMPLATE_ID.to_string(),
                ResolvedTemplate {
                    id: GENERIC_TEMPLATE_ID.to_string(),
                    core: vec![
                        ResolvedTemplateLine {
                            slot: SemanticSlotId::FirstAction,
                            label: Some("help".to_string()),
                            suffix_slot: None,
                            optional: true,
                        },
                        ResolvedTemplateLine {
                            slot: SemanticSlotId::WhyRaw,
                            label: Some("why".to_string()),
                            suffix_slot: None,
                            optional: false,
                        },
                    ],
                },
            ),
            (
                "contrast_block".to_string(),
                ResolvedTemplate {
                    id: "contrast_block".to_string(),
                    core: vec![
                        ResolvedTemplateLine {
                            slot: SemanticSlotId::FirstAction,
                            label: Some("help".to_string()),
                            suffix_slot: None,
                            optional: true,
                        },
                        ResolvedTemplateLine {
                            slot: SemanticSlotId::Want,
                            label: Some("want".to_string()),
                            suffix_slot: None,
                            optional: false,
                        },
                        ResolvedTemplateLine {
                            slot: SemanticSlotId::Got,
                            label: Some("got".to_string()),
                            suffix_slot: None,
                            optional: false,
                        },
                        ResolvedTemplateLine {
                            slot: SemanticSlotId::Via,
                            label: Some("via".to_string()),
                            suffix_slot: Some("omitted_notes_suffix".to_string()),
                            optional: true,
                        },
                    ],
                },
            ),
            (
                "parser_block".to_string(),
                ResolvedTemplate {
                    id: "parser_block".to_string(),
                    core: vec![
                        ResolvedTemplateLine {
                            slot: SemanticSlotId::FirstAction,
                            label: Some("help".to_string()),
                            suffix_slot: None,
                            optional: true,
                        },
                        ResolvedTemplateLine {
                            slot: SemanticSlotId::Want,
                            label: Some("want".to_string()),
                            suffix_slot: None,
                            optional: true,
                        },
                        ResolvedTemplateLine {
                            slot: SemanticSlotId::Near,
                            label: Some("near".to_string()),
                            suffix_slot: None,
                            optional: true,
                        },
                    ],
                },
            ),
            (
                "linker_block".to_string(),
                ResolvedTemplate {
                    id: "linker_block".to_string(),
                    core: vec![
                        ResolvedTemplateLine {
                            slot: SemanticSlotId::FirstAction,
                            label: Some("help".to_string()),
                            suffix_slot: None,
                            optional: true,
                        },
                        ResolvedTemplateLine {
                            slot: SemanticSlotId::Symbol,
                            label: Some("symbol".to_string()),
                            suffix_slot: None,
                            optional: false,
                        },
                        ResolvedTemplateLine {
                            slot: SemanticSlotId::From,
                            label: Some("from".to_string()),
                            suffix_slot: None,
                            optional: true,
                        },
                        ResolvedTemplateLine {
                            slot: SemanticSlotId::Archive,
                            label: Some("archive".to_string()),
                            suffix_slot: None,
                            optional: true,
                        },
                    ],
                },
            ),
            (
                "lookup_block".to_string(),
                ResolvedTemplate {
                    id: "lookup_block".to_string(),
                    core: vec![
                        ResolvedTemplateLine {
                            slot: SemanticSlotId::FirstAction,
                            label: Some("help".to_string()),
                            suffix_slot: None,
                            optional: true,
                        },
                        ResolvedTemplateLine {
                            slot: SemanticSlotId::Name,
                            label: Some("name".to_string()),
                            suffix_slot: None,
                            optional: true,
                        },
                        ResolvedTemplateLine {
                            slot: SemanticSlotId::Use,
                            label: Some("use".to_string()),
                            suffix_slot: None,
                            optional: true,
                        },
                        ResolvedTemplateLine {
                            slot: SemanticSlotId::Need,
                            label: Some("need".to_string()),
                            suffix_slot: None,
                            optional: true,
                        },
                        ResolvedTemplateLine {
                            slot: SemanticSlotId::From,
                            label: Some("from".to_string()),
                            suffix_slot: None,
                            optional: true,
                        },
                        ResolvedTemplateLine {
                            slot: SemanticSlotId::Near,
                            label: Some("near".to_string()),
                            suffix_slot: None,
                            optional: true,
                        },
                    ],
                },
            ),
            (
                "missing_header_block".to_string(),
                ResolvedTemplate {
                    id: "missing_header_block".to_string(),
                    core: vec![
                        ResolvedTemplateLine {
                            slot: SemanticSlotId::FirstAction,
                            label: Some("help".to_string()),
                            suffix_slot: None,
                            optional: true,
                        },
                        ResolvedTemplateLine {
                            slot: SemanticSlotId::Need,
                            label: Some("need".to_string()),
                            suffix_slot: None,
                            optional: false,
                        },
                        ResolvedTemplateLine {
                            slot: SemanticSlotId::From,
                            label: Some("from".to_string()),
                            suffix_slot: None,
                            optional: true,
                        },
                    ],
                },
            ),
            (
                "conflict_block".to_string(),
                ResolvedTemplate {
                    id: "conflict_block".to_string(),
                    core: vec![
                        ResolvedTemplateLine {
                            slot: SemanticSlotId::FirstAction,
                            label: Some("help".to_string()),
                            suffix_slot: None,
                            optional: true,
                        },
                        ResolvedTemplateLine {
                            slot: SemanticSlotId::Now,
                            label: Some("now".to_string()),
                            suffix_slot: None,
                            optional: true,
                        },
                        ResolvedTemplateLine {
                            slot: SemanticSlotId::Prev,
                            label: Some("prev".to_string()),
                            suffix_slot: None,
                            optional: true,
                        },
                    ],
                },
            ),
            (
                "context_block".to_string(),
                ResolvedTemplate {
                    id: "context_block".to_string(),
                    core: vec![
                        ResolvedTemplateLine {
                            slot: SemanticSlotId::FirstAction,
                            label: Some("help".to_string()),
                            suffix_slot: None,
                            optional: true,
                        },
                        ResolvedTemplateLine {
                            slot: SemanticSlotId::From,
                            label: Some("from".to_string()),
                            suffix_slot: None,
                            optional: true,
                        },
                        ResolvedTemplateLine {
                            slot: SemanticSlotId::Via,
                            label: Some("via".to_string()),
                            suffix_slot: None,
                            optional: true,
                        },
                    ],
                },
            ),
        ]);

        Self {
            preset_id: "subject_blocks_v1".to_string(),
            session_mode: SessionMode::AllVisibleBlocks,
            location_policy: ResolvedLocationPolicy {
                default_placement: LocationPlacement::InlineSuffix,
                fallback_order: vec![
                    LocationPlacement::HeaderSuffix,
                    LocationPlacement::EvidenceSuffix,
                    LocationPlacement::ExcerptHeader,
                    LocationPlacement::None,
                ],
            },
            label_catalog: builtin_label_catalog(),
            templates,
            family_mappings: vec![
                ResolvedFamilyPresentation {
                    matcher: "type_overload".to_string(),
                    display_family: Some("type_mismatch".to_string()),
                    template_id: "contrast_block".to_string(),
                },
                ResolvedFamilyPresentation {
                    matcher: "concepts_constraints".to_string(),
                    display_family: Some("type_mismatch".to_string()),
                    template_id: "contrast_block".to_string(),
                },
                ResolvedFamilyPresentation {
                    matcher: "format_string".to_string(),
                    display_family: Some("type_mismatch".to_string()),
                    template_id: "contrast_block".to_string(),
                },
                ResolvedFamilyPresentation {
                    matcher: "conversion_narrowing".to_string(),
                    display_family: Some("type_mismatch".to_string()),
                    template_id: "contrast_block".to_string(),
                },
                ResolvedFamilyPresentation {
                    matcher: "const_qualifier".to_string(),
                    display_family: Some("type_mismatch".to_string()),
                    template_id: "contrast_block".to_string(),
                },
                ResolvedFamilyPresentation {
                    matcher: "prefix:linker.".to_string(),
                    display_family: Some("linker".to_string()),
                    template_id: "linker_block".to_string(),
                },
                ResolvedFamilyPresentation {
                    matcher: "syntax".to_string(),
                    display_family: Some("syntax".to_string()),
                    template_id: "parser_block".to_string(),
                },
                ResolvedFamilyPresentation {
                    matcher: "preprocessor_directive".to_string(),
                    display_family: Some("syntax".to_string()),
                    template_id: "parser_block".to_string(),
                },
                ResolvedFamilyPresentation {
                    matcher: "attribute".to_string(),
                    display_family: Some("syntax".to_string()),
                    template_id: "parser_block".to_string(),
                },
                ResolvedFamilyPresentation {
                    matcher: "storage_class".to_string(),
                    display_family: Some("syntax".to_string()),
                    template_id: "parser_block".to_string(),
                },
                ResolvedFamilyPresentation {
                    matcher: "module_import".to_string(),
                    display_family: Some("syntax".to_string()),
                    template_id: "parser_block".to_string(),
                },
                ResolvedFamilyPresentation {
                    matcher: "coroutine".to_string(),
                    display_family: Some("syntax".to_string()),
                    template_id: "parser_block".to_string(),
                },
                ResolvedFamilyPresentation {
                    matcher: "asm_inline".to_string(),
                    display_family: Some("syntax".to_string()),
                    template_id: "parser_block".to_string(),
                },
                ResolvedFamilyPresentation {
                    matcher: "openmp".to_string(),
                    display_family: Some("syntax".to_string()),
                    template_id: "parser_block".to_string(),
                },
                ResolvedFamilyPresentation {
                    matcher: "scope_declaration".to_string(),
                    display_family: Some("missing_name".to_string()),
                    template_id: "lookup_block".to_string(),
                },
                ResolvedFamilyPresentation {
                    matcher: "pointer_reference".to_string(),
                    display_family: Some("incomplete_type".to_string()),
                    template_id: "lookup_block".to_string(),
                },
                ResolvedFamilyPresentation {
                    matcher: "deleted_function".to_string(),
                    display_family: Some("unavailable_api".to_string()),
                    template_id: "lookup_block".to_string(),
                },
                ResolvedFamilyPresentation {
                    matcher: "access_control".to_string(),
                    display_family: Some("unavailable_api".to_string()),
                    template_id: "lookup_block".to_string(),
                },
                ResolvedFamilyPresentation {
                    matcher: "redefinition".to_string(),
                    display_family: Some("redefinition".to_string()),
                    template_id: "conflict_block".to_string(),
                },
                ResolvedFamilyPresentation {
                    matcher: "odr_inline_linkage".to_string(),
                    display_family: Some("redefinition".to_string()),
                    template_id: "conflict_block".to_string(),
                },
                ResolvedFamilyPresentation {
                    matcher: "macro_include".to_string(),
                    display_family: Some("macro_include".to_string()),
                    template_id: "context_block".to_string(),
                },
            ],
            default_template_id: SUBJECT_BLOCKS_DEFAULT_TEMPLATE_ID.to_string(),
            generic_template_id: GENERIC_TEMPLATE_ID.to_string(),
            warnings: Vec::new(),
            fell_back_to_default: false,
        }
    }

    /// Looks up a resolved template by ID.
    pub fn template(&self, template_id: &str) -> Option<&ResolvedTemplate> {
        self.templates.get(template_id)
    }

    /// Returns a resolved label by stable ID.
    pub fn label(&self, label_id: &str) -> Option<&str> {
        self.label_catalog.get(label_id).map(String::as_str)
    }

    /// Returns the default label used for a given semantic slot.
    pub fn slot_label(&self, slot: SemanticSlotId) -> Option<&str> {
        self.label(slot.stable_id())
    }

    /// Resolves display family and template selection for a card.
    pub fn resolve_card_presentation(
        &self,
        internal_family: Option<&str>,
    ) -> ResolvedCardPresentation {
        let mapping = internal_family.and_then(|family| {
            self.family_mappings
                .iter()
                .find(|candidate| matcher_matches(&candidate.matcher, family))
        });

        let requested_template_id = mapping
            .map(|candidate| candidate.template_id.as_str())
            .unwrap_or(self.default_template_id.as_str());
        let fell_back_to_generic_template = self.template(requested_template_id).is_none();

        ResolvedCardPresentation {
            template_id: if fell_back_to_generic_template {
                self.generic_template_id.clone()
            } else {
                requested_template_id.to_string()
            },
            display_family: mapping.and_then(|candidate| candidate.display_family.clone()),
            subject_first_header: self.preset_id == "subject_blocks_v1",
            location_policy: self.location_policy.clone(),
            fell_back_to_generic_template,
        }
    }
}

/// One resolved semantic slot instance attached to a rendered card.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct RenderSemanticSlot {
    pub(crate) slot: SemanticSlotId,
    pub(crate) value: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) label: Option<String>,
}

/// Internal semantic card representation used before legacy adaptation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub(crate) struct RenderSemanticCard {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) internal_family: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) display_family: Option<String>,
    pub(crate) subject: String,
    pub(crate) presentation: ResolvedCardPresentation,
    #[serde(default)]
    pub(crate) slots: Vec<RenderSemanticSlot>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) canonical_location: Option<String>,
    pub(crate) raw_message: String,
}

impl Default for ResolvedCardPresentation {
    fn default() -> Self {
        Self {
            template_id: GENERIC_TEMPLATE_ID.to_string(),
            display_family: None,
            subject_first_header: false,
            location_policy: default_card_location_policy(),
            fell_back_to_generic_template: false,
        }
    }
}

impl RenderSemanticCard {
    /// Returns the first slot value for the requested semantic slot.
    pub(crate) fn slot_text(&self, slot: SemanticSlotId) -> Option<&str> {
        self.slots
            .iter()
            .find(|candidate| candidate.slot == slot)
            .map(|candidate| candidate.value.as_str())
    }

    /// Returns the resolved label for the requested semantic slot.
    pub(crate) fn slot_label(&self, slot: SemanticSlotId) -> Option<&str> {
        self.slots
            .iter()
            .find(|candidate| candidate.slot == slot)
            .and_then(|candidate| candidate.label.as_deref())
    }
}

fn default_template_id() -> String {
    LEGACY_DEFAULT_TEMPLATE_ID.to_string()
}

fn generic_template_id() -> String {
    GENERIC_TEMPLATE_ID.to_string()
}

fn default_card_location_policy() -> ResolvedLocationPolicy {
    ResolvedLocationPolicy {
        default_placement: LocationPlacement::DedicatedLine,
        fallback_order: vec![
            LocationPlacement::DedicatedLine,
            LocationPlacement::ExcerptHeader,
            LocationPlacement::None,
        ],
    }
}

fn matcher_matches(matcher: &str, family: &str) -> bool {
    matcher
        .strip_prefix("prefix:")
        .map_or_else(|| matcher == family, |prefix| family.starts_with(prefix))
}

fn builtin_label_catalog() -> BTreeMap<String, String> {
    BTreeMap::from([
        ("help".to_string(), "help".to_string()),
        ("want".to_string(), "want".to_string()),
        ("got".to_string(), "got".to_string()),
        ("via".to_string(), "via".to_string()),
        ("name".to_string(), "name".to_string()),
        ("use".to_string(), "use".to_string()),
        ("need".to_string(), "need".to_string()),
        ("from".to_string(), "from".to_string()),
        ("near".to_string(), "near".to_string()),
        ("now".to_string(), "now".to_string()),
        ("prev".to_string(), "prev".to_string()),
        ("symbol".to_string(), "symbol".to_string()),
        ("archive".to_string(), "archive".to_string()),
        ("why_raw".to_string(), "why".to_string()),
        ("raw".to_string(), "raw".to_string()),
        ("omitted".to_string(), "omitted".to_string()),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_policy_uses_legacy_default_template() {
        let policy = ResolvedPresentationPolicy::legacy_v1();

        let resolved = policy.resolve_card_presentation(Some("syntax"));

        assert_eq!(resolved.template_id, "legacy_primary_block");
        assert!(!resolved.fell_back_to_generic_template);
    }

    #[test]
    fn family_mapping_matches_prefix_entries() {
        let policy = ResolvedPresentationPolicy::legacy_v1();

        let resolved = policy.resolve_card_presentation(Some("linker.multiple_definition"));

        assert_eq!(resolved.template_id, "legacy_linker_block");
        assert_eq!(resolved.display_family.as_deref(), Some("linker"));
    }

    #[test]
    fn subject_blocks_maps_contrast_and_linker_families() {
        let policy = ResolvedPresentationPolicy::subject_blocks_v1();

        let contrast = policy.resolve_card_presentation(Some("const_qualifier"));
        let linker = policy.resolve_card_presentation(Some("linker.undefined_reference"));
        let lookup = policy.resolve_card_presentation(Some("pointer_reference"));
        let conflict = policy.resolve_card_presentation(Some("redefinition"));
        let context = policy.resolve_card_presentation(Some("macro_include"));

        assert_eq!(contrast.template_id, "contrast_block");
        assert_eq!(contrast.display_family.as_deref(), Some("type_mismatch"));
        assert!(contrast.subject_first_header);
        assert_eq!(linker.template_id, "linker_block");
        assert_eq!(linker.display_family.as_deref(), Some("linker"));
        assert!(linker.subject_first_header);
        assert_eq!(lookup.template_id, "lookup_block");
        assert_eq!(lookup.display_family.as_deref(), Some("incomplete_type"));
        assert_eq!(conflict.template_id, "conflict_block");
        assert_eq!(conflict.display_family.as_deref(), Some("redefinition"));
        assert_eq!(context.template_id, "context_block");
        assert_eq!(context.display_family.as_deref(), Some("macro_include"));
    }

    #[test]
    fn missing_template_falls_back_to_generic_block() {
        let mut policy = ResolvedPresentationPolicy::subject_blocks_v1();
        policy.family_mappings = vec![ResolvedFamilyPresentation {
            matcher: "syntax".to_string(),
            display_family: Some("syntax".to_string()),
            template_id: "missing_block".to_string(),
        }];

        let resolved = policy.resolve_card_presentation(Some("syntax"));

        assert_eq!(resolved.template_id, "generic_block");
        assert!(resolved.fell_back_to_generic_template);
    }

    #[test]
    fn builtin_labels_cover_core_slot_ids() {
        let policy = ResolvedPresentationPolicy::subject_blocks_v1();

        assert_eq!(policy.slot_label(SemanticSlotId::WhyRaw), Some("why"));
        assert_eq!(policy.label("help"), Some("help"));
        assert_eq!(policy.label("raw"), Some("raw"));
    }
}
