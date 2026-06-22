# テスト用フォントをリポジトリに同梱する

- Priority: High
- Category: add
- Created: 2026-06-22
- Model: Opus 4.7
- Branch: feature/add-bundled-test-fonts
- Polished: 2026-06-22

## 目的

raden の font 関連テストが macOS 限定 `Arial.ttf` に依存している状況を解消するため、SIL OFL 1.1 ライセンスの **Source Sans 3 Regular** (TTF) と **Source Serif 4 Regular** (OTF / CFF) の 2 ファイルを `tests/fixtures/fonts/` 配下に同梱する。これにより 0001 メタ issue の Tracked items「テスト用フォント選定」を 1 PR で完了させ、0025 / 0026 / 0027 の「kern 付き / GSUB+GPOS 付き / CFF+CFF2 付きフォントのリポジトリ配置」着手前提を一括解消する。

採用フォントが要求テーブル (kern / GSUB / GPOS / CFF) を実際に持つかは本 issue 着手時に `ttx` で実機検証する責務を持つ (検証手順は解決方法 3 参照)。

**auto-resolve エージェントが要件不足を検出した場合の振る舞い**: auto-resolve は本 issue の close を中止して停止しユーザーに報告する (auto-resolve は `gh issue reopen` 等の reopen 操作を実行しない)。reopen と採用フォント表更新は polish 担当またはユーザーが後追いで実施する。本 issue 本文内の「reopen」記述はすべてこの責務分離を前提とする。

## 優先度根拠

High。0025 (カーニング) / 0026 (シェーピング) / 0027 (CFF/CFF2 対応) の 3 つの concrete issue 全ての着手前提であり、本 issue が close されない限り 0025 以降が auto-resolve で進められない。0001 メタ issue 完了条件の前提でもある。

## 現状

- リポジトリ内に `.ttf` / `.otf` ファイルは一切ない (`find . -type f \( -name "*.ttf" -o -name "*.otf" -o -name "*.ttc" \) -not -path "*/target/*" -not -path "*/.git/*"` で 0 件)。`tests/fixtures/` / `tests/common/` / `fonts/` ディレクトリも未作成。
- `.gitattributes` はリポジトリルートに存在しない (新規作成対象)。
- `tests/test_font.rs` の `load_arial()` 関数と Arial 依存テスト 11 件、`tests/test_context.rs` の `mod stroke_text` 内 `load_arial()` 関数と利用箇所 5 件、`pbt/tests/prop_font/main.rs` の `load_arial()` 関数と多数の利用箇所が macOS の `/System/Library/Fonts/Supplemental/Arial.ttf` を直接参照する。Linux / Windows の CI ではすべてスキップされている。具体的な行番号は本 issue 着手時に grep で確認する (0035-0038 の編集で行が動くため本文には固定しない)。
- 現状 raden が実装済みのフォントテーブルは `head` / `maxp` / `hhea` / `hmtx` / `loca` / `cmap` / `glyf` / `OS/2` および TTC のみ。kern / GSUB / GPOS / CFF / CFF2 は **未パース** (`src/font/tables.rs` の `TAG_*` 定数と `parse_all` の `dir.require(TAG_GLYF)` 経路で確認)。
- `Cargo.toml:11` の `include = ["/LICENSE", "/README.md", "/src/**"]` はホワイトリスト方式で `tests/` 配下と `.gitattributes` は `cargo publish` 配布物から自動的に除外される。
- 0001 メタ issue の Tracked items「テスト用フォント選定」が、本 issue で concrete issue 化される。

## 設計方針

### 採用フォント (本 issue で確定)

| 用途 | フォント | 形式 | ライセンス | release タグ |
|---|---|---|---|---|
| TrueType (kern / GSUB liga / GPOS Pair Adjustment 検証用) | Source Sans 3 Regular | TTF | SIL OFL 1.1 | `3.052R` |
| OpenType CFF (CFF アウトライン検証用) | Source Serif 4 Regular | OTF (CFF) | SIL OFL 1.1 | `4.005R` |

Adobe Source ファミリの release タグは「数字のみ (例: `4.005`) と数字 + `R` (例: `4.005R`) の 2 種類」を打つ慣習があり、**`R` 付きが release バイナリを含む正規 release tag**。本 issue は両フォントとも `R` 付きタグを採用する (数字のみのタグは正規 release バイナリを含まない場合があり、`refs/tags/4.005/OTF/SourceSerif4-Regular.otf` が 404 を返す事例を 2 周目レビューで実機確認済み)。

タグ番号は本 issue 着手時に `git ls-remote --tags <repo>` で実際に存在する最新の `R` 付きタグを再確認する。再確認結果が本表と異なる場合は **本 issue を reopen して polish 担当が本表を更新する** (実装着手者は issue 本文を書き換えない。issue 本文と PR description のタグ不整合を避けるため)。

選定理由:

