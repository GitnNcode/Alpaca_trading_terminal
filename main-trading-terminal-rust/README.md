# Alpaca Trading Terminal — Rust + egui

A native desktop trading terminal for [Alpaca Markets](https://alpaca.markets/),
built in Rust on [eframe / egui](https://github.com/emilk/egui) +
[egui_plot](https://docs.rs/egui_plot). Three tabs in one fast window, with
real-time data streaming over WebSocket. **Paper-trading by default.**

> Package name `alpaca-egui`; the window title and distributed binaries are
> branded **Alpaca Trading Terminal**.

## Tabs

The top strip switches between three workspaces (default tab is **Trading
Terminal**):

- **Trading Terminal** — account-management surface with `Positions / Trade /
  Orders / Activity` sub-tabs. Place market, limit, stop, stop-limit,
  trailing-stop and bracket orders; every order and cancel routes through a
  confirm-before-fire modal. Positions re-price P&L live between snapshots.
- **Chart** — single-symbol, TradingView-style multi-pane chart. Price +
  overlays, volume, RSI and MACD panes share one X-axis and a linked crosshair
  (pan/zoom/hover any pane and the rest follow). Live OHLC + change header with
  active-indicator chips, latest-close line, and a floating OHLC tooltip.
- **Compare** — multi-asset risk/return: normalized return lines, drawdown,
  correlation heatmap, risk/return scatter, and a Monte Carlo growth projection
  (inline `Xorshift64` + Box-Muller). Series are aligned to a common history so
  the math lines up.

## Across every tab

- **Live data stream** — a dedicated WebSocket thread
  (`wss://stream.data.alpaca.markets/v2/iex`) parses trades / quotes / minute
  bars into a shared tick cache. Charts patch tick-by-tick; the positions table
  re-prices on every frame.
- **Command palette** — press `/` for a Bloomberg-style command bar to jump
  symbols and tabs (pure, fully unit-tested parser).
- **Watchlist + ticker tape** — mounted on every tab, riding the same tick
  cache; the symbol list persists.
- **State persistence** — last symbol, ranges, indicator toggles, compare slots
  and watchlist are debounced-saved to `{config_dir}/alpaca-tui/state.json`.

## Indicators (Chart tab)

| Indicator | Default period | Where it draws | Hotkey |
|-----------|---------------|----------------|--------|
| EMA       | 10            | Overlay on price (cyan) | `E` |
| SMA       | 20            | Overlay on price (yellow) | `S` |
| Bollinger Bands | 20, 2.0σ | Overlay on price (3 gray lines) | `B` |
| VWAP      | (cumulative)  | Overlay on price (yellow) | `U` |
| Volume    | —             | Sub-panel (green/red bars) | `V` |
| RSI       | 14 (Wilder)   | Sub-panel 0–100, 30/70 lines | `I` |
| MACD      | 12/26/9       | Sub-panel histogram + 2 lines | `O` |

EMA and Volume are on by default. Indicator hotkeys are gated to the Chart tab.
Math is pure Rust and unit-tested in `src/indicators.rs`.

## Download

Prebuilt binaries live in [`../binaries/main-trading-terminal-rust/`](../binaries/main-trading-terminal-rust/):

| Platform | File |
|----------|------|
| macOS (Universal) | `Alpaca_Trading_Terminal_Rust.dmg` · `Alpaca_Trading_Terminal_Rust_MAC_UNIVERSAL` |
| Windows (x64) | `Alpaca_Trading_Terminal_Rust_WIN.exe` |
| Linux (x64) | `Alpaca_Trading_Terminal_Rust_LINUX` — built in CI (see below) |

The binaries are unsigned, so the first launch needs a one-time Gatekeeper
(macOS) / SmartScreen (Windows) bypass. There's also a download page in
[`../website/`](../website/).

## Run

```bash
cargo run --release            # build + run from source
cargo run --release -- --reset # re-enter credentials
```

On first launch the app prompts for your Alpaca API key & secret and stores them
at `~/Library/Application Support/alpaca-tui/credentials.json` (or the
OS-equivalent) — the **same file shared by every build** in this repo, so
binaries swap without re-entering keys. It defaults to the paper-trading API.

## Build from source

```bash
cd main-trading-terminal-rust
cargo build --release          # output: target/release/alpaca-egui
./target/release/alpaca-egui
```

**Linux build deps** (egui needs windowing + GL, tungstenite uses native-tls):

```bash
sudo apt-get install -y \
  libxcb-render0-dev libxcb-shape0-dev libxcb-xfixes0-dev \
  libxkbcommon-dev libssl-dev libgl1-mesa-dev
```

The Linux release binary is built by the
[`deploy-site` GitHub Actions workflow](../.github/workflows/deploy-site.yml) on
a real Ubuntu runner and bundled straight into the published site — a GUI app
can't be cross-compiled cleanly from macOS/Windows. (CI no longer commits it
back, so pushing never forces a pull; an initial copy is kept here for local
preview.)

## Tests

```bash
cargo test                     # indicator + compare + command-palette math
```

## Project layout

```
main-trading-terminal-rust/
├── Cargo.toml
├── src/
│   ├── main.rs       # eframe entry + first-run credential setup
│   ├── app.rs        # ChartApp — tab routing, update loop, command dispatch
│   ├── terminal.rs   # Trading Terminal tab — positions/trade/orders/activity
│   ├── chart.rs      # multi-pane plots, linked axis + cursor, live patching
│   ├── compare.rs    # multi-asset compare + Monte Carlo (Xorshift64)
│   ├── indicators.rs # SMA / EMA / BB / RSI / MACD / VWAP / ATR + tests
│   ├── strategies.rs # signal strategies (MA-cross / BB / MACD)
│   ├── command.rs    # pure command-palette parser
│   ├── workers.rs    # background HTTP threads → mpsc Msg channel
│   ├── stream.rs     # live WebSocket thread → shared tick cache
│   ├── api.rs        # Alpaca REST client (account/orders/positions/bars/…)
│   ├── stocks.rs     # asset cache + symbol autocomplete
│   ├── watchlist.rs  # watchlist side panel + ticker tape
│   ├── persist.rs    # debounced AppState save/load
│   ├── config.rs     # credentials read/write
│   └── theme.rs      # shared Bloomberg palette
└── README.md
```

## History

This started as a chart-only egui experiment and grew into the full three-tab
terminal above (Trading Terminal + Chart + Compare). The folder was also renamed
from `chart-compare-gui-rust/` to `main-trading-terminal-rust/`; older docs may
still reference the previous name.
