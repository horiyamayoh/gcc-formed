# gcc-formed 成長マイルストーン兼コーディングエージェント実行プレイブック

> **Legacy notice**
>
> This file reflects the pre-vNext delivery model and is kept only as historical reference.
> It is **not** the planning authority for current vNext work.
>
> The authoritative order is:
>
> 1. `SUPPORT-BOUNDARY.md`
> 2. `EXECUTION-MODEL.md`
> 3. current ADRs
> 4. current contract docs
> 5. GitHub Issues / Sub-issues / Project fields
>
> If this file conflicts with those documents, this file loses.
- 対象リポジトリ: `horiyamayoh/gcc-formed`
- 対象ブランチ: `main`
- 作成日: 2026-04-08
- 想定読者: maintainer / reviewer / coding agent
- 文書の役割: **ロードマップ**と**そのまま実装に入れる実行契約**を 1 本にまとめる

---

## 1. この文書の使い方

この文書は、単なる「理想論の roadmap」ではない。  
`gcc-formed` を **現状の `v1alpha` 基線**から **`v1beta` → `v1.0.0-rc` → `v1.0.0 stable`** へ成長させるために、coding agent がそのまま参照して PR を作れる粒度まで落とした実装計画書である。

作業者は次の順番で使う。

1. **最も手前の未完了マイルストーン**を選ぶ  
2. その中の **P0 → P1 → P2** の順で work package を切る  
3. 各 work package の「読む文書」「触るファイル」「受け入れ基準」「必須コマンド」を満たして 1 PR にする  
4. user-visible / contract 変更があれば docs / changelog / ADR を同時更新する  
5. 先のマイルストーンの実装を始めない。**alpha exit が終わる前に beta work を広げない**

---

## 2. 現在地の整理

### 2.1 現在の基線
`gcc-formed` は、spec-first な **Public Beta / `v1beta`** の実装リポジトリである。  
現在の shipped contract は意図的に狭く、次を primary とする。

- Linux first
- `x86_64-unknown-linux-musl` を primary artifact とする
- GCC 15 を primary support とする
- terminal renderer を primary surface とする
- GCC 13/14 は compatibility path とする
- raw fallback は「失敗」ではなく shipped contract の一部として残す

### 2.2 すでにある強み
この repo は、単なる CLI 試作ではなく、既に次を持っている。

- IR / adapter / renderer / trace / testkit / xtask に分かれた workspace
- quality gate / corpus / packaging / rollback の仕様書
- `cargo xtask package / install / rollback / uninstall / vendor / hermetic-release-check / release-publish / release-promote / release-resolve / install-release`
- `KNOWN-LIMITATIONS.md`, `RELEASE-CHECKLIST.md`, `SECURITY.md`, `CONTRIBUTING.md`
- issue template に support tier / trace bundle を要求する運用の芽
- `cargo xtask check` が Rust workspace test に加えて Python の CI/docs contract tests も実行する

### 2.3 いま優先すべき弱点
現時点で release-ready でない主因は、機能不足そのものよりも **「品質を主張するための証跡と gate の安定性が足りない」** ことにある。特に次が優先論点になる。

- `pr-gate` と `nightly-gate` がまだ Failure
- public beta GitHub Release path は反映済みだが、`1.0.0-rc.N` 判定用の自動 gate はまだ incomplete だった
- `v1beta` という成熟度ラベルと `0.2.0-beta.N` 系 artifact version の意味は整理済みで、RC metrics packet・fuzz / adversarial hardening・human evaluation kit は `main` に反映済みである
- `xtask` の root shell 化は完了したが、`xtask/src/commands/release.rs`（2261 lines）と `xtask/src/commands/corpus.rs`（1906 lines）に command cluster が残っている
- `diag_cli_front` と `xtask` の root entrypoint 分割は完了し、`diag_enrich` / `diag_render` の deterministic hardening も `main` に反映済みである
- public beta release path、RC gate automation、RC metrics instrumentation、fuzz / adversarial hardening、human evaluation kit、compatibility path の honest UX、stable release automation、support / incident / rollback runbook、governance freeze は `main` に反映済みで、現在の playbook に列挙された work package はすべて `main` に反映済みである
- snapshot 安定化のための transient normalization が最近まで個別パッチで継続しており、test harness 側の中心化がまだ必要

---

## 3. まず先に固定すること: ステージ名とバージョン番号を分ける

現状は以下が同居している。

- 成熟度ラベル: `v1alpha`
- workspace / artifact version: `0.1.0`
- 初回一般公開スコープの表現: `v0.1.0` 相当

このままだと、beta / rc / stable の議論で「成熟度」と「artifact semver」が混線する。  
したがって **最初の docs/ADR 作業**で、以下を固定する。

### 3.1 推奨する表現
- **成熟度ラベル**
  - `v1alpha`
  - `v1beta`
  - `v1.0.0-rc`
  - `v1.0.0 stable`
- **artifact semver**
  - `0.1.x`: alpha baseline
  - `0.2.0-beta.N`: public beta
  - `1.0.0-rc.N`: release candidate
  - `1.0.0`: stable

### 3.2 実装ルール
- README では **成熟度**と**artifact version**を明示的に分けて書く
- `CHANGELOG.md` / `RELEASE-NOTES.md` / `RELEASE-CHECKLIST.md` / `KNOWN-LIMITATIONS.md` / `SECURITY.md` でも同じ語彙を使う
- 変更は docs だけで済ませず、**ADR を 1 本追加**して用語を固定する

