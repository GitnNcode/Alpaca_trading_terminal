package main

import (
	"encoding/csv"
	"flag"
	"fmt"
	"log"
	"os"
	"sort"
	"strconv"
	"strings"
	"time"

	"github.com/gdamore/tcell/v2"
	"github.com/rivo/tview"
)

// ── Bloomberg palette ─────────────────────────────────────────────────────────

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

const (
	tabPositions = 0
	tabTrade     = 1
	tabOrders    = 2
	tabActivity  = 3
	tabChart     = 4
	tabCount     = 5
)

// ── App ───────────────────────────────────────────────────────────────────────

type termApp struct {
	tapp        *tview.Application
	pages       *tview.Pages
	posTable    *tview.Table
	ordersTable *tview.Table
	form        *tview.Form
	statusBar   *tview.TextView
	tabBar      *tview.TextView
	resultTV    *tview.TextView
	indicatorTV   *tview.TextView
	activityTable *tview.Table
	companyTV     *tview.TextView

	symField   *tview.InputField
	qtyField   *tview.InputField
	priceField *tview.InputField
	actionDD   *tview.DropDown
	typeDD     *tview.DropDown

	// Chart tab
	chartPage      *tview.Flex
	chartSymField  *tview.InputField
	chartCompanyTV *tview.TextView
	chartRangeTV   *tview.TextView
	chartTFTV      *tview.TextView
	chartStatsTV   *tview.TextView
	chartCanvasV   *chartCanvas
	chartRangeIdx  int
	chartTFIdx     int
	chartAutoOpen  bool

	activeTab        int
	account          Account
	confirmActive    bool
	autocompleteOpen bool // true while symField autocomplete list is visible
	stopCh           chan struct{}

	// Hit-test ranges for clickable text bars: each entry is [startCol, endCol).
	tabHitRanges        [][2]int
	chartRangeHitRanges [][2]int
	chartTFHitRanges    [][2]int
}

func newTermApp() *termApp {
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

	a := &termApp{
		tapp:      tview.NewApplication(),
		activeTab: tabPositions,
	}
	a.tapp.EnableMouse(true)
	a.build()
	return a
}

// ── Build ─────────────────────────────────────────────────────────────────────

func (a *termApp) build() {
	headerLeft := a.makeHeader()
	a.indicatorTV = tview.NewTextView().SetDynamicColors(true).SetTextAlign(tview.AlignRight)
	a.indicatorTV.SetBackgroundColor(cBlack)
	header := tview.NewFlex().
		AddItem(headerLeft, 0, 1, false).
		AddItem(a.indicatorTV, 28, 0, false)
	header.SetBorder(true)
	header.SetBorderColor(cOrange)

	a.tabBar = a.makeTabBar()
	a.buildPositionsTable()
	a.buildOrdersTable()
	a.buildActivityTable()
	a.buildTradeForm()
	a.buildChartTab()
	a.resultTV = tview.NewTextView().SetDynamicColors(true)
	a.resultTV.SetBackgroundColor(cBlack)

	tradePage := tview.NewFlex().SetDirection(tview.FlexRow).
		AddItem(a.form, 0, 1, true).
		AddItem(a.companyTV, 1, 0, false).
		AddItem(a.resultTV, 2, 0, false)

	a.pages = tview.NewPages().
		AddPage("positions", a.posTable, true, true).
		AddPage("trade", tradePage, true, false).
		AddPage("orders", a.ordersTable, true, false).
		AddPage("activity", a.activityTable, true, false).
		AddPage("chart", a.chartPage, true, false)

	a.statusBar = tview.NewTextView().SetDynamicColors(true)
	a.statusBar.SetBackgroundColor(cDark)
	a.refreshStatus()

	root := tview.NewFlex().SetDirection(tview.FlexRow).
		AddItem(header, 2, 0, false).
		AddItem(a.tabBar, 1, 0, false).
		AddItem(a.pages, 0, 1, true).
		AddItem(a.statusBar, 1, 0, false)

	a.tapp.SetRoot(root, true).SetInputCapture(a.globalKeys)
}

func (a *termApp) makeHeader() *tview.TextView {
	env := "[#00BFFF]PAPER[-]"
	if !strings.Contains(client.BaseURL, "paper") {
		env = "[#FF3131]LIVE[-]"
	}
	tv := tview.NewTextView().
		SetDynamicColors(true).
		SetText(fmt.Sprintf(
			"[#FF6600::b] ALPACA TRADING TERMINAL [-]  |  %s  |  [#555555] %s [-]",
			env, client.BaseURL,
		))
	tv.SetBackgroundColor(cBlack)
	return tv
}

func (a *termApp) makeTabBar() *tview.TextView {
	tv := tview.NewTextView().SetDynamicColors(true)
	tv.SetBackgroundColor(cBlack)
	a.updateTabBar(tv)
	// Click-to-switch: each tab label occupies a known visible column range,
	// recorded by updateTabBar in tabHitRanges.
	// NOTE: tview's Box.WrapMouseHandler invokes our capture BEFORE the inner
	// MouseHandler's InRect check, and it's called for any event Flex bubbles
	// through this primitive — so we must InRect-check ourselves, otherwise
	// clicks elsewhere on screen would match column ranges here.
	// Consuming requires returning tview.MouseConsumed (returning the original
	// action keeps consumed=false and the click bubbles to siblings).
	tv.SetMouseCapture(func(action tview.MouseAction, event *tcell.EventMouse) (tview.MouseAction, *tcell.EventMouse) {
		if action != tview.MouseLeftClick || event == nil {
			return action, event
		}
		mx, my := event.Position()
		if !tv.InRect(mx, my) {
			return action, event
		}
		bx, _, _, _ := tv.GetInnerRect()
		col := mx - bx
		for i, rng := range a.tabHitRanges {
			if col >= rng[0] && col < rng[1] {
				a.switchTab(i)
				return tview.MouseConsumed, nil
			}
		}
		return action, event
	})
	return tv
}

