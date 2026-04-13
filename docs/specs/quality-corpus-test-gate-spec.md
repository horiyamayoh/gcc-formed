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

# gcc-formed Quality / Corpus / Test Gate 仕様書

- **文書種別**: 内部仕様書（実装契約）
- **状態**: Accepted Baseline
- **版**: `1.0.0-alpha.1`
- **対象**: `gcc-formed` / 将来の `cc-formed`
- **主用途**: 品質保証戦略、コーパス運用、テスト階層、CI gate、release gate、rollout readiness の契約固定
- **想定実装**: Linux first / GCC first / 品質最優先 / fail-open / corpus-driven
- **関連文書**:
  - `../architecture/gcc-formed-vnext-change-design.md`
  - `diagnostic-ir-v1alpha-spec.md`
  - `gcc-adapter-ingestion-spec.md`
  - `rendering-ux-contract-spec.md`
  - `packaging-runtime-operations-spec.md`
  - `../support/SUPPORT-BOUNDARY.md`
- **関連 ADR**:
  - `adr-initial-set/adr-0006-fail-open-fallback-and-provenance.md`
  - `adr-initial-set/adr-0010-deterministic-rule-engine-no-ai-core.md`
  - `adr-initial-set/adr-0016-trace-bundle-content-and-redaction.md`
  - `adr-initial-set/adr-0017-dependency-allowlist-and-license-policy.md`
  - `adr-initial-set/adr-0018-corpus-governance.md`
  - `adr-initial-set/adr-0019-render-modes.md`
  - `adr-initial-set/adr-0020-stability-promises.md`
  - `adr-initial-set/adr-0029-path-b-and-c-are-first-class-product-paths.md`
  - `adr-initial-set/adr-0031-native-non-regression-for-tty-default.md`

---

## 1. この文書の目的

本仕様書は、`gcc-formed` の品質を **偶然の見た目改善** ではなく、**再現可能な契約**として固定するための文書である。

本プロジェクトでは、UX の良し悪しを最後に人間の印象で判断してはならない。  
代わりに、以下を明確に定義し、テストと gate に落とし込む。

1. **何を品質と見なすか**
2. **どの失敗を stop-ship とするか**
3. **どのコーパスで regressions を止めるか**
4. **どのテスト層が何を保証するか**
5. **どの gate が merge / release / rollout を止めるか**
6. **shadow mode や harvested trace をどう製品品質へ還元するか**

この文書の目的は、単なるテスト一覧の作成ではない。  
本仕様は、以下を同時に満たすことを狙う。

- adapter / renderer / analysis の責務境界を test contract に写像する
- 「見た目がよくても誤誘導なら失敗」という品質思想を gate に反映する
- GCC バージョン差異と将来の Clang 対応を破綻させない
- CI で回る自動 gate と、人間がレビューすべき UX gate を分ける
- 社内 rollout 時の shadow 観測を curated corpus に接続する

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

品質 / corpus / gate 層は pipeline 上で以下に位置する。

```text
source fixture / harvested trace / real compiler invocation
        │
        ├──► ingress capture goldens
        ├──► adapter contract tests
        ├──► IR validation + canonical snapshot
        ├──► analysis overlay tests
        ├──► renderer contract + text snapshot
        ├──► end-to-end real compiler tests
        ├──► performance / robustness / fuzz
        └──► UX evaluation / shadow review
                │
                ▼
        merge gate / nightly gate / release gate / rollout gate
```

この文書は、`diagnostic-ir-v1alpha-spec.md` と `rendering-ux-contract-spec.md` の**上に乗る品質運用契約**である。  
IR や renderer の仕様を再定義するものではなく、**何をどの深さで固定し、どの drift を許容し、どの失敗で止めるか**を定義する。

---

## 4. スコープ

### 4.1 本仕様が扱うもの

- テスト戦略の層構造
- curated corpus / harvested corpus / fuzz corpus の役割
- fixture schema と artifact 体系
- goldens / snapshots / semantic assertions の使い分け
- GCC version / environment / locale / linker の matrix
- merge gate / nightly gate / release gate / rollout gate
- performance / memory / determinism / flake 管理
- shadow mode と corpus promotion 流れ
- privacy / redaction / artifact retention
- 将来の Clang 対応を壊さない test harness 設計

### 4.2 本仕様が直接扱わないもの

- 実装言語固有の test framework 選定
- CI vendor 固有の YAML 記述
- IDE plugin の UX テスト詳細
- LSP / editor protocol の conformance
- 製品外公開向け benchmark paper の書式
- 組織固有のセキュリティ承認プロセスそのもの

---

## 5. 中核品質原則

本節は最重要である。  
以後のすべての test と gate はこの原則を実装に写したものでなければならない。

### 原則 1: fidelity beats prettiness

**元 compiler の事実を歪めるくらいなら、見た目は悪くても raw fallback のほうが良い。**

これにより次を意味する。

- 誤った primary location は stop-ship
- 誤った fix-it / action hint は stop-ship になりうる
- 低 confidence の推定を断定表示した場合は重大欠陥である
- 省略はよいが、黙って消すのは不可

### 原則 2: corpus is the product

curated corpus は単なる QA asset ではない。  
**このプロダクトの品質仕様そのもの**である。

新しい診断 family を本気でサポートすると主張するなら、先に最低限の corpus が入っていなければならない。

### 原則 3: semantic assertions come before snapshots

text snapshot は重要だが、それだけでは brittle である。  
各 fixture では、可能な限り以下を持つ。

- semantic assertions
- canonical IR snapshot
- view model snapshot
- final text snapshot
- native-parity expectation fields for stop-ship dimensions such as line budget, disclosure honesty, first-action visibility, and noisy-family compaction

### 原則 4: same input, same output, same verdict

同じ fixture、同じ compiler version、同じ capability、同じ config なら、同じ出力と同じ verdict が得られるべきである。

非決定性は bug とみなす。

### 原則 5: test what is shipped, but also test normalized controls

製品 hot path と同じ条件での E2E を必須とする。  
同時に、差分解析しやすいように harness 専用の normalized capture も持つ。

### 原則 6: high-confidence mislead is worse than fallback

高 confidence で誤った root-cause や fix 方針を出すことは、raw fallback より危険である。  
したがって mislead rate は主要 KPI かつ release gate 対象である。

### 原則 7: harvested reality must be curated before it becomes contract

shadow mode や field trace で集めた実データは、そのまま golden にしてはならない。  
sanitize、dedup、minimize、label、review を経て curated corpus に昇格させる。

### 原則 8: future multi-compiler support must be preserved by the harness

test harness 自体が GCC 特化 API に依存しすぎてはならない。  
semantic scenario を first-class にし、backend-specific snapshot は隔離する。

