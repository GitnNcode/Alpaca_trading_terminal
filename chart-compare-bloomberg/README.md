# Chart + Compare — Bloomberg-Style Build

Bloomberg-terminal-style port of the [Rust egui chart+compare GUI](../main-trading-terminal-rust/),
using the same stack idea as the real Bloomberg Professional Service:

| Layer | Bloomberg's stack | This build |
|------|------|------|
| Native shell | C++ + Chromium | **Electron** (Chromium + V8, both C++) |
| UI | JavaScript / TypeScript | **TypeScript + React + Vite** |
| Charts | bespoke renderer | [`lightweight-charts`](https://github.com/tradingview/lightweight-charts) + Canvas |
| Data / ML / quant | Python | **FastAPI sidecar** (numpy / httpx) |
| Real-time feed | proprietary | Alpaca data API (IEX) |

The **renderer never talks to Alpaca directly.** All HTTP, indicator math, risk metrics, and Monte Carlo runs through the Python sidecar — same separation Bloomberg uses between presentation and pricing.

---

## Layout

```
chart-compare-bloomberg/
├── src/
│   ├── main/              # Electron main process (TS, compiled to dist/main)
│   │   ├── main.ts        # window + lifecycle
│   │   ├── preload.ts     # context bridge → window.ccb
│   │   └── sidecar.ts     # spawn + health-poll the Python process
│   ├── renderer/          # UI (Vite + React)
│   │   ├── index.html
│   │   ├── App.tsx        # titlebar + function bar + tab routing + status bar
│   │   ├── components/
│   │   │   ├── ChartTab.tsx
│   │   │   ├── CompareTab.tsx
│   │   │   ├── FunctionBar.tsx
│   │   │   ├── StatusBar.tsx
│   │   │   ├── SymbolAutocomplete.tsx
│   │   │   ├── CredentialsModal.tsx
│   │   │   └── compare/   # NormalizedAndDrawdown, Heatmap, Scatter, MonteCarlo
│   │   ├── lib/api.ts     # localhost HTTP client → Python sidecar
│   │   └── styles/bloomberg.css
│   └── shared/types.ts    # shared between main + renderer
├── python/
│   ├── server.py          # FastAPI app
│   ├── alpaca.py          # async Alpaca bars + assets
│   ├── indicators.py      # SMA / EMA / Bollinger / VWAP / RSI / MACD / ATR
│   ├── metrics.py         # CAGR · ann vol · Sharpe · Sortino · Max DD · Calmar
│   ├── monte_carlo.py     # GBM via inline Xorshift64 + Box-Muller (numpy-vectorized)
│   ├── config.py          # shared credentials file
│   └── requirements.txt
├── package.json
├── tsconfig.json / tsconfig.main.json
├── vite.config.ts
└── CLAUDE.md
```

---

## Run it

```bash
# 1) deps
npm install
npm run py:install        # python3 -m pip install -r python/requirements.txt

# 2) dev — main (tsc -w) + renderer (vite) in parallel
npm run dev               # terminal A
npm start                 # terminal B — launches Electron, spawns the Python sidecar

# 3) auth — first run pops the AUTH modal. Or type "AUTH <GO>" in the command bar.
```

Credentials are stored at the **same path the canonical Go terminal and the Rust ports use**:

- macOS — `~/Library/Application Support/alpaca-tui/credentials.json`
- Windows — `%APPDATA%\alpaca-tui\credentials.json`
- Linux — `~/.config/alpaca-tui/credentials.json`

so binaries swap freely without re-entering keys.

---

## The Bloomberg UX, briefly

- **Function-code bar.** `GP <GO>` switches to the chart, `COMP <GO>` to compare, `AUTH <GO>` opens credentials, a bare ticker (e.g. `NVDA<Enter>`) jumps GP to that symbol. `/` focuses the command line.
- **Pure black background, amber accent.** Bloomberg's amber (#ff8c00) is the only "active" color. White is data, gray is chrome, cyan/green/yellow/red carry semantic weight.
- **Monospace + tabular numerals.** Everything aligns; nothing rounds visually.
- **Indicator hotkeys (GP tab only).** `V` volume · `E` EMA-20 · `S` SMA-50 · `B` Bollinger · `U` VWAP · `I` RSI · `O` MACD. Gated to chart-tab scope so they don't fire on inputs or while in COMP.
- **Dense status bar.** Sidecar status, credentials state, data feed, UTC clock.

---

## What's intentionally not here

This build is **chart + compare only**. Order entry, positions, trade log, account view — those live in the canonical Go terminal ([main-trading-terminal-go](../main-trading-terminal-go/)). The Bloomberg-style build is read-only market data.
