use rusqlite::{params, Connection, OptionalExtension};
use std::path::Path;

use crate::error::IndexError;
use crate::schema::init_schema;

/// ファイルメタデータ＋int8量子化埋め込みの永続ストア。
///
/// 埋め込みは `(path, mtime)` をキーにキャッシュし、
/// `mtime` が一致する限り再計算をスキップできる（USN差分更新と組み合わせる前提）。
pub struct IndexStore {
    conn: Connection,
}

impl IndexStore {
    /// SQLiteファイルを開く（存在しなければ作成し、スキーマを初期化する）。
    pub fn open(db_path: &Path) -> Result<Self, IndexError> {
        let conn = Connection::open(db_path)?;
        init_schema(&conn)?;
        Ok(Self { conn })
    }

    /// インメモリDB（テスト用）。
    pub fn open_in_memory() -> Result<Self, IndexError> {
        let conn = Connection::open_in_memory()?;
        init_schema(&conn)?;
        Ok(Self { conn })
    }

    /// ファイルの埋め込みキャッシュを取得する。
    ///
    /// 戻り値は `(embedding_i8, emb_mtime)`。`path` が未登録、または
    /// embedding が未設定（NULL）の場合は `None`。
    pub fn get_embedding(&self, path: &str) -> Result<Option<(Vec<i8>, i64)>, IndexError> {
        let row: Option<(Option<Vec<u8>>, Option<i64>)> = self
            .conn
            .query_row(
                "SELECT embedding, emb_mtime FROM files WHERE path = ?1",
                params![path],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?;

        Ok(match row {
            Some((Some(blob), Some(emb_mtime))) => Some((bytes_to_i8(&blob), emb_mtime)),
            _ => None,
        })
    }

    /// `(path, mtime)` キーでキャッシュが鮮度を保っているか確認する。
    pub fn is_embedding_fresh(&self, path: &str, mtime: i64) -> Result<bool, IndexError> {
        Ok(self
            .get_embedding(path)?
            .is_some_and(|(_, cached_mtime)| cached_mtime == mtime))
    }

    /// ファイルメタデータと int8 量子化埋め込みを upsert する。
    #[allow(clippy::too_many_arguments)]
    pub fn upsert_file(
        &self,
        path: &str,
        name: &str,
        name_reading: Option<&str>,
        ext: Option<&str>,
        size: i64,
        modified: i64,
        embedding: &[i8],
        emb_mtime: i64,
    ) -> Result<(), IndexError> {
        self.conn.execute(
            "INSERT INTO files (path, name, name_reading, ext, size, modified, embedding, emb_mtime)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
             ON CONFLICT(path) DO UPDATE SET
                name = excluded.name,
                name_reading = excluded.name_reading,
                ext = excluded.ext,
                size = excluded.size,
                modified = excluded.modified,
                embedding = excluded.embedding,
                emb_mtime = excluded.emb_mtime",
            params![
                path,
                name,
                name_reading,
                ext,
                size,
                modified,
                i8_to_bytes(embedding),
                emb_mtime,
            ],
        )?;
        Ok(())
    }

    /// パスをキーにファイル行を削除する（USN差分更新での削除反映用）。
    pub fn remove_file(&self, path: &str) -> Result<(), IndexError> {
        self.conn
            .execute("DELETE FROM files WHERE path = ?1", params![path])?;
        Ok(())
    }

    /// フォルダの int8 量子化埋め込みを upsert する（`mtime` なし、後方互換用）。
    pub fn upsert_folder_embedding(
        &self,
        path: &str,
        embedding: &[i8],
    ) -> Result<(), IndexError> {
        self.conn.execute(
            "INSERT INTO folders (path, folder_embedding, mtime) VALUES (?1, ?2, NULL)
             ON CONFLICT(path) DO UPDATE SET folder_embedding = excluded.folder_embedding",
            params![path, i8_to_bytes(embedding)],
        )?;
        Ok(())
    }

    /// フォルダの int8 量子化埋め込みを `mtime` 付きで upsert する。
    ///
    /// `mtime` はディレクトリの更新時刻（UNIX秒）。次回インデックス時に
    /// [`IndexStore::folder_mtime`] と比較して再計算をスキップする判定に使う。
    pub fn upsert_folder_embedding_with_mtime(
        &self,
        path: &str,
        embedding: &[i8],
        mtime: i64,
    ) -> Result<(), IndexError> {
        self.conn.execute(
            "INSERT INTO folders (path, folder_embedding, mtime) VALUES (?1, ?2, ?3)
             ON CONFLICT(path) DO UPDATE SET
                folder_embedding = excluded.folder_embedding,
                mtime = excluded.mtime",
            params![path, i8_to_bytes(embedding), mtime],
        )?;
        Ok(())
    }

    /// 登録済みフォルダの `mtime` を取得する（未登録なら `None`）。
    pub fn folder_mtime(&self, path: &str) -> Result<Option<i64>, IndexError> {
        Ok(self
            .conn
            .query_row(
                "SELECT mtime FROM folders WHERE path = ?1",
                params![path],
                |row| row.get::<_, Option<i64>>(0),
            )
            .optional()?
            .flatten())
    }

    /// インデックス済みフォルダパスの集合を取得する（USN差分での削除検知用）。
    pub fn all_folder_paths(&self) -> Result<std::collections::HashSet<String>, IndexError> {
        let mut stmt = self.conn.prepare("SELECT path FROM folders")?;
        let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
        rows.map(|r| r.map_err(IndexError::from)).collect()
    }

    /// パスをキーにフォルダ行を削除する（消滅したディレクトリの反映用）。
    pub fn remove_folder(&self, path: &str) -> Result<(), IndexError> {
        self.conn
            .execute("DELETE FROM folders WHERE path = ?1", params![path])?;
        Ok(())
    }

    /// 全フォルダの `(path, embedding)` を path 昇順で取得する。
    ///
    /// mmap常駐用の連続行列（[`crate::folder_matrix::FolderEmbeddingMatrix`]）を
    /// 構築する際の供給元として使う。
    pub fn all_folder_embeddings(&self) -> Result<Vec<(String, Vec<i8>)>, IndexError> {
        let mut stmt = self
            .conn
            .prepare("SELECT path, folder_embedding FROM folders WHERE folder_embedding IS NOT NULL ORDER BY path")?;
        let rows = stmt.query_map([], |row| {
            let path: String = row.get(0)?;
            let blob: Vec<u8> = row.get(1)?;
            Ok((path, bytes_to_i8(&blob)))
        })?;
        rows.map(|r| r.map_err(IndexError::from)).collect()
    }
}

/// `&[i8]` を SQLite BLOB 用 `Vec<u8>` に変換する（ビット列をそのまま保持）。
fn i8_to_bytes(v: &[i8]) -> Vec<u8> {
    v.iter().map(|&x| x as u8).collect()
}

/// SQLite BLOB から `Vec<i8>` に復元する。
fn bytes_to_i8(v: &[u8]) -> Vec<i8> {
    v.iter().map(|&x| x as i8).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn upsert_and_get_embedding_roundtrip() {
        let store = IndexStore::open_in_memory().unwrap();
        let emb: Vec<i8> = vec![-128, -1, 0, 1, 127];

        store
            .upsert_file("/a/b.txt", "b.txt", None, Some("txt"), 100, 1000, &emb, 1000)
            .unwrap();

        let (got_emb, got_mtime) = store.get_embedding("/a/b.txt").unwrap().unwrap();
        assert_eq!(got_emb, emb);
        assert_eq!(got_mtime, 1000);
    }

    #[test]
    fn get_embedding_missing_path_returns_none() {
        let store = IndexStore::open_in_memory().unwrap();
        assert!(store.get_embedding("/nope").unwrap().is_none());
    }

    #[test]
    fn is_embedding_fresh_detects_stale_mtime() {
        let store = IndexStore::open_in_memory().unwrap();
        let emb: Vec<i8> = vec![1, 2, 3];
        store
            .upsert_file("/a/b.txt", "b.txt", None, None, 0, 1000, &emb, 1000)
            .unwrap();

        assert!(store.is_embedding_fresh("/a/b.txt", 1000).unwrap());
        assert!(!store.is_embedding_fresh("/a/b.txt", 2000).unwrap());
        assert!(!store.is_embedding_fresh("/missing", 1000).unwrap());
    }

    #[test]
    fn upsert_file_overwrites_existing_row() {
        let store = IndexStore::open_in_memory().unwrap();
        let emb1: Vec<i8> = vec![1, 1, 1];
        let emb2: Vec<i8> = vec![2, 2, 2];

        store
            .upsert_file("/a/b.txt", "b.txt", None, None, 0, 1000, &emb1, 1000)
            .unwrap();
        store
            .upsert_file("/a/b.txt", "b.txt", None, None, 0, 2000, &emb2, 2000)
            .unwrap();

        let (got_emb, got_mtime) = store.get_embedding("/a/b.txt").unwrap().unwrap();
        assert_eq!(got_emb, emb2);
        assert_eq!(got_mtime, 2000);
    }

    #[test]
    fn remove_file_deletes_row() {
        let store = IndexStore::open_in_memory().unwrap();
        let emb: Vec<i8> = vec![1, 2, 3];
        store
            .upsert_file("/a/b.txt", "b.txt", None, None, 0, 1000, &emb, 1000)
            .unwrap();
        store.remove_file("/a/b.txt").unwrap();
        assert!(store.get_embedding("/a/b.txt").unwrap().is_none());
    }

    #[test]
    fn folder_embeddings_roundtrip_and_ordering() {
        let store = IndexStore::open_in_memory().unwrap();
        store
            .upsert_folder_embedding("/z", &[1, 2, 3])
            .unwrap();
        store
            .upsert_folder_embedding("/a", &[-1, -2, -3])
            .unwrap();

        let all = store.all_folder_embeddings().unwrap();
        assert_eq!(all.len(), 2);
        assert_eq!(all[0].0, "/a");
        assert_eq!(all[0].1, vec![-1i8, -2, -3]);
        assert_eq!(all[1].0, "/z");
        assert_eq!(all[1].1, vec![1i8, 2, 3]);
    }

    #[test]
    fn upsert_folder_embedding_overwrites() {
        let store = IndexStore::open_in_memory().unwrap();
        store.upsert_folder_embedding("/a", &[1, 2, 3]).unwrap();
        store.upsert_folder_embedding("/a", &[9, 9, 9]).unwrap();

        let all = store.all_folder_embeddings().unwrap();
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].1, vec![9i8, 9, 9]);
    }
}
