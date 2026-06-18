# font モジュールの成熟度を正式リリースに向けて改善する (メタ issue)

- Priority: High
- Category: other（メタ issue）。ファイル名の `enhance` は作成時の命名であり、`git mv` による副作用（git log 断絶・他ブランチの作業中 issue からの参照断絶 等）を避けるため維持する
- Created: 2026-03-28
- Model: Opus 4.6
- Polished: 2026-06-19

## 目的

font モジュールを正式リリース可能な品質にする。個別の具体的な課題は下記の concrete issue に分割済みであり、本 issue は進捗のトラッカーとして機能する。

## 現状

- TrueType アウトライン (Simple Glyph / Compound Glyph) の描画に**一部**対応（後述の既知の制限参照）
- cmap Format 4 (BMP) / Format 12 (Full Unicode) に対応
- head, maxp, hhea, hmtx, loca, cmap, glyf テーブルのパースに対応
- TTC (TrueType Collection) に対応
- 公開 API: `FontData`, `FontFace`, `Font`, `FontError`。`Font` は `size()`, `scale()`, `map_char_to_glyph()`, `glyph_advance()`, `append_glyph_outline()`, `ascent()`, `descent()` を公開
- `Context::fill_text` によるテキスト描画に対応済み。`Context::fill_text` はエラーを黙殺し、一部の文字が描画されない場合がある（既知の制限参照）
- 単体テスト (`tests/test_font.rs`) が 5 つ存在 (メトリクス、cmap ルックアップ、アウトライン、fill_text 統合)
- テストは macOS の Arial.ttf に依存しており、CI (Ubuntu/Windows) では全テストがスキップされる

### 補足

- `Cargo.toml` の `exclude = ["fuzz"]` により `fuzz/` はワークスペースメンバーから除外されている。cargo-fuzz の実行には `--manifest-path fuzz/Cargo.toml` が必要であり、fuzzing 基盤の concrete issue 化時に CI コマンドへ反映すること

## 残存課題

### PBT / fuzzing が未整備

テーブルパーサとグリフアウトライン変換の正しさを PBT で検証していない。フォントファイルは外部入力であり、不正なデータに対するクラッシュ耐性を fuzzing で検証する必要がある。

- tables.rs: パース結果の不変条件（テーブルサイズ検証、必須フィールドの範囲、値の一貫性等）の PBT が必要
- glyph.rs: Simple/Compound Glyph のアウトライン変換における不変条件（NaN を含まない、contour が閉じている、点数が上限を超えない等）の PBT が必要
- cmap: Format 4 / Format 12 の既知フォントに対するルックアップ結果が FreeType 等のリファレンス実装と一致することの検証が必要
- fuzzing: `fuzz/` に `.gitkeep` のみ存在。`cargo-fuzz` (libfuzzer) を使用する前提で font テーブルパーサの fuzzing ターゲットが未作成

完了条件の具体化は後述の「テスト品質」セクションに記述する。

### OpenType (CFF / CFF2) アウトラインに未対応

CFF/CFF2 ベースの OpenType フォントは読み込めない（`src/font/tables.rs:62-66` で `OTTO` sfnt version はエラーとして拒否される）。CFF 対応の要不要判断は本 issue の tracked items「CFF 調査」および完了条件「CFF 判断」で管理する（調査方法・閾値・CFF/CFF2 区別の詳細はそちらを参照）。

## Concrete issue 一覧

| issue | タイトル | 状態 | 優先度 |
|-------|---------|------|--------|
| 0021 | Font にスケール済みメトリクス取得を追加する | 未着手 | High |
| 0022 | テキストサイズ計測機能を追加する | 未着手 | High |
| 0023 | グリフ境界ボックス取得機能を追加する | 未着手 | High |
| 0024 | ストロークテキスト描画機能を追加する | 未着手 | High |
| 0025 | カーニング適用機能を追加する | 未着手 | Medium |
| 0026 | OpenType 基本シェーピング機能を追加する | 未着手 | Medium |

### Tracked items

本 issue で追跡するが、concrete issue ファイルとして未作成の項目:

| 項目 | 説明 | 状態 |
|------|------|------|
| 既存コード PBT | tables.rs のテーブルパース関数群（`parse_head`, `parse_maxp`, `parse_hhea`, `parse_hmtx`, `parse_loca`, cmap format 4/12 パース）、glyph.rs のグリフアウトライン変換関数群（`parse_simple_glyph`, `parse_compound_glyph`, `emit_contour`）の PBT。concrete issue 0021-0026 が追加するテーブルパーサ（OS/2, kern, GSUB, GPOS）の PBT は各 issue のスコープ | concrete issue 未作成 |
| fuzzing 基盤 | cargo-fuzz 初期化（`fuzz/Cargo.toml` 作成 + `cargo fuzz run` が手動実行可能な状態）。font テーブルパーサの fuzzing ターゲット追加は本 tracked item のスコープ外（各 concrete issue で追加する）。CI での定期 fuzzing 実行（cron スケジュールの GitHub Actions workflow による）を含む。コーパスシードの有無・実行時間制限は concrete issue 化時に決定する | concrete issue 未作成 |
| CFF 調査 | macOS/Windows/Linux の OS 標準バンドルフォント全件のアウトライン形式調査、CFF 使用率算出、対応可否判断。sfnt version の確認には**実フォントファイルのバイナリチェックが必須**（sfnt version はファイル先頭 4 バイトの値であり、フォント名からの推定は不可能）。調査では CFF と CFF2 を区別して集計する | concrete issue 未作成 |
| テスト用フォント選定 | 全プラットフォームで利用可能・ライセンス的に配布可能なテスト用フォントの選定とリポジトリ同梱。選定要件: SIL OFL 等の配布許容ライセンス、各 concrete issue が必要とするテーブル（OS/2, kern, GSUB, GPOS）を充足すること、BMP 外文字および CJK 統合漢字のカバレッジを持つこと（cmap Format 12 のテストに必要）。複数フォントの使用も許容する。`FontData::from_bytes` を活用した `include_bytes!` 埋め込みの可否も検討する | concrete issue 未作成 |
| ベースライン benchmark 計測基盤 | `benches/font_text.rs` の作成と criterion 設定。テスト用フォント選定が完了するまでは空の bench 関数を定義し、`cargo bench` がエラーなく実行できる状態を計測基盤完了の基準とする。実際のベースライン値の取得は各 concrete issue の実装完了ごとに、選定済みフォントを用いて実施する。回帰判定基準は concrete issue 化時に決定する | concrete issue 未作成 |
| Compound Glyph point-matching 実装 | Compound Glyph の `ARGS_ARE_XY_VALUES == 0` のケース（point-matching による合成位置決定）を実装する。参照グリフの point 座標を解決し、適切な合成位置に子グリフを配置する。Bug カテゴリの concrete issue として発行する | concrete issue 未作成 |
| fill_text エラー黙殺コメント | `src/api/context.rs:1140-1169` の `fill_text` に、エラー黙殺のトレードオフ（部分欠落の許容、クラッシュ耐性の確保、glyph_id=0 の advance 幅が常に有効値とは限らないことへの注意）をコメントとして追記する。0022 の `fill_text` リファクタリング時に組み込む | concrete issue 未作成 |
| `pbt/tests/prop_font/main.rs` 初回作成 | ファイルが存在しないため最初に作成する必要がある。`src/font/` がディレクトリモジュールのため PBT 命名規則（`pbt/README.md`）に従い `prop_font/main.rs` 構成とする。既存の PBT テストはフラットファイルだが、font はサブモジュール（glyph, tables）を持つためディレクトリ構成が適切。0021 で作成する。本ファイルの初回作成が完了しなくても、各 concrete issue が個別にファイルを作成・拡張することは可能だが、ディレクトリ構成の一貫性のため 0021 で一括作成する | 0021 に紐付け |

上記の tracked items のうち、0021 着手前に concrete issue 化して**完了**させる必要があるもの:
- テスト用フォント選定（全 concrete issue がテストフォントに依存するため）
- fuzzing 基盤（0025 および fuzzing ターゲットを追加する全 concrete issue が依存するため）

ベースライン benchmark 計測基盤のセットアップ（`benches/font_text.rs` の作成、criterion の設定、空の bench 関数で `cargo bench` がエラーなく実行できる状態）は 0021 着手前に完了させる。テスト用フォント選定が完了するまでは空実装で問題ない。実際のベースライン値の取得はテスト用フォント選定完了後に、各 concrete issue の実装完了ごとに実施する。

