# CI Gate Artifacts

`pr-gate`、`nightly-gate`、`rc-gate` は、既存の replay / snapshot / release report に加えて、`REPORT_ROOT/gate/` 配下へ step-level observability artifacts を出力する。`pr-gate` は intentionally `gcc15_plus` の reference path で、`nightly-gate` は `GCC13-14` / `GCC15+` の matrix へ coverage を広げる。`replay/` には `replay-report.json` に加えて `native-parity-report.json` を出し、line budget / disclosure honesty / color meaning / compaction の stop-ship分類を機械可読で残す。さらに `gate/replay-stop-ship.json` は `replay-report.json` を band / path / surface / concern へ正規化した gate blocker 正本とする。`rc-gate` は加えて `REPORT_ROOT/rc-gate/` に release-candidate 判定用の machine-readable report、metrics packet、fuzz packet、human-eval bundle を保存する。

`cargo xtask check` は `cargo fmt --check`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test --workspace`、representative replay、`python3 -B -m unittest discover -s ci -p test_*.py` を同じ標準 gate として実行する。したがって `cargo-xtask-check` step が green であれば、Rust workspace lint/test、representative replay、CI helper scripts、support-boundary docs、governance docs、PR template の contract tests が同じ入口で通っている。

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

既存の `replay/`, `snapshot/`, `self-check/`, `release/` はそのまま維持し、`replay/` の `native-parity-report.json` は representative replay の分類正本とする。`gate/replay-stop-ship.json` は missing `VersionBand × ProcessingPath × Surface` cell と path-aware quality blocker を reviewer 向け prose に潰す前の machine-readable blocker list とする。`rc-gate/` は release-candidate verdict、`metrics-report.json`、`native-parity-report.json`、`fuzz-smoke-report.json`、manual evaluation packet、`human-eval/` review bundle を保持する。`gate/` は「どの step が、どの command で、どの gate scope / GCC version / VersionBand で失敗したか」に加えて、「どの build environment でその結果になったか」を集約する。

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

`gate_scope` は CI contract 上の step 適用範囲を表し、少なくとも `repository`, `reference_path`, `release_candidate`, `matrix` を使う。`pr-gate` の checked-in plan/workflow はこの `reference_path` slice を `gcc15_plus` で固定し、`nightly-gate` はそこから matrix bands を広げる。`version_band` は `GCC15+` / `GCC13-14` / `GCC9-12` を machine-readable に落とした `gcc15_plus`, `gcc13_14`, `gcc9_12` などの current-authority label を使う。

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
- `nightly-gate` では replay / self-check / snapshot の matrix steps が `gcc:13` / `gcc:14` / `gcc:15` すべてで blocker として実行され、`reference_path_only` steps だけが `gcc15_plus` reference-path run 以外で `skipped_by_policy` になる。
- plan にある step が failure より前に missing の場合、summary generation 自体を failure にして instrumentation drift を検知する。
- build-environment capture step が success なのに `build-environment.json` や必要 section が欠けている場合も instrumentation drift として anomaly にする。

## Static Plans

- [pr-gate.json](/home/dhuru/13_gcc-formed/gcc-formed/ci/plans/pr-gate.json)
- [nightly-gate.json](/home/dhuru/13_gcc-formed/gcc-formed/ci/plans/nightly-gate.json)
- [rc-gate.json](/home/dhuru/13_gcc-formed/gcc-formed/ci/plans/rc-gate.json)

plan files は step order, gate scope / version-band classification, synthetic skip metadata, summary-only command preview を固定する。`pr-gate` は `gcc15_plus` reference path の checked-in contract として扱い、`nightly-gate` では `reference_path_only` policy を使って release packaging / install / dependency checks を `gcc15_plus` reference path に限定し、matrix replay / self-check / snapshot steps は in-scope bands すべての blocker として残す。workflow YAML 側では実際の shell command を `ci/gate_step.py` 経由で実行し、path-aware replay gate は `ci/gate_replay_contract.py` が `replay-report.json` を classification artifact に変換し、summary は `ci/gate_summary.py` が生成する。
