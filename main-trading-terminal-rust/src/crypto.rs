// Crypto desk — top-level Tab::Crypto with its own sub-tab strip:
//   1) Markets   — grid of all tradable USD-quoted Pairs (live bid/ask/last,
//                  24h %, volume) that doubles as the trade entry point —
//                  click B/S on a row, exactly like the Options Chain.
//   2) Positions — crypto holdings only (asset_class == "crypto"), live P&L
//                  recomputed from the crypto tick stream.
//   3) Orders    — open crypto orders only, with confirm-before-cancel.
//
// Data model (see docs/adr/0002): /v2/assets?asset_class=crypto supplies the
// Pair universe; a bulk REST snapshot (/v1beta3/crypto/us/snapshots) seeds
// bid/ask/last + 24h reference; the ALWAYS-ACTIVE crypto WebSocket overlays
// live prices. Trading rides the same /v2/orders + two-phase confirm modal as
// the Terminal and Options tabs.
//
// Ticket scope is the full crypto surface (per the design interview):
// Market / Limit / Stop-Limit, TIF GTC/IOC (crypto has no "day"), size as
// fractional Qty OR Notional dollars (Notional is market-only per Alpaca).

use std::collections::HashMap;
use std::sync::mpsc::Sender;
use std::sync::Arc;
use std::time::Instant;

use egui::{Color32, Key, RichText};

use crate::api::{AlpacaClient, Asset, CryptoSnapshot, Order, OrderRequest, Position};
use crate::stream::{LastTick, TickCache};
use crate::terminal::TradeSide;
use crate::theme;
use crate::workers::{self, Msg};

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum SubTab {
    Markets,
    Positions,
    Orders,
}

/// Crypto order types. Alpaca supports exactly these three for crypto —
/// there is no plain `stop` and no trailing stop.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum CryptoOrderKind {
    Market,
    Limit,
    StopLimit,
}

impl CryptoOrderKind {
    fn api_str(self) -> &'static str {
        match self {
            CryptoOrderKind::Market => "market",
            CryptoOrderKind::Limit => "limit",
            CryptoOrderKind::StopLimit => "stop_limit",
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            CryptoOrderKind::Market => "MARKET",
            CryptoOrderKind::Limit => "LIMIT",
            CryptoOrderKind::StopLimit => "STOP-LIMIT",
        }
    }
    pub fn needs_limit(self) -> bool {
        matches!(self, CryptoOrderKind::Limit | CryptoOrderKind::StopLimit)
    }
    pub fn needs_stop(self) -> bool {
        matches!(self, CryptoOrderKind::StopLimit)
    }
}

/// Crypto TIF — `day` does not exist for crypto; Alpaca accepts gtc / ioc.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum CryptoTif {
    Gtc,
    Ioc,
}

impl CryptoTif {
    fn api_str(self) -> &'static str {
        match self {
            CryptoTif::Gtc => "gtc",
            CryptoTif::Ioc => "ioc",
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            CryptoTif::Gtc => "GTC",
            CryptoTif::Ioc => "IOC",
        }
    }
}

/// The ticket's size field has two modes: fractional quantity (0.0035 BTC) or
/// Notional dollars ("$100 of BTC"). Never both — see CONTEXT.md "Notional".
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum SizeMode {
    Qty,
    Notional,
}

impl SizeMode {
    pub fn label(self) -> &'static str {
        match self {
            SizeMode::Qty => "QTY",
            SizeMode::Notional => "NOTIONAL $",
        }
    }
}

// ============================================================================
//  Pair universe (pure, unit-tested)
// ============================================================================

/// Filter the raw crypto asset list to the v1 universe: tradable, USD-quoted
/// Pairs, sorted by symbol. Stablecoin-quoted (BTC/USDT) and cross (ETH/BTC)
/// markets are deliberately out of scope — every dollar-denominated
/// assumption in the ticket and the positions table stays honest.
pub fn usd_pairs(assets: Vec<Asset>) -> Vec<Asset> {
    let mut v: Vec<Asset> = assets
        .into_iter()
        .filter(|a| a.tradable && a.symbol.ends_with("/USD"))
        .collect();
    v.sort_by(|a, b| a.symbol.cmp(&b.symbol));
    v
}

// ============================================================================
//  Order building (pure, unit-tested)
// ============================================================================

/// Price formatting that survives both BTC (5 figures) and sub-cent alts:
/// two decimals at >= $1, six (trimmed) below.
fn fmt_px(p: f64) -> String {
    if p >= 1.0 {
        format!("{:.2}", p)
    } else {
        let s = format!("{:.6}", p);
        s.trim_end_matches('0').trim_end_matches('.').to_string()
    }
}