### 3.3 最初に追加する ADR 候補
- `ADR-0021`: Release maturity labels and artifact semver policy

---

## 4. 全マイルストーン共通のガードレール

以下は、すべての実装で守る。  
これを破る変更は「改善」ではなく baseline drift とみなす。

### 4.1 製品スコープ
- GCC 15 primary / GCC 13–14 compatibility の線を stable まで維持する
- Linux first / `x86_64-unknown-linux-musl` primary を維持する
- terminal renderer を primary surface に据える
- Clang / editor integration / daemon / TUI / auto-fix apply は **1.0.0 の前に抱えない**

### 4.2 技術原則
- fail-open を維持する
- raw stderr を常に保存できる経路を壊さない
- GCC owned diagnostics の authoritative source は GCC 15+ では SARIF のままにする
- core path に LLM / 生成 AI 依存を入れない
- facts と analysis overlay を混同しない

### 4.3 ドキュメント原則
次の変更は、実装だけで済ませず docs/ADR を更新する。

- CLI surface の変更
- config / env precedence の変更
- IR schema semantics の変更
- renderer wording / confidence 表示規約の変更
- support matrix / release channel / install contract の変更

### 4.4 リリース境界
stable 前に**保証を広げない**。  
特に次は stable まで非保証のままでよい。

- non-Linux production artifact
- GCC 13/14 enhanced render quality guarantee
- raw fallback の廃止
- package-manager-native 差分 build
- self-updater
- container-primary distribution

---

## 5. コーディングエージェントの実行ルール

### 5.1 1 PR の大きさ
1 PR は **1 work package** を原則とする。  
ただし以下を同梱してよい。

- その work package に必要な docs 更新
- その work package に必要な test / snapshot 更新
- その work package に必要な changelog 追記

### 5.2 先に読む文書
#### 常に読む
- `README.md`
- `gcc-formed-architecture-proposal.md`
- `quality-corpus-test-gate-spec.md`
- `packaging-runtime-operations-spec.md`
- `CONTRIBUTING.md`

#### adapter / capture / CLI を触るとき
- `gcc-adapter-ingestion-spec.md`
- `implementation-bootstrap-sequence.md`

#### IR / enrich / render を触るとき
- `diagnostic-ir-v1alpha-spec.md`
- `rendering-ux-contract-spec.md`
- `adr-initial-set/adr-0010-deterministic-rule-engine-no-ai-core.md`
- `adr-initial-set/adr-0015-source-ownership-model.md`
- `adr-initial-set/adr-0019-render-modes.md`
- `adr-initial-set/adr-0020-stability-promises.md`

#### packaging / release / install を触るとき
- `packaging-runtime-operations-spec.md`
- `RELEASE-CHECKLIST.md`
- `KNOWN-LIMITATIONS.md`
- `SECURITY.md`

### 5.3 変更前に必ず確認すること
- その変更が **support claim の拡大**になっていないか
- compatibility path を primary quality path のように見せていないか
- raw fallback の honest path を隠していないか
- snapshot / semantic assertion の差分説明が reviewer に渡せるか

### 5.4 最低限のローカル検証
documentation-only でない限り、最低でも次を通す。

```bash
cargo xtask check
cargo xtask replay --root corpus
cargo xtask snapshot --root corpus --subset representative --check --docker-image gcc:15
cargo deny check
cargo xtask hermetic-release-check --vendor-dir vendor --bin gcc-formed --target-triple x86_64-unknown-linux-musl
```

release / install / metadata を触る場合は追加で次を行う。

```bash
cargo xtask package --binary <path-to-binary> --target-triple x86_64-unknown-linux-musl
cargo xtask install --control-dir <control-dir> --install-root <install-root> --bin-dir <bin-dir>
cargo xtask rollback --install-root <install-root> --bin-dir <bin-dir> --version <version>
cargo xtask uninstall --install-root <install-root> --bin-dir <bin-dir> --mode purge-install
cargo xtask release-publish --control-dir <control-dir> --repository-root <repo-root>
cargo xtask release-promote --repository-root <repo-root> --target-triple x86_64-unknown-linux-musl --version <version> --channel canary
cargo xtask release-resolve --repository-root <repo-root> --target-triple x86_64-unknown-linux-musl --channel canary
cargo xtask install-release --repository-root <repo-root> --target-triple x86_64-unknown-linux-musl --version <version> --install-root <install-root> --bin-dir <bin-dir>
```

### 5.5 PR テンプレートとして必須にする項目
agent が作る PR には最低限、次を入れる。

- Goal
- Why now
- Read docs
- Files touched
- Out of scope
- Acceptance criteria
- Commands run
- Snapshot / corpus / docs update rationale
- Support tier impact
- Trace / fallback impact

---

## 6. マイルストーン全体像

| マイルストーン | 狙い | 出荷メッセージ | 終了の意味 |
|---|---|---|---|
| M0: Alpha Exit / Baseline Stabilization | gate を信用できるようにする | 「壊れにくい alpha」 | beta work に進んでよい |
| M1: Public Beta | 狭い範囲で信頼して試せる | 「GCC 15 / Linux / musl の public beta」 | 外部配布してよい |
| M2: Release Candidate | raw GCC より実際に直しやすいことを証明する | 「候補版、ship 判定待ち」 | stable 可否を判断できる |
| M3: Stable Release | 運用できる製品にする | 「1.0.0 stable」 | 継続運用と support handoff が可能 |

---

## 7. M0: Alpha Exit / Baseline Stabilization

