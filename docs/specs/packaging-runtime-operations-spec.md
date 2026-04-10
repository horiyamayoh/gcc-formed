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

# gcc-formed Packaging / Runtime / Operations 仕様書

- **文書種別**: 内部仕様書（実装契約）
- **状態**: Accepted Baseline
- **版**: `1.0.0-alpha.1`
- **対象**: `gcc-formed` / `g++-formed` / 将来の `cc-formed`
- **主用途**: Linux first の配布・インストール・ランタイム境界・更新/ロールバック・社内運用の契約固定
- **想定実装**: Rust / 単一バイナリ中心 / GCC backend 外部依存 / no daemon / no mandatory runtime
- **関連文書**:
  - `../architecture/gcc-formed-vnext-change-design.md`
  - `diagnostic-ir-v1alpha-spec.md`
  - `gcc-adapter-ingestion-spec.md`
  - `rendering-ux-contract-spec.md`
  - `quality-corpus-test-gate-spec.md`
  - `../releases/PUBLIC-BETA-RELEASE.md`
- **関連 ADR**:
  - `adr-initial-set/adr-0001-wrapper-first-entrypoint.md`
  - `adr-initial-set/adr-0007-rust-as-implementation-language.md`
  - `adr-initial-set/adr-0008-linux-first-single-binary-musl-distribution.md`
  - `adr-initial-set/adr-0016-trace-bundle-content-and-redaction.md`
  - `adr-initial-set/adr-0017-dependency-allowlist-and-license-policy.md`
  - `adr-initial-set/adr-0020-stability-promises.md`
  - `adr-initial-set/adr-0025-stable-release-automation-and-rollback-evidence.md`

---

## 1. この文書の目的

本仕様書は、`gcc-formed` を **どう配るか**、**どの環境なら supported と言えるか**、**どこに state を置くか**、**どう更新し、どう即時に切り戻せるか** を固定する。

この文書は単なる配布手順書ではない。compiler wrapper は build path の最前面に差し込まれるため、配布・ランタイム・運用の失敗は、そのまま全開発者と CI の停止につながる。したがって本仕様の中心は「便利なインストーラ」ではなく、以下の 6 点にある。

1. **導入障壁を最小にしつつ、rollback を最短にすること**
2. **runtime 依存の壊れやすさを製品契約の外に追い出すこと**
3. **developer machine と CI で同じ artifact を使えること**
4. **trace / support / incident response に必要な build metadata を確実に残すこと**
5. **将来の multi-compiler / multi-surface 化でも破綻しない install/state 境界を選ぶこと**
6. **社内配布で現実的に回る release / promote / rollback model を作ること**

したがって本仕様は、以下を規定する。

- artifact の種類・命名・内容
- target triple / libc / arch ごとの artifact support class
- install root と symlink 切り替え方式
- config / cache / state / runtime file の配置
- build reproducibility と release engineering の必須条件
- distribution channel の優先順位
- update / rollback / pinning / support の運用契約
- packaging に固有の品質 gate

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

1. Rust 公式の platform support には `x86_64-unknown-linux-musl` と `aarch64-unknown-linux-musl` が含まれており、Linux musl 向け target を正式に扱っている。 [R1]
2. Python 公式は `venv` を既存 Python installation 上に作られる isolated environment と説明し、さらに virtual environment は disposable であり、movable / copyable ではないとしている。つまり venv 前提配布は「単一 artifact を持ち運ぶ CLI 製品」と相性が悪い。 [R2]
3. XDG Base Directory Specification は、config / data / state / cache / runtime の配置を `XDG_CONFIG_HOME`, `XDG_DATA_HOME`, `XDG_STATE_HOME`, `XDG_CACHE_HOME`, `XDG_RUNTIME_DIR` で定義し、それぞれの既定値と意味を示している。 [R3]
4. Cargo は `Cargo.lock` に exact dependency information を保持し、`cargo build --locked` は lockfile の変更を伴う解決を拒否し、`--offline` は network access を禁止する。 [R4][R7]
5. Cargo は `cargo vendor` と source replacement により、依存 source を local filesystem 側へ vendor して offline / mirrored build を構成できる。 [R5][R6]

この公開事実に基づき、本プロジェクトでは **Rust 製単一バイナリを primary artifact とし、Python/venv を shipped runtime に持ち込まず、XDG を state の基準にし、build は lock/vendor/offline を前提にする**。

---

## 4. 設計上の最重要判断

