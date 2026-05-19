use std::time::Duration;

use anyhow::{anyhow, Result};
use serde::Deserialize;
use serde_json::{json, Value};

const ANTHROPIC_URL: &str = "https://api.anthropic.com/v1/messages";
const ANTHROPIC_VERSION: &str = "2023-06-01";
const MODEL: &str = "claude-haiku-4-5-20251001";

const SYSTEM_PROMPT: &str = "You are a financial research assistant. Given a publicly-traded company ticker, return its key suppliers, competitors, and major customers/buyers in structured form.

Rules:
- Use the report_supply_chain tool. Do not respond in plain text.
- Only include relationships you have high confidence about from public filings (10-K, 10-Q), well-known industry reporting, or major press releases.
- Cap each list at 8 entries, ordered by importance/relationship strength.
- For each entity, include a US ticker symbol when the company is publicly listed on a US exchange; otherwise set ticker to null. Use the foreign exchange ticker for major non-US listings (e.g. 2330 for TSMC) when no US listing exists.
- The rationale field should be one short sentence (under 80 chars) explaining the relationship — segment, percentage, geography, or product line.
- The note field should briefly describe data quality (e.g. \"based on FY24 10-K\", \"largely qualitative — exact percentages not disclosed\").
- If the ticker is not a real publicly-traded company, return empty arrays and explain in the note.";

use crate::app::{Relation, SupplyChainData};

pub struct ClaudeClient {
    api_key: String,
    agent: ureq::Agent,
}

impl ClaudeClient {
    pub fn new(api_key: Option<String>) -> Self {
        let key = api_key
            .or_else(|| std::env::var("ANTHROPIC_API_KEY").ok())
            .unwrap_or_default();
        let agent = ureq::AgentBuilder::new()
            .timeout(Duration::from_secs(30))
            .build();
        ClaudeClient { api_key: key, agent }
    }

    pub fn is_configured(&self) -> bool {
        !self.api_key.is_empty()
    }

    pub fn fetch_supply_chain(&self, ticker: &str) -> Result<SupplyChainData> {
        if !self.is_configured() {
            return Err(anyhow!(
                "ANTHROPIC_API_KEY not set — export it or save it to credentials.json"
            ));
        }

        let body = build_request_body(ticker);
        let resp = self
            .agent
            .post(ANTHROPIC_URL)
            .set("x-api-key", &self.api_key)
            .set("anthropic-version", ANTHROPIC_VERSION)
            .set("content-type", "application/json")
            .send_json(body);

        let resp = match resp {
            Ok(r) => r,
            Err(ureq::Error::Status(code, r)) => {
                let body = r.into_string().unwrap_or_default();
                if let Ok(v) = serde_json::from_str::<Value>(&body) {
                    if let Some(msg) = v
                        .get("error")
                        .and_then(|e| e.get("message"))
                        .and_then(|m| m.as_str())
                    {
                        return Err(anyhow!("Anthropic API {}: {}", code, msg));
                    }
                }
                return Err(anyhow!("Anthropic API {}: {}", code, body));
            }
            Err(other) => return Err(anyhow!("transport: {}", other)),
        };

        let parsed: MessagesResponse = resp
            .into_json()
            .map_err(|e| anyhow!("decode messages response: {}", e))?;

        let tool_input = parsed
            .content
            .into_iter()
            .find_map(|b| match b {
                ContentBlock::ToolUse { input, .. } => Some(input),
                _ => None,
            })
            .ok_or_else(|| anyhow!("Claude response had no tool_use block"))?;

        parse_tool_input(tool_input)
    }
}

fn build_request_body(ticker: &str) -> Value {
    json!({
        "model": MODEL,
        "max_tokens": 2048,
        "system": [{
            "type": "text",
            "text": SYSTEM_PROMPT,
            "cache_control": {"type": "ephemeral"},
        }],
        "tools": [{
            "name": "report_supply_chain",
            "description": "Return the supply chain relationships for the requested ticker.",
            "input_schema": tool_schema(),
            "cache_control": {"type": "ephemeral"},
        }],
        "tool_choice": {"type": "tool", "name": "report_supply_chain"},
        "messages": [{
            "role": "user",
            "content": format!(
                "Provide supply chain relationships for ticker {}.",
                ticker.to_ascii_uppercase()
            ),
        }],
    })
}

fn tool_schema() -> Value {
    let relation = json!({
        "type": "object",
        "properties": {
            "name": {"type": "string"},
            "ticker": {"type": ["string", "null"]},
            "rationale": {"type": "string"},
        },
        "required": ["name", "ticker", "rationale"],
    });
    json!({
        "type": "object",
        "properties": {
            "company_name": {"type": "string"},
            "suppliers": {"type": "array", "items": relation},
            "competitors": {"type": "array", "items": relation},
            "customers": {"type": "array", "items": relation},
            "note": {"type": "string"},
        },
        "required": ["company_name", "suppliers", "competitors", "customers", "note"],
    })
}

#[derive(Deserialize)]
struct MessagesResponse {
    #[serde(default)]
    content: Vec<ContentBlock>,
}

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum ContentBlock {
    Text {
        #[allow(dead_code)]
        text: String,
    },
    ToolUse {
        #[allow(dead_code)]
        name: String,
        input: Value,
    },
    #[serde(other)]
    Other,
}

fn parse_tool_input(v: Value) -> Result<SupplyChainData> {
    let obj = v
        .as_object()
        .ok_or_else(|| anyhow!("tool_use.input was not an object"))?;
    let company_name = obj
        .get("company_name")
        .and_then(|x| x.as_str())
        .unwrap_or_default()
        .to_string();
    let note = obj
        .get("note")
        .and_then(|x| x.as_str())
        .unwrap_or_default()
        .to_string();
    Ok(SupplyChainData {
        company_name,
        suppliers: extract_relations(obj.get("suppliers")),
        competitors: extract_relations(obj.get("competitors")),
        customers: extract_relations(obj.get("customers")),
        note,
    })
}

fn extract_relations(v: Option<&Value>) -> Vec<Relation> {
    let Some(arr) = v.and_then(|x| x.as_array()) else {
        return Vec::new();
    };
    arr.iter()
        .filter_map(|item| {
            let o = item.as_object()?;
            let name = o.get("name")?.as_str()?.to_string();
            let ticker = o
                .get("ticker")
                .and_then(|t| t.as_str())
                .filter(|s| !s.is_empty())
                .map(|s| s.to_ascii_uppercase());
            let rationale = o
                .get("rationale")
                .and_then(|r| r.as_str())
                .unwrap_or_default()
                .to_string();
            Some(Relation {
                name,
                ticker,
                rationale,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_canned_tool_use_response() {
        let json = r#"
        {
            "id": "msg_test",
            "type": "message",
            "role": "assistant",
            "model": "claude-haiku-4-5-20251001",
            "stop_reason": "tool_use",
            "content": [
                {
                    "type": "tool_use",
                    "id": "tu_01",
                    "name": "report_supply_chain",
                    "input": {
                        "company_name": "Apple Inc.",
                        "suppliers": [
                            {"name": "TSMC", "ticker": "TSM", "rationale": "Primary chip foundry for A-series and M-series silicon."},
                            {"name": "Foxconn", "ticker": null, "rationale": "Main contract manufacturer of iPhone."}
                        ],
                        "competitors": [
                            {"name": "Samsung Electronics", "ticker": "SSNLF", "rationale": "Smartphone and consumer electronics rival."}
                        ],
                        "customers": [],
                        "note": "Based on FY24 10-K; major customers list omitted as no single customer exceeded 10% of revenue."
                    }
                }
            ]
        }
        "#;
        let parsed: MessagesResponse = serde_json::from_str(json).unwrap();
        let tool_input = parsed
            .content
            .into_iter()
            .find_map(|b| match b {
                ContentBlock::ToolUse { input, .. } => Some(input),
                _ => None,
            })
            .unwrap();
        let data = parse_tool_input(tool_input).unwrap();
        assert_eq!(data.company_name, "Apple Inc.");
        assert_eq!(data.suppliers.len(), 2);
        assert_eq!(data.suppliers[0].ticker.as_deref(), Some("TSM"));
        assert!(data.suppliers[1].ticker.is_none());
        assert_eq!(data.competitors.len(), 1);
        assert!(data.customers.is_empty());
        assert!(data.note.contains("FY24"));
    }
}
