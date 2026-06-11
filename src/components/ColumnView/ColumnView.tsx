import { useRef, useEffect, useCallback } from "react";
import { AnimatePresence } from "framer-motion";
import { Sparkles } from "lucide-react";
import { AstarColumn, AstarEntry } from "../../lib/tauri";
import { ColumnPanel } from "./ColumnPanel";

interface Props {
  columns: AstarColumn[];
  /** カラムのエントリ選択時 → 親が columns を更新してカラムを追加/削除する */
  onEntrySelect: (colIndex: number, entry: AstarEntry) => void;
  /** AIガイドパスラインの点灯（選択中ファイルのスコア >= 0.8） */
  showGuide: boolean;
}

export function ColumnView({ columns, onEntrySelect, showGuide }: Props) {
  const scrollRef = useRef<HTMLDivElement>(null);

  // カラムが追加されたら右端へ自動スクロール
  useEffect(() => {
    const el = scrollRef.current;
    if (!el) return;
    el.scrollTo({ left: el.scrollWidth, behavior: "smooth" });
  }, [columns.length]);

  const handleEntrySelect = useCallback(
    (colIndex: number, entry: AstarEntry) => {
      onEntrySelect(colIndex, entry);
    },
    [onEntrySelect]
  );

  if (columns.length === 0) {
    return (
      <div className="columns-scroll" ref={scrollRef}>
        <div className="empty-state">
          <div className="empty-icon"><Sparkles aria-hidden width={24} height={24} strokeWidth={1.5} /></div>
          <div className="empty-title">検索を実行するとA*探索の過程を表示します</div>
        </div>
      </div>
    );
  }

  return (
    <div className="columns-scroll" ref={scrollRef} role="tree" aria-label="探索カラムビュー">
      <AnimatePresence initial={false}>
        {columns.map((col, i) => (
          <ColumnPanel
            key={col.id}
            column={col}
            colIndex={i}
            onEntrySelect={handleEntrySelect}
            isLast={i === columns.length - 1}
            showGuide={showGuide}
          />
        ))}
      </AnimatePresence>
    </div>
  );
}
