use std::collections::HashSet;
use std::sync::Mutex;

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};

use astar::{AstarConfig, ExploreCallback};
use nlp::parse_query;
use pipeline::{run_with_paths, scoring};
use search::{MatchOptions, SearchError, SearchResult as EvResult};

mod browse_session;
use browse_session::{browse, fetch_window, BrowseSnapshot, BrowseState};

mod folder_index;
use folder_index::{index_folders_command, FolderIndexState};

// ── Phase 1 既存コマンド（後方互換） ────────────────────────
#[tauri::command(async)]
fn search_files(query: String, max: Option<u32>, options: Option<MatchOptions>) -> Result<Vec<EvResult>, String> {
    let gen = search::next_generation();
    search::search(&query, max.unwrap_or(200), options.unwrap_or_default(), gen).map_err(|e| e.to_string())
}

// ── Phase 4: カラムUIのフォルダ展開 ──────────────────────────
//
// 中央ペインでフォルダを選択した際に直下の中身を一覧取得する。
// Everything に依存せず std::fs で読むため非Windowsでも動作する。
#[derive(Serialize)]
struct DirEntryResult {
    name: String,
    path: String,
    folder: String,
    is_dir: bool,
    ext: String,
    /// 検索クエリに対する一致スコア（0.0〜1.0）。query 未指定時は 0.0。
    /// 中央ペインの手動展開列にもヒート色を付けるために返す。
    score: f32,
}

#[tauri::command(async)]
fn list_directory(path: String, query: Option<String>) -> Result<Vec<DirEntryResult>, String> {
    // query 指定時のみキーワードを抽出してスコアリングする（未指定＝無着色）
    let (keywords, extensions): (Vec<String>, Vec<String>) = match query.filter(|q| !q.trim().is_empty()) {
        Some(q) => {
            let parsed = parse_query(&q);
            (
                parsed.keywords.iter().map(|s| s.to_lowercase()).collect(),
                parsed.extensions.clone(),
            )
        }
        None => (Vec::new(), Vec::new()),
    };

    // "C:" のようなドライブレターのみのパスは Windows ではカレントドライブの
    // 作業ディレクトリ相対パスとして解釈され、ドライブ直下ではなくCWDが返ってしまう。
    // ドライブルートを明示するため "\\" を付与して正規化する
    let normalized = if path.len() == 2 && path.ends_with(':') {
        format!("{}\\", path)
    } else {
        path.clone()
    };

    let dir = std::path::Path::new(&normalized);
    let mut entries: Vec<DirEntryResult> = std::fs::read_dir(dir)
        .map_err(|e| e.to_string())?
        .filter_map(|entry| entry.ok())
        .map(|entry| {
            let entry_path = entry.path();
            let is_dir = entry_path.is_dir();
            let ext = if is_dir {
                String::new()
            } else {
                entry_path
                    .extension()
                    .map(|e| e.to_string_lossy().into_owned())
                    .unwrap_or_default()
            };
            let score = scoring::score_path(&entry_path, &keywords, &extensions);
            DirEntryResult {
                name: entry.file_name().to_string_lossy().into_owned(),
                path: entry_path.to_string_lossy().into_owned(),
                folder: normalized.clone(),
                is_dir,
                ext,
                score,
            }
        })
        .collect();

    // フォルダを先頭、その後はファイル名（大小無視）の昇順
    entries.sort_by(|a, b| {
        b.is_dir
            .cmp(&a.is_dir)
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });

    Ok(entries)
}

// ── Phase 4: 「PC」階層のドライブ一覧 ─────────────────────────
//
// 左ペイン最上位カラムに、検索結果が0件のドライブもグレー表示するための一覧。
#[tauri::command]
fn list_drives() -> Vec<String> {
    search::list_drives()
}

// ── Phase 4: ファイルプレビュー ──────────────────────────────
//
// 検索パイプラインとは独立した経路。アイテム選択時にのみ呼ばれるため
// Everything絞り込み/A*探索の <200ms 目標には影響しない。
#[tauri::command]
async fn get_preview(path: String) -> Result<preview::PreviewResult, String> {
    preview::get_preview(std::path::Path::new(&path))
        .await
        .map_err(|e| e.to_string())
}

// ── Phase 2: 2フェーズセマンティック検索 ─────────────────────

