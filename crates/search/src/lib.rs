#[cfg(windows)]
mod everything;

#[cfg(windows)]
pub use everything::{search, SearchError, SearchResult};

// 非 Windows（開発・テスト用）スタブ
#[cfg(not(windows))]
mod stub {
    use serde::Serialize;

    #[derive(Debug, thiserror::Error)]
    pub enum SearchError {
        #[error("Windows 環境が必要です")]
        Unsupported,
    }

    #[derive(Debug, Serialize)]
    pub struct SearchResult {
        pub name: String,
        pub path: String,
        pub folder: String,
        pub is_dir: bool,
        pub ext: String,
    }

    pub fn search(_query: &str, _max: u32) -> Result<Vec<SearchResult>, SearchError> {
        Ok(vec![])
    }
}

#[cfg(not(windows))]
pub use stub::{search, SearchError, SearchResult};
