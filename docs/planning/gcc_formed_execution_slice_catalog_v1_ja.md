---
doc_role: reference-only
lifecycle_status: draft
audience: both
use_for: Active execution-planning reference for issue emission and execution-slice packaging.
do_not_use_for: Normative implementation contract or product support wording.
supersedes: []
superseded_by: []
---
> [!IMPORTANT]
> Authority: `reference-only` / `draft`
> Use for: Active execution-planning reference for issue emission and execution-slice packaging.
> Do not use for: Normative implementation contract or product support wording.

# gcc-formed Execution Slice Catalog v1
## v6 Issue Map を GitHub issue draft bundle へ落とす最終カタログ

作成日: 2026-04-11  
対象: `horiyamayoh/gcc-formed`  
上位文書: `gcc_formed_final_brushup_v5_ja.md`, `gcc_formed_issue_map_v1_ja.md`

---

## 0. この文書の役割

このカタログは、v5 の doctrine と v6 の Issue Map を、そのまま GitHub issue として起票し、さらに Codex 系の実装エージェントへ渡せる粒度まで落とした最終成果物である。

この文書の採用後、**issue 起票のための追加ブレークダウンは不要** とする。
以後の人手判断は、原則として次の 2 つに限る。

- stop-ship 条件に当たるため、境界の再設計が必要になった場合
- catalog に書かれた dependency / proof artifact / lane 情報が、実装時に current repo と物理的に齟齬を起こした場合

それ以外では、issue opener / coding agent はこの catalog と machine-readable bundle をそのまま使ってよい。

---

## 1. inventory と KPI

- tracker issues: **14**
- sliced-parent trackers: **6**
- direct implementation issues: **29**
- execution slices: **12**
- KPI 対象の implementation issues 合計: **41**
- KPI 対象の target average rLoC: **約 1030.5**

KPI は **direct + slice** に対してだけ計算し、root/epic/sliced-parent は平均 rLoC から除外する。

---

## 2. catalog-wide rules

### 2.1 issue emission

- root epic と epics は tracking issue として起票する。
- `execution_mode = direct` は、そのまま 1 issue = 1 実装単位として起票する。
- `execution_mode = sliced_parent` は tracking only とし、実装は child slice issue に置く。
- `execution_mode = slice` は Codex へ直接渡す implementation issue である。

### 2.2 status 初期値

- dependencies が空の issue は `Status=Ready`
- dependencies が 1 つ以上ある issue は `Status=Blocked`

### 2.3 allowed reinterpretation

- coding agent が変えてよいのは `likely files` の微修正だけ。
- title / parent / dependencies / proof artifact / lane / authority inputs は勝手に変えない。
- 1 issue に 2 つ以上の proof artifact を抱え込まない。

### 2.4 stop-ship triggers

- 推定 review surface が **1800 rLoC** を超える
- public schema / runtime migration / renderer switch-over が 1 issue に同時流入する
- proof artifact が 2 個以上必要になる
- 3 crate を超える cross-cutting change が unavoidable になった

stop-ship が出た場合だけ、人手で catalog を更新する。

---

## 3. command vocabulary

issue opener と coding agent は、acceptance に現れる command 名を次で固定してよい。

- `cargo xtask adol generate --check`
- `cargo xtask probe toolchain --reference-lanes --out <path>`
- `cargo xtask capture sample --mode proof --out <path>`
- `cargo xtask completeness emit --lane <lane> --scope <scope> --out <path>`
- `cargo xtask corpus anti-collision --check`
- `cargo xtask trace harvest-candidates --out <path>`

実装中に CLI の内部レイアウトが変わってもよいが、**issue body に書く command 名はこの語彙に寄せる**。

---

## 4. tracker issue catalog

