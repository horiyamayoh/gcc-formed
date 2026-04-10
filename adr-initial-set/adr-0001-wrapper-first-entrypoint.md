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

# ADR-0001: Wrapper-first compiler-compatible entrypoint

- **Status**: Accepted
- **Date**: 2026-04-07

## Context

既存の C/C++ build flow は `gcc` / `g++` 互換の CLI surface を前提にしている。導入障壁を最小化するには、製品を compiler-like wrapper として差し込める必要がある。一方で、本製品の本体は単なる text prettifier ではなく、structured diagnostics を取り込む診断基盤である。

## Decision

- 製品の最初の導入口は compiler-compatible wrapper とする
- エントリポイントは `gcc-formed` / `g++-formed` を基準とし、symlink 名で C/C++ driver を切り替えられるようにする
- wrapper は argv を保守的に透過転送し、real compiler discovery を安全に実装する
- wrapper は導入形態であり、プロダクトの中心は IR と analysis / rendering に置く

## Consequences

- 既存 build system へ段階的に差し込める
- CLI surface は安定性の対象になり、破壊的変更のコストが高い
- wrapper failure は build path 全体へ波及しやすいため、fail-open が前提になる

## Out of Scope

- IDE 専用エントリポイント
- 常駐 daemon 前提の高速化
- build-wide aggregated UI

## Supersedes/Related

- **Supersedes**: None
- **Related**: `ADR-0005`, `ADR-0006`, `ADR-0009`, `ADR-0020`

## Source Specs

- `../docs/history/architecture/gcc-formed-architecture-proposal.md` の Executive Summary、6.1.1、19
- `../docs/specs/gcc-adapter-ingestion-spec.md` の 1、31、32、33
- `../docs/specs/packaging-runtime-operations-spec.md` の 7.2、18.2
