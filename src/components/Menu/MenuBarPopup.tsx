import { useEffect, useRef } from "react";

export type MenuItemDef =
  | { type: "action"; label: string; shortcut?: string; onSelect: () => void; disabled?: boolean }
  | { type: "checkbox"; label: string; shortcut?: string; checked: boolean; onToggle: () => void }
  | { type: "separator" };

interface Props {
  anchorEl: HTMLElement | null;
  items: MenuItemDef[];
  onClose: () => void;
  ariaLabel: string;
}

/** タイトルバーのメニュー項目（ファイル/編集/検索など）共通のドロップダウン */
export function MenuBarPopup({ anchorEl, items, onClose, ariaLabel }: Props) {
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

  if (!anchorEl) return null;

  const rect = anchorEl.getBoundingClientRect();
  const POPUP_MIN_WIDTH = 220;
  const left = Math.min(rect.left, window.innerWidth - POPUP_MIN_WIDTH - 4);

  return (
    <div
      className="menubar-popup open"
      role="menu"
      aria-label={ariaLabel}
      ref={popupRef}
      style={{ top: rect.bottom, left }}
    >
      {items.map((item, i) => {
        if (item.type === "separator") {
          return <div className="menubar-popup-sep" key={`sep-${i}`} role="separator" />;
        }
        if (item.type === "checkbox") {
          return (
            <button
              key={item.label}
              type="button"
              className="menubar-popup-item"
              role="menuitemcheckbox"
              aria-checked={item.checked}
              onClick={() => { item.onToggle(); onClose(); }}
            >
              <span className="menubar-popup-check" aria-hidden />
              <span className="menubar-popup-label">{item.label}</span>
              {item.shortcut && <span className="menubar-popup-shortcut">{item.shortcut}</span>}
            </button>
          );
        }
        return (
          <button
            key={item.label}
            type="button"
            className="menubar-popup-item"
            role="menuitem"
            disabled={item.disabled}
            onClick={() => { item.onSelect(); onClose(); }}
          >
            <span className="menubar-popup-label">{item.label}</span>
            {item.shortcut && <span className="menubar-popup-shortcut">{item.shortcut}</span>}
          </button>
        );
      })}
    </div>
  );
}
