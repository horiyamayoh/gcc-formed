//! Enriches diagnostic documents with family classification, action hints,
//! headlines, and ownership information.

mod action_hint;
mod family;
mod headline;
mod ownership;

use crate::action_hint::action_hint_for;
use crate::family::{classify_confidence, classify_family};
use crate::headline::headline_for;
use crate::ownership::classify_ownership;
use diag_core::{AnalysisOverlay, DiagnosticDocument, DiagnosticNode};
use std::path::Path;

/// Enriches every diagnostic node in `document` with family, confidence,
/// headline, action hint, and file-ownership annotations.
pub fn enrich_document(document: &mut DiagnosticDocument, cwd: &Path) {
    for node in &mut document.diagnostics {
        enrich_node(node, cwd);
    }
    document.refresh_fingerprints();
}

fn enrich_node(node: &mut DiagnosticNode, cwd: &Path) {
    for location in &mut node.locations {
        if location.file.ownership.is_none() {
            location.file.ownership = Some(classify_ownership(location.path_raw(), cwd));
        }
    }

    let family_decision = classify_family(node);
    let confidence = classify_confidence(node, &family_decision);
    let headline = headline_for(node, family_decision.family.as_str());
    let first_action_hint = action_hint_for(node, family_decision.family.as_str());

    let analysis = node.analysis.get_or_insert(AnalysisOverlay {
        family: None,
        family_version: None,
        family_confidence: None,
        root_cause_score: None,
        actionability_score: None,
        user_code_priority: None,
        headline: None,
        first_action_hint: None,
        confidence: None,
        preferred_primary_location_id: None,
        rule_id: None,
        matched_conditions: Vec::new(),
        suppression_reason: None,
        collapsed_child_ids: Vec::new(),
        collapsed_chain_ids: Vec::new(),
        group_ref: None,
        reasons: Vec::new(),
        policy_profile: None,
        producer_version: None,
    });
    analysis.family = Some(family_decision.family.clone().into());
    analysis.headline = Some(headline.into());
    analysis.first_action_hint = Some(first_action_hint.into());
    analysis.set_confidence_bucket(confidence);
    analysis.rule_id = Some(family_decision.rule_id.into());
    analysis.matched_conditions = family_decision
        .matched_conditions
        .into_iter()
        .map(Into::into)
        .collect();
    analysis.suppression_reason = family_decision.suppression_reason;

    for child in &mut node.children {
        enrich_node(child, cwd);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use diag_core::{
        Confidence, ContextChain, ContextChainKind, DiagnosticDocument, DiagnosticNode,
        DocumentCompleteness, Location, MessageText, NodeCompleteness, Origin, Ownership, Phase,
        ProducerInfo, Provenance, ProvenanceSource, RunInfo, SemanticRole, Severity, SymbolContext,
        ToolInfo,
    };

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
                rulepack_version: None,
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
        Location::caret(path, 3, 1, diag_core::LocationRole::Primary)
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

    #[test]
    fn annotates_user_owned_syntax_diagnostic() {
        let mut node = sample_node("expected ';' before '}' token");
        node.phase = Phase::Parse;
        let mut document = sample_document(node);

        enrich_document(&mut document, Path::new("/tmp/project"));

        let analysis = document.diagnostics[0].analysis.as_ref().unwrap();
        assert_eq!(analysis.family.as_deref(), Some("syntax"));
        assert_eq!(analysis.confidence_bucket(), Some(Confidence::High));
        assert_eq!(
            document.diagnostics[0].locations[0].ownership(),
            Some(&Ownership::User)
        );
    }

    #[test]
    fn preserves_existing_file_ownership_annotations() {
        let mut node = sample_node("expected ';' before '}' token");
        node.phase = Phase::Parse;
        node.locations[0] =
            sample_location("src/main.c").with_ownership(Ownership::Vendor, "fixture_vendor");
        let mut document = sample_document(node);

        enrich_document(&mut document, Path::new("/tmp/project"));

        let location = &document.diagnostics[0].locations[0];
        assert_eq!(location.ownership(), Some(&Ownership::Vendor));
        assert_eq!(
            location
                .file
                .ownership
                .as_ref()
                .map(|ownership| ownership.reason.as_str()),
            Some("fixture_vendor")
        );
    }

    #[test]
    fn classifies_type_overload_with_explicit_message_rule() {
        let node = sample_node("invalid conversion from 'const char*' to 'int'");
        let mut document = sample_document(node);

        enrich_document(&mut document, Path::new("/tmp/project"));

        let analysis = document.diagnostics[0].analysis.as_ref().unwrap();
        assert_eq!(analysis.family.as_deref(), Some("type_overload"));
        assert_eq!(
            analysis.rule_id.as_deref(),
            Some("rule.family.type_overload.structured_or_message")
        );
        assert_eq!(analysis.confidence_bucket(), Some(Confidence::Medium));
        assert_eq!(
            analysis.headline.as_deref(),
            Some("type or overload mismatch")
        );
        assert_eq!(
            analysis.first_action_hint.as_deref(),
            Some("compare the expected type and actual argument at the call site")
        );
    }

    #[test]
    fn classifies_type_overload_with_candidate_note_as_high_confidence() {
        let mut node = sample_node("no matching function for call to 'consume(value)'");
        node.children.push(DiagnosticNode {
            id: "candidate".to_string(),
            origin: Origin::Gcc,
            phase: Phase::Semantic,
            severity: Severity::Note,
            semantic_role: SemanticRole::Candidate,
            message: MessageText {
                raw_text: "candidate: void consume(const char *)".to_string(),
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
        let mut document = sample_document(node);

        enrich_document(&mut document, Path::new("/tmp/project"));

        let analysis = document.diagnostics[0].analysis.as_ref().unwrap();
        assert_eq!(analysis.family.as_deref(), Some("type_overload"));
        assert_eq!(analysis.confidence_bucket(), Some(Confidence::High));
        assert!(
            analysis
                .matched_conditions
                .iter()
                .any(|condition| condition == "child_role=candidate")
        );
    }

    #[test]
    fn classifies_concepts_constraints_from_child_note_before_type_overload() {
        let mut node = sample_node("no matching function for call to 'consume(1)'");
        node.phase = Phase::Constraints;
        node.children.push(DiagnosticNode {
            id: "constraints".to_string(),
            origin: Origin::Gcc,
            phase: Phase::Constraints,
            severity: Severity::Note,
            semantic_role: SemanticRole::Supporting,
            message: MessageText {
                raw_text: "constraints not satisfied".to_string(),
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
        let mut document = sample_document(node);

        enrich_document(&mut document, Path::new("/tmp/project"));

        let analysis = document.diagnostics[0].analysis.as_ref().unwrap();
        assert_eq!(analysis.family.as_deref(), Some("concepts_constraints"));
        assert_eq!(
            analysis.rule_id.as_deref(),
            Some("rule.family.concepts_constraints.structured_or_message")
        );
        assert!(
            analysis
                .matched_conditions
                .iter()
                .any(|condition| condition == "child_message_contains=constraints not satisfied")
        );
    }

    #[test]
    fn classifies_template_from_context_chain_and_child_notes() {
        let mut node = sample_node("no matching function for call to 'expect_ptr(int&)'");
        node.phase = Phase::Instantiate;
        node.locations = vec![sample_location("src/main.cpp")];
        node.context_chains = vec![sample_context_chain(
            ContextChainKind::TemplateInstantiation,
            "required from here",
        )];
        node.children.push(DiagnosticNode {
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
        let mut document = sample_document(node);

        enrich_document(&mut document, Path::new("/tmp/project"));

        let analysis = document.diagnostics[0].analysis.as_ref().unwrap();
        assert_eq!(analysis.family.as_deref(), Some("template"));
        assert_eq!(
            analysis.rule_id.as_deref(),
            Some("rule.family.template.structured_or_message")
        );
        assert!(
            analysis
                .matched_conditions
                .iter()
                .any(|condition| condition == "context=template_instantiation")
        );
        assert_eq!(analysis.confidence_bucket(), Some(Confidence::High));
        assert_eq!(
            analysis.headline.as_deref(),
            Some("template instantiation failed")
        );
    }

    #[test]
    fn classifies_macro_include_from_context_chain() {
        let mut node = sample_node("'Box' has no member named 'missing_field'");
        node.context_chains = vec![sample_context_chain(
            ContextChainKind::MacroExpansion,
            "in expansion of macro 'READ_FIELD'",
        )];
        let mut document = sample_document(node);

        enrich_document(&mut document, Path::new("/tmp/project"));

        let analysis = document.diagnostics[0].analysis.as_ref().unwrap();
        assert_eq!(analysis.family.as_deref(), Some("macro_include"));
        assert_eq!(
            analysis.rule_id.as_deref(),
            Some("rule.family.macro_include.structured_or_message")
        );
        assert!(
            analysis
                .matched_conditions
                .iter()
                .any(|condition| condition == "context=macro_expansion")
        );
        assert_eq!(analysis.confidence_bucket(), Some(Confidence::High));
    }

    #[test]
    fn preserves_specific_linker_family_from_ingress() {
        let mut node = sample_node("collect2: error: ld returned 1 exit status");
        node.phase = Phase::Link;
        node.locations.clear();
        node.symbol_context = Some(SymbolContext {
            primary_symbol: Some("missing_symbol".to_string()),
            related_objects: Vec::new(),
            archive: None,
        });
        node.analysis = Some(AnalysisOverlay {
            family: Some("linker.undefined_reference".into()),
            family_version: None,
            family_confidence: None,
            root_cause_score: None,
            actionability_score: None,
            user_code_priority: None,
            headline: None,
            first_action_hint: None,
            confidence: None,
            preferred_primary_location_id: None,
            rule_id: None,
            matched_conditions: Vec::new(),
            suppression_reason: None,
            collapsed_child_ids: Vec::new(),
            collapsed_chain_ids: Vec::new(),
            group_ref: None,
            reasons: Vec::new(),
            policy_profile: None,
            producer_version: None,
        });
        let mut document = sample_document(node);

        enrich_document(&mut document, Path::new("/tmp/project"));

        let analysis = document.diagnostics[0].analysis.as_ref().unwrap();
        assert_eq!(
            analysis.family.as_deref(),
            Some("linker.undefined_reference")
        );
        assert_eq!(
            analysis.rule_id.as_deref(),
            Some("rule.family.ingress_specific_override")
        );
        assert_eq!(analysis.confidence_bucket(), Some(Confidence::Medium));
        assert_eq!(
            analysis.headline.as_deref(),
            Some("undefined reference to `missing_symbol`")
        );
        assert_eq!(
            analysis.first_action_hint.as_deref(),
            Some("define the missing symbol or link the object/library that provides it")
        );
    }

    #[test]
    fn preserves_specific_ingress_headline_and_action_without_local_override() {
        let mut node = sample_node("/usr/bin/ld: cannot find -lmissing");
        node.phase = Phase::Link;
        node.locations.clear();
        node.analysis = Some(AnalysisOverlay {
            family: Some("linker.cannot_find_library".into()),
            family_version: None,
            family_confidence: None,
            root_cause_score: None,
            actionability_score: None,
            user_code_priority: None,
            headline: Some("cannot find library `-lmissing`".into()),
            first_action_hint: Some(
                "check the library search path and whether the archive is installed".into(),
            ),
            confidence: None,
            preferred_primary_location_id: None,
            rule_id: None,
            matched_conditions: Vec::new(),
            suppression_reason: None,
            collapsed_child_ids: Vec::new(),
            collapsed_chain_ids: Vec::new(),
            group_ref: None,
            reasons: Vec::new(),
            policy_profile: None,
            producer_version: None,
        });
        let mut document = sample_document(node);

        enrich_document(&mut document, Path::new("/tmp/project"));

        let analysis = document.diagnostics[0].analysis.as_ref().unwrap();
        assert_eq!(
            analysis.family.as_deref(),
            Some("linker.cannot_find_library")
        );
        assert_eq!(
            analysis.headline.as_deref(),
            Some("cannot find library `-lmissing`")
        );
        assert_eq!(
            analysis.first_action_hint.as_deref(),
            Some("check the library search path and whether the archive is installed")
        );
        assert_eq!(analysis.confidence_bucket(), Some(Confidence::Medium));
    }

    #[test]
    fn annotates_passthrough_nodes_conservatively() {
        let mut node = sample_node("wrapper preserved stderr");
        node.semantic_role = SemanticRole::Passthrough;
        let mut document = sample_document(node);

        enrich_document(&mut document, Path::new("/tmp/project"));

        let analysis = document.diagnostics[0].analysis.as_ref().unwrap();
        assert_eq!(analysis.family.as_deref(), Some("passthrough"));
        assert_eq!(
            analysis.rule_id.as_deref(),
            Some("rule.family.passthrough.semantic_role")
        );
        assert_eq!(analysis.confidence_bucket(), Some(Confidence::Low));
        assert_eq!(
            analysis.headline.as_deref(),
            Some("showing conservative wrapper view")
        );
        assert_eq!(
            analysis.first_action_hint.as_deref(),
            Some("inspect the preserved raw diagnostics for the first corrective action")
        );
    }

    #[test]
    fn leaves_unmatched_diagnostics_as_unknown() {
        let node = sample_node("opaque compiler failure text");
        let mut document = sample_document(node);

        enrich_document(&mut document, Path::new("/tmp/project"));

        let analysis = document.diagnostics[0].analysis.as_ref().unwrap();
        assert_eq!(analysis.family.as_deref(), Some("unknown"));
        assert_eq!(analysis.rule_id.as_deref(), Some("rule.family.unknown"));
        assert_eq!(analysis.confidence_bucket(), Some(Confidence::Low));
        assert_eq!(
            analysis.suppression_reason.as_deref(),
            Some("generic_fallback")
        );
        assert_eq!(
            analysis.headline.as_deref(),
            Some("opaque compiler failure text")
        );
    }

    #[test]
    fn classifies_relative_and_absolute_paths_with_shared_rules() {
        let cwd = Path::new("/tmp/project");
        assert_eq!(classify_ownership("src/main.c", cwd).owner, Ownership::User);
        assert_eq!(
            classify_ownership("third_party/lib/foo.h", cwd).owner,
            Ownership::Vendor
        );
        assert_eq!(
            classify_ownership("generated/parser.generated.h", cwd).owner,
            Ownership::Generated
        );
        assert_eq!(
            classify_ownership("/usr/include/stdio.h", cwd).owner,
            Ownership::System
        );
        assert_eq!(
            classify_ownership("/tmp/project/build/generated.c", cwd).owner,
            Ownership::Generated
        );
        assert_eq!(
            classify_ownership("/opt/custom-sdk/include/foo.h", cwd).owner,
            Ownership::Unknown
        );
    }
}
