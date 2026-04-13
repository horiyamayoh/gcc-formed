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
    Raw,
}

impl SemanticSlotId {
    #[allow(non_upper_case_globals)]
    pub const WhyRaw: Self = Self::Raw;

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
            Self::Raw => "raw",
        }
    }
}

/// Stable semantic extraction/routing shapes used by the view-model layer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SemanticShape {
    Contrast,
    Parser,
    Lookup,
    MissingHeader,
    Conflict,
    Context,
    Linker,
    Generic,
}

/// Ordered fallback shape candidates evaluated before the primary shape.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResolvedShapeFallback {
    pub shape: SemanticShape,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display_family: Option<String>,
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
    #[serde(default = "default_inline_suffix_format")]
    pub inline_suffix_format: String,
    #[serde(default = "default_location_width_soft_limit")]
    pub width_soft_limit: usize,
}

/// Resolved header policy carried from the presentation config.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResolvedHeaderPolicy {
    pub subject_first: bool,
    pub interactive_format: String,
    pub ci_path_first_format: String,
    pub unknown_family: String,
}

impl Default for ResolvedHeaderPolicy {
    fn default() -> Self {
        Self {
            subject_first: false,
            interactive_format: "{severity}: [{family}] {subject}".to_string(),
            ci_path_first_format: "{location}: {severity}: [{family}] {subject}".to_string(),
            unknown_family: "generic".to_string(),
        }
    }
}

/// Resolved evidence label alignment strategy.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LabelWidthMode {
    #[default]
    TemplateMax,
    Fixed,
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub semantic_shape: Option<SemanticShape>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub shape_fallbacks: Vec<ResolvedShapeFallback>,
}

/// The effective presentation decision for a single rendered card.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResolvedCardPresentation {
    pub template_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display_family: Option<String>,
    pub semantic_shape: SemanticShape,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub shape_fallbacks: Vec<ResolvedShapeFallback>,
    #[serde(default)]
    pub header: ResolvedHeaderPolicy,
    #[serde(default)]
    pub subject_first_header: bool,
    #[serde(default = "default_card_location_policy")]
    pub location_policy: ResolvedLocationPolicy,
    #[serde(default)]
    pub evidence_label_width: usize,
    #[serde(default)]
    pub fell_back_to_generic_template: bool,
}

/// Resolved, render-ready presentation policy accepted by the render layer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResolvedPresentationPolicy {
    pub preset_id: String,
    pub session_mode: SessionMode,
    #[serde(default)]
    pub header: ResolvedHeaderPolicy,
    pub location_policy: ResolvedLocationPolicy,
    #[serde(default)]
    pub label_width_mode: LabelWidthMode,
    #[serde(default)]
    pub fixed_label_width: Option<usize>,
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
        Self::subject_blocks_v2()
    }
}

