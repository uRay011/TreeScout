import { useState, useRef, useEffect, useMemo } from "react";
import { useVirtualizer, observeElementOffset, elementScroll, type Virtualizer } from "@tanstack/react-virtual";
import { Search } from "lucide-react";
import { SearchResult } from "../../lib/tauri";
import { ViewMode } from "./ViewModePopup";

// 表示モードごとの行高（App.css の .file-row height と一致させる）
const ROW_HEIGHT: Record<ViewMode, number> = {
  compact: 28,
  standard: 40,
  detail: 44,
};

// WebView2(Chromium)のCSS要素高さ上限(約33.5Mpx)対策。
// 170万件×行高がこれを超えるとheightがブラウザにsilently clampされ、
// scrollHeightがそれ以上拡張されず末尾までスクロールできなくなる。
// この値未満にコンテナ高さを抑え、ネイティブscrollTopと仮想化の論理座標を
// ratio倍でリマッピングする（observeElementOffset/scrollToFn参照）。
const SAFE_HEIGHT = 8_000_000;

type ObserveOffsetCallback = (offset: number, isScrolling: boolean) => void;

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

/** バックエンド窓取得（useBrowseWindow）のデータソース。指定時はResultListが窓モードで動作する */
export interface WindowSource {
  /** browse() で確定した総件数 */
  total: number;
  /** index に対応する行を取得する（未取得ならundefined） */
  getRow: (i: number) => SearchResult | undefined;
  /** [lo, hi] の範囲を非同期で取得・キャッシュする */
  ensureRange: (lo: number, hi: number) => void;
}

// 可視範囲の前後に確保する先読みマージン（行数）
const ENSURE_MARGIN = 50;

interface Props {
  /** 配列モード（既存・semantic / listDirectory経路）の結果一覧 */
  results: SearchResult[];
  /** 指定時は窓モードで動作し、results は無視される */
  windowSource?: WindowSource;
  /** 窓モードのcontrolledソート（ヘッダ表示用） */
  sort?: { col: SortCol; asc: boolean };
  /** 窓モードでヘッダクリック時に呼ばれる（ローカルソートはしない） */
  onSortChange?: (col: SortCol) => void;
  selectedIndex: number;
  /** 編集メニュー「全て選択」による複数選択中の行インデックス集合 */
  selectedIndices?: Set<number>;
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

export function ResultList({ results, windowSource, sort, onSortChange, selectedIndex, selectedIndices, onSelect, onOpen, query, isLoading, hasSearched, viewMode }: Props) {
  const isWindowMode = windowSource !== undefined;
  // スコア降順をデフォルトにする（mock_v2.html J3 sortBy 準拠）
  const [localSortCol, setLocalSortCol] = useState<SortCol>("score");
  const [localSortAsc, setLocalSortAsc] = useState(false);
  const [isMd, setIsMd] = useState(false);
  const [isLg, setIsLg] = useState(false);
  const paneRef = useRef<HTMLDivElement>(null);
  const listRef = useRef<HTMLUListElement>(null);

  // 窓モードではcontrolled sort（親から渡される）、配列モードではローカルstate
  const sortCol = isWindowMode ? (sort?.col ?? "name") : localSortCol;
  const sortAsc = isWindowMode ? (sort?.asc ?? true) : localSortAsc;

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
    if (isWindowMode) {
      onSortChange?.(col);
      return;
    }
    if (localSortCol === col) setLocalSortAsc(a => !a);
    else { setLocalSortCol(col); setLocalSortAsc(col !== "score"); }
  };

