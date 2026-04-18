---
doc_role: current-authority
lifecycle_status: draft
audience: both
use_for: Current product doctrine and architecture pivot toward essence extraction.
do_not_use_for: Historical rephrase-oriented presentation direction or superseded subject-blocks design.
supersedes: []
superseded_by: []
---
> [!IMPORTANT]
> Authority: `current-authority` / `draft`
> Use for: Current product doctrine and architecture pivot toward essence extraction.
> Do not use for: Historical rephrase-oriented presentation direction or superseded subject-blocks design.

# gcc-formed Essence-Extraction Doctrine — 起死回生設計書

- 文書種別: 製品ドクトリン / アーキテクチャ転回設計書
- 状態: Draft for approval
- 対象: `horiyamayoh/gcc-formed` (`main`, 2026-04-18 時点)
- 目的: 「GCC エラーの分かりやすい言い直し」志向から、「同根多発の束ね・標準ライブラリ越境の抑制 = **本質抽出**」志向へ製品命題を転回し、その実装姿勢・退化対象・強化対象・段階的ロールアウトを 1 冊で固定する
- 想定読者: maintainer / reviewer / coding agent / future contributor

---

## 0. この文書の位置づけ

本書は ADR ではなく、**architecture doctrine** である。
[`gcc-formed-vnext-change-design.md`](gcc-formed-vnext-change-design.md) と同階層の橋渡し文書として、上位の `current-authority` 群（`README.md`, `docs/README.md`, `docs/support/SUPPORT-BOUNDARY.md`, ADR-0001 / ADR-0002 / ADR-0006 / ADR-0026 / ADR-0027 / ADR-0029）を満たしつつ、下位の specs（特に `rendering-ux-contract-spec.md` の rephrase 系節）の責務範囲を再定義する。

本書自身では既存 ADR / spec の **実 supersede 宣言は行わない**。supersede は §11 の候補リストに従って Phase 1 で個別 ADR を起票して行う。

本書は **approval 直後に有効になる規範** と **phase 完了後に到達する target state** を同時に固定する。実装・文書同期・user-visible cutover は §12 の phase に従って進める。

本 change set では本書のみを是正対象とし、README / SUPPORT / specs の追随更新は別 change set に切り出す。ただし、本書承認後に下位文書を本書へ追随させることは必須であり、必要な追随先は §11.4 に列挙する。

本書承認前に Epic を増やしてはならない。最初にやるべきは ADR 起票（Phase 1）であり、その後に IR / cascade / render の段階的変更（Phase 2 以降）に着手する。

### 0.1 承認直後に有効になる規範 (Effective now on approval)

- rephrase は今後の主価値命題ではない。
- fail-open / raw fallback / passthrough は shipped contract の一部として維持する。
- public machine surface は additive-only compatibility を維持する。
- renderer の正規進行方向は `native + essence overlay` であり、新規設計判断はこの方向へ寄せる。

### 0.2 段階的ロールアウト後の到達状態 (Target state after phased rollout)

- `render.presentation = "essence_overlay"` を default presentation にする。
- `subject_blocks_v2`, `subject_blocks_v1`, `legacy_v1` は presentation として deprecated 化し、段階的に除去する。
- user-facing `headline` / `first_action` prose は cutover 後に default surface から除去する。
- Phase 5 までは current shipped default が残りうるが、Phase 4 以降の opt-in / cutover はすべて `essence_overlay` を正規名として行う。

---

## 1. 撤回宣言 (Withdrawal Notice)

本節は **approval 直後から有効な doctrinal withdrawal** を定める。ここで撤回するのは「何を主価値命題とするか」であり、実際の user-facing cutover と default 切替タイミングは §12 に従う。

`gcc-formed` は、これまで暗黙のうちに「GCC のエラーメッセージを人間にとって分かりやすく言い直す」ことを価値命題の一部として追ってきた。本書はその主命題化を撤回する。

- **撤回する命題**: 「コンパイラ出力の wording を、より自然・より親切・より educational に言い換える」こと。
- **撤回の根拠**:
  1. GCC 自身の出力は多くの場合すでに十分に正確で読みやすい。
  2. wrapper による rephrase は GCC 出力との二重表示・行数増加・false confidence を生み、ADR-0031 (native non-regression) が要求する「TTY default で native を下回らない」を default profile で破りやすい。
  3. ユーザが本当に困るのは「1 つのミスから派生した N 個のエラー」「標準ライブラリ越境後の読まなくてよいエラー」であり、これは wording の問題ではなく **量と境界の問題** である。
  4. テンプレート由来エラーはその一例にすぎず、テンプレート rephrase はゴールではない。