- 両フォントとも SIL OFL 1.1 で配布。Adobe の Source ファミリは Reserved Font Name (`Source`) を持つが、本 issue は **未改変バイナリのみを再配布** するため OFL 1.1 Section 3 の RFN 条項は発動しない (RFN は「改変版に元の名前を残すこと」を禁じる条項)。リネーム・サブセット化は一切行わない。
- Source Sans 3 は GSUB (`liga` feature の `fi` / `fl` リガチャ) と GPOS (Pair Adjustment Lookup) を持つことが想定される。Adobe Source 系は legacy `kern` テーブルを廃して GPOS のみで提供するケースもあるため、`kern` テーブルの存在は **着手時の `ttx` 検証で確認する** (解決方法 3 参照)。`kern` テーブル不在の場合、0025 着手時に別フォント (例: Liberation Sans / DejaVu Sans) の追加同梱を別 fix issue として起票する責務は 0025 の polish に委ねる (本 issue では GPOS Pair Adjustment 検証用として Source Sans 3 を選び、kern fallback は 0025 の polish で再評価する)。
- Source Serif 4 (OTF) は CFF テーブルを持つ。0027 の CFF 着手前提を満たす (CFF2 については本 issue では対応せず、別途 polish で再評価する)。
- 合計サイズの想定値は本 polish 段階では確証できないため、ファイルサイズの予測 / 上限判定は本 issue では行わず、**実装着手時に実測値を PR description に記録する** (解決方法 1 参照)。git LFS は使用しない方針 (約 1 MB 未満を想定するが、実測で大きく超えた場合のみ polish 段階で再選定を検討する)。

### CJK / BMP 外文字カバレッジは本 issue のスコープ外

0026 issue「OpenType 基本シェーピング機能を追加する」の対象はラテン script (`'latn'`) のみで、複雑スクリプト (Arabic / Indic / Hebrew / CJK 縦書き 等) と双方向テキスト (BiDi) は明示的にスコープ外。CJK 統合漢字や BMP 外文字 (Plane 1 以降、U+10000-) のカバレッジを満たすフォントは巨大なため本 issue では要件から撤回する。再評価は font モジュール安定化フェーズの別 issue に委ねる。

### 配置とリポジトリ構成

- **ディレクトリ**: `tests/fixtures/fonts/` を新規作成。本 issue は **ヘルパーモジュール (`tests/common/font.rs`) の作成・新ヘルパー定義を含まず**、フォントバイナリと LICENSE の配置のみを行う。ヘルパー追加と既存 Arial 依存テスト切り替えは 0037 (またはその後続) のスコープ。
- **ファイル一覧** (本 issue で同梱する全ファイル):

```
tests/fixtures/fonts/SourceSans3-Regular.ttf
tests/fixtures/fonts/SourceSans3-LICENSE.txt
tests/fixtures/fonts/SourceSerif4-Regular.otf
tests/fixtures/fonts/SourceSerif4-LICENSE.txt
```

`SourceSans3-LICENSE.txt` / `SourceSerif4-LICENSE.txt` はアップストリームの `LICENSE.md` を **全文未改変** で `.txt` 拡張子に変えて保存する (拡張子変更はファイル名のみで内容は 1 バイトも改変しない)。各フォントごとに同名 prefix を付けるのは、フォントバイナリと LICENSE の対応関係をディレクトリ内で一目で識別するため。`.md` → `.txt` 拡張子変更の理由: `tests/fixtures/fonts/` 配下は markdown レンダリング対象ではなく、ファイル種別を「プレーンテキストの法的文書」として明示する方が誤解が少ない (OFL 1.1 Section 4 の Permission Notice は本文同梱を要求するのみでファイル名規定はないため、拡張子変更は OFL 違反にならない)。LICENSE 内容には Adobe Copyright + Reserved Font Name 'Source' 宣言 + SIL OFL 1.1 本文が含まれる (これらすべてが OFL Section 4 の Permission Notice 要件として必須)。

- **`Cargo.toml`**: 編集しない。現状の `include = ["/LICENSE", "/README.md", "/src/**"]` (`Cargo.toml:11`) はホワイトリスト方式のため `tests/fixtures/fonts/` と `.gitattributes` は `cargo publish` 配布物から自動的に除外される。検証は解決方法 5 で行う。
- **`.gitattributes`**: ルート直下に新規作成し、以下 2 行のみを書く。デフォルト改行設定 (`* text=auto` 等) は **本 issue では追加しない** (リポジトリ全体の改行ポリシー検討は本 issue のスコープを超える)。

```
*.ttf binary
*.otf binary
```

これにより Windows での `core.autocrlf=true` 環境でも .ttf / .otf バイナリが CRLF 変換による破損を受けないことを Git 属性で明示する (Git のデフォルトバイナリ検出ロジックは大抵のケースで動作するが、明示する方が安全)。

### ヘルパー追加と既存 Arial 依存テストの扱い

