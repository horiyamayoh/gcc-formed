# ADR-0020: Stability promises

- **Status**: Accepted
- **Date**: 2026-04-07

## Context

wrapper CLI、config、IR schema のどれが安定対象かが曖昧なまま実装を始めると、将来の supersede 条件も曖昧になる。spec-first repository として、何を stable contract と見なし、どの変更を ADR 対象にするかを先に固定する必要がある。

## Decision

- stability promise は少なくとも CLI surface、config / environment contract、IR schema semantics に対して定義する
- additive change と breaking change を区別し、意味変更は ADR で扱う
- v1alpha baseline の変更は仕様書への自由追記ではなく、ADR の追加または supersede で行う
- renderer wording や export behavior も user-visible contract として stability review の対象に含める

## Consequences

- 実装変更のレビュー観点が明確になる
- CLI / config / schema の偶発的 breakage を抑制できる
- まだ public SDK を凍結しない領域との線引きが必要になる

## Out of Scope

- すべての内部 module path の安定化
- experimental feature の永久互換保証
- future compiler adapter の詳細約束

## Supersedes/Related

- **Supersedes**: None
- **Related**: `ADR-0001`, `ADR-0009`, `ADR-0012`, `ADR-0019`

## Source Specs

- `../gcc-formed-architecture-proposal.md` の 19
- `../diagnostic-ir-v1alpha-spec.md` の enum / schema stability 関連節
- `../packaging-runtime-operations-spec.md` の version / install / promote 関連節
- `../rendering-ux-contract-spec.md` の canonical output / label stability 関連節
