---
doc_role: current-authority
lifecycle_status: accepted-baseline
audience: both
use_for: Current implementation contract for this surface.
do_not_use_for: Historical baseline or superseded path assumptions.
supersedes: []
superseded_by: []
---
> [!IMPORTANT]
> Authority: `current-authority` / `accepted-baseline`
> Use for: Current implementation contract for this surface.
> Do not use for: Historical baseline or superseded path assumptions.

# gcc-formed Rendering / UX Contract 仕様書

- **文書種別**: 内部仕様書（実装契約）
- **状態**: Accepted Baseline
- **版**: `1.0.0-alpha.1`
- **対象**: `gcc-formed` / 将来の `cc-formed`
- **主用途**: Diagnostic IR から terminal / CI 向け human-readable output を生成する renderer の契約固定
- **想定実装**: Linux first / GCC first / 品質最優先 / fail-open
- **関連文書**:
  - `../architecture/gcc-formed-vnext-change-design.md`
  - `diagnostic-ir-v1alpha-spec.md`
  - `gcc-adapter-ingestion-spec.md`
  - `../support/SUPPORT-BOUNDARY.md`
- **関連 ADR**:
  - `adr-initial-set/adr-0002-diagnostic-ir-as-product-core.md`
  - `adr-initial-set/adr-0006-fail-open-fallback-and-provenance.md`
  - `adr-initial-set/adr-0010-deterministic-rule-engine-no-ai-core.md`
  - `adr-initial-set/adr-0011-locale-policy-english-first-reduced-fallback.md`
  - `adr-initial-set/adr-0015-source-ownership-model.md`
  - `adr-initial-set/adr-0019-render-modes.md`
  - `adr-initial-set/adr-0020-stability-promises.md`
  - `adr-initial-set/adr-0030-theme-layout-separated-from-analysis-view-model.md`
  - `adr-initial-set/adr-0031-native-non-regression-for-tty-default.md`

---

## 1. この文書の目的

本仕様書は、`DiagnosticDocument` と `AnalysisOverlay` から、**人間が最短で修正行動に移れる表示**を生成するための renderer 契約を定義する。

本プロジェクトにおいて renderer は単なる「文字列整形器」ではない。renderer は次の 4 つを同時に満たす必要がある。

1. **root cause を最初に見せること**
2. **最初の修正行動を最初の画面内に出すこと**
3. **情報を圧縮しても compiler の事実を失わないこと**
4. **TTY / pipe / CI log のいずれでも劣化しにくいこと**

したがって本仕様の関心は、色や box drawing の美しさではなく、次にある。

- group / lead diagnostic の選択規則
- 情報圧縮と omission の規則
- source excerpt の再表示契約
- template / macro / include / linker の family ごとの表示規則
- low-confidence / partial / passthrough 時の表現制約
- TTY と CI の profile 差分
- raw diagnostics への安全な導線

本仕様は **実装コード** ではなく、**意味論・順序・ budget・ failure policy・ user-visible contract** を定義する。

---

## 2. 規範語

本仕様では以下の意味で規範語を使う。

- **MUST**: 必須
- **MUST NOT**: 禁止
- **SHOULD**: 強い推奨
- **SHOULD NOT**: 強い非推奨
- **MAY**: 任意

---

## 3. この仕様の位置づけ

rendering / UX 層は pipeline 上で以下に位置する。

```text
adapter / ingestion
    │
    ▼
DiagnosticDocument (facts)
    │
    ├── optional AnalysisOverlay
    ▼
render selection / grouping / budgeting
    ▼
view model synthesis
    ▼
layout / emission
    ▼
terminal text / CI log text / raw fallback text
```

この層の責務は、IR を利用して **「何を先に見せ、何を畳み、どこで raw に戻すか」** を決めることである。

本仕様は以下を扱う。

- terminal / CI 向け text rendering
- view profile と output capability に応じた劣化戦略
- deterministic な ordering と omission
- low-confidence / partial / passthrough の安全表示

本仕様が直接扱わないものは以下である。

- machine-readable export（SARIF / JSON / editor transport）
- IDE の widget / tree / hover UI
- public plugin API
- color theme の具体 palette
- 自動 apply-fix の実行
- 画面内インタラクション（paging, keyboard navigation, TUI）

---

## 4. この層が解くべき UX 上の本質課題

renderer が解くべき本質課題は、単に「見た目を良くする」ことではない。C/C++ 診断では、次が主要な痛点になる。

1. **最初に読むべき診断が埋もれる**
   - compile failure 時に warning / note / candidate / include chain が先に見えてしまう
2. **行動に移れる情報が遅い**
   - 何が悪いかは書いてあっても、どこを直せば良いかが後ろにある
3. **文脈が遠い**
   - 呼び出し地点と宣言地点、macro 展開元と定義元、template outer frame と failure point が分離している
4. **note の洪水が認知負荷を増やす**
   - overload candidate, template instantiation, include chain, linker repeated lines がそのまま並ぶ
5. **CI で可読性が崩れる**
   - 色なし、幅不明、スクロールのみ、grep / clickable path 依存という制約がある
6. **wrapper の推定が強すぎると危険**
   - 低信頼の要約が compiler facts を覆い隠すと誤誘導になる

したがって renderer は、以下の順で価値を出さなければならない。

1. 最初に着手すべき問題を前に出す
2. 修正着手点を前に出す
3. 証拠を少数に圧縮して示す
4. 深掘り可能な文脈を必要時だけ展開する
5. 常に raw compiler view に戻れる

---

## 5. 非目標

本仕様は以下を goal にしない。

1. rustc や GHC の見た目をそのまま模倣すること
2. compiler ごとの差を renderer 側だけで完全吸収すること
3. すべての診断情報を最初の view に出すこと
4. interactive な診断 explorer を v1alpha で実装すること
5. locale ごとの UI 文言翻訳を v1alpha で完成させること
6. 修正提案を自動適用すること
7. build 全体の multi-invocation 集約表示をこの文書だけで完成させること
8. vendor-specific CI 機能（fold marker など）を v1alpha で標準化すること

**決定事項**: v1alpha の renderer が自前で付加する UI ラベルは **英語固定** とする。ローカライズは post-MVP の別仕様とする。

理由:

- snapshot 安定性を上げる
- GCC の diagnostic 文脈と混在しても意味のズレを減らす
- UX quality を先に詰め、文言ローカライズは後段で分離する

---

## 6. 中核 UX 原則

本節は最重要である。実装はすべてこれに従う。

### 原則 1: one screenful, one action