- **本 issue のスコープ内**: フォントバイナリ + LICENSE の配置と、配置物の存在・読み込み確認テスト 3 件 (後述「テスト追加」)。
- **本 issue のスコープ外**:
  - `tests/common/font.rs` の作成・ヘルパー追加 → 0037 のスコープ。集約方法 (`tests/common/font.rs` 単独か `tests/common/mod.rs` + `pub mod font;` か、`RADEN_TEST_FONT_PATH` 環境変数経由か) は 0037 polish で確定する。本 issue は本 issue のテスト 3 件のみで `concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/fonts/...")` パターンを使い、`load_arial()` 既存テストの絶対パス hardcode スタイルとは独立して書く。
  - 既存 Arial 依存テスト全件 (`tests/test_font.rs` / `tests/test_context.rs` / `pbt/tests/prop_font/main.rs`) の新ヘルパー切り替え → 0001 メタ issue 完了条件「Arial.ttf への依存テストはリポジトリフォントへ切り替えるか、`load_arial()` ヘルパー撤去を別 concrete issue / PR で実施」の別 concrete issue (本 issue 完了後に 0037 と並行で別途扱う)。
  - 0025 / 0026 / 0027 issue 本文内のパスプレースホルダ (`"<テスト用フォント選定 tracked で確定したパス>"` 等) の置換 → 各 issue 着手時に該当 issue の polish (または実装着手時) で確定する。本 issue close PR では編集しない。

短期的には `load_arial()` 経路 (macOS 限定) と本 issue の新規テスト 3 件 (全プラットフォーム) が共存する。

### 0037 との順序

**本 issue を 0037 より先に着手・close する**。理由は優先度根拠 L17 に記載のクリティカルパス上にあるため。0037 polish は本 issue 完了後に走らせ、その時点で本 issue が配置したフォントパスを参照するヘルパーを集約する。0037 polish に対する申し送りは完了条件「0037 への申し送り」参照。

### テスト追加

`tests/test_font.rs` 末尾 (既存 `font_fill_text_integration` の後) に **3 件** の単体テストを追加する。本テストは既存 `load_arial()` を使わず、`include_bytes!` を使わず、`FontData::from_file` を直接呼ぶ。失敗時は `Option::None` 早期 return ではなく `expect` で panic させ、CI で fail として顕在化させる。既存テストは 1 関数 1 関心で書かれているため、新規も同じ流儀に従い、metrics 確認と cmap 確認を別関数に分ける。

`kern` プレフィックスは関数名に入れない (Source Sans 3 が `kern` テーブルを持つかは `ttx` 検証で確定するため、kern を含む命名は誤解を招く)。`truetype` / `cff` のフォーマット種別をプレフィックスに使う。

```rust
/// 同梱した Source Sans 3 Regular のメトリクスが妥当な範囲にあることを確認する。
/// CI 全構成で pass することが 0001 メタ issue の完了条件。
/// `units_per_em` の固定値 assert は採用フォント差し替え時に破綻するため避け、
/// 値非依存の不変条件のみ確認する。
#[test]
fn font_bundled_truetype_test_font_loads() {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/fonts/SourceSans3-Regular.ttf");
    let data = FontData::from_file(path).expect("同梱した TrueType テスト用フォントは必ずロードできるはず");
    let face = FontFace::from_data(&data, 0).expect("Source Sans 3 Regular の parse は必ず成功するはず");
    assert!(face.units_per_em() > 0, "units_per_em は正値である必要がある");
    assert!(face.ascent() > 0, "ascent は正値である必要がある");
    assert!(face.descent() < 0, "descent は負値である必要がある");
}

/// 同梱した Source Sans 3 Regular の cmap に ASCII 'A' が含まれることを確認する。
/// `Font::map_char_to_glyph` は scale 非依存のため `Font::from_face` の size は最小値で良い。
/// raden が対応する cmap subtable format (Format 4 / Format 12) を採用フォントが持つことの
/// スモークテストにもなる。`Path` (`/` 区切り) は Windows でも `std::path::Path` が解釈するため
/// CI matrix `windows-2025` 構成でも動作する。
#[test]
fn font_bundled_truetype_test_font_has_ascii_cmap() {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/fonts/SourceSans3-Regular.ttf");
    let data = FontData::from_file(path).expect("同梱した TrueType テスト用フォントは必ずロードできるはず");
    let face = FontFace::from_data(&data, 0).expect("Source Sans 3 Regular の parse は必ず成功するはず");
    // map_char_to_glyph は scale に依存しないため size は最小値の 1.0 で良い。
    let font = Font::from_face(&face, 1.0);
    assert_ne!(font.map_char_to_glyph('A'), 0, "ASCII 'A' は cmap に含まれる必要がある");
}

/// 同梱した Source Serif 4 Regular のファイルが OpenType + CFF 形式 (sfnt magic = "OTTO")
/// として正しく checkout されていることを確認する。0027 (CFF / CFF2 対応) 未完了の現時点では
/// `FontFace::from_data` は `parse_all` の `dir.require(TAG_GLYF)` で失敗するため、ここでは
/// `FontData::from_file` (ファイル全体読み込み) 成功と sfnt magic 一致のみ確認する。
/// 0027 完了後に本テストを `font_bundled_cff_test_font_loads` に改名し、
/// `FontFace::from_data` 成功と `outline_type() == OutlineType::Cff` の assert を追加する責務は
/// 0027 のスコープに含まれる (0027 issue 本文の既存スコープ。本 issue からの追加責務ではない)。
#[test]
fn font_bundled_cff_test_font_file_readable() {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/fonts/SourceSerif4-Regular.otf");
    let data = FontData::from_file(path).expect("同梱した CFF テスト用フォントのファイルは必ず読める");
    let bytes = data.data();
    assert!(bytes.len() > 4, "OTF バイナリは 4 バイト以上ある必要がある");
    assert_eq!(
        &bytes[..4],
        b"OTTO",
        "CFF アウトラインを持つ OTF の sfnt version magic は 'OTTO' である必要がある"
    );
}
```

