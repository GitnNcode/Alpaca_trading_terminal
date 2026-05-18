package main

import (
	"math"
	"testing"
)

// ── Helpers ──────────────────────────────────────────────────────────────────

func TestReturnsFromBarsBasic(t *testing.T) {
	bars := barsFromCloses([]float64{100, 110, 99})
	got := returnsFromBars(bars)
	want := []float64{0, 0.10, -0.10}
	if len(got) != len(want) {
		t.Fatalf("len = %d, want %d", len(got), len(want))
	}
	for i, w := range want {
		if math.Abs(got[i]-w) > 1e-9 {
			t.Fatalf("returns[%d] = %v, want %v", i, got[i], w)
		}
	}
}

func TestClassifyReturnStateBoundaries(t *testing.T) {
	sigma := 0.01
	mult := 0.3 // band = 0.003
	cases := []struct {
		r    float64
		want int
	}{
		{-0.01, 0},   // well below -band
		{-0.0031, 0}, // just below -band
		{-0.003, 1},  // exactly at -band → Flat (band is inclusive)
		{0, 1},
		{0.003, 1},
		{0.0031, 2},
		{0.01, 2},
	}
	for _, c := range cases {
		got := classifyReturnState(c.r, sigma, mult)
		if got != c.want {
			t.Errorf("classifyReturnState(%v) = %d, want %d", c.r, got, c.want)
		}
	}
}

// ── MarkovChain ──────────────────────────────────────────────────────────────

func TestMarkovChainWarmupSafe(t *testing.T) {
	bars := barsFromCloses([]float64{1, 2, 3, 4, 5, 6, 7, 8, 9, 10})
	signals := NewMarkovChain().GenerateSignals(bars)
	if len(signals) != len(bars) {
		t.Fatalf("len signals = %d, want %d", len(signals), len(bars))
	}
	for i, s := range signals {
		if s != SignalHold {
			t.Fatalf("signal[%d] = %v, want Hold during warm-up", i, s)
		}
	}
}

func TestMarkovChainEmitsBuyOnPersistentUptrend(t *testing.T) {
	// 200 bars compounding at +0.5%/bar — overwhelming Up-state persistence
	// so the transition matrix should predict Up given Up, with confidence
	// well above the 1/3 + 0.05 threshold.
	closes := make([]float64, 200)
	closes[0] = 100
	for i := 1; i < len(closes); i++ {
		closes[i] = closes[i-1] * 1.005
	}
	bars := barsFromCloses(closes)
	signals := NewMarkovChain().GenerateSignals(bars)

	var buys int
	for _, s := range signals {
		if s == SignalBuy {
			buys++
		}
	}
	if buys == 0 {
		t.Fatal("MarkovChain emitted no Buy signals on a sustained uptrend")
	}
}

func TestMarkovChainEmitsShortOnPersistentDowntrend(t *testing.T) {
	closes := make([]float64, 200)
	closes[0] = 100
	for i := 1; i < len(closes); i++ {
		closes[i] = closes[i-1] * 0.995
	}
	bars := barsFromCloses(closes)
	signals := NewMarkovChain().GenerateSignals(bars)

	var shorts int
	for _, s := range signals {
		if s == SignalShort {
			shorts++
		}
	}
	if shorts == 0 {
		t.Fatal("MarkovChain emitted no Short signals on a sustained downtrend")
	}
}

func TestMarkovChainFlatPredictionHolds(t *testing.T) {
	// Constant prices → every return is exactly 0 → every state is Flat.
	// Predicted-Flat must NOT trigger a Sell — the strategy holds throughout.
	closes := make([]float64, 200)
	for i := range closes {
		closes[i] = 100
	}
	bars := barsFromCloses(closes)
	signals := NewMarkovChain().GenerateSignals(bars)
	for i, s := range signals {
		if s != SignalHold {
			t.Fatalf("signal[%d] = %v on flat series, want Hold", i, s)
		}
	}
}

func TestMarkovChainZeroSigmaSafe(t *testing.T) {
	// Constant prices yield sigma=0 — the sigma floor must prevent NaN/Inf
	// from leaking into a signal. (TestMarkovChainFlatPredictionHolds covers
	// the semantic check; this one explicitly assures no NaN values exist.)
	closes := make([]float64, 80)
	for i := range closes {
		closes[i] = 50
	}
	bars := barsFromCloses(closes)
	signals := NewMarkovChain().GenerateSignals(bars)
	for i, s := range signals {
		if s != SignalHold {
			t.Fatalf("signal[%d] = %v, want Hold (and no NaN)", i, s)
		}
	}
}

// ── MarkovRegimeSwitch ───────────────────────────────────────────────────────

func TestMRSEmitsLongInUptrend(t *testing.T) {
	// Long, smooth uptrend with realistic intra-bar range so ATR > 0.
	closes := make([]float64, 200)
	closes[0] = 100
	for i := 1; i < len(closes); i++ {
		closes[i] = closes[i-1] + 0.5
	}
	bars := barsWithRange(closes, 0.001)
	signals := NewMarkovRegimeSwitch().GenerateSignals(bars)

	var buys, shorts int
	for _, s := range signals {
		switch s {
		case SignalBuy:
			buys++
		case SignalShort:
			shorts++
		}
	}
	if buys == 0 {
		t.Fatal("MRS emitted no Buy signals during a sustained uptrend")
	}
	if shorts > 0 {
		t.Fatalf("MRS emitted %d Shorts during a sustained uptrend (expected 0)", shorts)
	}
}

