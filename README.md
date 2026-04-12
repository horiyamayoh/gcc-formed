---
doc_role: current-authority
lifecycle_status: accepted-baseline
audience: both
use_for: Repo overview and current reading order.
do_not_use_for: Historical provenance or low-level implementation details.
supersedes: []
superseded_by: []
---
> [!IMPORTANT]
> Authority: `current-authority` / `accepted-baseline`
> Use for: Repo overview and current reading order.
> Do not use for: Historical provenance or low-level implementation details.

# gcc-formed

> GCC diagnostic UX wrapper for GCC 9-15 that keeps terminal output shorter, root-cause-first, and fail-open.

> **30秒サマリ**
> Before: `error: no matching function for call to 'combine(int, const char [2])'`
> After: `error: template instantiation failed` と `help:` / `why:` から読める
> Fail-open: 改善しきれない run は raw diagnostics をそのまま返す

- **状態**: Public Beta
- **成熟度ラベル**: `v1beta`
- **artifact semver 系列**: `0.2.0-beta.N`
- **一般利用向け安定版**: 未提供
- **日付**: 2026-04-11
- **位置づけ**: doctrine-driven / spec-first / multi-path diagnostic UX wrapper

`gcc-formed` は、GCC をラップし、C/C++ のコンパイルエラーやリンクエラーを**より短く、より根因に近く、より誠実に**提示するためのリポジトリである。

目標は GCC の生出力を単に prettier にすることではない。  
目標は、**GCC 9〜15 にまたがる複数の capture / ingest 経路を持ちながら、1 つの UX 原則で価値を返すこと**である。

AI コーディングエージェント向けの入口は [AGENTS.md](AGENTS.md) である。

---

## 30秒でわかる Before / After

既存の corpus snapshot と fail-open fixture から短く抜粋する。  
README では価値の方向が 30 秒で伝わることを優先し、細部は出典の artifact を参照する。

### 1. テンプレートエラー（C++）

出典: [GCC raw](corpus/cpp/template/case-05/snapshots/gcc15/stderr.raw) / [gcc-formed render](corpus/cpp/template/case-05/snapshots/gcc15/render.default.txt)

**Before (GCC raw)**

```text
src/main.cpp: In function 'int main()':
src/main.cpp:5:12: error: no matching function for call to 'combine(int, const char [2])'
    5 |     combine(1, "x");
      |     ~~~~~~~^~~~~~~~
src/main.cpp:2:6: note: candidate 1: 'template<class T> void combine(T, T)'
src/main.cpp:2:6: note: template argument deduction/substitution failed:
src/main.cpp:5:12: note:   deduced conflicting types for parameter 'T' ('int' and 'const char*')
```

**After (gcc-formed)**

```text
error: template instantiation failed
--> src/main.cpp:5:5
help: start from the first user-owned template frame and match template arguments
why: no matching function for call to 'combine(int, const char [2])'
| src/main.cpp:5:5
|     combine(1, "x");
|     ^
while instantiating:
  - src/main.cpp:2:6 candidate 1: 'template<class T> void combine(T, T)'
  - src/main.cpp:2:6 template argument deduction/substitution failed:
  - src/main.cpp:5:5 deduced conflicting types for parameter 'T' ('int' and 'const char*')
raw: rerun with --formed-profile=raw_fallback to inspect the original compiler output
```

### 2. リンカーエラー（C）

出典: [GCC raw](corpus/c/linker/case-02/snapshots/gcc15/stderr.raw) / [gcc-formed render](corpus/c/linker/case-02/snapshots/gcc15/render.default.txt)

**Before (GCC raw)**

```text
main.c:(.text+0x5): undefined reference to `missing_symbol'
collect2: error: ld returned 1 exit status
```

**After (gcc-formed)**

```text
note: some compiler details were not fully structured; original diagnostics are preserved
error: undefined reference to `missing_symbol`
help: define the missing symbol or link the object/library that provides it
why: main.c:(.text+0x5): undefined reference to `missing_symbol'
linker: symbol `missing_symbol`
raw:
  main.c:(.text+0x5): undefined reference to `missing_symbol'
other errors:
  - error: linker reported a failure
raw: rerun with --formed-profile=raw_fallback to inspect the original compiler output
```

### 3. 改善しない方が誠実なケース（passthrough / fail-open）

出典: [passthrough fixture](fuzz/cases/residual-ansi-passthrough/stderr.txt)

改善が trustworthy でない run では、`gcc-formed` は無理に要約しない。  
その場合の default 出力は raw diagnostics をそのまま保ち、事実を隠さない。

```text
C:\bad\path\helper.obj: undefined reference to `missing_symbol'
note: [31mnot an ANSI escape, but still noisy residual text
collect2: error: ld returned 1 exit status
```

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

SupportLevel / ProcessingPath / RawPreservationLevel の正本は [docs/support/SUPPORT-BOUNDARY.md](docs/support/SUPPORT-BOUNDARY.md) に置く。  
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

1. [docs/support/SUPPORT-BOUNDARY.md](docs/support/SUPPORT-BOUNDARY.md)  
   現在の public wording と beta support posture の正本
2. [docs/process/EXECUTION-MODEL.md](docs/process/EXECUTION-MODEL.md)  
   Epic を切る前提、Issue 正本主義、nightly agent 運用の正本
3. [docs/specs/diagnostic-ir-v1alpha-spec.md](docs/specs/diagnostic-ir-v1alpha-spec.md)  
   正規化 IR の実装契約