---

## 6. 品質モデル

本プロダクトの品質は、少なくとも以下 7 軸で評価する。

| 軸 | 定義 | 代表指標 | 主な gate |
|---|---|---|---|
| Fidelity | compiler facts を失わず歪めないこと | P0/P1 fidelity bugs, exit parity, provenance retention | PR / release |
| Utility | 修正行動に速く到達できること | TRC, TFAH, first-fix success | release / UX review |
| Noise control | 不要情報を圧縮できること | Noise Before Action, compression ratio | nightly / release |
| Band Coverage | GCC version band / linker / env 差異に耐えること | matrix pass rate, fallback correctness | nightly / release |
| Robustness | 壊れた入力や巨大入力でも壊れないこと | fuzz crash count, malformed parse resilience | nightly / release |
| Performance | 実行時間とメモリ増加が許容範囲であること | p95 overhead, RSS, artifact I/O | PR / release |
| Operability | debug・trace・rollout が現実的であること | shadow ingest success, trace reproducibility | rollout |

### 6.1 品質優先順位

衝突時の優先順位は以下とする。

1. **Correctness / fidelity**
2. **Safe fallback**
3. **Determinism / debuggability**
4. **Utility / actionability**
5. **Noise reduction**
6. **Performance**
7. **Cosmetic consistency**

---

## 7. 欠陥分類と重要度

### 7.1 Fidelity defect taxonomy

| 種別 | 例 | 既定重要度 |
|---|---|---|
| Fact loss | compiler が出していた severity / code / location / child を失う | P0/P1 |
| Fact mutation | expected/actual の関係が逆転、message の意味が変わる | P0 |
| Wrong primary | primary location が別地点になる | P0 |
| Wrong fix | 機械適用 suggestion や help が誤方向 | P0 |
| Unsafe certainty | 低 confidence 推定を断定表示 | P1 |
| Silent omission | 省略したのに omission notice が無い | P1 |
| Exit divergence | wrapper exit code が compiler と食い違う | P0 |
| Fallback corruption | fallback 時の raw output が壊れる | P0 |
| Non-deterministic ordering | 同条件で lead group や行順が変わる | P1 |
| Crash / hang / OOM | parser / renderer / harness が落ちる | P0 |

### 7.2 UX defect taxonomy

| 種別 | 例 | 既定重要度 |
|---|---|---|
| First action buried | 最初の修正行動が first screenful に無い | P1 |
| Root-cause regression | lead group が不適切になる | P1 |
| Over-compression | 必要な declaration/use/context が見えない | P1 |
| Under-compression | note flood が大量に残る | P2 |
| CI unreadability | path / line 情報が grep しづらい | P1/P2 |
| Caret misalignment | excerpt/caret がズレる | P1 |
| Cosmetic drift | 文言や空白の微修正 | P3 |

### 7.3 Gate blocking rule

- **P0**: merge / release / rollout すべて blocker
- **P1**: release blocker。PR では changed scope に含まれれば blocker
- **P2**: 原則 triage。UX regressions が継続する場合は release blocker に昇格可
- **P3**: cosmetic。snapshot update と reviewer 承認で可

---

## 8. KPI と gate 指標

本節の数値目標は、上位設計で定義した成功基準を gate に変換したものである。

### 8.1 定量 KPI

| KPI | 定義 | 測定方法 | 目標 |
|---|---|---|---|
| Time to Root Cause (TRC) | ユーザーが最初に直すべき箇所に到達するまでの時間 | task study / first corrective edit | raw GCC 比で **35% 以上短縮** |
| Time to First Actionable Hint (TFAH) | 最初の具体的修正候補が提示されるまでの時間 | 表示開始から hint 行到達まで | raw GCC 比で **50% 以上短縮** |
| First-Fix Success Rate | 最初の修正で root cause を改善できた割合 | task success rate | raw GCC 比で **+20pt** |
| Noise Before Action | 最初の actionable line より前に読む非本質行数 | rendered text 解析 | median **8 行以下** |
| Diagnostic Compression Ratio | raw 行数 / rendered 行数 | corpus 比較 | 主要ケースで **1.5x〜4x** |
| High-Confidence Mislead Rate | 高 confidence で誤った root cause を出した率 | expert review / labeled corpus | **2% 未満** |
| Unexpected Fallback Rate | enhanced-eligible run で意図せず raw fallback になった率 | shadow telemetry / trace | **0.1% 未満** |
| Fidelity Defect Rate | compiler 情報を欠落/改変して問題化した率 | bug triage | **P0 = 0** |
| p95 Overhead (success path) | wrapper 追加オーバーヘッド | benchmark | **p95 < 40ms** |
| p95 Postprocess Time (failure path) | compiler 終了後の解析・描画時間 | benchmark | **p95 < 80ms** |

### 8.2 KPI と hard gate の違い

すべての KPI を毎 PR gate に直接使ってはならない。  
hard gate と trend metric を分ける。

#### hard gate
- P0 / P1 bug count
- curated corpus pass rate
- exit parity
- snapshot determinism
- unexpected fallback
- performance budget
- fuzz crash count
- flake rate

#### trend metric
- TRC
- TFAH
- first-fix success
- subjective helpfulness
- compression ratio の改善幅

---

## 9. テスト階層

本仕様では、少なくとも以下 9 層の test を持つ。

### 9.1 Layer 1: pure unit tests

目的:
- 小さな規則の局所保証
- failure localization の高速化

対象例:
- version detection
- version band / capability resolution
- path normalization
- ownership classification
- diagnostic family classifier
- ranking tie-break
- omission counter
- ANSI stripping / escape sanitization
- redaction filters
- fingerprint generation
- canonical JSON sort / stable id rules

**MUST**:
- ファイル I/O や実 GCC 起動に依存しない
- 1 テスト失敗で原因箇所が狭く特定できる

### 9.2 Layer 2: schema / semantic validation tests

目的:
- IR と view model の structural correctness を保証する

対象:
- `DiagnosticDocument` validation
- `AnalysisOverlay` validation
- render view model validation
- manifest schema validation

**MUST**:
- unknown field / extension を許容する path も持つ
- invalid fixture に対する明確なエラー分類を返す

### 9.3 Layer 3: adapter contract tests

入力:
- raw stderr fixture
- SARIF fixture
- environment manifest
- expected version band / processing path / mode

出力:
- canonical `DiagnosticDocument` facts
- `CaptureArtifact` / `IntegrityIssue`
- passthrough / fallback verdict

目的:
- GCC version 差異を adapter 内に閉じ込める
- ingestion が raw provenance を失わないことを検証する

