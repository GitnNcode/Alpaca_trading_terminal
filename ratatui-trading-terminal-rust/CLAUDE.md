# Ratatui trading terminal (Rust port)

Rust port of the canonical Go TUI. Feature-matched to an early state of the main build; **not kept in lockstep**.

## Stack
- Rust 2021, `ratatui` 0.28 + `crossterm` 0.28
- `ureq` for HTTP, `chrono`, `serde` / `serde_json`, `csv`, `dirs`, `anyhow`
- 13 source files in [src/](src/)

## Commands (run from this directory)
- Build: `cargo build --release` (output: `target/release/alpaca-rs`)
- Run: `cargo run --release`
- Reset stored credentials: `cargo run --release -- --reset`
- Test: `cargo test`
- Single test by name: `cargo test -- cycle_tf`

## Architecture notes

- **Immediate-mode renderer.** Unlike tview's retained-mode widgets in the main build, ratatui rebuilds the full UI on every frame from an immutable state struct.
- HTTP background work lives in [src/workers.rs](src/workers.rs); layout in [src/app.rs](src/app.rs) and [src/ui.rs](src/ui.rs).
- Setup / first-run flow is in [src/setup.rs](src/setup.rs), separate from the main app loop.

## Don't

- **Don't mirror every main-build change here.** The user wants this port to remain an architectural reference, not a moving target — wait for an explicit ask before porting.
- Don't commit Cargo `target/` churn. The dir is tracked at the repo level (see root CLAUDE.md), but a commit should only touch `src/`, `Cargo.toml`, or `Cargo.lock` unless the user says otherwise.
