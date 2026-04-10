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

# ADR-0017: Dependency allowlist and license policy

- **Status**: Accepted
- **Date**: 2026-04-07

## Context

build path に入る CLI は dependency risk を低く保ち、release artifact の provenance と legal surface を追跡できる必要がある。single-binary distribution を採る以上、dependency selection と license policy を release engineering の一部として固定する必要がある。

## Decision

- dependency は allowlist ベースで選定し、host 固有 probing や重い native dependency を最小化する
- release artifact には manifest、checksum、license report、dependency notice を紐づける
- release build は lockfile 固定、vendor、offline build を原則とする
- legal / security review を corpus / release gate と分離せず扱う

## Consequences

- release reproducibility と supportability が上がる
- dependency 追加のハードルは上がる
- build.rs や platform-specific dependency の導入には強い正当化が必要になる

## Out of Scope

- package manager ごとの独自 dependency graph
- end user primary path としての source build
- 無制限な optional dependency の導入

## Supersedes/Related

- **Supersedes**: None
- **Related**: `ADR-0007`, `ADR-0008`, `ADR-0016`, `ADR-0018`

## Source Specs

- `../docs/history/architecture/gcc-formed-architecture-proposal.md` の配布 / 品質 / release 関連節、19
- `../docs/specs/packaging-runtime-operations-spec.md` の 12、17、19
- `../docs/specs/quality-corpus-test-gate-spec.md` の release / rollout gate 関連節
