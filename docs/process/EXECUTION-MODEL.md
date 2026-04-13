---
doc_role: current-authority
lifecycle_status: accepted-baseline
audience: both
use_for: Current execution and implementation sequencing rules.
do_not_use_for: Historical planning provenance or superseded delivery playbooks.
supersedes: []
superseded_by: []
---
> [!IMPORTANT]
> Authority: `current-authority` / `accepted-baseline`
> Use for: Current execution and implementation sequencing rules.
> Do not use for: Historical planning provenance or superseded delivery playbooks.

# gcc-formed vNext Execution Model

- 文書種別: 実装運用契約 / delivery system 正本
- 状態: Accepted baseline for immediate use
- 対象: `horiyamayoh/gcc-formed` (`main`, 2026-04-09 時点)
- 目的: Doctrine と vNext 変更設計を、Epic 生成と夜間 agent 運用に耐える実行体系へ落とす
- 想定読者: maintainer / reviewer / coding agent / future contributor

---

## 0. この文書の位置づけ

この文書は、上位の doctrine と変更設計を**実装運用へ翻訳する正本**である。  
ここで固定するのは「何を作るか」ではなく、**どう進めるか**である。

本書の責務は次の 5 つである。

1. 仕様・ADR・Issue・Prompt の上下関係を固定する
2. Epic を切る前に必要な前提条件を固定する
3. 夜間に coding agent を 10〜20 件流しても壊れない作業単位を定義する
4. 朝のレビューと仕分けを deterministic にする
5. 旧来の GCC 15 single-track 前提で組まれた delivery 文書群を、vNext 方針へ置き換える

本書が承認されるまで、**新規 Epic を正式に起票してはならない**。  
Epic の正本は本書承認後に初めて生成される。

---

## 1. 結論

`gcc-formed` の vNext で最初にやるべきことは、コードではなく **Execution Model の導入**である。

理由は明快である。

- 現行 repo の README / support boundary / bootstrap sequence / agent playbook / PR template / bug form は、いずれも **GCC 15 single-track の旧前提**を色濃く残している
- この状態で夜間 agent を回すと、速く進む代わりに **誤った方角へ速く進む**
- vNext は「単一の privileged path を磨く repo」ではなく、「複数 capture path を持ちながら 1 つの UX 原則を守る repo」へ移行する。そのためには作業単位、レビュー単位、停止条件も作り直す必要がある

したがって vNext の最初の到達状態は、機能追加ではない。  
**「正しい方向に安全に速く進める delivery system を install した状態」**である。

---

## 2. 基本原則

### 2.1 Issue が正本、Prompt は派生物

作業単位の正本は GitHub Issue である。  
Prompt は Issue を coding agent に配送するための派生物であり、正本ではない。

したがって、夜間投入する prompt は必ず Issue から生成する。  
maintainer が自由文で毎晩都度プロンプトを書く運用を正本にしてはならない。

### 2.2 1 Issue = 1 PR = 1 主目的

1 つの PR は 1 つの主目的だけを持つ。  
仕様変更と大規模リファクタ、renderer 改善と corpus 追加、support wording 変更とコード挙動変更を 1 つの PR に混在させてはならない。

補助的に同梱してよいものは次に限る。

- その主目的に必要な docs 更新
- その主目的に必要な test / snapshot 更新
- その主目的に必要な changelog / ADR 追記

### 2.3 Architecture first, then behavior

vNext では、まず architecture migration の受け皿を作り、その後に振る舞いを変える。

順序は次で固定する。

1. Execution Model
2. ADR batch
3. contract docs rewrite
4. no-behavior-change abstraction refactor
5. user-visible behavior change
6. quality gate hardening
7. nightly autopilot expansion

この順を逆転させてはならない。

### 2.4 GCC 9-15 は 1 つの public contract として扱う

vNext の delivery system は、`GCC15` / `GCC13-14` / `GCC9-12` を **1 つの in-scope public contract** として並列に扱えるよう設計する。  
internal capture capability は異なってよいが、Issue / PR / docs / quality gate は band ごとの public 価値序列を再導入してはならない。

正しく分けるべき概念は次の 4 つである。

- **VersionBand**: `GCC16+`, `GCC15`, `GCC13-14`, `GCC9-12`, `Unknown`
- **CapabilityProfile**: `dual_sink`, `sarif`, `json`, `native_text`, `tty_color_control`, `fixits`, `locale_stabilization` など
- **ProcessingPath**: `DualSinkStructured`, `SingleSinkStructured`, `NativeTextCapture`, `Passthrough`
- **SupportLevel**: `InScope` または `PassthroughOnly`

