use std::collections::BinaryHeap;
use std::path::Path;

use crate::node::{SearchNode, SearchResult};
use crate::tree::VirtualTree;

/// A*探索の設定パラメータ
#[derive(Debug, Clone)]
pub struct AstarConfig {
    /// 深さペナルティ係数。0 = 貪欲探索、大 = 浅いファイル優先
    pub lambda: f32,
    /// ベクトル化済みファイル数コスト係数
    pub mu: f32,
    /// 返す上位件数
    pub top_k: usize,
}

impl Default for AstarConfig {
    fn default() -> Self {
        Self { lambda: 0.1, mu: 0.001, top_k: 20 }
    }
}

/// ノード展開時に呼ばれるコールバック（UIへのストリーミング等に使う）
pub trait ExploreCallback: Send {
    fn on_open_dir(&mut self, path: &Path, h_score: f32);
    fn on_skip_dir(&mut self, path: &Path, h_score: f32);
    fn on_found_file(&mut self, path: &Path, score: f32);
}

/// 何もしないデフォルトコールバック
pub struct NoopCallback;
impl ExploreCallback for NoopCallback {
    fn on_open_dir(&mut self, _: &Path, _: f32) {}
    fn on_skip_dir(&mut self, _: &Path, _: f32) {}
    fn on_found_file(&mut self, _: &Path, _: f32) {}
}

/// A*探索エンジン
///
/// # 型パラメータ
/// - `H`: ヒューリスティック関数（フォルダ埋め込みとクエリ埋め込みの cosine 類似度）
/// - `S`: ファイルスコアリング関数（ファイルをオンデマンドでベクトル化して cosine）
pub struct AstarEngine<H, S> {
    config: AstarConfig,
    /// h(node): フォルダのヒューリスティック評価。戻り値は [0, 1]
    heuristic: H,
    /// ファイルの精密スコア計算。戻り値は [0, 1]
    scorer: S,
}

impl<H, S> AstarEngine<H, S>
where
    H: Fn(&Path) -> f32,
    S: Fn(&Path) -> f32,
{
    pub fn new(config: AstarConfig, heuristic: H, scorer: S) -> Self {
        Self { config, heuristic, scorer }
    }

    /// 仮想ツリー上で A* 探索を実行し、上位 K 件を返す。
    ///
    /// `callback` には探索ログをリアルタイムで通知する。
    /// Tauri Channel API と組み合わせる際はここでイベントを発行する。
    pub fn search(
        &self,
        tree: &VirtualTree,
        callback: &mut dyn ExploreCallback,
    ) -> Vec<SearchResult> {
        let cfg = &self.config;
        let mut queue: BinaryHeap<SearchNode> = BinaryHeap::new();
        let mut results: Vec<SearchResult> = Vec::with_capacity(cfg.top_k);
        let mut vectorized_count: usize = 0;

        // ルートノードをキューへ積む
        for root in &tree.roots {
            let is_file = tree.is_file(root);
            let h = if is_file { 0.0 } else { (self.heuristic)(root) };
            let g = 0.0_f32;
            queue.push(SearchNode {
                path: root.clone(),
                f_score: h - g,
                g_cost: g,
                h_score: h,
                is_file,
            });
        }

        while let Some(node) = queue.pop() {
            if results.len() >= cfg.top_k {
                break;
            }

            if node.is_file {
                // ファイル到達: 精密ベクトル化してスコア確定
                let score = (self.scorer)(&node.path);
                vectorized_count += 1;
                callback.on_found_file(&node.path, score);
                results.push(SearchResult { path: node.path, score });
            } else {
                // ディレクトリ展開
                let children = tree.children_of(&node.path);
                if children.is_empty() {
                    continue;
                }
                callback.on_open_dir(&node.path, node.h_score);

                let depth = node.path.components().count() as f32;
                for child in children {
                    let is_file = tree.is_file(child);
                    let h = if is_file {
                        // ファイルは親のヒューリスティックを継承（展開前にスコアは不明）
                        node.h_score
                    } else {
                        (self.heuristic)(child)
                    };
                    // g = 深さペナルティ + ベクトル化コスト
                    let g = depth * cfg.lambda
                        + vectorized_count as f32 * cfg.mu;
                    let f = h - g;

                    queue.push(SearchNode {
                        path: child.clone(),
                        f_score: f,
                        g_cost: g,
                        h_score: h,
                        is_file,
                    });
                }
            }
        }

        // スコア降順でソート
        results.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
        results
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tree::VirtualTree;
    use std::path::PathBuf;

    fn pb(s: &str) -> PathBuf {
        PathBuf::from(s)
    }

    /// ヒューリスティック: パスに "components" が含まれれば 0.9、さもなければ 0.1
    fn heuristic(path: &Path) -> f32 {
        if path.to_str().unwrap_or("").contains("components") {
            0.9
        } else {
            0.1
        }
    }

    /// スコアリング: ファイル名に "Button" が含まれれば 0.97、さもなければ 0.3
    fn scorer(path: &Path) -> f32 {
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if name.contains("Button") { 0.97 } else { 0.3 }
    }

    fn build_tree() -> VirtualTree {
        VirtualTree::from_paths(&[
            pb("/src/components/Button.tsx"),
            pb("/src/components/Input.tsx"),
            pb("/src/hooks/useButton.ts"),
            pb("/src/utils/format.ts"),
        ])
    }

    #[test]
    fn returns_top_k_results() {
        let tree = build_tree();
        let config = AstarConfig { top_k: 2, ..Default::default() };
        let engine = AstarEngine::new(config, heuristic, scorer);
        let results = engine.search(&tree, &mut NoopCallback);
        assert_eq!(results.len(), 2);
        // 最高スコアが先頭
        assert!(results[0].score >= results[1].score);
    }

    #[test]
    fn button_file_has_highest_score() {
        let tree = build_tree();
        let config = AstarConfig { top_k: 4, lambda: 0.0, mu: 0.0, ..Default::default() };
        let engine = AstarEngine::new(config, heuristic, scorer);
        let results = engine.search(&tree, &mut NoopCallback);
        assert_eq!(results[0].path.file_name().unwrap(), "Button.tsx");
        approx::assert_relative_eq!(results[0].score, 0.97, epsilon = 1e-6);
    }

    #[test]
    fn lambda_zero_finds_all_files() {
        let tree = build_tree();
        let config = AstarConfig { top_k: 10, lambda: 0.0, mu: 0.0, ..Default::default() };
        let engine = AstarEngine::new(config, heuristic, scorer);
        let results = engine.search(&tree, &mut NoopCallback);
        // 4 ファイル全件取得できること
        assert_eq!(results.len(), 4);
    }

    #[test]
    fn callback_receives_events() {
        struct Counter {
            dirs: usize,
            files: usize,
        }
        impl ExploreCallback for Counter {
            fn on_open_dir(&mut self, _: &Path, _: f32) { self.dirs += 1; }
            fn on_skip_dir(&mut self, _: &Path, _: f32) { self.dirs += 1; }
            fn on_found_file(&mut self, _: &Path, _: f32) { self.files += 1; }
        }

        let tree = build_tree();
        let config = AstarConfig { top_k: 10, lambda: 0.0, mu: 0.0, ..Default::default() };
        let engine = AstarEngine::new(config, heuristic, scorer);
        let mut cb = Counter { dirs: 0, files: 0 };
        engine.search(&tree, &mut cb);
        assert_eq!(cb.files, 4);
        assert!(cb.dirs > 0);
    }
}
