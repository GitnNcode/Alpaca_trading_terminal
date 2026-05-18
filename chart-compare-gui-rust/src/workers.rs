// Background HTTP workers for the chart-only egui app. We only need two
// commands: load the asset list (for autocomplete) and load bars for a
// symbol/range/timeframe. Each runs on its own thread, sends a Msg back via
// mpsc, and wakes the egui UI via ctx.request_repaint().

use std::sync::mpsc::Sender;
use std::sync::Arc;
use std::thread;

use chrono::{Datelike, Duration as ChronoDuration, TimeZone, Utc};

use crate::api::{AlpacaClient, Bar};

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
