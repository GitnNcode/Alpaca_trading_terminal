// TradingView-style multi-pane chart for the egui port.
//
// Layout (top to bottom):
//   1. Price panel — candlesticks + EMA / SMA / Bollinger / VWAP overlays
//   2. Volume panel    (optional)
//   3. RSI panel       (optional, 0..100 with 30/70 reference lines)
//   4. MACD panel      (optional, histogram + MACD line + signal line)
//
// All panels share an X-axis via Plot::link_axis(group) and a shared cursor
// via Plot::link_cursor(group), so panning/zooming/hovering one moves the
// others together — exactly how TradingView's stacked panes behave.

use egui::{Color32, Stroke, Vec2b};
use egui_plot::{
    AxisHints, Bar as BarMark, BarChart, BoxElem, BoxPlot, BoxSpread, GridMark, HLine, Line,
    MarkerShape, Plot, PlotPoints, Points, Polygon,
};

use crate::api::Bar;
use crate::app::{ActiveIndicator, ChartApp};
use crate::indicators;
use crate::strategies::{self, Signal, SignalKind};
use crate::theme;

// ── Range / TF presets ───────────────────────────────────────────────────

pub struct Range {
    pub label: &'static str,
    pub default_tf: usize,
    pub lookback_hours: i64,
    pub ytd: bool,
}

pub const RANGES: &[Range] = &[
    Range { label: "1D",  default_tf: 1, lookback_hours: 24,            ytd: false },
    Range { label: "1W",  default_tf: 3, lookback_hours: 24 * 7,        ytd: false },
    Range { label: "1M",  default_tf: 5, lookback_hours: 24 * 31,       ytd: false },
    Range { label: "YTD", default_tf: 5, lookback_hours: 0,             ytd: true  },
    Range { label: "1Y",  default_tf: 5, lookback_hours: 24 * 365,      ytd: false },
    Range { label: "5Y",  default_tf: 6, lookback_hours: 24 * 365 * 5,  ytd: false },
    Range { label: "MAX", default_tf: 7, lookback_hours: 24 * 365 * 30, ytd: false },
];

pub struct Timeframe {
    pub label: &'static str,
    pub value: &'static str,
}

pub const TFS: &[Timeframe] = &[
    Timeframe { label: "1m",  value: "1Min"   },
    Timeframe { label: "5m",  value: "5Min"   },
    Timeframe { label: "15m", value: "15Min"  },
    Timeframe { label: "30m", value: "30Min"  },
    Timeframe { label: "1h",  value: "1Hour"  },
    Timeframe { label: "1D",  value: "1Day"   },
    Timeframe { label: "1W",  value: "1Week"  },
    Timeframe { label: "1M",  value: "1Month" },
];

// ── Public entry point ──────────────────────────────────────────────────

