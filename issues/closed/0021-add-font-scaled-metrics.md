# Font にスケール済みメトリクス取得を追加する

- Priority: High
- Category: add
- Created: 2026-05-11
- Model: Kimi K2.7 Code
- Branch: feature/add-font-scaled-metrics
- Polished: 2026-06-20
- Completed: 2026-06-21

## 目的

`Font` (サイズ指定済みインスタンス) から `line_gap` / `cap_height` / `x_height` をスケール済み値で取得できるようにする。Blend2D の `BLFontMetrics` (スケール済み float) と `BLFontDesignMetrics` (デザイン値 int) のうち、`line_gap` / `cap_height` / `x_height` フィールドに対応する個別メソッドを `FontFace` と `Font` に追加する。OS/2 テーブル (cap/x の取得元) の新規パースを含む。

上付き / 下付き / アクセント位置決定や CSS `cap` / `ex` 単位の応用は OS/2 の `subscript_*` / `superscript_*` 等の別フィールドが必要で本 issue のスコープ外。

## Blend2D との対応

本 issue は `docs/BLEND2D.md` の以下の行を解消する (メタ issue 0001 の対応マトリクスで確定済み)。

- **L418 `BLFontFace::design_metrics()`**: raden は構造体一括取得 (`BLFontDesignMetrics` 互換) を本 issue では導入せず、`FontFace::cap_height()` / `FontFace::x_height()` の個別メソッドを追加する
- **L443 `BLFont::metrics()` / `design_metrics()`**: 同様に個別メソッド `Font::line_gap()` / `Font::cap_height()` / `Font::x_height()` を追加する
- **L457 raden 独自 `Font::scale()`**: 既存。スケール乗算の前提として利用 (状態変更なし、表は更新しない)

差異の理由:

- raden は `Option<i16>` / `Option<f64>` で OS/2 不在 / version < 2 を明示的に区別する (Blend2D は `int` の 0 で代用)。型安全性と呼び出し側でのフォールバック判断強制のため
- `BLFontMetrics` / `BLFontDesignMetrics` 互換の構造体一括取得は font モジュール安定化フェーズで別 issue として判断する (0001 メタ issue の「font 公開 API の安定化方針」参照)

完了 PR で `docs/BLEND2D.md` L418 / L443 の raden 列に新規 API 名 (`cap_height()` / `x_height()` / `line_gap()` 等) を追記する。状態列 (`差異あり: ...`) の最終整理は 0001 メタ issue の close PR でまとめて行う (本 issue では `差異あり` を `実装済み` に変えない)。

## 現状

- `FontFace::ascent()` / `descent()` / `line_gap()` は実装済み (`src/font/mod.rs:88-97`、hhea 由来)
- `Font::ascent()` / `Font::descent()` は実装済み (`src/font/mod.rs:170-178`)。`Font::line_gap()` のみ欠落
- `cap_height` / `x_height` は OS/2 テーブル version 2 以上の `sCapHeight` / `sxHeight` 由来。OS/2 は未パース (`src/font/tables.rs` の `parse_all` 経路で取得していない)
- `pbt/tests/prop_font/main.rs` は未作成 (0001 メタ issue で本 issue のサブタスクと確定)

## 設計方針

### OS/2 テーブルの取得方式

OS/2 テーブルは OpenType 仕様で必須だが、実フォントには:

- Wingdings / Webdings 等の記号フォント
- 一部の古い Apple TrueType (Mac OS Classic 時代の system フォント)

など OS/2 を持たないものが存在する。raden は `fill_text` 経路で OS/2 不在のフォントも許容する寛容方針 (cmap / hhea / hmtx / glyf があれば描画可能) のため `dir.find()` で取得し、不在時は `cap_height = None` / `x_height = None` で続行する (`dir.require()` を使うと `FontFace::from_data` 自体が失敗する)。

### `ParsedTables` への直結

`Os2Table` 構造体を `parse_os2` の戻り値型として一時的に定義するが、`ParsedTables` には `cap_height: Option<i16>` / `x_height: Option<i16>` を**直結追加**する (既存 `ascent` / `descent` / `line_gap` と同じパターン)。

将来 `BLFontDesignMetrics` 互換の構造体を導入する場合は別 issue で `ParsedTables` を抜本的に再設計する前提とし、本 issue では `Os2Table` を `tables.rs` モジュール内 private 型として小さく保つ。`Os2Table` を公開構造体に昇格させることは行わない。

