package main

import (
	"math"
	"testing"
	"time"
)

// barsFromCloses fabricates a bars slice where O=H=L=C for testing — the
// strategies under test only look at Close.
func barsFromCloses(closes []float64) []Bar {
	bars := make([]Bar, len(closes))
	t0 := time.Date(2020, 1, 1, 0, 0, 0, 0, time.UTC)
	for i, c := range closes {
		bars[i] = Bar{
			Time:  t0.Add(time.Duration(i) * 24 * time.Hour),
			Open:  c,
			High:  c,
			Low:   c,
			Close: c,
		}
	}
	return bars
}

func TestEMASeedAndProgress(t *testing.T) {
	// EMA of [1,2,3,4,5] with period 3: seed = (1+2+3)/3 = 2.0 at index 2,
	// then alpha = 2/4 = 0.5 — so EMA[3] = (4-2)*0.5 + 2 = 3.0, EMA[4] = 4.0.
	out := ema([]float64{1, 2, 3, 4, 5}, 3)
	want := []float64{0, 0, 2.0, 3.0, 4.0}
	for i, w := range want {
		if math.Abs(out[i]-w) > 1e-9 {
			t.Fatalf("ema[%d] = %v, want %v", i, out[i], w)
		}
	}
}

func TestMACDDetectsCrossover(t *testing.T) {
	// Construct a price series that trends down then up — MACD MUST emit at
	// least one bullish crossover (Buy signal) after the reversal. The
	// trend filter is intentionally disabled here so we isolate the
	// crossover-detection logic from the SMA gate (which gets its own test).
	closes := make([]float64, 0, 120)
	// Trend down for 60 bars from 100 -> 40
	for i := 0; i < 60; i++ {
		closes = append(closes, 100.0-float64(i))
	}
	// Trend up for 60 bars from 41 -> 100
	for i := 0; i < 60; i++ {
		closes = append(closes, 41.0+float64(i))
	}
	bars := barsFromCloses(closes)

	m := &MACD{Fast: 12, Slow: 26, Signal: 9, TrendSMA: 0}
	signals := m.GenerateSignals(bars)

	var buys, sells int
	for _, s := range signals {
		switch s {
		case SignalBuy:
			buys++
		case SignalSell:
			sells++
		}
	}
	if buys == 0 {
		t.Fatalf("expected at least one Buy signal after reversal, got 0")
	}
}

func TestMACDReturnsEmptyOnShortInput(t *testing.T) {
	bars := barsFromCloses([]float64{1, 2, 3, 4, 5})
	m := NewMACD(12, 26, 9)
	signals := m.GenerateSignals(bars)
	if len(signals) != len(bars) {
		t.Fatalf("len signals = %d, want %d", len(signals), len(bars))
	}
	for i, s := range signals {
		if s != SignalHold {
			t.Fatalf("signal[%d] = %v, want SignalHold on short input", i, s)
		}
	}
}

func TestSimulateBuyHoldEquivalent(t *testing.T) {
	// A strategy that buys on day 1 and never sells should match buy-and-hold
	// to within floating-point noise.
	closes := []float64{10, 11, 12, 11, 13, 14, 12, 15}
	bars := barsFromCloses(closes)
	signals := make([]Signal, len(bars))
	signals[0] = SignalBuy

	ending, trades := simulate(bars, signals, 10000)
	if trades != 1 {
		t.Fatalf("trades = %d, want 1 (one entry, no exit)", trades)
	}
	stratPct := (ending - 10000) / 10000 * 100
	bhPct := buyHoldReturn(bars)
	if math.Abs(stratPct-bhPct) > 1e-6 {
		t.Fatalf("strategy %f vs buy&hold %f mismatch", stratPct, bhPct)
	}
}

func TestSimulateSellLocksGains(t *testing.T) {
	// Buy at 10, sell at 15, then price collapses — strategy should keep its
	// 50% gain and ignore the subsequent crash.
	closes := []float64{10, 12, 15, 5, 1}
	bars := barsFromCloses(closes)
	signals := make([]Signal, len(bars))
	signals[0] = SignalBuy
	signals[2] = SignalSell

	ending, trades := simulate(bars, signals, 10000)
	if trades != 2 {
		t.Fatalf("trades = %d, want 2", trades)
	}
	if math.Abs(ending-15000) > 1e-6 {
		t.Fatalf("ending = %f, want 15000 (50%% gain locked in)", ending)
	}
}

