import { useState, useCallback, useRef, useLayoutEffect } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { SearchBar } from "./components/SearchBar/SearchBar";
import { ResultList } from "./components/SearchBar/ResultList";
import { ColumnView } from "./components/ColumnView/ColumnView";
import {
  semanticSearch,
  SearchResult,
  ExploreEvent,
  AstarColumn,
  AstarEntry,
  buildColumnsFromEvents,
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
  return (
    <div className="titlebar-winctrls" aria-label="ウィンドウ操作">
      <button className="winctrl" title="最小化" aria-label="最小化" type="button">
        <svg width="10" height="1" viewBox="0 0 10 1" aria-hidden><rect width="10" height="1" fill="currentColor"/></svg>
      </button>
      <button className="winctrl" title="最大化" aria-label="最大化" type="button">
        <svg width="10" height="10" viewBox="0 0 10 10" aria-hidden><rect x="0.5" y="0.5" width="9" height="9" stroke="currentColor" fill="none"/></svg>
      </button>
      <button className="winctrl close" title="閉じる" aria-label="閉じる" type="button">
        <svg width="10" height="10" viewBox="0 0 10 10" aria-hidden>
          <line x1="1" y1="1" x2="9" y2="9" stroke="currentColor" strokeWidth="1.4" strokeLinecap="round"/>
          <line x1="9" y1="1" x2="1" y2="9" stroke="currentColor" strokeWidth="1.4" strokeLinecap="round"/>
        </svg>
      </button>
    </div>
  );
}

const MENU_ITEMS = ["ファイル", "編集", "表示", "検索", "ブックマーク", "ツール", "ヘルプ"];