- **段階的に default / user-facing surface から撤回する範囲**:
  - `diag_enrich` が生成する `headline` / `first_action_hint` / family rephrase / display name の **ユーザ向け前面表示**。
  - `diag_render` の `subject_blocks_v2` / `subject_blocks_v1` / family-specific 段組み・slot catalog (`want` / `got` / `via` / ...)・`why:` / `because:` ライン。
  - `rendering-ux-contract-spec.md` §15（slot catalog）/ §16.5（first action 合成）/ §17（family-specific rendering）の rephrase 依存節を default profile から外す。

撤回しないもの（明示）:

- 機械タグ（`semantic_role` = root / follow_on / duplicate / uncertain、`family`、`confidence`）の **内部保持と公開 JSON への露出**。
- compiler が出力した正規の `note:` / `error:` / `warning:` 行の **そのままの保持と再生**。
- low-confidence 時の honesty disclosure と raw fallback ヒント。

---

## 2. 新製品命題 — Essence Extraction

> `gcc-formed` は GCC の出力を **書き換えない**。本質ではない出力を **出さない**。

### 2.1 3 軸の本質定義

本質抽出は次の 3 軸で構成される。これらは独立に評価・段階導入できる。

- **C-axis — Cascade compression**
  同根の派生エラーを 1 つの root にまとめ、follow-on を畳み込む。ユーザに「最初に直すべき 1 箇所」を出す。
- **B-axis — Boundary suppression**
  ユーザコードから標準ライブラリ・vendor header・generated code への越境点 (user frontier) を検出し、越境後に発生した派生診断を default で出さない。
- **D-axis — Duplicate dedup**
  同 symbol / 同 message / 同 frontier の繰り返しを「最初の 1 件 + 件数」に圧縮する。

### 2.2 anti-goals (やらないこと)

- GCC より「読みやすい」文を生成すること。
- color / box / decoration による視覚的「整え」を default で行うこと。
- LLM / AI による要約・rephrase・教育的補足。
- 独自プレゼンテーション grammar (`subject_blocks_v2` のような) の維持・拡張。
- family ごとの専用段組み（slot catalog 等）。

### 2.3 製品命題の一行要約

> **「1 つのミスを 1 行で見せ、派生と越境は黙らせ、必要なときだけ raw に逃がす。」**

---

## 3. First-class モデル — User Frontier & Blast Radius

essence extraction を deterministic に実装するため、IR に次の 2 概念を一級モデルとして加える。これらは既存の `Ownership`（User / Vendor / System / Generated / Tool / Unknown）と `*_frontier_key`（template / macro / include）の昇格である。

### 3.1 `UserFrontierCrossing`

ユーザコードから foreign（System / Vendor / Generated / Tool / Unknown）コードへ制御・展開・解決が渡った瞬間を表すイベント。

| フィールド | 型 | 必須 | 意味 |
|---|---|---:|---|
| `from_node_id` | string | MUST | user-side anchor（最後にユーザコードに居た frame） |
| `to_node_id` | string | MUST | foreign-side origin（越境先で最初に出た frame） |
| `crossing_kind` | enum-like string | MUST | 既定値は `include` \| `template_instantiation` \| `macro_expansion` \| `linker_resolution` \| `call`。doctrine では additive 拡張を許す |
| `user_side_anchor` | `Location` | MUST | ユーザに見せるべき位置（責任の所在） |
| `foreign_side_origin` | `Location` | MAY | 越境先位置（debug 表示用） |
| `foreign_owner` | enum | MUST | `system` \| `vendor` \| `generated` \| `tool` \| `unknown` |

### 3.1.1 default suppression の安全規則

- default で suppression 候補にしてよいのは `system`, `vendor`, `generated` owner を持つ foreign-side の follow-on / duplicate のみ。
- `tool` と `unknown` は owner だけを根拠に default suppression しない。
- frontier crossing が確信できない run / band / path では suppression しない。特に `GCC9-12` / `NativeTextCapture` で根拠不足なら visible を優先する。
- user-side root は常に visible でなければならない。
- mixed chain で user-owned node に戻った場合、その node は visible に戻さなければならない。

