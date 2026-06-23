## ファイル系公開 API の引数を `<P: AsRef<Path>>(path: P)` 形式に変更する

- Priority: Low
- Category: refactor
- Created: 2026-06-23
- Completed: {YYYY-MM-DD}
- Model: Opus 4.7
- Branch: feature/refactor-font-data-from-file-path-generic
- Polished: 2026-06-23
- Reporter: @voluntas

## 目的

raden の公開 API のうち、ファイルパスを受け取る 3 関数の引数型を std 慣習の `<P: AsRef<Path>>(path: P)` 形式に揃える。主目的は `FontData::from_file` を `&str` から汎用形に変えること（`Path` / `PathBuf` 直渡しを許容する）で、合わせて既に `impl AsRef<Path>` だった他 2 関数の形式も `<P: ...>` 形式に変更する。

対象 3 関数:

- `src/font/mod.rs:52` の `FontData::from_file`（公開 API）: `path: &str` → `<P: AsRef<std::path::Path>>(path: P)`
- `src/codec/bmp.rs:10-11` の `write_bmp`（`src/codec/mod.rs:1` の `pub mod bmp;` 経由で公開）: `path: impl AsRef<Path>` → `<P: AsRef<Path>>(path: P)`
- `src/api/image.rs:62` の `Image::write_to_file`（公開 API）: `path: impl AsRef<Path>` → `<P: AsRef<Path>>(path: P)`

### なぜ `<P: AsRef<Path>>(path: P)` 形式か

std とエコシステム多数派の慣習に揃える。

- `std::fs::read` / `std::fs::write` / `std::fs::File::open` / `std::fs::read_to_string` などはすべて `<P: AsRef<Path>>(path: P)` 形式
- 主要クレート（tokio, image, tempfile, walkdir, notify, fs_extra など）も `<P: AsRef<Path>>(path: P)` 形式
- `impl Trait` 形式と意味は同じだが、std と揃える方が API 利用者にとって馴染みが深い

`FontData::from_file` 現状の `&str` 引数は、呼び出し側が `Path` / `PathBuf` を持っている場合に `.to_str().unwrap()` のような変換を強いる。`Path` が必ずしも UTF-8 とは限らない（OS のパスは `OsStr` ベース）という Rust の前提に対して情報を落とす変換であり、慣習的でもない。

本変更は **既存呼び出しのコンパイル互換性を保つ**（後述）。リポジトリ内の呼び出し箇所は全てソース無変更でビルド・テストが通る想定で、CalVer（`Cargo.toml:3` `version = "2026.1.1"`）のバージョン bump も不要。

## 優先度根拠

Low。機能・性能・正しさへの影響なし。既存呼び出しを破壊しないため緊急性もない。std 慣習へのアラインメントが主目的。`FontData::from_file` の `&str` のままだと不自然との指摘が出ている経緯がある（フィードバック由来は `Reporter:` フィールドで記録）ため、まとめて直す価値はある。

## 現状

### `src/font/mod.rs` の現状

- L11: `use crate::api::path::Path;` （raden ドメイン型 `Path` の import がある）
- L51: docstring `/// ファイルからフォントデータを読み込む。` の 1 行
- L52-55: `pub fn from_file(path: &str) -> Result<Self, FontError>` を定義
- L188: `FontFace::outline_glyph(..., path: &mut Path, ...)` で raden ドメイン型 `Path` を参照（本変更後も同じ識別子 `Path` の意味を保つ）

```rust
// L52-55
impl FontData {
    /// ファイルからフォントデータを読み込む。
    pub fn from_file(path: &str) -> Result<Self, FontError> {
        let data = std::fs::read(path)?;
        Ok(Self { data })
    }
```

`crate::api::path::Path` は `raden::Path` として再エクスポート済み（`src/lib.rs:17`）で `std::path::Path` と完全同名。`use std::path::Path;` を素朴に追加すると `E0252` で衝突するため、`<P: AsRef<std::path::Path>>(path: P)` のように fully-qualified で書く（既存の `use crate::api::path::Path;` には触れない）。

### `src/codec/bmp.rs` の現状

- L3: `use std::path::Path;`（衝突なし、そのまま使える）
- L10-16: `pub fn write_bmp(path: impl AsRef<Path>, width: u32, height: u32, stride: usize, data: &[u8]) -> std::io::Result<()>`
- L17: 本体は `let mut file = File::create(path)?;` で `File::create` が `<P: AsRef<Path>>` を受けるため `path` をそのまま渡せる

### `src/api/image.rs` の現状

- L1: `use std::path::Path;`（衝突なし）
- L62-64: `pub fn write_to_file(&self, path: impl AsRef<Path>) -> std::io::Result<()> { bmp::write_bmp(path, self.width, self.height, self.stride, &self.data) }`

### リポジトリ内の呼び出し箇所

`FontData::from_file` 呼び出し（いずれも `&str` リテラル / `&str` 変数）:

- `examples/raden_player.rs:225`: `FontData::from_file(FONT_PATH)` （`FONT_PATH: &str`）
- `tests/helpers/font_local.rs:15`: `FontData::from_file(path)` （`path: &str`）
- `pbt/tests/prop_font/main.rs:15`: `FontData::from_file(path)` （`path: &str`）

`bmp::write_bmp` の外部からの直接呼び出しは現状なし（`Image::write_to_file` 内部のみ）。`Image::write_to_file` も同様に外部からの利用は examples / tests には見当たらない。

`pbt/` は `Cargo.toml:15` の `members = ["pbt"]` で workspace member。検証コマンドは `--workspace` 必須。

### ドキュメント記述の現状

`README.md:201` / `docs/BLEND2D.md:406` / `skills/raden/SKILL.md:228` はいずれも `from_file(path)` の表記で型まで踏み込んでおらず、本変更で書き換え不要。

## 設計方針

### 採用形式: `<P: AsRef<Path>>(path: P)`

3 関数すべてを `<P: AsRef<Path>>(path: P)` 形式に揃える。`<P: AsRef<std::path::Path>>(path: P)` 形式は std / 主要クレートで広く採用されている、ファイルパス引数のデファクト慣習。

`FontData::from_file` だけは同ファイル内で `raden::Path` と名前衝突するため `<P: AsRef<std::path::Path>>(path: P)` と fully-qualified で書く。`bmp::write_bmp` と `Image::write_to_file` は既に `use std::path::Path;` 済みのため `<P: AsRef<Path>>(path: P)` でよい。

### 関数本体への影響

いずれも本体は無変更で済む。

- `FontData::from_file`: `std::fs::read(path)?` — `std::fs::read` 自身が `<P: AsRef<Path>>` を受けるため、`P` をそのまま渡せる
- `bmp::write_bmp`: `File::create(path)?` — `File::create` 自身が `<P: AsRef<Path>>` を受ける
- `Image::write_to_file`: `bmp::write_bmp(path, ...)` — `bmp::write_bmp` 側も同じく `<P: AsRef<Path>>` 化するため、`P` をそのまま渡せる

### 後方互換性と非互換ケース

互換（変更前後でビルドが通る）:

- `FontData::from_file`: 既存の `&str` リテラル / `&str` 変数渡し
- `FontData::from_file` で新たに渡せるようになるもの: `&Path` / `&PathBuf` / `&String` / `String` / `PathBuf` 等、`AsRef<Path>` を実装する任意の型
- `bmp::write_bmp` / `Image::write_to_file`: 既存呼び出し（`impl AsRef<Path>` も `<P: AsRef<Path>>` も同じく `AsRef<Path>` 実装型を受けるため、利用側に差はない）

厳密には非互換となる稀ケース（raden の利用パターンとして想定しない）:

- `FontData::from_file` の関数ポインタ取得: `let f: fn(&str) -> _ = FontData::from_file;` はジェネリック関数化により壊れる
- `impl Trait` 形式から `<P: ...>` 形式への変更は、`bmp::write_bmp::<&Path>(...)` のようなターボフィッシュ呼び出しが可能になる方向の拡張（壊れる方向ではない）

`shiguredo-changelog` の分類では、通常呼び出しが無変更で通るため `[CHANGE]` ではなく `[UPDATE]` で扱う。

## 完了条件

- `src/font/mod.rs:52` の `FontData::from_file` のシグネチャが `pub fn from_file<P: AsRef<std::path::Path>>(path: P) -> Result<Self, FontError>` になっている
- `src/font/mod.rs` の他箇所（`outline_glyph` 等の `&mut Path` 参照）が壊れていない
- `src/codec/bmp.rs:10` の `write_bmp` のシグネチャが `pub fn write_bmp<P: AsRef<Path>>(path: P, width: u32, height: u32, stride: usize, data: &[u8]) -> std::io::Result<()>` になっている
- `src/api/image.rs:62` の `Image::write_to_file` のシグネチャが `pub fn write_to_file<P: AsRef<Path>>(&self, path: P) -> std::io::Result<()>` になっている
- `tests/test_font.rs` 先頭の `use raden::{...};` に `FontError` が追加されている
- `tests/test_font.rs` に `&str` / `&Path` / `&PathBuf` / `&String` の 4 種を渡してコンパイルが通り、いずれも `Err(FontError::Io(_))` が返ることを確認するテストが 1 件追加されている
- `CHANGES.md` の `## develop` セクション末尾に以下 3 件 6 行（各エントリ 2 行 = 変更内容 + 担当者）が追加されている
  ```
  - [UPDATE] `FontData::from_file` の引数を `<P: AsRef<Path>>(path: P)` 形式に変更する
    - @sile
  - [UPDATE] `codec::bmp::write_bmp` の引数を `<P: AsRef<Path>>(path: P)` 形式に変更する
    - @sile
  - [UPDATE] `Image::write_to_file` の引数を `<P: AsRef<Path>>(path: P)` 形式に変更する
    - @sile
  ```
- CI と同じ 3 コマンド（`cargo fmt --all --check` / `cargo test --workspace` / `cargo clippy --workspace -- -D warnings`、`.github/workflows/ci.yml:47-49`）がローカルで通る

