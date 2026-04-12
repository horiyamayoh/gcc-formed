# gcc-formed カスケード圧縮 最終アーキテクチャ / Work Package 実行計画
最終ブラッシュアップ版

作成日: 2026-04-12  
対象リポジトリ: `horiyamayoh/gcc-formed` `main`  
前提: current-authority 文書、現行 workspace、現行 issue template、現行 `execute -> enrich -> render` 実装を踏まえた最終設計

---

## 0. この版で締めたこと

この版では、前版の曖昧さや残っていた段差を明示的に潰した。特に次を修正した。

1. **E1 / E2 / E3 と WP の対応を厳密化した。**  
   前版では、`WP-006` と `WP-007` が E1 の成立に実質必要なのに W2 側へ寄っていた。  
   本版では **E1 完了条件を WP-007 完了時点** に固定する。

2. **IR 変更の扱いを明示した。**  
   `EpisodeGraph` は最終的に `DiagnosticDocument` の first-class field とする。  
   逃げで `extensions` に押し込める設計は採らない。代わりに **`diagnostic-ir-v1alpha-spec` の additive update** を同時に行う。

3. **カスケード圧縮レベルの意味を固定した。**  
   `off / conservative / balanced / aggressive` の挙動差を言葉だけでなく **表示決定規則** として定義した。

4. **GitHub issue 化しやすい WP に磨き直した。**  
   既存 `.github/ISSUE_TEMPLATE/work_package.yml` の欄にそのまま落とせるように、各 WP を  
   `Goal / Why now / Allowed files / Forbidden surfaces / Acceptance criteria / Commands / Docs impact / Stop conditions / Reviewer evidence`  
   まで記述した。

5. **GCC9–15 の扱いをさらに明確にした。**  
   同一の成功定義・同一の UX 契約・同一の stop-ship 思想を採る。  
   path-aware なのは **証拠の冗長性** と **uncertain への倒し方** だけであり、目標そのものは下げない。

---

## 1. current-authority を踏まえた固定前提

本設計は、現行 repo の doctrine と矛盾しないように次を固定前提とする。

### 1.1 プロダクト前提

- `gcc-formed` の目標は「GCC 生出力の prettier 化」ではなく、**GCC 9〜15 をまたぐ複数 path で 1 つの UX 原則を返すこと**。
- `GCC15+`, `GCC13-14`, `GCC9-12` はすべて **in-scope product bands** である。
- `GCC13-14` と `GCC9-12` は「あとで足す fallback」ではなく、**first-class product paths** として扱う。
- shipped contract として、**default TTY で native GCC 非劣化** が要求される。

### 1.2 実装前提

- 現行パイプラインには `enrich_document(&mut document, &cwd)` の直後、`render(RenderRequest { ... })` の直前に document-wide analysis を差し込める自然な継ぎ目がある。
- 現行 renderer には、expanded card と `summary_only_cards` を分ける選択機構がある。  
  したがって「独立 error を 1 回の run で複数見せる」方向は既存設計と衝突しない。
- `AnalysisOverlay` には `root_cause_score`, `suppression_reason`, `collapsed_child_ids`, `collapsed_chain_ids`, `group_ref` などが既にある。
- CLI / admin config / user config の合成と、`--formed-*` 形式の option parse は既にある。
- workspace には `diag_core`, `diag_enrich`, `diag_render`, `diag_rulepack`, `diag_testkit`, `diag_trace`, `xtask` があり、`diag_cascade` だけを新設すれば層構造として収まりがよい。

---

## 2. この機能の最終的な製品価値

この機能の本質は「GCC の各エラーをもっと上手に言い換えること」ではない。  
本質は次の変換である。

> **N 個の compiler diagnostics を、独立 root episode と従属 follow-on / duplicate / context flood に分解し、default では独立 root を優先表示し、従属群は圧縮または非表示にする。**

つまり価値の中心は **episode-first compression** である。

### 2.1 目標とする user experience

1. **独立な root error は 1 回の compile で全部見える。**
2. **強い follow-on / duplicate / template flood は既定で減る。**
3. **raw compiler facts と provenance は失わない。**
4. **default TTY の読み始めコストは native GCC 以下、または少なくとも十分に正当化される。**
5. **C だけでなく C++ template / macro / include flood でも最初に直すべき user-owned site へ視線誘導できる。**

### 2.2 この機能が失敗している状態

次のどれかに当てはまるなら失敗である。

- 1 個の parse error を隠せても、別の独立 error まで見えなくなる
- suppression の説明文が長すぎて native GCC と同じくらい読む羽目になる
- GCC15 では動くが GCC9–12 では「実質対象外」になる
- template 系で `std::` / candidate note / include flood を圧縮できない
- raw facts を捨てるか、debug でも suppression の根拠が追えない
- renderer 側に relation heuristic が漏れ、保守不能になる

---

## 3. 最終アーキテクチャ決定事項

ここは「案」ではなく **固定した設計判断** を並べる。

### AD-1: 成功定義は GCC9–15 で共通

- 共通 success definition:
  - independent root recall を守る
  - uncertain を hidden にしない
  - strong follow-on / duplicate を圧縮する
  - raw facts/provenance を残す
  - default TTY non-regression を守る
- band 差として許容するのは
  - hidden に入るために必要な証拠量
  - structured/native-text の取りやすさ
  - debug で追える補助情報の濃さ
  のみ。

### AD-2: document-wide analysis は first-class に持つ

`DiagnosticDocument` に optional な `document_analysis` を追加する。  
`EpisodeGraph` を `extensions` の無名 payload に押し込める設計は採らない。

理由:

- renderer / trace / tests / replay gate から typed に読める必要がある
- `extensions` に押し込めると contract と実装が乖離しやすい
- カスケード抑制は製品価値の核であり、補助 metadata ではない

### AD-3: 新規 crate `diag_cascade` を設ける

責務は以下に限定する。

- top-level groups 間の相互関係推定
- episode 形成
- suppression / summary 候補決定に必要な score と evidence 生成
- document-wide analysis の materialize

renderer や adapter に heuristic を埋め込まない。

### AD-4: renderer は episode-first にする

selector の主語を `DiagnosticNode` / group から `DiagnosticEpisode` へ引き上げる。  
renderer は relation を再発明しない。  
renderer が見るのは次だけである。

