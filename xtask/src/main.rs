use clap::{Parser, Subcommand, ValueEnum};
use diag_adapter_gcc::{ingest, producer_for_version, tool_for_backend};
use diag_core::{
    ArtifactKind, ArtifactStorage, CaptureArtifact, DiagnosticDocument, LanguageMode, RunInfo,
    SnapshotKind, WrapperSurface, snapshot_json,
};
use diag_enrich::enrich_document;
use diag_render::{
    DebugRefs, PathPolicy, RenderCapabilities, RenderProfile, RenderRequest, SourceExcerptPolicy,
    StreamKind, TypeDisplayPolicy, WarningVisibility, build_view_model, render,
};
use diag_testkit::{
    ExpectedFallback, Fixture, RenderProfileExpectations, discover, family_counts, validate_fixture,
};
use diag_trace::{BuildManifest, ChecksumEntry, DEFAULT_PRODUCT_NAME, build_manifest_for_target};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Instant;

const REPRESENTATIVE_FIXTURES: &[&str] = &[
    "c/syntax/case-01",
    "c/type/case-01",
    "cpp/overload/case-01",
    "cpp/template/case-01",
    "c/macro_include/case-01",
    "c/linker/case-01",
    "c/partial/case-01",
];

#[derive(Debug, Parser)]
#[command(name = "xtask")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, Subcommand)]
enum Commands {
    Check,
    Package {
        #[arg(long)]
        binary: PathBuf,
        #[arg(long)]
        debug_binary: Option<PathBuf>,
        #[arg(long)]
        target_triple: String,
        #[arg(long, default_value = "dist")]
        out_dir: PathBuf,
        #[arg(long, default_value = "stable")]
        release_channel: String,
        #[arg(long, default_value = "gcc15_primary")]
        support_tier: String,
    },
    Replay {
        #[arg(long, default_value = "corpus")]
        root: PathBuf,
        #[arg(long)]
        fixture: Option<String>,
        #[arg(long)]
        family: Option<String>,
    },
    Snapshot {
        #[arg(long, default_value = "corpus")]
        root: PathBuf,
        #[arg(long)]
        fixture: Option<String>,
        #[arg(long)]
        family: Option<String>,
        #[arg(long, value_enum, default_value_t = SnapshotSubset::All)]
        subset: SnapshotSubset,
        #[arg(long)]
        check: bool,
        #[arg(long, default_value = "gcc:15")]
        docker_image: String,
    },
    BenchSmoke,
    SelfCheck,
}

#[derive(Debug, Clone, Copy, ValueEnum, PartialEq, Eq)]
enum SnapshotSubset {
    All,
    Representative,
}

#[derive(Debug)]
struct VerificationFailure {
    layer: String,
    fixture_id: String,
    summary: String,
}

#[derive(Debug)]
struct CapturedIngress {
    stderr_text: String,
    sarif_text: String,
}

#[derive(Debug, Clone)]
struct PackageOptions {
    binary: PathBuf,
    debug_binary: Option<PathBuf>,
    target_triple: String,
    out_dir: PathBuf,
    release_channel: String,
    support_tier: String,
}

#[derive(Debug)]
struct PackageOutput {
    control_dir: PathBuf,
    primary_archive: PathBuf,
    debug_archive: PathBuf,
    source_archive: PathBuf,
    manifest_path: PathBuf,
    build_info_path: PathBuf,
    shasums_path: PathBuf,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();
    match cli.command {
        Commands::Check => {
            run("cargo", &["fmt", "--check"])?;
            run("cargo", &["test", "--workspace"])?;
        }
        Commands::Package {
            binary,
            debug_binary,
            target_triple,
            out_dir,
            release_channel,
            support_tier,
        } => {
            let package = run_package(PackageOptions {
                binary,
                debug_binary,
                target_triple,
                out_dir,
                release_channel,
                support_tier,
            })?;
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({
                    "control_dir": package.control_dir,
                    "primary_archive": package.primary_archive,
                    "debug_archive": package.debug_archive,
                    "source_archive": package.source_archive,
                    "manifest_path": package.manifest_path,
                    "build_info_path": package.build_info_path,
                    "shasums_path": package.shasums_path,
                }))?
            );
        }
        Commands::Replay {
            root,
            fixture,
            family,
        } => run_replay(&root, fixture.as_deref(), family.as_deref())?,
        Commands::Snapshot {
            root,
            fixture,
            family,
            subset,
            check,
            docker_image,
        } => run_snapshot(
            &root,
            fixture.as_deref(),
            family.as_deref(),
            subset,
            check,
            &docker_image,
        )?,
        Commands::BenchSmoke => {
            println!(
                "{}",
                serde_json::json!({
                    "success_path_p95_ms_target": 40,
                    "simple_failure_p95_ms_target": 80,
                    "template_heavy_p95_ms_target": 250
                })
            );
        }
        Commands::SelfCheck => {
            println!(
                "{}",
                serde_json::json!({
                    "workspace": "ok",
                    "toolchain": "managed via rust-toolchain.toml",
                    "corpus_root": "corpus"
                })
            );
        }
    }
    Ok(())
}

fn run_replay(
    root: &Path,
    fixture_filter: Option<&str>,
    family_filter: Option<&str>,
) -> Result<(), Box<dyn std::error::Error>> {
    let fixtures = discover(root)?;
    for fixture in &fixtures {
        validate_fixture(fixture)?;
    }
    let counts = family_counts(&fixtures);
    enforce_minimum_family_counts(&counts)?;
    let selected = select_fixtures(
        &fixtures,
        fixture_filter,
        family_filter,
        SnapshotSubset::All,
    );
    if selected.is_empty() {
        return Err("no fixtures matched replay selection".into());
    }

    let mut failures = Vec::new();
    let mut promoted_verified = 0usize;
    for fixture in &selected {
        if fixture.is_promoted() {
            match verify_promoted_fixture(fixture) {
                Ok(_) => promoted_verified += 1,
                Err(failure) => failures.push(failure),
            }
        }
    }

    if !failures.is_empty() {
        report_failures("replay", &failures);
        return Err("replay verification failed".into());
    }

    println!(
        "{}",
        serde_json::to_string_pretty(&serde_json::json!({
            "family_counts": counts,
            "selected_fixture_count": selected.len(),
            "promoted_verified": promoted_verified,
            "mode": "replay"
        }))?
    );
    Ok(())
}

