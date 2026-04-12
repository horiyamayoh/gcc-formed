---
doc_role: current-authority
lifecycle_status: accepted-baseline
audience: both
use_for: Current implementation contract for this surface.
do_not_use_for: Historical baseline or superseded path assumptions.
supersedes: []
superseded_by: []
---
> [!IMPORTANT]
> Authority: `current-authority` / `accepted-baseline`
> Use for: Current implementation contract for this surface.
> Do not use for: Historical baseline or superseded path assumptions.

# gcc-formed GCC Adapter / Ingestion 仕様書

- **文書種別**: 内部仕様書（実装契約）
- **状態**: Accepted Baseline
- **版**: `1.0.0-alpha.1`
- **対象**: `gcc-formed` / 将来の `cc-formed`
- **主用途**: GCC 呼び出し・診断捕捉・構造化取り込み・安全フォールバックの契約固定
- **想定実装**: Linux first / GCC first / 品質最優先
- **関連文書**:
  - `../architecture/gcc-formed-vnext-change-design.md`
  - `diagnostic-ir-v1alpha-spec.md`
  - `../process/implementation-bootstrap-sequence.md`
  - `../support/SUPPORT-BOUNDARY.md`
- **関連 ADR**:
  - `adr-initial-set/adr-0001-wrapper-first-entrypoint.md`
  - `adr-initial-set/adr-0006-fail-open-fallback-and-provenance.md`
  - `adr-initial-set/adr-0011-locale-policy-english-first-reduced-fallback.md`
  - `adr-initial-set/adr-0013-sarif-egress-scope.md`
  - `adr-initial-set/adr-0014-linker-diagnostics-via-staged-text-adapter.md`
  - `adr-initial-set/adr-0016-trace-bundle-content-and-redaction.md`
  - `adr-initial-set/adr-0019-render-modes.md`
  - `adr-initial-set/adr-0026-capability-profile-replaces-support-tier.md`
  - `adr-initial-set/adr-0027-processing-path-separate-from-support-level.md`
  - `adr-initial-set/adr-0028-capturebundle-only-ingest-entry.md`
  - `adr-initial-set/adr-0029-path-b-and-c-are-first-class-product-paths.md`

---

## 1. この文書の目的

本仕様書は、`gcc-formed` が **GCC をどう呼び出し**、**どの出力をどの優先順位で取り込み**、**どこで安全にフォールバックするか**を固定する。

この文書は単なる「ラッパーの実装メモ」ではない。  
本プロダクトにおいて adapter / ingestion は、以下の 4 つを同時に満たさなければならない。

1. **既存の build flow をほぼ壊さずに差し込めること**
2. **compiler が知っている structured facts を最大限取れること**
3. **wrapper が壊れても native compiler 体験へ安全に戻れること**
4. **将来の Clang adapter を壊さない境界になること**

したがって本仕様の関心は、CLI 表面の小さな使い勝手よりも、以下にある。

- 子プロセス起動契約
- 入力引数と環境変数の扱い
- structured diagnostics の authoritative source
- raw stderr / structured artifact / exit status の整合
- external tool（assembler / linker / driver）の扱い
- capture artifact, provenance, integrity issue
- fail-open policy
- version band / capability / path downgrade 条件

---

## 2. 規範語

本仕様では以下の意味で規範語を使う。

- **MUST**: 必須
- **MUST NOT**: 禁止
- **SHOULD**: 強い推奨
- **SHOULD NOT**: 強い非推奨
- **MAY**: 任意

---

## 3. 本仕様で置く前提

本仕様は以下の公開事実を前提に置く。

1. **GCC 15** では `-fdiagnostics-add-output=` により複数の診断 output sink を同時に使え、通常の text 出力を維持しつつ追加で SARIF をファイル出力できる。さらに `-fdiagnostics-format=json` は deprecated で、機械可読診断は SARIF が推奨されている。 [R1][R2]
2. **GCC 13** で SARIF 出力が追加され、`-fdiagnostics-format=sarif-stderr` / `sarif-file`、および `json-stderr` / `json-file` が導入された。 [R3]
3. `-fdiagnostics-add-output=` の `sarif` scheme は `file=...` と `version=2.1` を持ち、`2.2-prerelease` と `experimental-html` は実験的扱いである。 [R2]
4. `-fdiagnostics-parseable-fixits`、`GCC_EXTRA_DIAGNOSTIC_OUTPUT`、`GCC_DIAGNOSTICS_LOG`、`EXPERIMENTAL_SARIF_SOCKET` など、診断 capture に影響する環境変数・出力経路が存在する。特に `EXPERIMENTAL_SARIF_SOCKET` は接続できないと compiler が即失敗する。 [R6]
5. GCC は `LC_MESSAGES` と `LC_CTYPE` を参照し、`LC_ALL` はそれらを上書きする。診断メッセージの言語は `LC_MESSAGES` に、文字分類は `LC_CTYPE` に影響される。 [R6]
6. `sarif-replay` / `libgdiagnostics` は GCC の configure オプションに依存する optional component であり、製品の中核依存にはできない。 [R7]
7. `-fdiagnostics-format=sarif-*` を使うと、`-ftime-report` 情報が SARIF 内の JSON として出力されうる。つまり adapter は「SARIF は diagnostic result だけが入る」と仮定してはならない。 [R8]

---

## 4. 設計上の最重要判断

この文書で固定する最重要判断を先に明示する。

### 4.1 正本モデルは multi-band / multi-path

**現行 vNext の adapter 正本は、`VersionBand` と `ProcessingPath` を分離して扱う。**

- `GCC15+` は最良の reference path であり、`DualSinkStructured` を最も安全に使える
- `GCC13-14` は first-class beta product band であり、default は `NativeTextCapture`、explicit structured path は `SingleSinkStructured`
- `GCC9-12` も in-scope product band であり、`NativeTextCapture` と explicit `SingleSinkStructured` (JSON) を path-aware に扱う
- どの band でも raw fallback と provenance 保持を契約に含める

### 4.2 authoritative source は path-aware に選ぶ

`DualSinkStructured` では、**GCC 自身の diagnostics facts は SARIF を一次ソース**とする。  
ただし external tool（assembler / linker / driver 外部サブプロセス）の text は SARIF に含まれないことがありうるため、**raw stderr は常に同時に保持**する。

`SingleSinkStructured` では structured artifact を一次ソースとして使ってよいが、same-run native+structured preservation を前提にしてはならない。

### 4.3 raw stderr は常に capture する

structured source が使える場合でも、**raw stderr を捨ててはならない**。  
用途は以下。

- wrapper failure 時の fail-open fallback
- external tool diagnostics 取り込み
- provenance
- diff / debugging
- corpus / regression fixture

### 4.4 GCC 13–14 は first-class beta band だが GCC 15 と同一ではない

