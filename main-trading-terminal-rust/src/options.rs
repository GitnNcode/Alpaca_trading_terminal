// Options desk — top-level Tab::Options with its own sub-tab strip:
//   1) Chain     — pick an underlying + expiration, see a calls/puts strike
//                  grid (bid/ask/last/vol/OI) and place single-leg orders.
//   2) Positions — option holdings only (filtered to asset_class us_option),
//                  live P&L recomputed from the options tick stream.
//   3) Orders    — open option orders only, with confirm-before-cancel.
//
// Data model (see docs/adr/0001): a REST snapshot (/v1beta1/options/snapshots,
// indicative feed) seeds bid/ask + %chg; the chain SKELETON + open interest
// come from /v2/options/contracts; the options WebSocket overlays live bid/ask
// onto the displayed expiration. Greeks/IV are NOT shown — the indicative feed
// doesn't carry them (OPRA agreement unsigned).
//
// Order placement reuses the equity OrderRequest + a two-phase confirm modal,
// exactly like the Terminal tab: the OCC symbol just flows through /v2/orders.

use std::collections::HashMap;
use std::sync::mpsc::Sender;
use std::sync::Arc;
use std::time::Instant;

use chrono::{NaiveDate, Utc};
use egui::{Color32, Key, RichText};

use crate::api::{AlpacaClient, OptionContract, OptionSnapshot, Order, OrderRequest, Position};
use crate::stocks::AssetCache;
use crate::stream::{LastTick, TickCache};
use crate::terminal::TradeSide;
use crate::theme;
use crate::workers::{self, Msg};

/// Standard equity-option contract multiplier (shares per contract). Alpaca's
/// position payload doesn't echo it, so we apply the convention for P&L /
/// market-value math on the Positions sub-tab.
const OPTION_MULTIPLIER: f64 = 100.0;

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum SubTab {
    Chain,
    Positions,
    Orders,
}

/// Options orders are single-leg Market or Limit only (the agreed v1 scope).
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum OptOrderKind {
    Market,
    Limit,
}