本仕様で固定する最重要判断を先に明示する。

### 4.1 primary artifact は「単一実行バイナリを中心とした versioned archive」である

配布の製品契約は package manager ではなく **artifact 自体** に置く。

初期の canonical artifact は以下とする。

- `tar.gz` archive
- 中身は **単一の Rust 製 ELF 実行ファイル** と、その alias / symlink / metadata / license 同梱物
- install は versioned directory へ展開して行う

`deb` / `rpm` / Homebrew / asdf / container image は **secondary channel** であり、primary artifact の再包材として扱う。

### 4.2 Linux first の production artifact は musl 系を基線にする

v1alpha で production-quality の portability baseline として扱うのは **Linux + musl target** である。

初期の優先順位は次の通り。

1. `x86_64-unknown-linux-musl` を Tier 1 production artifact とする
2. `aarch64-unknown-linux-musl` は release pipeline と fleet 検証が整うまで Tier 2 preview とする
3. `*-unknown-linux-gnu` artifact は **exception path** とし、明示的な fleet 要件がある場合にのみ publish を許可する

理由:

- runtime 前提を減らしやすい
- CI / ephemeral environment / rootless install に向く
- install failure の責任範囲を製品側に寄せやすい
- 「その host に Python / venv / shared lib / package manager が揃っているか」という不安定性を減らせる

### 4.3 shipped runtime に Python / Node / Java / container runtime を要求しない

本製品は **Rust バイナリ単体で動作可能** でなければならない。

以下は v1alpha の shipped runtime 前提として **MUST NOT**:

- Python runtime
- `pip install` / `venv` 必須運用
- Node.js runtime
- Java runtime
- container engine 必須
- background daemon 必須

補助スクリプトや build/test tooling に Python や shell を使うことは許されるが、**製品利用者の実行環境契約には含めない**。

### 4.4 package manager integration は secondary であり、binary bits を変えてはならない

`deb` / `rpm` / 将来の Homebrew/asdf は配布 convenience として価値がある。だが v1alpha では、**各 channel が別々に build された別製品** になることを避ける。

したがって secondary package は、原則として以下を満たさなければならない。

1. primary artifact 由来の同一 binary bits を使う
2. package manager ごとに source rebuild しない
3. install path, symlink, docs, shell completion などの周辺だけを付加する
4. upstream release checksum / manifest を保存する

### 4.5 install は versioned root + atomic switch で行う

install/upgrade/rollback は **in-place overwrite** ではなく、以下の transactional model を採る。

1. 新バージョンを versioned staging directory へ展開
2. checksum / manifest / self-check を検証
3. 問題がなければ `current` symlink を atomic に切り替える
4. 旧バージョンは一定数保持
5. rollback は symlink 戻しで完了

これにより、「壊れた upgrade が build path を即死させる」リスクを下げる。

### 4.6 mutable state は XDG に従って分離する

config / cache / state / runtime object は install payload と混在させてはならない。

- install payload は read-mostly
- config は user/admin override
- cache は消してよい
- state は support/troubleshooting に必要だが portable data ではない
- runtime object は session-bound / short-lived

この分離を崩すと、upgrade/rollback/uninstall の責務が曖昧になる。

### 4.7 same artifact across local and CI を必須原則にする

開発者ローカル・CI・canary rollout で異なる build artifact を使う設計は、fidelity 問題と support の切り分けを難しくする。したがって、**CI 用の専用品ではなく、developer machine と同じ release artifact を CI でも使う**。

### 4.8 self-update は実装しない

本製品は build path に入るため、自己更新はリスクが大きい。v1alpha では **self-update 機能を持たない**。

更新は常に外部の release/promote/install 経路から行う。

---

## 5. 非目標

本仕様は以下を goal にしない。

1. すべての Linux distribution 向け native package を最初から提供すること
2. install UX の自然さを portability より優先すること
3. package manager ごとに最適化した独自 build を許可すること
4. background service / daemon による高速化を前提にすること
5. self-updater を実装すること
6. 開発者ローカルの環境差を package manager 側に吸収させること
7. すべての state を永続化すること
8. source build を end user 向け primary install path にすること

---

## 6. スコープ

### 6.1 扱うもの

- release artifact と installable package の契約
- target triple / arch / libc / artifact support class
- install root / symlink / uninstall / rollback
- config / cache / state / runtime file layout
- release build と reproducibility policy
- distribution channel policy
- promote / pin / support / incident response の operational contract

