package main

import (
	"fmt"
	"math"
	"strings"
	"time"

	"github.com/gdamore/tcell/v2"
	"github.com/rivo/tview"
)

// ── Range definitions ─────────────────────────────────────────────────────────

type chartRange struct {
	label    string        // display label
	hotkey   rune          // letter key to select
	defaultTF int          // index into chartTimeframes used when the range is first picked
	lookback time.Duration // how far back from now; 0 means "use ytdStart"
	ytd      bool          // YTD special-case: start = Jan 1 of current year
	dateFmt  string        // strftime-style for x-axis labels
}

// Indices into chartTimeframes (see below). Kept in sync by hand.
const (
	tf1Min = iota
	tf5Min
	tf15Min
	tf30Min
	tf1Hour
	tf1Day
	tf1Week
	tf1Month
)

var chartRanges = []chartRange{
	{"1D", 'd', tf5Min, 24 * time.Hour, false, "15:04"},
	{"1W", 'w', tf30Min, 7 * 24 * time.Hour, false, "01/02"},
	{"1M", 'm', tf1Day, 31 * 24 * time.Hour, false, "01/02"},
	{"YTD", 't', tf1Day, 0, true, "01/02"},
	{"1Y", 'y', tf1Day, 365 * 24 * time.Hour, false, "01/06"},
	{"5Y", 'f', tf1Week, 5 * 365 * 24 * time.Hour, false, "01/06"},
	{"MAX", 'x', tf1Month, 30 * 365 * 24 * time.Hour, false, "01/06"},
}

type chartTimeframe struct {
	label string // display label (short)
	value string // Alpaca timeframe parameter
}

var chartTimeframes = []chartTimeframe{
	{"1m", "1Min"},
	{"5m", "5Min"},
	{"15m", "15Min"},
	{"30m", "30Min"},
	{"1h", "1Hour"},
	{"1D", "1Day"},
	{"1W", "1Week"},
	{"1M", "1Month"},
}

func (r chartRange) startTime(now time.Time) time.Time {
	if r.ytd {
		return time.Date(now.Year(), 1, 1, 0, 0, 0, 0, now.Location())
	}
	return now.Add(-r.lookback)
}

// ── Canvas ────────────────────────────────────────────────────────────────────

type chartCanvas struct {
	*tview.Box
	bars       []Bar
	symbol     string
	rangeLabel string
	dateFmt    string
	err        string
	loading    bool

	// scrollOffset is how many bars to skip from the right (newest) end.
	// 0 = the newest bar is the rightmost candle. Increased to view older data.
	scrollOffset int

	// visibleStart and visibleEnd are recomputed on each Draw() so input
	// handlers know the current window without re-running the layout math.
	visibleStart int
	visibleEnd   int
	visibleStep  int // step (in bars) to scroll by per ,/. press, set in Draw
}

func newChartCanvas() *chartCanvas {
	c := &chartCanvas{Box: tview.NewBox()}
	c.SetBackgroundColor(cBlack)
	c.SetBorder(true)
	c.SetBorderColor(cOrange)
	c.SetTitleColor(cOrange)
	c.SetTitleAlign(tview.AlignLeft)
	c.SetTitle(" [#FF6600::b]CHART[-] ")
	return c
}