impl OptOrderKind {
    fn api_str(self) -> &'static str {
        match self {
            OptOrderKind::Market => "market",
            OptOrderKind::Limit => "limit",
        }
    }
    fn label(self) -> &'static str {
        match self {
            OptOrderKind::Market => "MARKET",
            OptOrderKind::Limit => "LIMIT",
        }
    }
    fn needs_limit(self) -> bool {
        matches!(self, OptOrderKind::Limit)
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum CallPut {
    Call,
    Put,
}

// ============================================================================
//  OCC symbol parse / format (pure, unit-tested)
// ============================================================================

#[derive(Debug, Clone, PartialEq)]
pub struct ParsedOcc {
    pub root: String,
    pub expiration: NaiveDate,
    pub kind: CallPut,
    pub strike: f64,
}

/// Parse an OCC option symbol like `AAPL260619C00150000`. The trailing 15
/// chars are fixed: `YYMMDD` + `C`/`P` + 8-digit strike×1000; everything before
/// is the (variable-length) root. Anchored from the right so multi-char roots
/// work without OCC's space-padding.
pub fn parse_occ(sym: &str) -> Option<ParsedOcc> {
    let s = sym.trim();
    // root (>=1) + 6 date + 1 cp + 8 strike = at least 16 chars.
    if s.len() < 16 || !s.is_ascii() {
        return None;
    }
    let n = s.len();
    let strike_str = &s[n - 8..];
    let cp = &s[n - 9..n - 8];
    let date_str = &s[n - 15..n - 9];
    let root = &s[..n - 15];
    if root.is_empty() {
        return None;
    }
    let strike_raw: u64 = strike_str.parse().ok()?;
    let kind = match cp {
        "C" => CallPut::Call,
        "P" => CallPut::Put,
        _ => return None,
    };
    let expiration = NaiveDate::parse_from_str(date_str, "%y%m%d").ok()?;
    Some(ParsedOcc {
        root: root.to_string(),
        expiration,
        kind,
        strike: strike_raw as f64 / 1000.0,
    })
}

/// Build an OCC symbol from its parts. Inverse of `parse_occ` — exercised by
/// the round-trip test and kept as a public helper for symbol construction.
#[allow(dead_code)]
pub fn format_occ(root: &str, expiration: NaiveDate, kind: CallPut, strike: f64) -> String {
    let cp = match kind {
        CallPut::Call => 'C',
        CallPut::Put => 'P',
    };
    let strike_int = (strike * 1000.0).round() as u64;
    format!(
        "{}{}{}{:08}",
        root.to_ascii_uppercase(),
        expiration.format("%y%m%d"),
        cp,
        strike_int
    )
}

/// Human-friendly contract label, e.g. `AAPL 06/19/26 150C`. Falls back to the
/// raw OCC symbol if it doesn't parse.
pub fn human_occ(occ: &str) -> String {
    match parse_occ(occ) {
        Some(p) => {
            let cp = match p.kind {
                CallPut::Call => "C",
                CallPut::Put => "P",
            };
            format!(
                "{} {} {}{}",
                p.root,
                p.expiration.format("%m/%d/%y"),
                trim_strike(p.strike),
                cp
            )
        }
        None => occ.to_string(),
    }
}

fn trim_strike(strike: f64) -> String {
    // 150.0 -> "150", 152.5 -> "152.5".
    if (strike.fract()).abs() < f64::EPSILON {
        format!("{}", strike as i64)
    } else {
        let s = format!("{:.2}", strike);
        s.trim_end_matches('0').trim_end_matches('.').to_string()
    }
}

// ============================================================================
//  Chain grouping (pure, unit-tested)
// ============================================================================

/// Sorted, de-duplicated list of expiration dates present in a contract set.
pub fn expirations(contracts: &[OptionContract]) -> Vec<String> {
    let mut v: Vec<String> = contracts
        .iter()
        .map(|c| c.expiration_date.clone())
        .filter(|e| !e.is_empty())
        .collect();
    v.sort();
    v.dedup();
    v
}

/// One strike row of the Chain: the call and put (if listed) at that strike.
#[derive(Debug, Clone)]
pub struct ChainRow {
    pub strike: f64,
    pub call: Option<OptionContract>,
    pub put: Option<OptionContract>,
}

/// Group an underlying's contracts into strike rows for ONE expiration,
/// ascending by strike. Calls and puts at the same strike share a row.
pub fn build_chain(contracts: &[OptionContract], expiration: &str) -> Vec<ChainRow> {
    use std::collections::BTreeMap;
    // Key by strike×1000 (integer) so f64 ordering / equality is exact.
    let mut map: BTreeMap<u64, (Option<OptionContract>, Option<OptionContract>)> = BTreeMap::new();
    for c in contracts {
        if c.expiration_date != expiration {
            continue;
        }
        let strike: f64 = c.strike_price.parse().unwrap_or(0.0);
        let key = (strike * 1000.0).round() as u64;
        let entry = map.entry(key).or_default();
        match c.kind.as_str() {
            "call" => entry.0 = Some(c.clone()),
            "put" => entry.1 = Some(c.clone()),
            _ => {}
        }
    }
    map.into_iter()
        .map(|(k, (call, put))| ChainRow {
            strike: k as f64 / 1000.0,
            call,
            put,
        })
        .collect()
}

// ============================================================================
//  Order building (pure, unit-tested)
// ============================================================================

/// Validate a single-leg option order and produce an `OrderRequest` + a
/// human summary. `qty` must be a whole number of contracts; a Limit order
/// needs a positive limit price (None ⇒ rejected). TIF is always `day`.
pub fn build_option_order(
    occ: &str,
    side: TradeSide,
    qty: &str,
    kind: OptOrderKind,
    limit_price: Option<f64>,
) -> Option<(OrderRequest, String)> {
    let occ = occ.trim().to_ascii_uppercase();
    let qty = qty.trim();
    if occ.is_empty() || qty.is_empty() {
        return None;
    }
    let q: f64 = qty.parse().ok()?;
    if q <= 0.0 || q.fract().abs() > f64::EPSILON {
        return None; // whole contracts only
    }
    let side_s = match side {
        TradeSide::Buy => "buy",
        TradeSide::Sell => "sell",
    }
    .to_string();
    let order_type = kind.api_str().to_string();

    let limit = if kind.needs_limit() {
        let p = limit_price?;
        if p <= 0.0 {
            return None;
        }
        format!("{:.2}", p)
    } else {
        String::new()
    };

    let mut summary = format!("{} {} {} @ {}", side_s.to_uppercase(), qty, occ, kind.label());
    if !limit.is_empty() {
        summary.push_str(&format!(" {}", limit));
    }
    summary.push_str(" (DAY)");

    let req = OrderRequest {
        symbol: occ,
        qty: qty.to_string(),
        notional: None,
        side: side_s,
        order_type,
        time_in_force: "day".to_string(),
        limit_price: limit,
        stop_price: None,
        trail_percent: None,
        order_class: None,
        take_profit: None,
        stop_loss: None,
    };
    Some((req, summary))
}

// ============================================================================
//  State
// ============================================================================

pub struct OptOrderForm {
    pub qty_input: String,
    pub kind: OptOrderKind,
    pub result: String,
    pub result_color: Color32,
    pub busy: bool,
}

impl Default for OptOrderForm {
    fn default() -> Self {
        OptOrderForm {
            qty_input: "1".to_string(),
            // Limit is the responsible default for options — market orders can
            // fill at terrible prices across the typically-wide spread.
            kind: OptOrderKind::Limit,
            result: String::new(),
            result_color: theme::GRAY2,
            busy: false,
        }
    }
}

pub struct OptPlaceConfirm {
    pub req: OrderRequest,
    pub summary: String,
}

pub struct OptCancelConfirm {
    pub id: String,
    pub symbol: String,
}

pub struct OptionsState {
    pub sub_tab: SubTab,

    // Chain selection + data.
    pub underlying_input: String,
    pub underlying: String,
    pub autocomplete: Vec<(String, String)>,
    pub contracts: Vec<OptionContract>,
    pub snapshots: HashMap<String, OptionSnapshot>,
    pub expirations: Vec<String>,
    pub expiration_idx: usize,
    pub loading: bool,
    pub err: String,
    /// Stale-response guard for chain loads (contracts + snapshots carry it
    /// back). Bumped on each `kick_chain_load`.
    pub gen: u64,

    // Positions / Orders (filtered to us_option).
    pub positions: Vec<Position>,
    pub positions_err: String,
    pub positions_loading: bool,
    pub open_orders: Vec<Order>,
    pub open_orders_err: String,
    pub orders_loading: bool,
    pub cancelling: std::collections::HashSet<String>,
    pub last_refresh: Option<Instant>,

    // Order form + modals.
    pub form: OptOrderForm,
    pub place_confirm: Option<OptPlaceConfirm>,
    pub cancel_confirm: Option<OptCancelConfirm>,
}

impl Default for OptionsState {
    fn default() -> Self {
        OptionsState {
            sub_tab: SubTab::Chain,
            underlying_input: String::new(),
            underlying: String::new(),
            autocomplete: Vec::new(),
            contracts: Vec::new(),
            snapshots: HashMap::new(),
            expirations: Vec::new(),
            expiration_idx: 0,
            loading: false,
            err: String::new(),
            gen: 0,
            positions: Vec::new(),
            positions_err: String::new(),
            positions_loading: false,
            open_orders: Vec::new(),
            open_orders_err: String::new(),
            orders_loading: false,
            cancelling: std::collections::HashSet::new(),
            last_refresh: None,
            form: OptOrderForm::default(),
            place_confirm: None,
            cancel_confirm: None,
        }
    }
}

impl OptionsState {
    pub fn new() -> Self {
        Self::default()
    }

    /// Load (or reload) the chain for an underlying: contracts + snapshots in
    /// parallel. Bumps `gen` so any in-flight response for a prior underlying
    /// is discarded on arrival.
    pub fn kick_chain_load(
        &mut self,
        client: Arc<AlpacaClient>,
        tx: Sender<Msg>,
        ctx: &egui::Context,
        underlying: String,
    ) {
        let u = underlying.trim().to_ascii_uppercase();
        if u.is_empty() {
            return;
        }
        self.gen = self.gen.wrapping_add(1);
        self.underlying = u.clone();
        self.underlying_input = u.clone();
        self.loading = true;
        self.err.clear();
        self.contracts.clear();
        self.snapshots.clear();
        self.expirations.clear();
        self.expiration_idx = 0;
        self.form.result.clear();
        workers::spawn_option_contracts(client.clone(), tx.clone(), ctx.clone(), u.clone(), self.gen);
        workers::spawn_option_snapshots(client, tx, ctx.clone(), u, self.gen);
    }

    /// Apply a freshly-loaded contract list: store it, recompute the
    /// expiration list, and default the selection to the nearest non-expired
    /// expiration.
    pub fn on_contracts_loaded(&mut self, contracts: Vec<OptionContract>) {
        self.contracts = contracts;
        self.expirations = expirations(&self.contracts);
        let today = Utc::now().date_naive().format("%Y-%m-%d").to_string();
        self.expiration_idx = self
            .expirations
            .iter()
            .position(|e| e.as_str() >= today.as_str())
            .unwrap_or(0);
        self.loading = false;
    }

    /// Refresh option positions + open orders (the Positions / Orders
    /// sub-tabs). Mirrors the Terminal tab's 10s auto-refresh, but scoped to
    /// options.
    pub fn refresh_account_data(
        &mut self,
        client: Arc<AlpacaClient>,
        tx: Sender<Msg>,
        ctx: &egui::Context,
    ) {
        self.positions_loading = true;
        self.orders_loading = true;
        self.last_refresh = Some(Instant::now());
        workers::spawn_option_positions(client.clone(), tx.clone(), ctx.clone());
        workers::spawn_option_open_orders(client, tx, ctx.clone());
    }

    /// OCC symbols of the currently-displayed expiration — the live-WS
    /// subscription target (unioned with held positions by the caller).
    pub fn displayed_symbols(&self) -> Vec<String> {
        match self.expirations.get(self.expiration_idx) {
            Some(exp) => self
                .contracts
                .iter()
                .filter(|c| &c.expiration_date == exp)
                .map(|c| c.symbol.clone())
                .collect(),
            None => Vec::new(),
        }
    }
}

// ============================================================================
//  Rendering
// ============================================================================

pub fn render(
    state: &mut OptionsState,
    client: Arc<AlpacaClient>,
    tx: Sender<Msg>,
    assets: &AssetCache,
    tick_cache: &TickCache,
    ui: &mut egui::Ui,
) {
    sub_tab_strip(state, ui);
    ui.separator();

    // Sub-tab number hotkeys 1..3 + R refresh — only when no field is focused.
    if !ui.ctx().memory(|m| m.focused().is_some()) {
        let pressed = |k: Key| ui.ctx().input(|i| i.key_pressed(k));
        if pressed(Key::Num1) {
            state.sub_tab = SubTab::Chain;
        }
        if pressed(Key::Num2) {
            state.sub_tab = SubTab::Positions;
        }
        if pressed(Key::Num3) {
            state.sub_tab = SubTab::Orders;
        }
        if pressed(Key::R) || pressed(Key::F5) {
            if !state.underlying.is_empty() {
                let ctx = ui.ctx().clone();
                state.kick_chain_load(client.clone(), tx.clone(), &ctx, state.underlying.clone());
            }
            state.refresh_account_data(client.clone(), tx.clone(), ui.ctx());
        }
    }

    underlying_bar(state, client.clone(), tx.clone(), assets, ui);
    ui.separator();

    match state.sub_tab {
        SubTab::Chain => chain_view(state, tick_cache, ui),
        SubTab::Positions => positions_view(state, tick_cache, ui),
        SubTab::Orders => orders_view(state, ui),
    }

    if state.place_confirm.is_some() {
        place_confirm_modal(state, client.clone(), tx.clone(), ui.ctx());
    }
    if state.cancel_confirm.is_some() {
        cancel_confirm_modal(state, client, tx, ui.ctx());
    }
}

fn sub_tab_strip(state: &mut OptionsState, ui: &mut egui::Ui) {
    ui.horizontal(|ui| {
        ui.label(RichText::new(" OPTIONS ").color(theme::ORANGE).strong());
        ui.separator();
        for (sub, label, hk) in [
            (SubTab::Chain, "Chain", "1"),
            (SubTab::Positions, "Positions", "2"),
            (SubTab::Orders, "Orders", "3"),
        ] {
            let active = state.sub_tab == sub;
            let text = format!(" [{hk}] {label} ");
            let btn = if active {
                egui::Button::new(RichText::new(text).color(theme::BLACK).strong()).fill(theme::CYAN)
            } else {
                egui::Button::new(RichText::new(text).color(theme::GRAY2)).fill(theme::DARK)
            };
            if ui.add(btn).clicked() {
                state.sub_tab = sub;
            }
        }
    });
}

fn underlying_bar(
    state: &mut OptionsState,
    client: Arc<AlpacaClient>,
    tx: Sender<Msg>,
    assets: &AssetCache,
    ui: &mut egui::Ui,
) {
    let mut commit: Option<String> = None;
    ui.horizontal_wrapped(|ui| {
        ui.label(RichText::new(" UNDERLYING ").color(theme::ORANGE).strong());
        let resp = ui.add(
            egui::TextEdit::singleline(&mut state.underlying_input).desired_width(110.0),
        );
        if resp.changed() {
            state.underlying_input = state.underlying_input.to_uppercase();
            state.autocomplete = if state.underlying_input.is_empty() {
                Vec::new()
            } else {
                assets.filter(&state.underlying_input, 6)
            };
        }
        if resp.has_focus() && ui.input(|i| i.key_pressed(Key::Escape)) {
            state.autocomplete.clear();
        }
        let submitted = resp.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter));
        if ui.button(" Load ").clicked() || submitted {
            commit = Some(state.underlying_input.clone());
            state.autocomplete.clear();
        }

        if !state.expirations.is_empty() {
            ui.separator();
            ui.label(RichText::new("EXP").color(theme::ORANGE).strong());
            let exps = state.expirations.clone();
            let cur = exps
                .get(state.expiration_idx)
                .cloned()
                .unwrap_or_default();
            egui::ComboBox::from_id_salt("opt_exp")
                .selected_text(RichText::new(cur).color(theme::WHITE))
                .show_ui(ui, |ui| {
                    for (i, e) in exps.iter().enumerate() {
                        ui.selectable_value(&mut state.expiration_idx, i, e);
                    }
                });
        }

        if state.loading {
            ui.label(RichText::new(" loading…").color(theme::GRAY2));
        }
        if !state.err.is_empty() {
            ui.label(RichText::new(format!(" ERROR: {}", state.err)).color(theme::RED));
        }
    });

    if !state.autocomplete.is_empty() {
        let sugg = state.autocomplete.clone();
        ui.horizontal_wrapped(|ui| {
            for (sym, _name) in sugg.iter().take(6) {
                if ui
                    .add(egui::Button::new(RichText::new(sym).color(theme::CYAN)).fill(theme::DARK))
                    .clicked()
                {
                    state.underlying_input = sym.clone();
                    state.autocomplete.clear();
                    commit = Some(sym.clone());
                }
            }
        });
    }

    if let Some(sym) = commit {
        let ctx = ui.ctx().clone();
        state.kick_chain_load(client, tx, &ctx, sym);
    }
}

