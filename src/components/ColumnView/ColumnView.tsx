import { useRef, useEffect, useCallback } from "react";
import { AnimatePresence, motion } from "framer-motion";
import { AstarColumn, AstarEntry } from "../../lib/tauri";
import { ColumnPanel } from "./ColumnPanel";

interface Props {
  columns: AstarColumn[];
  /** カラムのエントリ選択時 → 親が columns を更新してカラムを追加/削除する */
  onEntrySelect: (colIndex: number, entry: AstarEntry) => void;
  /** 選択されたファイルエントリ（詳細カードに表示） */
  selectedFile: AstarEntry | null;
}

export function ColumnView({ columns, onEntrySelect, selectedFile }: Props) {
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

  return (
    <div className="columns-scroll" ref={scrollRef} role="tree" aria-label="探索カラムビュー"><AnimatePresence initial={false}>
        {columns.map((col, i) => (
          <ColumnPanel
            key={col.id}
            column={col}
            colIndex={i}
            onEntrySelect={handleEntrySelect}
          />
        ))}
      </AnimatePresence>

      {/* 詳細カード：ファイル選択時に右端に表示 */}
      <AnimatePresence>
        {selectedFile && (
          <motion.div
            className="col-panel detail-card"
            role="region"
            aria-label="ファイル詳細"
            initial={{ opacity: 0, x: -20 }}
            animate={{ opacity: 1, x: 0 }}
            exit={{ opacity: 0, x: -10 }}
            transition={{ duration: 0.2, ease: "easeOut" }}
          ><div className="col-head">{selectedFile.name}</div><div className="col-body" style={{ padding: "12px" }}><div className="detail-filename">{selectedFile.name}</div><div className="detail-table">
                {selectedFile.ext && (
                  <div className="detail-row"><span className="detail-key">形式</span><span className="detail-val">{selectedFile.ext}</span></div>
                )}
                <div className="detail-row"><span className="detail-key">スコア</span><span className="detail-val">★ {selectedFile.score.toFixed(3)}</span></div><div className="detail-row"><span className="detail-key">パス</span><span className="detail-val" style={{ wordBreak: "break-all", fontSize: "10px" }}>
                    {selectedFile.path}
                  </span></div></div>

              {/* AIガイドパスライン: score が高い時に光るアニメーション */}
              {selectedFile.score >= 0.8 && (
                <motion.div
                  className="guide-path-glow"
                  aria-label="AIガイドパス: 高スコアファイル"
                  initial={{ opacity: 0, scaleX: 0 }}
                  animate={{ opacity: 1, scaleX: 1 }}
                  transition={{ duration: 0.35, ease: "easeOut", delay: 0.1 }}
                />
              )}
            </div></motion.div>
        )}
      </AnimatePresence>

      {/* 空状態 */}
      {columns.length === 0 && (
        <div className="col-panel phase4-placeholder" aria-label="空のカラムビュー"><div className="col-head">A* 探索カラム</div><div className="col-body" style={{ padding: "16px", color: "var(--text2)", fontSize: "11px", lineHeight: "1.7" }}>
            検索を実行すると

            A*探索パスがここに

            リアルタイム展開されます
          </div></div>
      )}
    </div>
  );
}
