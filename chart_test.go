package main

import (
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
//   - With room to spare (bars*4 fits), wide bodies (slotW=4) are used and
//     every bar is visible.
//   - In the medium range, narrow bodies with a 1-col gap (slotW=2) fit all
//     bars without scrolling.
//   - When bars exceed chartW/2, the chart goes into scroll mode (subset
//     visible). The user always has a gap between candles — never the
//     no-gap "wall of color" they reported.
func TestChartCandleSpacing(t *testing.T) {
	const w, h = 80, 20
	// Canvas has a border, so InnerRect is 78x18.
	// chartW = innerW - rightAxis(10) - 1 = 67.
	cases := []struct {
		bars         int
		expectAllFit bool
	}{
		{bars: 10, expectAllFit: true},   // 10*4=40 <= 67 → wide body, all fit
		{bars: 16, expectAllFit: true},   // 16*4=64 <= 67 → wide body, all fit
		{bars: 25, expectAllFit: true},   // 25*2=50 <= 67 → narrow body+gap, all fit
		{bars: 33, expectAllFit: true},   // 33*2=66 <= 67 → fits at edge
		{bars: 34, expectAllFit: false},  // 34*2=68 > 67 → scroll mode
		{bars: 60, expectAllFit: false},  // scroll
		{bars: 500, expectAllFit: false}, // packed; scroll on
	}

	for _, tc := range cases {
		c := newChartCanvas()
		c.bars = makeBars(tc.bars)
		c.dateFmt = "01/02"
		drawCanvasOnce(t, c, w, h)

		fits := (c.visibleEnd - c.visibleStart) == tc.bars
		if fits != tc.expectAllFit {
			t.Errorf("bars=%d: fits=%v want %v (visible %d..%d)",
				tc.bars, fits, tc.expectAllFit, c.visibleStart, c.visibleEnd)
		}
	}
}
