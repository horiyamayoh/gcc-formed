---
doc_role: current-authority
lifecycle_status: draft
audience: both
use_for: Current top-level vNext architecture decisions.
do_not_use_for: Historical GCC15-first baseline or superseded planning.
supersedes: []
superseded_by: []
---
> [!IMPORTANT]
> Authority: `current-authority` / `draft`
> Use for: Current top-level vNext architecture decisions.
> Do not use for: Historical GCC15-first baseline or superseded planning.

# gcc-formed Doctrine 準拠 vNext 変更設計書

- 文書種別: 変更設計書 / 実装移行設計 / Execution Model 前提文書
- 状態: Draft for approval
- 対象: `horiyamayoh/gcc-formed` (`main`, 2026-04-09 時点)
- 目的: Doctrine を満たすための「譲れないソフトウェアアーキテクチャ」と「そこへ至る具体変更」を固定する
- 想定読者: maintainer / reviewer / coding agent / future contributor

---

## 0. この文書の位置づけ

この文書は、上位の doctrine をそのまま実装に落とすための**橋渡し文書**である。  
ここで固定するのは次の 2 点だけである。

1. **あるべきソフトウェアアーキテクチャ**  
   何を中心に分離し、どこを共通化し、どこを GCC バージョン帯ごとに分けるか。
2. **そこへ至るための変更設計**  
   現在の repo に対して、どの文書・どのモジュール・どの契約を、どの順番で、どの不変条件の下で変更するか。

この文書は roadmap ではあるが、単なる願望リストではない。  
**Epics を切る前に承認されるべき、設計上の最後の土台**である。  
したがって本書承認前に Epic を増やしてはならない。最初にやるべきは **Execution Model の確立**であり、その後に Epic / Work Package を生成する。

---

## 1. エグゼクティブサマリー

### 1.1 結論

`gcc-formed` は、思想そのものはかなり正しい。  
現行 repo はすでに、IR を製品コアと見なし、adapter / renderer / quality gate を分離し、fail-open と raw provenance を重視している。README も「生出力を prettier にすること」より「wrapper・adapter・Diagnostic IR・renderer・quality gate を分離した実装可能な製品基線」を重視すると書いている。[R1]

しかし、**現在の具体的な契約と実装の軸が GCC 15 / SARIF / dual-sink に寄りすぎている**。  
公開 support boundary は GCC 15 を primary enhanced-render path として扱い、GCC 13/14 を狭い補助帯域として記述してきた。ingestion spec も GCC 15+ を単一路線の高 fidelity path、GCC 13/14 を標準 render 既定値の外に置いていた。[R2][R3]  
実装も `SupportTier::A/B/C` によって 15+ / 13–14 / それ未満を分け、旧 B tier の render は passthrough に落とし、capture runtime は `inject_sarif` による `-fdiagnostics-add-output=sarif:...` 注入を中心に組まれている。[R4][R5][R6]

このため、現行 repo は**「GCC 15 を最もきれいに扱うプロダクト」**としては一貫しているが、あなたが本当に欲しい  
**「GCC 9〜15 にわたり、程度差はあっても似た UX 原則で価値を返す wrapper」**  
とはまだズレている。

### 1.2 本書の最重要提案

vNext では設計の主語を「Tier」から外し、次の 3 層へ切り替える。

- **VersionBand**  
  `GCC15+` / `GCC13-14` / `GCC9-12`
- **CapabilityProfile**  
  `dual_sink`, `sarif`, `json`, `native_text`, `color_control`, `caret_control`, `fixits`, `locale_stabilization` などの具体能力
- **ProcessingPath**  
  `DualSinkStructured`, `NativeTextCapture`, `SingleSinkStructured`, `Passthrough`

重要なのは、**VersionBand と ProcessingPath を分離すること**である。  
GCC 13/14 と GCC 9–12 は「構造化出力が弱いから価値を返せない」のではない。  
むしろ、

- TTY 既定では `NativeTextCapture` を中心に、raw text を捕捉しつつ compaction / ranking / rendering で改善する
- 明示的 structured mode や CI profile では `SingleSinkStructured` を使って JSON/SARIF を読む
- GCC 15+ では `DualSinkStructured` が使えるため、最も安全に高品質 path へ行ける

という形で、**帯ごとに最適な capture path を持ちつつ、分析と表示は共通化する**のが正しい。

### 1.3 ここから先の実装方針

実装は次の順で進める。

1. **Execution Model を先に固定する**  
   仕様更新順、ADR の順、Issue taxonomy、nightly agent のガードレールを決める。
2. **契約を GCC 15 前提から capability / path 前提へ置き換える**
3. **no-behavior-change の抽象化リファクタリングを先に入れる**
4. **TTY default の UX 非劣化を先に直す**  
   特に色・長さ・raw disclosure・template/std:: compaction
5. **Path B (GCC 13–14) を first-class path にする**
6. **Path C (GCC 9–12) を first-class path にする**
7. **rulepack externalization と quality gate 再設計で安定化する**

---

## 2. Doctrine を満たすための譲れない設計原則

以下は doctrine をソフトウェア設計へ翻訳した**非交渉事項**である。  
ここを破る実装は、局所的に便利でも中長期では破綻する。

### 2.1 GCC 15 は privileged path であって、プロダクトの唯一中心ではない

GCC 15+ の dual-sink path は最も恵まれている。  
GCC 15 では `-fdiagnostics-add-output=` により text と SARIF を同時に扱え、JSON は deprecated で SARIF が推奨される。[G1]  
これは vNext でも使う。だが、それは**最良の path**であって**唯一の価値提供 path**ではない。

### 2.2 バージョン差は capture path で吸収し、UX 原則は共通化する

GCC 13 以降では `sarif-stderr` / `sarif-file` と `json-stderr` / `json-file` が使える。[G2]  
GCC 9〜12 では `-fdiagnostics-format=` は `text` と `json` が中心である。[G3]  
この差異は**capture path の差異**として扱うべきであり、ユーザー向け原則まで分断してはならない。

### 2.3 native GCC に負ける default は出荷禁止

