/// λ・μパラメータのグリッドサーチによる自動チューニング評価セット
///
/// # 使用方法
/// `cargo test -p astar tune -- --nocapture` で全チューニング結果を表示
///
/// # 評価指標
/// - Precision@K: 上位K件中に期待ファイルが含まれる割合
/// - Recall@K: 期待ファイル全件のうち上位K件に入った割合
/// - Mean Reciprocal Rank (MRR): 最初の正解が何位に来るかの逆数平均
/// - vectorized_count: ベクトル化したファイル数（コスト代理指標）

use crate::engine::{AstarConfig, AstarEngine, ExploreCallback};
use crate::node::SearchResult;
use crate::tree::VirtualTree;
use std::path::{Path, PathBuf};

// -----------------------------------------------------------------------
// 評価データセット
// -----------------------------------------------------------------------

/// 単一クエリのシナリオ定義
#[derive(Debug, Clone)]
pub struct EvalCase {
    pub name: &'static str,
    /// クエリ文字列（実際のembeddingが使えない単体テストでは判定クロージャで代用）
    pub query: &'static str,
    /// ツリー内の全パス
    pub paths: Vec<PathBuf>,
    /// 上位K件に含まれていてほしいパス（正解セット）
    pub relevant: Vec<PathBuf>,
    /// 評価する top_k
    pub top_k: usize,
}

fn pb(s: &str) -> PathBuf {
    PathBuf::from(s)
}

/// 組み込み評価ケース一覧
pub fn default_eval_cases() -> Vec<EvalCase> {
    vec![
        // ケース1: 浅いディレクトリに正解が集中（λ大で有利）
        // /project 以下の単一ルートで構成。"button" が浅い /components 直下にある。
        EvalCase {
            name: "shallow_relevant",
            query: "button component",
            paths: vec![
                pb("/project/src/components/Button.tsx"),
                pb("/project/src/components/Input.tsx"),
                pb("/project/src/components/Modal.tsx"),
                pb("/project/src/deep/a/b/c/d/Button2.tsx"),
                pb("/project/src/utils/format.ts"),
                pb("/project/src/utils/parse.ts"),
                pb("/project/lib/helpers/string.ts"),
            ],
            relevant: vec![
                pb("/project/src/components/Button.tsx"),
                pb("/project/src/components/Modal.tsx"),
            ],
            top_k: 3,
        },
        // ケース2: 正解が深いディレクトリにある（λ小で有利）
        // /project 以下の単一ルートで構成。"auth" がディレクトリ・ファイル名に含まれる。
        EvalCase {
            name: "deep_relevant",
            query: "auth token",
            paths: vec![
                pb("/project/src/core/main.rs"),
                pb("/project/src/core/lib.rs"),
                pb("/project/src/server/auth/token.rs"),
                pb("/project/src/server/auth/refresh.rs"),
                pb("/project/src/server/public/index.rs"),
                pb("/project/src/server/public/health.rs"),
            ],
            relevant: vec![
                pb("/project/src/server/auth/token.rs"),
                pb("/project/src/server/auth/refresh.rs"),
            ],
            top_k: 3,
        },
        // ケース3: 正解が複数ディレクトリに分散（μが探索幅に影響）
        EvalCase {
            name: "spread_relevant",
            query: "database query",
            paths: vec![
                pb("/project/src/db/query.rs"),
                pb("/project/src/db/schema.rs"),
                pb("/project/src/db/migration.rs"),
                pb("/project/src/api/users.rs"),
                pb("/project/src/api/posts.rs"),
                pb("/project/src/cache/redis.rs"),
                pb("/project/tests/db/query_test.rs"),
                pb("/project/tests/api/users_test.rs"),
            ],
            relevant: vec![
                pb("/project/src/db/query.rs"),
                pb("/project/tests/db/query_test.rs"),
            ],
            top_k: 4,
        },
        // ケース4: 正解が1件のみ、ノイズが多い
        EvalCase {
            name: "needle_in_haystack",
            query: "config loader",
            paths: vec![
                pb("/project/src/core/config.rs"),
                pb("/project/src/utils/string.rs"),
                pb("/project/src/utils/number.rs"),
                pb("/project/src/utils/date.rs"),
                pb("/project/src/models/user.rs"),
                pb("/project/src/models/post.rs"),
                pb("/project/src/models/comment.rs"),
                pb("/project/src/views/index.html"),
                pb("/project/src/views/about.html"),
            ],
            relevant: vec![pb("/project/src/core/config.rs")],
            top_k: 3,
        },
    ]
}

