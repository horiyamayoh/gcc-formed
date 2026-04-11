//! Validation functions for rulepack manifests and sections.

use crate::{
    ChildNoteConditionConfig, ConfidencePolicyConfig, ContextConditionConfig,
    ENRICH_RULEPACK_SCHEMA_VERSION, EnrichRulepack, FallbackRuleConfig,
    RENDER_RULEPACK_SCHEMA_VERSION, RULEPACK_MANIFEST_SCHEMA_VERSION, RenderRulepack,
    RendererFamilyKind, RulepackError, RulepackManifest, TermGroupConfig,
};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::path::{Component, Path};

pub(crate) fn validate_manifest(
    manifest: &RulepackManifest,
    path: &Path,
) -> Result<(), RulepackError> {
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

    let mut section_kinds = BTreeSet::new();
    let mut section_paths = BTreeSet::new();
    for section in &manifest.sections {
        if !section_kinds.insert(section.kind) {
            return Err(invalid_rulepack(
                path,
                "manifest contains duplicate section kinds",
            ));
        }
        if !section_paths.insert(section.path.as_str()) {
            return Err(invalid_rulepack(
                path,
                "manifest contains duplicate section paths",
            ));
        }
        ensure_relative_json_path(&section.path, path)?;
        ensure_sha256_hex(&section.sha256, path, &section.path)?;
    }
    Ok(())
}

pub(crate) fn validate_enrich_rulepack(
    rulepack: &EnrichRulepack,
    path: &Path,
    expected_version: &str,
) -> Result<(), RulepackError> {
    validate_section_header(
        &rulepack.schema_version,
        &rulepack.rulepack_version,
        ENRICH_RULEPACK_SCHEMA_VERSION,
        path,
        expected_version,
    )?;
    ensure_non_empty(
        &rulepack.ingress_specific_override_rule_id,
        path,
        "ingress_specific_override_rule_id",
    )?;
    ensure_fallback_rule(&rulepack.unknown_fallback, path, "unknown_fallback")?;
    ensure_fallback_rule(&rulepack.passthrough_fallback, path, "passthrough_fallback")?;
    if rulepack.rules.is_empty() {
        return Err(invalid_rulepack(
            path,
            "checked-in enrich rulepack must define at least one family rule",
        ));
    }

    let mut families = BTreeSet::new();
    let mut rule_ids = BTreeSet::new();
    for rule in &rulepack.rules {
        if !families.insert(rule.family.as_str()) {
            return Err(invalid_rulepack(
                path,
                format!(
                    "duplicate family rule in checked-in enrich rulepack: {}",
                    rule.family
                ),
            ));
        }
        if !rule_ids.insert(rule.rule_id.as_str()) {
            return Err(invalid_rulepack(
                path,
                format!(
                    "duplicate rule id in checked-in enrich rulepack: {}",
                    rule.rule_id
                ),
            ));
        }
        ensure_non_empty(&rule.rule_id, path, "rule.rule_id")?;
        ensure_non_empty(&rule.family, path, "rule.family")?;
        ensure_non_empty_groups(&rule.message_groups, path, "rule.message_groups")?;
        ensure_non_empty_groups(
            &rule.child_message_groups,
            path,
            "rule.child_message_groups",
        )?;
        ensure_non_empty_strings(
            &rule.candidate_child_terms,
            path,
            "rule.candidate_child_terms",
        )?;
        ensure_conditions_non_empty(&rule.contexts, path, "rule.contexts")?;
        ensure_conditions_non_empty(&rule.child_notes, path, "rule.child_notes")?;
        if let Some(condition) = &rule.symbol_context_condition {
            ensure_non_empty(condition, path, "rule.symbol_context_condition")?;
        }
        if let Some(condition) = &rule.candidate_child_condition {
            ensure_non_empty(condition, path, "rule.candidate_child_condition")?;
        }
        if let Some(condition) = &rule.semantic_role_condition {
            ensure_non_empty(condition, path, "rule.semantic_role_condition")?;
        }
        for annotation in &rule.phase_annotations {
            ensure_non_empty(
                &annotation.condition,
                path,
                "rule.phase_annotations.condition",
            )?;
        }
    }

    let mut confidence_families = BTreeSet::new();
    for policy in &rulepack.confidence_policies {
        let family = policy.family.as_deref().ok_or_else(|| {
            invalid_rulepack(path, "family confidence policies must name a family")
        })?;
        if !confidence_families.insert(family) {
            return Err(invalid_rulepack(
                path,
                format!("duplicate confidence policy in checked-in enrich rulepack: {family}"),
            ));
        }
        validate_confidence_policy(policy, path)?;
    }
    validate_confidence_policy(&rulepack.default_confidence_policy, path)?;

    for family in [
        "syntax",
        "type_overload",
        "template",
        "macro_include",
        "linker",
    ] {
        if !rulepack
            .confidence_policies
            .iter()
            .any(|policy| policy.family.as_deref() == Some(family))
        {
            return Err(invalid_rulepack(
                path,
                format!("missing confidence policy in checked-in enrich rulepack: {family}"),
            ));
        }
    }
    Ok(())
}