func TestMRSEmitsShortInDowntrend(t *testing.T) {
	closes := make([]float64, 200)
	closes[0] = 200
	for i := 1; i < len(closes); i++ {
		closes[i] = closes[i-1] - 0.5
	}
	bars := barsWithRange(closes, 0.001)
	signals := NewMarkovRegimeSwitch().GenerateSignals(bars)

	var buys, shorts int
	for _, s := range signals {
		switch s {
		case SignalBuy:
			buys++
		case SignalShort:
			shorts++
		}
	}
	if shorts == 0 {
		t.Fatal("MRS emitted no Short signals during a sustained downtrend")
	}
	if buys > 0 {
		t.Fatalf("MRS emitted %d Buys during a sustained downtrend (expected 0)", buys)
	}
}

func TestMRSChoppyHolds(t *testing.T) {
	// High-frequency oscillation (period < 5 bars) — the EMA(20) averages
	// the motion out so the 5-bar slope hovers near zero. The strategy
	// should rarely (if ever) flip into a directional trade.
	closes := make([]float64, 300)
	for i := range closes {
		closes[i] = 100 + 0.5*math.Sin(float64(i)*2.0)
	}
	bars := barsWithRange(closes, 0.005)
	signals := NewMarkovRegimeSwitch().GenerateSignals(bars)

	var directional int
	for _, s := range signals {
		if s == SignalBuy || s == SignalShort {
			directional++
		}
	}
	if directional > 5 {
		t.Fatalf("MRS emitted %d directional signals on a choppy series (expected ≤ 5)", directional)
	}
}

func TestMRSZeroATRSafe(t *testing.T) {
	// True ATR=0 case: prices constant means tr = max(H-L, |H-prevC|,
	// |L-prevC|) = 0 for every bar after the first. The classifier's
	// `if a <= 0` guard must keep MRS from dividing by zero and emit Hold.
	closes := make([]float64, 200)
	for i := range closes {
		closes[i] = 100
	}
	bars := barsFromCloses(closes)
	signals := NewMarkovRegimeSwitch().GenerateSignals(bars)
	for i, s := range signals {
		if s != SignalHold {
			t.Fatalf("signal[%d] = %v with ATR=0, want Hold", i, s)
		}
	}
}

// ── HMM ──────────────────────────────────────────────────────────────────────

func TestLogSumExpBasic(t *testing.T) {
	// log(exp(0)+exp(0)+exp(0)) = log(3)
	got := logSumExp([]float64{0, 0, 0})
	want := math.Log(3)
	if math.Abs(got-want) > 1e-9 {
		t.Fatalf("logSumExp([0,0,0]) = %v, want %v", got, want)
	}
	// logSumExp of a single value is the value itself.
	if got := logSumExp([]float64{1.234}); math.Abs(got-1.234) > 1e-9 {
		t.Fatalf("logSumExp single = %v, want 1.234", got)
	}
	// Empty → -Inf.
	if got := logSumExp(nil); !math.IsInf(got, -1) {
		t.Fatalf("logSumExp(nil) = %v, want -Inf", got)
	}
}

func TestLogSumExpNumericalStability(t *testing.T) {
	// Naive exp(1000) overflows. logSumExp must remain finite & accurate.
	got := logSumExp([]float64{1000, 1000})
	want := 1000 + math.Log(2)
	if math.IsInf(got, 0) || math.IsNaN(got) {
		t.Fatalf("logSumExp overflowed: got %v", got)
	}
	if math.Abs(got-want) > 1e-6 {
		t.Fatalf("logSumExp([1000,1000]) = %v, want %v", got, want)
	}
	// Very negative values shouldn't underflow to -Inf when at least one
	// pair is close in magnitude.
	got = logSumExp([]float64{-1000, -1000})
	want = -1000 + math.Log(2)
	if math.Abs(got-want) > 1e-6 {
		t.Fatalf("logSumExp([-1000,-1000]) = %v, want %v", got, want)
	}
}

func TestHMMWarmupSafe(t *testing.T) {
	// Below the (MinTrainBars + 10) bound, the strategy must return all
	// Holds — no panic, no out-of-bound reads.
	closes := make([]float64, 40)
	for i := range closes {
		closes[i] = 100 + float64(i)
	}
	bars := barsFromCloses(closes)
	signals := NewHMMStrategy().GenerateSignals(bars)
	if len(signals) != len(bars) {
		t.Fatalf("len = %d, want %d", len(signals), len(bars))
	}
	for i, s := range signals {
		if s != SignalHold {
			t.Fatalf("signal[%d] = %v, want Hold", i, s)
		}
	}
}

