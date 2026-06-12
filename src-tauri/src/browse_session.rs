//! 「バックエンド窓取得」セッション管理
//!
//! Everything でソート済み全件（数十万件）を取得して Rust 側 State に常駐させ、
//! フロントの可視ウィンドウ分だけ `fetch_window` で返す
//! （Everything の LVS_OWNERDATA owner-data ListView と同じモデル）。
//! 1回の invoke で全件 JSON 転送すると webview メインスレッドが固まるための対策。

use std::sync::Mutex;
use std::time::Instant;

use search::{BrowseRow, MatchOptions};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter};

/// `browse` で取得したソート済み全件を、確定時の検索世代とともに保持する状態。
///
/// 非同期コマンド化により browse は並行実行され得る。総件数（フロントへ返す）と
/// スナップショット（fetch_window が読む）を必ず同一世代で対応付けるため、
/// 世代番号を添えて一体で差し替える。
pub struct BrowseState(pub Mutex<(u64, Vec<BrowseRow>)>);

/// メモリ安全のための取得上限件数。
const BROWSE_MAX: u32 = 2_000_000;

/// フロントから渡されるソート指定。
#[derive(Debug, Deserialize)]
pub struct BrowseSort {
    /// "name" | "folder" | "size" | "date"
    pub col: String,
    pub asc: bool,
}

/// 全件抽出中の進捗（`progress_channel` 経由で emit）。
#[derive(Debug, Clone, Serialize)]
pub struct BrowseProgress {
    /// これまでに抽出した件数。
    pub count: usize,
    /// browse 開始からの経過ミリ秒。
    pub elapsed_ms: u64,
}

/// `browse` の戻り値。総件数と、そのスナップショットを常駐させた検索世代。
///
/// フロントは「最大世代のみ採用」することで、並行 browse による
/// 総件数とスナップショットの世代ズレ（途中までしか表示されない不整合）を防ぐ。
#[derive(Debug, Clone, Serialize)]
pub struct BrowseResult {
    pub total: usize,
    pub generation: u64,
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
///
/// `#[tauri::command(async)]` によりワーカースレッドで実行され、全件抽出中も
/// webview のメインスレッド（UI）を塞がない。`search::next_generation()` で世代を
/// 進めるため、本コマンドの呼び出し自体が進行中の旧 browse/search をキャンセルする。
/// `progress_channel` 指定時は抽出件数を ~100ms 間隔で emit する。
#[tauri::command(async)]
pub fn browse(
    app: AppHandle,
    query: String,
    sort: BrowseSort,
    options: Option<MatchOptions>,
    progress_channel: Option<String>,
    state: tauri::State<BrowseState>,
) -> Result<BrowseResult, String> {
    // 新しい検索エポックを開始（進行中の旧検索は次のチェックポイントで打ち切られる）
    let gen = search::next_generation();

    // 抽出件数を ~100ms 間隔に間引いて進捗チャンネルへ送る
    let start = Instant::now();
    let mut last_emit = start;
    let on_progress = |count: usize| {
        if let Some(ch) = progress_channel.as_deref() {
            let now = Instant::now();
            if now.duration_since(last_emit).as_millis() >= 100 {
                last_emit = now;
                let _ = app.emit(
                    ch,
                    BrowseProgress {
                        count,
                        elapsed_ms: now.duration_since(start).as_millis() as u64,
                    },
                );
            }
        }
    };

    let rows = match search::browse(
        &query,
        sort_const(&sort.col, sort.asc),
        BROWSE_MAX,
        options.unwrap_or_default(),
        gen,
        on_progress,
    ) {
        Ok(rows) => rows,
        // 後続検索に置き換えられた。世代 0 件として返し、フロントは採用しない
        Err(search::SearchError::Cancelled) => return Ok(BrowseResult { total: 0, generation: gen }),
        Err(e) => return Err(e.to_string()),
    };

    // 抽出・ソート完了後に後続検索へ追い越されていたら、スナップショットを確定させない
    // （古い世代で State を上書きして総件数と食い違わせないため）
    if search::current_generation() != gen {
        return Ok(BrowseResult { total: 0, generation: gen });
    }

    let len = rows.len();
    let mut guard = state.0.lock().map_err(|e| e.to_string())?;
    *guard = (gen, rows);

    Ok(BrowseResult { total: len, generation: gen })
}

/// 直近の `browse` 結果から `[offset, offset+limit)` のウィンドウを返す。
///
/// `offset` が全件数以上の場合は空 Vec を返す。
/// `generation` がフロントの表示中世代。常駐スナップショットの世代と一致しない場合
/// （新しい browse に差し替え済み）は空 Vec を返し、フロントは新世代で取得し直す。
/// 可視範囲分の派生（name/folder/ext/mtime整形）も `(async)` でメインスレッド外に逃がす。
#[tauri::command(async)]
pub fn fetch_window(
    offset: usize,
    limit: usize,
    generation: u64,
    state: tauri::State<BrowseState>,
) -> Result<Vec<WindowRow>, String> {
    let guard = state.0.lock().map_err(|e| e.to_string())?;
    let (snapshot_gen, rows) = &*guard;

    // 表示中世代と常駐スナップショットの世代が食い違う場合は空を返す
    if *snapshot_gen != generation || offset >= rows.len() {
        return Ok(Vec::new());
    }

    let end = (offset + limit).min(rows.len());
    let window = rows[offset..end]
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
