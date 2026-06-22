//! テスト用フォントのダウンロード・キャッシュ・SHA-256 検証ヘルパー。
//!
//! Source Sans 3 / Source Serif 4 を初回利用時に HTTPS 経由でダウンロードし、
//! `<CARGO_TARGET_TMPDIR>/test-fonts/` にキャッシュする。実装は依存クレートを
//! 一切追加せず、HTTPS 取得は `curl` の子プロセス呼び出し、SHA-256 検証は
//! 各 OS の標準ハッシュツール (macOS: `shasum`, Linux: `sha256sum`, Windows:
//! `certutil`) の子プロセス呼び出しで完結させる。
//!
//! 並列スモークテストでの race を避けるため、tmp ファイル名は
//! `<filename>.tmp.<pid>.<nanos>.<seq>` 形式でユニーク化し、SHA-256 検証成功後に
//! `std::fs::rename` で atomic に最終ファイル名へ置換する。

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

/// ダウンロード対象フォントの仕様。
struct FontSpec {
    /// ライセンス通知に表示する表示名。
    name: &'static str,
    /// 取得 URL。GitHub Release タグ固定で安定参照。
    url: &'static str,
    /// 期待される SHA-256 (小文字 hex 64)。
    expected_sha256: &'static str,
    /// キャッシュ時のファイル名。
    filename: &'static str,
}

/// Source Sans 3 Regular (TTF, Adobe Fonts 3.052R, SIL OFL 1.1)。
/// GSUB / GPOS テストの対象フォント。
const SOURCE_SANS_3: FontSpec = FontSpec {
    name: "Source Sans 3 Regular",
    url: "https://github.com/adobe-fonts/source-sans/raw/refs/tags/3.052R/TTF/SourceSans3-Regular.ttf",
    expected_sha256: "4644c81b86ec9caaa76b634889968ed3c4f4f52f054855933acc7c2b21e53b0f",
    filename: "SourceSans3-Regular.ttf",
};

/// Source Serif 4 Regular (OTF / CFF1, Adobe Fonts 4.005R, SIL OFL 1.1)。
/// CFF アウトラインテストの対象フォント。
const SOURCE_SERIF_4: FontSpec = FontSpec {
    name: "Source Serif 4 Regular",
    url: "https://github.com/adobe-fonts/source-serif/raw/refs/tags/4.005R/OTF/SourceSerif4-Regular.otf",
    expected_sha256: "edf160d0d584deee8a3bb2c3371b2a7624ca63580fbe02c57c1f4c91e84d8787",
    filename: "SourceSerif4-Regular.otf",
};

/// 一時ファイル名のユニーク化に使う単調増加カウンタ。
///
/// 同一プロセス内で並列 thread から fetch を呼び出した場合、
/// SystemTime ベースの nanos が衝突する可能性がある。
/// このカウンタの単調増加により tmp 名のユニーク性を保証する。
/// Relaxed で十分な根拠: tmp 名のユニーク識別のみが目的であり、
/// 他のメモリ操作との順序関係を必要としない。
static SEQ: AtomicU64 = AtomicU64::new(0);

/// 取得・検証時のエラー。`Debug` のみ派生し、`Display` は本ヘルパーでは実装しない。
/// 呼び出し側 (スモークテスト) は `panic!("...: {e:?}")` で `Debug` 出力を使う想定。
///
/// `dead_code` 抑制の根拠: 各 variant のフィールドは `Debug` 経由
/// (panic メッセージの `{e:?}` 出力) でのみ read される。
/// Rust の dead code 解析は derive された `Debug` 実装内の read を未使用扱いするため、
/// この lint をここで抑制しないとビルドが警告で汚れる。
#[derive(Debug)]
#[expect(
    dead_code,
    reason = "各 variant のフィールドは Debug 経由でのみ read されるため dead code 解析では未使用扱いになる"
)]
pub enum FetchError {
    /// 取得に失敗した (curl 不在以外のネットワーク・HTTP・TLS 失敗)。
    NetworkFailure { url: String, last_error: String },
    /// SHA-256 検証で期待値と異なる hash が返った。
    Sha256Mismatch { expected: String, actual: String },
    /// ローカル I/O エラー (rename / read / mkdir 等)。
    IoError(String),
    /// curl コマンドが PATH 上で見つからない。
    CurlNotFound,
    /// OS 既定 hash ツールが PATH 上で見つからない。
    HashToolNotFound(String),
    /// OS 既定 hash ツールが非ゼロ終了 / 出力パース失敗。
    HashToolFailure { tool: String, last_error: String },
}

