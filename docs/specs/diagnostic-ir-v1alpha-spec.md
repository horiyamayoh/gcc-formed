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

# gcc-formed / cc-formed Diagnostic IR v1alpha 仕様書

- **文書種別**: 内部仕様書（実装契約）
- **状態**: Accepted Baseline
- **版**: `1.0.0-alpha.1`
- **対象**: `gcc-formed` / 将来の `cc-formed`
- **主用途**: compiler adapter / enrichment / renderer / test harness 間の共通契約
- **想定実装**: Linux first, GCC first, 将来の Clang 拡張を阻害しない設計
- **関連文書**:
  - `../architecture/gcc-formed-vnext-change-design.md`
  - `../process/implementation-bootstrap-sequence.md`
  - `../support/SUPPORT-BOUNDARY.md`
- **関連 ADR**:
  - `adr-initial-set/adr-0002-diagnostic-ir-as-product-core.md`
  - `adr-initial-set/adr-0009-library-plus-cli-layering.md`
  - `adr-initial-set/adr-0010-deterministic-rule-engine-no-ai-core.md`
  - `adr-initial-set/adr-0012-native-ir-json-as-canonical-machine-output.md`
  - `adr-initial-set/adr-0015-source-ownership-model.md`
  - `adr-initial-set/adr-0016-trace-bundle-content-and-redaction.md`
  - `adr-initial-set/adr-0020-stability-promises.md`
  - `adr-initial-set/adr-0028-capturebundle-only-ingest-entry.md`

---

> **Authority**
> This is the current Diagnostic IR contract. `v1alpha` in this document names the schema/version lineage, not the current product maturity line. The product remains on the `v1beta` artifact line unless a release policy doc says otherwise.

## 1. この文書の目的

本仕様書は、C/C++ コンパイル失敗を「コンパイラの生出力」から「開発者にとって修正可能な診断情報」へ変換するための、**内部正規形 Diagnostic IR (Intermediate Representation)** を定義する。

この IR は、次の 4 者の境界契約である。

1. **Compiler/Linker Adapter**
   - GCC / 将来の Clang / 各種 linker の診断を取り込み、構造化する。
2. **Enrichment / Ranking**
   - root cause 推定、template / macro / include / linker の要約、action hint の生成を行う。
3. **Renderer**
   - terminal / CI / machine-readable / 将来の editor 連携に対して再表示する。
4. **Test Harness / Corpus**
   - raw diagnostics、normalized IR、rendered output を固定して品質回帰を検出する。

本仕様は、**実装コードの詳細**ではなく、**意味論・責務境界・不変条件・型概念・validation 規則**を定義する。

---

## 2. 規範語と前提

### 2.1 規範語

本仕様では以下の意味で規範語を使う。

- **MUST**: 必須
- **MUST NOT**: 禁止
- **SHOULD**: 強い推奨
- **SHOULD NOT**: 強い非推奨
- **MAY**: 任意

### 2.2 置く前提

本仕様は以下を前提とする。

- GCC 15 では、`-fdiagnostics-add-output=` により診断の追加出力 sink を指定でき、text と SARIF を同時に出力できる。
- GCC 15 では `-fdiagnostics-format=json` 系は deprecated であり、機械可読な一次入力としては SARIF を優先すべきである。
- GCC 13/14 系では JSON / SARIF / child diagnostics / fix-it / path といった構造化要素が利用可能である。
- Clang は parseable fix-its と parseable source ranges を持ち、将来 adapter を実装しやすい。
- Clang / GCC ともに template diff や source ranges の表現機能を持つが、それらの**表示形式**自体を core IR にしない。

### 2.3 スコープ

本仕様が扱うのは以下である。

- 1 回の compiler/linker invocation に対する診断群
- 診断の木構造
- source location / source range / fix-it / suggestion
- include / macro / template / analyzer path / linker resolution の文脈
- provenance（元データへの追跡）
- analysis overlay（ranking / summary / confidence）
- 部分失敗・passthrough・fallback の表現

本仕様が直接扱わないものは以下である。

- ビルド全体集約の public schema
- LSP / IDE プロトコル
- trace bundle のアーカイブ形式そのもの
- editor plugin API
- organization 固有 knowledge base 連携
- AI/LLM による自由文説明

---

## 3. IR の位置づけ

IR は pipeline 上で以下の位置に置く。

```text
compiler/linker stdout/stderr + SARIF/JSON/text
        │
        ▼
[adapter / ingestion]
        │
        ▼
[core Diagnostic IR: facts]
        │
        ├──► [renderer: fallback-capable]
        │
        ▼
[analysis overlay: ranking/summarization]
        │
        ├──► [terminal renderer]
        ├──► [CI renderer]
        ├──► [machine-readable export]
        └──► [future editor integration]
```

重要な原則は以下の通り。

- **core facts** と **analysis overlay** は分離する。
- renderer は analysis が無くても最低限動作しなければならない。
- adapter は compiler ごとの差を吸収するが、**元情報を捨てない**。
- analysis は **追加**であり、facts の置換ではない。

---

## 4. v1alpha の設計目標

### 4.1 主要目標

1. **情報損失を最小化する**
2. **GCC first だが compiler-agnostic に伸ばせる**
3. **renderer が message 文字列のパースに依存しない**
4. **template / macro / include / linker を first-class に扱う**
5. **partial parse / fallback / passthrough を表現できる**
6. **テストしやすい**
7. **future Clang adapter を壊さない**

### 4.2 非目標

v1alpha は以下を goal にしない。

- 外部公開 API としての長期安定
- 全 compiler 間で完全一致する taxonomy の凍結
- C/C++ AST を自前で再構築すること
- raw diagnostics を置き換える唯一の表示形式になること
- build 全体での dedup / clustering をこの IR だけで完結させること

---

## 5. 中核原則（不変条件）

本節は **最重要** である。実装はすべてこれに従う。

### 原則 1: facts を analysis で上書きしない

compiler/linker が持っている事実と、wrapper が推定した解釈は分離する。

- `message.raw_text` は事実
- `analysis.headline` は解釈
- `locations[]` は事実
- `analysis.preferred_primary_location_id` は解釈

### 原則 2: tree を保持する

診断はまず **木構造**で保持する。表示都合の flatten は renderer の責務である。

**MUST NOT**: core IR 上で child notes を最初から平坦化する。  
**理由**: GCC/Clang の child 診断、candidate 群、analyzer path、template note 群は因果情報を持つため。

### 原則 3: unknown は guess より良い

不明な値は `unknown` または欠落として表現し、根拠の薄い推定値を埋めない。

### 原則 4: raw へ必ず戻れる

core IR の任意の diagnostic node は、可能な限り raw ingress へ追跡できるべきである。

### 原則 5: display location と edit range を混同しない

「表示上のハイライト範囲」と「機械適用する edit の範囲」は意味が違う。  
前者は compiler 表示の都合を含み、後者は置換操作の意味を持つ。

### 原則 6: byte column と display column を混同しない

特に multibyte 文字・tab・Unicode を考えると、byte 列と表示列は同一ではない。  
両者を持てる設計にし、必要なら native unit も保持する。

### 原則 7: normalized chain は additive である

include / macro / template / linker の chain は、元の child notes から抽出したとしても、元の node を消さない。  
chain は **正規化ビュー**であり、原本の置換ではない。

### 原則 8: root clustering は後段責務である

adapter は、backend が独立に出した top-level diagnostic を勝手に merge しない。  
複数の root を 1 つの論理原因として扱うかは analysis の責務である。

### 原則 9: partial success を表現できる

機械可読入力の一部が壊れていても、passthrough と raw provenance により fail-open で扱う。

### 原則 10: compiler-specific extension は許可するが、依存しない

`extensions.gcc` や `extensions.clang` は許す。  
ただし **renderer の correctness は core field のみで成立**しなければならない。

---

## 6. IR 単位と交換境界

### 6.1 IR の基本単位

**1 つの `DiagnosticDocument` は、1 回の wrapper が捕捉した 1 invocation を表す。**

この invocation は以下を含みうる。

- `gcc` / `g++` driver
- `cc1` / `cc1plus`
- assembler
- linker（`collect2`, `ld.bfd`, `gold`, `lld`, `mold` 等）

