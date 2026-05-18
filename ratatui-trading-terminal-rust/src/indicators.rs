// Technical-indicator math for the chart tab. Each function takes a slice of
// bars and a period; returns one or more `Vec<f64>` of the same length as the
// input where pre-warmup values are `NaN` (so the renderer can skip them
// cleanly with `is_nan()`).
//
// Conventions mirror TradingView's defaults:
//   * EMA / SMA / Bollinger seed from the first `period` closes (SMA).
//   * RSI uses Wilder's smoothing (typical period 14).
//   * MACD uses fast=12, slow=26, signal=9 EMAs.
//   * VWAP is cumulative from the first bar in `bars` — TradingView resets it
//     daily; that's a UI concern handled where the indicator is rendered.

use crate::api::Bar;

pub fn compute_sma(bars: &[Bar], period: usize) -> Vec<f64> {
    let mut out = vec![f64::NAN; bars.len()];
    if period == 0 || bars.len() < period {
        return out;
    }
    let mut sum: f64 = bars[..period].iter().map(|b| b.close).sum();
    out[period - 1] = sum / period as f64;
    for i in period..bars.len() {
        sum += bars[i].close - bars[i - period].close;
        out[i] = sum / period as f64;
    }
    out
}

pub fn compute_ema(bars: &[Bar], period: usize) -> Vec<f64> {
    let mut out = vec![f64::NAN; bars.len()];
    if period == 0 || bars.len() < period {
        return out;
    }
    let seed: f64 = bars[..period].iter().map(|b| b.close).sum::<f64>() / period as f64;
    out[period - 1] = seed;
    let k = 2.0 / (period as f64 + 1.0);
    for i in period..bars.len() {
        out[i] = bars[i].close * k + out[i - 1] * (1.0 - k);
    }
    out
}

/// Returns (upper_band, middle_band, lower_band). middle_band = SMA(period).
/// Bands are middle ± `mult` × stdev(closes over window).
pub fn compute_bollinger(
    bars: &[Bar],
    period: usize,
    mult: f64,
) -> (Vec<f64>, Vec<f64>, Vec<f64>) {
    let n = bars.len();
    let mut upper = vec![f64::NAN; n];
    let mut middle = vec![f64::NAN; n];
    let mut lower = vec![f64::NAN; n];
    if period == 0 || n < period {
        return (upper, middle, lower);
    }
    for i in (period - 1)..n {
        let window = &bars[i + 1 - period..=i];
        let mean: f64 = window.iter().map(|b| b.close).sum::<f64>() / period as f64;
        let var: f64 =
            window.iter().map(|b| (b.close - mean).powi(2)).sum::<f64>() / period as f64;
        let sd = var.sqrt();
        middle[i] = mean;
        upper[i] = mean + mult * sd;
        lower[i] = mean - mult * sd;
    }
    (upper, middle, lower)
}

/// Wilder's RSI, 0..100 scale. NaN until index `period`.
pub fn compute_rsi(bars: &[Bar], period: usize) -> Vec<f64> {
    let n = bars.len();
    let mut out = vec![f64::NAN; n];
    if period == 0 || n < period + 1 {
        return out;
    }
    let mut gain = 0.0;
    let mut loss = 0.0;
    for i in 1..=period {
        let d = bars[i].close - bars[i - 1].close;
        if d >= 0.0 {
            gain += d;
        } else {
            loss -= d;
        }
    }
    let mut avg_gain = gain / period as f64;
    let mut avg_loss = loss / period as f64;
    out[period] = rsi_from(avg_gain, avg_loss);
    for i in (period + 1)..n {
        let d = bars[i].close - bars[i - 1].close;
        let (g, l) = if d >= 0.0 { (d, 0.0) } else { (0.0, -d) };
        // Wilder's smoothing: prior average × (period-1) + new sample, all / period.
        avg_gain = (avg_gain * (period as f64 - 1.0) + g) / period as f64;
        avg_loss = (avg_loss * (period as f64 - 1.0) + l) / period as f64;
        out[i] = rsi_from(avg_gain, avg_loss);
    }
    out
}

