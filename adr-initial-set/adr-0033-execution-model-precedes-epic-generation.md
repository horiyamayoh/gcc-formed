# ADR-0033: Execution Model precedes Epic generation

- **Status**: Accepted
- **Date**: 2026-04-09

## Context

nightly agent 前提の開発では、Issue の切り方・レビューの仕方・停止条件が未整備なまま Epic を増やすと、実装速度だけが上がって wrong-direction PR と手戻りが増える。今回の vNext 変更は feature addition ではなく design axis replacement なので、delivery system を先に固定しないと backlog 自体が旧前提で量産される。

## Decision

- `EXECUTION-MODEL.md` を Epic より前に固定する
- Issue taxonomy, Project field vocabulary, `Agent Ready` definition, nightly extraction rule, morning review 4-way split, human-only boundary を先に決め、その後に Epic を生成する
- prompt は Issue から生成する派生物とし、自由文 prompt 運用を planning authority にしない

## Consequences

- 最初の 1〜2 週間は delivery system install が主作業になる
- nightly agent 運用の失敗原因を Issue design へ還元しやすくなる
- chat session が切れても GitHub 上の issue tree と handoff comment だけで再開しやすくなる

## Out of Scope

- Project automation rule の細部
- GitHub-specific UI 機能の恒久保証
- implementation detail を含む work package backlog の自動生成

## Supersedes/Related

- **Supersedes**: None
- **Related**: `ADR-0020`, `ADR-0026`, `ADR-0027`, `ADR-0031`

## Source Specs

- `../EXECUTION-MODEL.md`
- `../gcc-formed-vnext-change-design.md`
- `../implementation-bootstrap-sequence.md`
- `../docs/runbooks/agent-handoff.md`