ただし build 全体の複数 invocation を 1 文書にまとめない。  
build 単位の集約は上位レイヤの責務とする。

### 6.2 facts 層と analysis 層

本仕様は 1 つの JSON 直列化内で facts と analysis を共存させてよいが、意味論上は別層である。

- `provenance.source = compiler_native | adapter_structural` → facts 側
- `document_analysis.*` / `analysis.*` → wrapper 解釈側

consumer は analysis を捨てても facts を使えることが望ましい。

---

## 7. データモデル概要

```text
DiagnosticDocument
├── producer
├── run
├── captures[]
├── diagnostics[]            # top-level root diagnostics
├── document_analysis?
├── ingestion_issues[]
├── fingerprints?
└── extensions?

DiagnosticNode
├── identity/classifier
├── message
├── locations[]
├── children[]
├── suggestions[]
├── context_chains[]
├── symbol_context?
├── provenance
├── analysis?
├── fingerprints?
└── extensions?
```

---

## 8. 規範的スキーマ

以下の表は「概念上の型契約」を示す。  
JSON / YAML / CBOR 等の直列化は後続節で定義する。

### 8.1 `DiagnosticDocument`

| フィールド | 型 | 必須 | 意味 |
|---|---|---:|---|
| `schema_version` | string | MUST | 例: `1.0.0-alpha.1` |
| `document_id` | string | MUST | 文書一意 ID。UUID / ULID / opaque ID を許容 |
| `producer` | `ProducerInfo` | MUST | この IR を生成したツール情報 |
| `run` | `RunInfo` | MUST | invocation 単位の実行文脈 |
| `captures` | array<`CaptureArtifact`> | SHOULD | raw ingress / 補助 artifact 一覧 |
| `diagnostics` | array<`DiagnosticNode`> | MUST | top-level root diagnostics |
| `document_analysis` | `DocumentAnalysis` | MAY | document-wide episode / cascade analysis |
| `document_completeness` | enum | MUST | `complete | partial | passthrough | failed` |
| `ingestion_issues` | array<`IntegrityIssue`> | MAY | capture/parse/normalize 失敗や警告 |
| `fingerprints` | `FingerprintSet` | MAY | 文書単位 fingerprint |
| `extensions` | map | MAY | compiler-specific / product-specific 拡張 |

#### `document_completeness` の意味

- `complete`: 機械可読入力を主として取り込み、IR と provenance が十分そろっている
- `partial`: 一部要素が欠落しているが、意味のある IR を構築できた
- `passthrough`: raw text を主体にした安全 fallback
- `failed`: 診断 IR として成立しなかった。wrapper 自身の障害または capture 不能

### 8.2 `ProducerInfo`

| フィールド | 型 | 必須 | 意味 |
|---|---|---:|---|
| `name` | string | MUST | 例: `gcc-formed` |
| `version` | string | MUST | 製品バージョン |
| `git_revision` | string | MAY | ビルド識別子 |
| `build_profile` | string | MAY | `release`, `dev`, `ci` など |
| `rulepack_version` | string | MAY | resolved shared checked-in `diag_rulepack` version identifier loaded by runtime analysis / render policy（例: `phase1`） |

### 8.3 `RunInfo`

| フィールド | 型 | 必須 | 意味 |
|---|---|---:|---|
| `invocation_id` | string | MUST | 1 invocation の一意 ID |
| `invoked_as` | string | MAY | 例: `gcc-formed`, `g++-formed` |
| `argv_redacted` | array<string> | MAY | redaction 済み argv。秘匿上の理由で省略可 |
| `cwd_display` | string | MAY | 表示用作業ディレクトリ |
| `exit_status` | integer | MUST | 最終 exit code |
| `primary_tool` | `ToolInfo` | MUST | 主たる backend or driver |
| `secondary_tools` | array<`ToolInfo`> | MAY | linker 等の補助ツール |
| `language_mode` | enum | MAY | `c | c++ | objc | objc++ | unknown` |
| `target_triple` | string | MAY | ターゲット triple |
| `wrapper_mode` | enum | MAY | `terminal | ci | editor | trace-only | unknown` |

### 8.4 `ToolInfo`

| フィールド | 型 | 必須 | 意味 |
|---|---|---:|---|
| `name` | string | MUST | 例: `gcc`, `g++`, `collect2`, `ld.bfd`, `clang` |
| `version` | string | MAY | 例: `15.2.0` |
| `component` | string | MAY | 例: `cc1plus`, `driver`, `linker` |
| `vendor` | string | MAY | 例: `GNU`, `LLVM` |

### 8.5 `CaptureArtifact`

| フィールド | 型 | 必須 | 意味 |
|---|---|---:|---|
| `id` | string | MUST | capture 一意 ID |
| `kind` | enum | MUST | `gcc_sarif | gcc_json | compiler_stderr_text | linker_stderr_text | compiler_stdout_text | wrapper_trace | source_snippet | other` |
| `media_type` | string | MUST | 例: `application/sarif+json`, `text/plain` |
| `encoding` | string | MAY | 例: `utf-8`, `binary` |
| `digest_sha256` | string | SHOULD | 原 payload digest |
| `size_bytes` | integer | MAY | payload size |
| `storage` | enum | MUST | `inline | external_ref | unavailable` |
| `inline_text` | string | MAY | 小さな text payload の inline 本文 |
| `external_ref` | string | MAY | trace bundle 内部参照などの opaque ref |
| `produced_by` | `ToolInfo` | MAY | どのツールがこの artifact を生成したか |

#### `CaptureArtifact` に関する規則

- `storage = inline` の場合、`inline_text` SHOULD be present。
- `storage = external_ref` の場合、`external_ref` MUST be present。
- `storage = unavailable` はメタデータのみで payload 本体を保持していないことを意味する。

### 8.6 `IntegrityIssue`

| フィールド | 型 | 必須 | 意味 |
|---|---|---:|---|
| `severity` | enum | MUST | `error | warning | info` |
| `stage` | enum | MUST | `capture | parse | normalize | analyze | render` |
| `message` | string | MUST | 何が壊れたか |
| `provenance` | `Provenance` | MAY | 関連 capture がある場合の追跡情報 |
| `extensions` | map | MAY | 実装詳細 |

---

## 9. 診断ノードモデル

### 9.1 `DiagnosticNode`

`diagnostics[]` に入る top-level 要素、およびその `children[]` は、同じ `DiagnosticNode` 型を再帰的に使う。

| フィールド | 型 | 必須 | 意味 |
|---|---|---:|---|
| `id` | string | MUST | 文書内で一意 |
| `origin` | enum | MUST | `gcc | clang | linker | driver | wrapper | external_tool | unknown` |
| `origin_component` | string | MAY | 例: `cc1plus`, `collect2`, `ld.bfd` |
| `phase` | enum | MUST | `driver | preprocess | parse | semantic | instantiate | constraints | analyze | optimize | codegen | assemble | link | archive | unknown` |
| `severity` | enum | MUST | `fatal | error | warning | note | remark | info | debug | unknown` |
| `semantic_role` | enum | MUST | `root | supporting | help | candidate | path_event | summary | passthrough | unknown` |
| `classifier` | `DiagnosticClassifier` | MAY | compiler/wrapper 分類情報 |
| `message` | `MessageText` | MUST | 主メッセージ |
| `locations` | array<`Location`> | MAY | source location 群 |
| `children` | array<`DiagnosticNode`> | MAY | child diagnostics |
| `suggestions` | array<`Suggestion`> | MAY | fix-it / manual hint |
| `context_chains` | array<`ContextChain`> | MAY | include/macro/template/linker 等 |
| `symbol_context` | `SymbolContext` | MAY | linker/symbol 問題の補助情報 |
| `node_completeness` | enum | MUST | `complete | partial | passthrough | synthesized` |
| `provenance` | `Provenance` | MUST | 元データへの追跡情報 |
| `analysis` | `AnalysisOverlay` | MAY | ranking / summary / confidence |
| `fingerprints` | `FingerprintSet` | MAY | node 単位 fingerprint |
| `extensions` | map | MAY | compiler-specific 拡張 |

#### `semantic_role` の意味

- `root`: top-level 問題本体
- `supporting`: 通常の補助 note / context
- `help`: 修正のための明示ヒント
- `candidate`: overload candidate / candidate function 等
- `path_event`: analyzer 等の event
- `summary`: wrapper が追加した要約ノード
- `passthrough`: ほぼ raw text をそのまま保持するノード
- `unknown`: 分類不能

