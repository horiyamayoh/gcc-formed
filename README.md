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
- `cargo xtask package` により release artifact / control file の最小セットを生成できる
- `cargo xtask install` / `rollback` / `uninstall` により versioned root + symlink switch の install story をローカルで検証できる
- `cargo xtask vendor` / `hermetic-release-check` により vendored dependency + offline locked release build を検証できる
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
- `xtask`: `check`, `replay`, `snapshot`, `bench-smoke`, `self-check`

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
