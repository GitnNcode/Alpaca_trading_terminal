use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{
    Block, BorderType, Borders, Cell, Clear, List, ListItem, ListState, Paragraph, Row, Table,
    TableState,
};
use ratatui::Frame;

use crate::api::Order;
use crate::app::*;
use crate::chart::ChartCanvas;
use crate::theme::*;

pub fn draw(f: &mut Frame, app: &mut App) {
    let area = f.area();
    let bg = Block::default().style(Style::default().bg(BLACK));
    f.render_widget(bg, area);

    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // header
            Constraint::Length(1), // tab bar
            Constraint::Min(0),    // body
            Constraint::Length(1), // status bar
        ])
        .split(area);

    draw_header(f, app, layout[0]);
    draw_tab_bar(f, app, layout[1]);

    match app.active_tab {
        TAB_POSITIONS => draw_positions(f, app, layout[2]),
        TAB_TRADE => draw_trade(f, app, layout[2]),
        TAB_ORDERS => draw_orders(f, app, layout[2]),
        TAB_ACTIVITY => draw_activity(f, app, layout[2]),
        TAB_CHART => draw_chart(f, app, layout[2]),
        TAB_SUPPLYCHAIN => draw_supplychain(f, app, layout[2]),
        _ => {}
    }

    draw_status_bar(f, app, layout[3]);

    if app.modal.is_some() {
        draw_modal(f, app, area);
    } else {
        // Clear modal button rects so stale clicks don't trigger.
        app.layout.modal_left_btn = Default::default();
        app.layout.modal_right_btn = Default::default();
    }
}

// ── Header ────────────────────────────────────────────────────────────────────

fn draw_header(f: &mut Frame, app: &App, area: Rect) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Plain)
        .border_style(Style::default().fg(ORANGE))
        .style(Style::default().bg(BLACK));
    let inner = block.inner(area);
    f.render_widget(block, area);

    let env_span = if app.client.base_url.contains("paper") {
        Span::styled("PAPER", Style::default().fg(CYAN))
    } else {
        Span::styled("LIVE", Style::default().fg(RED))
    };

    let header_line = Line::from(vec![
        Span::styled(
            " ALPACA TRADING TERMINAL ",
            Style::default().fg(ORANGE).add_modifier(Modifier::BOLD),
        ),
        Span::raw("  |  "),
        env_span,
        Span::raw("  |  "),
        Span::styled(
            format!(" {} ", app.client.base_url),
            Style::default().fg(GRAY),
        ),
    ]);

    let layout = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Min(0), Constraint::Length(28)])
        .split(inner);

    f.render_widget(
        Paragraph::new(header_line).style(Style::default().bg(BLACK)),
        layout[0],
    );

    // Auto-refresh indicator (right side)
    let bar_width = 10usize;
    let total_ticks = 40u32;
    let filled = ((app.tick * bar_width as u32) / total_ticks).min(bar_width as u32) as usize;
    let bar = "█".repeat(filled) + &"░".repeat(bar_width - filled);
    let spin = ['|', '/', '-', '\\'][app.spinner_idx % 4];
    let elapsed = app.tick / 4;
    let ind = Line::from(vec![
        Span::styled(" AUTO ", Style::default().fg(GRAY)),
        Span::styled(format!("{} ", spin), Style::default().fg(ORANGE)),
        Span::styled(bar, Style::default().fg(GREEN)),
        Span::styled(format!(" {}s ", elapsed), Style::default().fg(GRAY)),
    ]);
    f.render_widget(
        Paragraph::new(ind)
            .alignment(Alignment::Right)
            .style(Style::default().bg(BLACK)),
        layout[1],
    );
}

// ── Tab bar ───────────────────────────────────────────────────────────────────

fn draw_tab_bar(f: &mut Frame, app: &mut App, area: Rect) {
    let mut spans: Vec<Span> = Vec::new();
    let mut hits: Vec<(u16, u16)> = Vec::with_capacity(TAB_LABELS.len());
    let mut col = area.x;
    for (i, label) in TAB_LABELS.iter().enumerate() {
        let style = if i == app.active_tab {
            Style::default()
                .fg(BLACK)
                .bg(ORANGE)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(GRAY2)
        };
        let text = format!(" {} ", label);
        let w = text.chars().count() as u16;
        hits.push((col, col + w));
        col += w + 1; // +1 for the single-space separator below
        spans.push(Span::styled(text, style));
        spans.push(Span::raw(" "));
    }
    app.layout.tab_bar = area;
    app.layout.tab_hits = hits;
    f.render_widget(
        Paragraph::new(Line::from(spans)).style(Style::default().bg(BLACK)),
        area,
    );
}

// ── Status bar ────────────────────────────────────────────────────────────────

fn draw_status_bar(f: &mut Frame, app: &App, area: Rect) {
    let hint = match app.active_tab {
        TAB_ORDERS => "[Q]UIT  [R]/F5 REFRESH  [X]/DEL CANCEL ORDER",
        TAB_POSITIONS => "[Q]UIT  [R]/F5 REFRESH  [↑↓] NAVIGATE",
        TAB_CHART => "[D][W][M][T]YD [Y][F]IVE MA[X]  IND: [E]MA [S]MA [B]B vwa[U] [V]OL r[I]si macd[O]  ←/→ SCROLL · HOVER FOR CROSSHAIR",
        TAB_SUPPLYCHAIN => "[TAB] CYCLE PANEL  [ENTER] FETCH / DRILL DOWN  [R] FORCE REFRESH",
        _ => "[Q]UIT  [R]/F5 REFRESH  [1][2][3][4][5][6] TABS",
    };
    let line = Line::from(vec![
        Span::styled("  PORTFOLIO ", Style::default().fg(ORANGE)),
        Span::styled(fmt_money(&app.account.portfolio_value), Style::default().fg(WHITE)),
        Span::raw("   "),
        Span::styled("CASH ", Style::default().fg(ORANGE)),
        Span::styled(fmt_money(&app.account.cash), Style::default().fg(WHITE)),
        Span::raw("   "),
        Span::styled("BUYING POWER ", Style::default().fg(ORANGE)),
        Span::styled(fmt_money(&app.account.buying_power), Style::default().fg(WHITE)),
        Span::raw("    "),
        Span::styled(hint, Style::default().fg(GRAY)),
    ]);
    f.render_widget(
        Paragraph::new(line).style(Style::default().bg(DARK)),
        area,
    );
}

