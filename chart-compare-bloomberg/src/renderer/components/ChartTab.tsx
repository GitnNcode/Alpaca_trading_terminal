import React, { useEffect, useMemo, useRef, useState } from "react";
import {
  createChart,
  CrosshairMode,
  ColorType,
  LineStyle,
  type IChartApi,
  type ISeriesApi,
  type Time,
  type UTCTimestamp,
  type LogicalRange,
} from "lightweight-charts";
import { api, defaultTimeframe, rangeWindow } from "../lib/api";
import type {
  BarsResponse,
  Bar,
  ChartRange,
  IndicatorKey,
  Timeframe,
} from "@shared/types";
import { fmtMoney, fmtVol, fmtSigned, fmtPct, classChange, toUnixSec } from "../lib/format";
import { SymbolAutocomplete } from "./SymbolAutocomplete";

const RANGES: ChartRange[] = ["1D", "1W", "1M", "YTD", "1Y", "5Y", "MAX"];
const TIMEFRAMES: Timeframe[] = ["1Min", "5Min", "15Min", "30Min", "1Hour", "1Day", "1Week", "1Month"];

const HOTKEYS: Record<string, keyof IndState> = {
  V: "volume",
  E: "ema",
  S: "sma",
  B: "bb",
  U: "vwap",
  I: "rsi",
  O: "macd",
};

interface IndState {
  volume: boolean;
  ema: boolean;
  sma: boolean;
  bb: boolean;
  vwap: boolean;
  rsi: boolean;
  macd: boolean;
}

const DEFAULTS: IndState = { volume: true, ema: true, sma: false, bb: false, vwap: false, rsi: false, macd: false };

const PALETTE = {
  upBody: "#00ff5a",
  downBody: "#ff3b3b",
  upWick: "#00ff5a",
  downWick: "#ff3b3b",
  grid: "#0e0e0e",
  bg: "#000000",
  text: "#9a9a9a",
  ema: "#00bfff",
  sma: "#ffd700",
  bbMid: "#888888",
  bbBand: "#5a5a5a",
  vwap: "#ffae3a",
  rsi: "#00bfff",
  macdLine: "#00bfff",
  macdSig: "#ffd700",
};

const COMMON_LAYOUT = {
  background: { type: ColorType.Solid, color: PALETTE.bg },
  textColor: PALETTE.text,
  fontFamily: '"JetBrains Mono", monospace',
  fontSize: 11,
};

const COMMON_GRID = {
  vertLines: { color: PALETTE.grid, style: LineStyle.Solid },
  horzLines: { color: PALETTE.grid, style: LineStyle.Solid },
};

interface Props {
  symbol: string;
  setSymbol: (s: string) => void;
  ready: boolean;
}