**GCC 13–14 は v1beta の first-class beta band** とする。  
default は `NativeTextCapture` で、`SingleSinkStructured` は explicit opt-in path として扱う。  
ただし、GCC 15+ のような same-run native+structured の保証はないため、**fidelity claim は GCC15 と分離したまま**にする。

理由:

- Band B は issue / test / gate / corpus 上で first-class に扱う必要がある
- `NativeTextCapture` は honest fallback と bounded render を両立しやすい
- `SingleSinkStructured` は explicit structured coverage を与えられるが、raw native preservation の保証は同一ではない
- hot path で JSON を core 依存にすると、GCC 15 以降の方向と逆行する

### 4.5 JSON は path-aware に限定して使う

GCC 13–14 の JSON は **offline corpus importer / fixture normalizer** としては許容する。  
GCC 9–12 の explicit `SingleSinkStructured` では JSON を許容してよい。  
しかし **default hot path の中心概念** を JSON にしてはならない。

### 4.6 `sarif-replay` や `libgdiagnostics` に依存しない

これらは optional build artifact であり、配布先の GCC build に存在する保証がない。  
したがって wrapper の core path は **自前で SARIF を parse し、自前で render / fallback する**。

### 4.7 locale の安定化は `LC_MESSAGES` だけを触る

render mode で deterministic な英語 diagnostics が必要な場合でも、**wrapper は `LC_ALL` を設定してはならない**。  
`LC_ALL` は `LC_CTYPE` も変えてしまい、文字境界や multibyte 文字の扱いに副作用を持つからである。  
必要なら `LC_MESSAGES=C` を設定し、`LC_CTYPE` は preserve する。 [R6]

### 4.8 experimental feature は hot path で使わない

以下は v1alpha hot path で **MUST NOT**:

- `EXPERIMENTAL_SARIF_SOCKET` [R6]
- `sarif:version=2.2-prerelease` [R2]
- `experimental-html` [R2]
- `cfgs=yes`, `state-graphs=yes` のような debug/experimental sink extensions [R2]

---

## 5. 非目標

本仕様は以下を goal にしない。

1. GCC 全バージョンで同一品質の wrapper rendering を提供すること
2. generic text parser で GCC front-end diagnostics を完全再構成すること
3. linker diagnostics を compiler diagnostics と同レベルに lossless 構造化すること
4. `gcc` の全 diagnostic-formatting options と完全互換にすること
5. `sarif-replay` による native text 再現を製品の必須経路にすること
6. editor/LSP 連携の transport をここで固定すること
7. build 全体の multi-invocation aggregation をこの文書だけで定義すること

---

## 6. スコープ

### 6.1 扱うもの

- GCC driver compatible CLI の起動契約
- child process の stdout / stderr / exit status / signal
- structured sidecar (SARIF) の生成と取り込み
- raw stderr capture
- version band / capability profile 判定
- capture artifact の保存・削除
- external tool diagnostics の ingress
- fallback / passthrough policy
- `DiagnosticDocument` 生成前の ingestion pipeline

### 6.2 扱わないもの

- IR の意味論そのもの（`diagnostic-ir-v1alpha-spec.md` を参照）
- ranking / summarization / renderer UX の詳細
- public SARIF export policy
- IDE 向け transport / daemon
- organization 固有 linker パターンの完全 taxonomy

---

## 7. コンポーネント境界

本仕様が定義する adapter / ingestion 層の責務を明示する。

```text
user/build system
    │
    ▼
[gcc-formed CLI front]
    │
    ├─ resolve backend compiler
    ├─ classify invocation
    ├─ choose version band / processing path / mode
    ├─ sanitize env / inject flags
    ├─ spawn child
    ├─ capture stderr + sidecar artifacts
    │
    ▼
[CaptureBundle]
    │
    ▼
[gcc adapter ingestion]
    │
    ├─ parse supported structured artifacts (SARIF first)
    ├─ classify raw stderr residuals
    ├─ emit IngestReport
    ├─ map to DiagnosticDocument facts
    │
    ▼
[IR validator]
    │
    ├─ success -> renderer/enrichment
    └─ fatal issue -> passthrough / raw fallback
```

adapter / ingestion 層の責務は **facts の収集と正規化まで** である。  
root-cause ranking や headline 生成は後段であり、adapter が勝手に意味づけしてはならない。

### 7.1 normative ingress boundary

adapter / ingestion 層の **normative** な入口は次とする。

```rust
ingest_bundle(bundle: &CaptureBundle, policy: IngestPolicy) -> IngestReport
```

ここで:

- `CaptureBundle` は invocation metadata、resolved processing path、raw text artifacts、structured artifacts、exit status、integrity metadata を運ぶ
- `IngestPolicy` は少なくとも `producer` と `run` を持つ
- `IngestReport` は少なくとも `document`, `source_authority`, `confidence_ceiling`, `fallback_grade`, `warnings` を返す

`ingest(sarif_path, stderr_text, ...)` / `ingest_with_reason(...)` は **compatibility wrapper only** とし、normative boundary と見なしてはならない。

### 7.2 source authority / fallback grade semantics

`IngestReport` の意味は以下で固定する。

- `source_authority=structured`: supported structured artifact が authoritative source として受理された
- `source_authority=residual_text`: residual text が事実上の一次入力になった
- `source_authority=none`: authoritative structured source も residual text も実質的に得られなかった

- `fallback_grade=none`: intended source path のまま ingest できた
- `fallback_grade=compatibility`: residual-only / unsupported-structured compatibility path で conservative ingest した
- `fallback_grade=fail_open`: authoritative structured source の欠落または parse failure から raw preservation に fail-open した

`confidence_ceiling` は downstream が「どこまで断定してよいか」を判断する上限であり、source authority / fallback grade に矛盾してはならない。

---

## 8. サポートマトリクス

### 8.1 バージョン別 path matrix

| Path | VersionBand | default mode | primary structured source | raw native preservation | rollout 位置づけ |
|---|---|---|---|---|---|
| A | `GCC15+` | `render` / `shadow` / `passthrough` | `-fdiagnostics-add-output=sarif:file=...` | same-run で保持可能 | primary fidelity reference path |
| B | `GCC13-14` | `render` on `NativeTextCapture`; explicit `SingleSinkStructured`; `shadow` / `passthrough` for conflicts | native text or `-fdiagnostics-format=sarif-file` | path-dependent / same-run native+structured は保証しない | first-class beta product path |
| C | `GCC9-12` / `Unknown` | `passthrough` or `NativeTextCapture`; explicit `SingleSinkStructured` only when capability/profile permits | `json-file` / `json-stderr` or none | path-dependent / raw 保持を優先 | first-class beta product path or passthrough-first |

### 8.2 プラットフォーム前提

- **MUST**: Linux を第一級対象とする
- **MAY**: 将来の macOS / Windows は、本仕様の概念を踏襲して別 adapter 仕様で補う
- **MUST NOT**: Linux 向け hot path に platform-specific optional dependency を持ち込む

