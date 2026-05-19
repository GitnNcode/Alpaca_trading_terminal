use std::sync::mpsc::Sender;
use std::sync::Arc;
use std::thread;

use chrono::{Duration as ChronoDuration, Utc};

use crate::api::{AlpacaClient, OrderRequest};
use crate::app::{chart_start_time, Msg, CHART_RANGES, CHART_TFS};
use crate::fmp::FmpClient;
use crate::llm::ClaudeClient;

pub fn spawn_refresh(client: Arc<AlpacaClient>, tx: Sender<Msg>) {
    thread::spawn(move || {
        let c1 = client.clone();
        let tx1 = tx.clone();
        thread::spawn(move || {
            let positions = c1.get_positions();
            let _ = tx1.send(Msg::Positions(positions));
        });

        let c2 = client.clone();
        let tx2 = tx.clone();
        thread::spawn(move || {
            let account = c2.get_account();
            let _ = tx2.send(Msg::Account(account));
        });

        let c3 = client.clone();
        let tx3 = tx.clone();
        thread::spawn(move || {
            let orders = c3.get_orders();
            let _ = tx3.send(Msg::Orders(orders));
        });

        let c4 = client.clone();
        let tx4 = tx.clone();
        thread::spawn(move || {
            let activities = c4.get_activities();
            let closed = c4.get_closed_orders();
            let _ = tx4.send(Msg::Activities(activities, closed));
        });
    });
}

pub fn spawn_assets(client: Arc<AlpacaClient>, tx: Sender<Msg>) {
    thread::spawn(move || {
        let assets = client.get_assets();
        let _ = tx.send(Msg::Assets(assets));
    });
}

pub fn spawn_place_order(client: Arc<AlpacaClient>, tx: Sender<Msg>, req: OrderRequest) {
    thread::spawn(move || {
        let result = client.place_order(&req);
        let _ = tx.send(Msg::OrderPlaced(result, req));
    });
}

pub fn spawn_cancel_order(client: Arc<AlpacaClient>, tx: Sender<Msg>, order_id: String) {
    let id_clone = order_id.clone();
    thread::spawn(move || {
        let result = client.cancel_order(&order_id);
        let _ = tx.send(Msg::OrderCanceled(result, id_clone));
    });
}

pub fn spawn_load_chart(
    client: Arc<AlpacaClient>,
    tx: Sender<Msg>,
    symbol: String,
    range_idx: usize,
    tf_idx: usize,
) {
    thread::spawn(move || {
        if range_idx >= CHART_RANGES.len() || tf_idx >= CHART_TFS.len() {
            return;
        }
        let rg = &CHART_RANGES[range_idx];
        let tf = &CHART_TFS[tf_idx];
        let now = Utc::now();
        let end = now - ChronoDuration::minutes(2);
        let start = chart_start_time(rg, now);
        let bars = client.get_bars(&symbol, tf.value, start, end);
        let _ = tx.send(Msg::Bars {
            symbol,
            range_idx,
            tf_idx,
            bars,
        });
    });
}

pub fn spawn_fetch_supplychain_fmp(client: Arc<FmpClient>, tx: Sender<Msg>, symbol: String) {
    thread::spawn(move || {
        let result = client.fetch_supply_chain(&symbol);
        let _ = tx.send(Msg::SupplyChainFmp(symbol, result));
    });
}

pub fn spawn_fetch_supplychain_claude(client: Arc<ClaudeClient>, tx: Sender<Msg>, symbol: String) {
    thread::spawn(move || {
        let result = client.fetch_supply_chain(&symbol);
        let _ = tx.send(Msg::SupplyChainClaude(symbol, result));
    });
}