**MUST**:
- compiler 実行無しで replay 可能
- GCC 15 / 14 / 13 / unknown を fixture で表現できる

### 9.4 Layer 4: analysis / ranking contract tests

目的:
- root-cause prioritization と first-action generation を固定する

入力:
- validated IR
- analysis policy
- labeled expectation

出力:
- `AnalysisOverlay`
- lead group / lead node selection
- collapsed counts / first action

**MUST**:
- 「正しい答えが複数ある」ケースを allowed set で表現できる
- confidence band を test oracle に含める

### 9.5 Layer 5: renderer contract + view model tests

目的:
- text snapshot 以前に、情報設計の崩れを検出する

検証対象:
- selected lead group
- excerpt count
- omitted section notice
- displayed family badges
- confidence labeling
- raw provenance linkability
- first screenful 行数

**SHOULD**:
- final text snapshot に加えて、view model snapshot を持つ
- profile / capability ごとの差分を明示する

### 9.6 Layer 6: rendered text snapshot tests

目的:
- user-visible output drift をレビュー可能にする

対象軸:
- profile: `default`, `concise`, `verbose`, `debug`, `ci`, `raw_fallback`
- color: on/off
- width: 60 / 80 / 100 / 140
- source availability: readable / unreadable
- completeness: complete / partial / passthrough
- confidence: high / medium / low

**注意**:
text snapshot は重要だが brittle である。  
semantic assertion と view model snapshot を持たない text-only fixture は新規追加を原則禁止する。

### 9.7 Layer 7: end-to-end real compiler tests

目的:
- 製品 hot path と同じ条件で、実際の GCC invocation が期待通り動くことを保証する

検証対象:
- compiler resolution
- env sanitization
- sidecar capture
- exit parity
- raw stderr capture
- path-aware fallback
- render output
- trace bundle completeness

E2E は 2 種類に分ける。

#### shipped-realistic mode
製品が実際に使う flags / env で起動する。  
これが最重要である。

#### harness-normalized mode
差分解析用に raw text を安定化した control run。  
GCC では `-fdiagnostics-plain-output` のような option を **harness 限定**で使ってよい。  
ただし、これを製品 UX の代表値として扱ってはならない。

### 9.8 Layer 8: performance / resource benchmarks

目的:
- 成功 path と failure path の overhead を予算内に保つ

対象:
- success no-diagnostics
- one syntax error
- one type mismatch
- template explosion
- linker undefined reference
- 100+ diagnostics
- CI non-TTY
- large source file
- unreadable source excerpt fallback

**MUST**:
- 参照マシン構成を固定する
- warm/cold の両方を分けて測る
- p50/p95/p99 と peak RSS を記録する
- operator-real workload を `VersionBand × ProcessingPath` で breakdown し、少なくとも linker-heavy / compatibility-native-text / honest-fallback availability を report に残す
- benchmark artifact は scenario list だけでなく band/path breakdown と designated baseline comparison を machine-readable に retain する

### 9.9 Layer 9: robustness / fuzz / adversarial tests

目的:
- 壊れた入力や敵対的入力でも crash しないことを保証する

対象例:
- malformed SARIF
- truncated JSON-like payload
- invalid UTF-8
- gigantic template names
- deeply nested notes
- malicious ANSI escape in source
- path traversal-like filenames
- huge include chain
- repeated residual stderr fragments

**MUST**:
- no crash
- no hang
- bounded memory
- safe fallback or explicit integrity issue

---

## 10. Differential tests と metamorphic tests

### 10.1 differential tests

同一 fixture に対し、以下を比較する。

1. raw GCC default
2. harness-normalized GCC
3. wrapper render
4. wrapper raw_fallback
5. 将来は Clang baseline

differential test では以下を確認する。

- exit code parity
- diagnostics count drift（許容ルール付き）
- lead location の一貫性
- fix-it / suggestion の保持
- raw provenance への可逆性
- wrapper failure 時の native parity

### 10.2 metamorphic tests

特定の意味が変わらない入力変形に対して、重要性の高い結論が保たれることを確認する。

例:
- workspace root を変える
- file path を相対/絶対で変える
- include directory の順序に無関係な path prefix だけ変える
- タブ/スペースや空行を追加しても family 分類が保たれる
- terminal width が変わっても lead group は変わらない
- color on/off で information content が変わらない

**MUST**:
- 変わるべきもの（列番号、wrap 位置）と変わってはならないもの（family、lead group、first action presence）を明確に分ける

---

## 11. Corpus の基本戦略

### 11.1 corpus は 4 層に分ける

| 種別 | 役割 | commit 対象 | 品質契約性 |
|---|---|---:|---|
| Curated corpus | 製品品質の正契約 | YES | 最重要 |
| Seed corpus | 立ち上げ用の最小代表集合 | YES | 高 |
| Harvested corpus | shadow / field 由来の素材 | 原則 NO | 中 |
| Fuzz corpus | robustness seed | YES | 高（ただし semantics 契約ではない） |

### 11.2 curated corpus の役割

curated corpus は、**この製品が何をサポートしていると言えるか**を定義する。

新規 family をサポートすると主張するには、最低限以下が必要。

- hand-authored or minimized repro
- expectations manifest
- ingress artifact
- canonical IR snapshot
- renderer snapshot（少なくとも `default` と `ci`）
- reviewer note

### 11.3 corpus family taxonomy

最低限以下の family を持つ。

1. syntax error
2. undeclared identifier / name lookup
3. type mismatch / conversion
4. qualifier mismatch / constness
5. overload candidate flood
6. template instantiation chain
7. template substitution failure summary
8. concepts / constraints failure（post-MVP でも seed は持つ）
9. macro expansion chain
10. include chain
11. system-header-heavy diagnostic
12. linker undefined reference
13. linker multiple definition
14. duplicate symbol / ODR-like family
15. assembler residual diagnostic
16. analyzer/path-like diagnostic
17. passthrough residual text
18. partial / malformed structured input
19. unreadable source excerpt
20. non-UTF8 / exotic filename / unicode path

### 11.4 seed corpus 初期規模

Phase 1 の seed corpus 目標:

- **最低 50 件**
- 推奨 **80〜129 件**
- うち C++ template/macro/linker 系が **40% 以上**

理由:
- C/C++ の本当の難所を後回しにすると、簡単な syntax case に最適化された誤った UX が固まるため

### 11.5 composition quota（初期推奨）

seed/curated には偏りを避けるため、少なくとも以下の下限を置く。

| family 群 | 初期下限 |
|---|---:|
| syntax / parser | 8 |
| simple sema / type mismatch | 10 |
| overload / candidates | 6 |
| template / constraints | 12 |
| macro / include | 10 |
| linker / assembler residual | 10 |
| partial / malformed / passthrough | 6 |
| path / locale / unreadable source / unicode | 6 |