### 8.3 「対応」の意味

本仕様で「対応」と言うとき、それは次の 3 レベルを区別する。

1. **enhanced render path**: wrapper が証拠を保持したまま path-aware UX を返してよい
2. **safe passthrough / native-text path**: wrapper が前面に立っても native compiler 体験を壊さない
3. **corpus / research path**: 研究・比較・fixture 収集用の retained artifact を残してよい

GCC 15+ は (1)(2)(3)。  
GCC 13–14 は v1beta でも first-class beta として (1)(2)(3) を持つが、`SingleSinkStructured` は explicit policy と retained evidence を要する。  
GCC 9–12 は原則 (2)(3) を中心とし、(1) は capability/profile と quality gate が揃う path に限る。

---

## 9. モード定義

adapter は少なくとも以下の mode を持つ。

### 9.1 `render`

- user には wrapper render を見せる
- child stderr は即時表示せず capture する
- structured path が使える場合はそれを優先
- fatal ingestion failure 時は raw fallback へ fail-open

### 9.2 `shadow`

- user には native compiler stderr を見せる
- wrapper は同時に capture / parse / trace を行う
- rollout, corpus 収集, 比較評価用
- shadow では **user-visible behavior を極力変えない**

### 9.3 `passthrough`

- wrapper は compiler をほぼそのまま実行する
- support level 不足、hard conflict、explicit opt-out 時の安全モード
- 必要なら最低限の trace だけ残してよい

### 9.4 `force-structured-experimental`

- user が明示的に要求した場合のみ
- GCC 13–14 で `sarif-file` を使う等、safe fallback を一部犠牲にする実験モード
- rollout default にしてはならない

### 9.5 mode 選択原則

- **default は fail-open**
- `force-*` mode は fail-open を弱めるので opt-in に限る
- query/introspection invocation では mode より bypass 判定が優先する

---

## 10. Backend 解決と invocation 分類

### 10.1 backend compiler 解決

wrapper は以下の順で backend compiler を決める。

1. explicit CLI / config 指定
2. 呼び出し名に基づく既定
   - `gcc-formed` → `gcc`
   - `g++-formed` → `g++`
3. PATH 探索

解決時の規則:

- **MUST** shell を介さず `execve` 相当で起動する
- **MUST** `realpath` を取得し、trace には実パスを記録する
- **SHOULD** inode / mtime / file size を probe cache key に使う
- **MUST NOT** `@response-file` を wrapper 側で展開する
- **MUST** 作業ディレクトリは caller のものをそのまま使う

### 10.2 invocation class

adapter は argv を以下のクラスに分類する。

1. **compile-like**  
   診断を出しうる通常の compile / assemble / link / preprocess invocation

2. **introspection-like**  
   `--help`, `--version`, `-dump*`, `-print-*`, `-###` 等、compiler の情報表示が主目的の invocation

3. **wrapper-bypass-required**  
   user が sink topology を明示指定している、または wrapper との衝突が強い invocation

### 10.3 introspection-like の扱い

以下に該当する場合、wrapper は原則 bypass する。

- source compilation が主目的でない
- 出力の大部分が diagnostics ではなく tool information である
- structured sidecar を足す合理性がない

代表例:

- `--help`
- `--version`
- `-dumpmachine`
- `-dumpversion`
- `-dumpfullversion`
- `-print-search-dirs`
- `-print-prog-name=*`
- `-print-file-name=*`
- `-###`

**MUST**: これらに wrapper render を重ねない。  
**MAY**: debug trace だけ残す。

### 10.4 capability probe

adapter は compiler binary ごとに capability を判断する。

推奨実装:

- first use で version probe
- 結果を `(realpath, inode, mtime, size)` 等に紐付けてキャッシュ
- 少なくとも以下を保持:
  - compiler kind (`gcc`, `g++`, vendor-patched gcc-like 等)
  - version string
  - parsed major/minor
  - version band / capability profile / support level
  - known flag support assumptions

**SHOULD**: hot path で毎回余分な probe process を起動しない。

### 10.5 mode / path 決定アルゴリズム

擬似コードで示す。

```text
resolve backend
classify invocation

if invocation is introspection-like:
    bypass

probe capabilities (cached)
detect hard conflicts
detect explicit user mode

if explicit mode == passthrough:
    passthrough

if hard conflict:
    passthrough

if path == A:
    if explicit mode == shadow:
        shadow
    else:
        render

if path == B:
    if explicit mode == force-structured-experimental:
        force-structured-experimental
    else if explicit mode == shadow:
        shadow
    else:
        render on NativeTextCapture

if path == C:
    if explicit mode == force-structured-experimental and capability supports structured:
        force-structured-experimental
    else:
        passthrough or conservative NativeTextCapture
```

---

## 11. child process 契約

### 11.1 引数 forwarding

- **MUST** semantic compile options をそのまま渡す
- **MUST NOT** response file を展開しない
- **MUST NOT** shell quoting を再解釈しない
- **MUST** wrapper 所有の injected options は argv 末尾に付与する
- **MUST** hard conflict がある場合、silent override せず downgrade する

### 11.2 stdin

- default では child にそのまま継承する
- wrapper は stdin を eager read してはならない
- `-` を入力 source とする compile でも挙動を変えない

### 11.3 stdout

- **MUST** default で passthrough する
- **MUST NOT** compile artifact の可能性がある stdout を adapter が解釈・整形しない
- **SHOULD NOT** default で stdout をフル capture しない
- **MAY** debug/trace mode で opt-in capture を提供する

理由:

- `-E`, `-M*`, `-print-*` など stdout は build artifact / metadata として意味を持つ
- stdout capture を常時行うと deadlock / memory / ordering リスクが増える

### 11.4 stderr

mode ごとの規則:

| mode | child stderr user表示 | adapter 内部 capture | 備考 |
|---|---|---|---|
| `render` | 即時表示しない | MUST | wrapper が最終判断後に表示 |
| `shadow` | native を即時表示 | MUST (tee) | rollout/corpus 比較 |
| `passthrough` | native | MAY | trace 無効なら単純継承でよい |
| `force-structured-experimental` | 即時表示しない | MUST | fallback 劣化あり |

stderr capture 実装規則:

- **MUST** non-blocking / 専用 reader task / thread で drain する
- **MUST** memory ではなく spool file を一次保管に使える設計にする
- **MUST NOT** child が stderr pipe で詰まる設計にする

### 11.5 exit status と signal

- child が通常終了した場合、wrapper は **同じ exit code を返す MUST**
- child が signal で終了した場合、wrapper は **可能なら同じ signal で終了する SHOULD**
- 同じ signal を再送できない実装では `128 + signo` 等の conventional code を返してよいが、trace に signal 情報を残す **MUST**

### 11.6 wrapper 内部失敗時の扱い

