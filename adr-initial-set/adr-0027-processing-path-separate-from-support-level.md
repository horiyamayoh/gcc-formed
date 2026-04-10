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

# ADR-0027: Processing Path is separate from Support Level

- **Status**: Accepted
- **Date**: 2026-04-09

## Context

現行 repo は GCC version と単一 tier vocabulary から runtime mode と public wording を直接決めている。そのため `GCC13-14` やそれ未満の経路は狭い補助帯域や passthrough に寄りやすく、「実際にどの path で価値を返したのか」と「その artifact がどの程度保証するか」が混線する。

vNext では同じ compiler band でも `TTY default`, `CI`, `explicit structured mode` で最適 path が変わりうる。runtime path と support claim は別概念として固定する必要がある。

## Decision

- 実行経路は `ProcessingPath` として表現する
- 最低限 `DualSinkStructured`, `SingleSinkStructured`, `NativeTextCapture`, `Passthrough` を持つ
- `SupportLevel` は public quality claim を表す別概念とし、runtime path を直接表さない
- docs, issue forms, PR template, trace bundle では `VersionBand` と `ProcessingPath` を併記できるようにする

## Consequences

- `GCC13-14` / `GCC9-12` でも「何の path で動いたか」を明示できる
- runtime resolution を `VersionBand x CapabilityProfile -> ProcessingPath` で整理しやすくなる
- release wording と runtime behavior を段階的に独立して改善できる

## Out of Scope

- 各 path の full implementation
- `SupportLevel` の artifact-line policy 変更
- path ごとの render budget 詳細

## Supersedes/Related

- **Supersedes**: None
- **Related**: `ADR-0005`, `ADR-0019`, `ADR-0026`, `ADR-0029`

## Source Specs

- `../docs/architecture/gcc-formed-vnext-change-design.md`
- `../docs/support/SUPPORT-BOUNDARY.md`
- `../docs/process/EXECUTION-MODEL.md`
- `../docs/process/implementation-bootstrap-sequence.md`
