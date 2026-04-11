//! GCC diagnostic artifact ingestion adapter.
//!
//! Parses SARIF and stderr output produced by GCC and converts them into
//! [`diag_core::DiagnosticDocument`] instances defined in the core IR.
//!
//! Key entry points:
//! - [`ingest`] -- simplified one-shot conversion (SARIF path + stderr text).
//! - [`ingest_bundle`] -- full-fidelity ingestion from a [`diag_capture_runtime::CaptureBundle`].
//! - [`from_sarif`] -- parse a standalone SARIF file on disk.

mod classify;
mod fallback;
mod gcc_json;
mod ingest;
mod sarif;
mod stderr;

pub use fallback::{producer_for_version, tool_for_backend};
pub use ingest::{
    AdapterError, IngestOutcome, IngestPolicy, IngestReport, ingest, ingest_bundle,
    ingest_with_reason,
};
pub use sarif::from_sarif;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gcc_json::from_gcc_json_payload;
    use crate::ingest::compatibility_bundle_from_legacy_inputs;
    use diag_core::{
        ArtifactKind, ArtifactStorage, CaptureArtifact, Confidence, ContextChainKind,
        DocumentCompleteness, FallbackGrade, FallbackReason, LanguageMode, NodeCompleteness,
        ProvenanceSource, RunInfo, SemanticRole, SourceAuthority, WrapperSurface,
    };
    use diag_rulepack::checked_in_rulepack_version;
    use std::fs;

    fn base_run_info() -> RunInfo {
        RunInfo {
            invocation_id: "inv".to_string(),
            invoked_as: Some("gcc-formed".to_string()),
            argv_redacted: vec!["gcc".to_string()],
            cwd_display: None,
            exit_status: 1,
            primary_tool: tool_for_backend("gcc", Some("15.2.0".to_string())),
            secondary_tools: Vec::new(),
            language_mode: Some(LanguageMode::C),
            target_triple: None,
            wrapper_mode: Some(WrapperSurface::Terminal),
        }
    }

    #[test]
    fn producer_uses_checked_in_rulepack_version() {
        let producer = producer_for_version("0.1.0");
        assert_eq!(
            producer.rulepack_version.as_deref(),
            Some(checked_in_rulepack_version())
        );
    }

    #[test]
    fn parses_minimal_sarif() {
        let tempdir = tempfile::tempdir().unwrap();
        let path = tempdir.path().join("diag.sarif");
        fs::write(
            &path,
            r#"{
              "version":"2.1.0",
              "runs":[
                {
                  "results":[
                    {
                      "level":"error",
                      "message":{"text":"expected ';' before '}' token"},
                      "locations":[
                        {
                          "physicalLocation":{
                            "artifactLocation":{"uri":"src/main.c"},
                            "region":{"startLine":4,"startColumn":1}
                          }
                        }
                      ]
                    }
                  ]
                }
              ]
            }"#,
        )
        .unwrap();
        let document = from_sarif(&path, producer_for_version("0.1.0"), base_run_info()).unwrap();
        assert_eq!(document.diagnostics.len(), 1);
        assert_eq!(
            document.diagnostics[0].locations[0].path_raw(),
            "src/main.c"
        );
    }

    #[test]
    fn parses_minimal_gcc_json() {
        let document = from_gcc_json_payload(
            r#"[
              {
                "kind":"error",
                "message":"expected ';' before '}' token",
                "locations":[
                  {
                    "caret":{"file":"src/main.c","line":4,"column":1},
                    "finish":{"file":"src/main.c","line":4,"column":4}
                  }
                ]
              }
            ]"#,
            "diagnostics.json",
            producer_for_version("0.1.0"),
            base_run_info(),
        )
        .unwrap();

        assert_eq!(document.diagnostics.len(), 1);
        assert_eq!(
            document.diagnostics[0].locations[0].path_raw(),
            "src/main.c"
        );
        assert_eq!(document.diagnostics[0].locations[0].end_column(), Some(4));
        assert_eq!(
            document.diagnostics[0].provenance.capture_refs,
            vec!["diagnostics.json".to_string()]
        );
    }

    #[test]
    fn ignores_message_less_related_locations() {
        let tempdir = tempfile::tempdir().unwrap();
        let path = tempdir.path().join("diag.sarif");
        fs::write(
            &path,
            r#"{
              "version":"2.1.0",
              "runs":[
                {
                  "results":[
                    {
                      "level":"error",
                      "message":{"text":"'missing_symbol' undeclared"},
                      "locations":[
                        {
                          "physicalLocation":{
                            "artifactLocation":{"uri":"src/main.c"},
                            "region":{"startLine":3,"startColumn":25}
                          }
                        }
                      ],
                      "relatedLocations":[
                        {
                          "physicalLocation":{
                            "artifactLocation":{"uri":"src/main.c"},
                            "region":{"startLine":3,"startColumn":25}
                          },
                          "message":{"text":"each undeclared identifier is reported only once"}
                        },
                        {
                          "physicalLocation":{
                            "artifactLocation":{"uri":"src/wrapper.h"},
                            "region":{"startLine":1}
                          }
                        },
                        {
                          "physicalLocation":{
                            "artifactLocation":{"uri":"src/main.c"},
                            "region":{"startLine":1}
                          }
                        }
                      ]
                    }
                  ]
                }
              ]
            }"#,
        )
        .unwrap();
        let document = from_sarif(&path, producer_for_version("0.1.0"), base_run_info()).unwrap();

        assert_eq!(document.diagnostics.len(), 1);
        assert_eq!(document.diagnostics[0].children.len(), 1);
        assert_eq!(
            document.diagnostics[0].children[0].message.raw_text,
            "each undeclared identifier is reported only once"
        );
    }

    #[test]
    fn ignores_candidate_count_related_locations() {
        let tempdir = tempfile::tempdir().unwrap();
        let path = tempdir.path().join("diag.sarif");
        fs::write(
            &path,
            r#"{
              "version":"2.1.0",
              "runs":[
                {
                  "results":[
                    {
                      "level":"error",
                      "message":{"text":"no matching function for call to 'takes(int)'"},
                      "locations":[
                        {
                          "physicalLocation":{
                            "artifactLocation":{"uri":"src/main.cpp"},
                            "region":{"startLine":5,"startColumn":5}
                          }
                        }
                      ],
                      "relatedLocations":[
                        {
                          "physicalLocation":{
                            "artifactLocation":{"uri":"src/main.cpp"},
                            "region":{"startLine":5,"startColumn":5}
                          },
                          "message":{"text":"there are 2 candidates"}
                        },
                        {
                          "physicalLocation":{
                            "artifactLocation":{"uri":"src/main.cpp"},
                            "region":{"startLine":1,"startColumn":6}
                          },
                          "message":{"text":"candidate 1: 'void takes(int, int)'"}
                        }
                      ]
                    }
                  ]
                }
              ]
            }"#,
        )
        .unwrap();
        let document = from_sarif(
            &path,
            producer_for_version("0.1.0"),
            RunInfo {
                argv_redacted: vec!["g++".to_string()],
                primary_tool: tool_for_backend("g++", Some("15.2.0".to_string())),
                language_mode: Some(LanguageMode::Cpp),
                ..base_run_info()
            },
        )
        .unwrap();

        assert_eq!(document.diagnostics[0].children.len(), 1);
        assert_eq!(
            document.diagnostics[0].children[0].message.raw_text,
            "candidate 1: 'void takes(int, int)'"
        );
    }

    #[test]
    fn parses_gcc_json_children_as_structured_notes() {
        let document = from_gcc_json_payload(
            r#"[
              {
                "kind":"error",
                "message":"no matching function for call to 'takes(int)'",
                "locations":[
                  {
                    "caret":{"file":"src/main.cpp","line":5,"column":5}
                  }
                ],
                "children":[
                  {
                    "kind":"note",
                    "message":"candidate 1: 'void takes(int, int)'",
                    "locations":[
                      {
                        "caret":{"file":"src/main.cpp","line":1,"column":6}
                      }
                    ]
                  }
                ]
              }
            ]"#,
            "diagnostics.json",
            producer_for_version("0.1.0"),
            RunInfo {
                argv_redacted: vec!["g++".to_string()],
                primary_tool: tool_for_backend("g++", Some("12.3.0".to_string())),
                language_mode: Some(LanguageMode::Cpp),
                ..base_run_info()
            },
        )
        .unwrap();

        assert_eq!(document.diagnostics[0].children.len(), 1);
        assert_eq!(
            document.diagnostics[0].children[0].semantic_role,
            SemanticRole::Candidate
        );
        assert_eq!(
            document.diagnostics[0]
                .analysis
                .as_ref()
                .and_then(|analysis| analysis.family.as_deref()),
            Some("type_overload")
        );
    }

    #[test]
    fn fail_opens_when_authoritative_sarif_is_missing() {
        let tempdir = tempfile::tempdir().unwrap();
        let path = tempdir.path().join("missing.sarif");
        let outcome = ingest_with_reason(
            Some(&path),
            "src/main.c:4:1: error: expected ';' before '}' token\n",
            producer_for_version("0.1.0"),
            base_run_info(),
        )
        .unwrap();

        assert_eq!(outcome.fallback_reason, Some(FallbackReason::SarifMissing));
        assert_eq!(
            outcome.document.document_completeness,
            DocumentCompleteness::Partial
        );
        assert!(outcome.document.diagnostics.iter().any(|node| {
            matches!(node.semantic_role, SemanticRole::Root)
                && node
                    .message
                    .raw_text
                    .contains("expected ';' before '}' token")
                && node.primary_location().is_some()
        }));
        assert!(
            outcome.document.integrity_issues[0]
                .message
                .contains("authoritative SARIF was not produced")
        );
    }

    #[test]
    fn fail_opens_when_authoritative_sarif_is_invalid() {
        let tempdir = tempfile::tempdir().unwrap();
        let path = tempdir.path().join("diag.sarif");
        fs::write(&path, "{\"version\":").unwrap();
        let outcome = ingest_with_reason(
            Some(&path),
            "src/main.c:4:1: error: expected ';' before '}' token\n",
            producer_for_version("0.1.0"),
            base_run_info(),
        )
        .unwrap();

        assert_eq!(
            outcome.fallback_reason,
            Some(FallbackReason::SarifParseFailed)
        );
        assert_eq!(
            outcome.document.document_completeness,
            DocumentCompleteness::Failed
        );
        assert_eq!(outcome.document.diagnostics.len(), 2);
        assert!(outcome.document.diagnostics.iter().any(|node| {
            node.provenance.source == ProvenanceSource::ResidualText
                && node.primary_location().is_some()
                && node
                    .analysis
                    .as_ref()
                    .and_then(|analysis| analysis.family.as_deref())
                    == Some("syntax")
        }));
        assert!(outcome.document.diagnostics.iter().any(|node| {
            matches!(node.semantic_role, SemanticRole::Passthrough)
                && node
                    .message
                    .raw_text
                    .contains("expected ';' before '}' token")
        }));
        assert!(
            outcome.document.integrity_issues[0]
                .message
                .contains("failed to parse authoritative SARIF")
        );
    }

    #[test]
    fn ingest_bundle_reports_structured_authority_for_valid_sarif() {
        let tempdir = tempfile::tempdir().unwrap();
        let path = tempdir.path().join("diag.sarif");
        fs::write(
            &path,
            r#"{
              "version":"2.1.0",
              "runs":[
                {
                  "results":[
                    {
                      "level":"error",
                      "message":{"text":"expected ';' before '}' token"}
                    }
                  ]
                }
              ]
            }"#,
        )
        .unwrap();
        let run = base_run_info();
        let report = ingest_bundle(
            &compatibility_bundle_from_legacy_inputs(Some(&path), "", &run),
            IngestPolicy {
                producer: producer_for_version("0.1.0"),
                run,
            },
        )
        .unwrap();

        assert_eq!(report.source_authority, SourceAuthority::Structured);
        assert_eq!(report.fallback_grade, FallbackGrade::None);
        assert_eq!(report.confidence_ceiling, Confidence::Medium);
        assert!(report.fallback_reason.is_none());
        assert!(report.warnings.is_empty());
        assert_eq!(report.document.diagnostics.len(), 1);
        assert_eq!(report.document.captures.len(), 1);
        assert_eq!(report.document.captures[0].id, "diagnostics.sarif");
        assert!(report.document.validate().is_ok());
    }

    #[test]
    fn ingest_bundle_scopes_stderr_context_augmentation_per_matching_root() {
        let run = base_run_info();
        let stderr = "\
In file included from src/wrapper_a.h:1,\n\
                 from src/main.c:1:\n\
src/config_a.h:1:23: error: first missing symbol\n\
src/main.c:3:25: note: in expansion of macro 'FETCH_A'\n\
In file included from src/wrapper_b.h:2,\n\
                 from src/other.c:1:\n\
src/config_b.h:2:11: error: second missing symbol\n\
src/other.c:8:9: note: in expansion of macro 'FETCH_B'\n";
        let sarif = r#"{
          "version":"2.1.0",
          "runs":[
            {
              "results":[
                {
                  "level":"error",
                  "message":{"text":"first missing symbol"},
                  "locations":[
                    {
                      "physicalLocation":{
                        "artifactLocation":{"uri":"src/main.c"},
                        "region":{"startLine":3,"startColumn":25}
                      }
                    }
                  ]
                },
                {
                  "level":"error",
                  "message":{"text":"second missing symbol"},
                  "locations":[
                    {
                      "physicalLocation":{
                        "artifactLocation":{"uri":"src/other.c"},
                        "region":{"startLine":8,"startColumn":9}
                      }
                    }
                  ]
                }
              ]
            }
          ]
        }"#;
        let mut bundle = compatibility_bundle_from_legacy_inputs(None, stderr, &run);
        bundle.structured_artifacts.push(CaptureArtifact {
            id: "diagnostics.sarif".to_string(),
            kind: ArtifactKind::GccSarif,
            media_type: "application/sarif+json".to_string(),
            encoding: Some("utf-8".to_string()),
            digest_sha256: None,
            size_bytes: Some(sarif.len() as u64),
            storage: ArtifactStorage::Inline,
            inline_text: Some(sarif.to_string()),
            external_ref: None,
            produced_by: Some(run.primary_tool.clone()),
        });

        let report = ingest_bundle(
            &bundle,
            IngestPolicy {
                producer: producer_for_version("0.1.0"),
                run,
            },
        )
        .unwrap();

        assert_eq!(report.document.diagnostics.len(), 2);
        let first = report
            .document
            .diagnostics
            .iter()
            .find(|node| node.message.raw_text == "first missing symbol")
            .unwrap();
        let second = report
            .document
            .diagnostics
            .iter()
            .find(|node| node.message.raw_text == "second missing symbol")
            .unwrap();
        let first_include = first
            .context_chains
            .iter()
            .find(|chain| matches!(chain.kind, ContextChainKind::Include))
            .unwrap();
        let second_include = second
            .context_chains
            .iter()
            .find(|chain| matches!(chain.kind, ContextChainKind::Include))
            .unwrap();
        let first_macro = first
            .context_chains
            .iter()
            .find(|chain| matches!(chain.kind, ContextChainKind::MacroExpansion))
            .unwrap();
        let second_macro = second
            .context_chains
            .iter()
            .find(|chain| matches!(chain.kind, ContextChainKind::MacroExpansion))
            .unwrap();

        assert!(
            first_include
                .frames
                .iter()
                .any(|frame| frame.path.as_deref() == Some("src/wrapper_a.h"))
        );
        assert!(
            !first_include
                .frames
                .iter()
                .any(|frame| frame.path.as_deref() == Some("src/wrapper_b.h"))
        );
        assert!(
            second_include
                .frames
                .iter()
                .any(|frame| frame.path.as_deref() == Some("src/wrapper_b.h"))
        );
        assert!(
            !second_include
                .frames
                .iter()
                .any(|frame| frame.path.as_deref() == Some("src/wrapper_a.h"))
        );
        assert!(
            first_macro
                .frames
                .iter()
                .any(|frame| frame.label.contains("FETCH_A"))
        );
        assert!(
            !first_macro
                .frames
                .iter()
                .any(|frame| frame.label.contains("FETCH_B"))
        );
        assert!(
            second_macro
                .frames
                .iter()
                .any(|frame| frame.label.contains("FETCH_B"))
        );
        assert!(
            !second_macro
                .frames
                .iter()
                .any(|frame| frame.label.contains("FETCH_A"))
        );
        assert!(report.document.validate().is_ok());
    }

    #[test]
    fn ingest_bundle_keeps_structured_compiler_residuals_when_passthrough_is_suppressed() {
        let tempdir = tempfile::tempdir().unwrap();
        let path = tempdir.path().join("diag.sarif");
        fs::write(
            &path,
            r#"{
              "version":"2.1.0",
              "runs":[
                {
                  "results":[
                    {
                      "level":"error",
                      "message":{"text":"'missing_symbol' undeclared"},
                      "locations":[
                        {
                          "physicalLocation":{
                            "artifactLocation":{"uri":"src/main.c"},
                            "region":{"startLine":3,"startColumn":25}
                          }
                        }
                      ]
                    }
                  ]
                }
              ]
            }"#,
        )
        .unwrap();
        let run = base_run_info();
        let stderr = "\
src/main.cpp:5:7: error: no matching function for call to 'takes(int)'\n\
src/main.cpp:2:6: note: candidate 1: 'void takes(int, int)'\n";
        let report = ingest_bundle(
            &compatibility_bundle_from_legacy_inputs(Some(&path), stderr, &run),
            IngestPolicy {
                producer: producer_for_version("0.1.0"),
                run,
            },
        )
        .unwrap();

        assert_eq!(report.source_authority, SourceAuthority::Structured);
        assert_eq!(report.fallback_grade, FallbackGrade::None);
        assert_eq!(report.confidence_ceiling, Confidence::Medium);
        assert!(report.fallback_reason.is_none());
        assert!(
            report.document.diagnostics.len() >= 2,
            "expected SARIF and residual diagnostics to both be present"
        );
        assert!(report.document.diagnostics.iter().any(|node| {
            node.provenance.source == ProvenanceSource::Compiler
                && node
                    .message
                    .raw_text
                    .contains("'missing_symbol' undeclared")
        }));
        assert!(report.document.diagnostics.iter().any(|node| {
            node.provenance.source == ProvenanceSource::ResidualText
                && node
                    .analysis
                    .as_ref()
                    .and_then(|analysis| analysis.family.as_deref())
                    == Some("type_overload")
        }));
        assert!(
            !report
                .document
                .diagnostics
                .iter()
                .any(|node| { matches!(node.semantic_role, SemanticRole::Passthrough) })
        );
    }

    #[test]
    fn ingest_bundle_reports_structured_authority_for_valid_gcc_json() {
        let tempdir = tempfile::tempdir().unwrap();
        let path = tempdir.path().join("diagnostics.json");
        fs::write(
            &path,
            r#"[
              {
                "kind":"error",
                "message":"expected ';' before '}' token",
                "locations":[
                  {
                    "caret":{"file":"src/main.c","line":4,"column":1}
                  }
                ]
              }
            ]"#,
        )
        .unwrap();
        let run = base_run_info();
        let mut bundle = compatibility_bundle_from_legacy_inputs(None, "", &run);
        bundle.structured_artifacts.push(CaptureArtifact {
            id: "diagnostics.json".to_string(),
            kind: ArtifactKind::GccJson,
            media_type: "application/json".to_string(),
            encoding: Some("utf-8".to_string()),
            digest_sha256: None,
            size_bytes: None,
            storage: ArtifactStorage::ExternalRef,
            inline_text: None,
            external_ref: Some(path.display().to_string()),
            produced_by: Some(run.primary_tool.clone()),
        });

        let report = ingest_bundle(
            &bundle,
            IngestPolicy {
                producer: producer_for_version("0.1.0"),
                run,
            },
        )
        .unwrap();

        assert_eq!(report.source_authority, SourceAuthority::Structured);
        assert_eq!(report.fallback_grade, FallbackGrade::None);
        assert_eq!(report.confidence_ceiling, Confidence::Medium);
        assert!(report.fallback_reason.is_none());
        assert!(report.warnings.is_empty());
        assert_eq!(
            report.document.document_completeness,
            DocumentCompleteness::Complete
        );
        assert_eq!(report.document.diagnostics.len(), 1);
        assert_eq!(
            report.document.diagnostics[0].provenance.capture_refs,
            vec!["diagnostics.json".to_string()]
        );
        assert_eq!(report.document.captures.len(), 1);
        assert_eq!(report.document.captures[0].id, "diagnostics.json");
        assert!(report.document.validate().is_ok());
    }

    #[test]
    fn ingest_bundle_accepts_residual_only_path() {
        let run = base_run_info();
        let report = ingest_bundle(
            &compatibility_bundle_from_legacy_inputs(
                None,
                "src/main.c:4:1: error: expected ';' before '}' token\n",
                &run,
            ),
            IngestPolicy {
                producer: producer_for_version("0.1.0"),
                run,
            },
        )
        .unwrap();

        assert_eq!(report.source_authority, SourceAuthority::ResidualText);
        assert_eq!(report.fallback_grade, FallbackGrade::Compatibility);
        assert_eq!(report.confidence_ceiling, Confidence::Low);
        assert!(report.fallback_reason.is_none());
        assert_eq!(
            report.document.document_completeness,
            DocumentCompleteness::Partial
        );
        assert!(report.document.diagnostics.iter().any(|node| {
            matches!(node.semantic_role, SemanticRole::Root)
                && node
                    .message
                    .raw_text
                    .contains("expected ';' before '}' token")
                && node.primary_location().is_some()
        }));
    }

    #[test]
    fn ingest_bundle_recognizes_type_overload_residual_useful_subset() {
        let run = base_run_info();
        let stderr = "\
src/main.cpp:5:7: error: no matching function for call to 'takes(int)'\n\
src/main.cpp:2:6: note: candidate 1: 'void takes(int, int)'\n";
        let report = ingest_bundle(
            &compatibility_bundle_from_legacy_inputs(None, stderr, &run),
            IngestPolicy {
                producer: producer_for_version("0.1.0"),
                run,
            },
        )
        .unwrap();

        assert_eq!(report.source_authority, SourceAuthority::ResidualText);
        assert_eq!(report.fallback_grade, FallbackGrade::Compatibility);
        assert_eq!(report.confidence_ceiling, Confidence::Low);
        assert_eq!(report.fallback_reason, None);
        assert_eq!(
            report.document.document_completeness,
            DocumentCompleteness::Partial
        );
        assert_eq!(report.document.diagnostics.len(), 1);
        assert_eq!(
            report.document.diagnostics[0]
                .analysis
                .as_ref()
                .and_then(|analysis| analysis.family.as_deref()),
            Some("type_overload")
        );
        assert!(
            report.document.diagnostics[0]
                .children
                .iter()
                .any(|child| matches!(child.semantic_role, SemanticRole::Candidate))
        );
    }

    #[test]
    fn ingest_bundle_recognizes_template_residual_useful_subset() {
        let run = base_run_info();
        let stderr = "\
src/main.cpp:8:15: error: no matching function for call to 'expect_ptr(int&)'\n\
src/main.cpp:3:7: note: template argument deduction/substitution failed:\n\
src/main.cpp:8:15: note:   required from here\n";
        let report = ingest_bundle(
            &compatibility_bundle_from_legacy_inputs(None, stderr, &run),
            IngestPolicy {
                producer: producer_for_version("0.1.0"),
                run,
            },
        )
        .unwrap();

        assert_eq!(report.source_authority, SourceAuthority::ResidualText);
        assert_eq!(report.fallback_grade, FallbackGrade::Compatibility);
        assert_eq!(report.confidence_ceiling, Confidence::Low);
        assert_eq!(report.fallback_reason, None);
        assert_eq!(
            report.document.document_completeness,
            DocumentCompleteness::Partial
        );
        assert_eq!(
            report.document.diagnostics[0]
                .analysis
                .as_ref()
                .and_then(|analysis| analysis.family.as_deref()),
            Some("template")
        );
        assert!(
            report.document.diagnostics[0]
                .context_chains
                .iter()
                .any(|chain| matches!(chain.kind, ContextChainKind::TemplateInstantiation))
        );
    }

    #[test]
    fn ingest_bundle_recognizes_linker_residual_useful_subset() {
        let run = base_run_info();
        let stderr = "\
/usr/bin/ld: main.o: in function `main':\n\
main.c:(.text+0x15): undefined reference to `foo`\n";
        let report = ingest_bundle(
            &compatibility_bundle_from_legacy_inputs(None, stderr, &run),
            IngestPolicy {
                producer: producer_for_version("0.1.0"),
                run,
            },
        )
        .unwrap();

        assert_eq!(report.source_authority, SourceAuthority::ResidualText);
        assert_eq!(report.fallback_grade, FallbackGrade::Compatibility);
        assert_eq!(report.confidence_ceiling, Confidence::Low);
        assert_eq!(report.fallback_reason, None);
        assert_eq!(
            report.document.document_completeness,
            DocumentCompleteness::Partial
        );
        assert_eq!(
            report.document.diagnostics[0]
                .analysis
                .as_ref()
                .and_then(|analysis| analysis.family.as_deref()),
            Some("linker.undefined_reference")
        );
    }

    #[test]
    fn ingest_bundle_keeps_unclassified_residuals_on_passthrough_document() {
        let run = base_run_info();
        let report = ingest_bundle(
            &compatibility_bundle_from_legacy_inputs(
                None,
                "totally unstructured compiler output\n",
                &run,
            ),
            IngestPolicy {
                producer: producer_for_version("0.1.0"),
                run,
            },
        )
        .unwrap();

        assert_eq!(
            report.document.document_completeness,
            DocumentCompleteness::Passthrough
        );
        assert!(report.document.diagnostics.iter().any(|node| {
            matches!(node.semantic_role, SemanticRole::Passthrough)
                && node
                    .message
                    .raw_text
                    .contains("totally unstructured compiler output")
        }));
        assert_eq!(report.fallback_grade, FallbackGrade::FailOpen);
        assert_eq!(report.fallback_reason, Some(FallbackReason::ResidualOnly));
    }

    #[test]
    fn ingest_bundle_fail_opens_on_opaque_compiler_residuals() {
        let run = base_run_info();
        let stderr = "\
src/main.c:4:1: error: opaque compiler wording here\n\
src/main.c:4:1: note: extra opaque detail\n";
        let report = ingest_bundle(
            &compatibility_bundle_from_legacy_inputs(None, stderr, &run),
            IngestPolicy {
                producer: producer_for_version("0.1.0"),
                run,
            },
        )
        .unwrap();

        assert_eq!(report.source_authority, SourceAuthority::ResidualText);
        assert_eq!(report.fallback_grade, FallbackGrade::FailOpen);
        assert_eq!(report.confidence_ceiling, Confidence::Low);
        assert_eq!(report.fallback_reason, Some(FallbackReason::ResidualOnly));
        assert_eq!(
            report.document.document_completeness,
            DocumentCompleteness::Passthrough
        );
        assert!(report.document.diagnostics.iter().any(|node| {
            matches!(node.semantic_role, SemanticRole::Passthrough)
                && node
                    .message
                    .raw_text
                    .contains("opaque compiler wording here")
        }));
    }

    #[test]
    fn ingest_bundle_marks_partial_for_incomplete_gcc_json() {
        let run = base_run_info();
        let tempdir = tempfile::tempdir().unwrap();
        let path = tempdir.path().join("diagnostics.json");
        fs::write(
            &path,
            r#"[
              {
                "kind":"error",
                "message":"expected ';' before '}' token"
              }
            ]"#,
        )
        .unwrap();
        let mut bundle = compatibility_bundle_from_legacy_inputs(None, "", &run);
        bundle.structured_artifacts.push(CaptureArtifact {
            id: "diagnostics.json".to_string(),
            kind: ArtifactKind::GccJson,
            media_type: "application/json".to_string(),
            encoding: Some("utf-8".to_string()),
            digest_sha256: None,
            size_bytes: None,
            storage: ArtifactStorage::ExternalRef,
            inline_text: None,
            external_ref: Some(path.display().to_string()),
            produced_by: Some(run.primary_tool.clone()),
        });

        let report = ingest_bundle(
            &bundle,
            IngestPolicy {
                producer: producer_for_version("0.1.0"),
                run,
            },
        )
        .unwrap();

        assert_eq!(report.source_authority, SourceAuthority::Structured);
        assert_eq!(report.fallback_grade, FallbackGrade::None);
        assert_eq!(report.confidence_ceiling, Confidence::Medium);
        assert!(report.fallback_reason.is_none());
        assert_eq!(
            report.document.document_completeness,
            DocumentCompleteness::Partial
        );
        assert_eq!(
            report.document.diagnostics[0].node_completeness,
            NodeCompleteness::Partial
        );
    }

    #[test]
    fn ingest_bundle_fail_opens_on_invalid_gcc_json() {
        let run = base_run_info();
        let tempdir = tempfile::tempdir().unwrap();
        let path = tempdir.path().join("diagnostics.json");
        fs::write(&path, "[").unwrap();
        let stderr = "src/main.c:4:1: error: expected ';' before '}' token\n";
        let mut bundle = compatibility_bundle_from_legacy_inputs(None, stderr, &run);
        bundle.structured_artifacts.push(CaptureArtifact {
            id: "diagnostics.json".to_string(),
            kind: ArtifactKind::GccJson,
            media_type: "application/json".to_string(),
            encoding: Some("utf-8".to_string()),
            digest_sha256: None,
            size_bytes: None,
            storage: ArtifactStorage::ExternalRef,
            inline_text: None,
            external_ref: Some(path.display().to_string()),
            produced_by: Some(run.primary_tool.clone()),
        });

        let report = ingest_bundle(
            &bundle,
            IngestPolicy {
                producer: producer_for_version("0.1.0"),
                run,
            },
        )
        .unwrap();

        assert_eq!(report.source_authority, SourceAuthority::ResidualText);
        assert_eq!(report.fallback_grade, FallbackGrade::FailOpen);
        assert_eq!(report.confidence_ceiling, Confidence::Low);
        assert!(report.fallback_reason.is_none());
        assert_eq!(
            report.document.document_completeness,
            DocumentCompleteness::Failed
        );
        assert!(report.document.integrity_issues.iter().any(|issue| {
            issue
                .message
                .contains("failed to parse structured GCC JSON")
        }));
        assert!(report.document.diagnostics.iter().any(|node| {
            node.message
                .raw_text
                .contains("expected ';' before '}' token")
        }));
    }

    #[test]
    fn ingest_with_reason_matches_bundle_report_for_missing_sarif() {
        let tempdir = tempfile::tempdir().unwrap();
        let path = tempdir.path().join("missing.sarif");
        let stderr = "src/main.c:4:1: error: expected ';' before '}' token\n";
        let run = base_run_info();
        let report = ingest_bundle(
            &compatibility_bundle_from_legacy_inputs(Some(&path), stderr, &run),
            IngestPolicy {
                producer: producer_for_version("0.1.0"),
                run: run.clone(),
            },
        )
        .unwrap();
        let outcome =
            ingest_with_reason(Some(&path), stderr, producer_for_version("0.1.0"), run).unwrap();

        assert_eq!(outcome.fallback_reason, report.fallback_reason);
        assert_eq!(
            outcome.document.document_completeness,
            report.document.document_completeness
        );
        assert_eq!(
            outcome.document.diagnostics.len(),
            report.document.diagnostics.len()
        );
        assert_eq!(
            outcome.document.integrity_issues,
            report.document.integrity_issues
        );
        assert_eq!(
            report
                .document
                .captures
                .iter()
                .map(|artifact| artifact.id.as_str())
                .collect::<Vec<_>>(),
            vec!["stderr.raw", "diagnostics.sarif"]
        );
        assert_eq!(
            outcome
                .document
                .captures
                .iter()
                .map(|artifact| artifact.id.as_str())
                .collect::<Vec<_>>(),
            vec!["stderr.raw", "diagnostics.sarif"]
        );
        assert!(report.document.validate().is_ok());
        assert!(outcome.document.validate().is_ok());
    }
}