`use raden::{Font, FontData, FontFace};` は `tests/test_font.rs:1` で既に import 済みのため追加・編集不要。

### CHANGES.md への記載

本 issue は **テスト用フォントバイナリと LICENSE ファイルの追加のみ** で、公開 API 変更も描画結果変更もない。`shiguredo-changelog` 規約 (記載対象は UPDATE / ADD / CHANGE / FIX の各分類で、いずれも raden の公開 API や挙動への影響を対象とする) に照らし、テストアセットの追加は記載対象外。`CHANGES.md` には記載しない。

## 完了条件

### 配置

- `tests/fixtures/fonts/` ディレクトリを新規作成。
- 以下 4 ファイルを配置:
  - `tests/fixtures/fonts/SourceSans3-Regular.ttf` (Source Sans 3 公式 release タグ `3.052R` の未改変バイナリ。タグ番号が変動していた場合は auto-resolve は停止しユーザーに報告)
  - `tests/fixtures/fonts/SourceSans3-LICENSE.txt` (Source Sans 公式 `LICENSE.md` の全文を `.txt` 拡張子で保存。内容改変なし)
  - `tests/fixtures/fonts/SourceSerif4-Regular.otf` (Source Serif 4 公式 release タグ `4.005R` の未改変バイナリ。タグ番号が変動していた場合は auto-resolve は停止しユーザーに報告)
  - `tests/fixtures/fonts/SourceSerif4-LICENSE.txt` (Source Serif 公式 `LICENSE.md` の全文を `.txt` 拡張子で保存。内容改変なし)
- `.gitattributes` を新規作成 (既存なし。`test -e .gitattributes` で確認済み) し、`*.ttf binary` と `*.otf binary` の 2 行のみを記述。デフォルト改行設定は追加しない。
- **PR description に記録**: `wc -c tests/fixtures/fonts/SourceSans3-Regular.ttf tests/fixtures/fonts/SourceSerif4-Regular.otf` の出力と `shasum -a 256` (Windows なら `Get-FileHash -Algorithm SHA256`) の出力を PR description に貼り付ける (再現性確認用)。

### 必要テーブルの実機検証

- 解決方法 3 の `ttx` 検証コマンドを本 issue 着手時に実行し、Source Sans 3 が GSUB テーブル + `liga` feature + GPOS テーブル + Pair Adjustment Lookup (LookupType 2) + cmap (Format 4 または Format 12) を持つことを確認 (PR description に記録)。`kern` テーブルの有無も記録する。
- Source Serif 4 が `CFF` テーブルを持つことを確認 (PR description に記録)。
- 要件不足 (GSUB / GPOS / cmap (Format 4 or 12) / CFF のいずれかが欠落、または GSUB に `liga` 不在、または GPOS に Pair Adjustment Lookup 不在) が判明した場合、auto-resolve は本 issue の close を中止して停止しユーザーに報告する。`kern` 欠落のみは記録のみで停止条件としない (0025 polish で別フォント追加を検討する)。

### Cargo.toml 検証

- `Cargo.toml` を **編集しない**。
- 本 issue close PR を出す前に実装着手者が手元で以下を 1 回実行し、`OK` 表示を PR description に記録する:

```sh
# Cargo.toml が無編集であることを事前 assert (他の用件でローカル編集していた場合の偽陰性回避)
git diff --quiet Cargo.toml && git diff --cached --quiet Cargo.toml \
  || { echo "FAIL: Cargo.toml has uncommitted changes"; exit 1; }
# テストフィクスチャを stage して publish 配布物に含まれないことを確認
git add tests/fixtures/fonts/ .gitattributes
if cargo package --list --allow-dirty 2>/dev/null | grep -qE '^(tests/|\.gitattributes$)'; then
  echo "FAIL: test fixtures leaked into cargo publish list"; exit 1
else
  echo "OK: no test fixtures in cargo publish list"
fi
```

- `2>&1` ではなく `2>/dev/null` で stderr を捨てる (stderr の warning が `.ttf` 等の文字列を含む場合の偽陽性回避)。
- `grep` の正規表現は行頭一致 (`^(tests/|\.gitattributes$)`) で `cargo package --list` の出力 (1 行 1 ファイルパス) を厳密に判定する。
- `git diff --quiet Cargo.toml` を事前 assert に入れることで「`Cargo.toml` を編集しない」不変条件の検証範囲を担保する。
- CI 化は将来の別 issue で検討する。