func (a *termApp) updateTabBar(tv *tview.TextView) {
	// Visible labels (color tags don't affect the rendered column width).
	labels := []string{
		"  [1] POSITIONS  ",
		"  [2] TRADE  ",
		"  [3] ORDERS  ",
		"  [4] ACTIVITY  ",
		"  [5] CHART  ",
	}
	const sep = " "

	a.tabHitRanges = make([][2]int, len(labels))
	col := 0
	for i, lbl := range labels {
		a.tabHitRanges[i] = [2]int{col, col + len(lbl)}
		col += len(lbl) + len(sep)
	}

	color := func(i int, lbl string) string {
		if i == a.activeTab {
			return "[#000000:#FF6600:b]" + lbl + "[-:-:-]"
		}
		return "[#888888]" + lbl + "[-]"
	}
	parts := make([]string, len(labels))
	for i, lbl := range labels {
		parts[i] = color(i, lbl)
	}
	tv.SetText(strings.Join(parts, sep))
}

func (a *termApp) buildPositionsTable() {
	a.posTable = tview.NewTable().
		SetBorders(false).
		SetFixed(1, 0).
		SetSelectable(true, false).
		SetSelectedStyle(tcell.StyleDefault.
			Foreground(cBlack).
			Background(cOrange).
			Attributes(tcell.AttrBold))

	a.posTable.SetBackgroundColor(cBlack)
	a.posTable.SetBorder(true)
	a.posTable.SetBorderColor(cOrange)
	a.posTable.SetTitle(" [#FF6600::b]OPEN POSITIONS[-] ")
	a.posTable.SetTitleAlign(tview.AlignLeft)

	for i, h := range []string{"SYMBOL", "QTY", "AVG ENTRY", "CUR PRICE", "MKT VALUE", "P&L", "P&L %", "SIDE"} {
		a.posTable.SetCell(0, i,
			tview.NewTableCell(" "+h+" ").
				SetTextColor(cOrange).
				SetAttributes(tcell.AttrBold).
				SetSelectable(false),
		)
	}
	a.posTable.SetCell(1, 0,
		tview.NewTableCell("  LOADING...").SetTextColor(cGray2).SetSelectable(false),
	)

	// Double-click a position row to pre-fill the trade form with a SELL of
	// that symbol+qty, then jump to the TRADE tab.
	a.posTable.SetMouseCapture(func(action tview.MouseAction, event *tcell.EventMouse) (tview.MouseAction, *tcell.EventMouse) {
		if action != tview.MouseLeftDoubleClick || event == nil {
			return action, event
		}
		x, y := event.Position()
		if !a.posTable.InRect(x, y) {
			return action, event
		}
		row, _ := a.posTable.CellAt(x, y)
		if row < 1 {
			return action, event
		}
		symbol := strings.TrimSpace(a.posTable.GetCell(row, 0).Text)
		qty := strings.TrimSpace(a.posTable.GetCell(row, 1).Text)
		if symbol == "" {
			return action, event
		}
		a.actionDD.SetCurrentOption(1) // SELL
		a.typeDD.SetCurrentOption(0)   // MARKET
		a.symField.SetText(symbol)
		a.qtyField.SetText(qty)
		a.priceField.SetText("")
		a.switchTab(tabTrade)
		return tview.MouseConsumed, nil
	})
}

func (a *termApp) buildOrdersTable() {
	a.ordersTable = tview.NewTable().
		SetBorders(false).
		SetFixed(1, 0).
		SetSelectable(true, false).
		SetSelectedStyle(tcell.StyleDefault.
			Foreground(cBlack).
			Background(cOrange).
			Attributes(tcell.AttrBold))

	a.ordersTable.SetBackgroundColor(cBlack)
	a.ordersTable.SetBorder(true)
	a.ordersTable.SetBorderColor(cOrange)
	a.ordersTable.SetTitle(" [#FF6600::b]PENDING ORDERS[-] ")
	a.ordersTable.SetTitleAlign(tview.AlignLeft)

	for i, h := range []string{"ORDER ID", "SYMBOL", "SIDE", "TYPE", "QTY", "FILLED", "LIMIT PX", "STATUS", "CREATED"} {
		a.ordersTable.SetCell(0, i,
			tview.NewTableCell(" "+h+" ").
				SetTextColor(cOrange).
				SetAttributes(tcell.AttrBold).
				SetSelectable(false),
		)
	}
	a.ordersTable.SetCell(1, 0,
		tview.NewTableCell("  LOADING...").SetTextColor(cGray2).SetSelectable(false),
	)

	// X or Delete cancels the selected order
	a.ordersTable.SetInputCapture(func(event *tcell.EventKey) *tcell.EventKey {
		if event.Key() == tcell.KeyDelete || event.Rune() == 'x' || event.Rune() == 'X' {
			row, _ := a.ordersTable.GetSelection()
			if row < 1 {
				return nil
			}
			ref := a.ordersTable.GetCell(row, 0).GetReference()
			if ref == nil {
				return nil
			}
			orderID := ref.(string)
			symbol := strings.TrimSpace(a.ordersTable.GetCell(row, 1).Text)
			a.showCancelModal(orderID, symbol)
			return nil
		}
		return event
	})

	// Right-click also triggers the cancel modal for the row under the cursor.
	a.ordersTable.SetMouseCapture(func(action tview.MouseAction, event *tcell.EventMouse) (tview.MouseAction, *tcell.EventMouse) {
		if action != tview.MouseRightClick || event == nil {
			return action, event
		}
		x, y := event.Position()
		if !a.ordersTable.InRect(x, y) {
			return action, event
		}
		row, _ := a.ordersTable.CellAt(x, y)
		if row < 1 {
			return action, event
		}
		ref := a.ordersTable.GetCell(row, 0).GetReference()
		if ref == nil {
			return action, event
		}
		orderID, ok := ref.(string)
		if !ok {
			return action, event
		}
		symbol := strings.TrimSpace(a.ordersTable.GetCell(row, 1).Text)
		a.ordersTable.Select(row, 0)
		a.showCancelModal(orderID, symbol)
		return tview.MouseConsumed, nil
	})
}

