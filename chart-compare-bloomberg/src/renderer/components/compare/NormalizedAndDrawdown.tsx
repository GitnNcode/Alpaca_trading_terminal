import React, { useEffect, useRef } from "react";
import {
  ColorType,
  createChart,
  CrosshairMode,
  LineStyle,
  type IChartApi,
  type UTCTimestamp,
} from "lightweight-charts";
import type { CompareSeriesOk } from "@shared/types";
import { toUnixSec } from "../../lib/format";

interface Props {
  series: CompareSeriesOk[];
  colors: string[];
}

export function NormalizedAndDrawdown({ series, colors }: Props) {
  const topRef = useRef<HTMLDivElement>(null);
  const botRef = useRef<HTMLDivElement>(null);
  const topChart = useRef<IChartApi | null>(null);
  const botChart = useRef<IChartApi | null>(null);

  useEffect(() => {
    if (topRef.current && !topChart.current) {
      topChart.current = createChart(topRef.current, {
        layout: { background: { type: ColorType.Solid, color: "#000" }, textColor: "#9a9a9a", fontFamily: '"JetBrains Mono", monospace', fontSize: 10 },
        grid: { vertLines: { color: "#0e0e0e" }, horzLines: { color: "#0e0e0e" } },
        rightPriceScale: { borderColor: "#1c1c1c" },
        timeScale: { borderColor: "#1c1c1c", visible: false, timeVisible: true },
        crosshair: { mode: CrosshairMode.Normal, vertLine: { color: "#ff8c00", style: LineStyle.Dashed } },
        autoSize: true,
      });
    }
    if (botRef.current && !botChart.current) {
      botChart.current = createChart(botRef.current, {
        layout: { background: { type: ColorType.Solid, color: "#000" }, textColor: "#9a9a9a", fontFamily: '"JetBrains Mono", monospace', fontSize: 10 },
        grid: { vertLines: { color: "#0e0e0e" }, horzLines: { color: "#0e0e0e" } },
        rightPriceScale: { borderColor: "#1c1c1c" },
        timeScale: { borderColor: "#1c1c1c", visible: true, timeVisible: true, secondsVisible: false },
        crosshair: { mode: CrosshairMode.Normal, vertLine: { color: "#ff8c00", style: LineStyle.Dashed } },
        autoSize: true,
      });
    }
    return () => {};
  }, []);

  // sync time scales
  useEffect(() => {
    if (!topChart.current || !botChart.current) return;
    let muted = false;
    const subA = topChart.current.timeScale().subscribeVisibleLogicalRangeChange((r) => {
      if (muted || !r || !botChart.current) return;
      muted = true; botChart.current.timeScale().setVisibleLogicalRange(r); muted = false;
    });
    const subB = botChart.current.timeScale().subscribeVisibleLogicalRangeChange((r) => {
      if (muted || !r || !topChart.current) return;
      muted = true; topChart.current.timeScale().setVisibleLogicalRange(r); muted = false;
    });
    return () => {
      topChart.current?.timeScale().unsubscribeVisibleLogicalRangeChange(subA as any);
      botChart.current?.timeScale().unsubscribeVisibleLogicalRangeChange(subB as any);
    };
  }, []);

  // write data
  useEffect(() => {
    if (!topChart.current || !botChart.current) return;
    // wipe previous: lightweight-charts has no "removeAllSeries" — clear via re-create cheap path
    // we re-create them when series array length changes (uncommon enough)
    const tc = topChart.current; const bc = botChart.current;
    // workaround: hold refs to series across renders
  }, [series]);

  // Re-render strategy: nuke + recreate series on each data change. Simpler than diffing for ≤4 lines.
  const seriesKey = series.map(s => s.symbol).join(",") + ":" + (series[0]?.times.length ?? 0);
  useEffect(() => {
    if (!topChart.current || !botChart.current) return;
    // remove all existing series via API
    const tc = topChart.current; const bc = botChart.current;
    // lightweight-charts < 5 doesn't expose a list, so we track via attaching to a private map:
    const tcAny = tc as any; const bcAny = bc as any;
    for (const s of (tcAny._ccbSeries ?? [])) tc.removeSeries(s);
    for (const s of (bcAny._ccbSeries ?? [])) bc.removeSeries(s);
    tcAny._ccbSeries = []; bcAny._ccbSeries = [];

    series.forEach((s, i) => {
      const color = colors[i] ?? "#999";
      const normLine = tc.addLineSeries({ color, lineWidth: 1, priceLineVisible: false, lastValueVisible: true, title: s.symbol });
      const ddLine = bc.addLineSeries({ color, lineWidth: 1, priceLineVisible: false, lastValueVisible: false });
      tcAny._ccbSeries.push(normLine);
      bcAny._ccbSeries.push(ddLine);
      normLine.setData(s.times.map((t, j) => ({ time: toUnixSec(t) as UTCTimestamp, value: s.normalized[j] })));
      ddLine.setData(s.times.map((t, j) => ({ time: toUnixSec(t) as UTCTimestamp, value: s.drawdown[j] })));
    });
    tc.timeScale().fitContent();
  }, [seriesKey, colors]);

  return (
    <div style={{ position: "absolute", inset: 0, display: "flex", flexDirection: "column" }}>
      <div style={{ flex: 2, position: "relative" }}>
        <div className="pane-tag" style={{ position: "absolute", top: 4, left: 8, zIndex: 3, fontSize: 10, letterSpacing: "0.22em", color: "#666", background: "rgba(0,0,0,0.6)", padding: "1px 6px", borderLeft: "2px solid #ff8c00" }}>NORMALIZED · 100</div>
        <div ref={topRef} style={{ position: "absolute", inset: 0 }} />
      </div>
      <div style={{ flex: 1, position: "relative", borderTop: "1px solid #1c1c1c" }}>
        <div className="pane-tag" style={{ position: "absolute", top: 4, left: 8, zIndex: 3, fontSize: 10, letterSpacing: "0.22em", color: "#666", background: "rgba(0,0,0,0.6)", padding: "1px 6px", borderLeft: "2px solid #ff8c00" }}>DRAWDOWN</div>
        <div ref={botRef} style={{ position: "absolute", inset: 0 }} />
      </div>
    </div>
  );
}
