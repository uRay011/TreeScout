import { invoke } from "@tauri-apps/api/core";
import { listen, UnlistenFn } from "@tauri-apps/api/event";

// Phase 1 既存型（後方互換）
export interface SearchResult {
  name: string;
  path: string;
  folder: string;
  is_dir: boolean;
  ext: string;
  size: number;
  modified: string;
  /** セマンティック検索の一致スコア（0.0〜1.0）。Phase1検索結果では未設定 */
  score?: number;
}

// 検索メニューのトグル（大文字小文字の区別 / 単語に完全一致 / フォルダ名にマッチ）。
// Rust側 search::MatchOptions と同形（camelCaseでシリアライズ）。
export interface MatchOptions {
  caseSensitive?: boolean;
  wholeWord?: boolean;
  matchPath?: boolean;
}

export async function searchFiles(query: string, max = 200, options?: MatchOptions): Promise<SearchResult[]> {
  return invoke<SearchResult[]>("search_files", { query, max, options });
}

// ── バックエンド窓取得（Everything 全件をRust側に常駐させ、可視窓だけ転送） ──
//
// 数十万件を1回のinvokeで全件転送すると JSON.parse がメインスレッドを止めるため、
// browse() で全件をソートしてバックエンドに常駐させ、件数だけ受け取る。
// 実際の行データは fetchWindow() で可視範囲＋先読み分だけ取得する（Everything の
// LVS_OWNERDATA owner-data ListView と同じモデル）。
export type BrowseSortCol = "name" | "folder" | "size" | "date";
export interface BrowseSort {
  col: BrowseSortCol;
  asc: boolean;
}

/** 全件抽出中の進捗（バックエンドの BrowseProgress に対応）。 */
export interface BrowseProgress {
  count: number;
  elapsed_ms: number;
}

/**
 * browse の結果。総件数と、そのスナップショットを常駐させた検索世代。
 * 並行 browse 時、フロントは generation が最大のものだけを採用し、
 * fetchWindow にも同じ generation を渡して世代の食い違いを防ぐ。
 */
export interface BrowseResult {
  total: number;
  generation: number;
}

/**
 * Everything で query を実行し、sort 済み全件をバックエンドに常駐させて
 * 総件数と確定世代を返す。
 *
 * `onProgress` を渡すと、抽出中の件数・経過msを ~100ms 間隔で受け取れる
 * （全ドライブ等の大量件数時に進捗表示するため）。
 */
export async function browse(
  query: string,
  sort: BrowseSort,
  options?: MatchOptions,
  onProgress?: (p: BrowseProgress) => void,
): Promise<BrowseResult> {
  let unlisten: UnlistenFn | undefined;
  const channel = onProgress ? `treescout://browse-progress/${Date.now()}` : undefined;
  if (channel && onProgress) {
    unlisten = await listen<BrowseProgress>(channel, (ev) => onProgress(ev.payload));
  }
  try {
    return await invoke<BrowseResult>("browse", { query, sort, options, progressChannel: channel ?? null });
  } finally {
    unlisten?.();
  }
}

/**
 * 常駐スナップショットの [offset, offset+limit) を SearchResult 形で取得する。
 * `generation` は browse が返した世代。スナップショットが別世代に差し替わっていれば空配列。
 */
export async function fetchWindow(offset: number, limit: number, generation: number): Promise<SearchResult[]> {
  return invoke<SearchResult[]>("fetch_window", { offset, limit, generation });
}

// Phase 4: カラムUIのフォルダ展開
export interface DirEntry {
  name: string;
  path: string;
  folder: string;
  is_dir: boolean;
  ext: string;
  /** 検索クエリへの一致スコア（0.0〜1.0）。query 未指定時は 0.0 */
  score: number;
}

/**
 * `path` 直下のエントリ一覧を取得する（非再帰）。
 * `query` を渡すと各エントリにヒート色用のスコアが付与される。
 */
export async function listDirectory(path: string, query?: string): Promise<DirEntry[]> {
  return invoke<DirEntry[]>("list_directory", { path, query: query || null });
}

// Phase 4: フォルダembedding事前インデックス
export interface FolderIndexResult {
  updated: number;
  skipped: number;
  scanned: number;
  removed: number;
  matrix_len: number;
}

/** `root` 配下のフォルダ embedding を差分更新し、mmap行列を再構築する */
export async function indexFolders(root: string): Promise<FolderIndexResult> {
  return invoke<FolderIndexResult>("index_folders_command", { root });
}

// Phase 2: セマンティック検索
export interface SemanticResult {
  path: string;
  name: string;
  ext: string;
  is_dir: boolean;
  score: number;
}

export type ExploreEvent =
  | { type: "open_dir"; path: string; h_score: number }
  | { type: "skip_dir"; path: string; h_score: number }
  | { type: "found_file"; path: string; score: number };

// Phase 4: 探索型カラムUI
/** カラム1件分のエントリ（A*探索ログを表示用に変換したもの） */
export interface AstarEntry {
  path: string;
  name: string;
  ext: string;
  is_dir: boolean;
  /** スコア（h_score または最終スコア）。ヒートマップの輝度に使用 */
  score: number;
  /** found: 検索結果ファイル / skipped: 探索スキップ / opened: 探索済みフォルダ */
  kind: "found" | "skipped" | "opened";
}

/** カラムUIの1カラム分（フォルダ階層に対応） */
export interface AstarColumn {
  id: string;
  label: string;
  entries: AstarEntry[];
  activeEntryPath: string | null;
}

