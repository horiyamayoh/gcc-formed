---
doc_role: design-authority
lifecycle_status: accepted-target
audience: both
use_for: Final decision record and implementation plan for compiler error presentation.
do_not_use_for: Historical provenance or low-level code archaeology.
supersedes:
  - gcc-formed-presentation-v2-design.md
  - gcc-formed-presentation-vnext-brushed.md
superseded_by: []
---

# gcc-formed 最終決定版設計書
## subject_blocks_v2: Subject-first / Visible-root / Config-driven Presentation

- 日付: 2026-04-13
- 状態: Accepted target
- 対象: `gcc-formed` の terminal / CI 向けコンパイル診断表示
- 主眼: 「最高のコンパイル体験」を、人間向け表示として最短修正時間に最適化する

---

## 1. 要旨

本設計は、`gcc-formed` における今後の標準的なコンパイルエラー表示を、**subject-first の block 表示**として最終決定するものである。

最終形の基本体験は、次の 3 点で定義される。

1. **先頭は Subject** で始まる。人間が最初に読むのは `src/main.cpp:5:12` ではなく、`error: [type_mismatch] arguments do not match` のような「何の失敗か」である。
2. **1 回のコンパイルで見えている本質的に別件のエラーは、すべて block として出す。** 1件直すごとに次の 1件だけが見える、という体験を既定値にしない。
3. **書式は可能な限り設定ファイルで変えられる。** Rust コードを直さなくても、family ごとに template を差し替え、header 書式や evidence 行の順番を変えられる。

本設計は現行 `subject_blocks_v1` の方向性を否定しない。むしろ、その方向を**最終形として矛盾なく閉じる**。ただし、次の点では現行方針を意図的に進化させる。

- header 書式を `error: [family] subject` に固定する
- compile failure 時の visible root は **summary-only に落とさない**ことを既定契約として明文化する
- semantic facts の抽出責務を template 名から切り離し、**`semantic_shape`** によって決める
- presentation 設定ファイルを、**人間と AI のどちらにも編集しやすい TOML** として強化する

この設計の完成形は、built-in preset **`subject_blocks_v2`** として実装される。

### 1.1 主たる機能一覧

最終アプリの主要機能は、先に次の表で把握できる。

| 機能 | 何をするか | compile failure 時の既定挙動 | 設定で変えられる範囲 |
|---|---|---|---|
| visible-root selection | raw diagnostic 群から独立した correction target を選ぶ | visible root を全件 block 化する | session mode / legacy preset / emergency degradation |
| subject-first rendering | 人間が最初に読む Subject を先頭へ置く | `error: [family] subject` を使う | header format / location policy / label policy |
| family-aware evidence | family ごとに最小限の証拠を先頭へ並べる | `want/got/via` や `name/use/need` などを使う | template / display family / label catalog |
| cascade compression | 同じ話の増殖を block 数に反映させない | dependent / duplicate / follow-on を block 内へ圧縮する | compression level / omission notice policy |
| warning handling | failure run の warning を主視界からどかす | warning は tail summary 化してよい | profile / warning policy |
| fail-open fallback | structured 化が不誠実なとき raw を残す | low confidence では generic block または raw fallback | profile / user opt-out / fallback policy |
| presentation config | no-code で書式を変える | built-in `subject_blocks_v2` を使う | external TOML preset / overlay / family-template mapping |

### 1.2 この文書で固定するもの

本設計書は次を固定する。

- built-in default としての block grammar
- compile failure 時の visible-root contract
- family / semantic shape / template の責務分離
- external presentation config による no-code customization 方針
- 実装 issue に落とし込める粒度の migration / acceptance criteria


---

## 2. 背景

### 2.1 解きたい問題

GCC の生出力は事実としては豊富だが、人間が最短で修正行動に移るための並びになっていないことが多い。特に次がつらい。

- 先に location や note flood が見え、本質的な subject が埋もれる
- `why:` 的な文章が長く、差分そのものより prose が前に出る
- overload candidate / template instantiation / include chain / repeated linker lines が、そのまま並ぶ
- 1回のコンパイルで複数の独立エラーがあるのに、1件だけ強く見え、残りが summary に沈みやすい設計だと不信感が出る
- 書式調整のたびにコードを触る設計だと、業務でのカスタマイズコストが高い

### 2.2 現行 repo の到達点

現行 `main` はすでに、かなり重要な前進を果たしている。

- README は `subject_blocks_v1` を beta runtime default と位置づけ、subject-first blocks を既定の価値としている。
- current-authority の rendering spec は、failure run の session mode を `all_visible_blocks` とし、visible root を built-in default で summary-only に落としてはならない、という方向を明記している。
- 実装にも presentation preset / external presentation file / template / family mapping / location policy の枠組みがある。

ただし、まだ「橋の中央」が完全には噛み合っていない。

- selector には legacy / compatibility / warning-only 由来の summary path が残る
- view model では family facts の抽出がまだ `template_id` に引っ張られており、本当の意味で no-code customization になり切っていない
- header の punctuation や evidence label の揃え方は、一部がまだ renderer 実装に埋まっている
- `subject_first_header` の有無が現状は preset 名 (`subject_blocks_v1`) に結びついており、schema だけでは subject-first を完全に切り替え切れない

本設計は、この残りのズレを解消する。

### 2.3 本設計が参照した current-authority / 実装面

本設計は、作成時点の `main` を軽く流し読みしたうえで、次の current-authority / 実装面を根拠としている。

- `README.md`
- `docs/specs/rendering-ux-contract-spec.md`
- `config/presentation/subject_blocks_v1.toml`
- `diag_cli_front/src/config.rs`
- `diag_render/src/presentation.rs`
- `diag_render/src/view_model.rs`
- `diag_render/src/layout.rs`
- `diag_render/src/selector.rs`

設計判断は、これらのうち **README / rendering spec を上位 authority** とし、実装は「どこにズレが残っているか」を確認する材料として扱う。

---

## 3. 本設計の最終判断

### 3.1 最上位の判断

今後の `gcc-formed` は、**「1 compiler invocation を、独立した correction target の block 列として提示するアプリ」** として整理する。

