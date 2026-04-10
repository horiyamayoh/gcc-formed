---
doc_role: current-authority
lifecycle_status: accepted-baseline
audience: both
use_for: Accepted design decisions that constrain implementation.
do_not_use_for: Historical superseded policy or workflow detail outside the decision.
supersedes: []
superseded_by: []
---
> [!IMPORTANT]
> Authority: `current-authority` / `accepted-baseline`
> Use for: Accepted design decisions that constrain implementation.
> Do not use for: Historical superseded policy or workflow detail outside the decision.

# ADR-0009: Library + CLI layering

- **Status**: Accepted
- **Date**: 2026-04-07

## Context

wrapper、adapter、IR、analysis、renderer を 1 枚の CLI 実装に閉じ込めると、再利用と将来拡張が難しくなる。terminal、CI、将来の editor bridge を支えるには、library と CLI の責務境界を先に固定する必要がある。

## Decision

- 実装は library + CLI の二層構成を基本にする
- core library は IR、adapter、analysis、renderer の再利用単位を提供する
- CLI は mode selection、backend resolution、user-facing fallback wiring を担当する
- crate / module 境界は core ownership を崩さないように設計する

## Consequences

- 将来 surface を増やしても core logic を共有できる
- test harness は library API を直接使いやすくなる
- CLI convenience と core semantics を分離して安定性管理できる

## Out of Scope

- daemon/service 化
- editor plugin の即時実装
- public SDK の凍結

## Supersedes/Related

- **Supersedes**: None
- **Related**: `ADR-0001`, `ADR-0002`, `ADR-0012`, `ADR-0019`

## Source Specs

- `../docs/history/architecture/gcc-formed-architecture-proposal.md` の推奨アーキテクチャ、19
- `../docs/specs/diagnostic-ir-v1alpha-spec.md` の 27
- `../docs/specs/gcc-adapter-ingestion-spec.md` の 31
