import { memo, type ReactElement } from "react";
import { ArrowRight, ChevronRight } from "lucide-react";
import { AstarEntry } from "../../lib/tauri";
import { heatmapStyle, scoreTier, TIER_LABELS } from "../../lib/heatmap";

// フォルダSVG（GitHub Octicon file-directory・currentColor）
function IconFolder() {
  return (
    <svg viewBox="0 0 16 16" fill="currentColor" aria-hidden><path d="M1.75 2A1.75 1.75 0 0 0 0 3.75v8.5C0 13.216.784 14 1.75 14h12.5A1.75 1.75 0 0 0 16 12.25v-7.5A1.75 1.75 0 0 0 14.25 3H7.81a.25.25 0 0 1-.177-.073L6.323 1.616A1.75 1.75 0 0 0 5.085 1H1.75ZM1.5 3.75a.25.25 0 0 1 .25-.25h3.335a.25.25 0 0 1 .177.073l1.31 1.311c.329.329.775.514 1.24.516H14.25a.25.25 0 0 1 .25.25v7.5a.25.25 0 0 1-.25.25H1.75a.25.25 0 0 1-.25-.25v-8.5Z"/></svg>
  );
}

// コードファイルSVG（stroke・currentColor）
function IconFileCode() {
  return (
    <svg viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round" aria-hidden><path d="M3.5 1.5h6L13 5v9a.5.5 0 0 1-.5.5h-9a.5.5 0 0 1-.5-.5v-12a.5.5 0 0 1 .5-.5Z"/><path d="M9.5 1.5V5h3.5"/><path d="M6 8 4.5 9.5 6 11"/><path d="M9 8l1.5 1.5L9 11"/></svg>
  );
}

// テキストファイルSVG（stroke・currentColor）
function IconFileText() {
  return (
    <svg viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round" aria-hidden><path d="M3.5 1.5h6L13 5v9a.5.5 0 0 1-.5.5h-9a.5.5 0 0 1-.5-.5v-12a.5.5 0 0 1 .5-.5Z"/><path d="M9.5 1.5V5h3.5"/><path d="M5 8h5"/><path d="M5 10.25h5"/><path d="M5 12.5h3"/></svg>
  );
}

// その他ファイルSVG（stroke・currentColor）
function IconFileGeneric() {
  return (
    <svg viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round" aria-hidden><path d="M3.5 1.5h6L13 5v9a.5.5 0 0 1-.5.5h-9a.5.5 0 0 1-.5-.5v-12a.5.5 0 0 1 .5-.5Z"/><path d="M9.5 1.5V5h3.5"/></svg>
  );
}

// アイコンの形（ファイル種別の見た目）を決める拡張子グループ
const CODE_SHAPE_EXT = new Set([
  "ts", "tsx", "js", "jsx", "mjs", "cjs", "css", "scss", "sass", "less",
  "html", "htm", "json", "json5", "jsonc", "jsonl", "ndjson", "yaml", "yml", "toml", "rs", "sh", "bash", "zsh", "ps1",
  "py", "pyw", "go",
  "c", "h", "cpp", "cc", "cxx", "hpp", "hh", "hxx", "cs",
  "java", "dart",
]);
const TEXT_SHAPE_EXT = new Set(["md", "markdown", "txt", "csv", "rtf"]);

// 拡張子 → 着色クラス（よく使うファイルはツール・サービスの連想色、マイナーな拡張子は無彩色のまま）
const EXT_COLOR_CLASS: Record<string, string> = {
  ts: "ico-ts", tsx: "ico-tsx",
  js: "ico-js", jsx: "ico-js", mjs: "ico-js", cjs: "ico-js",
  css: "ico-css",
  scss: "ico-scss", sass: "ico-scss", less: "ico-scss",
  html: "ico-html", htm: "ico-html",
  json: "ico-json", json5: "ico-json", jsonc: "ico-json", jsonl: "ico-json", ndjson: "ico-json",
  yaml: "ico-yaml", yml: "ico-yaml",
  rs: "ico-rs",
  toml: "ico-toml",
  sh: "ico-sh", bash: "ico-sh", zsh: "ico-sh", ps1: "ico-sh",
  md: "ico-md", markdown: "ico-md",
  txt: "ico-txt",
  pdf: "ico-pdf",
  doc: "ico-docx", docx: "ico-docx", rtf: "ico-docx",
  xls: "ico-xlsx", xlsx: "ico-xlsx", xlsm: "ico-xlsx", csv: "ico-xlsx",
  ppt: "ico-ppt", pptx: "ico-ppt",
  png: "ico-image", jpg: "ico-image", jpeg: "ico-image", gif: "ico-image",
  webp: "ico-image", svg: "ico-image", bmp: "ico-image", ico: "ico-image",
  py: "ico-py", pyw: "ico-py",
  pkl: "ico-py", pickle: "ico-py",
  go: "ico-go",
  exe: "ico-exe", dll: "ico-exe", msi: "ico-exe",
  zip: "ico-zip", rar: "ico-zip", "7z": "ico-zip", tar: "ico-zip", gz: "ico-zip", bz2: "ico-zip",
  c: "ico-cpp", h: "ico-cpp", cpp: "ico-cpp", cc: "ico-cpp", cxx: "ico-cpp", hpp: "ico-cpp", hh: "ico-cpp", hxx: "ico-cpp",
  cs: "ico-csharp",
  java: "ico-java",
  dart: "ico-dart",
  mp4: "ico-video", avi: "ico-video", mov: "ico-video", mkv: "ico-video", wmv: "ico-video", webm: "ico-video", flv: "ico-video", m4v: "ico-video",
  mp3: "ico-audio", wav: "ico-audio", flac: "ico-audio", aac: "ico-audio", ogg: "ico-audio", m4a: "ico-audio", wma: "ico-audio",
};

