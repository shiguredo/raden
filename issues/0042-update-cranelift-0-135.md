# Cranelift を 0.135 へ更新する

- Priority: Medium
- Category: update
- Created: 2026-08-29
- Completed: 2026-08-29
- Branch: feature/update-cranelift-0-135
- Polished: 2026-08-29

## 目的

`Cargo.toml` の `[dependencies]` で `~0.133` に固定している cranelift 5 クレート (`cranelift-codegen` / `cranelift-frontend` / `cranelift-jit` / `cranelift-module` / `cranelift-native`) を最新安定版 0.135.1 へ更新する。0.134 で入った API 変更へのコード追従を 1 つの単位として切り出し、cranelift の更新が再び可能な状態に戻す。

## 優先度根拠

Medium。現行の 0.133.3 のままでもビルドとテスト (179 件) はすべて通っており、動作上の不具合はない。cranelift 由来ではない clippy の新 lint 指摘 (`src/api/pattern.rs` の `Pattern::prepare` の `chunks_exact_to_as_chunks`) が 1 件あったが、これは 2026-08-29 に `as_chunks` へ置き換えて解消済みで、`cargo clippy --workspace --all-targets -- -D warnings` は通る。ただし cranelift は描画パイプラインの JIT コンパイルという中核部分そのもので、0.134 系列以降へ上げる手段は本 issue のコード追従以外にないことが実測で確定した。追従を置いたままにすると cranelift の更新が恒久的にブロックされ、系列が開くほど更新コストが増える。前回の 0.133 更新 (`b1339aa`) も `MemFlags` から `MemFlagsData` への追従を伴っており、同種の作業は繰り返されている。

## 現状

### 0.135 への更新はコード修正なしに成立しない

`~0.135` (0.135.1) と `rust-version = "1.95"` でビルドした結果、`cargo build` は 19 件のエラーで失敗し、511 件の警告が出た。

- エラー: すべて `E0061` (`this method takes 1 argument but 0 arguments were supplied`)。`src/pipeline/compiler/` 配下の 7 ファイルに集中する
- 警告: すべて `use of deprecated method cranelift_codegen::ir::InstBuilder::*_imm`

cranelift 5 クレートの要求範囲を `*` に緩めて `rust-version = "1.94"` のまま `cargo generate-lockfile` を実行すると、5 クレートとも `Adding cranelift-codegen v0.134.4 (available: v0.135.1, requires Rust 1.95.0)` の案内で 0.134.4 が選ばれ、MSRV 考慮が 0.135 系を候補から落としていることが確認できる。要求範囲が `~0.133` の間は semver 範囲そのものが 0.135 を除外しており、既に 0.135.1 へロックした lockfile へ `~0.133` のマニフェストに戻して `cargo update -p cranelift-codegen --precise 0.135.1` を実行すると `Downgrading cranelift-codegen v0.135.1 -> v0.133.3 (available: v0.135.1, requires Rust 1.95.0)`（他 4 クレートは `Downgrading ... (available: v0.134.4)`）が出る。

