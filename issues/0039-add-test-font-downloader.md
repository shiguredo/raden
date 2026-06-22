# テスト用フォントのダウンロード機構を追加する

- Priority: High
- Category: add
- Created: 2026-06-22
- Model: Opus 4.7
- Branch: feature/add-test-font-downloader
- Polished: 2026-06-22

## 目的

raden の font 関連テスト・PBT が macOS 限定 `Arial.ttf` に依存している状況を解消するため、CI 全プラットフォームで利用可能なテスト用フォント (Source Sans 3 / Source Serif 4) を **必要なタイミングで外部からダウンロードする仕組み** をリポジトリに追加する。

**前提ルール (raden プロジェクト共通)**: フォントバイナリ (.ttf / .otf / .ttc / .woff / .woff2) および LICENSE 全文は **リポジトリにコミットしない**。代わりに取得スクリプト・テストヘルパー等の「ダウンロードする仕組み」をリポジトリに置く。ダウンロード先キャッシュは `.gitignore` で除外する。

本 issue は 0001 メタ issue の Tracked items「テスト用フォント選定」を完了させる責務を持つ。0025 (kern fallback 動作確認) / 0026 (GSUB + GPOS) / 0027 (CFF) の各 concrete issue は本 issue が close されない限り着手前提を満たせない。本 issue は単独で closed 可能であり、0025 / 0026 / 0027 / 0037 の本文書き換え (リポジトリ同梱前提 → ダウンロード機構前提) は **本 issue closed 後にユーザーが `/polish-issue 0025 0026 0027 0037` を起動して各 issue 側の polish で確定する** (本 issue PR ではこれらの issue ファイルを編集しない。`shiguredo-issues` SKILL.md L59「`Polished:` を更新できるのは `/polish-issue` スキルのみ」に整合)。

スコープ外:

- 既存 `load_arial()` ヘルパー集約・撤去 (0037 のスコープ)
- 既存 Arial 依存テスト全件の切り替え (0037 およびその後続のスコープ)
- 0025 / 0026 / 0027 / 0037 issue 本文の書き換え (本 issue closed 後に各 issue を `/polish-issue` で個別に再 polish)
- `examples/raden_player.rs:189` の `/System/Library/Fonts/Helvetica.ttc` 直参照 (本 issue では扱わない)
- CJK 統合漢字 / BMP 外文字のカバレッジ (Source Sans 3 / Source Serif 4 はラテン系のみ。対応方針は完了条件「0001 メタ issue 更新」項目 8 で扱う)

## 優先度根拠

High。0025 / 0026 / 0027 の 3 つの concrete issue の着手前提であり、本 issue が close されない限り 0025 以降の auto-resolve が進められない。0001 メタ issue の完了条件「全プラットフォームで font テストが CI で動作する」の前提でもある。

## 現状

- リポジトリ内に `.ttf` / `.otf` / `.ttc` / `.woff` / `.woff2` は 1 件もコミットされていない (`git ls-files | grep -E '\.(ttf|otf|ttc|woff2?)$'` で 0 件)。
- `Cargo.lock` はリポジトリにコミット済み (`git ls-files | grep Cargo.lock` で `Cargo.lock` 1 件)。本 issue で `[dev-dependencies]` 追加に伴い `Cargo.lock` も更新コミットする。CI ワークフロー (`.github/workflows/ci.yml:48`) の `cargo test --workspace` には `--locked` フラグがないため `Cargo.lock` が不整合でも自動解決されるが、PR では明示コミットして再現性を担保する。
- `Arial.ttf` 直参照は 3 ファイル:
  - `tests/test_font.rs:7-14` (`load_arial()` 定義、`#[test]` 12 件すべてが利用)
  - `tests/test_context.rs:497-504` (`mod stroke_text` 内 `load_arial()` 定義、5 箇所で利用 — 510 / 540 / 593 / 617 / 635 行)
  - `pbt/tests/prop_font/main.rs:10-17` (`load_arial()` 定義、PBT 13 件すべてが `load_arial()` を呼ぶ)
