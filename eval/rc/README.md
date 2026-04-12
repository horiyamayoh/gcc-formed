# RC Gate Manual Evidence

`cargo xtask rc-gate` は curated replay / rollout matrix / benchmark / deterministic replay / fuzz smoke / human-eval bundle を自動実行しつつ、まだ fully automated でない RC sign-off evidence をこの directory から読み込む。

現在の入力ファイル:

- `metrics-manual-eval.json`: TRC / TFAH / first-fix success / high-confidence mislead の manual raw-GCC comparison packet
- `issue-budget.json`: RC 時点の open `P0` / `P1` bug budget
- `ux-signoff.json`: human UX review sign-off 状態
- `bench-smoke-baseline.json`: designated benchmark baseline。`cargo xtask bench-smoke --subset all` の scenario-level `p95_ms` を current-authority scenario names で固定し、report の `baseline_comparison` がこの file を参照する

生成される補助 bundle:

- `cargo xtask human-eval-kit --root corpus --report-dir target/human-eval`
- `cargo xtask rc-gate --report-dir ...` を実行すると `.../human-eval/` に同じ bundle が自動生成される
- bundle には `README.md`, `expert-review-sheet.csv`, `task-study-sheet.csv`, `counterbalance.csv`, `metrics-manual-eval.template.json`, `ux-signoff.template.json`, fixture-local actual/expected artifacts が含まれる
- human-eval bundle は C-first operator packet も兼ね、`compile`, `link`, `include_path`, `macro`, `preprocessor`, `honest_fallback` の各 category を `human-eval-report.json` と CSV に明示する
- `bench-smoke-report.json` には core smoke scenario に加えて `operator_real_workloads`, `band_path_breakdown`, `baseline_comparison` が入り、release candidate の benchmark evidence 正本として保持する

運用ルール:

- machine-readable JSON を更新してから `cargo xtask rc-gate --report-dir ...` を実行する
- manual evidence を埋める前に、生成済みの human-eval bundle を reviewer / participant に配布する
- fuzz / adversarial hardening は `cargo xtask fuzz-smoke --root fuzz --report-dir ...` と `cargo xtask rc-gate` が自動 report を生成する
- `metrics-manual-eval.json` を `approved` にする場合は `high_confidence_mislead_rate`, `trc_improvement_percent`, `tfah_improvement_percent`, `first_fix_success_delta_points` を全て埋める
- `status: "pending"` は strict RC gate では ship blocker として扱う
- ローカル dry-run や nightly observation では `--allow-pending-manual-checks` を使って pending evidence を warning 扱いにできる
- `schema_version` は `1` を維持し、互換を壊すときは `CHANGELOG.md` と docs を更新する