func TestSimulateIgnoresDoubleBuy(t *testing.T) {
	// A Buy while already long is a no-op (and vice versa for sells). This
	// matches the long-only, all-in / all-out execution model in backtest.go.
	closes := []float64{10, 11, 12, 13}
	bars := barsFromCloses(closes)
	signals := []Signal{SignalBuy, SignalBuy, SignalHold, SignalSell}

	_, trades := simulate(bars, signals, 10000)
	if trades != 2 {
		t.Fatalf("trades = %d, want 2 (one entry, one exit; duplicate buy ignored)", trades)
	}
}

func TestRunStrategiesAtTimeframeSkipsShortWindow(t *testing.T) {
	// Only 10 bars total — every registered strategy should report
	// "not enough data" at every timeframe.
	bars := barsFromCloses([]float64{1, 2, 3, 4, 5, 6, 7, 8, 9, 10})
	now := bars[len(bars)-1].Time
	strats := []Strategy{NewMACD(12, 26, 9), NewMACDRSI(12, 26, 9, 14, 50.0)}

	for _, tf := range timeframes {
		results := runStrategiesAtTimeframe(bars, strats, tf, now)
		if len(results) != len(strats) {
			t.Fatalf("%s: results = %d, want %d", tf.Label, len(results), len(strats))
		}
		for _, r := range results {
			if r.Error == "" {
				t.Fatalf("%s / %s: expected error, got none", tf.Label, r.StrategyName)
			}
		}
	}
}

func TestRunStrategiesAtTimeframeProducesOneResultPerStrategy(t *testing.T) {
	// 400 bars of a slow uptrend — plenty for both MACD and MACD+RSI to run.
	closes := make([]float64, 400)
	for i := range closes {
		closes[i] = 100 + float64(i)*0.5
	}
	bars := barsFromCloses(closes)
	now := bars[len(bars)-1].Time
	strats := []Strategy{NewMACD(12, 26, 9), NewMACDRSI(12, 26, 9, 14, 50.0)}

	results := runStrategiesAtTimeframe(bars, strats, Timeframe{"1Y", 365 * 24 * time.Hour}, now)
	if len(results) != 2 {
		t.Fatalf("results = %d, want 2", len(results))
	}
	if results[0].StrategyName != "MACD" {
		t.Fatalf("results[0].StrategyName = %q, want MACD", results[0].StrategyName)
	}
	if results[1].StrategyName != "MACD+RSI" {
		t.Fatalf("results[1].StrategyName = %q, want MACD+RSI", results[1].StrategyName)
	}
}

// The RSI gate must be a pure filter: every signal MACD+RSI emits must also
// appear in plain MACD's output. Whether the gate trips depends on the EMA /
// RSI dynamics aligning — that's tested separately. This invariant is much
// more robust because it only asserts what the filter *cannot* do.
func TestMACDRSISignalsAreSubsetOfMACD(t *testing.T) {
	// A sawtooth-ish series with several crossings.
	closes := make([]float64, 0, 300)
	for cycle := 0; cycle < 3; cycle++ {
		for i := 0; i < 50; i++ {
			closes = append(closes, 100.0-float64(i))
		}
		for i := 0; i < 50; i++ {
			closes = append(closes, 50.0+2.0*float64(i))
		}
	}
	bars := barsFromCloses(closes)

	plain := NewMACD(12, 26, 9).GenerateSignals(bars)
	filtered := NewMACDRSI(12, 26, 9, 14, 50.0).GenerateSignals(bars)

	for i := range plain {
		if filtered[i] == SignalBuy && plain[i] != SignalBuy {
			t.Fatalf("MACD+RSI emitted Buy at idx %d where MACD did not (%v)", i, plain[i])
		}
		if filtered[i] == SignalSell && plain[i] != SignalSell {
			t.Fatalf("MACD+RSI emitted Sell at idx %d where MACD did not (%v)", i, plain[i])
		}
	}
}

// Every Buy MACD+RSI emits must satisfy the momentum-confirmation contract:
// RSI strictly above the floor AND rising vs the prior bar.
func TestMACDRSIBuysOnlyAboveFloorAndRising(t *testing.T) {
	closes := make([]float64, 0, 300)
	for cycle := 0; cycle < 3; cycle++ {
		for i := 0; i < 50; i++ {
			closes = append(closes, 100.0-float64(i))
		}
		for i := 0; i < 50; i++ {
			closes = append(closes, 50.0+2.0*float64(i))
		}
	}
	bars := barsFromCloses(closes)

	const floor = 50.0
	filtered := NewMACDRSI(12, 26, 9, 14, floor).GenerateSignals(bars)

	closesF := make([]float64, len(bars))
	for i, b := range bars {
		closesF[i] = b.Close
	}
	rsiSeries := rsi(closesF, 14)

	for i := 15; i < len(bars); i++ {
		if filtered[i] != SignalBuy {
			continue
		}
		if rsiSeries[i] <= floor {
			t.Fatalf("MACD+RSI bought at idx %d with RSI %.2f ≤ floor %.0f", i, rsiSeries[i], floor)
		}
		if rsiSeries[i] <= rsiSeries[i-1] {
			t.Fatalf("MACD+RSI bought at idx %d with RSI not rising (%.2f → %.2f)",
				i, rsiSeries[i-1], rsiSeries[i])
		}
	}
}