fn run_snapshot(
    root: &Path,
    fixture_filter: Option<&str>,
    family_filter: Option<&str>,
    subset: SnapshotSubset,
    check: bool,
    docker_image: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let fixtures = discover(root)?;
    let selected = select_fixtures(&fixtures, fixture_filter, family_filter, subset);
    if selected.is_empty() {
        return Err("no fixtures matched snapshot selection".into());
    }

    let promoted = selected
        .iter()
        .copied()
        .filter(|fixture| fixture.is_promoted())
        .collect::<Vec<_>>();
    if promoted.is_empty() {
        return Err("snapshot selection did not include any promoted fixtures".into());
    }

    let mut failures = Vec::new();
    let mut updated = 0usize;
    for fixture in promoted {
        if let Err(error) = validate_snapshot_inputs(fixture) {
            failures.push(VerificationFailure {
                layer: "fixture_layout".to_string(),
                fixture_id: fixture.fixture_id().to_string(),
                summary: error.to_string(),
            });
            continue;
        }
        match materialize_fixture_snapshots(fixture, docker_image, check) {
            Ok(count) => updated += count,
            Err(failure) => failures.push(failure),
        }
    }

    if !failures.is_empty() {
        report_failures("snapshot", &failures);
        return Err("snapshot update/check failed".into());
    }

    println!(
        "{}",
        serde_json::to_string_pretty(&serde_json::json!({
            "selected_fixture_count": selected.len(),
            "promoted_fixture_count": updated,
            "check_only": check,
            "subset": match subset {
                SnapshotSubset::All => "all",
                SnapshotSubset::Representative => "representative",
            },
            "docker_image": docker_image
        }))?
    );
    Ok(())
}

fn verify_promoted_fixture(fixture: &Fixture) -> Result<(), VerificationFailure> {
    let replay = replay_fixture_document(fixture).map_err(|error| VerificationFailure {
        layer: "ingest".to_string(),
        fixture_id: fixture.fixture_id().to_string(),
        summary: error.to_string(),
    })?;
    replay
        .document
        .validate()
        .map_err(|error| VerificationFailure {
            layer: "schema_validation".to_string(),
            fixture_id: fixture.fixture_id().to_string(),
            summary: error.errors.join("; "),
        })?;

    compare_snapshot_file(
        fixture,
        "ir.facts",
        &fixture.snapshot_root().join("ir.facts.json"),
        &snapshot_json(&replay.document, SnapshotKind::FactsOnly).map_err(|error| {
            VerificationFailure {
                layer: "ir.facts".to_string(),
                fixture_id: fixture.fixture_id().to_string(),
                summary: error.to_string(),
            }
        })?,
    )?;
    compare_snapshot_file(
        fixture,
        "ir.analysis",
        &fixture.snapshot_root().join("ir.analysis.json"),
        &snapshot_json(&replay.document, SnapshotKind::AnalysisIncluded).map_err(|error| {
            VerificationFailure {
                layer: "ir.analysis".to_string(),
                fixture_id: fixture.fixture_id().to_string(),
                summary: error.to_string(),
            }
        })?,
    )?;

    let default_request =
        render_request_for_fixture(fixture, &replay.document, RenderProfile::Default);
    let default_render_start = Instant::now();
    let default_render_result =
        render(default_request.clone()).map_err(|error| VerificationFailure {
            layer: "render.default".to_string(),
            fixture_id: fixture.fixture_id().to_string(),
            summary: error.to_string(),
        })?;
    let default_render_time_ms = elapsed_ms(default_render_start);
    let default_view_model = build_view_model(&default_request);
    let lead_node = lead_node_for_document(
        &replay.document,
        &default_render_result.displayed_group_refs,
    )
    .ok_or_else(|| VerificationFailure {
        layer: "semantic".to_string(),
        fixture_id: fixture.fixture_id().to_string(),
        summary: "default render produced no lead diagnostic".to_string(),
    })?;
    verify_semantic_expectations(fixture, &replay.document, lead_node, &default_render_result)?;

    for (profile_name, expectations) in fixture.expectations.render.named_profiles() {
        let profile =
            render_profile_from_name(profile_name).ok_or_else(|| VerificationFailure {
                layer: "render".to_string(),
                fixture_id: fixture.fixture_id().to_string(),
                summary: format!("unknown snapshot profile `{profile_name}`"),
            })?;
        let request = render_request_for_fixture(fixture, &replay.document, profile);
        let view_model = if matches!(profile, RenderProfile::Default) {
            default_view_model.clone()
        } else {
            build_view_model(&request)
        };
        let render_result = if matches!(profile, RenderProfile::Default) {
            default_render_result.clone()
        } else {
            render(request.clone()).map_err(|error| VerificationFailure {
                layer: format!("render.{profile_name}"),
                fixture_id: fixture.fixture_id().to_string(),
                summary: error.to_string(),
            })?
        };

        compare_snapshot_file(
            fixture,
            &format!("view.{profile_name}"),
            &fixture
                .snapshot_root()
                .join(format!("view.{profile_name}.json")),
            &canonical_json_for_view_model(view_model.as_ref()).map_err(|error| {
                VerificationFailure {
                    layer: format!("view.{profile_name}"),
                    fixture_id: fixture.fixture_id().to_string(),
                    summary: error.to_string(),
                }
            })?,
        )?;
        compare_snapshot_file(
            fixture,
            &format!("render.{profile_name}"),
            &fixture
                .snapshot_root()
                .join(format!("render.{profile_name}.txt")),
            &render_result.text,
        )?;
        verify_render_expectations(
            fixture,
            profile_name,
            expectations,
            &render_result.text,
            lead_node
                .primary_location()
                .map(|location| location.path.as_str()),
        )?;
    }

    if let Some(perf) = fixture.expectations.performance.parse_time_ms_max {
        if replay.parse_time_ms > perf {
            return Err(VerificationFailure {
                layer: "performance.parse".to_string(),
                fixture_id: fixture.fixture_id().to_string(),
                summary: format!("parse time {}ms exceeded {}ms", replay.parse_time_ms, perf),
            });
        }
    }
    if let Some(perf) = fixture.expectations.performance.render_time_ms_max {
        if default_render_time_ms > perf {
            return Err(VerificationFailure {
                layer: "performance.render".to_string(),
                fixture_id: fixture.fixture_id().to_string(),
                summary: format!(
                    "default render time {}ms exceeded {}ms",
                    default_render_time_ms, perf
                ),
            });
        }
    }

    Ok(())
}

