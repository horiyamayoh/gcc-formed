# CI Gate Artifacts

`pr-gate`、`nightly-gate`、`rc-gate` は、既存の replay / snapshot / release report に加えて、`REPORT_ROOT/gate/` 配下へ step-level observability artifacts を出力する。`rc-gate` は加えて `REPORT_ROOT/rc-gate/` に release-candidate 判定用の machine-readable report、metrics packet、fuzz packet、human-eval bundle を保存する。

`cargo xtask check` は Rust workspace test だけでなく `python3 -B -m unittest discover -s ci -p test_*.py` も実行する。したがって `cargo-xtask-check` step が green であれば、CI helper scripts、support-boundary docs、governance docs、PR template の contract tests も同じ入口で通っている。

## Layout

```text
$REPORT_ROOT/
  gate/
    build-environment.json
    gate-summary.json
    gate-summary.md
    status/
      <nn>-<step>.json
    logs/
      <nn>-<step>.stdout.log
      <nn>-<step>.stderr.log
```

既存の `replay/`, `snapshot/`, `self-check/`, `release/` はそのまま維持し、`rc-gate/` は release-candidate verdict、`metrics-report.json`、`fuzz-smoke-report.json`、manual evaluation packet、`human-eval/` review bundle を保持する。`gate/` は「どの step が、どの command で、どの support tier / GCC version で失敗したか」に加えて、「どの build environment でその結果になったか」を集約する。

## Status Schema

各 `status/*.json` は schema version `2` を使い、最低限次を持つ。

- `workflow`, `job`
- `step.id`, `step.name`, `step.order`, `step.policy`, `step.failure_classification`
- `status`: `success` / `failure` / `skipped_prior_failure` / `skipped_by_policy`
- `command`, `exit_code`
- `fixture`
- `gcc_version`
- `support_tier`
- `artifact_paths`
- `log_paths.stdout`, `log_paths.stderr`
- `started_at`, `finished_at`, `duration_ms`

`step.failure_classification` は、少なくとも `product` / `infrastructure` / `instrumentation` を取り、workflow/platform 側の不調を product regression と混同しないために使う。

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
- summary には `overall_failure_classification`, `failure_classification_counts`, `build_environment_path`, `build_environment` が含まれる。
- `nightly-gate` の `release_blocker_only` steps は、`gcc:13` / `gcc:14` matrix run では `skipped_by_policy` として summary に残る。
- plan にある step が failure より前に missing の場合、summary generation 自体を failure にして instrumentation drift を検知する。
- build-environment capture step が success なのに `build-environment.json` や必要 section が欠けている場合も instrumentation drift として anomaly にする。

## Static Plans

- [pr-gate.json](/home/dhuru/13_gcc-formed/gcc-formed/ci/plans/pr-gate.json)
- [nightly-gate.json](/home/dhuru/13_gcc-formed/gcc-formed/ci/plans/nightly-gate.json)
- [rc-gate.json](/home/dhuru/13_gcc-formed/gcc-formed/ci/plans/rc-gate.json)

plan files は step order, support tier classification, synthetic skip metadata, summary-only command preview を固定する。workflow YAML 側では実際の shell command を `ci/gate_step.py` 経由で実行し、summary は `ci/gate_summary.py` が生成する。
