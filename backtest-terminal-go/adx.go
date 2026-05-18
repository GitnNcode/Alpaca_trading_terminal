package main

import "math"

// adx returns the Wilder-smoothed Average Directional Index over `period`.
// Output is parallel to bars; the first 2*period-1 entries are warm-up and
// should be treated as 0 by callers.
//
// Construction (Wilder 1978):
//
//	+DM[i] = max(0, high[i]-high[i-1])  when high[i]-high[i-1] > low[i-1]-low[i]
//	-DM[i] = max(0, low[i-1]-low[i])    when low[i-1]-low[i]  > high[i]-high[i-1]
//	TR[i]  = max(high-low, |high-prevClose|, |low-prevClose|)
//	Smoothed series use Wilder smoothing: seed = sum of first `period`
//	values, then S[i] = S[i-1] - S[i-1]/period + x[i].
//	+DI = 100 * smoothed(+DM) / smoothed(TR)
//	-DI = 100 * smoothed(-DM) / smoothed(TR)
//	DX  = 100 * |+DI − −DI| / (+DI + −DI)
//	ADX = Wilder smoothed DX
//
// ADX measures TREND STRENGTH irrespective of direction. Below ~20 is
// generally choppy/range-bound; above ~25 indicates a real trend. The
// Bollinger Bands mean-reversion strategy uses ADX > 25 as a skip-entry
// filter — mean-reversion gets crushed in trending markets.
func adx(bars []Bar, period int) []float64 {
	out := make([]float64, len(bars))
	if period <= 0 || len(bars) < 2*period+1 {
		return out
	}

	plusDM := make([]float64, len(bars))
	minusDM := make([]float64, len(bars))
	tr := make([]float64, len(bars))
	for i := 1; i < len(bars); i++ {
		upMove := bars[i].High - bars[i-1].High
		downMove := bars[i-1].Low - bars[i].Low
		if upMove > downMove && upMove > 0 {
			plusDM[i] = upMove
		}
		if downMove > upMove && downMove > 0 {
			minusDM[i] = downMove
		}
		hl := bars[i].High - bars[i].Low
		hc := math.Abs(bars[i].High - bars[i-1].Close)
		lc := math.Abs(bars[i].Low - bars[i-1].Close)
		tr[i] = math.Max(hl, math.Max(hc, lc))
	}

	smPlus := make([]float64, len(bars))
	smMinus := make([]float64, len(bars))
	smTR := make([]float64, len(bars))
	// Wilder seed = sum of first `period` values (indices 1..period).
	for i := 1; i <= period; i++ {
		smPlus[period] += plusDM[i]
		smMinus[period] += minusDM[i]
		smTR[period] += tr[i]
	}
	for i := period + 1; i < len(bars); i++ {
		smPlus[i] = smPlus[i-1] - smPlus[i-1]/float64(period) + plusDM[i]
		smMinus[i] = smMinus[i-1] - smMinus[i-1]/float64(period) + minusDM[i]
		smTR[i] = smTR[i-1] - smTR[i-1]/float64(period) + tr[i]
	}

	dx := make([]float64, len(bars))
	for i := period; i < len(bars); i++ {
		if smTR[i] == 0 {
			continue
		}
		plusDI := 100 * smPlus[i] / smTR[i]
		minusDI := 100 * smMinus[i] / smTR[i]
		sum := plusDI + minusDI
		if sum == 0 {
			continue
		}
		dx[i] = 100 * math.Abs(plusDI-minusDI) / sum
	}

	// ADX = Wilder smoothed DX, with seed = mean of first `period` DX
	// values (indices period..2*period-1).
	var seed float64
	for i := period; i < 2*period; i++ {
		seed += dx[i]
	}
	out[2*period-1] = seed / float64(period)
	for i := 2 * period; i < len(bars); i++ {
		out[i] = (out[i-1]*float64(period-1) + dx[i]) / float64(period)
	}
	return out
}
