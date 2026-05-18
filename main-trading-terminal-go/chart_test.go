package main

import (
	"fmt"
	"math"
	"net/http"
	"net/http/httptest"
	"testing"
	"time"

	"github.com/gdamore/tcell/v2"
)

// TestChartAutocompleteNoDeadlock drives the real chart-tab UI through tview's
// event loop using a SimulationScreen, then types a ticker prefix and presses
// Enter on the autocomplete suggestion. Under the old code this deadlocked
// because the SetAutocompletedFunc callback (which runs on the event-loop
// goroutine) called Application.QueueUpdateDraw — which blocks waiting for
// the same goroutine to drain the updates channel.
//
// The test fails (times out) if the callback ever deadlocks.
func TestChartAutocompleteNoDeadlock(t *testing.T) {
	// Stub the asset cache so the autocomplete func has something to return.
	assetMu.Lock()
	assetSymbols = []string{"AAPL"}
	assetNames = map[string]string{"AAPL": "Apple Inc."}
	assetMu.Unlock()

	// Stub the API client. loadChart runs on its own goroutine — its HTTP call
	// will fail against this fake URL, but that's irrelevant to the deadlock
	// check, which only cares about the callback returning.
	client = NewAlpacaClient(Credentials{
		APIKey:    "test",
		APISecret: "test",
		BaseURL:   "http://127.0.0.1:1", // unreachable; HTTP call errors on its goroutine
	})

	a := newTermApp()

	screen := tcell.NewSimulationScreen("UTF-8")
	if err := screen.Init(); err != nil {
		t.Fatalf("init simulation screen: %v", err)
	}
	screen.SetSize(180, 50)
	a.tapp.SetScreen(screen)

	runDone := make(chan error, 1)
	go func() { runDone <- a.tapp.Run() }()
	defer func() {
		a.tapp.Stop()
		select {
		case <-runDone:
		case <-time.After(2 * time.Second):
			// app didn't shut down — likely still deadlocked
		}
	}()

	// Wait briefly for the app's event loop to start.
	time.Sleep(150 * time.Millisecond)

	// Switch to the chart tab. QueueUpdate is safe from outside the event loop.
	a.tapp.QueueUpdate(func() { a.switchTab(tabChart) })

	// Type "A" to trigger the autocomplete dropdown with "AAPL".
	a.tapp.QueueEvent(tcell.NewEventKey(tcell.KeyRune, 'A', tcell.ModNone))
	time.Sleep(200 * time.Millisecond)

	// Press Enter on the highlighted suggestion. This is the path that used to
	// deadlock — SetAutocompletedFunc called QueueUpdateDraw from the event loop.
	a.tapp.QueueEvent(tcell.NewEventKey(tcell.KeyEnter, 0, tcell.ModNone))

	// Poll the symbol field's text. Each poll uses QueueUpdate, which blocks
	// waiting for the event loop — so if the loop is deadlocked, polls hang
	// and the outer select-timeout below catches it.
	result := make(chan string, 1)
	go func() {
		deadline := time.Now().Add(3 * time.Second)
		for time.Now().Before(deadline) {
			done := make(chan string, 1)
			a.tapp.QueueUpdate(func() { done <- a.chartSymField.GetText() })
			if text := <-done; text == "AAPL" {
				result <- "ok"
				return
			}
			time.Sleep(50 * time.Millisecond)
		}
		result <- "symbol field never filled"
	}()

	select {
	case r := <-result:
		if r != "ok" {
			t.Fatalf("autocomplete didn't complete: %s", r)
		}
	case <-time.After(5 * time.Second):
		t.Fatal("deadlock: autocomplete callback never returned (event loop stuck)")
	}
}

// TestChartTFClickDoesNotSwitchTabs proves that a left-click on a timeframe
// label (1m/5m/...) selects that timeframe AND stays on the chart tab. Earlier
// the timeframe and range mouse-capture handlers returned (action, nil), but
// tview only treats an event as consumed if the action is MouseConsumed (see
// rivo/tview box.go WrapMouseHandler). So the click was being dispatched to
// the next primitive in the Flex chain, eventually reaching the top tab bar's
// click handler — which then switched tabs because the numeric labels' visible
// column ranges overlap with the timeframe label columns.
func TestChartTFClickDoesNotSwitchTabs(t *testing.T) {
	assetMu.Lock()
	assetSymbols = []string{"AAPL"}
	assetNames = map[string]string{"AAPL": "Apple Inc."}
	assetMu.Unlock()

	client = NewAlpacaClient(Credentials{
		APIKey:    "test",
		APISecret: "test",
		BaseURL:   "http://127.0.0.1:1",
	})

	a := newTermApp()

	screen := tcell.NewSimulationScreen("UTF-8")
	if err := screen.Init(); err != nil {
		t.Fatalf("init simulation screen: %v", err)
	}
	screen.SetSize(180, 50)
	a.tapp.SetScreen(screen)

	runDone := make(chan error, 1)
	go func() { runDone <- a.tapp.Run() }()
	defer func() {
		a.tapp.Stop()
		select {
		case <-runDone:
		case <-time.After(2 * time.Second):
		}
	}()

	time.Sleep(150 * time.Millisecond)

	// Switch to chart tab with QueueUpdateDraw so the layout actually paints
	// (otherwise the chartTFTV's rect is still zero from before its first Draw).
	a.tapp.QueueUpdateDraw(func() { a.switchTab(tabChart) })
	time.Sleep(150 * time.Millisecond)

	type clickInfo struct {
		row       int
		clickX    int
		targetIdx int
	}
	infoCh := make(chan clickInfo, 1)
	a.tapp.QueueUpdate(func() {
		_, ry, _, _ := a.chartTFTV.GetRect()
		bx, _, _, _ := a.chartTFTV.GetInnerRect()
		// Pick the 5m button (index 1) — its visible columns are in chartTFHitRanges[1].
		rng := a.chartTFHitRanges[1]
		clickX := bx + (rng[0]+rng[1])/2
		t.Logf("chartTFTV rect Y=%d innerX=%d clickX=%d hitRange=%v", ry, bx, clickX, rng)
		infoCh <- clickInfo{row: ry, clickX: clickX, targetIdx: 1}
	})
	info := <-infoCh

	// Simulate the click via tcell.EventMouse (down → up generates LeftClick).
	mouseAt := func(x, y int, btn tcell.ButtonMask) {
		ev := tcell.NewEventMouse(x, y, btn, tcell.ModNone)
		a.tapp.QueueEvent(ev)
	}
	mouseAt(info.clickX, info.row, tcell.ButtonPrimary) // down
	mouseAt(info.clickX, info.row, tcell.ButtonNone)    // up → click

	// Allow tview to deliver and process the events.
	time.Sleep(250 * time.Millisecond)

	stateCh := make(chan struct {
		tab int
		tf  int
	}, 1)
	a.tapp.QueueUpdate(func() {
		stateCh <- struct {
			tab int
			tf  int
		}{tab: a.activeTab, tf: a.chartTFIdx}
	})

	select {
	case st := <-stateCh:
		if st.tab != tabChart {
			t.Fatalf("clicking timeframe button switched tabs: activeTab=%d (want %d)", st.tab, tabChart)
		}
		if st.tf != info.targetIdx {
			t.Fatalf("timeframe not selected: chartTFIdx=%d (want %d)", st.tf, info.targetIdx)
		}
	case <-time.After(3 * time.Second):
		t.Fatal("state read timed out")
	}
}

// drawCanvasOnce renders the canvas once against an in-memory screen so we can
// inspect the visibleStart/visibleEnd/visibleStep fields that Draw computes.
func drawCanvasOnce(t *testing.T, c *chartCanvas, w, h int) {
	t.Helper()
	screen := tcell.NewSimulationScreen("UTF-8")
	if err := screen.Init(); err != nil {
		t.Fatalf("init sim screen: %v", err)
	}
	defer screen.Fini()
	screen.SetSize(w, h)
	c.SetRect(0, 0, w, h)
	c.Draw(screen)
}

