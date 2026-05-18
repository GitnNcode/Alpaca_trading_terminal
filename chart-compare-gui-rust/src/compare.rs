// Compare tab — multi-asset side-by-side risk/return view.
//
// Up to 4 assets selectable. For each combination we render:
//   * Stats table (CAGR, vol, Sharpe, Sortino, MaxDD, Calmar) with best/worst
//     in each column highlighted
//   * Normalized return chart (each series starts at 100; legend toggles)
//   * Drawdown overlay (underwater curves, shared X with the normalized chart)
//   * Correlation heatmap (N×N on daily log returns)
//   * Risk/return scatter (annualized vol on X, CAGR on Y)
//   * Optional Monte Carlo growth projection for one of the selected assets
//
// All bars come from the same Alpaca /v2/stocks/{sym}/bars endpoint the
// chart tab uses — no extra data sources, no extra deps.

use std::sync::mpsc::Sender;
use std::sync::Arc;

use egui::{Color32, Key, RichText, Stroke};
use egui_plot::{Legend, Line, MarkerShape, Plot, PlotPoints, Points, Polygon, Text};

use crate::api::{AlpacaClient, Bar};
use crate::stocks::AssetCache;
use crate::theme;
use crate::workers::{self, Msg};

pub const MAX_SLOTS: usize = 4;

/// One color per slot — re-used across the chip, stats row, normalized line,
/// drawdown line, and scatter point so the eye can track each asset.
pub const SLOT_COLORS: [Color32; 4] = [
    theme::CYAN,
    theme::ORANGE,
    theme::YELLOW,
    theme::GREEN,
];

pub struct Slot {
    pub symbol: String,
    pub bars: Vec<Bar>,
    pub loading: bool,
    pub err: String,
    /// Per-slot generation. Bumped each time a load is kicked off for *this*
    /// slot so responses for older loads get discarded on arrival. We don't
    /// use a single global gen because adding a second symbol while the first
    /// is still loading must NOT invalidate the first's pending response.
    pub gen: u64,
}

impl Slot {
    fn new(symbol: String) -> Self {
        Slot {
            symbol,
            bars: Vec::new(),
            loading: false,
            err: String::new(),
            gen: 0,
        }
    }
}

pub struct CompareRange {
    pub label: &'static str,
    pub years: u32,
}

/// Compare uses its own range list (always paired with daily bars). Risk
/// metrics on intraday bars don't really make sense, so we expose only the
/// lookbacks that actually matter for portfolio comparison.
pub const COMPARE_RANGES: &[CompareRange] = &[
    CompareRange { label: "1Y", years: 1 },
    CompareRange { label: "3Y", years: 3 },
    CompareRange { label: "5Y", years: 5 },
    CompareRange { label: "10Y", years: 10 },
];

pub const MC_HORIZONS: &[CompareRange] = &[
    CompareRange { label: "1y", years: 1 },
    CompareRange { label: "3y", years: 3 },
    CompareRange { label: "5y", years: 5 },
    CompareRange { label: "10y", years: 10 },
];

pub struct CompareState {
    pub slots: Vec<Slot>,
    pub input: String,
    pub autocomplete: Vec<(String, String)>,
    pub range_idx: usize,
    pub mc_show: bool,
    pub mc_horizon_idx: usize,
    pub mc_asset_idx: usize,
    pub mc_results: Option<MonteCarloResult>,
    pub mc_seed: u64,
    pub mc_n_sims: usize,
    /// Monotonic counter used to derive per-slot gens. Each load bumps it.
    pub seq: u64,
    /// When false (default), plots are static and the page scrolls freely.
    /// When true, plots capture drag/scroll/zoom and the outer ScrollArea is
    /// disabled — otherwise mouse-wheel events would fight between the two.
    pub interactive: bool,
}

impl CompareState {
    pub fn new() -> Self {
        CompareState {
            slots: Vec::new(),
            input: String::new(),
            autocomplete: Vec::new(),
            range_idx: 1, // 3Y default — long enough for stable stats, short enough to load quickly
            mc_show: false,
            mc_horizon_idx: 0,
            mc_asset_idx: 0,
            mc_results: None,
            mc_seed: 0xC0FFEE_DEAD_BEEFu64,
            mc_n_sims: 1000,
            seq: 0,
            interactive: false,
        }
    }

    fn next_gen(&mut self) -> u64 {
        self.seq = self.seq.wrapping_add(1);
        self.seq
    }

    pub fn add_symbol(
        &mut self,
        sym: String,
        client: Arc<AlpacaClient>,
        tx: Sender<Msg>,
        ctx: &egui::Context,
    ) {
        let sym = sym.trim().to_uppercase();
        if sym.is_empty() || self.slots.len() >= MAX_SLOTS {
            return;
        }
        if self.slots.iter().any(|s| s.symbol == sym) {
            return;
        }
        let gen = self.next_gen();
        let mut slot = Slot::new(sym.clone());
        slot.loading = true;
        slot.gen = gen;
        self.slots.push(slot);
        let years = COMPARE_RANGES[self.range_idx].years;
        workers::spawn_load_compare_bars(
            client,
            tx,
            ctx.clone(),
            sym,
            self.range_idx,
            (years as i64) * 365 * 24,
            gen,
        );
        self.mc_results = None;
    }

    pub fn remove_slot(&mut self, idx: usize) {
        if idx < self.slots.len() {
            self.slots.remove(idx);
            if self.mc_asset_idx >= self.slots.len() {
                self.mc_asset_idx = self.slots.len().saturating_sub(1);
            }
            self.mc_results = None;
        }
    }

    /// Reload every slot from the current range. Per-slot gens are bumped so
    /// any in-flight older responses are discarded on arrival.
    pub fn reload_all(
        &mut self,
        client: Arc<AlpacaClient>,
        tx: Sender<Msg>,
        ctx: &egui::Context,
    ) {
        let years = COMPARE_RANGES[self.range_idx].years;
        let range_idx = self.range_idx;
        for i in 0..self.slots.len() {
            let gen = self.next_gen();
            let slot = &mut self.slots[i];
            slot.loading = true;
            slot.err.clear();
            slot.bars.clear();
            slot.gen = gen;
            workers::spawn_load_compare_bars(
                client.clone(),
                tx.clone(),
                ctx.clone(),
                slot.symbol.clone(),
                range_idx,
                (years as i64) * 365 * 24,
                gen,
            );
        }
        self.mc_results = None;
    }

