use std::time::{Duration, Instant};

#[allow(unused_imports)]
use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::Rect;

use crate::api::OrderRequest;
use crate::app::*;
use crate::theme::*;
use crate::trade_log;
use crate::workers;

const DOUBLE_CLICK_MS: u128 = 500;

fn rect_contains(r: Rect, x: u16, y: u16) -> bool {
    r.width > 0 && r.height > 0
        && x >= r.x && x < r.x + r.width
        && y >= r.y && y < r.y + r.height
}

pub fn handle_key(app: &mut App, key: KeyEvent) {
    if key.kind != KeyEventKind::Press {
        return;
    }
    // Ctrl-C is always quit
    if key.modifiers.contains(KeyModifiers::CONTROL) && matches!(key.code, KeyCode::Char('c')) {
        app.should_quit = true;
        return;
    }

    // Modal eats everything
    if let Some(_modal) = &app.modal {
        return handle_modal_key(app, key);
    }

    match app.active_tab {
        TAB_POSITIONS => handle_positions_key(app, key),
        TAB_TRADE => handle_trade_key(app, key),
        TAB_ORDERS => handle_orders_key(app, key),
        TAB_ACTIVITY => handle_activity_key(app, key),
        TAB_CHART => handle_chart_key(app, key),
        _ => {}
    }
}

fn handle_global_letter(app: &mut App, c: char) -> bool {
    match c {
        '1' => { app.switch_tab(TAB_POSITIONS); true }
        '2' => { app.switch_tab(TAB_TRADE); true }
        '3' => { app.switch_tab(TAB_ORDERS); true }
        '4' => { app.switch_tab(TAB_ACTIVITY); true }
        '5' => { app.switch_tab(TAB_CHART); true }
        'r' | 'R' => { workers::spawn_refresh(app.client.clone(), app.tx.clone()); true }
        'q' | 'Q' => { app.should_quit = true; true }
        _ => false,
    }
}

fn switch_tab_arrow(app: &mut App, delta: i32) {
    let mut t = app.active_tab as i32 + delta;
    if t < 0 { t = TAB_COUNT as i32 - 1; }
    if t >= TAB_COUNT as i32 { t = 0; }
    app.switch_tab(t as usize);
}

// ── Tables ────────────────────────────────────────────────────────────────────

fn handle_positions_key(app: &mut App, key: KeyEvent) {
    match key.code {
        KeyCode::Esc => app.should_quit = true,
        KeyCode::Up => {
            if app.pos_selected > 0 { app.pos_selected -= 1; }
        }
        KeyCode::Down => {
            if app.pos_selected + 1 < app.positions.len() {
                app.pos_selected += 1;
            }
        }
        KeyCode::Left => switch_tab_arrow(app, -1),
        KeyCode::Right => switch_tab_arrow(app, 1),
        KeyCode::F(5) => workers::spawn_refresh(app.client.clone(), app.tx.clone()),
        KeyCode::Char(c) => { handle_global_letter(app, c); }
        _ => {}
    }
}

fn handle_orders_key(app: &mut App, key: KeyEvent) {
    match key.code {
        KeyCode::Esc => app.should_quit = true,
        KeyCode::Up => { if app.orders_selected > 0 { app.orders_selected -= 1; } }
        KeyCode::Down => {
            if app.orders_selected + 1 < app.orders.len() {
                app.orders_selected += 1;
            }
        }
        KeyCode::Delete => trigger_cancel_modal(app),
        KeyCode::Left => switch_tab_arrow(app, -1),
        KeyCode::Right => switch_tab_arrow(app, 1),
        KeyCode::F(5) => workers::spawn_refresh(app.client.clone(), app.tx.clone()),
        KeyCode::Char('x') | KeyCode::Char('X') => trigger_cancel_modal(app),
        KeyCode::Char(c) => { handle_global_letter(app, c); }
        _ => {}
    }
}

fn trigger_cancel_modal(app: &mut App) {
    if app.orders.is_empty() || app.orders_selected >= app.orders.len() {
        return;
    }
    let o = &app.orders[app.orders_selected];
    app.modal = Some(Modal::CancelOrder {
        order_id: o.id.clone(),
        symbol: o.symbol.clone(),
        focus_cancel: true,
    });
}

fn handle_activity_key(app: &mut App, key: KeyEvent) {
    match key.code {
        KeyCode::Esc => app.should_quit = true,
        KeyCode::Up => { if app.activity_selected > 0 { app.activity_selected -= 1; } }
        KeyCode::Down => {
            if app.activity_selected + 1 < app.activity_rows.len() {
                app.activity_selected += 1;
            }
        }
        KeyCode::Left => switch_tab_arrow(app, -1),
        KeyCode::Right => switch_tab_arrow(app, 1),
        KeyCode::F(5) => workers::spawn_refresh(app.client.clone(), app.tx.clone()),
        KeyCode::Char(c) => { handle_global_letter(app, c); }
        _ => {}
    }
}

// ── Modal ─────────────────────────────────────────────────────────────────────

