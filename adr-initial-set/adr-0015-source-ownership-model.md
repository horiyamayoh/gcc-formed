# ADR-0015: Source ownership model

- **Status**: Accepted
- **Date**: 2026-04-07

## Context

C/C++ diagnostics では system header や vendor code が前面に出やすいが、開発者が最初に直す対象は通常 user code である。root-cause ranking と renderer 両方で同じ ownership model を共有しないと、表示の優先順位がぶれる。

## Decision

- source location は `user` / `vendor` / `system` / `generated` の ownership model で扱う
- user code を最優先で surfacing し、system / vendor / generated は後段に圧縮する
- ownership 判定は IR / analysis / renderer で共有される product contract とする
- 判定不能な場合は conservative に扱い、誤分類を避ける

## Consequences

- root-cause ranking と renderer の優先順位が整う
- path classification policy を後から ad hoc に変えにくくなる
- build system や generated code の扱いには明示規則が必要になる

## Out of Scope

- 組織固有 path rule の無制限な埋め込み
- ownership を compiler vendor ごとに分岐すること
- editor 固有の workspace detection

## Supersedes/Related

- **Supersedes**: None
- **Related**: `ADR-0002`, `ADR-0019`

## Source Specs

- `../gcc-formed-architecture-proposal.md` の 2.5、19
- `../diagnostic-ir-v1alpha-spec.md` の ownership / location 関連節
- `../rendering-ux-contract-spec.md` の path ordering / ownership run compression 関連節
