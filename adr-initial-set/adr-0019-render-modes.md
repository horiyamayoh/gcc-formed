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

# ADR-0019: Render modes

- **Status**: Accepted
- **Date**: 2026-04-07

## Context

terminal、CI、reduced fallback の各状況で同じ情報密度を押し付けると、root-cause surfacing と fidelity の両立が難しい。user-visible surface を mode として固定しないと、出力 contract と snapshot がぶれる。

## Decision

- v1alpha の renderer surface は `concise` / `default` / `verbose` / `raw` の 4 mode に固定する
- terminal human mode と CI plain mode は同じ IR から profile-aware に再表示する
- low-confidence / partial / fallback 時は conservative に mode を選び、raw へ安全に戻る
- note flood compression と omission budget は mode / profile ごとに管理する

## Consequences

- user-visible contract と snapshot の軸が明確になる
- fallback wording と density control は mode policy の一部になる
- build-wide aggregated view のような別 surface は post-MVP 扱いになる

## Out of Scope

- interactive explorer
- build-wide multi-invocation aggregated UI
- vendor-specific CI folding の標準化

## Supersedes/Related

- **Supersedes**: None
- **Related**: `ADR-0006`, `ADR-0011`, `ADR-0014`, `ADR-0020`

## Source Specs

- `../docs/history/architecture/gcc-formed-architecture-proposal.md` の 6.1.4、19
- `../docs/specs/rendering-ux-contract-spec.md` の profile / density / raw fallback 関連節
- `../docs/specs/gcc-adapter-ingestion-spec.md` の mode selection / user-facing fallback 関連節