- 上記はすべて macOS の `/System/Library/Fonts/Supplemental/Arial.ttf` を直接参照しており、Linux / Windows の CI ジョブではすべてスキップされている。
- `tests/common/` ディレクトリは未作成 (0037 で作成予定)。本 issue は `tests/common/font_fetch.rs` を新設する。
- 現状 raden が実装済みのフォントテーブルは `head` / `maxp` / `hhea` / `hmtx` / `loca` / `cmap` / `glyf` / `OS/2` および TTC (TrueType Collection) のみ。`src/font/tables.rs:113-120` には `TAG_HEAD` / `TAG_MAXP` / `TAG_HHEA` / `TAG_HMTX` / `TAG_LOCA` / `TAG_CMAP` / `TAG_GLYF` / `TAG_OS2` の 8 定数しか定義されておらず (TTC は同ファイル末尾近くの `TAG_TTCF`)、`TAG_KERN` / `TAG_GSUB` / `TAG_GPOS` / `TAG_CFF` / `TAG_CFF2` の TAG 定数および対応 parse 関数は **存在しない** ことから、これらテーブルは未パースである。
- `src/font/tables.rs:58-93` の `TableDirectory::parse` は sfnt version `0x0001_0000` (TrueType) と `0x7472_7565` (`'true'`、Apple TrueType variant) のみを受理する。`0x4F54544F` (`OTTO`、OpenType with CFF アウトライン) を渡すと L67 で `FontError::InvalidData("unsupported sfnt version")` を返す。CFF only OTF (Source Serif 4 等) は `parse_all` (L629) に到達する前に `TableDirectory::parse` の段階で弾かれる。`OTTO` 受理の追加と CFF アウトライン対応は 0027 のスコープ。
- `Cargo.toml:11` の `include = ["/LICENSE", "/README.md", "/src/**"]` はホワイトリスト方式 (`Cargo.toml:16` の `exclude = ["fuzz"]` は `include` 優先で無効化される)。`tests/` 配下と `.gitignore` の追記は `cargo publish` 配布物に混入しない。本 issue では `Cargo.toml` の `exclude` フィールドは触らない (`include` ホワイトリストで十分)。
- `raden::FontData::from_bytes(bytes: Vec<u8>) -> FontData` API は `src/font/mod.rs` (公開 API) に存在する。本 issue のスモークテストでこれを使う (実装着手時の API 不整合はない)。
- 0001 メタ issue の Tracked items「テスト用フォント選定」の完了基準 L142 (`全プラットフォームで利用可能なフォントがリポジトリに含まれる PR がマージ済み`) および完了条件「クロスプラットフォーム」L244 (`全プラットフォームで利用可能なテスト用フォントバイナリがリポジトリに含まれる`) は本 issue の方針 (リポジトリにコミットしない) と矛盾する。本 issue 完了 PR で書き換える (詳細は完了条件「0001 メタ issue 更新」参照)。

## 設計方針

### 方式選定: 方式 A (テストヘルパー内 ローカルキャッシュ) を採用する

`tests/common/font_fetch.rs` を新規作成し、初回利用時に HTTPS でダウンロード → SHA-256 検証 → `<CARGO_TARGET_TMPDIR>/test-fonts/` にキャッシュ保存する形を採る。

採用理由:

- `cargo test --workspace` 単独で完結し、CI / ローカル開発で機構の差を作らない。
- `Cargo.toml:11` の `include` ホワイトリストにより `tests/` 配下は `cargo publish` 配布物から自動除外される (テストヘルパーが publish 用パッケージに混入しない)。
- `build.rs` を増設しないため、本番ビルド時に余計なネットワーク I/O が走らない。
- 検証用 sfnt table directory 走査も同じヘルパー内で完結する (0025 / 0026 / 0027 の前提解消が後工程なしで成立する)。

不採用方式の理由 (各 1 文):

- **方式 B (`build.rs`)**: `Cargo.toml:11` の `include` ホワイトリストに `build.rs` が含まれていないため publish 時にビルド不能。`include` に追加すれば publish 時にダウンロードが走る暴走リスク。
- **方式 C (取得スクリプト)**: Unix / Windows 用 2 種類のスクリプトを保守する負担と、ローカル開発で手動実行を忘れたときの fail が方式 A に対する優位を持たない。
- **方式 D (CI ワークフロー直接)**: ローカル開発で `cargo test` が動かなくなり単独採用不可。

### キャッシュディレクトリ

- 配置先: `<CARGO_TARGET_TMPDIR>/test-fonts/`。`CARGO_TARGET_TMPDIR` は Rust 1.51 以降の integration test 専用環境変数で、`cargo` が `target/tmp/<test target hash>/` 形式で **test target (integration test バイナリ) 単位** に解決する。`Cargo.toml:5` の `rust-version = "1.91"` を満たす。
- 本 issue では `tests/test_font.rs` という 1 つの test target からのみ利用するためキャッシュは 1 箇所に閉じる。将来 `tests/test_context.rs` 等の別 test target から同じヘルパーを呼ぶと別キャッシュディレクトリに解決され、Source Sans 3 / Source Serif 4 をその test target でも再ダウンロードする (約 80KB + 200KB)。本 issue の利用範囲では問題ない。pbt クレートからの共有が必要になった場合は workspace 共有 helper crate 化を別 issue で検討する。
- `tests/common/font_fetch.rs` 内で `env!("CARGO_TARGET_TMPDIR")` を直接展開してパスを構築する (`cargo metadata` のような外部プロセス起動は避ける)。
- `<CARGO_TARGET_TMPDIR>/test-fonts/` ディレクトリ自身は cargo が自動作成しないため、`fetch_*_bytes()` の冒頭で `std::fs::create_dir_all` を呼ぶ。
- 一時ファイル名: `<filename>.tmp.<pid>.<nanos>.<seq>` 形式でユニーク化し、SHA-256 検証成功後に `rename` で `<filename>` に atomic 置換する。`<nanos>` は `std::time::SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0)`、`<seq>` はモジュールトップレベルの `static SEQ: AtomicU64 = AtomicU64::new(0);` から `SEQ.fetch_add(1, Ordering::Relaxed)` で取得する。これにより clock skew (`nanos == 0` に落ちた場合) でも `seq` の単調増加で必ずユニーク化される (`thread_id` は使わない — `ThreadId` の `Debug` 表示形式が Rust バージョン依存で将来互換性に難がある)。
- 残骸 `.tmp.*` ファイルはヘルパー起動時 (`fetch_*_bytes()` の冒頭、`create_dir_all` の直後) に `<CARGO_TARGET_TMPDIR>/test-fonts/` 直下を `read_dir` で走査して削除する。
- 既に最終ファイル (`<filename>`) が存在する場合は SHA-256 を再検証して一致すればそのまま再利用、不一致なら削除して再ダウンロード。

