# pbt

Raden の Property-Based Testing (PBT) クレート。

[proptest](https://crates.io/crates/proptest) を使用して、各モジュールの不変条件やラウンドトリップ性質を検証する。

## テストファイル命名規則

- `pbt/tests/prop_<module>.rs` — `src/<module>.rs` に対応
- ディレクトリモジュールの場合は `pbt/tests/prop_<module>/main.rs` でサブモジュール分割

## 実行方法

```bash
cargo test -p pbt
```

## カバレッジ取得

```bash
cargo llvm-cov clean --workspace
cargo llvm-cov --no-report -p pbt --test prop_<module>
cargo llvm-cov report
```

## PBT の役割

- 型情報 (Strategy) に基づいてランダム入力を生成し、プロパティを検証する
- ラウンドトリップ、不変条件の保持、数値演算の性質などが対象
- パニック安全性の検証は fuzzing の役割であり、PBT では扱わない
