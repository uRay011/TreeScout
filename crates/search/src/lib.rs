#[cfg(windows)]
mod everything;

#[cfg(windows)]
pub use everything::{
    browse, current_generation, format_mtime, next_generation, search, BrowseRow, MatchOptions, SearchError, SearchResult,
    EVERYTHING_SORT_DATE_MODIFIED_ASCENDING, EVERYTHING_SORT_DATE_MODIFIED_DESCENDING,
    EVERYTHING_SORT_NAME_ASCENDING, EVERYTHING_SORT_NAME_DESCENDING,
    EVERYTHING_SORT_PATH_ASCENDING, EVERYTHING_SORT_PATH_DESCENDING,
    EVERYTHING_SORT_SIZE_ASCENDING, EVERYTHING_SORT_SIZE_DESCENDING,
};

// 非 Windows（開発・テスト用）スタブ
#[cfg(not(windows))]
mod stub {
    use serde::Serialize;

    #[derive(Debug, thiserror::Error)]
    pub enum SearchError {
        #[error("Windows 環境が必要です")]
        Unsupported,
        #[error("検索がキャンセルされました")]
        Cancelled,
    }

    #[derive(Debug, Serialize)]
    pub struct SearchResult {
        pub name: String,
        pub path: String,
        pub folder: String,
        pub is_dir: bool,
        pub ext: String,
    }

    /// `browse` 用の1行分データ（Windows実装と同形）。
    #[derive(Debug, Clone)]
    pub struct BrowseRow {
        pub path: String,
        pub is_dir: bool,
        pub size: u64,
        pub mtime: i64,
    }

    /// Everything のマッチングオプション（Windows実装と同形）。
    #[derive(Debug, Clone, Copy, Default, serde::Deserialize)]
    #[serde(default, rename_all = "camelCase")]
    pub struct MatchOptions {
        pub case_sensitive: bool,
        pub whole_word: bool,
        pub match_path: bool,
    }

    pub fn next_generation() -> u64 {
        0
    }

    pub fn current_generation() -> u64 {
        0
    }

    pub fn search(_query: &str, _max: u32, _opts: MatchOptions, _gen: u64) -> Result<Vec<SearchResult>, SearchError> {
        Ok(vec![])
    }

    pub fn browse<F: FnMut(usize)>(
        _query: &str,
        _sort: u32,
        _max: u32,
        _opts: MatchOptions,
        _gen: u64,
        _on_progress: F,
    ) -> Result<Vec<BrowseRow>, SearchError> {
        Ok(vec![])
    }

    pub fn format_mtime(_ft: i64) -> String {
        String::new()
    }

    // Everything ソート定数（Windows実装と同値。非Windownsではダミー値として保持）
    pub const EVERYTHING_SORT_NAME_ASCENDING: u32 = 1;
    pub const EVERYTHING_SORT_NAME_DESCENDING: u32 = 2;
    pub const EVERYTHING_SORT_PATH_ASCENDING: u32 = 3;
    pub const EVERYTHING_SORT_PATH_DESCENDING: u32 = 4;
    pub const EVERYTHING_SORT_SIZE_ASCENDING: u32 = 5;
    pub const EVERYTHING_SORT_SIZE_DESCENDING: u32 = 6;
    pub const EVERYTHING_SORT_DATE_MODIFIED_ASCENDING: u32 = 13;
    pub const EVERYTHING_SORT_DATE_MODIFIED_DESCENDING: u32 = 14;
}

#[cfg(not(windows))]
pub use stub::{
    browse, current_generation, format_mtime, next_generation, search, BrowseRow, MatchOptions, SearchError, SearchResult,
    EVERYTHING_SORT_DATE_MODIFIED_ASCENDING, EVERYTHING_SORT_DATE_MODIFIED_DESCENDING,
    EVERYTHING_SORT_NAME_ASCENDING, EVERYTHING_SORT_NAME_DESCENDING,
    EVERYTHING_SORT_PATH_ASCENDING, EVERYTHING_SORT_PATH_DESCENDING,
    EVERYTHING_SORT_SIZE_ASCENDING, EVERYTHING_SORT_SIZE_DESCENDING,
};
