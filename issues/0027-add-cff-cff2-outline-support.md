# CFF / CFF2 フォントアウトライン対応を追加する

- Priority: Medium
- Category: add
- Created: 2026-06-20
- Model: Kimi K2.7 Code
- Branch: feature/add-cff-cff2-outline-support
- Polished: 2026-06-20

## 目的

Blend2D は TrueType (`glyf`/`loca`) と CFF / CFF2 の両方を対等にサポートしている。raden は現状 `glyf`/`loca` のみのため、CFF ベースの OTF フォント（多くの Adobe 系プロフェッショナルフォント、Noto Sans CJK JP 等）でアウトライン取得に失敗する。本 issue では `CFF ` / `CFF2` テーブルをパースし、Type 2 CharString を既存の `Path` API に変換することで、Blend2D と同等のフォント対応範囲を実現する。

## 優先度根拠

Medium。Blend2D 互換を目指す上で必須な機能だが、0021-0026 の TrueType 前提機能群（メトリクス、サイズ計測、ストロークテキスト、カーニング、シェーピング）が完了した後に着手する。CFF 対応はこれらのパイプラインを流用する形で統合するため、0026 完了後が自然な着手タイミングとなる。

## Blend2D との対応

本 issue は `docs/BLEND2D.md` の以下の行を解消する。

- **L420 `outline_type()` / `diag_flags()`**: raden は `FontFace::outline_type()` でアウトライン種別（`TrueType` / `Cff` / `Cff2`）を公開する（Blend2D の `BLFontOutlineType` に対応）
- **L451 `get_glyph_outlines(...)`**: raden は既存の `Font::append_glyph_outline(glyph_id, offset_x, offset_y, &mut Path)` で CFF / CFF2 グリフも出力する（内部で `glyf` か CFF かを振り分ける）

差異の理由:

- raden は Rust の所有権モデルのため、`BLGlyphBuffer` への in-place 操作ではなく `Font::shape()` が新規 `GlyphBuffer` を返す。CFF 対応もこの設計を維持する
- `BLFontFaceInfo` / `BLFontOutlineType` 互換の構造体一括取得は font モジュール安定化フェーズで別 issue として判断する。本 issue では `outline_type()` の個別メソッドのみ追加する

完了 PR で `docs/BLEND2D.md` L420 / L451 の raden 列に新規 API 名を追記する。状態列 (`未実装` / `差異あり`) の最終整理は 0001 メタ issue の close PR でまとめて行う。

## 現状

- `src/font/tables.rs` は `head`, `maxp`, `hhea`, `hmtx`, `loca`, `cmap`, `glyf` のみパース
- `parse_all` は `glyf` を `dir.require(TAG_GLYF)?` で必須としており、`CFF ` / `CFF2` テーブルは未認識
- `src/font/glyph.rs` は `tables.loca_offsets` を直接参照しており、CFF フォントでは動作しない
- `FontFace::from_data` / `Font::append_glyph_outline` は TrueType アウトラインのみを想定
- 0001 メタ issue の CFF / CFF2 判断は「対応」に確定し、本 issue として concrete issue 化された

## 設計方針

### テーブル選択

Blend2D (`blend2d/opentype/otface.cpp:47-62`) と同じ優先順位を採用する。

1. `CFF ` または `CFF2` テーブルがあれば CFF 経由で初期化
2. なければ `glyf` + `loca` テーブルがあれば TrueType 経由で初期化
3. どちらもなければ `FontError::InvalidData("missing outline table")`

`CFF ` と `CFF2` が両方ある場合は `CFF2` を優先する（Blend2D と同じ）。

### CFF / CFF2 パーサ

新規モジュール `src/font/cff.rs` を作成し、以下を実装する。

