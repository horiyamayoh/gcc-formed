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
> After (default `subject_blocks_v1`): `error: [type_mismatch] type or overload mismatch` と `want:` / `got:` / `via:` から読める
> Fail-open: 改善しきれない run は raw diagnostics をそのまま返す

- **状態**: Public Beta
- **成熟度ラベル**: `v1beta`
- **artifact semver 系列**: `0.2.0-beta.N`
- **一般利用向け安定版**: 未提供
- **日付**: 2026-04-13
- **位置づけ**: doctrine-driven / spec-first / multi-path diagnostic UX wrapper

`gcc-formed` は、GCC をラップし、C/C++ のコンパイルエラーやリンクエラーを**より短く、より根因に近く、より誠実に**提示するためのリポジトリである。

目標は GCC の生出力を単に prettier にすることではない。  
目標は、**GCC 9〜15 にまたがる複数の capture / ingest 経路を持ちながら、1 つの UX 原則で価値を返すこと**である。

AI コーディングエージェント向けの入口は [AGENTS.md](AGENTS.md) である。
機械可読の public surface は [docs/specs/public-machine-readable-diagnostic-surface-spec.md](docs/specs/public-machine-readable-diagnostic-surface-spec.md) に別契約として置く。

---

## 30秒でわかる Before / After

既存の corpus snapshot と fail-open fixture から短く抜粋する。  
README では価値の方向が 30 秒で伝わることを優先し、細部は出典の artifact を参照する。

Presentation V2 の `subject_blocks_v1` は beta runtime default であり、no-config の terminal render は subject-first blocks を使う。  
rollout は `docs / ADR -> opt-in preset -> corpus / snapshot / review -> default promotion` の gate を通した。  
`legacy_v1` は explicit rollback preset として残しており、`[render] presentation = "legacy_v1"` または `--formed-presentation=legacy_v1` で切り戻せる。  
`cascade.max_expanded_independent_roots` は visible-root cap としては deprecated であり、新しい visible-root behavior は `render.presentation` または `render.presentation_file.session.visible_root_mode` で表現する。  
representative corpus は `snapshots/.../subject_blocks_v1/` に review 用 cluster を持てるが、`render.presentation.json` は internal artifact であり public contract ではない。

### 1. テンプレートエラー（C++）

出典: [GCC raw](corpus/cpp/overload/case-01/snapshots/gcc15/stderr.raw) / [default render](corpus/cpp/overload/case-01/snapshots/gcc15/render.default.txt)

**Before (GCC raw)**

```text
src/main.cpp: In function 'int main()':
src/main.cpp:5:10: error: too few arguments to function 'void takes(int, int)'
    5 |     takes(1);
      |     ~~~~~^~~
src/main.cpp:1:6: note: declared here
    1 | void takes(int, int) {}
      |      ^~~~~
```

**After (default `subject_blocks_v1`)**

```text
error: [type_mismatch] type or overload mismatch @ src/main.cpp:5:5
help: compare the expected type and actual argument at the call site
want: int, int
got : int
via : void takes(int, int) @ src/main.cpp:1:6  +3 candidates
raw: rerun with --formed-profile=raw_fallback to inspect the original compiler output
```

### 2. リンカーエラー（C）

出典: [GCC raw](corpus/c/linker/case-01/snapshots/gcc15/stderr.raw) / [default render](corpus/c/linker/case-01/snapshots/gcc15/render.default.txt)

**Before (GCC raw)**

```text
helper.c:(.text+0x0): multiple definition of `duplicate'; /tmp/cczB1U1i.o:main.c:(.text+0x0): first defined here
collect2: error: ld returned 1 exit status
```

**After (default `subject_blocks_v1`)**

```text
error: [linker] multiple definition of `duplicate`
help  : remove the duplicate definition or make the symbol internal to one translation unit
symbol: duplicate
from  : helper.c:(.text+0x0)
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

operator guidance は [docs/support/OPERATOR-INTEROP.md](docs/support/OPERATOR-INTEROP.md)、既知の制約は [docs/support/KNOWN-LIMITATIONS.md](docs/support/KNOWN-LIMITATIONS.md) を参照する。  
Trace bundle の収集と replay は [docs/runbooks/trace-bundle-collection.md](docs/runbooks/trace-bundle-collection.md) / [docs/runbooks/trace-bundle-replay.md](docs/runbooks/trace-bundle-replay.md) を正本とする。

## Public Machine-Readable Export