// ── Positions tab ─────────────────────────────────────────────────────────────

fn draw_positions(f: &mut Frame, app: &mut App, area: Rect) {
    let block = panel(" OPEN POSITIONS ");
    let inner = block.inner(area);
    f.render_widget(block, area);
    app.layout.positions_table = area;

    if app.positions.is_empty() {
        let msg = "  NO OPEN POSITIONS — PRESS R TO REFRESH";
        f.render_widget(
            Paragraph::new(msg).style(Style::default().fg(GRAY2).bg(BLACK)),
            inner,
        );
        return;
    }

    let header = Row::new([
        "SYMBOL", "QTY", "AVG ENTRY", "CUR PRICE", "MKT VALUE", "P&L", "P&L %", "SIDE",
    ])
    .style(Style::default().fg(ORANGE).add_modifier(Modifier::BOLD));

    let rows = app.positions.iter().map(|p| {
        let pl: f64 = p.unrealized_pl.parse().unwrap_or(0.0);
        let plpc: f64 = p.unrealized_plpc.parse().unwrap_or(0.0);
        let (pl_str, plpc_str, pl_color) = if pl < 0.0 {
            (
                format!("-${:.2}", -pl),
                format!("{:.2}%", plpc * 100.0),
                RED,
            )
        } else {
            (
                format!("+${:.2}", pl),
                format!("+{:.2}%", plpc * 100.0),
                GREEN,
            )
        };
        Row::new([
            Cell::from(p.symbol.clone()).style(Style::default().fg(WHITE).add_modifier(Modifier::BOLD)),
            Cell::from(p.qty.clone()).style(Style::default().fg(WHITE)),
            Cell::from(format!("${}", fmt_price(&p.avg_entry_price))).style(Style::default().fg(WHITE)),
            Cell::from(format!("${}", fmt_price(&p.current_price))).style(Style::default().fg(WHITE)),
            Cell::from(format!("${}", fmt_price(&p.market_value))).style(Style::default().fg(WHITE)),
            Cell::from(pl_str).style(Style::default().fg(pl_color).add_modifier(Modifier::BOLD)),
            Cell::from(plpc_str).style(Style::default().fg(pl_color)),
            Cell::from(p.side.to_uppercase()).style(Style::default().fg(CYAN)),
        ])
    });

    let table = Table::new(
        rows,
        [
            Constraint::Length(10),
            Constraint::Length(10),
            Constraint::Length(12),
            Constraint::Length(12),
            Constraint::Length(14),
            Constraint::Length(14),
            Constraint::Length(10),
            Constraint::Length(8),
        ],
    )
    .header(header)
    .highlight_style(
        Style::default()
            .bg(ORANGE)
            .fg(BLACK)
            .add_modifier(Modifier::BOLD),
    )
    .style(Style::default().bg(BLACK));

    let mut state = TableState::default().with_selected(Some(app.pos_selected.min(app.positions.len().saturating_sub(1))));
    f.render_stateful_widget(table, inner, &mut state);
}

// ── Orders tab ────────────────────────────────────────────────────────────────

fn draw_orders(f: &mut Frame, app: &mut App, area: Rect) {
    let block = panel(" PENDING ORDERS ");
    let inner = block.inner(area);
    f.render_widget(block, area);
    app.layout.orders_table = area;

    if app.orders.is_empty() {
        let msg = "  NO PENDING ORDERS — PRESS R TO REFRESH";
        f.render_widget(
            Paragraph::new(msg).style(Style::default().fg(GRAY2).bg(BLACK)),
            inner,
        );
        return;
    }

    let header = Row::new([
        "ORDER ID", "SYMBOL", "SIDE", "TYPE", "QTY", "FILLED", "LIMIT PX", "STATUS", "CREATED",
    ])
    .style(Style::default().fg(ORANGE).add_modifier(Modifier::BOLD));

    let rows = app.orders.iter().map(order_to_row);

    let table = Table::new(
        rows,
        [
            Constraint::Length(10),
            Constraint::Length(8),
            Constraint::Length(6),
            Constraint::Length(8),
            Constraint::Length(8),
            Constraint::Length(8),
            Constraint::Length(12),
            Constraint::Length(12),
            Constraint::Length(10),
        ],
    )
    .header(header)
    .highlight_style(
        Style::default()
            .bg(ORANGE)
            .fg(BLACK)
            .add_modifier(Modifier::BOLD),
    )
    .style(Style::default().bg(BLACK));

    let mut state = TableState::default().with_selected(Some(
        app.orders_selected.min(app.orders.len().saturating_sub(1)),
    ));
    f.render_stateful_widget(table, inner, &mut state);
}

