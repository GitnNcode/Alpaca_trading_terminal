// Background HTTP workers for the chart-only egui app. We only need two
// commands: load the asset list (for autocomplete) and load bars for a
// symbol/range/timeframe. Each runs on its own thread, sends a Msg back via
// mpsc, and wakes the egui UI via ctx.request_repaint().

use std::sync::mpsc::Sender;
use std::sync::Arc;
use std::thread;

use chrono::{Datelike, Duration as ChronoDuration, TimeZone, Utc};

use crate::api::{Account, Activity, AlpacaClient, Bar, Order, OrderRequest, Position};

pub enum Msg {
    Assets(anyhow::Result<Vec<crate::api::Asset>>),
    Bars {
        symbol: String,
        range_idx: usize,
        tf_idx: usize,
        gen: u64,
        bars: anyhow::Result<Vec<Bar>>,
    },
    /// Bars for a Compare-tab slot. Per-slot `gen` lets the receiver discard
    /// stale responses (e.g. when the user changes range while a load is
    /// still in flight). `range_idx` is carried so the receiver can also
    /// drop responses for a range that's no longer active.
    CompareBars {
        symbol: String,
        range_idx: usize,
        gen: u64,
        bars: anyhow::Result<Vec<Bar>>,
    },
    // ---- Terminal tab fetches ----
    Positions(anyhow::Result<Vec<Position>>),
    AccountInfo(anyhow::Result<Account>),
    OpenOrders(anyhow::Result<Vec<Order>>),
    ClosedOrders(anyhow::Result<Vec<Order>>),
    Activities(anyhow::Result<Vec<Activity>>),
    /// Order placement result. Carries back the request snapshot so the
    /// Trade view can show a confirmation referencing what was just sent.
    OrderPlaced {
        req_summary: String,
        result: anyhow::Result<Order>,
    },
    /// Order cancellation result. Carries the order id so the Orders view
    /// can clear "cancelling…" state for the right row.
    OrderCancelled {
        id: String,
        result: anyhow::Result<()>,
    },
    /// Bars for the Trade sub-tab's inline preview chart. Separate variant
    /// (vs reusing `Bars`) because the Chart-tab receiver discards anything
    /// whose symbol/range/tf doesn't match its current state — and the
    /// Trade chart has its own symbol that's independent of the main Chart
    /// tab.
    TradeChartBars {
        symbol: String,
        gen: u64,
        bars: anyhow::Result<Vec<Bar>>,
    },
    /// Connection / latency state of the live data WebSocket. Tick data
    /// itself does NOT travel through this channel — it lands directly in
    /// the shared `TickCache` to avoid 1000 msgs/sec of `mpsc` pressure on
    /// the main loop. This variant is just the connect/disconnect signal.
    StreamStatus {
        connected: bool,
        // Reported by the stream thread for the planned status-bar telemetry
        // (CLAUDE.md tier-3 status bar). Surfaced only when that lands.
        #[allow(dead_code)]
        latency_ms: Option<u32>,
    },
}

pub fn spawn_assets(client: Arc<AlpacaClient>, tx: Sender<Msg>, ctx: egui::Context) {
    thread::spawn(move || {
        let _ = tx.send(Msg::Assets(client.get_assets()));
        ctx.request_repaint();
    });
}

#[allow(clippy::too_many_arguments)]
pub fn spawn_load_bars(
    client: Arc<AlpacaClient>,
    tx: Sender<Msg>,
    ctx: egui::Context,
    symbol: String,
    timeframe: &'static str,
    range_idx: usize,
    tf_idx: usize,
    lookback_hours: i64,
    ytd: bool,
    gen: u64,
) {
    thread::spawn(move || {
        let now = Utc::now();
        let end = now - ChronoDuration::minutes(2);
        let start = if ytd {
            Utc.with_ymd_and_hms(now.year(), 1, 1, 0, 0, 0).unwrap()
        } else {
            now - ChronoDuration::hours(lookback_hours)
        };
        let bars = client.get_bars(&symbol, timeframe, start, end);
        let _ = tx.send(Msg::Bars {
            symbol,
            range_idx,
            tf_idx,
            gen,
            bars,
        });
        ctx.request_repaint();
    });
}

