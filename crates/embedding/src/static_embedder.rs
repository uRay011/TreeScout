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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cosine_f32;

    // 変換済みモデル（~275MB、.gitignore対象）が `src-tauri/resources/embedding-model/`
    // に無い環境では配置手順未実施のためスキップする。
    #[test]
    fn loads_f16_model_and_embeds_cross_lingual() {
        let model_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../src-tauri/resources/embedding-model");
        if !model_dir.join("model.safetensors").exists() {
            eprintln!("skip: embedding model not found at {}", model_dir.display());
            return;
        }

        let embedder = StaticEmbedder::from_dir(&model_dir).expect("モデルロード失敗");
        assert_eq!(embedder.dim(), 256);

        let v_ja = embedder.embed_one("請求書");
        let v_en = embedder.embed_one("invoice");
        let v_unrelated = embedder.embed_one("猫");

        let sim_cross = cosine_f32(&v_ja, &v_en);
        let sim_unrelated = cosine_f32(&v_ja, &v_unrelated);
        assert!(
            sim_cross > sim_unrelated,
            "日英cosine({sim_cross}) は無関係語cosine({sim_unrelated}) を上回るべき"
        );
    }
}
