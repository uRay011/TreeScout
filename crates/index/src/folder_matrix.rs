//! フォルダ埋め込みの mmap 常駐連続行列ストア
//!
//! A* ヒューリスティックはフォルダ埋め込みを高頻度に参照するため、
//! SQLite の行単位 BLOB 取得（キャッシュミスが発生しやすい）ではなく、
//! 固定長レコードを敷き詰めたバイナリファイルを mmap して
//! メモリ上の連続行列として保持する。
//!
//! ファイルレイアウト（リトルエンディアン）:
//! ```text
//! [u32: dim][u32: n]
//! [embedding行列: i8 * dim * n]  ← 連続領域。SIMD総当たりはここのみ走査
//! [パステーブル: (u32 len, UTF-8 bytes) * n]
//! ```

use memmap2::Mmap;
use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::Path;

use crate::error::IndexError;

const HEADER_LEN: usize = 8; // u32: dim, u32: n

/// mmap 常駐フォルダ埋め込み行列。
///
/// `iter_embeddings()` / `embedding(i)` で `&[i8]` を取得し、
/// SIMD cosine 総当たりに直接渡せる。
#[derive(Debug)]
pub struct FolderEmbeddingMatrix {
    mmap: Mmap,
    dim: usize,
    paths: Vec<String>,
}

impl FolderEmbeddingMatrix {
    /// `(path, embedding)` の一覧からバイナリファイルを構築して mmap で開く。
    ///
    /// 全 embedding は同一次元 `dim` を持つ必要がある。
    pub fn build(
        out_path: &Path,
        entries: &[(String, Vec<i8>)],
        dim: usize,
    ) -> Result<Self, IndexError> {
        for (_, emb) in entries {
            if emb.len() != dim {
                return Err(IndexError::DimMismatch {
                    expected: dim,
                    actual: emb.len(),
                });
            }
        }

        {
            let file = File::create(out_path)?;
            let mut w = BufWriter::new(file);
            w.write_all(&(dim as u32).to_le_bytes())?;
            w.write_all(&(entries.len() as u32).to_le_bytes())?;

            // 埋め込み行列を連続領域として先頭にまとめる
            for (_, emb) in entries {
                let bytes: Vec<u8> = emb.iter().map(|&x| x as u8).collect();
                w.write_all(&bytes)?;
            }
            // 続けてパステーブル（長さプレフィックス付き）
            for (path, _) in entries {
                let path_bytes = path.as_bytes();
                w.write_all(&(path_bytes.len() as u32).to_le_bytes())?;
                w.write_all(path_bytes)?;
            }
            w.flush()?;
        }

        Self::open(out_path)
    }

    /// 既存のバイナリファイルを mmap で開く。
    pub fn open(path: &Path) -> Result<Self, IndexError> {
        let file = File::open(path)?;
        let mmap = unsafe { Mmap::map(&file)? };

        let dim = u32::from_le_bytes(mmap[0..4].try_into().unwrap()) as usize;
        let n = u32::from_le_bytes(mmap[4..8].try_into().unwrap()) as usize;

        let mut offset = HEADER_LEN + n * dim;
        let mut paths = Vec::with_capacity(n);
        for _ in 0..n {
            let len = u32::from_le_bytes(mmap[offset..offset + 4].try_into().unwrap()) as usize;
            offset += 4;
            let s = std::str::from_utf8(&mmap[offset..offset + len])
                .expect("パステーブルはUTF-8である必要がある")
                .to_string();
            paths.push(s);
            offset += len;
        }

        Ok(Self { mmap, dim, paths })
    }

    /// 埋め込み次元数。
    pub fn dim(&self) -> usize {
        self.dim
    }

    /// 件数。
    pub fn len(&self) -> usize {
        self.paths.len()
    }

    pub fn is_empty(&self) -> bool {
        self.paths.is_empty()
    }

