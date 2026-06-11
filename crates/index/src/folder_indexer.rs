//! フォルダ embedding の事前インデックス（バックグラウンド常駐）
//!
//! A* ヒューリスティックが参照するフォルダ embedding を、実ファイルシステムの
//! ディレクトリツリーから事前計算して [`IndexStore`] に永続化する。
//!
//! - 対象テキストは**フォルダ名のみ**（フルパスではない）。軽量・ms単位で計算できる。
//! - ディレクトリの `mtime` をキーに差分判定し、変更のないフォルダは再計算をスキップする。
//! - 配下フォルダ名は**1回のバッチ呼び出し**でまとめて embedding する（固定オーバーヘッド削減）。
//! - 完了後は [`FolderEmbeddingMatrix`] を再構築し、A* が mmap 経由で参照できるようにする。

use std::path::Path;
use std::time::UNIX_EPOCH;
use walkdir::WalkDir;

use crate::error::IndexError;
use crate::folder_matrix::FolderEmbeddingMatrix;
use crate::store::IndexStore;
use embedding::{quantize_f32_to_i8, Embedder};

/// フォルダ事前インデックスの実行結果。
#[derive(Debug, Default, Clone, Copy)]
pub struct FolderIndexStats {
    /// 新規/更新で embedding を計算したフォルダ数
    pub updated: usize,
    /// `mtime` 変化なしでスキップしたフォルダ数
    pub skipped: usize,
    /// 走査済みフォルダの総数
    pub scanned: usize,
    /// 消滅していたため削除したフォルダ数
    pub removed: usize,
}

/// `root` 配下のディレクトリを走査し、フォルダ embedding を差分更新する。
///
/// 1. ディレクトリツリーを走査し、各ディレクトリの `mtime` を取得する。
/// 2. 既存レコードと `mtime` が一致するフォルダはスキップする。
/// 3. 変更のあったフォルダ名をまとめて `embedder.embed()` でバッチ embedding する。
/// 4. int8 量子化して `(path, embedding, mtime)` を upsert する。
/// 5. 走査で見つからなかった既存フォルダ（消滅扱い）は削除する。
///
/// `embed_batch_size` は1回の `embed()` 呼び出しに渡すフォルダ数の上限。
pub fn index_folders<E: Embedder + ?Sized>(
    store: &IndexStore,
    root: &Path,
    embedder: &E,
    embed_batch_size: usize,
) -> Result<FolderIndexStats, IndexError> {
    let mut stats = FolderIndexStats::default();
    let mut seen_paths: std::collections::HashSet<String> = std::collections::HashSet::new();

    // (path, folder_name, mtime) のうち embedding 計算が必要なもの
    let mut pending: Vec<(String, String, i64)> = Vec::new();

    for entry in WalkDir::new(root)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_dir())
    {
        stats.scanned += 1;

        let path = entry.path();
        let path_str = match path.to_str() {
            Some(s) => s.to_string(),
            None => continue, // 不正なUTF-8パスはスキップ
        };
        seen_paths.insert(path_str.clone());

        let mtime = dir_mtime(&entry);
        if store.folder_mtime(&path_str)? == Some(mtime) {
            stats.skipped += 1;
            continue;
        }

        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or(&path_str)
            .to_string();
        pending.push((path_str, name, mtime));
    }

    // バッチ embedding（固定オーバーヘッド削減のため複数件まとめて呼ぶ）
    for chunk in pending.chunks(embed_batch_size.max(1)) {
        let names: Vec<&str> = chunk.iter().map(|(_, name, _)| name.as_str()).collect();
        let embeddings = embedder.embed(&names);

        for ((path_str, _, mtime), emb) in chunk.iter().zip(embeddings.iter()) {
            let quantized = quantize_f32_to_i8(emb);
            store.upsert_folder_embedding_with_mtime(path_str, &quantized, *mtime)?;
            stats.updated += 1;
        }
    }

    // 消滅したフォルダを削除
    for existing in store.all_folder_paths()? {
        if !seen_paths.contains(&existing) {
            store.remove_folder(&existing)?;
            stats.removed += 1;
        }
    }

    Ok(stats)
}

/// `index_folders` 実行後、SQLite の全フォルダ embedding から
/// mmap常駐用の連続行列を `out_path` に再構築する。
pub fn rebuild_folder_matrix(
    store: &IndexStore,
    out_path: &Path,
    dim: usize,
) -> Result<FolderEmbeddingMatrix, IndexError> {
    let entries = store.all_folder_embeddings()?;
    FolderEmbeddingMatrix::build(out_path, &entries, dim)
}

