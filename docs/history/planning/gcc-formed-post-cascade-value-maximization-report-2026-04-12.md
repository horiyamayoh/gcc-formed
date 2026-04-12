---
doc_role: history-only
lifecycle_status: legacy
audience: both
use_for: Historical post-cascade planning context and issue-draft decomposition.
do_not_use_for: Current implementation authority, active prioritization, or canonical support wording.
supersedes: []
superseded_by: []
---
> [!IMPORTANT]
> Authority: `history-only` / `legacy`
> Use for: historical post-cascade planning context and issue-draft decomposition.
> Do not use for: implementation truth or active prioritization. Defer to `README.md`, `docs/support/SUPPORT-BOUNDARY.md`, `docs/process/EXECUTION-MODEL.md`, accepted ADRs, and current specs.

# gcc-formed ポスト cascade 製品価値最大化レポート
**対象リポジトリ**: <https://github.com/horiyamayoh/gcc-formed>  
**作成日**: 2026-04-12 (JST)  
**可変情報の確認時点**: GitHub の visible release / Actions / repo landing は 2026-04-12 JST 時点。issue 起票時に再確認すること。  
**前提**: 現在の cascade 系 workstream（#115 付近〜の一連の issues。起票時に最新の open/close 状態を再確認すること）はもうすぐ実装完了すると仮定し、本レポートでは **cascade 本体は扱わない**。  
**目的**: `gcc-formed` を「ごっこ遊びではなく、本当にガチで使える GCC 診断オペレーティングレイヤ」にするため、**post-cascade の価値最大化領域**を特定し、**GitHub issue draft へ落とせる粒度**まで分解する。  
**命名方針**: 既存の cascade 側の Epic / WP 命名と衝突しないよう、本書では post-cascade の Epic / WP を **`P*` 系**で採番する。

---

## 0. このレポートの読み方

このレポートは次の 4 つを同時に満たすように作っている。

1. **いま何が repo の公開面で危険か**がすぐわかる  
2. **post-cascade でどこを掘ると製品価値が最も伸びるか**が優先順でわかる  
3. **GitHub Epic / Work Package issue draft の親子構造をすぐ作れる**  
4. **1 Issue = 1 PR = 1 主目的** を守れるように、WP を 1000 行前後の bounded scope に落としてある

本書は「思いつきの改善案一覧」ではなく、**issue draft bundle としてそのまま叩き台にできる実行レポート**である。

### 0.1 文書の authority と issue 化ルール

- 本書は `history-only` の historical planning document であり、**current-authority ではない**。
- Epic / WP はそのまま merge 判断や実装 authority に使わず、**起票前に `docs/process/EXECUTION-MODEL.md` の current taxonomy に正規化**する。
- 特に各 WP には、起票時に `Affected layers`、`Rollback / abandon rule`、必要なら `Depends On` / `Night Batch` を補う。
- `Agent Ready = Ready` は、最終 issue body が current `EXECUTION-MODEL.md` の ready 条件を満たした時だけ付与する。
- `README` / support docs / templates / workflows を同時に触る計画や、`Human Review Type = Design` の計画は、原則 `Draft` のまま分割要否を先に判断する。
- machine-readable output と trace bundle は既存の accepted baseline を置き換える話ではなく、**既存 baseline を public surface / operator surface として製品化する**話として扱う。

---

## 1. エグゼクティブサマリ

### 1.1 結論
cascade の次に最も効くのは、診断アルゴリズムの追加ではない。  
**公開面の信頼性**、**Make / CMake への実戦導入性**、**AI/自動化向けの deterministic surface**、**support / replay の現場運用性**、**大規模ビルドでの無害性**、**GCC9-12 / C-first の本当の製品化**である。

### 1.2 post-cascade の勝ち筋
`gcc-formed` は以下の 6 本柱で価値を最大化すべきである。

1. **P1: Trustworthy public entry surfaces**  
   README / Release / Templates / Repo metadata / current-authority docs の公開面を 1 つの真実に揃える

2. **P2: Build-chain-native adoption**  
   GCC 固定、Make / CMake 固定の現場に、wrapper を “drop-in” で入れられるようにする

3. **P3: Public machine-readable output**  
   AI や CI が screen scrape しなくて済む、stable / versioned / deterministic JSON surface を出す

4. **P4: Trace bundle / replay productization**  
   現場で事故ったときに、1 コマンドで trace bundle を作り、maintainer が replay できるようにする

5. **P5: Performance / scale / concurrency hardening**  
   成功 path では邪魔をせず、巨大 stderr や `make -j` でも壊れないことを製品契約にする

6. **P6: GCC9-12 / C-first product truth**  
   「古い GCC / C 案件でも使える」を docs の文言でなく、artifact と gate で証明する

### 1.3 最初に止めるべきこと
post-cascade では、少なくとも最初の 1 波は以下をやらない。

- IDE widget / editor hover / GUI を先に作る
- AI の自由文説明を core hot path に入れる
- non-Linux artifact を広げる
- GCC 以外の compiler へ主戦場を広げる
- 新しい診断 family を無限追加する
- “すごそうに見えるが、本番導入の friction を下げない” 施策に時間を使う

---

## 2. 最新 repo snapshot とそこから見える課題

### 2.1 今見えている repo の強み
現時点の main branch から読み取れる強みは明確である。

- README は `Public Beta` / `v1beta` / `0.2.0-beta.N` を明示している
- README は **GCC15+ だけでなく GCC13-14 と GCC9-12 も first-class product bands** として扱っている
- packaging / install / rollback / exact-pin / release-repo / signing まで土台がある
- `diag_adapter_contract` / `diag_adapter_gcc` / `diag_backend_probe` / `diag_capture_runtime` / `diag_cascade` / `diag_cli_front` / `diag_core` / `diag_enrich` / `diag_render` / `diag_residual_text` / `diag_rulepack` / `diag_testkit` / `diag_trace` / `xtask` の 14 crate で workspace が分かれており、bounded WP を切りやすい（なお `diag_cascade` は cascade workstream の中核であり、本レポートのスコープ外である）
- issue / epic / work package template と PR template がすでに整っている

### 2.2 いま公開面で危険なこと
一方で、**製品として最初に疑われる箇所**もかなりはっきりしている。

#### A. current-authority docs と latest public release page の言い分がズレている
main README と support boundary は GCC13-14 / GCC9-12 を in-scope product band としている。  
しかし、latest visible prerelease の GitHub Release body はなお **GCC13/14 compatibility-only** と書いており、しかも旧 commit を指す docs link が混ざっている。

これは「内部では前進しているが、公開面では古い truth が出ている」状態であり、**導入検討者にとっては stop-ship 級の不信要因**になる。

#### B. About 欄が空
repo の About が **description / website / topics なし** で空なのは、初見ユーザにとって「何者かわからない」状態である。  
README が強くても、repo landing はそれだけで離脱を生む。

#### C. Actions の visible surface が赤い
最新の visible Actions runs では、`pr.yml` / `nightly.yml` / `rc-gate.yml` の failure が最近の複数 commit で並んでいる。  
中身が product bug か infra failure かに関係なく、**“mainline が不安定に見える” こと自体が製品価値を毀損**する。

#### D. 開発導線は強いが、導入導線が弱い
README の「開発開始」は `cargo xtask check` / `cargo build` / `--formed-self-check` / `cargo xtask replay` から始まる。  
これは maintainer には良いが、**GCC 固定の現場ユーザが最初に知りたいのは `CC=gcc-formed` / `CXX=g++-formed` / Make / CMake でどう入るか** である。

#### E. AI 向けの明確な public deterministic surface はまだ弱い
IR spec と renderer spec は machine-readable export / editor transport / plugin API を「別層」として切り分けており、これは正しい。  
しかし、**本当に人間と AI の両方に最高の体験を与える**なら、screen scrape でなく、stable JSON で読む public surface が必要になる。

### 2.3 ここから導く最重要仮説
post-cascade の主戦場は **“診断をさらに賢くすること” だけではなく**、  
**“この repo を信用して入れられる / 現場で回る / AI が壊さずに読める / 事故っても回収できる”** に移っている。

---

## 3. post-cascade の製品仮説

### 3.1 gcc-formed が勝つべき定義
`gcc-formed` は「GCC を prettier にするツール」では弱い。  
勝つ定義は次である。

> **変更できない GCC toolchain のための、trustworthy diagnostic operating layer**

つまり、

- compiler は GCC のまま
- language は C / C++ のまま
- build system は Make / CMake のまま
- しかし診断体験だけは世代を一段上げる

この定義なら、Rust/Haskell の“新言語の快感”とは違う軸で勝てる。  
**レガシー制約を受け入れたまま UX を革命する** のが本製品の独自性である。

### 3.2 Human + AI 両方に効かせる方法
AI 依存を core に入れるのではなく、次の二層構造にする。

- **core**: deterministic / fail-open / provenance-preserving / no-AI dependency
- **surface**: terminal UX + public JSON export + trace bundle + CI integration

これにより、

- 人間は terminal で素早く直せる
- AI / CI / tooling は JSON と bundle を読む
- どちらも raw provenance を失わない

### 3.3 post-cascade で価値が最大になる順番
1. **信用して入れられること**
2. **Make / CMake / older GCC / C で本当に回ること**
3. **AI / CI が deterministic に読めること**
4. **事故っても 1 trace bundle で回収できること**
5. **大規模並列ビルドでも無害なこと**

---

## 4. 優先順位マップ