### 7.1 目的
ここでの目的は機能追加ではない。  
**PR gate / nightly gate / packaging smoke を信用できる状態にすること**、および docs の意味論を整理して「beta で何を約束するか」を誤解なく言える状態にすること。

### 7.2 Exit Criteria
次をすべて満たしたら M0 完了。

- `pr-gate` が `main` で連続して安定 green
- `nightly-gate` の GCC 15 blocker path が連続して green
- representative replay / snapshot の flake が収束し、原因不明の transient failure が残っていない
- docs 上で成熟度ラベルと artifact version の意味が整理済み
- README / release notes / known limitations / security / contributing の support boundary が一致している
- beta work に必要な failure observability が artifacts で取得できる

### 7.3 P0 Work Package

#### M0-P0-1: Gate failure observability を追加する
**目的**  
GitHub Actions の failure が「exit code 1」だけで終わらず、artifact を見ればどの step / fixture / support tier / command が落ちたかすぐ分かる状態にする。

**読む文書**
- `quality-corpus-test-gate-spec.md`
- `CONTRIBUTING.md`
- `.github/workflows/pr.yml`
- `.github/workflows/nightly.yml`

**主に触るファイル**
- `.github/workflows/pr.yml`
- `.github/workflows/nightly.yml`
- `xtask/src/main.rs`（後で module split 予定）
- 必要なら `ci/README.md` を追加

**実装指示**
1. 各 workflow の主要 step が machine-readable JSON summary を残すようにする  
2. `REPORT_ROOT` 配下に step ごとの status file を出す  
3. 最終 step で `gate-summary` 相当の集約を出す  
4. failure 時も artifact upload は必ず走るようにする  
5. report には最低限、`step`, `status`, `command`, `exit_code`, `fixture`, `gcc_version`, `support_tier`, `artifact_paths` を含める

**受け入れ基準**
- 意図的な failure を 1 つ起こしたとき、artifact だけで failing step が特定できる
- PR gate と nightly gate の両方で同じ report schema を使う
- reviewer がログ全文を読まなくても落ちた理由をたどれる

**Out of scope**
- dashboard の可視化
- external telemetry service 連携

---

#### M0-P0-2: CI determinism と pinning を完成させる
**目的**  
「同じ commit なのに結果が揺れる」を潰す。  
特に action runtime, toolchain, docker image, normalization の揺れを減らす。

**読む文書**
- `quality-corpus-test-gate-spec.md`
- `packaging-runtime-operations-spec.md`
- `rust-toolchain.toml`
- `.github/workflows/pr.yml`
- `.github/workflows/nightly.yml`

**主に触るファイル**
- `.github/workflows/pr.yml`
- `.github/workflows/nightly.yml`
- `rust-toolchain.toml`
- 必要なら `ci/images/gcc-matrix/Dockerfile`

**実装指示**
1. workflow と `rust-toolchain.toml` の toolchain version を意図的に一致させる  
2. JS actions の runtime deprecation warning を解消する  
3. 可能であれば docker image 側も再現性の高い参照に寄せる  
4. report に `rustc`, `cargo`, `docker image`, `gcc version` を残す  
5. workflow 失敗のうち Node runtime / action runtime 起因のものを product failure と分離する

**受け入れ基準**
- 同一 commit の rerun で snapshot / replay 結果が揺れない
- workflow annotation に runtime deprecation warning が恒常的に残らない
- gate report に build environment が残る

**Out of scope**
- self-hosted runner 移行
- multi-OS CI matrix

---

#### M0-P0-3: snapshot / replay normalization を test harness 側に寄せる
**目的**  
最近までの snapshot 安定化は、個別の transient field 対応が多い。  
これを `diag_testkit` / replay / snapshot 側に集約し、将来の diff を reviewer が読める状態にする。

**読む文書**
- `quality-corpus-test-gate-spec.md`
- `diagnostic-ir-v1alpha-spec.md`

**主に触るファイル**
- `diag_testkit/**`
- `xtask/src/main.rs`
- `corpus/**`

**実装指示**
1. transient path, object path, quote style, line number drift, non-semantic SARIF fields の normalize 処理を testkit 側に集約する  
2. normalize してよいもの / してはいけないものをコメントまたは doc に残す  
3. raw contract を壊すような過剰 normalization を禁止する  
4. representative subset について semantic diff と cosmetic diff を分けて出せるようにする

**受け入れ基準**
- snapshot 更新理由が「semantic change」か「normalization-only」か区別できる
- per-case ad-hoc patch ではなく共通処理として実装される
- corpus fixture を増やしても transient noise が増えにくい

**Out of scope**
- renderer wording の大規模変更
- corpus family 拡張

---

#### M0-P0-4: versioning semantics を揃える
**目的**  
`v1alpha` と `0.1.0` の混線を解消し、beta / rc / stable の naming を fixed contract にする。

**読む文書**
- `README.md`
- `CHANGELOG.md`
- `RELEASE-NOTES.md`
- `RELEASE-CHECKLIST.md`
- `KNOWN-LIMITATIONS.md`
- `SECURITY.md`
- `CONTRIBUTING.md`

**主に触るファイル**
- `README.md`
- `CHANGELOG.md`
- `RELEASE-NOTES.md`
- `RELEASE-CHECKLIST.md`
- `KNOWN-LIMITATIONS.md`
- `SECURITY.md`
- `CONTRIBUTING.md`
- 新規 `VERSIONING.md` または新規 ADR

**実装指示**
1. maturity label と artifact semver を明示的に分けた説明を書く  
2. beta / rc / stable で使う naming を先に決める  
3. すべての doc の用語を統一する  
4. contract change 扱いとして ADR を追加する

