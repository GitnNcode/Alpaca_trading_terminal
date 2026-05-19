# Chart + Compare GUI (Rust + egui)

Three-tab desktop GUI:
- **Chart** — single-symbol multi-pane TradingView-style chart matching the main build's indicators
- **Compare** — multi-asset risk/return view (normalized lines, drawdown, scatter, Monte Carlo)
- **Terminal** — port of the canonical Go terminal's account-management surface; renders a *secondary* sub-tab strip beneath the top tab strip for `Positions / Trade / Orders / Activity`

## Stack
- Rust 2021, `eframe` 0.29 + `egui` 0.29 + `egui_plot` 0.29
- `ureq` for HTTP, `chrono`, `serde` / `serde_json`, `csv`, `dirs`, `anyhow`
- 12 source files in [src/](src/), 21 tests

## Commands (run from this directory)
- Build: `cargo build --release` (output: `target/release/alpaca-egui`)
- Run: `cargo run --release`
- Reset stored credentials: `cargo run --release -- --reset`
- Test: `cargo test` (21 tests — indicators + compare math)

## Architecture rules

- **Tab routing.** `Tab` enum in [src/app.rs](src/app.rs) selects Chart / Compare / Terminal. **Chart-tab indicator hotkeys (`V B S E U I O`) are gated on `current_tab == Tab::Chart`** so they don't fire silently while on Compare or Terminal. The Terminal tab renders its own sub-tab strip (Positions / Trade / Orders / Activity) right under the top strip — sub-tab number hotkeys `1..4` also gate on focus to avoid stealing keys from the Trade form's text fields.
- **Background HTTP.** `std::thread` + `mpsc::channel` in [src/workers.rs](src/workers.rs). `Msg` variants: `Assets`, `Bars` (Chart), `CompareBars` (Compare per-slot), plus the Terminal-tab set: `Positions`, `AccountInfo`, `OpenOrders`, `ClosedOrders`, `Activities`, `OrderPlaced { req_summary, result }`, `OrderCancelled { id, result }`. Every worker calls `ctx.request_repaint()` after sending.
- **Terminal tab is lazy + auto-refresh.** Nothing is fetched until the user first opens the Terminal tab (`terminal_primed: bool` flips on first visit). While the tab is active, `ChartApp::update` checks `terminal.last_refresh.elapsed() >= 10s` every frame and re-runs `TerminalState::refresh_all` — matches the Go terminal's 10s background goroutine. A `ctx.request_repaint_after(1s)` keeps the timer honest when the user isn't moving the mouse. Don't run the refresh while on Chart/Compare — burning API calls on a tab nobody is looking at.
- **Order placement is two-phase.** Clicking PLACE ORDER in the Trade sub-tab does *not* hit Alpaca directly; it stashes the validated `OrderRequest` in `terminal.place_confirm` and pops an `egui::Window` modal. Only the modal's CONFIRM button calls `workers::spawn_place_order`. Same shape for cancels via `cancel_confirm`. **Don't bypass this** — the Go terminal's tests lock in the confirm-before-fire UX.
- **Cancelling-state lifecycle.** When a CANCEL row button is clicked, the order id is added to `terminal.cancelling: HashSet<String>` for an inline "cancelling…" label. The set is pruned in two places: when `Msg::OrderCancelled` arrives (immediate remove) AND when `Msg::OpenOrders` returns and the id is no longer in the live list (defensive — covers the rare case where the cancel message is lost but the order really did clear).
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
- Don't ungate the indicator hotkeys from the Chart tab — they'll silently fire on Compare / Terminal.
- Don't make Terminal-tab fetches at app startup. Fetching has to be gated on `current_tab == Tab::Terminal` so first-launch latency stays on Chart.
- Don't call `place_order` / `cancel_order` from a button click — route through the confirm modal (`place_confirm` / `cancel_confirm`).
- Don't restore the Go terminal's Chart sub-tab here. The egui app already has a richer Chart top-level tab; duplicating it inside Terminal would just confuse the tab strip.