default profile では、lead diagnostic について **最初の修正行動** が最初の screenful に入るべきである。

### 原則 2: root cause first

同一 invocation で複数診断があっても、**最も root cause に近く、かつ user code に近い** group を最初に展開する。

### 原則 3: facts before flourish

見た目の演出よりも、以下を優先する。

- 正しい primary location
- expected vs actual の関係
- declaration / use / expansion / instantiation の因果
- raw provenance の保持

### 原則 4: compress, never erase

圧縮はしてよいが、**省略したことを明示せずに消してはならない**。省略した場合は count と family を出す。

### 原則 5: user-owned evidence first

user code / workspace 内 location を system / vendor / generated より優先して見せる。

### 原則 6: low-confidence is visible

heuristic な要約や action hint は確信度に応じて表現を弱める。低信頼時に断定口調を使ってはならない。

### 原則 7: raw is always reachable

wrapper 独自の headline や summary を出しても、必要なら元の compiler message と raw diagnostics に辿れるべきである。

### 原則 8: default view is opinionated

default profile は「全部を平等に出す」設計にしてはならない。最初に直すべき問題 1 件を中心に据える。

### 原則 9: CI-safe by default

色・Unicode・terminal width に依存しすぎてはならない。CI では ASCII / deterministic / grep-friendly を優先する。

### 原則 10: same input, same view

同じ `DiagnosticDocument` と同じ capability / profile / policy からは、同じ text が出るべきである。

### 原則 11: no color-only meaning

色は補助にすぎない。重要な意味はラベル・位置・記号で表す。

### 原則 12: fallback beats false precision

IR や analysis が不十分なとき、曖昧な整理表示で誤誘導するより raw fallback を選ぶほうが良い。

---

## 7. 入出力契約

renderer は概念上、以下の入力と出力を持つ。

### 7.1 `RenderRequest`

| フィールド | 型 | 必須 | 意味 |
|---|---|---:|---|
| `document` | `DiagnosticDocument` | MUST | facts を持つ入力文書 |
| `cascade_policy` | `CascadePolicySnapshot` | MUST | resolved external cascade policy |
| `profile` | enum | MUST | `default | concise | verbose | ci | debug | raw_fallback` |
| `capabilities` | `RenderCapabilities` | MUST | 出力先の制約 |
| `path_policy` | enum | MAY | `shortest_unambiguous | relative_to_cwd | absolute` |
| `warning_visibility` | enum | MAY | `auto | show_all | suppress_all` |
| `selected_group_refs` | array<string> | MAY | 上位レイヤが特定 group のみ描画したい場合 |
| `expansion_policy` | map | MAY | template / macro / include / candidates などの展開量 |
| `debug_refs` | enum | MAY | `none | trace_id | capture_ref` |
| `type_display_policy` | enum | MAY | `full | compact_safe | raw_first` |
| `source_excerpt_policy` | enum | MAY | `auto | force_on | force_off` |
| `redaction_policy` | enum | MAY | `inherit | strict | off` |
| `line_budget_override` | integer | MAY | profile 既定値の上書き |

#### `RenderRequest` の規則

- `document` は validation 済みであることが望ましいが、renderer は未知 field や partial document でも壊れてはならない。
- `cascade_policy` は `CLI > user config > admin config > built-in defaults` で解決済みの値を受け取る。
- `document.document_analysis.episode_graph` が利用可能な場合、renderer は episode-first の selection / ordering を優先する。
- `profile = raw_fallback` は wrapper 独自 UX を最小化し、raw diagnostics を主とする表示を意味する。
- `selected_group_refs` が無い場合、renderer 自身が group selection を行う。

### 7.2 `RenderCapabilities`

| フィールド | 型 | 必須 | 意味 |
|---|---|---:|---|
| `stream_kind` | enum | MUST | `tty | pipe | file | ci_log` |
| `width_columns` | integer or `unknown` | MUST | 利用可能幅。未知なら `unknown` |
| `ansi_color` | bool | MUST | ANSI color 利用可否 |
| `unicode` | bool | MUST | Unicode を使ってよいか |
| `hyperlinks` | bool | MUST | file hyperlink を埋めてよいか |
| `interactive` | bool | MUST | 対話端末か |

#### `RenderCapabilities` の規則

- `unicode = true` であっても、v1alpha の canonical output は ASCII safe であるべきである。
- `hyperlinks = true` でも、link は補助であり、path text 自体を省略してはならない。
- `width_columns = unknown` の場合、renderer は profile 既定幅を使う。
- native compiler color の保全は capture/runtime の契約であり、renderer は ANSI color の有無に依存して意味を運んではならない。

### 7.3 `RenderResult`

| フィールド | 型 | 必須 | 意味 |
|---|---|---:|---|
| `text` | string | MUST | user-visible output |
| `used_analysis` | bool | MUST | analysis overlay を実表示に使ったか |
| `used_fallback` | bool | MUST | raw fallback が発動したか |
| `fallback_reason` | enum | SHOULD | `used_fallback = true` のとき reason-coded taxonomy を保持する。非 fallback では省略可 |
| `displayed_group_refs` | array<string> | MAY | 展開表示した group |
| `suppressed_group_count` | integer | MAY | summary-only または count-only に圧縮した group 数 |
| `suppressed_warning_count` | integer | MAY | 表示抑制した warning 数 |
| `truncation_occurred` | bool | MAY | budget により省略が起きたか |
| `render_issues` | array<`IntegrityIssue`> | MAY | render stage の問題 |

`RenderResult` は主に test harness / telemetry / debug 用であり、user-visible contract の主役は `text` である。

#### `fallback_reason` の規則

- `used_fallback = true` のとき、renderer は `fallback_reason` を埋めるべきである。
- `fallback_reason` は free-form string ではなく、reason-coded taxonomy を使うべきである。
- 少なくとも `unsupported_tier`, `incompatible_sink`, `shadow_mode`, `sarif_missing`, `sarif_parse_failed`, `residual_only`, `renderer_low_confidence`, `internal_error`, `timeout_or_budget`, `user_opt_out` を表現可能であるべきである。
- trace / replay / snapshot report はこの taxonomy をそのまま再利用してよい。

---

## 8. profile と budget

renderer は profile ごとに **情報量の既定値** を持つ。ここでの budget は「行数を必ず一致させる」という意味ではなく、「この量を超える場合は省略 / 要約へ切り替える」という契約である。

### 8.1 profile 一覧