| ID | Type | Parent | LockWave | Title |
|---|---|---|---|---|
| `PRG-000` | `root_epic` | `-` | `-` | [epic] Program root: Zero Unknown across GCC 9–15 |
| `FEP-010` | `epic` | `PRG-000` | `LW00` | [epic] Foundation: GitHub execution OS and issue discipline |
| `FEP-020` | `epic` | `PRG-000` | `LW00` | [epic] Foundation: IR / ADOL / policy substrate |
| `FEP-030` | `epic` | `PRG-000` | `LW00` | [epic] Foundation: toolchain fingerprint and capability inventory |
| `FEP-040` | `epic` | `PRG-000` | `LW00` | [epic] Foundation: typed skeleton normalization |
| `FEP-050` | `epic` | `PRG-000` | `LW00` | [epic] Foundation: episode-first classifier |
| `FEP-060` | `epic` | `PRG-000` | `LW00` | [epic] Foundation: render profiles and degradation policy |
| `FEP-070` | `epic` | `PRG-000` | `LW00` | [epic] Foundation: corpus, anti-collision, proof gates |
| `LEP-110` | `epic` | `PRG-000` | `LW10` | [epic] Lane lock: C core across GCC 9–15 reference lanes |
| `LEP-120` | `epic` | `PRG-000` | `LW20` | [epic] Lane lock: C++03/11 core on GCC 9–12 reference lanes |
| `LEP-130` | `epic` | `PRG-000` | `LW30` | [epic] Lane lock: C++14/17 core on GCC 13–15 reference lanes |
| `LEP-140` | `epic` | `PRG-000` | `LW40` | [epic] Lane lock: C++20/23 advanced on GCC 13–15 reference lanes |
| `LEP-150` | `epic` | `PRG-000` | `LW50` | [epic] Lane lock: toolchain / driver / linker / system families across all GCC |
| `LEP-160` | `epic` | `PRG-000` | `LW50` | [epic] Lane lock: warnings and analyzer families across all GCC |

tracker issues の body は machine-readable bundle の `body_md` をそのまま使う。

---

## 5. work package execution matrix

### 5.1 execution mode summary

| Parent WP | Mode | Child slices | Bundle | Notes |
|---|---|---|---|---|
| `WP-001` | `direct` | - | `X00` | direct issue として起票 |
| `WP-002` | `direct` | - | `X01` | direct issue として起票 |
| `WP-003` | `sliced_parent` | `ES-003A`, `ES-003B` | `X02` | ADOL source tree 導入と generator/境界 enforcement を 1 issue に詰めると、schema・xtask・checked-in artifact・docs が同時に膨らみやすい。 |
| `WP-004` | `direct` | - | `X00` | direct issue として起票 |
| `WP-005` | `direct` | - | `X01` | direct issue として起票 |
| `WP-006` | `direct` | - | `X01` | direct issue として起票 |
| `WP-007` | `sliced_parent` | `ES-007A`, `ES-007B` | `X02` | placeholderization と episode shape extraction は同じ normalizer 基盤を共有するが、実装責務と回帰面がはっきり分かれる。 |
| `WP-008` | `sliced_parent` | `ES-008A`, `ES-008B` | `X04` | predicate/evidence model と compiled rule loading/capture binding は別々にレビューした方が事故が少ない。 |
| `WP-009` | `direct` | - | `X06` | direct issue として起票 |
| `WP-010` | `sliced_parent` | `ES-010A`, `ES-010B` | `X07` | RenderProfileId 導入と renderer switch-over を分けると、golden diff を安全に観察できる。 |
| `WP-011` | `direct` | - | `X04` | direct issue として起票 |
| `WP-012` | `direct` | - | `X05` | direct issue として起票 |
| `WP-013` | `direct` | - | `X04` | direct issue として起票 |
| `WP-014` | `direct` | - | `X00` | direct issue として起票 |
| `WP-015` | `direct` | - | `X01` | direct issue として起票 |
| `WP-101` | `direct` | - | `X09` | direct issue として起票 |
| `WP-102` | `direct` | - | `X09` | direct issue として起票 |
| `WP-103` | `direct` | - | `X09` | direct issue として起票 |
| `WP-104` | `direct` | - | `X09` | direct issue として起票 |
| `WP-105` | `direct` | - | `X10` | direct issue として起票 |
| `WP-201` | `direct` | - | `X11` | direct issue として起票 |
| `WP-202` | `direct` | - | `X11` | direct issue として起票 |
| `WP-203` | `sliced_parent` | `ES-203A`, `ES-203B` | `X11` | template root-cause 抽出と note compression/render 変更を分けると、分類と表示の故障点を切り分けやすい。 |
| `WP-204` | `direct` | - | `X13` | direct issue として起票 |
| `WP-301` | `direct` | - | `X11` | direct issue として起票 |
| `WP-302` | `direct` | - | `X13` | direct issue として起票 |
| `WP-303` | `direct` | - | `X14` | direct issue として起票 |
| `WP-401` | `sliced_parent` | `ES-401A`, `ES-401B` | `X15` | concept family seed と constraint trace condensation は両方重いが、2段に分けると Codex の throughput に乗せやすい。 |
| `WP-402` | `direct` | - | `X14` | direct issue として起票 |
| `WP-403` | `direct` | - | `X05` | direct issue として起票 |
| `WP-404` | `direct` | - | `X17` | direct issue として起票 |
| `WP-501` | `direct` | - | `X09` | direct issue として起票 |
| `WP-502` | `direct` | - | `X05` | direct issue として起票 |
| `WP-503` | `direct` | - | `X09` | direct issue として起票 |
| `WP-504` | `direct` | - | `X10` | direct issue として起票 |