### 6.2 扱わないもの

- compiler invocation の詳細（`gcc-adapter-ingestion-spec.md` を参照）
- renderer UX の詳細（`rendering-ux-contract-spec.md` を参照）
- IR の schema（`diagnostic-ir-v1alpha-spec.md` を参照）
- 組織固有 artifact repository 製品の選定
- OSS 公開時の legal checklist 全体
- Windows / macOS の fully-specified packaging

---

## 7. Artifact 契約

### 7.1 artifact family 一覧

v1alpha で定義する artifact family は以下。

1. **primary binary archive**
   - 例: `gcc-formed-v0.1.0-linux-x86_64-musl.tar.gz`
2. **debug/symbol companion archive**
   - 例: `gcc-formed-v0.1.0-linux-x86_64-musl.debug.tar.gz`
3. **source/provenance bundle**
   - 例: `gcc-formed-v0.1.0-source.tar.gz`
4. **package-manager wrapper**（optional secondary）
   - 例: `.deb`, `.rpm`
5. **checksum / signature / manifest set**
   - `SHA256SUMS`
   - detached signature
   - `manifest.json`
   - license notice / dependency notice

### 7.2 primary archive の最小内容

primary archive は少なくとも以下を含まなければならない。

```text
bin/
  gcc-formed
  g++-formed
share/doc/gcc-formed/
  README.md
  RELEASE-NOTES.md
share/licenses/gcc-formed/
  LICENSE
  NOTICE
manifest.json
build-info.txt
```

規則:

1. `gcc-formed` と `g++-formed` は **同一 binary bits** であってよい
2. alias は symlink, hardlink, duplicate file のいずれでもよいが、**内容差を持ってはならない**
3. 将来の `cc-formed` / `c++-formed` は reserved だが、v1alpha では default install 対象にしない
4. primary archive には user config を含めてはならない
5. primary archive には mutable cache/state を含めてはならない

### 7.3 `manifest.json` の必須項目

`manifest.json` は少なくとも以下を持つこと。

- product name
- product version
- artifact target triple
- artifact OS / arch / libc family
- git commit
- build profile
- rustc version
- cargo version
- build timestamp または `SOURCE_DATE_EPOCH`
- lockfile hash
- vendor hash（vendor を使う場合）
- IR spec version
- adapter spec version
- renderer spec version
- artifact support class declaration
- release channel
- checksum list

### 7.4 debug/symbol artifact

production 配布では stripped binary を許容する。だが support のため、release pipeline は **unstripped binary または debug symbol artifact を必ず保持** しなければならない。

規則:

1. end user 向け primary archive は stripped でよい
2. release engineering は対応する debug artifact を同時生成する
3. support/incident response で version と manifest から debug artifact を一意に引けること
4. debug artifact は通常 install 経路に混ぜない

### 7.5 checksum / signature

release artifact には以下を同梱または併置する。

- SHA-256 checksum
- detached signature
- release manifest

v1alpha の install contract では、**checksum verification は MUST**、**signature verification は SHOULD** とする。

理由:

- checksum は install script / CI pinning に必須
- detached signature は supply chain hardening に有効だが、初期 rollout では verifier 配布の都合がある

### 7.6 archive format

primary archive の canonical format は **`.tar.gz`** とする。

理由:

- ほぼすべての Linux 環境で扱いやすい
- `zstd` 非搭載環境を避けられる
- rootless install / CI bootstrap が単純

`.tar.zst` は将来の optional mirror format としては許容するが、v1alpha の canonical 契約には含めない。

---

## 8. Target / artifact support policy

### 8.1 artifact support matrix

| Class | target triple | artifact status | 主用途 | 備考 |
|---|---|---|---|---|
| T1 | `x86_64-unknown-linux-musl` | Required | developer machine / CI / canary / prod | primary baseline |
| T2 | `aarch64-unknown-linux-musl` | Recommended later | ARM64 fleet / CI | preview until fleet validation complete |
| T3 | `x86_64-unknown-linux-gnu` | Exception only | special fleet workaround | musl 例外時のみ |
| T4 | source build | Escape hatch only | internal engineering | end user primary path ではない |

### 8.2 artifact support class の意味

- **T1**: release gate, rollback, support playbook, install docs, CI coverage の全対象
- **T2**: release artifact は出すが、fleet coverage や rollback playbook が T1 より弱い
- **T3**: 明示的な事情がある場合のみ publish。標準運用では使わない
- **T4**: 再現やデバッグのための補助。運用契約の中心ではない

