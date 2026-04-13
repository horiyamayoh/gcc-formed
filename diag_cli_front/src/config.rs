use crate::args::{
    ParsedArgs, parse_compression_level, parse_debug_refs, parse_mode, parse_probability,
    parse_processing_path, parse_profile, parse_retention_policy,
    parse_suppressed_count_visibility,
};
use crate::error::CliError;
use diag_backend_probe::ProcessingPath;
use diag_capture_runtime::ExecutionMode;
use diag_core::{CascadePolicySnapshot, CompressionLevel, SuppressedCountVisibility};
use diag_render::{
    DebugRefs, LocationPlacement, PathPolicy, RenderProfile,
    ResolvedFamilyPresentation as RenderResolvedFamilyPresentation,
    ResolvedHeaderPolicy as RenderResolvedHeaderPolicy,
    ResolvedLocationPolicy as RenderResolvedLocationPolicy,
    ResolvedPresentationPolicy as RenderResolvedPresentationPolicy, ResolvedShapeFallback,
    ResolvedTemplate as RenderResolvedTemplate, ResolvedTemplateLine as RenderResolvedTemplateLine,
    SemanticShape, SemanticSlotId, SessionMode,
};
use diag_trace::{RetentionPolicy, WrapperPaths};
use serde::Deserialize;
use std::collections::BTreeMap;
use std::env;
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};

const DEFAULT_PRESENTATION_PRESET: &str = "subject_blocks_v1";
const PRESENTATION_SCHEMA_KIND: &str = "cc_formed_presentation";
const PRESENTATION_SCHEMA_VERSION_V1: u32 = 1;
const PRESENTATION_SCHEMA_VERSION_V2: u32 = 2;
const GENERIC_BLOCK_TEMPLATE: &str = "generic_block";
const SUBJECT_BLOCKS_V2_ASSET: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../config/presentation/subject_blocks_v2.toml"
));
const SUBJECT_BLOCKS_V1_ASSET: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../config/presentation/subject_blocks_v1.toml"
));
const LEGACY_V1_ASSET: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../config/presentation/legacy_v1.toml"
));
const KNOWN_SESSION_MODES: &[&str] = &["all_visible_blocks", "lead_plus_summary", "capped_blocks"];
const KNOWN_BLOCK_SEPARATORS: &[&str] = &["blank_line"];
const KNOWN_LOCATION_PLACEMENTS: &[&str] = &[
    "inline_suffix",
    "header_suffix",
    "evidence_suffix",
    "excerpt_header",
    "dedicated_line",
    "none",
];
const KNOWN_FALLBACK_LOCATIONS: &[&str] = &[
    "header",
    "header_suffix",
    "evidence",
    "evidence_suffix",
    "excerpt_header",
    "dedicated_line",
    "none",
];
const KNOWN_TEMPLATE_EXCERPTS: &[&str] = &["off", "auto", "on"];
const KNOWN_TEMPLATE_SLOTS: &[&str] = &[
    "first_action",
    "help",
    "expected",
    "want",
    "actual",
    "got",
    "via",
    "need",
    "from",
    "name",
    "use",
    "near",
    "symbol",
    "archive",
    "now",
    "prev",
    "why_raw",
    "raw",
];
const KNOWN_SUFFIX_SLOTS: &[&str] = &["omitted_notes_suffix", "omitted_refs_suffix"];
const KNOWN_LABEL_WIDTH_MODES: &[&str] = &["template_max", "fixed"];
const KNOWN_SEMANTIC_SHAPES: &[&str] = &[
    "contrast",
    "parser",
    "lookup",
    "missing_header",
    "conflict",
    "context",
    "linker",
    "generic",
];

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
pub(crate) struct PresentationConfigFile {
    pub(crate) kind: Option<String>,
    pub(crate) schema_version: Option<u32>,
    #[serde(default)]
    pub(crate) session: PresentationSessionSection,
    #[serde(default)]
    pub(crate) header: PresentationHeaderSection,
    #[serde(default)]
    pub(crate) labels: PresentationLabelsSection,
    #[serde(default)]
    pub(crate) location: PresentationLocationSection,
    #[serde(default)]
    pub(crate) templates: Vec<PresentationTemplate>,
    #[serde(default)]
    pub(crate) family_mappings: Vec<PresentationFamilyMapping>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
pub(crate) struct PresentationSessionSection {
    #[serde(default)]
    pub(crate) visible_root_mode: Option<String>,
    #[serde(default)]
    pub(crate) warning_only_mode: Option<String>,
    #[serde(default)]
    pub(crate) block_separator: Option<String>,
    #[serde(default)]
    pub(crate) unknown_template: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
pub(crate) struct PresentationHeaderSection {
    #[serde(default)]
    pub(crate) subject_first: Option<bool>,
    #[serde(default)]
    pub(crate) interactive_format: Option<String>,
    #[serde(default)]
    pub(crate) ci_path_first_format: Option<String>,
    #[serde(default)]
    pub(crate) unknown_family: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
pub(crate) struct PresentationLabelsSection {
    #[serde(default)]
    pub(crate) label_width_mode: Option<String>,
    #[serde(default)]
    pub(crate) fixed_label_width: Option<usize>,
    #[serde(flatten)]
    pub(crate) values: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
pub(crate) struct PresentationLocationSection {
    #[serde(default)]
    pub(crate) default_placement: Option<String>,
    #[serde(default)]
    pub(crate) fallback_order: Option<Vec<String>>,
    #[serde(default)]
    pub(crate) inline_suffix_format: Option<String>,
    #[serde(default)]
    pub(crate) width_soft_limit: Option<usize>,
    #[serde(default)]
    pub(crate) label_width: Option<usize>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
pub(crate) struct PresentationTemplate {
    pub(crate) id: String,
    #[serde(default)]
    pub(crate) excerpt: Option<String>,
    #[serde(default)]
    pub(crate) core: Vec<PresentationTemplateLine>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
pub(crate) struct PresentationTemplateLine {
    pub(crate) slot: String,
    #[serde(default)]
    pub(crate) label: Option<String>,
    #[serde(default)]
    pub(crate) suffix_slot: Option<String>,
    #[serde(default)]
    pub(crate) optional: Option<bool>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
pub(crate) struct PresentationFamilyMapping {
    #[serde(default, rename = "match")]
    pub(crate) matchers: Vec<String>,
    #[serde(default)]
    pub(crate) display_family: Option<String>,
    #[serde(default)]
    pub(crate) template: Option<String>,
    #[serde(default)]
    pub(crate) semantic_shape: Option<String>,
    #[serde(default)]
    pub(crate) shape_fallback: Vec<PresentationShapeFallback>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
pub(crate) struct PresentationShapeFallback {
    pub(crate) shape: String,
    #[serde(default)]
    pub(crate) display_family: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(dead_code)]
pub(crate) struct ResolvedPresentationPolicy {
    pub(crate) preset_id: String,
    pub(crate) presentation_file: Option<PathBuf>,
    pub(crate) policy: PresentationConfigFile,
    pub(crate) warnings: Vec<String>,
    pub(crate) fell_back_to_default: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ResolvedCascadePolicy {
    pub(crate) policy: CascadePolicySnapshot,
    pub(crate) warnings: Vec<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub(crate) struct ConfigFile {
    #[serde(default)]
    pub(crate) schema_version: Option<u32>,
    #[serde(default)]
    pub(crate) backend: BackendSection,
    #[serde(default)]
    pub(crate) runtime: RuntimeSection,
    #[serde(default)]
    pub(crate) render: RenderSection,
    #[serde(default)]
    pub(crate) trace: TraceSection,
    #[serde(default)]
    pub(crate) cascade: CascadeSection,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub(crate) struct BackendSection {
    #[serde(default)]
    pub(crate) gcc: Option<PathBuf>,
    #[serde(default)]
    pub(crate) launcher: Option<PathBuf>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub(crate) struct RuntimeSection {
    #[serde(default, deserialize_with = "deserialize_optional_mode")]
    pub(crate) mode: Option<ExecutionMode>,
    #[serde(default, deserialize_with = "deserialize_optional_processing_path")]
    pub(crate) processing_path: Option<ProcessingPath>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub(crate) struct RenderSection {
    #[serde(default, deserialize_with = "deserialize_optional_profile")]
    pub(crate) profile: Option<RenderProfile>,
    #[serde(default, deserialize_with = "deserialize_optional_path_policy")]
    pub(crate) path_policy: Option<PathPolicy>,
    #[serde(default, deserialize_with = "deserialize_optional_debug_refs")]
    pub(crate) debug_refs: Option<DebugRefs>,
    #[serde(default, alias = "presentation_preset")]
    pub(crate) presentation: Option<String>,
    #[serde(default)]
    pub(crate) presentation_file: Option<PathBuf>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub(crate) struct TraceSection {
    #[serde(default, deserialize_with = "deserialize_optional_retention")]
    pub(crate) retention_policy: Option<RetentionPolicy>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub(crate) struct CascadeSection {
    #[serde(default, deserialize_with = "deserialize_optional_compression_level")]
    pub(crate) compression_level: Option<CompressionLevel>,
    #[serde(
        default,
        deserialize_with = "deserialize_optional_suppress_likelihood_threshold"
    )]
    pub(crate) suppress_likelihood_threshold: Option<f32>,
    #[serde(
        default,
        deserialize_with = "deserialize_optional_summary_likelihood_threshold"
    )]
    pub(crate) summary_likelihood_threshold: Option<f32>,
    #[serde(default, deserialize_with = "deserialize_optional_min_parent_margin")]
    pub(crate) min_parent_margin: Option<f32>,
    #[serde(default)]
    pub(crate) max_expanded_independent_roots: Option<usize>,
    #[serde(
        default,
        deserialize_with = "deserialize_optional_suppressed_count_visibility"
    )]
    pub(crate) show_suppressed_count: Option<SuppressedCountVisibility>,
}

impl ConfigFile {
    pub(crate) fn load(paths: &WrapperPaths) -> Result<Self, CliError> {
        Self::load_from_paths(admin_config_paths(), Some(&paths.config_path))
    }

