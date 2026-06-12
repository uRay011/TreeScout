//! 2フェーズ検索パイプライン
//!
//! ```text
//! [自然言語クエリ]
//!       ↓ (1) NLP解析 (nlp::parse_query)
//! [ParsedQuery] → Everything構文
//!       ↓ (2) Everything高速絞り込み (<50ms)
//! [パスリスト ~1000件]
//!       ↓ (3) VirtualTree構築
//! [仮想ツリー]
//!       ↓ (4) A*探索 (<150ms)
//! [上位K件 SearchResult]
//! ```
//!
//! Windows 以外では (2) のみダミー実装になるため、
//! テストは cross-platform で実行できる。

mod error;
pub mod scoring;
pub use error::PipelineError;

use std::path::PathBuf;

use astar::{AstarConfig, AstarEngine, ExploreCallback, SearchResult, VirtualTree};
use nlp::{parse_query, ParsedQuery};

/// パイプライン設定
pub struct PipelineConfig {
    pub astar: AstarConfig,
    /// Everything 最大取得件数（探索空間の上限）
    pub everything_max: u32,
}

impl Default for PipelineConfig {
    fn default() -> Self {
        Self {
            astar: AstarConfig::default(),
            everything_max: 1000,
        }
    }
}

/// 2フェーズ検索パイプライン
///
/// # 型パラメータ
/// - `H`: フォルダヒューリスティック（cosine類似度を返す）
/// - `S`: ファイルスコアリング（cosine類似度を返す）
pub struct SearchPipeline<H, S> {
    config: PipelineConfig,
    heuristic: H,
    scorer: S,
}

impl<H, S> SearchPipeline<H, S>
where
    H: Fn(&std::path::Path) -> f32 + Send + Sync,
    S: Fn(&std::path::Path) -> f32 + Send + Sync,
{
    pub fn new(config: PipelineConfig, heuristic: H, scorer: S) -> Self {
        Self { config, heuristic, scorer }
    }

    /// 自然言語クエリを受け取り上位K件を返す。
    ///
    /// `callback` には A* の探索ログがリアルタイムで通知される。
    /// Tauri Channel API と接続する場合はここで `channel.send()` する。
    pub fn run(
        &self,
        query: &str,
        callback: &mut dyn ExploreCallback,
    ) -> Result<Vec<SearchResult>, PipelineError> {
        // --- Phase 1: NLP クエリ解析 ---
        let parsed = parse_query(query);
        let everything_query = parsed.to_everything_query();

        // --- Phase 2: Everything 絞り込み ---
        let paths = self.fetch_candidates(&everything_query)?;
        if paths.is_empty() {
            return Ok(vec![]);
        }

        // --- Phase 3: VirtualTree 構築 ---
        let tree = VirtualTree::from_paths(&paths);

        // --- Phase 4: A* 探索 ---
        let engine = AstarEngine::new(
            self.config.astar.clone(),
            &self.heuristic,
            &self.scorer,
        );
        let results = engine.search(&tree, callback);

        Ok(results)
    }

    /// `run` の NLP 解析結果を外部から渡すバリアント。
    ///
    /// UI 側でクエリプレビューを表示しつつ探索したい場合など、
    /// ParsedQuery を事前に計算して渡せる。
    pub fn run_parsed(
        &self,
        parsed: &ParsedQuery,
        callback: &mut dyn ExploreCallback,
    ) -> Result<Vec<SearchResult>, PipelineError> {
        let everything_query = parsed.to_everything_query();
        let paths = self.fetch_candidates(&everything_query)?;
        if paths.is_empty() {
            return Ok(vec![]);
        }
        let tree = VirtualTree::from_paths(&paths);
        let engine = AstarEngine::new(
            self.config.astar.clone(),
            &self.heuristic,
            &self.scorer,
        );
        Ok(engine.search(&tree, callback))
    }

    // ------------------------------------------------------------------
    // 内部: Everything 呼び出し（Windows only）／非Windows はスタブ
    // ------------------------------------------------------------------

    #[cfg(windows)]
    fn fetch_candidates(&self, query: &str) -> Result<Vec<PathBuf>, PipelineError> {
        use search::search as ev_search;

        // 独立した検索エポックで実行する（キャンセル機構は呼び出し元では未使用）
        let gen = search::next_generation();
        let results = ev_search(query, self.config.everything_max, search::MatchOptions::default(), gen)
            .map_err(PipelineError::Everything)?;
        Ok(results.into_iter().map(|r| PathBuf::from(r.path)).collect())
    }

    /// 非Windows（開発・テスト用）: クエリをそのまま返すスタブ。
    /// 実際のファイル探索は行わない。テストで `paths` を直接渡す場合は
    /// `run_with_paths` を使う。
    #[cfg(not(windows))]
    fn fetch_candidates(&self, _query: &str) -> Result<Vec<PathBuf>, PipelineError> {
        Ok(vec![])
    }
}