| profile | 主用途 | expanded groups (default) | target lines / lead group | hard max lines / lead group | source excerpts | template frames | macro/include frames | candidate notes | warnings on failure |
|---|---|---:|---:|---:|---:|---:|---:|---:|---|
| `default` | local TTY | 1 | 18 | 28 | 2 | 5 | 4 | 3 | summarize |
| `concise` | pipe / quick scan | 1 | 10 | 14 | 1 | 3 | 2 | 2 | suppress |
| `verbose` | investigation | all | 40 | 80 | 6 | 20 | 12 | 10 | show |
| `ci` | CI log | 1 | 12 | 16 | 1 | 3 | 2 | 2 | summarize |
| `debug` | internal triage | all | 60 | 120 | 8 | 30 | 20 | 20 | show |
| `raw_fallback` | safe escape hatch | 0 | n/a | n/a | 0 | 0 | 0 | 0 | raw-first |

### 8.2 profile 自動選択

profile が明示されない場合、既定は以下とする。

1. `stream_kind = ci_log` -> `ci`
2. `stream_kind = tty` かつ `interactive = true` -> `default`
3. それ以外 -> `concise`

### 8.3 width 既定値

| 条件 | 既定幅 |
|---|---:|
| `tty` で幅取得成功 | 実幅を使用 |
| `tty` で幅不明 | 100 |
| `pipe` / `file` / `ci_log` | 100 |

#### width の規則

- 幅が 60 未満なら multi-column 的な整形を避け、single-column 寄りに degrade する。
- 幅が極端に広くても、excerpt 周辺以外は 160 列相当を超えて利用しないことが望ましい。

### 8.4 warning 表示既定

- compile/link が **失敗** した場合、`default` / `concise` / `ci` は warning を主 view から外して summary count のみにしてよい。
- compile/link が **成功** した場合、warning は通常の diagnostic として扱う。
- `verbose` / `debug` は失敗時でも warning を落とさない。

### 8.5 expanded group 数の規則

- `default` / `concise` / `ci` は lead group を 1 件だけ fully expanded にしてよい。
- `document.document_analysis.episode_graph` がある場合、独立 root group は visibility を失ってはならない。resolved `cascade_policy.max_expanded_independent_roots` を超えた overflow root は invisible ではなく summary-only に落とす。
- 2 件目以降の visible root は summary line のみでもよいが、hidden にしてはならない。
- warning-only run では `default` は最大 2 groups まで expanded にしてよい。

---

## 9. レンダリングパイプライン

renderer は概念上、以下の段階で処理する。

```text
[1] input validation
      │
      ▼
[2] group construction
      │
      ▼
[3] lead selection + ordering
      │
      ▼
[4] section budgeting
      │
      ▼
[5] view model synthesis
      │
      ▼
[6] layout + emission
      │
      ├── normal render
      └── raw fallback / mixed fallback
```

### 9.1 stage 1: input validation

renderer は少なくとも以下を判定する。

- `document_completeness`
- root diagnostics の有無
- `analysis` の有無と confidence
- source snippet を表示できる location の有無
- `passthrough` / `partial` node の比率

ここで renderer 自身の failure が予見できる場合は、早めに `raw_fallback` へ落としてよい。`document.document_analysis.episode_graph` / `group_analysis` が欠落・矛盾・不完全で episode-first selection を安全に構成できない場合は、renderer は relation を補完せず legacy selection に fail-open しなければならない。

### 9.2 stage 2: group construction

group は以下の順で決める。

1. `document.document_analysis.episode_graph` が usable な場合、episode root を first-class group として構成する
2. `analysis.group_ref` がある場合、それで group 化する
3. 無い場合、top-level root ごとに独立 group とする
4. `passthrough` node は独立 group として扱う

adapter が root を merge しない原則は維持する。renderer も、根拠のない独自 merge や relation heuristic の再推定を行ってはならない。episode-first 情報が不十分なときは group 分解を諦め、legacy selection に fail-open してよい。

### 9.3 stage 3: lead selection

各 group には 1 件の **lead node** を選ぶ。lead node は expanded view の中心になる。

lead node の優先順は以下とする。

1. `analysis.root_cause_score` が高い
2. `analysis.user_code_priority` が高い
3. `severity` が高い（`fatal` > `error` > `warning` > `note` > `info`）
4. `analysis.actionability_score` が高い
5. user-owned primary location を持つ
6. original order が早い

episode graph がある場合、lead selection は independent root を優先し、follow-on / duplicate / dependent member は lead 候補に持ち上げない。これらは同一 episode の note として折り畳むか、visible lead がない場合のみ hidden / summary-only に落とす。

### 9.4 stage 4: section budgeting

budget は group ごとに配分する。budget 消費の優先順位は以下。

1. title / severity
2. canonical location
3. first action
4. primary excerpt
5. expected vs actual / declaration evidence
6. family-specific context summary
7. suggestions
8. collapsed notices
9. raw / debug footer

budget を超える場合、下位優先項目から summary 化または omission する。

### 9.5 stage 5: view model synthesis

layout 前に一度 **view model** を作るべきである。これにより renderer のテストを、最終 text snapshot だけでなく section 単位でも行える。

### 9.6 stage 6: emission

emission は profile と capability に応じて行う。

- `default`: human-friendly だが ASCII safe
- `concise`: line economy 優先
- `ci`: grep / clickable path / deterministic 優先
- `verbose`: omission 少なめ
- `raw_fallback`: wrapper prose を最小化

---

## 10. View Model 契約

実装は自由だが、少なくとも以下に相当する view model を内部で持つことを強く推奨する。

### 10.1 `RenderSessionSummary`

| フィールド | 型 | 意味 |
|---|---|---|
| `exit_status` | integer | child exit status |
| `failure_kind` | enum | `compile_failure | link_failure | warnings_only | passthrough | wrapper_failure | unknown` |
| `expanded_group_count` | integer | fully expanded group 数 |
| `suppressed_group_count` | integer | summary-only または count-only に圧縮した group 数 |
| `suppressed_warning_count` | integer | 抑制 warning 数 |
| `partial_notice` | bool | partial / passthrough notice を出すか |

### 10.2 `RenderGroupCard`

| フィールド | 型 | 意味 |
|---|---|---|
| `group_ref` | string | 論理 group ID |
| `lead_node_id` | string | 中心 node |
| `severity_label` | string | `error`, `warning`, `note` など |
| `title` | string | user が最初に読む headline |
| `canonical_location` | string | 最も見せるべき location |
| `confidence_label` | enum | `certain | likely | possible | hidden` |
| `first_action` | string or null | 最初に試す修正行動 |
| `excerpts` | array<`RenderExcerptBlock`> | source 表示 |
| `evidence_lines` | array<string> | because / note / declaration summary |
| `context_sections` | array<`RenderContextSection`> | template / macro / include / linker / path |
| `suggestions` | array<`RenderActionItem`> | fix-it / hint |
| `collapsed_notices` | array<string> | omitted counts |
| `footer_refs` | array<string> | raw / trace / debug ref |

