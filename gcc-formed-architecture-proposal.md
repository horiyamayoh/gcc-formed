
# C/C++ 診断 UX 改善プロジェクト最上位設計提案
**作業名**: `gcc-formed`（長期的には compiler-agnostic な `cc-formed` を推奨）  
**対象**: Linux first / 社内配布 / GCC first / 高品質優先  
**版**: Initial Architecture Proposal  
**日付**: 2026-04-06

---

## 前提として置く合理的な仮定

1. 初期の公式サポート対象は **Linux 上の GCC 15 系** とする。  
   理由は、GCC 13 で SARIF 出力が追加され、GCC 15 で「テキスト + SARIF の同時出力」「SARIF 上の include chain 強化」「legacy JSON の非推奨化」が入り、品質重視の設計に必要な前提が最も揃っているため。  
   ただし、**GCC 13–14 は互換モード**、**GCC 12 以下は passthrough only** として扱う。

2. 成功の中心は **見た目** ではなく、**修正速度** と **誤誘導の少なさ** である。

3. 初期フェーズでは **LLM/生成 AI に依存しない**。  
   理由は、診断 UX の基礎品質で最も重要なのは、  
   - 構造化  
   - 因果関係の保持  
   - 根本原因のランキング  
   - 情報の削減ではなく「圧縮」  
   であり、まずは deterministic / testable な設計が必要だから。

4. ローカル端末、CI、将来のエディタ連携は **同じ正規化 IR** を共有し、レンダラだけを分ける。

---

# 1. エグゼクティブサマリー

## このプロジェクトをどう捉えるべきか

このプロジェクトは「gcc の stderr を少し綺麗にするツール」ではない。  
**C/C++ 向けの“診断プラットフォーム”** として捉えるべきである。

本質は以下の 4 層にある。

1. **既存ビルドフローに最小変更で差し込める導入口**  
   → wrapper / compiler-like CLI

2. **コンパイラ固有出力を失わずに取り込む構造化層**  
   → GCC SARIF / raw stderr / 将来の Clang adapter

3. **compiler-agnostic な診断 IR**  
   → severity, span, note, fix, include/macro/template chain, linker context, confidence など

4. **人間向け / CI 向け / 将来の editor 向け再レンダリング層**  
   → terminal renderer, CI renderer, JSON IR emitter, SARIF emitter

つまり、作るべきものは **「gcc ラッパー」単体ではなく、wrapper-first の診断基盤** である。  
ラッパーは導入形態であり、プロダクト本体は **IR と enrichment/ranking/rendering** にある。

## 何を作るべきか

最初に作るべきものは次の 7 点に絞る。

1. **drop-in wrapper CLI**
2. **GCC structured diagnostics adapter**
3. **normalized diagnostic IR**
4. **root-cause ranking / summarization engine**
5. **terminal / CI renderer**
6. **safe fallback / passthrough**
7. **実コンパイラ出力コーパス + ゴールデンテスト基盤**

逆に、最初に作ってはいけないものは以下。

- IDE プラグイン
- daemon/service 常駐機構
- 全コンパイラ同時対応
- 深い自前 C++ 解析器
- “賢く見える”が検証不能な AI 説明機能

## 何を最初に作るべきで、何を後回しにすべきか

### 最初に作る
- GCC 15 first の **single-pass structured path**
- GCC 13–14 用の **compatibility / replay path**
- 診断 IR v1alpha
- 主要 5 診断族の root-cause UX
  - 構文エラー
  - 型不一致
  - C++ テンプレート連鎖
  - include / macro 連鎖
  - linker の代表的失敗
- 可観測性・trace bundle
- corpus-driven QA

### 後回し
- Clang adapter
- editor integration
- advanced linker reasoning
- org-specific knowledge base link
- interactive TUI
- distributed daemon
- auto-fix apply

## 最終推奨案

**推奨アーキテクチャ**  
> **IR-centered / adapter-separated / wrapper-first / library+CLI 二層構成**

**推奨導入形態**  
> まずは `gcc-formed` / `g++-formed` として導入し、長期的には compiler-agnostic な `cc-formed` ブランドに寄せる。

**推奨技術**  
> **Rust 実装 + 単一バイナリ配布 + GCC SARIF を一次情報源とする設計**

**最重要判断**  
> **“text parsing first” を捨てる。Structured diagnostics first にする。**

---

# 2. 問題定義

## gcc / g++ の診断体験の何がつらいのか

GCC 診断がつらい主因は、単に見た目が古いからではない。  
本質的には次の 7 つである。

### 2.1 根本原因より派生症状が目立つ
C/C++ では、1 個の構文ミスや型不一致が、その後の多くの派生エラーを生む。  
ユーザーが欲しいのは「最初に直すべき 1 箇所」だが、現実には note / error / instantiation trace / include trace が混ざる。

### 2.2 note が文脈説明とノイズの両方を兼ねてしまう
note は有益だが、頻繁に洪水になる。  
しかも「なぜ起きたか」と「どこから来たか」と「修正ヒント」が同じ平面に置かれがちで、読み手の認知コストが高い。

### 2.3 原因が現在地から遠い
C/C++ は以下の “遠距離依存” が強い。
- include 連鎖
- macro expansion
- template instantiation
- implicit conversion / overload resolution
- linker まで遅延する欠陥

そのため、エラー箇所に見える行が「原因」ではなく「爆発地点」であることが多い。

### 2.4 型表現が長く、差分が読みにくい
特に C++ では、型不一致が「型そのものの構造差」ではなく「長い文字列の差」として現れやすい。  
人間が見たいのはフル型名ではなく、**最初に食い違う節点** である。

### 2.5 system header / stdlib / vendor code が前面に出る
ユーザーが直せるのは通常 “自分のコード” だが、出力の見た目では system header 側が前面に出ることがある。  
これは修正速度を落とす。

### 2.6 linker diagnostics はさらに非構造
コンパイル段階よりも link 段階の診断は非構造・非一貫になりやすく、未定義参照や多重定義でも、修正行動へ直結する整理が不足しがち。

### 2.7 機械可読データを活かし切れていない
GCC 側には SARIF / JSON / fix-it / path / template tree など、診断改善の素材がある。  
しかし日常利用の UX は、依然として「コンパイラがその場で吐いた表示」に強く制約される。  
つまり **compiler diagnostic capabilities** と **developer UX** の間に製品レイヤが無い。

## Rust / Haskell の診断体験の何が優れているのか

Rust / Haskell の優位は「カラフル」だからではない。  
以下の設計思想が効いている。

### 2.8 情報の役割が分かれている
Rust では、`help` は「どう直すか」、`note` は「追加文脈」と明確に役割分離され、提案には applicability の概念まである。  
これは “読む” ための出力ではなく、**修正行動を誘導する出力** である。

### 2.9 安定した識別子や外部説明先がある
GHC は JSON 診断、エラー index へのリンク、エラーコード化を進めており、単なる一次メッセージで終わらず、**知識ベースへの接続点** を持つ。  
これは長期運用で非常に効く。

### 2.10 診断が「構造」として扱われている
span, label, note, help, suggestion, applicability などが first-class であり、出力フォーマットを変えても意味が壊れにくい。  
ここが最重要で、C/C++ 側の UX 改善もこのレベルに上げる必要がある。

## C/C++ 特有の難しさ

### 2.11 テンプレート
- instantiation stack が長い
- stdlib 側ノイズが多い
- 型の共通部分と差分が絡む
- “error site” と “action site” がズレる