func (a *termApp) buildActivityTable() {
	a.activityTable = tview.NewTable().
		SetBorders(false).
		SetFixed(1, 0).
		SetSelectable(true, false).
		SetSelectedStyle(tcell.StyleDefault.
			Foreground(cBlack).
			Background(cOrange).
			Attributes(tcell.AttrBold))

	a.activityTable.SetBackgroundColor(cBlack)
	a.activityTable.SetBorder(true)
	a.activityTable.SetBorderColor(cOrange)
	a.activityTable.SetTitle(" [#FF6600::b]ACCOUNT ACTIVITY  (last 100 events + closed orders)[-] ")
	a.activityTable.SetTitleAlign(tview.AlignLeft)

	for i, h := range []string{"TIME", "TYPE", "SYMBOL", "DIR", "QTY", "PRICE", "AMOUNT", "DETAIL"} {
		a.activityTable.SetCell(0, i,
			tview.NewTableCell(" "+h+" ").
				SetTextColor(cOrange).
				SetAttributes(tcell.AttrBold).
				SetSelectable(false),
		)
	}
	a.activityTable.SetCell(1, 0,
		tview.NewTableCell("  LOADING...").SetTextColor(cGray2).SetSelectable(false),
	)
}

func (a *termApp) buildTradeForm() {
	// Company name display line (shown below SYMBOL field after selection)
	a.companyTV = tview.NewTextView().SetDynamicColors(true)
	a.companyTV.SetBackgroundColor(cBlack)

	a.form = tview.NewForm()
	a.form.SetBackgroundColor(cBlack)
	a.form.
		SetFieldBackgroundColor(cDark).
		SetFieldTextColor(cWhite).
		SetLabelColor(cOrange).
		SetButtonBackgroundColor(cOrange).
		SetButtonTextColor(cBlack).
		SetBorder(true).
		SetBorderColor(cOrange).
		SetTitle(" [#FF6600::b]NEW ORDER[-] ").
		SetTitleAlign(tview.AlignLeft)

	a.form.
		AddDropDown("  ACTION     ", []string{"BUY", "SELL"}, 0, nil).
		AddDropDown("  TYPE       ", []string{"MARKET", "LIMIT"}, 0, nil).
		AddInputField("  SYMBOL     ", "", 28, nil, nil).
		AddInputField("  QUANTITY   ", "", 28, nil, nil).
		AddInputField("  LIMIT PX   ", "", 28, nil, nil).
		AddButton("   PLACE ORDER   ", a.onSubmit).
		AddButton("   CLEAR   ", a.onClear)

	a.actionDD = a.form.GetFormItem(0).(*tview.DropDown)
	a.typeDD = a.form.GetFormItem(1).(*tview.DropDown)
	a.symField = a.form.GetFormItem(2).(*tview.InputField)
	a.qtyField = a.form.GetFormItem(3).(*tview.InputField)
	a.priceField = a.form.GetFormItem(4).(*tview.InputField)

	// Distinct popup list background so it fully overwrites underlying text
	listBg := tcell.NewRGBColor(40, 40, 40)
	listStyle := tcell.StyleDefault.Foreground(cWhite).Background(listBg)
	selectedStyle := tcell.StyleDefault.Foreground(cBlack).Background(cCyan).Attributes(tcell.AttrBold)
	a.actionDD.SetListStyles(listStyle, selectedStyle)
	a.typeDD.SetListStyles(listStyle, selectedStyle)

	a.typeDD.SetSelectedFunc(func(option string, _ int) {
		if option == "LIMIT" {
			a.priceField.SetPlaceholder("required")
		} else {
			a.priceField.SetPlaceholder("not used for market orders")
		}
	})
	a.priceField.SetPlaceholder("not used for market orders")
	a.priceField.SetPlaceholderStyle(tcell.StyleDefault.Foreground(cGray))

	// Autocomplete: search by ticker prefix and company name substring.
	// Track open/closed state so globalKeys can decide whether Down should
	// navigate the list (open) or move to the next form field (closed).
	a.symField.SetAutocompleteFunc(func(text string) []string {
		upper := strings.ToUpper(strings.TrimSpace(text))
		if upper == "" {
			a.autocompleteOpen = false
			return nil
		}
		results := filterStocks(upper, 10)
		a.autocompleteOpen = len(results) > 0
		return results
	})
	a.symField.SetAutocompletedFunc(func(text string, _ int, source int) bool {
		if source != tview.AutocompletedNavigate {
			sym := strings.ToUpper(strings.Fields(text)[0])
			a.symField.SetText(sym)
			a.autocompleteOpen = false // list just closed via selection
			return true
		}
		return false
	})
	a.symField.SetAutocompleteStyles(
		listBg,
		tcell.StyleDefault.Foreground(cWhite),
		tcell.StyleDefault.Foreground(cBlack).Background(cCyan).Attributes(tcell.AttrBold),
	)
	// Show company name in blue below the symbol field on any text change
	a.symField.SetChangedFunc(func(text string) {
		sym := strings.ToUpper(strings.TrimSpace(text))
		name := getCompanyName(sym)
		if name != "" {
			a.companyTV.SetText("  [#00BFFF]" + name + "[-]")
		} else {
			a.companyTV.SetText("")
		}
	})

	// Up/down on dropdowns navigate to prev/next field.
	// Per-DropDown (not form-level) so InputField Up/Down still reach the
	// autocomplete list when it is visible.
	navDD := func(dd *tview.DropDown) {
		dd.SetInputCapture(func(event *tcell.EventKey) *tcell.EventKey {
			switch event.Key() {
			case tcell.KeyDown:
				return tcell.NewEventKey(tcell.KeyTab, 0, tcell.ModNone)
			case tcell.KeyUp:
				return tcell.NewEventKey(tcell.KeyBacktab, 0, tcell.ModNone)
			}
			return event
		})
	}
	navDD(a.actionDD)
	navDD(a.typeDD)
}