Issue と PR は、この 4 つを混同してはならない。

### 2.5 default TTY 非劣化は stop-ship

native GCC より色が消える、長くなる、最初の画面で読みにくくなる、template/std:: ノイズが増える。  
これらは「改善途中の粗さ」ではなく、**default UX regression** である。

vNext では次を stop-ship とする。

- default TTY で色を失い native より見づらくなる
- default TTY で native より有意に長くなるにもかかわらず、修正開始が速くならない
- template / overload / stdlib noise を抑えるという存在理由に反した出力肥大

### 2.6 Human-only tasks を明示する

nightly agent に流してよいのは bounded task のみである。  
次は human-only とする。

- support boundary の再定義そのもの
- IR schema semantics の破壊的変更
- 複数 workstream をまたぐ rename / move の一括整理
- acceptance criteria を yes/no に落とせない探索作業
- doctrine の再解釈を伴う design decision

---

## 3. 上下関係と正本の優先順位

### 3.1 優先順位

設計と運用の正本は、次の優先順位に従う。

1. `gcc-formed-rebuild-doctrine-final.md` 相当の doctrine
2. `gcc-formed-vnext-change-design.md` 相当の変更設計
3. 本書 `EXECUTION-MODEL.md`
4. 新 ADR 群
5. 契約文書群（README, SUPPORT-BOUNDARY, ingestion, rendering, quality, bootstrap など）
6. GitHub Issues / Sub-issues / Work Packages
7. nightly prompt packet
8. PR / commit / branch

下位が上位に反した場合、下位が誤りである。

### 3.2 変更に必要な文書更新

次の変更は、コードだけで終えてはならない。

- capture path / processing path の追加または削除
- support wording の変更
- renderer の line budget / color / disclosure / fallback 規約の変更
- IR schema semantics の変更
- issue taxonomy / project field / nightly policy の変更

### 3.3 本書承認後の生成順

本書承認後の最初の生成順は次で固定する。

1. ADR batch 0026–0033
2. contract docs rewrite 草案
3. `Delivery System Install` 系 Issue 群
4. architecture skeleton Epic 群
5. bounded Work Package 群
6. nightly batch

---

## 4. 現行 repo に対して直ちに行う運用上の切替

### 4.1 legacy 扱いにする文書

次の文書は、現時点の repo では有用だが、vNext の正本ではない。  
本書採択後は **legacy or superseded** 扱いにする。

- `../history/planning/gcc_formed_milestones_agent_playbook.md`
- 旧前提の `implementation-bootstrap-sequence.md`
- 旧 support boundary をコピーした GitHub templates

### 4.2 直ちに差し替える対象

次は `Delivery System Install` の最初の変更対象とする。

- `README.md` の support posture 概要
- `SUPPORT-BOUNDARY.md`
- `.github/pull_request_template.md`
- `.github/ISSUE_TEMPLATE/bug_report.yml`
- `implementation-bootstrap-sequence.md`

### 4.3 CODEOWNERS を導入する

コード責任境界を曖昧にしないため、`.github/CODEOWNERS` を導入する。  
最初の境界は crate / docs 群ベースで十分である。

推奨の最初の切り方:

- `diag_backend_probe/`, `diag_capture_runtime/`, `diag_adapter_gcc/`
- `diag_core/`, `diag_enrich/`, `diag_residual_text/`, `diag_render/`
- `xtask/`
- `docs/`, `adr-initial-set/`, top-level specs
- `.github/`, workflow, templates

---

## 5. GitHub 運用の正本構造

### 5.1 Projects

Project は 1 つに統合し、Issue / PR / draft item を一元管理する。  
board を分けず、view を分ける。

Project で最低限持つべき view:

1. **Backlog Table** — 全件の表形式
2. **Tonight Queue** — `Agent Ready = Ready` かつ `Night Batch = Tonight`
3. **By Workstream Board** — Workstream 単位のカンバン
4. **By Milestone Roadmap** — 到達状態ベースのロードマップ
5. **Stop-Ship Watchlist** — `Stop-Ship = Yes` の一覧
6. **Design Review Queue** — `Human Review Type = Design`

### 5.2 Milestones

Milestone は時期ではなく**到達状態**で切る。

vNext の最初の推奨 Milestone:

- `M0 Delivery System Install`
- `M1 Architecture Skeleton`
- `M2 Capture/Ingress Abstraction`
- `M3 Native-Parity Renderer`
- `M4 Noise Compaction & Ownership`
- `M5 Quality Gate & Corpus`
- `M6 Nightly Autopilot Beta`

