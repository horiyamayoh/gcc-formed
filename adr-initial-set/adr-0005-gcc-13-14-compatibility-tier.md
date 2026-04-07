# ADR-0005: GCC 13–14 compatibility tier

- **Status**: Accepted
- **Date**: 2026-04-07

## Context

GCC 13–14 には structured diagnostics の一部があるが、GCC 15 と同じ fidelity を前提にした rendering contract はまだ置けない。互換 path を持ちつつ production claim を抑制しないと、v1alpha の品質主張が過剰になる。

## Decision

- GCC 13–14 は compatibility tier とし、v1alpha では production rendering の対象外とする
- 必要に応じて compatibility path や replay / rerun fallback を許容する
- unsupported / degraded / passthrough の区別を user-visible に保つ
- GCC <=12 は passthrough only 扱いとする

## Consequences

- v1alpha の fidelity claim を守りやすい
- implementation は degrade path と raw fallback を明確に持つ必要がある
- corpus と benchmark は tier 別に分けて評価する必要がある

## Out of Scope

- GCC 13–14 を GCC 15 同等に render する保証
- legacy compiler 向けの複雑な text normalization
- downgrade path の美観最適化

## Supersedes/Related

- **Supersedes**: None
- **Related**: `ADR-0004`, `ADR-0006`, `ADR-0018`

## Source Specs

- `../gcc-formed-architecture-proposal.md` の前提 1、6.1.2、19
- `../gcc-adapter-ingestion-spec.md` の support tier 方針、19、20、32、33、34
- `../quality-corpus-test-gate-spec.md` の compatibility matrix 関連節
