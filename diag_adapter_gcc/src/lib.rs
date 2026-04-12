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
mod context;
mod fallback;
mod fixits;
mod gcc_json;
mod ingest;
mod sarif;
mod stderr;

pub use diag_adapter_contract::{DiagnosticAdapter, IngestPolicy, IngestReport};
use diag_core::TextEdit;
use serde_json::Value;

pub use fallback::{producer_for_version, tool_for_backend};
pub use ingest::{
    AdapterError, GccAdapter, IngestOutcome, ingest, ingest_bundle, ingest_with_reason,
};
pub use sarif::from_sarif;

/// Returns the string value at `key`, or `""` if absent or not a string.
pub(crate) fn json_str<'a>(v: &'a Value, key: &str) -> &'a str {
    v.get(key).and_then(Value::as_str).unwrap_or("")
}

/// Returns the `u64` value at `key`, if present and numeric.
pub(crate) fn json_u64(v: &Value, key: &str) -> Option<u64> {
    v.get(key).and_then(Value::as_u64)
}

/// Returns the `u32` value at `key`, if present, numeric, and in range.
pub(crate) fn json_u32(v: &Value, key: &str) -> Option<u32> {
    json_u64(v, key).and_then(|value| u32::try_from(value).ok())
}

pub(crate) fn is_valid_text_edit(edit: &TextEdit) -> bool {
    edit.start_line >= 1
        && edit.start_column >= 1
        && edit.end_line >= 1
        && edit.end_column >= 1
        && point_leq(
            edit.start_line,
            edit.start_column,
            edit.end_line,
            edit.end_column,
        )
}

pub(crate) fn text_edits_overlap(edits: &[TextEdit]) -> bool {
    for (index, left) in edits.iter().enumerate() {
        for right in edits.iter().skip(index + 1) {
            if left.path != right.path {
                continue;
            }
            if edit_ranges_overlap(left, right) {
                return true;
            }
        }
    }
    false
}

fn edit_ranges_overlap(left: &TextEdit, right: &TextEdit) -> bool {
    let left_start = (left.start_line, left.start_column);
    let left_end = (left.end_line, left.end_column);
    let right_start = (right.start_line, right.start_column);
    let right_end = (right.end_line, right.end_column);

    if left_start == left_end && right_start == right_end {
        return left_start == right_start;
    }

    !(left_end <= right_start || right_end <= left_start)
}