/// フロントエンドへ流すセマンティック検索の最終結果
#[derive(Debug, Serialize, Clone)]
pub struct SemanticResult {
    pub path: String,
    pub name: String,
    pub ext: String,
    pub is_dir: bool,
    pub score: f32,
    /// Phase1候補（Everything結果）外から見つかったAIサジェストか
    pub is_suggestion: bool,
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
#[tauri::command(async)]
fn semantic_search(
    app: AppHandle,
    query: String,
    top_k: Option<usize>,
    lambda: Option<f32>,
    mu: Option<f32>,
    explore_channel: Option<String>,
    root_path: Option<String>,
    options: Option<MatchOptions>,
) -> Result<Vec<SemanticResult>, String> {
    // 新しい検索エポックを開始（進行中の旧 browse/search をキャンセルする）
    let gen = search::next_generation();

    let parsed = parse_query(&query);
    let everything_query = parsed.to_everything_query();
    let everything_query = match root_path.filter(|r| !r.is_empty()) {
        Some(root) if everything_query.is_empty() => format!("path:\"{}\"", root),
        Some(root) => format!("path:\"{}\" {}", root, everything_query),
        None => everything_query,
    };

    // Everything 絞り込み（非 Windows は空リストを返すスタブ）。
    // is_dir を保持するため PathBuf ではなくレコードのまま受け取る。
    let records = fetch_candidates(&everything_query, 1000, options.unwrap_or_default(), gen).map_err(|e| e.to_string())?;
    if records.is_empty() {
        return Ok(vec![]);
    }

    let config = AstarConfig {
        top_k: top_k.unwrap_or(20),
        lambda: lambda.unwrap_or(0.1),
        mu: mu.unwrap_or(0.001),
        ..Default::default()
    };

    // 段階的スコアラー（埋め込みは Phase 4 で置換予定）。
    // フォルダ(ヒューリスティック)・ファイル(スコアリング)とも同じ scoring::score_path を使い、
    // 左ペインのスコアと中央ペインのヒート色を一致させる。
    let keywords: Vec<String> = parsed.keywords.iter().map(|s| s.to_lowercase()).collect();
    let extensions = parsed.extensions.clone();

    let kw_h = keywords.clone();
    let ext_h = extensions.clone();
    let heuristic = move |path: &std::path::Path| -> f32 {
        scoring::score_path(path, &kw_h, &ext_h)
    };
    let kw_s = keywords.clone();
    let ext_s = extensions.clone();
    let scorer = move |path: &std::path::Path| -> f32 {
        scoring::score_path(path, &kw_s, &ext_s)
    };

    let mut cb: Box<dyn ExploreCallback> = if let Some(ch) = explore_channel {
        Box::new(TauriCallback { app, channel: ch })
    } else {
        Box::new(astar::NoopCallback)
    };

    // A* 探索は中央ペイン用の探索ログ(callback)を流すために実行する。
    // 戻り値（左ペイン）は A* 出力ではなく Everything 全候補のスコア付き一覧を使う
    // （A* はファイル到達ノードのみ結果に積むためフォルダや一部ファイルが欠落するため）。
    let paths: Vec<std::path::PathBuf> =
        records.iter().map(|r| std::path::PathBuf::from(&r.path)).collect();
    let astar_results = run_with_paths(&config, &paths, heuristic, scorer, cb.as_mut());

    // 左ペイン: 全候補をスコア付けし降順ソートして返す
    let phase1_paths: HashSet<String> = records.iter().map(|r| r.path.clone()).collect();

    let mut results: Vec<SemanticResult> = records
        .into_iter()
        .map(|r| {
            let score = scoring::score_path(std::path::Path::new(&r.path), &keywords, &extensions);
            SemanticResult {
                path: r.path,
                name: r.name,
                ext: r.ext,
                is_dir: r.is_dir,
                score,
                is_suggestion: false,
            }
        })
        .collect();
    results.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));

    // AIサジェスト: Phase1候補外（folders.bin由来の探索）から見つかった高スコアファイルを
    // Phase1パスとdedupした上で末尾に追加する
    results.extend(astar_results.into_iter().filter(|r| !r.in_phase1).filter_map(|r| {
        let path = r.path.to_string_lossy().into_owned();
        if phase1_paths.contains(&path) {
            return None;
        }
        let name = r.path.file_name()?.to_string_lossy().into_owned();
        let ext = r.path.extension().map(|e| e.to_string_lossy().into_owned()).unwrap_or_default();
        Some(SemanticResult {
            path,
            name,
            ext,
            is_dir: false,
            score: r.score,
            is_suggestion: true,
        })
    }));

    Ok(results)
}

// ── Everything 呼び出し（Windows only / 非 Windows はスタブ） ──
//
// is_dir を保持するため Everything のレコード（EvResult）をそのまま返す。

#[cfg(windows)]
fn fetch_candidates(query: &str, max: u32, options: MatchOptions, gen: u64) -> Result<Vec<EvResult>, SearchError> {
    search::search(query, max, options, gen)
}

#[cfg(not(windows))]
fn fetch_candidates(_query: &str, _max: u32, _options: MatchOptions, _gen: u64) -> Result<Vec<EvResult>, SearchError> {
    Ok(vec![])
}

// ── Tauri エントリ ──────────────────────────────────────────
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .setup(|app| {
            let state = FolderIndexState::init(app.handle()).map_err(std::io::Error::other)?;
            app.manage(state);
            app.manage(BrowseState(Mutex::new(BrowseSnapshot::default())));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            search_files,
            semantic_search,
            list_drives,
            get_preview,
            index_folders_command,
            list_directory,
            browse,
            fetch_window
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
