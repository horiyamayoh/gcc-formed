# gcc-formed を release-ready にするための具体的な改善優先順位

作成日: 2026-04-08  
対象: `horiyamayoh/gcc-formed` の最新版 `main`

## 0. 結論

このリポジトリは、**設計思想・品質ゲート・配布/rollback 設計の筋がかなり良い**です。  
ただし、最新版の `main` はまだ **「一般公開向けに安心して出荷する状態」ではありません**。

最大の理由は、単に CI が赤いからではなく、次の 4 点が同時に残っているためです。

1. **release gate 自体が安定していない**
2. **最初の出荷スコープがまだ十分に固定されていない**
3. **診断品質のコア（enrich / select / render）の妥当性が corpus ベースで十分に保証されていない**
4. **public beta release path、RC gate automation、RC metrics packet、fuzz / adversarial hardening、human evaluation kit、compatibility path の honest UX、stable release automation、support / incident / rollback runbooks、governance freeze は 2026-04-09 時点で `main` に反映済みで、現在の playbook に残る user-visible work package はない**

したがって、優先順位は **「見た目の改善」より「出荷失敗確率を下げる改善」** に寄せるべきです。

---

## 1. 優先順位の全体像

| 優先度 | 項目 | 目的 | リリース判定への効き方 |
|---|---|---|---|
| P0 | CI / release gate を安定化して main を常時 green にする | 出荷判定の信頼性を作る | **必須** |
| P0 | 初回リリースの対象範囲を明文化して凍結する | 品質責務を絞る | **必須** |
| P0 | GCC 15 向け corpus の「結果品質」ゲートを追加する | 製品コアの妥当性を担保する | **必須** |
| P1 | `xtask` を分割して release/install 系を独立させる | 出荷系変更の事故率を下げる | 強く推奨 |
| P1 | CLI orchestration を分離して可読性と変更容易性を上げる | 今後の機能追加を安全にする | 強く推奨 |
| P1 | diagnostics の enrich/select/render を rule-based に昇格する | “効く診断 UX” を作る | 強く推奨 |
| P2 | supply chain / GitHub Actions / provenance を締める | 公開配布の信頼性を上げる | 推奨 |
| P2 | 公開リリース用ドキュメントと運用手順を整える | ユーザーが使える状態にする | 推奨 |

---

## 2. P0: これが終わるまで「リリースしない」項目

### P0-1. CI / release gate を安定化し、`main` を常時 green にする

## なぜ最優先か

現在の repo は quality gate をかなり真面目に持っていますが、**その gate 自体が不安定だと「通ったから出してよい」が成立しません**。  
release-ready の最初の条件は「品質が高いこと」ではなく、**品質を判定する仕組みが信頼できること**です。

## 現状から見える問題

- `pr-gate` の push run が失敗している
- `nightly-gate` も失敗している
- `nightly-gate #6` は `gcc:15` job が exit code 1 で落ちている
- workflow は `dtolnay/rust-toolchain@master` を参照しており、CI の再現性を外部の `master` に依存している
- GitHub Actions では `actions/checkout@v4` に対して Node.js 20 deprecation warning が出ている
- 直近の commit 群が snapshot drift / transient path / quote style normalization に集中しており、**テストが本質差分より環境差分に引きずられている**

## 具体的にやること

1. **workflow action を固定する**
   - `dtolnay/rust-toolchain@master` をやめる
   - immutable な tag か commit SHA に pin する
   - `actions/checkout` も Node 24 対応版へ上げる

2. **落ちる step を即断できるようにする**
   - `cargo xtask check`
   - `cargo xtask replay`
   - `cargo xtask snapshot`
   - hermetic build
   - package / install / uninstall
   - release-publish / resolve / install-release
   をそれぞれ別 step / 可能なら別 job に分割し、どこで壊れたかを 1 画面で分かるようにする

3. **snapshot drift 対策を「その場しのぎ」から contract 化へ移す**
   - 行番号、引用符、object path、tool version、time などの揺れるフィールドを仕様として整理する
   - 「何を normalize してよく、何は差分として扱うか」を `diag_core` の snapshot normalization contract に一本化する
   - snapshot check 側で ad hoc に吸収しない

4. **失敗時 artifact を必ず残す**
   - normalized IR
   - raw stderr
   - rendered output
   - failing snapshot diff
   - build manifest
   を artifact として保存し、再現調査を 1 run で完結できるようにする

5. **`gcc:15` を release blocker、`gcc:13/14` を health indicator に分離する**
   - 初回出荷では `gcc:15` の green を blocker にする
   - `gcc:13/14` は nightly で警戒監視しつつ、赤でも出荷を止めない設計にする
   - ただし regression は issue 化する