### 2.12 マクロ
- 展開前後で責務がズレる
- 見えているコードとコンパイラが見ているコードが違う
- macro note は有益だが非常に読みにくい

### 2.13 include 連鎖
- 失敗地点が indirect include の奥にある
- ユーザー視点では「どの自分の include から辿ったか」が重要

### 2.14 型爆発 / overload resolution
- overload candidate 列挙が長い
- 何が “最初の不一致” かが見えにくい

### 2.15 linker エラー
- symbol name, mangling, 宣言/定義ズレ, 入力不足, ライブラリ順序, ABI 不一致が混ざる
- phase が parse/typecheck と違い、修正戦略も違う

### 2.16 文脈の遠さ
- 原因が「今見ている行」に無い
- しかも“本当に直せる場所”は 1 つとは限らない

## このプロジェクトが解くべき本質課題

このプロジェクトが解くべき本質課題は以下に尽きる。

> **C/C++ コンパイル失敗を、「コンパイラが知っている事実」から「開発者が次に取るべき修正行動」へ、情報損失なく変換すること。**

見た目の再装飾ではなく、
- **根本原因の抽出**
- **因果連鎖の圧縮**
- **修正可能地点の強調**
- **元情報の保持**
が本質である。

---

# 3. 成功指標と評価方法

成功条件は定量・定性の両方で定義すべきである。

## 3.1 KPI の基本方針

ベンチマーク対象は少なくとも以下の 4 つ。
1. raw GCC default
2. tuned GCC（公平性のため、可能なら template-tree 等を有効にした版）
3. Clang default（比較上限の一つ）
4. 本プロダクト

## 3.2 定量 KPI

| KPI | 定義 | 測定方法 | 目標 |
|---|---|---|---|
| Time to Root Cause (TRC) | ユーザーが「最初に直すべき箇所」に到達するまでの時間 | タスク実験 / 画面録画 / first corrective edit | raw GCC 比で **35% 以上短縮** |
| Time to First Actionable Hint (TFAH) | 最初の具体的修正候補が提示されるまでの時間 | 表示開始から hint 行到達まで | raw GCC 比で **50% 以上短縮** |
| First-Fix Success Rate | 最初の修正で root cause を改善できた割合 | タスク成功率 | raw GCC 比で **+20pt** |
| Noise Before Action | 最初の actionable line より前に読む必要がある非本質行数 | 表示解析 | median **8 行以下** |
| Diagnostic Compression Ratio | raw 表示行数 / 新表示行数 | corpus 比較 | 主要ケースで **1.5x〜4x 圧縮** |
| High-Confidence Mislead Rate | 高 confidence で誤った root cause を出した割合 | expert review / corpus | **2% 未満** |
| Fallback Rate | enhanced path が使えず raw fallback になった率 | telemetry / trace | enhanced-eligible run の **0.1% 未満** |
| Fidelity Defect Rate | 元 compiler 情報を欠落/改変して問題化した率 | bug triage | **0 P0**、P1 極小 |
| p95 Overhead (success path) | wrapper 自体の追加オーバーヘッド | benchmark | **p95 < 40ms**（single-pass path） |
| p95 Postprocess Time (failure path) | コンパイラ終了後の解析・描画時間 | benchmark | **p95 < 80ms** |

## 3.3 定性 KPI

### ユーザー理解評価
5 段階で評価する。
- 何が悪いか分かったか
- 最初にどこを直せばよいか分かったか
- note が役に立ったか
- 情報が多すぎないか
- raw GCC より速く直せそうか

目標: 平均 **4.2/5 以上**

### CI 可読性評価
CI 利用者に対して以下を評価。
- スクロールせず root cause が見えるか
- 横幅制約下でも読めるか
- ANSI 無しでも意味が保てるか
- raw log が必要になったとき追跡可能か

### 運用担当者評価
- 配布が壊れにくいか
- バージョン固定が容易か
- rollback しやすいか
- 既存ビルドフローに無理なく入るか

## 3.4 評価設計

### A. Expert-labeled corpus
各ケースに以下を付与する。
- ground-truth root cause
- expected first action
- acceptable alternative actions
- expected noise suppression
- required preserved facts

### B. Developer task study
- C / C++ を混ぜた課題セット
- raw GCC と本ツールをランダム順で比較
- タスク終了までの時間、修正回数、主観評価を取得

### C. Shadow deployment
CI / opt-in local 環境で shadow mode を走らせ、
- raw output
- structured ingress
- rendered output
- fallback reason
- user feedback
を蓄積し、回帰と corpus 拡張に使う。

---

# 4. プロダクト原則

本ツールが守るべき原則を以下に固定する。

## 4.1 Root cause first
表示の先頭には「最初に直すべきもの」を置く。  
ただし confidence が低いときは断定せず、`Likely root cause` に落とす。

## 4.2 Action before explanation
最初に「何をすればよいか」を示し、その後に「なぜそうなったか」を示す。

## 4.3 Compress, never silently discard
note や instantiation trace は削除しない。  
**圧縮して見せる**。  
省略した場合は件数と種別を明示する。

## 4.4 Preserve provenance
すべての表示要素は、元の compiler diagnostic に辿れること。  
rendered summary は provenance を持つ。

## 4.5 Fail open
内部エラー、未対応バージョン、解析失敗時は **確実に raw compiler output にフォールバック** する。

## 4.6 No hallucinated precision
診断コードが無いのにあるように見せない。  
不確実な heuristic は confidence を表示する。

## 4.7 User-code first
system header / stdlib / vendor code より、**ユーザーが直せる地点** を先に出す。

## 4.8 Same facts, different renderers
terminal / CI / machine output で意味が変わってはならない。  
変わるのは表示密度だけ。

## 4.9 Deterministic and testable
同じ入力に対して同じ出力を返す。  
ランキングと suppression は deterministic rule であること。

## 4.10 Trust compiler truth
このツールは compiler の代わりに意味を創作しない。  
**解釈は足すが、事実は書き換えない。**

---

# 5. ユーザー像と主要ユースケース

## 5.1 対象ユーザー

### A. C 開発者
- 構文エラー
- incompatible pointer type
- macro 汚染
- include 問題
- linker 未定義参照

### B. C++ 開発者
- overload resolution
- template instantiation
- concepts / SFINAE / 型爆発
- 名前探索
- ODR / ABI / linker 問題

### C. 大規模コードベース保守者
- vendor / stdlib / generated code を跨いだ診断
- CI failure triage
- regression 再現

### D. CI 利用者
- ログが長く、失敗箇所だけ素早く見たい
- ANSI なし / 幅制限下でも読みたい

### E. editor / IDE 統合を望む開発者
- 将来的に same IR をエディタ上でも使いたい

### F. 社内配布・運用担当者
- 単一バイナリ
- 壊れにくい依存
- version pinning
- rollback 容易性
を重視する

## 5.2 主要ユースケース

### UC-1: ローカルでの単一ファイル compile failure
- `CC=gcc-formed make`
- 1 個の構文ミス
- すぐに直せる出力が必要

### UC-2: C++ テンプレート連鎖で stdlib ノイズが多い
- 実際に直すべき user frame を見つけたい
- 共通型部分ではなく差分だけ見たい

### UC-3: include / macro 越しに爆発したエラー
- 自分のどの include / macro 使用が起点か知りたい

### UC-4: CI の linker failure
- undefined reference の原因が「入力不足」か「シグネチャ不一致」か判断したい