要求範囲を `~0.135` に書き換えると、`rust-version = "1.94"` をそのままでも `cargo generate-lockfile` は 0.135.1 を解決できる (`Adding cranelift-codegen v0.135.1 (requires Rust 1.95.0)` と注記されるだけで失敗はしない)。つまり `rust-version` の引き上げは解決を通すためではなく、cranelift 0.135 が要求する rustc 1.95 を正しく宣言するために必要である。`rust-version = "1.94"` のまま 0.135.1 をロックすると、MSRV 宣言が実態と食い違った状態になる。

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
- 波及規模: `bcx` を値で受け取る `build_*` 関数は 68 個で、うち 19 個が finalize を呼ぶ（`blend_build.rs` 32 / `porter_duff.rs` 24 / `core_pipelines.rs` 4 / `box_pipelines.rs` 2 / `gradient_pipelines.rs` 2 / `span_pipelines.rs` 2 / `sweep.rs` 1 / `transform.rs` 1）。`src/pipeline/compiler/mod.rs` から `build_*` を呼んでいるのは 68 か所で、いずれも `ptr_type` を実引数に渡している。加えて `blend_build.rs` と `porter_duff.rs` の中に、`build_*` から別の `build_*` へ `ptr_type` をそのまま渡す中継呼び出しが 49 か所ある（1 行完結 21 と複数行の実引数 28）。`bcx` を値で受ける `build_*` 68 個は、実測で「他 `build_*` へ `ptr_type` を中継するだけの 49 個」「finalize を呼び、かつ `ptr_type` を値として使う 17 個」「finalize を呼ぶが `_ptr_type` 未使用引数の 2 個」に完全に分かれる（このいずれにも入らない関数は 0 個）。`ptr_type: Type` を受ける関数定義は 69 個（68 個の `build_*` と、`src/pipeline/compiler/gradient_pipelines.rs` の `emit_lut_lookup` 1 個。うち 2 個は後述の `_ptr_type` 未使用引数）、`bcx` を `&mut FunctionBuilder` で受ける IR ヘルパーは 72 個。`src/pipeline/compiler/` 全体で `ptr_type` を参照する行は 458 行
- 置き換えの例外が 1 種ある。`src/pipeline/compiler/porter_duff.rs` の `build_dst_copy` / `build_dst_copy_cov` は引数が `_ptr_type: Type` で関数本体では一切使われないが、finalize を呼ぶ 19 箇所に含まれる（アンダースコア付き `ptr_type` 引数は実コードでこの 2 個のみ）。詳細は設計方針 1 の「置き換え手順」参照
- import の連鎖も別途必要になる。`Type` を import しているのは 8 ファイル（`blend_build.rs` / `box_pipelines.rs` / `core_pipelines.rs` / `gradient_pipelines.rs` / `porter_duff.rs` / `span_pipelines.rs` / `sweep.rs` / `transform.rs`）で、複数のファイルは `MemFlagsData` / `Value` などと同じ import 行に並んでいる。置き換え後に `TargetFrontendConfig` の import を追加するファイルも同じ 8 ファイルだが、`TargetFrontendConfig` は `cranelift_codegen::isa` 側にある（`Type` は `cranelift_codegen::ir`）ため、追加先は既存行に括り付けられない

### 変更点 2: `InstBuilder` の `*_imm` 非推奨化

0.134 で `*_imm` 系メソッドが非推奨となり、`*_imm_s`（即値を符号拡張）と `*_imm_u`（即値をゼロ拡張）へ分離された。cranelift-codegen-meta 0.135.1 の `src/gen_inst.rs` にあるジェネレータ `gen_imm_inst_builder` の doc コメントは「`_imm_s` variant sign-extends the immediate while `_imm_u` zero-extends it; this only matters for `i128`」＝ `_s` / `_u` の差は `i128` でだけ意味を持つと述べている（生成物 `inst_builder.rs` 側にこの説明は現れない。実体はジェネレータ側のコメントである）。raden が使っているのは `ushr_imm` / `ishl_imm` / `band_imm` / `sshr_imm` の 4 種である。

raden の使用箇所は 511 箇所で、ビルド警告 511 件と一致する（`src/pipeline/compiler/sweep.rs` の doc コメント内の言及 2 件を除いた実コード数）。

- API 別: raden が使うのは `ushr_imm` 364 / `ishl_imm` 59 / `band_imm` 86 / `sshr_imm` 2 の 4 種。0.133.3 の `InstBuilder` に生えている `*_imm` は 15 種 (`band_imm` / `bor_imm` / `bxor_imm` / `iadd_imm` / `icmp_imm` / `imul_imm` / `ishl_imm` / `rotl_imm` / `rotr_imm` / `sdiv_imm` / `srem_imm` / `sshr_imm` / `udiv_imm` / `urem_imm` / `ushr_imm`) で、残る 11 種は不使用
- ファイル別: `porter_duff.rs` 175 / `blend_modes.rs` 133 / `core_pipelines.rs` 76 / `span_pipelines.rs` 66 / `gradient_pipelines.rs` 35 / `box_pipelines.rs` 15 / `mod.rs` 6 / `sweep.rs` 5
- 即値はすべて非負のリテラル（1 / 2 / 3 / 4 / 8 / 9 / 16 / 24 / 0xF / 0xFF / 511）。負の値・変数・定数式は 0 件
- 使用する型は `src/pipeline/compiler/` 全体で `I32` / `I32X4` / `F32X4` / `F32` / `I64` / `F64X2` / `I8` / `F64` / `I8X16` / `I16X8` の 10 種（制御型ではなくモジュールで使う型の一覧）で、`i128` は `src/` 全体で 0 件

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

