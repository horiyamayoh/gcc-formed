# gcc-formed vNext Contract Docs Rewrite Pack

- 文書種別: contract docs rewrite 草案
- 状態: Draft for adoption
- 対象: `horiyamayoh/gcc-formed` (`main`, 2026-04-09 時点)
- 目的: doctrine / change design / execution model を、repo の公開契約と運用テンプレートへ落とす
- 想定読者: maintainer / reviewer / coding agent / future contributor

---

## 0. この文書の位置づけ

この文書は、`gcc-formed` の vNext に向けた**契約文書差し替えパック**である。  
目的は、「設計思想は変わったが、README と templates が旧前提のまま」という状態を終わらせることにある。

ここで行うのは実装変更ではない。  
ここで行うのは、**repo の表の顔を、vNext doctrine に合わせて差し替えること**である。

---

## 1. なぜこの rewrite が必要か

現行 repo の公開文書とテンプレートは、まだ GCC 15 中心の旧前提を強く保持している。

- `README.md` は current support boundary として  
  **“GCC 15 is the primary enhanced-render path”**  
  **“GCC 13/14 are compatibility-only paths”**  
  をそのまま掲げている。
- `SUPPORT-BOUNDARY.md` も同じ wording を canonical wording として固定している。
- `implementation-bootstrap-sequence.md` は、実装初手を  
  **backend resolution → capture runtime → GCC 15 shadow → SARIF parser → render → raw fallback**  
  としており、GCC 15 shadow 起点の順序を前提にしている。
- `.github/pull_request_template.md` は `Support Tier Impact` という項目で  
  **GCC 15 primary enhanced-render path / GCC 13/14 compatibility-only path**  
  を選ばせる構造になっている。
- `.github/ISSUE_TEMPLATE/bug_report.yml` も `Support tier` を起点に bug を収集している。

この状態では、Execution Model を入れても、nightly agent も reviewer も**旧い語彙で repo を解釈してしまう**。  
そのため、Epic を増やす前に contract docs rewrite を行う必要がある。

---

## 2. rewrite の狙い

この rewrite の狙いは 5 つである。

1. **GCC 15-only posture をやめる**
2. **VersionBand / ProcessingPath / SupportLevel を公開語彙にする**
3. **GCC 13–14 と GCC 9–12 を first-class product bands として表に出す**
4. **raw fallback と TTY default 非劣化を public contract に格上げする**
5. **README を細部の実装目録ではなく、方針と導線の文書へ戻す**

---

## 3. この rewrite で差し替える対象

このパックでは、次の 6 つを ready-to-land draft として用意する。

1. `README.md`
2. `SUPPORT-BOUNDARY.md`
3. `implementation-bootstrap-sequence.md`
4. `.github/pull_request_template.md`
5. `.github/ISSUE_TEMPLATE/bug_report.yml`
6. `.github/CODEOWNERS`

加えて、旧 playbook をすぐ消せない場合のための legacy banner も付ける。

---

## 4. landing 方針

この rewrite は、できれば **docs-only PR** として先に land する。  
同じ PR でやってよいのは、せいぜい `CODEOWNERS` の追加までである。

この PR では**実装挙動を変えない**。  
変えるのは repo の契約語彙と開発運用の前提だけである。

---

## 5. file-by-file draft

## 5.1 `README.md`