### 3.2 `BlastRadius`

ある root 診断から派生した「同根群」の量を要約する。本 doctrine における `BlastRadius` は **public count contract** であり、root selection や scoring の exact tuning 自体は内部 rulepack 実装事項である。

| フィールド | 型 | 必須 | 意味 |
|---|---|---:|---|
| `root_node_id` | string | MUST | 起点 root |
| `follow_on_count` | integer | MUST | 同 episode 内 follow-on 数 |
| `duplicate_count` | integer | MUST | 同 root 由来 duplicate 数 |
| `frontier_crossing_count` | integer | MUST | この root を起点に発生した user-frontier 越境数 |
| `suppressed_foreign_node_count` | integer | MUST | 越境後に default で抑制された node 数 |

### 3.3 公開契約への露出

- 公開 JSON schema を `2.0.0-alpha.1` → `2.0.0-alpha.2` へ **additive only** で進める。既存フィールド削除・改名・型変更は禁止。
- 本書が追加する field はすべて **additive optional** とする。対象は `status = available` の export のみ。
- `result.summary.frontier_crossing_count?: integer`
  - boundary analysis を実行した export でのみ出す。
  - analysis 実行済みで 0 件なら `0` を出す。
  - analysis 未実行なら欠落させる。
- `result.summary.suppressed_foreign_node_count?: integer`
  - boundary analysis を実行した export でのみ出す。
  - analysis 実行済みで 0 件なら `0` を出す。
  - analysis 未実行なら欠落させる。
- `result.diagnostics[*].blast_radius?: object`
  - `semantic_role = root` の diagnostic にのみ出す。
  - `follow_on` / `duplicate` / `uncertain` には出さない。
  - block 内の count はすべて非負整数とする。
  - count 未計算なら block ごと欠落させる。
- SARIF property bag への露出は本 doctrine の範囲外とし、別 change で扱う。
- terminal text は本書 §5 のとおり overlay 経由でこれらを表示するが、terminal は public contract ではない。

---

## 4. 既存資産との接続

essence extraction は **新規の半分以上が既に IR / cascade に実装済み** である。本書はそれを表に出す責務再配置である。

- `diag_cascade::LogicalGroup` / `EpisodeGraph` / `GroupCascadeRole` (LeadRoot / IndependentRoot / FollowOn / Duplicate / Uncertain) / `VisibilityFloor` (NeverHidden / SummaryOrExpandedOnly / HiddenAllowed) / `CascadePolicySnapshot` → C-axis と D-axis の実体は既に存在する。
- `diag_core::OwnershipInfo` と `diag_cascade` の `frontier_key`（template / macro / include） → B-axis の素材は既に存在する。本書 §3 はこの素材に **名前と公開可能な型** を与える。
- `diag_rulepack::cascade::CascadeRulepack` → §8 で boundary policy を additive に拡張する。
- `diag_public_export` → §3.3 の additive のみ。
- `diag_capture_runtime` / `diag_adapter_gcc` ingest / `diag_residual_text` / `diag_trace` / fail-open / SARIF egress / CaptureBundle (ADR-0028) → **不変**。

---

## 5. 目標レンダラ姿勢 — Native + Essence Overlay

### 5.1 Phase 5 cutover 後の default presentation の姿勢

Phase 5 cutover 後、`render.profile = default` の既定 presentation は `essence_overlay` とする。そこでの terminal 出力は次のとおりとする。

1. **GCC native の `error:` / `warning:` / `note:` 行をそのまま出す** （wording は一切書き換えない）。
2. その上下に wrapper が **構造オーバーレイ** だけを付ける。

### 5.2 オーバーレイで出してよい要素（許可リスト, exhaustive）

- root 行マーカー（1 行、native 文の直前）

  ```text
  [root 1/3] src/main.cpp:42
  src/main.cpp:42:18: error: invalid conversion from 'const char*' to 'short int'
  ```
- follow-on / duplicate 圧縮行（同 root 直後、1 行）

  ```text
  [+5 follow-ons collapsed, +2 duplicates] (rerun with --formed-show-collapsed to expand)
  ```
- 越境抑制サマリ（episode 末尾、1 行）

  ```text
  [suppressed 12 lines past <bits/stl_vector.h> (system header) — see --formed-show-foreign]
  ```