native GCC には、少なくとも次の強みがある。

- 既定での短さ
- caret / source pointing
- TTY に応じた color
- 既存 build flow との親和性

GCC は `-fdiagnostics-color` で色を制御でき、`always / auto / never` を取る。[G4]  
wrapper が stderr を pipe にすると `auto` は色を失いやすい。  
この non-regression を設計で扱わない限り、ラッパーは「賢いが見にくい」に落ちる。

### 2.4 「構造化入力が不完全だから改善できない」は禁止

構造化入力の強さは**改善の上限**を左右するが、**価値提供の可否**を決めるものではない。  
GCC 13/14 や GCC 9–12 でも、raw text の捕捉、ownership-aware compaction、template/std:: noise suppression、root ranking、fallback honesty は実現できる。

### 2.5 facts / analysis / view model / theme を分離する

- **facts**: compiler / linker / wrapper が観測した事実
- **analysis**: family, confidence, root cause ranking, action hint
- **view model**: 表示順・折りたたみ・省略の結果
- **theme/layout**: 色・見た目・行組み

この 4 層を混ぜると、表示変更のたびに分析ロジックが壊れ、Path 差分のたびに renderer が壊れる。

### 2.6 rule はコード埋め込みより contract 化を優先する

現状の enrich / residual parser は既に役に立つが、family 判定や action hint が Rust 実装に直書きされているため、将来の書式変更や family 追加が高コストになる。[R7][R8]  
vNext では、**ルールの意味論を contract 化し、可能な範囲で外部 rulepack 化**する。

### 2.7 fallback は失敗ではなく shipped contract である

README と support boundary は raw fallback を shipped contract の一部と位置づけている。[R1][R2]  
この思想は残す。  
ただし vNext では fallback をもっと厳密に分ける。

- **UserFallback**: ユーザーに見せる安全経路
- **DebugFallback**: 解析用に残す補助経路
- **SourceFallback**: compiler-owned source へ戻る経路
- **SyntheticFallback**: raw text がなくても preserved structured source から保守的に戻る経路

---

## 3. 現状アーキテクチャのレビュー

この節は MECE に、現状の repo が doctrine とどこでズレるかを整理する。

> Historical baseline note:
> この節で引用する GCC15-first wording は旧 baseline の記述であり、現行 authority として再採用してはならない。

### 3.1 契約レベルのズレ

README は現在の support boundary を GCC 15 優先の enhanced-render path と、GCC 13/14 の狭い補助帯域として固定していた。[R1]  
`SUPPORT-BOUNDARY.md` も同様に、GCC 15 外では enhanced-render guarantees を境界外としていた。[R2]

さらに ingestion spec は、GCC 15+ のみを高 fidelity な wrapper path、GCC 13–14 を標準 render 既定値の外に置き、JSON を hot path core dependency にしない、と明記していた。[R3]  
bootstrap sequence も、backend resolution → capture runtime → GCC 15 shadow → SARIF parser → render → raw fallback の順で始めるよう固定している。[R9]

**評価**  
これは「最初の出荷範囲を狭く切る」という意味では誠実だが、vNext の doctrine から見ると狭すぎる。  
問題は、出荷範囲を狭く切ったこと自体ではなく、**設計契約そのものが GCC 15 をプロダクト本線として固定してしまっている**点にある。

### 3.2 ランタイム分岐のズレ

`diag_backend_probe` は `support_tier_for_major` で `15+ -> A`, `13/14 -> B`, それ以外 -> `C` を返し、`add_output_sarif_supported` は `major >= 15` で判定している。[R4]  
`diag_cli_front/src/mode.rs` は旧 B tier に対し、`Render` 要求時でも `tier_b_render_unsupported=passthrough` で passthrough を選び、notice でも GCC 13/14 を restricted mode と明記している。[R5]  
`diag_capture_runtime` では `CaptureRequest` が `inject_sarif: bool` を持ち、実行時には `-fdiagnostics-add-output=sarif:version=2.1,file=...` を注入する path を基本にしている。[R6]

**評価**  
ここでは `VersionBand`, `CapabilityProfile`, `ProcessingPath`, `SupportLevel` が 1 つの `SupportTier` に押し込まれている。  
そのため、Path B / Path C に必要な分岐は、仕様の外側に「対象外」として押し出される。

### 3.3 取り込み境界のズレ

`diag_adapter_gcc::ingest` は `sarif_path: Option<&Path>` と `stderr_text: &str` を受け、`sarif_path` があれば `from_sarif` を authoritative source として読む。失敗時は `failed_document(..., "failed to parse authoritative SARIF; preserving raw diagnostics: ...")` に落とす。[R10]  
つまり現在の adapter 境界は、概念上 `StructuredArtifact` 一般ではなく、**ほぼ SARIF 専用**である。

一方で IR spec 自体は、`compiler/linker stdout/stderr + SARIF/JSON/text -> adapter / ingestion -> core Diagnostic IR` と書いており、IR の設計はもっと広い。[R11]

**評価**  
IR は広い。だが ingest API は狭い。  
このミスマッチが、GCC 13/14 や GCC 9–12 に向けた伸びしろを削っている。

### 3.4 分析ロジックのズレ

`diag_enrich::enrich_document` は、各 node に対して `classify_family -> classify_confidence -> headline_for -> action_hint_for` を適用し、analysis overlay を埋める。[R7]  
テストからも、`invalid conversion from 'const char*' to 'int'` を `type_overload` family へ、`undefined reference` を linker family へ、passthrough node を conservative wrapper view へ振るなど、明示的な family / headline / action hint の規則が Rust コード側で保持されていることが分かる。[R7]

`diag_residual_text::classify` は raw stderr を Regex ベースで分類し、`undefined reference`, `multiple definition`, `cannot find -l`, assembler, collect2 など少数のパターンをまとめる。[R8]

**評価**  
これは PoC としては良い。  
だが doctrine に照らすと、次の弱点がある。

- family / action hint の保守コストが高い
- Path B / C の追加とともに if/else が増えやすい
- display wording の変更と family logic の変更が同じコード面に出やすい
- ownership-aware compaction をもっと強くしたいとき、ルールが散らばる