    pub fn refresh_autocomplete(&mut self, assets: &AssetCache) {
        if self.input.is_empty() {
            self.autocomplete.clear();
            return;
        }
        self.autocomplete = assets.filter(&self.input, 6);
    }
}

// ── Math ─────────────────────────────────────────────────────────────────

const TRADING_DAYS_PER_YEAR: f64 = 252.0;

pub fn daily_log_returns(closes: &[f64]) -> Vec<f64> {
    let mut out = Vec::with_capacity(closes.len().saturating_sub(1));
    for i in 1..closes.len() {
        if closes[i - 1] > 0.0 && closes[i] > 0.0 {
            out.push((closes[i] / closes[i - 1]).ln());
        } else {
            out.push(0.0);
        }
    }
    out
}

pub fn cagr(closes: &[f64]) -> f64 {
    if closes.len() < 2 || closes[0] <= 0.0 {
        return 0.0;
    }
    let total = closes[closes.len() - 1] / closes[0];
    let years = (closes.len() - 1) as f64 / TRADING_DAYS_PER_YEAR;
    if years <= 0.0 || total <= 0.0 {
        return 0.0;
    }
    total.powf(1.0 / years) - 1.0
}

pub fn annualized_vol(returns: &[f64]) -> f64 {
    if returns.len() < 2 {
        return 0.0;
    }
    let n = returns.len() as f64;
    let mean: f64 = returns.iter().sum::<f64>() / n;
    let var: f64 =
        returns.iter().map(|r| (r - mean).powi(2)).sum::<f64>() / (n - 1.0);
    var.sqrt() * TRADING_DAYS_PER_YEAR.sqrt()
}

pub fn sharpe(returns: &[f64]) -> f64 {
    if returns.len() < 2 {
        return 0.0;
    }
    let n = returns.len() as f64;
    let mean: f64 = returns.iter().sum::<f64>() / n;
    let var: f64 =
        returns.iter().map(|r| (r - mean).powi(2)).sum::<f64>() / (n - 1.0);
    let sd = var.sqrt();
    if sd == 0.0 {
        return 0.0;
    }
    (mean / sd) * TRADING_DAYS_PER_YEAR.sqrt()
}

pub fn sortino(returns: &[f64]) -> f64 {
    if returns.len() < 2 {
        return 0.0;
    }
    let n = returns.len() as f64;
    let mean: f64 = returns.iter().sum::<f64>() / n;
    let downside_sq: f64 = returns
        .iter()
        .filter(|r| **r < 0.0)
        .map(|r| r.powi(2))
        .sum::<f64>();
    if downside_sq == 0.0 {
        return 0.0;
    }
    let downside_sd = (downside_sq / n).sqrt();
    (mean / downside_sd) * TRADING_DAYS_PER_YEAR.sqrt()
}

/// Returns a negative value in [-1, 0]. 0 = no drawdown.
pub fn max_drawdown(closes: &[f64]) -> f64 {
    if closes.is_empty() {
        return 0.0;
    }
    let mut peak = closes[0];
    let mut max_dd: f64 = 0.0;
    for &c in closes {
        if c > peak {
            peak = c;
        }
        if peak > 0.0 {
            let dd = (c - peak) / peak;
            if dd < max_dd {
                max_dd = dd;
            }
        }
    }
    max_dd
}

pub fn drawdown_series(closes: &[f64]) -> Vec<f64> {
    let mut out = Vec::with_capacity(closes.len());
    let mut peak = closes.first().copied().unwrap_or(0.0);
    for &c in closes {
        if c > peak {
            peak = c;
        }
        let v = if peak > 0.0 { (c - peak) / peak } else { 0.0 };
        out.push(v);
    }
    out
}

pub fn calmar(closes: &[f64]) -> f64 {
    let c = cagr(closes);
    let dd = max_drawdown(closes).abs();
    if dd == 0.0 {
        return 0.0;
    }
    c / dd
}

pub fn pearson(a: &[f64], b: &[f64]) -> f64 {
    let n = a.len().min(b.len());
    if n < 2 {
        return 0.0;
    }
    let mean_a: f64 = a[..n].iter().sum::<f64>() / n as f64;
    let mean_b: f64 = b[..n].iter().sum::<f64>() / n as f64;
    let mut num = 0.0;
    let mut den_a = 0.0;
    let mut den_b = 0.0;
    for i in 0..n {
        let da = a[i] - mean_a;
        let db = b[i] - mean_b;
        num += da * db;
        den_a += da * da;
        den_b += db * db;
    }
    let den = (den_a * den_b).sqrt();
    if den == 0.0 {
        0.0
    } else {
        num / den
    }
}

// ── RNG (xorshift64* + Box-Muller) ───────────────────────────────────────
//
// Inlined to avoid pulling in the `rand` crate for one tab. Quality is plenty
// for visualizing growth distributions — we're not running cryptography or
// quant production code, just driving a fan chart.

pub struct Xorshift64(pub u64);

impl Xorshift64 {
    pub fn new(seed: u64) -> Self {
        Xorshift64(if seed == 0 { 0xDEAD_BEEF_C0FFEEu64 } else { seed })
    }
    pub fn next_u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }
    pub fn next_f64(&mut self) -> f64 {
        // Maps to (0, 1) using top 53 bits, +0.5 to avoid the lower endpoint
        // (Box-Muller needs ln(u) so u must be strictly > 0).
        let x = self.next_u64() >> 11;
        (x as f64 + 0.5) / ((1u64 << 53) as f64)
    }
    pub fn next_normal(&mut self) -> f64 {
        let u1 = self.next_f64();
        let u2 = self.next_f64();
        (-2.0 * u1.ln()).sqrt() * (2.0 * std::f64::consts::PI * u2).cos()
    }
}

