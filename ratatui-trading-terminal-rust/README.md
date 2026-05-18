# Alpaca Trading Terminal — Rust port

A Rust reimplementation of the Go terminal trading app, built on
[ratatui](https://github.com/ratatui/ratatui) and
[crossterm](https://github.com/crossterm-rs/crossterm).

Feature parity with the Go version: Positions, Trade, Orders, Activity, and
Chart tabs; auto-refresh every 10 s; first-run credential setup; ticker +
company-name autocomplete; candlestick chart with range/timeframe selectors and
horizontal scrolling; `trades.csv` logging on every placed order.

## Run

```bash
# Build
cargo build --release

# Run (paper trading default)
./target/release/alpaca-rs

# Reset stored credentials
./target/release/alpaca-rs --reset
```

On first launch, enter your API key, secret, and pick Paper/Live. Credentials
are saved to:

| OS | Path |
|----|------|
| Windows | `%APPDATA%\alpaca-tui\credentials.json` |
| macOS | `~/Library/Application Support/alpaca-tui/credentials.json` |
| Linux | `~/.config/alpaca-tui/credentials.json` |

## Keyboard

| Key | Action |
|-----|--------|
| `1` `2` `3` `4` `5` | Switch tabs |
| `←` `→` | Switch tabs (when not in a text field) |
| `↑` `↓` | Navigate fields / table rows / autocomplete |
| `Tab` / `Shift-Tab` | Navigate trade form fields |
| `Enter` | Cycle dropdown / select autocomplete / activate button |
| `R` / `F5` | Manual refresh |
| `X` / `Del` | Cancel selected order (Orders tab) |
| `Q` / `Esc` | Quit |

### Chart tab
| Key | Action |
|-----|--------|
| `D` `W` `M` `T` `Y` `F` `X` | Range: 1D, 1W, 1M, YTD, 1Y, 5Y, MAX |
| `[` `]` | Cycle range |
| `{` `}` (or `-` `=`) | Cycle CANDLE timeframe (1m → 1M) |
| `←` `→` or `,` `.` | Scroll bars |
| `<` `>` | Scroll one page |
| `Home` `End` | Oldest / newest |
| `Tab` / `Enter` | Toggle focus between symbol input and canvas |
| `Esc` | (on canvas) return to symbol input |

### Mouse
| Action | Effect |
|--------|--------|
| Click tab label | Switch tab |
| Click CANDLE label | Select timeframe |
| Click RANGE label | Select range |
| Click chart symbol input | Focus symbol input |
| Click chart canvas | Focus canvas |
| Scroll wheel on chart | Scroll bars by visible step |
| Single-click order row | Select row |
| Right-click order row | Open cancel modal |
| Double-click position row | Pre-fill SELL on Trade tab |
| Click trade form field | Focus that field (or cycle dropdown) |
| Click PLACE ORDER / CLEAR | Activate the button |
| Click modal button | Confirm / dismiss |

## Layout

```
src/
├── main.rs        # terminal setup + event loop
├── api.rs         # Alpaca REST client (ureq)
├── config.rs      # credential load/save
├── setup.rs       # first-run setup screen
├── stocks.rs      # asset cache + autocomplete
├── app.rs         # AppState, activity row builders, formatting
├── workers.rs     # background HTTP threads
├── input.rs       # keyboard / message dispatch
├── ui.rs          # main rendering for header, tabs, status bar, modals
├── chart.rs       # candlestick chart widget
├── trade_log.rs   # trades.csv append
└── theme.rs       # Bloomberg color palette
```