func makeBars(n int) []Bar {
	bars := make([]Bar, n)
	base := time.Date(2024, 1, 1, 9, 30, 0, 0, time.UTC)
	for i := 0; i < n; i++ {
		bars[i] = Bar{
			Time:  base.Add(time.Duration(i) * time.Minute),
			Open:  100 + float64(i%5),
			High:  102 + float64(i%5),
			Low:   99 + float64(i%5),
			Close: 101 + float64(i%5),
		}
	}
	return bars
}

// TestChartWindowedScroll verifies that when there are more bars than chart
// columns, Draw produces a windowed view (no aggregation overlap), defaults
// to showing the newest bars, and scrolls back into history on demand.
func TestChartWindowedScroll(t *testing.T) {
	c := newChartCanvas()
	c.bars = makeBars(500) // 500 bars
	c.dateFmt = "01/02"

	// Render with an 80x20 inner-rect-equivalent screen.
	const w, h = 80, 20
	drawCanvasOnce(t, c, w, h)

	// 500 > chartW (~69 after reserving the axis). visibleStart should be near
	// the end of the dataset (offset 0 = newest).
	if c.visibleEnd != 500 {
		t.Fatalf("default visibleEnd = %d, want 500 (newest bar)", c.visibleEnd)
	}
	if c.visibleStart <= 0 || c.visibleStart >= 500 {
		t.Fatalf("default visibleStart = %d, expected between 1 and 499", c.visibleStart)
	}
	firstWindow := c.visibleEnd - c.visibleStart
	if firstWindow <= 0 || firstWindow > w {
		t.Fatalf("first window size = %d, expected (0, %d]", firstWindow, w)
	}

	// Scroll back 50 bars; the window should slide left by 50.
	prevEnd := c.visibleEnd
	c.scrollOffset += 50
	drawCanvasOnce(t, c, w, h)
	if c.visibleEnd != prevEnd-50 {
		t.Fatalf("after scrollBy +50: visibleEnd = %d, want %d", c.visibleEnd, prevEnd-50)
	}

	// Overshoot scroll: should clamp at the oldest data.
	c.scrollOffset = 10000
	drawCanvasOnce(t, c, w, h)
	if c.visibleStart != 0 {
		t.Fatalf("after huge overshoot: visibleStart = %d, want 0", c.visibleStart)
	}
}

// TestChartCandleSpacing verifies that:
// Candle width is now driven by the chart-zoom preset, not by an auto-fit
// based on bar count. Each preset has a known slotW; the test asserts that:
//
//   - At each zoom level, the number of bars that fit equals min(n, chartW/slotW).
//   - More aggressive zoom-out (XS) shows strictly more bars than M
//     given enough history, so the zoom control actually does something.
//
// Replaces the prior auto-fit assertions.
func TestChartCandleSpacing(t *testing.T) {
	const w, h = 80, 20
	// Canvas has a border, so InnerRect is 78x18. chartW = innerW - axis - sep = 67.
	const chartW = 67

	for zi, z := range chartZooms {
		c := newChartCanvas()
		c.zoomIdx = zi
		c.bars = makeBars(2000) // plenty to fill every zoom
		c.dateFmt = "01/02"
		drawCanvasOnce(t, c, w, h)

		// Visible window is measured in raw bars: chartW/slotW displayed slots,
		// each consuming `barsPerSlot` raw bars from history.
		wantVisible := (chartW / z.slotW) * z.barsPerSlot
		if wantVisible > 2000 {
			wantVisible = 2000
		}
		got := c.visibleEnd - c.visibleStart
		if got != wantVisible {
			t.Errorf("zoom=%s (slotW=%d, barsPerSlot=%d): visible=%d want %d",
				z.label, z.slotW, z.barsPerSlot, got, wantVisible)
		}
	}

	// Sanity: XS strictly shows more bars than M for the same chartW.
	xs := newChartCanvas()
	xs.zoomIdx = 0
	xs.bars = makeBars(500)
	xs.dateFmt = "01/02"
	drawCanvasOnce(t, xs, w, h)

	m := newChartCanvas()
	m.zoomIdx = chartZoomDefaultIdx
	m.bars = makeBars(500)
	m.dateFmt = "01/02"
	drawCanvasOnce(t, m, w, h)

	if (xs.visibleEnd - xs.visibleStart) <= (m.visibleEnd - m.visibleStart) {
		t.Errorf("XS should fit more bars than M: XS=%d M=%d",
			xs.visibleEnd-xs.visibleStart, m.visibleEnd-m.visibleStart)
	}
}

// stubAssetsAndClient sets up the in-memory asset cache + a dummy client so
// tests can drive newTermApp() without hitting the network. Keep aligned with
// the helpers above — same shape.
func stubAssetsAndClient() {
	assetMu.Lock()
	assetSymbols = []string{"AAPL"}
	assetNames = map[string]string{"AAPL": "Apple Inc."}
	assetMu.Unlock()

	client = NewAlpacaClient(Credentials{
		APIKey:    "test",
		APISecret: "test",
		BaseURL:   "http://127.0.0.1:1",
	})
}

// startSimApp boots the termApp on a tcell SimulationScreen and returns the
// app + a cleanup function. Callers can then QueueUpdate/QueueEvent against
// the live event loop.
func startSimApp(t *testing.T) (*termApp, func()) {
	t.Helper()
	a := newTermApp()
	screen := tcell.NewSimulationScreen("UTF-8")
	if err := screen.Init(); err != nil {
		t.Fatalf("init simulation screen: %v", err)
	}
	screen.SetSize(180, 50)
	a.tapp.SetScreen(screen)

	runDone := make(chan error, 1)
	go func() { runDone <- a.tapp.Run() }()

	cleanup := func() {
		a.tapp.Stop()
		select {
		case <-runDone:
		case <-time.After(2 * time.Second):
		}
	}
	time.Sleep(150 * time.Millisecond)
	return a, cleanup
}

// withChartTab switches to the chart tab + drains a frame so the canvas's
// rect is populated. After this returns, focus is on the chart canvas.
func withChartTab(t *testing.T, a *termApp) {
	t.Helper()
	a.tapp.QueueUpdateDraw(func() {
		a.switchTab(tabChart)
		// switchTab focuses chartSymField; for canvas-key tests we want the
		// canvas to receive the keys, so move focus explicitly.
		a.tapp.SetFocus(a.chartCanvasV)
	})
	time.Sleep(150 * time.Millisecond)
}

// queueRead runs fn on the event-loop goroutine and returns its result. Times
// out the test if the event loop is wedged or has exited. The QueueUpdate
// call runs in its own goroutine because tview's update channel can block if
// the application has already Stopped — without this wrapper, a buggy test
// that causes the app to quit unexpectedly would hang the whole suite.
func queueRead[T any](t *testing.T, a *termApp, fn func() T) T {
	t.Helper()
	done := make(chan T, 1)
	go func() {
		a.tapp.QueueUpdate(func() { done <- fn() })
	}()
	select {
	case v := <-done:
		return v
	case <-time.After(2 * time.Second):
		t.Fatal("event loop didn't run QueueUpdate within 2s (app may have quit)")
		var zero T
		return zero
	}
}

