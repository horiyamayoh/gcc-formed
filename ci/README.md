# CI Gate Artifacts

`pr-gate`、`nightly-gate`、`rc-gate` は、既存の replay / snapshot / release report に加えて、`REPORT_ROOT/gate/` 配下へ step-level observability artifacts を出力する。`pr-gate` は `gcc9_12` / `gcc13_14` / `gcc15` の parity matrix で representative replay, self-check, snapshot を blocker として実行し、`nightly-gate` はそこへ追加の real-compiler evidence lane と release-only smoke を重ねる。`replay/` には `replay-report.json` に加えて `native-parity-report.json` を出し、line budget / disclosure honesty / color meaning / compaction の stop-ship分類を機械可読で残す。さらに `gate/replay-stop-ship.json` は `replay-report.json` を band / path / surface / concern へ正規化した gate blocker 正本とし、`matrix-summary.json` / `matrix-summary.md` は nightly lanes をまたいだ missing cell と path-aware regression を lane 単位で集約する。`rc-gate` は加えて `REPORT_ROOT/rc-gate/` に release-candidate 判定用の machine-readable report、metrics packet、fuzz packet、human-eval bundle を保存する。`bench-smoke-report.json` は issue `#131` の current benchmark artifact として core smoke scenario だけでなく `operator_real_workloads`, `band_path_breakdown`, `baseline_comparison` を保持し、designated baseline は checked-in `eval/rc/bench-smoke-baseline.json` を使う。

`cargo xtask check` は `cargo fmt --check`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test --workspace`、representative replay、`python3 -B -m unittest discover -s ci -p test_*.py` を同じ標準 gate として実行する。したがって `cargo-xtask-check` step が green であれば、Rust workspace lint/test、representative replay、CI helper scripts、support-boundary docs、governance docs、PR template の contract tests が同じ入口で通っている。

`cargo xtask ci-gate --workflow <pr|nightly|rc>` は local GitHub CI-equivalent gate であり、GitHub Actions と同じ shared execution catalog を使って `ci/run_gate_step.py` から step を解決する。local 実行は `target/local-gates/<workflow>/` を既定出力先とし、`vendor/`、`dist/`、release repository、signing key は `work/` 配下へ隔離する。

## Layout

```text
$REPORT_ROOT/
  gate/
    build-environment.json
    gate-summary.json
    gate-summary.md
    replay-stop-ship.json
    status/
      <nn>-<step>.json
    logs/
      <nn>-<step>.stdout.log
      <nn>-<step>.stderr.log