### テスト

- `tests/test_font.rs` 末尾 (既存 `font_fill_text_integration` の後) に `font_bundled_truetype_test_font_loads` / `font_bundled_truetype_test_font_has_ascii_cmap` / `font_bundled_cff_test_font_file_readable` の 3 件を追加 (設計方針「テスト追加」参照)。
- CI 全 7 構成 (`ubuntu-24.04` / `ubuntu-24.04-arm` / `ubuntu-22.04` / `ubuntu-22.04-arm` / `macos-26` / `macos-15` / `windows-2025`) で 3 件すべて pass。

### 0001 メタ issue 更新 (本 issue close PR の責務)

`issues/0001-enhance-font-module-maturity.md` を以下の通り編集する。行番号は polish 時点のもので、実装着手時に動いている可能性があるため、見出し名で位置を特定する。**項目 1-3 / 5-7 はファイル編集を伴い (計 6 項目)、項目 4 と末尾の `### クロスプラットフォーム` 注記はファイル編集を伴わず close PR description で確認するのみ (計 2 件)**。

1. **`## Concrete issue 一覧` セクション直下のテーブル**: 末尾に新規行を追加。

   ```
   | 0039 | `0039-add-bundled-test-fonts.md` | テスト用フォントをリポジトリに同梱する | closed | High |
   ```

2. **`## Tracked items` セクション直下のテーブル**: 「テスト用フォント選定」行を **削除**。
3. **`### tracked: テスト用フォント選定` サブセクション (見出し + 「含まれる」「含まれない」「完了基準」「段階的選定」の 4 項目から成るブロック)**: ブロック **全体を削除**。Concrete issue 化により tracked 詳細は不要。
4. **`### concrete issue` セクションの完了条件項目** (ファイル編集なし): 「Tracked items のうち concrete issue 化対象 ... テスト用フォント選定 ... が、closed または `issues/pending/` 移動済み」は本 issue が closed 化されることで自動的に satisfy される。close PR description にこの完了条件項目が satisfy されたことを明記する。
5. **`### 前提 concrete issue (0021 着手前に concrete issue 化して close)` セクションのリスト本文 + リスト**:
   - 直前の本文 `以下 4 件を concrete issue として **順次起票**` の「**4 件**」を「**3 件**」に書き換える。
   - リスト項目 1「テスト用フォント選定 (TTF アウトラインのみのフォントを先行選定とリポジトリ追加)」を **削除** し、残りの項目 2-4 (fuzzing 基盤 / ベンチマーク / Compound Glyph point-matching) を 1-3 に番号付け直す。
6. **`### パフォーマンス基準` セクションの完了条件項目内の文** (Before / After):
   - Before: `測定対象フォントはテスト用フォント選定で確定する`
   - After: `測定対象フォントは 0039 で同梱済みの Source Sans 3 Regular を使用する`
   - 前段の `Context::fill_text` 1 ループ所要時間の文言は変更しない。同一箇条書き行内に複数の文が `。` で連結されているため、当該文字列のみを置換する。
7. **`### CFF / CFF2 判断` セクションの完了条件項目内の文** (Before / After):
   - Before: `テスト用フォント選定 tracked item には CFF / CFF2 フォントの選定を含める`
   - After: `CFF / CFF2 フォントは 0039 で Source Serif 4 Regular (OTF / CFF) を同梱済み`

`### クロスプラットフォーム` セクションの「全プラットフォームで利用可能なテスト用フォントバイナリがリポジトリに含まれる」項目は本 issue 完了で satisfy。文言修正不要 (close PR description でこの項目の satisfy を明記)。

### 0037 への申し送り (本 issue close PR の責務)

`issues/0037-refactor-tests-common-font-helper.md` には現状 `## 関連` セクションが **存在しない**。本 issue close PR で `## 解決方法` セクションの後ろに `## 関連` 見出しを新規作成し、以下を追記する:

```
## 関連

- `0039-add-bundled-test-fonts.md` (本 issue 完了後に 0037 のヘルパー集約を行う。
  集約先 (`tests/common/font.rs` 単独か `tests/common/mod.rs` + `pub mod font;` か、
  `RADEN_TEST_FONT_PATH` 環境変数経由か) は 0037 polish で確定する。0039 で配置済みの
  フォントは `tests/fixtures/fonts/SourceSans3-Regular.ttf` と
  `tests/fixtures/fonts/SourceSerif4-Regular.otf`。ヘルパー命名は 0025 / 0026 / 0027 が
  参照する `load_*_test_font()` 系統で 0037 polish で確定する。
  `pbt/tests/prop_font/main.rs` の `load_arial()` 複製を 0037 のスコープに含めるかは
  0037 polish で判断する。)
```

0037 polish は本 issue close 後に走らせる前提のため、本 issue close PR 時点で `## 関連` が既に追加されていても 0037 polish 担当が再構成する余地はある (本 issue は申し送りリンクの設置までを責務とする)。

### 0027 への申し送り (本 issue close PR の責務)