### 3.5 表示 UX のズレ

render API は `RenderCapabilities` として `stream_kind`, `width_columns`, `ansi_color`, `unicode`, `hyperlinks`, `interactive` を持つ。[R12]  
一方で、render tests では ANSI escape を含むメッセージや headline を渡したとき、出力に生の ESC を残さず `\\x1b[...]` にエスケープしていることが明示的に確認できる。[R13]  
また low-confidence 時には  
`note: wrapper confidence is low; verify against the preserved raw diagnostics`  
のような honesty notice を付け、partial 時には original diagnostics preservation notice を出し、linker などでは `why:` と `raw:` block を追加している。[R14]

**評価**  
honesty 自体は正しい。  
しかし、現状では **「どの場面でどれだけ長くなってよいか」** が budget contract として十分固定されていない。  
そのため、ユーザーの実観測どおり

- native GCC より長い
- color がなく見づらい
- noisy case で逆に情報量が増えうる

という印象が起こりうる。

### 3.6 品質運用のズレ

quality spec は「fidelity beats prettiness」「same input, same output, same verdict」を掲げる非常に良い文書である。[R15]  
しかし playbook は依然として  
「GCC 15 primary / GCC 13–14 compatibility」  
を guardrail に持ち、stable まで GCC 15 primary の線を維持すると書いている。[R16]

**評価**  
品質思想は正しいが、quality matrix が path abstraction 前提になっていない。  
その結果、「Path B / C でも default UX 非劣化を守る」という doctrine にまだ変換されていない。

---

## 4. vNext で採用する正式な概念モデル

### 4.1 用語

#### VersionBand

- `Gcc15Plus`
- `Gcc13_14`
- `Gcc9_12`
- `Unknown`

これは compiler family/version の帯を表す。  
**能力そのものではない。**

#### CapabilityProfile

wrapper が runtime で観測した具体能力。

```text
CapabilityProfile
- version_band
- structured_formats: { sarif, json }
- dual_sink_supported: bool
- file_sink_supported: bool
- stderr_structured_supported: bool
- raw_native_text_available: bool
- color_control_available: bool
- caret_control_available: bool
- locale_stabilization_safe: bool
- known_limitations: [...]
```

#### ProcessingPath

1 invocation をどう捕捉するか。

- `DualSinkStructured`
- `NativeTextCapture`
- `SingleSinkStructured`
- `Passthrough`

#### SupportLevel

ユーザーへの約束の強さ。

- `Primary`
- `Supported`
- `Conservative`
- `Experimental`
- `Unsupported`

#### FallbackGrade

fallback がどの程度 compiler-owned source に戻れるか。

- `NativeRaw`
- `StructuredSource`
- `ResidualText`
- `Synthetic`
- `PassthroughOnly`

### 4.2 重要原則

- `VersionBand` から直接 `SupportLevel` を決めない
- `CapabilityProfile` から `ProcessingPath` を選ぶ
- `SupportLevel` は user-facing promise であり、内部 path とは別軸にする
- 1 つの VersionBand に複数の `ProcessingPath` がありうる
- default path と explicit path を分ける
- TTY default / Pipe default / CI default を分ける

---

## 5. あるべきソフトウェアアーキテクチャ

### 5.1 全体像

```text
user / build system
        │
        ▼
[ CLI Front / Orchestrator ]
        │
        ├─ CapabilityProbe
        ├─ InvocationClassifier
        ├─ PathSelector
        └─ CapturePlanBuilder
        │
        ▼
[ CaptureRuntime ]
        ├─ Path A: DualSinkStructured      (GCC 15+)
        ├─ Path B1: NativeTextCapture      (GCC 13/14 default TTY)
        ├─ Path B2: SingleSinkStructured   (GCC 13/14 explicit/CI)
        ├─ Path C1: NativeTextCapture      (GCC 9–12 default TTY)
        └─ Path C2: SingleSinkStructured   (GCC 9–12 explicit/CI JSON)
        │
        ▼
[ CaptureBundle ]
        ├─ invocation_record
        ├─ native_text_artifacts[]
        ├─ structured_artifacts[]
        ├─ residual_artifacts[]
        ├─ integrity_issues[]
        └─ trace_refs[]
        │
        ▼
[ IngestMux / Normalizer ]
        ├─ adapter_sarif
        ├─ adapter_gcc_json
        ├─ adapter_residual_text
        └─ provenance merger
        │
        ▼
[ Core Diagnostic IR ]
        │
        ▼
[ Analysis Pipeline ]
        ├─ ownership classifier
        ├─ family classifier
        ├─ root ranking
        ├─ compaction
        ├─ action hint synthesis
        └─ confidence ceiling
        │
        ▼
[ RenderViewModel ]
        │
        ├─ TTY Layout
        ├─ Pipe/CI Layout
        ├─ Raw disclosure Layout
        └─ Debug Layout
        │
        ▼
[ Theme / Emitter ]
        ├─ ANSI theme
        ├─ Plain theme
        └─ Hyperlink policy
```

### 5.2 この構造で守られること

- Capture の差は path に閉じ込められる
- GCC 15 依存は `Path A` に局所化される
- 分析は `StructuredArtifact` の強弱に応じて confidence ceiling を変えるだけで共通化できる
- 表示変更は `RenderViewModel` と `Theme/Layout` に閉じる
- Path B / C 追加が renderer を壊しにくい

### 5.3 3 経路の正式設計

#### Path A: GCC 15+ (`DualSinkStructured`)

**目的**  
最も安全・高品質な主経路。  
text と SARIF を single-pass で同時取得する。

**使う機能**  
`-fdiagnostics-add-output=sarif:version=2.1,file=...` [G1]

**得られるもの**

- compiler-owned native text
- authoritative structured SARIF
- raw fallback の強さ
- external tool text の併存
- 高 confidence analysis

**UX 方針**

- TTY default では enhanced render を使う
- low confidence / partial / parse failure では raw disclosure を前面に出せる
- best effort ではなく primary path とする