| ID | Epic | 価値の主軸 | ユーザ価値 | 実装コスト | 緊急度 | 優先度 |
|---|---|---|---:|---:|---:|---:|
| BLOCKER-0 | mainline green 回復 | 信頼 | 10 | 6 | 10 | 最優先 |
| P1 | Trustworthy public entry surfaces | 信頼 / 導入 | 10 | 4 | 10 | 1 |
| P2 | Build-chain-native adoption | 導入 / 実用 | 10 | 8 | 9 | 2 |
| P6 | GCC9-12 / C-first product truth | 実用 / 現場適合 | 9 | 7 | 9 | 3 |
| P3 | Public machine-readable output | Human + AI | 9 | 7 | 7 | 4 |
| P4 | Trace bundle / replay | 運用 / 保守 | 8 | 7 | 7 | 5 |
| P5 | Performance / scale / concurrency | 本番安全性 | 9 | 8 | 6 | 6 |

---

## 5. まず起こすべき blocking issue

## BLOCKER-0 — [spike] Classify visible mainline gate failures and open recovery splits before post-cascade expansion

### Suggested Project fields
- Workstream: Quality
- Band: Cross-cutting
- Processing Path: Cross-path
- Support Level: Public
- Layer: Quality
- Issue Kind: Spike
- Task Size: M
- Risk: High
- Contract Change: None
- Agent Ready: Draft
- Human Review Type: Deep
- Stop-Ship: Yes
- Owner Layer: Shared
- Acceptance ID: BLOCKER-0

### Goal
現在 visible な `pr-gate` / `nightly-gate` / `rc-gate` の mainline failures を分類し、product bug / infra bug / instrumentation bug のどれかを明示した上で、**回復用 bug / WP へ分割し、最新 main が green か、少なくとも failure が説明可能な状態** に戻す。

### Why now
mainline が赤いまま post-cascade の feature epic を開くと、すべての evidence が汚染される。  
また、公開面から見ても「この repo は自分で自分を通せていない」と見える。

### Acceptance criteria
- [ ] latest `pr-gate` on `main` が green である、または infra-only failure として別 bug に切り分けられている
- [ ] latest `nightly-gate` on `main` が green である、または infra-only failure として別 bug に切り分けられている
- [ ] latest `rc-gate` on `main` が green である、または infra-only failure として別 bug に切り分けられている
- [ ] 各 failure に対して再現手順または retained artifact が issue から辿れる
- [ ] この blocker が閉じるまでは、新しい post-cascade feature PR を merge しない方針が共有されている

### Commands
- `cargo xtask check`
- `cargo xtask rc-gate`
- latest retained gate artifacts の確認

### Stop conditions
- 原因が GitHub-hosted runner 側や外部 image 側であり、repo 内で解決不能と判明した場合は infra bug として分離し、本 issue は「repo でできる最小 mitigation」までで止める
- 1 issue で複数系統の failure を抱え込む場合は分類だけして分割する

### Reviewer evidence
- 最新 gate artifact へのリンク
- failure classifier の表
- 再現ログまたは infra 判定の根拠

---

## 6. Epic 一覧（issue 先行で切る親 issue）

## EPIC-P1 — [epic] Make every public entry surface say the same current truth

### Suggested Project fields
- Workstream: Release
- Band: Cross-cutting
- Processing Path: Cross-path
- Support Level: Public
- Layer: Packaging
- Issue Kind: Epic
- Task Size: M
- Risk: Medium
- Contract Change: Docs
- Agent Ready: Draft
- Human Review Type: Quick
- Stop-Ship: Yes
- Owner Layer: Shared

### Objective
README / Release notes / Templates / Repo landing / support docs を通じて、`gcc-formed` の current support posture と product statement が **1 つの真実**として見えるようにする。

### Why this matters to doctrine
`gcc-formed` は trust を失うと導入されない。  
fail-open や support boundary を大事にする doctrine と最も整合するのは、「内部 truth と公開 truth が一致すること」である。

### Completion criteria
- [ ] GitHub Release body が current-authority docs と自動的に整合する
- [ ] current support wording の drift を CI で検出できる
- [ ] repo landing の description / website / topics / top copy が空でない
- [ ] 過去の GCC15-only / compatibility-only wording は historical docs にだけ残る

### Dependencies
- BLOCKER-0 の緩和が望ましい
- current-authority docs を正本とする現運用

### Generates these work package classes
- release-note generation
- wording drift tests
- landing metadata sync

### Out of scope
- 診断ロジックの改善
- non-Linux artifact 拡張
- IDE / editor 連携

### No-go conditions
- release ごとに人手 copy/paste が前提のままになること
- unversioned な真実の置き場を増やすこと

---

## EPIC-P2 — [epic] Make gcc-formed drop-in for real Make / CMake GCC projects

### Suggested Project fields
- Workstream: Tooling
- Band: Cross-cutting
- Processing Path: Cross-path
- Support Level: Public
- Layer: Packaging
- Issue Kind: Epic
- Task Size: L
- Risk: High
- Contract Change: CLI
- Agent Ready: Draft
- Human Review Type: Design
- Stop-Ship: No
- Owner Layer: Shared

### Objective
Make / CMake / CI の GCC 固定プロジェクトで、`gcc-formed` を **drop-in で入れて回る** 状態をつくる。

### Why this matters to doctrine
実運用の build path の最前面に入る wrapper である以上、導入 friction を下げること自体が製品価値である。  
人が「良さそう」ではなく「明日から使える」と判断できる必要がある。

### Completion criteria
- [ ] operator 向け quickstart が README / release docs の先頭導線にある
- [ ] Make / CMake / response file / depfile / stdout-sensitive path を含む interop lab がある
- [ ] launcher / cache / distcc 系の supported topology が versioned に定義される
- [ ] 少なくとも single backend launcher chain が shell-free にサポートされる、または明示的に unsupported と宣言される

### Dependencies
- P1 の公開面整合があると効果最大
- BLOCKER-0 の緩和が望ましい

### Generates these work package classes
- operator docs
- build-system interop lab
- topology policy
- backend launcher support

### Out of scope
- package manager ごとの native installer
- IDE plugin
- non-GCC backend

### No-go conditions
- shell string parsing
- hidden recursion
- stdout artifact を壊す導入方法

---

## EPIC-P3 — [epic] Expose a deterministic public machine-readable diagnostic surface

### Suggested Project fields
- Workstream: IR
- Band: Cross-cutting
- Processing Path: Cross-path
- Support Level: Beta
- Layer: IR
- Issue Kind: Epic
- Task Size: L
- Risk: High
- Contract Change: Schema
- Agent Ready: Draft
- Human Review Type: Design
- Stop-Ship: No
- Owner Layer: Shared

### Objective
受理済みの machine-readable baseline を踏まえ、人間 / CI / AI agent が screen scrape ではなく **stable JSON export** で `gcc-formed` の結果を読めるようにする。

### Why this matters to doctrine
AI を core に入れずに human + AI 体験を伸ばす最短ルートは、deterministic public surface を設けることである。  
これは renderer spec / IR spec の設計方針とも整合する。

### Completion criteria
- [ ] public schema が versioned で定義される
- [ ] CLI から file / safe stdout へ export できる
- [ ] backward-compat / determinism gate が CI にある
- [ ] AGENTS / README / CI docs に consumption 例がある

### Dependencies
- `ADR-0012`, `ADR-0013`
- current internal IR / analysis の現行 contract
- P1 の公開 wording 整合があると説明が楽

### Generates these work package classes
- public export contract docs
- export implementation
- golden snapshot gate
- docs / CI examples

### Out of scope
- AI による自由文 explanation
- IDE widget / hover
- public plugin API 全体
- SARIF egress を最初から同時にやること

### No-go conditions
- default stdout / stderr semantics を壊すこと
- internal IR をそのまま public contract にしてしまうこと
- accepted ADR baseline を無視して別 canonical model を発明すること

---

## EPIC-P4 — [epic] Productize trace bundle and replay for real incidents

### Suggested Project fields
- Workstream: Quality
- Band: Cross-cutting
- Processing Path: Cross-path
- Support Level: Beta
- Layer: Packaging
- Issue Kind: Epic
- Task Size: M
- Risk: Medium
- Contract Change: CLI
- Agent Ready: Draft
- Human Review Type: Deep
- Stop-Ship: No
- Owner Layer: Shared

### Objective
既存の trace bundle baseline を前提に、end user が **1 コマンドで shareable trace bundle** を作れ、maintainer が **trace bundle から replay** できるようにする。

### Why this matters to doctrine
fail-open と provenance-preserving を本当に製品化するには、事故時にその証拠を安全に持ち帰れる必要がある。  
support / incident response は “後で考える” ではなく wrapper 製品の core operating surface である。

### Completion criteria
- [ ] one-command trace bundle creation がある
- [ ] trace bundle から replay / export / triage ができる
- [ ] redaction / size / corruption regression suite がある
- [ ] support docs / runbooks / bug reporting が同じ trace bundle 語彙で揃う

### Dependencies
- current trace bundle / redaction specs
- P3 があると replay output 先として相性が良い

### Generates these work package classes
- bundle packaging
- replay-from-bundle
- privacy / redaction regression

### Out of scope
- remote telemetry upload
- SaaS support backend
- default-on collection

### No-go conditions
- secret leak の危険を説明せずに bundle を推すこと
- install root に trace / bundle を書くこと

---

## EPIC-P5 — [epic] Harden gcc-formed for performance, scale, and concurrency

