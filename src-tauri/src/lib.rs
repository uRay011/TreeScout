use std::collections::HashSet;
use std::sync::Mutex;

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};

use astar::{AstarConfig, ExploreCallback};
use embedding::Embedder;
use nlp::parse_query;
use pipeline::{run_with_paths, scoring};
use search::{MatchOptions, SearchError, SearchResult as EvResult};

mod browse_session;
use browse_session::{browse, fetch_window, BrowseSnapshot, BrowseState};

mod folder_index;
use folder_index::{index_folders_command, FolderIndexState};

/// AIサジェスト探索: folders.bin から取得するクエリ近傍フォルダの最大件数
const EXTRA_FOLDER_TOP_N: usize = 20;
/// AIサジェスト探索: 近傍フォルダとして採用するcosine類似度の下限
const EXTRA_FOLDER_MIN_SIMILARITY: f32 = 0.3;

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
    pub size: u64,
    pub modified: String,
}

/// A* 探索ログイベント（Tauri emit で流す）
#[derive(Debug, Serialize, Clone)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ExploreEvent {
    OpenDir { path: String, h_score: f32 },
    SkipDir { path: String, h_score: f32 },
    FoundFile { path: String, score: f32 },
}

/// 探索ログのコアレス間隔（CLAUDE.md記載の方針に合わせ ~16ms 単位でまとめて送る）
const EXPLORE_EVENT_COALESCE: std::time::Duration = std::time::Duration::from_millis(16);

/// Channel 経由で探索ログをバッチ emit するコールバック。
///
/// ノードごとに `app.emit()` すると、JSON シリアライズ＋WebView2へのIPC往復が
/// A* の探索ループ自体を支配するコストになる（広域クエリでは数千〜数万ノード visit）。
/// バッファに溜め、~16ms ごと（および探索終了時の Drop）にまとめて1イベントとして送る。
struct TauriCallback {
    app: AppHandle,
    channel: String,
    buffer: Vec<ExploreEvent>,
    last_emit: std::time::Instant,
}

impl TauriCallback {
    fn new(app: AppHandle, channel: String) -> Self {
        Self {
            app,
            channel,
            buffer: Vec::new(),
            last_emit: std::time::Instant::now(),
        }
    }

    fn push(&mut self, ev: ExploreEvent) {
        self.buffer.push(ev);
        if self.last_emit.elapsed() >= EXPLORE_EVENT_COALESCE {
            self.flush();
        }
    }

    fn flush(&mut self) {
        if self.buffer.is_empty() {
            return;
        }
        let batch = std::mem::take(&mut self.buffer);
        let _ = self.app.emit(&self.channel, batch);
        self.last_emit = std::time::Instant::now();
    }
}

impl Drop for TauriCallback {
    fn drop(&mut self) {
        self.flush();
    }
}

impl ExploreCallback for TauriCallback {
    fn on_open_dir(&mut self, path: &std::path::Path, h_score: f32) {
        self.push(ExploreEvent::OpenDir {
            path: path.to_string_lossy().into_owned(),
            h_score,
        });
    }
    fn on_skip_dir(&mut self, path: &std::path::Path, h_score: f32) {
        self.push(ExploreEvent::SkipDir {
            path: path.to_string_lossy().into_owned(),
            h_score,
        });
    }
    fn on_found_file(&mut self, path: &std::path::Path, score: f32) {
        self.push(ExploreEvent::FoundFile {
            path: path.to_string_lossy().into_owned(),
            score,
        });
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
    state: tauri::State<FolderIndexState>,
    query: String,
    top_k: Option<usize>,
    lambda: Option<f32>,
    mu: Option<f32>,
    explore_channel: Option<String>,
    root_path: Option<String>,
    options: Option<MatchOptions>,
) -> Result<Vec<SemanticResult>, String> {
    // 切り分け用計測: フロント側の「完了」表示までの時間とRust側計算時間の差分が
    // 大きい場合、フロントの再描画/レンダリングがボトルネックであることの裏付けになる
    let t_total = std::time::Instant::now();

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
        Box::new(TauriCallback::new(app, ch))
    } else {
        Box::new(astar::NoopCallback)
    };

