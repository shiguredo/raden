# Cranelift を 0.135 へ更新する

- Priority: Medium
- Category: update
- Created: 2026-08-29
- Completed: {YYYY-MM-DD}
- Branch: feature/update-cranelift-0-135
- Polished: {YYYY-MM-DD}

## 目的

`Cargo.toml` の `[dependencies]` で `~0.133` に固定している cranelift 5 クレート (`cranelift-codegen` / `cranelift-frontend` / `cranelift-jit` / `cranelift-module` / `cranelift-native`) を最新安定版 0.135.1 へ更新する。0.134 で入った API 変更へのコード追従を 1 つの単位として切り出し、cranelift の更新が再び可能な状態に戻す。

## 優先度根拠

Medium。現行の 0.133.3 のままでもビルドとテスト (179 件) はすべて通っており、動作上の不具合はない。cranelift 由来ではない clippy の新 lint 指摘 (`src/api/pattern.rs` の `Pattern::prepare` の `chunks_exact_to_as_chunks`) が 1 件あったが、これは 2026-08-29 に `as_chunks` へ置き換えて解消済みで、`cargo clippy --workspace --all-targets -- -D warnings` は通る。ただし cranelift は描画パイプラインの JIT コンパイルという中核部分そのもので、0.134 系列以降へ上げる手段は本 issue のコード追従以外にないことが実測で確定した。追従を置いたままにすると cranelift の更新が恒久的にブロックされ、系列が開くほど更新コストが増える。前回の 0.133 更新 (`b1339aa`) も `MemFlags` から `MemFlagsData` への追従を伴っており、同種の作業は繰り返されている。

## 現状

### 0.135 への更新はコード修正なしに成立しない

`~0.135` (0.135.1) と `rust-version = "1.95"` でビルドした結果、`cargo build` は 19 件のエラーで失敗し、511 件の警告が出た。

- エラー: すべて `E0061` (`this method takes 1 argument but 0 arguments were supplied`)。`src/pipeline/compiler/` 配下の 7 ファイルに集中する
- 警告: すべて `use of deprecated method cranelift_codegen::ir::InstBuilder::*_imm`

`cargo update` は MSRV を考慮して 0.135 系を候補から除外するため (`available: v0.134.4` と案内される)、`rust-version` を上げない限り 0.135.1 は解決されない。

### 中間段階としての 0.134 は意味がない

`~0.134` (0.134.4) でも同じ 19 エラー・511 警告になることを実測で確認した。`FunctionBuilder::finalize` の引数追加も `*_imm` の非推奨化も 0.134 で既に入っている。0.134 に留める利点は MSRV が 1.94 のまま据えられる点だけで、コード修正量は 0.135 と同一である。

### 変更点 1: `FunctionBuilder::finalize` の引数追加

0.133 系は `pub fn finalize(mut self)`、0.134 系以降は `pub fn finalize(mut self, frontend_config: TargetFrontendConfig)`（crates.io から展開した crate ソースで確認）。0.135.1 の実装で `frontend_config` が使われるのは safepoint 用 stack map 生成のパス (`self.func_ctx.safepoints.run(...)`) だけである。raden は safepoint / stack map / `try_call` / `sequence_point` を一切使っていないため、追加される引数の受け渡しは意味論的には機械的に済む。

`bcx.finalize();` を引数なしで呼んでいるのは 19 箇所。いずれも `bcx` を値で受け取る `pub(super) fn build_*` の末尾で、18 箇所は `bcx.seal_all_blocks();` の直後、残る 1 箇所 (`build_transform_edges`) は個別の `seal_block` 呼び出しの後である。

| ファイル | 件数 | 関数 |
|---|---|---|
| `src/pipeline/compiler/porter_duff.rs` | 7 | `build_clear` / `build_dst_copy` / `build_generic_compose` / `build_plus` / `build_clear_cov` / `build_dst_copy_cov` / `build_generic_compose_cov` |
| `src/pipeline/compiler/core_pipelines.rs` | 4 | `build_src_copy` / `build_src_copy_cov` / `build_src_over_cov` / `build_src_over` |
| `src/pipeline/compiler/span_pipelines.rs` | 2 | `build_src_over_span` / `build_src_over_span_cov` |
| `src/pipeline/compiler/gradient_pipelines.rs` | 2 | `build_radial_row_opaque` / `build_linear_gradient_cov_opaque` |
| `src/pipeline/compiler/box_pipelines.rs` | 2 | `build_src_over_box` / `build_src_copy_box` |
| `src/pipeline/compiler/transform.rs` | 1 | `build_transform_edges` |
| `src/pipeline/compiler/sweep.rs` | 1 | `build_sweep` |