// ----------------------------------------------------------------------------
//  CHAIN sub-tab
// ----------------------------------------------------------------------------

/// Resolved per-contract quote: snapshot values overlaid with live ticks.
struct Quote {
    bid: Option<f64>,
    ask: Option<f64>,
    last: Option<f64>,
    vol: Option<f64>,
    oi: Option<f64>,
    prev_close: Option<f64>,
    /// True when any of bid/ask/last came from the live stream (vs snapshot).
    live: bool,
}

fn quote_for(c: &OptionContract, snap: Option<&OptionSnapshot>, tick: Option<&LastTick>) -> Quote {
    let mut bid = snap
        .and_then(|s| s.latest_quote.as_ref())
        .map(|q| q.bid)
        .filter(|v| *v > 0.0);
    let mut ask = snap
        .and_then(|s| s.latest_quote.as_ref())
        .map(|q| q.ask)
        .filter(|v| *v > 0.0);
    let mut last = snap
        .and_then(|s| s.latest_trade.as_ref())
        .map(|t| t.price)
        .filter(|v| *v > 0.0);
    let mut live = false;
    if let Some(t) = tick {
        if let Some(b) = t.bid {
            if b > 0.0 {
                bid = Some(b);
                live = true;
            }
        }
        if let Some(a) = t.ask {
            if a > 0.0 {
                ask = Some(a);
                live = true;
            }
        }
        if let Some(p) = t.last_price {
            if p > 0.0 {
                last = Some(p);
                live = true;
            }
        }
    }
    let vol = snap.and_then(|s| s.daily_bar.as_ref()).map(|b| b.volume);
    let oi = c.open_interest.as_ref().and_then(|s| s.parse::<f64>().ok());
    let prev_close = c
        .close_price
        .as_ref()
        .and_then(|s| s.parse::<f64>().ok())
        .or_else(|| snap.and_then(|s| s.prev_daily_bar.as_ref()).map(|b| b.close));
    Quote {
        bid,
        ask,
        last,
        vol,
        oi,
        prev_close,
        live,
    }
}

