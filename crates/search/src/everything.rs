use serde::Serialize;
use std::ffi::OsString;
use std::os::windows::ffi::OsStringExt;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

use windows::core::PCWSTR;
use windows::Win32::Foundation::{FILETIME, SYSTEMTIME};
use windows::Win32::Storage::FileSystem::FileTimeToLocalFileTime;
use windows::Win32::System::LibraryLoader::{GetProcAddress, LoadLibraryW};
use windows::Win32::System::Time::FileTimeToSystemTime;

/// Everything DLL はプロセス内でグローバルなクエリ状態を1つだけ持つ。
/// `search()` と `browse()` が同時に呼ばれると互いの SetSearchW/QueryW 呼び出しが
/// 競合するため、DLL操作を行う区間全体をこのロックで直列化する。
static QUERY_LOCK: Mutex<()> = Mutex::new(());

#[derive(Debug, thiserror::Error)]
pub enum SearchError {
    #[error("Everything64.dll のロードに失敗: {0}")]
    DllLoad(String),
    #[error("Everything が起動していません")]
    NotRunning,
    #[error("クエリ失敗: code={0}")]
    QueryFailed(u32),
}

/// Everything のマッチングオプション（検索メニューのトグルに対応）
#[derive(Debug, Clone, Copy, Default, serde::Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct MatchOptions {
    /// 大文字小文字の区別（Ctrl+I）
    pub case_sensitive: bool,
    /// 単語に完全一致（Ctrl+B）
    pub whole_word: bool,
    /// ファイル名だけでなくフォルダ名にもマッチ（Ctrl+U）
    pub match_path: bool,
}

#[derive(Debug, Serialize)]
pub struct SearchResult {
    pub name: String,
    pub path: String,
    pub folder: String,
    pub is_dir: bool,
    pub ext: String,
}

// Everything SDK エラーコード
const EVERYTHING_OK: u32 = 0;
const EVERYTHING_ERROR_IPC: u32 = 2;

// リクエストフラグ（必要列のみ）
const EVERYTHING_REQUEST_FILE_NAME: u32 = 0x00000001;
const EVERYTHING_REQUEST_PATH: u32 = 0x00000002;
const EVERYTHING_REQUEST_SIZE: u32 = 0x00000010;
const EVERYTHING_REQUEST_DATE_MODIFIED: u32 = 0x00000040;

// Everything ソート定数
pub const EVERYTHING_SORT_NAME_ASCENDING: u32 = 1;
pub const EVERYTHING_SORT_NAME_DESCENDING: u32 = 2;
pub const EVERYTHING_SORT_PATH_ASCENDING: u32 = 3;
pub const EVERYTHING_SORT_PATH_DESCENDING: u32 = 4;
pub const EVERYTHING_SORT_SIZE_ASCENDING: u32 = 5;
pub const EVERYTHING_SORT_SIZE_DESCENDING: u32 = 6;
pub const EVERYTHING_SORT_DATE_MODIFIED_ASCENDING: u32 = 13;
pub const EVERYTHING_SORT_DATE_MODIFIED_DESCENDING: u32 = 14;

type FnSetSearchW = unsafe extern "C" fn(search: PCWSTR);
type FnSetRequestFlags = unsafe extern "C" fn(flags: u32);
type FnSetMax = unsafe extern "C" fn(max: u32);
type FnSetMatchCase = unsafe extern "C" fn(enable: i32);
type FnSetMatchWholeWord = unsafe extern "C" fn(enable: i32);
type FnSetMatchPath = unsafe extern "C" fn(enable: i32);
type FnQueryW = unsafe extern "C" fn(wait: i32) -> i32;
type FnGetLastError = unsafe extern "C" fn() -> u32;
type FnGetNumResults = unsafe extern "C" fn() -> u32;
type FnGetResultFileNameW = unsafe extern "C" fn(index: u32) -> PCWSTR;
type FnGetResultPathW = unsafe extern "C" fn(index: u32) -> PCWSTR;
type FnIsFileResult = unsafe extern "C" fn(index: u32) -> i32;
type FnGetResultSize = unsafe extern "C" fn(index: u32, lp_size: *mut i64) -> i32;
type FnGetResultDateModified = unsafe extern "C" fn(index: u32, lp_ft: *mut i64) -> i32;

