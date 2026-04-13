
---
doc_role: reference-only
lifecycle_status: draft
audience: both
use_for: Subject-first / multi-block / configurable diagnostic presentation design review and issue drafting.
do_not_use_for: Current implementation contract until adopted into current-authority docs.
supersedes: []
superseded_by: []
---

> [!IMPORTANT]
> Authority: `reference-only` / `draft`
> Use for: design review, issue drafting, and implementation planning.
> Do not use for: current implementation contract until this proposal is accepted and reflected into current-authority docs.

# gcc-formed 診断表示 Presentation V2 設計書

- 文書種別: design / issue-draft packet
- 状態: Draft for review
- 対象: `horiyamayoh/gcc-formed` `main` を 2026-04-12 に読み取りながら整理した設計案
- 主題: Subject-first / multi-block / family-slot / config-driven diagnostic text format
- 想定読者: maintainer / reviewer / coding agent / future contributor

---

## 0. エグゼクティブサマリ

本設計は、`gcc-formed` の人間向け診断表示を、**「1 visible root = 1 block」** の Subject-first 形式へ移行する提案である。

北極星は次の 7 点である。

1. **先頭行は Subject-first にする。**  
   `src/main.cpp:5:12: ...` ではなく、`error: [type_mismatch] arguments do not match` のように「何の失敗か」を最初に置く。

2. **`error[type_mismatch]:` ではなく `error: [type_mismatch]` を採る。**  
   `:` の位置が severity の後ろで縦に揃い、block を複数並べたときに scan しやすい。

3. **family ごとに evidence slot を切り替える。**  
   すべてを `want / got / via` に強制しない。  
   type mismatch には `want / got / via`、syntax には `want / near`、missing header には `need / from`、redefinition には `now / prev`、linker には `symbol / from` を使う。

4. **複数 compile / link error は summary-only に落とさず、その block をそのまま縦に並べる。**  
   ただし cascade で hide / collapse された dependent / duplicate / follow-on は block にしない。  
   「1件の block が N 件並ぶだけ」という mental model を維持する。

5. **location は path-first ではなく inline suffix を基本にする。**  
   dedicated `-->` line を減らし、`@ src/main.cpp:5:12` のように header 末尾や evidence 行末尾へ寄せる。  
   `at:` 専用行は default では原則作らない。

6. **presentation はコードではなく設定で変えられるようにする。**  
   TOML の presentation file で template catalog と family mapping を定義し、設定が無いときは built-in default を使う。  
   人間にも AI にも編集しやすい形にする。

7. **analysis と presentation を分離する。**  
   selector / cascade は「何を見せるか」を決め、presentation policy は「どう見せるか」を決める。  
   public JSON は従来どおり機械向け正本であり、terminal text を scrape させない。

---

## 1. 最新 repo を軽く流し読みして確認したこと

この設計を起こす前に、`main` の以下を読み直した。

- `README.md`
- `docs/process/EXECUTION-MODEL.md`
- `docs/specs/rendering-ux-contract-spec.md`
- `docs/specs/public-machine-readable-diagnostic-surface-spec.md`
- `diag_render/src/layout.rs`
- `diag_render/src/view_model.rs`
- `diag_render/src/selector.rs`
- `diag_render/src/budget.rs`
- `diag_cli_front/src/config.rs`
- `config/cc-formed.example.toml`
- `rules/render.rulepack.json`
- `rules/residual.rulepack.json`

そこから固定しておきたい現状認識は次のとおり。

### 1.1 現行 repo の思想は今回の方向と噛み合う

`README.md` は `gcc-formed` を **shorter / root-cause-first / fail-open** の UX wrapper と明言している。  
「デフォルト TTY は native GCC より読みにくくなってはならない」も product 原則として掲げている。  
今回の Subject-first / evidence-first 路線は、repo の思想から外れていない。

### 1.2 現行 renderer は prose-heavy で、location line と raw `why:` が長さ感を増やしている

`diag_render/src/layout.rs` の現行順序は、ほぼ次のとおりである。

1. primary line
2. canonical location line (`--> ...`)
3. confidence notice
4. `help:`
5. `why: <raw_message>`
6. excerpts
7. context lines
8. child notes
9. collapsed notices
10. suggestions
11. raw sub-block

つまり、analysis headline があっても `why:` で raw compiler message を必ず足しており、headline と raw が重複して見えやすい。  
また default / verbose では location が dedicated line なので、視認上の行数が増えやすい。

### 1.3 現行 view model は flat で、summary-only root を持つ

`diag_render/src/view_model.rs` には `RenderGroupCard` と `SummaryOnlyGroup` があり、`RenderViewModel` には `cards` と `summary_only_groups` の両方がある。  
つまり今は「1件 expanded + 残り summary-only」という session model が前提になっている。

### 1.4 root cap は現在 cascade 側に寄っている

`diag_render/src/selector.rs` では `expanded_independent_root_limit()` が `request.cascade_policy.max_expanded_independent_roots` を参照している。  
つまり visible independent root をどこまで expanded にするかが、presentation ではなく cascade policy から来ている。

### 1.5 budget も lead group 1件中心で設計されている

`diag_render/src/budget.rs` の current default は、おおむね次を前提にしている。

- `default`: `expanded_groups = 1`, `first_screenful_max_lines = 28`
- `concise`: `expanded_groups = 1`, `first_screenful_max_lines = 14`
- `ci`: `expanded_groups = 1`, `first_screenful_max_lines = 16`

つまり「先頭の 1 件をどう見せるか」に最適化されている。

### 1.6 config はすでに overlay precedence を持つ

`diag_cli_front/src/config.rs` は `overlay.or(base)` の merge を実装しており、`config/cc-formed.example.toml` には `[render]` と `[cascade]` が存在する。  
つまり `presentation` / `presentation_file` のような additive config を入れる土台はすでにある。

### 1.7 rulepack には family ごとの headline / first-action の種が十分ある

`rules/render.rulepack.json` と `rules/residual.rulepack.json` には、`scope_declaration`, `redefinition`, `concepts_constraints`, `format_string`, `pointer_reference`, `linker.*` など、display family へマップできる internal family と action hint が並んでいる。  
だから今回の設計は、分析ロジック全交換ではなく **presentation surface の再編成** で進められる。

---

## 2. 今回の問題設定

### 2.1 いま起きている不満

現状の `gcc-formed` は「構造化されている」こと自体には価値がある。  
ただしユーザー体験としては、次の違和感が残る。

- 標準 GCC の raw diagnostics より「長く感じる」ことがある
- 特に type mismatch / template / candidate mismatch 系では、説明文を読むより差分を見たい
- location が先頭に来ると Subject / title を掴むまでに 1 呼吸かかる
- include 系は経路も重要なので、単純な `want / got / via` だけでは整理しきれない
- 複数 compile error のとき、1件だけ expanded で他が `other errors:` に落ちると、理解モデルが不連続になる
- 将来は業務都合で「この family はこの書式で出したい」という customization がほぼ確実に発生する

### 2.2 これは「短文化」だけでは解けない

行数を数行削るだけでは根本的には解けない。  
本当に必要なのは、**読み順の再設計**である。

具体的には、

- 1行目で Subject を掴む
- 2行目で最初の行動を掴む
- 3〜5行目で根拠を比較する
- 6行目以降は context / chain / omissions に留める

という情報階層に変える必要がある。

### 2.3 今回の提案は「同一テンプレート化」ではない

この設計は、すべての compiler error を 1 つの見た目に押し込める提案ではない。  
統一するのは **共通文法** であって、ラベルや evidence の中身までは統一しない。

共通文法は次の 3 段だけでよい。

1. Subject
2. First action
3. Family-specific evidence

この設計なら、type mismatch も syntax も include も linker も同じ読み順で読める。

---

## 3. Presentation V2 の設計目標

### 3.1 目標

1. **最初の 1〜2 行で「何の失敗か」「最初に何をすべきか」が分かる**
2. **証拠は prose ではなく比較可能な slot で出す**
3. **複数 error は 1件 block の単純反復で理解できる**
4. **cascade が隠したものだけを隠し、visible root は block にする**
5. **default TTY の読みやすさを native GCC より悪くしない**
6. **CI / concise / verbose / debug の差は「追加 tail の量」であって、core grammar の差ではない**
7. **将来の customization は config で済む**
8. **public JSON は machine contract のまま維持する**

### 3.2 非目標

1. IDE widget / tree UI をここで設計しない
2. ローカライズは扱わない
3. fix-it 自動適用は扱わない
4. internal family を public JSON ごと display family へ置き換えない
5. compiler ingest / IR 自体の刷新はこの提案の範囲外

---

## 4. 北極星となる block grammar

### 4.1 canonical grammar

すべての visible root block は、次の共通文法で表示する。

```text
<severity>: [<display-family>] <subject>[ @ <location-suffix>]
help: <first action>
<family-specific evidence line 1>
<family-specific evidence line 2>
<context / chain / omission summary>
```

### 4.2 canonical example

```text
error: [type_mismatch] arguments do not match
help: make both arguments the same type
want: T, T
got : int, const char*
via : combine(T, T)  +2 notes
```

### 4.3 `error: [type_mismatch]` を採る理由

`error[type_mismatch]:` ではなく `error: [type_mismatch]` にする。

理由は次の 4 つ。

1. `:` の位置が severity の直後で揃うので、縦スクロール時に見やすい
2. `severity` と `family` と `subject` が 3 要素として分離される
3. family tag が headline の一部ではなく注釈に見えるので、主語が subject に残る
4. block を複数並べたときに header の視線誘導が安定する

