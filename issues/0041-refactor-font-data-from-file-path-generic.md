## FontData::from_file の引数を `&str` から `impl AsRef<std::path::Path>` に変更する

- Priority: Low
- Category: refactor
- Created: 2026-06-23
- Completed: {YYYY-MM-DD}
- Model: Opus 4.7
- Branch: feature/refactor-font-data-from-file-path-generic
- Polished: 2026-06-23
- Reporter: @voluntas

## 目的

`raden::FontData::from_file` の引数型を `&str` から `impl AsRef<std::path::Path>` に変更し、ファイルパスを受け取る Rust 標準の慣習に揃える。

現状の `&str` 引数は、呼び出し側が `Path` / `PathBuf` を持っている場合に `.to_str().unwrap()` のような変換を強いる。これは `Path` が必ずしも UTF-8 とは限らない（OS のパスは `OsStr` ベース）という Rust の前提に対して情報を落とす変換であり、慣習的でもない。

リポジトリ内の他のファイル系公開 API は既に `impl AsRef<Path>` を採用しており、`FontData::from_file` だけが浮いている。

- `src/codec/bmp.rs:10-11` の `write_bmp`（`src/codec/mod.rs:1` の `pub mod bmp;` 経由で公開）: `path: impl AsRef<Path>`
- `src/api/image.rs:62` の `Image::write_to_file`（公開 API）: `path: impl AsRef<Path>`
- `src/font/mod.rs:52` の `FontData::from_file`（公開 API）: `path: &str` ← これだけ揃っていない

本変更は **既存呼び出しのコンパイル互換性を保つ**（後述）。リポジトリ内の呼び出し 3 箇所はソース無変更でビルド・テストが通る想定で、CalVer（`Cargo.toml:3` `version = "2026.1.1"`）のバージョン bump も不要。

## 優先度根拠

Low。機能・性能・正しさへの影響なし。既存呼び出しを破壊しないため緊急性もない。リポジトリ内既存パターンと Rust 慣習へのアラインメントが主目的。`&str` のままだと不自然との指摘が出ている経緯がある（フィードバック由来は `Reporter:` フィールドで記録）ため、直す価値はある。

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

`crate::api::path::Path` は `raden::Path` として再エクスポート済み（`src/lib.rs:17`）で `std::path::Path` と完全同名。`use std::path::Path;` を素朴に追加すると `E0252` で衝突する点は「設計方針 > 衝突回避」で扱う。

### リポジトリ内の呼び出し箇所

いずれも `&str` リテラル / `&str` 変数を渡している。本変更後もソース無変更でビルド・テストが通る。

- `examples/raden_player.rs:225`: `FontData::from_file(FONT_PATH)` （`FONT_PATH: &str`）
- `tests/helpers/font_local.rs:15`: `FontData::from_file(path)` （`path: &str`）
- `pbt/tests/prop_font/main.rs:15`: `FontData::from_file(path)` （`path: &str`）

`pbt/` は `Cargo.toml:15` の `members = ["pbt"]` で workspace member。検証コマンドは `--workspace` 必須。

### ドキュメント記述の現状

`README.md:201` / `docs/BLEND2D.md:406` / `skills/raden/SKILL.md:228` はいずれも `from_file(path)` の表記で型まで踏み込んでおらず、本変更で書き換え不要。

## 設計方針

### 衝突回避: fully-qualified `impl AsRef<std::path::Path>` を採用する

`use std::path::Path;` は **追加しない**。シグネチャ・本体で `std::path::Path` を fully-qualified で書き、既存の `use crate::api::path::Path;` には触れない。`src/font/mod.rs:188` の `path: &mut Path`（raden ドメイン型）の意味も維持する。

### 採用形式

リポジトリ内の既存公開 API と同じく `impl Trait` 形式を採用する。`<P: AsRef<Path>>(path: P)` 形式と意味は同じだが、型パラメータを呼び出し側に公開しないシンプルさを優先する。

`std::fs::read` 自身が `<P: AsRef<Path>>` を受けるため、関数本体は `std::fs::read(path)?` で書ける（`.as_ref()` 明示は不要。`bmp::write_bmp` 内の `File::create(path)?` と同じスタイル）。

### 後方互換性と非互換ケース

互換（変更前後でビルドが通る）:

- `&str` リテラル渡し: `FontData::from_file("path/to/font.ttf")`
- `&str` 変数渡し: `FontData::from_file(path)` （`path: &str`）
- 新たに渡せるようになるもの: `&Path` / `&PathBuf` / `&String` 等、`AsRef<Path>` を実装する任意の型

