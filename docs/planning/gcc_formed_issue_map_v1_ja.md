
---
doc_role: reference-only
lifecycle_status: draft
audience: both
use_for: Active execution-planning reference for issue decomposition and implementation sequencing.
do_not_use_for: Normative implementation contract or product support wording.
supersedes: []
superseded_by: []
---
> [!IMPORTANT]
> Authority: `reference-only` / `draft`
> Use for: Active execution-planning reference for issue decomposition and implementation sequencing.
> Do not use for: Normative implementation contract or product support wording.

# gcc-formed 設計書 v6
## Issue Map v1 / 1kL Work Package Program
### v5 正本を GitHub Issue 実行盤面へ変換するための実行設計書

作成日: 2026-04-11  
対象: `horiyamayoh/gcc-formed`  
親文書: `gcc_formed_final_brushup_v5_ja.md`

---

## 0. この文書の役割

v5 は doctrine / architecture / proof policy の正本である。  
この v6 は、その v5 を **GitHub Issues と GitHub Projects で回せる実行盤面** に変換するための設計書である。

言い換えると、役割分担は次のとおりである。

- **v5**: 何を正本とし、どういう品質で勝つかを定める。
- **v6**: それをどの issue 単位で、どの順序で、どの proof artifact を持って実装するかを定める。

この v6 の目的は 3 つだけである。

1. **issue を平均 1kL 前後の実装単位へ落とすこと**
2. **Codex に主実装を委任しやすい agent-ready issue を量産できること**
3. **品質の正本を issue tracker に奪われず、repo 内 ADOL / completeness proof を守ること**

---

## 1. 最終方針

この program の default 実装単位は **Bounded Work Package（BWP）** とする。  
BWP は、GitHub issue 1 枚で表現される、原則として Codex にそのまま渡せる実装単位である。

本 program では、BWP に対して次の中心線を採用する。

- **target review LoC（rLoC）: 700〜1300**
- **期待平均: 約 1000 rLoC**
- **通常の触る crate 数: 1〜2**
- **proof artifact: 1 個**
- **主たる acceptance gate: 1 系統**
- **lane の重心: 1 本**
- **family batch の重心: 1 塊**

運用 SLO も置く。
実装 issue（BWP / Architectural Reset / Bug）について、**直近 20 件の closed issue の平均 rLoC は 800〜1200 を維持** する。  
これを大きく外れたら、split / merge のルールを見直して issue 設計を補正する。

ここで重要なのは、「すべての issue を 1000 行に揃える」ことではない。  
重要なのは、**日常の主戦場を 1kL 前後にそろえ、巨大改造と小修正を例外として明確に管理すること** である。

---

## 2. 1kL の定義を曖昧にしない

### 2.1 `rLoC` を program metric とする

この文書では、issue の大きさを **review LoC = rLoC** で測る。  
`rLoC` は、PR マージ時点の review surface を表す。

### 2.2 `rLoC` の数え方

`rLoC` には次を含める。

- 手書き Rust コード
- schema / spec / ADR / docs
- corpus fixture
- rule / ledger source
- test / harness / CI config

`rLoC` からは次を除外する。

- 生成物
- lockfile だけの更新
- rename だけの変更
- snapshot 再生成だけで意味差のない churn
- formatting-only churn が大半を占める差分

### 2.3 なぜ `rLoC` か

Codex に効くのは file size ではなく **review surface** である。  
また、この repo は tests / docs / corpus / schema が品質そのものなので、source code だけを見ると実装量を過小評価する。  
したがって、本 program は `rLoC` を正式なサイズ指標とする。

---

## 3. issue の種類と予算

## 3.1 Root Epic

program 全体を表す唯一の親 issue。  
実装量を持たない。  
目的は progress visibility と doctrine 参照点の提供である。

- 期待 rLoC: 0〜150
- agent-ready: **no**
- owner: human
- proof artifact: なし

## 3.2 Epic

Epic は実装ではなく、**proof を持つ仕事の塊** である。  
この v6 では 2 種類だけ使う。

- **Foundation Epic**: lane をまたぐ基盤変更
- **Lane Epic**: 特定 lane 群の lock を進める変更

- 期待 rLoC: 0〜300
- agent-ready: **no**
- owner: human
- proof artifact: child issue 群の完了と completeness gate

## 3.3 Bounded Work Package（BWP）

この program の default issue 単位。  
Codex に主実装を委任するなら、基本的にこれを使う。

- 期待 rLoC: **700〜1300**
- 上限目安: **1800**
- crate 数: **1〜2**
- 親: Epic
- agent-ready: **yes**
- proof artifact: **1 個**
- acceptance gate: **1 系統**

## 3.4 Architectural Reset Issue

一度にやらないと意味がない cross-cutting redesign 専用。  
許容するが、濫用しない。  
この種別は **設計上どうしても原子的に入れる必要がある変更** にだけ使う。

