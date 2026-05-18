# Chart + Compare GUI (Rust + egui)

Two-tab desktop GUI: **Chart** (single-symbol multi-pane TradingView-style chart matching the main build's indicators) and **Compare** (multi-asset risk/return view with normalized lines, drawdown, scatter, Monte Carlo).

## Stack
- Rust 2021, `eframe` 0.29 + `egui` 0.29 + `egui_plot` 0.29
- `ureq` for HTTP, `chrono`, `serde` / `serde_json`, `csv`, `dirs`, `anyhow`
- 11 source files in [src/](src/), 21 tests

## Commands (run from this directory)
- Build: `cargo build --release` (output: `target/release/alpaca-egui`)
- Run: `cargo run --release`
- Reset stored credentials: `cargo run --release -- --reset`
- Test: `cargo test` (21 tests — indicators + compare math)

## Architecture rules

- **Tab routing.** `Tab` enum in [src/app.rs](src/app.rs) selects Chart vs Compare. **Chart-tab indicator hotkeys (`V B S E U I O`) are gated on `current_tab == Tab::Chart`** so they don't fire silently while on Compare.
- **Background HTTP.** `std::thread` + `mpsc::channel` in [src/workers.rs](src/workers.rs). `Msg` variants are `Assets`, `Bars` (Chart tab), and `CompareBars` (Compare tab — separate variant because the receivers update different state). Workers call `ctx.request_repaint()` after sending.
- **Stale-response protection differs by tab.**
  - Chart tab: single `gen: u64` on `ChartApp` (one symbol at a time, same pattern as the main build's `chartLoadGen`).
  - Compare tab: **per-slot gens.** Each `Slot` carries its own `gen`; `CompareState::seq` is a monotonic counter that issues a fresh gen for every load. Load-bearing — adding a second asset while the first is still loading must NOT invalidate the first's pending response, so a single global gen wouldn't work.
- **Compare lock toggle.** `egui_plot` captures mouse-wheel/drag inside the plot area by default, which fights the outer `ScrollArea`. `CompareState::interactive` (default `false`) gates this:
  - Locked: every Compare plot gets `allow_drag/scroll/zoom = Vec2b::FALSE` and `ScrollArea::enable_scrolling(true)`, so the page scrolls past charts.
  - Unlocked: the inverse.
  - The `apply_lock(plot, interactive)` helper in [src/compare.rs](src/compare.rs) is the single place to flip this — **every new chart in Compare must route through it.**
- **Series alignment.** Symbols may have different bar histories (IPO date, listing changes). `aligned_closes()` right-truncates all series to the shortest length so metrics and overlays line up. Don't add length-mismatched arithmetic — it'll silently mis-align.
- **Monte Carlo RNG is inline** (`Xorshift64` + Box-Muller in [src/compare.rs](src/compare.rs)) on purpose — adding the `rand` crate for one tab was rejected. The seed advances by an LCG step on each Re-run so successive runs give different draws but a single run is reproducible. Sim count is a typeable `DragValue` clamped to 100–10000; higher × 10y horizon ≈ 200MB working set.
- **Theme palette.** [src/theme.rs](src/theme.rs) is shared across both tabs. Compare uses `SLOT_COLORS` (cyan/orange/yellow/green) keyed by slot index — same color reused for the chip, stats-row dot, normalized line, drawdown line, and scatter point so the eye can track each asset.

## Don't

- Don't add a chart to the Compare tab that bypasses `apply_lock` — it'll fight the outer `ScrollArea`.
- Don't pull in the `rand` crate just for Monte Carlo; extend `Xorshift64` instead.
- Don't do arithmetic on close-arrays of different lengths without going through `aligned_closes()`.
- Don't ungate the indicator hotkeys from the Chart tab — they'll silently fire on Compare.
