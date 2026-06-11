import { useState, useCallback, useEffect, useRef, useLayoutEffect } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { writeText } from "@tauri-apps/plugin-clipboard-manager";
import { PanelRight, Globe, Folder, X } from "lucide-react";
import { SearchBar } from "./components/SearchBar/SearchBar";
import { ResultList } from "./components/SearchBar/ResultList";
import { ViewModePopup, ViewMode } from "./components/SearchBar/ViewModePopup";
import { MenuBarPopup, MenuItemDef } from "./components/Menu/MenuBarPopup";
import { ColumnView } from "./components/ColumnView/ColumnView";
import { PreviewPane, PreviewSelection } from "./components/Preview/PreviewPane";
import {
  semanticSearch,
  SearchResult,
  ExploreEvent,
  AstarColumn,
  AstarEntry,
  buildColumnsFromEvents,
  basename,
  listDirectory,
  BrowseSort,
  BrowseSortCol,
  MatchOptions,
} from "./lib/tauri";
import { useBrowseWindow } from "./hooks/useBrowseWindow";
import "./App.css";

// ── ロゴSVG ──────────────────────────────────────
function LogoMark() {
  return (
    <svg className="logo-mark" viewBox="0 0 20 20" fill="none" xmlns="http://www.w3.org/2000/svg" aria-hidden>
      <line x1="10" y1="5.5" x2="10" y2="9" stroke="#58677a" strokeWidth="1.4" strokeLinecap="round"/>
      <line x1="10" y1="9" x2="5.5" y2="14" stroke="#58677a" strokeWidth="1.4" strokeLinecap="round"/>
      <line x1="10" y1="9" x2="14.5" y2="14" stroke="#58a6ff" strokeWidth="1.5" strokeLinecap="round"/>
      <circle cx="10" cy="4.5" r="2" fill="#3fb950"/>
      <circle cx="5.5" cy="14.5" r="1.6" fill="#58677a"/>
      <circle cx="14.5" cy="15" r="3" stroke="#316dca" strokeWidth="1.5" fill="none"/>
      <circle cx="14.5" cy="15" r="1.2" fill="#58a6ff"/>
    </svg>
  );
}

// ── ウィンドウコントロール ────────────────────────
function WindowControls() {
  const appWindow = getCurrentWindow();
  const [isMaximized, setIsMaximized] = useState(false);

  useEffect(() => {
    let cancelled = false;
    appWindow.isMaximized().then(v => { if (!cancelled) setIsMaximized(v); });
    const unlistenPromise = appWindow.onResized(() => {
      appWindow.isMaximized().then(v => { if (!cancelled) setIsMaximized(v); });
    });
    return () => {
      cancelled = true;
      unlistenPromise.then(f => f());
    };
  }, []);

  return (
    <div className="titlebar-winctrls" aria-label="ウィンドウ操作">
      <button className="winctrl" title="最小化" aria-label="最小化" type="button" onClick={() => appWindow.minimize()}>
        <svg width="10" height="1" viewBox="0 0 10 1" aria-hidden><rect width="10" height="1" fill="currentColor"/></svg>
      </button>
      <button
        className="winctrl"
        title={isMaximized ? "元に戻す" : "最大化"}
        aria-label={isMaximized ? "元に戻す" : "最大化"}
        type="button"
        onClick={() => appWindow.toggleMaximize()}
      >
        {isMaximized ? (
          <svg width="10" height="10" viewBox="0 0 10 10" aria-hidden>
            <path d="M2.5 2.5V0.5H9.5V7.5H7.5" stroke="currentColor" fill="none"/>
            <rect x="0.5" y="2.5" width="7" height="7" stroke="currentColor" fill="none"/>
          </svg>
        ) : (
          <svg width="10" height="10" viewBox="0 0 10 10" aria-hidden><rect x="0.5" y="0.5" width="9" height="9" stroke="currentColor" fill="none"/></svg>
        )}
      </button>
      <button className="winctrl close" title="閉じる" aria-label="閉じる" type="button" onClick={() => appWindow.close()}>
        <svg width="10" height="10" viewBox="0 0 10 10" aria-hidden>
          <line x1="1" y1="1" x2="9" y2="9" stroke="currentColor" strokeWidth="1.4" strokeLinecap="round"/>
          <line x1="9" y1="1" x2="1" y2="9" stroke="currentColor" strokeWidth="1.4" strokeLinecap="round"/>
        </svg>
      </button>
    </div>
  );
}

const MENU_ITEMS = ["ファイル", "編集", "表示", "検索", "ブックマーク", "ツール", "ヘルプ"];

// 左ペインの表示形式（コンパクト/標準/詳細）はlocalStorageに保存し次回起動時も維持する
const VIEW_MODE_KEY = "treescout.viewMode";

function loadViewMode(): ViewMode {
  const saved = localStorage.getItem(VIEW_MODE_KEY);
  return saved === "compact" || saved === "detail" || saved === "standard" ? saved : "standard";
}