それ以外の tracked items（既存コード PBT、CFF 調査、fill_text エラー黙殺コメント）は各 concrete issue の進捗に応じて適宜 concrete issue 化する。番号は SEQUENCE に従い割り当てる。

**進捗追跡**: 本 issue の tracked items テーブルの「状態」列を、concrete issue 化完了時に本 issue ファイルを直接編集して更新する。各 concrete issue の完了時も「Concrete issue 一覧」テーブルの「状態」列を更新する。

### 依存関係

concrete issue (0021-0026) 間の依存関係:

```
0021 (独立)
0022 (独立)
0023 (独立)
0024 → 0022 に依存（`glyph_run_for_text` を使用するため）
0025 → 0022, 0024 に依存（`glyph_run_for_text` と `stroke_text` の両方にカーニングを適用するため）
0026 → 0022, 0025 に依存（`glyph_run_for_text` を `shape()` で置き換えるため。GPOS Pair Adjustment で kern テーブルを置き換えるため 0025 に依存）
```

### fill_text リファクタリング担当の明確化

0022 と 0024 の間で `fill_text` を `glyph_run_for_text` を使う形に変更する担当が曖昧になっている。本メタ issue として以下のように決定する:

- `glyph_run_for_text` を作成する **0022 が担当する**（作成したヘルパーの初回利用を 0022 で完結させる）
- 0024 は 0022 完了後の `glyph_run_for_text` を前提として `stroke_text` を追加するのみとする
- この決定は 0022 および 0024 の各 issue ファイルにも反映すること

### 着手順序

1. **0021 着手前に concrete issue 化して完了**: テスト用フォント選定、fuzzing 基盤、ベースライン benchmark の計測基盤セットアップ。テスト用フォント選定は 0021-0023 に必要な最小限のフォント（cmap Format 4/12 を持ち TTF アウトラインのみのフォント）を先行して選定し、0025 以降に必要な kern/GSUB/GPOS テーブルを持つフォントは各 concrete issue の着手時までに追加選定する
2. **concrete issue**: 0021 / 0022 / 0023 (並行) → 0024 → 0025 → 0026

## 既知の制限

### Compound Glyph の point-matching 未実装

Compound Glyph のうち、`ARGS_ARE_XY_VALUES` フラグが 0 のケース（point-matching による合成位置決定）は未実装であり、合成位置が原点 (0, 0) にフォールバックされる（`src/font/glyph.rs:441-450`）。和文 TrueType フォント（MS ゴシック等）では point-matching が多用されており、これらのフォントでは Compound Glyph の描画結果が破綻する。

本制限は tracked item として concrete issue 化し、別途対応する。

### fill_text のエラー黙殺と未定義グリフのスキップ

`fill_text` は以下の理由で一部の文字が描画されない場合がある:

1. `append_glyph_outline` のエラーを完全に無視している（`src/api/context.rs:1160` で `let _ =` により黙殺）。不正フォントで特定グリフのアウトラインがパースできない場合、または上記 point-matching 未実装により Compound Glyph 合成位置が誤っている場合、そのグリフが描画されずにカーソルだけ進む。
2. `glyph_id == 0`（cmap でマッピング不可の文字）の場合、`.notdef` グリフすら描画せずにスキップし、glyph_id=0 の advance 幅でカーソルだけ進む。

いずれも外部入力を想定したクラッシュ耐性としては正しいが、描画の部分欠落が発生しうる。

本件は現時点では仕様として許容する。`fill_text` の戻り値を `Result` にする等のエラー通知は破壊的変更になるため、メジャーバージョンアップ時に別 issue として検討する。トレードオフのコメント追記は Tracked item「fill_text エラー黙殺コメント」で対応する。

## 備考

- 本 issue はメタ issue のため `Branch` フィールドを持たない（複数ブランチにまたがる）
- close 後は issues/closed へ移動し、「解決方法」セクションに完了した全 concrete issue の番号と完了日を記載する

## 解決方法

（本 issue の close 時に追記する）

## 完了条件

以下すべてを満たした時点で本 issue を close する。

