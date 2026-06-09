// Terminal tab — ports the canonical Go trading terminal's four
// account-management sub-tabs into a single egui Tab::Terminal. Sub-tabs:
//   1) Positions — table of open positions, double-click → Sell shortcut
//   2) Trade     — order form (action/type/symbol/qty/limit) + confirm modal
//   3) Orders    — pending orders table, row "Cancel" with confirm modal
//   4) Activity  — merged feed of /account/activities + closed orders
//
// We intentionally skip the Go terminal's own Chart sub-tab because this app
// already has a richer Chart top-level tab.
//
// API surface used (all already on AlpacaClient):
//   get_positions, get_account, get_orders, get_closed_orders, get_activities,
//   place_order, cancel_order. No new HTTP code needed.

use std::sync::mpsc::Sender;
use std::sync::Arc;

use egui::{Color32, Key, RichText};

use crate::api::{Account, Activity, AlpacaClient, Bar, Order, OrderRequest, Position, StopLoss, TakeProfit};
use crate::stocks::AssetCache;
use crate::stream::TickCache;
use crate::theme;
use crate::workers::{self, Msg};

/// One executed fill, projected onto the Chart tab as a marker. Sourced from
/// the user's own activity feed + closed orders — *not* the strategy
/// signals (which are theoretical buy/sell rules).
#[derive(Debug, Clone)]
pub struct Fill {
    pub time: chrono::DateTime<chrono::Utc>,
    pub price: f64,
    // Recorded from the activity feed so the upcoming hover tooltip can show
    // size; the diamond marker itself is positioned only by time + price.
    #[allow(dead_code)]
    pub qty: f64,
    pub side: FillSide,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FillSide {
    Buy,
    Sell,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum SubTab {
    Positions,
    Trade,
    Orders,
    Activity,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum TradeSide {
    Buy,
    Sell,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum OrderKind {
    Market,
    Limit,
    Stop,
    StopLimit,
    TrailingStop,
}

impl OrderKind {
    /// String the form needs to put on `order_type` for the Alpaca API.
    fn api_str(self) -> &'static str {
        match self {
            OrderKind::Market => "market",
            OrderKind::Limit => "limit",
            OrderKind::Stop => "stop",
            OrderKind::StopLimit => "stop_limit",
            OrderKind::TrailingStop => "trailing_stop",
        }
    }
    fn label(self) -> &'static str {
        match self {
            OrderKind::Market => "MARKET",
            OrderKind::Limit => "LIMIT",
            OrderKind::Stop => "STOP",
            OrderKind::StopLimit => "STOP-LIMIT",
            OrderKind::TrailingStop => "TRAIL",
        }
    }
    fn needs_limit(self) -> bool { matches!(self, OrderKind::Limit | OrderKind::StopLimit) }
    fn needs_stop(self) -> bool { matches!(self, OrderKind::Stop | OrderKind::StopLimit) }
    fn needs_trail(self) -> bool { matches!(self, OrderKind::TrailingStop) }
}

/// Form state for the Trade sub-tab. Mirrors the Go terminal's `tradeForm`
/// fields verbatim so the UX feels the same.
pub struct TradeForm {
    pub side: TradeSide,
    pub kind: OrderKind,
    pub symbol_input: String,
    pub qty_input: String,
    pub limit_input: String,
    /// Stop price for `stop` / `stop_limit`. Ignored for other kinds.
    pub stop_input: String,
    /// Trail percent for `trailing_stop`. Ignored for other kinds.
    pub trail_pct_input: String,
    /// When true, this order will be sent as a `bracket` — Alpaca attaches
    /// a take-profit limit AND a stop-loss to the parent. The TP/SL legs
    /// must be on the OPPOSITE side from the parent and only fire after the
    /// parent fills, so Alpaca enforces them server-side.
    pub bracket: bool,
    pub tp_input: String,        // bracket take-profit limit price
    pub sl_stop_input: String,   // bracket stop-loss STOP price
    pub sl_limit_input: String,  // optional bracket stop-loss LIMIT price
    /// Inline result message under the form. Color-coded green on success,
    /// red on error, gray for "submitting…".
    pub result: String,
    pub result_color: Color32,
    pub busy: bool,
    /// Autocomplete suggestions for the SYMBOL field. Populated from the
    /// shared AssetCache on each keystroke; rendered as buttons in a row.
    pub autocomplete: Vec<(String, String)>,
}

impl Default for TradeForm {
    fn default() -> Self {
        TradeForm {
            side: TradeSide::Buy,
            kind: OrderKind::Market,
            symbol_input: String::new(),
            qty_input: String::new(),
            limit_input: String::new(),
            stop_input: String::new(),
            trail_pct_input: String::new(),
            bracket: false,
            tp_input: String::new(),
            sl_stop_input: String::new(),
            sl_limit_input: String::new(),
            result: String::new(),
            result_color: theme::GRAY2,
            busy: false,
            autocomplete: Vec::new(),
        }
    }
}

/// Confirm-place-order modal: pops up after the user clicks PLACE ORDER.
pub struct PlaceConfirm {
    pub req: OrderRequest,
    pub summary: String,
}

/// Confirm-cancel modal: pops up after the user clicks Cancel on an order row.
pub struct CancelConfirm {
    pub id: String,
    pub symbol: String,
}

/// Inline preview chart on the Trade sub-tab — like Fidelity's order ticket
/// showing the stock you're about to trade. ~3 months of daily closes.
/// `gen` guards against stale responses if the user types a new symbol
/// before the old one's HTTP completes.
#[derive(Default)]
pub struct TradeChartState {
    pub symbol: String,
    pub bars: Vec<Bar>,
    pub loading: bool,
    pub err: String,
    pub gen: u64,
}

pub struct TerminalState {
    pub sub_tab: SubTab,

    // Data caches (last successful fetch lives here; stale-while-revalidate).
    pub positions: Vec<Position>,
    pub positions_err: String,
    pub positions_loading: bool,

    pub account: Option<Account>,
    pub account_err: String,

    pub open_orders: Vec<Order>,
    pub open_orders_err: String,
    pub orders_loading: bool,
    /// Set of order ids the user has dispatched a cancel for. Used purely
    /// for the inline "cancelling…" hint; the actual removal happens on
    /// the next refresh.
    pub cancelling: std::collections::HashSet<String>,

    pub closed_orders: Vec<Order>,
    pub activities: Vec<Activity>,
    pub activity_err: String,
    pub activity_loading: bool,

    // Form + modal state.
    pub form: TradeForm,
    pub place_confirm: Option<PlaceConfirm>,
    pub cancel_confirm: Option<CancelConfirm>,

    /// Trade sub-tab preview chart (Fidelity-style).
    pub trade_chart: TradeChartState,
    /// Deferred chart-load request. Set by paths that pre-fill the form
    /// without rendering it themselves (Positions SELL shortcut). Drained
    /// by `render()` at the top of the next frame so the load fires with
    /// `client`/`tx` properly in scope.
    pub pending_chart_load: Option<String>,

    // Auto-refresh bookkeeping. We re-fetch every 10s when on the Terminal
    // tab — matches the Go terminal's background goroutine.
    pub last_refresh: Option<std::time::Instant>,
}

impl Default for TerminalState {
    fn default() -> Self {
        TerminalState {
            sub_tab: SubTab::Positions,
            positions: Vec::new(),
            positions_err: String::new(),
            positions_loading: false,
            account: None,
            account_err: String::new(),
            open_orders: Vec::new(),
            open_orders_err: String::new(),
            orders_loading: false,
            cancelling: std::collections::HashSet::new(),
            closed_orders: Vec::new(),
            activities: Vec::new(),
            activity_err: String::new(),
            activity_loading: false,
            form: TradeForm::default(),
            place_confirm: None,
            cancel_confirm: None,
            trade_chart: TradeChartState::default(),
            pending_chart_load: None,
            last_refresh: None,
        }
    }
}

impl TerminalState {
    pub fn new() -> Self { Self::default() }

    /// Load the Fidelity-style preview chart on the Trade sub-tab for the
    /// given symbol. Skipped when the symbol is empty or matches what's
    /// already shown (no point re-fetching what we already have). The `gen`
    /// counter discards stale HTTP responses from rapid symbol changes.
    pub fn kick_load_trade_chart(
        &mut self,
        client: Arc<AlpacaClient>,
        tx: Sender<Msg>,
        ctx: &egui::Context,
        symbol: String,
    ) {
        let sym = symbol.trim().to_uppercase();
        if sym.is_empty() { return; }
        if sym == self.trade_chart.symbol && !self.trade_chart.bars.is_empty() {
            return;
        }
        self.trade_chart.gen = self.trade_chart.gen.wrapping_add(1);
        self.trade_chart.symbol = sym.clone();
        self.trade_chart.loading = true;
        self.trade_chart.err.clear();
        self.trade_chart.bars.clear();
        workers::spawn_load_trade_chart(client, tx, ctx.clone(), sym, self.trade_chart.gen);
    }

    /// Build the user's fill list for a given symbol — preferred source is
    /// the activity feed (precise transaction times); closed orders are a
    /// fallback for cases where Alpaca emitted only the order summary, not
    /// per-fill activity rows. De-duped by `order_id`. Returned in ascending
    /// time order so the caller can render markers in chronological order.
    pub fn fills_for_symbol(&self, symbol: &str) -> Vec<Fill> {
        let sym = symbol.to_ascii_uppercase();
        let mut out: Vec<Fill> = Vec::new();
        let mut seen_order_ids: std::collections::HashSet<String> = std::collections::HashSet::new();
        for a in &self.activities {
            if a.activity_type != "FILL" && a.activity_type != "PARTIAL_FILL" {
                continue;
            }
            let sym_match = a.symbol.as_deref().map(|s| s.to_ascii_uppercase()) == Some(sym.clone());
            if !sym_match {
                continue;
            }
            let Some(time) = a.transaction_time else { continue; };
            let Some(price) = a.price.as_ref().and_then(|s| s.parse::<f64>().ok()) else { continue; };
            let qty = a.qty.as_ref().and_then(|s| s.parse::<f64>().ok()).unwrap_or(0.0);
            let side = match a.side.as_deref() {
                Some("buy") | Some("BUY") => FillSide::Buy,
                Some("sell") | Some("SELL") => FillSide::Sell,
                _ => continue,
            };
            if let Some(id) = &a.order_id {
                seen_order_ids.insert(id.clone());
            }
            out.push(Fill { time, price, qty, side });
        }
        for o in &self.closed_orders {
            if o.symbol.to_ascii_uppercase() != sym {
                continue;
            }
            if o.status.to_lowercase() != "filled" {
                continue;
            }
            if seen_order_ids.contains(&o.id) {
                continue;
            }
            let Some(price) = o.filled_avg_price_str().parse::<f64>().ok() else { continue; };
            let qty = o.filled_qty.parse::<f64>().unwrap_or(0.0);
            let side = match o.side.as_str() {
                "buy" => FillSide::Buy,
                "sell" => FillSide::Sell,
                _ => continue,
            };
            out.push(Fill { time: o.created_at, price, qty, side });
        }
        out.sort_by_key(|f| f.time);
        out
    }

    /// Kick off all background fetches relevant to the Terminal tab. Called
    /// when the tab is first opened, when the user presses R/F5, and from
    /// the 10s auto-refresh.
    pub fn refresh_all(
        &mut self,
        client: Arc<AlpacaClient>,
        tx: Sender<Msg>,
        ctx: &egui::Context,
    ) {
        self.positions_loading = true;
        self.orders_loading = true;
        self.activity_loading = true;
        self.last_refresh = Some(std::time::Instant::now());
        workers::spawn_positions(client.clone(), tx.clone(), ctx.clone());
        workers::spawn_account(client.clone(), tx.clone(), ctx.clone());
        workers::spawn_open_orders(client.clone(), tx.clone(), ctx.clone());
        workers::spawn_closed_orders(client.clone(), tx.clone(), ctx.clone());
        workers::spawn_activities(client, tx, ctx.clone());
    }
}

// ============================================================================
//  Rendering
// ============================================================================

pub fn render(
    state: &mut TerminalState,
    client: Arc<AlpacaClient>,
    tx: Sender<Msg>,
    assets: &AssetCache,
    tick_cache: &TickCache,
    ui: &mut egui::Ui,
) {
    // Sub-tab strip — appears UNDER the main top-level tab strip in app.rs.
    sub_tab_strip(state, ui);
    ui.separator();

    // Sub-tab number hotkeys 1..4 — only while no text field has focus,
    // matching the global Q/R rule in the Go terminal.
    if !ui.ctx().memory(|m| m.focused().is_some()) {
        let pressed = |k: Key| ui.ctx().input(|i| i.key_pressed(k));
        if pressed(Key::Num1) { state.sub_tab = SubTab::Positions; }
        if pressed(Key::Num2) { state.sub_tab = SubTab::Trade; }
        if pressed(Key::Num3) { state.sub_tab = SubTab::Orders; }
        if pressed(Key::Num4) { state.sub_tab = SubTab::Activity; }
        if pressed(Key::R) || pressed(Key::F5) {
            state.refresh_all(client.clone(), tx.clone(), ui.ctx());
        }
    }

    // Account summary band (mirrors the Go terminal's bottom status line —
    // but here it lives at the top of every sub-tab so it's always visible).
    account_band(state, ui);
    ui.separator();

    // Drain any deferred chart-load requests (e.g. from Positions SELL
    // shortcut) before sub-tab dispatch — so when Trade renders this frame
    // the chart state already says "loading…" instead of being empty.
    if let Some(sym) = state.pending_chart_load.take() {
        state.kick_load_trade_chart(client.clone(), tx.clone(), ui.ctx(), sym);
    }

    // Dispatch to the active sub-tab.
    match state.sub_tab {
        SubTab::Positions => positions_view(state, tick_cache, ui),
        SubTab::Trade => trade_view(state, client.clone(), tx.clone(), assets, ui),
        SubTab::Orders => orders_view(state, ui),
        SubTab::Activity => activity_view(state, ui),
    }

    // Modal overlays (only one can be open at a time in practice).
    if state.place_confirm.is_some() {
        place_confirm_modal(state, client.clone(), tx.clone(), ui.ctx());
    }
    if state.cancel_confirm.is_some() {
        cancel_confirm_modal(state, client, tx, ui.ctx());
    }
}

fn sub_tab_strip(state: &mut TerminalState, ui: &mut egui::Ui) {
    ui.horizontal(|ui| {
        ui.label(RichText::new(" TERMINAL ").color(theme::ORANGE).strong());
        ui.separator();
        for (sub, label, hk) in [
            (SubTab::Positions, "Positions", "1"),
            (SubTab::Trade, "Trade", "2"),
            (SubTab::Orders, "Orders", "3"),
            (SubTab::Activity, "Activity", "4"),
        ] {
            let active = state.sub_tab == sub;
            let text = format!(" [{hk}] {label} ");
            let btn = if active {
                egui::Button::new(RichText::new(text).color(theme::BLACK).strong())
                    .fill(theme::CYAN)
            } else {
                egui::Button::new(RichText::new(text).color(theme::GRAY2)).fill(theme::DARK)
            };
            if ui.add(btn).clicked() {
                state.sub_tab = sub;
            }
        }
    });
}

fn account_band(state: &TerminalState, ui: &mut egui::Ui) {
    ui.horizontal_wrapped(|ui| {
        match &state.account {
            Some(a) => {
                kv(ui, "PORTFOLIO", &fmt_money_str(&a.portfolio_value), theme::WHITE);
                kv(ui, "EQUITY", &fmt_money_str(&a.equity), theme::WHITE);
                kv(ui, "CASH", &fmt_money_str(&a.cash), theme::WHITE);
                kv(ui, "BUYING PWR", &fmt_money_str(&a.buying_power), theme::GREEN);
            }
            None if !state.account_err.is_empty() => {
                ui.label(
                    RichText::new(format!(" ACCOUNT ERROR: {}", state.account_err))
                        .color(theme::RED),
                );
            }
            None => {
                ui.label(RichText::new(" loading account…").color(theme::GRAY2));
            }
        }
    });
}

fn kv(ui: &mut egui::Ui, k: &str, v: &str, color: Color32) {
    ui.label(RichText::new(format!(" {k} ")).color(theme::ORANGE).strong());
    ui.label(RichText::new(v).color(color));
}

// ----------------------------------------------------------------------------
//  POSITIONS sub-tab
// ----------------------------------------------------------------------------

fn positions_view(state: &mut TerminalState, tick_cache: &TickCache, ui: &mut egui::Ui) {
    ui.horizontal(|ui| {
        ui.label(RichText::new(" POSITIONS ").color(theme::ORANGE).strong());
        if state.positions_loading {
            ui.label(RichText::new("  loading…").color(theme::GRAY2));
        }
        if !state.positions_err.is_empty() {
            ui.label(RichText::new(format!("  ERROR: {}", state.positions_err)).color(theme::RED));
        }
        // Hint that the table is updating between the 10-second refreshes.
        let any_live = tick_cache
            .read()
            .map(|c| state.positions.iter().any(|p| c.get(&p.symbol).and_then(|t| t.last_price).is_some()))
            .unwrap_or(false);
        if any_live {
            ui.label(
                RichText::new("  • LIVE ")
                    .color(theme::BLACK)
                    .background_color(theme::GREEN)
                    .strong()
                    .size(10.0),
            );
        }
    });
    ui.add_space(2.0);

    if state.positions.is_empty() && !state.positions_loading {
        ui.label(
            RichText::new("  NO OPEN POSITIONS — PRESS R TO REFRESH").color(theme::GRAY2),
        );
        return;
    }

    egui::ScrollArea::vertical().show(ui, |ui| {
        egui::Grid::new("positions_grid")
            .num_columns(8)
            .striped(true)
            .spacing([10.0, 4.0])
            .show(ui, |ui| {
                let head = |ui: &mut egui::Ui, s: &str| {
                    ui.label(RichText::new(s).color(theme::ORANGE).strong());
                };
                head(ui, "SYMBOL");
                head(ui, "SIDE");
                head(ui, "QTY");
                head(ui, "AVG ENTRY");
                head(ui, "CURRENT");
                head(ui, "MKT VALUE");
                head(ui, "P&L");
                head(ui, "P&L %");
                ui.end_row();

                // Snapshot before borrowing into the row closure below.
                let positions = state.positions.clone();
                // Take a single read-lock for the whole table — release at
                // the end. Live ticks are written by the WS thread; reads
                // are cheap and we don't want one lock per row.
                let live_lock = tick_cache.read().ok();
                for p in &positions {
                    let qty = p.qty.parse::<f64>().unwrap_or(0.0);
                    let avg = p.avg_entry_price.parse::<f64>().unwrap_or(0.0);
                    // Prefer live last over the broker's snapshot. If the
                    // stream hasn't emitted anything for this symbol yet,
                    // fall back to whatever /v2/positions returned at the
                    // last refresh.
                    let live_last = live_lock
                        .as_ref()
                        .and_then(|c| c.get(&p.symbol))
                        .and_then(|t| t.last_price);
                    let cur = live_last
                        .unwrap_or_else(|| p.current_price.parse::<f64>().unwrap_or(0.0));
                    // Sign so shorts profit when price falls. side is "long"
                    // or "short" per Alpaca; everything else is treated as
                    // long for safety.
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
                    // Make the CURRENT cell visually distinct when it's
                    // coming from the stream — orange tint vs the broker's
                    // stale snapshot.
                    let cur_color = if live_last.is_some() { theme::ORANGE } else { theme::WHITE };
                    ui.label(RichText::new(&p.symbol).color(theme::WHITE).strong());
                    ui.label(RichText::new(p.side.to_uppercase()).color(side_color));
                    ui.label(RichText::new(&p.qty).color(theme::WHITE));
                    ui.label(RichText::new(format!("{:>12.2}", avg)).color(theme::WHITE));
                    ui.label(RichText::new(format!("{:>12.2}", cur)).color(cur_color));
                    ui.label(RichText::new(format!("{:>12.2}", mkt_value)).color(theme::WHITE));
                    ui.label(RichText::new(fmt_signed_money(pl)).color(pl_color));
                    ui.label(RichText::new(fmt_pct(plpc)).color(pl_color));
                    ui.end_row();
                }
            });

        // Row-action shortcuts under the table.
        ui.add_space(8.0);
        ui.horizontal_wrapped(|ui| {
            ui.label(RichText::new(" SHORTCUT ").color(theme::ORANGE).strong());
            for p in state.positions.clone() {
                let label = format!(" SELL {} ", p.symbol);
                if ui
                    .add(
                        egui::Button::new(RichText::new(label).color(theme::WHITE))
                            .fill(theme::DARK),
                    )
                    .clicked()
                {
                    state.form.side = TradeSide::Sell;
                    state.form.kind = OrderKind::Market;
                    state.form.symbol_input = p.symbol.clone();
                    state.form.qty_input = p.qty.clone();
                    state.form.result.clear();
                    state.sub_tab = SubTab::Trade;
                    state.pending_chart_load = Some(p.symbol.clone());
                }
            }
        });
    });
}

// ----------------------------------------------------------------------------
//  TRADE sub-tab
// ----------------------------------------------------------------------------

fn trade_view(
    state: &mut TerminalState,
    client: Arc<AlpacaClient>,
    tx: Sender<Msg>,
    assets: &AssetCache,
    ui: &mut egui::Ui,
) {
    ui.heading(RichText::new("Place a new order").color(theme::ORANGE));
    ui.add_space(4.0);

    // Symbols the user committed this frame — fetched after the grid so we
    // don't double-borrow `state` inside the closure.
    let mut commit_symbol: Option<String> = None;

    egui::Grid::new("trade_form").num_columns(2).spacing([8.0, 8.0]).show(ui, |ui| {
        ui.label(RichText::new("ACTION").color(theme::ORANGE).strong());
        ui.horizontal(|ui| {
            side_pill(ui, "BUY", &mut state.form.side, TradeSide::Buy, theme::CYAN);
            side_pill(ui, "SELL", &mut state.form.side, TradeSide::Sell, theme::RED);
        });
        ui.end_row();

        ui.label(RichText::new("TYPE").color(theme::ORANGE).strong());
        ui.horizontal(|ui| {
            // ComboBox handles 5 kinds cleanly; pills would be cramped.
            egui::ComboBox::from_id_salt("trade_kind")
                .selected_text(RichText::new(state.form.kind.label()).color(theme::WHITE))
                .show_ui(ui, |ui| {
                    for k in [
                        OrderKind::Market,
                        OrderKind::Limit,
                        OrderKind::Stop,
                        OrderKind::StopLimit,
                        OrderKind::TrailingStop,
                    ] {
                        ui.selectable_value(&mut state.form.kind, k, k.label());
                    }
                });
        });
        ui.end_row();

        ui.label(RichText::new("SYMBOL").color(theme::ORANGE).strong());
        let resp = ui.add(
            egui::TextEdit::singleline(&mut state.form.symbol_input)
                .desired_width(180.0),
        );
        if resp.changed() {
            state.form.symbol_input = state.form.symbol_input.to_uppercase();
            state.form.autocomplete = if state.form.symbol_input.is_empty() {
                Vec::new()
            } else {
                assets.filter(&state.form.symbol_input, 6)
            };
        }
        // Esc dismisses the autocomplete row without clearing the input.
        if resp.has_focus() && ui.input(|i| i.key_pressed(Key::Escape)) {
            state.form.autocomplete.clear();
        }
        // Press Enter in the symbol field → commit, load preview chart, and
        // drop the autocomplete row so it doesn't linger over the result.
        if resp.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
            commit_symbol = Some(state.form.symbol_input.clone());
            state.form.autocomplete.clear();
        }
        ui.end_row();

        ui.label("");
        if !state.form.autocomplete.is_empty() {
            let suggestions = state.form.autocomplete.clone();
            ui.horizontal_wrapped(|ui| {
                // Tickers only — typing a company name still finds the
                // ticker via `AssetCache::filter`'s name-substring fallback.
                for (sym, _name) in suggestions.iter().take(6) {
                    if ui
                        .add(
                            egui::Button::new(RichText::new(sym).color(theme::CYAN))
                                .fill(theme::DARK),
                        )
                        .clicked()
                    {
                        state.form.symbol_input = sym.clone();
                        state.form.autocomplete.clear();
                        commit_symbol = Some(sym.clone());
                    }
                }
            });
        } else {
            let name = assets.company_name(&state.form.symbol_input);
            if !name.is_empty() {
                ui.label(RichText::new(name).color(theme::CYAN));
            } else {
                ui.label("");
            }
        }
        ui.end_row();

        ui.label(RichText::new("QUANTITY").color(theme::ORANGE).strong());
        ui.add(
            egui::TextEdit::singleline(&mut state.form.qty_input)
                .desired_width(120.0)
                .hint_text("shares"),
        );
        ui.end_row();

        // Conditional price fields — only render the rows that the current
        // order kind actually uses, instead of greying out unused fields.
        // Keeps the form short and obvious.
        if state.form.kind.needs_limit() {
            ui.label(RichText::new("LIMIT PX").color(theme::ORANGE).strong());
            ui.add(
                egui::TextEdit::singleline(&mut state.form.limit_input)
                    .desired_width(120.0)
                    .hint_text("e.g. 187.50"),
            );
            ui.end_row();
        }
        if state.form.kind.needs_stop() {
            ui.label(RichText::new("STOP PX").color(theme::ORANGE).strong());
            ui.add(
                egui::TextEdit::singleline(&mut state.form.stop_input)
                    .desired_width(120.0)
                    .hint_text("trigger price"),
            );
            ui.end_row();
        }
        if state.form.kind.needs_trail() {
            ui.label(RichText::new("TRAIL %").color(theme::ORANGE).strong());
            ui.add(
                egui::TextEdit::singleline(&mut state.form.trail_pct_input)
                    .desired_width(120.0)
                    .hint_text("e.g. 5  (= 5%)"),
            );
            ui.end_row();
        }
    });

    // Bracket toggle — when on, Alpaca attaches a take-profit and stop-loss
    // leg server-side. The protective legs fire on the OPPOSITE side of the
    // parent only after the parent fills.
    ui.add_space(6.0);
    ui.horizontal(|ui| {
        ui.checkbox(&mut state.form.bracket, "Bracket (attach TP + SL)");
        if state.form.bracket {
            ui.label(
                RichText::new(" both legs required ").color(theme::GRAY2).size(11.0),
            );
        }
    });
    if state.form.bracket {
        egui::Grid::new("bracket_form").num_columns(2).spacing([8.0, 6.0]).show(ui, |ui| {
            ui.label(RichText::new("TAKE PROFIT @").color(theme::GREEN).strong());
            ui.add(
                egui::TextEdit::singleline(&mut state.form.tp_input)
                    .desired_width(120.0)
                    .hint_text("limit price"),
            );
            ui.end_row();

            ui.label(RichText::new("STOP LOSS @").color(theme::RED).strong());
            ui.add(
                egui::TextEdit::singleline(&mut state.form.sl_stop_input)
                    .desired_width(120.0)
                    .hint_text("stop trigger"),
            );
            ui.end_row();

            ui.label(RichText::new("SL LIMIT (opt)").color(theme::GRAY2).strong());
            ui.add(
                egui::TextEdit::singleline(&mut state.form.sl_limit_input)
                    .desired_width(120.0)
                    .hint_text("blank = stop market"),
            );
            ui.end_row();
        });
    }

    ui.add_space(6.0);
    ui.horizontal(|ui| {
        // Submit gating reflects the kind- and bracket-specific required
        // fields. Centralized here so PLACE ORDER greys-out cleanly without
        // having to call build_order on every keystroke.
        let f = &state.form;
        let mut can_submit = !f.busy
            && !f.symbol_input.trim().is_empty()
            && !f.qty_input.trim().is_empty();
        if f.kind.needs_limit() && f.limit_input.trim().is_empty() { can_submit = false; }
        if f.kind.needs_stop() && f.stop_input.trim().is_empty() { can_submit = false; }
        if f.kind.needs_trail() && f.trail_pct_input.trim().is_empty() { can_submit = false; }
        if f.bracket {
            if f.tp_input.trim().is_empty() || f.sl_stop_input.trim().is_empty() {
                can_submit = false;
            }
        }

        let place_btn = egui::Button::new(
            RichText::new(" PLACE ORDER ").color(theme::BLACK).strong(),
        )
        .fill(if can_submit { theme::GREEN } else { theme::GRAY });
        if ui.add_enabled(can_submit, place_btn).clicked() {
            if let Some((req, summary)) = build_order(&state.form) {
                state.place_confirm = Some(PlaceConfirm { req, summary });
            } else {
                state.form.result = "Invalid form — check quantity / limit price.".to_string();
                state.form.result_color = theme::RED;
            }
        }

        if ui
            .add(egui::Button::new(RichText::new(" CLEAR ").color(theme::WHITE)).fill(theme::DARK))
            .clicked()
        {
            state.form = TradeForm::default();
            state.trade_chart = TradeChartState::default();
        }

        // Explicit "LOAD CHART" button — useful when the user types the
        // ticker fully without picking from autocomplete and doesn't want
        // to press Enter.
        let show_load = !state.form.symbol_input.trim().is_empty()
            && state.form.symbol_input.trim().to_uppercase() != state.trade_chart.symbol;
        if show_load {
            if ui
                .add(
                    egui::Button::new(RichText::new(" LOAD CHART ").color(theme::BLACK).strong())
                        .fill(theme::CYAN),
                )
                .clicked()
            {
                commit_symbol = Some(state.form.symbol_input.clone());
            }
        }
    });

    if !state.form.result.is_empty() {
        ui.add_space(6.0);
        ui.label(RichText::new(&state.form.result).color(state.form.result_color));
    }

    if let Some(sym) = commit_symbol {
        state.kick_load_trade_chart(client.clone(), tx.clone(), ui.ctx(), sym);
    }

    // Preview chart goes beneath the form — only renders once a symbol has
    // been committed (autocomplete pick, Enter, or LOAD CHART click).
    if !state.trade_chart.symbol.is_empty() {
        ui.add_space(10.0);
        ui.separator();
        ui.add_space(4.0);
        trade_chart_preview(state, assets, ui);
    }
}

fn trade_chart_preview(state: &mut TerminalState, assets: &AssetCache, ui: &mut egui::Ui) {
    use egui_plot::{Line, Plot, PlotPoints};

    // Top stat strip — symbol, company, last close, day Δ.
    let (last_close, prev_close, day_high, day_low) = {
        let bars = &state.trade_chart.bars;
        let last = bars.last().map(|b| b.close);
        let prev = if bars.len() >= 2 { Some(bars[bars.len() - 2].close) } else { None };
        let hi = bars.last().map(|b| b.high);
        let lo = bars.last().map(|b| b.low);
        (last, prev, hi, lo)
    };

    ui.horizontal_wrapped(|ui| {
        ui.label(
            RichText::new(&state.trade_chart.symbol)
                .color(theme::ORANGE)
                .strong()
                .size(20.0),
        );
        let name = assets.company_name(&state.trade_chart.symbol);
        if !name.is_empty() {
            ui.label(RichText::new(format!("  {}", name)).color(theme::CYAN));
        }
        ui.add_space(12.0);

        if state.trade_chart.loading {
            ui.label(RichText::new("loading…").color(theme::GRAY2));
        } else if !state.trade_chart.err.is_empty() {
            ui.label(
                RichText::new(format!("ERROR: {}", state.trade_chart.err)).color(theme::RED),
            );
        } else if let (Some(last), Some(prev)) = (last_close, prev_close) {
            let chg = last - prev;
            let pct = if prev != 0.0 { chg / prev } else { 0.0 };
            let color = if chg >= 0.0 { theme::GREEN } else { theme::RED };
            ui.label(RichText::new(format!("${:.2}", last)).color(theme::WHITE).strong().size(18.0));
            ui.label(
                RichText::new(format!("  {}{:.2}  ({}{:.2}%)",
                    if chg >= 0.0 { "+" } else { "" }, chg,
                    if chg >= 0.0 { "+" } else { "" }, pct * 100.0,
                ))
                .color(color)
                .strong(),
            );
            if let (Some(h), Some(l)) = (day_high, day_low) {
                ui.add_space(12.0);
                ui.label(RichText::new(format!("H {:.2}  L {:.2}", h, l)).color(theme::GRAY2));
            }
        }

        // Side anchor: the side currently selected on the form. Visual
        // reminder of which way you're about to trade this stock.
        ui.add_space(16.0);
        let (side_label, side_color) = match state.form.side {
            TradeSide::Buy => ("▲ BUY", theme::CYAN),
            TradeSide::Sell => ("▼ SELL", theme::RED),
        };
        ui.label(RichText::new(side_label).color(side_color).strong());
    });

    ui.add_space(4.0);

    // The plot itself — line of recent daily closes, plus a faint "last
    // price" horizontal reference. ~3 months of data; fits compactly.
    let plot_h = 200.0_f32;
    let chart_color = if let (Some(last), Some(prev)) = (last_close, prev_close) {
        if last >= prev { theme::GREEN } else { theme::RED }
    } else {
        theme::CYAN
    };

    let points: Vec<[f64; 2]> = state
        .trade_chart
        .bars
        .iter()
        .enumerate()
        .map(|(i, b)| [i as f64, b.close])
        .collect();

    if points.is_empty() && !state.trade_chart.loading && state.trade_chart.err.is_empty() {
        ui.label(RichText::new("(no data)").color(theme::GRAY2));
        return;
    }

    Plot::new("trade_preview_plot")
        .height(plot_h)
        .show_axes([true, true])
        .show_grid([true, true])
        .allow_drag(false)
        .allow_zoom(false)
        .allow_scroll(false)
        .allow_boxed_zoom(false)
        .show(ui, |plot_ui| {
            if !points.is_empty() {
                plot_ui.line(Line::new(PlotPoints::from(points)).color(chart_color).width(1.5));
                if let Some(last) = last_close {
                    let len = state.trade_chart.bars.len() as f64;
                    plot_ui.line(
                        Line::new(PlotPoints::from(vec![[0.0, last], [len - 1.0, last]]))
                            .color(theme::GRAY2)
                            .width(0.5),
                    );
                }
            }
        });
}

fn side_pill(
    ui: &mut egui::Ui,
    label: &str,
    cur: &mut TradeSide,
    val: TradeSide,
    on_color: Color32,
) {
    let active = *cur == val;
    let txt = format!(" {label} ");
    let btn = if active {
        egui::Button::new(RichText::new(txt).color(theme::BLACK).strong()).fill(on_color)
    } else {
        egui::Button::new(RichText::new(txt).color(theme::GRAY2)).fill(theme::DARK)
    };
    if ui.add(btn).clicked() {
        *cur = val;
    }
}

// `kind_pill` was retired when the TYPE picker became a ComboBox — 5 kinds
// (Market / Limit / Stop / StopLimit / TrailingStop) didn't fit cleanly as
// pills, so we render via egui::ComboBox now. Keep this comment so the
// pattern isn't reintroduced accidentally.

/// Validate the form and produce an OrderRequest. Returns None if any
/// required field is missing or non-numeric. Bracket orders require BOTH a
/// take-profit limit and a stop-loss stop — partial brackets aren't valid
/// per Alpaca's API.
fn build_order(f: &TradeForm) -> Option<(OrderRequest, String)> {
    let sym = f.symbol_input.trim().to_uppercase();
    let qty = f.qty_input.trim();
    if sym.is_empty() || qty.is_empty() { return None; }
    qty.parse::<f64>().ok()?;

    let side = match f.side { TradeSide::Buy => "buy", TradeSide::Sell => "sell" }.to_string();
    let order_type = f.kind.api_str().to_string();

    // Per-kind validation + summary fragment construction.
    let limit_price = if f.kind.needs_limit() {
        let lp = f.limit_input.trim();
        lp.parse::<f64>().ok()?;
        lp.to_string()
    } else {
        String::new()
    };
    let stop_price = if f.kind.needs_stop() {
        let sp = f.stop_input.trim();
        sp.parse::<f64>().ok()?;
        Some(sp.to_string())
    } else {
        None
    };
    let trail_percent = if f.kind.needs_trail() {
        let tp = f.trail_pct_input.trim();
        tp.parse::<f64>().ok()?;
        Some(tp.to_string())
    } else {
        None
    };

    // Bracket legs. Optional unless `bracket` toggled, in which case both
    // TP-limit AND SL-stop are mandatory. SL-limit is optional even on a
    // bracket (gives the user "stop" vs "stop_limit" for the protective leg).
    let (order_class, take_profit, stop_loss) = if f.bracket {
        let tp = f.tp_input.trim();
        let sl_stop = f.sl_stop_input.trim();
        tp.parse::<f64>().ok()?;
        sl_stop.parse::<f64>().ok()?;
        let sl_limit_opt = if !f.sl_limit_input.trim().is_empty() {
            let sl = f.sl_limit_input.trim();
            sl.parse::<f64>().ok()?;
            Some(sl.to_string())
        } else {
            None
        };
        (
            Some("bracket".to_string()),
            Some(TakeProfit { limit_price: tp.to_string() }),
            Some(StopLoss {
                stop_price: sl_stop.to_string(),
                limit_price: sl_limit_opt,
            }),
        )
    } else {
        (None, None, None)
    };

    // Human-readable summary for the confirm modal.
    let mut summary = format!("{} {} {}", side.to_uppercase(), qty, sym);
    summary.push_str(" @ ");
    summary.push_str(f.kind.label());
    if !limit_price.is_empty() {
        summary.push_str(&format!(" {}", limit_price));
    }
    if let Some(sp) = &stop_price {
        summary.push_str(&format!(" (stop {})", sp));
    }
    if let Some(tp) = &trail_percent {
        summary.push_str(&format!(" ({}% trail)", tp));
    }
    if let Some(tp) = &take_profit {
        summary.push_str(&format!(" + TP {}", tp.limit_price));
    }
    if let Some(sl) = &stop_loss {
        match &sl.limit_price {
            Some(l) => summary.push_str(&format!(" / SL {} → {}", sl.stop_price, l)),
            None => summary.push_str(&format!(" / SL {}", sl.stop_price)),
        }
    }
    summary.push_str(" (DAY)");

    let req = OrderRequest {
        symbol: sym,
        qty: qty.to_string(),
        side,
        order_type,
        time_in_force: "day".to_string(),
        limit_price,
        stop_price,
        trail_percent,
        order_class,
        take_profit,
        stop_loss,
    };
    Some((req, summary))
}

fn place_confirm_modal(
    state: &mut TerminalState,
    client: Arc<AlpacaClient>,
    tx: Sender<Msg>,
    ctx: &egui::Context,
) {
    let summary = state.place_confirm.as_ref().map(|p| p.summary.clone()).unwrap_or_default();
    egui::Window::new("Confirm order")
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
                    .add(
                        egui::Button::new(RichText::new(" CONFIRM ").color(theme::BLACK).strong())
                            .fill(theme::GREEN),
                    )
                    .clicked()
                {
                    if let Some(pc) = state.place_confirm.take() {
                        state.form.busy = true;
                        state.form.result = format!("Submitting: {}…", pc.summary);
                        state.form.result_color = theme::GRAY2;
                        workers::spawn_place_order(client.clone(), tx.clone(), ctx.clone(), pc.req, pc.summary);
                    }
                }
                if ui
                    .add(
                        egui::Button::new(RichText::new(" CANCEL ").color(theme::WHITE))
                            .fill(theme::DARK),
                    )
                    .clicked()
                {
                    state.place_confirm = None;
                }
            });
        });
}