`collapsed_notices` には follow-on, duplicate, related diagnostic など、同一 episode 内で lead に吸収された子 diagnostic の省略理由を含めてよい。

### 10.3 `RenderExcerptBlock`

| フィールド | 型 | 意味 |
|---|---|---|
| `location_header` | string | `--> path:line:col` 相当 |
| `source_lines` | array<string> | snippet 本体 |
| `annotation_lines` | array<string> | caret / range / label |
| `role` | enum | `primary | secondary | declaration | definition | reference | expansion | instantiation` |

### 10.4 `RenderContextSection`

| フィールド | 型 | 意味 |
|---|---|---|
| `kind` | enum | `template | macro | include | linker | path | notes | candidates | other` |
| `header` | string | section 見出し |
| `lines` | array<string> | 要約行 |
| `collapsed_count` | integer | 省略した frame / item 数 |

### 10.5 `RenderActionItem`

| フィールド | 型 | 意味 |
|---|---|---|
| `label` | string | `suggested edit`, `likely edit`, `consider`, `possible fix` など |
| `text` | string | action 説明 |
| `inline_patch` | array<string> | 小さな差分を inline 表示する場合 |
| `applicability` | enum | IR の suggestion applicability |

---

## 11. group 化と ordering 規則

### 11.1 document 全体の ordering

document に error/fatal を含む場合、表示順は以下とする。

1. failure を引き起こした error/fatal groups
2. error 由来の supporting groups
3. warning groups
4. note-only / info-only / passthrough groups

同順位内では以下で並べる。

1. lead `root_cause_score` 降順
2. lead `user_code_priority` 降順
3. lead `actionability_score` 降順
4. original group order 昇順

### 11.2 lead group の規則

`default` / `concise` / `ci` では、最上位 group 1 件のみを fully expanded にしてよい。

ただし以下の場合、2 件目も expanded にしてよい。

- 1 件目が low-confidence である
- 1 件目が passthrough summary しか持てない
- 2 件目が別ファイルの declaration/definition conflict として強く関連する

`document.document_analysis.episode_graph` がある場合、独立 root group は全件 visible に保たなければならない。expanded budget を超えた root は summary-only にし、invisible にしてはならない。follow-on / duplicate / dependent group は独立 root として数えず、visible lead の collapsed notice か hidden count に吸収してよい。

`default` / `concise` / `ci` は resolved `cascade_policy.compression_level` と threshold 群を使って dependent group の hidden / summary-only / collapsed-notice 境界を決める。`off` は hidden suppression を無効にし、dependent group を少なくとも summary-only には残す。`verbose` / `debug` は dependent group を hidden にせず、少なくとも summary-only として見えるようにする。

### 11.3 unrelated warning の扱い

失敗 run に warning が混在する場合:

- `default`: `N warnings hidden while showing the primary failure` のように数だけ示してよい
- `concise`: warning 完全非表示でもよい
- `ci`: summary 1 行のみ許可
- `verbose` / `debug`: warning を後段に出す

### 11.4 summary-only group の形式

summary-only group は 1〜2 行に収める。summary-only は「表示しない」ことを意味せず、独立 root については必ず少なくとも 1 行の存在表示を維持する。

例:

```text
other errors:
  - src/b.cc:87: error: no matching function for call to 'push_back'
  - include/foo.hpp:41: warning: comparison of signed and unsigned values
```

hidden suppression が発生した場合、suppressed count line の表示は resolved `cascade_policy.show_suppressed_count` に従う。

- `always`: hidden count line を出す
- `never`: hidden count line を出さない
- `auto`: `default` / `concise` / `ci` では 1 行の hidden count line を出してよい。`verbose` / `debug` では suppressed member 自体が見えていることを優先してよい

### 11.5 AnalysisOverlay の利用規則

renderer は `analysis` を使ってよいが、facts を上書きしてはならない。以下を既定契約とする。

1. `analysis.preferred_primary_location_id` が妥当な `Location` を指す場合、canonical location 選択に優先的に使ってよい
2. `analysis.first_action_hint` は confidence が `>= 0.60` のとき first action 候補として使ってよい
3. `analysis.collapsed_child_ids` / `collapsed_chain_ids` は **hint** であり、renderer は profile budget や user policy に応じて無視してよい
4. `analysis.family` は family-specific formatter 選択の入力に使ってよい
5. `analysis.reasons[]` は verbose/debug で evidence line に降ろしてよいが、internal heuristic 名の生出しは禁止
6. `analysis.group_ref` は grouping hint として使ってよいが、raw provenance を失う merge を生んではならない
7. `document.document_analysis.episode_graph` / `group_analysis` がある場合、renderer はそこに書かれた episode / role の関係をそのまま使い、relation heuristic を再推定してはならない

### 11.6 canonical location の選択規則

canonical location は以下の優先順で選ぶ。

1. `analysis.preferred_primary_location_id` が指す valid location
2. lead node の primary role location
3. user-owned secondary location のうち最も actionability が高いもの
4. first available location
5. location 無しの場合は symbol/object context

**MUST NOT**: system header 内の深い location を、より有用な user-owned call siteがあるのに先頭へ出すこと。

---

## 12. title / headline / confidence 規則

### 12.1 title の優先順位

`RenderGroupCard.title` は以下の順で決める。

1. `analysis.headline`（confidence が十分高い場合）
2. lead node の `message.raw_text`
3. family-specific synthesized title
4. `unclassified compiler diagnostic`

### 12.2 confidence threshold

`analysis` の確信度に応じた扱いを以下で固定する。

| confidence | renderer の扱い |
|---:|---|
| `>= 0.85` | headline を無修飾で使用してよい |
| `0.60 - 0.84` | `likely:` 相当の弱い qualifier を付けてよい |
| `0.35 - 0.59` | raw message を主 title にし、analysis は `why` または `help` に回す |
| `< 0.35` | analysis headline / action hint を user-visible 主経路に使ってはならない |

### 12.3 断定表現の制約

- confidence が低い場合、`fix this by ...` のような断定は禁止。
- 低信頼 suggestion は `consider`, `possible`, `likely` といった弱い表現を使う。

### 12.4 raw message 保持

