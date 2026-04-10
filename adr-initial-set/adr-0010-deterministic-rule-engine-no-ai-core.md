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

# ADR-0010: Deterministic rule engine; no AI core dependency

- **Status**: Accepted
- **Date**: 2026-04-07

## Context

v1alpha の価値は「賢そうに見える説明」ではなく、root cause ranking と actionable hint を testable に提供することにある。検証不能な AI 説明を core に置くと、品質 gate と誤誘導率の管理が難しくなる。

## Decision

- v1alpha の root-cause analysis は deterministic な rules-based engine を採る
- LLM / 生成 AI を core dependency にしない
- ranking、compression、hint 生成は corpus と golden test で回帰検知できる形にする
- AI 活用を検討する場合も post-MVP の付加機能として扱う

## Consequences

- 誤誘導率と説明品質を test gate に落とし込める
- 実装は family classification と ranking heuristic を明示的に持つ必要がある
- AI による自由文生成を前提にした UX は v1alpha では扱わない

## Out of Scope

- interactive AI assistant
- 自動修正提案の生成 AI 依存
- deterministic でない ranking

## Supersedes/Related

- **Supersedes**: None
- **Related**: `ADR-0002`, `ADR-0014`, `ADR-0018`, `ADR-0019`

## Source Specs

- `../docs/history/architecture/gcc-formed-architecture-proposal.md` の前提 3、6.1.5、19
- `../docs/specs/diagnostic-ir-v1alpha-spec.md` の analysis / overlay 関連節
- `../docs/specs/quality-corpus-test-gate-spec.md` の KPI と gate 関連節