- `DocumentAnalysis`
- `GroupCascadeAnalysis`
- render profile
- cascade policy snapshot

### AD-5: default は aggressive だが、uncertain を hidden にしない

既定は攻めた suppression でよい。  
ただし aggressive とは

- weak evidence 1 個で消す
- band 差を理由に適当に隠す
- explanation を長々と付けて正当化する

ことを意味しない。

**default aggressive の意味は:**
- duplicate は強く圧縮
- strong follow-on は hidden 可
- uncertain は summary か visible のまま
- suppressed reason は 1 行で済ませる

### AD-6: deep template / macro / include は別 wave だが最終像に含める

C++ template を「対象外」にはしない。  
最終アーキテクチャの中に

- template frontier
- macro frontier
- include frontier
- candidate-note cluster

を入れる。  
ただし実装波としては W2 に切る。

### AD-7: fail-open を崩さない

`diag_cascade` が panic / error / unsupported state に入った場合は

- render を止めない
- build を壊さない
- raw facts を捨てない

の 3 つを守る。  
episode analysis が無いとき renderer は現行挙動へ戻る。

### AD-8: stop-ship は quality gate に置く

この機能は heuristic 的だが、品質判断を印象論にしない。  
最低でも次は stop-ship にする。

- anti-collision corpus で false hidden suppression > 0
- independent root recall < 100%
- default TTY inflation が正当化できない
- raw/provenance が追えない
- GCC9–12 を issue taxonomy / corpus / gate から外す change

---

## 4. 最終状態の crate 責務

| crate | 最終責務 |
|---|---|
| `diag_adapter_gcc` | compiler/linker facts の capture / ingest。相互依存判定はしない |
| `diag_enrich` | node-local family / headline / first-action / ownership の付与 |
| `diag_cascade` | cross-group relation, episode graph, suppression likelihood, evidence tags |
| `diag_rulepack` | cascade rules / weights / thresholds / path-aware redundancy policy の authority |
| `diag_render` | episode-first selection と presentation。relation 推論はしない |
| `diag_cli_front` | CLI/config merge、resolved policy、pipeline hook、fail-open 接続 |
| `diag_testkit` | corpus schema / fixtures / expectation helpers |
| `diag_trace` | debug/explainability trace、suppressed episode の追跡面 |
| `xtask` | replay / snapshot / gate orchestration |

---

## 5. 最終パイプライン

```text
capture
  -> ingest
  -> enrich (node-local)
  -> cascade analyze (document-wide)   <-- new
  -> render (episode-first)
  -> trace / debug disclosure
```

### 5.1 具体的な差し込み位置

`diag_cli_front/src/execute.rs` の現在の流れを踏まえると、差し込み位置は次で固定する。

```rust
let mut document = ingest_report.document;
document.captures = capture.capture_artifacts();

enrich_document(&mut document, &cwd);

// ここに追加
let cascade_report = analyze_document(
    &mut document,
    &cascade_context,
    &cascade_policy,
).unwrap_or_else(fail_open_to_none);

let render_result = render(RenderRequest {
    document: document.clone(),
    profile: plan.profile,
    // existing fields...
    cascade_policy,
});
```

### 5.2 fail-open の扱い

- `diag_cascade` が失敗しても `render()` は継続する
- `document.document_analysis == None` なら renderer は現行 selection に戻る
- trace には `cascade_unavailable_reason` を残す
- CLI exit code はコンパイラ由来のまま

---

## 6. IR と API の最終形

### 6.1 IR 変更方針

`diagnostic-ir-v1alpha-spec` は additive に更新する。  
ここでは便宜的に `1.0.0-alpha.2` 相当を想定するが、版番号の最終表記は repo 側に合わせればよい。  
重要なのは **optional field の追加で済ませる** ことである。

### 6.2 型の最終形

```rust
pub struct DiagnosticDocument {
    // existing fields...
    pub diagnostics: Vec<DiagnosticNode>,
    pub document_analysis: Option<DocumentAnalysis>,
}

pub struct DocumentAnalysis {
    pub policy_profile: Option<String>,
    pub producer_version: Option<String>,
    pub episode_graph: EpisodeGraph,
    pub group_analysis: Vec<GroupCascadeAnalysis>,
    pub stats: CascadeStats,
}

pub struct EpisodeGraph {
    pub episodes: Vec<DiagnosticEpisode>,
    pub relations: Vec<EpisodeRelation>,
}

pub struct DiagnosticEpisode {
    pub episode_ref: String,
    pub lead_group_ref: String,
    pub member_group_refs: Vec<String>,
    pub family: Option<String>,
    pub lead_root_score: Option<Score>,
    pub confidence: Option<Score>,
}

pub struct GroupCascadeAnalysis {
    pub group_ref: String,
    pub episode_ref: Option<String>,
    pub role: GroupCascadeRole,
    pub best_parent_group_ref: Option<String>,
    pub root_score: Option<Score>,
    pub independence_score: Option<Score>,
    pub suppress_likelihood: Option<Score>,
    pub summary_likelihood: Option<Score>,
    pub visibility_floor: VisibilityFloor,
    pub evidence_tags: Vec<String>,
}

pub enum GroupCascadeRole {
    LeadRoot,
    IndependentRoot,
    FollowOn,
    Duplicate,
    Uncertain,
}

pub enum VisibilityFloor {
    NeverHidden,
    SummaryOrExpandedOnly,
    HiddenAllowed,
}

pub struct EpisodeRelation {
    pub from_group_ref: String,
    pub to_group_ref: String,
    pub kind: EpisodeRelationKind,
    pub confidence: Score,
    pub evidence_tags: Vec<String>,
}

pub enum EpisodeRelationKind {
    Cascade,
    Duplicate,
    Context,
}

pub struct CascadeStats {
    pub independent_root_count: u32,
    pub dependent_follow_on_count: u32,
    pub duplicate_count: u32,
    pub uncertain_count: u32,
}
```

### 6.3 なぜ `group_analysis` を別に持つか

前版では `EpisodeGraph` だけでも足りる形にしていたが、最終的には `group_analysis` を別に持つほうがよい。

理由:

- renderer が「episode は分かるが各 group の role が分からない」状態を避けられる
- `NeverHidden` などの safety floor を typed に持てる
- debug で `why suppressed?` を出しやすい
- quality gate で `false hidden suppression = 0` を機械的に検査しやすい

### 6.4 analyzer / renderer API

```rust
pub struct CascadeContext {
    pub version_band: VersionBand,
    pub processing_path: ProcessingPath,
    pub source_authority: SourceAuthority,
    pub fallback_grade: FallbackGrade,
    pub cwd: PathBuf,
}

pub struct CascadePolicySnapshot {
    pub compression_level: CompressionLevel,
    pub suppress_likelihood_threshold: f32,
    pub summary_likelihood_threshold: f32,
    pub min_parent_margin: f32,
    pub max_expanded_independent_roots: usize,
    pub show_suppressed_count: bool,
}

pub fn analyze_document(
    document: &mut DiagnosticDocument,
    ctx: &CascadeContext,
    policy: &CascadePolicySnapshot,
) -> Result<CascadeReport, CascadeError>;
```

`RenderRequest` には `cascade_policy: CascadePolicySnapshot` を追加する。  
これにより **graph 生成** と **表示決定** を分離できる。

---

## 7. 分析アルゴリズムの最終設計

### 7.1 論理単位: LogicalGroup

内部実装では、まず top-level diagnostic をそのまま pairwise 比較しない。  
最初に `LogicalGroup` を作る。

### ルール

1. top-level `DiagnosticNode` 1 個を 1 group の基本単位とする
2. `analysis.group_ref` は hint としてのみ使う
3. child notes / context chains / candidate notes は group 内部情報として残す
4. group 間の関係だけを `EpisodeRelation` に持つ

### 意図

- 既存 IR / renderer の思想を壊さない
- group 内構造と group 間構造を分離する
- deep template/candidate-note 圧縮に耐える

### 7.2 anchor / key 抽出

各 group から次を導出する。

- `primary_file_key`
- `primary_line_bucket`
- `translation_unit_key`
- `origin_phase_key`
- `symbol_key`
- `family_key`
- `ownership_key`
- `normalized_message_key`
- `template_frontier_key`
- `macro_frontier_key`
- `include_frontier_key`
- `ordinal_in_invocation`

これを candidate pre-filter に使う。  
全 group 総当たりの O(N^2) は避ける。

### 7.3 root score

各 group に `root_score` を付ける。  
これは「この group 自体が episode の根らしいか」を測る値。

### 強い正の証拠

- syntax / parse / delimiter / desync 系 family
- user-owned primary location
- first action が短く出せる
- invocation 中で早い位置
- linker では generic `collect2` より具体的 unresolved symbol / multiple definition
- template では最初の user-owned frontier

### 強い負の証拠

- generic follow-on wording
- candidate-note repeat
- system/vendor 深部のみ
- すでに近傍により強い root 候補がある
- compiler-owned context だけで user-owned anchor が弱い

### 7.4 relation score

親候補 A と子候補 B に対し、`dependency_score(A -> B)` を計算する。

### 強い正の証拠

- A が B より前
- 同一 TU / 同一 file 近傍
- A が syntax / parse / template / macro / include root 系
- B が generic, repeated, actionability 低
- 同一 symbol / same frontier / same normalized message
- A が user-owned, B が internal/vendor 深部

### 強い負の証拠

- 別 TU かつ shared key 無し
- compile phase と link phase が混ざる
- B 自身が高い root score を持つ
- A/B の primary user-owned location が明確に別問題
- parent 候補が複数いて margin が薄い

### 7.5 episode 形成規則

1. `root_score` を計算
2. candidate parent を列挙し `dependency_score` を計算
3. child は **最良 parent が threshold 超かつ margin 超** のときだけ parent を持つ
4. cycle を禁止
5. parent chain で connected component を作る
6. component 内の最大 `root_score` を lead root とする
7. component 外に残った group は independent episode とする

### 7.6 hidden suppression の安全条件

hidden suppression は score だけで決めない。  
**score + 冗長証拠 + visibility floor** で決める。

### hidden にしてよい条件

- `role in {FollowOn, Duplicate}`
- `visibility_floor == HiddenAllowed`
- `suppress_likelihood >= suppress_threshold`
- `best_parent_margin >= min_parent_margin`
- かつ、以下のどちらか:
  - `strong evidence >= 2`
  - `strong evidence >= 1 && medium evidence >= 1`

### hidden にしてはいけない条件

- `role in {LeadRoot, IndependentRoot, Uncertain}`
- `visibility_floor != HiddenAllowed`
- 競合 parent 差が小さい
- 別 frontier の独立 error
- band/path の証拠不足で uncertain へ倒れるべきケース

### 7.7 summary に落とす条件

- `role == Duplicate` なら原則 hidden か count-only
- `role == FollowOn` で hidden 条件未満だが `summary_likelihood >= summary_threshold`
- `role == IndependentRoot` だが expanded budget を超えた
- `role == Uncertain` で expanded budget を超えた

### 7.8 見え方の最終クラス

| role | default の扱い |
|---|---|
| `LeadRoot` | expanded |
| `IndependentRoot` | expanded もしくは summary-only |
| `FollowOn` | hidden または collapsed summary |
| `Duplicate` | count-only か hidden |
| `Uncertain` | hidden 禁止。expanded か summary-only |

---

## 8. GCC9–15 / Path-aware parity の最終方針

### 8.1 同一にするもの

以下は band/path によらず同じである。

- role taxonomy
- visibility floor の意味
- independent root recall を守ること
- uncertain を hidden にしないこと
- quality gate の stop-ship 思想
- C / C++ template を対象 family に含めること

### 8.2 path-aware にするもの

path-aware にするのは以下だけ。

- evidence の取りやすさ
- hidden 判定に必要な冗長証拠量
- debug で露出できる根拠の密度

### 8.3 実務上の扱い

