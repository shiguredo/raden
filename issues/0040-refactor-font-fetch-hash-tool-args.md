# `compute_sha256` の引数構築を `hash_tool` 側に集約する

- Priority: Low
- Created: 2026-06-22
- Completed: {YYYY-MM-DD}
- Model: Opus 4.7
- Branch: feature/refactor-font-fetch-hash-tool-args
- Polished: {YYYY-MM-DD}

## 目的

`tests/helpers/font_fetch.rs` の `compute_sha256` における引数構築を `hash_tool()` 側に集約し、全 OS の引数列が `hash_tool()` の戻り値だけで完結する形に整える。現状は Windows 用の末尾 `SHA256` 引数だけが `compute_sha256` 関数内の `#[cfg(target_os = "windows")] command.arg("SHA256");` でインラインに追加されており、引数構築の責務が `hash_tool()` と `compute_sha256` の 2 箇所に分散している。

可読性 / 保守性の改善のみで、動作上の不具合はない。

## 優先度根拠

Low。動作上は正常で 0039 のマージ時点で CI 全 7 構成 (`ubuntu-24.04` / `ubuntu-24.04-arm` / `ubuntu-22.04` / `ubuntu-22.04-arm` / `macos-26` / `macos-15` / `windows-2025`) が pass 済み。テスト基盤のヘルパー内部の責務分散指摘であり、ユーザー影響もない。後続 issue (0025 / 0026 / 0027 / 0037) の着手前提を阻害しない。

## 現状

`tests/helpers/font_fetch.rs` の該当箇所:

- L380-396: `hash_tool()` を OS 別に 3 つ定義し、それぞれ `(cmd: &'static str, fixed_args: &'static [&'static str])` のタプルを返す
  - macOS: `("shasum", &["-a", "256"])`
  - Linux: `("sha256sum", &[])`
  - Windows: `("certutil", &["-hashfile"])`
- L405-415: `compute_sha256` が 3 段で引数を組み立てる
  - L412 `command.args(fixed_args);` で固定引数
  - L413 `command.arg(path_str);` で対象ファイルパス
  - L414-415 `#[cfg(target_os = "windows")] command.arg("SHA256");` で Windows のみ末尾 algorithm

問題点:

- `fixed_args` という命名が「コマンド固有の固定引数 = これだけで完結する」と誤読されやすい
- `compute_sha256` を読み終わるまで「Windows では `path` の後にさらに `SHA256` を渡す」事実が見えない
- 引数構築の責務が `hash_tool()` と `compute_sha256` に分散しているため、新しい OS を追加する際に 2 箇所を更新する必要がある

経緯: 0039 の `/review-diff-code` レビューで重要指摘 (H9) として浮上した。0039 では動作優先で見送り、本 issue として切り出した。

## 設計方針

`hash_tool()` の戻り値だけで全 OS の引数列が完結する形に変える。`compute_sha256` 側の `#[cfg(target_os = "windows")]` 分岐を消す。

採用方式の候補は 2 つあり、polish で確定する:

### 方式 A: `hash_tool(path: &str)` に path を取り込む

```rust
#[cfg(target_os = "windows")]
fn hash_tool(path: &str) -> (&'static str, Vec<String>) {
    ("certutil", vec!["-hashfile".to_string(), path.to_string(), "SHA256".to_string()])
}
```

`compute_sha256` 側は `let (cmd, args) = hash_tool(path_str); Command::new(cmd).args(args).output()` の 1 行で済む。短所はヒープ allocation が増えること (テストヘルパーなので実質無影響)。

### 方式 B: 構造体で leading / trailing を分ける

```rust
struct HashCommand {
    cmd: &'static str,
    leading_args: &'static [&'static str], // path 前
    trailing_args: &'static [&'static str], // path 後
}

#[cfg(target_os = "windows")]
fn hash_tool() -> HashCommand {
    HashCommand {
        cmd: "certutil",
        leading_args: &["-hashfile"],
        trailing_args: &["SHA256"],
    }
}
```

`compute_sha256` 側は `let HashCommand { cmd, leading_args, trailing_args } = hash_tool(); Command::new(cmd).args(leading_args).arg(path_str).args(trailing_args).output()` で組み立てる。長所はヒープ allocation なし / 短所は構造体が増えること。

判断材料:

- 方式 A はシンプルだが `Vec<String>` のヒープ allocation が発生
- 方式 B はゼロアロケーションを維持できるが構造体定義が増える
- テストヘルパーで毎テスト 1 回だけ呼ばれる経路のため、性能差は実質ゼロ。可読性で選ぶならどちらでも良い

## 完了条件

- `tests/helpers/font_fetch.rs` の `compute_sha256` 関数内に `#[cfg(target_os = "windows")] command.arg("SHA256");` 行が存在しない
- 全 OS 用の引数構築責務が `hash_tool()` 1 箇所に集約されている
- 公開 API 変更なし
- `cargo fmt --all --check` 通過
- `cargo clippy --workspace --all-targets -- -D warnings` 通過
- `cargo test --workspace` 通過 (`tests/test_font.rs` 14 件含む)
- CI 全 7 構成で pass
- `CHANGES.md` 更新は不要 (テスト基盤の内部リファクタで公開 API 変更なし / 描画結果変化なし)

## 解決方法

設計方針セクションの方式 A / 方式 B のいずれかを polish で確定し、`tests/helpers/font_fetch.rs` をリファクタする。

## 変更対象ファイル

| ファイル | 種別 | 内容 |
|---|---|---|
| `tests/helpers/font_fetch.rs` | 既存編集 | `hash_tool()` の戻り値を全 OS の引数列を含む形に変更し、`compute_sha256` 内の `#[cfg(target_os = "windows")] command.arg("SHA256");` 分岐を削除 |

## 関連

- `issues/closed/0039-add-test-font-downloader.md` (本 issue の発端となった `/review-diff-code` レビュー指摘 H9)