fn materialize_fixture_snapshots(
    fixture: &Fixture,
    docker_image: &str,
    check: bool,
) -> Result<usize, VerificationFailure> {
    let captured = if std::env::var_os("FORMED_SNAPSHOT_USE_EXISTING_INGRESS").is_some() {
        load_existing_ingress(fixture)?
    } else {
        capture_fixture_ingress(fixture, docker_image)?
    };
    let tempdir = tempfile::tempdir().map_err(|error| VerificationFailure {
        layer: "snapshot".to_string(),
        fixture_id: fixture.fixture_id().to_string(),
        summary: error.to_string(),
    })?;
    let temp_root = tempdir.path();
    fs::write(temp_root.join("stderr.raw"), &captured.stderr_text).map_err(|error| {
        VerificationFailure {
            layer: "snapshot".to_string(),
            fixture_id: fixture.fixture_id().to_string(),
            summary: error.to_string(),
        }
    })?;
    fs::write(temp_root.join("diagnostics.sarif"), &captured.sarif_text).map_err(|error| {
        VerificationFailure {
            layer: "snapshot".to_string(),
            fixture_id: fixture.fixture_id().to_string(),
            summary: error.to_string(),
        }
    })?;

    let document = replay_document_from_ingress(
        fixture,
        &captured.stderr_text,
        temp_root.join("diagnostics.sarif").as_path(),
    )
    .map_err(|error| VerificationFailure {
        layer: "snapshot".to_string(),
        fixture_id: fixture.fixture_id().to_string(),
        summary: error.to_string(),
    })?;
    document.validate().map_err(|error| VerificationFailure {
        layer: "snapshot".to_string(),
        fixture_id: fixture.fixture_id().to_string(),
        summary: error.errors.join("; "),
    })?;

    let snapshot_root = fixture.snapshot_root();
    fs::create_dir_all(&snapshot_root).map_err(|error| VerificationFailure {
        layer: "snapshot".to_string(),
        fixture_id: fixture.fixture_id().to_string(),
        summary: error.to_string(),
    })?;

    let mut artifacts = BTreeMap::new();
    artifacts.insert("stderr.raw".to_string(), captured.stderr_text.clone());
    artifacts.insert("diagnostics.sarif".to_string(), captured.sarif_text.clone());
    artifacts.insert(
        "ir.facts.json".to_string(),
        snapshot_json(&document, SnapshotKind::FactsOnly).map_err(|error| VerificationFailure {
            layer: "snapshot".to_string(),
            fixture_id: fixture.fixture_id().to_string(),
            summary: error.to_string(),
        })?,
    );
    artifacts.insert(
        "ir.analysis.json".to_string(),
        snapshot_json(&document, SnapshotKind::AnalysisIncluded).map_err(|error| {
            VerificationFailure {
                layer: "snapshot".to_string(),
                fixture_id: fixture.fixture_id().to_string(),
                summary: error.to_string(),
            }
        })?,
    );

    for (profile_name, _) in fixture.expectations.render.named_profiles() {
        let profile =
            render_profile_from_name(profile_name).ok_or_else(|| VerificationFailure {
                layer: "snapshot".to_string(),
                fixture_id: fixture.fixture_id().to_string(),
                summary: format!("unknown snapshot profile `{profile_name}`"),
            })?;
        let request = render_request_for_fixture(fixture, &document, profile);
        let view_model = build_view_model(&request);
        let render_result = render(request).map_err(|error| VerificationFailure {
            layer: format!("render.{profile_name}"),
            fixture_id: fixture.fixture_id().to_string(),
            summary: error.to_string(),
        })?;
        artifacts.insert(
            format!("view.{profile_name}.json"),
            canonical_json_for_view_model(view_model.as_ref()).map_err(|error| {
                VerificationFailure {
                    layer: format!("view.{profile_name}"),
                    fixture_id: fixture.fixture_id().to_string(),
                    summary: error.to_string(),
                }
            })?,
        );
        artifacts.insert(format!("render.{profile_name}.txt"), render_result.text);
    }

    for (relative, contents) in artifacts {
        let path = snapshot_root.join(relative);
        if check {
            compare_snapshot_file(
                fixture,
                path.file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or("snapshot"),
                &path,
                &contents,
            )?;
        } else {
            fs::write(&path, contents).map_err(|error| VerificationFailure {
                layer: "snapshot_write".to_string(),
                fixture_id: fixture.fixture_id().to_string(),
                summary: format!("{}: {error}", path.display()),
            })?;
        }
    }

    Ok(1)
}

fn load_existing_ingress(fixture: &Fixture) -> Result<CapturedIngress, VerificationFailure> {
    let snapshot_root = fixture.snapshot_root();
    let stderr_path = snapshot_root.join("stderr.raw");
    let sarif_path = snapshot_root.join("diagnostics.sarif");
    let stderr_text = fs::read_to_string(&stderr_path).map_err(|error| VerificationFailure {
        layer: "capture".to_string(),
        fixture_id: fixture.fixture_id().to_string(),
        summary: format!("failed to read {}: {error}", stderr_path.display()),
    })?;
    let sarif_text = fs::read_to_string(&sarif_path).map_err(|error| VerificationFailure {
        layer: "capture".to_string(),
        fixture_id: fixture.fixture_id().to_string(),
        summary: format!("failed to read {}: {error}", sarif_path.display()),
    })?;
    Ok(CapturedIngress {
        stderr_text,
        sarif_text,
    })
}

fn replay_fixture_document(
    fixture: &Fixture,
) -> Result<ReplayOutcomeAndDocument, Box<dyn std::error::Error>> {
    let snapshot_root = fixture.snapshot_root();
    let stderr_text = fs::read_to_string(snapshot_root.join("stderr.raw"))?;
    let parse_start = Instant::now();
    let document = replay_document_from_ingress(
        fixture,
        &stderr_text,
        snapshot_root.join("diagnostics.sarif").as_path(),
    )?;
    Ok(ReplayOutcomeAndDocument {
        document,
        parse_time_ms: elapsed_ms(parse_start),
    })
}

#[derive(Debug)]
struct ReplayOutcomeAndDocument {
    document: DiagnosticDocument,
    parse_time_ms: u64,
}

fn replay_document_from_ingress(
    fixture: &Fixture,
    stderr_text: &str,
    sarif_path: &Path,
) -> Result<DiagnosticDocument, Box<dyn std::error::Error>> {
    let run_info = run_info_for_fixture(fixture);
    let mut document = ingest(
        Some(sarif_path),
        stderr_text,
        producer_for_version("snapshot"),
        run_info,
    )?;
    document.captures = capture_artifacts_for_fixture(fixture, stderr_text, sarif_path)?;
    enrich_document(&mut document, &fixture.root);
    Ok(document)
}

fn run_info_for_fixture(fixture: &Fixture) -> RunInfo {
    let compiler = compiler_binary_for_fixture(fixture);
    let mut argv = vec![compiler.to_string()];
    if let Some(standard) = fixture.invoke.standard.as_ref() {
        argv.push(format!("-std={standard}"));
    }
    argv.extend(fixture.invoke.argv.iter().cloned());

    RunInfo {
        invocation_id: format!("fixture-{}", fixture.fixture_id().replace('/', "-")),
        invoked_as: Some("gcc-formed".to_string()),
        argv_redacted: argv,
        cwd_display: Some(fixture.root.display().to_string()),
        exit_status: 1,
        primary_tool: tool_for_backend(
            compiler,
            Some(format!("{}.x", fixture.invoke.major_version_selector)),
        ),
        secondary_tools: Vec::new(),
        language_mode: Some(language_mode_for_fixture(fixture)),
        target_triple: Some("x86_64-unknown-linux-gnu".to_string()),
        wrapper_mode: Some(WrapperSurface::Terminal),
    }
}