### 5.2 implementation bundles

#### `X00`

| ID | Mode | Parent | Target rLoC | Dependencies |
|---|---|---|---|---|
| `WP-001` | `direct` | `FEP-020` | `M (800〜1200)` | - |
| `WP-004` | `direct` | `FEP-030` | `M/L (1000〜1500)` | - |
| `WP-014` | `direct` | `FEP-010` | `M (700〜1100)` | - |

#### `X01`

| ID | Mode | Parent | Target rLoC | Dependencies |
|---|---|---|---|---|
| `WP-002` | `direct` | `FEP-020` | `M/L (1000〜1500)` | `WP-001` |
| `WP-005` | `direct` | `FEP-030` | `M (800〜1200)` | `WP-004` |
| `WP-006` | `direct` | `FEP-040` | `M (800〜1200)` | `WP-001` |
| `WP-015` | `direct` | `FEP-010` | `S/M (500〜900)` | `WP-014` |

#### `X02`

| ID | Mode | Parent | Target rLoC | Dependencies |
|---|---|---|---|---|
| `ES-003A` | `slice` | `WP-003` | `M (700〜900)` | `WP-001`, `WP-002` |
| `ES-007A` | `slice` | `WP-007` | `M (600〜800)` | `WP-006` |
| `WP-003` | `sliced_parent` | `FEP-020` | `L (1300〜1800)` | `WP-001`, `WP-002` |
| `WP-007` | `sliced_parent` | `FEP-040` | `L (1300〜1800)` | `WP-006` |

#### `X03`

| ID | Mode | Parent | Target rLoC | Dependencies |
|---|---|---|---|---|
| `ES-003B` | `slice` | `WP-003` | `M (600〜800)` | `ES-003A` |
| `ES-007B` | `slice` | `WP-007` | `M (700〜900)` | `ES-007A` |

#### `X04`

| ID | Mode | Parent | Target rLoC | Dependencies |
|---|---|---|---|---|
| `ES-008A` | `slice` | `WP-008` | `M (700〜900)` | `ES-003B`, `ES-007B` |
| `WP-008` | `sliced_parent` | `FEP-050` | `L (1300〜1800)` | `ES-003B`, `ES-007B` |
| `WP-011` | `direct` | `FEP-070` | `M/L (1000〜1500)` | `ES-003B`, `WP-004` |
| `WP-013` | `direct` | `FEP-070` | `M (800〜1200)` | `WP-004`, `ES-007B` |

#### `X05`

