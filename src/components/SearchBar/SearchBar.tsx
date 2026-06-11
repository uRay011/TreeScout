import { useRef, useEffect, useState } from "react";
import { Search } from "lucide-react";

interface Props {
  value: string;
  onChange: (v: string) => void;
  onSubmit: () => void;
  isLoading: boolean;
}

export function SearchBar({ value, onChange, onSubmit, isLoading }: Props) {
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