**受け入れ基準**
- docs を読んだ third party が「いま stable かどうか」を誤読しない
- beta / rc / stable の artifact naming が既に定義されている
- reviewer が semver と maturity label を区別できる

---

### 7.4 P1 Work Package

#### M0-P1-1: support boundary 文言を 1 つに揃える
**目的**  
support matrix の説明が README / release notes / known limitations / security / contributing で微妙にズレないようにする。

**実装指示**
- 同じ support matrix を使う
- GCC 15 primary / GCC 13–14 compatibility / Linux first / musl primary / raw fallback included を全ドキュメントで同じ表現にする
- 必要なら共通の markdown section をコピーベースで管理する

**追加でやってよいこと**
- `.github/pull_request_template.md` を追加し、support tier impact と docs impact を強制する

**受け入れ基準**
- support boundary に関する user-visible 表現が 1 つに収束している

---

## 8. M1: Public Beta

### 8.1 目的
ここで初めて「外部に試してもらえる狭い範囲の製品」にする。  
ただし広げるのではなく、**GCC 15 / Linux / musl / terminal** の主戦場を磨く。

### 8.2 Exit Criteria
- public beta artifact が GitHub Releases などで公開されている
- curated corpus が beta bar に達している
- fallback が reason-coded で追跡できる
- renderer / enrich が top families で deterministic に動く
- GCC 13/14 compatibility path の honest wording が固定されている
- CLI / install / rollback / exact-pin install の docs が揃っている

### 8.3 P0 Work Package

#### M1-P0-1: curated corpus を beta bar まで拡張する
**目的**  
quality spec の hardening 条件に合わせて、corpus を seed から curated へ育てる。

**現在の足場**
- `corpus/c`: `linker`, `macro_include`, `partial`, `path`, `syntax`, `type`
- `corpus/cpp`: `overload`, `template`

**読む文書**
- `quality-corpus-test-gate-spec.md`
- `CONTRIBUTING.md`
- `adr-initial-set/adr-0018-corpus-governance.md`

**主に触るファイル**
- `corpus/**`
- `diag_testkit/**`
- `xtask/src/main.rs`

**実装指示**
1. curated corpus の目標を **80〜120 件** に置く  
2. quota を少なくとも次で満たす  
   - syntax / parser
   - simple sema / type mismatch
   - overload / candidates
   - template / constraints
   - macro / include
   - linker / assembler residual
   - partial / malformed / passthrough
   - path / locale / unicode / unreadable source
3. fixture ごとに expectation metadata を持たせる  
   例: `family`, `support_tier`, `expected_mode`, `must_fallback`, `must_have_user_owned_lead`, `expected_profile`
4. harvested trace から curated へ昇格する手順を doc 化する

**受け入れ基準**
- curated corpus 80〜120 件
- GCC 15 primary families の偏りがない
- promoted fixture の expectation を reviewer が読める
- replay / snapshot / report に family 集計が出る

---

#### M1-P0-2: fallback reason taxonomy を実装する
**目的**  
beta では「fallback した」だけでは足りない。  
**なぜ fallback したか**を reason-coded に集計できるようにする。

**読む文書**
- `gcc-formed-architecture-proposal.md`
- `quality-corpus-test-gate-spec.md`
- `gcc-adapter-ingestion-spec.md`
- `diagnostic-ir-v1alpha-spec.md`

**主に触るファイル**
- `diag_backend_probe/**`
- `diag_capture_runtime/**`
- `diag_adapter_gcc/**`
- `diag_render/**`
- `diag_trace/**`
- `diag_core/**`

**推奨 enum 例**
- `unsupported_tier`
- `incompatible_sink`
- `shadow_mode`
- `sarif_missing`
- `sarif_parse_failed`
- `residual_only`
- `renderer_low_confidence`
- `internal_error`
- `timeout_or_budget`
- `user_opt_out`

**実装指示**
1. fallback reason を IR または trace で保持する  
2. adapter / render / backend probe で reason を埋める  
3. replay / snapshot report に reason 集計を出す  
4. unexpected fallback と expected fallback を区別できるようにする

**受け入れ基準**
- fallback が「理由なしの raw fallback」にならない
- quality gate で reason ごとの件数が見える
- support / bug report / shadow review で reason が使える

---

#### M1-P0-3: `diag_enrich` を deterministic rule engine として硬化する
この work package は **2026-04-09 時点で `main` に反映済み**です。`diag_enrich` は structured rule input（context chain / phase / semantic role / symbol context / ownership）を優先し、message substring 判定を fallback に後退させました。absolute path ownership の既定値は `unknown` へ戻し、`linker.cannot_find_library` / `linker.file_format_or_relocation` など ingress-specific family の headline / first action は local override が無い限り保持されます。`diag_render` の lead selection / mixed fallback / CI hardening も続いて反映済みで、その後の public beta / RC / stable / governance freeze まで current playbook の work package はすべて `main` に反映済みです。

**目的**  
現在の `diag_enrich` は baseline としては妥当だが、beta quality を主張するには family / headline / first action の根拠を整理する必要がある。

**読む文書**
- `rendering-ux-contract-spec.md`
- `diagnostic-ir-v1alpha-spec.md`
- `adr-0010`, `adr-0015`, `adr-0019`

**主に触るファイル**
- `diag_enrich/src/lib.rs`
- 新規 `diag_enrich/src/family.rs`
- 新規 `diag_enrich/src/headline.rs`
- 新規 `diag_enrich/src/action_hint.rs`
- 新規 `diag_enrich/src/ownership.rs`