// ----------------------------------------------------------------------------
//  ORDERS sub-tab
// ----------------------------------------------------------------------------

fn orders_view(state: &mut TerminalState, ui: &mut egui::Ui) {
    ui.horizontal(|ui| {
        ui.label(RichText::new(" OPEN ORDERS ").color(theme::ORANGE).strong());
        if state.orders_loading {
            ui.label(RichText::new("  loading…").color(theme::GRAY2));
        }
        if !state.open_orders_err.is_empty() {
            ui.label(RichText::new(format!("  ERROR: {}", state.open_orders_err)).color(theme::RED));
        }
    });
    ui.add_space(2.0);

    if state.open_orders.is_empty() && !state.orders_loading {
        ui.label(
            RichText::new("  NO PENDING ORDERS — PRESS R TO REFRESH").color(theme::GRAY2),
        );
        return;
    }

    let orders = state.open_orders.clone();
    egui::ScrollArea::vertical().show(ui, |ui| {
        egui::Grid::new("orders_grid")
            .num_columns(9)
            .striped(true)
            .spacing([10.0, 4.0])
            .show(ui, |ui| {
                let head = |ui: &mut egui::Ui, s: &str| {
                    ui.label(RichText::new(s).color(theme::ORANGE).strong());
                };
                head(ui, "ID");
                head(ui, "SYMBOL");
                head(ui, "SIDE");
                head(ui, "TYPE");
                head(ui, "QTY");
                head(ui, "FILLED");
                head(ui, "LIMIT");
                head(ui, "STATUS");
                head(ui, "");
                ui.end_row();

                for o in &orders {
                    let short_id: String = o.id.chars().take(8).collect();
                    let side_color = match o.side.as_str() {
                        "buy" => theme::CYAN,
                        "sell" => theme::RED,
                        _ => theme::WHITE,
                    };
                    let status_color = status_color(&o.status);
                    ui.label(RichText::new(short_id).color(theme::GRAY2));
                    ui.label(RichText::new(&o.symbol).color(theme::WHITE).strong());
                    ui.label(RichText::new(o.side.to_uppercase()).color(side_color));
                    ui.label(RichText::new(o.order_type.to_uppercase()).color(theme::WHITE));
                    ui.label(RichText::new(&o.qty).color(theme::WHITE));
                    ui.label(RichText::new(&o.filled_qty).color(theme::WHITE));
                    ui.label(RichText::new(fmt_money_str(o.limit_price_str())).color(theme::WHITE));
                    ui.label(RichText::new(o.status.to_uppercase()).color(status_color));
                    let busy = state.cancelling.contains(&o.id);
                    if busy {
                        ui.label(RichText::new("cancelling…").color(theme::GRAY2));
                    } else if ui
                        .add(
                            egui::Button::new(RichText::new(" CANCEL ").color(theme::WHITE))
                                .fill(theme::DARK),
                        )
                        .clicked()
                    {
                        state.cancel_confirm = Some(CancelConfirm {
                            id: o.id.clone(),
                            symbol: o.symbol.clone(),
                        });
                    }
                    ui.end_row();
                }
            });
    });
}