### 4.4 `help:` は core に残す

以前の議論では `fix:` も魅力があった。  
ただし built-in default では、現行 spec の語彙と整合しやすい `help:` を維持する。  
その代わり **label catalog を config override 可能** にする。  
これにより、運用側が `help` を `fix` や `next` へ変えたくなってもコード変更は不要になる。

---

## 5. display family と internal family を分ける

### 5.1 なぜ分けるか

internal family は analysis / rulepack 的には正しいが、そのまま `[scope_declaration]` のように見せると少し内部名すぎる。  
表示側では、人間が理解しやすい粗い tag に丸めた方がよい。

### 5.2 2 層モデル

- **internal family**  
  analysis / rulepack / public JSON のための machine family
- **display family**  
  terminal text の `[ ... ]` に出す human-facing family

### 5.3 built-in display family 初期案

| internal family / prefix | display family | built-in template |
|---|---|---|
| `type_overload`, `concepts_constraints`, `format_string`, `conversion_narrowing`, `const_qualifier`^1^ | `type_mismatch` | `contrast_block` |
| `syntax`, `openmp`, `attribute`, `storage_class`, `module_import`, `coroutine`, `asm_inline`, `preprocessor_directive`(missing header を除く) | `syntax` | `parser_block` |
| `scope_declaration` | `missing_name` | `lookup_block` |
| `pointer_reference` | `incomplete_type` または `reference_misuse` | `lookup_block` |
| `deleted_function`, `access_control` | `unavailable_api` | `lookup_block` |
| `redefinition`, `odr_inline_linkage` | `redefinition` | `conflict_block` |
| `macro_include` | `macro_include` | `context_block` |
| `linker.*` | `linker` | `linker_block` |
| unmatched / unknown / low-confidence | `raw` または `other` | `generic_block` |

> ^1^ `const_qualifier` は const 修飾の追加・除去に関する診断であり、厳密には「型が不一致」とは異なる。ただし GCC が出す診断パターンでは `expected` vs `actual` の対比構造を持つため、`contrast_block` との相性が高い。将来、`const_qualifier` 固有の evidence slot（例: `qualify: const` / `strip: const`）が有用になった場合は、dedicated template への分離を検討する。

### 5.4 public JSON には display family を入れない

display family は config で変えうる。  
それを public JSON に混ぜると machine contract が presentation customization に引っ張られる。  
したがって public JSON は internal semantics のまま維持し、display family は internal debug artifact にのみ出してよいものとする。

---

## 6. family ごとの価値検証

### 6.1 type mismatch / overload / constraints

最も相性が良い。  
ユーザーが知りたい本質が `expected vs actual` だからである。

```text
error: [type_mismatch] arguments do not match
help: make both arguments the same type
want: T, T
got : int, const char*
via : combine(T, T)  +2 notes
```

### 6.2 syntax / parser expectation

相性は良いが、`got` より `near` が強いことが多い。

```text
error: [syntax] expected ';' after declaration
help: insert ';' before the next declaration
want: ';'
near: struct Widget { int x } @ include/foo.hpp:42:7
```

必要なら excerpt を続ける。

```text
... Widget { int x } struct Next ...
                    ^ expected ';'
```

### 6.3 missing header

include 自体が主役の direct failure なので、専用 block がよい。

```text
error: [missing_header] header could not be found
help: fix the include path or add the dependency that provides it
need: "foo/bar.hpp"
from: #include "foo/bar.hpp" @ src/main.cpp:1:10
```

### 6.4 missing declaration / incomplete type / include 不足の二次影響

これは include ではなく **本当の failure** を headline にした方が強い。

```text
error: [missing_name] identifier is not declared
help: include the declaration or fix the symbol name
name: Widget
use : Widget w; @ src/main.cpp:8:5
```

または:

```text
error: [incomplete_type] type is incomplete here
help: include the full definition before using the type
name: Widget
use : sizeof(Widget) @ src/main.cpp:13:18
```

### 6.5 redefinition

文章より `now / prev` の方が理解が速い。

```text
error: [redefinition] symbol is defined more than once
help: keep one definition and turn the others into declarations if needed
now : src/main.cpp:12:5
prev: include/widget.hpp:7:5
```

### 6.6 macro / include context

headline は経路ではなく failure subject に寄せ、経路は `in / via / from` に落とす。

```text
error: [macro_include] error surfaced through macro expansion
help: inspect the outermost user macro invocation first
in  : FETCH_VALUE(id) @ src/main.c:9:12
via : OUTER_ACCESS -> INNER_ACCESS  +1 frame
```

### 6.7 linker

source span より `symbol / object / archive` が主役なので専用 block の価値が高い。

```text
error: [linker] symbol could not be resolved
help: define the symbol or link the library that provides it
symbol: missing_symbol
from  : main.o  +3 refs
```

### 6.8 generic / low-confidence / passthrough

ここでは無理な structuring をしない。

```text
error: [raw] compiler diagnostics were preserved
help: inspect the original diagnostics directly
why : structured summary is low-confidence here
raw :
  ...
```

### 6.9 結論

この路線は type mismatch 専用ではない。  
**共通文法 + family-specific slot** に一般化すれば、多くの compile / link error へ意味ある形で展開できる。

---

## 7. include は一枚岩ではない

include は今回の要点なので、明示的に 3 分類しておく。

### 7.1 類型 A: header 自体が見つからない

headline は include 自体でよい。

```text
error: [missing_header] header could not be found
help: fix the include path or add the dependency that provides it
need: "foo/bar.hpp"
from: #include "foo/bar.hpp"
```

### 7.2 類型 B: included header 内で別の本体エラーが起きている

headline は include ではなく本当の failure にする。  
include chain は context に落とす。

```text
error: [syntax] expected ';' after declaration
help: insert ';' before the next declaration
want: ';'
from: src/main.cpp -> include/foo.hpp
```

### 7.3 類型 C: include 不足のせいで declaration / complete type が見えていない

headline は `missing_name` や `incomplete_type` にする。  
first action の中で include を言えばよい。

```text
error: [missing_name] identifier is not declared
help: include the declaration or fix the symbol name
name: Widget
use : Widget w; @ src/main.cpp:8:5
```

### 7.4 この分類が重要な理由

ユーザーの「include がないかも」は、実際には

- header missing
- include context
- declaration visibility failure

のどれかであることが多い。  
この 3 つを混ぜると、headline がブレて block grammar の一貫性が崩れる。

---

## 8. block template catalog（built-in default）

### 8.1 なぜ named template にするか

運用側は `A/B/C...` と呼んでもよい。  
ただし built-in では、レビュー・差分確認・issue 管理のために意味のある template ID を持たせる。

- `contrast_block`
- `parser_block`
- `lookup_block`
- `missing_header_block`
- `conflict_block`
- `context_block`
- `linker_block`
- `generic_block`

必要なら config 側で `A = contrast_block` のような alias を表現してよい。

### 8.2 built-in templates

#### 8.2.1 `contrast_block`

用途:
- type mismatch
- overload / candidate flood
- constraints / conversion

core:
1. header
2. `help`
3. `want`
4. `got`
5. `via`

excerpt:
- default off
- 誤読の余地が大きい場合のみ auto-on

#### 8.2.2 `parser_block`

用途:
- syntax
- parser expectation
- token near failure

core:
1. header
2. `help`
3. `want`
4. `near`

excerpt:
- auto-on
- highlight-centered windowing MUST

#### 8.2.3 `lookup_block`

用途:
- missing name
- incomplete type
- unavailable API
- declaration visibility failure

core:
1. header
2. `help`
3. `name`
4. `use`
5. 必要なら `need`

excerpt:
- auto

#### 8.2.4 `missing_header_block`

用途:
- direct include path failure

core:
1. header
2. `help`
3. `need`
4. `from`

excerpt:
- off

#### 8.2.5 `conflict_block`

用途:
- redefinition
- duplicate symbol
- now vs previous

core:
1. header
2. `help`
3. `now`
4. `prev`

excerpt:
- off by default
- verbose/debug で contrasted location を追加可

#### 8.2.6 `context_block`

用途:
- macro/include chain
- user-owned macro call context

core:
1. header
2. `help`
3. `in`
4. `via`
5. `from` or `root`

excerpt:
- auto

#### 8.2.7 `linker_block`

用途:
- `linker.*`

core:
1. header
2. `help`
3. `symbol`
4. `from`
5. `archive`（必要時）

excerpt:
- off

#### 8.2.8 `generic_block`

用途:
- low-confidence
- unknown family
- raw-first honesty

core:
1. header
2. `help`
3. `why`
4. `raw`（必要時）

excerpt:
- off

### 8.3 label width

built-in default では **per-block dynamic alignment** を採用する。

#### alignment 規則

1. 各 block 内の evidence label のうち最長のものを `max_label_width` とする
2. すべての label を `max_label_width` まで右に空白を詰めてから `:` を置く
3. `max_label_width` のフロアは 4 とする（短い label だけの block でも最低 4 文字幅を確保）

#### 例: contrast_block（max_label_width = 4）

- `want:`
- `got :`
- `via :`

#### 例: linker_block（max_label_width = 7）

- `help   :`
- `symbol :`
- `from   :`
- `archive:`

この規則自体も presentation config の `label_width` で override 可能にする。  
`label_width` に固定値を指定した場合は per-block dynamic ではなくその固定値を使う。

---

## 9. rendering algorithm

### 9.1 high-level pipeline

```text
diagnostic document
  -> selector / cascade
  -> visible roots
  -> semantic slot extraction
  -> presentation policy resolve
  -> block template render
  -> profile tail / omission notices
```