**実装指示**
1. `lib.rs` から family classification / ownership / headline / action hint を分離する  
2. family ごとに rule を module 化する  
3. message `contains()` はやむを得ない fallback とし、context chain / phase / semantic role / symbol context / ownership を優先する  
4. `unknown` を無理に減らさない。**誤分類より unknown を優先**する  
5. promoted family ごとに unit tests + corpus assertions を足す

**受け入れ基準**
- family rule の追加が 1 ファイルの巨大 if-chain 追記ではなくなる
- syntax / type_overload / template / macro_include / linker / passthrough / unknown に test がある
- high-confidence を出す条件が明示される

---

#### M1-P0-4: `diag_render` を beta quality に引き上げる
**目的**  
renderer の使命は prettify ではなく、**root cause と first action を最初の画面内に出す**ことにある。beta ではここを contract として固める。

**読む文書**
- `rendering-ux-contract-spec.md`
- `quality-corpus-test-gate-spec.md`
- `gcc-formed-architecture-proposal.md`

**主に触るファイル**
- `diag_render/**`
- `diag_enrich/**`
- `corpus/**`

**実装指示**
1. lead selection は user-owned location / confidence / phase / semantic role を踏まえて決める  
2. TTY / pipe / CI profile の budget を固定する  
3. low-confidence / partial / passthrough / failed document では honest wording を徹底する  
4. raw diagnostics への導線を必ず残す  
5. template / overload / macro/include / linker family の group card 表示を corpus で固定する  
6. warning suppression は profile ごとに deterministic にし、verbose/CI での意味のない脱落を防ぐ

**受け入れ基準**
- root cause と first action が representative cases で first screen に入る
- CI profile でも意味が崩れない
- low-confidence case で断定口調を避ける
- raw fallback と enhanced path の境界が user に分かる

**進捗メモ（2026-04-09）**
この work package は `main` に反映済み。`diag_render` は low-confidence lead のとき `default` / `concise` / `ci` で 2 件目 group まで展開できるようになり、partial document では mixed fallback の `raw:` sub-block を出しつつ `raw_fallback` への導線を残す。enhanced path の transient object path は `<temp-object>` に正規化し、exact raw は `raw_fallback` にだけ残す境界を corpus と unit tests で固定した。その後の public beta / RC / stable / governance freeze まで current playbook の work package はすべて `main` に反映済みです。

---

#### M1-P0-5: public beta release path を作る
**進捗メモ（2026-04-09）**
この work package は `main` に反映済み。`.github/workflows/release-beta.yml` が signed public beta artifact / release notes / provenance / install smoke / release repository smoke を publish し、`PUBLIC-BETA-RELEASE.md` / `RELEASE-CHECKLIST.md` / `README.md` / `CHANGELOG.md` が public user 向けの install / rollback / exact pin / promote story を固定している。その後の RC / stable / governance freeze まで current playbook の work package はすべて `main` に反映済みです。

**目的**  
packaging machinery は既にかなりある。beta では「仕組みがある」から「実際に配る」へ進む。

**読む文書**
- `packaging-runtime-operations-spec.md`
- `RELEASE-CHECKLIST.md`
- `RELEASE-NOTES.md`
- `KNOWN-LIMITATIONS.md`

**主に触るファイル**
- `.github/workflows/`（新規 release workflow を作るならここ）
- `README.md`
- `RELEASE-NOTES.md`
- `CHANGELOG.md`

**実装指示**
1. GitHub Release に載せる artifact / notes / checksums / signatures の最小セットを固定する  
2. beta channel の promote story を doc 化する  
3. install / rollback / exact version pin / install-release の user-facing 手順を README か docs に載せる  
4. `There aren’t any releases here` の状態を解消する

**受け入れ基準**
- beta artifact が public に取得できる
- release notes が support boundary と known limits を正しく言う
- install / rollback 手順が beta user 向けに完備している

---

### 8.4 P1 Work Package

#### M1-P1-1: `diag_cli_front` を分割する
**目的**  
`diag_cli_front/src/main.rs` は 1282 lines ある。beta 以降の変更速度を保つには、CLI policy / mode selection / execution / render invocation / self-check を分ける必要がある。

**進捗メモ（2026-04-09）**  
この work package は `main` に反映済み。`diag_cli_front/src/main.rs` は thin entrypoint（172 lines）まで縮小し、`args` / `config` / `mode` / `execute` / `self_check` / `trace_output` へ責務を分離した。`diag_enrich` と `diag_render` の hardening も反映済みで、その後の public beta / RC / stable / governance freeze まで current playbook の work package はすべて `main` に反映済みです。

**推奨分割**
- `src/main.rs`
- `src/args.rs`
- `src/config.rs`
- `src/mode.rs`
- `src/backend.rs`
- `src/execute.rs`
- `src/render.rs`
- `src/self_check.rs`

**ルール**
- 最初の PR では behavior-preserving refactor に徹する
- その後で機能変更 PR を重ねる

**受け入れ基準**
- root file は薄くなる
- module 境界で unit tests を書ける
- CLI contract は変えない

---

#### M1-P1-2: `xtask` を command modules に分割する
**目的**  
`xtask/src/main.rs` は 5415 lines ある。release path, gate path, install path, report path を今のまま 1 ファイルで抱えると review も事故も重くなる。

