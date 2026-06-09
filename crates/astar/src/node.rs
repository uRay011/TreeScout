use std::cmp::Ordering;
use std::path::PathBuf;

/// A*優先キューのノード。最大ヒープで f_score 降順に取り出す。
#[derive(Debug, Clone)]
pub struct SearchNode {
    pub path: PathBuf,
    /// f = h - g（最大ヒープのため符号を反転した g を引く）
    pub f_score: f32,
    pub g_cost: f32,
    pub h_score: f32,
    pub is_file: bool,
}

impl PartialEq for SearchNode {
    fn eq(&self, other: &Self) -> bool {
        self.f_score == other.f_score
    }
}

impl Eq for SearchNode {}

impl PartialOrd for SearchNode {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for SearchNode {
    fn cmp(&self, other: &Self) -> Ordering {
        // NaN を最小扱いにして安定させる
        self.f_score
            .partial_cmp(&other.f_score)
            .unwrap_or(Ordering::Equal)
    }
}

/// A*探索の確定済み結果
#[derive(Debug, Clone)]
pub struct SearchResult {
    pub path: PathBuf,
    /// ファイルの最終スコア（コサイン類似度）
    pub score: f32,
}