### 9.2 steps

1. selector が visible root を選ぶ
2. cascade が dependent / duplicate / follow-on の hide / collapse を決める
3. semantic slot extractor が root ごとに slot map を作る
4. presentation resolver が internal family -> display family / template / label / location policy を解決する
5. template engine が core block を作る
6. profile に応じて excerpt / tail / raw footer を足す
7. session formatter が blank line で block を区切る

### 9.3 fail-open

次の場合は `generic_block` へ degrade する。

- family mapping が見つからない
- template が壊れている
- core evidence line が空になりすぎた
- confidence が低く、analysis headline を主経路に置けない
- raw を見せないと不誠実になる

---

## 10. location policy

### 10.1 要件

- path を先頭に置きたくない
- location を隠したくはない
- `at:` 行を増やしたくない
- line count は増やしたくない

### 10.2 結論

default の built-in subject-block preset では、**dedicated location line を廃止し、inline suffix を基本**にする。

### 10.3 placement algorithm

優先順位は次のとおり。

1. header suffix に置けるなら置く  
   `error: [type_mismatch] arguments do not match @ src/main.cpp:5:12`

2. header が長すぎるなら family-specific evidence line に移す  
   `via : combine(T, T) @ src/main.cpp:5:12  +2 notes`

3. excerpt が visible なら excerpt header に委譲してよい  
   block header 自体は subject-only のままでもよい

4. それでも失うなら verbose/debug のみ dedicated line を許可  
   default / concise / ci では `at:` を常用しない

### 10.4 CI

interactive default と CI は切り分ける。

- `default / concise / verbose / debug`: subject-first
- `ci`: preset ごとに subject-first か path-first を選べる

初期 built-in では、CI は path-first 維持でもよい。  
大事なのは **interactive default を CI の都合で劣化させない**こと。

---

## 11. excerpt policy

### 11.1 基本方針

excerpt は装飾ではなく、**いま直すべき source point を確定する視覚補助**である。  
したがって常時 on ではなく、family と有用性で出し分ける。

### 11.2 built-in default

- `contrast_block`: off
- `parser_block`: auto-on
- `lookup_block`: auto
- `missing_header_block`: off
- `conflict_block`: off
- `context_block`: auto
- `linker_block`: off
- `generic_block`: off

### 11.3 windowing MUST

long line の excerpt は **highlight-centered windowing** を MUST にする。

```text
... vec.push_back(widget_factory(id, flags, context)) ...
                ^^^^^^^^^^^^^ no matching overload
```

規則:

- 左右に `...` を付ける
- highlight は必ず window 内に残す
- window を切ったことを隠さない
- caret alignment が明らかに壊れる場合は summary annotation に degrade する

### 11.4 excerpt と location

excerpt が visible な場合、その excerpt header が location carrier になってよい。  
これにより `at:` 行を増やさずに location を保持できる。

---

## 12. 複数エラー時の挙動

### 12.1 結論

**はい、その認識で正しい。**  
複数 compile / link error を検出したときは、この block が連続して縦に表示される設計にする。

### 12.2 重要な条件

ただし block になるのは **visible root のみ** である。

- cascade で hide された dependent / duplicate / follow-on は block にしない
- cascade で parent に吸収された chain は suffix や omission notice へ落とす
- warning suppression policy は profile ごとに尊重する

### 12.3 新しい mental model

現行:
- lead 1 件 expanded
- visible root overflow は summary-only
- dependent chain は collapsed notice

Presentation V2:
- visible root A -> block
- visible root B -> block
- visible root C -> block
- hidden dependent members -> A/B/C block 内の suffix / omission notice
- suppressed warnings -> session tail note

つまり **1件 block の N 回反復** である。

### 12.4 canonical session example

```text
error: [type_mismatch] arguments do not match
help: make both arguments the same type
want: T, T
got : int, const char*
via : combine(T, T) @ src/main.cpp:5:12  +2 notes

error: [missing_name] identifier is not declared
help: include the declaration or fix the symbol name
name: Widget
use : Widget w; @ src/main.cpp:8:5

error: [linker] symbol could not be resolved
help: define the symbol or link the library that provides it
symbol: missing_symbol
from  : main.o  +3 refs

note: suppressed 2 warning(s) while focusing on failure blocks
raw: rerun with --formed-profile=raw_fallback to inspect the original compiler output
```

### 12.5 visible root summary-only は built-in default でやめる

subject-block preset の default では、visible root を summary-only bucket へ落とさない。  
ただし例外はある。

- warning-only run の初期互換モード
- user config で `lead_plus_summary` を明示したとき
- internal safety cap を超えた extreme case（契約外の防御）

### 12.6 session mode

presentation policy に session mode を持たせる。

- `all_visible_blocks`
- `lead_plus_summary`
- `capped_blocks`

built-in default は次を推奨する。

- compile/link failure run: `all_visible_blocks`
- warning-only run: 当面 `lead_plus_summary`
- verbose/debug: `all_visible_blocks`

### 12.7 warning-only run の将来方向

warning-only run を当面 `lead_plus_summary` に据え置く理由は、warning block は failure block ほど即座に修正対象にならないことが多く、全展開すると情報過多になりやすいためである。  
将来的には以下の段階で移行を検討する。

1. **Phase 3 以降**: warning severity にも subject-block grammar を適用するが、session mode は `capped_blocks`（上限付き block 展開）を既定にする  
2. **ユーザー feedback 次第**: warning-only run でも `all_visible_blocks` を config で選べるようにする（Issue 02 の config surface で対応可能）  
3. **長期**: warning / note の priority ranking が成熟したら、top-N block + tail summary へ最適化する

---

## 13. cascade と presentation の責務分離

### 13.1 原則

- cascade は **dependent / duplicate / follow-on をどう扱うか** を決める
- presentation は **visible root をどう描くか** を決める

### 13.2 現状の問題

現在の `max_expanded_independent_roots` は `cascade` セクションにあり、selector 側で visible independent root の expanded 数を決めている。  
これは Subject-block design では責務がずれている。

### 13.3 提案

`cascade.max_expanded_independent_roots` は **presentation.session.max_root_blocks** へ移す。  
旧 field は compatibility alias として当面読むが、最終的には deprecate する。

### 13.4 新しい整理

- `cascade.*`
  - hide / suppress / summary eligibility
  - parent margin / likelihood thresholds
- `presentation.session.*`
  - visible root mode
  - max root blocks
  - block separator
  - warning-only mode

---

## 14. budget policy の見直し

### 14.1 現行の問題

session-global の line budget は、複数 block との相性が悪い。  
2件目以降が見切れやすいからである。

### 14.2 新しい考え方

budget は **session-global** から **block-local** へ重心を移す。

### 14.3 built-in default の目安

#### `default`
- target 5–8 lines / block
- hard max 10 lines / block
- blank line separator 1
- session-global truncation 無効
- internal safety cap: **50 blocks**（contract 外の防御。超過分は `+N more error(s)` 1 行で打ち切る）

#### `concise`
- target 4–6 lines / block
- hard max 7 lines / block
- excerpt は syntax family 以外極力 off

#### `verbose`
- core grammar は default と同じ
- tail に context / notes / raw を追加

#### `debug`
- verbose + debug facts / resolved template / slot trace

#### `ci`
- 4–6 lines / block
- path-first か subject-first かは preset ごと
- multi-root block 自体は維持

### 14.4 省略優先順位

1. extra context tails
2. extra child notes
3. secondary excerpts
4. repeated notes/candidates/frames（suffix 化）
5. raw sub-block
6. dedicated location line（default ではそもそも使わない）

### 14.5 省略禁止

- severity
- display family
- subject
- first action
- 最低 1 つの evidence
- low-confidence / raw fallback honesty

---

## 15. config-driven customization

### 15.1 目標

将来の業務カスタマイズでは、次が起きると想定する。

- `help:` を `fix:` に変えたい
- `type_overload` と `concepts_constraints` は同じ template を使いたい
- `macro_include` だけ context-heavy template にしたい
- CI だけ path-first にしたい
- separator を無くしたい
- family tag 名を現場用語に寄せたい

これをコード変更なしで行う。

### 15.2 設定形式は TOML

理由:

1. repo ですでに TOML config を採用している
2. コメントを書ける
3. YAML より indentation 事故が少ない
4. humans / AI ともに diff / merge しやすい
5. Rust loader / validation が素直
6. deterministic parsing と相性がよい

### 15.3 2 層 config

#### 15.3.1 main config

短い operational setting を置く。

```toml
schema_version = 1

[render]
profile = "default"
path_policy = "relative_to_cwd"
presentation = "subject_blocks_v1"
presentation_file = "/home/me/.config/cc-formed/presentation.toml"
```

#### 15.3.2 presentation file

template / family mapping / labels / location / session policy を置く。

```toml
kind = "cc_formed_presentation"
schema_version = 1

[session]
visible_root_mode = "all_visible_blocks"
warning_only_mode = "lead_plus_summary"
block_separator = "blank_line"
unknown_template = "generic_block"

[labels]
help = "help"
want = "want"
got = "got"
via = "via"
need = "need"
from = "from"
name = "name"
use = "use"
near = "near"
symbol = "symbol"
archive = "archive"
now = "now"
prev = "prev"

[location]
default_placement = "inline_suffix"
fallback_order = ["header", "evidence", "excerpt_header", "none"]
width_soft_limit = 100
label_width = 4

[[templates]]
id = "contrast_block"
excerpt = "off"
core = [
  { slot = "first_action", label = "help" },
  { slot = "expected", label = "want" },
  { slot = "actual", label = "got" },
  { slot = "via", label = "via", suffix_slot = "omitted_notes_suffix" },
]

[[templates]]
id = "linker_block"
excerpt = "off"
core = [
  { slot = "first_action", label = "help" },
  { slot = "symbol", label = "symbol" },
  { slot = "from", label = "from", suffix_slot = "omitted_refs_suffix" },
  { slot = "archive", label = "archive", optional = true },
]

[[family_mappings]]
match = ["type_overload", "concepts_constraints", "format_string", "conversion_narrowing"]
display_family = "type_mismatch"
template = "contrast_block"

[[family_mappings]]
match = ["prefix:linker."]
display_family = "linker"
template = "linker_block"
```