fn handle_modal_key(app: &mut App, key: KeyEvent) {
    let modal = app.modal.take();
    match modal {
        Some(Modal::PlaceOrder { req, focus_confirm }) => {
            match key.code {
                KeyCode::Esc => {} // closes modal
                KeyCode::Left | KeyCode::Right | KeyCode::Tab | KeyCode::BackTab => {
                    app.modal = Some(Modal::PlaceOrder { req, focus_confirm: !focus_confirm });
                }
                KeyCode::Enter => {
                    if focus_confirm {
                        app.set_result(
                            &format!(
                                ">> PLACING {} ORDER FOR {} x{}...",
                                req.order_type.to_uppercase(),
                                req.symbol,
                                req.qty
                            ),
                            YELLOW,
                        );
                        workers::spawn_place_order(app.client.clone(), app.tx.clone(), req);
                    }
                }
                _ => {
                    app.modal = Some(Modal::PlaceOrder { req, focus_confirm });
                }
            }
        }
        Some(Modal::CancelOrder { order_id, symbol, focus_cancel }) => {
            match key.code {
                KeyCode::Esc => {}
                KeyCode::Left | KeyCode::Right | KeyCode::Tab | KeyCode::BackTab => {
                    app.modal = Some(Modal::CancelOrder { order_id, symbol, focus_cancel: !focus_cancel });
                }
                KeyCode::Enter => {
                    if focus_cancel {
                        workers::spawn_cancel_order(app.client.clone(), app.tx.clone(), order_id);
                    }
                }
                _ => {
                    app.modal = Some(Modal::CancelOrder { order_id, symbol, focus_cancel });
                }
            }
        }
        None => {}
    }
}

// ── Trade tab ─────────────────────────────────────────────────────────────────

fn handle_trade_key(app: &mut App, key: KeyEvent) {
    // Autocomplete in symbol field has priority for Up/Down/Enter
    if app.trade.focus == TradeField::Symbol && app.trade.autocomplete.open {
        match key.code {
            KeyCode::Down => {
                if !app.trade.autocomplete.items.is_empty() {
                    app.trade.autocomplete.selected =
                        (app.trade.autocomplete.selected + 1) % app.trade.autocomplete.items.len();
                }
                return;
            }
            KeyCode::Up => {
                if !app.trade.autocomplete.items.is_empty() {
                    if app.trade.autocomplete.selected == 0 {
                        app.trade.autocomplete.selected = app.trade.autocomplete.items.len() - 1;
                    } else {
                        app.trade.autocomplete.selected -= 1;
                    }
                }
                return;
            }
            KeyCode::Enter => {
                if let Some((sym, _)) = app
                    .trade
                    .autocomplete
                    .items
                    .get(app.trade.autocomplete.selected)
                    .cloned()
                {
                    app.trade.symbol = sym;
                    app.trade.autocomplete.close();
                }
                return;
            }
            KeyCode::Esc => {
                app.trade.autocomplete.close();
                return;
            }
            _ => {}
        }
    }

    match key.code {
        KeyCode::Esc => app.should_quit = true,
        KeyCode::Left => switch_tab_arrow(app, -1),
        KeyCode::Right => switch_tab_arrow(app, 1),
        KeyCode::Tab | KeyCode::Down => app.trade.focus = app.trade.focus.next(),
        KeyCode::BackTab | KeyCode::Up => app.trade.focus = app.trade.focus.prev(),
        KeyCode::F(5) => workers::spawn_refresh(app.client.clone(), app.tx.clone()),
        KeyCode::Enter => match app.trade.focus {
            TradeField::Action => app.trade.action_idx = (app.trade.action_idx + 1) % 2,
            TradeField::Type => app.trade.type_idx = (app.trade.type_idx + 1) % 2,
            TradeField::Place => attempt_place(app),
            TradeField::Clear => clear_trade(app),
            _ => app.trade.focus = app.trade.focus.next(),
        },
        KeyCode::Char(c) => match app.trade.focus {
            TradeField::Symbol => {
                let mut s = c.to_ascii_uppercase().to_string();
                if !s.chars().all(|ch| ch.is_ascii_alphanumeric() || ch == '.' || ch == '-') {
                    s.clear();
                }
                if !s.is_empty() {
                    app.trade.symbol.push_str(&s);
                    let prefix = app.trade.symbol.clone();
                    app.trade.autocomplete.refresh(&prefix, &app.assets);
                }
            }
            TradeField::Qty => {
                if c.is_ascii_digit() || c == '.' {
                    app.trade.qty.push(c);
                }
            }
            TradeField::Price => {
                if c.is_ascii_digit() || c == '.' {
                    app.trade.price.push(c);
                }
            }
            _ => {
                // Bottom buttons or dropdowns — treat letters as global shortcuts
                handle_global_letter(app, c);
            }
        },
        KeyCode::Backspace => match app.trade.focus {
            TradeField::Symbol => {
                app.trade.symbol.pop();
                let prefix = app.trade.symbol.clone();
                if prefix.is_empty() {
                    app.trade.autocomplete.close();
                } else {
                    app.trade.autocomplete.refresh(&prefix, &app.assets);
                }
            }
            TradeField::Qty => { app.trade.qty.pop(); }
            TradeField::Price => { app.trade.price.pop(); }
            _ => {}
        },
        _ => {}
    }
}

fn clear_trade(app: &mut App) {
    app.trade.symbol.clear();
    app.trade.qty.clear();
    app.trade.price.clear();
    app.trade.autocomplete.close();
    app.result_msg.clear();
}

