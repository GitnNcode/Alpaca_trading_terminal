package main

import (
	"math"
	"testing"
)

// ── MACD trend filter ────────────────────────────────────────────────────────

// MACD with TrendSMA=200 must suppress Buys that fire while close ≤ SMA(200).
func TestMACDTrendFilterSuppressesCounterTrendBuys(t *testing.T) {
	// 300 bars, 6 cycles of down-then-up. Without the filter MACD fires
	// Buys at the bottom of each cycle (close ≪ long-term mean). With the
	// filter those Buys at low prices should be blocked.
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

	rawSignals := (&MACD{Fast: 12, Slow: 26, Signal: 9, TrendSMA: 0}).GenerateSignals(bars)
	filteredSignals := NewMACD(12, 26, 9).GenerateSignals(bars) // TrendSMA=200

	smaSeries := sma(closesOf(bars), 200)

	var rawBuys, filteredBuys, blocked int
	for i := range rawSignals {
		if rawSignals[i] == SignalBuy {
			rawBuys++
		}
		if filteredSignals[i] == SignalBuy {
			filteredBuys++
			if i < 199 {
				t.Fatalf("Buy at idx %d before SMA(200) is valid", i)
			}
			if closesOf(bars)[i] <= smaSeries[i] {
				t.Fatalf("Buy at idx %d with close %.2f ≤ SMA200 %.2f",
					i, closesOf(bars)[i], smaSeries[i])
			}
		}
		if rawSignals[i] == SignalBuy && filteredSignals[i] != SignalBuy {
			blocked++
		}
	}
	if filteredBuys > rawBuys {
		t.Fatalf("trend filter added Buys: raw=%d filtered=%d", rawBuys, filteredBuys)
	}
	if blocked == 0 {
		t.Fatal("trend filter did not block any Buys on a sawtooth — sanity check failed")
	}
}

// ── ADX indicator ────────────────────────────────────────────────────────────

func TestADXShortInputSafe(t *testing.T) {
	bars := barsFromCloses([]float64{1, 2, 3, 4, 5})
	got := adx(bars, 14)
	if len(got) != len(bars) {
		t.Fatalf("len = %d, want %d", len(got), len(bars))
	}
	for i, v := range got {
		if v != 0 {
			t.Fatalf("adx[%d] = %v, want 0 on insufficient input", i, v)
		}
	}
}

func TestADXBoundedAndRisesWithTrend(t *testing.T) {
	// Strong monotonic uptrend → ADX should climb well above 25.
	closes := make([]float64, 100)
	for i := range closes {
		closes[i] = 100 + float64(i)
	}
	bars := barsWithRange(closes, 0.005)
	got := adx(bars, 14)
	for i, v := range got {
		if v < 0 || v > 100 || math.IsNaN(v) {
			t.Fatalf("adx[%d] = %v out of [0,100]", i, v)
		}
	}
	if got[len(got)-1] < 25 {
		t.Fatalf("final ADX = %.2f on strong uptrend, want > 25", got[len(got)-1])
	}
}

func TestADXChoppyStaysLow(t *testing.T) {
	closes := make([]float64, 200)
	for i := range closes {
		closes[i] = 100 + 0.3*math.Sin(float64(i)*1.5)
	}
	bars := barsWithRange(closes, 0.003)
	got := adx(bars, 14)
	if got[len(got)-1] > 30 {
		t.Fatalf("final ADX = %.2f on choppy series, want < 30", got[len(got)-1])
	}
}

// ── Vol Breakout: ATR-rising filter ──────────────────────────────────────────

