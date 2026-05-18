# Main trading terminal (Go + tview)

**The canonical implementation.** Bug reports and new features land here first.

## Stack
- Go 1.22, `package main`, no internal packages
- tview + tcell for the TUI
- Plain `net/http` for the Alpaca REST client (no SDK)

## Commands (run from this directory)
- Run: `go run .`
- Build: `go build -o alpaca-tcell .`
- Test: `go test ./...` (38 tests, ~12s)
- Single test: `go test -run TestChartTimeframeHotkeys -v ./...`
- Race detector: `go test -race ./...`
- Vet: `go vet ./...`
- Reset stored credentials: `./alpaca-tcell --reset`

Cross-compile all four release binaries into `bin/`:
```bash
CGO_ENABLED=0 GOOS=windows GOARCH=amd64 go build -ldflags="-s -w" -trimpath -o bin/Alpaca_Trading_Terminal_WIN.exe .
CGO_ENABLED=0 GOOS=darwin  GOARCH=arm64 go build -ldflags="-s -w" -trimpath -o bin/Alpaca_Trading_Terminal_MAC_ARM .
CGO_ENABLED=0 GOOS=darwin  GOARCH=amd64 go build -ldflags="-s -w" -trimpath -o bin/Alpaca_Trading_Terminal_MAC_INTEL .
CGO_ENABLED=0 GOOS=linux   GOARCH=amd64 go build -ldflags="-s -w" -trimpath -o bin/Alpaca_Trading_Terminal_LINUX .
```

## Files

Six source files, everything `package main`:

- **[main.go](main.go)** — `termApp` struct holds every tview primitive (tables, form, dropdowns, pages). `globalKeys` is the application-level input capture; per-widget capture lives on each primitive. Auto-refresh goroutine fires every 10s. Tab switching = `pages.SwitchToPage()` + setting focus. Confirmation modals are `tview.Modal` pages added/removed dynamically. Status-bar hint per-tab uses tview's `[X[]` escape syntax (renders as `[X]`).
- **[chart.go](chart.go)** — Most complex file. Custom widget `chartCanvas` embeds `tview.Box` and overrides `Draw(screen)`. Renders candles via direct `screen.SetContent` calls. Candle-sizing model is **zoom presets + bar aggregation**: `chartZooms` maps a label (XXS → XL) to `{slotW, bodyW, barsPerSlot}`. At `barsPerSlot > 1`, `aggregateBars` aggregates N raw bars into one OHLC candle. Y-axis is **rigid** — fit once on chart load, `↑`/`↓` pan, `0` resets. EMA overlay uses Braille (see [braille.go](braille.go)). `chartLoadGen atomic.Int64` drops stale HTTP responses so a slow 1m response can't clobber a fast 1Day result.
- **[braille.go](braille.go)** — `brailleLayer` accumulates dots in a 2×4-sub-pixel grid per terminal cell and renders one `U+2800 + mask` Braille rune per occupied cell. `thickLine` draws a 2-sub-pixel-wide Bresenham line — the canonical primitive for indicators. **All future indicators (MACD, RSI, etc.) should use this**; one layer per indicator with its own color.
- **[api.go](api.go)** — Plain `net/http` Alpaca REST client. Methods are synchronous; the UI wraps them in goroutines + `tapp.QueueUpdateDraw` to apply results. `alpacaDataBase` is a `var` (not const) so tests can swap it for an `httptest.Server`.
- **[config.go](config.go)** — Credentials + first-run setup screen (runs its own short-lived `tview.Application`).
- **[stocks.go](stocks.go)** — Asset cache + autocomplete. Binary-search ticker prefix + linear company-name substring scan.

## Behavioral rules (locked in by tests)

- **`Q` / `R` global shortcuts only fire from focuses where letters are meaningless** (Tables, chart canvas). On `*tview.InputField` letters type into the field; on `*tview.DropDown` and `*tview.Button` only the explicit tab-nav shortcuts (`1`-`5`, `<`, `>`, F5) pass through. See the type-switch in `globalKeys`.
- **Tab navigation uses `<` and `>`**, not arrows. Arrows are reserved for: caret in text fields, row nav in tables, chart pan/scroll.
- **`<` / `>` in DropDown / Button** must pass through to global tab-switching (`globalKeys` type-switch handles this explicitly).
- **`Esc` on the chart canvas** moves focus back to the symbol input — does NOT quit. `Q` / `Ctrl+C` quit.
- **Left/Right on chart canvas** must scroll bars; they're explicitly excluded from `globalKeys`'s tab-switching logic.
- **Chart-load races**: every call to `loadChart` must bump `chartLoadGen` and check it before writing results. Two atomic checks: one before queueing the UI update, one inside the queued closure.

## Chart tab layout

Four rows above the canvas: symbol input → CANDLE/RANGE row → EMA/ZOOM row → canvas → stats. Hit-test ranges for the clickable label bars are stored on `termApp` (`chartRangeHitRanges`, `chartTFHitRanges`, etc.) and updated by the corresponding `updateChartXBar` function. **Always update both the rendered string AND the hit-range slice** when changing one of those bars.

## Mouse handling

tview routes mouse events through `SetMouseCapture` on each primitive. To consume a click you must return `tview.MouseConsumed` — not `nil` (that lets the event bubble up to siblings and triggers unintended tab-bar clicks). See `TestChartTFClickDoesNotSwitchTabs`.

## Test infrastructure ([chart_test.go](chart_test.go))

- Uses `tcell.NewSimulationScreen` to drive the real tview event loop. `startSimApp(t)` boots `newTermApp()` on a sim screen; `withChartTab(t, a)` switches to the chart tab and focuses the canvas.
- **`queueRead[T]`** runs a fn on the event-loop goroutine and returns the result with a 2-second timeout. The QueueUpdate is wrapped in a goroutine so a dead app can't hang the suite (was a real bug — tests used to hang 10 minutes when the app quit unexpectedly).
- HTTP-dependent tests use `httptest.NewServer` and swap `alpacaDataBase` to point at it (see `TestLoadChartLatestWins`).
- `drawCanvasOnce` draws a canvas to a `SimulationScreen` without the full app — for low-level layout assertions.
- `SimulationScreen.GetContent` only sees writes after `screen.Show()` — easy to forget when reading screen state in a test.

## Don't

- Don't delete the prebuilt binaries in [bin/](bin/) — they ship to end-users.
- Don't return `nil` from a mouse handler when you mean to consume the event (use `tview.MouseConsumed`).
- Don't add a chart indicator using ad-hoc `screen.SetContent` — use `brailleLayer` / `thickLine`.
- Don't call `loadChart` without bumping and checking `chartLoadGen`.
