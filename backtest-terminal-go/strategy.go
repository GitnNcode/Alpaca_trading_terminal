package main

import "math"

// Signal is the trading action emitted by a Strategy at each bar.
type Signal int

const (
	SignalHold Signal = iota
	SignalBuy        // target = long position
	SignalSell       // target = flat (closes long OR short — universal flatten)
	SignalShort      // target = short position
)

// Strategy turns a series of OHLCV bars into a parallel series of trading
// signals. signals[i] is the action to take at the close of bars[i]; the
// simulation engine executes at bars[i+1] open (or close — see backtest.go)
// to avoid look-ahead bias.
//
// To add a new strategy: implement this interface and append a constructor to
// the registry in registerStrategies().
type Strategy interface {
	Name() string
	GenerateSignals(bars []Bar) []Signal
}

// strategyRegistry is the plug-in table the UI reads to populate its dropdown.
// Constructors (rather than instances) keep strategies stateless across runs.
var strategyRegistry []func() Strategy

func registerStrategies() {
	strategyRegistry = []func() Strategy{
		func() Strategy { return NewMACD(12, 26, 9) },
		func() Strategy { return NewMACDRSI(12, 26, 9, 14, 50.0) },
		func() Strategy { return NewVolBreakout(20, 2.0, 50, 14, 2.0) },
		func() Strategy { return NewBollingerBands(20, 2.0) },
		func() Strategy { return NewMarkovChain() },
		func() Strategy { return NewMarkovRegimeSwitch() },
		func() Strategy { return NewHMMStrategy() },
	}
}

func availableStrategies() []Strategy {
	out := make([]Strategy, 0, len(strategyRegistry))
	for _, c := range strategyRegistry {
		out = append(out, c())
	}
	return out
}

// ema returns a series the same length as values. The first (period-1) entries
// are seeded with NaN-equivalent zero and should not be treated as valid; the
// entry at index period-1 is the SMA seed and entries beyond use the standard
// exponential smoothing formula.
func ema(values []float64, period int) []float64 {
	out := make([]float64, len(values))
	if period <= 0 || len(values) < period {
		return out
	}
	alpha := 2.0 / float64(period+1)

	// SMA seed at index period-1
	sum := 0.0
	for i := 0; i < period; i++ {
		sum += values[i]
	}
	out[period-1] = sum / float64(period)

	for i := period; i < len(values); i++ {
		out[i] = (values[i]-out[i-1])*alpha + out[i-1]
	}
	return out
}

// ── MACD ─────────────────────────────────────────────────────────────────────

// MACD is the classic moving-average-convergence-divergence crossover with
// a long-term SMA trend filter (Appel + Raschke's textbook combination).
// Long-only: bullish crossover (MACD crosses above signal) AND close above
// TrendSMA → buy; bearish crossover → sell (unconditional, the trend
// filter does not gate exits). The trend filter is the single biggest
// edge-improver per the optimization notes: raw crossovers without it have
// ~50% win rate and negative expectancy after costs.
type MACD struct {
	Fast, Slow, Signal int
	TrendSMA           int // 200 — disables Buy signals while close ≤ SMA(TrendSMA)
}

func NewMACD(fast, slow, signal int) *MACD {
	return &MACD{Fast: fast, Slow: slow, Signal: signal, TrendSMA: 200}
}

func (m *MACD) Name() string { return "MACD" }