- 19 箇所のいずれにも `TargetFrontendConfig` は渡っておらず、スコープ内にも存在しない。build 関数が受け取っているのは `bcx` と `ptr_type: Type` など限られた値のみ
- 供給元は既に手元にある。`src/pipeline/compiler/mod.rs` の `compile*` 9 メソッド (`compile` / `compile_cov` / `compile_box` / `compile_sweep` / `compile_span` / `compile_span_cov` / `compile_linear_gradient_cov` / `compile_radial_row` / `compile_transform_edges`) が `module.target_config()` を呼んで `ptr_type` を得ている。`Module::target_config` は `TargetFrontendConfig` を値で返し、`TargetFrontendConfig` は `Clone + Copy` で `pointer_type(self)` を持つ
- 波及規模: `bcx` を値で受け取る `build_*` 関数は 68 個で、うち 19 個が finalize を呼ぶ（`blend_build.rs` 32 / `porter_duff.rs` 24 / `core_pipelines.rs` 4 / `box_pipelines.rs` 2 / `gradient_pipelines.rs` 2 / `span_pipelines.rs` 2 / `sweep.rs` 1 / `transform.rs` 1）。`src/pipeline/compiler/mod.rs` から `build_*` を呼んでいるのは 68 か所で、いずれも `ptr_type` を実引数に渡している。`ptr_type: Type` を受ける関数定義は 69 個（68 個の `build_*` と、`src/pipeline/compiler/gradient_pipelines.rs` の `emit_lut_lookup` 1 個）、`bcx` を `&mut FunctionBuilder` で受ける IR ヘルパーは 72 個。`src/pipeline/compiler/` 全体で `ptr_type` を参照する行は 458 行

### 変更点 2: `InstBuilder` の `*_imm` 非推奨化

0.134 で `*_imm` 系メソッドが非推奨となり、`*_imm_s`（即値を符号拡張）と `*_imm_u`（即値をゼロ拡張）へ分離された。cranelift-codegen-meta 0.135.1 の生成コメントには「`_s` / `_u` の差は `i128` でだけ意味を持つ」と明記されている。raden が使っているのは `ushr_imm` / `ishl_imm` / `band_imm` / `sshr_imm` の 4 種である。

raden の使用箇所は 511 箇所で、ビルド警告 511 件と一致する（`src/pipeline/compiler/sweep.rs` の doc コメント内の言及 2 件を除いた実コード数）。

- API 別: raden が使うのは `ushr_imm` 364 / `ishl_imm` 59 / `band_imm` 86 / `sshr_imm` 2 の 4 種。0.133.3 の `InstBuilder` に生えている `*_imm` は 15 種 (`band_imm` / `bor_imm` / `bxor_imm` / `iadd_imm` / `icmp_imm` / `imul_imm` / `ishl_imm` / `rotl_imm` / `rotr_imm` / `sdiv_imm` / `srem_imm` / `sshr_imm` / `udiv_imm` / `urem_imm` / `ushr_imm`) で、残る 11 種は不使用
- ファイル別: `porter_duff.rs` 175 / `blend_modes.rs` 133 / `core_pipelines.rs` 76 / `span_pipelines.rs` 66 / `gradient_pipelines.rs` 35 / `box_pipelines.rs` 15 / `mod.rs` 6 / `sweep.rs` 5
- 即値はすべて非負のリテラル（1 / 2 / 3 / 4 / 8 / 9 / 16 / 24 / 0xF / 0xFF / 511）。負の値・変数・定数式は 0 件
- 使用する型は `I32` / `I32X4` / `F32X4` / `F32` / `I64` / `F64X2` / `I8` / `F64` / `I8X16` / `I16X8` で、`i128` は使わない

**`_imm_s` / `_imm_u` は 0.133 には存在しない**（0.133.3 のビルド生成物 `inst_builder.rs` には `*_imm` が 15 種並ぶだけで `_imm_s` / `_imm_u` も `#[deprecated]` も 0 種。0.134.4 と 0.135.1 のビルドでは同じ生成物に 3 種が併存し、警告文が `_imm_s` / `_imm_u` への移行を推奨することを確認した）。したがってこの移行はバージョン更新と同時にしか行えない。`.github/workflows/ci.yml` の `cargo clippy --workspace -- -D warnings`、および `prek.toml` の pre-commit フック `cargo clippy --workspace --all-targets -- -D warnings` を実行するため、非推奨 API を残したままではコミット自体ができず、更新をマージできない。

### 0.135 で影響が無かった部分