### 15.4 free-form string template は採らない

完全自由な `header_format = "{severity}: ..."` 方式だけに依存しない。  
それだと validation と fail-open が難しい。

採るのは **slot-based declarative template** である。

- line = `slot + label + optional suffix_slot`
- slot は catalog から選ぶ
- slot が無ければその line は skip
- evidence が空になりすぎたら `generic_block` へ fallback

### 15.5 built-in default も asset 化する

built-in default も可能なら Rust コードにハードコードせず、checked-in TOML asset として持つ。

推奨配置:

- `config/presentation/subject_blocks_v1.toml`
- `config/presentation/legacy_v1.toml`

### 15.6 precedence

現行 config merge と同様に、presentation も次の precedence を取る。

1. CLI override
2. user config
3. admin config
4. built-in default

### 15.7 fail-open for presentation config

presentation customization は non-fatal とする。

- TOML parse 失敗 ->その file を無視して built-in default
- unknown template -> `generic_block`
- unknown slot -> line skip + warning
- invalid location policy -> built-in default
- invalid family mapping ->その mapping だけ無効

presentation file が壊れたせいで compile/link invocation 全体を止めない。

---

## 16. semantic slot layer

### 16.1 なぜ必要か

現行 `RenderGroupCard` は `title`, `raw_message`, `context_lines`, `child_notes` など flat string field が中心である。  
このまま layout だけをいじると、`why:` と新 evidence line が競合し、また prose-heavy に戻る。

### 16.2 新しい内部モデル

```text
selector
  -> visible roots
semantic slot extractor
  -> slot map + tail sections
presentation resolver
  -> display family + template + location host
formatter
  -> text output
```

### 16.3 推奨 struct 概念

```rust
struct RenderSemanticCard {
    group_id: String,
    severity: String,
    internal_family: Option<String>,
    display_family: String,
    template_id: String,
    subject: String,
    first_action: Option<String>,
    primary_location: Option<String>,
    slots: BTreeMap<SlotId, SlotValue>,
    omitted_suffixes: Vec<SlotSuffix>,
    core_excerpt: Option<ExcerptBlock>,
    tail_sections: Vec<RenderContextSection>,
    raw_sub_block: Vec<String>,
    confidence_notice: Option<String>,
}
```

### 16.4 stable slot catalog 初期案

- `expected`
- `actual`
- `via`
- `name`
- `use`
- `need`
- `from`
- `near`
- `symbol`
- `archive`
- `now`
- `prev`
- `why_raw`

### 16.5 extraction 原則

slot extraction の優先順位:

1. analysis overlay / structured IR
2. context chain / child notes の structured fact
3. rulepack-aware heuristic
4. raw regex heuristic
5. 取れなければ空

禁止:

- invented fact を slot に埋める
- `expected` と `actual` が曖昧なのに両方埋めたふりをする
- raw fact を消す

### 16.6 `why:` の位置づけ

`why:` は core grammar から外す。  
ただし `generic_block` と low-confidence honesty では使ってよい。

---

## 17. profile 間の一貫性

### 17.1 原則

`default / concise / verbose / debug / ci` は、**core grammar を変えずに tail と budget だけ変える**のが望ましい。

### 17.2 期待する差分

- `default`: もっとも代表的な見た目
- `concise`: excerpt / tail を減らす
- `verbose`: core lines の後ろに context を増やす
- `debug`: verbose + resolved presentation facts
- `ci`: same core grammar ただし first line policy は preset 依存

### 17.3 これで得られるもの

- profile を跨いでも「どこに何が出るか」がほぼ同じ
- docs と corpus comparison が簡単
- customization も template と budget を差し替えるだけで済む

---

## 18. public JSON との関係

### 18.1 変えないこと

public JSON は machine-readable contract であり、terminal presentation customization と独立している。  
したがって次を基本的に変えない。

- `family`
- `headline`
- `first_action`
- `primary_location`
- `related_diagnostics`
- execution metadata

### 18.2 変えるべきでない理由

presentation が config で変わるようになるほど、terminal text を scrape するのは危険になる。  
むしろ public JSON の独立性はさらに重要になる。

### 18.3 internal debug artifact は追加してよい

レビューや snapshot のために、次の internal-only artifact を足す価値がある。

- `render.presentation.json`
  - resolved template id
  - display family
  - extracted slots
  - location host decision
  - omission strategy

これは public contract ではなく、test/debug artifact とする。

---

## 19. rollout 方針

### 19.1 一気に default 差し替えはしない

Execution Model 的に、まず contract と architecture を固定し、次に opt-in preset を出し、そのあと default promotion を行う。

### 19.2 Phase 0 — design / docs / preset asset

- design draft を整理
- contract rewrite 草案
- built-in preset TOML 草案
- README before/after 草案

### 19.3 Phase 1 — opt-in preset

- `presentation = "subject_blocks_v1"` 指定時のみ新表示
- `legacy_v1` は現行互換
- corpus snapshot を side-by-side 比較

### 19.4 Phase 2 — default promotion

- regression と corpus review を通ったら default を `subject_blocks_v1` に切替
- `legacy_v1` は互換 preset として残す
- example config / README を更新

### 19.5 Phase 3 — family coverage expansion

- residual families に mapping を広げる
- 新しい family 追加が mostly TOML edit で済む状態へ寄せる

---

## 20. 代表例集

### 20.1 type mismatch

```text
error: [type_mismatch] arguments do not match
help: make both arguments the same type
want: T, T
got : int, const char*
via : combine(T, T) @ src/main.cpp:5:12  +2 notes
```

### 20.2 syntax

```text
error: [syntax] expected ';' after declaration
help: insert ';' before the next declaration
want: ';'
near: struct Widget { int x } @ include/foo.hpp:42:7

... Widget { int x } struct Next ...
                    ^ expected ';'
```

### 20.3 missing header

```text
error: [missing_header] header could not be found
help: fix the include path or add the dependency that provides it
need: "foo/bar.hpp"
from: #include "foo/bar.hpp" @ src/main.cpp:1:10
```

### 20.4 missing name

```text
error: [missing_name] identifier is not declared
help: include the declaration or fix the symbol name
name: Widget
use : Widget w; @ src/main.cpp:8:5
```

### 20.5 redefinition

```text
error: [redefinition] symbol is defined more than once
help: keep one definition and move the others to declarations if needed
now : src/main.cpp:12:5
prev: include/widget.hpp:7:5
```

### 20.6 macro/include context

```text
error: [macro_include] error surfaced through macro expansion
help: inspect the outermost user macro invocation first
in  : FETCH_VALUE(id) @ src/main.c:9:12
via : OUTER_ACCESS -> INNER_ACCESS  +1 frame
```

### 20.7 linker

```text
error: [linker] symbol could not be resolved
help: define the symbol or link the library that provides it
symbol: missing_symbol
from  : main.o  +3 refs
```

### 20.8 generic

```text
error: [raw] compiler diagnostics were preserved
help: inspect the original diagnostics directly
why : structured summary is low-confidence here
raw :
  ...
```

---

## 21. 影響範囲

### 21.1 docs / contracts

- `README.md`
- `docs/specs/rendering-ux-contract-spec.md`
- `docs/specs/public-machine-readable-diagnostic-surface-spec.md`
- `config/cc-formed.example.toml`
- 新規 `config/presentation/*.toml`
- 必要なら新規 ADR

### 21.2 renderer code

- `diag_render/src/view_model.rs`
- `diag_render/src/layout.rs`
- `diag_render/src/formatter.rs`
- `diag_render/src/family.rs`
- `diag_render/src/excerpt.rs`
- `diag_render/src/selector.rs`
- 新規 `diag_render/src/presentation.rs` または `diag_render/src/presentation/*`

### 21.3 CLI / config

- `diag_cli_front/src/config.rs`
- `diag_cli_front/src/args.rs`
- `diag_cli_front/src/render.rs`

### 21.4 tests / corpus

- renderer unit tests
- config parse / merge tests
- snapshot tests
- corpus replay
- optional `render.presentation.json` fixture

---

## 22. リスクと対策

### 22.1 複数 block で長くなる

対策:
- `why:` を core から外す
- dedicated location line を削る
- per-block budget へ移行
- repeated notes を suffix 化する

### 22.2 family ごとに見た目が増えすぎる

対策:
- built-in template 数を 6〜8 に制限
- new family は原則既存 template へ map
- template を code ではなく config で管理

### 22.3 customization が壊れる

対策:
- slot-based TOML に限定
- invalid config は fail-open
- unknown slot / template は warning + fallback

### 22.4 public JSON と text の意味がズレる

対策:
- public JSON に display family を入れない
- internal debug artifact を分ける
- docs で text scraping 非推奨を明記

### 22.5 CI grepability が落ちる

対策:
- CI preset を別に持てるようにする
- path-first CI を残せるようにする
- interactive default と CI を分離する

