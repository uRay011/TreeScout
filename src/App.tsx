import { useState, useCallback, useEffect, useRef, useLayoutEffect } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { PanelRight, Globe, Folder, X } from "lucide-react";
import { SearchBar } from "./components/SearchBar/SearchBar";
import { ResultList } from "./components/SearchBar/ResultList";
import { ViewModePopup, ViewMode } from "./components/SearchBar/ViewModePopup";
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
} from "./lib/tauri";
import "./App.css";

// ── ロゴSVG ──────────────────────────────────────
function LogoMark() {
  return (
    <svg className="logo-mark" viewBox="0 0 20 20" fill="none" xmlns="http://www.w3.org/2000/svg" aria-hidden>
      <rect x="9" y="13" width="2" height="4" rx="0.5" fill="#8b949e"/>
      <polygon points="10,2 16,9 4,9"   fill="#3fb950" opacity="0.9"/>
      <polygon points="10,5 15,11 5,11" fill="#2ea043" opacity="0.85"/>
      <polygon points="10,8 14,13 6,13" fill="#238636" opacity="0.8"/>
      <circle cx="14" cy="14" r="3.5" stroke="#316dca" strokeWidth="1.5" fill="none"/>
      <line x1="16.5" y1="16.5" x2="18.5" y2="18.5" stroke="#316dca" strokeWidth="1.5" strokeLinecap="round"/>
    </svg>
  );
}

// ── ウィンドウコントロール ────────────────────────
function WindowControls() {
  const appWindow = getCurrentWindow();
  return (
    <div className="titlebar-winctrls" aria-label="ウィンドウ操作">
      <button className="winctrl" title="最小化" aria-label="最小化" type="button" onClick={() => appWindow.minimize()}>
        <svg width="10" height="1" viewBox="0 0 10 1" aria-hidden><rect width="10" height="1" fill="currentColor"/></svg>
      </button>
      <button className="winctrl" title="最大化" aria-label="最大化" type="button" onClick={() => appWindow.toggleMaximize()}>
        <svg width="10" height="10" viewBox="0 0 10 10" aria-hidden><rect x="0.5" y="0.5" width="9" height="9" stroke="currentColor" fill="none"/></svg>
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

  // Phase 4: プレビューペイン
  const [previewTarget,    setPreviewTarget]    = useState<PreviewSelection | null>(null);
  const [previewCollapsed, setPreviewCollapsed] = useState(false);
  const previewPaneRef = useRef<HTMLDivElement>(null);

  // ルートフォルダ（探索範囲の絞り込み。既定は未指定＝ドライブ全体）
  const [rootPath, setRootPath] = useState("");

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
    setIsLoading(true);
    setColumns([]);
    setGuideScore(null);
    setPreviewTarget(null);
    exploreEventsRef.current = [];
    setPhase("Phase 1: Everything 絞り込み…");
    setLogEntry(null);
    setCounts({ o: 0, s: 0, f: 0 });
    const counts = { o: 0, s: 0, f: 0 };
    let phaseAdvanced = false;
    const t0 = performance.now();
    startElapsedTimer(t0);
    try {
      const items = await semanticSearch(query, {
        rootPath,
        onExplore: (ev) => {
          exploreEventsRef.current.push(ev);
          setColumns(buildColumnsFromEvents(exploreEventsRef.current));
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
      // SemanticResult → SearchResult（ResultList 互換）に変換
      const normalized: SearchResult[] = items.map(r => ({
        name: r.name,
        path: r.path,
        folder: r.path.replace(/[\\/][^\\/]+$/, "") || r.path,
        is_dir: false,
        ext: r.ext,
        size: 0,
        modified: "",
        score: r.score,
      }));
      setResults(normalized);
      stopElapsedTimer(performance.now() - t0);
      setPhase(`完了 — ${normalized.length}件 / ${Math.round(performance.now() - t0)}ms`);
    } catch {
      setResults([]);
      stopElapsedTimer(performance.now() - t0);
      setPhase("エラーが発生しました");
    } finally {
      setIsLoading(false);
    }
  }, [query, rootPath, startElapsedTimer, stopElapsedTimer]);

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

  // 初回表示: 検索キーワード未入力でも全ファイルを表示する（Everythingの挙動に合わせる）
  useEffect(() => {
    handleSearch();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // ルートフォルダ確定後、即座に一覧を取得する
  useEffect(() => {
    if (rootPath) {
      handleSearch();
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [rootPath]);

  // カラムUI: エントリ選択 → アクティブ化 + プレビュー更新
  const handleColumnEntrySelect = useCallback((colIndex: number, entry: AstarEntry) => {
    setColumns(cols =>
      cols.map((col, i) => (i === colIndex ? { ...col, activeEntryPath: entry.path } : col))
    );
    if (entry.kind === "found") {
      setGuideScore(entry.score);
      setPreviewTarget({ path: entry.path, name: entry.name, ext: entry.ext, score: entry.score });
    }
  }, []);

  // 結果リスト: 行選択 → 最終カラムをアクティブ化 + プレビュー更新
  const handleResultSelect = useCallback((index: number) => {
    setSelectedIndex(index);
    const r = results[index];
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
  }, [results]);

  // プレビューペインの表示切替
  const togglePreview = useCallback(() => setPreviewCollapsed(v => !v), []);

  // 左ペインの表示形式を変更し、localStorageへ永続化する
  const handleViewModeChange = useCallback((mode: ViewMode) => {
    setViewMode(mode);
    localStorage.setItem(VIEW_MODE_KEY, mode);
  }, []);

  // 「表示」メニューボタン（タイトルバー / ハンバーガードロップダウン共通）
  const onViewMenuClick = useCallback((e: React.MouseEvent<HTMLButtonElement>) => {
    setViewMenuAnchor(prev => (prev ? null : e.currentTarget));
  }, []);
  const closeViewMenu = useCallback(() => setViewMenuAnchor(null), []);

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
      const next = e.key === "ArrowDown"
        ? Math.min(selectedIndex + 1, results.length - 1)
        : Math.max(selectedIndex - 1, 0);
      handleResultSelect(next);
    }
  }, [results.length, selectedIndex, handleResultSelect, togglePreview]);

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
          <span className="logo-name">TreeScout</span>
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

        {/* 検索（タイトルバー中央。幅不足時は非表示） */}
        {!searchCollapsed && (
          <div className="titlebar-search-center" ref={searchRef}>
            <SearchBar
              value={query}
              onChange={setQuery}
              onSubmit={handleSearch}
              isLoading={isLoading}
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
            selectedIndex={selectedIndex}
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
