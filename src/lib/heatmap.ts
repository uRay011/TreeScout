/**
 * ヒートマップ色設計：スコア→輝度マッピング
 *
 * 設計仕様（design.md §3.5）:
 *   background: hsl(220, 80%, calc(20% + score * 60%))
 *   score=0.97 → hsl(220,80%,78%) 明るいブルー
 *   score=0.30 → hsl(220,80%,38%) 暗いブルー
 *   score=0.00 → 無色（背景デフォルト）
 *
 * スコア帯ティア:
 *   NONE  : score < 0.30  → 無色（背景デフォルト）
 *   LOW   : 0.30 ≤ score < 0.60  → ░░░░ 薄いブルー
 *   MID   : 0.60 ≤ score < 0.90  → ▓▓▓▓ 中間ブルー
 *   HIGH  : 0.90 ≤ score          → ████ 明るいブルー
 */

export type HeatTier = "none" | "low" | "mid" | "high";

/** スコア値（0.0〜1.0）から CSS HSL 色文字列を返す。score < 0.3 は null（無色）。 */
export function scoreToColor(score: number): string | null {
  if (score < 0.3) return null;
  // 輝度: 20% + score * 60%  →  範囲 [38%, 78%]
  const lightness = 20 + score * 60;
  return `hsl(220, 80%, ${lightness.toFixed(1)}%)`;
}

/** スコアのティアを返す。アイコン・aria ラベル・クラス名に使用する。 */
export function scoreTier(score: number): HeatTier {
  if (score >= 0.9) return "high";
  if (score >= 0.6) return "mid";
  if (score >= 0.3) return "low";
  return "none";
}

/** ティアに対応する aria ラベル文字列（アクセシビリティ用）。 */
export const TIER_LABELS: Record<HeatTier, string> = {
  high: "スコア高（0.9以上）",
  mid:  "スコア中（0.6〜0.9）",
  low:  "スコア低（0.3〜0.6）",
  none: "スコアなし（0.3未満）",
};

/**
 * スコアをバックグラウンド輝度に変換し、可読テキスト色を返す。
 * ダークテーマ前提で輝度 < 50% なら明色テキスト、>= 50% なら暗色テキストを選択する。
 */
export function scoreToTextColor(score: number): string {
  const lightness = 20 + score * 60;
  return lightness >= 55 ? "hsl(220, 15%, 12%)" : "hsl(220, 20%, 90%)";
}

/** CSS変数として注入するインラインスタイルオブジェクトを生成する。
 *  GPU合成を維持するため `opacity` のみを変数で制御し、背景色は別途 CSS で hsl() を使う。 */
export function heatmapStyle(score: number): React.CSSProperties {
  const color = scoreToColor(score);
  if (!color) {
    return { "--heat-bg": "transparent", "--heat-opacity": "0" } as React.CSSProperties;
  }
  const lightness = 20 + score * 60;
  return {
    "--heat-bg":      `hsl(220, 80%, ${lightness.toFixed(1)}%)`,
    "--heat-opacity": "1",
    "--heat-text":    scoreToTextColor(score),
  } as React.CSSProperties;
}