この quota は「均等に作る」ためではなく、簡単な family に過剰最適化することを防ぐためのものである。

### 11.6 Band B / Band C representative matrix

GCC13-14 / GCC9-12 を first-class beta band として扱う場合、curated corpus は version だけでなく `ProcessingPath`、replay stop-ship surface、fallback contract を明示できなければならない。

| VersionBand | ProcessingPath | Surface | Corpus expectation | Required representative metadata |
|---|---|---|---|---|
| `GCC15` | `DualSinkStructured` | `default`, `ci`, `debug` | dual-sink structured render を fixture で明示し、checked-in replay expectation が存在する surface だけを `surface:*` と `matrix_applicability.surfaces` で宣言する。`debug` をまだ証明していない fixture は `matrix_applicability.note` で omission reason を残す | `band:gcc15`, `processing_path:dual_sink_structured`, `surface:default`, `surface:ci`, optional `surface:debug`, `matrix_applicability` |
| `GCC13-14` | `NativeTextCapture` | `default`, `ci`, `debug` | bounded render か honest fallback のどちらを期待するかを fixture で明示し、checked-in replay expectation が存在する surface を `surface:*` と `matrix_applicability.surfaces` で宣言する。missing surface は `matrix_applicability.note` で理由を残す | `band:gcc13_14`, `processing_path:native_text_capture`, `surface:default`, `surface:ci`, optional `surface:debug`, `fallback_contract:honest_fallback`, `matrix_applicability` |
| `GCC13-14` | `SingleSinkStructured` | `default`, `ci`, `debug` | explicit structured capture の bounded render を fixture で明示し、checked-in replay expectation が存在する surface を `surface:*` と `matrix_applicability.surfaces` で宣言する。missing surface は `matrix_applicability.note` で理由を残す | `band:gcc13_14`, `processing_path:single_sink_structured`, `surface:default`, `surface:ci`, optional `surface:debug`, `fallback_contract:bounded_render`, `matrix_applicability` |
| `GCC9-12` | `NativeTextCapture` | `default`, `ci`, `debug` | useful-subset family では bounded render か honest fallback のどちらを期待するかを fixture で明示し、checked-in replay expectation が存在する surface を `surface:*` と `matrix_applicability.surfaces` で宣言する。missing surface は `matrix_applicability.note` で理由を残す | `band:gcc9_12`, `processing_path:native_text_capture`, `surface:default`, `surface:ci`, optional `surface:debug`, `fallback_contract:bounded_render`, `matrix_applicability` |
| `GCC9-12` | `SingleSinkStructured` | `default`, `ci`, `debug` | explicit JSON structured capture の bounded render を fixture で明示し、checked-in replay expectation が存在する surface を `surface:*` と `matrix_applicability.surfaces` で宣言する。missing surface は `matrix_applicability.note` で理由を残す | `band:gcc9_12`, `processing_path:single_sink_structured`, `surface:default`, `surface:ci`, optional `surface:debug`, `fallback_contract:bounded_render`, `matrix_applicability` |

representative fixture を gate に含めるときは、`VersionBand` を潰さず、`ProcessingPath × Surface` ごとの coverage を集計すること。report は後方互換のため `VersionBand × ProcessingPath` 集計も残してよいが、missing cell の判定は `VersionBand × ProcessingPath × Surface` を正とする。`debug` surface を宣言した fixture は explainability signal と suppressed-group visibility の差分を replay で検証できなければならない。`debug` を宣言しない fixture は、`matrix_applicability.note` で omission reason を残さなければならない。

representative replay には、別途 anti-collision corpus slice を持たせなければならない。anti-collision fixture は `anti_collision` tag と scenario tag を持ち、少なくとも `same_file_dual_syntax`, `syntax_flood_plus_type`, `template_frontier_independent` を 1 件以上ずつ含むこと。Band coverage は `gcc15/dual_sink_structured`, `gcc13_14/native_text_capture`, `gcc13_14/single_sink_structured`, `gcc9_12/native_text_capture`, `gcc9_12/single_sink_structured` を必須とする。

anti-collision replay report は、default surface で `false_hidden_suppression_count == 0` と `independent_root_recall_rate == 1.0` を満たさなければならない。ここで recall は independent root が expanded または summary-only として残った割合を指し、hidden は `visibility_floor != hidden_allowed` の group が default output から完全に消えた件数を指す。

---

## 12. Fixture モデル

### 12.1 fixture directory 例

```text
corpus/
  curated/
    c/
      syntax/
        missing-semicolon/
          src/
            main.c
          invoke.yaml
          expectations.yaml
          meta.yaml
          snapshots/
            gcc13_14/
              native_text_capture/
                ingress.stderr.txt
                ingress.sarif.json
                ir.facts.json
                ir.analysis.json
                view.default.json
                render.default.txt
                render.ci.txt
    cpp/
      templates/
        no-matching-constructor/
          ...
  fuzz-seeds/
  harvested-index/
```

### 12.2 必須ファイル

| ファイル | 必須 | 目的 |
|---|---:|---|
| `src/` | MUST | 最小再現ソース |
| `invoke.yaml` | MUST | compiler, args, env, cwd policy |
| `expectations.yaml` | MUST | semantic assertions と gate 対象 |
| `meta.yaml` | MUST | tags, ownership, provenance, reviewer info |
| `snapshots/<version_band>/<processing_path>/...` | MUST | ingress / IR / render goldens |
| `README.md` | SHOULD | fixture の背景、意図、注意点 |

`snapshots/gcc15/` の implicit root layout は legacy compatibility residue としてのみ許容する。新規 fixture と normalized fixture は `snapshots/<version_band>/<processing_path>/` を正とし、presentation-specific artifact がある場合も `snapshots/<version_band>/<processing_path>/<preset>/` にネストすること。

### 12.3 `invoke.yaml` で持つべき項目

- language / standard
- target compiler family (`gcc`)
- required version band / support level
- major version selector
- argv
- cwd policy
- env overrides
- source readability expectation
- linker involvement
- expected mode (`render` / `passthrough` / `shadow`)
- canonical path policy

### 12.4 `expectations.yaml` で持つべき項目

最低限以下を持つ。

- expected version band / processing path / support level
- expected family / subfamily
- expected lead group family
- allowed lead group set（必要時）
- expected severity
- expected fallback (`allowed` / `forbidden` / `required`)
- expected primary location(s)
- expected first action presence
- expected omission notice presence
- expected cascade episode / root / follow-on / duplicate / uncertain counts
- expected summary-only / hidden / suppressed group counts per surface
- expected raw provenance retention
- allowed integrity issue codes
- allowed compiler drift notes
- confidence floor / ceiling（必要時）

