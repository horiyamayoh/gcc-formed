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

# ADR-0034: Presentation V2 subject-first blocks stay separate from the machine contract

- **Status**: Accepted
- **Date**: 2026-04-13

## Context

現行の renderer contract は `why:` と dedicated location line と lead-plus-summary session model を前提にしており、Presentation V2 で目指す Subject-first / multi-block / config-driven presentation semantics を十分に固定できていない。

この変更は単なる layout tweak ではない。次の境界を一度に扱う必要がある。

- terminal header grammar
- visible root を block として扱う session model
- display family と internal family の分離
- presentation config / preset の責務と fail-open
- public JSON を presentation customization から独立させること

これらを spec だけに埋め込むと、「どこまでが machine semantics でどこからが presentation policy か」が将来また曖昧になりやすい。

## Decision

- 人間向け terminal presentation は Subject-first block grammar を正本とする。
- canonical header grammar は `severity: [display-family] subject` を基本とし、interactive subject-first preset では inline location suffix を優先する。
- `1 visible root = 1 block` を Presentation V2 の基本 session model とする。cascade-hidden / dependent / duplicate / follow-on は block にしない。
- `lead_plus_summary` や capped summary は legacy compatibility, warning-only optimization, safety cap のために残してよいが、visible root の built-in default にはしない。
- `internal family` は analysis / rulepack / public JSON の machine semantics として維持する。
- `display family` は terminal presentation 専用の human-facing label とし、preset / config で解決してよい。
- public JSON は presentation-independent な machine contract のまま維持し、display family, template id, location host decision を public field に昇格しない。
- presentation config / preset は non-fatal とし、壊れていても compile/link invocation 全体を止めず、built-in default または generic block へ fail-open する。
- rollout は `docs + ADR` 固定を先行し、その後 `opt-in preset`, 最後に `default promotion` の順で進める。

## Consequences

- renderer 実装は analysis / view model / presentation policy / layout をより明確に分ける必要がある。
- 複数 error session は「visible root block の反復」として理解できるようになる。
- CI first-line policy は interactive default と切り離して扱える。
- machine consumer は terminal text ではなく public JSON に依存すべき、という原則がさらに強化される。
- 既存の `why:` 中心の prose-heavy output は legacy compatibility / fallback / low-confidence honesty に限定される。

## Out of Scope

- Presentation V2 preset loader の実装完了
- built-in preset asset format の細部
- default promotion の即時実施
- public JSON schema change

## Supersedes/Related

- **Supersedes**: None
- **Related**: `ADR-0019`, `ADR-0020`, `ADR-0030`, `ADR-0031`

## Source Specs

- `../README.md`
- `../docs/specs/rendering-ux-contract-spec.md`
- `../docs/specs/public-machine-readable-diagnostic-surface-spec.md`
- `../docs/process/EXECUTION-MODEL.md`
