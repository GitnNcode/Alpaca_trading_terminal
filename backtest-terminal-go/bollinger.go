package main

// BollingerBands is the classic mean-reversion strategy on Bollinger Bands
// with an ADX trend-strength filter. It's intentionally distinct from
// VolBreakout (which uses the same bands to trade BREAKOUTS in the trend
// direction): here we fade extremes back toward the moving mean.
//
// Rules:
//
//	flat  + close < lower + ADX ≤ ADXMax → SignalBuy   (oversold, range-bound)
//	flat  + close > upper + ADX ≤ ADXMax → SignalShort (overbought, range-bound)
//	long  + close ≥ middle               → SignalSell  (reverted — exit)
//	short + close ≤ middle               → SignalSell  (reverted — exit)
//
// The ADX gate is essential: per the optimization notes, mean-reversion
// gets steamrolled in trending markets. ADX > 25 indicates a real trend;
// we skip new entries in that regime but still exit existing positions on
// mean reversion (the position already exists, so getting out at the mean
// is still the right move).
type BollingerBands struct {
	Period    int     // 20
	StdDev    float64 // 2.0
	ADXPeriod int     // 14
	ADXMax    float64 // 25.0 — skip new entries when ADX exceeds this
}

func NewBollingerBands(period int, stdDev float64) *BollingerBands {
	return &BollingerBands{Period: period, StdDev: stdDev, ADXPeriod: 14, ADXMax: 25.0}
}

func (b *BollingerBands) Name() string { return "Bollinger Bands" }

func (b *BollingerBands) GenerateSignals(bars []Bar) []Signal {
	signals := make([]Signal, len(bars))
	if len(bars) < b.Period+1 {
		return signals
	}

	closes := make([]float64, len(bars))
	for i, bar := range bars {
		closes[i] = bar.Close
	}
	upper, middle, lower := bb(closes, b.Period, b.StdDev)

	var adxSeries []float64
	adxFirstValid := 0
	if b.ADXPeriod > 0 {
		adxSeries = adx(bars, b.ADXPeriod)
		adxFirstValid = 2*b.ADXPeriod - 1
	}

	position := 0 // -1 short, 0 flat, +1 long
	for i := b.Period - 1; i < len(bars); i++ {
		price := closes[i]
		switch position {
		case 0:
			// Block new entries while in a trending regime. If ADX isn't
			// valid yet (warm-up), the safe default is to hold.
			if b.ADXPeriod > 0 {
				if i < adxFirstValid || adxSeries[i] > b.ADXMax {
					continue
				}
			}
			if price < lower[i] {
				signals[i] = SignalBuy
				position = 1
			} else if price > upper[i] {
				signals[i] = SignalShort
				position = -1
			}
		case 1:
			if price >= middle[i] {
				signals[i] = SignalSell
				position = 0
			}
		case -1:
			if price <= middle[i] {
				signals[i] = SignalSell
				position = 0
			}
		}
	}
	return signals
}