`issues/0027-add-cff-cff2-outline-support.md` の `## 隣接 issue への影響` セクション内、既存の「テスト用フォント選定 (0001 tracked)」項目 (1 行) を以下の 1 項目に書き換える (Before / After とも 0027 ファイル内では 1 行のリスト項目として配置する。backtick の入れ子表記破綻を避けるため fenced code block で示す)。

Before:

````
- **テスト用フォント選定 (0001 tracked)**: CFF ベースのテスト用フォントを選定する責務は本 issue 着手前に完了している必要がある
````

After:

````
- **0039 (0027 の着手前提として完了済)**: CFF ベースのテスト用フォント `tests/fixtures/fonts/SourceSerif4-Regular.otf` (Source Serif 4 Regular / SIL OFL 1.1) が 0039 で同梱済み。0027 着手時には本ファイルを使う。
````

加えて 0027 の `## 関連` セクション内の「依存: 0001 メタ issue の「テスト用フォント選定」tracked item (CFF フォントの選定)」行を以下に書き換える。

Before:

````
- 依存: 0001 メタ issue の「テスト用フォント選定」tracked item (CFF フォントの選定)
````

After:

````
- 依存: `0039-add-bundled-test-fonts.md` (CFF テスト用フォント `tests/fixtures/fonts/SourceSerif4-Regular.otf` を同梱済み)
````

`parse_all` の `dir.require(TAG_GLYF)` を緩める実装と `outline_type()` の追加、および本 issue が追加する `font_bundled_cff_test_font_file_readable` テストの改名・assert 追加は **0027 issue が当初から自身のスコープとして含めている事項** (0027 設計方針「テーブル選択」「公開 API」参照) のため、本 issue から新規に申し送る責務はない。

### 0025 / 0026 への申し送りは行わない

0025 issue「着手前提」と 0026 issue「着手前提」内のパス文字列プレースホルダ (`"<テスト用フォント選定 tracked で確定したパス>"` 等) の置換は、本 issue close PR ではなく各 issue 着手時 (polish または実装着手時) に行う。本 issue close PR では 0025 / 0026 を編集しない。

### ライセンス

- `tests/fixtures/fonts/SourceSans3-LICENSE.txt` と `tests/fixtures/fonts/SourceSerif4-LICENSE.txt` を SIL OFL 1.1 で配置済み (アップストリーム `LICENSE.md` 全文を `.txt` 拡張子で未改変保存)。
- リポジトリルートの `LICENSE` (Apache-2.0、raden 本体) には手を加えない。

## 解決方法

本セクションは「実装手順」に絞り、編集対象ファイルの詳細は完了条件側で重複なく参照する。

### 1. フォントバイナリと LICENSE の取得 (Bash 環境想定)

実装着手時に `git ls-remote --tags https://github.com/adobe-fonts/source-sans` と `git ls-remote --tags https://github.com/adobe-fonts/source-serif` を実行し、最新の `R` 付き release タグを確認する。本 issue 起票時点 (2026-06-22) の想定は Source Sans 3 = `3.052R`、Source Serif 4 = `4.005R`。**確認結果が想定と異なる場合は本 issue を reopen して polish 担当が採用フォント表を更新する**。

```sh
mkdir -p tests/fixtures/fonts
# 想定タグが変動していなかった場合はそのまま実行可能。変動していた場合は auto-resolve は停止し
# ユーザーに報告し、reopen 後の polish 担当が下記 2 行と完了条件「配置」の表記を更新する。
SOURCE_SANS_TAG=3.052R   # git ls-remote --tags で再確認
SOURCE_SERIF_TAG=4.005R  # git ls-remote --tags で再確認
# Source Sans 3 Regular (TTF) と LICENSE
curl -L -o tests/fixtures/fonts/SourceSans3-Regular.ttf \
  "https://github.com/adobe-fonts/source-sans/raw/refs/tags/${SOURCE_SANS_TAG}/TTF/SourceSans3-Regular.ttf"
curl -L -o tests/fixtures/fonts/SourceSans3-LICENSE.txt \
  "https://raw.githubusercontent.com/adobe-fonts/source-sans/refs/tags/${SOURCE_SANS_TAG}/LICENSE.md"
# Source Serif 4 Regular (OTF / CFF) と LICENSE
curl -L -o tests/fixtures/fonts/SourceSerif4-Regular.otf \
  "https://github.com/adobe-fonts/source-serif/raw/refs/tags/${SOURCE_SERIF_TAG}/OTF/SourceSerif4-Regular.otf"
curl -L -o tests/fixtures/fonts/SourceSerif4-LICENSE.txt \
  "https://raw.githubusercontent.com/adobe-fonts/source-serif/refs/tags/${SOURCE_SERIF_TAG}/LICENSE.md"
# サイズと SHA-256 を確認し PR description に記録
wc -c tests/fixtures/fonts/SourceSans3-Regular.ttf tests/fixtures/fonts/SourceSerif4-Regular.otf
shasum -a 256 tests/fixtures/fonts/SourceSans3-Regular.ttf tests/fixtures/fonts/SourceSerif4-Regular.otf
```