#### `node_completeness` の意味

- `complete`: node の主要事実が十分構造化されている
- `partial`: 一部が不明・欠落
- `passthrough`: 構造化より raw 再掲示を主とする
- `synthesized`: wrapper が独自に生成した node

### 9.2 `DiagnosticClassifier`

| フィールド | 型 | 必須 | 意味 |
|---|---|---:|---|
| `tool_code` | string | MAY | compiler/native error code, checker 名など |
| `warning_option` | string | MAY | 例: `-Wconversion` |
| `category` | string | MAY | compiler category / checker category |
| `checker_name` | string | MAY | analyzer/checker 名 |
| `documentation_ref` | string | MAY | option URL や docs ref |
| `native_tags` | array<string> | MAY | backend 固有タグ |

### 9.3 `MessageText`

| フィールド | 型 | 必須 | 意味 |
|---|---|---:|---|
| `raw_text` | string | MUST | adapter が取り出した原文メッセージ |
| `normalized_text` | string | MAY | 空白や quoting を正規化した意味等価表現 |
| `locale` | string | MAY | `C`, `en_US`, `ja_JP` 等 |

#### `raw_text` に関する規則

- `raw_text` は、その node 自体の message 内容であり、必ずしも source location prefix を含む必要はない。
- source location prefix や full raw line は `captures[]` + `provenance.raw_locators[]` 側で追跡する。
- `raw_text` は renderer の最終出力文ではない。分析要約は `analysis.headline` 側で持つ。

---

## 10. 位置・範囲モデル

### 10.1 設計方針

位置情報はこのプロジェクトの品質上、特に慎重に扱う。

- 表示 caret / highlight は `Location`
- 機械適用 edit は `TextEdit`
- `byte` と `display` を分ける
- 可能なら `native_column` と `native_column_unit` も保持する
- range の境界意味は曖昧なことがあるため、`boundary_semantics` を明示する

### 10.2 `Location`

| フィールド | 型 | 必須 | 意味 |
|---|---|---:|---|
| `id` | string | MUST | node 内で一意であることを推奨 |
| `file` | `FileRef` | MUST | 対象ファイル |
| `anchor` | `SourcePoint` | MAY | caret 位置 |
| `range` | `SourceRange` | MAY | highlight 範囲 |
| `role` | enum | MUST | `primary | secondary | related | context | edit_target | symbol_reference | symbol_definition | other` |
| `source_kind` | enum | MUST | `caret | range | token | insertion | expansion | generated | other` |
| `label` | string | MAY | `char *`, `candidate here` など |
| `ownership_override` | `OwnershipInfo` | MAY | file 由来の owner を上書きする場合のみ |
| `provenance_override` | `Provenance` | MAY | node 由来と異なる場合のみ |
| `source_excerpt_ref` | string | MAY | snippet artifact への参照 |

#### `Location` の規則

- `anchor` と `range` の少なくとも一方が MUST be present。
- `role = primary` は 0 個以上許す。  
  ただし renderer は `analysis.preferred_primary_location_id` を優先し、無ければ最初の `primary` を使う。
- `label` は location 自体に付く説明であり、message の代替ではない。

### 10.3 `FileRef`

| フィールド | 型 | 必須 | 意味 |
|---|---|---:|---|
| `path_raw` | string | MUST | compiler が出した元の path |
| `display_path` | string | MAY | 表示用 remap 後パス |
| `uri` | string | MAY | 正規化された file URI 等 |
| `path_style` | enum | SHOULD | `posix | windows | uri | virtual | unknown` |
| `path_kind` | enum | SHOULD | `absolute | relative | virtual | generated | unknown` |
| `ownership` | `OwnershipInfo` | MAY | user/vendor/system/generated 等 |
| `exists_at_capture` | boolean | MAY | capture 時点で存在が確認できたか |

### 10.4 `OwnershipInfo`

| フィールド | 型 | 必須 | 意味 |
|---|---|---:|---|
| `owner` | enum | MUST | `user | vendor | system | generated | tool | unknown` |
| `reason` | string | MUST | classification の根拠 |
| `confidence` | number | MAY | 0.0〜1.0 |

#### owner の意味

- `user`: ユーザーが通常編集する workspace code
- `vendor`: 依存ライブラリ、submodule、third-party tree など
- `system`: `/usr/include` 等の system headers / system libs
- `generated`: codegen 生成物
- `tool`: wrapper / compiler が生成した仮想 or 補助ファイル
- `unknown`: 分類不能

### 10.5 `SourcePoint`

| フィールド | 型 | 必須 | 意味 |
|---|---|---:|---|
| `line` | integer | MUST | 1-based line number |
| `column_origin` | integer | SHOULD | 0 または 1 が多い |
| `column_byte` | integer | MAY | byte 列 |
| `column_display` | integer | MAY | display 列 |
| `column_native` | integer | MAY | backend native unit の列 |
| `column_native_unit` | enum | MAY | `byte | display | utf16_code_unit | unicode_scalar | unknown` |

#### `SourcePoint` の規則

- `line` は 1 以上でなければならない。
- column 値は 0 以上または 1 以上のどちらでもよいが、その基準は `column_origin` で明示する。
- `column_byte` / `column_display` は両方なくてもよい。
- 値が不明なら省略し、推定しない。

### 10.6 `SourceRange`

| フィールド | 型 | 必須 | 意味 |
|---|---|---:|---|
| `start` | `SourcePoint` | MUST | 開始点 |
| `end` | `SourcePoint` | MUST | 終了点 |
| `boundary_semantics` | enum | MUST | `half_open | inclusive_end | point | unknown` |

#### `SourceRange` の規則

- `start` と `end` は同一ファイルを前提とする。
- `boundary_semantics = unknown` を許す。  
  **理由**: compiler の location 表示範囲は half-open と限らないため。
- edit では `half_open` を MUST とする（後述）。

---

## 11. Suggestion / Fix-It モデル

### 11.1 方針

提案は 2 種類ある。

1. **機械適用可能な edit 群**
2. **人間に対する修正ヒント**

両者を同じ `Suggestion` に入れてよいが、**edits の有無** と **applicability** で区別する。

### 11.2 `Suggestion`

| フィールド | 型 | 必須 | 意味 |
|---|---|---:|---|
| `id` | string | MUST | suggestion 一意 ID |
| `kind` | enum | MUST | `fixit | hint | link_action | docs | other` |
| `message` | string | MUST | 人間に見せる提案文 |
| `applicability` | enum | MUST | `machine_exact | machine_probable | manual | unsafe | unknown` |
| `confidence` | number | MAY | 0.0〜1.0 |
| `edits` | array<`TextEdit`> | MAY | 適用 edit 群 |
| `source` | enum | MUST | `compiler_native | adapter_derived | heuristic | policy | wrapper_generated` |
| `provenance` | `Provenance` | MUST | 提案の出所 |
| `extensions` | map | MAY | compiler-specific extra fields |

#### `applicability` の意味

- `machine_exact`: 自動適用に耐えると producer が判断
- `machine_probable`: 自動提案は可能だが適用前確認推奨
- `manual`: 人間に意味はあるが edit の正確適用は保証しない
- `unsafe`: 誤修正リスクが高い
- `unknown`: 不明

#### `Suggestion` の規則

- `applicability = machine_exact` の suggestion は、少なくとも 1 つの `edits` を SHOULD 持つ。
- `source = heuristic` の suggestion は、明確な保証ロジックがない限り `machine_exact` にしてはならない。
- 同一 suggestion 内の `edits[]` は **atomic set**とみなす。  
  すなわち、一部だけ適用しても意味が保たれるとは限らない。

### 11.3 `TextEdit`

| フィールド | 型 | 必須 | 意味 |
|---|---|---:|---|
| `file` | `FileRef` | MUST | 編集対象ファイル |
| `range` | `SourceRange` | MUST | **half-open** の置換範囲 |
| `replacement` | string | MUST | 置換文字列。空文字列は delete |
| `encoding` | string | MAY | 通常は `utf-8` |
| `order` | integer | MAY | 複数 edit の適用順ヒント |

#### `TextEdit` の規則