#### Path B: GCC 13–14

Path B は 2 本持つ。  
これが vNext の重要な変更である。

##### Path B1: `NativeTextCapture`（default TTY / safe default）

**目的**  
raw native text を温存しながら、heuristic/ownership-aware 改善を返す。

**特徴**

- text を pipe capture する
- residual text parser と compaction を使う
- structured facts は使わない、または補助的にしか使わない
- fallback は `NativeRaw`

**価値**

- template/std:: ノイズ圧縮
- root cause ranking
- user-owned first
- terse summary
- raw disclosure への戻りやすさ

##### Path B2: `SingleSinkStructured`（explicit / CI / experimental-primary）

**目的**  
SARIF / JSON の構造を優先して richer facts を得る。

**特徴**

- `-fdiagnostics-format=sarif-file` または `json-file`
- native text fallback は失われる
- fallback は `StructuredSource` または `Synthetic`
- explicit mode として扱う

**価値**

- richer spans
- fix-it / structured children
- CI / non-interactive で強い

**設計上の判断**

GCC 13/14 は「旧 B tier だから render 不可」ではなく、  
**default TTY では B1、explicit/CI では B2** を使い分ける。  
これにより「安全性」と「構造化の恩恵」の両方を取りにいく。

#### Path C: GCC 9–12

Path C も 2 本持つ。

##### Path C1: `NativeTextCapture`（default）

**目的**  
最も保守的だが、少なくとも raw text より悪くしない。

**特徴**

- text capture
- residual parser
- ownership / compaction / ranking
- fallback は `NativeRaw`

##### Path C2: `SingleSinkStructured(JSON)`（explicit / CI）

**目的**  
JSON を使える場合に structured facts を得る。

**特徴**

- `-fdiagnostics-format=json` [G3]
- raw native text は失われうる
- structured parser の quality は Path A より低い
- confidence ceiling を厳しくする

**設計上の判断**

JSON は vNext では「禁じ手」ではない。  
ただし **primary doctrine を支える唯一ソースにしてはならない**。  
Path C2 は便利な explicit path であり、Path C1 を消してはならない。

---

## 6. vNext のデータ契約

### 6.1 `CapabilityProfile` を first-class にする

現在の `SupportTier` は情報量が少なすぎる。[R4][R5]  
最低限、以下を持たせる。

```rust
pub struct CapabilityProfile {
    pub version_band: VersionBand,
    pub support_level: SupportLevel,
    pub structured_formats: BTreeSet<StructuredFormat>,
    pub dual_sink_supported: bool,
    pub file_sink_supported: bool,
    pub stderr_structured_supported: bool,
    pub raw_native_text_available: bool,
    pub color_control_available: bool,
    pub caret_control_available: bool,
    pub locale_stabilization_safe: bool,
    pub recommended_default_path: ProcessingPath,
    pub allowed_paths: BTreeSet<ProcessingPath>,
}
```

### 6.2 `CapturePlan` を追加する

`mode + inject_sarif` では足りない。[R6]  
vNext では次を持つ。

```rust
pub struct CapturePlan {
    pub processing_path: ProcessingPath,
    pub requested_surface: UserSurfaceMode,
    pub raw_preservation: RawPreservationLevel,
    pub native_text_policy: NativeTextPolicy,
    pub structured_policy: StructuredPolicy,
    pub locale_policy: LocalePolicy,
    pub retention_policy: RetentionPolicy,
}
```

ここで `NativeTextPolicy` は色・caret・URL・nesting の preservation を含む。

### 6.3 `CaptureBundle` を ingest の唯一入口にする

`ingest(sarif_path, stderr_text, ...)` では狭い。[R10]  
vNext では次に統一する。

```rust
pub struct CaptureBundle {
    pub invocation: InvocationRecord,
    pub native_text_artifacts: Vec<TextArtifact>,
    pub structured_artifacts: Vec<StructuredArtifact>,
    pub residual_text_artifacts: Vec<TextArtifact>,
    pub integrity_issues: Vec<IntegrityIssue>,
    pub trace_refs: Vec<String>,
}
```

`StructuredArtifact` は少なくとも

- `Sarif`
- `GccJson`
- `UnknownJson`

を持つ。

### 6.4 `IngestReport` を導入する

現行の `IngestOutcome { document, fallback_reason }` では情報が薄い。[R10]  
vNext では

```rust
pub struct IngestReport {
    pub document: DiagnosticDocument,
    pub source_authority: SourceAuthority,
    pub confidence_ceiling: Confidence,
    pub fallback_grade: FallbackGrade,
    pub warnings: Vec<IntegrityIssue>,
}
```

を返す。  
これにより renderer / orchestrator は「どこまで断定してよいか」を path-aware に判断できる。

---

## 7. 分析アーキテクチャ

### 7.1 分析は 1 パスではなく段階的 pipeline にする

現状の `classify_family -> classify_confidence -> headline_for -> action_hint_for` だけでは、Path 差や ownership-aware compaction を十分表現しにくい。[R7]  
vNext では次の順に固定する。

1. **Ownership Pass**
2. **Structural Family Pass**
3. **Residual Family Pass**
4. **Root Ranking Pass**
5. **Compaction Pass**
6. **Action Hint Pass**
7. **Confidence Ceiling Pass**
8. **Suppression / Disclosure Pass**

### 7.2 ownership-aware compaction を first-class にする

これは doctrine 上の本丸である。  
template/std:: ノイズ抑制は「表示の好み」ではなく、**修正速度のための主要機能**として扱う。

具体的な compaction 規則:

- user-owned frame を先頭に出す
- system / vendor frames は既定で折りたたむ
- 展開チェインは「最初の user-owned 到達点」だけを first screen に出す
- `std::` / vendor namespace は first screen で全文を出さず、差分や責務だけを出す
- overload candidate 群は個別列挙ではなく cluster summary を作る
- same-label macro/include frames は dedup する
- compaction した事実は `collapsed_*` に残し、黙って捨てない

### 7.3 confidence は family ではなく source quality と evidence quality に依存させる