```md
# gcc-formed

- **状態**: Public Beta
- **成熟度ラベル**: `v1beta`
- **artifact semver 系列**: `0.2.0-beta.N`
- **一般利用向け安定版**: 未提供
- **日付**: 2026-04-09
- **位置づけ**: doctrine-driven / spec-first / multi-path diagnostic UX wrapper

`gcc-formed` は、GCC をラップし、C/C++ のコンパイルエラーやリンクエラーを**より短く、より根因に近く、より誠実に**提示するためのリポジトリである。

目標は GCC の生出力を単に prettier にすることではない。  
目標は、**GCC 9〜15 にまたがる複数の capture / ingest 経路を持ちながら、1 つの UX 原則で価値を返すこと**である。

---

## プロダクト原則

`gcc-formed` は、次の原則を repo 全体の正本として採用する。

- **GCC 15+ は最良の reference path だが、唯一の product path ではない。**
- **GCC 13–14 と GCC 9–12 も first-class product bands である。**  
  保証や fidelity は異なってよいが、issue / test / quality gate / roadmap 上の正規対象から外してはならない。
- **capture path は複数でも、UX 原則は 1 つである。**
- **raw fallback は shipped contract の一部である。**  
  wrapper は、根拠なく compiler-owned facts を隠してはならない。
- **default TTY は native GCC より読みにくくなってはならない。**  
  色、長さ、ノイズ量、視線誘導で負けるなら、表示か fallback を見直す。

---

## 現在の support posture

SupportLevel / ProcessingPath / RawPreservationLevel の正本は [SUPPORT-BOUNDARY.md](SUPPORT-BOUNDARY.md) に置く。  
README では要点だけを再掲する。

| VersionBand | 典型的な ProcessingPath | 現在の beta support level | 位置づけ |
|---|---|---|---|
| GCC 15+ | `DualSinkStructured` | `Preview` | 最高 fidelity の reference path |
| GCC 13–14 | `SingleSinkStructured` / `NativeTextCapture` | `Experimental` | in-scope product path。compatibility-only ではない |
| GCC 9–12 | `SingleSinkStructured` (JSON) / `NativeTextCapture` | `Experimental` | in-scope product path。価値の幅はより限定的 |
| Unknown / other | `Passthrough` | `PassthroughOnly` | fail-open と provenance 保持を優先 |

---

## この repo の読み方

上から下へ読む。

1. [SUPPORT-BOUNDARY.md](SUPPORT-BOUNDARY.md)  
   現在の public wording と beta support posture の正本
2. [EXECUTION-MODEL.md](EXECUTION-MODEL.md)  
   Epic を切る前提、Issue 正本主義、nightly agent 運用の正本
3. [diagnostic-ir-v1alpha-spec.md](diagnostic-ir-v1alpha-spec.md)  
   正規化 IR の実装契約
4. [gcc-adapter-ingestion-spec.md](gcc-adapter-ingestion-spec.md)  
   capture / ingest の実装契約
5. [rendering-ux-contract-spec.md](rendering-ux-contract-spec.md)  
   renderer と disclosure の実装契約
6. [quality-corpus-test-gate-spec.md](quality-corpus-test-gate-spec.md)  
   corpus-driven quality gate の実装契約
7. [adr-initial-set/README.md](adr-initial-set/README.md)  
   採択済み ADR の索引
8. [VERSIONING.md](VERSIONING.md) / [GOVERNANCE.md](GOVERNANCE.md)  
   成熟度ラベル、artifact 系列、変更分類の用語契約

---

## vNext で何を変えるのか

vNext では、repo の主語を **SupportTier** から外し、次の 4 概念に分ける。

- **VersionBand**  
  `GCC15+` / `GCC13-14` / `GCC9-12` / `Unknown`
- **CapabilityProfile**  
  `dual_sink`, `sarif`, `json`, `native_text_capture`, `tty_color_control`, `caret_control`, `parseable_fixits` など
- **ProcessingPath**  
  `DualSinkStructured` / `SingleSinkStructured` / `NativeTextCapture` / `Passthrough`
- **SupportLevel**  
  `Preview` / `Experimental` / `PassthroughOnly`

重要なのは、**GCC 15 を特別扱いしてよいが、GCC 15 だけを“プロダクト”にしてはならない**という点である。

---

## リポジトリにあるもの

### 契約文書

- [SUPPORT-BOUNDARY.md](SUPPORT-BOUNDARY.md): public wording と support posture の正本
- [EXECUTION-MODEL.md](EXECUTION-MODEL.md): delivery system の正本
- [diagnostic-ir-v1alpha-spec.md](diagnostic-ir-v1alpha-spec.md): IR 契約
- [gcc-adapter-ingestion-spec.md](gcc-adapter-ingestion-spec.md): capture / ingest 契約
- [rendering-ux-contract-spec.md](rendering-ux-contract-spec.md): terminal / CI renderer 契約
- [quality-corpus-test-gate-spec.md](quality-corpus-test-gate-spec.md): quality gate 契約
- [packaging-runtime-operations-spec.md](packaging-runtime-operations-spec.md): packaging / install / rollback / release engineering 契約
- [implementation-bootstrap-sequence.md](implementation-bootstrap-sequence.md): 実装開始順の正本
- [adr-initial-set/README.md](adr-initial-set/README.md): ADR 索引
- [VERSIONING.md](VERSIONING.md): 成熟度ラベルと artifact semver
- [GOVERNANCE.md](GOVERNANCE.md): 変更分類と freeze ルール

### 実装ワークスペース

- `diag_backend_probe`: VersionBand / CapabilityProfile の解決
- `diag_capture_runtime`: child spawn、capture、artifact 収集
- `diag_adapter_gcc`: GCC structured artifact / raw text の ingest
- `diag_core`: Diagnostic IR、validation、fallback metadata
- `diag_enrich`: family / confidence / first-action / ownership などの分析
- `diag_render`: view model、layout、theme、TTY / CI 表示
- `diag_trace`: trace bundle と provenance
- `diag_testkit`: corpus fixture loader / validation
- `diag_cli_front`: wrapper CLI
- `xtask`: replay / snapshot / fuzz / human-eval / package / install / release 操作用

---

## 開発ルール

- **Issue が正本、prompt は派生物**  
  nightly agent に渡す prompt は Issue から生成する。
- **1 Issue = 1 PR = 1 主目的**
- **architecture first, then behavior**
- **contract change は docs / ADR / tests を同じ change に含める**
- **renderer を触る変更は default TTY 非劣化を自分で証明する**

詳しくは [EXECUTION-MODEL.md](EXECUTION-MODEL.md) を参照。

---

## 開発開始

```bash
cargo xtask check
cargo build --bin gcc-formed
./target/debug/gcc-formed --formed-self-check
cargo xtask replay --root corpus
```

Path-aware の実装が進んだら、band ごとの replay / snapshot / quality gate を追加で回す。  
個別の release / install / rollback / stable-promotion 手順は [packaging-runtime-operations-spec.md](packaging-runtime-operations-spec.md) と関連 runbook を正本とする。

---

## いま README に書かないもの

README は overview 専用である。  
次は README に詳細列挙しない。

- fallback reason の全列挙
- 現時点の xtask サブコマンド詳細
- すべての release artifact の完全な目録
- すべての known limitations の本文
- historic GCC 15-only wording の再掲

詳細はそれぞれの契約文書に置き、README は**方針と導線だけ**を保持する。

```