// TestChartLeftRightArrowsScroll proves that ← and → with the chart canvas
// focused scroll the bars instead of switching tabs. Regression test for the
// globalKeys bug where arrows were eaten before the canvas saw them.
func TestChartLeftRightArrowsScroll(t *testing.T) {
	stubAssetsAndClient()
	a, cleanup := startSimApp(t)
	defer cleanup()
	withChartTab(t, a)

	// Pre-load the canvas with bars so scrolling has something to do, and
	// trigger an initial Draw to populate visibleStep.
	a.tapp.QueueUpdateDraw(func() {
		a.chartCanvasV.bars = makeBars(500)
		a.chartCanvasV.dateFmt = "01/02"
	})
	time.Sleep(100 * time.Millisecond)

	// Confirm we're starting on the chart tab.
	if got := queueRead(t, a, func() int { return a.activeTab }); got != tabChart {
		t.Fatalf("precondition: activeTab=%d, want chart(%d)", got, tabChart)
	}
	step := queueRead(t, a, func() int { return a.chartCanvasV.visibleStep })
	if step < 1 {
		t.Fatalf("visibleStep not populated by Draw; got %d", step)
	}

	// Press Left: scrolls older (offset increases by step).
	a.tapp.QueueEvent(tcell.NewEventKey(tcell.KeyLeft, 0, tcell.ModNone))
	time.Sleep(120 * time.Millisecond)
	got := queueRead(t, a, func() int { return a.chartCanvasV.scrollOffset })
	if got != step {
		t.Fatalf("after Left: scrollOffset=%d, want %d", got, step)
	}
	if tab := queueRead(t, a, func() int { return a.activeTab }); tab != tabChart {
		t.Fatalf("Left switched tabs to %d; should stay on chart(%d)", tab, tabChart)
	}

	// Press Right: scrolls newer (offset decreases).
	a.tapp.QueueEvent(tcell.NewEventKey(tcell.KeyRight, 0, tcell.ModNone))
	time.Sleep(120 * time.Millisecond)
	got = queueRead(t, a, func() int { return a.chartCanvasV.scrollOffset })
	if got != 0 {
		t.Fatalf("after Right: scrollOffset=%d, want 0", got)
	}
	if tab := queueRead(t, a, func() int { return a.activeTab }); tab != tabChart {
		t.Fatalf("Right switched tabs to %d; should stay on chart(%d)", tab, tabChart)
	}
}

// TestChartEscapeReturnsToSymbolNotQuit proves Esc on the chart canvas moves
// focus back to the symbol input rather than terminating the app.
func TestChartEscapeReturnsToSymbolNotQuit(t *testing.T) {
	stubAssetsAndClient()
	a, cleanup := startSimApp(t)
	defer cleanup()
	withChartTab(t, a)

	a.tapp.QueueEvent(tcell.NewEventKey(tcell.KeyEscape, 0, tcell.ModNone))
	time.Sleep(150 * time.Millisecond)

	// App must still be running.
	if onSym := queueRead(t, a, func() bool { return a.tapp.GetFocus() == a.chartSymField }); !onSym {
		t.Fatal("Esc on chart canvas didn't move focus to chart symbol input")
	}
}

// TestChartTimeframeHotkeys proves '{' and '}' cycle the candle timeframe
// while on the chart canvas.
func TestChartTimeframeHotkeys(t *testing.T) {
	stubAssetsAndClient()
	a, cleanup := startSimApp(t)
	defer cleanup()
	withChartTab(t, a)

	start := queueRead(t, a, func() int { return a.chartTFIdx })

	a.tapp.QueueEvent(tcell.NewEventKey(tcell.KeyRune, '}', tcell.ModNone))
	time.Sleep(120 * time.Millisecond)
	after := queueRead(t, a, func() int { return a.chartTFIdx })
	want := (start + 1) % len(chartTimeframes)
	if after != want {
		t.Fatalf("after '}': tfIdx=%d want %d", after, want)
	}

	a.tapp.QueueEvent(tcell.NewEventKey(tcell.KeyRune, '{', tcell.ModNone))
	time.Sleep(120 * time.Millisecond)
	after = queueRead(t, a, func() int { return a.chartTFIdx })
	if after != start {
		t.Fatalf("after '{': tfIdx=%d want %d (back to start)", after, start)
	}
}

// TestChartRangeHotkeyStillWorks is a smoke test for the existing letter
// hotkeys, ensuring my refactor didn't break range selection.
func TestChartRangeHotkeyStillWorks(t *testing.T) {
	stubAssetsAndClient()
	a, cleanup := startSimApp(t)
	defer cleanup()
	withChartTab(t, a)

	a.tapp.QueueEvent(tcell.NewEventKey(tcell.KeyRune, 'y', tcell.ModNone))
	time.Sleep(120 * time.Millisecond)
	// 'y' is the hotkey for 1Y, which is index 4 in chartRanges.
	if got := queueRead(t, a, func() int { return a.chartRangeIdx }); got != 4 {
		t.Fatalf("after 'y': rangeIdx=%d want 4 (1Y)", got)
	}
}

// TestTabSwitchingViaNumberKeys verifies 1..5 switch tabs from tables AND
// from dropdowns. Input fields type the digit instead — that's by design;
// users can click the tab or Esc out first. The key regression is the
// dropdown case: '3' on the Trade tab's actionDD used to be swallowed.
func TestTabSwitchingViaNumberKeys(t *testing.T) {
	stubAssetsAndClient()
	a, cleanup := startSimApp(t)
	defer cleanup()

	// Positions (Table focus) → Trade via '2'.
	a.tapp.QueueEvent(tcell.NewEventKey(tcell.KeyRune, '2', tcell.ModNone))
	time.Sleep(100 * time.Millisecond)
	if got := queueRead(t, a, func() int { return a.activeTab }); got != tabTrade {
		t.Fatalf("after '2': activeTab=%d want %d", got, tabTrade)
	}

	// Trade tab's focus is actionDD (DropDown). '3' must still switch tabs.
	// Regression for the dropdown-swallowing-number-keys bug.
	a.tapp.QueueEvent(tcell.NewEventKey(tcell.KeyRune, '3', tcell.ModNone))
	time.Sleep(100 * time.Millisecond)
	if got := queueRead(t, a, func() int { return a.activeTab }); got != tabOrders {
		t.Fatalf("after '3' from dropdown: activeTab=%d want %d (dropdown swallowed it?)", got, tabOrders)
	}

	a.tapp.QueueEvent(tcell.NewEventKey(tcell.KeyRune, '4', tcell.ModNone))
	time.Sleep(80 * time.Millisecond)
	if got := queueRead(t, a, func() int { return a.activeTab }); got != tabActivity {
		t.Errorf("after '4': activeTab=%d want %d", got, tabActivity)
	}
	a.tapp.QueueEvent(tcell.NewEventKey(tcell.KeyRune, '5', tcell.ModNone))
	time.Sleep(80 * time.Millisecond)
	if got := queueRead(t, a, func() int { return a.activeTab }); got != tabChart {
		t.Errorf("after '5': activeTab=%d want %d", got, tabChart)
	}
}

// TestPlaceOrderRejectsEmptySymbol drives the trade form with an empty symbol
// and verifies an error message is surfaced rather than an order being sent.
func TestPlaceOrderRejectsEmptySymbol(t *testing.T) {
	stubAssetsAndClient()
	a, cleanup := startSimApp(t)
	defer cleanup()

	a.tapp.QueueUpdateDraw(func() {
		a.switchTab(tabTrade)
		a.symField.SetText("")
		a.qtyField.SetText("10")
		a.onSubmit()
	})
	time.Sleep(150 * time.Millisecond)

	got := queueRead(t, a, func() string { return a.resultTV.GetText(true) })
	if got == "" || !contains(got, "SYMBOL IS REQUIRED") {
		t.Fatalf("expected SYMBOL IS REQUIRED, got %q", got)
	}
	if active := queueRead(t, a, func() bool { return a.confirmActive }); active {
		t.Fatal("confirmation modal should not appear for invalid input")
	}
}

// TestPlaceOrderOpensConfirmModal verifies a valid market order surfaces the
// confirm modal before any HTTP call goes out.
func TestPlaceOrderOpensConfirmModal(t *testing.T) {
	stubAssetsAndClient()
	a, cleanup := startSimApp(t)
	defer cleanup()

	a.tapp.QueueUpdateDraw(func() {
		a.switchTab(tabTrade)
		a.actionDD.SetCurrentOption(0) // BUY
		a.typeDD.SetCurrentOption(0)   // MARKET
		a.symField.SetText("AAPL")
		a.qtyField.SetText("5")
		a.priceField.SetText("")
		a.onSubmit()
	})
	time.Sleep(150 * time.Millisecond)

	if active := queueRead(t, a, func() bool { return a.confirmActive }); !active {
		t.Fatal("confirmation modal didn't open for valid order")
	}
}

