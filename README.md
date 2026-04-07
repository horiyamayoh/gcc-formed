# gcc-formed

- **状態**: Accepted Baseline
- **フェーズ**: `v1alpha`
- **日付**: 2026-04-07
- **実装状況**: Phase 1 MVP の Rust workspace を同梱。仕様書と ADR は実装契約の正本として維持する。

`gcc-formed` は、GCC first / Linux first の C/C++ 診断 UX 基盤を定義する spec-first リポジトリである。目標は「コンパイラの生出力を prettier にすること」ではなく、wrapper・adapter・Diagnostic IR・renderer・quality gate を分離した実装可能な製品基線を固めることにある。

## このリポジトリにあるもの

- [gcc-formed-architecture-proposal.md](gcc-formed-architecture-proposal.md): 上位設計と v1alpha の意思決定候補
- [diagnostic-ir-v1alpha-spec.md](diagnostic-ir-v1alpha-spec.md): 正規化 IR の実装契約
- [gcc-adapter-ingestion-spec.md](gcc-adapter-ingestion-spec.md): GCC 呼び出しと structured ingestion の実装契約
- [rendering-ux-contract-spec.md](rendering-ux-contract-spec.md): terminal / CI renderer の実装契約
- [quality-corpus-test-gate-spec.md](quality-corpus-test-gate-spec.md): corpus-driven 品質 gate の実装契約
- [packaging-runtime-operations-spec.md](packaging-runtime-operations-spec.md): 配布・install・rollback・release engineering の実装契約
- [implementation-bootstrap-sequence.md](implementation-bootstrap-sequence.md): 実装開始時の最小順序
- [adr-initial-set/README.md](adr-initial-set/README.md): Accepted baseline の ADR 一覧

## 現在の基線

- 仕様上の正本は 5 本の主要仕様書と `adr-initial-set/` 配下の ADR 20 本
- 実装は Cargo workspace として存在し、wrapper CLI、IR、adapter、renderer、trace、testkit、xtask を含む
- `cargo xtask package` により release artifact / control file の最小セットを生成でき、必要なら `SHA256SUMS.sig` を Ed25519 で付与できる
- `cargo xtask install` / `rollback` / `uninstall` により versioned root + symlink switch の install story をローカルで検証でき、署名付き release は signing key id pin でも検証できる
- `cargo xtask vendor` / `hermetic-release-check` により vendored dependency + offline locked release build を検証できる
- `cargo xtask release-publish` / `release-promote` / `release-resolve` / `install-release` により immutable version repository, metadata-only channel promote, exact-version + checksum pin install を検証できる
- `/opt/cc-formed/...` + `/usr/local/bin` 相当の system-wide layout も pseudo-root smoke で検証している
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
- `diag_cli_front`: `gcc-formed` wrapper CLI
- `xtask`: `check`, `replay`, `snapshot`, `bench-smoke`, `self-check`, `package`, `install`, `rollback`, `uninstall`, `vendor`, `hermetic-release-check`, `release-publish`, `release-promote`, `release-resolve`, `install-release`

## 開発開始

```bash
cargo xtask check
cargo xtask replay --root corpus
cargo build --bin gcc-formed
./target/debug/gcc-formed --formed-self-check
cargo xtask vendor --output-dir vendor
cargo xtask hermetic-release-check --vendor-dir vendor --bin gcc-formed
cargo xtask package --binary target/debug/gcc-formed --target-triple x86_64-unknown-linux-gnu
```

生成された control dir を使って install / rollback / uninstall を検証する最小例:

```bash
control_dir=dist/gcc-formed-v0.1.0-linux-x86_64-gnu
install_root="$HOME/.local/opt/cc-formed/x86_64-unknown-linux-gnu"
bin_dir="$HOME/.local/bin"

cargo xtask install --control-dir "$control_dir" --install-root "$install_root" --bin-dir "$bin_dir"
"$bin_dir/gcc-formed" --formed-version
cargo xtask rollback --install-root "$install_root" --bin-dir "$bin_dir" --version 0.1.0
cargo xtask uninstall --install-root "$install_root" --bin-dir "$bin_dir" --mode purge-install
```

署名付き release を作って signing key id pin で検証する最小例:

```bash
signing_private_key="$PWD/release-signing.key"
control_dir=dist/gcc-formed-v0.1.0-linux-x86_64-gnu

cargo xtask package \
  --binary target/debug/gcc-formed \
  --target-triple x86_64-unknown-linux-gnu \
  --signing-private-key "$signing_private_key"

signing_key_id="$(python3 - "$control_dir/SHA256SUMS.sig" <<'PY'
import json
import sys

with open(sys.argv[1], "r", encoding="utf-8") as handle:
    print(json.load(handle)["key_id"])
PY
)"

cargo xtask install \
  --control-dir "$control_dir" \
  --install-root "$install_root" \
  --bin-dir "$bin_dir" \
  --expected-signing-key-id "$signing_key_id"
```

immutable release repository と exact pin install を検証する最小例:

```bash
repo_root="$PWD/.release-repo"

cargo xtask release-publish --control-dir "$control_dir" --repository-root "$repo_root"
cargo xtask release-promote --repository-root "$repo_root" --target-triple x86_64-unknown-linux-gnu --version 0.1.0 --channel canary
cargo xtask release-promote --repository-root "$repo_root" --target-triple x86_64-unknown-linux-gnu --version 0.1.0 --channel stable
cargo xtask release-resolve --repository-root "$repo_root" --target-triple x86_64-unknown-linux-gnu --channel stable
cargo xtask install-release --repository-root "$repo_root" --target-triple x86_64-unknown-linux-gnu --channel stable --install-root "$install_root" --bin-dir "$bin_dir"
```

CI の exact version + checksum pin を再現したい場合は、`release-resolve` の JSON 出力から `resolved_version` と `primary_archive_sha256` を取り出し、`install-release --version ... --expected-primary-sha256 ...` を使う。署名も併用するなら `signing_key_id` を取り出し、`--expected-signing-key-id ...` を追加する。

system-wide layout を pseudo-root で検証する最小例:

```bash
system_root="$PWD/.system-root"
install_root="$system_root/opt/cc-formed/x86_64-unknown-linux-gnu"
bin_dir="$system_root/usr/local/bin"

cargo xtask install \
  --control-dir "$control_dir" \
  --install-root "$install_root" \
  --bin-dir "$bin_dir" \
  --expected-signing-key-id "$signing_key_id"
```

本命の production artifact は引き続き `x86_64-unknown-linux-musl` であり、`x86_64-unknown-linux-gnu` は compatibility smoke / 例外経路として扱う。

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