pub(crate) fn validate_render_rulepack(
    rulepack: &RenderRulepack,
    path: &Path,
    expected_version: &str,
) -> Result<(), RulepackError> {
    validate_section_header(
        &rulepack.schema_version,
        &rulepack.rulepack_version,
        RENDER_RULEPACK_SCHEMA_VERSION,
        path,
        expected_version,
    )?;
    if rulepack.family_policies.is_empty() {
        return Err(invalid_rulepack(
            path,
            "checked-in render rulepack must define family_policies",
        ));
    }

    let mut seen_kinds = BTreeSet::new();
    for policy in &rulepack.family_policies {
        if policy.kind == RendererFamilyKind::Unknown {
            return Err(invalid_rulepack(
                path,
                "checked-in render rulepack must not define unknown family policies",
            ));
        }
        if !seen_kinds.insert(policy.kind) {
            return Err(invalid_rulepack(
                path,
                format!(
                    "duplicate renderer family policy in checked-in render rulepack: {:?}",
                    policy.kind
                ),
            ));
        }
        if policy.match_exact.is_some() == policy.match_prefix.is_some() {
            return Err(invalid_rulepack(
                path,
                "renderer family policy must set exactly one of match_exact/match_prefix",
            ));
        }
        if let Some(match_exact) = policy.match_exact.as_deref() {
            ensure_non_empty(match_exact, path, "render.match_exact")?;
        }
        if let Some(match_prefix) = policy.match_prefix.as_deref() {
            ensure_non_empty(match_prefix, path, "render.match_prefix")?;
        }
        if let Some(exclude_exact) = policy.exclude_exact.as_deref() {
            ensure_non_empty(exclude_exact, path, "render.exclude_exact")?;
        }
    }
    Ok(())
}

pub(crate) fn validate_section_header(
    schema_version: &str,
    rulepack_version: &str,
    expected_schema: &str,
    path: &Path,
    expected_version: &str,
) -> Result<(), RulepackError> {
    ensure_schema_version(schema_version, expected_schema, path)?;
    ensure_version_id(rulepack_version, path, "rulepack_version")?;
    if rulepack_version != expected_version {
        return Err(invalid_rulepack(
            path,
            format!(
                "section rulepack_version {} does not match manifest {}",
                rulepack_version, expected_version
            ),
        ));
    }
    Ok(())
}

fn ensure_fallback_rule(
    fallback: &FallbackRuleConfig,
    path: &Path,
    label: &str,
) -> Result<(), RulepackError> {
    ensure_non_empty(&fallback.family, path, &format!("{label}.family"))?;
    ensure_non_empty(&fallback.rule_id, path, &format!("{label}.rule_id"))?;
    ensure_non_empty(
        &fallback.suppression_reason,
        path,
        &format!("{label}.suppression_reason"),
    )?;
    ensure_non_empty_strings(
        &fallback.matched_conditions,
        path,
        &format!("{label}.matched_conditions"),
    )?;
    Ok(())
}