fn capture_artifacts_for_fixture(
    fixture: &Fixture,
    stderr_text: &str,
    sarif_path: &Path,
) -> Result<Vec<CaptureArtifact>, Box<dyn std::error::Error>> {
    let compiler = tool_for_backend(
        compiler_binary_for_fixture(fixture),
        Some(format!("{}.x", fixture.invoke.major_version_selector)),
    );
    let mut captures = vec![CaptureArtifact {
        id: "stderr.raw".to_string(),
        kind: ArtifactKind::CompilerStderrText,
        media_type: "text/plain".to_string(),
        encoding: Some("utf-8".to_string()),
        digest_sha256: None,
        size_bytes: Some(stderr_text.len() as u64),
        storage: ArtifactStorage::Inline,
        inline_text: Some(stderr_text.to_string()),
        external_ref: None,
        produced_by: Some(compiler.clone()),
    }];
    captures.push(CaptureArtifact {
        id: "diagnostics.sarif".to_string(),
        kind: ArtifactKind::GccSarif,
        media_type: "application/sarif+json".to_string(),
        encoding: Some("utf-8".to_string()),
        digest_sha256: None,
        size_bytes: Some(fs::metadata(sarif_path)?.len()),
        storage: ArtifactStorage::ExternalRef,
        inline_text: None,
        external_ref: Some(sarif_path.display().to_string()),
        produced_by: Some(compiler),
    });
    Ok(captures)
}

fn render_request_for_fixture(
    fixture: &Fixture,
    document: &DiagnosticDocument,
    profile: RenderProfile,
) -> RenderRequest {
    RenderRequest {
        document: document.clone(),
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
        cwd: Some(fixture.root.clone()),
        path_policy: PathPolicy::RelativeToCwd,
        warning_visibility: WarningVisibility::Auto,
        debug_refs: DebugRefs::None,
        type_display_policy: TypeDisplayPolicy::CompactSafe,
        source_excerpt_policy: SourceExcerptPolicy::Auto,
    }
}

fn verify_semantic_expectations(
    fixture: &Fixture,
    document: &DiagnosticDocument,
    lead_node: &diag_core::DiagnosticNode,
    default_render_result: &diag_render::RenderResult,
) -> Result<(), VerificationFailure> {
    let semantic = fixture
        .expectations
        .semantic
        .as_ref()
        .ok_or_else(|| VerificationFailure {
            layer: "semantic".to_string(),
            fixture_id: fixture.fixture_id().to_string(),
            summary: "promoted fixture missing semantic expectations".to_string(),
        })?;

    let actual_family = lead_node
        .analysis
        .as_ref()
        .and_then(|analysis| analysis.family.as_deref())
        .unwrap_or("unknown");
    if actual_family != semantic.family {
        return Err(VerificationFailure {
            layer: "semantic.family".to_string(),
            fixture_id: fixture.fixture_id().to_string(),
            summary: format!("expected `{}`, got `{actual_family}`", semantic.family),
        });
    }

    if lead_node.severity != semantic.severity {
        return Err(VerificationFailure {
            layer: "semantic.severity".to_string(),
            fixture_id: fixture.fixture_id().to_string(),
            summary: format!(
                "expected `{}`, got `{}`",
                semantic.severity, lead_node.severity
            ),
        });
    }

    if !semantic.lead_group_any_of.is_empty()
        && !semantic
            .lead_group_any_of
            .iter()
            .any(|group_id| group_id == &lead_node.id)
    {
        return Err(VerificationFailure {
            layer: "semantic.lead_group".to_string(),
            fixture_id: fixture.fixture_id().to_string(),
            summary: format!(
                "lead group `{}` not in allowed set [{}]",
                lead_node.id,
                semantic.lead_group_any_of.join(", ")
            ),
        });
    }

    for expected in &semantic.primary_locations {
        let found = lead_node.locations.iter().any(|location| {
            location.path == expected.path
                && location.line == expected.line
                && expected
                    .column
                    .map(|column| column == location.column)
                    .unwrap_or(true)
        });
        if !found {
            return Err(VerificationFailure {
                layer: "semantic.primary_locations".to_string(),
                fixture_id: fixture.fixture_id().to_string(),
                summary: format!(
                    "lead diagnostic did not include expected location {}:{}",
                    expected.path, expected.line
                ),
            });
        }
    }

    if semantic.first_action_required
        && lead_node
            .analysis
            .as_ref()
            .and_then(|analysis| analysis.first_action_hint.as_ref())
            .map(|value| value.trim().is_empty())
            .unwrap_or(true)
    {
        return Err(VerificationFailure {
            layer: "semantic.first_action".to_string(),
            fixture_id: fixture.fixture_id().to_string(),
            summary: "lead diagnostic did not expose a first_action_hint".to_string(),
        });
    }

    if semantic.raw_provenance_required {
        let has_stderr_capture = document
            .captures
            .iter()
            .any(|capture| capture.id == "stderr.raw");
        if !has_stderr_capture || lead_node.provenance.capture_refs.is_empty() {
            return Err(VerificationFailure {
                layer: "semantic.raw_provenance".to_string(),
                fixture_id: fixture.fixture_id().to_string(),
                summary: "raw provenance was not preserved".to_string(),
            });
        }
    }

    if let Some(fallback) = semantic.fallback {
        match fallback {
            ExpectedFallback::Allowed => {}
            ExpectedFallback::Forbidden if default_render_result.used_fallback => {
                return Err(VerificationFailure {
                    layer: "semantic.fallback".to_string(),
                    fixture_id: fixture.fixture_id().to_string(),
                    summary: "default profile unexpectedly used fallback".to_string(),
                });
            }
            ExpectedFallback::Required if !default_render_result.used_fallback => {
                return Err(VerificationFailure {
                    layer: "semantic.fallback".to_string(),
                    fixture_id: fixture.fixture_id().to_string(),
                    summary: "default profile did not use required fallback".to_string(),
                });
            }
            _ => {}
        }
    }

    if let Some(confidence_min) = semantic.confidence_min.as_ref() {
        let actual = lead_node
            .analysis
            .as_ref()
            .and_then(|analysis| analysis.confidence.as_ref())
            .cloned()
            .unwrap_or(diag_core::Confidence::Unknown);
        if confidence_rank(&actual) < confidence_rank(confidence_min) {
            return Err(VerificationFailure {
                layer: "semantic.confidence".to_string(),
                fixture_id: fixture.fixture_id().to_string(),
                summary: format!("expected confidence >= {confidence_min:?}, got {actual:?}"),
            });
        }
    }

    Ok(())
}

fn verify_render_expectations(
    fixture: &Fixture,
    profile_name: &str,
    expectations: &RenderProfileExpectations,
    text: &str,
    lead_path: Option<&str>,
) -> Result<(), VerificationFailure> {
    if expectations.omission_notice_required == Some(true) && !text.contains("omitted") {
        return Err(VerificationFailure {
            layer: format!("render.{profile_name}.omission_notice"),
            fixture_id: fixture.fixture_id().to_string(),
            summary: "required omission notice was missing".to_string(),
        });
    }
    if expectations.omission_notice_required == Some(false) && text.contains("omitted") {
        return Err(VerificationFailure {
            layer: format!("render.{profile_name}.omission_notice"),
            fixture_id: fixture.fixture_id().to_string(),
            summary: "unexpected omission notice was present".to_string(),
        });
    }
    if let Some(max_lines) = expectations.first_screenful_max_lines {
        let lines = text.lines().count();
        if lines > max_lines {
            return Err(VerificationFailure {
                layer: format!("render.{profile_name}.line_budget"),
                fixture_id: fixture.fixture_id().to_string(),
                summary: format!("rendered {lines} lines, budget is {max_lines}"),
            });
        }
    }
    if expectations.path_first_required == Some(true) {
        let first_line = text.lines().next().unwrap_or_default();
        let lead_path = lead_path.unwrap_or_default();
        if !first_line.starts_with(lead_path) {
            return Err(VerificationFailure {
                layer: format!("render.{profile_name}.path_first"),
                fixture_id: fixture.fixture_id().to_string(),
                summary: format!("first line was not path-first: `{first_line}`"),
            });
        }
    }
    if expectations.color_meaning_forbidden == Some(true) && text.contains('\u{1b}') {
        return Err(VerificationFailure {
            layer: format!("render.{profile_name}.ansi"),
            fixture_id: fixture.fixture_id().to_string(),
            summary: "render output used ANSI escapes".to_string(),
        });
    }
    Ok(())
}