0.135 でのビルド結果、エラーは上記 `finalize` の 19 件のみだった。`cranelift_native::builder` / `isa_builder.finish` / `JITBuilder::with_isa` / `JITModule::new` / `default_libcall_names` / `module.make_signature` / `declare_function` / `make_context` / `define_function` / `clear_context` / `finalize_definitions` / `get_finalized_function` / `target_config` はそのまま通る。cranelift 0.133 の `InstBuilder` に存在する `stack_load` / `stack_store` / `global_value` / `band_not` / `bor_not` / `bxor_not` と `FunctionBuilder` の var 系 API (`declare_var` / `use_var`)、`Linker` は raden で不使用（いずれも grep で 0 件）。cranelift を参照するファイルは `src/pipeline/compiler/` 配下の 10 ファイルだけで、`tests/` / `pbt/` / `benches/` / `examples/` に直接利用はない。

### MSRV 引き上げの波及範囲

cranelift 0.133.3 と 0.134.4 の `rust_version` は 1.94.0、0.135.1 は 1.95.0（crates.io インデックスで確認）。0.135 に上げると以下の記載が実態とズレる。

- `Cargo.toml` と `pbt/Cargo.toml` の `rust-version = "1.94"`
- `README.md` の「制約」節の「Rust 1.94 以上が必要 (`Cargo.toml` の `rust-version`)」
- `skills/raden/SKILL.md` の「依存関係」節の「cranelift-codegen / ... (~0.133)」「Rust edition 2024 / rust-version 1.94」

CI は stable ツールチェーン参照（`rust-toolchain.toml` は `channel = "stable"`、`.github/workflows/ci.yml` も `stable`）のため、ワークフロー側の版本変更は不要。cranelift の更新に伴い間接依存は `wasmtime-internal-core` が 46 系から 48 系、`memmap2` が 0.2 系から 0.9 系へ追随する。

## 設計方針

### 1. `ptr_type` を `frontend_config` に置き換える

`bcx` を値で受け取る `build_*` 関数の `ptr_type: Type` 引数を `frontend_config: TargetFrontendConfig` に置き換える。`TargetFrontendConfig::pointer_type()` は `self` を取る Copy 型なので、`ptr_type` が必要な関数の冒頭で `let ptr_type = frontend_config.pointer_type();` を取ればよい。

- 採用理由: 引数の数を増やさずに 19 か所へ `TargetFrontendConfig` を届けられる。既存の `ptr_type` 受け渡し経路がそのまま流用でき、`ptr_type` と `TargetFrontendConfig` の二重管理が発生しない
- 採らない案 1: `ptr_type` を残したまま `frontend_config` を追加引数にする案。68 個の `build_*` のシグネチャと、`src/pipeline/compiler/` 全体で 458 行ある `ptr_type` の受け渡し行のほとんどに 1 引数が増え、`ptr_type` が `frontend_config` から導出される値である関係を呼び出し側に要求し続けることになる
- 採らない案 2: `src/pipeline/compiler/mod.rs` 側で finalize を集約し、build 関数が `bcx` を返す設計にする案。`finalize(mut self, ...)` は `self` を move するため、中継 `build_*` 群の戻り値設計まで変えることになり、今回の更新の目的を超えて追従範囲が膨らむ

`src/pipeline/compiler/mod.rs` の 9 メソッドでは `let frontend_config = module.target_config();` を取ってから `let ptr_type = frontend_config.pointer_type();` に置き、`sig.params.push(AbiParam::new(ptr_type))` は従来どおり `ptr_type` を使い、build 関数には `frontend_config` を渡す。

### 2. `*_imm` は `_imm_u` へ一律置換する

全 511 箇所の即値が非負リテラルで、raden は `i128` を使わないため `_imm_s` と `_imm_u` の生成結果は同じである。さらに cranelift-codegen-meta 0.135.1 の生成コードでは、非推奨の `*_imm` が旧命令の拡張挙動を維持する対象 (`historically_signed`) を `iadd` / `imul` / `sdiv` / `srem` / `icmp` に限定している。raden が使う `ushr` / `ishl` / `band` / `sshr` はこの対象外で、非推奨の `*_imm` は `_imm_u` と同じく即値をゼロ拡張して `iconst` を materialize する。したがって `_u` への置換は既存の挙動をそのまま保つ。`ins().band_imm(x, 0xFF)` を `ins().band_imm_u(x, 0xFF)` のように置換する。`#[expect(deprecated)]` による局所抑制は採らない（511 箇所の抑制は保守性を大きく損なう。AGENTS.md も lint の恒久抑制を嫌う）。

`src/pipeline/compiler/sweep.rs` の doc コメント 2 箇所（`emit_fill_rule_convert` の命令列説明にある `band_imm(511)` と、`build_sweep` のアルゴリズム説明にある `sshr_imm(9)`）は命令名の記述なので、実装とズレないように併せて更新する。

### 3. MSRV を 1.95 に引き上げる

