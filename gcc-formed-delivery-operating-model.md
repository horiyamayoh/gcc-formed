# gcc-formed 実装推進オペレーティングモデル

作成日: 2026-04-09  
対象: `horiyamayoh/gcc-formed` vNext 再建フェーズ  
目的: 上位ドグマを、夜間自走可能な実装運用へ落とす

---

## 1. 結論

`gcc-formed` の次の一手として最適なのは、**GitHub Issue 中心の実装運用**に切り替え、その上に **夜間バッチで消化する agent-ready work package** を積む方式である。

言い換えると、

- 上位: doctrine / architecture / spec
- 中位: epic / workstream / acceptance criteria
- 下位: 1 PR = 1 work package
- 実行: 夜間に 10〜20 件の独立タスクを投入
- 朝: maintainer が merge / revise / split / kill を判定

という 4 層構造にする。

自由文のプロンプトを毎晩 20 本投げるだけでは、依存関係、責務境界、停止条件、再試行条件が消える。長期戦では破綻する。

---

## 2. 基本方針

### 2.1 中心は「Issue」、中心は「Prompt」ではない

Prompt はあくまで **Issue を実行するための配送形態** とみなす。

単位は必ず GitHub Issue に持つ。
各 Issue は以下を必須とする。

- 目的
- なぜ今やるか
- 触ってよいレイヤ
- 触ってはいけないレイヤ
- 受け入れ基準
- 実行コマンド
- 依存 Issue
- out of scope
- stop conditions

### 2.2 1 Issue = 1 PR = 1 主目的

夜間自走の成否はここで決まる。

- 1 PR で 2 つ以上の主目的を持たせない
- 仕様変更と大規模リファクタを同時にやらない
- renderer 改善と corpus 追加は原則分ける
- docs-only、tests-only、code-only を混ぜすぎない

### 2.3 「agent-ready」になるまでキューに入れない

Issue を作った瞬間に agent に投げてはいけない。
以下が満たされたものだけを夜間キューに入れる。

- 依存関係が解消済み
- 触る crate が限定されている
- 受け入れ基準が yes/no で判定可能
- コマンドが明示されている
- 失敗時の rollback が明示されている
- contract surface 変更の有無が明示されている

---

## 3. おすすめの管理構造

## 3.1 リポジトリ内の文書レイヤ

最低限、次の 4 文書に分ける。

### A. Doctrine

役割: 非交渉の上位規範。  
内容: 理想像、ドグマ、stop-ship、原則。

### B. Execution Model

役割: doctrine を delivery に落とす文書。  
内容: workstream、issue taxonomy、night queue policy、review policy。

### C. Workstream Index

役割: 現在進める実装群の索引。  
内容: epic 一覧、完了条件、依存グラフ。

### D. Agent Runbook

役割: agent が 1 Issue を解く時の作業手順。  
内容: 読む文書、branch 名、禁止事項、検証、PR 記載事項。

---

## 3.2 GitHub 側の構造

### Milestone

Milestone は**時期**ではなく**到達状態**で切る。

推奨:

- M0: Delivery System Install
- M1: Multi-Band Architecture Skeleton
- M2: Structured Input Abstraction
- M3: Renderer Native-Parity
- M4: Noise Compaction / Ownership
- M5: Corpus & Metrics Gate
- M6: Nightly Agent Autopilot Beta

### Project

Project は 1 つに統合し、すべての Issue / PR を入れる。

推奨 custom field:

- `Workstream` : Architecture / Ingest / Render / Enrich / Corpus / Tooling / Docs
- `Band` : GCC15+ / GCC13-14 / GCC9-12 / Cross-cutting
- `Layer` : Capture / Ingest / IR / Analysis / Render / Quality / Packaging
- `Task Size` : XS / S / M / L
- `Risk` : Low / Medium / High
- `Agent Ready` : No / Draft / Ready / Blocked
- `Night Batch` : None / Tonight / Later
- `Contract Change` : None / Docs / UX / Schema / CLI
- `Human Review Type` : Quick / Deep / Design
- `Stop-Ship` : No / Yes