    /// パス一覧（行列の行と同じ順序）。
    pub fn paths(&self) -> &[String] {
        &self.paths
    }

    /// `index` 行目の埋め込みスライス（`[i8; dim]` 相当）。
    pub fn embedding(&self, index: usize) -> &[i8] {
        let start = HEADER_LEN + index * self.dim;
        let end = start + self.dim;
        bytes_as_i8(&self.mmap[start..end])
    }

    /// 全埋め込みを `dim` 単位のチャンクとして走査するイテレータ。
    pub fn iter_embeddings(&self) -> impl Iterator<Item = &[i8]> {
        (0..self.len()).map(move |i| self.embedding(i))
    }
}

/// `&[u8]` を `&[i8]` として再解釈する（i8/u8 はメモリレイアウト互換）。
fn bytes_as_i8(b: &[u8]) -> &[i8] {
    // SAFETY: i8 と u8 はサイズ・アラインメントが同一で、
    // ビットパターンの再解釈のみ行う（値の意味はビット列として一致）。
    unsafe { std::slice::from_raw_parts(b.as_ptr() as *const i8, b.len()) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn build_and_open_roundtrip() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("folders.bin");

        let entries = vec![
            ("/a".to_string(), vec![1i8, 2, 3, 4]),
            ("/b/c".to_string(), vec![-1i8, -2, -3, -4]),
            ("/d".to_string(), vec![0i8, 0, 0, 0]),
        ];

        let matrix = FolderEmbeddingMatrix::build(&path, &entries, 4).unwrap();

        assert_eq!(matrix.dim(), 4);
        assert_eq!(matrix.len(), 3);
        assert_eq!(matrix.paths(), &["/a".to_string(), "/b/c".to_string(), "/d".to_string()]);
        assert_eq!(matrix.embedding(0), &[1i8, 2, 3, 4]);
        assert_eq!(matrix.embedding(1), &[-1i8, -2, -3, -4]);
        assert_eq!(matrix.embedding(2), &[0i8, 0, 0, 0]);
    }

    #[test]
    fn reopen_from_disk() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("folders.bin");

        let entries = vec![
            ("/x".to_string(), vec![10i8, 20, 30]),
            ("/y".to_string(), vec![-10i8, -20, -30]),
        ];
        FolderEmbeddingMatrix::build(&path, &entries, 3).unwrap();

        let matrix = FolderEmbeddingMatrix::open(&path).unwrap();
        assert_eq!(matrix.dim(), 3);
        assert_eq!(matrix.len(), 2);
        assert_eq!(matrix.embedding(0), &[10i8, 20, 30]);
        assert_eq!(matrix.embedding(1), &[-10i8, -20, -30]);
        assert_eq!(matrix.paths()[1], "/y");
    }

    #[test]
    fn empty_entries() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("empty.bin");
        FolderEmbeddingMatrix::build(&path, &[], 384).unwrap();
        let matrix = FolderEmbeddingMatrix::open(&path).unwrap();
        assert_eq!(matrix.dim(), 384);
        assert!(matrix.is_empty());
    }

    #[test]
    fn dim_mismatch_rejected() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("bad.bin");
        let entries = vec![("/a".to_string(), vec![1i8, 2, 3])];
        let err = FolderEmbeddingMatrix::build(&path, &entries, 4).unwrap_err();
        assert!(matches!(err, IndexError::DimMismatch { expected: 4, actual: 3 }));
    }

    #[test]
    fn iter_embeddings_matches_paths_order() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("folders.bin");
        let entries = vec![
            ("/a".to_string(), vec![1i8, 1]),
            ("/b".to_string(), vec![2i8, 2]),
        ];
        let matrix = FolderEmbeddingMatrix::build(&path, &entries, 2).unwrap();
        let collected: Vec<&[i8]> = matrix.iter_embeddings().collect();
        assert_eq!(collected, vec![&[1i8, 1][..], &[2i8, 2][..]]);
    }
}
