# RC Gate Manual Evidence

`cargo xtask rc-gate` は curated replay / rollout matrix / benchmark / deterministic replay を自動実行しつつ、まだ fully automated でない RC sign-off evidence をこの directory から読み込む。

現在の入力ファイル:

- `metrics-manual-eval.json`: TRC / TFAH / first-fix success / high-confidence mislead の manual raw-GCC comparison packet
- `issue-budget.json`: RC 時点の open `P0` / `P1` bug budget
- `fuzz-status.json`: fuzz / adversarial hardening の sign-off 状態
- `ux-signoff.json`: human UX review sign-off 状態

運用ルール:

- machine-readable JSON を更新してから `cargo xtask rc-gate --report-dir ...` を実行する
- `metrics-manual-eval.json` を `approved` にする場合は `high_confidence_mislead_rate`, `trc_improvement_percent`, `tfah_improvement_percent`, `first_fix_success_delta_points` を全て埋める
- `status: "pending"` は strict RC gate では ship blocker として扱う
- ローカル dry-run や nightly observation では `--allow-pending-manual-checks` を使って pending evidence を warning 扱いにできる
- `schema_version` は `1` を維持し、互換を壊すときは `CHANGELOG.md` と docs を更新する
