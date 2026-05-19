import React, { useEffect, useRef } from "react";

interface Props {
  labels: string[];
  matrix: number[][];
}

export function Heatmap({ labels, matrix }: Props) {
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
      ctx.clearRect(0, 0, w, h);
      ctx.fillStyle = "#000";
      ctx.fillRect(0, 0, w, h);

      const n = labels.length;
      if (n === 0 || matrix.length === 0) {
        ctx.fillStyle = "#444";
        ctx.font = '11px "JetBrains Mono", monospace';
        ctx.textAlign = "center";
        ctx.fillText("NO DATA — LOAD 2+ SYMBOLS", w / 2, h / 2);
        return;
      }

      // layout: top + left margin for labels, square grid
      const ml = 60, mt = 30, mr = 10, mb = 10;
      const gridW = Math.max(0, w - ml - mr);
      const gridH = Math.max(0, h - mt - mb);
      const side = Math.min(gridW, gridH);
      const cell = side / n;

      // cell colors
      for (let i = 0; i < n; i++) {
        for (let j = 0; j < n; j++) {
          const c = matrix[i]?.[j];
          if (typeof c !== "number") continue;
          ctx.fillStyle = corrColor(c);
          ctx.fillRect(ml + j * cell, mt + i * cell, cell - 1, cell - 1);
          // number label
          ctx.fillStyle = Math.abs(c) > 0.6 ? "#000" : "#fff";
          ctx.font = '10px "JetBrains Mono", monospace';
          ctx.textAlign = "center";
          ctx.textBaseline = "middle";
          ctx.fillText(c.toFixed(2), ml + j * cell + cell / 2, mt + i * cell + cell / 2);
        }
      }

      // labels (top + left)
      ctx.fillStyle = "#ff8c00";
      ctx.font = '10px "JetBrains Mono", monospace';
      ctx.textAlign = "center";
      ctx.textBaseline = "bottom";
      for (let j = 0; j < n; j++) {
        ctx.fillText(labels[j], ml + j * cell + cell / 2, mt - 4);
      }
      ctx.textAlign = "right";
      ctx.textBaseline = "middle";
      for (let i = 0; i < n; i++) {
        ctx.fillText(labels[i], ml - 6, mt + i * cell + cell / 2);
      }
    }
  }, [labels, matrix]);

  return (
    <div className="ccb-canvas-wrap"><canvas ref={ref} /></div>
  );
}

function corrColor(c: number): string {
  // c in [-1, 1]: red ↔ black ↔ green
  const a = Math.max(-1, Math.min(1, c));
  if (a >= 0) {
    const i = Math.round(a * 200);
    return `rgb(${Math.max(0, 30 - i / 3)}, ${i}, ${Math.max(0, 30 - i / 3)})`;
  }
  const i = Math.round(-a * 200);
  return `rgb(${i}, ${Math.max(0, 30 - i / 3)}, ${Math.max(0, 30 - i / 3)})`;
}
