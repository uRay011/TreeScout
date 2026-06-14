//! folders.bin 接続の動作確認用デモ（WSLでWindows実機なしに確認できる）
//!
//! 実際のフォルダ名候補をいくつか埋め込み、クエリに対する Top-N 近傍を表示する。
//! 実行には変換済み embedding モデル（`src-tauri/resources/embedding-model/`）が必要。
//!
//! 実行例:
//! ```text
//! cargo run -p index --example nearest_demo -- "請求書"
//! ```

use std::path::Path;

use embedding::{quantize_f32_to_i8, Embedder, StaticEmbedder};
use index::FolderEmbeddingMatrix;
use tempfile::tempdir;

fn main() {
    let query = std::env::args().nth(1).unwrap_or_else(|| "請求書".to_string());

    let model_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../src-tauri/resources/embedding-model");
    if !model_dir.join("model.safetensors").exists() {
        eprintln!("embedding model not found at {}", model_dir.display());
        eprintln!("先に src-tauri/resources/embedding-model/ にモデルを配置してください。");
        std::process::exit(1);
    }

    let embedder = StaticEmbedder::from_dir(&model_dir).expect("モデルロード失敗");
    println!("model dim = {}", embedder.dim());

    // 動作確認用のサンプルフォルダ名（日英混在）
    let folders = [
        "C:/Users/nao/Documents/請求書",
        "C:/Users/nao/Documents/Invoices",
        "C:/Users/nao/Documents/領収書",
        "C:/Users/nao/Pictures/猫",
        "C:/Users/nao/Pictures/旅行写真",
        "C:/Projects/TreeScout/src",
        "C:/Projects/TreeScout/docs",
        "C:/Users/nao/Music/Playlist",
    ];

    let entries: Vec<(String, Vec<i8>)> = folders
        .iter()
        .map(|&path| {
            let name = Path::new(path).file_name().unwrap().to_string_lossy().to_string();
            let emb = embedder.embed_one(&name);
            (path.to_string(), quantize_f32_to_i8(&emb))
        })
        .collect();

    let dir = tempdir().expect("tempdir作成失敗");
    let matrix_path = dir.path().join("folders.bin");
    let matrix = FolderEmbeddingMatrix::build(&matrix_path, &entries, embedder.dim()).expect("行列構築失敗");

    let query_emb = embedder.embed_one(&query);
    let query_i8 = quantize_f32_to_i8(&query_emb);

    println!("\nquery = \"{query}\"");
    println!("{:<40}  cosine", "path");
    println!("{}", "-".repeat(50));
    for (path, score) in matrix.nearest(&query_i8, 5) {
        println!("{:<40}  {:+.4}", path, score);
    }
}
