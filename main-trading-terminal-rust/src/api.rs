use anyhow::{anyhow, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::time::Duration;

use crate::config::Credentials;

pub const ALPACA_DATA_BASE: &str = "https://data.alpaca.markets";

#[derive(Clone)]
pub struct AlpacaClient {
    pub base_url: String,
    pub api_key: String,
    pub api_secret: String,
    pub agent: ureq::Agent,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Position {
    pub symbol: String,
    pub qty: String,
    pub avg_entry_price: String,
    pub current_price: String,
    pub market_value: String,
    pub unrealized_pl: String,
    pub unrealized_plpc: String,
    pub side: String,
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

#[derive(Debug, Clone, Serialize)]
pub struct OrderRequest {
    pub symbol: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub qty: String,
    pub side: String,
    #[serde(rename = "type")]
    pub order_type: String,
    pub time_in_force: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub limit_price: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Order {
    pub id: String,
    pub symbol: String,
    pub side: String,
    #[serde(rename = "type")]
    pub order_type: String,
    pub qty: String,
    #[serde(default)]
    pub limit_price: Option<String>,
    pub status: String,
    #[serde(default)]
    pub filled_qty: String,
    #[serde(default)]
    pub filled_avg_price: Option<String>,
    pub created_at: DateTime<Utc>,
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
    #[serde(rename = "v")]
    pub volume: i64,
}

#[derive(Debug, Deserialize)]
struct BarsResponse {
    #[serde(default)]
    bars: Vec<Bar>,
    #[serde(default)]
    next_page_token: Option<String>,
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
            Ok(r) => Self::handle_resp(r),
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
            Ok(r) => Self::handle_resp(r),
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
            Ok(r) => Self::handle_resp(r),
            Err(e) => Err(Self::handle_err(e)),
        }
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

    pub fn get_bars(
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