### 取得対象フォント

| 用途 | フォント | 形式 | ライセンス | release タグ | SHA-256 | URL |
|---|---|---|---|---|---|---|
| TrueType (GSUB liga / GPOS Pair Adjustment 検証) | Source Sans 3 Regular | TTF | SIL OFL 1.1 | `3.052R` | `4644c81b86ec9caaa76b634889968ed3c4f4f52f054855933acc7c2b21e53b0f` | `https://github.com/adobe-fonts/source-sans/raw/refs/tags/3.052R/TTF/SourceSans3-Regular.ttf` |
| OpenType CFF (CFF アウトライン検証) | Source Serif 4 Regular | OTF (CFF1) | SIL OFL 1.1 | `4.005R` | `edf160d0d584deee8a3bb2c3371b2a7624ca63580fbe02c57c1f4c91e84d8787` | `https://github.com/adobe-fonts/source-serif/raw/refs/tags/4.005R/OTF/SourceSerif4-Regular.otf` |

SHA-256 値は polish 中 (2026-06-22) に `curl -fsSL <URL> | shasum -a 256` で実機取得した値。タグが GitHub 上で消失していた場合 (リトライ後の HTTP 404) は **本 issue 着手を停止しユーザーに報告** する。auto-resolve 停止後の手順: 本 issue を pending に移動 (`git mv issues/0039-... issues/pending/0039-...`) し、代替フォント選定を扱う別 issue を `/create-issue` で起票する。

### Microsoft `kern` テーブル v0 の扱い

本 issue の polish 中に Source Sans 3 (`3.052R`) と Source Serif 4 (`4.005R`) の table directory を実機ダンプした結果、両フォントとも `kern` テーブルを持たない。

これにより 0025 (`kern` テーブル v0 / Format 0 パース) のスモークテストを **`kern` テーブル存在前提のフォントで実走させる経路は本 issue では用意しない**。本 issue 完了 PR で 0001 メタ issue 備考に申し送りを追記し (完了条件「0001 メタ issue 更新」項目 9 参照)、本 issue closed 後に 0025 / 0026 / 0027 を `/polish-issue` で個別に再 polish する。0026 (GSUB / GPOS) の前提は両フォントが GPOS / GSUB を持つため成立する。

将来 Source Sans 3 のリリースバージョンを更新する際は、SHA-256 値の更新と同時に **`kern` テーブルの有無を再確認** すること (この運用ガイダンスは 0001 メタ issue 備考の項目 9 にも転記する)。

### 検証方式 (`verify_has_table`)

`tests/common/font_fetch.rs` 内に sfnt table directory を直接走査するヘルパー `verify_has_table(data: &[u8], tag: [u8; 4]) -> bool` を実装する。0027 完了 (`TableDirectory::parse` の `OTTO` 受理追加 + CFF パース) までは Source Serif 4 を `FontFace::from_data` で読めないため、独立した低レベル走査が必要。

`verify_has_table` の仕様:

- offset 0..4 の sfnt version が `0x0001_0000` (TrueType) / `0x4F54544F` (`OTTO`、OpenType with CFF) / `0x7472_7565` (`'true'`、Apple TrueType variant) の **いずれかに一致しなければ `false` を返す**。raden 本体パーサ (`src/font/tables.rs:66`) が許容する `'true'` も本ヘルパーで受理することで sfnt 受理範囲を本体と揃える。TTC (`0x7474_6366` = `ttcf`) は本ヘルパーでは `false` 返却 (本 issue 取得対象は単体 TTF / OTF のみ。本ヘルパーは TTC 内の face 走査を行わない — 0027 polish 時に `FontFace::has_table` 等の本体 API への置換を検討する場合、本体 API が TTC を扱える点との挙動差を 0027 polish 担当が評価する)
- `data.len() < 12` (table directory ヘッダ未満) なら `false` を返す
- offset 4..6 の `num_tables: u16` を読み、offset 12 から `(tag: 4 bytes, checksum: 4 bytes, offset: 4 bytes, length: 4 bytes) × num_tables` の固定レイアウトを走査
- table directory 末尾は `12_usize.checked_add((num_tables as usize).checked_mul(16)?)?` で計算し、`None` (オーバーフロー) または `data.len()` 超過なら `false` を返す (32bit プラットフォームでも `num_tables = 65535` で `12 + 65535 * 16 = 1048572` のためオーバーフローしないが防御的に書く)
- `tag` 列のいずれかが指定 `tag` 引数と一致すれば `true`

