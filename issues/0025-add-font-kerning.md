# カーニング適用機能を追加する

- Priority: Medium
- Category: add
- Created: 2026-05-11
- Model: Kimi K2.7 Code
- Branch: feature/add-font-kerning
- Polished: 2026-06-21

## 目的

グリフペア間のカーニングを適用し、プロポーショナルフォント (例: `(A, V)` / `(T, o)` / `(W, A)`) でのテキスト描画品質を向上させる。Microsoft OpenType `kern` テーブル v0 (Format 0 / horizontal) を新規パースし、`fill_text` / `measure_text` / `stroke_text` の 3 経路でカーニング量を `advance` に加算する。

本 issue は `add` カテゴリ (API 追加が主目的) として扱うが、`Font::measure_text` および `Context::fill_text` / `Context::stroke_text` の値・描画結果に意味的変化を生じ、また `Font::measure_text` の結合性 (隣接 advance の和) 保証を取り下げるため、`CHANGES.md` には `CHANGE` 種別も併記する。これは 0001 メタ issue「font 公開 API の安定化方針」で「font モジュールの公開 API は本 issue close までは未安定とみなし、0021-0027 で破壊的変更を許容する」「破壊的変更 (戻り値の意味変化等) は `CHANGES.md` の `## develop` トップ階層に `CHANGE` 種別で記載する」と確定済みのため、`add` カテゴリ concrete issue 内で `CHANGE` 種別を併発する設計が許容される。同方針により `Category: change` の別 issue は起票しない (0026 も同じ設計で 0001 由来)。

## Blend2D との対応

本 issue は `docs/BLEND2D.md` の以下の行を解消する (0001 メタ issue「Blend2D との対応表」で確定済み)。

- **L449 `BLFont::apply_kerning(BLGlyphBuffer&)`**: raden は `FontFace::kern(u16, u16) -> i16` / `Font::kern(u16, u16) -> f64` の個別取得 API と、`fill_text` / `measure_text` / `stroke_text` 内での自動適用で対応。raden は `BLGlyphBuffer` 相当 (`GlyphBuffer`) を 0026 で初出するため、本 issue 時点では一括適用 API を持たず、隣接ペア走査で `advance` に加算する形で対応する

申し送り:

- 本 issue ではカーニングは無条件で適用する。`FontFeatureSettings` (`kern` feature の on/off 制御) は 0026 のスコープ
- 0026 で `Font::shape()` が GPOS Pair Adjustment (kern テーブルの後継) を実装した時点で、本 issue の `kern` テーブル使用経路は GPOS 不在フォントへのフォールバックとして残る (0026 polish 済で確定)

完了 PR で `docs/BLEND2D.md` L449 の raden 列に新規 API 名 (`Font::kern()` / `FontFace::kern()`) を追記する。状態列 (`未実装: カーニング適用`) は本 issue では変更せず、0001 メタ issue の close PR でまとめて整理する。

## 現状

- `measure_text()` (0022) も `fill_text()` (0022 でリファクタ済) も `stroke_text()` (0024 で追加予定) もカーニング非適用
- `Font::glyph_run_for_text` (0022 で追加された `pub(crate)`) の出力バッファ `&mut Vec<(u16, f64)>` の `f64` 要素は「カーニング非適用の単体 advance」(0022 設計方針で確定)
- `kern` テーブルは未パース。`src/font/tables.rs` に TAG 定数も `parse_kern` 関数もない
- GPOS テーブルは未パースまたは未利用 (0026 で対応予定)

## 設計方針

### Microsoft OpenType `kern` テーブル v0 (Format 0) を採用

Microsoft OpenType `kern` v0 仕様を採用する。Apple `kern` v1 (`version == 0x00010000`、ヘッダ・subtable レイアウトが Microsoft 版と異なる) は **テーブル全体をスキップ** して空 `KernTable` を返す。

レイアウト:

- **kern テーブルヘッダ (4 バイト)**: `version: uint16` (offset 0)、`n_tables: uint16` (offset 2)
- **subtable ヘッダ (6 バイト)**: `sub_version: uint16` (Microsoft v0 では 0) / `length: uint16` / `coverage: uint16`。最初の subtable は `rec.offset + 4` から始まる
- **subtable 全体長**: subtable ヘッダの `length` フィールドは **subtable 全体の長さ (ヘッダ 6 + body 含む)**。次の subtable は `current_offset + length` で求める
- **Format 0 body (8 + n_pairs * 6 バイト)**: `n_pairs: uint16` / `search_range: uint16` / `entry_selector: uint16` / `range_shift: uint16` の後に `n_pairs` 個の `(left: uint16, right: uint16, value: int16)` ペア配列。raden は線形探索のため `search_range` / `entry_selector` / `range_shift` は読み捨てる

coverage フィールド (u16) のビット配置 (Microsoft kern v0、bit 番号は u16 全体に対する位置):

- bit 0: horizontal (1 ならば水平 kerning)
- bit 1: minimum (set されていれば不採用)
- bit 2: cross-stream (set されていれば不採用)
- bit 3: override (set されていれば不採用)
- bits 4-7: reserved (本 issue では reserved 非ゼロでも採用する。将来拡張への耐性および HarfBuzz / FreeType の慣行に合わせる)
- bits 8-15: format (0 ならば Format 0)

