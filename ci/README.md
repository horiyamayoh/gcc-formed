# CI Gate Artifacts

`pr-gate` と `nightly-gate` は、既存の replay / snapshot / release report に加えて、`REPORT_ROOT/gate/` 配下へ step-level observability artifacts を出力する。

## Layout

```text
$REPORT_ROOT/
  gate/
    gate-summary.json
    gate-summary.md
    status/
      <nn>-<step>.json
    logs/
      <nn>-<step>.stdout.log
      <nn>-<step>.stderr.log
```

既存の `replay/`, `snapshot/`, `self-check/`, `release/` はそのまま維持し、`gate/` は「どの step が、どの command で、どの support tier / GCC version で失敗したか」を集約するためだけに追加する。

## Status Schema

各 `status/*.json` は schema version `1` を使い、最低限次を持つ。

- `workflow`, `job`
- `step.id`, `step.name`, `step.order`, `step.policy`
- `status`: `success` / `failure` / `skipped_prior_failure` / `skipped_by_policy`
- `command`, `exit_code`
- `fixture`
- `gcc_version`
- `support_tier`
- `artifact_paths`
- `log_paths.stdout`, `log_paths.stderr`
- `started_at`, `finished_at`, `duration_ms`

## Summary Semantics

- `gate-summary.json` は static plan と recorded status files を照合した正本。
- `gate-summary.md` は同内容の reviewer 向け要約で、GitHub Actions の `GITHUB_STEP_SUMMARY` にも追記される。
- `nightly-gate` の `release_blocker_only` steps は、`gcc:13` / `gcc:14` matrix run では `skipped_by_policy` として summary に残る。
- plan にある step が failure より前に missing の場合、summary generation 自体を failure にして instrumentation drift を検知する。

## Static Plans

- [pr-gate.json](/home/dhuru/13_gcc-formed/gcc-formed/ci/plans/pr-gate.json)
- [nightly-gate.json](/home/dhuru/13_gcc-formed/gcc-formed/ci/plans/nightly-gate.json)

plan files は step order, support tier classification, synthetic skip metadata, summary-only command preview を固定する。workflow YAML 側では実際の shell command を `ci/gate_step.py` 経由で実行し、summary は `ci/gate_summary.py` が生成する。
