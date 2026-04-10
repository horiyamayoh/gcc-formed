---
doc_role: current-authority
lifecycle_status: accepted-baseline
audience: both
use_for: Current authority map and reading order inside docs.
do_not_use_for: Historical provenance or superseded planning context.
supersedes: []
superseded_by: []
---
> [!IMPORTANT]
> Authority: `current-authority` / `accepted-baseline`
> Use for: Current authority map and reading order inside docs.
> Do not use for: Historical provenance or superseded planning context.

# Documentation Map

このリポジトリでは、まず「何を信じるべきか」を先に判断できるように、authority-first で文書を読む。

AI コーディングエージェントの入口は [../AGENTS.md](../AGENTS.md) である。

## Current Authority

- [support/SUPPORT-BOUNDARY.md](support/SUPPORT-BOUNDARY.md): public wording と support posture の正本
- [process/EXECUTION-MODEL.md](process/EXECUTION-MODEL.md): vNext delivery system の正本
- [process/implementation-bootstrap-sequence.md](process/implementation-bootstrap-sequence.md): 実装順序の正本
- [policies/VERSIONING.md](policies/VERSIONING.md): maturity label / artifact semver policy
- [policies/GOVERNANCE.md](policies/GOVERNANCE.md): change classification と freeze ルール
- [specs/diagnostic-ir-v1alpha-spec.md](specs/diagnostic-ir-v1alpha-spec.md): Diagnostic IR 契約
- [specs/gcc-adapter-ingestion-spec.md](specs/gcc-adapter-ingestion-spec.md): capture / ingest 契約
- [specs/rendering-ux-contract-spec.md](specs/rendering-ux-contract-spec.md): renderer 契約
- [specs/quality-corpus-test-gate-spec.md](specs/quality-corpus-test-gate-spec.md): quality gate 契約
- [specs/packaging-runtime-operations-spec.md](specs/packaging-runtime-operations-spec.md): packaging / install / release 契約
- [releases/](releases/): current artifact / release / signing 契約
- [runbooks/README.md](runbooks/README.md): current support runbooks
- [../adr-initial-set/README.md](../adr-initial-set/README.md): accepted ADR の索引

## Active Non-Authoritative Planning

現在、`docs/planning/` に active な planning authority は置かない。

- [planning/README.md](planning/README.md): reference-only planning workspace。現在は空
- 作業前提や設計判断は current authority docs と accepted ADR を優先する
- 新しい planning material が必要な場合でも `reference-only` として扱う

## Historical Material

- [history/README.md](history/README.md): historical architecture, legacy planning, superseded drafts の索引
- [archive/](archive/): archived bundle / tarball の保管場所

## Root Docs

- [../README.md](../README.md): プロダクト概要と読み順
- `CHANGELOG.md`: user-visible 変更履歴
- `CONTRIBUTING.md`: 変更手順と contributor 向けルール
- `SECURITY.md`: security reporting policy
- `SUPPORT.md`: maintainer / user のサポート導線