/// Source Sans 3 Regular (TTF) をダウンロードして bytes を返す。
///
/// 初回はネットワークから取得し SHA-256 を検証してキャッシュする。
/// 以降は `<CARGO_TARGET_TMPDIR>/test-fonts/SourceSans3-Regular.ttf` を再利用する。
pub fn fetch_source_sans_3_bytes() -> Result<Vec<u8>, FetchError> {
    fetch_bytes_internal(&SOURCE_SANS_3)
}

/// Source Serif 4 Regular (OTF / CFF1) をダウンロードして bytes を返す。
///
/// 初回はネットワークから取得し SHA-256 を検証してキャッシュする。
/// 以降は `<CARGO_TARGET_TMPDIR>/test-fonts/SourceSerif4-Regular.otf` を再利用する。
pub fn fetch_source_serif_4_bytes() -> Result<Vec<u8>, FetchError> {
    fetch_bytes_internal(&SOURCE_SERIF_4)
}

/// sfnt table directory を直接走査し、指定 4 文字 tag を持つテーブルが
/// 存在するかを返す。
///
/// raden 本体 `TableDirectory::parse` は現在 CFF 系 (sfnt version `OTTO`) を
/// 未受理のため、CFF テスト用フォントを `FontFace::from_data` 経由で扱えない。
/// このヘルパーは sfnt 受理範囲を `0x0001_0000` (TrueType) / `0x4F54_544F`
/// (`OTTO`, OpenType with CFF) / `0x7472_7565` (`'true'`, Apple TrueType variant)
/// の 3 種に広げて低レベル走査だけを行う。
///
/// tag は ASCII 4 文字。短い tag は呼び出し側で右側スペース padding を行う
/// (例: CFF テーブルは `*b"CFF "`、CFF2 と区別される)。
/// 各 table record の中身 (offset / length / checksum) の妥当性検証は
/// 本ヘルパーの責務外。
pub fn verify_has_table(data: &[u8], tag: [u8; 4]) -> bool {
    // table directory ヘッダ最小サイズ (sfnt version 4 + num_tables 2 +
    // search_range 2 + entry_selector 2 + range_shift 2 = 12) 未満は無効。
    if data.len() < 12 {
        return false;
    }
    let sfnt_version = u32::from_be_bytes([data[0], data[1], data[2], data[3]]);
    if !matches!(sfnt_version, 0x0001_0000 | 0x4F54_544F | 0x7472_7565) {
        // TTC (ttcf = 0x7474_6366) は本ヘルパーでは false。
        // 取得対象は単体 TTF / OTF のみで TTC ではないため、ここで弾く。
        return false;
    }
    let num_tables = u16::from_be_bytes([data[4], data[5]]) as usize;
    // header_end <= data.len() を確認した時点で、後続ループ内の
    // data[base..base + 4] (base = 12 + i * 16, 0 <= i < num_tables) が
    // 安全にアクセスできる
    // (base + 4 <= 12 + num_tables * 16 = header_end <= data.len())。
    match num_tables.checked_mul(16).and_then(|v| v.checked_add(12)) {
        Some(header_end) if header_end <= data.len() => {}
        _ => return false,
    }
    let target = u32::from_be_bytes(tag);
    for i in 0..num_tables {
        let base = 12 + i * 16;
        let record_tag =
            u32::from_be_bytes([data[base], data[base + 1], data[base + 2], data[base + 3]]);
        if record_tag == target {
            return true;
        }
    }
    false
}

