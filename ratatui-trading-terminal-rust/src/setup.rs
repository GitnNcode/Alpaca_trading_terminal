use std::io::{self, Stdout};
use std::time::Duration;

use anyhow::Result;
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Clear, Paragraph};
use ratatui::{Frame, Terminal};
use ratatui::backend::CrosstermBackend;

use crate::config::Credentials;
use crate::theme::*;

#[derive(PartialEq, Eq)]
enum Field {
    Key,
    Secret,
    Env,
    Connect,
    Quit,
}

struct SetupState {
    key: String,
    secret: String,
    env_idx: usize, // 0 = paper, 1 = live
    focus: Field,
    err: String,
    done: Option<Credentials>,
    quit: bool,
}

impl SetupState {
    fn new() -> Self {
        SetupState {
            key: String::new(),
            secret: String::new(),
            env_idx: 0,
            focus: Field::Key,
            err: String::new(),
            done: None,
            quit: false,
        }
    }

    fn focus_next(&mut self) {
        self.focus = match self.focus {
            Field::Key => Field::Secret,
            Field::Secret => Field::Env,
            Field::Env => Field::Connect,
            Field::Connect => Field::Quit,
            Field::Quit => Field::Key,
        };
    }
    fn focus_prev(&mut self) {
        self.focus = match self.focus {
            Field::Key => Field::Quit,
            Field::Secret => Field::Key,
            Field::Env => Field::Secret,
            Field::Connect => Field::Env,
            Field::Quit => Field::Connect,
        };
    }

    fn try_connect(&mut self) {
        let key = self.key.trim().to_string();
        let secret = self.secret.trim().to_string();
        if key.is_empty() || secret.is_empty() {
            self.err = ">> API Key and Secret are both required.".to_string();
            return;
        }
        let base_url = if self.env_idx == 1 {
            "https://api.alpaca.markets".to_string()
        } else {
            "https://paper-api.alpaca.markets".to_string()
        };
        self.done = Some(Credentials {
            api_key: key,
            api_secret: secret,
            base_url,
            anthropic_api_key: None,
            fmp_api_key: None,
        });
    }
}

pub fn run_setup(
    terminal: &mut Terminal<CrosstermBackend<Stdout>>,
) -> Result<Option<Credentials>> {
    let mut state = SetupState::new();
    loop {
        terminal.draw(|f| draw(f, &state))?;
        if event::poll(Duration::from_millis(200))? {
            if let Event::Key(key) = event::read()? {
                if key.kind != KeyEventKind::Press {
                    continue;
                }
                handle_key(&mut state, key);
            }
        }
        if state.quit {
            return Ok(None);
        }
        if state.done.is_some() {
            return Ok(state.done);
        }
    }
}

fn handle_key(state: &mut SetupState, key: KeyEvent) {
    // Ctrl-C quits
    if key.modifiers.contains(KeyModifiers::CONTROL) && matches!(key.code, KeyCode::Char('c')) {
        state.quit = true;
        return;
    }
    match key.code {
        KeyCode::Esc => state.quit = true,
        KeyCode::Tab | KeyCode::Down => state.focus_next(),
        KeyCode::BackTab | KeyCode::Up => state.focus_prev(),
        KeyCode::Enter => match state.focus {
            Field::Connect => state.try_connect(),
            Field::Quit => state.quit = true,
            Field::Env => state.env_idx = (state.env_idx + 1) % 2,
            _ => state.focus_next(),
        },
        KeyCode::Left | KeyCode::Right if state.focus == Field::Env => {
            state.env_idx = (state.env_idx + 1) % 2;
        }
        KeyCode::Char(c) => match state.focus {
            Field::Key => state.key.push(c),
            Field::Secret => state.secret.push(c),
            _ => {}
        },
        KeyCode::Backspace => match state.focus {
            Field::Key => {
                state.key.pop();
            }
            Field::Secret => {
                state.secret.pop();
            }
            _ => {}
        },
        _ => {}
    }
}

