package main

import (
	"flag"
	"fmt"
	"log"
	"os"
	"strings"
	"sync/atomic"
	"time"

	"github.com/gdamore/tcell/v2"
	"github.com/rivo/tview"
)

// ── Palette (shared with the canonical alpaca-tui build) ─────────────────────

var (
	cBlack  = tcell.ColorBlack
	cOrange = tcell.NewRGBColor(255, 102, 0)
	cCyan   = tcell.NewRGBColor(0, 191, 255)
	cGreen  = tcell.NewRGBColor(0, 255, 65)
	cRed    = tcell.NewRGBColor(255, 49, 49)
	cWhite  = tcell.ColorWhite
	cGray   = tcell.NewRGBColor(85, 85, 85)
	cGray2  = tcell.NewRGBColor(136, 136, 136)
	cDark   = tcell.NewRGBColor(13, 13, 13)
	cYellow = tcell.NewRGBColor(255, 215, 0)
)

var client *AlpacaClient

type app struct {
	tapp *tview.Application

	symField *tview.InputField
	runBtn   *tview.Button
	resetBtn *tview.Button

	pillsTV      *tview.TextView
	resultsTable *tview.Table
	statusTV     *tview.TextView
	titleTV      *tview.TextView

	strategies []Strategy

	// activeTFIdx indexes into timeframes; clicking a pill or pressing [ / ]
	// changes it without refetching data — we re-run strategies over the
	// cached lastBars slice.
	activeTFIdx int
	tfHitRanges [][2]int // [start, end) column ranges into pillsTV per pill

	// Cached bars from the most recent successful fetch, so timeframe switches
	// stay snappy. Cleared when the user types a new ticker or clicks RESET.
	lastBars   []Bar
	lastSymbol string
	lastEnd    time.Time

	// Monotonic counter incremented on every Run so that a slow in-flight
	// backtest cannot clobber a newer one's results.
	runGen atomic.Int64
}

func main() {
	resetFlag := flag.Bool("reset", false, "delete stored credentials and re-run setup")
	flag.Parse()

	if *resetFlag {
		deleteCredentials()
		fmt.Println("Credentials cleared.")
	}

	creds, err := loadCredentials()
	if err != nil || creds.APIKey == "" || creds.APISecret == "" {
		creds = runSetup()
		if creds.APIKey == "" {
			os.Exit(0)
		}
		if err := saveCredentials(creds); err != nil {
			log.Fatalf("save credentials: %v", err)
		}
	}
	client = NewAlpacaClient(creds)
	registerStrategies()
	go loadAssets() // fire-and-forget; powers ticker autocomplete

	a := newApp()
	if err := a.tapp.Run(); err != nil {
		log.Fatal(err)
	}
}

func newApp() *app {
	tview.Styles = tview.Theme{
		PrimitiveBackgroundColor:    cBlack,
		ContrastBackgroundColor:     tcell.NewRGBColor(55, 55, 55),
		MoreContrastBackgroundColor: cBlack,
		BorderColor:                 cOrange,
		TitleColor:                  cOrange,
		GraphicsColor:               cOrange,
		PrimaryTextColor:            cWhite,
		SecondaryTextColor:          cOrange,
		TertiaryTextColor:           cGray2,
		InverseTextColor:            cBlack,
		ContrastSecondaryTextColor:  cCyan,
	}

	// Default timeframe: 1Y is the most common starting view.
	defaultTFIdx := 3
	if defaultTFIdx >= len(timeframes) {
		defaultTFIdx = 0
	}

	a := &app{
		tapp:        tview.NewApplication(),
		strategies:  availableStrategies(),
		activeTFIdx: defaultTFIdx,
	}
	a.build()
	return a
}