- 期待 rLoC: **1500〜3500**
- 親: Foundation Epic
- agent-ready: 条件付き
- 必須条件:
  - human-owned design review 済み
  - sub-issue / execution slice を持つ
  - 1 回で壊し切る理由が issue body に明記されている

## 3.5 Execution Slice

BWP が oversize になったときだけ切る 2 段目。  
常設の基本単位ではない。  
**必要なときだけ** 使う。

- 期待 rLoC: 200〜700
- 親: BWP
- agent-ready: **yes**
- 使う条件:
  - 見積もりが 1800 rLoC を超える
  - public schema と runtime migration が同時に入る
  - 1 issue で proof artifact が複数になる
  - renderer / classifier / corpus を同時に大きく触る

## 3.6 Bug / Hotfix

1 行修正や明白な regression は小さくてよい。  
1kL は中心線であって、最小値ではない。

- 期待 rLoC: 1〜150
- agent-ready: 条件付き
- 使う条件:
  - 明白な不具合
  - acceptance が簡潔
  - program 構造を増やさない

---

## 4. split / merge ルール

### 4.1 split すべき条件

次のどれかに当たるなら、BWP を分割する。

- 見積もりが **1800 rLoC** を超える
- 触る runtime crate が **3 個以上**
- **public schema** と **generator** と **runtime migration** が同時に入る
- proof artifact が **2 個以上**
- lane が **2 本以上** で主従が曖昧
- family batch が **12 以上** で render profile が分かれる
- human design review を途中で挟まないと危険

### 4.2 merge してよい条件

次のすべてを満たすなら、小 issue を隣接 BWP に吸収してよい。

- 見積もりが **300 rLoC 未満**
- 同じ親 Epic
- 同じ lane
- 同じ proof artifact
- 同じ acceptance gate
- reviewer が変わらない

### 4.3 絶対にやってはいけないこと

- family 1 個 = issue 1 枚 を基本運用にする
- 逆に、lane 1 本全部 = issue 1 枚 にする
- docs / runtime / corpus / CI / render を全部 1 issue に詰める
- proof artifact が曖昧なまま `agent-ready` を付ける

---

## 5. GitHub での実行モデル

## 5.1 親子関係は 1 本だけにする

GitHub の sub-issues は親子の可視化に強いが、親は 1 本で考える方が盤面が壊れにくい。  
したがって、本 program では **各 issue は「どの proof owner にぶら下がるか」を最優先に親を選ぶ**。

つまり、

- cross-lane 基盤変更は Foundation Epic の子
- lane lock に効く family batch は Lane Epic の子
- oversized BWP だけが Execution Slice を子に持つ

とする。

## 5.2 階層の深さ

GitHub は深くネストできるが、この program では **4 層まで** に制限する。

1. Root Epic
2. Foundation / Lane Epic
3. BWP
4. Execution Slice（必要なときだけ）

## 5.3 Project は 1 枚

実行盤面は 1 project に統一する。  
おすすめ名称は `Zero Unknown Program` とする。

### 5.4 Project field

最低限必要なのは次の 10 個である。

- `Status`: Inbox / Triaged / Ready / Doing / Blocked / Review / Done
- `Stream`: Governance / ADOL / Probe / Normalize / Classify / Render / Corpus / Gate / Docs
- `Lane`: lane key 文字列
- `LockWave`: LW00 / LW10 / LW20 / LW30 / LW40 / LW50
- `Priority`: P0 / P1 / P2
- `Risk`: Low / Medium / High
- `Target rLoC`: XS / S / M / L / XL
- `Actual rLoC`: 数値
- `Proof Artifact`: 文字列
- `Owner Mode`: human / codex

### 5.5 label の使い方

label は安定軸だけに使う。  
`Status` や `Priority` は field に置く。

既存 label はかなり良いので、そのまま活かす。  
追加するなら最小限でよい。

- `area:adl`
- `area:probe`
- `area:normalize`
- `area:classify`
- `area:render`
- `area:corpus`
- `area:gate`
- `lang:c`
- `lang:cpp`

### 5.6 dependency の使い方

dependency は blocker にだけ使う。  
親子の代用にしてはいけない。

- 親子 = sub-issues
- blocker = dependency
- 関連 = issue body 参照

---

## 6. v5 から v6 へどう切るか

v5 は強いが、章ごとに issue 化するとまだ大きすぎる。  
そこで v6 では、v5 を次の 4 軸で切る。

1. **基盤か lane か**
2. **schema か runtime か**
3. **proof artifact が何か**
4. **Codex が一筆書きで完了できるか**

この基準で切ると、v5 の各章は次のように落ちる。

- v5 §3, §8, §13 → fingerprint / capability / capture 基盤 issue
- v5 §5, §9, §10, §11 → ADOL / taxonomy / policy 基盤 issue
- v5 §6, §7 → normalize / episode / classifier 基盤 issue
- v5 §14, §15, §18 → proof / anti-collision / gate 基盤 issue
- v5 §16 → lane epic と lock wave の順序
- v5 §17 → crate 別 BWP の切り口