**進捗メモ（2026-04-09）**
この work package は `main` に反映済み。`xtask/src/main.rs` は dispatch-oriented shell（602 lines）として保ち、release / install / vendor / rollback / release repository 操作は `xtask/src/commands/release.rs`、replay / snapshot / acceptance report は `xtask/src/commands/corpus.rs`、shared filesystem / process helper は `xtask/src/util/fs.rs` / `xtask/src/util/process.rs`、unit tests は `xtask/src/tests.rs` へ分離された。さらに `xtask/src/commands/check.rs` が `cargo xtask check` の policy gate を担当し、Rust workspace tests と Python CI/docs contract tests を同じ入口で流す。`diag_enrich` と `diag_render` の hardening も反映済みで、その後の public beta / RC / stable / governance freeze まで current playbook の work package はすべて `main` に反映済みです。

**推奨分割**
- `src/main.rs`
- `src/commands/check.rs`
- `src/commands/replay.rs`
- `src/commands/snapshot.rs`
- `src/commands/package.rs`
- `src/commands/install.rs`
- `src/commands/release_repo.rs`
- `src/commands/gate_summary.rs`
- `src/util/fs.rs`
- `src/util/process.rs`
- `src/util/report.rs`

**ルール**
- まず CLI surface を維持したまま内部だけを split する
- JSON output schema が変わる場合は changelog と docs を更新する

**受け入れ基準**
- 新 command 追加時に giant switch を触らずに済む
- release / install / gate のレビュー範囲が分離される
- root `main.rs` は dispatch のみになる

---

## 9. M2: Release Candidate

### 9.1 目的
beta を「使ってみてください」から「ship 候補です」に上げる。  
ここで重要なのは、**raw GCC より実際に直しやすいことを証拠付きで示す**こと。

### 9.2 Exit Criteria
- curated corpus pass 100%
- rollout mode matrix pass 100%
- P0 / P1 open bug 0
- unexpected fallback threshold pass
- benchmark budgets pass
- fuzz crash 0
- deterministic replay pass
- UX review sign-off 完了

### 9.3 P0 Work Package

#### M2-P0-1: RC gate を自動化する
**進捗メモ（2026-04-09）**  
この work package は `main` に反映済み。`cargo xtask rc-gate` が curated replay / rollout matrix / benchmark smoke / deterministic replay と sign-off evidence を 1 コマンドで集約し、`rc-gate-report.json` / `rc-gate-summary.md` / sub-report を `report_dir` に保存する。`cargo xtask bench-smoke` は target-only stub から実測 report へ昇格し、`--formed-self-check` も canonical rollout matrix を JSON に含めるようになった。その後の hardening で fuzz は manual template ではなく自動 `fuzz-smoke` へ置き換わっており、current playbook の work package はすべて `main` に反映済みです。

**目的**  
quality spec に書いてある release candidate gate を、手順書ではなく自動 gate にする。

**主に触るファイル**
- `xtask/**`
- `.github/workflows/`（必要なら release candidate workflow を追加）
- `RELEASE-CHECKLIST.md`

**実装指示**
1. `cargo xtask rc-gate` 相当を作る  
2. curated replay / rollout matrix / benchmark / fuzz / deterministic replay / issue budget を 1 コマンドで実行できるようにする  
3. report を machine-readable に保存する  
4. failure 時に ship blocker を明示する

**受け入れ基準**
- RC 判定が人手の checklist 依存だけでなく自動 gate になる
- report から blocker を 1 画面で読める

---

#### M2-P0-2: metrics instrumentation を入れる
**進捗メモ（2026-04-09）**
この work package は `main` に反映済み。`cargo xtask rc-gate` は `metrics-report.json` を生成し、`replay-report.json` fixture summary から fallback rate / diagnostic compression ratio / rendered first-action line / family coverage / compatibility-vs-primary policy / benchmark p95 を集約する。完全自動で測れない TRC / TFAH / first-fix success / high-confidence mislead は `eval/rc/metrics-manual-eval.json` に落とし、strict RC gate では pending を blocker にできる。その後の hardening で fuzz / adversarial gate、human evaluation kit、compatibility path の honest UX、stable release automation、support / incident / rollback runbook、governance freeze まで固定され、current playbook の work package はすべて `main` に反映済みです。

**目的**  
architecture proposal にある KPI を、完全自動でなくてもよいので測れる形にする。

**測るもの**
- fallback rate
- high-confidence mislead rate
- diagnostic compression ratio
- first actionable hint 到達行
- p95 overhead / postprocess time
- family coverage
- compatibility vs primary path の比較

**主に触るファイル**
- `diag_trace/**`
- `xtask/**`
- 必要なら `docs/eval/` や `eval/`

**実装指示**
1. replay / render / trace から KPI の素材を取る  
2. 完全自動で測れない指標は manual evaluation packet に落とす  
3. report に raw GCC 比較の枠を作る

**受け入れ基準**
- RC sign-off に必要な数字が欠落しない
- fallback / mislead / overhead を継続観測できる

---

#### M2-P0-3: fuzz / adversarial hardening を入れる
**進捗メモ（2026-04-09）**
この work package は `main` に反映済み。`cargo xtask fuzz-smoke --root fuzz` が checked-in adversarial seed suite（11 cases）を replay し、SARIF ingest / residual stderr classifier / invalid IR validation / render stress / trace serialization / capture-runtime path sanitization を crash-free で検証する。`cargo xtask rc-gate` は fuzz smoke を自動実行して `fuzz-smoke-report.json` / `fuzz-evidence.json` を保存し、manual `fuzz-status.json` なしで `fuzz crash 0` を gate 判定できる。`nightly-gate` も release-blocker lane で `cargo xtask fuzz-smoke` を実行するようになった。human evaluation kit、compatibility path の honest UX、stable release automation、support / incident / rollback runbook、governance freeze まで main に反映され、current playbook の work package はすべて `main` に反映済みです。

