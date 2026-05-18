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

// emaPeriods is the selectable list for the EMA indicator. Index 0 ("OFF",
// period=0) hides the overlay. The default period is 10 — emaDefaultIdx
// keeps that single source of truth.
var emaPeriods = []struct {
	label  string
	period int
}{
	{"OFF", 0},
	{"5", 5},
	{"10", 10},
	{"20", 20},
	{"50", 50},
	{"100", 100},
	{"200", 200},
}

const emaDefaultIdx = 2 // → period 10

// chartZoom controls candle rendering density. Below S, candles touch (slotW=1).
// To pack even more history, barsPerSlot lets one cell aggregate N raw bars
// into a single OHLC candle — that's how XS and XXS show 2× and 4× more bars
// than S without going below the 1-cell minimum. Higher zoom levels show wider
// candles with breathing room. M is the default and matches the prior "sparse"
// rendering.
type chartZoom struct {
	label       string
	slotW       int // total column width per displayed candle (body + gap)
	bodyW       int // body width within slotW; remainder is gap
	barsPerSlot int // raw bars aggregated into each displayed candle (>=1)
}

var chartZooms = []chartZoom{
	{"XXS", 1, 1, 4}, // 0: NEW — aggregate 4 raw bars per displayed candle
	{"XS", 1, 1, 2},  // 1: NOW SMALLER — aggregate 2 raw bars per displayed candle
	{"S", 2, 1, 1},   // 2: unchanged — 1-col body + 1-col gap
	{"M", 4, 3, 1},   // 3: DEFAULT — 3-col body + 1-col gap
	{"L", 6, 5, 1},   // 4: unchanged
	{"XL", 8, 7, 1},  // 5: unchanged — most zoomed in
}

const chartZoomDefaultIdx = 3 // → M

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

	// Rigid y-axis. yLocked=false on first draw or after a new chart load
	// triggers a fit-to-visible-data; subsequent draws keep yMin/yMax fixed
	// so horizontal scrolling never re-zooms the chart. ↑/↓ pan the range,
	// 0 resets to auto-fit.
	yMin, yMax float64

	// EMA overlay. emaPeriod=0 means hidden. When >0, the canvas computes
	// EMA[i] from c.bars (using period closes) and draws a connected line.
	emaPeriod int
	yLocked   bool

	// Zoom level (index into chartZooms). Controls candle width and gap;
	// changed via z/Z hotkeys or the ZOOM selector row.
	zoomIdx int
}

