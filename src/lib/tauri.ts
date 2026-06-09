import { invoke } from "@tauri-apps/api/core";

export interface SearchResult {
  name: string;
  path: string;
  folder: string;
  is_dir: boolean;
  ext: string;
  // Everything SDK は現状 size/modified を返さない（Phase2以降で追加予定）
  size: number;
  modified: string;
}

export async function searchFiles(query: string, max = 200): Promise<SearchResult[]> {
  return invoke<SearchResult[]>("search_files", { query, max });
}
