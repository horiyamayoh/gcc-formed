---
doc_role: reference-only
lifecycle_status: draft
audience: both
use_for: Reference-only planning workspace for active drafts.
do_not_use_for: Normative implementation decisions.
supersedes: []
superseded_by: []
---
> [!IMPORTANT]
> Authority: `reference-only` / `draft`
> Use for: Reference-only planning workspace for active drafts.
> Do not use for: Normative implementation decisions.

# Planning Workspace

`docs/planning/` は active work-in-progress を一時的に置く場所である。

- ここに置く文書は **reference-only** として扱う
- implementation の正本判断は `README.md`, `AGENTS.md`, `docs/README.md`, accepted ADR, current authority docs を優先する
- 作業が完了したら current authority へ昇格するか、`docs/history/` へ移す

## Active Drafts

- [gcc_formed_issue_map_v1_ja.md](gcc_formed_issue_map_v1_ja.md): GitHub Issues / Projects に落とすための実行設計書
- [gcc_formed_execution_slice_catalog_v1_ja.md](gcc_formed_execution_slice_catalog_v1_ja.md): issue emission と execution slice packaging の最終カタログ
- [gcc_formed_execution_slice_bundle_v1.json](gcc_formed_execution_slice_bundle_v1.json): catalog を機械可読化した bundle