    pub(crate) fn resolve_cascade_policy(&self, parsed: &ParsedArgs) -> ResolvedCascadePolicy {
        let defaults = CascadePolicySnapshot::default();
        let policy = CascadePolicySnapshot {
            compression_level: parsed
                .cascade_compression_level
                .or(self.cascade.compression_level)
                .unwrap_or(defaults.compression_level),
            suppress_likelihood_threshold: parsed
                .cascade_suppress_likelihood_threshold
                .or(self.cascade.suppress_likelihood_threshold)
                .unwrap_or(defaults.suppress_likelihood_threshold),
            summary_likelihood_threshold: parsed
                .cascade_summary_likelihood_threshold
                .or(self.cascade.summary_likelihood_threshold)
                .unwrap_or(defaults.summary_likelihood_threshold),
            min_parent_margin: parsed
                .cascade_min_parent_margin
                .or(self.cascade.min_parent_margin)
                .unwrap_or(defaults.min_parent_margin),
            max_expanded_independent_roots: parsed
                .cascade_max_expanded_independent_roots
                .or(self.cascade.max_expanded_independent_roots)
                .unwrap_or(defaults.max_expanded_independent_roots),
            show_suppressed_count: parsed
                .cascade_show_suppressed_count
                .or(self.cascade.show_suppressed_count)
                .unwrap_or(defaults.show_suppressed_count),
        };
        let warnings = if parsed.cascade_max_expanded_independent_roots.is_some()
            || self.cascade.max_expanded_independent_roots.is_some()
        {
            vec![
                "note: cascade.max_expanded_independent_roots and the corresponding CLI flags are deprecated as visible-root caps; the current value is still honored for compatibility, but new visible-root behavior should be expressed through render.presentation or render.presentation_file.session.visible_root_mode".to_string(),
            ]
        } else {
            Vec::new()
        };

        ResolvedCascadePolicy { policy, warnings }
    }

    #[allow(dead_code)]
    pub(crate) fn resolve_presentation_policy(
        &self,
        parsed: &ParsedArgs,
    ) -> ResolvedPresentationPolicy {
        let default_policy = load_builtin_presentation_asset(DEFAULT_PRESENTATION_PRESET)
            .expect("valid default preset");
        let requested_preset = parsed
            .presentation
            .as_deref()
            .or(self.render.presentation.as_deref())
            .unwrap_or(DEFAULT_PRESENTATION_PRESET);
        let presentation_file = parsed
            .presentation_file
            .clone()
            .or_else(|| self.render.presentation_file.clone());

        let mut warnings = Vec::new();
        let mut fell_back_to_default = false;
        let mut preset_id = requested_preset.to_string();
        let mut policy = match load_builtin_presentation_asset(requested_preset) {
            Ok(policy) => policy,
            Err(error) => {
                fell_back_to_default = true;
                warnings.push(format!(
                    "note: presentation preset '{requested_preset}' is unavailable; using built-in default '{DEFAULT_PRESENTATION_PRESET}' ({error})"
                ));
                preset_id = DEFAULT_PRESENTATION_PRESET.to_string();
                default_policy.clone()
            }
        };

        if let Some(path) = presentation_file.as_ref() {
            match load_presentation_file(path) {
                Ok(overlay) => {
                    policy = merge_presentation_config(policy, overlay);
                }
                Err(error) => {
                    fell_back_to_default = true;
                    warnings.push(format!(
                        "note: failed to load presentation file {}; using built-in default '{DEFAULT_PRESENTATION_PRESET}' ({error})",
                        path.display()
                    ));
                    preset_id = DEFAULT_PRESENTATION_PRESET.to_string();
                    policy = default_policy.clone();
                }
            }
        }

        normalize_presentation_config(&mut policy, &default_policy, &mut warnings);

        ResolvedPresentationPolicy {
            preset_id,
            presentation_file,
            policy,
            warnings,
            fell_back_to_default,
        }
    }