// ── Monte Carlo ──────────────────────────────────────────────────────────

pub struct MonteCarloResult {
    pub symbol: String,
    pub horizon_years: u32,
    #[allow(dead_code)]
    pub days: usize,
    pub median: Vec<f64>,
    pub p5: Vec<f64>,
    pub p95: Vec<f64>,
    pub prob_above_start: f64,
    pub prob_50_dd: f64,
    pub final_p5: f64,
    pub final_p50: f64,
    pub final_p95: f64,
    pub mu_daily: f64,
    pub sigma_daily: f64,
    pub n_sims: usize,
}

pub fn run_monte_carlo(
    symbol: &str,
    closes: &[f64],
    horizon_years: u32,
    n_sims: usize,
    seed: u64,
) -> Option<MonteCarloResult> {
    let returns = daily_log_returns(closes);
    if returns.len() < 30 {
        return None;
    }
    let n = returns.len() as f64;
    let mu: f64 = returns.iter().sum::<f64>() / n;
    let var: f64 =
        returns.iter().map(|r| (r - mu).powi(2)).sum::<f64>() / (n - 1.0);
    let sigma = var.sqrt();
    let days = (horizon_years as usize) * (TRADING_DAYS_PER_YEAR as usize);
    if days == 0 || n_sims == 0 {
        return None;
    }

    let mut rng = Xorshift64::new(seed);
    let mut paths: Vec<Vec<f64>> = Vec::with_capacity(n_sims);
    let mut hit_50_dd_count = 0u32;
    let mut above_start_count = 0u32;
    let mut final_vals: Vec<f64> = Vec::with_capacity(n_sims);

    for _ in 0..n_sims {
        let mut path = Vec::with_capacity(days + 1);
        path.push(1.0);
        let mut cum: f64 = 0.0;
        let mut peak: f64 = 1.0;
        let mut hit_50_dd = false;
        for _ in 0..days {
            let z = rng.next_normal();
            cum += mu + sigma * z;
            let v = cum.exp();
            if v > peak {
                peak = v;
            }
            if peak > 0.0 && v / peak <= 0.5 {
                hit_50_dd = true;
            }
            path.push(v);
        }
        let final_v = *path.last().unwrap();
        if final_v > 1.0 {
            above_start_count += 1;
        }
        if hit_50_dd {
            hit_50_dd_count += 1;
        }
        final_vals.push(final_v);
        paths.push(path);
    }

    // Per-step percentiles via per-column sort.
    let mut median = vec![0.0; days + 1];
    let mut p5 = vec![0.0; days + 1];
    let mut p95 = vec![0.0; days + 1];
    let mut col = vec![0.0; n_sims];
    let pct = |sorted: &[f64], q: f64| -> f64 {
        let idx = ((sorted.len() as f64 - 1.0) * q).round() as usize;
        sorted[idx.min(sorted.len() - 1)]
    };
    for d in 0..=days {
        for s in 0..n_sims {
            col[s] = paths[s][d];
        }
        col.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        p5[d] = pct(&col, 0.05);
        median[d] = pct(&col, 0.50);
        p95[d] = pct(&col, 0.95);
    }

    final_vals
        .sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let final_p5 = pct(&final_vals, 0.05);
    let final_p50 = pct(&final_vals, 0.50);
    let final_p95 = pct(&final_vals, 0.95);

    Some(MonteCarloResult {
        symbol: symbol.to_string(),
        horizon_years,
        days,
        median,
        p5,
        p95,
        prob_above_start: above_start_count as f64 / n_sims as f64,
        prob_50_dd: hit_50_dd_count as f64 / n_sims as f64,
        final_p5,
        final_p50,
        final_p95,
        mu_daily: mu,
        sigma_daily: sigma,
        n_sims,
    })
}

// ── Helpers ──────────────────────────────────────────────────────────────

/// Right-aligned closes across all slots so charts and metrics line up. Returns
/// one Vec<f64> per slot, each of length `min_len`. Returns empty Vecs if
/// any slot is empty or the alignment would produce fewer than 2 points.
fn aligned_closes(slots: &[Slot]) -> Vec<Vec<f64>> {
    let lens: Vec<usize> = slots
        .iter()
        .map(|s| s.bars.len())
        .filter(|&n| n > 0)
        .collect();
    if lens.len() != slots.len() || lens.is_empty() {
        return slots.iter().map(|_| Vec::new()).collect();
    }
    let min_len = *lens.iter().min().unwrap();
    if min_len < 2 {
        return slots.iter().map(|_| Vec::new()).collect();
    }
    slots
        .iter()
        .map(|s| {
            let bars = &s.bars;
            let start = bars.len() - min_len;
            bars[start..].iter().map(|b| b.close).collect()
        })
        .collect()
}

// ── UI ───────────────────────────────────────────────────────────────────

pub fn render(
    state: &mut CompareState,
    client: Arc<AlpacaClient>,
    tx: Sender<Msg>,
    assets: &AssetCache,
    ui: &mut egui::Ui,
) {
    render_picker(state, client.clone(), tx.clone(), assets, ui);
    ui.separator();

    if state.slots.is_empty() {
        ui.add_space(20.0);
        ui.vertical_centered(|ui| {
            ui.label(
                RichText::new("Add up to 4 symbols to compare")
                    .color(theme::GRAY2)
                    .size(18.0),
            );
            ui.add_space(4.0);
            ui.label(
                RichText::new("Metrics, normalized returns, drawdowns, correlations, and Monte Carlo")
                    .color(theme::GRAY)
                    .size(13.0),
            );
        });
        return;
    }

    if state.slots.iter().any(|s| s.loading) {
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            ui.label(
                RichText::new("Loading…")
                    .color(theme::YELLOW)
                    .size(14.0),
            );
            for s in &state.slots {
                if s.loading {
                    ui.label(RichText::new(&s.symbol).color(theme::GRAY2));
                }
            }
        });
    }
    let errs: Vec<&Slot> = state
        .slots
        .iter()
        .filter(|s| !s.err.is_empty())
        .collect();
    if !errs.is_empty() {
        for s in errs {
            ui.label(
                RichText::new(format!("• {}: {}", s.symbol, s.err))
                    .color(theme::RED),
            );
        }
    }

    let closes = aligned_closes(&state.slots);
    let ready = !closes.iter().any(|c| c.is_empty());

    // When unlocked the plots own the wheel/drag events, so the outer
    // ScrollArea must release them — otherwise scrolling on a chart fights
    // the page. When locked the inverse: charts are static, page scrolls.
    egui::ScrollArea::vertical()
        .enable_scrolling(!state.interactive)
        .show(ui, |ui| {
        if ready {
            render_stats_table(state, &closes, ui);
            ui.add_space(8.0);
            render_normalized_and_drawdown(state, &closes, ui);
            ui.add_space(8.0);
            ui.columns(2, |cols| {
                render_correlation(state, &closes, &mut cols[0]);
                render_risk_return(state, &closes, &mut cols[1]);
            });
            ui.add_space(8.0);
            render_monte_carlo(state, &closes, ui);
        } else if !state.slots.iter().any(|s| s.loading)
            && state.slots.iter().all(|s| s.err.is_empty())
        {
            ui.add_space(8.0);
            ui.label(
                RichText::new("Not enough data to align series — try a longer range.")
                    .color(theme::GRAY2),
            );
        }
    });
}