export function basename(path: string): string {
  return path.replace(/[\\/]+$/, "").split(/[\\/]/).pop() ?? path;
}

function extname(path: string): string {
  const name = basename(path);
  const i = name.lastIndexOf(".");
  return i > 0 ? name.slice(i + 1) : "";
}

function pathDepth(path: string): number {
  return path.replace(/[\\/]+$/, "").split(/[\\/]/).length;
}

/**
 * ExploreEvent の系列から探索型カラムUI用の AstarColumn[] を構築する。
 * パスの深さをカラムインデックスとして割り当て、open_dir/skip_dir はそのフォルダ自身を
 * 親カラムのエントリとして、found_file は最も深いカラムへ確定エントリとして追加する。
 *
 * rootPath 指定時は一番左のカラムをルートフォルダ自身に固定し、
 * ルートより上位階層のフォルダ（祖先ディレクトリ）のイベントは非表示にする。
 */
export function buildColumnsFromEvents(events: ExploreEvent[], rootPath?: string): AstarColumn[] {
  const columns: AstarColumn[] = [];
  let baseDepth: number | null = null;

  if (rootPath) {
    baseDepth = pathDepth(rootPath);
    columns.push({ id: "col-0", label: basename(rootPath) || rootPath, entries: [], activeEntryPath: null });
  }

  const ensureColumn = (depth: number, label: string): AstarColumn | null => {
    if (baseDepth === null) baseDepth = depth;
    const idx = depth - baseDepth;
    if (idx < 0) return null; // ルートより上位階層は非表示
    while (columns.length <= idx) {
      columns.push({ id: `col-${columns.length}`, label: "", entries: [], activeEntryPath: null });
    }
    // rootPath未指定時のcol-0はドライブ一覧（C:, D:...が並ぶ）なので、
    // 最初に見つかったドライブ名をラベルにしてしまうと他のドライブと矛盾する
    if (!columns[idx].label) columns[idx].label = idx === 0 && !rootPath ? "PC" : label;
    return columns[idx];
  };

  for (const ev of events) {
    switch (ev.type) {
      case "open_dir":
      case "skip_dir": {
        const depth = pathDepth(ev.path);
        const col = ensureColumn(depth, basename(ev.path) || ev.path);
        if (!col) break;
        const entry: AstarEntry = {
          path: ev.path,
          name: basename(ev.path),
          ext: "",
          is_dir: true,
          score: ev.h_score,
          kind: ev.type === "open_dir" ? "opened" : "skipped",
        };
        // 同一パスの再探索（h_score更新）は既存行を上書きし、重複行を防ぐ
        const existing = col.entries.findIndex(e => e.path === ev.path);
        if (existing >= 0) col.entries[existing] = entry;
        else col.entries.push(entry);
        break;
      }
      case "found_file": {
        const depth = pathDepth(ev.path);
        const parentLabel = basename(ev.path).replace(/[\\/][^\\/]+$/, "") || "結果";
        const col = ensureColumn(depth, parentLabel);
        if (!col) break;
        const entry: AstarEntry = {
          path: ev.path,
          name: basename(ev.path),
          ext: extname(ev.path),
          is_dir: false,
          score: ev.score,
          kind: "found",
        };
        const existing = col.entries.findIndex(e => e.path === ev.path);
        if (existing >= 0) col.entries[existing] = entry;
        else col.entries.push(entry);
        col.activeEntryPath = ev.path;
        break;
      }
    }
  }

  return columns;
}

export interface SemanticSearchOptions {
  topK?: number;
  lambda?: number;
  mu?: number;
  /** 探索ログを受け取るコールバック。省略すると無効 */
  onExplore?: (event: ExploreEvent) => void;
  /** 探索のルートフォルダ（Everything の path: フィルタに渡す） */
  rootPath?: string;
  /** 検索メニューのマッチオプション（大文字小文字の区別 / 単語に完全一致 / フォルダ名にマッチ） */
  matchOptions?: MatchOptions;
}

/**
 * 2フェーズセマンティック検索。
 * NLP 解析 → Everything 絞り込み → 仮想ツリー → A* 探索。
 */
export async function semanticSearch(
  query: string,
  options: SemanticSearchOptions = {},
): Promise<SemanticResult[]> {
  const { topK, lambda, mu, onExplore, rootPath, matchOptions } = options;

  let unlisten: UnlistenFn | undefined;
  const channel = onExplore ? `treescout://explore/${Date.now()}` : undefined;

  if (channel && onExplore) {
    unlisten = await listen<ExploreEvent>(channel, (ev) => onExplore(ev.payload));
  }

  try {
    return await invoke<SemanticResult[]>("semantic_search", {
      query,
      topK: topK ?? null,
      lambda: lambda ?? null,
      mu: mu ?? null,
      exploreChannel: channel ?? null,
      rootPath: rootPath || null,
      options: matchOptions ?? null,
    });
  } finally {
    unlisten?.();
  }
}

// Phase 4: ファイルプレビュー（crates/preview の PreviewResult に対応）
export type PreviewResult =
  | { kind: "text"; content: string; truncated: boolean }
  | { kind: "markdown"; content: string; truncated: boolean }
  | { kind: "image" }
  | { kind: "unsupported" };

/** `path` のプレビューを取得する。テキスト/Markdownは先頭64KB、画像は種別判定のみ。 */
export async function getPreview(path: string): Promise<PreviewResult> {
  return invoke<PreviewResult>("get_preview", { path });
}