### 8.3 musl baseline を壊す例外条件

以下の全条件を満たす場合のみ `*-gnu` artifact を publish してよい。

1. 実在 fleet で musl artifact に再現可能な blocking issue がある
2. issue が wrapper 実装ではなく target/runtime 相性に起因すると判断された
3. workaround が運用で吸収困難である
4. exception artifact を artifact support class とともに明示できる
5. 例外 publish が ADR または運用記録で追跡可能である

### 8.4 backend compiler との関係

本製品は GCC backend を bundle しない。したがって artifact の support は、

- wrapper binary が実行可能であること
- artifact support class で定義された host 上で install/rollback できること
- backend compiler は別途 host に存在すること

を意味する。GCC 自体の存在や版管理は、本仕様では **external dependency** として扱う。

---

## 9. Install layout と切り替えモデル

### 9.1 system-wide install の既定

system-wide install の推奨 layout は以下。

```text
/opt/cc-formed/
  x86_64-unknown-linux-musl/
    v0.1.0/
    v0.1.1/
    current -> v0.1.1
/usr/local/bin/
  gcc-formed -> /opt/cc-formed/x86_64-unknown-linux-musl/current/bin/gcc-formed
  g++-formed -> /opt/cc-formed/x86_64-unknown-linux-musl/current/bin/g++-formed
```

規則:

1. versioned payload は `/opt/cc-formed/<target>/<version>/` を既定とする
2. PATH に出すのは `current/bin/*` への symlink とする
3. installer は既存 version を上書きしてはならない
4. 切り替えは symlink swap で行う

### 9.2 user-local install の既定

rootless install の推奨 layout は以下。

```text
$HOME/.local/opt/cc-formed/
  x86_64-unknown-linux-musl/
    v0.1.0/
    current -> v0.1.0
$HOME/.local/bin/
  gcc-formed -> $HOME/.local/opt/cc-formed/x86_64-unknown-linux-musl/current/bin/gcc-formed
  g++-formed -> $HOME/.local/opt/cc-formed/x86_64-unknown-linux-musl/current/bin/g++-formed
```

`$HOME` が複数 architecture で共有される環境を考慮し、user-local install root にも **target triple を含める**。

### 9.3 install transaction

install/upgrade は以下の順で行う。

1. artifact download / locate
2. checksum verification
3. staging directory へ展開
4. `manifest.json` と target triple を検証
5. `--formed-self-check` を実行
6. `current` symlink を atomic に更新
7. 旧 version を retention policy に従って整理

何らかの失敗でも、`current` は旧 version を指したままでなければならない。

### 9.4 uninstall

uninstall は 2 つの mode を持つ。

- **remove-version**: 指定 version の payload だけ削除
- **purge-install**: current symlink と payload をすべて削除

規則:

1. uninstall は user config/state を default で削除してはならない
2. `--purge-state` のような明示 opt-in がある場合のみ cache/state を削除してよい
3. install root と state root は別物として扱う

### 9.5 shared binary bits と alias

`gcc-formed` と `g++-formed` は、同一 binary の `argv[0]` dispatch で実装してよい。

規則:

1. mode 判定は executable name で行ってよい
2. alias 名によって build metadata や version が変わってはならない
3. plain `--version` の compiler-compatible semantics と wrapper introspection は混同しない

---

## 10. Runtime 契約

### 10.1 host runtime の前提

本製品の host-side runtime 前提は最小にする。

MUST:

- ELF 実行可能ファイルとして起動できること
- filesystem へ read/write できること（config/cache/state/runtime 用）
- subprocess として backend compiler を起動できること
- TTY/pipe の標準入出力が使えること

MUST NOT:

- Python/Node/Java runtime を必須にする
- background daemon を必須にする
- local DB, message broker, socket service を必須にする
- network access を通常実行で必須にする

### 10.2 外部依存の境界

本製品の runtime 外部依存は **backend compiler toolchain** に限定する。

v1alpha の明示的 external dependency:

- `gcc` / `g++` backend binary
- assembler / linker など GCC toolchain が起動する外部実行ファイル

それ以外の依存は product bundle 内または OS primitive に留める。

### 10.3 privilege model

本製品は root 権限を前提にしてはならない。

- rootless install を first-class にする
- 通常の compile 実行は non-root で成立しなければならない
- setuid / privileged helper は持たない
- system-wide install だけが管理者権限を必要としてよい

