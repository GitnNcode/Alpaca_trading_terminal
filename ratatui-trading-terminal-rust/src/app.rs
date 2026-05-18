use std::sync::mpsc::Sender;
use std::sync::Arc;
use std::time::Instant;

use chrono::{DateTime, Datelike, Duration as ChronoDuration, TimeZone, Utc};
use ratatui::layout::Rect;
use ratatui::style::Color;

use crate::api::{Activity, AlpacaClient, Bar, Order, OrderRequest, Position};
use crate::stocks::AssetCache;
use crate::theme::*;

// ── Tab indices ───────────────────────────────────────────────────────────────
pub const TAB_POSITIONS: usize = 0;
pub const TAB_TRADE: usize = 1;
pub const TAB_ORDERS: usize = 2;
pub const TAB_ACTIVITY: usize = 3;
pub const TAB_CHART: usize = 4;
pub const TAB_COUNT: usize = 5;

pub const TAB_LABELS: [&str; TAB_COUNT] = [
    " [1] POSITIONS ",
    " [2] TRADE ",
    " [3] ORDERS ",
    " [4] ACTIVITY ",
    " [5] CHART ",
];

// ── Background messages ───────────────────────────────────────────────────────

pub enum Msg {
    Positions(anyhow::Result<Vec<Position>>),
    Account(anyhow::Result<crate::api::Account>),
    Orders(anyhow::Result<Vec<Order>>),
    Activities(
        anyhow::Result<Vec<Activity>>,
        anyhow::Result<Vec<Order>>,
    ),
    Assets(anyhow::Result<Vec<crate::api::Asset>>),
    Bars {
        symbol: String,
        range_idx: usize,
        tf_idx: usize,
        bars: anyhow::Result<Vec<Bar>>,
    },
    OrderPlaced(anyhow::Result<Order>, OrderRequest),
    OrderCanceled(anyhow::Result<()>, String),
}

// ── Trade form ────────────────────────────────────────────────────────────────

#[derive(Copy, Clone, PartialEq, Eq)]
pub enum TradeField {
    Action,
    Type,
    Symbol,
    Qty,
    Price,
    Place,
    Clear,
}

impl TradeField {
    pub fn next(self) -> Self {
        match self {
            TradeField::Action => TradeField::Type,
            TradeField::Type => TradeField::Symbol,
            TradeField::Symbol => TradeField::Qty,
            TradeField::Qty => TradeField::Price,
            TradeField::Price => TradeField::Place,
            TradeField::Place => TradeField::Clear,
            TradeField::Clear => TradeField::Action,
        }
    }
    pub fn prev(self) -> Self {
        match self {
            TradeField::Action => TradeField::Clear,
            TradeField::Type => TradeField::Action,
            TradeField::Symbol => TradeField::Type,
            TradeField::Qty => TradeField::Symbol,
            TradeField::Price => TradeField::Qty,
            TradeField::Place => TradeField::Price,
            TradeField::Clear => TradeField::Place,
        }
    }
}

pub struct TradeForm {
    pub action_idx: usize, // 0 BUY, 1 SELL
    pub type_idx: usize,   // 0 MARKET, 1 LIMIT
    pub symbol: String,
    pub qty: String,
    pub price: String,
    pub focus: TradeField,
    pub autocomplete: Autocomplete,
}

impl TradeForm {
    pub fn new() -> Self {
        TradeForm {
            action_idx: 0,
            type_idx: 0,
            symbol: String::new(),
            qty: String::new(),
            price: String::new(),
            focus: TradeField::Action,
            autocomplete: Autocomplete::new(),
        }
    }

    pub fn action_str(&self) -> &'static str {
        if self.action_idx == 0 {
            "BUY"
        } else {
            "SELL"
        }
    }
    pub fn type_str(&self) -> &'static str {
        if self.type_idx == 0 {
            "MARKET"
        } else {
            "LIMIT"
        }
    }
}

// ── Autocomplete shared state ─────────────────────────────────────────────────

pub struct Autocomplete {
    pub open: bool,
    pub items: Vec<(String, String)>, // (symbol, company)
    pub selected: usize,
}