lead node の raw compiler message が analysis headline と実質同値でない場合、以下のいずれかを満たすべきである。

1. verbose/debug で raw message を明示表示する
2. footer / secondary line から raw message に辿れる

raw message を完全不可視にしてはならない。

---

## 13. canonical layout 契約

### 13.1 group card の section 順序

expanded group の section 順は以下で固定する。

1. severity + title
2. canonical location
3. first action（あれば）
4. primary excerpt
5. expected vs actual / declaration evidence
6. family-specific context summary
7. suggestions / fix-it
8. collapsed notices
9. raw / debug footer

### 13.2 v1alpha の標準ラベル

v1alpha で renderer が自前で出すラベルは、意味を安定させるため以下の英語語彙を既定とする。

| 用途 | ラベル |
|---|---|
| severity | `error`, `warning`, `note`, `info` |
| action | `help` |
| causal summary | `why`, `because` |
| chain intro | `while`, `from`, `through` |
| linker section | `linker` |
| raw fallback | `raw` |
| omission | `omitted` |

### 13.3 first action の位置

`first_action` がある場合、**primary excerpt より前** または **直後** に置かなければならない。候補が複数ある場合、最も局所的で低コストなものを選ぶ。

### 13.4 title line の最小要件

title line には少なくとも以下を含む。

- severity
- headline または raw message

`ci` profile で location がある場合、first line は次形を推奨する。

```text
path:line:column: error: <headline>
```

これにより grep / clickable path と両立する。

### 13.5 canonical location の最小要件

location がある場合、expanded group は location を title 付近に必ず出す。`default` / `verbose` では `--> path:line:col` 形式、`ci` / `concise` では path-first line を優先してよい。

---

## 14. source excerpt / span rendering 規則

### 14.1 基本方針

excerpt は「きれいに見せる」ためではなく、**いま直すべき source point を確定する**ためにある。

### 14.2 excerpt 表示条件

以下を満たす場合、renderer は source excerpt を出すべきである。

- file path がある
- line 情報がある
- source を安全に読める
- excerpt が user action に寄与する

以下の場合、location line のみでもよい。

- source file が取得できない
- linker/object ベースで source span が弱い
- very large generated file で excerpt がノイズ化する

### 14.3 excerpt 数

- `default`: 2 つまで
- `concise` / `ci`: 1 つまで
- `verbose`: 6 つまで
- `debug`: 8 つまで

複数 file が絡む場合、primary excerpt を最優先し、2 つ目は declaration/definition や contrasted location に使う。

### 14.4 point / range 表示

- point のみ分かる場合は `^`
- single-line range は `^~~~`
- label を付ける場合、annotation line 末尾に短い説明を置いてよい
- multi-line range は full 展開せず、first/last line と `range spans N lines` の summary を許可する

### 14.5 同一行に複数 range がある場合

- 最大 2 つまでは inline annotation を許可する
- 3 つ以上は numbered marker または bullet legend に落とす
- 読みやすさが壊れるなら 1 つを primary とし、残りを evidence line に移す

### 14.6 長い行の切り詰め

長い source line は highlight 周辺の window を切り出してよい。規則は以下。

- 左右に `...` を付ける
- highlight 範囲は必ず window 内に残す
- window 外を切り落としたことを暗黙にせず、`...` を明示する

### 14.7 列の意味

IR に display column がある場合はそれを優先する。無い場合は native / byte column から安全に degrade する。

**MUST NOT**: tab や multibyte によって caret が明らかにズレる実装を容認すること。

### 14.8 ownership の表示

location の ownership が `system` / `vendor` / `generated` の場合、必要なら header や evidence line に軽い label を出してよい。

例:

```text
--> [system] /usr/include/c++/15/vector:412:9
```

ただし user-owned の excerpt を押しのけて前に出してはならない。

---

## 15. evidence / because セクション規則

### 15.1 何を evidence とみなすか

evidence には以下を含みうる。

- expected type / actual type
- declaration site
- prior definition site
- candidate mismatch 要約
- ownership 境界の説明
- `analysis.reasons[]` 由来の要約（高信頼時のみ）

### 15.2 evidence の順序

1. expected vs actual
2. declaration / definition / previous use
3. family-specific short explanation
4. remaining notes

### 15.3 line 予算

- `default`: 4 行程度
- `concise` / `ci`: 2 行程度
- `verbose`: 10 行程度

長い note 群は bullets ではなく summary section に降ろす。

### 15.4 family-specific causal wording

renderer は family ごとに短い causal wording を使ってよい。

例:

- `why: this call passes 'Foo *' where 'Bar *' is required`
- `because: the selected overload expects 2 arguments`
- `because: this macro expands to an expression with type 'int'`

ただし compiler facts と矛盾してはならない。

---

## 16. suggestion / fix-it rendering 規則

### 16.1 applicability ごとの既定ラベル

| `Suggestion.applicability` | 既定ラベル |
|---|---|
| `machine_exact` | `suggested edit` |
| `machine_probable` | `likely edit` |
| `manual` | `consider` |
| `unsafe` | `possible but risky` |
| `unknown` | `possible fix` |

### 16.2 suggestion の並び順

1. `machine_exact`
2. `machine_probable`
3. `manual`
4. `unsafe`
5. `unknown`

同順位では以下を優先する。

1. user-owned file に触るもの
2. single-file edit
3. edit 数が少ないもの
4. original order

### 16.3 inline patch を出す条件

inline patch は以下をすべて満たす場合にのみ `default` / `ci` で出してよい。

- 1 file のみ
- 3 edit 以内
- 合計変更文字数が 80 以下
- 変更行数が 3 以下
- applicability が `machine_exact` または `machine_probable`

それ以外は summary line のみに留め、詳細は `verbose` / debug trace に委ねる。

### 16.4 multiple suggestions

複数 suggestion が **相互排他的** と思われる場合、以下のように group 化してよい。

```text
possible fixes (choose one):
  - add an explicit cast
  - change the callee parameter type
```

### 16.5 first action との関係

`analysis.first_action_hint` が無くても、high-confidence な `machine_exact` fix-it がある場合、renderer は中立的な first action を組み立ててよい。

例:

```text
help: apply the suggested edit below
```

### 16.6 renderer がやってはいけないこと

- 複数 file にまたがる patch を default で長く展開すること
- applicability が低い suggestion を断定口調で出すこと
- suggestion を facts より前面に出しすぎること

---

## 17. family-specific rendering 規則

### 17.1 syntax / parse error

syntax family の default 表示は以下を優先する。