func (c *chartCanvas) Draw(screen tcell.Screen) {
	c.Box.DrawForSubclass(screen, c)
	x, y, w, h := c.GetInnerRect()
	if w < 4 || h < 3 {
		return
	}

	if c.loading {
		drawString(screen, x+2, y+1, "  LOADING...", cYellow)
		return
	}
	if c.err != "" {
		drawString(screen, x+2, y+1, "  ERROR: "+strings.ToUpper(c.err), cRed)
		return
	}
	if len(c.bars) == 0 {
		drawString(screen, x+2, y+1, "  ENTER A SYMBOL ABOVE AND PRESS ENTER  ·  [D]AY [W]EEK [M]ONTH Y[T]D [Y]EAR [F]IVE-YR MA[X]", cGray2)
		return
	}

	// Reserve right axis (price labels) and bottom axis (date + scroll bar).
	const rightAxisW = 10
	const bottomAxisH = 2 // row N-2 = scrollbar, row N-1 = date labels
	chartW := w - rightAxisW - 1
	chartH := h - bottomAxisH - 1
	chartX := x + 1
	chartY := y
	if chartW < 10 || chartH < 4 {
		return
	}

	n := len(c.bars)

	// TradingView-style candle sizing.
	//   slotW = bodyW + gap. We always keep a 1-column gap between candles so
	//   they never visually merge into a solid wall.
	//
	//   sparse (room to spare)  → bodyW 3, gap 1 (3-wide body, wick centered)
	//   medium                  → bodyW 1, gap 1 (1-wide body+wick)
	//   dense                   → bodyW 1, gap 1 with scrolling enabled
	var slotW, bodyW int
	switch {
	case n*4 <= chartW:
		slotW, bodyW = 4, 3
	default:
		slotW, bodyW = 2, 1
	}

	visibleCount := chartW / slotW
	if visibleCount > n {
		visibleCount = n
	}

	// Clamp scroll offset and resolve the window of bars to render.
	maxOffset := n - visibleCount
	if maxOffset < 0 {
		maxOffset = 0
	}
	if c.scrollOffset > maxOffset {
		c.scrollOffset = maxOffset
	}
	if c.scrollOffset < 0 {
		c.scrollOffset = 0
	}
	endIdx := n - c.scrollOffset
	startIdx := endIdx - visibleCount
	if startIdx < 0 {
		startIdx = 0
	}
	visible := c.bars[startIdx:endIdx]
	c.visibleStart = startIdx
	c.visibleEnd = endIdx
	c.visibleStep = visibleCount / 8
	if c.visibleStep < 1 {
		c.visibleStep = 1
	}

	// Min/max over the visible window so the y-axis zooms with the scroll.
	minP, maxP := math.Inf(1), math.Inf(-1)
	for _, b := range visible {
		if b.Low < minP {
			minP = b.Low
		}
		if b.High > maxP {
			maxP = b.High
		}
	}
	if maxP <= minP {
		maxP = minP + 1
	}
	pad := (maxP - minP) * 0.02
	minP -= pad
	maxP += pad

	priceToRow := func(p float64) int {
		r := int(math.Round((maxP - p) / (maxP - minP) * float64(chartH-1)))
		if r < 0 {
			r = 0
		}
		if r > chartH-1 {
			r = chartH - 1
		}
		return r
	}

	// Faint horizontal grid lines at 5 evenly-spaced rows
	gridStyle := tcell.StyleDefault.Foreground(cGray).Background(cBlack)
	for i := 0; i < 5; i++ {
		gr := chartY + i*(chartH-1)/4
		for cx := chartX; cx < chartX+chartW; cx++ {
			screen.SetContent(cx, gr, '·', nil, gridStyle)
		}
	}

	// Candles — TradingView-style: wide body, thin wick centered through it,
	// always a 1-column gap between slots so candles can't visually merge.
	for i, b := range visible {
		slotX := chartX + i*slotW
		if slotX+bodyW > chartX+chartW {
			break
		}
		wickCol := slotX + bodyW/2 // center of the body
		color := cGreen
		if b.Close < b.Open {
			color = cRed
		}
		st := tcell.StyleDefault.Foreground(color).Background(cBlack)

		hiR := chartY + priceToRow(b.High)
		loR := chartY + priceToRow(b.Low)
		opR := chartY + priceToRow(b.Open)
		clR := chartY + priceToRow(b.Close)

		// Wick: single column running from high to low, through the body.
		for r := hiR; r <= loR; r++ {
			screen.SetContent(wickCol, r, '│', nil, st)
		}
		// Body: rectangle bodyW wide, open→close vertically. Body chars overwrite
		// the wick in the open→close region, leaving a true wick only above/below.
		bTop, bBot := opR, clR
		if bTop > bBot {
			bTop, bBot = bBot, bTop
		}
		for bcx := slotX; bcx < slotX+bodyW; bcx++ {
			for r := bTop; r <= bBot; r++ {
				screen.SetContent(bcx, r, '█', nil, st)
			}
		}
	}

	// Right-side price axis
	axisX := chartX + chartW
	for i := 0; i < 5; i++ {
		p := maxP - (maxP-minP)*float64(i)/4.0
		row := chartY + i*(chartH-1)/4
		drawString(screen, axisX+1, row, fmt.Sprintf("%-*.2f", rightAxisW-1, p), cGray2)
	}

	// Scroll-position bar (row chartY+chartH). A faint track with a bright
	// segment showing which bars are visible relative to the whole dataset.
	scrollRow := chartY + chartH
	if n > 0 {
		trackStyle := tcell.StyleDefault.Foreground(cGray).Background(cBlack)
		thumbStyle := tcell.StyleDefault.Foreground(cOrange).Background(cBlack).Attributes(tcell.AttrBold)
		for cx := chartX; cx < chartX+chartW; cx++ {
			screen.SetContent(cx, scrollRow, '─', nil, trackStyle)
		}
		thumbStart := chartX + startIdx*chartW/n
		thumbEnd := chartX + endIdx*chartW/n
		if thumbEnd <= thumbStart {
			thumbEnd = thumbStart + 1
		}
		if thumbEnd > chartX+chartW {
			thumbEnd = chartX + chartW
		}
		for cx := thumbStart; cx < thumbEnd; cx++ {
			screen.SetContent(cx, scrollRow, '━', nil, thumbStyle)
		}
		// Right-side label: visible range / total
		info := fmt.Sprintf("%d-%d/%d", startIdx+1, endIdx, n)
		drawString(screen, axisX+1, scrollRow, info, cGray2)
	}

	// Bottom-row date labels (~5 of them) for the visible window
	dateRow := chartY + chartH + 1
	labels := 5
	if chartW < 60 {
		labels = 3
	}
	vn := len(visible)
	if vn > 0 {
		for i := 0; i < labels; i++ {
			var idx int
			if labels == 1 {
				idx = 0
			} else {
				idx = i * (vn - 1) / (labels - 1)
			}
			col := chartX + idx*slotW + bodyW/2
			s := visible[idx].Time.Local().Format(c.dateFmt)
			start := col - len(s)/2
			if start < chartX {
				start = chartX
			}
			if start+len(s) > chartX+chartW {
				start = chartX + chartW - len(s)
			}
			drawString(screen, start, dateRow, s, cGray2)
		}
	}
}