### UC-5: rollout / support
- wrapper が壊れても build が止まらないこと
- debug bundle を添えて問題報告できること

---

# 6. 機能要件

## 6.1 MVP（Phase 1〜2）で必須

### 6.1.1 透過 wrapper 実行
- `gcc-formed` / `g++-formed` として gcc/g++ 互換に呼べる
- argv を極力解釈せず透過転送
- real compiler discovery が安全
- symlink 名から C/C++ driver を切替可能

### 6.1.2 GCC structured ingestion
- GCC 15+: `-fdiagnostics-add-output=sarif:file=...` を用いた single-pass structured capture
- GCC 13–14: compatibility path
  - `-fdiagnostics-format=sarif-file`
  - 必要に応じて replay / rerun fallback
- raw stderr も保持

### 6.1.3 Diagnostic IR v1alpha
最低限以下を持つ。
- severity
- message
- primary span
- secondary spans
- child diagnostics / notes
- fixits / suggestions
- include/macro/template/linker context（埋まる範囲で）
- raw payload reference
- confidence / ranking metadata
- provenance

### 6.1.4 再構成表示
- terminal human mode
- CI plain mode
- concise / default / verbose の 3 密度
- color / no-color
- width aware
- raw fallback mode

### 6.1.5 Root-cause ranking
MVP は rules-based でよい。  
最初に高品質で扱う対象は以下。
1. syntax / missing token
2. type mismatch / incompatible argument
3. undeclared / no member / rename candidate
4. template mismatch + instantiation condensation
5. common linker failures

### 6.1.6 Note flood compression
- follow-on error の圧縮
- template trace の圧縮
- include chain の圧縮
- macro expansion chain の圧縮

### 6.1.7 Safe fallback
- unsupported compiler version
- sidecar missing
- parse failure
- internal panic
- budget exceed
で raw output へ fail-open

### 6.1.8 Trace / debug bundle
- raw stderr
- SARIF sidecar
- normalized IR JSON
- render decision log
- version / environment summary（redacted）
- fallback reason

### 6.1.9 テストハーネス
- fixture compile runner
- multi-GCC matrix
- golden render test
- IR snapshot test

## 6.2 将来拡張

### 6.2.1 Clang adapter
- stable public options を優先
- parseable fixits / source range info / optional SARIF path

### 6.2.2 richer linker diagnostics
- undefined reference / multiple definition / cannot find -l / ABI mismatch の精密分類
- demangling / reference-to-definition correlation

### 6.2.3 machine-readable outputs
- canonical IR JSON schema v1
- enriched SARIF export
- maybe LSP-oriented projection

### 6.2.4 editor integration
- same core library を用いた editor bridge
- compile_commands.json ベースの実行

### 6.2.5 policy / org knowledge links
- classifier ID から社内 wiki / coding guideline へ誘導

### 6.2.6 advanced summarization
- overload candidate grouping
- concept / requires failure summary
- ownership-aware suppression
- “why this is not your code” explanation

---

# 7. 非機能要件

## 7.1 性能
- success path p95 overhead < 40ms（single-pass mode）
- failure path postprocess p95 < 80ms
- memory p95 < 100MB
- pathological template caseでも hard cap / graceful degrade あり

## 7.2 可搬性
- Linux x86_64 / aarch64 を first-class
- glibc 依存を最小化
- 可能なら musl 単一バイナリ
- 将来 Windows/macOS へ持ち込みやすい設計

## 7.3 配布容易性
- 単一バイナリ優先
- 追加ランタイム不要
- repo 単位・toolchain 単位で導入容易
- rollback が簡単

## 7.4 保守性
- compiler adapter と renderer を分離
- IR を中核に固定
- versioned schema
- rule engine をデータ駆動寄りにする

## 7.5 テスタビリティ
- adapter contract tests
- IR snapshot tests
- renderer golden tests
- end-to-end compiler matrix
- regression corpus

## 7.6 観測可能性
- structured trace
- debug bundle
- optional shadow mode
- redacted telemetry hooks（デフォルト off）

## 7.7 安全なフォールバック
- raw compiler output を失わない
- wrapper failure が build failure の原因にならない

## 7.8 backwards compatibility
- CLI flag の安定性
- config schema versioning
- IR schema additive evolution
- supported compiler tier の明文化

## 7.9 依存性管理
- permissive-license のみ
- lockfile と vendor
- SBOM / license report 生成
- transitive dependency の継続監査

## 7.10 セキュリティ
- shell 文字列ではなく argv 配列で compiler 起動
- temp file は安全な専用 dir
- source excerpt は terminal escape を sanitize
- path / env / source snippet の trace には redaction policy を適用

## 7.11 ライセンス健全性
- product 本体は permissive
- GPL 連鎖を持ち込まない
- GCC は外部プロセスとして使い、リンクしない
- GCC plugin / internal ABI 依存は避ける

---

# 8. 非目標（Non-goals）

初期フェーズでは以下をやらない。

1. 自前コンパイラを作らない
2. 自前 C/C++ パーサをゼロから作らない
3. 全コンパイラ同時対応をしない
4. IDE プラグインを最初から作らない
5. 常駐 daemon を前提にしない
6. auto-apply fix をしない
7. LLM による自由生成 explanation をコア依存にしない
8. compiler message のローカライズを最初からやらない
9. analyzer / sanitizer / static analysis 全般を一気に取り込まない
10. linker diagnostics を完全構造化できると仮定しない

---

# 9. アーキテクチャ候補の比較

## 9.1 候補 A: 薄い text-wrapper 型

**概要**  
gcc の stderr を受けて regex / text parser で再整形する。

### 長所
- 初速が速い
- 実装は比較的単純
- GCC バージョンに関係なく一応動く

### 短所
- 品質の天井が低い
- バージョン差分に脆い
- Clang 拡張がむしろ難しい
- CI / IDE 用の machine-readable へ拡張しにくい
- root-cause ranking の信頼性が低い

### 評価
**採用非推奨**。  
MVP を急ぐ誘惑はあるが、長期品質では負ける。

---

## 9.2 候補 B: 診断 IR 中心の CLI モノリス

**概要**  
wrapper が structured diagnostics を取り込み、単一バイナリ内部で IR 化→ranking→rendering まで行う。

### 長所
- 品質は高い
- extension point は明確
- single binary にしやすい

### 短所
- 将来 IDE / CI / replay tool を別形態で使いたくなったとき再利用が弱い
- テストで renderer と core が密結合化しやすい

### 評価
**良い候補**。  
ただし長寿命化を考えると core library 分離が欲しい。

---

## 9.3 候補 C: compiler-adapter + normalized IR + renderer 分離、library + CLI 二層型

**概要**  
内部コアを library として持ち、その上に wrapper CLI / render CLI / future editor bridge を載せる。

### 長所
- 品質が最も高い
- GCC → Clang 拡張に強い
- CI / editor / replay / corpus tooling に横展開しやすい
- テストしやすい
- renderer 変更が core を壊しにくい

### 短所
- 初期設計コストが高い
- 過剰抽象の誘惑がある

### 評価
**最有力**。  
複雑さは増えるが、このプロジェクトの寿命と品質要求に見合う。

---

## 9.4 候補 D: daemon / service 併用型

**概要**  
wrapper は軽くし、常駐プロセスが compiler metadata / source cache / ranking を担う。

### 長所
- 将来 editor との統合が強い
- キャッシュが効く
- 重い解析を amortize できる

### 短所
- 運用が難しい
- CI / hermetic build と相性が悪い
- 障害点が増える
- 初期導入障壁が高い