ここでいう correction target とは、単なる raw diagnostic 1行ではなく、ユーザーが「この話を 1つ直す」と認識できる独立した修正対象である。

### 3.2 既定表示の核

compile failure 時の既定値は次で固定する。

- local / pipe / default: subject-first blocks
- compile failure: all visible roots as blocks
- warning on failure: summary 化してよい
- cascade / follow-on / duplicate: 親 block に吸収してよい
- low confidence / unsupported: fail-open で raw に戻る

### 3.3 subject_blocks_v2 built-in default header

正規形は次とする。

```text
error: [family] subject
```

具体例:

```text
error: [type_mismatch] arguments do not match
```

これは **`subject_blocks_v2` built-in default の規格**である。external presentation config が別の header grammar を定義すること自体は許容するが、no-config / built-in default の正規形はこれで固定する。

`error[type_mismatch]:` は採用しない。理由は 2 つある。

1. `error:` / `warning:` / `note:` のコロン位置が縦に揃い、視線の走査が安定する
2. severity と family の役割が視覚的に分離され、header が読みやすい

### 3.4 location の原則

interactive default では、location は subject の従属情報であり、Subject を押しのけない。

既定の placement は次とする。

1. inline suffix: ` @ src/main.cpp:5:12`
2. evidence suffix
3. excerpt header
4. none

dedicated な `at:` 行や `--> path:line:col` 行は、verbose/debug、legacy preset、または inline placement が不誠実な場合に限って使う。

### 3.5 複数エラー時の原則

**1回のコンパイルで visible root と判断されたものは、すべて block として出す。**

この原則は、「single lead を 1件だけ強く見せて残りを summary に沈める」モデルを既定値にしない、という意味である。  
本設計の基本単位はあくまで **visible-root block** であり、複数 root 時はその block が順に並ぶ。

これは次を意味する。

- block 数は raw diagnostic 数ではなく、**独立した correction target 数**で決まる
- candidate flood / template flood / include chain / duplicate linker lines は block 数を増やさない
- warnings は compile failure 時に summary 化されうるが、error root の visibility を奪わない
- compiler 自体がそこで処理を停止した場合のみ、見えている root が 1件で終わる

### 3.6 カスタマイズの原則

今後の presentation customization は、**原則として設定ファイルで完結できる**ようにする。

そのために、次を分離する。

- `display_family`: header に出す人間向け family 名
- `semantic_shape`: どの semantic facts を抽出するか
- `template`: 抽出済み facts をどう並べるか

この 3 つを分けることで、**family を変えずに template を差し替える**、あるいは **template を複数作って family ごとに割り当てる**、という操作をコード改変なしで行える。

---

## 4. 目標と非目標

### 4.1 目標

本設計の目標は次である。

- コンパイルエラーを、最短で修正行動に移れる順序で見せる
- prose より差分を前に出す
- 1回のコンパイルで独立した問題を取りこぼさない
- include / macro / template / linker のような「経路が重要な失敗」を family-aware に表現する
- 表示書式を業務都合で変えやすくする
- AI が設定ファイルを編集するときにも、意味が明瞭で壊れにくいフォーマットにする
- fail-open を守り、低信頼時は raw facts を隠さない

### 4.2 非目標

次は本設計の主目標ではない。

- text output を public machine-readable contract にすること
- IDE / HTML / TUI 用の最終 UI をこの設計で固定すること
- すべての internal family を、そのまま人間向け family として露出すること
- デコレーションや box drawing を主役にすること
- every raw line をそのまま保存的に並べること

---

## 5. 最終的なプロダクトモデル

### 5.1 ユーザーが理解すべき単位

最終アプリでユーザーが理解すべき単位は **block** である。

block は、1つの独立した correction target を表す。

block は raw diagnostic 1件と一致しないことがある。たとえば:

- 1つの type mismatch に 8 個の candidate note がぶら下がる → 1 block
- 1つの macro expansion に include chain がぶら下がる → 1 block
- 2つの全く別のエラーが同時にある → 2 block

### 5.2 用語定義

#### visible root

同一 invocation の中で、独立した correction target として人間に見せるべき root group。

#### dependent member

visible root に従属する follow-on / duplicate / note / chain member。通常は独立 block にしない。

#### semantic facts

表示の根拠となる構造化された短い事実。例: `want`, `got`, `via`, `need`, `from`, `symbol`, `now`, `prev`。

#### semantic shape

ある family に対して、どの semantic facts が意味を持つかを定める抽出モデル。例: `contrast`, `parser`, `lookup`, `linker`。

#### template

抽出済み semantic facts をどの順序で、どの label で、どの excerpt policy で並べるかを決める presentation 定義。

#### display family

header に出す人間向け family 名。例: `type_mismatch`, `syntax`, `missing_name`, `linker`。

### 5.3 このモデルが意味すること

このモデルでは、ユーザーは「raw line の列」ではなく「修正対象 block の列」を読む。

- 何を直すかは block 単位で理解する
- block の先頭で Subject を理解し、次の行で最初の修正行動を見る
- 同じ話の増殖は block 内 evidence / context / omission へ押し込む
- block の並び順が、そのまま correction target の優先順位になる

### 5.4 このモデルが意味しないこと

誤解を避けるため、次は本設計の意図ではない。

- raw diagnostic 1件 = block 1件、という単純対応
- 単独エラー UI をそのまま N 個複製しただけの表示
- warnings が error root と同格に failure view の先頭へ出てくること
- summary-only を visible root の既定表現にすること
- template / note / include chain の量が増えたときに block 数まで増やすこと

## 6. ユーザー体験の完成形

### 6.1 単独エラー

最も典型的な体験は次である。

```text
error: [type_mismatch] arguments do not match @ src/main.cpp:5:12
help: make both arguments the same type
want: T, T
got : int, const char*
via : combine(T, T) +2 notes
```

ポイントは次である。

- 1行目で「何の失敗か」が分かる
- 2行目で「最初にどう直すか」が分かる
- 3〜5行目で「なぜそう言えるか」が短い事実として分かる
- long prose を読まなくても、修正の方向が見える

