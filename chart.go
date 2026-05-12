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

	// Reserve right axis (price labels) and bottom axis (date labels)
	const rightAxisW = 10
	const bottomAxisH = 1
	chartW := w - rightAxisW - 1
	chartH := h - bottomAxisH - 1
	chartX := x + 1
	chartY := y
	if chartW < 10 || chartH < 4 {
		return
	}

	// Aggregate bars to fit the chart width if we have more candles than columns
	bars := reduceBars(c.bars, chartW)
	n := len(bars)
	if n == 0 {
		return
	}

	minP, maxP := math.Inf(1), math.Inf(-1)
	for _, b := range bars {
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
	// Pad min/max by 2% so candles don't kiss the edges
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

	// Faint horizontal grid lines at 4 equally-spaced rows
	gridStyle := tcell.StyleDefault.Foreground(cGray).Background(cBlack)
	for i := 0; i < 5; i++ {
		gr := chartY + i*(chartH-1)/4
		for cx := chartX; cx < chartX+chartW; cx++ {
			screen.SetContent(cx, gr, '·', nil, gridStyle)
		}
	}

	// Candles — one column per bar
	for i, b := range bars {
		col := chartX + i*chartW/n
		if col >= chartX+chartW {
			col = chartX + chartW - 1
		}
		color := cGreen
		if b.Close < b.Open {
			color = cRed
		}
		st := tcell.StyleDefault.Foreground(color).Background(cBlack)

		hiR := chartY + priceToRow(b.High)
		loR := chartY + priceToRow(b.Low)
		opR := chartY + priceToRow(b.Open)
		clR := chartY + priceToRow(b.Close)

		// Wick
		for r := hiR; r <= loR; r++ {
			screen.SetContent(col, r, '│', nil, st)
		}
		// Body (open→close)
		bTop, bBot := opR, clR
		if bTop > bBot {
			bTop, bBot = bBot, bTop
		}
		for r := bTop; r <= bBot; r++ {
			screen.SetContent(col, r, '█', nil, st)
		}
	}

	// Right-side price axis (5 labels evenly spaced)
	axisX := chartX + chartW
	for i := 0; i < 5; i++ {
		p := maxP - (maxP-minP)*float64(i)/4.0
		row := chartY + i*(chartH-1)/4
		drawString(screen, axisX+1, row, fmt.Sprintf("%-*.2f", rightAxisW-1, p), cGray2)
	}

	// Bottom-row date labels (~5 of them)
	dateRow := chartY + chartH
	labels := 5
	if chartW < 60 {
		labels = 3
	}
	for i := 0; i < labels; i++ {
		idx := i * (n - 1) / (labels - 1)
		col := chartX + idx*chartW/n
		s := bars[idx].Time.Local().Format(c.dateFmt)
		// Center label under its column, clipped to chart bounds
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

// reduceBars aggregates len(bars) bars into at most n output candles using
// OHLC aggregation: open of first, close of last, high=max, low=min.
func reduceBars(bars []Bar, n int) []Bar {
	if n <= 0 || len(bars) <= n {
		return bars
	}
	out := make([]Bar, 0, n)
	step := float64(len(bars)) / float64(n)
	for i := 0; i < n; i++ {
		s := int(float64(i) * step)
		e := int(float64(i+1) * step)
		if e > len(bars) {
			e = len(bars)
		}
		if s >= e {
			continue
		}
		agg := bars[s]
		agg.Close = bars[e-1].Close
		var vol int64
		for j := s; j < e; j++ {
			if bars[j].High > agg.High {
				agg.High = bars[j].High
			}
			if bars[j].Low < agg.Low {
				agg.Low = bars[j].Low
			}
			vol += bars[j].Volume
		}
		agg.Volume = vol
		out = append(out, agg)
	}
	return out
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

	// Range hotkeys on the canvas — d/w/m/t/y/f/x cycle through chartRanges,
	// and [ / ] step prev/next. Letters are intercepted only when the canvas
	// (not the symbol input) has focus, so typing in the symbol field is unaffected.
	a.chartCanvasV.SetInputCapture(func(event *tcell.EventKey) *tcell.EventKey {
		switch event.Key() {
		case tcell.KeyEnter:
			a.tapp.SetFocus(a.chartSymField)
			return nil
		case tcell.KeyTab, tcell.KeyBacktab:
			a.tapp.SetFocus(a.chartSymField)
			return nil
		}
		r := event.Rune()
		// prev/next
		if r == '[' {
			a.cycleChartRange(-1)
			return nil
		}
		if r == ']' {
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