### 12.5 `meta.yaml` で持つべき項目

- corpus id
- human title
- source provenance (`hand-authored`, `minimized-from-shadow`, etc.)
- redaction class
- owner team
- last reviewed date
- reviewer(s)
- tags (`syntax`, `template`, `linker`, `ci-critical`, etc.)
- tags (`syntax`, `template`, `linker`, `ci-critical`, `band:gcc15`, `band:gcc13_14`, `band:gcc9_12`, `processing_path:dual_sink_structured`, `processing_path:native_text_capture`, `processing_path:single_sink_structured`, `surface:default`, `surface:ci`, optional `surface:debug`, `fallback_contract:bounded_render`, `fallback_contract:honest_fallback`, etc.)
- representative fixture では `matrix_applicability.version_band`, `matrix_applicability.processing_path`, `matrix_applicability.surfaces`
- representative fixture で stop-ship surface を intentionally omit するときの `matrix_applicability.note`
- promotion status (`seed`, `curated`, `deprecated`)
- notes on known version drift

---

## 13. Goldens / snapshots / oracles の分離

### 13.1 golden artifact の 5 層

各 curated fixture では、可能な限り以下を保存する。

1. **Ingress goldens**
   - `stderr.raw`
   - `sarif.raw`
   - optional residual stderr
2. **Canonical facts snapshot**
   - `ir.facts.json`
3. **Canonical analysis snapshot**
   - `ir.analysis.json`
4. **Public machine-readable snapshot**
   - `public.export.json`
5. **View model snapshot**
   - `view.<profile>.json`
6. **Rendered text snapshot**
   - `render.<profile>.txt`

### 13.2 なぜ 5 層持つのか

変更の影響範囲を切り分けるためである。

| 変化 | まず壊れる層 | 原因の典型 |
|---|---|---|
| compiler drift | ingress | GCC patch/major 差分 |
| adapter bug | facts / public export | SARIF mapping / residual parse / projection bug |
| ranking change | analysis / public export / view | lead group / first action rules |
| renderer bug | view / render | omission / layout / profile handling |
| cosmetic change | render | wording / spacing / section title |

### 13.3 semantic oracle を snapshot より上位に置く

snapshot diff は reviewer に可視性を与えるが、**pass/fail の本体は semantic oracle** であるべきである。

例:
- lead group は `group-1` または `group-2` のどちらでもよい
- confidence は `medium` 以上でよい
- primary location は exact column ではなく location set に含まれればよい

これにより、不要な brittle failure を減らす。

---

## 14. Snapshot canonicalization policy

### 14.1 canonical snapshot の基本規則

- JSON key は辞書順
- 改行は LF
- 文字コードは UTF-8
- path separator は platform-native ではなく canonical path policy に従う
- 非決定的 id は stable derivation か canonical renumbering を行う
- timestamps / temp paths / random suffix は snapshot から除外または正規化する
- `public.export.json` は versioned public contract として保持し、report bundle では `public.export.schema-shape-fingerprint.txt` を compatibility sentinel として併置してよい

### 14.2 text snapshot の canonical capability

`render.default.txt` の canonical 条件:

- width = 100
- color = off
- unicode = off（ASCII-safe）
- hyperlinks = off
- stream_kind = `pipe`
- path policy = `relative_to_cwd`

別途 `render.ci.txt` と `render.verbose.txt` を持ってよい。

### 14.3 stable と volatile の分離

以下は原則 volatile とみなし、text snapshot で exact match を要求しない設定も許可する。

- temporary file names
- GCC patch version banner
- OS-specific linker wording の一部
- source availability による excerpt 欠落理由文の詳細

ただし、**volatile 扱いは例外**であり、濫用してはならない。

### 14.4 snapshot 更新ルール

snapshot 更新は単なる `bless` で終えてはならない。  
変更 PR では以下を必須とする。

- change reason label
- affected layers
- user-visible impact summary
- if lead group changed, rationale
- if omission counts changed, rationale
- if fallback behavior changed, explicit ack

---

## 15. Real compiler matrix

real compiler matrix は、`GCC15` / `GCC13-14` / `GCC9-12` を 1 つの in-scope public contractとして path-aware に評価する、という current support boundary を test に写したものである。 [R1][R2]

### 15.1 compiler version matrix

| Band | 対象 | gate レベル |
|---|---|---|
| `GCC15` representative lane | GCC 15 latest patch | PR / nightly / release |
| `GCC13-14` representative lane | GCC 13 latest patch | PR / nightly / release |
| `GCC13-14` additional evidence lane | GCC 14 latest patch | nightly / release |
| `GCC9-12` representative lane | GCC 12 latest patch or distro default | PR / nightly / release |

### 15.2 mode matrix

| Version | `render` | `shadow` | `passthrough` | 備考 |
|---|---|---|---|---|
| GCC 15 | MUST | MUST | MUST | shared in-scope contract; dual-sink default capability |
| GCC 14 | MUST on `NativeTextCapture`; explicit `SingleSinkStructured` is allowed | MUST | MUST | shared in-scope contract; additional evidence lane inside `gcc13_14` |
| GCC 13 | MUST on `NativeTextCapture`; explicit `SingleSinkStructured` is allowed | MUST | MUST | shared in-scope contract; representative `gcc13_14` lane |
| GCC 12 | MUST on `NativeTextCapture` useful-subset representative fixtures; explicit `SingleSinkStructured` (JSON) is allowed and MUST be tagged separately | MUST | MUST | shared in-scope contract; representative `gcc9_12` lane |

Band B / Band C の fixture / gate では、`GCC13-14` / `GCC9-12` をまとめて扱うのではなく、`ProcessingPath × Surface` ごとに `bounded_render` と `honest_fallback` を区別すること。

### 15.3 environment matrix

最低限以下を cover する。

- glibc 系 Linux（primary）
- distro-default GCC image
- non-interactive CI shell
- TTY / pseudo-TTY
- narrow width / wide width
- readable source file / deleted source file / permission denied
- `LC_MESSAGES=C` と user locale
- long path / symlink / relative cwd
- linker present / absent / alternate linker selected

### 15.4 language matrix

最低限以下を seed / curated で cover する。

- C11 / C17 / C23
- C++17 / C++20 / C++23
- warning as error 有/無
- preprocessing heavy case
- template heavy case
- pure compile / compile+link

### 15.5 linker matrix

少なくとも以下を family として区別する。

- GCC driver emitted
- `collect2`
- GNU ld.bfd
- gold
- lld
- mold
- assembler residuals