| band / path | hidden 判定の扱い | ゴール |
|---|---|---|
| `GCC15+ / DualSinkStructured` | 標準冗長証拠で hidden 可 | 最終参照 path |
| `GCC13-14 / SingleSinkStructured` | 標準冗長証拠で hidden 可 | 同じ UX 契約 |
| `GCC13-14 / NativeTextCapture` | 追加 corroboration を要求 | 同じ UX 契約 |
| `GCC9-12 / SingleSinkStructured(JSON)` | 標準〜やや強めの冗長証拠 | 同じ UX 契約 |
| `GCC9-12 / NativeTextCapture` | 最も保守的な hidden 条件 | 同じ UX 契約 |

ここで言う「保守的」とは、**independent root を見逃さないために uncertain を増やす** という意味であり、  
「改善対象 family を減らす」「成功定義を下げる」という意味ではない。

---

## 9. C++ template / macro / include の最終設計

### 9.1 template frontier

template flood の本質は「全部同じ error か」ではなく、  
**最初に見るべき user-owned frontier がどこか** である。

したがって最終設計では次を抽出する。

- first user-owned instantiation frame
- failing call / declaration / substitution site
- candidate-note cluster
- same-call-site overload family

同じ frontier を共有する group は強い same-episode 候補とする。

### 9.2 macro frontier

- first user-owned macro invocation
- definition site
- expanded expression / token mismatch summary

を key 化する。

### 9.3 include frontier

- first user-owned include boundary
- downstream std/vendor/generated 深部
- same-chain repeated context

を key 化する。

### 9.4 hidden にしてよい template 系

- internal instantiation repeats
- candidate-note flood
- 同一 frontier 下の substitution-failed repeats
- 同一 call-site 由来の overload repeat
- std/vendor header 深部の context flood

### 9.5 hidden にしてはいけない template 系

- 別 frontier の独立 error
- user-owned 別 call-site の別失敗
- `static_assert` など独立に意味がある root
- call-site は近いが symbol/instantiation が別の問題

---

## 10. 外部設定面の最終設計

### 10.1 Config/TOML

```toml
schema_version = 1

[render]
profile = "default"
path_policy = "relative_to_cwd"
debug_refs = "none"

[cascade]
compression_level = "aggressive"
suppress_likelihood_threshold = 0.78
summary_likelihood_threshold = 0.55
min_parent_margin = 0.12
max_expanded_independent_roots = 2
show_suppressed_count = true
```

### 10.2 CLI

既存の `--formed-*` 命名規則に合わせて、少なくとも次を追加する。

```text
--formed-cascade-level=off|conservative|balanced|aggressive
--formed-cascade-suppress-threshold=<float>
--formed-cascade-summary-threshold=<float>
--formed-cascade-min-parent-margin=<float>
--formed-max-expanded-independent-roots=<n>
--formed-show-suppressed-count=auto|always|never
```

### 10.3 precedence

既存方針と合わせて次で固定する。

`CLI > user config > admin config > built-in defaults`

### 10.4 compression level の厳密な意味

| level | hidden suppression | summary compaction | 用途 |
|---|---|---|---|
| `off` | しない | しない | raw 比較 / デバッグ |
| `conservative` | duplicate のみ原則可。follow-on hidden はほぼ不可 | 可 | 誤 suppress 最優先 |
| `balanced` | duplicate と very-strong follow-on を hidden 可 | 可 | 日常利用の中庸 |
| `aggressive` | strong follow-on / duplicate を hidden 可 | 可 | 既定値 |

### 補足

- `off` でも analysis 自体は走らせてよい。trace/debug のためである。
- `conservative` でも independent root recall 契約は変わらない。
- `aggressive` でも uncertain hidden は禁止。

---

## 11. user-facing UX の最終契約

### 11.1 default で守る不変条件

- compile failure 時に visible error が 0 件になる suppression は禁止
- independent root を hidden にするのは禁止
- 複数 independent root がある run では、少なくとも summary では全部見える
- suppression の説明は default で 1 行を原則とする
- raw facts に戻る導線を残す

### 11.2 default の表示契約

1. independent root episode をまず並べる
2. expanded は `max_expanded_independent_roots` 件まで
3. 残り independent root は `other independent errors:` に summary-only で出す
4. follow-on は hidden または root 配下の collapsed notice
5. duplicate は count-only
6. suppressed count 行は 1 行以内

例:

```text
error: likely missing ';' after this declaration
--> src/a.c:12:5
help: add ';' after 'int x = 10'
why: the parser reached the next statement before closing this declaration
note: 5 likely follow-on diagnostics were hidden for readability

other independent errors:
  - src/b.c:31:17: error: this call passes 'const char *' where 'int' is required
  - src/c.c:8:3: error: undefined reference to 'foo_init'
```

### 11.3 debug / verbose / raw_fallback

- `default` / `concise` / `ci`  
  読む量を減らす。suppression 理由は最小。
- `verbose`  
  collapsed section を少し開く。
- `debug`  
  suppressed episode 一覧、evidence tag、best-parent、threshold 判定を見せる。
- `raw_fallback`  
  wrapper がよりよい表示を保証できないと判断した場合の honest fallback。

---

## 12. 到達状態 E1 / E2 / E3 の最終定義

## E1: Safe Cascade Compression

### 到達条件

- `diag_cascade` が導入済み
- episode graph と group analysis が document に入る
- parse / type / linker / basic-template の cascade rules が入る
- renderer が episode-first になっている
- external cascade controls が配線済み
- GCC9–15 で independent root recall 契約を守る

### この時点でユーザーに約束できること

- 1 個の parse root で大量にズレたエラーは default でかなり減る
- 独立な compile error 3 件は 1 回の run で 3 件とも見える
- `--formed-cascade-level=off` で生比較ができる

## E2: Deep Context Compression

### 到達条件

- template/macro/include frontier が入る
- candidate-note flood が count 化できる
- C++ template family で user-owned frontier が安定する
- GCC9–15 の template 系で同じ episode-first UX を返せる

### この時点でユーザーに約束できること

- template / stdlib / include flood を wrapper する意味が明確になる
- 「gcc-formed にしかない価値」が最も見えやすくなる

## E3: Operational Final State

### 到達条件

- path-aware corpus と gate が揃う
- anti-collision corpus が stop-ship 化される
- debug/explainability surfaces が揃う
- default / verbose / debug / raw_fallback の契約差が固定される

### この時点でユーザーに約束できること

- regression を印象論ではなく gate で止められる
- suppression の透明性を必要に応じて追跡できる
- GCC9–15 を正式に維持運用できる

