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
  /** Everything候補外からA*が発見したAIサジェストか（true: サジェスト / Phase1結果では未設定） */
  is_suggestion?: boolean;
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
  /** 【調査用】0件時の診断情報。原因判明後に削除する。 */
  debug?: string;
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

/**
 * 利用可能な論理ドライブのルートパス一覧を取得する（例: ["C:\\", "D:\\", "E:\\"]）。
 * 左ペイン「PC」階層に、検索結果が0件のドライブもグレー表示するために使う。
 */
export async function listDrives(): Promise<string[]> {
  return invoke<string[]>("list_drives");
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
  is_suggestion: boolean;
  size: number;
  modified: string;
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
  /** このカラムが表示しているフォルダの実パス（fillMissingEntries で全件補完する際に使う）。
   *  rootPath未指定時のcol-0（仮想的な「PC」）は undefined */
  dirPath?: string;
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

function dirname(path: string): string {
  return path.replace(/[\\/][^\\/]+[\\/]*$/, "");
}

/**
 * カラムのヘッダーラベル（=このカラムが表示しているフォルダの名前）を求める。
 * ドライブルート（"E:" / "E:\"）は「Eドライブ」と表示する。
 */
function formatDirLabel(path: string): string {
  const trimmed = path.replace(/[\\/]+$/, "");
  const driveMatch = trimmed.match(/^([A-Za-z]):$/);
  if (driveMatch) return `${driveMatch[1].toUpperCase()}ドライブ`;
  return basename(path) || path;
}

/** dirname() や rootPath/エントリパスの末尾区切り文字（ドライブルート "C:\" 等）を揃えて
 *  byParent のキーが一致するようにする */
function normalizeDir(path: string): string {
  return path.replace(/[\\/]+$/, "") || path;
}

/**
 * ExploreEvent の系列から探索型カラムUI用の AstarColumn[] を構築する。
 *
 * まずイベントを (カラムインデックス, 親フォルダパス) ごとにグルーピングする。
 * カラムNの内容は「1つ左のカラムで最もスコアの高いフォルダ（=展開先）の子」だけに限定し、
 * 探索順序が先だった兄弟フォルダ（例: 別年度のフォルダ）のイベントが混入しないようにする。
 *
 * col-0 はルート未指定時は仮想的な「PC」（ドライブ一覧）、ルート指定時はルートフォルダ自身
 * （中身は col-1 に並ぶ）。col-1 以降は、1つ左のカラムで is_dir のエントリのうち
 * スコア最大のものを「展開先」として選び、その子をエントリにする。
 *
 * `allDrives` を渡すと（rootPath未指定時のみ）col-0に、探索で一度も触れられていない
 * ドライブも非ヒット（skipped）として追加表示する。
 */
export function buildColumnsFromEvents(events: ExploreEvent[], rootPath?: string, allDrives?: string[]): AstarColumn[] {
  let baseDepth: number | null = rootPath ? pathDepth(rootPath) : null;

  const allByIdx = new Map<number, AstarEntry[]>();
  const byParent = new Map<string, AstarEntry[]>();

  const upsert = (arr: AstarEntry[], entry: AstarEntry) => {
    // 同一パスの再探索（h_score更新）は既存行を上書きし、重複行を防ぐ
    const existing = arr.findIndex(e => e.path === entry.path);
    if (existing >= 0) arr[existing] = entry;
    else arr.push(entry);
  };

  for (const ev of events) {
    const depth = pathDepth(ev.path);
    if (baseDepth === null) baseDepth = depth;
    const idx = depth - baseDepth;
    if (idx < 0) continue; // ルートより上位階層は非表示

    const entry: AstarEntry = ev.type === "found_file"
      ? { path: ev.path, name: basename(ev.path), ext: extname(ev.path), is_dir: false, score: ev.score, kind: "found" }
      : { path: ev.path, name: basename(ev.path), ext: "", is_dir: true, score: ev.h_score, kind: ev.type === "open_dir" ? "opened" : "skipped" };

    if (!allByIdx.has(idx)) allByIdx.set(idx, []);
    upsert(allByIdx.get(idx)!, entry);

    const parentKey = `${idx} ${normalizeDir(dirname(ev.path))}`;
    if (!byParent.has(parentKey)) byParent.set(parentKey, []);
    upsert(byParent.get(parentKey)!, entry);
  }

  const columns: AstarColumn[] = [];
  if (rootPath) {
    columns.push({
      id: "col-0",
      label: basename(rootPath) || rootPath,
      entries: byParent.get(`0 ${normalizeDir(rootPath)}`) ?? [],
      activeEntryPath: null,
      dirPath: rootPath,
    });
  } else if (allByIdx.size > 0 || (allDrives && allDrives.length > 0)) {
    const entries = [...(allByIdx.get(0) ?? [])];
    if (allDrives && allDrives.length > 0) {
      const present = new Set(entries.map(e => basename(e.path).toUpperCase()));
      for (const drive of allDrives) {
        const name = basename(drive) || drive;
        if (present.has(name.toUpperCase())) continue;
        entries.push({ path: drive, name, ext: "", is_dir: true, score: 0, kind: "skipped" });
      }
      entries.sort((a, b) => a.name.localeCompare(b.name));
    }
    columns.push({ id: "col-0", label: "PC", entries, activeEntryPath: null });
  } else {
    return [];
  }

  // 1つ左のカラムで最もスコアの高いフォルダを「展開先」として選び、その子を次カラムにする
  let idx = 0;
  while (allByIdx.has(idx + 1)) {
    let dirPath: string;
    if (idx === 0 && rootPath) {
      dirPath = rootPath;
    } else {
      const best = columns[idx].entries
        .filter(e => e.is_dir)
        .reduce<AstarEntry | null>((b, e) => (b === null || e.score > b.score) ? e : b, null);
      if (!best) break;
      dirPath = best.path;
      columns[idx].activeEntryPath = best.path;
    }
    columns.push({
      id: `col-${idx + 1}`,
      label: formatDirLabel(dirPath),
      entries: byParent.get(`${idx + 1} ${normalizeDir(dirPath)}`) ?? [],
      activeEntryPath: null,
      dirPath,
    });
    idx++;
  }

  // 最終カラム: 最もスコアの高いエントリ（found/opened/skipped問わず）をAIの注目先として強調する
  const lastCol = columns[columns.length - 1];
  if (lastCol.activeEntryPath === null && lastCol.entries.length > 0) {
    const best = lastCol.entries.reduce<AstarEntry | null>((b, e) => (b === null || e.score > b.score) ? e : b, null);
    if (best) lastCol.activeEntryPath = best.path;
  }

  return columns;
}

/**
 * 各カラムの `dirPath`（=実フォルダパス）を listDirectory で取得し、A*探索で
 * 一度も触れられなかったエントリを非ヒット（skipped）として補完する。
 *
 * 探索ログには「ヒットへの経路上のフォルダ／ファイル」しか登場しないため、
 * 何もしないとそのフォルダの他の中身（例: Eドライブ直下のDocument以外のフォルダ）が
 * 一切表示されない。検索完了後に1回だけ呼び、全件表示に補完する。
 */
export async function fillMissingEntries(columns: AstarColumn[], query?: string): Promise<AstarColumn[]> {
  return Promise.all(
    columns.map(async (col) => {
      if (!col.dirPath) return col;
      try {
        const children = await listDirectory(col.dirPath, query);
        const present = new Set(col.entries.map(e => e.path));
        const extra: AstarEntry[] = children
          .filter(c => !present.has(c.path))
          .map(c => ({ path: c.path, name: c.name, ext: c.ext, is_dir: c.is_dir, score: 0, kind: "skipped" }));
        if (extra.length === 0) return col;
        const entries = [...col.entries, ...extra];
        entries.sort((a, b) => (b.is_dir ? 1 : 0) - (a.is_dir ? 1 : 0) || a.name.localeCompare(b.name));
        return { ...col, entries };
      } catch {
        return col;
      }
    }),
  );
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
