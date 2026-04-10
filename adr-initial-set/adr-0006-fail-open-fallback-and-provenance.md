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

# ADR-0006: Fail-open fallback and provenance

- **Status**: Accepted
- **Date**: 2026-04-07

## Context

compiler wrapper は build path の最前面に入るため、wrapper 自身の失敗で native compiler 体験を壊してはならない。structured path が使えない状況でも raw diagnostics へ戻り、後から原因追跡できる provenance を残す必要がある。

## Decision

- unsupported tier、parse failure、internal error、budget exceed では fail-open を採る
- raw stderr と child exit status を必ず保持する
- normalized result から raw artifact へ辿れる provenance を持たせる
- wrapper failure は compiler failure を隠さず、user-facing contract は常に conservative にする

## Consequences

- safety は上がるが、structured path 以外の degrade path も製品として扱う必要がある
- trace bundle と render policy は provenance 前提で設計する必要がある
- KPI には fallback rate と fidelity defect rate が含まれる

## Out of Scope

- wrapper 失敗時にも常に enriched output を出すこと
- silent override
- raw artifact を捨てる軽量化優先設計

## Supersedes/Related

- **Supersedes**: None
- **Related**: `ADR-0001`, `ADR-0005`, `ADR-0016`, `ADR-0019`

## Source Specs

- `../docs/history/architecture/gcc-formed-architecture-proposal.md` の 6.1.7、KPI、19
- `../docs/specs/gcc-adapter-ingestion-spec.md` の 1、30、32、33、34
- `../docs/specs/diagnostic-ir-v1alpha-spec.md` の provenance 関連節