fn cancel_confirm_modal(
    state: &mut TerminalState,
    client: Arc<AlpacaClient>,
    tx: Sender<Msg>,
    ctx: &egui::Context,
) {
    let (id, symbol) = match state.cancel_confirm.as_ref() {
        Some(c) => (c.id.clone(), c.symbol.clone()),
        None => return,
    };
    egui::Window::new("Cancel order")
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .collapsible(false)
        .resizable(false)
        .show(ctx, |ui| {
            ui.label(
                RichText::new(format!("Cancel order for {symbol}?")).color(theme::WHITE).strong(),
            );
            ui.label(RichText::new(format!("ID: {}", &id[..id.len().min(12)])).color(theme::GRAY2));
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if ui
                    .add(
                        egui::Button::new(
                            RichText::new(" CANCEL ORDER ").color(theme::BLACK).strong(),
                        )
                        .fill(theme::RED),
                    )
                    .clicked()
                {
                    state.cancelling.insert(id.clone());
                    state.cancel_confirm = None;
                    workers::spawn_cancel_order(client.clone(), tx.clone(), ctx.clone(), id.clone());
                }
                if ui
                    .add(
                        egui::Button::new(RichText::new(" KEEP ").color(theme::WHITE))
                            .fill(theme::DARK),
                    )
                    .clicked()
                {
                    state.cancel_confirm = None;
                }
            });
        });
}

