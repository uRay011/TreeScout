import { motion } from "framer-motion";
import { AstarColumn, AstarEntry } from "../../lib/tauri";
import { HeatmapItem } from "./HeatmapItem";

interface Props {
  column: AstarColumn;
  colIndex: number;
  onEntrySelect: (colIndex: number, entry: AstarEntry) => void;
}

export function ColumnPanel({ column, colIndex, onEntrySelect }: Props) {
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
      <div className="col-head">{column.label}</div>

      {/* アイテムリスト */}
      <div className="col-body">
        {column.entries.map((entry, i) => (
          <HeatmapItem
            key={entry.path}
            entry={entry}
            isActive={column.activeEntryPath === entry.path}
            index={i}
            onSelect={(e) => onEntrySelect(colIndex, e)}
          />
        ))}
      </div></motion.div>
  );
}