fn attempt_place(app: &mut App) {
    let sym = app.trade.symbol.trim().to_ascii_uppercase();
    let qty = app.trade.qty.trim().to_string();
    let price = app.trade.price.trim().to_string();

    if sym.is_empty() {
        app.set_result(">> SYMBOL IS REQUIRED", RED);
        return;
    }
    if qty.is_empty() {
        app.set_result(">> QUANTITY IS REQUIRED", RED);
        return;
    }
    if qty.parse::<f64>().is_err() {
        app.set_result(">> QUANTITY MUST BE A NUMBER", RED);
        return;
    }

    let order_type = app.trade.type_str().to_ascii_lowercase();
    let mut req = OrderRequest {
        symbol: sym,
        qty,
        side: app.trade.action_str().to_ascii_lowercase(),
        order_type,
        time_in_force: "day".to_string(),
        limit_price: String::new(),
    };

    if app.trade.type_idx == 1 {
        // limit
        if price.is_empty() {
            app.set_result(">> LIMIT PRICE IS REQUIRED", RED);
            return;
        }
        if price.parse::<f64>().is_err() {
            app.set_result(">> LIMIT PRICE MUST BE A NUMBER", RED);
            return;
        }
        req.limit_price = price;
    }

    app.modal = Some(Modal::PlaceOrder { req, focus_confirm: true });
}

// ── Chart tab ─────────────────────────────────────────────────────────────────

fn handle_chart_key(app: &mut App, key: KeyEvent) {
    match app.chart.focus {
        ChartFocus::Symbol => handle_chart_symbol_key(app, key),
        ChartFocus::Canvas => handle_chart_canvas_key(app, key),
    }
}

fn handle_chart_symbol_key(app: &mut App, key: KeyEvent) {
    if app.chart.autocomplete.open {
        match key.code {
            KeyCode::Down => {
                if !app.chart.autocomplete.items.is_empty() {
                    app.chart.autocomplete.selected =
                        (app.chart.autocomplete.selected + 1) % app.chart.autocomplete.items.len();
                }
                return;
            }
            KeyCode::Up => {
                if !app.chart.autocomplete.items.is_empty() {
                    if app.chart.autocomplete.selected == 0 {
                        app.chart.autocomplete.selected = app.chart.autocomplete.items.len() - 1;
                    } else {
                        app.chart.autocomplete.selected -= 1;
                    }
                }
                return;
            }
            KeyCode::Enter => {
                if let Some((sym, _)) = app
                    .chart
                    .autocomplete
                    .items
                    .get(app.chart.autocomplete.selected)
                    .cloned()
                {
                    app.chart.symbol_input = sym.clone();
                    app.chart.autocomplete.close();
                    load_chart(app, sym);
                }
                return;
            }
            KeyCode::Esc => {
                app.chart.autocomplete.close();
                return;
            }
            _ => {}
        }
    }

    match key.code {
        KeyCode::Esc => {
            app.chart.focus = ChartFocus::Canvas;
        }
        KeyCode::Tab | KeyCode::Enter => {
            let sym = app.chart.symbol_input.trim().to_ascii_uppercase();
            if !sym.is_empty() {
                app.chart.symbol_input = sym.clone();
                load_chart(app, sym);
            }
            app.chart.focus = ChartFocus::Canvas;
        }
        KeyCode::Left => switch_tab_arrow(app, -1),
        KeyCode::Right => switch_tab_arrow(app, 1),
        KeyCode::F(5) => workers::spawn_refresh(app.client.clone(), app.tx.clone()),
        KeyCode::Char(c) => {
            if c.is_ascii_alphanumeric() || c == '.' || c == '-' {
                app.chart.symbol_input.push(c.to_ascii_uppercase());
                let prefix = app.chart.symbol_input.clone();
                app.chart.autocomplete.refresh(&prefix, &app.assets);
            }
        }
        KeyCode::Backspace => {
            app.chart.symbol_input.pop();
            let prefix = app.chart.symbol_input.clone();
            if prefix.is_empty() {
                app.chart.autocomplete.close();
            } else {
                app.chart.autocomplete.refresh(&prefix, &app.assets);
            }
        }
        _ => {}
    }
}

fn handle_chart_canvas_key(app: &mut App, key: KeyEvent) {
    match key.code {
        // Esc returns to the symbol input — quit is reserved for Q/Ctrl-C.
        KeyCode::Esc => app.chart.focus = ChartFocus::Symbol,
        KeyCode::Enter | KeyCode::Tab | KeyCode::BackTab => {
            app.chart.focus = ChartFocus::Symbol;
        }
        KeyCode::Home => {
            app.chart.scroll_offset = app.chart.bars.len();
        }
        KeyCode::End => {
            app.chart.scroll_offset = 0;
        }
        KeyCode::Left => {
            chart_scroll(app, app.chart.visible_step as isize);
        }
        KeyCode::Right => {
            chart_scroll(app, -(app.chart.visible_step as isize));
        }
        KeyCode::F(5) => workers::spawn_refresh(app.client.clone(), app.tx.clone()),
        KeyCode::Char(',') => chart_scroll(app, app.chart.visible_step as isize),
        KeyCode::Char('.') => chart_scroll(app, -(app.chart.visible_step as isize)),
        KeyCode::Char('<') => chart_scroll(app, (app.chart.visible_step as isize) * 8),
        KeyCode::Char('>') => chart_scroll(app, -(app.chart.visible_step as isize) * 8),
        KeyCode::Char('[') => cycle_range(app, -1),
        KeyCode::Char(']') => cycle_range(app, 1),
        // Timeframe (CANDLE) cycling. Range hotkeys consume the letter row, so
        // we use the adjacent shifted brackets for timeframe.
        KeyCode::Char('{') => cycle_tf(app, -1),
        KeyCode::Char('}') => cycle_tf(app, 1),
        KeyCode::Char('-') => cycle_tf(app, -1),
        KeyCode::Char('=') | KeyCode::Char('+') => cycle_tf(app, 1),
        KeyCode::Char(c) => {
            let lc = c.to_ascii_lowercase();
            // Range hotkey first — these are case-insensitive on the canvas.
            for (i, r) in CHART_RANGES.iter().enumerate() {
                if r.hotkey == lc {
                    select_range(app, i);
                    return;
                }
            }
            // Indicator toggles. Letter → IND_TOGGLES index (kept in sync
            // with the row labels in ui.rs).
            let toggle_idx: Option<usize> = match lc {
                'e' => Some(0), // EMA
                's' => Some(1), // SMA
                'b' => Some(2), // Bollinger
                'u' => Some(3), // VWAP
                'v' => Some(4), // Volume
                'i' => Some(5), // RSI
                'o' => Some(6), // MACD
                _ => None,
            };
            if let Some(idx) = toggle_idx {
                toggle_indicator(app, idx);
                return;
            }
            // not a recognised hotkey — fall through to global shortcuts
            handle_global_letter(app, c);
        }
        _ => {}
    }
}

