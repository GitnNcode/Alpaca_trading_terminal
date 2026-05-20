// ChartApp — tabbed egui app. Two tabs: the canonical multi-pane chart and a
// multi-asset Compare view. Order entry / positions / activity live in the
// other ports (tview, bt_port); this build stays focused on analysis.

use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Arc;

use eframe::App as EApp;
use egui::{Key, RichText};

use crate::api::{AlpacaClient, Bar};
use crate::chart;
use crate::command::{self as cmd, Command, Page, Side as CmdSide};
use crate::compare::{CompareState, COMPARE_RANGES};
use crate::persist;
use crate::stocks::AssetCache;
use crate::stream::{self, SubMsg, TickCache};
use crate::terminal::TerminalState;
use crate::theme;
use crate::watchlist::WatchlistState;
use crate::workers::{self, Msg};

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Tab {
    Chart,
    Compare,
    Terminal,
}

/// Indicator toggles + their (default) periods. Matches the tview/ratatui
/// builds so the muscle memory carries over.
pub struct Indicators {
    pub ema: bool,
    pub ema_period: usize,
    pub sma: bool,
    pub sma_period: usize,
    pub bollinger: bool,
    pub bollinger_period: usize,
    pub bollinger_mult: f64,
    pub vwap: bool,
    pub volume: bool,
    pub rsi: bool,
    pub rsi_period: usize,
    pub macd: bool,
    pub macd_fast: usize,
    pub macd_slow: usize,
    pub macd_signal: usize,
}

impl Default for Indicators {
    fn default() -> Self {
        Indicators {
            ema: true,
            ema_period: 10,
            sma: false,
            sma_period: 20,
            bollinger: false,
            bollinger_period: 20,
            bollinger_mult: 2.0,
            vwap: false,
            volume: true, // visible by default — volume is a TradingView baseline
            rsi: false,
            rsi_period: 14,
            macd: false,
            macd_fast: 12,
            macd_slow: 26,
            macd_signal: 9,
        }
    }
}

/// Identifies which indicator is selected when exactly one is active. Used
/// by the strategy toggle to know which buy/sell rule to apply.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum ActiveIndicator {
    Ema,
    Sma,
    Bollinger,
    Vwap,
    Rsi,
    Macd,
}

impl Indicators {
    pub fn count_active(&self) -> usize {
        (self.ema as usize)
            + (self.sma as usize)
            + (self.bollinger as usize)
            + (self.vwap as usize)
            + (self.volume as usize)
            + (self.rsi as usize)
            + (self.macd as usize)
    }

    /// Returns the single active indicator if exactly one is selected AND it
    /// has a defined strategy. Volume-only returns None — there's no
    /// canonical Buy/Sell rule for "just volume" so the strategy toggle
    /// stays hidden in that case.
    pub fn only_active_with_strategy(&self) -> Option<ActiveIndicator> {
        if self.count_active() != 1 {
            return None;
        }
        if self.ema {
            Some(ActiveIndicator::Ema)
        } else if self.sma {
            Some(ActiveIndicator::Sma)
        } else if self.bollinger {
            Some(ActiveIndicator::Bollinger)
        } else if self.vwap {
            Some(ActiveIndicator::Vwap)
        } else if self.rsi {
            Some(ActiveIndicator::Rsi)
        } else if self.macd {
            Some(ActiveIndicator::Macd)
        } else {
            None // Volume-only
        }
    }
}

pub struct ChartApp {
    pub client: Arc<AlpacaClient>,
    pub assets: Arc<AssetCache>,
    pub tx: Sender<Msg>,
    rx: Receiver<Msg>,

    pub symbol_input: String,
    pub current_symbol: String,
    pub range_idx: usize,
    pub tf_idx: usize,
    pub bars: Vec<Bar>,
    pub loading: bool,
    pub err: String,
    pub gen: u64,
    pub indicators: Indicators,

    pub autocomplete: Vec<(String, String)>,
    pub autocomplete_open: bool,

    // Independent X/Y zoom controls. egui_plot's allow_zoom accepts a Vec2b
    // so we expose each axis as its own toggle. Default: zoom X (the time
    // axis) but not Y — matches TradingView's "scroll the chart through
    // time" feel where price autoscales.
    pub zoom_x: bool,
    pub zoom_y: bool,

    // Strategy mode: when exactly one indicator is active AND this flag is
    // on, the chart shows Buy/Sell markers from that indicator's classic
    // trading strategy (see src/strategies.rs).
    pub strategy_enabled: bool,

    pub current_tab: Tab,
    pub compare: CompareState,
    pub terminal: TerminalState,
    /// Tracks whether the Terminal tab has been opened at least once — used
    /// to kick off the initial /account, /positions, /orders, /activities
    /// fetches lazily (no point spending API calls on a tab the user hasn't
    /// visited).
    pub terminal_primed: bool,