### 5.3 Sub-issues

Epic は親 Issue とし、Work Package は sub-issue とする。  
Markdown の task list は補助としてはよいが、正本は sub-issue に置く。

### 5.4 Project custom fields

Project の custom field は次を必須とする。

| Field | Type | Allowed values / meaning |
|---|---|---|
| `Workstream` | single select | Architecture / Capture / Ingest / IR / Analysis / Render / Quality / Tooling / Docs / Release |
| `Band` | single select | GCC15 / GCC13-14 / GCC9-12 / GCC16+ / Unknown / Cross-cutting |
| `Processing Path` | single select | DualSinkStructured / SingleSinkStructured / NativeTextCapture / Passthrough / Cross-path |
| `Support Level` | single select | InScope / PassthroughOnly / Internal-only |
| `Layer` | single select | Capture / Ingest / IR / Analysis / ViewModel / Theme / Quality / Templates / Packaging |
| `Issue Kind` | single select | Epic / Work Package / Bug / Spike / Docs / ADR / Chore |
| `Task Size` | single select | XS / S / M / L |
| `Risk` | single select | Low / Medium / High |
| `Contract Change` | single select | None / Docs / UX / Schema / CLI / Packaging |
| `Agent Ready` | single select | No / Draft / Ready / Blocked |
| `Night Batch` | single select | None / Tonight / Later |
| `Human Review Type` | single select | Quick / Deep / Design |
| `Stop-Ship` | single select | No / Yes |
| `Owner Layer` | single select | Maintainer / Agent / Shared |
| `Depends On` | text | blocker issue numbers |
| `Commands` | text | minimum validation command set |
| `Acceptance ID` | text | short identifier of the acceptance contract |

### 5.5 Fallback when Project features are unavailable

GitHub Project custom fields / views が使えない環境では、語彙を変えずに正本の置き場だけを縮退させる。

その場合の暫定正本は次とする。

- 親 Epic Issue
- その sub-issue としての Work Package
- milestone
- labels
- handoff comment

Project field の値は、各 Issue body に同じ field 名で明示する。  
Project unavailable を理由に vocabulary や Issue taxonomy を変えてはならない。

### 5.6 Labels

label は検索の補助に限る。  
field と責務を重ねすぎない。

推奨最小集合:

- `kind:epic`
- `kind:work-package`
- `kind:bug`
- `kind:docs`
- `kind:adr`
- `band:gcc15`
- `band:gcc13-14`
- `band:gcc9-12`
- `band:cross`
- `stop-ship`
- `agent-ready`
- `human-only`

---

## 6. Issue taxonomy

### 6.1 Epic

Epic は 1〜3 週間以上のまとまりを表す親 Issue である。  
Epic 自身は原則としてコード変更を持たない。

Epic が必ず持つもの:

- 目的
- doctrine / change-design 上の根拠
- 完了条件
- 依存 Epic
- 生成すべき Work Package の型
- out of scope
- no-go condition

### 6.2 Work Package

Work Package は nightly agent に流せる最小単位である。  
原則として 1 PR で閉じる。

Work Package が必ず持つもの:

- Goal
- Why now
- Affected layers
- Allowed files / crates
- Forbidden surfaces
- Acceptance criteria
- Commands
- Docs impact
- Stop conditions
- Rollback / abandon rule

### 6.3 Bug

Bug は現象を報告する Issue であり、実装タスクではない。  
Bug をそのまま nightly に流してはならない。  
triage の結果、1 つ以上の Work Package へ分解してから流す。

### 6.4 Spike

Spike は探索タスクである。  
探索結果をまとめることが主目的であり、コード変更は副次的である。  
Spike は human-owned を原則とし、nightly の主対象にしない。

### 6.5 ADR

ADR は設計判断そのものを固定する Issue / 文書である。  
ADR の受理前に、その ADR に依存する Epic を正式起票してはならない。

---

## 7. agent-ready の定義

Issue は、次の条件をすべて満たすときだけ `Agent Ready = Ready` になる。

### 7.1 必須条件

1. 親 Epic または上位 ADR が承認済み
2. 依存 Issue がすべて closed または同夜投入不要
3. 触る crate / docs が限定されている
4. 1 PR で閉じる主目的が 1 つに絞られている
5. 受け入れ基準が yes/no で判定できる
6. 最低限のコマンド列が明示されている
7. contract surface 変更の有無が明示されている
8. stop condition が明示されている
9. reviewer が確認すべき証拠が明示されている

### 7.2 agent-ready にしてはならない条件