fn chain_view(state: &mut OptionsState, tick_cache: &TickCache, ui: &mut egui::Ui) {
    if state.underlying.is_empty() {
        ui.label(
            RichText::new("  Type an underlying above and press Load to see its option chain.")
                .color(theme::GRAY2),
        );
        return;
    }
    if state.contracts.is_empty() {
        if state.loading {
            ui.label(RichText::new("  loading chain…").color(theme::GRAY2));
        } else {
            ui.label(RichText::new("  No contracts for this underlying.").color(theme::GRAY2));
        }
        return;
    }
    let Some(exp) = state.expirations.get(state.expiration_idx).cloned() else {
        ui.label(RichText::new("  No expirations.").color(theme::GRAY2));
        return;
    };
    let rows = build_chain(&state.contracts, &exp);

    order_settings_bar(state, ui);
    ui.add_space(2.0);

    if rows.is_empty() {
        ui.label(RichText::new("  No strikes for this expiration.").color(theme::GRAY2));
        return;
    }

    let kind = state.form.kind;
    let snapshots = &state.snapshots;
    let live = tick_cache.read().ok();
    let mut pending: Option<(TradeSide, String, Option<f64>)> = None;

    egui::ScrollArea::both().show(ui, |ui| {
        egui::Grid::new("opt_chain")
            .num_columns(13)
            .striped(true)
            .spacing([8.0, 3.0])
            .show(ui, |ui| {
                let head = |ui: &mut egui::Ui, s: &str| {
                    ui.label(RichText::new(s).color(theme::ORANGE).strong());
                };
                // CALLS | STRIKE | PUTS
                head(ui, "BID");
                head(ui, "ASK");
                head(ui, "LAST");
                head(ui, "VOL");
                head(ui, "OI");
                head(ui, "CALL");
                head(ui, "STRIKE");
                head(ui, "PUT");
                head(ui, "BID");
                head(ui, "ASK");
                head(ui, "LAST");
                head(ui, "VOL");
                head(ui, "OI");
                ui.end_row();

                for row in &rows {
                    render_side(ui, row.call.as_ref(), snapshots, live.as_deref(), kind, &mut pending, true);
                    ui.label(RichText::new(format!("{:>8.2}", row.strike)).color(theme::YELLOW).strong());
                    render_side(ui, row.put.as_ref(), snapshots, live.as_deref(), kind, &mut pending, false);
                    ui.end_row();
                }
            });
    });
    drop(live);

    if let Some((side, occ, px)) = pending {
        match build_option_order(&occ, side, &state.form.qty_input, kind, px) {
            Some((req, summary)) => {
                state.form.result.clear();
                state.place_confirm = Some(OptPlaceConfirm { req, summary });
            }
            None => {
                state.form.result =
                    "Invalid order — qty must be a whole number of contracts; Limit needs a live quote.".to_string();
                state.form.result_color = theme::RED;
            }
        }
    }
}

