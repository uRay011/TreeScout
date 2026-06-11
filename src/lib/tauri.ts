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

export async function searchFiles(query: string, max = 200): Promise<SearchResult[]> {
  return invoke<SearchResult[]>("search_files", { query, max });
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
 */
export function buildColumnsFromEvents(events: ExploreEvent[]): AstarColumn[] {
  const columns: AstarColumn[] = [];
  let baseDepth: number | null = null;

  const ensureColumn = (depth: number, label: string): AstarColumn => {
    if (baseDepth === null) baseDepth = depth;
    const idx = depth - baseDepth;
    while (columns.length <= idx) {
      columns.push({ id: `col-${columns.length}`, label: "", entries: [], activeEntryPath: null });
    }
    if (!columns[idx].label) columns[idx].label = label;
    return columns[idx];
  };

  for (const ev of events) {
    switch (ev.type) {
      case "open_dir":
      case "skip_dir": {
        const depth = pathDepth(ev.path);
        const col = ensureColumn(depth, basename(ev.path) || ev.path);
        col.entries.push({
          path: ev.path,
          name: basename(ev.path),
          ext: "",
          is_dir: true,
          score: ev.h_score,
          kind: ev.type === "open_dir" ? "opened" : "skipped",
        });
        break;
      }
      case "found_file": {
        const depth = pathDepth(ev.path);
        const parentLabel = basename(ev.path).replace(/[\\/][^\\/]+$/, "") || "結果";
        const col = ensureColumn(depth, parentLabel);
        col.entries.push({
          path: ev.path,
          name: basename(ev.path),
          ext: extname(ev.path),
          is_dir: false,
          score: ev.score,
          kind: "found",
        });
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
}

/**
 * 2フェーズセマンティック検索。
 * NLP 解析 → Everything 絞り込み → 仮想ツリー → A* 探索。
 */
export async function semanticSearch(
  query: string,
  options: SemanticSearchOptions = {},
): Promise<SemanticResult[]> {
  const { topK, lambda, mu, onExplore, rootPath } = options;

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