| ID | Mode | Parent | Target rLoC | Dependencies |
|---|---|---|---|---|
| `ES-008B` | `slice` | `WP-008` | `M (700〜900)` | `ES-008A` |
| `WP-012` | `direct` | `FEP-070` | `M/L (1000〜1500)` | `WP-011` |
| `WP-403` | `direct` | `LEP-140` | `M (800〜1200)` | `WP-001`, `WP-011` |
| `WP-502` | `direct` | `LEP-160` | `M/L (1000〜1500)` | `WP-004`, `WP-013` |

#### `X06`

| ID | Mode | Parent | Target rLoC | Dependencies |
|---|---|---|---|---|
| `WP-009` | `direct` | `FEP-050` | `M/L (1000〜1500)` | `ES-008B` |

#### `X07`

| ID | Mode | Parent | Target rLoC | Dependencies |
|---|---|---|---|---|
| `ES-010A` | `slice` | `WP-010` | `M (600〜800)` | `ES-003B`, `WP-009` |
| `WP-010` | `sliced_parent` | `FEP-060` | `L (1200〜1700)` | `ES-003B`, `WP-009` |

#### `X08`

| ID | Mode | Parent | Target rLoC | Dependencies |
|---|---|---|---|---|
| `ES-010B` | `slice` | `WP-010` | `M (700〜900)` | `ES-010A` |

#### `X09`

| ID | Mode | Parent | Target rLoC | Dependencies |
|---|---|---|---|---|
| `WP-101` | `direct` | `LEP-110` | `M/L (1000〜1500)` | `ES-008B`, `ES-010B`, `WP-012` |
| `WP-102` | `direct` | `LEP-110` | `M/L (1000〜1500)` | `ES-008B`, `ES-010B`, `WP-012` |
| `WP-103` | `direct` | `LEP-110` | `M/L (1000〜1500)` | `ES-008B`, `ES-010B`, `WP-012` |
| `WP-104` | `direct` | `LEP-110` | `M/L (1000〜1500)` | `ES-008B`, `ES-010B`, `WP-012` |
| `WP-501` | `direct` | `LEP-150` | `M/L (1000〜1500)` | `WP-009`, `ES-010B` |
| `WP-503` | `direct` | `LEP-160` | `M/L (1000〜1500)` | `WP-004`, `ES-010B` |

#### `X10`

| ID | Mode | Parent | Target rLoC | Dependencies |
|---|---|---|---|---|
| `WP-105` | `direct` | `LEP-110` | `M (800〜1200)` | `WP-101`, `WP-102`, `WP-103`, `WP-104` |
| `WP-504` | `direct` | `LEP-160` | `M (800〜1200)` | `WP-501`, `WP-502`, `WP-503` |

#### `X11`

| ID | Mode | Parent | Target rLoC | Dependencies |
|---|---|---|---|---|
| `ES-203A` | `slice` | `WP-203` | `M (700〜900)` | `WP-105`, `ES-007B`, `ES-010B` |
| `WP-201` | `direct` | `LEP-120` | `M/L (1000〜1500)` | `WP-102`, `WP-105` |
| `WP-202` | `direct` | `LEP-120` | `M/L (1000〜1500)` | `ES-010B`, `WP-105` |
| `WP-203` | `sliced_parent` | `LEP-120` | `L (1200〜1700)` | `ES-007B`, `ES-010B`, `WP-105` |
| `WP-301` | `direct` | `LEP-130` | `M/L (1000〜1500)` | `ES-008B`, `ES-010B`, `WP-105` |

#### `X12`

| ID | Mode | Parent | Target rLoC | Dependencies |
|---|---|---|---|---|
| `ES-203B` | `slice` | `WP-203` | `M (700〜900)` | `ES-203A` |

#### `X13`

| ID | Mode | Parent | Target rLoC | Dependencies |
|---|---|---|---|---|
| `WP-204` | `direct` | `LEP-120` | `M (900〜1300)` | `WP-201`, `WP-202`, `ES-203B` |
| `WP-302` | `direct` | `LEP-130` | `M/L (1000〜1500)` | `ES-203B` |

