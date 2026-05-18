// Single-indicator trading-strategy signal generators.
//
// Each function takes the bar series + the indicator's precomputed values
// and returns a list of (bar_idx, price, Buy|Sell) events. The chart renders
// them as up/down triangles on the price panel — same convention TradingView
// uses for its built-in strategies.
//
// Rules implemented match the *classic / most widely cited* version of each
// strategy, not optimized variants:
//
//   * EMA / SMA — price crosses above / below the moving average
//   * Bollinger — close touches the lower (buy) or upper (sell) band
//   * VWAP      — same as MA cross, treating VWAP as the line
//   * RSI       — cross up through 30 (buy) or down through 70 (sell)
//   * MACD      — MACD line crosses above (buy) or below (sell) its signal
//
// Volume-only mode has no canonical rule and is intentionally omitted —
// callers should hide the strategy toggle when only Volume is selected.

use crate::api::Bar;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SignalKind {
    Buy,
    Sell,
}

#[derive(Clone, Debug)]
pub struct Signal {
    pub bar_idx: usize,
    pub price: f64,
    pub kind: SignalKind,
}

fn push_cross(out: &mut Vec<Signal>, bars: &[Bar], i: usize, kind: SignalKind) {
    out.push(Signal {
        bar_idx: i,
        price: bars[i].close,
        kind,
    });
}

/// Price-vs-line crossover. Used for EMA, SMA, and VWAP strategies.
pub fn ma_cross_signals(bars: &[Bar], ma: &[f64]) -> Vec<Signal> {
    let mut out = Vec::new();
    let mut prev_above: Option<bool> = None;
    for i in 0..bars.len().min(ma.len()) {
        if ma[i].is_nan() {
            continue;
        }
        let above = bars[i].close > ma[i];
        if let Some(was) = prev_above {
            if !was && above {
                push_cross(&mut out, bars, i, SignalKind::Buy);
            } else if was && !above {
                push_cross(&mut out, bars, i, SignalKind::Sell);
            }
        }
        prev_above = Some(above);
    }
    out
}

/// Bollinger Bands mean-reversion. Generates at most one Buy per touch of
/// the lower band and one Sell per touch of the upper band, alternating —
/// so a continued downtrend doesn't spam Buys on every bar.
pub fn bollinger_signals(bars: &[Bar], upper: &[f64], lower: &[f64]) -> Vec<Signal> {
    let mut out = Vec::new();
    let mut last: Option<SignalKind> = None;
    let n = bars.len().min(upper.len()).min(lower.len());
    for i in 0..n {
        if upper[i].is_nan() || lower[i].is_nan() {
            continue;
        }
        let c = bars[i].close;
        if c <= lower[i] && last != Some(SignalKind::Buy) {
            push_cross(&mut out, bars, i, SignalKind::Buy);
            last = Some(SignalKind::Buy);
        } else if c >= upper[i] && last != Some(SignalKind::Sell) {
            push_cross(&mut out, bars, i, SignalKind::Sell);
            last = Some(SignalKind::Sell);
        }
    }
    out
}

/// Classic RSI overbought/oversold: buy on the bar that takes RSI back above
/// 30 (recovering from oversold), sell on the bar that takes it back below
/// 70 (cooling from overbought).
pub fn rsi_signals(bars: &[Bar], rsi: &[f64]) -> Vec<Signal> {
    let mut out = Vec::new();
    let n = bars.len().min(rsi.len());
    for i in 1..n {
        if rsi[i].is_nan() || rsi[i - 1].is_nan() {
            continue;
        }
        if rsi[i - 1] < 30.0 && rsi[i] >= 30.0 {
            push_cross(&mut out, bars, i, SignalKind::Buy);
        } else if rsi[i - 1] > 70.0 && rsi[i] <= 70.0 {
            push_cross(&mut out, bars, i, SignalKind::Sell);
        }
    }
    out
}

