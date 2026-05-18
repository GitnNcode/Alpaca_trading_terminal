// TradingView-style chart widget. Stacks panels vertically:
//
//   ┌─ price + overlays (EMA/SMA/Bollinger/VWAP) ──────┐
//   ├─ volume (histogram, optional) ──────────────────┤
//   ├─ RSI (optional) ────────────────────────────────┤
//   ├─ MACD (optional) ───────────────────────────────┤
//   └─ scrollbar + date labels ──────────────────────┘
//
// Each panel renders independently with its own Y-scale. The X axis is
// shared (bar index → terminal column). Indicators are computed from the
// FULL `bars` history so the leftmost visible value has correctly-
// accumulated weight (not a cold start).

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::widgets::{Block, BorderType, Borders, Widget};

use crate::api::Bar;
use crate::app::IndicatorState;
use crate::indicators;
use crate::theme::*;

pub struct ChartCanvas<'a> {
    pub bars: &'a [Bar],
    pub date_fmt: &'a str,
    pub title: String,
    pub loading: bool,
    pub err: &'a str,
    pub focused: bool,
    pub scroll_offset: usize,
    pub indicators: &'a IndicatorState,
    pub hover: Option<(u16, u16)>,
}

/// Returns (slot_w, body_w, start_idx, end_idx, step) given the bars count,
/// requested scroll offset, and the available canvas width. Shared with
/// input handlers so they know how big a single ←/→ scroll step is.
pub fn compute_window(
    n: usize,
    scroll_offset: usize,
    chart_w: usize,
) -> (usize, usize, usize, usize, usize) {
    if n == 0 || chart_w < 4 {
        return (2, 1, 0, 0, 1);
    }
    let (slot_w, body_w) = if n * 4 <= chart_w { (4usize, 3usize) } else { (2usize, 1usize) };
    let visible_count = (chart_w / slot_w).min(n);
    let max_offset = n - visible_count;
    let scroll = scroll_offset.min(max_offset);
    let end_idx = n - scroll;
    let start_idx = end_idx - visible_count;
    let step = (visible_count / 8).max(1);
    (slot_w, body_w, start_idx, end_idx, step)
}