impl ResolvedPresentationPolicy {
    /// Built-in legacy-compatible preset kept as an explicit rollback option.
    pub fn legacy_v1() -> Self {
        let templates = BTreeMap::from([
            (
                GENERIC_TEMPLATE_ID.to_string(),
                ResolvedTemplate {
                    id: GENERIC_TEMPLATE_ID.to_string(),
                    core: vec![ResolvedTemplateLine {
                        slot: SemanticSlotId::Raw,
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
                            slot: SemanticSlotId::Raw,
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
                        slot: SemanticSlotId::Raw,
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
            header: ResolvedHeaderPolicy {
                subject_first: false,
                interactive_format: "{severity}: [{family}] {subject}".to_string(),
                ci_path_first_format: "{location}: {severity}: [{family}] {subject}".to_string(),
                unknown_family: "generic".to_string(),
            },
            location_policy: ResolvedLocationPolicy {
                default_placement: LocationPlacement::DedicatedLine,
                fallback_order: vec![
                    LocationPlacement::DedicatedLine,
                    LocationPlacement::ExcerptHeader,
                    LocationPlacement::None,
                ],
                inline_suffix_format: default_inline_suffix_format(),
                width_soft_limit: default_location_width_soft_limit(),
            },
            label_width_mode: LabelWidthMode::TemplateMax,
            fixed_label_width: None,
            label_catalog: builtin_label_catalog(),
            templates,
            family_mappings: vec![ResolvedFamilyPresentation {
                matcher: "prefix:linker.".to_string(),
                display_family: Some("linker".to_string()),
                template_id: "legacy_linker_block".to_string(),
                semantic_shape: None,
                shape_fallbacks: Vec::new(),
            }],
            default_template_id: LEGACY_DEFAULT_TEMPLATE_ID.to_string(),
            generic_template_id: GENERIC_TEMPLATE_ID.to_string(),
            warnings: Vec::new(),
            fell_back_to_default: false,
        }
    }

    /// Built-in subject-first preset used by the current no-config default.
    pub fn subject_blocks_v2() -> Self {
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
                            slot: SemanticSlotId::Raw,
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
            preset_id: "subject_blocks_v2".to_string(),
            session_mode: SessionMode::AllVisibleBlocks,
            header: ResolvedHeaderPolicy {
                subject_first: true,
                interactive_format: "{severity}: [{family}] {subject}".to_string(),
                ci_path_first_format: "{location}: {severity}: [{family}] {subject}".to_string(),
                unknown_family: "generic".to_string(),
            },
            location_policy: ResolvedLocationPolicy {
                default_placement: LocationPlacement::InlineSuffix,
                fallback_order: vec![
                    LocationPlacement::HeaderSuffix,
                    LocationPlacement::EvidenceSuffix,
                    LocationPlacement::ExcerptHeader,
                    LocationPlacement::None,
                ],
                inline_suffix_format: default_inline_suffix_format(),
                width_soft_limit: default_location_width_soft_limit(),
            },
            label_width_mode: LabelWidthMode::TemplateMax,
            fixed_label_width: None,
            label_catalog: subject_blocks_label_catalog(),
            templates,
            family_mappings: vec![
                ResolvedFamilyPresentation {
                    matcher: "type_overload".to_string(),
                    display_family: Some("type_mismatch".to_string()),
                    template_id: "contrast_block".to_string(),
                    semantic_shape: Some(SemanticShape::Contrast),
                    shape_fallbacks: Vec::new(),
                },
                ResolvedFamilyPresentation {
                    matcher: "concepts_constraints".to_string(),
                    display_family: Some("type_mismatch".to_string()),
                    template_id: "contrast_block".to_string(),
                    semantic_shape: Some(SemanticShape::Contrast),
                    shape_fallbacks: Vec::new(),
                },
                ResolvedFamilyPresentation {
                    matcher: "format_string".to_string(),
                    display_family: Some("type_mismatch".to_string()),
                    template_id: "contrast_block".to_string(),
                    semantic_shape: Some(SemanticShape::Contrast),
                    shape_fallbacks: Vec::new(),
                },
                ResolvedFamilyPresentation {
                    matcher: "conversion_narrowing".to_string(),
                    display_family: Some("type_mismatch".to_string()),
                    template_id: "contrast_block".to_string(),
                    semantic_shape: Some(SemanticShape::Contrast),
                    shape_fallbacks: Vec::new(),
                },
                ResolvedFamilyPresentation {
                    matcher: "const_qualifier".to_string(),
                    display_family: Some("type_mismatch".to_string()),
                    template_id: "contrast_block".to_string(),
                    semantic_shape: Some(SemanticShape::Contrast),
                    shape_fallbacks: Vec::new(),
                },
                ResolvedFamilyPresentation {
                    matcher: "prefix:linker.".to_string(),
                    display_family: Some("linker".to_string()),
                    template_id: "linker_block".to_string(),
                    semantic_shape: Some(SemanticShape::Linker),
                    shape_fallbacks: Vec::new(),
                },
                ResolvedFamilyPresentation {
                    matcher: "syntax".to_string(),
                    display_family: Some("syntax".to_string()),
                    template_id: "parser_block".to_string(),
                    semantic_shape: Some(SemanticShape::Parser),
                    shape_fallbacks: Vec::new(),
                },
                ResolvedFamilyPresentation {
                    matcher: "preprocessor_directive".to_string(),
                    display_family: Some("syntax".to_string()),
                    template_id: "parser_block".to_string(),
                    semantic_shape: Some(SemanticShape::Parser),
                    shape_fallbacks: vec![ResolvedShapeFallback {
                        shape: SemanticShape::MissingHeader,
                        display_family: Some("missing_header".to_string()),
                    }],
                },
                ResolvedFamilyPresentation {
                    matcher: "attribute".to_string(),
                    display_family: Some("syntax".to_string()),
                    template_id: "parser_block".to_string(),
                    semantic_shape: Some(SemanticShape::Parser),
                    shape_fallbacks: Vec::new(),
                },
                ResolvedFamilyPresentation {
                    matcher: "storage_class".to_string(),
                    display_family: Some("syntax".to_string()),
                    template_id: "parser_block".to_string(),
                    semantic_shape: Some(SemanticShape::Parser),
                    shape_fallbacks: Vec::new(),
                },
                ResolvedFamilyPresentation {
                    matcher: "module_import".to_string(),
                    display_family: Some("syntax".to_string()),
                    template_id: "parser_block".to_string(),
                    semantic_shape: Some(SemanticShape::Parser),
                    shape_fallbacks: Vec::new(),
                },
                ResolvedFamilyPresentation {
                    matcher: "coroutine".to_string(),
                    display_family: Some("syntax".to_string()),
                    template_id: "parser_block".to_string(),
                    semantic_shape: Some(SemanticShape::Parser),
                    shape_fallbacks: Vec::new(),
                },
                ResolvedFamilyPresentation {
                    matcher: "asm_inline".to_string(),
                    display_family: Some("syntax".to_string()),
                    template_id: "parser_block".to_string(),
                    semantic_shape: Some(SemanticShape::Parser),
                    shape_fallbacks: Vec::new(),
                },
                ResolvedFamilyPresentation {
                    matcher: "openmp".to_string(),
                    display_family: Some("syntax".to_string()),
                    template_id: "parser_block".to_string(),
                    semantic_shape: Some(SemanticShape::Parser),
                    shape_fallbacks: Vec::new(),
                },
                ResolvedFamilyPresentation {
                    matcher: "scope_declaration".to_string(),
                    display_family: Some("missing_name".to_string()),
                    template_id: "lookup_block".to_string(),
                    semantic_shape: Some(SemanticShape::Lookup),
                    shape_fallbacks: Vec::new(),
                },
                ResolvedFamilyPresentation {
                    matcher: "pointer_reference".to_string(),
                    display_family: Some("incomplete_type".to_string()),
                    template_id: "lookup_block".to_string(),
                    semantic_shape: Some(SemanticShape::Lookup),
                    shape_fallbacks: Vec::new(),
                },
                ResolvedFamilyPresentation {
                    matcher: "deleted_function".to_string(),
                    display_family: Some("unavailable_api".to_string()),
                    template_id: "lookup_block".to_string(),
                    semantic_shape: Some(SemanticShape::Lookup),
                    shape_fallbacks: Vec::new(),
                },
                ResolvedFamilyPresentation {
                    matcher: "access_control".to_string(),
                    display_family: Some("unavailable_api".to_string()),
                    template_id: "lookup_block".to_string(),
                    semantic_shape: Some(SemanticShape::Lookup),
                    shape_fallbacks: Vec::new(),
                },
                ResolvedFamilyPresentation {
                    matcher: "redefinition".to_string(),
                    display_family: Some("redefinition".to_string()),
                    template_id: "conflict_block".to_string(),
                    semantic_shape: Some(SemanticShape::Conflict),
                    shape_fallbacks: Vec::new(),
                },
                ResolvedFamilyPresentation {
                    matcher: "odr_inline_linkage".to_string(),
                    display_family: Some("redefinition".to_string()),
                    template_id: "conflict_block".to_string(),
                    semantic_shape: Some(SemanticShape::Conflict),
                    shape_fallbacks: Vec::new(),
                },
                ResolvedFamilyPresentation {
                    matcher: "macro_include".to_string(),
                    display_family: Some("macro_include".to_string()),
                    template_id: "context_block".to_string(),
                    semantic_shape: Some(SemanticShape::Context),
                    shape_fallbacks: Vec::new(),
                },
            ],
            default_template_id: SUBJECT_BLOCKS_DEFAULT_TEMPLATE_ID.to_string(),
            generic_template_id: GENERIC_TEMPLATE_ID.to_string(),
            warnings: Vec::new(),
            fell_back_to_default: false,
        }
    }

    /// Previous beta default kept as an explicit subject-first rollback preset ID.
    pub fn subject_blocks_v1() -> Self {
        let mut policy = Self::subject_blocks_v2();
        policy.preset_id = "subject_blocks_v1".to_string();
        policy
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

    fn template_line_label_text<'a>(&'a self, line: &'a ResolvedTemplateLine) -> &'a str {
        line.label
            .as_deref()
            .and_then(|label_id| self.label(label_id).or(Some(label_id)))
            .or_else(|| self.slot_label(line.slot))
            .unwrap_or_else(|| line.slot.stable_id())
    }

    fn template_label_width(&self, template_id: &str) -> usize {
        self.template(template_id)
            .map(|template| {
                template
                    .core
                    .iter()
                    .map(|line| self.template_line_label_text(line).chars().count())
                    .max()
                    .unwrap_or(0)
            })
            .unwrap_or(0)
    }

    fn resolved_evidence_label_width(&self, template_id: &str) -> usize {
        match self.label_width_mode {
            LabelWidthMode::TemplateMax => self.template_label_width(template_id),
            LabelWidthMode::Fixed => self
                .fixed_label_width
                .unwrap_or_else(|| self.template_label_width(template_id)),
        }
    }

    /// Infers the semantic shape supported by a resolved template.
    pub fn template_semantic_shape(&self, template_id: &str) -> SemanticShape {
        let Some(template) = self.template(template_id) else {
            return SemanticShape::Generic;
        };
        let semantic_slots = template
            .core
            .iter()
            .map(|line| line.slot)
            .filter(|slot| !matches!(slot, SemanticSlotId::FirstAction | SemanticSlotId::Raw))
            .collect::<Vec<_>>();
        semantic_shape_from_slots(&semantic_slots)
    }

    /// Resolves a template ID capable of rendering the requested semantic shape.
    pub fn template_id_for_shape<'a>(
        &'a self,
        current_template_id: &'a str,
        shape: SemanticShape,
    ) -> Option<&'a str> {
        if shape == SemanticShape::Generic {
            return Some(self.generic_template_id.as_str());
        }

        if self.template(current_template_id).is_some()
            && current_template_id != self.generic_template_id
            && !current_template_id.starts_with("legacy_")
            && self.template_semantic_shape(current_template_id) == shape
        {
            return Some(current_template_id);
        }

        let canonical = match shape {
            SemanticShape::Contrast => "contrast_block",
            SemanticShape::Parser => "parser_block",
            SemanticShape::Lookup => "lookup_block",
            SemanticShape::MissingHeader => "missing_header_block",
            SemanticShape::Conflict => "conflict_block",
            SemanticShape::Context => "context_block",
            SemanticShape::Linker => "linker_block",
            SemanticShape::Generic => self.generic_template_id.as_str(),
        };
        if self.template(canonical).is_some() {
            return Some(canonical);
        }

        self.templates.iter().find_map(|(template_id, _)| {
            (template_id != &self.generic_template_id
                && !template_id.starts_with("legacy_")
                && self.template_semantic_shape(template_id) == shape)
                .then_some(template_id.as_str())
        })
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
        let resolved_template_id = if fell_back_to_generic_template {
            self.generic_template_id.as_str()
        } else {
            requested_template_id
        };
        let (semantic_shape, shape_fallbacks) = if fell_back_to_generic_template
            || resolved_template_id == self.generic_template_id
            || resolved_template_id.starts_with("legacy_")
        {
            (SemanticShape::Generic, Vec::new())
        } else if let Some(mapping) = mapping {
            if let Some(semantic_shape) = mapping.semantic_shape {
                (semantic_shape, mapping.shape_fallbacks.clone())
            } else {
                semantic_shape_plan(
                    internal_family,
                    self.template_semantic_shape(resolved_template_id),
                )
            }
        } else {
            semantic_shape_plan(
                internal_family,
                self.template_semantic_shape(resolved_template_id),
            )
        };

        ResolvedCardPresentation {
            template_id: resolved_template_id.to_string(),
            display_family: mapping.and_then(|candidate| candidate.display_family.clone()),
            semantic_shape,
            shape_fallbacks,
            header: self.header.clone(),
            subject_first_header: self.header.subject_first,
            location_policy: self.location_policy.clone(),
            evidence_label_width: self.resolved_evidence_label_width(resolved_template_id),
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
            semantic_shape: SemanticShape::Generic,
            shape_fallbacks: Vec::new(),
            header: ResolvedHeaderPolicy::default(),
            subject_first_header: false,
            location_policy: default_card_location_policy(),
            evidence_label_width: 0,
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
        inline_suffix_format: default_inline_suffix_format(),
        width_soft_limit: default_location_width_soft_limit(),
    }
}

fn default_inline_suffix_format() -> String {
    " @ {location}".to_string()
}

fn default_location_width_soft_limit() -> usize {
    100
}

fn matcher_matches(matcher: &str, family: &str) -> bool {
    matcher
        .strip_prefix("prefix:")
        .map_or_else(|| matcher == family, |prefix| family.starts_with(prefix))
}

fn semantic_shape_plan(
    internal_family: Option<&str>,
    template_shape: SemanticShape,
) -> (SemanticShape, Vec<ResolvedShapeFallback>) {
    match internal_family {
        Some(
            "type_overload" | "concepts_constraints" | "format_string" | "conversion_narrowing",
        )
        | Some("const_qualifier") => (SemanticShape::Contrast, Vec::new()),
        Some("syntax" | "attribute" | "storage_class" | "module_import" | "coroutine")
        | Some("asm_inline" | "openmp") => (SemanticShape::Parser, Vec::new()),
        Some("preprocessor_directive") => (
            SemanticShape::Parser,
            vec![ResolvedShapeFallback {
                shape: SemanticShape::MissingHeader,
                display_family: Some("missing_header".to_string()),
            }],
        ),
        Some("scope_declaration" | "pointer_reference" | "deleted_function")
        | Some("access_control") => (SemanticShape::Lookup, Vec::new()),
        Some("redefinition" | "odr_inline_linkage") => (SemanticShape::Conflict, Vec::new()),
        Some("macro_include") => (SemanticShape::Context, Vec::new()),
        Some(family) if family.starts_with("linker.") => (SemanticShape::Linker, Vec::new()),
        _ => (template_shape, Vec::new()),
    }
}

fn semantic_shape_from_slots(slots: &[SemanticSlotId]) -> SemanticShape {
    let has = |needle| slots.contains(&needle);
    if slots.is_empty() {
        return SemanticShape::Generic;
    }
    if has(SemanticSlotId::Symbol) || has(SemanticSlotId::Archive) {
        return SemanticShape::Linker;
    }
    if has(SemanticSlotId::Now) || has(SemanticSlotId::Prev) {
        return SemanticShape::Conflict;
    }
    if has(SemanticSlotId::Name) || has(SemanticSlotId::Use) {
        return SemanticShape::Lookup;
    }
    if has(SemanticSlotId::Need)
        && !has(SemanticSlotId::Name)
        && !has(SemanticSlotId::Use)
        && !has(SemanticSlotId::Near)
        && !has(SemanticSlotId::Want)
        && !has(SemanticSlotId::Got)
        && !has(SemanticSlotId::Via)
    {
        return SemanticShape::MissingHeader;
    }
    if has(SemanticSlotId::From)
        && has(SemanticSlotId::Via)
        && !has(SemanticSlotId::Want)
        && !has(SemanticSlotId::Got)
        && !has(SemanticSlotId::Need)
    {
        return SemanticShape::Context;
    }
    if has(SemanticSlotId::Got) || has(SemanticSlotId::Via) {
        return SemanticShape::Contrast;
    }
    if has(SemanticSlotId::Want) || has(SemanticSlotId::Near) {
        return SemanticShape::Parser;
    }
    SemanticShape::Generic
}

fn builtin_label_catalog() -> BTreeMap<String, String> {
    BTreeMap::from([
        ("first_action".to_string(), "help".to_string()),
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
        ("raw".to_string(), "why".to_string()),
        ("omitted".to_string(), "omitted".to_string()),
    ])
}

fn subject_blocks_label_catalog() -> BTreeMap<String, String> {
    let mut labels = builtin_label_catalog();
    labels.insert("raw".to_string(), "raw".to_string());
    labels.insert("why_raw".to_string(), "raw".to_string());
    labels
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
        assert!(!resolved.subject_first_header);
    }

    #[test]
    fn subject_blocks_maps_contrast_and_linker_families() {
        let policy = ResolvedPresentationPolicy::subject_blocks_v1();

        let contrast = policy.resolve_card_presentation(Some("const_qualifier"));
        let linker = policy.resolve_card_presentation(Some("linker.undefined_reference"));
        let lookup = policy.resolve_card_presentation(Some("pointer_reference"));
        let conflict = policy.resolve_card_presentation(Some("redefinition"));
        let context = policy.resolve_card_presentation(Some("macro_include"));
        let preprocessor = policy.resolve_card_presentation(Some("preprocessor_directive"));

        assert_eq!(contrast.template_id, "contrast_block");
        assert_eq!(contrast.display_family.as_deref(), Some("type_mismatch"));
        assert_eq!(contrast.semantic_shape, SemanticShape::Contrast);
        assert!(contrast.subject_first_header);
        assert_eq!(linker.template_id, "linker_block");
        assert_eq!(linker.display_family.as_deref(), Some("linker"));
        assert_eq!(linker.semantic_shape, SemanticShape::Linker);
        assert!(linker.subject_first_header);
        assert_eq!(lookup.template_id, "lookup_block");
        assert_eq!(lookup.display_family.as_deref(), Some("incomplete_type"));
        assert_eq!(lookup.semantic_shape, SemanticShape::Lookup);
        assert_eq!(conflict.template_id, "conflict_block");
        assert_eq!(conflict.display_family.as_deref(), Some("redefinition"));
        assert_eq!(conflict.semantic_shape, SemanticShape::Conflict);
        assert_eq!(context.template_id, "context_block");
        assert_eq!(context.display_family.as_deref(), Some("macro_include"));
        assert_eq!(context.semantic_shape, SemanticShape::Context);
        assert_eq!(preprocessor.template_id, "parser_block");
        assert_eq!(preprocessor.display_family.as_deref(), Some("syntax"));
        assert_eq!(preprocessor.semantic_shape, SemanticShape::Parser);
        assert_eq!(preprocessor.shape_fallbacks.len(), 1);
        assert_eq!(
            preprocessor.shape_fallbacks[0].shape,
            SemanticShape::MissingHeader
        );
        assert_eq!(
            preprocessor.shape_fallbacks[0].display_family.as_deref(),
            Some("missing_header")
        );
    }

    #[test]
    fn missing_template_falls_back_to_generic_block() {
        let mut policy = ResolvedPresentationPolicy::subject_blocks_v1();
        policy.family_mappings = vec![ResolvedFamilyPresentation {
            matcher: "syntax".to_string(),
            display_family: Some("syntax".to_string()),
            template_id: "missing_block".to_string(),
            semantic_shape: None,
            shape_fallbacks: Vec::new(),
        }];

        let resolved = policy.resolve_card_presentation(Some("syntax"));

        assert_eq!(resolved.template_id, "generic_block");
        assert_eq!(resolved.semantic_shape, SemanticShape::Generic);
        assert!(resolved.fell_back_to_generic_template);
    }

    #[test]
    fn template_semantic_shape_infers_custom_contrast_aliases() {
        let mut policy = ResolvedPresentationPolicy::subject_blocks_v1();
        let contrast_template = policy.template("contrast_block").unwrap().clone();
        policy.templates.insert(
            "contrast_alt".to_string(),
            ResolvedTemplate {
                id: "contrast_alt".to_string(),
                core: contrast_template.core,
            },
        );

        assert_eq!(
            policy.template_semantic_shape("contrast_alt"),
            SemanticShape::Contrast
        );
    }

    #[test]
    fn template_id_for_shape_prefers_current_alias_and_fallback_shape_template() {
        let mut policy = ResolvedPresentationPolicy::subject_blocks_v1();
        let contrast_template = policy.template("contrast_block").unwrap().clone();
        policy.templates.insert(
            "contrast_alt".to_string(),
            ResolvedTemplate {
                id: "contrast_alt".to_string(),
                core: contrast_template.core,
            },
        );

        assert_eq!(
            policy.template_id_for_shape("contrast_alt", SemanticShape::Contrast),
            Some("contrast_alt")
        );
        assert_eq!(
            policy.template_id_for_shape("parser_block", SemanticShape::MissingHeader),
            Some("missing_header_block")
        );
    }

    #[test]
    fn builtin_labels_cover_core_slot_ids() {
        let legacy = ResolvedPresentationPolicy::legacy_v1();
        let policy = ResolvedPresentationPolicy::subject_blocks_v1();

        assert_eq!(legacy.slot_label(SemanticSlotId::Raw), Some("why"));
        assert_eq!(policy.slot_label(SemanticSlotId::Raw), Some("raw"));
        assert_eq!(policy.slot_label(SemanticSlotId::WhyRaw), Some("raw"));
        assert_eq!(policy.slot_label(SemanticSlotId::FirstAction), Some("help"));
        assert_eq!(policy.label("help"), Some("help"));
        assert_eq!(policy.label("raw"), Some("raw"));
        assert_eq!(policy.label_width_mode, LabelWidthMode::TemplateMax);
        assert_eq!(SemanticSlotId::Raw.stable_id(), "raw");
    }

    #[test]
    fn header_policy_controls_subject_first_activation() {
        let mut policy = ResolvedPresentationPolicy::subject_blocks_v1();
        policy.header.subject_first = false;

        let resolved = policy.resolve_card_presentation(Some("syntax"));

        assert!(!resolved.subject_first_header);
    }

    #[test]
    fn card_presentation_inherits_header_location_and_template_label_width() {
        let mut policy = ResolvedPresentationPolicy::subject_blocks_v1();
        policy.header.interactive_format = "{severity} -> [{family}] {subject}".to_string();
        policy.location_policy.inline_suffix_format = " ({location})".to_string();
        policy.location_policy.width_soft_limit = 72;
        policy
            .label_catalog
            .insert("want".to_string(), "expected type".to_string());

        let resolved = policy.resolve_card_presentation(Some("type_overload"));

        assert_eq!(
            resolved.header.interactive_format,
            "{severity} -> [{family}] {subject}"
        );
        assert_eq!(
            resolved.location_policy.inline_suffix_format,
            " ({location})"
        );
        assert_eq!(resolved.location_policy.width_soft_limit, 72);
        assert_eq!(
            resolved.evidence_label_width,
            "expected type".chars().count()
        );
    }

    #[test]
    fn fixed_label_width_overrides_template_max_width() {
        let mut policy = ResolvedPresentationPolicy::subject_blocks_v1();
        policy.label_width_mode = LabelWidthMode::Fixed;
        policy.fixed_label_width = Some(6);
        policy
            .label_catalog
            .insert("symbol".to_string(), "very_long_symbol".to_string());

        let resolved = policy.resolve_card_presentation(Some("linker.undefined_reference"));

        assert_eq!(resolved.evidence_label_width, 6);
    }
}