Windows ネイティブ (PowerShell) で同等手順を行う場合は `Invoke-WebRequest -Uri <url> -OutFile <path>` + `Get-FileHash -Algorithm SHA256 <path>` + `(Get-Item <path>).Length` を使う。CI ではなく開発者手元の作業のため、シェル環境は実装着手者が選択する。

合計サイズが約 1 MB を大きく超える場合は polish で再選定を検討する (git LFS 不使用方針のため)。

### 2. .gitattributes 追加

既存有無を確認してから新規作成:

```sh
# Bash 環境
test -e .gitattributes && echo "EXISTS - append manually" || printf '*.ttf binary\n*.otf binary\n' > .gitattributes
```

Windows (PowerShell) で同等手順を行う場合: `if (Test-Path .gitattributes) { Write-Host "EXISTS - append manually" } else { Set-Content -Path .gitattributes -Value "*.ttf binary`n*.otf binary" }`。既存があった場合は上書きせず末尾に追記する。本 issue 着手時点では既存なし。

追加後に Git 属性が効くことを確認:

```sh
git add tests/fixtures/fonts/ .gitattributes
git check-attr binary tests/fixtures/fonts/SourceSans3-Regular.ttf
# 期待: tests/fixtures/fonts/SourceSans3-Regular.ttf: binary: set
git ls-files --eol tests/fixtures/fonts/
# 期待: 各ファイルに attr/binary が付く (LF / CRLF 変換が無効化される)
```

### 3. 必要テーブルの実機検証

ローカルで `fontTools` (`pip install fonttools`) をインストールし、採用フォントが要求テーブルを持つことを確認する。検証結果は close PR description に記録する。

```sh
# Source Sans 3: 主要テーブルの存在確認 (kern は採用フォントによっては不在の場合あり)
ttx -l tests/fixtures/fonts/SourceSans3-Regular.ttf | grep -E '(kern|GSUB|GPOS|cmap)'
# 期待: GSUB / GPOS / cmap は必須。kern が無い場合は記録のみ (0025 polish で別フォント追加を検討)

# GSUB の liga feature 確認
ttx -t GSUB -o /tmp/SourceSans3-GSUB.ttx tests/fixtures/fonts/SourceSans3-Regular.ttf
grep -E '<FeatureTag value="liga"' /tmp/SourceSans3-GSUB.ttx
# 期待: <FeatureTag value="liga"/> が 1 件以上

# GPOS の Pair Adjustment Lookup (LookupType 2) 確認
ttx -t GPOS -o /tmp/SourceSans3-GPOS.ttx tests/fixtures/fonts/SourceSans3-Regular.ttf
grep -E 'LookupType value="2"|<PairPos ' /tmp/SourceSans3-GPOS.ttx
# 期待: LookupType="2" または PairPos タグが 1 件以上

# cmap subtable format 確認 (raden 対応の Format 4 / Format 12 が必要)
ttx -t cmap -o /tmp/SourceSans3-cmap.ttx tests/fixtures/fonts/SourceSans3-Regular.ttf
grep -E 'cmap_format_4|cmap_format_12' /tmp/SourceSans3-cmap.ttx
# 期待: cmap_format_4 または cmap_format_12 のいずれか以上

# Source Serif 4: CFF テーブルの存在確認
ttx -l tests/fixtures/fonts/SourceSerif4-Regular.otf | grep 'CFF '
# 期待: CFF
```

GSUB の `liga` feature、GPOS の Pair Adjustment Lookup、cmap (Format 4 か Format 12)、CFF のいずれかが欠落していた場合、auto-resolve は本 issue の close を中止して停止しユーザーに報告する (本 issue 冒頭の「auto-resolve エージェントが要件不足を検出した場合の振る舞い」参照)。reopen と採用フォント表更新は polish 担当が後追いで実施する。`kern` の有無は記録のみで停止条件としない (0025 polish で対応する)。

### 4. テストの追加

`tests/test_font.rs` 末尾 (既存 `font_fill_text_integration` の後) に設計方針「テスト追加」のコード片 3 件を追加する。`use raden::{Font, FontData, FontFace};` は既存の冒頭 use 文 (`tests/test_font.rs:1`) で必要シンボルがすべて揃っているため `use` 追加は不要。

### 5. cargo publish 配布物確認

完了条件「Cargo.toml 検証」のコマンドを開発者手元で 1 回実行し、grep 0 行マッチ (exit code 1) を PR description に記録する。

### 6. CI 全構成 pass の確認

push 後 GitHub Actions で全 7 構成が pass することを確認する。fail 時の初動診断:

- **macOS / Linux で fail**: `ls -la tests/fixtures/fonts/` で各ファイルサイズが解決方法 1 で記録した実測値と一致するか確認。極端に小さければ checkout 不整合 (Git LFS placeholder 等)。
- **Windows で fail**: `git ls-files --eol tests/fixtures/fonts/` で `attr/binary` が付いているかを確認。`-text` 表記が出ていれば `.gitattributes` の追記漏れ。
- **共通**: `expect` panic ログから `FontError` variant を読み取り、`InvalidData` 系なら parser の対応範囲外 (cmap format の想定外、テーブル欠落 等)、`Io` 系ならファイル checkout 不整合。