func TestRSIBoundsAndMonotonicUp(t *testing.T) {
	// RSI of a strictly increasing series should converge near 100; bounds
	// [0, 100] must always hold.
	closes := make([]float64, 50)
	for i := range closes {
		closes[i] = 10 + float64(i)
	}
	out := rsi(closes, 14)
	for i, v := range out {
		if v < 0 || v > 100 {
			t.Fatalf("rsi[%d] = %v out of [0,100]", i, v)
		}
	}
	if out[len(out)-1] < 95 {
		t.Fatalf("final RSI on monotonic up = %v, want ≥ 95", out[len(out)-1])
	}
}

func TestRegistryIncludesAllStrategies(t *testing.T) {
	registerStrategies()
	strats := availableStrategies()
	names := make(map[string]bool, len(strats))
	for _, s := range strats {
		names[s.Name()] = true
	}
	for _, want := range []string{"MACD", "MACD+RSI", "Vol Breakout", "Bollinger Bands", "Markov Chain", "Markov Regime", "HMM"} {
		if !names[want] {
			t.Fatalf("%q missing from registry", want)
		}
	}
}

// ── SMA ──────────────────────────────────────────────────────────────────────

func TestSMASeedAndProgress(t *testing.T) {
	// sma([1,2,3,4,5], 3) → 0, 0, 2, 3, 4 (first period-1 zero, then trailing mean).
	out := sma([]float64{1, 2, 3, 4, 5}, 3)
	want := []float64{0, 0, 2, 3, 4}
	for i, w := range want {
		if math.Abs(out[i]-w) > 1e-9 {
			t.Fatalf("sma[%d] = %v, want %v", i, out[i], w)
		}
	}
}

// ── Simulator with shorts ────────────────────────────────────────────────────

func TestSimulateShortProfitOnDrop(t *testing.T) {
	// Open short at $10, price drops to $5, cover. Expect 50% gain.
	closes := []float64{10, 8, 5, 5}
	bars := barsFromCloses(closes)
	signals := []Signal{SignalShort, SignalHold, SignalSell, SignalHold}

	ending, trades := simulate(bars, signals, 10000)
	if trades != 2 {
		t.Fatalf("trades = %d, want 2 (open short + cover)", trades)
	}
	if math.Abs(ending-15000) > 1e-6 {
		t.Fatalf("ending = %f, want 15000 (+50%% on short)", ending)
	}
}

func TestSimulateShortLossOnRise(t *testing.T) {
	// Open short at $10, price rises to $15, cover. Expect 50% loss.
	closes := []float64{10, 12, 15, 15}
	bars := barsFromCloses(closes)
	signals := []Signal{SignalShort, SignalHold, SignalSell, SignalHold}

	ending, trades := simulate(bars, signals, 10000)
	if trades != 2 {
		t.Fatalf("trades = %d, want 2", trades)
	}
	if math.Abs(ending-5000) > 1e-6 {
		t.Fatalf("ending = %f, want 5000 (-50%% on short)", ending)
	}
}

func TestSimulateFlipLongToShortInOneSignal(t *testing.T) {
	// Buy at $10 (open long), then on next bar Short at $12 — flip should
	// close the long and open the short in a single signal. 3 legs total:
	// open long, close long, open short.
	closes := []float64{10, 12, 6, 6}
	bars := barsFromCloses(closes)
	signals := []Signal{SignalBuy, SignalShort, SignalSell, SignalHold}

	ending, trades := simulate(bars, signals, 10000)
	if trades != 4 {
		t.Fatalf("trades = %d, want 4 (open long, close long, open short, cover)", trades)
	}
	// At bar 1: long 1000 shares @ $10. Sell @ $12 → cash = 12000.
	// Open short with 12000/12 = 1000 shares; cash += 1000*12 = 24000; shares=-1000.
	// At bar 2: cover short — cash += -1000*6 = -6000 → cash = 18000. shares = 0.
	if math.Abs(ending-18000) > 1e-6 {
		t.Fatalf("ending = %f, want 18000", ending)
	}
}