impl<'a> Widget for ChartCanvas<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let border_color = if self.focused { ORANGE } else { GRAY };
        let block = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Plain)
            .border_style(Style::default().fg(border_color))
            .style(Style::default().bg(BLACK))
            .title(ratatui::text::Span::styled(
                format!(" {} ", self.title),
                Style::default().fg(ORANGE).add_modifier(Modifier::BOLD),
            ));
        let inner = block.inner(area);
        block.render(area, buf);

        let x = inner.x;
        let y = inner.y;
        let w = inner.width as usize;
        let h = inner.height as usize;
        if w < 6 || h < 4 {
            return;
        }

        if self.loading {
            write_str(buf, x + 2, y + 1, "  LOADING...", YELLOW);
            return;
        }
        if !self.err.is_empty() {
            let msg = format!("  ERROR: {}", self.err.to_uppercase());
            write_str(buf, x + 2, y + 1, &truncate(&msg, w.saturating_sub(2)), RED);
            return;
        }
        if self.bars.is_empty() {
            let msg =
                "  ENTER A SYMBOL ABOVE AND PRESS ENTER  ·  [D]AY [W]EEK [M]ONTH Y[T]D [Y]EAR [F]IVE-YR MA[X]";
            write_str(buf, x + 2, y + 1, &truncate(msg, w.saturating_sub(2)), GRAY2);
            return;
        }

        let right_axis_w: usize = 10;
        let bottom_axis_h: usize = 2;
        let chart_w = w.saturating_sub(right_axis_w + 1);
        if chart_w < 10 {
            return;
        }
        let chart_x = x + 1;

        // Compute window once — shared across all panels.
        let n = self.bars.len();
        let (slot_w, body_w, start_idx, end_idx, _step) = compute_window(n, self.scroll_offset, chart_w);
        if end_idx <= start_idx {
            return;
        }
        let visible = &self.bars[start_idx..end_idx];

        // ── Panel layout ──────────────────────────────────────────────────
        // Total vertical budget: h - bottom_axis (2 rows).
        // Each enabled sub-panel takes a fixed share; price gets the rest.
        let total_h = h.saturating_sub(bottom_axis_h + 1) as i32;
        if total_h < 6 {
            return;
        }
        let mut sub_heights: Vec<(SubPanel, u16)> = Vec::new();
        let ind = self.indicators;
        if ind.volume {
            sub_heights.push((SubPanel::Volume, sub_panel_height(total_h, 0.12, 4)));
        }
        if ind.rsi {
            sub_heights.push((SubPanel::Rsi, sub_panel_height(total_h, 0.18, 6)));
        }
        if ind.macd {
            sub_heights.push((SubPanel::Macd, sub_panel_height(total_h, 0.18, 6)));
        }
        let sub_total: i32 = sub_heights.iter().map(|&(_, h)| h as i32 + 1).sum::<i32>(); // +1 per separator row
        let price_h = (total_h - sub_total).max(5) as u16;

        // ── Price panel ───────────────────────────────────────────────────
        let price_top = y;
        let price_panel = PanelLayout {
            top: price_top,
            height: price_h,
            chart_x: chart_x,
            chart_w: chart_w as u16,
            axis_x: chart_x + chart_w as u16,
            right_axis_w: right_axis_w as u16,
        };

        // y-range over visible window
        let (mut min_p, mut max_p) = (f64::INFINITY, f64::NEG_INFINITY);
        for b in visible {
            if b.low < min_p {
                min_p = b.low;
            }
            if b.high > max_p {
                max_p = b.high;
            }
        }
        // Bollinger / VWAP can extend beyond candle high/low — include them.
        let (bb_upper, _bb_mid, bb_lower) = if ind.bollinger {
            indicators::compute_bollinger(self.bars, ind.bollinger_period, ind.bollinger_mult)
        } else {
            (vec![], vec![], vec![])
        };
        if ind.bollinger {
            for i in start_idx..end_idx {
                if let Some(&v) = bb_upper.get(i) {
                    if !v.is_nan() && v > max_p {
                        max_p = v;
                    }
                }
                if let Some(&v) = bb_lower.get(i) {
                    if !v.is_nan() && v < min_p {
                        min_p = v;
                    }
                }
            }
        }
        if max_p <= min_p {
            max_p = min_p + 1.0;
        }
        let pad = (max_p - min_p) * 0.05;
        let min_p = min_p - pad;
        let max_p = max_p + pad;

        draw_grid(buf, &price_panel, GRAY);
        // Candles
        for (i, b) in visible.iter().enumerate() {
            let slot_x = chart_x as usize + i * slot_w;
            if slot_x + body_w > chart_x as usize + chart_w {
                break;
            }
            let wick_col = slot_x + body_w / 2;
            let (color, style) = if b.close < b.open {
                (RED, Style::default().fg(RED).bg(BLACK))
            } else {
                (GREEN, Style::default().fg(GREEN).bg(BLACK))
            };
            let _ = color;
            let hi_r = price_to_row(b.high, min_p, max_p, price_panel.height) + price_panel.top;
            let lo_r = price_to_row(b.low, min_p, max_p, price_panel.height) + price_panel.top;
            let op_r = price_to_row(b.open, min_p, max_p, price_panel.height) + price_panel.top;
            let cl_r = price_to_row(b.close, min_p, max_p, price_panel.height) + price_panel.top;
            for r in hi_r..=lo_r {
                buf[(wick_col as u16, r)].set_char('│').set_style(style);
            }
            let (b_top, b_bot) = if op_r > cl_r { (cl_r, op_r) } else { (op_r, cl_r) };
            for bcx in slot_x..slot_x + body_w {
                for r in b_top..=b_bot {
                    buf[(bcx as u16, r)].set_char('█').set_style(style);
                }
            }
        }

        // Overlay indicators on the price panel
        if ind.ema {
            draw_line_indicator(
                buf,
                &price_panel,
                &indicators::compute_ema(self.bars, ind.ema_period),
                start_idx,
                end_idx,
                slot_w,
                body_w,
                min_p,
                max_p,
                CYAN,
            );
        }
        if ind.sma {
            draw_line_indicator(
                buf,
                &price_panel,
                &indicators::compute_sma(self.bars, ind.sma_period),
                start_idx,
                end_idx,
                slot_w,
                body_w,
                min_p,
                max_p,
                YELLOW,
            );
        }
        if ind.bollinger {
            // bb_upper/lower already computed above; recompute middle for drawing
            let (u, m, l) = indicators::compute_bollinger(self.bars, ind.bollinger_period, ind.bollinger_mult);
            for (vals, col) in [(&u, GRAY2), (&m, GRAY2), (&l, GRAY2)] {
                draw_line_indicator(
                    buf, &price_panel, vals, start_idx, end_idx, slot_w, body_w, min_p, max_p, col,
                );
            }
        }
        if ind.vwap {
            draw_line_indicator(
                buf,
                &price_panel,
                &indicators::compute_vwap(self.bars),
                start_idx,
                end_idx,
                slot_w,
                body_w,
                min_p,
                max_p,
                YELLOW,
            );
        }

        // Price axis labels
        for i in 0..5usize {
            let p = max_p - (max_p - min_p) * (i as f64) / 4.0;
            let row = price_panel.top + (i as u16) * (price_panel.height - 1) / 4;
            let label = format!("{:<width$.2}", p, width = right_axis_w - 1);
            write_str(buf, price_panel.axis_x + 1, row, &label, GRAY2);
        }
        // Current-price line + label box
        let latest = &self.bars[n - 1];
        let lp_color = if latest.close < latest.open {
            RED
        } else if latest.close > latest.open {
            GREEN
        } else {
            YELLOW
        };
        if latest.close >= min_p && latest.close <= max_p {
            let pr =
                price_to_row(latest.close, min_p, max_p, price_panel.height) + price_panel.top;
            let line_style = Style::default().fg(lp_color).bg(BLACK);
            for cx in chart_x..chart_x + chart_w as u16 {
                if (cx - chart_x) % 2 == 0 {
                    buf[(cx, pr)].set_char('─').set_style(line_style);
                }
            }
            // Right-axis box label
            let box_style = Style::default().fg(BLACK).bg(lp_color).add_modifier(Modifier::BOLD);
            let label = format!(" {:.2}", latest.close);
            for i in 0..right_axis_w {
                let ch = label.chars().nth(i).unwrap_or(' ');
                buf[(price_panel.axis_x + i as u16, pr)].set_char(ch).set_style(box_style);
            }
        }

        // Indicator-value legend in the top-left corner of the price panel.
        // TradingView-style: each active series shows its name + last value.
        let mut legend_row = price_panel.top;
        if ind.ema {
            let v = indicators::compute_ema(self.bars, ind.ema_period);
            write_str(
                buf,
                chart_x + 2,
                legend_row,
                &format!("EMA({}) {:.2}", ind.ema_period, last_finite(&v)),
                CYAN,
            );
            legend_row += 1;
        }
        if ind.sma {
            let v = indicators::compute_sma(self.bars, ind.sma_period);
            write_str(
                buf,
                chart_x + 2,
                legend_row,
                &format!("SMA({}) {:.2}", ind.sma_period, last_finite(&v)),
                YELLOW,
            );
            legend_row += 1;
        }
        if ind.bollinger {
            write_str(
                buf,
                chart_x + 2,
                legend_row,
                &format!("BB({}, {:.1})", ind.bollinger_period, ind.bollinger_mult),
                GRAY2,
            );
            legend_row += 1;
        }
        if ind.vwap {
            let v = indicators::compute_vwap(self.bars);
            write_str(
                buf,
                chart_x + 2,
                legend_row,
                &format!("VWAP {:.2}", last_finite(&v)),
                YELLOW,
            );
            #[allow(unused_assignments)]
            {
                legend_row += 1;
            }
        }

        // ── Sub-panels ────────────────────────────────────────────────────
        let mut cursor_y = price_panel.top + price_panel.height + 1;
        for &(sub, sub_h) in &sub_heights {
            // Separator row
            let sep_style = Style::default().fg(GRAY).bg(BLACK);
            for cx in chart_x..chart_x + chart_w as u16 {
                buf[(cx, cursor_y - 1)].set_char('─').set_style(sep_style);
            }
            let panel = PanelLayout {
                top: cursor_y,
                height: sub_h,
                chart_x,
                chart_w: chart_w as u16,
                axis_x: chart_x + chart_w as u16,
                right_axis_w: right_axis_w as u16,
            };
            match sub {
                SubPanel::Volume => draw_volume(buf, &panel, visible, slot_w, body_w),
                SubPanel::Rsi => draw_rsi(
                    buf,
                    &panel,
                    self.bars,
                    start_idx,
                    end_idx,
                    slot_w,
                    body_w,
                    ind.rsi_period,
                ),
                SubPanel::Macd => draw_macd(
                    buf,
                    &panel,
                    self.bars,
                    start_idx,
                    end_idx,
                    slot_w,
                    body_w,
                    ind.macd_fast,
                    ind.macd_slow,
                    ind.macd_signal,
                ),
            }
            cursor_y += sub_h + 1;
        }

        // Scrollbar
        let scroll_row = y + h as u16 - 2;
        let track_style = Style::default().fg(GRAY).bg(BLACK);
        let thumb_style = Style::default().fg(ORANGE).bg(BLACK).add_modifier(Modifier::BOLD);
        for cx in chart_x..chart_x + chart_w as u16 {
            buf[(cx, scroll_row)].set_char('─').set_style(track_style);
        }
        if n > 0 {
            let thumb_start = chart_x as usize + start_idx * chart_w / n;
            let mut thumb_end = chart_x as usize + end_idx * chart_w / n;
            if thumb_end <= thumb_start {
                thumb_end = thumb_start + 1;
            }
            thumb_end = thumb_end.min(chart_x as usize + chart_w);
            for cx in thumb_start..thumb_end {
                buf[(cx as u16, scroll_row)].set_char('━').set_style(thumb_style);
            }
            let info = format!("{}-{}/{}", start_idx + 1, end_idx, n);
            write_str(buf, price_panel.axis_x + 1, scroll_row, &info, GRAY2);
        }

        // Date labels — bottom row
        let date_row = y + h as u16 - 1;
        let labels = if chart_w < 60 { 3 } else { 5 };
        let vn = visible.len();
        if vn > 0 {
            for i in 0..labels {
                let idx = if labels == 1 { 0 } else { i * (vn - 1) / (labels - 1) };
                let col = chart_x as usize + idx * slot_w + body_w / 2;
                let local = visible[idx].time.with_timezone(&chrono::Local);
                let s = local.format(self.date_fmt).to_string();
                let mut start = col as isize - s.chars().count() as isize / 2;
                if start < chart_x as isize {
                    start = chart_x as isize;
                }
                if start + s.chars().count() as isize > (chart_x as isize + chart_w as isize) {
                    start = chart_x as isize + chart_w as isize - s.chars().count() as isize;
                }
                write_str(buf, start as u16, date_row, &s, GRAY2);
            }
        }

        // ── Crosshair (hover) ─────────────────────────────────────────────
        if let Some((mx, my)) = self.hover {
            // Only if the cursor is over the chart area (not the axis / borders)
            if mx >= chart_x
                && mx < chart_x + chart_w as u16
                && my >= price_panel.top
                && my < cursor_y - 1
            {
                let xline_style = Style::default().fg(GRAY2).bg(BLACK);
                // Vertical line at mx, except where it crosses candles (keep candle visible)
                for ry in price_panel.top..(cursor_y - 1) {
                    let cell = &buf[(mx, ry)];
                    let ch = cell.symbol().chars().next().unwrap_or(' ');
                    if ch == ' ' || ch == '·' {
                        buf[(mx, ry)].set_char('┊').set_style(xline_style);
                    }
                }
                // Horizontal line + price label, only inside the price panel
                if my >= price_panel.top && my < price_panel.top + price_panel.height {
                    for cx in chart_x..chart_x + chart_w as u16 {
                        let cell = &buf[(cx, my)];
                        let ch = cell.symbol().chars().next().unwrap_or(' ');
                        if ch == ' ' || ch == '·' {
                            buf[(cx, my)].set_char('┄').set_style(xline_style);
                        }
                    }
                    let p_at_y =
                        max_p - ((my - price_panel.top) as f64) / (price_panel.height as f64 - 1.0) * (max_p - min_p);
                    let label = format!(" {:.2} ", p_at_y);
                    let tag_style = Style::default().fg(BLACK).bg(GRAY2).add_modifier(Modifier::BOLD);
                    for i in 0..right_axis_w {
                        let ch = label.chars().nth(i).unwrap_or(' ');
                        buf[(price_panel.axis_x + i as u16, my)].set_char(ch).set_style(tag_style);
                    }
                }
                // OHLC tooltip in top-right of the price panel
                let bar_col = ((mx - chart_x) as usize) / slot_w;
                if bar_col < visible.len() {
                    let b = &visible[bar_col];
                    let tip = format!(
                        " O:{:.2}  H:{:.2}  L:{:.2}  C:{:.2} ",
                        b.open, b.high, b.low, b.close
                    );
                    let tw = tip.chars().count() as u16;
                    let tx = (price_panel.axis_x).saturating_sub(tw);
                    let tip_style = Style::default().fg(WHITE).bg(POPUP_BG).add_modifier(Modifier::BOLD);
                    write_str_styled(buf, tx, price_panel.top, &tip, tip_style);
                }
            }
        }
    }
}