func (a *app) build() {
	// ── Title ────────────────────────────────────────────────────────────
	a.titleTV = tview.NewTextView().SetDynamicColors(true).
		SetText("[#FF6600::b] BACKTEST TUI [-]  [#888888]strategy returns at the selected timeframe[-]")
	a.titleTV.SetBackgroundColor(cBlack)

	// ── Inputs row (ticker + buttons; strategy is no longer a dropdown) ──
	a.symField = tview.NewInputField().
		SetLabel(" Ticker  ").
		SetLabelColor(cOrange).
		SetFieldBackgroundColor(cDark).
		SetFieldTextColor(cWhite).
		SetFieldWidth(10)
	a.symField.SetChangedFunc(func(text string) {
		// Typing a new ticker invalidates the cached bars so the next Run
		// always refetches. Empty-text changes (e.g. on clear) are fine.
		if strings.ToUpper(strings.TrimSpace(text)) != a.lastSymbol {
			a.lastBars = nil
		}
	})

	// Autocomplete: prefix-match on tickers plus company-name substring scan.
	// Returns "SYMBOL  Company Name" entries; we strip to the ticker on
	// selection. Until loadAssets() finishes (or if it fails), the cache is
	// empty and filterStocks returns nothing — autocomplete just stays dark.
	a.symField.SetAutocompleteFunc(func(text string) []string {
		upper := strings.ToUpper(strings.TrimSpace(text))
		if upper == "" {
			return nil
		}
		return filterStocks(upper, 10)
	})
	a.symField.SetAutocompletedFunc(func(text string, _ int, source int) bool {
		// Don't disturb the field while the user is just arrowing through
		// the dropdown — only commit on Enter / click / Tab.
		if source == tview.AutocompletedNavigate {
			return false
		}
		sym := strings.ToUpper(strings.Fields(text)[0])
		a.symField.SetText(sym)
		// Enter on a suggestion both fills the ticker AND kicks off the
		// backtest, so the user doesn't have to press Enter twice.
		if source == tview.AutocompletedEnter {
			go a.tapp.QueueUpdateDraw(a.runBacktest)
		}
		return true
	})
	a.symField.SetAutocompleteStyles(
		tcell.NewRGBColor(40, 40, 40),
		tcell.StyleDefault.Foreground(cWhite),
		tcell.StyleDefault.Foreground(cBlack).Background(cCyan).Attributes(tcell.AttrBold),
	)

	a.runBtn = tview.NewButton(" RUN BACKTEST ").
		SetSelectedFunc(a.runBacktest)
	a.runBtn.SetLabelColor(cBlack)
	a.runBtn.SetBackgroundColor(cOrange)

	a.resetBtn = tview.NewButton(" RESET ").
		SetSelectedFunc(a.clearResults)
	a.resetBtn.SetLabelColor(cWhite)
	a.resetBtn.SetBackgroundColor(cDark)

	inputs := tview.NewFlex().
		AddItem(a.symField, 22, 0, true).
		AddItem(a.runBtn, 18, 0, false).
		AddItem(tview.NewBox().SetBackgroundColor(cBlack), 1, 0, false).
		AddItem(a.resetBtn, 10, 0, false).
		AddItem(tview.NewBox().SetBackgroundColor(cBlack), 0, 1, false)
	inputs.SetBackgroundColor(cBlack)

	// ── Timeframe pills ──────────────────────────────────────────────────
	a.pillsTV = tview.NewTextView().
		SetDynamicColors(true).
		SetRegions(false).
		SetWrap(false)
	a.pillsTV.SetBackgroundColor(cBlack)
	a.renderPills()
	a.pillsTV.SetMouseCapture(a.onPillsMouse)

	// ── Results table ────────────────────────────────────────────────────
	a.resultsTable = tview.NewTable().
		SetBorders(false).
		SetSelectable(false, false)
	a.resultsTable.SetBackgroundColor(cBlack)
	a.resultsTable.SetBorder(true).
		SetBorderColor(cOrange).
		SetTitle(" STRATEGIES @ " + timeframes[a.activeTFIdx].Label + " ").
		SetTitleAlign(tview.AlignLeft)
	a.renderEmptyTable()

	// ── Status bar ───────────────────────────────────────────────────────
	a.statusTV = tview.NewTextView().SetDynamicColors(true).
		SetText(" [#888888]Type ticker (autocomplete) → Enter. Pills or [ / ] switch timeframe. R re-runs, Q quits.[-]")
	a.statusTV.SetBackgroundColor(cBlack)

	// ── Layout ───────────────────────────────────────────────────────────
	root := tview.NewFlex().SetDirection(tview.FlexRow).
		AddItem(a.titleTV, 1, 0, false).
		AddItem(inputs, 1, 0, true).
		AddItem(a.pillsTV, 1, 0, false).
		AddItem(a.resultsTable, 0, 1, false).
		AddItem(a.statusTV, 1, 0, false)
	root.SetBackgroundColor(cBlack)

	// ── Keybinds ─────────────────────────────────────────────────────────
	// Symbol field: Enter triggers backtest; Tab moves focus to the RUN button.
	a.symField.SetDoneFunc(func(key tcell.Key) {
		switch key {
		case tcell.KeyEnter:
			a.runBacktest()
		case tcell.KeyTab:
			a.tapp.SetFocus(a.runBtn)
		}
	})

	// Global capture. Three letter rules:
	//   1. Ctrl+C always quits.
	//   2. [ and ] cycle timeframes (when focus does not need them).
	//   3. Q quits and R re-runs, except on InputField / DropDown / List where
	//      letters must reach the widget (so "QQQ", "RBLX", type-ahead, etc.).
	a.tapp.SetInputCapture(func(ev *tcell.EventKey) *tcell.EventKey {
		if ev.Key() == tcell.KeyCtrlC {
			a.tapp.Stop()
			return nil
		}
		r := ev.Rune()

		// Bracket-key timeframe nav. Brackets are not valid ticker characters,
		// so passing them through to the InputField would just beep — safe to
		// intercept globally even when the field has focus.
		if r == '[' {
			a.setTimeframe(a.activeTFIdx - 1)
			return nil
		}
		if r == ']' {
			a.setTimeframe(a.activeTFIdx + 1)
			return nil
		}

		if r != 'q' && r != 'Q' && r != 'r' && r != 'R' {
			return ev
		}
		switch a.tapp.GetFocus().(type) {
		case *tview.InputField, *tview.DropDown, *tview.List:
			return ev // letters are meaningful here — pass through
		}
		if r == 'q' || r == 'Q' {
			a.tapp.Stop()
			return nil
		}
		// r/R: re-run the current ticker
		a.runBacktest()
		return nil
	})

	a.tapp.SetRoot(root, true).EnableMouse(true).SetFocus(a.symField)
}

