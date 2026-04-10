# gcc-formed GCC Adapter / Ingestion 仕様書

- **文書種別**: 内部仕様書（実装契約）
- **状態**: Accepted Baseline
- **版**: `1.0.0-alpha.1`
- **対象**: `gcc-formed` / 将来の `cc-formed`
- **主用途**: GCC 呼び出し・診断捕捉・構造化取り込み・安全フォールバックの契約固定
- **想定実装**: Linux first / GCC first / 品質最優先
- **関連文書**:
  - `gcc-formed-architecture-proposal.md`
  - `diagnostic-ir-v1alpha-spec.md`
  - `implementation-bootstrap-sequence.md`
  - `adr-initial-set/README.md`
- **関連 ADR**:
  - `adr-initial-set/adr-0001-wrapper-first-entrypoint.md`
  - `adr-initial-set/adr-0003-structured-first-gcc-ingress.md`
  - `adr-initial-set/adr-0004-gcc-15-first-support-policy.md`
  - `adr-initial-set/adr-0005-gcc-13-14-compatibility-tier.md`
  - `adr-initial-set/adr-0006-fail-open-fallback-and-provenance.md`
  - `adr-initial-set/adr-0011-locale-policy-english-first-reduced-fallback.md`
  - `adr-initial-set/adr-0013-sarif-egress-scope.md`
  - `adr-initial-set/adr-0014-linker-diagnostics-via-staged-text-adapter.md`
  - `adr-initial-set/adr-0016-trace-bundle-content-and-redaction.md`
  - `adr-initial-set/adr-0019-render-modes.md`

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
- support tier と downgrade 条件

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

### 4.1 本命サポートは GCC 15+

**v1alpha で production quality の wrapper rendering を約束するのは GCC 15+ のみ** とする。

理由:

- text と SARIF の **single-pass 並行取得** ができる
- JSON deprecated 後の方向と整合する
- raw stderr fallback を温存したまま structured-first にできる

### 4.2 authoritative source は SARIF、ただし GCC 所有診断に限る

GCC 15+ の structured path では、**GCC 自身の diagnostics facts は SARIF を一次ソース**とする。  
ただし external tool（assembler / linker / driver 外部サブプロセス）の text は SARIF に含まれないことがありうるため、**raw stderr は常に同時に保持**する。

### 4.3 raw stderr は常に capture する

structured source が使える場合でも、**raw stderr を捨ててはならない**。  
用途は以下。

- wrapper failure 時の fail-open fallback
- external tool diagnostics 取り込み
- provenance
- diff / debugging
- corpus / regression fixture

### 4.4 GCC 13–14 は production rendering の対象外

**GCC 13–14 は v1alpha では production rendering の対象外** とする。  
default は `passthrough` または `shadow(raw-capture-only)` であり、`sarif-file` を使う experimental mode は設けてもよいが、**本番 rollout-ready とみなさない**。

理由:

- GCC 13–14 には SARIF はあるが、`add-output` による dual-sink がないため、structured path を使うと native text fallback を失う
- 品質最優先という前提に反し、wrapper failure 時の安全性が落ちる
- hot path で JSON を core 依存にすると、GCC 15 以降の方向と逆行する

### 4.5 JSON は hot path の core dependency にしない

GCC 13–14 の JSON は **offline corpus importer / fixture normalizer** としては許容する。  
しかし **production adapter hot path** は JSON に依存してはならない。

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
- support tier 判定
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
    ├─ choose support tier / mode
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

### 8.1 バージョン別 support tier

| Tier | GCC major | default mode | structured source | raw native fallback | rollout 位置づけ |
|---|---:|---|---|---|---|
| A | 15.x 以上 | `render` / `shadow` / `passthrough` | `-fdiagnostics-add-output=sarif:file=...` | あり | production |
| B | 13.x–14.x | `passthrough` / `shadow(raw-only)` | なし（default） | あり | compatibility |
| B-exp | 13.x–14.x | `force-structured-experimental` | `-fdiagnostics-format=sarif-file` | 限定的 / 劣化 | 開発者向け実験 |
| C | 12 以下 / 不明 | `passthrough` | なし | native のみ | unsupported / corpus only |

### 8.2 プラットフォーム前提

- **MUST**: Linux を第一級対象とする
- **MAY**: 将来の macOS / Windows は、本仕様の概念を踏襲して別 adapter 仕様で補う
- **MUST NOT**: Linux 向け hot path に platform-specific optional dependency を持ち込む

### 8.3 「対応」の意味

本仕様で「対応」と言うとき、それは次の 3 レベルを区別する。

1. **production rendering**: wrapper が独自 UX を出してよい
2. **safe passthrough**: wrapper が前面に立っても native compiler 体験を壊さない
3. **corpus ingestion only**: 研究・比較・fixture 用であり本番利用はしない

