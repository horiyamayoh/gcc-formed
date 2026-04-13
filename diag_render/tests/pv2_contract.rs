use diag_core::{
    AnalysisOverlay, ArtifactKind, ArtifactStorage, CaptureArtifact, CascadePolicySnapshot,
    Confidence, DiagnosticDocument, DiagnosticEpisode, DiagnosticNode, DocumentAnalysis,
    DocumentCompleteness, EpisodeGraph, GroupCascadeAnalysis, GroupCascadeRole, Location,
    MessageText, NodeCompleteness, Origin, Ownership, Phase, ProducerInfo, Provenance,
    ProvenanceSource, RunInfo, SemanticRole, Severity, ToolInfo, VisibilityFloor,
};
use diag_render::{
    DebugRefs, PathPolicy, RenderCapabilities, RenderProfile, RenderRequest, SourceExcerptPolicy,
    StreamKind, TypeDisplayPolicy, WarningVisibility,
    build_presentation_snapshot_with_presentation_policy, build_view_model, render,
    render_with_presentation_policy,
};
use std::fs;
use std::path::{Path, PathBuf};

fn fixture_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/pv2_contract")
}

fn fixture_path(relative: &str) -> PathBuf {
    fixture_dir().join(relative)
}

fn load_fixture(relative: &str) -> String {
    fs::read_to_string(fixture_path(relative)).unwrap()
}

fn assert_fixture_eq(relative: &str, actual: &str) {
    let path = fixture_path(relative);
    if std::env::var_os("BLESS").is_some() {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(&path, actual).unwrap();
    }
    let expected = fs::read_to_string(&path).unwrap();
    assert_eq!(actual, expected, "snapshot mismatch for {}", path.display());
}

fn ownership_reason(ownership: Ownership) -> &'static str {
    match ownership {
        Ownership::User => "user_workspace",
        Ownership::Vendor => "vendor_path",
        Ownership::System => "system_path",
        Ownership::Generated => "generated_path",
        Ownership::Tool => "tool_generated",
        Ownership::Unknown => "unknown",
    }
}

fn sample_location(path: &str, line: u32, column: u32, ownership: Ownership) -> Location {
    Location::caret(path, line, column, diag_core::LocationRole::Primary)
        .with_ownership(ownership, ownership_reason(ownership))
}

fn sample_analysis(
    family: &str,
    headline: &str,
    first_action_hint: Option<&str>,
    confidence: Confidence,
    rule_id: &str,
) -> AnalysisOverlay {
    AnalysisOverlay {
        family: Some(family.to_string().into()),
        family_version: None,
        family_confidence: None,
        root_cause_score: None,
        actionability_score: None,
        user_code_priority: None,
        headline: Some(headline.to_string().into()),
        first_action_hint: first_action_hint.map(|value| value.to_string().into()),
        confidence: Some(confidence.score()),
        preferred_primary_location_id: None,
        rule_id: Some(rule_id.to_string().into()),
        matched_conditions: Vec::new(),
        suppression_reason: None,
        collapsed_child_ids: Vec::new(),
        collapsed_chain_ids: Vec::new(),
        group_ref: None,
        reasons: Vec::new(),
        policy_profile: None,
        producer_version: None,
    }
}

