//! フォルダ embedding 事前インデックスの Tauri コマンド統合
//!
//! `index::index_folders` / `index::rebuild_folder_matrix` をフロントから
//! 明示的に呼び出すための薄いアダプタ層。
//!
//! 埋め込みモデル（model2vec StaticEmbedder）は未バンドルのため、
//! 配線確認用にダミー Embedder を暫定使用する。モデル統合時は
//! `DummyEmbedder` を `embedding::StaticEmbedder` に差し替える。

use std::path::PathBuf;
use std::sync::Mutex;

use embedding::Embedder;
use index::{index_folders, rebuild_folder_matrix, FolderEmbeddingMatrix, IndexStore};
use serde::Serialize;
use tauri::{AppHandle, Manager};

/// folder_indexer 配線確認用のダミー埋め込み器
///
/// フォルダ名の文字数からハッシュ的に決定的なベクトルを生成するだけで、
/// 意味的な類似度は持たない。`StaticEmbedder` 統合までの暫定実装。
struct DummyEmbedder {
    dim: usize,
}

impl Embedder for DummyEmbedder {
    fn embed(&self, texts: &[&str]) -> Vec<Vec<f32>> {
        texts
            .iter()
            .map(|t| {
                let v = (t.len() as f32 % 10.0) / 10.0;
                vec![v; self.dim]
            })
            .collect()
    }

    fn dim(&self) -> usize {
        self.dim
    }
}

const FOLDER_EMBEDDING_DIM: usize = 64;
const EMBED_BATCH_SIZE: usize = 64;

/// フォルダ事前インデックスの永続化先（SQLite + mmap行列）を保持する状態。
pub struct FolderIndexState {
    store: Mutex<IndexStore>,
    matrix_path: PathBuf,
}

impl FolderIndexState {
    /// アプリデータディレクトリ配下に SQLite を開いて状態を初期化する。
    pub fn init(app: &AppHandle) -> Result<Self, String> {
        let data_dir = app
            .path()
            .app_data_dir()
            .map_err(|e| e.to_string())?;
        std::fs::create_dir_all(&data_dir).map_err(|e| e.to_string())?;

        let store = IndexStore::open(&data_dir.join("index.sqlite")).map_err(|e| e.to_string())?;
        let matrix_path = data_dir.join("folders.bin");

        Ok(Self {
            store: Mutex::new(store),
            matrix_path,
        })
    }
}

/// フォルダ事前インデックス実行結果（フロント返却用）
#[derive(Debug, Serialize, Clone)]
pub struct FolderIndexResult {
    pub updated: usize,
    pub skipped: usize,
    pub scanned: usize,
    pub removed: usize,
    pub matrix_len: usize,
}

/// `root` 配下のフォルダ embedding を差分更新し、mmap行列を再構築する。
#[tauri::command]
pub fn index_folders_command(
    state: tauri::State<FolderIndexState>,
    root: String,
) -> Result<FolderIndexResult, String> {
    let embedder = DummyEmbedder { dim: FOLDER_EMBEDDING_DIM };
    let store = state.store.lock().map_err(|e| e.to_string())?;

    let stats = index_folders(&store, std::path::Path::new(&root), &embedder, EMBED_BATCH_SIZE)
        .map_err(|e| e.to_string())?;

    let matrix: FolderEmbeddingMatrix =
        rebuild_folder_matrix(&store, &state.matrix_path, FOLDER_EMBEDDING_DIM)
            .map_err(|e| e.to_string())?;

    Ok(FolderIndexResult {
        updated: stats.updated,
        skipped: stats.skipped,
        scanned: stats.scanned,
        removed: stats.removed,
        matrix_len: matrix.len(),
    })
}
