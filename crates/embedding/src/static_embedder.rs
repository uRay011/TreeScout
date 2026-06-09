use std::path::Path;

use anyhow::Context as _;
use model2vec_rs::model::StaticModel;

use crate::{EmbedError, Embedder};

/// model2vec-rs による静的埋め込み生成器
///
/// transformer を通さないルックアップ＋mean pool のため推論がサブms/件。
/// モデルは起動時に一度ロードしてキャッシュする。
pub struct StaticEmbedder {
    model: StaticModel,
    dim: usize,
}

impl StaticEmbedder {
    /// ローカルディレクトリからモデルをロードする
    ///
    /// `model_dir` には `tokenizer.json`, `model.safetensors`, `config.json` が必要。
    pub fn from_dir(model_dir: &Path) -> Result<Self, EmbedError> {
        let path_str = model_dir
            .to_str()
            .ok_or_else(|| EmbedError::ModelNotFound(model_dir.display().to_string()))?;

        let model = StaticModel::from_pretrained(
            path_str,
            None,  // HF token 不要（local-only）
            Some(true),  // L2 正規化を強制（cosine 類似度を内積で計算可能にする）
            None,
        )
        .with_context(|| format!("モデルロード失敗: {}", model_dir.display()))?;

        // dim を1件ダミー推論で取得
        let probe = model.encode(&["_".to_string()]);
        let dim = probe.first().map(|v| v.len()).unwrap_or(0);

        Ok(Self { model, dim })
    }

    /// メモリ上のバイト列からロードする（バンドル埋め込み用）
    pub fn from_bytes(
        tokenizer_json: &[u8],
        model_safetensors: &[u8],
        config_json: &[u8],
    ) -> Result<Self, EmbedError> {
        let model = StaticModel::from_bytes(tokenizer_json, model_safetensors, config_json, Some(true))
            .with_context(|| "バイト列からのモデルロード失敗")?;

        let probe = model.encode(&["_".to_string()]);
        let dim = probe.first().map(|v| v.len()).unwrap_or(0);

        Ok(Self { model, dim })
    }
}

impl Embedder for StaticEmbedder {
    fn embed(&self, texts: &[&str]) -> Vec<Vec<f32>> {
        let owned: Vec<String> = texts.iter().map(|s| s.to_string()).collect();
        self.model.encode(&owned)
    }

    fn dim(&self) -> usize {
        self.dim
    }
}