fn base_request(cwd: &Path, profile: RenderProfile) -> RenderRequest {
    RenderRequest {
        document: DiagnosticDocument {
            document_id: "issue-181".to_string(),
            schema_version: "1".to_string(),
            document_completeness: DocumentCompleteness::Complete,
            producer: ProducerInfo {
                name: "gcc-formed".to_string(),
                version: "0.2.0-beta.1".to_string(),
                git_revision: None,
                build_profile: None,
                rulepack_version: None,
            },
            run: RunInfo {
                invocation_id: "issue-181".to_string(),
                invoked_as: Some("gcc-formed".to_string()),
                argv_redacted: vec![
                    "gcc".to_string(),
                    "-c".to_string(),
                    "src/main.c".to_string(),
                ],
                cwd_display: Some(cwd.display().to_string()),
                exit_status: 1,
                primary_tool: ToolInfo {
                    name: "gcc".to_string(),
                    version: Some("15.1.0".to_string()),
                    component: None,
                    vendor: Some("GNU".to_string()),
                },
                secondary_tools: Vec::new(),
                language_mode: Some(diag_core::LanguageMode::C),
                target_triple: None,
                wrapper_mode: Some(diag_core::WrapperSurface::Terminal),
            },
            captures: vec![CaptureArtifact {
                id: "stderr.raw".to_string(),
                kind: ArtifactKind::CompilerStderrText,
                media_type: "text/plain".to_string(),
                encoding: Some("utf-8".to_string()),
                digest_sha256: None,
                size_bytes: Some(64),
                storage: ArtifactStorage::Inline,
                inline_text: Some("stderr".to_string()),
                external_ref: None,
                produced_by: None,
            }],
            integrity_issues: Vec::new(),
            diagnostics: Vec::new(),
            document_analysis: None,
            fingerprints: None,
        },
        cascade_policy: CascadePolicySnapshot {
            max_expanded_independent_roots: 1,
            ..Default::default()
        },
        profile,
        capabilities: RenderCapabilities {
            stream_kind: if matches!(profile, RenderProfile::Ci) {
                StreamKind::CiLog
            } else {
                StreamKind::Pipe
            },
            width_columns: Some(100),
            ansi_color: false,
            unicode: false,
            hyperlinks: false,
            interactive: false,
        },
        cwd: Some(cwd.to_path_buf()),
        path_policy: PathPolicy::RelativeToCwd,
        warning_visibility: WarningVisibility::Auto,
        debug_refs: DebugRefs::None,
        type_display_policy: TypeDisplayPolicy::CompactSafe,
        source_excerpt_policy: SourceExcerptPolicy::ForceOn,
    }
}