fn order_settings_bar(state: &mut OptionsState, ui: &mut egui::Ui) {
    ui.horizontal_wrapped(|ui| {
        ui.label(RichText::new(" ORDER ").color(theme::ORANGE).strong());
        ui.label(RichText::new("QTY").color(theme::ORANGE));
        ui.add(egui::TextEdit::singleline(&mut state.form.qty_input).desired_width(48.0));
        ui.label(RichText::new("TYPE").color(theme::ORANGE));
        egui::ComboBox::from_id_salt("opt_kind")
            .selected_text(RichText::new(state.form.kind.label()).color(theme::WHITE))
            .show_ui(ui, |ui| {
                for k in [OptOrderKind::Limit, OptOrderKind::Market] {
                    ui.selectable_value(&mut state.form.kind, k, k.label());
                }
            });
        ui.label(
            RichText::new("· click B (buy) / S (sell) on a strike to place")
                .color(theme::GRAY2),
        );
        if !state.form.result.is_empty() {
            ui.label(RichText::new(&state.form.result).color(state.form.result_color));
        }
    });
}

#[allow(clippy::too_many_arguments)]
fn render_side(
    ui: &mut egui::Ui,
    contract: Option<&OptionContract>,
    snaps: &HashMap<String, OptionSnapshot>,
    cache: Option<&HashMap<String, LastTick>>,
    kind: OptOrderKind,
    pending: &mut Option<(TradeSide, String, Option<f64>)>,
    is_call: bool,
) {
    let Some(c) = contract else {
        // Keep the grid aligned when one side has no contract at this strike.
        for _ in 0..6 {
            ui.label("");
        }
        return;
    };
    let q = quote_for(c, snaps.get(&c.symbol), cache.and_then(|m| m.get(&c.symbol)));
    if is_call {
        side_data_cells(ui, &q);
        side_trade_cell(ui, c, &q, kind, pending);
    } else {
        side_trade_cell(ui, c, &q, kind, pending);
        side_data_cells(ui, &q);
    }
}

fn side_data_cells(ui: &mut egui::Ui, q: &Quote) {
    num_cell(ui, q.bid, if q.live { theme::CYAN } else { theme::WHITE });
    num_cell(ui, q.ask, if q.live { theme::ORANGE } else { theme::WHITE });
    // LAST tinted by direction vs prev close.
    let last_color = match (q.last, q.prev_close) {
        (Some(l), Some(p)) if p > 0.0 => {
            if l >= p {
                theme::GREEN
            } else {
                theme::RED
            }
        }
        _ => theme::WHITE,
    };
    num_cell(ui, q.last, last_color);
    int_cell(ui, q.vol);
    int_cell(ui, q.oi);
}