  // results/sortCol/sortAsc が変わらない限り再ソートしない（大量件数時に選択操作のたびO(n log n)が走るのを防ぐ）
  // 窓モードではバックエンド側で索引ソート済みのため、ここではソートしない
  const sorted = useMemo(() => {
    if (isWindowMode) return results;
    return [...results].sort((a, b) => {
      let va: string | number, vb: string | number;
      switch (localSortCol) {
        case "name":   va = a.name;     vb = b.name;     break;
        case "folder": va = a.folder;   vb = b.folder;   break;
        case "size":   va = a.size;     vb = b.size;     break;
        case "date":   va = a.modified; vb = b.modified; break;
        default:       va = a.score ?? 0; vb = b.score ?? 0; break;
      }
      if (va < vb) return localSortAsc ? -1 : 1;
      if (va > vb) return localSortAsc ?  1 : -1;
      return 0;
    });
  }, [results, isWindowMode, localSortCol, localSortAsc]);

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
  // 窓モード（フィルタ結果）は類似度を持たないため常に非表示
  const hasScore = !isWindowMode && query.trim() !== "";

  const itemCount = isWindowMode ? windowSource!.total : sorted.length;

  // ネイティブscrollTopの可動範囲 [0, containerHeight] と、仮想化の論理座標
  // [0, realTotal] をratio倍でリマッピングする（SAFE_HEIGHT超のときのみ縮小）
  const rowHeight = ROW_HEIGHT[viewMode];
  const realTotal = itemCount * rowHeight;
  const ratio = Math.max(1, realTotal / SAFE_HEIGHT);
  const containerHeight = realTotal / ratio;

  // observeElementOffset/scrollToFnはscrollElement確定時に一度だけ登録されるため、
  // クロージャ内では最新のratio/realTotalをrefから読む
  const ratioRef = useRef(ratio);
  ratioRef.current = ratio;
  const realTotalRef = useRef(realTotal);
  realTotalRef.current = realTotal;

  const customObserveElementOffset = useMemo(
    () => (instance: Virtualizer<HTMLUListElement, Element>, cb: ObserveOffsetCallback) =>
      observeElementOffset(instance, (offset, isScrolling) => {
        const r = ratioRef.current;
        const element = instance.scrollElement;
        if (r > 1 && element) {
          // scrollTop*ratioでは末尾でrealTotal-viewport*ratio止まりとなり、
          // 末尾付近の行が仮想化範囲に入らず常に描画されない隙間ができる。
          // ネイティブscrollTopが上限に達したら論理offsetをrealTotal-viewportに
          // 固定し、最終行まで仮想化範囲に含める。
          const maxScrollTop = element.scrollHeight - element.clientHeight;
          if (maxScrollTop > 0 && offset >= maxScrollTop - 1) {
            cb(realTotalRef.current - element.clientHeight, isScrolling);
            return;
          }
        }
        cb(offset * r, isScrolling);
      }),
    []
  );

  const customScrollToFn = useMemo(
    () => (offset: number, options: { adjustments?: number; behavior?: ScrollBehavior }, instance: Virtualizer<HTMLUListElement, Element>) =>
      elementScroll(offset / ratioRef.current, options, instance),
    []
  );

  // 大量件数（全体検索時は数十万件規模）でもDOMノード数を一定に保つための仮想スクロール
  const rowVirtualizer = useVirtualizer({
    count: itemCount,
    getScrollElement: () => listRef.current,
    estimateSize: () => rowHeight,
    overscan: 10,
    observeElementOffset: customObserveElementOffset,
    scrollToFn: customScrollToFn,
    // 窓モードは未取得→取得済みでpathが変わるとキーが入れ替わり、
    // measurementsCache/elementsCacheの不整合(スケルトン残留)を招くためindex固定
    getItemKey: (index) => isWindowMode ? index : sorted[index].path,
  });

  // キーボード操作で選択行が変わったら表示範囲外でもスクロール追従させる
  useEffect(() => {
    if (selectedIndex >= 0) rowVirtualizer.scrollToIndex(selectedIndex, { align: "auto" });
  }, [selectedIndex, rowVirtualizer]);

