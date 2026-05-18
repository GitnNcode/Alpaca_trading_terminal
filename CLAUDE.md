# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Repository layout

Each implementation lives in its own folder. The Go tview build is the canonical one; the others are architectural ports / spin-offs.

| Path | Stack | Status |
|------|-------|--------|
| `main-trading-terminal-go/` | Go 1.22 + tview + tcell | **Canonical, feature-complete, 38 tests** |
| `ratatui-trading-terminal-rust/` | Rust + ratatui + crossterm + ureq | Working port, feature-matched to early state |
| `chart-compare-gui-rust/` | Rust + eframe/egui + egui_plot | Chart + Compare GUI (multi-asset risk/return, Monte Carlo) |
| `backtest-terminal-go/` | Go + tview (separate module) | Standalone strategy backtester |

When the user says "the app" without qualification, they almost always mean `main-trading-terminal-go`. The ports exist for comparison and exploration; **bug reports / new features generally land in the main build first**.

All builds share a credentials file: `~/Library/Application Support/alpaca-tui/credentials.json` on macOS (`%APPDATA%\alpaca-tui\credentials.json` on Windows, `~/.config/alpaca-tui/credentials.json` on Linux). So the user can swap binaries without re-entering keys. They also share the `trades.csv` format.

## Commands

### Main Go build (`main-trading-terminal-go/`)
```bash
cd main-trading-terminal-go

# Run locally
go run .

# Build current platform
go build -o alpaca-tcell .

# Run all tests (38 tests, ~12s)
go test ./...

# Single test (verbose, with logs)
go test -run TestChartTimeframeHotkeys -v ./...

# Race detector
go test -race ./...

# Vet
go vet ./...

# Reset stored credentials (forces first-run setup)
./alpaca-tcell --reset

# Cross-compile all four release binaries (see bin/)
CGO_ENABLED=0 GOOS=windows GOARCH=amd64 go build -ldflags="-s -w" -trimpath -o bin/Alpaca_Trading_Terminal_WIN.exe .
CGO_ENABLED=0 GOOS=darwin  GOARCH=arm64 go build -ldflags="-s -w" -trimpath -o bin/Alpaca_Trading_Terminal_MAC_ARM .
CGO_ENABLED=0 GOOS=darwin  GOARCH=amd64 go build -ldflags="-s -w" -trimpath -o bin/Alpaca_Trading_Terminal_MAC_INTEL .
CGO_ENABLED=0 GOOS=linux   GOARCH=amd64 go build -ldflags="-s -w" -trimpath -o bin/Alpaca_Trading_Terminal_LINUX .
```

### Ratatui Rust port (`ratatui-trading-terminal-rust/`)
```bash
cd ratatui-trading-terminal-rust
cargo build --release        # builds target/release/alpaca-rs
cargo test                   # all tests
cargo test -- cycle_tf       # single test
cargo run --release -- --reset
```

### Chart + Compare GUI (`chart-compare-gui-rust/`)
```bash
cd chart-compare-gui-rust
cargo build --release        # builds target/release/alpaca-egui
cargo test                   # 21 tests (indicators + compare math)
cargo run --release -- --reset
```

### Backtest terminal (`backtest-terminal-go/`)
```bash
cd backtest-terminal-go
go build .
go test ./...
```

## High-level architecture

### Main Go build (the canonical one — in `main-trading-terminal-go/`)

Five files, no internal packages — everything is `package main`:

- **`main.go`** — `termApp` struct holds every tview primitive (tables, form, dropdowns, pages). `globalKeys` is the application-level input capture; per-widget capture lives on each primitive. Auto-refresh is a goroutine that fires every 10s. Tab switching = `pages.SwitchToPage()` + setting focus. Confirmation modals are `tview.Modal` pages added/removed dynamically. Status-bar hint per-tab is rendered with tview's `[X[]` escape syntax (which renders as `[X]`).
- **`chart.go`** — The most complex file. Custom widget `chartCanvas` embeds `tview.Box` and overrides `Draw(screen)`. Renders candles (wick + body) using direct `screen.SetContent` calls. The candle-sizing model is **zoom presets + bar aggregation**: `chartZooms` slice maps a label (XXS → XL) to `{slotW, bodyW, barsPerSlot}`. At `barsPerSlot > 1` the renderer aggregates N raw bars into one OHLC candle via `aggregateBars`. Y-axis is **rigid** — fit once on chart load, then `↑`/`↓` pan; `0` resets. EMA overlay uses Braille (see below). `chartLoadGen atomic.Int64` is a generation counter so concurrent `loadChart` goroutines drop stale HTTP responses on completion (without it, a slow 1m response would clobber a fast 1Day result).
- **`braille.go`** — `brailleLayer` accumulates dots in a 2×4-sub-pixel grid per terminal cell and renders one `U+2800 + mask` Braille rune per occupied cell. `thickLine` draws a 2-sub-pixel-wide Bresenham line — the canonical primitive for indicators. **All future indicators (MACD, RSI, etc.) should use this**; one layer per indicator with its own color.
- **`api.go`** — Plain `net/http` Alpaca REST client. Methods are synchronous; the UI wraps them in goroutines + `tapp.QueueUpdateDraw` to apply results. `alpacaDataBase` is a `var` (not const) so tests can swap it for an `httptest.Server`.
- **`config.go`** — Credentials + first-run setup screen (runs its own short-lived `tview.Application`).
- **`stocks.go`** — Asset cache + autocomplete. Binary-search ticker prefix + linear company-name substring scan.

### Important behavioral rules in the tview build

These come from bugs the user reported and tests that lock them in:

- **`Q` / `R` global shortcuts only fire from focuses where letters are meaningless** (Tables, chart canvas). On `*tview.InputField` letters type into the field; on `*tview.DropDown` and `*tview.Button` only the explicit tab-nav shortcuts (`1`-`5`, `<`, `>`, F5) pass through. See the type-switch in `globalKeys`.
- **Tab navigation uses `<` and `>`**, not arrows. Arrows are reserved for: caret in text fields, row nav in tables, chart pan/scroll.
- **`<` / `>` in DropDown / Button** must pass through to global tab-switching (`globalKeys` type-switch handles this explicitly).
- **`Esc` on the chart canvas** moves focus back to the symbol input — does NOT quit. `Q` / `Ctrl+C` quit.
- **Left/Right on chart canvas** must scroll bars; they're explicitly excluded from `globalKeys`'s tab-switching logic.
- **Chart-load races**: every call to `loadChart` must bump `chartLoadGen` and check it before writing results. Two atomic checks: one before queueing the UI update, one inside the queued closure.

### Test infrastructure (chart_test.go)

- Uses `tcell.NewSimulationScreen` to drive the real tview event loop in tests. `startSimApp(t)` boots `newTermApp()` on a sim screen and returns the app + a cleanup func. `withChartTab(t, a)` switches to the chart tab and focuses the canvas.
- **`queueRead[T]`** runs a function on the event-loop goroutine and returns the result with a 2-second timeout. The QueueUpdate is wrapped in a goroutine so a dead app can't hang the test suite (this was a real bug — tests used to hang for 10 minutes when the app quit unexpectedly).
- HTTP-dependent tests use `httptest.NewServer` and swap `alpacaDataBase` to point at it (see `TestLoadChartLatestWins`).
- The `drawCanvasOnce` helper draws a canvas to a `SimulationScreen` without the full app — used for low-level layout assertions.
- `SimulationScreen.GetContent` only sees writes after `screen.Show()` — easy to forget when reading screen state in a test.

### Layout

The chart tab has FOUR rows above the canvas: symbol input → CANDLE/RANGE row → EMA/ZOOM row → canvas → stats. Hit-test ranges for the clickable label bars are stored on `termApp` (`chartRangeHitRanges`, `chartTFHitRanges`, etc.) and updated by the corresponding `updateChartXBar` function. **Always update both the rendered string AND the hit-range slice** when changing one of those bars.

### Mouse handling

tview routes mouse events through `SetMouseCapture` on each primitive. To consume a click you must return `tview.MouseConsumed` (not just `nil` — that lets the event bubble up to siblings and can trigger unintended tab-bar clicks). See the comment in `TestChartTFClickDoesNotSwitchTabs` — this was a real bug.

### Architectural ports

