## g++ / g++-formed Demo Lab

`sample/` は、このリポジトリの見た目の差分をすぐ試すための手元デモ用フォルダです。
意図的にコンパイルやリンクを失敗させるサンプルを置いてあり、`g++` と `g++-formed` の出力をそのまま比較できます。

### 最初の 3 コマンド

```bash
./sample/compare.sh lambda_capture
./sample/run-formed.sh template_instantiation verbose
./sample/run-matrix.sh lambda_capture
```

すべての実行結果は `sample/out/` に保存されます。
サンプルは失敗ケースなので、コンパイラの終了コードは `1` になることがありますが、スクリプト自体は比較しやすいように結果保存を優先します。

### 収録ケース

| case id | 何を見るか | メモ |
|---|---|---|
| `lambda_capture` | lambda capture 漏れ | GCC 13 の既定 path でも差分が出やすい最初のおすすめ |
| `template_instantiation` | template instantiation failure | スクリプト側で `single_sink_structured` を付けて見やすい形に寄せる |
| `ranges_views` | C++20 ranges/views 失敗 | `-std=c++20` と `single_sink_structured` を自動で付ける |
| `duplicate_symbol` | 複数 translation unit の link error | `g++` と `g++-formed` の linker 表示差分を見る |

### 設定ファイル

設定は `sample/config/` にまとめてあります。スクリプトは `FORMED_CONFIG_FILE` でその場の設定を直接読ませるので、XDG 配下へ手でコピーする必要はありません。

| config id | 内容 |
|---|---|
| `default` | `subject_blocks_v2` + `default` profile |
| `concise` | 情報量を減らした `concise` profile |
| `verbose` | 情報量を増やした `verbose` profile |
| `subject_blocks_v1` | 旧 beta default の preset |
| `legacy_v1` | 旧 wording / 旧 session へ戻す preset |
| `dedicated_location` | カスタム presentation overlay。location を専用行へ寄せ、ラベル幅も固定化 |

### スクリプト

```bash
./sample/run-raw.sh <case> [-- extra g++ args]
./sample/run-formed.sh <case> [config] [-- extra wrapper/compiler args]
./sample/compare.sh <case> [config] [-- extra wrapper/compiler args]
./sample/run-matrix.sh <case> [-- extra wrapper/compiler args]
```

補足:

- `run-formed.sh` は terminal output に加えて `public.<config>.json` も `sample/out/<case>/` へ保存します。
- `g++-formed` の binary がまだ無い場合は、スクリプトが `cargo build -p diag_cli_front --bin gcc-formed` を自動実行します。
- `run-matrix.sh` は `raw g++` と複数 config の formed 出力をまとめて保存します。

### 例

```bash
./sample/run-formed.sh lambda_capture concise
./sample/run-formed.sh ranges_views dedicated_location
./sample/compare.sh duplicate_symbol legacy_v1
./sample/run-formed.sh template_instantiation default -- --formed-profile=debug
```