    fn load_from_paths<I, P>(admin_paths: I, user_path: Option<P>) -> Result<Self, CliError>
    where
        I: IntoIterator<Item = PathBuf>,
        P: AsRef<Path>,
    {
        let mut merged = ConfigFile::default();
        if let Some(admin) = admin_paths.into_iter().find(|path| path.exists()) {
            merged = merge_config(merged, load_config_file(&admin)?);
        }
        if let Some(user_path) = user_path
            && user_path.as_ref().exists()
        {
            merged = merge_config(merged, load_config_file(user_path.as_ref())?);
        }
        Ok(merged)
    }
}

impl ResolvedPresentationPolicy {
    pub(crate) fn to_render_policy(&self) -> RenderResolvedPresentationPolicy {
        let label_catalog = resolved_label_catalog(&self.policy);
        let templates = self
            .policy
            .templates
            .iter()
            .map(|template| {
                (
                    template.id.clone(),
                    RenderResolvedTemplate {
                        id: template.id.clone(),
                        core: template
                            .core
                            .iter()
                            .filter_map(|line| {
                                Some(RenderResolvedTemplateLine {
                                    slot: semantic_slot_id(&line.slot)?,
                                    label: line.label.clone(),
                                    suffix_slot: line.suffix_slot.clone(),
                                    optional: line.optional.unwrap_or(false),
                                })
                            })
                            .collect(),
                    },
                )
            })
            .collect();

        RenderResolvedPresentationPolicy {
            preset_id: self.preset_id.clone(),
            session_mode: session_mode(
                self.policy
                    .session
                    .visible_root_mode
                    .as_deref()
                    .unwrap_or("all_visible_blocks"),
            ),
            header: RenderResolvedHeaderPolicy {
                subject_first: self.policy.header.subject_first.unwrap_or(false),
                interactive_format: self
                    .policy
                    .header
                    .interactive_format
                    .clone()
                    .unwrap_or_else(|| "{severity}: [{family}] {subject}".to_string()),
                ci_path_first_format: self
                    .policy
                    .header
                    .ci_path_first_format
                    .clone()
                    .unwrap_or_else(|| "{location}: {severity}: [{family}] {subject}".to_string()),
                unknown_family: self
                    .policy
                    .header
                    .unknown_family
                    .clone()
                    .unwrap_or_else(|| "generic".to_string()),
            },
            location_policy: RenderResolvedLocationPolicy {
                default_placement: location_placement(
                    self.policy
                        .location
                        .default_placement
                        .as_deref()
                        .unwrap_or("inline_suffix"),
                ),
                fallback_order: self
                    .policy
                    .location
                    .fallback_order
                    .as_ref()
                    .into_iter()
                    .flatten()
                    .map(|placement| location_placement(placement))
                    .collect(),
            },
            label_catalog,
            templates,
            family_mappings: self
                .policy
                .family_mappings
                .iter()
                .flat_map(|mapping| {
                    mapping
                        .matchers
                        .iter()
                        .map(|matcher| RenderResolvedFamilyPresentation {
                            matcher: matcher.clone(),
                            display_family: mapping.display_family.clone(),
                            template_id: mapping
                                .template
                                .clone()
                                .unwrap_or_else(|| GENERIC_BLOCK_TEMPLATE.to_string()),
                            semantic_shape: mapping
                                .semantic_shape
                                .as_deref()
                                .and_then(resolved_semantic_shape),
                            shape_fallbacks: mapping
                                .shape_fallback
                                .iter()
                                .filter_map(resolved_shape_fallback)
                                .collect(),
                        })
                })
                .collect(),
            default_template_id: default_template_id_for_preset(&self.preset_id).to_string(),
            generic_template_id: GENERIC_BLOCK_TEMPLATE.to_string(),
            warnings: self.warnings.clone(),
            fell_back_to_default: self.fell_back_to_default,
        }
    }
}

fn admin_config_paths() -> Vec<PathBuf> {
    admin_config_paths_from(env::var_os("XDG_CONFIG_DIRS"))
}

fn admin_config_paths_from(raw_xdg_config_dirs: Option<OsString>) -> Vec<PathBuf> {
    let dirs = raw_xdg_config_dirs
        .as_deref()
        .filter(|value| !value.is_empty())
        .map(|value| {
            env::split_paths(value)
                .filter(|path| !path.as_os_str().is_empty())
                .collect::<Vec<_>>()
        })
        .filter(|paths| !paths.is_empty())
        .unwrap_or_else(|| vec![PathBuf::from("/etc/xdg")]);
    dirs.into_iter()
        .map(|dir| dir.join("cc-formed").join("config.toml"))
        .collect()
}

fn load_config_file(path: &Path) -> Result<ConfigFile, CliError> {
    let mut config: ConfigFile =
        toml::from_str(&fs::read_to_string(path)?).map_err(|e| CliError::Config(e.to_string()))?;
    if let Some(presentation_file) = config.render.presentation_file.as_mut()
        && presentation_file.is_relative()
        && let Some(parent) = path.parent()
    {
        *presentation_file = parent.join(&*presentation_file);
    }
    Ok(config)
}

fn merge_config(base: ConfigFile, overlay: ConfigFile) -> ConfigFile {
    ConfigFile {
        schema_version: overlay.schema_version.or(base.schema_version),
        backend: BackendSection {
            gcc: overlay.backend.gcc.or(base.backend.gcc),
            launcher: overlay.backend.launcher.or(base.backend.launcher),
        },
        runtime: RuntimeSection {
            mode: overlay.runtime.mode.or(base.runtime.mode),
            processing_path: overlay
                .runtime
                .processing_path
                .or(base.runtime.processing_path),
        },
        render: RenderSection {
            profile: overlay.render.profile.or(base.render.profile),
            path_policy: overlay.render.path_policy.or(base.render.path_policy),
            debug_refs: overlay.render.debug_refs.or(base.render.debug_refs),
            presentation: overlay.render.presentation.or(base.render.presentation),
            presentation_file: overlay
                .render
                .presentation_file
                .or(base.render.presentation_file),
        },
        trace: TraceSection {
            retention_policy: overlay
                .trace
                .retention_policy
                .or(base.trace.retention_policy),
        },
        cascade: CascadeSection {
            compression_level: overlay
                .cascade
                .compression_level
                .or(base.cascade.compression_level),
            suppress_likelihood_threshold: overlay
                .cascade
                .suppress_likelihood_threshold
                .or(base.cascade.suppress_likelihood_threshold),
            summary_likelihood_threshold: overlay
                .cascade
                .summary_likelihood_threshold
                .or(base.cascade.summary_likelihood_threshold),
            min_parent_margin: overlay
                .cascade
                .min_parent_margin
                .or(base.cascade.min_parent_margin),
            max_expanded_independent_roots: overlay
                .cascade
                .max_expanded_independent_roots
                .or(base.cascade.max_expanded_independent_roots),
            show_suppressed_count: overlay
                .cascade
                .show_suppressed_count
                .or(base.cascade.show_suppressed_count),
        },
    }
}

fn load_builtin_presentation_asset(preset_id: &str) -> Result<PresentationConfigFile, String> {
    let source = match preset_id {
        "subject_blocks_v2" => SUBJECT_BLOCKS_V2_ASSET,
        "subject_blocks_v1" => SUBJECT_BLOCKS_V1_ASSET,
        "legacy_v1" => LEGACY_V1_ASSET,
        other => return Err(format!("unknown preset id: {other}")),
    };
    let mut config = parse_presentation_config(source, &format!("built-in preset '{preset_id}'"))?;
    apply_builtin_presentation_defaults(&mut config, preset_id);
    Ok(config)
}

fn load_presentation_file(path: &Path) -> Result<PresentationConfigFile, String> {
    parse_presentation_config(
        &fs::read_to_string(path).map_err(|e| e.to_string())?,
        &path.display().to_string(),
    )
}

fn parse_presentation_config(
    contents: &str,
    source: &str,
) -> Result<PresentationConfigFile, String> {
    let config: PresentationConfigFile =
        toml::from_str(contents).map_err(|e| format!("failed to parse {source}: {e}"))?;
    match config.kind.as_deref() {
        Some(PRESENTATION_SCHEMA_KIND) => {}
        Some(other) => {
            return Err(format!(
                "expected kind = \"{PRESENTATION_SCHEMA_KIND}\" in {source}, found \"{other}\""
            ));
        }
        None => return Err(format!("missing kind in {source}")),
    }
    match config.schema_version {
        Some(PRESENTATION_SCHEMA_VERSION_V1 | PRESENTATION_SCHEMA_VERSION_V2) => {}
        Some(other) => {
            return Err(format!(
                "unsupported presentation schema_version in {source}: {other} (supported: {} and {})",
                PRESENTATION_SCHEMA_VERSION_V1, PRESENTATION_SCHEMA_VERSION_V2
            ));
        }
        None => return Err(format!("missing schema_version in {source}")),
    }
    Ok(config)
}

fn merge_presentation_config(
    mut base: PresentationConfigFile,
    overlay: PresentationConfigFile,
) -> PresentationConfigFile {
    base.kind = overlay.kind.or(base.kind);
    base.schema_version = overlay.schema_version.or(base.schema_version);
    base.header.subject_first = overlay.header.subject_first.or(base.header.subject_first);
    base.header.interactive_format = overlay
        .header
        .interactive_format
        .or(base.header.interactive_format);
    base.header.ci_path_first_format = overlay
        .header
        .ci_path_first_format
        .or(base.header.ci_path_first_format);
    base.header.unknown_family = overlay.header.unknown_family.or(base.header.unknown_family);
    base.session.visible_root_mode = overlay
        .session
        .visible_root_mode
        .or(base.session.visible_root_mode);
    base.session.warning_only_mode = overlay
        .session
        .warning_only_mode
        .or(base.session.warning_only_mode);
    base.session.block_separator = overlay
        .session
        .block_separator
        .or(base.session.block_separator);
    base.session.unknown_template = overlay
        .session
        .unknown_template
        .or(base.session.unknown_template);
    base.labels.label_width_mode = overlay
        .labels
        .label_width_mode
        .or(base.labels.label_width_mode);
    base.labels.fixed_label_width = overlay
        .labels
        .fixed_label_width
        .or(base.labels.fixed_label_width);
    for (key, value) in overlay.labels.values {
        base.labels.values.insert(key, value);
    }
    base.location.default_placement = overlay
        .location
        .default_placement
        .or(base.location.default_placement);
    base.location.fallback_order = overlay
        .location
        .fallback_order
        .or(base.location.fallback_order);
    base.location.inline_suffix_format = overlay
        .location
        .inline_suffix_format
        .or(base.location.inline_suffix_format);
    base.location.width_soft_limit = overlay
        .location
        .width_soft_limit
        .or(base.location.width_soft_limit);
    base.location.label_width = overlay.location.label_width.or(base.location.label_width);
    base.templates = merge_templates(base.templates, overlay.templates);
    base.family_mappings = merge_family_mappings(base.family_mappings, overlay.family_mappings);
    base
}

fn merge_templates(
    mut base: Vec<PresentationTemplate>,
    overlay: Vec<PresentationTemplate>,
) -> Vec<PresentationTemplate> {
    for template in overlay {
        if let Some(index) = base
            .iter()
            .position(|candidate| candidate.id == template.id)
        {
            base[index] = template;
        } else {
            base.push(template);
        }
    }
    base
}

fn merge_family_mappings(
    mut base: Vec<PresentationFamilyMapping>,
    overlay: Vec<PresentationFamilyMapping>,
) -> Vec<PresentationFamilyMapping> {
    for mapping in overlay {
        if let Some(index) = base
            .iter()
            .position(|candidate| candidate.matchers == mapping.matchers)
        {
            base[index] = mapping;
        } else {
            base.push(mapping);
        }
    }
    base
}

fn semantic_slot_id(slot: &str) -> Option<SemanticSlotId> {
    match slot {
        "first_action" => Some(SemanticSlotId::FirstAction),
        "help" => Some(SemanticSlotId::FirstAction),
        "expected" => Some(SemanticSlotId::Want),
        "want" => Some(SemanticSlotId::Want),
        "actual" => Some(SemanticSlotId::Got),
        "got" => Some(SemanticSlotId::Got),
        "via" => Some(SemanticSlotId::Via),
        "name" => Some(SemanticSlotId::Name),
        "use" => Some(SemanticSlotId::Use),
        "need" => Some(SemanticSlotId::Need),
        "from" => Some(SemanticSlotId::From),
        "near" => Some(SemanticSlotId::Near),
        "now" => Some(SemanticSlotId::Now),
        "prev" => Some(SemanticSlotId::Prev),
        "symbol" => Some(SemanticSlotId::Symbol),
        "archive" => Some(SemanticSlotId::Archive),
        "why_raw" => Some(SemanticSlotId::WhyRaw),
        "raw" => Some(SemanticSlotId::Raw),
        _ => None,
    }
}

fn resolved_semantic_shape(shape: &str) -> Option<SemanticShape> {
    match shape {
        "contrast" => Some(SemanticShape::Contrast),
        "parser" => Some(SemanticShape::Parser),
        "lookup" => Some(SemanticShape::Lookup),
        "missing_header" => Some(SemanticShape::MissingHeader),
        "conflict" => Some(SemanticShape::Conflict),
        "context" => Some(SemanticShape::Context),
        "linker" => Some(SemanticShape::Linker),
        "generic" => Some(SemanticShape::Generic),
        _ => None,
    }
}

fn resolved_shape_fallback(fallback: &PresentationShapeFallback) -> Option<ResolvedShapeFallback> {
    Some(ResolvedShapeFallback {
        shape: resolved_semantic_shape(&fallback.shape)?,
        display_family: fallback.display_family.clone(),
    })
}

fn session_mode(mode: &str) -> SessionMode {
    match mode {
        "all_visible_blocks" => SessionMode::AllVisibleBlocks,
        "lead_plus_summary" => SessionMode::LeadPlusSummary,
        "capped_blocks" => SessionMode::CappedBlocks,
        _ => SessionMode::AllVisibleBlocks,
    }
}

fn location_placement(placement: &str) -> LocationPlacement {
    match placement {
        "inline_suffix" => LocationPlacement::InlineSuffix,
        "header" => LocationPlacement::HeaderSuffix,
        "header_suffix" => LocationPlacement::HeaderSuffix,
        "evidence" => LocationPlacement::EvidenceSuffix,
        "evidence_suffix" => LocationPlacement::EvidenceSuffix,
        "excerpt_header" => LocationPlacement::ExcerptHeader,
        "dedicated_line" => LocationPlacement::DedicatedLine,
        "none" => LocationPlacement::None,
        _ => LocationPlacement::None,
    }
}

fn resolved_label_catalog(policy: &PresentationConfigFile) -> BTreeMap<String, String> {
    let mut labels = policy.labels.values.clone();
    for template in &policy.templates {
        for line in &template.core {
            if let (Some(slot), Some(label_id)) =
                (semantic_slot_id(&line.slot), line.label.as_deref())
                && let Some(label) = policy.labels.values.get(label_id)
            {
                labels.insert(slot.stable_id().to_string(), label.clone());
                if matches!(slot, SemanticSlotId::Raw) {
                    labels.insert("why_raw".to_string(), label.clone());
                }
            }
        }
    }
    if let Some(raw) = labels.get("raw").cloned() {
        labels.entry("why_raw".to_string()).or_insert(raw);
    }
    labels
}

fn default_template_id_for_preset(preset_id: &str) -> &'static str {
    match preset_id {
        "legacy_v1" => "legacy_primary_block",
        _ => GENERIC_BLOCK_TEMPLATE,
    }
}

fn normalize_presentation_config(
    policy: &mut PresentationConfigFile,
    defaults: &PresentationConfigFile,
    warnings: &mut Vec<String>,
) {
    policy
        .kind
        .get_or_insert_with(|| PRESENTATION_SCHEMA_KIND.to_string());
    if policy.schema_version.is_none() {
        policy.schema_version = defaults
            .schema_version
            .or(Some(PRESENTATION_SCHEMA_VERSION_V2));
    }
    normalize_session(policy, defaults, warnings);
    normalize_header(policy, defaults);
    normalize_labels(policy, defaults, warnings);
    normalize_location(policy, defaults, warnings);
    normalize_templates(policy, defaults, warnings);
    normalize_family_mappings(policy, defaults, warnings);
}

fn normalize_header(policy: &mut PresentationConfigFile, defaults: &PresentationConfigFile) {
    policy.header.subject_first = policy
        .header
        .subject_first
        .or(defaults.header.subject_first)
        .or(Some(false));
    policy.header.interactive_format = policy
        .header
        .interactive_format
        .clone()
        .or_else(|| defaults.header.interactive_format.clone())
        .or(Some("{severity}: [{family}] {subject}".to_string()));
    policy.header.ci_path_first_format = policy
        .header
        .ci_path_first_format
        .clone()
        .or_else(|| defaults.header.ci_path_first_format.clone())
        .or(Some(
            "{location}: {severity}: [{family}] {subject}".to_string(),
        ));
    policy.header.unknown_family = policy
        .header
        .unknown_family
        .clone()
        .or_else(|| defaults.header.unknown_family.clone())
        .or(Some("generic".to_string()));
}

fn normalize_labels(
    policy: &mut PresentationConfigFile,
    defaults: &PresentationConfigFile,
    warnings: &mut Vec<String>,
) {
    normalize_enum_field(
        &mut policy.labels.label_width_mode,
        defaults
            .labels
            .label_width_mode
            .as_deref()
            .or_else(|| policy.location.label_width.map(|_| "fixed")),
        KNOWN_LABEL_WIDTH_MODES,
        "labels.label_width_mode",
        warnings,
    );

    if matches!(policy.labels.label_width_mode.as_deref(), Some("fixed"))
        && policy.labels.fixed_label_width.unwrap_or(0) == 0
    {
        policy.labels.fixed_label_width = policy
            .location
            .label_width
            .or(defaults.labels.fixed_label_width)
            .or(defaults.location.label_width)
            .or(Some(4));
    }
}

fn normalize_session(
    policy: &mut PresentationConfigFile,
    defaults: &PresentationConfigFile,
    warnings: &mut Vec<String>,
) {
    normalize_enum_field(
        &mut policy.session.visible_root_mode,
        defaults.session.visible_root_mode.as_deref(),
        KNOWN_SESSION_MODES,
        "session.visible_root_mode",
        warnings,
    );
    normalize_enum_field(
        &mut policy.session.warning_only_mode,
        defaults.session.warning_only_mode.as_deref(),
        KNOWN_SESSION_MODES,
        "session.warning_only_mode",
        warnings,
    );
    normalize_enum_field(
        &mut policy.session.block_separator,
        defaults.session.block_separator.as_deref(),
        KNOWN_BLOCK_SEPARATORS,
        "session.block_separator",
        warnings,
    );

    let generic_template = if policy
        .templates
        .iter()
        .any(|template| template.id == GENERIC_BLOCK_TEMPLATE)
    {
        GENERIC_BLOCK_TEMPLATE.to_string()
    } else {
        defaults
            .session
            .unknown_template
            .clone()
            .unwrap_or_else(|| GENERIC_BLOCK_TEMPLATE.to_string())
    };
    match policy.session.unknown_template.as_deref() {
        Some(value) if policy.templates.iter().any(|template| template.id == value) => {}
        Some(value) => {
            warnings.push(format!(
                "note: unknown session.unknown_template '{value}'; using '{generic_template}'"
            ));
            policy.session.unknown_template = Some(generic_template);
        }
        None => {
            policy.session.unknown_template = Some(generic_template);
        }
    }
}

fn normalize_location(
    policy: &mut PresentationConfigFile,
    defaults: &PresentationConfigFile,
    warnings: &mut Vec<String>,
) {
    normalize_enum_field(
        &mut policy.location.default_placement,
        defaults.location.default_placement.as_deref(),
        KNOWN_LOCATION_PLACEMENTS,
        "location.default_placement",
        warnings,
    );

    let default_fallback = defaults.location.fallback_order.clone().unwrap_or_else(|| {
        vec![
            "header".to_string(),
            "evidence".to_string(),
            "excerpt_header".to_string(),
            "none".to_string(),
        ]
    });
    let mut fallback = policy
        .location
        .fallback_order
        .clone()
        .unwrap_or(default_fallback.clone());
    let original_len = fallback.len();
    fallback.retain(|value| KNOWN_FALLBACK_LOCATIONS.contains(&value.as_str()));
    if fallback.is_empty() {
        warnings.push(
            "note: location.fallback_order was empty or invalid; using built-in default order"
                .to_string(),
        );
        fallback = default_fallback;
    } else if fallback.len() != original_len {
        warnings.push(
            "note: location.fallback_order contained unknown entries; they were ignored"
                .to_string(),
        );
    }
    policy.location.fallback_order = Some(fallback);

    if policy.location.width_soft_limit.unwrap_or(0) == 0 {
        policy.location.width_soft_limit = defaults.location.width_soft_limit.or(Some(100));
    }
    if policy.location.label_width.unwrap_or(0) == 0 {
        policy.location.label_width = defaults.location.label_width.or(Some(4));
    }
    if policy.location.inline_suffix_format.is_none() {
        policy.location.inline_suffix_format = defaults
            .location
            .inline_suffix_format
            .clone()
            .or(Some(" @ {location}".to_string()));
    }
}

fn normalize_templates(
    policy: &mut PresentationConfigFile,
    defaults: &PresentationConfigFile,
    warnings: &mut Vec<String>,
) {
    for template in &mut policy.templates {
        normalize_enum_field(
            &mut template.excerpt,
            template_default_excerpt(defaults, &template.id),
            KNOWN_TEMPLATE_EXCERPTS,
            &format!("template '{}'.excerpt", template.id),
            warnings,
        );
        template.core.retain(|line| {
            if KNOWN_TEMPLATE_SLOTS.contains(&line.slot.as_str()) {
                true
            } else {
                warnings.push(format!(
                    "note: template '{}' references unknown slot '{}'; skipping line",
                    template.id, line.slot
                ));
                false
            }
        });
        for line in &mut template.core {
            if let Some(suffix_slot) = line.suffix_slot.as_deref()
                && !KNOWN_SUFFIX_SLOTS.contains(&suffix_slot)
            {
                warnings.push(format!(
                    "note: template '{}' references unknown suffix slot '{}'; dropping suffix",
                    template.id, suffix_slot
                ));
                line.suffix_slot = None;
            }
        }
    }

    if !policy
        .templates
        .iter()
        .any(|template| template.id == GENERIC_BLOCK_TEMPLATE)
        && let Some(generic) = defaults
            .templates
            .iter()
            .find(|template| template.id == GENERIC_BLOCK_TEMPLATE)
    {
        warnings.push(
            "note: generic_block template was missing; restoring built-in default".to_string(),
        );
        policy.templates.push(generic.clone());
    }
}

fn normalize_family_mappings(
    policy: &mut PresentationConfigFile,
    _defaults: &PresentationConfigFile,
    warnings: &mut Vec<String>,
) {
    let generic_template = policy
        .session
        .unknown_template
        .clone()
        .unwrap_or_else(|| GENERIC_BLOCK_TEMPLATE.to_string());
    policy.family_mappings.retain_mut(|mapping| {
        if mapping.matchers.is_empty() {
            warnings.push("note: family mapping without any match entries was ignored".to_string());
            return false;
        }
        if mapping
            .display_family
            .as_deref()
            .is_none_or(|value| value.trim().is_empty())
        {
            warnings.push(format!(
                "note: family mapping {:?} is missing display_family and was ignored",
                mapping.matchers
            ));
            return false;
        }
        match mapping.template.as_deref() {
            Some(template_id)
                if policy
                    .templates
                    .iter()
                    .any(|template| template.id == template_id) => {}
            Some(template_id) => {
                warnings.push(format!(
                    "note: family mapping {:?} references unknown template '{}'; using '{}'",
                    mapping.matchers, template_id, generic_template
                ));
                mapping.template = Some(generic_template.clone());
            }
            None => {
                mapping.template = Some(generic_template.clone());
            }
        }
        if let Some(shape) = mapping.semantic_shape.as_deref()
            && !KNOWN_SEMANTIC_SHAPES.contains(&shape)
        {
            warnings.push(format!(
                "note: family mapping {:?} references unsupported semantic_shape '{}'; using built-in routing defaults",
                mapping.matchers, shape
            ));
            mapping.semantic_shape = None;
        }
        mapping.shape_fallback.retain(|fallback| {
            if KNOWN_SEMANTIC_SHAPES.contains(&fallback.shape.as_str()) {
                true
            } else {
                warnings.push(format!(
                    "note: family mapping {:?} references unsupported shape_fallback '{}'; skipping fallback",
                    mapping.matchers, fallback.shape
                ));
                false
            }
        });
        true
    });
}

fn template_default_excerpt<'a>(
    defaults: &'a PresentationConfigFile,
    template_id: &str,
) -> Option<&'a str> {
    defaults
        .templates
        .iter()
        .find(|template| template.id == template_id)
        .and_then(|template| template.excerpt.as_deref())
        .or(Some("off"))
}

fn normalize_enum_field(
    field: &mut Option<String>,
    default: Option<&str>,
    allowed: &[&str],
    label: &str,
    warnings: &mut Vec<String>,
) {
    match field.as_deref() {
        Some(value) if allowed.contains(&value) => {}
        Some(value) => {
            warnings.push(format!(
                "note: unsupported {label} '{value}'; using built-in default"
            ));
            *field = default.map(ToOwned::to_owned);
        }
        None => {
            *field = default.map(ToOwned::to_owned);
        }
    }
}

fn apply_builtin_presentation_defaults(policy: &mut PresentationConfigFile, preset_id: &str) {
    if policy.header == PresentationHeaderSection::default() {
        policy.header = builtin_header_defaults(preset_id);
    }
    if policy.labels.label_width_mode.is_none() && policy.location.label_width.is_some() {
        policy.labels.label_width_mode = Some("fixed".to_string());
        policy.labels.fixed_label_width = policy.location.label_width;
    }
}

fn builtin_header_defaults(preset_id: &str) -> PresentationHeaderSection {
    PresentationHeaderSection {
        subject_first: Some(matches!(
            preset_id,
            "subject_blocks_v1" | "subject_blocks_v2"
        )),
        interactive_format: Some("{severity}: [{family}] {subject}".to_string()),
        ci_path_first_format: Some("{location}: {severity}: [{family}] {subject}".to_string()),
        unknown_family: Some("generic".to_string()),
    }
}

fn deserialize_optional_mode<'de, D>(deserializer: D) -> Result<Option<ExecutionMode>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = Option::<String>::deserialize(deserializer)?;
    value
        .map(|value| parse_mode(&value).map_err(serde::de::Error::custom))
        .transpose()
}

fn deserialize_optional_processing_path<'de, D>(
    deserializer: D,
) -> Result<Option<ProcessingPath>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = Option::<String>::deserialize(deserializer)?;
    value
        .map(|value| parse_processing_path(&value).map_err(serde::de::Error::custom))
        .transpose()
}

fn deserialize_optional_profile<'de, D>(deserializer: D) -> Result<Option<RenderProfile>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = Option::<String>::deserialize(deserializer)?;
    value
        .map(|value| parse_profile(&value).map_err(serde::de::Error::custom))
        .transpose()
}

fn deserialize_optional_path_policy<'de, D>(deserializer: D) -> Result<Option<PathPolicy>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = Option::<String>::deserialize(deserializer)?;
    match value.as_deref() {
        None => Ok(None),
        Some("shortest_unambiguous") => Ok(Some(PathPolicy::ShortestUnambiguous)),
        Some("relative_to_cwd") => Ok(Some(PathPolicy::RelativeToCwd)),
        Some("absolute") => Ok(Some(PathPolicy::Absolute)),
        Some(other) => Err(serde::de::Error::custom(format!(
            "unsupported path policy: {other}"
        ))),
    }
}

fn deserialize_optional_debug_refs<'de, D>(deserializer: D) -> Result<Option<DebugRefs>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = Option::<String>::deserialize(deserializer)?;
    value
        .map(|value| parse_debug_refs(&value).map_err(serde::de::Error::custom))
        .transpose()
}

