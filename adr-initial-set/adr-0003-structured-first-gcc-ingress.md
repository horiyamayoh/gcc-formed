# ADR-0003: Structured-first GCC ingress

- **Status**: Accepted
- **Date**: 2026-04-07

## Context

GCC には SARIF や JSON などの structured diagnostics capability があり、text-first parsing は fidelity と保守性の両面で不利である。v1alpha では GCC diagnostics の authoritative source を固定し、plugin 依存や brittle な text parsing を core path に置かない必要がある。

## Decision

- GCC ingress は structured-first とし、GCC SARIF を一次情報源として扱う
- text parsing first を採らず、raw stderr は fallback / provenance のために保持する
- GCC plugin 依存を導入しない
- adapter は capture runtime と分離し、将来の Clang adapter でも再利用できる境界にする

## Consequences

- single-pass structured path を GCC 15+ で成立させやすい
- compiler facts の欠落や誤解釈を抑制できる
- linker や driver の residual text は別扱いにする必要がある

## Out of Scope

- GCC text diagnostics 全面パース
- custom GCC plugin の配布
- non-GCC compiler の同時対応

## Supersedes/Related

- **Supersedes**: None
- **Related**: `ADR-0002`, `ADR-0004`, `ADR-0005`, `ADR-0013`, `ADR-0014`

## Source Specs

- `../gcc-formed-architecture-proposal.md` の前提、1、6.1.2、19
- `../gcc-adapter-ingestion-spec.md` の 1、3、19、31、32、33、34
- `../diagnostic-ir-v1alpha-spec.md` の 20、28