pub fn render(app: &ChartApp, ui: &mut egui::Ui) {
    if app.loading {
        ui.centered_and_justified(|ui| {
            ui.label(egui::RichText::new("Loading…").color(theme::YELLOW).size(18.0));
        });
        return;
    }
    if !app.err.is_empty() {
        ui.centered_and_justified(|ui| {
            ui.label(
                egui::RichText::new(format!("Error: {}", app.err))
                    .color(theme::RED)
                    .size(16.0),
            );
        });
        return;
    }
    if app.bars.is_empty() {
        ui.centered_and_justified(|ui| {
            ui.label(
                egui::RichText::new("Enter a symbol above and click Load")
                    .color(theme::GRAY2)
                    .size(16.0),
            );
        });
        return;
    }

    // Total height available for chart panes. Subtract a little padding.
    let total_h = ui.available_height();
    let total_w = ui.available_width();
    let ind = &app.indicators;

    // How many sub-panels are visible?
    let sub_count =
        (ind.volume as i32) + (ind.rsi as i32) + (ind.macd as i32);
    // Volume gets ~12% of available height; RSI/MACD ~18% each. Price takes
    // the rest. These ratios feel right at typical window heights — egui's
    // built-in zoom lets the user adjust as needed.
    let vol_h: f32 = if ind.volume { total_h * 0.12 } else { 0.0 };
    let rsi_h: f32 = if ind.rsi { total_h * 0.18 } else { 0.0 };
    let macd_h: f32 = if ind.macd { total_h * 0.18 } else { 0.0 };
    let separator_h = if sub_count > 0 { 4.0 * sub_count as f32 } else { 0.0 };
    let price_h = (total_h - vol_h - rsi_h - macd_h - separator_h - 8.0).max(120.0);
    let _ = total_w;

    // Shared groups so all plots pan/zoom/hover together.
    let axis_group = ui.id().with("alpaca_axis_link");
    let cursor_group = ui.id().with("alpaca_cursor_link");

    // X/Y zoom toggles from the toolbar. egui_plot::Plot::allow_zoom takes a
    // Vec2b — pass per-axis booleans so the user can lock one axis while
    // free-scaling the other. Same Vec2b is reused for every panel.
    let zoom = Vec2b::new(app.zoom_x, app.zoom_y);

    // If strategy mode is on AND exactly one indicator is selected, compute
    // its Buy/Sell signal list now so we can render markers on the price
    // panel. Each strategy's rule lives in `strategies.rs`.
    let signals: Vec<Signal> = if app.strategy_enabled {
        match app.indicators.only_active_with_strategy() {
            Some(ActiveIndicator::Ema) => strategies::ma_cross_signals(
                app.bars.as_slice(),
                &indicators::compute_ema(app.bars.as_slice(), app.indicators.ema_period),
            ),
            Some(ActiveIndicator::Sma) => strategies::ma_cross_signals(
                app.bars.as_slice(),
                &indicators::compute_sma(app.bars.as_slice(), app.indicators.sma_period),
            ),
            Some(ActiveIndicator::Vwap) => strategies::ma_cross_signals(
                app.bars.as_slice(),
                &indicators::compute_vwap(app.bars.as_slice()),
            ),
            Some(ActiveIndicator::Bollinger) => {
                let (u, _m, l) = indicators::compute_bollinger(
                    app.bars.as_slice(),
                    app.indicators.bollinger_period,
                    app.indicators.bollinger_mult,
                );
                strategies::bollinger_signals(app.bars.as_slice(), &u, &l)
            }
            Some(ActiveIndicator::Rsi) => strategies::rsi_signals(
                app.bars.as_slice(),
                &indicators::compute_rsi(app.bars.as_slice(), app.indicators.rsi_period),
            ),
            Some(ActiveIndicator::Macd) => {
                let (m, s, _) = indicators::compute_macd(
                    app.bars.as_slice(),
                    app.indicators.macd_fast,
                    app.indicators.macd_slow,
                    app.indicators.macd_signal,
                );
                strategies::macd_cross_signals(app.bars.as_slice(), &m, &s)
            }
            None => Vec::new(),
        }
    } else {
        Vec::new()
    };

    // ── Price panel ──────────────────────────────────────────────────────
    let bars = app.bars.as_slice();
    let stats = compute_stats(bars);
    render_header_strip(app, ui, &stats);

    let candles = build_candles(bars);
    let ema = if ind.ema {
        Some(indicators::compute_ema(bars, ind.ema_period))
    } else {
        None
    };
    let sma = if ind.sma {
        Some(indicators::compute_sma(bars, ind.sma_period))
    } else {
        None
    };
    let (bb_upper, bb_mid, bb_lower) = if ind.bollinger {
        let t = indicators::compute_bollinger(bars, ind.bollinger_period, ind.bollinger_mult);
        (Some(t.0), Some(t.1), Some(t.2))
    } else {
        (None, None, None)
    };
    let vwap = if ind.vwap {
        Some(indicators::compute_vwap(bars))
    } else {
        None
    };

    Plot::new("price")
        .height(price_h)
        .legend(egui_plot::Legend::default().background_alpha(0.35))
        .link_axis(axis_group, true, false)
        .link_cursor(cursor_group, true, false)
        .allow_drag(zoom)
        .allow_scroll(zoom)
        .allow_zoom(zoom)
        .show_axes([true, true])
        .x_axis_label("")
        .show_x(false) // we'll show our own time tooltip
        .label_formatter(make_label_formatter(bars))
        .show(ui, |plot_ui| {
            plot_ui.box_plot(
                BoxPlot::new(candles)
                    .name("Price")
                    .element_formatter(Box::new(|elem, _| {
                        format!(
                            "O:{:.2}  H:{:.2}  L:{:.2}  C:{:.2}",
                            elem.spread.quartile1,
                            elem.spread.upper_whisker,
                            elem.spread.lower_whisker,
                            elem.spread.quartile3,
                        )
                    })),
            );
            if let Some(values) = &ema {
                plot_ui.line(line_for("EMA", values, theme::CYAN, 2.0));
            }
            if let Some(values) = &sma {
                plot_ui.line(line_for("SMA", values, theme::YELLOW, 2.0));
            }
            if let (Some(u), Some(m), Some(l)) = (&bb_upper, &bb_mid, &bb_lower) {
                plot_ui.line(line_for("BB upper", u, theme::GRAY2, 1.0));
                plot_ui.line(line_for("BB middle", m, theme::GRAY2, 1.0));
                plot_ui.line(line_for("BB lower", l, theme::GRAY2, 1.0));
            }
            if let Some(values) = &vwap {
                plot_ui.line(line_for("VWAP", values, theme::YELLOW, 2.0));
            }
            // Strategy Buy/Sell markers — drawn just below each Buy bar's low
            // and just above each Sell bar's high. Sized in plot coordinates
            // so they stay readable through zoom.
            if !signals.is_empty() {
                let span = (stats.high - stats.low).max(1.0);
                let offset = span * 0.015;
                let (mut buys, mut sells) = (Vec::new(), Vec::new());
                for s in &signals {
                    let b = &bars[s.bar_idx];
                    match s.kind {
                        SignalKind::Buy => buys.push([s.bar_idx as f64, b.low - offset]),
                        SignalKind::Sell => sells.push([s.bar_idx as f64, b.high + offset]),
                    }
                }
                if !buys.is_empty() {
                    plot_ui.points(
                        Points::new(PlotPoints::new(buys))
                            .shape(MarkerShape::Up)
                            .color(theme::GREEN)
                            .radius(8.0)
                            .filled(true)
                            .name("Buy"),
                    );
                }
                if !sells.is_empty() {
                    plot_ui.points(
                        Points::new(PlotPoints::new(sells))
                            .shape(MarkerShape::Down)
                            .color(theme::RED)
                            .radius(8.0)
                            .filled(true)
                            .name("Sell"),
                    );
                }
            }
            // TradingView-style horizontal line at the latest close.
            if let Some(last) = bars.last() {
                let lp_color = if last.close < last.open {
                    theme::RED
                } else if last.close > last.open {
                    theme::GREEN
                } else {
                    theme::YELLOW
                };
                plot_ui.hline(
                    HLine::new(last.close)
                        .color(lp_color)
                        .stroke(Stroke::new(1.0, lp_color))
                        .name(format!("Close {:.2}", last.close)),
                );
            }
        });

    // ── Volume panel ─────────────────────────────────────────────────────
    if ind.volume {
        ui.add_space(2.0);
        let vol_bars = build_volume_bars(bars);
        Plot::new("volume")
            .height(vol_h)
            .legend(egui_plot::Legend::default().background_alpha(0.35))
            .link_axis(axis_group, true, false)
            .link_cursor(cursor_group, true, false)
            .allow_drag(zoom)
            .allow_scroll(zoom)
            .allow_zoom(zoom)
            .show_axes([false, true])
            .x_axis_label("")
            .show_x(false)
            .show_y(true)
            .show(ui, |plot_ui| {
                plot_ui.bar_chart(BarChart::new(vol_bars).name("Volume"));
            });
    }

    // ── RSI panel ────────────────────────────────────────────────────────
    if ind.rsi {
        ui.add_space(2.0);
        let rsi = indicators::compute_rsi(bars, ind.rsi_period);
        Plot::new("rsi")
            .height(rsi_h)
            .legend(egui_plot::Legend::default().background_alpha(0.35))
            .link_axis(axis_group, true, false)
            .link_cursor(cursor_group, true, false)
            .allow_drag(zoom)
            .allow_scroll(zoom)
            .allow_zoom(zoom)
            .show_axes([false, true])
            .x_axis_label("")
            .show_x(false)
            .include_y(0.0)
            .include_y(100.0)
            .y_axis_formatter(rsi_axis_formatter())
            .show(ui, |plot_ui| {
                // Neutral momentum zone (45..55), shaded behind everything
                // else as a TradingView-style reference band. Drawn first so
                // the RSI line + reference lines paint on top.
                plot_ui.polygon(
                    Polygon::new(PlotPoints::new(vec![
                        [-1.0e9, 45.0],
                        [1.0e9, 45.0],
                        [1.0e9, 55.0],
                        [-1.0e9, 55.0],
                    ]))
                    .fill_color(Color32::from_rgba_unmultiplied(180, 180, 180, 18))
                    .stroke(Stroke::NONE)
                    .name("Neutral zone (45–55)"),
                );
                // Overbought / oversold reference lines.
                plot_ui.hline(
                    HLine::new(70.0)
                        .color(theme::GRAY2)
                        .stroke(Stroke::new(0.8, theme::GRAY2))
                        .name("Overbought (70)"),
                );
                plot_ui.hline(
                    HLine::new(30.0)
                        .color(theme::GRAY2)
                        .stroke(Stroke::new(0.8, theme::GRAY2))
                        .name("Oversold (30)"),
                );
                // Zone boundaries — thinner so they read as a zone, not full lines.
                let zone_stroke =
                    Stroke::new(0.4, Color32::from_rgba_unmultiplied(180, 180, 180, 90));
                plot_ui.hline(HLine::new(55.0).color(zone_stroke.color).stroke(zone_stroke));
                plot_ui.hline(HLine::new(45.0).color(zone_stroke.color).stroke(zone_stroke));
                // Midline at 50 — slightly more prominent, dashed feel via a
                // dimmed but bold-ish stroke.
                plot_ui.hline(
                    HLine::new(50.0)
                        .color(Color32::from_rgb(140, 140, 140))
                        .stroke(Stroke::new(1.0, Color32::from_rgb(140, 140, 140)))
                        .name("Midline (50)"),
                );

                plot_ui.line(line_for(
                    &format!("RSI({})", ind.rsi_period),
                    &rsi,
                    theme::CYAN,
                    2.0,
                ));
            });
    }

    // ── MACD panel ───────────────────────────────────────────────────────
    if ind.macd {
        ui.add_space(2.0);
        let (macd_line, signal_line, hist) =
            indicators::compute_macd(bars, ind.macd_fast, ind.macd_slow, ind.macd_signal);

        let hist_bars: Vec<BarMark> = hist
            .iter()
            .enumerate()
            .filter_map(|(i, &v)| {
                if v.is_nan() {
                    None
                } else {
                    let color = if v >= 0.0 { theme::GREEN } else { theme::RED };
                    Some(
                        BarMark::new(i as f64, v)
                            .fill(color)
                            .stroke(Stroke::NONE)
                            .width(VOLUME_BAR_WIDTH),
                    )
                }
            })
            .collect();

        Plot::new("macd")
            .height(macd_h)
            .legend(egui_plot::Legend::default().background_alpha(0.35))
            .link_axis(axis_group, true, false)
            .link_cursor(cursor_group, true, false)
            .allow_drag(zoom)
            .allow_scroll(zoom)
            .allow_zoom(zoom)
            .show_axes([false, true])
            .x_axis_label("")
            .show_x(false)
            .show(ui, |plot_ui| {
                plot_ui.hline(
                    HLine::new(0.0)
                        .color(theme::GRAY2)
                        .stroke(Stroke::new(0.5, theme::GRAY2)),
                );
                plot_ui.bar_chart(BarChart::new(hist_bars).name("Histogram"));
                plot_ui.line(line_for(
                    &format!("MACD({},{},{})", ind.macd_fast, ind.macd_slow, ind.macd_signal),
                    &macd_line,
                    theme::CYAN,
                    2.0,
                ));
                plot_ui.line(line_for("Signal", &signal_line, theme::YELLOW, 2.0));
            });
    }
}