### 6.2 複数の独立エラー

1回のコンパイルで 2つの独立エラーがある場合、block がそのまま縦に並ぶ。

```text
error: [type_mismatch] arguments do not match @ src/main.cpp:5:12
help: make both arguments the same type
want: T, T
got : int, const char*
via : combine(T, T)

error: [missing_name] unknown identifier @ src/main.cpp:11:7
help: include the header that declares the symbol or fix the name
name: WidgetFactory
use : WidgetFactory::create()
near: auto x = WidgetFactory::create();
```

このとき 2 block で終わる理由は、2件とも独立した correction target だからである。

### 6.3 cascade が激しい場合

1つの root に大量の dependent member がある場合は、block 数を増やさずに圧縮する。

```text
error: [type_mismatch] no viable overload for call @ src/main.cpp:22:9
help: convert the second argument to std::string
want: combine(int, std::string)
got : combine(int, const char*)
via : selected overload set +7 omitted candidates
```

ここで `+7 omitted candidates` は、**別 block を増やさずに情報量を保持するための圧縮**である。

### 6.4 include 不足で本当に止まる場合

header 自体が見つからず、その先の semantic analysis が本質的に進まない場合は、1 block で終わってよい。

```text
error: [missing_header] header could not be found @ src/main.cpp:1:10
help: fix the include path or add the missing dependency
need: "foo/bar.hpp"
from: #include "foo/bar.hpp"
```

これは「後続 root を隠した」のではなく、**コンパイラがその先の意味解析へ行けていない**からである。

### 6.5 warning-only run

warning-only の成功 run では、既定値として `lead_plus_summary` を維持してよい。

理由は、成功 run の可読性とログ長の管理である。ここは compile failure と目的が違うため、設計を分ける。

### 6.6 CI

CI では grep 性と clickable path を優先し、preset ごとに first line policy を固定する。

`subject_blocks_v2` の CI 既定は次とする。

```text
src/main.cpp:5:12: error: [type_mismatch] arguments do not match
```

local interactive default と CI first line は、契約を分けてよい。

---

## 7. Canonical block grammar

### 7.1 基本文法

interactive default の expanded block は次の順で構成する。

1. subject-first header
2. help
3. family-specific evidence
4. primary excerpt
5. family-specific context summary
6. suggestions / fix-it
7. omission notices
8. raw / debug footer

### 7.2 正規 header

interactive default の header format は次とする。

```text
{severity}: [{family}] {subject}{location_suffix_if_inline}
```

既定の location suffix は次とする。

```text
 @ {location}
```

### 7.3 evidence label の揃え方

evidence label のコロン位置は、block 内で揃える。

既定値は **template ごとの最大 label 幅** である。たとえば `help`, `want`, `got`, `via` は 4 文字幅で揃い、`symbol`, `archive` を使う template ではその template 内の最大幅で揃う。

この挙動は人間にとって見やすく、AI が template を編集したときも「label を足したせいで見た目が崩れる」問題を減らせる。

### 7.4 help 行の位置

`help:` は header の直後に置く。subject が分かった直後に、最初の修正行動へ行けるようにする。

### 7.5 raw の扱い

family facts が十分に抽出できない場合、あるいは confidence が低い場合は、generic block または raw fallback に戻る。

たとえば:

```text
error: [generic] unsupported diagnostic shape @ src/main.cpp:8:3
help: inspect the raw compiler message below
raw : declaration conflicts with previous statement of different kind
```

---

## 8. Family model

### 8.1 基本方針

全 family を `want / got / via` に強制しない。

統一するのは **文法** である。

- Subject
- Help
- Evidence
- Context

Evidence label は family ごとに変える。

### 8.2 表示 family と semantic shape の対応

| display family | semantic shape | 主な evidence slot | 典型例 |
|---|---|---|---|
| `type_mismatch` | `contrast` | `want`, `got`, `via` | overload / conversion / qualifier mismatch |
| `syntax` | `parser` | `want`, `near` | parse error / directive misuse |
| `missing_name` | `lookup` | `name`, `use`, `need`, `from`, `near` | undeclared identifier |
| `incomplete_type` | `lookup` | `name`, `use`, `need`, `from`, `near` | incomplete type / forward decl only |
| `unavailable_api` | `lookup` | `name`, `use`, `need`, `from`, `near` | deleted / inaccessible API |
| `missing_header` | `missing_header` | `need`, `from` | header not found |
| `redefinition` | `conflict` | `now`, `prev` | redefinition / conflicting declaration |
| `macro_include` | `context` | `from`, `via` | macro invocation + expansion + include chain |
| `linker` | `linker` | `symbol`, `from`, `archive`, `now`, `prev` | undefined reference / multiple definition |
| `generic` | `generic` | `raw` | unsupported or low-confidence case |

> **注記**: `incomplete_type` は internal family `pointer_reference` から、`unavailable_api` は `deleted_function` / `access_control` からルーティングされる。両方とも `lookup` semantic shape を `missing_name` と共有する。`linker` display family は `prefix:linker.`（ドット付き）マッチで解決される。ドットなしの bare `linker` internal family は generic に fallback する。

### 8.4 未マッピング family と generic fallback

enrich rulepack には約 52 の internal family が定義されているが、上記 §8.2 で明示的な family_mapping を持つのは約 21 のみである。残りの約 30 family（例: `template`, `constexpr`, `coroutine`, `deprecated`, `lambda_closure`, `move_semantics`, `init_order`, `structured_binding`, `ranges_views`, `uninitialized`, `unused` 等）は `generic_block` に fallback し、`raw` スロットのみで表示される。

これは段階的アプローチであり、設計上の欠落ではない。generic fallback は §12 の fail-open 方針に沿っており、低信頼な構造化よりも raw 保持を優先する。

ただし、以下の family は corpus データの頻度と render rulepack の既存対応を踏まえ、将来の extraction 追加の優先候補である。

| 優先度 | internal family | 理由 |
|---|---|---|
| 最優先 | `template` | render rulepack に `RendererFamilyKind::Template` があり、conservative limits で supporting evidence を制御済み。§17.3-17.4 で要求されるテスト基準あり |
| 高 | `constexpr`, `return_type` | contrast shape に適合しうる |
| 中 | `init_order`, `lifetime_dangling`, `null_pointer` | lookup または contrast shape に適合しうる |

