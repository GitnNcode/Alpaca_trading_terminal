# Graph Report - Alpaca_trading_terminal  (2026-06-10)

## Corpus Check
- 55 files · ~197,226 words
- Verdict: corpus is large enough that graph structure adds value.

## Summary
- 1180 nodes · 2949 edges · 44 communities (39 shown, 5 thin omitted)
- Extraction: 95% EXTRACTED · 5% INFERRED · 0% AMBIGUOUS · INFERRED: 150 edges (avg confidence: 0.81)
- Token cost: 0 input · 0 output

## Graph Freshness
- Built from commit: `9ec233ff`
- Run `git rev-parse HEAD` and compare to check if the graph is stale.
- Run `graphify update .` after code changes (no API cost).

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
- [[_COMMUNITY_Community 39|Community 39]]
- [[_COMMUNITY_Community 40|Community 40]]
- [[_COMMUNITY_Community 41|Community 41]]
- [[_COMMUNITY_Community 42|Community 42]]
- [[_COMMUNITY_Community 43|Community 43]]

## God Nodes (most connected - your core abstractions)
1. `ChartApp` - 45 edges
2. `T` - 42 edges
3. `barsFromCloses()` - 37 edges
4. `termApp` - 36 edges
5. `CryptoState` - 32 edges
6. `TerminalState` - 32 edges
7. `OptionsState` - 31 edges
8. `T` - 27 edges
9. `Msg` - 24 edges
10. `Arc` - 24 edges

## Surprising Connections (you probably didn't know these)
- `Backtest Terminal (Go + tview)` --semantically_similar_to--> `Main Trading Terminal (Go + tview) — Canonical`  [INFERRED] [semantically similar]
  backtest-terminal-go/CLAUDE.md → main-trading-terminal-go/CLAUDE.md
- `adx()` --calls--> `abs()`  [INFERRED]
  backtest-terminal-go/adx.go → main-trading-terminal-go/braille.go
- `TestReturnsFromBarsBasic()` --calls--> `abs()`  [INFERRED]
  backtest-terminal-go/markov_test.go → main-trading-terminal-go/braille.go
- `TestLogSumExpBasic()` --calls--> `abs()`  [INFERRED]
  backtest-terminal-go/markov_test.go → main-trading-terminal-go/braille.go
- `TestLogSumExpNumericalStability()` --calls--> `abs()`  [INFERRED]
  backtest-terminal-go/markov_test.go → main-trading-terminal-go/braille.go

## Import Cycles
- 1-file cycle: `main-trading-terminal-rust/src/api.rs -> main-trading-terminal-rust/src/api.rs`
- 1-file cycle: `main-trading-terminal-rust/src/stream.rs -> main-trading-terminal-rust/src/stream.rs`
- 1-file cycle: `main-trading-terminal-rust/src/app.rs -> main-trading-terminal-rust/src/app.rs`
- 1-file cycle: `main-trading-terminal-rust/src/chart.rs -> main-trading-terminal-rust/src/chart.rs`
- 1-file cycle: `main-trading-terminal-rust/src/compare.rs -> main-trading-terminal-rust/src/compare.rs`
- 1-file cycle: `main-trading-terminal-rust/src/crypto.rs -> main-trading-terminal-rust/src/crypto.rs`
- 1-file cycle: `main-trading-terminal-rust/src/options.rs -> main-trading-terminal-rust/src/options.rs`
- 1-file cycle: `main-trading-terminal-rust/src/persist.rs -> main-trading-terminal-rust/src/persist.rs`
- 1-file cycle: `main-trading-terminal-rust/src/terminal.rs -> main-trading-terminal-rust/src/terminal.rs`
- 1-file cycle: `main-trading-terminal-rust/src/watchlist.rs -> main-trading-terminal-rust/src/watchlist.rs`
- 1-file cycle: `main-trading-terminal-rust/src/workers.rs -> main-trading-terminal-rust/src/workers.rs`
- 1-file cycle: `main-trading-terminal-rust/src/config.rs -> main-trading-terminal-rust/src/config.rs`
- 1-file cycle: `main-trading-terminal-rust/src/indicators.rs -> main-trading-terminal-rust/src/indicators.rs`
- 1-file cycle: `main-trading-terminal-rust/src/stocks.rs -> main-trading-terminal-rust/src/stocks.rs`
- 1-file cycle: `main-trading-terminal-rust/src/strategies.rs -> main-trading-terminal-rust/src/strategies.rs`