/// Apply the Compare-tab lock to a plot. When `interactive` is false the plot
/// is fully static (no drag, no scroll-wheel zoom, no pinch-zoom) so the outer
/// page can scroll past it without being hijacked by the wheel.
fn apply_lock<'a>(plot: Plot<'a>, interactive: bool) -> Plot<'a> {
    let v = egui::Vec2b::new(interactive, interactive);
    plot.allow_drag(v).allow_scroll(v).allow_zoom(v)
}

fn render_picker(
    state: &mut CompareState,
    client: Arc<AlpacaClient>,
    tx: Sender<Msg>,
    assets: &AssetCache,
    ui: &mut egui::Ui,
) {
    ui.add_space(4.0);
    ui.horizontal_wrapped(|ui| {
        ui.label(
            RichText::new(" COMPARE ")
                .color(theme::ORANGE)
                .strong(),
        );
        ui.separator();
        ui.label(
            RichText::new(format!("{}/{}", state.slots.len(), MAX_SLOTS))
                .color(theme::CYAN)
                .strong(),
        );
        ui.separator();

        // Chips for selected symbols
        let mut to_remove: Option<usize> = None;
        for (i, slot) in state.slots.iter().enumerate() {
            let color = SLOT_COLORS[i];
            ui.add(
                egui::Button::new(
                    RichText::new(format!(" {} ", slot.symbol))
                        .color(theme::BLACK)
                        .strong(),
                )
                .fill(color)
                .stroke(Stroke::NONE),
            );
            if ui
                .add(
                    egui::Button::new(
                        RichText::new("✕").color(theme::WHITE).strong(),
                    )
                    .fill(theme::DARK),
                )
                .on_hover_text(format!("Remove {}", slot.symbol))
                .clicked()
            {
                to_remove = Some(i);
            }
            ui.add_space(2.0);
        }
        if let Some(i) = to_remove {
            state.remove_slot(i);
        }

        if state.slots.len() < MAX_SLOTS {
            ui.separator();
            ui.label(RichText::new("ADD").color(theme::ORANGE).strong());
            let resp = ui.add(
                egui::TextEdit::singleline(&mut state.input)
                    .desired_width(110.0)
                    .hint_text("AAPL"),
            );
            if resp.changed() {
                state.input = state.input.to_uppercase();
                state.refresh_autocomplete(assets);
            }
            let submitted =
                resp.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter));
            if ui.button(" + Add ").clicked() || submitted {
                let s = std::mem::take(&mut state.input);
                state.autocomplete.clear();
                state.add_symbol(s, client.clone(), tx.clone(), ui.ctx());
            }
            // Autocomplete chips
            if resp.has_focus() && !state.autocomplete.is_empty() {
                let suggestions = state.autocomplete.clone();
                for (sym, _name) in suggestions.iter().take(6) {
                    if ui
                        .add(
                            egui::Button::new(
                                RichText::new(sym).color(theme::CYAN),
                            )
                            .fill(theme::DARK),
                        )
                        .clicked()
                    {
                        state
                            .add_symbol(
                                sym.clone(),
                                client.clone(),
                                tx.clone(),
                                ui.ctx(),
                            );
                        state.input.clear();
                        state.autocomplete.clear();
                    }
                }
            }
        }
    });

    // Range pills + lock toggle. The lock toggle decides whether the chart
    // pane captures wheel/drag (interactive) or the outer page does (locked).
    ui.add_space(2.0);
    ui.horizontal_wrapped(|ui| {
        ui.label(RichText::new("RANGE").color(theme::ORANGE).strong());
        for (i, r) in COMPARE_RANGES.iter().enumerate() {
            let active = i == state.range_idx;
            let label = format!(" {} ", r.label);
            let btn = if active {
                egui::Button::new(
                    RichText::new(label).color(theme::BLACK).strong(),
                )
                .fill(theme::ORANGE)
            } else {
                egui::Button::new(RichText::new(label).color(theme::GRAY2))
                    .fill(theme::DARK)
            };
            if ui.add(btn).clicked() && i != state.range_idx {
                state.range_idx = i;
                state.reload_all(client.clone(), tx.clone(), ui.ctx());
            }
        }
        ui.label(
            RichText::new("(daily bars)")
                .color(theme::GRAY)
                .italics(),
        );

        ui.add_space(16.0);
        ui.separator();
        let (label, fill, hint) = if state.interactive {
            (
                " 🔓 UNLOCKED — click to lock ",
                theme::GREEN,
                "Charts capture scroll & drag. Page scroll is disabled.",
            )
        } else {
            (
                " 🔒 LOCKED — click to unlock ",
                theme::GRAY2,
                "Charts are static so the page scrolls. Click to interact with charts.",
            )
        };
        let resp = ui
            .add(
                egui::Button::new(
                    RichText::new(label).color(theme::BLACK).strong(),
                )
                .fill(fill),
            )
            .on_hover_text(hint);
        if resp.clicked() {
            state.interactive = !state.interactive;
        }
    });
    ui.add_space(2.0);
}