func newChartCanvas() *chartCanvas {
	c := &chartCanvas{Box: tview.NewBox(), zoomIdx: chartZoomDefaultIdx}
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

	// Candle sizing is driven by the user-controlled zoom level (XS → XL).
	// XS lets you see the most bars at once (candles touch) — useful on
	// long ranges like 5Y/MAX. Higher zoom levels show wider bodies with
	// breathing room. Default is M.
	zi := c.zoomIdx
	if zi < 0 || zi >= len(chartZooms) {
		zi = chartZoomDefaultIdx
	}
	zoom := chartZooms[zi]
	slotW, bodyW, bps := zoom.slotW, zoom.bodyW, zoom.barsPerSlot
	if bps < 1 {
		bps = 1
	}

	// Each displayed candle covers `bps` raw bars (>=1). The visible window is
	// measured in RAW bars so that scrollOffset stays meaningful across zoom
	// changes — pressing z/Z preserves which slice of history you're looking at.
	visibleSlots := chartW / slotW
	if visibleSlots < 1 {
		visibleSlots = 1
	}
	visibleRawBars := visibleSlots * bps
	if visibleRawBars > n {
		visibleRawBars = n
	}

	maxOffset := n - visibleRawBars
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
	startIdx := endIdx - visibleRawBars
	if startIdx < 0 {
		startIdx = 0
	}
	c.visibleStart = startIdx
	c.visibleEnd = endIdx
	// Scroll step is in RAW bars; at higher aggregation, each ←/→ press moves
	// proportionally more history — which feels right (coarser view = bigger jumps).
	c.visibleStep = visibleRawBars / 8
	if c.visibleStep < 1 {
		c.visibleStep = 1
	}

	// Build the list of displayed candles. When bps==1 this is a no-op view
	// of c.bars[startIdx:endIdx]; otherwise each entry aggregates `bps` raw
	// bars into one OHLC candle. visibleRawIdx tracks the LAST raw-bar index
	// inside each slot (used below for EMA / date-label sampling).
	type displayBar struct {
		bar        Bar
		lastRawIdx int
	}
	var visible []displayBar
	if bps == 1 {
		visible = make([]displayBar, endIdx-startIdx)
		for i := range visible {
			visible[i] = displayBar{bar: c.bars[startIdx+i], lastRawIdx: startIdx + i}
		}
	} else {
		visible = make([]displayBar, 0, visibleSlots)
		for s := 0; s < visibleSlots && startIdx+s*bps < endIdx; s++ {
			lo := startIdx + s*bps
			hi := lo + bps
			if hi > endIdx {
				hi = endIdx
			}
			agg := aggregateBars(c.bars[lo:hi])
			visible = append(visible, displayBar{bar: agg, lastRawIdx: hi - 1})
		}
	}

	// Rigid y-axis: compute min/max only on first draw or after a manual reset
	// (yLocked=false). After that, ↑/↓ pan the range and horizontal scrolling
	// does NOT re-zoom. This matches TradingView's default behavior.
	if !c.yLocked && len(visible) > 0 {
		mn, mx := math.Inf(1), math.Inf(-1)
		for _, d := range visible {
			if d.bar.Low < mn {
				mn = d.bar.Low
			}
			if d.bar.High > mx {
				mx = d.bar.High
			}
		}
		if mx <= mn {
			mx = mn + 1
		}
		pad := (mx - mn) * 0.05
		c.yMin = mn - pad
		c.yMax = mx + pad
		c.yLocked = true
	}
	minP, maxP := c.yMin, c.yMax
	if maxP <= minP {
		maxP = minP + 1
	}

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

	// Current-price horizontal dotted line — drawn before candles so candle
	// bodies break the line where they intersect (TradingView style).
	// Uses the LATEST bar's close, not the rightmost visible bar.
	latest := c.bars[n-1]
	latestPrice := latest.Close
	priceLineColor := cYellow
	if latest.Close < latest.Open {
		priceLineColor = cRed
	} else if latest.Close > latest.Open {
		priceLineColor = cGreen
	}
	priceVisible := latestPrice >= minP && latestPrice <= maxP
	var priceRow int
	if priceVisible {
		priceRow = chartY + priceToRow(latestPrice)
		lineStyle := tcell.StyleDefault.Foreground(priceLineColor).Background(cBlack)
		for cx := chartX; cx < chartX+chartW; cx++ {
			if (cx-chartX)%2 == 0 {
				screen.SetContent(cx, priceRow, '─', nil, lineStyle)
			}
		}
	}

	// Candles — TradingView-style: wide body, thin wick centered through it,
	// always a 1-column gap between slots so candles can't visually merge.
	for i, d := range visible {
		b := d.bar
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

	// EMA overlay — drawn AFTER candles so the line sits on top of bodies.
	// Rendered via Braille sub-pixels (2×4 per terminal cell) for smooth
	// diagonals. Computed from the FULL bar history so the EMA at the left
	// edge of the visible window has correctly-accumulated weight.
	if c.emaPeriod > 0 && n >= c.emaPeriod {
		ema := computeEMA(c.bars, c.emaPeriod)
		layer := newBrailleLayer()

		// Sub-pixel space anchored at (chartX, chartY): each terminal column
		// spans 2 sub-pixels horizontally, each row spans 4 vertically.
		priceToSubY := func(p float64) int {
			return int(math.Round((maxP - p) / (maxP - minP) * float64(chartH*4-1)))
		}

		prevX, prevY, havePrev := 0, 0, false
		for i := 0; i < len(visible); i++ {
			v := ema[visible[i].lastRawIdx]
			if math.IsNaN(v) || v < minP || v > maxP {
				havePrev = false
				continue
			}
			subX := (i*slotW + bodyW/2) * 2
			subY := priceToSubY(v)
			if havePrev {
				layer.thickLine(prevX, prevY, subX, subY)
			} else {
				// Match thickLine's 2-sub-pixel weight for isolated points.
				layer.plot(subX, subY)
				layer.plot(subX, subY+1)
			}
			prevX, prevY, havePrev = subX, subY, true
		}

		emaStyle := tcell.StyleDefault.Foreground(cCyan).Background(cBlack).Attributes(tcell.AttrBold)
		layer.renderAt(screen, chartX, chartY, emaStyle)
	}

	// Right-side price axis
	axisX := chartX + chartW
	for i := 0; i < 5; i++ {
		p := maxP - (maxP-minP)*float64(i)/4.0
		row := chartY + i*(chartH-1)/4
		drawString(screen, axisX+1, row, fmt.Sprintf("%-*.2f", rightAxisW-1, p), cGray2)
	}

	// Current-price label box on the right axis at the latest-close row.
	// Drawn last so it overwrites any axis tick that lives at the same row.
	if priceVisible {
		boxFg := cBlack
		boxStyle := tcell.StyleDefault.
			Foreground(boxFg).
			Background(priceLineColor).
			Attributes(tcell.AttrBold)
		label := fmt.Sprintf(" %.2f", latestPrice)
		// Pad to rightAxisW so the box fills the whole axis gutter.
		for i := 0; i < rightAxisW; i++ {
			ch := ' '
			if i < len(label) {
				ch = rune(label[i])
			}
			screen.SetContent(axisX+i, priceRow, ch, nil, boxStyle)
		}
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
			s := visible[idx].bar.Time.Local().Format(c.dateFmt)
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

// aggregateBars compresses a contiguous slice of raw bars into one OHLCV
// candle: open = first.open, close = last.close, high = max of highs, low =
// min of lows, volume = sum, time = last.time (so date labels point at the
// most-recent moment in the aggregate). Used by the densest zoom levels
// (XXS, XS) so a single terminal cell can represent multiple raw bars.
func aggregateBars(bars []Bar) Bar {
	if len(bars) == 0 {
		return Bar{}
	}
	out := Bar{
		Time:   bars[len(bars)-1].Time,
		Open:   bars[0].Open,
		Close:  bars[len(bars)-1].Close,
		High:   bars[0].High,
		Low:    bars[0].Low,
		Volume: 0,
	}
	for _, b := range bars {
		if b.High > out.High {
			out.High = b.High
		}
		if b.Low < out.Low {
			out.Low = b.Low
		}
		out.Volume += b.Volume
	}
	return out
}

// computeEMA returns the EMA of bars' closes at the given period. Result has
// the same length as `bars`; values before the seed are NaN. The seed is the
// SMA of the first `period` closes (TradingView convention); subsequent values
// follow the standard recurrence EMA[i] = close[i]*k + EMA[i-1]*(1-k), with
// k = 2/(period+1).
func computeEMA(bars []Bar, period int) []float64 {
	ema := make([]float64, len(bars))
	for i := range ema {
		ema[i] = math.NaN()
	}
	if period <= 0 || len(bars) < period {
		return ema
	}
	sum := 0.0
	for i := 0; i < period; i++ {
		sum += bars[i].Close
	}
	ema[period-1] = sum / float64(period)
	k := 2.0 / float64(period+1)
	for i := period; i < len(bars); i++ {
		ema[i] = bars[i].Close*k + ema[i-1]*(1-k)
	}
	return ema
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
			go a.loadChart(sym, a.chartRangeIdx, a.chartTFIdx)
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
			go a.loadChart(sym, a.chartRangeIdx, a.chartTFIdx)
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
	a.chartEMATV = tview.NewTextView().SetDynamicColors(true)
	a.chartEMATV.SetBackgroundColor(cBlack)
	a.chartEMATV.SetMouseCapture(func(action tview.MouseAction, event *tcell.EventMouse) (tview.MouseAction, *tcell.EventMouse) {
		if action != tview.MouseLeftClick || event == nil {
			return action, event
		}
		mx, my := event.Position()
		if !a.chartEMATV.InRect(mx, my) {
			return action, event
		}
		bx, _, _, _ := a.chartEMATV.GetInnerRect()
		col := mx - bx
		for i, rng := range a.chartEMAHitRanges {
			if col >= rng[0] && col < rng[1] {
				a.selectChartEMA(i)
				return tview.MouseConsumed, nil
			}
		}
		return action, event
	})
	a.chartZoomTV = tview.NewTextView().SetDynamicColors(true)
	a.chartZoomTV.SetBackgroundColor(cBlack)
	a.chartZoomTV.SetMouseCapture(func(action tview.MouseAction, event *tcell.EventMouse) (tview.MouseAction, *tcell.EventMouse) {
		if action != tview.MouseLeftClick || event == nil {
			return action, event
		}
		mx, my := event.Position()
		if !a.chartZoomTV.InRect(mx, my) {
			return action, event
		}
		bx, _, _, _ := a.chartZoomTV.GetInnerRect()
		col := mx - bx
		for i, rng := range a.chartZoomHitRanges {
			if col >= rng[0] && col < rng[1] {
				a.selectChartZoom(i)
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
	// Default EMA: period 10, on.
	a.chartEMAIdx = emaDefaultIdx
	a.chartCanvasV.emaPeriod = emaPeriods[emaDefaultIdx].period
	a.updateChartRangeBar()
	a.updateChartTFBar()
	a.updateChartEMABar()
	a.updateChartZoomBar()

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
		case tcell.KeyUp:
			a.chartPanY(+0.10) // pan view 10% up (show higher prices)
			return nil
		case tcell.KeyDown:
			a.chartPanY(-0.10)
			return nil
		case tcell.KeyPgUp:
			a.chartScrollBy(+a.chartCanvasV.visibleStep * 8) // page-sized jump back
			return nil
		case tcell.KeyPgDn:
			a.chartScrollBy(-a.chartCanvasV.visibleStep * 8)
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
		case '0':
			// Reset Y-axis to auto-fit on next draw.
			a.chartCanvasV.yLocked = false
			return nil
		case '[':
			a.cycleChartRange(-1)
			return nil
		case ']':
			a.cycleChartRange(+1)
			return nil
		case '{':
			a.cycleChartTF(-1)
			return nil
		case '}':
			a.cycleChartTF(+1)
			return nil
		case '-':
			a.cycleChartTF(-1)
			return nil
		case '=', '+':
			a.cycleChartTF(+1)
			return nil
		case 'e':
			a.cycleChartEMA(+1)
			return nil
		case 'E':
			a.cycleChartEMA(-1)
			return nil
		case 'z':
			// Zoom OUT: smaller candles, more visible at once.
			a.cycleChartZoom(-1)
			return nil
		case 'Z':
			// Zoom IN: wider candles, more breathing room.
			a.cycleChartZoom(+1)
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

	// Indicator + zoom selectors share a row. EMA grows to fill; ZOOM is
	// fixed-width on the right. With 6 zoom labels (XXS S M L XL plus XS)
	// the bar needs ~36 cols to render every label clickable.
	indicatorRow := tview.NewFlex().
		AddItem(a.chartEMATV, 0, 1, false).
		AddItem(a.chartZoomTV, 38, 0, false)

	a.chartPage = tview.NewFlex().SetDirection(tview.FlexRow).
		AddItem(symRow, 1, 0, true).
		AddItem(selectorRow, 1, 0, false).
		AddItem(indicatorRow, 1, 0, false).
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

func (a *termApp) updateChartZoomBar() {
	const prefix = "ZOOM:   "
	const sep = " "
	parts := make([]string, 0, len(chartZooms))
	a.chartZoomHitRanges = make([][2]int, len(chartZooms))
	col := len(prefix)
	for i, z := range chartZooms {
		visible := " " + z.label + " "
		a.chartZoomHitRanges[i] = [2]int{col, col + len(visible)}
		col += len(visible) + len(sep)
		if i == a.chartCanvasV.zoomIdx {
			parts = append(parts, fmt.Sprintf("[#000000:#FF6600:b]%s[-:-:-]", visible))
		} else {
			parts = append(parts, fmt.Sprintf("[#888888]%s[-]", visible))
		}
	}
	a.chartZoomTV.SetText(prefix + strings.Join(parts, sep))
}

func (a *termApp) selectChartZoom(idx int) {
	if idx < 0 || idx >= len(chartZooms) {
		return
	}
	a.chartCanvasV.zoomIdx = idx
	// Force the y-axis to refit on the new candle layout so the chart looks
	// well-framed after a zoom change (e.g., XS → XL shifts what's on screen).
	a.chartCanvasV.yLocked = false
	a.updateChartZoomBar()
}

func (a *termApp) cycleChartZoom(delta int) {
	idx := (a.chartCanvasV.zoomIdx + delta + len(chartZooms)) % len(chartZooms)
	a.selectChartZoom(idx)
}

func (a *termApp) updateChartEMABar() {
	const prefix = "EMA:    "
	const sep = " "
	parts := make([]string, 0, len(emaPeriods))
	a.chartEMAHitRanges = make([][2]int, len(emaPeriods))
	col := len(prefix)
	for i, p := range emaPeriods {
		visible := " " + p.label + " "
		a.chartEMAHitRanges[i] = [2]int{col, col + len(visible)}
		col += len(visible) + len(sep)
		if i == a.chartEMAIdx {
			// Highlight in cyan to match the EMA line color on the canvas.
			parts = append(parts, fmt.Sprintf("[#000000:#00BFFF:b]%s[-:-:-]", visible))
		} else {
			parts = append(parts, fmt.Sprintf("[#888888]%s[-]", visible))
		}
	}
	a.chartEMATV.SetText(prefix + strings.Join(parts, sep))
}

func (a *termApp) selectChartEMA(idx int) {
	if idx < 0 || idx >= len(emaPeriods) {
		return
	}
	a.chartEMAIdx = idx
	a.chartCanvasV.emaPeriod = emaPeriods[idx].period
	a.updateChartEMABar()
	// No reload needed — the EMA is computed from existing bars in Draw().
}

func (a *termApp) cycleChartEMA(delta int) {
	idx := (a.chartEMAIdx + delta + len(emaPeriods)) % len(emaPeriods)
	a.selectChartEMA(idx)
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
		go a.loadChart(sym, idx, a.chartTFIdx)
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
		go a.loadChart(sym, a.chartRangeIdx, idx)
	}
}

func (a *termApp) cycleChartRange(delta int) {
	idx := (a.chartRangeIdx + delta + len(chartRanges)) % len(chartRanges)
	a.selectChartRange(idx)
}

func (a *termApp) cycleChartTF(delta int) {
	idx := (a.chartTFIdx + delta + len(chartTimeframes)) % len(chartTimeframes)
	a.selectChartTF(idx)
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

// chartPanY shifts the rigid y-axis by `frac` of its current span. Positive
// shifts the view UP (showing higher prices), negative shifts it DOWN. The
// y-axis stays at this position through horizontal scrolling until reset
// (press '0') or a new chart is loaded.
func (a *termApp) chartPanY(frac float64) {
	c := a.chartCanvasV
	if !c.yLocked {
		// First-time pan before any draw — let the next Draw compute the
		// initial range; the pan will apply on the draw after.
		return
	}
	span := c.yMax - c.yMin
	if span <= 0 {
		return
	}
	delta := span * frac
	c.yMin += delta
	c.yMax += delta
}

// loadChart fetches bars for the selected symbol/range/timeframe and redraws.
//
// Concurrency: every call bumps `chartLoadGen` and remembers its own
// generation as `myGen`. On completion the goroutine checks the counter
// before writing — if a newer load was started (user clicked another
// range/timeframe), the in-flight result is silently dropped. This is what
// guarantees the chart never "gets stuck" showing stale bars when the user
// changes timeframe faster than the network can respond.
//
// `tfIdx` is passed explicitly (rather than read from a.chartTFIdx on this
// goroutine) so the timeframe captured at call time is what's actually loaded,
// avoiding a data race with the main event loop that may have already moved on.
func (a *termApp) loadChart(symbol string, rangeIdx, tfIdx int) {
	if rangeIdx < 0 || rangeIdx >= len(chartRanges) {
		return
	}
	rg := chartRanges[rangeIdx]
	if tfIdx < 0 || tfIdx >= len(chartTimeframes) {
		tfIdx = rg.defaultTF
	}
	tf := chartTimeframes[tfIdx]

	myGen := a.chartLoadGen.Add(1)

	a.tapp.QueueUpdateDraw(func() {
		a.chartCanvasV.loading = true
		a.chartCanvasV.err = ""
		a.chartCanvasV.symbol = symbol
		a.chartCanvasV.rangeLabel = rg.label
		a.chartCanvasV.dateFmt = rg.dateFmt
		a.chartCanvasV.scrollOffset = 0 // start at the most recent bar
		a.chartCanvasV.yLocked = false  // auto-fit y-axis to the new data
		a.chartCanvasV.SetTitle(fmt.Sprintf(" [#FF6600::b]CHART  %s  ·  %s  ·  %s[-] ", symbol, rg.label, tf.label))
		a.chartStatsTV.SetText("")
	})

	// End slightly in the past — free/paper plans can reject queries for the
	// most recent minute or two of data.
	now := time.Now()
	end := now.Add(-2 * time.Minute)
	start := rg.startTime(now)

	bars, err := client.GetBars(symbol, tf.value, start, end)

	// Early exit: if a newer load already kicked off, discard this result
	// without even queuing a UI update.
	if a.chartLoadGen.Load() != myGen {
		return
	}

	a.tapp.QueueUpdateDraw(func() {
		// Re-check inside the event loop in case yet another load started
		// after the atomic read above and before this callback runs.
		if a.chartLoadGen.Load() != myGen {
			return
		}
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