### 22.6 ANSI color / styling との相互作用

Subject-first header では、severity / family tag / subject / location suffix が 1 行に並ぶ。  
これらの色分けルールを決めないと、block grammar が意味的に明快でも視覚的に掴みにくくなるリスクがある。

対策:
- severity は従来どおり severity 色（error=赤、warning=黄 等）で塗る
- `[display_family]` は bold なし・dim または severity 色の暗めトーンにし、subject より目立たないようにする
- subject text は default foreground + bold にし、header の主語として最も目を引く位置に残す
- evidence label（`want:` / `got:` 等）は cyan 系で統一し、label 列が縦に揃って見えるようにする
- 具体的な color scheme は Issue 07（location / alignment / excerpt の仕上げ）で扱う
- presentation config に `[colors]` section を将来追加する余地を残すが、初期 built-in は hard-coded default でよい

---

## 23. open questions と推奨回答

### Q1. CI も subject-first に揃えるべきか
推奨回答: 初期 built-in は path-first 維持でもよい。  
ただし config で subject-first CI を選べるようにする。

### Q2. warning-only run も all-visible blocks にするか
推奨回答: failure run は all-visible blocks、warning-only run は当面 legacy 互換寄り。  
将来 config で切替可能にする。

### Q3. `help:` を `fix:` に変えるか
推奨回答: built-in default は `help:` 維持。  
ただし config で override 可能にする。

### Q4. built-in family tag は何個まで許容するか
推奨回答: 8 前後に抑える。  
増やす場合も code ではなく mapping config で増やせる設計にする。

---

## 24. 実装順序の推奨

1. 契約文書 / ADR / design を固定
2. presentation config schema と built-in preset asset を入れる
3. semantic slot layer と template resolver の骨格を作る
4. multi-root block session model を入れる
5. contrast + linker families を先行実装 ← **Issue 05 と 06 は並列実行可能**
6. syntax + lookup + conflict + context を追加 ← **Issue 05 と 06 は並列実行可能**
7. location policy と excerpt windowing を仕上げる
8. opt-in preset として corpus / snapshot を検証
9. default promotion と legacy cleanup

---

## 25. Epic / Issue 草稿パケット

以下は GitHub Issue 草稿としてほぼそのまま使える粒度を想定している。  
Execution Model の field vocabulary を本文中に明示している。

---

# Epic — Presentation V2: Subject-first configurable diagnostic blocks

- Workstream: Render
- Band: Cross-cutting
- Processing Path: Cross-path
- Support Level: Beta
- Layer: ViewModel / Theme / Templates
- Issue Kind: Epic
- Task Size: L
- Risk: High
- Contract Change: UX / CLI / Docs
- Agent Ready: No
- Night Batch: None
- Human Review Type: Design
- Stop-Ship: Yes
- Owner Layer: Shared
- Acceptance ID: `EPIC-PV2-SUBJECT-BLOCKS`

## Objective

`gcc-formed` の人間向け diagnostic text を、Subject-first / family-slot / multi-block の Presentation V2 へ移行する。  
目的は prettier ではなく、**最初の fix に到達する速度を上げつつ、複数 root を honest に見せ、将来の運用カスタマイズを code-free にすること**である。

## Why this matters to doctrine

repo はすでに shorter / root-cause-first / fail-open / default TTY non-regression を掲げている。  
しかし現行 renderer は `why:` と dedicated location line と summary-only root の組み合わせにより、「構造化されているが長く感じる」体験が残る。  
Presentation V2 は doctrine を崩すのではなく、むしろ**読ませたい順に情報を並べ直す**ための再設計である。

## Completion criteria

- [ ] current-authority docs に Presentation V2 の contract が固定されている
- [ ] built-in preset と external presentation file schema が存在する
- [ ] visible compile/link failure roots が subject-block として縦に並ぶ
- [ ] cascade-hidden dependent / duplicate が block 化されない
- [ ] primary families で family-specific evidence slot が有効
- [ ] invalid presentation config が fail-open で built-in default へ戻る
- [ ] public JSON と terminal text の責務分離が docs / tests で固定されている

## Dependencies

- Execution Model の issue taxonomy を前提にする
- rendering spec / public JSON spec / README の rewrite を伴う
- corpus snapshot のレビューコストが高いので opt-in phase を置く

## Generates these work package classes

- [ ] Docs / ADR
- [ ] Config schema / loader
- [ ] ViewModel / presentation skeleton
- [ ] Selector / budget rewrite
- [ ] Family-specific rendering
- [ ] Snapshot / corpus validation
- [ ] Default-promotion cleanup

## Out of scope

- IDE widget
- localization
- fix-it 自動適用
- public JSON の presentation 化
- compiler ingest architecture overhaul

## No-go conditions

- contract docs が未承認のまま user-visible behavior change を入れる
- visible root を summary-only に落とす旧 mental model を new default に残す
- customization を hard-coded enum 追加だけで実装し、config surface を後回しにする
- public JSON と text presentation の責務が混線する

---

# Issue 01 — ADR / contract rewrite for Presentation V2

- Workstream: Docs / Render
- Band: Cross-cutting
- Processing Path: Cross-path
- Support Level: Beta
- Layer: Templates / Theme
- Issue Kind: ADR
- Task Size: M
- Risk: High
- Contract Change: Docs / UX
- Agent Ready: No
- Night Batch: None
- Human Review Type: Design
- Stop-Ship: Yes
- Owner Layer: Maintainer
- Depends On: Epic only
- Commands: `cargo xtask check` (if examples or snapshots touched)
- Acceptance ID: `PV2-01-CONTRACT`

## Goal

Presentation V2 の意味論を current-authority docs として固定する。  
コード変更より先に、少なくとも次を文書で明確にする。

- Subject-first header grammar
- `error: [family] subject` 形式
- visible root = block, cascade-hidden = no block
- multi-error session model
- display family / internal family の 2 層
- family-specific evidence slot の catalog と位置づけ
- inline location suffix policy
- presentation config の責務と fail-open
- public JSON を presentation から独立させること
- rollout: opt-in preset -> default promotion

## Why now

本件は formatter の小修正ではなく、renderer contract / config semantics / snapshot expectations をまたぐ。  
Execution Model 上、こうした user-visible behavior は contract rewrite より先に進めるべきではない。  
先に docs を固めないと、後続の issue が毎回 design review をやり直すことになり、夜間分割にも向かない。

## Parent epic / ADR

- Parent epic: Presentation V2: Subject-first configurable diagnostic blocks
- ADR candidate: `adr-presentation-v2-subject-blocks.md`（必要なら新規）

## Affected band

- [x] GCC15+
- [x] GCC13-14
- [x] GCC9-12
- [x] Cross-cutting

## Processing path

- [x] DualSinkStructured
- [x] SingleSinkStructured
- [x] NativeTextCapture
- [x] Passthrough
- [x] Cross-path

## Allowed files

- `README.md`
- `docs/specs/rendering-ux-contract-spec.md`
- `docs/specs/public-machine-readable-diagnostic-surface-spec.md`
- 新規 ADR（必要時）
- `config/cc-formed.example.toml`（例示だけ先に足すなら可）

## Forbidden surfaces

- `diag_render/*`
- `diag_cli_front/*`
- corpus snapshots
- rulepack semantics

## Detailed scope

1. rendering spec を Subject-first grammar 前提へ書き換える  
2. `expanded groups = 1` を unconditional な前提にしない  
3. `visible root overflow -> summary-only` を built-in default から外す文言へ更新  
4. `why:` を high-confidence core family から外し、generic / low-confidence fallback に再定義  
5. CI と interactive default の責務分離を文書化  
6. public JSON spec には「presentation customization しても scrape 不要」の観点を追記  
7. README の before/after 例は最終仕様に合わせた placeholder を用意する

## Acceptance criteria

- [ ] rendering spec に canonical header grammar が明記されている
- [ ] multiple error session model が visible roots の block 反復として記述されている
- [ ] display family と internal family の違いが明記されている
- [ ] presentation config の存在と fail-open が spec で定義されている
- [ ] public JSON spec に terminal scraping 非依存の原則が明示されている
- [ ] docs 全体で `error: [family] subject` の表記が一貫している
- [ ] code behavior は一切変更しない

## Docs impact

- [ ] None
- [x] README / SUPPORT-BOUNDARY
- [x] Spec / ADR
- [ ] Template / workflow

## Stop conditions

- [ ] render code の user-visible change をこの issue に混ぜ始めたら停止
- [ ] README と spec の語彙が食い違ったら停止
- [ ] CI / default の両方を同じ first-line 契約で固定しようとしたら停止

## Reviewer evidence

- [ ] Docs diff rationale
- [ ] Before / after examples
- [ ] Explicit note that code behavior is unchanged

## If blocked

current-authority doc の rewrite が重い場合は、まず ADR / design doc を landing し、その次の issue で spec へ昇格してよい。  
ただし「コード先行」は不可。

## Do not do

- free-form template language を先に正本化しない
- `help` を即 `fix` に固定しない
- summary-only の旧 mental model を新 default として温存しない

## PR body must include

- Goal
- Why now
- Parent epic / ADR
- Contract change summary
- Render behavior unchanged の明記
- Docs diff rationale

---

# Issue 02 — Presentation config schema, built-in preset assets, and fail-open loader

- Workstream: Tooling / Render
- Band: Cross-cutting
- Processing Path: Cross-path
- Support Level: Beta
- Layer: Templates / Packaging
- Issue Kind: Work Package
- Task Size: M
- Risk: Medium
- Contract Change: CLI / Schema
- Agent Ready: Draft
- Night Batch: Later
- Human Review Type: Deep
- Stop-Ship: Yes
- Owner Layer: Shared
- Depends On: Issue 01
- Commands: `cargo test --workspace`, `cargo xtask check`
- Acceptance ID: `PV2-02-CONFIG`