/// Validate a crypto order and produce an `OrderRequest` + a human summary.
/// Returns a user-facing error string on any invalid combination so the
/// Markets view can explain exactly what's wrong.
#[allow(clippy::too_many_arguments)]
pub fn build_crypto_order(
    pair: &str,
    side: TradeSide,
    size_mode: SizeMode,
    size: &str,
    kind: CryptoOrderKind,
    tif: CryptoTif,
    limit_price: Option<f64>,
    stop_price: Option<f64>,
) -> Result<(OrderRequest, String), &'static str> {
    let pair = pair.trim().to_ascii_uppercase();
    if !pair.contains('/') {
        return Err("not a Pair — crypto symbols are slash-form (BTC/USD)");
    }
    let size = size.trim();
    let s: f64 = size.parse().map_err(|_| "size is not a number")?;
    if s <= 0.0 {
        return Err("size must be positive");
    }
    if size_mode == SizeMode::Notional && kind != CryptoOrderKind::Market {
        return Err("NOTIONAL is market-only — switch SIZE to QTY for limit / stop-limit");
    }

    let side_s = match side {
        TradeSide::Buy => "buy",
        TradeSide::Sell => "sell",
    }
    .to_string();

    let limit = if kind.needs_limit() {
        let p = limit_price.ok_or("limit price required (no live quote yet — type one in LMT)")?;
        if p <= 0.0 {
            return Err("limit price must be positive");
        }
        fmt_px(p)
    } else {
        String::new()
    };
    let stop = if kind.needs_stop() {
        let p = stop_price.ok_or("stop price required — type one in STOP")?;
        if p <= 0.0 {
            return Err("stop price must be positive");
        }
        Some(fmt_px(p))
    } else {
        None
    };

    let (qty, notional, size_str) = match size_mode {
        SizeMode::Qty => (size.to_string(), None, size.to_string()),
        SizeMode::Notional => (String::new(), Some(size.to_string()), format!("${}", size)),
    };

    let mut summary = format!("{} {} {} @ {}", side_s.to_uppercase(), size_str, pair, kind.label());
    if let Some(st) = &stop {
        summary.push_str(&format!(" stop {}", st));
    }
    if !limit.is_empty() {
        summary.push_str(&format!(" lmt {}", limit));
    }
    summary.push_str(&format!(" ({})", tif.label()));

    let req = OrderRequest {
        symbol: pair,
        qty,
        notional,
        side: side_s,
        order_type: kind.api_str().to_string(),
        time_in_force: tif.api_str().to_string(),
        limit_price: limit,
        stop_price: stop,
        trail_percent: None,
        order_class: None,
        take_profit: None,
        stop_loss: None,
    };
    Ok((req, summary))
}

// ============================================================================
//  State
// ============================================================================

pub struct CryptoOrderForm {
    pub size_mode: SizeMode,
    pub size_input: String,
    pub kind: CryptoOrderKind,
    pub tif: CryptoTif,
    /// Optional explicit limit price. Empty ⇒ fall back to the click-time
    /// ask (buy) / bid (sell), same convention as the Options Chain.
    pub limit_input: String,
    pub stop_input: String,
    pub result: String,
    pub result_color: Color32,
    pub busy: bool,
}

impl Default for CryptoOrderForm {
    fn default() -> Self {
        CryptoOrderForm {
            // $100 market GTC — always a valid combination, and notional
            // caps exposure better than a "1 unit" default would (1 BTC is
            // five figures).
            size_mode: SizeMode::Notional,
            size_input: "100".to_string(),
            kind: CryptoOrderKind::Market,
            tif: CryptoTif::Gtc,
            limit_input: String::new(),
            stop_input: String::new(),
            result: String::new(),
            result_color: theme::GRAY2,
            busy: false,
        }
    }
}

pub struct CryptoPlaceConfirm {
    pub req: OrderRequest,
    pub summary: String,
}

pub struct CryptoCancelConfirm {
    pub id: String,
    pub symbol: String,
}

pub struct CryptoState {
    pub sub_tab: SubTab,

    /// Markets-grid filter. Crypto-scoped input: a bare coin ("BTC") is
    /// treated as its /USD Pair. Persisted as `last_pair`.
    pub pair_input: String,

    // Markets data.
    pub pairs: Vec<Asset>,
    pub snapshots: HashMap<String, CryptoSnapshot>,
    pub loading: bool,
    pub err: String,
    /// Stale-response guard for markets loads (assets + snapshots carry it
    /// back). Bumped on each `kick_markets_load`.
    pub gen: u64,