/// Compare always uses 1Day bars — the canonical step for risk metrics like
/// CAGR / vol / Sharpe / drawdowns. Intraday math here would just add noise.
#[allow(clippy::too_many_arguments)]
pub fn spawn_load_compare_bars(
    client: Arc<AlpacaClient>,
    tx: Sender<Msg>,
    ctx: egui::Context,
    symbol: String,
    range_idx: usize,
    lookback_hours: i64,
    gen: u64,
) {
    thread::spawn(move || {
        let now = Utc::now();
        let end = now - ChronoDuration::minutes(2);
        let start = now - ChronoDuration::hours(lookback_hours);
        let bars = client.get_bars(&symbol, "1Day", start, end);
        let _ = tx.send(Msg::CompareBars {
            symbol,
            range_idx,
            gen,
            bars,
        });
        ctx.request_repaint();
    });
}

// ---------------- Terminal tab spawn helpers ----------------

pub fn spawn_positions(client: Arc<AlpacaClient>, tx: Sender<Msg>, ctx: egui::Context) {
    thread::spawn(move || {
        let _ = tx.send(Msg::Positions(client.get_positions()));
        ctx.request_repaint();
    });
}

pub fn spawn_account(client: Arc<AlpacaClient>, tx: Sender<Msg>, ctx: egui::Context) {
    thread::spawn(move || {
        let _ = tx.send(Msg::AccountInfo(client.get_account()));
        ctx.request_repaint();
    });
}

pub fn spawn_open_orders(client: Arc<AlpacaClient>, tx: Sender<Msg>, ctx: egui::Context) {
    thread::spawn(move || {
        let _ = tx.send(Msg::OpenOrders(client.get_orders()));
        ctx.request_repaint();
    });
}

pub fn spawn_closed_orders(client: Arc<AlpacaClient>, tx: Sender<Msg>, ctx: egui::Context) {
    thread::spawn(move || {
        let _ = tx.send(Msg::ClosedOrders(client.get_closed_orders()));
        ctx.request_repaint();
    });
}

pub fn spawn_activities(client: Arc<AlpacaClient>, tx: Sender<Msg>, ctx: egui::Context) {
    thread::spawn(move || {
        let _ = tx.send(Msg::Activities(client.get_activities()));
        ctx.request_repaint();
    });
}

/// Place an order on a background thread. `req_summary` is the user-visible
/// "BUY 10 AAPL @ MKT" string we computed before dispatch — carried back on
/// the response so the Trade view can show what was placed without having
/// to rebuild it from the OrderRequest fields.
pub fn spawn_place_order(
    client: Arc<AlpacaClient>,
    tx: Sender<Msg>,
    ctx: egui::Context,
    req: OrderRequest,
    req_summary: String,
) {
    thread::spawn(move || {
        let result = client.place_order(&req);
        let _ = tx.send(Msg::OrderPlaced { req_summary, result });
        ctx.request_repaint();
    });
}

pub fn spawn_cancel_order(
    client: Arc<AlpacaClient>,
    tx: Sender<Msg>,
    ctx: egui::Context,
    id: String,
) {
    thread::spawn(move || {
        let result = client.cancel_order(&id);
        let _ = tx.send(Msg::OrderCancelled { id, result });
        ctx.request_repaint();
    });
}

/// Fetch ~3 months of daily bars to drive the Trade sub-tab's preview chart.
/// Daily bars are right for a "what does this stock look like" glance and
/// keep the request cheap.
pub fn spawn_load_trade_chart(
    client: Arc<AlpacaClient>,
    tx: Sender<Msg>,
    ctx: egui::Context,
    symbol: String,
    gen: u64,
) {
    thread::spawn(move || {
        let now = Utc::now();
        let end = now - ChronoDuration::minutes(2);
        let start = now - ChronoDuration::days(95);
        let bars = client.get_bars(&symbol, "1Day", start, end);
        let _ = tx.send(Msg::TradeChartBars { symbol, gen, bars });
        ctx.request_repaint();
    });
}