struct EverythingApi {
    set_search_w: FnSetSearchW,
    set_request_flags: FnSetRequestFlags,
    set_max: FnSetMax,
    set_match_case: FnSetMatchCase,
    set_match_whole_word: FnSetMatchWholeWord,
    set_match_path: FnSetMatchPath,
    query_w: FnQueryW,
    get_last_error: FnGetLastError,
    get_num_results: FnGetNumResults,
    get_result_file_name_w: FnGetResultFileNameW,
    get_result_path_w: FnGetResultPathW,
    is_file_result: FnIsFileResult,
    get_result_size: FnGetResultSize,
    get_result_date_modified: FnGetResultDateModified,
}

// DLLは一度だけロードしてプロセス全体で共有する
static API: OnceLock<Result<EverythingApi, SearchError>> = OnceLock::new();

unsafe fn load_api() -> Result<EverythingApi, SearchError> {
    // exeと同じディレクトリのDLLを優先してロード
    let dll_name: Vec<u16> = "Everything64.dll\0".encode_utf16().collect();
    let hmod = LoadLibraryW(PCWSTR(dll_name.as_ptr()))
        .map_err(|e| SearchError::DllLoad(e.to_string()))?;

    macro_rules! proc {
        ($name:literal, $ty:ty) => {{
            let sym = concat!($name, "\0");
            let addr = GetProcAddress(hmod, windows::core::PCSTR(sym.as_ptr()))
                .ok_or_else(|| SearchError::DllLoad(format!("{} が見つかりません", $name)))?;
            std::mem::transmute::<_, $ty>(addr)
        }};
    }

    Ok(EverythingApi {
        set_search_w: proc!("Everything_SetSearchW", FnSetSearchW),
        set_request_flags: proc!("Everything_SetRequestFlags", FnSetRequestFlags),
        set_max: proc!("Everything_SetMax", FnSetMax),
        set_match_case: proc!("Everything_SetMatchCase", FnSetMatchCase),
        set_match_whole_word: proc!("Everything_SetMatchWholeWord", FnSetMatchWholeWord),
        set_match_path: proc!("Everything_SetMatchPath", FnSetMatchPath),
        query_w: proc!("Everything_QueryW", FnQueryW),
        get_last_error: proc!("Everything_GetLastError", FnGetLastError),
        get_num_results: proc!("Everything_GetNumResults", FnGetNumResults),
        get_result_file_name_w: proc!("Everything_GetResultFileNameW", FnGetResultFileNameW),
        get_result_path_w: proc!("Everything_GetResultPathW", FnGetResultPathW),
        is_file_result: proc!("Everything_IsFileResult", FnIsFileResult),
        get_result_size: proc!("Everything_GetResultSize", FnGetResultSize),
        get_result_date_modified: proc!(
            "Everything_GetResultDateModified",
            FnGetResultDateModified
        ),
    })
}

/// クエリをUTF-16(NUL終端)へ変換する。
///
/// 空文字（フィルタなし）を `Everything_SetSearchW` にそのまま渡すと
/// IPC経由では0件になるため、Everything本体のGUIと同じ「全件表示」に
/// 揃えるべく `*`（全マッチワイルドカード）に置き換える。
fn query_to_wide(query: &str) -> Vec<u16> {
    let q = if query.trim().is_empty() { "*" } else { query };
    q.encode_utf16().chain(std::iter::once(0)).collect()
}

unsafe fn pcwstr_to_string(p: PCWSTR) -> String {
    if p.is_null() {
        return String::new();
    }
    let mut len = 0usize;
    while *p.0.add(len) != 0 {
        len += 1;
    }
    let slice = std::slice::from_raw_parts(p.0, len);
    OsString::from_wide(slice).to_string_lossy().into_owned()
}