### 10.4 no daemon / no background process

wrapper 実行は 1 invocation 単位で完結する。

規則:

1. short-lived subprocess と一時ファイルのみを前提とする
2. 常駐 service, user agent, launchd/systemd user service を要求しない
3. lock file や runtime object が残っても、次回実行で安全に回復できること

### 10.5 network policy

本製品は compile hot path で network access を行ってはならない。

MUST NOT:

- telemetry upload
- update check
- remote config fetch
- remote schema fetch
- remote symbol lookup

例外は、将来の明示 opt-in support tool に限る。この場合でも hot path から分離されなければならない。

---

## 11. Config / cache / state / runtime layout

### 11.1 path policy

XDG 準拠の既定 path は以下とする。 [R3]

| 種別 | 既定 path | 用途 |
|---|---|---|
| config | `$XDG_CONFIG_HOME/cc-formed/config.toml` | user 設定 |
| admin config | `$XDG_CONFIG_DIRS/cc-formed/config.toml` | system-wide default |
| cache | `$XDG_CACHE_HOME/cc-formed/` | 再生成可能 cache |
| state | `$XDG_STATE_HOME/cc-formed/` | trace index, local history, persistent non-portable state |
| runtime | `$XDG_RUNTIME_DIR/cc-formed/` | per-session temp object, lock, socket 予備 |

`XDG_*` が未設定のときは specification の既定値を用いる。 [R3]

### 11.2 install payload と state の分離

以下を明確に分ける。

- install payload: versioned binary, docs, manifest
- config: policy override
- cache: delete-safe
- state: support/troubleshooting に必要だが bundle ではないもの
- runtime: process/session bound

この分離は MUST である。install root に trace や temp file を書いてはならない。

### 11.3 precedence

runtime 設定の優先順位は次の通り。

1. wrapper-specific CLI option (`--formed-*`)
2. 明示的 environment variable (`FORMED_*`)
3. user config file
4. admin config file
5. built-in defaults

plain GCC option や GCC 環境変数と衝突する場合は、`gcc-adapter-ingestion-spec.md` の conflict policy に従う。

### 11.4 既定環境変数

v1alpha で予約する wrapper-owned environment variable 名は以下。

- `FORMED_CONFIG_FILE`
- `FORMED_CONFIG_DIR`
- `FORMED_CACHE_DIR`
- `FORMED_STATE_DIR`
- `FORMED_RUNTIME_DIR`
- `FORMED_TRACE_DIR`
- `FORMED_INSTALL_ROOT`
- `FORMED_BACKEND_GCC`

これらは将来の public surface になりうるため、互換性境界として扱う。

### 11.5 permission policy

- config file: `0600` または user の umask でそれ以下
- state / cache / runtime directory: `0700` を既定
- raw stderr, SARIF sidecar, trace bundle: `0600`
- detached manifest / checksums は read-only でよい

raw capture には file path や source excerpt が含まれうるため、world-readable にしてはならない。

### 11.6 runtime fallback

`XDG_RUNTIME_DIR` が未設定のとき、wrapper は private temp directory を作って代用してよい。だが user-facing compiler output に noisy warning を出してはならない。

規則:

1. fallback directory は `0700` であること
2. session-end cleanup は best-effort
3. fallback 発生は trace に記録する
4. compile hot path の stderr に install/runtime warning を混ぜない

### 11.7 retention policy

既定の retention policy は以下。

- cache: size-based pruning（soft cap 256 MiB 推奨）
- runtime: process 終了時に best-effort cleanup
- trace bundle: default-off。enabled 時は 14 日または 2 GiB の小さい方で prune 推奨
- previous installs: 直近 2 version を保持推奨

保持期間は policy/config で変えられてよいが、**default で無制限に増え続けてはならない**。

---

## 12. Release engineering と reproducibility

### 12.1 toolchain pin

release build は pin された stable Rust toolchain を使う。

規則:

1. `rust-toolchain.toml` または同等手段で toolchain version を固定する
2. nightly を shipped path に持ち込まない
3. release note / manifest に rustc/cargo version を記録する

### 12.2 lockfile / vendor / offline policy

release build は **lockfile 固定 + vendored dependency + offline build** を原則とする。 [R4][R5][R6][R7]

必須条件:

1. `Cargo.lock` を VCS に commit する
2. release build は `cargo build --locked` で行う
3. hermetic release step は `--offline` を使う
4. remote dependency は事前に `cargo vendor` 等で local source 化する
5. source replacement を使い、release step で crates.io へ到達しない