4. [docs/specs/gcc-adapter-ingestion-spec.md](docs/specs/gcc-adapter-ingestion-spec.md)  
   capture / ingest の実装契約
5. [docs/specs/rendering-ux-contract-spec.md](docs/specs/rendering-ux-contract-spec.md)  
   renderer と disclosure の実装契約
6. [docs/specs/quality-corpus-test-gate-spec.md](docs/specs/quality-corpus-test-gate-spec.md)  
   corpus-driven quality gate の実装契約
7. [adr-initial-set/README.md](adr-initial-set/README.md)  
   採択済み ADR の索引
8. [docs/policies/VERSIONING.md](docs/policies/VERSIONING.md) / [docs/policies/GOVERNANCE.md](docs/policies/GOVERNANCE.md)  
   成熟度ラベル、artifact 系列、変更分類の用語契約

全体の文書索引は [docs/README.md](docs/README.md) を参照。

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

- [docs/support/SUPPORT-BOUNDARY.md](docs/support/SUPPORT-BOUNDARY.md): public wording と support posture の正本
- [docs/support/PUBLIC-SURFACE.md](docs/support/PUBLIC-SURFACE.md): repo landing / release body / GitHub About metadata の正本
- [docs/process/EXECUTION-MODEL.md](docs/process/EXECUTION-MODEL.md): delivery system の正本
- [docs/specs/diagnostic-ir-v1alpha-spec.md](docs/specs/diagnostic-ir-v1alpha-spec.md): IR 契約
- [docs/specs/gcc-adapter-ingestion-spec.md](docs/specs/gcc-adapter-ingestion-spec.md): capture / ingest 契約
- [docs/specs/rendering-ux-contract-spec.md](docs/specs/rendering-ux-contract-spec.md): terminal / CI renderer 契約
- [docs/specs/quality-corpus-test-gate-spec.md](docs/specs/quality-corpus-test-gate-spec.md): quality gate 契約
- [docs/specs/packaging-runtime-operations-spec.md](docs/specs/packaging-runtime-operations-spec.md): packaging / install / rollback / release engineering 契約
- [docs/process/implementation-bootstrap-sequence.md](docs/process/implementation-bootstrap-sequence.md): 実装開始順の正本
- [docs/releases/PUBLIC-BETA-RELEASE.md](docs/releases/PUBLIC-BETA-RELEASE.md): 公開 beta artifact の install / rollback / exact-pin 契約
- [adr-initial-set/README.md](adr-initial-set/README.md): ADR 索引
- [docs/policies/VERSIONING.md](docs/policies/VERSIONING.md): 成熟度ラベルと artifact semver
- [docs/policies/GOVERNANCE.md](docs/policies/GOVERNANCE.md): 変更分類と freeze ルール

### 実装ワークスペース

- `diag_backend_probe`: VersionBand / CapabilityProfile の解決
- `diag_capture_runtime`: child spawn、capture、artifact 収集
- `diag_adapter_contract`: compiler adapter trait と共通 ingest 契約
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

詳しくは [docs/process/EXECUTION-MODEL.md](docs/process/EXECUTION-MODEL.md) を参照。

---

## 開発開始

```bash
cargo xtask check
cargo build --bin gcc-formed
./target/debug/gcc-formed --formed-self-check
cargo xtask replay --root corpus
```

`cargo xtask check` は、`cargo fmt --check`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test --workspace`、representative replay、Python の `ci/test_*.py` contract suite を同じ標準 developer gate として順に実行する。

Path-aware の実装が進んだら、band ごとの replay / snapshot / quality gate を追加で回す。  
個別の release / install / rollback / stable-promotion 手順は [docs/specs/packaging-runtime-operations-spec.md](docs/specs/packaging-runtime-operations-spec.md) と関連 runbook を正本とする。

公開 beta artifact の install / rollback / exact-pin は [docs/releases/PUBLIC-BETA-RELEASE.md](docs/releases/PUBLIC-BETA-RELEASE.md) を参照。

## Operator Quickstart for Make / CMake

The current lab-proven build-system insertion path is direct `CC` / `CXX` replacement. Start there, and if you need one cache / remote-exec launcher, place it behind the wrapper with `FORMED_BACKEND_LAUNCHER`.

### Make

```bash
export CC=gcc-formed
export CXX=g++-formed
export FORMED_BACKEND_GCC="$(command -v gcc)"
make -j
```

Optional single backend launcher:

```bash
export CC=gcc-formed
export CXX=g++-formed
export FORMED_BACKEND_GCC="$(command -v gcc)"
export FORMED_BACKEND_LAUNCHER="/absolute/path/to/ccache"
make -j
```

### CMake

```bash
cmake -S . -B build -G "Unix Makefiles" \
  -DCMAKE_C_COMPILER=gcc-formed \
  -DCMAKE_CXX_COMPILER=g++-formed
cmake --build build -j
```

If the wrapper is not yet proven for a build, fall back to raw `gcc` / `g++` for that build or use `--formed-mode=passthrough` on a direct invocation. Do not put ccache / distcc / sccache-style launchers in front of the wrapper, and do not build a multi-launcher chain.

The full topology policy is in [docs/support/OPERATOR-INTEROP.md](docs/support/OPERATOR-INTEROP.md); rollback and raw-fallback instructions remain in [docs/releases/PUBLIC-BETA-RELEASE.md](docs/releases/PUBLIC-BETA-RELEASE.md).

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
