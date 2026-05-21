// Alpaca Market Data v2 WebSocket client.
//
// One thread holds the socket. On the inbound side, it auths, manages the
// active subscription set (trades + quotes + minute bars), and dispatches
// every received frame into:
//   1) the shared `TickCache` (Arc<RwLock<HashMap<symbol, LastTick>>>) so UI
//      surfaces can look up the most recent price for any symbol without
//      going through a channel hop or copying every tick, and
//   2) a `Msg::StreamStatus` on the existing app channel for connection /
//      latency state.
// On every tick it also calls `ctx.request_repaint()` so the UI wakes up.
//
// On disconnect: exponential backoff up to 30s, then re-auth + re-subscribe
// to the current set. The thread never panics out; even malformed frames
// just get logged to stderr and skipped.
//
// Subscription set: the UI sends the *full* desired set on every change via
// the `SubMsg` channel; the stream thread diffs it against what it last
// subscribed to and sends `subscribe`/`unsubscribe` frames accordingly.

use std::collections::HashSet;
use std::io;
use std::net::TcpStream;
use std::sync::mpsc::{Receiver, Sender, TryRecvError};
use std::sync::{Arc, RwLock};
use std::thread;
use std::time::Duration;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tungstenite::stream::MaybeTlsStream;
use tungstenite::{connect, Message, WebSocket};

use crate::api::AlpacaClient;
use crate::workers::Msg;

/// Default IEX endpoint — the only feed available on the free tier. Paid
/// tiers can swap for `/sip` or `/test` via env override.
const STREAM_URL: &str = "wss://stream.data.alpaca.markets/v2/iex";

/// Minimum backoff between reconnect attempts. Doubles up to MAX_BACKOFF.
const MIN_BACKOFF: Duration = Duration::from_secs(1);
const MAX_BACKOFF: Duration = Duration::from_secs(30);

/// How often the read loop wakes to poll the `SubMsg` channel. Short enough
/// that subscription changes feel instant; long enough that we're not
/// burning CPU when ticks are sparse.
const READ_TIMEOUT: Duration = Duration::from_millis(50);

/// Last seen market data for a single symbol. The stream thread writes;
/// the UI reads. Timestamps come from Alpaca and are wall-clock UTC.
#[derive(Debug, Clone, Default)]
pub struct LastTick {
    pub last_price: Option<f64>,
    pub last_size: Option<u64>,
    pub bid: Option<f64>,
    pub bid_size: Option<u64>,
    pub ask: Option<f64>,
    pub ask_size: Option<u64>,
    /// Most recent minute bar's OHLCV. Used by the Chart tab to morph the
    /// rightmost candle on intraday timeframes.
    pub last_bar: Option<MinuteBar>,
    /// Wall-clock time of the most recent update of any kind.
    pub updated_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Copy)]
pub struct MinuteBar {
    pub t: DateTime<Utc>,
    pub o: f64,
    pub h: f64,
    pub l: f64,
    pub c: f64,
    pub v: f64,
}

/// Lock-free reads from the UI side: `tick_cache.read().unwrap().get(sym)`.
/// The write side holds the lock only long enough to update a single entry.
pub type TickCache = Arc<RwLock<std::collections::HashMap<String, LastTick>>>;

pub fn new_tick_cache() -> TickCache {
    Arc::new(RwLock::new(std::collections::HashMap::new()))
}

/// Messages from the UI to the stream thread. Currently just one kind —
/// "this is the full set we want subscribed to right now." The thread
/// diffs it against what's actually subscribed and emits the right frames.
#[derive(Debug, Clone)]
pub enum SubMsg {
    SetSubscriptions(HashSet<String>),
    /// Graceful shutdown — the thread closes the socket and exits. Reserved
    /// for the future "quit cleanly on app exit" path; nothing sends it yet,
    /// so silence dead-code for the variant rather than removing the API
    /// surface the stream loop already accepts.
    #[allow(dead_code)]
    Shutdown,
}

/// Spawn the long-lived stream thread. Returns the inbound channel sender
/// (kept on ChartApp; used to push subscription changes) — and *nothing
/// else*, because the tick stream lands in `cache` and status lands on the
/// existing app `Msg` channel.
pub fn spawn_stream(
    client: Arc<AlpacaClient>,
    tx: Sender<Msg>,
    ctx: egui::Context,
    cache: TickCache,
) -> Sender<SubMsg> {
    let (sub_tx, sub_rx) = std::sync::mpsc::channel();
    thread::spawn(move || {
        run_loop(client, tx, ctx, cache, sub_rx);
    });
    sub_tx
}