採用条件:

- **最初に Format 0 / horizontal / `sub_version == 0` の全条件を満たし、かつ `n_pairs > 0` でペア配列を持つ subtable のペアのみ採用する** (Microsoft 仕様は複数 subtable の累積適用も許容するが、本 issue では単純化のため最初の 1 つに限定。`n_pairs == 0` の空 subtable で「採用済み」とすると後続の有効ペアを取りこぼすため `n_pairs > 0` 必須。HarfBuzz / FreeType も同じ慣行。0026 で GPOS Pair Adjustment 経路に統合されるため本 issue 限定の妥協)
- 採用条件を満たさない subtable は不採用としてスキップ (`current_offset += length` で次の subtable へ)
- グリフペアの検索は線形探索 (`n_pairs` 件の `(left, right)` を順次比較)

### API 設計

- `FontFace::kern(glyph_id1: u16, glyph_id2: u16) -> i16` (デザインユニット、テーブル不在時 / ペア未登録時は 0)
- `Font::kern(glyph_id1: u16, glyph_id2: u16) -> f64` (スケール済みピクセル、同じく 0 フォールバック)

`glyph_id == 0` (.notdef) を含むペアもカーニング探索の対象とする (ペア未登録時の 0 フォールバックで自然に処理される)。

`FontFace::kern` と `Font::kern` の探索ロジックは共通化するため、`KernTable` に `pub(crate) fn lookup(&self, left: u16, right: u16) -> Option<i16>` ヘルパーを定義し、両者から呼び出す (重複排除)。

### `glyph_run_for_text` の不変性維持

0022 で追加された `Font::glyph_run_for_text(&self, text: &str, buf: &mut Vec<(u16, f64)>)` (`pub(crate)`) の出力バッファ要素型 `(u16, f64)` は本 issue でも変更しない。`f64` は「カーニング非適用の単体 advance」のまま保ち、docstring (`/Users/voluntas/shiguredo/raden/src/font/mod.rs:180-190`) も維持する。

カーニング適用は **呼び出し側 (`Font::measure_text` / `Context::fill_text` / `Context::stroke_text`) の責務** とし、`glyph_run_for_text` の出力に対して隣接ペア `(glyph_id[i], glyph_id[i+1])` に対して `Font::kern(glyph_id[i], glyph_id[i+1])` を計算し、`advance` 累積に加算する。

実装内 (`measure_text` / `fill_text` / `stroke_text`) のカーニング加算は `Font::kern` (戻り値 `f64`) を使う。`FontFace::kern` (戻り値 `i16`) は公開 API 用で、内部加算には使わない。

### 0022 / 0023 / 0024 PBT・テストへの影響

- 0022 で追加された `pbt/tests/prop_font/main.rs:51-67` の `prop_concatenation` 関数本体および `pbt/tests/prop_font/main.rs:107-110` の `#[test] fn concatenation()` は、本 issue で `measure_text(a + b)` の境界に kern が適用されるため不変条件が破綻する。両方を本 issue で削除する
- 0022 で追加された `pbt/tests/prop_font/main.rs:91-100` の `prop_non_negative` 関数本体および `pbt/tests/prop_font/main.rs:117-120` の `#[test] fn non_negative()` も本 issue で削除する (kern 適用後の advance 総和の非負性は使用フォントの kern 値域に依存し、proptest で生成された文字列で偶発的に負値となるケースを許容しないため)
- 0026 で `Font::shape()` 経路に対する結合性・非負性類似 PBT を再導入するか否かは 0026 polish で確定する (本 issue は 0026 の判断を拘束しない)
- 0024 で追加された `tests/test_context.rs` の `context_stroke_text_matches_manual` の手動再実装ロジックを本 issue でカーニング適用済みに揃える

### 値の意味的変化と CHANGE 種別

- `Font::measure_text` の `TextMetrics.advance` の値の意味が「カーニング非適用の単体 advance 総和」から「Microsoft OpenType `kern` v0 適用済み」に変化する (`CHANGE` 種別)
- `Font::measure_text` の結合性 `measure_text(a + b).advance == measure_text(a).advance + measure_text(b).advance` の不変条件を本 issue で取り下げる (隣接ペアにカーニングが適用されるため公開 API としては保証しない。`CHANGE` 種別)
- `Context::fill_text` / `Context::stroke_text` の描画結果のグリフ間隔がカーニング適用後の advance に変わる (`(A, V)` 等の典型ペアで差分テスト `tolerance = 0` で確実に fail するため `CHANGE` 種別)

0001 メタ issue「font 公開 API の安定化方針」セクションで「戻り値の意味変化・描画結果の差は `CHANGES.md` の `## develop` のトップ階層に `CHANGE` 種別で記載する」と確定済みの規則に従う。

## 完了条件

### 追加される API

- `FontFace::kern(glyph_id1: u16, glyph_id2: u16) -> i16` (デザインユニット)
- `Font::kern(glyph_id1: u16, glyph_id2: u16) -> f64` (スケール済みピクセル)

### 既存 API の挙動変化