fn deserialize_optional_retention<'de, D>(
    deserializer: D,
) -> Result<Option<RetentionPolicy>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = Option::<String>::deserialize(deserializer)?;
    value
        .map(|value| parse_retention_policy(&value).map_err(serde::de::Error::custom))
        .transpose()
}

fn deserialize_optional_compression_level<'de, D>(
    deserializer: D,
) -> Result<Option<CompressionLevel>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = Option::<String>::deserialize(deserializer)?;
    value
        .map(|value| parse_compression_level(&value).map_err(serde::de::Error::custom))
        .transpose()
}

fn deserialize_optional_suppress_likelihood_threshold<'de, D>(
    deserializer: D,
) -> Result<Option<f32>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    deserialize_optional_probability(deserializer, "cascade suppress threshold")
}

fn deserialize_optional_summary_likelihood_threshold<'de, D>(
    deserializer: D,
) -> Result<Option<f32>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    deserialize_optional_probability(deserializer, "cascade summary threshold")
}

fn deserialize_optional_min_parent_margin<'de, D>(deserializer: D) -> Result<Option<f32>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    deserialize_optional_probability(deserializer, "cascade min parent margin")
}

fn deserialize_optional_probability<'de, D>(
    deserializer: D,
    label: &'static str,
) -> Result<Option<f32>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = Option::<f32>::deserialize(deserializer)?;
    value
        .map(|value| parse_probability(label, &value.to_string()).map_err(serde::de::Error::custom))
        .transpose()
}