### Option 設計

- `Font::line_gap()` の戻り値は `f64` (常に取得可能、`parse_all` で hhea を `dir.require(TAG_HHEA)?` で必須化しているため、`FontFace::from_data` 成功時点で値が保証される)
- `Font::cap_height()` / `Font::x_height()` は `Option<f64>` (OS/2 不在 or version < 2 で `None`)
- フィールド値が 0 でも `Some(0)` を返す (OpenType 仕様で 0 = 未定義の慣例は明文化されておらず、記号フォントで実際に 0 のケースがあるため)
- OS/2 v2+ で `rec.length < 90` (sCapHeight offset 88 + sizeof(i16) = 90) の場合は寛容方針に揃え、`cap_height = None` / `x_height = None` で続行する (`InvalidData` で `FontFace::from_data` を失敗させない)

### `Font` のフィールドアクセス経路

`Font.face: Arc<FontFaceInner>` (`mod.rs:101`) で、`FontFaceInner` (`mod.rs:107-110`) が `tables: ParsedTables` を保持する中継構造体。`self.face.tables.<field>` は Arc の auto-deref で `FontFaceInner.tables` を指す。既存 `Font::ascent()` / `Font::descent()` (`mod.rs:170-178`) と同じパターン。

## 完了条件

### 追加される API

- `Font::line_gap() -> f64`
- `FontFace::cap_height() -> Option<i16>`
- `FontFace::x_height() -> Option<i16>`
- `Font::cap_height() -> Option<f64>`
- `Font::x_height() -> Option<f64>`

### テスト

- 単体テストで Arial 環境での Some 側挙動を検証 (Arial は OS/2 v4 で cap_height / x_height が定義済み)
- PBT で「スケール済み値 = デザイン値 × scale」の不変条件を size をランダム化して検証
- OS/2 不在フォントでの `None` 検証は本 issue では実施しない (Arial 依存環境では検証不可能、0001 メタ issue「テスト用フォント選定」tracked で OS/2 不在フォントが選定されてから別 PR で追加)
- `size = 0 / 負値 / NaN / Inf` の挙動はテストでロックしない (`size <= 0` を拒否する設計変更が後発 issue として起票予定のため、現状を固定すると後発 issue の妨げになる)

### ドキュメント

- `docs/BLEND2D.md` L418 / L443 の raden 列に新規 API 名を追記
- `CHANGES.md` の `## develop` の `### misc` 直下に `ADD` 種別で 5 件を追記 (`shiguredo-changelog` スキル準拠)

### サブタスク

- `pbt/tests/prop_font/main.rs` を新規作成
- `pbt/Cargo.toml` に `[[test]]` エントリを追加 (`name = "prop_font"`, `path = "tests/prop_font/main.rs"`)

## 解決方法

### 1. OS/2 テーブルパースの追加 (`src/font/tables.rs`)

- `tables.rs:112-118` の TAG_* 群に `const TAG_OS2: u32 = tag(b"OS/2");` を追加 (4 文字 ASCII = `0x4F532F32`。タグの 4 文字目は数字 `2` で末尾スペースは含まない)
- private 構造体を新規定義:

  ```rust
  struct Os2Table {
      cap_height: Option<i16>,
      x_height: Option<i16>,
  }
  ```

  `pub` も `pub(crate)` も付けず、`Clone` / `Default` も derive しない (使い捨て一時型のため)

- `parse_os2` を新規作成 (`parse_head` / `parse_hhea` のシグネチャに揃える):

  ```rust
  fn parse_os2(data: &[u8], rec: TableRecord) -> Result<Os2Table, FontError> {
      let off = rec.offset as usize;
      // OS/2 version 0 の最小サイズ (Microsoft 拡張形式) は 78 バイト。
      // Apple 旧形式 68 バイトは本実装でエラーとする (現代の OS バンドルフォントには存在しない)。
      if rec.length < 78 {
          return Err(FontError::InvalidData("OS/2 table too short"));
      }
      let version = read_u16(data, off)?;
      if version < 2 {
          // v0 / v1 では sCapHeight / sxHeight が存在しない
          return Ok(Os2Table { cap_height: None, x_height: None });
      }
      // v2+ で sxHeight (offset 86) + sCapHeight (offset 88) を読むには 90 バイト必要。
      // 不足時は寛容方針 (FontFace::from_data 自体は成功させる) で None を返す。
      if rec.length < 90 {
          return Ok(Os2Table { cap_height: None, x_height: None });
      }
      let x_height = Some(read_i16(data, off + 86)?);
      let cap_height = Some(read_i16(data, off + 88)?);
      Ok(Os2Table { cap_height, x_height })
  }
  ```

  OpenType v6 まで sxHeight / sCapHeight の offset は変わっていない (仕様確認済み)。将来 v7 以上が出ても v2+ と同等扱いとする (offset 不変前提のフォワード互換)