// ── Stats table ──────────────────────────────────────────────────────────

#[derive(Copy, Clone, PartialEq, Eq)]
enum Direction {
    HigherBetter,
    LowerBetter,
}

fn best_worst(values: &[f64], dir: Direction) -> (Option<usize>, Option<usize>) {
    if values.len() < 2 {
        return (None, None);
    }
    let mut best_i = 0;
    let mut worst_i = 0;
    for i in 1..values.len() {
        let v = values[i];
        let better_than_best = match dir {
            Direction::HigherBetter => v > values[best_i],
            Direction::LowerBetter => v < values[best_i],
        };
        let worse_than_worst = match dir {
            Direction::HigherBetter => v < values[worst_i],
            Direction::LowerBetter => v > values[worst_i],
        };
        if better_than_best {
            best_i = i;
        }
        if worse_than_worst {
            worst_i = i;
        }
    }
    (Some(best_i), Some(worst_i))
}

fn colored_value(ui: &mut egui::Ui, text: String, i: usize, best: Option<usize>, worst: Option<usize>) {
    let color = if best == Some(i) {
        theme::GREEN
    } else if worst == Some(i) {
        theme::RED
    } else {
        theme::WHITE
    };
    let mut rt = RichText::new(text).color(color);
    if best == Some(i) || worst == Some(i) {
        rt = rt.strong();
    }
    ui.label(rt);
}

fn render_stats_table(state: &CompareState, closes: &[Vec<f64>], ui: &mut egui::Ui) {
    ui.add_space(4.0);
    ui.label(
        RichText::new(" METRICS ")
            .color(theme::ORANGE)
            .strong()
            .size(14.0),
    );

    let cagrs: Vec<f64> = closes.iter().map(|c| cagr(c)).collect();
    let vols: Vec<f64> = closes
        .iter()
        .map(|c| annualized_vol(&daily_log_returns(c)))
        .collect();
    let sharpes: Vec<f64> = closes
        .iter()
        .map(|c| sharpe(&daily_log_returns(c)))
        .collect();
    let sortinos: Vec<f64> = closes
        .iter()
        .map(|c| sortino(&daily_log_returns(c)))
        .collect();
    let max_dds: Vec<f64> = closes.iter().map(|c| max_drawdown(c)).collect();
    let calmars: Vec<f64> = closes.iter().map(|c| calmar(c)).collect();

    let bw_cagr = best_worst(&cagrs, Direction::HigherBetter);
    let bw_vol = best_worst(&vols, Direction::LowerBetter);
    let bw_sharpe = best_worst(&sharpes, Direction::HigherBetter);
    let bw_sortino = best_worst(&sortinos, Direction::HigherBetter);
    // For max DD: values are <= 0. "Better" = closer to 0 = higher.
    let bw_max_dd = best_worst(&max_dds, Direction::HigherBetter);
    let bw_calmar = best_worst(&calmars, Direction::HigherBetter);

    egui::Grid::new("compare_stats")
        .striped(true)
        .spacing([16.0, 4.0])
        .show(ui, |ui| {
            // Header row
            let h = |text: &str| RichText::new(text).color(theme::ORANGE).strong();
            ui.label(h("Asset"));
            ui.label(h("CAGR"));
            ui.label(h("Ann. Vol"));
            ui.label(h("Sharpe"));
            ui.label(h("Sortino"));
            ui.label(h("Max DD"));
            ui.label(h("Calmar"));
            ui.end_row();

            for (i, slot) in state.slots.iter().enumerate() {
                ui.horizontal(|ui| {
                    let dot = "●";
                    ui.label(RichText::new(dot).color(SLOT_COLORS[i]).strong());
                    ui.label(
                        RichText::new(&slot.symbol)
                            .color(theme::WHITE)
                            .strong(),
                    );
                });
                colored_value(ui, format!("{:+.2}%", cagrs[i] * 100.0), i, bw_cagr.0, bw_cagr.1);
                colored_value(ui, format!("{:.2}%", vols[i] * 100.0), i, bw_vol.0, bw_vol.1);
                colored_value(ui, format!("{:.2}", sharpes[i]), i, bw_sharpe.0, bw_sharpe.1);
                colored_value(ui, format!("{:.2}", sortinos[i]), i, bw_sortino.0, bw_sortino.1);
                colored_value(ui, format!("{:.2}%", max_dds[i] * 100.0), i, bw_max_dd.0, bw_max_dd.1);
                colored_value(ui, format!("{:.2}", calmars[i]), i, bw_calmar.0, bw_calmar.1);
                ui.end_row();
            }
        });
    ui.add_space(2.0);
    ui.label(
        RichText::new("Best in green, worst in red. CAGR & volatility annualized from daily log returns (252 trading days).")
            .color(theme::GRAY)
            .size(11.0)
            .italics(),
    );
}

// ── Normalized + drawdown charts ─────────────────────────────────────────