---

## 5.2 `SUPPORT-BOUNDARY.md`

```md
# Support Boundary

This document is the canonical wording for the current `v1beta` / `0.2.0-beta.N` vNext support posture.  
Keep `README.md`, release notes, support docs, contribution docs, and GitHub templates aligned with this wording.

---

## 1. Canonical vocabulary

### VersionBand

Compiler band used to reason about product scope.

- `GCC15+`
- `GCC13-14`
- `GCC9-12`
- `Unknown`

### ProcessingPath

Resolved execution path used by the wrapper.

- `DualSinkStructured`
- `SingleSinkStructured`
- `NativeTextCapture`
- `Passthrough`

### SupportLevel

Public quality claim for the current artifact line.

- `Preview`
- `Experimental`
- `PassthroughOnly`

### RawPreservationLevel

How much native / raw compiler output is preserved in the same run.

- `NativeAndStructuredSameRun`
- `StructuredOnlySameRun`
- `RawOnly`

---

## 2. Current `v1beta` / `0.2.0-beta.N` support boundary

- Linux first.
- `x86_64-unknown-linux-musl` is the primary production artifact.
- The terminal renderer is the primary user-facing surface.
- `GCC15+`, `GCC13-14`, and `GCC9-12` are all **in-scope product bands**.
- `GCC15+` is the primary fidelity reference path.
- `GCC13-14` and `GCC9-12` are **not** compatibility-only escape hatches.  
  They are product paths with narrower guarantees and different capture constraints.
- `ProcessingPath` and `RawPreservationLevel` may differ by band and by invocation.
- Raw fallback remains part of the shipped contract when the wrapper cannot produce a clearly better, trustworthy result.
- The wrapper must not knowingly ship a default TTY experience that is less legible than native GCC without either correcting the output or conservatively falling back / explicitly disclosing the limitation.

---

## 3. Current beta matrix

| VersionBand | Typical ProcessingPath | RawPreservationLevel | SupportLevel | Current expectation |
|---|---|---|---|---|
| `GCC15+` | `DualSinkStructured` | `NativeAndStructuredSameRun` | `Preview` | Highest-fidelity reference path |
| `GCC13-14` | `SingleSinkStructured`, `NativeTextCapture` | path-dependent; do not assume same-run native+structured | `Experimental` | Meaningful improvements where evidence is strong |
| `GCC9-12` | `SingleSinkStructured` (JSON), `NativeTextCapture` | path-dependent; do not assume same-run native+structured | `Experimental` | Wins on simple / type / linker / basic-template cases |
| `Unknown` | `Passthrough` | `RawOnly` | `PassthroughOnly` | Do not break the build or hide facts |

### Interpretive notes

- “first-class product band” means: present in specs, tests, issue taxonomy, quality gates, and roadmap.
- It does **not** mean that all bands have identical fidelity or identical raw-preservation guarantees.
- If a run resolves to `Passthrough`, that is still a valid shipped behavior when it is the most trustworthy choice.

---

## 4. Release-gate language

A beta or release-candidate build must be held if any of the following are true on representative fixtures:

1. default TTY output loses useful color, pointing, or severity signaling relative to native GCC without compensating user benefit
2. default TTY output becomes substantially longer than native GCC without improving first-fix behavior
3. template / overload / stdlib noise is not compressed enough to justify wrapping
4. the wrapper hides provenance, confidence, or compiler-owned facts
5. fallback behavior becomes opaque or misleading

---

## 5. Explicitly outside the current boundary

- Non-Linux production artifacts
- Claims that all VersionBands have identical guarantees
- Claims that every GCC diagnostic family is already improved
- Elimination of passthrough or raw fallback
- Stable / GA promises beyond what this document explicitly states

---

## 6. Required wording alignment

The following files must stay aligned with this document:

- `README.md`
- release notes
- bug report template
- pull request template
- support runbooks
- contribution docs
- any user-facing “current support” wording

If wording changes here, update those surfaces in the same change.

```