fn chart_scroll(app: &mut App, delta: isize) {
    let new_off = app.chart.scroll_offset as isize + delta;
    if new_off < 0 {
        app.chart.scroll_offset = 0;
    } else {
        app.chart.scroll_offset = new_off as usize;
    }
}

pub fn select_range(app: &mut App, idx: usize) {
    if idx >= CHART_RANGES.len() { return; }
    app.chart.range_idx = idx;
    app.chart.tf_idx = CHART_RANGES[idx].default_tf;
    let sym = app.chart.symbol_input.trim().to_ascii_uppercase();
    if !sym.is_empty() {
        app.chart.symbol_input = sym.clone();
        load_chart(app, sym);
    }
}

pub fn cycle_range(app: &mut App, delta: i32) {
    let n = CHART_RANGES.len() as i32;
    let idx = ((app.chart.range_idx as i32 + delta) % n + n) % n;
    select_range(app, idx as usize);
}

pub fn select_tf(app: &mut App, idx: usize) {
    if idx >= CHART_TFS.len() { return; }
    app.chart.tf_idx = idx;
    let sym = app.chart.symbol_input.trim().to_ascii_uppercase();
    if !sym.is_empty() {
        app.chart.symbol_input = sym.clone();
        load_chart(app, sym);
    }
}

pub fn cycle_tf(app: &mut App, delta: i32) {
    let n = CHART_TFS.len() as i32;
    let idx = ((app.chart.tf_idx as i32 + delta) % n + n) % n;
    select_tf(app, idx as usize);
}

pub fn load_chart(app: &mut App, symbol: String) {
    app.chart.loading = true;
    app.chart.err.clear();
    app.chart.current_symbol = symbol.clone();
    app.chart.bars.clear();
    app.chart.scroll_offset = 0;
    workers::spawn_load_chart(
        app.client.clone(),
        app.tx.clone(),
        symbol,
        app.chart.range_idx,
        app.chart.tf_idx,
    );
}

// ── Mouse handling ────────────────────────────────────────────────────────────

pub fn handle_mouse(app: &mut App, ev: MouseEvent) {
    let (x, y) = (ev.column, ev.row);

    // Modal eats all clicks.
    if app.modal.is_some() {
        if let MouseEventKind::Down(MouseButton::Left) = ev.kind {
            if rect_contains(app.layout.modal_left_btn, x, y) {
                confirm_modal_action(app, true);
            } else if rect_contains(app.layout.modal_right_btn, x, y) {
                // Right button (CANCEL/KEEP) — dismiss.
                app.modal = None;
            }
        }
        return;
    }

    // Tab bar (any tab) — left click switches tab.
    if let MouseEventKind::Down(MouseButton::Left) = ev.kind {
        if rect_contains(app.layout.tab_bar, x, y) {
            for (i, (s, e)) in app.layout.tab_hits.clone().iter().enumerate() {
                if x >= *s && x < *e {
                    app.switch_tab(i);
                    return;
                }
            }
            return;
        }
    }

    match app.active_tab {
        TAB_POSITIONS => handle_mouse_positions(app, ev),
        TAB_TRADE => handle_mouse_trade(app, ev),
        TAB_ORDERS => handle_mouse_orders(app, ev),
        TAB_ACTIVITY => handle_mouse_activity(app, ev),
        TAB_CHART => handle_mouse_chart(app, ev),
        _ => {}
    }
}

fn confirm_modal_action(app: &mut App, confirmed: bool) {
    let modal = app.modal.take();
    match modal {
        Some(Modal::PlaceOrder { req, .. }) => {
            if confirmed {
                app.set_result(
                    &format!(
                        ">> PLACING {} ORDER FOR {} x{}...",
                        req.order_type.to_uppercase(),
                        req.symbol,
                        req.qty
                    ),
                    YELLOW,
                );
                workers::spawn_place_order(app.client.clone(), app.tx.clone(), req);
            }
        }
        Some(Modal::CancelOrder { order_id, .. }) => {
            if confirmed {
                workers::spawn_cancel_order(app.client.clone(), app.tx.clone(), order_id);
            }
        }
        None => {}
    }
}

fn table_row_at(table: Rect, y: u16) -> Option<usize> {
    // Table has 1-row border + 1-row header (table widget header). Account for both.
    if !rect_contains(table, table.x, y) {
        return None;
    }
    // y relative to inner content; ratatui Table renders header on first inner row.
    let inner_y = table.y + 1; // skip top border
    let header_y = inner_y;    // header row
    if y <= header_y {
        return None;
    }
    Some((y - header_y - 1) as usize)
}

