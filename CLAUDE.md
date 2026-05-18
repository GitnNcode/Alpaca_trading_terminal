# Alpaca Trading Terminal

Multi-implementation Alpaca trading terminal: one canonical TUI plus three architectural ports. Each implementation has its own folder and its own CLAUDE.md.

## Sub-projects

| Path | Stack | Status |
|------|-------|--------|
| [main-trading-terminal-go/](main-trading-terminal-go/CLAUDE.md) | Go 1.22 + tview + tcell | **Canonical** — feature-complete, 38 tests |
| [ratatui-trading-terminal-rust/](ratatui-trading-terminal-rust/CLAUDE.md) | Rust + ratatui + crossterm + ureq | Port, feature-matched to early state |
| [chart-compare-gui-rust/](chart-compare-gui-rust/CLAUDE.md) | Rust + eframe/egui + egui_plot | Chart + Compare GUI (multi-asset risk/return, Monte Carlo) |
| [backtest-terminal-go/](backtest-terminal-go/CLAUDE.md) | Go + tview (separate module) | Standalone strategy backtester |

When the user says "the app" without qualification, they mean **main-trading-terminal-go**. Bug reports and new features land there first. **Don't reflexively port changes to the other folders** — the ports are architectural references, not living code that must track every feature. Wait for an explicit ask.

## Shared across builds

- **Credentials file** (so binaries swap without re-entering keys):
  - macOS: `~/Library/Application Support/alpaca-tui/credentials.json`
  - Windows: `%APPDATA%\alpaca-tui\credentials.json`
  - Linux: `~/.config/alpaca-tui/credentials.json`
- **`trades.csv`** format is shared across builds.

## Repo-wide gotchas

- **Rust ports' `target/` folders are tracked in git** (not gitignored).
  - `git mv old_dir new_dir` on a Rust port can fail with "bad source" if the index has stale incremental `.o` files. Workaround: `mv old_dir new_dir && git add -A old_dir new_dir`.
  - Cargo rebuilds churn `git status` with megabytes of object files. Don't commit those — only `src/`, `Cargo.toml`, `Cargo.lock` belong in source-level commits.
- **No Go workspace.** Each Go folder has its own `go.mod`; build and test from inside the folder.
- **Release binaries are tracked** in [main-trading-terminal-go/bin/](main-trading-terminal-go/bin/) (Windows / macOS ARM / macOS Intel / Linux). End-users grab these directly — don't delete as "build artifacts."

## Don't

- Don't gitignore the Rust `target/` dirs without checking — they're deliberately tracked.
- Don't touch [backtest-terminal-go/](backtest-terminal-go/) unless the user is explicitly working on backtesting.
- Don't try to share code between Go modules — they're deliberately independent (no workspace, separate module names).