fn order_to_row(o: &Order) -> Row<'static> {
    let id: String = o.id.chars().take(8).collect();
    let side_color = if o.side.eq_ignore_ascii_case("sell") { RED } else { CYAN };
    let status_color = match o.status.to_ascii_lowercase().as_str() {
        "filled" => GREEN,
        "partially_filled" => CYAN,
        "canceled" | "expired" | "rejected" => RED,
        _ => YELLOW,
    };
    let limit_str = match &o.limit_price {
        Some(s) if !s.is_empty() && s != "0" => format!("${}", fmt_price(s)),
        _ => "—".to_string(),
    };
    let created = o.created_at.with_timezone(&chrono::Local).format("%H:%M:%S").to_string();

    Row::new([
        Cell::from(id).style(Style::default().fg(GRAY2)),
        Cell::from(o.symbol.clone()).style(Style::default().fg(WHITE).add_modifier(Modifier::BOLD)),
        Cell::from(o.side.to_uppercase()).style(Style::default().fg(side_color).add_modifier(Modifier::BOLD)),
        Cell::from(o.order_type.to_uppercase()).style(Style::default().fg(WHITE)),
        Cell::from(o.qty.clone()).style(Style::default().fg(WHITE)),
        Cell::from(o.filled_qty.clone()).style(Style::default().fg(GRAY2)),
        Cell::from(limit_str).style(Style::default().fg(WHITE)),
        Cell::from(o.status.to_uppercase()).style(Style::default().fg(status_color).add_modifier(Modifier::BOLD)),
        Cell::from(created).style(Style::default().fg(GRAY2)),
    ])
}

// ── Activity tab ──────────────────────────────────────────────────────────────

fn draw_activity(f: &mut Frame, app: &mut App, area: Rect) {
    let block = panel(" ACCOUNT ACTIVITY  (last 100 events + closed orders) ");
    let inner = block.inner(area);
    f.render_widget(block, area);
    app.layout.activity_table = area;

    if app.activity_rows.is_empty() {
        let msg = "  NO ACTIVITY FOUND — PRESS R TO REFRESH";
        f.render_widget(
            Paragraph::new(msg).style(Style::default().fg(GRAY2).bg(BLACK)),
            inner,
        );
        return;
    }

    let header = Row::new([
        "TIME", "TYPE", "SYMBOL", "DIR", "QTY", "PRICE", "AMOUNT", "DETAIL",
    ])
    .style(Style::default().fg(ORANGE).add_modifier(Modifier::BOLD));

    let rows = app.activity_rows.iter().map(|r| {
        let time_str = match r.when {
            Some(t) => t.with_timezone(&chrono::Local).format("%m/%d %H:%M:%S").to_string(),
            None => "—".to_string(),
        };
        Row::new([
            Cell::from(time_str).style(Style::default().fg(GRAY2)),
            Cell::from(r.type_str.clone()).style(Style::default().fg(r.type_color).add_modifier(Modifier::BOLD)),
            Cell::from(r.symbol.clone()).style(Style::default().fg(WHITE).add_modifier(Modifier::BOLD)),
            Cell::from(r.dir.clone()).style(Style::default().fg(r.dir_color).add_modifier(Modifier::BOLD)),
            Cell::from(r.qty.clone()).style(Style::default().fg(WHITE)),
            Cell::from(r.price.clone()).style(Style::default().fg(WHITE)),
            Cell::from(r.amount.clone()).style(Style::default().fg(r.amount_color).add_modifier(Modifier::BOLD)),
            Cell::from(r.detail.clone()).style(Style::default().fg(GRAY2)),
        ])
    });

    let table = Table::new(
        rows,
        [
            Constraint::Length(16),
            Constraint::Length(12),
            Constraint::Length(8),
            Constraint::Length(10),
            Constraint::Length(8),
            Constraint::Length(12),
            Constraint::Length(14),
            Constraint::Min(8),
        ],
    )
    .header(header)
    .highlight_style(
        Style::default()
            .bg(ORANGE)
            .fg(BLACK)
            .add_modifier(Modifier::BOLD),
    )
    .style(Style::default().bg(BLACK));

    let mut state = TableState::default().with_selected(Some(
        app.activity_selected.min(app.activity_rows.len().saturating_sub(1)),
    ));
    f.render_stateful_widget(table, inner, &mut state);
}

// ── Trade tab ─────────────────────────────────────────────────────────────────