`template` family については、現時点では generic block として表示されるが、supporting evidence（template frame）の圧縮は render rulepack の `RendererFamilyKind::Template` limits により既に機能している。将来の family_mapping 追加は §18 の migration plan で扱う。

### 8.5 Residual-only family

以下の family は residual rulepack にのみ存在し、enrich rulepack には含まれない。これらはコンパイラ本体の診断ではなく、ドライバ・アセンブラ・リンカドライバなどから発生する。

| family | 発生源 | 表示戦略 |
|---|---|---|
| `assembler_error` | assembler phase | generic block, raw 全文保持 |
| `driver_fatal` | GCC driver | generic block, raw 全文保持 |
| `internal_compiler_error_banner` | ICE (Internal Compiler Error) | **raw 全文保持必須**。切り詰め・構造化禁止。ICE はユーザーの修正対象ではなくコンパイラのバグ報告対象であるため、元のバナーを完全に保持する |
| `compiler.residual` | 未分類残余 | generic block |
| `collect2_summary` | linker driver (collect2) | 親 linker root の dependent member として cascade pairing。独立 block にしない |

`collect2_summary` は rendering-ux-contract-spec §17.8 の規定により、具体的な linker root（`linker.undefined_reference` 等）がある場合に lead を奪ってはならず、root を補完する summary-only / context note に留める。

### 8.3 各 family の canonical 例

#### type mismatch / overload / conversion

```text
error: [type_mismatch] arguments do not match @ src/main.cpp:5:12
help: make both arguments the same type
want: T, T
got : int, const char*
via : combine(T, T) +2 notes
```

#### syntax

```text
error: [syntax] expected ';' after declaration @ src/main.cpp:14:18
help: insert ';' before the next declaration
want: ';'
near: int x = 1
```

#### missing name

```text
error: [missing_name] unknown identifier @ src/main.cpp:9:7
help: include the header that declares the symbol or fix the name
name: WidgetFactory
use : WidgetFactory::create()
near: auto x = WidgetFactory::create();
```

#### incomplete type

```text
error: [incomplete_type] type is incomplete here @ src/main.cpp:27:9
help: include the full definition before dereferencing the type
name: struct Node
use : node->next
need: full type definition
```

#### unavailable API

```text
error: [unavailable_api] function is deleted or inaccessible @ src/main.cpp:15:5
help: use an alternative API or check access specifiers
name: Widget::reset()
use : widget.reset()
from: include/widget.hpp:42:5
```

#### missing header

```text
error: [missing_header] header could not be found @ src/main.cpp:1:10
help: fix the include path or add the missing dependency
need: "foo/bar.hpp"
from: #include "foo/bar.hpp"
```

#### redefinition

```text
error: [redefinition] symbol is defined more than once @ src/lib.cpp:18:5
help: keep one definition and remove or rename the other
now : src/lib.cpp:18:5
prev: include/lib.hpp:7:5
```

#### macro/include

```text
error: [macro_include] macro expansion has the wrong type @ src/app.c:33:12
help: cast the macro result or change the macro definition
from: invocation of 'READ_COUNT'
via : include/io_macros.h:14:9 expands to '(read_count(fd))'
```

#### linker (undefined reference)

```text
error: [linker] symbol could not be resolved
help: define the symbol or link the library that provides it
symbol: WidgetFactory::create()
from  : main.o +3 refs
archive: libwidgets.a
```

#### linker (multiple definition)

```text
error: [linker] symbol is defined in multiple objects
help: remove the duplicate definition or make the symbol internal
symbol : shared
now    : helper.c:(.text+0x0)
prev   : main.c:(.text+0x0)
from   : helper.o  +1 ref
```

`now` と `prev` は `linker.multiple_definition` で使用される。すべての linker sub-family がすべてのスロットを埋めるわけではなく、undefined reference では `symbol`, `from`, `archive` が主役であり、multiple definition では `now`, `prev` が加わる。

#### generic fallback

```text
error: [generic] compiler diagnostic could not be structured @ src/main.cpp:40:3
help: inspect the raw compiler message below
raw : declaration conflicts with previous statement of different kind
```

---

## 9. include / macro / preprocessing の整理

### 9.1 include 系は 1種類ではない

include 周りは、最終設計では次の 3 類型に分ける。

#### A. header 自体が見つからない

これは `missing_header` が主役である。subject も header 探索失敗に寄せる。

#### B. included header の内部で本当のエラーが起きている

この場合の主役は include ではなく、header 内部で起きた `syntax` / `type_mismatch` / `macro_include` などの実エラーである。include chain は context に回す。

#### C. 必要な include が足りない結果、名前や完全型が見えていない

この場合は `missing_name` や `incomplete_type` が主役である。`need:` や `from:` に「どの include が足りないか」のヒントを乗せてよい。

### 9.2 include chain の summary 規則

include chain は full 展開しない。default で見せる最小セットは次とする。

1. failure が起きた header
2. その header へ到達する first visible user-owned include edge
3. root translation unit

中間は `omitted N intermediate includes` とする。

### 9.3 macro expansion の summary 規則

macro family では次を最小セットとする。

1. `from:` first user-owned invocation site
2. `via:` terminal expansion / definition site

nested macro が深い場合、中間は omission notice に落とす。

---

## 10. 複数エラー時の挙動

### 10.1 基本契約

compile failure 時、visible root は **all visible as blocks** である。

「multiple errors のとき 1件表示の block が N個に増えるだけ」というあなたの要求は、最終設計では次のように具体化される。

- N は visible root 数である
- 依存メンバや follow-on は N に含めない
- cascade により圧縮されるものは block を増やさない
- hidden/suppressed にしてよいのは dependent member、warning tail、legacy compatibility mode、emergency degradation に限る

### 10.2 ordering

document 全体の ordering は次とする。

1. failure を引き起こした error/fatal groups
2. error 由来の supporting groups
3. warning groups
4. note-only / info-only / passthrough groups