func drawString(screen tcell.Screen, x, y int, s string, fg tcell.Color) {
	st := tcell.StyleDefault.Foreground(fg).Background(cBlack)
	col := x
	for _, r := range s {
		screen.SetContent(col, y, r, nil, st)
		col++
	}
}

// ── Tab wiring on termApp ─────────────────────────────────────────────────────

func (a *termApp) buildChartTab() {
	a.chartCanvasV = newChartCanvas()

	// Symbol input, mirroring the trade tab's autocomplete behavior.
	a.chartSymField = tview.NewInputField()
	a.chartSymField.
		SetLabel("  SYMBOL  ").
		SetLabelColor(cOrange).
		SetFieldBackgroundColor(cDark).
		SetFieldTextColor(cWhite).
		SetFieldWidth(16)
	a.chartSymField.SetBackgroundColor(cBlack)
	a.chartSymField.SetAutocompleteFunc(func(text string) []string {
		upper := strings.ToUpper(strings.TrimSpace(text))
		if upper == "" {
			a.chartAutoOpen = false
			return nil
		}
		results := filterStocks(upper, 10)
		a.chartAutoOpen = len(results) > 0
		return results
	})
	a.chartSymField.SetAutocompletedFunc(func(text string, _ int, source int) bool {
		if source == tview.AutocompletedNavigate {
			return false
		}
		sym := strings.ToUpper(strings.Fields(text)[0])
		a.chartSymField.SetText(sym)
		a.chartAutoOpen = false
		// Pressing Enter (or clicking) on a suggestion both fills the field
		// AND loads the chart in one step. We're already on the event-loop
		// goroutine here, so call SetFocus directly — QueueUpdateDraw would
		// deadlock waiting for itself.
		if source == tview.AutocompletedEnter || source == tview.AutocompletedClick {
			a.tapp.SetFocus(a.chartCanvasV)
			go a.loadChart(sym, a.chartRangeIdx)
		}
		return true
	})
	a.chartSymField.SetAutocompleteStyles(
		tcell.NewRGBColor(40, 40, 40),
		tcell.StyleDefault.Foreground(cWhite),
		tcell.StyleDefault.Foreground(cBlack).Background(cCyan).Attributes(tcell.AttrBold),
	)
	a.chartSymField.SetDoneFunc(func(key tcell.Key) {
		if key == tcell.KeyEnter {
			sym := strings.ToUpper(strings.TrimSpace(a.chartSymField.GetText()))
			if sym == "" {
				return
			}
			a.chartSymField.SetText(sym)
			go a.loadChart(sym, a.chartRangeIdx)
			a.tapp.SetFocus(a.chartCanvasV)
		}
	})

	a.chartRangeTV = tview.NewTextView().SetDynamicColors(true)
	a.chartRangeTV.SetBackgroundColor(cBlack)
	a.chartRangeTV.SetMouseCapture(func(action tview.MouseAction, event *tcell.EventMouse) (tview.MouseAction, *tcell.EventMouse) {
		if action != tview.MouseLeftClick || event == nil {
			return action, event
		}
		mx, my := event.Position()
		if !a.chartRangeTV.InRect(mx, my) {
			return action, event
		}
		bx, _, _, _ := a.chartRangeTV.GetInnerRect()
		col := mx - bx
		for i, rng := range a.chartRangeHitRanges {
			if col >= rng[0] && col < rng[1] {
				a.selectChartRange(i)
				return tview.MouseConsumed, nil
			}
		}
		return action, event
	})
	a.chartTFTV = tview.NewTextView().SetDynamicColors(true)
	a.chartTFTV.SetBackgroundColor(cBlack)
	a.chartTFTV.SetMouseCapture(func(action tview.MouseAction, event *tcell.EventMouse) (tview.MouseAction, *tcell.EventMouse) {
		if action != tview.MouseLeftClick || event == nil {
			return action, event
		}
		mx, my := event.Position()
		if !a.chartTFTV.InRect(mx, my) {
			return action, event
		}
		bx, _, _, _ := a.chartTFTV.GetInnerRect()
		col := mx - bx
		for i, rng := range a.chartTFHitRanges {
			if col >= rng[0] && col < rng[1] {
				a.selectChartTF(i)
				return tview.MouseConsumed, nil
			}
		}
		return action, event
	})
	a.chartCompanyTV = tview.NewTextView().SetDynamicColors(true)
	a.chartCompanyTV.SetBackgroundColor(cBlack)
	a.chartStatsTV = tview.NewTextView().SetDynamicColors(true)
	a.chartStatsTV.SetBackgroundColor(cBlack)
	// Default the timeframe to the active range's default before first render.
	a.chartTFIdx = chartRanges[a.chartRangeIdx].defaultTF
	a.updateChartRangeBar()
	a.updateChartTFBar()

	// Reflect company name as the user types
	a.chartSymField.SetChangedFunc(func(text string) {
		sym := strings.ToUpper(strings.TrimSpace(text))
		name := getCompanyName(sym)
		if name != "" {
			a.chartCompanyTV.SetText("  [#00BFFF]" + name + "[-]")
		} else {
			a.chartCompanyTV.SetText("")
		}
	})

	// Range + scroll hotkeys on the canvas. Letters are intercepted only when
	// the canvas (not the symbol input) has focus, so typing in the symbol
	// field is unaffected.
	a.chartCanvasV.SetInputCapture(func(event *tcell.EventKey) *tcell.EventKey {
		switch event.Key() {
		case tcell.KeyEnter, tcell.KeyTab, tcell.KeyBacktab:
			a.tapp.SetFocus(a.chartSymField)
			return nil
		case tcell.KeyHome:
			a.chartScrollTo(len(a.chartCanvasV.bars)) // far back; Draw clamps
			return nil
		case tcell.KeyEnd:
			a.chartScrollTo(0)
			return nil
		case tcell.KeyLeft:
			a.chartScrollBy(+a.chartCanvasV.visibleStep)
			return nil
		case tcell.KeyRight:
			a.chartScrollBy(-a.chartCanvasV.visibleStep)
			return nil
		}
		r := event.Rune()
		switch r {
		case ',':
			a.chartScrollBy(+a.chartCanvasV.visibleStep)
			return nil
		case '.':
			a.chartScrollBy(-a.chartCanvasV.visibleStep)
			return nil
		case '<':
			a.chartScrollBy(+a.chartCanvasV.visibleStep * 8) // page-sized jump
			return nil
		case '>':
			a.chartScrollBy(-a.chartCanvasV.visibleStep * 8)
			return nil
		case '[':
			a.cycleChartRange(-1)
			return nil
		case ']':
			a.cycleChartRange(+1)
			return nil
		}
		lower := r
		if lower >= 'A' && lower <= 'Z' {
			lower += 'a' - 'A'
		}
		for i, rg := range chartRanges {
			if rg.hotkey == lower {
				a.selectChartRange(i)
				return nil
			}
		}
		return event
	})

	// Mouse wheel on the canvas scrolls through history.
	a.chartCanvasV.SetMouseCapture(func(action tview.MouseAction, event *tcell.EventMouse) (tview.MouseAction, *tcell.EventMouse) {
		if event == nil {
			return action, event
		}
		mx, my := event.Position()
		if !a.chartCanvasV.InRect(mx, my) {
			return action, event
		}
		switch action {
		case tview.MouseScrollUp:
			a.chartScrollBy(+a.chartCanvasV.visibleStep)
			return tview.MouseConsumed, nil
		case tview.MouseScrollDown:
			a.chartScrollBy(-a.chartCanvasV.visibleStep)
			return tview.MouseConsumed, nil
		}
		return action, event
	})

	symRow := tview.NewFlex().
		AddItem(a.chartSymField, 30, 0, true).
		AddItem(a.chartCompanyTV, 0, 1, false)

	selectorRow := tview.NewFlex().
		AddItem(a.chartTFTV, 60, 0, false).
		AddItem(a.chartRangeTV, 0, 1, false)

	a.chartPage = tview.NewFlex().SetDirection(tview.FlexRow).
		AddItem(symRow, 1, 0, true).
		AddItem(selectorRow, 1, 0, false).
		AddItem(a.chartCanvasV, 0, 1, false).
		AddItem(a.chartStatsTV, 1, 0, false)
}