- 採用理由: 引数の数を増やさずに 68 個の `build_*` を経由して finalize を呼ぶ 19 か所へ `TargetFrontendConfig` を届けられる。既存の `ptr_type` 受け渡し経路がそのまま流用でき、`ptr_type` と `TargetFrontendConfig` の二重管理が発生しない
- 採らない案 1: `ptr_type` を残したまま `frontend_config` を追加引数にする案。68 個の `build_*` のシグネチャと、`src/pipeline/compiler/` 全体で 458 行ある `ptr_type` の受け渡し行のほとんどに 1 引数が増え、`ptr_type` が `frontend_config` から導出される値である関係を呼び出し側に要求し続けることになる
- 採らない案 2: `src/pipeline/compiler/mod.rs` 側で finalize を集約し、build 関数が `bcx` を返す設計にする案。`finalize(mut self, ...)` は `self` を move するため、中継 `build_*` 群の戻り値設計まで変えることになり、今回の更新の目的を超えて追従範囲が膨らむ

`src/pipeline/compiler/mod.rs` の 9 メソッドは現状 `let ptr_type = module.target_config().pointer_type();` の 1 式で受けている。これを `let frontend_config = module.target_config();` と `let ptr_type = frontend_config.pointer_type();` の 2 行に分割し、`sig.params.push(AbiParam::new(ptr_type))` は従来どおり `ptr_type` を使い、build 関数には `frontend_config` を渡す。

置き換え手順（実測の例外と波及先を明記する）:

1. `bcx` を値で受け取る `build_*` 68 個の `ptr_type: Type` を `frontend_config: TargetFrontendConfig` に置き換える。実引数の書き換えは `mod.rs` の 68 か所だけでは済まない。`build_*` 同士の中継呼び出しが 49 か所（`blend_build.rs` 32 と `porter_duff.rs` 17。うち 1 行完結が 21 か所、複数行の実引数が 28 か所）あり、これらも `frontend_config` を渡す形に変わる。中継先は `build_generic_compose` / `build_generic_compose_cov` の 2 関数で、`mod.rs` からは呼ばれていない（`build_flags` / `build_isa` は `src/pipeline/compiler/mod.rs` 内の別関数で、上の 68 か所には含めない）
2. `ptr_type` を中継実引数としてではなく値として使っている関数の冒頭に `let ptr_type = frontend_config.pointer_type();` を追加する。必要なのは 17 関数で、finalize を呼ぶ 19 のうち `porter_duff.rs` の `build_dst_copy` / `build_dst_copy_cov` を除いた 17 個（`build_generic_compose` / `build_generic_compose_cov` も `iconst(ptr_type, 0)` などで使うため含まれる）。`bcx` を値で受ける `build_*` 68 個のうち残る 49 個（`blend_build.rs` 32 と `porter_duff.rs` 17）は、本体での `ptr_type` の使われ方が他 `build_*` への実引数だけなので、`let ptr_type` を追加してはならない。追加すると未使用変数の警告になり `-D warnings` に抵触する。中継専用関数と値として使う関数が本体で併存するケースは実測でない（49 と 17 と 2 のいずれにも入らない関数は 0 個）
3. 上記 2 個の未使用引数は `_ptr_type: Type` を `frontend_config: TargetFrontendConfig`（アンダースコアなし）に置き、`let ptr_type = ...` は追加せず末尾の `bcx.finalize(frontend_config);` でだけ使う。実引数となる時点で未使用ではないため `unused_variables` 警告は出ない
4. `Type` を使わなくなった 7 ファイル（`blend_build.rs` / `box_pipelines.rs` / `core_pipelines.rs` / `porter_duff.rs` / `span_pipelines.rs` / `sweep.rs` / `transform.rs`）から `use cranelift_codegen::ir::...` の `Type` を除去する。`gradient_pipelines.rs` は `emit_lut_lookup` が `ptr_type: Type` を受けたまま残るため import を維持する
5. `build_*` を定義する上記 8 ファイルへ `cranelift_codegen::isa::TargetFrontendConfig` の import を追加する。`TargetFrontendConfig` は `ir` ではなく `isa` にあり（0.133.3 でも同じ）、`Type` の `cranelift_codegen::ir::Type` とは別モジュールなので、import 行は増えるか `use cranelift_codegen::{ir, isa}` の形に寄せることになる。`mod.rs` は `module.target_config().pointer_type()` を 1 式で繋いで型名を字面に出さないため追加不要