fn draw(f: &mut Frame, state: &SetupState) {
    let area = f.area();
    let bg = Block::default().style(Style::default().bg(BLACK));
    f.render_widget(bg, area);

    // Centered 64-wide box
    let inner = centered_rect(64, 18, area);
    f.render_widget(Clear, inner);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Plain)
        .border_style(Style::default().fg(ORANGE))
        .title(Span::styled(
            " ALPACA TUI — FIRST-TIME SETUP ",
            Style::default().fg(ORANGE).add_modifier(Modifier::BOLD),
        ))
        .style(Style::default().bg(BLACK));
    let body = block.inner(inner);
    f.render_widget(block, inner);

    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Length(2),
            Constraint::Length(2),
            Constraint::Length(2),
            Constraint::Length(1),
            Constraint::Length(2),
            Constraint::Length(1),
            Constraint::Min(0),
        ])
        .split(body);

    let env_label = if state.env_idx == 0 {
        "Paper  (paper-api.alpaca.markets)"
    } else {
        "Live   (api.alpaca.markets)"
    };

    let secret_display: String = "*".repeat(state.secret.chars().count());

    let render_field = |label: &str, value: &str, focused: bool| -> Paragraph<'static> {
        let label_span = Span::styled(format!("  {:<14}", label), Style::default().fg(ORANGE));
        let value_style = if focused {
            Style::default().fg(WHITE).bg(POPUP_BG)
        } else {
            Style::default().fg(WHITE).bg(DARK)
        };
        let value_str = if focused {
            format!(" {} ", value)
        } else {
            format!(" {} ", value)
        };
        Paragraph::new(Line::from(vec![
            label_span,
            Span::styled(format!("{:<44}", value_str), value_style),
        ]))
        .style(Style::default().bg(BLACK))
    };

    f.render_widget(
        Paragraph::new("").style(Style::default().bg(BLACK)),
        layout[0],
    );
    f.render_widget(
        render_field("API Key", &state.key, state.focus == Field::Key),
        layout[1],
    );
    f.render_widget(
        render_field("API Secret", &secret_display, state.focus == Field::Secret),
        layout[2],
    );
    f.render_widget(
        render_field("Environment", env_label, state.focus == Field::Env),
        layout[3],
    );

    // Buttons
    let btn = |label: &str, focused: bool| -> Paragraph<'static> {
        let style = if focused {
            Style::default().fg(BLACK).bg(ORANGE).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(WHITE).bg(POPUP_BG)
        };
        Paragraph::new(Line::from(Span::styled(format!("  {}  ", label), style)))
            .alignment(Alignment::Left)
            .style(Style::default().bg(BLACK))
    };

    let button_row = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(20),
            Constraint::Length(10),
            Constraint::Min(0),
        ])
        .split(layout[5]);
    f.render_widget(btn("CONNECT", state.focus == Field::Connect), button_row[0]);
    f.render_widget(btn("QUIT", state.focus == Field::Quit), button_row[1]);

    let err = Paragraph::new(Line::from(Span::styled(
        format!("  {}", state.err),
        Style::default().fg(RED),
    )))
    .style(Style::default().bg(BLACK));
    f.render_widget(err, layout[6]);

    let hint = Paragraph::new(Line::from(Span::styled(
        "  Credentials saved to your OS config directory. Run with --reset to re-enter.",
        Style::default().fg(GRAY),
    )))
    .style(Style::default().bg(BLACK));
    f.render_widget(hint, layout[7]);
}

fn centered_rect(width: u16, height: u16, area: Rect) -> Rect {
    let x = area.x + area.width.saturating_sub(width) / 2;
    let y = area.y + area.height.saturating_sub(height) / 2;
    Rect {
        x,
        y,
        width: width.min(area.width),
        height: height.min(area.height),
    }
}

// Required helper to make the closure type work
pub fn _dummy(_: io::Result<()>) {}
