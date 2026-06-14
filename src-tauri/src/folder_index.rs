//! フォルダ embedding 事前インデックスの Tauri コマンド統合
//!
//! `index::index_folders` / `index::rebuild_folder_matrix` をフロントから
//! 明示的に呼び出すための薄いアダプタ層。
//!
//! 埋め込みモデルは `tauri.conf.json` の `bundle.resources` で
//! `resources/embedding-model/`（model2vec, tokenizer.json/model.safetensors/config.json）
//! としてバンドルされ、`StaticEmbedder::from_dir` でロードする。

use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock};

use embedding::{Embedder, StaticEmbedder};
use index::{index_folders, rebuild_folder_matrix, FolderEmbeddingMatrix, IndexStore};
use serde::Serialize;
use tauri::path::BaseDirectory;
use tauri::{AppHandle, Manager};

const EMBED_BATCH_SIZE: usize = 64;

/// フォルダ事前インデックスの永続化先（SQLite + mmap行列）と埋め込みモデルを保持する状態。
///
/// 埋め込みモデル（f16, ~256MB）はロードにdebugビルドで約20秒かかるため、
/// `init` ではバックグラウンドスレッドでロードを開始するだけにし、ウィンドウ表示を
/// ブロックしない。ロード完了までは `embedder()` が `None` を返すので、
/// 呼び出し側はAIサジェスト探索を空扱いにする等のフォールバックを行う。
pub struct FolderIndexState {
    store: Mutex<IndexStore>,
    matrix_path: PathBuf,
    embedder: Arc<OnceLock<StaticEmbedder>>,
}

impl FolderIndexState {
    /// アプリデータディレクトリ配下に SQLite を開き、バンドル済みモデルのロードを
    /// バックグラウンドスレッドで開始して状態を初期化する。
    pub fn init(app: &AppHandle) -> Result<Self, String> {
        let data_dir = app
            .path()
            .app_data_dir()
            .map_err(|e| e.to_string())?;
        std::fs::create_dir_all(&data_dir).map_err(|e| e.to_string())?;

        let store = IndexStore::open(&data_dir.join("index.sqlite")).map_err(|e| e.to_string())?;
        let matrix_path = data_dir.join("folders.bin");

        let model_dir = app
            .path()
            .resolve("resources/embedding-model", BaseDirectory::Resource)
            .map_err(|e| e.to_string())?;

        let embedder: Arc<OnceLock<StaticEmbedder>> = Arc::new(OnceLock::new());
        {
            let embedder = embedder.clone();
            std::thread::spawn(move || match StaticEmbedder::from_dir(&model_dir) {
                Ok(e) => {
                    let _ = embedder.set(e);
                }
                Err(err) => {
                    eprintln!("埋め込みモデルのロードに失敗しました: {err}");
                }
            });
        }

        Ok(Self {
            store: Mutex::new(store),
            matrix_path,
            embedder,
        })
    }

    /// クエリ・フォルダ名の埋め込みに使うモデル。バックグラウンドロードが未完了なら `None`。
    pub fn embedder(&self) -> Option<&StaticEmbedder> {
        self.embedder.get()
    }

    /// 事前構築済みのフォルダ embedding 行列を mmap で開く。
    /// 未構築（初回起動など）の場合は `Err` を返すので、呼び出し側で空扱いにフォールバックする。
    pub fn load_matrix(&self) -> Result<FolderEmbeddingMatrix, String> {
        FolderEmbeddingMatrix::open(&self.matrix_path).map_err(|e| e.to_string())
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
    let embedder = state
        .embedder()
        .ok_or_else(|| "埋め込みモデルをロード中です。しばらく待ってから再実行してください".to_string())?;
    let store = state.store.lock().map_err(|e| e.to_string())?;

    let stats = index_folders(&store, std::path::Path::new(&root), embedder, EMBED_BATCH_SIZE)
        .map_err(|e| e.to_string())?;

    let matrix: FolderEmbeddingMatrix =
        rebuild_folder_matrix(&store, &state.matrix_path, embedder.dim())
            .map_err(|e| e.to_string())?;

    Ok(FolderIndexResult {
        updated: stats.updated,
        skipped: stats.skipped,
        scanned: stats.scanned,
        removed: stats.removed,
        matrix_len: matrix.len(),
    })
}
