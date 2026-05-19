import React, { useEffect, useRef } from "react";

export interface FuncCode {
  code: string;
  label: string;
  description: string;
}

interface Props {
  active: string;
  codes: FuncCode[];
  onSelect: (code: string) => void;
  cmd: string;
  setCmd: (v: string) => void;
  onSubmit: (cmd: string) => void;
}

export function FunctionBar({ active, codes, onSelect, cmd, setCmd, onSubmit }: Props) {
  const inputRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "/" && document.activeElement?.tagName !== "INPUT") {
        e.preventDefault();
        inputRef.current?.focus();
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  return (
    <div className="ccb-funcbar">
      <div className="codes">
        {codes.map((c) => (
          <div
            key={c.code}
            className={"code" + (c.code === active ? " active" : "")}
            onClick={() => onSelect(c.code)}
            title={c.description}
          >
            {c.code} · {c.label}
          </div>
        ))}
      </div>
      <div className="ccb-divider" />
      <div className="cmdline">
        <span className="prompt">›</span>
        <input
          ref={inputRef}
          value={cmd}
          onChange={(e) => setCmd(e.target.value)}
          onKeyDown={(e) => { if (e.key === "Enter") onSubmit(cmd); }}
          placeholder="TYPE SYMBOL OR FUNCTION  <GO>"
          spellCheck={false}
          autoComplete="off"
        />
        <span className="hint">[ / TO FOCUS · GP · COMP · AUTH ]</span>
      </div>
    </div>
  );
}