## Hyperedges (group relationships)
- **Filenames live only on index.html cards; script.js and CI consume them** — website_index_download_cards, website_script_applyos, deploy_site_linux_binary_artifact [INFERRED 0.85]
- **Three-tab egui terminal workspaces** — readme_chart_tab, readme_compare_tab, readme_trading_terminal_tab, readme_alpaca_egui_app [EXTRACTED 1.00]
- **CI builds Linux binary then publishes site to GitHub Pages** — deploy_site_build_linux, deploy_site_deploy, deploy_site_linux_binary_artifact [EXTRACTED 1.00]

## Communities (44 total, 5 thin omitted)

### Community 0 - "Rust Alpaca API Models"
Cohesion: 0.08
Nodes (69): Account, Activity, FillSide, AlpacaClient, Arc, AssetCache, Bar, Color32 (+61 more)

### Community 1 - "Go Chart Rendering"
Cohesion: 0.10
Nodes (58): brailleBit(), Screen, Style, newBrailleLayer(), brailleLayer, computeEMA(), approxEq(), contains() (+50 more)

### Community 2 - "Rust App State & Client"
Cohesion: 0.09
Nodes (57): AlpacaClient, Arc, AssetCache, Bar, Color32, Context, Msg, Option (+49 more)

### Community 3 - "Rust Order/Account Ops"
Cohesion: 0.08
Nodes (47): Agent, Credentials, D, DateTime, Error, HashMap, Option, Result (+39 more)

### Community 4 - "Cross-App Feature Concepts"
Cohesion: 0.06
Nodes (45): Backtest Terminal (Go + tview), Backtest Regime/Strategy Engine, Release Binaries End-User README.txt, Indicator Math (SMA/EMA/BB/RSI/MACD/VWAP/ATR), Linked Axis + Crosshair Multi-Pane, Alpaca Chart egui Tool README, Alpaca Trading Terminal (root project), No Go Workspace (independent modules) (+37 more)

### Community 5 - "Rust Compare Tab State"
Cohesion: 0.08
Nodes (34): AppState, Cell, Command, CompareState, CryptoState, EApp, Frame, AlpacaClient (+26 more)

### Community 6 - "Go tview UI Widgets"
Cohesion: 0.07
Nodes (25): DropDown, EventKey, Flex, Form, actRow, fmtMoney(), fmtPrice(), Account (+17 more)

### Community 7 - "Rust egui Chart Plotting"
Cohesion: 0.06
Nodes (52): AxisHints, BarMark, Box, BoxElem, ChartApp, Fn, GridMark, Id (+44 more)

### Community 8 - "Rust Formatting/Client Utils"
Cohesion: 0.12
Nodes (44): Debug, Duration, Formatter, AlpacaClient, Arc, Context, DateTime, Error (+36 more)

### Community 9 - "Backtest UI App"
Cohesion: 0.09
Nodes (28): app, fmtPct(), fmtPctSigned(), Application, Bar, Color, InputField, Int64 (+20 more)

### Community 10 - "Rust Messaging/Layout"
Cohesion: 0.13
Nodes (24): AlpacaClient, Arc, AssetCache, Msg, Option, Self, Sender, String (+16 more)

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
Cohesion: 0.07
Nodes (10): Option, String, Command, is_pairish(), is_tickerish(), normalize_pair(), Page, parse() (+2 more)