export default function App() {
  const [query,         setQuery]         = useState("");
  const [results,       setResults]       = useState<SearchResult[]>([]);
  const [selectedIndex, setSelectedIndex] = useState(-1);
  const [isLoading,     setIsLoading]     = useState(false);
  const [elapsed,       setElapsed]       = useState(0);
  const [menuOpen,      setMenuOpen]      = useState(false);
  // メニューバーと検索ボックスが重なる前にハンバーガーへ切り替え
  const [menuCollapsed, setMenuCollapsed] = useState(false);
  const titlebarRef = useRef<HTMLElement>(null);
  const menuRef     = useRef<HTMLElement>(null);
  const searchRef   = useRef<HTMLDivElement>(null);

  // Phase 4: A*探索カラムUI
  const [columns,      setColumns]      = useState<AstarColumn[]>([]);
  const [selectedFile, setSelectedFile] = useState<AstarEntry | null>(null);
  const exploreEventsRef = useRef<ExploreEvent[]>([]);

  // ルートフォルダ（探索範囲の絞り込み）
  const [rootInput, setRootInput] = useState("");
  const [rootPath,  setRootPath]  = useState("");

  // ペイン分割リサイズ
  const leftPaneRef  = useRef<HTMLDivElement>(null);
  const resizerRef   = useRef<HTMLDivElement>(null);

  const handleSearch = useCallback(async () => {
    if (!query.trim()) return;
    setIsLoading(true);
    setColumns([]);
    setSelectedFile(null);
    exploreEventsRef.current = [];
    const t0 = performance.now();
    try {
      const items = await semanticSearch(query, {
        rootPath,
        onExplore: (ev) => {
          exploreEventsRef.current.push(ev);
          setColumns(buildColumnsFromEvents(exploreEventsRef.current));
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
      }));
      setResults(normalized);
      setElapsed((performance.now() - t0) / 1000);
    } catch {
      setResults([]);
      setElapsed((performance.now() - t0) / 1000);
    } finally {
      setIsLoading(false);
    }
  }, [query, rootPath]);

  // ルートフォルダ選択ダイアログ
  const handleBrowseRoot = useCallback(async () => {
    const selected = await open({ directory: true, multiple: false });
    if (typeof selected === "string") {
      setRootInput(selected);
      setRootPath(selected);
    }
  }, []);

  // ルートフォルダ入力を確定
  const handleApplyRoot = useCallback(() => {
    setRootPath(rootInput.trim());
  }, [rootInput]);

  // カラムUI: エントリ選択 → アクティブ化 + 詳細カード表示
  const handleColumnEntrySelect = useCallback((colIndex: number, entry: AstarEntry) => {
    setColumns(cols =>
      cols.map((col, i) => (i === colIndex ? { ...col, activeEntryPath: entry.path } : col))
    );
    setSelectedFile(entry.kind === "found" ? entry : null);
  }, []);

  // ── キーボードナビゲーション ──
  const handleKeyDown = useCallback((e: React.KeyboardEvent) => {
    if (e.key === "ArrowDown") {
      e.preventDefault();
      setSelectedIndex(i => Math.min(i + 1, results.length - 1));
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      setSelectedIndex(i => Math.max(i - 1, 0));
    }
  }, [results.length]);

  // ── メニューバー/検索ボックスの重なり検知 ──
  // メニュー項目は固定なので、表示中に一度だけ自然幅を計測してキャッシュする
  const menuNaturalWidthRef = useRef<number | null>(null);

  useLayoutEffect(() => {
    const titlebarEl = titlebarRef.current;
    const searchEl   = searchRef.current;
    if (!titlebarEl || !searchEl) return;

    const MARGIN = 16; // メニューと検索ボックスの間に最低限確保する余白(px)

    const check = () => {
      const menuEl = menuRef.current;
      if (menuEl && !menuEl.hidden) {
        menuNaturalWidthRef.current = menuEl.getBoundingClientRect().width;
      }
      const menuWidth = menuNaturalWidthRef.current;
      if (menuWidth == null) return;

      const logoWidth = titlebarEl.querySelector(".titlebar-logo")?.getBoundingClientRect().width ?? 0;
      const menuLeft  = (titlebarEl.querySelector(".titlebar-logo")?.getBoundingClientRect().right ?? logoWidth);
      const menuRight = menuLeft + menuWidth;
      const searchLeft = searchEl.getBoundingClientRect().left;

      setMenuCollapsed(menuRight + MARGIN > searchLeft);
    };

    const ro = new ResizeObserver(check);
    ro.observe(titlebarEl);
    check();

    return () => ro.disconnect();
  }, []);

  // ── ペイン分割リサイズ ──
  const onResizerMouseDown = (e: React.MouseEvent) => {
    const startX  = e.clientX;
    const startW  = leftPaneRef.current?.getBoundingClientRect().width ?? 520;

    const onMove = (ev: MouseEvent) => {
      const nw = Math.min(Math.max(startW + (ev.clientX - startX), 200), window.innerWidth - 280);
      if (leftPaneRef.current) leftPaneRef.current.style.width = `${nw}px`;
    };
    const onUp = () => {
      document.body.style.cursor     = "";
      document.body.style.userSelect = "";
      window.removeEventListener("mousemove", onMove);
      window.removeEventListener("mouseup", onUp);
    };
    document.body.style.cursor     = "col-resize";
    document.body.style.userSelect = "none";
    window.addEventListener("mousemove", onMove);
    window.addEventListener("mouseup", onUp);
  };

  return (
    <div className="app-root" onKeyDown={handleKeyDown}>
      {/* ══ TITLEBAR ══ */}
      <header className="titlebar" role="banner" ref={titlebarRef}>
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
            <button key={item} className="menu-btn" type="button">{item}</button>
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
              <button key={item} className="menu-btn" role="menuitem" type="button">{item}</button>
            ))}
          </div>
        )}

        {/* 検索（タイトルバー中央） */}
        <div className="titlebar-search-center" ref={searchRef}>
          <SearchBar
            value={query}
            onChange={setQuery}
            onSubmit={handleSearch}
            isLoading={isLoading}
          />
        </div>

        <WindowControls />
      </header>

      {/* ══ MAIN ══ */}
      <main className="main" role="main">
        {/* 左ペイン: 検索結果リスト */}
        <div ref={leftPaneRef} style={{ width: 520 }}>
          <ResultList
            results={results}
            selectedIndex={selectedIndex}
            onSelect={setSelectedIndex}
            onOpen={(r) => console.info("open:", r.path)}
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

        {/* 右ペイン: パスバー + カラムエクスプローラー（Phase4実装予定） */}
        <div className="right-pane" id="rightPane">
          <div className="path-bar">
            <span className="path-label">ルート</span>
            <input
              className="path-input"
              value={rootInput}
              onChange={(e) => setRootInput(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === "Enter") handleApplyRoot();
              }}
              placeholder="未指定（全体検索）"
              aria-label="ルートパス"
            />
            <button className="btn-sm" type="button" onClick={handleBrowseRoot}>参照</button>
            <button className="btn-primary" type="button" onClick={handleApplyRoot}>適用</button>
          </div>

          {/* 探索型カラムUI: A*探索ログをリアルタイム展開（design.md §3.3） */}
          <ColumnView
            columns={columns}
            onEntrySelect={handleColumnEntrySelect}
            selectedFile={selectedFile}
          />
        </div>
      </main>

      {/* ══ STATUS BAR ══ */}
      <footer className="statusbar" role="status" aria-live="polite" aria-atomic>
        <span className={`status-dot${isLoading ? " loading" : ""}`} aria-hidden/>
        <span className="status-text">
          {isLoading
            ? "検索中…"
            : elapsed === 0
              ? "検索ワードを入力してください"
              : `${results.length}件 · ${elapsed.toFixed(2)}s`}
        </span>
      </footer>
    </div>
  );
}
