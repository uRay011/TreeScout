import { useState, useCallback, useRef } from "react";
import { SearchBar } from "./components/SearchBar/SearchBar";
import { ResultList } from "./components/SearchBar/ResultList";
import { searchFiles, SearchResult } from "./lib/tauri";
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

  // ペイン分割リサイズ
  const leftPaneRef  = useRef<HTMLDivElement>(null);
  const resizerRef   = useRef<HTMLDivElement>(null);

  const handleSearch = useCallback(async () => {
    if (!query.trim()) return;
    setIsLoading(true);
    const t0 = performance.now();
    try {
      const items = await searchFiles(query);
      // size/modified は現状 Everything から取れないためデフォルト値を補完
      const normalized = items.map(r => ({
        ...r,
        size: r.size ?? 0,
        modified: r.modified ?? "",
      }));
      setResults(normalized);
      setElapsed((performance.now() - t0) / 1000);
    } catch {
      setResults([]);
      setElapsed((performance.now() - t0) / 1000);
    } finally {
      setIsLoading(false);
    }
  }, [query]);

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

  const selectedResult = results[selectedIndex] ?? null;

  return (
    <div className="app-root" onKeyDown={handleKeyDown}>
      {/* ══ TITLEBAR ══ */}
      <header className="titlebar" role="banner">
        <div className="titlebar-logo" aria-label="TreeScout">
          <LogoMark />
          <span className="logo-name">TreeScout</span>
        </div>

        <nav className="titlebar-menu" aria-label="メインメニュー" id="titlebarMenu">
          {MENU_ITEMS.map(item => (
            <button key={item} className="menu-btn" type="button">{item}</button>
          ))}
        </nav>

        {/* ハンバーガー（幅不足時） */}
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
        {menuOpen && (
          <div className="menu-dropdown open" id="menuDropdown" role="menu">
            {MENU_ITEMS.map(item => (
              <button key={item} className="menu-btn" role="menuitem" type="button">{item}</button>
            ))}
          </div>
        )}

        {/* 検索（タイトルバー中央） */}
        <div className="titlebar-search-center">
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
              defaultValue=""
              aria-label="ルートパス"
              readOnly
            />
            <button className="btn-sm" type="button">参照</button>
            <button className="btn-primary" type="button">適用</button>
          </div>

          {/* カラムエクスプローラー（Phase4実装予定） */}
          <div className="columns-scroll">
            <div className="col-panel phase4-placeholder">
              <div className="col-head">Phase 4 実装予定</div>
              <div className="col-body" style={{ padding: "16px", color: "var(--text2)", fontSize: "11px", lineHeight: "1.6" }}>
                探索型カラム UI・ヒートマップ・<br/>AIガイドパスラインは<br/>Phase 4 で実装します。
              </div>
            </div>
            {/* 詳細カード */}
            {selectedResult && (
              <div className="col-panel detail-card">
                <div className="col-head">{selectedResult.name}</div>
                <div className="col-body">
                  <div className="detail-filename">{selectedResult.name}</div>
                  <div className="detail-table">
                    <div className="detail-row">
                      <span className="detail-key">形式</span>
                      <span className="detail-val">{selectedResult.ext}</span>
                    </div>
                    <div className="detail-row">
                      <span className="detail-key">サイズ</span>
                      <span className="detail-val">
                        {selectedResult.size < 1024
                          ? `${selectedResult.size} B`
                          : `${(selectedResult.size / 1024).toFixed(1)} KB`}
                      </span>
                    </div>
                    <div className="detail-row">
                      <span className="detail-key">更新日</span>
                      <span className="detail-val">{selectedResult.modified}</span>
                    </div>
                    <div className="detail-row">
                      <span className="detail-key">パス</span>
                      <span className="detail-val" style={{ wordBreak: "break-all" }}>{selectedResult.path}</span>
                    </div>
                  </div>
                </div>
              </div>
            )}
          </div>
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