fn handle_mouse_positions(app: &mut App, ev: MouseEvent) {
    if let MouseEventKind::Down(MouseButton::Left) = ev.kind {
        let (x, y) = (ev.column, ev.row);
        if let Some(row) = table_row_at(app.layout.positions_table, y) {
            if row < app.positions.len() && rect_contains(app.layout.positions_table, x, y) {
                app.pos_selected = row;
                // Detect double-click: same position within 500ms → pre-fill trade form as SELL.
                let now = Instant::now();
                let dbl = matches!(
                    app.last_click,
                    Some((t, lx, ly)) if now.duration_since(t) <= Duration::from_millis(DOUBLE_CLICK_MS as u64)
                        && lx == x && ly == y
                );
                app.last_click = Some((now, x, y));
                if dbl {
                    let p = &app.positions[row];
                    app.trade.action_idx = 1; // SELL
                    app.trade.type_idx = 0;   // MARKET
                    app.trade.symbol = p.symbol.clone();
                    app.trade.qty = p.qty.clone();
                    app.trade.price.clear();
                    app.trade.autocomplete.close();
                    app.switch_tab(TAB_TRADE);
                }
            }
        }
    }
}

fn handle_mouse_orders(app: &mut App, ev: MouseEvent) {
    let (x, y) = (ev.column, ev.row);
    if !rect_contains(app.layout.orders_table, x, y) {
        return;
    }
    let Some(row) = table_row_at(app.layout.orders_table, y) else { return; };
    if row >= app.orders.len() {
        return;
    }
    match ev.kind {
        MouseEventKind::Down(MouseButton::Left) => {
            app.orders_selected = row;
        }
        MouseEventKind::Down(MouseButton::Right) => {
            app.orders_selected = row;
            let o = &app.orders[row];
            app.modal = Some(Modal::CancelOrder {
                order_id: o.id.clone(),
                symbol: o.symbol.clone(),
                focus_cancel: true,
            });
        }
        _ => {}
    }
}

fn handle_mouse_activity(app: &mut App, ev: MouseEvent) {
    if let MouseEventKind::Down(MouseButton::Left) = ev.kind {
        let (x, y) = (ev.column, ev.row);
        if let Some(row) = table_row_at(app.layout.activity_table, y) {
            if row < app.activity_rows.len() && rect_contains(app.layout.activity_table, x, y) {
                app.activity_selected = row;
            }
        }
    }
}

fn handle_mouse_trade(app: &mut App, ev: MouseEvent) {
    let (x, y) = (ev.column, ev.row);
    match ev.kind {
        MouseEventKind::Down(MouseButton::Left) => {
            if rect_contains(app.layout.trade_action, x, y) {
                app.trade.focus = TradeField::Action;
                app.trade.action_idx = (app.trade.action_idx + 1) % 2;
            } else if rect_contains(app.layout.trade_type, x, y) {
                app.trade.focus = TradeField::Type;
                app.trade.type_idx = (app.trade.type_idx + 1) % 2;
            } else if rect_contains(app.layout.trade_symbol, x, y) {
                app.trade.focus = TradeField::Symbol;
            } else if rect_contains(app.layout.trade_qty, x, y) {
                app.trade.focus = TradeField::Qty;
            } else if rect_contains(app.layout.trade_price, x, y) {
                app.trade.focus = TradeField::Price;
            } else if rect_contains(app.layout.trade_place, x, y) {
                app.trade.focus = TradeField::Place;
                attempt_place(app);
            } else if rect_contains(app.layout.trade_clear, x, y) {
                app.trade.focus = TradeField::Clear;
                clear_trade(app);
            }
        }
        _ => {}
    }
}

fn handle_mouse_chart(app: &mut App, ev: MouseEvent) {
    let (x, y) = (ev.column, ev.row);
    match ev.kind {
        MouseEventKind::Down(MouseButton::Left) => {
            if rect_contains(app.layout.chart_symbol_input, x, y) {
                app.chart.focus = ChartFocus::Symbol;
                return;
            }
            if rect_contains(app.layout.chart_range_bar, x, y) {
                for (i, (s, e)) in app.layout.chart_range_hits.clone().iter().enumerate() {
                    if x >= *s && x < *e {
                        select_range(app, i);
                        return;
                    }
                }
                return;
            }
            if rect_contains(app.layout.chart_tf_bar, x, y) {
                for (i, (s, e)) in app.layout.chart_tf_hits.clone().iter().enumerate() {
                    if x >= *s && x < *e {
                        select_tf(app, i);
                        return;
                    }
                }
                return;
            }
            if rect_contains(app.layout.chart_ind_bar, x, y) {
                for (i, (s, e)) in app.layout.chart_ind_hits.clone().iter().enumerate() {
                    if x >= *s && x < *e {
                        toggle_indicator(app, i);
                        return;
                    }
                }
                return;
            }
            if rect_contains(app.layout.chart_canvas, x, y) {
                app.chart.focus = ChartFocus::Canvas;
                return;
            }
        }
        MouseEventKind::ScrollUp => {
            if rect_contains(app.layout.chart_canvas, x, y) {
                chart_scroll(app, app.chart.visible_step as isize);
            }
        }
        MouseEventKind::ScrollDown => {
            if rect_contains(app.layout.chart_canvas, x, y) {
                chart_scroll(app, -(app.chart.visible_step as isize));
            }
        }
        // Hover tracking for the chart crosshair. We update `app.chart.hover`
        // on Move events (and clear it when the cursor leaves the canvas).
        MouseEventKind::Moved | MouseEventKind::Drag(_) => {
            if rect_contains(app.layout.chart_canvas, x, y) {
                app.chart.hover = Some((x, y));
            } else {
                app.chart.hover = None;
            }
        }
        _ => {}
    }
}

