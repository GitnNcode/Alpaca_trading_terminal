# Alpaca Chart — egui charting tool

A native desktop **chart-only** app for Alpaca Markets data. No order entry,
no portfolio views — just charting, like a stripped-down personal
TradingView. Built on [eframe / egui](https://github.com/emilk/egui) +
[egui_plot](https://docs.rs/egui_plot).

## Run

```bash
./alpaca-egui-mac-arm           # Apple Silicon
./alpaca-egui-mac-arm --reset   # re-enter credentials
```

Credentials live at `~/Library/Application Support/alpaca-tui/credentials.json`
— same file as the other builds.

## Layout

```
┌─ Toolbar ───────────────────────────────────────────────┐
│ SYMBOL  [AAPL]  Apple Inc.       Load                   │
│ RANGE   1D 1W 1M YTD 1Y 5Y MAX                          │
│ CANDLE  1m 5m 15m 30m 1h 1D 1W 1M                       │
│ IND     EMA(10) SMA(20) BB(20) VWAP VOL RSI(14) MACD    │
├─────────────────────────────────────────────────────────┤
│  AAPL  $123.45  +$1.20 (+0.98%)   H L  Vol  Bars …      │
│  active indicator chips ...                             │
│                                                         │
│      ┌─── Price + overlays ───┐                         │
│      │   candlesticks         │                         │
│      │   EMA / SMA / BB / VWAP│                         │
│      │   horizontal close line│                         │
│      └────────────────────────┘                         │
│  ── separator ──                                        │
│      ┌─── Volume ──────┐                                │
│      │ green/red bars  │                                │
│      └─────────────────┘                                │
│  ── separator ──                                        │
│      ┌─── RSI(14) ─────┐                                │
│      │ 30 / 70 lines   │                                │
│      └─────────────────┘                                │
│  ── separator ──                                        │
│      ┌─── MACD ────────┐                                │
│      │ histogram + lines│                               │
│      └─────────────────┘                                │
└─────────────────────────────────────────────────────────┘
```

All sub-panels share an X-axis via `Plot::link_axis` and a shared crosshair
via `Plot::link_cursor` — pan, zoom, or hover on **any** panel and the
others move together, exactly like TradingView's stacked panes.

## Indicators

| Indicator | Default period | Where it draws | Hotkey |
|-----------|---------------|----------------|--------|
| EMA       | 10            | Overlay on price (cyan) | `E` |
| SMA       | 20            | Overlay on price (yellow) | `S` |
| Bollinger Bands | 20, 2.0σ | Overlay on price (3 gray lines) | `B` |
| VWAP      | (cumulative)  | Overlay on price (yellow) | `U` |
| Volume    | —             | Sub-panel (green/red bars) | `V` |
| RSI       | 14 (Wilder)   | Sub-panel 0–100, 30/70 lines | `I` |
| MACD      | 12/26/9       | Sub-panel histogram + 2 lines | `O` |

EMA and Volume are on by default. Click the pills in the toolbar or press
the letter while the chart has focus to toggle.

## TradingView-style behavior

- **Click + drag** any plot pans it (and the others) along the X axis
- **Mouse wheel** zooms the X axis on the plot under the cursor; other
  panes follow because of the shared axis link
- **Hover** anywhere → the cursor line appears across all panels at the
  same X coordinate (linked cursor)
- **OHLC + time + volume** tooltip floats near the cursor (via egui_plot's
  `label_formatter`)
- **Latest close** is shown as a horizontal colored line spanning the
  price panel
- **Active indicators** show as colored chips in the header strip
- **`Ctrl+drag` / `Shift+drag`** for box-zoom (egui_plot built-in)

## Indicator math

Pure Rust, fully unit-tested in `src/indicators.rs`:

- `compute_sma`, `compute_ema` — seed from SMA of first `period` closes
- `compute_bollinger` — middle = SMA, bands = ± mult × stdev
- `compute_rsi` — Wilder's smoothing
- `compute_macd` — EMA(fast) − EMA(slow), signal = EMA(macd, signal)
- `compute_vwap` — cumulative typical-price × volume
- `compute_atr` — Wilder's true range smoothing (unused but kept for
  future Keltner / Supertrend)

## Build from source

```bash
cd chart-compare-gui-rust
cargo build --release
./target/release/alpaca-egui
```

## Project layout

```
chart-compare-gui-rust/
├── Cargo.toml
├── alpaca-egui-mac-arm  # 4.1 MB release binary
├── src/
│   ├── main.rs       # eframe entry + first-run setup
│   ├── app.rs        # ChartApp — single-screen state, toolbar
│   ├── chart.rs      # 🌟 multi-pane plots with linked axis + cursor
│   ├── indicators.rs # math (SMA, EMA, BB, RSI, MACD, VWAP, ATR) + tests
│   ├── workers.rs    # background HTTP threads → mpsc messages
│   ├── theme.rs      # Bloomberg palette
│   ├── api.rs        # Alpaca REST client (only get_assets + get_bars used)
│   ├── config.rs     # credentials read/write
│   └── stocks.rs     # asset cache + autocomplete
└── README.md
```

## What changed from the previous egui port

This used to be a 5-tab "do-everything" port. It's now a **chart-only**
tool because the user keeps the Go tview build for trading and wanted the
egui side to focus on charting.

Removed:
- Positions / Trade / Orders / Activity tabs
- Place-order / cancel-order modals
- Auto-refresh loop (not needed without live portfolio data)
- Trade log

Added:
- SMA / Bollinger / VWAP overlays
- Volume / RSI / MACD sub-panels
- Linked X-axis + crosshair across panels
- Header strip with live OHLC + change + active-indicator chips
- Indicator hotkeys (`E S B U V I O`)
- Floating OHLC + time tooltip on hover
