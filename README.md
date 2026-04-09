# gcc-formed

- **状態**: Public Beta
- **成熟度ラベル**: `v1beta`
- **artifact semver 系列**: `0.2.0-beta.N`
- **一般利用向け安定版**: 未提供
- **日付**: 2026-04-09
- **実装状況**: Phase 1 MVP の Rust workspace を同梱。仕様書と ADR は実装契約の正本として維持する。

`gcc-formed` は、GCC first / Linux first の C/C++ 診断 UX 基盤を定義する spec-first リポジトリである。目標は「コンパイラの生出力を prettier にすること」ではなく、wrapper・adapter・Diagnostic IR・renderer・quality gate を分離した実装可能な製品基線を固めることにある。

成熟度ラベルと artifact semver の使い分けは [VERSIONING.md](VERSIONING.md) に固定する。現在の baseline は **`v1beta` という成熟度ラベル**と**`0.2.0-beta.N` という artifact 系列**を別物として扱う。

## 現在の support boundary

現在の public beta artifact 系列 (`0.2.0-beta.N`、先頭 artifact は `0.2.0-beta.1`) の support boundary wording は [SUPPORT-BOUNDARY.md](SUPPORT-BOUNDARY.md) に固定する。README でも同じ文言をそのまま使う。

- Linux first.
- `x86_64-unknown-linux-musl` is the primary production artifact.
- GCC 15 is the primary enhanced-render path.
- The terminal renderer is the primary user-facing surface.
- GCC 13/14 are compatibility-only paths and may use conservative passthrough or shadow behavior instead of the primary enhanced-render path.
- Raw fallback remains part of the shipped contract when the wrapper cannot produce a clearly better, trustworthy render.

現在の public beta baseline で**保証しないもの**は [KNOWN-LIMITATIONS.md](KNOWN-LIMITATIONS.md) と [RELEASE-CHECKLIST.md](RELEASE-CHECKLIST.md) にまとめてある。

release repository の `canary` / `beta` / `stable` channel は配布先ポインタであり、成熟度ラベルそのものではない。たとえば `0.2.0-beta.1` artifact が `beta` channel に載っていても、それは引き続き `v1beta` の artifact であり、`1.0.0 stable` を意味しない。

## このリポジトリにあるもの

- [gcc-formed-architecture-proposal.md](gcc-formed-architecture-proposal.md): 上位設計と v1alpha の意思決定候補
- [diagnostic-ir-v1alpha-spec.md](diagnostic-ir-v1alpha-spec.md): 正規化 IR の実装契約
- [gcc-adapter-ingestion-spec.md](gcc-adapter-ingestion-spec.md): GCC 呼び出しと structured ingestion の実装契約
- [rendering-ux-contract-spec.md](rendering-ux-contract-spec.md): terminal / CI renderer の実装契約
- [quality-corpus-test-gate-spec.md](quality-corpus-test-gate-spec.md): corpus-driven 品質 gate の実装契約
- [packaging-runtime-operations-spec.md](packaging-runtime-operations-spec.md): 配布・install・rollback・release engineering の実装契約
- [implementation-bootstrap-sequence.md](implementation-bootstrap-sequence.md): 実装開始時の最小順序
- [adr-initial-set/README.md](adr-initial-set/README.md): Accepted baseline の ADR 一覧
- [CHANGELOG.md](CHANGELOG.md): 外部向けの変更履歴
- [VERSIONING.md](VERSIONING.md): 成熟度ラベル / artifact semver / release channel の用語契約
- [SUPPORT-BOUNDARY.md](SUPPORT-BOUNDARY.md): `v1beta` / `0.2.0-beta.N` の support boundary の正本
- [KNOWN-LIMITATIONS.md](KNOWN-LIMITATIONS.md): 初回公開時点で保証しない範囲と raw fallback の意味
- [RELEASE-CHECKLIST.md](RELEASE-CHECKLIST.md): 初回公開用の release blocker / non-goals / 出荷前確認項目
- [PUBLIC-BETA-RELEASE.md](PUBLIC-BETA-RELEASE.md): GitHub Releases 上の public beta artifact / install / rollback / exact-pin install の手順
- [SIGNING-KEY-OPERATIONS.md](SIGNING-KEY-OPERATIONS.md): signing key rotation / revoke / emergency re-sign と provenance 保持
- [SECURITY.md](SECURITY.md): 脆弱性報告とサポート方針
- [CONTRIBUTING.md](CONTRIBUTING.md): 変更提案時の gate と contribution 方針
- [corpus/README.md](corpus/README.md): curated corpus の beta-bar target と shadow-to-curated promotion 手順
- [eval/rc/README.md](eval/rc/README.md): RC gate が ingest する manual metrics / issue budget / UX sign-off evidence の置き場
- [fuzz/README.md](fuzz/README.md): deterministic fuzz / adversarial smoke suite

