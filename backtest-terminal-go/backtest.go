package main

import "time"

// startingCash is the simulated capital each backtest begins with. The return
// percentage is invariant to this value — it only affects the share count in
// debug logs — but a round number keeps trade math readable.
const startingCash = 10_000.0

// Result is the outcome of running one strategy over one window of bars.
type Result struct {
	StrategyName string
	StartTime    time.Time
	EndTime      time.Time
	StartingCash float64
	EndingValue  float64
	ReturnPct    float64 // strategy return, %
	BuyHoldPct   float64 // buy-and-hold over the same window, %
	Trades       int     // number of executed entries+exits
	Bars         int
	Error        string // non-empty when this window failed (e.g. not enough data)
}

// Timeframe defines one row in the results table.
type Timeframe struct {
	Label    string
	Duration time.Duration
}

// timeframes lists the windows displayed in the UI, shortest first.
// 30-day months and 365-day years are deliberate approximations — exact
// calendar boundaries don't matter for return comparison.
var timeframes = []Timeframe{
	{"1M", 30 * 24 * time.Hour},
	{"3M", 90 * 24 * time.Hour},
	{"6M", 180 * 24 * time.Hour},
	{"1Y", 365 * 24 * time.Hour},
	{"3Y", 3 * 365 * 24 * time.Hour},
	{"5Y", 5 * 365 * 24 * time.Hour},
	{"10Y", 10 * 365 * 24 * time.Hour},
}

// simulate runs the all-in/all-out execution model with support for both long
// and short positions. `shares` is signed: positive = long, negative = short.
// Signals are evaluated AT the close that generated them — standard retail
// backtest approximation, avoids the look-ahead trap of acting before a bar
// closes.
//
// Signal semantics:
//
//	SignalBuy   → target long.  Covers any short first, then opens long with cash.
//	SignalShort → target short. Closes any long first, then opens short.
//	SignalSell  → target flat.  Universal flatten — closes long OR short.
//
// Trade counting: each leg of a position change is one trade. A "flip" (Buy
// when short, or Short when long) executes two legs in one bar and counts as
// 2 trades. Open + close = 2 trades, matching long-only behavior.
//
// Short equity math: opening a short at price P with n = cash/P shares
// credits cash by n*P (sale proceeds) and sets shares = -n. The universal
// mark-to-market `cash + shares*price` then tracks P&L correctly for both
// signs. No margin model — losses on shorts can drive equity negative; the
// UI's percentage formatter handles that.
//
// Backward compat: strategies that only emit Buy/Sell run identically to the
// prior long-only simulator. The new branches only activate when shares is
// nonzero in the opposite direction.
func simulate(bars []Bar, signals []Signal, cash float64) (endingValue float64, trades int) {
	shares := 0.0
	for i, sig := range signals {
		price := bars[i].Close
		if price <= 0 {
			continue
		}
		switch sig {
		case SignalBuy:
			if shares < 0 {
				cash += shares * price // cover short (shares is negative → cash decreases)
				shares = 0
				trades++
			}
			if shares == 0 {
				shares = cash / price
				cash = 0
				trades++
			}
		case SignalShort:
			if shares > 0 {
				cash += shares * price // close long
				shares = 0
				trades++
			}
			if shares == 0 {
				n := cash / price
				cash += n * price // sale proceeds from shorting
				shares = -n
				trades++
			}
		case SignalSell:
			if shares != 0 {
				cash += shares * price
				shares = 0
				trades++
			}
		}
	}
	// Mark to market at the last close. Formula works for long, short, or flat.
	endingValue = cash
	if shares != 0 && len(bars) > 0 {
		endingValue += shares * bars[len(bars)-1].Close
	}
	return endingValue, trades
}

// buyHoldReturn is the percent change from the first close to the last close.
// Used as the benchmark column in the results table.
func buyHoldReturn(bars []Bar) float64 {
	if len(bars) < 2 || bars[0].Close == 0 {
		return 0
	}
	first := bars[0].Close
	last := bars[len(bars)-1].Close
	return (last - first) / first * 100.0
}

// sliceFrom returns the subslice of bars whose timestamps are at or after
// `start`. Assumes bars are sorted ascending by time, which Alpaca guarantees.
func sliceFrom(bars []Bar, start time.Time) []Bar {
	lo, hi := 0, len(bars)
	for lo < hi {
		mid := (lo + hi) / 2
		if bars[mid].Time.Before(start) {
			lo = mid + 1
		} else {
			hi = mid
		}
	}
	return bars[lo:]
}

// runStrategiesAtTimeframe runs every strategy over a single window carved
// out of `bars`, returning one Result per strategy. `now` is parameterised
// so tests can pin time without monkey-patching.
//
// Windows with fewer than 30 bars are reported as Error="not enough data"
// for every strategy, so the UI renders a graceful placeholder.
func runStrategiesAtTimeframe(bars []Bar, strategies []Strategy, tf Timeframe, now time.Time) []Result {
	start := now.Add(-tf.Duration)
	window := sliceFrom(bars, start)

	results := make([]Result, 0, len(strategies))
	for _, strat := range strategies {
		r := Result{
			StrategyName: strat.Name(),
			StartingCash: startingCash,
			Bars:         len(window),
		}

		if len(window) < 30 {
			r.Error = "not enough data"
			results = append(results, r)
			continue
		}

		r.StartTime = window[0].Time
		r.EndTime = window[len(window)-1].Time

		signals := strat.GenerateSignals(window)
		endingValue, trades := simulate(window, signals, startingCash)

		r.EndingValue = endingValue
		r.ReturnPct = (endingValue - startingCash) / startingCash * 100.0
		r.BuyHoldPct = buyHoldReturn(window)
		r.Trades = trades

		results = append(results, r)
	}
	return results
}
