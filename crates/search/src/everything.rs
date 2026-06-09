use serde::Serialize;
use std::ffi::OsString;
use std::os::windows::ffi::OsStringExt;
use std::path::PathBuf;
use std::sync::OnceLock;

use windows::core::PCWSTR;
use windows::Win32::Foundation::HWND;
use windows::Win32::System::LibraryLoader::{GetProcAddress, LoadLibraryW};

#[derive(Debug, thiserror::Error)]
pub enum SearchError {
    #[error("Everything64.dll のロードに失敗: {0}")]
    DllLoad(String),
    #[error("Everything が起動していません")]
    NotRunning,
    #[error("クエリ失敗: code={0}")]
    QueryFailed(u32),
}

#[derive(Debug, Serialize)]
pub struct SearchResult {
    pub path: String,
    pub is_dir: bool,
}

// Everything SDK エラーコード
const EVERYTHING_OK: u32 = 0;
const EVERYTHING_ERROR_IPC: u32 = 2;

// リクエストフラグ（必要列のみ）
const EVERYTHING_REQUEST_FILE_NAME: u32 = 0x00000001;
const EVERYTHING_REQUEST_PATH: u32 = 0x00000002;

type FnSetSearchW = unsafe extern "C" fn(search: PCWSTR);
type FnSetRequestFlags = unsafe extern "C" fn(flags: u32);
type FnSetMax = unsafe extern "C" fn(max: u32);
type FnQueryW = unsafe extern "C" fn(wait: i32) -> i32;
type FnGetLastError = unsafe extern "C" fn() -> u32;
type FnGetNumResults = unsafe extern "C" fn() -> u32;
type FnGetResultFileNameW = unsafe extern "C" fn(index: u32) -> PCWSTR;
type FnGetResultPathW = unsafe extern "C" fn(index: u32) -> PCWSTR;
type FnIsFileResult = unsafe extern "C" fn(index: u32) -> i32;

struct EverythingApi {
    set_search_w: FnSetSearchW,
    set_request_flags: FnSetRequestFlags,
    set_max: FnSetMax,
    query_w: FnQueryW,
    get_last_error: FnGetLastError,
    get_num_results: FnGetNumResults,
    get_result_file_name_w: FnGetResultFileNameW,
    get_result_path_w: FnGetResultPathW,
    is_file_result: FnIsFileResult,
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
        query_w: proc!("Everything_QueryW", FnQueryW),
        get_last_error: proc!("Everything_GetLastError", FnGetLastError),
        get_num_results: proc!("Everything_GetNumResults", FnGetNumResults),
        get_result_file_name_w: proc!("Everything_GetResultFileNameW", FnGetResultFileNameW),
        get_result_path_w: proc!("Everything_GetResultPathW", FnGetResultPathW),
        is_file_result: proc!("Everything_IsFileResult", FnIsFileResult),
    })
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
pub fn search(query: &str, max: u32) -> Result<Vec<SearchResult>, SearchError> {
    let api_result = API.get_or_init(|| unsafe { load_api() });
    let api = match api_result {
        Ok(a) => a,
        Err(e) => return Err(SearchError::DllLoad(e.to_string())),
    };

    let query_w: Vec<u16> = query.encode_utf16().chain(std::iter::once(0)).collect();

    unsafe {
        (api.set_request_flags)(EVERYTHING_REQUEST_FILE_NAME | EVERYTHING_REQUEST_PATH);
        (api.set_max)(max);
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

            let path = PathBuf::from(&dir).join(&name);
            results.push(SearchResult {
                path: path.to_string_lossy().into_owned(),
                is_dir: !is_file,
            });
        }

        Ok(results)
    }
}