### Labels

label は検索用に限定し、field と競合させない。

推奨:

- `kind:epic`
- `kind:work-package`
- `kind:bug`
- `kind:refactor`
- `kind:quality-gate`
- `kind:docs`
- `band:gcc15`
- `band:gcc13-14`
- `band:gcc9-12`
- `band:cross`
- `surface:renderer`
- `surface:ingest`
- `surface:corpus`
- `agent-ready`
- `human-only`
- `stop-ship`

---

## 4. Issue の階層

### Epic

Epic は「まとまり」を表す。  
1〜3 週間以上の塊。

例:

- Multi-Band Capability Model を導入する
- StructuredInput abstraction を導入する
- terminal renderer を native parity まで引き上げる
- ownership-aware compaction を実装する

### Work Package

Work Package は agent に渡す最小単位。  
原則 1 PR で閉じる。

1 件あたりの目安:

- 変更ファイル: 3〜10 ファイル程度
- 変更 crate: 1〜2 crate 程度
- 主目的: 1 個
- 実装 + test + docs の説明が 1 PR に収まる

### Human-only Design Task

以下は夜間投入しない。

- support boundary の再定義
- IR schema の大変更
- 互換性を壊す rename / move を大量に伴う変更
- 複数 workstream をまたぐ一括整理
- 「良さそうだから」で acceptance が曖昧な改善

---

## 5. 夜間運用の設計

## 5.1 夜間バッチのルール

1 夜に投入するのは最大 20 件でよいが、**独立性**を優先する。

### 同時投入してよいもの

- 別 crate の小改修
- docs / tests / corpus の独立タスク
- renderer の theme と ingest の enum 分離のように競合しないもの
- 同一 epic 配下でも別ファイル群に閉じるもの

### 同時投入してはいけないもの

- 同じ型や enum を触るタスク
- 同じ snapshot 群を大きく更新するタスク
- 互いに相手の PR を前提にするタスク
- support wording / README / templates を同時に複数人が触るタスク

### 1 夜 1 件に制限すべきもの

- schema 変更
- issue template / PR template 変更
- GitHub workflow 変更
- xtask root command 変更
- support boundary wording 変更

---

## 5.2 夜間バッチの作り方

毎晩、Project から以下条件で抽出する。

- `Agent Ready = Ready`
- `Night Batch = Tonight`
- `Task Size in [XS, S, M]`
- `Risk != High`
- `Human Review Type != Design`
- 依存 Issue が closed

その中から、

- 同一 crate 競合なし
- 同一 file path 競合なし
- 同一 milestone に偏りすぎない
- stop-ship 系は多くても 2 件まで

で 8〜20 件選ぶ。

---

## 5.3 Prompt Packet の標準形

各 work package から機械的に prompt を生成できるようにする。

必須項目:

- Issue title
- Goal
- Why now
- Read first
- Files allowed to touch
- Files forbidden to touch
- Acceptance criteria
- Commands to run
- Snapshot update policy
- Docs update policy
- Out of scope
- Report format

### 推奨プロンプト末尾

- 1 PR に主目的を 1 つだけ含めること
- 依存変更を勝手に広げないこと
- acceptance を満たせない場合は partial patch ではなく failure report を返すこと
- raw fallback / fail-open / support boundary を悪化させないこと

---

## 6. 朝の運用

朝やるべきことはレビューではなく、**仕分け**が先。

各 PR を次の 4 つに分類する。

- `Merge`: そのまま入れる
- `Revise`: 方向は良いが修正が必要
- `Split`: 粒度が大きすぎるので分割
- `Kill`: Issue の切り方自体が悪いので破棄

ここで重要なのは、agent の出来より **Issue 設計の質**を毎朝見直すこと。

失敗 PR が多いなら agent が悪いのではなく、たいていは次のどれか。