- `Font::measure_text` の `TextMetrics.advance` の意味が「カーニング非適用」から「`kern` テーブル適用済み」に変わる (`CHANGE` 種別)
- `Font::measure_text` の結合性保証を取り下げる (`CHANGE` 種別)
- `Context::fill_text` の描画結果のグリフ間隔がカーニング適用後の advance に変わる (`CHANGE` 種別)
- `Context::stroke_text` の描画結果のグリフ間隔がカーニング適用後の advance に変わる (`CHANGE` 種別、0024 で追加された `stroke_text` への後追い改修を本 issue のスコープに含む)

### docstring 更新

- `Font::measure_text` の docstring (`/Users/voluntas/shiguredo/raden/src/font/mod.rs:200-208`) のうち、`mod.rs:203` の「カーニングは適用されない (`kern` テーブルは未対応)。」の **1 文のみ** を「Microsoft OpenType `kern` v0 (Format 0 / horizontal) によるカーニングを隣接グリフペアに適用した advance を返す。詳細は `Font::kern` 参照。」に置換する。`mod.rs:203` 行頭の「総アドバンスを格納する。」および空行前後 (`mod.rs:200-202`、`mod.rs:204-208`) は維持する
- `glyph_run_for_text` の docstring (`/Users/voluntas/shiguredo/raden/src/font/mod.rs:180-190`) は現状のまま維持

### 着手前提

- 0022 が close 済
- 0024 が close 済 (`stroke_text` への適用は 0024 で `stroke_text` が存在する状態でないと完了できないため、必ず 0024 close 後に着手する。0001 メタ issue「依存関係」セクションでは「0025 は `stroke_text` 以外について 0024 と並行着手可能」とあるが、本 issue は 3 経路を 1 PR で同時改修する方針のため 0024 close 済を厳密な着手前提とする。本 issue close PR で 0001 メタ issue 該当行も「0024 → 0025 直列」に同期更新する責務を持つ)
- 0001 メタ issue の前提 concrete issue 群 (fuzzing 基盤 / ベースライン benchmark 計測基盤 / Compound Glyph point-matching 実装) が close 済
- 「テスト用フォント選定」tracked の段階的選定 (0001 メタ issue) のうち、**kern 付きフォントの追加選定が完了し配置場所が決定済** (tracked 全体の close は 0026 / 0027 のフォント追加にも依存するため本 issue 着手前提には含めない。kern 段階のみ完了していれば足りる)。kern テーブル付きフォントの配置場所 (`tests/fixtures/` 等) は当該追加選定で確定する

### 開発初期テスト未カバーリスクの許容

kern 付きフォントが未配置のまま本 issue を着手すると単体テスト 3 件 (`font_kern_known_pair` / `font_kern_no_pair` / `font_measure_text_with_kerning`) が全件スキップされ、PBT「隣接ペア加算の関係」も `Font::kern == 0` のケース (kern 不在環境) で `measure_text == 単体 advance 総和` に縮退し、`parse_kern` ロジックの正しさが `parse_all` 経由の fuzz target (クラッシュ耐性のみ) しか検証されない状態になる。

本 issue ではこのリスクを許容しない方針として、着手前提 (kern 付きフォントの追加選定完了) を厳密に守る。開発手順上、本 issue 着手の最初の段階で kern 付きフォントの配置を確認すること。配置がまだなら「テスト用フォント選定 tracked」の kern 段階完了を先に進める。

### close 前提

- 上記着手前提を満たし、本 issue 内で `fill_text` / `measure_text` / `stroke_text` 3 経路へのカーニング適用、0022 PBT 削除、0024 テスト更新が完了し、CI でテストがパス

### ドキュメント

- `docs/BLEND2D.md` L449 の raden 列に新規 API 名を追記 (状態列は本 issue で変更しない)
- `CHANGES.md` に `CHANGE` 4 件・`ADD` 2 件を追加 (詳細は解決方法 7 参照)

### Fuzzing

不正な `kern` テーブル (length 不正、coverage 不正、n_pairs オーバーフロー等) に対するクラッシュ耐性は、前提 concrete issue「fuzzing 基盤と既存コード fuzz target」で追加された `parse_all` 経由の fuzz target で自動的にカバーされる (`parse_all` 内から `parse_kern` が呼ばれるため、フォントファイル全体のバイト列を入力とする target に kern バイトも含まれる)。新規 fuzz target は本 issue では追加しない。

## 解決方法

### 1. `kern` テーブルパースの追加 (`src/font/tables.rs`)

#### 1-1. TAG 定数と構造体定義

`tables.rs:112-118` の TAG_* 群に `const TAG_KERN: u32 = tag(b"kern");` を追加する。

構造体を新規定義 (フィールド可視性は既存 `HmtxTable` 等のパターンに揃える):

```rust
#[derive(Clone)]
pub(crate) struct KernPair {
    pub left: u16,
    pub right: u16,
    pub value: i16,
}

#[derive(Clone, Default)]
pub(crate) struct KernTable {
    pub pairs: Vec<KernPair>,
}

impl KernTable {
    /// グリフペアを線形探索する。ペア未登録時は None。
    pub(crate) fn lookup(&self, left: u16, right: u16) -> Option<i16> {
        self.pairs
            .iter()
            .find(|p| p.left == left && p.right == right)
            .map(|p| p.value)
    }
}
```

