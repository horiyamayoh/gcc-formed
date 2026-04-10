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

# ADR-0029: Path B and Path C are first-class product paths

- **Status**: Accepted
- **Date**: 2026-04-09

## Context

現行 repo は `GCC 15` を本線に据え、`GCC13-14` を狭い補助帯域、より古い帯域を実質 scope 外として扱ってきた。これは v1alpha の品質主張としては合理的だったが、vNext の doctrine が求める「GCC 9〜15 にまたがる複数 path で 1 つの UX 原則を返す repo」とは整合しない。

band ごとの差はあってよいが、spec / tests / issues / roadmap 上の正規対象から `GCC13-14` と `GCC9-12` を外すと、delivery system 全体が再び single-track に戻る。

## Decision

- `GCC13-14` と `GCC9-12` を first-class product paths として扱う
- これは identical guarantee を即時に主張する意味ではなく、spec / tests / issue taxonomy / quality gates / roadmap 上の正式対象にするという意味である
- `GCC13-14` には `NativeTextCapture` と `SingleSinkStructured` の両 path を用意する
- `GCC9-12` についても `NativeTextCapture` と JSON-based structured path を正式対象として扱う

## Consequences

- Path B/C は backlog の「後で足す fallback」ではなく現在の設計対象になる
- quality matrix と corpus の再構成が必要になる
- public wording は quality claim の幅と product scope を分けて説明する必要がある

## Out of Scope

- 全 band の identical quality guarantee
- すべての diagnostic family に対する即時改善
- non-GCC compiler bands

## Supersedes/Related

- **Supersedes**: None
- **Related**: `ADR-0004`, `ADR-0005`, `ADR-0018`, `ADR-0026`, `ADR-0027`

## Source Specs

- `../docs/architecture/gcc-formed-vnext-change-design.md`
- `../docs/support/SUPPORT-BOUNDARY.md`
- `../docs/specs/quality-corpus-test-gate-spec.md`
- `../docs/process/implementation-bootstrap-sequence.md`