// ── Sub-panel renderers ─────────────────────────────────────────────────

#[derive(Copy, Clone)]
enum SubPanel {
    Volume,
    Rsi,
    Macd,
}

struct PanelLayout {
    top: u16,
    height: u16,
    chart_x: u16,
    chart_w: u16,
    axis_x: u16,
    #[allow(dead_code)] // kept for future indicator widths; currently only consulted via local
    right_axis_w: u16,
}

fn sub_panel_height(total: i32, frac: f64, min: u16) -> u16 {
    let h = (total as f64 * frac).round() as i32;
    h.max(min as i32) as u16
}

fn price_to_row(p: f64, min_p: f64, max_p: f64, h: u16) -> u16 {
    let r = ((max_p - p) / (max_p - min_p) * (h as f64 - 1.0)).round() as isize;
    r.clamp(0, h as isize - 1) as u16
}

fn draw_grid(buf: &mut Buffer, panel: &PanelLayout, color: ratatui::style::Color) {
    let st = Style::default().fg(color).bg(BLACK);
    for i in 0..5u16 {
        let gr = panel.top + i * (panel.height - 1) / 4;
        for cx in panel.chart_x..panel.chart_x + panel.chart_w {
            buf[(cx, gr)].set_char('·').set_style(st);
        }
    }
}