---

## 5.3 `implementation-bootstrap-sequence.md`

```md
# gcc-formed vNext 実装開始シーケンス

- **文書種別**: 実装順序契約
- **状態**: Draft for adoption
- **版**: `vNext-bootstrap-1`
- **日付**: 2026-04-09
- **関連文書**:
  - `SUPPORT-BOUNDARY.md`
  - `EXECUTION-MODEL.md`
  - `diagnostic-ir-v1alpha-spec.md`
  - `gcc-adapter-ingestion-spec.md`
  - `rendering-ux-contract-spec.md`
  - `quality-corpus-test-gate-spec.md`
  - `adr-initial-set/README.md`

---

## 1. 目的

この文書は、vNext で**何から実装してよいか**を固定するための順序契約である。  
ここで守りたいのは 1 つだけである。

> **GCC 15 を本線にした設計の上へ、そのまま Path B / Path C を継ぎ足してはならない。**

vNext の初手は feature 追加ではなく、**複数 ProcessingPath を受け止められる受け皿づくり**である。

---

## 2. 着手前の前提条件

次が揃うまで、新規 Epic を正式に起票してはならない。

1. `EXECUTION-MODEL.md` が承認されている
2. `SupportTier` を分解する ADR 群が承認されている
3. `README.md` / `SUPPORT-BOUNDARY.md` / GitHub templates の rewrite 方針が承認されている

---

## 3. 固定する実装順序

### Step 0. Delivery System Install

最初にやるのはコードではない。delivery system の導入である。

- `README.md`, `SUPPORT-BOUNDARY.md`, `.github` templates を vNext wording に置換する
- `CODEOWNERS` を導入する
- `gcc_formed_milestones_agent_playbook.md` を legacy 扱いにする
- GitHub Project / labels / fields / milestone の用語を `VersionBand` / `ProcessingPath` / `SupportLevel` に合わせる

**出口条件**

- repo の表の契約が GCC 15-only wording を引きずらない
- nightly agent が旧前提で誤作業しない

### Step 1. Capability model skeleton

- `SupportTier` を product の中心概念から外す
- `VersionBand`, `CapabilityProfile`, `ProcessingPath`, `RawPreservationLevel` を導入する
- downstream は `major >= 15` や `tier == A` を直接見ない

**出口条件**

- runtime decision が tier 中心でなく capability/path 中心になる
- まだ user-visible behavior は変えなくてよい

### Step 2. CaptureBundle / ingest abstraction

- capture runtime の返り値を `CaptureBundle` に寄せる
- adapter の入口を `CaptureBundle -> DiagnosticDocument` に寄せる
- `sarif_path + stderr_text` 固定の入口を新規に増やさない

**出口条件**

- Path A/B/C が同じ ingest 境界へ流れ込める
- この段階では no-behavior-change refactor を優先する

### Step 3. Render pipeline split and TTY non-regression

- analysis / view model / layout / theme を分離する
- default TTY の色・長さ・ノイズ量を gate にする
- `NativeTextCapture` を first-class path として扱える下地を作る

**出口条件**

- color regression と output inflation を test で再現・監視できる
- renderer を変えても analysis rule が壊れにくい

### Step 4. Path B skeleton (`GCC13-14`)

- `SingleSinkStructured` を first-class path として実装する
- 13/14 を compatibility-only wording で扱わない
- honesty / fallback / disclosure を path-aware にする

**出口条件**

- 13/14 が spec / tests / issue taxonomy 上で正式 path になる
- 最小限の user-visible render が返せる

### Step 5. Path C skeleton (`GCC9-12`)

- GCC JSON と `NativeTextCapture` を組み合わせた path を first-class 化する
- simple / type / linker / basic-template の改善を狙う
- “古い帯域だから対象外” という扱いをやめる

**出口条件**

- 9–12 が issue / corpus / quality gate 上で正式対象になる
- at least useful subset が定義される

### Step 6. Rulepack and compaction hardening

- family / first-action / compaction の規則を pipeline 化する
- ownership-aware template suppression を強化する
- renderer wording の変更が rule logic の破壊に直結しないようにする

**出口条件**

- template / overload / macro/include / linker の noisy cases が縮む
- rule 変更の保守コストが下がる

### Step 7. Path-aware quality gates

- band-aware / path-aware replay matrix を作る
- native non-regression, template suppression, fallback honesty を gate にする
- representative corpus を Path A/B/C で回す

**出口条件**

- “改善したつもり” をやめ、測定で出荷可否を決められる
- stop-ship 条件が CI に落ちる

### Step 8. Release widening and polish

- release docs / install docs / support runbooks を新アーキテクチャにそろえる
- public wording と実挙動のズレを解消する
- ここで初めて wider beta posture を検討する

**出口条件**

- docs / code / issue taxonomy が同じ語彙でそろう
- code 先行で contract が追随する状態を脱する

---

## 4. 初手でやってはならないこと

次を先に始めてはならない。

- `GCC 15 shadow -> SARIF parser -> render` の旧順序を再採用すること
- packaging / release automation の磨き込みを architecture より先に進めること
- Path B / Path C を “あとで入れる fallback” とみなすこと
- renderer の cosmetics だけを先にいじること
- no-behavior-change abstraction を飛ばして user-visible behavior を増やすこと

---

## 5. 最初の 6 本の PR の理想形

1. docs rewrite only
2. `VersionBand` / `CapabilityProfile` / `ProcessingPath` skeleton
3. `CaptureBundle` skeleton
4. render pipeline split + TTY regression tests
5. Path B skeleton
6. Path C skeleton

この 6 本が済むまで、nightly agent の同時並行度は抑える。

---

## 6. 成功条件

この bootstrap が成功したと見なす条件は次である。

- repo の public wording が GCC 15-only posture から脱している
- Path A/B/C を同じ概念系で語れる
- TTY default 非劣化が stop-ship gate になっている
- Path B/C が “将来の非本線” ではなく “今の設計対象” になっている
- 以後の Epic がこの順序を前提に切れる

```