func contains(haystack, needle string) bool {
	// tiny helper so we don't have to import strings just for this
	if len(haystack) < len(needle) {
		return false
	}
	for i := 0; i+len(needle) <= len(haystack); i++ {
		if haystack[i:i+len(needle)] == needle {
			return true
		}
	}
	return false
}

// ── New behavior: < > switch tabs globally; arrows are reserved for chart
// pan/scroll and table row navigation. ───────────────────────────────────────

// TestAngleBracketsSwitchTabs verifies '>' advances the active tab and '<'
// goes back, from a non-input focus. Regression for the change that retired
// the ← / → tab-switching shortcuts.
func TestAngleBracketsSwitchTabs(t *testing.T) {
	stubAssetsAndClient()
	a, cleanup := startSimApp(t)
	defer cleanup()

	// Start on positions. '>' → trade.
	a.tapp.QueueEvent(tcell.NewEventKey(tcell.KeyRune, '>', tcell.ModNone))
	time.Sleep(80 * time.Millisecond)
	if got := queueRead(t, a, func() int { return a.activeTab }); got != tabTrade {
		t.Fatalf("'>' from positions: activeTab=%d want %d", got, tabTrade)
	}
	// '<' → back to positions.
	a.tapp.QueueEvent(tcell.NewEventKey(tcell.KeyRune, '<', tcell.ModNone))
	time.Sleep(80 * time.Millisecond)
	if got := queueRead(t, a, func() int { return a.activeTab }); got != tabPositions {
		t.Fatalf("'<' from trade: activeTab=%d want %d", got, tabPositions)
	}
	// '<' from positions wraps backward to chart.
	a.tapp.QueueEvent(tcell.NewEventKey(tcell.KeyRune, '<', tcell.ModNone))
	time.Sleep(80 * time.Millisecond)
	if got := queueRead(t, a, func() int { return a.activeTab }); got != tabChart {
		t.Fatalf("'<' from positions: activeTab=%d want %d (should wrap)", got, tabChart)
	}
}

// TestArrowsDoNotSwitchTabs proves left/right arrows no longer switch tabs.
// On a Table focus (Positions), Left/Right should be inert.
func TestArrowsDoNotSwitchTabs(t *testing.T) {
	stubAssetsAndClient()
	a, cleanup := startSimApp(t)
	defer cleanup()

	a.tapp.QueueEvent(tcell.NewEventKey(tcell.KeyRight, 0, tcell.ModNone))
	time.Sleep(80 * time.Millisecond)
	if got := queueRead(t, a, func() int { return a.activeTab }); got != tabPositions {
		t.Fatalf("Right from positions changed tab to %d; arrows must not switch tabs anymore", got)
	}
	a.tapp.QueueEvent(tcell.NewEventKey(tcell.KeyLeft, 0, tcell.ModNone))
	time.Sleep(80 * time.Millisecond)
	if got := queueRead(t, a, func() int { return a.activeTab }); got != tabPositions {
		t.Fatalf("Left from positions changed tab to %d; arrows must not switch tabs anymore", got)
	}
}

// TestChartYAxisRigidUnderHorizontalScroll proves that scrolling horizontally
// (←/→ or ,/.) leaves yMin/yMax untouched after the first auto-fit.
func TestChartYAxisRigidUnderHorizontalScroll(t *testing.T) {
	c := newChartCanvas()
	c.bars = makeBars(500)
	c.dateFmt = "01/02"

	// First draw fits y-axis to the visible (newest) window.
	drawCanvasOnce(t, c, 80, 20)
	if !c.yLocked {
		t.Fatal("first draw should set yLocked=true")
	}
	yMin0, yMax0 := c.yMin, c.yMax

	// Scroll back 50 bars and redraw.
	c.scrollOffset += 50
	drawCanvasOnce(t, c, 80, 20)
	if c.yMin != yMin0 || c.yMax != yMax0 {
		t.Fatalf("y-axis moved on horizontal scroll: was (%.2f,%.2f) now (%.2f,%.2f)",
			yMin0, yMax0, c.yMin, c.yMax)
	}

	// Scroll way back; still locked.
	c.scrollOffset = 400
	drawCanvasOnce(t, c, 80, 20)
	if c.yMin != yMin0 || c.yMax != yMax0 {
		t.Fatalf("y-axis moved on big scroll: was (%.2f,%.2f) now (%.2f,%.2f)",
			yMin0, yMax0, c.yMin, c.yMax)
	}
}

// TestChartYAxisPan proves Up/Down arrows pan the rigid y-axis and that the
// span is preserved (pan, not zoom).
func TestChartYAxisPan(t *testing.T) {
	stubAssetsAndClient()
	a, cleanup := startSimApp(t)
	defer cleanup()
	withChartTab(t, a)

	// Seed bars + force initial draw so yMin/yMax + yLocked are populated.
	a.tapp.QueueUpdateDraw(func() {
		a.chartCanvasV.bars = makeBars(60)
		a.chartCanvasV.dateFmt = "01/02"
		a.chartCanvasV.yLocked = false
	})
	time.Sleep(120 * time.Millisecond)

	type yRange struct{ min, max float64 }
	read := func() yRange {
		return queueRead(t, a, func() yRange {
			return yRange{a.chartCanvasV.yMin, a.chartCanvasV.yMax}
		})
	}
	r0 := read()
	span0 := r0.max - r0.min
	if span0 <= 0 {
		t.Fatalf("initial y-span = %.4f, expected > 0 (yLocked=%v)", span0, queueRead(t, a, func() bool { return a.chartCanvasV.yLocked }))
	}

	// Up arrow shifts BOTH bounds up by 10% of span.
	a.tapp.QueueEvent(tcell.NewEventKey(tcell.KeyUp, 0, tcell.ModNone))
	time.Sleep(80 * time.Millisecond)
	r1 := read()
	span1 := r1.max - r1.min
	if !approxEq(span1, span0, 1e-6) {
		t.Errorf("y-span changed: was %.4f now %.4f (pan should preserve span)", span0, span1)
	}
	wantDelta := span0 * 0.10
	if !approxEq(r1.min-r0.min, wantDelta, 1e-6) || !approxEq(r1.max-r0.max, wantDelta, 1e-6) {
		t.Errorf("Up pan: min Δ=%.4f max Δ=%.4f, want %.4f for both", r1.min-r0.min, r1.max-r0.max, wantDelta)
	}

	// Down arrow reverses it.
	a.tapp.QueueEvent(tcell.NewEventKey(tcell.KeyDown, 0, tcell.ModNone))
	time.Sleep(80 * time.Millisecond)
	r2 := read()
	if !approxEq(r2.min, r0.min, 1e-6) || !approxEq(r2.max, r0.max, 1e-6) {
		t.Errorf("Down didn't reverse Up: got (%.4f,%.4f) want (%.4f,%.4f)", r2.min, r2.max, r0.min, r0.max)
	}
}

// TestChartZeroKeyResetsYAxis verifies pressing '0' refits the y-axis to the
// visible data. The next Draw after the keypress sees yLocked=false and
// recomputes, so we assert on the resulting yMin/yMax (they should return to
// the original auto-fit values, undoing any prior pan).
func TestChartZeroKeyResetsYAxis(t *testing.T) {
	stubAssetsAndClient()
	a, cleanup := startSimApp(t)
	defer cleanup()
	withChartTab(t, a)

	a.tapp.QueueUpdateDraw(func() {
		a.chartCanvasV.bars = makeBars(60)
		a.chartCanvasV.dateFmt = "01/02"
		a.chartCanvasV.yLocked = false
	})
	time.Sleep(120 * time.Millisecond)

	// Capture the auto-fit range before any pan.
	type yRange struct{ min, max float64 }
	read := func() yRange {
		return queueRead(t, a, func() yRange {
			return yRange{a.chartCanvasV.yMin, a.chartCanvasV.yMax}
		})
	}
	fit := read()
	if fit.max-fit.min <= 0 {
		t.Fatalf("auto-fit span = %.4f, expected > 0", fit.max-fit.min)
	}

	// Pan up twice — moves yMin and yMax away from the fit values.
	a.tapp.QueueEvent(tcell.NewEventKey(tcell.KeyUp, 0, tcell.ModNone))
	a.tapp.QueueEvent(tcell.NewEventKey(tcell.KeyUp, 0, tcell.ModNone))
	time.Sleep(80 * time.Millisecond)
	panned := read()
	if approxEq(panned.min, fit.min, 1e-6) {
		t.Fatalf("pan didn't move yMin: still %.4f", panned.min)
	}

	// '0' resets — force a draw, then yMin/yMax should match the original fit.
	a.tapp.QueueEvent(tcell.NewEventKey(tcell.KeyRune, '0', tcell.ModNone))
	time.Sleep(40 * time.Millisecond)
	a.tapp.QueueUpdateDraw(func() {}) // force a redraw on the event loop
	time.Sleep(80 * time.Millisecond)
	after := read()
	if !approxEq(after.min, fit.min, 1e-6) || !approxEq(after.max, fit.max, 1e-6) {
		t.Fatalf("'0' didn't refit: got (%.4f,%.4f) want (%.4f,%.4f)",
			after.min, after.max, fit.min, fit.max)
	}
}