func (m *MACD) GenerateSignals(bars []Bar) []Signal {
	signals := make([]Signal, len(bars))
	if len(bars) < m.Slow+m.Signal {
		return signals
	}

	closes := make([]float64, len(bars))
	for i, b := range bars {
		closes[i] = b.Close
	}

	emaFast := ema(closes, m.Fast)
	emaSlow := ema(closes, m.Slow)

	// MACD line valid from index slow-1 onward
	macdLine := make([]float64, len(bars))
	for i := m.Slow - 1; i < len(bars); i++ {
		macdLine[i] = emaFast[i] - emaSlow[i]
	}

	// Signal line = EMA(signal) of MACD line, computed only over the valid range
	macdValid := macdLine[m.Slow-1:]
	sigValid := ema(macdValid, m.Signal)
	signalLine := make([]float64, len(bars))
	for i := range sigValid {
		signalLine[m.Slow-1+i] = sigValid[i]
	}

	// Optional long-term trend filter. We still compute it for short
	// windows so the filter degrades gracefully — if there aren't enough
	// bars for SMA(TrendSMA), it's all zeros and no Buys ever pass the
	// `close > trendSMA[i]` test, which matches the conservative intent.
	var trendSMA []float64
	if m.TrendSMA > 0 {
		trendSMA = sma(closes, m.TrendSMA)
	}

	// First index where BOTH macd and signal are valid:
	// macd valid from slow-1, signal seeded at slow-1+(signal-1) = slow+signal-2
	firstValid := m.Slow + m.Signal - 2
	if firstValid < 1 {
		firstValid = 1
	}

	for i := firstValid + 1; i < len(bars); i++ {
		prevDiff := macdLine[i-1] - signalLine[i-1]
		currDiff := macdLine[i] - signalLine[i]
		switch {
		case prevDiff <= 0 && currDiff > 0:
			// Bullish crossover — gate on the long-term trend filter.
			if m.TrendSMA <= 0 || (i >= m.TrendSMA-1 && closes[i] > trendSMA[i]) {
				signals[i] = SignalBuy
			}
		case prevDiff >= 0 && currDiff < 0:
			signals[i] = SignalSell
		}
	}
	return signals
}

// ── RSI ──────────────────────────────────────────────────────────────────────

// rsi returns the Relative Strength Index using Wilder's smoothing — the same
// definition used by virtually every charting platform (TradingView, etc.).
// Warm-up entries (i < period) are filled with 50.0 (neutral) so callers can
// index uniformly; a strategy must still ignore those positions explicitly.
func rsi(closes []float64, period int) []float64 {
	out := make([]float64, len(closes))
	for i := range out {
		out[i] = 50.0
	}
	if period <= 0 || len(closes) < period+1 {
		return out
	}

	var avgGain, avgLoss float64
	for i := 1; i <= period; i++ {
		change := closes[i] - closes[i-1]
		if change > 0 {
			avgGain += change
		} else {
			avgLoss -= change
		}
	}
	avgGain /= float64(period)
	avgLoss /= float64(period)

	writeRSI := func(idx int) {
		switch {
		case avgLoss == 0 && avgGain == 0:
			out[idx] = 50.0
		case avgLoss == 0:
			out[idx] = 100.0
		default:
			rs := avgGain / avgLoss
			out[idx] = 100.0 - 100.0/(1.0+rs)
		}
	}
	writeRSI(period)

	for i := period + 1; i < len(closes); i++ {
		change := closes[i] - closes[i-1]
		gain, loss := 0.0, 0.0
		if change > 0 {
			gain = change
		} else {
			loss = -change
		}
		avgGain = (avgGain*float64(period-1) + gain) / float64(period)
		avgLoss = (avgLoss*float64(period-1) + loss) / float64(period)
		writeRSI(i)
	}
	return out
}

// ── MACD + RSI ───────────────────────────────────────────────────────────────

// MACDRSI is MACD crossovers confirmed by RSI momentum. The original gate
// (block Buy when RSI ≥ 70 ceiling) only marginally beat plain MACD because
// it removes very few signals in practice. The momentum-confirmation form
// flips the test: require RSI to be above the floor AND rising, i.e. the
// crossover is happening with measurable upside momentum already in place.
// This matches the "Version A — Confirmation" rule from the optimization
// notes and is what produces the documented win-rate lift.
//
// Bearish crossovers exit unconditionally (the trend filter on the parent
// MACD already handles direction; the RSI gate only protects entries).
type MACDRSI struct {
	Fast, Slow, Signal int
	RSIPeriod          int
	RSIFloor           float64 // 50.0 — minimum RSI for a Buy; RSI must also be rising
}

func NewMACDRSI(fast, slow, signal, rsiPeriod int, rsiFloor float64) *MACDRSI {
	return &MACDRSI{Fast: fast, Slow: slow, Signal: signal, RSIPeriod: rsiPeriod, RSIFloor: rsiFloor}
}

func (m *MACDRSI) Name() string { return "MACD+RSI" }