/// Everything でファイル検索する。max 件まで返す。
pub fn search(query: &str, max: u32, opts: MatchOptions) -> Result<Vec<SearchResult>, SearchError> {
    let api_result = API.get_or_init(|| unsafe { load_api() });
    let api = match api_result {
        Ok(a) => a,
        Err(e) => return Err(SearchError::DllLoad(e.to_string())),
    };

    let query_w = query_to_wide(query);

    let _guard = QUERY_LOCK.lock().unwrap();
    unsafe {
        (api.set_request_flags)(EVERYTHING_REQUEST_FILE_NAME | EVERYTHING_REQUEST_PATH);
        (api.set_max)(max);
        (api.set_match_case)(opts.case_sensitive as i32);
        (api.set_match_whole_word)(opts.whole_word as i32);
        (api.set_match_path)(opts.match_path as i32);
        (api.set_search_w)(PCWSTR(query_w.as_ptr()));

        if (api.query_w)(1) == 0 {
            let code = (api.get_last_error)();
            return Err(if code == EVERYTHING_ERROR_IPC {
                SearchError::NotRunning
            } else {
                SearchError::QueryFailed(code)
            });
        }

        let count = (api.get_num_results)();
        let mut results = Vec::with_capacity(count as usize);

        for i in 0..count {
            let name = pcwstr_to_string((api.get_result_file_name_w)(i));
            let dir = pcwstr_to_string((api.get_result_path_w)(i));
            let is_file = (api.is_file_result)(i) != 0;

            let full_path = PathBuf::from(&dir).join(&name);
            let ext = if is_file {
                PathBuf::from(&name)
                    .extension()
                    .map(|e| e.to_string_lossy().into_owned())
                    .unwrap_or_default()
            } else {
                String::new()
            };
            results.push(SearchResult {
                name,
                path: full_path.to_string_lossy().into_owned(),
                folder: dir,
                is_dir: !is_file,
                ext,
            });
        }

        Ok(results)
    }
}

/// 「バックエンド窓取得」用の1行分データ。
///
/// 軽量化のため name/folder/ext はここでは生成せず、フルパスのみ保持する
/// （フロント可視ウィンドウ分のみ `fetch_window` で派生させる）。
#[derive(Debug, Clone)]
pub struct BrowseRow {
    pub path: String,
    pub is_dir: bool,
    pub size: u64,
    pub mtime: i64,
}

/// パスからファイル名部分のみを取り出し小文字化する（名前ソート用キー）。
fn path_basename_lower(path: &str) -> String {
    path.rsplit(|c| c == '\\' || c == '/')
        .next()
        .unwrap_or(path)
        .to_lowercase()
}

/// 取得済みの `BrowseRow` を `EVERYTHING_SORT_*` 定数に従ってRust側でソートする。
///
/// `Everything_SetSort` はインストール環境によってはIPC経由で結果が0件になる
/// （SDKバージョン不整合等）ため使用せず、クライアント側で安定ソートする。
fn sort_browse_rows(rows: &mut [BrowseRow], sort: u32) {
    let desc = matches!(
        sort,
        EVERYTHING_SORT_NAME_DESCENDING
            | EVERYTHING_SORT_PATH_DESCENDING
            | EVERYTHING_SORT_SIZE_DESCENDING
            | EVERYTHING_SORT_DATE_MODIFIED_DESCENDING
    );

    match sort {
        EVERYTHING_SORT_PATH_ASCENDING | EVERYTHING_SORT_PATH_DESCENDING => {
            rows.sort_by_cached_key(|r| r.path.to_lowercase());
        }
        EVERYTHING_SORT_SIZE_ASCENDING | EVERYTHING_SORT_SIZE_DESCENDING => {
            rows.sort_by_key(|r| r.size);
        }
        EVERYTHING_SORT_DATE_MODIFIED_ASCENDING | EVERYTHING_SORT_DATE_MODIFIED_DESCENDING => {
            rows.sort_by_key(|r| r.mtime);
        }
        _ => {
            rows.sort_by_cached_key(|r| path_basename_lower(&r.path));
        }
    }
    if desc {
        rows.reverse();
    }
}

