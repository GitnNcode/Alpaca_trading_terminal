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
use crate::config;
use crate::persist;
use crate::stocks::AssetCache;
use crate::stream::{self, SubMsg, TickCache};
use crate::terminal::TerminalState;
use crate::theme;
use crate::watchlist::WatchlistState;
use crate::workers::{self, Msg};

/// Build the multi-line tooltip shown when hovering the palette text-edit.
/// Lists every function code with its description. Cheap to rebuild per
/// frame; egui caches tooltip layout internally.
fn palette_tooltip_text() -> String {
    let mut out =
        String::from("Command palette — function codes:\n\n");
    for (cmd, desc) in PALETTE_FUNCS {
        out.push_str(&format!("  {cmd:<22}  {desc}\n"));
    }
    out.push_str("\nTab accepts the first suggestion · Enter runs · Esc clears.");
    out
}

/// Reference list of function codes for the palette tooltip + help overlay +
/// autocomplete. Single source of truth — adding a new code here is enough
/// to surface it in all three places. The first element is the typed prefix,
/// the second is a human description.
const PALETTE_FUNCS: &[(&str, &str)] = &[
    ("<TICKER>",            "load on Chart, e.g.  AAPL"),
    ("COMP <T1> <T2> ...",  "replace Compare slots with these symbols"),
    ("PORT",                "Trading Terminal / Positions"),
    ("TRADE [<TICKER>]",    "Trade form, prefill symbol"),
    ("BUY  <QTY> <TICKER>", "Trade form, MARKET buy"),
    ("SELL <QTY> <TICKER>", "Trade form, MARKET sell"),
    ("ORDERS",              "Trading Terminal / Orders"),
    ("ACT / ACTIVITY",      "Trading Terminal / Activity"),
    ("WATCH <TICKER>",      "add to watchlist sidebar"),
    ("API CHANGE",          "re-enter API key / secret / paper-or-live"),
    ("HELP / ?",            "show this help overlay"),
];

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

    // One-shot "fit chart to window" request. The toolbar HOME button sets
    // this; `chart::render` propagates `.reset()` to every plot pane that
    // frame and clears the flag. Using Cell keeps `chart::render`'s &-borrow
    // (no signature churn) while letting it consume the flag.
    pub home_requested: std::cell::Cell<bool>,

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
    /// Transient confirmation banner displayed under the palette — used by
    /// the `api change` flow to confirm "credentials saved" without popping
    /// a modal that the user then has to dismiss.
    pub command_status: Option<(String, egui::Color32)>,
    /// `?` / HELP toggles a help overlay.
    pub command_help_open: bool,
    /// Per-frame autocomplete suggestions for the palette. Recomputed each
    /// frame from `command_input` + AssetCache + `PALETTE_FUNCS`; cleared on
    /// dispatch / Esc / chip click.
    pub command_suggestions: Vec<String>,

    // ---------------- Credentials modal ----------------
    /// True whenever the credentials dialog is showing. Set by the
    /// `Command::ApiChange` dispatch AND by `ChartApp::new` when the loaded
    /// credentials are empty (first-launch path).
    pub creds_modal_open: bool,
    /// First-launch mode: the dialog can't be dismissed without saving, and
    /// the rest of the UI is hidden behind a blocking overlay. Distinct from
    /// the `api change` mid-session flow which renders as a normal Window.
    pub creds_modal_first_run: bool,
    pub creds_form_key: String,
    pub creds_form_secret: String,
    /// `true` ⇒ paper-trading endpoint, `false` ⇒ live (real money).
    pub creds_form_paper: bool,
    /// Inline validation error inside the modal (empty key/secret, etc.).
    pub creds_form_error: Option<String>,
    /// Toggles password-masking on the secret field.
    pub creds_show_secret: bool,
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

        // If the on-disk credentials are missing or empty, gate the UI on
        // the first-run setup modal. `client.api_key` is empty in that case
        // (main.rs falls back to Credentials::default()), so all background
        // workers will fail until the user enters real keys — which is
        // exactly why we don't show the rest of the UI yet.
        let need_setup = client.api_key.trim().is_empty()
            || client.api_secret.trim().is_empty();
        // Default new-user mode = paper. If they're re-opening the modal
        // mid-session via `api change`, we'll overwrite this from the
        // current client's base_url before showing the form.
        let form_paper =
            client.base_url.is_empty() || client.base_url.contains("paper");

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
            home_requested: std::cell::Cell::new(false),
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
            command_status: None,
            command_help_open: false,
            command_suggestions: Vec::new(),
            creds_modal_open: need_setup,
            creds_modal_first_run: need_setup,
            creds_form_key: client.api_key.clone(),
            creds_form_secret: client.api_secret.clone(),
            creds_form_paper: form_paper,
            creds_form_error: None,
            creds_show_secret: false,
        };

        // If we restored a non-empty symbol, kick its bars load so the Chart
        // tab is already populated when the user navigates to it. Skip
        // entirely when we're in first-run setup — no point firing requests
        // against an empty client; they'd error out and confuse the new user.
        if !need_setup && !saved.last_symbol.is_empty() {
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
            Command::ApiChange => {
                self.open_credentials_modal(false);
            }
            Command::Unknown(raw) => {
                self.command_error = Some(format!("Unknown: {raw}"));
            }
        }
    }

    /// Open the credentials modal in either first-run mode (UI blocked
    /// behind it) or mid-session mode (rendered as an egui::Window). The
    /// form is pre-filled from the current client so the user only has to
    /// edit what's actually changing — typically just flipping paper/live.
    fn open_credentials_modal(&mut self, first_run: bool) {
        self.creds_form_key = self.client.api_key.clone();
        self.creds_form_secret = self.client.api_secret.clone();
        self.creds_form_paper =
            self.client.base_url.is_empty() || self.client.base_url.contains("paper");
        self.creds_form_error = None;
        self.creds_show_secret = false;
        self.creds_modal_open = true;
        self.creds_modal_first_run = first_run;
    }

    /// Apply the form: validate, write to disk, hot-swap the AlpacaClient,
    /// reauth the stream, and reset everything the old client was holding
    /// onto (positions / orders / asset cache / bars). Returns `true` on
    /// success so the modal can close itself.
    fn save_credentials_from_form(&mut self, ctx: &egui::Context) -> bool {
        let key = self.creds_form_key.trim().to_string();
        let secret = self.creds_form_secret.trim().to_string();
        if key.is_empty() || secret.is_empty() {
            self.creds_form_error = Some("API key and secret are both required.".into());
            return false;
        }
        let base_url = if self.creds_form_paper {
            config::PAPER_BASE_URL
        } else {
            config::LIVE_BASE_URL
        }
        .to_string();
        let creds = config::Credentials { api_key: key, api_secret: secret, base_url };
        if let Err(e) = config::save_credentials(&creds) {
            self.creds_form_error = Some(format!("Could not save credentials: {e}"));
            return false;
        }

        // Hot-swap the client. Anything in flight against the old client
        // will land on the stale-response guards (gen counter for bars,
        // symbol mismatch for everything else) and get discarded.
        let new_client = Arc::new(AlpacaClient::new(creds));
        self.client = new_client.clone();
        // Tell the stream thread to drop the socket and reconnect with the
        // new key. Best-effort — if the thread has died the next launch
        // re-spawns cleanly.
        let _ = self.stream_tx.send(SubMsg::ReplaceClient(new_client.clone()));
        // Force a re-fetch of the asset universe (different account tiers
        // see different asset sets) and reset everything the Terminal tab
        // was holding so the next 10s tick repopulates from scratch.
        workers::spawn_assets(new_client.clone(), self.tx.clone(), ctx.clone());
        self.terminal = TerminalState::new();
        self.terminal_primed = false;
        // Bars from the old client are now meaningless — wipe and bump gen
        // so any in-flight response is filtered out by drain_messages.
        self.bars.clear();
        self.current_symbol.clear();
        self.gen = self.gen.wrapping_add(1);
        // Clear the live tick cache; tickers will repopulate as the new WS
        // stream sends events for the resubscribed symbols.
        if let Ok(mut w) = self.tick_cache.write() {
            w.clear();
        }
        self.last_subscribed.clear();

        self.creds_modal_open = false;
        self.creds_modal_first_run = false;
        self.creds_form_error = None;
        self.command_status = Some((
            format!(
                "API credentials updated ({}). Live stream reconnecting…",
                if self.creds_form_paper { "PAPER" } else { "LIVE" }
            ),
            theme::GREEN,
        ));
        true
    }

    /// Mid-session credentials modal — renders as a normal egui::Window so
    /// the underlying UI is still visible (just unusable for API actions
    /// until the swap completes). Used by the `api change` command.
    fn render_credentials_window(&mut self, ctx: &egui::Context) {
        let mut open = self.creds_modal_open;
        egui::Window::new("Change API credentials")
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .collapsible(false)
            .resizable(false)
            .open(&mut open)
            .show(ctx, |ui| {
                self.render_credentials_form(ui, false);
            });
        // The window's close (X) button toggles `open` off; reflect that
        // back unless first-run is gating the UI.
        if !open && !self.creds_modal_first_run {
            self.creds_modal_open = false;
        }
    }

    /// First-run credentials modal — takes over the entire viewport so the
    /// user can't interact with anything else until they've entered keys.
    /// The form itself is identical to the mid-session window; only the
    /// chrome differs.
    fn render_credentials_first_run(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.vertical_centered(|ui| {
                ui.add_space(40.0);
                ui.label(
                    RichText::new("ALPACA TRADING TERMINAL")
                        .color(theme::ORANGE)
                        .strong()
                        .size(22.0),
                );
                ui.add_space(8.0);
                ui.label(
                    RichText::new("First-time setup — enter your Alpaca API credentials")
                        .color(theme::GRAY2)
                        .size(13.0),
                );
                ui.add_space(24.0);
                ui.allocate_ui_with_layout(
                    egui::vec2(500.0, 0.0),
                    egui::Layout::top_down(egui::Align::LEFT),
                    |ui| {
                        self.render_credentials_form(ui, true);
                    },
                );
            });
        });
    }

    /// The form itself — shared between the first-run CentralPanel and the
    /// mid-session Window. Hint text guides the user through key/secret
    /// generation; the paper/live radio defaults to paper because that's
    /// the safe choice.
    fn render_credentials_form(&mut self, ui: &mut egui::Ui, first_run: bool) {
        ui.label(
            RichText::new("API Key").color(theme::ORANGE).strong(),
        );
        ui.add(
            egui::TextEdit::singleline(&mut self.creds_form_key)
                .desired_width(420.0)
                .hint_text("PK… or AK… — from app.alpaca.markets → API Keys"),
        );
        ui.add_space(8.0);

        ui.label(
            RichText::new("API Secret").color(theme::ORANGE).strong(),
        );
        ui.horizontal(|ui| {
            ui.add(
                egui::TextEdit::singleline(&mut self.creds_form_secret)
                    .desired_width(370.0)
                    .password(!self.creds_show_secret)
                    .hint_text("only shown once when you create the key"),
            );
            let label = if self.creds_show_secret { " hide " } else { " show " };
            if ui.button(label).clicked() {
                self.creds_show_secret = !self.creds_show_secret;
            }
        });
        ui.add_space(8.0);

        ui.label(
            RichText::new("Environment").color(theme::ORANGE).strong(),
        );
        ui.horizontal(|ui| {
            ui.radio_value(&mut self.creds_form_paper, true, " Paper  (simulated, no real money) ")
                .on_hover_text("Recommended starting point. Trades route through paper-api.alpaca.markets.");
            ui.radio_value(&mut self.creds_form_paper, false, " Live  (real orders) ")
                .on_hover_text("Real money. Orders route through api.alpaca.markets.");
        });
        ui.add_space(12.0);

        if let Some(err) = self.creds_form_error.clone() {
            ui.label(RichText::new(err).color(theme::RED).size(12.0));
            ui.add_space(4.0);
        }

        ui.horizontal(|ui| {
            let save_label = if first_run { "  Save and continue  " } else { "  Save  " };
            if ui
                .add(
                    egui::Button::new(
                        RichText::new(save_label).color(theme::BLACK).strong(),
                    )
                    .fill(theme::ORANGE),
                )
                .clicked()
            {
                let ctx = ui.ctx().clone();
                self.save_credentials_from_form(&ctx);
            }
            if !first_run {
                if ui.button("  Cancel  ").clicked() {
                    self.creds_modal_open = false;
                    self.creds_form_error = None;
                }
            }
        });

        ui.add_space(12.0);
        ui.label(
            RichText::new(
                "Get your keys: app.alpaca.markets → Home → API Keys → Generate New Key. \
                 The secret is only shown once; copy it before closing that screen.",
            )
            .color(theme::GRAY)
            .size(11.0),
        );
        ui.label(
            RichText::new("Credentials are stored at:")
                .color(theme::GRAY)
                .size(11.0),
        );
        if let Ok(path) = config::config_path() {
            ui.label(
                RichText::new(format!("  {}", path.display()))
                    .color(theme::GRAY2)
                    .monospace()
                    .size(11.0),
            );
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

        // Build the tooltip string once per frame — it's stable across frames
        // and showing it as a multi-line on_hover_text is cheaper than
        // building a popup ourselves.
        let tooltip = palette_tooltip_text();

        egui::TopBottomPanel::top("cmd_palette")
            .resizable(false)
            .min_height(28.0)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(" › ")
                            .color(theme::ORANGE)
                            .strong()
                            .size(15.0),
                    );
                    let edit = egui::TextEdit::singleline(&mut self.command_input)
                        .id(palette_id)
                        .desired_width(ui.available_width() - 220.0)
                        .hint_text(
                            "/ symbol · BUY 10 AAPL · COMP MSFT NVDA · PORT · API CHANGE · HELP",
                        );
                    let resp = ui.add(edit).on_hover_text(&tooltip);

                    if self.command_focus_requested {
                        resp.request_focus();
                        self.command_focus_requested = false;
                    }
                    if resp.changed() {
                        // Re-derive suggestions every keystroke. Cheap —
                        // AssetCache::filter does a partition_point lookup
                        // and bounded scan, and PALETTE_FUNCS is 11 entries.
                        self.refresh_command_suggestions();
                        // Typing also clears any stale unknown-command error
                        // / status banner so the user isn't reading leftover
                        // feedback from the previous dispatch.
                        self.command_error = None;
                        self.command_status = None;
                    }
                    if resp.has_focus() && ui.input(|i| i.key_pressed(Key::Escape)) {
                        self.command_input.clear();
                        self.command_suggestions.clear();
                        self.command_error = None;
                        resp.surrender_focus();
                    }
                    // Tab while focused accepts the first autocomplete
                    // suggestion. egui normally uses Tab for focus
                    // traversal — we consume it before that fires by
                    // pressing-and-consuming via `key_pressed`.
                    if resp.has_focus()
                        && ui.input(|i| i.key_pressed(Key::Tab))
                        && !self.command_suggestions.is_empty()
                    {
                        let first = self.command_suggestions[0].clone();
                        self.command_input = first;
                        self.refresh_command_suggestions();
                    }
                    if resp.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
                        let raw = std::mem::take(&mut self.command_input);
                        self.command_suggestions.clear();
                        let parsed = cmd::parse(&raw);
                        self.dispatch_command(parsed, ui.ctx());
                    }

                    ui.add_space(8.0);
                    if let Some(err) = &self.command_error {
                        ui.label(RichText::new(err).color(theme::RED).size(11.0));
                    } else if let Some((msg, color)) = &self.command_status {
                        ui.label(RichText::new(msg).color(*color).size(11.0));
                    } else {
                        ui.label(
                            RichText::new("/ focus  ·  Tab autocomplete  ·  ? help")
                                .color(theme::GRAY2)
                                .size(11.0),
                        )
                        .on_hover_text(&tooltip);
                    }
                });

                // Autocomplete suggestions chip strip — same pattern as the
                // Chart toolbar's symbol-input autocomplete. Clicking a chip
                // commits it into the input but does NOT dispatch; the user
                // still presses Enter (matches the chart-toolbar behavior so
                // muscle memory carries over).
                if !self.command_suggestions.is_empty() {
                    ui.horizontal_wrapped(|ui| {
                        ui.add_space(20.0);
                        ui.label(RichText::new("↳").color(theme::GRAY2));
                        let suggestions = self.command_suggestions.clone();
                        for sug in suggestions.iter() {
                            if ui
                                .add(
                                    egui::Button::new(
                                        RichText::new(sug).color(theme::CYAN),
                                    )
                                    .fill(theme::DARK),
                                )
                                .on_hover_text("Click to fill, then press Enter to run")
                                .clicked()
                            {
                                self.command_input = sug.clone();
                                self.refresh_command_suggestions();
                                self.command_focus_requested = true;
                            }
                        }
                    });
                }
            });

        // Help overlay — toggled by HELP / ? and dismissed by Esc / X.
        if self.command_help_open {
            egui::Window::new("Command palette — help")
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .collapsible(false)
                .resizable(false)
                .show(ctx, |ui| {
                    for (cmd, desc) in PALETTE_FUNCS {
                        ui.horizontal(|ui| {
                            ui.label(
                                RichText::new(format!(" {cmd:<22} "))
                                    .color(theme::ORANGE)
                                    .monospace(),
                            );
                            ui.label(RichText::new(*desc).color(theme::WHITE));
                        });
                    }
                    ui.add_space(6.0);
                    if ui.button(" CLOSE ").clicked()
                        || ui.input(|i| i.key_pressed(Key::Escape))
                    {
                        self.command_help_open = false;
                    }
                });
        }
    }

    /// Recompute the palette autocomplete suggestions from the current
    /// input. Kept on `ChartApp` (not the pure `command::parse`) because
    /// suggestions depend on live state — the asset cache.
    pub fn refresh_command_suggestions(&mut self) {
        self.command_suggestions = self.compute_palette_suggestions();
    }

    fn compute_palette_suggestions(&self) -> Vec<String> {
        let raw = &self.command_input;
        if raw.trim().is_empty() {
            return Vec::new();
        }
        let upper = raw.to_ascii_uppercase();
        let tokens: Vec<&str> = upper.split_whitespace().collect();
        let trailing_ws = raw
            .chars()
            .next_back()
            .map(|c| c.is_whitespace())
            .unwrap_or(false);

        let mut out: Vec<String> = Vec::with_capacity(8);

        // Single-token (no trailing space) ⇒ suggest top-level codes that
        // start with this prefix, plus tickers that start with this prefix.
        if tokens.len() == 1 && !trailing_ws {
            let prefix = tokens[0];
            const CODES: &[&str] = &[
                "BUY ",
                "SELL ",
                "COMP ",
                "TRADE ",
                "WATCH ",
                "PORT",
                "ORDERS",
                "ACT",
                "ACTIVITY",
                "HELP",
                "API CHANGE",
            ];
            for c in CODES {
                if c.starts_with(prefix) && *c != prefix {
                    out.push((*c).to_string());
                }
            }
            for (sym, _) in self.assets.filter(prefix, 6) {
                out.push(sym);
            }
        } else {
            // Multi-token (or single-token + trailing space) ⇒ argument
            // completion based on the head function code.
            let head = tokens[0];
            let last = if trailing_ws { "" } else { tokens.last().copied().unwrap_or("") };
            // Reconstruct the "everything except the last token" prefix so
            // we can splice the chosen suggestion back in.
            let prefix_str: String = if trailing_ws {
                let mut s = tokens.join(" ");
                s.push(' ');
                s
            } else {
                let head_tokens = &tokens[..tokens.len() - 1];
                let mut s = head_tokens.join(" ");
                s.push(' ');
                s
            };
            match head {
                "BUY" | "SELL" => {
                    // Expected: <head> <qty> <ticker>. The ticker slot is
                    // either tokens[2] OR (if trailing space after qty)
                    // a fresh token to come.
                    let on_ticker = (tokens.len() >= 3 && !trailing_ws)
                        || (tokens.len() == 2 && trailing_ws);
                    if on_ticker {
                        for (sym, _) in self.assets.filter(last, 6) {
                            out.push(format!("{prefix_str}{sym}"));
                        }
                    }
                }
                "COMP" | "COMPARE" | "TRADE" | "WATCH" => {
                    for (sym, _) in self.assets.filter(last, 6) {
                        out.push(format!("{prefix_str}{sym}"));
                    }
                }
                "API" => {
                    if "CHANGE".starts_with(last) && "CHANGE" != last {
                        out.push("API CHANGE".to_string());
                    }
                }
                _ => {}
            }
        }
        out.truncate(8);
        out
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
                            // Auto-fit on every fresh load. Symbol switch,
                            // range/TF change, sidebar click, command-bar
                            // load — they all funnel through this Msg, so
                            // wiring home here covers every entry point.
                            // Re-uses the same one-shot Cell the toolbar
                            // HOME button drives, so chart::render handles
                            // it identically (Plot::reset on every pane).
                            self.home_requested.set(true);
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

        // First-run setup gate: when no credentials are configured we
        // commandeer the whole viewport with a setup screen. Nothing else
        // renders this frame — including the palette and tab strip — so the
        // user can't trigger background requests that would fail with auth
        // errors. This is the in-GUI replacement for the old stdin prompt.
        if self.creds_modal_first_run {
            self.render_credentials_first_run(ctx);
            return;
        }

        // Mid-session credentials modal (`api change`) — renders as a normal
        // Window above whatever tab is active, so the user can still see the
        // context they were in.
        if self.creds_modal_open {
            self.render_credentials_window(ctx);
        }

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

            // HOME button — one-shot "fit to window". Sets a Cell flag that
            // chart::render consumes this frame, calling Plot::reset() on
            // every pane so egui_plot recomputes bounds from the data + the
            // current pixel rect. That auto-fit is device/DPI-agnostic: it
            // only ever uses the actual available pane size, so the chart
            // re-centers correctly across window resizes, monitor swaps, or
            // OS-level UI scaling. Greyed out while no bars are loaded —
            // pressing it then would be a no-op.
            let home_enabled = !self.bars.is_empty();
            let home_btn = egui::Button::new(
                RichText::new(" HOME ")
                    .color(if home_enabled { theme::BLACK } else { theme::GRAY2 })
                    .strong(),
            )
            .fill(if home_enabled { theme::ORANGE } else { theme::DARK });
            let resp = ui
                .add_enabled(home_enabled, home_btn)
                .on_hover_text("Center + scale chart to fit the window");
            if resp.clicked() {
                self.home_requested.set(true);
            }

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