// ── Builders / helpers ───────────────────────────────────────────────────

/// Candle width was visibly merging at zoomed-out densities because the 1.4 px
/// stroke around each box added pixels on top of the box_width. Pure fill
/// (no stroke) at a narrower box_width keeps a real gap between adjacent
/// candles at every reasonable zoom.
const CANDLE_BOX_WIDTH: f64 = 0.55; // leaves 0.45 of gap between unit-spaced bars
const VOLUME_BAR_WIDTH: f64 = 0.55; // match candles for visual alignment

fn build_candles(bars: &[Bar]) -> Vec<BoxElem> {
    bars.iter()
        .enumerate()
        .map(|(i, b)| {
            let bullish = b.close >= b.open;
            let color = if bullish { theme::GREEN } else { theme::RED };
            let (lo_body, hi_body) = if bullish { (b.open, b.close) } else { (b.close, b.open) };
            let median = (b.open + b.close) / 2.0;
            BoxElem::new(
                i as f64,
                BoxSpread::new(b.low, lo_body, median, hi_body, b.high),
            )
            .fill(color)
            .stroke(Stroke::NONE)
            .whisker_width(0.0)
            .box_width(CANDLE_BOX_WIDTH)
        })
        .collect()
}

fn build_volume_bars(bars: &[Bar]) -> Vec<BarMark> {
    bars.iter()
        .enumerate()
        .map(|(i, b)| {
            let color = if b.close >= b.open { theme::GREEN } else { theme::RED };
            BarMark::new(i as f64, b.volume as f64)
                .fill(color)
                .stroke(Stroke::NONE)
                .width(VOLUME_BAR_WIDTH)
        })
        .collect()
}

