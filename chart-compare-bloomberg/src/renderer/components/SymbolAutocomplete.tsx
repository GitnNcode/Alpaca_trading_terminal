import React, { useEffect, useRef, useState } from "react";
import { api } from "../lib/api";
import type { Asset } from "@shared/types";

let cache: Asset[] | null = null;
async function loadAssets(): Promise<Asset[]> {
  if (cache) return cache;
  try { cache = await api.assets(); } catch { cache = []; }
  return cache;
}

interface Props {
  value: string;
  onChange: (v: string) => void;
  onSubmit: (sym: string) => void;
  className?: string;
  placeholder?: string;
}

export function SymbolAutocomplete({ value, onChange, onSubmit, className, placeholder }: Props) {
  const [open, setOpen] = useState(false);
  const [matches, setMatches] = useState<Asset[]>([]);
  const [active, setActive] = useState(0);
  const wrapRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    void loadAssets();
  }, []);

  useEffect(() => {
    const q = value.trim().toUpperCase();
    if (!q) { setMatches([]); return; }
    void loadAssets().then((all) => {
      const prefix = all.filter((a) => a.symbol.startsWith(q)).slice(0, 8);
      if (prefix.length >= 6) { setMatches(prefix.slice(0, 6)); return; }
      const sub = all.filter((a) => !a.symbol.startsWith(q) && a.name.toUpperCase().includes(q)).slice(0, 6 - prefix.length);
      setMatches([...prefix, ...sub]);
    });
  }, [value]);

  useEffect(() => {
    const onDoc = (e: MouseEvent) => {
      if (!wrapRef.current?.contains(e.target as Node)) setOpen(false);
    };
    document.addEventListener("mousedown", onDoc);
    return () => document.removeEventListener("mousedown", onDoc);
  }, []);

  const submit = (sym: string) => {
    onChange(sym);
    setOpen(false);
    onSubmit(sym);
  };

  return (
    <div className="ccb-autocomplete" ref={wrapRef}>
      <input
        className={className ?? "symbol"}
        value={value}
        onChange={(e) => { onChange(e.target.value.toUpperCase()); setOpen(true); setActive(0); }}
        onFocus={() => setOpen(true)}
        onKeyDown={(e) => {
          if (e.key === "Enter") { e.preventDefault(); submit(matches[active]?.symbol ?? value); return; }
          if (e.key === "ArrowDown") { e.preventDefault(); setActive((i) => Math.min(matches.length - 1, i + 1)); return; }
          if (e.key === "ArrowUp") { e.preventDefault(); setActive((i) => Math.max(0, i - 1)); return; }
          if (e.key === "Escape") setOpen(false);
        }}
        placeholder={placeholder ?? "SYMBOL"}
        spellCheck={false}
        autoComplete="off"
      />
      {open && matches.length > 0 && (
        <div className="menu">
          {matches.map((m, i) => (
            <div
              key={m.symbol}
              className={"item" + (i === active ? " active" : "")}
              onMouseDown={(e) => { e.preventDefault(); submit(m.symbol); }}
              onMouseEnter={() => setActive(i)}
            >
              <span className="sym">{m.symbol}</span>
              <span className="name">{m.name}</span>
            </div>
          ))}
        </div>
      )}
    </div>
  );
}