関数本体の `iconst(ptr_type, ...)` / `append_block_param(block, ptr_type)` / `emit_lut_lookup(&mut bcx, ..., ptr_type, ...)` などの `ptr_type` 参照は、手順 2 の `let` により解決先が変わるだけで字面は変更不要。`bcx` を `&mut FunctionBuilder` で受ける IR ヘルパー 72 個のうち `ptr_type: Type` を受けるのは `emit_lut_lookup` 1 個だけで、このシグネチャは維持する（呼び出し元である `gradient_pipelines.rs` の `build_radial_row_opaque` / `build_linear_gradient_cov_opaque` が手順 2 で `ptr_type` を取るため、実引数はそのまま）。残る 71 個は `ptr_type` を受け取らないため変更しない。

実引数 `ptr_type` を `frontend_config` へ語長の長い名前へ置き換えると、1 行完結の呼び出しが rustfmt の行幅ヒューリスティックを超えるため、手順適用後に `cargo fmt --all` を 1 回かける必要がある（置換直後に `cargo fmt --all --check` を先に走らせると差分で落ちる）。

`Type` / `TargetFrontendConfig` の import 不整合は `cargo build --workspace` の警告（未使用 import）と clippy `-D warnings` で必ず表面化するが、pre-commit フックで最初から通すため 4 と 5 を置き換え手順に含める。

### 2. `*_imm` は `_imm_u` へ一律置換する

全 511 箇所の即値が非負リテラルで、raden は `i128` を使わないため `_imm_s` と `_imm_u` の生成結果は同じである。加えて、現行 0.133.3 の `cranelift-codegen` が持つ `InstBuilderBase::build_imm_const` は `base_opcode` が `Iadd` / `Imul` / `Sdiv` / `Srem` / `Icmp` のときだけ `Sextend` を選び、それ以外（raden が使う `band` / `ushr` / `ishl` / `sshr` を含む）は `Uextend` である。しかも `Sextend` / `Uextend` の分岐は `controlling_type == types::I128` のときだけ通る経路で、それ以外の型では即値を lane 型で mask して `iconst` を作るのみであり、拡張種別は生成結果に関与しない（0.135.1 の同メソッド doc は「narrower types では `signed` フラグは効果を持たない」と明記している）。0.135.1 で生える非推奨 `*_imm` 4 種も同じゼロ拡張経路を呼ぶため、`_imm_u` への置換は生成コードを一切変えない。`ins().band_imm(x, 0xFF)` を `ins().band_imm_u(x, 0xFF)` のように置換する。

`#[expect(deprecated)]` による局所抑制は採らない。`shiguredo-rust` は lint 抑制について「`#[allow(...)]` を使わないこと（例外なし）」「抑制が必要なときは必ず `#[expect(...)]` を使うこと」を定めるだけで、`AGENTS.md` / `CLAUDE.md` に lint 抑制の規定はない。したがって 511 か所への `#[expect(deprecated)]` は規約違反ではなく、採らない理由は純粋に保守性の判断である。生成結果が同一だと確認できた以上、抑制を残すより実置換する方が明確に優れており、抑制の位置管理という恒久的な負担を増やす理由がない。