```

既存の `replay/`, `snapshot/`, `self-check/`, `release/` はそのまま維持し、`replay/` の `native-parity-report.json` は representative replay の分類正本とする。`gate/replay-stop-ship.json` は missing `VersionBand × ProcessingPath × Surface` cell と path-aware quality blocker を reviewer 向け prose に潰す前の machine-readable blocker list とする。`rc-gate/` は release-candidate verdict、`metrics-report.json`、`native-parity-report.json`、`bench-smoke-report.json`、`fuzz-smoke-report.json`、manual evaluation packet、`human-eval/` review bundle を保持する。`gate/` は「どの step が、どの command で、どの gate scope / GCC version / VersionBand で失敗したか」に加えて、「どの build environment でその結果になったか」を集約する。

## Status Schema

各 `status/*.json` は schema version `3` を使い、最低限次を持つ。

- `workflow`, `job`
- `step.id`, `step.name`, `step.order`, `step.policy`, `step.failure_classification`
- `status`: `success` / `failure` / `skipped_prior_failure` / `skipped_by_policy`
- `command`, `exit_code`
- `fixture`
- `gcc_version`
- `gate_scope`
- `version_band`
- `artifact_paths`
- `log_paths.stdout`, `log_paths.stderr`
- `started_at`, `finished_at`, `duration_ms`

`gate_scope` は CI contract 上の step 適用範囲を表し、少なくとも `repository`, `release_candidate`, `matrix` を使う。`pr-gate` と `nightly-gate` の diagnostic blocker steps は `matrix` scope で `gcc9_12`, `gcc13_14`, `gcc15` を明示し、release-only steps は repository/release-candidate scope に残す。`version_band` は `GCC15` / `GCC13-14` / `GCC9-12` / `GCC16+` を machine-readable に落とした `gcc15`, `gcc13_14`, `gcc9_12`, `gcc16_plus` などの current-authority label を使う。nightly の release-only steps は `release_lane_only` policy で制御し、`gcc15` を “reference path” として扱う意味づけには戻さない。

`step.failure_classification` は、少なくとも `product` / `infrastructure` / `instrumentation` を取り、workflow/platform 側の不調を product regression と混同しないために使う。

`release-beta.yml` と `release-stable.yml` は checked-in static plans の外側にある release workflows で、release provenance が path-aware release evidence の正本になる。release-provenance には `replay-stop-ship` と `rollout-matrix-report` のような release gate artifacts を含め、release notes と GitHub Release assets と整合させる。

`ci/test_release_provenance.py` と `ci/test_gate_scripts.py` は、この release provenance 連携が workflow order と current multi-band vocabulary に従っているかを固定する。workflow step の並び、release provenance の workflow 別 artifact routing、manifest / published-release metadata の `maturity_label` vocabulary、そして legacy `support_tier` / `gcc15_primary` の再混入を contract test で検知する。

## Build Environment Schema

`build-environment.json` は schema version `1` を使い、少なくとも次を持つ。

- `host.runner`
- `host.toolchain_policy`
- `host.rustc`, `host.cargo`, `host.docker`
- `ci_image.requested_base_image`, `ci_image.built_image_tag`, `ci_image.dockerfile`
- `ci_image.image.gcc`, `ci_image.image.rustc`, `ci_image.image.cargo`

PR / nightly ともに host 側の `rustc` / `cargo` / Docker version を先に採取し、CI image build 後に selected GCC image 上の `rustc` / `cargo` / `gcc` を同じ JSON に追記する。

## Summary Semantics

- `gate-summary.json` は static plan と recorded status files を照合した正本。
- `gate-summary.md` は同内容の reviewer 向け要約で、GitHub Actions の `GITHUB_STEP_SUMMARY` にも追記される。
- summary には `overall_failure_classification`, `failure_classification_counts`, `build_environment_path`, `build_environment`, `machine_readable_blockers`, `machine_readable_blocker_counts` が含まれる。
- `matrix-summary.json` / `matrix-summary.md` は nightly lane ごとの blocker を lane / category / matrix_cell / concern 単位で保持し、missing `VersionBand × ProcessingPath × Surface` cells を lane-level failed boolean に潰さない。
- `nightly-gate` では replay / self-check / snapshot の matrix steps が `gcc:12` / `gcc:13` / `gcc:14` / `gcc:15` すべてで blocker として実行され、release-only smoke steps だけが `gcc15` release lane 以外で `skipped_by_policy` になる。
- plan にある step が failure より前に missing の場合、summary generation 自体を failure にして instrumentation drift を検知する。
- build-environment capture step が success なのに `build-environment.json` や必要 section が欠けている場合も instrumentation drift として anomaly にする。

## Public Diagnostic Export

Machine-readable diagnostic consumers should parse the public JSON surface directly instead of scraping terminal text. The contract for that surface lives in [../docs/specs/public-machine-readable-diagnostic-surface-spec.md](../docs/specs/public-machine-readable-diagnostic-surface-spec.md).

CI examples should validate the export before trusting it downstream:

```bash
python3 -m json.tool diagnostic-export.json >/dev/null
jq -e '
  .schema_version != null
  and .kind == "gcc_formed_public_diagnostic_export"
  and .execution.version_band != null
  and .execution.processing_path != null
  and .execution.support_level != null
' diagnostic-export.json
```

When a job consumes a machine-readable export, keep the raw JSON artifact as part of the report root and treat unknown additive fields as forward-compatible unless the current spec says otherwise. Snapshot/report producers may also emit `public.export.schema-shape-fingerprint.txt` as a compatibility sentinel for schema-shape drift review; `public.export.json` remains the checked-in golden artifact.

## Static Plans

- [pr-gate.json](/home/dhuru/13_gcc-formed/gcc-formed/ci/plans/pr-gate.json)
- [nightly-gate.json](/home/dhuru/13_gcc-formed/gcc-formed/ci/plans/nightly-gate.json)
- [rc-gate.json](/home/dhuru/13_gcc-formed/gcc-formed/ci/plans/rc-gate.json)

plan files は step order, gate scope / version-band classification, synthetic skip metadata, summary-only command preview を固定する。`pr-gate` は `gcc9_12` / `gcc13_14` / `gcc15` の checked-in parity contract として扱い、`nightly-gate` では `release_lane_only` policy を使って release-only smoke を `gcc15` release lane に限定しつつ、matrix replay / self-check / snapshot steps は in-scope bands すべての blocker として残す。workflow YAML 側では `ci/run_gate_step.py` が shared execution catalog から実コマンドを解決し、path-aware replay gate は `ci/gate_replay_contract.py` が `replay-report.json` を classification artifact に変換し、summary は `ci/gate_summary.py` が生成する。

実コマンドの正本は `ci/gate_catalog.py` と `ci/run_gate_step.py` にあり、checked-in workflows と `ci/run_local_gate.py` の両方がそこを通る。`nightly` を local 実行するときは `cargo xtask ci-gate --workflow nightly --matrix-lane gcc12|gcc13|gcc14|gcc15|all` を使い、lane ごとの report root に加えて top-level `matrix-summary.json` / `matrix-summary.md` を出力する。top-level summary は missing `VersionBand × ProcessingPath × Surface` cell と path-aware regression を lane 付きで列挙する。