fn draw_volume(
    buf: &mut Buffer,
    panel: &PanelLayout,
    visible: &[Bar],
    slot_w: usize,
    body_w: usize,
) {
    let max_vol = visible.iter().map(|b| b.volume).max().unwrap_or(0).max(1) as f64;
    write_str(buf, panel.chart_x + 1, panel.top, "VOL", GRAY2);
    for (i, b) in visible.iter().enumerate() {
        let slot_x = panel.chart_x as usize + i * slot_w;
        let h = ((b.volume as f64 / max_vol) * (panel.height as f64 - 1.0)).round() as u16;
        let bottom = panel.top + panel.height - 1;
        let top = bottom.saturating_sub(h);
        let color = if b.close >= b.open { GREEN } else { RED };
        let st = Style::default().fg(color).bg(BLACK);
        for r in top..=bottom {
            for bcx in slot_x..slot_x + body_w {
                buf[(bcx as u16, r)].set_char('▌').set_style(st);
            }
        }
    }
    // Axis: just the max value at the top
    write_str(
        buf,
        panel.axis_x + 1,
        panel.top,
        &fmt_volume_short(max_vol as i64),
        GRAY2,
    );
}

fn draw_rsi(
    buf: &mut Buffer,
    panel: &PanelLayout,
    bars: &[Bar],
    start_idx: usize,
    end_idx: usize,
    slot_w: usize,
    body_w: usize,
    period: usize,
) {
    let rsi = indicators::compute_rsi(bars, period);
    // Fixed scale 0..100
    let (min_v, max_v) = (0.0, 100.0);
    // 30 / 70 reference lines
    let st_ref = Style::default().fg(GRAY2).bg(BLACK);
    for level in [30.0, 70.0] {
        let row = price_to_row(level, min_v, max_v, panel.height) + panel.top;
        for cx in panel.chart_x..panel.chart_x + panel.chart_w {
            if (cx - panel.chart_x) % 3 == 0 {
                buf[(cx, row)].set_char('·').set_style(st_ref);
            }
        }
    }
    draw_line_indicator(
        buf, panel, &rsi, start_idx, end_idx, slot_w, body_w, min_v, max_v, CYAN,
    );
    write_str(
        buf,
        panel.chart_x + 1,
        panel.top,
        &format!("RSI({}) {:.1}", period, last_finite(&rsi)),
        CYAN,
    );
    // Axis labels: 0 / 50 / 100
    write_str(buf, panel.axis_x + 1, panel.top, "100", GRAY2);
    write_str(
        buf,
        panel.axis_x + 1,
        panel.top + panel.height / 2,
        " 50",
        GRAY2,
    );
    write_str(
        buf,
        panel.axis_x + 1,
        panel.top + panel.height - 1,
        "  0",
        GRAY2,
    );
}

