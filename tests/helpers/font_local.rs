//! ローカルシステムフォントを読み込む共有ヘルパー。
//!
//! CI 等で該当フォントが存在しない環境では `None` を返し、テストをスキップする。

use raden::{FontData, FontFace};

/// macOS の Arial.ttf を読み込む。
///
/// `/System/Library/Fonts/Supplemental/Arial.ttf` が存在しない場合は `None` を返す。
pub fn load_arial() -> Option<FontFace> {
    let path = "/System/Library/Fonts/Supplemental/Arial.ttf";
    if !std::path::Path::new(path).exists() {
        return None;
    }
    let data = FontData::from_file(path).ok()?;
    FontFace::from_data(&data, 0).ok()
}