- CFF Header のパース（v1: major / minor / hdrSize / offSize、v2: major / minor / headerSize / topDictLength）
- INDEX 構造のパース（v1: count 2 バイト + offSize、v2: count 4 バイト + offSize）
- Name INDEX / TopDict INDEX / String INDEX / GSubR INDEX / CharStrings INDEX（v1）
- TopDict から `CharStrings` オフセット、`Private` オフセット、`FDArray` / `FDSelect` 有無を取得
- PrivateDict から `Subrs` オフセットを取得
- GSubR / LSubR の bias 計算（v1: count < 1240 → 107、< 33900 → 1131、それ以上 → 32768。v2: 固定で 32768）
- CID フォントの FDArray / FDSelect 対応（Format 0 / 3）
- CFF2 variation 関連（`vsindex`, `blend` 等）は初版でスキップし、スタッククリアのみ対応（Blend2D と同じ TODO 方針）

### Type 2 CharString インタプリタ

- 数値スタック（v1: 最大 48、v2: 最大 513 を目標）
- 呼び出しスタック（最大 16）
- パス命令:
  - `move_to`: `rmoveto`, `hmoveto`, `vmoveto`
  - `line_to`: `rlineto`, `hlineto`, `vlineto`
  - `cubic_to`: `rrcurveto`, `hhcurveto`, `vvcurveto`, `vhcurveto`, `hvcurveto`, `rcurveline`, `rlinecurve`, `flex`, `flex1`, `hflex`, `hflex1`
  - `close`: `endchar`（v1）/ グリフ終了（v2）
- サブルーチン呼び出し: `callgsubr`, `calllsubr`, `return`
- v1 算術・条件分岐: 必要に応じて対応。安全のために未対命令は `FontError::InvalidData` とするか、寛容方針で無視するかは実装時に決定
- ヒント系 (`hstem`, `vstem`, `hintmask`, `cntrmask`): ビット数を数えてスタックを消費するが、実際のヒント処理は無視（Blend2D と同じ）

### 公開 API

- `FontFace::outline_type() -> OutlineType`
  - `OutlineType` は `#[non_exhaustive]` な enum:
    - `TrueType`
    - `Cff`
    - `Cff2`
- `Font::append_glyph_outline(...)` は内部で `outline_type` を見て `glyf` 経路または CFF 経路に振り分ける
- 既存の `FontFace::units_per_em()` / `ascent()` / `descent()` / `line_gap()` は `head` / `hhea` テーブル経由で維持（CFF フォントでもこれらのテーブルは通常存在）

### 0025 / 0026 との統合

- CFF 対応後も `kern` テーブル / GSUB / GPOS は TrueType と同じパイプラインで適用する
- `Font::shape()` は `outline_type` に関わらず同じ `GlyphBuffer` を返す
- `measure_text` / `fill_text` / `stroke_text` も `shape()` 経由で統一されるため、CFF 対応時にこれらを改修する必要はない

## 完了条件

### 追加される API

- `FontFace::outline_type() -> OutlineType`
- `OutlineType` enum（`TrueType`, `Cff`, `Cff2`）

### 既存 API の挙動変化

- `FontFace::from_data` が `CFF ` / `CFF2` テーブルを持つフォントでも成功するように変更（`glyf` の必須化を解除）
- `Font::append_glyph_outline` が CFF / CFF2 グリフも描画するように変更

### テスト

- 単体テスト: CFF ベースのテスト用フォント（0001 メタ issue「テスト用フォント選定」で確定）で `outline_type() == OutlineType::Cff` を検証
- 単体テスト: CFF フォントの既知グリフ（例: `.notdef`, `A`）で `append_glyph_outline` が空でない `Path` を生成することを検証
- PBT: 任意の CFF フォントに対し `outline_type()` が `TrueType` / `Cff` / `Cff2` のいずれかを返し、`glyph_id` 範囲外で `append_glyph_outline` がエラーまたは no-op となる不変条件を検証
- fuzzing: `parse_all` 経由で CFF / CFF2 テーブルを含むフォントの不正データに対するクラッシュ耐性を検証

### ドキュメント

- `docs/BLEND2D.md` L420 / L451 の raden 列に新規 API 名を追記
- `CHANGES.md` の `## develop` の `### misc` 直下に `ADD` 種別で `FontFace::outline_type` と CFF / CFF2 アウトライン対応を追記