func (a *termApp) updateChartRangeBar() {
	const prefix = "RANGE: "
	const sep = " "
	parts := make([]string, 0, len(chartRanges))
	a.chartRangeHitRanges = make([][2]int, len(chartRanges))
	col := len(prefix)
	for i, r := range chartRanges {
		visible := " " + r.label + " "
		a.chartRangeHitRanges[i] = [2]int{col, col + len(visible)}
		col += len(visible) + len(sep)
		if i == a.chartRangeIdx {
			parts = append(parts, fmt.Sprintf("[#000000:#FF6600:b]%s[-:-:-]", visible))
		} else {
			parts = append(parts, fmt.Sprintf("[#888888]%s[-]", visible))
		}
	}
	a.chartRangeTV.SetText(prefix + strings.Join(parts, sep))
}

func (a *termApp) updateChartTFBar() {
	const prefix = "CANDLE: "
	const sep = " "
	parts := make([]string, 0, len(chartTimeframes))
	a.chartTFHitRanges = make([][2]int, len(chartTimeframes))
	col := len(prefix)
	for i, tf := range chartTimeframes {
		visible := " " + tf.label + " "
		a.chartTFHitRanges[i] = [2]int{col, col + len(visible)}
		col += len(visible) + len(sep)
		if i == a.chartTFIdx {
			parts = append(parts, fmt.Sprintf("[#000000:#00BFFF:b]%s[-:-:-]", visible))
		} else {
			parts = append(parts, fmt.Sprintf("[#888888]%s[-]", visible))
		}
	}
	a.chartTFTV.SetText(prefix + strings.Join(parts, sep))
}

