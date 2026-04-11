//! Validation functions for the residual rulepack section.

use crate::validate::{
    ensure_non_empty, ensure_non_empty_strings, invalid_rulepack, validate_section_header,
};
use crate::{
    CompilerResidualKind, HeadlineStrategy, LinkerResidualKind, LinkerResidualSeed,
    RESIDUAL_RULEPACK_SCHEMA_VERSION, ResidualRulepack, RulepackError,
};
use diag_core::Phase;
use regex::Regex;
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

pub(crate) fn validate_residual_rulepack(
    rulepack: &ResidualRulepack,
    path: &Path,
    expected_version: &str,
) -> Result<(), RulepackError> {
    validate_section_header(
        &rulepack.schema_version,
        &rulepack.rulepack_version,
        RESIDUAL_RULEPACK_SCHEMA_VERSION,
        path,
        expected_version,
    )?;
    ensure_non_empty(
        &rulepack.wording.default_action_hint,
        path,
        "wording.default_action_hint",
    )?;
    ensure_non_empty(
        &rulepack.wording.generic_linker_symbol_headline_template,
        path,
        "wording.generic_linker_symbol_headline_template",
    )?;

    let mut headline_families = BTreeSet::new();
    let mut action_families = BTreeSet::new();
    let mut specific_families = BTreeSet::new();
    for entry in &rulepack.wording.generic_headlines {
        ensure_non_empty(&entry.family, path, "wording.generic_headlines.family")?;
        ensure_non_empty(&entry.text, path, "wording.generic_headlines.text")?;
        if !headline_families.insert(entry.family.as_str()) {
            return Err(invalid_rulepack(
                path,
                format!(
                    "duplicate generic headline family in residual rulepack: {}",
                    entry.family
                ),
            ));
        }
    }
    for entry in &rulepack.wording.generic_action_hints {
        ensure_non_empty(&entry.family, path, "wording.generic_action_hints.family")?;
        ensure_non_empty(&entry.text, path, "wording.generic_action_hints.text")?;
        if !action_families.insert(entry.family.as_str()) {
            return Err(invalid_rulepack(
                path,
                format!(
                    "duplicate generic action family in residual rulepack: {}",
                    entry.family
                ),
            ));
        }
    }
    if headline_families != action_families {
        return Err(invalid_rulepack(
            path,
            "generic headline/action family sets must stay aligned",
        ));
    }
    for entry in &rulepack.wording.specific_overrides {
        ensure_non_empty(&entry.family, path, "wording.specific_overrides.family")?;
        ensure_non_empty(
            &entry.headline_template,
            path,
            "wording.specific_overrides.headline_template",
        )?;
        ensure_non_empty(
            &entry.headline_without_symbol,
            path,
            "wording.specific_overrides.headline_without_symbol",
        )?;
        ensure_non_empty(
            &entry.first_action_hint,
            path,
            "wording.specific_overrides.first_action_hint",
        )?;
        if !specific_families.insert(entry.family.as_str()) {
            return Err(invalid_rulepack(
                path,
                format!(
                    "duplicate specific wording family in residual rulepack: {}",
                    entry.family
                ),
            ));
        }
    }

    let mut compiler_kinds = BTreeSet::new();
    for entry in &rulepack.residual.compiler_groups {
        if !compiler_kinds.insert(entry.kind) {
            return Err(invalid_rulepack(
                path,
                "duplicate compiler residual kind in checked-in residual rulepack",
            ));
        }
        ensure_non_empty(&entry.family, path, "compiler_group.family")?;
        ensure_non_empty(&entry.rule_id, path, "compiler_group.rule_id")?;
        ensure_non_empty(
            &entry.first_action_hint,
            path,
            "compiler_group.first_action_hint",
        )?;
        ensure_non_empty_strings(&entry.match_any, path, "compiler_group.match_any")?;
        if matches!(entry.headline_strategy, HeadlineStrategy::FixedText) {
            let Some(headline) = entry.headline.as_deref() else {
                return Err(invalid_rulepack(
                    path,
                    "fixed_text compiler residual seeds must include headline",
                ));
            };
            ensure_non_empty(headline, path, "compiler_group.headline")?;
        }
    }
    if !compiler_kinds.contains(&CompilerResidualKind::Unknown) {
        return Err(invalid_rulepack(
            path,
            "checked-in residual rulepack must include unknown compiler seed",
        ));
    }

    ensure_non_empty(
        &rulepack
            .residual
            .compiler_note_rules
            .candidate_numbered_prefix,
        path,
        "compiler_note_rules.candidate_numbered_prefix",
    )?;
    ensure_non_empty_strings(
        &rulepack.residual.compiler_note_rules.template_context_any,
        path,
        "compiler_note_rules.template_context_any",
    )?;
    ensure_non_empty_strings(
        &rulepack.residual.compiler_note_rules.candidate_contains,
        path,
        "compiler_note_rules.candidate_contains",
    )?;

    let mut linker_rules = BTreeMap::new();
    for entry in &rulepack.residual.linker_groups {
        if linker_rules.insert(entry.kind, entry).is_some() {
            return Err(invalid_rulepack(
                path,
                "duplicate linker residual kind in checked-in residual rulepack",
            ));
        }
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
        if entry.group_key.is_some() == entry.group_key_template.is_some() {
            return Err(invalid_rulepack(
                path,
                "linker residual rules must set exactly one of group_key/group_key_template",
            ));
        }
        if entry.match_regex.is_none() && entry.match_prefix.is_none() {
            return Err(invalid_rulepack(
                path,
                "linker residual rules must set match_regex or match_prefix",
            ));
        }
        if let Some(group_key) = &entry.group_key {
            ensure_non_empty(group_key, path, "linker_group.group_key")?;
        }
        if let Some(group_key_template) = &entry.group_key_template {
            ensure_non_empty(group_key_template, path, "linker_group.group_key_template")?;
        }
        if let Some(pattern) = &entry.match_regex {
            ensure_non_empty(pattern, path, "linker_group.match_regex")?;
            Regex::new(pattern).map_err(|error| {
                invalid_rulepack(
                    path,
                    format!("invalid linker residual regex `{pattern}`: {error}"),
                )
            })?;
        }
        if let Some(prefix) = &entry.match_prefix {
            ensure_non_empty(prefix, path, "linker_group.match_prefix")?;
        }
        if let Some(symbol_capture) = &entry.symbol_capture {
            ensure_non_empty(symbol_capture, path, "linker_group.symbol_capture")?;
        }
    }
    for kind in [
        LinkerResidualKind::UndefinedReference,
        LinkerResidualKind::MultipleDefinition,
        LinkerResidualKind::CannotFindLibrary,
        LinkerResidualKind::FileFormatOrRelocation,
        LinkerResidualKind::Collect2Summary,
        LinkerResidualKind::AssemblerError,
        LinkerResidualKind::DriverFatal,
        LinkerResidualKind::InternalCompilerErrorBanner,
    ] {
        if !linker_rules.contains_key(&kind) {
            return Err(invalid_rulepack(
                path,
                format!("missing grouped residual kind in checked-in residual rulepack: {kind:?}"),
            ));
        }
    }
    ensure_grouped_residual_contract(
        linker_rules[&LinkerResidualKind::UndefinedReference],
        "linker.undefined_reference",
        diag_core::Origin::Linker,
        Phase::Link,
        path,
    )?;
    ensure_grouped_residual_contract(
        linker_rules[&LinkerResidualKind::MultipleDefinition],
        "linker.multiple_definition",
        diag_core::Origin::Linker,
        Phase::Link,
        path,
    )?;
    ensure_grouped_residual_contract(
        linker_rules[&LinkerResidualKind::CannotFindLibrary],
        "linker.cannot_find_library",
        diag_core::Origin::Linker,
        Phase::Link,
        path,
    )?;
    ensure_grouped_residual_contract(
        linker_rules[&LinkerResidualKind::FileFormatOrRelocation],
        "linker.file_format_or_relocation",
        diag_core::Origin::Linker,
        Phase::Link,
        path,
    )?;
    ensure_grouped_residual_contract(
        linker_rules[&LinkerResidualKind::Collect2Summary],
        "collect2_summary",
        diag_core::Origin::Driver,
        Phase::Link,
        path,
    )?;
    ensure_grouped_residual_contract(
        linker_rules[&LinkerResidualKind::AssemblerError],
        "assembler_error",
        diag_core::Origin::ExternalTool,
        Phase::Assemble,
        path,
    )?;
    ensure_grouped_residual_contract(
        linker_rules[&LinkerResidualKind::DriverFatal],
        "driver_fatal",
        diag_core::Origin::Driver,
        Phase::Driver,
        path,
    )?;
    ensure_grouped_residual_contract(
        linker_rules[&LinkerResidualKind::InternalCompilerErrorBanner],
        "internal_compiler_error_banner",
        diag_core::Origin::Gcc,
        Phase::Unknown,
        path,
    )?;

    ensure_non_empty(
        &rulepack.residual.passthrough.family,
        path,
        "passthrough.family",
    )?;
    ensure_non_empty(
        &rulepack.residual.passthrough.rule_id,
        path,
        "passthrough.rule_id",
    )?;
    ensure_non_empty(
        &rulepack.residual.passthrough.headline,
        path,
        "passthrough.headline",
    )?;
    ensure_non_empty(
        &rulepack.residual.passthrough.first_action_hint,
        path,
        "passthrough.first_action_hint",
    )?;
    Ok(())
}

fn ensure_grouped_residual_contract(
    entry: &LinkerResidualSeed,
    expected_family: &str,
    expected_origin: diag_core::Origin,
    expected_phase: Phase,
    path: &Path,
) -> Result<(), RulepackError> {
    if entry.family != expected_family {
        return Err(invalid_rulepack(
            path,
            format!(
                "grouped residual kind {:?} must use family `{expected_family}`, got `{}`",
                entry.kind, entry.family
            ),
        ));
    }
    if entry.origin != expected_origin || entry.phase != expected_phase {
        return Err(invalid_rulepack(
            path,
            format!(
                "grouped residual kind {:?} must use origin `{:?}` and phase `{:?}`",
                entry.kind, expected_origin, expected_phase
            ),
        ));
    }
    Ok(())
}