// -----------------------------------------------------------------------
// ヒューリスティック・スコアラファクトリ
// -----------------------------------------------------------------------

/// ヒューリスティック: query キーワードがパスに含まれる割合を [0,1] で返す簡易実装。
/// 実際の embedding cosine をテスト内でモックするためのファクトリ。
pub fn keyword_heuristic(query: &str) -> impl Fn(&Path) -> f32 + '_ {
    let keywords: Vec<String> = query
        .split_whitespace()
        .map(|w| w.to_lowercase())
        .collect();
    move |path: &Path| {
        let s = path.to_str().unwrap_or("").to_lowercase();
        let hits = keywords.iter().filter(|k| s.contains(k.as_str())).count();
        hits as f32 / keywords.len().max(1) as f32
    }
}

/// スコアラ: ファイルパスとクエリのキーワード一致率をスコアとして返す。
pub fn keyword_scorer(query: &str) -> impl Fn(&Path) -> f32 + '_ {
    let keywords: Vec<String> = query
        .split_whitespace()
        .map(|w| w.to_lowercase())
        .collect();
    move |path: &Path| {
        let name = path.file_stem().and_then(|s| s.to_str()).unwrap_or("").to_lowercase();
        let dir = path.parent().and_then(|p| p.to_str()).unwrap_or("").to_lowercase();
        let combined = format!("{} {}", name, dir);
        let hits = keywords.iter().filter(|k| combined.contains(k.as_str())).count();
        // ファイル名一致はボーナス（0.5 baseline + 最大 0.5 の加算）
        0.5 + 0.5 * (hits as f32 / keywords.len().max(1) as f32)
    }
}

// -----------------------------------------------------------------------
// 評価指標
// -----------------------------------------------------------------------

/// 1ケースの評価結果
#[derive(Debug, Clone)]
pub struct EvalMetrics {
    pub case_name: &'static str,
    pub lambda: f32,
    pub mu: f32,
    pub precision_at_k: f32,
    pub recall_at_k: f32,
    pub mrr: f32,
    /// 探索中にベクトル化したファイル数（コスト指標）
    pub vectorized_count: usize,
}

impl EvalMetrics {
    /// 精度・コストの複合スコア（高いほど良い）
    /// コスト項は正規化のため total_files で割る
    pub fn composite_score(&self, total_files: usize) -> f32 {
        let cost_penalty = self.vectorized_count as f32 / total_files.max(1) as f32;
        0.5 * self.precision_at_k + 0.3 * self.recall_at_k + 0.2 * self.mrr
            - 0.1 * cost_penalty
    }
}

/// 結果セットに対してメトリクスを計算する
pub fn compute_metrics(
    case: &EvalCase,
    results: &[SearchResult],
    lambda: f32,
    mu: f32,
    vectorized_count: usize,
) -> EvalMetrics {
    let top_k = case.top_k.min(results.len());
    let top_results = &results[..top_k];

    // Precision@K
    let hits = top_results
        .iter()
        .filter(|r| case.relevant.contains(&r.path))
        .count();
    let precision_at_k = hits as f32 / top_k.max(1) as f32;

    // Recall@K
    let recall_at_k = hits as f32 / case.relevant.len().max(1) as f32;

    // MRR: 最初の正解の順位の逆数
    let mrr = top_results
        .iter()
        .enumerate()
        .find(|(_, r)| case.relevant.contains(&r.path))
        .map(|(i, _)| 1.0 / (i + 1) as f32)
        .unwrap_or(0.0);

    EvalMetrics {
        case_name: case.name,
        lambda,
        mu,
        precision_at_k,
        recall_at_k,
        mrr,
        vectorized_count,
    }
}

// -----------------------------------------------------------------------
// グリッドサーチランナー
// -----------------------------------------------------------------------

