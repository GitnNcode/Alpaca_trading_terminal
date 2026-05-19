use std::collections::HashMap;
use std::sync::RwLock;

use crate::api::Asset;

pub struct AssetCache {
    symbols: RwLock<Vec<String>>,         // sorted, for binary-search prefix lookup
    names: RwLock<HashMap<String, String>>, // symbol -> company name
}

impl AssetCache {
    pub fn new() -> Self {
        AssetCache {
            symbols: RwLock::new(Vec::new()),
            names: RwLock::new(HashMap::new()),
        }
    }

    pub fn load(&self, assets: Vec<Asset>) {
        let mut syms: Vec<String> = Vec::with_capacity(assets.len());
        let mut names: HashMap<String, String> = HashMap::with_capacity(assets.len());
        for a in assets {
            if a.tradable {
                syms.push(a.symbol.clone());
                names.insert(a.symbol, a.name);
            }
        }
        syms.sort();
        *self.symbols.write().unwrap() = syms;
        *self.names.write().unwrap() = names;
    }

    pub fn company_name(&self, sym: &str) -> String {
        self.names
            .read()
            .unwrap()
            .get(sym)
            .cloned()
            .unwrap_or_default()
    }

    /// Returns up to `limit` "SYMBOL  Company Name" suggestions matching `prefix`.
    /// Fast path: binary-search ticker prefix. Slow path: substring of company name.
    pub fn filter(&self, prefix: &str, limit: usize) -> Vec<(String, String)> {
        let prefix = prefix.to_ascii_uppercase();
        let symbols = self.symbols.read().unwrap();
        let names = self.names.read().unwrap();
        if symbols.is_empty() || prefix.is_empty() {
            return Vec::new();
        }

        let mut out: Vec<(String, String)> = Vec::with_capacity(limit);
        let mut seen: std::collections::HashSet<&str> = std::collections::HashSet::new();

        let start = symbols.partition_point(|s| s.as_str() < prefix.as_str());
        for sym in &symbols[start..] {
            if !sym.starts_with(&prefix) || out.len() >= limit {
                break;
            }
            seen.insert(sym.as_str());
            let mut name = names.get(sym).cloned().unwrap_or_default();
            if name.chars().count() > 38 {
                name = format!("{}…", name.chars().take(35).collect::<String>());
            }
            out.push((sym.clone(), name));
        }

        if out.len() < limit {
            let lower = prefix.to_ascii_lowercase();
            for sym in symbols.iter() {
                if out.len() >= limit {
                    break;
                }
                if seen.contains(sym.as_str()) {
                    continue;
                }
                let name = names.get(sym).cloned().unwrap_or_default();
                if name.to_ascii_lowercase().contains(&lower) {
                    let display = if name.chars().count() > 38 {
                        format!("{}…", name.chars().take(35).collect::<String>())
                    } else {
                        name
                    };
                    out.push((sym.clone(), display));
                }
            }
        }
        out
    }
}