`KernTable::Default` は `dir.find(TAG_KERN)` が `None` (kern テーブル不在) のとき `unwrap_or_default()` で空テーブルを得るために必要。`HmtxTable` 等の必須テーブル (`dir.require`) と異なり kern は欠落許容のため Default 派生が必要。`KernPair` 側は `Default` 不要 (`Vec::<KernPair>::default()` は要素型の `Default` を要求しない)。

`Clone` 派生は `ParsedTables` 全体 (`tables.rs:542 #[derive(Clone)]`) に組み込むため必須。

#### 1-2. `parse_kern` の実装

`parse_kern(data: &[u8], rec: TableRecord) -> KernTable` を新規作成する。**寛容方針** を採り、kern テーブル本体が不在 / 不正でもフォントロード自体を失敗させない (kern は描画 (cmap / hmtx / glyf) に必須ではなく、0021 の OS/2 と同じ位置付け)。すべてのエラー経路で `KernTable { pairs }` (`pairs` は途中まで読めた分を保持) を返すため戻り値型は `KernTable` 直返しとし `Result` でラップしない。

オフセット計算はすべて `usize` で行う (`rec.offset` / `rec.length` / `length` / `n_pairs` を `as usize` でキャスト)。範囲判定は `usize` のオーバーフローを避けるため `u64` への一時キャストを用いる (既存 `tables.rs:83-86` の `TableDirectory::parse` パターン)。

以下は擬似コード。**`?` 表記は `read_u16` / `read_i16` の `Result<_, FontError>` を吸収するための略記**。Rust 実装時は `?` を `let Ok(v) = expr else { return KernTable { pairs }; };` パターンに展開する (途中まで読めた分の `pairs` を保持して関数から早期 return する寛容方針):

```text
fn parse_kern(data: &[u8], rec: TableRecord) -> KernTable:
    let mut pairs: Vec<KernPair> = Vec::new()
    let off: usize = rec.offset as usize
    let table_end_u64: u64 = off as u64 + rec.length as u64
    if table_end_u64 > data.len() as u64 or rec.length < 4:
        return KernTable { pairs }
    let table_end: usize = table_end_u64 as usize

    let version: u16 = read_u16(data, off)?
    if version != 0:
        // Apple v1 (0x0001_0000 等) を含む非対応 version はテーブル全体スキップ
        return KernTable { pairs }
    let n_tables: u16 = read_u16(data, off + 2)?
    if n_tables == 0:
        return KernTable { pairs }

    // kern header (version 2 + n_tables 2 = 4 バイト) の直後、data 先頭基準のオフセット
    let mut current_offset: usize = off + 4
    let mut accepted: bool = false

    for _ in 0..(n_tables as usize):
        // 残りデータが subtable ヘッダ 6 バイト未満ならループ打ち切り
        if (current_offset as u64 + 6 > table_end as u64): break
        let sub_version: u16 = read_u16(data, current_offset)?
        let length_u16: u16 = read_u16(data, current_offset + 2)?
        let coverage: u16 = read_u16(data, current_offset + 4)?
        let length: usize = length_u16 as usize
        // length < 6 は subtable header 未満で壊れているとみなしループ打ち切り (length 値も
        // 信頼できないため current_offset += length では次の subtable に到達できない)
        if length < 6 or (current_offset as u64 + length as u64) > table_end as u64: break

        if accepted:
            current_offset += length; continue  // 採用済み、以降スキップ

        let is_format_0 = (coverage & 0xff00) == 0
        let is_horizontal = (coverage & 0x0001) == 0x0001
        let exclusive_flags_clear = (coverage & 0x000e) == 0  // bits 1,2,3
        let sub_v0 = sub_version == 0
        if not (is_format_0 and is_horizontal and exclusive_flags_clear and sub_v0):
            current_offset += length; continue  // 不採用、次の subtable へ

        // Format 0 body の固定部 (8 バイト) を読む
        // length >= 6 + 8 = 14 (subtable header 6 + body 固定部 8) を担保
        if length < 14:
            current_offset += length; continue
        let body_off: usize = current_offset + 6
        let n_pairs: usize = read_u16(data, body_off)? as usize
        // search_range / entry_selector / range_shift (body_off + 2 .. body_off + 8) は読み捨て
        // ペア配列 (n_pairs * 6 バイト) が残りペア領域 (length - 14) に収まるか確認
        // (n_pairs <= u16::MAX = 65535 のため n_pairs * 6 は最大 393210 で usize overflow しない)
        if n_pairs * 6 > length - 14:
            current_offset += length; continue  // body 不正、当該 subtable をスキップ

        // n_pairs == 0 (空 subtable) は accepted = true にせず次の subtable へ
        // (後続の有効ペアを持つ subtable を取りこぼさないため)
        if n_pairs == 0:
            current_offset += length; continue

        let pairs_off: usize = body_off + 8
        for i in 0..n_pairs:
            let entry_off: usize = pairs_off + i * 6
            let left: u16 = read_u16(data, entry_off)?
            let right: u16 = read_u16(data, entry_off + 2)?
            let value: i16 = read_i16(data, entry_off + 4)?
            pairs.push(KernPair { left, right, value })
        accepted = true
        current_offset += length

    return KernTable { pairs }
```