The ratatui Rust port (`ratatui-trading-terminal-rust/`) and the egui GUI (`chart-compare-gui-rust/`) are both **immediate-mode** renderers, unlike tview's retained-mode widgets. They rebuild the full UI on every frame from an immutable state struct. The ratatui port has the most TUI feature parity; the egui GUI is chart-focused (see next section).

Don't reflexively port a main-build change to the ports — they have their own test suites and patterns, and the user typically wants the ports to remain as architectural references rather than living code that has to track every feature.

### egui GUI build (`chart-compare-gui-rust/`)

Two tabs: **Chart** (single-symbol multi-pane TradingView-style chart matching the main build's indicators) and **Compare** (multi-asset risk/return view). The whole app is `eframe`, 11 source files in `src/`, 21 tests.

- **Tabs are routed by `Tab` enum in `app.rs`.** Chart-tab indicator hotkeys (`V B S E U I O`) are gated on `current_tab == Tab::Chart` so they don't fire silently when on Compare.
- **Background HTTP** uses `std::thread` + `mpsc::channel`. `Msg` variants are `Assets`, `Bars` (chart tab), `CompareBars` (compare tab — separate variant because the receivers update different state). Workers call `ctx.request_repaint()` after sending.
- **Stale-response protection differs from the main build.** The Chart tab uses a single `gen: u64` on `ChartApp` (one symbol at a time, same pattern as main's `chartLoadGen`). The Compare tab uses **per-slot gens**: each `Slot` carries its own `gen`, and `CompareState::seq` is a monotonic counter that issues a fresh gen for every load. This is load-bearing — adding a second asset while the first is still loading must NOT invalidate the first's pending response, so a single global gen wouldn't work.
- **Compare lock toggle.** `egui_plot`'s default behaviour is to capture mouse-wheel/drag inside the plot area, which fights the outer `ScrollArea`. `CompareState::interactive` (default `false`) gates this: when locked, every Compare plot gets `allow_drag/scroll/zoom = Vec2b::FALSE` and `ScrollArea::enable_scrolling(true)` so the page scrolls past charts. When unlocked, the inverse. The `apply_lock(plot, interactive)` helper in `compare.rs` is the single place to flip this — every new chart in Compare must route through it.
- **Series alignment.** Different symbols may have different bar histories (IPO date, listing changes). `aligned_closes()` right-truncates all series to the shortest length so metrics and overlays line up. Don't add length-mismatched arithmetic — it'll silently mis-align.
- **Monte Carlo RNG is inline** (`Xorshift64` + Box-Muller in `compare.rs`) on purpose — adding the `rand` crate for one tab was rejected. The seed advances by an LCG step on each Re-run so successive runs give different draws but a single run is reproducible. Sim count is a typeable `DragValue` clamped to 100–10000; higher × 10y horizon = ~200MB working set.
- **Theme palette** (`theme.rs`) is shared across both tabs. Compare uses `SLOT_COLORS` (cyan/orange/yellow/green) keyed by slot index — same color reused for the chip, stats row dot, normalized line, drawdown line, and scatter point so the eye can track each asset.

### Backtest terminal (`backtest-terminal-go/`)

Standalone Go module (own `go.mod`, module name `backtest-tui`) — does **not** share code with the main trading terminal. tview-based TUI for running backtests with multiple regime detection / strategy combinations (Bollinger, ADX, HMM, Markov chain, regime-switch). Tests live alongside source (`*_test.go`). Touch this folder only when the user explicitly asks about backtesting; changes here don't need to be mirrored into the trading-terminal builds.

## Repo gotchas

- **The Rust ports' `target/` folders are tracked in git.** They're not in `.gitignore`. This means:
  - `git mv old_dir new_dir` for a Rust port folder will fail with "bad source" if stale incremental `.o` files exist in the index but are missing from the working tree. Workaround: `mv old_dir new_dir && git add -A old_dir new_dir`.
  - Cargo rebuilds will create huge churn in `git status`. Don't reflexively commit those changes — they bloat history. Only commit source-level changes under `src/`, `Cargo.toml`, `Cargo.lock`.
- **`go.mod` lives inside each Go folder**, not at the repo root. There's no Go workspace — each Go module is built/tested from its own directory.
- **Release binaries are tracked** in `main-trading-terminal-go/bin/` (Windows / macOS ARM / macOS Intel / Linux). End-users grab these directly from the repo, so don't delete them as "build artifacts."