同順位内では次で並べる。

1. `root_cause_score` 降順
2. `user_code_priority` 降順
3. `actionability_score` 降順
4. original group order 昇順

### 10.3 block を増やさないもの

次は block 数を増やさない。

- overload candidates
- template instantiation frames
- include chain の中間
- macro chain の中間
- duplicate / follow-on / related diagnostics
- 同一 symbol / same role / same object 由来の repeated linker lines

### 10.4 warning の扱い

failure run に warning が混在する場合、warning は error block より前に expanded 表示しない。

既定値では summary 化してよい。例:

```text
note: suppressed 2 warning(s) while focusing on failure blocks
```

### 10.5 例外的な summary-only

summary-only を visible root 用の primary modelとして使ってはならない。

summary-only に落としてよいのは次に限る。

- cascade-hidden member の omission notice
- warning-tail summary
- `lead_plus_summary` を明示した legacy / compatibility mode
- extreme safety cap による emergency degradation

---

## 11. Local / CI / verbose の役割分担

### 11.1 local default

local default は人間が読むモードであり、subject-first を最優先する。

- header: `error: [family] subject`
- location: inline suffix 優先
- first action: header 直後
- excerpt: family と budget に応じて 0〜1 以上

### 11.2 concise

concise は quick scan 用であり、block 数は変えない。1 block あたりの情報量だけを減らす。

### 11.3 verbose / debug

verbose / debug は investigation 用であり、chain / excerpt / omitted の展開量を増やす。

### 11.4 CI

CI は grep / clickable path / deterministic text を優先する。

`subject_blocks_v2` の CI 既定は path-first first line とする。ただし local contract を CI に引きずらない。

CI header format の切り替えは `RenderProfile` によって決定される。`profile = Ci` のとき renderer は `[header].ci_path_first_format` を使用し、それ以外のプロファイルでは `[header].interactive_format` を使用する。この profile 判定が、local と CI の header policy を分離する唯一のメカニズムである。

---

## 12. Fail-open と honesty

### 12.1 方針

wrapper は compiler facts を隠してはいけない。低信頼のときほど控えめに振る舞う。

### 12.2 fail-open の発動条件

次では generic block ないし raw fallback を許容する。

- unsupported diagnostic tier
- incompatible sink
- residual only
- renderer low confidence
- internal error
- timeout / budget overrun
- user opt-out

### 12.3 ユーザーに見せる振る舞い

- partial / fallback は silent に発動してはならない
- raw へ戻る導線を必ず残す
- structured 表示と raw 表示が混在してもよいが、structured 部分が断定的すぎてはいけない

---

## 13. Configuration architecture

### 13.1 基本方針

設定ファイルは **TOML** を採用し続ける。

理由は次である。

- 人間が読みやすい
- AI が編集しやすい
- コメントを書ける
- preset / overlay / relative path 解決と相性が良い
- 現行 repo にすでに導入されている

### 13.2 built-in preset 戦略

built-in preset は次の 3 系列を持つ。

- `subject_blocks_v2`: 本設計の最終 target
- `subject_blocks_v1`: 直前世代の beta preset
- `legacy_v1`: 明示 rollback 用

`subject_blocks_v2` が最終的な既定 preset になる。

### 13.3 schema の考え方

外部 presentation file は **schema version 2** を導入する。

理由は、次の 3 つを設定ファイル側へ移すためである。

1. header format
2. semantic shape の明示
3. label 幅ポリシー

v1 は互換入力として読み続けてよいが、最終 target は v2 とする。

### 13.4 schema v2 の責務分離

presentation config で表現できる責務は次のとおり。

- session policy
- header format
- label catalog
- location policy
- template body
- family mapping
- semantic shape selection

これにより、「hoge/fuga は A template、piyo は B template」のような切り替えを no-code で行える。

### 13.5 schema v2 の最小例

```toml
kind = "cc_formed_presentation"
schema_version = 2

[session]
visible_root_mode = "all_visible_blocks"
warning_only_mode = "lead_plus_summary"
block_separator = "blank_line"
unknown_template = "generic_block"

[header]
subject_first = true
interactive_format = "{severity}: [{family}] {subject}"
ci_path_first_format = "{location}: {severity}: [{family}] {subject}"
unknown_family = "generic"

[labels]
label_width_mode = "template_max"
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
raw = "raw"
omitted = "omitted"

[location]
default_placement = "inline_suffix"
inline_suffix_format = " @ {location}"
fallback_order = ["header", "evidence", "excerpt_header", "none"]
width_soft_limit = 100

[[templates]]
id = "contrast_compact"
excerpt = "off"
core = [
  { slot = "help", optional = true },
  { slot = "want", optional = true },
  { slot = "got",  optional = true },
  { slot = "via",  optional = true, suffix_slot = "omitted_notes_suffix" },
]

[[family_mappings]]
match = ["type_overload", "concepts_constraints", "format_string", "conversion_narrowing", "const_qualifier"]
display_family = "type_mismatch"
semantic_shape = "contrast"
template = "contrast_compact"
```

### 13.6 slot 名の正規化

schema v2 では、config 内 slot 名を **見た目と同じ語彙** に揃える。

- `help`
- `want`
- `got`
- `via`
- `need`
- `from`
- `name`
- `use`
- `near`
- `symbol`
- `archive`
- `now`
- `prev`
- `raw`
- `omitted`

v1 からのリネームと根拠は次のとおり。

| v1 slot 名 | v2 slot 名 | リネーム根拠 |
|---|---|---|
| `first_action` | `help` | 表示ラベル `help:` と一致させる |
| `expected` | `want` | 短縮・直感性。`want` / `got` の対で人間が差分を即座に読める |
| `actual` | `got` | 同上 |
| `why_raw` | `raw` | v2 では `raw` は fail-open raw-preservation 専用スロット。`why:` ラベルは高信頼コア文法から除外する（rendering-ux-contract-spec §15.5 準拠）。Rust enum `SemanticSlotId::WhyRaw` は Issue 2 で `SemanticSlotId::Raw` にリネームする |