厳密には非互換となる稀ケース（raden の利用パターンとして想定しない）:

- 関数ポインタ取得: `let f: fn(&str) -> _ = FontData::from_file;` はジェネリック関数化により壊れる
- `impl Trait` 形式のためターボフィッシュ `FontData::from_file::<&str>(...)` は元から不可（現行 `&str` 版でも不要）

`shiguredo-changelog` の分類では、通常呼び出しが無変更で通るため `[CHANGE]`（後方互換のない変更）ではなく `[UPDATE]`（後方互換がある変更）で扱う。

## 完了条件

- `src/font/mod.rs:52` の `FontData::from_file` のシグネチャが `pub fn from_file(path: impl AsRef<std::path::Path>) -> Result<Self, FontError>` になっている
- `src/font/mod.rs:51` の docstring に、受け付ける型に関する補足の 1 行が追加されている（本文案は「解決方法 3」参照）
- `src/font/mod.rs` の他箇所（`outline_glyph` 等の `&mut Path` 参照）が壊れていない
- `tests/test_font.rs` 先頭の `use raden::{...};` に `FontError` が追加されている
- `tests/test_font.rs` に `&str` / `&Path` / `&PathBuf` / `&String` の 4 種を渡してコンパイルが通り、いずれも `Err(FontError::Io(_))` が返ることを確認するテストが 1 件追加されている
- `CHANGES.md` の `## develop` セクション末尾に以下 2 行（変更内容 + 担当者）が追加されている
  ```
  - [UPDATE] `FontData::from_file` の引数を `impl AsRef<std::path::Path>` に変更する
    - @voluntas
  ```
- CI と同じ 3 コマンド（`cargo fmt --all --check` / `cargo test --workspace` / `cargo clippy --workspace -- -D warnings`、`.github/workflows/ci.yml:47-49`）がローカルで通る

## 解決方法

1. `src/font/mod.rs` の `FontData::from_file` のシグネチャを `pub fn from_file(path: impl AsRef<std::path::Path>) -> Result<Self, FontError>` に変更する（`use std::path::Path;` は追加しない。既存の `use crate::api::path::Path;` には触れない）。関数本体（`let data = std::fs::read(path)?;` の行）は無変更でよい（`std::fs::read` 自身が `AsRef<Path>` を受けるため）

2. `src/font/mod.rs:51` の docstring（`/// ファイルからフォントデータを読み込む。`）の下に空行を 1 行挟み、補足を 1 行追加する。最終的な関数定義は以下のようになる。

   ```rust
   impl FontData {
       /// ファイルからフォントデータを読み込む。
       ///
       /// `AsRef<std::path::Path>` を実装する任意の型を受け付ける。
       pub fn from_file(path: impl AsRef<std::path::Path>) -> Result<Self, FontError> {
           let data = std::fs::read(path)?;
           Ok(Self { data })
       }
   ```

3. `tests/test_font.rs` に受け入れ型の網羅テストを追加する。

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

4. `CHANGES.md` の `## develop` セクション末尾（次節 `## 2026.1.1` の直前）に以下の 2 行を追加する。`shiguredo-changelog` 規約の 2 行構造（変更内容 + 2 スペースインデントの担当者）に従う。

   ```
   - [UPDATE] `FontData::from_file` の引数を `impl AsRef<std::path::Path>` に変更する
     - @voluntas
   ```

5. ローカルで CI と同じ 3 コマンド（`cargo fmt --all --check` / `cargo test --workspace` / `cargo clippy --workspace -- -D warnings`、`.github/workflows/ci.yml:47-49`）を流し、既存呼び出し箇所が無変更で通ることを確認する。

## 変更対象ファイル

- `src/font/mod.rs`: 解決方法 1, 2
- `tests/test_font.rs`: 解決方法 3
- `CHANGES.md`: 解決方法 4

## 将来の拡張（本 issue のスコープ外）

- `raden::Path`（`crate::api::path::Path`）と `std::path::Path` の同名衝突は、ファイル系の公開 API を追加するたびに `src/font/mod.rs` 系で同じ罠を引く構造。本変更で fully-qualified の `std::path::Path` を使う回避策が定着するが、根本的には raden 側の公開シンボル名 `Path` のリネーム可否を別途検討する余地がある。必要が確定した時点で `create-issue` 経由で別 issue として起票する
