# ADR-0012: Native IR JSON as canonical machine-readable output

- **Status**: Accepted
- **Date**: 2026-04-07

## Context

adapter と renderer の境界を安定させるには、機械可読出力の正本を 1 つに固定する必要がある。SARIF は ingress / export に有用だが、product analysis をそのまま表す canonical format にするには制約が多い。

## Decision

- v1alpha の canonical machine-readable output は native IR JSON とする
- internal at-rest / on-wire の正本は `DiagnosticDocument` ベースの JSON とする
- public export を検討する場合も、まず native IR JSON を基準にする
- SARIF は canonical core ではなく、必要に応じた export target として扱う

## Consequences

- IR schema の進化と validation を直接管理できる
- adapter / renderer / test harness の共通 fixture を 1 つにできる
- SARIF egress は projection として別管理が必要になる

## Out of Scope

- public stable schema の完全凍結
- IDE / LSP 専用 projection の標準化
- SARIF を internal canonical model にすること

## Supersedes/Related

- **Supersedes**: None
- **Related**: `ADR-0002`, `ADR-0009`, `ADR-0013`, `ADR-0020`

## Source Specs

- `../gcc-formed-architecture-proposal.md` の 6.2.3、19
- `../diagnostic-ir-v1alpha-spec.md` の serialization / export 関連節、28
- `../rendering-ux-contract-spec.md` の machine-readable projection 関連節