CI、agent、wrapper integration が診断を機械可読で消費するときは、terminal text や internal trace を scrape せず、`--formed-public-json` を使う。

```bash
gcc-formed --formed-public-json=artifacts/diagnostic.json -c src/main.c
```

stdout を JSON 専用チャネルとして安全に使える invocation では `--formed-public-json=-` も使える。

```bash
gcc-formed --formed-public-json=- -c src/main.c | jq '.execution.version_band'
```

この surface の正本は [docs/specs/public-machine-readable-diagnostic-surface-spec.md](docs/specs/public-machine-readable-diagnostic-surface-spec.md) であり、internal IR や trace bundle の代替 public contract ではない。presentation preset や terminal header grammar が今後変わっても、machine consumer は terminal text を scrape せずこの surface を使う。

---

## この repo の読み方

上から下へ読む。

1. [docs/support/SUPPORT-BOUNDARY.md](docs/support/SUPPORT-BOUNDARY.md)  
   現在の public wording と beta support posture の正本
2. [docs/README.md](docs/README.md)  
   文書群の authority map と配置ルール
3. [docs/architecture/gcc-formed-vnext-change-design.md](docs/architecture/gcc-formed-vnext-change-design.md)  
   vNext architecture baseline と migration design
4. [docs/process/EXECUTION-MODEL.md](docs/process/EXECUTION-MODEL.md)  
   Epic を切る前提、Issue 正本主義、nightly agent 運用の正本
5. [docs/specs/diagnostic-ir-v1alpha-spec.md](docs/specs/diagnostic-ir-v1alpha-spec.md)  
   正規化 IR の実装契約
6. [docs/specs/gcc-adapter-ingestion-spec.md](docs/specs/gcc-adapter-ingestion-spec.md)  
   capture / ingest の実装契約
7. [docs/specs/rendering-ux-contract-spec.md](docs/specs/rendering-ux-contract-spec.md)  
   renderer と disclosure の実装契約
8. [docs/specs/public-machine-readable-diagnostic-surface-spec.md](docs/specs/public-machine-readable-diagnostic-surface-spec.md): public JSON export の実装契約
9. [docs/specs/quality-corpus-test-gate-spec.md](docs/specs/quality-corpus-test-gate-spec.md): corpus-driven quality gate の実装契約
10. [adr-initial-set/README.md](adr-initial-set/README.md): 採択済み ADR の索引
11. [docs/policies/VERSIONING.md](docs/policies/VERSIONING.md) / [docs/policies/GOVERNANCE.md](docs/policies/GOVERNANCE.md): 成熟度ラベル、artifact 系列、変更分類の用語契約

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
- [docs/architecture/gcc-formed-vnext-change-design.md](docs/architecture/gcc-formed-vnext-change-design.md): top-level architecture baseline
- [docs/process/EXECUTION-MODEL.md](docs/process/EXECUTION-MODEL.md): delivery system の正本
- [docs/specs/diagnostic-ir-v1alpha-spec.md](docs/specs/diagnostic-ir-v1alpha-spec.md): IR 契約
- [docs/specs/gcc-adapter-ingestion-spec.md](docs/specs/gcc-adapter-ingestion-spec.md): capture / ingest 契約
- [docs/specs/rendering-ux-contract-spec.md](docs/specs/rendering-ux-contract-spec.md): terminal / CI renderer 契約
- [docs/specs/public-machine-readable-diagnostic-surface-spec.md](docs/specs/public-machine-readable-diagnostic-surface-spec.md): public JSON export 契約
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
cargo xtask ci-gate --workflow pr
cargo build --bin gcc-formed
./target/debug/gcc-formed --formed-self-check
cargo xtask replay --root corpus
```

`cargo xtask check` は、`cargo fmt --check`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test --workspace`、representative replay、Python の `ci/test_*.py` contract suite を同じ標準 developer gate として順に実行する。

GitHub CI 相当の gate をローカルで回すときは `cargo xtask ci-gate --workflow <pr|nightly|rc>` を使う。`nightly` は `--matrix-lane gcc12|gcc13|gcc14|gcc15|all` を取り、出力は既定で `target/local-gates/<workflow>/` に隔離する。

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

`--formed-self-check` and the runtime notices use the same current-vocabulary operator guidance. The self-check output keeps a shared `summary`, `representative_limitations`, `actionable_next_steps`, and `c_first_focus_areas`; the same band-specific next-step wording is documented in [docs/support/OPERATOR-INTEROP.md](docs/support/OPERATOR-INTEROP.md).

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