/// `fetch_*_bytes` の内部実装。キャッシュ確認 → ダウンロード →
/// SHA-256 検証 → atomic rename → bytes 読み出しを行う。
fn fetch_bytes_internal(spec: &FontSpec) -> Result<Vec<u8>, FetchError> {
    let cache_dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("test-fonts");
    std::fs::create_dir_all(&cache_dir).map_err(|e| FetchError::IoError(format!("{e}")))?;
    cleanup_stale_tmp(&cache_dir, spec.filename);

    let final_path = cache_dir.join(spec.filename);

    // 既存最終ファイルが SHA-256 期待値と一致するなら即時再利用する。
    if final_path.exists() {
        match compute_sha256(&final_path) {
            Ok(actual) if actual == spec.expected_sha256 => {
                return std::fs::read(&final_path).map_err(|e| FetchError::IoError(format!("{e}")));
            }
            Ok(_) => {
                // hash 不一致なら削除して再ダウンロード経路に合流する。
                let _ = std::fs::remove_file(&final_path);
            }
            Err(e) => return Err(e),
        }
    }

    // ダウンロード + SHA-256 検証を最大 2 回試行する。
    // 内部 curl リトライは 3 回まで、SHA-256 不一致時の追加再ダウンロードは
    // この外側ループで管理する (途中切断による不完全 tmp の救済が目的)。
    let mut last_actual = String::new();
    for sha_attempt in 0..2 {
        let tmp_path = make_tmp_path(&cache_dir, spec.filename);
        if sha_attempt == 0 {
            // ライセンス通知。`cargo test` の既定では capture されるが、
            // `--nocapture` 実行時とテスト失敗の panic 経路で表示される。
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
                    // Windows で他 thread / プロセスが rename 先を open 中の race を想定。
                    // 最終ファイルが既に SHA-256 期待値と一致しているなら成功扱いで合流する。
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

/// `<filename>.tmp.<pid>.<nanos>.<seq>` 形式の一時ファイルパスを生成する。
///
/// 並列 thread / プロセスでのファイル名衝突を避けるため pid と単調増加 seq を
/// 組み合わせる。clock skew で nanos が 0 に落ちても seq の単調増加で
/// ユニーク化される (clock skew への安全網)。
fn make_tmp_path(cache_dir: &Path, filename: &str) -> PathBuf {
    let pid = std::process::id();
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let seq = SEQ.fetch_add(1, Ordering::Relaxed);
    cache_dir.join(format!("{filename}.tmp.{pid}.{nanos}.{seq}"))
}

/// `<filename>.tmp.<pid>.*` 形式の残骸を best-effort で削除する。
///
/// 削除対象は **自プロセス pid と filename 一致** の tmp のみで、他プロセスの
/// 作業中 tmp や他フォントの並走 tmp は触らない。同一プロセス・同一フォントを
/// 並列 thread から重複呼び出しすると in-flight tmp を巻き込みうるが、
/// 本ヘルパーはそのユースケースを想定外とする。
/// `read_dir` / `remove_file` 失敗はすべて無視する (権限不在等)。
fn cleanup_stale_tmp(cache_dir: &Path, filename: &str) {
    let dir = match std::fs::read_dir(cache_dir) {
        Ok(d) => d,
        Err(_) => return,
    };
    let pid = std::process::id().to_string();
    let prefix = format!("{filename}.tmp.{pid}.");
    for entry in dir.flatten() {
        // 非 UTF-8 ファイル名は外部要因のため触らず skip する。
        // 本ヘルパーが作る tmp は必ず ASCII filename のためここで漏れない。
        if let Some(name) = entry.file_name().to_str()
            && name.starts_with(&prefix)
        {
            let _ = std::fs::remove_file(entry.path());
        }
    }
}

/// curl の終了状態。リトライ可否を呼び出し側で判定するための内部判定型。
enum CurlOutcome {
    Ok,
    /// 接続不能 / ホスト解決失敗 / タイムアウト等。指数バックオフでリトライする。
    Retryable {
        exit_code: i32,
        stderr: String,
    },
    /// HTTP 4xx/5xx / TLS handshake 失敗 / 証明書検証失敗等。即時 fail とする。
    Permanent {
        exit_code: i32,
        stderr: String,
    },
    /// curl 自体が起動できなかった (PATH 不在 / 権限不足等)。
    SpawnFailed {
        kind: std::io::ErrorKind,
        err: String,
    },
}

/// 単発の curl 呼び出し。`--insecure` 系のフラグは絶対に使わない (SSL 検証維持)。
fn run_curl(url: &str, tmp_path: &Path) -> CurlOutcome {
    let tmp_str = match tmp_path.to_str() {
        Some(s) => s,
        None => {
            return CurlOutcome::SpawnFailed {
                kind: std::io::ErrorKind::InvalidInput,
                err: "tmp path is not valid UTF-8".to_string(),
            };
        }
    };
    // `--fail`: HTTP 4xx/5xx で exit code 22 を返す (エラー HTML を 200 として
    //           ダウンロードしないため必須)。
    // `--silent` + `--show-error`: 進捗表示を抑制しつつエラーは stderr に出す。
    // `--location`: GitHub raw リダイレクト追跡に必須。
    // `--connect-timeout` / `--max-time`: 接続確立と全体実行の上限秒。
    // `--retry` 系は使わない (exit 22 を即時 fail させたいため、本体側でリトライする)。
    let result = std::process::Command::new("curl")
        .args([
            "--fail",
            "--silent",
            "--show-error",
            "--location",
            "--connect-timeout",
            "10",
            "--max-time",
            "60",
            "--output",
            tmp_str,
            url,
        ])
        .output();
    let output = match result {
        Ok(o) => o,
        Err(e) => {
            return CurlOutcome::SpawnFailed {
                kind: e.kind(),
                err: format!("{e}"),
            };
        }
    };
    if output.status.success() {
        return CurlOutcome::Ok;
    }
    let exit_code = output.status.code().unwrap_or(-1);
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    // 一時的失敗 (リトライ対象):
    //   6 = couldn't resolve host
    //   7 = couldn't connect to host
    //   18 = transferred partial file (途中切断)
    //   28 = operation timeout
    //   52 = server returned nothing (empty reply)
    //   55 = failure sending network data
    //   56 = failure receiving network data
    //   92 = HTTP/2 stream error (GitHub raw は HTTP/2 を使う)
    if matches!(exit_code, 6 | 7 | 18 | 28 | 52 | 55 | 56 | 92) {
        CurlOutcome::Retryable { exit_code, stderr }
    } else {
        // 22 = HTTP 4xx/5xx (`--fail`), 35 = TLS handshake,
        // 60 = cert verify failed 等の構造的失敗はリトライしない。
        CurlOutcome::Permanent { exit_code, stderr }
    }
}

/// curl をリトライ付きで呼び出す。
///
/// 3 回試行し、最終 attempt 後は sleep しない (1 秒 + 2 秒 = 計 3 秒、CI 時間の節約)。
fn curl_with_retry(url: &str, tmp_path: &Path) -> Result<(), FetchError> {
    let mut last: Option<(i32, String)> = None;
    for attempt in 0..3 {
        match run_curl(url, tmp_path) {
            CurlOutcome::Ok => return Ok(()),
            CurlOutcome::Retryable { exit_code, stderr } => {
                last = Some((exit_code, stderr));
                if attempt < 2 {
                    std::thread::sleep(std::time::Duration::from_secs(1u64 << attempt));
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

// OS 別 hash ツール選択。`#[cfg]` の静的分岐で未対応 OS をコンパイル時に弾く
// (動的判定だと dead branch が残り、サポート OS の見落としを実行時まで検出できないため)。

#[cfg(target_os = "macos")]
fn hash_tool() -> (&'static str, &'static [&'static str]) {
    // Apple shasum は perl スクリプトで `-a 256` がアルゴリズム指定。
    ("shasum", &["-a", "256"])
}

#[cfg(target_os = "linux")]
fn hash_tool() -> (&'static str, &'static [&'static str]) {
    // GNU coreutils の sha256sum。引数追加なしで SHA-256 を計算する。
    ("sha256sum", &[])
}

#[cfg(target_os = "windows")]
fn hash_tool() -> (&'static str, &'static [&'static str]) {
    // Windows 10 以降標準同梱の certutil。`-hashfile <path> SHA256` 形式で呼ぶ。
    ("certutil", &["-hashfile"])
}

#[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
compile_error!("unsupported target_os for test font hash check");

/// 指定ファイルの SHA-256 hash を小文字 hex 64 で返す。
///
/// 子プロセス起動失敗 / 非ゼロ終了 / 出力パース失敗を `FetchError` に吸収し、
/// `panic!` は使わない。
fn compute_sha256(path: &Path) -> Result<String, FetchError> {
    let path_str = path
        .to_str()
        .ok_or_else(|| FetchError::IoError("path is not valid UTF-8".to_string()))?;
    let (cmd, fixed_args) = hash_tool();
    // Command::arg を順次呼んで lifetime の混在 (`&'static` と関数内ローカル) を回避する。
    let mut command = std::process::Command::new(cmd);
    command.args(fixed_args);
    command.arg(path_str);
    #[cfg(target_os = "windows")]
    command.arg("SHA256");
    let result = command.output();
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
        // certutil が数 KB 出力するケースで FetchError 値の肥大化を防ぐ。
        // multibyte 境界を保持するため char 単位で切り詰める (byte 単位は禁止)。
        stderr.truncate(
            stderr
                .char_indices()
                .nth(512)
                .map_or(stderr.len(), |(i, _)| i),
        );
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

/// macOS / Linux 用の hash 出力パーサ。
///
/// `shasum -a 256 <file>` / `sha256sum <file>` の default 出力フォーマットは
/// `<hex64>  <file>\n` で、先頭 token がそのまま SHA-256 値。
#[cfg(any(target_os = "macos", target_os = "linux"))]
fn parse_hash_output(stdout: &str) -> Option<String> {
    let first = stdout.split_whitespace().next()?;
    if first.len() == 64 && first.chars().all(|c| c.is_ascii_hexdigit()) {
        Some(first.to_ascii_lowercase())
    } else {
        None
    }
}

/// Windows 用の hash 出力パーサ。
///
/// certutil は複数行で中央付近に hex 64 (大文字混在も許容) を出力する。
/// 古い版でスペース区切りの可能性もあるため、各行から空白を除去して
/// 連続 64 hex を抽出し、小文字化して返す。
#[cfg(target_os = "windows")]
fn parse_hash_output(stdout: &str) -> Option<String> {
    for line in stdout.lines() {
        let cleaned: String = line.chars().filter(|c| !c.is_whitespace()).collect();
        if cleaned.len() == 64 && cleaned.chars().all(|c| c.is_ascii_hexdigit()) {
            return Some(cleaned.to_ascii_lowercase());
        }
    }
    None
}
