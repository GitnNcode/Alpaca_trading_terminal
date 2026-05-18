mod api;
mod app;
mod chart;
mod config;
mod indicators;
mod input;
mod setup;
mod stocks;
mod theme;
mod trade_log;
mod ui;
mod workers;

use std::env;
use std::io::{self, Stdout};
use std::sync::mpsc;
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::Result;
use crossterm::event::{self, DisableMouseCapture, EnableMouseCapture, Event};
use crossterm::execute;
use crossterm::terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen};
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;

use crate::api::AlpacaClient;
use crate::stocks::AssetCache;

fn main() -> Result<()> {
    let args: Vec<String> = env::args().collect();
    let reset = args.iter().any(|a| a == "--reset");

    if reset {
        config::delete_credentials();
    }

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal: Terminal<CrosstermBackend<Stdout>> = Terminal::new(backend)?;

    let result = run(&mut terminal);

    // Always restore terminal
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), DisableMouseCapture, LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    if let Err(e) = result {
        eprintln!("error: {}", e);
        std::process::exit(1);
    }
    Ok(())
}

fn run(terminal: &mut Terminal<CrosstermBackend<Stdout>>) -> Result<()> {
    let creds = match config::load_credentials() {
        Ok(c) if !c.api_key.is_empty() => c,
        _ => match setup::run_setup(terminal)? {
            Some(c) => {
                if let Err(e) = config::save_credentials(&c) {
                    eprintln!("warning: could not save credentials: {}", e);
                }
                c
            }
            None => return Ok(()),
        },
    };

    let client = Arc::new(AlpacaClient::new(creds));
    let assets = Arc::new(AssetCache::new());
    let (tx, rx) = mpsc::channel::<app::Msg>();

    let mut app = app::App::new(client.clone(), assets.clone(), tx.clone());

    workers::spawn_assets(client.clone(), tx.clone());
    workers::spawn_refresh(client.clone(), tx.clone());

    let tick_rate = Duration::from_millis(250);
    let mut last_tick = Instant::now();

    loop {
        terminal.draw(|f| ui::draw(f, &mut app))?;

        let timeout = tick_rate
            .checked_sub(last_tick.elapsed())
            .unwrap_or_else(|| Duration::from_millis(0));
        if event::poll(timeout)? {
            match event::read()? {
                Event::Key(k) => input::handle_key(&mut app, k),
                Event::Mouse(m) => input::handle_mouse(&mut app, m),
                _ => {}
            }
        }

        loop {
            match rx.try_recv() {
                Ok(msg) => input::handle_msg(&mut app, msg),
                Err(mpsc::TryRecvError::Empty) => break,
                Err(mpsc::TryRecvError::Disconnected) => break,
            }
        }

        if last_tick.elapsed() >= tick_rate {
            app.tick += 1;
            app.spinner_idx = app.spinner_idx.wrapping_add(1);
            if app.tick >= 40 {
                app.tick = 0;
                workers::spawn_refresh(client.clone(), tx.clone());
            }
            last_tick = Instant::now();
        }

        if app.should_quit {
            break;
        }
    }

    Ok(())
}