impl Autocomplete {
    pub fn new() -> Self {
        Autocomplete {
            open: false,
            items: Vec::new(),
            selected: 0,
        }
    }
    pub fn close(&mut self) {
        self.open = false;
        self.items.clear();
        self.selected = 0;
    }
    pub fn refresh(&mut self, prefix: &str, cache: &AssetCache) {
        if prefix.is_empty() {
            self.close();
            return;
        }
        let items = cache.filter(prefix, 10);
        if items.is_empty() {
            self.close();
            return;
        }
        self.items = items;
        if self.selected >= self.items.len() {
            self.selected = 0;
        }
        self.open = true;
    }
}

// ── Chart state ───────────────────────────────────────────────────────────────

#[derive(Clone)]
pub struct ChartRange {
    pub label: &'static str,
    pub hotkey: char,
    pub default_tf: usize,
    pub lookback_hours: i64, // 0 means YTD
    pub ytd: bool,
    pub date_fmt: &'static str,
}

#[derive(Clone)]
pub struct ChartTimeframe {
    pub label: &'static str,
    pub value: &'static str,
}

pub const CHART_RANGES: &[ChartRange] = &[
    ChartRange { label: "1D",  hotkey: 'd', default_tf: 1, lookback_hours: 24,           ytd: false, date_fmt: "%H:%M" },
    ChartRange { label: "1W",  hotkey: 'w', default_tf: 3, lookback_hours: 24 * 7,       ytd: false, date_fmt: "%m/%d" },
    ChartRange { label: "1M",  hotkey: 'm', default_tf: 5, lookback_hours: 24 * 31,      ytd: false, date_fmt: "%m/%d" },
    ChartRange { label: "YTD", hotkey: 't', default_tf: 5, lookback_hours: 0,            ytd: true,  date_fmt: "%m/%d" },
    ChartRange { label: "1Y",  hotkey: 'y', default_tf: 5, lookback_hours: 24 * 365,     ytd: false, date_fmt: "%m/%y" },
    ChartRange { label: "5Y",  hotkey: 'f', default_tf: 6, lookback_hours: 24 * 365 * 5, ytd: false, date_fmt: "%m/%y" },
    ChartRange { label: "MAX", hotkey: 'x', default_tf: 7, lookback_hours: 24 * 365 * 30,ytd: false, date_fmt: "%m/%y" },
];

pub const CHART_TFS: &[ChartTimeframe] = &[
    ChartTimeframe { label: "1m",  value: "1Min" },
    ChartTimeframe { label: "5m",  value: "5Min" },
    ChartTimeframe { label: "15m", value: "15Min" },
    ChartTimeframe { label: "30m", value: "30Min" },
    ChartTimeframe { label: "1h",  value: "1Hour" },
    ChartTimeframe { label: "1D",  value: "1Day" },
    ChartTimeframe { label: "1W",  value: "1Week" },
    ChartTimeframe { label: "1M",  value: "1Month" },
];

pub fn chart_start_time(r: &ChartRange, now: DateTime<Utc>) -> DateTime<Utc> {
    if r.ytd {
        Utc.with_ymd_and_hms(now.year(), 1, 1, 0, 0, 0).unwrap()
    } else {
        now - ChronoDuration::hours(r.lookback_hours)
    }
}

pub enum ChartFocus {
    Symbol,
    Canvas,
}

/// Toggleable indicator state. Periods are TradingView defaults; everything
/// starts off so the user gets a clean candle chart by default and opts in
/// to overlays / sub-panels.
pub struct IndicatorState {
    // Overlays (drawn on the price panel)
    pub ema: bool,
    pub ema_period: usize,
    pub sma: bool,
    pub sma_period: usize,
    pub bollinger: bool,
    pub bollinger_period: usize,
    pub bollinger_mult: f64,
    pub vwap: bool,
    // Sub-panels (stacked below price)
    pub volume: bool,
    pub rsi: bool,
    pub rsi_period: usize,
    pub macd: bool,
    pub macd_fast: usize,
    pub macd_slow: usize,
    pub macd_signal: usize,
}

impl IndicatorState {
    pub fn new() -> Self {
        IndicatorState {
            ema: true,
            ema_period: 10,
            sma: false,
            sma_period: 20,
            bollinger: false,
            bollinger_period: 20,
            bollinger_mult: 2.0,
            vwap: false,
            volume: false,
            rsi: false,
            rsi_period: 14,
            macd: false,
            macd_fast: 12,
            macd_slow: 26,
            macd_signal: 9,
        }
    }
}