export function ChartTab({ symbol, setSymbol, ready }: Props) {
  const [range, setRange] = useState<ChartRange>("1Y");
  const [tf, setTf] = useState<Timeframe>(defaultTimeframe("1Y"));
  const [ind, setInd] = useState<IndState>(DEFAULTS);
  const [resp, setResp] = useState<BarsResponse | null>(null);
  const [loading, setLoading] = useState(false);
  const [err, setErr] = useState<string | null>(null);
  const genRef = useRef(0);

  // ---- hotkeys (Chart tab only — gated by render scope) ----
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const tag = (e.target as HTMLElement | null)?.tagName;
      if (tag === "INPUT" || tag === "TEXTAREA") return;
      if (e.metaKey || e.ctrlKey || e.altKey) return;
      const k = e.key.toUpperCase();
      const slot = HOTKEYS[k];
      if (slot) {
        e.preventDefault();
        setInd((p) => ({ ...p, [slot]: !p[slot] }));
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  // ---- when range changes, choose a sensible default timeframe ----
  useEffect(() => { setTf(defaultTimeframe(range)); }, [range]);

  // ---- fetch ----
  useEffect(() => {
    if (!symbol) { setResp(null); setErr(null); return; }
    if (!ready) return;
    const myGen = ++genRef.current;
    const { start, end } = rangeWindow(range);
    const keys: IndicatorKey[] = [];
    if (ind.ema) keys.push("ema");
    if (ind.sma) keys.push("sma");
    if (ind.bb) keys.push("bb");
    if (ind.vwap) keys.push("vwap");
    if (ind.rsi) keys.push("rsi");
    if (ind.macd) keys.push("macd");
    setLoading(true);
    setErr(null);
    api.bars(symbol, tf, start, end, keys)
      .then((r) => { if (genRef.current === myGen) setResp(r); })
      .catch((e) => { if (genRef.current === myGen) setErr(String(e?.message ?? e)); })
      .finally(() => { if (genRef.current === myGen) setLoading(false); });
  }, [symbol, range, tf, ind, ready]);

  const stats = useMemo(() => {
    if (!resp || resp.bars.length === 0) return null;
    const bars = resp.bars;
    const last = bars[bars.length - 1];
    const first = bars[0];
    const prev = bars.length > 1 ? bars[bars.length - 2] : first;
    const dChg = last.c - prev.c;
    const pctChg = prev.c ? dChg / prev.c : 0;
    const h = bars.reduce((m, b) => Math.max(m, b.h), bars[0].h);
    const l = bars.reduce((m, b) => Math.min(m, b.l), bars[0].l);
    const vol = bars.reduce((s, b) => s + b.v, 0);
    return { last, first, dChg, pctChg, h, l, vol };
  }, [resp]);

  return (
    <div className="ccb-chart">
      <div className="toolbar">
        <div className="group">
          <span className="label">SYMBOL</span>
          <SymbolAutocomplete
            value={symbol}
            onChange={(v) => setSymbol(v.toUpperCase())}
            onSubmit={(s) => setSymbol(s.toUpperCase())}
          />
        </div>
        <div className="ccb-divider" />
        <div className="group">
          <span className="label">RANGE</span>
          {RANGES.map((r) => (
            <button key={r} className={r === range ? "active" : ""} onClick={() => setRange(r)}>{r}</button>
          ))}
        </div>
        <div className="ccb-divider" />
        <div className="group">
          <span className="label">TF</span>
          <select aria-label="Timeframe" title="Timeframe" value={tf} onChange={(e) => setTf(e.target.value as Timeframe)} style={{ background: "#000", color: "var(--ccb-fg)", border: "1px solid var(--ccb-border-hi)", padding: "2px 4px" }}>
            {TIMEFRAMES.map((t) => <option key={t} value={t}>{t}</option>)}
          </select>
        </div>
        <div className="ccb-divider" />
        <div className="group">
          <span className="label">INDICATORS</span>
          <IndBtn label="V · VOL" on={ind.volume} onClick={() => setInd({ ...ind, volume: !ind.volume })} />
          <IndBtn label="E · EMA20" on={ind.ema} onClick={() => setInd({ ...ind, ema: !ind.ema })} />
          <IndBtn label="S · SMA50" on={ind.sma} onClick={() => setInd({ ...ind, sma: !ind.sma })} />
          <IndBtn label="B · BB" on={ind.bb} onClick={() => setInd({ ...ind, bb: !ind.bb })} />
          <IndBtn label="U · VWAP" on={ind.vwap} onClick={() => setInd({ ...ind, vwap: !ind.vwap })} />
          <IndBtn label="I · RSI" on={ind.rsi} onClick={() => setInd({ ...ind, rsi: !ind.rsi })} />
          <IndBtn label="O · MACD" on={ind.macd} onClick={() => setInd({ ...ind, macd: !ind.macd })} />
        </div>
        <div className="ccb-spacer" />
        <span style={{ color: "var(--ccb-fg-muted)", fontSize: 10, letterSpacing: "0.12em" }}>
          {loading ? "LOADING…" : err ? <span style={{ color: "var(--ccb-red)" }}>ERR {err}</span> : `${resp?.bars.length ?? 0} BARS`}
        </span>
      </div>

      {stats && resp && (
        <div className="info-row">
          <span className="symbol">{resp.symbol}</span>
          <KV k="LAST" v={fmtMoney(stats.last.c)} />
          <KV k="Δ$" v={fmtSigned(stats.dChg)} cls={classChange(stats.dChg)} />
          <KV k="%Δ" v={fmtPct(stats.pctChg)} cls={classChange(stats.pctChg)} />
          <KV k="H" v={fmtMoney(stats.h)} />
          <KV k="L" v={fmtMoney(stats.l)} />
          <KV k="VOL" v={fmtVol(stats.vol)} />
          <KV k="O" v={fmtMoney(stats.last.o)} />
          <KV k="C(PREV)" v={fmtMoney(stats.first.c)} />
          <KV k="BARS" v={String(resp.bars.length)} />
          <KV k="TF" v={resp.timeframe} />
        </div>
      )}

      <ChartPanes resp={resp} ind={ind} />
    </div>
  );
}

function KV({ k, v, cls }: { k: string; v: string; cls?: string }) {
  return <span className="kv"><span className="k">{k}</span><span className={"v " + (cls ?? "")}>{v}</span></span>;
}

function IndBtn({ label, on, onClick }: { label: string; on: boolean; onClick: () => void }) {
  return <button className={on ? "active" : ""} onClick={onClick} style={{ fontSize: 10, letterSpacing: "0.08em" }}>{label}</button>;
}

// =====================================================================
// PANES (lightweight-charts) — synced time scale across panes
// =====================================================================

interface PanesProps {
  resp: BarsResponse | null;
  ind: IndState;
}

function ChartPanes({ resp, ind }: PanesProps) {
  const priceRef = useRef<HTMLDivElement>(null);
  const volRef = useRef<HTMLDivElement>(null);
  const rsiRef = useRef<HTMLDivElement>(null);
  const macdRef = useRef<HTMLDivElement>(null);

  const priceChart = useRef<IChartApi | null>(null);
  const volChart = useRef<IChartApi | null>(null);
  const rsiChart = useRef<IChartApi | null>(null);
  const macdChart = useRef<IChartApi | null>(null);

  const candleSeries = useRef<ISeriesApi<"Candlestick"> | null>(null);
  const overlaySeries = useRef<Record<string, ISeriesApi<any>>>({});
  const volSeries = useRef<ISeriesApi<"Histogram"> | null>(null);
  const rsiSeries = useRef<ISeriesApi<"Line"> | null>(null);
  const macdLine = useRef<ISeriesApi<"Line"> | null>(null);
  const macdSig = useRef<ISeriesApi<"Line"> | null>(null);
  const macdHist = useRef<ISeriesApi<"Histogram"> | null>(null);

  // create + destroy the price chart on mount / unmount
  useEffect(() => {
    if (priceRef.current && !priceChart.current) {
      priceChart.current = createChart(priceRef.current, {
        layout: COMMON_LAYOUT,
        grid: COMMON_GRID,
        rightPriceScale: { borderColor: "#1c1c1c" },
        timeScale: { borderColor: "#1c1c1c", timeVisible: true, secondsVisible: false, rightOffset: 4 },
        crosshair: { mode: CrosshairMode.Normal, vertLine: { color: "#ff8c00", style: LineStyle.Dashed }, horzLine: { color: "#ff8c00", style: LineStyle.Dashed } },
        autoSize: true,
      });
      candleSeries.current = priceChart.current.addCandlestickSeries({
        upColor: PALETTE.upBody, downColor: PALETTE.downBody,
        wickUpColor: PALETTE.upWick, wickDownColor: PALETTE.downWick,
        borderVisible: false,
        priceFormat: { type: "price", precision: 2, minMove: 0.01 },
      });
    }
    return () => {
      // tear down ALL charts on unmount (tab switch / HMR)
      for (const ref of [volChart, rsiChart, macdChart, priceChart]) {
        try { ref.current?.remove(); } catch { /* noop */ }
        ref.current = null;
      }
      candleSeries.current = null;
      overlaySeries.current = {};
      volSeries.current = null;
      rsiSeries.current = null;
      macdLine.current = macdSig.current = macdHist.current = null;
    };
  }, []);

  // sub-pane lifecycle
  useEffect(() => {
    if (ind.volume) {
      if (volRef.current && !volChart.current) {
        volChart.current = createChart(volRef.current, {
          layout: COMMON_LAYOUT, grid: COMMON_GRID,
          rightPriceScale: { borderColor: "#1c1c1c" },
          timeScale: { borderColor: "#1c1c1c", visible: false },
          crosshair: { mode: CrosshairMode.Normal },
          autoSize: true,
        });
        volSeries.current = volChart.current.addHistogramSeries({ priceFormat: { type: "volume" }, priceScaleId: "" });
        volChart.current.priceScale("").applyOptions({ scaleMargins: { top: 0.1, bottom: 0 } });
      }
    } else if (volChart.current) { volChart.current.remove(); volChart.current = null; volSeries.current = null; }

    if (ind.rsi) {
      if (rsiRef.current && !rsiChart.current) {
        rsiChart.current = createChart(rsiRef.current, {
          layout: COMMON_LAYOUT, grid: COMMON_GRID,
          rightPriceScale: { borderColor: "#1c1c1c" },
          timeScale: { borderColor: "#1c1c1c", visible: false },
          crosshair: { mode: CrosshairMode.Normal },
          autoSize: true,
        });
        // Lock the y-axis to [0,100] via the series autoscaleInfoProvider —
        // lightweight-charts v4 has no public setVisiblePriceRange.
        rsiSeries.current = rsiChart.current.addLineSeries({
          color: PALETTE.rsi,
          lineWidth: 1,
          priceLineVisible: false,
          lastValueVisible: true,
          autoscaleInfoProvider: () => ({ priceRange: { minValue: 0, maxValue: 100 } }),
        });
        rsiSeries.current.createPriceLine({ price: 70, color: "#444", lineWidth: 1, lineStyle: LineStyle.Dashed, axisLabelVisible: true, title: "70" });
        rsiSeries.current.createPriceLine({ price: 50, color: "#222", lineWidth: 1, lineStyle: LineStyle.Dotted, axisLabelVisible: true, title: "50" });
        rsiSeries.current.createPriceLine({ price: 30, color: "#444", lineWidth: 1, lineStyle: LineStyle.Dashed, axisLabelVisible: true, title: "30" });
      }
    } else if (rsiChart.current) { rsiChart.current.remove(); rsiChart.current = null; rsiSeries.current = null; }

    if (ind.macd) {
      if (macdRef.current && !macdChart.current) {
        macdChart.current = createChart(macdRef.current, {
          layout: COMMON_LAYOUT, grid: COMMON_GRID,
          rightPriceScale: { borderColor: "#1c1c1c" },
          timeScale: { borderColor: "#1c1c1c", visible: true, timeVisible: true, secondsVisible: false },
          crosshair: { mode: CrosshairMode.Normal },
          autoSize: true,
        });
        macdHist.current = macdChart.current.addHistogramSeries({ priceFormat: { type: "price", precision: 4, minMove: 0.0001 } });
        macdLine.current = macdChart.current.addLineSeries({ color: PALETTE.macdLine, lineWidth: 1 });
        macdSig.current = macdChart.current.addLineSeries({ color: PALETTE.macdSig, lineWidth: 1 });
      }
    } else if (macdChart.current) { macdChart.current.remove(); macdChart.current = null; macdLine.current = macdSig.current = macdHist.current = null; }

    // when MACD pane is removed/added we have to flip time-axis visibility on others
    refreshTimeScaleVisibility();
  }, [ind.volume, ind.rsi, ind.macd]);

  function refreshTimeScaleVisibility() {
    const bottomChart = macdChart.current ?? rsiChart.current ?? volChart.current ?? priceChart.current;
    const all = [priceChart.current, volChart.current, rsiChart.current, macdChart.current].filter(Boolean) as IChartApi[];
    all.forEach((c) => c.timeScale().applyOptions({ visible: c === bottomChart, timeVisible: c === bottomChart, secondsVisible: false }));
  }

  // sync visible time range across panes
  useEffect(() => {
    const charts = () => [priceChart.current, volChart.current, rsiChart.current, macdChart.current].filter(Boolean) as IChartApi[];
    const unsubs: (() => void)[] = [];
    let muted = false;
    const sync = (source: IChartApi) => (range: LogicalRange | null) => {
      if (muted || !range) return;
      muted = true;
      try {
        charts().forEach((c) => { if (c !== source) c.timeScale().setVisibleLogicalRange(range); });
      } finally {
        muted = false;
      }
    };
    charts().forEach((c) => {
      const fn = sync(c);
      c.timeScale().subscribeVisibleLogicalRangeChange(fn);
      unsubs.push(() => c.timeScale().unsubscribeVisibleLogicalRangeChange(fn));
    });
    return () => unsubs.forEach((u) => u());
  }, [ind.volume, ind.rsi, ind.macd]);

  // ---- write data ----
  useEffect(() => {
    if (!resp || !candleSeries.current || !priceChart.current) return;
    const bars = resp.bars;
    const candles = bars.map<{ time: UTCTimestamp; open: number; high: number; low: number; close: number }>((b) => ({
      time: toUnixSec(b.t) as UTCTimestamp,
      open: b.o, high: b.h, low: b.l, close: b.c,
    }));
    candleSeries.current.setData(candles);

    // overlay management on the price pane: SMA / EMA / BB / VWAP
    const want: Record<string, { color: string; lineWidth: 1 | 2 }> = {};
    if (resp.indicators.ema && ind.ema) want.ema = { color: PALETTE.ema, lineWidth: 1 };
    if (resp.indicators.sma && ind.sma) want.sma = { color: PALETTE.sma, lineWidth: 1 };
    if (resp.indicators.vwap && ind.vwap) want.vwap = { color: PALETTE.vwap, lineWidth: 1 };
    if (resp.indicators.bb && ind.bb) {
      want.bb_u = { color: PALETTE.bbBand, lineWidth: 1 };
      want.bb_m = { color: PALETTE.bbMid, lineWidth: 1 };
      want.bb_l = { color: PALETTE.bbBand, lineWidth: 1 };
    }

    // remove series no longer wanted
    for (const k of Object.keys(overlaySeries.current)) {
      if (!want[k]) {
        try { priceChart.current.removeSeries(overlaySeries.current[k]); } catch { /* noop */ }
        delete overlaySeries.current[k];
      }
    }
    // add new
    for (const [k, opts] of Object.entries(want)) {
      if (!overlaySeries.current[k]) {
        overlaySeries.current[k] = priceChart.current.addLineSeries({ color: opts.color, lineWidth: opts.lineWidth, priceLineVisible: false, lastValueVisible: false, crosshairMarkerVisible: false });
      }
    }
    // feed data
    const feed = (s: ISeriesApi<any>, arr: (number | null)[]) => {
      const pts: { time: UTCTimestamp; value: number }[] = [];
      for (let i = 0; i < bars.length; i++) {
        const v = arr[i];
        if (v != null && isFinite(v)) pts.push({ time: toUnixSec(bars[i].t) as UTCTimestamp, value: v });
      }
      s.setData(pts);
    };
    if (overlaySeries.current.ema && resp.indicators.ema) feed(overlaySeries.current.ema, resp.indicators.ema);
    if (overlaySeries.current.sma && resp.indicators.sma) feed(overlaySeries.current.sma, resp.indicators.sma);
    if (overlaySeries.current.vwap && resp.indicators.vwap) feed(overlaySeries.current.vwap, resp.indicators.vwap);
    if (resp.indicators.bb) {
      if (overlaySeries.current.bb_u) feed(overlaySeries.current.bb_u, resp.indicators.bb.upper);
      if (overlaySeries.current.bb_m) feed(overlaySeries.current.bb_m, resp.indicators.bb.mid);
      if (overlaySeries.current.bb_l) feed(overlaySeries.current.bb_l, resp.indicators.bb.lower);
    }

    // volume
    if (volSeries.current) {
      const data = bars.map((b) => ({
        time: toUnixSec(b.t) as UTCTimestamp,
        value: b.v,
        color: b.c >= b.o ? "rgba(0,255,90,0.55)" : "rgba(255,59,59,0.55)",
      }));
      volSeries.current.setData(data);
    }

    // RSI — series-level autoscaleInfoProvider locks the y-axis to [0,100],
    // so we just feed the data; no per-update priceScale calls needed.
    if (rsiSeries.current && resp.indicators.rsi) {
      const pts: { time: UTCTimestamp; value: number }[] = [];
      for (let i = 0; i < bars.length; i++) {
        const v = resp.indicators.rsi[i];
        if (v != null) pts.push({ time: toUnixSec(bars[i].t) as UTCTimestamp, value: v });
      }
      rsiSeries.current.setData(pts);
    }

    // MACD
    if (resp.indicators.macd) {
      const ll = resp.indicators.macd.line;
      const ss = resp.indicators.macd.signal;
      const hh = resp.indicators.macd.hist;
      const t = (i: number) => toUnixSec(bars[i].t) as UTCTimestamp;
      if (macdLine.current) macdLine.current.setData(ll.map((v, i) => v == null ? null : { time: t(i), value: v }).filter(Boolean) as any);
      if (macdSig.current) macdSig.current.setData(ss.map((v, i) => v == null ? null : { time: t(i), value: v }).filter(Boolean) as any);
      if (macdHist.current) macdHist.current.setData(hh.map((v, i) => v == null ? null : { time: t(i), value: v, color: v >= 0 ? "rgba(0,255,90,0.7)" : "rgba(255,59,59,0.7)" }).filter(Boolean) as any);
    }

    priceChart.current.timeScale().fitContent();
    refreshTimeScaleVisibility();
  }, [resp, ind.ema, ind.sma, ind.bb, ind.vwap]);

  // ResizeObserver handled by `autoSize: true` in lightweight-charts.

  const rows: string[] = ["price"];
  if (ind.volume) rows.push("vol");
  if (ind.rsi) rows.push("rsi");
  if (ind.macd) rows.push("macd");
  const weight: Record<string, number> = { price: 6, vol: 1.2, rsi: 1.8, macd: 1.8 };
  const sum = rows.reduce((s, k) => s + (weight[k] ?? 1), 0);
  const styleFor = (k: string) => ({ flex: `${(weight[k] ?? 1) / sum}` });

  return (
    <div className="panes">
      <div className="pane" style={styleFor("price")}>
        <div className="pane-tag">PRICE</div>
        <div ref={priceRef} style={{ position: "absolute", inset: 0 }} />
      </div>
      {ind.volume && (
        <div className="pane" style={styleFor("vol")}>
          <div className="pane-tag">VOLUME</div>
          <div ref={volRef} style={{ position: "absolute", inset: 0 }} />
        </div>
      )}
      {ind.rsi && (
        <div className="pane" style={styleFor("rsi")}>
          <div className="pane-tag">RSI (14)</div>
          <div ref={rsiRef} style={{ position: "absolute", inset: 0 }} />
        </div>
      )}
      {ind.macd && (
        <div className="pane" style={styleFor("macd")}>
          <div className="pane-tag">MACD (12/26/9)</div>
          <div ref={macdRef} style={{ position: "absolute", inset: 0 }} />
        </div>
      )}
    </div>
  );
}
