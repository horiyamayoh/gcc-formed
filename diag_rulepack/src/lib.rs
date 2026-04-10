use diag_core::Phase;
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::fs;
use std::path::{Component, Path, PathBuf};

pub const RULEPACK_MANIFEST_SCHEMA_VERSION: &str = "diag_rulepack_manifest/v1alpha1";
pub const RULEPACK_SECTION_SCHEMA_VERSION: &str = "diag_rulepack_section/v1alpha1";
pub const CHECKED_IN_RULEPACK_VERSION: &str = "phase1";
pub const CHECKED_IN_MANIFEST_FILE: &str = "diag_rulepack.manifest.phase1.json";
#[cfg(test)]
const CHECKED_IN_SECTION_FILES: &[&str] = &[
    CHECKED_IN_MANIFEST_FILE,
    "diag_rulepack.enrich_family.phase1.json",
    "diag_rulepack.enrich_wording.phase1.json",
    "diag_rulepack.residual.phase1.json",
];

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RulepackManifest {
    pub schema_version: String,
    pub rulepack_version: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub sections: Vec<ManifestSection>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ManifestSection {
    pub kind: SectionKind,
    pub path: String,
    pub sha256: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum SectionKind {
    EnrichFamily,
    EnrichWording,
    Residual,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MatchStrategy {
    StructuredOrMessage,
    PhaseOrMessage,
    SemanticRole,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TextStrategy {
    FixedText,
    MessagePassthrough,
    FirstMessageLine,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EnrichFamilySection {
    pub schema_version: String,
    pub rulepack_version: String,
    pub kind: SectionKind,
    pub fallback_family: String,
    pub fallback_rule_id: String,
    pub ingress_specific_override_rule_id: String,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub ingress_specific_family_overrides: Vec<String>,
    pub rules: Vec<EnrichFamilyRule>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EnrichFamilyRule {
    pub rule_id: String,
    pub family: String,
    pub match_strategy: MatchStrategy,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EnrichWordingSection {
    pub schema_version: String,
    pub rulepack_version: String,
    pub kind: SectionKind,
    pub fallback: FallbackWording,
    pub headlines: Vec<FamilyText>,
    pub action_hints: Vec<FamilyText>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub specific_overrides: Vec<SpecificWordingOverride>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FamilyText {
    pub family: String,
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FallbackWording {
    pub headline_strategy: TextStrategy,
    pub first_action_hint: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SpecificWordingOverride {
    pub family: String,
    pub headline_template: String,
    pub first_action_hint: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ResidualSection {
    pub schema_version: String,
    pub rulepack_version: String,
    pub kind: SectionKind,
    pub compiler_groups: Vec<CompilerResidualSeed>,
    pub linker_groups: Vec<LinkerResidualSeed>,
    pub passthrough: PassthroughResidualSeed,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum CompilerResidualKind {
    Syntax,
    Template,
    TypeOverload,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CompilerResidualSeed {
    pub kind: CompilerResidualKind,
    pub family: String,
    pub phase: Phase,
    pub rule_id: String,
    pub headline_strategy: TextStrategy,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub headline: Option<String>,
    pub first_action_hint: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum LinkerResidualKind {
    AssemblerError,
    CannotFindLibrary,
    FileFormatOrRelocation,
    MultipleDefinition,
    UndefinedReference,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LinkerResidualSeed {
    pub kind: LinkerResidualKind,
    pub family: String,
    pub phase: Phase,
    pub rule_id: String,
    pub headline_template: String,
    pub first_action_hint: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PassthroughResidualSeed {
    pub family: String,
    pub phase: Phase,
    pub rule_id: String,
    pub headline: String,
    pub first_action_hint: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoadedRulepack {
    manifest: RulepackManifest,
    enrich_family: EnrichFamilySection,
    enrich_wording: EnrichWordingSection,
    residual: ResidualSection,
}

impl LoadedRulepack {
    pub fn version(&self) -> &str {
        &self.manifest.rulepack_version
    }

    pub fn manifest(&self) -> &RulepackManifest {
        &self.manifest
    }

    pub fn enrich_family_section(&self) -> &EnrichFamilySection {
        &self.enrich_family
    }

    pub fn enrich_wording_section(&self) -> &EnrichWordingSection {
        &self.enrich_wording
    }

    pub fn residual_section(&self) -> &ResidualSection {
        &self.residual
    }

    pub fn enrich_rule(&self, family: &str) -> Option<&EnrichFamilyRule> {
        self.enrich_family
            .rules
            .iter()
            .find(|rule| rule.family == family)
    }

    pub fn generic_headline(&self, family: &str) -> Option<&str> {
        self.enrich_wording
            .headlines
            .iter()
            .find(|entry| entry.family == family)
            .map(|entry| entry.text.as_str())
    }

    pub fn generic_action_hint(&self, family: &str) -> Option<&str> {
        self.enrich_wording
            .action_hints
            .iter()
            .find(|entry| entry.family == family)
            .map(|entry| entry.text.as_str())
    }

    pub fn specific_wording_override(&self, family: &str) -> Option<&SpecificWordingOverride> {
        self.enrich_wording
            .specific_overrides
            .iter()
            .find(|entry| entry.family == family)
    }

    pub fn compiler_residual_seed(
        &self,
        kind: CompilerResidualKind,
    ) -> Option<&CompilerResidualSeed> {
        self.residual
            .compiler_groups
            .iter()
            .find(|entry| entry.kind == kind)
    }

    pub fn linker_residual_seed(&self, kind: LinkerResidualKind) -> Option<&LinkerResidualSeed> {
        self.residual
            .linker_groups
            .iter()
            .find(|entry| entry.kind == kind)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum RulepackError {
    #[error("failed to read {path}: {source}")]
    ReadFile {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("failed to parse JSON in {path}: {source}")]
    ParseJson {
        path: PathBuf,
        #[source]
        source: serde_json::Error,
    },
    #[error("rulepack digest mismatch for {path}: expected {expected}, got {actual}")]
    DigestMismatch {
        path: PathBuf,
        expected: String,
        actual: String,
    },
    #[error("invalid rulepack at {path}: {message}")]
    InvalidRulepack { path: PathBuf, message: String },
}

pub fn checked_in_rules_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("workspace root")
        .join("rules")
}

pub fn checked_in_manifest_path() -> PathBuf {
    checked_in_rules_dir().join(CHECKED_IN_MANIFEST_FILE)
}

pub fn load_checked_in_rulepack() -> Result<LoadedRulepack, RulepackError> {
    load_rulepack_from_manifest(checked_in_manifest_path())
}

pub fn load_rulepack_from_manifest(
    manifest_path: impl AsRef<Path>,
) -> Result<LoadedRulepack, RulepackError> {
    let manifest_path = manifest_path.as_ref().to_path_buf();
    let manifest_raw = read_raw_file(&manifest_path)?;
    let manifest: RulepackManifest = parse_json(&manifest_path, &manifest_raw)?;
    validate_manifest(&manifest, &manifest_path)?;

    let manifest_dir = manifest_path
        .parent()
        .ok_or_else(|| invalid_rulepack(&manifest_path, "manifest path has no parent"))?;

    let mut enrich_family = None;
    let mut enrich_wording = None;
    let mut residual = None;

    for section in &manifest.sections {
        let section_path = manifest_dir.join(&section.path);
        let section_raw = read_raw_file(&section_path)?;
        let actual_digest = hex_sha256(&section_raw);
        if actual_digest != section.sha256 {
            return Err(RulepackError::DigestMismatch {
                path: section_path,
                expected: section.sha256.clone(),
                actual: actual_digest,
            });
        }

        match section.kind {
            SectionKind::EnrichFamily => {
                let parsed: EnrichFamilySection = parse_json(&section_path, &section_raw)?;
                validate_enrich_family_section(&parsed, &section_path, &manifest.rulepack_version)?;
                enrich_family = Some(parsed);
            }
            SectionKind::EnrichWording => {
                let parsed: EnrichWordingSection = parse_json(&section_path, &section_raw)?;
                validate_enrich_wording_section(
                    &parsed,
                    &section_path,
                    &manifest.rulepack_version,
                )?;
                enrich_wording = Some(parsed);
            }
            SectionKind::Residual => {
                let parsed: ResidualSection = parse_json(&section_path, &section_raw)?;
                validate_residual_section(&parsed, &section_path, &manifest.rulepack_version)?;
                residual = Some(parsed);
            }
        }
    }

    Ok(LoadedRulepack {
        manifest,
        enrich_family: enrich_family.ok_or_else(|| {
            invalid_rulepack(
                &manifest_path,
                "manifest did not resolve an enrich_family section",
            )
        })?,
        enrich_wording: enrich_wording.ok_or_else(|| {
            invalid_rulepack(
                &manifest_path,
                "manifest did not resolve an enrich_wording section",
            )
        })?,
        residual: residual.ok_or_else(|| {
            invalid_rulepack(
                &manifest_path,
                "manifest did not resolve a residual section",
            )
        })?,
    })
}

fn read_raw_file(path: &Path) -> Result<Vec<u8>, RulepackError> {
    fs::read(path).map_err(|source| RulepackError::ReadFile {
        path: path.to_path_buf(),
        source,
    })
}

fn parse_json<T: DeserializeOwned>(path: &Path, raw: &[u8]) -> Result<T, RulepackError> {
    serde_json::from_slice(raw).map_err(|source| RulepackError::ParseJson {
        path: path.to_path_buf(),
        source,
    })
}

fn validate_manifest(manifest: &RulepackManifest, path: &Path) -> Result<(), RulepackError> {
    ensure_schema_version(
        &manifest.schema_version,
        RULEPACK_MANIFEST_SCHEMA_VERSION,
        path,
    )?;
    ensure_version_id(&manifest.rulepack_version, path, "rulepack_version")?;
    if manifest.sections.is_empty() {
        return Err(invalid_rulepack(
            path,
            "manifest must include at least one section",
        ));
    }
    ensure_sections_sorted_and_unique(&manifest.sections, path)?;
    for section in &manifest.sections {
        ensure_relative_json_path(&section.path, path)?;
        ensure_sha256_hex(&section.sha256, path, &section.path)?;
    }
    Ok(())
}

fn validate_enrich_family_section(
    section: &EnrichFamilySection,
    path: &Path,
    expected_version: &str,
) -> Result<(), RulepackError> {
    validate_section_header(
        &section.schema_version,
        &section.rulepack_version,
        section.kind,
        SectionKind::EnrichFamily,
        path,
        expected_version,
    )?;
    ensure_non_empty(&section.fallback_family, path, "fallback_family")?;
    ensure_non_empty(&section.fallback_rule_id, path, "fallback_rule_id")?;
    ensure_non_empty(
        &section.ingress_specific_override_rule_id,
        path,
        "ingress_specific_override_rule_id",
    )?;
    if section.rules.is_empty() {
        return Err(invalid_rulepack(
            path,
            "enrich_family rules must be non-empty",
        ));
    }
    ensure_strings_sorted_and_unique(
        &section.ingress_specific_family_overrides,
        path,
        "ingress_specific_family_overrides",
    )?;
    ensure_vec_sorted_and_unique(&section.rules, path, "enrich_family rules", |rule| {
        rule.rule_id.as_str()
    })?;
    for rule in &section.rules {
        ensure_non_empty(&rule.rule_id, path, "rule.rule_id")?;
        ensure_non_empty(&rule.family, path, "rule.family")?;
    }
    Ok(())
}

fn validate_enrich_wording_section(
    section: &EnrichWordingSection,
    path: &Path,
    expected_version: &str,
) -> Result<(), RulepackError> {
    validate_section_header(
        &section.schema_version,
        &section.rulepack_version,
        section.kind,
        SectionKind::EnrichWording,
        path,
        expected_version,
    )?;
    ensure_non_empty(
        &section.fallback.first_action_hint,
        path,
        "fallback.first_action_hint",
    )?;
    ensure_vec_sorted_and_unique(&section.headlines, path, "headlines", |entry| {
        entry.family.as_str()
    })?;
    ensure_vec_sorted_and_unique(&section.action_hints, path, "action_hints", |entry| {
        entry.family.as_str()
    })?;
    ensure_vec_sorted_and_unique(
        &section.specific_overrides,
        path,
        "specific_overrides",
        |entry| entry.family.as_str(),
    )?;
    let headline_families: BTreeSet<_> = section
        .headlines
        .iter()
        .map(|entry| entry.family.as_str())
        .collect();
    let action_families: BTreeSet<_> = section
        .action_hints
        .iter()
        .map(|entry| entry.family.as_str())
        .collect();
    if headline_families != action_families {
        return Err(invalid_rulepack(
            path,
            "headlines and action_hints must cover the same family set",
        ));
    }
    for entry in &section.headlines {
        ensure_non_empty(&entry.family, path, "headline.family")?;
        ensure_non_empty(&entry.text, path, "headline.text")?;
    }
    for entry in &section.action_hints {
        ensure_non_empty(&entry.family, path, "action_hint.family")?;
        ensure_non_empty(&entry.text, path, "action_hint.text")?;
    }
    for entry in &section.specific_overrides {
        ensure_non_empty(&entry.family, path, "specific_override.family")?;
        ensure_non_empty(
            &entry.headline_template,
            path,
            "specific_override.headline_template",
        )?;
        ensure_non_empty(
            &entry.first_action_hint,
            path,
            "specific_override.first_action_hint",
        )?;
    }
    Ok(())
}

fn validate_residual_section(
    section: &ResidualSection,
    path: &Path,
    expected_version: &str,
) -> Result<(), RulepackError> {
    validate_section_header(
        &section.schema_version,
        &section.rulepack_version,
        section.kind,
        SectionKind::Residual,
        path,
        expected_version,
    )?;
    ensure_vec_sorted_and_unique(&section.compiler_groups, path, "compiler_groups", |entry| {
        compiler_residual_kind_key(entry.kind)
    })?;
    ensure_vec_sorted_and_unique(&section.linker_groups, path, "linker_groups", |entry| {
        linker_residual_kind_key(entry.kind)
    })?;
    if section.compiler_groups.is_empty() || section.linker_groups.is_empty() {
        return Err(invalid_rulepack(
            path,
            "residual section must include compiler_groups and linker_groups",
        ));
    }
    for entry in &section.compiler_groups {
        ensure_non_empty(&entry.family, path, "compiler_group.family")?;
        ensure_non_empty(&entry.rule_id, path, "compiler_group.rule_id")?;
        ensure_non_empty(
            &entry.first_action_hint,
            path,
            "compiler_group.first_action_hint",
        )?;
        match entry.headline_strategy {
            TextStrategy::FixedText => {
                let Some(headline) = entry.headline.as_deref() else {
                    return Err(invalid_rulepack(
                        path,
                        "compiler_group fixed_text entries must set headline",
                    ));
                };
                ensure_non_empty(headline, path, "compiler_group.headline")?;
            }
            TextStrategy::MessagePassthrough | TextStrategy::FirstMessageLine => {
                if entry.headline.is_some() {
                    return Err(invalid_rulepack(
                        path,
                        "compiler_group non-fixed headline strategies must omit headline",
                    ));
                }
            }
        }
    }
    for entry in &section.linker_groups {
        ensure_non_empty(&entry.family, path, "linker_group.family")?;
        ensure_non_empty(&entry.rule_id, path, "linker_group.rule_id")?;
        ensure_non_empty(
            &entry.headline_template,
            path,
            "linker_group.headline_template",
        )?;
        ensure_non_empty(
            &entry.first_action_hint,
            path,
            "linker_group.first_action_hint",
        )?;
    }
    ensure_non_empty(&section.passthrough.family, path, "passthrough.family")?;
    ensure_non_empty(&section.passthrough.rule_id, path, "passthrough.rule_id")?;
    ensure_non_empty(&section.passthrough.headline, path, "passthrough.headline")?;
    ensure_non_empty(
        &section.passthrough.first_action_hint,
        path,
        "passthrough.first_action_hint",
    )?;
    Ok(())
}

fn validate_section_header(
    schema_version: &str,
    rulepack_version: &str,
    actual_kind: SectionKind,
    expected_kind: SectionKind,
    path: &Path,
    expected_version: &str,
) -> Result<(), RulepackError> {
    ensure_schema_version(schema_version, RULEPACK_SECTION_SCHEMA_VERSION, path)?;
    ensure_version_id(rulepack_version, path, "rulepack_version")?;
    if rulepack_version != expected_version {
        return Err(invalid_rulepack(
            path,
            &format!(
                "section rulepack_version {} does not match manifest {}",
                rulepack_version, expected_version
            ),
        ));
    }
    if actual_kind != expected_kind {
        return Err(invalid_rulepack(
            path,
            &format!(
                "section kind {:?} does not match manifest entry {:?}",
                actual_kind, expected_kind
            ),
        ));
    }
    Ok(())
}

fn ensure_schema_version(actual: &str, expected: &str, path: &Path) -> Result<(), RulepackError> {
    if actual == expected {
        Ok(())
    } else {
        Err(invalid_rulepack(
            path,
            &format!("schema_version must be {expected}, got {actual}"),
        ))
    }
}

fn ensure_version_id(value: &str, path: &Path, field_name: &str) -> Result<(), RulepackError> {
    if value.is_empty()
        || !value.chars().all(|ch| {
            ch.is_ascii_lowercase() || ch.is_ascii_digit() || matches!(ch, '.' | '_' | '-')
        })
    {
        return Err(invalid_rulepack(
            path,
            &format!("{field_name} must be a non-empty lowercase ascii version identifier"),
        ));
    }
    Ok(())
}

fn ensure_relative_json_path(path_value: &str, path: &Path) -> Result<(), RulepackError> {
    let section_path = Path::new(path_value);
    if !section_path
        .extension()
        .is_some_and(|extension| extension == "json")
    {
        return Err(invalid_rulepack(
            path,
            &format!("section path {path_value} must end with .json"),
        ));
    }
    if section_path.is_absolute()
        || section_path.components().any(|component| {
            matches!(
                component,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        })
    {
        return Err(invalid_rulepack(
            path,
            &format!("section path {path_value} must be a normalized relative path"),
        ));
    }
    Ok(())
}

fn ensure_sha256_hex(value: &str, path: &Path, label: &str) -> Result<(), RulepackError> {
    if value.len() == 64 && value.chars().all(|ch| ch.is_ascii_hexdigit()) {
        Ok(())
    } else {
        Err(invalid_rulepack(
            path,
            &format!("{label} must be a 64-character sha256 hex digest"),
        ))
    }
}

fn ensure_sections_sorted_and_unique(
    sections: &[ManifestSection],
    path: &Path,
) -> Result<(), RulepackError> {
    let mut last_kind = None;
    let mut kinds = BTreeSet::new();
    let mut section_paths = BTreeSet::new();
    for section in sections {
        if let Some(previous) = last_kind {
            if previous >= section.kind {
                return Err(invalid_rulepack(
                    path,
                    "manifest sections must be sorted by kind without duplicates",
                ));
            }
        }
        last_kind = Some(section.kind);
        if !kinds.insert(section.kind) {
            return Err(invalid_rulepack(
                path,
                "manifest sections must not repeat the same kind",
            ));
        }
        if !section_paths.insert(section.path.as_str()) {
            return Err(invalid_rulepack(
                path,
                "manifest sections must not repeat the same path",
            ));
        }
    }
    Ok(())
}

fn ensure_vec_sorted_and_unique<T, F>(
    values: &[T],
    path: &Path,
    label: &str,
    key_fn: F,
) -> Result<(), RulepackError>
where
    F: Fn(&T) -> &str,
{
    let mut previous = None::<&str>;
    let mut seen = BTreeSet::new();
    for value in values {
        let key = key_fn(value);
        if let Some(last) = previous {
            if last >= key {
                return Err(invalid_rulepack(
                    path,
                    &format!("{label} must be sorted by key without duplicates"),
                ));
            }
        }
        previous = Some(key);
        if !seen.insert(key) {
            return Err(invalid_rulepack(
                path,
                &format!("{label} must not repeat the same key"),
            ));
        }
    }
    Ok(())
}

fn ensure_strings_sorted_and_unique(
    values: &[String],
    path: &Path,
    label: &str,
) -> Result<(), RulepackError> {
    let mut previous = None::<&str>;
    let mut seen = BTreeSet::new();
    for value in values {
        ensure_non_empty(value, path, label)?;
        let key = value.as_str();
        if let Some(last) = previous {
            if last >= key {
                return Err(invalid_rulepack(
                    path,
                    &format!("{label} must be sorted without duplicates"),
                ));
            }
        }
        previous = Some(key);
        if !seen.insert(key) {
            return Err(invalid_rulepack(
                path,
                &format!("{label} must not repeat values"),
            ));
        }
    }
    Ok(())
}

fn ensure_non_empty(value: &str, path: &Path, label: &str) -> Result<(), RulepackError> {
    if value.trim().is_empty() {
        Err(invalid_rulepack(
            path,
            &format!("{label} must be non-empty"),
        ))
    } else {
        Ok(())
    }
}

fn compiler_residual_kind_key(kind: CompilerResidualKind) -> &'static str {
    match kind {
        CompilerResidualKind::Syntax => "syntax",
        CompilerResidualKind::Template => "template",
        CompilerResidualKind::TypeOverload => "type_overload",
        CompilerResidualKind::Unknown => "unknown",
    }
}

fn linker_residual_kind_key(kind: LinkerResidualKind) -> &'static str {
    match kind {
        LinkerResidualKind::AssemblerError => "assembler_error",
        LinkerResidualKind::CannotFindLibrary => "cannot_find_library",
        LinkerResidualKind::FileFormatOrRelocation => "file_format_or_relocation",
        LinkerResidualKind::MultipleDefinition => "multiple_definition",
        LinkerResidualKind::UndefinedReference => "undefined_reference",
    }
}

fn invalid_rulepack(path: &Path, message: &str) -> RulepackError {
    RulepackError::InvalidRulepack {
        path: path.to_path_buf(),
        message: message.to_string(),
    }
}

fn hex_sha256(raw: &[u8]) -> String {
    let digest = Sha256::digest(raw);
    let mut output = String::with_capacity(digest.len() * 2);
    for byte in digest {
        output.push_str(&format!("{byte:02x}"));
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;
    use diag_core::{
        AnalysisOverlay, Confidence, ContextChain, ContextChainKind, DiagnosticDocument,
        DiagnosticNode, DocumentCompleteness, Location, MessageText, NodeCompleteness, Origin,
        ProducerInfo, Provenance, ProvenanceSource, RunInfo, SemanticRole, Severity, SymbolContext,
        ToolInfo,
    };
    use diag_enrich::enrich_document;
    use diag_residual_text::classify;
    use std::path::Path;
    use tempfile::TempDir;

    fn sample_document(node: DiagnosticNode) -> DiagnosticDocument {
        DiagnosticDocument {
            document_id: "doc".to_string(),
            schema_version: "1".to_string(),
            document_completeness: DocumentCompleteness::Complete,
            producer: ProducerInfo {
                name: "gcc-formed".to_string(),
                version: "0.1.0".to_string(),
                git_revision: None,
                build_profile: None,
                rulepack_version: Some(CHECKED_IN_RULEPACK_VERSION.to_string()),
            },
            run: RunInfo {
                invocation_id: "inv".to_string(),
                invoked_as: None,
                argv_redacted: Vec::new(),
                cwd_display: None,
                exit_status: 1,
                primary_tool: ToolInfo {
                    name: "gcc".to_string(),
                    version: None,
                    component: None,
                    vendor: None,
                },
                secondary_tools: Vec::new(),
                language_mode: None,
                target_triple: None,
                wrapper_mode: None,
            },
            captures: Vec::new(),
            integrity_issues: Vec::new(),
            diagnostics: vec![node],
            fingerprints: None,
        }
    }

    fn sample_location(path: &str) -> Location {
        Location {
            path: path.to_string(),
            line: 3,
            column: 1,
            end_line: None,
            end_column: None,
            display_path: None,
            ownership: None,
        }
    }

    fn sample_context_chain(kind: ContextChainKind, label: &str) -> ContextChain {
        ContextChain {
            kind,
            frames: vec![diag_core::ContextFrame {
                label: label.to_string(),
                path: Some("src/main.cpp".to_string()),
                line: Some(6),
                column: Some(15),
            }],
        }
    }

    fn sample_node(message: &str) -> DiagnosticNode {
        DiagnosticNode {
            id: "n1".to_string(),
            origin: Origin::Gcc,
            phase: Phase::Semantic,
            severity: Severity::Error,
            semantic_role: SemanticRole::Root,
            message: MessageText {
                raw_text: message.to_string(),
                normalized_text: None,
                locale: None,
            },
            locations: vec![sample_location("src/main.c")],
            children: Vec::new(),
            suggestions: Vec::new(),
            context_chains: Vec::new(),
            symbol_context: None,
            node_completeness: NodeCompleteness::Complete,
            provenance: Provenance {
                source: ProvenanceSource::Compiler,
                capture_refs: vec!["stderr.raw".to_string()],
            },
            analysis: None,
            fingerprints: None,
        }
    }

    fn copy_checked_in_rulepack(temp_dir: &TempDir) -> PathBuf {
        for file_name in CHECKED_IN_SECTION_FILES {
            fs::copy(
                checked_in_rules_dir().join(file_name),
                temp_dir.path().join(file_name),
            )
            .unwrap();
        }
        temp_dir.path().join(CHECKED_IN_MANIFEST_FILE)
    }

    #[test]
    fn loads_checked_in_phase1_rulepack() {
        let rulepack = load_checked_in_rulepack().unwrap();
        assert_eq!(rulepack.version(), CHECKED_IN_RULEPACK_VERSION);
        assert_eq!(rulepack.manifest().sections.len(), 3);
        assert_eq!(
            rulepack
                .enrich_family_section()
                .ingress_specific_family_overrides,
            vec![
                "linker.cannot_find_library".to_string(),
                "linker.file_format_or_relocation".to_string(),
                "linker.multiple_definition".to_string(),
                "linker.undefined_reference".to_string(),
            ]
        );
    }

    #[test]
    fn checked_in_contract_matches_enrich_phase1_outputs() {
        let rulepack = load_checked_in_rulepack().unwrap();

        let mut syntax_node = sample_node("expected ';' before '}' token");
        syntax_node.phase = Phase::Parse;
        let mut syntax_document = sample_document(syntax_node);
        enrich_document(&mut syntax_document, Path::new("/tmp/project"));
        let analysis = syntax_document.diagnostics[0].analysis.as_ref().unwrap();
        assert_eq!(
            analysis.rule_id.as_deref(),
            Some(rulepack.enrich_rule("syntax").unwrap().rule_id.as_str())
        );
        assert_eq!(
            analysis.headline.as_deref(),
            Some(rulepack.generic_headline("syntax").unwrap())
        );
        assert_eq!(
            analysis.first_action_hint.as_deref(),
            Some(rulepack.generic_action_hint("syntax").unwrap())
        );

        let type_node = sample_node("invalid conversion from 'const char*' to 'int'");
        let mut type_document = sample_document(type_node);
        enrich_document(&mut type_document, Path::new("/tmp/project"));
        let analysis = type_document.diagnostics[0].analysis.as_ref().unwrap();
        assert_eq!(
            analysis.rule_id.as_deref(),
            Some(
                rulepack
                    .enrich_rule("type_overload")
                    .unwrap()
                    .rule_id
                    .as_str()
            )
        );
        assert_eq!(
            analysis.headline.as_deref(),
            Some(rulepack.generic_headline("type_overload").unwrap())
        );
        assert_eq!(
            analysis.first_action_hint.as_deref(),
            Some(rulepack.generic_action_hint("type_overload").unwrap())
        );

        let mut template_node = sample_node("no matching function for call to 'expect_ptr(int&)'");
        template_node.phase = Phase::Instantiate;
        template_node.context_chains = vec![sample_context_chain(
            ContextChainKind::TemplateInstantiation,
            "required from here",
        )];
        template_node.children.push(DiagnosticNode {
            id: "child".to_string(),
            origin: Origin::Gcc,
            phase: Phase::Instantiate,
            severity: Severity::Note,
            semantic_role: SemanticRole::Supporting,
            message: MessageText {
                raw_text: "template argument deduction/substitution failed:".to_string(),
                normalized_text: None,
                locale: None,
            },
            locations: vec![sample_location("src/main.cpp")],
            children: Vec::new(),
            suggestions: Vec::new(),
            context_chains: Vec::new(),
            symbol_context: None,
            node_completeness: NodeCompleteness::Complete,
            provenance: Provenance {
                source: ProvenanceSource::Compiler,
                capture_refs: vec!["stderr.raw".to_string()],
            },
            analysis: None,
            fingerprints: None,
        });
        let mut template_document = sample_document(template_node);
        enrich_document(&mut template_document, Path::new("/tmp/project"));
        let analysis = template_document.diagnostics[0].analysis.as_ref().unwrap();
        assert_eq!(
            analysis.rule_id.as_deref(),
            Some(rulepack.enrich_rule("template").unwrap().rule_id.as_str())
        );
        assert_eq!(
            analysis.headline.as_deref(),
            Some(rulepack.generic_headline("template").unwrap())
        );
        assert_eq!(
            analysis.first_action_hint.as_deref(),
            Some(rulepack.generic_action_hint("template").unwrap())
        );

        let mut macro_node = sample_node("'Box' has no member named 'missing_field'");
        macro_node.context_chains = vec![sample_context_chain(
            ContextChainKind::MacroExpansion,
            "in expansion of macro 'READ_FIELD'",
        )];
        let mut macro_document = sample_document(macro_node);
        enrich_document(&mut macro_document, Path::new("/tmp/project"));
        let analysis = macro_document.diagnostics[0].analysis.as_ref().unwrap();
        assert_eq!(
            analysis.rule_id.as_deref(),
            Some(
                rulepack
                    .enrich_rule("macro_include")
                    .unwrap()
                    .rule_id
                    .as_str()
            )
        );
        assert_eq!(
            analysis.headline.as_deref(),
            Some(rulepack.generic_headline("macro_include").unwrap())
        );
        assert_eq!(
            analysis.first_action_hint.as_deref(),
            Some(rulepack.generic_action_hint("macro_include").unwrap())
        );

        let mut passthrough_node = sample_node("wrapper preserved stderr");
        passthrough_node.semantic_role = SemanticRole::Passthrough;
        let mut passthrough_document = sample_document(passthrough_node);
        enrich_document(&mut passthrough_document, Path::new("/tmp/project"));
        let analysis = passthrough_document.diagnostics[0]
            .analysis
            .as_ref()
            .unwrap();
        assert_eq!(
            analysis.rule_id.as_deref(),
            Some(
                rulepack
                    .enrich_rule("passthrough")
                    .unwrap()
                    .rule_id
                    .as_str()
            )
        );
        assert_eq!(
            analysis.headline.as_deref(),
            Some(rulepack.generic_headline("passthrough").unwrap())
        );
        assert_eq!(
            analysis.first_action_hint.as_deref(),
            Some(rulepack.generic_action_hint("passthrough").unwrap())
        );

        let mut linker_node = sample_node("collect2: error: ld returned 1 exit status");
        linker_node.phase = Phase::Link;
        linker_node.locations.clear();
        linker_node.symbol_context = Some(SymbolContext {
            primary_symbol: Some("missing_symbol".to_string()),
            related_objects: Vec::new(),
            archive: None,
        });
        linker_node.analysis = Some(AnalysisOverlay {
            family: Some("linker.undefined_reference".to_string()),
            headline: None,
            first_action_hint: None,
            confidence: Some(Confidence::Medium),
            rule_id: None,
            matched_conditions: Vec::new(),
            suppression_reason: None,
            collapsed_child_ids: Vec::new(),
            collapsed_chain_ids: Vec::new(),
        });
        let mut linker_document = sample_document(linker_node);
        enrich_document(&mut linker_document, Path::new("/tmp/project"));
        let analysis = linker_document.diagnostics[0].analysis.as_ref().unwrap();
        let override_rule = rulepack
            .specific_wording_override("linker.undefined_reference")
            .unwrap();
        assert_eq!(
            analysis.rule_id.as_deref(),
            Some(
                rulepack
                    .enrich_family_section()
                    .ingress_specific_override_rule_id
                    .as_str()
            )
        );
        assert_eq!(
            analysis.headline.as_deref(),
            Some("undefined reference to `missing_symbol`")
        );
        assert_eq!(
            analysis.first_action_hint.as_deref(),
            Some(override_rule.first_action_hint.as_str())
        );
        assert_eq!(
            override_rule.headline_template,
            "undefined reference to `{symbol}`"
        );
    }

    #[test]
    fn checked_in_contract_matches_residual_phase1_seeds() {
        let rulepack = load_checked_in_rulepack().unwrap();

        let syntax_line = "src/main.c:3:1: error: expected ';' before '}' token";
        let syntax = classify(syntax_line, true);
        let syntax_analysis = syntax[0].analysis.as_ref().unwrap();
        let syntax_seed = rulepack
            .compiler_residual_seed(CompilerResidualKind::Syntax)
            .unwrap();
        assert_eq!(
            syntax_analysis.family.as_deref(),
            Some(syntax_seed.family.as_str())
        );
        assert_eq!(
            syntax_analysis.rule_id.as_deref(),
            Some(syntax_seed.rule_id.as_str())
        );
        assert_eq!(
            syntax_analysis.headline.as_deref(),
            Some("expected ';' before '}' token")
        );
        assert_eq!(
            syntax_analysis.first_action_hint.as_deref(),
            Some(syntax_seed.first_action_hint.as_str())
        );

        let type_line = "src/main.cpp:4:2: error: invalid conversion from 'const char*' to 'int'";
        let type_nodes = classify(type_line, true);
        let type_analysis = type_nodes[0].analysis.as_ref().unwrap();
        let type_seed = rulepack
            .compiler_residual_seed(CompilerResidualKind::TypeOverload)
            .unwrap();
        assert_eq!(
            type_analysis.family.as_deref(),
            Some(type_seed.family.as_str())
        );
        assert_eq!(
            type_analysis.rule_id.as_deref(),
            Some(type_seed.rule_id.as_str())
        );
        assert_eq!(
            type_analysis.headline.as_deref(),
            type_seed.headline.as_deref()
        );
        assert_eq!(
            type_analysis.first_action_hint.as_deref(),
            Some(type_seed.first_action_hint.as_str())
        );

        let template_line =
            "src/main.cpp:4:2: error: template argument deduction/substitution failed:";
        let template_nodes = classify(template_line, true);
        let template_analysis = template_nodes[0].analysis.as_ref().unwrap();
        let template_seed = rulepack
            .compiler_residual_seed(CompilerResidualKind::Template)
            .unwrap();
        assert_eq!(
            template_analysis.family.as_deref(),
            Some(template_seed.family.as_str())
        );
        assert_eq!(
            template_analysis.rule_id.as_deref(),
            Some(template_seed.rule_id.as_str())
        );
        assert_eq!(
            template_analysis.headline.as_deref(),
            template_seed.headline.as_deref()
        );

        let linker_stderr = "main.c:3: undefined reference to `missing_symbol`";
        let linker_nodes = classify(linker_stderr, true);
        let linker_seed = rulepack
            .linker_residual_seed(LinkerResidualKind::UndefinedReference)
            .unwrap();
        let linker_analysis = linker_nodes[0].analysis.as_ref().unwrap();
        assert_eq!(
            linker_analysis.family.as_deref(),
            Some(linker_seed.family.as_str())
        );
        assert_eq!(
            linker_analysis.rule_id.as_deref(),
            Some(linker_seed.rule_id.as_str())
        );
        assert_eq!(
            linker_analysis.first_action_hint.as_deref(),
            Some(linker_seed.first_action_hint.as_str())
        );
        assert_eq!(
            linker_analysis.headline.as_deref(),
            Some("undefined reference to `missing_symbol`")
        );

        let passthrough_nodes = classify("opaque residual line", true);
        let passthrough = passthrough_nodes
            .iter()
            .find(|node| node.semantic_role == SemanticRole::Passthrough)
            .unwrap();
        let passthrough_analysis = passthrough.analysis.as_ref().unwrap();
        assert_eq!(
            passthrough_analysis.family.as_deref(),
            Some(rulepack.residual_section().passthrough.family.as_str())
        );
        assert_eq!(
            passthrough_analysis.rule_id.as_deref(),
            Some(rulepack.residual_section().passthrough.rule_id.as_str())
        );
        assert_eq!(
            passthrough_analysis.headline.as_deref(),
            Some(rulepack.residual_section().passthrough.headline.as_str())
        );
    }

    #[test]
    fn rejects_section_digest_mismatch() {
        let temp_dir = TempDir::new().unwrap();
        let manifest_path = copy_checked_in_rulepack(&temp_dir);
        let residual_path = temp_dir.path().join("diag_rulepack.residual.phase1.json");
        let mutated = fs::read_to_string(&residual_path)
            .unwrap()
            .replace("\"phase\": \"semantic\"", "\"phase\": \"analyze\"");
        fs::write(&residual_path, mutated).unwrap();

        let error = load_rulepack_from_manifest(manifest_path).unwrap_err();
        assert!(matches!(error, RulepackError::DigestMismatch { .. }));
    }

    #[test]
    fn rejects_mixed_section_version_ids() {
        let temp_dir = TempDir::new().unwrap();
        let manifest_path = copy_checked_in_rulepack(&temp_dir);
        let section_path = temp_dir
            .path()
            .join("diag_rulepack.enrich_wording.phase1.json");
        let mutated = fs::read_to_string(&section_path).unwrap().replace(
            "\"rulepack_version\": \"phase1\"",
            "\"rulepack_version\": \"phase9\"",
        );
        fs::write(&section_path, mutated).unwrap();

        let error = load_rulepack_from_manifest(manifest_path).unwrap_err();
        assert!(matches!(error, RulepackError::DigestMismatch { .. }));
    }
}