---

## 7. lock wave

milestone は calendar ではなく **lock wave** に使う。  
名前は次で固定する。

- `LW00 Foundation`
- `LW10 Lock C core across GCC 9–15`
- `LW20 Lock C++03/11 core on GCC 9–12`
- `LW30 Lock C++14/17 core on GCC 13–15`
- `LW40 Lock C++20/23 advanced on GCC 13–15`
- `LW50 Lock warnings / toolchain / analyzer families`

この順序は v5 の「old GCC を後回しにしない」方針に一致する。  
Track A の C core を最初に閉じることで、最も有限で完璧に近い surface から証明能力を獲得する。

---

## 8. Root Epic と Epic catalog

## 8.1 Root Epic

### `PRG-000`
**Title**: `[epic] Program root: Zero Unknown across GCC 9–15`

**目的**  
program 全体の親。doctrine 参照点。進捗の最上位可視化。

**完了条件**  
- `LW00`〜`LW50` が完了
- 各 release lane に completeness report が出る
- README / AGENTS / CONTRIBUTING が新運用を参照

---

## 8.2 Foundation Epics

### `FEP-010`
**Title**: `[epic] Foundation: GitHub execution OS and issue discipline`

### `FEP-020`
**Title**: `[epic] Foundation: IR / ADOL / policy substrate`

### `FEP-030`
**Title**: `[epic] Foundation: toolchain fingerprint and capability inventory`

### `FEP-040`
**Title**: `[epic] Foundation: typed skeleton normalization`

### `FEP-050`
**Title**: `[epic] Foundation: episode-first classifier`

### `FEP-060`
**Title**: `[epic] Foundation: render profiles and degradation policy`

### `FEP-070`
**Title**: `[epic] Foundation: corpus, anti-collision, proof gates`

---

## 8.3 Lane Epics

### `LEP-110`
**Title**: `[epic] Lane lock: C core across GCC 9–15 reference lanes`

### `LEP-120`
**Title**: `[epic] Lane lock: C++03/11 core on GCC 9–12 reference lanes`

### `LEP-130`
**Title**: `[epic] Lane lock: C++14/17 core on GCC 13–15 reference lanes`

### `LEP-140`
**Title**: `[epic] Lane lock: C++20/23 advanced on GCC 13–15 reference lanes`

### `LEP-150`
**Title**: `[epic] Lane lock: toolchain / driver / linker / system families across all GCC`

### `LEP-160`
**Title**: `[epic] Lane lock: warnings and analyzer families across all GCC`

---

## 9. BWP catalog v1

この節が、この文書の主本体である。  
ここに並ぶ BWP が、最初に GitHub issue 化する原案である。

---

## 9.1 Foundation BWP

### `WP-001`
**Title**: `[wp] Extend RunInfo and ToolInfo with lane and fingerprint keys`

- Parent: `FEP-020`
- Stream: ADOL
- LockWave: `LW00`
- Target rLoC: M (800〜1200)
- Crates: `diag_core`, `diag_adapter_gcc`
- Goal:
  - `RunInfo` に `standard_mode`, `dialect_mode`, `strictness_profile`, `permissive_profile`, `locale_profile`, `fingerprint_class` を追加
  - `ToolInfo` に capability / vendor 由来の拡張点を追加
- Proof artifact:
  - updated IR schema examples
  - round-trip serialization tests
- Dependencies: なし

### `WP-002`
**Title**: `[wp] Add DiagnosticEpisode, EpisodeSatellite, ClassificationProvenance, and SeverityOrigin`

- Parent: `FEP-020`
- Stream: ADOL
- LockWave: `LW00`
- Target rLoC: M/L (1000〜1500)
- Crates: `diag_core`
- Goal:
  - episode-first architecture の最低限の object model を追加
  - `SeverityOrigin` と provenance を facts/analysis から分離
- Proof artifact:
  - IR schema diff
  - constructor / serde tests
- Dependencies: `WP-001`

### `WP-003`
**Title**: `[wp] Scaffold ADOL source tree, schema, and generated artifact boundaries`

- Parent: `FEP-020`
- Stream: ADOL
- LockWave: `LW00`
- Target rLoC: L (1300〜1800)
- Crates: `diag_rulepack`, `xtask`, `docs`
- Goal:
  - hand-edited `rules/*.json` を source-of-truth から降格
  - ADOL source tree と generator 境界を導入
  - checked-in generated artifacts の責務を定義
- Proof artifact:
  - `xtask` generator dry-run
  - source tree contract doc
- Dependencies: `WP-001`, `WP-002`

### `WP-004`
**Title**: `[wp] Implement toolchain fingerprint and capability probe pipeline`