---

## 5.4 `.github/pull_request_template.md`

```md
## Goal

-

## Why Now

-

## Parent Issue / Work Package

- Parent issue:
- Work package:
- Milestone:

## Workstream / Band / Layer

- Workstream:
- VersionBand:
- ProcessingPath(s):
- Layer:

## Change Classification

- [ ] Internal-only refactor
- [ ] Behavior change
- [ ] Contract change
- [ ] ADR required or updated
- Why this classification is correct:

## Read Docs

- [ ] `README.md`
- [ ] `SUPPORT-BOUNDARY.md`
- [ ] `EXECUTION-MODEL.md`
- [ ] Relevant ADR(s)
- [ ] Relevant spec(s)
- [ ] `CONTRIBUTING.md`
- [ ] Other:

## Contract Surfaces

- [ ] CLI surface
- [ ] Probe / capture / environment contract
- [ ] IR schema semantics / machine output
- [ ] Analysis / rule semantics
- [ ] Renderer wording / color / disclosure / budgets
- [ ] Support boundary / runbooks / templates
- [ ] Packaging / install / rollback / release
- [ ] No contract surface changed

## In Scope

-

## Out Of Scope

-

## Acceptance Criteria

-

## Evidence

### Commands Run

- [ ] `cargo xtask check`
- [ ] `cargo test --workspace`
- [ ] `cargo xtask replay --root corpus`
- [ ] Path-specific smoke / snapshot commands are listed below
- [ ] Other:

### Reports / traces / screenshots

-

## Path Impact

- [ ] `GCC15+`
- [ ] `GCC13-14`
- [ ] `GCC9-12`
- [ ] `Unknown` / passthrough only
- ProcessingPath selection changed:
- RawPreservationLevel changed:
- Support wording changed:

## Non-Negotiables

- [ ] fail-open remains intact
- [ ] compiler-owned facts / provenance are not hidden
- [ ] default TTY is not made less legible than native
- [ ] no undocumented widening or narrowing of support boundary
- [ ] issue / docs / tests were updated together when contract changed

## Docs Updated

- [ ] `README.md`
- [ ] `SUPPORT-BOUNDARY.md`
- [ ] `EXECUTION-MODEL.md`
- [ ] Relevant spec(s)
- [ ] ADR(s)
- [ ] Issue template / PR template / runbook wording
- [ ] No docs update needed

## Human Review Requested

- [ ] Quick
- [ ] Deep
- [ ] Design

## Risk / Rollback

- Risk:
- Rollback plan:

```

