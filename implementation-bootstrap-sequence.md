# gcc-formed v1alpha 実装開始シーケンス

- **文書種別**: 実装受け渡しメモ
- **状態**: Accepted Baseline
- **版**: `v1alpha`
- **日付**: 2026-04-07
- **関連文書**:
  - `gcc-formed-architecture-proposal.md`
  - `gcc-adapter-ingestion-spec.md`
  - `diagnostic-ir-v1alpha-spec.md`
  - `rendering-ux-contract-spec.md`
  - `adr-initial-set/README.md`

---

## 1. 目的

この文書は、仕様整理が完了した状態から実装チームが最初にどの順序で着手するかを固定するための薄い受け渡し文書である。ここでは backlog 全体を展開せず、既存仕様で合意済みの最小順序だけを再掲する。

## 2. 固定する実装順序

v1alpha の初手は次の 6 段階に限定する。

1. **backend resolution**
   - real compiler discovery と passthrough-only path を成立させる
   - 出典: `gcc-adapter-ingestion-spec.md` の最低限の実装順序 1

2. **capture runtime**
   - child spawn、pipe drain、raw stderr 保持、exit status 整合を固める
   - 出典: `gcc-adapter-ingestion-spec.md` の最低限の実装順序 2

3. **GCC 15 shadow**
   - `-fdiagnostics-add-output=sarif:file=...` による shadow capture を入れる
   - 出典: `gcc-adapter-ingestion-spec.md` の最低限の実装順序 3

4. **SARIF parser**
   - GCC structured output を `DiagnosticDocument` へ写像する
   - 出典: `gcc-adapter-ingestion-spec.md` の最低限の実装順序 4
   - 出典: `diagnostic-ir-v1alpha-spec.md` の MVP subset

5. **render**
   - terminal / CI 向け最小 renderer をつなぎ、structured path の end-to-end を通す
   - 出典: `gcc-adapter-ingestion-spec.md` の最低限の実装順序 5
   - 出典: `rendering-ux-contract-spec.md` の v1alpha contract

6. **raw fallback**
   - parse failure、unsupported tier、internal failure で fail-open に戻す
   - 出典: `gcc-adapter-ingestion-spec.md` の最低限の実装順序 6

## 3. この文書で扱わないもの

- Rust workspace や crate 名の最終確定
- corpus seed の具体的な収集運用
- packaging pipeline の自動化詳細
- post-MVP backlog の優先順位

これらは各仕様書と ADR 群を参照して、次の実装計画フェーズで詳細化する。