### Suggested Project fields
- Workstream: Quality
- Band: Cross-cutting
- Processing Path: Cross-path
- Support Level: Public
- Layer: Capture
- Issue Kind: Epic
- Task Size: L
- Risk: High
- Contract Change: None
- Agent Ready: Draft
- Human Review Type: Deep
- Stop-Ship: No
- Owner Layer: Shared

### Objective
`gcc-formed` を大規模ビルド・並列ビルド・巨大 stderr に対して operationally safe にする。

### Why this matters to doctrine
wrapper は build path の最前面にいるため、性能・並列性・I/O の事故はそのまま全開発者の停止になる。  
「成功 path では邪魔しない」「失敗 path でも bounded」は製品価値そのものである。

### Completion criteria
- [ ] performance suite が operator-real workload を含む
- [ ] `make -j` / parallel CMake stress harness がある
- [ ] large artifact / truncation / malformed sidecar に対する hardening がある
- [ ] perf / memory / integrity が artifact として残る

### Dependencies
- current bench-smoke / fuzz-smoke / rc-gate の基盤
- P2 interop lab と一部相乗り可能

### Generates these work package classes
- benchmark expansion
- parallel stress harness
- large-artifact hardening

### Out of scope
- 新規 family coverage 追加
- UI polish だけの変更
- compiler backend 自体の性能改善

### No-go conditions
- benchmark が不安定すぎて gate に使えないまま放置されること
- bounded memory を壊すこと

---

## EPIC-P6 — [epic] Make GCC9-12 and C-first usage a real, evidenced product path

### Suggested Project fields
- Workstream: Quality
- Band: GCC9-12
- Processing Path: Cross-path
- Support Level: Experimental
- Layer: Quality
- Issue Kind: Epic
- Task Size: M
- Risk: Medium
- Contract Change: Docs
- Agent Ready: Draft
- Human Review Type: Deep
- Stop-Ship: No
- Owner Layer: Shared

### Objective
「GCC9-12 / C 案件でも使える」を、README の文言ではなく **corpus / self-check / workflow artifact / release evidence** で証明する。

### Why this matters to doctrine
ユーザ前提は「業務上の理由でどうしても GCC」「C 言語案件も多い」であり、ここに刺さらなければ製品価値の中心を外す。  
GCC15+ だけで強くても、旧帯域 / C-first に根拠がなければ“現場の製品”にならない。

### Completion criteria
- [ ] representative GCC9-12 real-compiler evidence がある
- [ ] C-first corpus / eval pack がある
- [ ] self-check / runtime disclosure が older GCC/C で具体的になる
- [ ] docs と release evidence が band/path truth を同じ語彙で語る

### Dependencies
- P1 の公開面整合
- P2 interop lab と相乗り可能

### Generates these work package classes
- GCC9-12 lane
- C-first corpus
- older-band self-check / runtime disclosure

### Out of scope
- GCC15+ と同一 fidelity を約束すること
- non-Linux artifact
- new diagnostic algorithm epic

### No-go conditions
- artifact evidence なしに “first-class” を強く言い切ること
- old-band の制約を曖昧にすること

---

## 7. Work Package 詳細（issue draft へ落とせる粒度）

### 7.0 起票前の共通正規化

- 以下の WP は **concise draft packet** であり、GitHub issue body に転記する前に current `EXECUTION-MODEL.md` へ合わせて正規化する。
- 各 WP には最低限、`Affected layers`、`Rollback / abandon rule`、必要なら `Depends On` / `Night Batch` を追記する。
- `Task Size = L`、`Human Review Type = Design`、複数の top-level contract doc / workflow を同時に触る計画は、原則 `Agent Ready = Draft` のまま扱う。
- 可変な GitHub visible facts は、issue 起票直前に再確認して本文へ反映する。

# P1 — Trustworthy public entry surfaces

## P1-WP1 — [wp] Generate GitHub Release body from current-authority support metadata

### Suggested Project fields
- Workstream: Release
- Band: Cross-cutting
- Processing Path: Cross-path
- Layer: Packaging
- Task Size: M
- Risk: Medium
- Contract Change: Docs
- Agent Ready: Draft
- Human Review Type: Quick
- Stop-Ship: Yes
- Owner Layer: Shared
- Acceptance ID: P1-WP1

### Goal
GitHub Release body を手書き / stale copy ではなく、**versioned current-authority metadata から生成**する。

### Why now
latest visible prerelease の release page に、current main の support posture と矛盾する wording が残っている。  
ここがズレている限り、導入候補者には「古い説明の repo」と見える。

### Parent epic / ADR
- EPIC-P1

### Affected band
- [ ] GCC15+
- [ ] GCC13-14
- [ ] GCC9-12
- [x] Cross-cutting

### Processing path
- [ ] DualSinkStructured
- [ ] SingleSinkStructured
- [ ] NativeTextCapture
- [ ] Passthrough
- [x] Cross-path

### Allowed files
- `xtask/`
- `.github/workflows/`
- `docs/releases/`
- `docs/support/`
- `README.md`
- `CHANGELOG.md`

### Forbidden surfaces
- `diag_render/`
- `diag_enrich/`
- `diag_adapter_gcc/`
- 新しい診断意味論の追加
- support boundary を docs なしに実質変更すること

### Acceptance criteria
- [ ] public beta / stable release body は versioned metadata から生成される
- [ ] release body は current support boundary / known limitations / release doc / signing pins を必ず含む
- [ ] release body は current support boundary と矛盾する old wording を出力しない
- [ ] release note generation は local / CI の contract test で検証される

### Commands
- `cargo xtask check`
- `cargo xtask rc-gate`
- release-note generation の dry-run と diff 確認

### Docs impact
- `docs/releases/PUBLIC-BETA-RELEASE.md`
- 必要なら `docs/releases/STABLE-RELEASE.md`
- 生成元 metadata を置く current-authority doc

### Stop conditions
- GitHub Release body の生成に unversioned external state が必要と判明した場合は止めて設計を分割する
- wording 整合と release automation を 1 issue に抱え込み始めたら分離する

### Reviewer evidence
- generated release body artifact
- stale wording が除去されたことを示す diff
- GitHub Release draft screenshot または artifact preview

---

## P1-WP2 — [wp] Add public-surface drift tests across README / support docs / templates / release text

### Suggested Project fields
- Workstream: Docs
- Band: Cross-cutting
- Processing Path: Cross-path
- Layer: Templates
- Task Size: M
- Risk: Low
- Contract Change: Docs
- Agent Ready: Draft
- Human Review Type: Quick
- Stop-Ship: Yes
- Owner Layer: Shared
- Acceptance ID: P1-WP2

### Goal
README / SUPPORT-BOUNDARY / KNOWN-LIMITATIONS / templates / support docs / generated release text の **語彙 drift を CI で止める**。

### Why now
support boundary は「同時更新しろ」と明記しているが、公開面では実際に drift が起きている。  
human discipline だけでは再発する。

### Parent epic / ADR
- EPIC-P1

### Affected band
- [ ] GCC15+
- [ ] GCC13-14
- [ ] GCC9-12
- [x] Cross-cutting

### Processing path
- [ ] DualSinkStructured
- [ ] SingleSinkStructured
- [ ] NativeTextCapture
- [ ] Passthrough
- [x] Cross-path

### Allowed files
- `ci/`
- `xtask/`
- `.github/ISSUE_TEMPLATE/`
- `.github/pull_request_template.md`
- `README.md`
- `docs/support/`
- `SUPPORT.md`
- `CONTRIBUTING.md`
- `AGENTS.md`

### Forbidden surfaces
- `diag_*` workspace の挙動変更
- release note generation 本体の実装（必要なら P1-WP1 に分ける）
- historical docs の全面 rewrite

### Acceptance criteria
- [ ] current-authority docs と template / support surfaces の drift を検出する CI test がある
- [ ] historical でのみ許される語彙が明示される
- [ ] generated release text も drift set に含まれる
- [ ] drift failure は implementer が直すべき対象を 1 回で特定できるメッセージを出す

### Commands
- `cargo xtask check`
- drift test 単体実行コマンド
- template / docs diff 確認

### Docs impact
- `docs/support/SUPPORT-BOUNDARY.md`
- `README.md`
- `SUPPORT.md`
- `.github/ISSUE_TEMPLATE/*`
- `.github/pull_request_template.md`

### Stop conditions
- drift rule が厳しすぎて current-authority docs の編集コストを不必要に上げる場合は設計を分割する
- “文字列一致” だけで意味 drift を過検知し始めたら rule class を見直す

### Reviewer evidence
- red → green になる drift test 例
- drift failure message の sample
- affected files 一覧

---

## P1-WP3 — [wp] Version the repo landing statement and sync GitHub About / website / topics

### Suggested Project fields
- Workstream: Docs
- Band: Cross-cutting
- Processing Path: Cross-path
- Layer: Templates
- Task Size: S
- Risk: Low
- Contract Change: Docs
- Agent Ready: Draft
- Human Review Type: Quick
- Stop-Ship: No
- Owner Layer: Maintainer
- Acceptance ID: P1-WP3

### Goal
repo landing に表示される **description / website / topics / top copy** の canonical text を versioned に置き、GitHub repo settings と同期する。

### Why now
About が空だと、README を開く前に離脱される。  
最初の 5 秒で「何をする repo か」が伝わらないのは導入損失である。

### Parent epic / ADR
- EPIC-P1

### Affected band
- [ ] GCC15+
- [ ] GCC13-14
- [ ] GCC9-12
- [x] Cross-cutting

### Processing path
- [ ] DualSinkStructured
- [ ] SingleSinkStructured
- [ ] NativeTextCapture
- [ ] Passthrough
- [x] Cross-path

