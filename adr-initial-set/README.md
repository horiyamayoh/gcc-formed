# gcc-formed / cc-formed ADR 初版セット

- **文書種別**: Architecture Decision Record（ADR）索引
- **状態**: Accepted Baseline
- **版**: `v1beta`
- **日付**: 2026-04-09
- **対象**: `gcc-formed` / 将来の `cc-formed`
- **関連文書**:
  - `../gcc-formed-architecture-proposal.md`
  - `../diagnostic-ir-v1alpha-spec.md`
  - `../gcc-adapter-ingestion-spec.md`
  - `../rendering-ux-contract-spec.md`
  - `../quality-corpus-test-gate-spec.md`
  - `../packaging-runtime-operations-spec.md`
  - `../implementation-bootstrap-sequence.md`

---

## 1. この ADR セットの目的

本ディレクトリは、上位設計と実装契約仕様を「実装チームが迷わず着手できる短い意思決定記録」に変換した正本である。ここでの ADR は設計思想の繰り返しではなく、実装開始前に揺れやすい判断点を固定し、仕様書に新しい判断を足さずに進めるための更新単位として扱う。

## 2. 含まれる ADR 一覧

| ADR | Title | Status | 目的 |
|---|---|---|---|
| [ADR-0001](./adr-0001-wrapper-first-entrypoint.md) | Wrapper-first compiler-compatible entrypoint | Accepted | 導入障壁を最小化する |
| [ADR-0002](./adr-0002-diagnostic-ir-as-product-core.md) | Diagnostic IR as product core | Accepted | adapter / renderer / analysis を疎結合化する |
| [ADR-0003](./adr-0003-structured-first-gcc-ingress.md) | Structured-first GCC ingress | Accepted | GCC diagnostics の authoritative source を固定する |
| [ADR-0004](./adr-0004-gcc-15-first-support-policy.md) | GCC 15-first support policy | Accepted | 公式サポートの品質主張を明確化する |
| [ADR-0005](./adr-0005-gcc-13-14-compatibility-tier.md) | GCC 13–14 compatibility tier | Accepted | 互換 path と production claim を切り分ける |
| [ADR-0006](./adr-0006-fail-open-fallback-and-provenance.md) | Fail-open fallback and provenance | Accepted | wrapper failure が build failure を悪化させないようにする |
| [ADR-0007](./adr-0007-rust-as-implementation-language.md) | Rust as implementation language | Accepted | 長期品質と配布性を両立する |
| [ADR-0008](./adr-0008-linux-first-single-binary-musl-distribution.md) | Linux-first single-binary musl distribution | Accepted | install / rollback / support を安定化する |
| [ADR-0009](./adr-0009-library-plus-cli-layering.md) | Library + CLI layering | Accepted | 実装境界と再利用単位を固定する |
| [ADR-0010](./adr-0010-deterministic-rule-engine-no-ai-core.md) | Deterministic rule engine; no AI core dependency | Accepted | root-cause UX を testable に保つ |
| [ADR-0011](./adr-0011-locale-policy-english-first-reduced-fallback.md) | Locale policy: English-first, reduced fallback | Accepted | 表示文言と互換モードの挙動を安定化する |
| [ADR-0012](./adr-0012-native-ir-json-as-canonical-machine-output.md) | Native IR JSON as canonical machine-readable output | Accepted | 機械可読出力の正本を固定する |
| [ADR-0013](./adr-0013-sarif-egress-scope.md) | SARIF egress scope | Accepted | internal IR と export format の境界を明確化する |
| [ADR-0014](./adr-0014-linker-diagnostics-via-staged-text-adapter.md) | Linker diagnostics via staged text adapter | Accepted | 非構造 linker diagnostics を段階導入する |
| [ADR-0015](./adr-0015-source-ownership-model.md) | Source ownership model | Accepted | user / vendor / system / generated の扱いを固定する |
| [ADR-0016](./adr-0016-trace-bundle-content-and-redaction.md) | Trace bundle content and redaction | Accepted | supportability と機密性の境界を固定する |
| [ADR-0017](./adr-0017-dependency-allowlist-and-license-policy.md) | Dependency allowlist and license policy | Accepted | release artifact の品質と法務境界を固定する |
| [ADR-0018](./adr-0018-corpus-governance.md) | Corpus governance | Accepted | fixture 追加・sanitize・review の統制を固定する |
| [ADR-0019](./adr-0019-render-modes.md) | Render modes | Accepted | concise / default / verbose / raw の surface を固定する |
| [ADR-0020](./adr-0020-stability-promises.md) | Stability promises | Accepted | CLI / config / IR / renderer / release contract の change classification と governance freeze を固定する |
| [ADR-0021](./adr-0021-release-maturity-labels-and-artifact-semver-policy.md) | Release maturity labels and artifact semver policy | Accepted | `v1alpha` と `0.1.x` の混線を防ぎ、channel との境界を固定する |
| [ADR-0024](./adr-0024-public-beta-release-channel-and-github-release-policy.md) | Public beta release channel and GitHub Release policy | Accepted | public beta artifact の公開方法と promote story を固定する |
| [ADR-0025](./adr-0025-stable-release-automation-and-rollback-evidence.md) | Stable release automation and rollback evidence | Accepted | stable cut の same-bits promote と rollback drill 証跡を固定する |

## 3. 読み方

- **0001–0006** は導入形態、structured ingress、support tier、fallback の基礎判断
- **0007–0010** は実装言語、配布、実装境界、analysis 方針の骨格
- **0011–0016** は出力 surface、ownership、trace/redaction の製品境界
- **0017–0025** は dependency、corpus、render surface、stability、versioning semantics、public beta / stable release policy の運用統制

最初に読む順序は `0001 → 0003 → 0006 → 0002 → 0009 → 0004/0005 → 0019 → 0016 → 0018` を推奨する。

## 4. ステータス運用

ADR の状態語彙は次の 5 つに固定する。

- **Proposed**
- **Accepted**
- **Superseded**
- **Deprecated**
- **Rejected**

v1beta 現在では、この一覧にある ADR をすべて **Accepted** とする。今後の変更は既存仕様への自由追記ではなく、ADR の追加または supersede で扱う。

## 5. 実装への直結ポイント

この ADR セットにより、少なくとも次の着手点が曖昧でなくなる。

1. wrapper / subprocess / passthrough の境界
2. GCC 15 structured-first adapter の authoritative source
3. Diagnostic IR の ownership と machine-readable 正本
4. terminal / CI renderer の mode と locale policy
5. trace bundle / redaction / dependency / corpus governance
6. packaging / install / rollback / release engineering の原則

## 6. 更新原則

- 実質的な判断変更は新しい ADR を追加し、必要なら旧 ADR を `Superseded` にする
- 仕様書は Accepted baseline の実装契約として保ち、新しい判断は直接足さない
- 実装差分や運用補足は仕様書へ追記してよいが、判断変更は必ず ADR へ戻す