func TestSimulateOpenShortHeldToEndOfWindow(t *testing.T) {
	// Open short and never cover — ending value must use signed MTM correctly.
	// Cash starts at 10000. Short 1000 shares @ $10 → cash = 20000, shares = -1000.
	// Last close $7 → ending = 20000 + (-1000)*7 = 13000 → +30%.
	closes := []float64{10, 9, 8, 7}
	bars := barsFromCloses(closes)
	signals := []Signal{SignalShort, SignalHold, SignalHold, SignalHold}

	ending, trades := simulate(bars, signals, 10000)
	if trades != 1 {
		t.Fatalf("trades = %d, want 1 (just the open)", trades)
	}
	if math.Abs(ending-13000) > 1e-6 {
		t.Fatalf("ending = %f, want 13000 (open short MTM at $7)", ending)
	}
}

// Critical regression test: the rewrite must NOT change long-only outcomes.
// If a future change to simulate() drifts the math for existing strategies,
// this test catches it.
func TestSimulateBackwardCompatLongOnly(t *testing.T) {
	closes := []float64{10, 11, 12, 13, 14}
	bars := barsFromCloses(closes)
	signals := []Signal{SignalBuy, SignalHold, SignalHold, SignalSell, SignalHold}

	ending, trades := simulate(bars, signals, 10000)
	if trades != 2 {
		t.Fatalf("trades = %d, want 2", trades)
	}
	// 10000/10 = 1000 shares @ $10. Sell @ $13 → cash = 13000.
	if math.Abs(ending-13000) > 1e-6 {
		t.Fatalf("ending = %f, want 13000 (long-only behavior unchanged)", ending)
	}
}

// ── Bollinger Bands & ATR ────────────────────────────────────────────────────

func TestBBBoundsAreSymmetricAroundMean(t *testing.T) {
	closes := []float64{1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20}
	upper, mid, lower := bb(closes, 20, 2.0)
	last := len(closes) - 1
	if math.Abs(mid[last]-10.5) > 1e-9 {
		t.Fatalf("mid = %v, want 10.5 (mean of 1..20)", mid[last])
	}
	// Upper and lower must be equidistant from the middle.
	if math.Abs((upper[last]-mid[last])-(mid[last]-lower[last])) > 1e-9 {
		t.Fatalf("BB asymmetric: upper-mid=%f, mid-lower=%f",
			upper[last]-mid[last], mid[last]-lower[last])
	}
	if upper[last] <= mid[last] || lower[last] >= mid[last] {
		t.Fatalf("upper=%f mid=%f lower=%f ordering wrong", upper[last], mid[last], lower[last])
	}
}

func TestATRReflectsBarRange(t *testing.T) {
	// Constant range per bar: H-L = 2 always, no gaps. ATR should converge to 2.
	bars := make([]Bar, 50)
	t0 := time.Date(2020, 1, 1, 0, 0, 0, 0, time.UTC)
	for i := range bars {
		bars[i] = Bar{
			Time: t0.Add(time.Duration(i) * 24 * time.Hour),
			High: 11, Low: 9, Open: 10, Close: 10,
		}
	}
	out := atr(bars, 14)
	if math.Abs(out[len(out)-1]-2.0) > 1e-9 {
		t.Fatalf("final ATR = %v, want 2.0 on constant H-L=2 series", out[len(out)-1])
	}
}

// ── Vol Breakout ─────────────────────────────────────────────────────────────

// barsWithRange synthesises bars with non-zero high/low so ATR is meaningful
// (barsFromCloses sets H=L=C → ATR=0, which collapses the trailing stop).
func barsWithRange(closes []float64, hlPct float64) []Bar {
	bars := make([]Bar, len(closes))
	t0 := time.Date(2020, 1, 1, 0, 0, 0, 0, time.UTC)
	for i, c := range closes {
		bars[i] = Bar{
			Time:  t0.Add(time.Duration(i) * 24 * time.Hour),
			Open:  c,
			High:  c * (1 + hlPct),
			Low:   c * (1 - hlPct),
			Close: c,
		}
	}
	return bars
}