child 実行後に wrapper 側で parse/render/validation が失敗した場合:

- **MUST** child exit status を優先して保持する
- **MUST** user に何らかの diagnostics を見せる
- **MUST NOT** compiler failure を wrapper failure に置き換えて握りつぶす
- **SHOULD** raw stderr を verbatim に近い形で表示し、wrapper failure は一行注記に留める

---

## 12. 環境変数と locale ポリシー

### 12.1 基本原則

- semantic compile behavior に影響する環境は preserve が原則
- diagnostics capture を壊す環境だけを最小限 sanitize する
- `passthrough` では sanitize を極力行わない
- `render` と `shadow` では capture 汚染を避けるための限定 sanitization を許可する

### 12.2 環境変数ポリシー表

| 変数 | `render` | `shadow` | `passthrough` | 理由 |
|---|---|---|---|---|
| `LC_ALL` | preserve | preserve | preserve | wrapper が上書きすると `LC_CTYPE` まで変わる |
| `LC_MESSAGES` | default で `C` に設定可 | preserve 推奨 | preserve | 言語安定化のみを狙う |
| `LC_CTYPE` | preserve | preserve | preserve | 文字境界・multibyte 扱いを壊さない |
| `LANG` | preserve | preserve | preserve | locale 全体の既定値は極力尊重 |
| `GCC_DIAGNOSTICS_LOG` | unset | unset | preserve | capture 汚染防止 [R6] |
| `GCC_EXTRA_DIAGNOSTIC_OUTPUT` | unset | unset | preserve | parseable fixit 混入防止 [R6] |
| `EXPERIMENTAL_SARIF_SOCKET` | unset | unset | preserve | fail-fast 実験機能を避ける [R6] |
| `GCC_COLORS` | preserveだが child flags で上書き可 | preserve | preserve | render では flag 側で制御 |
| `GCC_URLS` / `TERM_URLS` | preserveだが child flags で上書き可 | preserve | preserve | render では escape-free capture を優先 |
| `TMPDIR` | preserve | preserve | preserve | GCC 自身の temp 戦略を壊さない |

### 12.3 locale 安定化ポリシー

`render` mode の default では以下を推奨する。

- `LC_MESSAGES=C`
- `LC_CTYPE` は preserve
- `LC_ALL` は触らない

理由:

- message text の英語安定化は得たい
- しかし `LC_CTYPE` を変えると multibyte 文字の境界や分類に影響しうる [R6]

### 12.4 `LC_ALL` が既に設定されている場合

user / CI が `LC_ALL` を既に設定している場合:

- wrapper はそれを勝手に unset してはならない
- その場合 message text の deterministic English を保証しない
- text-dependent heuristic は confidence を下げるか無効化する

---

## 13. 診断 formatting option の衝突ポリシー

### 13.1 基本原則

wrapper は compile semantics の owner ではない。  
しかし **diagnostic sink topology** に関しては wrapper 自身が owner になる。

したがって、user が明示的に sink topology を指定している場合、wrapper は silent override してはならない。

### 13.2 hard conflict

以下の指定が user argv に存在する場合、`auto` mode では **MUST passthrough** とする。

- `-fdiagnostics-format=*`
- `-fdiagnostics-add-output=*`
- `-fdiagnostics-set-output=*`
- `-fdiagnostics-parseable-fixits`
- `-fdiagnostics-generate-patch`

理由:

- これらは output sink / stderr payload / sidecar topology を変える
- wrapper の capture 契約と直接衝突する

### 13.3 soft conflict

以下は user が指定していても compile correctness は壊さないため、`render` mode では wrapper が無視または正規化してよい。

- `-fdiagnostics-color=*`
- `-fdiagnostics-urls=*`
- `-fmessage-length=*`
- `-fdiagnostics-show-caret` / `-fno-diagnostics-show-caret`
- `-fdiagnostics-show-line-numbers` / inverse
- `-fdiagnostics-column-unit=*`
- `-fdiagnostics-column-origin=*`
- `-fdiagnostics-show-template-tree`
- `-fno-elide-type`
- `-fdiagnostics-text-art-charset=*`
- `-fdiagnostics-show-context`

ただし:

- `shadow` mode では **preserve SHOULD**
- `TTY render` mode では、user が color policy を明示していなければ native color を preserve してよい
- `non-TTY` / `CI` では color-preserving flags を追加してはならない
- `render` mode で wrapper が上書きした場合、その事実は trace に残す **SHOULD**

### 13.4 衝突時の user-visible ルール

- default では静かに passthrough してよい
- verbose/debug mode では「diagnostic sink conflict のため passthrough した」と記録してよい
- user が `force-*` を明示した場合のみ warning を出して強行してよい

---

## 14. capture artifact モデル

### 14.1 一時ディレクトリ

各 invocation ごとに private temp dir を作る。

要件:

- **MUST** `0700` 相当の権限
- **MUST** symlink race を避ける
- **MUST** unique path
- **MUST** `-fdiagnostics-add-output=` の値に使えるよう、**comma を含まない path** を生成する [R2]
- **SHOULD** 空白・制御文字・`=` も避ける
- **SHOULD** base dir は `--diag-tmpdir` > wrapper config > `TMPDIR` > system default の順で選ぶ

### 14.2 artifact 一覧

最低限、以下の artifact ID を定義する。

| artifact id | 必須 | 内容 |
|---|---|---|
| `stderr.raw` | render/shadow で MUST | child stderr の生 bytes |
| `diagnostics.sarif` | Path A render/shadow で MUST | GCC 追加出力 sidecar |
| `invocation.json` | SHOULD | backend path, argv, selected mode, env subset |
| `trace.json` | SHOULD | capability, timing, integrity issues |
| `stdout.raw` | default off | opt-in のみ |

### 14.3 retention policy

adapter は以下の retention policy をサポートする **SHOULD**。

- `never`
- `on-wrapper-failure`
- `on-child-error`
- `always`

default policy は本仕様では固定しない。  
ただし production local default は `on-wrapper-failure` か `never` が望ましい。  
CI / shadow rollout では `on-child-error` が有効になりうる。

### 14.4 サイズ制限

- **MUST NOT** 無制限にメモリへ積む
- **SHOULD** spool file に逃がす
- **SHOULD** configurable hard cap を持つ
- cap 超過時は **MUST** `IntegrityIssue` を記録する
- truncation は **MUST** silent であってはならない

### 14.5 provenance 連携

IR に出る各 root / child diagnostic は、可能な限り少なくとも 1 つ以上の `capture_ref` を持つ **MUST**。  
`wrapper_generated` のみで原始証拠に辿れない node を乱造してはならない。

---

## 15. Path A: GCC 15+ dual-sink structured reference path

### 15.1 目的

GCC 15+ では、**single-pass で GCC text stderr と SARIF sidecar を同時取得** し、  
GCC 所有診断は structured facts として、external tool 診断は raw residual として取り込む。

