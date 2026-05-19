import React, { useEffect, useMemo, useRef, useState } from "react";
import { api } from "../lib/api";
import type {
  CompareRange,
  CompareResponse,
  CompareSeriesOk,
  MCResponse,
} from "@shared/types";
import { fmtPct, fmtMoney } from "../lib/format";
import { SymbolAutocomplete } from "./SymbolAutocomplete";
import { NormalizedAndDrawdown } from "./compare/NormalizedAndDrawdown";
import { Heatmap } from "./compare/Heatmap";
import { Scatter } from "./compare/Scatter";
import { MonteCarlo } from "./compare/MonteCarlo";

const SLOT_COLORS = ["#00bfff", "#ff8c00", "#ffd700", "#00ff5a"];
const RANGES: CompareRange[] = ["1Y", "3Y", "5Y", "10Y"];

interface Props { ready: boolean; }

export function CompareTab({ ready }: Props) {
  const [slots, setSlots] = useState<string[]>(["AAPL", "MSFT", "NVDA", "GOOGL"]);
  const [range, setRange] = useState<CompareRange>("3Y");
  const [resp, setResp] = useState<CompareResponse | null>(null);
  const [err, setErr] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);

  const okSeries = useMemo<CompareSeriesOk[]>(() =>
    (resp?.series ?? []).filter((s): s is CompareSeriesOk => "bars" in s)
  , [resp]);

  // Accept an optional `override` so callers that just updated slot state
  // (e.g. autocomplete onSubmit) can pass the new array without waiting for
  // React to re-render — otherwise `slots` here reads the stale closure.
  const fetchAll = (override?: string[]) => {
    if (!ready) return;
    const source = override ?? slots;
    const filled = source.map(s => s.trim().toUpperCase()).filter(Boolean);
    if (filled.length === 0) { setResp(null); return; }
    setLoading(true); setErr(null);
    api.compare(filled, range)
      .then(setResp)
      .catch((e) => setErr(String(e?.message ?? e)))
      .finally(() => setLoading(false));
  };

  useEffect(() => { fetchAll(); /* eslint-disable-next-line react-hooks/exhaustive-deps */ }, [range, ready]);

  return (
    <div className="ccb-compare">
      <div className="slot-bar">
        {[0, 1, 2, 3].map((i) => (
          <div
            key={i}
            className="slot"
            style={{ "--slot-color": SLOT_COLORS[i] } as React.CSSProperties}
          >
            <div className="row">
              <span className="swatch" />
              <span className="idx">SLOT {i + 1}</span>
              <span className="sym">{slots[i] || "—"}</span>
            </div>
            <SymbolAutocomplete
              className=""
              value={slots[i] || ""}
              onChange={(v) => setSlots((p) => { const c = [...p]; c[i] = v.toUpperCase(); return c; })}
              onSubmit={(sym) => {
                const updated = [...slots];
                updated[i] = sym.toUpperCase();
                setSlots(updated);
                fetchAll(updated);
              }}
            />
          </div>
        ))}
      </div>

      <div className="controls">
        <span className="label">RANGE</span>
        {RANGES.map((r) => (
          <button key={r} className={r === range ? "active" : ""} onClick={() => setRange(r)}>{r}</button>
        ))}
        <span className="ccb-divider" />
        <button onClick={() => fetchAll()}>RELOAD &lt;GO&gt;</button>
        <span className="ccb-spacer" />
        <span style={{ color: "var(--ccb-fg-muted)", fontSize: 10, letterSpacing: "0.12em" }}>
          {loading ? "LOADING…" : err ? <span style={{ color: "var(--ccb-red)" }}>ERR {err}</span> : `${okSeries.length}/${slots.filter(Boolean).length} LOADED`}
        </span>
      </div>

      <div className="grid">
        {/* Metrics table */}
        <div className="card" style={{ gridColumn: "1 / span 2" }}>
          <div className="ccb-section-title">RISK / RETURN METRICS<span className="sub">CAGR · VOL · SHARPE · SORTINO · MAX DD · CALMAR · 252 TD ANNUALIZATION</span></div>
          <div className="body" style={{ overflow: "auto" }}>
            <MetricsTable series={okSeries} />
          </div>
        </div>

        <div className="card">
          <div className="ccb-section-title">NORMALIZED RETURN (100=START)<span className="sub">+ DRAWDOWN UNDERLAY</span></div>
          <div className="body"><NormalizedAndDrawdown series={okSeries} colors={SLOT_COLORS} /></div>
        </div>

        <div className="card">
          <div className="ccb-section-title">RISK / RETURN SCATTER<span className="sub">X = ANN VOL · Y = CAGR</span></div>
          <div className="body"><Scatter series={okSeries} colors={SLOT_COLORS} /></div>
        </div>

        <div className="card">
          <div className="ccb-section-title">CORRELATION MATRIX<span className="sub">DAILY LOG RETURNS · PEARSON</span></div>
          <div className="body"><Heatmap labels={resp?.labels ?? []} matrix={resp?.matrix ?? []} /></div>
        </div>

        <div className="card">
          <div className="ccb-section-title">MONTE CARLO PROJECTION<span className="sub">GBM · XORSHIFT64 + BOX-MULLER</span></div>
          <div className="body"><MonteCarlo okSeries={okSeries} range={range} colors={SLOT_COLORS} /></div>
        </div>
      </div>
    </div>
  );
}