### Allowed files
- `README.md`
- `docs/` 配下の repo landing 用文書
- GitHub repository settings（human-only）
- 必要なら badge / link 周辺の markdown

### Forbidden surfaces
- `diag_*` 全般
- 診断挙動
- support boundary の実質変更

### Acceptance criteria
- [ ] repo About が空でなくなる
- [ ] description / website / topics の canonical source が repo 内にある
- [ ] README 冒頭の positioning と GitHub About の文言が一致する
- [ ] “人手でどこを同期するか” が文書化される

### Commands
- `cargo xtask check`（docs lint 用）
- manual repo settings sync

### Docs impact
- `README.md`
- repo landing canonical doc
- maintainer runbook

### Stop conditions
- GitHub settings 自動同期に無理に踏み込み始めたら止める
- marketing copy の議論で implementation issue が止まり始めたら文面を最小限に固定する

### Reviewer evidence
- Before / After screenshot（repo top page）
- canonical landing statement の diff
- maintainer handoff note

---

# P2 — Build-chain-native adoption

## P2-WP1 — [wp] Add operator quickstart for CC/CXX, Make, and CMake

### Suggested Project fields
- Workstream: Docs
- Band: Cross-cutting
- Processing Path: Cross-path
- Layer: Packaging
- Task Size: M
- Risk: Low
- Contract Change: Docs
- Agent Ready: Draft
- Human Review Type: Quick
- Stop-Ship: No
- Owner Layer: Shared
- Acceptance ID: P2-WP1

### Goal
README / release docs の最短導線を **maintainer-first から operator-first** に反転し、`CC=gcc-formed` / `CXX=g++-formed` / Make / CMake の導入例を追加する。

### Why now
現場ユーザが最初に知りたいのは `cargo xtask check` ではなく、**どう差し込めば既存 build が動くか** である。

### Parent epic / ADR
- EPIC-P2

### Dependencies
- P2-WP2（interop lab が先行すると docs の実証性が上がる。interop lab 未構築の段階では最小限の例のみ記載する）

### Affected band
- [x] GCC15+
- [x] GCC13-14
- [x] GCC9-12
- [ ] Cross-cutting

### Processing path
- [ ] DualSinkStructured
- [ ] SingleSinkStructured
- [ ] NativeTextCapture
- [ ] Passthrough
- [x] Cross-path

### Allowed files
- `README.md`
- `docs/releases/PUBLIC-BETA-RELEASE.md`
- `SUPPORT.md`
- `CONTRIBUTING.md`
- interop example docs
- install / rollback runbooks

### Forbidden surfaces
- `diag_*` behavior changes
- unsupported topology を docs だけで “使える” と言い切ること
- platform 拡張

### Acceptance criteria
- [ ] README の上位導線に end-user quickstart が追加される
- [ ] `CC` / `CXX` / Make / CMake の最小例がある
- [ ] C 案件向け例と C++ 案件向け例が最低 1 つずつある
- [ ] rollback / raw fallback / uninstall の導線が同じ場所にある
- [ ] docs は interop lab で実証済みのトポロジーだけを推奨する

### Commands
- `cargo xtask check`
- `CC=$PWD/target/debug/gcc-formed make -C eval/interop/make-c`（P2-WP2 で interop lab が構築された後に実行可能）
- `cmake -S eval/interop/cmake-cxx -B /tmp/gf-cmake -DCMAKE_C_COMPILER=$PWD/target/debug/gcc-formed -DCMAKE_CXX_COMPILER=$PWD/target/debug/g++-formed && cmake --build /tmp/gf-cmake`（P2-WP2 で interop lab が構築された後に実行可能）

### Docs impact
- `README.md`
- `docs/releases/PUBLIC-BETA-RELEASE.md`
- support / install runbooks

### Stop conditions
- 実証されていない topology を docs に先書きし始めたら止める
- ccache / distcc / launcher 支援が必要になったら P2-WP3 / P2-WP4 に分ける

### Reviewer evidence
- quickstart section diff
- example build logs
- README top-of-funnel screenshot

---

## P2-WP2 — [wp] Build a real Make/CMake interoperability lab

### Suggested Project fields
- Workstream: Tooling
- Band: Cross-cutting
- Processing Path: Cross-path
- Layer: Quality
- Task Size: L
- Risk: Medium
- Contract Change: None
- Agent Ready: Draft
- Human Review Type: Deep
- Stop-Ship: No
- Owner Layer: Shared
- Acceptance ID: P2-WP2

### Goal
Make / CMake / depfile / response file / stdout-sensitive path を含む **実ビルド interop lab** を repo に持つ。

### Why now
導入 docs があっても、実 build path で壊れるなら価値はない。  
wrapper 製品は “診断が賢い” より前に **ビルドを壊さない** を証明しなければならない。

### Parent epic / ADR
- EPIC-P2

### Affected band
- [x] GCC15+
- [x] GCC13-14
- [x] GCC9-12
- [ ] Cross-cutting

### Processing path
- [x] DualSinkStructured
- [x] SingleSinkStructured
- [x] NativeTextCapture
- [x] Passthrough
- [ ] Cross-path

### Allowed files
- `eval/interop/`（新設可）
- `ci/`
- `xtask/`
- `diag_testkit/`
- `README.md`
- support docs

### Forbidden surfaces
- 新しい診断 family 実装
- renderer wording の改善を主目的にすること
- build system の挙動を shell trick で隠すこと

### Acceptance criteria
- [ ] interop lab に Make(C), CMake(C++), parallel build の最低ケースがある
- [ ] depfile (`-MMD -MF`) を壊さないケースがある
- [ ] response file を wrapper 側で展開しないことを検証するケースがある
- [ ] `-E` / `-print-*` 等 stdout-sensitive path を壊さないケースがある
- [ ] CI で representative subset が回る
- [ ] failure 時の trace / fallback も evidence に残る

### Commands
- `cargo xtask check`
- `cargo xtask replay --root corpus`
- interop lab の make / cmake 実行
- representative CI job

### Docs impact
- interop README
- operator quickstart docs
- support troubleshooting docs

### Stop conditions
- CMake generator ごとの差分を 1 issue に詰め込み始めたら分割する
- interop lab と launcher support を同時に抱え込み始めたら P2-WP4 に切る

### Reviewer evidence
- interop matrix 表
- CI artifact logs
- build products / depfile / compile_commands / stdout の保持確認

---

## P2-WP3 — [wp] Define supported launcher/cache topologies and disclose unsupported chains

### Suggested Project fields
- Workstream: Tooling
- Band: Cross-cutting
- Processing Path: Cross-path
- Layer: Packaging
- Task Size: M
- Risk: Medium
- Contract Change: CLI
- Agent Ready: Draft
- Human Review Type: Design
- Stop-Ship: No
- Owner Layer: Shared
- Acceptance ID: P2-WP3

### Goal
`ccache` / `distcc` / `sccache` / CMake compiler-launcher stacking を含む topology を **supported / unsupported / not-yet** に分け、runtime と self-check で正直に出す。

### Why now
real-world GCC project は launcher / cache / remote compile を使いがちである。  
ここを曖昧にすると「たまたま動いたけど壊れる」領域になる。

### Parent epic / ADR
- EPIC-P2

### Affected band
- [x] GCC15+
- [x] GCC13-14
- [x] GCC9-12
- [ ] Cross-cutting

### Processing path
- [ ] DualSinkStructured
- [ ] SingleSinkStructured
- [ ] NativeTextCapture
- [ ] Passthrough
- [x] Cross-path

### Allowed files
- `diag_cli_front/`
- `diag_backend_probe/`
- `docs/specs/gcc-adapter-ingestion-spec.md`
- `README.md`
- `SUPPORT.md`
- `KNOWN-LIMITATIONS.md`

### Forbidden surfaces
- shell parsing
- multi-launcher support の実装
- build-system interop lab の全面実装（必要なら P2-WP2 へ）
- ccache / distcc を “たぶん動く” でサポート扱いにすること

### Acceptance criteria
- [ ] supported / unsupported / not-yet の topology matrix が versioned doc にある
- [ ] unsupported topology は runtime または self-check で明示される
- [ ] docs / runbooks / self-check が同じ vocabulary を使う
- [ ] unsupported chain では silent misbehavior でなく conservative behavior になる

### Commands
- `cargo xtask check`
- `./target/debug/gcc-formed --formed-self-check`
- supported / unsupported topology の sample 実行

### Docs impact
- `docs/specs/gcc-adapter-ingestion-spec.md`
- `README.md`
- `SUPPORT.md`
- `KNOWN-LIMITATIONS.md`

### Stop conditions
- single launcher 実装まで入りたくなったら P2-WP4 に分離する
- topology matrix の議論で vendor-specific detail が増えすぎたら generic shape に戻す

### Reviewer evidence
- topology matrix
- self-check sample output
- unsupported chain の disclosure sample

---

## P2-WP4 — [wp] Add explicit single backend-launcher support for ccache/distcc-like tools

### Suggested Project fields
- Workstream: Tooling
- Band: Cross-cutting
- Processing Path: Cross-path
- Layer: Capture
- Task Size: L
- Risk: High
- Contract Change: CLI
- Agent Ready: Draft
- Human Review Type: Design
- Stop-Ship: No
- Owner Layer: Shared
- Acceptance ID: P2-WP4

### Goal
shell を使わずに、`launcher backend_gcc args...` という **single backend-launcher chain** を wrapper が明示的に扱えるようにする。

### Why now
real project では cache / distributed compile を避けられないことが多い。  
この層をきちんと支えると、本当に “入れて使える” 製品になる。