現状の enrich は family ベースで confidence を付けている。[R7]  
vNext では最低限、次を加味する。

- source authority (`Sarif > GccJson > ResidualText`)
- ownership classification の精度
- primary location の確からしさ
- note / chain の completeness
- fix-it の有無
- rule match の強さ
- path band

つまり、`undefined reference` でも  
Path A structured complete なら High、Path C residual only なら Medium/Low  
になりうるべきである。

### 7.4 rulepack を外出しする

完全な外部 DSL 化を急ぐ必要はない。  
ただし少なくとも次はデータ化すべきである。

- family rules
- headline templates
- first-action templates
- compaction rules
- confidence overrides
- suppression rules

推奨は `rules/*.yaml` または `rules/*.toml` で、build 時に canonical JSON へコンパイルし、`rulepack_version` を埋める形である。  
現行 IR にも `rulepack_version` の置き場はすでにある。[R11]

---

## 8. 表示アーキテクチャ

### 8.1 view model と theme/layout を分離する

現行 render は `selector::select_groups -> view_model::build -> formatter::emit` という良い芽をすでに持っている。[R12]  
vNext ではここを正式アーキテクチャに昇格する。

```text
DiagnosticDocument + AnalysisOverlay
    -> Selection
    -> RenderViewModel
    -> LayoutProfile
    -> ThemePolicy
    -> Emission
```

### 8.2 default TTY の MUST

- color を扱う
- 1 screen 目は短い
- first action を先頭に出す
- noisy details は折りたたむ
- raw disclosure への導線を持つ
- low confidence では断定を弱める

### 8.3 color 問題の根治

GCC の色は `-fdiagnostics-color=always/auto/never` で制御される。[G4]  
wrapper が stderr を pipe capture すると compiler 側の `auto` 判定は崩れやすい。  
少なくとも公開コード上では capture runtime に `-fdiagnostics-color` への明示処理を確認できず、render 側の tests では ANSI control sequence をそのまま通さず `\\x1b[...]` に escape することが明示されている。[R6][R13]

vNext では色を 2 層で扱う。

#### 層 1: native text preservation

`NativeTextCapture` path で TTY target のとき、user が明示的に色を殺していない限り、compiler 呼び出しに

- `-fdiagnostics-color=always`

を注入して色を保全する。  
これは raw disclosure や native-like compact view に使う。

#### 層 2: wrapper-owned theme

enhanced render では native raw ANSI に依存せず、wrapper 自身が theme を持つ。

```rust
pub enum ThemePolicy {
    Plain,
    AnsiBasic,
    AnsiRich,
}
```

色の具体 palette は別契約でよい。  
重要なのは、**analysis / view model と独立に theme を差し替えられること**である。

### 8.4 長さ問題の根治

長くなる主因は 3 つある。

1. honesty notice の追加
2. `why:` / `help:` / `raw:` の積み増し
3. noisy details の compaction 不足

vNext では `DisplayBudget` を first-class にする。

```rust
pub struct DisplayBudget {
    pub max_primary_lines: usize,
    pub max_evidence_lines: usize,
    pub max_context_lines: usize,
    pub raw_disclosure_mode: RawDisclosureMode,
}
```

TTY default は厳しく、CI / verbose は緩くする。  
**default profile では native GCC より明らかに長くならない**ことを merge gate で見る。

### 8.5 template/std:: noise suppression を正式要求にする

template / stdlib 問題は「よくなったらうれしい機能」ではなく、**このプロダクトの存在理由の一部**である。  
従って rendering/UX contract を改訂し、以下を MUST にする。

- first screen で system header 深掘りを出しすぎない
- template outer frames をそのまま長く並べない
- user-owned first corrective location を first screen に出す
- full raw chain は disclosure に回す

---

## 9. 品質アーキテクチャ

### 9.1 matrix を path-aware に作り直す

現行 quality 哲学は強いが、matrix が GCC 15 中心である。[R15][R16]  
vNext では次の軸で固定する。

- VersionBand: `15+ / 13-14 / 9-12`
- ProcessingPath: `DualSinkStructured / NativeTextCapture / SingleSinkStructured / Passthrough`
- Surface: `TTY / Pipe / CI`
- Family: `syntax / type_overload / template / include_macro / linker / passthrough`
- QualityConcern: `fidelity / brevity / ownership / color / fallback honesty`

### 9.2 新しい stop-ship

以下は出荷禁止。

- TTY default で native GCC より読みにくくなる既知 regression
- raw fallback より誤誘導率が高い improved render
- Path B / C で「価値を返す」と言いながら実質 passthrough-only
- template/std:: noisy case で非圧縮が known issue のまま default を上げる
- color regression が既知で放置されたまま TTY default を名乗る
- line budget の drift が gate されていない

### 9.3 新しい定量指標

- **first-screen line count ratio**
- **user-owned location first-hit rate**
- **template/std:: collapse ratio**
- **raw disclosure click-through need rate**
- **mislead rate vs raw fallback**
- **TTY color preservation rate**
- **fallback honesty correctness**

---

## 10. 具体的な変更設計


### 10.0 変更マップ一覧

| 領域 | 現状 | vNext 変更 | ねらい |
|---|---|---|---|
| バージョン判定 | `SupportTier` で 15+/13-14/その他 を直結 | `VersionBand + CapabilityProfile + SupportLevel` に分離 | GCC 15 偏重をやめる |
| mode 選択 | Tier から `Render/Shadow/Passthrough` を選ぶ | `CapturePlan` と `ProcessingPath` を選ぶ | Path B/C を first-class 化する |
| capture | `inject_sarif: bool` と `-fdiagnostics-add-output=` が中心 | `CaptureStrategy` 列挙に置換 | JSON / native text / dual-sink を同列化する |
| ingest | `ingest(sarif_path, stderr_text, ...)` | `ingest_bundle(CaptureBundle, ...)` | SARIF 専用入口をやめる |
| residual text | fallback 補助 | Path B/C の本線 parser | GCC 9–14 に価値を返す |
| enrich | family/headline/action がコード中心 | pipeline + rulepack 中心 | 保守性を上げる |
| render | plain text 主体、budget が弱い | view model + layout + theme + display budget | 色と長さの regression を止める |
| quality | GCC 15 中心 matrix | path-aware matrix | 全帯域を同じ原則で評価する |