fn draw_trade(f: &mut Frame, app: &mut App, area: Rect) {
    let block = panel(" NEW ORDER ");
    let inner = block.inner(area);
    f.render_widget(block, area);

    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // spacer
            Constraint::Length(2), // ACTION
            Constraint::Length(2), // TYPE
            Constraint::Length(2), // SYMBOL
            Constraint::Length(1), // company name
            Constraint::Length(2), // QTY
            Constraint::Length(2), // PRICE
            Constraint::Length(1), // spacer
            Constraint::Length(2), // buttons
            Constraint::Length(1), // spacer
            Constraint::Length(1), // result msg
            Constraint::Min(0),
        ])
        .split(inner);

    let dropdown = |label: &str, value: &str, focused: bool| -> Paragraph<'static> {
        let value_style = if focused {
            Style::default().fg(BLACK).bg(ORANGE).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(WHITE).bg(DARK)
        };
        Paragraph::new(Line::from(vec![
            Span::styled(format!("  {:<12}", label), Style::default().fg(ORANGE)),
            Span::styled(format!(" {:<28} ", value), value_style),
        ]))
        .style(Style::default().bg(BLACK))
    };

    let input = |label: &str, value: &str, placeholder: &str, focused: bool| -> Paragraph<'static> {
        let value_style = if focused {
            Style::default().fg(WHITE).bg(POPUP_BG)
        } else {
            Style::default().fg(WHITE).bg(DARK)
        };
        let display: String = if value.is_empty() {
            format!(" {:<28} ", placeholder)
        } else if focused {
            format!(" {:<28} ", format!("{}_", value))
        } else {
            format!(" {:<28} ", value)
        };
        let placeholder_color = if value.is_empty() {
            Style::default().fg(GRAY).bg(if focused { POPUP_BG } else { DARK })
        } else {
            value_style
        };
        Paragraph::new(Line::from(vec![
            Span::styled(format!("  {:<12}", label), Style::default().fg(ORANGE)),
            Span::styled(display, placeholder_color),
        ]))
        .style(Style::default().bg(BLACK))
    };

    app.layout.trade_action = layout[1];
    app.layout.trade_type = layout[2];
    app.layout.trade_symbol = layout[3];
    f.render_widget(
        dropdown("ACTION", app.trade.action_str(), app.trade.focus == TradeField::Action),
        layout[1],
    );
    f.render_widget(
        dropdown("TYPE", app.trade.type_str(), app.trade.focus == TradeField::Type),
        layout[2],
    );

    f.render_widget(
        input("SYMBOL", &app.trade.symbol, "", app.trade.focus == TradeField::Symbol),
        layout[3],
    );

    // Company name line
    let name = app.assets.company_name(&app.trade.symbol.to_ascii_uppercase());
    if !name.is_empty() {
        f.render_widget(
            Paragraph::new(Line::from(Span::styled(
                format!("                {}", name),
                Style::default().fg(CYAN),
            )))
            .style(Style::default().bg(BLACK)),
            layout[4],
        );
    } else {
        f.render_widget(
            Paragraph::new("").style(Style::default().bg(BLACK)),
            layout[4],
        );
    }

    app.layout.trade_qty = layout[5];
    app.layout.trade_price = layout[6];
    f.render_widget(
        input("QUANTITY", &app.trade.qty, "", app.trade.focus == TradeField::Qty),
        layout[5],
    );

    let price_placeholder = if app.trade.type_idx == 1 { "required" } else { "not used for market orders" };
    f.render_widget(
        input("LIMIT PX", &app.trade.price, price_placeholder, app.trade.focus == TradeField::Price),
        layout[6],
    );

    // Buttons
    let button_row = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(20),
            Constraint::Length(15),
            Constraint::Min(0),
        ])
        .split(layout[8]);

    let btn = |label: &str, focused: bool| -> Paragraph<'static> {
        let style = if focused {
            Style::default().fg(BLACK).bg(ORANGE).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(BLACK).bg(ORANGE)
        };
        Paragraph::new(Line::from(Span::styled(format!("   {}   ", label), style)))
            .alignment(Alignment::Center)
            .style(Style::default().bg(BLACK))
    };
    app.layout.trade_place = button_row[0];
    app.layout.trade_clear = button_row[1];
    f.render_widget(btn("PLACE ORDER", app.trade.focus == TradeField::Place), button_row[0]);
    f.render_widget(btn("CLEAR", app.trade.focus == TradeField::Clear), button_row[1]);

    // Result message
    if !app.result_msg.is_empty() {
        f.render_widget(
            Paragraph::new(Line::from(Span::styled(
                format!("  {}", app.result_msg),
                Style::default().fg(app.result_color),
            )))
            .style(Style::default().bg(BLACK)),
            layout[10],
        );
    }

    // Autocomplete popup
    if app.trade.focus == TradeField::Symbol && app.trade.autocomplete.open && !app.trade.autocomplete.items.is_empty() {
        let popup_y = layout[3].y + 1;
        let popup_x = layout[3].x + 14;
        let h = app.trade.autocomplete.items.len().min(10) as u16;
        let popup = Rect {
            x: popup_x,
            y: popup_y,
            width: 50.min(area.width.saturating_sub(popup_x - area.x)),
            height: h,
        };
        draw_autocomplete(f, &app.trade.autocomplete, popup);
    }
}

fn draw_autocomplete(f: &mut Frame, ac: &Autocomplete, area: Rect) {
    f.render_widget(Clear, area);
    let items: Vec<ListItem> = ac
        .items
        .iter()
        .enumerate()
        .map(|(_i, (sym, name))| {
            ListItem::new(Line::from(vec![
                Span::styled(format!(" {:<6}", sym), Style::default().fg(WHITE)),
                Span::styled(format!(" {}", name), Style::default().fg(GRAY2)),
            ]))
        })
        .collect();

    let list = List::new(items)
        .style(Style::default().bg(POPUP_BG))
        .highlight_style(
            Style::default()
                .bg(CYAN)
                .fg(BLACK)
                .add_modifier(Modifier::BOLD),
        );
    let mut state = ListState::default().with_selected(Some(ac.selected));
    f.render_stateful_widget(list, area, &mut state);
}

// ── Chart tab ─────────────────────────────────────────────────────────────────