---

## 13. Epic / Wave / Work Package 構成

### 13.1 Epic

- `CEP-010` Contract and pipeline foundation
- `CEP-020` Safe cascade analysis
- `CEP-030` Episode-first rendering and deep C++ compression
- `CEP-040` Quality, parity, and explainability

### 13.2 Wave と到達状態の対応

- **W0**: `WP-001`〜`WP-002`（foundation）
- **W1 / E1**: `WP-003`〜`WP-007`
- **W2 / E2**: `WP-008`
- **W3 / E3**: `WP-009`〜`WP-011`

### 13.3 一覧表

| WP | Title | Epic | Wave | rLoC 目安 | 主 crate | 完了時の意味 |
|---|---|---|---:|---:|---|---|
| WP-001 | Add typed document-wide cascade schema and policy surface | CEP-010 | W0 | 900–1200 | `diag_core`, `diag_cli_front` | foundation |
| WP-002 | Introduce `diag_cascade` and hook it into execute pipeline with fail-open | CEP-010 | W0 | 800–1100 | `diag_cascade`, `diag_cli_front` | foundation |
| WP-003 | Implement logical-group extraction and canonical anchor/key derivation | CEP-020 | W1 | 900–1200 | `diag_cascade` | E1 前提 |
| WP-004 | Implement safe relation graph, root scoring, and episode formation | CEP-020 | W1 | 1000–1300 | `diag_cascade`, `diag_core` | E1 前提 |
| WP-005 | Seed parse/type/linker/basic-template cascade rulepack across GCC 9–15 | CEP-020 | W1 | 1000–1300 | `diag_rulepack`, `diag_cascade` | E1 前提 |
| WP-006 | Make renderer episode-first and guarantee independent-root visibility in one run | CEP-030 | W1 | 900–1200 | `diag_render` | E1 前提 |
| WP-007 | Wire external cascade controls and aggressive defaults | CEP-030 | W1 | 800–1100 | `diag_cli_front`, `diag_render`, `diag_cascade` | **E1 完成** |
| WP-008 | Implement deep template/macro/include/candidate-note compaction with frontier selection | CEP-030 | W2 | 1100–1400 | `diag_cascade`, `diag_rulepack`, `diag_render` | **E2 完成** |
| WP-009 | Add cascade-aware corpus schema and path-aware quality gates | CEP-040 | W3 | 900–1200 | `diag_testkit`, `xtask` | E3 前提 |
| WP-010 | Build anti-collision corpus for independent-root recall and false-suppression prevention | CEP-040 | W3 | 800–1100 | `diag_testkit`, `xtask` | E3 前提 |
| WP-011 | Add debug/explainability surfaces for suppressed episodes and evidence tags | CEP-040 | W3 | 800–1000 | `diag_trace`, `diag_render`, `diag_cascade` | **E3 完成** |

---

## 14. WP issue 化ルール

各 WP は repo 既存の `work_package.yml` にそのまま流し込める形で起票する。  
以下の欄名をそのまま使う。

- Goal
- Why now
- Parent epic or ADR
- Affected band
- Processing path
- Allowed files
- Forbidden surfaces
- Acceptance criteria
- Commands
- Docs impact
- Stop conditions
- Reviewer evidence

---

## 15. Work Package 詳細

### WP-001
### Title
`[wp] Add typed document-wide cascade schema and policy surface`

- **Parent epic or ADR**: `CEP-010`, `ADR-0029`, `ADR-0031`
- **Affected band**: Cross-cutting
- **Processing path**: Cross-path
- **Target rLoC**: 900–1200

### Goal
`DiagnosticDocument.document_analysis` と `CascadePolicySnapshot` を導入し、CLI/config から解決できる外部 policy surface を定義する。

### Why now
後続 WP が共通 contract なしに進むと、graph・renderer・tests で表現が分岐して ad-hoc 化するため。

### Allowed files
- `diag_core/src/**`
- `diag_cli_front/src/args.rs`
- `diag_cli_front/src/config.rs`
- `config/cc-formed.example.toml`
- `docs/specs/diagnostic-ir-v1alpha-spec.md`

### Forbidden surfaces
- `diag_adapter_gcc/**`
- `diag_render` の user-visible behavior
- `diag_rulepack` の family rules

### Acceptance criteria
- Yes: `DiagnosticDocument` が `document_analysis` を optional field として serde round-trip できる
- Yes: `[cascade]` config と `--formed-cascade-*` CLI が parse できる
- Yes: precedence が `CLI > user > admin > built-in` でテスト固定される
- Yes: この WP 単独では user-visible render snapshot が変わらない
- Yes: IR spec が additive update される

### Commands
- `cargo test -p diag_core`
- `cargo test -p diag_cli_front`
- `cargo xtask check`

### Docs impact
- `docs/specs/diagnostic-ir-v1alpha-spec.md`
- `config/cc-formed.example.toml`

### Stop conditions
- optional field 追加で済まず破壊的 schema change が必要になった
- CLI naming が既存 `--formed-*` 方針と衝突した
- config merge が既存 precedence を壊す必要が出た

### Reviewer evidence
- serde round-trip test
- config precedence test
- no-behavior-change snapshot

---

### WP-002
### Title
`[wp] Introduce diag_cascade and hook it into execute pipeline with fail-open`

- **Parent epic or ADR**: `CEP-010`, `ADR-0031`
- **Affected band**: Cross-cutting
- **Processing path**: Cross-path
- **Target rLoC**: 800–1100

### Goal
`diag_cascade` crate を新設し、`execute.rs` の `enrich -> render` 間に fail-open 前提の analyzer hook を差し込む。

### Why now
graph と renderer を分離した層構造を先に固定しないと、後の WP で heuristic が render 側へ漏れるため。

### Allowed files
- 新設 `diag_cascade/**`
- `diag_cli_front/src/execute.rs`
- root `Cargo.toml`
- 必要最小限の `diag_trace/**`

### Forbidden surfaces
- `diag_render` の selection ルール変更
- family-specific rule 実装
- adapter contract 変更

### Acceptance criteria
- Yes: `diag_cascade` を workspace member として追加できる
- Yes: analyzer 呼び出しが pipeline に入る
- Yes: analyzer error/panic を想定した fail-open test がある
- Yes: `document_analysis == None` でも render が通る
- Yes: この WP 単独では render 結果が変わらない