- `range.boundary_semantics` MUST be `half_open`。
- `replacement = ""` は削除を意味する。
- `start == end` かつ非空 `replacement` は挿入を意味する。
- 1 つの suggestion 内で、同一ファイル上の overlapping edits は MUST NOT。  
  （将来必要なら supersede で明示的に解禁する）

---

## 12. Context Chain モデル

### 12.1 なぜ chain を first-class にするか

C/C++ 診断で本当に辛いのは、原因が「今見ている行」にないことだ。  
その遠距離因果を first-class にしない限り、root cause UX は改善しない。

chain はそのための正規化ビューである。

### 12.2 `ContextChain`

| フィールド | 型 | 必須 | 意味 |
|---|---|---:|---|
| `id` | string | MUST | chain 一意 ID |
| `kind` | enum | MUST | `include | macro_expansion | template_instantiation | overload_candidates | analyzer_path | call_stack | linker_resolution | concept_constraints | module_import | other` |
| `completeness` | enum | MUST | `complete | truncated_by_tool | truncated_by_policy | derived_partial | unknown` |
| `frames` | array<`ContextFrame`> | MUST | chain 本体 |
| `summary` | string | MAY | 短い要約 |
| `source_node_ids` | array<string> | MAY | 元 child nodes への参照 |
| `extensions` | map | MAY | kind-specific 拡張 |

#### `ContextChain` の規則

- `frames[]` は **outermost / earliest visible cause → innermost / current manifestation** の順で並べる。
- `source_node_ids` を持つ場合、それらは同一 root diagnostic の descendant を指すこと。
- chain は additive であり、元 child node を消さない。

### 12.3 `ContextFrame`

| フィールド | 型 | 必須 | 意味 |
|---|---|---:|---|
| `ordinal` | integer | MUST | 0-based 順序 |
| `frame_role` | enum | MUST | `source | via | sink | candidate | reference | definition | callsite | callee | event | other` |
| `message` | string | MAY | frame 説明 |
| `location` | `Location` | MAY | frame の位置 |
| `entity_name` | string | MAY | macro 名, template entity 名, symbol 名など |
| `signature` | string | MAY | 関数シグネチャ等 |
| `object_name` | string | MAY | object/archive/header/module 等 |
| `object_kind` | enum | MAY | `object | archive | shared_object | header | macro | template | module | function | symbol | unknown` |
| `status` | string | MAY | kind-specific 状態値 |
| `source_node_id` | string | MAY | 対応する元 node |
| `extensions` | map | MAY | 詳細拡張 |

### 12.4 kind 別の標準的解釈

#### `include`
- `entity_name`: include された file/path
- `object_name`: include 元 file/path
- `frame_role`: 通常 `source` / `via` / `sink`

#### `macro_expansion`
- `entity_name`: macro 名
- `location`: expansion site または定義 site
- `status`: `definition`, `expansion`, `argument-substitution` などを許容

#### `template_instantiation`
- `entity_name`: instantiated entity 名
- `signature`: specialization の表示文字列
- `status`: `instantiated-from`, `required-from`, `substitution-failure`, `constraint-failure` などを許容

#### `overload_candidates`
- `frame_role`: `candidate`
- `status`: `viable`, `rejected`, `deleted`, `constrained_out`, `ambiguous`, `selected`, `unknown` などを許容

#### `analyzer_path`
- `frame_role`: `event`
- `message`: event description
- `location`: event site
- `status`: stack depth / event category を拡張で持ってよい

#### `linker_resolution`
- `entity_name`: symbol 名
- `frame_role`: `reference`, `definition`, `via`, `sink`
- `object_name`: object/archive/shared object 名
- `status`: `undefined-reference`, `multiple-definition`, `duplicate-symbol`, `archive-member`, `abi-mismatch` などを許容

---

## 13. Linker / Symbol Context

linker エラーは source span が弱く、symbol と binary input が主役になる。  
そのため `SymbolContext` を別 object として持つ。

### 13.1 `SymbolContext`

| フィールド | 型 | 必須 | 意味 |
|---|---|---:|---|
| `symbols` | array<`SymbolRecord`> | MUST | 関連 symbol 群 |
| `primary_symbol_index` | integer | MAY | 主 symbol の index |
| `extensions` | map | MAY | linker-specific 拡張 |

### 13.2 `SymbolRecord`

| フィールド | 型 | 必須 | 意味 |
|---|---|---:|---|
| `raw_name` | string | MUST | mangled を含む元名 |
| `demangled_name` | string | MAY | demangle 結果 |
| `role` | enum | MUST | `undefined_reference | multiple_definition | duplicate_symbol | unresolved_vtable | abi_mismatch | other` |
| `language` | enum | MAY | `c | c++ | fortran | unknown` |
| `reference_sites` | array<`Location`> | MAY | 参照元位置 |
| `definition_sites` | array<`Location`> | MAY | 定義位置 |
| `binary_inputs` | array<`BinaryInputRef`> | MAY | どの object/archive/lib に関与したか |
| `extensions` | map | MAY | linker-specific 拡張 |

### 13.3 `BinaryInputRef`

| フィールド | 型 | 必須 | 意味 |
|---|---|---:|---|
| `name` | string | MUST | 例: `main.o`, `libfoo.a(bar.o)` |
| `kind` | enum | MUST | `object | archive | shared_object | linker_script | unknown` |

---

## 14. Provenance モデル

### 14.1 目的

provenance は以下のために必要である。

- 元の compiler/linker 情報を失わないため
- 誤解析時に debug できるようにするため
- regression test で比較根拠を追えるようにするため
- CI/ローカルで raw fallback に戻れるようにするため

### 14.2 `Provenance`

| フィールド | 型 | 必須 | 意味 |
|---|---|---:|---|
| `source` | enum | MUST | `compiler_native | adapter_structural | heuristic | policy | wrapper_generated | user_supplied` |
| `capture_refs` | array<string> | MUST | 参照する capture IDs |
| `raw_locators` | array<`RawLocator`> | MAY | capture 内の位置 |
| `adapter_version` | string | MAY | adapter 実装版 |
| `tool_version` | string | MAY | backend version |
| `rule_ids` | array<string> | MAY | ルール ID / 推論 ID |
| `notes` | array<string> | MAY | provenance に関する補足 |

### 14.3 `RawLocator`

| フィールド | 型 | 必須 | 意味 |
|---|---|---:|---|
| `capture_id` | string | MUST | `captures[].id` を参照 |
| `locator_kind` | enum | MUST | `json_pointer | sarif_result | line_range | byte_range | opaque` |
| `locator` | string | MUST | locator 本体 |
| `extra` | map | MAY | line start/end など補助 |

#### `RawLocator` の例

- JSON Pointer: `/0/children/1`
- SARIF result index: `run[0].results[12]`
- line range: `lines:120-128`
- byte range: `bytes:4096-4318`

### 14.4 provenance の規則

- `compiler_native`: compiler/linker が直接表現した事実
- `adapter_structural`: compiler 事実から lossless もしくは mechanical に導いた構造
- `heuristic`: message 文言やパターンに基づく推論
- `policy`: ownership や ranking policy など設定依存の導出
- `wrapper_generated`: wrapper 自身が作った説明・エラー
- `user_supplied`: 将来の user annotation 用に予約

`heuristic` と `policy` は **fact として扱わない**。

---

## 15. Analysis Overlay

### 15.1 目的

analysis overlay は、core facts を消さずに次を表現する。

- root cause の強さ
- user code 優先度
- actionability
- family 分類
- summary / headline
- default view で何を畳むか
- document-wide episode / cascade 関係

### 15.2 `DocumentAnalysis`

| フィールド | 型 | 必須 | 意味 |
|---|---|---:|---|
| `policy_profile` | string | MAY | どの cascade policy profile を使ったか |
| `producer_version` | string | MAY | document-wide analyzer の版 |
| `episode_graph` | `EpisodeGraph` | MUST | group 間 relation と episode clustering |
| `group_analysis` | array<`GroupCascadeAnalysis`> | MUST | group ごとの cascade role / score / visibility floor |
| `stats` | `CascadeStats` | MUST | independent/follow-on/duplicate/uncertain の集計 |

#### `DocumentAnalysis` の規則

