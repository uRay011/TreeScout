import { useEffect, useRef, useState } from "react";
import { FileText, FileCode, Sparkles, TriangleAlert } from "lucide-react";
import { marked } from "marked";
import DOMPurify from "dompurify";
import { convertFileSrc } from "@tauri-apps/api/core";
import { getPreview, PreviewResult } from "../../lib/tauri";

const EXT_CLASS: Record<string, string> = {
  tsx: "ext-tsx",
  ts: "ext-ts",
  css: "ext-css",
  md: "ext-md",
  json: "ext-json",
  rs: "ext-rs",
  toml: "ext-toml",
};

function extClass(ext: string): string {
  return EXT_CLASS[ext.toLowerCase()] ?? "ext-other";
}

const CODE_EXT = new Set([
  "tsx", "ts", "jsx", "js", "css", "scss", "rs", "toml", "json", "jsonc",
  "py", "go", "java", "c", "h", "cpp", "hpp", "cs", "rb", "php", "sh",
  "bat", "ps1", "html", "xml", "sql", "yaml", "yml",
]);

function fileIconFor(ext: string) {
  return CODE_EXT.has(ext.toLowerCase())
    ? <FileCode aria-hidden width={13} height={13} strokeWidth={1.5} />
    : <FileText aria-hidden width={13} height={13} strokeWidth={1.5} />;
}

function formatSize(bytes?: number): string | null {
  if (!bytes) return null;
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / 1024 / 1024).toFixed(1)} MB`;
}

// ヒートマップ色: hsl(220,80%, 20%+score*60%)（frontend.md §2 準拠。mock_v2.html J2 と同一式）
function heatBg(score: number): string {
  return `hsl(220, 80%, ${(20 + score * 60).toFixed(1)}%)`;
}
function heatText(score: number): string {
  return score >= 0.6 ? "#0d1117" : "#e6edf3";
}

const PREVIEW_DEBOUNCE_MS = 150;

export interface PreviewSelection {
  path: string;
  name: string;
  ext: string;
  score: number;
  size?: number;
  modified?: string;
}

interface Props {
  selection: PreviewSelection | null;
}

export function PreviewPane({ selection }: Props) {
  const [preview, setPreview] = useState<PreviewResult | null>(null);
  const [loading, setLoading] = useState(false);
  const [entering, setEntering] = useState(false);
  const requestIdRef = useRef(0);

  // 選択デバウンス（backend.md §6: 100〜150ms）。連続選択は前回リクエストを無効化
  useEffect(() => {
    if (!selection) {
      requestIdRef.current++;
      setPreview(null);
      setLoading(false);
      return;
    }
    const requestId = ++requestIdRef.current;
    setLoading(true);
    const timer = setTimeout(() => {
      getPreview(selection.path)
        .then((result) => {
          if (requestIdRef.current !== requestId) return;
          setPreview(result);
          setLoading(false);
          setEntering(true);
        })
        .catch(() => {
          if (requestIdRef.current !== requestId) return;
          setPreview(null);
          setLoading(false);
        });
    }, PREVIEW_DEBOUNCE_MS);
    return () => clearTimeout(timer);
  }, [selection]);

  // プレビュー切り替え時のフェードイン: マウント直後に .pre を外してトランジション発火
  useEffect(() => {
    if (!entering) return;
    const id = requestAnimationFrame(() => setEntering(false));
    return () => cancelAnimationFrame(id);
  }, [entering]);

  if (!selection) {
    return (
      <div className="empty-state">
        <div className="empty-icon"><FileText aria-hidden width={24} height={24} strokeWidth={1.5} /></div>
        <div className="empty-title">ファイルを選択するとプレビューを表示します</div>
        <div className="empty-hint"><kbd>↑</kbd><kbd>↓</kbd> で選択を移動</div>
      </div>
    );
  }

  if (loading || !preview) {
    return (
      <div className="pv-skel">
        {[90, 75, 85, 60, 80, 70, 40].map((w, i) => (
          <span key={i} className="sk-block" style={{ width: `${w}%` }} />
        ))}
      </div>
    );
  }

  const sizeText = formatSize(selection.size);
  const truncated = (preview.kind === "text" || preview.kind === "markdown") && preview.truncated;
  const scorePillStyle = {
    "--heat-bg": heatBg(selection.score),
    "--heat-text": heatText(selection.score),
  } as React.CSSProperties;

  return (
    <div className={`pv-view${entering ? " pre" : ""}`}>
      <div className="preview-header">
        <span className={`ext-badge ${extClass(selection.ext)}`}>{(selection.ext || "?").toUpperCase().slice(0, 4)}</span>
        <div className="preview-title">
          <div className="preview-filename">{selection.name}</div>
          <div className="preview-path" title={selection.path}>{selection.path}</div>
        </div>
      </div>

      <div className="preview-meta">
        <div className="meta-chip">
          <span className="meta-k">スコア</span>
          <span className="score-pill" style={scorePillStyle}>{selection.score.toFixed(2)}</span>
        </div>
        {sizeText && (
          <div className="meta-chip"><span className="meta-k">サイズ</span><span className="meta-v">{sizeText}</span></div>
        )}
        {selection.modified && (
          <div className="meta-chip"><span className="meta-k">更新</span><span className="meta-v">{selection.modified}</span></div>
        )}
        <div className="meta-chip"><span className="meta-k">形式</span><span className="meta-v">{selection.ext || "—"}</span></div>
      </div>

      {/* AIガイド表示（score >= 0.8 のファイルのみ） */}
      {selection.score >= 0.8 && (
        <div className="preview-guide">
          <Sparkles aria-hidden width={12} height={12} strokeWidth={1.5} />
          <span>AIガイド: 高一致ルート</span>
          <span className="guide-line" />
        </div>
      )}

      <div className="preview-body">
        <PreviewBody preview={preview} ext={selection.ext} path={selection.path} />
      </div>

      {/* truncated バナー（backend.md §6: 先頭64KBのみ） */}
      {truncated && (
        <div className="preview-trunc">
          <TriangleAlert aria-hidden width={12} height={12} strokeWidth={1.5} />
          <span>先頭 64KB のみ表示しています{sizeText ? `（全体 ${sizeText}）` : ""}</span>
        </div>
      )}
    </div>
  );
}

function PreviewBody({ preview, ext, path }: { preview: PreviewResult; ext: string; path: string }) {
  switch (preview.kind) {
    case "text":
      return (
        <div className="pv-code">
          {preview.content.split("\n").map((line, i) => (
            <div className="pv-line" key={i}>
              <span className="pv-ln">{i + 1}</span>
              <span className="pv-tx">{line}</span>
            </div>
          ))}
        </div>
      );

    case "markdown": {
      const html = DOMPurify.sanitize(marked.parse(preview.content, { async: false, breaks: true }));
      return <div className="pv-md" dangerouslySetInnerHTML={{ __html: html }} />;
    }

    case "image":
      return (
        <div className="pv-image">
          <img src={convertFileSrc(path)} alt="" />
        </div>
      );

    case "unsupported":
    default:
      return (
        <div className="empty-state">
          <div className="empty-icon">{fileIconFor(ext)}</div>
          <div className="empty-title">プレビュー非対応の形式です</div>
          <div className="empty-hint"><span className="pv-format">{ext ? `.${ext}` : "unknown"}</span></div>
        </div>
      );
  }
}