## 完了条件 (Definition of Done)

- `main` で `pr-gate` が連続 10 回以上 green
- `nightly-gate` が連続 3 日以上 green、または `gcc:15` blocker 部分が連続 3 日以上 green
- workflow の action 参照が `master` を含まない
- CI 失敗時に、再現に必要な artifact が必ず保存される

---

### P0-2. 初回リリースの対象範囲を明文化し、そこだけを release criteria にする

## なぜ最優先か

release-ready を達成する最短経路は、**「何をまだ約束しないか」を明確にすること**です。  
この repo は support tier の考え方自体は良いので、ここをさらに一段 concretize すべきです。

## 提案する初回出荷スコープ

**初回の一般公開は以下に限定するのが妥当です。**

- Linux first
- `x86_64-unknown-linux-musl` を primary artifact とする
- GCC 15 を primary support とする
- terminal renderer を primary surface とする
- GCC 13/14 は **passthrough / compatibility support** と明記する
- `shadow` や trace bundle は残してよいが、**改善品質を保証するのは GCC 15 render path のみ**とする

## 具体的にやること

1. README と release notes に、**first release scope / non-goals / known limits** を明記する
2. CLI が tier 判定時に、非 primary path では「保守的 fallback である」ことを明示する
3. release checklist に「この release は何を保証しないか」を入れる
4. issue / PR テンプレートに support tier を入れる

## 完了条件

- README に first release scope が 1 画面で分かる形で書かれている
- `RELEASE-NOTES.md` に既知制約がある
- GCC 13/14 で enhanced UX を期待させる表現が残っていない

---

### P0-3. GCC 15 corpus に対する「結果品質」ゲートを入れる

## なぜ最優先か

CI が green でも、**出力が役に立たなければ製品としては未完成**です。  
この repo の本体価値は formatter そのものではなく、**raw stderr より意思決定しやすい診断表現を返すこと**にあります。

この項目は **2026-04-09 時点で `main` に反映済み**です。representative GCC 15 corpus に対して family / lead location / first action / fallback expectation の gate が入り、`diag_render` も low-confidence lead の 2 件目 group 展開、partial document の mixed fallback、CI profile の `<temp-object>` 正規化まで反映されました。

## 現状から見える問題

- representative corpus で improved path と raw fallback の境界は固定されたが、一般公開 release に必要な artifact / notes / install story はまだ未公開である
- long-tail corpus の拡張と nightly health indicator は引き続き必要だが、core renderer contract 自体は representative set で固定済みである
- snapshot の整合性はかなり改善したため、次の重点は output quality そのものより release artifact / workflow 証跡の公開に移っている

## 具体的にやること

1. **代表ケースごとの acceptance contract を作る**
   - syntax
   - macro/include
   - template
   - type / overload
   - linker
   の 5 family について、最低 3 ケースずつ representative corpus を決める

2. **結果品質の gate を追加する**
   例:
   - primary user-owned location が 1 件は出る
   - headline が raw message の丸写しだけにならない
   - first action が空でない
   - passthrough に落ちた率が閾値以下
   - 誤 family 分類率が閾値以下

3. **“raw より良い” を測る観点を固定する**
   - 先頭に user-owned location が来るか
   - note の洪水を抑えられているか
   - macro/include/template 文脈を捨てずに圧縮できているか
   - verbose と default の情報密度差が説明可能か

4. **negative corpus を入れる**
   - 誤分類しやすい文言
   - vendor path / generated path / user path の誤 ownership 判定
   - multi-error / warning 混在ケース
   - broken SARIF / partial SARIF / no SARIF ケース

## 完了条件

- representative GCC 15 corpus について family ごとの acceptance test がある
- fallback 率が数値で見える
- release candidate ごとに「どのケースで raw より改善したか / していないか」が説明できる

---

## 3. P1: P0 の次に着手すべき項目

### P1-1. `xtask` を分割し、release/install 系を独立モジュールまたは crate に切り出す

## なぜ優先度が高いか

この項目は **2026-04-09 時点で `main` に反映済み**です。
`xtask/src/main.rs` は dispatch-oriented CLI shell（602 lines）として保たれ、release/install/vendor/release-repo cluster は `xtask/src/commands/release.rs`、replay/snapshot/acceptance report は `xtask/src/commands/corpus.rs`、共有 helper は `xtask/src/util/`、tests は `xtask/src/tests.rs` に分離されました。その上で `main` には `xtask/src/commands/rc_gate.rs`、`xtask/src/commands/fuzz.rs`、`xtask/src/commands/human_eval.rs`、`xtask/src/commands/stable.rs`、`xtask/src/commands/check.rs` が追加され、`cargo xtask rc-gate` / `bench-smoke` / `fuzz-smoke` / `human-eval-kit` / `stable-release` / `metrics-report.json` / `human-eval/human-eval-report.json` を含む machine-readable release evidence が動くようになっています。`cargo xtask check` も Python の `ci/test_*.py` contract suite を含むようになり、compatibility path の honest UX、stable release automation、support / incident / rollback runbooks、governance freeze まで含む drift を同じ標準 gate で止められるため、現在の playbook に残る user-visible work package はありません。