- facts が有効であるために `document_analysis` は必須ではない。
- `document_analysis` が存在する場合でも、consumer はこれを落として facts を扱えてよい。
- `group_analysis[*].group_ref` と `episode_graph.*_group_ref` は同一 document 内で解決できなければならない。

### 15.3 `EpisodeGraph`

| フィールド | 型 | 必須 | 意味 |
|---|---|---:|---|
| `episodes` | array<`DiagnosticEpisode`> | MUST | 独立 root episode とその member group 群 |
| `relations` | array<`EpisodeRelation`> | MUST | group 間の directed relation |

### 15.4 `DiagnosticEpisode`

| フィールド | 型 | 必須 | 意味 |
|---|---|---:|---|
| `episode_ref` | string | MUST | document 内で一意な episode ID |
| `lead_group_ref` | string | MUST | episode の lead group |
| `member_group_refs` | array<string> | MUST | episode に属する group 一覧 |
| `family` | string | MAY | coarse family |
| `lead_root_score` | number | MAY | lead group の root score |
| `confidence` | number | MAY | episode clustering の確信度 |

### 15.5 `GroupCascadeAnalysis`

| フィールド | 型 | 必須 | 意味 |
|---|---|---:|---|
| `group_ref` | string | MUST | logical group ID |
| `episode_ref` | string | MAY | 属する episode |
| `role` | enum | MUST | `lead_root | independent_root | follow_on | duplicate | uncertain` |
| `best_parent_group_ref` | string | MAY | 最有力 parent group |
| `root_score` | number | MAY | root らしさ |
| `independence_score` | number | MAY | 独立性の強さ |
| `suppress_likelihood` | number | MAY | hidden suppression の安全性 |
| `summary_likelihood` | number | MAY | summary compaction の適性 |
| `visibility_floor` | enum | MUST | `never_hidden | summary_or_expanded_only | hidden_allowed` |
| `evidence_tags` | array<string> | MAY | 判定根拠タグ |

### 15.6 `EpisodeRelation`

| フィールド | 型 | 必須 | 意味 |
|---|---|---:|---|
| `from_group_ref` | string | MUST | relation の source group |
| `to_group_ref` | string | MUST | relation の target group |
| `kind` | enum | MUST | `cascade | duplicate | context` |
| `confidence` | number | MUST | relation の確信度 |
| `evidence_tags` | array<string> | MAY | relation の根拠タグ |

### 15.7 `CascadeStats`

| フィールド | 型 | 必須 | 意味 |
|---|---|---:|---|
| `independent_root_count` | integer | MUST | independent root group 数 |
| `dependent_follow_on_count` | integer | MUST | follow-on group 数 |
| `duplicate_count` | integer | MUST | duplicate group 数 |
| `uncertain_count` | integer | MUST | uncertain group 数 |

### 15.8 `CascadePolicySnapshot`

`CascadePolicySnapshot` は renderer / analyzer 間で共有する resolved external policy surface であり、`DiagnosticDocument` 本体には埋め込まない。

| フィールド | 型 | 必須 | 意味 |
|---|---|---:|---|
| `compression_level` | enum | MUST | `off | conservative | balanced | aggressive` |
| `suppress_likelihood_threshold` | number | MUST | hidden suppression の最低 score |
| `summary_likelihood_threshold` | number | MUST | summary compaction の最低 score |
| `min_parent_margin` | number | MUST | parent 候補採用に必要な最小 margin |
| `max_expanded_independent_roots` | integer | MUST | default view で expanded にする独立 root 上限 |
| `show_suppressed_count` | enum | MUST | `auto | always | never`。TOML では `true`=`always`, `false`=`never` の shorthand を許容してよい |

### 15.9 `AnalysisOverlay`

| フィールド | 型 | 必須 | 意味 |
|---|---|---:|---|
| `family` | string | MAY | product 定義の coarse/fine family |
| `family_version` | string | MAY | family taxonomy の版 |
| `family_confidence` | number | MAY | 0.0〜1.0 |
| `root_cause_score` | number | MAY | 0.0〜1.0 |
| `actionability_score` | number | MAY | 0.0〜1.0 |
| `user_code_priority` | number | MAY | 0.0〜1.0 |
| `confidence` | number | MAY | overlay 全体の確信度 |
| `headline` | string | MAY | 短い要約 |
| `first_action_hint` | string | MAY | 最初に試す修正行動 |
| `preferred_primary_location_id` | string | MAY | renderer がまず見せる location |
| `collapsed_child_ids` | array<string> | MAY | default view で折りたたむ child node IDs |
| `collapsed_chain_ids` | array<string> | MAY | default view で折りたたむ chain IDs |
| `group_ref` | string | MAY | 複数 root を論理的に束ねる group ID |
| `reasons` | array<string> | MAY | ranking/summary に使った理由 |
| `policy_profile` | string | MAY | どの分析ポリシーを使ったか |
| `producer_version` | string | MAY | 分析器版 |

#### `AnalysisOverlay` の規則

- 数値スコアは 0.0〜1.0 の閉区間に収める。
- `headline` / `first_action_hint` は facts と矛盾してはならない。
- `analysis` が無くても facts は有効でなければならない。
- `group_ref` は **同一 document 内の node grouping**を主目的とし、build 全体 ID としては扱わない。

---

## 16. Fingerprint モデル

fingerprint は回帰テスト・telemetry clustering・差分比較に必要である。  
1 種類ではなく、**目的別に分ける**。

### 16.1 `FingerprintSet`

| フィールド | 型 | 必須 | 意味 |
|---|---|---:|---|
| `algorithm` | string | MUST | 例: `sha256` |
| `raw` | string | MAY | raw capture 依存の fingerprint |
| `structural` | string | MAY | normalized facts 依存の fingerprint |
| `family` | string | MAY | path/line の揺れに強い coarse clustering 用 |

### 16.2 定義

#### `raw`
以下を材料とした hash。

- 参照 capture payload
- raw locator
- tool version
- origin component

用途:
- exact 再現
- ingestion bug triage

#### `structural`
以下を材料とした canonical hash。

- origin
- phase
- severity
- semantic_role
- classifier
- message.raw_text
- locations
- child subtree
- context_chains
- symbol_context

**含めないもの**:
- `analysis.*`
- `extensions` の unknown namespace
- timestamp
- document_id
- capture storage ref

用途:
- regression test
- renderer / analysis の変更に左右されない比較

#### `family`
以下を材料とした coarse hash。

- `analysis.family`（ある場合）
- tool_code / warning_option / symbol 名 / message 正規化テンプレート
- primary owner class
- phase

**なるべく含めないもの**:
- line/column
- exact path
- raw child order の微差
- confidence

用途:
- corpus clustering
- telemetry aggregation

### 16.3 canonicalization の原則

- 直列化順序は canonical JSON を前提にする
- array order が意味を持つものはその順序を保持する
- omitted field と `null` を混同しない
- `family` fingerprint は意図的に情報を捨ててよい

---

## 17. Serialization ルール

### 17.1 直列化形式

v1alpha の canonical at-rest / on-wire 表現は **UTF-8 JSON** を推奨する。

- key 名は `snake_case`
- object key order は意味を持たない
- canonical snapshot では key を辞書順に整列する
- optional field は **未知/欠落なら省略**を基本とし、`null` を常用しない

### 17.2 array の順序

以下の配列順序は **意味を持つ**。

- `diagnostics[]`: backend emission order
- `children[]`: child emission order
- `locations[]`: backend order
- `suggestions[]`: producer order
- `context_chains[].frames[]`: causal/context order
- `symbols[]`: producer order（`primary_symbol_index` があるならそれが優先）

### 17.3 unknown field handling

consumer は以下を守る。

- unknown field は **MUST ignore**
- unknown enum 値は `unknown` と同等に degrade してよい
- unknown extension namespace は無視してよい

### 17.4 versioning

- `1.0.0-alpha.x` 内では **additive change only**
- 既存 field の意味変更は **MUST NOT**
- breaking change は `2.0.0` または新 namespace で導入
- `extensions` は escape hatch だが、core field の代替として乱用しない

---

## 18. Validation 規則

validation は schema check ではなく、**意味論チェック**を含む。

### 18.1 文書レベル