### 12.3 二段階 build model

推奨 build model は以下。

#### Stage A: dependency preparation

- trusted network 環境で dependency resolve / fetch
- lockfile 検証
- vendor directory 生成
- vendor hash 生成

#### Stage B: hermetic release build

- network disabled
- vendored source のみ利用
- `cargo build --locked --offline --release`
- artifact, manifest, checksum, license report, debug artifact を生成

### 12.4 deterministic build の最小条件

release pipeline は少なくとも以下を固定する。

- source revision
- toolchain revision
- target triple
- build profile
- Cargo.lock
- vendor hash
- feature flag set
- build environment variable allowlist

規則:

1. dirty working tree build を release artifact にしてはならない
2. release manifest から build inputs を追跡できること
3. build step が time-of-day や host PATH に過度依存してはならない

### 12.5 dependency policy との接続

packaging/release では、依存健全性を manifest の一部として扱う。

release artifact には少なくとも以下の report を紐づける。

- dependency license report
- dependency inventory
- vulnerability scan result またはその参照
- allowed/denied dependency policy verdict

### 12.6 build outputs の分類

release pipeline は生成物を以下に分類する。

- **installable artifact**: end user が使う binary archive
- **support artifact**: debug symbols, source bundle, provenance data
- **release control artifact**: manifest, checksum, signature, license report
- **internal CI artifact**: logs, raw intermediate, temporary bundle

shipped artifact と support-only artifact を混同してはならない。

---

## 13. Distribution channel policy

### 13.1 primary channel

primary channel は **immutable versioned binary archive を置く内部 artifact repository** とする。

規則:

1. URL / path は version immutability を前提とする
2. `latest` のような mutable alias は convenience であり、製品契約の主語にしない
3. CI は immutable version + checksum を pin する
4. promote（canary → beta → stable）は artifact 再build ではなく metadata の昇格で行う
5. stable cut workflow は prior GitHub Release の immutable `.release-repo.tar.gz` bundle を seed に使い、same bits を GitHub Release asset と release-repo bundle の両方へ再公開しなければならない

### 13.2 secondary channel

secondary channel として以下を将来的に許容する。

- `deb`
- `rpm`
- Homebrew formula
- asdf plugin

ただし v1alpha の契約では、これらは **same bits repackaging** に限る。

### 13.3 container image

container image は primary channel ではない。使うとしても **CI convenience** に留める。

規則:

1. image 内の binary は primary artifact と同一 build であること
2. local developer 導入の唯一経路にしてはならない
3. container image 前提で compile hot path を設計してはならない

### 13.4 source bundle

source bundle は T4 escape hatch とし、以下を含めることを推奨する。

- source tree
- `Cargo.lock`
- vendor metadata または vendor snapshot 参照
- build instructions
- manifest / commit / tag 対応情報

ただし source build は end user primary path ではない。

### 13.5 installer script policy

installer script がある場合、以下を守る。

1. convenience に留める
2. artifact verification をスキップしてはならない
3. destructive shell init modification をしてはならない
4. `curl | sh` を primary install story にしてはならない
5. local file install と predownloaded artifact install をサポートしてよい

---

## 14. Update / rollback / rollout policy

### 14.1 release channels

運用 channel は以下を持つ。

- `canary`
- `beta`
- `stable`

規則:

1. すべて immutable version を背後に持つ
2. channel promote は artifact rebuild ではなく metadata 更新で行う
3. CI/release workflow は channel 名ではなく exact version を pin する
4. stable release workflow は publish / resolve / channel pointer の checksum と signing metadata を artifact として保存し、no-rebuild evidence を残さなければならない

### 14.2 developer machine の update 方針

developer machine では convenience のため channel install を許容してよいが、install 完了後は必ず **exact installed version** を表示・保存しなければならない。

### 14.3 rollback

rollback は以下の最小手順で成立しなければならない。

1. previous version が install root に残っている
2. `current` symlink を旧 version へ戻せる
3. shell init や config migration を追加で巻き戻さなくてよい
4. rollback 後、`--formed-version` で old version が確認できる
5. stable cut の rollback drill は artifact として保存し、managed launcher refresh ではなく 1 回の `current` symlink switch で成立することを示さなければならない

### 14.4 config migration

v1alpha では **breaking config migration を導入しない** ことを強く推奨する。

必要になった場合の規則:

1. config schema version を持つ
2. migration は one-way 自動変換より explicit backup を優先する
3. rollback と config 破壊が連動してはならない

### 14.5 CI pinning

CI は以下を MUST とする。

- exact version pin
- checksum pin
- artifact cache key に version と target triple を含める
- floating latest を使わない

CI での install 成功だけでなく、**rollback 可能性** も release readiness の一部として扱う。

---

## 15. 運用・サポート契約

### 15.1 wrapper introspection command

運用のため、wrapper は compiler-compatible surface と衝突しない namespaced introspection を持つべきである。

v1alpha で推奨する最低 surface:

- `--formed-version`
- `--formed-version=verbose`
- `--formed-print-paths`
- `--formed-self-check`
- `--formed-dump-build-manifest`

plain `--version` の意味を wrapper 独自都合で壊してはならない。

### 15.2 `--formed-version=verbose` の最小出力

- product version
- target triple
- git commit
- build profile
- rustc/cargo version
- build timestamp or source epoch
- artifact support class
- IR/adapter/renderer spec version
- install root
- config path

### 15.3 self-check の役割

`--formed-self-check` は少なくとも次を確認する。

- binary 自身の起動可能性
- install root / state root への基本アクセス
- target triple と manifest の整合
- backend compiler 解決の最小確認（存在確認レベルでよい）

self-check は network に依存してはならない。

### 15.4 support ticket の最小情報

運用上の一次切り分けでは、少なくとも以下を収集する。

- `--formed-version=verbose`
- backend compiler version
- target triple
- install channel / artifact URL / checksum
- failing command line（必要に応じて redaction）
- opt-in trace bundle の有無

### 15.5 trace bundle 方針

telemetry は default-off とし、support 用 trace bundle は opt-in で収集する。

規則:

1. default runtime で external upload をしてはならない
2. trace bundle は local file として生成する
3. trace bundle は redaction policy に従う
4. trace bundle path は state root 配下または user-specified path とする

---

## 16. セキュリティ・ライセンス・依存運用

### 16.1 dependency class policy

単一バイナリ・将来 OSS 化・社内再配布を前提に、dependency は原則として permissive license を優先する。

規則:

1. paid / closed dependency は禁止
2. static single-binary distribution と相性の悪い dependency は architecture review を必須とする
3. license allow/deny policy は release gate に接続する
4. transitive dependency も inventory 対象とする

### 16.2 system library 依存

primary musl artifact は、system library 依存を極力持たない設計にする。

したがって以下を強く推奨する。

- OpenSSL など heavyweight shared lib 依存を core path に入れない
- native dependency を避けられる crate を優先する
- `build.rs` で host 固有 probing を必要とする dependency を最小化する

### 16.3 secrets / privacy

- trace bundle は secret 漏えいリスクを前提に扱う
- file path, username, repo path, source snippet を含みうることを明示する
- trace bundle は default-off
- install manifest や build info に secret を入れてはならない

### 16.4 no auto-update / no remote control

security 上も運用上も、v1alpha では以下を禁止する。

- self-update
- remote kill-switch
- remote feature flag fetch
- remote execution hook

---

## 17. Packaging quality gate

### 17.1 release blocking test

release candidate では少なくとも以下を block する。

1. clean machine rootless install
2. clean machine system-wide install
3. exact version pin install
4. checksum verification failure path
5. corrupted artifact rejection
6. upgrade without in-place overwrite
7. rollback by symlink switch
8. uninstall without state loss
9. same artifact on local/CI smoke test
10. `gcc-formed` / `g++-formed` alias dispatch test
11. XDG path resolution test
12. `--formed-self-check` success on supported fleet images

### 17.2 release blocker の定義

以下は packaging blocker とする。

- install 後に binary が起動しない
- rollback が即時に成立しない
- state と install payload が混在して消し分け不能
- CI と local で artifact が異なる
- checksum mismatch が検出されない
- artifact manifest が欠けている

### 17.3 matrix

packaging gate は少なくとも次の matrix を持つ。

- supported target triple ごと
- user-local / system-wide ごと
- canary / stable channel install ごと
- upgrade / rollback / uninstall ごと

### 17.4 shared-home architecture test

`$HOME` 共有環境では compiled binary path が architecture-specific になりうる。したがって installer は **install root に target triple を含める** 方針を守る。 [R3]

この前提を壊す回帰は packaging defect とする。

---

## 18. 実装ガイド（v1alpha の推奨具体案）