// ── Keys ──────────────────────────────────────────────────────────────────────

func (a *termApp) globalKeys(event *tcell.EventKey) *tcell.EventKey {
	// Pass all events through while confirmation modal is active
	if a.confirmActive {
		return event
	}

	// Fix 5: left/right arrows switch tabs (but not when cursor is in a text field)
	if event.Key() == tcell.KeyLeft || event.Key() == tcell.KeyRight {
		if _, isInput := a.tapp.GetFocus().(*tview.InputField); !isInput {
			if event.Key() == tcell.KeyLeft {
				t := a.activeTab - 1
				if t < 0 {
					t = tabCount - 1
				}
				a.switchTab(t)
			} else {
				t := a.activeTab + 1
				if t >= tabCount {
					t = 0
				}
				a.switchTab(t)
			}
			return nil
		}
	}

	switch a.tapp.GetFocus().(type) {
	case *tview.InputField:
		focus := a.tapp.GetFocus()
		// Chart tab's symbol input: Escape moves focus to the chart canvas
		// (where range hotkeys are intercepted); Down/Up are left alone so the
		// autocomplete list can scroll.
		if focus == a.chartSymField {
			if event.Key() == tcell.KeyEscape {
				a.tapp.SetFocus(a.chartCanvasV)
				return nil
			}
			return event
		}
		if event.Key() == tcell.KeyEscape {
			a.tapp.SetFocus(a.actionDD)
			return nil
		}
		// Down/Up navigate to next/prev field — UNLESS the autocomplete list in
		// symField is currently open (in that case arrows must scroll the list).
		symListOpen := focus == a.symField && a.autocompleteOpen
		if !symListOpen {
			switch event.Key() {
			case tcell.KeyDown:
				return tcell.NewEventKey(tcell.KeyTab, 0, tcell.ModNone)
			case tcell.KeyUp:
				return tcell.NewEventKey(tcell.KeyBacktab, 0, tcell.ModNone)
			}
		}
		return event
	case *tview.DropDown:
		return event
	}

	switch event.Key() {
	case tcell.KeyEscape:
		a.tapp.Stop()
		return nil
	case tcell.KeyF5:
		go a.refresh()
		return nil
	}

	switch event.Rune() {
	case '1':
		a.switchTab(tabPositions)
		return nil
	case '2':
		a.switchTab(tabTrade)
		return nil
	case '3':
		a.switchTab(tabOrders)
		return nil
	case '4':
		a.switchTab(tabActivity)
		return nil
	case '5':
		a.switchTab(tabChart)
		return nil
	case 'r', 'R':
		go a.refresh()
		return nil
	case 'q', 'Q':
		a.tapp.Stop()
		return nil
	}

	return event
}

func (a *termApp) switchTab(tab int) {
	a.activeTab = tab
	a.updateTabBar(a.tabBar)
	a.refreshStatus()
	switch tab {
	case tabPositions:
		a.pages.SwitchToPage("positions")
		a.tapp.SetFocus(a.posTable)
	case tabTrade:
		a.pages.SwitchToPage("trade")
		a.tapp.SetFocus(a.actionDD) // land on the first logical field, not the ticker
	case tabOrders:
		a.pages.SwitchToPage("orders")
		a.tapp.SetFocus(a.ordersTable)
	case tabActivity:
		a.pages.SwitchToPage("activity")
		a.tapp.SetFocus(a.activityTable)
	case tabChart:
		a.pages.SwitchToPage("chart")
		a.tapp.SetFocus(a.chartSymField)
	}
}

// ── Data ──────────────────────────────────────────────────────────────────────

func (a *termApp) refresh() {
	positions, posErr := client.GetPositions()
	a.tapp.QueueUpdateDraw(func() {
		if posErr != nil {
			a.setResult("[#FF3131]FETCH ERROR: " + strings.ToUpper(posErr.Error()) + "[-]")
		} else {
			a.loadPositions(positions)
		}
	})

	account, err := client.GetAccount()
	if err == nil {
		a.tapp.QueueUpdateDraw(func() {
			a.account = account
			a.refreshStatus()
		})
	}

	orders, ordErr := client.GetOrders()
	a.tapp.QueueUpdateDraw(func() {
		a.loadOrders(orders, ordErr)
	})

	activities, actErr := client.GetActivities()
	closedOrders, ordErr := client.GetClosedOrders()
	a.tapp.QueueUpdateDraw(func() {
		a.loadActivities(activities, closedOrders, actErr, ordErr)
	})
}

func (a *termApp) loadPositions(positions []Position) {
	a.posTable.Clear()

	for i, h := range []string{"SYMBOL", "QTY", "AVG ENTRY", "CUR PRICE", "MKT VALUE", "P&L", "P&L %", "SIDE"} {
		a.posTable.SetCell(0, i,
			tview.NewTableCell(" "+h+" ").
				SetTextColor(cOrange).
				SetAttributes(tcell.AttrBold).
				SetSelectable(false),
		)
	}

	if len(positions) == 0 {
		a.posTable.SetCell(1, 0,
			tview.NewTableCell("  NO OPEN POSITIONS — PRESS R TO REFRESH").
				SetTextColor(cGray2).SetSelectable(false),
		)
		return
	}

	for i, p := range positions {
		pl, _ := strconv.ParseFloat(p.UnrealizedPL, 64)
		plpc, _ := strconv.ParseFloat(p.UnrealizedPLPC, 64)

		plColor := cGreen
		plStr := fmt.Sprintf("+$%.2f", pl)
		plPctStr := fmt.Sprintf("+%.2f%%", plpc*100)
		if pl < 0 {
			plColor = cRed
			plStr = fmt.Sprintf("-$%.2f", -pl)
			plPctStr = fmt.Sprintf("%.2f%%", plpc*100)
		}

		r := i + 1
		cell := func(text string, color tcell.Color, attr tcell.AttrMask) *tview.TableCell {
			return tview.NewTableCell(" " + text + " ").SetTextColor(color).SetAttributes(attr)
		}
		a.posTable.SetCell(r, 0, cell(p.Symbol, cWhite, tcell.AttrBold))
		a.posTable.SetCell(r, 1, cell(p.Qty, cWhite, 0))
		a.posTable.SetCell(r, 2, cell("$"+fmtPrice(p.AvgEntryPrice), cWhite, 0))
		a.posTable.SetCell(r, 3, cell("$"+fmtPrice(p.CurrentPrice), cWhite, 0))
		a.posTable.SetCell(r, 4, cell("$"+fmtPrice(p.MarketValue), cWhite, 0))
		a.posTable.SetCell(r, 5, cell(plStr, plColor, tcell.AttrBold))
		a.posTable.SetCell(r, 6, cell(plPctStr, plColor, 0))
		a.posTable.SetCell(r, 7, cell(strings.ToUpper(p.Side), cCyan, 0))
	}
}

