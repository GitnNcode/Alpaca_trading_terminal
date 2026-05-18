# Backtest terminal (Go + tview)

Standalone strategy backtester. **Not part of the trading terminal** — own Go module (`module backtest-tui`), no shared imports.

## Stack
- Go 1.22, `package main`, own `go.mod`
- tview + tcell (same library set as the main build, but an independent module)

## Commands (run from this directory)
- Build: `go build .` (output: `backtest-tui`)
- Run: `./backtest-tui`
- Test: `go test ./...`

## What's here

Multiple regime-detection / strategy combinations, tests alongside source:

- [bollinger.go](bollinger.go) + [bollinger_test.go](bollinger_test.go) — Bollinger band strategy
- [adx.go](adx.go) — ADX
- [hmm.go](hmm.go) — Hidden Markov model
- [markov_chain.go](markov_chain.go) + [markov_test.go](markov_test.go) — Markov chain
- [regime_switch.go](regime_switch.go) — regime switching
- [strategy.go](strategy.go) + [strategy_test.go](strategy_test.go) + [optimization_test.go](optimization_test.go) — strategy framework and parameter search
- [backtest.go](backtest.go) — backtest engine
- [ui_test.go](ui_test.go) — TUI tests
- [api.go](api.go), [stocks.go](stocks.go), [config.go](config.go) — share concepts with the main build (credentials path, asset list), but **the code is duplicated here, not imported**

## Don't

- **Don't touch this folder unless the user explicitly asks about backtesting.** Changes here don't need to be mirrored into the trading-terminal builds, and vice versa.
- Don't try to share Go packages with [../main-trading-terminal-go/](../main-trading-terminal-go/) — there's no Go workspace and the modules are deliberately independent.