fn side_trade_cell(
    ui: &mut egui::Ui,
    c: &OptionContract,
    q: &Quote,
    kind: OptOrderKind,
    pending: &mut Option<(TradeSide, String, Option<f64>)>,
) {
    ui.horizontal(|ui| {
        if ui
            .add(egui::Button::new(RichText::new(" B ").color(theme::BLACK).strong()).fill(theme::GREEN))
            .on_hover_text("Buy to open")
            .clicked()
        {
            let px = if kind.needs_limit() { q.ask } else { None };
            *pending = Some((TradeSide::Buy, c.symbol.clone(), px));
        }
        if ui
            .add(egui::Button::new(RichText::new(" S ").color(theme::WHITE)).fill(theme::RED))
            .on_hover_text("Sell")
            .clicked()
        {
            let px = if kind.needs_limit() { q.bid } else { None };
            *pending = Some((TradeSide::Sell, c.symbol.clone(), px));
        }
    });
}

fn num_cell(ui: &mut egui::Ui, v: Option<f64>, color: Color32) {
    match v {
        Some(x) => ui.label(RichText::new(format!("{:>7.2}", x)).color(color)),
        None => ui.label(RichText::new("      —").color(theme::GRAY)),
    };
}

fn int_cell(ui: &mut egui::Ui, v: Option<f64>) {
    match v {
        Some(x) => ui.label(RichText::new(format!("{:>7}", x as i64)).color(theme::WHITE)),
        None => ui.label(RichText::new("      —").color(theme::GRAY)),
    };
}

// ----------------------------------------------------------------------------
//  POSITIONS sub-tab (options only)
// ----------------------------------------------------------------------------

fn positions_view(state: &mut OptionsState, tick_cache: &TickCache, ui: &mut egui::Ui) {
    ui.horizontal(|ui| {
        ui.label(RichText::new(" OPTION POSITIONS ").color(theme::ORANGE).strong());
        if state.positions_loading {
            ui.label(RichText::new("  loading…").color(theme::GRAY2));
        }
        if !state.positions_err.is_empty() {
            ui.label(RichText::new(format!("  ERROR: {}", state.positions_err)).color(theme::RED));
        }
    });
    ui.add_space(2.0);

    if state.positions.is_empty() && !state.positions_loading {
        ui.label(RichText::new("  NO OPTION POSITIONS — PRESS R TO REFRESH").color(theme::GRAY2));
        return;
    }

    let positions = state.positions.clone();
    let live = tick_cache.read().ok();
    egui::ScrollArea::both().show(ui, |ui| {
        egui::Grid::new("opt_pos")
            .num_columns(8)
            .striped(true)
            .spacing([10.0, 4.0])
            .show(ui, |ui| {
                let head = |ui: &mut egui::Ui, s: &str| {
                    ui.label(RichText::new(s).color(theme::ORANGE).strong());
                };
                head(ui, "CONTRACT");
                head(ui, "SIDE");
                head(ui, "QTY");
                head(ui, "AVG");
                head(ui, "CURRENT");
                head(ui, "MKT VALUE");
                head(ui, "P&L");
                head(ui, "P&L %");
                ui.end_row();

                for p in &positions {
                    let qty = p.qty.parse::<f64>().unwrap_or(0.0);
                    let avg = p.avg_entry_price.parse::<f64>().unwrap_or(0.0);
                    let live_last = live
                        .as_ref()
                        .and_then(|c| c.get(&p.symbol))
                        .and_then(|t| t.last_price);
                    let cur =
                        live_last.unwrap_or_else(|| p.current_price.parse::<f64>().unwrap_or(0.0));
                    let side_sign: f64 = if p.side == "short" { -1.0 } else { 1.0 };
                    let mkt_value = cur * qty * OPTION_MULTIPLIER;
                    let pl = (cur - avg) * qty * OPTION_MULTIPLIER * side_sign;
                    let plpc = if avg.abs() > 0.0 {
                        (cur - avg) / avg * side_sign
                    } else {
                        0.0
                    };
                    let pl_color = if pl >= 0.0 { theme::GREEN } else { theme::RED };
                    let side_color = match p.side.as_str() {
                        "long" => theme::CYAN,
                        "short" => theme::RED,
                        _ => theme::WHITE,
                    };
                    let cur_color = if live_last.is_some() { theme::ORANGE } else { theme::WHITE };
                    ui.label(RichText::new(human_occ(&p.symbol)).color(theme::WHITE).strong());
                    ui.label(RichText::new(p.side.to_uppercase()).color(side_color));
                    ui.label(RichText::new(&p.qty).color(theme::WHITE));
                    ui.label(RichText::new(format!("{:>8.2}", avg)).color(theme::WHITE));
                    ui.label(RichText::new(format!("{:>8.2}", cur)).color(cur_color));
                    ui.label(RichText::new(format!("{:>12.2}", mkt_value)).color(theme::WHITE));
                    ui.label(RichText::new(fmt_signed_money(pl)).color(pl_color));
                    ui.label(RichText::new(fmt_pct(plpc)).color(pl_color));
                    ui.end_row();
                }
            });
    });
}

// ----------------------------------------------------------------------------
//  ORDERS sub-tab (options only)
// ----------------------------------------------------------------------------