v1 の `first_action`, `expected`, `actual`, `why_raw` は互換 alias として受理してよいが、新規作成では推奨しない。

chain intro labels（`while`, `from`, `through`）は renderer 内部の文言であり、v2 では config 管理対象外とする。将来カスタマイズが必要になった場合は `[chain_labels]` セクションを追加してよい。

### 13.7 semantic_shape の導入

`semantic_shape` は本設計の中核である。

`template` は表示形式であり、extractor の責務を持たない。
`semantic_shape` は抽出責務を持ち、template は抽出済み slot を並べるだけにする。

例:

- `type_mismatch` + `contrast_compact`
- `type_mismatch` + `contrast_verbose`

どちらも `semantic_shape = "contrast"` なら、抽出される facts は同じで、並び方だけが違う。

この分離によって、**template 名を増やしても Rust 側の `match template_id` を増やさなくて済む**。

### 13.8 header format を設定化する理由

header の punctuation は UX に直結する。

今回の `error: [family] subject` 変更が示したように、コロンや bracket の位置は重要である。にもかかわらず、それがコードに埋まっていると、業務上の微修正でもビルドが必要になる。

そのため header は config 管理へ移す。これは「装飾」ではなく、**製品ポリシーの外部化**である。

`[header].subject_first = true` は、subject-first evidence layout（family-aware evidence を header 直後に並べるレイアウト）を有効化する config key である。`false` を指定すると legacy evidence layout を使う。この key が、現行実装の `self.preset_id == "subject_blocks_v1"` によるハードコード比較を置き換える。`subject_blocks_v2` built-in preset は `subject_first = true` を既定値として持つ。

### 13.9 v1 から v2 への互換規則

v2 loader は、既存の v1 asset / external file からの移行を次のように扱う。

- v1 の `location.label_width = N` は、v2 では `label_width_mode = "fixed"` と `fixed_label_width = N` に正規化してよい
- v1 には `[header]` が存在しないため、loader は built-in default header policy を補う
- v1 の slot 名 (`first_action`, `expected`, `actual`, `why_raw`) は互換 alias として受理してよいが、新規定義では v2 語彙 (`help`, `want`, `got`, `raw`) を使う
- v2 の `semantic_shape` が未指定のときは、family mapping と built-in defaults から補完してよい
- 互換入力から v2 の内部表現へ正規化した結果は、warning を出してもよいが silent に意味を変えてはならない

## 14. Internal architecture

### 14.1 pipeline

最終 pipeline は概念的に次の順で整理する。

1. capture / ingest
2. grouping / episode analysis
3. visible-root selection
4. ordering / budget
5. semantic fact extraction
6. presentation policy resolution
7. block layout / emission
8. fail-open fallback

### 14.2 semantic facts 抽出の責務

semantic facts 抽出は、`semantic_shape` と input facts を見て行う。

- `contrast` → `want`, `got`, `via`
- `parser` → `want`, `near`
- `lookup` → `name`, `use`, `need`, `from`, `near`
- `missing_header` → `need`, `from`
- `conflict` → `now`, `prev`
- `context` → `from`, `via`
- `linker` → `symbol`, `from`, `archive`, `now`, `prev`
- `generic` → `raw`

### 14.3 template の責務

template は、抽出済み slot を次の観点で並べるだけにする。

- 順番
- label
- optional / required
- suffix slot
- excerpt policy

### 14.4 layout の責務

layout は次だけを担当する。

- header text 組み立て
- location placement
- label padding
- line wrapping / truncation
- block separator
- ASCII / ANSI 差分

### 14.5 selector の責務

selector は次だけを担当する。

- visible root 決定
- ordering
- profile budget
- warning suppression
- dependent member の omission

selector は `template_id` を見てはいけない。

### 14.6 view model の責務

view model は、selection と semantic extraction の結果を block 単位に固定する中間表現である。

この層でテストできるようにし、text snapshot だけに依存しない。

### 14.7 semantic_shape と ADR-0030 四層モデルの対応

ADR-0030 は renderer 内部を `facts / analysis / view model / theme-layout` の四層に分離することを規定している。`semantic_shape` はこの四層モデルの中で次のように位置づける。

- `semantic_shape` は **view model 層** に属する routing 概念である
- `analysis` 層が internal family と confidence を生成し、`semantic_shape` は presentation policy resolution（pipeline step 6）で `family_mapping` テーブルから解決される
- `semantic_shape` は「analysis が生成した facts のうち、どれを構造的に取り出すか」を選択する。analysis の出力を消費するが、analysis の意味論には干渉しない
- `template` は theme-layout 層に属し、抽出済み slot の並べ方・見せ方だけを担当する

したがって、ADR-0030 の四層と本設計の三層分離の対応は次である。

| ADR-0030 の層 | 本設計の概念 |
|---|---|
| facts | DiagnosticDocument / raw diagnostics |
| analysis | internal family / confidence / AnalysisOverlay |
| view model | **display_family** / **semantic_shape** / semantic facts extraction / RenderGroupCard |
| theme-layout | **template** / layout / header format / label catalog |

`semantic_shape` の実装にあたっては、ADR-0034 を改訂して `semantic_shape` を正式に定義するか、新規 ADR を起票して view model 層における抽出 routing の位置づけを固定する必要がある。

### 14.8 Shape fallback と動的再ルーティング

一部の internal family は、diagnostic の内容に応じて複数の semantic shape に適合しうる。

典型例は `preprocessor_directive` である。通常は `parser` shape を使うが、ファイルが見つからない `#include` エラーの場合は `missing_header` shape のほうが適切なスロット（`need`, `from`）を抽出できる。

この動的再ルーティングを `semantic_shape` モデルで表現するため、family_mapping に `shape_fallback` を導入する。

```toml
[[family_mappings]]
match = ["preprocessor_directive"]
display_family = "syntax"
semantic_shape = "parser"
shape_fallback = [
  { shape = "missing_header", display_family = "missing_header" }
]
```

ルーティングの評価順は次とする。

1. `shape_fallback` のリストを先頭から試す
2. fallback shape のスロット抽出が成功すれば、その shape と `display_family` を採用する
3. すべての fallback が失敗した場合、主 `semantic_shape` を使用する

