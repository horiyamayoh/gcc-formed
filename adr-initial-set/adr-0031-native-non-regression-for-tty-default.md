# ADR-0031: Native non-regression for TTY default

- **Status**: Accepted
- **Date**: 2026-04-09

## Context

ユーザーが最初に比較するのは native GCC と wrapper の default TTY experience である。そこで色が消える、初画面が長くなる、template/std:: ノイズが増える、fallback honesty が読みにくくなるなら、将来どれだけ理想形があっても採用されない。

v1alpha では render quality 自体は重視していたが、default TTY non-regression を stop-ship として十分に gate 化できていなかった。

## Decision

- default TTY では native GCC 非劣化を shipped contract に昇格する
- 少なくとも color handling, first-screen line budget, root cause / first action visibility, template/std:: noise compaction, raw disclosure honesty を MUST にする
- renderer 変更だけでなく capture/runtime 変更もこの non-regression contract の対象にする

## Consequences

- color regression と output inflation が release gate になる
- TTY-specific regression fixtures と budget assertions が必要になる
- path-aware quality gates が renderer 美観ではなく ship criteria になる

## Out of Scope

- all terminals / all color systems での完全同一表示
- verbose / CI profile の line budget 固定
- every family の one-shot perfect compaction

## Supersedes/Related

- **Supersedes**: None
- **Related**: `ADR-0019`, `ADR-0020`, `ADR-0030`

## Source Specs

- `../gcc-formed-vnext-change-design.md`
- `../SUPPORT-BOUNDARY.md`
- `../rendering-ux-contract-spec.md`
- `../quality-corpus-test-gate-spec.md`