### 15.2 注入する child flags

`render` mode では、hard conflict がない限り以下を argv 末尾に追加する。

```text
-fdiagnostics-add-output=sarif:version=2.1,file=<tmp>/diagnostics.sarif
-fdiagnostics-color=always
-fdiagnostics-urls=never
-fmessage-length=0
```

補足:

- `version=2.1` を固定し、`2.2-prerelease` は使わない [R2]
- `color=always` は TTY render path で native compiler color を保全するための安全な注入であり、user が color policy を指定している場合は上書きしない
- `urls=never` は raw capture を escape-free にするため
- `message-length=0` は raw fallback / residual grouping の安定化のため

### 15.3 child 実行フロー

```text
prepare temp dir
resolve backend
sanitize env
spawn child
  stdout -> passthrough
  stderr -> pipe -> spool file
wait child exit
close stderr capture
assemble CaptureBundle
ingest_bundle(bundle, policy)
validate
if fatal failure:
    raw fallback
else:
    hand off to renderer
```

### 15.4 SARIF parser 成功条件

Path A render path で structured ingestion を「成功」とみなす条件:

1. `diagnostics.sarif` が存在する
2. JSON として parse できる
3. SARIF version が受理可能 (`2.1.0` 相当) である
4. 少なくとも `run/results` を走査できる
5. mapping に必要な fields を読める
6. `DiagnosticDocument` が IR validator を通る

### 15.5 SARIF parser の hot-path 方針

- **MUST** full JSON schema validation を hot path で必須にしない
- **MUST** 必要最小限の構造検証 + graceful degradation とする
- **SHOULD** unknown properties / extra payload を無視できる
- **MUST** `-ftime-report` 由来の追加 JSON を想定し、結果配列以外のノイズで壊れない [R8]

### 15.6 GCC 所有診断の authoritative source

Path A では以下を原則とする。

- GCC front-end / middle-end / analyzer / diagnostic subsystem の facts → **SARIF authoritative**
- raw stderr は provenance / fallback / residual 用
- raw text の wording が SARIF とズレても、facts は SARIF を優先

### 15.7 raw stderr residual parser の役割

Path A render path の raw stderr parser は **generic GCC front-end text parser ではない**。  
役割は以下に限る。

- linker
- assembler
- gcc driver fatal
- `collect2` summary
- internal compiler error banner の補助情報
- unclassified residual blob の capture

### 15.8 この path でやってはいけないこと

- raw stderr から generic `file:line:col:` GCC diagnostics を再構成し、SARIF より優先する
- SARIF にない location を source 読みで捏造する
- external residual parser で GCC 本体 diagnostics を重複生成する

---

## 16. Path A: shadow path

### 16.1 目的

shadow は rollout / corpus / A/B 比較のための mode であり、**user-visible output を極力 native GCC に近づける**。

### 16.2 flag policy

Path A shadow では、原則として次だけを追加する。

```text
-fdiagnostics-add-output=sarif:version=2.1,file=<tmp>/diagnostics.sarif
```

追加で `color=never` などを強制するかは policy 次第だが、**default は preserve SHOULD**。

### 16.3 stderr

- child stderr は user へそのまま流す
- 同時に spool file に tee する
- wrapper parse failure は user-visible output を変えない

### 16.4 shadow で許されること / 許されないこと

許される:

- capture
- parse
- trace
- corpus 保存
- render candidate の非表示生成

許されない:

- native stderr の suppression
- mode fail で user-visible error を増やすこと
- compiler exit semantics の変更

---

## 17. Path B: GCC 13–14 product path

### 17.1 default path policy

GCC 13–14 の default は以下。

- default TTY / local path は `NativeTextCapture` を優先してよい
- `render` / `shadow` / `passthrough` は path-aware policy で選んでよい
- `SingleSinkStructured` は explicit policy と retained evidence がある場合に使ってよい

### 17.2 dual-sink 前提を持ち込まない理由

GCC 13–14 には SARIF はあるが、`-fdiagnostics-format=sarif-file` は main sink を置き換える。  
GCC 15 のような dual-sink がないため、structured facts を取る path と native text を保つ path を同一 run で同時保証できない。[R1][R3]

したがって Path B では、

- local default では `NativeTextCapture`
- CI / retained artifact / explicit opt-in では `SingleSinkStructured`
- hard conflict や unsupported shape では `passthrough`

という path-aware 切り分けを採る。

### 17.3 `SingleSinkStructured` policy

明示的 opt-in、CI profile、fixture 収集、または retained artifact を前提とする運用で、以下を許可してよい。

```text
-fdiagnostics-format=sarif-file
```

または equivalent policy。

ただし:

- same-run native text fallback は保証しない
- wrapper failure 時は retained SARIF path / raw residual / preserved artifact への導線が主になる
- user の default TTY path を silent に置き換えてはならない
- quality gate では `SingleSinkStructured` と `NativeTextCapture` を別 path として扱う

### 17.4 JSON の扱い

- `json-stderr` / `json-file` は存在するが [R3]
- **MUST NOT** Path B の唯一の hot-path 既定値にしない
- **MAY** offline importer / fixture translator / explicit `SingleSinkStructured` path に使う

---

## 18. Path C: GCC 9–12 / unknown gcc-like product-or-passthrough path

### 18.1 基本方針

- default は `passthrough` または `NativeTextCapture`
- explicit JSON path が capability/profile と quality gate を満たす場合だけ `SingleSinkStructured` を使ってよい
- wrapper render は raw evidence と provenance を失わない範囲でのみ行ってよい

### 18.2 理由

- structured path の可用性・安定性は Path A/B より弱い
- text parser を中核に据えると将来負債になる
- それでも raw capture、compaction、ranking、honest fallback で返せる価値はある

### 18.3 corpus 収集

Path C でも shadow/raw capture により corpus は集めてよい。  
その corpus は **比較資料であると同時に product hardening の入力**であり、将来の quality gate 昇格材料として扱う。

---

## 19. SARIF ingestion 詳細契約

### 19.1 受理する SARIF

- version は `2.1.x` を受理対象とする
- GCC 15 production path は `version=2.1` を指定して出力させる [R2]
- prerelease variant は production で使わない

### 19.2 parser 実装方針

- **MUST** streaming / bounded-memory parse が可能な設計にする
- **MUST** UTF-8 JSON を扱える
- **SHOULD** pretty-printed JSON でも compact JSON でも同じに扱える
- **MUST** unknown property bag に耐える
- **SHOULD** required subset のみ厳格に扱う

### 19.3 run / result の扱い

adapter は SARIF の全機能を網羅する必要はないが、少なくとも以下を読む。

- `runs[]`
- `results[]`
- `message`
- `level`
- `ruleId` または rule metadata
- `locations[]`
- `relatedLocations[]`（あれば）
- GCC 固有 property bag / include chain / location relationship（あれば）
- artifact URI / physical location 情報

