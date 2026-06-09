# Graph Report - .  (2026-06-07)

## Corpus Check
- 7 files · ~179,771 words
- Verdict: corpus is large enough that graph structure adds value.

## Summary
- 922 nodes · 2158 edges · 39 communities (35 shown, 4 thin omitted)
- Extraction: 93% EXTRACTED · 7% INFERRED · 0% AMBIGUOUS · INFERRED: 152 edges (avg confidence: 0.81)
- Token cost: 84,572 input · 0 output

## Community Hubs (Navigation)
- [[_COMMUNITY_Rust Alpaca API Models|Rust Alpaca API Models]]
- [[_COMMUNITY_Go Chart Rendering|Go Chart Rendering]]
- [[_COMMUNITY_Rust App State & Client|Rust App State & Client]]
- [[_COMMUNITY_Rust OrderAccount Ops|Rust Order/Account Ops]]
- [[_COMMUNITY_Cross-App Feature Concepts|Cross-App Feature Concepts]]
- [[_COMMUNITY_Rust Compare Tab State|Rust Compare Tab State]]
- [[_COMMUNITY_Go tview UI Widgets|Go tview UI Widgets]]
- [[_COMMUNITY_Rust egui Chart Plotting|Rust egui Chart Plotting]]
- [[_COMMUNITY_Rust FormattingClient Utils|Rust Formatting/Client Utils]]
- [[_COMMUNITY_Backtest UI App|Backtest UI App]]
- [[_COMMUNITY_Rust MessagingLayout|Rust Messaging/Layout]]
- [[_COMMUNITY_MarkovHMM Regime Tests|Markov/HMM Regime Tests]]
- [[_COMMUNITY_ADX Strategy|ADX Strategy]]
- [[_COMMUNITY_Strategy Simulation Tests|Strategy Simulation Tests]]
- [[_COMMUNITY_MACDRSI Strategies|MACD/RSI Strategies]]
- [[_COMMUNITY_Rust Command Palette Tests|Rust Command Palette Tests]]
- [[_COMMUNITY_HMM Gaussian Fitting|HMM Gaussian Fitting]]
- [[_COMMUNITY_Go Chart Tab & Time|Go Chart Tab & Time]]
- [[_COMMUNITY_Rust Indicator Tests|Rust Indicator Tests]]
- [[_COMMUNITY_Rust Background Workers|Rust Background Workers]]
- [[_COMMUNITY_Rust Indicator PrefsPersistence|Rust Indicator Prefs/Persistence]]
- [[_COMMUNITY_Rust Compare Strategies|Rust Compare Strategies]]
- [[_COMMUNITY_Backtest CredentialsConfig|Backtest Credentials/Config]]
- [[_COMMUNITY_Rust Asset Cache|Rust Asset Cache]]
- [[_COMMUNITY_Backtest Alpaca API|Backtest Alpaca API]]
- [[_COMMUNITY_Backtest Engine|Backtest Engine]]
- [[_COMMUNITY_Backtest UI Tests|Backtest UI Tests]]
- [[_COMMUNITY_Bollinger Strategy|Bollinger Strategy]]
- [[_COMMUNITY_Website Screenshot UI|Website Screenshot UI]]
- [[_COMMUNITY_App Logo & Branding|App Logo & Branding]]
- [[_COMMUNITY_Go CredentialsConfig|Go Credentials/Config]]
- [[_COMMUNITY_Claude Settings|Claude Settings]]
- [[_COMMUNITY_CLAUDE.md Authoring|CLAUDE.md Authoring]]
- [[_COMMUNITY_Rust Theme|Rust Theme]]
- [[_COMMUNITY_VSCode Launch Config|VSCode Launch Config]]
- [[_COMMUNITY_BaseModel|BaseModel]]
- [[_COMMUNITY_Box Widget|Box Widget]]

## God Nodes (most connected - your core abstractions)
1. `ChartApp` - 43 edges
2. `T` - 42 edges
3. `barsFromCloses()` - 37 edges
4. `termApp` - 36 edges
5. `TerminalState` - 30 edges
6. `T` - 27 edges
7. `startSimApp()` - 23 edges
8. `queueRead()` - 22 edges
9. `app` - 21 edges
10. `abs()` - 20 edges

## Surprising Connections (you probably didn't know these)
- `Backtest Terminal (Go + tview)` --semantically_similar_to--> `Main Trading Terminal (Go + tview) — Canonical`  [INFERRED] [semantically similar]
  backtest-terminal-go/CLAUDE.md → main-trading-terminal-go/CLAUDE.md