// renderPills paints the timeframe pill bar and refreshes tfHitRanges. Active
// pill is filled orange; others stay dark. Each pill is two visible blocks of
// padding ("[ 1M ]") so hit-targets are big enough to click reliably.
func (a *app) renderPills() {
	var sb strings.Builder
	hits := make([][2]int, len(timeframes))
	col := 0
	leading := " "
	sb.WriteString(leading)
	col += len(leading)

	for i, tf := range timeframes {
		label := " " + tf.Label + " "
		start := col
		if i == a.activeTFIdx {
			sb.WriteString(fmt.Sprintf("[black:#FF6600:b]%s[-:-:-]", label))
		} else {
			sb.WriteString(fmt.Sprintf("[#888888:#0D0D0D:-]%s[-:-:-]", label))
		}
		col += len(label)
		hits[i] = [2]int{start, col}
		// 1-col gap between pills
		sb.WriteString(" ")
		col++
	}
	a.tfHitRanges = hits
	a.pillsTV.SetText(sb.String())
}

// onPillsMouse turns a click on the pill bar into a setTimeframe call.
// Coordinates from SetMouseCapture are screen-global; subtract the pillsTV
// inner rect to get a column relative to the pill text.
func (a *app) onPillsMouse(action tview.MouseAction, event *tcell.EventMouse) (tview.MouseAction, *tcell.EventMouse) {
	if action != tview.MouseLeftClick || event == nil {
		return action, event
	}
	ix, iy, _, _ := a.pillsTV.GetInnerRect()
	x, y := event.Position()
	if y != iy {
		return action, event
	}
	relX := x - ix
	for i, hr := range a.tfHitRanges {
		if relX >= hr[0] && relX < hr[1] {
			a.setTimeframe(i)
			return tview.MouseConsumed, nil
		}
	}
	return action, event
}