- `parse_all` (`tables.rs:582-619`) で `dir.find(TAG_OS2)` を使い、戻り値の構造体リテラル末尾に新規フィールドを追加:

  ```rust
  let os2 = match dir.find(TAG_OS2) {
      Some(rec) => parse_os2(data, rec)?,
      None => Os2Table { cap_height: None, x_height: None },
  };
  // ...
  Ok(ParsedTables {
      // ... 既存フィールド ...
      cap_height: os2.cap_height,
      x_height: os2.x_height,
  })
  ```

- `ParsedTables` (`tables.rs:539-556`) に `pub cap_height: Option<i16>` と `pub x_height: Option<i16>` を直結追加 (`hmtx: HmtxTable` のような構造体保持ではなく、`ascent` / `descent` の直結パターンに揃える)

### 2. `FontFace` メソッド追加 (`src/font/mod.rs`)

```rust
impl FontFace {
    /// OS/2 テーブル v2 以上の sCapHeight (デザインユニット)。
    /// OS/2 不在 / v < 2 / v2+ かつテーブル長 < 90 のいずれかで None。
    pub fn cap_height(&self) -> Option<i16> {
        self.tables.cap_height
    }

    /// OS/2 テーブル v2 以上の sxHeight (デザインユニット)。
    /// None の条件は cap_height と同じ。
    pub fn x_height(&self) -> Option<i16> {
        self.tables.x_height
    }
}
```

### 3. `Font` メソッド追加 (`src/font/mod.rs`)

```rust
impl Font {
    /// 行間 (ピクセル単位)。hhea 必須テーブル由来のため常に値を返す。
    pub fn line_gap(&self) -> f64 {
        self.face.tables.line_gap as f64 * self.scale
    }

    /// cap_height のスケール済み値 (ピクセル単位)。
    /// FontFace::cap_height() が None ならば None を返す。
    pub fn cap_height(&self) -> Option<f64> {
        self.face.tables.cap_height.map(|v| v as f64 * self.scale)
    }

    /// x_height のスケール済み値 (ピクセル単位)。
    /// FontFace::x_height() が None ならば None を返す。
    pub fn x_height(&self) -> Option<f64> {
        self.face.tables.x_height.map(|v| v as f64 * self.scale)
    }
}
```

### 4. `pbt/tests/prop_font/main.rs` 初回作成と `pbt/Cargo.toml` 編集

- `pbt/tests/prop_font/main.rs` を新規作成 (`pbt/README.md` のディレクトリモジュール命名規則)。0022 と方針共有: 本 issue 着手時点では `main.rs` 直書きで進め、サブモジュール分割は 0025 / 0026 着手時にテスト本数が増えた段階で導入する (その際、本 issue で書いた既存テストもサブモジュールに移動する)
- `pbt/Cargo.toml` に以下を追加 (Cargo はディレクトリ配下の `main.rs` を自動認識しないため明示指定が必須。Cargo Book「Target auto-discovery」参照):

  ```toml
  [[test]]
  name = "prop_font"
  path = "tests/prop_font/main.rs"
  ```

- Arial 不在時スキップ用ヘルパー (`load_arial` 相当) を `prop_font/main.rs` 冒頭で再定義 (`tests/test_font.rs:5-13` の `load_arial()` は別クレートのため再利用不可)。`proptest!` の外で `let Some(face) = load_arial() else { return; };` のガードを置く (`prop_assume!` は proptest が低品質扱いするため避ける)
- 0001 メタ issue で「並行ブロックで先着が作成、0021 が原則担当」と確定。0022 / 0023 が先着の場合は当該担当が作成し 0021 がマージする。`[[test]]` エントリ追加とヘルパー再定義は先着が行う