### Parent epic / ADR
- EPIC-P2

### Affected band
- [x] GCC15+
- [x] GCC13-14
- [x] GCC9-12
- [ ] Cross-cutting

### Processing path
- [ ] DualSinkStructured
- [ ] SingleSinkStructured
- [ ] NativeTextCapture
- [ ] Passthrough
- [x] Cross-path

### Allowed files
- `diag_cli_front/`
- `diag_backend_probe/`
- `diag_capture_runtime/`
- `docs/specs/gcc-adapter-ingestion-spec.md`
- `README.md`
- interop lab files

### Forbidden surfaces
- shell command string parsing
- multi-launcher chain support
- non-Linux support 拡張
- default topology の暗黙変更

### Acceptance criteria
- [ ] 1 個の backend launcher を namespaced config / env / CLI で指定できる
- [ ] child spawn は shell-free で deterministic である
- [ ] self-recursion / launcher recursion を検出して拒否できる
- [ ] interop lab で ccache-like / distcc-like sample が通る
- [ ] unsupported multi-launcher は明示的に拒否または disclose する

### Commands
- `cargo xtask check`
- supported launcher chain の sample build
- unsupported recursion case の sample build

### Docs impact
- `docs/specs/gcc-adapter-ingestion-spec.md`
- operator quickstart docs
- support runbooks

### Stop conditions
- multi-launcher chain や package-manager integration が必要になった時点で別 epic に切る
- stdout/stderr semantics に影響し始めたら issue を分ける

### Reviewer evidence
- launcher chain spawn log / trace excerpt
- recursion rejection sample
- interop lab CI evidence

---

# P3 — Public machine-readable output

## P3-WP1 — [wp] Define the public JSON export contract from the accepted machine-readable baseline

### Suggested Project fields
- Workstream: IR
- Band: Cross-cutting
- Processing Path: Cross-path
- Layer: IR
- Task Size: M
- Risk: Medium
- Contract Change: Schema
- Agent Ready: Draft
- Human Review Type: Design
- Stop-Ship: No
- Owner Layer: Shared
- Acceptance ID: P3-WP1

### Goal
`ADR-0012` / `ADR-0013` を踏まえ、accepted machine-readable baseline から **public contract として安定した JSON export contract** を定義する。

### Why now
human + AI 体験を両立するには、screen scrape でなく public export が必要である。  
しかも internal IR をそのまま晒すのではなく、public contract として versioned に切る必要がある。

### Parent epic / ADR
- EPIC-P3
- `ADR-0012`
- `ADR-0013`

### Affected band
- [x] GCC15+
- [x] GCC13-14
- [x] GCC9-12
- [x] Cross-cutting

### Processing path
- [x] DualSinkStructured
- [x] SingleSinkStructured
- [x] NativeTextCapture
- [x] Passthrough
- [x] Cross-path

### Allowed files
- `docs/specs/diagnostic-ir-v1alpha-spec.md`
- `docs/specs/rendering-ux-contract-spec.md`
- 新しい public export contract doc
- `README.md`

### Forbidden surfaces
- CLI 実装
- renderer UI 変更
- AI freeform explanation
- SARIF egress を同時に入れること
- accepted ADR baseline を破って別 canonical model を立てること

### Acceptance criteria
- [ ] public JSON export contract は versioned である
- [ ] backward / forward compatibility policy が定義されている
- [ ] canonical JSON ordering / normalization が定義されている
- [ ] version_band / processing_path / support_level / fallback reason / provenance refs / user-facing action fields が public schema に含まれる
- [ ] internal-only field と public field の境界が明示されている
- [ ] out-of-scope（editor UI / AI prose / plugin API / SARIF egress）が明記されている
- [ ] `ADR-0012` / `ADR-0013` と矛盾しないことが cross-reference で示されている

### Commands
- `cargo xtask check`
- schema example fixture review

### Docs impact
- `docs/specs/diagnostic-ir-v1alpha-spec.md`
- `docs/specs/rendering-ux-contract-spec.md`
- 新 public export contract doc

### Stop conditions
- internal IR の全面刷新が必要になったら止めて別設計にする
- 1 issue で SARIF / editor transport / plugin API まで抱え込み始めたら分割する
- accepted baseline を変える新判断が必要になったら、実装 issue ではなく別 ADR issue に切り出す

### Reviewer evidence
- export contract doc
- example JSON
- compatibility policy diff

---

## P3-WP2 — [wp] Implement file/stdout JSON export without breaking compiler stdout semantics

### Suggested Project fields
- Workstream: IR
- Band: Cross-cutting
- Processing Path: Cross-path
- Layer: IR
- Task Size: L
- Risk: High
- Contract Change: CLI
- Agent Ready: Draft
- Human Review Type: Deep
- Stop-Ship: No
- Owner Layer: Shared
- Acceptance ID: P3-WP2

### Goal
CLI から public JSON を **file sink** と **safe stdout sink** へ出せるようにする。

### Why now
schema だけでは製品価値にならない。  
だが adapter spec 上、stdout は build artifact になりうるため、**壊さない設計で出す** 必要がある。

### Parent epic / ADR
- EPIC-P3
- P3-WP1

### Affected band
- [x] GCC15+
- [x] GCC13-14
- [x] GCC9-12
- [x] Cross-cutting

### Processing path
- [x] DualSinkStructured
- [x] SingleSinkStructured
- [x] NativeTextCapture
- [x] Passthrough
- [x] Cross-path

### Allowed files
- `diag_cli_front/`
- `diag_trace/`
- `diag_core/`（mapping helper が必要な場合のみ）
- `diag_enrich/`（public mapping helper が必要な場合のみ）
- new export tests / fixtures

### Forbidden surfaces
- default TTY render の変更
- compiler stdout を wrapper 都合で奪うこと
- screen-scrape 前提の hack
- editor plugin surface の追加

### Acceptance criteria
- [ ] public JSON export を file に出せる
- [ ] stdout export は safe な場合のみ有効、unsafe な場合は明示的に拒否または file sink を要求する
- [ ] default invocation の stderr/stdout semantics は変わらない
- [ ] fallback / passthrough でも truthful export または explicit no-export reason を返せる
- [ ] export は provenance と disclosure を落とさない

### Commands
- `cargo xtask check`
- JSON export fixture replay
- stdout-sensitive sample (`-E` など) の safety check

### Docs impact
- CLI docs
- support docs
- public output schema doc

### Stop conditions
- stdout export の安全条件が複雑化しすぎたら file-only を先に出して issue を分割する
- internal IR mapping が大規模 refactor になり始めたら別 issue 化する

### Reviewer evidence
- file export sample
- safe stdout / rejected stdout sample
- fallback export sample

---

## P3-WP3 — [wp] Add golden snapshots and compatibility gates for public JSON export

### Suggested Project fields
- Workstream: Quality
- Band: Cross-cutting
- Processing Path: Cross-path
- Layer: Quality
- Task Size: M
- Risk: Medium
- Contract Change: Schema
- Agent Ready: Draft
- Human Review Type: Deep
- Stop-Ship: No
- Owner Layer: Shared
- Acceptance ID: P3-WP3

### Goal
public JSON export を **snapshot / determinism / schema-compatibility gate** に載せる。

### Why now
machine-readable output は一度出すと downstream が依存する。  
出して終わりではなく、**壊したら検出できる** ことが必要である。

### Parent epic / ADR
- EPIC-P3
- P3-WP1
- P3-WP2

### Affected band
- [x] GCC15+
- [x] GCC13-14
- [x] GCC9-12
- [x] Cross-cutting

### Processing path
- [x] DualSinkStructured
- [x] SingleSinkStructured
- [x] NativeTextCapture
- [x] Passthrough
- [x] Cross-path

### Allowed files
- `diag_testkit/`
- `xtask/`
- `ci/`
- `corpus/`
- export schema doc

### Forbidden surfaces
- new semantic analysis feature
- renderer optimization を主目的にすること
- public schema の無断 breaking change

### Acceptance criteria
- [ ] export snapshot fixture がある
- [ ] canonical JSON / ordering が test される
- [ ] schema breaking change は version bump なしでは CI failure になる
- [ ] representative older-band fixture でも export snapshot がある
- [ ] replay artifact に export evidence が残る

### Commands
- `cargo xtask check`
- `cargo xtask replay --root corpus`
- export snapshot update / verify command

### Docs impact
- schema doc
- quality gate docs
- replay / testkit docs

### Stop conditions
- snapshot volume が大きくなりすぎたら representative subset と full subset を分ける
- golden が flaky なら deterministic normalization を先に直す

### Reviewer evidence
- export snapshot diff
- compatibility gate failure example
- representative band/path evidence

---

## P3-WP4 — [wp] Add CI and agent integration examples that consume JSON export instead of screen scraping

### Suggested Project fields
- Workstream: Docs
- Band: Cross-cutting
- Processing Path: Cross-path
- Layer: Templates
- Task Size: S
- Risk: Low
- Contract Change: Docs
- Agent Ready: Draft
- Human Review Type: Quick
- Stop-Ship: No
- Owner Layer: Shared
- Acceptance ID: P3-WP4

### Goal
README / AGENTS / support docs に、public JSON export を **CI / human / agent** がどう使うかの最小例を追加する。

### Why now
新しい public surface は、使われなければ価値にならない。  
また AI を使う側に「screen scrape ではなく export を使え」と導線を引く必要がある。

### Parent epic / ADR
- EPIC-P3