`src/pipeline/compiler/sweep.rs` の doc コメント 2 箇所（`emit_fill_rule_convert` の命令列説明にある `band_imm(511)` と、`build_sweep` のアルゴリズム説明にある `sshr_imm(9)`）は命令名の記述なので、実装とズレないように併せて更新する。

### 3. MSRV を 1.95 に引き上げる

`Cargo.toml` / `pbt/Cargo.toml` の `rust-version` を 1.95 にし、`README.md` の「制約」と `skills/raden/SKILL.md` の「依存関係」の表記を 0.135 / 1.95 へ更新する。MSRV 引き上げは利用者にとって後方互換のない変更なので `CHANGES.md` では `[CHANGE]` として扱い、cranelift 更新そのものは `[UPDATE]` として記載する。時雨堂の Rust 規約が定める MSRV は 1.93 で、現行 1.94 に続き 1.95 へ上げるのも依存クレートの要求に基づく例外運用になる。前回の 0.133 更新 (`b1339aa`) も同じ判断で 1.91 から 1.94 へ上げているが、このとき `CHANGES.md` の追記は行われていない。現在 `README.md` の MSRV 表記 (`0593273`) と `skills/raden/SKILL.md` の表記 (`4cf3ee9`) は 1.94 / `~0.133` で実装と一致しており、崩れているのは cranelift の版本そのものだけである。

### 4. 検証

`cargo build --workspace` / `cargo test --workspace` / `cargo fmt --all --check` / `cargo clippy --workspace --all-targets -- -D warnings` をすべて実行する。`*_imm` の置換対象 511 か所は `src/pipeline/compiler/` 配下の 8 ファイル、`bcx.finalize();` 19 か所は同 7 ファイルに閉じている（和集合は 9 ファイル。`transform.rs` は finalize のみ、`mod.rs` と `blend_modes.rs` は `*_imm` のみ）。`src/` の他ディレクトリ、`tests/` / `pbt/` / `benches/` / `examples/` に該当は 0 件。

性能確認は `Makefile` の 2 段構成（`bench-save` / `bench-compare`）で行う。criterion のベースラインは `target/criterion/` 配下に置かれ `.gitignore` で `target/` ごと無視されるため、リポジトリには既存のベースラインが存在しない。更新前のツリーで先に `make bench-save` を走らせて `before` を取得し、更新とコード追従が完了したツリーで `make bench-compare` を実行して比較する。既定の `make bench-save` / `make bench-compare` は `cargo bench --benches` で `Cargo.toml` の `[[bench]]` 10 本（`fill_rect` / `fill_rect_smooth` / `fill_rect_rotated` / `fill_polygon` / `fill_shape` / `stroke_rect` / `stroke_polygon` / `stroke_shape` / `fill_gradient` / `matrix`）を走らせるが、うち `matrix` は `benches/matrix.rs` が `Matrix2D` の演算だけを回し `Context` も `PipelineRuntime` も使わないため、cranelift の影響を受けない。比較の意味で絞るなら `BENCH=fill_rect` など JIT パイプラインを通る 9 本側のベンチ名を指定する。

`*_imm_u` への置換と `finalize` 引数追加は生成コードを変えないはずだが、`wasmtime-internal-core` 46 から 48、`memmap2` 0.2 から 0.9 などの追随が `Cargo.lock` 再生成で同時に入るため、比較で変動が出た場合はコード追従のせいではなく依存更新の帰属として切り分ける。数値の合格ラインは定めず、有意な劣化が確認されたら原因を調査して issue に追記する。

## 完了条件

- `Cargo.toml` の cranelift 5 クレートが `~0.135` で、`cargo build --workspace` がエラー 0・警告 0 で通ること
- `cargo test --workspace` が全件 pass すること (2026-08-29 時点で 179 件。`pbt` の PBT を含む)
- `cargo clippy --workspace --all-targets -- -D warnings` と `cargo fmt --all --check` が通ること
- `src/` 配下から引数なしの `bcx.finalize();` と非推奨 `*_imm` の呼び出しがなくなる（それぞれ 0 件）こと。確認は `bcx.finalize();` の完全一致と、`*_imm_u` / `*_imm_s` を除外した `*_imm` の grep で行うこと
- `rust-version` が 1.95 で、`README.md` と `skills/raden/SKILL.md` の MSRV / cranelift バージョン表記が実態と一致すること
- `CHANGES.md` に `[CHANGE]`（MSRV 1.95 化）と `[UPDATE]`（cranelift 0.135 更新）が記載されていること

