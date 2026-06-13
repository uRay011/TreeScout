use std::collections::{BinaryHeap, HashSet};
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
    /// AIサジェストとして採用するスコアの下限値
    pub suggest_threshold: f32,
    /// AIサジェストの最大件数
    pub k_suggest: usize,
}

impl Default for AstarConfig {
    fn default() -> Self {
        Self { lambda: 0.1, mu: 0.001, top_k: 20, suggest_threshold: 0.6, k_suggest: 10 }
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

    /// 仮想ツリー上で A* 探索を実行する。
    ///
    /// Phase1候補（Everything結果由来のノード）は全件スコアリングしてヒートマップ用の
    /// スコアを付与し、取りこぼしなく返す。Phase1候補外のノードも `VirtualTree::expand`
    /// 経由で探索し、`suggest_threshold` を超えた上位 `k_suggest` 件をAIサジェストとして
    /// Phase1結果に統合する。
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
        let mut phase1_results: Vec<SearchResult> = Vec::new();
        let mut suggestions: Vec<SearchResult> = Vec::new();
        let mut visited: HashSet<std::path::PathBuf> = HashSet::new();
        let mut vectorized_count: usize = 0;
        // Phase1候補ファイルのうち、まだスコア未付与の件数
        let mut phase1_pending = tree.phase1_file_count();

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
                in_phase1: tree.in_phase1(root),
            });
        }

        while let Some(node) = queue.pop() {
            // Phase1候補を全件スコアリングし、AIサジェストが k_suggest 件揃ったら終了
            if phase1_pending == 0 && suggestions.len() >= cfg.k_suggest {
                break;
            }

            if !visited.insert(node.path.clone()) {
                continue;
            }

            if node.is_file {
                // ファイル到達: 精密ベクトル化してスコア確定
                let score = (self.scorer)(&node.path);
                vectorized_count += 1;
                callback.on_found_file(&node.path, score);

                if node.in_phase1 {
                    phase1_results.push(SearchResult { path: node.path, score, in_phase1: true });
                    phase1_pending = phase1_pending.saturating_sub(1);
                } else if score > cfg.suggest_threshold {
                    suggestions.push(SearchResult { path: node.path, score, in_phase1: false });
                }
            } else {
                // ディレクトリ展開（Phase1候補の子 ＋ 候補外の兄弟/親ディレクトリ）
                let children = tree.expand(&node.path);
                if children.is_empty() {
                    continue;
                }
                callback.on_open_dir(&node.path, node.h_score);

                let depth = node.path.components().count() as f32;
                for child in children {
                    if visited.contains(&child) {
                        continue;
                    }
                    let is_file = tree.is_file(&child);
                    let h = if is_file {
                        // ファイルは親のヒューリスティックを継承（展開前にスコアは不明）
                        node.h_score
                    } else {
                        (self.heuristic)(&child)
                    };
                    // g = 深さペナルティ + ベクトル化コスト
                    let g = depth * cfg.lambda
                        + vectorized_count as f32 * cfg.mu;
                    let f = h - g;
                    let in_phase1 = tree.in_phase1(&child);

                    queue.push(SearchNode {
                        path: child,
                        f_score: f,
                        g_cost: g,
                        h_score: h,
                        is_file,
                        in_phase1,
                    });
                }
            }
        }

        // スコア降順でソート。Phase1結果は全件維持し、AIサジェストは上位 k_suggest 件のみ統合する。
        phase1_results.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
        suggestions.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
        suggestions.truncate(cfg.k_suggest);

        phase1_results.extend(suggestions);
        phase1_results
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
    fn phase1_results_are_returned_in_full_regardless_of_top_k() {
        let tree = build_tree();
        // top_k はもはや結果件数を制限しない（Phase1候補は全件スコアリングして返す）
        let config = AstarConfig { top_k: 2, ..Default::default() };
        let engine = AstarEngine::new(config, heuristic, scorer);
        let results = engine.search(&tree, &mut NoopCallback);
        assert_eq!(results.len(), 4);
        // 最高スコアが先頭
        assert!(results[0].score >= results[1].score);
        // build_tree の全パスはEverything候補（Phase1）由来
        assert!(results.iter().all(|r| r.in_phase1));
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

    #[test]
    fn expand_explores_extra_dirs_without_affecting_phase1_results() {
        struct DirCollector {
            opened: Vec<PathBuf>,
        }
        impl ExploreCallback for DirCollector {
            fn on_open_dir(&mut self, path: &Path, _: f32) { self.opened.push(path.to_path_buf()); }
            fn on_skip_dir(&mut self, _: &Path, _: f32) {}
            fn on_found_file(&mut self, _: &Path, _: f32) {}
        }

        // folders.bin 由来の兄弟ディレクトリ /src/extra を追加登録する
        let tree = build_tree().with_extra_folders(&[
            pb("/src/components"),
            pb("/src/hooks"),
            pb("/src/utils"),
            pb("/src/extra"),
        ]);
        let config = AstarConfig { lambda: 0.0, mu: 0.0, ..Default::default() };
        let engine = AstarEngine::new(config, heuristic, scorer);
        let mut cb = DirCollector { opened: vec![] };
        let results = engine.search(&tree, &mut cb);

        // Phase1候補のスコアリングは変わらず全件返る
        assert_eq!(results.iter().filter(|r| r.in_phase1).count(), 4);
        // 候補外の兄弟ディレクトリ /src/extra も探索対象として開かれる（無限ループせず終了）
        assert!(cb.opened.contains(&pb("/src/extra")));
    }
}