GCC 15+ は (1)(2)(3)。  
GCC 13–14 は v1alpha では原則 (2)(3)。  
GCC 12 以下は原則 (2) のみ。

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
- support tier 不足、hard conflict、explicit opt-out 時の安全モード
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
  - support tier
  - known flag support assumptions

**SHOULD**: hot path で毎回余分な probe process を起動しない。

### 10.5 mode / tier 決定アルゴリズム

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

if tier == A:
    if explicit mode == shadow:
        shadow
    else:
        render

if tier == B:
    if explicit mode == force-structured-experimental:
        force-structured-experimental
    else if explicit mode == shadow:
        shadow(raw-only)
    else:
        passthrough

if tier == C:
    passthrough
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
| `diagnostics.sarif` | Tier A render/shadow で MUST | GCC 追加出力 sidecar |
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

## 15. Tier A: GCC 15+ production render path

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

Tier A render path で structured ingestion を「成功」とみなす条件:

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

Tier A では以下を原則とする。

- GCC front-end / middle-end / analyzer / diagnostic subsystem の facts → **SARIF authoritative**
- raw stderr は provenance / fallback / residual 用
- raw text の wording が SARIF とズレても、facts は SARIF を優先

### 15.7 raw stderr residual parser の役割

Tier A render path の raw stderr parser は **generic GCC front-end text parser ではない**。  
役割は以下に限る。

- linker
- assembler
- gcc driver fatal
- `collect2` summary
- internal compiler error banner の補助情報
- unclassified residual blob の capture

### 15.8 この tier でやってはいけないこと

- raw stderr から generic `file:line:col:` GCC diagnostics を再構成し、SARIF より優先する
- SARIF にない location を source 読みで捏造する
- external residual parser で GCC 本体 diagnostics を重複生成する

---

## 16. Tier A: shadow path

### 16.1 目的

shadow は rollout / corpus / A/B 比較のための mode であり、**user-visible output を極力 native GCC に近づける**。

### 16.2 flag policy

Tier A shadow では、原則として次だけを追加する。

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

## 17. Tier B: GCC 13–14 compatibility path

### 17.1 default policy

GCC 13–14 の default は以下。

- `render`: **使わない**
- `shadow`: raw stderr capture のみ
- `passthrough`: fully allowed

### 17.2 なぜ production render をやらないか

GCC 13–14 には SARIF はあるが、`-fdiagnostics-format=sarif-file` は main sink を置き換える。  
GCC 15 のような dual-sink がないため、structured facts を取ると native text fallback を失う。 [R1][R3]

本プロジェクトは品質優先なので、v1alpha ではこの tradeoff を飲まない。

### 17.3 `force-structured-experimental`

明示的 opt-in のみで、以下を許可してよい。

```text
-fdiagnostics-format=sarif-file
```

または equivalent policy。

ただし:

- native text fallback は保証しない
- wrapper failure 時は raw SARIF path / retained artifact への導線が主になる
- rollout default にしてはならない
- CI 研究・fixture 収集・parser 開発用途に限定する

### 17.4 JSON の扱い

- `json-stderr` / `json-file` は存在するが [R3]
- **MUST NOT** production hot path の default にしない
- **MAY** offline importer / fixture translator に限って使う

---

## 18. Tier C: GCC 12 以下 / unknown gcc-like

### 18.1 基本方針

- default は passthrough
- optional raw stderr capture は許容
- wrapper render は行わない

### 18.2 理由

- structured path の可用性・安定性が弱い
- text parser を中核にすると将来負債になる
- fail-open を最優先すると passthrough が最も安全

### 18.3 corpus 収集

Tier C でも shadow/raw capture により corpus は集めてよい。  
ただし、その corpus は **将来の比較資料**であり、Tier C の native rendering 約束ではない。

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

1. `driver_fatal`
2. `linker_undefined_reference`
3. `linker_multiple_definition`
4. `linker_cannot_find_library`
5. `linker_file_format_or_relocation`
6. `collect2_summary`
7. `assembler_error`
8. `internal_compiler_error_banner`
9. `unclassified_residual_blob`

### 20.3 classifier の安全原則

- **MUST** explicit tool prefix または強いアンカーを要求する
- **MUST NOT** generic GCC text diagnostics を推定で再構成する
- **MUST NOT** location を捏造する
- **SHOULD** confidence を family ごとに固定または narrow range で出す
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
| Tier A, SARIF ok, validator ok | render |
| Tier A, SARIF missing, raw に高信頼 linker あり | render（linker-only） |
| Tier A, SARIF invalid, raw stderr あり | raw fallback |
| Tier A, renderer crash | raw fallback |
| Tier B default | passthrough or shadow(raw-only) |
| Tier B experimental, parse fail | raw residual + retained SARIF path、必要なら wrapper warning |
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

