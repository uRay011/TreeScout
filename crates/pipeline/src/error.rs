use thiserror::Error;

#[derive(Debug, Error)]
pub enum PipelineError {
    #[cfg(windows)]
    #[error("Everything 検索エラー: {0}")]
    Everything(#[from] search::SearchError),

    #[error("VirtualTree の構築に失敗: {0}")]
    TreeBuild(String),
}