fn render_normalized_and_drawdown(
    state: &CompareState,
    closes: &[Vec<f64>],
    ui: &mut egui::Ui,
) {
    ui.label(
        RichText::new(" NORMALIZED RETURN (base = 100, click legend to toggle) ")
            .color(theme::ORANGE)
            .strong()
            .size(14.0),
    );

    let axis_group = ui.id().with("compare_axis_link");
    let cursor_group = ui.id().with("compare_cursor_link");

    apply_lock(
        Plot::new("compare_normalized")
            .height(240.0)
            .legend(Legend::default().background_alpha(0.35))
            .link_axis(axis_group, true, false)
            .link_cursor(cursor_group, true, false)
            .show_axes([true, true])
            .show_x(false),
        state.interactive,
    )
        .show(ui, |plot_ui| {
            for (i, slot) in state.slots.iter().enumerate() {
                let series = &closes[i];
                if series.is_empty() || series[0] == 0.0 {
                    continue;
                }
                let base = series[0];
                let pts: PlotPoints = series
                    .iter()
                    .enumerate()
                    .map(|(x, v)| [x as f64, v / base * 100.0])
                    .collect();
                plot_ui.line(
                    Line::new(pts)
                        .color(SLOT_COLORS[i])
                        .stroke(Stroke::new(2.0, SLOT_COLORS[i]))
                        .name(&slot.symbol),
                );
            }
        });

    ui.add_space(4.0);
    ui.label(
        RichText::new(" DRAWDOWN ")
            .color(theme::ORANGE)
            .strong()
            .size(14.0),
    );
    apply_lock(
        Plot::new("compare_drawdown")
            .height(160.0)
            .legend(Legend::default().background_alpha(0.35))
            .link_axis(axis_group, true, false)
            .link_cursor(cursor_group, true, false)
            .show_axes([true, true])
            .show_x(false)
            .y_axis_formatter(|m, _| format!("{:.0}%", m.value * 100.0)),
        state.interactive,
    )
        .show(ui, |plot_ui| {
            for (i, slot) in state.slots.iter().enumerate() {
                let series = &closes[i];
                if series.is_empty() {
                    continue;
                }
                let dd = drawdown_series(series);
                let pts: PlotPoints = dd
                    .iter()
                    .enumerate()
                    .map(|(x, v)| [x as f64, *v])
                    .collect();
                plot_ui.line(
                    Line::new(pts)
                        .color(SLOT_COLORS[i])
                        .stroke(Stroke::new(1.6, SLOT_COLORS[i]))
                        .name(&slot.symbol),
                );
            }
        });
}

// ── Correlation heatmap ─────────────────────────────────────────────────

fn render_correlation(state: &CompareState, closes: &[Vec<f64>], ui: &mut egui::Ui) {
    ui.label(
        RichText::new(" CORRELATION (daily log returns) ")
            .color(theme::ORANGE)
            .strong()
            .size(14.0),
    );
    let returns: Vec<Vec<f64>> =
        closes.iter().map(|c| daily_log_returns(c)).collect();
    let n = state.slots.len();

    egui::Grid::new("compare_corr")
        .spacing([2.0, 2.0])
        .show(ui, |ui| {
            // Header row: blank + symbols
            ui.label("");
            for (j, s) in state.slots.iter().enumerate() {
                ui.horizontal(|ui| {
                    ui.label(RichText::new("●").color(SLOT_COLORS[j]).strong());
                    ui.label(
                        RichText::new(&s.symbol)
                            .color(theme::ORANGE)
                            .strong(),
                    );
                });
            }
            ui.end_row();

            for i in 0..n {
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new("●").color(SLOT_COLORS[i]).strong(),
                    );
                    ui.label(
                        RichText::new(&state.slots[i].symbol)
                            .color(theme::ORANGE)
                            .strong(),
                    );
                });
                for j in 0..n {
                    let c = if i == j {
                        1.0
                    } else {
                        pearson(&returns[i], &returns[j])
                    };
                    let bg = corr_color(c);
                    let fg = if c.abs() > 0.55 {
                        theme::BLACK
                    } else {
                        theme::WHITE
                    };
                    egui::Frame::default()
                        .fill(bg)
                        .inner_margin(egui::Margin::symmetric(10.0, 6.0))
                        .show(ui, |ui| {
                            ui.label(
                                RichText::new(format!("{:+.2}", c))
                                    .color(fg)
                                    .strong()
                                    .monospace(),
                            );
                        });
                }
                ui.end_row();
            }
        });
}

/// Maps a correlation in [-1, 1] to a color. Negative = red, ~0 = dark gray,
/// positive = green. Saturation grows with |c|.
fn corr_color(c: f64) -> Color32 {
    let c = c.clamp(-1.0, 1.0);
    if c >= 0.0 {
        // dark gray → green
        let t = c as f32;
        lerp_color(Color32::from_rgb(40, 40, 40), theme::GREEN, t)
    } else {
        let t = (-c) as f32;
        lerp_color(Color32::from_rgb(40, 40, 40), theme::RED, t)
    }
}

fn lerp_color(a: Color32, b: Color32, t: f32) -> Color32 {
    let t = t.clamp(0.0, 1.0);
    let lerp = |x: u8, y: u8| (x as f32 * (1.0 - t) + y as f32 * t).round() as u8;
    Color32::from_rgb(
        lerp(a.r(), b.r()),
        lerp(a.g(), b.g()),
        lerp(a.b(), b.b()),
    )
}

// ── Risk/return scatter ─────────────────────────────────────────────────

fn render_risk_return(state: &CompareState, closes: &[Vec<f64>], ui: &mut egui::Ui) {
    ui.label(
        RichText::new(" RISK / RETURN ")
            .color(theme::ORANGE)
            .strong()
            .size(14.0),
    );
    apply_lock(
        Plot::new("compare_scatter")
            .height(220.0)
            .show_axes([true, true])
            .x_axis_label("Ann. Volatility")
            .y_axis_label("CAGR")
            .x_axis_formatter(|m, _| format!("{:.0}%", m.value * 100.0))
            .y_axis_formatter(|m, _| format!("{:+.0}%", m.value * 100.0)),
        state.interactive,
    )
        .show(ui, |plot_ui| {
            for (i, slot) in state.slots.iter().enumerate() {
                let series = &closes[i];
                if series.is_empty() {
                    continue;
                }
                let vol = annualized_vol(&daily_log_returns(series));
                let cgr = cagr(series);
                plot_ui.points(
                    Points::new(PlotPoints::new(vec![[vol, cgr]]))
                        .shape(MarkerShape::Circle)
                        .color(SLOT_COLORS[i])
                        .radius(8.0)
                        .filled(true)
                        .name(&slot.symbol),
                );
                plot_ui.text(
                    Text::new([vol, cgr].into(), RichText::new(format!("  {}", slot.symbol)).color(theme::WHITE).strong())
                        .anchor(egui::Align2::LEFT_CENTER),
                );
            }
        });
}

// ── Monte Carlo ─────────────────────────────────────────────────────────