fn run_loop(
    client: Arc<AlpacaClient>,
    tx: Sender<Msg>,
    ctx: egui::Context,
    cache: TickCache,
    sub_rx: Receiver<SubMsg>,
) {
    let mut backoff = MIN_BACKOFF;
    // The most recent desired subscription set, regardless of connection
    // state. Survives disconnects so we re-subscribe to the right symbols
    // on reconnect.
    let mut desired: HashSet<String> = HashSet::new();

    loop {
        emit_status(&tx, &ctx, false, None);
        match connect_and_serve(&client, &tx, &ctx, &cache, &sub_rx, &mut desired) {
            Ok(ShutdownReason::Requested) => break,
            Ok(ShutdownReason::Disconnected) | Err(_) => {
                // Drain any subscription updates that arrived during the
                // disconnected window so we re-subscribe with the latest.
                while let Ok(msg) = sub_rx.try_recv() {
                    match msg {
                        SubMsg::SetSubscriptions(set) => desired = set,
                        SubMsg::Shutdown => return,
                    }
                }
                thread::sleep(backoff);
                backoff = (backoff * 2).min(MAX_BACKOFF);
                continue;
            }
        }
    }
}

enum ShutdownReason {
    Requested,
    Disconnected,
}

/// One full lifecycle of the socket: connect → auth → subscribe → read loop.
/// Returns `Disconnected` on any transport error so the outer loop can
/// reconnect with backoff. Returns `Requested` only when the UI explicitly
/// asks the thread to stop.
fn connect_and_serve(
    client: &AlpacaClient,
    tx: &Sender<Msg>,
    ctx: &egui::Context,
    cache: &TickCache,
    sub_rx: &Receiver<SubMsg>,
    desired: &mut HashSet<String>,
) -> Result<ShutdownReason, anyhow::Error> {
    let (mut socket, _resp) = connect(STREAM_URL)?;
    set_read_timeout(&mut socket, READ_TIMEOUT)?;

    // Alpaca sends a `[{"T":"success","msg":"connected"}]` greeting
    // immediately. Don't strictly need to wait for it — the next frame we
    // care about is the auth response.
    let auth = serde_json::json!({
        "action": "auth",
        "key": client.api_key,
        "secret": client.api_secret,
    });
    socket.send(Message::Text(auth.to_string()))?;

    // The desired set may already be populated (a prior connection's). Send
    // a fresh subscribe so the new socket gets it.
    let mut subscribed: HashSet<String> = HashSet::new();
    sync_subscriptions(&mut socket, &mut subscribed, desired)?;

    emit_status(tx, ctx, true, None);

    loop {
        // 1) Poll the sub channel — non-blocking.
        loop {
            match sub_rx.try_recv() {
                Ok(SubMsg::SetSubscriptions(new_set)) => {
                    *desired = new_set;
                    sync_subscriptions(&mut socket, &mut subscribed, desired)?;
                }
                Ok(SubMsg::Shutdown) => {
                    let _ = socket.close(None);
                    return Ok(ShutdownReason::Requested);
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => return Ok(ShutdownReason::Requested),
            }
        }

        // 2) Read a frame (with read-timeout so we loop back to step 1).
        match socket.read() {
            Ok(Message::Text(payload)) => {
                if let Err(e) = handle_payload(&payload, cache, ctx) {
                    eprintln!("[stream] frame parse failed: {e}");
                }
            }
            Ok(Message::Binary(_)) => { /* ignore */ }
            Ok(Message::Ping(p)) => {
                let _ = socket.send(Message::Pong(p));
            }
            Ok(Message::Pong(_)) => { /* ignore */ }
            Ok(Message::Close(_)) => return Ok(ShutdownReason::Disconnected),
            Ok(Message::Frame(_)) => { /* low-level frame; ignore */ }
            Err(tungstenite::Error::Io(ref e))
                if e.kind() == io::ErrorKind::WouldBlock
                    || e.kind() == io::ErrorKind::TimedOut =>
            {
                // No data this tick — fine, loop back.
                continue;
            }
            Err(e) => {
                eprintln!("[stream] socket read error: {e}");
                return Ok(ShutdownReason::Disconnected);
            }
        }
    }
}

/// Set a read timeout on the TCP stream underneath the TLS layer so reads
/// return WouldBlock periodically — letting us poll the sub channel.
fn set_read_timeout(
    socket: &mut WebSocket<MaybeTlsStream<TcpStream>>,
    timeout: Duration,
) -> io::Result<()> {
    let stream = socket.get_mut();
    match stream {
        MaybeTlsStream::Plain(t) => t.set_read_timeout(Some(timeout)),
        MaybeTlsStream::NativeTls(t) => t.get_mut().set_read_timeout(Some(timeout)),
        _ => Ok(()),
    }
}

/// Diff `desired` against `subscribed`, send subscribe / unsubscribe frames
/// for the symbols that changed, then update `subscribed`.
fn sync_subscriptions(
    socket: &mut WebSocket<MaybeTlsStream<TcpStream>>,
    subscribed: &mut HashSet<String>,
    desired: &HashSet<String>,
) -> Result<(), anyhow::Error> {
    let add: Vec<&String> = desired.difference(subscribed).collect();
    let drop: Vec<&String> = subscribed.difference(desired).collect();

    if !drop.is_empty() {
        let frame = serde_json::json!({
            "action": "unsubscribe",
            "trades": drop, "quotes": drop, "bars": drop,
        });
        socket.send(Message::Text(frame.to_string()))?;
    }
    if !add.is_empty() {
        let frame = serde_json::json!({
            "action": "subscribe",
            "trades": add, "quotes": add, "bars": add,
        });
        socket.send(Message::Text(frame.to_string()))?;
    }
    *subscribed = desired.clone();
    Ok(())
}

/// Parse a server payload (always a JSON array of one or more event objects)
/// and update the cache. Ticks request a repaint; control frames (success /
/// subscription / error) just log.
fn handle_payload(
    payload: &str,
    cache: &TickCache,
    ctx: &egui::Context,
) -> Result<(), serde_json::Error> {
    let events: Vec<Value> = serde_json::from_str(payload)?;
    let mut any_tick = false;
    for ev in events {
        let kind = ev.get("T").and_then(|v| v.as_str()).unwrap_or("");
        match kind {
            "t" => {
                let frame: TradeFrame = serde_json::from_value(ev)?;
                let sym = frame.symbol.clone();
                if let Ok(mut w) = cache.write() {
                    let slot = w.entry(sym).or_default();
                    slot.last_price = Some(frame.price);
                    slot.last_size = Some(frame.size);
                    slot.updated_at = Some(frame.timestamp);
                }
                any_tick = true;
            }
            "q" => {
                let frame: QuoteFrame = serde_json::from_value(ev)?;
                let sym = frame.symbol.clone();
                if let Ok(mut w) = cache.write() {
                    let slot = w.entry(sym).or_default();
                    slot.bid = Some(frame.bid_price);
                    slot.bid_size = Some(frame.bid_size);
                    slot.ask = Some(frame.ask_price);
                    slot.ask_size = Some(frame.ask_size);
                    slot.updated_at = Some(frame.timestamp);
                }
                any_tick = true;
            }
            "b" => {
                let frame: BarFrame = serde_json::from_value(ev)?;
                let sym = frame.symbol.clone();
                if let Ok(mut w) = cache.write() {
                    let slot = w.entry(sym).or_default();
                    slot.last_bar = Some(MinuteBar {
                        t: frame.timestamp,
                        o: frame.open,
                        h: frame.high,
                        l: frame.low,
                        c: frame.close,
                        v: frame.volume,
                    });
                    // Bars also imply a last price — keep them in sync so a
                    // bar-only consumer (e.g. Chart on 1Day with sparse
                    // trades) still sees a "last".
                    slot.last_price = Some(frame.close);
                    slot.updated_at = Some(frame.timestamp);
                }
                any_tick = true;
            }
            "subscription" => {
                // Server confirmation of the current subscription state.
                // Useful for debugging; nothing to do.
            }
            "success" | "error" => {
                if let Some(msg) = ev.get("msg").and_then(|v| v.as_str()) {
                    eprintln!("[stream] {kind}: {msg}");
                }
            }
            _ => { /* unknown event kind — ignore forward-compat */ }
        }
    }
    if any_tick {
        ctx.request_repaint();
    }
    Ok(())
}

fn emit_status(tx: &Sender<Msg>, ctx: &egui::Context, connected: bool, latency_ms: Option<u32>) {
    let _ = tx.send(Msg::StreamStatus { connected, latency_ms });
    ctx.request_repaint();
}

// ---------------- Wire frames ----------------
//
// Alpaca's WS payloads are JSON arrays of event objects, each with a "T"
// discriminator. We only deserialize the fields we use.

#[derive(Debug, Deserialize, Serialize)]
struct TradeFrame {
    #[serde(rename = "S")]
    symbol: String,
    #[serde(rename = "p")]
    price: f64,
    #[serde(rename = "s", default)]
    size: u64,
    #[serde(rename = "t")]
    timestamp: DateTime<Utc>,
}

#[derive(Debug, Deserialize, Serialize)]
struct QuoteFrame {
    #[serde(rename = "S")]
    symbol: String,
    #[serde(rename = "bp")]
    bid_price: f64,
    #[serde(rename = "bs", default)]
    bid_size: u64,
    #[serde(rename = "ap")]
    ask_price: f64,
    #[serde(rename = "as", default)]
    ask_size: u64,
    #[serde(rename = "t")]
    timestamp: DateTime<Utc>,
}

#[derive(Debug, Deserialize, Serialize)]
struct BarFrame {
    #[serde(rename = "S")]
    symbol: String,
    #[serde(rename = "o")]
    open: f64,
    #[serde(rename = "h")]
    high: f64,
    #[serde(rename = "l")]
    low: f64,
    #[serde(rename = "c")]
    close: f64,
    #[serde(rename = "v")]
    volume: f64,
    #[serde(rename = "t")]
    timestamp: DateTime<Utc>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_trade_frame_into_cache() {
        let cache = new_tick_cache();
        let ctx = egui::Context::default();
        let payload = r#"[{"T":"t","S":"AAPL","i":1,"x":"V","p":225.43,"s":7,"t":"2024-09-12T13:30:00Z"}]"#;
        handle_payload(payload, &cache, &ctx).unwrap();
        let slot = cache.read().unwrap().get("AAPL").cloned().unwrap();
        assert_eq!(slot.last_price, Some(225.43));
        assert_eq!(slot.last_size, Some(7));
    }

    #[test]
    fn parses_quote_frame_into_cache() {
        let cache = new_tick_cache();
        let ctx = egui::Context::default();
        let payload = r#"[{"T":"q","S":"AAPL","bp":225.40,"bs":2,"ap":225.45,"as":3,"t":"2024-09-12T13:30:00Z"}]"#;
        handle_payload(payload, &cache, &ctx).unwrap();
        let slot = cache.read().unwrap().get("AAPL").cloned().unwrap();
        assert_eq!(slot.bid, Some(225.40));
        assert_eq!(slot.ask, Some(225.45));
        assert_eq!(slot.bid_size, Some(2));
        assert_eq!(slot.ask_size, Some(3));
    }

    #[test]
    fn parses_bar_frame_and_mirrors_close_to_last_price() {
        let cache = new_tick_cache();
        let ctx = egui::Context::default();
        let payload = r#"[{"T":"b","S":"AAPL","o":225.0,"h":226.0,"l":224.5,"c":225.7,"v":1234,"t":"2024-09-12T13:30:00Z"}]"#;
        handle_payload(payload, &cache, &ctx).unwrap();
        let slot = cache.read().unwrap().get("AAPL").cloned().unwrap();
        let bar = slot.last_bar.unwrap();
        assert_eq!(bar.c, 225.7);
        assert_eq!(slot.last_price, Some(225.7));
    }

    #[test]
    fn batched_frames_all_land_in_cache() {
        let cache = new_tick_cache();
        let ctx = egui::Context::default();
        let payload = r#"[
            {"T":"t","S":"AAPL","p":100.0,"s":1,"t":"2024-09-12T13:30:00Z"},
            {"T":"t","S":"MSFT","p":420.5,"s":2,"t":"2024-09-12T13:30:00Z"}
        ]"#;
        handle_payload(payload, &cache, &ctx).unwrap();
        assert_eq!(cache.read().unwrap().get("AAPL").unwrap().last_price, Some(100.0));
        assert_eq!(cache.read().unwrap().get("MSFT").unwrap().last_price, Some(420.5));
    }

    #[test]
    fn control_frames_do_not_panic() {
        let cache = new_tick_cache();
        let ctx = egui::Context::default();
        let payload = r#"[{"T":"success","msg":"authenticated"}]"#;
        handle_payload(payload, &cache, &ctx).unwrap();
        assert!(cache.read().unwrap().is_empty());
    }
}