### Affected band
- [x] GCC15+
- [x] GCC13-14
- [x] GCC9-12
- [x] Cross-cutting

### Processing path
- [x] DualSinkStructured
- [x] SingleSinkStructured
- [x] NativeTextCapture
- [x] Passthrough
- [x] Cross-path

### Allowed files
- `README.md`
- `AGENTS.md`
- `SUPPORT.md`
- CI example docs
- support runbooks

### Forbidden surfaces
- AI prompt engineering を本質にすること
- hosted service / cloud upload
- editor integration 実装

### Acceptance criteria
- [ ] human CLI example がある
- [ ] GitHub Actions / CI artifact example がある
- [ ] agent / automation example がある
- [ ] docs は screen scraping を推奨しない
- [ ] fallback / partial analysis の扱い方が説明される

### Commands
- `cargo xtask check`
- example scripts の smoke

### Docs impact
- `README.md`
- `AGENTS.md`
- `SUPPORT.md`
- CI docs

### Stop conditions
- docs issue が new command design に引っ張られ始めたら P3-WP2 側へ戻す
- AI specific examples が vendor lock-in し始めたら generic JSON consumption に戻す

### Reviewer evidence
- docs diff
- CI example artifact
- agent consumption sample

---

# P4 — Trace bundle / replay

## P4-WP1 — [wp] Add one-command opt-in trace bundle creation to the user-facing wrapper

### Suggested Project fields
- Workstream: Quality
- Band: Cross-cutting
- Processing Path: Cross-path
- Layer: Packaging
- Task Size: M
- Risk: Medium
- Contract Change: CLI
- Agent Ready: Draft
- Human Review Type: Deep
- Stop-Ship: No
- Owner Layer: Shared
- Acceptance ID: P4-WP1

### Goal
end user が wrapper invocation から **1 コマンドで shareable trace bundle** を生成できるようにする。

### Why now
support docs や trace bundle 方針はあるが、現場では「どのファイルを集めればよいか」で躓く。  
運用 surface は CLI まで落ちてはじめて製品になる。

### Parent epic / ADR
- EPIC-P4

### Affected band
- [x] GCC15+
- [x] GCC13-14
- [x] GCC9-12
- [x] Cross-cutting

### Processing path
- [x] DualSinkStructured
- [x] SingleSinkStructured
- [x] NativeTextCapture
- [x] Passthrough
- [x] Cross-path

### Allowed files
- `diag_cli_front/`
- `diag_trace/`
- `docs/support/`
- `SUPPORT.md`
- packaging / runbook docs

### Forbidden surfaces
- default-on telemetry
- remote upload
- install root への write
- secret-safe でない追加 capture

### Acceptance criteria
- [ ] trace bundle creation は opt-in である
- [ ] trace bundle output path は user-specified または state root 配下である
- [ ] trace bundle に manifest / version / band / path / retained artifacts / redaction status が含まれる
- [ ] fallback / passthrough case でも trace bundle は useful である
- [ ] user-facing stderr に trace bundle path を 1 行で示せる

### Commands
- `cargo xtask check`
- failing sample invocation with trace-bundle option
- generated trace bundle inspection

### Docs impact
- `SUPPORT.md`
- trace bundle runbooks
- packaging / runtime docs
- bug report guidance

### Stop conditions
- replay tooling まで一緒に抱え込み始めたら P4-WP2 に分ける
- redaction policy の再設計が必要なら P4-WP3 に分ける

### Reviewer evidence
- sample trace bundle tree
- user-facing trace bundle path disclosure sample
- manifest example

---

## P4-WP2 — [wp] Implement maintainer replay-from-trace-bundle flow with explicit degradation disclosure

### Suggested Project fields
- Workstream: Quality
- Band: Cross-cutting
- Processing Path: Cross-path
- Layer: Quality
- Task Size: M
- Risk: Medium
- Contract Change: CLI
- Agent Ready: Draft
- Human Review Type: Deep
- Stop-Ship: No
- Owner Layer: Shared
- Acceptance ID: P4-WP2

### Goal
maintainer が trace bundle から **terminal render / JSON export / provenance summary** を再現できるようにする。

### Why now
trace bundle を受け取っても replay できなければ support loop が速くならない。  
また source excerpt が不足する場合の degradation も明示が必要である。

### Parent epic / ADR
- EPIC-P4
- P4-WP1

### Affected band
- [x] GCC15+
- [x] GCC13-14
- [x] GCC9-12
- [x] Cross-cutting

### Processing path
- [x] DualSinkStructured
- [x] SingleSinkStructured
- [x] NativeTextCapture
- [x] Passthrough
- [x] Cross-path

### Allowed files
- `xtask/`
- `diag_trace/`
- `diag_testkit/`
- support runbooks
- JSON export integration docs

### Forbidden surfaces
- original source tree への依存
- hidden network access
- excerpt 欠落を silent に飲み込むこと

### Acceptance criteria
- [ ] trace bundle から replay する maintainer command がある
- [ ] terminal render を再生できる
- [ ] public JSON export がある場合はそれも再生できる
- [ ] source excerpt や path 情報が不足する場合は degrade を明示する
- [ ] replay output は stored bundle contents に対して deterministic である

### Commands
- `cargo xtask check`
- replay-from-trace-bundle command
- trace bundle fixture replay

### Docs impact
- support triage runbook
- replay docs
- bug triage docs

### Stop conditions
- source reconstruction や外部参照が必要になり始めたら設計を縮小する
- replay と corpus promotion を混同し始めたら issue を分ける

### Reviewer evidence
- replay command sample
- degradation disclosure sample
- deterministic replay evidence

---

## P4-WP3 — [wp] Add redaction, leakage, size-cap, and corruption regression tests for trace bundles

### Suggested Project fields
- Workstream: Quality
- Band: Cross-cutting
- Processing Path: Cross-path
- Layer: Quality
- Task Size: M
- Risk: Medium
- Contract Change: None
- Agent Ready: Ready
- Human Review Type: Deep
- Stop-Ship: No
- Owner Layer: Shared
- Acceptance ID: P4-WP3

### Goal
trace bundle を **shareable だが危険を過小評価しない** artifact として regression test する。

### Why now
trace bundle が便利でも、secret leak や巨大 bundle で運用不能になれば逆効果である。

### Parent epic / ADR
- EPIC-P4

### Affected band
- [x] GCC15+
- [x] GCC13-14
- [x] GCC9-12
- [x] Cross-cutting

### Processing path
- [x] DualSinkStructured
- [x] SingleSinkStructured
- [x] NativeTextCapture
- [x] Passthrough
- [x] Cross-path

### Allowed files
- `diag_trace/`
- `diag_testkit/`
- `ci/`
- `fuzz/`
- support docs / runbooks

### Forbidden surfaces
- default bundle content の無制限拡張
- remote upload
- redaction policy の無断変更

### Acceptance criteria
- [ ] secret-like env / path / snippet fixture に対する redaction test がある
- [ ] size cap / retention rule に対する regression test がある
- [ ] corrupted or partial trace-bundle members でも panic しない
- [ ] support docs に redaction class と “アップロードしてはいけないケース” が書かれる

### Commands
- `cargo xtask check`
- `cargo xtask fuzz-smoke`
- trace-bundle regression suite

### Docs impact
- support docs
- trace bundle / redaction docs
- bug reporting guidance

### Stop conditions
- privacy policy の全面見直しになったら docs 設計 issue に切り出す
- regression suite が too synthetic なら harvested trace ベースを別 issue で追加する

### Reviewer evidence
- redaction test report
- corruption test sample
- size-cap evidence

---

# P5 — Performance / scale / concurrency

## P5-WP1 — [wp] Expand benchmark smoke to operator-real workloads and band/path breakdowns

### Suggested Project fields
- Workstream: Quality
- Band: Cross-cutting
- Processing Path: Cross-path
- Layer: Quality
- Task Size: M
- Risk: Medium
- Contract Change: None
- Agent Ready: Ready
- Human Review Type: Deep
- Stop-Ship: No
- Owner Layer: Shared
- Acceptance ID: P5-WP1

### Goal
existing bench smoke を、**C compile success / linker failure / template-heavy / raw fallback / older-band** を含む operator-real suite に拡張する。

### Why now
今ある benchmark の存在だけでは、本番導入時の不安は消えない。  
特に C-first / linker-heavy / raw fallback が重要である。

### Parent epic / ADR
- EPIC-P5

### Affected band
- [x] GCC15+
- [x] GCC13-14
- [x] GCC9-12
- [x] Cross-cutting

### Processing path
- [x] DualSinkStructured
- [x] SingleSinkStructured
- [x] NativeTextCapture
- [x] Passthrough
- [x] Cross-path

### Allowed files
- `xtask/`
- `eval/rc/`
- `diag_testkit/`
- `ci/`
- `corpus/`

### Forbidden surfaces
- family coverage の追加を主目的にすること
- renderer 文言の調整だけで benchmark issue を消費すること

### Acceptance criteria
- [ ] success / simple failure / template-heavy / linker-heavy / raw fallback のケースがある
- [ ] benchmark report は version_band / processing_path 別に分解できる
- [ ] spec の budget と baseline 比の両方で regression を見られる
- [ ] rc-gate artifact に summary が残る

### Commands
- `cargo xtask bench-smoke`
- `cargo xtask rc-gate`
- benchmark suite run

### Docs impact
- performance budget docs
- rc-gate docs
- benchmark runbook

### Stop conditions
- benchmark noise が高すぎて threshold を決められない場合は metrics-only issue に分割する
- workload expansion が corpus governance issue に化けたら分ける