### 18.1 初期 artifact set

最初に本番運用へ出す artifact は以下を推奨する。

- `gcc-formed-v0.1.0-linux-x86_64-musl.tar.gz`
- `gcc-formed-v0.1.0-linux-x86_64-musl.debug.tar.gz`
- `gcc-formed-v0.1.0-source.tar.gz`
- `SHA256SUMS`
- detached signature
- `manifest.json`

### 18.2 初期 install story

初期 install story は次の 2 本に絞る。

1. user-local install to `$HOME/.local/opt/cc-formed/...`
2. system-wide install to `/opt/cc-formed/...` + `/usr/local/bin` symlink

`deb` / `rpm` は rollout-ready まで必須にしない。

### 18.3 初期運用 story

初期運用では以下を固定する。

- stable / beta / canary の 3 channel
- CI は stable exact version pin
- dogfood は canary
- rollback は symlink 差し戻し
- trace bundle は opt-in
- no auto-update

### 18.4 最初に切るべきもの

v1alpha で後回しにすべきもの:

- self-updater
- package-manager-native build differentiation
- container-primary distribution
- Windows/macOS installer 詳細
- automatic config migration framework
- background cache service

---

## 19. 本仕様で固定された最終判断

本仕様の最終判断を簡潔に再掲する。

1. **配布の正本は primary binary archive である**
2. **Linux first の本命 artifact は `x86_64-unknown-linux-musl`**
3. **shipped runtime に Python/venv を持ち込まない**
4. **package manager は secondary channel であり、same bits repackaging を原則とする**
5. **install は versioned root + atomic symlink switch**
6. **config/cache/state/runtime は XDG に分離する**
7. **release build は lock/vendor/offline を必須にする**
8. **CI と developer machine は同じ artifact を使う**
9. **self-update は持たない**
10. **rollback は 1 symlink 操作で成立する設計にする**

この判断により、`gcc-formed` は「配布方法ごとに別物になる CLI」ではなく、
**単一 artifact を中心に install / rollback / support が回る長寿命の compiler wrapper 製品** になる。

---

## 付録 A: 参考にした公開資料

- **[R1] Rust Platform Support**  
  https://doc.rust-lang.org/rustc/platform-support.html

- **[R2] Python `venv` documentation**  
  https://docs.python.org/3/library/venv.html

- **[R3] XDG Base Directory Specification**  
  https://specifications.freedesktop.org/basedir/latest/

- **[R4] Cargo build (`--locked`, `--offline`)**  
  https://doc.rust-lang.org/cargo/commands/cargo-build.html

- **[R5] Cargo vendor**  
  https://doc.rust-lang.org/cargo/commands/cargo-vendor.html

- **[R6] Cargo source replacement**  
  https://doc.rust-lang.org/cargo/reference/source-replacement.html

- **[R7] Cargo.lock purpose**  
  https://doc.rust-lang.org/cargo/guide/cargo-toml-vs-cargo-lock.html

---

## 付録 B: 初回実装チェックリスト

### B.1 artifact

- [x] `x86_64-unknown-linux-musl` primary artifact を生成できる
- [x] `gcc-formed` / `g++-formed` が同一 bits である
- [x] `manifest.json` に target / version / commit / lock hash が入る
- [x] checksum と detached signature を生成できる
- [x] debug artifact を保持できる

### B.2 install / rollback

- [x] user-local install が rootless で成立する
- [x] system-wide install が `/opt/cc-formed/...` モデルで成立する
- [x] symlink swap upgrade が成立する
- [x] rollback で旧 binary に戻せる
- [x] uninstall が state を壊さない

### B.3 XDG / state

- [x] config / cache / state / runtime が分離される
- [x] state dir permission が `0700` 既定である
- [x] trace bundle が install root に書かれない
- [x] `XDG_RUNTIME_DIR` 未設定 fallback が安全に動く

### B.4 release engineering

- [x] `Cargo.lock` を commit している
- [x] `cargo vendor` で依存を local 化できる
- [x] `cargo build --locked --offline --release` が通る
- [x] dirty tree release が禁止されている
- [x] version / commit / rustc/cargo version が manifest に残る

### B.5 operations

- [x] `--formed-version` がある
- [x] `--formed-version=verbose` が build metadata を出す
- [x] `--formed-self-check` が install/host/runtime を検査できる
- [x] CI が exact version + checksum を pin する
- [x] canary / beta / stable の promote が rebuild なしで行える