/// ディレクトリエントリの更新時刻を UNIX 秒で取得する。
/// 取得失敗時は 0（= 常に再計算対象）を返す。
fn dir_mtime(entry: &walkdir::DirEntry) -> i64 {
    entry
        .metadata()
        .ok()
        .and_then(|m| m.modified().ok())
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    /// テスト用の決定的ダミー埋め込み器。
    /// 文字列の長さを次元1のベクトルに変換するだけ。
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

    #[test]
    fn indexes_all_directories_recursively() {
        let dir = tempdir().unwrap();
        fs::create_dir_all(dir.path().join("a/b")).unwrap();
        fs::create_dir_all(dir.path().join("a/c")).unwrap();

        let store = IndexStore::open_in_memory().unwrap();
        let embedder = DummyEmbedder { dim: 4 };

        let stats = index_folders(&store, dir.path(), &embedder, 32).unwrap();

        // root, a, a/b, a/c の4ディレクトリ
        assert_eq!(stats.scanned, 4);
        assert_eq!(stats.updated, 4);
        assert_eq!(stats.skipped, 0);
        assert_eq!(stats.removed, 0);

        let all = store.all_folder_embeddings().unwrap();
        assert_eq!(all.len(), 4);
    }

    #[test]
    fn second_run_skips_unchanged_directories() {
        let dir = tempdir().unwrap();
        fs::create_dir_all(dir.path().join("a")).unwrap();

        let store = IndexStore::open_in_memory().unwrap();
        let embedder = DummyEmbedder { dim: 4 };

        let first = index_folders(&store, dir.path(), &embedder, 32).unwrap();
        assert_eq!(first.updated, 2); // root + a

        let second = index_folders(&store, dir.path(), &embedder, 32).unwrap();
        assert_eq!(second.updated, 0);
        assert_eq!(second.skipped, 2);
    }

    #[test]
    fn removed_directory_is_pruned() {
        let dir = tempdir().unwrap();
        let sub = dir.path().join("a");
        fs::create_dir_all(&sub).unwrap();

        let store = IndexStore::open_in_memory().unwrap();
        let embedder = DummyEmbedder { dim: 4 };

        index_folders(&store, dir.path(), &embedder, 32).unwrap();
        assert_eq!(store.all_folder_embeddings().unwrap().len(), 2);

        fs::remove_dir(&sub).unwrap();
        let stats = index_folders(&store, dir.path(), &embedder, 32).unwrap();

        assert_eq!(stats.removed, 1);
        assert_eq!(store.all_folder_embeddings().unwrap().len(), 1);
    }

    #[test]
    fn rebuild_matrix_reflects_indexed_folders() {
        let dir = tempdir().unwrap();
        fs::create_dir_all(dir.path().join("a/b")).unwrap();

        let store = IndexStore::open_in_memory().unwrap();
        let embedder = DummyEmbedder { dim: 4 };
        index_folders(&store, dir.path(), &embedder, 32).unwrap();

        let out = dir.path().join("folders.bin");
        let matrix = rebuild_folder_matrix(&store, &out, 4).unwrap();

        // root, a, a/b の3ディレクトリ
        assert_eq!(matrix.len(), 3);
        assert_eq!(matrix.dim(), 4);
    }

    #[test]
    fn modified_directory_is_recomputed() {
        let dir = tempdir().unwrap();
        let sub = dir.path().join("a");
        fs::create_dir_all(&sub).unwrap();

        let store = IndexStore::open_in_memory().unwrap();
        let embedder = DummyEmbedder { dim: 4 };
        index_folders(&store, dir.path(), &embedder, 32).unwrap();

        // mtime を意図的に過去にずらして「変更あり」を再現
        let old_mtime = store.folder_mtime(sub.to_str().unwrap()).unwrap().unwrap();
        store
            .upsert_folder_embedding_with_mtime(sub.to_str().unwrap(), &[0; 4], old_mtime - 100)
            .unwrap();

        let stats = index_folders(&store, dir.path(), &embedder, 32).unwrap();
        assert_eq!(stats.updated, 1); // a のみ再計算
        assert_eq!(stats.skipped, 1); // root はスキップ
    }
}
