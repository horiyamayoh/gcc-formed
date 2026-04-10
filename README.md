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

Representative corpus / replay gates でも、`GCC9-12` は `NativeTextCapture` と explicit `SingleSinkStructured` (JSON) を別 path として扱う。

| VersionBand | 典型的な ProcessingPath | 現在の beta support level | 位置づけ |
|---|---|---|---|
| GCC 15+ | `DualSinkStructured` | `Preview` | 最高 fidelity の reference path |
| GCC 13–14 | `SingleSinkStructured` / `NativeTextCapture` | `Experimental` | in-scope product path。reference path より保証は狭い |
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

vNext では、repo の主語を単一 tier から外し、次の 4 概念に分ける。

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
- [PUBLIC-BETA-RELEASE.md](PUBLIC-BETA-RELEASE.md): 公開 beta artifact の install / rollback / exact-pin 契約
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

公開 beta artifact の install / rollback / exact-pin は [PUBLIC-BETA-RELEASE.md](PUBLIC-BETA-RELEASE.md) を参照。

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
