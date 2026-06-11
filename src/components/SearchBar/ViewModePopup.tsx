import { useEffect, useRef, ReactElement } from "react";

export type ViewMode = "compact" | "standard" | "detail";

const MODES: { mode: ViewMode; name: string; desc: string; preview: ReactElement }[] = [
  {
    mode: "compact",
    name: "コンパクト",
    desc: "1行 · 28px",
    preview: (
      <svg className="view-popup-preview" viewBox="0 0 40 28" aria-hidden="true">
        <rect x="4" y="5"  width="32" height="6" rx="1" fill="currentColor" opacity="0.25"/>
        <rect x="4" y="13" width="32" height="6" rx="1" fill="currentColor" opacity="0.25"/>
        <rect x="4" y="21" width="24" height="6" rx="1" fill="currentColor" opacity="0.25"/>
      </svg>
    ),
  },
  {
    mode: "standard",
    name: "標準",
    desc: "2行 · 40px（デフォルト）",
    preview: (
      <svg className="view-popup-preview" viewBox="0 0 40 36" aria-hidden="true">
        <rect x="4" y="3"  width="28" height="5" rx="1" fill="currentColor" opacity="0.7"/>
        <rect x="4" y="10" width="20" height="3" rx="1" fill="currentColor" opacity="0.3"/>
        <rect x="4" y="17" width="28" height="5" rx="1" fill="currentColor" opacity="0.7"/>
        <rect x="4" y="24" width="20" height="3" rx="1" fill="currentColor" opacity="0.3"/>
      </svg>
    ),
  },
  {
    mode: "detail",
    name: "詳細",
    desc: "列常時表示 · 44px",
    preview: (
      <svg className="view-popup-preview" viewBox="0 0 40 40" aria-hidden="true">
        <rect x="4" y="3"  width="19" height="4" rx="1" fill="currentColor" opacity="0.7"/>
        <rect x="4" y="9"  width="13" height="3" rx="1" fill="currentColor" opacity="0.3"/>
        <rect x="29" y="4" width="7"  height="3" rx="1" fill="currentColor" opacity="0.45"/>
        <rect x="4" y="17" width="19" height="4" rx="1" fill="currentColor" opacity="0.7"/>
        <rect x="4" y="23" width="13" height="3" rx="1" fill="currentColor" opacity="0.3"/>
        <rect x="29" y="18" width="7" height="3" rx="1" fill="currentColor" opacity="0.45"/>
      </svg>
    ),
  },
];

interface Props {
  anchorEl: HTMLElement | null;
  value: ViewMode;
  onChange: (mode: ViewMode) => void;
  onClose: () => void;
}

export function ViewModePopup({ anchorEl, value, onChange, onClose }: Props) {
  const popupRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const handlePointerDown = (e: MouseEvent) => {
      const target = e.target as Node;
      if (popupRef.current?.contains(target)) return;
      if (anchorEl?.contains(target)) return;
      onClose();
    };
    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key === "Escape") onClose();
    };
    document.addEventListener("mousedown", handlePointerDown);
    document.addEventListener("keydown", handleKeyDown);
    return () => {
      document.removeEventListener("mousedown", handlePointerDown);
      document.removeEventListener("keydown", handleKeyDown);
    };
  }, [anchorEl, onClose]);

  useEffect(() => {
    (popupRef.current?.querySelector(`[data-mode="${value}"]`) as HTMLElement | null)?.focus();
  }, [value]);

  if (!anchorEl) return null;

  const rect = anchorEl.getBoundingClientRect();
  const POPUP_WIDTH = 210;
  const left = Math.min(rect.left, window.innerWidth - POPUP_WIDTH - 4);

  const handleKeyDownNav = (e: React.KeyboardEvent) => {
    const items = [...(popupRef.current?.querySelectorAll<HTMLElement>(".view-popup-item") ?? [])];
    const cur = items.findIndex((el) => el === document.activeElement);
    if (e.key === "ArrowDown") {
      e.preventDefault();
      items[(cur + 1) % items.length]?.focus();
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      items[(cur - 1 + items.length) % items.length]?.focus();
    }
  };

  return (
    <div
      className="view-popup open"
      id="viewPopup"
      role="menu"
      aria-label="左ペインの表示形式"
      ref={popupRef}
      style={{ top: rect.bottom, left }}
      onKeyDown={handleKeyDownNav}
    >
      <div className="view-popup-label">左ペインの表示形式</div>
      {MODES.map((m) => (
        <button
          key={m.mode}
          className="view-popup-item"
          role="menuitemradio"
          data-mode={m.mode}
          aria-checked={value === m.mode}
          type="button"
          onClick={() => { onChange(m.mode); onClose(); }}
        >
          <span className="view-popup-radio" />
          {m.preview}
          <span className="view-popup-name">{m.name}</span>
          <span className="view-popup-desc">{m.desc}</span>
        </button>
      ))}
    </div>
  );
}