// setTimeframe wraps activeTFIdx changes, then redraws pills + re-runs the
// strategies over the cached lastBars (no network call). When no bars are
// cached yet, it just updates the pill highlight.
func (a *app) setTimeframe(idx int) {
	if idx < 0 {
		idx = len(timeframes) - 1
	}
	if idx >= len(timeframes) {
		idx = 0
	}
	if idx == a.activeTFIdx {
		return
	}
	a.activeTFIdx = idx
	a.renderPills()
	a.resultsTable.SetTitle(" STRATEGIES @ " + timeframes[a.activeTFIdx].Label + " ")
	if len(a.lastBars) > 0 {
		a.recomputeFromCache()
	}
}

// recomputeFromCache runs every strategy over lastBars at the active timeframe
// and re-renders the table. Cheap — no network, no allocations beyond the
// results slice.
func (a *app) recomputeFromCache() {
	results := runStrategiesAtTimeframe(a.lastBars, a.strategies, timeframes[a.activeTFIdx], a.lastEnd)
	a.renderResults(a.lastSymbol, results)
}

// renderEmptyTable lays out the headers and one row per strategy with dashes.
// Called on startup and on Reset.
func (a *app) renderEmptyTable() {
	a.resultsTable.Clear()
	headers := []string{" STRATEGY ", " RETURN % ", " BUY & HOLD % ", " ALPHA ", " TRADES ", " RANGE "}
	for col, h := range headers {
		cell := tview.NewTableCell(h).
			SetTextColor(cOrange).
			SetAttributes(tcell.AttrBold).
			SetSelectable(false).
			SetExpansion(1)
		a.resultsTable.SetCell(0, col, cell)
	}
	for row, s := range a.strategies {
		a.resultsTable.SetCell(row+1, 0, tview.NewTableCell(" "+s.Name()).
			SetTextColor(cCyan).
			SetAttributes(tcell.AttrBold).
			SetExpansion(1))
		for col := 1; col < len(headers); col++ {
			a.resultsTable.SetCell(row+1, col, tview.NewTableCell(" —").
				SetTextColor(cGray2).
				SetExpansion(1))
		}
	}
}

// renderResults fills the table from a completed run. One row per strategy
// in registration order.
func (a *app) renderResults(symbol string, results []Result) {
	a.renderEmptyTable()
	a.resultsTable.SetTitle(fmt.Sprintf(" %s — %s ", strings.ToUpper(symbol), timeframes[a.activeTFIdx].Label))

	for row, r := range results {
		if r.Error != "" {
			a.resultsTable.SetCell(row+1, 1, tview.NewTableCell(" "+r.Error).
				SetTextColor(cGray2).
				SetExpansion(1))
			for c := 2; c < 6; c++ {
				a.resultsTable.SetCell(row+1, c, tview.NewTableCell(" —").
					SetTextColor(cGray2).
					SetExpansion(1))
			}
			continue
		}

		a.resultsTable.SetCell(row+1, 1, tview.NewTableCell(" "+fmtPct(r.ReturnPct)).
			SetTextColor(pnlColor(r.ReturnPct)).
			SetAttributes(tcell.AttrBold).
			SetExpansion(1))
		a.resultsTable.SetCell(row+1, 2, tview.NewTableCell(" "+fmtPct(r.BuyHoldPct)).
			SetTextColor(pnlColor(r.BuyHoldPct)).
			SetExpansion(1))

		alpha := r.ReturnPct - r.BuyHoldPct
		a.resultsTable.SetCell(row+1, 3, tview.NewTableCell(" "+fmtPctSigned(alpha)).
			SetTextColor(pnlColor(alpha)).
			SetExpansion(1))

		a.resultsTable.SetCell(row+1, 4, tview.NewTableCell(fmt.Sprintf(" %d", r.Trades)).
			SetTextColor(cWhite).
			SetExpansion(1))

		a.resultsTable.SetCell(row+1, 5, tview.NewTableCell(" "+
			r.StartTime.Format("2006-01-02")+" → "+r.EndTime.Format("2006-01-02")).
			SetTextColor(cGray2).
			SetExpansion(1))
	}
}