### 19.4 path, stack, extra payload

- analyzer path 相当があれば `ContextChain(kind=analyzer_path)` に落とす
- 読めない payload は破棄してよいが、capture は残す
- path 情報を捨てるなら `IntegrityIssue` ではなく `partial coverage` として trace に残す

### 19.5 URI / path 解決

- relative path は invocation の working directory を基準に解決する
- path normalization はするが、元の表記も保持する **SHOULD**
- symlink realpath で勝手に書き換えない
- source ownership 判定のための prefix 判定は adapter の責務外にしてよい

### 19.6 構造欠損時の扱い

例:

- location なし
- message はあるが rule id なし
- result はあるが physical location が不完全

この場合:

- 読める範囲で IR に落とす
- completeness を `partial` / `passthrough` に落とす
- parse 全体を即 fatal にしない
- ただし document が validator を通らないなら render path を諦める

---

## 20. raw stderr residual classifier

### 20.1 目的

raw stderr residual classifier は、structured path の外側にある text を **控えめに** 取り込む。  
目標は「全部を parse すること」ではなく、「明らかに tool-origin が分かる重要なものだけを高信頼で IR 化すること」である。

### 20.2 v1alpha で対象にする family

最低限、以下を対象にする。

1. `preprocessor_directive`
2. `syntax`
3. `template`
4. `type_overload`
5. `scope_declaration`
6. `redefinition`
7. `deleted_function`
8. `concepts_constraints`
9. `unused`
10. `return_type`
11. `fallthrough`
12. `sanitizer_buffer`
13. `format_string`
14. `uninitialized`
15. `overflow_arithmetic`
16. `enum_switch`
17. `analyzer`
18. `null_pointer`
19. `move_semantics`
20. `strict_aliasing`
21. `abi_alignment`
22. `storage_class`
23. `exception_handling`
24. `attribute`
25. `odr_inline_linkage`
26. `sizeof_allocation`
27. `conversion_narrowing`
28. `const_qualifier`
29. `pointer_reference`
30. `access_control`
31. `inheritance_virtual`
32. `constexpr`
33. `lambda_closure`
34. `lifetime_dangling`
35. `init_order`
36. `coroutine`
37. `module_import`
38. `deprecated`
39. `pedantic_compliance`
40. `driver_fatal`
41. `linker.undefined_reference`
42. `linker.multiple_definition`
43. `linker.cannot_find_library`
44. `linker.file_format_or_relocation`
45. `collect2_summary`
46. `assembler_error`
47. `internal_compiler_error_banner`
48. `passthrough`
49. `ranges_views`
50. `structured_binding`
51. `designated_init`
52. `three_way_comparison`
53. `asm_inline`
54. `openmp`
55. `thread_safety`
56. `string_character`
57. `bit_field_packed`

### 20.3 classifier の安全原則

- **MUST** explicit tool prefix、structured context、または強い lexical anchor を要求する
- **MUST NOT** weak / open-ended な generic GCC text diagnostics を推定で family 化する
- **MUST NOT** location を捏造する
- **SHOULD** confidence を family ごとに固定または narrow range で出す
- **MAY** `preprocessor_directive` / `openmp` / `scope_declaration` / `redefinition` / `deleted_function` / `concepts_constraints` / `unused` / `return_type` / `fallthrough` / `sanitizer_buffer` / `format_string` / `uninitialized` / `overflow_arithmetic` / `enum_switch` / `analyzer` / `null_pointer` / `move_semantics` / `strict_aliasing` / `asm_inline` / `bit_field_packed` / `abi_alignment` / `thread_safety` / `storage_class` / `exception_handling` / `attribute` / `odr_inline_linkage` / `sizeof_allocation` / `conversion_narrowing` / `const_qualifier` / `pointer_reference` / `access_control` / `inheritance_virtual` / `constexpr` / `lambda_closure` / `lifetime_dangling` / `init_order` / `coroutine` / `module_import` / `deprecated` / `pedantic_compliance` / `ranges_views` / `structured_binding` / `designated_init` / `three_way_comparison` / `string_character` のような high-precision compiler residual family を明示 wording で分類する
- **MAY** symbol / archive / object file 名を抽出する

### 20.4 grouping 規則

start line の例:

- `/usr/bin/ld: ...`
- `ld: ...`
- `ld.bfd: ...`
- `ld.gold: ...`
- `collect2: error: ...`
- `as: ...`
- `gcc: fatal error: ...`
- `cc1: internal compiler error: ...`
- `cc1plus: internal compiler error: ...`

grouping:

- start line から次の start line 直前までを 1 block とする
- indented line / continuation line は block に含める
- blank line は block 継続にしてよい
- file/function context line は child / note / chain に落としてよい

### 20.5 linker summary の扱い

`collect2: error: ld returned ...` のような summary は、直前に linker block 群があれば **独立 root にしない SHOULD**。  
代わりに:

- 直前 linker root の child/note に畳む
- または document-level summary metadata に吸収する

### 20.6 unclassified residual

分類できなかった text は捨てない。

- `origin = unknown`
- `phase = unknown`
- `node_completeness = passthrough`
- raw capture ref を持つ residual node または document residual として保持する

---

## 21. merge / dedup 規則

### 21.1 merge sources

document 生成時の入力源は最大 3 つ。

1. structured SARIF facts
2. raw stderr residual parsed diagnostics
3. wrapper-generated integrity / fallback diagnostics

### 21.2 優先順位

- GCC-owned diagnostic facts: structured > raw
- external tool facts: raw > wrapper-generated
- wrapper-generated は最下位

### 21.3 dedup 原則

dedup は **保守的に** 行う。

dedup してよいのは、少なくとも以下が揃うときのみ。

- same origin family
- same normalized primary location（または location absent）
- same normalized message core
- same phase

それ以外は **別 diagnostic として残す**。

### 21.4 `collect2` と linker 本体の重複

`collect2` summary は本体 linker error と重複しやすい。  
v1alpha では、summary-only 行は基本的に root ではなく child/note 化する。

### 21.5 empty document 防止

child exit 非ゼロなのに diagnostic roots が 0 件になった場合:

- **MUST** wrapper-generated generic diagnostic を 1 件作る
- message 例: `compilation failed but no renderable diagnostics were captured`
- provenance は raw artifact と child exit info を指す

---

## 22. fallback / downgrade マトリクス

### 22.1 代表ケース

| ケース | 動作 |
|---|---|
| hard conflict | passthrough |
| introspection invocation | bypass |
| Path A, SARIF ok, validator ok | render |
| Path A, SARIF missing, raw に高信頼 linker あり | render（linker-only） |
| Path A, SARIF invalid, raw stderr あり | raw fallback |
| Path A, renderer crash | raw fallback |
| Path B default (`NativeTextCapture`) | render or passthrough |
| Path B explicit `SingleSinkStructured`, parse fail | raw residual + retained SARIF path、必要なら wrapper warning |
| Path C default | passthrough or conservative render |
| child signaled | raw fallback + signal propagate |
| stderr truncated | render 可なら render、ただし integrity issue を必ず付与 |