`Vec::with_capacity(n_pairs)` は使わず `Vec::new()` から push する (`shiguredo-rust`「入力バイナリデータをデコードする際には `Vec::with_capacity()` などのメモリを事前に割り当てるメソッドを原則として使用しない」方針)。既存 `tables.rs` 内の `TableDirectory::parse` / `parse_hmtx` / `parse_cmap_format4` / `parse_cmap_format12` は同規約に違反した `Vec::with_capacity` を使っているが、本 issue では撤去せず、別 issue (例: tracked「既存コード PBT」と同時または別の最適化 issue) で扱う。本 issue で追加する `parse_kern` のみ規約準拠で書く。

#### 1-3. `parse_all` への組み込み

`parse_all` (`tables.rs:582-619`) に kern のロードを組み込む。位置は `let cmap = parse_cmap(data, cmap_rec)?;` (`tables.rs:605`) の直後、`Ok(ParsedTables { ... })` 構造体リテラル (`tables.rs:607`) の直前:

```rust
let kern = dir
    .find(TAG_KERN)
    .map(|rec| parse_kern(data, rec))
    .unwrap_or_default();
```

`parse_kern` は `Result` を返さないため `?` 不要。`dir.require` ではなく `dir.find` を用いて kern 不在を許容する。

`ParsedTables` (`tables.rs:543-556`) に `pub kern: KernTable` を `hmtx: HmtxTable` の直後に追加し、合わせて `parse_all` 末尾の `Ok(ParsedTables { ... })` 構造体リテラル (`tables.rs:607-618`) に `kern,` フィールドを `hmtx,` の直後に追加する (フィールド未指定だとコンパイルエラーになるため必須)。

### 2. `FontFace::kern()` 追加 (`src/font/mod.rs`)

```rust
impl FontFace {
    /// グリフペアのカーニング量 (デザインユニット)。
    ///
    /// Microsoft OpenType `kern` テーブル v0 の Format 0 / horizontal subtable のみを参照する。
    /// テーブル不在、Apple v1 形式、サポート外の subtable、ペア未登録のいずれの場合も 0 を返す。
    /// `glyph_id == 0` (.notdef) を含むペアもそのまま検索対象とする。
    pub fn kern(&self, glyph_id1: u16, glyph_id2: u16) -> i16 {
        self.tables.kern.lookup(glyph_id1, glyph_id2).unwrap_or(0)
    }
}
```

### 3. `Font::kern()` 追加 (`src/font/mod.rs`)

既存 `Font::glyph_advance` / `Font::ascent` と同じく `self.face.tables.kern.lookup()` を経由する。

```rust
impl Font {
    /// グリフペアのカーニング量 (スケール済みピクセル単位)。
    ///
    /// `FontFace::kern` を `Font::scale` でスケールした値と等価。
    /// `glyph_id == 0` (.notdef) を含むペアもそのまま検索対象とする。
    /// `size == 0` のとき `scale == 0` で戻り値は `0.0`。`size` が NaN や非有限のときは
    /// IEEE 754 算術の結果がそのまま伝播する。
    pub fn kern(&self, glyph_id1: u16, glyph_id2: u16) -> f64 {
        match self.face.tables.kern.lookup(glyph_id1, glyph_id2) {
            Some(v) => v as f64 * self.scale,
            None => 0.0,
        }
    }
}
```

### 4. `Font::measure_text` の修正 (`src/font/mod.rs`)

`glyph_run_for_text` の出力に対して隣接ペアのカーニング量を加算する。既存実装 (`src/font/mod.rs:209-214`) の変数名 `buf` を踏襲する。`Font` は現状 immutable な設計 (`Sync` 想定、内部可変性 `Cell` / `RefCell` を持たない方針) のため、`Context::fill_text` のような `tmp_glyph_run` 再利用は行わず、毎呼び出しごとに `Vec::new()` を確保する (0022 設計の踏襲。ホットパス化は別 issue で thread-local 等を検討):

```rust
pub fn measure_text(&self, text: &str) -> TextMetrics {
    let mut buf: Vec<(u16, f64)> = Vec::new();
    self.glyph_run_for_text(text, &mut buf);

    let mut advance_total: f64 = 0.0;
    for (i, &(glyph_id, advance)) in buf.iter().enumerate() {
        advance_total += advance;
        // 最後のグリフ (i == buf.len() - 1) は次のグリフがないため kern を加算しない。
        if i + 1 < buf.len() {
            advance_total += self.kern(glyph_id, buf[i + 1].0);
        }
    }
    TextMetrics { advance: advance_total }
}
```

#### 0023 の close 順との関係

0023 (`TextMetrics::bounding_box` フィールド追加) と 0025 は 0001 メタ issue 上は並行ブロック (相互依存なし) として位置付けられているが、実装上は `TextMetrics` 構造体リテラルへのフィールド追加で衝突する。両ケースに対応:

- **0023 が本 issue より先に close する場合**: 0023 で `TextMetrics` に `bounding_box: Option<GlyphBounds>` が追加されているため、上記擬似コードの `TextMetrics { advance: advance_total }` リテラルはコンパイルエラーになる。本 issue では 0023 で導入された `bounding_box` 計算ロジック (`cursor_x += advance` ループでの bbox 積み上げ) に kern 加算を組み込み、`TextMetrics { advance: advance_total, bounding_box: <0023 の計算結果> }` に書き換える。bbox 値もカーニング適用済みに変わるため、CHANGES.md の `CHANGE` エントリにもう 1 件 (`Font::measure_text` の `TextMetrics.bounding_box` がカーニング適用済みの bbox に変わる) を追加する
- **本 issue が 0023 より先に close する場合**: 本 issue は擬似コードのままで OK。0023 polish 時に「本 issue 完了後のカーニング適用済み `measure_text` を前提として `bounding_box` 計算に kern を組み込む責務」を 0023 のスコープに含める旨を 0023 issue 側に追記する責務は、本 issue close PR 内で行う (0023 issue ファイルに 0024 → 0025 と同等の「申し送り (重要)」記述を追加する)

