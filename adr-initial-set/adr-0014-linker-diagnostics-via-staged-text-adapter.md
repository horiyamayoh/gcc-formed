# ADR-0014: Linker diagnostics via staged text adapter

- **Status**: Accepted
- **Date**: 2026-04-07

## Context

linker diagnostics は compiler front-end より非構造で、一貫した structured source を最初から期待しにくい。にもかかわらず、undefined reference や multiple definition は v1alpha の主要 family に含めたい。

## Decision

- linker diagnostics は staged text adapter で段階的に扱う
- GCC structured path と linker residual text は source を分けて保持する
- v1alpha では common linker failures に絞って family-aware UX を提供する
- high-confidence classification ができない場合は raw fallback を優先する

## Consequences

- linker family を MVP に入れつつ、過剰な claim を避けられる
- residual classifier と provenance の実装が必要になる
- advanced linker reasoning は post-MVP backlog に残る

## Out of Scope

- linker 全面 structured normalization
- ABI mismatch の完全自動推論
- toolchain ごとの linker 方言の包括対応

## Supersedes/Related

- **Supersedes**: None
- **Related**: `ADR-0003`, `ADR-0010`, `ADR-0019`

## Source Specs

- `../gcc-formed-architecture-proposal.md` の 6.1.5、6.2.2、19
- `../gcc-adapter-ingestion-spec.md` の residual text / linker family 関連節、31、32、33
- `../rendering-ux-contract-spec.md` の linker family 表示契約