    /// Last `AppState` we serialized to disk. We diff against this at the end
    /// of every frame to decide whether to save; cheap struct comparison.
    pub last_saved_state: persist::AppState,
    /// When did the live state first diverge from `last_saved_state`?
    /// `None` ⇒ nothing to save. We flush once `elapsed >= SAVE_DEBOUNCE`,
    /// matching the "1 second of quiescence after the last change" rule.
    pub state_dirty_since: Option<std::time::Instant>,

    /// Live tick stream — shared with the WS thread. Read-only from the UI:
    /// `tick_cache.read().unwrap().get(symbol)` returns the latest known
    /// last/bid/ask/bar for any subscribed symbol. Updates trigger a
    /// `ctx.request_repaint()` from the WS thread so the UI wakes up.
    pub tick_cache: TickCache,
    /// Outbound channel to the WS thread — push the full desired subscribe
    /// set whenever it changes. The thread diffs and emits the right
    /// subscribe/unsubscribe frames.
    pub stream_tx: std::sync::mpsc::Sender<SubMsg>,
    /// Most recent subscription set we sent. Compared against the freshly
    /// computed set each frame so we only push when something changes.
    pub last_subscribed: std::collections::HashSet<String>,
    /// Live connection state of the WS, reported by the stream thread via
    /// `Msg::StreamStatus`. Surfaced in the top tab strip until the proper
    /// status bar lands (Tier 3 / Step ?).
    pub stream_connected: bool,

    /// Pinned watchlist symbols + transient sidebar state (input/edit
    /// mode). The symbol list is persisted via `snapshot_state`.
    pub watchlist: WatchlistState,

    /// Bloomberg-style command palette text. `/` focuses; Enter dispatches.
    pub command_input: String,
    /// Set on the frame `/` is pressed; the palette TextEdit calls
    /// `response.request_focus()` and clears the flag.
    pub command_focus_requested: bool,
    /// If a parse failed or returned `Unknown`, surface a short error
    /// underneath the palette. Cleared on the next successful dispatch.
    pub command_error: Option<String>,
    /// `?` / HELP toggles a help overlay.
    pub command_help_open: bool,
}

impl ChartApp {
    pub fn new(ctx: &egui::Context, client: Arc<AlpacaClient>) -> Self {
        theme::apply(ctx);
        let (tx, rx) = mpsc::channel();
        let assets = Arc::new(AssetCache::new());
        workers::spawn_assets(client.clone(), tx.clone(), ctx.clone());

        // Live data stream — one long-running thread handles auth +
        // subscriptions + reconnects. Ticks land in the shared cache; only
        // connection status flows through the Msg channel.
        let tick_cache = stream::new_tick_cache();
        let stream_tx = stream::spawn_stream(
            client.clone(),
            tx.clone(),
            ctx.clone(),
            tick_cache.clone(),
        );

        // Restore the saved state from disk (or fall back to defaults if no
        // state.json exists yet / it's unreadable). Clamp the persisted
        // indices so a state file from a future build that grew the RANGES /
        // TFS arrays can't index out-of-bounds in older binaries.
        let saved = persist::load();
        let range_idx = saved.range_idx.min(chart::RANGES.len() - 1);
        let tf_idx = saved.tf_idx.min(chart::TFS.len() - 1);
        let mut indicators = Indicators::default();
        indicators.ema = saved.indicators.ema;
        indicators.sma = saved.indicators.sma;
        indicators.bollinger = saved.indicators.bollinger;
        indicators.vwap = saved.indicators.vwap;
        indicators.volume = saved.indicators.volume;
        indicators.rsi = saved.indicators.rsi;
        indicators.macd = saved.indicators.macd;

        let mut compare = CompareState::new();
        compare.range_idx = saved.compare_range_idx.min(COMPARE_RANGES.len() - 1);
        // Kick a background load for each persisted Compare slot — same code
        // path as the user adding them by hand. The Compare tab will show
        // "loading…" until each response lands.
        for sym in &saved.compare_slots {
            compare.add_symbol(sym.clone(), client.clone(), tx.clone(), ctx);
        }

        let mut app = ChartApp {
            client: client.clone(),
            assets,
            tx: tx.clone(),
            rx,
            symbol_input: saved.last_symbol.clone(),
            current_symbol: String::new(),
            range_idx,
            tf_idx,
            bars: Vec::new(),
            loading: false,
            err: String::new(),
            gen: 0,
            indicators,
            autocomplete: Vec::new(),
            autocomplete_open: false,
            zoom_x: true,
            zoom_y: false,
            strategy_enabled: false,
            current_tab: Tab::Terminal,
            compare,
            terminal: TerminalState::new(),
            terminal_primed: false,
            last_saved_state: saved.clone(),
            state_dirty_since: None,
            tick_cache,
            stream_tx,
            last_subscribed: std::collections::HashSet::new(),
            stream_connected: false,
            watchlist: WatchlistState::from_saved(&saved.watchlist),
            command_input: String::new(),
            command_focus_requested: false,
            command_error: None,
            command_help_open: false,
        };

        // If we restored a non-empty symbol, kick its bars load so the Chart
        // tab is already populated when the user navigates to it.
        if !saved.last_symbol.is_empty() {
            app.kick_off_load(ctx);
        }

        app
    }