### 10.1 まず変えるべき文書

#### 新設

1. `EXECUTION-MODEL.md`  
   仕様更新順、ADR batch、Issue taxonomy、agent-ready 条件を定義する
2. `SOFTWARE-ARCHITECTURE-vnext.md`  
   本書の圧縮版。実装参照用
3. `capability-and-processing-path-spec.md`
4. `rulepack-schema-spec.md`
5. `render-budget-and-disclosure-spec.md`

#### 改訂

1. `README.md`  
   GCC 15 primary の public wording を、path-aware wording に改める
2. `SUPPORT-BOUNDARY.md`  
   `support level` と `processing path` を分ける
3. `gcc-adapter-ingestion-spec.md`  
   SARIF authoritative 1 本から `CaptureBundle + IngestMux` 契約へ変更
4. `rendering-ux-contract-spec.md`  
   color, line budget, disclosure, template/std:: MUST を追加
5. `quality-corpus-test-gate-spec.md`  
   path matrix と non-regression budget を追加
6. `implementation-bootstrap-sequence.md`  
   GCC 15 shadow 起点ではなく、Execution Model -> abstraction -> UX hardening -> Path B -> Path C の順へ変更
7. `gcc_formed_milestones_agent_playbook.md`  
   legacy 扱いにし、Execution Model 承認後に再生成

### 10.2 `diag_backend_probe` の変更

**現状**  
`SupportTier` と `add_output_sarif_supported` が中心。[R4]

**変更**

- `SupportTier` を deprecated にする
- `CapabilityProfile` を返す
- `support_tier_for_major` を削除し、`capability_profile_for_major` へ置換する
- 判定は GCC version だけでなく observed flags/documented features に基づく struct とする

**DoD**

- downstream が `SupportTier` を参照しない
- CLI / capture / quality tests が `CapabilityProfile` 前提へ移行する

### 10.3 `diag_cli_front` の変更

**現状**  
`select_mode(tier, requested, hard_conflict)` が tier 中心。[R5]

**変更**

- `select_mode` を `select_capture_plan` へ置換
- 入力は `CapabilityProfile`, `RequestedMode`, `RequestedSurface`, `Conflicts`, `Policy`
- 出力は `CapturePlanDecision { plan, support_notice, fallback_expectation }`
- GCC 13/14 で render 要求時に即 passthrough としない
- Path B1 / B2, Path C1 / C2 を正式に選べるようにする

**DoD**

- `tier_b_render_unsupported=passthrough` ロジックが消える
- 互換 notice が path-aware wording に変わる

### 10.4 `diag_capture_runtime` の変更

**現状**  
`ExecutionMode` は `Render/Shadow/Passthrough`、`CaptureRequest` は `inject_sarif: bool` を持ち、`run_capture` は `-fdiagnostics-add-output=sarif:...` を注入する path を持つ。[R6]

**変更**

- `inject_sarif: bool` を削除し、`capture_strategy: CaptureStrategy` にする
- `CaptureStrategy` 例:
  - `PreserveNativeText`
  - `DualSinkSarifFile`
  - `SingleSinkSarifFile`
  - `SingleSinkJsonFile`
  - `SingleSinkJsonStderr`
  - `Passthrough`
- `NativeTextPolicy` を導入し、color / caret / urls の preservation を扱う
- TTY target の `NativeTextCapture` では `-fdiagnostics-color=always` の safe injection をサポートする
- `InvocationRecord` に `processing_path`, `fallback_grade`, `color_preservation_mode` を追加する

**DoD**

- Path A/B/C を runtime が明示的に実行できる
- native text preservation regression test が入る

### 10.5 `diag_adapter_gcc` の変更

**現状**  
`ingest(sarif_path, stderr_text, ...)` と `from_sarif(...)` が中心。[R10]

**変更**

- `ingest_bundle(bundle: &CaptureBundle, policy: IngestPolicy) -> IngestReport`
- 内部モジュールへ分割:
  - `sarif.rs`
  - `gcc_json.rs`
  - `residual.rs`
  - `merge.rs`
  - `provenance.rs`
- `SourceAuthority` を導入
- residual text も first-class 入口にする
- structured source が複数ある場合の merge 規則を定義する

**DoD**

- adapter API が SARIF 専用でなくなる
- Path B2 / C2 の parser が追加される
- Path B1 / C1 でも adapter を通る

### 10.6 `diag_residual_text` の変更

**現状**  
Regex ベースで linker / assembler など少数の residual grouping を実装している。[R8]

**変更**

- path B1 / C1 の first-class parser に昇格する
- family / symbol / location extraction を `CaptureBundle` 由来の provenance と結びつける
- ルールは `rulepack` と連携できるよう分離する
- regex 群を `rules/residual/*.yaml` 相当へ段階移行する

**DoD**

- residual parser が fallback 専用ではなく、本線 path の一部になる
- template/include/macro 用の 최소 heuristic が入る

### 10.7 `diag_enrich` の変更

**現状**  
`classify_family`, `headline_for`, `action_hint_for` をコードで適用している。[R7]

**変更**

- pipeline modules に分割:
  - `ownership`
  - `family`
  - `ranking`
  - `compaction`
  - `actions`
  - `confidence`
- `Rulepack` を読む層を追加
- `confidence ceiling` を path/source-aware にする
- template/std:: compaction を dedicated module として切り出す

**DoD**

- headline/action の大半が rulepack で変えられる
- Path B/C 追加で enrich の if/else が爆発しない

### 10.8 `diag_render` の変更

**現状**  
`RenderCapabilities` は `ansi_color` 等を持つが、tests では ANSI escape をエスケープして plain text 出力を前提にしている。[R12][R13]

**変更**

- `ThemePolicy` を導入
- `LayoutProfile` を導入
- `RenderRequest` に `display_budget`, `disclosure_policy`, `theme_policy` を追加
- formatter を
  - `view_model`
  - `layout`
  - `theme`
  - `emit`
 へ分ける
