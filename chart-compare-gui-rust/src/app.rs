// ChartApp — tabbed egui app. Two tabs: the canonical multi-pane chart and a
// multi-asset Compare view. Order entry / positions / activity live in the
// other ports (tview, bt_port); this build stays focused on analysis.

use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Arc;

use eframe::App as EApp;
use egui::{Key, RichText};

use crate::api::{AlpacaClient, Bar};
use crate::chart;
use crate::compare::CompareState;
use crate::stocks::AssetCache;
use crate::theme;
use crate::workers::{self, Msg};

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Tab {
    Chart,
    Compare,
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
}

impl ChartApp {
    pub fn new(ctx: &egui::Context, client: Arc<AlpacaClient>) -> Self {
        theme::apply(ctx);
        let (tx, rx) = mpsc::channel();
        let assets = Arc::new(AssetCache::new());
        workers::spawn_assets(client.clone(), tx.clone(), ctx.clone());
        ChartApp {
            client,
            assets,
            tx,
            rx,
            symbol_input: String::new(),
            current_symbol: String::new(),
            range_idx: 4, // 1Y default
            tf_idx: chart::RANGES[4].default_tf,
            bars: Vec::new(),
            loading: false,
            err: String::new(),
            gen: 0,
            indicators: Indicators::default(),
            autocomplete: Vec::new(),
            autocomplete_open: false,
            zoom_x: true,
            zoom_y: false,
            strategy_enabled: false,
            current_tab: Tab::Chart,
            compare: CompareState::new(),
        }
    }

    fn drain_messages(&mut self) {
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
        self.drain_messages();

        // Indicator-toggle hotkeys only fire on the Chart tab — they'd be
        // invisible (and confusing) on Compare. The focused-widget guard
        // keeps them from stealing letters typed into any text field.
        let pressed = |k: Key| ctx.input(|i| i.key_pressed(k));
        if self.current_tab == Tab::Chart && !ctx.memory(|m| m.focused().is_some()) {
            if pressed(Key::V) {
                self.indicators.volume = !self.indicators.volume;
            }
            if pressed(Key::B) {
                self.indicators.bollinger = !self.indicators.bollinger;
            }
            if pressed(Key::S) {
                self.indicators.sma = !self.indicators.sma;
            }
            if pressed(Key::E) {
                self.indicators.ema = !self.indicators.ema;
            }
            if pressed(Key::U) {
                self.indicators.vwap = !self.indicators.vwap;
            }
            if pressed(Key::I) {
                self.indicators.rsi = !self.indicators.rsi;
            }
            if pressed(Key::O) {
                self.indicators.macd = !self.indicators.macd;
            }
        }

        egui::TopBottomPanel::top("tab_strip").show(ctx, |ui| {
            self.render_tab_strip(ui);
        });

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
        }
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
            for (tab, label) in [(Tab::Chart, "Chart"), (Tab::Compare, "Compare")] {
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
                    .desired_width(110.0)
                    .hint_text("AAPL"),
            );
            if resp.changed() {
                self.symbol_input = self.symbol_input.to_uppercase();
                self.refresh_autocomplete();
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
            // Autocomplete suggestions appear inline (small popup below
            // would be nicer; this is functional and doesn't need positioning math)
            if resp.has_focus() && !self.autocomplete.is_empty() {
                let suggestions = self.autocomplete.clone();
                ui.separator();
                ui.horizontal_wrapped(|ui| {
                    for (sym, _name) in suggestions.iter().take(6) {
                        if ui
                            .add(egui::Button::new(RichText::new(sym).color(theme::CYAN)).fill(theme::DARK))
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
        });

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