fn render_monte_carlo(state: &mut CompareState, closes: &[Vec<f64>], ui: &mut egui::Ui) {
    let header = RichText::new(" MONTE CARLO GROWTH PROJECTION ")
        .color(theme::ORANGE)
        .strong()
        .size(14.0);
    egui::CollapsingHeader::new(header)
        .id_salt("compare_mc_header")
        .default_open(state.mc_show)
        .show(ui, |ui| {
            state.mc_show = true;
            if state.mc_asset_idx >= state.slots.len() {
                state.mc_asset_idx = 0;
            }
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new("ASSET").color(theme::ORANGE).strong());
                for (i, slot) in state.slots.iter().enumerate() {
                    let active = i == state.mc_asset_idx;
                    let label = format!(" {} ", slot.symbol);
                    let btn = if active {
                        egui::Button::new(
                            RichText::new(label).color(theme::BLACK).strong(),
                        )
                        .fill(SLOT_COLORS[i])
                    } else {
                        egui::Button::new(RichText::new(label).color(theme::GRAY2))
                            .fill(theme::DARK)
                    };
                    if ui.add(btn).clicked() {
                        state.mc_asset_idx = i;
                        state.mc_results = None;
                    }
                }
                ui.add_space(12.0);
                ui.label(RichText::new("HORIZON").color(theme::ORANGE).strong());
                for (i, h) in MC_HORIZONS.iter().enumerate() {
                    let active = i == state.mc_horizon_idx;
                    let label = format!(" {} ", h.label);
                    let btn = if active {
                        egui::Button::new(
                            RichText::new(label).color(theme::BLACK).strong(),
                        )
                        .fill(theme::CYAN)
                    } else {
                        egui::Button::new(RichText::new(label).color(theme::GRAY2))
                            .fill(theme::DARK)
                    };
                    if ui.add(btn).clicked() {
                        state.mc_horizon_idx = i;
                        state.mc_results = None;
                    }
                }
                ui.add_space(12.0);
                ui.label(RichText::new("SIMS").color(theme::ORANGE).strong());
                ui.add(
                    egui::DragValue::new(&mut state.mc_n_sims)
                        .range(100..=10_000)
                        .speed(10.0),
                )
                .on_hover_text("Click to type, drag to scrub. 100–10,000 paths.");
                ui.add_space(6.0);
                let run_label = if state.mc_results.is_some() {
                    format!(" Re-run {} sims ", state.mc_n_sims)
                } else {
                    format!(" Run {} sims ", state.mc_n_sims)
                };
                if ui
                    .add(
                        egui::Button::new(
                            RichText::new(run_label).color(theme::BLACK).strong(),
                        )
                        .fill(theme::GREEN),
                    )
                    .clicked()
                {
                    let series = &closes[state.mc_asset_idx];
                    let h = MC_HORIZONS[state.mc_horizon_idx].years;
                    // Re-seed each run with a stable nonce so users can hit
                    // Re-run to see a different draw, but a given asset+horizon
                    // is reproducible within one run.
                    state.mc_seed = state.mc_seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
                    state.mc_results = run_monte_carlo(
                        &state.slots[state.mc_asset_idx].symbol,
                        series,
                        h,
                        state.mc_n_sims,
                        state.mc_seed,
                    );
                }
            });

            if let Some(res) = &state.mc_results {
                ui.add_space(4.0);
                apply_lock(
                    Plot::new("compare_mc")
                        .height(280.0)
                        .legend(Legend::default().background_alpha(0.35))
                        .show_axes([true, true])
                        .y_axis_formatter(|m, _| format!("{:.1}×", m.value))
                        .x_axis_formatter(|m, _| {
                            // Convert trading-day index to years.
                            format!("{:.1}y", m.value / TRADING_DAYS_PER_YEAR)
                        }),
                    state.interactive,
                )
                    .show(ui, |plot_ui| {
                        // 5–95% fan: polygon walking up p5 then back down p95.
                        let mut band: Vec<[f64; 2]> = Vec::with_capacity(res.p5.len() * 2);
                        for (i, &v) in res.p5.iter().enumerate() {
                            band.push([i as f64, v]);
                        }
                        for (i, &v) in res.p95.iter().enumerate().rev() {
                            band.push([i as f64, v]);
                        }
                        plot_ui.polygon(
                            Polygon::new(PlotPoints::new(band))
                                .fill_color(Color32::from_rgba_unmultiplied(0, 191, 255, 32))
                                .stroke(Stroke::NONE)
                                .name("5–95% band"),
                        );
                        let line_for = |name: &str, vs: &[f64], color: Color32, w: f32| {
                            let pts: PlotPoints = vs
                                .iter()
                                .enumerate()
                                .map(|(i, v)| [i as f64, *v])
                                .collect();
                            Line::new(pts)
                                .color(color)
                                .stroke(Stroke::new(w, color))
                                .name(name)
                        };
                        plot_ui.line(line_for("5th pct", &res.p5, theme::GRAY2, 1.0));
                        plot_ui.line(line_for("Median", &res.median, theme::YELLOW, 2.0));
                        plot_ui.line(line_for("95th pct", &res.p95, theme::GRAY2, 1.0));
                    });
                ui.add_space(6.0);
                render_mc_summary(res, ui);
            } else {
                ui.add_space(4.0);
                ui.label(
                    RichText::new(format!(
                        "Click Run to simulate {} future paths from historical daily log-return mean & stdev.",
                        state.mc_n_sims
                    ))
                    .color(theme::GRAY2)
                    .italics(),
                );
            }
        });
}