## Goal

presentation customization のための config surface を追加する。  
ここで入れるのは **schema / asset / loader / merge / fail-open** までであり、subject-block behavior 自体の default promotion はまだ行わない。

## Why now

「コード改変なしで書式を差し替えたい」という要求は本件の主要要件である。  
あとから config を足そうとすると、template engine や family mapping が hard-coded 前提で広がってしまう。  
したがって config surface は early に固定した方が、後続の実装がきれいになる。

## Parent epic / ADR

- Parent epic: Presentation V2
- Depends on: Issue 01 contract acceptance

## Affected band

- [x] GCC15+
- [x] GCC13-14
- [x] GCC9-12
- [x] Cross-cutting

## Processing path

- [x] Cross-path

## Allowed files

- `diag_cli_front/src/config.rs`
- `diag_cli_front/src/args.rs`
- `diag_cli_front/src/render.rs`
- `config/cc-formed.example.toml`
- 新規 `config/presentation/subject_blocks_v1.toml`
- 新規 `config/presentation/legacy_v1.toml`
- 必要な unit tests

## Forbidden surfaces

- `diag_render/src/selector.rs`
- `diag_render/src/layout.rs`
- corpus snapshots
- family heuristics
- README overhaul（最小限の example 更新は可）

## Detailed scope

1. main config に `render.presentation` / `render.presentation_file` を追加  
2. external presentation file の TOML schema を定義  
3. built-in preset asset を checked-in file として追加  
4. built-in -> admin -> user -> CLI の precedence を実装  
5. invalid presentation config は fail-open で built-in default へ戻す  
6. unknown slot / template / mapping の validation warning path を整える  
7. `legacy_v1` と `subject_blocks_v1` の 2 preset を読み込めるようにする  
8. schema versioning と `kind = "cc_formed_presentation"` を固定する

## Acceptance criteria

- [ ] `render.presentation` が built-in preset id を取れる
- [ ] `render.presentation_file` が外部 TOML を読める
- [ ] built-in preset asset を file として repo に持つ
- [ ] overlay merge が deterministic である
- [ ] invalid external presentation file でも wrapper 全体は fail-open で動作する
- [ ] unknown template / slot が panic ではなく fallback + warning になる
- [ ] existing config without presentation keys が従来どおり動く

## Reviewer evidence

- [ ] Config parse tests
- [ ] Merge precedence tests
- [ ] Invalid-file fallback tests
- [ ] Example TOML reviewed by human and AI both読みやすいことの説明

## Docs impact

- [ ] None
- [ ] README / SUPPORT-BOUNDARY
- [x] Spec / ADR
- [x] Template / workflow

## Stop conditions

- [ ] template engine behavior changeまで同時に入れ始めたら停止
- [ ] `render.presentation` と `cascade` の責務境界が曖昧なままマージしない
- [ ] hard-coded built-in strings だけで preset を実装し始めたら停止

## If blocked

loader のみ先に入れ、preset file の schema validation を別小 issue へ切ってもよい。  
ただし `presentation_file` と built-in preset の 2 層構造は守る。

## Do not do

- free-form format string だけの DSL にしない
- invalid config を fatal error にしない
- `legacy_v1` を用意せず新 preset だけにしない

## PR body must include

- new config keys
- precedence summary
- fail-open policy summary
- sample preset snippet
- existing config compatibility note

---

# Issue 03 — Semantic slot layer and resolved presentation policy skeleton

- Workstream: Render
- Band: Cross-cutting
- Processing Path: Cross-path
- Support Level: Beta
- Layer: ViewModel / Templates
- Issue Kind: Work Package
- Task Size: M
- Risk: Medium
- Contract Change: None
- Agent Ready: Draft
- Night Batch: Later
- Human Review Type: Deep
- Stop-Ship: No
- Owner Layer: Shared
- Depends On: Issue 02
- Commands: `cargo test -p diag_render`, `cargo test --workspace`, `cargo xtask check`
- Acceptance ID: `PV2-03-SLOTS`

## Goal

`diag_render` に semantic slot layer と resolved presentation policy の骨格を導入する。  
この issue の目的は **architecture first** であり、最終見た目の大変更ではない。  
可能な限り legacy output を保ったまま、新 engine を裏に差し込める状態を作る。

## Why now

layout だけを直接書き換えると、`title`, `raw_message`, `context_lines` の flat model に新しい concept をねじ込むことになり、再び prose-heavy な bridge を作る。  
slot layer を先に入れておけば、その後の family renderer は slot producer になるだけで済み、template engine と config customization も自然に乗る。

## Parent epic / ADR

- Parent epic: Presentation V2
- Depends on: Issue 01, Issue 02

## Affected band

- [x] Cross-cutting

## Processing path

- [x] Cross-path

## Allowed files

- `diag_render/src/view_model.rs`
- 新規 `diag_render/src/presentation.rs` または `diag_render/src/presentation/*`
- `diag_render/src/layout.rs`（adapter 追加のみ）
- tests under `diag_render`

## Forbidden surfaces

- selector/cascade semantics
- corpus snapshots
- CLI config
- family heuristics beyond placeholder extraction

## Detailed scope

1. `RenderSemanticCard` 相当の内部 struct を追加  
2. slot catalog enum / stable ids を追加  
3. resolved presentation policy（template id / display family / label catalog / location policy）を表す型を追加  
4. legacy `RenderGroupCard` への adapter または dual path を作る  
5. no-op / generic mapping で既存 rendering を保つ  
6. optional internal debug artifact の型だけ先に定義してよい

## Acceptance criteria

- [ ] semantic slot layer がコンパイルされる
- [ ] legacy preset では既存 output と大きな差が出ない
- [ ] slot catalog が stable id で定義される
- [ ] resolved presentation policy を config layer から受け取れる
- [ ] template engine がまだ未完成でも generic path で動く
- [ ] no panic on missing slots / missing template data

## Reviewer evidence

- [ ] New type graph / module graph explanation
- [ ] Unit tests for slot absence / fallback path
- [ ] Legacy output non-regression evidence on small representative fixtures

## Docs impact

- [ ] None
- [ ] README / SUPPORT-BOUNDARY
- [x] Spec / ADR
- [ ] Template / workflow

## Stop conditions

- [ ] contrast_block など具体 family behavior まで同時に作り始めたら停止
- [ ] selector/root visibility semantics に手を出し始めたら停止
- [ ] external config loader 変更まで抱え込み始めたら停止

## If blocked

まず type aliases と adapter だけ landing してよい。  
ただし slot ids と resolved presentation policy の概念は同じ change で入れる。

## Do not do

- final text format をこの issue だけで決め打ちしない
- summary_only_groups の behavior 変更をここでやらない
- config parsing logic を `diag_render` 側へ複製しない

## PR body must include

- new internal model summary
- why layout-only patch ではないのか
- legacy behavior compatibility note
- follow-up issues dependency list

---

# Issue 04 — Multi-root block emission and budget rewrite for failure runs

- Workstream: Render
- Band: Cross-cutting
- Processing Path: Cross-path
- Support Level: Beta
- Layer: ViewModel / Theme
- Issue Kind: Work Package
- Task Size: M
- Risk: High
- Contract Change: UX
- Agent Ready: No
- Night Batch: None
- Human Review Type: Design
- Stop-Ship: Yes
- Owner Layer: Shared
- Depends On: Issue 03
- Commands: `cargo test --workspace`, `cargo xtask replay --root corpus`, `cargo xtask check`
- Acceptance ID: `PV2-04-MULTIROOT`

## Goal

compile / link failure run において、visible error/fatal root を summary-only に落とさず、subject-block をそのまま縦に並べる session model を導入する。  
同時に budget の重心を session-global から block-local へ移す。

## Why now

Presentation V2 の肝は、1件 block の mental model を複数 error 時にも壊さないことである。  
ここを直さない限り、どれだけ block を美しくしても 2 件目以降は `other errors:` に落ちてしまい、UX の根本課題は残る。

## Parent epic / ADR

- Parent epic: Presentation V2
- Depends on: Issue 01, Issue 03
- Strongly related: Issue 02

## Allowed files

- `diag_render/src/selector.rs`
- `diag_render/src/budget.rs`
- `diag_render/src/view_model.rs`
- `diag_render/src/formatter.rs`
- `diag_render/src/layout.rs`
- tests / fixtures needed for render session behavior

## Forbidden surfaces

- family-specific slot extraction beyond generic block
- CLI parsing except minimal config hookup already defined
- README marketing rewrite
- public JSON schema changes

## Detailed scope

1. failure run では `all_visible_blocks` を built-in subject-block preset の既定にする  
2. visible error/fatal roots は full block として出す  
3. dependent / duplicate / follow-on は cascade の結果に従って非blockのまま  
4. `summary_only_groups` は legacy preset か warning-only path に限定する  
5. `max_expanded_independent_roots` の責務を presentation 側へ寄せる  
6. budget を per-block local へ寄せる  
7. block 間 separator と session tail notice の規則を入れる

## Acceptance criteria

- [ ] visible error/fatal roots が 3 件あれば 3 block 出る
- [ ] cascade-hidden dependent nodes は block 化されない
- [ ] parent block 内の omitted dependent count は suffix / note として残せる
- [ ] warning suppression note は session tail に残る
- [ ] legacy preset では旧 lead-plus-summary behavior を保持できる
- [ ] compile/link failure run に対し `expanded_groups = 1` 前提の regressions がない