#### `X14`

| ID | Mode | Parent | Target rLoC | Dependencies |
|---|---|---|---|---|
| `WP-303` | `direct` | `LEP-130` | `M (800〜1200)` | `WP-301`, `WP-302` |
| `WP-402` | `direct` | `LEP-140` | `M/L (1000〜1500)` | `WP-302` |

#### `X15`

| ID | Mode | Parent | Target rLoC | Dependencies |
|---|---|---|---|---|
| `ES-401A` | `slice` | `WP-401` | `M (700〜900)` | `WP-303`, `ES-007B`, `ES-010B` |
| `WP-401` | `sliced_parent` | `LEP-140` | `L (1300〜1800)` | `ES-007B`, `ES-010B`, `WP-303` |

#### `X16`

| ID | Mode | Parent | Target rLoC | Dependencies |
|---|---|---|---|---|
| `ES-401B` | `slice` | `WP-401` | `M (700〜900)` | `ES-401A` |

#### `X17`

| ID | Mode | Parent | Target rLoC | Dependencies |
|---|---|---|---|---|
| `WP-404` | `direct` | `LEP-140` | `M (800〜1200)` | `ES-401B`, `WP-402`, `WP-403` |

---

## 6. sliced-parent catalog

### `WP-003`

**Title**: [wp] Scaffold ADOL source tree, schema, and generated artifact boundaries

**Why sliced**: ADOL source tree 導入と generator/境界 enforcement を 1 issue に詰めると、schema・xtask・checked-in artifact・docs が同時に膨らみやすい。

**Child slices**: `ES-003A`, `ES-003B`

#### `ES-003A`

- title: [slice] ADOL source tree layout and schema skeleton
- target rLoC: `M (700〜900)`
- crates: `diag_rulepack`, `docs`
- proof path: `artifacts/adol/es-003A/source-tree-contract/`
- dependencies: `WP-001`, `WP-002`
- in scope:
  - ADOL source tree の checked-in layout を導入する。
  - canonical family catalog / lane catalog / render profile catalog / authority stamp の schema skeleton を定義する。
  - source-of-truth と generated artifact の境界を docs と validate path で明示する。
- out of scope:
  - generator 実装本体。
  - xtask からの dry-run。

#### `ES-003B`

- title: [slice] ADOL generator dry-run and artifact boundary enforcement
- target rLoC: `M (600〜800)`
- crates: `diag_rulepack`, `xtask`
- proof path: `artifacts/adol/es-003B/generator-dry-run/`
- dependencies: `ES-003A`
- in scope:
  - ADOL source tree から generated artifacts を出す dry-run/check path を実装する。
  - checked-in artifact boundary enforcement を validate/check に組み込む。
- out of scope:
  - family migration。
  - render migration。

### `WP-007`

**Title**: [wp] Implement DiagnosticTextNormalizer phase 2: placeholderization and episode shape extraction

**Why sliced**: placeholderization と episode shape extraction は同じ normalizer 基盤を共有するが、実装責務と回帰面がはっきり分かれる。

**Child slices**: `ES-007A`, `ES-007B`

#### `ES-007A`

- title: [slice] Normalizer: semantic placeholderization
- target rLoC: `M (600〜800)`
- crates: `diag_enrich`, `diag_residual_text`
- proof path: `artifacts/normalize/es-007A/placeholder-snapshots/`
- dependencies: `WP-006`
- in scope:
  - identifier/type/path/literal などの semantic placeholderization を導入する。
  - locale-stable skeleton token を出せるようにする。
- out of scope:
  - episode root/satellite grouping。
  - classifier predicate evaluation。

#### `ES-007B`

- title: [slice] Normalizer: episode shape extraction and typed skeleton emission
- target rLoC: `M (700〜900)`
- crates: `diag_enrich`, `diag_residual_text`
- proof path: `artifacts/normalize/es-007B/typed-skeleton-corpus/`
- dependencies: `ES-007A`
- in scope:
  - episode root/satellite grouping を抽出する。
  - typed skeleton emission を classifier 入力として出せるようにする。
