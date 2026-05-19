use std::time::Duration;

use anyhow::{anyhow, Result};
use serde::Deserialize;
use serde_json::Value;

use crate::app::{Relation, SupplyChainData};

const FMP_BASE: &str = "https://financialmodelingprep.com";

pub struct FmpClient {
    api_key: String,
    agent: ureq::Agent,
}

impl FmpClient {
    pub fn new(api_key: Option<String>) -> Self {
        let key = api_key
            .or_else(|| std::env::var("FMP_API_KEY").ok())
            .unwrap_or_default();
        let agent = ureq::AgentBuilder::new()
            .timeout(Duration::from_secs(15))
            .build();
        FmpClient { api_key: key, agent }
    }

    pub fn is_configured(&self) -> bool {
        !self.api_key.is_empty()
    }

    pub fn fetch_supply_chain(&self, ticker: &str) -> Result<SupplyChainData> {
        if !self.is_configured() {
            return Err(anyhow!(
                "FMP_API_KEY not set — export it or save it to credentials.json"
            ));
        }
        let t = ticker.to_ascii_uppercase();

        let company_name = self.fetch_company_name(&t).unwrap_or_default();
        let competitors = self.fetch_peers(&t)?;
        let (suppliers, customers, note) = self.fetch_supply_records(&t)?;

        Ok(SupplyChainData {
            company_name: if company_name.is_empty() { t.clone() } else { company_name },
            suppliers,
            competitors,
            customers,
            note,
        })
    }

    fn url(&self, path: &str, extra_query: &str) -> String {
        let sep = if extra_query.is_empty() { "" } else { "&" };
        format!(
            "{}{}?apikey={}{}{}",
            FMP_BASE, path, self.api_key, sep, extra_query
        )
    }

    fn get_json(&self, url: &str) -> Result<Value> {
        let resp = match self.agent.get(url).call() {
            Ok(r) => r,
            Err(ureq::Error::Status(code, r)) => {
                let body = r.into_string().unwrap_or_default();
                return Err(anyhow!("FMP API {}: {}", code, body));
            }
            Err(other) => return Err(anyhow!("transport: {}", other)),
        };
        let body = resp
            .into_string()
            .map_err(|e| anyhow!("read body: {}", e))?;
        serde_json::from_str(&body).map_err(|e| anyhow!("decode json: {}", e))
    }

    fn fetch_company_name(&self, ticker: &str) -> Option<String> {
        let url = self.url(&format!("/api/v3/profile/{}", ticker), "");
        let v = self.get_json(&url).ok()?;
        let arr = v.as_array()?;
        let obj = arr.first()?.as_object()?;
        obj.get("companyName")
            .and_then(|x| x.as_str())
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string())
    }

    fn fetch_peers(&self, ticker: &str) -> Result<Vec<Relation>> {
        let url = self.url("/api/v4/stock_peers", &format!("symbol={}", ticker));
        let v = self.get_json(&url)?;
        let arr = match v.as_array() {
            Some(a) => a,
            None => return Ok(Vec::new()),
        };
        let Some(obj) = arr.first().and_then(|x| x.as_object()) else {
            return Ok(Vec::new());
        };
        let Some(peers) = obj.get("peersList").and_then(|x| x.as_array()) else {
            return Ok(Vec::new());
        };
        Ok(peers
            .iter()
            .filter_map(|p| p.as_str())
            .filter(|s| !s.is_empty())
            .take(8)
            .map(|sym| Relation {
                name: sym.to_string(),
                ticker: Some(sym.to_ascii_uppercase()),
                rationale: String::from("Peer per FMP stock_peers"),
            })
            .collect())
    }

    /// Returns (suppliers, customers, note). The FMP supply-chain endpoint is
    /// gated to higher plans — a 403 / 401 surfaces as a per-call error so the
    /// caller can still render whatever else loaded.
    fn fetch_supply_records(
        &self,
        ticker: &str,
    ) -> Result<(Vec<Relation>, Vec<Relation>, String)> {
        let url = self.url("/api/v4/supply-chain", &format!("symbol={}", ticker));
        let v = self.get_json(&url)?;
        let arr = match v.as_array() {
            Some(a) => a,
            None => return Ok((Vec::new(), Vec::new(), "No supply-chain records returned by FMP".into())),
        };

        let mut suppliers: Vec<Relation> = Vec::new();
        let mut customers: Vec<Relation> = Vec::new();
        let mut latest_date = String::new();

        for rec in arr {
            let Some(o) = rec.as_object() else { continue };
            let dir = o
                .get("direction")
                .and_then(|x| x.as_str())
                .unwrap_or("")
                .to_ascii_lowercase();
            let related_symbol = o
                .get("relatedSymbol")
                .and_then(|x| x.as_str())
                .filter(|s| !s.is_empty())
                .map(|s| s.to_ascii_uppercase());
            let related_name = o
                .get("relatedName")
                .and_then(|x| x.as_str())
                .filter(|s| !s.is_empty())
                .map(|s| s.to_string())
                .or_else(|| related_symbol.clone())
                .unwrap_or_default();
            if related_name.is_empty() {
                continue;
            }
            let pct = o.get("percentage").and_then(|x| {
                if let Some(s) = x.as_str() {
                    s.parse::<f64>().ok()
                } else {
                    x.as_f64()
                }
            });
            let segment = o
                .get("segment")
                .and_then(|x| x.as_str())
                .filter(|s| !s.is_empty());
            let date = o
                .get("date")
                .and_then(|x| x.as_str())
                .filter(|s| !s.is_empty())
                .unwrap_or("");
            if !date.is_empty() && date > latest_date.as_str() {
                latest_date = date.to_string();
            }
            let rationale = match (pct, segment) {
                (Some(p), Some(seg)) => format!("{:.1}% — {}", p, seg),
                (Some(p), None) => format!("{:.1}% of revenue", p),
                (None, Some(seg)) => seg.to_string(),
                (None, None) => String::new(),
            };
            let row = Relation {
                name: related_name,
                ticker: related_symbol,
                rationale,
            };
            match dir.as_str() {
                "supplier" => suppliers.push(row),
                "customer" => customers.push(row),
                _ => {}
            }
        }

        let note = if latest_date.is_empty() {
            "FMP supply-chain dataset (disclosure-based)".into()
        } else {
            format!("FMP supply-chain dataset — latest record {}", latest_date)
        };
        Ok((suppliers, customers, note))
    }
}

