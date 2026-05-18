package main

import "math"

// returnsFromBars produces simple per-bar returns parallel to `bars`. The
// first entry is 0 (no prior bar). Used by the HMM and as a building block
// for rolling N-bar returns.
func returnsFromBars(bars []Bar) []float64 {
	out := make([]float64, len(bars))
	for i := 1; i < len(bars); i++ {
		prev := bars[i-1].Close
		if prev <= 0 {
			continue
		}
		out[i] = (bars[i].Close - prev) / prev
	}
	return out
}

// rollingReturns produces N-bar lookback returns: out[i] = (close[i] -
// close[i-window]) / close[i-window]. Entries before `window` are 0 and
// should be treated as warm-up. Multi-day rolling returns are noticeably
// more persistent than 1-bar returns, which is the chief problem with a
// vanilla 1-bar Markov chain on daily prices.
func rollingReturns(bars []Bar, window int) []float64 {
	out := make([]float64, len(bars))
	if window <= 0 {
		return out
	}
	for i := window; i < len(bars); i++ {
		prev := bars[i-window].Close
		if prev <= 0 {
			continue
		}
		out[i] = (bars[i].Close - prev) / prev
	}
	return out
}

// classifyReturnState bins a single return into {0=Down, 1=Flat, 2=Up}
// relative to the supplied rolling stddev. mult is the half-width of the
// Flat band measured in sigmas. Caller passes a small epsilon for sigma
// when prices are constant so everything classifies Flat instead of
// dividing by zero.
func classifyReturnState(r, sigma, mult float64) int {
	band := mult * sigma
	switch {
	case r < -band:
		return 0
	case r > band:
		return 2
	default:
		return 1
	}
}

// MarkovChain is a 3-state discrete Markov chain over MULTI-DAY rolling
// returns (5-day default) with a confidence-threshold trade gate and a
// 2-bar signal-persistence requirement. The classic 1-bar form generates
// ~110 trades/year on daily data because 1-day returns aren't very Markov
// — costs eat any edge. Using a longer rolling return increases state
// persistence; requiring the same predicted direction for 2 consecutive
// bars filters another ~80% of the residual noise.
//
// The transition matrix is estimated walk-forward (only from bars[0:i]),
// so the strategy stays lookahead-free.
//
// Decision rule:
//
//	predicted next state = argmax over the row indexed by the previous
//	bar's state. Only act when the argmax probability exceeds
//	`ConfThreshold` (default 0.55 — meaningfully above the 1/3 uniform
//	baseline) AND the same direction was predicted on the prior bar
//	(persistence). A predicted Flat never flattens an open position —
//	whipsaw cost in choppy regimes is too high.
type MarkovChain struct {
	StateCount    int     // 3
	ReturnWindow  int     // 5 — rolling-return window (multi-day for persistence)
	FlatSigmaMult float64 // 0.3 — half-width of the Flat band, in sigmas
	ConfThreshold float64 // 0.55 — required probability for the argmax row entry
	Persistence   int     // 2 — required consecutive bars predicting the same direction
	WarmupBars    int     // 60 — bars before any signal is emitted
}

func NewMarkovChain() *MarkovChain {
	return &MarkovChain{
		StateCount:    3,
		ReturnWindow:  5,
		FlatSigmaMult: 0.3,
		ConfThreshold: 0.55,
		Persistence:   2,
		WarmupBars:    60,
	}
}

func (m *MarkovChain) Name() string { return "Markov Chain" }

func (m *MarkovChain) GenerateSignals(bars []Bar) []Signal {
	signals := make([]Signal, len(bars))
	if len(bars) < m.WarmupBars+m.ReturnWindow+2 {
		return signals
	}

	rets := rollingReturns(bars, m.ReturnWindow)
	states := make([]int, len(bars))
	firstRet := m.ReturnWindow

	// Rolling sums for walk-forward sigma over rets[firstRet:i+1].
	var sum, sumSq float64
	var n int

	// Pre-fill the warmup range so the transition counts at the trade
	// boundary already have meaningful history.
	warmupEnd := m.WarmupBars + firstRet
	if warmupEnd > len(bars) {
		warmupEnd = len(bars)
	}
	for i := firstRet; i < warmupEnd; i++ {
		r := rets[i]
		sum += r
		sumSq += r * r
		n++
		mean := sum / float64(n)
		variance := sumSq/float64(n) - mean*mean
		if variance < 0 {
			variance = 0
		}
		sigma := math.Sqrt(variance)
		if sigma < 1e-9 {
			sigma = 1e-9
		}
		states[i] = classifyReturnState(r, sigma, m.FlatSigmaMult)
	}

	// Tally initial transition counts from the warmup states.
	counts := [3][3]int{}
	for i := firstRet + 1; i < warmupEnd; i++ {
		counts[states[i-1]][states[i]]++
	}

	// predictDir tracks the previously predicted direction so we can
	// enforce a 2-bar persistence requirement before emitting a trade.
	// Values: -1 = predict Down, 0 = no prediction / Flat, +1 = Up.
	prevPred, runLen := 0, 0

	for i := warmupEnd; i < len(bars); i++ {
		prevState := states[i-1]
		row := counts[prevState]
		total := row[0] + row[1] + row[2]

		var probs [3]float64
		if total == 0 {
			probs = [3]float64{1.0 / 3, 1.0 / 3, 1.0 / 3}
		} else {
			t := float64(total)
			probs[0] = float64(row[0]) / t
			probs[1] = float64(row[1]) / t
			probs[2] = float64(row[2]) / t
		}

		argmax, best := 0, probs[0]
		for k := 1; k < 3; k++ {
			if probs[k] > best {
				best = probs[k]
				argmax = k
			}
		}

		thisPred := 0
		if best >= m.ConfThreshold {
			switch argmax {
			case 2:
				thisPred = 1
			case 0:
				thisPred = -1
			}
		}

		if thisPred != 0 && thisPred == prevPred {
			runLen++
		} else {
			runLen = 1
		}
		if thisPred != 0 && runLen >= m.Persistence {
			if thisPred == 1 {
				signals[i] = SignalBuy
			} else {
				signals[i] = SignalShort
			}
		}
		prevPred = thisPred

		// Advance the rolling sigma to include rets[i], then classify and
		// tally the transition that just became known.
		r := rets[i]
		sum += r
		sumSq += r * r
		n++
		mean := sum / float64(n)
		variance := sumSq/float64(n) - mean*mean
		if variance < 0 {
			variance = 0
		}
		sigma := math.Sqrt(variance)
		if sigma < 1e-9 {
			sigma = 1e-9
		}
		states[i] = classifyReturnState(r, sigma, m.FlatSigmaMult)
		counts[states[i-1]][states[i]]++
	}

	return signals
}