// ----------------------------------------------------------------------------
//  ACTIVITY sub-tab
// ----------------------------------------------------------------------------

fn activity_view(state: &mut TerminalState, ui: &mut egui::Ui) {
    ui.horizontal(|ui| {
        ui.label(RichText::new(" ACTIVITY ").color(theme::ORANGE).strong());
        if state.activity_loading {
            ui.label(RichText::new("  loading…").color(theme::GRAY2));
        }
        if !state.activity_err.is_empty() {
            ui.label(RichText::new(format!("  ERROR: {}", state.activity_err)).color(theme::RED));
        }
    });
    ui.add_space(2.0);

    if state.activities.is_empty() && state.closed_orders.is_empty() && !state.activity_loading {
        ui.label(RichText::new("  NO ACTIVITY — PRESS R TO REFRESH").color(theme::GRAY2));
        return;
    }

    egui::ScrollArea::vertical().show(ui, |ui| {
        egui::Grid::new("activity_grid")
            .num_columns(8)
            .striped(true)
            .spacing([10.0, 4.0])
            .show(ui, |ui| {
                let head = |ui: &mut egui::Ui, s: &str| {
                    ui.label(RichText::new(s).color(theme::ORANGE).strong());
                };
                head(ui, "TIME");
                head(ui, "TYPE");
                head(ui, "SYMBOL");
                head(ui, "DIR");
                head(ui, "QTY");
                head(ui, "PRICE");
                head(ui, "AMOUNT");
                head(ui, "DETAIL");
                ui.end_row();

                // Build a merged feed of activities + closed orders, sorted
                // newest-first by timestamp. Closed orders with an existing
                // FILL activity for the same order id are filtered out to
                // avoid double-rows.
                let fill_ids: std::collections::HashSet<&str> = state
                    .activities
                    .iter()
                    .filter(|a| a.activity_type == "FILL" || a.activity_type == "PARTIAL_FILL")
                    .filter_map(|a| a.order_id.as_deref())
                    .collect();

                let mut rows: Vec<ActivityRow> = Vec::new();
                for a in &state.activities {
                    rows.push(ActivityRow::from_activity(a));
                }
                for o in &state.closed_orders {
                    if fill_ids.contains(o.id.as_str()) { continue; }
                    rows.push(ActivityRow::from_closed_order(o));
                }
                rows.sort_by(|a, b| b.sort_key.cmp(&a.sort_key));

                for r in rows {
                    ui.label(RichText::new(r.time).color(theme::WHITE));
                    ui.label(RichText::new(r.type_).color(r.type_color));
                    ui.label(RichText::new(r.symbol).color(theme::WHITE).strong());
                    ui.label(RichText::new(r.dir).color(r.dir_color));
                    ui.label(RichText::new(r.qty).color(theme::WHITE));
                    ui.label(RichText::new(r.price).color(theme::WHITE));
                    ui.label(RichText::new(r.amount).color(r.amount_color));
                    ui.label(RichText::new(r.detail).color(theme::GRAY2));
                    ui.end_row();
                }
            });
    });
}

