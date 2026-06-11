//! 「バックエンド窓取得」セッション管理
//!
//! Everything でソート済み全件（数十万件）を取得して Rust 側 State に常駐させ、
//! フロントの可視ウィンドウ分だけ `fetch_window` で返す
//! （Everything の LVS_OWNERDATA owner-data ListView と同じモデル）。
//! 1回の invoke で全件 JSON 転送すると webview メインスレッドが固まるための対策。

use std::sync::Mutex;

use search::{BrowseRow, MatchOptions};
use serde::{Deserialize, Serialize};

/// `browse` で取得したソート済み全件を保持する状態。
pub struct BrowseState(pub Mutex<Vec<BrowseRow>>);

/// メモリ安全のための取得上限件数。
const BROWSE_MAX: u32 = 2_000_000;

/// フロントから渡されるソート指定。
#[derive(Debug, Deserialize)]
pub struct BrowseSort {
    /// "name" | "folder" | "size" | "date"
    pub col: String,
    pub asc: bool,
}

/// `fetch_window` が返す1行分（フロント表示用に派生済み）。
#[derive(Debug, Serialize)]
pub struct WindowRow {
    pub name: String,
    pub path: String,
    pub folder: String,
    pub is_dir: bool,
    pub ext: String,
    pub size: u64,
    pub modified: String,
}

/// 列名+昇降順から Everything ソート定数へ変換する。
fn sort_const(col: &str, asc: bool) -> u32 {
    match (col, asc) {
        ("name", true) => search::EVERYTHING_SORT_NAME_ASCENDING,
        ("name", false) => search::EVERYTHING_SORT_NAME_DESCENDING,
        ("folder", true) => search::EVERYTHING_SORT_PATH_ASCENDING,
        ("folder", false) => search::EVERYTHING_SORT_PATH_DESCENDING,
        ("size", true) => search::EVERYTHING_SORT_SIZE_ASCENDING,
        ("size", false) => search::EVERYTHING_SORT_SIZE_DESCENDING,
        ("date", true) => search::EVERYTHING_SORT_DATE_MODIFIED_ASCENDING,
        ("date", false) => search::EVERYTHING_SORT_DATE_MODIFIED_DESCENDING,
        (_, true) => search::EVERYTHING_SORT_NAME_ASCENDING,
        (_, false) => search::EVERYTHING_SORT_NAME_DESCENDING,
    }
}

/// Everything でソート済み全件を取得し、State に格納する。
///
/// 戻り値は総件数（フロントは仮想スクロールの全長として使う）。
#[tauri::command]
pub fn browse(
    query: String,
    sort: BrowseSort,
    options: Option<MatchOptions>,
    state: tauri::State<BrowseState>,
) -> Result<usize, String> {
    let rows = search::browse(&query, sort_const(&sort.col, sort.asc), BROWSE_MAX, options.unwrap_or_default())
        .map_err(|e| e.to_string())?;
    let len = rows.len();

    let mut guard = state.0.lock().map_err(|e| e.to_string())?;
    *guard = rows;

    Ok(len)
}

/// 直近の `browse` 結果から `[offset, offset+limit)` のウィンドウを返す。
///
/// `offset` が全件数以上の場合は空 Vec を返す。
#[tauri::command]
pub fn fetch_window(
    offset: usize,
    limit: usize,
    state: tauri::State<BrowseState>,
) -> Result<Vec<WindowRow>, String> {
    let guard = state.0.lock().map_err(|e| e.to_string())?;

    if offset >= guard.len() {
        return Ok(Vec::new());
    }

    let end = (offset + limit).min(guard.len());
    let window = guard[offset..end]
        .iter()
        .map(|row| {
            let path = std::path::Path::new(&row.path);
            let name = path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            let folder = path
                .parent()
                .map(|p| p.to_string_lossy().into_owned())
                .unwrap_or_default();
            let ext = if row.is_dir {
                String::new()
            } else {
                path.extension()
                    .map(|e| e.to_string_lossy().into_owned())
                    .unwrap_or_default()
            };

            WindowRow {
                name,
                path: row.path.clone(),
                folder,
                is_dir: row.is_dir,
                ext,
                size: row.size,
                modified: search::format_mtime(row.mtime),
            }
        })
        .collect();

    Ok(window)
}