fn orders_view(state: &mut OptionsState, ui: &mut egui::Ui) {
    ui.horizontal(|ui| {
        ui.label(RichText::new(" OPEN OPTION ORDERS ").color(theme::ORANGE).strong());
        if state.orders_loading {
            ui.label(RichText::new("  loading…").color(theme::GRAY2));
        }
        if !state.open_orders_err.is_empty() {
            ui.label(RichText::new(format!("  ERROR: {}", state.open_orders_err)).color(theme::RED));
        }
    });
    ui.add_space(2.0);

    if state.open_orders.is_empty() && !state.orders_loading {
        ui.label(RichText::new("  NO PENDING OPTION ORDERS — PRESS R TO REFRESH").color(theme::GRAY2));
        return;
    }

    let orders = state.open_orders.clone();
    egui::ScrollArea::both().show(ui, |ui| {
        egui::Grid::new("opt_orders")
            .num_columns(8)
            .striped(true)
            .spacing([10.0, 4.0])
            .show(ui, |ui| {
                let head = |ui: &mut egui::Ui, s: &str| {
                    ui.label(RichText::new(s).color(theme::ORANGE).strong());
                };
                head(ui, "CONTRACT");
                head(ui, "SIDE");
                head(ui, "TYPE");
                head(ui, "QTY");
                head(ui, "FILLED");
                head(ui, "LIMIT");
                head(ui, "STATUS");
                head(ui, "");
                ui.end_row();

                for o in &orders {
                    let side_color = match o.side.as_str() {
                        "buy" => theme::CYAN,
                        "sell" => theme::RED,
                        _ => theme::WHITE,
                    };
                    ui.label(RichText::new(human_occ(&o.symbol)).color(theme::WHITE).strong());
                    ui.label(RichText::new(o.side.to_uppercase()).color(side_color));
                    ui.label(RichText::new(o.order_type.to_uppercase()).color(theme::WHITE));
                    ui.label(RichText::new(&o.qty).color(theme::WHITE));
                    ui.label(RichText::new(&o.filled_qty).color(theme::WHITE));
                    ui.label(RichText::new(fmt_money_opt(o.limit_price_str())).color(theme::WHITE));
                    ui.label(RichText::new(o.status.to_uppercase()).color(status_color(&o.status)));
                    let busy = state.cancelling.contains(&o.id);
                    if busy {
                        ui.label(RichText::new("cancelling…").color(theme::GRAY2));
                    } else if ui
                        .add(egui::Button::new(RichText::new(" CANCEL ").color(theme::WHITE)).fill(theme::DARK))
                        .clicked()
                    {
                        state.cancel_confirm = Some(OptCancelConfirm {
                            id: o.id.clone(),
                            symbol: o.symbol.clone(),
                        });
                    }
                    ui.end_row();
                }
            });
    });
}

// ----------------------------------------------------------------------------
//  Modals
// ----------------------------------------------------------------------------

fn place_confirm_modal(
    state: &mut OptionsState,
    client: Arc<AlpacaClient>,
    tx: Sender<Msg>,
    ctx: &egui::Context,
) {
    let summary = state
        .place_confirm
        .as_ref()
        .map(|p| p.summary.clone())
        .unwrap_or_default();
    egui::Window::new("Confirm option order")
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .collapsible(false)
        .resizable(false)
        .show(ctx, |ui| {
            ui.label(RichText::new("You are about to place:").color(theme::GRAY2));
            ui.add_space(4.0);
            ui.label(RichText::new(&summary).color(theme::WHITE).strong().size(15.0));
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if ui
                    .add(egui::Button::new(RichText::new(" CONFIRM ").color(theme::BLACK).strong()).fill(theme::GREEN))
                    .clicked()
                {
                    if let Some(pc) = state.place_confirm.take() {
                        state.form.busy = true;
                        state.form.result = format!("Submitting: {}…", pc.summary);
                        state.form.result_color = theme::GRAY2;
                        workers::spawn_option_place_order(client.clone(), tx.clone(), ctx.clone(), pc.req, pc.summary);
                    }
                }
                if ui
                    .add(egui::Button::new(RichText::new(" CANCEL ").color(theme::WHITE)).fill(theme::DARK))
                    .clicked()
                {
                    state.place_confirm = None;
                }
            });
        });
}

fn cancel_confirm_modal(
    state: &mut OptionsState,
    client: Arc<AlpacaClient>,
    tx: Sender<Msg>,
    ctx: &egui::Context,
) {
    let (id, symbol) = match state.cancel_confirm.as_ref() {
        Some(c) => (c.id.clone(), c.symbol.clone()),
        None => return,
    };
    egui::Window::new("Cancel option order")
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .collapsible(false)
        .resizable(false)
        .show(ctx, |ui| {
            ui.label(RichText::new(format!("Cancel order for {}?", human_occ(&symbol))).color(theme::WHITE).strong());
            ui.label(RichText::new(format!("ID: {}", &id[..id.len().min(12)])).color(theme::GRAY2));
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if ui
                    .add(egui::Button::new(RichText::new(" CANCEL ORDER ").color(theme::BLACK).strong()).fill(theme::RED))
                    .clicked()
                {
                    state.cancelling.insert(id.clone());
                    state.cancel_confirm = None;
                    workers::spawn_option_cancel_order(client.clone(), tx.clone(), ctx.clone(), id.clone());
                }
                if ui
                    .add(egui::Button::new(RichText::new(" KEEP ").color(theme::WHITE)).fill(theme::DARK))
                    .clicked()
                {
                    state.cancel_confirm = None;
                }
            });
        });
}

// ----------------------------------------------------------------------------
//  Formatting helpers
// ----------------------------------------------------------------------------

fn fmt_money_opt(s: &str) -> String {
    if s.is_empty() {
        return "—".to_string();
    }
    match s.parse::<f64>() {
        Ok(v) => format!("{:>8.2}", v),
        Err(_) => s.to_string(),
    }
}

fn fmt_signed_money(v: f64) -> String {
    let sign = if v >= 0.0 { "+" } else { "" };
    format!("{}{:.2}", sign, v)
}