// 拡張子 → アイコンの種類とCSSクラス
function fileIconFor(ext: string): { icon: ReactElement; className: string } {
  const lower = ext.toLowerCase();
  const colorClass = EXT_COLOR_CLASS[lower];
  const className = colorClass ? `col-icon ${colorClass}` : "col-icon";
  if (CODE_SHAPE_EXT.has(lower)) return { icon: <IconFileCode />, className };
  if (TEXT_SHAPE_EXT.has(lower)) return { icon: <IconFileText />, className };
  return { icon: <IconFileGeneric />, className };
}

interface Props {
  entry: AstarEntry;
  isActive: boolean;
  colIndex: number;
  onSelect: (colIndex: number, entry: AstarEntry) => void;
  /** 検索キーワード未入力時はスコア0=「最低スコア」ではなく「スコアなし」として無着色にする */
  hasScore: boolean;
}

// 広域クエリ（"AI"等）ではカラムに数百〜数千エントリが載る。各アイテムをFramer Motion
// (motion.button) にすると、マウント時の入場アニメ＋毎レンダーの再調整がメインスレッドを
// 数秒〜十数秒占有し、検索完了後の入力反映まで遅延させる。素のbutton＋memoで、
// 不要な再レンダー（親=ColumnPanelがmemoでスキップされる限り）を止める。
export const HeatmapItem = memo(function HeatmapItem({ entry, isActive, colIndex, onSelect, hasScore }: Props) {
  const heatStyle = hasScore ? heatmapStyle(entry.score, entry.kind) : undefined;
  const isSkipped = entry.kind === "skipped";
  const tier = scoreTier(entry.score);
  const heatLabel = hasScore && entry.kind === "found" ? `, ${TIER_LABELS[tier]}` : "";

  return (
    <button
      type="button"
      role="option"
      aria-selected={isActive}
      aria-label={`${entry.name}${heatLabel}`}
      className={`col-item${isActive ? " active" : ""}${isSkipped ? " skip" : ""}`}
      onClick={() => onSelect(colIndex, entry)}
      // ヒートマップ背景はオーバーレイ（GPU合成、--heat-* はCSS変数経由でopacityのみ制御）
      style={{ position: "relative", width: "100%", textAlign: "left", ...heatStyle }}
    >
      {/* ヒートマップ背景オーバーレイ（色・最終濃度はCSS変数 --heat-bg / --heat-opacity 経由）。
          motion.spanでopacityを上書きするとCSSのcalc()が無効化され
          スコア0でも全面着色されてしまうため、素のspanでCSS側のopacity計算に委ねる
          （入場フェードはmotion.buttonの opacity 0→1 が乗算されてカバーする） */}
      {hasScore && <span className="heat-overlay" aria-hidden />}

      {/* 左端ヒートバー：面のopacityでは潰れる低スコア帯を輝度差で判別させる */}
      {hasScore && <span className="item-heatbar" aria-hidden />}

      {/* アイコン: フォルダ = --folder、コードファイル = --c-tsx に統一着色（mock.html準拠） */}
      {entry.is_dir ? (
        <span className="col-icon ico-folder"><IconFolder /></span>
      ) : (
        (() => {
          const { icon, className } = fileIconFor(entry.ext);
          return <span className={className}>{icon}</span>;
        })()
      )}

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
    </button>
  );
});