- low-confidence honesty line（既存仕様を維持）。
- partial / fallback notice（既存仕様を維持）。
- raw fallback hint（既存仕様を維持）。

### 5.3 オーバーレイで出してはならない要素（禁止リスト, exhaustive）

- `headline:` 行（rephrase）
- `first_action:` 行（rephrase）
- `why:` / `because:` 行
- slot catalog (`want` / `got` / `via` / `name` / `use` / ...)
- family-specific 段組み・小見出し
- color による「意味付け」（color は構造アクセントに限る）
- ascii box / 装飾罫線

### 5.4 control plane 整理

- `render.profile` と `render.presentation` は分離して扱う。
- `render.profile` は本 doctrine 改訂では変更しない。canonical set は `default | concise | verbose | ci | debug | raw_fallback` である。
- `render.presentation` は layout grammar / overlay grammar を担う。将来追加する canonical 名は `essence_overlay` とする。
- `subject_blocks_v2`, `subject_blocks_v1`, `legacy_v1` は **profile ではなく presentation 名** として扱い、Phase 5–6 で deprecated / removed とする。
- canonical CLI / config surface は `render.presentation = "essence_overlay"` または `--formed-presentation=essence_overlay`、および `--formed-show-collapsed`, `--formed-show-foreign` とする。
- 既存 `--formed-public-json` と `--formed-profile` は不変とする。
- `--formed-render-preset` は canonical surface として採用しない。
- `--formed-essence=off|overlay|aggressive` のような別軸 CLI は導入しない。

### 5.5 不変条件

- §5.2 の許可リストに無い行を default で出してはならない。
- native 行を改変・並び替え・行分割してはならない（オーバーレイ挿入のみ可）。
- 越境抑制で hidden 化した行は、`--formed-show-foreign` で必ず復元できなければならない。

---

## 6. 責務再定義マトリクス

各 crate を 4 軸（C / B / D / rephrase）で再評価し、維持・拡張・縮退・全廃を固定する。

| Crate | C-axis | B-axis | D-axis | rephrase | 結論 |
|---|---|---|---|---|---|
| `diag_capture_runtime` | — | — | — | — | 不変 |
| `diag_adapter_gcc` | 維持 | **拡張**（frontier crossing event の抽出） | 維持 | — | ingest 強化 |
| `diag_adapter_contract` | 維持 | 拡張（新 IR 型追加に追従） | 維持 | — | 契約追従のみ |
| `diag_core` | 維持 | **拡張**（`UserFrontierCrossing` / `BlastRadius` の新規型追加） | 維持 | — | 拡張のみ |
| `diag_rulepack` | 維持 | **拡張**（boundary policy の新規追加） | 維持 | — | policy 追加 |
| `diag_cascade` | **強化**（boundary 認識） | **強化**（foreign-side suppression rule） | 維持 | — | rule 拡張 |
| `diag_residual_text` | — | — | — | — | 不変 |
| `diag_enrich` | — | — | — | **user-facing rephrase を全廃**（headline / first_action_hint / family rephrase） | 縮退（machine-tag extraction のみ） |
| `diag_render` | label 出力のみ | overlay 出力 | overlay 出力 | **user-facing rephrase を全廃** | 大幅縮退 |
| `diag_cli_front` | 既存 | 既存 + presentation surface 整理 | 既存 | — | surface 整理 |
| `diag_public_export` | 既存 | **拡張**（§3.3 の additive） | 既存 | — | additive |
| `diag_trace` | — | — | — | — | 不変 |
| `diag_backend_probe` | — | — | — | — | 不変 |

---

## 7. 退化対象 (Deprecation List, normative)

以下は本書承認をもって **deprecation policy として確定** し、Phase 5–6 の user-facing cutover で段階的に除去する。

1. `diag_enrich` の **rephrase 系出力**:
   - `AnalysisOverlay.headline` のユーザ向け文生成
   - `AnalysisOverlay.first_action_hint` のユーザ向け文生成
   - family 表示名・display label の rewrite
   - IR 上のフィールド自体は当面残し（公開 JSON 後方互換のため）、`diag_enrich` は machine-tag extraction 層として残す。生成器側では user-facing prose を empty / passthrough にする。
2. `diag_render` の **presentation v2 系プリセット**:
   - `subject_blocks_v2`
   - `subject_blocks_v1`
   - `legacy_v1`
   - family-specific renderer (`docs/specs/rendering-ux-contract-spec.md` §17 全体)