### 23.1 Tier A render canonical set

```text
-fdiagnostics-add-output=sarif:version=2.1,file=<tmp>/diagnostics.sarif
-fdiagnostics-color=never
-fdiagnostics-urls=never
-fmessage-length=0
```

### 23.2 Tier A shadow canonical set

```text
-fdiagnostics-add-output=sarif:version=2.1,file=<tmp>/diagnostics.sarif
```

### 23.3 Tier B experimental canonical set

```text
-fdiagnostics-format=sarif-file
```

注記:

- `sarif-file` の出力先は GCC の既定ファイル名規則に依存するため、wrapper は artifact discovery を実装するか、実験モード専用の working layout を定める必要がある
- この面倒さ自体が Tier B experimental を本番対象外にする理由である

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
- support tier
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

- Tier A structured success rate
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
  probe -> tier A
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
  tier A
  mode -> shadow
  add SARIF sidecar only
  tee native stderr to console
  collect artifacts
  parse offline/in-process
  store trace for comparison
  return child exit code
```

### 27.3 GCC 14 compatibility host

```text
user invokes gcc-formed ...
wrapper:
  tier B
  default mode -> passthrough
  exec backend unchanged
  optionally capture raw stderr if shadow(raw-only) configured
```

### 27.4 link failure with no SARIF results

```text
tier A render
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

### 28.1 Tier A 必須条件

1. GCC 15+ で `render` / `shadow` / `passthrough` の 3 mode が動く
2. `render` で SARIF sidecar を使い、raw stderr も capture する
3. child stdout artifact を壊さない
4. hard conflict 時に passthrough できる
5. renderer crash / parser crash 時に raw fallback できる
6. linker family 最低 5 種を raw residual から高信頼で拾える
7. IR validator fatal 時に fail-open できる

### 28.2 Tier B 必須条件

1. GCC 13–14 を誤って production render しない
2. passthrough が壊れない
3. optional shadow(raw-only) で corpus 収集ができる

### 28.3 Tier C 必須条件

1. unsupported compiler を理由に build を壊さない
2. user が opt-out しなくても安全に passthrough できる

---

## 29. テストへの含意

### 29.1 adapter goldens

Tier A では最低 3 層の fixture を持つ。

1. child `stderr.raw`
2. `diagnostics.sarif`
3. normalized `DiagnosticDocument`

### 29.2 mode matrix tests

少なくとも以下を行う。

- Tier A render
- Tier A shadow
- Tier A hard conflict -> passthrough
- Tier A linker-only failure
- Tier A renderer crash -> raw fallback
- Tier B passthrough
- Tier C passthrough

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

1. support tier  
   `ADR-0004` と `ADR-0005`

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
3. Tier B experimental を製品バイナリへ同梱するか

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
- `diag_backend_probe`: backend identity / tier cache
- `diag_adapter_gcc`: Tier A/B/C policy と SARIF ingest
- `diag_residual_text`: linker/driver/assembler classifier
- `diag_trace`: trace bundle / integrity issue
- `diag_cli_front`: mode selection, user-facing fallback wiring

重要なのは、**adapter と capture runtime を分ける**こと。  
これにより将来 Clang adapter を追加しても capture runtime を使い回せる。

---

## 32. 最低限の実装順序

1. backend resolution + passthrough only
2. secure stderr capture runtime
3. Tier A shadow (`add-output=sarif:file=...`)
4. SARIF parser → `DiagnosticDocument`
5. Tier A render
6. raw fallback
7. linker residual classifier
8. hard conflict detection
9. environment sanitization
10. Tier B/C compatibility hardening

この順序にする理由は、**まず fail-open を完成させ、その上に structured path を載せるため**である。

---

## 33. この仕様の Done 条件

本仕様が十分に機能していると言える条件。

1. GCC 15+ で wrapper rendering が single-pass で成立する
2. raw stderr fallback が常に残る
3. hard conflict / unsupported tier / introspection invocation で安全に passthrough できる
4. Tier A で linker/driver residual を別 source として取り込める
5. `DiagnosticDocument` が provenance を失わない
6. adapter failure が compiler failure を隠さない
7. 将来の Clang adapter が capture runtime と mode policy の大部分を再利用できる

---

## 34. まとめ

この仕様の要点は 6 つだけである。

1. **本命は GCC 15+**
2. **GCC facts は SARIF authoritative**
3. **raw stderr は必ず保持**
4. **GCC 13–14 は v1alpha では production render しない**
5. **hard conflict では silent override せず passthrough**
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

### B.2 Tier A

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
- [ ] Tier B default non-render
- [ ] unsupported tier safe fallback