---

## 5.5 `.github/ISSUE_TEMPLATE/bug_report.yml`

```yaml
name: Bug report
description: Report a bug with VersionBand, ProcessingPath, fallback, and UX context.
title: "[bug] "
labels:
  - bug
body:
  - type: markdown
    attributes:
      value: |
        Before filing, check the support docs:

        - [Support boundary](https://github.com/horiyamayoh/gcc-formed/blob/main/SUPPORT-BOUNDARY.md)
        - [Support overview](https://github.com/horiyamayoh/gcc-formed/blob/main/SUPPORT.md)
        - [Execution model](https://github.com/horiyamayoh/gcc-formed/blob/main/EXECUTION-MODEL.md)
        - [Incident triage](https://github.com/horiyamayoh/gcc-formed/blob/main/docs/runbooks/incident-triage.md)
        - [Trace bundle collection](https://github.com/horiyamayoh/gcc-formed/blob/main/docs/runbooks/trace-bundle-collection.md)
        - [Rollback / reinstall](https://github.com/horiyamayoh/gcc-formed/blob/main/docs/runbooks/rollback.md)

        Please file bugs in terms of **VersionBand** and **ProcessingPath**, not old support-tier wording.
  - type: dropdown
    id: version_band
    attributes:
      label: Version band
      description: Which compiler band was involved?
      options:
        - GCC 15+
        - GCC 13-14
        - GCC 9-12
        - Unknown / other
    validations:
      required: true
  - type: dropdown
    id: processing_path
    attributes:
      label: Processing path
      description: If known, which path was resolved for this run?
      options:
        - DualSinkStructured
        - SingleSinkStructured
        - NativeTextCapture
        - Passthrough
        - Unknown
    validations:
      required: true
  - type: dropdown
    id: surface
    attributes:
      label: User surface
      options:
        - TTY renderer
        - CI renderer
        - Probe / capture
        - Analysis / compaction
        - Raw fallback / disclosure
        - Packaging / install / release
    validations:
      required: true
  - type: checkboxes
    id: symptoms
    attributes:
      label: Symptoms
      options:
        - label: Color or pointing was worse than native GCC
        - label: Output became longer without helping the first fix
        - label: Template / stdlib / overload noise was not suppressed enough
        - label: Root cause or first action looked wrong
        - label: Provenance / raw fallback / confidence was hidden or misleading
        - label: Wrapper crashed, stalled, or silently changed path
  - type: textarea
    id: summary
    attributes:
      label: What happened?
      description: Describe the actual behavior and the expected behavior.
    validations:
      required: true
  - type: textarea
    id: reproduction
    attributes:
      label: Reproduction
      description: Include the exact compiler invocation or `gcc-formed` command.
      placeholder: |
        gcc-formed ...
        # or the exact compiler command that reproduces the issue
    validations:
      required: true
  - type: textarea
    id: expected_behavior
    attributes:
      label: What should have happened instead?
      description: Focus on first-fix usability, compression, honesty, and fallback behavior.
    validations:
      required: true
  - type: textarea
    id: attachments
    attributes:
      label: Attached artifacts
      description: Paste paths, snippets, or filenames for attached evidence.
      placeholder: |
        trace.json:
        stderr.raw:
        diagnostics.sarif:
        diagnostics.json:
        ir.analysis.json:
        tty transcript / screenshot:
        --formed-self-check output:
    validations:
      required: true
  - type: textarea
    id: environment
    attributes:
      label: Environment
      description: Include OS / distro, terminal, wrapper version, compiler version, target triple, locale, and any environment variables that may affect output.
      placeholder: |
        OS / distro:
        terminal:
        gcc version:
        gcc-formed version:
        target triple:
        locale:
        NO_COLOR / CLICOLOR / TERM:
    validations:
      required: true

```