### Commands
- `cargo test -p diag_cascade`
- `cargo test -p diag_cli_front`
- `cargo xtask check`

### Docs impact
- 必要なら `docs/process/EXECUTION-MODEL.md` の pipeline 記述
- 最小限の crate-level docs

### Stop conditions
- hook に adapter 側の破壊的変更が必要
- fail-open が守れない
- analyzer 失敗時に build/exit code semantics が壊れる

### Reviewer evidence
- pipeline wiring test
- fail-open test
- workspace build evidence

---

### WP-003
### Title
`[wp] Implement logical-group extraction and canonical anchor/key derivation`

- **Parent epic or ADR**: `CEP-020`
- **Affected band**: GCC15+, GCC13-14, GCC9-12
- **Processing path**: DualSinkStructured, SingleSinkStructured, NativeTextCapture
- **Target rLoC**: 900–1200

### Goal
top-level diagnostics から deterministic な `LogicalGroup` を作り、anchor/key 群と candidate pre-filter を導出する。

### Why now
relation graph の品質は group/anchor/key の安定性に依存するため。ここを曖昧にすると後続 WP が全部揺れる。

### Allowed files
- `diag_cascade/src/**`
- 必要最小限の `diag_core/src/**`（型補助のみ）
- group/key 用の unit test fixture

### Forbidden surfaces
- renderer layout
- suppression threshold
- family-specific deep rules

### Acceptance criteria
- Yes: 同じ入力に対して grouping と key 導出が deterministic
- Yes: multi-file / same-file / linker fixture で group partition test がある
- Yes: structured/native-text 両 path に対する key 導出 test がある
- Yes: candidate pre-filter が全比較前提を外している

### Commands
- `cargo test -p diag_cascade`
- `cargo xtask check`

### Docs impact
- 必要なら internal design note
- spec 更新は原則不要

### Stop conditions
- anchor 導出に adapter facts の不足が見つかり、ingest contract 変更が必要
- group hint (`group_ref`) 依存が強すぎて deterministic にならない

### Reviewer evidence
- deterministic snapshot
- group partition tests
- candidate pre-filter tests

---

### WP-004
### Title
`[wp] Implement safe relation graph, root scoring, and episode formation`

- **Parent epic or ADR**: `CEP-020`
- **Affected band**: GCC15+, GCC13-14, GCC9-12
- **Processing path**: DualSinkStructured, SingleSinkStructured, NativeTextCapture
- **Target rLoC**: 1000–1300

### Goal
generic relation engine、root/dependency scoring、forest-based episode formation、visibility floor の基礎を実装する。

### Why now
「何を independent root と見なすか」が実質的な製品価値の中心であり、family rules より先に generic safety model を固定すべきため。

### Allowed files
- `diag_cascade/src/**`
- `diag_core/src/**`（analysis/document types）
- 必要最小限の test fixtures

### Forbidden surfaces
- deep template/include/macro rules
- renderer の表示文言
- config/CLI 追加

### Acceptance criteria
- Yes: relation graph に cycle ができない
- Yes: parent margin が不足する場合は child が independent/uncertain 側に残る
- Yes: evidence が弱いときは hidden suppression に入らない
- Yes: independent same-file fixture を 1 episode に潰さない
- Yes: `group_analysis.role` と `visibility_floor` が materialize される

### Commands
- `cargo test -p diag_cascade`
- `cargo test -p diag_core`
- `cargo xtask check`

### Docs impact
- `docs/specs/diagnostic-ir-v1alpha-spec.md`（必要なら group analysis 追記）

### Stop conditions
- generic engine だけではなく family rule 実装まで広がり始めた
- hidden 条件を曖昧な人間判断に寄せないと成立しない

### Reviewer evidence
- graph formation tests
- cycle prevention tests
- independent-preservation tests

---

### WP-005
### Title
`[wp] Seed parse/type/linker/basic-template cascade rulepack across GCC 9–15`

- **Parent epic or ADR**: `CEP-020`, `ADR-0029`
- **Affected band**: GCC15+, GCC13-14, GCC9-12
- **Processing path**: DualSinkStructured, SingleSinkStructured, NativeTextCapture
- **Target rLoC**: 1000–1300

### Goal
parse/type/linker/basic-template family について、最初の製品価値が出る cascade rulepack を seed する。

### Why now
generic graph があっても family 規則が無いと root/follow-on の分離精度が足りず、E1 の実益が出ないため。

### Allowed files
- `diag_rulepack/src/**`
- `diag_cascade/src/**`
- 必要最小限の `diag_enrich/src/**`
- representative corpus fixtures

### Forbidden surfaces
- deep template frontier
- renderer layout redesign
- config surface 拡張

### Acceptance criteria
- Yes: parse/desync flood で follow-on groups が hidden または collapsed される
- Yes: linker repeat で generic `collect2` が lead root を奪わない
- Yes: basic-template flood で candidate repeats が圧縮される
- Yes: GCC9–15 representative corpus で role taxonomy と success definition が共通のまま
- Yes: band/path 差は hidden 条件の証拠量だけで吸収している

### Commands
- `cargo test -p diag_rulepack`
- `cargo test -p diag_cascade`
- `cargo xtask replay --subset representative`
- `cargo xtask check`

### Docs impact
- 必要なら rule taxonomy note
- family coverage を示す docs 追記

### Stop conditions
- GCC9–12 だけ success definition を下げないと成立しないという議論が出た
- family rule が renderer 側へ漏れ始めた

### Reviewer evidence
- representative replay report
- parse/type/linker/basic-template fixtures の before/after
- band/path 別の assertion evidence

---

### WP-006
### Title
`[wp] Make renderer episode-first and guarantee independent-root visibility in one run`

- **Parent epic or ADR**: `CEP-030`, `ADR-0031`
- **Affected band**: Cross-cutting
- **Processing path**: Cross-path
- **Target rLoC**: 900–1200

### Goal
renderer を episode-first に変更し、独立 root を 1 回の run で必ず可視化する。

### Why now
analysis だけ先にあっても user-facing value は出ない。E1 を成立させるには renderer 契約まで確定している必要があるため。