### 評価
**初期採用非推奨**。  
将来拡張としては有効だが、MVP では不要。

---

## 9.5 候補 E: GCC plugin / compiler internal integration

**概要**  
GCC plugin あるいは GCC 改造で診断を直接取得・制御する。

### 長所
- 取り得る情報量は多い
- long-term では強力

### 短所
- GCC バージョン依存が激しい
- plugin ライセンス制約がある
- 配布性が悪い
- multi-compiler 展開に不向き
- “既存ビルドフローに最小変更” から遠ざかる

### 評価
**不採用**。  
研究用途ならあり得るが、製品戦略としては悪手。

---

## 9.6 比較表

| 観点 | A 薄い text-wrapper | B IR モノリス | C library+CLI 二層 | D daemon 併用 | E GCC plugin |
|---|---:|---:|---:|---:|---:|
| 品質上限 | 2 | 4 | 5 | 4 | 4 |
| 拡張性 | 2 | 4 | 5 | 5 | 2 |
| 保守性 | 2 | 4 | 5 | 3 | 1 |
| Linux 配布性 | 5 | 4 | 4 | 2 | 1 |
| Clang 対応しやすさ | 2 | 4 | 5 | 4 | 1 |
| CI/IDE 展開 | 2 | 4 | 5 | 5 | 1 |
| 複雑さ | 2 | 3 | 4 | 5 | 5 |
| 実装リスク | 2 | 3 | 4 | 5 | 5 |
| 学習コスト | 2 | 3 | 4 | 5 | 5 |
| デバッグ容易性 | 3 | 4 | 5 | 2 | 1 |

## 9.7 推奨案

**候補 C** を推奨する。  
理由は以下。

1. **IR を中核に置かないと multi-compiler / CI / editor に伸びない**
2. **品質最優先なら、adapter / ranking / renderer の責務分離が必須**
3. **単一バイナリ配布は CLI front で達成でき、内部が library でも配布性は落ちない**
4. **将来の clang support を壊さない**

つまり、表面は wrapper-first だが、内部は **platform core** にすべきである。

---

# 10. 推奨アーキテクチャの詳細

## 10.1 全体像

```text
Build System / Developer CLI / CI
                |
                v
        +-------------------+
        | Wrapper CLI Front |
        | gcc-formed/g++... |
        +-------------------+
                |
                v
      +------------------------+
      | Invocation Controller  |
      | version detect / mode  |
      | single-pass or compat  |
      +------------------------+
                |
                v
      +------------------------+
      | Real Compiler Process  |
      | gcc / g++ / linker     |
      +------------------------+
         | structured sidecar | raw stderr/stdout
         v                    v
 +----------------+   +----------------------+
 | Backend Adapter|   | Raw Stream Capture   |
 | GCC SARIF      |   | linker text, traces  |
 +----------------+   +----------------------+
          \              /
           \            /
            v          v
        +----------------------+
        | Normalized Diag IR   |
        +----------------------+
                    |
                    v
        +----------------------+
        | Enrichment Pipeline  |
        | classify / rank /    |
        | summarize / compress |
        +----------------------+
           |          |          |
           v          v          v
 +----------------+ +----------------+ +----------------+
 | Terminal Render| | CI Text Render | | JSON/SARIF Out |
 +----------------+ +----------------+ +----------------+
                    |
                    v
        +----------------------+
        | Fallback / Passthru  |
        +----------------------+

Cross-cutting:
- Config / Policy
- Source Cache / Ownership Model
- Telemetry / Trace / Debug Bundle
- Corpus / Test Harness
```

## 10.2 コンポーネント責務

### A. compiler invocation layer
責務:
- argv 透過転送
- real compiler 発見
- compiler version 判定
- GCC tier 判定
- sidecar file の安全生成
- subprocess 実行
- exit code / signal 処理

重要設計:
- wrapper 独自フラグは compiler と衝突しない名前空間を使う
- shell 展開を避ける
- recursion guard を持つ
- response file は展開せず compiler に渡す

### B. backend adapter layer
責務:
- GCC SARIF / legacy JSON / raw stderr / linker patterns を backend-specific に解釈
- raw payload を落とさず IR 原料に変換

MVP:
- `GccSarifAdapter`
- `LinkerTextAdapter`
- `RawPassthroughAdapter`

将来:
- `ClangTextStructuredAdapter`
- `ClangSarifAdapter`（検証後）

### C. diagnostic ingestion
責務:
- sidecar file 読み込み
- parse budget 管理
- malformed input 耐性
- version skew 吸収

設計方針:
- SARIF/JSON は **streaming parse 優先**
- 失敗時は reason-coded fallback

### D. normalized diagnostic IR
責務:
- compiler 非依存の意味表現
- render に必要な意味情報の保持
- provenance 追跡

### E. enrichment / summarization / ranking
責務:
- root-cause 推定
- user-owned frame 選定
- include / macro / template chain 圧縮
- fix-it applicability 整理
- suppressed diagnostics 集約
- stable family classifier 付与

### F. rendering layer
責務:
- terminal human renderer
- CI plain renderer
- machine-readable emitter

重要:
- renderer は IR を読むだけ
- ranking/heuristic を持たない

### G. fallback / passthrough layer
責務:
- unsupported version
- parse failure
- timeout / memory budget exceed
- internal error
時の raw fallback

### H. config / policy layer
責務:
- config precedence
- mode 切替
- user/system header roots
- verbosity
- locale policy
- debug artifact retention
- org policy

推奨 precedence:
1. built-in defaults
2. system config
3. repo config
4. environment
5. CLI flags

### I. telemetry / debug / trace support
責務:
- decision trace
- fallback reason
- performance counters
- optional shadow records
- redaction

### J. test harness
責務:
- fixture compile orchestration
- GCC version matrix
- IR snapshot
- golden render
- corpus diff
- benchmark

---

# 11. 診断 IR（中間表現）の設計方針

## 11.1 IR の位置づけ

**SARIF を core IR にしてはいけない。**

理由:
- SARIF は交換形式として有用だが、UX ranking / compression / confidence / render sections にはそのままでは扱いにくい
- compiler ごとの差異を吸収するには内部 IR が必要
- renderer / classifier / regression test を安定させるには、製品側の正規形が要る

したがって、
- **Ingress**: SARIF / raw text / future structured streams
- **Core**: Normalized Diagnostic IR
- **Egress**: human text / CI text / IR JSON / optional SARIF
とする。

## 11.2 必須概念

### Diagnostic
- `id`
- `origin`（gcc / clang / linker / wrapper）
- `phase`（preprocess / parse / sema / instantiate / codegen / link / analyzer / driver）
- `severity`（fatal / error / warning / note / help）
- `message`
- `headline`（短い要約。enrichment 後）
- `code`（compiler code or wrapper family code）
- `option`（例: `-Wfoo`）
- `category`

### Span / Location
- `file`
- `line`
- `column_byte`
- `column_display`
- `end_line`
- `end_column_byte`
- `end_column_display`
- `label`
- `is_primary`
- `origin_kind`（caret / range / insertion / expansion / related）

**重要**: byte-based と display-based を分ける。  
compiler 間で列の意味が異なるため、正規化時に両方保持できる設計が望ましい。

### Hierarchy
- `children[]`
- `related[]`
- `notes[]`
- `helps[]`

**重要**: flatten しない。  
GCC/Clang の tree 構造を保持してから、表示の都合で圧縮する。