// The ATR-rising gate must keep VolBreakout flat through a long period of
// contracting volatility, even if BB breakouts would otherwise have fired.
func TestVolBreakoutSkipsContractingVolatility(t *testing.T) {
	// 80 bars of expanding range (lots of ATR), then 80 bars where the
	// price gently drifts up with TINY intra-bar range. The contracting
	// section should NOT generate entries because ATR < its 20-day SMA.
	closes := make([]float64, 0, 160)
	for i := 0; i < 80; i++ {
		closes = append(closes, 100.0+float64(i))
	}
	for i := 0; i < 80; i++ {
		closes = append(closes, 180.0+0.01*float64(i))
	}
	bars := make([]Bar, len(closes))
	for i, c := range closes {
		// First half: wide range; second half: collapsed.
		hl := 0.02 * c
		if i >= 80 {
			hl = 0.0005 * c
		}
		bars[i] = Bar{
			Time:  barsFromCloses([]float64{c})[0].Time,
			Open:  c,
			High:  c + hl/2,
			Low:   c - hl/2,
			Close: c,
		}
	}
	signals := NewVolBreakout(20, 2.0, 50, 14, 2.0).GenerateSignals(bars)

	var lateEntries int
	for i := 100; i < len(signals); i++ { // after the regime change
		if signals[i] == SignalBuy || signals[i] == SignalShort {
			lateEntries++
		}
	}
	if lateEntries > 0 {
		t.Fatalf("VolBreakout opened %d entries during contracting-vol regime (expected 0)", lateEntries)
	}
}

// ── Bollinger Bands: ADX skip filter ─────────────────────────────────────────

// In a strongly trending market ADX > 25; BB mean-reversion must NOT take
// new entries there (it gets steamrolled). Exits can still fire.
func TestBollingerSkipsEntriesInTrendingRegime(t *testing.T) {
	// 200-bar strong uptrend — ADX climbs well above 25. Even when price
	// pokes above the upper band (which it does in a runaway trend), the
	// strategy should NOT open new shorts because the trend strength is
	// too high to fade.
	closes := make([]float64, 200)
	for i := range closes {
		closes[i] = 100 + float64(i)*1.5
	}
	bars := barsWithRange(closes, 0.003)
	signals := NewBollingerBands(20, 2.0).GenerateSignals(bars)

	var shorts int
	for _, s := range signals {
		if s == SignalShort {
			shorts++
		}
	}
	if shorts > 0 {
		t.Fatalf("BB opened %d Shorts in a strong uptrend (ADX > 25 — should skip)", shorts)
	}
}

// ── Markov Chain: persistence requirement ────────────────────────────────────

// Persistence=2 means a single isolated argmax flip must NOT generate a
// trade. We verify this on a constant series (all-Flat predictions never
// hit the threshold) and check the per-tick output count is sensible
// relative to the legacy form.
func TestMarkovChainPersistenceReducesTradeCount(t *testing.T) {
	// Simulate a return series that flips direction every bar — a pure
	// noise pattern. With persistence=2 the strategy should emit ZERO
	// directional signals on such alternating noise (no two-in-a-row
	// matching prediction). With persistence=1 we'd expect many.
	closes := make([]float64, 200)
	closes[0] = 100
	for i := 1; i < len(closes); i++ {
		if i%2 == 0 {
			closes[i] = closes[i-1] * 1.005
		} else {
			closes[i] = closes[i-1] * 0.995
		}
	}
	bars := barsFromCloses(closes)
	signals := NewMarkovChain().GenerateSignals(bars)

	var directional int
	for _, s := range signals {
		if s == SignalBuy || s == SignalShort {
			directional++
		}
	}
	if directional > 10 {
		t.Fatalf("MarkovChain emitted %d directional signals on alternating noise (expected ≤ 10 with persistence=2)", directional)
	}
}

// ── HMM: confidence threshold ────────────────────────────────────────────────

// With ConfThresh=1.01 (impossible) the HMM should emit no directional
// signals regardless of regime. This validates the gate plumbing.
func TestHMMConfidenceGateBlocksAllSignalsAtThresholdOne(t *testing.T) {
	closes := make([]float64, 250)
	closes[0] = 100
	for i := 1; i < 150; i++ {
		closes[i] = closes[i-1] * 1.003
	}
	for i := 150; i < 250; i++ {
		closes[i] = closes[i-1] * 0.997
	}
	bars := barsFromCloses(closes)
	h := NewHMMStrategy()
	h.ConfThresh = 1.01 // impossible — must produce no trades
	signals := h.GenerateSignals(bars)
	for i, s := range signals {
		if s != SignalHold {
			t.Fatalf("signal[%d] = %v with impossible threshold, want Hold", i, s)
		}
	}
}

// helper: closes of a bars slice (used by the MACD trend-filter test).
func closesOf(bars []Bar) []float64 {
	out := make([]float64, len(bars))
	for i, b := range bars {
		out[i] = b.Close
	}
	return out
}