// TestChartCurrentPriceLineRenders verifies the dotted line + price box are
// drawn at the row corresponding to the latest bar's close.
func TestChartCurrentPriceLineRenders(t *testing.T) {
	c := newChartCanvas()
	c.bars = makeBars(40)
	c.dateFmt = "01/02"

	const w, h = 80, 20
	screen := tcell.NewSimulationScreen("UTF-8")
	if err := screen.Init(); err != nil {
		t.Fatalf("init: %v", err)
	}
	defer screen.Fini()
	screen.SetSize(w, h)
	c.SetRect(0, 0, w, h)
	c.Draw(screen)
	screen.Show() // SimulationScreen requires Show() before GetContent sees writes.

	// Find the price box: any cell with bg == cGreen / cRed / cYellow (the
	// only places those colors appear as a background — candles use them for
	// fg with cBlack bg). Scan from the right edge inward since the box is
	// always on the right axis.
	foundRow := -1
	foundCol := -1
	for col := w - 1; col >= 0; col-- {
		for row := 0; row < h; row++ {
			_, _, st, _ := screen.GetContent(col, row)
			_, bg, _ := st.Decompose()
			if bg == cGreen || bg == cRed || bg == cYellow {
				foundRow, foundCol = row, col
				break
			}
		}
		if foundRow >= 0 {
			break
		}
	}
	if foundRow < 0 {
		t.Fatal("did not find a current-price box (no cell with green/red/yellow bg)")
	}
	if foundCol < w-15 {
		t.Errorf("price box at col %d is not on the right axis (w=%d)", foundCol, w)
	}

	// Verify the box has digits inside (the formatted price like "105.00").
	hasDigit := false
	for col := foundCol - 10; col <= foundCol+10 && col < w; col++ {
		if col < 0 {
			continue
		}
		ch, _, _, _ := screen.GetContent(col, foundRow)
		if ch >= '0' && ch <= '9' {
			hasDigit = true
			break
		}
	}
	if !hasDigit {
		t.Fatalf("price box at row %d has no digits in its row", foundRow)
	}

	// Verify a dotted character renders somewhere in the chart row at foundRow.
	dottedFound := false
	for col := 0; col < foundCol; col++ {
		ch, _, _, _ := screen.GetContent(col, foundRow)
		if ch == '─' {
			dottedFound = true
			break
		}
	}
	if !dottedFound {
		t.Fatalf("no '─' in price-line row %d; expected a dotted horizontal line", foundRow)
	}
}

func approxEq(a, b, eps float64) bool {
	d := a - b
	if d < 0 {
		d = -d
	}
	return d <= eps
}

// ── Trade-tab bug: letters quit the app instead of typing into fields. ──────

// TestTradeSymbolFieldAcceptsLetters verifies that pressing Q/R/etc. while
// focused on the symbol input fills the field instead of quitting/refreshing.
// User report: "I cant type q or other letters in the trading menu when
// buying stocks. it just quits the app."
func TestTradeSymbolFieldAcceptsLetters(t *testing.T) {
	stubAssetsAndClient()
	a, cleanup := startSimApp(t)
	defer cleanup()

	a.tapp.QueueUpdateDraw(func() {
		a.switchTab(tabTrade)
		a.tapp.SetFocus(a.symField)
	})
	time.Sleep(150 * time.Millisecond)

	for _, ch := range []rune{'Q', 'Q', 'Q'} {
		a.tapp.QueueEvent(tcell.NewEventKey(tcell.KeyRune, ch, tcell.ModNone))
		time.Sleep(60 * time.Millisecond)
	}
	text := queueRead(t, a, func() string { return a.symField.GetText() })
	if text != "QQQ" {
		t.Fatalf("symField after typing QQQ: got %q want %q (app may have quit)", text, "QQQ")
	}
}

// TestTradeButtonLettersDontQuit verifies that letters on the PLACE ORDER
// button don't trigger global Q/R shortcuts. Users often tab through to a
// button and then mistype; the app shouldn't exit on a stray keypress.
func TestTradeButtonLettersDontQuit(t *testing.T) {
	stubAssetsAndClient()
	a, cleanup := startSimApp(t)
	defer cleanup()

	a.tapp.QueueUpdateDraw(func() {
		a.switchTab(tabTrade)
		// Find the PLACE ORDER button and focus it. It's the 6th form item
		// (index 5: 2 dropdowns + 3 inputs + this button).
		btn := a.form.GetButton(0) // first button = PLACE ORDER
		a.tapp.SetFocus(btn)
	})
	time.Sleep(150 * time.Millisecond)

	a.tapp.QueueEvent(tcell.NewEventKey(tcell.KeyRune, 'q', tcell.ModNone))
	time.Sleep(120 * time.Millisecond)

	// queueRead times out (and fails the test) if the app quit.
	if got := queueRead(t, a, func() int { return a.activeTab }); got != tabTrade {
		t.Fatalf("activeTab after 'q' on button: %d (want %d)", got, tabTrade)
	}
}

// TestTradeDropdownLettersDontQuit verifies that letters on the ACTION/TYPE
// dropdowns don't trigger global Q/R. They only have 2 options each so
// type-to-search is useless, but typing q/r should still NOT exit the app.
func TestTradeDropdownLettersDontQuit(t *testing.T) {
	stubAssetsAndClient()
	a, cleanup := startSimApp(t)
	defer cleanup()

	a.tapp.QueueUpdateDraw(func() {
		a.switchTab(tabTrade)
		a.tapp.SetFocus(a.actionDD)
	})
	time.Sleep(150 * time.Millisecond)

	a.tapp.QueueEvent(tcell.NewEventKey(tcell.KeyRune, 'q', tcell.ModNone))
	time.Sleep(120 * time.Millisecond)

	if got := queueRead(t, a, func() int { return a.activeTab }); got != tabTrade {
		t.Fatalf("activeTab after 'q' on dropdown: %d (want %d)", got, tabTrade)
	}
}

// ── Race regression: slow stale load must NOT overwrite fast fresh load. ────

