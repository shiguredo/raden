# テスト用フォントのダウンロード機構を追加する

- Priority: High
- Category: add
- Created: 2026-06-22
- Completed: 2026-06-22
- Model: Opus 4.7
- Branch: feature/add-test-font-downloader
- Polished: 2026-06-22

## 目的

raden の font 関連テスト・PBT が macOS 限定 `Arial.ttf` に依存している状況を解消するため、CI 全プラットフォームで利用可能なテスト用フォント (Source Sans 3 / Source Serif 4) を、**フォントバイナリ (.ttf / .otf / .ttc / .woff / .woff2) や LICENSE 全文をリポジトリにコミットせず、必要なタイミングで外部からダウンロードする仕組み** をリポジトリに追加する。

本 issue は 0001 メタ issue の Tracked items「テスト用フォント選定」を完了させる責務を持つ。0025 (kern fallback 動作確認) / 0026 (GSUB + GPOS) / 0027 (CFF) の各 concrete issue は本 issue が close されない限り着手前提を満たせない。本 issue は単独で closed 可能であり、0025 / 0026 / 0027 / 0037 の本文書き換えは **本 issue closed 後にユーザーが `/polish-issue 0025 0026 0027 0037` を起動して各 issue 側の polish で確定する** (`shiguredo-issues` SKILL.md L59「`Polished:` を更新できるのは `/polish-issue` スキルのみ」に整合)。

スコープ外:

- 既存 `load_arial()` ヘルパー集約・撤去 (0037 のスコープ)
- 既存 Arial 依存テスト全件の切り替え (0037 およびその後続のスコープ)
- 0025 / 0026 / 0027 / 0037 issue 本文の書き換え (本 issue closed 後に各 issue を `/polish-issue` で個別に再 polish)
- `examples/raden_player.rs:189` の `/System/Library/Fonts/Helvetica.ttc` 直参照 (サンプルは macOS 限定動作で運用継続)
- CJK 統合漢字 / BMP 外文字のカバレッジ (Source Sans 3 / Source Serif 4 はラテン系のみ。対応方針は完了条件「0001 メタ issue 更新」項目 8 で扱う)

## 優先度根拠

High。0025 / 0026 / 0027 の 3 つの concrete issue の着手前提であり、本 issue が close されない限り 0025 以降の auto-resolve が進められない。0001 メタ issue の完了条件「全プラットフォームで font テストが CI で動作する」の前提でもある。

## 現状

- リポジトリ内に `.ttf` / `.otf` / `.ttc` / `.woff` / `.woff2` は 1 件もコミットされていない (`git ls-files | grep -E '\.(ttf|otf|ttc|woff2?)$'` で 0 件)。
- `Cargo.toml` の `[dev-dependencies]` は `criterion` / `rand` / `raw_player` の 3 件のみ。本 issue は **依存を一切追加しない**。
- `Arial.ttf` 直参照は 3 ファイル:
  - `tests/test_font.rs:7-14` (`load_arial()` 定義、`#[test]` 12 件すべてが利用)
  - `tests/test_context.rs:497-504` (`mod stroke_text` 内 `load_arial()` 定義、5 箇所で利用 — 510 / 540 / 593 / 617 / 635 行)
  - `pbt/tests/prop_font/main.rs:10-17` (`load_arial()` 定義、PBT 13 件すべてが `load_arial()` を呼ぶ)
- 上記はすべて macOS の `/System/Library/Fonts/Supplemental/Arial.ttf` を直接参照しており、Linux / Windows の CI ジョブではすべてスキップされている。
- `tests/helpers/` ディレクトリは未作成 (0037 で作成予定)。本 issue は `tests/helpers/mod.rs` と `tests/helpers/font_fetch.rs` を新設する。
- 現状 raden が実装済みのフォントテーブルは `head` / `maxp` / `hhea` / `hmtx` / `loca` / `cmap` / `glyf` / `OS/2` および TTC (TrueType Collection) のみ。`src/font/tables.rs:113-120` には `TAG_HEAD` / `TAG_MAXP` / `TAG_HHEA` / `TAG_HMTX` / `TAG_LOCA` / `TAG_CMAP` / `TAG_GLYF` / `TAG_OS2` の 8 定数しか定義されておらず (TTC は同ファイル末尾近くの `TAG_TTCF`)、`TAG_KERN` / `TAG_GSUB` / `TAG_GPOS` / `TAG_CFF` / `TAG_CFF2` の TAG 定数および対応 parse 関数は **存在しない** ことから、これらテーブルは未パースである。
- `src/font/tables.rs:58-93` の `TableDirectory::parse` は sfnt version `0x0001_0000` (TrueType) と `0x7472_7565` (`'true'`、Apple TrueType variant) のみを受理する。`0x4F54544F` (`OTTO`、OpenType with CFF アウトライン) を渡すと L67 で `FontError::InvalidData("unsupported sfnt version")` を返す。CFF only OTF (Source Serif 4 等) は `parse_all` (L629) に到達する前に `TableDirectory::parse` の段階で弾かれる。`OTTO` 受理の追加と CFF アウトライン対応は 0027 のスコープ。
- `Cargo.toml:11` の `include = ["/LICENSE", "/README.md", "/src/**"]` はホワイトリスト方式で `tests/` 配下は `cargo publish` 配布物から自動除外される。`Cargo.toml:16` の `exclude = ["fuzz"]` は `include` 優先で無効化される。本 issue では `Cargo.toml` を一切編集しない。
- `raden::FontData::from_bytes(bytes: Vec<u8>) -> FontData` (`src/font/mod.rs:57`) / `raden::FontFace::from_data(&FontData, u32) -> Result<FontFace, FontError>` (`src/font/mod.rs:76`) / `raden::FontFace::units_per_em(&self) -> u16` (`src/font/mod.rs:82`) はいずれも公開 API として存在し、本 issue のスモークテストで使用する。
- 0001 メタ issue の Tracked items「テスト用フォント選定」の完了基準 L142 (`全プラットフォームで利用可能なフォントがリポジトリに含まれる PR がマージ済み`) および完了条件「クロスプラットフォーム」L244 (`全プラットフォームで利用可能なテスト用フォントバイナリがリポジトリに含まれる`) は本 issue の方針 (リポジトリにコミットしない) と矛盾する。本 issue 完了 PR で書き換える (詳細は完了条件「0001 メタ issue 更新」参照)。
- 既存 `.gitignore` には `target/` の包括除外があり、`<CARGO_TARGET_TMPDIR>` 配下 (実体は `target/tmp/...`) もそれに含まれるため、本 issue では `.gitignore` 編集も不要。

## 設計方針

### 方式選定: 方式 A (テストヘルパー内で外部コマンドを子プロセス呼び出ししローカルキャッシュ) を採用する

`tests/helpers/font_fetch.rs` を新規作成し、初回利用時に HTTPS でダウンロード (`curl`) → SHA-256 検証 (OS 既定 hash ツール) → `<CARGO_TARGET_TMPDIR>/test-fonts/` にキャッシュ保存する形を採る。

採用理由:

- `cargo test --workspace` 単独で完結し、CI / ローカル開発で機構の差を作らない。
- `Cargo.toml:11` の `include` ホワイトリストにより `tests/` 配下は `cargo publish` 配布物から自動除外される。
- `build.rs` を増設しないため、本番ビルド時に余計なネットワーク I/O が走らない。
- 検証用 sfnt table directory 走査も同じヘルパー内で完結する (0025 / 0026 / 0027 の前提解消が後工程なしで成立する)。

不採用方式の理由 (各 1 文):

- **方式 B (`build.rs`)**: `Cargo.toml:11` の `include` ホワイトリストに `build.rs` が含まれていないため publish 時にビルド不能。`include` に追加すれば publish 時にダウンロードが走る暴走リスク。
- **方式 C (リポジトリ直下取得スクリプト)**: ローカル開発で `cargo test` 単独実行を保証できない (スクリプト手動実行を忘れたときに fail する)。
- **方式 D (CI ワークフロー直接)**: 同じくローカル開発で `cargo test` が動かなくなり単独採用不可。

### キャッシュディレクトリ

- 配置先: `<CARGO_TARGET_TMPDIR>/test-fonts/`。`CARGO_TARGET_TMPDIR` は Rust 1.51 以降の integration test / benchmark 専用環境変数で、cargo が 1 つの test target に対して `target/tmp/` 配下のパスを設定する (公式 cargo book "Environment variables Cargo sets for crates")。同一 cargo `target dir` を共有する複数 test target は **同じ `CARGO_TARGET_TMPDIR` を見る前提**。`Cargo.toml:5` の `rust-version = "1.91"` は要件 1.51 を満たす。
- `tests/helpers/font_fetch.rs` 内で `env!("CARGO_TARGET_TMPDIR")` を直接展開してパスを構築する。`env!` はコンパイル時マクロのため、`tests/test_font.rs` 等 test target をビルドする際に cargo が設定する環境変数を取得する (`std::env::var` の実行時取得は使わない)。
- `<CARGO_TARGET_TMPDIR>/test-fonts/` ディレクトリ自身は cargo が自動作成しないため、`fetch_*_bytes()` の冒頭で `std::fs::create_dir_all` を呼ぶ。
- 一時ファイル名: `<filename>.tmp.<pid>.<nanos>.<seq>` 形式でユニーク化し、SHA-256 検証成功後に `std::fs::rename` で `<filename>` に置換する。`<nanos>` は `std::time::SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0)` (`u128`、最大 39 桁)、`<seq>` はモジュールトップレベルの `static SEQ: AtomicU64 = AtomicU64::new(0);` から `SEQ.fetch_add(1, Ordering::Relaxed)` で取得する (Relaxed で十分な根拠: tmp 名のユニーク識別のみが目的で、他のメモリ操作との順序関係を要求しない)。`SEQ` は `fetch_source_sans_3_bytes` / `fetch_source_serif_4_bytes` の 2 関数で共有する 1 個の static で、同一プロセス内の並列 thread 呼び出しでも `seq` の単調増加によりユニーク化される (clock skew で `nanos == 0` に落ちた場合の安全網)。`thread_id` は使わない (`ThreadId` の `Debug` 表示形式が Rust バージョン依存)。最大長は `SourceSerif4-Regular.otf` (24 文字) + `.tmp.` (5 文字) + `<u32 max 10>.<u128 max 39>.<u64 max 20>` で約 100 文字以内に収まるため、Windows MAX_PATH (260 文字) に対して `<CARGO_TARGET_TMPDIR>` (CI 上は `D:\a\raden\raden\target\tmp\<hash>\` 形式で約 60 文字) と合わせても余裕がある。
- 残骸 `.tmp.*` cleanup:
  - ヘルパー起動時に `<CARGO_TARGET_TMPDIR>/test-fonts/` 直下を `read_dir` で走査。`read_dir` 自体が失敗した場合は best-effort で無視 (権限不在等)。
  - 削除対象は **(a) `<filename>` 前方一致** かつ **(b) `<pid>` 部が `std::process::id()` と一致** の両方を満たすもののみ (`<filename>` 部の判定で他フォントの並走 tmp を巻き込まない / `<pid>` 部の判定で他プロセスの作業中 tmp を消さない)。
  - `remove_file` 失敗も best-effort で無視 (権限不在 / 別 thread の rename と race した等)。
- `std::fs::rename` は POSIX で atomic。Windows でも公式ドキュメントどおり既存ファイルを置換可能 (`MoveFileExW` ベースで実装)。ただし Windows 上で対象最終ファイルが他プロセス・他 thread から **open 中の場合** は `ERROR_SHARING_VIOLATION` で失敗しうる。失敗時の動作は「最終ファイルが既に存在し SHA-256 が期待値と一致するならエラーを無視して既存ファイルを使う、それ以外は `FetchError::IoError` を返す」とする。これにより並列 thread / プロセスで複数の rename 競合があっても、後勝ち / 先勝ちのどちらでも結果としての最終ファイル内容は SHA-256 検証済みで一致する。
- 既に最終ファイル `<filename>` が存在する場合は SHA-256 を再検証して一致すればそのまま再利用、不一致なら削除して再ダウンロードフローに合流する。ファイル不在は普通に新規ダウンロード経路に進む。

### 取得対象フォント

| 用途 | フォント | 形式 | ライセンス | release タグ | filename | SHA-256 (小文字 hex 64) | URL |
|---|---|---|---|---|---|---|---|
| TrueType (GSUB liga / GPOS Pair Adjustment 検証) | Source Sans 3 Regular | TTF | SIL OFL 1.1 | `3.052R` | `SourceSans3-Regular.ttf` | `4644c81b86ec9caaa76b634889968ed3c4f4f52f054855933acc7c2b21e53b0f` | `https://github.com/adobe-fonts/source-sans/raw/refs/tags/3.052R/TTF/SourceSans3-Regular.ttf` |
| OpenType CFF (CFF アウトライン検証) | Source Serif 4 Regular | OTF (CFF1) | SIL OFL 1.1 | `4.005R` | `SourceSerif4-Regular.otf` | `edf160d0d584deee8a3bb2c3371b2a7624ca63580fbe02c57c1f4c91e84d8787` | `https://github.com/adobe-fonts/source-serif/raw/refs/tags/4.005R/OTF/SourceSerif4-Regular.otf` |

SHA-256 値は polish 2026-06-22 時点で `curl -fsSL <URL> | shasum -a 256` で実機取得した。本 issue の実装内で使う hash ツール (後述) も同じ SHA-256 値を返すため、polish 値と実装値の transcription error が発生しない。

定数の置き場所: `tests/helpers/font_fetch.rs` モジュールトップに次の形で書く。

```rust
struct FontSpec {
    name: &'static str,        // ライセンス通知用の表示名
    url: &'static str,
    expected_sha256: &'static str, // 小文字 hex 64 で書く
    filename: &'static str,
}

const SOURCE_SANS_3: FontSpec = FontSpec {
    name: "Source Sans 3 Regular",
    url: "https://github.com/adobe-fonts/source-sans/raw/refs/tags/3.052R/TTF/SourceSans3-Regular.ttf",
    expected_sha256: "4644c81b86ec9caaa76b634889968ed3c4f4f52f054855933acc7c2b21e53b0f",
    filename: "SourceSans3-Regular.ttf",
};

const SOURCE_SERIF_4: FontSpec = FontSpec {
    name: "Source Serif 4 Regular",
    url: "https://github.com/adobe-fonts/source-serif/raw/refs/tags/4.005R/OTF/SourceSerif4-Regular.otf",
    expected_sha256: "edf160d0d584deee8a3bb2c3371b2a7624ca63580fbe02c57c1f4c91e84d8787",
    filename: "SourceSerif4-Regular.otf",
};
```

タグが GitHub 上で消失していた場合 (リトライ後の HTTP エラー) は **本 issue 着手を停止しユーザーに報告** する。auto-resolve 停止後の手順は `auto-resolve` スキル側の責務で、本ヘルパーは単に `FetchError::NetworkFailure` を返す。

### Microsoft `kern` テーブル v0 の扱い

**事実 (polish 中の実機ダンプ結果)**: Source Sans 3 (`3.052R`) と Source Serif 4 (`4.005R`) の table directory を実機ダンプした結果、両フォントとも `kern` テーブルを持たない。

**0025 への影響**: これにより 0025 (`kern` テーブル v0 / Format 0 パース) のスモークテストを **`kern` テーブル存在前提のフォントで実走させる経路は本 issue では用意しない**。本 issue 完了 PR で 0001 メタ issue 備考に申し送りを追記し (完了条件「0001 メタ issue 更新」項目 9 参照、将来 Source Sans 3 のバージョン更新時に `kern` テーブル有無を再確認する運用ガイダンスを含む)、本 issue closed 後に 0025 / 0026 / 0027 を `/polish-issue` で個別に再 polish する。0026 (GSUB / GPOS) の前提は両フォントが GPOS / GSUB を持つため成立する。

### 外部コマンド呼び出し方針 (curl + OS 既定 hash ツール)

`shiguredo-rust` 規約「TLS は rustls を使うこと」「暗号ライブラリは aws-lc-rs を使うこと」は **Rust 依存クレートの選定に対する規約**。本 issue は HTTPS 取得と SHA-256 検証を OS 既存コマンドの子プロセス呼び出しで完結させ、Rust 依存を新規追加しないため当該規約の適用範囲外と扱う。同時に `CLAUDE.md` L16「モック・スタブ禁止」とも整合する (実プロセスを起動する)。

#### curl コマンド呼び出し

呼び出し例 (戻り値型は内部判定構造体 `CurlOutcome`):

```rust
enum CurlOutcome {
    Ok,
    Retryable { exit_code: i32, stderr: String },     // 6 / 7 / 28
    Permanent { exit_code: i32, stderr: String },     // 22 / 35 / 60 等
    SpawnFailed { kind: std::io::ErrorKind, err: String },
}

fn run_curl(url: &str, tmp_path: &std::path::Path) -> CurlOutcome {
    let tmp_str = match tmp_path.to_str() {
        Some(s) => s,
        None => return CurlOutcome::SpawnFailed {
            kind: std::io::ErrorKind::InvalidInput,
            err: "tmp_path is not valid UTF-8".to_string(),
        },
    };
    let result = std::process::Command::new("curl")
        .args([
            "--fail",                       // HTTP 4xx / 5xx で exit code 22
            "--silent",                     // 進捗表示を抑制
            "--show-error",                 // エラーは stderr に出す
            "--location",                   // リダイレクト追跡
            "--connect-timeout", "10",
            "--max-time", "60",
            "--output", tmp_str,
            url,
        ])
        .output();
    let output = match result {
        Ok(o) => o,
        Err(e) => return CurlOutcome::SpawnFailed { kind: e.kind(), err: format!("{e}") },
    };
    if output.status.success() {
        return CurlOutcome::Ok;
    }
    let exit_code = output.status.code().unwrap_or(-1);
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    // 一時的失敗 (リトライ対象): 接続不能 / ホスト解決失敗 / タイムアウト
    if matches!(exit_code, 6 | 7 | 28) {
        CurlOutcome::Retryable { exit_code, stderr }
    } else {
        // 22 (HTTP 4xx/5xx) / 35 (TLS handshake) / 60 (cert verify failed) / その他はすべて即時 fail
        CurlOutcome::Permanent { exit_code, stderr }
    }
}
```

採用フラグの根拠:

- `--fail`: HTTP 4xx / 5xx で exit code 22 を返す (これがないとエラー HTML をダウンロードして 200 として扱う)
- `--silent`: テスト実行時の標準出力汚染を抑制
- `--show-error`: silent 併用時もエラーメッセージは stderr に出す (output.stderr で取得)
- `--location`: GitHub raw リダイレクト追跡に必須
- `--connect-timeout 10`: 接続確立までの上限 (秒)
- `--max-time 60`: 全体実行の上限 (秒)
- `-k` / `--insecure` は **絶対に使わない**。SSL 検証で中間者攻撃を防ぐ (SHA-256 検証で間接的に守られるが TLS 層も二重化する)
- `--retry` 系は使わない。curl 組み込みリトライではなくヘルパー側 Rust ループで実装する (exit 22 を即時 fail させたいため、curl 任せにすると 5xx と区別なくリトライされてしまう)
- プロキシ環境変数 (`HTTPS_PROXY` / `NO_PROXY`) は curl が自動参照する
- curl 自体は `Command::new("curl")` で PATH 解決を許可する (絶対パスは強制しない)

リトライ実装 (ヘルパー側ループ):

```rust
fn curl_with_retry(url: &str, tmp_path: &std::path::Path) -> Result<(), FetchError> {
    let mut last: Option<(i32, String)> = None;
    for attempt in 0..3 {
        match run_curl(url, tmp_path) {
            CurlOutcome::Ok => return Ok(()),
            CurlOutcome::Retryable { exit_code, stderr } => {
                last = Some((exit_code, stderr));
                if attempt < 2 {
                    std::thread::sleep(std::time::Duration::from_secs(1u64 << attempt)); // 1 秒、2 秒
                }
            }
            CurlOutcome::Permanent { exit_code, stderr } => {
                return Err(FetchError::NetworkFailure {
                    url: url.to_string(),
                    last_error: format!("curl exit code {exit_code}, stderr: {stderr}"),
                });
            }
            CurlOutcome::SpawnFailed { kind, err } => {
                if kind == std::io::ErrorKind::NotFound {
                    return Err(FetchError::CurlNotFound);
                }
                return Err(FetchError::IoError(err));
            }
        }
    }
    let (exit_code, stderr) = last.expect("Retryable 結果は last に必ず詰めている");
    Err(FetchError::NetworkFailure {
        url: url.to_string(),
        last_error: format!("curl exit code {exit_code} after 3 attempts, stderr: {stderr}"),
    })
}
```

ポイント: 最終 attempt (index 2) では sleep しない (CI 時間の節約。1 秒 + 2 秒 = 計 3 秒)。3 回失敗時の最終エラーは `last` に保持した一時情報を `NetworkFailure` に詰める。

#### OS 既定 hash ツール呼び出し

`#[cfg(target_os = ...)]` の **静的分岐** で 3 OS 用の関数を別実装する (`cfg!()` の動的判定だと dead branch が残るため、静的分岐で未対応 OS を弾く)。

| OS | コマンド | 出力形式 |
|---|---|---|
| macOS | `shasum -a 256 <file>` | `<hex 64>  <file>\n` (2 スペース区切り、小文字) |
| Linux | `sha256sum <file>` | `<hex 64>  <file>\n` (2 スペース区切り、小文字) |
| Windows | `certutil -hashfile <file> SHA256` | 複数行、中央付近の行に hex 64 (Windows 10 以降は連続 64 文字、大文字混在も許容) |

呼び出し擬似コード:

```rust
#[cfg(any(target_os = "macos", target_os = "linux"))]
fn hash_tool() -> (&'static str, &'static [&'static str]) {
    #[cfg(target_os = "macos")]
    return ("shasum", &["-a", "256"]);
    #[cfg(target_os = "linux")]
    return ("sha256sum", &[]);
}

#[cfg(target_os = "windows")]
fn hash_tool() -> (&'static str, &'static [&'static str]) {
    ("certutil", &["-hashfile"])
}

#[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
compile_error!("unsupported target_os for test font hash check");

fn compute_sha256(path: &std::path::Path) -> Result<String, FetchError> {
    let path_str = path
        .to_str()
        .ok_or_else(|| FetchError::IoError("path is not valid UTF-8".to_string()))?;
    let (cmd, fixed_args) = hash_tool();
    let mut args: Vec<&str> = fixed_args.to_vec();
    args.push(path_str);
    #[cfg(target_os = "windows")]
    args.push("SHA256");
    let result = std::process::Command::new(cmd).args(&args).output();
    let output = match result {
        Ok(o) => o,
        Err(e) => {
            if e.kind() == std::io::ErrorKind::NotFound {
                return Err(FetchError::HashToolNotFound(cmd.to_string()));
            }
            return Err(FetchError::IoError(format!("{e}")));
        }
    };
    if !output.status.success() {
        let exit_code = output.status.code().unwrap_or(-1);
        let mut stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        stderr.truncate(stderr.char_indices().nth(512).map_or(stderr.len(), |(i, _)| i));
        return Err(FetchError::HashToolFailure {
            tool: cmd.to_string(),
            last_error: format!("{cmd} exit code {exit_code}, stderr: {stderr}"),
        });
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    parse_hash_output(&stdout).ok_or_else(|| FetchError::HashToolFailure {
        tool: cmd.to_string(),
        last_error: format!("hash parse failed: {stdout}"),
    })
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
fn parse_hash_output(stdout: &str) -> Option<String> {
    // shasum / sha256sum: 先頭 token がそのまま hex 64
    let first = stdout.split_whitespace().next()?;
    (first.len() == 64 && first.chars().all(|c| c.is_ascii_hexdigit()))
        .then(|| first.to_ascii_lowercase())
}

#[cfg(target_os = "windows")]
fn parse_hash_output(stdout: &str) -> Option<String> {
    // certutil: 複数行で中央付近に hex 64 が並ぶ行。
    // 古い版でスペース区切りで出る可能性もあるため、各行から空白を除去して 64 hex を抽出。
    for line in stdout.lines() {
        let cleaned: String = line.chars().filter(|c| !c.is_whitespace()).collect();
        if cleaned.len() == 64 && cleaned.chars().all(|c| c.is_ascii_hexdigit()) {
            return Some(cleaned.to_ascii_lowercase());
        }
    }
    None
}
```

採用根拠:

- 3 OS の hash ツールはすべてのターゲット CI runner に標準同梱されている (macOS: `shasum` Apple 同梱の perl スクリプト / Linux: `sha256sum` GNU coreutils / Windows: `certutil` Windows 標準)
- `#[cfg]` 静的分岐で未対応 OS は `compile_error!` で弾くため、サポート OS の見落としをコンパイル時に検出できる
- パース失敗は `FetchError::HashToolFailure` で Result に吸収し、`panic!` は使わない
- 比較は小文字統一 (`to_ascii_lowercase`) で行う (certutil は大文字、他は小文字を返すため)
- `stderr` は **UTF-8 文字単位** で先頭 512 char に切り詰めて `FetchError` 内に格納する (`char_indices` で multibyte 境界を保持。certutil が数 KB 出力するケースでエラー値が肥大化するのを防ぐ)
- `sha256sum` / `shasum` の出力は GNU coreutils / Apple shasum の標準フォーマット (`<hex64>  <file>`、先頭 token = hex64) で確定。`--tag` (BSD-style `SHA256 (file) = <hex64>`) flag は default では使われない (環境変数 `BSD_SHASUM_FORMAT` 等の override がある場合は CI が壊れるが、CI 環境では default 動作のみ想定)。default 出力で先頭 token を hex64 として読む実装で全 7 構成をカバーする

#### CI 環境での可用性

`.github/workflows/ci.yml` は本 issue では編集しない。`curl` / `sha256sum` (coreutils) は Ubuntu base に標準同梱、`curl` / `shasum` は macOS に標準同梱、`curl.exe` / `certutil.exe` は Windows 10 以降の `C:\Windows\System32\` に標準同梱されている (GitHub Actions runner image ドキュメントで確認できる)。実装着手者は (a) 着手前に各 OS で `<tool> --version` を実行して PATH 解決と起動成功を確認、(b) もし不在ならば auto-resolve を停止しユーザーに報告する責務を負う。

### 検証方式 (`verify_has_table`)

`tests/helpers/font_fetch.rs` 内に sfnt table directory を直接走査するヘルパー `verify_has_table(data: &[u8], tag: [u8; 4]) -> bool` を実装する。0027 完了 (`TableDirectory::parse` の `OTTO` 受理追加 + CFF パース) までは Source Serif 4 を `FontFace::from_data` で読めないため、独立した低レベル走査が必要。

仕様:

- sfnt table directory の 4 byte tag は ASCII で右側スペース padding。本ヘルパー引数 `[u8; 4]` もこの仕様に合わせる (例: CFF タグは `b'C', b'F', b'F', b' '` の末尾スペース必須、CFF2 と区別される)
- offset 0..4 の sfnt version が `0x0001_0000` (TrueType) / `0x4F54544F` (`OTTO`) / `0x7472_7565` (`'true'`、Apple TrueType variant) のいずれかに一致しなければ `false` を返す。raden 本体パーサ (`src/font/tables.rs:66`) が許容する `'true'` も本ヘルパーで受理することで sfnt 受理範囲を本体と揃える
- TTC (sfnt version = `0x7474_6366` = `ttcf`) は本ヘルパーでは `false` 返却 (本 issue 取得対象は単体 TTF / OTF のみで TTC ではない)
- `data.len() < 12` (table directory ヘッダ未満) なら `false`
- offset 4..6 の `num_tables: u16` を読み、offset 12 から `(tag: 4, checksum: 4, offset: 4, length: 4 bytes) × num_tables` の固定レイアウトを走査
- 検証コア (`?` 演算子は `-> bool` 関数内で使えないため `match` で明示):

```rust
pub fn verify_has_table(data: &[u8], tag: [u8; 4]) -> bool {
    if data.len() < 12 {
        return false;
    }
    let sfnt_version = u32::from_be_bytes([data[0], data[1], data[2], data[3]]);
    if !matches!(sfnt_version, 0x0001_0000 | 0x4F54_544F | 0x7472_7565) {
        return false;
    }
    let num_tables = u16::from_be_bytes([data[4], data[5]]) as usize;
    // header_end <= data.len() を確認した時点で、後続ループ内の data[base..base + 4]
    // (base = 12 + i * 16, 0 <= i < num_tables) は安全にアクセスできる
    // (base + 4 <= 12 + num_tables * 16 = header_end <= data.len())。
    // table record 内の offset / length フィールドの妥当性検証は本ヘルパーの責務外
    // (tag の存在判定にのみ専念。各 record の中身の検証は raden 本体 TableDirectory::parse か 0027 の本体置換側で行う)。
    let header_end = match num_tables.checked_mul(16).and_then(|v| v.checked_add(12)) {
        Some(v) if v <= data.len() => v,
        _ => return false,
    };
    let _: usize = header_end; // 上記コメントの不変条件を保持するために計算する (ループ内のインデックスアクセス安全性の根拠)。
    let target = u32::from_be_bytes(tag);
    for i in 0..num_tables {
        let base = 12 + i * 16;
        let record_tag = u32::from_be_bytes([data[base], data[base + 1], data[base + 2], data[base + 3]]);
        if record_tag == target {
            return true;
        }
    }
    false
}
```

raden 本体との関係: 本ヘルパーは 0027 完了までの一時実装。0027 で `TableDirectory::parse` の `OTTO` 受理が追加され、かつ本体に `FontFace::has_table(tag) -> bool` 相当 API が追加されれば置換できる。置換するか否か、本体 API が TTC を扱える場合の挙動差評価は 0027 polish 担当の責務 (本 issue では恒久存続 / 置換のどちらも許容)。

### スモークテスト assertion (確定)

文体規定:

- `assert!` / `assert_eq!` のメッセージは「〜である必要がある」
- `.expect()` のメッセージは「〜できる必要がある」

`tests/test_font.rs` 現状 L1 の `use raden::{Font, FontData, FontFace};` に `FontData` / `FontFace` が既に含まれているため、スモークテスト内で重複 use しない。`mod helpers;` 宣言はファイル冒頭に 1 度書くだけで、各テスト関数の冒頭に再掲しない (擬似コードの `mod helpers;` は配置位置を示すだけ)。

擬似コード (`tests/test_font.rs` ファイル冒頭への追加 + 末尾への 2 テスト追加):

```rust
// ファイル冒頭 (L1 直前):
mod helpers;

use raden::{Font, FontData, FontFace};   // 既存

// ... 既存テスト群 ...

// ファイル末尾に追加:
#[test]
fn fetch_source_sans_3_has_required_tables() {
    let bytes = helpers::font_fetch::fetch_source_sans_3_bytes()
        .unwrap_or_else(|e| panic!("Source Sans 3 をダウンロードできる必要がある: {e:?}"));
    assert!(
        helpers::font_fetch::verify_has_table(&bytes, *b"GSUB"),
        "Source Sans 3 は GSUB テーブルを持つ必要がある"
    );
    assert!(
        helpers::font_fetch::verify_has_table(&bytes, *b"GPOS"),
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
    let bytes = helpers::font_fetch::fetch_source_serif_4_bytes()
        .unwrap_or_else(|e| panic!("Source Serif 4 をダウンロードできる必要がある: {e:?}"));
    assert!(
        helpers::font_fetch::verify_has_table(&bytes, *b"CFF "),
        "Source Serif 4 は CFF テーブルを持つ必要がある"
    );
}
```

`.expect()` ではなく `.unwrap_or_else(|e| panic!(... "{e:?}"))` を採用する根拠: `FetchError` には `Debug` のみ派生し `Display` は実装しないため、panic メッセージで `{e:?}` を使うことで CI 失敗時に curl 終了コード / SHA-256 mismatch 等の根本原因を panic 出力に残せる (`shiguredo-rust` 規約「`.unwrap()` ではなく `.expect("MESSAGE")` を使うこと」は「unwrap 直書きを避ける」の趣旨であり、本テストヘルパーの panic は明示メッセージ付きで規約と整合する)。

スモークテストの並列実行は許容する。ヘルパー設計上 (a) `SEQ AtomicU64` で tmp 名を unique 化、(b) cleanup 対象を **`<filename>` 前方一致 + 自プロセス pid** に限定、(c) 最終ファイル rename は OS の atomic 性で安全、により並列セーフ。`cargo test -- --test-threads=1` への退避は不要。

### ダウンロード機構の公開 API

公開関数とエラー型:

```rust
pub fn fetch_source_sans_3_bytes() -> Result<Vec<u8>, FetchError> {
    fetch_bytes_internal(&SOURCE_SANS_3)
}

pub fn fetch_source_serif_4_bytes() -> Result<Vec<u8>, FetchError> {
    fetch_bytes_internal(&SOURCE_SERIF_4)
}

pub fn verify_has_table(data: &[u8], tag: [u8; 4]) -> bool {
    // 「設計方針 検証方式」セクションの擬似コードのとおり
}

#[derive(Debug)]
pub enum FetchError {
    NetworkFailure { url: String, last_error: String },
    Sha256Mismatch { expected: String, actual: String },
    IoError(String),
    CurlNotFound,
    HashToolNotFound(String),
    HashToolFailure { tool: String, last_error: String },
}
```

`Display` 実装は本 issue では行わない (スモークテストが `panic!("...: {e:?}")` で `Debug` を出力する設計に合わせる)。0025 / 0026 / 0027 polish で必要が生じた時点で追加する。

内部処理 (`fetch_bytes_internal`) の擬似コード:

```rust
fn fetch_bytes_internal(spec: &FontSpec) -> Result<Vec<u8>, FetchError> {
    use std::path::PathBuf;
    let cache_dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("test-fonts");
    std::fs::create_dir_all(&cache_dir).map_err(|e| FetchError::IoError(format!("{e}")))?;
    cleanup_stale_tmp(&cache_dir, spec.filename);

    let final_path = cache_dir.join(spec.filename);

    // (1) 既存最終ファイルが SHA-256 期待値と一致するなら即時再利用
    if final_path.exists() {
        match compute_sha256(&final_path) {
            Ok(actual) if actual == spec.expected_sha256 => {
                return std::fs::read(&final_path).map_err(|e| FetchError::IoError(format!("{e}")));
            }
            Ok(_) => {
                // 不一致なら削除して再ダウンロード経路へ
                let _ = std::fs::remove_file(&final_path);
            }
            Err(e) => return Err(e),
        }
    }

    // (2) ダウンロード + SHA-256 検証セット を最大 2 回試行
    // (curl 内部リトライは 3 回。SHA-256 不一致時の追加再ダウンロードはこのループで管理)
    let mut last_actual = String::new();
    for sha_attempt in 0..2 {
        let tmp_path = make_tmp_path(&cache_dir, spec.filename);
        if sha_attempt == 0 {
            eprintln!(
                "テスト用フォントをダウンロードします: {} (SIL OFL 1.1) {}",
                spec.name, spec.url
            );
        }
        curl_with_retry(spec.url, &tmp_path)?;
        let actual = match compute_sha256(&tmp_path) {
            Ok(a) => a,
            Err(e) => {
                let _ = std::fs::remove_file(&tmp_path);
                return Err(e);
            }
        };
        if actual == spec.expected_sha256 {
            match std::fs::rename(&tmp_path, &final_path) {
                Ok(()) => {}
                Err(e) => {
                    // Windows で他 thread の open 中ファイルとの race の場合、
                    // 最終ファイルが既に SHA-256 一致しているなら成功扱い、それ以外は IoError
                    let recovered = final_path.exists()
                        && compute_sha256(&final_path)
                            .map(|h| h == spec.expected_sha256)
                            .unwrap_or(false);
                    if !recovered {
                        let _ = std::fs::remove_file(&tmp_path);
                        return Err(FetchError::IoError(format!("{e}")));
                    }
                    let _ = std::fs::remove_file(&tmp_path);
                }
            }
            return std::fs::read(&final_path).map_err(|e| FetchError::IoError(format!("{e}")));
        }
        last_actual = actual;
        let _ = std::fs::remove_file(&tmp_path);
    }
    Err(FetchError::Sha256Mismatch {
        expected: spec.expected_sha256.to_string(),
        actual: last_actual,
    })
}

fn make_tmp_path(cache_dir: &std::path::Path, filename: &str) -> std::path::PathBuf {
    use std::sync::atomic::Ordering;
    use std::time::{SystemTime, UNIX_EPOCH};
    let pid = std::process::id();
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let seq = SEQ.fetch_add(1, Ordering::Relaxed);
    cache_dir.join(format!("{filename}.tmp.{pid}.{nanos}.{seq}"))
}

fn cleanup_stale_tmp(cache_dir: &std::path::Path, filename: &str) {
    // best-effort cleanup。失敗は静かに無視する。
    let dir = match std::fs::read_dir(cache_dir) {
        Ok(d) => d,
        Err(_) => return,
    };
    let pid = std::process::id().to_string();
    let prefix = format!("{filename}.tmp.{pid}.");
    for entry in dir.flatten() {
        // non-UTF-8 filename は外部要因のため触らず skip する
        // (本ヘルパーが作る tmp は必ず ASCII filename のためここで漏れることはない)。
        if let Some(name) = entry.file_name().to_str() {
            if name.starts_with(&prefix) {
                let _ = std::fs::remove_file(entry.path());
            }
        }
    }
}
```

`static SEQ` は `tests/helpers/font_fetch.rs` のモジュールトップに `static SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);` で宣言する。上記擬似コード群の上下関係 (関数定義の順序) は Rust の item 順序自由により任意で良い。

SHA-256 不一致時の「ダウンロード + 検証セット 最大 2 回試行」の意味: curl 内部リトライ (3 回) を **再びフルに使う** 試行を 2 回まで行う。これにより「初回 curl が不完全に返した (途中切断) + 2 回目 curl が正常完了」のケースで救済できる。2 回失敗で `Sha256Mismatch`。リトライ 1 回上限の根拠はネットワーク不通は curl リトライで吸収済み、SHA-256 不一致が 2 回続くなら「タグ消失 / URL 改ざん」シグナルなので即時 fail でユーザー通知する。

eprintln! のライセンス通知は `cargo test` 既定の標準で capture されて見えないが、`--nocapture` 実行時とテスト失敗時 (panic 経路) に表示される。`CLAUDE.md` L13「テストのログメッセージは全て日本語にすること」に整合。並列呼び出しで複数回出る可能性は許容する。

### エラーハンドリング

| エッジケース | 確定挙動 |
|---|---|
| ネットワーク不通 (curl exit 6 / 7 / 28) | 指数バックオフで最大 3 回 (1 秒 → 2 秒 → 最終 attempt は sleep なし) リトライ。3 回失敗で `FetchError::NetworkFailure { url, last_error }`。`last_error` は `curl exit code {n} after 3 attempts, stderr: {stderr}` 形式。 |
| HTTP 4xx / 5xx (curl exit 22) | 即時 fail (`FetchError::NetworkFailure`)。リトライしない (タグ消失等の構造的失敗とみなす)。 |
| TLS ハンドシェイク失敗 (curl exit 35) / 証明書検証失敗 (curl exit 60) | 即時 fail (`FetchError::NetworkFailure`)。リトライしない。 |
| SHA-256 不一致 (途中切断による不完全 tmp も含む) | tmp ファイルを削除し「ダウンロード + 検証セット」を 1 回再試行する (curl リトライ含む。初回 1 セット + 再試行 1 セット = 計 2 セット)。2 セット連続失敗で `FetchError::Sha256Mismatch { expected, actual }`。 |
| ディスクフル | `std::io::Error` を `FetchError::IoError(format!("{e}"))` に転送する。 |
| 並列実行 race | スモークテスト 2 件 (`fetch_source_sans_3_has_required_tables` / `fetch_source_serif_4_has_cff_table`) は **異なるフォント** を取得するため、現実的に同一ファイルを同時に書こうとする経路は本 issue では発生しない。仕様としては tmp 名 `<filename>.tmp.<pid>.<nanos>.<seq>` でユニーク化、cleanup は **`<filename>` 前方一致 + 自プロセス pid** の tmp のみが対象 (他フォントの並走 tmp や他プロセス tmp は無視)、最終 rename は POSIX で atomic / Windows でも `MoveFileEx` ベースで上書き可、により並列セーフ。Windows で他 thread が rename 先を open 中だった場合は `ERROR_SHARING_VIOLATION` で失敗しうるが、最終ファイルが既に SHA-256 一致しているなら成功扱いで合流する (擬似コード参照)。同一プロセス内で同じ fetch 関数を別 thread から重複呼び出しするユースケースは想定外 (将来そうした使い方が必要になれば別 issue で `Mutex` / `OnceLock<Vec<u8>>` 等の serialization を検討する)。 |
| release タグ消失 (curl exit 22 / HTTP 404) | 即時 `FetchError::NetworkFailure`。auto-resolve は停止しユーザーに報告 (本ヘルパーの責務は `FetchError` を返すまで)。 |
| curl 不在 (`Command::spawn` が `ErrorKind::NotFound`) | 即時 `FetchError::CurlNotFound`。寛容スキップしない。 |
| OS hash ツール不在 | 即時 `FetchError::HashToolNotFound(tool)`。寛容スキップしない。 |
| OS hash ツール非ゼロ終了 / 出力パース失敗 | `FetchError::HashToolFailure { tool, last_error }`。`panic!` は使わない。`last_error` 内の stderr は **UTF-8 文字単位 (byte 単位ではない)** で先頭 512 char に切り詰める (`char_indices` で multibyte 境界を保持)。 |
| `verify_has_table` で要求テーブル不在 | `false` 返却 → スモークテスト側で日本語アサーション付きで fail。 |

スモークテスト側で `.unwrap_or_else(|e| panic!("...: {e:?}"))` で受ける (`FetchError` の `Debug` 出力で原因が panic ログに残る)。

### ライセンス対応

- フォント本体は **再配布しない** ため、SIL OFL 1.1 Section 4 (Permission Notice 同梱要件) は適用されない。
- Section 3 (Reserved Font Name 制約) は raden がフォント名を改変しないため抵触しない。Source Sans 3 / Source Serif 4 の OFL 表記で `Reserved Font Name 'Source'` を polish 2026-06-22 時点で確認済 (両者とも `'Source'` 指定)。
- 通知はランタイム `eprintln!` で出力する (前述)。Section 4 不適用 (再配布なし) のため法的必須ではないが、テスト実行者が本ヘルパーが SIL OFL 1.1 ライセンス下の外部フォントを取得することを認識する機会を提供する目的で残す。`README.md` / `tests/helpers/README.md` 等への明記は本 issue では行わない。

### 既存 `load_arial()` 経路の扱い

- 本 issue では `tests/test_font.rs:7-14` / `tests/test_context.rs:497-504` / `pbt/tests/prop_font/main.rs:10-17` の `load_arial()` 経路は **変更しない**。
- 本 issue 完了後の世界では macOS 限定 `load_arial()` 経路と全プラットフォーム動作する `fetch_*_bytes` 経路が共存する。共存期間は 0037 (`load_arial()` 集約) の完了まで。
- 0037 と本 issue は `tests/helpers/` ディレクトリで作業が交差するが、本 issue で追加するファイル名は `font_fetch.rs` のため、0037 が想定する `font.rs` とファイル名が衝突しない。`tests/helpers/mod.rs` の作成は本 issue で行う (`pub mod font_fetch;` のみ宣言。`#![cfg(test)]` 等の追加属性は不要 — `tests/` 配下は cargo が test target 用にのみコンパイルするため自明)。0037 着手時に 0037 polish 担当が本 issue 完了状態を見て `pub mod font;` 行を 1 行追加する流れに統一する。
- Rust integration test 慣行に従い、`tests/helpers/mod.rs` 経由で参照する (`#[path]` 直接参照は本 issue では採用しない)。各 test ファイルから参照する場合は `mod helpers;` 宣言 + `helpers::font_fetch::fetch_source_sans_3_bytes()` の形で呼ぶ。Rust integration test では `.rs` 1 ファイル = 1 独立クレートのため、別 test target (`tests/test_context.rs` 等) から参照する場合も各ファイルが個別に `mod helpers;` を宣言する必要がある (0025 / 0026 / 0037 着手時の責務)。なお別 test target は別バイナリにコンパイルされるため `static SEQ` も別インスタンスになる。これは別プロセス相当のため衝突排除に問題なし (cleanup は `<filename>` + 自 pid 限定のため互いに干渉しない)。
- `tests/helpers/font_fetch.rs` 冒頭にモジュールレベル `#![expect(dead_code)]` は **付けない**。一方、`FetchError` の各 variant フィールドは `Debug` 経由 (panic メッセージの `{e:?}` 出力) でのみ read されるため、Rust の dead code 解析は derive された `Debug` 実装内の read を未使用扱いし、コンパイル時に dead_code 警告が出る。これを抑制するため `FetchError` の enum 自体に 1 箇所だけ `#[expect(dead_code, reason = "各 variant のフィールドは Debug 経由でのみ read されるため dead code 解析では未使用扱いになる")]` を付与する (`shiguredo-rust` 規約「`#[allow]` ではなく `#[expect]` を使うこと」に整合)。
- 公開関数 / 公開型は `pub` で宣言する (`pub(crate)` ではなく `pub`)。Rust integration test では `tests/helpers/mod.rs` が test target (`tests/test_font.rs` 等) からのみ参照されるため、`pub` でも同等の可視性となる。`unused` 系の lint 警告は本 issue 完了直後のスモークテスト 2 件で公開 API すべて使用されるため発生しない。
- `tests/helpers/mod.rs` という `mod.rs` ファイル名にすることで、Cargo が `helpers` を独立 integration test として実行しない (`tests/helpers.rs` 直下だと別 test crate になる)。

## 完了条件

### リポジトリ作業ツリーの不変条件

- `git ls-files | grep -E '\.(ttf|otf|ttc|woff2?)$'` が 0 件 (フォントバイナリが 1 つもコミットされていない)
- `git ls-files | grep -iE '(SourceSans|SourceSerif).*LICENSE'` が 0 件 (フォント由来 LICENSE が同梱されていない)
- raden workspace ルートで `cargo package --list 2>&1 | grep -E '^(tests|target)/'` が 0 件 (publish 配布物にテストヘルパーが混入しないことの検証)
- `git diff develop -- Cargo.toml Cargo.lock` が空 (依存追加なしの検証)

### 動作

- `cargo test --workspace` が CI 全 7 構成 (`ubuntu-24.04` / `ubuntu-24.04-arm` / `ubuntu-22.04` / `ubuntu-22.04-arm` / `macos-26` / `macos-15` / `windows-2025`) で pass する。pass 判定は「スキップを除いた全テストが pass し、スキップは macOS 限定 `Arial.ttf` 直参照を含むテスト群のみ」とする。
- ローカルで `cargo test` 単独実行時にダウンロード機構が動作する (初回はネットワーク必須、以降はキャッシュ利用)
- 新規スモークテスト 2 件 (`fetch_source_sans_3_has_required_tables` / `fetch_source_serif_4_has_cff_table`) が `tests/test_font.rs` で全 7 構成で pass する
- `.github/workflows/ci.yml` は本 issue では編集しない (curl / hash ツールは全 CI runner で標準同梱されているため)

### テスト

- `tests/test_font.rs` に新規スモークテスト 2 件を追加する (実装は設計方針セクションの擬似コードに準拠)
- 既存 `load_arial()` 経路 (`tests/test_font.rs` / `tests/test_context.rs` / `pbt/tests/prop_font/main.rs`) は本 issue では変更しない
- アサーションメッセージは日本語 (`CLAUDE.md` 規約)
- `verify_has_table` の独自走査ロジックの PBT / Fuzzing は本 issue では追加しない (テスト基盤の限定的なヘルパーであり、raden 本体 `TableDirectory::parse` の PBT で sfnt 走査の不変条件は別途 0001 メタ issue tracked のスコープで検証)

### 依存追加なし

- `Cargo.toml` の `[dependencies]` / `[dev-dependencies]` / `[build-dependencies]` および `include` / `exclude` を編集しない
- `Cargo.lock` を編集しない
- `.gitignore` を編集しない (既存の `target/` 包括除外でキャッシュ位置がカバーされる)
- `.github/workflows/ci.yml` を編集しない
- HTTPS 取得 / SHA-256 検証はすべて `std::process::Command` 経由の外部コマンド呼び出しで完結させる

### CHANGES.md

- 公開 API 変更なし・描画結果変化なしのテスト基盤変更のみのため `CHANGES.md` への記載は不要 (`shiguredo-changelog` SKILL.md L20「`.rst` / `.md` ファイルの変更は変更履歴に反映しないこと」)。

### 0001 メタ issue 更新 (本 issue 完了 PR 内)

`issues/0001-enhance-font-module-maturity.md` を以下のとおり更新する (auto-resolve スキルの「1 issue 1 コミット」原則のため、実装と同一コミットに同梱する):

1. `## Concrete issue 一覧` テーブル (L100-108) に `0039` 行を追加 (Priority: High、状態: closed)
2. `## Tracked items` テーブル (L116-123) から「テスト用フォント選定」行を削除
3. `### tracked: テスト用フォント選定` サブセクション (L138-143) 全体を削除
4. `### 前提 concrete issue` リスト (L165-172) の「1. テスト用フォント選定」項目を削除し、後続 1〜3 番に番号繰り上げ
5. `### クロスプラットフォーム` 完了条件 L244 (`全プラットフォームで利用可能なテスト用フォントバイナリがリポジトリに含まれる`) を「`fetch_source_sans_3_bytes` / `fetch_source_serif_4_bytes` 経由で全プラットフォームのフォントテストが pass する」に書き換える (L243 と L245 は文意が生きているため触らない)
6. `### パフォーマンス基準` L227 の「測定対象フォントはテスト用フォント選定で確定する」を「測定対象フォントは `fetch_source_sans_3_bytes` で取得する Source Sans 3 を使う」に書き換える
7. `### CFF / CFF2 判断` L239 「テスト用フォント選定 tracked item には CFF / CFF2 フォントの選定を含める」を「CFF / CFF2 テストは `fetch_source_serif_4_bytes` で取得する Source Serif 4 を使う」に書き換える
8. `## 備考` (L196-200) に「CJK 統合漢字 / BMP 外文字のカバレッジは Source Sans 3 / Source Serif 4 では満たせない。必要になった時点で別 tracked / 別 issue を起票する」を 1 行追記
9. `## 備考` (L196-200) に「0039 closed 後、0025 / 0026 / 0027 / 0037 を `/polish-issue` で個別に再 polish し、本文中の『リポジトリ同梱前提』を `fetch_source_sans_3_bytes` / `fetch_source_serif_4_bytes` 参照に書き換える。0025 polish では Source Sans 3 / Source Serif 4 のいずれも `kern` テーブルを持たない事実を踏まえ、kern fallback 動作確認 (`Font::kern() == 0`) に再定義するか、`kern` 付きフォント追加選定の別 issue を起票するかを確定する。0027 polish では `TableDirectory::parse` (`src/font/tables.rs:58-93`) の sfnt version 判定に `0x4F54544F` (`OTTO`) を追加する責務を含める。0037 polish では本 issue が新設した `tests/helpers/mod.rs` に `pub mod font;` 行を 1 行追加する流れを前提とする。Source Sans 3 のバージョン更新時は SHA-256 と同時に `kern` テーブル有無の再確認を必須とする」を 1 段落追記

## 解決方法

設計方針セクションで確定した方針に従い、依存追加なしで次の実装を行った。

- `tests/helpers/mod.rs` を新設し `pub mod font_fetch;` を 1 行宣言した。
- `tests/helpers/font_fetch.rs` を新設し、Source Sans 3 Regular (TTF) / Source Serif 4 Regular (OTF / CFF1) のダウンロードヘルパー (`fetch_source_sans_3_bytes` / `fetch_source_serif_4_bytes`)、sfnt table directory 走査ヘルパー (`verify_has_table`)、エラー型 (`FetchError`) を実装した。HTTPS 取得は `curl` 子プロセス呼び出しでリトライ付き (exit code 6/7/18/28/52/55/56/92 をリトライ対象、22/35/60 等は即時 fail)、SHA-256 検証は OS 別の標準ハッシュツール (`shasum` / `sha256sum` / `certutil`) を子プロセス呼び出しで行い、検証成功後に `std::fs::rename` で `<CARGO_TARGET_TMPDIR>/test-fonts/` にキャッシュする。tmp 名は `<filename>.tmp.<pid>.<nanos>.<seq>` で並列セーフ。dead_code 警告は `FetchError` enum 1 箇所に `#[expect(dead_code, reason = ...)]` で抑制した。
- `tests/test_font.rs` に `mod helpers;` 宣言と `use helpers::font_fetch::{fetch_source_sans_3_bytes, fetch_source_serif_4_bytes, verify_has_table};` を追加し、末尾にスモークテスト 2 件 (`fetch_source_sans_3_has_required_tables` / `fetch_source_serif_4_has_cff_table`) を追加した。GSUB / GPOS / CFF テーブルの存在確認と、Source Sans 3 の `FontFace::from_data` ロード確認を行う。
- `issues/0001-enhance-font-module-maturity.md` を完了条件「0001 メタ issue 更新」項目 1〜9 に沿って更新した (`0039` 行を Concrete issue 一覧に追加、Tracked items から「テスト用フォント選定」を削除、`### tracked: テスト用フォント選定` サブセクション全体を削除、前提 concrete issue リストの番号繰り上げ、パフォーマンス基準 / CFF / CFF2 判断 / クロスプラットフォーム完了条件の本文書き換え、備考に CJK カバレッジと再 polish 申し送りを追記)。
- `Cargo.toml` / `Cargo.lock` / `.gitignore` / `.github/workflows/ci.yml` は編集していない。

ローカルで `cargo fmt --all --check` / `cargo clippy --workspace --all-targets -- -D warnings` / `cargo test --workspace` を pass させた。スモークテスト 2 件も含めて全 14 件が `tests/test_font.rs` で pass する。

## 変更対象ファイル

| ファイル | 種別 | 内容 |
|---|---|---|
| `tests/helpers/mod.rs` | 新規 | `pub mod font_fetch;` を 1 行のみ宣言 |
| `tests/helpers/font_fetch.rs` | 新規 | 設計方針セクションの擬似コードに準拠して `FontSpec` 定数 2 件 / `FetchError` / `fetch_source_sans_3_bytes` / `fetch_source_serif_4_bytes` / `verify_has_table` / 内部関数 (`fetch_bytes_internal` / `run_curl` / `curl_with_retry` / `compute_sha256` / `hash_tool` / `parse_hash_output` / `make_tmp_path` / `cleanup_stale_tmp`) / `static SEQ: AtomicU64` を実装する |
| `tests/test_font.rs` | 既存編集 | 現状 L1 (`use raden::{Font, FontData, FontFace};`) の直前に `mod helpers;` を 1 行追加 + 空行 1 行。新規スモークテスト 2 件を末尾 (現状 L229 直後) に追加。既存 use 文 (`FontData` / `FontFace`) はそのまま流用、新規 use 追加なし |
| `issues/0001-enhance-font-module-maturity.md` | 既存編集 | 完了条件「0001 メタ issue 更新」項目 1〜9 を実施 |

`Cargo.toml` / `Cargo.lock` / `.gitignore` / `.github/workflows/ci.yml` は **本 issue では編集しない**。

`issues/0025-add-font-kerning.md` / `issues/0026-add-opentype-basic-shaping.md` / `issues/0027-add-cff-cff2-outline-support.md` / `issues/0037-refactor-tests-common-font-helper.md` は本 issue PR では編集しない (`shiguredo-issues` SKILL.md L59「`Polished:` を更新できるのは `/polish-issue` スキルのみ」に整合)。本 issue closed 後にユーザーが `/polish-issue 0025 0026 0027 0037` を順次実行する想定。再 polish 時に行う作業内容は 0001 メタ issue の備考に申し送り済み (完了条件「0001 メタ issue 更新」項目 9 参照)。

## 関連

- `0001-enhance-font-module-maturity.md` (メタ issue)
- `0025-add-font-kerning.md` (kern fallback 動作確認の前提)
- `0026-add-opentype-basic-shaping.md` (GSUB / GPOS 付きフォント前提)
- `0027-add-cff-cff2-outline-support.md` (CFF / CFF2 フォント前提)
- `0037-refactor-tests-common-font-helper.md` (`load_arial()` 集約。本 issue が `tests/helpers/mod.rs` を新設)