    /// Materialise the current persistable state. Cheap struct build — only
    /// stores the *user-visible* surface (no bars, no loading flags, no
    /// errors).
    pub fn snapshot_state(&self) -> persist::AppState {
        persist::AppState {
            last_symbol: self.current_symbol.clone(),
            range_idx: self.range_idx,
            tf_idx: self.tf_idx,
            indicators: persist::IndicatorPrefs {
                ema: self.indicators.ema,
                sma: self.indicators.sma,
                bollinger: self.indicators.bollinger,
                vwap: self.indicators.vwap,
                volume: self.indicators.volume,
                rsi: self.indicators.rsi,
                macd: self.indicators.macd,
            },
            compare_slots: self.compare.slots.iter().map(|s| s.symbol.clone()).collect(),
            compare_range_idx: self.compare.range_idx,
            watchlist: self.watchlist.symbols.clone(),
        }
    }

    /// Apply a parsed command. Mutates ChartApp / Compare / Terminal /
    /// Watchlist as appropriate; sets `command_error` if the command was
    /// `Unknown`. Kept separate from the parser so the parser stays pure
    /// and testable.
    fn dispatch_command(&mut self, c: Command, ctx: &egui::Context) {
        self.command_error = None;
        match c {
            Command::Noop => {}
            Command::Help => self.command_help_open = true,
            Command::LoadSymbol(sym) => {
                self.symbol_input = sym;
                self.current_tab = Tab::Chart;
                self.kick_off_load(ctx);
            }
            Command::Compare(syms) => {
                // Replace all slots — fastest way to reset is remove then
                // add. add_symbol uppercases and dedups internally.
                while !self.compare.slots.is_empty() {
                    self.compare.remove_slot(0);
                }
                for s in syms {
                    self.compare.add_symbol(s, self.client.clone(), self.tx.clone(), ctx);
                }
                self.current_tab = Tab::Compare;
            }
            Command::GoTo(page) => {
                self.current_tab = Tab::Terminal;
                self.terminal.sub_tab = match page {
                    Page::Positions => crate::terminal::SubTab::Positions,
                    Page::TradeForm => crate::terminal::SubTab::Trade,
                    Page::Orders => crate::terminal::SubTab::Orders,
                    Page::Activity => crate::terminal::SubTab::Activity,
                };
            }
            Command::Trade(intent) => {
                self.current_tab = Tab::Terminal;
                self.terminal.sub_tab = crate::terminal::SubTab::Trade;
                if let Some(s) = intent.symbol {
                    self.terminal.form.symbol_input = s.clone();
                    self.terminal.pending_chart_load = Some(s);
                }
                if let Some(q) = intent.qty {
                    self.terminal.form.qty_input = q;
                }
                if let Some(side) = intent.side {
                    self.terminal.form.side = match side {
                        CmdSide::Buy => crate::terminal::TradeSide::Buy,
                        CmdSide::Sell => crate::terminal::TradeSide::Sell,
                    };
                    // BUY/SELL implies market — the user can flip to limit
                    // manually once on the form.
                    self.terminal.form.kind = crate::terminal::OrderKind::Market;
                }
            }
            Command::AddToWatchlist(sym) => {
                self.watchlist.add(&sym);
            }
            Command::Unknown(raw) => {
                self.command_error = Some(format!("Unknown: {raw}"));
            }
        }
    }

    /// Render the Bloomberg-style command palette as a top-bottom panel
    /// above the tab strip. `/` from anywhere outside a text field focuses
    /// it; Enter dispatches; Esc clears + blurs.
    fn render_command_palette(&mut self, ctx: &egui::Context) {
        let palette_id = egui::Id::new("ccb_command_palette_input");

        // Hotkey: '/' from any non-text-field context steals focus to the
        // palette. We honor egui's "focused widget gets every key" rule by
        // checking that the currently-focused widget (if any) isn't the
        // palette itself.
        let focused_id = ctx.memory(|m| m.focused());
        if focused_id.map_or(true, |id| id == palette_id) || focused_id.is_none() {
            // No field actively eating keystrokes.
            if ctx.input(|i| i.key_pressed(Key::Slash)) {
                self.command_focus_requested = true;
            }
        }

        egui::TopBottomPanel::top("cmd_palette")
            .resizable(false)
            .exact_height(28.0)
            .show(ctx, |ui| {
                ui.horizontal_centered(|ui| {
                    ui.label(
                        RichText::new(" › ")
                            .color(theme::ORANGE)
                            .strong()
                            .size(15.0),
                    );
                    let edit = egui::TextEdit::singleline(&mut self.command_input)
                        .id(palette_id)
                        .desired_width(ui.available_width() - 200.0)
                        .hint_text("/ symbol · BUY 10 AAPL · COMP MSFT NVDA · PORT · HELP");
                    let resp = ui.add(edit);

                    if self.command_focus_requested {
                        resp.request_focus();
                        self.command_focus_requested = false;
                    }
                    if resp.has_focus() && ui.input(|i| i.key_pressed(Key::Escape)) {
                        self.command_input.clear();
                        self.command_error = None;
                        resp.surrender_focus();
                    }
                    if resp.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
                        let raw = std::mem::take(&mut self.command_input);
                        let parsed = cmd::parse(&raw);
                        self.dispatch_command(parsed, ui.ctx());
                    }

                    ui.add_space(8.0);
                    if let Some(err) = &self.command_error {
                        ui.label(RichText::new(err).color(theme::RED).size(11.0));
                    } else {
                        ui.label(
                            RichText::new("press / to focus  ·  ? for help")
                                .color(theme::GRAY2)
                                .size(11.0),
                        );
                    }
                });
            });