fn draw_macd(
    buf: &mut Buffer,
    panel: &PanelLayout,
    bars: &[Bar],
    start_idx: usize,
    end_idx: usize,
    slot_w: usize,
    body_w: usize,
    fast: usize,
    slow: usize,
    signal: usize,
) {
    let (macd, sig, hist) = indicators::compute_macd(bars, fast, slow, signal);
    // y-range across all three series in the visible window
    let (mut mn, mut mx) = (f64::INFINITY, f64::NEG_INFINITY);
    for i in start_idx..end_idx {
        for v in [macd.get(i), sig.get(i), hist.get(i)].iter().flatten() {
            let v = **v;
            if !v.is_nan() {
                if v < mn {
                    mn = v;
                }
                if v > mx {
                    mx = v;
                }
            }
        }
    }
    if mx <= mn {
        mx = mn + 1.0;
    }
    // Zero baseline
    let zero_row = price_to_row(0.0, mn, mx, panel.height) + panel.top;
    let zero_st = Style::default().fg(GRAY2).bg(BLACK);
    for cx in panel.chart_x..panel.chart_x + panel.chart_w {
        buf[(cx, zero_row)].set_char('─').set_style(zero_st);
    }
    // Histogram (vertical bars from zero)
    for (i, raw_idx) in (start_idx..end_idx).enumerate() {
        let v = hist[raw_idx];
        if v.is_nan() {
            continue;
        }
        let row = price_to_row(v, mn, mx, panel.height) + panel.top;
        let color = if v >= 0.0 { GREEN } else { RED };
        let st = Style::default().fg(color).bg(BLACK);
        let slot_x = panel.chart_x as usize + i * slot_w;
        let (top, bot) = if row < zero_row { (row, zero_row) } else { (zero_row, row) };
        for bcx in slot_x..slot_x + body_w {
            for r in top..=bot {
                buf[(bcx as u16, r)].set_char('█').set_style(st);
            }
        }
    }
    // MACD + signal lines
    draw_line_indicator(buf, panel, &macd, start_idx, end_idx, slot_w, body_w, mn, mx, CYAN);
    draw_line_indicator(buf, panel, &sig, start_idx, end_idx, slot_w, body_w, mn, mx, YELLOW);
    write_str(
        buf,
        panel.chart_x + 1,
        panel.top,
        &format!("MACD({},{},{})", fast, slow, signal),
        CYAN,
    );
    // Axis labels
    write_str(
        buf,
        panel.axis_x + 1,
        panel.top,
        &format!("{:.2}", mx),
        GRAY2,
    );
    write_str(
        buf,
        panel.axis_x + 1,
        panel.top + panel.height - 1,
        &format!("{:.2}", mn),
        GRAY2,
    );
}