func TestHMMConvergesOnSyntheticTwoRegime(t *testing.T) {
	// Two synthetic regimes with very different means; Baum-Welch should
	// fit without numerical failure and return ok=true.
	obs := make([]float64, 200)
	for i := 0; i < 100; i++ {
		obs[i] = 0.01 + 0.001*math.Sin(float64(i))
	}
	for i := 100; i < 200; i++ {
		obs[i] = -0.01 + 0.001*math.Sin(float64(i))
	}
	pi, A, mu, sigma, ok := fitHMMGaussian(obs, 3, 50, 1e-4)
	if !ok {
		t.Fatal("fitHMMGaussian returned ok=false on a clean two-regime synthetic")
	}
	if len(pi) != 3 || len(A) != 3 || len(mu) != 3 || len(sigma) != 3 {
		t.Fatalf("returned dimensions wrong: pi=%d A=%d mu=%d sigma=%d",
			len(pi), len(A), len(mu), len(sigma))
	}
	// pi normalised
	var s float64
	for _, p := range pi {
		s += p
	}
	if math.Abs(s-1) > 1e-6 {
		t.Fatalf("pi not normalised: sum = %v", s)
	}
	// A rows normalised
	for i, row := range A {
		var rs float64
		for _, x := range row {
			rs += x
		}
		if math.Abs(rs-1) > 1e-6 {
			t.Fatalf("A row %d not normalised: sum = %v", i, rs)
		}
	}
	// fitted means must span both regimes (lowest < 0, highest > 0).
	minMu, maxMu := mu[0], mu[0]
	for _, m := range mu {
		if m < minMu {
			minMu = m
		}
		if m > maxMu {
			maxMu = m
		}
	}
	if minMu > -0.001 || maxMu < 0.001 {
		t.Fatalf("fitted means did not separate the two regimes: %v", mu)
	}
}

func TestHMMTradesAlignWithRegime(t *testing.T) {
	// 300-bar series with multiple regime flips so the HMM training window
	// (first 30% = 90 bars) sees BOTH an up and a down sub-regime — without
	// this variance during training the three hidden states collapse to
	// similar means and the model can't discriminate at inference time.
	// Held-out: bars 90..199 up regime, bars 200..299 down regime.
	closes := make([]float64, 300)
	closes[0] = 100
	drift := func(i int) float64 {
		switch {
		case i < 45 || (i >= 90 && i < 200):
			return 0.003
		default:
			return -0.003
		}
	}
	for i := 1; i < len(closes); i++ {
		r := drift(i) + 0.005*math.Sin(float64(i)*0.7)
		closes[i] = closes[i-1] * (1 + r)
	}
	bars := barsFromCloses(closes)
	signals := NewHMMStrategy().GenerateSignals(bars)

	var upBuys, upShorts, downBuys, downShorts int
	for i, s := range signals {
		switch {
		case i >= 95 && i < 195:
			if s == SignalBuy {
				upBuys++
			} else if s == SignalShort {
				upShorts++
			}
		case i >= 210 && i < 300:
			if s == SignalBuy {
				downBuys++
			} else if s == SignalShort {
				downShorts++
			}
		}
	}
	if upBuys == 0 {
		t.Fatal("HMM emitted no Buy signals during the held-out up regime")
	}
	if upBuys < upShorts {
		t.Fatalf("HMM more Shorts than Buys during up regime (%d vs %d)", upShorts, upBuys)
	}
	if downShorts == 0 {
		t.Fatal("HMM emitted no Short signals during the held-out down regime")
	}
	if downShorts < downBuys {
		t.Fatalf("HMM more Buys than Shorts during down regime (%d vs %d)", downBuys, downShorts)
	}
}

func TestHMMVarianceFloorSafe(t *testing.T) {
	// All-zero observations → degenerate variance — variance floor must
	// keep the fitter from dividing by zero or returning NaN.
	obs := make([]float64, 100)
	pi, A, mu, sigma, ok := fitHMMGaussian(obs, 3, 20, 1e-4)
	if !ok {
		t.Fatal("fitHMMGaussian returned ok=false on constant input")
	}
	for i := 0; i < 3; i++ {
		if !finite(pi[i]) || !finite(mu[i]) || !finite(sigma[i]) {
			t.Fatalf("non-finite parameter at state %d: pi=%v mu=%v sigma=%v",
				i, pi[i], mu[i], sigma[i])
		}
		if sigma[i] < 1e-4 {
			t.Fatalf("sigma[%d] = %v below variance-floor expectation", i, sigma[i])
		}
		for j := 0; j < 3; j++ {
			if !finite(A[i][j]) {
				t.Fatalf("non-finite A[%d][%d] = %v", i, j, A[i][j])
			}
		}
	}
}

// ── Registry ─────────────────────────────────────────────────────────────────

func TestRegistryIncludesMarkovStrategies(t *testing.T) {
	registerStrategies()
	names := map[string]bool{}
	for _, s := range availableStrategies() {
		names[s.Name()] = true
	}
	for _, want := range []string{"Markov Chain", "Markov Regime", "HMM"} {
		if !names[want] {
			t.Fatalf("registry missing %q", want)
		}
	}
}