func (a *termApp) loadOrders(orders []Order, fetchErr error) {
	a.ordersTable.Clear()

	for i, h := range []string{"ORDER ID", "SYMBOL", "SIDE", "TYPE", "QTY", "FILLED", "LIMIT PX", "STATUS", "CREATED"} {
		a.ordersTable.SetCell(0, i,
			tview.NewTableCell(" "+h+" ").
				SetTextColor(cOrange).
				SetAttributes(tcell.AttrBold).
				SetSelectable(false),
		)
	}

	if fetchErr != nil {
		a.ordersTable.SetCell(1, 0,
			tview.NewTableCell("  ERROR: "+strings.ToUpper(fetchErr.Error())).
				SetTextColor(cRed).SetSelectable(false),
		)
		return
	}

	if len(orders) == 0 {
		a.ordersTable.SetCell(1, 0,
			tview.NewTableCell("  NO PENDING ORDERS — PRESS R TO REFRESH").
				SetTextColor(cGray2).SetSelectable(false),
		)
		return
	}

	for i, o := range orders {
		id := o.ID
		if len(id) > 8 {
			id = id[:8]
		}

		sideColor := cCyan
		if strings.EqualFold(o.Side, "sell") {
			sideColor = cRed
		}

		statusColor := cYellow
		switch strings.ToLower(o.Status) {
		case "filled":
			statusColor = cGreen
		case "partially_filled":
			statusColor = cCyan
		case "canceled", "expired", "rejected":
			statusColor = cRed
		}

		limitStr := "—"
		if o.LimitPrice != "" && o.LimitPrice != "0" {
			limitStr = "$" + fmtPrice(o.LimitPrice)
		}

		r := i + 1
		cell := func(text string, color tcell.Color, attr tcell.AttrMask) *tview.TableCell {
			return tview.NewTableCell(" " + text + " ").SetTextColor(color).SetAttributes(attr)
		}
		a.ordersTable.SetCell(r, 0, cell(id, cGray2, 0).SetReference(o.ID))
		a.ordersTable.SetCell(r, 1, cell(o.Symbol, cWhite, tcell.AttrBold))
		a.ordersTable.SetCell(r, 2, cell(strings.ToUpper(o.Side), sideColor, tcell.AttrBold))
		a.ordersTable.SetCell(r, 3, cell(strings.ToUpper(o.Type), cWhite, 0))
		a.ordersTable.SetCell(r, 4, cell(o.Qty, cWhite, 0))
		a.ordersTable.SetCell(r, 5, cell(o.FilledQty, cGray2, 0))
		a.ordersTable.SetCell(r, 6, cell(limitStr, cWhite, 0))
		a.ordersTable.SetCell(r, 7, cell(strings.ToUpper(o.Status), statusColor, tcell.AttrBold))
		a.ordersTable.SetCell(r, 8, cell(o.CreatedAt.Local().Format("15:04:05"), cGray2, 0))
	}
}

// actRow is a unified display row for the activity table.
type actRow struct {
	when    time.Time
	typeStr string
	symbol  string
	dir     string
	qty     string
	price   string
	amount  string
	detail  string
	typeClr tcell.Color
	dirClr  tcell.Color
	amtClr  tcell.Color
}

