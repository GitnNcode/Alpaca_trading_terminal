import React, { useEffect, useRef, useState } from "react";
import { api } from "../../lib/api";
import type { CompareRange, CompareSeriesOk, MCResponse } from "@shared/types";
import { fmtPct } from "../../lib/format";

interface Props {
  okSeries: CompareSeriesOk[];
  range: CompareRange;
  colors: string[];
}

export function MonteCarlo({ okSeries, range, colors }: Props) {
  const [target, setTarget] = useState<number>(0);
  const [horizon, setHorizon] = useState<number>(5);
  const [nSims, setNSims] = useState<number>(1000);
  const [seed, setSeed] = useState<number>(0xC0FFEE);
  const [result, setResult] = useState<MCResponse | null>(null);
  const [busy, setBusy] = useState(false);
  const [err, setErr] = useState<string | null>(null);

  useEffect(() => { setResult(null); }, [okSeries.map(s => s.symbol).join(",")]);

  const symbol = okSeries[target]?.symbol;
  const slotColor = colors[target] ?? "#ff8c00";

  const run = async () => {
    if (!symbol) return;
    setBusy(true); setErr(null);
    try {
      const r = await api.montecarlo(symbol, range, horizon, nSims, seed);
      setResult(r);
    } catch (e) {
      setErr(String(e instanceof Error ? e.message : e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <div style={{ position: "absolute", inset: 0, display: "flex", flexDirection: "column" }}>
      <div style={{ display: "flex", gap: 10, alignItems: "center", padding: "6px 10px", borderBottom: "1px solid #1c1c1c", background: "#030303", flexWrap: "wrap" }}>
        <span style={{ color: "#666", fontSize: 10, letterSpacing: "0.16em" }}>ASSET</span>
        {okSeries.map((s, i) => (
          <button key={s.symbol} className={i === target ? "active" : ""} onClick={() => setTarget(i)} style={{ color: i === target ? "#000" : colors[i], borderColor: colors[i] }}>{s.symbol}</button>
        ))}
        <span style={{ color: "#666", fontSize: 10, letterSpacing: "0.16em", marginLeft: 8 }}>HORIZON</span>
        {[1, 3, 5, 10].map(h => (
          <button key={h} className={h === horizon ? "active" : ""} onClick={() => setHorizon(h)}>{h}Y</button>
        ))}
        <span style={{ color: "#666", fontSize: 10, letterSpacing: "0.16em", marginLeft: 8 }}>SIMS</span>
        <input type="number" min={100} max={10000} step={100} value={nSims} onChange={(e) => setNSims(Math.max(100, Math.min(10000, +e.target.value || 1000)))} style={{ width: 70 }} />
        <button onClick={() => setSeed(seed + 1)}>SEED+</button>
        <button className="active" onClick={run} disabled={!symbol || busy}>RUN &lt;GO&gt;</button>
        <span style={{ color: "#666", fontSize: 10, letterSpacing: "0.12em" }}>{busy ? "RUNNING…" : err ? <span style={{ color: "var(--ccb-red)" }}>ERR {err}</span> : ""}</span>
      </div>
      <div style={{ flex: 1, position: "relative" }}>
        {result ? (
          <McFan result={result} color={slotColor} />
        ) : (
          <div style={{ position: "absolute", inset: 0, display: "flex", alignItems: "center", justifyContent: "center", color: "#666", fontSize: 11, letterSpacing: "0.12em" }}>
            {symbol ? "PRESS RUN <GO> TO PROJECT" : "LOAD A SLOT FIRST"}
          </div>
        )}
      </div>
      {result && (
        <div style={{ display: "flex", gap: 18, padding: "6px 12px", borderTop: "1px solid #1c1c1c", background: "#030303", fontSize: 11, fontVariantNumeric: "tabular-nums", flexWrap: "wrap" }}>
          <KV k="N SIMS" v={result.n_sims.toLocaleString()} />
          <KV k="DAYS" v={result.days.toLocaleString()} />
          <KV k="μ DAILY" v={fmtPct(result.mu_daily, 3)} />
          <KV k="σ DAILY" v={fmtPct(result.sigma_daily, 3)} />
          <KV k="P5" v={result.final_p05.toFixed(2) + "×"} />
          <KV k="P50" v={result.final_p50.toFixed(2) + "×"} clr={slotColor} />
          <KV k="P95" v={result.final_p95.toFixed(2) + "×"} />
          <KV k="P(>START)" v={fmtPct(result.prob_above_start)} clr="#00ff5a" />
          <KV k="P(>50% DD)" v={fmtPct(result.prob_50_dd)} clr="#ff3b3b" />
        </div>
      )}
    </div>
  );
}

function KV({ k, v, clr }: { k: string; v: string; clr?: string }) {
  return (
    <span style={{ display: "inline-flex", gap: 6, alignItems: "baseline" }}>
      <span style={{ color: "#666", letterSpacing: "0.16em", fontSize: 10 }}>{k}</span>
      <span style={{ color: clr ?? "#f0f0f0" }}>{v}</span>
    </span>
  );
}

function McFan({ result, color }: { result: MCResponse; color: string }) {
  const ref = useRef<HTMLCanvasElement>(null);
  useEffect(() => {
    const canvas = ref.current;
    if (!canvas) return;
    const dpr = window.devicePixelRatio || 1;
    const ro = new ResizeObserver(() => draw());
    ro.observe(canvas.parentElement!);
    draw();
    return () => ro.disconnect();

    function draw() {
      if (!canvas) return;
      const w = canvas.clientWidth, h = canvas.clientHeight;
      canvas.width = w * dpr; canvas.height = h * dpr;
      const ctx = canvas.getContext("2d")!;
      ctx.scale(dpr, dpr);
      ctx.fillStyle = "#000"; ctx.fillRect(0, 0, w, h);

      const ml = 56, mr = 14, mt = 12, mb = 28;
      const pw = w - ml - mr; const ph = h - mt - mb;
      const n = result.p50.length;
      const yMax = Math.max(...result.p95);
      const yMin = Math.min(...result.p05);
      const ypad = 0.05 * (yMax - yMin || 1);
      const yLo = Math.max(0, yMin - ypad);
      const yHi = yMax + ypad;

      const xAt = (i: number) => ml + (i / (n - 1)) * pw;
      const yAt = (v: number) => mt + ph - ((v - yLo) / (yHi - yLo)) * ph;

      // grid + axis labels
      ctx.strokeStyle = "#1c1c1c"; ctx.lineWidth = 1;
      ctx.fillStyle = "#666"; ctx.font = '10px "JetBrains Mono", monospace';
      ctx.textAlign = "right"; ctx.textBaseline = "middle";
      for (let k = 0; k <= 4; k++) {
        const v = yLo + ((yHi - yLo) * k) / 4;
        const y = yAt(v);
        ctx.beginPath(); ctx.moveTo(ml, y); ctx.lineTo(ml + pw, y); ctx.stroke();
        ctx.fillText(v.toFixed(2) + "×", ml - 4, y);
      }
      ctx.textAlign = "center"; ctx.textBaseline = "top";
      for (let k = 0; k <= 4; k++) {
        const x = ml + (pw * k) / 4;
        ctx.beginPath(); ctx.moveTo(x, mt); ctx.lineTo(x, mt + ph); ctx.stroke();
        const days = Math.round((result.days * k) / 4);
        const years = days / 252;
        ctx.fillText(years.toFixed(1) + "y", x, mt + ph + 4);
      }

      // 5–95% fan
      ctx.beginPath();
      ctx.moveTo(xAt(0), yAt(result.p95[0]));
      for (let i = 1; i < n; i++) ctx.lineTo(xAt(i), yAt(result.p95[i]));
      for (let i = n - 1; i >= 0; i--) ctx.lineTo(xAt(i), yAt(result.p05[i]));
      ctx.closePath();
      const c = hexToRgba(color, 0.18);
      ctx.fillStyle = c;
      ctx.fill();

      // p05 / p95 lines
      ctx.lineWidth = 1; ctx.strokeStyle = hexToRgba(color, 0.6);
      ctx.beginPath();
      result.p95.forEach((v, i) => i === 0 ? ctx.moveTo(xAt(i), yAt(v)) : ctx.lineTo(xAt(i), yAt(v)));
      ctx.stroke();
      ctx.beginPath();
      result.p05.forEach((v, i) => i === 0 ? ctx.moveTo(xAt(i), yAt(v)) : ctx.lineTo(xAt(i), yAt(v)));
      ctx.stroke();

      // median
      ctx.lineWidth = 2; ctx.strokeStyle = color;
      ctx.beginPath();
      result.p50.forEach((v, i) => i === 0 ? ctx.moveTo(xAt(i), yAt(v)) : ctx.lineTo(xAt(i), yAt(v)));
      ctx.stroke();

      // start=1 reference
      const y1 = yAt(1.0);
      ctx.strokeStyle = "#444"; ctx.setLineDash([3, 3]);
      ctx.beginPath(); ctx.moveTo(ml, y1); ctx.lineTo(ml + pw, y1); ctx.stroke();
      ctx.setLineDash([]);
    }
  }, [result, color]);
  return <div className="ccb-canvas-wrap"><canvas ref={ref} /></div>;
}

function hexToRgba(hex: string, a: number): string {
  const h = hex.replace("#", "");
  const r = parseInt(h.slice(0, 2), 16);
  const g = parseInt(h.slice(2, 4), 16);
  const b = parseInt(h.slice(4, 6), 16);
  return `rgba(${r}, ${g}, ${b}, ${a})`;
}
