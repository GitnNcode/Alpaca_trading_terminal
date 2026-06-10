use anyhow::{anyhow, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::Duration;

use crate::config::Credentials;

pub const ALPACA_DATA_BASE: &str = "https://data.alpaca.markets";

/// Options market-data feed. `indicative` is the free/paper feed (the only one
/// available without signing the OPRA agreement). One swap-point — flip to
/// `opra` here if/when the account gains the real-time entitlement (which also
/// unlocks greeks/IV on the snapshot). See docs/adr/0001.
pub const OPTIONS_DATA_FEED: &str = "indicative";

#[derive(Clone)]
pub struct AlpacaClient {
    pub base_url: String,
    pub api_key: String,
    pub api_secret: String,
    pub agent: ureq::Agent,
}

// Only the wire fields we actually use are listed; serde drops the rest of
// the /v2/positions payload (market_value / unrealized_pl / unrealized_plpc).
// The positions table recomputes those live from the tick cache every frame,
// so the broker-snapshot copies were unused.
#[derive(Debug, Clone, Deserialize)]
pub struct Position {
    pub symbol: String,
    pub qty: String,
    pub avg_entry_price: String,
    pub current_price: String,
    pub side: String,
    /// `us_equity` or `us_option`. The Options desk filters on this to show
    /// only option positions; the Terminal tab leaves it unfiltered. Defaulted
    /// so older payloads (or feeds that omit it) still parse.
    #[serde(default)]
    pub asset_class: String,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[allow(dead_code)]
pub struct Account {
    #[serde(default)]
    pub buying_power: String,
    #[serde(default)]
    pub cash: String,
    #[serde(default)]
    pub portfolio_value: String,
    #[serde(default)]
    pub equity: String,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct OrderRequest {
    pub symbol: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub qty: String,
    /// Dollar-sized crypto market orders ("$100 of BTC"). Mutually exclusive
    /// with `qty`; valid on market orders only. Omitted entirely for every
    /// other order so the simple stock/option serialization stays
    /// byte-identical.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notional: Option<String>,
    pub side: String,
    #[serde(rename = "type")]
    pub order_type: String,
    pub time_in_force: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub limit_price: String,
    /// Required for `stop` and `stop_limit`; omitted otherwise. Alpaca
    /// rejects the request if it appears on the wrong order_type.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop_price: Option<String>,
    /// For `trailing_stop` orders. We expose percent (easier risk UX); the
    /// `trail_price` alternative is intentionally omitted.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trail_percent: Option<String>,
    /// Bracket / OTO / OCO. When `Some("bracket")`, both `take_profit` and
    /// `stop_loss` must be set per Alpaca's spec.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub order_class: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub take_profit: Option<TakeProfit>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop_loss: Option<StopLoss>,
}

#[derive(Debug, Clone, Serialize)]
pub struct TakeProfit {
    pub limit_price: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct StopLoss {
    pub stop_price: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit_price: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Order {
    pub id: String,
    pub symbol: String,
    pub side: String,
    #[serde(rename = "type")]
    pub order_type: String,
    /// Empty for notional crypto orders (Alpaca returns `qty: null` there) —
    /// defaulted so one such order can't fail the whole orders fetch.
    #[serde(default, deserialize_with = "de_null_string")]
    pub qty: String,
    /// Dollar size of a notional crypto order; None everywhere else.
    #[serde(default)]
    pub notional: Option<String>,
    #[serde(default)]
    pub limit_price: Option<String>,
    pub status: String,
    #[serde(default)]
    pub filled_qty: String,
    #[serde(default)]
    pub filled_avg_price: Option<String>,
    pub created_at: DateTime<Utc>,
    /// `us_equity` or `us_option`. The Options desk Orders sub-tab filters on
    /// this; Terminal leaves it unfiltered.
    #[serde(default)]
    pub asset_class: String,
}

impl Order {
    pub fn limit_price_str(&self) -> &str {
        self.limit_price.as_deref().unwrap_or("")
    }
    pub fn filled_avg_price_str(&self) -> &str {
        self.filled_avg_price.as_deref().unwrap_or("")
    }
}

#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
pub struct Activity {
    #[serde(default)]
    pub id: String,
    pub activity_type: String,
    #[serde(default)]
    pub transaction_time: Option<DateTime<Utc>>,
    #[serde(default)]
    pub date: Option<String>,
    #[serde(default, rename = "type")]
    pub fill_type: Option<String>,
    #[serde(default)]
    pub price: Option<String>,
    #[serde(default)]
    pub qty: Option<String>,
    #[serde(default)]
    pub cum_qty: Option<String>,
    #[serde(default)]
    pub side: Option<String>,
    #[serde(default)]
    pub symbol: Option<String>,
    #[serde(default)]
    pub order_id: Option<String>,
    #[serde(default)]
    pub net_amount: Option<String>,
    #[serde(default)]
    pub per_share_amount: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
pub struct Asset {
    pub symbol: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub tradable: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Bar {
    #[serde(rename = "t")]
    pub time: DateTime<Utc>,
    #[serde(rename = "o")]
    pub open: f64,
    #[serde(rename = "h")]
    pub high: f64,
    #[serde(rename = "l")]
    pub low: f64,
    #[serde(rename = "c")]
    pub close: f64,
    /// Crypto bars carry fractional volume (e.g. 2345.67 BTC), stock bars
    /// integral — parse as f64 and truncate so one Bar type serves both.
    #[serde(rename = "v", deserialize_with = "de_f64_as_i64")]
    pub volume: i64,
}

/// Accept either an integer or a float in JSON and truncate to i64.
fn de_f64_as_i64<'de, D: serde::Deserializer<'de>>(d: D) -> Result<i64, D::Error> {
    let v = f64::deserialize(d)?;
    Ok(v as i64)
}

/// Accept a string or JSON null, mapping null to "".
fn de_null_string<'de, D: serde::Deserializer<'de>>(d: D) -> Result<String, D::Error> {
    let v: Option<String> = Option::deserialize(d)?;
    Ok(v.unwrap_or_default())
}

#[derive(Debug, Deserialize)]
struct BarsResponse {
    #[serde(default)]
    bars: Vec<Bar>,
    #[serde(default)]
    next_page_token: Option<String>,
}

/// Crypto bars come back keyed by pair: `{"bars": {"BTC/USD": [...]}}`.
#[derive(Debug, Deserialize)]
struct CryptoBarsResponse {
    #[serde(default)]
    bars: HashMap<String, Vec<Bar>>,
    #[serde(default)]
    next_page_token: Option<String>,
}

// ---------------- Options ----------------
//
// Two sources feed the Options Chain (see docs/adr/0001):
//   - `/v2/options/contracts` (trading API) → the chain skeleton: every
//     contract's strike / expiration / type / open interest / prev close.
//   - `/v1beta1/options/snapshots/{underlying}` (data API, indicative feed) →
//     bid/ask + last + daily/prev bars for initial fill and %chg.
// Live bid/ask updates ride the options WebSocket (see stream.rs), not these.

/// One option contract as returned by `/v2/options/contracts`. Only the fields
/// the Chain needs are deserialized; the rest of the payload is dropped. A few
/// (name / underlying / tradable) are carried for completeness / future use.
#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
pub struct OptionContract {
    /// OCC symbol, e.g. `AAPL260619C00150000`.
    pub symbol: String,
    #[serde(default)]
    pub name: String,
    /// `YYYY-MM-DD`.
    #[serde(default)]
    pub expiration_date: String,
    #[serde(default)]
    pub underlying_symbol: String,
    /// `call` or `put`.
    #[serde(default, rename = "type")]
    pub kind: String,
    /// Strike as a decimal string, e.g. `150`.
    #[serde(default)]
    pub strike_price: String,
    #[serde(default)]
    pub open_interest: Option<String>,
    /// Previous-day close of the contract — the reference for %chg.
    #[serde(default)]
    pub close_price: Option<String>,
    #[serde(default)]
    pub tradable: bool,
}

#[derive(Debug, Deserialize)]
struct OptionContractsResponse {
    #[serde(default)]
    option_contracts: Vec<OptionContract>,
    #[serde(default)]
    next_page_token: Option<String>,
}

/// Per-contract snapshot from `/v1beta1/options/snapshots`. On the indicative
/// feed this carries quotes/trades/bars but NOT greeks or IV (those need the
/// paid OPRA feed) — so the v1 Chain shows no greeks.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct OptionSnapshot {
    #[serde(default, rename = "latestQuote")]
    pub latest_quote: Option<OptQuote>,
    #[serde(default, rename = "latestTrade")]
    pub latest_trade: Option<OptTrade>,
    #[serde(default, rename = "dailyBar")]
    pub daily_bar: Option<OptBar>,
    #[serde(default, rename = "prevDailyBar")]
    pub prev_daily_bar: Option<OptBar>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[allow(dead_code)] // bid_size / ask_size parsed for completeness; not shown in v1
pub struct OptQuote {
    #[serde(rename = "bp", default)]
    pub bid: f64,
    #[serde(rename = "bs", default)]
    pub bid_size: f64,
    #[serde(rename = "ap", default)]
    pub ask: f64,
    #[serde(rename = "as", default)]
    pub ask_size: f64,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct OptTrade {
    #[serde(rename = "p", default)]
    pub price: f64,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct OptBar {
    #[serde(rename = "c", default)]
    pub close: f64,
    #[serde(rename = "v", default)]
    pub volume: f64,
}

#[derive(Debug, Deserialize)]
struct OptionSnapshotsResponse {
    #[serde(default)]
    snapshots: HashMap<String, OptionSnapshot>,
    #[serde(default)]
    next_page_token: Option<String>,
}

// ---------------- Crypto ----------------
//
// Crypto data lives on `/v1beta3/crypto/us` (REST + WS); trading rides the
// same `/v2/orders` / `/v2/positions` as everything else with
// `asset_class == "crypto"`. See docs/adr/0002.

/// Canonical Pair spelling is the slash form (`BTC/USD`) — it's what the data
/// API and the tick cache key on. The TRADING API returns crypto symbols
/// slashless (`BTCUSD`), so positions/orders are normalized through this at
/// the API boundary. Known quote currencies are matched longest-first so
/// `BTCUSDT` → `BTC/USDT`, not `BTCUSD` + trailing `T`.
pub fn canonical_crypto_symbol(sym: &str) -> String {
    if sym.contains('/') {
        return sym.to_string();
    }
    for quote in ["USDT", "USDC", "USD", "BTC"] {
        if let Some(base) = sym.strip_suffix(quote) {
            if !base.is_empty() {
                return format!("{}/{}", base, quote);
            }
        }
    }
    sym.to_string()
}

fn canonicalize_crypto_positions(mut v: Vec<Position>) -> Vec<Position> {
    for p in &mut v {
        if p.asset_class == "crypto" {
            p.symbol = canonical_crypto_symbol(&p.symbol);
        }
    }
    v
}

fn canonicalize_crypto_orders(mut v: Vec<Order>) -> Vec<Order> {
    for o in &mut v {
        if o.asset_class == "crypto" {
            o.symbol = canonical_crypto_symbol(&o.symbol);
        }
    }
    v
}

/// Per-pair snapshot from `/v1beta3/crypto/us/snapshots` — same wire shape as
/// the options snapshot (quote/trade/daily bars), reusing the same inner
/// frame types.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct CryptoSnapshot {
    #[serde(default, rename = "latestQuote")]
    pub latest_quote: Option<OptQuote>,
    #[serde(default, rename = "latestTrade")]
    pub latest_trade: Option<OptTrade>,
    #[serde(default, rename = "dailyBar")]
    pub daily_bar: Option<OptBar>,
    #[serde(default, rename = "prevDailyBar")]
    pub prev_daily_bar: Option<OptBar>,
}

#[derive(Debug, Deserialize)]
struct CryptoSnapshotsResponse {
    #[serde(default)]
    snapshots: HashMap<String, CryptoSnapshot>,
}

impl AlpacaClient {
    pub fn new(creds: Credentials) -> Self {
        let base_url = if creds.base_url.is_empty() {
            "https://paper-api.alpaca.markets".to_string()
        } else {
            creds.base_url
        };
        let agent = ureq::AgentBuilder::new()
            .timeout(Duration::from_secs(15))
            .build();
        AlpacaClient {
            base_url,
            api_key: creds.api_key,
            api_secret: creds.api_secret,
            agent,
        }
    }

    fn auth(&self, req: ureq::Request) -> ureq::Request {
        req.set("APCA-API-KEY-ID", &self.api_key)
            .set("APCA-API-SECRET-KEY", &self.api_secret)
    }

    fn handle_resp<T: for<'de> Deserialize<'de>>(resp: ureq::Response) -> Result<T> {
        let status = resp.status();
        let body = resp
            .into_string()
            .map_err(|e| anyhow!("read body: {}", e))?;
        if status >= 400 {
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(&body) {
                if let Some(msg) = v.get("message").and_then(|m| m.as_str()) {
                    return Err(anyhow!("API error {}: {}", status, msg));
                }
            }
            return Err(anyhow!("API error {}: {}", status, body));
        }
        serde_json::from_str(&body).map_err(|e| anyhow!("decode json: {}", e))
    }

    fn handle_err(err: ureq::Error) -> anyhow::Error {
        match err {
            ureq::Error::Status(code, resp) => {
                let body = resp.into_string().unwrap_or_default();
                if let Ok(v) = serde_json::from_str::<serde_json::Value>(&body) {
                    if let Some(msg) = v.get("message").and_then(|m| m.as_str()) {
                        return anyhow!("API error {}: {}", code, msg);
                    }
                }
                anyhow!("API error {}: {}", code, body)
            }
            other => anyhow!("transport: {}", other),
        }
    }

    pub fn get_positions(&self) -> Result<Vec<Position>> {
        let url = format!("{}/v2/positions", self.base_url);
        match self.auth(self.agent.get(&url)).call() {
            Ok(r) => Self::handle_resp(r).map(canonicalize_crypto_positions),
            Err(e) => Err(Self::handle_err(e)),
        }
    }

    pub fn get_account(&self) -> Result<Account> {
        let url = format!("{}/v2/account", self.base_url);
        match self.auth(self.agent.get(&url)).call() {
            Ok(r) => Self::handle_resp(r),
            Err(e) => Err(Self::handle_err(e)),
        }
    }

    pub fn place_order(&self, req: &OrderRequest) -> Result<Order> {
        let url = format!("{}/v2/orders", self.base_url);
        let body = serde_json::to_value(req)?;
        match self
            .auth(self.agent.post(&url))
            .set("Content-Type", "application/json")
            .send_json(body)
        {
            Ok(r) => Self::handle_resp(r),
            Err(e) => Err(Self::handle_err(e)),
        }
    }

    pub fn get_orders(&self) -> Result<Vec<Order>> {
        let url = format!("{}/v2/orders?status=open&limit=50", self.base_url);
        match self.auth(self.agent.get(&url)).call() {
            Ok(r) => Self::handle_resp(r).map(canonicalize_crypto_orders),
            Err(e) => Err(Self::handle_err(e)),
        }
    }

    pub fn cancel_order(&self, order_id: &str) -> Result<()> {
        let url = format!("{}/v2/orders/{}", self.base_url, order_id);
        match self.auth(self.agent.delete(&url)).call() {
            Ok(_) => Ok(()),
            Err(e) => Err(Self::handle_err(e)),
        }
    }

    pub fn get_activities(&self) -> Result<Vec<Activity>> {
        let url = format!(
            "{}/v2/account/activities?page_size=100&direction=desc",
            self.base_url
        );
        match self.auth(self.agent.get(&url)).call() {
            Ok(r) => Self::handle_resp(r),
            Err(e) => Err(Self::handle_err(e)),
        }
    }

    pub fn get_closed_orders(&self) -> Result<Vec<Order>> {
        let url = format!(
            "{}/v2/orders?status=closed&limit=100&direction=desc",
            self.base_url
        );
        match self.auth(self.agent.get(&url)).call() {
            Ok(r) => Self::handle_resp(r).map(canonicalize_crypto_orders),
            Err(e) => Err(Self::handle_err(e)),
        }
    }

    /// Tradable crypto asset list — the Crypto desk's Markets-grid universe
    /// (the desk filters to USD-quoted Pairs).
    pub fn get_crypto_assets(&self) -> Result<Vec<Asset>> {
        let url = format!(
            "{}/v2/assets?status=active&asset_class=crypto",
            self.base_url
        );
        match self.auth(self.agent.get(&url)).call() {
            Ok(r) => Self::handle_resp(r),
            Err(e) => Err(Self::handle_err(e)),
        }
    }

    /// Bulk snapshots (quote/trade/daily bars) for a set of Pairs, keyed by
    /// slash symbol. Initial fill + %chg source for the Markets grid; the
    /// crypto WebSocket keeps displayed rows fresher than this.
    pub fn get_crypto_snapshots(
        &self,
        symbols: &[String],
    ) -> Result<HashMap<String, CryptoSnapshot>> {
        if symbols.is_empty() {
            return Ok(HashMap::new());
        }
        let joined = symbols.join(",");
        let url = format!(
            "{}/v1beta3/crypto/us/snapshots?symbols={}",
            ALPACA_DATA_BASE,
            urlencode(&joined),
        );
        let resp = match self.auth(self.agent.get(&url)).call() {
            Ok(r) => r,
            Err(e) => return Err(Self::handle_err(e)),
        };
        let sr: CryptoSnapshotsResponse = Self::handle_resp(resp)?;
        Ok(sr.snapshots)
    }

    pub fn get_assets(&self) -> Result<Vec<Asset>> {
        let url = format!(
            "{}/v2/assets?status=active&asset_class=us_equity",
            self.base_url
        );
        match self.auth(self.agent.get(&url)).call() {
            Ok(r) => Self::handle_resp(r),
            Err(e) => Err(Self::handle_err(e)),
        }
    }

    /// Fetch the option-contract list for an underlying (the chain skeleton),
    /// covering expirations from today out ~2 years. The explicit date window
    /// is load-bearing: WITHOUT it the endpoint returns only the few
    /// nearest-dated expirations, so the chain would be missing most of its
    /// expiration dropdown. Paginated + capped; the UI groups by expiration.
    pub fn get_option_contracts(&self, underlying: &str) -> Result<Vec<OptionContract>> {
        let today = Utc::now().date_naive();
        let lte = today + chrono::Duration::days(730);
        let gte_s = today.format("%Y-%m-%d").to_string();
        let lte_s = lte.format("%Y-%m-%d").to_string();
        let mut all: Vec<OptionContract> = Vec::new();
        let mut page_token: Option<String> = None;
        loop {
            let mut url = format!(
                "{}/v2/options/contracts?underlying_symbols={}&expiration_date_gte={}&expiration_date_lte={}&limit=10000",
                self.base_url,
                urlencode(underlying),
                gte_s,
                lte_s,
            );
            if let Some(tok) = &page_token {
                url.push_str("&page_token=");
                url.push_str(&urlencode(tok));
            }
            let resp = match self.auth(self.agent.get(&url)).call() {
                Ok(r) => r,
                Err(e) => return Err(Self::handle_err(e)),
            };
            let cr: OptionContractsResponse = Self::handle_resp(resp)?;
            all.extend(cr.option_contracts);
            match cr.next_page_token {
                Some(t) if !t.is_empty() => page_token = Some(t),
                _ => break,
            }
            if all.len() > 20_000 {
                break;
            }
        }
        Ok(all)
    }

    /// Fetch snapshots (bid/ask + last + daily bars) for every listed contract
    /// of an underlying, keyed by OCC symbol. Uses the indicative feed. The
    /// live WebSocket keeps the displayed rows fresher than this; the snapshot
    /// is the initial fill + the source of %chg / open-interest-adjacent data.
    pub fn get_option_snapshots(&self, underlying: &str) -> Result<HashMap<String, OptionSnapshot>> {
        let mut all: HashMap<String, OptionSnapshot> = HashMap::new();
        let mut page_token: Option<String> = None;
        loop {
            let mut url = format!(
                "{}/v1beta1/options/snapshots/{}?feed={}&limit=1000",
                ALPACA_DATA_BASE,
                urlencode(underlying),
                OPTIONS_DATA_FEED,
            );
            if let Some(tok) = &page_token {
                url.push_str("&page_token=");
                url.push_str(&urlencode(tok));
            }
            let resp = match self.auth(self.agent.get(&url)).call() {
                Ok(r) => r,
                Err(e) => return Err(Self::handle_err(e)),
            };
            let sr: OptionSnapshotsResponse = Self::handle_resp(resp)?;
            all.extend(sr.snapshots);
            match sr.next_page_token {
                Some(t) if !t.is_empty() => page_token = Some(t),
                _ => break,
            }
            if all.len() > 20_000 {
                break;
            }
        }
        Ok(all)
    }

    pub fn get_bars(
        &self,
        symbol: &str,
        timeframe: &str,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
    ) -> Result<Vec<Bar>> {
        // Pairs (slash symbols) branch to the crypto data API — same OHLCV
        // shape, different host path + symbol-keyed response. The whole
        // Chart/Compare stack upstream of this call is symbol-format
        // agnostic. See docs/adr/0002.
        if symbol.contains('/') {
            return self.get_crypto_bars(symbol, timeframe, start, end);
        }
        let mut all: Vec<Bar> = Vec::new();
        let mut page_token: Option<String> = None;

        loop {
            let mut url = format!(
                "{}/v2/stocks/{}/bars?timeframe={}&start={}&end={}&limit=10000&adjustment=split&feed=iex",
                ALPACA_DATA_BASE,
                urlencode(symbol),
                urlencode(timeframe),
                urlencode(&start.to_rfc3339()),
                urlencode(&end.to_rfc3339()),
            );
            if let Some(tok) = &page_token {
                url.push_str("&page_token=");
                url.push_str(&urlencode(tok));
            }

            let resp = match self.auth(self.agent.get(&url)).call() {
                Ok(r) => r,
                Err(e) => return Err(Self::handle_err(e)),
            };
            let br: BarsResponse = Self::handle_resp(resp)?;
            all.extend(br.bars);
            match br.next_page_token {
                Some(t) if !t.is_empty() => page_token = Some(t),
                _ => break,
            }
            if all.len() > 50_000 {
                break;
            }
        }
        Ok(all)
    }

    /// Crypto bars from `/v1beta3/crypto/us/bars`. No `feed`/`adjustment`
    /// params (those are stock concepts); the response keys bars by Pair.
    fn get_crypto_bars(
        &self,
        symbol: &str,
        timeframe: &str,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
    ) -> Result<Vec<Bar>> {
        let mut all: Vec<Bar> = Vec::new();
        let mut page_token: Option<String> = None;
        loop {
            let mut url = format!(
                "{}/v1beta3/crypto/us/bars?symbols={}&timeframe={}&start={}&end={}&limit=10000",
                ALPACA_DATA_BASE,
                urlencode(symbol),
                urlencode(timeframe),
                urlencode(&start.to_rfc3339()),
                urlencode(&end.to_rfc3339()),
            );
            if let Some(tok) = &page_token {
                url.push_str("&page_token=");
                url.push_str(&urlencode(tok));
            }
            let resp = match self.auth(self.agent.get(&url)).call() {
                Ok(r) => r,
                Err(e) => return Err(Self::handle_err(e)),
            };
            let br: CryptoBarsResponse = Self::handle_resp(resp)?;
            if let Some(bars) = br.bars.into_values().next() {
                all.extend(bars);
            }
            match br.next_page_token {
                Some(t) if !t.is_empty() => page_token = Some(t),
                _ => break,
            }
            if all.len() > 50_000 {
                break;
            }
        }
        Ok(all)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonical_crypto_symbol_inserts_slash() {
        assert_eq!(canonical_crypto_symbol("BTCUSD"), "BTC/USD");
        assert_eq!(canonical_crypto_symbol("ETHUSD"), "ETH/USD");
        assert_eq!(canonical_crypto_symbol("AVAXUSD"), "AVAX/USD");
    }

    #[test]
    fn canonical_crypto_symbol_matches_longest_quote_first() {
        assert_eq!(canonical_crypto_symbol("BTCUSDT"), "BTC/USDT");
        assert_eq!(canonical_crypto_symbol("ETHUSDC"), "ETH/USDC");
        assert_eq!(canonical_crypto_symbol("ETHBTC"), "ETH/BTC");
    }

    #[test]
    fn canonical_crypto_symbol_leaves_slash_form_alone() {
        assert_eq!(canonical_crypto_symbol("BTC/USD"), "BTC/USD");
    }

    #[test]
    fn canonical_crypto_symbol_never_empties_the_base() {
        // "USD" alone must not become "/USD" — no quote suffix may eat the
        // entire symbol.
        assert_eq!(canonical_crypto_symbol("USD"), "USD");
        assert_eq!(canonical_crypto_symbol("BTC"), "BTC");
    }

    #[test]
    fn bar_volume_accepts_fractional_and_integral() {
        let crypto = r#"{"t":"2026-01-05T00:00:00Z","o":1.0,"h":2.0,"l":0.5,"c":1.5,"v":2345.67}"#;
        let b: Bar = serde_json::from_str(crypto).unwrap();
        assert_eq!(b.volume, 2345);
        let stock = r#"{"t":"2026-01-05T00:00:00Z","o":1.0,"h":2.0,"l":0.5,"c":1.5,"v":1234}"#;
        let b: Bar = serde_json::from_str(stock).unwrap();
        assert_eq!(b.volume, 1234);
    }

    #[test]
    fn simple_order_serialization_omits_crypto_fields() {
        // Load-bearing: Alpaca rejects unknown/wrong fields per order_type, so
        // a plain market order must serialize without notional/stop/etc.
        let req = OrderRequest {
            symbol: "AAPL".into(),
            qty: "10".into(),
            side: "buy".into(),
            order_type: "market".into(),
            time_in_force: "day".into(),
            ..Default::default()
        };
        let v = serde_json::to_value(&req).unwrap();
        let obj = v.as_object().unwrap();
        assert!(!obj.contains_key("notional"));
        assert!(!obj.contains_key("stop_price"));
        assert!(!obj.contains_key("limit_price"));
    }
}

fn urlencode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            'a'..='z' | 'A'..='Z' | '0'..='9' | '-' | '_' | '.' | '~' => out.push(c),
            _ => {
                let mut buf = [0u8; 4];
                let bytes = c.encode_utf8(&mut buf).as_bytes();
                for b in bytes {
                    out.push_str(&format!("%{:02X}", b));
                }
            }
        }
    }
    out
}
