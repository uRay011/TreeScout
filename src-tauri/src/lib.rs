use serde::Serialize;
use tauri::{AppHandle, Emitter};

use astar::{AstarConfig, ExploreCallback};
use nlp::parse_query;
use pipeline::run_with_paths;
use search::SearchResult as EvResult;

// ── Phase 1 既存コマンド（後方互換） ────────────────────────
#[tauri::command]
fn search_files(query: String, max: Option<u32>) -> Result<Vec<EvResult>, String> {
    search::search(&query, max.unwrap_or(200)).map_err(|e| e.to_string())
}

// ── Phase 2: 2フェーズセマンティック検索 ─────────────────────

/// フロントエンドへ流すセマンティック検索の最終結果
#[derive(Debug, Serialize, Clone)]
pub struct SemanticResult {
    pub path: String,
    pub name: String,
    pub ext: String,
    pub score: f32,
}

/// A* 探索ログイベント（Tauri emit で流す）
#[derive(Debug, Serialize, Clone)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ExploreEvent {
    OpenDir { path: String, h_score: f32 },
    SkipDir { path: String, h_score: f32 },
    FoundFile { path: String, score: f32 },
}

/// Channel 経由で探索ログを emit するコールバック
struct TauriCallback {
    app: AppHandle,
    channel: String,
}

impl ExploreCallback for TauriCallback {
    fn on_open_dir(&mut self, path: &std::path::Path, h_score: f32) {
        let _ = self.app.emit(
            &self.channel,
            ExploreEvent::OpenDir {
                path: path.to_string_lossy().into_owned(),
                h_score,
            },
        );
    }
    fn on_skip_dir(&mut self, path: &std::path::Path, h_score: f32) {
        let _ = self.app.emit(
            &self.channel,
            ExploreEvent::SkipDir {
                path: path.to_string_lossy().into_owned(),
                h_score,
            },
        );
    }
    fn on_found_file(&mut self, path: &std::path::Path, score: f32) {
        let _ = self.app.emit(
            &self.channel,
            ExploreEvent::FoundFile {
                path: path.to_string_lossy().into_owned(),
                score,
            },
        );
    }
}

/// 2フェーズセマンティック検索コマンド
///
/// Phase 1: NLP 解析 → Everything 絞り込み
/// Phase 2: 仮想ツリー構築 → A* 探索
///
/// `explore_channel` に指定したイベント名で A* 探索ログをリアルタイム送信する。
/// 省略すると探索ログは送信されない（静粛モード）。
#[tauri::command]
fn semantic_search(
    app: AppHandle,
    query: String,
    top_k: Option<usize>,
    lambda: Option<f32>,
    mu: Option<f32>,
    explore_channel: Option<String>,
) -> Result<Vec<SemanticResult>, String> {
    let parsed = parse_query(&query);
    let everything_query = parsed.to_everything_query();

    // Everything 絞り込み（非 Windows は空リストを返すスタブ）
    let paths = fetch_candidates(&everything_query, 1000).map_err(|e| e.to_string())?;
    if paths.is_empty() {
        return Ok(vec![]);
    }

    let config = AstarConfig {
        top_k: top_k.unwrap_or(20),
        lambda: lambda.unwrap_or(0.1),
        mu: mu.unwrap_or(0.001),
    };

    // ヒューリスティック: フォルダ名のキーワード一致スコア（埋め込みは Phase 4 で置換予定）
    let keywords: Vec<String> = parsed.keywords.iter().map(|s| s.to_lowercase()).collect();
    let heuristic = move |path: &std::path::Path| -> f32 {
        keyword_heuristic(path, &keywords)
    };

    // スコアリング: ファイル名のキーワード一致スコア
    let keywords2: Vec<String> = parsed.keywords.iter().map(|s| s.to_lowercase()).collect();
    let extensions = parsed.extensions.clone();
    let scorer = move |path: &std::path::Path| -> f32 {
        keyword_scorer(path, &keywords2, &extensions)
    };

    let mut cb: Box<dyn ExploreCallback> = if let Some(ch) = explore_channel {
        Box::new(TauriCallback { app, channel: ch })
    } else {
        Box::new(astar::NoopCallback)
    };

    let results = run_with_paths(&config, &paths, heuristic, scorer, cb.as_mut());

    Ok(results
        .into_iter()
        .map(|r| {
            let path_str = r.path.to_string_lossy().into_owned();
            let name = r
                .path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            let ext = r
                .path
                .extension()
                .map(|e| e.to_string_lossy().into_owned())
                .unwrap_or_default();
            SemanticResult { path: path_str, name, ext, score: r.score }
        })
        .collect())
}

// ── ヒューリスティック / スコアリング（キーワードベース暫定実装） ──
//
// Phase 4 でフォルダ embedding（mmap 常駐）に置換する。
// 現時点ではキーワード出現割合で近似する。

fn keyword_heuristic(path: &std::path::Path, keywords: &[String]) -> f32 {
    if keywords.is_empty() {
        return 0.5;
    }
    let dir_name = path
        .file_name()
        .map(|n| n.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    let matched = keywords.iter().filter(|kw| dir_name.contains(kw.as_str())).count();
    0.1 + 0.9 * (matched as f32 / keywords.len() as f32)
}

fn keyword_scorer(
    path: &std::path::Path,
    keywords: &[String],
    extensions: &[String],
) -> f32 {
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().to_lowercase())
        .unwrap_or_default();

    // 拡張子ボーナス
    let ext_bonus = if !extensions.is_empty() {
        let file_ext = path
            .extension()
            .map(|e| e.to_string_lossy().to_lowercase())
            .unwrap_or_default();
        if extensions.iter().any(|e| e.as_str() == file_ext.as_str()) { 0.2 } else { 0.0 }
    } else {
        0.0
    };

    if keywords.is_empty() {
        return 0.5 + ext_bonus;
    }
    let matched = keywords.iter().filter(|kw| name.contains(kw.as_str())).count();
    let kw_score = matched as f32 / keywords.len() as f32;
    (0.1 + 0.7 * kw_score + ext_bonus).min(1.0)
}

// ── Everything 呼び出し（Windows only / 非 Windows はスタブ） ──

#[cfg(windows)]
fn fetch_candidates(
    query: &str,
    max: u32,
) -> Result<Vec<std::path::PathBuf>, SearchError> {
    let results = search::search(query, max)?;
    Ok(results.into_iter().map(|r| std::path::PathBuf::from(r.path)).collect())
}

#[cfg(not(windows))]
fn fetch_candidates(
    _query: &str,
    _max: u32,
) -> Result<Vec<std::path::PathBuf>, std::io::Error> {
    Ok(vec![])
}

// ── Tauri エントリ ──────────────────────────────────────────
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![search_files, semantic_search])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