pub struct ChartState {
    pub symbol_input: String,
    pub current_symbol: String,
    pub range_idx: usize,
    pub tf_idx: usize,
    pub focus: ChartFocus,
    pub autocomplete: Autocomplete,
    pub bars: Vec<Bar>,
    pub loading: bool,
    pub err: String,
    pub scroll_offset: usize,
    // Visible window — recomputed by chart renderer each frame
    pub visible_start: usize,
    pub visible_end: usize,
    pub visible_step: usize,
    // Indicator toggles + parameters
    pub indicators: IndicatorState,
    // Mouse hover position (in absolute terminal coordinates) for the
    // crosshair overlay. None while the cursor is outside the canvas.
    pub hover: Option<(u16, u16)>,
}

impl ChartState {
    pub fn new() -> Self {
        // Default to 1Y / 1Day — quick to load, broadly useful first view.
        let range_idx = 4;
        let tf_idx = CHART_RANGES[range_idx].default_tf;
        ChartState {
            symbol_input: String::new(),
            current_symbol: String::new(),
            range_idx,
            tf_idx,
            focus: ChartFocus::Symbol,
            autocomplete: Autocomplete::new(),
            bars: Vec::new(),
            loading: false,
            err: String::new(),
            scroll_offset: 0,
            visible_start: 0,
            visible_end: 0,
            visible_step: 1,
            indicators: IndicatorState::new(),
            hover: None,
        }
    }
}

// ── Modals ────────────────────────────────────────────────────────────────────

pub enum Modal {
    PlaceOrder {
        req: OrderRequest,
        focus_confirm: bool,
    },
    CancelOrder {
        order_id: String,
        symbol: String,
        focus_cancel: bool,
    },
}

// ── Layout-tracking rects (updated by `ui::draw` each frame, read by mouse handler) ──

#[derive(Default, Clone)]
pub struct LayoutRects {
    pub tab_bar: Rect,
    pub tab_hits: Vec<(u16, u16)>, // absolute screen column ranges per tab

    pub positions_table: Rect,
    pub orders_table: Rect,
    pub activity_table: Rect,

    pub trade_action: Rect,
    pub trade_type: Rect,
    pub trade_symbol: Rect,
    pub trade_qty: Rect,
    pub trade_price: Rect,
    pub trade_place: Rect,
    pub trade_clear: Rect,

    pub chart_symbol_input: Rect,
    pub chart_range_bar: Rect,
    pub chart_range_hits: Vec<(u16, u16)>,
    pub chart_tf_bar: Rect,
    pub chart_tf_hits: Vec<(u16, u16)>,
    pub chart_ind_bar: Rect,
    /// Hit ranges for each toggleable indicator on the INDICATORS row, in the
    /// same order as `IND_TOGGLES` in ui.rs.
    pub chart_ind_hits: Vec<(u16, u16)>,
    pub chart_canvas: Rect,

    pub modal_left_btn: Rect,
    pub modal_right_btn: Rect,
}

// ── App state ─────────────────────────────────────────────────────────────────

pub struct App {
    pub client: Arc<AlpacaClient>,
    pub assets: Arc<AssetCache>,
    pub tx: Sender<Msg>,

    pub active_tab: usize,
    pub account: crate::api::Account,
    pub positions: Vec<Position>,
    pub orders: Vec<Order>,
    pub activity_rows: Vec<ActRow>,

    pub pos_selected: usize,
    pub orders_selected: usize,
    pub activity_selected: usize,

    pub trade: TradeForm,
    pub chart: ChartState,

    pub result_msg: String,
    pub result_color: Color,

    pub modal: Option<Modal>,

    // Auto-refresh ticking (40 ticks * 250ms = 10s)
    pub tick: u32,
    pub spinner_idx: usize,

    pub should_quit: bool,

    pub layout: LayoutRects,
    // Last left-click for double-click detection (used on positions table).
    pub last_click: Option<(Instant, u16, u16)>,
}

impl App {
    pub fn new(client: Arc<AlpacaClient>, assets: Arc<AssetCache>, tx: Sender<Msg>) -> Self {
        App {
            client,
            assets,
            tx,
            active_tab: TAB_POSITIONS,
            account: Default::default(),
            positions: Vec::new(),
            orders: Vec::new(),
            activity_rows: Vec::new(),
            pos_selected: 0,
            orders_selected: 0,
            activity_selected: 0,
            trade: TradeForm::new(),
            chart: ChartState::new(),
            result_msg: String::new(),
            result_color: WHITE,
            modal: None,
            tick: 0,
            spinner_idx: 0,
            should_quit: false,
            layout: LayoutRects::default(),
            last_click: None,
        }
    }