// =====================================================================

function MetricsTable({ series }: { series: CompareSeriesOk[] }) {
  if (series.length === 0) {
    return <div style={{ padding: 14, color: "var(--ccb-fg-muted)", fontSize: 11 }}>NO DATA · LOAD A SLOT TO BEGIN</div>;
  }
  const cols: { k: keyof CompareSeriesOk["metrics"]; label: string; fmt: (v: number) => string; better: "high" | "low" }[] = [
    { k: "cagr",    label: "CAGR",    fmt: (v) => fmtPct(v), better: "high" },
    { k: "ann_vol", label: "ANN VOL", fmt: (v) => fmtPct(v), better: "low"  },
    { k: "sharpe",  label: "SHARPE",  fmt: (v) => v.toFixed(2), better: "high" },
    { k: "sortino", label: "SORTINO", fmt: (v) => v.toFixed(2), better: "high" },
    { k: "max_dd",  label: "MAX DD",  fmt: (v) => fmtPct(v), better: "high" },
    { k: "calmar",  label: "CALMAR",  fmt: (v) => v.toFixed(2), better: "high" },
  ];

  const bestWorst: Record<string, { best: number; worst: number }> = {};
  for (const c of cols) {
    const vals = series.map((s) => s.metrics[c.k]).filter((v) => isFinite(v));
    if (vals.length === 0) continue;
    bestWorst[c.k] = c.better === "high"
      ? { best: Math.max(...vals), worst: Math.min(...vals) }
      : { best: Math.min(...vals), worst: Math.max(...vals) };
  }

  return (
    <table className="ccb-metrics">
      <thead>
        <tr>
          <th>SYMBOL</th>
          {cols.map((c) => <th key={c.label}>{c.label}</th>)}
        </tr>
      </thead>
      <tbody>
        {series.map((s, i) => (
          <tr key={s.symbol} style={{ "--slot-color": SLOT_COLORS[i] } as React.CSSProperties}>
            <td className="sym">{s.symbol}</td>
            {cols.map((c) => {
              const v = s.metrics[c.k];
              const bw = bestWorst[c.k];
              const cls = !bw || !isFinite(v) ? "" : v === bw.best ? "best" : v === bw.worst ? "worst" : "";
              return <td key={c.label} className={cls}>{c.fmt(v)}</td>;
            })}
          </tr>
        ))}
      </tbody>
    </table>
  );
}
