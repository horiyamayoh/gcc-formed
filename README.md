# gcc-formed / cc-formed ADR 初版セット

- **文書種別**: Architecture Decision Record（ADR）索引
- **状態**: Initial Baseline
- **版**: `v1alpha`
- **日付**: 2026-04-07
- **対象**: `gcc-formed` / 将来の `cc-formed`
- **関連文書**:
  - `../gcc-formed-architecture-proposal.md`
  - `../diagnostic-ir-v1alpha-spec.md`
  - `../gcc-adapter-ingestion-spec.md`
  - `../rendering-ux-contract-spec.md`
  - `../quality-corpus-test-gate-spec.md`

---

## 1. この ADR セットの目的

本ディレクトリは、既に確定した上位設計と各種仕様書を、**実装チームが迷わず着手できる短い意思決定記録**に落としたものである。  
ここでの ADR は、設計思想の繰り返しではなく、次の用途を持つ。

1. 実装着手前に揺れやすい判断点を固定する
2. 「なぜその案を採らなかったか」を将来に残す
3. 仕様書の責務境界を、コード上の ownership に写像する
4. 将来の Clang 対応や IDE 連携時に、どこを再検討すべきかを明示する

---

## 2. 含まれる ADR 一覧

| ADR | Title | Status | 目的 |
|---|---|---|---|
| [ADR-0001](./adr-0001-wrapper-first-entrypoint.md) | Wrapper-first compiler-compatible entrypoint | Accepted | 導入障壁を最小化する |
| [ADR-0002](./adr-0002-diagnostic-ir-as-product-core.md) | Diagnostic IR as product core | Accepted | compiler/renderer/analysis を疎結合化する |
| [ADR-0003](./adr-0003-structured-first-gcc-ingestion-no-plugin-dependency.md) | Structured-first GCC ingestion; no plugin dependency | Accepted | GCC 診断の authoritative source を固定する |
| [ADR-0004](./adr-0004-gcc-support-tier-policy.md) | GCC support tier policy | Accepted | 対応範囲と品質主張を明確にする |
| [ADR-0005](./adr-0005-fail-open-fallback-and-provenance.md) | Fail-open fallback and provenance preservation | Accepted | wrapper が体験を悪化させないようにする |
| [ADR-0006](./adr-0006-renderer-separation-and-profile-based-output.md) | Renderer separation and profile-based output | Accepted | TTY/CI/将来の editor を同一 IR で支える |
| [ADR-0007](./adr-0007-rust-as-implementation-language.md) | Rust as implementation language | Accepted | 長期品質と配布性を両立する |
| [ADR-0008](./adr-0008-linux-first-single-binary-distribution.md) | Linux-first single-binary distribution | Accepted | 社内配布の安定性を上げる |
| [ADR-0009](./adr-0009-default-off-telemetry-and-opt-in-trace-bundles.md) | Default-off telemetry and opt-in trace bundles | Accepted | 観測可能性と機密性を両立する |
| [ADR-0010](./adr-0010-corpus-driven-quality-gate-and-snapshot-governance.md) | Corpus-driven quality gate and snapshot governance | Accepted | 品質を人間の感覚ではなく gate にする |

---

## 3. このセットの読み方

- **0001–0005** は、プロダクトの骨格を固定する基礎 ADR。
- **0006–0008** は、実装言語・配布・UX 境界を固定する運用 ADR。
- **0009–0010** は、観測と品質を release 可能な形にする統制 ADR。

実装着手の観点では、少なくとも **ADR-0001〜0008** は着手前に accepted であるべきである。  
**ADR-0009〜0010** は、Phase 1 の途中で曖昧なままだと rollout-ready に到達しにくい。

---

## 4. ステータス運用ルール

本セットでは ADR の状態を次の 5 つで扱う。

- **Proposed**: 提案中
- **Accepted**: 現時点で採用
- **Superseded**: 後続 ADR に置換された
- **Deprecated**: 互換のため記録は残すが新規採用しない
- **Rejected**: 検討したが不採用

初版セットでは、**v1alpha 実装の基線として全て Accepted** とする。  
ただし、以下の条件では改定または supersede を検討する。

- GCC 側の structured diagnostic 契約が大きく変わった
- Clang adapter 導入で IR または support tier に無理が出た
- 単一バイナリ配布の前提を壊す運用制約が判明した
- quality gate が velocity を過度に阻害している
- fail-open では吸収しきれない fidelity 問題が継続的に発生した

---

## 5. 更新原則

ADR の更新は自由文ではなく、**必ず新しい決定か supersede か** のいずれかで行う。

- 単なる補足説明: 既存 ADR の追記
- 実質的な判断変更: 新しい ADR を追加し、旧 ADR を `Superseded`
- 運用上の例外: 補遺または support policy 文書へ移す

---

## 6. 実装への直結ポイント

この ADR セットにより、最初の 90 日で少なくとも次を迷わず始められる。

1. wrapper / subprocess 層の実装
2. Diagnostic IR validator と fixture schema
3. GCC 15 structured-first adapter
4. renderer/view-model の骨格
5. fallback / provenance / trace bundle
6. curated corpus と snapshot gate の基盤

---

## 7. 参照

外部事実に依存する ADR では、各ファイル末尾の `References` を参照すること。  
原則として **一次情報（公式マニュアル、公式リリースノート、公式言語ドキュメント）** のみを参照する。