- `chartLoadGen Stale-Response Protection` --semantically_similar_to--> `Live TickCache (Arc<RwLock>) Streaming`  [INFERRED] [semantically similar]
  main-trading-terminal-go/CLAUDE.md → main-trading-terminal-rust/CLAUDE.md
- `adx()` --calls--> `abs()`  [INFERRED]
  backtest-terminal-go/adx.go → main-trading-terminal-go/braille.go
- `TestReturnsFromBarsBasic()` --calls--> `abs()`  [INFERRED]
  backtest-terminal-go/markov_test.go → main-trading-terminal-go/braille.go
- `TestLogSumExpBasic()` --calls--> `abs()`  [INFERRED]
  backtest-terminal-go/markov_test.go → main-trading-terminal-go/braille.go

## Import Cycles
- 1-file cycle: `main-trading-terminal-rust/src/api.rs -> main-trading-terminal-rust/src/api.rs`
- 1-file cycle: `main-trading-terminal-rust/src/app.rs -> main-trading-terminal-rust/src/app.rs`
- 1-file cycle: `main-trading-terminal-rust/src/chart.rs -> main-trading-terminal-rust/src/chart.rs`
- 1-file cycle: `main-trading-terminal-rust/src/stream.rs -> main-trading-terminal-rust/src/stream.rs`
- 1-file cycle: `main-trading-terminal-rust/src/compare.rs -> main-trading-terminal-rust/src/compare.rs`
- 1-file cycle: `main-trading-terminal-rust/src/config.rs -> main-trading-terminal-rust/src/config.rs`
- 1-file cycle: `main-trading-terminal-rust/src/indicators.rs -> main-trading-terminal-rust/src/indicators.rs`
- 1-file cycle: `main-trading-terminal-rust/src/persist.rs -> main-trading-terminal-rust/src/persist.rs`
- 1-file cycle: `main-trading-terminal-rust/src/stocks.rs -> main-trading-terminal-rust/src/stocks.rs`
- 1-file cycle: `main-trading-terminal-rust/src/strategies.rs -> main-trading-terminal-rust/src/strategies.rs`
- 1-file cycle: `main-trading-terminal-rust/src/terminal.rs -> main-trading-terminal-rust/src/terminal.rs`
- 1-file cycle: `main-trading-terminal-rust/src/watchlist.rs -> main-trading-terminal-rust/src/watchlist.rs`
- 1-file cycle: `main-trading-terminal-rust/src/workers.rs -> main-trading-terminal-rust/src/workers.rs`

## Hyperedges (group relationships)
- **Filenames live only on index.html cards; script.js and CI consume them** — website_index_download_cards, website_script_applyos, deploy_site_linux_binary_artifact [INFERRED 0.85]
- **Three-tab egui terminal workspaces** — readme_chart_tab, readme_compare_tab, readme_trading_terminal_tab, readme_alpaca_egui_app [EXTRACTED 1.00]
- **CI builds Linux binary then publishes site to GitHub Pages** — deploy_site_build_linux, deploy_site_deploy, deploy_site_linux_binary_artifact [EXTRACTED 1.00]

## Communities (39 total, 4 thin omitted)

### Community 0 - "Rust Alpaca API Models"
Cohesion: 0.08
Nodes (66): FillSide, Account, Activity, AlpacaClient, Arc, AssetCache, Bar, Color32 (+58 more)

### Community 1 - "Go Chart Rendering"
Cohesion: 0.10
Nodes (62): newBrailleLayer(), aggregateBars(), computeEMA(), drawString(), fmtVolume(), Bar, Color, Screen (+54 more)

### Community 2 - "Rust App State & Client"
Cohesion: 0.09
Nodes (52): AlpacaClient, Arc, AssetCache, Bar, Color32, Context, Msg, Option (+44 more)

### Community 3 - "Rust Order/Account Ops"
Cohesion: 0.09
Nodes (38): Account, Activity, AlpacaClient, Client, Credentials, Time, NewAlpacaClient(), Asset (+30 more)

### Community 4 - "Cross-App Feature Concepts"
Cohesion: 0.05
Nodes (52): Backtest Terminal (Go + tview), Backtest Regime/Strategy Engine, Release Binaries End-User README.txt, Chart+Compare GUI (Rust + egui), Pure-Parser Command Palette, Two-Phase Order Confirm Modal, egui Chart-Tab Indicator Hotkeys (V B S E U I O), Indicator Math (SMA/EMA/BB/RSI/MACD/VWAP/ATR) (+44 more)