### Suggestions / Fixes
- `text`
- `edits[]`（half-open range）
- `applicability`
  - machine
  - likely
  - manual
  - unsafe
- `source`（compiler-native / heuristic / org-policy）

### Context Chains
- `include_chain[]`
- `macro_chain[]`
- `template_chain[]`
- `call_path[]`
- `link_chain[]`

### Linker Context
- `symbol_raw`
- `symbol_demangled`
- `reference_sites[]`
- `definition_sites[]`
- `archive_or_object`
- `linker_flavor`

### Provenance
- `backend_version`
- `ingress_format`
- `raw_index`
- `raw_payload_ref`
- `capture_artifact_ref`

### Ranking Metadata
- `root_cause_score`
- `confidence`
- `ranking_reasons[]`
- `suppressed_descendant_count`
- `user_code_priority`

### Ownership Metadata
- `location_owner`（user / vendor / system / generated / unknown）
- `ownership_reason`

## 11.3 重要な設計原則

### 原則 1: “message string” を唯一の真実にしない
message は必要だが、分類はそれに全面依存しない。  
span, child structure, option, phase, ownership, fixit も使う。

### 原則 2: tree を保持する
後で flatten はできるが、失われた tree は戻せない。

### 原則 3: compiler-specific extension を許す
`extensions.gcc`, `extensions.clang`, `extensions.linker` の escape hatch を許す。  
ただし renderer がそれに依存しないようにする。

### 原則 4: stable fingerprint を持つ
- raw fingerprint
- normalized fingerprint
- family fingerprint
を分ける。  
回帰テストと telemetry clustering に効く。

### 原則 5: confidence は first-class
root-cause や action hint は、確信度なしには dangerous。  
UX 上も示す。

---

# 12. 実装言語・ランタイム・配布方式の選定

## 12.1 比較方針

比較観点:
- Linux 配布容易性
- 単一バイナリ化
- 依存安定性
- 実行環境の壊れにくさ
- 文字列/JSON/CLI 処理
- 並行処理と将来拡張
- テスト容易性
- 長期保守性
- 開発速度
- 社内展開のしやすさ
- OSS エコシステム健全性

## 12.2 Rust

### 長所
- 単一バイナリ配布に強い
- 所有権と enum により IR モデル化に向く
- JSON / CLI / test tooling が非常に強い
- UB リスクが低く、長寿命 CLI に向く
- pattern matching が ranking/renderer 実装に効く
- reproducible build / dependency pinning がしやすい
- musl target を活用しやすい

### 短所
- コンパイル時間が長い
- チームに Rust 習熟が必要
- FFI が絡むとやや面倒

### 評価
**最有力**。  
このプロジェクトは「低レベル compiler internal」ではなく「高品質な構造化 CLI/IR 製品」であり、Rust と非常に相性が良い。

---

## 12.3 Go

### 長所
- ビルド・配布が簡単
- 実行環境が壊れにくい
- 単一バイナリ文化が強い
- 学習コストが比較的低い
- 並行処理は扱いやすい

### 短所
- rich IR を表す型表現は Rust より弱い
- exhaustiveness / ownership / immutability discipline が弱く、長期的に domain model がにじみやすい
- 複雑な render/state 変換で accidental complexity が出やすい

### 評価
**次点**。  
配布性は極めて良いが、このプロジェクトの核心である「診断 IR の厳密さ」と「変換パイプラインの安全性」で Rust に一歩劣る。

---

## 12.4 C++

### 長所
- compiler 周辺との文化的相性
- 実行性能
- 外部 runtime 不要
- 既存 compiler 開発者には馴染みがある

### 短所
- 依存管理が難しい
- 文字列/JSON/CLI/テストが比較的つらい
- UB / lifetime バグのコストが高い
- 長期保守で “製品レイヤ” より “システムレイヤ” の複雑さを招きやすい

### 評価
**不採用**。  
やろうと思えばできるが、このプロジェクトの勝ち筋ではない。

---

## 12.5 Python

### 長所
- 初速が速い
- プロトタイピングしやすい
- テキスト処理は書きやすい

### 短所
- runtime 依存が壊れやすい
- venv/pip/packaging が運用リスク
- 単一バイナリ化が不自然
- 実行環境を揃えるコストが高い
- 社内配布 CLI の長期品質要求と相性が悪い

### 評価
**製品本体には不採用**。  
ただし corpus tooling / analysis scripts / fixture generation には補助的に使ってよい。

---

## 12.6 総合比較表

| 観点 | Rust | Go | C++ | Python |
|---|---:|---:|---:|---:|
| Linux 配布容易性 | 5 | 5 | 3 | 2 |
| 単一バイナリ化 | 5 | 5 | 3 | 2 |
| 依存安定性 | 4 | 5 | 2 | 2 |
| 実行環境の壊れにくさ | 5 | 5 | 4 | 2 |
| 文字列/JSON/CLI 適性 | 5 | 4 | 2 | 5 |
| 並行処理/将来拡張 | 4 | 4 | 3 | 3 |
| テスト容易性 | 5 | 4 | 3 | 4 |
| 長期保守性 | 5 | 4 | 2 | 2 |
| 開発速度 | 4 | 5 | 2 | 5 |
| 社内展開しやすさ | 5 | 5 | 3 | 2 |
| 総合 | **47** | 46 | 27 | 29 |

## 12.7 最終推奨

**Rust を推奨する。**

理由:
1. **IR 中心設計に最も向く**
2. **単一バイナリ + no runtime で Linux 配布に強い**
3. **長期保守で安全性が効く**
4. **テスト基盤が作りやすい**
5. **将来の multi-renderer / multi-adapter に耐える**

> 結論:  
> **製品本体は Rust。**  
> Python は補助ツールに限定。Go は fallback 候補だが第一選択ではない。

---

# 13. パッケージング・配布・運用方針

## 13.1 Linux first の最適解

### 推奨
- **単一バイナリ配布**
- 可能なら **musl ターゲット**
- 同梱物は最小
  - 実行ファイル
  - symlink / wrapper alias
  - sample config
  - license manifest

## 13.2 配布形態

### 初期
- internal artifact repository に tarball 配布
- 例:
  - `cc-formed-linux-x86_64.tar.gz`
  - `cc-formed-linux-aarch64.tar.gz`

### 併設
- repo-local toolchain dir へ展開できる
- `bin/gcc`, `bin/g++` symlink を提供できる

### 将来
- deb / rpm
- Homebrew / asdf（社外/OSS を見据えるなら）
- container image for CI

## 13.3 バージョン固定方針
- semantic versioning
- compiler support matrix を release note に明記
- config schema version を分離
- IR schema version を分離

## 13.4 再現可能ビルド
- lockfile commit
- dependency vendor
- deterministic release pipeline
- SBOM 生成
- release artifact hash 公開（社内でも実施）

## 13.5 導入方法の優先順位

### 第一候補
- `CC=/path/to/gcc-formed`
- `CXX=/path/to/g++-formed`

### 第二候補
- toolchain directory を PATH 先頭へ

### 第三候補
- build-system launcher 経由（必要な場合）

## 13.6 運用ポリシー
- stable / beta / canary channel
- rollback はバイナリ差し替えだけで可能
- debug bundle は opt-in or CI artifact

---

# 14. 品質保証戦略

このプロジェクトの成否は **QA の設計** でほぼ決まる。  
UI を作る前に **診断コーパス** を作るべきである。

## 14.1 テスト戦略の全体像