### Allowed files
- `diag_render/src/**`
- 必要最小限の `diag_core/src/**`（view model 補助）
- render snapshot fixtures

### Forbidden surfaces
- relation graph 再推論
- family-specific rulepack changes
- adapter changes

### Acceptance criteria
- Yes: 3 件の独立 compile error fixture で 1 run 中に 3 件とも可視
- Yes: `max_expanded_independent_roots` 超過時は summary-only へ落ちるが不可視にはならない
- Yes: follow-on / duplicate は root 配下の collapsed notice か hidden/count-only に落ちる
- Yes: episode graph が無い場合は現行 selection に fail-open する
- Yes: renderer が relation heuristic を再計算していない

### Commands
- `cargo test -p diag_render`
- `cargo xtask snapshot --check --subset representative`
- `cargo xtask check`

### Docs impact
- `docs/specs/rendering-ux-contract-spec.md`

### Stop conditions
- renderer 側に parent/child 推論ロジックを足したくなった
- independent root recall を expanded budget 都合で落とし始めた

### Reviewer evidence
- multi-root snapshot
- cascade fixture snapshot
- no-episode-graph fallback test

---

### WP-007
### Title
`[wp] Wire external cascade controls and aggressive defaults`

- **Parent epic or ADR**: `CEP-030`, `ADR-0031`
- **Affected band**: Cross-cutting
- **Processing path**: Cross-path
- **Target rLoC**: 800–1100

### Goal
`compression_level`, thresholds, margin, expanded root budget, suppressed count 表示を完全に配線し、default を aggressive に固定する。

### Why now
E1 で価値を出しても、off/debug/raw 比較や現場調整ができなければ実運用に入れないため。

### Allowed files
- `diag_cli_front/src/**`
- `diag_render/src/**`
- `diag_cascade/src/**`
- `config/cc-formed.example.toml`
- `docs/specs/rendering-ux-contract-spec.md`

### Forbidden surfaces
- deep template frontier rules
- corpus schema redesign

### Acceptance criteria
- Yes: `--formed-cascade-level=off` で hidden suppression が消える
- Yes: threshold/margin を変えると summary/hidden 境界が deterministic に変わる
- Yes: default profile では aggressive semantics が有効
- Yes: `debug`/`raw_fallback` と default の振る舞い差が固定される
- Yes: config precedence が既存方針どおり維持される

### Commands
- `cargo test -p diag_cli_front`
- `cargo test -p diag_render`
- `cargo xtask snapshot --check --subset representative`
- `cargo xtask check`

### Docs impact
- `config/cc-formed.example.toml`
- `docs/specs/rendering-ux-contract-spec.md`
- 必要なら README の user-facing flags

### Stop conditions
- external control のために hidden safety floor を崩す必要が出た
- default aggressive を正当化する replay evidence が出ない

### Reviewer evidence
- CLI/config precedence tests
- level/threshold behavior tests
- representative snapshots

### Stage result
- **この WP 完了で E1 完成**

---

### WP-008
### Title
`[wp] Implement deep template/macro/include/candidate-note compaction with frontier selection`

- **Parent epic or ADR**: `CEP-030`, `ADR-0029`, `ADR-0031`
- **Affected band**: GCC15+, GCC13-14, GCC9-12
- **Processing path**: DualSinkStructured, SingleSinkStructured, NativeTextCapture
- **Target rLoC**: 1100–1400

### Goal
template/macro/include/candidate-note flood を user-owned frontier 基準で圧縮し、deep C++ cases を本命改善対象にする。

### Why now
ここをやらないと「gcc-formed にしかない価値」が template 系で十分に出ないため。

### Allowed files
- `diag_cascade/src/**`
- `diag_rulepack/src/**`
- `diag_render/src/**`
- template/macro/include corpus fixtures

### Forbidden surfaces
- global config/schema の再設計
- generic graph engine の大幅やり直し
- unrelated family の scope 拡大

### Acceptance criteria
- Yes: deep template fixture で visible frame 数が bounded になる
- Yes: first user-owned frontier が lead context として前に出る
- Yes: candidate-note flood が count-only または collapsed summary に落ちる
- Yes: macro/include flood で最初の user-owned boundary が見える
- Yes: 別 frontier の独立 error は hidden されない

### Commands
- `cargo test -p diag_cascade`
- `cargo test -p diag_render`
- `cargo xtask replay --subset representative`
- `cargo xtask snapshot --check --subset representative`

### Docs impact
- `docs/specs/rendering-ux-contract-spec.md`
- 必要なら family-specific corpus docs

### Stop conditions
- 別 frontier の独立 error を hidden にしないと line budget が守れない
- template 改善のために GCC9–12 を実質 scope 外へ追いやる案が出た

### Reviewer evidence
- deep template snapshots
- macro/include snapshots
- anti-collision template fixtures

### Stage result
- **この WP 完了で E2 完成**

---

### WP-009
### Title
`[wp] Add cascade-aware corpus schema and path-aware quality gates`

- **Parent epic or ADR**: `CEP-040`, `ADR-0029`, `ADR-0031`
- **Affected band**: Cross-cutting
- **Processing path**: Cross-path
- **Target rLoC**: 900–1200

### Goal
fixture schema に episode/cascade expectation を追加し、`VersionBand × ProcessingPath × Surface` ごとの gate を導入する。

### Why now
設計ができても gate が無いと再び single-track へ縮退するため。

### Allowed files
- `diag_testkit/**`
- `xtask/src/**`
- `corpus/**`
- `docs/specs/quality-corpus-test-gate-spec.md`

### Forbidden surfaces
- adapter/render/analysis の本体ロジック
- unrelated release tooling

### Acceptance criteria
- Yes: fixture schema が `expected_independent_episode_count` 等を持てる
- Yes: replay が episode-aware expectation を検証できる
- Yes: band/path ごとの missing cell が可視化される
- Yes: default/ci/debug の surface 差も gate できる
- Yes: `GCC9-12/NativeTextCapture` と `GCC9-12/SingleSinkStructured` を分けて扱う

### Commands
- `cargo test -p diag_testkit`
- `cargo test -p xtask`
- `cargo xtask replay --subset representative`
- `cargo xtask snapshot --check --subset representative`

