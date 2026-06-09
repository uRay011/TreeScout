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
}

export async function searchFiles(query: string, max = 200): Promise<SearchResult[]> {
  return invoke<SearchResult[]>("search_files", { query, max });
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

export interface SemanticSearchOptions {
  topK?: number;
  lambda?: number;
  mu?: number;
  /** 探索ログを受け取るコールバック。省略すると無効 */
  onExplore?: (event: ExploreEvent) => void;
}

/**
 * 2フェーズセマンティック検索。
 * NLP 解析 → Everything 絞り込み → 仮想ツリー → A* 探索。
 */
export async function semanticSearch(
  query: string,
  options: SemanticSearchOptions = {},
): Promise<SemanticResult[]> {
  const { topK, lambda, mu, onExplore } = options;

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
    });
  } finally {
    unlisten?.();
  }
}