  // 窓モード: 可視範囲＋先読みマージン分をensureRangeでバックエンドから取得する
  const virtualItems = rowVirtualizer.getVirtualItems();
  useEffect(() => {
    if (!isWindowMode || virtualItems.length === 0) return;
    const lo = virtualItems[0].index - ENSURE_MARGIN;
    const hi = virtualItems[virtualItems.length - 1].index + ENSURE_MARGIN;
    windowSource!.ensureRange(lo, hi);
  }, [isWindowMode, windowSource, virtualItems]);

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
      <ul className="file-list" role="listbox" aria-label="検索結果" ref={listRef}>
        {isLoading && itemCount === 0 && SKELETON_WIDTHS.map((w, i) => (
          <li className="sk-row" key={i} aria-hidden>
            {hasScore && <span className="sk-block sk-score" />}
            <span className="sk-block sk-badge" />
            <span className="sk-block sk-line" style={{ flex: `0 0 ${w}%` }} />
          </li>
        ))}
        {!isLoading && itemCount === 0 && (
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
        {itemCount > 0 && (
          <div className="file-list-virtual" style={{ height: containerHeight }}>
            {(() => {
              // 仮想化の論理座標(item.start)を現在のscrollTop基準の相対座標に変換する。
              // ratio===1のときは renderTop === item.start に退化する。
              // ネイティブscrollTop上限ではvirtualOffsetがrealTotal-viewportに
              // 補正される（customObserveElementOffset）ため、scrollTop側も
              // ネイティブ上限でクランプして表示位置のズレを防ぐ
              const virtualOffset = rowVirtualizer.scrollOffset ?? 0;
              const viewportHeight = rowVirtualizer.scrollRect?.height ?? 0;
              // containerHeight < viewportHeight（結果件数が少ない）の場合、上限が負になり
              // 全行がtranslateYで画面上方向に押し出されて空欄に見えるため、下限0でクランプする
              const scrollTop = Math.min(virtualOffset / ratio, Math.max(0, containerHeight - viewportHeight));
              return virtualItems.map((vRow) => {
                const i = vRow.index;
                const r = isWindowMode ? windowSource!.getRow(i) : sorted[i];
                const renderTop = scrollTop + (vRow.start - virtualOffset);

                if (!r) {
                  // 窓モードで未取得の行はスケルトンを表示する（仮想化のtransformを適用）
                  const w = SKELETON_WIDTHS[i % SKELETON_WIDTHS.length];
                  return (
                    <li
                      className="sk-row"
                      key={i}
                      aria-hidden
                      style={{
                        position: "absolute",
                        top: 0,
                        left: 0,
                        width: "100%",
                        height: vRow.size,
                        transform: `translateY(${renderTop}px)`,
                      }}
                    >
                      <span className="sk-block sk-badge" />
                      <span className="sk-block sk-line" style={{ flex: `0 0 ${w}%` }} />
                    </li>
                  );
                }

                const score = r.score ?? 0;
                const isSelected = selectedIndex === i || (selectedIndices?.has(i) ?? false);
                return (
                  <li
                    key={r.path}
                    role="option"
                    aria-selected={isSelected}
                    className={`file-row${isSelected ? " selected" : ""}`}
                    style={{
                      ...(hasScore ? { "--heat-bg": heatBg(score) } as React.CSSProperties : undefined),
                      height: vRow.size,
                      transform: `translateY(${renderTop}px)`,
                    }}
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
                        {r.is_suggestion && (
                          <span className="ai-badge" title="AIサジェスト: Everythingの絞り込み候補外からA*が発見">AI</span>
                        )}
                        <span className="row-file">{r.name}</span>
                      </div>
                      <div className="row-folder">{r.folder}</div>
                    </div>
                    <div className="row-folder-col">{r.folder}</div>
                    <div className="row-size">{r.is_dir ? "" : formatSize(r.size)}</div>
                    <div className="row-date">{r.modified}</div>
                  </li>
                );
              });
            })()}
          </div>
        )}
      </ul>
    </div>
  );
}