---

## 5.6 `.github/CODEOWNERS`

```text
# Default owner for the repository.
* @horiyamayoh

# Architecture and core diagnostic path ownership.
/diag_backend_probe/ @horiyamayoh
/diag_capture_runtime/ @horiyamayoh
/diag_adapter_gcc/ @horiyamayoh
/diag_core/ @horiyamayoh
/diag_enrich/ @horiyamayoh
/diag_render/ @horiyamayoh
/diag_trace/ @horiyamayoh
/diag_testkit/ @horiyamayoh
/diag_cli_front/ @horiyamayoh
/xtask/ @horiyamayoh

# Contract docs and process control.
/adr-initial-set/ @horiyamayoh
/*.md @horiyamayoh
/.github/ @horiyamayoh
/.github/workflows/ @horiyamayoh
/.github/CODEOWNERS @horiyamayoh

```

---

## 5.7 legacy banner for `gcc_formed_milestones_agent_playbook.md`

```md
# Legacy notice for `gcc_formed_milestones_agent_playbook.md`

Insert the following block at the top of the existing playbook until the file is removed or superseded:

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

```

---

## 6. この rewrite が満たすべき acceptance checks

### 6.1 wording drift checks

次の grep で旧前提が消えていることを確認する。

```bash
grep -R "compatibility-only" README.md SUPPORT-BOUNDARY.md .github || true
grep -R "Support tier" .github || true
grep -R "primary enhanced-render path" README.md SUPPORT-BOUNDARY.md .github || true
```