        // Help overlay — toggled by HELP / ? and dismissed by Esc / X.
        if self.command_help_open {
            egui::Window::new("Command palette — help")
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .collapsible(false)
                .resizable(false)
                .show(ctx, |ui| {
                    let row = |ui: &mut egui::Ui, cmd: &str, desc: &str| {
                        ui.horizontal(|ui| {
                            ui.label(RichText::new(format!(" {cmd:<22} ")).color(theme::ORANGE).monospace());
                            ui.label(RichText::new(desc).color(theme::WHITE));
                        });
                    };
                    row(ui, "<TICKER>", "load on Chart, e.g.  AAPL");
                    row(ui, "COMP <T1> <T2> ...", "replace Compare slots with these symbols");
                    row(ui, "PORT", "jump to Trading Terminal / Positions");
                    row(ui, "TRADE [<TICKER>]", "jump to Trade form, prefill symbol");
                    row(ui, "BUY  <QTY> <TICKER>", "Trade form, MARKET buy");
                    row(ui, "SELL <QTY> <TICKER>", "Trade form, MARKET sell");
                    row(ui, "ORDERS", "Trading Terminal / Orders");
                    row(ui, "ACT / ACTIVITY", "Trading Terminal / Activity");
                    row(ui, "WATCH <TICKER>", "add to watchlist sidebar");
                    row(ui, "HELP / ?", "show this overlay");
                    ui.add_space(6.0);
                    if ui.button(" CLOSE ").clicked()
                        || ui.input(|i| i.key_pressed(Key::Escape))
                    {
                        self.command_help_open = false;
                    }
                });
        }
    }

    /// Compute the union of symbols we currently want streaming: the chart's
    /// active symbol, every Compare slot, every open position, and the
    /// watchlist (Step 5 will populate it). Push to the WS thread *only*
    /// when the set differs from what we last sent; the thread itself
    /// further diffs and emits subscribe/unsubscribe frames so we don't
    /// thrash the socket.
    fn sync_stream_subscriptions(&mut self) {
        let mut want: std::collections::HashSet<String> = std::collections::HashSet::new();
        if !self.current_symbol.is_empty() {
            want.insert(self.current_symbol.clone());
        }
        for s in &self.compare.slots {
            if !s.symbol.is_empty() {
                want.insert(s.symbol.clone());
            }
        }
        for p in &self.terminal.positions {
            if !p.symbol.is_empty() {
                want.insert(p.symbol.clone());
            }
        }
        for w in &self.watchlist.symbols {
            if !w.is_empty() {
                want.insert(w.clone());
            }
        }
        if want != self.last_subscribed {
            // Best-effort: if the WS thread has died we just drop the
            // update; it'll re-fetch on its next connect from the
            // `desired` set it holds across reconnects.
            let _ = self.stream_tx.send(SubMsg::SetSubscriptions(want.clone()));
            self.last_subscribed = want;
        }
    }

    /// End-of-frame: if persistable state has drifted from what's on disk,
    /// arm the debounce timer. Once a full second has passed without further
    /// changes, flush to `state.json`. Errors are silently swallowed —
    /// persistence is best-effort, not load-bearing for trading.
    fn maybe_save_state(&mut self, ctx: &egui::Context) {
        let current = self.snapshot_state();
        if current != self.last_saved_state {
            // Re-arm the debounce on every change so a rapid pill drag only
            // triggers one save when the user pauses.
            self.state_dirty_since = Some(std::time::Instant::now());
            self.last_saved_state = current;
        }
        if let Some(t) = self.state_dirty_since {
            let elapsed = t.elapsed();
            if elapsed >= persist::SAVE_DEBOUNCE {
                let _ = persist::save(&self.last_saved_state);
                self.state_dirty_since = None;
            } else {
                // Wake up at the debounce deadline so the save fires even
                // when the user has stopped interacting and egui would
                // otherwise sit idle.
                ctx.request_repaint_after(persist::SAVE_DEBOUNCE - elapsed);
            }
        }
    }

    fn drain_messages(&mut self, ctx: &egui::Context) {
        while let Ok(msg) = self.rx.try_recv() {
            match msg {
                Msg::Assets(Ok(a)) => self.assets.load(a),
                Msg::Assets(Err(_)) => {}
                Msg::Bars { symbol, range_idx, tf_idx, gen, bars } => {
                    if gen != self.gen
                        || symbol != self.current_symbol
                        || range_idx != self.range_idx
                        || tf_idx != self.tf_idx
                    {
                        continue; // stale
                    }
                    self.loading = false;
                    match bars {
                        Ok(b) => {
                            self.bars = b;
                            self.err.clear();
                        }
                        Err(e) => {
                            self.bars.clear();
                            self.err = e.to_string();
                        }
                    }
                }
                Msg::CompareBars { symbol, range_idx, gen, bars } => {
                    // Only accept if the slot still exists, its per-slot gen
                    // matches the response, and the range hasn't shifted.
                    if range_idx != self.compare.range_idx {
                        continue;
                    }
                    let slot = self
                        .compare
                        .slots
                        .iter_mut()
                        .find(|s| s.symbol == symbol && s.gen == gen);
                    if let Some(slot) = slot {
                        slot.loading = false;
                        match bars {
                            Ok(b) => {
                                slot.bars = b;
                                slot.err.clear();
                            }
                            Err(e) => {
                                slot.bars.clear();
                                slot.err = e.to_string();
                            }
                        }
                        // New data invalidates any previously-rendered MC.
                        self.compare.mc_results = None;
                    }
                }
                // ---- Terminal tab fetches ----
                Msg::Positions(result) => {
                    self.terminal.positions_loading = false;
                    match result {
                        Ok(v) => { self.terminal.positions = v; self.terminal.positions_err.clear(); }
                        Err(e) => self.terminal.positions_err = e.to_string(),
                    }
                }
                Msg::AccountInfo(result) => match result {
                    Ok(a) => { self.terminal.account = Some(a); self.terminal.account_err.clear(); }
                    Err(e) => self.terminal.account_err = e.to_string(),
                },
                Msg::OpenOrders(result) => {
                    self.terminal.orders_loading = false;
                    match result {
                        Ok(v) => {
                            // Drop any "cancelling…" hints whose orders are
                            // no longer in the open list (Alpaca confirmed
                            // the cancel landed).
                            let live: std::collections::HashSet<_> =
                                v.iter().map(|o| o.id.clone()).collect();
                            self.terminal.cancelling.retain(|id| live.contains(id));
                            self.terminal.open_orders = v;
                            self.terminal.open_orders_err.clear();
                        }
                        Err(e) => self.terminal.open_orders_err = e.to_string(),
                    }
                }
                Msg::ClosedOrders(result) => {
                    if let Ok(v) = result { self.terminal.closed_orders = v; }
                }
                Msg::Activities(result) => {
                    self.terminal.activity_loading = false;
                    match result {
                        Ok(v) => { self.terminal.activities = v; self.terminal.activity_err.clear(); }
                        Err(e) => self.terminal.activity_err = e.to_string(),
                    }
                }
                Msg::OrderPlaced { req_summary, result } => {
                    self.terminal.form.busy = false;
                    match result {
                        Ok(o) => {
                            self.terminal.form.result = format!(
                                "Placed {} — order id {} (status: {})",
                                req_summary,
                                &o.id[..o.id.len().min(8)],
                                o.status
                            );
                            self.terminal.form.result_color = theme::GREEN;
                            // Clear the form so the next order doesn't
                            // accidentally reuse stale qty/limit.
                            self.terminal.form.symbol_input.clear();
                            self.terminal.form.qty_input.clear();
                            self.terminal.form.limit_input.clear();
                            self.terminal.form.autocomplete.clear();
                            // Immediate refresh so the new order shows up
                            // on Positions / Orders without waiting 10s.
                            self.terminal.refresh_all(
                                self.client.clone(),
                                self.tx.clone(),
                                ctx,
                            );
                        }
                        Err(e) => {
                            self.terminal.form.result = format!(
                                "Failed to place {}: {}",
                                req_summary, e
                            );
                            self.terminal.form.result_color = theme::RED;
                        }
                    }
                }
                Msg::OrderCancelled { id, result } => {
                    self.terminal.cancelling.remove(&id);
                    if result.is_ok() {
                        self.terminal
                            .open_orders
                            .retain(|o| o.id != id);
                        self.terminal.refresh_all(
                            self.client.clone(),
                            self.tx.clone(),
                            ctx,
                        );
                    }
                }
                Msg::StreamStatus { connected, latency_ms: _ } => {
                    self.stream_connected = connected;
                }
                Msg::TradeChartBars { symbol, gen, bars } => {
                    // Stale-response guard — drop responses whose gen lags
                    // the latest, or whose symbol no longer matches what's
                    // displayed.
                    if gen != self.terminal.trade_chart.gen
                        || symbol != self.terminal.trade_chart.symbol
                    {
                        continue;
                    }
                    self.terminal.trade_chart.loading = false;
                    match bars {
                        Ok(b) => {
                            self.terminal.trade_chart.bars = b;
                            self.terminal.trade_chart.err.clear();
                        }
                        Err(e) => {
                            self.terminal.trade_chart.bars.clear();
                            self.terminal.trade_chart.err = e.to_string();
                        }
                    }
                }
            }
        }
    }

    pub fn kick_off_load(&mut self, ctx: &egui::Context) {
        let sym = self.symbol_input.trim().to_uppercase();
        if sym.is_empty() {
            return;
        }
        self.current_symbol = sym.clone();
        self.loading = true;
        self.err.clear();
        self.bars.clear();
        self.gen += 1;
        let r = &chart::RANGES[self.range_idx];
        let tf = &chart::TFS[self.tf_idx];
        workers::spawn_load_bars(
            self.client.clone(),
            self.tx.clone(),
            ctx.clone(),
            sym,
            tf.value,
            self.range_idx,
            self.tf_idx,
            r.lookback_hours,
            r.ytd,
            self.gen,
        );
    }
}