// ------------------------------------------------------------------
// テスト用: パスリストを直接渡してパイプラインを動かすヘルパー
// ------------------------------------------------------------------

/// Everything を使わずパスリストを直接渡して A* まで実行する。
/// 単体テスト・統合テストで利用する。
pub fn run_with_paths<H, S>(
    config: &AstarConfig,
    paths: &[PathBuf],
    heuristic: H,
    scorer: S,
    callback: &mut dyn ExploreCallback,
) -> Vec<SearchResult>
where
    H: Fn(&std::path::Path) -> f32,
    S: Fn(&std::path::Path) -> f32,
{
    if paths.is_empty() {
        return vec![];
    }
    let tree = VirtualTree::from_paths(paths);
    let engine = AstarEngine::new(config.clone(), heuristic, scorer);
    engine.search(&tree, callback)
}

/// NLP 解析結果（Everything クエリ文字列）を公開するユーティリティ。
/// UI 側でクエリプレビューを表示したい場合に使う。
pub fn preview_query(query: &str) -> (ParsedQuery, String) {
    let parsed = parse_query(query);
    let ev = parsed.to_everything_query();
    (parsed, ev)
}

#[cfg(test)]
mod tests {
    use super::*;
    use astar::NoopCallback;
    use std::path::{Path, PathBuf};

    fn pb(s: &str) -> PathBuf {
        PathBuf::from(s)
    }

    fn dummy_heuristic(path: &Path) -> f32 {
        if path.to_str().unwrap_or("").contains("components") {
            0.9
        } else {
            0.3
        }
    }

    fn dummy_scorer(path: &Path) -> f32 {
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if name.contains("Button") { 0.95 } else { 0.4 }
    }

    #[test]
    fn run_with_paths_returns_top_k() {
        let paths = vec![
            pb("/src/components/Button.tsx"),
            pb("/src/components/Input.tsx"),
            pb("/src/hooks/useButton.ts"),
            pb("/src/utils/format.ts"),
        ];
        let config = AstarConfig { top_k: 2, lambda: 0.0, mu: 0.0 };
        let mut cb = NoopCallback;
        let results = run_with_paths(&config, &paths, dummy_heuristic, dummy_scorer, &mut cb);
        assert_eq!(results.len(), 2);
        assert!(results[0].score >= results[1].score);
    }

    #[test]
    fn button_scores_highest() {
        let paths = vec![
            pb("/src/components/Button.tsx"),
            pb("/src/components/Input.tsx"),
            pb("/src/hooks/useButton.ts"),
        ];
        let config = AstarConfig { top_k: 3, lambda: 0.0, mu: 0.0 };
        let mut cb = NoopCallback;
        let results = run_with_paths(&config, &paths, dummy_heuristic, dummy_scorer, &mut cb);
        assert_eq!(results[0].path.file_name().unwrap(), "Button.tsx");
    }

    #[test]
    fn empty_paths_returns_empty() {
        let config = AstarConfig::default();
        let mut cb = NoopCallback;
        let results = run_with_paths(&config, &[], dummy_heuristic, dummy_scorer, &mut cb);
        assert!(results.is_empty());
    }

    #[test]
    fn preview_query_parses_japanese() {
        let (parsed, ev) = preview_query("先週のPDFファイル");
        assert_eq!(parsed.date_modified, Some("lastweek".to_string()));
        assert!(parsed.extensions.contains(&"pdf".to_string()));
        assert!(ev.contains("ext:pdf"));
        assert!(ev.contains("dm:lastweek"));
    }

    #[test]
    fn preview_query_parses_path_filter() {
        let (parsed, ev) = preview_query("src/のTypeScriptファイル");
        assert!(parsed.extensions.iter().any(|e| e == "ts" || e == "tsx"));
        assert!(ev.contains("ext:"));
    }

    #[test]
    fn callback_receives_explore_events() {
        struct EventCounter {
            dirs: usize,
            files: usize,
        }
        impl ExploreCallback for EventCounter {
            fn on_open_dir(&mut self, _: &Path, _: f32) { self.dirs += 1; }
            fn on_skip_dir(&mut self, _: &Path, _: f32) {}
            fn on_found_file(&mut self, _: &Path, _: f32) { self.files += 1; }
        }

        let paths = vec![
            pb("/root/a/x.txt"),
            pb("/root/a/y.txt"),
            pb("/root/b/z.txt"),
        ];
        let config = AstarConfig { top_k: 10, lambda: 0.0, mu: 0.0 };
        let mut cb = EventCounter { dirs: 0, files: 0 };
        let results = run_with_paths(
            &config,
            &paths,
            |_| 0.5,
            |_| 0.5,
            &mut cb,
        );
        assert_eq!(results.len(), 3);
        assert!(cb.files == 3);
        assert!(cb.dirs >= 1);
    }
}