fn line_for(name: &str, values: &[f64], color: Color32, width: f32) -> Line {
    let pts: PlotPoints = values
        .iter()
        .enumerate()
        .filter_map(|(i, &v)| if v.is_nan() { None } else { Some([i as f64, v]) })
        .collect();
    Line::new(pts)
        .color(color)
        .stroke(Stroke::new(width, color))
        .name(name)
}

/// Formats the floating crosshair label as "TIME · O:.. H:.. L:.. C:.."
/// when the cursor is over a bar in the price plot.
fn make_label_formatter(
    bars: &[Bar],
) -> Box<dyn Fn(&str, &egui_plot::PlotPoint) -> String + Send + Sync + 'static> {
    let owned: Vec<Bar> = bars.to_vec();
    Box::new(move |_name, point| {
        let idx = point.x.round() as i64;
        if idx < 0 || (idx as usize) >= owned.len() {
            return format!("{:.2}", point.y);
        }
        let b = &owned[idx as usize];
        let t = b.time.with_timezone(&chrono::Local).format("%Y-%m-%d %H:%M");
        format!(
            "{}\nO:{:.2}  H:{:.2}  L:{:.2}  C:{:.2}\nVol {}",
            t,
            b.open,
            b.high,
            b.low,
            b.close,
            short_volume(b.volume),
        )
    })
}