### Reviewer evidence
- benchmark report diff
- per-band / per-path summary
- threshold pass/fail example

---

## P5-WP2 — [wp] Add parallel-build stress harness for make -j / CMake -j with temp and trace contention checks

### Suggested Project fields
- Workstream: Quality
- Band: Cross-cutting
- Processing Path: Cross-path
- Layer: Capture
- Task Size: L
- Risk: High
- Contract Change: None
- Agent Ready: Draft
- Human Review Type: Deep
- Stop-Ship: No
- Owner Layer: Shared
- Acceptance ID: P5-WP2

### Goal
並列ビルド下で temp path / trace path / stderr spool / cleanup が壊れないことを stress harness で検証する。

### Why now
`make -j` / `cmake --build -j` を回せない wrapper は、現場では採用されない。  
単発 replay が通るだけでは不十分である。

### Parent epic / ADR
- EPIC-P5

### Affected band
- [x] GCC15+
- [x] GCC13-14
- [x] GCC9-12
- [x] Cross-cutting

### Processing path
- [x] DualSinkStructured
- [x] SingleSinkStructured
- [x] NativeTextCapture
- [x] Passthrough
- [x] Cross-path

### Allowed files
- `diag_capture_runtime/`
- `diag_trace/`
- `xtask/`
- `ci/`
- `eval/interop/`

### Forbidden surfaces
- diagnostic semantics の変更
- shell-based orchestration への逃避
- install root への temp write

### Acceptance criteria
- [ ] parallel stress harness がある
- [ ] no trace overwrite / no temp collision が確認できる
- [ ] deadlock / blocked pipe / stdout corruption がない
- [ ] representative parallel case が CI または periodic gate に載る
- [ ] cleanup / prune policy が文書化される

### Commands
- `cargo xtask check`
- parallel stress harness command
- representative make/cmake parallel build

### Docs impact
- runtime / trace docs
- support troubleshooting docs
- interop docs

### Stop conditions
- harness 自体が flaky なら deterministic subset と soak subset に分割する
- generator / platform 差分が大きすぎる場合は reference subset を先に固定する

### Reviewer evidence
- stress report
- temp / trace directory evidence
- no-deadlock logs

---

## P5-WP3 — [wp] Harden large stderr / SARIF / JSON artifact streaming and truncation handling

### Suggested Project fields
- Workstream: Capture
- Band: Cross-cutting
- Processing Path: Cross-path
- Layer: Capture
- Task Size: M
- Risk: High
- Contract Change: None
- Agent Ready: Ready
- Human Review Type: Deep
- Stop-Ship: No
- Owner Layer: Shared
- Acceptance ID: P5-WP3

### Goal
巨大 stderr / structured sidecar / malformed or truncated artifact に対する capture / ingest / trace を bounded にする。

### Why now
本番の壊れ方は “きれいな 30 行の error” ではない。  
巨大 linker stderr、壊れた sidecar、途中で切れた artifact に対して安全であることが必要である。

### Parent epic / ADR
- EPIC-P5

### Affected band
- [x] GCC15+
- [x] GCC13-14
- [x] GCC9-12
- [x] Cross-cutting

### Processing path
- [x] DualSinkStructured
- [x] SingleSinkStructured
- [x] NativeTextCapture
- [x] Passthrough
- [x] Cross-path

### Allowed files
- `diag_capture_runtime/`
- `diag_adapter_gcc/`
- `diag_trace/`
- `fuzz/`
- `ci/`

### Forbidden surfaces
- new family semantics
- renderer 文言だけの調整
- large artifact を “諦めて黙って捨てる” こと

### Acceptance criteria
- [ ] giant stderr / sidecar input で bounded memory と streaming policy が守られる
- [ ] truncated / malformed artifact で integrity issue と honest fallback が出る
- [ ] panic / silent drop がない
- [ ] fuzz / stress / benchmark の evidence が残る

### Commands
- `cargo xtask check`
- `cargo xtask fuzz-smoke`
- large-artifact stress suite

### Docs impact
- capture / ingest spec
- performance docs
- support docs

### Stop conditions
- adapter / runtime / trace の 3 層同時大改修になり始めたら層ごとに分割する
- benchmark で only-performance tuning に化けたら P5-WP1 に戻す

### Reviewer evidence
- stress artifact report
- fuzz evidence
- integrity issue sample

---

# P6 — GCC9-12 / C-first product truth

## P6-WP1 — [wp] Add representative GCC9-12 real-compiler evidence to nightly or periodic gates

### Suggested Project fields
- Workstream: Quality
- Band: GCC9-12
- Processing Path: Cross-path
- Layer: Quality
- Task Size: M
- Risk: Medium
- Contract Change: None
- Agent Ready: Draft
- Human Review Type: Deep
- Stop-Ship: No
- Owner Layer: Shared
- Acceptance ID: P6-WP1

### Goal
Band C (`GCC9-12`) に対して、**real compiler evidence** を nightly / periodic / rc-gate のいずれかに追加する。

### Why now
current support posture は GCC9-12 を in-scope としている。  
ならば “fixture だけではない” 実 compiler evidence が必要である。

### Parent epic / ADR
- EPIC-P6

### Affected band
- [ ] GCC15+
- [ ] GCC13-14
- [x] GCC9-12
- [ ] Cross-cutting

### Processing path
- [ ] DualSinkStructured
- [x] SingleSinkStructured
- [x] NativeTextCapture
- [ ] Passthrough
- [ ] Cross-path

### Allowed files
- `.github/workflows/`
- `ci/`
- `xtask/`
- `docs/support/`
- rc-gate docs / reports

### Forbidden surfaces
- renderer 挙動の変更
- support wording の無断拡張
- “image が重いから証拠なしで主張だけ残す” こと

### Acceptance criteria
- [ ] GCC9-12 representative lane が nightly / periodic / rc-gate のどこかに追加される
- [ ] artifact は `NativeTextCapture` と `SingleSinkStructured` を区別する
- [ ] lane を常時回せない場合は periodic/manual evidence contract が versioned で定義される
- [ ] release / rc summary で Band C evidence の有無が見える

### Commands
- `cargo xtask rc-gate`
- target workflow run
- lane artifact review

### Docs impact
- support boundary evidence docs
- workflow docs
- release / rc docs

### Stop conditions
- CI image / distro availability が blocker なら manual periodic evidence path に縮退する
- band evidence と corpus expansion を 1 issue に詰め込み始めたら分離する

### Reviewer evidence
- lane run artifact
- band/path matrix excerpt
- summary report diff

---

## P6-WP2 — [wp] Build C-first representative corpus and human-eval packs for older GCC bands

### Suggested Project fields
- Workstream: Quality
- Band: GCC9-12
- Processing Path: Cross-path
- Layer: Quality
- Task Size: M
- Risk: Medium
- Contract Change: None
- Agent Ready: Ready
- Human Review Type: Deep
- Stop-Ship: No
- Owner Layer: Shared
- Acceptance ID: P6-WP2

### Goal
C compile / linker / include-path / macro / preprocessor / fallback-honest case を中心に、**older-band で意味のある corpus と eval pack** を整備する。

### Why now
C-first / older GCC の価値は、C++ template fixture だけでは測れない。  
現場が欲しいのは、**C と linker で役立つか** である。

### Parent epic / ADR
- EPIC-P6

### Affected band
- [ ] GCC15+
- [x] GCC13-14
- [x] GCC9-12
- [ ] Cross-cutting

### Processing path
- [ ] DualSinkStructured
- [x] SingleSinkStructured
- [x] NativeTextCapture
- [x] Passthrough
- [ ] Cross-path

### Allowed files
- `corpus/`
- `diag_testkit/`
- `eval/rc/`
- corpus docs
- fixture metadata
- representative replay config

### Forbidden surfaces
- cascade / rule semantics の実装
- renderer 変更を主目的にすること
- harvested trace を無審査で curated に昇格させること

### Acceptance criteria
- [ ] C-first representative fixture set が追加される
- [ ] compile / link / include / macro / preprocessor / honest fallback を含む
- [ ] fixture は version_band / processing_path / expected support behavior でタグ付けされる
- [ ] human-eval pack に C-first task が追加される
- [ ] representative replay / rc-gate evidence に組み込まれる

### Commands
- `cargo xtask replay --root corpus`
- `cargo xtask human-eval-kit`
- `cargo xtask rc-gate`

### Docs impact
- `corpus/README.md`
- quality gate docs
- human eval docs

### Stop conditions
- fixture 追加が family expansion project に化けたら止める
- harvested trace promotion review が重い場合は separate promotion issue に切る

### Reviewer evidence
- fixture inventory
- tags / metadata sample
- human-eval packet diff

---

## P6-WP3 — [wp] Make self-check and runtime disclosure concrete for older GCC and C-first operators

### Suggested Project fields
- Workstream: Docs
- Band: GCC9-12
- Processing Path: Cross-path
- Layer: Packaging
- Task Size: M
- Risk: Low
- Contract Change: CLI
- Agent Ready: Ready
- Human Review Type: Quick
- Stop-Ship: No
- Owner Layer: Shared
- Acceptance ID: P6-WP3

### Goal
`--formed-self-check` と runtime disclosure を、older GCC / C-first operator にとって **具体的で行動可能** なものにする。

### Why now
band/path truth は docs に書くだけでは足りない。  
現場ユーザは CLI の一発目で「この環境で何ができるか」を知りたい。

### Parent epic / ADR
- EPIC-P6

### Affected band
- [ ] GCC15+
- [x] GCC13-14
- [x] GCC9-12
- [ ] Cross-cutting