1. failure point の primary excerpt
2. parser が期待していた token / construct の 1 行 summary
3. 直前の likely local cause（不足した `;`, `)`, `}` など）

**MUST NOT**: parser recovery が生んだ cascade errors をすべて expanded にすること。

### 17.2 type mismatch / conversion / overload

この family では、**expected vs actual** を最も短い文で前に出す。

推奨構造:

1. call / assignment site excerpt
2. `why:` expected vs actual
3. declaration site または selected overload
4. remaining candidate notes の summary

### 17.3 overload candidate flood

candidate note が多い場合:

- `default`: 最大 3 件まで表示
- `concise` / `ci`: 最大 2 件まで
- `verbose`: 最大 10 件まで

残りは以下のように要約する。

```text
omitted 9 other candidates with similar mismatches
```

candidate の並び順は以下。

1. user-owned declaration
2. exact arity match だが type mismatch のもの
3. arity mismatch のもの
4. system / vendor 由来

### 17.4 template instantiation

template family は最もノイズ化しやすいため、以下を固定する。

#### default / ci / concise

- failing point を 1 件表示
- first user-owned frame を 1 件表示
- outermost user-owned caller を 1 件表示
- 残りは `omitted N internal template frames` で畳む

#### verbose / debug

- 全 frame を順序通り表示してよい
- ただし consecutive internal frames は group 化してもよい

#### template frame の説明語彙

- `while instantiating:`
- `required from:`
- `through:`

### 17.5 macro expansion

macro family では、次の 2 点を最小セットとする。

1. invocation site
2. macro definition site または直近 expansion source

nested macro が深い場合:

- `default`: invocation + terminal definition を見せ、中間は summary
- `verbose`: 全段または大半を見せてよい

### 17.6 include chain

include chain は full 展開すると読みづらい。default では以下を見せる。

1. failure が起きた header
2. その header へ到達する直近 user-owned include edge
3. root translation unit

中間は `through 6 intermediate includes` のように summary 化する。

### 17.7 analyzer / path diagnostics

control-flow path がある場合、default では全 event を出さず、以下の key event を優先する。

1. source of value/resource
2. state transition
3. failing use site

残りは `omitted N path events` とする。

### 17.8 linker diagnostics

linker family は source span より symbol / object / archive が主役になる。default では以下を前に出す。

1. primary symbol（demangled 名があればそれを優先）
2. role（undefined reference, multiple definition など）
3. 最も役立つ object/archive context
4. source 由来 reference site があれば 1 件まで

#### undefined reference

- 同じ symbol の repeated line は 1 group に畳む
- `default` では symbol 1 件につき 1 つの card にしてよい
- `binary_inputs` は最多 3 件まで表示

#### multiple definition / duplicate symbol

- first conflicting definition 2 件を表示
- それ以外は count summary

#### unmangled / mangled

- demangled 名があれば title に使ってよい
- raw mangled 名は verbose/debug または footer で辿れるようにする

### 17.9 passthrough family

`semantic_role = passthrough` や unclassified residual が主体の group では、wrapper は minimal summary のみ付与して raw block を主表示にしてよい。

---

## 18. note / context の圧縮規則

### 18.1 duplicate note

同一 location・同一 message・同一 role の note は dedup してよい。ただし dedup したことを omission notice に反映させることが望ましい。

### 18.2 ownership run compression

consecutive な `system` / `vendor` / `generated` frame 群は 1 つの collapsed summary にできる。

例:

```text
omitted 8 internal frames under /usr/include/c++/15
```

### 18.3 type name shortening

C++ 型名は爆発しやすいため、`type_display_policy = compact_safe` では安全な短縮を認める。

#### compact_safe の条件

- 最終識別子は残す
- pointer/reference/cv qualifier は残す
- namespace は末尾 1〜2 segment を残し、それ以前は `...::` で畳める
- template argument は先頭 2 個程度まで見せ、残りを `...` にできる
- 2 つの型が短縮後に同一視される場合は短縮を弱める

#### MUST NOT

- 型差分の本質を隠す短縮
- signed / unsigned, reference / value, const / non-const の差を消すこと

### 18.4 repeated linker lines

同じ symbol / same role / same object 由来の repeated raw line は group 化してよい。

### 18.5 omission notice の規則

omission notice は単なる `...` ではなく、**何が何件省略されたか** を言うべきである。

例:

- `omitted 7 template frames`
- `omitted 12 overload candidates`
- `omitted 4 linker references to the same symbol`

---

## 19. path / file / hyperlink 規則

### 19.1 path 表示方針

既定の `path_policy` は `shortest_unambiguous` とする。

優先順:

1. `cwd` からの相対 path で一意ならそれ
2. 相対で曖昧なら必要分だけ prefix を増やす
3. それでも曖昧なら absolute path

### 19.2 spaces と quoting

path に space があっても parser-friendly であるよう、path 全体を壊さず出す。CI first line では一般的な `path:line:col:` 形式を維持する。

### 19.3 hyperlinks

`hyperlinks = true` の場合、location line に file hyperlink を埋めてもよい。ただし以下を守る。

- path text 自体は必ず表示する
- hyperlink が壊れても意味が残る
- `ci_log` では既定で無効でよい

### 19.4 internal path leak 防止

trace bundle の temp path や capture path を user-visible output にそのまま出してはならない。`debug_refs = capture_ref` が明示された場合のみ許可する。

---

## 20. CI / log contract

CI は local TTY より制約が強い。したがって `ci` profile は別契約とする。

### 20.1 CI first line

location がある group の first line は以下を推奨形とする。

```text
path:line:column: error: <headline>
```

location が弱い linker/passthrough group では以下を許可する。

```text
linker: error: <headline>
```

### 20.2 CI の禁止事項

`ci` profile では以下を **MUST NOT**。

- cursor movement を前提とする escape
- color だけで意味を表すこと
- terminal width に強く依存する複雑な左右配置
- 実行ホスト固有の temp path 露出

### 20.3 CI の行数 budget

lead group は原則 16 行以内に抑える。これを超える場合、family-specific context を summary line に落とす。

### 20.4 CI と raw fallback

fallback 時でも、冒頭 1〜2 行で wrapper が何をしたかを簡潔に示してよい。

例:

```text
error: showing original compiler diagnostics because structured rendering is incomplete
--- raw compiler diagnostics ---
```

---

## 21. low-confidence / partial / fallback 規則

### 21.1 `document_completeness` ごとの既定動作

| completeness | 既定動作 |
|---|---|
| `complete` | 通常 render |
| `partial` | conservative render + partial notice |
| `passthrough` | raw-first render |
| `failed` | wrapper failure message + raw fallback（可能なら） |