/// Row of the activity table — both /account/activities and closed orders
/// flatten into this. `sort_key` is the UTC timestamp as a string (ISO
/// format sorts correctly lexicographically).
struct ActivityRow {
    time: String,
    type_: String,
    type_color: Color32,
    symbol: String,
    dir: String,
    dir_color: Color32,
    qty: String,
    price: String,
    amount: String,
    amount_color: Color32,
    detail: String,
    sort_key: String,
}

impl ActivityRow {
    fn from_activity(a: &Activity) -> Self {
        let (type_, type_color) = match a.activity_type.as_str() {
            "FILL" => ("FILL".to_string(), theme::GREEN),
            "PARTIAL_FILL" => ("PART FILL".to_string(), theme::YELLOW),
            t if t.contains("DIV") => (t.to_string(), theme::GREEN),
            "JNLC" | "JNLS" => ("JOURNAL".to_string(), theme::YELLOW),
            "CSD" | "CSW" => ("WITHDRAWAL".to_string(), theme::ORANGE),
            "ACATC" | "ACATS" => ("TRANSFER".to_string(), theme::CYAN),
            "FEE" => ("CHARGE".to_string(), theme::RED),
            other => (other.to_string(), theme::WHITE),
        };
        let dir = a.side.clone().unwrap_or_default().to_uppercase();
        let dir_color = match dir.as_str() {
            "BUY" => theme::CYAN,
            "SELL" => theme::RED,
            _ => theme::WHITE,
        };
        let amount = a.net_amount.clone().unwrap_or_default();
        let amount_val = amount.parse::<f64>().unwrap_or(0.0);
        let amount_color = if amount_val > 0.0 {
            theme::GREEN
        } else if amount_val < 0.0 {
            theme::RED
        } else {
            theme::WHITE
        };
        let time = a
            .transaction_time
            .map(|t| t.format("%Y-%m-%d %H:%M").to_string())
            .or_else(|| a.date.clone())
            .unwrap_or_default();
        let sort_key = a
            .transaction_time
            .map(|t| t.to_rfc3339())
            .or_else(|| a.date.clone())
            .unwrap_or_default();
        ActivityRow {
            time,
            type_,
            type_color,
            symbol: a.symbol.clone().unwrap_or_default(),
            dir,
            dir_color,
            qty: a.qty.clone().unwrap_or_default(),
            price: a.price.clone().map(|s| fmt_money_str(&s)).unwrap_or_default(),
            amount: if amount.is_empty() {
                String::new()
            } else {
                fmt_signed_money(amount_val)
            },
            amount_color,
            detail: a.description.clone().unwrap_or_default(),
            sort_key,
        }
    }