func (m *MACDRSI) GenerateSignals(bars []Bar) []Signal {
	// Lean on the existing MACD implementation (which already carries its
	// own trend filter) so the crossover logic stays in one place. The RSI
	// momentum gate is layered on top, Buy-only.
	base := NewMACD(m.Fast, m.Slow, m.Signal).GenerateSignals(bars)
	if len(bars) < m.RSIPeriod+2 {
		return base
	}

	closes := make([]float64, len(bars))
	for i, b := range bars {
		closes[i] = b.Close
	}
	rsiSeries := rsi(closes, m.RSIPeriod)

	for i := range base {
		if base[i] != SignalBuy {
			continue
		}
		if i <= m.RSIPeriod {
			base[i] = SignalHold
			continue
		}
		rising := rsiSeries[i] > rsiSeries[i-1]
		if rsiSeries[i] <= m.RSIFloor || !rising {
			base[i] = SignalHold
		}
	}
	return base
}

// ── SMA ──────────────────────────────────────────────────────────────────────

// sma returns a trailing simple moving average of `values` with the given
// period. The first (period-1) entries are 0 — callers must treat indices
// below period-1 as warm-up. From period-1 onward, out[i] = mean(values[i-period+1 .. i]).
func sma(values []float64, period int) []float64 {
	out := make([]float64, len(values))
	if period <= 0 || len(values) < period {
		return out
	}
	sum := 0.0
	for i := 0; i < period; i++ {
		sum += values[i]
	}
	out[period-1] = sum / float64(period)
	for i := period; i < len(values); i++ {
		sum += values[i] - values[i-period]
		out[i] = sum / float64(period)
	}
	return out
}

// ── Bollinger Bands ──────────────────────────────────────────────────────────

// bb returns (upper, middle, lower) Bollinger Band series. Middle is SMA over
// `period`; upper/lower are middle ± stdDev * population standard deviation
// over the same window. First period-1 entries are 0 (warm-up).
func bb(closes []float64, period int, stdDev float64) (upper, middle, lower []float64) {
	upper = make([]float64, len(closes))
	lower = make([]float64, len(closes))
	middle = sma(closes, period)
	if period <= 0 || len(closes) < period {
		return
	}
	for i := period - 1; i < len(closes); i++ {
		var sumSq float64
		for j := i - period + 1; j <= i; j++ {
			d := closes[j] - middle[i]
			sumSq += d * d
		}
		sd := math.Sqrt(sumSq / float64(period))
		upper[i] = middle[i] + stdDev*sd
		lower[i] = middle[i] - stdDev*sd
	}
	return
}

// ── ATR (Average True Range) ─────────────────────────────────────────────────

// atr returns the Wilder-smoothed Average True Range over `period`. True range
// is max(high-low, |high-prevClose|, |low-prevClose|). First period-1 entries
// are 0 (warm-up); the seed at index period-1 is the SMA of the first period
// true ranges; subsequent values use Wilder's exponential smoothing with
// alpha = 1/period.
func atr(bars []Bar, period int) []float64 {
	out := make([]float64, len(bars))
	if period <= 0 || len(bars) < period+1 {
		return out
	}
	tr := make([]float64, len(bars))
	tr[0] = bars[0].High - bars[0].Low
	for i := 1; i < len(bars); i++ {
		hl := bars[i].High - bars[i].Low
		hc := math.Abs(bars[i].High - bars[i-1].Close)
		lc := math.Abs(bars[i].Low - bars[i-1].Close)
		tr[i] = math.Max(hl, math.Max(hc, lc))
	}
	var sum float64
	for i := 0; i < period; i++ {
		sum += tr[i]
	}
	out[period-1] = sum / float64(period)
	for i := period; i < len(bars); i++ {
		out[i] = (out[i-1]*float64(period-1) + tr[i]) / float64(period)
	}
	return out
}

// ── Vol Breakout (BB + EMA trend + ATR trailing stop) ───────────────────────

