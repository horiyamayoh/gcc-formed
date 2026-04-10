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

# ADR-0016: Trace bundle content and redaction

- **Status**: Accepted
- **Date**: 2026-04-07

## Context

wrapper failures や fidelity defects を支援するには raw stderr、structured artifacts、decision log が必要になる。一方で compile path の artifact には path、argv、source excerpt などの機微情報が含まれうるため、trace bundle の中身と redaction policy を明確にする必要がある。

## Decision

- trace bundle は opt-in を原則とし、default-on upload をしない
- bundle には raw stderr、structured artifact、normalized IR、render decision log、version / environment summary、fallback reason を含める
- path、argv、source excerpt などは redaction policy の対象とし、保持範囲を明示する
- trace / state / runtime object は install payload から分離して管理する

## Consequences

- supportability と confidentiality の境界が明確になる
- provenance と fallback investigation がやりやすくなる
- packaging、quality、renderer は bundle を前提に incident response を設計できる

## Out of Scope

- default runtime での外部アップロード
- trace bundle を primary artifact に混在させること
- 組織固有 SIEM 連携の標準化

## Supersedes/Related

- **Supersedes**: None
- **Related**: `ADR-0006`, `ADR-0008`, `ADR-0017`, `ADR-0018`

## Source Specs

- `../docs/history/architecture/gcc-formed-architecture-proposal.md` の 6.1.8、19
- `../docs/specs/gcc-adapter-ingestion-spec.md` の artifact retention / integrity issue 関連節
- `../docs/specs/packaging-runtime-operations-spec.md` の XDG / trace / security 関連節
- `../docs/specs/quality-corpus-test-gate-spec.md` の shadow telemetry / trace harvesting 関連節