// TestLoadChartLatestWins simulates the user clicking timeframe '1m' (slow
// network) and then quickly clicking '1D' (fast). Without the chartLoadGen
// guard the slow 1m response would arrive last and clobber the 1D bars,
// leaving the chart "stuck" on the wrong timeframe. With the guard, the 1m
// goroutine sees the generation has advanced and silently bails — chart
// shows the 1D data.
func TestLoadChartLatestWins(t *testing.T) {
	// Spin up an httptest server that delays per-timeframe so we can
	// deterministically order completions.
	type tfResp struct {
		delay time.Duration
		price float64 // unique close to identify which response wrote the bars
	}
	tfs := map[string]tfResp{
		"1Min": {delay: 600 * time.Millisecond, price: 100.0},
		"1Day": {delay: 50 * time.Millisecond, price: 500.0},
	}
	mux := http.NewServeMux()
	mux.HandleFunc("/v2/stocks/", func(w http.ResponseWriter, r *http.Request) {
		tf := r.URL.Query().Get("timeframe")
		resp, ok := tfs[tf]
		if !ok {
			resp = tfResp{delay: 10 * time.Millisecond, price: 1.0}
		}
		time.Sleep(resp.delay)
		w.Header().Set("Content-Type", "application/json")
		fmt.Fprintf(w, `{"bars":[{"t":"2024-01-01T00:00:00Z","o":%[1]f,"h":%[1]f,"l":%[1]f,"c":%[1]f,"v":100}]}`, resp.price)
	})
	server := httptest.NewServer(mux)
	defer server.Close()

	// Redirect both REST and market-data hosts at the test server. GetBars
	// uses alpacaDataBase; configure NewAlpacaClient via Credentials.BaseURL.
	prev := alpacaDataBase
	alpacaDataBase = server.URL
	defer func() { alpacaDataBase = prev }()
	stubAssetsAndClient()
	client = NewAlpacaClient(Credentials{
		APIKey:    "test",
		APISecret: "test",
		BaseURL:   server.URL,
	})

	a, cleanup := startSimApp(t)
	defer cleanup()
	withChartTab(t, a)

	// Pre-fill a symbol so selectChartTF actually fires loadChart.
	a.tapp.QueueUpdateDraw(func() {
		a.chartSymField.SetText("AAPL")
	})
	time.Sleep(80 * time.Millisecond)

	// Trigger the slow 1m load.
	a.tapp.QueueUpdate(func() { a.selectChartTF(0) }) // 1m
	time.Sleep(80 * time.Millisecond)                  // let it kick off the HTTP call
	// Now trigger the fast 1D load — generation advances, slow result will be discarded.
	a.tapp.QueueUpdate(func() { a.selectChartTF(5) }) // 1D

	// Wait long enough for BOTH responses to land (slow=600ms + buffer).
	time.Sleep(1200 * time.Millisecond)

	got := queueRead(t, a, func() float64 {
		if len(a.chartCanvasV.bars) == 0 {
			return -1
		}
		return a.chartCanvasV.bars[0].Close
	})
	if got != 500.0 {
		t.Fatalf("chart shows stale data: bars[0].Close=%.1f, want 500.0 (fast 1D load) — slow 1m response leaked through", got)
	}
}

// TestLoadChartManyConcurrentLatestWins flips between timeframes a bunch of
// times in quick succession and verifies the chart still ends on the final
// selection (and never an intermediate one).
func TestLoadChartManyConcurrentLatestWins(t *testing.T) {
	// Random-ish delays so completion order is shuffled.
	rng := []time.Duration{200, 80, 400, 50, 300, 120, 250, 30}
	prices := []float64{1, 2, 3, 4, 5, 6, 7, 8}
	mux := http.NewServeMux()
	mux.HandleFunc("/v2/stocks/", func(w http.ResponseWriter, r *http.Request) {
		tf := r.URL.Query().Get("timeframe")
		idx := tfIndex(tf)
		if idx < 0 || idx >= len(rng) {
			idx = 0
		}
		time.Sleep(rng[idx] * time.Millisecond)
		fmt.Fprintf(w, `{"bars":[{"t":"2024-01-01T00:00:00Z","o":%[1]f,"h":%[1]f,"l":%[1]f,"c":%[1]f,"v":100}]}`, prices[idx])
	})
	server := httptest.NewServer(mux)
	defer server.Close()
	prev := alpacaDataBase
	alpacaDataBase = server.URL
	defer func() { alpacaDataBase = prev }()
	stubAssetsAndClient()
	client = NewAlpacaClient(Credentials{APIKey: "t", APISecret: "t", BaseURL: server.URL})

	a, cleanup := startSimApp(t)
	defer cleanup()
	withChartTab(t, a)
	a.tapp.QueueUpdateDraw(func() { a.chartSymField.SetText("AAPL") })
	time.Sleep(60 * time.Millisecond)

	// Fire every timeframe in sequence, ending on tf index 7 (price 8).
	for i := 0; i < len(chartTimeframes); i++ {
		idx := i
		a.tapp.QueueUpdate(func() { a.selectChartTF(idx) })
		time.Sleep(20 * time.Millisecond)
	}

	// Wait long enough for the slowest (400ms) response to finish.
	time.Sleep(700 * time.Millisecond)

	got := queueRead(t, a, func() float64 {
		if len(a.chartCanvasV.bars) == 0 {
			return -1
		}
		return a.chartCanvasV.bars[0].Close
	})
	if got != prices[len(prices)-1] {
		t.Fatalf("chart shows %.1f after rapid TF flips, want %.1f (final selection)", got, prices[len(prices)-1])
	}
}

// tfIndex finds the timeframe index by Alpaca's value string ("1Min", etc.).
func tfIndex(value string) int {
	for i, t := range chartTimeframes {
		if t.value == value {
			return i
		}
	}
	return -1
}

// ── EMA indicator ──────────────────────────────────────────────────────────

// TestComputeEMAMath spot-checks the EMA recurrence against a hand-computed
// example. With period=3 (k = 2/4 = 0.5) and closes [10,11,12,13,14]:
//   - ema[0..1] = NaN
//   - ema[2] = SMA(10,11,12) = 11
//   - ema[3] = 13*0.5 + 11*0.5 = 12
//   - ema[4] = 14*0.5 + 12*0.5 = 13
func TestComputeEMAMath(t *testing.T) {
	closes := []float64{10, 11, 12, 13, 14}
	bars := make([]Bar, len(closes))
	for i, c := range closes {
		bars[i] = Bar{Close: c}
	}
	ema := computeEMA(bars, 3)
	if !math.IsNaN(ema[0]) || !math.IsNaN(ema[1]) {
		t.Fatalf("ema[0..1] should be NaN; got %v, %v", ema[0], ema[1])
	}
	want := []float64{11.0, 12.0, 13.0}
	for i, w := range want {
		got := ema[i+2]
		if math.Abs(got-w) > 1e-9 {
			t.Errorf("ema[%d] = %.6f want %.6f", i+2, got, w)
		}
	}
}

// TestComputeEMAInsufficientBars verifies an all-NaN result when there aren't
// enough bars to seed the period (so the Draw code skips the overlay cleanly).
func TestComputeEMAInsufficientBars(t *testing.T) {
	bars := []Bar{{Close: 1}, {Close: 2}}
	ema := computeEMA(bars, 10)
	for i, v := range ema {
		if !math.IsNaN(v) {
			t.Errorf("ema[%d] = %v, want NaN (insufficient bars)", i, v)
		}
	}
}

// TestEMACycleKeyboardChangesPeriod verifies pressing 'e' cycles forward
// through emaPeriods and 'E' cycles backward, with the canvas's emaPeriod
// kept in sync with the selector index.
func TestEMACycleKeyboardChangesPeriod(t *testing.T) {
	stubAssetsAndClient()
	a, cleanup := startSimApp(t)
	defer cleanup()
	withChartTab(t, a)

	// Default is index 2 (period 10).
	start := queueRead(t, a, func() int { return a.chartEMAIdx })
	if start != emaDefaultIdx {
		t.Fatalf("default EMA idx = %d, want %d", start, emaDefaultIdx)
	}
	if p := queueRead(t, a, func() int { return a.chartCanvasV.emaPeriod }); p != 10 {
		t.Fatalf("default emaPeriod = %d, want 10", p)
	}

	// 'e' advances.
	a.tapp.QueueEvent(tcell.NewEventKey(tcell.KeyRune, 'e', tcell.ModNone))
	time.Sleep(80 * time.Millisecond)
	if idx := queueRead(t, a, func() int { return a.chartEMAIdx }); idx != start+1 {
		t.Fatalf("after 'e': idx = %d want %d", idx, start+1)
	}
	if p := queueRead(t, a, func() int { return a.chartCanvasV.emaPeriod }); p != emaPeriods[start+1].period {
		t.Fatalf("after 'e': emaPeriod = %d want %d", p, emaPeriods[start+1].period)
	}

	// 'E' goes back.
	a.tapp.QueueEvent(tcell.NewEventKey(tcell.KeyRune, 'E', tcell.ModNone))
	time.Sleep(80 * time.Millisecond)
	if idx := queueRead(t, a, func() int { return a.chartEMAIdx }); idx != start {
		t.Fatalf("after 'E': idx = %d want %d", idx, start)
	}
}

