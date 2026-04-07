# gcc-formed

- **状態**: Accepted Baseline
- **フェーズ**: `v1alpha`
- **日付**: 2026-04-07
- **実装状況**: まだコードは存在せず、現時点のリポジトリは仕様書と ADR のみ

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
- 実装は未着手で、Cargo workspace、CLI、ソースコード、テストハーネスはまだ存在しない
- 今後の判断追加や変更は、仕様書への追記ではなく ADR の追加または supersede で行う

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