### 21.2 partial notice

`document_completeness = partial` の場合、session または lead group 近傍に 1 回だけ notice を出してよい。

例:

```text
note: some compiler details were not fully structured; original diagnostics are preserved
```

### 21.3 mixed fallback

1 つの group 内に partial/passthrough node が多い場合、renderer は **mixed mode** を使ってよい。

構成:

1. wrapper headline
2. minimal excerpt / summary
3. raw sub-block

### 21.4 raw_fallback の条件

以下のいずれかで `raw_fallback` を許可する。

- render stage 自身が failure した
- lead group の必要 facts が欠けすぎている
- confidence が極端に低く、wrapper 表示が誤誘導になりうる
- source excerpt や key location をまったく構成できない

### 21.5 raw_fallback でも守るべきこと

- raw diagnostics は順序を保つ
- wrapper prose は短くする
- 原因不明の美化をしない
- 可能なら trace id だけを出す

---

## 22. accessibility / compatibility 規則

### 22.1 ASCII safe

v1alpha の canonical output は ASCII safe を基本とする。Unicode が使える場合でも、ASCII で意味が保てる記法を優先する。

### 22.2 色の役割

色は以下の補助に限定することが望ましい。

- severity の視認性向上
- primary excerpt と secondary context の強弱
- suggestion の識別

ただし色無しでも意味が完全に通る必要がある。

### 22.3 screen reader / copy-paste

- section の順序は linearly understandable であるべき
- copy-paste して issue tracker / chat に貼っても壊れにくいこと
- box drawing 依存や列位置依存が強すぎる表現は避ける

### 22.4 source 不可読時の動作

source file が読めない場合、renderer は location と short evidence のみで成立しなければならない。

---

## 23. policy surface（設定可能項目）

CLI flag や config file への写像は別仕様でもよいが、renderer が受け入れるべき policy 概念は少なくとも以下を持つべきである。

| policy | 既定値 | 意味 |
|---|---|---|
| `profile` | auto | view profile |
| `warning_visibility` | auto | warning 抑制 |
| `path_policy` | shortest_unambiguous | path 表示戦略 |
| `type_display_policy` | compact_safe | 型名短縮 |
| `max_expanded_groups` | profile dependent | fully expanded group 数 |
| `max_template_frames` | profile dependent | template 表示数 |
| `max_macro_frames` | profile dependent | macro 表示数 |
| `max_include_frames` | profile dependent | include 表示数 |
| `max_candidate_notes` | profile dependent | candidate 表示数 |
| `show_raw_message` | auto | raw compiler message を見せる閾値 |
| `debug_refs` | none | trace/capture ref の露出 |
| `color` | auto | color 使用 |
| `hyperlinks` | auto | hyperlink 使用 |

### 23.1 policy precedence

1. explicit CLI / API request
2. project config
3. environment-derived defaults
4. renderer built-in defaults

---

## 24. 受け入れ基準（renderer 単体）

本仕様の実装が受け入れ可能であるための最低条件を定める。

### 24.1 local default profile

1. lead group の severity, location, first action が最初の 8 非空行以内に入ること（存在する場合）
2. compile failure 時、default profile の lead group が原則 28 行以内に収まること
3. warning が failure より前に expanded 表示されないこと
4. partial / fallback 時に raw への導線が残ること

### 24.2 CI profile

1. first line が path-first または linker-first で grep しやすいこと
2. ANSI 無しで十分に読めること
3. lead group が原則 16 行以内に収まること
4. temp path を漏らさないこと

### 24.3 family-specific

1. template flood fixture で default が 5 visible frame 以内に圧縮されること
2. overload candidate flood fixture で default が 3 visible candidate 以内に圧縮されること
3. linker repeated undefined reference fixture で symbol 単位に group 化されること
4. macro/include chain で invocation/definition と root include edge が可視になること

### 24.4 fidelity

1. low-confidence analysis が raw facts を覆い隠さないこと
2. unknown enum / unknown extension でも renderer が壊れないこと
3. same input / same profile / same capability で同一出力になること

---

## 25. テストへの含意

### 25.1 snapshot test 軸

少なくとも以下の軸で snapshot を持つべきである。

- profile: `default`, `concise`, `verbose`, `ci`, `raw_fallback`
- color: on/off
- width: 60, 80, 100, 140
- source availability: readable / unreadable
- completeness: complete / partial / passthrough
- confidence bands: high / medium / low
- ownership: user / system / vendor / generated

### 25.2 corpus family

最低限以下の fixture family を持つべきである。

1. syntax error
2. type mismatch / conversion
3. overload candidate flood
4. template instantiation deep chain
5. macro expansion chain
6. include chain
7. linker undefined reference
8. linker multiple definition
9. analyzer path-like diagnostic
10. passthrough residual text

### 25.3 regression guard

以下は renderer regression として扱うべきである。

- lead group が変わる
- first action が first screenful から落ちる
- omission notice が消える
- raw fallback が silent に発動する
- path leak が起きる
- caret alignment が崩れる

### 25.4 contract test

text snapshot だけでなく、view model レベルで以下を検証することが望ましい。

- selected lead node
- displayed excerpt count
- collapsed counts
- confidence label
- whether raw message is retained
- suggestion ordering

---

## 26. canonical mock outputs

以下は **概念モック** であり、空白や句読点の 1 文字単位を固定するものではない。だが、情報の順序と section 構造は本仕様の意図を示す。

### 26.1 syntax error (`default`)

```text
error: likely missing ';' after this declaration
--> src/main.c:12:5
help: add ';' after 'int x = 10'
12 |     int x = 10
   |     ^^^^^^^^^^ expected ';' here
why: the parser reached the next statement before closing this declaration
omitted 3 follow-up parse errors
```

### 26.2 type mismatch / overload (`default`)

```text
error: this call passes 'const char *' where 'int' is required
--> src/parse.cc:48:20
help: convert the argument to int or call the overload that accepts text
48 |     set_limit("42");
   |               ^~~~ argument has type 'const char *'
why: selected overload expects 'int'
because: 'void set_limit(int)' is declared at include/config.hpp:19:6
omitted 4 other overload candidates
```

### 26.3 template instantiation (`default`)

```text
error: no matching constructor for 'Widget<T>' in this instantiation
--> src/build.cc:77:14
help: pass an allocator argument or use the default-constructible specialization
77 |     return Widget<T>(value);
   |              ^~~~~~~~~~~~~~~
why: 'T = std::pair<int, std::string>' is not default-constructible here
while instantiating:
  - src/build.hpp:41:8 required from 'make_widget<T>'
  - src/app.cc:12:16 required from here
omitted 9 internal template frames
```

