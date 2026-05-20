// Watchlist + ticker tape.
//
// The watchlist is a vertically-stacked list of pinned tickers rendered in a
// collapsible `SidePanel::left` that's mounted across all three top-level
// tabs. Each row shows symbol + live last + day change driven off the shared
// `TickCache`. Clicking a row commits the symbol to the Chart tab.
//
// The ticker tape is a thin scrolling horizontal strip at the bottom (mounted
// via `TopBottomPanel::bottom`) showing the same data. It animates left at
// `SCROLL_SPEED` px/sec; we request a per-frame repaint while it's visible.
//
// Symbols persist via `persist::AppState.watchlist`. The sidebar's transient
// fields (text-input, edit mode) are NOT persisted.

use std::sync::mpsc::Sender;
use std::sync::Arc;

use egui::{Color32, RichText};

use crate::api::AlpacaClient;
use crate::stocks::AssetCache;
use crate::stream::TickCache;
use crate::theme;
use crate::workers::Msg;

const SCROLL_SPEED_PX_PER_SEC: f32 = 35.0;
/// Sidebar width when expanded — narrow enough to stay out of the way, wide
/// enough to fit a 5-letter ticker + 2 numeric columns.
const SIDEBAR_W: f32 = 180.0;

#[derive(Default)]
pub struct WatchlistState {
    /// Pinned symbols, in display order. Persisted in `state.json` so a
    /// restart preserves the user's curated list.
    pub symbols: Vec<String>,
    /// Add-row text input. Transient — not persisted.
    pub input: String,
    /// When true, each row gets a "✕ Remove" affordance. Toggles via the
    /// edit pencil in the sidebar header.
    pub editing: bool,
    /// Whether the side panel is expanded. Click-to-collapse for users who
    /// want max chart real estate.
    pub collapsed: bool,
    /// Autocomplete buffer for the add input — populated from `AssetCache`
    /// the same way as the Chart / Compare / Trade inputs.
    pub autocomplete: Vec<(String, String)>,
}

impl WatchlistState {
    pub fn from_saved(saved: &[String]) -> Self {
        WatchlistState {
            symbols: saved.iter().filter(|s| !s.is_empty()).cloned().collect(),
            ..Default::default()
        }
    }

    fn refresh_autocomplete(&mut self, assets: &AssetCache) {
        self.autocomplete = if self.input.is_empty() {
            Vec::new()
        } else {
            assets.filter(&self.input, 6)
        };
    }

    /// Add a symbol to the watchlist; rejects empties, duplicates, and a
    /// soft cap of 32 (the sidebar gets crowded past that).
    pub fn add(&mut self, sym: &str) {
        let sym = sym.trim().to_uppercase();
        if sym.is_empty() || self.symbols.len() >= 32 { return; }
        if self.symbols.iter().any(|s| s == &sym) { return; }
        self.symbols.push(sym);
    }

    pub fn remove(&mut self, sym: &str) {
        self.symbols.retain(|s| s != sym);
    }
}

/// Outcome the sidebar can emit on a row click — the caller routes it back
/// to ChartApp to load the symbol on the Chart tab.
pub struct SidebarOutcome {
    /// Symbol the user wants to load on the Chart tab. `None` if the user
    /// just edited the list / didn't click a row.
    pub load_symbol: Option<String>,
}