3. `rendering-ux-contract-spec.md` の **rephrase 依存節**:
   - §15 (slot catalog)
   - §16.5 (first action 合成)
   - §17 (family-specific rendering)
   - これらは `essence_overlay` 仕様で置換し、Phase 6 で history-only に降格。
4. **ADR の supersede 候補**（実 supersede は Phase 1 で個別 ADR 起票）:
   - ADR-0030 (theme/layout 分離) — theme 機構自体不要。
   - ADR-0031 (TTY native 非劣化) — 「non-regression」から「native passthrough by default」へ強化版に書き直し。
   - ADR-0034 (presentation-v2 subject-first-blocks) — 全面 supersede。
5. corpus の `subject_blocks_v2` 系 snapshot — Phase 5 までは A/B 比較資産として残し、Phase 6 で archive 送り。

---

## 8. 強化対象 (Reinforcement List, normative)

1. `diag_rulepack::cascade` に **boundary policy** を追加:
   - default で suppression 候補にしてよいのは foreign-side の `Ownership::System` | `Ownership::Vendor` | `Ownership::Generated` に属する follow-on / duplicate のみ。
   - `Ownership::Tool` と `Ownership::Unknown` は owner だけを根拠に default suppression しない。
   - user-side root は `VisibilityFloor::NeverHidden`。
   - mixed chain で user-owned node に戻ったら、その node は visible に戻す。
   - foreign owner と path の組み合わせごとに `extra_evidence_points` を加減できる additive 構造。
2. cascade rulepack は frontier crossing を follow-on evidence として利用してよい:
   - user→foreign 越境が確定した時点以降の同 episode 内 node は、強い follow-on 候補として扱ってよい。
   - frontier crossing の exact weight / duplicate 判定閾値 / tuning は内部 rulepack 実装事項であり、本 doctrine の public contract ではない。
3. **公開 JSON schema 2.0.0-alpha.2**（additive only, §3.3）:
   - `result.summary.frontier_crossing_count?`
   - `result.summary.suppressed_foreign_node_count?`
   - `result.diagnostics[*].blast_radius?` (`semantic_role = root` のみ)
4. **CLI / config surface**（`diag_cli_front`）:
   - `render.presentation = "essence_overlay"` / `--formed-presentation=essence_overlay`
   - `--formed-show-collapsed`
   - `--formed-show-foreign`
   - 既存 `--formed-public-json` と `--formed-profile` は不変。
   - `--formed-render-preset` と `--formed-essence=...` は canonical surface として採用しない。
5. **corpus**:
   - anti-collision に「越境抑制で実 root が消えていないこと」を確認する fixture を追加。
   - 同根多発 fixture（template / macro / linker undefined）に boundary suppression の期待値を追記。
6. **quality gate KPI ローテ**（`docs/specs/quality-corpus-test-gate-spec.md` 改訂で扱う）:
   - **Fidelity P0**:
     - 測定対象: `default` / `ci` で visible な native `error:` / `warning:` / `note:` 行の wording 改変有無。
     - Gate: designated fixture 全件で 100% 非改変。
   - **Anti-collision P0**:
     - 測定対象: false-hidden suppression count / real-root count。
     - Gate: designated anti-collision fixture で false-hidden suppression 0 件、pass rate 100%。
   - **Noise-Before-Action**:
     - 測定対象: `first visible actionable/root line` までに読まされる wrapper-added line 数。
     - Gate: representative corpus で median / p90 を前 phase から悪化させない。
   - **Suppression-Precision**:
     - 測定対象: suppressed node のうち、owner が `system|vendor|generated` で user-owned actionable evidence を持たない node 数 / suppressed node 総数。
     - Gate: designated suppression fixture では 100%、broad corpus では trend metric として観測。
   - **Foreign-Leak-Rate**:
     - 測定対象: suppression 対象だった foreign-side follow-on / duplicate のうち default 出力に残った node 数 / suppression 対象 node 総数。
     - Gate: designated suppression fixture では 0%、broad corpus では trend metric として観測。

---

## 9. 既存資産の活かし方マップ (Asset Reuse Map)

### 9.1 100% 維持（破壊的変更禁止）