- out of scope:
  - generic classifier engine。
  - render migration。

### `WP-008`

**Title**: [wp] Introduce generic classifier engine and predicate/capture model

**Why sliced**: predicate/evidence model と compiled rule loading/capture binding は別々にレビューした方が事故が少ない。

**Child slices**: `ES-008A`, `ES-008B`

#### `ES-008A`

- title: [slice] Classifier engine: evidence model and predicate evaluator
- target rLoC: `M (700〜900)`
- crates: `diag_enrich`, `diag_rulepack`
- proof path: `artifacts/classify/es-008A/predicate-evaluator-tests/`
- dependencies: `ES-003B`, `ES-007B`
- in scope:
  - EvidenceSet / predicate model を導入する。
  - generic predicate evaluator を実装する。
- out of scope:
  - compiled rule loading。
  - six family migration。

#### `ES-008B`

- title: [slice] Classifier engine: capture binding and compiled rule loading
- target rLoC: `M (700〜900)`
- crates: `diag_enrich`, `diag_rulepack`
- proof path: `artifacts/classify/es-008B/compiled-rule-loading/`
- dependencies: `ES-008A`
- in scope:
  - capture binding と compiled rule loading を実装する。
  - 現行 enrich pipeline から generic engine への接続点を導入する。
- out of scope:
  - render profile migration。
  - 新 family seed。

### `WP-010`

**Title**: [wp] Replace RendererFamilyKind with RenderProfileId and generated render policies

**Why sliced**: RenderProfileId 導入と renderer switch-over を分けると、golden diff を安全に観察できる。

**Child slices**: `ES-010A`, `ES-010B`

#### `ES-010A`

- title: [slice] Render migration: RenderProfileId introduction
- target rLoC: `M (600〜800)`
- crates: `diag_rulepack`, `diag_render`
- proof path: `artifacts/render/es-010A/render-profile-schema/`
- dependencies: `ES-003B`, `WP-009`
- in scope:
  - RenderProfileId と generated render policy schema を導入する。
  - renderer が RenderProfileId を受け取れる seam を用意する。
- out of scope:
  - 全面 switch-over。
  - golden output の大規模変更。

#### `ES-010B`

- title: [slice] Render migration: policy generation and renderer switch-over
- target rLoC: `M (700〜900)`
- crates: `diag_rulepack`, `diag_render`
- proof path: `artifacts/render/es-010B/golden-diff-review/`
- dependencies: `ES-010A`
- in scope:
  - generated render policies を renderer main path に接続する。
  - RendererFamilyKind 依存を primary path から外す。
- out of scope:
  - family seed の追加。
  - toolchain/analyzer special cases の追加。

### `WP-203`

**Title**: [wp] Seed C++03/11 template deduction and core template-note families on GCC 9–12

**Why sliced**: template root-cause 抽出と note compression/render 変更を分けると、分類と表示の故障点を切り分けやすい。

**Child slices**: `ES-203A`, `ES-203B`

#### `ES-203A`

- title: [slice] Template families: deduction root-cause extraction and family seeding
- target rLoC: `M (700〜900)`
- crates: `rules`, `diag_enrich`, `corpus`
- lane: `cpp03_11@gcc9-12@reference`
- proof path: `artifacts/completeness/cpp03-11-gcc9-12.template-root.json`
- dependencies: `WP-105`, `ES-007B`, `ES-010B`
- in scope:
  - template argument deduction / substitution root cause 系 family を seed する。
  - root cause extraction を note compression とは分離して導入する。
- out of scope:
  - render-side note condensation。
  - concepts/constraints。

#### `ES-203B`

- title: [slice] Template families: note compression and render integration
- target rLoC: `M (700〜900)`
- crates: `rules`, `diag_enrich`, `diag_render`, `corpus`
- lane: `cpp03_11@gcc9-12@reference`
- proof path: `artifacts/render/cpp03-11-template-trace-pack/`
- dependencies: `ES-203A`
- in scope:
  - template note chain の condensation を導入する。
  - template trace render profile を安定化する。
