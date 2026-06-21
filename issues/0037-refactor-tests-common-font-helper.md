# tests/ 配下のフォントヘルパー (load_arial) を共通モジュールに集約する

- Priority: Low
- Category: refactor
- Created: 2026-06-22
- Model: Opus 4.7
- Branch: feature/refactor-tests-common-font-helper
- Polished: {YYYY-MM-DD}

## 目的

統合テスト (`tests/*.rs`) で重複している `load_arial()` フォントヘルパーを `tests/common/font.rs` (または同等の構造) に集約する。N=2 で許容、N=3 で集約という方針に基づき、0025 / 0026 で更に重複が増える前に整理する。

## 優先度根拠

Low。実害はないが、重複コード 2 箇所が `tests/test_font.rs:7-14` と `tests/test_context.rs:497-504` (`mod stroke_text` 内) で並んでおり、0025 / 0026 (`measure_text` / kerning / shaping のテスト追加) で更に重複が増える見込み。Don't live with broken windows 原則。

## 現状

- `tests/test_font.rs:7-14` の `load_arial()`:
  ```rust
  fn load_arial() -> Option<FontFace> {
      let path = "/System/Library/Fonts/Supplemental/Arial.ttf";
      if !std::path::Path::new(path).exists() {
          return None;
      }
      let data = FontData::from_file(path).ok()?;
      FontFace::from_data(&data, 0).ok()
  }
  ```
- `tests/test_context.rs` の `mod stroke_text` 内 (現状 L497-504) に同一実装が存在する。
- どちらも macOS 固有のシステムフォントパスをハードコードしている。
- 0001 メタ issue の Tracked items「テスト用フォント選定」が closed になれば、リポジトリ同梱フォントへの切り替えがあるが、それまでは Arial 依存が続く。

## 設計方針

以下のいずれかを選ぶ。

1. `tests/common/font.rs` を新規作成し、各テストファイルで `#[path = "common/font.rs"] mod common;` 経由で参照する。
2. `tests/common/mod.rs` を作成し、サブモジュール (`font`、将来追加する `pixel` 等) を `pub mod font;` でエクスポート。

統合テストでヘルパーを共有する Rust の慣習は `tests/common/mod.rs` パターンが標準。

加えて、パス文字列のハードコードを `RADEN_TEST_FONT_PATH` 等の環境変数経由に切り替える検討も含むかを polish 時に決定する。

## 完了条件

- `tests/common/` (または同等) 配下にフォントヘルパーが集約されている。
- `tests/test_font.rs` / `tests/test_context.rs` の重複定義が削除され、共通モジュールを参照している。
- 既存テスト全件が引き続き pass。
- 0025 / 0026 で追加されるテストでも同じヘルパーを共有できる構造になっている。

## 解決方法

polish 段階で確定する。
