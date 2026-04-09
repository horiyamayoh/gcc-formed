# ADR-0032: Rulepack externalization policy

- **Status**: Accepted
- **Date**: 2026-04-09

## Context

family classification, first-action hint, compaction, residual grouping が Rust code へ直接埋め込まれるほど、Path B/C の追加や wording 変更のたびに if/else が増殖し、保守コストが急上昇する。vNext では path-aware analysis を維持しつつ deterministic で reviewable な rule evolution が必要になる。

## Decision

- rule の意味論を contract 化し、可能な範囲で rulepack として外部化する
- ただし core path では deterministic, versioned, reviewable を維持する
- 初期段階では internal table-driven representation へ寄せ、semantic assertions を rule id 単位で持つ
- external file format は staged adoption とし、抽象化前に全面外出ししない

## Consequences

- docs と tests が rule semantics の正本になる
- enrich / residual parser の保守コストを下げられる
- renderer wording と family logic の結合を弱められる

## Out of Scope

- non-deterministic or network-backed rule evaluation
- immediate full DSL design
- reviewer なしの runtime rule mutation

## Supersedes/Related

- **Supersedes**: None
- **Related**: `ADR-0010`, `ADR-0015`, `ADR-0030`

## Source Specs

- `../gcc-formed-vnext-change-design.md`
- `../quality-corpus-test-gate-spec.md`
- `../rendering-ux-contract-spec.md`
- `../EXECUTION-MODEL.md`