`Cargo.toml` / `pbt/Cargo.toml` の `rust-version` を 1.95 にし、`README.md` の「制約」と `skills/raden/SKILL.md` の「依存関係」の表記を 0.135 / 1.95 へ更新する。MSRV 引き上げは利用者にとって後方互換のない変更なので `CHANGES.md` では `[CHANGE]` として扱い、cranelift 更新そのものは `[UPDATE]` として記載する。時雨堂の Rust 規約が定める MSRV は 1.93 で、現行 1.94 に続き 1.95 へ上げるのも依存クレートの要求に基づく例外運用になる。前回の 0.133 更新 (`b1339aa`) も同じ判断で 1.91 から 1.94 へ上げているが、このとき `CHANGES.md` の追記は行われていない。現在 `README.md` の MSRV 表記 (`0593273`) と `skills/raden/SKILL.md` の表記 (`4cf3ee9`) は 1.94 / `~0.133` で実装と一致しており、崩れているのは cranelift の版本そのものだけである。

### 4. 検証

`cargo build --workspace` / `cargo test --workspace` / `cargo fmt --all --check` / `cargo clippy --workspace --all-targets -- -D warnings` をすべて実行する。加えて `make bench` 相当で主要パイプラインの性能が大きく劣化していないことを確認する。`*_imm_u` への置換と `finalize` 引数追加は生成コードを変えないはずだが、`wasmtime-internal-core` 46 から 48 などの追随が入るため、前回ベースラインとの比較で見る。

## 完了条件

- `Cargo.toml` の cranelift 5 クレートが `~0.135` で、`cargo build --workspace` がエラー 0・警告 0 で通ること
- `cargo test --workspace` が全件 pass すること (2026-08-29 時点で 179 件。`pbt` の PBT を含む)
- `cargo clippy --workspace --all-targets -- -D warnings` と `cargo fmt --all --check` が通ること
- `src/` 配下から引数なしの `bcx.finalize();` と非推奨 `*_imm` の呼び出しがなくなる（それぞれ 0 件）こと
- `rust-version` が 1.95 で、`README.md` と `skills/raden/SKILL.md` の MSRV / cranelift バージョン表記が実態と一致すること
- `CHANGES.md` に `[CHANGE]`（MSRV 1.95 化）と `[UPDATE]`（cranelift 0.135 更新）が記載されていること

## 変更対象ファイル

| ファイル | 種別 | 内容 |
|---|---|---|
| `Cargo.toml` | 既存編集 | cranelift 5 クレートを `~0.135`、`rust-version` を 1.95 |
| `Cargo.lock` | 既存編集 | `cargo update` で再生成 |
| `pbt/Cargo.toml` | 既存編集 | `rust-version` を 1.95 |
| `src/pipeline/compiler/mod.rs` | 既存編集 | `frontend_config` の取得と 68 か所の `build_*` への受け渡し |
| `src/pipeline/compiler/blend_build.rs` | 既存編集 | 中継 `build_*` の引数変更 |
| `src/pipeline/compiler/blend_modes.rs` | 既存編集 | `*_imm_u` への置換 |
| `src/pipeline/compiler/porter_duff.rs` | 既存編集 | 引数変更と `*_imm_u` への置換 |
| `src/pipeline/compiler/core_pipelines.rs` | 既存編集 | 引数変更と `*_imm_u` への置換 |
| `src/pipeline/compiler/span_pipelines.rs` | 既存編集 | 引数変更と `*_imm_u` への置換 |
| `src/pipeline/compiler/box_pipelines.rs` | 既存編集 | 引数変更と `*_imm_u` への置換 |
| `src/pipeline/compiler/gradient_pipelines.rs` | 既存編集 | 引数変更と `*_imm_u` への置換 |
| `src/pipeline/compiler/sweep.rs` | 既存編集 | 引数変更と `*_imm_u` への置換（doc コメント 2 箇所を含む） |
| `src/pipeline/compiler/transform.rs` | 既存編集 | `build_transform_edges` の引数変更と finalize 引数追加 |
| `README.md` | 既存編集 | 「制約」の MSRV を 1.95 |
| `skills/raden/SKILL.md` | 既存編集 | 「依存関係」の cranelift と rust-version の表記 |
| `CHANGES.md` | 既存編集 | `[CHANGE]` と `[UPDATE]` の追記 |

## 関連

- 前回の同種作業: `b1339aa`「cranelift を 0.133 に更新する」（`MemFlags` から `MemFlagsData` への追従、MSRV 1.91 から 1.94 へ引き上げ、`skills/raden/SKILL.md` を同時更新）
- 本 issue は 2026-08-29 の依存更新作業で cranelift を `~0.133`（0.133.3）に留めて `Cargo.lock` のみ更新した結果として生じた残作業である。lockfile のみの更新は取り込み済み
- 他 issue の前提を阻害しない。`issues/0001-enhance-font-module-maturity.md` などの作業とは独立している

## 解決方法

polish 段階で確定する。