`xtask/src/main.rs` は **5415 lines / 183 KB** あり、現在の release engineering, packaging, install, rollback, vendor, hermetic build, repository promotion まで一箇所に集中しています。  
設計意図は良い一方で、**出荷系ロジックの変更が巨大ファイルに集約されると、直前修正の事故率が上がります**。

## 具体的にやること

1. command ごとに module を分ける
   - `check`
   - `replay`
   - `snapshot`
   - `package`
   - `install`
   - `rollback`
   - `release_repo`
   - `vendor`

2. shared utility を library 化する
   - archive / checksum / signature
   - file layout
   - symlink switch
   - repository metadata read/write
   - process execution helper

3. `xtask` を「CLI shell」にして、実体ロジックを library に寄せる

4. filesystem mutation を dry-run 可能にする
   - install / rollback / uninstall を dry-run で検証できるようにする

## 完了条件

- `xtask/src/main.rs` が 1000 lines を大きく下回る
- install / rollback / release-publish のロジックが unit test 可能になる
- release-related PR で diff review が command 単位で追える

---

### P1-2. `diag_cli_front` の責務を分割する

## なぜ優先度が高いか

この項目は **2026-04-09 時点で `main` に反映済み**です。  
`diag_cli_front/src/main.rs` は thin entrypoint（172 lines）へ縮小され、CLI parsing、config merge、mode selection、execution planning、trace/self-check が別 module に分離されました。さらに `--formed-self-check` は canonical rollout matrix と exact compatibility notices も返すようになり、RC gate が wrapper policy drift を自動検知できます。human evaluation packet と stable release packet も `cargo xtask human-eval-kit` / `cargo xtask stable-release` と `rc-gate/human-eval/` / `stable-release/` に固定され、support / incident / rollback runbooks も docs と bug template へ張られ、`GOVERNANCE.md` / `ADR-0020` / PR template で change classification と pre-/post-`1.0.0` backlog 分離も固定されたため、現在の playbook に残る user-visible work package はありません。

## 具体的にやること

1. 次の責務に分割する
   - argument parsing
   - execution plan construction
   - backend/tier resolution
   - trace/session setup
   - rendering policy selection
   - introspection commands (`--formed-version`, `--formed-self-check` など)

2. `ExecutionPlan` のような中間表現を作る
   - 入力: argv + env + probe result
   - 出力: passthrough / shadow / render の実行計画

3. side effect を境界に寄せる
   - process spawn
   - file write
   - stderr/stdout 出力
   - trace bundle 書き込み

4. CLI test を拡充する
   - tier conflict
   - unsupported compiler
   - self-check failure
   - trace path permission error
   - fallback path

## 完了条件

- `main.rs` は thin entrypoint になる
- 実行モード分岐が pure function ベースでテストできる
- release 前の挙動変更が狭い diff で済む

---

### P1-3. enrich / select / render を「文字列ヒューリスティックの束」から rule engine に昇格する

`diag_enrich` と `diag_render` の hardening は **2026-04-09 時点で `main` に反映済み**です。context chain / phase / semantic role / symbol context / ownership を優先する structured rule input、low-confidence lead の 2 件目 group 展開、partial mixed fallback、CI `<temp-object>` 正規化が入ったため、この項目の残差は core renderer contract ではなく long-tail corpus 拡張と RC metrics / evaluation evidence 運用にあります。

## なぜ必要か

今の実装は MVP としては十分妥当ですが、公開製品としては **誤分類時の説明可能性** と **拡張可能性** が足りません。

特に次のような構造は、ケースが増えるほど保守しづらくなります。

- `contains("template")`
- `contains("macro")`
- `contains("expected")`
- path 文字列による ownership 推定

## 具体的にやること

1. family / ownership / headline / first-action を rule table 化する
   - YAML / TOML / Rust static table のどれでもよい
   - 優先順位・前提条件・confidence を明示できるようにする

2. rule の入力を structured にする
   - semantic role
   - context chain kind
   - primary location ownership
   - child note kinds
   - GCC rule / level / SARIF metadata