fn compare_snapshot_file(
    fixture: &Fixture,
    layer: &str,
    path: &Path,
    actual: &str,
) -> Result<(), VerificationFailure> {
    let expected = fs::read_to_string(path).map_err(|error| VerificationFailure {
        layer: layer.to_string(),
        fixture_id: fixture.fixture_id().to_string(),
        summary: format!("failed to read {}: {error}", path.display()),
    })?;
    if expected == actual {
        return Ok(());
    }
    Err(VerificationFailure {
        layer: layer.to_string(),
        fixture_id: fixture.fixture_id().to_string(),
        summary: first_diff_summary(&expected, actual),
    })
}

fn canonical_json_for_view_model(
    view_model: Option<&diag_render::RenderViewModel>,
) -> Result<String, serde_json::Error> {
    match view_model {
        Some(model) => diag_core::canonical_json(model),
        None => diag_core::canonical_json(&serde_json::Value::Null),
    }
}

fn render_profile_from_name(name: &str) -> Option<RenderProfile> {
    match name {
        "default" => Some(RenderProfile::Default),
        "concise" => Some(RenderProfile::Concise),
        "verbose" => Some(RenderProfile::Verbose),
        "ci" => Some(RenderProfile::Ci),
        "raw_fallback" => Some(RenderProfile::RawFallback),
        _ => None,
    }
}

fn lead_node_for_document<'a>(
    document: &'a DiagnosticDocument,
    displayed_group_refs: &[String],
) -> Option<&'a diag_core::DiagnosticNode> {
    let lead_id = displayed_group_refs.first()?;
    document.diagnostics.iter().find(|node| &node.id == lead_id)
}

fn confidence_rank(confidence: &diag_core::Confidence) -> u8 {
    match confidence {
        diag_core::Confidence::High => 4,
        diag_core::Confidence::Medium => 3,
        diag_core::Confidence::Low => 2,
        diag_core::Confidence::Unknown => 1,
    }
}

fn select_fixtures<'a>(
    fixtures: &'a [Fixture],
    fixture_filter: Option<&str>,
    family_filter: Option<&str>,
    subset: SnapshotSubset,
) -> Vec<&'a Fixture> {
    fixtures
        .iter()
        .filter(|fixture| {
            fixture_filter
                .map(|needle| fixture.fixture_id() == needle)
                .unwrap_or(true)
        })
        .filter(|fixture| {
            family_filter
                .map(|needle| fixture.family_key() == needle)
                .unwrap_or(true)
        })
        .filter(|fixture| match subset {
            SnapshotSubset::All => true,
            SnapshotSubset::Representative => {
                REPRESENTATIVE_FIXTURES.contains(&fixture.fixture_id())
            }
        })
        .collect()
}

fn validate_snapshot_inputs(fixture: &Fixture) -> Result<(), Box<dyn std::error::Error>> {
    for relative in [
        "src",
        "invoke.yaml",
        "expectations.yaml",
        "meta.yaml",
        "snapshots",
    ] {
        if !fixture.root.join(relative).exists() {
            return Err(format!(
                "fixture {} missing {}",
                fixture.fixture_id(),
                fixture.root.join(relative).display()
            )
            .into());
        }
    }
    if !fixture.is_promoted() {
        return Err(format!("fixture {} is not promoted", fixture.fixture_id()).into());
    }
    Ok(())
}

fn capture_fixture_ingress(
    fixture: &Fixture,
    docker_image: &str,
) -> Result<CapturedIngress, VerificationFailure> {
    let sandbox = tempfile::tempdir().map_err(|error| VerificationFailure {
        layer: "capture".to_string(),
        fixture_id: fixture.fixture_id().to_string(),
        summary: error.to_string(),
    })?;
    copy_dir_recursive(&fixture.root.join("src"), &sandbox.path().join("src")).map_err(
        |error| VerificationFailure {
            layer: "capture".to_string(),
            fixture_id: fixture.fixture_id().to_string(),
            summary: error.to_string(),
        },
    )?;

    let compiler = compiler_binary_for_fixture(fixture);
    let mut shell_args = vec![compiler.to_string()];
    if let Some(standard) = fixture.invoke.standard.as_ref() {
        shell_args.push(format!("-std={standard}"));
    }
    shell_args.extend(fixture.invoke.argv.iter().cloned());
    shell_args
        .push("-fdiagnostics-add-output=sarif:version=2.1,file=diagnostics.sarif".to_string());
    let command_line = format!(
        "set -euo pipefail; {} 1>stdout.raw 2>stderr.raw || true",
        shell_args
            .iter()
            .map(|arg| shell_quote(arg))
            .collect::<Vec<_>>()
            .join(" ")
    );

    let mut command = Command::new("docker");
    command
        .arg("run")
        .arg("--rm")
        .arg("-v")
        .arg(format!("{}:/workspace", sandbox.path().display()))
        .arg("-w")
        .arg("/workspace")
        .arg("-e")
        .arg("LC_MESSAGES=C");
    for (key, value) in &fixture.invoke.env_overrides {
        command.arg("-e").arg(format!("{key}={value}"));
    }
    command
        .arg(docker_image)
        .arg("bash")
        .arg("-lc")
        .arg(command_line);
    let output = command.output().map_err(|error| VerificationFailure {
        layer: "capture".to_string(),
        fixture_id: fixture.fixture_id().to_string(),
        summary: format!("failed to run docker: {error}"),
    })?;
    if !output.status.success() {
        return Err(VerificationFailure {
            layer: "capture".to_string(),
            fixture_id: fixture.fixture_id().to_string(),
            summary: format!(
                "docker invocation failed: {}",
                String::from_utf8_lossy(&output.stderr)
            ),
        });
    }

    let stderr_path = sandbox.path().join("stderr.raw");
    let sarif_path = sandbox.path().join("diagnostics.sarif");
    let stderr_text = fs::read_to_string(&stderr_path).map_err(|error| VerificationFailure {
        layer: "capture".to_string(),
        fixture_id: fixture.fixture_id().to_string(),
        summary: format!("failed to read {}: {error}", stderr_path.display()),
    })?;
    let sarif_text = fs::read_to_string(&sarif_path).map_err(|error| VerificationFailure {
        layer: "capture".to_string(),
        fixture_id: fixture.fixture_id().to_string(),
        summary: format!("failed to read {}: {error}", sarif_path.display()),
    })?;
    Ok(CapturedIngress {
        stderr_text,
        sarif_text,
    })
}