// 検索モード（AIセマンティック検索 / 素のEverythingフィルタ）もlocalStorageに永続化する。
// 既定は本アプリの主機能であるAIセマンティック検索。
type SearchMode = "ai" | "filter";
const SEARCH_MODE_KEY = "treescout.searchMode";

function loadSearchMode(): SearchMode {
  return localStorage.getItem(SEARCH_MODE_KEY) === "filter" ? "filter" : "ai";
}

// 検索メニューのマッチオプション（大文字小文字の区別 / 単語に完全一致 / フォルダ名にマッチ）も永続化する
const SEARCH_OPTIONS_KEY = "treescout.searchOptions";

function loadSearchOptions(): Required<MatchOptions> {
  try {
    const saved = JSON.parse(localStorage.getItem(SEARCH_OPTIONS_KEY) ?? "{}");
    return {
      caseSensitive: !!saved.caseSensitive,
      wholeWord: !!saved.wholeWord,
      matchPath: !!saved.matchPath,
    };
  } catch {
    return { caseSensitive: false, wholeWord: false, matchPath: false };
  }
}

// ── ステータスバー: A*探索ログ1件分のテキストを構築 ──────
interface StatusLogData { key: number; type: "open" | "skip" | "found"; text: string }

function exploreLogEntry(ev: ExploreEvent, key: number): StatusLogData {
  switch (ev.type) {
    case "open_dir":
      return { key, type: "open", text: `opened   ${basename(ev.path)}/   h=${ev.h_score.toFixed(2)}` };
    case "skip_dir":
      return { key, type: "skip", text: `skipped  ${basename(ev.path)}/   h=${ev.h_score.toFixed(2)}` };
    case "found_file":
      return { key, type: "found", text: `found    ${basename(ev.path)}   f=${ev.score.toFixed(2)}` };
  }
}

// ── ステータスバー: ログエントリの入場フェード ──────────
function StatusLogEntry({ entry }: { entry: StatusLogData | null }) {
  const [pre, setPre] = useState(true);

  useEffect(() => {
    if (!entry) return;
    setPre(true);
    const id = requestAnimationFrame(() => setPre(false));
    return () => cancelAnimationFrame(id);
  }, [entry?.key]);

  if (!entry) return null;
  return <span className={`entry log-${entry.type}${pre ? " pre" : ""}`}>{entry.text}</span>;
}

