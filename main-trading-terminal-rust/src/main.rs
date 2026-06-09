// Hide the console window on Windows release builds — egui apps are GUIs.
#![cfg_attr(all(not(debug_assertions), target_os = "windows"), windows_subsystem = "windows")]

mod api;
mod app;
mod chart;
mod command;
mod compare;
mod config;
mod indicators;
mod options;
mod persist;
mod stocks;
mod stream;
mod strategies;
mod terminal;
mod theme;
mod watchlist;
mod workers;

use std::env;
use std::sync::Arc;

use api::AlpacaClient;
use app::ChartApp;

fn main() -> eframe::Result<()> {
    let reset = env::args().any(|a| a == "--reset");
    if reset {
        config::delete_credentials();
    }

    // Always launch the GUI — even if credentials are missing or empty.
    // In that case ChartApp detects the unconfigured state and presents
    // the setup modal as a viewport-blocking screen; nothing else renders
    // until the user saves valid keys. This replaces the old stdin prompt
    // which only worked when launched from a terminal (broken under
    // packaged .app/.dmg/.exe distributions).
    let creds = config::load_credentials().unwrap_or_default();

    let client = Arc::new(AlpacaClient::new(creds));

    let native_options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_inner_size([1280.0, 800.0])
            .with_min_inner_size([900.0, 600.0])
            .with_title("Alpaca Trading Terminal"),
        ..Default::default()
    };
    eframe::run_native(
        "Alpaca Trading Terminal",
        native_options,
        Box::new(move |cc| Ok(Box::new(ChartApp::new(&cc.egui_ctx, client)))),
    )
}