    fn from_closed_order(o: &Order) -> Self {
        let dir = o.side.to_uppercase();
        let dir_color = match dir.as_str() {
            "BUY" => theme::CYAN,
            "SELL" => theme::RED,
            _ => theme::WHITE,
        };
        let status_color = status_color(&o.status);
        ActivityRow {
            time: o.created_at.format("%Y-%m-%d %H:%M").to_string(),
            type_: o.status.to_uppercase(),
            type_color: status_color,
            symbol: o.symbol.clone(),
            dir,
            dir_color,
            qty: o.qty.clone(),
            price: fmt_money_str(o.filled_avg_price_str()),
            amount: String::new(),
            amount_color: theme::WHITE,
            detail: format!("{} {}", o.order_type.to_uppercase(), short_id(&o.id)),
            sort_key: o.created_at.to_rfc3339(),
        }
    }
}

// ----------------------------------------------------------------------------
//  Formatting helpers
// ----------------------------------------------------------------------------

fn fmt_money_str(s: &str) -> String {
    if s.is_empty() {
        return "—".to_string();
    }
    match s.parse::<f64>() {
        Ok(v) => format!("{:>12.2}", v),
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

fn short_id(s: &str) -> String {
    s.chars().take(8).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base_form() -> TradeForm {
        let mut f = TradeForm::default();
        f.symbol_input = "AAPL".into();
        f.qty_input = "10".into();
        f
    }

    #[test]
    fn market_order_serializes_with_no_optional_fields() {
        let (req, sum) = build_order(&base_form()).unwrap();
        assert_eq!(req.order_type, "market");
        assert!(req.stop_price.is_none());
        assert!(req.trail_percent.is_none());
        assert!(req.order_class.is_none());
        assert!(sum.contains("MARKET"));
        let json = serde_json::to_string(&req).unwrap();
        assert!(!json.contains("stop_price"));
        assert!(!json.contains("order_class"));
    }

    #[test]
    fn limit_order_requires_limit_price() {
        let mut f = base_form();
        f.kind = OrderKind::Limit;
        assert!(build_order(&f).is_none()); // empty limit
        f.limit_input = "187.5".into();
        let (req, _) = build_order(&f).unwrap();
        assert_eq!(req.order_type, "limit");
        assert_eq!(req.limit_price, "187.5");
    }

    #[test]
    fn stop_order_emits_stop_price() {
        let mut f = base_form();
        f.kind = OrderKind::Stop;
        f.stop_input = "180.0".into();
        let (req, _) = build_order(&f).unwrap();
        assert_eq!(req.order_type, "stop");
        assert_eq!(req.stop_price.as_deref(), Some("180.0"));
        assert!(req.limit_price.is_empty());
    }

    #[test]
    fn stop_limit_requires_both_prices() {
        let mut f = base_form();
        f.kind = OrderKind::StopLimit;
        f.stop_input = "180".into();
        // limit missing
        assert!(build_order(&f).is_none());
        f.limit_input = "179".into();
        let (req, _) = build_order(&f).unwrap();
        assert_eq!(req.order_type, "stop_limit");
        assert_eq!(req.stop_price.as_deref(), Some("180"));
        assert_eq!(req.limit_price, "179");
    }

    #[test]
    fn trailing_stop_emits_trail_percent() {
        let mut f = base_form();
        f.kind = OrderKind::TrailingStop;
        f.trail_pct_input = "5".into();
        let (req, sum) = build_order(&f).unwrap();
        assert_eq!(req.order_type, "trailing_stop");
        assert_eq!(req.trail_percent.as_deref(), Some("5"));
        assert!(sum.contains("trail"));
    }

    #[test]
    fn bracket_requires_both_legs() {
        let mut f = base_form();
        f.bracket = true;
        f.kind = OrderKind::Limit;
        f.limit_input = "187".into();
        // TP/SL empty -> reject
        assert!(build_order(&f).is_none());
        f.tp_input = "200".into();
        assert!(build_order(&f).is_none()); // SL still missing
        f.sl_stop_input = "175".into();
        let (req, sum) = build_order(&f).unwrap();
        assert_eq!(req.order_class.as_deref(), Some("bracket"));
        assert_eq!(req.take_profit.as_ref().unwrap().limit_price, "200");
        assert_eq!(req.stop_loss.as_ref().unwrap().stop_price, "175");
        assert!(req.stop_loss.as_ref().unwrap().limit_price.is_none());
        assert!(sum.contains("TP 200"));
        assert!(sum.contains("SL 175"));
    }

    #[test]
    fn bracket_with_sl_limit_serializes_both() {
        let mut f = base_form();
        f.bracket = true;
        f.tp_input = "200".into();
        f.sl_stop_input = "175".into();
        f.sl_limit_input = "174".into();
        let (req, _) = build_order(&f).unwrap();
        let sl = req.stop_loss.as_ref().unwrap();
        assert_eq!(sl.stop_price, "175");
        assert_eq!(sl.limit_price.as_deref(), Some("174"));
    }

    fn make_activity(
        kind: &str,
        symbol: &str,
        side: &str,
        price: &str,
        qty: &str,
        order_id: &str,
    ) -> crate::api::Activity {
        crate::api::Activity {
            id: format!("a-{order_id}"),
            activity_type: kind.to_string(),
            transaction_time: Some(chrono::Utc::now()),
            date: None,
            fill_type: Some("market".to_string()),
            price: Some(price.to_string()),
            qty: Some(qty.to_string()),
            cum_qty: None,
            side: Some(side.to_string()),
            symbol: Some(symbol.to_string()),
            order_id: Some(order_id.to_string()),
            net_amount: None,
            per_share_amount: None,
            description: None,
        }
    }

    fn make_closed_order(
        id: &str,
        symbol: &str,
        side: &str,
        status: &str,
        avg: &str,
        qty: &str,
    ) -> crate::api::Order {
        crate::api::Order {
            id: id.to_string(),
            symbol: symbol.to_string(),
            side: side.to_string(),
            order_type: "market".to_string(),
            qty: qty.to_string(),
            limit_price: None,
            status: status.to_string(),
            filled_qty: qty.to_string(),
            filled_avg_price: Some(avg.to_string()),
            created_at: chrono::Utc::now(),
            asset_class: "us_equity".to_string(),
        }
    }

    #[test]
    fn fills_for_symbol_pulls_from_activity_first() {
        let mut state = TerminalState::new();
        state.activities = vec![
            make_activity("FILL", "AAPL", "buy", "150.0", "10", "ord-1"),
            make_activity("FILL", "MSFT", "sell", "420.0", "5", "ord-2"),
        ];
        let aapl = state.fills_for_symbol("AAPL");
        assert_eq!(aapl.len(), 1);
        assert_eq!(aapl[0].price, 150.0);
        assert_eq!(aapl[0].side, FillSide::Buy);
        assert_eq!(state.fills_for_symbol("MSFT").len(), 1);
        assert_eq!(state.fills_for_symbol("NVDA").len(), 0);
    }

    #[test]
    fn fills_for_symbol_dedups_closed_orders_already_in_activity() {
        let mut state = TerminalState::new();
        state.activities = vec![make_activity(
            "FILL", "AAPL", "buy", "150.0", "10", "shared-id",
        )];
        state.closed_orders = vec![make_closed_order(
            "shared-id", "AAPL", "buy", "filled", "150.0", "10",
        )];
        // Only one row even though the same order appears in both feeds.
        assert_eq!(state.fills_for_symbol("AAPL").len(), 1);
    }

    #[test]
    fn fills_for_symbol_falls_back_to_closed_order_when_no_activity() {
        let mut state = TerminalState::new();
        state.closed_orders = vec![make_closed_order(
            "only-here", "TSLA", "sell", "filled", "180.0", "3",
        )];
        let fills = state.fills_for_symbol("TSLA");
        assert_eq!(fills.len(), 1);
        assert_eq!(fills[0].side, FillSide::Sell);
        assert_eq!(fills[0].price, 180.0);
    }

    #[test]
    fn fills_ignores_canceled_or_rejected_closed_orders() {
        let mut state = TerminalState::new();
        state.closed_orders = vec![
            make_closed_order("c1", "AAPL", "buy", "canceled", "0", "0"),
            make_closed_order("r1", "AAPL", "buy", "rejected", "0", "0"),
        ];
        assert_eq!(state.fills_for_symbol("AAPL").len(), 0);
    }

    #[test]
    fn non_numeric_inputs_are_rejected() {
        let mut f = base_form();
        f.qty_input = "abc".into();
        assert!(build_order(&f).is_none());
        let mut f = base_form();
        f.kind = OrderKind::Stop;
        f.stop_input = "abc".into();
        assert!(build_order(&f).is_none());
    }
}