**目的**  
release candidate で crash-free を主張するために、ingest / render / trace の robustness を上げる。

**主に触るファイル**
- `diag_adapter_gcc/**`
- `diag_render/**`
- `diag_core/**`
- `diag_trace/**`
- `fuzz/`（新設してよい）

**実装指示**
1. SARIF ingest に fuzz target を追加  
2. malformed / partial / residual text path の adversarial corpus を増やす  
3. crash した入力は minimized fixture として corpus に昇格する  
4. panic / OOM / pathological parse を release blocker として扱う

**受け入れ基準**
- known crashers が regression fixture で塞がれている
- nightly で fuzz or fuzz-corpus replay が回る
- RC gate で fuzz crash 0 が確認できる

---

#### M2-P0-4: human evaluation kit を整える
この work package は `main` に反映済み。`cargo xtask human-eval-kit --root corpus --report-dir target/human-eval` が representative fixture 16 件から repeatable review bundle を生成し、expert review sheet、task-study sheet、counterbalance matrix、`metrics-manual-eval.template.json`、`ux-signoff.template.json`、fixture-local actual/expected/render/source artifacts を保存する。`cargo xtask rc-gate` も同じ bundle を `report_dir/human-eval/` に自動生成し、RC artifact retention の一部として保持する。task-study set は syntax / macro_include / template / type / overload / linker の required family をすべて含む 15 fixtures で固定され、expert review set には omission honesty を見るための partial representative も含める。compatibility path の honest UX、stable release automation、support / incident / rollback runbook、governance freeze まで main に反映され、current playbook の work package はすべて `main` に反映済みです。

---

### 9.4 P1 Work Package

#### M2-P1-1: compatibility path の honest UX を固定する
この work package は `main` に反映済み。`diag_cli_front` は compatibility path を selected mode だけでぼかさず、`support tier=b/c`・`selected mode`・`fallback reason` を含む exact banner を stderr に出すようになった。`--formed-self-check` の rollout matrix も同じ `scope_notice` を返し、`cargo xtask rc-gate` は expected rollout matrix に notice wording まで含めて drift を検知する。wrapper integration tests では GCC 13 passthrough / GCC 13 shadow / GCC 12 out-of-scope の banner と trace support tier を固定し、`KNOWN-LIMITATIONS.md` と `ADR-0005` も同じ wording に更新した。stable release automation と support / incident / rollback runbook、governance freeze まで main に反映され、current playbook の work package はすべて `main` に反映済みです。

---

## 10. M3: Stable Release

### 10.1 目的
ここで初めて「よくできた CLI」から「運用できる製品」へ進む。

### 10.2 Exit Criteria
- rollout readiness gate を満たす
- shadow mode の十分なサンプルを確保している
- unexpected fallback < 0.1%
- high-confidence mislead < 2%
- top 10 family で raw GCC 比非劣化
- trace bundle redaction audit pass
- support / rollback / incident docs 完備
- signed stable artifacts を publish 済み

### 10.3 P0 Work Package

#### M3-P0-1: stable release automation を完成させる
この work package は `main` に反映済み。`xtask` には `cargo xtask stable-release` が追加され、seed した immutable release repository を使って candidate control dir を `canary` → `beta` → `stable` へ promote し、`stable-release-report.json` / `stable-release-summary.md` / `promotion-evidence.json` / `rollback-drill.json` を保存できるようになった。rollback drill は previous published version を install した状態から candidate を exact version / checksum / signature pin で install し、1 回の `current` symlink switch で baseline へ戻ることを test と report の両方で固定する。`.github/workflows/release-stable.yml` と `STABLE-RELEASE.md` は prior GitHub Release の `.release-repo.tar.gz` bundle を seed に使う stable cut runbook を提供し、`ADR-0025` と packaging spec も same-bits promote / rollback evidence の契約に更新した。support / incident / rollback runbook と governance freeze まで main に反映され、current playbook の work package はすべて `main` に反映済みです。

---

#### M3-P0-2: support / incident / rollback runbook を揃える
この work package は `main` に反映済み。`SUPPORT.md` が support tier / public bug report / security path の分岐を固定し、`docs/runbooks/incident-triage.md` は tier A/B/C と surface ごとの初動、`docs/runbooks/trace-bundle-collection.md` は `--formed-self-check` と `--formed-trace=always` を使った trace bundle 採取・redaction・最小公開 packet、`docs/runbooks/rollback.md` は rollback / uninstall / reinstall / exact-pin recovery 手順を提供する。`.github/ISSUE_TEMPLATE/bug_report.yml` もこれら runbook へリンクし、README / RELEASE-CHECKLIST / SECURITY / CONTRIBUTING が support 導線を参照するよう更新した。governance freeze まで main に反映され、current playbook の work package はすべて `main` に反映済みです。

---