/// Render the left sidebar. Returns any user intent (e.g. "load this symbol
/// on the Chart tab") for the caller to apply.
pub fn render_sidebar(
    state: &mut WatchlistState,
    tick_cache: &TickCache,
    assets: &AssetCache,
    _client: Arc<AlpacaClient>,
    _tx: Sender<Msg>,
    ui: &mut egui::Ui,
) -> SidebarOutcome {
    let mut outcome = SidebarOutcome { load_symbol: None };

    // Header strip: label + collapse + edit toggles.
    ui.horizontal(|ui| {
        ui.label(RichText::new(" WATCHLIST ").color(theme::ORANGE).strong());
        ui.add_space(4.0);
        let edit_btn = if state.editing {
            egui::Button::new(RichText::new(" ✕ ").color(theme::BLACK).strong()).fill(theme::ORANGE)
        } else {
            egui::Button::new(RichText::new(" ✎ ").color(theme::GRAY2)).fill(theme::DARK)
        };
        if ui.add(edit_btn).on_hover_text("Toggle edit mode").clicked() {
            state.editing = !state.editing;
        }
    });
    ui.separator();

    // Add-symbol row.
    ui.horizontal(|ui| {
        let resp = ui.add(
            egui::TextEdit::singleline(&mut state.input)
                .desired_width(80.0),
        );
        if resp.changed() {
            state.input = state.input.to_uppercase();
            state.refresh_autocomplete(assets);
        }
        let submitted = resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
        if ui.button(" + ").clicked() || submitted {
            let s = std::mem::take(&mut state.input);
            state.add(&s);
            state.autocomplete.clear();
        }
    });
    if !state.autocomplete.is_empty() {
        let suggestions = state.autocomplete.clone();
        ui.horizontal_wrapped(|ui| {
            // Tickers only — same convention as the other autocomplete
            // chips in the app.
            for (sym, _name) in suggestions.iter().take(6) {
                if ui
                    .add(
                        egui::Button::new(RichText::new(sym).color(theme::CYAN))
                            .fill(theme::DARK),
                    )
                    .clicked()
                {
                    state.add(sym);
                    state.input.clear();
                    state.autocomplete.clear();
                }
            }
        });
    }
    ui.separator();

    // Body: one row per symbol. Snapshot the symbol list so we can mutate
    // `state.symbols` (for removes) safely inside the loop.
    let symbols_snapshot = state.symbols.clone();
    let live_lock = tick_cache.read().ok();
    let mut to_remove: Option<String> = None;
    egui::ScrollArea::vertical().show(ui, |ui| {
        for sym in &symbols_snapshot {
            let live = live_lock.as_ref().and_then(|c| c.get(sym));
            let last = live.and_then(|t| t.last_price);
            let bar = live.and_then(|t| t.last_bar);

            // Day change from the streamed bar — best signal we've got.
            // No bar yet ⇒ no Δ to show.
            let (chg, pct) = match bar {
                Some(b) => {
                    let chg = b.c - b.o;
                    let pct = if b.o > 0.0 { chg / b.o * 100.0 } else { 0.0 };
                    (Some(chg), Some(pct))
                }
                None => (None, None),
            };

            ui.horizontal(|ui| {
                // Click target — symbol button on the left.
                let resp = ui.add(
                    egui::Button::new(RichText::new(sym).color(theme::WHITE).strong())
                        .fill(theme::DARK),
                );
                if resp.clicked() {
                    outcome.load_symbol = Some(sym.clone());
                }

                // Last price.
                let last_str = last
                    .map(|v| format!("{:>7.2}", v))
                    .unwrap_or_else(|| "    —  ".to_string());
                let last_color = if last.is_some() { theme::WHITE } else { theme::GRAY };
                ui.label(RichText::new(last_str).color(last_color).monospace());

                // Day change %.
                if let Some(p) = pct {
                    let color = if p >= 0.0 { theme::GREEN } else { theme::RED };
                    let sign = if p >= 0.0 { "+" } else { "" };
                    ui.label(
                        RichText::new(format!("{sign}{:.2}%", p)).color(color).monospace(),
                    );
                } else {
                    ui.label(RichText::new(" — ").color(theme::GRAY).monospace());
                }
                let _ = chg; // Δ$ omitted in the cramped sidebar — pct is enough

                if state.editing {
                    if ui
                        .add(
                            egui::Button::new(RichText::new(" ✕ ").color(theme::RED))
                                .fill(theme::DARK),
                        )
                        .on_hover_text("Remove")
                        .clicked()
                    {
                        to_remove = Some(sym.clone());
                    }
                }
            });
        }
    });
    if let Some(sym) = to_remove {
        state.remove(&sym);
    }

    if symbols_snapshot.is_empty() {
        ui.label(
            RichText::new("  No symbols yet — add one above.").color(theme::GRAY2),
        );
    }

    outcome
}