fn rsi_axis_formatter() -> impl Fn(GridMark, &std::ops::RangeInclusive<f64>) -> String {
    move |mark, _| {
        let v = mark.value;
        if (v - 30.0).abs() < 0.01 || (v - 50.0).abs() < 0.01 || (v - 70.0).abs() < 0.01 {
            format!("{:.0}", v)
        } else if v == 0.0 || v == 100.0 {
            format!("{:.0}", v)
        } else {
            String::new()
        }
    }
}

// ── Stats strip ──────────────────────────────────────────────────────────

struct Stats {
    last_close: f64,
    chg: f64,
    pct: f64,
    high: f64,
    low: f64,
    vol: i64,
    bars: usize,
}

fn compute_stats(bars: &[Bar]) -> Stats {
    let first = &bars[0];
    let last = &bars[bars.len() - 1];
    let mut hi = first.high;
    let mut lo = first.low;
    let mut vol: i64 = 0;
    for b in bars {
        if b.high > hi {
            hi = b.high;
        }
        if b.low < lo {
            lo = b.low;
        }
        vol += b.volume;
    }
    let chg = last.close - first.open;
    let pct = if first.open > 0.0 {
        chg / first.open * 100.0
    } else {
        0.0
    };
    Stats {
        last_close: last.close,
        chg,
        pct,
        high: hi,
        low: lo,
        vol,
        bars: bars.len(),
    }
}