// TestEMAOffHidesLine verifies selecting OFF (index 0) prevents the overlay
// from rendering.
func TestEMAOffHidesLine(t *testing.T) {
	c := newChartCanvas()
	c.bars = makeBars(60)
	c.dateFmt = "01/02"
	c.emaPeriod = 0 // OFF

	const w, h = 80, 22
	screen := tcell.NewSimulationScreen("UTF-8")
	if err := screen.Init(); err != nil {
		t.Fatalf("init: %v", err)
	}
	defer screen.Fini()
	screen.SetSize(w, h)
	c.SetRect(0, 0, w, h)
	c.Draw(screen)
	screen.Show()

	// Verify no EMA-styled cells were painted. The EMA color is cCyan; the
	// only other thing using cCyan as fg is... nothing on the chart canvas
	// itself. (cCyan is used for the BUY side in tables, not here.)
	for row := 0; row < h; row++ {
		for col := 0; col < w; col++ {
			_, _, st, _ := screen.GetContent(col, row)
			fg, _, _ := st.Decompose()
			if fg == cCyan {
				t.Fatalf("found cCyan cell at (%d,%d) with EMA off — overlay leaked", col, row)
			}
		}
	}
}

// TestEMALineRenders verifies that turning the EMA on produces a cyan
// Braille overlay (runes in U+2800–U+28FF) — the new smooth-line rendering.
// Also confirms the old single-char line glyphs are no longer used.
func TestEMALineRenders(t *testing.T) {
	c := newChartCanvas()
	c.bars = makeBars(60)
	c.dateFmt = "01/02"
	c.emaPeriod = 10

	const w, h = 100, 22
	screen := tcell.NewSimulationScreen("UTF-8")
	if err := screen.Init(); err != nil {
		t.Fatalf("init: %v", err)
	}
	defer screen.Fini()
	screen.SetSize(w, h)
	c.SetRect(0, 0, w, h)
	c.Draw(screen)
	screen.Show()

	brailleCells := 0
	oldGlyphCells := 0
	for row := 0; row < h; row++ {
		for col := 0; col < w; col++ {
			ch, _, st, _ := screen.GetContent(col, row)
			fg, _, _ := st.Decompose()
			if fg != cCyan {
				continue
			}
			if ch >= 0x2800 && ch <= 0x28FF {
				brailleCells++
			}
			switch ch {
			case '╱', '╲', '●':
				oldGlyphCells++
			}
		}
	}
	if brailleCells == 0 {
		t.Fatal("EMA overlay didn't draw any cyan Braille cells — smooth rendering missing")
	}
	if oldGlyphCells > 0 {
		t.Fatalf("found %d cells using legacy line glyphs — should all be Braille now", oldGlyphCells)
	}
}

// ── Braille primitive unit tests ───────────────────────────────────────────

// TestBrailleBitMapping verifies each of the 8 sub-pixel positions maps to
// the correct bit of U+2800. If this is wrong, every Braille rune is
// scrambled and the EMA renders as garbage.
func TestBrailleBitMapping(t *testing.T) {
	cases := []struct {
		subX, subY int
		wantBit    byte
	}{
		{0, 0, 0x01}, // dot 1
		{0, 1, 0x02}, // dot 2
		{0, 2, 0x04}, // dot 3
		{1, 0, 0x08}, // dot 4
		{1, 1, 0x10}, // dot 5
		{1, 2, 0x20}, // dot 6
		{0, 3, 0x40}, // dot 7
		{1, 3, 0x80}, // dot 8
	}
	for _, tc := range cases {
		got := brailleBit(tc.subX, tc.subY)
		if got != tc.wantBit {
			t.Errorf("brailleBit(%d,%d) = 0x%02x, want 0x%02x", tc.subX, tc.subY, got, tc.wantBit)
		}
	}
}

// TestBrailleLineLightsExpectedCells draws a horizontal Bresenham line across
// multiple cells in sub-pixel space and verifies each cell ends up with at
// least one dot. Catches off-by-one errors in plot() and Bresenham.
func TestBrailleLineLightsExpectedCells(t *testing.T) {
	l := newBrailleLayer()
	// Horizontal line at sub-row 0, sub-columns 0..9 (covers terminal cols 0..4).
	l.line(0, 0, 9, 0)
	for col := 0; col < 5; col++ {
		bits, ok := l.cells[[2]int{col, 0}]
		if !ok || bits == 0 {
			t.Errorf("cell (%d, 0) has no dots after horizontal line", col)
		}
	}
}

// TestBrailleThickLineDoublesWidth verifies thickLine plots adjacent
// sub-pixels — for a 1-cell-wide horizontal line, we expect dots in BOTH
// sub-rows 0 and 1, not just sub-row 0.
func TestBrailleThickLineDoublesWidth(t *testing.T) {
	l := newBrailleLayer()
	l.thickLine(0, 0, 5, 0) // horizontal: thickness expands vertically
	bits := l.cells[[2]int{0, 0}]
	// Sub-row 0 dot (bit 0) AND sub-row 1 dot (bit 1) should both be set.
	if bits&0x01 == 0 {
		t.Errorf("thickLine missing primary dot (bit 0); bits=0x%02x", bits)
	}
	if bits&0x02 == 0 {
		t.Errorf("thickLine missing parallel dot (bit 1); bits=0x%02x", bits)
	}
}

// TestBrailleRenderEmitsCorrectRune verifies that renderAt emits the correct
// U+2800-offset rune for the accumulated bitmask, anchored at the right origin.
func TestBrailleRenderEmitsCorrectRune(t *testing.T) {
	l := newBrailleLayer()
	l.plot(0, 0)
	l.plot(1, 1) // bits 0 and 4 → 0x11

	screen := tcell.NewSimulationScreen("UTF-8")
	if err := screen.Init(); err != nil {
		t.Fatalf("init: %v", err)
	}
	defer screen.Fini()
	screen.SetSize(10, 5)
	style := tcell.StyleDefault.Foreground(cCyan)
	l.renderAt(screen, 3, 2, style) // origin at (3,2)
	screen.Show()

	ch, _, st, _ := screen.GetContent(3, 2) // cell at origin
	wantRune := rune(0x2800 + 0x11)
	if ch != wantRune {
		t.Errorf("renderAt: cell (3,2) rune = U+%04X, want U+%04X", ch, wantRune)
	}
	fg, _, _ := st.Decompose()
	if fg != cCyan {
		t.Errorf("renderAt: fg = %v, want cCyan", fg)
	}
}

// ── Zoom control ────────────────────────────────────────────────────────────

// TestChartZoomDefault verifies the canvas boots at zoom level M (the index
// of the prior "sparse" preset).
func TestChartZoomDefault(t *testing.T) {
	c := newChartCanvas()
	if c.zoomIdx != chartZoomDefaultIdx {
		t.Fatalf("default zoomIdx = %d, want %d (M)", c.zoomIdx, chartZoomDefaultIdx)
	}
	if chartZooms[c.zoomIdx].label != "M" {
		t.Fatalf("default zoom label = %q, want \"M\"", chartZooms[c.zoomIdx].label)
	}
}