- Parent: `FEP-030`
- Stream: Probe
- LockWave: `LW00`
- Target rLoC: M/L (1000〜1500)
- Crates: `diag_backend_probe`, `xtask`
- Goal:
  - compiler / target / sysroot / diagnostics capability を probe して `CapabilityProfile` を凍結
  - reference lane pinning に必要な fingerprint data を出す
- Proof artifact:
  - probe snapshot fixture
  - reference lane capability JSON
- Dependencies: なし

### `WP-005`
**Title**: `[wp] Add Product Capture vs Proof Capture profile selection`

- Parent: `FEP-030`
- Stream: Probe
- LockWave: `LW00`
- Target rLoC: M (800〜1200)
- Crates: `diag_capture_runtime`, `diag_cli_front`, `diag_backend_probe`
- Goal:
  - product capture と proof capture を分離
  - capability-aware profile selection を導入
- Proof artifact:
  - capture mode integration tests
  - proof capture sample bundle
- Dependencies: `WP-004`

### `WP-006`
**Title**: `[wp] Implement DiagnosticTextNormalizer phase 1: noise removal and tokenization`

- Parent: `FEP-040`
- Stream: Normalize
- LockWave: `LW00`
- Target rLoC: M (800〜1200)
- Crates: `diag_enrich`, `diag_residual_text`
- Goal:
  - formatting noise removal
  - tokenization / span segmentation
  - locale-stable normalization substrate
- Proof artifact:
  - normalizer corpus tests
  - before/after skeleton snapshots
- Dependencies: `WP-001`

### `WP-007`
**Title**: `[wp] Implement DiagnosticTextNormalizer phase 2: placeholderization and episode shape extraction`

- Parent: `FEP-040`
- Stream: Normalize
- LockWave: `LW00`
- Target rLoC: L (1300〜1800)
- Crates: `diag_enrich`, `diag_residual_text`
- Goal:
  - semantic placeholderization
  - episode shape extraction
  - typed skeleton emission
- Proof artifact:
  - skeleton fixture corpus
  - anti-regression tests against current six families
- Dependencies: `WP-006`

### `WP-008`
**Title**: `[wp] Introduce generic classifier engine and predicate/capture model`

- Parent: `FEP-050`
- Stream: Classify
- LockWave: `LW00`
- Target rLoC: L (1300〜1800)
- Crates: `diag_enrich`, `diag_rulepack`
- Goal:
  - current `match_family_rule` 型の hard-coded branching を generic classifier engine へ置換する土台を作る
  - predicate / capture / evidence model を導入
- Proof artifact:
  - classifier engine unit tests
  - rule compilation smoke tests
- Dependencies: `WP-003`, `WP-007`

### `WP-009`
**Title**: `[wp] Migrate current six families into ADOL-generated classifier rules`

- Parent: `FEP-050`
- Stream: Classify
- LockWave: `LW00`
- Target rLoC: M/L (1000〜1500)
- Crates: `diag_enrich`, `diag_rulepack`, `rules`
- Goal:
  - 既存の `linker`, `template`, `macro_include`, `type_overload`, `syntax`, `unknown/passthrough` 周辺を新基盤へ移植
- Proof artifact:
  - parity report against current behavior
  - existing regression suite green
- Dependencies: `WP-008`

### `WP-010`
**Title**: `[wp] Replace RendererFamilyKind with RenderProfileId and generated render policies`

- Parent: `FEP-060`
- Stream: Render
- LockWave: `LW00`
- Target rLoC: L (1200〜1700)
- Crates: `diag_rulepack`, `diag_render`
- Goal:
  - `RendererFamilyKind` 依存を外し、render profile ベースへ移行
  - family / render / severity / legality の分離を renderer に反映
- Proof artifact:
  - render policy compile tests
  - golden output diff review
- Dependencies: `WP-003`, `WP-009`

### `WP-011`
**Title**: `[wp] Add completeness_report.json emitter and PR/nightly/release gates`

- Parent: `FEP-070`
- Stream: Gate
- LockWave: `LW00`
- Target rLoC: M/L (1000〜1500)
- Crates: `diag_testkit`, `xtask`, `ci`
- Goal:
  - completeness proof artifact を CI/release 契約へ昇格
  - PR / nightly / release で gate を分ける
- Proof artifact:
  - sample `completeness_report.json`
  - CI gate demo
- Dependencies: `WP-003`, `WP-004`

### `WP-012`
**Title**: `[wp] Add anti-collision corpus schema and adversarial runner`

- Parent: `FEP-070`
- Stream: Corpus
- LockWave: `LW00`
- Target rLoC: M/L (1000〜1500)
- Crates: `diag_testkit`, `corpus`, `fuzz`
- Goal:
  - anti-collision corpus を first-class にする
  - adversarial mutation runner を正規工程へ入れる
- Proof artifact:
  - anti-collision fixture pack
  - negative match report
- Dependencies: `WP-011`

### `WP-013`
**Title**: `[wp] Add harvest-to-candidate pipeline for ADOL row discovery`