impl EApp for ChartApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.drain_messages(ctx);

        let focused = ctx.memory(|m| m.focused().is_some());
        let pressed = |k: Key| ctx.input(|i| i.key_pressed(k));

        // Top-tab number hotkeys 1/2/3 — fire only when no field is focused
        // so they don't steal "1" / "2" typed into a quantity or symbol box.
        if !focused {
            // Chart-tab indicator hotkeys (V B S E U I O) are intentionally
            // gated to Chart only; they'd be silent (and weird) on Compare
            // or Terminal.
            if self.current_tab == Tab::Chart {
                if pressed(Key::V) { self.indicators.volume = !self.indicators.volume; }
                if pressed(Key::B) { self.indicators.bollinger = !self.indicators.bollinger; }
                if pressed(Key::S) { self.indicators.sma = !self.indicators.sma; }
                if pressed(Key::E) { self.indicators.ema = !self.indicators.ema; }
                if pressed(Key::U) { self.indicators.vwap = !self.indicators.vwap; }
                if pressed(Key::I) { self.indicators.rsi = !self.indicators.rsi; }
                if pressed(Key::O) { self.indicators.macd = !self.indicators.macd; }
            }
        }

        // Command palette goes ABOVE the tab strip — single bar across the
        // top, matches the Bloomberg "function code bar" position.
        self.render_command_palette(ctx);

        egui::TopBottomPanel::top("tab_strip").show(ctx, |ui| {
            self.render_tab_strip(ui);
        });

        // Lazy first-visit prime + 10s auto-refresh — only while we're
        // actually looking at the Terminal tab. No point burning API calls
        // on a tab nobody has opened.
        if self.current_tab == Tab::Terminal {
            if !self.terminal_primed {
                self.terminal_primed = true;
                self.terminal.refresh_all(self.client.clone(), self.tx.clone(), ctx);
            } else if self
                .terminal
                .last_refresh
                .map(|t| t.elapsed() >= std::time::Duration::from_secs(10))
                .unwrap_or(true)
            {
                self.terminal.refresh_all(self.client.clone(), self.tx.clone(), ctx);
            }
            // Wake again in 1s so the auto-refresh fires close to schedule
            // even if the user isn't moving the mouse.
            ctx.request_repaint_after(std::time::Duration::from_secs(1));
        }

        // Watchlist sidebar — mounted across every tab so the user can
        // glance at pinned tickers without context-switching. Click a row
        // → load that symbol on the Chart tab. The bottom ticker tape is
        // mounted below; together they form the persistent "what's
        // moving" UI surface.
        let mut load_from_sidebar: Option<String> = None;
        if !self.watchlist.collapsed {
            egui::SidePanel::left("watchlist_panel")
                .resizable(false)
                .min_width(crate::watchlist::sidebar_width())
                .default_width(crate::watchlist::sidebar_width())
                .show(ctx, |ui| {
                    let outcome = crate::watchlist::render_sidebar(
                        &mut self.watchlist,
                        &self.tick_cache,
                        &self.assets,
                        self.client.clone(),
                        self.tx.clone(),
                        ui,
                    );
                    load_from_sidebar = outcome.load_symbol;
                });
        }

        egui::TopBottomPanel::bottom("ticker_tape")
            .resizable(false)
            .exact_height(26.0)
            .show(ctx, |ui| {
                let _ = crate::watchlist::render_ticker_tape(
                    &self.watchlist,
                    &self.tick_cache,
                    ui,
                );
            });

        // Apply a click from the sidebar: jump to Chart and load the symbol.
        if let Some(sym) = load_from_sidebar {
            self.symbol_input = sym;
            self.current_tab = Tab::Chart;
            self.kick_off_load(ctx);
        }

        match self.current_tab {
            Tab::Chart => {
                egui::TopBottomPanel::top("toolbar").show(ctx, |ui| {
                    self.render_toolbar(ui);
                });
                egui::CentralPanel::default().show(ctx, |ui| {
                    chart::render(self, ui);
                });
            }
            Tab::Compare => {
                egui::CentralPanel::default().show(ctx, |ui| {
                    crate::compare::render(
                        &mut self.compare,
                        self.client.clone(),
                        self.tx.clone(),
                        &self.assets,
                        ui,
                    );
                });
            }
            Tab::Terminal => {
                egui::CentralPanel::default().show(ctx, |ui| {
                    crate::terminal::render(
                        &mut self.terminal,
                        self.client.clone(),
                        self.tx.clone(),
                        &self.assets,
                        &self.tick_cache,
                        ui,
                    );
                });
            }
        }

        // Push the latest desired stream subscriptions to the WS thread if
        // anything changed (new chart symbol, new compare slot, new
        // position, etc.). Cheap HashSet diff; only sends a channel msg
        // when there's a real change.
        self.sync_stream_subscriptions();

        // Last thing each frame — diff the persistable surface against what's
        // on disk and flush after the debounce. Cheap; struct compare only.
        self.maybe_save_state(ctx);
    }
}