この仕組みにより、現行 `view_model.rs` の `parser_block` → `missing_header_block` 動的切り替えを config-driven に移行できる。`shape_fallback` を持たない family_mapping エントリでは、主 `semantic_shape` のみが使用される（既定の挙動と完全に互換）。

---

## 15. 旧仕様との競合整理

### 15.1 `all_visible_blocks` と summary path

compile failure 時の built-in default は `all_visible_blocks` である。

したがって、`lead_plus_summary` / `capped_blocks` は次の用途に限定する。

- warning-only optimization
- legacy compatibility
- user opt-in
- extreme safety cap

default failure run では、visible root の抑制理由に使ってはならない。

### 15.2 `cascade.max_expanded_independent_roots`

`cascade.max_expanded_independent_roots` は、`subject_blocks_v2` の compile failure visibility を直接支配しない。

使ってよいのは次だけである。

- legacy preset
- explicit `capped_blocks`
- emergency degradation
- debug / experiment

### 15.3 `template_id` 依存の抽出

現行実装に残る `match template_id` ベースの slot extraction は、`subject_blocks_v2` では transitional compatibility とし、最終的には排除する。

---

## 16. Migration plan

### 16.1 段階

1. `subject_blocks_v2` preset を built-in で追加
2. schema v2 loader を追加
3. `semantic_shape` ベースの extractor へ移行
4. corpus / snapshot / review で family ごとの差分を確認
5. `subject_blocks_v2` を既定 preset へ昇格
6. `subject_blocks_v1` を rollback-compatible preset として維持

### 16.4 未マッピング family の段階的拡充

§8.4 に記載した約 30 の未マッピング family は、corpus データの頻度分析を基に段階的に family_mapping を追加する。優先順位は §8.4 のとおりであり、最優先は `template` family である。

この拡充は `subject_blocks_v2` の既定昇格（段階 5）の後でもよい。既定昇格時点で未マッピングの family は、引き続き generic block として表示され、fail-open の原則に沿う。

### 16.2 互換性

- `render.presentation = "subject_blocks_v1"` は引き続き動く
- `legacy_v1` も維持する
- external presentation file が壊れていたら built-in default へフォールバックする
- relative path 解決は従来通り config file 基準で行う

### 16.3 public contract との関係

人間向け text presentation の進化は、public machine-readable contract を直接壊す理由にしない。

必要な追加がある場合も、public JSON への変更は additive に行う。

---

## 17. 受け入れ基準

### 17.1 単体 UX 基準

- local default で severity / location / first action が最初の 8 非空行以内に入る
- type mismatch family で `want / got / via` が header 直後に見える
- path-first first line は CI に限定される
- warning が failure block より前に expanded 表示されない
- low-confidence / fallback で raw への導線が消えない

### 17.2 複数エラー基準

- compile failure 時、visible root が N 件なら N block 見える
- dependent member は block を増やさない
- overload flood は block 1件 + omission で済む
- include chain flood は block 1件 + omission で済む
- 同一 symbol の repeated linker lines は group 化される

### 17.3 カスタマイズ基準

- family ごとの template 差し替えが config だけでできる
- header punctuation が config だけで変えられる
- subject-first header の有効化が preset ID の文字列比較ではなく、resolved presentation policy で決まる
- v2 config にカスタム template を追加しても、Rust 側に `match template_id` を足さなくてよい
- label の追加・変更で evidence colon の視認性が壊れない

### 17.4 テスト基準

最低限、次の snapshot / contract test を持つ。

- syntax error
- type mismatch / conversion
- overload candidate flood
- template deep chain
- macro expansion chain
- include chain
- missing header
- missing include causing missing name
- linker undefined reference
- linker multiple definition
- multi-root compile failure
- warning-only success run
- raw fallback
- custom presentation file overlay

---

## 18. 実装 issue 草稿

以下は、そのまま GitHub issue 草稿として起票できる粒度で記す。

### Issue 1: Add `subject_blocks_v2` built-in preset and schema v2 loader

**目的**

`subject_blocks_v2` を新 built-in preset として導入し、schema version 2 の presentation file を読み込めるようにする。

**変更点**

- `subject_blocks_v2.toml` を追加
- schema v2 の parse / validate / normalize を追加
- `header` section（`subject_first`, `interactive_format`, `ci_path_first_format`）を config model に追加
- `semantic_shape` を config model に追加
- `subject_first_header` の有効化を preset 名ハードコードから外し、`[header].subject_first` config key で決定するよう移行（§13.8 参照）
- v1 preset / file は互換入力として継続

**Done 条件**

- `--formed-presentation=subject_blocks_v2` で起動できる
- external v2 file を読み込める
- `subject_blocks_v2` と custom v2 file のどちらでも subject-first header を有効化できる
- 読み込み失敗時は built-in default へフォールバックする
- schema error message が人間に読める

### Issue 2: Replace template-driven semantic extraction with `semantic_shape` extraction

**前提条件**

- ADR-0034 を改訂し、`semantic_shape` を view model 層の routing 概念として正式に定義すること（§14.7 参照）

**目的**

view model の semantic slot 抽出を `template_id` 依存から分離し、custom template を no-code で追加できる基盤を作る。

**変更点**

- extractor の入力を `semantic_shape` に変更
- `contrast / parser / lookup / missing_header / conflict / context / linker / generic` shape を定義
- template は slot の並べ替えに専念させる
- template 名に依存する分岐を compatibility layer へ隔離
- `view_model.rs` の `parser_block` → `missing_header_block` 動的切り替えを `shape_fallback` メカニズム（§14.8）に移行
- `SemanticSlotId::WhyRaw` を `SemanticSlotId::Raw` にリネーム（§13.6 参照）

**Done 条件**

- `contrast_compact` と `contrast_verbose` を config だけで追加しても同じ semantic facts が抽出される
- `match template_id` が新経路では不要になる
- `preprocessor_directive` が diagnostic 内容に応じて `parser` または `missing_header` shape に自動ルーティングされる
- regression test で v1 preset と v2 preset の両方が通る