## Reviewer evidence

- [ ] Representative multi-error fixture snapshots
- [ ] Explanation of how visible roots are ordered
- [ ] Legacy preset vs subject_blocks_v1 diff
- [ ] Budget rationale for default / concise / ci

## Docs impact

- [ ] None
- [ ] README / SUPPORT-BOUNDARY
- [x] Spec / ADR
- [ ] Template / workflow

## Stop conditions

- [ ] rulepack family extraction まで抱え込んだら停止
- [ ] warning-only policy まで完全 redesign しようとしたら停止
- [ ] legacy preset compatibility を失ったら停止

## If blocked

failure run だけ `all_visible_blocks` を先に入れ、warning-only run は legacy のままでもよい。  
ただし visible failure roots を summary-only に落とす挙動は subject-block preset では残さない。

## Do not do

- dependent nodes を全部 flat に並べない
- `other errors:` bucket を subject_blocks_v1 の built-in default に残さない
- `cascade` と `presentation` の責務を再び混ぜない

## PR body must include

- old vs new session model
- why visible roots are blocks
- cascade-hidden handling summary
- budget rewrite summary
- representative multi-error snapshot diff

---

# Issue 05 — Family slot extraction I: contrast families and linker families

> **並列実行**: Issue 05 と Issue 06 は互いに独立しており、Issue 03 + 04 完了後に並列で着手できる。

- Workstream: Render / Analysis
- Band: Cross-cutting
- Processing Path: Cross-path
- Support Level: Beta
- Layer: Templates / ViewModel
- Issue Kind: Work Package
- Task Size: M
- Risk: Medium
- Contract Change: UX
- Agent Ready: Draft
- Night Batch: Later
- Human Review Type: Deep
- Stop-Ship: Yes
- Owner Layer: Shared
- Depends On: Issue 03, Issue 04
- Commands: `cargo test -p diag_render`, `cargo xtask replay --root corpus`, `cargo xtask check`
- Acceptance ID: `PV2-05-CONTRAST-LINKER`

## Goal

`contrast_block` と `linker_block` を最初の本格 family coverage として実装する。  
対象は次を主に想定する。

- `type_overload`
- `concepts_constraints`
- `format_string`
- `conversion_narrowing`
- `const_qualifier`
- `linker.*`

## Why now

type mismatch / linker は価値が最も分かりやすく、corpuses でも存在感が強い。  
ここを先行実装すれば、Presentation V2 の方向性をレビューしやすく、残り family への一般化も容易になる。

## Allowed files

- `diag_render/src/family.rs`
- `diag_render/src/view_model.rs`
- `diag_render/src/presentation/*`
- relevant tests / snapshots
- built-in preset asset updates if needed

## Forbidden surfaces

- syntax / lookup / context family
- selector semantics
- CLI config semantics
- README final polish

## Detailed scope

1. internal family -> display family mapping を contrast/linker 分だけ追加  
2. `expected`, `actual`, `via`, `symbol`, `from`, `archive` slot extraction を実装  
3. `+N notes`, `+N candidates`, `+N refs` の suffix compaction を定義  
4. `why:` を contrast/linker core から外す  
5. low-confidence / extraction failure 時は `generic_block` へ fallback  
6. type display は compact-safe を優先し、衝突時だけ expand する

## Acceptance criteria

- [ ] type mismatch 系で `want / got / via` が core に出る
- [ ] linker 系で `symbol / from` が core に出る
- [ ] candidate flood / ref flood が suffix で圧縮される
- [ ] extraction failure 時に invented fact を出さず generic fallback する
- [ ] raw provenance は必要時 reachable のまま
- [ ] corpus representative cases で旧表示より短く、かつ意味が明確である

## Reviewer evidence

- [ ] Before / after snapshots for template mismatch cases
- [ ] Before / after snapshots for linker undefined reference cases
- [ ] Fallback behavior examples
- [ ] Type shortening policy explanation

## Docs impact

- [ ] None
- [ ] README / SUPPORT-BOUNDARY
- [x] Spec / ADR
- [ ] Template / workflow

## Stop conditions

- [ ] syntax family まで抱え込んだら停止
- [ ] raw regex only で `expected/actual` を無理に発明し始めたら停止
- [ ] candidate flood compaction が facts を隠しすぎるなら停止

## If blocked

contrast family と linker family を別 issue に分割してよい。  
ただし両方とも Presentation V2 の代表例なので、長く離さない方がよい。

## Do not do

- `why:` を消す代わりに何も evidence を出さない
- `want/got` を曖昧な raw sentence で埋める
- linker で source excerpt を無理に primary にしない

## PR body must include

- targeted families list
- slot extraction rules
- fallback rules
- representative corpus diff
- unresolved corner cases

---

# Issue 06 — Family slot extraction II: syntax, lookup, conflict, and context families

> **並列実行**: Issue 06 と Issue 05 は互いに独立しており、Issue 03 + 04 完了後に並列で着手できる。

- Workstream: Render / Analysis
- Band: Cross-cutting
- Processing Path: Cross-path
- Support Level: Beta
- Layer: Templates / ViewModel
- Issue Kind: Work Package
- Task Size: M
- Risk: Medium
- Contract Change: UX
- Agent Ready: Draft
- Night Batch: Later
- Human Review Type: Deep
- Stop-Ship: Yes
- Owner Layer: Shared
- Depends On: Issue 03, Issue 04
- Commands: `cargo test -p diag_render`, `cargo xtask replay --root corpus`, `cargo xtask check`
- Acceptance ID: `PV2-06-SYNTAX-LOOKUP-CONTEXT`

## Goal

次の family 群に対して、Presentation V2 の slot-based block を実装する。

- syntax / parser expectation
- missing header
- missing name / incomplete type / unavailable API
- redefinition / duplicate symbol
- macro/include context

## Why now

contrast/linker だけでは Presentation V2 が「型エラー専用」になってしまう。  
ユーザーが特に気にしている include 系・syntax 系・lookup 系まで揃って初めて、北極星の文法が本当に一般化できるか検証できる。

## Allowed files

- `diag_render/src/family.rs`
- `diag_render/src/excerpt.rs`
- `diag_render/src/presentation/*`
- relevant tests / snapshots
- built-in preset asset updates if needed

## Forbidden surfaces

- selector/cascade semantics
- CLI / config semantics
- default promotion docs
- public JSON schema

## Detailed scope

1. `parser_block` の `want / near` を実装  
2. missing header を direct include failure として `need / from` へ出す  
3. `scope_declaration` -> `missing_name`、`pointer_reference` -> `incomplete_type` など lookup family を整える  
4. redefinition family で `now / prev` を出す  
5. macro/include context で `in / via / from` を短く出す  
6. include を A/B/C 類型（missing header / include context / declaration visibility failure）で混ぜない

## Acceptance criteria

- [ ] syntax family で `want / near` と excerpt が協調する
- [ ] direct missing header が `need / from` block になる
- [ ] missing include の二次影響を headline にしていない
- [ ] redefinition family が `now / prev` 比較で出る
- [ ] macro/include context が core context line へ圧縮される
- [ ] include 3 類型の整理が docs / code / tests で一貫している

## Reviewer evidence

- [ ] Parser error before/after snapshots
- [ ] Missing header before/after snapshots
- [ ] Scope/incomplete type before/after snapshots
- [ ] Redefinition and macro/include examples

## Docs impact

- [ ] None
- [ ] README / SUPPORT-BOUNDARY
- [x] Spec / ADR
- [ ] Template / workflow

## Stop conditions

- [ ] include 類型を 1 family に雑にまとめ始めたら停止
- [ ] missing declaration と missing header を同一 headline にしたら停止
- [ ] syntax family で excerpt が見にくくなるなら停止

## If blocked

lookup family を `missing_name` / `incomplete_type` へ分割する issue をさらに分けてもよい。  
ただし include 3 類型の整理は同じ design note に残すこと。

## Do not do

- `scope_declaration` をそのまま display tag にしない
- macro/include chain を raw note 群として全面展開しない
- redefinition family で `why:` 文を長く戻さない

## PR body must include

- targeted family list
- display family mapping summary
- include taxonomy note
- representative corpus diffs
- unresolved ambiguities

---

# Issue 07 — Inline location placement, label alignment, and excerpt windowing

- Workstream: Render
- Band: Cross-cutting
- Processing Path: Cross-path
- Support Level: Beta
- Layer: Theme / Templates
- Issue Kind: Work Package
- Task Size: S
- Risk: Medium
- Contract Change: UX
- Agent Ready: Draft
- Night Batch: Later
- Human Review Type: Deep
- Stop-Ship: Yes
- Owner Layer: Shared
- Depends On: Issue 03, Issue 05, Issue 06
- Commands: `cargo test -p diag_render`, `cargo xtask replay --root corpus`, `cargo xtask check`
- Acceptance ID: `PV2-07-LOCATION-EXCERPT`

## Goal

Presentation V2 の視認性を仕上げる。  
具体的には、

- dedicated location line の削減
- inline suffix host selection
- label width alignment
- excerpt windowing

を実装する。

## Why now

Subject-first header が良くても、location の出し方と long-line excerpt が雑だと、見た目全体の密度がすぐ悪化する。  
この issue は「短いだけでなく視覚的に理解しやすい」を支える finishing work である。

## Allowed files

- `diag_render/src/layout.rs`
- `diag_render/src/excerpt.rs`
- `diag_render/src/presentation/*`
- tests / snapshots