### concrete issue

- 0021 ~ 0026 がすべて完了すること
- Tracked items（既存コード PBT、fuzzing 基盤、CFF 調査、テスト用フォント選定、ベースライン benchmark 計測基盤、Compound Glyph point-matching 実装、fill_text エラー黙殺コメント）が concrete issue として発行され、すべて完了していること

### パフォーマンス基準

- raden 全体の目標である 1080p 120fps（1 フレーム 8.33ms）を font モジュールが阻害しないこと。font 処理全体の予算は最大 1ms（1 フレームの約 12%）を目安とし、全文字描画処理（グリフアウトライン構築 + パス生成 + 描画）がこの範囲に収まることを基準とする
- font モジュール単体でのベースライン benchmark 計測基盤を 0021 着手前にセットアップし、各 concrete issue の実装完了ごとにベースライン値を取得して regress がないことを確認する
- 定量目標（N 文字あたりのアウトライン構築時間等）は各 concrete issue の実装時に設定する

### テスト品質

- PBT で font モジュールの各パーサが満たすべき不変条件が検証されていること。具体的な不変条件は各 concrete issue で列挙し、すべての PBT がパスしていることを完了条件とする
- fuzzing で font テーブルパーサのクラッシュ耐性が検証されていること。fuzzing の終了条件は: 最低 1 時間の連続実行、発見された全クラッシュの修正、修正後さらに 30 分の連続実行で新規クラッシュ 0 件
- テストの役割分担:
  - PBT: 型情報（Strategy）に基づく不変条件の検証
  - Fuzzing: 任意入力に対するクラッシュ耐性
  - 単体テスト: 意図的なエラーパス、境界値

### CFF 判断

- macOS / Windows / Linux の OS 標準バンドルフォント全件を対象に、アウトライン形式を調査する
- 各 OS の標準フォントリストは公開情報（Apple Font List、Microsoft Typography、各 Linux ディストリビューションのパッケージリスト）から取得する
- sfnt version の確認には**実フォントファイルのバイナリチェックが必須**である（sfnt version はファイル先頭 4 バイトの値であり、フォント名からの推定は不可能）
- CFF と CFF2 はフォーマットが異なるため、調査では両者を区別して集計する
- 調査結果から CFF 形式フォントの割合を算出し、対応可否を判断する
- 判断基準: `OTTO` sfnt version は CFF と CFF2 の両方で使用されるため、調査では各フォントの `CFF ` テーブル（tag 0x43464620）と `CFF2` テーブル（tag 0x43464632）の有無で両者を区別する。CFF と CFF2 それぞれについて、各 OS の標準バンドルフォントに占める割合を算出し、いずれか 1 OS で 10% 以上を占める場合に当該形式の対応 concrete issue を作成する。両方とも全 OS で 10% 未満の場合は、本 issue の完了条件から CFF 判断項目を削除し、CFF 調査 concrete issue を pending に移動する
- 閾値 10% は仮置きであり、調査結果を踏まえて本 issue の完了条件を更新する。閾値変更は本 issue を直接編集し、編集履歴は git に委ねる
- 調査結果は tracked item「CFF 調査」の concrete issue 内にレポートとして記録する
- 現状 `src/font/tables.rs:62-66` で `OTTO` sfnt version はエラーとして拒否されている。CFF 対応には `TableDirectory::parse` および `parse_all` の修正と CFF/CFF2 パーサの新規実装が必要

### API 安定性

- 既存の公開 API（FontData / FontFace / Font / FontError / Context::fill_text）に破壊的変更がないこと。新規追加される公開型（GlyphBounds, TextMetrics, GlyphBuffer, FontFeatureSettings 等）は `lib.rs` から re-export し、他公開型と一貫した API とする
- 後方互換のない変更は `CHANGES.md` の `CHANGE` 種別に従い明示すること

### クロスプラットフォーム

- macOS (arm64 + x86_64) / Windows (x86_64) / Ubuntu (x86_64 + arm64) で font テストがパスすること
- 全プラットフォームで利用可能なテスト用フォントバイナリをリポジトリに含める（tracked item「テスト用フォント選定」参照）
  - ライセンス的に配布可能なフォント（SIL Open Font License 等）を選定する