- out of scope:
  - 新しい non-template family の追加。

### `WP-401`

**Title**: [wp] Seed C++20/23 concepts and constraints families

**Why sliced**: concept family seed と constraint trace condensation は両方重いが、2段に分けると Codex の throughput に乗せやすい。

**Child slices**: `ES-401A`, `ES-401B`

#### `ES-401A`

- title: [slice] Concepts families: root/satellite extraction and family seeding
- target rLoC: `M (700〜900)`
- crates: `rules`, `diag_enrich`, `corpus`
- lane: `cpp20_23@gcc13-15@reference`
- proof path: `artifacts/completeness/cpp20-23-gcc13-15.constraint-root.json`
- dependencies: `WP-303`, `ES-007B`, `ES-010B`
- in scope:
  - concept unsatisfied / requires-expression invalid / atomic/nested requirement unsatisfied family を seed する。
  - constraint trace の root/satellite extraction を導入する。
- out of scope:
  - trace condensation/render overhaul。

#### `ES-401B`

- title: [slice] Concepts families: constraint trace condensation and render profile
- target rLoC: `M (700〜900)`
- crates: `rules`, `diag_enrich`, `diag_render`, `corpus`
- lane: `cpp20_23@gcc13-15@reference`
- proof path: `artifacts/render/cpp20-23-constraint-pack/`
- dependencies: `ES-401A`
- in scope:
  - constraint trace condensation を導入する。
  - constraint_trace_error render profile を安定化する。
- out of scope:
  - consteval/constinit family。
  - overlay work。

---

## 7. conditional split axes for direct issues

次の direct issue は、stop-ship trigger が出たときだけ、ここに書かれた軸で 2 分割してよい。

### `WP-004`
- probe signature capture / normalization
- reference-lane capability export and fixture pinning

### `WP-011`
- completeness report emitter/schema
- CI gate wiring and mode split

### `WP-012`
- anti-collision corpus schema/fixtures
- adversarial runner and negative match reporting

### `WP-301`
- auto/decltype deduction families
- initializer-list / CTAD-adjacent families

### `WP-502`
- warning inventory snapshot acquisition
- seed-candidate generation / overlay report

---

## 8. hand-off protocol

### 8.1 issue opener への手順

1. `PRG-000` と 13 本の epic を作成する。
2. 6 本の sliced-parent BWP を tracking issue として作成する。
3. 29 本の direct issue と 12 本の slice issue を machine-readable bundle から作成する。
4. 各 issue に parent / dependencies / execution bundle / initial status を設定する。
5. `body_md` をそのまま issue body に流し込む。

### 8.2 coding agent への手順

1. `Status=Ready` の issue だけを取得する。
2. `body_md` の acceptance / proof / out-of-scope を実装の契約とみなす。
3. issue 完了時は proof artifact を PR description に貼り、親 issue を更新する。
4. issue 境界を超える drive-by refactor はしない。

### 8.3 ここで思考仕事を閉じてよいか

よい。

この catalog と machine-readable bundle があれば、issue 起票と primary implementation は追加の概念設計なしで進めてよい。
再び人手の設計判断を開くのは、2.4 の stop-ship trigger が出たときだけである。

---

## Appendix A. machine-readable bundle contents

同梱の JSON bundle には、各 issue について次を含める。

- `id`, `type`, `title`, `parent`, `stream`, `lane`, `lock_wave`
- `execution_mode`, `count_in_rloc_kpi`, `target_rloc`, `target_rloc_mid`
- `crates`, `likely_files`, `authority_inputs`
- `in_scope`, `out_of_scope`, `acceptance`, `proof_path`, `dependencies`
- `execution_bundle`, `initial_status`, `notes_for_codex`, `body_md`

issue opener は JSON を source-of-truth として使い、markdown 本文は人間向けの読み物として扱う。