fn diagnostic_node(
    id: &str,
    path: &str,
    line: u32,
    column: u32,
    message: &str,
    severity: Severity,
    semantic_role: SemanticRole,
    phase: Phase,
) -> DiagnosticNode {
    DiagnosticNode {
        id: id.to_string(),
        origin: Origin::Gcc,
        phase,
        severity,
        semantic_role,
        message: MessageText {
            raw_text: message.to_string(),
            normalized_text: None,
            locale: None,
        },
        locations: vec![sample_location(path, line, column, Ownership::User)],
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

fn score(value: f32) -> diag_core::Score {
    value.into()
}

fn episode(
    episode_ref: &str,
    lead_group_ref: &str,
    member_group_refs: Vec<&str>,
    lead_root_score: f32,
) -> DiagnosticEpisode {
    DiagnosticEpisode {
        episode_ref: episode_ref.to_string(),
        lead_group_ref: lead_group_ref.to_string(),
        member_group_refs: member_group_refs
            .into_iter()
            .map(ToString::to_string)
            .collect(),
        family: Some("syntax".to_string()),
        lead_root_score: Some(score(lead_root_score)),
        confidence: Some(score(0.9)),
    }
}

fn lead_root_group(
    group_ref: &str,
    episode_ref: &str,
    root_score: f32,
    independence_score: f32,
) -> GroupCascadeAnalysis {
    GroupCascadeAnalysis {
        group_ref: group_ref.to_string(),
        episode_ref: Some(episode_ref.to_string()),
        role: GroupCascadeRole::LeadRoot,
        best_parent_group_ref: None,
        root_score: Some(score(root_score)),
        independence_score: Some(score(independence_score)),
        suppress_likelihood: Some(score(0.08)),
        summary_likelihood: Some(score(0.14)),
        visibility_floor: VisibilityFloor::NeverHidden,
        evidence_tags: vec!["user_owned_primary".to_string()],
    }
}

fn dependent_group(
    group_ref: &str,
    episode_ref: &str,
    parent_group_ref: &str,
    role: GroupCascadeRole,
) -> GroupCascadeAnalysis {
    GroupCascadeAnalysis {
        group_ref: group_ref.to_string(),
        episode_ref: Some(episode_ref.to_string()),
        role,
        best_parent_group_ref: Some(parent_group_ref.to_string()),
        root_score: Some(score(0.18)),
        independence_score: Some(score(0.12)),
        suppress_likelihood: Some(score(0.89)),
        summary_likelihood: Some(score(0.76)),
        visibility_floor: VisibilityFloor::HiddenAllowed,
        evidence_tags: vec!["cascade".to_string()],
    }
}

fn document_analysis(
    episodes: Vec<DiagnosticEpisode>,
    group_analysis: Vec<GroupCascadeAnalysis>,
) -> DocumentAnalysis {
    DocumentAnalysis {
        policy_profile: Some("default-aggressive".to_string()),
        producer_version: Some("test".to_string()),
        episode_graph: EpisodeGraph {
            episodes,
            relations: Vec::new(),
        },
        group_analysis,
        stats: Default::default(),
    }
}

fn write_source_file(root: &tempfile::TempDir, relative: &str, contents: &str) {
    let path = root.path().join(relative);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(path, contents).unwrap();
}

fn multi_root_compile_failure_case(profile: RenderProfile) -> (tempfile::TempDir, RenderRequest) {
    let tempdir = tempfile::tempdir().unwrap();
    write_source_file(
        &tempdir,
        "src/main.c",
        "int main(void) {\n    return }\n}\n",
    );
    write_source_file(
        &tempdir,
        "src/other.c",
        "int use_widget(void) {\n    return Widget();\n}\n",
    );
    write_source_file(
        &tempdir,
        "src/third.c",
        "int third(void) {\n    return missing_call(\n}\n",
    );
    write_source_file(
        &tempdir,
        "src/warn.c",
        "int warn(void) {\n    int tmp = 0;\n    return 1;\n}\n",
    );

    let mut request = base_request(tempdir.path(), profile);

    let mut root_a = diagnostic_node(
        "root-a",
        "src/main.c",
        2,
        12,
        "expected ';' before '}' token",
        Severity::Error,
        SemanticRole::Root,
        Phase::Parse,
    );
    root_a.analysis = Some(sample_analysis(
        "syntax",
        "syntax error",
        Some("fix the first parser error at the user-owned location"),
        Confidence::High,
        "rule.syntax.expected_or_before",
    ));

    let root_b = diagnostic_node(
        "root-b",
        "src/other.c",
        2,
        12,
        "secondary failure",
        Severity::Error,
        SemanticRole::Root,
        Phase::Semantic,
    );

    let root_c = diagnostic_node(
        "root-c",
        "src/third.c",
        2,
        12,
        "tertiary failure",
        Severity::Error,
        SemanticRole::Root,
        Phase::Semantic,
    );

    let tail_c = diagnostic_node(
        "tail-c",
        "src/third.c",
        3,
        1,
        "tertiary follow-on detail",
        Severity::Error,
        SemanticRole::Supporting,
        Phase::Semantic,
    );

    let warning = diagnostic_node(
        "warning",
        "src/warn.c",
        2,
        9,
        "unused variable 'tmp'",
        Severity::Warning,
        SemanticRole::Supporting,
        Phase::Semantic,
    );

    request.document.diagnostics = vec![root_a, root_b, root_c, tail_c, warning];
    request.document.document_analysis = Some(document_analysis(
        vec![
            episode("episode-a", "root-a", vec!["root-a"], 0.96),
            episode("episode-b", "root-b", vec!["root-b"], 0.93),
            episode("episode-c", "root-c", vec!["root-c", "tail-c"], 0.88),
        ],
        vec![
            lead_root_group("root-a", "episode-a", 0.96, 0.91),
            lead_root_group("root-b", "episode-b", 0.93, 0.88),
            lead_root_group("root-c", "episode-c", 0.88, 0.83),
            dependent_group("tail-c", "episode-c", "root-c", GroupCascadeRole::FollowOn),
        ],
    ));

    (tempdir, request)
}

fn custom_subject_first_case() -> (tempfile::TempDir, RenderRequest) {
    let tempdir = tempfile::tempdir().unwrap();
    write_source_file(
        &tempdir,
        "src/custom.c",
        "int custom(void) {\n    return }\n}\n",
    );

    let mut request = base_request(tempdir.path(), RenderProfile::Default);
    request.document.diagnostics = vec![{
        let mut node = diagnostic_node(
            "custom-root",
            "src/custom.c",
            2,
            12,
            "expected ';' before '}' token",
            Severity::Error,
            SemanticRole::Root,
            Phase::Parse,
        );
        node.analysis = Some(sample_analysis(
            "syntax",
            "syntax error",
            Some("fix the first parser error at the user-owned location"),
            Confidence::High,
            "rule.syntax.expected_or_before",
        ));
        node
    }];
    (tempdir, request)
}

fn load_custom_policy() -> diag_render::ResolvedPresentationPolicy {
    toml::from_str(&load_fixture("custom_subject_first_policy.toml")).unwrap()
}

#[test]
fn multi_root_compile_failure_contract_tracks_counts_and_profile_diffs() {
    let (_default_dir, default_request) = multi_root_compile_failure_case(RenderProfile::Default);
    let default_view = build_view_model(&default_request).unwrap();
    assert_eq!(default_view.summary.failure_kind, "compile_failure");
    assert_eq!(default_view.cards.len(), 3);
    assert!(default_view.summary_only_groups.is_empty());
    assert_eq!(
        default_view
            .cards
            .iter()
            .map(|card| card.excerpts.len())
            .sum::<usize>(),
        3
    );
    assert_eq!(
        default_view
            .cards
            .iter()
            .map(|card| card.collapsed_notices.len())
            .sum::<usize>(),
        1
    );
    assert_eq!(
        default_view.cards[2].collapsed_notices,
        vec!["omitted 1 follow-on diagnostic(s)".to_string()]
    );
    assert!(
        default_view
            .cards
            .iter()
            .all(|card| !card.raw_message.is_empty())
    );

    let default_output = render(default_request).unwrap();
    assert_eq!(
        default_output.displayed_group_refs,
        vec![
            "root-a".to_string(),
            "root-b".to_string(),
            "root-c".to_string(),
        ]
    );
    assert_eq!(default_output.suppressed_group_count, 0);
    assert_eq!(default_output.suppressed_warning_count, 1);
    assert!(
        default_output
            .text
            .starts_with("error: [syntax] syntax error")
    );
    assert!(
        default_output
            .text
            .contains("note: omitted 1 follow-on diagnostic(s)")
    );
    assert!(
        default_output
            .text
            .contains("note: suppressed 1 warning(s) while focusing on the failing group")
    );
    assert!(!default_output.text.contains("unused variable 'tmp'"));
    assert!(
        default_output
            .text
            .contains("raw: rerun with --formed-profile=raw_fallback")
    );

    let (_ci_dir, ci_request) = multi_root_compile_failure_case(RenderProfile::Ci);
    let ci_output = render(ci_request).unwrap();
    assert_eq!(ci_output.suppressed_warning_count, 1);
    assert!(
        ci_output
            .text
            .starts_with("src/main.c:2:12: error: [syntax] syntax error")
    );
    assert!(
        ci_output
            .text
            .contains("\n\nsrc/other.c:2:12: error: [generic] secondary failure")
    );
    assert!(!ci_output.text.contains("unused variable 'tmp'"));

    let (_verbose_dir, verbose_request) = multi_root_compile_failure_case(RenderProfile::Verbose);
    let verbose_view = build_view_model(&verbose_request).unwrap();
    assert_eq!(verbose_view.cards.len(), 5);
    assert!(verbose_view.summary_only_groups.is_empty());
    assert_eq!(
        verbose_view
            .cards
            .iter()
            .map(|card| card.excerpts.len())
            .sum::<usize>(),
        5
    );
    assert_eq!(
        verbose_view
            .cards
            .iter()
            .map(|card| card.collapsed_notices.len())
            .sum::<usize>(),
        0
    );
    assert!(
        verbose_view
            .cards
            .iter()
            .all(|card| !card.raw_message.is_empty())
    );

    let verbose_output = render(verbose_request).unwrap();
    assert_eq!(verbose_output.suppressed_warning_count, 0);
    assert!(
        verbose_output
            .text
            .starts_with("error: [syntax] syntax error")
    );
    assert!(
        verbose_output
            .text
            .contains("error: [generic] tertiary follow-on detail")
    );
    assert!(
        verbose_output
            .text
            .contains("warning: [generic] unused variable 'tmp'")
    );
    assert!(
        !verbose_output
            .text
            .contains("note: omitted 1 follow-on diagnostic(s)")
    );
    assert!(
        verbose_output
            .text
            .contains("raw: rerun with --formed-profile=raw_fallback")
    );
}

#[test]
fn custom_presentation_fixture_activates_subject_first_header_without_builtin_preset_name() {
    let policy = load_custom_policy();
    assert_ne!(policy.preset_id, "subject_blocks_v1");

    let (_tempdir, request) = custom_subject_first_case();
    let snapshot = build_presentation_snapshot_with_presentation_policy(&request, &policy).unwrap();

    assert_eq!(snapshot.preset_id, "custom_subject_first_contract");
    assert_eq!(snapshot.cards.len(), 1);
    assert!(snapshot.cards[0].presentation.subject_first_header);
    assert_eq!(snapshot.cards[0].presentation.template_id, "parser_block");
    assert_eq!(
        snapshot.cards[0]
            .slots
            .iter()
            .find(|slot| slot.slot == diag_render::SemanticSlotId::Want)
            .map(|slot| slot.value.as_str()),
        Some(";")
    );
    assert_eq!(
        snapshot.cards[0]
            .slots
            .iter()
            .find(|slot| slot.slot == diag_render::SemanticSlotId::Near)
            .map(|slot| slot.value.as_str()),
        Some("} token")
    );

    let output = render_with_presentation_policy(request, &policy).unwrap();
    assert!(output.text.starts_with("error: [syntax] syntax error"));
    assert!(
        output
            .text
            .contains("help: fix the first parser error at the user-owned location")
    );
    assert!(!output.text.contains("subject_blocks_v1"));
}

#[test]
#[ignore = "checked-in render snapshots for issue #181"]
fn multi_root_compile_failure_snapshots_match_checked_in_contract() {
    let (_default_dir, default_request) = multi_root_compile_failure_case(RenderProfile::Default);
    let default_view = build_view_model(&default_request).unwrap();
    assert_fixture_eq(
        "multi_root_compile_failure.view.default.json",
        &diag_core::canonical_json(&default_view).unwrap(),
    );
    assert_fixture_eq(
        "multi_root_compile_failure.render.default.txt",
        &render(default_request).unwrap().text,
    );

    let (_ci_dir, ci_request) = multi_root_compile_failure_case(RenderProfile::Ci);
    assert_fixture_eq(
        "multi_root_compile_failure.render.ci.txt",
        &render(ci_request).unwrap().text,
    );

    let (_verbose_dir, verbose_request) = multi_root_compile_failure_case(RenderProfile::Verbose);
    assert_fixture_eq(
        "multi_root_compile_failure.render.verbose.txt",
        &render(verbose_request).unwrap().text,
    );
}

#[test]
#[ignore = "checked-in custom policy snapshots for issue #181"]
fn custom_subject_first_policy_snapshots_match_checked_in_contract() {
    let policy = load_custom_policy();
    let (_tempdir, request) = custom_subject_first_case();
    let snapshot = build_presentation_snapshot_with_presentation_policy(&request, &policy).unwrap();
    assert_fixture_eq(
        "custom_subject_first.presentation.json",
        &diag_core::canonical_json(&snapshot).unwrap(),
    );
    assert_fixture_eq(
        "custom_subject_first.render.default.txt",
        &render_with_presentation_policy(request, &policy)
            .unwrap()
            .text,
    );
}