### Community 5 - "Rust Compare Tab State"
Cohesion: 0.08
Nodes (31): AppState, Cell, Command, CompareState, EApp, AlpacaClient, Arc, AssetCache (+23 more)

### Community 6 - "Go tview UI Widgets"
Cohesion: 0.07
Nodes (25): DropDown, EventKey, Flex, Form, actRow, fmtMoney(), fmtPrice(), Account (+17 more)

### Community 7 - "Rust egui Chart Plotting"
Cohesion: 0.07
Nodes (44): AxisHints, BarMark, BoxElem, ChartApp, Fn, GridMark, LastTick, Line (+36 more)

### Community 8 - "Rust Formatting/Client Utils"
Cohesion: 0.13
Nodes (40): Debug, Formatter, AlpacaClient, Arc, Context, DateTime, Duration, Error (+32 more)

### Community 9 - "Backtest UI App"
Cohesion: 0.11
Nodes (19): app, fmtPct(), fmtPctSigned(), Application, Bar, Color, InputField, Int64 (+11 more)

### Community 10 - "Rust Messaging/Layout"
Cohesion: 0.13
Nodes (24): AlpacaClient, Arc, AssetCache, Msg, Option, Rect, Self, Sender (+16 more)

### Community 11 - "Markov/HMM Regime Tests"
Cohesion: 0.15
Nodes (22): NewMarkovChain(), T, TestClassifyReturnStateBoundaries(), TestHMMConvergesOnSyntheticTwoRegime(), TestHMMTradesAlignWithRegime(), TestHMMWarmupSafe(), TestLogSumExpBasic(), TestLogSumExpNumericalStability() (+14 more)

### Community 12 - "ADX Strategy"
Cohesion: 0.13
Nodes (22): adx(), Bar, Bar, Signal, closesOf(), Bar, T, TestADXBoundedAndRisesWithTrend() (+14 more)

### Community 13 - "Strategy Simulation Tests"
Cohesion: 0.26
Nodes (23): Signal, simulate(), barsFromCloses(), T, TestATRReflectsBarRange(), TestBBBoundsAreSymmetricAroundMean(), TestBuyHoldReturn(), TestEMASeedAndProgress() (+15 more)

### Community 14 - "MACD/RSI Strategies"
Cohesion: 0.18
Nodes (21): MACD, MACDRSI, TestRegistryIncludesMarkovStrategies(), Signal, Strategy, atr(), availableStrategies(), bb() (+13 more)

### Community 15 - "Rust Command Palette Tests"
Cohesion: 0.10
Nodes (8): Option, String, Command, is_tickerish(), Page, parse(), Side, TradeIntent

### Community 16 - "HMM Gaussian Fitting"
Cohesion: 0.16
Nodes (17): finite(), fitHMMGaussian(), gaussianLogPDF(), Bar, Signal, hmmForwardLog(), logSumExp(), normalize() (+9 more)

### Community 17 - "Go Chart Tab & Time"
Cohesion: 0.23
Nodes (4): Duration, termApp, Time, chartRange

### Community 18 - "Rust Indicator Tests"
Cohesion: 0.27
Nodes (18): Bar, atr_is_positive_when_there_is_range(), bars_with_closes(), bollinger_middle_equals_sma(), compute_atr(), compute_bollinger(), compute_ema(), compute_macd() (+10 more)

### Community 19 - "Rust Background Workers"
Cohesion: 0.44
Nodes (18): AlpacaClient, Arc, Context, OrderRequest, Sender, spawn_assets(), spawn_cancel_order(), spawn_place_order() (+10 more)

### Community 20 - "Rust Indicator Prefs/Persistence"
Cohesion: 0.20
Nodes (11): IndicatorPrefs, PathBuf, Result, Self, String, Vec, AppState, default_state_round_trips_through_json() (+3 more)

### Community 21 - "Rust Compare Strategies"
Cohesion: 0.43
Nodes (13): Bar, Vec, bars(), bollinger_alternates_buy_and_sell(), bollinger_signals(), ma_cross_buys_when_price_goes_above_then_sells_when_below(), ma_cross_signals(), macd_cross_signals_fire_at_line_crossings() (+5 more)

### Community 22 - "Backtest Credentials/Config"
Cohesion: 0.32
Nodes (10): configPath(), deleteCredentials(), loadCredentials(), runSetup(), Credentials, PathBuf, Result, config_path() (+2 more)