// VolBreakout enters on Bollinger Band breakouts that align with the medium-
// term EMA trend, and exits via an ATR-based trailing stop or a trend break.
// It can short on the symmetric setup, so it makes money in both directions.
//
// Why this should outperform a vanilla MACD strategy:
//   - BB breakout fires when price moves ≥ BBStdDev standard deviations past
//     the moving mean — typically EARLIER than a MACD crossover, which lags.
//   - The ATR trailing stop adapts to volatility: tight stops in calm
//     markets, wide stops in volatile ones. MACD's reversal cross is fixed
//     and slow, often giving back a chunk of the move before exiting.
//   - The EMA(TrendEMA) filter rejects counter-trend setups, cutting the
//     whipsaw rate that's MACD's main weakness in choppy markets.
//
// Position state lives inside GenerateSignals (not in the struct) because
// signals must be reproducible from bars alone — callers may rerun against
// the same bars and expect identical output.
type VolBreakout struct {
	BBPeriod    int     // 20
	BBStdDev    float64 // 2.0
	TrendEMA    int     // 50 — trend-direction filter (longer than entry window)
	ATRPeriod   int     // 14
	StopMult    float64 // 2.0 — trailing-stop distance = StopMult × ATR
	ATRSMAPeriod int    // 20 — window for the ATR-rising filter (entries only fire when current ATR exceeds its SMA)
}

func NewVolBreakout(bbPeriod int, bbStdDev float64, trendEMA, atrPeriod int, stopMult float64) *VolBreakout {
	return &VolBreakout{
		BBPeriod: bbPeriod, BBStdDev: bbStdDev,
		TrendEMA: trendEMA, ATRPeriod: atrPeriod, StopMult: stopMult,
		ATRSMAPeriod: 20,
	}
}

func (v *VolBreakout) Name() string { return "Vol Breakout" }

func (v *VolBreakout) GenerateSignals(bars []Bar) []Signal {
	signals := make([]Signal, len(bars))
	firstValid := v.TrendEMA - 1
	if x := v.BBPeriod - 1; x > firstValid {
		firstValid = x
	}
	if x := v.ATRPeriod; x > firstValid {
		firstValid = x
	}
	if v.ATRSMAPeriod > 0 {
		if x := v.ATRPeriod + v.ATRSMAPeriod - 1; x > firstValid {
			firstValid = x
		}
	}
	if len(bars) <= firstValid+1 {
		return signals
	}

	closes := make([]float64, len(bars))
	for i, b := range bars {
		closes[i] = b.Close
	}
	upper, _, lower := bb(closes, v.BBPeriod, v.BBStdDev)
	trendEMA := ema(closes, v.TrendEMA)
	atrSeries := atr(bars, v.ATRPeriod)

	// ATR-rising filter: only trade when volatility is expanding (current
	// ATR above its rolling mean). On flat / contracting vol the false-
	// breakout rate dominates and the strategy bleeds through whipsaws.
	var atrMA []float64
	if v.ATRSMAPeriod > 0 {
		atrMA = sma(atrSeries, v.ATRSMAPeriod)
	}

	// Local position state. -1 = short, 0 = flat, +1 = long.
	position := 0
	var entryHigh, entryLow float64

	for i := firstValid + 1; i < len(bars); i++ {
		price := closes[i]
		bull := price > trendEMA[i]
		bear := price < trendEMA[i]
		volExpanding := v.ATRSMAPeriod <= 0 || atrSeries[i] > atrMA[i]

		switch position {
		case 0:
			if !volExpanding {
				continue // hold — vol contracting, breakouts are unreliable
			}
			// Flat — look for breakout aligned with trend.
			if bull && price > upper[i] {
				signals[i] = SignalBuy
				position = 1
				entryHigh = price
			} else if bear && price < lower[i] {
				signals[i] = SignalShort
				position = -1
				entryLow = price
			}
		case 1:
			// Long — track running high, exit on trailing stop or trend break.
			if price > entryHigh {
				entryHigh = price
			}
			stop := entryHigh - v.StopMult*atrSeries[i]
			if price <= stop || price < trendEMA[i] {
				signals[i] = SignalSell
				position = 0
			}
		case -1:
			// Short — symmetric.
			if price < entryLow {
				entryLow = price
			}
			stop := entryLow + v.StopMult*atrSeries[i]
			if price >= stop || price > trendEMA[i] {
				signals[i] = SignalSell
				position = 0
			}
		}
	}
	return signals
}