#### M3-P0-3: governance freeze をかける
この work package は `main` に反映済み。`GOVERNANCE.md` が stable-prep governance freeze の運用正本となり、CLI / config / IR / renderer wording / release-install / support routing を stable contract surface として列挙し、`breaking` / `non-breaking` / `experimental` の線引きと required action を明文化した。`ADR-0020` も stable 向けに補強され、post-`1.0.0` の breaking change は next major か versioned replacement lane を要求し、pre-`1.0.0` must-have backlog と post-`1.0.0` backlog の分離も `GOVERNANCE.md` に固定された。`.github/pull_request_template.md` は change classification と affected contract surfaces の記入欄を追加し、`ci/test_governance_docs.py` と `ci/test_support_boundary_docs.py` で PR template / governance docs / ADR alignment を検査できる。さらに `cargo xtask check` も `python3 -B -m unittest discover -s ci -p test_*.py` を含むようになり、CI helper script / support-boundary / governance drift は Rust workspace tests と同じ標準 gate で止まる。現在の playbook に列挙された work package はすべて `main` に反映済みであり、以後の change は governance freeze の下で新しい backlog として提案する。

---

## 11. 直近で切るべき PR の順序

ここから先は、実際に coding agent が最初に切るべき順番である。  
迷ったらこの順に進める。

1. **PR-1**: `VERSIONING.md` + README / CHANGELOG / RELEASE-NOTES / SECURITY / CHECKLIST / LIMITATIONS の用語統一 + `ADR-0021`
2. **PR-2**: workflow report schema 追加 + gate summary artifact 化
3. **PR-3**: CI determinism / Node runtime deprecation 解消 / build environment report 追加
4. **PR-4**: snapshot normalization の testkit 集約
5. **PR-5**: fallback reason enum と trace/report 連携
6. **PR-6**: curated corpus metadata schema 追加
7. **PR-7**: corpus 拡張（family quota を満たす）
8. **PR-8**: `diag_enrich` modularization + rule hardening（2026-04-09 時点で `main` 反映済み）
9. **PR-9**: `diag_render` lead selection / low-confidence / CI profile hardening（2026-04-09 時点で `main` 反映済み）
10. **PR-10**: `diag_cli_front` modularization（2026-04-09 時点で `main` 反映済み）
11. **PR-11**: `xtask` modularization（2026-04-09 時点で `main` 反映済み）
12. **PR-12**: public beta release workflow + first beta artifact

---

## 12. 新規に追加してよい ADR 候補

- `ADR-0021`: Release maturity labels and artifact semver policy（Accepted on `main`）
- `ADR-0022`: Fallback reason taxonomy and reporting contract
- `ADR-0023`: Curated corpus expectation schema and promotion policy
- `ADR-0024`: Public beta release channel and GitHub Release policy（Accepted on `main`）
- `ADR-0025`: Stable release automation and rollback evidence（Accepted on `main`）
- `ADR-0026`: RC gate automation and sign-off contract

---

## 13. coding agent が issue / PR に貼るテンプレート

````md
## Goal
この変更で達成することを 1 文で書く。

## Why now
どのマイルストーンのどの exit criteria を前進させるかを書く。

## Milestone / Work Package
- Milestone:
- Work package:

## Read first
- [ ] README.md
- [ ] relevant spec(s)
- [ ] relevant ADR(s)

## Files to touch
- path/to/file
- path/to/file

## Constraints
- support boundary を広げない
- fail-open を壊さない
- raw fallback を隠さない
- docs / changelog / ADR update 要否を確認する

## Implementation plan
1.
2.
3.

## Acceptance criteria
- [ ]
- [ ]
- [ ]

## Commands run
```bash
cargo xtask check
cargo xtask replay --root corpus
cargo xtask snapshot --root corpus --subset representative --check --docker-image gcc:15
cargo deny check
cargo xtask hermetic-release-check --vendor-dir vendor --bin gcc-formed --target-triple x86_64-unknown-linux-musl
```

## Docs updated
- [ ] CHANGELOG.md
- [ ] RELEASE-NOTES.md
- [ ] README.md
- [ ] ADR (if contract changed)

## Out of scope
- 
````

---

## 14. 明確に「やらない」と決めること

1. alpha exit 前に Clang support を始めない  
2. beta 前に editor/TUI/daemon を始めない  
3. fallback 率を下げるために compatibility path を enhanced path のように偽装しない  
4. unknown family を減らすために誤分類を増やさない  
5. snapshot を通すために semantic facts を削らない  
6. release automation の前に package-manager-native 差分配布へ広げない  
7. stable 前に self-updater を入れない  
8. “CI が緑だから stable” と判断しない。必ず RC gate と rollout readiness gate を通す

---

## 15. 最終判断の基準

### `v1beta` と言ってよい状態
- gate が安定
- public artifact がある
- support boundary が誤読されない
- curated corpus / fallback reason / top families の render が実用域
- 外部配布しても「試してもらう」価値がある

### `v1.0.0-rc` と言ってよい状態
- curated corpus 100%
- P0/P1 0
- fuzz crash 0
- deterministic replay pass
- human evaluation sign-off あり
- raw GCC 比で少なくとも primary path は非劣化

### `v1.0.0 stable` と言ってよい状態
- rollout readiness gate を満たす
- canary/beta/stable promote が実運用済み
- support/runbook/rollback が完備
- ship しても maintainer が支えられる

---

## 16. ひとことで言うと

`gcc-formed` を伸ばすときに大事なのは、**機能を増やすことではなく、狭い約束を強く守ること**である。  
この repo はすでに spec, ADR, workspace, packaging, corpus, gate の骨格を持っている。  
したがって次にやるべきことは「次の夢」を増やすことではなく、**alpha の曖昧さを潰し、beta の約束を作り、RC の証拠を揃え、stable の運用を完成させること**である。

その順番を崩さなければ、`gcc-formed` は「よく考えられた試作」から「リリース可能な製品」へ着実に進められる。
