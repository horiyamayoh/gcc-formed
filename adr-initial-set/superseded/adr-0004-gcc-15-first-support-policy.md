---
doc_role: history-only
lifecycle_status: superseded
audience: both
use_for: Historical design provenance and superseded decisions.
do_not_use_for: Current implementation decisions or current public support claims.
supersedes: []
superseded_by:
  - ../adr-0026-capability-profile-replaces-support-tier.md
  - ../adr-0027-processing-path-separate-from-support-level.md
  - ../adr-0029-path-b-and-c-are-first-class-product-paths.md
---
> [!IMPORTANT]
> Authority: `history-only` / `superseded`
> Use for: Historical design provenance and superseded decisions.
> Do not use for: Current implementation decisions or current public support claims.

# ADR-0004: GCC 15-first support policy

- **Status**: Superseded
- **Date**: 2026-04-07

## Context

品質主張を曖昧にしたまま複数 GCC 系列を同列 support すると、render fidelity と fallback rate の責任範囲が壊れる。GCC 15 は v1alpha で必要な structured diagnostics 契約が最も揃っているため、production-quality claim の基点を明確にする必要がある。

## Decision

- v1alpha の公式サポート本命は Linux 上の GCC 15 系とする
- production-quality rendering を約束する対象は GCC 15+ とする
- support tier は adapter と quality gate の両方で明示的に扱う
- GCC support tier の宣言は corpus と rollout readiness の基準にも使う

## Consequences

- 品質 KPI と fallback rate の解釈が明確になる
- GCC 13–14 や <=12 を同一品質で約束しないで済む
- corpus と CI matrix は GCC 15 を中心に設計する必要がある

## Out of Scope

- GCC 15 未満での production rendering 保証
- Clang support policy
- vendor-patched GCC 派生版の包括保証

## Supersedes/Related

- **Supersedes**: None
- **Related**: `ADR-0003`, `ADR-0018`, `ADR-0026`, `ADR-0029`

## Source Specs

- `../docs/history/architecture/gcc-formed-architecture-proposal.md` の前提 1、6.1.2、Phase 1、19
- `../docs/specs/gcc-adapter-ingestion-spec.md` の support tier 方針、20、33、34
- `../docs/specs/quality-corpus-test-gate-spec.md` の compatibility / rollout readiness 関連節