/// グリッドサーチで試すλ・μの候補値
pub const LAMBDA_GRID: &[f32] = &[0.0, 0.05, 0.1, 0.2, 0.5, 1.0];
pub const MU_GRID: &[f32] = &[0.0, 0.001, 0.005, 0.01, 0.05];

/// ベクトル化カウンタ付きコールバック
struct CountingCallback {
    pub vectorized_count: usize,
}

impl ExploreCallback for CountingCallback {
    fn on_open_dir(&mut self, _: &Path, _: f32) {}
    fn on_skip_dir(&mut self, _: &Path, _: f32) {}
    fn on_found_file(&mut self, _: &Path, _: f32) {
        self.vectorized_count += 1;
    }
}

/// 1ケース × 全λ・μ組み合わせを評価して結果リストを返す
pub fn grid_search_case(case: &EvalCase) -> Vec<EvalMetrics> {
    let tree = VirtualTree::from_paths(&case.paths);
    let total_files = case.paths.iter().filter(|p| {
        // VirtualTree はファイル/ディレクトリを自動判定するが、ここでは拡張子あり = ファイルと見なす
        p.extension().is_some()
    }).count();

    let heuristic_fn = keyword_heuristic(case.query);
    let scorer_fn = keyword_scorer(case.query);

    let mut all_metrics = Vec::new();

    for &lambda in LAMBDA_GRID {
        for &mu in MU_GRID {
            let config = AstarConfig { lambda, mu, top_k: case.top_k, ..Default::default() };
            let engine = AstarEngine::new(
                config,
                |p| heuristic_fn(p),
                |p| scorer_fn(p),
            );
            let mut cb = CountingCallback { vectorized_count: 0 };
            let results = engine.search(&tree, &mut cb);

            let metrics = compute_metrics(case, &results, lambda, mu, cb.vectorized_count);
            all_metrics.push((metrics, total_files));
        }
    }

    all_metrics.into_iter().map(|(m, _)| m).collect()
}

/// 全ケースを評価し、ケースごとに最良パラメータを返す
pub fn run_full_grid_search(cases: &[EvalCase]) -> Vec<GridSearchReport> {
    cases.iter().map(|case| {
        let tree = VirtualTree::from_paths(&case.paths);
        let total_files = case.paths.iter().filter(|p| p.extension().is_some()).count();

        let heuristic_fn = keyword_heuristic(case.query);
        let scorer_fn = keyword_scorer(case.query);

        let mut best: Option<(EvalMetrics, f32)> = None;
        let mut all: Vec<EvalMetrics> = Vec::new();

        for &lambda in LAMBDA_GRID {
            for &mu in MU_GRID {
                let config = AstarConfig { lambda, mu, top_k: case.top_k, ..Default::default() };
                let engine = AstarEngine::new(
                    config,
                    |p| heuristic_fn(p),
                    |p| scorer_fn(p),
                );
                let mut cb = CountingCallback { vectorized_count: 0 };
                let results = engine.search(&tree, &mut cb);
                let metrics = compute_metrics(case, &results, lambda, mu, cb.vectorized_count);
                let score = metrics.composite_score(total_files);

                if best.as_ref().map_or(true, |(_, s)| score > *s) {
                    best = Some((metrics.clone(), score));
                }
                all.push(metrics);
            }
        }

        let (best_metrics, best_score) = best.unwrap();
        GridSearchReport {
            case_name: case.name,
            best_lambda: best_metrics.lambda,
            best_mu: best_metrics.mu,
            best_score,
            best_precision: best_metrics.precision_at_k,
            best_recall: best_metrics.recall_at_k,
            best_mrr: best_metrics.mrr,
            best_vectorized: best_metrics.vectorized_count,
            all_metrics: all,
        }
    }).collect()
}

/// ケースごとのグリッドサーチ報告
#[derive(Debug)]
pub struct GridSearchReport {
    pub case_name: &'static str,
    pub best_lambda: f32,
    pub best_mu: f32,
    pub best_score: f32,
    pub best_precision: f32,
    pub best_recall: f32,
    pub best_mrr: f32,
    pub best_vectorized: usize,
    pub all_metrics: Vec<EvalMetrics>,
}