    // Positions / Orders (filtered to asset_class "crypto").
    pub positions: Vec<Position>,
    pub positions_err: String,
    pub positions_loading: bool,
    pub open_orders: Vec<Order>,
    pub open_orders_err: String,
    pub orders_loading: bool,
    pub cancelling: std::collections::HashSet<String>,
    pub last_refresh: Option<Instant>,

    // Order form + modals.
    pub form: CryptoOrderForm,
    pub place_confirm: Option<CryptoPlaceConfirm>,
    pub cancel_confirm: Option<CryptoCancelConfirm>,
}

impl Default for CryptoState {
    fn default() -> Self {
        CryptoState {
            sub_tab: SubTab::Markets,
            pair_input: String::new(),
            pairs: Vec::new(),
            snapshots: HashMap::new(),
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
            form: CryptoOrderForm::default(),
            place_confirm: None,
            cancel_confirm: None,
        }
    }
}

impl CryptoState {
    pub fn new() -> Self {
        Self::default()
    }

    /// Load (or reload) the Markets grid: asset list first, then (when it
    /// lands — see `on_assets_loaded`) a bulk snapshot for those Pairs.
    /// Bumps `gen` so in-flight responses from a prior load are discarded.
    pub fn kick_markets_load(&mut self, client: Arc<AlpacaClient>, tx: Sender<Msg>, ctx: &egui::Context) {
        self.gen = self.gen.wrapping_add(1);
        self.loading = true;
        self.err.clear();
        workers::spawn_crypto_assets(client, tx, ctx.clone(), self.gen);
    }

    /// Apply a freshly-loaded asset list and chain the snapshot fetch for the
    /// USD-Pair universe it defines.
    pub fn on_assets_loaded(
        &mut self,
        assets: Vec<Asset>,
        client: Arc<AlpacaClient>,
        tx: Sender<Msg>,
        ctx: &egui::Context,
    ) {
        self.pairs = usd_pairs(assets);
        self.loading = false;
        let symbols: Vec<String> = self.pairs.iter().map(|a| a.symbol.clone()).collect();
        if !symbols.is_empty() {
            workers::spawn_crypto_snapshots(client, tx, ctx.clone(), symbols, self.gen);
        }
    }

    /// Refresh crypto positions + open orders. Mirrors the Options desk's 10s
    /// auto-refresh, scoped to crypto.
    pub fn refresh_account_data(&mut self, client: Arc<AlpacaClient>, tx: Sender<Msg>, ctx: &egui::Context) {
        self.positions_loading = true;
        self.orders_loading = true;
        self.last_refresh = Some(Instant::now());
        workers::spawn_crypto_positions(client.clone(), tx.clone(), ctx.clone());
        workers::spawn_crypto_open_orders(client, tx, ctx.clone());
    }

    /// All Pairs in the Markets grid — the live-WS subscription target while
    /// the Crypto tab is open (unioned with chart/watchlist/position Pairs by
    /// the caller; see docs/adr/0002).
    pub fn displayed_pairs(&self) -> Vec<String> {
        self.pairs.iter().map(|a| a.symbol.clone()).collect()
    }

    /// Grid rows after the pair filter. A bare coin filter ("BTC") matches
    /// its Pair by base prefix.
    fn filtered_pairs(&self) -> Vec<Asset> {
        let f = self.pair_input.trim().to_ascii_uppercase();
        if f.is_empty() {
            return self.pairs.clone();
        }
        self.pairs
            .iter()
            .filter(|a| {
                a.symbol.starts_with(&f)
                    || a.symbol.replace('/', "").starts_with(&f.replace('/', ""))
            })
            .cloned()
            .collect()
    }
}

// ============================================================================
//  Rendering
// ============================================================================

