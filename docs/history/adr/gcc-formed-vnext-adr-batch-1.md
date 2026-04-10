---
doc_role: history-only
lifecycle_status: superseded
audience: both
use_for: Historical ADR batch context and provenance.
do_not_use_for: Current accepted design decisions.
supersedes: []
superseded_by:
  - ../../../adr-initial-set/README.md
---
> [!IMPORTANT]
> Authority: `history-only` / `superseded`
> Use for: Historical ADR batch context and provenance.
> Do not use for: Current accepted design decisions.

# gcc-formed vNext ADR Batch 1

- 文書種別: ADR たたき台集 / batch 承認用
- 状態: Draft for split-and-land
- 対象: `horiyamayoh/gcc-formed` (`main`, 2026-04-09 時点)
- 目的: vNext 変更設計を実装可能にする最初の 8 本の ADR を、個別ファイルへ分割できる粒度で固定する
- 想定読者: maintainer / reviewer / coding agent

---

## 0. この文書の使い方

本書は、個別 ADR ファイルへ分割する前の batch 正本である。  
現行 repo には `ADR-0024` と `ADR-0025` がすでに存在するため、この batch を split-and-land するときは `adr-initial-set/adr-0026-...md` から `adr-0033-...md` へ分割する。

本 batch の狙いは 1 つである。

**GCC 15 前提の single-track repo を、複数 processing path を持つ vNext repo へ安全に転換するための設計判断を、先に固定すること。**

---

## 1. 採択順序

採択順は次で固定する。

1. ADR-0033 Execution Model precedes Epic generation
2. ADR-0026 Capability Profile replaces Support Tier
3. ADR-0027 Processing Path is separate from Support Level
4. ADR-0028 CaptureBundle becomes the only ingest entry
5. ADR-0029 Path B and Path C are first-class product paths
6. ADR-0030 Theme/Layout separated from analysis/view model
7. ADR-0031 Native non-regression for TTY default
8. ADR-0032 Rulepack externalization policy

理由は、Execution Model がないと後続 ADR を安全に delivery へ落とせず、Capability / Path の分離がないと CaptureBundle も Support wording も正しく設計できないためである。

---

## 2. ADR-0026 Capability Profile replaces Support Tier

### Context

現行 repo は `SupportTier::A/B/C` に、少なくとも次の概念を押し込めている。

- GCC バージョン帯
- 利用可能な structured format
- dual-sink の可否
- user-visible render の保証度
- downgrade / fallback の扱い

この設計は GCC 15 / 13–14 / それ未満を大きく区別するには便利だが、`VersionBand` と `capabilities` と `public support claim` を混同する。

### Decision

`SupportTier` を製品中心概念として使うことをやめ、代わりに次を導入する。

- `VersionBand`
- `CapabilityProfile`
- `SupportLevel`

`CapabilityProfile` は実際に利用できる機能集合を表し、少なくとも次のフラグを持つ。

- `native_text_capture`
- `json_diagnostics`
- `sarif_diagnostics`
- `dual_sink`
- `tty_color_control`
- `caret_control`
- `parseable_fixits`
- `locale_stabilization`

### Consequences

- Band ごとの差を capture 設計に閉じ込めやすくなる
- GCC 13/14 や GCC 9–12 でも「価値を返せる理由」を capability ベースで説明できる
- README / bug form / PR template の語彙を更新する必要がある

### Must update

- `diag_backend_probe`
- `README.md`
- `SUPPORT-BOUNDARY.md`
- `.github/pull_request_template.md`
- `.github/ISSUE_TEMPLATE/bug_report.yml`

### First implementation slices

1. `SupportTier` を削除せず並存で `CapabilityProfile` を導入する
2. `ProbeResult` に `version_band` と `capability_profile` を追加する
3. user-visible wording を tier 依存から band / support-level 依存へ移す

---

## 3. ADR-0027 Processing Path is separate from Support Level

### Context

現行 repo では、GCC 13/14 やそれ未満の経路が「compatibility-only」「passthrough」へ収束しやすく、runtime 分岐が support claim と実行 path を混同している。

### Decision

実行経路を `ProcessingPath` として明示的に分離する。

- `DualSinkStructured`
- `SingleSinkStructured`
- `NativeTextCapture`
- `Passthrough`

`SupportLevel` は public claim を表す別概念とし、runtime path を直接表さない。

