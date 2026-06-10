//! ファイルメタデータ・埋め込みの永続化レイヤー
//!
//! - [`IndexStore`]: SQLite上の `files` / `folders` テーブル。
//!   ファイル埋め込みは `(path, mtime)` キーでキャッシュし、USN差分更新と組み合わせて再計算を回避する。
//! - [`FolderEmbeddingMatrix`]: A* ヒューリスティックが多用するフォルダ埋め込みを
//!   mmap常駐の連続行列として保持する（SQLite行単位BLOB取得のキャッシュミスを排除）。
//!
//! コアの意味検索（Everything絞り込み後の~1000件）はANN拡張を使わず、
//! Rust側のSIMD cosine総当たり（`embedding::cosine_i8`）で行う。

mod error;
mod folder_matrix;
mod schema;
mod store;

pub use error::IndexError;
pub use folder_matrix::FolderEmbeddingMatrix;
pub use store::IndexStore;