## 解決方法

### 1. `src/font/tables.rs` の拡張

- `TAG_CFF`, `TAG_CFF2` 定数を追加
- `parse_all` で `glyf` を必須とせず、以下の順でアウトライン実装を選択:
  1. `dir.find(TAG_CFF2)` → CFF2
  2. `dir.find(TAG_CFF)` → CFF
  3. `dir.find(TAG_GLYF)` → TrueType
  4. なければエラー
- `ParsedTables` に `outline_type: OutlineType` を追加
- `head` / `hhea` / `hmtx` / `cmap` は引き続き必須または既存経路で取得

### 2. `src/font/cff.rs` の新規作成

- `CffTable` / `Cff2Table` 構造体（または統合した `CffData`）を定義
- `parse_cff(data: &[u8], rec: TableRecord) -> Result<CffData, FontError>` を作成
- Type 2 CharString インタプリタを実装
- CID フォント対応（FDSelect format 0 / 3、FDArray）

### 3. `src/font/mod.rs` の変更

- `OutlineType` enum を公開
- `FontFace::outline_type()` メソッドを追加
- `FontFaceInner` / `Font` は `tables.outline_type` を参照
- `Font::append_glyph_outline` で `outline_type` に応じて分岐

### 4. `src/font/glyph.rs` との統合

- `append_glyph_outline` のシグネチャは維持
- 内部で `match face.tables.outline_type` し、TrueType 時は既存ロジック、CFF / CFF2 時は `cff::append_outline` を呼び出す

### 5. テスト追加

- `tests/test_font.rs` に CFF フォント用テストを追加
- `pbt/tests/prop_font/main.rs` に `outline_type` の不変条件 PBT を追加
- 必要に応じて `fuzz/fuzz_targets/` に CFF 用 target を追加（0001 メタ issue「fuzzing 基盤」完了後）

### 6. ドキュメント更新

- `docs/BLEND2D.md` の該当行を更新
- `CHANGES.md` に `ADD` エントリを追加

## エッジケース

- `CFF ` と `CFF2` の両方があるフォント: `CFF2` を優先
- `head` / `hhea` / `hmtx` / `cmap` のいずれかが欠落する CFF フォント: `FontFace::from_data` は失敗させる（既存の TrueType と同じ必須セットを維持）
- CID フォントで `FDSelect` が未対応形式: エラーまたは `.notdef` フォールバック
- CharString 内の未対命令: 安全のため `FontError::InvalidData` とする（寛容方針にする場合は本 issue の polish で確定）
- `size == 0`: `scale == 0` で全座標が 0 となり、空に近い Path が生成される
- CFF2 variation フォント: 初版では variation 適用なし、基本のグリフ輪郭を描画

## 隣接 issue への影響

- **0001 メタ issue**: 本 issue 作成時に「CFF / CFF2 調査」tracked item を削除し、concrete issue 一覧に 0027 を追加する。CFF / CFF2 判断は「対応」に確定する
- **0025 への影響**: CFF フォントでも legacy `kern` テーブルがあれば適用される。GPOS 優先の fallback ロジックは CFF / TrueType で共通
- **0026 への影響**: CFF フォントでも GSUB / GPOS を適用する。`Font::shape()` は `outline_type` に関わらず統一された `GlyphBuffer` を返す
- **テスト用フォント選定 (0001 tracked)**: CFF ベースのテスト用フォントを選定する責務は本 issue 着手前に完了している必要がある

## 関連

- `0001-enhance-font-module-maturity.md` (メタ issue。CFF / CFF2 対応の判断責務と 0021-0027 との統合責務)
- 依存: `0026-add-opentype-basic-shaping.md` (`shape()` 経路への統合。CFF 対応は 0026 完了後に着手するのが自然)
- 依存: 0001 メタ issue の「テスト用フォント選定」tracked item (CFF フォントの選定)
- 参考実装: Blend2D `blend2d/opentype/otcff.cpp`, `blend2d/opentype/otcff_p.h`, `blend2d/opentype/otface.cpp`