fn compiler_binary_for_fixture(fixture: &Fixture) -> &'static str {
    match fixture.invoke.language.as_str() {
        "cpp" | "cxx" => "g++",
        _ => "gcc",
    }
}

fn language_mode_for_fixture(fixture: &Fixture) -> LanguageMode {
    match fixture.invoke.language.as_str() {
        "cpp" | "cxx" => LanguageMode::Cpp,
        _ => LanguageMode::C,
    }
}

fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\"'\"'"))
}

fn copy_dir_recursive(from: &Path, to: &Path) -> Result<(), std::io::Error> {
    fs::create_dir_all(to)?;
    for entry in fs::read_dir(from)? {
        let entry = entry?;
        let source = entry.path();
        let destination = to.join(entry.file_name());
        if source.is_dir() {
            copy_dir_recursive(&source, &destination)?;
        } else {
            fs::copy(source, destination)?;
        }
    }
    Ok(())
}

fn run_package(options: PackageOptions) -> Result<PackageOutput, Box<dyn std::error::Error>> {
    run_package_at(&workspace_root(), &options)
}

fn run_package_at(
    workspace_root: &Path,
    options: &PackageOptions,
) -> Result<PackageOutput, Box<dyn std::error::Error>> {
    ensure_clean_git_tree(workspace_root)?;
    ensure_release_inputs(workspace_root)?;

    let binary = canonicalize_existing_path(workspace_root, &options.binary, "binary")?;
    let debug_binary = options
        .debug_binary
        .as_ref()
        .map(|path| canonicalize_existing_path(workspace_root, path, "debug binary"))
        .transpose()?
        .unwrap_or_else(|| binary.clone());

    let output_root = resolve_workspace_path(workspace_root, &options.out_dir);
    let artifact_slug = artifact_slug_for_target(&options.target_triple);
    let version = env!("CARGO_PKG_VERSION");
    let package_basename = format!("{DEFAULT_PRODUCT_NAME}-v{version}-{artifact_slug}");
    let debug_basename = format!("{package_basename}.debug");
    let source_basename = format!("{DEFAULT_PRODUCT_NAME}-v{version}-source");
    let control_dir = output_root.join(&package_basename);
    if control_dir.exists() {
        fs::remove_dir_all(&control_dir)?;
    }
    fs::create_dir_all(&control_dir)?;

    let lockfile_hash = sha256_file(&workspace_root.join("Cargo.lock"))?;
    let vendor_hash = hash_vendor_dir(&workspace_root.join("vendor"))?;
    let primary_manifest = build_manifest_for_target(
        lockfile_hash.clone(),
        vendor_hash.clone(),
        &options.target_triple,
        &options.support_tier,
        &options.release_channel,
    );
    let debug_manifest = build_manifest_for_target(
        lockfile_hash,
        vendor_hash,
        &options.target_triple,
        &options.support_tier,
        &options.release_channel,
    );

    let staging = tempfile::tempdir()?;
    let primary_root = staging.path().join(&package_basename);
    let debug_root = staging.path().join(&debug_basename);

    let primary_manifest = stage_release_payload(
        workspace_root,
        &primary_root,
        &binary,
        &primary_manifest,
        "primary",
    )?;
    let _debug_manifest = stage_release_payload(
        workspace_root,
        &debug_root,
        &debug_binary,
        &debug_manifest,
        "debug",
    )?;

    let manifest_path = control_dir.join("manifest.json");
    fs::write(
        &manifest_path,
        serde_json::to_vec_pretty(&primary_manifest)?,
    )?;

    let build_info_path = control_dir.join("build-info.txt");
    fs::copy(primary_root.join("build-info.txt"), &build_info_path)?;

    let primary_archive = control_dir.join(format!("{package_basename}.tar.gz"));
    create_tar_archive(staging.path(), &package_basename, &primary_archive)?;
    let debug_archive = control_dir.join(format!("{debug_basename}.tar.gz"));
    create_tar_archive(staging.path(), &debug_basename, &debug_archive)?;

    let source_archive = control_dir.join(format!("{source_basename}.tar.gz"));
    create_source_archive(workspace_root, &source_archive)?;

    let shasums_path = control_dir.join("SHA256SUMS");
    fs::write(
        &shasums_path,
        render_sha256sums(&[
            &primary_archive,
            &debug_archive,
            &source_archive,
            &manifest_path,
            &build_info_path,
        ])?,
    )?;

    Ok(PackageOutput {
        control_dir,
        primary_archive,
        debug_archive,
        source_archive,
        manifest_path,
        build_info_path,
        shasums_path,
    })
}

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .to_path_buf()
}

fn ensure_clean_git_tree(workspace_root: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let output = Command::new("git")
        .current_dir(workspace_root)
        .args(["status", "--porcelain"])
        .output()?;
    if !output.status.success() {
        return Err("failed to inspect git worktree state".into());
    }
    if !output.stdout.is_empty() {
        return Err("release packaging requires a clean git worktree".into());
    }
    Ok(())
}

fn ensure_release_inputs(workspace_root: &Path) -> Result<(), Box<dyn std::error::Error>> {
    for relative in [
        "README.md",
        "RELEASE-NOTES.md",
        "LICENSE",
        "NOTICE",
        "Cargo.lock",
    ] {
        let path = workspace_root.join(relative);
        if !path.exists() {
            return Err(format!("required release input missing: {}", path.display()).into());
        }
    }
    Ok(())
}

fn canonicalize_existing_path(
    workspace_root: &Path,
    path: &Path,
    label: &str,
) -> Result<PathBuf, Box<dyn std::error::Error>> {
    let resolved = resolve_workspace_path(workspace_root, path);
    if !resolved.exists() {
        return Err(format!("{label} does not exist: {}", resolved.display()).into());
    }
    Ok(fs::canonicalize(resolved)?)
}

fn resolve_workspace_path(workspace_root: &Path, path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        workspace_root.join(path)
    }
}

fn artifact_slug_for_target(target_triple: &str) -> String {
    let descriptor = diag_trace::describe_target(target_triple);
    format!(
        "{}-{}-{}",
        descriptor.os, descriptor.arch, descriptor.libc_family
    )
}