### Consequences

- 13/14 や 9–12 に対しても、今どの path で価値を返しているかを明示できる
- `mode.rs` や capture runtime の分岐が整理しやすくなる
- docs / tests / bug reports で `Processing Path` を露出させる必要がある

### Must update

- `diag_cli_front/src/mode.rs`
- `diag_capture_runtime`
- trace bundle schema
- bug form / PR template

### First implementation slices

1. `ProcessingPath` enum 導入
2. runtime resolution を `VersionBand x CapabilityProfile -> ProcessingPath` へ置換
3. trace bundle に resolved path を保存

---

## 4. ADR-0028 CaptureBundle becomes the only ingest entry

### Context

現行 adapter 境界は概念的には `StructuredArtifact` 一般ではなく、実質的に `sarif_path + stderr_text` に寄っている。  
このままでは JSON path、single-sink path、native-text-centered path が不自然になる。

### Decision

ingest の唯一の入口を `CaptureBundle` にする。

`CaptureBundle` は少なくとも次を持つ。

- backend metadata
- version band
- capability profile
- resolved processing path
- raw stdout / raw stderr
- structured artifact list（0..n）
- exit status
- timing / integrity / provenance
- tty / color / locale などの capture context

adapter は `CaptureBundle -> DiagnosticDocument` のみを責務とする。

### Consequences

- SARIF-only 前提を外せる
- future Clang / linker path へも広げやすい
- capture runtime と adapter の境界が明確になる

### Must update

- `gcc-adapter-ingestion-spec.md`
- `diag_adapter_gcc`
- `diag_capture_runtime`
- trace bundle docs

### First implementation slices

1. `CaptureBundle` 型を導入
2. 既存 `sarif_path` 入口を wrapper 化して `CaptureBundle` に詰める
3. adapter は `CaptureBundle` 以外の新規入口を増やさない

---

## 5. ADR-0029 Path B and Path C are first-class product paths

### Context

現行 repo では GCC 13/14 が compatibility-only、より古い系は事実上 first-release scope 外へ置かれている。  
しかし vNext の doctrine は「程度差はあっても GCC 9〜15 全体で類似の UX 原則を返す」ことを要求する。

### Decision

`GCC13-14` と `GCC9-12` を **first-class product paths** として扱う。  
これは「同一保証を即時に主張する」ことではない。  
意味するのは、

- 仕様に存在する
- tests に存在する
- bugs / issues / milestones に存在する
- roadmap 上の正規対象である

ということである。

### Consequences

- Path B/C は now-or-never の本線になる
- README や support boundary の語彙は慎重に再設計する必要がある
- quality gate も path-aware になる必要がある

### Must update

- `README.md`
- `SUPPORT-BOUNDARY.md`
- `quality-corpus-test-gate-spec.md`
- corpus / real-compiler matrix

### First implementation slices

1. matrix 上に GCC 13/14, GCC 9–12 の dedicated lanes を追加
2. bugs / issues / project fields に `Band` を導入
3. path-specific corpus subsets を作る

---

## 6. ADR-0030 Theme/Layout separated from analysis/view model

### Context

ユーザーの実観測で、色がなく native より見づらい、長い、書式を変えたくなった時の保守性が不安、という問題が出ている。  
これは theme/layout と analysis/view model が十分に分離されていない兆候である。

### Decision

表示層を少なくとも次の 4 層に分離する。

- `facts`
- `analysis`
- `view model`
- `theme/layout`

theme/layout は color, indentation, headings, disclosure markers, compact/expanded presentation を扱う。  
analysis は family, confidence, root ranking, first action など意味論のみを扱う。

### Consequences

- 表示書式変更のコストが下がる
- color / length / disclosure の回帰を isolated にテストできる
- Path 差を renderer 深部へ漏らしにくくなる

### Must update

- `diag_render`
- `rendering-ux-contract-spec.md`
- render snapshots / semantic assertions

### First implementation slices

1. `ViewModel` 明示化
2. `ThemeProfile` / `RenderTheme` 導入
3. layout tests と semantic tests を分離

---

## 7. ADR-0031 Native non-regression for TTY default

### Context

ユーザーが最初に比較するのは native GCC と wrapper である。  
そこで色が消える、長くなる、読みにくくなるなら、どれだけ将来の理想があっても採用されない。

### Decision