    pub fn set_result(&mut self, msg: &str, color: Color) {
        self.result_msg = msg.to_string();
        self.result_color = color;
    }

    pub fn switch_tab(&mut self, tab: usize) {
        if tab >= TAB_COUNT {
            return;
        }
        self.active_tab = tab;
    }
}

// ── Activity row (unified across activities + closed orders) ──────────────────

pub struct ActRow {
    pub when: Option<DateTime<Utc>>,
    pub type_str: String,
    pub symbol: String,
    pub dir: String,
    pub qty: String,
    pub price: String,
    pub amount: String,
    pub detail: String,
    pub type_color: Color,
    pub dir_color: Color,
    pub amount_color: Color,
}

pub fn activity_to_row(a: &Activity) -> ActRow {
    let when = a.transaction_time.or_else(|| {
        a.date.as_ref().and_then(|d| {
            chrono::NaiveDate::parse_from_str(d, "%Y-%m-%d")
                .ok()
                .map(|nd| {
                    nd.and_hms_opt(0, 0, 0)
                        .unwrap()
                        .and_local_timezone(Utc)
                        .unwrap()
                })
        })
    });
    let symbol = a.symbol.clone().unwrap_or_default();
    let mut row = ActRow {
        when,
        type_str: String::new(),
        symbol,
        dir: String::new(),
        qty: a.qty.clone().unwrap_or_default(),
        price: String::new(),
        amount: String::new(),
        detail: String::new(),
        type_color: GRAY2,
        dir_color: WHITE,
        amount_color: WHITE,
    };

    let activity_type = a.activity_type.as_str();
    match activity_type {
        "FILL" | "" => {
            let partial = a.fill_type.as_deref() == Some("partial_fill");
            if partial {
                row.type_str = "PART FILL".into();
                row.type_color = YELLOW;
            } else {
                row.type_str = "FILL".into();
                row.type_color = GREEN;
            }
            let side = a.side.clone().unwrap_or_default();
            row.dir = side.to_ascii_uppercase();
            row.dir_color = if side.eq_ignore_ascii_case("buy") { CYAN } else { RED };
            row.qty = a.qty.clone().unwrap_or_default();
            if let Some(p) = &a.price {
                row.price = format!("${}", fmt_price(p));
            }
            if let (Some(q), Some(p)) = (a.qty.as_ref(), a.price.as_ref()) {
                if let (Ok(qf), Ok(pf)) = (q.parse::<f64>(), p.parse::<f64>()) {
                    row.amount = format!("${:.2}", qf * pf);
                    row.amount_color = if side.eq_ignore_ascii_case("buy") { RED } else { GREEN };
                }
            }
            row.detail = a
                .order_id
                .clone()
                .map(|s| s.chars().take(8).collect())
                .unwrap_or_default();
        }
        "DIV" | "DIVNRA" | "DIVROC" | "DIVTXEX" | "CSD" => {
            row.type_str = activity_type.to_string();
            row.type_color = GREEN;
            row.dir = "CREDIT".into();
            row.dir_color = GREEN;
            if let Some(ps) = &a.per_share_amount {
                row.price = format!("${}/sh", fmt_price(ps));
            }
            if let Some(net) = &a.net_amount {
                row.amount = format!("${}", fmt_price(net));
                row.amount_color = GREEN;
            }
        }
        "JNLC" | "JNLS" => {
            row.type_str = "JOURNAL".into();
            row.type_color = YELLOW;
            if let Some(net) = a.net_amount.as_ref().and_then(|s| s.parse::<f64>().ok()) {
                if net >= 0.0 {
                    row.dir = "CREDIT".into();
                    row.dir_color = GREEN;
                    row.amount = format!("${:.2}", net);
                    row.amount_color = GREEN;
                } else {
                    row.dir = "DEBIT".into();
                    row.dir_color = RED;
                    row.amount = format!("-${:.2}", -net);
                    row.amount_color = RED;
                }
            }
            row.detail = a.description.clone().unwrap_or_default();
        }
        "CSW" => {
            row.type_str = "WITHDRAW".into();
            row.type_color = ORANGE;
            row.dir = "DEBIT".into();
            row.dir_color = RED;
            if let Some(net) = a.net_amount.as_ref().and_then(|s| s.parse::<f64>().ok()) {
                row.amount = format!("-${:.2}", -net);
                row.amount_color = RED;
            }
        }
        "ACATC" | "ACATU" => {
            row.type_str = "ACAT".into();
            row.type_color = CYAN;
            row.dir = "TRANSFER".into();
            row.dir_color = CYAN;
            if let Some(net) = &a.net_amount {
                row.amount = format!("${}", fmt_price(net));
                row.amount_color = CYAN;
            }
        }
        "PTC" => {
            row.type_str = "CHARGE".into();
            row.type_color = RED;
            row.dir = "DEBIT".into();
            row.dir_color = RED;
            if let Some(net) = a.net_amount.as_ref().and_then(|s| s.parse::<f64>().ok()) {
                row.amount = format!("-${:.2}", -net);
                row.amount_color = RED;
            }
        }
        "REORG" => {
            row.type_str = "REORG".into();
            row.type_color = YELLOW;
            row.qty = a.qty.clone().unwrap_or_default();
        }
        other => {
            row.type_str = other.to_string();
            row.type_color = GRAY2;
            if let Some(net) = &a.net_amount {
                row.amount = format!("${}", fmt_price(net));
                if let Ok(v) = net.parse::<f64>() {
                    row.amount_color = if v >= 0.0 { GREEN } else { RED };
                }
            }
            row.detail = a.description.clone().unwrap_or_default();
        }
    }

    row
}