### Processing path
- [ ] DualSinkStructured
- [x] SingleSinkStructured
- [x] NativeTextCapture
- [x] Passthrough
- [ ] Cross-path

### Allowed files
- `diag_cli_front/`
- `diag_backend_probe/`
- `README.md`
- `docs/support/`
- `KNOWN-LIMITATIONS.md`
- `SUPPORT.md`

### Forbidden surfaces
- support boundary 自体の拡張
- old-band を過大に約束する wording
- legacy vocabulary の再増殖

### Acceptance criteria
- [ ] self-check が VersionBand / ProcessingPath / SupportLevel を current vocabulary で示す
- [ ] older GCC/C-first での representative limitations と actionable next step を示す
- [ ] runtime notices / self-check / docs が同じ wording を使う
- [ ] legacy tier wording は current-authority surface から増えない

### Commands
- `./target/debug/gcc-formed --formed-self-check`
- representative older-band sample invocation
- `cargo xtask check`

### Docs impact
- `README.md`
- `docs/support/KNOWN-LIMITATIONS.md`
- `SUPPORT.md`
- self-check docs

### Stop conditions
- self-check の情報量が多すぎて operator を混乱させる場合は concise / verbose mode を分ける
- runtime disclosure と release-note wording を 1 issue に詰め込み始めたら分離する

### Reviewer evidence
- self-check output before/after
- older-band sample disclosure
- docs wording diff

---

## 8. Wave 計画（issue の起票順と着手順）

### Wave 0 — Stop-ship recovery
1. `BLOCKER-0`
2. `EPIC-P1`
3. `P1-WP1`
4. `P1-WP2`

> この wave では、**まず信頼面を直す**。  
> 公開面の真実と Actions の visible surface が揃わない限り、以後の価値訴求が弱い。

### Wave 1 — “本当に入れられる” を作る
1. `EPIC-P2`
2. `P2-WP1`
3. `P2-WP2`
4. `P2-WP3`
5. `EPIC-P6`
6. `P6-WP1`
7. `P6-WP2`
8. `P6-WP3`

> この wave では、**Make / CMake / GCC9-12 / C-first** を中心に、現場導入性を上げる。  
> 実はここが post-cascade で最も商売になる。

### Wave 2 — Human + AI surface を固定する
1. `EPIC-P3`
2. `P3-WP1`
3. `P3-WP2`
4. `P3-WP3`
5. `P3-WP4`

> ここで初めて、AI にとっても“最高の体験”に踏み込む。  
> ただし hot path に AI は入れない。**public JSON** を出す。

### Wave 3 — support / scale を製品化する
1. `EPIC-P4`
2. `P4-WP1`
3. `P4-WP2`
4. `P4-WP3`
5. `EPIC-P5`
6. `P5-WP1`
7. `P5-WP2`
8. `P5-WP3`

> ここで「大規模 CI でも怖くない」「事故っても bundle で持ち帰れる」を固める。

---

## 9. GitHub issue 化の運用ルール

### 9.1 先に親 Epic を作る
まずは以下を parent issue として作る。

- `EPIC-P1`
- `EPIC-P2`
- `EPIC-P3`
- `EPIC-P4`
- `EPIC-P5`
- `EPIC-P6`

その後、各 WP の `Parent epic / ADR` を実 issue 番号に置換して child issue を作る。

### 9.2 一気に全部 open しない
report としては全 WP を定義したが、**実際に open する WP は wave ごと**に絞った方がよい。

推奨:

- すぐ open: `BLOCKER-0`, `P1-WP1`, `P1-WP2`, `P1-WP3`, `P2-WP1`, `P2-WP2`, `P6-WP1`
- draft / later: その他

### 9.3 label と Project fields の推奨
Execution Model に合わせ、最低でも以下を付ける。

- `kind:epic` / `kind:work-package`
- `band:gcc15` / `band:gcc13-14` / `band:gcc9-12` / `band:cross`
- `agent-ready`
- `human-only`（P1-WP3 のような手作業含み issue）

Project field は少なくとも以下を埋める。

- `Workstream`
- `Band`
- `Processing Path`
- `Layer`
- `Task Size`
- `Risk`
- `Contract Change`
- `Human Review Type`
- `Acceptance ID`

### 9.4 1000 行前後の bounded scope を守る判断基準
次の兆候が出たら issue を割る。

- 2 つ以上の crate 境界をまたいで意味論変更が必要
- docs / CLI / schema / workflow を 1 issue で同時に全部変えたくなっている
- “ついでにこれも” が 3 個以上出る
- acceptance criteria が 6 個を超えて増殖する
- reviewer evidence が 1 画面で説明しきれなくなる

---

## 10. いまはやらない backlog（考えたが後回しにするもの）

価値はあるが、post-cascade 初手では優先しない。

1. **public website / docs portal の大規模整備**  
   P1-WP3 で最小 landing を整えた後でよい

2. **SARIF egress / editor transport / plugin API**  
   まずは 1 本の public JSON を versioned に出す

3. **Clang / MSVC / multi-compiler brand 展開 (`cc-formed`)**  
   packaging spec の将来像としては正しいが、現時点では GCC 現場への深掘りが先

4. **SBOM / SLSA / advanced supply-chain hardening**  
   重要だが、採用導線・build-chain 導入性・machine-readable output よりは後

5. **non-Linux artifacts**
   current support boundary を自ら壊さない方がよい

---

## 11. 最終提言

post-cascade で `gcc-formed` の価値を最大化するための中核は、**診断アルゴリズムの追加競争**ではない。  
次の 3 つである。

### 11.1 まず “信頼できる” にする
- public truth を 1 つにする
- red mainline を止める
- repo landing で何者かを即座に伝える

### 11.2 次に “現場に入る” にする
- Make / CMake / older GCC / C-first を真正面から扱う
- launcher / cache / distributed compile を曖昧にしない
- docs でなく interop lab で証明する

### 11.3 その上で “人間と AI の両方が使える” にする
- terminal UX は引き続き磨く
- しかし AI には JSON を渡す
- support は bundle で持ち帰る
- 本番では bounded performance を守る

この順番なら、`gcc-formed` は

- **使わざるを得ない GCC**
- **選べない C / C++ バージョン**
- **変えられない Make / CMake**
- **それでも最高の人間 + AI 体験**

という最も難しい現場条件で、逆に強い製品になれる。

---

## 12. 参照した主な公開面（snapshot 用）

> 以下は 2026-04-12 JST 時点でレビューした主要 surface。  
> issue 起票時には最新版を再確認すること。

- Repository main page  
  <https://github.com/horiyamayoh/gcc-formed>

- README  
  <https://github.com/horiyamayoh/gcc-formed/blob/main/README.md>

- Support Boundary  
  <https://github.com/horiyamayoh/gcc-formed/blob/main/docs/support/SUPPORT-BOUNDARY.md>

- Known Limitations  
  <https://github.com/horiyamayoh/gcc-formed/blob/main/docs/support/KNOWN-LIMITATIONS.md>

- Public Beta Release doc  
  <https://github.com/horiyamayoh/gcc-formed/blob/main/docs/releases/PUBLIC-BETA-RELEASE.md>

- Packaging / Runtime / Operations spec  
  <https://github.com/horiyamayoh/gcc-formed/blob/main/docs/specs/packaging-runtime-operations-spec.md>

- Diagnostic IR spec  
  <https://github.com/horiyamayoh/gcc-formed/blob/main/docs/specs/diagnostic-ir-v1alpha-spec.md>

- GCC adapter ingestion spec  
  <https://github.com/horiyamayoh/gcc-formed/blob/main/docs/specs/gcc-adapter-ingestion-spec.md>

- Rendering UX contract spec  
  <https://github.com/horiyamayoh/gcc-formed/blob/main/docs/specs/rendering-ux-contract-spec.md>

- Quality / Corpus / Test / Gate spec  
  <https://github.com/horiyamayoh/gcc-formed/blob/main/docs/specs/quality-corpus-test-gate-spec.md>

- Execution Model  
  <https://github.com/horiyamayoh/gcc-formed/blob/main/docs/process/EXECUTION-MODEL.md>

- Open issues  
  <https://github.com/horiyamayoh/gcc-formed/issues>

- Actions  
  <https://github.com/horiyamayoh/gcc-formed/actions>

- Latest visible release  
  <https://github.com/horiyamayoh/gcc-formed/releases>

- Issue templates  
  <https://github.com/horiyamayoh/gcc-formed/tree/main/.github/ISSUE_TEMPLATE>

- PR template  
  <https://github.com/horiyamayoh/gcc-formed/blob/main/.github/pull_request_template.md>

---

## 13. 付録: 起票コピペ用の最短一覧

### Parent Epics
- `[epic] Make every public entry surface say the same current truth`
- `[epic] Make gcc-formed drop-in for real Make / CMake GCC projects`
- `[epic] Expose a deterministic public machine-readable diagnostic surface`
- `[epic] Productize trace bundle and replay for real incidents`
- `[epic] Harden gcc-formed for performance, scale, and concurrency`
- `[epic] Make GCC9-12 and C-first usage a real, evidenced product path`

### Immediate child issues
- `[wp] Generate GitHub Release body from current-authority support metadata`
- `[wp] Add public-surface drift tests across README / support docs / templates / release text`
- `[wp] Add operator quickstart for CC/CXX, Make, and CMake`
- `[wp] Build a real Make/CMake interoperability lab`
- `[wp] Add representative GCC9-12 real-compiler evidence to nightly or periodic gates`