1. `document_id` MUST be non-empty.
2. `schema_version` MUST be parseable.
3. `diagnostics` が空の場合、`document_completeness` は `failed` または `passthrough` であるべき。
4. `captures[].id` は文書内で一意でなければならない。
5. `fingerprints.algorithm` がある場合、`raw/structural/family` はそのアルゴリズムで計算される。

### 18.2 node レベル

1. `DiagnosticNode.id` は文書内で一意でなければならない。
2. top-level `diagnostics[]` の各 node の `semantic_role` は `root` / `summary` / `passthrough` のいずれかであるべき。
3. child node が `semantic_role = root` を持つことは SHOULD NOT。
4. `message.raw_text` は空であってはならない。
5. `node_completeness = passthrough` の場合、`provenance.capture_refs` MUST be non-empty.
6. `node_completeness = synthesized` の場合、`provenance.source` は `wrapper_generated` または `policy` であるべき。
7. `analysis.preferred_primary_location_id` は存在する location を指さなければならない。
8. `analysis.collapsed_child_ids` は実在する descendant node IDs を指さなければならない。

### 18.3 location レベル

1. `Location` は `anchor` または `range` の少なくとも一方を持たなければならない。
2. `SourcePoint.line >= 1`
3. column 値がある場合、負であってはならない。
4. `Location.role = primary` が複数あってもよい。
5. `phase = parse | semantic | instantiate` かつ location が一つも無い node は、`node_completeness = complete` であるべきではない。  
   例外: driver 由来の転送メッセージなど。

### 18.4 edit / suggestion レベル

1. `TextEdit.range.boundary_semantics` MUST be `half_open`.
2. 同一 suggestion 内で、同一 file 上の overlapping edit は invalid。
3. `applicability = machine_exact` で `source = heuristic` は原則 invalid。  
   ただし将来 supersede で deterministic proof が定義された場合のみ例外。
4. `applicability = machine_exact | machine_probable` なら `confidence` SHOULD be present。

### 18.5 chain レベル

1. `frames[].ordinal` は 0 から単調増加であるべき。
2. `source_node_ids[]` がある場合、同一 root diagnostic の descendant を指すこと。
3. `kind = overload_candidates` の frame は `frame_role = candidate` を SHOULD use。
4. `kind = linker_resolution` で `entity_name` が空なのは SHOULD NOT。

### 18.6 provenance レベル

1. `capture_refs[]` は `captures[].id` を参照しなければならない。
2. `raw_locators[].capture_id` も同様に既存 capture を指す。
3. `heuristic` と `policy` 由来の情報を facts として `compiler_native` に偽装してはならない。

---

## 19. Normalization 規則

### 19.1 root diagnostic の単位

adapter は backend が独立に出した top-level diagnostics を 1:1 で root nodes に写像することを基本とする。

**MUST NOT**:
- message 類似性だけで複数 root を勝手に merge する
- note 群を root に昇格させる

**MAY**:
- `analysis.group_ref` で論理クラスタを示す

### 19.2 child diagnostics の扱い

- GCC / Clang / SARIF / JSON が child 構造を持つ場合、それを `children[]` に保持する。
- text parser が note 群を推定的に child に束ねる場合は `provenance.source = heuristic` または `adapter_structural` を適切に使い分ける。

### 19.3 location の扱い

- compiler が byte / display / native の複数列単位を出すなら保持する。
- 片方しかない場合は、分かる方だけ埋める。
- source を読んで display / byte を相互補完することは MAY だが、`provenance` で derived であることを示すこと。
- compiler の display range が inclusive か half-open か不明な場合は `boundary_semantics = unknown` とする。

### 19.4 chain 抽出

chain 抽出は次の順で行う。

1. backend が構造を持っているならそれを使う
2. child node 構造から mechanical に抽出する
3. text pattern から heuristically 抽出する
4. 抽出できなければ chain は作らず child/raw を残す

### 19.5 suggestion の扱い

- compiler native fix-it は `Suggestion.source = compiler_native`
- wrapper が mechanical に作る提案は `adapter_derived`
- heuristic 文言ベース提案は `heuristic`
- 組織ポリシーに基づく提案は `policy`

### 19.6 ownership の扱い

ownership は通常 path policy に基づくため、**facts ではなく policy-derived metadata**である。  
ただし UI の root cause ranking に重要なので core field として保持してよい。

### 19.7 partial / passthrough

機械可読取り込みが失敗した場合は以下を優先する。

1. raw text を capture する
2. 最小限の node を `semantic_role = passthrough` で作る
3. `ingestion_issues[]` に原因を記録する
4. raw provenance を保持する

---

## 20. Mapping ガイドライン

本節は adapter 実装者向けの規範的ガイドである。  
ここでは **「何を authoritative source とみなすか」**を明確にする。

### 20.1 GCC 15+ 推奨経路

GCC 15+ では、以下を推奨する。

1. compiler 実行時に text 出力を保持する
2. 同時に `-fdiagnostics-add-output=sarif:...` で SARIF を出す
3. **事実の取り込みは SARIF を一次ソース**とする
4. text は fallback / provenance / human diff 用として保持する

#### 理由

- JSON 系は deprecated
- `add-output` により text と SARIF を同時に得られる
- renderer が text parse に依存しなくなる
- raw fallback は温存できる

### 20.2 GCC 13–14 互換経路

GCC 13–14 では以下の優先度で扱う。

1. SARIF
2. JSON
3. text parser
4. passthrough

JSON を使う場合、以下の要素は core IR に比較的素直に写像できる。

- `kind` → `severity`
- `children[]` → `children[]`
- `locations[]` の `caret/start/finish` → `Location`
- `fixits[]` の `start/next/string` → `TextEdit`
- `path[]` → `ContextChain(kind=analyzer_path)`

### 20.3 GCC 12 以下

GCC 12 以下では structured path が弱い、または期待通りに揃わない可能性が高い。  
v1alpha では **無理な text parser を中核に据えない**。

推奨方針:

- support level は明確に narrower path として扱う
- passthrough を標準 fallback とする
- 必要なら限定的な text pattern parser を用いるが、`heuristic` として明示する

### 20.4 Clang 将来経路

Clang 対応時は以下を基本方針とする。

- fix-it は parseable fixits を優先
- source ranges は machine-parseable range info を優先
- `-fdiagnostics-show-template-tree` は renderer 補助情報または extension として扱い、core facts の一次ソースにはしない
- text 出力は provenance と fallback 用に保持する

### 20.5 Linker

linker 診断は compiler 診断と構造の質が違う。  
そのため adapter は以下を守る。

- `origin = linker`、`phase = link` を明示する
- source span が弱い場合でも無理に location を捏造しない
- symbol / object / archive を `SymbolContext` / `ContextChain(kind=linker_resolution)` に逃がす
- parse に失敗したら passthrough へ安全に落とす

### 20.6 Driver forwarding

`gcc` / `g++` driver が backend / linker 診断を転送する場合は、可能な限り `origin_component` で実発生元を残す。  
分からない場合は `driver` として保持し、偽らない。

---

## 21. v1alpha で標準化する enum 一覧

この節は実装間のブレを抑えるための一覧である。  
v1alpha 中は **既存値の意味変更を禁止**し、追加は additive に行う。

### 21.1 `origin`

- `gcc`
- `clang`
- `linker`
- `driver`
- `wrapper`
- `external_tool`
- `unknown`

### 21.2 `phase`

- `driver`
- `preprocess`
- `parse`
- `semantic`
- `instantiate`
- `constraints`
- `analyze`
- `optimize`
- `codegen`
- `assemble`
- `link`
- `archive`
- `unknown`

### 21.3 `severity`

- `fatal`
- `error`
- `warning`
- `note`
- `remark`
- `info`
- `debug`
- `unknown`

### 21.4 `semantic_role`

- `root`
- `supporting`
- `help`
- `candidate`
- `path_event`
- `summary`
- `passthrough`
- `unknown`

### 21.5 `Location.role`

- `primary`
- `secondary`
- `related`
- `context`
- `edit_target`
- `symbol_reference`
- `symbol_definition`
- `other`

### 21.6 `Location.source_kind`

- `caret`
- `range`
- `token`
- `insertion`
- `expansion`
- `generated`
- `other`

### 21.7 `SourceRange.boundary_semantics`

- `half_open`
- `inclusive_end`
- `point`
- `unknown`

### 21.8 `Suggestion.kind`