**MUST**:
- linker family ごとに text residual parser の期待を分ける
- 同一 renderer 契約を強制しすぎない

### 15.6 compiler drift triage

real compiler matrix で差分が出た場合、必ず以下のいずれかに分類する。

1. **expected compiler drift**
   - GCC patch/major の自然差分
   - baseline 更新で吸収可能
2. **adapter fidelity regression**
   - wrapper 側の mapping/parse 問題
3. **renderer/analysis regression**
   - facts は同じだが user-visible behavior が悪化
4. **environmental drift**
   - distro package, linker, locale, terminal capability 差異
5. **upstream compiler bug or incompatibility**
   - 製品側回避が必要か、support level や processing path を見直すか判断

**MUST**:
- drift は「snapshot を更新して終わり」にしてはならない
- triage label を issue / PR に残す
- GCC major baseline は major ごとに分離する

---

## 16. Harness-normalized capture policy

### 16.1 目的

差分比較のため、raw compiler baseline を安定化した control path を harness 側に持つ。  
これは製品 hot path とは別物である。

### 16.2 GCC で許される harness-only stabilization

公式ドキュメントには `-fdiagnostics-plain-output` があり、`dejagnu` などパース用途に有用とされている。  
したがって harness-only mode では、必要に応じてこれを利用して baseline を安定化してよい。 [R3]

### 16.3 Clang での将来方針

Clang には parseable fix-its と parseable source ranges がある。将来の cross-compiler harness ではこれらを comparison oracle の補助に使ってよい。 [R4]

### 16.4 禁止事項

- harness-normalized output を product-visible UX の評価と混同してはならない
- harness 専用 flags を製品 default path に混入してはならない
- normalized baseline の方が見やすいからといって、製品仕様をそれに引きずってはならない

---

## 17. Semantic assertion catalog

各 fixture の `expectations.yaml` では、少なくとも以下の assertion class を使えるようにする。

### 17.1 structural assertions

- parse succeeded / partial / passthrough
- selected version band / processing path / support level
- fallback verdict
- integrity issue presence/absence
- raw artifact presence

### 17.2 semantic assertions

- diagnostic family / subfamily
- severity
- lead group family
- primary location set
- related location set
- presence of declaration/use pair
- presence of template/macro/include/linker context chain
- suggestion availability / machine applicability
- cascade episode / independent-root count
- cascade follow-on / duplicate / uncertain count
- confidence band
- first action presence

### 17.3 rendering assertions

- first screenful line budget
- omission notice presence
- summary-only / hidden / suppressed group count per surface
- raw message retained
- path-first CI output
- color-free readability
- no color-only meaning
- no silent truncation
- no secret leak

### 17.4 performance assertions

- max parse time
- max render time
- max peak RSS
- max snapshot size
- no quadratic blow-up on designated pathological cases

---

## 18. Merge gate / nightly gate / release gate

### 18.1 Local fast gate

開発者がローカルで高速に回す gate。  
目標実行時間は **3 分以内**。

含むべきもの:
- unit tests
- schema validation
- changed fixture subset
- deterministic canonical snapshot check
- formatter / linter
- small benchmark smoke test

### 18.2 PR gate

目標実行時間は **10 分以内**。  
含むべきもの:

- local fast gate の全項目
- changed-area adapter contract tests
- changed-area renderer/view model tests
- representative GCC 15 E2E subset
- performance smoke budget
- no-open P0 in touched areas

**MUST**:
- snapshot update は reviewer に diff が見える形式で提出する
- changed semantic assertions があれば rationale を必須とする

### 18.3 Nightly gate

含むべきもの:

- full curated corpus replay
- GCC 15 / 14 / 13 matrix
- render profile matrix
- full benchmark suite
- fuzz corpus replay
- harvested trace replay subset
- flake detection

nightly は以下の役割を持つ。

- patch version drift 検出
- flaky test 検出
- performance regression 検出
- version-band / processing-path regression の早期検出

### 18.4 Release candidate gate

release 候補では少なくとも以下を満たす。

- curated corpus pass **100%**
- expected rollout modes の matrix pass **100%**
- P0 / P1 open bug **0**
- unexpected fallback rate threshold pass
- benchmark budgets pass
- fuzz crash **0**
- deterministic replay pass
- UX review sign-off 完了

### 18.5 Rollout readiness gate

社内広域 rollout 前に追加で必要なもの:

- shadow mode で enhanced-eligible run の十分なサンプル数
- unexpected fallback < **0.1%**
- high-confidence mislead < **2%**
- top 10 diagnostic family で raw GCC 比非劣化
- trace bundle redaction 監査 pass
- support / rollback 手順ドキュメント完備

#### 最低サンプル要件（推奨）

rollout readiness を名乗るには、少なくとも以下のいずれかを満たすことが望ましい。

- **10,000 以上の enhanced-eligible invocation** かつ **500 以上の failure invocation**
- または、複数チーム/複数 repo にまたがる **3 週間以上の shadow 運用**

理由は、単一 repo の偏った build パターンだけで fallback / mislead を評価すると、実運用の分布を大きく見誤るためである。

---

## 19. Flake policy

### 19.1 flake の定義

同一 commit、同一 environment、同一 fixture で verdict が揺れるものを flake と定義する。

### 19.2 許容方針

- PR gate: flake **0 許容**
- nightly: 一時的検知は可、ただし 7 日移動平均で **0.5% 未満**
- release gate: flake **0 許容**

### 19.3 flake の主因類型

- temp path / timestamp 混入
- unstable ordering
- locale 漏れ
- filesystem race
- real compiler patch drift
- benchmark noisy neighbor
- nondeterministic hash iteration

flake は「テストの問題」で片付けず、原則として product / harness のどちらかの bug として扱う。

---

## 20. Performance and resource budgets

### 20.1 基本 budget

| シナリオ | 指標 | 目標 |
|---|---|---|
| success path | wrapper overhead p95 | `< 40ms` |
| simple failure | postprocess p95 | `< 80ms` |
| template-heavy failure | postprocess p95 | `< 250ms` |
| typical failure | peak RSS | `< 128 MiB` |
| pathological failure | peak RSS | `< 256 MiB` |
| artifact I/O | sidecar + trace write overhead | bounded / streaming |

### 20.2 failure family 別予算

- syntax / single type mismatch: strict budget
- template explosion / huge linker stderr: relaxed budget
- malformed huge SARIF: safety budget優先、速度は二次

### 20.3 benchmark 判定

performance は absolute threshold だけでなく、baseline 比でも監視する。

- PR gate: smoke threshold のみ
- nightly: previous main branch 比
- release: designated baseline release 比