fn draw_chart(f: &mut Frame, app: &mut App, area: Rect) {
    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // symbol input row
            Constraint::Length(1), // selector row (TF + RANGE)
            Constraint::Length(1), // INDICATORS row (toggle overlays / sub-panels)
            Constraint::Min(0),    // canvas
            Constraint::Length(1), // stats
        ])
        .split(area);

    // Symbol input row
    let sym_focused = matches!(app.chart.focus, ChartFocus::Symbol);
    let sym_style = if sym_focused {
        Style::default().fg(WHITE).bg(POPUP_BG)
    } else {
        Style::default().fg(WHITE).bg(DARK)
    };
    let sym_display = if sym_focused {
        format!(" {}_ ", app.chart.symbol_input)
    } else {
        format!(" {} ", app.chart.symbol_input)
    };
    let company = app.assets.company_name(&app.chart.symbol_input.to_ascii_uppercase());

    let sym_row = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Length(30), Constraint::Min(0)])
        .split(layout[0]);

    app.layout.chart_symbol_input = sym_row[0];
    f.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled("  SYMBOL  ", Style::default().fg(ORANGE)),
            Span::styled(format!("{:<16}", sym_display), sym_style),
        ]))
        .style(Style::default().bg(BLACK)),
        sym_row[0],
    );
    if !company.is_empty() {
        f.render_widget(
            Paragraph::new(Line::from(Span::styled(
                format!("  {}", company),
                Style::default().fg(CYAN),
            )))
            .style(Style::default().bg(BLACK)),
            sym_row[1],
        );
    } else {
        f.render_widget(
            Paragraph::new("").style(Style::default().bg(BLACK)),
            sym_row[1],
        );
    }

    // Selector row: CANDLE + RANGE
    let sel_row = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Length(60), Constraint::Min(0)])
        .split(layout[1]);

    let tf_prefix = "CANDLE: ";
    let mut tf_spans: Vec<Span> = vec![Span::styled(tf_prefix, Style::default().fg(GRAY2))];
    let mut tf_hits: Vec<(u16, u16)> = Vec::with_capacity(CHART_TFS.len());
    let mut col = sel_row[0].x + tf_prefix.chars().count() as u16;
    for (i, tf) in CHART_TFS.iter().enumerate() {
        let style = if i == app.chart.tf_idx {
            Style::default().fg(BLACK).bg(CYAN).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(GRAY2)
        };
        let text = format!(" {} ", tf.label);
        let w = text.chars().count() as u16;
        tf_hits.push((col, col + w));
        col += w + 1;
        tf_spans.push(Span::styled(text, style));
        tf_spans.push(Span::raw(" "));
    }
    app.layout.chart_tf_bar = sel_row[0];
    app.layout.chart_tf_hits = tf_hits;
    f.render_widget(
        Paragraph::new(Line::from(tf_spans)).style(Style::default().bg(BLACK)),
        sel_row[0],
    );

    let rg_prefix = "RANGE: ";
    let mut rg_spans: Vec<Span> = vec![Span::styled(rg_prefix, Style::default().fg(GRAY2))];
    let mut rg_hits: Vec<(u16, u16)> = Vec::with_capacity(CHART_RANGES.len());
    let mut col = sel_row[1].x + rg_prefix.chars().count() as u16;
    for (i, r) in CHART_RANGES.iter().enumerate() {
        let style = if i == app.chart.range_idx {
            Style::default().fg(BLACK).bg(ORANGE).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(GRAY2)
        };
        let text = format!(" {} ", r.label);
        let w = text.chars().count() as u16;
        rg_hits.push((col, col + w));
        col += w + 1;
        rg_spans.push(Span::styled(text, style));
        rg_spans.push(Span::raw(" "));
    }
    app.layout.chart_range_bar = sel_row[1];
    app.layout.chart_range_hits = rg_hits;
    f.render_widget(
        Paragraph::new(Line::from(rg_spans)).style(Style::default().bg(BLACK)),
        sel_row[1],
    );

    // Indicator labels with hit-test ranges are built by draw_chart_indicators_bar
    // above. We just need the canvas dimensions here.

    // Canvas
    let rg = &CHART_RANGES[app.chart.range_idx];
    let tf = &CHART_TFS[app.chart.tf_idx];
    let title = if app.chart.current_symbol.is_empty() {
        "CHART".to_string()
    } else {
        format!("CHART  {}  ·  {}  ·  {}", app.chart.current_symbol, rg.label, tf.label)
    };
    let canvas_focused = matches!(app.chart.focus, ChartFocus::Canvas);

    // Indicators row (between selector row and canvas)
    draw_chart_indicators_bar(f, app, layout[2]);

    // Pre-compute the visible window so input handlers (←/→ scroll step etc.)
    // can use the same numbers without re-running the layout math.
    let canvas_area = layout[3];
    let inner_w = canvas_area.width.saturating_sub(2) as usize; // borders
    let chart_w = inner_w.saturating_sub(11); // right axis + spacer (matches chart.rs)
    let (_, _, vs, ve, step) =
        crate::chart::compute_window(app.chart.bars.len(), app.chart.scroll_offset, chart_w);
    app.chart.visible_start = vs;
    app.chart.visible_end = ve;
    app.chart.visible_step = step;

    app.layout.chart_canvas = canvas_area;
    let canvas = ChartCanvas {
        bars: &app.chart.bars,
        date_fmt: rg.date_fmt,
        title,
        loading: app.chart.loading,
        err: &app.chart.err,
        focused: canvas_focused,
        scroll_offset: app.chart.scroll_offset,
        indicators: &app.chart.indicators,
        hover: app.chart.hover,
    };
    f.render_widget(canvas, canvas_area);

    // Stats
    if !app.chart.bars.is_empty() {
        let first = &app.chart.bars[0];
        let last = &app.chart.bars[app.chart.bars.len() - 1];
        let mut hi = first.high;
        let mut lo = first.low;
        let mut vol: i64 = 0;
        for b in &app.chart.bars {
            if b.high > hi { hi = b.high; }
            if b.low < lo { lo = b.low; }
            vol += b.volume;
        }
        let chg = last.close - first.open;
        let pct = if first.open > 0.0 { chg / first.open * 100.0 } else { 0.0 };
        let (chg_color, sign) = if chg < 0.0 { (RED, "") } else { (GREEN, "+") };
        let stats = Line::from(vec![
            Span::styled("  CLOSE ", Style::default().fg(ORANGE)),
            Span::styled(format!("${:.2}", last.close), Style::default().fg(WHITE)),
            Span::raw("   "),
            Span::styled("CHG ", Style::default().fg(ORANGE)),
            Span::styled(
                format!("{}${:.2} ({}{:.2}%)", sign, chg, sign, pct),
                Style::default().fg(chg_color),
            ),
            Span::raw("   "),
            Span::styled("HIGH ", Style::default().fg(ORANGE)),
            Span::styled(format!("${:.2}", hi), Style::default().fg(WHITE)),
            Span::raw("   "),
            Span::styled("LOW ", Style::default().fg(ORANGE)),
            Span::styled(format!("${:.2}", lo), Style::default().fg(WHITE)),
            Span::raw("   "),
            Span::styled("VOL ", Style::default().fg(ORANGE)),
            Span::styled(fmt_volume(vol), Style::default().fg(WHITE)),
            Span::raw("   "),
            Span::styled("BARS ", Style::default().fg(ORANGE)),
            Span::styled(format!("{}", app.chart.bars.len()), Style::default().fg(WHITE)),
        ]);
        f.render_widget(
            Paragraph::new(stats).style(Style::default().bg(BLACK)),
            layout[4],
        );
    } else if app.chart.loading {
        // intentionally empty
    } else {
        f.render_widget(
            Paragraph::new("").style(Style::default().bg(BLACK)),
            layout[4],
        );
    }

    // Symbol autocomplete popup
    if sym_focused && app.chart.autocomplete.open && !app.chart.autocomplete.items.is_empty() {
        let popup_y = layout[0].y + 1;
        let popup_x = layout[0].x + 10;
        let h = app.chart.autocomplete.items.len().min(10) as u16;
        let popup = Rect {
            x: popup_x,
            y: popup_y,
            width: 50.min(area.width.saturating_sub(popup_x - area.x)),
            height: h,
        };
        draw_autocomplete(f, &app.chart.autocomplete, popup);
    }
}