/// Flip the boolean for the indicator at `idx` (matches IND_TOGGLES ordering
/// in ui.rs: EMA, SMA, BB, VWAP, VOL, RSI, MACD).
pub fn toggle_indicator(app: &mut App, idx: usize) {
    let ind = &mut app.chart.indicators;
    match idx {
        0 => ind.ema = !ind.ema,
        1 => ind.sma = !ind.sma,
        2 => ind.bollinger = !ind.bollinger,
        3 => ind.vwap = !ind.vwap,
        4 => ind.volume = !ind.volume,
        5 => ind.rsi = !ind.rsi,
        6 => ind.macd = !ind.macd,
        _ => {}
    }
}

// ── Message handling (background results) ─────────────────────────────────────

pub fn handle_msg(app: &mut App, msg: Msg) {
    match msg {
        Msg::Positions(result) => match result {
            Ok(p) => {
                app.positions = p;
                if app.pos_selected >= app.positions.len() && !app.positions.is_empty() {
                    app.pos_selected = app.positions.len() - 1;
                }
            }
            Err(e) => app.set_result(&format!("FETCH ERROR: {}", e.to_string().to_uppercase()), RED),
        },
        Msg::Account(result) => {
            if let Ok(a) = result {
                app.account = a;
            }
        }
        Msg::Orders(result) => match result {
            Ok(o) => {
                app.orders = o;
                if app.orders_selected >= app.orders.len() && !app.orders.is_empty() {
                    app.orders_selected = app.orders.len() - 1;
                }
            }
            Err(_) => app.orders.clear(),
        },
        Msg::Activities(activities, closed) => {
            app.activity_rows.clear();
            let mut filled_ids = std::collections::HashSet::new();
            if let Ok(acts) = &activities {
                for a in acts {
                    if a.activity_type == "FILL" {
                        if let Some(oid) = &a.order_id {
                            filled_ids.insert(oid.clone());
                        }
                    }
                }
                for a in acts {
                    app.activity_rows.push(crate::app::activity_to_row(a));
                }
            }
            if let Ok(orders) = &closed {
                for o in orders {
                    if o.status.eq_ignore_ascii_case("filled") && filled_ids.contains(&o.id) {
                        continue;
                    }
                    app.activity_rows.push(crate::app::closed_order_to_row(o));
                }
            }
            app.activity_rows.sort_by(|a, b| b.when.cmp(&a.when));
            if app.activity_selected >= app.activity_rows.len() && !app.activity_rows.is_empty() {
                app.activity_selected = app.activity_rows.len() - 1;
            }
        }
        Msg::Assets(result) => {
            if let Ok(assets) = result {
                app.assets.load(assets);
            }
        }
        Msg::Bars { symbol, range_idx, tf_idx, bars } => {
            // Discard if user moved on
            if symbol != app.chart.current_symbol
                || range_idx != app.chart.range_idx
                || tf_idx != app.chart.tf_idx
            {
                return;
            }
            app.chart.loading = false;
            match bars {
                Ok(b) => {
                    app.chart.bars = b;
                    app.chart.err.clear();
                }
                Err(e) => {
                    app.chart.bars.clear();
                    app.chart.err = e.to_string();
                }
            }
        }
        Msg::OrderPlaced(result, req) => {
            app.modal = None;
            match result {
                Ok(order) => {
                    trade_log::log_trade(&req, &order);
                    let id: String = order.id.chars().take(8).collect();
                    app.set_result(
                        &format!(
                            ">> ORDER PLACED  ID:{}  STATUS:{}  (logged to trades.csv)",
                            id,
                            order.status.to_uppercase()
                        ),
                        GREEN,
                    );
                    app.trade.symbol.clear();
                    app.trade.qty.clear();
                    app.trade.price.clear();
                    workers::spawn_refresh(app.client.clone(), app.tx.clone());
                }
                Err(e) => {
                    app.set_result(&format!(">> ERROR: {}", e.to_string().to_uppercase()), RED);
                }
            }
        }
        Msg::OrderCanceled(result, _id) => {
            app.modal = None;
            match result {
                Ok(_) => {
                    app.set_result(">> ORDER CANCELED", GREEN);
                    workers::spawn_refresh(app.client.clone(), app.tx.clone());
                }
                Err(e) => {
                    app.set_result(
                        &format!(">> CANCEL FAILED: {}", e.to_string().to_uppercase()),
                        RED,
                    );
                }
            }
        }
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;

    use crate::api::AlpacaClient;
    use crate::config::Credentials;
    use crate::stocks::AssetCache;

    fn mk_app() -> App {
        let client = std::sync::Arc::new(AlpacaClient::new(Credentials {
            api_key: "test".into(),
            api_secret: "test".into(),
            base_url: "https://paper-api.alpaca.markets".into(),
        }));
        let assets = std::sync::Arc::new(AssetCache::new());
        let (tx, _rx) = mpsc::channel();
        App::new(client, assets, tx)
    }

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    #[test]
    fn cycle_tf_advances_index() {
        let mut app = mk_app();
        app.chart.tf_idx = 0;
        cycle_tf(&mut app, 1);
        assert_eq!(app.chart.tf_idx, 1);
        cycle_tf(&mut app, 1);
        assert_eq!(app.chart.tf_idx, 2);
        cycle_tf(&mut app, -1);
        assert_eq!(app.chart.tf_idx, 1);
        // wrap-around backwards
        app.chart.tf_idx = 0;
        cycle_tf(&mut app, -1);
        assert_eq!(app.chart.tf_idx, CHART_TFS.len() - 1);
    }

    #[test]
    fn select_range_sets_default_tf() {
        let mut app = mk_app();
        select_range(&mut app, 0);
        assert_eq!(app.chart.range_idx, 0);
        assert_eq!(app.chart.tf_idx, CHART_RANGES[0].default_tf);

        select_range(&mut app, 5); // 5Y
        assert_eq!(app.chart.range_idx, 5);
        assert_eq!(app.chart.tf_idx, CHART_RANGES[5].default_tf);
    }

    #[test]
    fn esc_on_canvas_returns_to_symbol_not_quit() {
        let mut app = mk_app();
        app.switch_tab(TAB_CHART);
        app.chart.focus = ChartFocus::Canvas;
        handle_key(&mut app, key(KeyCode::Esc));
        assert!(matches!(app.chart.focus, ChartFocus::Symbol));
        assert!(!app.should_quit, "Esc on chart canvas must not quit");
    }

    #[test]
    fn brace_keys_cycle_timeframe_on_canvas() {
        let mut app = mk_app();
        app.switch_tab(TAB_CHART);
        app.chart.focus = ChartFocus::Canvas;
        let start = app.chart.tf_idx;
        handle_key(&mut app, key(KeyCode::Char('}')));
        assert_eq!(app.chart.tf_idx, (start + 1) % CHART_TFS.len());
        handle_key(&mut app, key(KeyCode::Char('{')));
        assert_eq!(app.chart.tf_idx, start);
    }

    #[test]
    fn range_hotkey_selects_range_on_canvas() {
        let mut app = mk_app();
        app.switch_tab(TAB_CHART);
        app.chart.focus = ChartFocus::Canvas;
        handle_key(&mut app, key(KeyCode::Char('d')));
        assert_eq!(app.chart.range_idx, 0); // 1D
        handle_key(&mut app, key(KeyCode::Char('y')));
        assert_eq!(app.chart.range_idx, 4); // 1Y
    }

    #[test]
    fn left_right_on_canvas_scroll_bars_not_switch_tabs() {
        let mut app = mk_app();
        app.switch_tab(TAB_CHART);
        app.chart.focus = ChartFocus::Canvas;
        app.chart.visible_step = 3;
        // simulate bars are present so scroll has effect
        app.chart.scroll_offset = 10;
        handle_key(&mut app, key(KeyCode::Right));
        assert_eq!(app.chart.scroll_offset, 7); // moved newer by 3
        handle_key(&mut app, key(KeyCode::Left));
        assert_eq!(app.chart.scroll_offset, 10);
        assert_eq!(app.active_tab, TAB_CHART, "arrows on canvas must not switch tabs");
    }

    #[test]
    fn mouse_click_on_tab_bar_switches_tab() {
        let mut app = mk_app();
        app.layout.tab_bar = Rect { x: 0, y: 2, width: 80, height: 1 };
        app.layout.tab_hits = vec![(0, 17), (18, 31), (32, 46), (47, 63), (64, 77)];
        let ev = MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: 40, // inside (32, 46) → tab index 2 = ORDERS
            row: 2,
            modifiers: KeyModifiers::NONE,
        };
        handle_mouse(&mut app, ev);
        assert_eq!(app.active_tab, TAB_ORDERS);
    }

    #[test]
    fn mouse_click_on_tf_bar_selects_tf() {
        let mut app = mk_app();
        app.switch_tab(TAB_CHART);
        app.chart.tf_idx = 0;
        app.layout.chart_tf_bar = Rect { x: 0, y: 4, width: 60, height: 1 };
        // pretend renderer set hits for 8 timeframes starting at col 8
        app.layout.chart_tf_hits = vec![
            (8, 12), (13, 17), (18, 23), (24, 29),
            (30, 34), (35, 39), (40, 44), (45, 49),
        ];
        let ev = MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: 32, // inside (30, 34) → index 4 = 1h
            row: 4,
            modifiers: KeyModifiers::NONE,
        };
        handle_mouse(&mut app, ev);
        assert_eq!(app.chart.tf_idx, 4);
    }

    #[test]
    fn mouse_click_on_range_bar_selects_range() {
        let mut app = mk_app();
        app.switch_tab(TAB_CHART);
        app.layout.chart_range_bar = Rect { x: 60, y: 4, width: 60, height: 1 };
        app.layout.chart_range_hits = vec![
            (67, 71), (72, 76), (77, 81), (82, 87),
            (88, 92), (93, 97), (98, 103),
        ];
        let ev = MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: 95, // inside (93, 97) → index 5 = 5Y
            row: 4,
            modifiers: KeyModifiers::NONE,
        };
        handle_mouse(&mut app, ev);
        assert_eq!(app.chart.range_idx, 5);
        assert_eq!(app.chart.tf_idx, CHART_RANGES[5].default_tf);
    }

    #[test]
    fn mouse_wheel_on_canvas_scrolls() {
        let mut app = mk_app();
        app.switch_tab(TAB_CHART);
        app.layout.chart_canvas = Rect { x: 0, y: 10, width: 80, height: 30 };
        app.chart.visible_step = 2;
        app.chart.scroll_offset = 5;
        let scroll_up = MouseEvent {
            kind: MouseEventKind::ScrollUp,
            column: 30, row: 20, modifiers: KeyModifiers::NONE,
        };
        handle_mouse(&mut app, scroll_up);
        assert_eq!(app.chart.scroll_offset, 7);
        let scroll_dn = MouseEvent {
            kind: MouseEventKind::ScrollDown,
            column: 30, row: 20, modifiers: KeyModifiers::NONE,
        };
        handle_mouse(&mut app, scroll_dn);
        assert_eq!(app.chart.scroll_offset, 5);
    }

    #[test]
    fn mouse_right_click_on_order_opens_cancel_modal() {
        let mut app = mk_app();
        app.switch_tab(TAB_ORDERS);
        app.orders = vec![crate::api::Order {
            id: "abc12345-x".into(),
            symbol: "AAPL".into(),
            side: "buy".into(),
            order_type: "limit".into(),
            qty: "10".into(),
            limit_price: Some("150".into()),
            status: "new".into(),
            filled_qty: "0".into(),
            filled_avg_price: None,
            created_at: chrono::Utc::now(),
        }];
        app.layout.orders_table = Rect { x: 0, y: 3, width: 80, height: 10 };
        let ev = MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Right),
            column: 10,
            // y = top border (3) + header (1) + 1 → first data row at y=5
            row: 5,
            modifiers: KeyModifiers::NONE,
        };
        handle_mouse(&mut app, ev);
        assert!(matches!(app.modal, Some(Modal::CancelOrder { .. })));
    }
}