### Docs impact
- `docs/specs/quality-corpus-test-gate-spec.md`

### Stop conditions
- 新 schema が multi-root/hidden count を表現できない
- band/path を折りたたまないと gate が組めない

### Reviewer evidence
- updated schema examples
- replay report
- gate matrix sample

---

### WP-010
### Title
`[wp] Build anti-collision corpus for independent-root recall and false-suppression prevention`

- **Parent epic or ADR**: `CEP-040`, `ADR-0029`
- **Affected band**: GCC15+, GCC13-14, GCC9-12
- **Processing path**: DualSinkStructured, SingleSinkStructured, NativeTextCapture
- **Target rLoC**: 800–1100

### Goal
independent root recall を壊す adversarial corpus を整備し、false hidden suppression を stop-ship 化する。

### Why now
カスケード圧縮の最大リスクは「本当の独立 error を隠す」ことだから。ここを gate 化しないなら製品価値は成立しない。

### Allowed files
- `corpus/**`
- `diag_testkit/**`
- `xtask/src/**`
- 必要なら corpus governance docs

### Forbidden surfaces
- algorithm 本体の scope 拡大
- render wording の調整

### Acceptance criteria
- Yes: curated anti-collision corpus で false hidden suppression = 0
- Yes: independent root recall = 100%
- Yes: 同一ファイル独立 syntax error 2 件 fixture がある
- Yes: syntax flood + truly independent type error fixture がある
- Yes: template flood + 別 frontier independent error fixture がある
- Yes: GCC9–12 の native text / JSON parity fixture がある

### Commands
- `cargo test -p diag_testkit`
- `cargo xtask replay --subset representative`
- `cargo xtask snapshot --check --subset representative`

### Docs impact
- 必要なら `docs/specs/quality-corpus-test-gate-spec.md`
- corpus metadata docs

### Stop conditions
- failure を「success definition を下げる」ことでしか回避できない
- anti-collision fixture が band/path 片寄りになる

### Reviewer evidence
- anti-collision replay report
- false-hidden = 0 evidence
- independent-root recall evidence

---

### WP-011
### Title
`[wp] Add debug/explainability surfaces for suppressed episodes and evidence tags`

- **Parent epic or ADR**: `CEP-040`, `ADR-0031`
- **Affected band**: Cross-cutting
- **Processing path**: Cross-path
- **Target rLoC**: 800–1000

### Goal
default を短く保ったまま、debug では suppressed episodes, evidence tags, best parent, raw provenance を追えるようにする。

### Why now
攻めた suppression を shipped behavior にするなら、review/debug 面で透明性を担保する必要があるため。

### Allowed files
- `diag_trace/**`
- `diag_render/src/**`
- `diag_cascade/src/**`
- 必要最小限の specs/docs

### Forbidden surfaces
- default output の大幅長文化
- family rule の再設計
- adapter changes

### Acceptance criteria
- Yes: debug profile で suppressed episode 一覧が deterministic に出る
- Yes: evidence tags と best parent が trace から追える
- Yes: raw provenance へ辿れる
- Yes: default profile の出力量は増やさない
- Yes: suppression explanation の fact/policy 境界が分かる

### Commands
- `cargo test -p diag_trace`
- `cargo test -p diag_render`
- `cargo xtask snapshot --check --subset representative`
- `cargo xtask check`

### Docs impact
- `docs/specs/rendering-ux-contract-spec.md`
- 必要なら trace docs

### Stop conditions
- debug explainability のために default を長文化し始めた
- evidence tag が compiler facts を装うような表現になった

### Reviewer evidence
- debug snapshot
- trace serialization sample
- default profile non-regression evidence

### Stage result
- **この WP 完了で E3 完成**

---

## 16. Issue 起票順

次の順で切る。

1. `WP-001`
2. `WP-002`
3. `WP-003`
4. `WP-004`
5. `WP-005`
6. `WP-006`
7. `WP-007`
8. `WP-008`
9. `WP-009`
10. `WP-010`
11. `WP-011`

### この順にする理由

- `WP-001` / `WP-002` が無いと contract と pipeline が固定できない
- `WP-003` / `WP-004` が無いと renderer に heuristic が漏れる
- `WP-005` で最初の family 価値が出る
- `WP-006` / `WP-007` で初めて E1 が user-visible に成立する
- `WP-008` で template 本命価値を取りにいく
- `WP-009` / `WP-010` / `WP-011` で運用可能な shipped contract になる

---

## 17. stop-ship 指標

この機能について最低限止めるべき指標をここで固定する。

### 17.1 必須

- anti-collision corpus で `false hidden suppression == 0`
- anti-collision corpus で `independent root recall == 100%`
- default TTY で native non-regression 契約に反しない
- `raw_fallback` / provenance が追える
- `GCC9-12` を issue/corpus/gate から外さない

### 17.2 強く推奨

- representative cascade fixtures で visible error/read cost が native より改善
- template representative fixtures で internal frame / candidate flood が bounded
- debug で suppression 理由が deterministic に見える

---

## 18. 最終判断

**実現可能である。しかも gcc-formed 固有の価値になり得る。**

ただし価値の源泉は、単なる headline 改善ではない。  
価値の源泉は **document 全体を見て independent root と follow-on cascade を分離する episode-first アーキテクチャ** にある。

この最終設計の結論は、次の 7 点に尽きる。

1. **`diag_enrich` の後、`diag_render` の前に `diag_cascade` を新設する**
2. **IR に typed な `document_analysis` / `EpisodeGraph` / `GroupCascadeAnalysis` を持たせる**
3. **renderer を episode-first に変え、独立 root を 1 回の compile で全部可視にする**
4. **default は aggressive suppression だが、uncertain hidden は禁止し、閾値は外部設定可能にする**
5. **GCC9–15 は同じ success definition を採り、path 差は証拠冗長性だけで吸収する**
6. **C++ template / macro / include は専用 wave で深くやる**
7. **anti-collision corpus を stop-ship gate にする**

この方針なら、  
「1 か所の根因でパーサーや文脈がずれ、大量の誤検知っぽい診断を延々読まされる」  
という C/C++ の典型的な苦痛に対して、gcc-formed は本当に意味のある解決策になれる。