raden 本体との関係: 本ヘルパーは 0027 完了までの一時実装で、0027 で `TableDirectory::parse` の `OTTO` 受理が追加され、かつ本体に `FontFace::has_table(tag: [u8; 4]) -> bool` 相当 API が追加されれば、本ヘルパーは `FontFace::has_table` に置換できる。置換するか否かは 0027 polish で確定する (本 issue では恒久存続/置換のどちらも許容)。

### スモークテスト assertion (確定)

- **Source Sans 3** (`fetch_source_sans_3_has_required_tables`):
  - `verify_has_table(&bytes, *b"GSUB") == true` (0026 着手前提)
  - `verify_has_table(&bytes, *b"GPOS") == true` (0026 着手前提)
  - `raden::FontData::from_bytes(bytes)` → `raden::FontFace::from_data(&data, 0).expect(...)` が成功し `face.units_per_em() > 0`
- **Source Serif 4** (`fetch_source_serif_4_has_cff_table`):
  - `verify_has_table(&bytes, *b"CFF ") == true` (タグは 4 バイト固定 `b'C', b'F', b'F', b' '`、0027 着手前提)
  - raden 本体パーサは `OTTO` を `unsupported sfnt version` で弾くため `FontFace::from_data` 呼び出しは行わない (0027 完了後にスモークテストを拡張する申し送り)

スモークテスト擬似コード (assertion メッセージは `CLAUDE.md` 規約に従い日本語、文体は「〜である必要がある」で統一):

```rust
mod common;

use raden::{FontData, FontFace};

#[test]
fn fetch_source_sans_3_has_required_tables() {
    let bytes = common::font_fetch::fetch_source_sans_3_bytes()
        .expect("Source Sans 3 のダウンロードに失敗しました");
    assert!(
        common::font_fetch::verify_has_table(&bytes, *b"GSUB"),
        "Source Sans 3 は GSUB テーブルを持つ必要がある"
    );
    assert!(
        common::font_fetch::verify_has_table(&bytes, *b"GPOS"),
        "Source Sans 3 は GPOS テーブルを持つ必要がある"
    );
    let data = FontData::from_bytes(bytes);
    let face = FontFace::from_data(&data, 0)
        .expect("Source Sans 3 を FontFace としてロードできる必要がある");
    assert!(
        face.units_per_em() > 0,
        "Source Sans 3 の units_per_em は正値である必要がある"
    );
}

#[test]
fn fetch_source_serif_4_has_cff_table() {
    let bytes = common::font_fetch::fetch_source_serif_4_bytes()
        .expect("Source Serif 4 のダウンロードに失敗しました");
    assert!(
        common::font_fetch::verify_has_table(&bytes, *b"CFF "),
        "Source Serif 4 は CFF テーブルを持つ必要がある"
    );
}
```

### ダウンロード機構の公開 API

`tests/common/font_fetch.rs` に公開ヘルパー 3 種:

```rust
pub fn fetch_source_sans_3_bytes() -> Result<Vec<u8>, FetchError>;
pub fn fetch_source_serif_4_bytes() -> Result<Vec<u8>, FetchError>;
pub fn verify_has_table(data: &[u8], tag: [u8; 4]) -> bool;
```

- `FetchError` は `pub enum` (戻り値型に登場するため公開せざるを得ない。テストヘルパークレート内で公開、`Cargo.toml:11` の `include` で publish 配布物には混入しない)。バリアントは `NetworkFailure { url: String, last_error: String }` / `Sha256Mismatch { expected: String, actual: String }` / `IoError(String)`。すべて文字列フィールドで内部ライブラリ型 (ureq エラー型等) を漏らさない。
- `_bytes` サフィックス命名で統一する (`Vec<u8>` 返却)。0027 完了後に `fetch_source_serif_4() -> raden::FontData` を追加する想定 (本 issue では追加しない)。0025 / 0026 で raden 本体ロード経路が必要な場合は呼び出し側で `raden::FontData::from_bytes(bytes)` する。
- 内部処理: `install_default()` → `create_dir_all` → 残骸 cleanup → 既存最終ファイル SHA-256 検証 → なければダウンロード → SHA-256 検証 → atomic rename → ファイル読み込み → 返却。SHA-256 計算はフォントサイズが 200KB 以下のため `aws_lc_rs::digest::digest(&aws_lc_rs::digest::SHA256, &bytes)` でバイト列一括計算する (ストリーミング `Context::update` は本 issue では採用しない)。
- ローカル開発でオフライン実行した場合、初回はネットワーク必須でテストが失敗する。これは寛容スキップを取らない (CI を確実に通すため)。ローカル開発者は初回オンライン環境で `cargo test` を 1 回回せば以後オフライン動作可能。
- 初回ダウンロード時は eprintln! でライセンス通知を日本語で出力する: `eprintln!("テスト用フォントをダウンロードします: <name> (SIL OFL 1.1) {}", url);` (`CLAUDE.md` 規約「テストのログメッセージは全て日本語にすること」に整合)。並列呼び出しで通知が複数回出る可能性は許容する (実害なし)。

### HTTP クライアントと SHA-256 ライブラリ

