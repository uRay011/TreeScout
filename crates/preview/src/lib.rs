//! ファイルプレビュー機能（Phase 4）
//!
//! 検索パイプラインとは独立した経路で動作する。`get_preview` は
//! ユーザーがアイテムを選択した際にのみ呼ばれ、検索の <200ms 目標には影響しない。

mod kind;
mod text;

pub use kind::PreviewKind;

use serde::Serialize;
use std::path::Path;
use thiserror::Error;

/// テキスト/Markdown プレビューで読み込む先頭バイト数の上限。
/// 大容量ファイルの全read を避けるためのキャップ。
const MAX_TEXT_BYTES: usize = 64 * 1024;

#[derive(Debug, Error)]
pub enum PreviewError {
    #[error("file not found: {0}")]
    NotFound(String),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PreviewResult {
    /// プレーンテキスト/コード（先頭 MAX_TEXT_BYTES、UTF-8 として解釈）
    Text { content: String, truncated: bool },
    /// Markdown（レンダリングはフロント側で行う）
    Markdown { content: String, truncated: bool },
    /// 画像。バイト列は転送せず、フロント側で `convertFileSrc` を使い直接参照する。
    Image,
    /// プレビュー非対応の種別
    Unsupported,
}

/// パスからプレビューを生成する。
///
/// - テキスト/Markdown: 先頭 [`MAX_TEXT_BYTES`] バイトを非同期読込
/// - 画像: 種別判定のみ（本体はフロントが `convertFileSrc` で取得）
/// - その他: `Unsupported`
pub async fn get_preview(path: &Path) -> Result<PreviewResult, PreviewError> {
    if !path.exists() {
        return Err(PreviewError::NotFound(path.to_string_lossy().into_owned()));
    }

    match kind::classify(path) {
        PreviewKind::Markdown => {
            let (content, truncated) = text::read_head(path, MAX_TEXT_BYTES).await?;
            Ok(PreviewResult::Markdown { content, truncated })
        }
        PreviewKind::Text => {
            let (content, truncated) = text::read_head(path, MAX_TEXT_BYTES).await?;
            Ok(PreviewResult::Text { content, truncated })
        }
        PreviewKind::Image => Ok(PreviewResult::Image),
        PreviewKind::Unsupported => Ok(PreviewResult::Unsupported),
    }
}