- Parent: `FEP-070`
- Stream: Corpus
- LockWave: `LW00`
- Target rLoC: M (800〜1200)
- Crates: `diag_trace`, `diag_testkit`, `xtask`
- Goal:
  - harvested trace から candidate family / skeleton / lane drift を抽出し、triage-ready な材料を生成
- Proof artifact:
  - candidate ledger seed report
  - trace clustering smoke test
- Dependencies: `WP-004`, `WP-007`

### `WP-014`
**Title**: `[wp] Bootstrap GitHub execution OS: project fields, issue forms, saved views, milestone migration`

- Parent: `FEP-010`
- Stream: Governance
- LockWave: `LW00`
- Target rLoC: M (700〜1100)
- Crates: `.github`, `docs`
- Goal:
  - issue form / template
  - project field 定義
  - saved view 指針
  - legacy milestone から lock wave への移行手順
- Proof artifact:
  - checked-in templates
  - operations doc
- Dependencies: なし

### `WP-015`
**Title**: `[wp] Update AGENTS, CONTRIBUTING, and README to the v5/v6 execution model`

- Parent: `FEP-010`
- Stream: Docs
- LockWave: `LW00`
- Target rLoC: S/M (500〜900)
- Crates: `AGENTS.md`, `CONTRIBUTING.md`, `README.md`, `docs`
- Goal:
  - repo entrypoints を新 doctrine と issue discipline に同期
- Proof artifact:
  - docs diff reviewed
- Dependencies: `WP-014`

---

## 9.2 Track A: C core across GCC 9–15

### `WP-101`
**Title**: `[wp] Seed C preprocessing families batch 1 across GCC 9–15`

- Parent: `LEP-110`
- Stream: Classify
- LockWave: `LW10`
- Lane: `c@reference@gcc9-15`
- Target rLoC: M/L (1000〜1500)
- Crates: `rules`, `diag_enrich`, `diag_residual_text`, `corpus`
- Scope examples:
  - `include_not_found`
  - include-chain surface errors
  - directive structure mismatches
  - macro expansion preprocessor failures that share a render shape
- Proof artifact:
  - C preprocessing completeness delta report
- Dependencies: `WP-008`, `WP-010`, `WP-012`

### `WP-102`
**Title**: `[wp] Seed C name and type families batch 1 across GCC 9–15`

- Parent: `LEP-110`
- Stream: Classify
- LockWave: `LW10`
- Lane: `c@reference@gcc9-15`
- Target rLoC: M/L (1000〜1500)
- Crates: `rules`, `diag_enrich`, `corpus`
- Scope examples:
  - undeclared identifier
  - unknown type name
  - incomplete type / storage size unknown
  - unknown field/member families with shared render profile
- Proof artifact:
  - C name/type completeness delta report
- Dependencies: `WP-008`, `WP-010`, `WP-012`

### `WP-103`
**Title**: `[wp] Seed C declaration and call families batch 1 across GCC 9–15`

- Parent: `LEP-110`
- Stream: Classify
- LockWave: `LW10`
- Lane: `c@reference@gcc9-15`
- Target rLoC: M/L (1000〜1500)
- Crates: `rules`, `diag_enrich`, `corpus`
- Scope examples:
  - implicit function declaration
  - conflicting types
  - too few / too many arguments
  - incompatible declaration / redeclaration families
- Proof artifact:
  - C decl/call completeness delta report
- Dependencies: `WP-008`, `WP-010`, `WP-012`

### `WP-104`
**Title**: `[wp] Seed C expression, conversion, and control families batch 1 across GCC 9–15`

- Parent: `LEP-110`
- Stream: Classify
- LockWave: `LW10`
- Lane: `c@reference@gcc9-15`
- Target rLoC: M/L (1000〜1500)
- Crates: `rules`, `diag_enrich`, `corpus`
- Scope examples:
  - assignment / return type mismatch
  - pointer/integer conversion mismatch
  - invalid operands
  - control-flow misuse families sharing syntax-ish render profiles
- Proof artifact:
  - C expr/control completeness delta report
- Dependencies: `WP-008`, `WP-010`, `WP-012`

### `WP-105`
**Title**: `[wp] Lock proof for C core reference lanes across GCC 9–15`

- Parent: `LEP-110`
- Stream: Gate
- LockWave: `LW10`
- Lane: `c@reference@gcc9-15`
- Target rLoC: M (800〜1200)
- Crates: `diag_testkit`, `corpus`, `ci`
- Goal:
  - Track A families を locked lane へ昇格する
  - unknown / collision / fallback 条件を gate 化する
- Proof artifact:
  - lane-specific `completeness_report.json`
- Dependencies: `WP-101`, `WP-102`, `WP-103`, `WP-104`

---

## 9.3 Track B: C++03/11 core on GCC 9–12

### `WP-201`
**Title**: `[wp] Seed C++03/11 lookup and include families on GCC 9–12`