`shiguredo-rust` 規約「TLS は rustls を使うこと」「暗号ライブラリは aws-lc-rs を使うこと」に整合させるため、`ureq` + `rustls` + `aws-lc-rs` の 3 段構成を採る。`Cargo.toml` の `[dev-dependencies]` に末尾追記する形で以下を追加する (`shiguredo-rust` 規約「依存ライブラリには用途をコメントで明記すること」「マイナーバージョンまで指定」に従う。既存 `[dev-dependencies]` の `criterion = "0.8"` / `rand = "0.10"` 等と同じキャレット表記を採用する):

```toml
# テスト用フォントの HTTPS ダウンロード (rustls TLS バックエンド、crypto provider は rustls 側で aws_lc_rs を指定)
ureq = { version = "3.3", default-features = false, features = ["rustls-no-provider", "rustls-webpki-roots"] }
# rustls の crypto provider を aws-lc-rs に固定する (feature 名はアンダースコア表記 `aws_lc_rs`)
rustls = { version = "0.23", default-features = false, features = ["aws_lc_rs"] }
# テスト用フォントの SHA-256 整合性検証と rustls の crypto provider 実装 (非 FIPS 経路を有効化)
aws-lc-rs = { version = "1.17", default-features = false, features = ["non-fips"] }
```

実装着手時の補足:

- **`rustls` の feature 名は `aws_lc_rs` (アンダースコア)** であり、クレート名 `aws-lc-rs` (ハイフン) と異なる。実装着手者は `cargo metadata --format-version 1 | jq '.packages[] | select(.name == "rustls" and .version | startswith("0.23")) | .features'` 等で feature 一覧を確認してから設定する (ハイフン表記指定はビルド失敗する)。
- **`aws-lc-rs` の `non-fips`** はクレート名と同じハイフン表記。`default-features = false` 時に `digest::SHA256` 等の暗号 primitive を利用可能にする必要最小 feature。FIPS 認証は本 issue 用途で不要。
- `ureq 3.3` の Agent 構築コードスケッチ:

  ```rust
  use std::time::Duration;
  let agent = ureq::Agent::config_builder()
      .timeout_connect(Some(Duration::from_secs(10)))
      .timeout_global(Some(Duration::from_secs(60)))
      .build()
      .new_agent();
  ```

- リトライは ureq 組み込みではなくヘルパー側で `for attempt in 0..3 { ...; std::thread::sleep(Duration::from_secs(1u64 << attempt)); }` のループで実装し、`ureq::Error::StatusCode(n)` の `n >= 500` および接続失敗・タイムアウトの 3 種類のみリトライする (HTTP 4xx は即時 fail)。
- プロセス default crypto provider 設定: モジュールトップレベルに `static INSTALL: std::sync::OnceLock<()> = std::sync::OnceLock::new();` を置き、`fetch_*_bytes()` の冒頭で:

  ```rust
  INSTALL.get_or_init(|| {
      rustls::crypto::aws_lc_rs::default_provider()
          .install_default()
          .expect("rustls の crypto provider が既に登録されている (aws-lc-rs 固定の前提が崩れているため要確認)");
  });
  ```

  を呼ぶ。`Err` を `.expect(...)` で検知することで「別 crypto provider が既登録 = `shiguredo-rust` 規約違反」を CI で検出する。`OnceLock` の get_or_init は 1 度だけ実行されるため、並列 thread 呼び出しでも安全。
- `aws-lc-rs` のビルドには cmake が必要。Linux 構成は `.github/workflows/ci.yml:33-46` で既に cmake インストール済 (SDL3 ビルド用)。macOS runner には標準で cmake が含まれる。Windows runner (`windows-2025`) の標準ソフトウェアに cmake / NASM が含まれるかは実装着手時に GitHub Actions の `actions/runner-images` ドキュメントを確認する。含まれない場合は `.github/workflows/ci.yml` の Windows ジョブで `choco install -y cmake nasm` 等のセットアップステップ追加を本 issue のスコープに含める (完了条件「動作」参照)。
- `ureq 3.3` / `rustls 0.23` / `aws-lc-rs 1.17` の各 MSRV は実装着手時に各クレートの `Cargo.toml` `rust-version` を確認し、`Cargo.toml:5` の `rust-version = "1.91"` を満たすことを確認する。満たさない場合は raden の `rust-version` 引き上げを別 issue 化する。
- これらは `[dev-dependencies]` 内に閉じるため、`cargo publish` の本番依存ツリーには混入しない。

### エラーハンドリング

