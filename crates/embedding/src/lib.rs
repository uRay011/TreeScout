mod error;
mod quantize;
mod similarity;

#[cfg(feature = "static")]
mod static_embedder;

pub use error::EmbedError;
pub use quantize::{dequantize_i8, quantize_f32_to_i8};
pub use similarity::{cosine_f32, cosine_i8};

#[cfg(feature = "static")]
pub use static_embedder::StaticEmbedder;

/// 埋め込みベクトル生成の共通インターフェース
pub trait Embedder: Send + Sync {
    /// テキストのバッチを埋め込みベクトルに変換する（正規化済み float32）
    fn embed(&self, texts: &[&str]) -> Vec<Vec<f32>>;

    /// 1件埋め込み（便利メソッド）
    fn embed_one(&self, text: &str) -> Vec<f32> {
        self.embed(&[text]).into_iter().next().unwrap_or_default()
    }

    /// 埋め込み次元数
    fn dim(&self) -> usize;
}