fn render_mc_summary(res: &MonteCarloResult, ui: &mut egui::Ui) {
    ui.horizontal_wrapped(|ui| {
        let kv = |ui: &mut egui::Ui, k: &str, v: String, color: Color32| {
            ui.label(RichText::new(k).color(theme::ORANGE).strong());
            ui.label(RichText::new(v).color(color).strong());
            ui.add_space(10.0);
        };
        kv(
            ui,
            "P(above start):",
            format!("{:.1}%", res.prob_above_start * 100.0),
            if res.prob_above_start >= 0.5 {
                theme::GREEN
            } else {
                theme::RED
            },
        );
        kv(
            ui,
            "P(>50% drawdown):",
            format!("{:.1}%", res.prob_50_dd * 100.0),
            if res.prob_50_dd <= 0.1 {
                theme::GREEN
            } else if res.prob_50_dd <= 0.3 {
                theme::YELLOW
            } else {
                theme::RED
            },
        );
        kv(ui, "Final 5th:", format!("{:.2}×", res.final_p5), theme::RED);
        kv(
            ui,
            "Final 50th:",
            format!("{:.2}×", res.final_p50),
            theme::YELLOW,
        );
        kv(ui, "Final 95th:", format!("{:.2}×", res.final_p95), theme::GREEN);
    });
    ui.label(
        RichText::new(format!(
            "{}  ·  horizon = {}y  ·  {} sims  ·  μ_daily = {:+.4}  ·  σ_daily = {:.4}",
            res.symbol, res.horizon_years, res.n_sims, res.mu_daily, res.sigma_daily
        ))
        .color(theme::GRAY)
        .size(11.0)
        .italics(),
    );
}

// ── Tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn approx(a: f64, b: f64, tol: f64) -> bool {
        (a - b).abs() < tol
    }

    #[test]
    fn cagr_doubles_in_one_year_is_100pct() {
        // 252 + 1 closes, last is double the first → 100% CAGR (1 year).
        let mut closes = vec![100.0];
        for i in 1..=252 {
            closes.push(100.0 + i as f64 * (100.0 / 252.0));
        }
        let c = cagr(&closes);
        assert!(approx(c, 1.0, 0.02), "cagr={}", c);
    }

    #[test]
    fn max_drawdown_finds_worst_peak_to_trough() {
        let closes = vec![100.0, 110.0, 120.0, 60.0, 90.0, 100.0];
        let dd = max_drawdown(&closes);
        // Peak 120, trough 60 → -50%
        assert!(approx(dd, -0.5, 1e-9), "dd={}", dd);
    }

    #[test]
    fn pearson_self_is_one() {
        let a = vec![0.01, -0.02, 0.005, 0.03, -0.01];
        assert!(approx(pearson(&a, &a), 1.0, 1e-9));
    }

    #[test]
    fn pearson_negation_is_minus_one() {
        let a = vec![0.01, -0.02, 0.005, 0.03, -0.01];
        let b: Vec<f64> = a.iter().map(|x| -x).collect();
        assert!(approx(pearson(&a, &b), -1.0, 1e-9));
    }

    #[test]
    fn drawdown_series_is_zero_at_peak_and_negative_below() {
        let closes = vec![100.0, 110.0, 105.0, 120.0, 90.0];
        let dd = drawdown_series(&closes);
        assert!(approx(dd[0], 0.0, 1e-9));
        assert!(approx(dd[1], 0.0, 1e-9));
        assert!(dd[2] < 0.0);
        assert!(approx(dd[3], 0.0, 1e-9));
        assert!(approx(dd[4], -0.25, 1e-9));
    }

    #[test]
    fn calmar_is_cagr_over_max_dd_abs() {
        let closes = vec![100.0, 110.0, 120.0, 60.0, 90.0, 100.0, 150.0];
        let cgr = cagr(&closes);
        let dd = max_drawdown(&closes).abs();
        let c = calmar(&closes);
        if dd > 0.0 {
            assert!(approx(c, cgr / dd, 1e-9));
        }
    }

    #[test]
    fn vol_zero_when_all_returns_equal() {
        // Constant log-return geometric series → zero stdev of log returns
        let mut closes = vec![100.0];
        for _ in 1..50 {
            let last = *closes.last().unwrap();
            closes.push(last * 1.01);
        }
        let rets = daily_log_returns(&closes);
        let v = annualized_vol(&rets);
        assert!(v < 1e-9, "vol={}", v);
    }

    #[test]
    fn xorshift_produces_normal_with_unit_variance() {
        let mut rng = Xorshift64::new(42);
        let n = 20_000;
        let mut sum = 0.0;
        let mut sq = 0.0;
        for _ in 0..n {
            let x = rng.next_normal();
            sum += x;
            sq += x * x;
        }
        let mean = sum / n as f64;
        let var = sq / n as f64 - mean * mean;
        assert!(mean.abs() < 0.05, "mean={}", mean);
        assert!((var - 1.0).abs() < 0.05, "var={}", var);
    }

    #[test]
    fn run_monte_carlo_returns_three_ordered_percentile_paths() {
        // Synthetic closes: smooth upward drift so MC has a well-defined μ, σ.
        let mut closes = vec![100.0];
        let mut rng = Xorshift64::new(7);
        for _ in 1..400 {
            let last = *closes.last().unwrap();
            let r = 0.0005 + 0.01 * rng.next_normal();
            closes.push(last * r.exp());
        }
        let res = run_monte_carlo("X", &closes, 1, 500, 999).unwrap();
        for i in 0..res.median.len() {
            assert!(
                res.p5[i] <= res.median[i] + 1e-9,
                "p5 > median at i={}: {} vs {}",
                i,
                res.p5[i],
                res.median[i]
            );
            assert!(
                res.median[i] <= res.p95[i] + 1e-9,
                "median > p95 at i={}: {} vs {}",
                i,
                res.median[i],
                res.p95[i]
            );
        }
        assert_eq!(res.median[0], 1.0);
        assert_eq!(res.p5[0], 1.0);
        assert_eq!(res.p95[0], 1.0);
        assert!(res.prob_above_start >= 0.0 && res.prob_above_start <= 1.0);
        assert!(res.prob_50_dd >= 0.0 && res.prob_50_dd <= 1.0);
    }

    #[test]
    fn best_worst_picks_extremes_with_correct_direction() {
        let values = vec![1.0, 3.0, 2.0, 0.5];
        let (b, w) = best_worst(&values, Direction::HigherBetter);
        assert_eq!(b, Some(1));
        assert_eq!(w, Some(3));
        let (b, w) = best_worst(&values, Direction::LowerBetter);
        assert_eq!(b, Some(3));
        assert_eq!(w, Some(1));
    }
}
