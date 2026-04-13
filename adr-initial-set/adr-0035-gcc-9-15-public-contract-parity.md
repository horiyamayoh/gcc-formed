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

# ADR-0035: GCC 9-15 share one public contract

- **Status**: Accepted
- **Date**: 2026-04-13

## Context

repo は `VersionBand`, `CapabilityProfile`, `ProcessingPath`, `SupportLevel` への分解を進めてきたが、実装・docs・CI・corpus の一部には依然として `GCC15` を事実上の reference product path とみなし、`GCC13-14` / `GCC9-12` を狭い fidelity claim に押し込める解釈が残っていた。

この状態では、band-aware observability を残しても delivery system 全体が再び `GCC15-first` の product hierarchy を再注入する。`VersionBand` と `ProcessingPath` を public metadata として残す価値はあるが、それらが in-scope bands の user value claim を弱める根拠になってはならない。

## Decision

- `GCC15`, `GCC13-14`, `GCC9-12` は **1 つの in-scope public contract** を共有する
- `SupportLevel` の public meaning は binary とし、in-scope bands では `InScope`、out-of-scope compilers では `PassthroughOnly` を使う
- `VersionBand` と `ProcessingPath` は public observability metadata として残すが、in-scope bands の public value hierarchy を正当化してはならない
- internal capture capability は band ごとに異なってよい。`DualSinkStructured`, `SingleSinkStructured`, `NativeTextCapture` は capability/profile の差として扱い、public contract の差として扱わない
- `GCC16+` と unknown gcc-like compilers は、separate evidence が揃うまで `PassthroughOnly` とする
- PR / nightly / RC の diagnostic blocker は `gcc9_12`, `gcc13_14`, `gcc15` を parity matrix として扱い、release-only artifact smoke だけを `gcc15` lane に残してよい

## Consequences

- current-authority docs は in-scope bands に public hierarchy を付ける live wording をやめる必要がある
- public JSON / self-check / trace / report の labels は `gcc15` / `gcc13_14` / `gcc9_12` / `gcc16_plus` と `in_scope` / `passthrough_only` に揃う
- corpus / replay / snapshot gate は `VersionBand × ProcessingPath × Surface` coverage を parity contract の観点で検証する必要がある
- release-only packaging / signing / install smoke が `gcc15` lane に残っても、それは diagnostic contract の hierarchy を意味しない

## Out of Scope

- `GCC16+` や unknown gcc-like compilers を即時に in-scope contract へ含めること
- 全 capability path で同一の same-run raw preservation を約束すること
- 全 diagnostic family で即時に identical internal fidelity を達成すること

## Supersedes/Related

- **Supersedes**: `ADR-0029` のうち、`GCC13-14` / `GCC9-12` を first-class としつつも public claim を band-ranked に読む余地を残していた解釈
- **Related**: `ADR-0020`, `ADR-0026`, `ADR-0027`, `ADR-0028`, `ADR-0029`, `ADR-0031`, `ADR-0033`

## Source Specs

- `../README.md`
- `../docs/support/SUPPORT-BOUNDARY.md`
- `../docs/specs/public-machine-readable-diagnostic-surface-spec.md`
- `../docs/specs/gcc-adapter-ingestion-spec.md`
- `../docs/specs/quality-corpus-test-gate-spec.md`