- `diag_capture_runtime`
- `diag_adapter_gcc` の ingest 経路（SARIF / GCC JSON / stderr 拾い）
- `diag_core` の既存型（`DiagnosticDocument`, `DiagnosticNode`, `Location`, `Ownership`, `ContextChain`, `SymbolContext`, `Provenance`, `AnalysisOverlay` のフィールド構造）
- `diag_public_export` の既存フィールド
- `diag_residual_text`
- `diag_trace`
- fail-open / raw fallback 経路
- SARIF egress (ADR-0013)
- CaptureBundle 契約 (ADR-0028)
- 公開 JSON schema 2.0.0-alpha.1 の既存 surface

### 9.2 拡張（additive only）

- `diag_core`: §3 の新型追加
- `diag_adapter_gcc`: frontier crossing event の抽出
- `diag_rulepack`: boundary policy 追加
- `diag_cascade`: boundary scoring 追加
- `diag_public_export`: §3.3 の additive
- `diag_cli_front`: §8.4 の flag 追加

### 9.3 縮退・全廃

- `diag_enrich` の rephrase ロジック
- `diag_render` の presentation v2 系プリセット
- `rendering-ux-contract-spec.md` §15 / §16.5 / §17

---

## 10. 危険ゾーン (Stop-ship Invariants)

以下は本書転回の **どの phase でも** 破ってはならない。

1. **fail-open / raw fallback / passthrough** 経路を劣化させない。essence overlay が失敗しても native は必ず出る。
2. **公開 JSON schema は additive only**。既存フィールドの削除・改名・型変更・意味変更を禁止。
3. **structured source authority**（SARIF / GCC JSON）を first-class のまま保つ（ADR-0028 不変）。
4. **CaptureBundle ingest 契約** (ADR-0028) を不変に保つ。
5. **anti-collision**: 「越境抑制」「同根畳み込み」が独立な real root を hidden 化してしまう false hidden suppression を出さない。corpus でガードし、Phase 3 以降の各リリースで stop-ship gate にする。
6. **native non-regression**: ADR-0031 の精神を強化する方向のみ可（弱める変更は禁止）。
7. **machine surface 不変**: terminal default の縮退と引き換えに、機械可読 surface（公開 JSON）の情報量は同等以上を保つ。

---

## 11. 既存 ADR / spec の supersede マッピング

本書では **候補列挙のみ** とし、実 supersede は Phase 1 の個別 ADR 起票で行う。

### 11.1 supersede 候補 ADR

| ADR | 状態 | 提案 |
|---|---|---|
| ADR-0030 (theme/layout 分離) | 全面 supersede 候補 | theme 機構不要 |
| ADR-0031 (TTY native 非劣化) | 強化版で置換候補 | 「non-regression」→「native passthrough by default」 |
| ADR-0034 (presentation-v2 subject-first-blocks) | 全面 supersede 候補 | essence overlay で置換 |

### 11.2 部分改訂候補 spec

| spec | 改訂対象 | 結末 |
|---|---|---|
| `rendering-ux-contract-spec.md` | §15 / §16.5 / §17 | 部分 history-only 降格、`essence_overlay` 仕様で置換 |
| `public-machine-readable-diagnostic-surface-spec.md` | schema additive 追記 | 2.0.0-alpha.2 |
| `gcc-adapter-ingestion-spec.md` | frontier crossing event 抽出仕様の追加 | additive |
| `quality-corpus-test-gate-spec.md` | KPI 追加（§8.6） | additive |
| `diagnostic-ir-v1alpha-spec.md` | §3 の新型追加 | additive |

### 11.3 維持土台 ADR（不変）

- ADR-0001 (wrapper-first entrypoint)
- ADR-0002 (diagnostic IR as product core)
- ADR-0006 (fail-open fallback and provenance)
- ADR-0019 (render modes) — preset 整理は §5.4 / §7
- ADR-0026 (CapabilityProfile replaces SupportTier)
- ADR-0027 (ProcessingPath separate from SupportLevel)
- ADR-0028 (CaptureBundle-only ingest)
- ADR-0029 (Path B/C are first-class)

### 11.4 本書承認後に追随更新が必要な下位文書

- `README.md`
- `docs/support/SUPPORT-BOUNDARY.md`
- `docs/specs/rendering-ux-contract-spec.md`
- `docs/specs/public-machine-readable-diagnostic-surface-spec.md`
- `docs/specs/quality-corpus-test-gate-spec.md`
- `docs/specs/diagnostic-ir-v1alpha-spec.md`
- `docs/specs/gcc-adapter-ingestion-spec.md`