pub fn closed_order_to_row(o: &Order) -> ActRow {
    let side = o.side.to_ascii_uppercase();
    let dir_color = if o.side.eq_ignore_ascii_case("buy") { CYAN } else { RED };
    let mut row = ActRow {
        when: Some(o.created_at),
        type_str: String::new(),
        symbol: o.symbol.clone(),
        dir: side,
        qty: o.qty.clone(),
        price: String::new(),
        amount: String::new(),
        detail: o.id.chars().take(8).collect(),
        type_color: GRAY2,
        dir_color,
        amount_color: WHITE,
    };
    let lp = o.limit_price_str();
    if !lp.is_empty() && lp != "0" {
        row.price = format!("${}", fmt_price(lp));
    } else {
        row.price = "MARKET".into();
    }
    match o.status.to_ascii_lowercase().as_str() {
        "filled" => {
            row.type_str = "FILLED".into();
            row.type_color = GREEN;
            let fap = o.filled_avg_price_str();
            if !fap.is_empty() {
                row.price = format!("${}", fmt_price(fap));
            }
            if let (Ok(q), Ok(p)) = (o.filled_qty.parse::<f64>(), fap.parse::<f64>()) {
                if p > 0.0 {
                    row.amount = format!("${:.2}", q * p);
                    row.amount_color = if o.side.eq_ignore_ascii_case("buy") { RED } else { GREEN };
                }
            }
        }
        "partially_filled" => {
            row.type_str = "PART FILLED".into();
            row.type_color = YELLOW;
            let fap = o.filled_avg_price_str();
            if !fap.is_empty() {
                row.price = format!("${}", fmt_price(fap));
            }
        }
        "canceled" => {
            row.type_str = "CANCELLED".into();
            row.type_color = GRAY2;
        }
        "expired" => {
            row.type_str = "EXPIRED".into();
            row.type_color = GRAY;
        }
        "rejected" => {
            row.type_str = "REJECTED".into();
            row.type_color = RED;
        }
        "held" => {
            row.type_str = "HELD".into();
            row.type_color = YELLOW;
        }
        s => {
            row.type_str = s.to_ascii_uppercase();
            row.type_color = GRAY2;
        }
    }
    row
}

pub fn fmt_price(s: &str) -> String {
    match s.parse::<f64>() {
        Ok(f) => format!("{:.2}", f),
        Err(_) => s.to_string(),
    }
}

pub fn fmt_money(s: &str) -> String {
    if s.is_empty() {
        return "---".to_string();
    }
    match s.parse::<f64>() {
        Ok(f) => format!("${:.2}", f),
        Err(_) => s.to_string(),
    }
}

pub fn fmt_volume(v: i64) -> String {
    if v >= 1_000_000_000 {
        format!("{:.2}B", v as f64 / 1e9)
    } else if v >= 1_000_000 {
        format!("{:.2}M", v as f64 / 1e6)
    } else if v >= 1_000 {
        format!("{:.2}K", v as f64 / 1e3)
    } else {
        v.to_string()
    }
}