### 5. `Context::fill_text` の修正 (`src/api/context.rs`)

`fill_text` (0022 でリファクタ済、`src/api/context.rs:1148-1172`) は次のように書き換える:

```rust
pub fn fill_text(&mut self, x: f64, y: f64, font: &Font, text: &str) {
    let mut path = std::mem::take(&mut self.tmp_path);
    let mut run = std::mem::take(&mut self.tmp_glyph_run);
    path.clear();

    font.glyph_run_for_text(text, &mut run);

    let mut cursor_x = x;
    for (i, &(glyph_id, advance)) in run.iter().enumerate() {
        // glyph_id == 0 はアウトラインを構築せず advance のみ加算する。
        // append_glyph_outline のエラーは let _ で黙殺し、外部入力に対する
        // クラッシュ耐性を優先する (部分欠落を許容)。
        if glyph_id != 0 {
            let _ = font.append_glyph_outline(glyph_id, cursor_x, y, &mut path);
        }
        cursor_x += advance;
        // 最後のグリフは次のグリフがないため kern を加算しない。
        if i + 1 < run.len() {
            cursor_x += font.kern(glyph_id, run[i + 1].0);
        }
    }

    if !path.is_empty() {
        self.fill_path(&path);
    }

    self.tmp_path = path;
    self.tmp_glyph_run = run;
}
```

`tmp_glyph_run` フィールドは 0022 で `Context` に追加済 (`src/api/context.rs:317`)。本 issue では追加しない。`run.iter().enumerate()` と `run[i + 1].0` の併用は両方とも不変借用のため Rust の借用規則上問題ない。

### 6. `Context::stroke_text` の修正 (`src/api/context.rs`)

0024 polish で確定した `stroke_text` の `for &(glyph_id, advance) in run.iter()` ループ (0024 issue 解決方法 step 1 参照) を、本 issue で上記 `fill_text` と同じ `enumerate()` + kern 加算パターンに書き換える。`fill_text` との違いは `self.fill_path(&path)` を `self.stroke_path(&path)` に差し替える 1 行のみ。

0024 単体テスト・PBT (`tests/test_context.rs` の `context_stroke_text_matches_manual`、および `pbt/tests/prop_font/main.rs` の `stroke_text` カーソル前進量一致 PBT があれば) もカーニング適用済みに書き換える (本 issue のスコープに含む)。

### 7. CHANGES.md 更新

`shiguredo-changelog` スキルと 0001 メタ issue「font 公開 API の安定化方針」に従い、`## develop` 直下 (`### misc` の前) に `CHANGE` 種別、`### misc` 直下に `ADD` 種別を追加する。0022 で既に書かれている `### misc` 内の `[ADD]` 2 件 (`TextMetrics` / `Font::measure_text`) は維持する。`@<author>` は実装者の GitHub ユーザー名 (例: `@voluntas`) で置換する。

`## develop` 直下に追加する `CHANGE` エントリ:

```
- [CHANGE] `Font::measure_text` の戻り値 `TextMetrics.advance` を Microsoft OpenType `kern` v0 適用済みの値に変更する (隣接 advance の和との結合性は保証しなくなる)
  - @<author>
- [CHANGE] `Context::fill_text` の描画結果のグリフ間隔を Microsoft OpenType `kern` v0 適用済みに変更する
  - @<author>
- [CHANGE] `Context::stroke_text` の描画結果のグリフ間隔を Microsoft OpenType `kern` v0 適用済みに変更する
  - @<author>
```

0023 が本 issue より先に close している場合は追加で 1 件 (`Font::measure_text` の `TextMetrics.bounding_box` がカーニング適用済みに変わる) を加える。

`## develop` の `### misc` 直下に追加する `ADD` エントリ:

```
- [ADD] `FontFace::kern` を追加する
  - @<author>
- [ADD] `Font::kern` を追加する
  - @<author>
```

### 8. BLEND2D.md 更新

`docs/BLEND2D.md` L449 の raden 列に `FontFace::kern() / Font::kern() (内部で fill_text / measure_text / stroke_text に自動適用)` を追記する。状態列 (`未実装: カーニング適用`) は本 issue では変更しない。

### 9. 0001 メタ issue の依存関係表更新

本 issue close PR 内で `issues/0001-enhance-font-module-maturity.md` の「依存関係」セクション (現状「0025 は `stroke_text` 以外について 0024 と並行着手可能」) を「0024 → 0025 を直列着手 (本 issue は 3 経路 1 PR 改修方針)」に更新する。これは本 issue「着手前提」の決定をメタ issue に反映する責務 (0024 / 0025 のシナリオ揺れを発生させないため)。

## 変更対象ファイル

