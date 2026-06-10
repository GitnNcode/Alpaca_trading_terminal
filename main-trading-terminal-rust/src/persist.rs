// Persistent app state — saved alongside credentials so a restart restores
// the user's last symbol / range / indicators / Compare slots / watchlist.
//
// Path: {OS config dir}/alpaca-tui/state.json (sibling of credentials.json).
// Forward-compat: every field is `#[serde(default)]`, so adding a new field
// in a future build never breaks an older state.json.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

/// Snapshot of user-visible state we restore on launch. Cheap to clone / diff —
/// the `ChartApp` snapshots it at the end of each frame and saves on change
/// after a 1-second quiescence (see `persist::SAVE_DEBOUNCE`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct AppState {
    /// Most recent symbol loaded on the Chart tab (uppercased).
    pub last_symbol: String,
    /// Index into `chart::RANGES` — defaults to 4 (1Y).
    pub range_idx: usize,
    /// Index into `chart::TFS` — defaults to 5 (1Day, matching RANGES[4].default_tf).
    pub tf_idx: usize,
    /// Chart-tab indicator toggles. Periods aren't persisted (yet) — there's
    /// no UI to change them, so they'd just round-trip the defaults.
    pub indicators: IndicatorPrefs,
    /// Compare tab symbols, in slot order.
    pub compare_slots: Vec<String>,
    /// Index into `compare::COMPARE_RANGES` — defaults to 1 (3Y).
    pub compare_range_idx: usize,
    /// Pinned tickers for the watchlist sidebar / ticker tape (Step 5).
    pub watchlist: Vec<String>,
    /// Whether the watchlist sidebar is hidden. Toggled from the sidebar's
    /// `«` button / the tab strip's ☰ WATCH toggle; restored on launch so a
    /// user who wants max chart real estate keeps it.
    pub watchlist_collapsed: bool,
    /// Last underlying loaded on the Options desk (uppercased). Restored so the
    /// chain is one keystroke away on relaunch; the chain itself loads lazily
    /// when the Options tab is first opened.
    pub last_underlying: String,
    /// Last Pair filter on the Crypto desk (slash form, e.g. "BTC/USD").
    /// Restored on launch; the Markets grid itself loads lazily on first
    /// Crypto-tab visit.
    pub last_pair: String,
}

impl Default for AppState {
    fn default() -> Self {
        AppState {
            last_symbol: String::new(),
            range_idx: 4,
            tf_idx: 5,
            indicators: IndicatorPrefs::default(),
            compare_slots: Vec::new(),
            compare_range_idx: 1,
            watchlist: Vec::new(),
            watchlist_collapsed: false,
            last_underlying: String::new(),
            last_pair: String::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct IndicatorPrefs {
    pub ema: bool,
    pub sma: bool,
    pub bollinger: bool,
    pub vwap: bool,
    pub volume: bool,
    pub rsi: bool,
    pub macd: bool,
}

impl Default for IndicatorPrefs {
    fn default() -> Self {
        // Mirrors Indicators::default() in src/app.rs — EMA + Volume on, rest off.
        IndicatorPrefs {
            ema: true,
            sma: false,
            bollinger: false,
            vwap: false,
            volume: true,
            rsi: false,
            macd: false,
        }
    }
}

/// How long the user must be idle (no state change) before we flush to disk.
/// Frame-to-frame change comparison is cheap; the debounce is just there so a
/// rapid drag across the range pills doesn't write the file ten times.
pub const SAVE_DEBOUNCE: std::time::Duration = std::time::Duration::from_secs(1);

pub fn state_path() -> Result<PathBuf> {
    let dir = dirs::config_dir().context("could not resolve OS config dir")?;
    Ok(dir.join("alpaca-tui").join("state.json"))
}

/// Load saved state, or return `AppState::default()` on any failure (missing
/// file / parse error / I/O error). State is non-critical — a clean first
/// launch is the right fallback if the file is unreadable.
pub fn load() -> AppState {
    state_path()
        .ok()
        .and_then(|p| fs::read_to_string(&p).ok())
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

pub fn save(state: &AppState) -> Result<()> {
    let path = state_path()?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let data = serde_json::to_string_pretty(state)?;
    fs::write(&path, data)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_state_round_trips_through_json() {
        let state = AppState::default();
        let json = serde_json::to_string(&state).unwrap();
        let parsed: AppState = serde_json::from_str(&json).unwrap();
        assert_eq!(state, parsed);
    }

    #[test]
    fn missing_fields_fall_back_to_defaults() {
        // An older / smaller JSON should still parse cleanly. The persist layer
        // is best-effort — losing a future field is fine; failing on it isn't.
        let json = "{}";
        let parsed: AppState = serde_json::from_str(json).unwrap();
        assert_eq!(parsed, AppState::default());
    }

    #[test]
    fn partial_json_keeps_other_defaults() {
        let json = r#"{"last_symbol":"NVDA","range_idx":5}"#;
        let parsed: AppState = serde_json::from_str(json).unwrap();
        assert_eq!(parsed.last_symbol, "NVDA");
        assert_eq!(parsed.range_idx, 5);
        assert_eq!(parsed.tf_idx, AppState::default().tf_idx);
        assert_eq!(parsed.indicators, IndicatorPrefs::default());
    }

    #[test]
    fn unknown_fields_dont_break_deserialize() {
        // serde defaults to ignoring unknown fields, but assert it explicitly
        // so a future `deny_unknown_fields` annotation doesn't silently regress
        // older builds reading newer state.json files.
        let json = r#"{"last_symbol":"AAPL","some_future_field":42}"#;
        let parsed: AppState = serde_json::from_str(json).unwrap();
        assert_eq!(parsed.last_symbol, "AAPL");
    }
}
