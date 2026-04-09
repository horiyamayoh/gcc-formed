# ADR-0030: Theme/Layout separated from analysis/view model

- **Status**: Accepted
- **Date**: 2026-04-09

## Context

ユーザーの痛点は renderer の cosmetic issue ではなく、色が落ちる、長くなる、noise suppression が弱い、layout を変えると analysis wording まで巻き込まれそうだ、という構造的問題として表れている。これは analysis と view model と theme/layout が十分に分離されていない兆候である。

vNext では Path A/B/C が増えるため、表示差分を renderer 深部へ漏らさない境界が必要になる。

## Decision

- 表示層を少なくとも `facts`, `analysis`, `view model`, `theme/layout` に分離する
- `theme/layout` は color, indentation, headings, disclosure markers, compact/expanded presentation を扱う
- `analysis` は family, confidence, root ranking, first action など意味論のみを扱う
- `RenderViewModel` と `ThemePolicy` を first-class にする

## Consequences

- 表示書式変更のコストが下がる
- color / length / disclosure の回帰を isolated にテストできる
- path 差分を renderer 深部へ漏らしにくくなる

## Out of Scope

- palette や theme token の細部
- final renderer wording の完全固定
- TUI / GUI / editor-specific layout policy

## Supersedes/Related

- **Supersedes**: None
- **Related**: `ADR-0015`, `ADR-0019`, `ADR-0031`, `ADR-0032`

## Source Specs

- `../gcc-formed-vnext-change-design.md`
- `../rendering-ux-contract-spec.md`
- `../EXECUTION-MODEL.md`
- `../implementation-bootstrap-sequence.md`
