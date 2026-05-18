// Hide the console window on Windows release builds — egui apps are GUIs.
#![cfg_attr(all(not(debug_assertions), target_os = "windows"), windows_subsystem = "windows")]

mod api;
mod app;
mod chart;
mod compare;
mod config;
mod indicators;
mod stocks;
mod strategies;
mod theme;
mod workers;

use std::env;
use std::io::{self, BufRead, Write};
use std::sync::Arc;

use api::AlpacaClient;
use app::ChartApp;

fn main() -> eframe::Result<()> {
    let reset = env::args().any(|a| a == "--reset");
    if reset {
        config::delete_credentials();
    }

    let creds = match config::load_credentials() {
        Ok(c) if !c.api_key.is_empty() => c,
        _ => {
            let c = run_stdin_setup();
            let _ = config::save_credentials(&c);
            c
        }
    };

    let client = Arc::new(AlpacaClient::new(creds));

    let native_options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_inner_size([1280.0, 800.0])
            .with_min_inner_size([900.0, 600.0])
            .with_title("Alpaca Chart"),
        ..Default::default()
    };
    eframe::run_native(
        "Alpaca Chart",
        native_options,
        Box::new(move |cc| Ok(Box::new(ChartApp::new(&cc.egui_ctx, client)))),
    )
}

fn run_stdin_setup() -> config::Credentials {
    let stdin = io::stdin();
    let mut stdout = io::stdout();
    println!("ALPACA CHART — FIRST-TIME SETUP");
    println!();
    let mut read = |prompt: &str| -> String {
        print!("  {}", prompt);
        let _ = stdout.flush();
        let mut s = String::new();
        let _ = stdin.lock().read_line(&mut s);
        s.trim().to_string()
    };
    let api_key = read("API Key:    ");
    let api_secret = read("API Secret: ");
    let env = read("Environment [paper/live] (default paper): ");
    let base_url = if env.to_lowercase().starts_with('l') {
        "https://api.alpaca.markets".to_string()
    } else {
        "https://paper-api.alpaca.markets".to_string()
    };
    config::Credentials { api_key, api_secret, base_url }
}
