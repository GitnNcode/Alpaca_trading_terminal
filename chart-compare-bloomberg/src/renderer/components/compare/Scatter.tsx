import React, { useEffect, useRef } from "react";
import type { CompareSeriesOk } from "@shared/types";

interface Props {
  series: CompareSeriesOk[];
  colors: string[];
}

export function Scatter({ series, colors }: Props) {
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
      const w = canvas.clientWidth;
      const h = canvas.clientHeight;
      canvas.width = w * dpr;
      canvas.height = h * dpr;
      const ctx = canvas.getContext("2d")!;
      ctx.scale(dpr, dpr);
      ctx.fillStyle = "#000";
      ctx.fillRect(0, 0, w, h);

      const ml = 48, mb = 30, mt = 14, mr = 14;
      const pw = w - ml - mr;
      const ph = h - mt - mb;

      // axes ranges
      const vols = series.map(s => s.metrics.ann_vol);
      const cagrs = series.map(s => s.metrics.cagr);
      const vMax = Math.max(0.6, Math.ceil((Math.max(0.01, ...vols) + 0.05) * 10) / 10);
      const cMax = Math.max(0.5, Math.ceil((Math.max(0.01, ...cagrs) + 0.05) * 10) / 10);
      const cMin = Math.min(-0.5, Math.floor((Math.min(0, ...cagrs) - 0.05) * 10) / 10);

      ctx.strokeStyle = "#1c1c1c";
      ctx.lineWidth = 1;
      // grid + axis labels
      ctx.font = '10px "JetBrains Mono", monospace';
      ctx.fillStyle = "#666";
      ctx.textAlign = "right";
      ctx.textBaseline = "middle";
      for (let k = 0; k <= 5; k++) {
        const yv = cMin + ((cMax - cMin) * k) / 5;
        const y = mt + ph - (ph * k) / 5;
        ctx.beginPath(); ctx.moveTo(ml, y); ctx.lineTo(ml + pw, y); ctx.stroke();
        ctx.fillText((yv * 100).toFixed(0) + "%", ml - 6, y);
      }
      ctx.textAlign = "center"; ctx.textBaseline = "top";
      for (let k = 0; k <= 5; k++) {
        const xv = (vMax * k) / 5;
        const x = ml + (pw * k) / 5;
        ctx.beginPath(); ctx.moveTo(x, mt); ctx.lineTo(x, mt + ph); ctx.stroke();
        ctx.fillText((xv * 100).toFixed(0) + "%", x, mt + ph + 4);
      }

      // zero-cagr line
      const zeroY = mt + ph - (ph * (0 - cMin)) / (cMax - cMin);
      ctx.strokeStyle = "#3a3a3a";
      ctx.setLineDash([3, 3]);
      ctx.beginPath(); ctx.moveTo(ml, zeroY); ctx.lineTo(ml + pw, zeroY); ctx.stroke();
      ctx.setLineDash([]);

      // axis titles
      ctx.fillStyle = "#ff8c00";
      ctx.textAlign = "right"; ctx.textBaseline = "top";
      ctx.fillText("CAGR", ml - 4, mt - 12);
      ctx.textAlign = "right"; ctx.textBaseline = "bottom";
      ctx.fillText("ANN VOL →", ml + pw, mt + ph + 18);

      // points
      series.forEach((s, i) => {
        const x = ml + (s.metrics.ann_vol / vMax) * pw;
        const y = mt + ph - ((s.metrics.cagr - cMin) / (cMax - cMin)) * ph;
        ctx.fillStyle = colors[i] ?? "#fff";
        ctx.beginPath();
        ctx.arc(x, y, 6, 0, Math.PI * 2);
        ctx.fill();
        ctx.strokeStyle = "#000"; ctx.lineWidth = 2; ctx.stroke();
        ctx.fillStyle = colors[i] ?? "#fff";
        ctx.font = '11px "JetBrains Mono", monospace';
        ctx.textAlign = "left"; ctx.textBaseline = "middle";
        ctx.fillText(" " + s.symbol, x + 8, y);
      });
    }
  }, [series, colors]);

  return <div className="ccb-canvas-wrap"><canvas ref={ref} /></div>;
}
