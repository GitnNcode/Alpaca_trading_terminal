import React, { useEffect, useMemo, useState } from "react";
import { api, bridgeAvailable } from "./lib/api";
import { ChartTab } from "./components/ChartTab";
import { CompareTab } from "./components/CompareTab";
import { FunctionBar, type FuncCode } from "./components/FunctionBar";
import { StatusBar } from "./components/StatusBar";
import { CredentialsModal } from "./components/CredentialsModal";

type TabKey = "GP" | "COMP";

const FUNC_CODES: FuncCode[] = [
  { code: "GP", label: "GRAPH", description: "Multi-pane price chart" },
  { code: "COMP", label: "COMPARE", description: "Risk / return comparator" },
];

export function App() {
  const [tab, setTab] = useState<TabKey>("GP");
  const [hasCreds, setHasCreds] = useState<boolean | null>(null);
  const [pyOnline, setPyOnline] = useState(false);
  const [pyErr, setPyErr] = useState<string | null>(null);
  const [showCreds, setShowCreds] = useState(false);
  const [symbol, setSymbol] = useState("");
  const [cmd, setCmd] = useState("");
  const [now, setNow] = useState(() => new Date());

  useEffect(() => {
    const id = setInterval(() => setNow(new Date()), 1000);
    return () => clearInterval(id);
  }, []);

  useEffect(() => {
    let cancelled = false;
    const probe = async () => {
      try {
        const h = await api.health();
        if (cancelled) return;
        setPyOnline(true);
        setPyErr(null);
        setHasCreds(h.has_credentials);
        if (!h.has_credentials) setShowCreds(true);
      } catch (e) {
        if (cancelled) return;
        setPyOnline(false);
        setPyErr(String(e instanceof Error ? e.message : e));
      }
    };
    void probe();
    const id = setInterval(probe, 4000);
    return () => { cancelled = true; clearInterval(id); };
  }, []);

  const handleCmd = (raw: string) => {
    const t = raw.trim().toUpperCase();
    if (!t) return;
    if (t === "GP" || t === "GP <GO>") { setTab("GP"); setCmd(""); return; }
    if (t === "COMP" || t === "COMP <GO>") { setTab("COMP"); setCmd(""); return; }
    if (t === "AUTH" || t === "LOGIN") { setShowCreds(true); setCmd(""); return; }
    if (t === "HELP" || t === "?") { setCmd(""); return; }
    if (/^[A-Z.]{1,8}$/.test(t)) {
      setSymbol(t);
      setTab("GP");
      setCmd("");
      return;
    }
    setCmd("");
  };

  const timeStr = useMemo(() => {
    const z = (n: number) => n.toString().padStart(2, "0");
    return `${z(now.getUTCHours())}:${z(now.getUTCMinutes())}:${z(now.getUTCSeconds())}Z`;
  }, [now]);

  return (
    <div className="ccb-app">
      <div className="ccb-titlebar">
        <span><span className="brand-dot" /><span className="brand">ALPACA TERMINAL</span></span>
        <span style={{ color: "var(--ccb-fg-muted)" }}>BLOOMBERG-STYLE BUILD · CHART/COMPARE</span>
        <span className="meta">{timeStr}</span>
      </div>

      <FunctionBar
        active={tab}
        codes={FUNC_CODES}
        onSelect={(c) => setTab(c as TabKey)}
        cmd={cmd}
        setCmd={setCmd}
        onSubmit={handleCmd}
      />

      <div className="ccb-main">
        {!pyOnline && (
          <div className="ccb-banner ccb-banner-err">
            <strong>SIDECAR OFFLINE</strong>
            <span>{pyErr ?? "no connection"}</span>
            <span className="hint">
              {bridgeAvailable()
                ? "Electron failed to start python — check the terminal for python3 / pip errors."
                : "Browser mode: run `npm run dev` in another terminal so the Python server starts on :8765, or launch via `npm start` (Electron)."}
            </span>
          </div>
        )}
        {tab === "GP" && <ChartTab symbol={symbol} setSymbol={setSymbol} ready={!!hasCreds && pyOnline} />}
        {tab === "COMP" && <CompareTab ready={!!hasCreds && pyOnline} />}
      </div>

      <StatusBar pyOnline={pyOnline} hasCreds={hasCreds} onAuth={() => setShowCreds(true)} time={timeStr} />

      {showCreds && (
        <CredentialsModal
          onClose={() => setShowCreds(false)}
          onSaved={() => { setHasCreds(true); setShowCreds(false); }}
        />
      )}
    </div>
  );
}