/// Bresenham line in sub-pixel space using a tiny ad-hoc Braille layer. Same
/// approach as the tview build's `braille.go` (2×4 sub-pixels per cell).
fn draw_line_indicator(
    buf: &mut Buffer,
    panel: &PanelLayout,
    values: &[f64],
    start_idx: usize,
    end_idx: usize,
    slot_w: usize,
    body_w: usize,
    min_v: f64,
    max_v: f64,
    color: ratatui::style::Color,
) {
    use std::collections::HashMap;
    let mut cells: HashMap<(u16, u16), u8> = HashMap::new();
    let panel_h = panel.height as f64 * 4.0 - 1.0;
    let plot = |sub_x: isize, sub_y: isize, cells: &mut HashMap<(u16, u16), u8>| {
        if sub_x < 0 || sub_y < 0 {
            return;
        }
        let cell_x = panel.chart_x + (sub_x as u16) / 2;
        let cell_y = panel.top + (sub_y as u16) / 4;
        if cell_x >= panel.chart_x + panel.chart_w || cell_y >= panel.top + panel.height {
            return;
        }
        let dx = (sub_x as usize) % 2;
        let dy = (sub_y as usize) % 4;
        let bit: u8 = if dy == 3 { 0x40 << dx } else { 1 << (dy + 3 * dx) };
        *cells.entry((cell_x, cell_y)).or_insert(0) |= bit;
    };

    let mut prev: Option<(isize, isize)> = None;
    let mut window_iter = (start_idx..end_idx).enumerate();
    while let Some((i, raw_idx)) = window_iter.next() {
        let v = values.get(raw_idx).copied().unwrap_or(f64::NAN);
        if v.is_nan() || v < min_v || v > max_v {
            prev = None;
            continue;
        }
        let sub_x = ((i * slot_w + body_w / 2) * 2) as isize;
        let sub_y =
            ((max_v - v) / (max_v - min_v) * panel_h).round() as isize;
        if let Some((px, py)) = prev {
            bresenham_thick(px, py, sub_x, sub_y, &mut |x, y| plot(x, y, &mut cells));
        } else {
            plot(sub_x, sub_y, &mut cells);
            plot(sub_x, sub_y + 1, &mut cells);
        }
        prev = Some((sub_x, sub_y));
    }
    let st = Style::default().fg(color).bg(BLACK).add_modifier(Modifier::BOLD);
    for ((cx, cy), bits) in cells {
        let rune = char::from_u32(0x2800 + bits as u32).unwrap_or(' ');
        buf[(cx, cy)].set_char(rune).set_style(st);
    }
}