// ── Modals ────────────────────────────────────────────────────────────────────

fn draw_modal(f: &mut Frame, app: &mut App, area: Rect) {
    // Avoid borrowing app.modal while we also need &mut app.layout below.
    let descriptor = match &app.modal {
        Some(Modal::PlaceOrder { req, focus_confirm }) => {
            let limit_str = if req.limit_price.is_empty() {
                "MARKET".to_string()
            } else {
                format!("${}", req.limit_price)
            };
            let body = format!(
                "  ACTION    :  {}\n  TYPE      :  {}\n  SYMBOL    :  {}\n  QUANTITY  :  {}\n  PRICE     :  {}\n  TIF       :  DAY",
                req.side.to_uppercase(),
                req.order_type.to_uppercase(),
                req.symbol,
                req.qty,
                limit_str,
            );
            Some(("CONFIRM ORDER".to_string(), body, "CONFIRM".to_string(), "CANCEL".to_string(), *focus_confirm))
        }
        Some(Modal::CancelOrder { order_id, symbol, focus_cancel }) => {
            let short_id: String = order_id.chars().take(8).collect();
            let body = format!("  CANCEL order for {}?\n\n  ID: {}", symbol, short_id);
            Some((
                "CANCEL ORDER".to_string(),
                body,
                "CANCEL ORDER".to_string(),
                "KEEP".to_string(),
                *focus_cancel,
            ))
        }
        None => None,
    };
    if let Some((title, body, left_label, right_label, focus_left)) = descriptor {
        draw_confirm(f, app, area, &title, &body, &left_label, &right_label, focus_left);
    }
}

fn draw_confirm(
    f: &mut Frame,
    app: &mut App,
    area: Rect,
    title: &str,
    body: &str,
    left_label: &str,
    right_label: &str,
    focus_left: bool,
) {
    let width = 60u16;
    let height = 14u16;
    let r = Rect {
        x: area.x + area.width.saturating_sub(width) / 2,
        y: area.y + area.height.saturating_sub(height) / 2,
        width: width.min(area.width),
        height: height.min(area.height),
    };
    f.render_widget(Clear, r);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Plain)
        .border_style(Style::default().fg(ORANGE))
        .style(Style::default().bg(DARK))
        .title(Span::styled(
            format!(" {} ", title),
            Style::default().fg(ORANGE).add_modifier(Modifier::BOLD),
        ));
    let inner = block.inner(r);
    f.render_widget(block, r);

    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(0), Constraint::Length(2), Constraint::Length(1)])
        .split(inner);

    f.render_widget(
        Paragraph::new(body.to_string()).style(Style::default().fg(WHITE).bg(DARK)),
        layout[0],
    );

    let buttons = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Min(2),
            Constraint::Length(18),
            Constraint::Length(2),
            Constraint::Length(14),
            Constraint::Min(2),
        ])
        .split(layout[1]);

    let btn = |label: &str, focused: bool| -> Paragraph<'static> {
        let style = if focused {
            Style::default().fg(BLACK).bg(ORANGE).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(BLACK).bg(GRAY2)
        };
        Paragraph::new(Line::from(Span::styled(format!(" {} ", label), style)))
            .alignment(Alignment::Center)
            .style(Style::default().bg(DARK))
    };
    app.layout.modal_left_btn = buttons[1];
    app.layout.modal_right_btn = buttons[3];
    f.render_widget(btn(left_label, focus_left), buttons[1]);
    f.render_widget(btn(right_label, !focus_left), buttons[3]);

    let hint = Paragraph::new(Line::from(Span::styled(
        "  ←/→ choose · ENTER confirm · ESC dismiss",
        Style::default().fg(GRAY2),
    )))
    .style(Style::default().bg(DARK));
    f.render_widget(hint, layout[2]);
}

// ── Supply Chain tab ──────────────────────────────────────────────────────────

const SC_CATEGORIES: [SupplyChainCategory; 3] = [
    SupplyChainCategory::Suppliers,
    SupplyChainCategory::Competitors,
    SupplyChainCategory::Customers,
];