fn point_leq(start_line: u32, start_column: u32, end_line: u32, end_column: u32) -> bool {
    (start_line, start_column) <= (end_line, end_column)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gcc_json::from_gcc_json_payload;
    use crate::ingest::compatibility_bundle_from_legacy_inputs;
    use diag_core::{
        ArtifactKind, ArtifactStorage, CaptureArtifact, Confidence, ContextChainKind,
        DocumentCompleteness, FallbackGrade, FallbackReason, LanguageMode, NodeCompleteness,
        Origin, Phase, ProvenanceSource, RunInfo, SemanticRole, SourceAuthority,
        SuggestionApplicability, WrapperSurface,
    };
    use diag_rulepack::checked_in_rulepack_version;
    use std::fs;
    use std::path::PathBuf;

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

    fn corpus_fixture(relative: &str) -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join(relative)
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
    fn gcc_adapter_reports_supported_origins() {
        let adapter = GccAdapter;
        assert!(adapter.supported_origins().contains(&Origin::Gcc));
        assert!(adapter.supported_origins().contains(&Origin::Driver));
        assert!(adapter.supported_origins().contains(&Origin::Linker));
        assert!(adapter.supported_origins().contains(&Origin::Wrapper));
        assert!(adapter.supported_origins().contains(&Origin::ExternalTool));
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
            &producer_for_version("0.1.0"),
            &base_run_info(),
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
    fn parses_sarif_fix_suggestions() {
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
                      ],
                      "fixes":[
                        {
                          "description":{"text":"insert missing punctuation"},
                          "artifactChanges":[
                            {
                              "artifactLocation":{"uri":"src/main.c"},
                              "replacements":[
                                {
                                  "deletedRegion":{"startLine":4,"startColumn":12,"endColumn":12},
                                  "insertedContent":{"text":";"}
                                }
                              ]
                            },
                            {
                              "artifactLocation":{"uri":"include/common.h"},
                              "replacements":[
                                {
                                  "deletedRegion":{"startLine":2,"startColumn":1,"endLine":2,"endColumn":7},
                                  "insertedContent":{"text":"static "}
                                }
                              ]
                            }
                          ]
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
        let suggestions = &document.diagnostics[0].suggestions;

        assert_eq!(suggestions.len(), 1);
        assert_eq!(suggestions[0].label, "insert missing punctuation");
        assert_eq!(
            suggestions[0].applicability,
            SuggestionApplicability::Manual
        );
        assert_eq!(suggestions[0].edits.len(), 2);
        assert_eq!(suggestions[0].edits[0].path, "src/main.c");
        assert_eq!(suggestions[0].edits[0].start_line, 4);
        assert_eq!(suggestions[0].edits[0].end_line, 4);
        assert_eq!(suggestions[0].edits[0].replacement, ";");
        assert_eq!(suggestions[0].edits[1].path, "include/common.h");
        assert_eq!(suggestions[0].edits[1].replacement, "static ");
    }

    #[test]
    fn drops_invalid_sarif_fix_suggestions() {
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
                      ],
                      "fixes":[
                        {
                          "artifactChanges":[
                            {
                              "artifactLocation":{"uri":"src/main.c"},
                              "replacements":[
                                {
                                  "deletedRegion":{"startLine":4,"startColumn":12,"endColumn":11},
                                  "insertedContent":{"text":";"}
                                }
                              ]
                            }
                          ]
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
        assert!(document.diagnostics[0].suggestions.is_empty());
    }

    #[test]
    fn parses_gcc_json_fixits_into_suggestions() {
        let document = from_gcc_json_payload(
            r#"[
              {
                "kind":"error",
                "message":"expected ';' before '}' token",
                "locations":[
                  {
                    "caret":{"file":"src/main.c","line":4,"column":1}
                  }
                ],
                "fixits":[
                  {
                    "start":{"file":"src/main.c","line":4,"column":12},
                    "next":{"file":"src/main.c","line":4,"column":12},
                    "string":";"
                  },
                  {
                    "start":{"file":"src/main.c","line":4,"column":5},
                    "next":{"file":"src/main.c","line":4,"column":11},
                    "string":"return"
                  }
                ]
              }
            ]"#,
            "diagnostics.json",
            &producer_for_version("0.1.0"),
            &base_run_info(),
        )
        .unwrap();

        let suggestions = &document.diagnostics[0].suggestions;
        assert_eq!(suggestions.len(), 2);
        assert_eq!(
            suggestions[0].applicability,
            SuggestionApplicability::MachineApplicable
        );
        assert_eq!(suggestions[0].edits[0].replacement, ";");
        assert_eq!(suggestions[1].edits[0].start_column, 5);
        assert_eq!(suggestions[1].edits[0].end_column, 11);
        assert_eq!(suggestions[1].edits[0].replacement, "return");
    }

    #[test]
    fn parses_fixit_hints_compatibility_and_drops_invalid_regions() {
        let document = from_gcc_json_payload(
            r#"[
              {
                "kind":"error",
                "message":"expected ';' before '}' token",
                "locations":[
                  {
                    "caret":{"file":"src/main.c","line":4,"column":1}
                  }
                ],
                "fixit-hints":[
                  {
                    "start":{"file":"src/main.c","line":4,"column":12},
                    "next":{"file":"src/main.c","line":4,"column":12},
                    "string":";"
                  },
                  {
                    "start":{"file":"src/main.c","line":4,"column":9},
                    "next":{"file":"src/main.c","line":4,"column":8},
                    "string":"broken"
                  }
                ]
              }
            ]"#,
            "diagnostics.json",
            &producer_for_version("0.1.0"),
            &base_run_info(),
        )
        .unwrap();

        let suggestions = &document.diagnostics[0].suggestions;
        assert_eq!(suggestions.len(), 1);
        assert_eq!(suggestions[0].edits[0].replacement, ";");
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
    fn parses_markdown_only_sarif_messages_and_related_locations() {
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
                      "message":{"markdown":"no matching function for call to `expect_ptr(int&)`"},
                      "locations":[
                        {
                          "physicalLocation":{
                            "artifactLocation":{"uri":"src/main.cpp"},
                            "region":{"startLine":6,"startColumn":15}
                          }
                        }
                      ],
                      "relatedLocations":[
                        {
                          "physicalLocation":{
                            "artifactLocation":{"uri":"src/main.cpp"},
                            "region":{"startLine":6,"startColumn":15}
                          },
                          "message":{"markdown":"there is 1 candidate"}
                        },
                        {
                          "physicalLocation":{
                            "artifactLocation":{"uri":"src/main.cpp"},
                            "region":{"startLine":2,"startColumn":6}
                          },
                          "message":{"markdown":"candidate 1: `template<class T> void expect_ptr(T*)`"}
                        },
                        {
                          "physicalLocation":{
                            "artifactLocation":{"uri":"src/main.cpp"},
                            "region":{"startLine":2,"startColumn":6}
                          },
                          "message":{"markdown":"template argument deduction/substitution failed:"}
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

        let root = &document.diagnostics[0];
        assert_eq!(
            root.message.raw_text,
            "no matching function for call to `expect_ptr(int&)`"
        );
        assert_eq!(root.phase, Phase::Instantiate);
        assert_eq!(
            root.analysis
                .as_ref()
                .and_then(|analysis| analysis.family.as_deref()),
            Some("template")
        );
        assert_eq!(root.children.len(), 2);
        assert_eq!(
            root.children[0].message.raw_text,
            "candidate 1: `template<class T> void expect_ptr(T*)`"
        );
        assert_eq!(
            root.children[1].message.raw_text,
            "template argument deduction/substitution failed:"
        );
    }

    #[test]
    fn prefers_non_empty_markdown_when_text_field_is_blank() {
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
                      "message":{"text":"   ","markdown":"expected ';' before '}' token"},
                      "locations":[
                        {
                          "physicalLocation":{
                            "artifactLocation":{"uri":"src/main.c"},
                            "region":{"startLine":4,"startColumn":1}
                          }
                        }
                      ],
                      "relatedLocations":[
                        {
                          "physicalLocation":{
                            "artifactLocation":{"uri":"src/main.c"},
                            "region":{"startLine":3,"startColumn":1}
                          },
                          "message":{"text":"","markdown":"to match this '{'"}
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

        let root = &document.diagnostics[0];
        assert_eq!(root.message.raw_text, "expected ';' before '}' token");
        assert_eq!(root.children.len(), 1);
        assert_eq!(root.children[0].message.raw_text, "to match this '{'");
    }

    #[test]
    fn defaults_overflowing_sarif_location_values_without_wrapping() {
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
                            "region":{
                              "startLine":4294967296,
                              "startColumn":4294967296,
                              "endLine":4294967297,
                              "endColumn":4294967297
                            }
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

        let location = document.diagnostics[0].primary_location().unwrap();
        assert_eq!(location.line(), 1);
        assert_eq!(location.column(), 1);
        assert!(document.diagnostics[0].locations[0].range.is_none());
    }

    #[test]
    fn prefers_non_empty_markdown_for_gcc_json_message_objects() {
        let document = from_gcc_json_payload(
            r#"[
              {
                "kind":"error",
                "message":{"text":"", "markdown":"expected ';' before '}' token"},
                "locations":[
                  {
                    "caret":{"file":"src/main.c","line":4,"column":1}
                  }
                ],
                "children":[
                  {
                    "kind":"note",
                    "message":{"text":"  ", "markdown":"to match this '{'"},
                    "locations":[
                      {
                        "caret":{"file":"src/main.c","line":3,"column":1}
                      }
                    ]
                  }
                ]
              }
            ]"#,
            "diagnostics.json",
            &producer_for_version("0.1.0"),
            &base_run_info(),
        )
        .unwrap();

        let root = &document.diagnostics[0];
        assert_eq!(root.message.raw_text, "expected ';' before '}' token");
        assert_eq!(root.children.len(), 1);
        assert_eq!(root.children[0].message.raw_text, "to match this '{'");
    }

    #[test]
    fn defaults_overflowing_gcc_json_location_values_without_wrapping() {
        let document = from_gcc_json_payload(
            r#"[
              {
                "kind":"error",
                "message":"expected ';' before '}' token",
                "locations":[
                  {
                    "caret":{
                      "file":"src/main.c",
                      "line":4294967296,
                      "column":4294967296
                    },
                    "finish":{
                      "file":"src/main.c",
                      "line":4294967297,
                      "column":4294967297
                    }
                  }
                ]
              }
            ]"#,
            "diagnostics.json",
            &producer_for_version("0.1.0"),
            &base_run_info(),
        )
        .unwrap();

        let location = document.diagnostics[0].primary_location().unwrap();
        assert_eq!(location.line(), 1);
        assert_eq!(location.column(), 1);
        assert!(document.diagnostics[0].locations[0].range.is_none());
    }

    #[test]
    fn classifies_sarif_required_by_substitution_as_template_context() {
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
                      "message":{"text":"no matching function for call to 'expect_ptr(int&)'"},
                      "locations":[
                        {
                          "physicalLocation":{
                            "artifactLocation":{"uri":"src/main.cpp"},
                            "region":{"startLine":8,"startColumn":15}
                          }
                        }
                      ],
                      "relatedLocations":[
                        {
                          "physicalLocation":{
                            "artifactLocation":{"uri":"src/main.cpp"},
                            "region":{"startLine":8,"startColumn":15}
                          },
                          "message":{"text":"  required by substitution of 'template<class T> void expect_ptr(T*) [with T = int]'"}}
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

        let root = &document.diagnostics[0];
        assert_eq!(root.phase, Phase::Instantiate);
        assert_eq!(
            root.analysis
                .as_ref()
                .and_then(|analysis| analysis.family.as_deref()),
            Some("template")
        );
        assert!(
            root.context_chains
                .iter()
                .any(|chain| matches!(chain.kind, ContextChainKind::TemplateInstantiation))
        );
    }

    #[test]
    fn classifies_gcc_json_required_by_substitution_as_template_context() {
        let document = from_gcc_json_payload(
            r#"[
              {
                "kind":"error",
                "message":"no matching function for call to 'expect_ptr(int&)'",
                "locations":[
                  {
                    "caret":{"file":"src/main.cpp","line":8,"column":15}
                  }
                ],
                "children":[
                  {
                    "kind":"note",
                    "message":"  required by substitution of 'template<class T> void expect_ptr(T*) [with T = int]'",
                    "locations":[
                      {
                        "caret":{"file":"src/main.cpp","line":8,"column":15}
                      }
                    ]
                  }
                ]
              }
            ]"#,
            "diagnostics.json",
            &producer_for_version("0.1.0"),
            &RunInfo {
                argv_redacted: vec!["g++".to_string()],
                primary_tool: tool_for_backend("g++", Some("12.3.0".to_string())),
                language_mode: Some(LanguageMode::Cpp),
                ..base_run_info()
            },
        )
        .unwrap();

        let root = &document.diagnostics[0];
        assert_eq!(root.phase, Phase::Instantiate);
        assert_eq!(
            root.analysis
                .as_ref()
                .and_then(|analysis| analysis.family.as_deref()),
            Some("template")
        );
        assert_eq!(root.children.len(), 1);
        assert_eq!(root.children[0].phase, Phase::Instantiate);
    }

    #[test]
    fn parses_sarif_codeflows_into_template_context_frames() {
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
                      "message":{"text":"no matching function for call to 'expect_ptr(int&)'"},
                      "taxa":[
                        {
                          "id":"cpp.template_instantiation",
                          "shortDescription":{"text":"template instantiation backtrace"}
                        }
                      ],
                      "locations":[
                        {
                          "physicalLocation":{
                            "artifactLocation":{"uri":"src/header.hpp"},
                            "region":{"startLine":3,"startColumn":7}
                          }
                        }
                      ],
                      "codeFlows":[
                        {
                          "threadFlows":[
                            {
                              "locations":[
                                {
                                  "location":{
                                    "physicalLocation":{
                                      "artifactLocation":{"uri":"src/header.hpp"},
                                      "region":{"startLine":3,"startColumn":7}
                                    },
                                    "message":{"text":"In instantiation of 'void expect_ptr(T*) [with T = int]'"}
                                  }
                                },
                                {
                                  "location":{
                                    "physicalLocation":{
                                      "artifactLocation":{"uri":"src/main.cpp"},
                                      "region":{"startLine":8,"startColumn":15}
                                    },
                                    "message":{"text":"required from here"}
                                  }
                                }
                              ]
                            }
                          ]
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

        let root = &document.diagnostics[0];
        let template_chain = root
            .context_chains
            .iter()
            .find(|chain| matches!(chain.kind, ContextChainKind::TemplateInstantiation))
            .unwrap();

        assert_eq!(root.phase, Phase::Instantiate);
        assert_eq!(template_chain.frames.len(), 2);
        assert_eq!(
            template_chain.frames[0].path.as_deref(),
            Some("src/header.hpp")
        );
        assert!(
            template_chain.frames[0]
                .label
                .contains("In instantiation of")
        );
        assert_eq!(
            template_chain.frames[1].path.as_deref(),
            Some("src/main.cpp")
        );
        assert_eq!(template_chain.frames[1].line, Some(8));
    }

    #[test]
    fn sarif_context_fallback_ignores_negative_include_phrases() {
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
                      "message":{"text":"record does not include member 'missing'"},
                      "locations":[
                        {
                          "physicalLocation":{
                            "artifactLocation":{"uri":"src/main.cpp"},
                            "region":{"startLine":8,"startColumn":15}
                          }
                        }
                      ],
                      "relatedLocations":[
                        {
                          "physicalLocation":{
                            "artifactLocation":{"uri":"src/main.cpp"},
                            "region":{"startLine":8,"startColumn":15}
                          },
                          "message":{"text":"record does not include member 'missing'"}
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

        let root = &document.diagnostics[0];
        assert!(
            !root
                .context_chains
                .iter()
                .any(|chain| matches!(chain.kind, ContextChainKind::Include))
        );
    }

    #[test]
    fn gcc_json_option_drives_template_context() {
        let document = from_gcc_json_payload(
            r#"[
              {
                "kind":"error",
                "option":"-Wtemplate-body",
                "message":"call expression is ill-formed",
                "locations":[
                  {
                    "caret":{"file":"src/main.cpp","line":8,"column":15}
                  }
                ]
              }
            ]"#,
            "diagnostics.json",
            &producer_for_version("0.1.0"),
            &RunInfo {
                argv_redacted: vec!["g++".to_string()],
                primary_tool: tool_for_backend("g++", Some("12.3.0".to_string())),
                language_mode: Some(LanguageMode::Cpp),
                ..base_run_info()
            },
        )
        .unwrap();

        let root = &document.diagnostics[0];
        assert_eq!(root.phase, Phase::Instantiate);
        assert!(
            root.context_chains
                .iter()
                .any(|chain| matches!(chain.kind, ContextChainKind::TemplateInstantiation))
        );
    }

    #[test]
    fn gcc_json_option_drives_analyzer_phase() {
        let document = from_gcc_json_payload(
            r#"[
              {
                "kind":"warning",
                "option":"-Wanalyzer-null-dereference",
                "message":"dereference of NULL 'ptr'",
                "locations":[
                  {
                    "caret":{"file":"src/main.c","line":8,"column":15}
                  }
                ]
              }
            ]"#,
            "diagnostics.json",
            &producer_for_version("0.1.0"),
            &RunInfo {
                argv_redacted: vec!["gcc".to_string()],
                primary_tool: tool_for_backend("gcc", Some("15.2.0".to_string())),
                language_mode: Some(LanguageMode::C),
                ..base_run_info()
            },
        )
        .unwrap();

        let root = &document.diagnostics[0];
        assert_eq!(root.phase, Phase::Analyze);
        assert!(
            root.context_chains
                .iter()
                .any(|chain| matches!(chain.kind, ContextChainKind::AnalyzerPath))
        );
    }

    #[test]
    fn gcc_json_hash_error_maps_to_preprocess_phase() {
        let document = from_gcc_json_payload(
            r##"[
              {
                "kind":"error",
                "message":"#error stop here",
                "locations":[
                  {
                    "caret":{"file":"src/config.h","line":3,"column":2}
                  }
                ]
              }
            ]"##,
            "diagnostics.json",
            &producer_for_version("0.1.0"),
            &RunInfo {
                argv_redacted: vec!["gcc".to_string()],
                primary_tool: tool_for_backend("gcc", Some("15.2.0".to_string())),
                language_mode: Some(LanguageMode::C),
                ..base_run_info()
            },
        )
        .unwrap();

        assert_eq!(document.diagnostics[0].phase, Phase::Preprocess);
    }

    #[test]
    fn sarif_rule_id_drives_constraints_phase() {
        let tempdir = tempfile::tempdir().unwrap();
        let path = tempdir.path().join("diag.sarif");
        fs::write(
            &path,
            r#"{
              "version":"2.1.0",
              "runs":[
                {
                  "invocations":[
                    {
                      "arguments":["/usr/local/libexec/gcc/x86_64-linux-gnu/15.2.0/cc1plus"]
                    }
                  ],
                  "results":[
                    {
                      "ruleId":"-Wconcepts",
                      "level":"error",
                      "message":{"text":"constraints not satisfied"},
                      "locations":[
                        {
                          "physicalLocation":{
                            "artifactLocation":{"uri":"src/main.cpp"},
                            "region":{"startLine":8,"startColumn":15}
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

        assert_eq!(document.diagnostics[0].phase, Phase::Constraints);
    }

    #[test]
    fn sarif_run_tool_component_drives_internal_compiler_phase() {
        let tempdir = tempfile::tempdir().unwrap();
        let path = tempdir.path().join("diag.sarif");
        fs::write(
            &path,
            r#"{
              "version":"2.1.0",
              "runs":[
                {
                  "invocations":[
                    {
                      "arguments":["/usr/local/libexec/gcc/x86_64-linux-gnu/15.2.0/cc1plus"]
                    }
                  ],
                  "results":[
                    {
                      "ruleId":"error",
                      "level":"error",
                      "message":{"text":"internal compiler error: unexpected failure\nduring rtl pass: expand"},
                      "locations":[
                        {
                          "physicalLocation":{
                            "artifactLocation":{"uri":"src/main.cpp"},
                            "region":{"startLine":8,"startColumn":15}
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

        assert_eq!(document.diagnostics[0].phase, Phase::Codegen);
    }

    #[test]
    fn parses_gcc_json_context_chains_from_recursive_note_children() {
        let document = from_gcc_json_payload(
            r#"[
              {
                "kind":"error",
                "message":"no matching function for call to 'expect_ptr(int&)'",
                "locations":[
                  {
                    "caret":{"file":"src/header.hpp","line":3,"column":7}
                  }
                ],
                "children":[
                  {
                    "kind":"note",
                    "message":"In instantiation of 'void expect_ptr(T*) [with T = int]'",
                    "locations":[
                      {
                        "caret":{"file":"src/header.hpp","line":3,"column":7}
                      }
                    ],
                    "children":[
                      {
                        "kind":"note",
                        "message":"  required from here",
                        "locations":[
                          {
                            "caret":{"file":"src/main.cpp","line":8,"column":15}
                          }
                        ]
                      }
                    ]
                  }
                ]
              }
            ]"#,
            "diagnostics.json",
            &producer_for_version("0.1.0"),
            &RunInfo {
                argv_redacted: vec!["g++".to_string()],
                primary_tool: tool_for_backend("g++", Some("12.3.0".to_string())),
                language_mode: Some(LanguageMode::Cpp),
                ..base_run_info()
            },
        )
        .unwrap();

        let root = &document.diagnostics[0];
        let template_chain = root
            .context_chains
            .iter()
            .find(|chain| matches!(chain.kind, ContextChainKind::TemplateInstantiation))
            .unwrap();

        assert_eq!(template_chain.frames.len(), 2);
        assert_eq!(
            template_chain.frames[0].path.as_deref(),
            Some("src/header.hpp")
        );
        assert_eq!(
            template_chain.frames[1].path.as_deref(),
            Some("src/main.cpp")
        );
        assert!(
            template_chain.frames[1]
                .label
                .contains("required from here")
        );
    }

    #[test]
    fn sarif_root_phase_uses_related_messages_for_linker_context() {
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
                      "message":{"text":"collect2: error: ld returned 1 exit status"},
                      "locations":[
                        {
                          "physicalLocation":{
                            "artifactLocation":{"uri":"src/main.c"},
                            "region":{"startLine":8,"startColumn":1}
                          }
                        }
                      ],
                      "relatedLocations":[
                        {
                          "physicalLocation":{
                            "artifactLocation":{"uri":"src/main.c"},
                            "region":{"startLine":8,"startColumn":1}
                          },
                          "message":{"text":"undefined reference to `missing_symbol`"}
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

        let root = &document.diagnostics[0];
        assert_eq!(root.phase, Phase::Link);
        assert_eq!(
            root.analysis
                .as_ref()
                .and_then(|analysis| analysis.family.as_deref()),
            Some("linker.undefined_reference")
        );
    }

    #[test]
    fn marks_template_deduction_child_notes_as_instantiate_phase() {
        let document = from_gcc_json_payload(
            r#"[
              {
                "kind":"error",
                "message":"no matching function for call to 'takes_same(int, double)'",
                "locations":[
                  {
                    "caret":{"file":"src/main.cpp","line":4,"column":24}
                  }
                ],
                "children":[
                  {
                    "kind":"note",
                    "message":"  deduced conflicting types for parameter 'T' ('int' and 'double')",
                    "locations":[
                      {
                        "caret":{"file":"src/main.cpp","line":4,"column":24}
                      }
                    ]
                  }
                ]
              }
            ]"#,
            "diagnostics.json",
            &producer_for_version("0.1.0"),
            &RunInfo {
                argv_redacted: vec!["g++".to_string()],
                primary_tool: tool_for_backend("g++", Some("15.2.0".to_string())),
                language_mode: Some(LanguageMode::Cpp),
                ..base_run_info()
            },
        )
        .unwrap();

        let root = &document.diagnostics[0];
        assert_eq!(
            root.analysis
                .as_ref()
                .and_then(|analysis| analysis.family.as_deref()),
            Some("template")
        );
        assert_eq!(root.children.len(), 1);
        assert_eq!(root.children[0].phase, Phase::Instantiate);
    }

    #[test]
    fn structured_multiple_definition_uses_specific_first_action_hint() {
        let document = from_gcc_json_payload(
            r#"[
              {
                "kind":"error",
                "message":"helper.c:(.text+0x0): multiple definition of `duplicate_symbol'; main.c:(.text+0x0): first defined here",
                "locations":[
                  {
                    "caret":{"file":"helper.c","line":1,"column":1}
                  }
                ]
              }
            ]"#,
            "diagnostics.json",
            &producer_for_version("0.1.0"),
            &base_run_info(),
        )
        .unwrap();

        let root = &document.diagnostics[0];
        assert_eq!(root.phase, Phase::Link);
        assert_eq!(
            root.analysis
                .as_ref()
                .and_then(|analysis| analysis.family.as_deref()),
            Some("linker.multiple_definition")
        );
        assert_eq!(
            root.analysis
                .as_ref()
                .and_then(|analysis| analysis.first_action_hint.as_deref()),
            Some(
                "remove the duplicate definition or make the symbol internal to one translation unit"
            )
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
            &producer_for_version("0.1.0"),
            &RunInfo {
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
    fn parses_corpus_sarif_fixture_from_gcc15_overload_case() {
        let document = from_sarif(
            &corpus_fixture("corpus/cpp/overload/case-02/snapshots/gcc15/diagnostics.sarif"),
            producer_for_version("0.1.0"),
            RunInfo {
                argv_redacted: vec!["g++".to_string()],
                primary_tool: tool_for_backend("g++", Some("15.2.0".to_string())),
                language_mode: Some(LanguageMode::Cpp),
                ..base_run_info()
            },
        )
        .unwrap();

        assert!(document.diagnostics.len() >= 2);
        let conversion = document
            .diagnostics
            .iter()
            .find(|node| node.message.raw_text.contains("invalid conversion"))
            .unwrap();
        assert_eq!(conversion.locations[0].path_raw(), "src/main.cpp");
        assert_eq!(conversion.locations[0].column(), 49);
        assert_eq!(conversion.locations[0].end_column(), None);
        assert_eq!(
            conversion
                .analysis
                .as_ref()
                .and_then(|analysis| analysis.family.as_deref()),
            Some("type_overload")
        );
    }

    #[test]
    fn parses_corpus_gcc_json_fixture_from_gcc9_12_overload_case() {
        let json = fs::read_to_string(corpus_fixture(
            "corpus/cpp/overload/case-07/snapshots/gcc9_12/single_sink_structured/diagnostics.json",
        ))
        .unwrap();
        let document = from_gcc_json_payload(
            &json,
            "diagnostics.json",
            &producer_for_version("0.1.0"),
            &RunInfo {
                argv_redacted: vec!["g++".to_string()],
                primary_tool: tool_for_backend("g++", Some("12.3.0".to_string())),
                language_mode: Some(LanguageMode::Cpp),
                ..base_run_info()
            },
        )
        .unwrap();

        assert_eq!(document.diagnostics.len(), 2);
        let conversion = document
            .diagnostics
            .iter()
            .find(|node| node.message.raw_text.contains("invalid conversion"))
            .unwrap();
        assert_eq!(conversion.locations[0].path_raw(), "src/main.cpp");
        assert_eq!(conversion.locations[0].end_column(), Some(19));
        assert!(
            document
                .diagnostics
                .iter()
                .any(|node| node.message.raw_text.contains("initializing argument 1"))
        );
    }

    #[test]
    fn rejects_sarif_payload_without_runs_array() {
        let tempdir = tempfile::tempdir().unwrap();
        let path = tempdir.path().join("diag.sarif");
        fs::write(&path, r#"{"version":"2.1.0"}"#).unwrap();

        let error = from_sarif(&path, producer_for_version("0.1.0"), base_run_info()).unwrap_err();

        assert!(matches!(error, AdapterError::MissingRuns));
    }

    #[test]
    fn rejects_unsupported_sarif_version() {
        let tempdir = tempfile::tempdir().unwrap();
        let path = tempdir.path().join("diag.sarif");
        fs::write(&path, r#"{"version":"1.0.0","runs":[]}"#).unwrap();

        let error = from_sarif(&path, producer_for_version("0.1.0"), base_run_info()).unwrap_err();

        assert!(matches!(
            error,
            AdapterError::UnsupportedVersion(version) if version == "1.0.0"
        ));
    }

    #[test]
    fn parses_gcc_json_context_chains_from_nested_children() {
        let document = from_gcc_json_payload(
            r#"[
              {
                "kind":"error",
                "message":"no matching function for call to 'expect_ptr(int&)'",
                "locations":[
                  {
                    "caret":{"file":"src/main.cpp","line":8,"column":15}
                  }
                ],
                "children":[
                  {
                    "kind":"note",
                    "message":"template argument deduction/substitution failed:",
                    "locations":[
                      {
                        "caret":{"file":"src/main.cpp","line":3,"column":7}
                      }
                    ]
                  },
                  {
                    "kind":"note",
                    "message":"  required from here",
                    "locations":[
                      {
                        "caret":{"file":"src/main.cpp","line":8,"column":15}
                      }
                    ]
                  },
                  {
                    "kind":"note",
                    "message":"In file included from src/header.hpp:1",
                    "locations":[
                      {
                        "caret":{"file":"src/header.hpp","line":1,"column":1}
                      }
                    ]
                  }
                ]
              }
            ]"#,
            "diagnostics.json",
            &producer_for_version("0.1.0"),
            &RunInfo {
                argv_redacted: vec!["g++".to_string()],
                primary_tool: tool_for_backend("g++", Some("12.3.0".to_string())),
                language_mode: Some(LanguageMode::Cpp),
                ..base_run_info()
            },
        )
        .unwrap();

        let root = &document.diagnostics[0];
        let template_chain = root
            .context_chains
            .iter()
            .find(|chain| matches!(chain.kind, ContextChainKind::TemplateInstantiation))
            .unwrap();
        let include_chain = root
            .context_chains
            .iter()
            .find(|chain| matches!(chain.kind, ContextChainKind::Include))
            .unwrap();

        assert_eq!(
            root.analysis
                .as_ref()
                .and_then(|analysis| analysis.family.as_deref()),
            Some("template")
        );
        assert_eq!(template_chain.frames.len(), 2);
        assert!(
            template_chain
                .frames
                .iter()
                .any(|frame| frame.label.contains("required from here"))
        );
        assert_eq!(include_chain.frames.len(), 1);
        assert_eq!(
            include_chain.frames[0].path.as_deref(),
            Some("src/header.hpp")
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
    fn ingest_bundle_prefers_later_available_sarif_over_earlier_missing_placeholder() {
        let run = base_run_info();
        let sarif = r#"{
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
        }"#;
        let mut bundle = compatibility_bundle_from_legacy_inputs(None, "", &run);
        bundle.structured_artifacts.push(CaptureArtifact {
            id: "diagnostics.sarif.missing".to_string(),
            kind: ArtifactKind::GccSarif,
            media_type: "application/sarif+json".to_string(),
            encoding: Some("utf-8".to_string()),
            digest_sha256: None,
            size_bytes: None,
            storage: ArtifactStorage::Unavailable,
            inline_text: None,
            external_ref: None,
            produced_by: Some(run.primary_tool.clone()),
        });
        bundle.structured_artifacts.push(CaptureArtifact {
            id: "diagnostics.sarif.inline".to_string(),
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

        assert_eq!(report.source_authority, SourceAuthority::Structured);
        assert!(report.fallback_reason.is_none());
        assert_eq!(
            report.document.diagnostics[0].provenance.capture_refs,
            vec!["diagnostics.sarif.inline".to_string()]
        );
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
    fn ingest_bundle_augments_template_context_from_stderr_instantiation_frames() {
        let run = RunInfo {
            argv_redacted: vec!["g++".to_string()],
            primary_tool: tool_for_backend("g++", Some("15.2.0".to_string())),
            language_mode: Some(LanguageMode::Cpp),
            ..base_run_info()
        };
        let stderr = "\
src/header.hpp: In instantiation of 'void expect_ptr(T*) [with T = int]':\n\
src/main.cpp:8:15: note:   required from here\n\
src/header.hpp:3:7: error: no matching function for call to 'expect_ptr(int&)'\n";
        let sarif = r#"{
          "version":"2.1.0",
          "runs":[
            {
              "results":[
                {
                  "level":"error",
                  "message":{"text":"no matching function for call to 'expect_ptr(int&)'"},
                  "locations":[
                    {
                      "physicalLocation":{
                        "artifactLocation":{"uri":"src/header.hpp"},
                        "region":{"startLine":3,"startColumn":7}
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

        let root = &report.document.diagnostics[0];
        let template_chain = root
            .context_chains
            .iter()
            .find(|chain| matches!(chain.kind, ContextChainKind::TemplateInstantiation))
            .unwrap();

        assert_eq!(template_chain.frames.len(), 2);
        assert!(
            template_chain
                .frames
                .iter()
                .any(|frame| frame.label.contains("In instantiation of"))
        );
        assert!(
            template_chain
                .frames
                .iter()
                .any(|frame| frame.label.contains("required from here"))
        );
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
    fn ingest_bundle_deduplicates_matching_structured_and_residual_compiler_roots() {
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
                      "message":{"text":"use of deleted function 'NoCopy::NoCopy(const NoCopy&)'" },
                      "locations":[
                        {
                          "physicalLocation":{
                            "artifactLocation":{"uri":"src/main.cpp"},
                            "region":{"startLine":10,"startColumn":5}
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
src/main.cpp:10:9: error: use of deleted function 'NoCopy::NoCopy(const NoCopy&)'\n\
src/main.cpp:3:5: note: declared here\n";
        let report = ingest_bundle(
            &compatibility_bundle_from_legacy_inputs(Some(&path), stderr, &run),
            IngestPolicy {
                producer: producer_for_version("0.1.0"),
                run,
            },
        )
        .unwrap();

        assert_eq!(report.source_authority, SourceAuthority::Structured);
        assert_eq!(
            report
                .document
                .diagnostics
                .iter()
                .filter(|node| {
                    node.provenance.source == ProvenanceSource::Compiler
                        && node.message.raw_text
                            == "use of deleted function 'NoCopy::NoCopy(const NoCopy&)'"
                })
                .count(),
            1
        );
        assert!(!report.document.diagnostics.iter().any(|node| {
            node.provenance.source == ProvenanceSource::ResidualText
                && node
                    .analysis
                    .as_ref()
                    .and_then(|analysis| analysis.family.as_deref())
                    == Some("deleted_function")
        }));
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
    fn ingest_bundle_prefers_later_available_gcc_json_over_earlier_missing_placeholder() {
        let run = base_run_info();
        let json = r#"[
          {
            "kind":"error",
            "message":"expected ';' before '}' token",
            "locations":[
              {
                "caret":{"file":"src/main.c","line":4,"column":1}
              }
            ]
          }
        ]"#;
        let mut bundle = compatibility_bundle_from_legacy_inputs(None, "", &run);
        bundle.structured_artifacts.push(CaptureArtifact {
            id: "diagnostics.json.missing".to_string(),
            kind: ArtifactKind::GccJson,
            media_type: "application/json".to_string(),
            encoding: Some("utf-8".to_string()),
            digest_sha256: None,
            size_bytes: None,
            storage: ArtifactStorage::Unavailable,
            inline_text: None,
            external_ref: None,
            produced_by: Some(run.primary_tool.clone()),
        });
        bundle.structured_artifacts.push(CaptureArtifact {
            id: "diagnostics.json.inline".to_string(),
            kind: ArtifactKind::GccJson,
            media_type: "application/json".to_string(),
            encoding: Some("utf-8".to_string()),
            digest_sha256: None,
            size_bytes: Some(json.len() as u64),
            storage: ArtifactStorage::Inline,
            inline_text: Some(json.to_string()),
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

        assert_eq!(report.source_authority, SourceAuthority::Structured);
        assert!(report.fallback_reason.is_none());
        assert_eq!(
            report.document.diagnostics[0].provenance.capture_refs,
            vec!["diagnostics.json.inline".to_string()]
        );
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