/// Everything で全件取得し、Rust側でソートする（owner-data ListView モデル）。
///
/// `sort` には `EVERYTHING_SORT_*` 定数を指定する。`max` 件まで取得し、
/// 結果は `BrowseRow` としてフルパス・サイズ・更新日時のみ保持する。
pub fn browse(query: &str, sort: u32, max: u32, opts: MatchOptions) -> Result<Vec<BrowseRow>, SearchError> {
    let api_result = API.get_or_init(|| unsafe { load_api() });
    let api = match api_result {
        Ok(a) => a,
        Err(e) => return Err(SearchError::DllLoad(e.to_string())),
    };

    let query_w = query_to_wide(query);

    let _guard = QUERY_LOCK.lock().unwrap();
    unsafe {
        (api.set_request_flags)(
            EVERYTHING_REQUEST_FILE_NAME
                | EVERYTHING_REQUEST_PATH
                | EVERYTHING_REQUEST_SIZE
                | EVERYTHING_REQUEST_DATE_MODIFIED,
        );
        (api.set_max)(max);
        (api.set_match_case)(opts.case_sensitive as i32);
        (api.set_match_whole_word)(opts.whole_word as i32);
        (api.set_match_path)(opts.match_path as i32);
        (api.set_search_w)(PCWSTR(query_w.as_ptr()));

        if (api.query_w)(1) == 0 {
            let code = (api.get_last_error)();
            return Err(if code == EVERYTHING_ERROR_IPC {
                SearchError::NotRunning
            } else {
                SearchError::QueryFailed(code)
            });
        }

        let count = (api.get_num_results)();
        let mut results = Vec::with_capacity(count as usize);

        for i in 0..count {
            let name = pcwstr_to_string((api.get_result_file_name_w)(i));
            let dir = pcwstr_to_string((api.get_result_path_w)(i));
            let is_file = (api.is_file_result)(i) != 0;

            let mut size: i64 = 0;
            if (api.get_result_size)(i, &mut size as *mut i64) == 0 {
                size = 0;
            }

            let mut mtime: i64 = 0;
            if (api.get_result_date_modified)(i, &mut mtime as *mut i64) == 0 {
                mtime = 0;
            }

            let full_path = PathBuf::from(&dir).join(&name);
            results.push(BrowseRow {
                path: full_path.to_string_lossy().into_owned(),
                is_dir: !is_file,
                size: size.max(0) as u64,
                mtime,
            });
        }

        sort_browse_rows(&mut results, sort);
        Ok(results)
    }
}

/// FILETIME(i64) をローカル時刻 "YYYY-MM-DD HH:MM" に変換する。
///
/// `ft == 0`（取得失敗）または変換失敗時は空文字を返す。
pub fn format_mtime(ft: i64) -> String {
    if ft == 0 {
        return String::new();
    }

    let utc_ft = FILETIME {
        dwLowDateTime: (ft as u64 & 0xFFFF_FFFF) as u32,
        dwHighDateTime: ((ft as u64) >> 32) as u32,
    };

    unsafe {
        let mut local_ft = FILETIME::default();
        if FileTimeToLocalFileTime(&utc_ft, &mut local_ft).is_err() {
            return String::new();
        }

        let mut st = SYSTEMTIME::default();
        if FileTimeToSystemTime(&local_ft, &mut st).is_err() {
            return String::new();
        }

        format!(
            "{:04}-{:02}-{:02} {:02}:{:02}",
            st.wYear, st.wMonth, st.wDay, st.wHour, st.wMinute
        )
    }
}