### レイヤ 1: unit tests
- version detection
- path resolution
- ownership classification
- ranking rules
- suppression rules
- snippet extraction
- ANSI sanitization

### レイヤ 2: adapter contract tests
入力:
- GCC SARIF fixture
- raw stderr fixture
- linker stderr fixture

出力:
- normalized IR snapshot

目的:
- compiler version 差異を adapter に閉じ込める

### レイヤ 3: renderer golden tests
IR を固定し、出力文字列を snapshot で比較する。
- terminal colored
- terminal no-color
- CI plain
- concise / verbose

### レイヤ 4: end-to-end real compiler tests
実際に GCC を起動して fixture source をコンパイルし、
- exit code
- sidecar capture
- IR
- render
- fallback behavior
を検証する。

### レイヤ 5: corpus regression tests
実ケース由来の再現を継続保持する。  
回帰の中心はここ。

## 14.2 ゴールデンテスト
ゴールデンは 3 種類持つ。
1. raw ingress
2. normalized IR
3. final render

これにより、  
- adapter の変更
- ranking の変更
- renderer の変更
を切り分けられる。

## 14.3 スナップショットテスト
- `*.ir.json`
- `*.rendered.txt`
- `*.ci.txt`
を snapshot 管理。  
差分レビュー時に UX 変更を可視化する。

## 14.4 実コンパイラ出力コーパス
コーパスは以下の軸で収集する。
- C / C++
- parser / sema / template / linker
- small / medium / pathological
- user-only / system-header-heavy
- GCC major versions

### コーパスのソース
- 手書き fixture
- 社内 shadow mode の sanitized bundle
- upstream inspired repro（ライセンス確認済み or 自前再作成）

## 14.5 fuzz / robustness
対象:
- malformed SARIF
- truncated JSON
- invalid UTF-8
- 巨大 note tree
- 巨大テンプレート型文字列
- 悪意ある escape sequence を含む source line

目的:
- crash しない
- fallback できる
- terminal injection しない

## 14.6 UX 品質評価
- task-based user study
- subjective helpfulness
- root-cause identification
- mislead case review

### 特に重要
**「役に立たないが見た目は綺麗」** を防ぐ。  
毎 release で top diagnostic families を人間がレビューする。

## 14.7 互換性テスト
- GCC 13 latest patch
- GCC 14 latest patch
- GCC 15 latest patch
- 主要 distro matrix
- TTY / non-TTY
- narrow/wide terminal
- locale variants（MVP では fallback / reduced mode の検証）

## 14.8 パフォーマンスベンチマーク
シナリオ:
- success path no diagnostics
- one syntax error
- template explosion
- linker undefined reference
- 100+ diagnostics
- CI non-TTY

計測:
- wrapper overhead
- parse time
- ranking time
- render time
- peak RSS
- temp artifact I/O

## 14.9 将来の clang 対応を壊さないテスト設計
- adapter contract test を backend-generic にする
- same semantic scenario を GCC/Clang 両方で fixture 化
- IR の core field expectation を共有
- backend-specific extension は別 snapshot に隔離

## 14.10 出荷ゲート
少なくとも以下を満たさない限り org-wide rollout しない。
- P0 fidelity bug = 0
- fallback unexpected rate < 0.1%
- target corpus pass 100%
- performance budget pass
- user study で raw GCC より悪化しない
- linker common cases pass threshold

---

# 15. ライセンスと依存ポリシー

## 15.1 基本方針
- 有償ライセンス禁止
- closed dependency 禁止
- 将来 OSS 化を阻害しない
- permissive license 中心

## 15.2 許可ライセンス（推奨 allowlist）
- Apache-2.0
- MIT
- BSD-2-Clause / BSD-3-Clause
- ISC
- Zlib
- Unicode DFS / data license 類

## 15.3 原則として避ける
- GPL / AGPL / SSPL
- copyleft がリンク境界に影響するもの
- ライセンス表記が曖昧な小規模ライブラリ
- メンテ不能な abandonware

## 15.4 GCC / LLVM / Python との関係
- GCC は **外部プロセスとして呼ぶ**
- GCC plugin は使わない
- LLVM/Clang を将来リンクする場合でも、コア設計は process boundary 前提で保つ
- Python runtime は product 必須依存にしない

## 15.5 依存ポリシー運用
- lockfile 固定
- vendor
- license scan を CI 必須
- SBOM 生成
- transitive 依存レビュー
- 新依存は ADR または dependency review で承認

---

# 16. リスク登録簿

重要度順に記載する。

| # | リスク | 重要度 | 内容 | 軽減策 |
|---|---|---|---|---|
| 1 | 誤った root-cause 推定 | Critical | 一番危険。速くても誤誘導なら失敗 | confidence 制御 / provenance / high-confidence threshold / corpus review |
| 2 | GCC バージョン差異 | Critical | SARIF/JSON/挙動差分で壊れる | support tier 明文化 / adapter 分離 / multi-version matrix |
| 3 | 情報圧縮で事実を失う | Critical | UX 向上のつもりが重要 note を隠す | “compress, don’t discard” 原則 / verbose/raw path / provenance |
| 4 | linker 診断の非一貫性 | High | 構造化しにくい | dedicated text adapter / limited-scope MVP / raw append |
| 5 | C++ template 複雑性 | High | ranking が外しやすい | first mismatch only / user frame prioritization / conservative confidence |
| 6 | build system / launcher 相性 | High | ccache, wrappers, response files など | transparent argv / replay compatibility mode / rollout canary |
| 7 | portability 崩壊 | High | ランタイム依存や glibc 問題 | single binary / musl / no Python runtime |
| 8 | 実装言語選定ミス | High | 長期で開発速度 or 保守性を失う | Rust 採用 / small dependency surface / periodic architecture review |
| 9 | CI ログで劣化 | Medium-High | ローカルでは良いが CI で読みにくい | dedicated CI renderer / no ANSI dependence |
| 10 | 多コンパイラ拡張で設計破綻 | Medium-High | GCC 前提の field を IR に埋め込む | compiler-agnostic core + backend extensions |
| 11 | locale 依存 | Medium | 非英語診断で classifier が崩れる | English-first support policy / reduced mode / locale ADR |
| 12 | temp artifact 漏れ / I/O 負荷 | Medium | sidecar 運用が雑だと壊れる | secure temp dir / cleanup / keep-on-debug only |
| 13 | 導入障壁が想定より高い | Medium | CC/CXX 差し替えでも嫌がられる | shadow mode / opt-in rollout / wrapper-first simplicity |
| 14 | aesthetic over-engineering | Medium | 見た目ばかり良くして本質が進まない | KPI を修正速度中心に固定 |
| 15 | libgdiagnostics / plugin に引っ張られる | Medium | 依存性が重くなる | production dependency にしない |
| 16 | telemetry が機微情報を漏らす | Medium | パスやコード断片の漏えい | default off / redaction / local-only trace |
| 17 | 成功 path overhead | Medium | 毎コンパイルで遅い | single-pass optimization / benchmark gate |
| 18 | corpus 偏り | Medium | 手元では良いが現場で外す | shadow harvest + periodic recuration |

---

# 17. 開発フェーズとロードマップ

## Phase 0: Discovery / Architecture Validation

### 目的
- 技術的成立性を確認する
- structured-first 路線を検証する
- support tier を固める

### 成果物
- architecture spike
- GCC 13/14/15 structured ingress 検証
- seed corpus 50〜100 件
- ADR 初版
- language decision
- packaging spike