| ファイル | 種別 | 内容 |
|---|---|---|
| `src/font/tables.rs` | 既存編集 | `TAG_KERN` 定数、`KernPair` / `KernTable` 構造体、`KernTable::lookup` ヘルパー、`parse_kern`、`ParsedTables` への `kern: KernTable` 追加、`parse_all` への組み込みと構造体リテラル更新 |
| `src/font/mod.rs` | 既存編集 | `FontFace::kern()`、`Font::kern()`、`Font::measure_text` 内のカーニング加算、`Font::measure_text` の docstring 1 文置換 |
| `src/api/context.rs` | 既存編集 | `fill_text` / `stroke_text` 内のカーニング加算 (`enumerate()` + 隣接ペア kern 加算) |
| `tests/test_font.rs` | 既存編集 | 単体テスト追加 (kern 付きテストフォントを用いた既知挙動の検証) |
| `tests/test_context.rs` | 既存編集 | 0024 単体テストの手動再実装をカーニング適用済みに揃える |
| `pbt/tests/prop_font/main.rs` | 既存編集 | `prop_concatenation` / `prop_non_negative` 関数本体と `#[test]` 関数の削除、新規 PBT「隣接ペア加算の関係」追加 |
| (kern 付きテストフォント配置先) | 新規 | テスト用フォント選定 tracked で確定した配置場所に追加 (OFL ライセンスファイルの同梱と `Cargo.toml` の `include` / `exclude` 設定の更新も同 tracked のスコープ) |
| `docs/BLEND2D.md` | 既存編集 | L449 の raden 列に新規 API 追記 (状態列は変更しない) |
| `CHANGES.md` | 既存編集 | `## develop` 直下に `CHANGE` 3 件 (0023 先 close の場合 4 件)、`### misc` 直下に `ADD` 2 件 |
| `issues/0001-enhance-font-module-maturity.md` | 既存編集 | 「依存関係」セクションを「0024 → 0025 直列」に更新 (解決方法 9) |
| `src/lib.rs` | 変更不要 | `Font::kern` / `FontFace::kern` はメソッド追加のみ。新規型は導入しない |

## エッジケース

- subtable Format が 0 以外 (Microsoft 仕様で Format 0 と Format 2 のみ定義済み、Format 2 はクラスベースで本 issue 不対応、Format 1 / 3..=255 は仕様予約): 当該 subtable のみスキップ、最終的に `pairs` が空ならカーニング量 0
- subtable Format 0 でも `n_pairs == 0` (空 subtable): 当該 subtable を採用済みとせず次の subtable へ進む (後続の有効ペアを取りこぼさないため)
- subtable `sub_version != 0`: 当該 subtable のみ不採用としてスキップ
- subtable `length < 6` (subtable header に満たない): 以降の subtable パースを打ち切り (length 値自体が壊れている可能性が高く、`current_offset += length` で次の subtable に到達できないため)
- subtable の `length` が範囲外 (`current_offset + length > rec.offset + rec.length`): 以降の subtable パースを打ち切り、それまでに採用した分を保持
- subtable body の必要サイズ `n_pairs * 6 + 14` が subtable 全体長 `length` を超える: 当該 subtable を不正としてスキップ
- 単一文字 / 空文字列: 隣接ペアなし、カーニング加算なし
- `size < 0` / NaN / Inf: テストでロックしない (`Font::from_face` の size 検証は別 issue で扱う方針を 0021 / 0022 と共有)

(その他のエッジケース「kern テーブル不在」「version != 0」「coverage flag set」「グリフペア未登録」「`glyph_id == 0` のペア」「`value == 0` のペア」「`size == 0`」「複数 subtable」「reserved bit set」は設計方針セクションおよび `FontFace::kern` / `Font::kern` の docstring で既に明示済み)

## 隣接 issue への影響

- **0023 への申し送り (重要)**: 本 issue 完了後のカーニング適用済み `Font::measure_text` を前提として、`TextMetrics::bounding_box` 計算 (`cursor_x` 進行ロジック) に kern を組み込む責務を 0023 のスコープに含める。0023 が本 issue より先 close している場合は本 issue 内で `bounding_box` 構築コードに kern を組み込み、CHANGES.md に 1 件追加する (解決方法 4「0023 の close 順との関係」)。0023 ファイル本体への申し送り反映は 0023 polish 時に当該 issue 側で行う責務とし、本 issue で 0023 ファイルを直接編集する責務は持たない (0024 → 0025 と同じ静的記述方式に揃える)
- **0026 への申し送り**: 0026 で GPOS テーブルの Pair Adjustment (Lookup Type 2) を実装し、本 issue の `kern` テーブル使用経路を GPOS Pair Adjustment 経路に統合する。`FontFace::kern` / `Font::kern` API は legacy フォント向けに維持される (0026 polish 済で確定)

## テスト戦略

### 単体テスト (`tests/test_font.rs`)

PBT で実現可能な不変条件は単体テストに書かない方針に従い、kern テーブル付きフォントを用いた既知挙動の確認のみ残す。`load_arial()` ヘルパー (`tests/test_font.rs:5-13`) は既存のまま維持しつつ、kern 付きテストフォント用に `load_kern_test_font()` を新規追加して優先利用する。kern 付きフォントが選定済みでない開発初期は単体テストをスキップする。