| エッジケース | 確定挙動 |
|---|---|
| ネットワーク不通 | `ureq` の接続タイムアウト 10 秒・全体タイムアウト 60 秒。指数バックオフで最大 3 回 (1 秒 → 2 秒 → 4 秒) リトライ。3 回失敗で `FetchError::NetworkFailure { url, last_error }` を返す (`last_error` は `format!("{e}")` で文字列化)。 |
| ダウンロード途中切断 | `<filename>.tmp.<pid>.<nanos>.<seq>` に書いた途中バイトは SHA-256 検証で必ず不一致になり、再ダウンロード経路に合流する。 |
| SHA-256 不一致 | キャッシュファイルを削除し再ダウンロードを 1 回試行する。それでも不一致なら `FetchError::Sha256Mismatch { expected, actual }`。リトライ 1 回 (3 回ではない) の根拠: ネットワーク不通はバックオフリトライで吸収済み、SHA-256 ミスマッチは「タグ消失・URL 改ざん」のシグナルなので即時 fail でユーザー通知する。 |
| ディスクフル | `std::io::Error` をそのまま `FetchError::IoError(format!("{e}"))` に転送する。 |
| 並列実行 race | `<filename>.tmp.<pid>.<nanos>.<seq>` でユニーク名化。最終 `rename` は OS の atomic 性で安全。同じ最終ファイルに 2 thread が rename しても、両者の SHA-256 検証は通っているため内容は一致する。 |
| release タグ消失 (HTTP 404) | ネットワーク不通と同じリトライ後に `FetchError::NetworkFailure`。auto-resolve は停止しユーザーに報告。 |
| `verify_has_table` で要求テーブル不在 | `false` 返却 → スモークテスト側で日本語アサーション付きで fail。auto-resolve は停止しユーザーに報告。 |

スモークテスト側で `fetch_source_sans_3_bytes().expect("...")` のように受ける (`shiguredo-rust` 規約「`.unwrap()` ではなく `.expect("MESSAGE")` を使用すること」に整合)。`panic!` の直接呼び出しはヘルパー内では行わない (`shiguredo-rust` 規約上「絶対に発生しない想定の表明」に該当しないため)。

### ライセンス対応

- フォント本体は **再配布しない** (CI / 各開発者が原本リポジトリから直接取得する) ため、SIL OFL 1.1 Section 4 (Permission Notice 同梱要件) は適用されない。
- Section 3 (Reserved Font Name 制約) は raden がフォント名を改変しないため抵触しない。Source Sans 3 / Source Serif 4 の OFL 表記で `Reserved Font Name 'Source'` を polish 中 (2026-06-22) に確認済 (両者とも `'Source'` 指定)。
- 通知はランタイム `eprintln!` で出力する (前述「ダウンロード機構の公開 API」参照)。`README.md` / `tests/common/README.md` 等への明記は本 issue では行わない。

### 既存 `load_arial()` 経路の扱い

- 本 issue では `tests/test_font.rs:7-14` / `tests/test_context.rs:497-504` / `pbt/tests/prop_font/main.rs:10-17` の `load_arial()` 経路は **変更しない**。
- 本 issue 完了後の世界では macOS 限定 `load_arial()` 経路と全プラットフォーム動作する `fetch_source_sans_3_bytes` / `fetch_source_serif_4_bytes` 経路が共存する。共存期間は 0037 (`load_arial()` 集約) の完了まで。
- 0037 と本 issue は `tests/common/` ディレクトリで作業が交差するが、本 issue で追加するファイル名は `font_fetch.rs` のため、0037 が想定する `font.rs` とファイル名が衝突しない。`tests/common/mod.rs` の作成は本 issue で行う (`pub mod font_fetch;` のみ宣言)。0037 着手時に 0037 polish 担当が本 issue 完了状態を見て `pub mod font;` 行を 1 行追加する流れに統一する。逆順 (0037 先着) は本 issue PR 着手時点で発生しない (本 issue が 0025 / 0026 / 0027 の着手前提のため先行する)。
- Rust integration test 慣行に従い、`tests/common/mod.rs` 経由で参照する (`#[path]` 直接参照は本 issue では採用しない)。各 test ファイルから参照する場合は `mod common;` 宣言 + `common::font_fetch::fetch_source_sans_3_bytes()` の形で呼ぶ。Rust integration test では `.rs` 1 ファイル = 1 独立クレートのため、他の test ファイル (`tests/test_context.rs` 等) から参照する場合も各ファイルが個別に `mod common;` を宣言する必要がある (0025 / 0026 / 0037 着手時に各 issue で対応する責務)。`tests/common/font_fetch.rs` の冒頭には `#![expect(dead_code, reason = "0025 / 0026 / 0027 から段階的に呼び出されるため、本 issue 完了直後は一部の公開ヘルパーが未使用となる")]` を付与する (`shiguredo-rust` 規約 L31-32「`#[allow(...)]` ではなく `#[expect(...)]` を使うこと」に整合、Rust 1.81+ で安定化済み)。
- `tests/common/mod.rs` という `mod.rs` ファイル名にすることで、Cargo が `common` を独立 integration test として実行しない (`tests/common.rs` 直下だと別 test crate になる)。

## 完了条件

### リポジトリ作業ツリーの不変条件

- `git ls-files | grep -E '\.(ttf|otf|ttc|woff2?)$'` が 0 件 (フォントバイナリが 1 つもコミットされていない)
- `git ls-files | grep -iE '(SourceSans|SourceSerif).*LICENSE'` が 0 件 (フォント由来 LICENSE が同梱されていない)
- `.gitignore` に `target/test-fonts/` の除外設定が追加されている
- `cargo package --list 2>&1 | grep -E '^(tests|target)/'` が 0 件 (publish 配布物にテストヘルパーが混入しないことの検証)

### 動作

