import { useRef, useEffect, useState } from "react";
import { Search } from "lucide-react";

interface Props {
  value: string;
  onChange: (v: string) => void;
  onSubmit: () => void;
  isLoading: boolean;
  /** 検索モード（AIセマンティック / 素のEverythingフィルタ） */
  mode: "ai" | "filter";
  onModeChange: (mode: "ai" | "filter") => void;
}

export function SearchBar({ value, onChange, onSubmit, isLoading, mode, onModeChange }: Props) {
  const inputRef = useRef<HTMLInputElement>(null);
  const [focused, setFocused] = useState(false);

  // Ctrl+F でフォーカス
  useEffect(() => {
    const handler = (e: KeyboardEvent) => {
      if ((e.ctrlKey || e.metaKey) && e.key === "f") {
        e.preventDefault();
        inputRef.current?.focus();
        inputRef.current?.select();
      }
    };
    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, []);

  return (
    <div className="search-wrap" data-loading={isLoading}>
      <div className="search-mode" role="group" aria-label="検索モード">
        <button
          type="button"
          className={`search-mode-btn${mode === "ai" ? " active" : ""}`}
          aria-pressed={mode === "ai"}
          title="AIセマンティック検索：意味的に近い順で上位を抽出"
          onClick={() => onModeChange("ai")}
        >
          AI
        </button>
        <button
          type="button"
          className={`search-mode-btn${mode === "filter" ? " active" : ""}`}
          aria-pressed={mode === "filter"}
          title="フィルタ：Everythingの名前一致を無制限に表示"
          onClick={() => onModeChange("filter")}
        >
          フィルタ
        </button>
      </div>
      <Search
        className="search-icon"
        width={13}
        height={13}
        strokeWidth={1.8}
        aria-hidden
      />
      <input
        ref={inputRef}
        className="search-input"
        type="search"
        value={value}
        placeholder="ファイルを検索…"
        aria-label="ファイル検索"
        autoComplete="off"
        spellCheck={false}
        onChange={(e) => onChange(e.currentTarget.value)}
        onKeyDown={(e) => {
          if (e.key === "Enter") {
            e.preventDefault();
            onSubmit();
          }
        }}
        onFocus={() => setFocused(true)}
        onBlur={() => setFocused(false)}
      />
      <span className="search-kbd" aria-hidden>
        {focused ? <kbd>Enter</kbd> : <><kbd>Ctrl</kbd><kbd>F</kbd></>}
      </span>
      <span className="search-loadbar" aria-hidden />
    </div>
  );
}
