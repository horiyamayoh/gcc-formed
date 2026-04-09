# ADR-0020: Stability promises

- **Status**: Accepted
- **Date**: 2026-04-07

## Context

wrapper CLI、config、IR schema のどれが安定対象かが曖昧なまま実装を始めると、将来の supersede 条件も曖昧になる。spec-first repository として、何を stable contract と見なし、どの変更を ADR 対象にするかを先に固定する必要がある。

public beta、RC gate、stable release automation、support runbooks まで main に入った現在は、stable 前でも reviewer が「どこから breaking か」「何が post-`1.0.0` backlog か」を一目で判定できる運用ルールが必要になった。

## Decision

- stability promise は少なくとも CLI surface、config / environment contract、IR schema semantics、renderer wording / confidence / fallback notices、release/install/rollback/signing contract、support boundary / runbook routing に対して定義する
- contract surface を触る change は `breaking` / `non-breaking` / `experimental` のいずれかに明示分類し、review は `../GOVERNANCE.md` の change matrix に従う
- `breaking` change は ADR の追加または supersede、migration / rollout impact の説明、関連 docs / changelog / release notes 更新を同時に要求する
- `non-breaking` change は additive か behavior-preserving の範囲に限り、既存 flag / field / notice / manifest / support promise の意味を silently 変えてはならない
- `experimental` change は opt-in かつ disabled-by-default とし、support boundary と release promise の外側に置く。stable path を silent に置き換えてはならない
- v1alpha / v1beta / stable-prep baseline の変更は仕様書への自由追記ではなく、ADR の追加または supersede で行う
- pre-`1.0.0` must-have backlog と post-`1.0.0` backlog は `../GOVERNANCE.md` に切り分け、post-`1.0.0` backlog を current support boundary に紛れ込ませない
- post-`1.0.0` の `breaking` change は次の major version か、明示的に versioned な replacement lane を要求する

## Consequences

- 実装変更のレビュー観点が明確になる
- reviewer が contract drift と roadmap drift を別々に検知できる
- CLI / config / schema / release contract の偶発的 breakage を抑制できる
- まだ public SDK を凍結しない領域との線引きが必要になる

## Out of Scope

- すべての内部 module path の安定化
- experimental feature の永久互換保証
- future compiler adapter の詳細約束
- pre/post-`1.0.0` backlog の個別優先順位そのもの

## Supersedes/Related

- **Supersedes**: None
- **Related**: `ADR-0001`, `ADR-0009`, `ADR-0012`, `ADR-0019`, `ADR-0021`, `ADR-0024`, `ADR-0025`

## Source Specs

- `../GOVERNANCE.md`
- `../gcc-formed-architecture-proposal.md` の 19
- `../diagnostic-ir-v1alpha-spec.md` の enum / schema stability 関連節
- `../packaging-runtime-operations-spec.md` の version / install / promote 関連節
- `../rendering-ux-contract-spec.md` の canonical output / label stability 関連節
