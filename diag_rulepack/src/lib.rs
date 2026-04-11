//! Rule pack definitions and loading for the diagnostic pipeline.
//!
//! This crate defines configuration types for enrichment rules, residual text
//! classification rules, and rendering policies. Rule packs are loaded from
//! JSON manifests that reference versioned, SHA-256-verified section files.
//!
//! Key types:
//! - [`LoadedRulepack`] -- a fully validated, ready-to-use rule pack bundle.
//! - [`EnrichRulepack`] -- family-level match rules and confidence policies.
//! - [`ResidualRulepack`] -- wording templates and residual classification seeds.
//! - [`RenderRulepack`] -- per-family rendering policies and profile limits.

mod manifest;
mod rules;
mod rules_enrich;
mod validate;
mod validate_residual;

pub use manifest::*;
pub use rules::*;
pub use rules_enrich::*;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manifest::CHECKED_IN_SECTION_FILES;
    use crate::validate::hex_sha256;
    use std::fs;
    use std::path::PathBuf;
    use tempfile::TempDir;

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
        let rulepack = checked_in_rulepack();
        assert_eq!(rulepack.version(), CHECKED_IN_RULEPACK_VERSION);
        assert_eq!(rulepack.manifest().sections.len(), 3);
        assert_eq!(
            rulepack.enrich().rule("syntax").rule_id,
            "rule.family.syntax.phase_or_message"
        );
        assert_eq!(
            rulepack
                .residual()
                .compiler_seed(CompilerResidualKind::Template)
                .headline
                .as_deref(),
            Some("template instantiation failed")
        );
        assert!(
            rulepack
                .render()
                .policy_for_kind(RendererFamilyKind::Linker)
                .is_some()
        );
        assert!(
            rulepack
                .residual()
                .residual
                .linker_groups
                .iter()
                .any(|entry| entry.kind == LinkerResidualKind::DriverFatal)
        );
        assert!(
            rulepack
                .residual()
                .residual
                .linker_groups
                .iter()
                .any(|entry| entry.kind == LinkerResidualKind::Collect2Summary)
        );
    }

    #[test]
    fn on_disk_loader_matches_embedded_rulepack() {
        let temp_dir = TempDir::new().unwrap();
        let manifest_path = copy_checked_in_rulepack(&temp_dir);
        let loaded = load_rulepack_from_manifest(manifest_path).unwrap();
        assert_eq!(loaded, checked_in_rulepack().clone());
    }

    #[test]
    fn rejects_section_digest_mismatch() {
        let temp_dir = TempDir::new().unwrap();
        let manifest_path = copy_checked_in_rulepack(&temp_dir);
        let mut manifest: RulepackManifest =
            serde_json::from_slice(&fs::read(&manifest_path).unwrap()).unwrap();
        manifest.sections[0].sha256 = "0".repeat(64);
        fs::write(
            &manifest_path,
            serde_json::to_vec_pretty(&manifest).unwrap(),
        )
        .unwrap();

        let error = load_rulepack_from_manifest(&manifest_path).unwrap_err();
        assert!(matches!(error, RulepackError::DigestMismatch { .. }));
    }

    #[test]
    fn rejects_mixed_section_version_ids() {
        let temp_dir = TempDir::new().unwrap();
        let manifest_path = copy_checked_in_rulepack(&temp_dir);
        let residual_path = temp_dir.path().join("residual.rulepack.json");
        let mut residual: ResidualRulepack =
            serde_json::from_slice(&fs::read(&residual_path).unwrap()).unwrap();
        residual.rulepack_version = "phase0".to_string();
        let residual_raw = serde_json::to_vec_pretty(&residual).unwrap();
        fs::write(&residual_path, &residual_raw).unwrap();

        let mut manifest: RulepackManifest =
            serde_json::from_slice(&fs::read(&manifest_path).unwrap()).unwrap();
        manifest
            .sections
            .iter_mut()
            .find(|section| section.path == "residual.rulepack.json")
            .unwrap()
            .sha256 = hex_sha256(&residual_raw);
        fs::write(
            &manifest_path,
            serde_json::to_vec_pretty(&manifest).unwrap(),
        )
        .unwrap();

        let error = load_rulepack_from_manifest(&manifest_path).unwrap_err();
        match error {
            RulepackError::InvalidRulepack { message, .. } => {
                assert!(message.contains("does not match manifest"));
            }
            other => panic!("unexpected error: {other:?}"),
        }
    }

    #[test]
    fn rejects_invalid_manifest_version_id() {
        let temp_dir = TempDir::new().unwrap();
        let manifest_path = copy_checked_in_rulepack(&temp_dir);
        let mut manifest: RulepackManifest =
            serde_json::from_slice(&fs::read(&manifest_path).unwrap()).unwrap();
        manifest.rulepack_version = "Phase 1".to_string();
        fs::write(
            &manifest_path,
            serde_json::to_vec_pretty(&manifest).unwrap(),
        )
        .unwrap();

        let error = load_rulepack_from_manifest(&manifest_path).unwrap_err();
        match error {
            RulepackError::InvalidRulepack { message, .. } => {
                assert!(
                    message.contains("rulepack_version must start with a lowercase ASCII letter")
                );
            }
            other => panic!("unexpected error: {other:?}"),
        }
    }

    #[test]
    fn rejects_non_normalized_section_paths() {
        let temp_dir = TempDir::new().unwrap();
        let manifest_path = copy_checked_in_rulepack(&temp_dir);
        let mut manifest: RulepackManifest =
            serde_json::from_slice(&fs::read(&manifest_path).unwrap()).unwrap();
        manifest.sections[0].path = "./enrich.rulepack.json".to_string();
        fs::write(
            &manifest_path,
            serde_json::to_vec_pretty(&manifest).unwrap(),
        )
        .unwrap();

        let error = load_rulepack_from_manifest(&manifest_path).unwrap_err();
        match error {
            RulepackError::InvalidRulepack { message, .. } => {
                assert!(message.contains("normalized relative JSON paths"));
            }
            other => panic!("unexpected error: {other:?}"),
        }
    }

    #[test]
    fn rejects_missing_required_grouped_residual_kind() {
        let temp_dir = TempDir::new().unwrap();
        let manifest_path = copy_checked_in_rulepack(&temp_dir);
        let residual_path = temp_dir.path().join("residual.rulepack.json");
        let mut residual: ResidualRulepack =
            serde_json::from_slice(&fs::read(&residual_path).unwrap()).unwrap();
        residual
            .residual
            .linker_groups
            .retain(|entry| entry.kind != LinkerResidualKind::DriverFatal);
        let residual_raw = serde_json::to_vec_pretty(&residual).unwrap();
        fs::write(&residual_path, &residual_raw).unwrap();

        let mut manifest: RulepackManifest =
            serde_json::from_slice(&fs::read(&manifest_path).unwrap()).unwrap();
        manifest
            .sections
            .iter_mut()
            .find(|section| section.path == "residual.rulepack.json")
            .unwrap()
            .sha256 = hex_sha256(&residual_raw);
        fs::write(
            &manifest_path,
            serde_json::to_vec_pretty(&manifest).unwrap(),
        )
        .unwrap();

        let error = load_rulepack_from_manifest(&manifest_path).unwrap_err();
        match error {
            RulepackError::InvalidRulepack { message, .. } => {
                assert!(message.contains("missing grouped residual kind"));
                assert!(message.contains("DriverFatal"));
            }
            other => panic!("unexpected error: {other:?}"),
        }
    }

    #[test]
    fn rejects_collect2_summary_with_linker_origin() {
        let temp_dir = TempDir::new().unwrap();
        let manifest_path = copy_checked_in_rulepack(&temp_dir);
        let residual_path = temp_dir.path().join("residual.rulepack.json");
        let mut residual: ResidualRulepack =
            serde_json::from_slice(&fs::read(&residual_path).unwrap()).unwrap();
        residual
            .residual
            .linker_groups
            .iter_mut()
            .find(|entry| entry.kind == LinkerResidualKind::Collect2Summary)
            .unwrap()
            .origin = diag_core::Origin::Linker;
        let residual_raw = serde_json::to_vec_pretty(&residual).unwrap();
        fs::write(&residual_path, &residual_raw).unwrap();

        let mut manifest: RulepackManifest =
            serde_json::from_slice(&fs::read(&manifest_path).unwrap()).unwrap();
        manifest
            .sections
            .iter_mut()
            .find(|section| section.path == "residual.rulepack.json")
            .unwrap()
            .sha256 = hex_sha256(&residual_raw);
        fs::write(
            &manifest_path,
            serde_json::to_vec_pretty(&manifest).unwrap(),
        )
        .unwrap();

        let error = load_rulepack_from_manifest(&manifest_path).unwrap_err();
        match error {
            RulepackError::InvalidRulepack { message, .. } => {
                assert!(message.contains("Collect2Summary"));
                assert!(message.contains("origin `Driver` and phase `Link`"));
            }
            other => panic!("unexpected error: {other:?}"),
        }
    }
}