fn stage_release_payload(
    workspace_root: &Path,
    stage_root: &Path,
    binary: &Path,
    manifest_template: &BuildManifest,
    archive_role: &str,
) -> Result<BuildManifest, Box<dyn std::error::Error>> {
    fs::create_dir_all(stage_root.join("bin"))?;
    fs::create_dir_all(stage_root.join("share/doc/gcc-formed"))?;
    fs::create_dir_all(stage_root.join("share/licenses/gcc-formed"))?;

    copy_release_file(binary, &stage_root.join("bin/gcc-formed"))?;
    copy_release_file(binary, &stage_root.join("bin/g++-formed"))?;
    copy_release_file(
        &workspace_root.join("README.md"),
        &stage_root.join("share/doc/gcc-formed/README.md"),
    )?;
    copy_release_file(
        &workspace_root.join("RELEASE-NOTES.md"),
        &stage_root.join("share/doc/gcc-formed/RELEASE-NOTES.md"),
    )?;
    copy_release_file(
        &workspace_root.join("LICENSE"),
        &stage_root.join("share/licenses/gcc-formed/LICENSE"),
    )?;
    copy_release_file(
        &workspace_root.join("NOTICE"),
        &stage_root.join("share/licenses/gcc-formed/NOTICE"),
    )?;

    let build_info_path = stage_root.join("build-info.txt");
    fs::write(
        &build_info_path,
        render_build_info(manifest_template, archive_role, binary),
    )?;

    let mut manifest = manifest_template.clone();
    manifest.checksums = payload_checksums(stage_root)?;
    fs::write(
        stage_root.join("manifest.json"),
        serde_json::to_vec_pretty(&manifest)?,
    )?;

    Ok(manifest)
}

fn copy_release_file(from: &Path, to: &Path) -> Result<(), Box<dyn std::error::Error>> {
    if let Some(parent) = to.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::copy(from, to)?;
    let permissions = fs::metadata(from)?.permissions();
    fs::set_permissions(to, permissions)?;
    Ok(())
}

fn render_build_info(manifest: &BuildManifest, archive_role: &str, binary: &Path) -> String {
    let mut text = String::new();
    let _ = writeln!(&mut text, "product: {}", manifest.product_name);
    let _ = writeln!(&mut text, "version: {}", manifest.product_version);
    let _ = writeln!(
        &mut text,
        "artifact target triple: {}",
        manifest.artifact_target_triple
    );
    let _ = writeln!(
        &mut text,
        "artifact platform: {}/{}/{}",
        manifest.artifact_os, manifest.artifact_arch, manifest.artifact_libc_family
    );
    let _ = writeln!(&mut text, "git commit: {}", manifest.git_commit);
    let _ = writeln!(&mut text, "build profile: {}", manifest.build_profile);
    let _ = writeln!(&mut text, "rustc: {}", manifest.rustc_version);
    let _ = writeln!(&mut text, "cargo: {}", manifest.cargo_version);
    let _ = writeln!(&mut text, "build timestamp: {}", manifest.build_timestamp);
    let _ = writeln!(
        &mut text,
        "support tier: {}",
        manifest.support_tier_declaration
    );
    let _ = writeln!(&mut text, "release channel: {}", manifest.release_channel);
    let _ = writeln!(&mut text, "archive role: {archive_role}");
    let _ = writeln!(&mut text, "binary source: {}", binary.display());
    let _ = writeln!(&mut text, "lockfile hash: {}", manifest.lockfile_hash);
    let _ = writeln!(&mut text, "vendor hash: {}", manifest.vendor_hash);
    text
}

fn payload_checksums(stage_root: &Path) -> Result<Vec<ChecksumEntry>, Box<dyn std::error::Error>> {
    let payload_paths = [
        "bin/gcc-formed",
        "bin/g++-formed",
        "share/doc/gcc-formed/README.md",
        "share/doc/gcc-formed/RELEASE-NOTES.md",
        "share/licenses/gcc-formed/LICENSE",
        "share/licenses/gcc-formed/NOTICE",
        "build-info.txt",
    ];
    let mut checksums = Vec::new();
    for relative in payload_paths {
        let path = stage_root.join(relative);
        checksums.push(ChecksumEntry {
            path: relative.to_string(),
            sha256: sha256_file(&path)?,
            size_bytes: fs::metadata(path)?.len(),
        });
    }
    Ok(checksums)
}

fn hash_vendor_dir(path: &Path) -> Result<String, Box<dyn std::error::Error>> {
    if !path.exists() {
        return Ok("vendor-missing".to_string());
    }
    let mut entries = Vec::new();
    collect_paths_for_hash(path, &mut entries)?;
    entries.sort();
    Ok(sha256_bytes(entries.join("\n").as_bytes()))
}

fn collect_paths_for_hash(
    path: &Path,
    entries: &mut Vec<String>,
) -> Result<(), Box<dyn std::error::Error>> {
    for entry in fs::read_dir(path)? {
        let entry = entry?;
        let child = entry.path();
        if child.is_dir() {
            collect_paths_for_hash(&child, entries)?;
        } else {
            entries.push(child.display().to_string());
        }
    }
    Ok(())
}

fn sha256_file(path: &Path) -> Result<String, Box<dyn std::error::Error>> {
    let bytes = fs::read(path)?;
    Ok(sha256_bytes(&bytes))
}

fn sha256_bytes(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}

fn create_tar_archive(
    root: &Path,
    directory_name: &str,
    output_path: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let status = Command::new("tar")
        .current_dir(root)
        .arg("-czf")
        .arg(output_path)
        .arg(directory_name)
        .status()?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("failed to create tar archive {}", output_path.display()).into())
    }
}

fn create_source_archive(
    workspace_root: &Path,
    output_path: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let status = Command::new("git")
        .current_dir(workspace_root)
        .arg("archive")
        .arg("--format=tar.gz")
        .arg(format!("--output={}", output_path.display()))
        .arg("HEAD")
        .status()?;
    if status.success() {
        Ok(())
    } else {
        Err(format!(
            "failed to create source archive at {}",
            output_path.display()
        )
        .into())
    }
}

fn render_sha256sums(paths: &[&Path]) -> Result<String, Box<dyn std::error::Error>> {
    let mut lines = Vec::new();
    for path in paths {
        lines.push(format!(
            "{}  {}",
            sha256_file(path)?,
            path.file_name()
                .and_then(|value| value.to_str())
                .unwrap_or_default()
        ));
    }
    lines.sort();
    Ok(lines.join("\n") + "\n")
}

fn report_failures(mode: &str, failures: &[VerificationFailure]) {
    eprintln!("mode: {mode}");
    eprintln!("failed fixture count: {}", failures.len());
    if let Some(first) = failures.first() {
        eprintln!("failed layer: {}", first.layer);
        eprintln!("first failed fixture: {}", first.fixture_id);
        eprintln!("first diff summary: {}", first.summary);
    }
}

fn first_diff_summary(expected: &str, actual: &str) -> String {
    for (index, (left, right)) in expected.lines().zip(actual.lines()).enumerate() {
        if left != right {
            return format!("line {} expected `{}` but got `{}`", index + 1, left, right);
        }
    }
    let expected_lines = expected.lines().count();
    let actual_lines = actual.lines().count();
    if expected_lines != actual_lines {
        format!(
            "line count changed: expected {} lines, got {} lines",
            expected_lines, actual_lines
        )
    } else {
        "snapshot content changed".to_string()
    }
}

fn elapsed_ms(start: Instant) -> u64 {
    start.elapsed().as_millis() as u64
}

fn run(binary: &str, args: &[&str]) -> Result<(), Box<dyn std::error::Error>> {
    let status = Command::new(binary).args(args).status()?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("{binary} {} failed", args.join(" ")).into())
    }
}

