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

# ADR-0034: Presentation V2 subject-first blocks stay separate from the machine contract

- **Status**: Accepted
- **Date**: 2026-04-13

## Context

現行の renderer contract は `why:` と dedicated location line と lead-plus-summary session model を前提にしており、Presentation V2 で目指す Subject-first / multi-block / config-driven presentation semantics を十分に固定できていない。

この変更は単なる layout tweak ではない。次の境界を一度に扱う必要がある。

- terminal header grammar
- visible root を block として扱う session model
- display family と internal family の分離
- presentation config / preset の責務と fail-open
- public JSON を presentation customization から独立させること

これらを spec だけに埋め込むと、「どこまでが machine semantics でどこからが presentation policy か」が将来また曖昧になりやすい。

## Decision

- 人間向け terminal presentation は Subject-first block grammar を正本とする。
- canonical header grammar は `severity: [display-family] subject` を基本とし、interactive subject-first preset では inline location suffix を優先する。
- `1 visible root = 1 block` を Presentation V2 の基本 session model とする。cascade-hidden / dependent / duplicate / follow-on は block にしない。
- `lead_plus_summary` や capped summary は legacy compatibility, warning-only optimization, safety cap のために残してよいが、visible root の built-in default にはしない。
- `internal family` は analysis / rulepack / public JSON の machine semantics として維持する。
- `display family` は terminal presentation 専用の human-facing label とし、preset / config で解決してよい。
- `semantic_shape` は Presentation V2 の **view-model routing key** とし、family mapping から解決する。
- public JSON は presentation-independent な machine contract のまま維持し、display family, template id, location host decision を public field に昇格しない。
- presentation config / preset は non-fatal とし、壊れていても compile/link invocation 全体を止めず、built-in default または generic block へ fail-open する。
- rollout は `docs + ADR` 固定を先行し、その後 `opt-in preset`, 最後に `default promotion` の順で進める。

## semantic_shape

`semantic_shape` は、analysis 済み診断からどの semantic facts を構造化抽出するかを決める **view-model 層の routing 概念** である。これは `template_id` の別名ではなく、template より一段手前で意味的な抽出責務を固定するための語彙である。

- `internal family` は analysis が生成する machine-facing semantics である。
- `display family` は human-facing presentation label である。
- `semantic_shape` は view model が structural facts を取り出すための routing key である。
- `template` は抽出済み slot の順序・label・optional/required・suffix を決める theme/layout 側の表現である。

したがって `semantic_shape` は terminal presentation の内部契約ではあるが、public JSON や machine-readable export の public field ではない。

### ADR-0030 四層対応

ADR-0030 が規定する `facts / analysis / view model / theme-layout` の四層に対して、本 ADR の用語は次のように対応する。

| ADR-0030 の層 | 本 ADR の責務 |
|---|---|
| facts | `DiagnosticDocument`, raw diagnostics, normalized message/location/context facts |
| analysis | `internal family`, confidence, rulepack/analysis overlay |
| view model | `display family`, `semantic_shape`, semantic fact extraction, rendered card facts |
| theme-layout | `template`, label catalog, location host choice, block layout/emission |

`semantic_shape` は analysis 結果を消費するが、analysis 自体の machine semantics を変更しない。view model がどの slot 群を stable に取り出すかだけを決める。

### semantic_shape 一覧

Presentation V2 では次の 8 shape を正本とする。

| semantic_shape | 主な抽出 slot |
|---|---|
| `contrast` | `want`, `got`, `via` |
| `parser` | `want`, `near` |
| `lookup` | `name`, `use`, `need`, `from`, `near` |
| `missing_header` | `need`, `from` |
| `conflict` | `now`, `prev` |
| `context` | `from`, `via` |
| `linker` | `symbol`, `from`, `archive`, `now`, `prev` |
| `generic` | `raw` |

template はこの slot 群のどれをどう並べるかを決めるだけで、shape 自体を定義しない。

### Shape fallback

一部の family は診断内容に応じて複数 shape に適合しうる。`preprocessor_directive` はその代表例であり、通常は `parser` shape だが、見つからない `#include` では `missing_header` shape の方が適切である。

そのため family mapping は `semantic_shape` に加えて ordered `shape_fallback` を持ってよい。

- renderer は `shape_fallback` を先頭から評価する。
- fallback shape の抽出が成功した場合、その shape と対応する `display_family` を採用する。
- すべて失敗した場合に主 `semantic_shape` を使う。
- どの shape でも安定抽出できない場合は `generic` へ fail-open する。

これにより `template_id` への依存なしに、`preprocessor_directive` の parser/missing-header 切り替えを view-model routing として扱える。

## Consequences

- renderer 実装は analysis / view model / presentation policy / layout をより明確に分ける必要がある。
- semantic fact extraction は `template_id` ではなく `semantic_shape` を起点に実装されるべき、という制約が加わる。
- 複数 error session は「visible root block の反復」として理解できるようになる。
- CI first-line policy は interactive default と切り離して扱える。
- machine consumer は terminal text ではなく public JSON に依存すべき、という原則がさらに強化される。
- 既存の `why:` 中心の prose-heavy output は legacy compatibility / fallback / low-confidence honesty に限定される。

## Out of Scope

- Presentation V2 preset loader の実装完了
- built-in preset asset format の細部
- default promotion の即時実施
- public JSON schema change

## Supersedes/Related

- **Supersedes**: None
- **Related**: `ADR-0019`, `ADR-0020`, `ADR-0030`, `ADR-0031`

## Source Specs

- `../README.md`
- `../docs/specs/rendering-ux-contract-spec.md`
- `../docs/specs/public-machine-readable-diagnostic-surface-spec.md`
- `../docs/process/EXECUTION-MODEL.md`