### Community 23 - "Rust Asset Cache"
Cohesion: 0.29
Nodes (7): Asset, HashMap, RwLock, Self, AssetCache, String, Vec

### Community 24 - "Backtest Alpaca API"
Cohesion: 0.31
Nodes (7): AlpacaClient, Client, Time, NewAlpacaClient(), Asset, Bar, barsResponse

### Community 25 - "Backtest Engine"
Cohesion: 0.38
Nodes (9): buyHoldReturn(), Bar, Duration, Strategy, Time, runStrategiesAtTimeframe(), sliceFrom(), Result (+1 more)

### Community 26 - "Backtest UI Tests"
Cohesion: 0.51
Nodes (9): contains(), app, T, queueRead(), startSimApp(), TestLowercaseQRTypeIntoSymbolField(), TestQAndRTypeIntoSymbolField(), TestQQuitsFromButtonFocus() (+1 more)

### Community 27 - "Bollinger Strategy"
Cohesion: 0.44
Nodes (7): NewBollingerBands(), T, TestBollingerBuysOnLowerBandTouch(), TestBollingerExitsAtMeanAfterLong(), TestBollingerPositionStateIsConsistent(), TestBollingerShortsOnUpperBandTouch(), TestBollingerWarmupSafe()

### Community 28 - "Website Screenshot UI"
Cohesion: 0.29
Nodes (8): AAPL Candlestick Price Chart, Top Command and Symbol Search Bar, Dark Bloomberg-Style Terminal Theme, Moving Average and Bollinger Band Overlays, MACD Sub-Panel with Histogram and Signal, RSI Sub-Panel, Top Tab Bar (Trading Terminal / Chart / Compare), Alpaca Trading Terminal Chart View Screenshot

### Community 29 - "App Logo & Branding"
Cohesion: 0.43
Nodes (7): App Logo (Candlestick Chart Icon), Candlestick Chart Motif, Alpaca Trading Terminal Brand, Bullish Uptrend Motif, Candlestick Chart Icon Motif, Dark Rounded-Square Glow Design Style, Download Website Branding

### Community 30 - "Go Credentials/Config"
Cohesion: 0.57
Nodes (6): configPath(), deleteCredentials(), loadCredentials(), runSetup(), saveCredentials(), Credentials

### Community 31 - "Claude Settings"
Cohesion: 0.50
Nodes (3): permissions, additionalDirectories, allow

### Community 33 - "CLAUDE.md Authoring"
Cohesion: 0.67
Nodes (3): CLAUDE.md Authoring Principles, Monorepo CLAUDE.md Splitting, claude-md-writer Skill

## Knowledge Gaps
- **137 isolated node(s):** `allow`, `additionalDirectories`, `version`, `configurations`, `Bar` (+132 more)
  These have ≤1 connection - possible missing edges or undocumented components.
- **4 thin communities (<3 nodes) omitted from report** — run `graphify query` to explore isolated nodes.

## Suggested Questions
_Questions this graph is uniquely positioned to answer:_

- **Why does `abs()` connect `Strategy Simulation Tests` to `Go Chart Rendering`, `Cross-App Feature Concepts`, `Markov/HMM Regime Tests`, `ADX Strategy`, `MACD/RSI Strategies`?**
  _High betweenness centrality (0.088) - this node is a cross-community bridge._
- **Why does `TestComputeEMAMath()` connect `Go Chart Rendering` to `Strategy Simulation Tests`?**
  _High betweenness centrality (0.060) - this node is a cross-community bridge._
- **Why does `newTermApp()` connect `Go Chart Rendering` to `Go tview UI Widgets`?**
  _High betweenness centrality (0.037) - this node is a cross-community bridge._
- **Are the 19 inferred relationships involving `barsFromCloses()` (e.g. with `TestBollingerBuysOnLowerBandTouch()` and `TestBollingerExitsAtMeanAfterLong()`) actually correct?**
  _`barsFromCloses()` has 19 INFERRED edges - model-reasoned connections that need verification._
- **What connects `allow`, `additionalDirectories`, `version` to the rest of the system?**
  _142 weakly-connected nodes found - possible documentation gaps or missing edges._
- **Should `Rust Alpaca API Models` be split into smaller, more focused modules?**
  _Cohesion score 0.0798442064264849 - nodes in this community are weakly interconnected._
- **Should `Go Chart Rendering` be split into smaller, more focused modules?**
  _Cohesion score 0.09903846153846153 - nodes in this community are weakly interconnected._