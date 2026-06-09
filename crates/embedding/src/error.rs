use thiserror::Error;

#[derive(Debug, Error)]
pub enum EmbedError {
    #[error("モデルロード失敗: {0}")]
    ModelLoad(#[from] anyhow::Error),

    #[error("モデルファイルが見つかりません: {0}")]
    ModelNotFound(String),

    #[error("空のテキストリストは埋め込み不可")]
    EmptyInput,
}