次のいずれかに該当する Issue は `Agent Ready = No` または `Blocked` とする。

- 仕様の意味論が未承認
- 依存する rename / move が先に必要
- 受け入れ基準が「良い感じ」しかない
- 変更範囲が 3 crate 以上に広がる見込み
- README / support boundary / templates / workflows を同時に複数触る
- snapshot 群が大規模に競合する見込み
- 同夜の別タスクと同じ型・同じ enum・同じ test fixture を触る

### 7.3 Task size 規約

- `XS`: docs-only または 1 file / 1 assertion / 1 template 級
- `S`: 1 crate / 1 small behavior / 1 docs touch / 競合低
- `M`: 1〜2 crate / abstraction refactor または bounded behavior change
- `L`: nightly 非推奨。human-owned に寄せる

nightly に流すのは `XS`, `S`, `M` に限る。

---

## 8. nightly batch policy

### 8.1 nightly の目的

nightly の目的は「人間が朝に merge 判断できる PR を量産すること」である。  
「コードをたくさん変えること」ではない。

### 8.2 nightly 抽出条件

その夜に抽出してよい条件は次のとおり。

- `Agent Ready = Ready`
- `Night Batch = Tonight`
- `Task Size in {XS, S, M}`
- `Risk != High`
- `Human Review Type != Design`
- `Depends On` が未解消でない

### 8.3 衝突回避規約

同じ夜に同時投入してはならない組み合わせ:

- 同じ enum / same file / same snapshot cluster を触る Issue 群
- 同じ top-level contract doc を触る Issue 群
- 同じ template / workflow / xtask entrypoint を触る Issue 群
- support wording とそれを参照する template 群を別 PR でばら撒くこと

### 8.4 1 夜 1 件制限

次は 1 夜につき 1 件まで。

- `SUPPORT-BOUNDARY.md` の変更
- `.github/pull_request_template.md` の変更
- `.github/ISSUE_TEMPLATE/*` の変更
- workflow / Actions / release automation の変更
- `diagnostic-ir-*.md` の schema semantics 変更

### 8.5 nightly prompt packet

nightly に投入する prompt は、各 Issue から次の形で生成する。

```text
Title:
Issue:
Why now:
Read first:
Allowed files:
Forbidden surfaces:
Acceptance criteria:
Commands:
If blocked:
Do not do:
PR body must include:
```

人間が追加で自由文を書く場合も、この構造を壊してはならない。

---

## 9. 朝のレビュー運用

### 9.1 朝の最初の判定は 4 択

各 PR は最初に次の 4 択で分類する。

- `Merge`
- `Revise`
- `Split`
- `Kill`

### 9.2 4 択の意味

- `Merge`: 受け入れ基準を満たし、方向も正しい
- `Revise`: 方向は正しいが証拠不足または小修正が必要
- `Split`: Issue の切り方が悪く、1 PR に主目的が複数入った
- `Kill`: 方角が誤っている、または stop condition を踏んだ

### 9.3 朝に見るべきもの

レビューの順序は次で固定する。

1. Issue と PR が一致しているか
2. 主目的が 1 つか
3. Acceptance criteria を満たす証拠があるか
4. contract surface が未申告で変わっていないか
5. default TTY 非劣化を破っていないか
6. out of scope を踏み越えていないか
7. 同夜の他 PR と衝突していないか

### 9.4 nightly の改善対象

nightly 失敗時にまず疑うべきは agent ではなく **Issue の切り方**である。  
朝の仕分け結果は、必ず次のどれかに還元する。

- acceptance criteria が曖昧だった
- allowed / forbidden surfaces が曖昧だった
- task size が大きすぎた
- dependencies を解消せず流した
- 同夜に競合タスクを流した

### 9.5 session handoff を固定する

各 session の終了時に、active milestone の親 Epic へ handoff comment を 1 件だけ追記する。  
形式は次で固定する。

```text
current state:
blockers:
in-flight PRs:
next 3 ready work packages:
docs or contracts touched:
```

chat にしか存在しない進捗を残して session を閉じてはならない。

---

## 10. vNext で最初に起こすべき Epic 群の前提

Epic を正式起票する前に、最低限次の 3 つを終える。

1. 本書の承認
2. ADR batch 0026–0033 の承認
3. contract docs rewrite 草案の承認

その後に初めて起票してよい Epic は次の種類である。

- Delivery System Install
- Capability / Path model install
- CaptureBundle install
- TTY native-parity renderer
- Noise compaction / ownership
- Quality gate re-baseline

---

## 11. 最初の 2 週間の実行順