### 7. 関連 issue の編集

本 issue close PR で完了条件「0001 メタ issue 更新」「0037 への申し送り」「0027 への申し送り」のとおり関連 issue を編集する。各編集の対象セクション・Before / After は完了条件側に詳述されているため、本セクションでは概要のみ示す:

- `issues/0001-enhance-font-module-maturity.md`: 7 項目 (うち 5 項目はファイル編集、2 項目は PR description 確認)。
- `issues/0037-refactor-tests-common-font-helper.md`: 新規 `## 関連` セクション追加。
- `issues/0027-add-cff-cff2-outline-support.md`: `## 隣接 issue への影響` 1 項目書き換え + `## 関連` 1 項目書き換え。

## エッジケース

- **フォントファイルが checkout されない (Git LFS 等の予期せぬ介在)**: `font_bundled_truetype_test_font_loads` が `expect` で panic し CI fail で顕在化する。Git LFS は使用しない方針のため、通常は発生しない。
- **フォントバイナリが CRLF 変換で破損**: `FontFace::from_data` が `Err` を返し、`expect` で panic、CI fail で顕在化する。`.gitattributes` の `binary` 指定で予防する。
- **採用フォントが想定テーブルを欠く**: 解決方法 3 の `ttx` 検証で着手時に発見する。GSUB / GPOS / cmap (Format 4 or 12) / CFF のいずれかが欠落していた場合は本 issue を reopen し採用フォントを再選定する。`kern` 欠落は記録のみで本 issue では reopen 条件としない。
- **採用フォントの release タグが変動**: 解決方法 1 の `git ls-remote --tags` で発見する。想定と異なる場合は本 issue を reopen し polish 担当が採用フォント表を更新する (実装着手者は issue 本文を書き換えない)。
- **リポジトリサイズ膨張**: 解決方法 1 の `wc -c` で実測値を記録する。約 1 MB を大きく超える場合は polish で再選定。
- **`Cargo.toml` の `include` が将来不用意に書き換えられテストフォントが publish に混入**: 本 issue では CI 化せず、開発者手元の `cargo package --list` 検証 (完了条件「Cargo.toml 検証」) のみで対応する。将来この回帰検出を自動化する場合は別 issue (例: 「`tests/fixtures/` が `cargo publish` 配布物に混入しないことを CI で検証する」) を起票する。

## 変更対象ファイル

| ファイル | 種別 | 内容 |
|---|---|---|
| `tests/fixtures/fonts/SourceSans3-Regular.ttf` | 新規 | Source Sans 3 Regular (TTF) 未改変バイナリ (release タグ `3.052R` 想定) |
| `tests/fixtures/fonts/SourceSans3-LICENSE.txt` | 新規 | Source Sans 公式 `LICENSE.md` 全文を `.txt` 拡張子で保存 |
| `tests/fixtures/fonts/SourceSerif4-Regular.otf` | 新規 | Source Serif 4 Regular (OTF/CFF) 未改変バイナリ (release タグ `4.005R` 想定) |
| `tests/fixtures/fonts/SourceSerif4-LICENSE.txt` | 新規 | Source Serif 公式 `LICENSE.md` 全文を `.txt` 拡張子で保存 |
| `.gitattributes` | 新規 (ルート直下) | `*.ttf binary` / `*.otf binary` の 2 行のみ |
| `tests/test_font.rs` | 既存編集 | 3 件のテストを末尾に追加 (`font_bundled_truetype_test_font_loads` / `font_bundled_truetype_test_font_has_ascii_cmap` / `font_bundled_cff_test_font_file_readable`) |
| `issues/0001-enhance-font-module-maturity.md` | 既存編集 | 完了条件「0001 メタ issue 更新」項目 1-3 / 5-6 (項目 4 / 7 はファイル編集なし) |
| `issues/0037-refactor-tests-common-font-helper.md` | 既存編集 | `## 解決方法` 後に新規 `## 関連` セクションを作成し申し送り追記 |
| `issues/0027-add-cff-cff2-outline-support.md` | 既存編集 | `## 隣接 issue への影響` / `## 関連` 各 1 項目書き換え |

## 関連

- `0001-enhance-font-module-maturity.md` (メタ issue。本 issue は Tracked items「テスト用フォント選定」を完了させる責務を持つ)
- `0025-add-font-kerning.md` (kern 付きフォント前提。本 issue 完了が着手前提。Source Sans 3 が `kern` 不在の場合は 0025 polish で別フォント追加を検討)
- `0026-add-opentype-basic-shaping.md` (GSUB / GPOS 付きフォント前提。本 issue 完了が着手前提)
- `0027-add-cff-cff2-outline-support.md` (CFF / CFF2 フォント前提。本 issue 完了が着手前提)
- `0037-refactor-tests-common-font-helper.md` (`load_arial()` 集約)