### Issue 3: Enforce compile-failure visible-root contract end-to-end

**目的**

compile failure 時の visible root は built-in default で summary-only に落ちない、という契約を selector / session / cascade の全経路で保証する。

**変更点**

- `all_visible_blocks` failure run の invariant を selector に固定
- `cascade.max_expanded_independent_roots` が default failure visibility を下げないように整理
- summary-only path の用途を warning-only / legacy / emergency に限定

**Done 条件**

- multi-root fixture で root 数と block 数が一致する
- dependent diagnostics は親 block の omission に吸収される
- default / concise / ci で root が silent に落ちない

### Issue 4: Refine family handling for include, macro, lookup, and linker

**目的**

include 不足・header 内エラー・missing include 起因の missing name を混同しないよう family semantics を整理する。

**変更点**

- `missing_header` の canonical subject / slots を固定
- `missing_name` / `incomplete_type` の `need/from` 抽出を強化
- macro/include chain summary を shape-level rule として実装
- linker repeated lines の symbol grouping を強化

**Done 条件**

- `#include` 不足と `header not found` が別 family として見える
- macro/include fixture で invocation と terminal expansion と root include edge が分かる
- linker fixture で `symbol:` / `from:` / `archive:` が安定して出る

### Issue 5: Externalize header and label layout policy

**目的**

header punctuation と evidence label alignment を設定ファイルで制御できるようにし、業務カスタマイズを no-code 化する。

**変更点**

- `header.interactive_format` / `header.ci_path_first_format` を追加
- `location.inline_suffix_format` を追加
- `label_width_mode = "template_max" | "fixed"` と `fixed_label_width` を追加
- v1 の `location.label_width` から v2 への正規化規則を loader に追加
- renderer layout を config-driven に移行

**Done 条件**

- `error: [family] subject` を config で別書式へ変更できる
- custom label を足しても colon alignment が崩れにくい
- `template_max` と `fixed` の両方で label 整列が安定する
- local / CI の header policy を preset ごとに固定できる

### Issue 6: Snapshot and contract-test expansion for final UX

**目的**

最終 UX を snapshot と contract test の両面で固定し、以後の regression を検出できるようにする。

**変更点**

- family corpus を拡充
- multi-root compile failure fixture を追加
- custom presentation file fixture を追加
- preset-name に依存しない subject-first header activation fixture を追加
- view model contract test を追加

**Done 条件**

- text snapshot だけでなく view model test がある
- root 数 / excerpt 数 / omission 数 / raw retention が機械検証される
- CI / default / verbose の差分が意図どおり固定される

### Issue 7: Promote `subject_blocks_v2` to default and retain rollback paths

**目的**

レビュー済みの `subject_blocks_v2` を既定 preset へ昇格しつつ、`subject_blocks_v1` と `legacy_v1` を rollback path として残す。

**変更点**

- default preset 定数を v2 へ変更
- README / docs / examples を更新
- migration note を追加
- rollback 手順を明文化

**Done 条件**

- no-config の terminal render が `subject_blocks_v2` を使う
- `subject_blocks_v1` / `legacy_v1` へ明示 rollback できる
- docs と snapshots が一致している

---

## 19. 最終決定

本設計により、`gcc-formed` の最終的な人間向け presentation は次のように整理される。

- **アプリの役割**は、GCC の診断を「独立した correction target の block 列」として見せること
- **既定の mental model** は、raw line ではなく visible root block
- **header の正規形**は `error: [family] subject`
- **compile failure の既定挙動**は「visible root を block として全部見せる」
- **cascade 圧縮**は「同じ話の増殖」を block の中へ押し込むためのもの
- **include / macro / linker** は family-aware に別設計で扱う
- **表示の将来カスタマイズ**は、TOML presentation config によって no-code で可能にする
- **内部アーキテクチャの核心**は、`display_family / semantic_shape / template` の三層分離
- **honesty の最後の砦**は fail-open と raw fallback

この方針を `subject_blocks_v2` として実装し、corpus / snapshot / review を経て既定値へ昇格させる。

これをもって、本件の presentation 方針の最終決定版とする。

---

## 付録 A: v1 から v2 への主要差分

| 項目 | `subject_blocks_v1` | `subject_blocks_v2` |
|---|---|---|
| 主 header 書式 | subject-first 実装あり | `error: [family] subject` を正規化 |
| visible roots | spec 上は all visible 方向 | compile failure で明示的に全 block 表示 |
| semantic extraction | template 名に依存する経路が残る | `semantic_shape` に一本化 |
| config editability | labels / location / templates / family mapping | header / semantic_shape / label width まで外部化 |
| custom template | 一部 no-code だが内部依存あり | 本当の意味で no-code を目指す |
| schema | v1 | v2 |

## 付録 B: 最小 preset 例

```toml
kind = "cc_formed_presentation"
schema_version = 2

[session]
visible_root_mode = "all_visible_blocks"
warning_only_mode = "lead_plus_summary"
block_separator = "blank_line"
unknown_template = "generic_block"

[header]
subject_first = true
interactive_format = "{severity}: [{family}] {subject}"
ci_path_first_format = "{location}: {severity}: [{family}] {subject}"
unknown_family = "generic"

[labels]
label_width_mode = "template_max"
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
raw = "raw"
omitted = "omitted"

[location]
default_placement = "inline_suffix"
inline_suffix_format = " @ {location}"
fallback_order = ["header", "evidence", "excerpt_header", "none"]
width_soft_limit = 100

[[templates]]
id = "generic_block"
excerpt = "off"
core = [
  { slot = "help", optional = true },
  { slot = "raw", optional = true },
]

[[templates]]
id = "contrast_block"
excerpt = "off"
core = [
  { slot = "help", optional = true },
  { slot = "want", optional = true },
  { slot = "got", optional = true },
  { slot = "via", optional = true, suffix_slot = "omitted_notes_suffix" },
]

[[family_mappings]]
match = ["type_overload", "concepts_constraints", "format_string", "conversion_narrowing", "const_qualifier"]
display_family = "type_mismatch"
semantic_shape = "contrast"
template = "contrast_block"
```