- default TTY で color を使えるようにする
- `raw:` block は disclosure policy に従って短縮表示する
- `why/help/raw` の line budget を profile ごとに制御する

**DoD**

- TTY default の no-color regression test が解消される
- long-output regression test が gate される
- simple case で native を下回らない snapshot が揃う

### 10.9 `diag_core` の変更

**現状**  
IR はかなり良い。`FallbackReason`, `Ownership`, `ContextChain`, `NodeCompleteness` などの型は残す価値が高い。[R17]

**変更**

- `SourceAuthority`
- `FallbackGrade`
- `VersionBand`
- `SupportLevel`
- `ProcessingPath`
- `CapabilityProfile`（もしくは別 crate）
- `CaptureBundle` / `StructuredArtifact`
- `DisplayBudget`
- `ThemePolicy`

を追加する。

**DoD**

- vNext の path-aware 契約を型で表現できる

---

## 11. 実行順序（移行の道筋）

### 11.1 Phase 0: Execution Model を先に作る

**これは Epic より前。**

やること:

1. 本書承認
2. `EXECUTION-MODEL.md` 作成
3. ADR batch 作成
4. 既存 playbook を legacy 扱いにする
5. 新しい Issue taxonomy / Project fields を定義する

**禁止**

- 先に大量の Epic を作る
- 先に Path B/C 実装へ飛び込む
- 先に rulepack 外出しへ飛び込む

### 11.2 Phase 1: no-behavior-change 抽象化

目的は GCC 15 前提の coupling をほどくこと。  
振る舞いを変える前に**責務境界**だけ変える。

やること:

- `SupportTier` -> `CapabilityProfile`
- `inject_sarif: bool` -> `CaptureStrategy`
- `ingest(...)` -> `ingest_bundle(...)`
- `select_mode(...)` -> `select_capture_plan(...)`

ここでは default behavior をまだ変えない。

### 11.3 Phase 2: TTY default UX hardening

ここで先にユーザー痛点を潰す。

最優先:

1. no-color regression
2. long-output regression
3. raw disclosure 冗長化
4. simple case 非劣化
5. template/std:: first-screen compaction

これは doctrine 上、Path B/C 実装より先にやる価値がある。  
理由は、Path を増やす前に**default UX の判断軸**を固定しないと、Path B/C でも同じ失敗を繰り返すからである。

### 11.4 Phase 3: Path B を正式実装

順序:

1. `NativeTextCapture` を Path B1 として本線化
2. residual parser + compaction の強化
3. explicit `SingleSinkStructured` を Path B2 として追加
4. quality gate を B1/B2 に張る

### 11.5 Phase 4: Path C を正式実装

順序:

1. Path C1 (`NativeTextCapture`) を本線化
2. JSON parser を adapter へ追加
3. explicit Path C2 (`SingleSinkStructured(JSON)`) を追加
4. confidence ceiling を厳格化

### 11.6 Phase 5: rulepack externalization

ここで初めて family / hint / compaction rules を外へ出す。  
抽象化前にやると chaos になるので後ろに置く。

### 11.7 Phase 6: support wording と default level を引き上げる

最後に初めて README / support boundary の promise を上げる。  
実装・quality gate が揃う前に wording を先に上げてはならない。

---

## 12. Execution Model を Epic より先にやる理由

あなたの指摘どおり、Epic より前に Execution Model をやるべきである。  
理由は 3 つある。

### 12.1 今回の変更は「機能追加」ではなく「設計軸の置換」だから

GCC 15 中心の tier logic を capability/path logic に置き換えるのは、単一 Epic の中で吸収できる話ではない。  
先に execution model を作らないと、Issue が旧前提のまま量産される。

### 12.2 nightly agent を回すなら、Issue の切り方が成果物の質を決めるから

この repo の理想開発形は nightly queue で coding agent に進めさせることだった。  
その場合、Prompt より**Work Package の境界**が重要になる。  
Execution Model を先に置かないと、夜間に wrong-direction PR を量産する。

### 12.3 文書正本の順番を誤ると手戻りコストが跳ねるから

本書レベルの変更では、少なくとも次の順が必要になる。

1. Change Design（本書）
2. Execution Model
3. ADR batch
4. Contract docs rewrite
5. Work Package generation
6. Epic generation
7. Nightly queue

この順を逆にしてはならない。

---

## 13. 承認後に最初に作るべき ADR

1. **ADR: Capability Profile replaces Support Tier**
2. **ADR: Processing Path is separate from Support Level**
3. **ADR: CaptureBundle becomes the only ingest entry**
4. **ADR: Path B and Path C are first-class product paths**
5. **ADR: Theme/Layout separated from analysis/view model**
6. **ADR: Native non-regression for TTY default**
7. **ADR: Rulepack externalization policy**
8. **ADR: Execution Model precedes Epic generation**

---

## 14. この変更設計のレビュー観点

本書承認時には、少なくとも次を Yes/No で判定する。

### 14.1 Yes でなければならないもの

- GCC 15 以外でも価値提供 path を持つ設計になっているか
- Path 差を renderer / analysis に漏らさない構造になっているか
- default TTY の非劣化が stop-ship 条件になっているか
- Execution Model が Epic より前にあるか
- quality gate が path-aware になっているか
- template/std:: suppression が存在理由として昇格しているか

### 14.2 No-Go になるもの

- Path B/C が依然として passthrough-only のまま
- support wording だけ先に広げる
- color/length regressions を known issue のまま default を上げる
- rulepack 外出しの前に if/else を積み増す
- nightly agent queue を旧 playbook のまま回す

---

## 15. 最終提言

現 repo は捨てる必要はない。  
だが **「GCC 15 / SARIF / dual-sink を中心とした single-track product」から、「複数 capture path を持つ IR-centered diagnostic platform」へ設計を置き換える必要がある。**

そのために最初にやるべきことは 3 つだけである。

1. **Execution Model を先に作る**
2. **Capability / Path / Support の概念分離を導入する**
3. **TTY default の非劣化を最優先で直す**