fn validate_confidence_policy(
    policy: &ConfidencePolicyConfig,
    path: &Path,
) -> Result<(), RulepackError> {
    for clause in policy
        .high_when_any
        .iter()
        .chain(policy.medium_when_any.iter())
    {
        if clause.all.is_empty() {
            return Err(invalid_rulepack(
                path,
                "confidence clauses must include at least one signal",
            ));
        }
    }
    Ok(())
}

fn ensure_non_empty_groups(
    groups: &[TermGroupConfig],
    path: &Path,
    label: &str,
) -> Result<(), RulepackError> {
    for group in groups {
        ensure_non_empty(&group.prefix, path, &format!("{label}.prefix"))?;
        ensure_non_empty_strings(&group.terms, path, &format!("{label}.terms"))?;
    }
    Ok(())
}

fn ensure_conditions_non_empty<T>(
    conditions: &[T],
    path: &Path,
    label: &str,
) -> Result<(), RulepackError>
where
    T: ConditionField,
{
    for condition in conditions {
        ensure_non_empty(condition.condition(), path, label)?;
    }
    Ok(())
}

trait ConditionField {
    fn condition(&self) -> &str;
}

impl ConditionField for ContextConditionConfig {
    fn condition(&self) -> &str {
        &self.condition
    }
}

impl ConditionField for ChildNoteConditionConfig {
    fn condition(&self) -> &str {
        &self.condition
    }
}

pub(crate) fn ensure_non_empty_strings(
    values: &[String],
    path: &Path,
    label: &str,
) -> Result<(), RulepackError> {
    for value in values {
        ensure_non_empty(value, path, label)?;
    }
    Ok(())
}

fn ensure_schema_version(actual: &str, expected: &str, path: &Path) -> Result<(), RulepackError> {
    if actual == expected {
        Ok(())
    } else {
        Err(invalid_rulepack(
            path,
            format!("expected schema_version {expected}, got {actual}"),
        ))
    }
}

fn ensure_version_id(value: &str, path: &Path, field: &str) -> Result<(), RulepackError> {
    ensure_non_empty(value, path, field)?;
    let mut chars = value.chars();
    let Some(first) = chars.next() else {
        unreachable!("ensure_non_empty returned ok for an empty string");
    };
    if !first.is_ascii_lowercase()
        || !chars.all(|ch| {
            ch.is_ascii_lowercase() || ch.is_ascii_digit() || matches!(ch, '.' | '_' | '-')
        })
    {
        return Err(invalid_rulepack(
            path,
            format!(
                "{field} must start with a lowercase ASCII letter and contain only lowercase ASCII letters, digits, '.', '_' or '-'"
            ),
        ));
    }
    Ok(())
}

pub(crate) fn ensure_non_empty(value: &str, path: &Path, field: &str) -> Result<(), RulepackError> {
    if value.trim().is_empty() {
        Err(invalid_rulepack(path, format!("{field} must be non-empty")))
    } else {
        Ok(())
    }
}

fn ensure_relative_json_path(value: &str, path: &Path) -> Result<(), RulepackError> {
    if value.trim().is_empty() {
        return Err(invalid_rulepack(path, "section paths must be non-empty"));
    }
    let section_path = Path::new(value);
    if section_path.is_absolute() {
        return Err(invalid_rulepack(path, "section paths must be relative"));
    }
    for component in section_path.components() {
        if !matches!(component, Component::Normal(_)) {
            return Err(invalid_rulepack(
                path,
                "section paths must be normalized relative JSON paths",
            ));
        }
    }
    if section_path.extension().and_then(|ext| ext.to_str()) != Some("json") {
        return Err(invalid_rulepack(
            path,
            "section paths must reference JSON files",
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
            format!("{label} must be a 64-character SHA-256 hex digest"),
        ))
    }
}

pub(crate) fn hex_sha256(raw: &[u8]) -> String {
    let digest = Sha256::digest(raw);
    let mut rendered = String::with_capacity(digest.len() * 2);
    for byte in digest {
        rendered.push_str(&format!("{byte:02x}"));
    }
    rendered
}

pub(crate) fn invalid_rulepack(path: &Path, message: impl Into<String>) -> RulepackError {
    RulepackError::InvalidRulepack {
        path: path.to_path_buf(),
        message: message.into(),
    }
}
