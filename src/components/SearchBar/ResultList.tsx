import { useState, useRef, useCallback } from "react";
import { SearchResult } from "../../lib/tauri";

type SortCol = "name" | "folder" | "size" | "date";

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
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / 1024 / 1024).toFixed(1)} MB`;
}

interface ColWidths {
  folder: number;
  size:   number;
  date:   number;
}

interface Props {
  results: SearchResult[];
  selectedIndex: number;
  onSelect: (index: number) => void;
  onOpen: (result: SearchResult) => void;
}

export function ResultList({ results, selectedIndex, onSelect, onOpen }: Props) {
  const [sortCol, setSortCol]   = useState<SortCol>("name");
  const [sortAsc, setSortAsc]   = useState(true);
  const [colWidths, setColWidths] = useState<ColWidths>({ folder: 150, size: 60, date: 86 });

  const resizingRef  = useRef<keyof ColWidths | null>(null);
  const startXRef    = useRef(0);
  const startWRef    = useRef(0);

  // ── ソート ──────────────────────────────────────────
  const handleSort = (col: SortCol) => {
    if (sortCol === col) setSortAsc(a => !a);
    else { setSortCol(col); setSortAsc(true); }
  };

  const sorted = [...results].sort((a, b) => {
    let va: string | number, vb: string | number;
    switch (sortCol) {
      case "name":   va = a.name;     vb = b.name;     break;
      case "folder": va = a.folder;   vb = b.folder;   break;
      case "size":   va = a.size;     vb = b.size;     break;
      case "date":   va = a.modified; vb = b.modified; break;
    }
    if (va < vb) return sortAsc ? -1 : 1;
    if (va > vb) return sortAsc ?  1 : -1;
    return 0;
  });

  // ── 列リサイズ ──────────────────────────────────────
  const onResizerMouseDown = useCallback((e: React.MouseEvent, col: keyof ColWidths) => {
    e.preventDefault();
    e.stopPropagation();
    resizingRef.current = col;
    startXRef.current   = e.clientX;
    startWRef.current   = colWidths[col];

    const onMove = (ev: MouseEvent) => {
      if (!resizingRef.current) return;
      const min = { folder: 80, size: 40, date: 60 }[resizingRef.current];
      const nw  = Math.max(startWRef.current + (ev.clientX - startXRef.current), min);
      setColWidths(prev => ({ ...prev, [resizingRef.current!]: nw }));
    };
    const onUp = () => {
      resizingRef.current = null;
      document.body.style.cursor     = "";
      document.body.style.userSelect = "";
      window.removeEventListener("mousemove", onMove);
      window.removeEventListener("mouseup", onUp);
    };
    document.body.style.cursor     = "col-resize";
    document.body.style.userSelect = "none";
    window.addEventListener("mousemove", onMove);
    window.addEventListener("mouseup", onUp);
  }, [colWidths]);

  // ── ヘッダーセル ──────────────────────────────────────
  const ColHeader = ({
    col, label, resizable = true,
  }: { col: SortCol; label: string; resizable?: boolean }) => (
    <div
      className="col-wrap"
      style={col !== "name" ? { width: colWidths[col as keyof ColWidths] } : undefined}
    >
      {col === "name" && <div className="col-wrap-name">
        <button
          type="button"
          className={`col-h${sortCol === col ? " sorted" : ""}`}
          onClick={() => handleSort(col)}
          aria-sort={sortCol === col ? (sortAsc ? "ascending" : "descending") : "none"}
        >
          <span>{label}</span>
          {sortCol === col && <span className="sort-arrow" aria-hidden>{sortAsc ? "↑" : "↓"}</span>}
        </button>
      </div>}
      {col !== "name" && <>
        <button
          type="button"
          className={`col-h${sortCol === col ? " sorted" : ""}`}
          onClick={() => handleSort(col)}
          aria-sort={sortCol === col ? (sortAsc ? "ascending" : "descending") : "none"}
        >
          <span>{label}</span>
          {sortCol === col && <span className="sort-arrow" aria-hidden>{sortAsc ? "↑" : "↓"}</span>}
        </button>
        {resizable && (
          <div
            className="col-resizer"
            role="separator"
            aria-orientation="vertical"
            aria-label={`${label}列の幅を調整`}
            onMouseDown={(e) => onResizerMouseDown(e, col as keyof ColWidths)}
          />
        )}
      </>}
    </div>
  );

  return (
    <div className="left-pane" id="leftPane">
      {/* ── ヘッダー ── */}
      <div className="list-header" role="row">
        <div className="col-wrap col-wrap-name">
          <ColHeader col="name" label="名前" resizable={false} />
        </div>
        <div className="col-wrap" style={{ width: colWidths.folder }}>
          <button
            type="button"
            className={`col-h${sortCol === "folder" ? " sorted" : ""}`}
            onClick={() => handleSort("folder")}
            aria-sort={sortCol === "folder" ? (sortAsc ? "ascending" : "descending") : "none"}
          >
            <span>フォルダ</span>
            {sortCol === "folder" && <span className="sort-arrow" aria-hidden>{sortAsc ? "↑" : "↓"}</span>}
          </button>
          <div
            className="col-resizer"
            role="separator"
            aria-orientation="vertical"
            aria-label="フォルダ列の幅を調整"
            onMouseDown={(e) => onResizerMouseDown(e, "folder")}
          />
        </div>
        <div className="col-wrap" style={{ width: colWidths.size }}>
          <button
            type="button"
            className={`col-h${sortCol === "size" ? " sorted" : ""}`}
            onClick={() => handleSort("size")}
            aria-sort={sortCol === "size" ? (sortAsc ? "ascending" : "descending") : "none"}
          >
            <span>サイズ</span>
            {sortCol === "size" && <span className="sort-arrow" aria-hidden>{sortAsc ? "↑" : "↓"}</span>}
          </button>
          <div
            className="col-resizer"
            role="separator"
            aria-orientation="vertical"
            aria-label="サイズ列の幅を調整"
            onMouseDown={(e) => onResizerMouseDown(e, "size")}
          />
        </div>
        <div className="col-wrap" style={{ width: colWidths.date }}>
          <button
            type="button"
            className={`col-h${sortCol === "date" ? " sorted" : ""}`}
            onClick={() => handleSort("date")}
            aria-sort={sortCol === "date" ? (sortAsc ? "ascending" : "descending") : "none"}
          >
            <span>更新日時</span>
            {sortCol === "date" && <span className="sort-arrow" aria-hidden>{sortAsc ? "↑" : "↓"}</span>}
          </button>
          <div
            className="col-resizer"
            role="separator"
            aria-orientation="vertical"
            aria-label="更新日時列の幅を調整"
            onMouseDown={(e) => onResizerMouseDown(e, "date")}
          />
        </div>
      </div>

      {/* ── ファイルリスト ── */}
      <ul className="file-list" role="listbox" aria-label="検索結果">
        {sorted.length === 0 && (
          <li className="empty-state" role="option" aria-disabled>
            結果なし
          </li>
        )}
        {sorted.map((r, i) => (
          <li
            key={r.path}
            role="option"
            aria-selected={selectedIndex === i}
            className={`file-row${selectedIndex === i ? " selected" : ""}`}
            onClick={() => onSelect(i)}
            onDoubleClick={() => onOpen(r)}
            onKeyDown={(e) => {
              if (e.key === "Enter") onOpen(r);
            }}
            tabIndex={selectedIndex === i ? 0 : -1}
          >
            <div className="file-name">
              <span className={`ext-badge ${extClass(r.ext)}`}>{r.ext.toUpperCase().slice(0, 4)}</span>
              <span className="file-text">{r.name}</span>
            </div>
            <div className="folder-col rc-folder" style={{ width: colWidths.folder }}>{r.folder}</div>
            <div className="size-col rc-size"     style={{ width: colWidths.size   }}>{formatSize(r.size)}</div>
            <div className="date-col rc-date"     style={{ width: colWidths.date   }}>{r.modified}</div>
          </li>
        ))}
      </ul>
    </div>
  );
}