- `fixit`
- `hint`
- `link_action`
- `docs`
- `other`

### 21.9 `Suggestion.applicability`

- `machine_exact`
- `machine_probable`
- `manual`
- `unsafe`
- `unknown`

### 21.10 `ContextChain.kind`

- `include`
- `macro_expansion`
- `template_instantiation`
- `overload_candidates`
- `analyzer_path`
- `call_stack`
- `linker_resolution`
- `concept_constraints`
- `module_import`
- `other`

---

## 22. canonical rendering との責務分離

IR は **表示そのもの**を規定しない。  
ただし renderer の自由度を無制限にすると品質がぶれるため、最低限の責務分離を定義する。

### 22.1 IR が MUST 提供するもの

- root / child 構造
- 位置情報
- suggestion / fix-it
- chain
- provenance
- optional analysis overlay

### 22.2 renderer の責務

- root cause first 表示
- child / chain の折りたたみ
- terminal width 対応
- color / no-color 対応
- CI mode での冗長抑制
- raw fallback 導線

### 22.3 IR が MUST NOT 含むもの

- ANSI escape code
- 特定 terminal 幅前提の改行
- box drawing / glyph choice
- スタイルテーマ
- 色ポリシー

---

## 23. 最低限の実装規約

v1alpha 実装開始時点で、各コンポーネントは少なくとも以下を満たすべきである。

### 23.1 Adapter

- `DiagnosticDocument` を生成する
- `provenance.capture_refs` を埋める
- `document_completeness` を正しく設定する
- structured input があれば text parser を一次ソースにしない
- structured input が壊れた場合、`ingestion_issues[]` を残す

### 23.2 Enrichment

- facts を上書きしない
- `analysis.*` のみを付加する
- `heuristic` と `policy` を provenance で明示する
- root ranking の根拠を `analysis.reasons[]` に残せるようにする

### 23.3 Renderer

- analysis が無くても表示できる
- `passthrough` node を表示できる
- raw fallback を辿れる
- unknown enum / unknown extension に対して壊れない

### 23.4 Validator

- schema + semantic validation を行う
- canonical snapshot 生成ができる
- fingerprint 再計算ができる

---

## 24. 例示（概念モック）

以下は **概念例**であり、最終 JSON schema の完全サンプルではない。  
狙いは「どの情報がどの field に入るべきか」を固定することにある。

### 24.1 例1: 単純な構文エラー

```yaml
diagnostics:
  - id: d1
    origin: gcc
    phase: parse
    severity: error
    semantic_role: root
    node_completeness: complete
    message:
      raw_text: "expected ';' before '}' token"
    locations:
      - id: loc1
        role: primary
        source_kind: caret
        file:
          path_raw: "main.c"
        anchor:
          line: 12
          column_origin: 1
          column_display: 5
    suggestions:
      - id: sug1
        kind: hint
        message: "ここに ';' を追加"
        applicability: manual
        source: wrapper_generated
        provenance:
          source: wrapper_generated
          capture_refs: ["cap-sarif-1"]
    provenance:
      source: compiler_native
      capture_refs: ["cap-sarif-1"]
```

### 24.2 例2: 二項演算子の型不一致

```yaml
diagnostics:
  - id: d2
    origin: gcc
    phase: semantic
    severity: error
    semantic_role: root
    message:
      raw_text: "invalid operands to binary +"
    locations:
      - id: loc2-primary
        role: primary
        source_kind: caret
        file: { path_raw: "bad-binary-ops.c" }
        anchor: { line: 64, column_origin: 1, column_display: 23 }
      - id: loc2-lhs
        role: secondary
        source_kind: range
        label: "S {aka struct s}"
        file: { path_raw: "bad-binary-ops.c" }
        range:
          start: { line: 64, column_origin: 1, column_display: 10 }
          end:   { line: 64, column_origin: 1, column_display: 21 }
          boundary_semantics: unknown
      - id: loc2-rhs
        role: secondary
        source_kind: range
        label: "T {aka struct t}"
        file: { path_raw: "bad-binary-ops.c" }
        range:
          start: { line: 64, column_origin: 1, column_display: 25 }
          end:   { line: 64, column_origin: 1, column_display: 36 }
          boundary_semantics: unknown
    analysis:
      family: "c.semantic.invalid_binary_operands"
      headline: "左辺と右辺の型が '+' で加算できません"
      first_action_hint: "まず左辺と右辺の実型を確認してください"
      preferred_primary_location_id: "loc2-primary"
```

### 24.3 例3: C++ template instantiation failure

```yaml
diagnostics:
  - id: d3
    origin: gcc
    phase: instantiate
    severity: error
    semantic_role: root
    message:
      raw_text: "no matching function for call to 'foo(...)'"
    context_chains:
      - id: chain-template-1
        kind: template_instantiation
        completeness: complete
        summary: "この呼び出しに至る template instantiation chain"
        frames:
          - ordinal: 0
            frame_role: source
            entity_name: "bar<T>"
            location:
              id: l1
              role: context
              source_kind: caret
              file: { path_raw: "bar.hpp" }
              anchor: { line: 10, column_origin: 1, column_display: 3 }
          - ordinal: 1
            frame_role: via
            entity_name: "baz<U>"
            location:
              id: l2
              role: context
              source_kind: caret
              file: { path_raw: "baz.hpp" }
              anchor: { line: 42, column_origin: 1, column_display: 7 }
          - ordinal: 2
            frame_role: sink
            entity_name: "foo"
            location:
              id: l3
              role: primary
              source_kind: caret
              file: { path_raw: "main.cpp" }
              anchor: { line: 88, column_origin: 1, column_display: 14 }
    children:
      - id: d3cand1
        origin: gcc
        phase: instantiate
        severity: note
        semantic_role: candidate
        node_completeness: complete
        message:
          raw_text: "candidate function not viable: no known conversion from ..."
        provenance:
          source: compiler_native
          capture_refs: ["cap-sarif-1"]
    analysis:
      family: "cpp.template.no_matching_call"
      headline: "最終的に失敗したのは 'foo' 呼び出しですが、原因は上流の template instantiation 条件にあります"
      first_action_hint: "template chain の最後の 2 フレームと candidate note を先に確認してください"
      collapsed_child_ids: ["d3cand1"]
      collapsed_chain_ids: []
    provenance:
      source: compiler_native
      capture_refs: ["cap-sarif-1"]
```

### 24.4 例4: include / macro 連鎖

```yaml
diagnostics:
  - id: d4
    origin: gcc
    phase: semantic
    severity: error
    semantic_role: root
    message:
      raw_text: "expected expression before ')' token"
    context_chains:
      - id: chain-include-1
        kind: include
        completeness: complete
        frames:
          - ordinal: 0
            frame_role: source
            entity_name: "main.c"
          - ordinal: 1
            frame_role: via
            entity_name: "config.h"
          - ordinal: 2
            frame_role: sink
            entity_name: "generated_macros.h"
      - id: chain-macro-1
        kind: macro_expansion
        completeness: derived_partial
        frames:
          - ordinal: 0
            frame_role: source
            entity_name: "MY_WRAP"
            status: "definition"
          - ordinal: 1
            frame_role: via
            entity_name: "MY_WRAP"
            status: "expansion"
          - ordinal: 2
            frame_role: sink
            entity_name: "CALL_IMPL"
            status: "argument-substitution"
    analysis:
      family: "c.preprocessor.macro_expansion"
      headline: "現在の行ではなく、macro 展開の途中で不正なトークン列ができています"
      first_action_hint: "展開後の最終引数列を確認し、空引数または余分な ',' がないか見てください"
```

### 24.5 例5: linker undefined reference