- task が大きすぎる
- acceptance が曖昧
- 読むべき文書が足りない
- dependency が未解決
- forbidden scope が書かれていない

---

## 7. 最初に整備すべきもの

優先順位は次の通り。

### Step 1. 既存の playbook を凍結する

現状の playbook / PR template / bug report は、旧来の support boundary を強く前提にしている。  
新 doctrine と整合しないなら、そのまま agent に食わせてはいけない。

やること:

- 現 playbook を `legacy-beta-playbook` 扱いにする
- vNext 用の `EXECUTION-MODEL.md` を新設する
- PR template を vNext 用に差し替える
- bug form を `Path Band / Support Level / Processing Path` に合わせて作り直す

### Step 2. Project を作る

Project を 1 つ作り、field を先に定義する。  
Issue を先に大量作成しない。

### Step 3. Epic を 6〜8 本だけ作る

多く作りすぎない。まずは次の 8 本で十分。

1. Delivery system install
2. Capability model split
3. Structured input abstraction
4. Renderer native parity
5. Noise compaction / ownership
6. Rulepack externalization
7. Corpus / metrics / gate
8. Trace / fallback / observability

### Step 4. 各 Epic を 3〜7 個の work package に分解する

最初から 100 Issue 作る必要はない。  
まず 25〜35 件で十分。

### Step 5. そのうち 8〜12 件だけ `Agent Ready = Ready` にする

キューに入れるのは本当に独立しているものだけ。

---

## 8. 最初の 2 週間のおすすめ順序

### Week 1: Delivery System Install

- vNext execution model 文書を追加
- 既存 playbook を legacy 扱いに変更
- PR template を更新
- Issue forms を追加
- Project field を定義
- label / milestone を定義
- workstream epics を作る
- 最初の 10 件の agent-ready issue を用意

### Week 2: 最初の nightly batch を回す

おすすめの初回バッチ:

- capability model の enum 分離
- structured input abstraction の interface だけ追加
- renderer の ANSI theme 導入
- terse / expanded profile の view model 追加
- rulepack 外部定義の読み込み骨格
- corpus metadata 拡張
- trace summary の schema 追加
- docs / templates の wording 統一

この段階では「大きな価値」より **夜間運用が壊れないこと** を優先する。

---

## 9. 成功判定

この運用が機能しているかは、次の数字で見ればよい。

- nightly 投入 Issue のうち PR 化率
- PR の first-pass merge 率
- revise / split / kill 比率
- Issue 見積もりサイズと実際の PR サイズの乖離
- gate failure のうち task 設計起因の比率
- stop-ship 変更の混入率

理想は、2〜3 週間で次に近づくこと。

- PR 化率: 60% 以上
- first-pass merge 率: 40% 以上
- kill 率: 10% 未満
- task の大きすぎ問題: 週次で減少

---

## 10. 何を避けるべきか

- doctrine からいきなりコードへ飛ぶこと
- issue を作らず prompt だけで運用すること
- 1 夜に同一 crate の競合タスクを大量投入すること
- acceptance が「いい感じにする」になっていること
- workflow / schema / support wording の大変更を同夜に複数流すこと
- release engineering を先に進めすぎること

---

## 11. 最終提案

ベストな進め方は、**GitHub Issue + Project を正本にした delivery system を先に作り、その後に夜間バッチで work package を消化する方式**である。

つまり順番はこうする。

1. doctrine を固定する
2. execution model を書く
3. GitHub Project / Issue taxonomy / templates を整備する
4. epic を切る
5. work package に落とす
6. agent-ready になったものだけ夜間投入する
7. 朝に merge / revise / split / kill を回す
8. 失敗原因を Issue 設計へフィードバックする

この方式なら、夜間に 20 件流しても、朝に repo が「何が起きたか分からない状態」になりにくい。  
長期的に見ると、最速なのはコードを書くことではなく、**夜間自走しても壊れない作業単位を設計すること**である。