### 何を決めるか
- wrapper-first を採るか
- SARIF ingest を core にできるか
- GCC support tier
- Rust 採用
- fallback 方針

### 何を捨てるか
- text parsing first
- plugin first
- IDE first

### Done 条件
- 主要 3 ケース（syntax/type/template）で structured ingest → IR → mock render が成立
- fallback story が成立
- build/packaging spike が成立

---

## Phase 1: GCC-first MVP

### 目的
- 日常の compile failure で使える最小製品を出す

### 成果物
- wrapper CLI
- GCC 15 single-pass path
- GCC 13–14 compatibility path
- IR v1alpha
- terminal renderer v1
- CI renderer v1
- safe fallback
- trace bundle
- corpus harness

### 何を決めるか
- config precedence
- render information architecture
- top 5 diagnostic family rules
- raw/verbose policy

### 何を捨てるか
- editor integration
- daemon
- clang
- advanced linker intelligence

### Done 条件
- target corpus pass
- local developer opt-in で使える
- no P0 fidelity bug
- basic rollout documentation 完成

---

## Phase 2: Hardening / Rollout Readiness

### 目的
- 社内配布可能な品質に上げる

### 成果物
- performance hardening
- fallback / telemetry hardening
- shadow mode
- version matrix CI
- packaging/release pipeline
- rollout playbook

### 何を決めるか
- support SLA
- release channels
- trace retention policy
- minimum acceptable helpfulness KPI

### 何を捨てるか
- 低頻度 exotic diagnostics の深追い

### Done 条件
- canary deployment 成功
- p95 budget 達成
- fallback unexpected rate 目標達成
- support handoff 可能

---

## Phase 3: Advanced Diagnostics / Ranking / Summarization

### 目的
- 真に “Rust/Haskell 以上” を狙う UX に踏み込む

### 成果物
- advanced template summarizer
- overload candidate grouping
- richer macro/include condensation
- linker family classifier 強化
- stable family IDs
- knowledge-base hooks

### 何を決めるか
- confidence presentation policy
- family ID taxonomy
- org-doc link policy

### 何を捨てるか
- 汎用 AI explanation の導入

### Done 条件
- template-heavy corpus で raw GCC を大きく上回る
- user study で理解率改善が明確

---

## Phase 4: Clang Support

### 目的
- architecture の multi-compiler 拡張性を実証する

### 成果物
- Clang adapter
- GCC/Clang shared IR contract tests
- renderer backend neutrality validation

### 何を決めるか
- Clang support tier
- cross-backend field mapping
- backend-specific extension policy

### 何を捨てるか
- Clang-only UX を core に混ぜること

### Done 条件
- semantic fixture parity
- GCC regression なし

---

## Phase 5: Editor / Machine Integration

### 目的
- same core を IDE/CI artifact に広げる

### 成果物
- IR JSON schema v1
- editor bridge / CLI render from saved bundle
- enriched SARIF export
- compile_commands based workflows

### 何を決めるか
- public machine schema
- stability promises
- extension points

### Done 条件
- editor side proof-of-value
- CI artifact integration 完成

---

# 18. 具体的な UX モック

以下は簡略モックであり、実際の wording は corpus と user test で調整する。

---

## 18.1 単純な構文エラー

### 現状の gcc 的な見え方（簡略）
```text
foo.c:12:12: error: expected ';' before '}' token
   12 |   return x
      |            ^
      |            ;
   13 | }
      | ~
```

### 理想的な見え方
```text
error: return 文の末尾に ';' がありません
--> foo.c:12:12

Root cause
  12 行目の文が閉じていないため、次の '}' は追従エラーです。

Action
  `return x;` に修正してください。

Context
  12 |   return x
     |            ^ insert ';'

Suppressed
  follow-on diagnostics: 1
```

---

## 18.2 型不一致

### 現状の gcc 的な見え方（簡略）
```text
foo.c:27:24: warning: passing argument 1 of 'takes_int_ptr' from incompatible pointer type [-Wincompatible-pointer-types]
   27 |   takes_int_ptr(name);
      |                        ^~~~
      |                        |
      |                        char *
foo.h:4:25: note: expected 'int *' but argument is of type 'char *'
```

### 理想的な見え方
```text
error: 引数 1 の型が合っていません
--> foo.c:27:17
callee: takes_int_ptr(int *)

Root cause
  `char *` を `int *` として渡しています。

Action
  次のどちらかです:
    1. 呼び出し側を `int *` にする
    2. callee のシグネチャが誤っているなら定義を見直す

Type mismatch
  expected: int *
  actual:   char *

Context
  27 |   takes_int_ptr(name);
     |                 ^^^^ char *
```

---

## 18.3 C++ テンプレート絡み

### 現状の gcc 的な見え方（簡略）
```text
error: no matching function for call to 'consume(std::map<int, std::vector<double> >&)'
note: candidate: 'void consume(const std::map<int, std::vector<float> >&)'
note:   no known conversion for argument 1 from 'std::map<int, std::vector<double> >'
        to 'const std::map<int, std::vector<float> >&'
note:   in instantiation of ...
note:   required from ...
note:   required from ...
... (大量)
```

### 理想的な見え方
```text
error: `consume` が受け取る map の value_type が一致していません
--> app.cpp:41:13
call: consume(points)

Root cause
  `consume` は `std::vector<float>` を持つ map を要求していますが、
  実際には `std::vector<double>` を持つ map を渡しています。

First mismatch
  std::map<int, std::vector<
-   float
+   double
  >>

Most actionable frame
  app.cpp:41:13   consume(points)

Instantiation summary
  total frames: 7
  user frames: 1
  library frames suppressed: 6
  use --verbose to expand
```

---

## 18.4 include / macro 連鎖

### 現状の gcc 的な見え方（簡略）
```text
In file included from app.c:1:
In file included from project/api.h:4:
include/lib.h:42:18: error: 'struct config' has no member named 'colour'; did you mean 'color'?
app.c:8:10: note: in expansion of macro 'GET_COLOUR'
```

### 理想的な見え方
```text
error: マクロ `GET_COLOUR` が存在しないメンバ `colour` を参照しています
--> include/lib.h:42:18
user use-site: app.c:8:10

Root cause
  `GET_COLOUR(cfg)` の展開先が `cfg->colour` を参照していますが、
  構造体には `color` しかありません。

Action
  高確率の修正:
    `colour` -> `color`

Where this came from
  app.c:1          includes project/api.h
  project/api.h:4 includes include/lib.h
  app.c:8:10       expands GET_COLOUR(cfg)

Context
  include/lib.h:42 |   ((cfg)->colour)
                    |          ^^^^^^ did you mean 'color'?
```

---

## 18.5 linker エラー

### 現状の gcc 的な見え方（簡略）
```text
/usr/bin/ld: main.o: in function `main':
main.cpp:(.text+0x2a): undefined reference to `foo(int)'
collect2: error: ld returned 1 exit status
```

### 理想的な見え方
```text
link error: `foo(int)` の定義が見つかりません
phase: link

Unresolved symbol
  foo(int)

Referenced from
  main.cpp:14  main()

Most likely causes
  1. `foo.cpp` / `libfoo.a` / `-lfoo` がリンク入力に入っていない
  2. 宣言と定義のシグネチャが一致していない
  3. C/C++ リンケージ (`extern "C"`) が一致していない

Suggested checks
  - build rule に対象 object/library が入っているか
  - `nm -C` で定義シンボル名を確認
  - ヘッダ宣言と実装の引数型・namespace を確認

Raw linker output
  /usr/bin/ld: main.o: ... undefined reference to `foo(int)'