default TTY では native GCC 非劣化を shipped contract に昇格する。  
少なくとも次を MUST にする。

- 色の扱いを設計対象にする
- line budget を規定する
- root cause と first action を初画面へ出す
- template / stdlib / overload noise を budget の中で圧縮する
- low-confidence や preserved raw への導線を honesty を保って出す

### Consequences

- renderer の変更に stop-ship 条件が増える
- capture runtime にも color strategy の契約が必要になる
- quality gate に UX regression lanes が必要になる

### Must update

- `rendering-ux-contract-spec.md`
- `quality-corpus-test-gate-spec.md`
- `diag_capture_runtime`
- `diag_render`

### First implementation slices

1. color regression fixture を追加
2. line budget assertions を追加
3. template noise exemplar に対する compaction assertions を追加

---

## 8. ADR-0032 Rulepack externalization policy

### Context

family 判定、headline、action hint、residual text 分類が Rust コード側に寄るほど、Path B/C の追加や wording 変更で if/else が増殖する。  
将来的な保守性のためには、意味論を外へ出す方針を先に固定する必要がある。

### Decision

rule は意味論を contract 化し、可能な範囲で rulepack として外部化する。  
ただし core path では deterministic・versioned・reviewable を維持する。

最初の対象候補:

- family classification
- first action hint
- note suppression / compaction policy
- linker / assembler residual grouping

### Consequences

- docs と tests が rulepack の正本になる
- core コードは rule engine と data loading に責務を絞れる
- 早すぎる全面外出しは危険なので staged adoption が必要

### Must update

- `diag_enrich`
- `diag_residual_text`
- `quality-corpus-test-gate-spec.md`
- future rulepack spec

### First implementation slices

1. internal table-driven rule representation へ寄せる
2. semantic assertions を rule id 単位で持つ
3. external file format は第 2 段階で導入

---

## 9. ADR-0033 Execution Model precedes Epic generation

### Context

nightly agent 前提の開発では、Issue の切り方・レビューの仕方・停止条件が未整備なまま Epic を増やすと、実装速度だけが上がり手戻りが増える。

### Decision

Execution Model を Epic より前に固定する。  
具体的には、

- Issue taxonomy
- Project fields
- `Agent Ready` の定義
- nightly 抽出規則
- morning review 4 択
- human-only boundary

を先に確定し、その後に Epic を生成する。

### Consequences

- 最初の 1〜2 週間は delivery system install が主作業になる
- その後の nightly ははるかに安全になる
- maintainers 間で「どの Issue が ready か」の基準が揃う

### Must update

- `EXECUTION-MODEL.md`
- agent playbook / project config / templates
- milestone planning docs

### First implementation slices

1. `EXECUTION-MODEL.md` を追加
2. Project / custom fields / milestone を作成
3. PR template / bug form を vNext vocabulary に置換

---

## 10. 8 本まとめて見たときの核心

この batch はバラバラの改善案ではない。  
1 本の筋は次である。

1. **Execution Model を先に固定する**
2. **Capability と Processing Path を分離する**
3. **CaptureBundle で入口を一本化する**
4. **Path B/C を正式な製品 path に昇格する**
5. **表示層を theme/layout まで分離する**
6. **default TTY 非劣化を ship 条件にする**
7. **rule を長期保守可能な形に出していく**

---

## 11. 分割時の推奨ファイル名

- `adr-0026-capability-profile-replaces-support-tier.md`
- `adr-0027-processing-path-separate-from-support-level.md`
- `adr-0028-capturebundle-only-ingest-entry.md`
- `adr-0029-path-b-and-c-are-first-class-product-paths.md`
- `adr-0030-theme-layout-separated-from-analysis-view-model.md`
- `adr-0031-native-non-regression-for-tty-default.md`
- `adr-0032-rulepack-externalization-policy.md`
- `adr-0033-execution-model-precedes-epic-generation.md`

---

## 12. この batch 承認後に直ちにやること

1. `EXECUTION-MODEL.md` を repo 正本へ追加
2. `Project` / `Milestone` / `CODEOWNERS` / templates を整備
3. contract docs rewrite 草案を作成
4. `Delivery System Install` Epic を起票
5. その Epic から bounded Work Package を切る

---

## 13. この batch の一文要約

> **GCC 15 専用に磨かれた repo を、複数 processing path を持つ製品 repo に変えるための、最初の 8 本の不可逆な判断。**
