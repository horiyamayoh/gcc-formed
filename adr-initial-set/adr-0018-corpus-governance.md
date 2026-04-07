# ADR-0018: Corpus governance

- **Status**: Accepted
- **Date**: 2026-04-07

## Context

診断 UX の品質は impression ではなく corpus と gate で担保する必要がある。fixture の収集、sanitize、review、snapshot 更新が運用として固定されていないと、fidelity defect や mislead regression を止められない。

## Decision

- quality baseline は corpus-driven に管理する
- fixture 追加、sanitize、review、snapshot 更新のプロセスを governance として扱う
- curated corpus、harvested trace、shadow observation の接続ルールを持つ
- stop-ship criteria と rollout readiness は corpus 指標に結びつける

## Consequences

- UX 変更も test gate の対象になる
- corpus maintenance は継続的な運用コストになる
- trace harvesting と redaction policy が governance と結びつく

## Out of Scope

- ad hoc な手動確認だけでの品質承認
- seed corpus を持たない rollout
- snapshot without review

## Supersedes/Related

- **Supersedes**: None
- **Related**: `ADR-0004`, `ADR-0005`, `ADR-0016`, `ADR-0017`, `ADR-0019`

## Source Specs

- `../gcc-formed-architecture-proposal.md` の KPI、6.1.9、19
- `../quality-corpus-test-gate-spec.md` の全体
