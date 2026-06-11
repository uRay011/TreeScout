import { useState, useRef, useEffect } from "react";
import { Search } from "lucide-react";
import { SearchResult } from "../../lib/tauri";
import { ViewMode } from "./ViewModePopup";

type SortCol = "score" | "name" | "folder" | "size" | "date";

const EXT_CLASS: Record<string, string> = {
  tsx:  "ext-tsx",
  ts:   "ext-ts",
  css:  "ext-css",
  md:   "ext-md",
  json: "ext-json",
  rs:   "ext-rs",
  toml: "ext-toml",
};

function extClass(ext: string) {
  return EXT_CLASS[ext.toLowerCase()] ?? "ext-other";
}

function formatSize(bytes: number): string {
  if (Number.isNaN(bytes)) return "";
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / 1024 / 1024).toFixed(1)} MB`;
}

// ヒートマップ色: hsl(220,80%, 20%+score*60%)（mock_v2.html J2 と同一式）
function heatBg(score: number): string {
  return `hsl(220, 80%, ${(20 + score * 60).toFixed(1)}%)`;
}

function SortIcon() {
  return (
    <svg viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth={1.5} strokeLinecap="round" strokeLinejoin="round" aria-hidden>
      <path d="M8 12.5v-9"/>
      <path d="M4.5 6.5 8 3l3.5 3.5"/>
    </svg>
  );
}

// スケルトン行の幅バリエーション（mock_v2.html J2 renderSkeleton 準拠）
const SKELETON_WIDTHS = [72, 55, 80, 48, 64, 70];

interface Props {
  results: SearchResult[];
  selectedIndex: number;
  onSelect: (index: number) => void;
  onOpen: (result: SearchResult) => void;
  /** 検索クエリ（未入力時は空状態のメッセージを切り替える） */
  query: string;
  /** 検索中はスケルトン行を表示する */
  isLoading: boolean;
  /** 一度でも検索を実行したか（初回表示前のみ入力案内を表示する） */
  hasSearched: boolean;
  /** 左ペイン表示モード（コンパクト/標準/詳細） */
  viewMode: ViewMode;
}

export function ResultList({ results, selectedIndex, onSelect, onOpen, query, isLoading, hasSearched, viewMode }: Props) {
  // スコア降順をデフォルトにする（mock_v2.html J3 sortBy 準拠）
  const [sortCol, setSortCol] = useState<SortCol>("score");
  const [sortAsc, setSortAsc] = useState(false);
  const [isMd, setIsMd] = useState(false);
  const [isLg, setIsLg] = useState(false);
  const paneRef = useRef<HTMLDivElement>(null);

  // 左ペイン幅に応じてサイズ・更新日時列の表示を切替
  useEffect(() => {
    const el = paneRef.current;
    if (!el) return;
    const ro = new ResizeObserver((entries) => {
      const w = entries[0].contentRect.width;
      setIsMd(w >= 380);
      setIsLg(w >= 460);
    });
    ro.observe(el);
    return () => ro.disconnect();
  }, []);

  const handleSort = (col: SortCol) => {
    if (sortCol === col) setSortAsc(a => !a);
    else { setSortCol(col); setSortAsc(col !== "score"); }
  };

  const sorted = [...results].sort((a, b) => {
    let va: string | number, vb: string | number;
    switch (sortCol) {
      case "name":   va = a.name;     vb = b.name;     break;
      case "folder": va = a.folder;   vb = b.folder;   break;
      case "size":   va = a.size;     vb = b.size;     break;
      case "date":   va = a.modified; vb = b.modified; break;
      default:       va = a.score ?? 0; vb = b.score ?? 0; break;
    }
    if (va < vb) return sortAsc ? -1 : 1;
    if (va > vb) return sortAsc ?  1 : -1;
    return 0;
  });

  const ColHeader = ({ col, label, className }: { col: SortCol; label: string; className: string }) => {
    const isSorted = sortCol === col;
    return (
      <button
        type="button"
        className={`col-h ${className}${isSorted ? " sorted" : ""}${isSorted && !sortAsc ? " desc" : ""}`}
        onClick={() => handleSort(col)}
        aria-sort={isSorted ? (sortAsc ? "ascending" : "descending") : "none"}
      >
        <span>{label}</span>
        <span className="sort-ico"><SortIcon /></span>
      </button>
    );
  };

  const viewClass = viewMode === "compact" ? " view-compact" : viewMode === "detail" ? " view-detail" : "";

  // 検索キーワード未入力時（Everythingの全件表示）はスコアが付与されないため非表示にする
  const hasScore = query.trim() !== "";

  return (
    <div className={`left-pane${isMd ? " w-md" : ""}${isLg ? " w-lg" : ""}${viewClass}`} id="leftPane" ref={paneRef}>
      {/* ── ヘッダー ── */}
      <div className="list-header" role="row">
        {hasScore && <ColHeader col="score"  label="一致"     className="col-h-score" />}
        <ColHeader col="name"   label="名前"     className="col-h-name" />
        <ColHeader col="folder" label="フォルダ" className="col-h-folder" />
        <ColHeader col="size"   label="サイズ"   className="col-h-size" />
        <ColHeader col="date"   label="更新日時" className="col-h-date" />
      </div>

      {/* ── ファイルリスト ── */}
      <ul className="file-list" role="listbox" aria-label="検索結果">
        {isLoading && sorted.length === 0 && SKELETON_WIDTHS.map((w, i) => (
          <li className="sk-row" key={i} aria-hidden>
            {hasScore && <span className="sk-block sk-score" />}
            <span className="sk-block sk-badge" />
            <span className="sk-block sk-line" style={{ flex: `0 0 ${w}%` }} />
          </li>
        ))}
        {!isLoading && sorted.length === 0 && (
          <li className="empty-state" role="option" aria-disabled>
            <div className="empty-icon"><Search aria-hidden width={24} height={24} strokeWidth={1.5} /></div>
            {!hasSearched && query.trim() === "" ? (
              <>
                <div className="empty-title">検索キーワードを入力してください</div>
                <div className="empty-hint"><kbd>Ctrl</kbd><kbd>F</kbd> で検索 / <kbd>Enter</kbd> で実行</div>
              </>
            ) : (
              <div className="empty-title">一致する結果が見つかりませんでした</div>
            )}
          </li>
        )}
        {sorted.map((r, i) => {
          const score = r.score ?? 0;
          return (
            <li
              key={r.path}
              role="option"
              aria-selected={selectedIndex === i}
              className={`file-row${selectedIndex === i ? " selected" : ""}`}
              style={hasScore ? ({ "--heat-bg": heatBg(score) } as React.CSSProperties) : undefined}
              onClick={() => onSelect(i)}
              onDoubleClick={() => onOpen(r)}
              onKeyDown={(e) => {
                if (e.key === "Enter") onOpen(r);
              }}
              tabIndex={selectedIndex === i ? 0 : -1}
            >
              {hasScore && <span className="row-heatbar" aria-hidden />}
              {hasScore && (
                <div className="row-score">
                  <span className="row-score-num">{score.toFixed(2)}</span>
                  <div className="row-score-bar">
                    <div className="row-score-fill" style={{ transform: `scaleX(${score})` }} />
                  </div>
                </div>
              )}
              <div className="row-main">
                <div className="row-name">
                  <span className={`ext-badge ${extClass(r.ext)}`}>{r.ext.toUpperCase().slice(0, 4)}</span>
                  <span className="row-file">{r.name}</span>
                </div>
                <div className="row-folder">{r.folder}</div>
              </div>
              <div className="row-folder-col">{r.folder}</div>
              <div className="row-size">{formatSize(r.size)}</div>
              <div className="row-date">{r.modified}</div>
            </li>
          );
        })}
      </ul>
    </div>
  );
}