**MUST**:
- benchmark は versioned fixture set に対して実行する
- 参照 CPU / memory / storage 条件を記録する
- designated checked-in benchmark baseline がある場合、report は scenario-level `p95_ms` delta を machine-readable に記録する

---

## 21. Robustness / fuzz / adversarial gate

### 21.1 fuzz target

最低限以下を持つ。

- SARIF ingestion parser
- residual stderr classifier
- canonical snapshot loader
- view model builder
- renderer layout engine
- redaction pipeline

### 21.2 success criteria

- crash しない
- panic/abort しない
- unbounded allocation を起こさない
- terminal escape injection をしない
- invalid UTF-8 でも safe rendering or replacement を行う

### 21.3 adversarial corpus

fuzz とは別に、人手で作る adversarial corpus を持つ。

例:
- 10,000 行の repeated notes
- 1,000 階層近い template-like names
- pathological macro nesting
- broken path separators
- source line に ESC 文字列
- huge columns / very long physical lines

---

## 22. UX quality evaluation

### 22.1 automation だけでは足りない理由

この製品の成否は「修正しやすさ」にあるため、snapshot pass だけでは不十分である。  
少なくとも release candidate ごとに human evaluation を行う。

### 22.2 UX review の 3 層

1. **expert fixture review**
   - compiler に詳しい reviewer が changed fixtures を確認する
2. **task-based internal study**
   - 開発者が実際に問題を直す
3. **shadow telemetry analysis**
   - field での fallback / mislead / abandonment を確認する

### 22.3 expert review checklist

- lead group は root cause に近いか
- first action は first screenful にあるか
- omission notice は正直か
- template / macro / include / linker の圧縮は妥当か
- confidence 表現は適切か
- raw facts に戻れるか
- CI で path-first か
- noise が raw GCC より悪化していないか

### 22.4 task study 最低要件

各 release candidate で最低以下を目標とする。

- 10 名以上の internal participants
- 10 ケース以上
- syntax / type / template / macro / linker を含む
- C-first operator packet として `compile`, `link`, `include_path`, `macro`, `preprocessor`, `honest_fallback` を少なくとも 1 件ずつ含む
- raw GCC との A/B もしくは counterbalanced 比較

### 22.5 release sign-off 条件

以下のいずれかを満たさない限り release sign-off を出さない。

- TRC / TFAH / first-fix success が raw GCC より有意に悪化しない
- 主要 family での expert review が許容を出す
- mislead case が既知で管理され、confidence/passthrough により抑止されている

---

## 23. Shadow mode と corpus promotion

### 23.1 shadow の役割

shadow mode は rollout と corpus 拡張のための mode であり、user-visible behavior を極力変えない。  
shadow から得るのは「現実の偏り」であって、そのまま契約ではない。

### 23.2 harvested trace の取り込み条件

harvested trace は少なくとも以下を持つ。

- raw stderr
- optional SARIF sidecar
- compiler version
- argv hash / normalized invocation metadata
- environment summary
- wrapper verdict
- fingerprint
- redaction status

### 23.3 promotion pipeline

```text
shadow trace
   │
   ├─ sanitize
   ├─ dedup by fingerprint
   ├─ classify family
   ├─ minimize / reproduce
   ├─ add expectations
   ├─ reviewer sign-off
   ▼
curated corpus
```

### 23.4 promotion rule

harvested trace を curated に昇格させるには、少なくとも以下が必要。

- 再現可能な minimal or bounded repro
- redaction review pass
- version band / processing path / family が明確
- semantic assertions が書かれている
- reviewer 2 名以上の承認（少なくとも 1 名は compiler/diagnostics 事情に詳しい）

---

## 24. Privacy / redaction / artifact retention

### 24.1 基本方針

- production source やパスを無制限に保存してはならない
- curated corpus は hand-authored または minimized repro を原則とする
- shadow trace は default で org 外送信禁止
- trace bundle は opt-in / sanitized / retention-bounded で扱う

### 24.2 redaction class

| クラス | 内容 | 例 |
|---|---|---|
| `none` | 公開可能な hand-authored fixture | 自作 minimal repro |
| `path-redacted` | パスのみ秘匿 | home directory を hash 化 |
| `snippet-redacted` | source excerpt を伏せる | field trace |
| `restricted` | commit 不可、secure storage のみ | 実運用 trace |

### 24.3 secret / privacy guard

**MUST**:
- home path, username, repo path, temp dir token を検出・正規化できる
- source excerpt の commit 前 scan を行う
- harvested trace を CI artifact として長期保持しすぎない

---

## 25. Clang を壊さない test harness 設計

### 25.1 backend-neutral scenario を first-class にする

fixture の top-level taxonomy は compiler 名ではなく **semantic scenario** を基準とする。

良い例:
- `cpp/templates/no-matching-constructor`
- `c/linker/undefined-reference`

悪い例:
- `gcc-only/weird-output-01`

### 25.2 backend-specific snapshot を分離する

同じ scenario に対して、以下を分ける。

- shared semantic expectations
- gcc-specific ingress / extensions
- clang-specific ingress / extensions（将来）

### 25.3 compare core, isolate extensions

future Clang support のため、test harness は以下を比較できるようにする。

- core IR fields
- shared renderer contract
- backend-specific extension snapshot

これにより、「GCC にしかない property bag」や「Clang の parseable range」を分離できる。

---

## 26. Review policy

### 26.1 fixture 追加レビュー

新規 curated fixture 追加時は以下を確認する。

- 本当に最小再現か
- family が妥当か
- expectations が semantic かつ十分か
- snapshot が過剰に brittle でないか
- sensitive data が無いか
- version band / processing path / mode が妥当か

### 26.2 snapshot diff review

snapshot diff reviewer は次を確認する。

- 変化は意図されたものか
- lead group / first action が悪化していないか
- omission notice が消えていないか
- raw provenance が失われていないか
- CI profile が path-first を維持しているか

### 26.3 release quality review

release 前には、少なくとも以下の role が必要。

- adapter owner
- renderer/UX owner
- QA/corpus owner
- release manager

---

## 27. 変更管理

### 27.1 corpus versioning

corpus には以下を持たせる。

- `schema_version`
- `fixture_version`
- `snapshot_version`
- `promotion_status`

### 27.2 incompatible change rule

以下の変更は incompatible とみなす。

- expectation schema の breaking change
- canonical path policy の変更
- profile canonical capability の変更
- lead group semantics の再定義
- version band / processing path matrix の変更

これらは ADR または仕様改訂を伴うべきである。

### 27.3 deprecating fixtures

obsolete な fixture は即削除してはならない。  
以下の手順を踏む。

1. `deprecated` マーク
2. replacement fixture を追加
3. 2 release cycle 以上保持
4. 参照先 gate から除外後に削除