## 現在の基線

- 仕様上の正本は 5 本の主要仕様書と `adr-initial-set/` 配下の ADR 20 本
- 実装は Cargo workspace として存在し、wrapper CLI、IR、adapter、renderer、trace、testkit、xtask を含む
- `cargo xtask package` により release artifact / control file の最小セットを生成でき、必要なら `SHA256SUMS.sig` を Ed25519 で付与できる
- `cargo xtask install` / `rollback` / `uninstall` により versioned root + symlink switch の install story をローカルで検証でき、署名付き release は signing key id と trusted signing public key sha256 pin の両方で検証できる
- `cargo xtask install --dry-run` / `rollback --dry-run` / `uninstall --dry-run` は実変更なしで symlink switch / 配置先 / 削除対象を JSON として確認できる
- `cargo xtask vendor` / `hermetic-release-check` により vendored dependency + offline locked release build を検証できる
- `cargo xtask release-publish` / `release-promote` / `release-resolve` / `install-release` により immutable version repository, metadata-only channel promote, exact-version + checksum pin install を検証できる
- public GitHub Releases には signed primary/debug/source archives, control-dir bundle, immutable release-repo bundle, `SHA256SUMS`, `SHA256SUMS.sig`, `manifest.json`, `build-info.txt`, `release-provenance.json` を載せる
- `/opt/cc-formed/...` + `/usr/local/bin` 相当の system-wide layout も pseudo-root smoke で検証している
- GCC 15 representative corpus に対する acceptance report と snapshot report を `cargo xtask replay --report-dir ...` / `cargo xtask snapshot --report-dir ...` で保存でき、どちらの report も reason-coded fallback 件数を保持する
- renderer は template / overload / macro/include / linker の group card を profile-aware に圧縮し、enhanced path でも `--formed-profile=raw_fallback` への導線を残す
- GitHub Actions は gate ごとに `gate-summary.{json,md}` と `build-environment.json` を artifact として保存し、step failure の class と `rustc` / `cargo` / Docker / GCC version を残す
- GitHub Actions は release smoke ごとに `release-provenance.json` を artifact として保存し、publish/promote/install 系の出荷証跡を残す
- `cargo xtask rc-gate` は curated replay / rollout matrix / benchmark smoke / deterministic replay / fuzz smoke と manual RC evidence を 1 コマンドで集約し、`fuzz-smoke-report.json`, `fuzz-evidence.json`, `metrics-report.json`, `rc-gate-report.json`, `rc-gate-summary.md` を出力できる
- Trace bundle は `unsupported_tier` / `incompatible_sink` / `shadow_mode` / `sarif_missing` / `sarif_parse_failed` / `renderer_low_confidence` などの fallback reason を保持し、fail-open 経路を後追いできる
- `--formed-self-check` は install/runtime/backend 状態に加えて canonical rollout mode matrix を JSON で返し、RC gate が wrapper policy drift を検知できる
- `diag_enrich` は context chain / phase / semantic role / symbol context / ownership を優先した deterministic rule input を使い、message substring 判定は fallback に限定する
- `diag_render` は user-owned location / confidence / phase / semantic role を踏まえて lead group を決め、lead が low-confidence のときは関連する 2 件目 group を展開できる
- `diag_render` の profile budget は `default` / `concise` / `verbose` / `ci` ごとに固定され、warning suppression・excerpt 数・template/macro/include 圧縮・child note 表示が deterministic に揃う。partial document では mixed fallback の `raw:` sub-block を残し、enhanced path の transient object path は `<temp-object>` に正規化する
- 今後の判断追加や変更は、仕様書への追記ではなく ADR の追加または supersede で行う