fn rsi_from(avg_gain: f64, avg_loss: f64) -> f64 {
    if avg_loss == 0.0 {
        return 100.0;
    }
    let rs = avg_gain / avg_loss;
    100.0 - 100.0 / (1.0 + rs)
}

/// MACD = EMA(fast) - EMA(slow). Signal = EMA(MACD, signal_period).
/// Histogram = MACD - Signal. Returns (macd, signal, histogram).
pub fn compute_macd(
    bars: &[Bar],
    fast: usize,
    slow: usize,
    signal: usize,
) -> (Vec<f64>, Vec<f64>, Vec<f64>) {
    let fast_ema = compute_ema(bars, fast);
    let slow_ema = compute_ema(bars, slow);
    let n = bars.len();
    let mut macd = vec![f64::NAN; n];
    for i in 0..n {
        if !fast_ema[i].is_nan() && !slow_ema[i].is_nan() {
            macd[i] = fast_ema[i] - slow_ema[i];
        }
    }
    // Signal = EMA over macd[]. Implement directly since compute_ema wants Bars.
    let mut sig = vec![f64::NAN; n];
    // First non-NaN index in macd:
    let start = macd.iter().position(|v| !v.is_nan()).unwrap_or(n);
    if start + signal <= n && signal > 0 {
        let seed: f64 = macd[start..start + signal].iter().sum::<f64>() / signal as f64;
        let seed_idx = start + signal - 1;
        sig[seed_idx] = seed;
        let k = 2.0 / (signal as f64 + 1.0);
        for i in (seed_idx + 1)..n {
            if macd[i].is_nan() {
                continue;
            }
            sig[i] = macd[i] * k + sig[i - 1] * (1.0 - k);
        }
    }
    let mut hist = vec![f64::NAN; n];
    for i in 0..n {
        if !macd[i].is_nan() && !sig[i].is_nan() {
            hist[i] = macd[i] - sig[i];
        }
    }
    (macd, sig, hist)
}

/// Cumulative VWAP from `bars[0]`. Typical price × volume, cumulative ÷
/// cumulative volume. (Daily-reset variant is the caller's responsibility.)
pub fn compute_vwap(bars: &[Bar]) -> Vec<f64> {
    let mut out = vec![f64::NAN; bars.len()];
    let mut cum_pv = 0.0;
    let mut cum_v: f64 = 0.0;
    for (i, b) in bars.iter().enumerate() {
        let tp = (b.high + b.low + b.close) / 3.0;
        cum_pv += tp * b.volume as f64;
        cum_v += b.volume as f64;
        if cum_v > 0.0 {
            out[i] = cum_pv / cum_v;
        }
    }
    out
}

/// ATR — average true range, Wilder's smoothing. Not rendered yet but
/// included so future indicators (Keltner channels, Supertrend, etc.) can
/// reuse it.
#[allow(dead_code)]
pub fn compute_atr(bars: &[Bar], period: usize) -> Vec<f64> {
    let n = bars.len();
    let mut out = vec![f64::NAN; n];
    if period == 0 || n < period + 1 {
        return out;
    }
    let true_range = |i: usize| {
        let prev_close = bars[i - 1].close;
        let h = bars[i].high;
        let l = bars[i].low;
        (h - l).max((h - prev_close).abs()).max((l - prev_close).abs())
    };
    let mut sum = 0.0;
    for i in 1..=period {
        sum += true_range(i);
    }
    out[period] = sum / period as f64;
    for i in (period + 1)..n {
        out[i] = (out[i - 1] * (period as f64 - 1.0) + true_range(i)) / period as f64;
    }
    out
}

// ── Tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    fn bars_with_closes(closes: &[f64]) -> Vec<Bar> {
        closes
            .iter()
            .enumerate()
            .map(|(i, &c)| Bar {
                time: Utc::now() + chrono::Duration::minutes(i as i64),
                open: c,
                high: c + 1.0,
                low: c - 1.0,
                close: c,
                volume: 100,
            })
            .collect()
    }

    fn approx(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-6
    }

    #[test]
    fn sma_seeds_at_period_minus_one() {
        let bars = bars_with_closes(&[10.0, 11.0, 12.0, 13.0, 14.0]);
        let sma = compute_sma(&bars, 3);
        assert!(sma[0].is_nan());
        assert!(sma[1].is_nan());
        assert!(approx(sma[2], 11.0)); // (10+11+12)/3
        assert!(approx(sma[3], 12.0));
        assert!(approx(sma[4], 13.0));
    }

    #[test]
    fn bollinger_middle_equals_sma() {
        let bars = bars_with_closes(&[10.0, 11.0, 12.0, 13.0, 14.0]);
        let (upper, middle, lower) = compute_bollinger(&bars, 3, 2.0);
        let sma = compute_sma(&bars, 3);
        for i in 0..bars.len() {
            if !middle[i].is_nan() {
                assert!(approx(middle[i], sma[i]));
                assert!(upper[i] > middle[i]);
                assert!(lower[i] < middle[i]);
            }
        }
    }

    #[test]
    fn rsi_pegs_to_100_on_pure_uptrend() {
        // 15 strictly increasing closes — every diff is positive, avg_loss = 0.
        let closes: Vec<f64> = (1..=15).map(|i| i as f64).collect();
        let bars = bars_with_closes(&closes);
        let rsi = compute_rsi(&bars, 14);
        assert!(rsi[14] > 99.0, "rsi[14] = {} (want ~100)", rsi[14]);
    }

    #[test]
    fn rsi_around_50_on_alternating() {
        // Alternating up/down by the same amount → roughly RSI 50.
        let closes: Vec<f64> = (0..30)
            .map(|i| 100.0 + if i % 2 == 0 { 1.0 } else { -1.0 })
            .collect();
        let bars = bars_with_closes(&closes);
        let rsi = compute_rsi(&bars, 14);
        let last = rsi[29];
        assert!(
            (last - 50.0).abs() < 5.0,
            "alternating RSI[29] = {} (want ~50)",
            last
        );
    }

    #[test]
    fn macd_histogram_is_macd_minus_signal() {
        let closes: Vec<f64> = (0..60).map(|i| 100.0 + (i as f64) * 0.3).collect();
        let bars = bars_with_closes(&closes);
        let (macd, signal, hist) = compute_macd(&bars, 12, 26, 9);
        for i in 0..bars.len() {
            if !hist[i].is_nan() {
                assert!(approx(hist[i], macd[i] - signal[i]));
            }
        }
    }

    #[test]
    fn vwap_within_high_low_bounds() {
        let bars = bars_with_closes(&[10.0, 11.0, 12.0, 13.0]);
        let vwap = compute_vwap(&bars);
        for (i, &v) in vwap.iter().enumerate() {
            assert!(!v.is_nan());
            // Cumulative VWAP must lie between the running min low and max high.
            let min_low = bars[..=i].iter().map(|b| b.low).fold(f64::INFINITY, f64::min);
            let max_high = bars[..=i]
                .iter()
                .map(|b| b.high)
                .fold(f64::NEG_INFINITY, f64::max);
            assert!(v >= min_low - 1e-9 && v <= max_high + 1e-9);
        }
    }

    #[test]
    fn atr_is_positive_when_there_is_range() {
        let bars = bars_with_closes(&[10.0, 11.0, 12.0, 13.0, 14.0, 15.0, 16.0, 17.0, 18.0, 19.0, 20.0, 21.0, 22.0, 23.0, 24.0, 25.0]);
        let atr = compute_atr(&bars, 14);
        assert!(atr[14] > 0.0);
    }
}