func TestVolBreakoutEntersLongOnUpsideBreakout(t *testing.T) {
	// 80 bars of flat-ish prices then a sharp upward break. Close must pop
	// above BB upper AND above EMA50 → expect a SignalBuy.
	closes := make([]float64, 0, 120)
	for i := 0; i < 80; i++ {
		closes = append(closes, 100.0+0.05*float64(i)) // slow drift up
	}
	for i := 0; i < 40; i++ {
		closes = append(closes, 104.0+1.5*float64(i)) // sharp breakout
	}
	bars := barsWithRange(closes, 0.005)

	signals := NewVolBreakout(20, 2.0, 50, 14, 2.0).GenerateSignals(bars)
	var sawBuy bool
	for _, s := range signals {
		if s == SignalBuy {
			sawBuy = true
			break
		}
	}
	if !sawBuy {
		t.Fatal("VolBreakout did not emit a Buy on an obvious upside breakout")
	}
}

func TestVolBreakoutEntersShortOnDownsideBreakout(t *testing.T) {
	// Symmetric: long base then sharp drop. Expect a Short.
	closes := make([]float64, 0, 120)
	for i := 0; i < 80; i++ {
		closes = append(closes, 100.0-0.05*float64(i))
	}
	for i := 0; i < 40; i++ {
		closes = append(closes, 96.0-1.5*float64(i))
	}
	bars := barsWithRange(closes, 0.005)

	signals := NewVolBreakout(20, 2.0, 50, 14, 2.0).GenerateSignals(bars)
	var sawShort bool
	for _, s := range signals {
		if s == SignalShort {
			sawShort = true
			break
		}
	}
	if !sawShort {
		t.Fatal("VolBreakout did not emit a Short on an obvious downside breakdown")
	}
}

func TestVolBreakoutPositionStateIsConsistent(t *testing.T) {
	// Every entry signal must be eventually followed by an exit (or end of
	// series with the position implicitly held to MTM). No two consecutive
	// entries without an exit between them.
	closes := make([]float64, 0, 300)
	for cycle := 0; cycle < 3; cycle++ {
		for i := 0; i < 50; i++ {
			closes = append(closes, 100.0-0.5*float64(i))
		}
		for i := 0; i < 50; i++ {
			closes = append(closes, 75.0+1.0*float64(i))
		}
	}
	bars := barsWithRange(closes, 0.01)

	signals := NewVolBreakout(20, 2.0, 50, 14, 2.0).GenerateSignals(bars)
	position := 0
	for i, s := range signals {
		switch s {
		case SignalBuy:
			if position != 0 {
				t.Fatalf("Buy at idx %d while position=%d (should be flat first)", i, position)
			}
			position = 1
		case SignalShort:
			if position != 0 {
				t.Fatalf("Short at idx %d while position=%d", i, position)
			}
			position = -1
		case SignalSell:
			if position == 0 {
				t.Fatalf("Sell at idx %d while flat", i)
			}
			position = 0
		}
	}
}

func TestVolBreakoutWarmupSafe(t *testing.T) {
	// Fewer bars than longest indicator (TrendEMA=50) — must return all Holds.
	closes := make([]float64, 30)
	for i := range closes {
		closes[i] = 100 + float64(i)
	}
	bars := barsWithRange(closes, 0.01)

	signals := NewVolBreakout(20, 2.0, 50, 14, 2.0).GenerateSignals(bars)
	if len(signals) != len(bars) {
		t.Fatalf("len signals = %d, want %d", len(signals), len(bars))
	}
	for i, s := range signals {
		if s != SignalHold {
			t.Fatalf("signal[%d] = %v, want Hold during warm-up", i, s)
		}
	}
}

func TestBuyHoldReturn(t *testing.T) {
	bars := barsFromCloses([]float64{100, 110, 120, 150})
	got := buyHoldReturn(bars)
	if math.Abs(got-50.0) > 1e-9 {
		t.Fatalf("buyHoldReturn = %f, want 50.0", got)
	}
}

func TestSliceFrom(t *testing.T) {
	closes := []float64{1, 2, 3, 4, 5, 6, 7, 8, 9, 10}
	bars := barsFromCloses(closes)
	// bars are day-spaced starting 2020-01-01. Slice from 2020-01-05.
	cutoff := bars[4].Time
	got := sliceFrom(bars, cutoff)
	if len(got) != 6 {
		t.Fatalf("len = %d, want 6", len(got))
	}
	if !got[0].Time.Equal(cutoff) {
		t.Fatalf("first time = %v, want %v", got[0].Time, cutoff)
	}
}

func TestStrategyRegistryPopulated(t *testing.T) {
	registerStrategies()
	strats := availableStrategies()
	if len(strats) == 0 {
		t.Fatal("expected at least one registered strategy")
	}
	if strats[0].Name() != "MACD" {
		t.Fatalf("first strategy = %q, want MACD", strats[0].Name())
	}
}