- `cargo test --workspace` が CI 全 7 構成 (`ubuntu-24.04` / `ubuntu-24.04-arm` / `ubuntu-22.04` / `ubuntu-22.04-arm` / `macos-26` / `macos-15` / `windows-2025`) で pass する。pass 判定は「スキップを除いた全テストが pass し、スキップは macOS 限定 `Arial.ttf` 直参照を含むテスト群のみ」とする。スキップ件数は 0021 / 0022 close で変動するため本文では断定しない (`grep -c "let Some(face) = load_arial()" tests/test_font.rs tests/test_context.rs pbt/tests/prop_font/main.rs` で実測した値を PR レビュー時に確認する想定)。
- ローカルで `cargo test` 単独実行時にダウンロード機構が動作する (初回はネットワーク必須、以降はキャッシュ利用)
- Windows runner で rustls (aws-lc-rs crypto provider) 経由の HTTPS 接続が成功する。`windows-2025` runner に cmake / NASM が含まれない場合は `.github/workflows/ci.yml` の Windows ジョブにセットアップステップを追加する (実装着手時に確認)
- 実装着手時に `cargo metadata --format-version 1` で `rustls 0.23` の `aws_lc_rs` feature、`aws-lc-rs 1.17` の `non-fips` feature が存在することを確認する
- スモークテスト 2 件 (`fetch_source_sans_3_has_required_tables` / `fetch_source_serif_4_has_cff_table`) が `tests/test_font.rs` で全 7 構成で pass する。スモークテスト本体は「設計方針 スモークテスト assertion (確定)」の擬似コードに準拠する

### テスト

- `tests/test_font.rs` に新規スモークテスト 2 件を追加する (実装は設計方針セクションの擬似コードに準拠)
- 既存 `load_arial()` 経路 (`tests/test_font.rs` / `tests/test_context.rs` / `pbt/tests/prop_font/main.rs`) は本 issue では変更しない
- アサーションメッセージは日本語 (`CLAUDE.md` 規約)
- `verify_has_table` の独自走査ロジックの PBT / Fuzzing は本 issue では追加しない (テスト基盤の限定的なヘルパーであり、raden 本体 `TableDirectory::parse` の PBT で sfnt 走査の不変条件は別途 0001 メタ issue tracked のスコープで検証)

### `[dev-dependencies]` 追加

- `Cargo.toml` の `[dev-dependencies]` 末尾に `ureq` / `rustls` / `aws-lc-rs` を用途コメント付きで追加する (`shiguredo-rust` 規約、`Cargo.toml` `[dev-dependencies]` の既存ブロック `criterion` / `rand` / `raw_player` の直後に追記)
- `Cargo.lock` も同 PR で更新 (実装コミットに同梱、`Cargo.toml` 編集の自動結果として扱う)
- `Cargo.toml` の `include` フィールドと `exclude` フィールドは編集しない (現状の `include = ["/LICENSE", "/README.md", "/src/**"]` で `tests/` 配下が publish 配布物から除外される。L42 で確認済)

### CHANGES.md

- 公開 API 変更なし・描画結果変化なしのテスト基盤変更のみ。`shiguredo-changelog` SKILL.md L20「`.rst` / `.md` ファイルの変更は変更履歴に反映しないこと」(0001 メタ issue / 本 issue 自体の `.md` 編集)、および 0001 メタ issue L80「純粋に内部実装のみで描画結果が変わらない変更は CHANGES.md に記載しない」(本 issue 内の `Cargo.toml` / `tests/` / `.gitignore` 変更) に整合し、`CHANGES.md` への記載は不要

### 0001 メタ issue 更新 (本 issue 完了 PR 内の独立コミット)

`issues/0001-enhance-font-module-maturity.md` を以下のとおり更新する:

1. `## Concrete issue 一覧` テーブル (L100-108) に `0039` 行を追加 (Priority: High、状態: closed)
2. `## Tracked items` テーブル (L116-123) から「テスト用フォント選定」行を削除
3. `### tracked: テスト用フォント選定` サブセクション (L138-143) 全体を削除
4. `### 前提 concrete issue` リスト (L165-172) の「1. テスト用フォント選定」項目を削除し、後続 1〜3 番に番号繰り上げ
5. `### クロスプラットフォーム` 完了条件 L244 (`全プラットフォームで利用可能なテスト用フォントバイナリがリポジトリに含まれる`) を「`fetch_source_sans_3_bytes` / `fetch_source_serif_4_bytes` 経由で全プラットフォームのフォントテストが pass する」に書き換える (L243 と L245 は文意が生きているため触らない)
6. `### パフォーマンス基準` L227 の「測定対象フォントはテスト用フォント選定で確定する」を「測定対象フォントは `fetch_source_sans_3_bytes` で取得する Source Sans 3 を使う」に書き換える
7. `### CFF / CFF2 判断` L239 「テスト用フォント選定 tracked item には CFF / CFF2 フォントの選定を含める」を「CFF / CFF2 テストは `fetch_source_serif_4_bytes` で取得する Source Serif 4 を使う」に書き換える
8. `## 備考` (L196-200) に「CJK 統合漢字 / BMP 外文字のカバレッジは Source Sans 3 / Source Serif 4 では満たせない。必要になった時点で別 tracked / 別 issue を起票する」を 1 行追記
9. `## 備考` (L196-200) に「0039 closed 後、0025 / 0026 / 0027 / 0037 を `/polish-issue` で個別に再 polish し、本文中の『リポジトリ同梱前提』を `fetch_source_sans_3_bytes` / `fetch_source_serif_4_bytes` 参照に書き換える。0025 polish では Source Sans 3 / Source Serif 4 のいずれも `kern` テーブルを持たない事実を踏まえ、kern fallback 動作確認 (`Font::kern() == 0`) に再定義するか、`kern` 付きフォント追加選定の別 issue を起票するかを確定する。0027 polish では `TableDirectory::parse` (`src/font/tables.rs:58-93`) の sfnt version 判定に `0x4F54544F` (`OTTO`) を追加する責務を含める。0037 polish では本 issue が新設した `tests/common/mod.rs` に `pub mod font;` 行を 1 行追加する流れを前提とする。Source Sans 3 のバージョン更新時は SHA-256 と同時に `kern` テーブル有無の再確認を必須とする」を 1 段落追記