```

---

# 19. 最初の意思決定セット（ADR 候補）

以下はプロジェクト開始時に必ず書くべき ADR である。

1. **ADR-001: wrapper-first を採るか**
2. **ADR-002: SARIF/JSON を ingress に使うか**
3. **ADR-003: SARIF を core IR にしない**
4. **ADR-004: official support を GCC 15 first にする**
5. **ADR-005: GCC 13–14 compatibility tier をどう扱うか**
6. **ADR-006: fallback は fail-open にする**
7. **ADR-007: implementation language は Rust**
8. **ADR-008: single binary / musl 優先配布**
9. **ADR-009: library + CLI 二層構成**
10. **ADR-010: deterministic rule engine を採用し、AI をコアにしない**
11. **ADR-011: locale policy（English-first / reduced mode / fallback）**
12. **ADR-012: machine-readable output は native IR JSON を canonical とする**
13. **ADR-013: SARIF egress の範囲（raw pass-through か enriched export か）**
14. **ADR-014: linker diagnostics を text adapter で段階的に扱う**
15. **ADR-015: source ownership model（user/vendor/system/generated）の定義**
16. **ADR-016: trace bundle の内容と redaction policy**
17. **ADR-017: dependency allowlist / license policy**
18. **ADR-018: corpus governance（fixture 追加・sanitize・review プロセス）**
19. **ADR-019: render modes（concise/default/verbose/raw）**
20. **ADR-020: stability promises（CLI / config / IR schema）**

---

# 20. 最終提案

## 20.1 推奨アーキテクチャ
**IR-centered, adapter-separated, wrapper-first, library+CLI 二層構成** を推奨する。

具体的には:
- 表面は gcc/g++ 互換 wrapper
- 中核は compiler-agnostic Diagnostic IR
- GCC 15 では SARIF sidecar を single-pass で取得
- GCC 13–14 は compatibility path
- linker は raw stderr text adapter
- renderer は terminal/CI/machine を分離
- fail-open fallback を徹底

## 20.2 推奨実装言語
**Rust**

## 20.3 最初の 90 日でやるべきこと

### 0–30 日
- ADR 決定
- seed corpus 構築
- GCC 13/14/15 spike
- wrapper / fallback spike
- Rust packaging spike

### 31–60 日
- IR v1alpha
- GCC adapter
- terminal/CI renderer v1
- top 3 diagnostic families（syntax, type, basic template）
- golden/snapshot harness

### 61–90 日
- include/macro/linker basic support
- trace bundle
- performance hardening
- canary-ready package
- shadow mode for internal rollout

## 20.4 最初に切り捨てるべきもの
- IDE プラグイン
- daemon
- clang support
- AI explanation
- auto-fix apply
- exotic linker cases の深追い
- full localization

## 20.5 このプロジェクトを失敗させやすい判断

1. **text parsing を主戦略にすること**
2. **raw fallback を甘くすること**
3. **IR を作らず renderer から始めること**
4. **clang / IDE / daemon を最初から抱えること**
5. **Python runtime を本体依存にすること**
6. **quality harness より先に見た目を作ること**
7. **confidence を出さずに断定的 root cause を出すこと**
8. **compiler truth を削って“分かりやすさ”を優先すること**

## 20.6 このプロジェクトを成功させるための最重要原則

> **Summarize aggressively, infer conservatively.**  
> **圧縮は大胆に、推定は保守的に。元の compiler truth は絶対に失わない。**

この原則を守れば、Rust/Haskell を超える UX は狙える。  
守れなければ、ただの “pretty stderr” で終わる。

---

# 付録 A: 追加の具体提案

## A.1 製品名
長期的には `cc-formed` を推奨。  
ただし導入障壁を下げるため、初期は以下の alias を提供する。
- `gcc-formed`
- `g++-formed`

## A.2 レンダリングの情報設計
すべての human-facing error は原則として以下の構造を持つ。

1. Headline
2. Root cause
3. Action
4. Context
5. Where this came from
6. Suppressed details
7. Raw / verbose expansion entry point

## A.3 confidence の表示規則
- High: `Root cause`
- Medium: `Likely root cause`
- Low: 根本原因断定を出さず raw facts のみ

## A.4 support tier
- Tier 1: GCC 15.x Linux
- Tier 2: GCC 13–14 Linux
- Tier 3: passthrough only
- Future: Clang tiered support

---

# 付録 B: 参考外部事実の要点

- GCC 13 で SARIF 診断出力が追加された。  
- GCC 15 で `-fdiagnostics-add-output=` による複数同時出力が追加され、legacy JSON は deprecated になった。  
- GCC 15 の SARIF は include chain や複数 location をよりよく保持する。  
- GCC には parseable fixits / template tree / path formatting などの診断素材がある。  
- `libgdiagnostics` は存在するが、shared library としての optional build であり、production dependency にするには不向き。  
- GCC plugin は GPL-compatible license が必要。  
- Clang は parseable fixits と source-range info を公開オプションとして持つ。  
- Rust は `help` と `note` を役割分離し、suggestion applicability を持つ。  
- GHC は JSON diagnostics と error index links を持つ。  
- Python は virtual environment 前提の運用が基本で、CLI 配布の壊れやすさ要因になりやすい。  

---

# 付録 C: 参考ソース（公式中心）

1. GCC 13 Release Series — Changes, New Features, and Fixes  
   https://gcc.gnu.org/gcc-13/changes.html

2. GCC 15 Release Series — Changes, New Features, and Fixes  
   https://gcc.gnu.org/gcc-15/changes.html

3. GCC Diagnostic Message Formatting Options  
   https://gcc.gnu.org/onlinedocs/gcc-15.2.0/gcc/Diagnostic-Message-Formatting-Options.html

4. libgdiagnostics documentation  
   https://gcc.gnu.org/onlinedocs/libgdiagnostics/

5. GCC configure docs (`--enable-libgdiagnostics`)  
   https://gcc.gnu.org/install/configure.html

6. GCC Plugin API (plugin license check)  
   https://gcc.gnu.org/onlinedocs/gccint/Plugin-API.html

7. Clang User’s Manual (Formatting of Diagnostics)  
   https://clang.llvm.org/docs/UsersManual.html

8. Clang Command Line Reference  
   https://clang.llvm.org/docs/ClangCommandLineReference.html

9. Clang TextDiagnostic / SARIFDiagnosticPrinter docs  
   https://clang.llvm.org/doxygen/classclang_1_1TextDiagnostic.html  
   https://clang.llvm.org/doxygen/SARIFDiagnosticPrinter_8h_source.html

10. Rust Compiler Development Guide — Errors and lints  
    https://rustc-dev-guide.rust-lang.org/diagnostics.html

11. Rust platform support / linkage docs  
    https://doc.rust-lang.org/rustc/platform-support.html  
    https://doc.rust-lang.org/reference/linkage.html

12. GHC User’s Guide / Haskell Error Index  
    https://ghc.gitlab.haskell.org/ghc/doc/users_guide/using.html  
    https://errors.haskell.org/index.html

13. Python docs — virtual environments  
    https://docs.python.org/3/tutorial/venv.html

14. LLVM license  
    https://llvm.org/LICENSE.txt

15. Go docs — build / cgo  
    https://go.dev/doc/tutorial/compile-install  
    https://pkg.go.dev/cmd/cgo