### 5. 単体テスト追加 (`tests/test_font.rs`)

PBT で実現可能な不変条件は単体テストに書かない (`shiguredo-rust` スキル「PBT 優先」)。本 issue で単体テストに残すのは「Arial 環境での既知特性の存在確認」のみ:

- `font_face_cap_x_height`: Arial.ttf (OS/2 v4) で `face.cap_height().is_some()` かつ `face.x_height().is_some()` を検証 (OS/2 不在フォントが選定されたら None ケースを追加)

スケール線形性 / 値の正値性は PBT (次節) で検証する。

### 6. PBT 追加 (`pbt/tests/prop_font/main.rs`)

既存 `prop_path.rs` の Strategy 範囲 (`r in 0.001f64..1000.0`) と許容誤差 (`1e-9` 級) に揃える。Arial 固定で size のみランダム化する。

検証する不変条件:

- **`line_gap` スケール線形性**: `size in 0.001f64..1000.0` で `(font.line_gap() - face.line_gap() as f64 * font.scale()).abs() < 1e-9 + (font.line_gap().abs() + face.line_gap() as f64 * font.scale().abs()) * 1e-12`
- **`cap_height` スケール線形性 (Some 側のみ)**: `face.cap_height()` が `Some(v)` のとき `(font.cap_height().unwrap() - v as f64 * font.scale()).abs() < 1e-9` (Arial では常に Some なので Some 側のみ。None 側は OS/2 不在フォント選定後に別 PR で追加)
- **`x_height` スケール線形性**: 上と同様
- **非負性 (Arial 限定)**: `font.line_gap() >= 0.0` かつ `font.cap_height().unwrap() > 0.0` かつ `font.x_height().unwrap() > 0.0`

`size = 0 / 負値 / NaN / Inf` は Strategy 範囲外 (`0.001..1000.0` で `is_finite() && size > 0.0` が自然に保証される)。後発 issue で `Font::from_face` の `size <= 0` 拒否が決まれば本 PBT の Strategy 上限・下限は変わらないため影響なし。

### 7. CHANGES.md 更新

`CHANGES.md` の `## develop` の `### misc` 直下に以下を追加 (`shiguredo-changelog` スキル準拠。`@<author>` は実装者の GitHub ユーザー名 (例: `@voluntas`) で置換する):

```
- [ADD] `Font::line_gap` を追加する
  - @<author>
- [ADD] `FontFace::cap_height` を追加する
  - @<author>
- [ADD] `FontFace::x_height` を追加する
  - @<author>
- [ADD] `Font::cap_height` を追加する
  - @<author>
- [ADD] `Font::x_height` を追加する
  - @<author>
```

順序: FontFace 系を先、Font 系を後 (API 依存順)。

### 8. BLEND2D.md 更新

- `docs/BLEND2D.md` L418 の raden 列に `cap_height() / x_height()` (i16 個別メソッド) を追記
- `docs/BLEND2D.md` L443 の raden 列に `line_gap() / cap_height() / x_height()` 追記

状態列 (`差異あり: ...`) は本 issue では変更しない。0001 メタ issue の close PR でまとめて整理する。

## 解決方法の補足

- `pbt/tests/prop_font/main.rs` と `pbt/Cargo.toml` の `[[test]]` エントリは 0022 が先着して作成済みだったため、本 issue ではそれらに 0021 用の PBT を追加した形となった。
- `src/font/tables.rs` 内に `parse_os2` の単体テストを追加し、v0 / v1 / v2+ かつ長さ 88 / 90 / 76 の各分岐を検証した。
- `tests/test_font.rs` の既存アサートメッセージを日本語に修正した（新規テスト追加に伴う影響範囲）。
- issue 本文の close 前提にあった「fuzzing 基盤が完了し、OS/2 パーサに対する fuzz target を追加する」は、ユーザー判断で不要となったため未実施。後続 issue で対応する場合がある。
- OS/2 テーブル 68-77 バイトの扱いは issue 設計通り `FontError::InvalidData` とした。

## issue 化候補

`/review-diff-code` ループで指摘されたが、本 issue の完了条件を超えるため別 issue 化を検討する項目:

- OS/2 テーブル不在 / v0 / v1 / 壊れた OS/2 フォントに対する統合テスト（`parse_all` 経由の `None` / `Err` 検証）
- OS/2 テーブル 68-77 バイトのフォントを `FontFace::from_data` で読み込めるようにするかどうかの再検討（後方互換影響あり）
- `Font::line_gap` / `cap_height` / `x_height` の固定サイズ統合テスト（PBT との重複を避けるため慎重に設計）

## 変更対象ファイル

| ファイル | 種別 | 内容 |
|---|---|---|
| `src/font/tables.rs` | 既存編集 | `TAG_OS2` 定数、`Os2Table` private 構造体、`parse_os2`、`ParsedTables` への `cap_height` / `x_height` 直結追加、`parse_all` への組み込み |
| `src/font/mod.rs` | 既存編集 | `FontFace::cap_height()` / `x_height()`、`Font::line_gap()` / `cap_height()` / `x_height()` の追加 |
| `tests/test_font.rs` | 既存編集 | 単体テスト 1 件追加 (`font_face_cap_x_height`) |
| `pbt/tests/prop_font/main.rs` | 新規 | PBT 4 件 (スケール線形性 3 + 非負性 1) |
| `pbt/Cargo.toml` | 既存編集 | `[[test]]` エントリ追加 |
| `docs/BLEND2D.md` | 既存編集 | L418 / L443 の raden 列に新規 API 追記 |
| `CHANGES.md` | 既存編集 | `### misc` 直下に `ADD` 5 件追加 |
| `src/lib.rs` | 変更不要 | 既存 `pub use font::{Font, FontData, FontError, FontFace};` でメソッド追加のみのため変更不要。本 issue で新規型は導入しない |

## エッジケース

- OS/2 テーブル不在 / OS/2 version < 2: `cap_height()` / `x_height()` は `None`
- OS/2 v2+ で `rec.length < 90` (壊れた OS/2): 寛容方針で `None` を返す (`InvalidData` で失敗させない)
- OS/2 v2+ で `rec.length < 78` (壊れた OS/2 v0 相当の最小サイズも満たさない): `FontError::InvalidData` (`FontFace::from_data` 全体が失敗)
- OS/2 フィールド値が 0 の場合: `Some(0)` を返す (0 を未定義扱いしない)
- `size = 0` で `scale = 0` の場合: `cap_height() == Some(0.0)` / `line_gap() == 0.0` 等を返すが、テストでロックしない (後発 issue で `size <= 0` 拒否が決まる可能性)
- `size < 0` / `size.is_nan()` / `size.is_infinite()`: IEEE754 演算結果がそのまま伝播。テストでロックしない

## 隣接 issue への影響

- **0026 への申し送り**: 0001 メタ issue の段階拡張表で `Font::from_face` シグネチャが 0026 で拡張される (`FontFeatureSettings` 対応)。本 issue で追加する単体テスト・PBT の `Font::from_face(&face, size)` 呼び出し箇所は 0026 で書き換えが必要。**この書き換え責務は 0026 のスコープに含む** (0026 polish 時に 0026 の解決方法に明記)
- **0022 との PBT ファイル共有**: 並行ブロック内で先着が `pbt/tests/prop_font/main.rs` を作成。サブモジュール分割は 0025 / 0026 着手時に実施し、既存テストもそのタイミングで再配置
- **0023 との API 命名差異**: 0023 の `FontFace::glyph_bounds()` はデザインユニットを `Option<GlyphBounds { f64 }>` で返す設計だが、本 issue は `Option<i16>` で返す。既存 `FontFace::ascent() -> i16` 系との整合を優先。0023 polish 時に `f64` ↔ `i16` の方針差異を再検討すべき

## 関連

- `0001-enhance-font-module-maturity.md` (メタ issue。本 issue の Blend2D 対応行と `pbt/tests/prop_font/main.rs` 初回作成サブタスクが確定)
- 並行可能 (依存なし): `0022-add-text-measurement.md`、`0023-add-glyph-bounds.md`
- 依存される: 0026 (本 issue の単体テスト / PBT が `Font::from_face` シグネチャ拡張で書き換え対象)
- **着手前提**: 0001 メタ issue の前提 concrete issue 群のうち「テスト用フォント選定」(Arial 依存テストパターンを踏襲するなら緩い前提)、「Compound Glyph point-matching 実装」が完了
- **close 前提**: 上記に加え「fuzzing 基盤」が完了し、本 issue で OS/2 パーサに対する fuzz target を追加する
