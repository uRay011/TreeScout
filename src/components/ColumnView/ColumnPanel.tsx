import { memo } from "react";
import { motion } from "framer-motion";
import { AstarColumn, AstarEntry } from "../../lib/tauri";
import { HeatmapItem } from "./HeatmapItem";

interface Props {
  column: AstarColumn;
  colIndex: number;
  onEntrySelect: (colIndex: number, entry: AstarEntry) => void;
  /** 最終カラムかどうか（AIガイドパスラインの表示位置） */
  isLast: boolean;
  /** AIガイドパスラインの点灯（選択中ファイルのスコア >= 0.8） */
  showGuide: boolean;
  /** 検索キーワードが入力されているか（false時はヒートマップを無着色にする） */
  hasScore: boolean;
}

// memo化により、検索ボックスへの打鍵などでAppが再レンダーしても、column等のpropsが
// 変わらないカラムは再調整をスキップする（onEntrySelectは呼び出し側で安定参照にしてある）。
export const ColumnPanel = memo(function ColumnPanel({ column, colIndex, onEntrySelect, isLast, showGuide, hasScore }: Props) {
  return (
    // カラム全体が左からスライドイン（コラム単位のアニメーション）
    <motion.div
      className="col-panel"
      role="listbox"
      aria-label={column.label}
      initial={{ opacity: 0, x: -20 }}
      animate={{ opacity: 1, x: 0 }}
      exit={{ opacity: 0, x: -10 }}
      transition={{
        duration: 0.22,
        delay: colIndex * 0.06,  // カラムが追加されるたびに少し遅らせる
        ease: "easeOut",
      }}
    >
      {/* ヘッダー */}
      <div className="col-head">
        <span className="col-depth">{colIndex + 1}</span>
        <span className="col-head-label">{column.label}</span>
      </div>

      {/* アイテムリスト */}
      <div className="col-body">
        {column.entries.map((entry) => (
          <HeatmapItem
            key={entry.path}
            entry={entry}
            isActive={column.activeEntryPath === entry.path}
            colIndex={colIndex}
            onSelect={onEntrySelect}
            hasScore={hasScore}
          />
        ))}

        {/* AIガイドパスライン: 最終カラムの found 一覧の下端で発光 */}
        {isLast && (
          <div
            className={`guide-path-glow${showGuide ? " on" : ""}`}
            aria-label="AIガイドパス: 高スコアファイル"
          />
        )}
      </div></motion.div>
  );
});