```rust
fn load_kern_test_font() -> Option<FontFace> {
    // テスト用フォント選定 tracked で確定した kern 付きフォントを読み込む。
    // 配置パスは tracked 完了時に確定するため、本 issue 着手時点で確認する。
    let path = "<テスト用フォント選定 tracked で確定したパス>";
    if !std::path::Path::new(path).exists() {
        return None;
    }
    let data = FontData::from_file(path).ok()?;
    FontFace::from_data(&data, 0).ok()
}

#[test]
fn font_kern_known_pair() {
    let Some(face) = load_kern_test_font() else { return; };
    let font = Font::from_face(&face, 48.0);
    let glyph_a = font.map_char_to_glyph('A');
    let glyph_v = font.map_char_to_glyph('V');
    // (A, V) ペアはネガティブカーニングが定義されているフォントを前提に選定する。
    // FontFace::kern はデザインユニット (i16) を返す。
    assert!(face.kern(glyph_a, glyph_v) < 0);
    // FontFace::kern を scale 倍した値と Font::kern は同じ演算順序のため bit-exact 一致する。
    let face_kern_scaled = face.kern(glyph_a, glyph_v) as f64 * font.scale();
    let font_kern = font.kern(glyph_a, glyph_v);
    assert_eq!(face_kern_scaled, font_kern);
}

#[test]
fn font_kern_no_pair() {
    let Some(face) = load_kern_test_font() else { return; };
    // 存在しないペア (例: glyph_id u16::MAX) のカーニング量は 0
    assert_eq!(face.kern(u16::MAX, u16::MAX), 0);
}

#[test]
fn font_measure_text_with_kerning() {
    // kern 付きフォントが利用可能なときのみ strict 不等号で kern 適用を保証する。
    // kern 不在環境で `<=` のテストにすると == で成立して kern 実装漏れを検出できないため、
    // kern 付きフォント限定で `<` を要求する。
    let Some(face) = load_kern_test_font() else { return; };
    let font = Font::from_face(&face, 48.0);
    let av_advance = font.measure_text("AV").advance;
    let a_advance = font.measure_text("A").advance;
    let v_advance = font.measure_text("V").advance;
    // (A, V) はネガティブカーニングのため strict 不等号で確認する。
    assert!(av_advance < a_advance + v_advance);
}
```

テストのアサーションメッセージは日本語で書く (`CLAUDE.md` 規約に従う)。

### PBT (`pbt/tests/prop_font/main.rs`)

`pub(crate)` の `glyph_run_for_text` は別クレートである `pbt/` からは見えないため、PBT は `Font::map_char_to_glyph` / `Font::glyph_advance` / `Font::measure_text` / `Font::kern` の公開 API のみで完結させる。`#[cfg(test)]` 越しのテスト用 wrapper は追加しない。

追加する PBT は既存の `pbt/tests/prop_font/main.rs` に追記する (`ascii_printable_string` / `close_enough` は同ファイル内 private fn のため、別ファイル分離はしない):

- **隣接ペア加算の関係**: 任意の ASCII printable 文字列 `text` に対し、`Font::measure_text(text).advance` が `text.chars()` を走査して各文字の `Font::glyph_advance(Font::map_char_to_glyph(c))` を合算し、隣接ペア `(c[i], c[i+1])` の `Font::kern(Font::map_char_to_glyph(c[i]), Font::map_char_to_glyph(c[i+1]))` を加算した値と一致することを 0022 の `close_enough` ヘルパー (相対 `1e-9` + 絶対 `1e-12`) で検証

削除する既存 PBT:

- `prop_concatenation` 関数本体 (`main.rs:51-67`) と `#[test] fn concatenation()` (`main.rs:107-110`) の両方
- `prop_non_negative` 関数本体 (`main.rs:91-100`) と `#[test] fn non_negative()` (`main.rs:117-120`) の両方

維持する既存 PBT:

- `prop_single_char_advance` (単一文字一致、`main.rs:33-43`): 単一文字には隣接ペアが無いためカーニング非適用で、不変条件は維持される
- `prop_size_linearity` (サイズ線形性、`main.rs:70-88`): `Font::glyph_advance` (`aw as f64 * self.scale`) と `Font::kern` (`v as f64 * self.scale`) がいずれも `scale = size / units_per_em` に線形なため `measure_text` の `advance_total` も size に線形。両辺に同じ k がかかるため線形性は維持される

## 関連

- `0001-enhance-font-module-maturity.md` (メタ issue。Blend2D 対応マトリクスで本 issue が担当する L449、`Font::measure_text` の意味的変化、0024 後追い改修の根拠、`FontFeatureSettings` は 0026 で導入の確定)
- 依存: `0022-add-text-measurement.md` (`glyph_run_for_text` の実装、`Font::measure_text` の存在、`tmp_glyph_run` フィールド、PBT 結合性 / 非負性の削除対象)、`0024-add-stroke-text.md` (`stroke_text` への後追い改修対象、関連テストの更新対象)
- 依存される: `0023-add-glyph-bounds.md` (`TextMetrics::bounding_box` 計算への kern 組み込み責務。本 issue close 順に応じて本 issue 内で対応または 0023 へ申し送り)、`0026-add-opentype-basic-shaping.md` (`kern` テーブル使用経路を GPOS Pair Adjustment 経路に統合、`FontFace::kern` / `Font::kern` は legacy フォント向けに維持)
