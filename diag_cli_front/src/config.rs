use crate::args::{
    ParsedArgs, parse_compression_level, parse_debug_refs, parse_mode, parse_probability,
    parse_processing_path, parse_profile, parse_retention_policy,
    parse_suppressed_count_visibility,
};
use crate::error::CliError;
use diag_backend_probe::ProcessingPath;
use diag_capture_runtime::ExecutionMode;
use diag_core::{CascadePolicySnapshot, CompressionLevel, SuppressedCountVisibility};
use diag_render::{DebugRefs, PathPolicy, RenderProfile};
use diag_trace::{RetentionPolicy, WrapperPaths};
use serde::Deserialize;
use std::collections::BTreeMap;
use std::env;
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};

const DEFAULT_PRESENTATION_PRESET: &str = "legacy_v1";
const PRESENTATION_SCHEMA_KIND: &str = "cc_formed_presentation";
const PRESENTATION_SCHEMA_VERSION: u32 = 1;
const GENERIC_BLOCK_TEMPLATE: &str = "generic_block";
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
    "expected",
    "actual",
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
];
const KNOWN_SUFFIX_SLOTS: &[&str] = &["omitted_notes_suffix", "omitted_refs_suffix"];

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
pub(crate) struct PresentationConfigFile {
    pub(crate) kind: Option<String>,
    pub(crate) schema_version: Option<u32>,
    #[serde(default)]
    pub(crate) session: PresentationSessionSection,
    #[serde(default)]
    pub(crate) labels: BTreeMap<String, String>,
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
pub(crate) struct PresentationLocationSection {
    #[serde(default)]
    pub(crate) default_placement: Option<String>,
    #[serde(default)]
    pub(crate) fallback_order: Option<Vec<String>>,
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

    pub(crate) fn resolve_cascade_policy(&self, parsed: &ParsedArgs) -> CascadePolicySnapshot {
        let defaults = CascadePolicySnapshot::default();
        CascadePolicySnapshot {
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
        }
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
        "subject_blocks_v1" => SUBJECT_BLOCKS_V1_ASSET,
        "legacy_v1" => LEGACY_V1_ASSET,
        other => return Err(format!("unknown preset id: {other}")),
    };
    parse_presentation_config(source, &format!("built-in preset '{preset_id}'"))
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
        Some(PRESENTATION_SCHEMA_VERSION) => {}
        Some(other) => {
            return Err(format!(
                "unsupported presentation schema_version in {source}: {other}"
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
    for (key, value) in overlay.labels {
        base.labels.insert(key, value);
    }
    base.location.default_placement = overlay
        .location
        .default_placement
        .or(base.location.default_placement);
    base.location.fallback_order = overlay
        .location
        .fallback_order
        .or(base.location.fallback_order);
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

fn normalize_presentation_config(
    policy: &mut PresentationConfigFile,
    defaults: &PresentationConfigFile,
    warnings: &mut Vec<String>,
) {
    policy.kind = Some(PRESENTATION_SCHEMA_KIND.to_string());
    policy.schema_version = Some(PRESENTATION_SCHEMA_VERSION);
    normalize_session(policy, defaults, warnings);
    normalize_location(policy, defaults, warnings);
    normalize_templates(policy, defaults, warnings);
    normalize_family_mappings(policy, defaults, warnings);
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

        assert_eq!(resolved.compression_level, CompressionLevel::Off);
        assert_eq!(resolved.suppress_likelihood_threshold, 0.76);
        assert_eq!(resolved.summary_likelihood_threshold, 0.91);
        assert_eq!(resolved.min_parent_margin, 0.08);
        assert_eq!(resolved.max_expanded_independent_roots, 1);
        assert_eq!(
            resolved.show_suppressed_count,
            SuppressedCountVisibility::Never
        );
    }

    #[test]
    fn resolve_cascade_policy_uses_built_in_defaults_when_unset() {
        let resolved = ConfigFile::default().resolve_cascade_policy(&ParsedArgs::default());

        assert_eq!(resolved, CascadePolicySnapshot::default());
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
            resolved.policy.labels.get("help").map(String::as_str),
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

        assert_eq!(resolved.preset_id, "legacy_v1");
        assert!(resolved.fell_back_to_default);
        assert!(
            resolved
                .warnings
                .iter()
                .any(|warning| warning.contains("failed to load presentation file"))
        );
        assert_eq!(
            resolved.policy.session.visible_root_mode.as_deref(),
            Some("lead_plus_summary")
        );
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
    fn existing_config_without_presentation_keys_keeps_legacy_default() {
        let resolved = ConfigFile::default().resolve_presentation_policy(&ParsedArgs::default());

        assert_eq!(resolved.preset_id, "legacy_v1");
        assert!(!resolved.fell_back_to_default);
        assert!(resolved.warnings.is_empty());
    }
}