## 解決方法

設計方針・完了条件・変更対象ファイルに記載のとおり。

## 変更対象ファイル

| ファイル | 種別 | 内容 |
|---|---|---|
| `tests/common/mod.rs` | 新規 | `pub mod font_fetch;` を宣言 |
| `tests/common/font_fetch.rs` | 新規 | 冒頭に `#![expect(dead_code, reason = "...")]`。`use std::sync::OnceLock;` / `use std::sync::atomic::{AtomicU64, Ordering};` / `use std::time::Duration;` 等を import。`pub enum FetchError` (`NetworkFailure` / `Sha256Mismatch` / `IoError`)、`fetch_source_sans_3_bytes` / `fetch_source_serif_4_bytes` / `verify_has_table` の実装。`static INSTALL: OnceLock<()>` と `static SEQ: AtomicU64`。SHA-256 検証 (aws-lc-rs 一括計算)、指数バックオフリトライ (ureq 3.x、ヘルパー側ループ)、`<filename>.tmp.<pid>.<nanos>.<seq>` の atomic rename、残骸 cleanup、`rustls::crypto::aws_lc_rs::default_provider().install_default()` を `OnceLock` で 1 度だけ `.expect(...)` で確認 |
| `Cargo.toml` | 既存編集 | `[dev-dependencies]` 末尾 (`raw_player` の直後) に `ureq` / `rustls` / `aws-lc-rs` を用途コメント付きで追加 (3 段構成)。`include` / `exclude` は触らない |
| `.gitignore` | 既存編集 | `target/test-fonts/` の除外設定追加 |
| `tests/test_font.rs` | 既存編集 | 現状 L1 (`use raden::{Font, FontData, FontFace};`) の直前に `mod common;` を 1 行追加、続けて 1 行空行を入れる (`mod common;` → 空行 → `use raden::...`)。新規スモークテスト 2 件 (`fetch_source_sans_3_has_required_tables` / `fetch_source_serif_4_has_cff_table`) を末尾 (現状 L229 直後) に追加 (擬似コードは設計方針セクション参照、各テスト 15-20 行) |
| `issues/0001-enhance-font-module-maturity.md` | 既存編集 (独立コミット) | 完了条件「0001 メタ issue 更新」項目 1〜9 を実施 |

`issues/0025-add-font-kerning.md` / `issues/0026-add-opentype-basic-shaping.md` / `issues/0027-add-cff-cff2-outline-support.md` / `issues/0037-refactor-tests-common-font-helper.md` は本 issue PR では編集しない (`shiguredo-issues` SKILL.md L59「`Polished:` を更新できるのは `/polish-issue` スキルのみ」に整合)。本 issue closed 後にユーザーが `/polish-issue 0025 0026 0027 0037` を順次実行する想定。再 polish 時に行う作業内容は 0001 メタ issue の備考に申し送り済み (完了条件「0001 メタ issue 更新」項目 9 参照)。

## 関連

- `0001-enhance-font-module-maturity.md` (メタ issue。本 issue は Tracked items「テスト用フォント選定」を完了させ、完了条件の文言書き換えと備考への申し送り追記を担う)
- `0025-add-font-kerning.md` (kern fallback 動作確認の前提。本 issue closed 後に `/polish-issue 0025` で再 polish し `fetch_source_sans_3_bytes` 参照に書き換える)
- `0026-add-opentype-basic-shaping.md` (GSUB / GPOS 付きフォント前提。本 issue closed 後に `/polish-issue 0026` で再 polish し `fetch_source_sans_3_bytes` 参照に書き換える)
- `0027-add-cff-cff2-outline-support.md` (CFF / CFF2 フォント前提。本 issue closed 後に `/polish-issue 0027` で再 polish し `TableDirectory::parse` の `OTTO` 受理追加と `fetch_source_serif_4_bytes` 参照に書き換える)
- `0037-refactor-tests-common-font-helper.md` (`load_arial()` 集約。本 issue が `tests/common/mod.rs` と `tests/common/font_fetch.rs` を新設し、0037 polish 時に `pub mod font;` 行を追加する流れを前提とする)
