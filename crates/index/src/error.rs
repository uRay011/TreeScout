use thiserror::Error;

#[derive(Debug, Error)]
pub enum IndexError {
    #[error("SQLiteエラー: {0}")]
    Sqlite(#[from] rusqlite::Error),

    #[error("I/Oエラー: {0}")]
    Io(#[from] std::io::Error),

    #[error("埋め込み次元数不一致: 期待={expected} 実際={actual}")]
    DimMismatch { expected: usize, actual: usize },
}