fn draw_supplychain(f: &mut Frame, app: &mut App, area: Rect) {
    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // ticker input row
            Constraint::Length(1), // resolved company name / hint
            Constraint::Percentage(50), // FMP panel
            Constraint::Min(0),    // Claude panel
        ])
        .split(area);

    // ── Ticker input row ─────────────────────────────────────────────────
    let input_focused = matches!(app.supply_chain.focused, SupplyChainField::Input);
    let input_style = if input_focused {
        Style::default().fg(WHITE).bg(POPUP_BG)
    } else {
        Style::default().fg(WHITE).bg(DARK)
    };
    let input_display = if input_focused {
        format!(" {}_ ", app.supply_chain.symbol_input)
    } else {
        format!(" {} ", app.supply_chain.symbol_input)
    };
    let input_row = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Length(36), Constraint::Min(0)])
        .split(layout[0]);
    app.layout.supplychain_input = input_row[0];
    f.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled("  COMPANY  ", Style::default().fg(ORANGE)),
            Span::styled(format!("{:<20}", input_display), input_style),
        ]))
        .style(Style::default().bg(BLACK)),
        input_row[0],
    );

    let queried_blurb = match &app.supply_chain.queried_symbol {
        Some(sym) => {
            let fmp_name = app
                .supply_chain
                .fmp
                .data
                .as_ref()
                .map(|d| d.company_name.clone())
                .unwrap_or_default();
            let claude_name = app
                .supply_chain
                .claude
                .data
                .as_ref()
                .map(|d| d.company_name.clone())
                .unwrap_or_default();
            let name = if !fmp_name.is_empty() {
                fmp_name
            } else if !claude_name.is_empty() {
                claude_name
            } else {
                app.assets.company_name(sym)
            };
            if name.is_empty() {
                format!("Showing supply chain for {}", sym)
            } else {
                format!("{}  ({})", name, sym)
            }
        }
        None => "Type a ticker and press Enter to fetch suppliers, competitors, and customers from FMP + Claude.".into(),
    };
    f.render_widget(
        Paragraph::new(Span::styled(
            format!("  {}", queried_blurb),
            Style::default().fg(GRAY2),
        ))
        .style(Style::default().bg(BLACK)),
        input_row[1],
    );

    // Inline status line under the input. Cell 0 dedicated to overall hint.
    f.render_widget(
        Paragraph::new(Span::styled(
            "  Sources rendered side-by-side. Bold matches signal agreement between FMP filings and Claude.",
            Style::default().fg(GRAY),
        ))
        .style(Style::default().bg(BLACK)),
        layout[1],
    );

    // Find matching tickers across panels for the agreement highlight.
    let matches = compute_supplychain_matches(&app.supply_chain);

    draw_supplychain_panel(
        f,
        app,
        layout[2],
        SupplyChainPanel::Fmp,
        " FMP — 10-K DISCLOSURES ",
        &matches,
    );
    draw_supplychain_panel(
        f,
        app,
        layout[3],
        SupplyChainPanel::Claude,
        " CLAUDE — TRAINING DATA (JAN-2026 CUTOFF) ",
        &matches,
    );
}

fn draw_supplychain_panel(
    f: &mut Frame,
    app: &mut App,
    area: Rect,
    panel_kind: SupplyChainPanel,
    title: &str,
    matches: &[std::collections::HashSet<String>; 3],
) {
    let block = panel(title);
    let inner = block.inner(area);
    f.render_widget(block, area);

    // Status line at top of panel: loading / error / note
    let status_h = 1u16;
    let cells_area = Rect {
        x: inner.x,
        y: inner.y + status_h,
        width: inner.width,
        height: inner.height.saturating_sub(status_h),
    };

    let p = app.supply_chain.panel(panel_kind);
    let status_line = if p.loading {
        let spin = ['|', '/', '-', '\\'][app.spinner_idx % 4];
        Line::from(Span::styled(
            format!("  {} loading…", spin),
            Style::default().fg(YELLOW),
        ))
    } else if let Some(err) = &p.error {
        Line::from(Span::styled(
            format!("  ! {}", err),
            Style::default().fg(RED),
        ))
    } else if let Some(d) = &p.data {
        Line::from(Span::styled(
            format!("  {}", d.note),
            Style::default().fg(GRAY2),
        ))
    } else {
        Line::from(Span::styled(
            "  (no symbol queried yet)",
            Style::default().fg(GRAY),
        ))
    };
    f.render_widget(
        Paragraph::new(status_line).style(Style::default().bg(BLACK)),
        Rect {
            x: inner.x,
            y: inner.y,
            width: inner.width,
            height: status_h,
        },
    );

    // Three side-by-side category columns
    let col_layout = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(33),
            Constraint::Percentage(34),
            Constraint::Percentage(33),
        ])
        .split(cells_area);

    let cell_base = match panel_kind {
        SupplyChainPanel::Fmp => 0,
        SupplyChainPanel::Claude => 3,
    };

    for (i, cat) in SC_CATEGORIES.iter().enumerate() {
        let rect = col_layout[i];
        app.layout.supplychain_cells[cell_base + i] = rect;
        let focused = matches!(
            app.supply_chain.focused,
            SupplyChainField::Column(p, c) if p == panel_kind && c == *cat
        );
        let col_block = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Plain)
            .border_style(Style::default().fg(if focused { CYAN } else { GRAY }))
            .title(Span::styled(
                format!(" {} ", cat.label()),
                Style::default()
                    .fg(if focused { CYAN } else { ORANGE })
                    .add_modifier(Modifier::BOLD),
            ))
            .style(Style::default().bg(BLACK));
        let col_inner = col_block.inner(rect);
        f.render_widget(col_block, rect);

        let rows_data = app.supply_chain.panel(panel_kind).rows_for(*cat);
        if rows_data.is_empty() && !app.supply_chain.panel(panel_kind).loading {
            let msg = if app.supply_chain.queried_symbol.is_some() {
                "  (none reported)"
            } else {
                ""
            };
            f.render_widget(
                Paragraph::new(msg).style(Style::default().fg(GRAY2).bg(BLACK)),
                col_inner,
            );
            continue;
        }

        let selected = app.supply_chain.panel(panel_kind).selected[cat.index()];
        let cat_matches = &matches[cat.index()];

        let table_rows: Vec<Row> = rows_data
            .iter()
            .map(|r| {
                let ticker_text = r.ticker.clone().unwrap_or_else(|| "—".to_string());
                let bold = r
                    .ticker
                    .as_deref()
                    .map(|t| cat_matches.contains(t))
                    .unwrap_or(false);
                let name_style = if bold {
                    Style::default().fg(GREEN).add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(WHITE)
                };
                let ticker_style = if bold {
                    Style::default().fg(GREEN).add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(CYAN)
                };
                Row::new(vec![
                    Cell::from(truncate(&r.name, 22)).style(name_style),
                    Cell::from(ticker_text).style(ticker_style),
                    Cell::from(truncate(&r.rationale, 36))
                        .style(Style::default().fg(GRAY2)),
                ])
            })
            .collect();

        let header = Row::new(vec!["NAME", "TICKER", "RATIONALE"])
            .style(Style::default().fg(ORANGE).add_modifier(Modifier::BOLD));

        let table = Table::new(
            table_rows,
            [
                Constraint::Percentage(40),
                Constraint::Length(8),
                Constraint::Min(0),
            ],
        )
        .header(header)
        .highlight_style(
            Style::default()
                .bg(POPUP_BG)
                .fg(WHITE)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("> ")
        .style(Style::default().bg(BLACK));

        let mut state = TableState::default();
        if focused && !rows_data.is_empty() {
            state.select(Some(selected.min(rows_data.len() - 1)));
        }
        f.render_stateful_widget(table, col_inner, &mut state);
    }

    // Autocomplete popup when input is focused
    if matches!(app.supply_chain.focused, SupplyChainField::Input)
        && app.supply_chain.autocomplete.open
        && !app.supply_chain.autocomplete.items.is_empty()
        && matches!(panel_kind, SupplyChainPanel::Fmp)
    {
        let popup_y = app.layout.supplychain_input.y + 1;
        let popup_x = app.layout.supplychain_input.x + 11;
        let h = app.supply_chain.autocomplete.items.len().min(10) as u16;
        let popup = Rect {
            x: popup_x,
            y: popup_y,
            width: 50,
            height: h,
        };
        draw_autocomplete(f, &app.supply_chain.autocomplete, popup);
    }
}

