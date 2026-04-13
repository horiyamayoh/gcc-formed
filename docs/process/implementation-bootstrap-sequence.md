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

# gcc-formed vNext 実装開始シーケンス

- **文書種別**: 実装順序契約
- **状態**: Accepted baseline for current adoption
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
`EXECUTION-MODEL.md` と change design が方針を定め、本書はその順序を operational checklist として固定する。

support posture や architecture rationale を再定義する文書ではない。  
理由づけや doctrine の読み直しが必要な場合は `EXECUTION-MODEL.md` と `docs/architecture/gcc-formed-vnext-change-design.md` を先に参照する。

---

## 2. 着手前の前提条件

次が揃うまで、新規 Epic を正式に起票してはならない。

1. `EXECUTION-MODEL.md` が承認されている
2. legacy tier model を分解する ADR 群が承認されている
3. `README.md` / `SUPPORT-BOUNDARY.md` / GitHub templates の rewrite 方針が承認されている

---

## 3. 固定する実装順序

### Step 0. Delivery System Install

最初にやるのはコードではない。delivery system の導入である。

- `README.md`, `SUPPORT-BOUNDARY.md`, `.github` templates を vNext wording に置換する
- `CODEOWNERS` を導入する
- `../history/planning/gcc_formed_milestones_agent_playbook.md` を legacy 扱いにする
- GitHub Project / labels / fields / milestone の用語を `VersionBand` / `ProcessingPath` / `SupportLevel` に合わせる

**出口条件**

- repo の表の契約が GCC 15-only wording を引きずらない
- nightly agent が旧前提で誤作業しない

### Step 1. Capability model skeleton

- legacy tier model を product の中心概念から外す
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
- 13/14 を legacy narrow-path wording で扱わない
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