pub fn render(
    state: &mut CryptoState,
    client: Arc<AlpacaClient>,
    tx: Sender<Msg>,
    tick_cache: &TickCache,
    ui: &mut egui::Ui,
) {
    sub_tab_strip(state, ui);
    ui.separator();

    // Sub-tab number hotkeys 1..3 + R refresh — only when no field is focused.
    if !ui.ctx().memory(|m| m.focused().is_some()) {
        let pressed = |k: Key| ui.ctx().input(|i| i.key_pressed(k));
        if pressed(Key::Num1) {
            state.sub_tab = SubTab::Markets;
        }
        if pressed(Key::Num2) {
            state.sub_tab = SubTab::Positions;
        }
        if pressed(Key::Num3) {
            state.sub_tab = SubTab::Orders;
        }
        if pressed(Key::R) || pressed(Key::F5) {
            let ctx = ui.ctx().clone();
            state.kick_markets_load(client.clone(), tx.clone(), &ctx);
            state.refresh_account_data(client.clone(), tx.clone(), ui.ctx());
        }
    }

    pair_bar(state, ui);
    ui.separator();

    match state.sub_tab {
        SubTab::Markets => markets_view(state, tick_cache, ui),
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

fn sub_tab_strip(state: &mut CryptoState, ui: &mut egui::Ui) {
    ui.horizontal(|ui| {
        ui.label(RichText::new(" CRYPTO ").color(theme::ORANGE).strong());
        ui.separator();
        for (sub, label, hk) in [
            (SubTab::Markets, "Markets", "1"),
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
        ui.separator();
        ui.label(RichText::new("24/7 market").color(theme::GRAY2).small());
    });
}

fn pair_bar(state: &mut CryptoState, ui: &mut egui::Ui) {
    ui.horizontal_wrapped(|ui| {
        ui.label(RichText::new(" PAIR ").color(theme::ORANGE).strong());
        let resp = ui.add(
            egui::TextEdit::singleline(&mut state.pair_input)
                .desired_width(110.0)
                .hint_text("filter — BTC or BTC/USD"),
        );
        if resp.changed() {
            state.pair_input = state.pair_input.to_uppercase();
        }
        if ui.button(" ✕ ").on_hover_text("Clear filter").clicked() {
            state.pair_input.clear();
        }
        if state.loading {
            ui.label(RichText::new(" loading…").color(theme::GRAY2));
        }
        if !state.err.is_empty() {
            ui.label(RichText::new(format!(" ERROR: {}", state.err)).color(theme::RED));
        }
    });
}

// ----------------------------------------------------------------------------
//  MARKETS sub-tab
// ----------------------------------------------------------------------------

/// Resolved per-pair quote: snapshot values overlaid with live ticks. Same
/// shape as the Options Chain's quote resolution.
struct Quote {
    bid: Option<f64>,
    ask: Option<f64>,
    last: Option<f64>,
    vol: Option<f64>,
    prev_close: Option<f64>,
    live: bool,
}

fn quote_for(snap: Option<&CryptoSnapshot>, tick: Option<&LastTick>) -> Quote {
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
    let prev_close = snap.and_then(|s| s.prev_daily_bar.as_ref()).map(|b| b.close);
    Quote { bid, ask, last, vol, prev_close, live }
}

fn markets_view(state: &mut CryptoState, tick_cache: &TickCache, ui: &mut egui::Ui) {
    if state.pairs.is_empty() {
        if state.loading {
            ui.label(RichText::new("  loading markets…").color(theme::GRAY2));
        } else {
            ui.label(RichText::new("  No tradable USD pairs — press R to refresh.").color(theme::GRAY2));
        }
        return;
    }

    order_settings_bar(state, ui);
    ui.add_space(2.0);

    let rows = state.filtered_pairs();
    if rows.is_empty() {
        ui.label(RichText::new("  No pairs match the filter.").color(theme::GRAY2));
        return;
    }

    let live = tick_cache.read().ok();
    let mut pending: Option<(TradeSide, String, Option<f64>)> = None;

    egui::ScrollArea::both().show(ui, |ui| {
        egui::Grid::new("crypto_markets")
            .num_columns(7)
            .striped(true)
            .spacing([12.0, 3.0])
            .show(ui, |ui| {
                let head = |ui: &mut egui::Ui, s: &str| {
                    ui.label(RichText::new(s).color(theme::ORANGE).strong());
                };
                head(ui, "PAIR");
                head(ui, "BID");
                head(ui, "ASK");
                head(ui, "LAST");
                head(ui, "24H %");
                head(ui, "VOLUME");
                head(ui, "TRADE");
                ui.end_row();

                for a in &rows {
                    let q = quote_for(
                        state.snapshots.get(&a.symbol),
                        live.as_ref().and_then(|m| m.get(&a.symbol)),
                    );
                    ui.label(RichText::new(&a.symbol).color(theme::WHITE).strong());
                    px_cell(ui, q.bid, if q.live { theme::CYAN } else { theme::WHITE });
                    px_cell(ui, q.ask, if q.live { theme::ORANGE } else { theme::WHITE });
                    let last_color = match (q.last, q.prev_close) {
                        (Some(l), Some(p)) if p > 0.0 => {
                            if l >= p { theme::GREEN } else { theme::RED }
                        }
                        _ => theme::WHITE,
                    };
                    px_cell(ui, q.last, last_color);
                    // 24h change vs prev daily close — the natural reference
                    // on a market with no session open/close.
                    match (q.last, q.prev_close) {
                        (Some(l), Some(p)) if p > 0.0 => {
                            let pct = (l - p) / p * 100.0;
                            let color = if pct >= 0.0 { theme::GREEN } else { theme::RED };
                            let sign = if pct >= 0.0 { "+" } else { "" };
                            ui.label(RichText::new(format!("{sign}{:.2}%", pct)).color(color));
                        }
                        _ => {
                            ui.label(RichText::new("    —").color(theme::GRAY));
                        }
                    }
                    match q.vol {
                        Some(v) => ui.label(RichText::new(format!("{:>12.0}", v)).color(theme::WHITE)),
                        None => ui.label(RichText::new("        —").color(theme::GRAY)),
                    };
                    ui.horizontal(|ui| {
                        if ui
                            .add(egui::Button::new(RichText::new(" B ").color(theme::BLACK).strong()).fill(theme::GREEN))
                            .on_hover_text("Buy")
                            .clicked()
                        {
                            pending = Some((TradeSide::Buy, a.symbol.clone(), q.ask));
                        }
                        if ui
                            .add(egui::Button::new(RichText::new(" S ").color(theme::WHITE)).fill(theme::RED))
                            .on_hover_text("Sell")
                            .clicked()
                        {
                            pending = Some((TradeSide::Sell, a.symbol.clone(), q.bid));
                        }
                    });
                    ui.end_row();
                }
            });
    });
    drop(live);

    if let Some((side, pair, click_px)) = pending {
        // Explicit LMT input wins; otherwise fall back to the click-time
        // ask/bid (the Options Chain convention).
        let limit = parse_pos(&state.form.limit_input).or(click_px);
        let stop = parse_pos(&state.form.stop_input);
        match build_crypto_order(
            &pair,
            side,
            state.form.size_mode,
            &state.form.size_input,
            state.form.kind,
            state.form.tif,
            limit,
            stop,
        ) {
            Ok((req, summary)) => {
                state.form.result.clear();
                state.place_confirm = Some(CryptoPlaceConfirm { req, summary });
            }
            Err(e) => {
                state.form.result = format!("Invalid order — {e}");
                state.form.result_color = theme::RED;
            }
        }
    }
}

fn parse_pos(s: &str) -> Option<f64> {
    s.trim().parse::<f64>().ok().filter(|v| *v > 0.0)
}

/// Price cell — `fmt_px` precision (2dp above $1, 6dp below) or an em-dash.
fn px_cell(ui: &mut egui::Ui, v: Option<f64>, color: Color32) {
    match v {
        Some(x) => ui.label(RichText::new(format!("{:>10}", fmt_px(x))).color(color)),
        None => ui.label(RichText::new("         —").color(theme::GRAY)),
    };
}

fn order_settings_bar(state: &mut CryptoState, ui: &mut egui::Ui) {
    ui.horizontal_wrapped(|ui| {
        ui.label(RichText::new(" ORDER ").color(theme::ORANGE).strong());
        ui.label(RichText::new("SIZE").color(theme::ORANGE));
        egui::ComboBox::from_id_salt("cry_size_mode")
            .selected_text(RichText::new(state.form.size_mode.label()).color(theme::WHITE))
            .show_ui(ui, |ui| {
                for m in [SizeMode::Notional, SizeMode::Qty] {
                    ui.selectable_value(&mut state.form.size_mode, m, m.label());
                }
            });
        ui.add(egui::TextEdit::singleline(&mut state.form.size_input).desired_width(70.0));
        ui.label(RichText::new("TYPE").color(theme::ORANGE));
        egui::ComboBox::from_id_salt("cry_kind")
            .selected_text(RichText::new(state.form.kind.label()).color(theme::WHITE))
            .show_ui(ui, |ui| {
                for k in [CryptoOrderKind::Market, CryptoOrderKind::Limit, CryptoOrderKind::StopLimit] {
                    ui.selectable_value(&mut state.form.kind, k, k.label());
                }
            });
        ui.label(RichText::new("TIF").color(theme::ORANGE));
        egui::ComboBox::from_id_salt("cry_tif")
            .selected_text(RichText::new(state.form.tif.label()).color(theme::WHITE))
            .show_ui(ui, |ui| {
                for t in [CryptoTif::Gtc, CryptoTif::Ioc] {
                    ui.selectable_value(&mut state.form.tif, t, t.label());
                }
            });
        // Render only the price rows the kind actually uses — no greyed-out
        // fields (house rule).
        if state.form.needs_stop() {
            ui.label(RichText::new("STOP").color(theme::ORANGE));
            ui.add(
                egui::TextEdit::singleline(&mut state.form.stop_input)
                    .desired_width(80.0)
                    .hint_text("price"),
            );
        }
        if state.form.needs_limit() {
            ui.label(RichText::new("LMT").color(theme::ORANGE));
            ui.add(
                egui::TextEdit::singleline(&mut state.form.limit_input)
                    .desired_width(80.0)
                    .hint_text("blank = quote"),
            );
        }
        ui.label(
            RichText::new("· click B (buy) / S (sell) on a pair to place")
                .color(theme::GRAY2),
        );
        if !state.form.result.is_empty() {
            ui.label(RichText::new(&state.form.result).color(state.form.result_color));
        }
    });
}

impl CryptoOrderForm {
    fn needs_limit(&self) -> bool {
        self.kind.needs_limit()
    }
    fn needs_stop(&self) -> bool {
        self.kind.needs_stop()
    }
}

// ----------------------------------------------------------------------------
//  POSITIONS sub-tab (crypto only)
// ----------------------------------------------------------------------------

fn positions_view(state: &mut CryptoState, tick_cache: &TickCache, ui: &mut egui::Ui) {
    ui.horizontal(|ui| {
        ui.label(RichText::new(" CRYPTO POSITIONS ").color(theme::ORANGE).strong());
        if state.positions_loading {
            ui.label(RichText::new("  loading…").color(theme::GRAY2));
        }
        if !state.positions_err.is_empty() {
            ui.label(RichText::new(format!("  ERROR: {}", state.positions_err)).color(theme::RED));
        }
    });
    ui.add_space(2.0);

    if state.positions.is_empty() && !state.positions_loading {
        ui.label(RichText::new("  NO CRYPTO POSITIONS — PRESS R TO REFRESH").color(theme::GRAY2));
        return;
    }

    let positions = state.positions.clone();
    let live = tick_cache.read().ok();
    egui::ScrollArea::both().show(ui, |ui| {
        egui::Grid::new("cry_pos")
            .num_columns(8)
            .striped(true)
            .spacing([10.0, 4.0])
            .show(ui, |ui| {
                let head = |ui: &mut egui::Ui, s: &str| {
                    ui.label(RichText::new(s).color(theme::ORANGE).strong());
                };
                head(ui, "PAIR");
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
                    let mkt_value = cur * qty;
                    let pl = (cur - avg) * qty * side_sign;
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
                    ui.label(RichText::new(&p.symbol).color(theme::WHITE).strong());
                    ui.label(RichText::new(p.side.to_uppercase()).color(side_color));
                    // Fractional qty — show as typed by the broker, no rounding.
                    ui.label(RichText::new(&p.qty).color(theme::WHITE));
                    ui.label(RichText::new(fmt_px(avg)).color(theme::WHITE));
                    ui.label(RichText::new(fmt_px(cur)).color(cur_color));
                    ui.label(RichText::new(format!("{:>12.2}", mkt_value)).color(theme::WHITE));
                    let sign = if pl >= 0.0 { "+" } else { "-" };
                    ui.label(RichText::new(format!("{sign}${:.2}", pl.abs())).color(pl_color));
                    ui.label(RichText::new(format!("{sign}{:.2}%", (plpc * 100.0).abs())).color(pl_color));
                    ui.end_row();
                }
            });
    });
}

// ----------------------------------------------------------------------------
//  ORDERS sub-tab (crypto only)
// ----------------------------------------------------------------------------

fn orders_view(state: &mut CryptoState, ui: &mut egui::Ui) {
    ui.horizontal(|ui| {
        ui.label(RichText::new(" OPEN CRYPTO ORDERS ").color(theme::ORANGE).strong());
        if state.orders_loading {
            ui.label(RichText::new("  loading…").color(theme::GRAY2));
        }
        if !state.open_orders_err.is_empty() {
            ui.label(RichText::new(format!("  ERROR: {}", state.open_orders_err)).color(theme::RED));
        }
    });
    ui.add_space(2.0);

    if state.open_orders.is_empty() && !state.orders_loading {
        ui.label(RichText::new("  NO OPEN CRYPTO ORDERS").color(theme::GRAY2));
        return;
    }

    let orders = state.open_orders.clone();
    egui::ScrollArea::both().show(ui, |ui| {
        egui::Grid::new("cry_orders")
            .num_columns(8)
            .striped(true)
            .spacing([10.0, 4.0])
            .show(ui, |ui| {
                let head = |ui: &mut egui::Ui, s: &str| {
                    ui.label(RichText::new(s).color(theme::ORANGE).strong());
                };
                head(ui, "PAIR");
                head(ui, "SIDE");
                head(ui, "TYPE");
                head(ui, "SIZE");
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
                    ui.label(RichText::new(&o.symbol).color(theme::WHITE).strong());
                    ui.label(RichText::new(o.side.to_uppercase()).color(side_color));
                    ui.label(RichText::new(o.order_type.to_uppercase()).color(theme::WHITE));
                    // Notional orders come back with empty qty — show $amount.
                    let size = if o.qty.is_empty() {
                        o.notional
                            .as_deref()
                            .map(|n| format!("${n}"))
                            .unwrap_or_else(|| "—".to_string())
                    } else {
                        o.qty.clone()
                    };
                    ui.label(RichText::new(size).color(theme::WHITE));
                    ui.label(RichText::new(&o.filled_qty).color(theme::WHITE));
                    let lim = o.limit_price_str();
                    ui.label(
                        RichText::new(if lim.is_empty() { "—" } else { lim }).color(theme::WHITE),
                    );
                    ui.label(RichText::new(o.status.to_uppercase()).color(status_color(&o.status)));
                    let busy = state.cancelling.contains(&o.id);
                    if busy {
                        ui.label(RichText::new("cancelling…").color(theme::GRAY2));
                    } else if ui
                        .add(egui::Button::new(RichText::new(" CANCEL ").color(theme::WHITE)).fill(theme::DARK))
                        .clicked()
                    {
                        state.cancel_confirm = Some(CryptoCancelConfirm {
                            id: o.id.clone(),
                            symbol: o.symbol.clone(),
                        });
                    }
                    ui.end_row();
                }
            });
    });
}

fn status_color(status: &str) -> Color32 {
    match status {
        "new" | "accepted" | "pending_new" => theme::CYAN,
        "partially_filled" => theme::YELLOW,
        "filled" => theme::GREEN,
        _ => theme::GRAY2,
    }
}

// ----------------------------------------------------------------------------
//  Modals
// ----------------------------------------------------------------------------

fn place_confirm_modal(
    state: &mut CryptoState,
    client: Arc<AlpacaClient>,
    tx: Sender<Msg>,
    ctx: &egui::Context,
) {
    let summary = state
        .place_confirm
        .as_ref()
        .map(|p| p.summary.clone())
        .unwrap_or_default();
    egui::Window::new("Confirm crypto order")
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
                        workers::spawn_crypto_place_order(client.clone(), tx.clone(), ctx.clone(), pc.req, pc.summary);
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
    state: &mut CryptoState,
    client: Arc<AlpacaClient>,
    tx: Sender<Msg>,
    ctx: &egui::Context,
) {
    let (id, symbol) = match state.cancel_confirm.as_ref() {
        Some(c) => (c.id.clone(), c.symbol.clone()),
        None => return,
    };
    egui::Window::new("Cancel crypto order")
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .collapsible(false)
        .resizable(false)
        .show(ctx, |ui| {
            ui.label(RichText::new(format!("Cancel order for {}?", symbol)).color(theme::WHITE).strong());
            ui.label(RichText::new(format!("ID: {}", &id[..id.len().min(12)])).color(theme::GRAY2));
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if ui
                    .add(egui::Button::new(RichText::new(" CANCEL ORDER ").color(theme::BLACK).strong()).fill(theme::RED))
                    .clicked()
                {
                    state.cancelling.insert(id.clone());
                    state.cancel_confirm = None;
                    workers::spawn_crypto_cancel_order(client.clone(), tx.clone(), ctx.clone(), id.clone());
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

// ============================================================================
//  Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn asset(sym: &str, tradable: bool) -> Asset {
        // Asset only derives Deserialize — build via JSON like the wire does.
        serde_json::from_value(serde_json::json!({
            "symbol": sym,
            "name": sym,
            "status": "active",
            "tradable": tradable,
        }))
        .unwrap()
    }

    #[test]
    fn usd_pairs_filters_to_tradable_usd_quotes() {
        let v = usd_pairs(vec![
            asset("BTC/USD", true),
            asset("BTC/USDT", true),
            asset("ETH/BTC", true),
            asset("ETH/USD", true),
            asset("DOGE/USD", false),
        ]);
        let syms: Vec<&str> = v.iter().map(|a| a.symbol.as_str()).collect();
        assert_eq!(syms, vec!["BTC/USD", "ETH/USD"]);
    }

    #[test]
    fn market_qty_order_builds() {
        let (req, summary) = build_crypto_order(
            "BTC/USD",
            TradeSide::Buy,
            SizeMode::Qty,
            "0.0035",
            CryptoOrderKind::Market,
            CryptoTif::Gtc,
            None,
            None,
        )
        .unwrap();
        assert_eq!(req.symbol, "BTC/USD");
        assert_eq!(req.qty, "0.0035");
        assert_eq!(req.notional, None);
        assert_eq!(req.order_type, "market");
        assert_eq!(req.time_in_force, "gtc");
        assert!(req.limit_price.is_empty());
        assert_eq!(req.stop_price, None);
        assert_eq!(summary, "BUY 0.0035 BTC/USD @ MARKET (GTC)");
    }

    #[test]
    fn market_notional_order_builds() {
        let (req, summary) = build_crypto_order(
            "ETH/USD",
            TradeSide::Buy,
            SizeMode::Notional,
            "100",
            CryptoOrderKind::Market,
            CryptoTif::Ioc,
            None,
            None,
        )
        .unwrap();
        assert_eq!(req.qty, "");
        assert_eq!(req.notional, Some("100".to_string()));
        assert_eq!(req.time_in_force, "ioc");
        assert_eq!(summary, "BUY $100 ETH/USD @ MARKET (IOC)");
        // Wire check: qty must be omitted entirely on a notional order.
        let v = serde_json::to_value(&req).unwrap();
        assert!(!v.as_object().unwrap().contains_key("qty"));
        assert_eq!(v["notional"], "100");
    }

    #[test]
    fn notional_is_market_only() {
        let err = build_crypto_order(
            "BTC/USD",
            TradeSide::Buy,
            SizeMode::Notional,
            "100",
            CryptoOrderKind::Limit,
            CryptoTif::Gtc,
            Some(60000.0),
            None,
        )
        .unwrap_err();
        assert!(err.contains("market-only"));
    }

    #[test]
    fn limit_order_requires_price() {
        assert!(build_crypto_order(
            "BTC/USD",
            TradeSide::Sell,
            SizeMode::Qty,
            "0.5",
            CryptoOrderKind::Limit,
            CryptoTif::Gtc,
            None,
            None,
        )
        .is_err());
        let (req, _) = build_crypto_order(
            "BTC/USD",
            TradeSide::Sell,
            SizeMode::Qty,
            "0.5",
            CryptoOrderKind::Limit,
            CryptoTif::Gtc,
            Some(68000.0),
            None,
        )
        .unwrap();
        assert_eq!(req.order_type, "limit");
        assert_eq!(req.limit_price, "68000.00");
    }

    #[test]
    fn stop_limit_requires_both_prices() {
        // Crypto has no plain stop — stop_limit needs stop AND limit.
        assert!(build_crypto_order(
            "BTC/USD",
            TradeSide::Sell,
            SizeMode::Qty,
            "0.5",
            CryptoOrderKind::StopLimit,
            CryptoTif::Gtc,
            Some(60000.0),
            None,
        )
        .is_err());
        let (req, summary) = build_crypto_order(
            "BTC/USD",
            TradeSide::Sell,
            SizeMode::Qty,
            "0.5",
            CryptoOrderKind::StopLimit,
            CryptoTif::Gtc,
            Some(59500.0),
            Some(60000.0),
        )
        .unwrap();
        assert_eq!(req.order_type, "stop_limit");
        assert_eq!(req.stop_price, Some("60000.00".to_string()));
        assert_eq!(req.limit_price, "59500.00");
        assert_eq!(summary, "SELL 0.5 BTC/USD @ STOP-LIMIT stop 60000.00 lmt 59500.00 (GTC)");
    }

    #[test]
    fn rejects_bad_sizes_and_non_pairs() {
        let mk = |size: &str, pair: &str| {
            build_crypto_order(
                pair,
                TradeSide::Buy,
                SizeMode::Qty,
                size,
                CryptoOrderKind::Market,
                CryptoTif::Gtc,
                None,
                None,
            )
        };
        assert!(mk("0", "BTC/USD").is_err());
        assert!(mk("-1", "BTC/USD").is_err());
        assert!(mk("abc", "BTC/USD").is_err());
        assert!(mk("", "BTC/USD").is_err());
        // Slashless symbols are not Pairs — the desk never sends BTCUSD.
        assert!(mk("1", "BTCUSD").is_err());
    }

    #[test]
    fn sub_dollar_prices_keep_precision() {
        let (req, _) = build_crypto_order(
            "SHIB/USD",
            TradeSide::Buy,
            SizeMode::Qty,
            "100000",
            CryptoOrderKind::Limit,
            CryptoTif::Gtc,
            Some(0.000012),
            None,
        )
        .unwrap();
        assert_eq!(req.limit_price, "0.000012");
    }
}