fn truncate(s: &str, max_chars: usize) -> String {
    let n = s.chars().count();
    if n <= max_chars {
        s.to_string()
    } else {
        let take = max_chars.saturating_sub(1);
        format!("{}…", s.chars().take(take).collect::<String>())
    }
}

/// For each category, returns the set of tickers that appear in BOTH the FMP
/// and Claude panel for the same category. Used to bold rows that agree.
fn compute_supplychain_matches(
    state: &SupplyChainState,
) -> [std::collections::HashSet<String>; 3] {
    let mut out: [std::collections::HashSet<String>; 3] = Default::default();
    for (i, cat) in SC_CATEGORIES.iter().enumerate() {
        let fmp_tickers: std::collections::HashSet<String> = state
            .fmp
            .rows_for(*cat)
            .iter()
            .filter_map(|r| r.ticker.clone())
            .collect();
        let claude_tickers: std::collections::HashSet<String> = state
            .claude
            .rows_for(*cat)
            .iter()
            .filter_map(|r| r.ticker.clone())
            .collect();
        out[i] = fmp_tickers.intersection(&claude_tickers).cloned().collect();
    }
    out
}

// ── Shared helpers ────────────────────────────────────────────────────────────

fn panel(title: &str) -> Block<'static> {
    Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Plain)
        .border_style(Style::default().fg(ORANGE))
        .title(Span::styled(
            title.to_string(),
            Style::default().fg(ORANGE).add_modifier(Modifier::BOLD),
        ))
        .style(Style::default().bg(BLACK))
}

// ── Indicator toggle bar ─────────────────────────────────────────────────

/// Order of toggles on the INDICATORS row. Index is what the mouse hit-test
/// returns and what `app::toggle_indicator` consumes.
pub const IND_TOGGLES: &[&str] = &[
    "EMA(10)", "SMA(20)", "BB(20)", "VWAP", "VOL", "RSI(14)", "MACD",
];

/// Read-only accessor used by the renderer to highlight active toggles.
fn ind_is_on(app: &crate::app::App, i: usize) -> bool {
    let ind = &app.chart.indicators;
    match i {
        0 => ind.ema,
        1 => ind.sma,
        2 => ind.bollinger,
        3 => ind.vwap,
        4 => ind.volume,
        5 => ind.rsi,
        6 => ind.macd,
        _ => false,
    }
}

fn draw_chart_indicators_bar(f: &mut Frame, app: &mut App, area: Rect) {
    let prefix = "IND:    ";
    let mut spans: Vec<Span> = vec![Span::styled(prefix, Style::default().fg(GRAY2))];
    let mut hits: Vec<(u16, u16)> = Vec::with_capacity(IND_TOGGLES.len());
    let mut col = area.x + prefix.chars().count() as u16;
    for (i, label) in IND_TOGGLES.iter().enumerate() {
        let on = ind_is_on(app, i);
        let text = format!(" {} ", label);
        let w = text.chars().count() as u16;
        hits.push((col, col + w));
        col += w + 1;
        let style = if on {
            // Color-code active indicators by what they draw with on the chart
            // so the UI signals match the visuals.
            let c = match i {
                0 => CYAN,    // EMA
                1 => YELLOW,  // SMA
                2 => GRAY2,   // Bollinger
                3 => YELLOW,  // VWAP
                4 => ORANGE,  // Volume
                5 => CYAN,    // RSI
                6 => CYAN,    // MACD
                _ => WHITE,
            };
            Style::default().fg(BLACK).bg(c).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(GRAY2)
        };
        spans.push(Span::styled(text, style));
        spans.push(Span::raw(" "));
    }
    app.layout.chart_ind_bar = area;
    app.layout.chart_ind_hits = hits;
    f.render_widget(
        Paragraph::new(Line::from(spans)).style(Style::default().bg(BLACK)),
        area,
    );
}