#[cfg(test)]
mod indicator_tests {
    use super::*;

    fn mk_app() -> App {
        let client = std::sync::Arc::new(crate::api::AlpacaClient::new(crate::config::Credentials {
            api_key: "t".into(),
            api_secret: "t".into(),
            base_url: "https://paper-api.alpaca.markets".into(),
        }));
        let assets = std::sync::Arc::new(crate::stocks::AssetCache::new());
        let (tx, _rx) = std::sync::mpsc::channel();
        App::new(client, assets, tx)
    }

    #[test]
    fn toggle_indicator_flips_each_flag() {
        let mut a = mk_app();
        // EMA defaults ON
        assert!(a.chart.indicators.ema);
        toggle_indicator(&mut a, 0);
        assert!(!a.chart.indicators.ema);
        toggle_indicator(&mut a, 0);
        assert!(a.chart.indicators.ema);

        // VOL defaults OFF
        assert!(!a.chart.indicators.volume);
        toggle_indicator(&mut a, 4);
        assert!(a.chart.indicators.volume);

        // MACD index 6
        assert!(!a.chart.indicators.macd);
        toggle_indicator(&mut a, 6);
        assert!(a.chart.indicators.macd);
    }

    #[test]
    fn keyboard_indicator_hotkeys_on_canvas() {
        let mut a = mk_app();
        a.switch_tab(crate::app::TAB_CHART);
        a.chart.focus = crate::app::ChartFocus::Canvas;

        // 'b' toggles Bollinger
        assert!(!a.chart.indicators.bollinger);
        handle_key(&mut a, crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Char('b'), crossterm::event::KeyModifiers::NONE,
        ));
        assert!(a.chart.indicators.bollinger);