### Week 1: delivery system install

1. 本書を repo 正本として追加
2. `../history/planning/gcc_formed_milestones_agent_playbook.md` を legacy 扱いに変更
3. PR template を vNext vocabulary に置換
4. bug form を legacy tier wording 中心から `Band / Processing Path / Support Level` 中心に置換
5. `.github/CODEOWNERS` を追加
6. GitHub Project を作成し custom fields を投入
7. Milestone を作成
8. ADR batch 0026–0033 を投入

### Week 2: contract docs rewrite

1. `README.md` の support posture rewrite
2. `SUPPORT-BOUNDARY.md` rewrite
3. `implementation-bootstrap-sequence.md` rewrite
4. `gcc-adapter-ingestion-spec.md` rewrite 草案
5. `rendering-ux-contract-spec.md` に color / line budget / compaction MUST を追加
6. `quality-corpus-test-gate-spec.md` を path-aware に更新
7. 最初の `Delivery System Install` Work Package 群を `Agent Ready` にする

---

## 12. stop-ship / no-go

### 12.1 stop-ship

次は merge gate または release gate を止める。

- default TTY 非劣化に反する change
- support wording だけ先に広げ、コードと gate が追いついていない change
- Path B/C を first-class と言いながら passthrough-only のままにする change
- prompt-only 運用へ逆戻りさせる change
- Project fields / Issue taxonomy を bypass して nightly を回す change

### 12.2 no-go

次の状態では nightly を拡大してはならない。

- `Agent Ready` の判断が maintainer 間で揺れる
- PR template と Issue template が旧 vocabulary のまま
- contract docs rewrite が未着手
- CODEOWNERS / review routing がない
- 4 択レビュー (`Merge/Revise/Split/Kill`) が運用されていない

---

## 13. Work Package テンプレート

```markdown
## Goal

## Why now

## Parent epic / ADR

## Affected band
- [ ] GCC15
- [ ] GCC13-14
- [ ] GCC9-12
- [ ] GCC16+ / Unknown
- [ ] Cross-cutting

## Processing path
- [ ] DualSinkStructured
- [ ] SingleSinkStructured
- [ ] NativeTextCapture
- [ ] Passthrough
- [ ] Cross-path

## Allowed files

## Forbidden surfaces

## Acceptance criteria
- [ ]
- [ ]
- [ ]

## Commands
- `...`

## Docs impact
- [ ] None
- [ ] README / SUPPORT-BOUNDARY
- [ ] Spec / ADR
- [ ] Template / workflow

## Stop conditions
- [ ]

## Reviewer evidence
- [ ] Tests
- [ ] Snapshot diff rationale
- [ ] Docs diff rationale
```

---

## 14. Epic テンプレート

```markdown
## Objective

## Why this matters to doctrine

## Completion criteria
- [ ]
- [ ]
- [ ]

## Dependencies

## Generates these work package classes
- [ ]
- [ ]

## Out of scope

## No-go conditions
```

---

## 15. PR テンプレートに必ず入れるべき項目

- Goal
- Why now
- Parent issue / ADR
- Band
- Processing Path
- Contract Change
- Allowed / forbidden surfaces compliance
- Acceptance criteria evidence
- Commands run
- Snapshot / docs rationale
- Stop condition not hit の明示

---

## 16. 本書の核心

> **Issue を正本にし、Prompt を派生物にする。**
>
> **Epic より先に Execution Model を固定する。**
>
> **夜間に 20 件流せることより、朝に 20 件正しく捨てられることを先に設計する。**

---

## Appendix A. 本書が supersede / legacy 化するもの

- `../history/planning/gcc_formed_milestones_agent_playbook.md` の vNext 正本としての役割
- 旧 single-tier 中心の delivery vocabulary
- GCC 15 single-track wording をそのまま Work Package 管理へ持ち込む運用

## Appendix B. 参照すべき現行 repo 文書

- `README.md`
- `SUPPORT-BOUNDARY.md`
- `implementation-bootstrap-sequence.md`
- `gcc-adapter-ingestion-spec.md`
- `rendering-ux-contract-spec.md`
- `quality-corpus-test-gate-spec.md`
- `.github/pull_request_template.md`
- `.github/ISSUE_TEMPLATE/bug_report.yml`
- `../history/planning/gcc_formed_milestones_agent_playbook.md`

## Appendix C. GitHub 機能で本書が前提にするもの

- Projects の custom fields / views / insights / built-in automation
- Issues の sub-issues
- repository-level templates / issue forms
- CODEOWNERS
- scheduled workflows / Actions