### 6.2 contract alignment checks

- `README.md` と `SUPPORT-BOUNDARY.md` が `VersionBand` / `ProcessingPath` / `SupportLevel` を同じ意味で使っている
- `implementation-bootstrap-sequence.md` が `GCC 15 shadow` 起点の順序を捨てている
- PR template と bug template が旧 `SupportTier` 語彙を要求しない
- `CODEOWNERS` に docs / workflows / core crates の所有者が定義されている
- old playbook を planning authority と誤読しないための legacy note が入っている

### 6.3 GitHub template validity checks

GitHub issue form は YAML と GitHub form schema に従う必要があるため、  
PR 作成前に `bug_report.yml` の構文と重複 label を確認する。

---

## 7. この rewrite の非目標

この rewrite は、まだ次をやらない。

- `gcc-adapter-ingestion-spec.md` の本格改訂
- `rendering-ux-contract-spec.md` の line budget / color / disclosure 詳細改訂
- `quality-corpus-test-gate-spec.md` の path-aware gate 改訂
- 実装コードの path selection 変更
- nightly queue 用の Issue 自動生成

それらはこの rewrite の**次**に進める。

---

## 8. 推奨 landing order

1. `EXECUTION-MODEL.md`
2. contract docs rewrite（本書の 6 ファイル + legacy banner）
3. ADR split (`adr-0026` 以降)
4. `CapabilityProfile` / `ProcessingPath` skeleton 実装
5. `CaptureBundle` skeleton
6. render pipeline split + TTY non-regression tests

---

## 9. 一言で言うと

> **repo の表の顔が GCC 15-only のままだと、内部設計を直しても運用が旧前提へ引き戻す。**
>
> **だから、Epic より先に、README と support wording と templates を直す。**

---

## Appendix A. 出力ファイル一覧

このパックには、bundle 文書に加えて ready-to-copy draft files も同梱している。

- `gcc-formed-vnext-contract-docs/README.md`
- `gcc-formed-vnext-contract-docs/SUPPORT-BOUNDARY.md`
- `gcc-formed-vnext-contract-docs/implementation-bootstrap-sequence.md`
- `gcc-formed-vnext-contract-docs/.github/pull_request_template.md`
- `gcc-formed-vnext-contract-docs/.github/ISSUE_TEMPLATE/bug_report.yml`
- `gcc-formed-vnext-contract-docs/.github/CODEOWNERS`
- `gcc-formed-vnext-contract-docs/gcc_formed_milestones_agent_playbook.legacy-banner.md`


## Appendix B. 参照した現行 repo surfaces と platform docs

### 現行 repo の置換対象

- `README.md`
- `SUPPORT-BOUNDARY.md`
- `implementation-bootstrap-sequence.md`
- `.github/pull_request_template.md`
- `.github/ISSUE_TEMPLATE/bug_report.yml`

### GitHub platform docs

- Issue forms syntax
- GitHub form schema
- CODEOWNERS

この rewrite pack の draft は、上記の現行 surfaces を vNext 語彙へ置換しつつ、Issue form YAML と CODEOWNERS の現行 GitHub 仕様に沿うように書いている。
