# pbt

Raden の Property-Based Testing (PBT) クレート。

[noprop](https://crates.io/crates/noprop) を使用して、各モジュールの不変条件やラウンドトリップ性質を検証する。

## テストファイル命名規則

- `pbt/tests/prop_<module>.rs` — `src/<module>.rs` に対応
- ディレクトリモジュールの場合は `pbt/tests/prop_<module>/main.rs` でサブモジュール分割

## 実行方法

```bash
cargo test -p pbt
```

失敗を再現する場合は環境変数 `RADEN_PBT_SEED` に失敗レポートのシードを指定する。

```bash
RADEN_PBT_SEED=<seed> cargo test -p pbt --test prop_<module> <test_name> -- --exact
```

## カバレッジ取得

```bash
cargo llvm-cov clean --workspace
cargo llvm-cov --no-report -p pbt --test prop_<module>
cargo llvm-cov report
```

## PBT の役割

- noprop のサンプラー (`sample_*`) で入力を生成し、プロパティを検証する
- ラウンドトリップ、不変条件の保持、数値演算の性質などが対象
- パニック安全性の検証は fuzzing の役割であり、PBT では扱わない