この 3 つができないなら、今後の実装量が増えるほど手戻りリスクが致命的になる。  
逆に、この 3 つができれば、nightly agent 開発とも非常に相性が良い。  
なぜなら、Path ごとの変更境界、rulepack の変更境界、render/theme の変更境界が明確になり、**1 Issue = 1 PR = 1 主目的** の形に分解しやすくなるからである。

---

## 16. 承認後の最初の 2 週間でやるべきこと

### Week 1

- 本書承認
- `EXECUTION-MODEL.md` 作成
- ADR 8 本起票
- `README.md` / `SUPPORT-BOUNDARY.md` の vNext rewrite 草案
- `implementation-bootstrap-sequence.md` の rewrite 草案
- `gcc_formed_milestones_agent_playbook.md` を legacy 化

### Week 2

- `CapabilityProfile` 導入 PR
- `CaptureStrategy` 導入 PR
- `CaptureBundle` 導入 PR
- TTY color regression を再現する test 追加
- line budget regression を再現する test 追加
- Path B1 skeleton 実装 PR

---

## 17. 参考: 本書の核心を一文で言うと

> **GCC 15 を特別扱いするのはよい。だが GCC 15 だけを“プロダクト”にしてはならない。**
>
> **Path は複数、UX 原則は 1 つ。capture は分ける、分析と表示は共通化する。**

---

## Appendix A. 根拠にした現行 repo 文書・実装

- [R1] README: <https://github.com/horiyamayoh/gcc-formed/blob/main/README.md>
- [R2] SUPPORT-BOUNDARY: <https://github.com/horiyamayoh/gcc-formed/blob/main/SUPPORT-BOUNDARY.md>
- [R3] GCC adapter / ingestion spec: <https://github.com/horiyamayoh/gcc-formed/blob/main/gcc-adapter-ingestion-spec.md>
- [R4] `diag_backend_probe/src/lib.rs` (`SupportTier`, `support_tier_for_major`, `add_output_sarif_supported`): <https://github.com/horiyamayoh/gcc-formed/blob/main/diag_backend_probe/src/lib.rs>
- [R5] `diag_cli_front/src/mode.rs` (`tier_b_render_unsupported=passthrough`, compatibility notices): <https://github.com/horiyamayoh/gcc-formed/blob/main/diag_cli_front/src/mode.rs>
- [R6] `diag_capture_runtime/src/lib.rs` (`CaptureRequest`, `inject_sarif`, `-fdiagnostics-add-output=...` injection): <https://github.com/horiyamayoh/gcc-formed/blob/main/diag_capture_runtime/src/lib.rs>
- [R7] `diag_enrich/src/lib.rs` (`enrich_document`, family/headline/action/confidence wiring): <https://github.com/horiyamayoh/gcc-formed/blob/main/diag_enrich/src/lib.rs>
- [R8] `diag_residual_text/src/lib.rs` (Regex-based residual classification): <https://github.com/horiyamayoh/gcc-formed/blob/main/diag_residual_text/src/lib.rs>
- [R9] implementation bootstrap sequence: <https://github.com/horiyamayoh/gcc-formed/blob/main/implementation-bootstrap-sequence.md>
- [R10] `diag_adapter_gcc/src/lib.rs` (`ingest`, `ingest_with_reason`, `from_sarif`): <https://github.com/horiyamayoh/gcc-formed/blob/main/diag_adapter_gcc/src/lib.rs>
- [R11] Diagnostic IR spec: <https://github.com/horiyamayoh/gcc-formed/blob/main/diagnostic-ir-v1alpha-spec.md>
- [R12] `diag_render/src/lib.rs` (`RenderCapabilities`, `RenderRequest`, `render`, `build_view_model`): <https://github.com/horiyamayoh/gcc-formed/blob/main/diag_render/src/lib.rs>
- [R13] `diag_render` tests escaping ANSI control sequences: same file as [R12]
- [R14] `diag_render` tests for low-confidence honesty notice / partial mixed fallback / `why:` + `raw:` rendering: same file as [R12]
- [R15] quality / corpus / gate spec: <https://github.com/horiyamayoh/gcc-formed/blob/main/quality-corpus-test-gate-spec.md>
- [R16] milestones agent playbook: <https://github.com/horiyamayoh/gcc-formed/blob/main/gcc_formed_milestones_agent_playbook.md>
- [R17] `diag_core/src/lib.rs` (`FallbackReason`, `Ownership`, `ContextChain`, etc.): <https://github.com/horiyamayoh/gcc-formed/blob/main/diag_core/src/lib.rs>

## Appendix B. GCC 公式情報

- [G1] GCC 15 changes (`-fdiagnostics-add-output=`, JSON deprecated, SARIF 推奨): <https://gcc.gnu.org/gcc-15/changes.html>
- [G2] GCC 13 diagnostic formatting options (`sarif-stderr`, `sarif-file`, `json-stderr`, `json-file`): <https://gcc.gnu.org/onlinedocs/gcc-13.2.0/gcc/Diagnostic-Message-Formatting-Options.html>
- [G3] GCC 9 diagnostic formatting options (`text` / `json`): <https://gcc.gnu.org/onlinedocs/gcc-9.5.0/gcc/Diagnostic-Message-Formatting-Options.html>
- [G4] GCC diagnostic color options (`-fdiagnostics-color=always/auto/never`): <https://gcc.gnu.org/onlinedocs/gcc-14.2.0/gcc/Diagnostic-Message-Formatting-Options.html>

## Appendix C. この文書から直接起こすべき最初の文書タスク

1. `EXECUTION-MODEL.md` を新設する
2. `SUPPORT-BOUNDARY.md` を capability/path-aware wording に改める
3. `gcc-adapter-ingestion-spec.md` を `CaptureBundle` 契約へ改訂する
4. `rendering-ux-contract-spec.md` に color / line budget / template/std:: MUST を追加する
5. `implementation-bootstrap-sequence.md` を vNext 向けに全面改訂する
6. `gcc_formed_milestones_agent_playbook.md` を廃止予定 or legacy 扱いにする