- Parent: `LEP-120`
- Stream: Classify
- LockWave: `LW20`
- Lane: `cpp03_11@gcc9-12@reference`
- Target rLoC: M/L (1000〜1500)
- Crates: `rules`, `diag_enrich`, `corpus`
- Scope examples:
  - undeclared identifier in scope
  - namespace/member lookup failures
  - include-driven symbol miss families
- Proof artifact:
  - C++03/11 lookup completeness delta report
- Dependencies: `WP-102`, `WP-105`

### `WP-202`
**Title**: `[wp] Seed C++03/11 overload and candidate-note families on GCC 9–12`

- Parent: `LEP-120`
- Stream: Classify
- LockWave: `LW20`
- Lane: `cpp03_11@gcc9-12@reference`
- Target rLoC: M/L (1000〜1500)
- Crates: `rules`, `diag_enrich`, `diag_render`, `corpus`
- Scope examples:
  - no matching function
  - candidate note compression
  - conversion/ranking failures with shared render profile
- Proof artifact:
  - overload render regression pack
- Dependencies: `WP-010`, `WP-105`

### `WP-203`
**Title**: `[wp] Seed C++03/11 template deduction and core template-note families on GCC 9–12`

- Parent: `LEP-120`
- Stream: Classify
- LockWave: `LW20`
- Lane: `cpp03_11@gcc9-12@reference`
- Target rLoC: L (1200〜1700)
- Crates: `rules`, `diag_enrich`, `diag_render`, `corpus`
- Scope examples:
  - template argument deduction failures
  - substitution-related note chains
  - root cause extraction from candidate/template note stacks
- Proof artifact:
  - template trace compression pack
- Dependencies: `WP-007`, `WP-010`, `WP-105`

### `WP-204`
**Title**: `[wp] Add C++03/11 policy overlays and lock proof on GCC 9–12`

- Parent: `LEP-120`
- Stream: Gate
- LockWave: `LW20`
- Lane: `cpp03_11@gcc9-12@reference`
- Target rLoC: M (900〜1300)
- Crates: `rules`, `diag_testkit`, `corpus`, `ci`
- Goal:
  - `-fpermissive`, `-pedantic-errors` などの severity/policy 差を overlay 化
  - Track B を locked lane へ昇格
- Proof artifact:
  - lane completeness report for C++03/11 GCC9-12
- Dependencies: `WP-201`, `WP-202`, `WP-203`

---

## 9.4 Track C: C++14/17 core on GCC 13–15

### `WP-301`
**Title**: `[wp] Seed C++14/17 auto, decltype, initializer, and CTAD-adjacent families`

- Parent: `LEP-130`
- Stream: Classify
- LockWave: `LW30`
- Lane: `cpp14_17@gcc13-15@reference`
- Target rLoC: M/L (1000〜1500)
- Crates: `rules`, `diag_enrich`, `corpus`
- Scope examples:
  - `auto` deduction failures
  - `decltype` misuse
  - initializer-list based resolution failures
  - CTAD-adjacent note patterns where available
- Proof artifact:
  - C++14/17 deduction pack
- Dependencies: `WP-008`, `WP-010`, `WP-105`

### `WP-302`
**Title**: `[wp] Seed C++14/17 constexpr, lambda, and modern template families`

- Parent: `LEP-130`
- Stream: Classify
- LockWave: `LW30`
- Lane: `cpp14_17@gcc13-15@reference`
- Target rLoC: M/L (1000〜1500)
- Crates: `rules`, `diag_enrich`, `diag_render`, `corpus`
- Scope examples:
  - constexpr evaluation core failures
  - lambda capture/signature surface
  - class/function template modern note stacks
- Proof artifact:
  - constexpr/lambda regression pack
- Dependencies: `WP-203`

### `WP-303`
**Title**: `[wp] Lock proof for C++14/17 reference lanes on GCC 13–15`

- Parent: `LEP-130`
- Stream: Gate
- LockWave: `LW30`
- Lane: `cpp14_17@gcc13-15@reference`
- Target rLoC: M (800〜1200)
- Crates: `diag_testkit`, `corpus`, `ci`
- Goal:
  - Track C を locked lane へ昇格
- Proof artifact:
  - lane completeness report for C++14/17 GCC13-15
- Dependencies: `WP-301`, `WP-302`

---

## 9.5 Track D: C++20/23 advanced on GCC 13–15

### `WP-401`
**Title**: `[wp] Seed C++20/23 concepts and constraints families`

- Parent: `LEP-140`
- Stream: Classify
- LockWave: `LW40`
- Lane: `cpp20_23@gcc13-15@reference`
- Target rLoC: L (1300〜1800)
- Crates: `rules`, `diag_enrich`, `diag_render`, `corpus`
- Scope examples:
  - concept unsatisfied
  - requires-expression invalid
  - constraint trace root cause extraction
  - context-chain condensation
- Proof artifact:
  - concepts/constraints regression pack
- Dependencies: `WP-007`, `WP-010`, `WP-303`