fn fmt_pct(v: f64) -> String {
    let sign = if v >= 0.0 { "+" } else { "" };
    format!("{}{:.2}%", sign, v * 100.0)
}

fn status_color(s: &str) -> Color32 {
    match s.to_lowercase().as_str() {
        "filled" => theme::GREEN,
        "partially_filled" => theme::YELLOW,
        "canceled" | "expired" | "rejected" => theme::RED,
        _ => theme::WHITE,
    }
}

// ============================================================================
//  Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn contract(sym: &str, exp: &str, kind: &str, strike: &str) -> OptionContract {
        OptionContract {
            symbol: sym.to_string(),
            name: String::new(),
            expiration_date: exp.to_string(),
            underlying_symbol: "AAPL".to_string(),
            kind: kind.to_string(),
            strike_price: strike.to_string(),
            open_interest: None,
            close_price: None,
            tradable: true,
        }
    }

    #[test]
    fn parse_occ_round_trips() {
        let p = parse_occ("AAPL260619C00150000").unwrap();
        assert_eq!(p.root, "AAPL");
        assert_eq!(p.kind, CallPut::Call);
        assert_eq!(p.strike, 150.0);
        assert_eq!(p.expiration, NaiveDate::from_ymd_opt(2026, 6, 19).unwrap());
        let back = format_occ(&p.root, p.expiration, p.kind, p.strike);
        assert_eq!(back, "AAPL260619C00150000");
    }

    #[test]
    fn parse_occ_handles_puts_and_fractional_strikes() {
        let p = parse_occ("SPY260320P00452500").unwrap();
        assert_eq!(p.root, "SPY");
        assert_eq!(p.kind, CallPut::Put);
        assert_eq!(p.strike, 452.5);
    }

    #[test]
    fn parse_occ_rejects_garbage() {
        assert!(parse_occ("AAPL").is_none());
        assert!(parse_occ("").is_none());
        assert!(parse_occ("260619C00150000").is_none()); // no root
        assert!(parse_occ("AAPL260619X00150000").is_none()); // bad call/put
    }

    #[test]
    fn human_occ_is_readable() {
        assert_eq!(human_occ("AAPL260619C00150000"), "AAPL 06/19/26 150C");
        assert_eq!(human_occ("SPY260320P00452500"), "SPY 03/20/26 452.5P");
        // Non-OCC falls through unchanged.
        assert_eq!(human_occ("AAPL"), "AAPL");
    }

    #[test]
    fn build_chain_groups_calls_and_puts_by_strike() {
        let contracts = vec![
            contract("AAPL260619C00150000", "2026-06-19", "call", "150"),
            contract("AAPL260619P00150000", "2026-06-19", "put", "150"),
            contract("AAPL260619C00155000", "2026-06-19", "call", "155"),
            // Different expiration — must be excluded.
            contract("AAPL260717C00150000", "2026-07-17", "call", "150"),
        ];
        let rows = build_chain(&contracts, "2026-06-19");
        assert_eq!(rows.len(), 2); // strikes 150 and 155
        assert_eq!(rows[0].strike, 150.0);
        assert!(rows[0].call.is_some());
        assert!(rows[0].put.is_some());
        assert_eq!(rows[1].strike, 155.0);
        assert!(rows[1].call.is_some());
        assert!(rows[1].put.is_none()); // no 155 put listed
    }

    #[test]
    fn expirations_are_sorted_and_deduped() {
        let contracts = vec![
            contract("AAPL260717C00150000", "2026-07-17", "call", "150"),
            contract("AAPL260619C00150000", "2026-06-19", "call", "150"),
            contract("AAPL260619P00150000", "2026-06-19", "put", "150"),
        ];
        assert_eq!(expirations(&contracts), vec!["2026-06-19", "2026-07-17"]);
    }

    #[test]
    fn build_market_order_omits_limit() {
        let (req, sum) = build_option_order(
            "AAPL260619C00150000",
            TradeSide::Buy,
            "2",
            OptOrderKind::Market,
            None,
        )
        .unwrap();
        assert_eq!(req.order_type, "market");
        assert_eq!(req.side, "buy");
        assert_eq!(req.qty, "2");
        assert_eq!(req.time_in_force, "day");
        assert!(req.limit_price.is_empty());
        assert!(sum.contains("MARKET"));
        // Simple option order serializes with no advanced fields.
        let json = serde_json::to_string(&req).unwrap();
        assert!(!json.contains("stop_price"));
        assert!(!json.contains("order_class"));
    }

    #[test]
    fn build_limit_order_requires_price() {
        // Limit with no price → rejected.
        assert!(build_option_order(
            "AAPL260619C00150000",
            TradeSide::Buy,
            "1",
            OptOrderKind::Limit,
            None
        )
        .is_none());
        let (req, _) = build_option_order(
            "AAPL260619C00150000",
            TradeSide::Sell,
            "1",
            OptOrderKind::Limit,
            Some(4.3),
        )
        .unwrap();
        assert_eq!(req.order_type, "limit");
        assert_eq!(req.limit_price, "4.30");
        assert_eq!(req.side, "sell");
    }

    #[test]
    fn build_order_rejects_fractional_and_nonpositive_qty() {
        assert!(build_option_order("AAPL260619C00150000", TradeSide::Buy, "1.5", OptOrderKind::Market, None).is_none());
        assert!(build_option_order("AAPL260619C00150000", TradeSide::Buy, "0", OptOrderKind::Market, None).is_none());
        assert!(build_option_order("AAPL260619C00150000", TradeSide::Buy, "abc", OptOrderKind::Market, None).is_none());
    }
}