impl GridSearchReport {
    pub fn print_summary(&self) {
        println!(
            "[{}] best λ={:.3} μ={:.4} | composite={:.4} P@K={:.2} R@K={:.2} MRR={:.2} vec={}",
            self.case_name,
            self.best_lambda,
            self.best_mu,
            self.best_score,
            self.best_precision,
            self.best_recall,
            self.best_mrr,
            self.best_vectorized,
        );
    }

    /// 全組み合わせをCSV形式で出力（スプレッドシートで可視化用）
    pub fn to_csv_rows(&self) -> Vec<String> {
        let mut rows = vec![
            "case,lambda,mu,precision_at_k,recall_at_k,mrr,vectorized_count,composite_score"
                .to_string(),
        ];
        for m in &self.all_metrics {
            // composite_score は total_files なしでは正規化できないが概算として vectorized / top_k を使う
            let approx_composite = 0.5 * m.precision_at_k + 0.3 * m.recall_at_k + 0.2 * m.mrr;
            rows.push(format!(
                "{},{:.3},{:.4},{:.4},{:.4},{:.4},{},{:.4}",
                m.case_name,
                m.lambda,
                m.mu,
                m.precision_at_k,
                m.recall_at_k,
                m.mrr,
                m.vectorized_count,
                approx_composite,
            ));
        }
        rows
    }
}

// -----------------------------------------------------------------------
// テスト
// -----------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grid_search_returns_metrics_for_all_combinations() {
        let cases = default_eval_cases();
        let first = &cases[0];
        let metrics = grid_search_case(first);
        assert_eq!(metrics.len(), LAMBDA_GRID.len() * MU_GRID.len());
    }

    #[test]
    fn full_grid_search_finds_nonzero_precision() {
        let cases = default_eval_cases();
        let reports = run_full_grid_search(&cases);
        assert_eq!(reports.len(), cases.len());
        for report in &reports {
            // 少なくとも最良パラメータでは1件以上ヒットすること
            assert!(
                report.best_precision > 0.0,
                "case '{}' got zero precision for all λ/μ combinations",
                report.case_name
            );
        }
    }

    #[test]
    fn lambda_zero_maximizes_recall() {
        // λ=0 はコストペナルティなしで全ファイルに到達できるため recall が最大になるはず
        let cases = default_eval_cases();
        for case in &cases {
            let tree = VirtualTree::from_paths(&case.paths);
            let hf = keyword_heuristic(case.query);
            let sf = keyword_scorer(case.query);

            let config_greedy =
                AstarConfig { lambda: 0.0, mu: 0.0, top_k: case.paths.len(), ..Default::default() };
            let engine = AstarEngine::new(config_greedy, |p| hf(p), |p| sf(p));
            let mut cb = CountingCallback { vectorized_count: 0 };
            let results = engine.search(&tree, &mut cb);

            // top_k = 全件 なら relevant は必ず全部含まれる
            for rel in &case.relevant {
                assert!(
                    results.iter().any(|r| &r.path == rel),
                    "case '{}': relevant file {:?} not found with λ=0 top_k=all",
                    case.name,
                    rel
                );
            }
        }
    }

    #[test]
    fn metrics_precision_recall_range() {
        let cases = default_eval_cases();
        let reports = run_full_grid_search(&cases);
        for report in &reports {
            for m in &report.all_metrics {
                assert!((0.0..=1.0).contains(&m.precision_at_k), "precision out of range");
                assert!((0.0..=1.0).contains(&m.recall_at_k), "recall out of range");
                assert!((0.0..=1.0).contains(&m.mrr), "mrr out of range");
            }
        }
    }

    /// --nocapture で全ケースのチューニング結果をダンプする
    #[test]
    fn print_tuning_report() {
        let cases = default_eval_cases();
        let reports = run_full_grid_search(&cases);
        println!("\n===== λ・μ チューニングレポート =====");
        for report in &reports {
            report.print_summary();
        }
        println!("=====================================\n");

        println!("--- CSV (全組み合わせ) ---");
        for report in &reports {
            for line in report.to_csv_rows() {
                println!("{}", line);
            }
            println!();
        }
    }
}