/// Render the bottom ticker tape. Continuously scrolls horizontally; returns
/// `Some(symbol)` if the user clicked one (load it on Chart). Calls
/// `ctx.request_repaint()` to keep animating frame-to-frame.
pub fn render_ticker_tape(
    state: &WatchlistState,
    tick_cache: &TickCache,
    ui: &mut egui::Ui,
) -> Option<String> {
    if state.symbols.is_empty() {
        ui.label(
            RichText::new(" Ticker tape — add symbols to the watchlist to populate.")
                .color(theme::GRAY)
                .small(),
        );
        return None;
    }
    // Build the strings up front so we can measure them.
    let live_lock = tick_cache.read().ok();
    let cells: Vec<TickerCell> = state
        .symbols
        .iter()
        .map(|sym| {
            let live = live_lock.as_ref().and_then(|c| c.get(sym));
            let last = live.and_then(|t| t.last_price);
            let bar = live.and_then(|t| t.last_bar);
            let pct = bar.and_then(|b| {
                if b.o > 0.0 { Some((b.c - b.o) / b.o * 100.0) } else { None }
            });
            TickerCell { symbol: sym.clone(), last, pct }
        })
        .collect();

    // Estimate cell widths. Each cell is roughly: "SYM 123.45  +1.23%" plus
    // padding. We don't need pixel-accuracy — egui will re-flow within the
    // strip and the scrolling is purely cosmetic.
    let cell_w: f32 = 170.0;
    let gap_w: f32 = 16.0;
    let row_w = cells.len() as f32 * (cell_w + gap_w);
    if row_w <= 0.0 {
        return None;
    }

    // Time-based scroll offset. Wraps at `row_w` so cells reappear from the
    // right after sliding off the left.
    let secs = ui.ctx().input(|i| i.time);
    let scroll = ((secs as f32) * SCROLL_SPEED_PX_PER_SEC).rem_euclid(row_w);

    let avail = ui.available_size();
    let (rect, _resp) = ui.allocate_exact_size(
        egui::vec2(avail.x, 24.0),
        egui::Sense::hover(),
    );
    // We don't need click hit-testing on the tape (it's narrow; clicking
    // is finicky on a moving target). Use the sidebar to load a symbol.
    let painter = ui.painter_at(rect);

    // Render TWO copies of the row, offset by row_w, so as the first one
    // scrolls left the second one fills in from the right seamlessly.
    let base_x = rect.left() - scroll;
    for copy in 0..2 {
        let mut x = base_x + (copy as f32) * row_w;
        for cell in &cells {
            draw_cell(&painter, rect, x, cell);
            x += cell_w + gap_w;
        }
    }

    // Continuous animation — request a repaint next frame so the scroll
    // keeps moving even when nothing else triggers one.
    ui.ctx().request_repaint();

    None
}

struct TickerCell {
    symbol: String,
    last: Option<f64>,
    pct: Option<f64>,
}

fn draw_cell(painter: &egui::Painter, clip: egui::Rect, x: f32, cell: &TickerCell) {
    let y = clip.top() + clip.height() / 2.0;
    let font = egui::FontId::monospace(12.0);
    // Symbol.
    let sym_galley = painter.layout_no_wrap(
        cell.symbol.clone(),
        font.clone(),
        theme::ORANGE,
    );
    painter.galley(egui::pos2(x, y - sym_galley.size().y / 2.0), sym_galley.clone(), Color32::PLACEHOLDER);
    let after_sym_x = x + sym_galley.size().x + 6.0;

    // Last price.
    let last_text = cell
        .last
        .map(|v| format!("{:.2}", v))
        .unwrap_or_else(|| "—".to_string());
    let last_galley = painter.layout_no_wrap(last_text, font.clone(), theme::WHITE);
    painter.galley(egui::pos2(after_sym_x, y - last_galley.size().y / 2.0), last_galley.clone(), Color32::PLACEHOLDER);
    let after_last_x = after_sym_x + last_galley.size().x + 6.0;

    // Day pct.
    let (pct_text, pct_color) = match cell.pct {
        Some(p) if p >= 0.0 => (format!("+{:.2}%", p), theme::GREEN),
        Some(p) => (format!("{:.2}%", p), theme::RED),
        None => ("—".to_string(), theme::GRAY),
    };
    let pct_galley = painter.layout_no_wrap(pct_text, font, pct_color);
    painter.galley(egui::pos2(after_last_x, y - pct_galley.size().y / 2.0), pct_galley.clone(), Color32::PLACEHOLDER);
}

/// Helper for ChartApp — keeps the magic number out of the call site.
pub fn sidebar_width() -> f32 {
    SIDEBAR_W
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn add_uppercases_and_trims() {
        let mut w = WatchlistState::default();
        w.add("  aapl  ");
        assert_eq!(w.symbols, vec!["AAPL"]);
    }

    #[test]
    fn add_rejects_duplicates_and_empties() {
        let mut w = WatchlistState::default();
        w.add("AAPL");
        w.add("AAPL");
        w.add("");
        w.add("   ");
        assert_eq!(w.symbols, vec!["AAPL"]);
    }

    #[test]
    fn add_caps_at_32() {
        let mut w = WatchlistState::default();
        for i in 0..40 {
            w.add(&format!("S{:02}", i));
        }
        assert_eq!(w.symbols.len(), 32);
    }

    #[test]
    fn from_saved_skips_empty_strings() {
        let w = WatchlistState::from_saved(&[
            "AAPL".into(),
            "".into(),
            "MSFT".into(),
        ]);
        assert_eq!(w.symbols, vec!["AAPL", "MSFT"]);
    }

    #[test]
    fn remove_purges_the_entry() {
        let mut w = WatchlistState::default();
        w.add("AAPL");
        w.add("MSFT");
        w.remove("AAPL");
        assert_eq!(w.symbols, vec!["MSFT"]);
    }
}