#[allow(dead_code)]
#[derive(Deserialize)]
struct ProfileEntry {
    #[serde(default, rename = "companyName")]
    company_name: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parses_supply_chain_payload() {
        let payload = json!([
            {
                "symbol": "AAPL",
                "relatedSymbol": "TSM",
                "relatedName": "Taiwan Semiconductor Manufacturing",
                "direction": "supplier",
                "percentage": "12.4",
                "segment": "Logic foundry",
                "date": "2024-09-30"
            },
            {
                "symbol": "AAPL",
                "relatedSymbol": "VZ",
                "relatedName": "Verizon Communications",
                "direction": "customer",
                "percentage": null,
                "segment": "iPhone carrier",
                "date": "2024-09-30"
            },
            {
                "symbol": "AAPL",
                "relatedSymbol": "",
                "relatedName": "Foxconn",
                "direction": "supplier",
                "percentage": null,
                "segment": null,
                "date": "2024-06-30"
            }
        ]);

        // Inline parse by replicating the field walk done in `fetch_supply_records`.
        let arr = payload.as_array().unwrap();
        let mut suppliers: Vec<Relation> = Vec::new();
        let mut customers: Vec<Relation> = Vec::new();
        for rec in arr {
            let o = rec.as_object().unwrap();
            let dir = o.get("direction").and_then(|x| x.as_str()).unwrap_or("");
            let related_symbol = o
                .get("relatedSymbol")
                .and_then(|x| x.as_str())
                .filter(|s| !s.is_empty())
                .map(|s| s.to_string());
            let related_name = o
                .get("relatedName")
                .and_then(|x| x.as_str())
                .unwrap()
                .to_string();
            let pct = o
                .get("percentage")
                .and_then(|x| x.as_str())
                .and_then(|s| s.parse::<f64>().ok());
            let segment = o.get("segment").and_then(|x| x.as_str());
            let rationale = match (pct, segment) {
                (Some(p), Some(seg)) => format!("{:.1}% — {}", p, seg),
                (Some(p), None) => format!("{:.1}% of revenue", p),
                (None, Some(seg)) => seg.to_string(),
                (None, None) => String::new(),
            };
            let row = Relation {
                name: related_name,
                ticker: related_symbol,
                rationale,
            };
            match dir {
                "supplier" => suppliers.push(row),
                "customer" => customers.push(row),
                _ => {}
            }
        }
        assert_eq!(suppliers.len(), 2);
        assert_eq!(suppliers[0].ticker.as_deref(), Some("TSM"));
        assert!(suppliers[0].rationale.contains("12.4%"));
        assert!(suppliers[1].ticker.is_none());
        assert_eq!(customers.len(), 1);
        assert_eq!(customers[0].ticker.as_deref(), Some("VZ"));
    }
}
