import { useCallback, useRef, useState } from "react";
import { browse, fetchWindow, BrowseSort, MatchOptions, SearchResult } from "../lib/tauri";

// 可視範囲先読み取得のデバウンス間隔（ms）。連続スクロール中の過剰fetchWindow呼び出しを抑える
const ENSURE_DEBOUNCE_MS = 24;
// runBrowse直後に即時prefetchする先頭窓の件数
const INITIAL_PREFETCH = 100;

export interface BrowseWindow {
  /** browse() で確定した総件数 */
  total: number;
  /** index に対応する行を取得する（未取得ならundefined） */
  getRow: (i: number) => SearchResult | undefined;
  /** [lo, hi] の範囲を非同期で取得・キャッシュする（デバウンス済み） */
  ensureRange: (lo: number, hi: number) => void;
  /** クエリ・ソートでバックエンドに全件を常駐させ、総件数を取得して返す */
  runBrowse: (query: string, sort: BrowseSort, options?: MatchOptions) => Promise<number>;
}

/**
 * バックエンド窓取得（browse/fetch_window）用フック。
 * 可視範囲＋先読み分だけを非同期取得してキャッシュし、tickで再描画を起こす。
 * stale-generationガードにより、古いrunBrowse/ensureRangeの遅延結果は破棄する。
 */
export function useBrowseWindow(): BrowseWindow {
  const [total, setTotal] = useState(0);
  // 再描画トリガー（cacheRef更新を反映させるため）
  const [, setTick] = useState(0);
  const tick = useCallback(() => setTick((t) => t + 1), []);

  const cacheRef = useRef<Map<number, SearchResult>>(new Map());
  const totalRef = useRef(0);
  const genRef = useRef(0);
  const debounceRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  // ensureRangeのデバウンス中に保持する最新範囲（最新範囲のみ処理すれば十分）
  const pendingRangeRef = useRef<{ lo: number; hi: number } | null>(null);
  // fetchWindow実行中フラグ（多重fetch防止。完了後にpendingがあれば再度処理）
  const fetchingRef = useRef(false);

  const getRow = useCallback((i: number): SearchResult | undefined => {
    return cacheRef.current.get(i);
  }, []);

  // pendingRangeRef の範囲のうち未キャッシュのindexをまとめてfetchWindowで取得する
  const flushEnsure = useCallback(() => {
    const range = pendingRangeRef.current;
    pendingRangeRef.current = null;
    if (!range) return;

    const { lo, hi } = range;
    const cache = cacheRef.current;

    // 未キャッシュのindexが存在するかチェックしつつ、min/maxを求める
    let minMissing = -1;
    let maxMissing = -1;
    for (let i = lo; i <= hi; i++) {
      if (!cache.has(i)) {
        if (minMissing === -1) minMissing = i;
        maxMissing = i;
      }
    }
    if (minMissing === -1) return; // 全てキャッシュ済み

    const gen = genRef.current;
    const offset = minMissing;
    const limit = maxMissing - minMissing + 1;
    fetchingRef.current = true;

    fetchWindow(offset, limit)
      .then((rows) => {
        if (gen !== genRef.current) return; // stale
        rows.forEach((row, idx) => {
          cache.set(offset + idx, row);
        });
        tick();
      })
      .catch(() => {
        // 取得失敗時はキャッシュせず放置（再度ensureRangeされれば再取得を試みる）
      })
      .finally(() => {
        fetchingRef.current = false;
        // デバウンス中に新しい範囲が積まれていれば続けて処理する
        if (pendingRangeRef.current) flushEnsure();
      });
  }, [tick]);

  const ensureRange = useCallback((lo: number, hi: number) => {
    const total_ = totalRef.current;
    const clampedLo = Math.max(0, Math.min(lo, hi));
    const clampedHi = Math.min(total_ - 1, Math.max(lo, hi));
    if (total_ <= 0 || clampedLo > clampedHi) return;

    // すでに全てキャッシュ済みなら何もしない
    const cache = cacheRef.current;
    let hasMissing = false;
    for (let i = clampedLo; i <= clampedHi; i++) {
      if (!cache.has(i)) { hasMissing = true; break; }
    }
    if (!hasMissing) return;

    pendingRangeRef.current = { lo: clampedLo, hi: clampedHi };

    if (debounceRef.current) clearTimeout(debounceRef.current);
    debounceRef.current = setTimeout(() => {
      debounceRef.current = null;
      if (!fetchingRef.current) flushEnsure();
    }, ENSURE_DEBOUNCE_MS);
  }, [flushEnsure]);

  const runBrowse = useCallback(async (query: string, sort: BrowseSort, options?: MatchOptions): Promise<number> => {
    const gen = ++genRef.current;

    // 進行中のデバウンス・キャッシュをリセット
    if (debounceRef.current) {
      clearTimeout(debounceRef.current);
      debounceRef.current = null;
    }
    pendingRangeRef.current = null;

    let t: number;
    try {
      t = await browse(query, sort, options);
    } catch {
      if (gen === genRef.current) {
        cacheRef.current.clear();
        totalRef.current = 0;
        setTotal(0);
        tick();
      }
      return 0;
    }
    if (gen !== genRef.current) return t; // stale（呼び出し側のseqガードで破棄される）

    cacheRef.current.clear();
    totalRef.current = t;
    setTotal(t);

    // 先頭窓を即prefetch
    if (t > 0) {
      const limit = Math.min(INITIAL_PREFETCH, t);
      try {
        const rows = await fetchWindow(0, limit);
        if (gen === genRef.current) {
          rows.forEach((row, idx) => cacheRef.current.set(idx, row));
        }
      } catch {
        // 先頭窓のprefetch失敗時はensureRangeでの再取得に委ねる
      }
    }

    if (gen === genRef.current) tick();
    return t;
  }, [tick]);

  return { total, getRow, ensureRange, runBrowse };
}
