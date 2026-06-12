import { motion } from "framer-motion";
import { ArrowRight, ChevronRight } from "lucide-react";
import { AstarEntry } from "../../lib/tauri";
import { heatmapStyle, scoreTier, TIER_LABELS } from "../../lib/heatmap";

// フォルダSVG
function IconFolder({ color }: { color: string }) {
  return (
    <svg width="13" height="13" viewBox="0 0 16 16" fill={color} aria-hidden><path d="M1.5 3.5A1 1 0 012.5 2.5h3.086a1 1 0 01.707.293l1.121 1.121A1 1 0 008.121 4.5H13.5A1 1 0 0114.5 5.5v7a1 1 0 01-1 1h-11a1 1 0 01-1-1v-9z"/></svg>
  );
}

// ファイルSVG
function IconFile({ color }: { color: string }) {
  return (
    <svg width="12" height="12" viewBox="0 0 16 16" fill={color} aria-hidden><path d="M4 1.5A1.5 1.5 0 002.5 3v10A1.5 1.5 0 004 14.5h8A1.5 1.5 0 0013.5 13V5.5L10 2H4zm5.5 4V2.25L12.75 5.5H9.5z"/></svg>
  );
}

// 拡張子 → アイコン色
const EXT_COLORS: Record<string, string> = {
  tsx: "#58a6ff", ts: "#3b82f6", css: "#7ee8fa",
  md: "#768390", json: "#e3b341", rs: "#f07178", toml: "#e8b250",
};
function fileColor(ext: string): string {
  return EXT_COLORS[ext.toLowerCase()] ?? "#6e7681";
}

interface Props {
  entry: AstarEntry;
  isActive: boolean;
  index: number;
  onSelect: (entry: AstarEntry) => void;
  /** 検索キーワード未入力時はスコア0=「最低スコア」ではなく「スコアなし」として無着色にする */
  hasScore: boolean;
}

export function HeatmapItem({ entry, isActive, index, onSelect, hasScore }: Props) {
  const heatStyle = hasScore ? heatmapStyle(entry.score, entry.kind) : undefined;
  const isSkipped = entry.kind === "skipped";
  const tier = scoreTier(entry.score);
  const heatLabel = hasScore && entry.kind === "found" ? `, ${TIER_LABELS[tier]}` : "";

  return (
    <motion.button
      type="button"
      role="option"
      aria-selected={isActive}
      aria-label={`${entry.name}${heatLabel}`}
      className={`col-item${isActive ? " active" : ""}${isSkipped ? " skip" : ""}`}
      onClick={() => onSelect(entry)}
      // 入場アニメーション: 左からスライドイン + フェード
      initial={{ opacity: 0, x: -8 }}
      animate={{ opacity: 1, x: 0 }}
      transition={{
        duration: 0.18,
        delay: index * 0.04,   // 30~50ms ずつずらしてスタッガー
        ease: "easeOut",
      }}
      // ヒートマップ背景はオーバーレイ（GPU合成、--heat-* はCSS変数経由でopacityのみ制御）
      style={{ position: "relative", width: "100%", textAlign: "left", ...heatStyle }}
    >
      {/* ヒートマップ背景オーバーレイ（色・最終濃度はCSS変数 --heat-bg / --heat-opacity 経由） */}
      {hasScore && (
        <motion.span
          className="heat-overlay"
          aria-hidden
          initial={{ opacity: 0 }}
          animate={{ opacity: 1 }}
          transition={{ duration: 0.25, delay: index * 0.04 }}
        />
      )}

      {/* 左端ヒートバー：面のopacityでは潰れる低スコア帯を輝度差で判別させる */}
      {hasScore && <span className="item-heatbar" aria-hidden />}

      {/* アイコン */}
      <span className="col-icon">
        {entry.is_dir
          ? <IconFolder color="#c69026" />
          : <IconFile   color={fileColor(entry.ext)} />}
      </span>

      {/* 名前 */}
      <span className="col-name">
        {entry.name}
      </span>

      {/* フォルダ展開シェブロン */}
      {entry.is_dir && (
        <span className="col-chevron" aria-hidden>
          <ChevronRight width={10} height={10} strokeWidth={1.5} />
        </span>
      )}

      {/* スコアバッジ（found のみ表示） */}
      {hasScore && entry.kind === "found" && (
        <span className="col-score">{entry.score.toFixed(2)}</span>
      )}

      {/* アクティブパスのコネクタ（active時のみCSSで表示） */}
      <span className="col-connector" aria-hidden>
        <ArrowRight width={10} height={10} strokeWidth={1.5} />
      </span>
    </motion.button>
  );
}