### 26.4 macro + include (`default`)

```text
error: this macro expands to an expression with the wrong type
--> src/app.c:33:12
help: cast the macro result to 'size_t' or change the macro definition
33 |     size_t n = READ_COUNT(fd);
   |                ^~~~~~~~~~~~~~
why: expanded expression has type 'int'
through macro expansion:
  - src/app.c:33:12 invocation of 'READ_COUNT'
  - include/io_macros.h:14:9 definition expands to '(read_count(fd))'
from include chain:
  - include/io_macros.h:1:1 included from src/app.c:2
  - omitted 5 intermediate includes
```

### 26.5 linker undefined reference (`ci`)

```text
src/main.cc:1:1: error: undefined reference to 'foo::Bar::baz()'
  linker: referenced from main.o
  because: no definition for this symbol was found in the linked objects or libraries
  omitted 3 additional references to the same symbol
```

### 26.6 low-confidence mixed fallback

```text
error: showing a conservative wrapper view; original compiler diagnostics are preserved
--> third_party/lib.hpp:201:7
note: some compiler details were not fully structured
raw:
  ... original compiler diagnostics follow ...
```

---

## 27. 実装分割の推奨

本仕様を保守しやすく実装するには、少なくとも以下の層に分けるべきである。

```text
renderer/
  selector/          # group selection, ordering, budget
  view_model/        # semantic layout blocks
  excerpt/           # source loading, windowing, caret alignment
  family/            # template/macro/include/linker specific summarizers
  formatter/         # default/concise/verbose/ci text emitter
  fallback/          # raw-first and mixed fallback emitter
  labels/            # fixed English label catalog for v1alpha
```

### 27.1 selector

- lead group / lead node の決定
- warning suppression
- budget 配分

### 27.2 excerpt

- path resolution
- source loading
- line windowing
- caret / range alignment

### 27.3 family summarizers

- template frames compression
- candidate notes compression
- include / macro chain compression
- linker symbol grouping

### 27.4 formatter

- profile ごとの差分吸収
- ASCII safe emission
- line wrapping / truncation

### 27.5 fallback emitter

- partial / passthrough / failed 時の raw-first 表示

---

## 28. 初回実装で MUST / MAY / SHOULD defer

### 28.1 MUST implement

1. `default`, `verbose`, `ci`, `raw_fallback`
2. lead group selection
3. warning suppression on failure
4. primary excerpt 1 件以上
5. template / macro / include / linker の summary 表示
6. omission notice
7. low-confidence / partial / passthrough handling
8. raw message retention path
9. path leak 防止

### 28.2 MAY defer

1. `concise` profile の微調整
2. hyperlink support
3. analyzer path の高度な key-event 抽出
4. compact_safe 型名短縮の高度化

### 28.3 SHOULD defer until post-MVP

1. localization
2. vendor-specific CI fold markers
3. rich interactive UI
4. automatic fix application UX
5. user-customizable theming

---

## 29. この仕様の Done 条件

本仕様が「実装開始できる」状態であるための条件は以下とする。

1. renderer が受け取る `RenderRequest` と返す `RenderResult` が定義されている
2. group selection と ordering の優先順位が固定されている
3. profile ごとの budget が固定されている
4. partial / passthrough / raw_fallback の user-visible 振る舞いが決まっている
5. template / macro / include / linker の family rules が決まっている
6. source excerpt の windowing / range / omission 規則が決まっている
7. CI first line と path policy が決まっている
8. snapshot test 観点が定義されている

---

## 30. ADR 対応と post-MVP backlog

この仕様に関わる基線判断は、以下の ADR で固定済みである。

1. locale policy  
   `ADR-0011`

2. source ownership priority  
   `ADR-0015`

3. render surface / density / raw mode  
   `ADR-0019`

4. canonical label / wording / compatibility review  
   `ADR-0020`

以下は post-MVP backlog とし、v1alpha の renderer contract には含めない。

1. v1 label catalog の localizable 化
2. type name compaction を renderer と enrichment のどちらへ寄せるか
3. hyperlink を default-on にするか
4. build-wide aggregated view をいつ導入するか

---

## 31. まとめ

本仕様が固定した最大の判断は次の 5 つである。

1. **renderer は root cause を 1 件強く見せる opinionated default を持つ**
2. **profile ごとに明示的な budget を持ち、note flood を summary 化する**
3. **template / macro / include / linker を family-aware に表示する**
4. **low-confidence / partial 時は conservative に振る舞い、raw へ安全に戻る**
5. **CI では path-first, ASCII-safe, deterministic を優先する**

この契約により、renderer は「きれいな整形器」ではなく、**修正速度を最適化する意思決定レイヤ**として実装できる。

---

## 付録 A: 実装チェックリスト

### A.1 selector

- [ ] error/fatal が warning より先に来る
- [ ] lead group が deterministic に決まる
- [ ] warnings on failure が profile 通りに抑制される
- [ ] omitted count が出る

### A.2 excerpt

- [ ] source file が読める場合に snippet が出る
- [ ] tab / UTF-8 fixture で caret がズレない
- [ ] long line truncation が highlight を落とさない
- [ ] unreadable source でも location-only で崩れない

### A.3 family summaries

- [ ] template frames が budget 通りに圧縮される
- [ ] candidate flood が budget 通りに圧縮される
- [ ] macro invocation と definition が visible になる
- [ ] include chain が root TU まで追える
- [ ] linker repeated lines が symbol 単位に束ねられる

### A.4 fallback

- [ ] partial notice が 1 回だけ出る
- [ ] raw_fallback が silent に発動しない
- [ ] raw block の前に短い説明がある
- [ ] temp path を漏らさない

### A.5 formatter

- [ ] `default`, `verbose`, `ci` の snapshot が安定している
- [ ] color off でも意味が通る
- [ ] CI first line が path-first / linker-first になる
- [ ] same input -> same output を満たす

---

## 付録 B: 次に書くべき設計書との接続

この仕様の次に自然に接続する設計書は以下である。

1. **Quality / Corpus / Test Gate 仕様書**
   - この rendering contract の snapshot 軸と acceptance gate を固定する
2. **CLI / Config Surface 仕様書**
   - `profile`, `path_policy`, `debug_refs` をどの flag / config へ写像するか決める
3. **Enrichment / Ranking 仕様書**
   - `analysis.headline`, `first_action_hint`, `group_ref` をどう生成するか固定する