    // AIサジェスト探索対象: クエリembeddingに近いフォルダ（folders.bin）をTop-Nで選び、
    // Phase1ツリー外の兄弟/親ディレクトリとして A* に開放する。
    // folders.bin 未構築（初回起動など）・クエリが空・埋め込みモデルロード中の場合は
    // 空のまま続行する。
    let extra_folders: Vec<std::path::PathBuf> = if query.trim().is_empty() {
        vec![]
    } else if let Some(embedder) = state.embedder() {
        let query_emb = embedder.embed_one(&query);
        let query_i8 = embedding::quantize_f32_to_i8(&query_emb);
        state
            .load_matrix()
            .map(|matrix| {
                matrix
                    .nearest(&query_i8, EXTRA_FOLDER_TOP_N)
                    .into_iter()
                    .filter(|(_, score)| *score > EXTRA_FOLDER_MIN_SIMILARITY)
                    .map(|(path, _)| std::path::PathBuf::from(path))
                    .collect()
            })
            .unwrap_or_default()
    } else {
        vec![]
    };

    // A* 探索は中央ペイン用の探索ログ(callback)を流すために実行する。
    // 戻り値（左ペイン）は A* 出力ではなく Everything 全候補のスコア付き一覧を使う
    // （A* はファイル到達ノードのみ結果に積むためフォルダや一部ファイルが欠落するため）。
    let paths: Vec<std::path::PathBuf> =
        records.iter().map(|r| std::path::PathBuf::from(&r.path)).collect();
    let candidate_count = paths.len();
    let extra_folder_count = extra_folders.len();
    let t_astar = std::time::Instant::now();
    let astar_results = run_with_paths(&config, &paths, &extra_folders, heuristic, scorer, cb.as_mut());
    let astar_ms = t_astar.elapsed().as_secs_f64() * 1000.0;

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
                size: r.size,
                modified: search::format_mtime(r.mtime),
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
        let (size, modified) = fs_size_and_mtime(&r.path);
        Some(SemanticResult {
            path,
            name,
            ext,
            is_dir: false,
            score: r.score,
            is_suggestion: true,
            size,
            modified,
        })
    }));

    eprintln!(
        "[semantic_search] query={:?} candidates={} extra_folders={} astar={:.1}ms total={:.1}ms",
        query, candidate_count, extra_folder_count, astar_ms, t_total.elapsed().as_secs_f64() * 1000.0,
    );

    Ok(results)
}

/// AIサジェスト（folders.bin由来でEverythingレコードを持たない）のサイズ・更新日時を
/// ファイルシステムから取得する。取得失敗時は 0 / 空文字を返す。
fn fs_size_and_mtime(path: &std::path::Path) -> (u64, String) {
    match std::fs::metadata(path) {
        Ok(meta) => {
            let modified = meta
                .modified()
                .ok()
                .map(|t| search::format_mtime(systemtime_to_filetime(t)))
                .unwrap_or_default();
            (meta.len(), modified)
        }
        Err(_) => (0, String::new()),
    }
}

/// `std::time::SystemTime`(Unix epoch) を Windows FILETIME(100ns, 1601-01-01起点) へ変換する。
/// `format_mtime` に渡すための橋渡し
fn systemtime_to_filetime(t: std::time::SystemTime) -> i64 {
    const UNIX_EPOCH_AS_FILETIME: i64 = 11_644_473_600 * 10_000_000;
    match t.duration_since(std::time::UNIX_EPOCH) {
        Ok(d) => UNIX_EPOCH_AS_FILETIME + d.as_secs() as i64 * 10_000_000 + d.subsec_nanos() as i64 / 100,
        Err(e) => {
            let d = e.duration();
            UNIX_EPOCH_AS_FILETIME - d.as_secs() as i64 * 10_000_000 - d.subsec_nanos() as i64 / 100
        }
    }
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