### Community 16 - "HMM Gaussian Fitting"
Cohesion: 0.16
Nodes (17): finite(), fitHMMGaussian(), gaussianLogPDF(), Bar, Signal, hmmForwardLog(), logSumExp(), normalize() (+9 more)

### Community 17 - "Go Chart Tab & Time"
Cohesion: 0.13
Nodes (12): aggregateBars(), drawString(), fmtVolume(), Bar, Color, Duration, termApp, Screen (+4 more)

### Community 18 - "Rust Indicator Tests"
Cohesion: 0.27
Nodes (18): Bar, atr_is_positive_when_there_is_range(), bars_with_closes(), bollinger_middle_equals_sma(), compute_atr(), compute_bollinger(), compute_ema(), compute_macd() (+10 more)

### Community 19 - "Rust Background Workers"
Cohesion: 0.32
Nodes (31): AlpacaClient, Arc, Context, OrderRequest, Sender, String, Vec, Msg (+23 more)

### Community 20 - "Rust Indicator Prefs/Persistence"
Cohesion: 0.18
Nodes (13): IndicatorPrefs, Default, Result, Self, String, Vec, PathBuf, AppState (+5 more)

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
Cohesion: 0.08
Nodes (67): AlpacaClient, Arc, AssetCache, Color32, Context, Default, HashMap, HashSet (+59 more)

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

### Community 39 - "Community 39"
Cohesion: 0.50
Nodes (3): Consequences, Considered Options, Options Chain data: REST snapshot for structure + open interest, live WS for prices

### Community 41 - "Community 41"
Cohesion: 0.07
Nodes (57): Asset, CryptoSnapshot, AlpacaClient, Arc, Color32, Context, Default, HashMap (+49 more)

### Community 42 - "Community 42"
Cohesion: 0.16
Nodes (13): Account, Activity, AlpacaClient, Client, Credentials, Time, NewAlpacaClient(), Asset (+5 more)

### Community 43 - "Community 43"
Cohesion: 0.50
Nodes (3): Consequences, Considered Options, Crypto desk: always-warm third stream + full Chart/Watchlist integration

## Knowledge Gaps
- **155 isolated node(s):** `Stack`, `Commands (run from this directory)`, `Architecture rules`, `Don't`, `Language` (+150 more)
  These have ≤1 connection - possible missing edges or undocumented components.
- **5 thin communities (<3 nodes) omitted from report** — run `graphify query` to explore isolated nodes.

## Suggested Questions
_Questions this graph is uniquely positioned to answer:_

- **Why does `abs()` connect `Strategy Simulation Tests` to `Go Chart Rendering`, `Markov/HMM Regime Tests`, `ADX Strategy`, `MACD/RSI Strategies`?**
  _High betweenness centrality (0.052) - this node is a cross-community bridge._
- **Why does `TestComputeEMAMath()` connect `Go Chart Rendering` to `Strategy Simulation Tests`?**
  _High betweenness centrality (0.033) - this node is a cross-community bridge._
- **Are the 19 inferred relationships involving `barsFromCloses()` (e.g. with `TestBollingerBuysOnLowerBandTouch()` and `TestBollingerExitsAtMeanAfterLong()`) actually correct?**
  _`barsFromCloses()` has 19 INFERRED edges - model-reasoned connections that need verification._
- **What connects `Stack`, `Commands (run from this directory)`, `Architecture rules` to the rest of the system?**
  _160 weakly-connected nodes found - possible documentation gaps or missing edges._
- **Should `Rust Alpaca API Models` be split into smaller, more focused modules?**
  _Cohesion score 0.07610931531002058 - nodes in this community are weakly interconnected._
- **Should `Go Chart Rendering` be split into smaller, more focused modules?**
  _Cohesion score 0.10119047619047619 - nodes in this community are weakly interconnected._
- **Should `Rust App State & Client` be split into smaller, more focused modules?**
  _Cohesion score 0.08637747336377473 - nodes in this community are weakly interconnected._