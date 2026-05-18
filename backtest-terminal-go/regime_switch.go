package main

// MarkovRegimeSwitch classifies each bar into a Bull/Bear/Choppy regime
// using an EMA-slope/ATR ratio (scale-free across assets), then applies a
// Markov persistence filter so 1-bar regime flickers don't trigger trades.
//
// Regime classifier per bar:
//
//	slope = (ema[i] - ema[i-SlopeLookback]) / SlopeLookback / atr[i]
//	slope > +SlopeThresh  → Bull
//	slope < -SlopeThresh  → Bear
//	otherwise              → Choppy
//
// Markov layer: maintain a 3x3 transition-count matrix walk-forward over
// the regime sequence. Only emit a trade on bar i when BOTH
//   - raw[i] == raw[i-1] (raw regime persisted across one bar)
//   - the most-likely transition from the row indexed by raw[i-1] equals
//     raw[i]  (the model agrees the persistence is "real")
//
// Mapping: Bull → SignalBuy, Bear → SignalShort, Choppy → SignalHold
// (preserves the current position rather than flattening).
type MarkovRegimeSwitch struct {
	TrendEMA      int     // 20
	SlopeLookback int     // 5
	ATRPeriod     int     // 14
	SlopeThresh   float64 // 0.15 — minimum |slope| (ATR-normalized) for a directional regime
}

func NewMarkovRegimeSwitch() *MarkovRegimeSwitch {
	return &MarkovRegimeSwitch{TrendEMA: 20, SlopeLookback: 5, ATRPeriod: 14, SlopeThresh: 0.15}
}

func (m *MarkovRegimeSwitch) Name() string { return "Markov Regime" }

const (
	regimeBear   = 0
	regimeChoppy = 1
	regimeBull   = 2
)

func (m *MarkovRegimeSwitch) GenerateSignals(bars []Bar) []Signal {
	signals := make([]Signal, len(bars))

	// First index at which all required inputs are valid:
	// - ema valid from TrendEMA-1
	// - slope needs another SlopeLookback bars of EMA history
	// - atr valid from ATRPeriod-1 (with seed)
	firstValid := m.TrendEMA - 1 + m.SlopeLookback
	if x := m.ATRPeriod; x > firstValid {
		firstValid = x
	}
	if len(bars) <= firstValid+2 {
		return signals
	}

	closes := make([]float64, len(bars))
	for i, b := range bars {
		closes[i] = b.Close
	}
	emaSeries := ema(closes, m.TrendEMA)
	atrSeries := atr(bars, m.ATRPeriod)

	regimes := make([]int, len(bars))
	for i := range regimes {
		regimes[i] = regimeChoppy
	}

	classify := func(i int) int {
		a := atrSeries[i]
		if a <= 0 {
			return regimeChoppy
		}
		slope := (emaSeries[i] - emaSeries[i-m.SlopeLookback]) / float64(m.SlopeLookback) / a
		switch {
		case slope > m.SlopeThresh:
			return regimeBull
		case slope < -m.SlopeThresh:
			return regimeBear
		default:
			return regimeChoppy
		}
	}

	// Pre-fill regimes up to firstValid so the transition counts at the
	// trade boundary have something to work with.
	for i := firstValid; i < len(bars); i++ {
		regimes[i] = classify(i)
	}

	counts := [3][3]int{}
	// Tally transitions strictly from history that is already classified
	// by firstValid+1. The first usable trade index is firstValid+2 (needs
	// raw[i] and raw[i-1] plus a transition row for prediction).
	for i := firstValid + 1; i < len(bars); i++ {
		// Predict using row indexed by regimes[i-1].
		row := counts[regimes[i-1]]
		total := row[0] + row[1] + row[2]

		argmax := regimes[i-1] // default to staying when no history
		if total > 0 {
			best := row[0]
			argmax = 0
			for k := 1; k < 3; k++ {
				if row[k] > best {
					best = row[k]
					argmax = k
				}
			}
		}

		// Only act when raw persistence (this bar same as prev) is
		// confirmed by the Markov prediction (predicted transition lands
		// on the persisted regime).
		if regimes[i] == regimes[i-1] && argmax == regimes[i] {
			switch regimes[i] {
			case regimeBull:
				signals[i] = SignalBuy
			case regimeBear:
				signals[i] = SignalShort
			}
		}

		// Tally the transition that just became known.
		counts[regimes[i-1]][regimes[i]]++
	}

	return signals
}