## 実装ワークスペース

- `diag_core`: Diagnostic IR、validation、canonical JSON、fingerprints
- `diag_backend_probe`: backend discovery と support tier 判定
- `diag_capture_runtime`: child spawn、stderr capture、SARIF sidecar 注入
- `diag_adapter_gcc`: GCC SARIF ingest と residual text 取り込み
- `diag_enrich`: family/ownership/headline/first action の付与
- `diag_render`: terminal/CI/raw fallback renderer
- `diag_trace`: XDG path、trace bundle、build manifest
- `diag_testkit`: corpus fixture loader と validation
- `diag_cli_front`: `gcc-formed` wrapper CLI。`src/main.rs` は dispatch のみとし、internals は `args` / `config` / `mode` / `backend` / `execute` / `render` / `self_check` に分割している
- `xtask`: `check`, `replay`, `snapshot`, `bench-smoke`, `fuzz-smoke`, `rc-gate`, `self-check`, `package`, `install`, `rollback`, `uninstall`, `vendor`, `hermetic-release-check`, `release-publish`, `release-promote`, `release-resolve`, `install-release`

## 開発開始

```bash
rustup target add x86_64-unknown-linux-musl
cargo xtask check
cargo xtask replay --root corpus
cargo build --bin gcc-formed
./target/debug/gcc-formed --formed-self-check
cargo xtask fuzz-smoke --root fuzz --report-dir target/fuzz-smoke
cargo xtask rc-gate --report-dir target/rc-gate --fuzz-root fuzz --metrics-manual-report eval/rc/metrics-manual-eval.json --allow-pending-manual-checks
cargo xtask vendor --output-dir vendor
cargo xtask hermetic-release-check --vendor-dir vendor --bin gcc-formed --target-triple x86_64-unknown-linux-musl
cargo xtask package --binary target/hermetic-release/x86_64-unknown-linux-musl/release/gcc-formed --target-triple x86_64-unknown-linux-musl
```

GitHub Releases から public beta artifact を取得して install / rollback / exact version pin install を行う手順は [PUBLIC-BETA-RELEASE.md](PUBLIC-BETA-RELEASE.md) にまとめてある。public user 向けの導線はこの文書を正本とする。

GitHub CI と同じ `gcc:15` snapshot gate を WSL2 の rootless Docker で再現する最小例:

```bash
sudo apt-get update
sudo apt-get install -y uidmap
sudo modprobe nf_tables

export DOCKER_HOST="unix://$XDG_RUNTIME_DIR/docker.sock"
dockerd-rootless.sh >"${HOME}/.cache/dockerd-rootless.log" 2>&1 &

until docker info >/dev/null 2>&1; do sleep 1; done
docker run --rm hello-world
cargo xtask snapshot --root corpus --subset representative --check --docker-image gcc:15
```

`dockerd-rootless-setuptool.sh check` が前提不足を出す場合は、その指示に従って `uidmap` と kernel module を先に揃える。systemd なしの WSL2 session では `dockerd-rootless.sh` を直接起動するのが最短。

生成された control dir を使って install / rollback / uninstall を検証する最小例:

```bash
control_dir=dist/gcc-formed-v0.2.0-beta.1-linux-x86_64-musl
install_root="$HOME/.local/opt/cc-formed/x86_64-unknown-linux-musl"
bin_dir="$HOME/.local/bin"

cargo xtask install --control-dir "$control_dir" --install-root "$install_root" --bin-dir "$bin_dir" --dry-run
cargo xtask install --control-dir "$control_dir" --install-root "$install_root" --bin-dir "$bin_dir"
"$bin_dir/gcc-formed" --formed-version
cargo xtask rollback --install-root "$install_root" --bin-dir "$bin_dir" --version 0.2.0-beta.1
cargo xtask uninstall --install-root "$install_root" --bin-dir "$bin_dir" --mode purge-install
```

署名付き release を作って signing key id + trusted public key sha256 pin で検証する最小例:

```bash
signing_private_key="$PWD/release-signing.key"
control_dir=dist/gcc-formed-v0.2.0-beta.1-linux-x86_64-musl

cargo xtask package \
  --binary target/hermetic-release/x86_64-unknown-linux-musl/release/gcc-formed \
  --target-triple x86_64-unknown-linux-musl \
  --signing-private-key "$signing_private_key"

signing_key_id="$(python3 - "$control_dir/SHA256SUMS.sig" <<'PY'
import json
import sys

with open(sys.argv[1], "r", encoding="utf-8") as handle:
    print(json.load(handle)["key_id"])
PY
)"

signing_public_key_sha256="$(python3 - "$control_dir/SHA256SUMS.sig" <<'PY'
import hashlib
import json
import sys

with open(sys.argv[1], "r", encoding="utf-8") as handle:
    envelope = json.load(handle)
    print(hashlib.sha256(bytes.fromhex(envelope["public_key_hex"])).hexdigest())
PY
)"

cargo xtask install \
  --control-dir "$control_dir" \
  --install-root "$install_root" \
  --bin-dir "$bin_dir" \
  --expected-signing-key-id "$signing_key_id" \
  --expected-signing-public-key-sha256 "$signing_public_key_sha256"
```

immutable release repository と exact pin install を検証する最小例:

```bash
repo_root="$PWD/.release-repo"

cargo xtask release-publish --control-dir "$control_dir" --repository-root "$repo_root"
cargo xtask release-promote --repository-root "$repo_root" --target-triple x86_64-unknown-linux-musl --version 0.2.0-beta.1 --channel canary
cargo xtask release-promote --repository-root "$repo_root" --target-triple x86_64-unknown-linux-musl --version 0.2.0-beta.1 --channel beta
cargo xtask release-resolve --repository-root "$repo_root" --target-triple x86_64-unknown-linux-musl --channel beta
cargo xtask install-release --repository-root "$repo_root" --target-triple x86_64-unknown-linux-musl --channel beta --install-root "$install_root" --bin-dir "$bin_dir"
```

CI の exact version + checksum pin を再現したい場合は、`release-resolve` の JSON 出力から `resolved_version` と `primary_archive_sha256` を取り出し、`install-release --version ... --expected-primary-sha256 ...` を使う。署名も併用するなら `signing_key_id` と `signing_public_key_sha256` を取り出し、`--expected-signing-key-id ... --expected-signing-public-key-sha256 ...` を追加する。local smoke では `SHA256SUMS.sig` から計算してもよいが、production CI と public beta install は trusted public key sha256 を release notes か別管理チャネルで pin する。

ここでの `beta` は release repository channel 名であり、`v1beta` という成熟度ラベルと同じ意味ではない。`beta` channel も exact version を背後に持つ distribution pointer である。

system-wide layout を pseudo-root で検証する最小例:

```bash
system_root="$PWD/.system-root"
install_root="$system_root/opt/cc-formed/x86_64-unknown-linux-musl"
bin_dir="$system_root/usr/local/bin"

cargo xtask install \
  --control-dir "$control_dir" \
  --install-root "$install_root" \
  --bin-dir "$bin_dir" \
  --expected-signing-key-id "$signing_key_id" \
  --expected-signing-public-key-sha256 "$signing_public_key_sha256"
```

`cargo xtask package` は clean git worktree を前提とする。本命の production artifact は `x86_64-unknown-linux-musl` であり、`x86_64-unknown-linux-gnu` は compatibility smoke / 例外経路として扱う。GCC 13/14 も同様に primary render path ではなく、compatibility support として扱う。

## 実装に入る順序

最初の実装順は [implementation-bootstrap-sequence.md](implementation-bootstrap-sequence.md) に固定してある。v1alpha の初手は次の 6 段階のみを対象とする。

1. backend resolution
2. capture runtime
3. GCC 15 shadow
4. SARIF parser
5. render
6. raw fallback

## 補足

- この README は repo overview 専用であり、ADR 索引本文は [adr-initial-set/README.md](adr-initial-set/README.md) に置く
- 参照パスはこのリポジトリ直下を基準に正規化してある