        // 'v' toggles Volume
        handle_key(&mut a, crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Char('v'), crossterm::event::KeyModifiers::NONE,
        ));
        assert!(a.chart.indicators.volume);

        // 'i' toggles RSI
        handle_key(&mut a, crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Char('i'), crossterm::event::KeyModifiers::NONE,
        ));
        assert!(a.chart.indicators.rsi);
    }

    #[test]
    fn mouse_move_inside_canvas_sets_hover_outside_clears() {
        let mut a = mk_app();
        a.layout.chart_canvas = ratatui::layout::Rect { x: 0, y: 5, width: 80, height: 30 };
        a.switch_tab(crate::app::TAB_CHART);

        // Move INSIDE canvas
        let ev = crossterm::event::MouseEvent {
            kind: crossterm::event::MouseEventKind::Moved,
            column: 20,
            row: 15,
            modifiers: crossterm::event::KeyModifiers::NONE,
        };
        handle_mouse(&mut a, ev);
        assert_eq!(a.chart.hover, Some((20, 15)));

        // Move OUTSIDE canvas (above)
        let ev = crossterm::event::MouseEvent {
            kind: crossterm::event::MouseEventKind::Moved,
            column: 20,
            row: 3,
            modifiers: crossterm::event::KeyModifiers::NONE,
        };
        handle_mouse(&mut a, ev);
        assert_eq!(a.chart.hover, None);
    }

    #[test]
    fn mouse_click_on_indicator_row_toggles() {
        let mut a = mk_app();
        a.layout.chart_ind_bar = ratatui::layout::Rect { x: 0, y: 3, width: 80, height: 1 };
        // Pretend the renderer set hit ranges so EMA (idx 0) is at cols [8, 16)
        a.layout.chart_ind_hits = vec![
            (8, 16), (17, 25), (26, 33), (34, 40), (41, 46), (47, 55), (56, 62),
        ];
        a.switch_tab(crate::app::TAB_CHART);
        let starting_ema = a.chart.indicators.ema;
        let ev = crossterm::event::MouseEvent {
            kind: crossterm::event::MouseEventKind::Down(crossterm::event::MouseButton::Left),
            column: 12, // inside (8, 16) → idx 0 = EMA
            row: 3,
            modifiers: crossterm::event::KeyModifiers::NONE,
        };
        handle_mouse(&mut a, ev);
        assert_eq!(a.chart.indicators.ema, !starting_ema);
    }
}