### `WP-402`
**Title**: `[wp] Seed C++20/23 constexpr, consteval, and constinit families`

- Parent: `LEP-140`
- Stream: Classify
- LockWave: `LW40`
- Lane: `cpp20_23@gcc13-15@reference`
- Target rLoC: M/L (1000〜1500)
- Crates: `rules`, `diag_enrich`, `corpus`
- Scope examples:
  - immediate function misuse
  - constant initialization failures
  - compile-time evaluation families with shared render shape
- Proof artifact:
  - consteval/constinit pack
- Dependencies: `WP-302`

### `WP-403`
**Title**: `[wp] Add C++20/23 compatibility and policy overlays`

- Parent: `LEP-140`
- Stream: Classify
- LockWave: `LW40`
- Lane: `cpp20_23@gcc13-15@reference`
- Target rLoC: M (800〜1200)
- Crates: `rules`, `corpus`
- Scope examples:
  - standard-version legality transitions
  - default/pedantic/error promotion overlays
  - `c++20 -> c++23` compatibility shifts where diagnostic meaning is stable but legality が変わる領域
- Proof artifact:
  - overlay diff report
- Dependencies: `WP-001`, `WP-011`

### `WP-404`
**Title**: `[wp] Lock proof for C++20/23 reference lanes on GCC 13–15`

- Parent: `LEP-140`
- Stream: Gate
- LockWave: `LW40`
- Lane: `cpp20_23@gcc13-15@reference`
- Target rLoC: M (800〜1200)
- Crates: `diag_testkit`, `corpus`, `ci`
- Goal:
  - Track D を locked lane へ昇格
- Proof artifact:
  - lane completeness report for C++20/23 GCC13-15
- Dependencies: `WP-401`, `WP-402`, `WP-403`

---

## 9.6 Track E: toolchain / warnings / analyzer

### `WP-501`
**Title**: `[wp] Expand toolchain, driver, linker, and system taxonomy v2 across all GCC`

- Parent: `LEP-150`
- Stream: Classify
- LockWave: `LW50`
- Lane: `toolchain@all-gcc`
- Target rLoC: M/L (1000〜1500)
- Crates: `rules`, `diag_enrich`, `diag_residual_text`, `corpus`
- Scope examples:
  - missing tool
  - linker resolution failures
  - sysroot/header/package style failures
  - driver invocation failures sharing system render profiles
- Proof artifact:
  - toolchain family regression pack
- Dependencies: `WP-009`, `WP-010`

### `WP-502`
**Title**: `[wp] Build warning-option inventory to family seed pipeline`

- Parent: `LEP-160`
- Stream: Probe
- LockWave: `LW50`
- Lane: `warnings@all-gcc`
- Target rLoC: M/L (1000〜1500)
- Crates: `diag_backend_probe`, `xtask`, `rules`, `corpus`
- Goal:
  - `--help=warnings` などの inventory を family seed / overlay へつなぐ
  - warning family discovery を ad-hoc から定常工程へ移す
- Proof artifact:
  - warning inventory snapshot
  - generated seed candidate report
- Dependencies: `WP-004`, `WP-013`

### `WP-503`
**Title**: `[wp] Add analyzer path and CWE-aware family integration`

- Parent: `LEP-160`
- Stream: Classify
- LockWave: `LW50`
- Lane: `analyzer@all-gcc`
- Target rLoC: M/L (1000〜1500)
- Crates: `rules`, `diag_enrich`, `diag_render`, `corpus`
- Goal:
  - analyzer-specific structured metadata / CWE / path diagnostics を taxonomy と render profile に取り込む
- Proof artifact:
  - analyzer regression pack
- Dependencies: `WP-004`, `WP-010`

### `WP-504`
**Title**: `[wp] Lock proof for toolchain, warnings, and analyzer tracks`

- Parent: `LEP-160`
- Stream: Gate
- LockWave: `LW50`
- Lane: `toolchain_warnings_analyzer@all-gcc`
- Target rLoC: M (800〜1200)
- Crates: `diag_testkit`, `corpus`, `ci`
- Goal:
  - toolchain / warnings / analyzer 系 lane を release gate に乗せる
- Proof artifact:
  - multi-track completeness report
- Dependencies: `WP-501`, `WP-502`, `WP-503`

---

## 10. 「もう 1 段階」必要になる候補

この v6 は Epic と BWP まで落ちている。  
通常はここから issue を起こせばよい。  
ただし、次の BWP は oversize になりやすいので、必要なら Execution Slice を先に切る。

### 10.1 第 1 候補

- `WP-003` ADOL source tree / generator
- `WP-007` placeholderization / episode shape
- `WP-008` generic classifier engine
- `WP-010` render profile migration
- `WP-203` C++ template-note compression
- `WP-401` concepts / constraints families

### 10.2 推奨 slice 例

#### `WP-007` 用
- `[slice] Normalizer: semantic placeholderization`
- `[slice] Normalizer: episode shape extraction and typed skeleton emission`