3. selection の explainability を出す
   - 「なぜこの card を出したか」
   - 「なぜこの warning を suppress したか」
   を trace / debug view で見えるようにする

4. family ごとに regression corpus を増やす

## 完了条件

- 新 family を 1 つ追加する時に `contains()` を増やさずに済む
- 誤判定の理由が trace で説明できる
- family / ownership / first-action に対して fixture test がある

---

## 4. P2: 公開配布品質を上げる項目

### P2-1. GitHub Actions / supply chain / provenance を締める

## なぜ必要か

この repo は release signing や trusted key pinning まで入っていて良いです。  
だからこそ最後に、**CI/CD 側の信頼境界も揃える**べきです。

## 具体的にやること

1. GitHub Actions を full pin する
   - reusable action は tag ではなく commit SHA pin を検討

2. provenance を追加する
   - artifact に build manifest を同梱するだけでなく、release provenance を生成・保存する

3. dependency gate を強化する
   - `cargo deny` だけでなく advisory / license allowlist / duplicate dependency policy を明文化

4. 公開鍵の運用手順を文書化する
   - rotate
   - revoke
   - emergency re-sign

## 完了条件

- CI workflow で未固定 action がない
- signing key rotation 手順がある
- 公開 artifact の provenance を説明できる

---

### P2-2. 公開リリース用ドキュメントを整える

## なぜ必要か

repo 内の設計書は強いですが、**利用者向けの「どう使えばよいか」「どこまで信用してよいか」文書**は別物です。  
spec が整っていても、ユーザーが safe に使えなければ release-ready ではありません。

## 具体的にやること

1. `Quick Start` を 5 分以内で終わる形にする
2. `Known Limitations` を README から独立ページにする
3. `When you will get raw fallback` を明示する
4. before / after の representative examples を載せる
5. bug report template に trace bundle の取り方を書く

## 完了条件

- 新規ユーザーが README だけで導入できる
- fallback 時の意味が誤解されない
- issue に必要情報が自然に集まる

---

## 5. 実行順の提案

### フェーズ A: まず 1 週間で終わらせること

1. workflow pinning (`dtolnay/rust-toolchain@master` をやめる)
2. Node 24 対応 action へ更新
3. failing step の可視化
4. CI artifact の保存
5. first release scope の README 明記

### フェーズ B: 次の 1〜2 週間で終わらせること

1. GCC 15 representative corpus に acceptance gate を追加
2. fallback 率 / family 判定率 / ownership 判定率を見える化
3. `xtask` を command 単位で分割開始
4. CLI orchestration の `ExecutionPlan` 導入

### フェーズ C: 最初の一般公開前に終わらせること

1. signed artifact を 1 本実際に GitHub Release として公開
2. install / rollback / uninstall をクリーン環境で検証
3. known limitations / troubleshooting / trace guide を公開
4. release checklist を PR template または issue template 化

---

## 6. 「やらないほうがよいこと」

### 1. 初回リリース前に GCC 13/14 の enhanced path を本気で広げる

これは品質責務が増えすぎます。  
初回は GCC 15 primary を守ったほうがよいです。

### 2. `xtask` 全部を一気に作り直す

大きいですが、全部書き換えると release から遠のきます。  
**package/install/release_repo まわりから順に切る**のがよいです。

### 3. 見た目だけの formatter 改善を先にやる

今必要なのは装飾ではなく、**誤判定しないこと・fallback 率を下げること・再現可能な release gate を持つこと**です。

---

## 7. 最終的な release gate の提案

次の条件をすべて満たしたら、`v0.1.0` 相当の初回公開を検討してよいです。

- `main` の `pr-gate` / `nightly-gate` が安定して green
- workflow action が固定されている
- GCC 15 representative corpus の acceptance gate がある
- fallback 率と誤 family 判定率が数値で見える
- signed artifact を生成し、install / rollback / uninstall / install-release を通せる
- README / known limitations / troubleshooting が揃っている
- 初回リリースの support scope が明文化されている

---

## 8. 要約

いまの gcc-formed は、**製品としての骨格はかなり良い**です。  
release-ready に足りないのは、主に次の 3 つです。

1. **gate の信頼性**
2. **結果品質の実証**
3. **出荷直前の変更に耐える実装分割**

したがって、優先順位は次の順でよいです。

1. **CI / release gate の安定化**
2. **初回リリーススコープの凍結**
3. **GCC 15 corpus に対する結果品質 gate の導入**
4. **`xtask` / CLI の分割**
5. **enrich/select/render の rule-based 化**
6. **supply chain / docs / public release 運用の整備**

この順で進めると、設計の良さを壊さずに、最短で「出せる品質」に持っていけます。