fn render_header_strip(app: &ChartApp, ui: &mut egui::Ui, s: &Stats) {
    ui.horizontal_wrapped(|ui| {
        ui.label(
            egui::RichText::new(format!(" {} ", app.current_symbol))
                .color(theme::ORANGE)
                .strong()
                .size(16.0),
        );
        let chg_color = if s.chg < 0.0 { theme::RED } else { theme::GREEN };
        let sign = if s.chg < 0.0 { "" } else { "+" };
        ui.label(egui::RichText::new(format!("${:.2}", s.last_close)).color(theme::WHITE).size(16.0));
        ui.label(
            egui::RichText::new(format!("{}${:.2} ({}{:.2}%)", sign, s.chg, sign, s.pct))
                .color(chg_color)
                .size(14.0),
        );
        ui.add_space(8.0);
        kv(ui, "H", &format!("{:.2}", s.high));
        kv(ui, "L", &format!("{:.2}", s.low));
        kv(ui, "Vol", &short_volume(s.vol));
        kv(ui, "Bars", &format!("{}", s.bars));
        kv(
            ui,
            "TF",
            TFS[app.tf_idx].label,
        );
        kv(ui, "Range", RANGES[app.range_idx].label);
    });
    ui.add_space(2.0);
    // Active indicator chips
    ui.horizontal_wrapped(|ui| {
        if app.indicators.ema {
            chip(ui, &format!("EMA({})", app.indicators.ema_period), theme::CYAN);
        }
        if app.indicators.sma {
            chip(ui, &format!("SMA({})", app.indicators.sma_period), theme::YELLOW);
        }
        if app.indicators.bollinger {
            chip(
                ui,
                &format!("BB({}, {:.1})", app.indicators.bollinger_period, app.indicators.bollinger_mult),
                theme::GRAY2,
            );
        }
        if app.indicators.vwap {
            chip(ui, "VWAP", theme::YELLOW);
        }
    });
}

fn kv(ui: &mut egui::Ui, k: &str, v: &str) {
    ui.label(egui::RichText::new(k).color(theme::ORANGE).strong());
    ui.label(egui::RichText::new(v).color(theme::WHITE));
    ui.add_space(4.0);
}

fn chip(ui: &mut egui::Ui, label: &str, color: Color32) {
    ui.add(
        egui::Button::new(egui::RichText::new(label).color(theme::BLACK).strong())
            .fill(color)
            .stroke(Stroke::new(0.0, color)),
    );
}

fn short_volume(v: i64) -> String {
    if v >= 1_000_000_000 {
        format!("{:.2}B", v as f64 / 1e9)
    } else if v >= 1_000_000 {
        format!("{:.2}M", v as f64 / 1e6)
    } else if v >= 1_000 {
        format!("{:.2}K", v as f64 / 1e3)
    } else {
        v.to_string()
    }
}

// `AxisHints` is imported but unused in this file currently; keep the import
// behind an `#[allow]` to silence dead-import warnings while leaving the door
// open for future axis customization.
#[allow(dead_code)]
fn _unused_axis_hint() -> AxisHints<'static> {
    AxisHints::new_x()
}
