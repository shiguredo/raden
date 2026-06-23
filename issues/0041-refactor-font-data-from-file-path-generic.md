## FontData::from_file の引数を `impl AsRef<Path>` に変更する

- Priority: Low
- Created: 2026-06-23
- Completed: {YYYY-MM-DD}
- Model: Opus 4.7
- Branch: feature/refactor-font-data-from-file-path-generic
- Polished: {YYYY-MM-DD}
- Reporter: @voluntas

## 目的

`raden::FontData::from_file` の引数型を `&str` から `impl AsRef<Path>` に変更し、ファイルパスを受け取る Rust 標準の慣習に揃える。

現状の `&str` 引数だと、呼び出し側が `Path` / `PathBuf` を持っている場合に `.to_str().unwrap()` のような変換を強いる。
これは `Path` が必ずしも UTF-8 とは限らないという Rust の前提（OS のパスは `OsStr` ベース）に対して情報を落とす変換であり、慣習的でもない。

また同じリポジトリ内の他のファイル読み書き API は既に `impl AsRef<Path>` を採用しており、`FontData::from_file` だけが浮いている。

- `src/codec/bmp.rs:11`: `path: impl AsRef<Path>`
- `src/api/image.rs:62` (`Image::write_to_file`): `path: impl AsRef<Path>`
- `src/font/mod.rs:52` (`FontData::from_file`): `path: &str`  ← これだけ揃っていない

`&str` から `impl AsRef<Path>` への変更は **後方互換**。`&str` は `AsRef<Path>` を実装しているため、既存の呼び出し側（`&str` リテラル渡し）はそのままコンパイルが通る。

## 優先度根拠

Low。

- 機能・性能・正しさへの影響なし。純粋に Rust 慣習へのアラインメント
- 既存の呼び出しは破壊しないため、緊急性はない
- ただし、外向きの公開 API であり、ドキュメントを見たユーザーから「`&str` は慣習的でない」と指摘された経緯がある（docs.rs の `FontData::from_file` シグネチャに対するフィードバック）。直す価値はある

## 現状

`src/font/mod.rs:50-55`:

```rust
impl FontData {
    /// ファイルからフォントデータを読み込む。
    pub fn from_file(path: &str) -> Result<Self, FontError> {
        let data = std::fs::read(path)?;
        Ok(Self { data })
    }
    ...
}
```

リポジトリ内の呼び出し箇所（いずれも `&str` リテラルまたは `&str` 変数を渡している）:

- `examples/raden_player.rs:225`: `FontData::from_file(FONT_PATH)` （`FONT_PATH: &str`）
- `tests/helpers/font_local.rs:15`: `FontData::from_file(path)` （`path: &str`）
- `pbt/tests/prop_font/main.rs:15`: `FontData::from_file(path)` （`path: &str`）

ドキュメント側の言及:

- `README.md:201`: 「`from_file(path)` または `from_bytes(bytes)` で作成」
- `docs/BLEND2D.md:406`: `FontData::from_file(path)` を Blend2D の `BLFontData::create_from_file(path, flags)` の対応として記載
- `skills/raden/SKILL.md:228`: 同上

シグネチャ自体には触れていないので、これらのドキュメント本文は今回の変更で書き換え不要。

## 設計方針

`src/codec/bmp.rs` と `src/api/image.rs` の既存パターンに合わせて、`impl AsRef<Path>` を引数に取る形に変える。

変更後イメージ:

```rust
use std::path::Path;

impl FontData {
    /// ファイルからフォントデータを読み込む。
    pub fn from_file(path: impl AsRef<Path>) -> Result<Self, FontError> {
        let data = std::fs::read(path.as_ref())?;
        Ok(Self { data })
    }
    ...
}
```

ポイント:

- `<P: AsRef<Path>>(path: P)` でも `impl AsRef<Path>` でもセマンティクスは同じ。リポジトリ内の `bmp.rs` / `image.rs` が `impl AsRef<Path>` を採用しているので **そちらに揃える**
- `std::fs::read` は `AsRef<Path>` を受け取るので、内部で `path.as_ref()` を渡せばよい
- `use std::path::Path;` を `src/font/mod.rs` に追加する
- `FontError` / 戻り値型は変えない。`std::io::Error` 経路はそのまま機能する

後方互換性:

- `&str` は `AsRef<Path>` を実装しているため、既存の `FontData::from_file("path/to/font.ttf")` 呼び出しはそのままビルドが通る
- `&Path` / `PathBuf` / `String` / `&String` などもそのまま渡せるようになる
- メジャーバージョンを上げる必要はない (互換)

## 完了条件

- `src/font/mod.rs` の `FontData::from_file` のシグネチャが `pub fn from_file(path: impl AsRef<Path>) -> Result<Self, FontError>` になっている
- リポジトリ内の既存呼び出し箇所がそのままビルド・テスト通過する（呼び出し側の変更は不要なはず）
  - `examples/raden_player.rs`
  - `tests/helpers/font_local.rs`
  - `pbt/tests/prop_font/main.rs`
- `cargo build` / `cargo test` / `cargo clippy` が通る
- 公開 API のドキュメントコメント（`/// ファイルからフォントデータを読み込む。`）は据え置きでよいが、`Path` を受け付けるようになったことを 1 行で補足してもよい

## 解決方法

1. `src/font/mod.rs` の冒頭の `use` に `std::path::Path` を追加する
2. `FontData::from_file` のシグネチャを `pub fn from_file(path: impl AsRef<Path>) -> Result<Self, FontError>` に変更する
3. 関数本体を `let data = std::fs::read(path.as_ref())?;` に変更する
4. ローカルで `cargo build` / `cargo test` / `cargo clippy --all-targets` を流して、既存呼び出し箇所が無変更で通ることを確認する

テスト:

- 既存の `tests/test_font.rs` などはすべて `from_bytes` 経由なので影響なし
- `tests/helpers/font_local.rs` と `pbt/tests/prop_font/main.rs` の `&str` 渡しがそのまま通ることが回帰テストになる
- 追加テストは原則不要（純粋なシグネチャ一般化で、ロジック差分なし）
