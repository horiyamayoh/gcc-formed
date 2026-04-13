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

# ADR-0026: Capability Profile replaces Support Tier

- **Status**: Accepted
- **Date**: 2026-04-09

## Context

現行 repo の `SupportTier::A/B/C` は、GCC version band、structured diagnostics capability、runtime path、user-visible quality claim、fallback expectation を 1 つの概念に押し込めている。そのため `GCC15` / `GCC13-14` / `GCC9-12` の差を説明できても、「何ができるか」と「何を public に約束するか」を分離できない。

vNext では、GCC 15 を privileged path として扱いつつ、GCC 13–14 と GCC 9–12 も first-class product bands として扱う必要がある。単一 tier の public vocabulary ではその設計を支えられない。

## Decision

- `SupportTier` を product の中心概念として使うことをやめる
- 代わりに `VersionBand`, `CapabilityProfile`, `SupportLevel` を導入する
- `CapabilityProfile` は runtime で観測できる機能集合を表し、少なくとも `native_text_capture`, `json_diagnostics`, `sarif_diagnostics`, `dual_sink`, `tty_color_control`, `caret_control`, `parseable_fixits`, `locale_stabilization` を表現できるようにする
- public docs, issue templates, PR templates は `SupportTier` ではなく `VersionBand` と `SupportLevel` を使う
- `SupportTier` は移行期間中のみ legacy compatibility layer として残してよいが、新しい contract surface に露出してはならない

## Consequences

- version band ごとの差を capture 設計に閉じ込めやすくなる
- `GCC13-14` / `GCC9-12` でも「なぜ価値を返せるか」を capability ベースで説明できる
- docs / runtime / tests の vocabulary migration が必要になる

## Out of Scope

- すべての capability flag の初回完全実装
- `SupportTier` の即時削除
- `ProcessingPath` や renderer policy の詳細設計

## Supersedes/Related

- **Supersedes**: None
- **Related**: `ADR-0004`, `ADR-0005`, `ADR-0021`, `ADR-0027`, `ADR-0029`

## Source Specs

- `../docs/architecture/gcc-formed-vnext-change-design.md`
- `../docs/process/EXECUTION-MODEL.md`
- `../docs/support/SUPPORT-BOUNDARY.md`
- `../README.md`