func activityToRow(a Activity) actRow {
	when := a.TransactionTime
	if when.IsZero() && a.Date != "" {
		when, _ = time.Parse("2006-01-02", a.Date)
	}
	row := actRow{when: when, symbol: a.Symbol}

	switch a.ActivityType {
	case "FILL", "":
		if strings.EqualFold(a.Type, "partial_fill") {
			row.typeStr, row.typeClr = "PART FILL", cYellow
		} else {
			row.typeStr, row.typeClr = "FILL", cGreen
		}
		row.dir = strings.ToUpper(a.Side)
		if strings.EqualFold(a.Side, "buy") {
			row.dirClr = cCyan
		} else {
			row.dirClr = cRed
		}
		row.qty = a.Qty
		row.price = "$" + fmtPrice(a.Price)
		if qty, err1 := strconv.ParseFloat(a.Qty, 64); err1 == nil {
			if px, err2 := strconv.ParseFloat(a.Price, 64); err2 == nil {
				row.amount = fmt.Sprintf("$%.2f", qty*px)
				if strings.EqualFold(a.Side, "buy") {
					row.amtClr = cRed
				} else {
					row.amtClr = cGreen
				}
			}
		}
		if len(a.OrderID) > 8 {
			row.detail = a.OrderID[:8]
		} else {
			row.detail = a.OrderID
		}

	case "DIV", "DIVNRA", "DIVROC", "DIVTXEX", "CSD":
		row.typeStr, row.typeClr = a.ActivityType, cGreen
		row.dir, row.dirClr = "CREDIT", cGreen
		row.qty = a.Qty
		if a.PerShareAmount != "" {
			row.price = "$" + fmtPrice(a.PerShareAmount) + "/sh"
		}
		if a.NetAmount != "" {
			row.amount = "$" + fmtPrice(a.NetAmount)
			row.amtClr = cGreen
		}

	case "JNLC", "JNLS":
		row.typeStr, row.typeClr = "JOURNAL", cYellow
		if net, err := strconv.ParseFloat(a.NetAmount, 64); err == nil {
			if net >= 0 {
				row.dir, row.dirClr = "CREDIT", cGreen
				row.amount = fmt.Sprintf("$%.2f", net)
				row.amtClr = cGreen
			} else {
				row.dir, row.dirClr = "DEBIT", cRed
				row.amount = fmt.Sprintf("-$%.2f", -net)
				row.amtClr = cRed
			}
		}
		row.detail = a.Description

	case "CSW":
		row.typeStr, row.typeClr = "WITHDRAW", cOrange
		row.dir, row.dirClr = "DEBIT", cRed
		if net, err := strconv.ParseFloat(a.NetAmount, 64); err == nil {
			row.amount = fmt.Sprintf("-$%.2f", -net)
			row.amtClr = cRed
		}

	case "ACATC", "ACATU":
		row.typeStr, row.typeClr = "ACAT", cCyan
		row.dir, row.dirClr = "TRANSFER", cCyan
		if a.NetAmount != "" {
			row.amount = "$" + fmtPrice(a.NetAmount)
			row.amtClr = cCyan
		}

	case "PTC":
		row.typeStr, row.typeClr = "CHARGE", cRed
		row.dir, row.dirClr = "DEBIT", cRed
		if net, err := strconv.ParseFloat(a.NetAmount, 64); err == nil {
			row.amount = fmt.Sprintf("-$%.2f", -net)
			row.amtClr = cRed
		}

	case "REORG":
		row.typeStr, row.typeClr = "REORG", cYellow
		row.qty = a.Qty

	default:
		row.typeStr, row.typeClr = a.ActivityType, cGray2
		if a.NetAmount != "" {
			row.amount = "$" + fmtPrice(a.NetAmount)
			if net, err := strconv.ParseFloat(a.NetAmount, 64); err == nil && net >= 0 {
				row.amtClr = cGreen
			} else {
				row.amtClr = cRed
			}
		}
		row.detail = a.Description
	}
	return row
}

func closedOrderToRow(o Order) (actRow, bool) {
	row := actRow{
		when:   o.CreatedAt,
		symbol: o.Symbol,
		dir:    strings.ToUpper(o.Side),
		qty:    o.Qty,
	}
	if strings.EqualFold(o.Side, "buy") {
		row.dirClr = cCyan
	} else {
		row.dirClr = cRed
	}
	if o.LimitPrice != "" && o.LimitPrice != "0" {
		row.price = "$" + fmtPrice(o.LimitPrice)
	} else {
		row.price = "MARKET"
	}
	if len(o.ID) > 8 {
		row.detail = o.ID[:8]
	} else {
		row.detail = o.ID
	}

	switch strings.ToLower(o.Status) {
	case "filled":
		row.typeStr, row.typeClr = "FILLED", cGreen
		if o.FilledAvgPrice != "" {
			row.price = "$" + fmtPrice(o.FilledAvgPrice)
		}
		qty, e1 := strconv.ParseFloat(o.FilledQty, 64)
		px, e2 := strconv.ParseFloat(o.FilledAvgPrice, 64)
		if e1 == nil && e2 == nil && px > 0 {
			row.amount = fmt.Sprintf("$%.2f", qty*px)
			if strings.EqualFold(o.Side, "buy") {
				row.amtClr = cRed
			} else {
				row.amtClr = cGreen
			}
		}
	case "partially_filled":
		row.typeStr, row.typeClr = "PART FILLED", cYellow
		if o.FilledAvgPrice != "" {
			row.price = "$" + fmtPrice(o.FilledAvgPrice)
		}
	case "canceled":
		row.typeStr, row.typeClr = "CANCELLED", cGray2
	case "expired":
		row.typeStr, row.typeClr = "EXPIRED", cGray
	case "rejected":
		row.typeStr, row.typeClr = "REJECTED", cRed
	case "held":
		row.typeStr, row.typeClr = "HELD", cYellow
	default:
		row.typeStr, row.typeClr = strings.ToUpper(o.Status), cGray2
	}
	return row, true
}

func (a *termApp) loadActivities(activities []Activity, closedOrders []Order, actErr, ordErr error) {
	a.activityTable.Clear()

	for i, h := range []string{"TIME", "TYPE", "SYMBOL", "DIR", "QTY", "PRICE", "AMOUNT", "DETAIL"} {
		a.activityTable.SetCell(0, i,
			tview.NewTableCell(" "+h+" ").
				SetTextColor(cOrange).
				SetAttributes(tcell.AttrBold).
				SetSelectable(false),
		)
	}

	if actErr != nil && ordErr != nil {
		a.activityTable.SetCell(1, 0,
			tview.NewTableCell("  ERROR: "+strings.ToUpper(actErr.Error())).
				SetTextColor(cRed).SetSelectable(false),
		)
		return
	}

	// Index order IDs that already have a confirmed FILL activity entry.
	// Filled closed-orders that are already represented get skipped to avoid
	// duplicates; ones that aren't (propagation delay) are shown as FILLED rows.
	filledByActivity := make(map[string]bool)
	for _, act := range activities {
		if act.ActivityType == "FILL" && act.OrderID != "" {
			filledByActivity[act.OrderID] = true
		}
	}

	var rows []actRow
	for _, act := range activities {
		rows = append(rows, activityToRow(act))
	}
	for _, o := range closedOrders {
		if strings.EqualFold(o.Status, "filled") && filledByActivity[o.ID] {
			continue // already shown as a FILL activity row
		}
		if r, ok := closedOrderToRow(o); ok {
			rows = append(rows, r)
		}
	}

	sort.Slice(rows, func(i, j int) bool {
		return rows[i].when.After(rows[j].when)
	})

	if len(rows) == 0 {
		a.activityTable.SetCell(1, 0,
			tview.NewTableCell("  NO ACTIVITY FOUND — PRESS R TO REFRESH").
				SetTextColor(cGray2).SetSelectable(false),
		)
		return
	}

	for i, row := range rows {
		timeStr := row.when.Local().Format("01/02 15:04:05")
		if row.when.IsZero() {
			timeStr = "—"
		}
		r := i + 1
		cell := func(text string, color tcell.Color, attr tcell.AttrMask) *tview.TableCell {
			return tview.NewTableCell(" " + text + " ").SetTextColor(color).SetAttributes(attr)
		}
		a.activityTable.SetCell(r, 0, cell(timeStr, cGray2, 0))
		a.activityTable.SetCell(r, 1, cell(row.typeStr, row.typeClr, tcell.AttrBold))
		a.activityTable.SetCell(r, 2, cell(row.symbol, cWhite, tcell.AttrBold))
		a.activityTable.SetCell(r, 3, cell(row.dir, row.dirClr, tcell.AttrBold))
		a.activityTable.SetCell(r, 4, cell(row.qty, cWhite, 0))
		a.activityTable.SetCell(r, 5, cell(row.price, cWhite, 0))
		a.activityTable.SetCell(r, 6, cell(row.amount, row.amtClr, tcell.AttrBold))
		a.activityTable.SetCell(r, 7, cell(row.detail, cGray2, 0))
	}
}