func (a *termApp) selectChartRange(idx int) {
	if idx < 0 || idx >= len(chartRanges) {
		return
	}
	a.chartRangeIdx = idx
	// Switching range resets the candle interval to that range's sensible default
	// (e.g. 1Y → 1Day). Users can then override via the CANDLE row.
	a.chartTFIdx = chartRanges[idx].defaultTF
	a.updateChartRangeBar()
	a.updateChartTFBar()
	sym := strings.ToUpper(strings.TrimSpace(a.chartSymField.GetText()))
	if sym != "" {
		go a.loadChart(sym, idx)
	}
}

func (a *termApp) selectChartTF(idx int) {
	if idx < 0 || idx >= len(chartTimeframes) {
		return
	}
	a.chartTFIdx = idx
	a.updateChartTFBar()
	sym := strings.ToUpper(strings.TrimSpace(a.chartSymField.GetText()))
	if sym != "" {
		go a.loadChart(sym, a.chartRangeIdx)
	}
}

func (a *termApp) cycleChartRange(delta int) {
	idx := (a.chartRangeIdx + delta + len(chartRanges)) % len(chartRanges)
	a.selectChartRange(idx)
}

// chartScrollBy moves the visible window by delta bars (positive = older,
// negative = newer). Clamping happens in chartCanvas.Draw so this can safely
// pass overshooting values like math.MaxInt.
func (a *termApp) chartScrollBy(delta int) {
	a.chartCanvasV.scrollOffset += delta
	if a.chartCanvasV.scrollOffset < 0 {
		a.chartCanvasV.scrollOffset = 0
	}
}

