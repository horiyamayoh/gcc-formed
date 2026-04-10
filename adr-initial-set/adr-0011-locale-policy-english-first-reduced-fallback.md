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

# ADR-0011: Locale policy: English-first, reduced fallback

- **Status**: Accepted
- **Date**: 2026-04-07

## Context

v1alpha の renderer は compiler facts を圧縮して再提示するが、locale ごとの揺れを早期に導入すると label catalog、snapshot、fallback 挙動が不安定になる。多言語化は将来価値があるが、初期品質を崩さずに扱う必要がある。

## Decision

- v1alpha で renderer が付加する UI ラベルは英語固定とする
- locale 対応は post-MVP の別判断とし、現時点では English-first policy を採る
- locale 差異で fidelity が崩れる場合は reduced fallback を優先する
- compiler raw message の locale 依存は adapter / fallback 側で保守的に扱う

## Consequences

- snapshot と label catalog を安定させやすい
- localized UX は v1alpha の goal から外れる
- non-English 環境では reduced mode や raw fallback を使う場面が残る

## Out of Scope

- 完全なローカライズ
- locale ごとの独立した renderer profile
- 翻訳品質保証

## Supersedes/Related

- **Supersedes**: None
- **Related**: `ADR-0019`, `ADR-0020`

## Source Specs

- `../docs/history/architecture/gcc-formed-architecture-proposal.md` の 19
- `../docs/specs/rendering-ux-contract-spec.md` の locale / label catalog 関連節
- `../docs/specs/gcc-adapter-ingestion-spec.md` の locale / environment sanitization 関連節