## 変更対象ファイル

| ファイル | 種別 | 内容 |
|---|---|---|
| `Cargo.toml` | 既存編集 | cranelift 5 クレートを `~0.135`、`rust-version` を 1.95 |
| `Cargo.lock` | 既存編集 | `cargo update` で再生成 |
| `pbt/Cargo.toml` | 既存編集 | `rust-version` を 1.95 |
| `src/pipeline/compiler/mod.rs` | 既存編集 | `frontend_config` の取得と 68 か所の `build_*` への受け渡し、`emit_extract_channels_simd` / `emit_pack_channels_simd` の `*_imm_u` への置換 6 か所 |
| `src/pipeline/compiler/blend_build.rs` | 既存編集 | 中継 `build_*` 32 個の引数変更と中継呼び出しの実引数 32 か所、`Type` import の除去 |
| `src/pipeline/compiler/blend_modes.rs` | 既存編集 | `*_imm_u` への置換 |
| `src/pipeline/compiler/porter_duff.rs` | 既存編集 | `build_*` 24 個の引数変更（finalize を呼ぶ 7 と中継 17。`_ptr_type` 未使用引数の 2 個を含む）と中継呼び出しの実引数 17 か所、`*_imm_u` への置換 175 か所 |
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

`Cargo.toml` の cranelift 5 クレートを `~0.135`（0.135.1）へ、`Cargo.toml` と `pbt/Cargo.toml` の `rust-version` を 1.95 へ更新し、以下のコード追従を行った。テストコードの変更は不要だった（後述の「テストへの影響」参照）。

### 1. `FunctionBuilder::finalize` の引数追加への追従

設計方針 1 の置き換え手順 1〜5 を額面どおり適用した。

- `bcx` を値で受け取る `build_*` 68 個の `ptr_type: Type` 引数を `frontend_config: TargetFrontendConfig` に置き換えた
- `src/pipeline/compiler/mod.rs` の `compile*` 9 メソッドは `let ptr_type = module.target_config().pointer_type();` の 1 式を `let frontend_config = module.target_config();` と `let ptr_type = frontend_config.pointer_type();` の 2 行に分割した
- `mod.rs` からの `build_*` 呼び出し 68 か所と、`build_*` 同士の中継呼び出し 49 か所（`blend_build.rs` 32 / `porter_duff.rs` 17）の実引数を `ptr_type` から `frontend_config` に読み替えた
- `ptr_type` を値として使う 17 関数の本体先頭に `let ptr_type = frontend_config.pointer_type();` を追加した。中継専用の 49 関数は実引数としてだけ `frontend_config` を受け渡すため `let ptr_type` を追加しておらず、追加すれば未使用変数警告になって `-D warnings` を通らなかった（設計方針 1 の予想どおり）
- `build_dst_copy` / `build_dst_copy_cov` の `_ptr_type` 未使用引数は `frontend_config: TargetFrontendConfig`（アンダースコアなし）に置き、`bcx.finalize(frontend_config);` の実引数としてのみ使用した。実引数となる時点で未使用ではないため警告は出ない
- `Type` を使わなくなった 7 ファイルから import を除去した。`gradient_pipelines.rs` は `emit_lut_lookup` が `ptr_type: Type` を受けたまま残るため import を維持し、`build_*` を定義する 8 ファイルへ `cranelift_codegen::isa::TargetFrontendConfig` を追加した。`TargetFrontendConfig` は `ir` ではなく `isa` にある（0.133.3 でも同じ）ため、追加は既存 import 行に括り付けられない別行になった
- 実引数名が長くなったことで 1 行完結の呼び出しが rustfmt の行幅を超える箇所が出たため、`cargo fmt --all` を適用した（`blend_build.rs` と `porter_duff.rs` の差分行数が増えているのはこの折返しによる）