## 解決方法

1. `src/font/mod.rs` の `FontData::from_file` のシグネチャを `pub fn from_file<P: AsRef<std::path::Path>>(path: P) -> Result<Self, FontError>` に変更する（`use std::path::Path;` は追加しない。既存の `use crate::api::path::Path;` には触れない）。関数本体（`let data = std::fs::read(path)?;` の行）と docstring は無変更でよい（`std::fs::read` 自身が `AsRef<Path>` を受けるため）

2. `src/codec/bmp.rs` の `write_bmp` のシグネチャを `pub fn write_bmp<P: AsRef<Path>>(path: P, width: u32, height: u32, stride: usize, data: &[u8]) -> std::io::Result<()>` に変更する。本体は無変更（`File::create(path)` のまま）

3. `src/api/image.rs` の `Image::write_to_file` のシグネチャを `pub fn write_to_file<P: AsRef<Path>>(&self, path: P) -> std::io::Result<()>` に変更する。本体は無変更（`bmp::write_bmp(path, ...)` のまま、`bmp::write_bmp` 側も同じ `<P: AsRef<Path>>` のため `P` をそのまま渡せる）

4. `tests/test_font.rs` に受け入れ型の網羅テストを追加する。

   - L7 の `use raden::{...};` に `FontError` を追加する（現状: `use raden::{Font, FontData, FontFace, FontFeatureSettings, GlyphBuffer};` → 追加後: `use raden::{Font, FontData, FontError, FontFace, FontFeatureSettings, GlyphBuffer};`。アルファベット順で `FontData` と `FontFace` の間に挿入する）
   - 追加テストはフォント実体を必要としないため、既存テストとの位置依存はない。`tests/test_font.rs` の末尾に追加する
   - テストの意図は「シグネチャ一般化で 4 種類の型を渡してもコンパイルが通ること（受け入れ型の網羅）」と「いずれの型でも存在しないパスに対して `FontError::Io(_)` を返すこと（エラーパスの一貫性）」の 2 点

   ```rust
   #[test]
   fn from_file_accepts_path_like_types() {
       // 4 種類のパス型を渡し、いずれもコンパイルが通り `FontError::Io` で返ることを確認する。
       // フォント実体は必要とせず、CI 環境でも実行できる。
       // 所有権を保持し続ける典型ユースケースに合わせて、`String` / `PathBuf` も参照渡しで検証する。
       let s: &str = "definitely_not_existing_font_file";
       assert!(matches!(FontData::from_file(s), Err(FontError::Io(_))));

       let p: &std::path::Path = std::path::Path::new("definitely_not_existing_font_file");
       assert!(matches!(FontData::from_file(p), Err(FontError::Io(_))));

       let buf: std::path::PathBuf = std::path::PathBuf::from("definitely_not_existing_font_file");
       assert!(matches!(FontData::from_file(&buf), Err(FontError::Io(_))));

       let owned: String = String::from("definitely_not_existing_font_file");
       assert!(matches!(FontData::from_file(&owned), Err(FontError::Io(_))));
   }
   ```

   `bmp::write_bmp` と `Image::write_to_file` は形式変更のみで受け付ける型は変わらないため追加テスト不要。

5. `CHANGES.md` の `## develop` セクション末尾（次節 `## 2026.1.1` の直前）に以下 3 件のエントリを追加する。`shiguredo-changelog` 規約の 2 行構造（変更内容 + 2 スペースインデントの担当者）に従う。

   ```
   - [UPDATE] `FontData::from_file` の引数を `<P: AsRef<Path>>(path: P)` 形式に変更する
     - @sile
   - [UPDATE] `codec::bmp::write_bmp` の引数を `<P: AsRef<Path>>(path: P)` 形式に変更する
     - @sile
   - [UPDATE] `Image::write_to_file` の引数を `<P: AsRef<Path>>(path: P)` 形式に変更する
     - @sile
   ```

6. ローカルで CI と同じ 3 コマンド（`cargo fmt --all --check` / `cargo test --workspace` / `cargo clippy --workspace -- -D warnings`、`.github/workflows/ci.yml:47-49`）を流し、既存呼び出し箇所が無変更で通ることを確認する。

## 変更対象ファイル

- `src/font/mod.rs`: 解決方法 1
- `src/codec/bmp.rs`: 解決方法 2
- `src/api/image.rs`: 解決方法 3
- `tests/test_font.rs`: 解決方法 4
- `CHANGES.md`: 解決方法 5

## 将来の拡張（本 issue のスコープ外）

- `raden::Path`（`crate::api::path::Path`）と `std::path::Path` の同名衝突は、ファイル系の公開 API を追加するたびに `src/font/mod.rs` 系で同じ罠を引く構造。本変更で fully-qualified の `std::path::Path` を使う回避策が定着するが、根本的には raden 側の公開シンボル名 `Path` のリネーム可否を別途検討する余地がある。必要が確定した時点で `create-issue` 経由で別 issue として起票する