func (a *termApp) refreshStatus() {
	// NOTE on bracket syntax: tview parses [X] as a color tag and strips it
	// silently if X isn't a valid color (so "[Q]UIT" rendered as just "UIT").
	// The documented escape is [X[]  →  rendered as [X].
	hint := "[#555555][Q[]UIT  [R[]/F5 REFRESH  [1[][2[][3[][4[][5[] TABS · MOUSE OK[-]"
	switch a.activeTab {
	case tabOrders:
		hint = "[#555555][Q[]UIT  [R[]/F5 REFRESH  [X[]/DEL OR RIGHT-CLICK TO CANCEL ORDER[-]"
	case tabPositions:
		hint = "[#555555][Q[]UIT  [R[]/F5 REFRESH  DOUBLE-CLICK POSITION → SELL[-]"
	case tabChart:
		hint = "[#555555][Q[]UIT  [R[]/F5 REFRESH  RANGE [D[][W[][M[][T[]YD [Y[][F[]IVE MA[X[]  ·  CLICK CANDLE ROW  ·  ←/→ , . SCROLL · HOME/END[-]"
	}
	a.statusBar.SetText(fmt.Sprintf(
		"  [#FF6600]PORTFOLIO[-] [white]%s[-]   [#FF6600]CASH[-] [white]%s[-]   [#FF6600]BUYING POWER[-] [white]%s[-]    %s",
		fmtMoney(a.account.PortfolioValue),
		fmtMoney(a.account.Cash),
		fmtMoney(a.account.BuyingPower),
		hint,
	))
}

// ── Order submission ──────────────────────────────────────────────────────────

func (a *termApp) onSubmit() {
	_, action := a.actionDD.GetCurrentOption()
	_, orderType := a.typeDD.GetCurrentOption()
	sym := strings.ToUpper(strings.TrimSpace(a.symField.GetText()))
	qty := strings.TrimSpace(a.qtyField.GetText())
	price := strings.TrimSpace(a.priceField.GetText())

	if sym == "" {
		a.setResult("[#FF3131]>> SYMBOL IS REQUIRED[-]")
		return
	}
	if qty == "" {
		a.setResult("[#FF3131]>> QUANTITY IS REQUIRED[-]")
		return
	}
	if _, err := strconv.ParseFloat(qty, 64); err != nil {
		a.setResult("[#FF3131]>> QUANTITY MUST BE A NUMBER[-]")
		return
	}

	req := OrderRequest{
		Symbol:      sym,
		Qty:         qty,
		Side:        strings.ToLower(action),
		Type:        strings.ToLower(orderType),
		TimeInForce: "day",
	}

	if strings.EqualFold(orderType, "LIMIT") {
		if price == "" {
			a.setResult("[#FF3131]>> LIMIT PRICE IS REQUIRED[-]")
			return
		}
		if _, err := strconv.ParseFloat(price, 64); err != nil {
			a.setResult("[#FF3131]>> LIMIT PRICE MUST BE A NUMBER[-]")
			return
		}
		req.LimitPrice = price
	}

	a.showConfirmModal(req)
}

func (a *termApp) showCancelModal(orderID, symbol string) {
	shortID := orderID
	if len(shortID) > 8 {
		shortID = shortID[:8]
	}

	a.confirmActive = true

	modal := tview.NewModal().
		SetText(fmt.Sprintf("  CANCEL order for [::b]%s[-]?\n\n  ID: %s", symbol, shortID)).
		AddButtons([]string{"CANCEL ORDER", "KEEP"}).
		SetBackgroundColor(cDark).
		SetTextColor(cWhite).
		SetButtonBackgroundColor(cOrange).
		SetButtonTextColor(cBlack).
		SetDoneFunc(func(_ int, label string) {
			a.confirmActive = false
			a.pages.RemovePage("confirm")
			a.tapp.SetFocus(a.ordersTable)
			if label == "CANCEL ORDER" {
				go a.executeCancelOrder(orderID)
			}
		})

	a.pages.AddPage("confirm", modal, true, true)
	a.tapp.SetFocus(modal)
}

func (a *termApp) executeCancelOrder(orderID string) {
	err := client.CancelOrder(orderID)
	a.tapp.QueueUpdateDraw(func() {
		if err != nil {
			a.setResult("[#FF3131]>> CANCEL FAILED: " + strings.ToUpper(err.Error()) + "[-]")
		} else {
			a.setResult("[#00FF41]>> ORDER CANCELED[-]")
			go a.refresh()
		}
	})
}