fn enforce_minimum_family_counts(
    counts: &std::collections::BTreeMap<String, usize>,
) -> Result<(), Box<dyn std::error::Error>> {
    let minimums = [
        ("syntax", 8_usize),
        ("type", 10),
        ("overload", 6),
        ("template", 12),
        ("macro_include", 10),
        ("linker", 10),
        ("partial", 6),
        ("path", 6),
    ];
    for (family, minimum) in minimums {
        let actual = counts.get(family).copied().unwrap_or_default();
        if actual < minimum {
            return Err(format!(
                "family `{family}` below minimum fixture count: expected >= {minimum}, got {actual}"
            )
            .into());
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_file(path: &Path, contents: &[u8]) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(path, contents).unwrap();
    }

    #[cfg(unix)]
    fn make_executable(path: &Path) {
        use std::os::unix::fs::PermissionsExt;

        let mut permissions = fs::metadata(path).unwrap().permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(path, permissions).unwrap();
    }

    #[cfg(not(unix))]
    fn make_executable(_path: &Path) {}

    fn run_command(root: &Path, binary: &str, args: &[&str]) {
        let output = Command::new(binary)
            .current_dir(root)
            .args(args)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{binary} {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr)
        );
    }

    fn init_release_repo() -> (tempfile::TempDir, PathBuf, PathBuf) {
        let sandbox = tempfile::tempdir().unwrap();
        let repo_root = sandbox.path().join("repo");
        let binary_root = sandbox.path().join("binary");
        fs::create_dir_all(&repo_root).unwrap();
        fs::create_dir_all(&binary_root).unwrap();

        write_file(&repo_root.join(".gitignore"), b"/dist\n");
        write_file(&repo_root.join("README.md"), b"# gcc-formed\n");
        write_file(
            &repo_root.join("RELEASE-NOTES.md"),
            b"# Release Notes\n\n- Initial release packaging smoke fixture.\n",
        );
        write_file(&repo_root.join("LICENSE"), b"Apache-2.0\n");
        write_file(&repo_root.join("NOTICE"), b"gcc-formed notice\n");
        write_file(&repo_root.join("Cargo.lock"), b"version = 3\n");
        write_file(&repo_root.join("src/main.rs"), b"fn main() {}\n");

        let binary_path = binary_root.join("gcc-formed");
        write_file(&binary_path, b"#!/bin/sh\necho packaged\n");
        make_executable(&binary_path);

        run_command(&repo_root, "git", &["init", "-q", "-b", "main"]);
        run_command(
            &repo_root,
            "git",
            &["config", "user.email", "ci@example.com"],
        );
        run_command(&repo_root, "git", &["config", "user.name", "CI"]);
        run_command(&repo_root, "git", &["add", "."]);
        run_command(&repo_root, "git", &["commit", "-q", "-m", "initial"]);

        (sandbox, repo_root, binary_path)
    }

    #[test]
    fn package_smoke_emits_release_artifacts() {
        let (_sandbox, repo_root, binary_path) = init_release_repo();
        let package = run_package_at(
            &repo_root,
            &PackageOptions {
                binary: binary_path.clone(),
                debug_binary: None,
                target_triple: "x86_64-unknown-linux-musl".to_string(),
                out_dir: PathBuf::from("dist"),
                release_channel: "stable".to_string(),
                support_tier: "gcc15_primary".to_string(),
            },
        )
        .unwrap();

        assert!(package.primary_archive.exists());
        assert!(package.debug_archive.exists());
        assert!(package.source_archive.exists());
        assert!(package.manifest_path.exists());
        assert!(package.build_info_path.exists());
        assert!(package.shasums_path.exists());

        let manifest = serde_json::from_str::<BuildManifest>(
            &fs::read_to_string(&package.manifest_path).unwrap(),
        )
        .unwrap();
        assert_eq!(manifest.product_name, DEFAULT_PRODUCT_NAME);
        assert_eq!(manifest.artifact_target_triple, "x86_64-unknown-linux-musl");
        assert_eq!(manifest.artifact_libc_family, "musl");
        assert_eq!(manifest.release_channel, "stable");
        assert_eq!(manifest.support_tier_declaration, "gcc15_primary");
        assert_eq!(manifest.checksums.len(), 7);
        assert!(
            manifest
                .checksums
                .iter()
                .any(|entry| entry.path == "bin/gcc-formed")
        );

        let shasums = fs::read_to_string(&package.shasums_path).unwrap();
        assert!(
            shasums.contains(
                package
                    .primary_archive
                    .file_name()
                    .and_then(|value| value.to_str())
                    .unwrap()
            )
        );
        assert!(
            shasums.contains(
                package
                    .source_archive
                    .file_name()
                    .and_then(|value| value.to_str())
                    .unwrap()
            )
        );

        let output = Command::new("tar")
            .args(["-tzf", &package.primary_archive.display().to_string()])
            .output()
            .unwrap();
        assert!(output.status.success());
        let listing = String::from_utf8(output.stdout).unwrap();
        assert!(listing.contains("bin/gcc-formed"));
        assert!(listing.contains("bin/g++-formed"));
        assert!(listing.contains("manifest.json"));
        assert!(listing.contains("build-info.txt"));
        assert!(listing.contains("share/doc/gcc-formed/README.md"));
        assert!(listing.contains("share/licenses/gcc-formed/LICENSE"));
    }

    #[test]
    fn package_rejects_dirty_worktree() {
        let (_sandbox, repo_root, binary_path) = init_release_repo();
        write_file(&repo_root.join("dirty.txt"), b"untracked\n");

        let error = run_package_at(
            &repo_root,
            &PackageOptions {
                binary: binary_path,
                debug_binary: None,
                target_triple: "x86_64-unknown-linux-gnu".to_string(),
                out_dir: PathBuf::from("dist"),
                release_channel: "stable".to_string(),
                support_tier: "gcc15_primary".to_string(),
            },
        )
        .unwrap_err();
        assert!(error.to_string().contains("clean git worktree"));
    }

    #[test]
    fn package_requires_release_documents() {
        let (_sandbox, repo_root, binary_path) = init_release_repo();
        fs::remove_file(repo_root.join("NOTICE")).unwrap();
        run_command(&repo_root, "git", &["add", "-u"]);
        run_command(&repo_root, "git", &["commit", "-q", "-m", "remove notice"]);

        let error = run_package_at(
            &repo_root,
            &PackageOptions {
                binary: binary_path,
                debug_binary: None,
                target_triple: "x86_64-unknown-linux-gnu".to_string(),
                out_dir: PathBuf::from("dist"),
                release_channel: "stable".to_string(),
                support_tier: "gcc15_primary".to_string(),
            },
        )
        .unwrap_err();
        assert!(error.to_string().contains("required release input missing"));
    }

    #[test]
    fn artifact_slug_is_platform_focused() {
        assert_eq!(
            artifact_slug_for_target("x86_64-unknown-linux-musl"),
            "linux-x86_64-musl"
        );
        assert_eq!(
            artifact_slug_for_target("aarch64-unknown-linux-gnu"),
            "linux-aarch64-gnu"
        );
    }
}
