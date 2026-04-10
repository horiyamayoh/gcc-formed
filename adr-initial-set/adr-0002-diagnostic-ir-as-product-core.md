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

# ADR-0002: Diagnostic IR as product core

- **Status**: Accepted
- **Date**: 2026-04-07

## Context

compiler 出力をその場の表示形式に依存したまま扱うと、renderer、analysis、test harness が強く結合する。terminal、CI、将来の editor 連携を同じ事実から再構成するには、compiler-agnostic な Diagnostic IR を製品の中核に置く必要がある。

## Decision

- 製品のコア契約は Diagnostic IR とする
- compiler facts と product interpretation は IR 上で分離する
- adapter、enrichment、renderer、test harness は IR を境界に疎結合化する
- IR 型の ownership は core layer に置き、adapter や renderer 側へ置かない

## Consequences

- 出力 surface を増やしても意味論を共有できる
- raw text parser への依存を renderer 側から追い出せる
- schema validation、snapshot、fidelity 検証を IR 中心で実装できる

## Out of Scope

- public export schema の完全固定
- editor-specific annotation の設計
- compiler vendor ごとの拡張 metadata の全面標準化

## Supersedes/Related

- **Supersedes**: None
- **Related**: `ADR-0003`, `ADR-0009`, `ADR-0012`, `ADR-0015`

## Source Specs

- `../docs/history/architecture/gcc-formed-architecture-proposal.md` の 1、6.1.3、19
- `../docs/specs/diagnostic-ir-v1alpha-spec.md` の 1、27、28
- `../docs/specs/rendering-ux-contract-spec.md` の 1
