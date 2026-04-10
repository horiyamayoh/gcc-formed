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

# ADR-0013: SARIF egress scope

- **Status**: Accepted
- **Date**: 2026-04-07

## Context

GCC ingress では SARIF が一次情報源になるが、製品内部の analysis や renderer 都合をそのまま SARIF へ戻すと、export の意味論が曖昧になる。SARIF egress の範囲を先に制限しないと、internal IR と export format の責務が混ざる。

## Decision

- SARIF は ingress source および optional export target として扱う
- v1alpha の SARIF egress は projection であり、internal canonical model ではない
- raw pass-through と enriched export の差は明示し、暗黙の上書きをしない
- export 時も provenance と fidelity を損なう変換を避ける

## Consequences

- internal IR の進化と export contract を切り離せる
- SARIF export 実装は separate concern として扱える
- renderer / analysis の内部都合を SARIF schema へ押し込まなくて済む

## Out of Scope

- v1alpha での fully enriched SARIF standardization
- ingress と egress を同一 schema で無理に統一すること
- compiler vendor ごとの差分吸収を SARIF だけで完結させること

## Supersedes/Related

- **Supersedes**: None
- **Related**: `ADR-0003`, `ADR-0012`, `ADR-0020`

## Source Specs

- `../docs/history/architecture/gcc-formed-architecture-proposal.md` の 6.2.3、19
- `../docs/specs/gcc-adapter-ingestion-spec.md` の SARIF ingest / residual handling 関連節
- `../docs/specs/diagnostic-ir-v1alpha-spec.md` の export / external reference 関連節
