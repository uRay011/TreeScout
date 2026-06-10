use rusqlite::Connection;

use crate::error::IndexError;

/// `files` / `folders` テーブルを作成する（既存なら何もしない）。
pub fn init_schema(conn: &Connection) -> Result<(), IndexError> {
    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS files (
            path          TEXT PRIMARY KEY,
            name          TEXT NOT NULL,
            name_reading  TEXT,
            ext           TEXT,
            size          INTEGER,
            modified      INTEGER,
            embedding     BLOB,
            emb_mtime     INTEGER
        );

        CREATE TABLE IF NOT EXISTS folders (
            path              TEXT PRIMARY KEY,
            folder_embedding  BLOB
        );
        ",
    )?;
    Ok(())
}