fn bresenham_thick<F: FnMut(isize, isize)>(x1: isize, y1: isize, x2: isize, y2: isize, mut plot: F) {
    let (dx, dy) = (x2 - x1, y2 - y1);
    let (adx, ady) = (dx.abs(), dy.abs());
    let horizontal = adx >= ady;
    let (sx, sy) = (if dx < 0 { -1 } else { 1 }, if dy < 0 { -1 } else { 1 });
    let mut err = adx - ady;
    let (mut x, mut y) = (x1, y1);
    loop {
        plot(x, y);
        if horizontal {
            plot(x, y + 1);
        } else {
            plot(x + 1, y);
        }
        if x == x2 && y == y2 {
            return;
        }
        let e2 = err * 2;
        if e2 > -ady {
            err -= ady;
            x += sx;
        }
        if e2 < adx {
            err += adx;
            y += sy;
        }
    }
}

// ── Helpers ──────────────────────────────────────────────────────────────

fn write_str(buf: &mut Buffer, x: u16, y: u16, s: &str, fg: ratatui::style::Color) {
    write_str_styled(buf, x, y, s, Style::default().fg(fg).bg(BLACK));
}

fn write_str_styled(buf: &mut Buffer, x: u16, y: u16, s: &str, st: Style) {
    let mut col = x;
    let max_x = buf.area.right();
    for c in s.chars() {
        if col >= max_x {
            break;
        }
        if y >= buf.area.bottom() {
            break;
        }
        buf[(col, y)].set_char(c).set_style(st);
        col += 1;
    }
}

fn truncate(s: &str, max: usize) -> String {
    let mut out = String::new();
    for (i, c) in s.chars().enumerate() {
        if i >= max {
            break;
        }
        out.push(c);
    }
    out
}

fn last_finite(values: &[f64]) -> f64 {
    for &v in values.iter().rev() {
        if !v.is_nan() {
            return v;
        }
    }
    f64::NAN
}

fn fmt_volume_short(v: i64) -> String {
    if v >= 1_000_000_000 {
        format!("{:.1}B", v as f64 / 1e9)
    } else if v >= 1_000_000 {
        format!("{:.1}M", v as f64 / 1e6)
    } else if v >= 1_000 {
        format!("{:.0}K", v as f64 / 1e3)
    } else {
        v.to_string()
    }
}
