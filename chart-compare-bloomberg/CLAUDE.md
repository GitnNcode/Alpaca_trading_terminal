# Chart + Compare — Bloomberg-Style Build

Port of [chart-compare-gui-rust](../chart-compare-gui-rust/) onto a Bloomberg-Professional-style stack: Electron (C++ Chromium + V8) shell, TypeScript + React renderer, Python (FastAPI + numpy) sidecar for all Alpaca calls and quant math. Read-only — order entry stays in the canonical Go build.

## Stack
- **Main process:** Electron 28, TypeScript, `tsc -p tsconfig.main.json` → `dist/main/`
- **Renderer:** Vite + React 18 + TypeScript, [`lightweight-charts`](https://github.com/tradingview/lightweight-charts) v4 for OHLC/line panes, raw Canvas 2D for heatmap / scatter / Monte-Carlo fan
- **Sidecar:** Python 3 + FastAPI + uvicorn + numpy + httpx. Spawned by main on app start, health-polled until `/health` returns 200, killed on `before-quit`
- **IPC:** localhost HTTP. Main picks a free port, exposes it to the renderer via `contextBridge` (`window.ccb.sidecarPort`), renderer's [src/renderer/lib/api.ts](src/renderer/lib/api.ts) is the only fetch wrapper
- **Theme:** [src/renderer/styles/bloomberg.css](src/renderer/styles/bloomberg.css) — pure black, amber accent, JetBrains Mono, sharp corners, thin borders

## Commands (run from this directory)
- `npm install`
- `npm run py:install` — installs Python deps (`fastapi`, `httpx`, `numpy`, `uvicorn`)
- `npm run dev` — `tsc -w` (main) + `vite` (renderer) in parallel
- `npm start` — launches Electron (must be run after / alongside `dev` for dev mode, or after `npm run build` for prod)
- `npm run build` — main + renderer
- `python3 python/server.py --port 8765` — manual sidecar (for backend dev without Electron)

## Architecture rules

- **The renderer must never call Alpaca directly.** All HTTP, indicator math, risk metrics, Monte-Carlo simulations live in [python/](python/). Adding `fetch("data.alpaca.markets/…")` to the renderer breaks the stack metaphor and bypasses credential storage.
- **Sidecar port is dynamic.** [src/main/sidecar.ts](src/main/sidecar.ts) calls `findFreePort()` and passes `--port` to the Python process. The renderer reads the port via `window.ccb.sidecarPort()` once, then caches it. **Never hardcode 8765 in renderer code** — the env var is for `npm run py:run`, not for app code.
- **Credentials path is shared with the other builds.** [python/config.py](python/config.py) writes to `~/Library/Application Support/alpaca-tui/credentials.json` (macOS) so binaries swap freely. Don't move it under an Electron-specific user-data dir.
- **Indicator hotkeys are gated to the Chart tab.** `V B S E U I O` registered in [src/renderer/components/ChartTab.tsx](src/renderer/components/ChartTab.tsx) only — same gating discipline as the Rust port. The handler also bails when an `INPUT`/`TEXTAREA` is focused so symbol typing doesn't toggle indicators.
- **Stale-response protection: tab-specific.**
  - Chart tab: single `genRef: useRef<number>` (same pattern as Rust `gen: u64`) — one symbol at a time.
  - Compare tab: re-issues the *whole* `/compare` call with all four slots on Reload / range change. There is no per-slot async race here because the sidecar fetches all symbols in one `asyncio.gather` and returns one payload.
- **Lightweight-charts time scales are synced manually.** Each pane is its own `IChartApi` (lib doesn't support true sub-panes in v4). Sync via `subscribeVisibleLogicalRangeChange`, with a `muted` flag to break the feedback loop. The bottom-most visible pane is the only one with `timeScale().visible = true`; helper `refreshTimeScaleVisibility()` flips the flag when panes mount/unmount.
- **Lightweight-charts NaN handling.** The library refuses `NaN`/`null` values. Indicators arrive from Python as `(number | null)[]` (NaN-padded → `None`); the renderer filters nulls before `setData`.
- **Monte Carlo math lives in Python.** [python/monte_carlo.py](python/monte_carlo.py) uses the *same* Xorshift64 + Box-Muller pair as the Rust port so a given seed reproduces the same paths. The python loop generates uniforms; numpy vectorizes the Box-Muller + cumulative-sum + percentile step. Don't replace with `np.random.standard_normal` — it'd change reproducibility across builds.
- **Series alignment.** [python/metrics.py](python/metrics.py) `aligned()` right-truncates closes to the shortest length before computing the correlation matrix. The single-asset metrics path doesn't need alignment because it operates per-series.

## File map

| File | Purpose |
|------|---------|
| [src/main/main.ts](src/main/main.ts) | Electron entry — window + lifecycle |
| [src/main/sidecar.ts](src/main/sidecar.ts) | Spawn / health-poll / kill the Python process |
| [src/main/preload.ts](src/main/preload.ts) | `contextBridge` — exposes `sidecarPort()` |
| [src/renderer/App.tsx](src/renderer/App.tsx) | Titlebar · function bar · tab routing · status bar · creds modal |
| [src/renderer/components/FunctionBar.tsx](src/renderer/components/FunctionBar.tsx) | Bloomberg-style command line + function codes |
| [src/renderer/components/ChartTab.tsx](src/renderer/components/ChartTab.tsx) | Symbol bar · range/TF · indicator hotkeys · panes |
| [src/renderer/components/CompareTab.tsx](src/renderer/components/CompareTab.tsx) | Slot bar · range · metrics table · grid of viz cards |
| [src/renderer/components/compare/NormalizedAndDrawdown.tsx](src/renderer/components/compare/NormalizedAndDrawdown.tsx) | Two synced lightweight-charts panes |
| [src/renderer/components/compare/Heatmap.tsx](src/renderer/components/compare/Heatmap.tsx) | Canvas correlation heatmap |
| [src/renderer/components/compare/Scatter.tsx](src/renderer/components/compare/Scatter.tsx) | Canvas vol-vs-CAGR scatter |
| [src/renderer/components/compare/MonteCarlo.tsx](src/renderer/components/compare/MonteCarlo.tsx) | Controls + canvas fan + summary KVs |
| [src/renderer/lib/api.ts](src/renderer/lib/api.ts) | Only place the renderer talks to the sidecar |
| [src/renderer/styles/bloomberg.css](src/renderer/styles/bloomberg.css) | The whole theme |
| [python/server.py](python/server.py) | FastAPI routes: `/health` `/credentials` `/assets` `/bars` `/compare` `/montecarlo` |

## Don't

- Don't add `fetch("data.alpaca.markets/…")` to the renderer — all market data goes through the Python sidecar.
- Don't hardcode `8765` or any port in renderer code. The port is dynamic (see [src/main/sidecar.ts](src/main/sidecar.ts)).
- Don't move the credentials file under Electron's `app.getPath("userData")`. The shared path is load-bearing for cross-build credential reuse.
- Don't ungate the Chart-tab hotkeys (`V B S E U I O`) — they'll fire while the user types into the Compare slot inputs.
- Don't replace the Xorshift64 RNG with `np.random` — reproducibility with the Rust port is intentional.
- Don't push lightweight-charts series with `NaN` / `null` values; filter first.
- Don't add order-entry or account-view features here — those live in [../main-trading-terminal-go/](../main-trading-terminal-go/).