### 2. `InstBuilder` の `*_imm` 非推奨化への追従

`band_imm` / `ushr_imm` / `ishl_imm` / `sshr_imm` を `*_imm_u` へ 511 か所置換した（`src/pipeline/compiler/sweep.rs` の doc コメント内の命令名 2 箇所を含む計 513 箇所）。`#[expect(deprecated)]` による局所抑制は採用していない。

`sshr_imm_u` は符号付き右シフト命令 `sshr` の即値をゼロ拡張で materialize するという意味であり、命令の意味を変えていない。cranelift 0.135.1 の生成物では非推奨 `sshr_imm` も `sshr_imm_u` も同じく即値をゼロ拡張する（`historically_signed` は `iadd` / `imul` / `sdiv` / `srem` / `icmp` に限定される）。

### 3. MSRV 引き上げと表記追従

`Cargo.toml` / `pbt/Cargo.toml` の `rust-version` を 1.95 にし、`README.md` の「制約」と `skills/raden/SKILL.md` の「依存関係」の表記を `~0.135` / 1.95 へ更新した。CI は stable ツールチェーン参照のためワークフロー側の版本変更は不要だった。`cargo update` で cranelift 0.135.1 を解決し、`memmap2` 0.2.3 から 0.9.11、`wasmtime-internal-core` 46.0.3 から 48.0.1 の追随を取り込んだ。`Cargo.toml` の `[dependencies]` に用途コメントが無かったため、本次第で書き換えた 5 クレート分を機に追記した（時雨堂 Rust 規約）。

### 4. 生成コードへの影響の実測

`RADEN_DUMP_PIPELINE` による JIT アセンブリダンプを 3 条件で突き合わせた。

| 条件 | 生成コード | 既存テスト |
|---|---|---|
| 0.135.1 + `*_imm_u`（本次第） | 基準 | 179 件 pass |
| 0.135.1 + 非推奨 `*_imm` を意図的に復元 | 全ブロック一致 | 179 件 pass |
| 0.133.3 + `*_imm`（変更前） | 命令ミックス同一。`pipeline_box_*` のレジスタ割り当てと prologue のみ変化 | 179 件 pass |

`*_imm_u` への置換は生成コードを一切変えない。cranelift 版本差による生成コードの違いは box パイプラインのレジスタ割り当てに留まり、命令の並びは変わらなかった。

### 5. 性能比較

`make bench-save`（更新前ツリー）→ `make bench-compare`（更新後ツリー）で 151 測定ブロックを比較した。中央値 -0.18%、有意な回退 28 件と有意な改善 28 件が両方向に分散した。同一コードをそのまま再実行した底本で最大 8.23% の変動が出るほか、cranelift を通らない `Matrix2D` の 10 ブロックも揃って -0.7% 〜 -1.7% 側へ振れており、機械状態・実行順序に起因する変動と判断した。本追従による有意な性能劣化は確認していない。

### テストへの影響

`tests/` / `pbt/` / `benches/` / `examples/` に cranelift の直接参照は無く、公開 API 経由の既存テストが置換対象の命令列を実行している。`tests/test_render.rs` は `Context::fill_rect` 後のピクセル値を完全一致で検証し、`pbt` は JIT sweep と Rust リファレンスの出力完全一致を性質検証しているため、生成コードが崩れた場合に捕まえる経路が通っている。cranelift は private 依存であり、規約上も公開 API 経由以外にテストを書けない。よって新規テストは追加していない。

### 見送った事項

- `src/pipeline/compiler/mod.rs` の `compile_span` / `compile_span_cov` に両アームが同一式の `match` が残っている。develop からの既存構造で本次第の行書き換えを踏んでいるが、除去は cranelift 追従とは別の論理変更になるため本コミットに含めない
- `frontend_config` を素通しする中継 `build_*` 49 個は、`mod.rs` から `compose_*` 関数ポインタを直接渡せば畳める。ただし本次第で導入した構造ではなく、設計方針 1 の「採らない案 2」と同じ理由で範囲外とした