本 change set ではこれらを更新しない。ただし、Phase 2 / 4 / 5 の cutover を実行する change set では、本書に対する追随更新を同時に含めなければならない。

---

## 12. 段階的ロールアウト (Phased Rollout, informative)

各 phase は corpus snapshot + KPI で独立に検証可能で、stop-ship gate を持つ。

Phase 4 以降の user-visible cutover は、§11.4 の下位文書が同じ change set で本書へ追随更新されている場合にのみ行ってよい。

| Phase | 内容 | コード変更 | 公開契約 |
|---|---|---|---|
| **0** | 本書承認 | なし | なし |
| **1** | ADR 起票 (§11.1 の supersede 提案 + 新 ADR「essence overlay default」) | なし | なし |
| **2** | IR 拡張 (`UserFrontierCrossing` / `BlastRadius`) と adapter 抽出実装、公開 schema 2.0.0-alpha.2 additive | `diag_core` / `diag_adapter_gcc` / `diag_public_export` | additive |
| **3** | cascade rulepack に boundary policy 追加、anti-collision corpus 拡充 | `diag_rulepack` / `diag_cascade` / `corpus` | なし |
| **4** | `render.presentation = "essence_overlay"` / `--formed-presentation=essence_overlay` を opt-in で導入、A/B 比較 | `diag_render` (新 overlay presentation 追加) / `diag_cli_front` | なし |
| **5** | default presentation を `essence_overlay` に切替、`subject_blocks_v2` を deprecated 表示 | `diag_render` (default presentation switch) / docs | なし |
| **6** | `diag_enrich` の rephrase 削除、render の deprecated preset 削除、`rendering-ux-contract-spec.md` §15/§16.5/§17 を history-only 降格 | `diag_enrich` / `diag_render` / docs | なし（terminal text のみ縮退） |

各 phase 終了時に:

- `Fidelity P0` が designated fixture で 100% pass している
- `Anti-collision P0` が designated anti-collision fixture で 100% pass している
- designated suppression fixture が `Suppression-Precision = 100%` と `Foreign-Leak-Rate = 0%` を満たしている
- representative corpus で `Noise-Before-Action` の median / p90 が前 phase より悪化していない
- 公開 JSON のスキーマ後方互換が保たれている

を確認する。

---

## 13. Deferred Follow-ups

以下は本 doctrine の blocking issue ではなく、後続 change set / ADR / spec で詰める項目である。本書の規範判断自体は、これらを待たずに確定してよい。

1. SARIF egress (ADR-0013) に `blast_radius` を載せるか否か。載せる場合の SARIF property bag 設計。
2. frontier crossing bonus の exact tuning と duplicate 判定閾値の最適化。
3. GCC9-12 `native_text_capture` における boundary extraction evidence の corpus 拡充。
4. doctrine 承認後に v1beta → v2alpha maturity bump をどのタイミングで宣言するか。

---

## 14. 受入基準 (Acceptance for this Doctrine)

本書はそれ自身が変更を行わないため、受入は **読みやすさ / 一意性 / 整合性** で判定する。

1. §0.1 と §0.2 により、「approval 直後に何が有効になるか」と「Phase 5 以降に何が切り替わるか」を曖昧さなく答えられること。
2. §5.4 により `render.profile` と `render.presentation` の責務が分離され、canonical CLI / config 名を 1 通りに特定できること。
3. §7（退化対象）と §8（強化対象）が排他的かつ normative で、新規エージェント / 開発者が「何を作り、何を作らないか」を本書だけから一意に決定できること。
4. §3.3 / §10.2 が公開 JSON schema 2.0.0-alpha.1 の既存約束を破壊せず、欠落と `0` の意味を区別できること。
5. §3.1.1 / §8.1 が `tool` / `unknown` ownership と mixed user/foreign chain の default visible ルールを明示していること。
6. §9 / §10 の不変条件が、現行 `current-authority` ADR / spec のいずれとも矛盾しないこと（特に ADR-0001/0002/0006/0026/0027/0028/0029）。
7. ADR-0031 (native non-regression) を §5 / §10.6 が **強化** する方向であり、弱めていないこと。
8. §11.4 が README / SUPPORT / rendering spec / public JSON spec / quality gate spec などの追随更新先を明示していること。
9. §12 の各 phase が corpus snapshot + KPI で独立に stop-ship 可能であること。