## Forbidden surfaces

- selector semantics
- family mapping logic
- config schema changes（小さな補助 field 追加は要相談）
- README large rewrite

## Detailed scope

1. default subject-block preset で dedicated `-->` line を原則無くす  
2. header / evidence / excerpt header の順で location host を選ぶ  
3. label width default = 4 を実装  
4. `got :`, `via :`, `use :`, `in  :` などの縦 alignment を作る  
5. long-line excerpt の highlight-centered windowing を実装  
6. caret alignment が壊れるケースでは safe degradation を行う  
7. CI preset では path-first へ切り替えられる余地を残す

## Acceptance criteria

- [ ] subject-block preset の default では dedicated location line が core から消える
- [ ] location が header suffix / evidence suffix / excerpt header のいずれかに必ず残る
- [ ] label alignment が代表例で安定している
- [ ] long source line が `...` 付き windowing される
- [ ] caret が壊れるケースで unsafe annotation を出さない
- [ ] CI / legacy preset 互換を保てる

## Reviewer evidence

- [ ] Before / after screenshots or snapshots for long-line syntax errors
- [ ] Examples of header suffix vs evidence suffix placement
- [ ] Alignment examples for mixed labels
- [ ] CI preset compatibility note

## Docs impact

- [ ] None
- [ ] README / SUPPORT-BOUNDARY
- [x] Spec / ADR
- [ ] Template / workflow

## Stop conditions

- [ ] path policy そのものの semantics を変え始めたら停止
- [ ] multi-column / Unicode heavy styling に寄り始めたら停止
- [ ] location が失われるケースを knowingly 残すなら停止

## If blocked

location placement と excerpt windowing を 2 PR に分けてもよい。  
ただし label alignment と inline suffix policy は同じ見た目変更なので、できれば同一セットでレビューしたい。

## Do not do

- `at:` line を default で常用しない
- line が長いからといって location を silently drop しない
- excerpt で全文表示に戻らない

## PR body must include

- placement algorithm summary
- alignment rule summary
- windowing examples
- known fallback cases

---

# Issue 08 — Snapshot, corpus, internal presentation artifact, and opt-in preset validation

- Workstream: Quality / Render
- Band: Cross-cutting
- Processing Path: Cross-path
- Support Level: Beta
- Layer: Quality / Templates
- Issue Kind: Work Package
- Task Size: M
- Risk: Medium
- Contract Change: Docs / UX
- Agent Ready: Draft
- Night Batch: Later
- Human Review Type: Deep
- Stop-Ship: Yes
- Owner Layer: Shared
- Depends On: Issue 04, Issue 05, Issue 06, Issue 07
- Commands: `cargo xtask replay --root corpus`, `cargo xtask check`, `cargo test --workspace`
- Acceptance ID: `PV2-08-QUALITY`

## Goal

subject-block preset を opt-in として検証可能な状態へ仕上げる。  
具体的には、

- corpus replay
- snapshot review
- optional internal presentation artifact
- example config / docs update

をまとめて行う。

## Why now

Presentation V2 は見た目の変更である以上、代表ケースを corpus 上で比較しないと良し悪しが判断できない。  
また customization が入ると terminal text の差分だけでは原因追跡しづらくなるため、internal `render.presentation.json` のような artifact が役に立つ。

## Allowed files

- corpus snapshots
- `diag_render` tests
- optional internal presentation artifact implementation
- `config/cc-formed.example.toml`
- `README.md`
- related docs

## Forbidden surfaces

- large new family behavior
- config schema redesign
- default promotion itself

## Detailed scope

1. `subject_blocks_v1` を opt-in preset として corpus replay 可能にする  
2. representative snapshot cluster をレビューしやすく整理する  
3. optional internal `render.presentation.json` を追加する（public ではない）  
4. example config に `presentation = "subject_blocks_v1"` を載せる  
5. README に before/after を少数更新する  
6. docs に opt-in であることを明記する

## Acceptance criteria

- [ ] `subject_blocks_v1` で corpus replay が通る
- [ ] representative fixtures の snapshot diff が人間レビュー可能な粒度に収まる
- [ ] internal presentation artifact が必要なデバッグ情報を持つ
- [ ] public JSON surface は unchanged である
- [ ] example config から新 preset を試せる
- [ ] README / docs が opt-in status を誤解なく伝える

## Reviewer evidence

- [ ] Snapshot diff rationale
- [ ] Corpus case selection rationale
- [ ] Example config diff
- [ ] Internal artifact sample（if implemented）
- [ ] Explicit statement that public JSON was not changed

## Docs impact

- [ ] None
- [x] README / SUPPORT-BOUNDARY
- [x] Spec / ADR
- [ ] Template / workflow

## Stop conditions

- [ ] default promotionまで一緒に進め始めたら停止
- [ ] public JSONに display family / template id を混ぜ始めたら停止
- [ ] corpus 全件を一気に更新して review 不能にしたら停止

## If blocked

internal presentation artifact は別 issue へ切ってもよい。  
ただし snapshot review のための可観測性は何らかの形で確保すること。

## Do not do

- opt-in なのに README で default と誤読させない
- public JSON をデバッグ都合で汚さない
- snapshot diff を「見れば分かる」で済ませない

## PR body must include

- how to enable the preset
- corpus commands run
- why selected cases are representative
- public JSON unchanged note
- snapshot review summary

---

# Issue 09 — Default promotion, legacy compatibility, and deprecation cleanup

- Workstream: Render / Docs / Release
- Band: Cross-cutting
- Processing Path: Cross-path
- Support Level: Beta
- Layer: Templates / Packaging
- Issue Kind: Work Package
- Task Size: M
- Risk: High
- Contract Change: UX / CLI / Docs
- Agent Ready: No
- Night Batch: None
- Human Review Type: Design
- Stop-Ship: Yes
- Owner Layer: Maintainer
- Depends On: Issue 08
- Commands: `cargo xtask replay --root corpus`, `cargo xtask check`, `cargo test --workspace`
- Acceptance ID: `PV2-09-DEFAULT-PROMOTION`

## Goal

subject-block preset を built-in default へ昇格し、legacy preset を後方互換オプションとして残しつつ、責務のズレた旧 field を deprecate する。

## Why now

opt-in のままでは実験はできても product direction にはならない。  
一方、default promotion は repo 全体の第一印象を変えるため、十分な corpus review と docs 整備の後で行う必要がある。  
そのため、この issue は最後に置く。

## Allowed files

- built-in preset selection logic
- `config/cc-formed.example.toml`
- `README.md`
- `docs/specs/rendering-ux-contract-spec.md`
- `diag_cli_front/src/config.rs`（deprecation wording）
- minimal related tests / snapshots

## Forbidden surfaces

- large new family implementations
- ingestion / IR changes
- public JSON changes

## Detailed scope

1. no-config default preset を `subject_blocks_v1` に切り替える  
2. `legacy_v1` を選べるよう残す  
3. `cascade.max_expanded_independent_roots` を root-cap meaning から deprecate し、presentation policy へ案内する  
4. docs / example config / README を新既定へ更新  
5. migration note と rollback path を書く  
6. default promotion の acceptance gate を docs に記録する

## Acceptance criteria

- [ ] no-config default が subject-block presentation になる
- [ ] `legacy_v1` が明示選択で使える
- [ ] deprecation wording が明確で silent behavior change ではない
- [ ] docs / example config / README が new default を一貫して説明する
- [ ] corpus representative cases で non-regression がレビュー済み
- [ ] rollback path が docs で明示される

## Reviewer evidence

- [ ] Default-vs-legacy comparison
- [ ] Migration / rollback note
- [ ] Updated example config
- [ ] Corpus rationale that justified promotion

## Docs impact

- [ ] None
- [x] README / SUPPORT-BOUNDARY
- [x] Spec / ADR
- [x] Template / workflow

## Stop conditions

- [ ] opt-in validationが不十分なまま default へ上げない
- [ ] legacy rollback path を用意せず promotion しない
- [ ] deprecation note がなく既存 config の意味を silently 変えない

## If blocked

promotion を延期し、`subject_blocks_v1` を beta opt-in のままにする。  
無理に default 化しないこと。

## Do not do

- legacy preset をいきなり削除しない
- `max_expanded_independent_roots` の意味を無言で変えない
- example config と docs を古いまま残さない

## PR body must include

- default change summary
- migration / rollback instructions
- legacy compatibility note
- deprecation wording
- corpus evidence for promotion

---

## 26. 推奨 milestone への割り当て

- `M1 Architecture Skeleton`
  - Issue 01
  - Issue 02
  - Issue 03

- `M3 Native-Parity Renderer`
  - Issue 04
  - Issue 05 ← **05 と 06 は並列実行可能**
  - Issue 06 ← **05 と 06 は並列実行可能**
  - Issue 07

- `M5 Quality Gate & Corpus`
  - Issue 08

- `M4 Noise Compaction & Ownership` と `M5` の間で判断
  - Issue 09

---

## 27. 最後に

本提案の本質は、「コンパイルエラーをもっと装飾する」ことではない。  
本質は、**ユーザーが直す順に、比較しやすい slot で情報を並べること**である。

したがって設計判断の優先順位は次になる。

1. Subject-first
2. First action visible
3. Family-specific evidence
4. Visible root = block
5. Configurable without code edits
6. Fail-open and public JSON independence

この優先順位を守れば、step-by-step で実装しても橋の両側がずれにくい。  
逆に、template を場当たりで足し始めると、すぐに整合性が崩れる。  
その意味で、本設計書は「最小パッチの前に置く橋脚の設計図」である。
