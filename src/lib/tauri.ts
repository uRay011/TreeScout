import { invoke } from "@tauri-apps/api/core";

export interface SearchResult {
  name: string;
  path: string;
  folder: string;
  size: number;
  modified: string;
  ext: string;
}

export interface SearchResponse {
  results: SearchResult[];
  total: number;
  elapsed_ms: number;
}

export async function searchFiles(query: string): Promise<SearchResponse> {
  return invoke<SearchResponse>("search_files", { query });
}