### 22.2 raw fallback の要件

raw fallback は以下を満たす **MUST**。

- user が compiler failure の主要情報を失わない
- child exit status を保持する
- raw stderr を極力そのまま出す
- wrapper 側の失敗説明は短く添えるだけにする

### 22.3 fallback でやってはいけないこと

- wrapper の stack trace を user-facing stderr に全面展開する
- child stderr を捨てて「wrapper internal error」だけを出す
- child exit status を wrapper 独自エラーコードに置換する

---

## 23. injected flag set の詳細

### 23.1 Path A render canonical set

```text
-fdiagnostics-add-output=sarif:version=2.1,file=<tmp>/diagnostics.sarif
-fdiagnostics-color=never
-fdiagnostics-urls=never
-fmessage-length=0
```

### 23.2 Path A shadow canonical set

```text
-fdiagnostics-add-output=sarif:version=2.1,file=<tmp>/diagnostics.sarif
```

### 23.3 Path B explicit structured canonical set

```text
-fdiagnostics-format=sarif-file
```

注記:

- `sarif-file` の出力先は GCC の既定ファイル名規則に依存するため、wrapper は artifact discovery を実装するか、explicit structured path 専用の working layout を定める必要がある
- この制約があるため、Path B では `SingleSinkStructured` と `NativeTextCapture` を distinct path として運用する

### 23.4 hot path で使わないもの

- `json-*`
- `experimental-html`
- `sarif:version=2.2-prerelease`
- `cfgs=yes`
- `state-graphs=yes`

---

## 24. driver / linker / assembler origin attribution

### 24.1 原則

adapter は、diagnostic の発生元を偽ってはならない。

### 24.2 attribution 優先順

1. explicit tool prefix から判定
2. invocation phase と known block family から判定
3. 不明なら `origin = driver` または `unknown`

### 24.3 典型例

| raw prefix / context | origin | phase |
|---|---|---|
| `gcc: fatal error:` | `driver` | `driver` or `compile` |
| `cc1:` / `cc1plus:` ICE banner | `compiler_frontend` | `compile` |
| `/usr/bin/ld:` / `ld:` | `linker` | `link` |
| `collect2: error:` | `driver` with linker summary context | `link` |
| `as:` | `assembler` | `assemble` |

### 24.4 fake precision を避ける

例:

- source span が無い linker error に source span を捏造しない
- `gcc:` で始まる fatal を無理に frontend error と決めつけない
- symbol extraction に失敗しても location を捏造しない

---

## 25. trace / observability 契約

### 25.1 trace に必ず残すべきもの

- wrapper version
- selected mode
- selected version band / processing path / support level
- backend path / version
- injected flags
- sanitized env keys の一覧（値は原則 full dump しない）
- temp artifact paths
- child exit code / signal
- parser result summary
- integrity issues
- fallback reason

### 25.2 user-visible と internal trace の分離

- user-facing stderr に詳細 trace を垂れ流してはならない
- retained artifact / trace path を一行で示すのはよい
- full trace は opt-in / retained bundle で見る

### 25.3 metrics 候補

adapter レベルでは以下を計測可能にしてよい。

- Path A structured success rate
- fallback rate
- hard conflict rate
- residual raw classifier hit rate
- average stderr bytes
- SARIF parse latency
- validation fatal rate

---

## 26. セキュリティ / 運用上の注意

### 26.1 temp file 安全性

- private dir
- secure create
- symlink 非追従
- world-readable にしない

### 26.2 path 漏洩

retained artifact には source path, include path, build root が入る。  
したがって:

- local default retention は控えめにする
- CI アップロード時は access control を明確にする
- trace bundle の redaction policy は別文書で定める

### 26.3 環境ダンプ

- 全 environment dump を default で保存してはならない
- whitelist key のみ保存する
- secret を含みうる env は redact する

### 26.4 shell 非依存

- wrapper は shell を使って backend を起動してはならない
- quoting / glob / expansion は caller 側の責務のままにする

---

## 27. 典型フロー

### 27.1 GCC 15 local render

```text
user invokes gcc-formed -c foo.cc -o foo.o

wrapper:
  resolve gcc -> /usr/bin/g++
  probe -> path A
  no hard conflict
  mode -> render
  create temp dir
  spawn child with SARIF sidecar + stable capture flags
  capture stderr.raw
  parse diagnostics.sarif
  parse linker/driver residuals if any
  validate
  render wrapper UX
  return child exit code
```

### 27.2 GCC 15 shadow rollout

```text
user invokes gcc-formed ...
wrapper:
  path A
  mode -> shadow
  add SARIF sidecar only
  tee native stderr to console
  collect artifacts
  parse offline/in-process
  store trace for comparison
  return child exit code
```

### 27.3 GCC 14 product path host

```text
user invokes gcc-formed ...
wrapper:
  path B
  default mode -> NativeTextCapture
  capture raw stderr
  render if confidence budget and disclosure policy are satisfied
  otherwise passthrough
```

### 27.4 link failure with no SARIF results

```text
path A render
child exits non-zero
stderr contains /usr/bin/ld: undefined reference ...
diagnostics.sarif is absent or empty
raw residual classifier detects linker family
adapter constructs linker-origin document
render proceeds
```

---

## 28. 実装受け入れ条件（adapter 単体）

以下を満たしたら adapter/integration 実装は v1alpha Done に近い。

### 28.1 Path A 必須条件

1. GCC 15+ で `render` / `shadow` / `passthrough` の 3 mode が動く
2. `render` で SARIF sidecar を使い、raw stderr も capture する
3. child stdout artifact を壊さない
4. hard conflict 時に passthrough できる
5. renderer crash / parser crash 時に raw fallback できる
6. linker family 最低 5 種を raw residual から高信頼で拾える
7. IR validator fatal 時に fail-open できる

### 28.2 Path B 必須条件

1. `NativeTextCapture` と `SingleSinkStructured` を別 path として選択できる
2. default TTY path を silent に `SingleSinkStructured` へ切り替えない
3. render / passthrough / retained artifact の経路が壊れない

### 28.3 Path C 必須条件

1. unsupported compiler を理由に build を壊さない
2. user が opt-out しなくても安全に passthrough または conservative capture できる
3. capability が揃う場合だけ explicit structured path を足せる

---

## 29. テストへの含意

### 29.1 adapter goldens

Path A では最低 3 層の fixture を持つ。

1. child `stderr.raw`
2. `diagnostics.sarif`
3. normalized `DiagnosticDocument`

### 29.2 mode matrix tests

少なくとも以下を行う。

