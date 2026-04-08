# clear_all / clear_rect を追加する

Created: 2026-04-09
Model: Opus 4.6

## 概要

`Context::clear_all` / `Context::clear_rect` が未実装。現状は `set_comp_op(CompOp::Clear)` + `fill_all` / `fill_rect` で代替する必要があり、フレーム初期化のたびに状態保存・復元が要求されてエルゴノミクスが悪い。

## 根拠

- ダミー映像生成は毎フレーム背景クリアから始まるユースケースが中心で、最頻出操作の一つ
- Blend2D は `clear_all` / `clear_rect` を備えており API 準拠の観点でも欠落
- `comp_op` を切り替えずに済むため、ユーザコードが現在の合成モードを意識しなくて良くなる

## 想定スコープ

- `Context::clear_all()` / `Context::clear_rect(&Rect)` を追加
- 内部実装は現在の `comp_op` を一時退避して `Clear` で fill する形でよい（ただし state stack には触れない）
- クリッピングが有効な場合の挙動 (Blend2D は clip 内のみクリア) を確認
- 単体テスト: クリア後のピクセルが透明になっていること、clip 範囲外が保持されていること
- `docs/BLEND2D.md` および `CHANGES.md` の更新