fn deserialize_optional_suppressed_count_visibility<'de, D>(
    deserializer: D,
) -> Result<Option<SuppressedCountVisibility>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum VisibilityWire {
        Bool(bool),
        String(String),
    }

    let value = Option::<VisibilityWire>::deserialize(deserializer)?;
    value
        .map(|value| match value {
            VisibilityWire::Bool(true) => Ok(SuppressedCountVisibility::Always),
            VisibilityWire::Bool(false) => Ok(SuppressedCountVisibility::Never),
            VisibilityWire::String(value) => {
                parse_suppressed_count_visibility(&value).map_err(serde::de::Error::custom)
            }
        })
        .transpose()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn overlay_config_overrides_matching_fields_only() {
        let base = ConfigFile {
            schema_version: Some(1),
            backend: BackendSection {
                gcc: Some(PathBuf::from("/usr/bin/gcc")),
                launcher: Some(PathBuf::from("/usr/bin/ccache")),
            },
            runtime: RuntimeSection {
                mode: Some(ExecutionMode::Shadow),
                processing_path: Some(ProcessingPath::NativeTextCapture),
            },
            render: RenderSection {
                profile: Some(RenderProfile::Verbose),
                path_policy: Some(PathPolicy::Absolute),
                debug_refs: Some(DebugRefs::CaptureRef),
                presentation: Some("legacy_v1".to_string()),
                presentation_file: Some(PathBuf::from("/etc/cc-formed/presentation.toml")),
            },
            trace: TraceSection {
                retention_policy: Some(RetentionPolicy::OnChildError),
            },
            cascade: CascadeSection {
                compression_level: Some(CompressionLevel::Conservative),
                suppress_likelihood_threshold: Some(0.71),
                summary_likelihood_threshold: Some(0.41),
                min_parent_margin: Some(0.09),
                max_expanded_independent_roots: Some(2),
                show_suppressed_count: Some(SuppressedCountVisibility::Never),
            },
        };
        let overlay = ConfigFile {
            schema_version: None,
            backend: BackendSection {
                gcc: None,
                launcher: Some(PathBuf::from("/usr/bin/distcc")),
            },
            runtime: RuntimeSection {
                mode: Some(ExecutionMode::Render),
                processing_path: Some(ProcessingPath::SingleSinkStructured),
            },
            render: RenderSection {
                profile: None,
                path_policy: Some(PathPolicy::RelativeToCwd),
                debug_refs: None,
                presentation: Some("subject_blocks_v1".to_string()),
                presentation_file: None,
            },
            trace: TraceSection {
                retention_policy: Some(RetentionPolicy::Always),
            },
            cascade: CascadeSection {
                compression_level: Some(CompressionLevel::Aggressive),
                suppress_likelihood_threshold: None,
                summary_likelihood_threshold: Some(0.63),
                min_parent_margin: None,
                max_expanded_independent_roots: Some(4),
                show_suppressed_count: Some(SuppressedCountVisibility::Always),
            },
        };

        let merged = merge_config(base, overlay);
        assert_eq!(merged.schema_version, Some(1));
        assert_eq!(merged.backend.gcc, Some(PathBuf::from("/usr/bin/gcc")));
        assert_eq!(
            merged.backend.launcher,
            Some(PathBuf::from("/usr/bin/distcc"))
        );
        assert_eq!(merged.runtime.mode, Some(ExecutionMode::Render));
        assert_eq!(
            merged.runtime.processing_path,
            Some(ProcessingPath::SingleSinkStructured)
        );
        assert_eq!(merged.render.profile, Some(RenderProfile::Verbose));
        assert_eq!(merged.render.path_policy, Some(PathPolicy::RelativeToCwd));
        assert_eq!(merged.render.debug_refs, Some(DebugRefs::CaptureRef));
        assert_eq!(
            merged.render.presentation.as_deref(),
            Some("subject_blocks_v1")
        );
        assert_eq!(
            merged.render.presentation_file,
            Some(PathBuf::from("/etc/cc-formed/presentation.toml"))
        );
        assert_eq!(merged.trace.retention_policy, Some(RetentionPolicy::Always));
        assert_eq!(
            merged.cascade.compression_level,
            Some(CompressionLevel::Aggressive)
        );
        assert_eq!(merged.cascade.suppress_likelihood_threshold, Some(0.71));
        assert_eq!(merged.cascade.summary_likelihood_threshold, Some(0.63));
        assert_eq!(merged.cascade.min_parent_margin, Some(0.09));
        assert_eq!(merged.cascade.max_expanded_independent_roots, Some(4));
        assert_eq!(
            merged.cascade.show_suppressed_count,
            Some(SuppressedCountVisibility::Always)
        );
    }

    #[test]
    fn load_uses_first_existing_admin_config_candidate() {
        let temp = tempdir().unwrap();
        let missing_admin = temp
            .path()
            .join("etc-a")
            .join("cc-formed")
            .join("config.toml");
        let fallback_admin = temp
            .path()
            .join("etc-b")
            .join("cc-formed")
            .join("config.toml");
        fs::create_dir_all(fallback_admin.parent().unwrap()).unwrap();
        fs::write(
            &fallback_admin,
            r#"
                [backend]
                gcc = "/opt/gcc-from-second"
                launcher = "/opt/ccache-from-second"
            "#,
        )
        .unwrap();

        let loaded =
            ConfigFile::load_from_paths([missing_admin, fallback_admin], Option::<&Path>::None)
                .unwrap();

        assert_eq!(
            loaded.backend.gcc,
            Some(PathBuf::from("/opt/gcc-from-second"))
        );
        assert_eq!(
            loaded.backend.launcher,
            Some(PathBuf::from("/opt/ccache-from-second"))
        );
    }

    #[test]
    fn user_config_overrides_first_existing_admin_config() {
        let temp = tempdir().unwrap();
        let admin = temp
            .path()
            .join("etc")
            .join("cc-formed")
            .join("config.toml");
        let user = temp.path().join("user-config.toml");
        fs::create_dir_all(admin.parent().unwrap()).unwrap();
        fs::write(
            &admin,
            r#"
                [backend]
                gcc = "/opt/gcc-from-admin"
                launcher = "/opt/ccache-from-admin"

                [runtime]
                mode = "shadow"
            "#,
        )
        .unwrap();
        fs::write(
            &user,
            r#"
                [runtime]
                mode = "render"

                [render]
                presentation = "subject_blocks_v1"

                [cascade]
                summary_likelihood_threshold = 0.62
                show_suppressed_count = true
            "#,
        )
        .unwrap();

        let loaded = ConfigFile::load_from_paths([admin], Some(&user)).unwrap();

        assert_eq!(
            loaded.backend.gcc,
            Some(PathBuf::from("/opt/gcc-from-admin"))
        );
        assert_eq!(
            loaded.backend.launcher,
            Some(PathBuf::from("/opt/ccache-from-admin"))
        );
        assert_eq!(loaded.runtime.mode, Some(ExecutionMode::Render));
        assert_eq!(
            loaded.render.presentation.as_deref(),
            Some("subject_blocks_v1")
        );
        assert_eq!(loaded.cascade.summary_likelihood_threshold, Some(0.62));
        assert_eq!(
            loaded.cascade.show_suppressed_count,
            Some(SuppressedCountVisibility::Always)
        );
    }

    #[test]
    fn admin_config_paths_fall_back_to_default_when_xdg_config_dirs_is_empty() {
        let paths = admin_config_paths_from(Some(OsString::new()));

        assert_eq!(paths, vec![PathBuf::from("/etc/xdg/cc-formed/config.toml")]);
    }

    #[test]
    fn admin_config_paths_ignore_empty_candidates_in_xdg_config_dirs() {
        let separator = if cfg!(windows) { ";" } else { ":" };
        let raw = OsString::from(format!(
            "{separator}/opt/xdg-a{separator}/opt/xdg-b{separator}"
        ));

        let paths = admin_config_paths_from(Some(raw));

        assert_eq!(
            paths,
            vec![
                PathBuf::from("/opt/xdg-a/cc-formed/config.toml"),
                PathBuf::from("/opt/xdg-b/cc-formed/config.toml"),
            ]
        );
    }

    #[test]
    fn resolve_cascade_policy_respects_cli_then_user_then_admin_then_defaults() {
        let temp = tempdir().unwrap();
        let admin = temp
            .path()
            .join("etc")
            .join("cc-formed")
            .join("config.toml");
        let user = temp.path().join("user-config.toml");
        fs::create_dir_all(admin.parent().unwrap()).unwrap();
        fs::write(
            &admin,
            r#"
                [cascade]
                compression_level = "conservative"
                suppress_likelihood_threshold = 0.70
                summary_likelihood_threshold = 0.40
                min_parent_margin = 0.08
                max_expanded_independent_roots = 1
                show_suppressed_count = false
            "#,
        )
        .unwrap();
        fs::write(
            &user,
            r#"
                [cascade]
                compression_level = "balanced"
                suppress_likelihood_threshold = 0.76
                summary_likelihood_threshold = 0.52
            "#,
        )
        .unwrap();
        let config = ConfigFile::load_from_paths([admin], Some(&user)).unwrap();
        let parsed = ParsedArgs::parse(vec![
            OsString::from("gcc-formed"),
            OsString::from("--formed-cascade-level=off"),
            OsString::from("--formed-cascade-summary-threshold=0.91"),
            OsString::from("--formed-cascade-show-suppressed-count=never"),
        ])
        .unwrap();

        let resolved = config.resolve_cascade_policy(&parsed);

        assert_eq!(resolved.policy.compression_level, CompressionLevel::Off);
        assert_eq!(resolved.policy.suppress_likelihood_threshold, 0.76);
        assert_eq!(resolved.policy.summary_likelihood_threshold, 0.91);
        assert_eq!(resolved.policy.min_parent_margin, 0.08);
        assert_eq!(resolved.policy.max_expanded_independent_roots, 1);
        assert_eq!(
            resolved.policy.show_suppressed_count,
            SuppressedCountVisibility::Never
        );
        assert_eq!(resolved.warnings.len(), 1);
        assert!(resolved.warnings[0].contains("deprecated"));
    }

    #[test]
    fn resolve_cascade_policy_uses_built_in_defaults_when_unset() {
        let resolved = ConfigFile::default().resolve_cascade_policy(&ParsedArgs::default());

        assert_eq!(resolved.policy, CascadePolicySnapshot::default());
        assert!(resolved.warnings.is_empty());
    }

    #[test]
    fn resolve_presentation_policy_respects_cli_then_user_then_admin_then_defaults() {
        let temp = tempdir().unwrap();
        let admin = temp
            .path()
            .join("etc")
            .join("cc-formed")
            .join("config.toml");
        let user = temp.path().join("user-config.toml");
        let admin_overlay = temp
            .path()
            .join("etc")
            .join("cc-formed")
            .join("admin-presentation.toml");
        let user_overlay = temp.path().join("user-presentation.toml");
        fs::create_dir_all(admin.parent().unwrap()).unwrap();
        fs::write(
            &admin,
            r#"
                [render]
                presentation = "legacy_v1"
                presentation_file = "admin-presentation.toml"
            "#,
        )
        .unwrap();
        fs::write(
            &user,
            r#"
                [render]
                presentation = "subject_blocks_v1"
                presentation_file = "user-presentation.toml"
            "#,
        )
        .unwrap();
        fs::write(
            &admin_overlay,
            r#"
                kind = "cc_formed_presentation"
                schema_version = 1

                [labels]
                help = "admin-help"
            "#,
        )
        .unwrap();
        fs::write(
            &user_overlay,
            r#"
                kind = "cc_formed_presentation"
                schema_version = 1

                [labels]
                help = "user-help"
            "#,
        )
        .unwrap();

        let config = ConfigFile::load_from_paths([admin], Some(&user)).unwrap();
        let parsed = ParsedArgs::parse(vec![
            OsString::from("gcc-formed"),
            OsString::from("--formed-presentation=legacy_v1"),
        ])
        .unwrap();

        let resolved = config.resolve_presentation_policy(&parsed);

        assert_eq!(resolved.preset_id, "legacy_v1");
        assert_eq!(resolved.presentation_file, Some(user_overlay));
        assert_eq!(
            resolved
                .policy
                .labels
                .values
                .get("help")
                .map(String::as_str),
            Some("user-help")
        );
        assert!(!resolved.fell_back_to_default);
        assert!(resolved.warnings.is_empty());
    }

    #[test]
    fn relative_presentation_file_paths_are_resolved_from_config_location() {
        let temp = tempdir().unwrap();
        let user = temp.path().join("nested").join("config.toml");
        let overlay = temp.path().join("nested").join("presentation.toml");
        fs::create_dir_all(user.parent().unwrap()).unwrap();
        fs::write(
            &user,
            r#"
                [render]
                presentation_file = "presentation.toml"
            "#,
        )
        .unwrap();
        fs::write(
            &overlay,
            r#"
                kind = "cc_formed_presentation"
                schema_version = 1
            "#,
        )
        .unwrap();

        let config = ConfigFile::load_from_paths([], Some(&user)).unwrap();
        let resolved = config.resolve_presentation_policy(&ParsedArgs::default());

        assert_eq!(resolved.presentation_file, Some(overlay));
    }

    #[test]
    fn invalid_external_presentation_file_falls_back_to_default_preset() {
        let temp = tempdir().unwrap();
        let user = temp.path().join("config.toml");
        let overlay = temp.path().join("broken-presentation.toml");
        fs::write(
            &user,
            r#"
                [render]
                presentation = "subject_blocks_v1"
                presentation_file = "broken-presentation.toml"
            "#,
        )
        .unwrap();
        fs::write(&overlay, "not = [valid").unwrap();

        let config = ConfigFile::load_from_paths([], Some(&user)).unwrap();
        let resolved = config.resolve_presentation_policy(&ParsedArgs::default());

        assert_eq!(resolved.preset_id, "subject_blocks_v1");
        assert!(resolved.fell_back_to_default);
        assert!(
            resolved
                .warnings
                .iter()
                .any(|warning| warning.contains("failed to load presentation file"))
        );
        assert_eq!(
            resolved.policy.session.visible_root_mode.as_deref(),
            Some("all_visible_blocks")
        );
        assert_eq!(resolved.policy.header.subject_first, Some(true));
    }

    #[test]
    fn unknown_template_and_slot_degrade_with_warnings() {
        let temp = tempdir().unwrap();
        let overlay = temp.path().join("presentation.toml");
        fs::write(
            &overlay,
            r#"
                kind = "cc_formed_presentation"
                schema_version = 1

                [[templates]]
                id = "contrast_block"
                excerpt = "off"
                core = [
                  { slot = "unknown_slot", label = "help" },
                  { slot = "expected", label = "want", suffix_slot = "unknown_suffix" },
                ]

                [[family_mappings]]
                match = ["type_overload"]
                display_family = "type_mismatch"
                template = "missing_block"
            "#,
        )
        .unwrap();

        let config = ConfigFile {
            render: RenderSection {
                presentation_file: Some(overlay),
                ..RenderSection::default()
            },
            ..ConfigFile::default()
        };

        let resolved = config.resolve_presentation_policy(&ParsedArgs::default());
        let contrast_block = resolved
            .policy
            .templates
            .iter()
            .find(|template| template.id == "contrast_block")
            .unwrap();
        let mapping = resolved
            .policy
            .family_mappings
            .iter()
            .find(|mapping| mapping.matchers == vec!["type_overload".to_string()])
            .unwrap();

        assert_eq!(contrast_block.core.len(), 1);
        assert_eq!(contrast_block.core[0].slot, "expected");
        assert_eq!(contrast_block.core[0].suffix_slot, None);
        assert_eq!(mapping.template.as_deref(), Some("generic_block"));
        assert!(
            resolved
                .warnings
                .iter()
                .any(|warning| warning.contains("unknown slot"))
        );
        assert!(
            resolved
                .warnings
                .iter()
                .any(|warning| warning.contains("unknown template"))
        );
    }

    #[test]
    fn existing_config_without_presentation_keys_uses_subject_blocks_default() {
        let resolved = ConfigFile::default().resolve_presentation_policy(&ParsedArgs::default());

        assert_eq!(resolved.preset_id, "subject_blocks_v1");
        assert!(!resolved.fell_back_to_default);
        assert!(resolved.warnings.is_empty());
    }

    #[test]
    fn resolved_presentation_policy_converts_to_subject_blocks_render_policy() {
        let resolved = ConfigFile::default().resolve_presentation_policy(&ParsedArgs::default());

        let render_policy = resolved.to_render_policy();

        assert_eq!(render_policy.preset_id, "subject_blocks_v1");
        assert_eq!(render_policy.session_mode, SessionMode::AllVisibleBlocks);
        assert!(render_policy.header.subject_first);
        assert_eq!(render_policy.default_template_id, "generic_block");
        assert_eq!(render_policy.generic_template_id, "generic_block");
        assert_eq!(render_policy.label("why_raw"), Some("raw"));
        assert_eq!(
            render_policy.slot_label(SemanticSlotId::WhyRaw),
            Some("raw")
        );
        let preprocessor = render_policy
            .family_mappings
            .iter()
            .find(|mapping| mapping.matcher == "preprocessor_directive")
            .unwrap();
        assert_eq!(preprocessor.semantic_shape, Some(SemanticShape::Parser));
        assert_eq!(preprocessor.shape_fallbacks.len(), 1);
        assert_eq!(
            preprocessor.shape_fallbacks[0].shape,
            SemanticShape::MissingHeader
        );
    }

    #[test]
    fn legacy_render_policy_conversion_keeps_legacy_default_template() {
        let parsed = ParsedArgs::parse(vec![
            OsString::from("gcc-formed"),
            OsString::from("--formed-presentation=legacy_v1"),
        ])
        .unwrap();

        let render_policy = ConfigFile::default()
            .resolve_presentation_policy(&parsed)
            .to_render_policy();

        assert_eq!(render_policy.preset_id, "legacy_v1");
        assert_eq!(render_policy.session_mode, SessionMode::LeadPlusSummary);
        assert!(!render_policy.header.subject_first);
        assert_eq!(render_policy.default_template_id, "legacy_primary_block");
        assert_eq!(
            render_policy.slot_label(SemanticSlotId::WhyRaw),
            Some("why")
        );
    }

    #[test]
    fn subject_blocks_v2_builtin_preset_is_available() {
        let parsed = ParsedArgs::parse(vec![
            OsString::from("gcc-formed"),
            OsString::from("--formed-presentation=subject_blocks_v2"),
        ])
        .unwrap();

        let render_policy = ConfigFile::default()
            .resolve_presentation_policy(&parsed)
            .to_render_policy();

        assert_eq!(render_policy.preset_id, "subject_blocks_v2");
        assert!(render_policy.header.subject_first);
        assert_eq!(
            render_policy.header.interactive_format,
            "{severity}: [{family}] {subject}"
        );
        assert_eq!(
            render_policy.header.ci_path_first_format,
            "{location}: {severity}: [{family}] {subject}"
        );
        assert_eq!(render_policy.label("raw"), Some("raw"));
    }

    #[test]
    fn external_v2_presentation_file_is_loaded_and_subject_first_is_config_driven() {
        let temp = tempdir().unwrap();
        let overlay = temp.path().join("presentation-v2.toml");
        fs::write(
            &overlay,
            r#"
                kind = "cc_formed_presentation"
                schema_version = 2

                [header]
                subject_first = false
                interactive_format = "{severity}: {subject}"

                [labels]
                label_width_mode = "fixed"
                fixed_label_width = 6

                [[family_mappings]]
                match = ["type_overload"]
                display_family = "type_mismatch"
                semantic_shape = "contrast"
                template = "contrast_block"
            "#,
        )
        .unwrap();

        let config = ConfigFile {
            render: RenderSection {
                presentation_file: Some(overlay),
                ..RenderSection::default()
            },
            ..ConfigFile::default()
        };

        let resolved = config.resolve_presentation_policy(&ParsedArgs::default());
        let render_policy = resolved.to_render_policy();
        let card = render_policy.resolve_card_presentation(Some("type_overload"));

        assert!(!render_policy.header.subject_first);
        assert_eq!(
            render_policy.header.interactive_format,
            "{severity}: {subject}"
        );
        assert_eq!(
            resolved.policy.labels.label_width_mode.as_deref(),
            Some("fixed")
        );
        assert_eq!(resolved.policy.labels.fixed_label_width, Some(6));
        assert!(!card.subject_first_header);
        assert_eq!(card.semantic_shape, SemanticShape::Contrast);
    }

    #[test]
    fn external_v1_presentation_file_keeps_subject_first_default_from_builtin_preset() {
        let temp = tempdir().unwrap();
        let overlay = temp.path().join("presentation-v1.toml");
        fs::write(
            &overlay,
            r#"
                kind = "cc_formed_presentation"
                schema_version = 1

                [labels]
                help = "custom-help"
            "#,
        )
        .unwrap();

        let config = ConfigFile {
            render: RenderSection {
                presentation_file: Some(overlay),
                ..RenderSection::default()
            },
            ..ConfigFile::default()
        };

        let render_policy = config
            .resolve_presentation_policy(&ParsedArgs::default())
            .to_render_policy();

        assert!(render_policy.header.subject_first);
        assert_eq!(
            render_policy
                .resolve_card_presentation(Some("syntax"))
                .subject_first_header,
            true
        );
        assert_eq!(render_policy.label("help"), Some("custom-help"));
    }

    #[test]
    fn schema_version_errors_are_human_readable() {
        let error = parse_presentation_config(
            r#"
                kind = "cc_formed_presentation"
                schema_version = 9
            "#,
            "fixture.toml",
        )
        .unwrap_err();

        assert!(error.contains("unsupported presentation schema_version"));
        assert!(error.contains("supported: 1 and 2"));
    }
}