func (a *termApp) showConfirmModal(req OrderRequest) {
	limitStr := "MARKET"
	if req.LimitPrice != "" {
		limitStr = "$" + req.LimitPrice
	}

	snapshot := fmt.Sprintf(
		"  ACTION    :  %s\n  TYPE      :  %s\n  SYMBOL    :  %s\n  QUANTITY  :  %s\n  PRICE     :  %s\n  TIF       :  DAY",
		strings.ToUpper(req.Side),
		strings.ToUpper(req.Type),
		req.Symbol,
		req.Qty,
		limitStr,
	)

	a.confirmActive = true

	modal := tview.NewModal().
		SetText(snapshot).
		AddButtons([]string{"CONFIRM", "CANCEL"}).
		SetBackgroundColor(cDark).
		SetTextColor(cWhite).
		SetButtonBackgroundColor(cOrange).
		SetButtonTextColor(cBlack).
		SetDoneFunc(func(_ int, label string) {
			a.confirmActive = false
			a.pages.RemovePage("confirm")
			a.tapp.SetFocus(a.form)
			if label == "CONFIRM" {
				a.setResult(fmt.Sprintf("[#FFD700]>> PLACING %s ORDER FOR %s x%s...[-]",
					strings.ToUpper(req.Type), req.Symbol, req.Qty))
				go a.executeOrder(req)
			}
		})

	a.pages.AddPage("confirm", modal, true, true)
	a.tapp.SetFocus(modal)
}

func (a *termApp) executeOrder(req OrderRequest) {
	order, err := client.PlaceOrder(req)
	a.tapp.QueueUpdateDraw(func() {
		if err != nil {
			a.setResult("[#FF3131]>> ERROR: " + strings.ToUpper(err.Error()) + "[-]")
			return
		}
		logTrade(req, order)
		id := order.ID
		if len(id) > 8 {
			id = id[:8]
		}
		a.setResult(fmt.Sprintf("[#00FF41]>> ORDER PLACED  ID:%s  STATUS:%s  (logged to trades.csv)[-]",
			id, strings.ToUpper(order.Status)))
		a.symField.SetText("")
		a.qtyField.SetText("")
		a.priceField.SetText("")
		go a.refresh() // immediate: closed order appears right away
		go func() {
			time.Sleep(3 * time.Second)
			a.refresh() // delayed: FILL activity propagates within a few seconds
		}()
	})
}

// logTrade appends a row to trades.csv, creating it with headers if needed.
func logTrade(req OrderRequest, order Order) {
	const csvPath = "trades.csv"
	headers := []string{"timestamp", "symbol", "side", "type", "qty", "limit_price", "order_id", "status"}

	isNew := false
	if _, err := os.Stat(csvPath); os.IsNotExist(err) {
		isNew = true
	}

	f, err := os.OpenFile(csvPath, os.O_APPEND|os.O_CREATE|os.O_WRONLY, 0644)
	if err != nil {
		return
	}
	defer f.Close()

	w := csv.NewWriter(f)
	if isNew {
		_ = w.Write(headers)
	}
	_ = w.Write([]string{
		time.Now().UTC().Format(time.RFC3339),
		order.Symbol,
		order.Side,
		order.Type,
		order.Qty,
		req.LimitPrice,
		order.ID,
		order.Status,
	})
	w.Flush()
}

func (a *termApp) onClear() {
	a.symField.SetText("")
	a.qtyField.SetText("")
	a.priceField.SetText("")
	a.setResult("")
}

func (a *termApp) setResult(text string) {
	a.resultTV.SetText("  " + text)
}

// ── Helpers ───────────────────────────────────────────────────────────────────

func fmtPrice(s string) string {
	f, err := strconv.ParseFloat(s, 64)
	if err != nil {
		return s
	}
	return fmt.Sprintf("%.2f", f)
}

func fmtMoney(s string) string {
	if s == "" {
		return "---"
	}
	f, err := strconv.ParseFloat(s, 64)
	if err != nil {
		return s
	}
	return fmt.Sprintf("$%.2f", f)
}

// ── Auto-refresh ──────────────────────────────────────────────────────────────

func (a *termApp) startAutoRefresh() {
	const (
		period    = 10 * time.Second
		tickRate  = 250 * time.Millisecond
		barWidth  = 10
	)
	totalTicks := int(period / tickRate) // 40 ticks per cycle
	spinFrames := []string{"|", "/", "-", "\\"}

	a.stopCh = make(chan struct{})
	go func() {
		ticker := time.NewTicker(tickRate)
		defer ticker.Stop()
		tick := 0
		for {
			select {
			case <-a.stopCh:
				return
			case <-ticker.C:
				tick++
				spin := spinFrames[tick%len(spinFrames)]
				filled := (tick * barWidth) / totalTicks
				if filled > barWidth {
					filled = barWidth
				}
				bar := strings.Repeat("█", filled) + strings.Repeat("░", barWidth-filled)
				elapsed := tick / 4
				text := fmt.Sprintf(" [#555555]AUTO [#FF6600]%s[-] [#00FF41]%s[-][#555555] %ds[-] ", spin, bar, elapsed)

				a.tapp.QueueUpdateDraw(func() {
					a.indicatorTV.SetText(text)
				})

				if tick >= totalTicks {
					tick = 0
					go a.refresh()
				}
			}
		}
	}()
}

// ── Main ──────────────────────────────────────────────────────────────────────

func main() {
	reset := flag.Bool("reset", false, "clear stored credentials and re-run setup")
	flag.Parse()

	if *reset {
		deleteCredentials()
	}

	creds, err := loadCredentials()
	if err != nil || creds.APIKey == "" {
		creds = runSetup()
		if err := saveCredentials(creds); err != nil {
			fmt.Fprintf(os.Stderr, "warning: could not save credentials: %v\n", err)
		}
	}

	client = NewAlpacaClient(creds)

	a := newTermApp()
	go loadAssets()
	go a.refresh()
	a.startAutoRefresh()
	if err := a.tapp.Run(); err != nil {
		log.Fatal(err)
	}
	close(a.stopCh)
}