// chartScrollTo sets an absolute scroll offset (clamped on next Draw).
func (a *termApp) chartScrollTo(offset int) {
	if offset < 0 {
		offset = 0
	}
	a.chartCanvasV.scrollOffset = offset
}

// loadChart fetches bars for the selected symbol and range, then redraws.
func (a *termApp) loadChart(symbol string, rangeIdx int) {
	if rangeIdx < 0 || rangeIdx >= len(chartRanges) {
		return
	}
	rg := chartRanges[rangeIdx]
	tfIdx := a.chartTFIdx
	if tfIdx < 0 || tfIdx >= len(chartTimeframes) {
		tfIdx = rg.defaultTF
	}
	tf := chartTimeframes[tfIdx]

	a.tapp.QueueUpdateDraw(func() {
		a.chartCanvasV.loading = true
		a.chartCanvasV.err = ""
		a.chartCanvasV.symbol = symbol
		a.chartCanvasV.rangeLabel = rg.label
		a.chartCanvasV.dateFmt = rg.dateFmt
		a.chartCanvasV.scrollOffset = 0 // start at the most recent bar
		a.chartCanvasV.SetTitle(fmt.Sprintf(" [#FF6600::b]CHART  %s  ·  %s  ·  %s[-] ", symbol, rg.label, tf.label))
		a.chartStatsTV.SetText("")
	})

	// End slightly in the past — free/paper plans can reject queries for the
	// most recent minute or two of data.
	now := time.Now()
	end := now.Add(-2 * time.Minute)
	start := rg.startTime(now)

	bars, err := client.GetBars(symbol, tf.value, start, end)

	a.tapp.QueueUpdateDraw(func() {
		a.chartCanvasV.loading = false
		if err != nil {
			a.chartCanvasV.err = err.Error()
			a.chartCanvasV.bars = nil
			a.chartStatsTV.SetText("")
			return
		}
		a.chartCanvasV.err = ""
		a.chartCanvasV.bars = bars
		a.updateChartStats(bars)
	})
}

func (a *termApp) updateChartStats(bars []Bar) {
	if len(bars) == 0 {
		a.chartStatsTV.SetText("  [#888888]NO DATA RETURNED FOR THIS RANGE[-]")
		return
	}
	first := bars[0]
	last := bars[len(bars)-1]
	hi, lo := first.High, first.Low
	var vol int64
	for _, b := range bars {
		if b.High > hi {
			hi = b.High
		}
		if b.Low < lo {
			lo = b.Low
		}
		vol += b.Volume
	}
	chg := last.Close - first.Open
	pct := 0.0
	if first.Open > 0 {
		pct = chg / first.Open * 100
	}
	chgColor := "#00FF41"
	sign := "+"
	if chg < 0 {
		chgColor = "#FF3131"
		sign = ""
	}
	a.chartStatsTV.SetText(fmt.Sprintf(
		"  [#FF6600]CLOSE[-] [white]$%.2f[-]   [#FF6600]CHG[-] [%s]%s$%.2f (%s%.2f%%)[-]   [#FF6600]HIGH[-] [white]$%.2f[-]   [#FF6600]LOW[-] [white]$%.2f[-]   [#FF6600]VOL[-] [white]%s[-]   [#FF6600]BARS[-] [white]%d[-]",
		last.Close, chgColor, sign, chg, sign, pct, hi, lo, fmtVolume(vol), len(bars),
	))
}

func fmtVolume(v int64) string {
	switch {
	case v >= 1_000_000_000:
		return fmt.Sprintf("%.2fB", float64(v)/1e9)
	case v >= 1_000_000:
		return fmt.Sprintf("%.2fM", float64(v)/1e6)
	case v >= 1_000:
		return fmt.Sprintf("%.2fK", float64(v)/1e3)
	}
	return fmt.Sprintf("%d", v)
}