```yaml
diagnostics:
  - id: d5
    origin: linker
    origin_component: "ld.bfd"
    phase: link
    severity: error
    semantic_role: root
    node_completeness: partial
    message:
      raw_text: "undefined reference to `Widget::run()'"
    symbol_context:
      primary_symbol_index: 0
      symbols:
        - raw_name: "_ZN6Widget3runEv"
          demangled_name: "Widget::run()"
          role: undefined_reference
          language: c++
          binary_inputs:
            - name: "main.o"
              kind: object
            - name: "libwidget.a(widget_impl.o)"
              kind: archive
    context_chains:
      - id: chain-link-1
        kind: linker_resolution
        completeness: derived_partial
        frames:
          - ordinal: 0
            frame_role: reference
            entity_name: "Widget::run()"
            object_name: "main.o"
          - ordinal: 1
            frame_role: via
            entity_name: "Widget::run()"
            object_name: "libwidget.a(widget_impl.o)"
            status: "candidate-definition-missing-or-not-linked"
    analysis:
      family: "link.undefined_reference"
      headline: "宣言は見えていても、最終リンクで `Widget::run()` の定義が見つかっていません"
      first_action_hint: "実装の未定義、ABI 不一致、リンク順、archive 抽出条件の順で確認してください"
    provenance:
      source: heuristic
      capture_refs: ["cap-linker-text-1"]
```

---

## 25. v1alpha における「絶対にやってはいけないこと」

以下は実装初期に陥りやすいが、将来品質を壊す判断である。

1. **text 表示の見た目から逆算して IR を作ること**
2. **diagnostic message の自然言語文字列を core 分類の唯一根拠にすること**
3. **child nodes を捨てて summary 文だけ残すこと**
4. **byte column と display column を 1 つの `column` に潰すこと**
5. **fix-it の range と display highlight の range を同一視すること**
6. **ownership を fact と偽ること**
7. **structured input があるのに text parser を優先すること**
8. **passthrough を failure と同一視して情報を捨てること**
9. **analysis が無いと renderer が壊れる設計にすること**
10. **compiler-specific extension に core renderer が依存すること**

---

## 26. テスト設計への含意

この仕様は、テスト戦略にも直接影響する。  
最低限、以下の 4 層テストを前提とする。

### 26.1 Adapter goldens

入力:
- GCC SARIF
- GCC JSON
- raw stderr
- linker stderr

出力:
- normalized `DiagnosticDocument`

評価:
- schema valid
- semantic valid
- provenance intact
- expected node/chain/suggestion counts

### 26.2 Canonical snapshot tests

- canonical JSON を生成して snapshot 比較する
- `analysis.*` を含む snapshot と除外 snapshot の 2 系統を持ってよい
- `structural` fingerprint が不必要に変化していないかを見る

### 26.3 Renderer contract tests

- 同じ IR から terminal / CI / no-color の各 renderer が expected shape を満たすか
- renderer は unknown enum / missing analysis / passthrough node に対して壊れないか

### 26.4 Compatibility matrix tests

- GCC 15+
- GCC 13–14
- GCC <=12 fallback
- future Clang adapter

少なくとも同一コーパスに対して、
- root count
- primary location count
- suggestion count
- chain count
- structural fingerprint stability
を比較する。

---

## 27. 実装上の推奨レイヤ分割

IR を安全に実装するため、コードベースも以下の crate / module 境界を推奨する。

1. `diag_core`
   - 型定義
   - validation
   - canonical serialization
   - fingerprints

2. `diag_adapter_gcc`
   - GCC SARIF / JSON / text 取り込み
   - mapping to `diag_core`

3. `diag_adapter_clang`（将来）
   - Clang parseable diagnostics 取り込み

4. `diag_enrich`
   - ranking
   - family classification
   - chain compression
   - action hint 生成

5. `diag_render`
   - terminal / CI / machine output

6. `diag_testkit`
   - corpus loader
   - golden helpers
   - fixture validation

重要なのは、**IR 型は renderer でも adapter でもなく core crate が所有する**こと。

---

## 28. 導入時の最小実装スコープ（MVP のための必須 subset）

v1alpha の full surface は広いが、MVP で最初に必須なのは以下である。

### 28.1 MUST implement

- `DiagnosticDocument`
- `DiagnosticNode`
- `Location`
- `TextEdit`
- `Provenance`
- `AnalysisOverlay`（最低限 `headline`, `first_action_hint`）
- `ContextChain.kind = include | macro_expansion | template_instantiation | linker_resolution`

### 28.2 MAY defer

- `module_import`
- `concept_constraints`
- `call_stack`
- `documentation_ref`
- `family` fingerprint
- `argv_redacted`
- `secondary_tools`
- 高度な demangle metadata

### 28.3 SHOULD defer until post-MVP

- public export schema
- editor-specific range annotations
- distributed trace bundle references
- organization-specific suppression metadata

---

## 29. 将来拡張のための予約方針

v1alpha で future-proofing のために予約する。

### 29.1 extension namespace

推奨 namespace 例:

- `extensions.gcc`
- `extensions.clang`
- `extensions.linker`
- `extensions.sarif`
- `extensions.internal`

### 29.2 reserved semantic families

以下は family taxonomy 設計用に予約してよい。

- `c.parse.*`
- `c.semantic.*`
- `cpp.template.*`
- `cpp.overload.*`
- `cpp.constraints.*`
- `pp.include.*`
- `pp.macro.*`
- `link.*`

### 29.3 future metadata

将来追加しうるが v1alpha では未規定:

- suppression / waiver info
- diagnostic code normalization map
- source excerpt inline payload policy
- symbol ABI metadata
- module BMI / PCM metadata

---

## 30. 仕様の Done 条件

この仕様書が「実装着手に十分」と言える条件を明示する。

以下が満たされれば、本仕様は v1alpha の実装契約として機能する。

1. adapter チームがこの仕様だけで `DiagnosticDocument` を生成できる
2. renderer チームが raw text parser に依存せず表示を作れる
3. test harness が schema/semantic validation と snapshot を作れる
4. future Clang adapter が core 型を変えずに追加可能である
5. passthrough / partial / failed の違いが明確である
6. provenance により raw へ辿れる
7. facts と analysis が分離されている

---

## 31. ADR 対応と post-MVP backlog

この仕様に関わる基線判断は、以下の ADR で固定済みである。

1. **VersionBand / SupportLevel / ProcessingPath**  
   `ADR-0026`, `ADR-0027`, `ADR-0028`, `ADR-0029`

2. **ownership policy**  
   `ADR-0015`

3. **trace bundle format / redaction**  
   `ADR-0016`

4. **public machine-readable output / SARIF egress**  
   `ADR-0012` と `ADR-0013`

以下は v1alpha 実装契約を阻害しない post-MVP backlog とする。

1. **family taxonomy v1 の外部公開粒度**
2. **demangler policy の製品契約化**

---

## 32. まとめ

この Diagnostic IR v1alpha の本質は、**「コンパイラ診断を prettier text にする」ための中間データではなく、compiler facts と product interpretation を分離しながら、root cause UX を支えるための長寿命コア契約**である点にある。

意思決定として重要なのは次の 7 点である。

1. **wrapper-first でも中心は IR**
2. **structured-first で text-first にしない**
3. **facts / analysis を分離する**
4. **tree と provenance を捨てない**
5. **range と fix-it を混同しない**
6. **unknown を許容し、partial / passthrough を first-class にする**
7. **compiler-specific extension を許しつつ core を汚さない**

この仕様を先に固定することで、以後の実装は
- adapter
- enrichment
- renderer
- validator/test harness

を並行に進められる。

---

## 付録 A: 参考にした公開仕様・文書

- GCC 15 Release Series — Changes, New Features, and Fixes
- GCC 15.2 Manual: Diagnostic Message Formatting Options
- GCC 14.2 Manual: Diagnostic Message Formatting Options
- Clang Compiler User’s Manual
- Clang documentation index（`-fdiagnostics-parseable-fixits`, `-fdiagnostics-print-source-range-info`, `-fdiagnostics-show-template-tree`）

---

## 付録 B: 初回実装時の最低限チェックリスト

### Adapter チェックリスト

- [ ] `document_completeness` を正しく出せる
- [ ] `captures[]` を残せる
- [ ] root / child を壊さない
- [ ] fix-it を `TextEdit` に落とせる
- [ ] column origin / byte / display を保持できる
- [ ] fallback 時に passthrough node を作れる

### Renderer チェックリスト

- [ ] analysis 無しで描画できる
- [ ] passthrough node を表示できる
- [ ] preferred primary location を使える
- [ ] child / chain の折りたたみができる
- [ ] raw provenance への導線を出せる

### Test Harness チェックリスト

- [ ] schema validation
- [ ] semantic validation
- [ ] canonical snapshot
- [ ] structural fingerprint stability
- [ ] GCC バージョン差異テスト
- [ ] linker passthrough テスト