/// MACD signal-line crossover.
pub fn macd_cross_signals(bars: &[Bar], macd: &[f64], signal: &[f64]) -> Vec<Signal> {
    let mut out = Vec::new();
    let mut prev_above: Option<bool> = None;
    let n = bars.len().min(macd.len()).min(signal.len());
    for i in 0..n {
        if macd[i].is_nan() || signal[i].is_nan() {
            continue;
        }
        let above = macd[i] > signal[i];
        if let Some(was) = prev_above {
            if !was && above {
                push_cross(&mut out, bars, i, SignalKind::Buy);
            } else if was && !above {
                push_cross(&mut out, bars, i, SignalKind::Sell);
            }
        }
        prev_above = Some(above);
    }
    out
}

// ── Tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    fn bars(closes: &[f64]) -> Vec<Bar> {
        closes
            .iter()
            .enumerate()
            .map(|(i, &c)| Bar {
                time: Utc::now() + chrono::Duration::minutes(i as i64),
                open: c,
                high: c + 0.5,
                low: c - 0.5,
                close: c,
                volume: 100,
            })
            .collect()
    }

    #[test]
    fn ma_cross_buys_when_price_goes_above_then_sells_when_below() {
        let b = bars(&[10.0, 11.0, 12.0, 13.0, 12.0, 11.0]);
        let ma = vec![11.0, 11.0, 11.0, 11.0, 11.0, 11.0];
        let sigs = ma_cross_signals(&b, &ma);
        // Bar 0: close=10 < 11 (below). Bar 1: 11 not strictly > 11 → still below.
        // Bar 2: 12 > 11 → Buy. Bar 4: 12 > 11 (still above). Bar 5: 11 not strictly > 11 → Sell.
        let kinds: Vec<_> = sigs.iter().map(|s| s.kind).collect();
        assert_eq!(kinds, vec![SignalKind::Buy, SignalKind::Sell]);
        assert_eq!(sigs[0].bar_idx, 2);
        assert_eq!(sigs[1].bar_idx, 5);
    }

    #[test]
    fn bollinger_alternates_buy_and_sell() {
        // Synthetic: 6 bars, lower=98 upper=102. Sequence touches lower, upper, lower again.
        let closes = [97.0, 99.0, 103.0, 100.0, 97.5, 100.0];
        let b = bars(&closes);
        let upper = vec![102.0; 6];
        let lower = vec![98.0; 6];
        let sigs = bollinger_signals(&b, &upper, &lower);
        let kinds: Vec<_> = sigs.iter().map(|s| s.kind).collect();
        assert_eq!(
            kinds,
            vec![SignalKind::Buy, SignalKind::Sell, SignalKind::Buy]
        );
        assert_eq!(sigs[0].bar_idx, 0);
        assert_eq!(sigs[1].bar_idx, 2);
        assert_eq!(sigs[2].bar_idx, 4);
    }

    #[test]
    fn rsi_signals_fire_on_threshold_crossings() {
        let b = bars(&[1.0; 8]);
        // RSI dips below 30 then climbs back; pokes above 70 then falls.
        let rsi = vec![50.0, 28.0, 25.0, 35.0, 60.0, 75.0, 72.0, 65.0];
        let sigs = rsi_signals(&b, &rsi);
        let kinds: Vec<_> = sigs.iter().map(|s| s.kind).collect();
        // bar 3: rsi[2]=25 < 30, rsi[3]=35 ≥ 30 → Buy
        // bar 7: rsi[6]=72 > 70, rsi[7]=65 ≤ 70 → Sell
        assert_eq!(kinds, vec![SignalKind::Buy, SignalKind::Sell]);
        assert_eq!(sigs[0].bar_idx, 3);
        assert_eq!(sigs[1].bar_idx, 7);
    }

    #[test]
    fn macd_cross_signals_fire_at_line_crossings() {
        let b = bars(&[1.0; 6]);
        let macd = vec![-1.0, -0.5, 0.5, 1.0, 0.5, -0.5];
        let signal = vec![0.0; 6];
        let sigs = macd_cross_signals(&b, &macd, &signal);
        // bar 2: macd crosses above signal → Buy
        // bar 5: macd crosses below signal → Sell
        let kinds: Vec<_> = sigs.iter().map(|s| s.kind).collect();
        assert_eq!(kinds, vec![SignalKind::Buy, SignalKind::Sell]);
        assert_eq!(sigs[0].bar_idx, 2);
        assert_eq!(sigs[1].bar_idx, 5);
    }
}