// TestZoomCycleKeyboard verifies 'z' decreases the index (zoom out) and 'Z'
// increases it (zoom in), with proper wrap-around in both directions.
func TestZoomCycleKeyboard(t *testing.T) {
	stubAssetsAndClient()
	a, cleanup := startSimApp(t)
	defer cleanup()
	withChartTab(t, a)

	if start := queueRead(t, a, func() int { return a.chartCanvasV.zoomIdx }); start != chartZoomDefaultIdx {
		t.Fatalf("default zoomIdx = %d want %d", start, chartZoomDefaultIdx)
	}

	// 'z' zooms OUT: M(2) → S(1).
	a.tapp.QueueEvent(tcell.NewEventKey(tcell.KeyRune, 'z', tcell.ModNone))
	time.Sleep(80 * time.Millisecond)
	if got := queueRead(t, a, func() int { return a.chartCanvasV.zoomIdx }); got != chartZoomDefaultIdx-1 {
		t.Fatalf("after 'z' from M: zoomIdx=%d want %d", got, chartZoomDefaultIdx-1)
	}

	// 'Z' zooms IN: S(1) → M(2).
	a.tapp.QueueEvent(tcell.NewEventKey(tcell.KeyRune, 'Z', tcell.ModNone))
	time.Sleep(80 * time.Millisecond)
	if got := queueRead(t, a, func() int { return a.chartCanvasV.zoomIdx }); got != chartZoomDefaultIdx {
		t.Fatalf("after 'Z' from S: zoomIdx=%d want %d", got, chartZoomDefaultIdx)
	}

	// 'z' from index 0 (XS) wraps to last (XL).
	a.tapp.QueueUpdate(func() { a.chartCanvasV.zoomIdx = 0 })
	time.Sleep(40 * time.Millisecond)
	a.tapp.QueueEvent(tcell.NewEventKey(tcell.KeyRune, 'z', tcell.ModNone))
	time.Sleep(80 * time.Millisecond)
	if got := queueRead(t, a, func() int { return a.chartCanvasV.zoomIdx }); got != len(chartZooms)-1 {
		t.Fatalf("'z' wrap-around: zoomIdx=%d want %d", got, len(chartZooms)-1)
	}
}

// TestZoomSelectorClick verifies clicking a label on the ZOOM row selects it.
func TestZoomSelectorClick(t *testing.T) {
	stubAssetsAndClient()
	a, cleanup := startSimApp(t)
	defer cleanup()

	a.tapp.QueueUpdateDraw(func() { a.switchTab(tabChart) })
	time.Sleep(150 * time.Millisecond)

	type clickInfo struct {
		row    int
		clickX int
	}
	infoCh := make(chan clickInfo, 1)
	a.tapp.QueueUpdate(func() {
		_, ry, _, _ := a.chartZoomTV.GetRect()
		bx, _, _, _ := a.chartZoomTV.GetInnerRect()
		// Click the XL label (last entry — XL).
		xlIdx := len(chartZooms) - 1
		rng := a.chartZoomHitRanges[xlIdx]
		infoCh <- clickInfo{row: ry, clickX: bx + (rng[0]+rng[1])/2}
	})
	info := <-infoCh

	ev := tcell.NewEventMouse(info.clickX, info.row, tcell.ButtonPrimary, tcell.ModNone)
	a.tapp.QueueEvent(ev)
	a.tapp.QueueEvent(tcell.NewEventMouse(info.clickX, info.row, tcell.ButtonNone, tcell.ModNone))
	time.Sleep(250 * time.Millisecond)

	wantIdx := len(chartZooms) - 1
	if got := queueRead(t, a, func() int { return a.chartCanvasV.zoomIdx }); got != wantIdx {
		t.Fatalf("after clicking XL: zoomIdx=%d want %d", got, wantIdx)
	}
}

// TestZoomXSFitsMoreBars verifies that switching from M to XS makes more
// bars visible at the same chart size. This is the user-visible payoff of
// the zoom control: bigger timeframes look less crowded after zooming out.
func TestZoomXSFitsMoreBars(t *testing.T) {
	const w, h = 80, 22

	c := newChartCanvas()
	c.bars = makeBars(500)
	c.dateFmt = "01/02"
	c.zoomIdx = chartZoomDefaultIdx // M
	drawCanvasOnce(t, c, w, h)
	mediumVisible := c.visibleEnd - c.visibleStart

	c.zoomIdx = 0 // XS
	c.yLocked = false
	drawCanvasOnce(t, c, w, h)
	xsVisible := c.visibleEnd - c.visibleStart

	if xsVisible <= mediumVisible {
		t.Fatalf("XS should fit more bars than M: XS=%d M=%d", xsVisible, mediumVisible)
	}
	// Sanity: XS uses slotW=1, so roughly chartW bars should fit (chartW = 67
	// for w=80 after border/axis/spacer; 500 bars total so we scroll-clip).
	if xsVisible < mediumVisible*3 {
		t.Errorf("XS visible (%d) should be much larger than M (%d)", xsVisible, mediumVisible)
	}
}

// ── Zoom aggregation (XXS / XS) ────────────────────────────────────────────

// TestAggregateBarsMath verifies the OHLCV aggregation rule: open from first,
// close from last, high = max of highs, low = min of lows, volume = sum.
func TestAggregateBarsMath(t *testing.T) {
	in := []Bar{
		{Open: 10, High: 12, Low: 9, Close: 11, Volume: 100},
		{Open: 11, High: 15, Low: 10, Close: 14, Volume: 200},
		{Open: 14, High: 14, Low: 8, Close: 9, Volume: 50},
	}
	got := aggregateBars(in)
	want := Bar{Open: 10, High: 15, Low: 8, Close: 9, Volume: 350}
	if got.Open != want.Open || got.High != want.High || got.Low != want.Low ||
		got.Close != want.Close || got.Volume != want.Volume {
		t.Fatalf("aggregateBars: got %+v want %+v", got, want)
	}
}

// TestZoomAggregationPacksMoreBars verifies that the new XXS (4 bars/cell)
// shows roughly 4× as many raw bars as the prior XS (S in the new layout)
// for the same chart area. This is the whole point of the smaller zooms.
func TestZoomAggregationPacksMoreBars(t *testing.T) {
	const w, h = 80, 22

	mk := func(zi int) int {
		c := newChartCanvas()
		c.zoomIdx = zi
		c.bars = makeBars(2000)
		c.dateFmt = "01/02"
		drawCanvasOnce(t, c, w, h)
		return c.visibleEnd - c.visibleStart
	}

	xxs := mk(0) // XXS, barsPerSlot=4, slotW=1
	xs := mk(1)  // XS,  barsPerSlot=2, slotW=1
	s := mk(2)   // S,   barsPerSlot=1, slotW=2 → fewer raw bars
	m := mk(3)   // M (default)

	if xxs <= xs {
		t.Errorf("XXS should fit more raw bars than XS: XXS=%d XS=%d", xxs, xs)
	}
	if xs <= s {
		t.Errorf("XS should fit more raw bars than S: XS=%d S=%d", xs, s)
	}
	if s <= m {
		t.Errorf("S should fit more raw bars than M: S=%d M=%d", s, m)
	}
	// Sanity bounds: XXS should be roughly 4× XS S-slot count (allowing for chart-width rounding).
	if xxs < 3*s {
		t.Errorf("XXS=%d expected to be much larger than S=%d (≥3×)", xxs, s)
	}
}

// TestZoomAggregateRendersValidOHLC verifies that at an aggregating zoom we
// still draw valid candles — at least one cell with a candle color must
// appear in the chart area, and the y-axis must have refit to encompass them.
func TestZoomAggregateRendersValidOHLC(t *testing.T) {
	c := newChartCanvas()
	c.zoomIdx = 0 // XXS
	c.bars = makeBars(500)
	c.dateFmt = "01/02"

	const w, h = 80, 22
	screen := tcell.NewSimulationScreen("UTF-8")
	if err := screen.Init(); err != nil {
		t.Fatalf("init: %v", err)
	}
	defer screen.Fini()
	screen.SetSize(w, h)
	c.SetRect(0, 0, w, h)
	c.Draw(screen)
	screen.Show()

	bodyCells := 0
	for row := 0; row < h; row++ {
		for col := 0; col < w; col++ {
			ch, _, st, _ := screen.GetContent(col, row)
			fg, _, _ := st.Decompose()
			if ch == '█' && (fg == cGreen || fg == cRed) {
				bodyCells++
			}
		}
	}
	if bodyCells == 0 {
		t.Fatal("XXS chart drew no candle bodies; aggregation broken")
	}
	if c.yMax <= c.yMin {
		t.Errorf("y-axis not initialized: yMin=%.2f yMax=%.2f", c.yMin, c.yMax)
	}
}