- Path A render
- Path A shadow
- Path A hard conflict -> passthrough
- Path A linker-only failure
- Path A renderer crash -> raw fallback
- Path B `NativeTextCapture`
- Path B explicit `SingleSinkStructured`
- Path C passthrough

### 29.3 environment collision tests

- `GCC_DIAGNOSTICS_LOG`
- `GCC_EXTRA_DIAGNOSTIC_OUTPUT`
- `EXPERIMENTAL_SARIF_SOCKET`
- `LC_ALL`
- `LC_MESSAGES`

の組み合わせに対し、mode ごとの sanitize / preserve が仕様通りかを確認する。

### 29.4 stderr stress tests

- 大量 diagnostics
- 長い template 失敗
- 巨大 include chain
- linker flood
- stderr truncation cap 超過

### 29.5 race / cleanup tests

- temp dir cleanup
- retained policy
- child signal termination
- interrupted wrapper
- concurrent invocations

---

## 30. ADR 対応と rollout backlog

この仕様に関わる基線判断は、以下の ADR で固定済みである。

1. version band / capability / processing path  
   `ADR-0026`, `ADR-0027`, `ADR-0028`, `ADR-0029`

2. fail-open / provenance  
   `ADR-0006`

3. locale / environment policy  
   `ADR-0011`

4. SARIF egress boundary  
   `ADR-0013`

5. linker residual handling  
   `ADR-0014`

6. retained artifact / trace / redaction  
   `ADR-0016`

7. renderer-visible mode surface  
   `ADR-0019`

以下は v1alpha の rollout backlog とし、この仕様の normative contract には含めない。

1. local default mode を dogfood / shadow / render のどこから始めるか
2. raw fallback 時の user-facing 注記の微文言
3. Path B explicit structured path をどの channel まで default shipping するか

---

## 31. 推奨実装分割

Rust 実装を前提とした概念分割例。

```text
diag_core_ir
diag_core_validate
diag_capture_runtime
diag_backend_probe
diag_adapter_gcc
diag_residual_text
diag_trace
diag_cli_front
```

責務:

- `diag_capture_runtime`: child spawn, pipe drain, artifact retention
- `diag_backend_probe`: backend identity / version band / capability cache
- `diag_adapter_gcc`: Path A/B/C policy と structured ingest
- `diag_residual_text`: linker/driver/assembler classifier
- `diag_trace`: trace bundle / integrity issue
- `diag_cli_front`: mode selection, user-facing fallback wiring

重要なのは、**adapter と capture runtime を分ける**こと。  
これにより将来 Clang adapter を追加しても capture runtime を使い回せる。

---

## 32. 最低限の実装順序

1. backend resolution + passthrough only
2. secure stderr capture runtime
3. Path A shadow (`add-output=sarif:file=...`)
4. SARIF parser → `DiagnosticDocument`
5. Path A render
6. raw fallback
7. linker residual classifier
8. hard conflict detection
9. environment sanitization
10. Path B/C hardening

この順序にする理由は、**まず fail-open を完成させ、その上に structured path を載せるため**である。

---

## 33. この仕様の Done 条件

本仕様が十分に機能していると言える条件。

1. GCC 15+ で wrapper rendering が single-pass で成立する
2. raw stderr fallback が常に残る
3. hard conflict / unsupported path / introspection invocation で安全に passthrough できる
4. Path A で linker/driver residual を別 source として取り込める
5. `DiagnosticDocument` が provenance を失わない
6. adapter failure が compiler failure を隠さない
7. 将来の Clang adapter が capture runtime と mode policy の大部分を再利用できる

---

## 34. まとめ

この仕様の要点は 6 つだけである。

1. **GCC 15+ は最良の reference path だが唯一の product path ではない**
2. **GCC facts は path-aware に structured source を優先する**
3. **raw stderr は必ず保持する**
4. **GCC 13–14 と GCC 9–12 も product path として扱う**
5. **hard conflict では silent override せず passthrough する**
6. **fail-open を第一原則にする**

この判断により、wrapper は「ちょっと賢い text parser」ではなく、  
**長期的に multi-compiler へ拡張可能な ingestion platform** になる。

---

## 付録 A: 参考にした公開資料

- **[R1] GCC 15 Changes**  
  https://gcc.gnu.org/gcc-15/changes.html

- **[R2] GCC Manual: Diagnostic Message Formatting Options**  
  https://gcc.gnu.org/onlinedocs/gcc/Diagnostic-Message-Formatting-Options.html

- **[R3] GCC 13 Changes**  
  https://gcc.gnu.org/gcc-13/changes.html

- **[R4] GCC 13.4 Manual: Diagnostic Message Formatting Options**  
  https://gcc.gnu.org/onlinedocs/gcc-13.4.0/gcc/Diagnostic-Message-Formatting-Options.html

- **[R5] GCC 14.2 Manual: Diagnostic Message Formatting Options**  
  https://gcc.gnu.org/onlinedocs/gcc-14.2.0/gcc/Diagnostic-Message-Formatting-Options.html

- **[R6] GCC Manual: Environment Variables Affecting GCC**  
  https://gcc.gnu.org/onlinedocs/gcc/Environment-Variables.html

- **[R7] GCC Install / Configuration: optional `--enable-libgdiagnostics` and `--enable-sarif-replay`**  
  https://gcc.gnu.org/install/configure.html

- **[R8] GCC Manual: SARIF format notes (`-ftime-report` interaction etc.)**  
  https://gcc.gnu.org/onlinedocs/gcc/Diagnostic-Message-Formatting-Options.html#sarif

---

## 付録 B: 初回実装チェックリスト

### B.1 adapter runtime

- [ ] shell を介さず backend を起動する
- [ ] stdout passthrough を壊さない
- [ ] stderr spool file capture がある
- [ ] child signal を保持できる
- [ ] temp dir は private である
- [ ] comma を含まない SARIF sidecar path を生成する

### B.2 Path A

- [ ] `-fdiagnostics-add-output=sarif:version=2.1,file=...` を注入できる
- [ ] SARIF parse に成功する
- [ ] raw stderr を保持する
- [ ] IR validator まで通る
- [ ] parse/render failure で raw fallback できる

### B.3 conflict / downgrade

- [ ] `-fdiagnostics-format=*` で passthrough する
- [ ] `-fdiagnostics-add-output=*` で passthrough する
- [ ] `-fdiagnostics-set-output=*` で passthrough する
- [ ] `-fdiagnostics-parseable-fixits` で passthrough する

### B.4 residual parser

- [ ] `ld` undefined reference
- [ ] `ld` multiple definition
- [ ] `ld` cannot find -l
- [ ] `collect2` summary fold
- [ ] `as` error
- [ ] `gcc: fatal error`
- [ ] `cc1plus: internal compiler error`

### B.5 operational modes

- [ ] render
- [ ] shadow
- [ ] passthrough
- [ ] Path B default `NativeTextCapture`
- [ ] unsupported path safe fallback