#### `WP-008` 用
- `[slice] Classifier engine: predicate evaluator and evidence model`
- `[slice] Classifier engine: capture binding and compiled rule loading`

#### `WP-010` 用
- `[slice] Render migration: RenderProfileId introduction`
- `[slice] Render migration: policy generation and renderer switch-over`

#### `WP-401` 用
- `[slice] Concepts families: root/satellite extraction`
- `[slice] Concepts families: constraint trace condensation and render profile`

重要なのは、**最初から全部 slice にしないこと** である。  
BWP で十分なら、そのまま Codex に渡した方が速い。

---

## 11. 既存 issue の扱い

### 11.1 `#97` の再配置

現行の `#97` は 3 つの価値あるテーマを 1 枚に抱えているが、v6 では大きすぎる。  
したがって次のように分解して再配置する。

- `#include not found` → `WP-101`
- `undeclared identifier` → `WP-102`（C）および `WP-201`（C++）
- `concepts constraint failure` → `WP-401`

`#97` 自体は、移行後は legacy umbrella として close するか、Root Epic への参照 issue に変更する。

### 11.2 `#98` の再配置

README の value communication は必要だが、program の根幹ではない。  
`WP-015` に含めるか、独立 docs issue として `LW00` で扱う。

---

## 12. issue form の本文テンプレート

Codex に渡す issue は、本文を次の骨格に固定する。

```md
## Goal
## Why this matters
## Parent Epic
## Stream / Lane / LockWave
## Target rLoC
## Crates / likely files
## In scope
## Out of scope
## Acceptance criteria
## Proof required
## Dependencies
## Notes for Codex
```

### 12.1 `Notes for Codex` に必ず入れること

- drive-by refactor をしない
- 指定の crate 境界を越えない
- 生成物を正本扱いしない
- acceptance と proof artifact を満たす最短経路を取る
- 不要な rename や formatting churn を避ける

### 12.2 `Acceptance criteria` のルール

acceptance は、なるべく **コマンド or テスト名 or artifact path** で判定できる形にする。  
「いい感じに整理されていること」は acceptance にしてはいけない。

---

## 13. program 作成順序

### 13.1 まず作るもの

1. Root Epic `PRG-000`
2. Foundation Epic 7 本
3. Lane Epic 6 本
4. `LW00` の BWP 15 本

### 13.2 その次

5. `LW10` Track A の BWP 5 本
6. `LW20` Track B の BWP 4 本
7. `LW30` Track C の BWP 3 本
8. `LW40` Track D の BWP 4 本
9. `LW50` Track E の BWP 4 本

### 13.3 実行上の優先順位

厳密な実装順は次を推奨する。

- `WP-001` → `WP-002` → `WP-004`
- `WP-006` → `WP-007`
- `WP-003` → `WP-008` → `WP-009` → `WP-010`
- `WP-011` → `WP-012` → `WP-013`
- その後 `WP-101`〜`WP-105`

つまり、**foundation を完全に終わらせてから lane に行く** のではなく、  
foundation の最小十分集合ができた時点で Track A に入る。

---

## 14. DoD（Definition of Done）

## 14.1 BWP の DoD

BWP は次を満たしたときに Done とする。

- issue body の acceptance criteria を満たす
- proof artifact が生成される
- parent Epic / fields / labels が正しく埋まっている
- 追加した corpus / tests / docs が green
- out-of-scope をはみ出していない
- 不要な generated churn を含まない

## 14.2 Epic の DoD

Epic は次を満たしたときに Done とする。

- 子 BWP が全て closed
- blocker dependency が解消
- 関連 lane の completeness report が green
- README / AGENTS / docs 参照が古くない

---

## 15. anti-pattern 集

### 15.1 盤面を壊すパターン

- issue を family 単位で細切れにする
- 1 issue に 2 つ以上の proof artifact を入れる
- `agent-ready` なのに dependencies が未整理
- lane と stream の両方を親 issue にしようとする
- current repo の hard-coded family から直接横滑りで rule を増やす

### 15.2 品質を壊すパターン

- proof capture と product capture を混ぜる
- old GCC を後回しにする
- warning family discovery を現場バグ待ちにする
- `unknown` を一時避難所として再び semantic family 化する
- generated rules を手修正する

---

## 16. 最終判断

v5 から直接 issue 原案を起こすことはできる。  
しかし、日常実装の単位としてはまだ粗い。  
この v6 が、その **最後の 1 段階の実行設計** である。

この文書の採用により、repo の issue 運用は次の形になる。

- doctrine は v5 に残す
- GitHub では v6 の Issue Map に従って親子を切る
- default 実装単位は 1kL 前後の BWP
- oversized なものだけ Execution Slice を切る
- completion は issue close ではなく proof artifact で判定する

要するに、**Issue tracker を計画表で終わらせず、proof-driven execution machine に変える** のが、この v6 の本当の役割である。