func (a *app) clearResults() {
	a.symField.SetText("")
	a.lastBars = nil
	a.lastSymbol = ""
	a.renderEmptyTable()
	a.resultsTable.SetTitle(" STRATEGIES @ " + timeframes[a.activeTFIdx].Label + " ")
	a.setStatus(" [#888888]Cleared.[-]")
	a.tapp.SetFocus(a.symField)
}

func (a *app) setStatus(msg string) {
	a.statusTV.SetText(msg)
}

// runBacktest fetches 10Y of daily bars for the entered ticker, caches them,
// and renders every registered strategy at the active timeframe. Subsequent
// timeframe changes reuse the cache.
//
// Concurrency: each call bumps runGen; the result-apply goroutine bails if a
// newer run started while it was waiting on the network.
func (a *app) runBacktest() {
	symbol := strings.ToUpper(strings.TrimSpace(a.symField.GetText()))
	if symbol == "" {
		a.setStatus(" [#FF3131]>> Enter a ticker symbol first.[-]")
		return
	}

	// If the user kept the same ticker and we already have bars, skip the
	// network round-trip entirely — just rerun strategies at the current TF.
	if symbol == a.lastSymbol && len(a.lastBars) > 0 {
		a.recomputeFromCache()
		a.setStatus(fmt.Sprintf(" [#00FF41]Re-ran from cache.[-] [#888888]%d bars.[-]", len(a.lastBars)))
		return
	}

	gen := a.runGen.Add(1)
	a.setStatus(fmt.Sprintf(" [#FFD700]Fetching %s daily bars (10Y)…[-]", symbol))

	go func() {
		end := time.Now()
		// Alpaca rejects requests for bars in the last 15 minutes on the free
		// IEX feed (SIP delay), so back end off by ~20 minutes to be safe.
		end = end.Add(-20 * time.Minute)
		start := end.Add(-10*365*24*time.Hour - 7*24*time.Hour) // +1wk slack

		bars, err := client.GetBars(symbol, "1Day", start, end)

		a.tapp.QueueUpdateDraw(func() {
			if gen != a.runGen.Load() {
				return // a newer run superseded us
			}
			if err != nil {
				a.setStatus(fmt.Sprintf(" [#FF3131]>> %s[-]", tview.Escape(err.Error())))
				return
			}
			if len(bars) == 0 {
				a.setStatus(fmt.Sprintf(" [#FF3131]>> No bars returned for %s.[-]", symbol))
				return
			}

			a.lastBars = bars
			a.lastSymbol = symbol
			a.lastEnd = end
			a.recomputeFromCache()
			a.setStatus(fmt.Sprintf(" [#00FF41]Done.[-] [#888888]%d bars, %s → %s.[-]",
				len(bars),
				bars[0].Time.Format("2006-01-02"),
				bars[len(bars)-1].Time.Format("2006-01-02")))
		})
	}()
}

// ── Formatting helpers ──────────────────────────────────────────────────────

func fmtPct(p float64) string {
	return fmt.Sprintf("%7.2f%%", p)
}

func fmtPctSigned(p float64) string {
	sign := "+"
	if p < 0 {
		sign = ""
	}
	return fmt.Sprintf("%s%6.2f%%", sign, p)
}

func pnlColor(p float64) tcell.Color {
	switch {
	case p > 0.001:
		return cGreen
	case p < -0.001:
		return cRed
	default:
		return cWhite
	}
}