---

## 28. フェーズ別 done 条件

### 28.1 Phase 0 / architecture validation

Done 条件:

- seed corpus 20 件以上
- IR validation と canonical facts snapshot が動く
- GCC 9-15 representative E2E が 5 family 以上通る
- performance smoke harness が存在する
- fuzz seed が 10 件以上ある

### 28.2 Phase 1 / GCC-first MVP

Done 条件:

- seed/curated 合計 50 件以上
- syntax / type / template / macro / linker を含む
- PR gate / nightly gate が動く
- GCC 15 render / shadow / passthrough の gate が確立
- P0 bug 0
- basic UX review 完了

### 28.3 Phase 2 / hardening

Done 条件:

- curated corpus 80〜129 件
- GCC 13/14 product-path gate 安定
- unexpected fallback tracking 可能
- harvested trace promotion pipeline 運用開始
- release candidate gate と rollout gate が定義済み

### 28.4 Phase 3 / advanced analysis

Done 条件:

- high-confidence mislead rate が定量化されている
- template-heavy / linker-heavy family で raw GCC 比優位
- view model tests と semantic assertions が充実
- adversarial corpus が実戦的規模に達する

---

## 29. 実装上の推奨事項

この節は規範ではないが、実装で強く推奨する。

### 29.1 replay-first harness

real compiler を毎回起動しなくても adapter / analysis / renderer を replay できる harness を先に作るべきである。  
これにより以下が容易になる。

- regression bisect
- snapshot review
- version drift 比較
- fuzz crash minimization
- renderer 改善の高速反復

### 29.2 changed-area test selection

大きな corpus を全部 PR で回すのは重い。  
changed area に応じて fixture subset を選ぶ戦略を持つべきである。

例:
- adapter 変更 → ingress / facts / E2E 強化
- renderer 変更 → view model / render snapshots 強化
- redaction 変更 → privacy fixtures 強化

### 29.3 result explanation for failed gate

CI は単に「snapshot mismatch」とだけ出してはならない。  
少なくとも以下を表示するべきである。

- failed layer
- failed fixture count
- first failed fixture
- changed semantic assertions
- changed lead group / fallback / performance budget
- artifact links

---

## 30. 非目標

本仕様は以下を goal にしない。

1. GCC の全 testsuite を mirror すること
2. patch version 差異を 0 にすること
3. text snapshot だけで品質を語ること
4. field trace をそのまま golden にすること
5. 全 linker の全文法を初期段階で網羅すること
6. 人間の UX review を不要にすること
7. one-click auto-fix の正しさ評価をこの仕様で完結させること

---

## 31. 受け入れ基準（この仕様書自体の Done）

この仕様書に基づく初期実装は、少なくとも以下を満たせば「実装開始可能」とみなす。

1. curated fixture schema が確定している
2. ingress / facts / render の 3 層 golden が回る
3. semantic assertions の最小 schema が確定している
4. PR / nightly / release gate の責務分担が決まっている
5. P0/P1 defect taxonomy が合意されている
6. GCC 15 / 14 / 13 の matrix 方針が固定されている
7. shadow-to-curated promotion の流れが決まっている
8. privacy/redaction の最低ルールが決まっている
9. Clang を壊さない shared scenario 方針が確定している

---

## 32. 推奨する最初の 30 日の作業順

1. seed corpus 20〜30 件を hand-authored で作る
2. replay harness を先に作る
3. `expectations.yaml` schema を固定する
4. canonical facts snapshot を出せるようにする
5. renderer view model snapshot を導入する
6. GCC 9-15 representative E2E subset を組む
7. PR gate と nightly gate を最小構成で通す
8. shadow trace の sanitize 形式を設計する
9. first expert review を実施し、family taxonomy を調整する

---

## 33. 参考文献

- [R1] [GCC 15 Release Series — Changes, New Features, and Fixes](https://gcc.gnu.org/gcc-15/changes.html)  
  GCC 15 で JSON diagnostics が deprecated であり、`-fdiagnostics-add-output=` による複数形式同時出力が導入されたことを確認するために参照。
- [R2] [GCC 13 Release Series — Changes, New Features, and Fixes](https://gcc.gnu.org/gcc-13/changes.html)  
  GCC 13 で SARIF 出力が導入されたことを確認するために参照。
- [R3] [Diagnostic Message Formatting Options (Using the GNU Compiler Collection)](https://gcc.gnu.org/onlinedocs/gcc/Diagnostic-Message-Formatting-Options.html)  
  `-fdiagnostics-plain-output` が parser/dejagnu 向けの安定化オプションとして説明されていることを確認するために参照。
- [R4] [Clang Users Manual — Formatting of Diagnostics](https://clang.llvm.org/docs/UsersManual.html)  
  `-fdiagnostics-print-source-range-info` と `-fdiagnostics-parseable-fixits` があることを確認し、将来の differential harness 設計の補助根拠とするために参照。

---

## 34. 付録 A: `expectations.yaml` の概念例

```yaml
schema_version: 1
fixture_id: cpp/templates/no-matching-constructor
version_band: gcc15
processing_path: dual_sink_structured
support_level: in_scope
expected_mode: render

semantic:
  family: template.no_matching_constructor
  severity: error
  lead_group_any_of:
    - ctor-no-match
    - template-instantiation-root
  primary_locations:
    - path: src/main.cpp
      line: 18
  first_action_required: true
  raw_provenance_required: true
  fallback: forbidden
  confidence_min: medium

render:
  default:
    omission_notice_required: true
    first_screenful_max_lines: 24
    expected_summary_only_group_count: 1
  ci:
    path_first_required: true
    color_meaning_forbidden: true
    expected_summary_only_group_count: 1
  debug:
    expected_summary_only_group_count: 0
    expected_hidden_group_count: 0
    expected_suppressed_group_count: 0
    required_substrings:
      - "debug: rule_id="

cascade:
  expected_independent_episode_count: 2
  expected_independent_root_count: 1
  expected_dependent_follow_on_count: 0
  expected_duplicate_count: 0
  expected_uncertain_count: 1

integrity:
  allowed_issue_codes: []

performance:
  parse_time_ms_max: 40
  render_time_ms_max: 30
```

---

## 35. 付録 B: reviewer checklist（簡略）

- [ ] fixture は最小再現か
- [ ] redaction は十分か
- [ ] semantic assertion は snapshot 依存になっていないか
- [ ] lead group 変更の理由は明確か
- [ ] first action は適切か
- [ ] omission notice は正直か
- [ ] raw provenance は維持されているか
- [ ] CI output は path-first か
- [ ] fallback / passthrough 変更は明示されているか
- [ ] future Clang shared scenario を壊していないか