impl ChartApp {
    fn render_tab_strip(&mut self, ui: &mut egui::Ui) {
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label(
                RichText::new(" ALPACA ")
                    .color(theme::ORANGE)
                    .strong()
                    .size(15.0),
            );
            ui.separator();
            for (tab, label) in [
                (Tab::Terminal, "Trading Terminal"),
                (Tab::Chart, "Chart"),
                (Tab::Compare, "Compare"),
            ] {
                let active = self.current_tab == tab;
                let text = format!("  {}  ", label);
                let btn = if active {
                    egui::Button::new(
                        RichText::new(text).color(theme::BLACK).strong(),
                    )
                    .fill(theme::ORANGE)
                } else {
                    egui::Button::new(RichText::new(text).color(theme::GRAY2))
                        .fill(theme::DARK)
                };
                if ui.add(btn).clicked() {
                    self.current_tab = tab;
                }
            }
        });
        ui.add_space(2.0);
    }

    fn render_toolbar(&mut self, ui: &mut egui::Ui) {
        ui.add_space(4.0);
        ui.horizontal_wrapped(|ui| {
            // Title
            ui.label(
                RichText::new(" ALPACA CHART ")
                    .color(theme::ORANGE)
                    .strong(),
            );
            ui.separator();
            // Symbol input
            ui.label(RichText::new("SYMBOL").color(theme::ORANGE).strong());
            let resp = ui.add(
                egui::TextEdit::singleline(&mut self.symbol_input)
                    .desired_width(110.0),
            );
            if resp.changed() {
                self.symbol_input = self.symbol_input.to_uppercase();
                self.refresh_autocomplete();
            }
            // Esc while focused on the symbol field dismisses suggestions
            // without clearing the input.
            if resp.has_focus() && ui.input(|i| i.key_pressed(Key::Escape)) {
                self.autocomplete.clear();
                self.autocomplete_open = false;
            }
            let submitted = resp.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter));
            if ui.button(" Load ").clicked() || submitted {
                let ctx = ui.ctx().clone();
                self.kick_off_load(&ctx);
                self.autocomplete.clear();
                self.autocomplete_open = false;
            }
            let name = self.assets.company_name(&self.symbol_input);
            if !name.is_empty() {
                ui.label(RichText::new(name).color(theme::CYAN));
            }
        });

        // Autocomplete suggestions render on their OWN row beneath the
        // symbol input. They used to be gated on `resp.has_focus()`, but
        // egui blurs the TextEdit during the same frame the user clicks on
        // a suggestion (the click lands outside the TextEdit's rect, which
        // triggers `surrender_focus_on_click_outside`). That meant the
        // buttons were un-rendered before the click could land — silent
        // dead clicks. Showing them whenever the list is non-empty and
        // clearing explicitly on commit/Esc avoids the focus race entirely.
        if !self.autocomplete.is_empty() {
            let suggestions = self.autocomplete.clone();
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new("↳").color(theme::GRAY2));
                // Tickers only — the company-name match still happens
                // inside `AssetCache::filter`, so typing "apple" finds
                // AAPL; we just don't need to render the long name in
                // every chip.
                for (sym, _name) in suggestions.iter().take(6) {
                    if ui
                        .add(
                            egui::Button::new(RichText::new(sym).color(theme::CYAN))
                                .fill(theme::DARK),
                        )
                        .clicked()
                    {
                        self.symbol_input = sym.clone();
                        self.autocomplete.clear();
                        self.autocomplete_open = false;
                        let ctx = ui.ctx().clone();
                        self.kick_off_load(&ctx);
                    }
                }
            });
        }

        // Range pills + CANDLE pills
        ui.add_space(2.0);
        ui.horizontal_wrapped(|ui| {
            ui.label(RichText::new("RANGE").color(theme::ORANGE).strong());
            for (i, r) in chart::RANGES.iter().enumerate() {
                let active = i == self.range_idx;
                let label = format!(" {} ", r.label);
                let btn = if active {
                    egui::Button::new(RichText::new(label).color(theme::BLACK).strong()).fill(theme::ORANGE)
                } else {
                    egui::Button::new(RichText::new(label).color(theme::GRAY2)).fill(theme::DARK)
                };
                if ui.add(btn).clicked() {
                    self.range_idx = i;
                    self.tf_idx = chart::RANGES[i].default_tf;
                    let ctx = ui.ctx().clone();
                    self.kick_off_load(&ctx);
                }
            }
            ui.add_space(12.0);
            ui.label(RichText::new("CANDLE").color(theme::ORANGE).strong());
            for (i, tf) in chart::TFS.iter().enumerate() {
                let active = i == self.tf_idx;
                let label = format!(" {} ", tf.label);
                let btn = if active {
                    egui::Button::new(RichText::new(label).color(theme::BLACK).strong()).fill(theme::CYAN)
                } else {
                    egui::Button::new(RichText::new(label).color(theme::GRAY2)).fill(theme::DARK)
                };
                if ui.add(btn).clicked() {
                    self.tf_idx = i;
                    let ctx = ui.ctx().clone();
                    self.kick_off_load(&ctx);
                }
            }
        });

        // Indicator pills — each toggles its boolean.
        ui.add_space(2.0);
        ui.horizontal_wrapped(|ui| {
            ui.label(RichText::new("IND").color(theme::ORANGE).strong());
            let pill = |ui: &mut egui::Ui, label: &str, on: &mut bool, color: egui::Color32| {
                let txt = format!(" {} ", label);
                let btn = if *on {
                    egui::Button::new(RichText::new(txt).color(theme::BLACK).strong()).fill(color)
                } else {
                    egui::Button::new(RichText::new(txt).color(theme::GRAY2)).fill(theme::DARK)
                };
                if ui.add(btn).clicked() {
                    *on = !*on;
                }
            };
            pill(ui, "EMA(10)", &mut self.indicators.ema, theme::CYAN);
            pill(ui, "SMA(20)", &mut self.indicators.sma, theme::YELLOW);
            pill(ui, "BB(20)", &mut self.indicators.bollinger, theme::GRAY2);
            pill(ui, "VWAP", &mut self.indicators.vwap, theme::YELLOW);
            pill(ui, "VOL", &mut self.indicators.volume, theme::ORANGE);
            pill(ui, "RSI(14)", &mut self.indicators.rsi, theme::CYAN);
            pill(ui, "MACD", &mut self.indicators.macd, theme::CYAN);
        });

        // Zoom-axis + Strategy controls
        ui.add_space(2.0);
        ui.horizontal_wrapped(|ui| {
            ui.label(RichText::new("ZOOM").color(theme::ORANGE).strong());
            let pill = |ui: &mut egui::Ui, label: &str, on: &mut bool, color: egui::Color32| {
                let txt = format!(" {} ", label);
                let btn = if *on {
                    egui::Button::new(RichText::new(txt).color(theme::BLACK).strong()).fill(color)
                } else {
                    egui::Button::new(RichText::new(txt).color(theme::GRAY2)).fill(theme::DARK)
                };
                if ui.add(btn).clicked() {
                    *on = !*on;
                }
            };
            pill(ui, "X", &mut self.zoom_x, theme::CYAN);
            pill(ui, "Y", &mut self.zoom_y, theme::CYAN);

            // The strategy toggle is only relevant when exactly one
            // strategy-capable indicator is selected. Render it as disabled
            // greyed-out otherwise so the user knows the option exists.
            ui.add_space(12.0);
            ui.label(RichText::new("STRATEGY").color(theme::ORANGE).strong());
            let active = self.indicators.only_active_with_strategy();
            if active.is_some() {
                let on = self.strategy_enabled;
                let label = " SIGNALS ON ";
                let btn = if on {
                    egui::Button::new(RichText::new(label).color(theme::BLACK).strong())
                        .fill(theme::GREEN)
                } else {
                    egui::Button::new(RichText::new(" Show buy/sell ").color(theme::GRAY2))
                        .fill(theme::DARK)
                };
                if ui.add(btn).clicked() {
                    self.strategy_enabled = !self.strategy_enabled;
                }
            } else {
                ui.label(
                    RichText::new("  (select exactly one indicator)")
                        .color(theme::GRAY),
                );
                // If the user disables their lone indicator, auto-disable
                // strategy mode so re-selecting one doesn't surprise them.
                self.strategy_enabled = false;
            }
        });
        ui.add_space(2.0);
    }

    fn refresh_autocomplete(&mut self) {
        if self.symbol_input.is_empty() {
            self.autocomplete.clear();
            self.autocomplete_open = false;
            return;
        }
        self.autocomplete = self.assets.filter(&self.symbol_input, 8);
        self.autocomplete_open = !self.autocomplete.is_empty();
    }
}