export default function App() {
  const [query,         setQuery]         = useState("");
  const [results,       setResults]       = useState<SearchResult[]>([]);
  const [selectedIndex, setSelectedIndex] = useState(-1);
  const [isLoading,     setIsLoading]     = useState(false);
  const [menuOpen,      setMenuOpen]      = useState(false);

  // ステータスバー: フェーズ・探索ログ・カウント・経過時間・ヒント
  const [phase,     setPhase]     = useState("待機中");
  const [counts,    setCounts]    = useState<{ o: number; s: number; f: number } | null>(null);
  const [logEntry,  setLogEntry]  = useState<StatusLogData | null>(null);
  const [elapsedMs, setElapsedMs] = useState<number | null>(null);
  const [hintsMode, setHintsMode] = useState<"list" | "default">("default");
  const elapsedRafRef = useRef<number | null>(null);
  const logKeyRef = useRef(0);
  // 左ペインの表示形式（コンパクト/標準/詳細）
  const [viewMode, setViewMode] = useState<ViewMode>(loadViewMode);
  const [viewMenuAnchor, setViewMenuAnchor] = useState<HTMLElement | null>(null);

  // ファイル/編集/検索メニューの開閉（共通ドロップダウン）
  const [openMenuKey, setOpenMenuKey] = useState<string | null>(null);
  const [menuPopupAnchor, setMenuPopupAnchor] = useState<HTMLElement | null>(null);

  // 検索メニューのマッチオプション（大文字小文字の区別/単語に完全一致/フォルダ名にマッチ）
  const [searchOptions, setSearchOptions] = useState<Required<MatchOptions>>(loadSearchOptions);

  // 結果リストの複数選択（編集メニューの「全て選択」「コピー」用）
  const [selectedSet, setSelectedSet] = useState<Set<number>>(new Set());

  // メニューバーと検索ボックスが重なる前にハンバーガーへ切り替え
  const [menuCollapsed, setMenuCollapsed] = useState(false);
  // さらに幅が不足したら検索ボックスを非表示にする
  const [searchCollapsed, setSearchCollapsed] = useState(false);
  const titlebarRef = useRef<HTMLElement>(null);
  const menuRef     = useRef<HTMLElement>(null);
  const searchRef   = useRef<HTMLDivElement>(null);

  // Phase 4: A*探索カラムUI
  const [columns,    setColumns]    = useState<AstarColumn[]>([]);
  // 選択中ファイルのスコア（最終カラムのAIガイドパスライン点灯判定に使用。score >= 0.8）
  const [guideScore, setGuideScore] = useState<number | null>(null);
  const exploreEventsRef = useRef<ExploreEvent[]>([]);
  // StrictModeの二重マウント等でhandleSearchが多重起動した際、古い実行のイベントが
  // 後勝ちのカラムに混ざって二重表示されるのを防ぐための実行世代カウンタ
  const searchSeqRef = useRef(0);

  // Phase 4: プレビューペイン
  const [previewTarget,    setPreviewTarget]    = useState<PreviewSelection | null>(null);
  const [previewCollapsed, setPreviewCollapsed] = useState(false);
  const previewPaneRef = useRef<HTMLDivElement>(null);

  // ルートフォルダ（探索範囲の絞り込み。既定は未指定＝ドライブ全体）
  const [rootPath, setRootPath] = useState("");

  // 検索モード（AI / フィルタ）と左ペインのデータ供給モード（配列 / 窓取得）
  const [searchMode, setSearchMode] = useState<SearchMode>(loadSearchMode);
  const [leftPaneMode, setLeftPaneMode] = useState<"array" | "window">("array");
  const [browseSort, setBrowseSort] = useState<BrowseSort>({ col: "name", asc: true });
  // 窓モードで現在バックエンドに常駐させている Everything クエリ（ソート変更時の再取得に使う）
  const lastBrowseQueryRef = useRef("");
  const browseWin = useBrowseWindow();

  // ペイン分割リサイズ
  const leftPaneRef  = useRef<HTMLDivElement>(null);
  const resizerRef   = useRef<HTMLDivElement>(null);

  // ステータスバー: 経過ms表示用 rAF タイマー
  const startElapsedTimer = useCallback((t0: number) => {
    const tick = () => {
      setElapsedMs(performance.now() - t0);
      elapsedRafRef.current = requestAnimationFrame(tick);
    };
    elapsedRafRef.current = requestAnimationFrame(tick);
  }, []);
  const stopElapsedTimer = useCallback((finalMs?: number) => {
    if (elapsedRafRef.current !== null) {
      cancelAnimationFrame(elapsedRafRef.current);
      elapsedRafRef.current = null;
    }
    if (finalMs !== undefined) setElapsedMs(finalMs);
  }, []);
  useEffect(() => () => stopElapsedTimer(), [stopElapsedTimer]);

  const handleSearch = useCallback(async () => {
    const seq = ++searchSeqRef.current;
    setIsLoading(true);
    setColumns([]);
    setGuideScore(null);
    setPreviewTarget(null);
    exploreEventsRef.current = [];
    setLogEntry(null);
    setCounts(null);
    const t0 = performance.now();
    startElapsedTimer(t0);
    setSelectedIndex(-1);
    setSelectedSet(new Set());

    // 窓モード共通: Everything全件をバックエンドに常駐させ、件数だけ受け取る。
    // 実際の行データは ResultList の可視範囲駆動で fetch_window から取得する。
    const runWindowBrowse = async (everythingQuery: string) => {
      lastBrowseQueryRef.current = everythingQuery;
      setLeftPaneMode("window");
      setResults([]);
      setPhase("Everything 検索中…");
      try {
        const total = await browseWin.runBrowse(everythingQuery, browseSort, searchOptions);
        if (seq !== searchSeqRef.current) return;
        stopElapsedTimer(performance.now() - t0);
        setPhase(`完了 — ${total}件 / ${Math.round(performance.now() - t0)}ms`);
      } catch {
        if (seq !== searchSeqRef.current) return;
        stopElapsedTimer(performance.now() - t0);
        setPhase("エラーが発生しました");
      } finally {
        if (seq === searchSeqRef.current) setIsLoading(false);
      }
    };

    // 空クエリ（フィルタなし）時はA*探索をバイパスする。
    // ルート指定時はそのフォルダ直下をlistDirectoryで取得し、左ペイン・カラムUIの
    // 起点（カラム0）として表示する。ルート未指定時はEverythingの全件結果を表示する
    if (!query.trim()) {
      if (rootPath) {
        setLeftPaneMode("array");
        setPhase("フォルダ読み込み中…");
        try {
          const children = await listDirectory(rootPath);
          if (seq !== searchSeqRef.current) return;
          const normalized: SearchResult[] = children.map(c => ({
            name: c.name,
            path: c.path,
            folder: c.folder,
            is_dir: c.is_dir,
            ext: c.ext,
            size: 0,
            modified: "",
          }));
          setResults(normalized);
          setColumns([{
            id: `col-0-${rootPath}`,
            label: basename(rootPath) || rootPath,
            entries: children.map(c => ({
              path: c.path,
              name: c.name,
              ext: c.ext,
              is_dir: c.is_dir,
              score: 0,
              kind: "opened",
            })),
            activeEntryPath: null,
          }]);
          stopElapsedTimer(performance.now() - t0);
          setPhase(`完了 — ${normalized.length}件 / ${Math.round(performance.now() - t0)}ms`);
        } catch {
          if (seq !== searchSeqRef.current) return;
          setResults([]);
          stopElapsedTimer(performance.now() - t0);
          setPhase("エラーが発生しました");
        } finally {
          if (seq === searchSeqRef.current) setIsLoading(false);
        }
        return;
      }

      // ルート未指定の空クエリ＝ドライブ全件ブラウズ。窓取得で無制限表示する
      await runWindowBrowse("");
      return;
    }

    // キーワードあり・フィルタモード: A*を介さず素のEverything結果を無制限表示（窓）
    if (searchMode === "filter") {
      const q = query.trim();
      await runWindowBrowse(rootPath ? `path:"${rootPath}" ${q}` : q);
      return;
    }

    // キーワードあり・AIモード: 2フェーズセマンティック検索（配列・従来通り）
    setLeftPaneMode("array");
    setPhase("Phase 1: Everything 絞り込み…");
    setCounts({ o: 0, s: 0, f: 0 });
    const counts = { o: 0, s: 0, f: 0 };
    let phaseAdvanced = false;
    try {
      const items = await semanticSearch(query, {
        // Everything候補上限(1000件)に合わせ、A*の上位K件絞り込みで結果が削られないようにする
        topK: 1000,
        rootPath,
        matchOptions: searchOptions,
        onExplore: (ev) => {
          if (seq !== searchSeqRef.current) return; // 古い実行からの遅延イベントは無視（カラム重複防止）
          exploreEventsRef.current.push(ev);
          setColumns(buildColumnsFromEvents(exploreEventsRef.current, rootPath));
          if (!phaseAdvanced) {
            phaseAdvanced = true;
            setPhase("Phase 2: A*探索…");
          }
          if (ev.type === "open_dir") counts.o++;
          else if (ev.type === "skip_dir") counts.s++;
          else counts.f++;
          setCounts({ ...counts });
          setLogEntry(exploreLogEntry(ev, ++logKeyRef.current));
        },
      });
      if (seq !== searchSeqRef.current) return;
      // SemanticResult → SearchResult（ResultList 互換）に変換
      const normalized: SearchResult[] = items.map(r => ({
        name: r.name,
        path: r.path,
        folder: r.path.replace(/[\\/][^\\/]+$/, "") || r.path,
        is_dir: r.is_dir,
        ext: r.ext,
        size: 0,
        modified: "",
        score: r.score,
      }));
      setResults(normalized);
      stopElapsedTimer(performance.now() - t0);
      setPhase(`完了 — ${normalized.length}件 / ${Math.round(performance.now() - t0)}ms`);
    } catch {
      if (seq !== searchSeqRef.current) return;
      setResults([]);
      stopElapsedTimer(performance.now() - t0);
      setPhase("エラーが発生しました");
    } finally {
      if (seq === searchSeqRef.current) setIsLoading(false);
    }
  }, [query, rootPath, searchMode, browseSort, searchOptions, browseWin.runBrowse, startElapsedTimer, stopElapsedTimer]);

  // ルートフォルダ選択ダイアログ
  const handleBrowseRoot = useCallback(async () => {
    const selected = await open({ directory: true, multiple: false });
    if (typeof selected === "string") {
      setRootPath(selected);
    }
  }, []);

  // スコープ解除（ドライブ全体検索に戻す）
  const handleClearRoot = useCallback(() => {
    setRootPath("");
  }, []);

  // 初回表示、ルートフォルダの確定・解除（全体に戻す）、検索オプション切替時に即座に一覧を取得する
  useEffect(() => {
    handleSearch();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [rootPath, searchMode, searchOptions]);

  // カラムUI: エントリ選択 → アクティブ化 + プレビュー更新
  // フォルダ選択時は直下の中身を取得し、次のカラムとして展開する（それ以降の既存カラムは破棄）
  const handleColumnEntrySelect = useCallback(async (colIndex: number, entry: AstarEntry) => {
    setColumns(cols =>
      cols.slice(0, colIndex + 1).map((col, i) => (i === colIndex ? { ...col, activeEntryPath: entry.path } : col))
    );

    if (!entry.is_dir) {
      setGuideScore(entry.score);
      setPreviewTarget({ path: entry.path, name: entry.name, ext: entry.ext, score: entry.score });
      return;
    }

    try {
      // query を渡し、展開した子にもヒート色用のスコアを付与する
      const children = await listDirectory(entry.path, query);
      const nextColumn: AstarColumn = {
        id: `col-${colIndex + 1}-${entry.path}`,
        label: entry.name,
        entries: children.map(c => ({
          path: c.path,
          name: c.name,
          ext: c.ext,
          is_dir: c.is_dir,
          score: c.score,
          kind: c.is_dir ? "opened" : c.score > 0 ? "found" : "skipped",
        })),
        activeEntryPath: null,
      };
      setColumns(cols => [...cols.slice(0, colIndex + 1), nextColumn]);
    } catch {
      setColumns(cols => cols.slice(0, colIndex + 1));
    }
  }, [query]);

  // 結果リスト: 行選択 → 最終カラムをアクティブ化 + プレビュー更新
  const handleResultSelect = useCallback((index: number) => {
    setSelectedIndex(index);
    setSelectedSet(new Set());
    const r = leftPaneMode === "window" ? browseWin.getRow(index) : results[index];
    if (r) {
      setPreviewTarget({
        path: r.path,
        name: r.name,
        ext: r.ext,
        score: r.score ?? 0,
        size: r.size || undefined,
        modified: r.modified || undefined,
      });
      setGuideScore(r.score ?? 0);
      setColumns(cols => {
        if (cols.length === 0) return cols;
        const lastIdx = cols.length - 1;
        return cols.map((col, i) => (i === lastIdx ? { ...col, activeEntryPath: r.path } : col));
      });
    }
  }, [results, leftPaneMode, browseWin.getRow]);

  // プレビューペインの表示切替
  const togglePreview = useCallback(() => setPreviewCollapsed(v => !v), []);

  // 左ペインの表示形式を変更し、localStorageへ永続化する
  const handleViewModeChange = useCallback((mode: ViewMode) => {
    setViewMode(mode);
    localStorage.setItem(VIEW_MODE_KEY, mode);
  }, []);

  // 検索モード（AI / フィルタ）切替。永続化し、searchMode を依存にした上の useEffect が
  // handleSearch を再実行する（キーワード入力中ならその場で結果が切り替わる）
  const handleModeChange = useCallback((mode: SearchMode) => {
    setSearchMode(mode);
    localStorage.setItem(SEARCH_MODE_KEY, mode);
  }, []);

  // 窓モードのソート列ヘッダクリック。ソートは Everything 索引側で行うため、
  // 同じ常駐クエリを新しいソートで再取得する（score 列は窓モードでは非表示）
  const handleListSortChange = useCallback((col: string) => {
    if (col === "score") return;
    const c = col as BrowseSortCol;
    setSelectedIndex(-1);
    setSelectedSet(new Set());
    setBrowseSort(prev => {
      const next: BrowseSort = prev.col === c ? { col: c, asc: !prev.asc } : { col: c, asc: true };
      browseWin.runBrowse(lastBrowseQueryRef.current, next, searchOptions);
      return next;
    });
  }, [browseWin.runBrowse, searchOptions]);

  // 「表示」メニューボタン（タイトルバー / ハンバーガードロップダウン共通）
  const onViewMenuClick = useCallback((e: React.MouseEvent<HTMLButtonElement>) => {
    setViewMenuAnchor(prev => (prev ? null : e.currentTarget));
  }, []);
  const closeViewMenu = useCallback(() => setViewMenuAnchor(null), []);

  // 「ファイル」「編集」「検索」メニューボタン共通（タイトルバー / ハンバーガードロップダウン共通）
  const onMenuPopupClick = useCallback((key: string, e: React.MouseEvent<HTMLButtonElement>) => {
    setOpenMenuKey(prev => {
      if (prev === key) {
        setMenuPopupAnchor(null);
        return null;
      }
      setMenuPopupAnchor(e.currentTarget);
      return key;
    });
  }, []);
  const closeMenuPopup = useCallback(() => {
    setOpenMenuKey(null);
    setMenuPopupAnchor(null);
  }, []);

  // 検索メニューのトグル（大文字小文字の区別/単語に完全一致/フォルダ名にマッチ）。永続化する
  const toggleSearchOption = useCallback((key: keyof MatchOptions) => {
    setSearchOptions(prev => {
      const next = { ...prev, [key]: !prev[key] };
      localStorage.setItem(SEARCH_OPTIONS_KEY, JSON.stringify(next));
      return next;
    });
  }, []);

  // 編集メニュー「全て選択」: 現在の左ペインに表示中の全行を選択状態にする
  const handleSelectAll = useCallback(() => {
    const count = leftPaneMode === "window" ? browseWin.total : results.length;
    if (count <= 0) return;
    setSelectedSet(new Set(Array.from({ length: count }, (_, i) => i)));
  }, [leftPaneMode, browseWin.total, results.length]);

  // 編集メニュー「コピー」: 複数選択中ならその全パス、なければ選択中の1件のパスをコピーする
  const handleCopy = useCallback(async () => {
    const getRow = (i: number) => (leftPaneMode === "window" ? browseWin.getRow(i) : results[i]);
    let paths: string[];
    if (selectedSet.size > 0) {
      paths = Array.from(selectedSet)
        .sort((a, b) => a - b)
        .map(getRow)
        .filter((r): r is SearchResult => !!r)
        .map(r => r.path);
    } else if (selectedIndex >= 0) {
      const r = getRow(selectedIndex);
      paths = r ? [r.path] : [];
    } else {
      paths = [];
    }
    if (paths.length === 0) return;
    await writeText(paths.join("\n"));
  }, [selectedSet, selectedIndex, leftPaneMode, browseWin.getRow, results]);

  // ステータスバー右端のヒント: フォーカス位置に応じて切替
  const handleFocus = useCallback((e: React.FocusEvent) => {
    const target = e.target as HTMLElement;
    setHintsMode(target.closest(".file-list") ? "list" : "default");
  }, []);

  // ── キーボードナビゲーション ──
  const handleKeyDown = useCallback((e: React.KeyboardEvent) => {
    if ((e.ctrlKey || e.metaKey) && (e.key === "p" || e.key === "P")) {
      e.preventDefault();
      togglePreview();
      return;
    }
    if (e.key === "ArrowDown" || e.key === "ArrowUp") {
      e.preventDefault();
      const count = leftPaneMode === "window" ? browseWin.total : results.length;
      const next = e.key === "ArrowDown"
        ? Math.min(selectedIndex + 1, count - 1)
        : Math.max(selectedIndex - 1, 0);
      handleResultSelect(next);
    }
  }, [results.length, browseWin.total, leftPaneMode, selectedIndex, handleResultSelect, togglePreview]);

  // ── メニューショートカット（フォーカス位置によらず常に有効） ──
  // ファイル＞終了 / 検索＞各種マッチオプション / 編集＞コピー・全て選択
  useEffect(() => {
    const handler = (e: KeyboardEvent) => {
      if (!(e.ctrlKey || e.metaKey)) return;
      const target = e.target as HTMLElement | null;
      const isTextInput = target?.tagName === "INPUT" || target?.tagName === "TEXTAREA";

      switch (e.key.toLowerCase()) {
        case "q":
          e.preventDefault();
          getCurrentWindow().close();
          break;
        case "i":
          e.preventDefault();
          toggleSearchOption("caseSensitive");
          break;
        case "b":
          e.preventDefault();
          toggleSearchOption("wholeWord");
          break;
        case "u":
          e.preventDefault();
          toggleSearchOption("matchPath");
          break;
        case "a":
          if (!isTextInput) {
            e.preventDefault();
            handleSelectAll();
          }
          break;
        case "c":
          if (!isTextInput) {
            e.preventDefault();
            handleCopy();
          }
          break;
      }
    };
    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, [toggleSearchOption, handleSelectAll, handleCopy]);

  // ── メニューバー/検索ボックスの重なり検知 ──
  // メニュー項目は固定なので、表示中に一度だけ自然幅を計測してキャッシュする
  const menuNaturalWidthRef = useRef<number | null>(null);

  useLayoutEffect(() => {
    const titlebarEl = titlebarRef.current;
    if (!titlebarEl) return;

    const MARGIN = 16; // 各要素間に最低限確保する余白(px)
    const SEARCH_WIDTH    = 400; // .search-wrap の幅
    const HAMBURGER_WIDTH = 36;  // .menu-hamburger（padding 11px*2 + 14px）
    const WINCTRLS_WIDTH  = 138; // .titlebar-winctrls（46px * 3）

    const check = () => {
      const menuEl = menuRef.current;
      if (menuEl && !menuEl.hidden) {
        menuNaturalWidthRef.current = menuEl.getBoundingClientRect().width;
      }
      const menuWidth = menuNaturalWidthRef.current;
      if (menuWidth == null) return;

      const titlebarRect = titlebarEl.getBoundingClientRect();
      const logoRect = titlebarEl.querySelector(".titlebar-logo")?.getBoundingClientRect();
      const logoRight = logoRect?.right ?? titlebarRect.left;

      // 検索ボックスはタイトルバー中央に固定幅で配置される
      const titlebarCenter = titlebarRect.left + titlebarRect.width / 2;
      const searchLeft  = titlebarCenter - SEARCH_WIDTH / 2;
      const searchRight = titlebarCenter + SEARCH_WIDTH / 2;

      const menuRight = logoRight + menuWidth;
      const collapsed = menuRight + MARGIN > searchLeft;
      setMenuCollapsed(collapsed);

      // メニュー折りたたみ後も足りなければ検索ボックスを非表示にする
      const leftGroupRight = collapsed ? logoRight + HAMBURGER_WIDTH : menuRight;
      const rightGroupLeft = titlebarRect.right - WINCTRLS_WIDTH;
      setSearchCollapsed(
        searchLeft < leftGroupRight + MARGIN || searchRight > rightGroupLeft - MARGIN
      );
    };

    const ro = new ResizeObserver(check);
    ro.observe(titlebarEl);
    check();

    return () => ro.disconnect();
  }, []);

  // ── ペイン分割リサイズ ──
  const onResizerMouseDown = (e: React.MouseEvent<HTMLDivElement>) => {
    const startX  = e.clientX;
    const startW  = leftPaneRef.current?.getBoundingClientRect().width ?? 520;
    const handle  = e.currentTarget;

    const onMove = (ev: MouseEvent) => {
      const nw = Math.min(Math.max(startW + (ev.clientX - startX), 200), window.innerWidth - 280);
      if (leftPaneRef.current) leftPaneRef.current.style.width = `${nw}px`;
    };
    const onUp = () => {
      document.body.style.cursor     = "";
      document.body.style.userSelect = "";
      handle.classList.remove("dragging");
      window.removeEventListener("mousemove", onMove);
      window.removeEventListener("mouseup", onUp);
    };
    handle.classList.add("dragging");
    document.body.style.cursor     = "col-resize";
    document.body.style.userSelect = "none";
    window.addEventListener("mousemove", onMove);
    window.addEventListener("mouseup", onUp);
  };

  // ── プレビューペイン分割リサイズ（左へドラッグで拡大） ──
  const onPreviewResizerMouseDown = (e: React.MouseEvent<HTMLDivElement>) => {
    const startX = e.clientX;
    const startW = previewPaneRef.current?.getBoundingClientRect().width ?? 380;
    const handle = e.currentTarget;

    const onMove = (ev: MouseEvent) => {
      const nw = Math.min(Math.max(startW - (ev.clientX - startX), 300), window.innerWidth * 0.42);
      if (previewPaneRef.current) previewPaneRef.current.style.width = `${nw}px`;
    };
    const onUp = () => {
      document.body.style.cursor     = "";
      document.body.style.userSelect = "";
      handle.classList.remove("dragging");
      window.removeEventListener("mousemove", onMove);
      window.removeEventListener("mouseup", onUp);
    };
    handle.classList.add("dragging");
    document.body.style.cursor     = "col-resize";
    document.body.style.userSelect = "none";
    window.addEventListener("mousemove", onMove);
    window.addEventListener("mouseup", onUp);
  };

  // ファイル/編集/検索メニューの項目定義（タイトルバー / ハンバーガードロップダウン共通）
  const menuPopupItems: Record<string, MenuItemDef[]> = {
    "ファイル": [
      { type: "action", label: "終了", shortcut: "Ctrl+Q", onSelect: () => getCurrentWindow().close() },
    ],
    "編集": [
      { type: "action", label: "コピー", shortcut: "Ctrl+C", onSelect: handleCopy },
      { type: "action", label: "全て選択", shortcut: "Ctrl+A", onSelect: handleSelectAll },
    ],
    "検索": [
      { type: "checkbox", label: "大文字小文字の区別", shortcut: "Ctrl+I", checked: searchOptions.caseSensitive, onToggle: () => toggleSearchOption("caseSensitive") },
      { type: "checkbox", label: "単語に完全一致", shortcut: "Ctrl+B", checked: searchOptions.wholeWord, onToggle: () => toggleSearchOption("wholeWord") },
      { type: "checkbox", label: "フォルダ名にマッチ", shortcut: "Ctrl+U", checked: searchOptions.matchPath, onToggle: () => toggleSearchOption("matchPath") },
    ],
  };

  return (
    <div className="app-root" onKeyDown={handleKeyDown} onFocus={handleFocus}>
      {/* ══ TITLEBAR ══ */}
      <header
        className="titlebar"
        role="banner"
        ref={titlebarRef}
        onDoubleClick={() => getCurrentWindow().toggleMaximize()}
      >
        <div className="titlebar-logo" aria-label="TreeScout">
          <LogoMark />
        </div>

        <nav
          className="titlebar-menu"
          aria-label="メインメニュー"
          id="titlebarMenu"
          ref={menuRef}
          hidden={menuCollapsed}
        >
          {MENU_ITEMS.map(item => (
            item === "表示" ? (
              <button
                key={item}
                className="menu-btn"
                type="button"
                aria-haspopup="true"
                aria-expanded={!!viewMenuAnchor}
                onClick={onViewMenuClick}
              >
                {item}
              </button>
            ) : menuPopupItems[item] ? (
              <button
                key={item}
                className="menu-btn"
                type="button"
                aria-haspopup="true"
                aria-expanded={openMenuKey === item}
                onClick={(e) => onMenuPopupClick(item, e)}
              >
                {item}
              </button>
            ) : (
              <button key={item} className="menu-btn" type="button">{item}</button>
            )
          ))}
        </nav>

        {/* ハンバーガー（幅不足時） */}
        {menuCollapsed && (
          <button
            className="menu-hamburger"
            type="button"
            aria-label="メニューを開く"
            aria-expanded={menuOpen}
            aria-controls="menuDropdown"
            onClick={() => setMenuOpen(v => !v)}
          >
            <span/><span/><span/>
          </button>
        )}
        {menuCollapsed && menuOpen && (
          <div className="menu-dropdown open" id="menuDropdown" role="menu">
            {MENU_ITEMS.map(item => (
              item === "表示" ? (
                <button
                  key={item}
                  className="menu-btn"
                  role="menuitem"
                  type="button"
                  aria-haspopup="true"
                  aria-expanded={!!viewMenuAnchor}
                  onClick={onViewMenuClick}
                >
                  {item}
                </button>
              ) : menuPopupItems[item] ? (
                <button
                  key={item}
                  className="menu-btn"
                  role="menuitem"
                  type="button"
                  aria-haspopup="true"
                  aria-expanded={openMenuKey === item}
                  onClick={(e) => onMenuPopupClick(item, e)}
                >
                  {item}
                </button>
              ) : (
                <button key={item} className="menu-btn" role="menuitem" type="button">{item}</button>
              )
            ))}
          </div>
        )}

        {viewMenuAnchor && (
          <ViewModePopup
            anchorEl={viewMenuAnchor}
            value={viewMode}
            onChange={handleViewModeChange}
            onClose={closeViewMenu}
          />
        )}

        {menuPopupAnchor && openMenuKey && menuPopupItems[openMenuKey] && (
          <MenuBarPopup
            anchorEl={menuPopupAnchor}
            items={menuPopupItems[openMenuKey]}
            onClose={closeMenuPopup}
            ariaLabel={`${openMenuKey}メニュー`}
          />
        )}

        {/* 検索（タイトルバー中央。幅不足時は非表示） */}
        {!searchCollapsed && (
          <div className="titlebar-search-center" ref={searchRef}>
            <SearchBar
              value={query}
              onChange={setQuery}
              onSubmit={handleSearch}
              isLoading={isLoading}
              mode={searchMode}
              onModeChange={handleModeChange}
            />
          </div>
        )}

        <WindowControls />
      </header>

      {/* ══ TOOLBAR（探索スコープのブレッドクラム） ══ */}
      <div className="toolbar" role="navigation" aria-label="探索スコープ">
        <span
          className={`scope-chip${rootPath ? "" : " scope-chip-all"}`}
          title={rootPath || "ドライブ全体を検索中"}
        >
          <span className="scope-ico" aria-hidden>
            {rootPath ? <Folder /> : <Globe />}
          </span>
          <span className="scope-text">{rootPath || "全体"}</span>
          {rootPath && (
            <button
              className="scope-clear"
              type="button"
              title="全体検索に戻す"
              aria-label="スコープを解除して全体検索に戻す"
              onClick={handleClearRoot}
            >
              <X />
            </button>
          )}
        </span>
        <button
          className="scope-pick"
          type="button"
          title="検索範囲をフォルダに限定"
          aria-label="検索範囲をフォルダに限定"
          onClick={handleBrowseRoot}
        >
          <Folder />
        </button>
        <div className="toolbar-spacer" />
        <button
          className="btn-icon"
          type="button"
          title="プレビュー切替 (Ctrl+P)"
          aria-label="プレビュー切替"
          aria-pressed={!previewCollapsed}
          onClick={togglePreview}
        >
          <PanelRight aria-hidden width={14} height={14} strokeWidth={1.5} />
        </button>
      </div>

      {/* ══ MAIN ══ */}
      <main className="main" role="main">
        {/* 左ペイン: 検索結果リスト */}
        <div ref={leftPaneRef} style={{ width: 320 }}>
          <ResultList
            results={results}
            windowSource={leftPaneMode === "window" ? {
              total: browseWin.total,
              getRow: browseWin.getRow,
              ensureRange: browseWin.ensureRange,
            } : undefined}
            sort={leftPaneMode === "window" ? browseSort : undefined}
            onSortChange={handleListSortChange}
            selectedIndex={selectedIndex}
            selectedIndices={selectedSet}
            onSelect={handleResultSelect}
            onOpen={(r) => console.info("open:", r.path)}
            query={query}
            isLoading={isLoading}
            hasSearched={phase !== "待機中"}
            viewMode={viewMode}
          />
        </div>

        {/* ペイン分割リサイザー */}
        <div
          ref={resizerRef}
          className="resizer"
          role="separator"
          aria-orientation="vertical"
          aria-label="ペインの幅を調整"
          onMouseDown={onResizerMouseDown}
        />

        {/* 中央ペイン: 探索型カラムUI（A*探索ログをリアルタイム展開、design.md §3.3） */}
        <div className="center-pane" id="centerPane">
          <ColumnView
            columns={columns}
            onEntrySelect={handleColumnEntrySelect}
            showGuide={guideScore !== null && guideScore >= 0.8}
            hasScore={query.trim() !== ""}
          />
        </div>

        {/* ペイン分割リサイザー（プレビュー） */}
        {!previewCollapsed && (
          <div
            className="resizer"
            role="separator"
            aria-orientation="vertical"
            aria-label="プレビューペインの幅を調整"
            onMouseDown={onPreviewResizerMouseDown}
          />
        )}

        {/* 右ペイン: ファイルプレビュー */}
        {!previewCollapsed && (
          <div ref={previewPaneRef} className="preview-pane" style={{ width: 380 }}>
            <PreviewPane selection={previewTarget} />
          </div>
        )}
      </main>

      {/* ══ STATUS BAR ══ */}
      <footer className="statusbar" role="status" aria-live="polite" aria-atomic>
        <span className={`status-dot${isLoading ? " loading" : ""}`} aria-hidden/>
        <span className="status-phase">{phase}</span>
        <span className="status-note">{rootPath ? `ルート: ${rootPath}` : ""}</span>
        <span className="status-log"><StatusLogEntry entry={logEntry} /></span>
        <span className="status-spacer" />
        <span className="status-counts">
          {counts && <>展開 <b>{counts.o}</b> ・ スキップ <b>{counts.s}</b> ・ 発見 <b>{counts.f}</b></>}
        </span>
        <span className="status-ms">{elapsedMs !== null ? `${Math.round(elapsedMs)}ms` : ""}</span>
        <span className="status-hints">
          {hintsMode === "list" ? (
            <>
              <span className="hint"><kbd>↑</kbd><kbd>↓</kbd> 移動</span>
              <span className="hint"><kbd>Ctrl</kbd><kbd>P</kbd> プレビュー</span>
            </>
          ) : (
            <span className="hint"><kbd>Ctrl</kbd><kbd>F</kbd> 検索</span>
          )}
        </span>
      </footer>
    </div>
  );
}
