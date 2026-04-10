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

# ADR-0028: CaptureBundle becomes the only ingest entry

- **Status**: Accepted
- **Date**: 2026-04-09

## Context

現行 adapter 境界は実質的に `sarif_path + stderr_text` を中心にしており、`GCC15+` の SARIF path には合うが、`SingleSinkStructured(JSON)` や `NativeTextCapture` を first-class path として扱いにくい。IR 自体はより広い source を受け止められるのに、ingest API が path-aware になっていない。

vNext では Path A/B/C を同じ ingest 境界へ流し込み、runtime と adapter の責務を明確に分ける必要がある。

## Decision

- ingest の唯一の入口を `CaptureBundle` にする
- `CaptureBundle` は invocation metadata、version band、capability profile、resolved processing path、raw text artifacts、structured artifacts、exit status、integrity/provenance 情報を表現できるようにする
- adapter は `CaptureBundle -> DiagnosticDocument` を責務とし、新しい one-off ingest entry を増やさない
- `IngestReport` により source authority、confidence ceiling、fallback grade を返せるようにする

## Consequences

- SARIF-only 前提を外し、JSON / native text / future toolchains へ広げやすくなる
- capture runtime と adapter の境界が明確になる
- no-behavior-change abstraction を先に入れやすくなる

## Out of Scope

- すべての structured format parser の即時実装
- cross-toolchain normalization policy
- render/theme policy の詳細

## Supersedes/Related

- **Supersedes**: None
- **Related**: `ADR-0003`, `ADR-0014`, `ADR-0026`, `ADR-0027`

## Source Specs

- `../docs/architecture/gcc-formed-vnext-change-design.md`
- `../docs/specs/gcc-adapter-ingestion-spec.md`
- `../docs/specs/diagnostic-ir-v1alpha-spec.md`
- `../docs/process/implementation-bootstrap-sequence.md`
