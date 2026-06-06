# Graph Report - .  (2026-06-06)

## Corpus Check
- 100 files · ~145,758 words
- Verdict: corpus is large enough that graph structure adds value.

## Summary
- 1569 nodes · 3542 edges · 67 communities (65 shown, 2 thin omitted)
- Extraction: 95% EXTRACTED · 5% INFERRED · 0% AMBIGUOUS · INFERRED: 166 edges (avg confidence: 0.8)
- Token cost: 76,181 input · 0 output

## Community Hubs (Navigation)
- [[_COMMUNITY_Rust egui App State|Rust egui App State]]
- [[_COMMUNITY_Rust Ratatui App Core|Rust Ratatui App Core]]
- [[_COMMUNITY_Go Braille Chart Renderer|Go Braille Chart Renderer]]
- [[_COMMUNITY_Rust egui UI Rendering|Rust egui UI Rendering]]
- [[_COMMUNITY_Rust Input Handling|Rust Input Handling]]
- [[_COMMUNITY_Rust egui Compare State|Rust egui Compare State]]
- [[_COMMUNITY_Go tview TablesForms|Go tview Tables/Forms]]
- [[_COMMUNITY_Rust egui Chart Plotting|Rust egui Chart Plotting]]
- [[_COMMUNITY_Project Architecture Docs|Project Architecture Docs]]
- [[_COMMUNITY_Rust Client FormattingErrors|Rust Client Formatting/Errors]]
- [[_COMMUNITY_Rust Alpaca API Client|Rust Alpaca API Client]]
- [[_COMMUNITY_Go Backtest UI App|Go Backtest UI App]]
- [[_COMMUNITY_Electron MainSidecar|Electron Main/Sidecar]]
- [[_COMMUNITY_Rust API Client (port)|Rust API Client (port)]]
- [[_COMMUNITY_Rust UI Supply-Chain Draw|Rust UI Supply-Chain Draw]]
- [[_COMMUNITY_Rust Chart Canvas|Rust Chart Canvas]]
- [[_COMMUNITY_ElectronReact Dependencies|Electron/React Dependencies]]
- [[_COMMUNITY_Rust egui TickAsset Cache|Rust egui Tick/Asset Cache]]
- [[_COMMUNITY_Go Strategy Tests|Go Strategy Tests]]
- [[_COMMUNITY_React Compare Visualizations|React Compare Visualizations]]
- [[_COMMUNITY_Go MarkovHMM Tests|Go Markov/HMM Tests]]
- [[_COMMUNITY_Python FastAPI Server|Python FastAPI Server]]
- [[_COMMUNITY_Go Alpaca API Client|Go Alpaca API Client]]
- [[_COMMUNITY_TypeScript Config (renderer)|TypeScript Config (renderer)]]
- [[_COMMUNITY_Go Chart Tab Controls|Go Chart Tab Controls]]
- [[_COMMUNITY_Rust Command Palette Parser|Rust Command Palette Parser]]
- [[_COMMUNITY_Rust Terminal Setup|Rust Terminal Setup]]
- [[_COMMUNITY_Go ADXOptimization Tests|Go ADX/Optimization Tests]]
- [[_COMMUNITY_Rust LLMClaude Client|Rust LLM/Claude Client]]
- [[_COMMUNITY_React Chart Tab|React Chart Tab]]
- [[_COMMUNITY_Rust FMP Client|Rust FMP Client]]
- [[_COMMUNITY_Rust Indicator Math|Rust Indicator Math]]
- [[_COMMUNITY_Rust Indicator Math (port)|Rust Indicator Math (port)]]
- [[_COMMUNITY_Rust State Persistence|Rust State Persistence]]
- [[_COMMUNITY_Rust Async Workers|Rust Async Workers]]
- [[_COMMUNITY_Go MACD Strategies|Go MACD Strategies]]
- [[_COMMUNITY_React UI Components|React UI Components]]
- [[_COMMUNITY_Rust Async Workers (port)|Rust Async Workers (port)]]
- [[_COMMUNITY_Go HMM Strategy|Go HMM Strategy]]
- [[_COMMUNITY_TypeScript Config (main)|TypeScript Config (main)]]
- [[_COMMUNITY_Rust Strategy Signals|Rust Strategy Signals]]
- [[_COMMUNITY_Go Bollinger Strategy|Go Bollinger Strategy]]
- [[_COMMUNITY_Python Indicator Math|Python Indicator Math]]
- [[_COMMUNITY_TypeScript Bridge Types|TypeScript Bridge Types]]
- [[_COMMUNITY_Python Risk Metrics|Python Risk Metrics]]
- [[_COMMUNITY_React APIAutocomplete|React API/Autocomplete]]
- [[_COMMUNITY_Rust Asset Cache|Rust Asset Cache]]
- [[_COMMUNITY_Rust Asset Cache (port)|Rust Asset Cache (port)]]
- [[_COMMUNITY_Go API Client Setup|Go API Client Setup]]
- [[_COMMUNITY_Python Monte Carlo|Python Monte Carlo]]
- [[_COMMUNITY_Go Backtest Engine|Go Backtest Engine]]
- [[_COMMUNITY_Rust Credentials Config|Rust Credentials Config]]
- [[_COMMUNITY_Go Markov Chain Strategy|Go Markov Chain Strategy]]
- [[_COMMUNITY_Rust Credentials Config (port)|Rust Credentials Config (port)]]
- [[_COMMUNITY_Go Credentials Config|Go Credentials Config]]
- [[_COMMUNITY_Go Credentials Config (port)|Go Credentials Config (port)]]
- [[_COMMUNITY_Rust Main Entrypoint|Rust Main Entrypoint]]
- [[_COMMUNITY_Go Strategy Registry|Go Strategy Registry]]
- [[_COMMUNITY_App Logo Branding|App Logo Branding]]
- [[_COMMUNITY_Claude SettingsPermissions|Claude Settings/Permissions]]
- [[_COMMUNITY_CLAUDE.md Authoring Skill|CLAUDE.md Authoring Skill]]
- [[_COMMUNITY_Rust Theme|Rust Theme]]
- [[_COMMUNITY_VSCode Launch Config|VSCode Launch Config]]

## God Nodes (most connected - your core abstractions)
1. `ChartApp` - 43 edges
2. `T` - 42 edges
3. `barsFromCloses()` - 37 edges
4. `termApp` - 36 edges
5. `App` - 36 edges
6. `TerminalState` - 31 edges
7. `T` - 27 edges
8. `App` - 25 edges
9. `startSimApp()` - 23 edges
10. `queueRead()` - 22 edges

## Surprising Connections (you probably didn't know these)
- `Backtest Terminal (Go + tview)` --semantically_similar_to--> `Main Trading Terminal (Go + tview) — Canonical`  [INFERRED] [semantically similar]
  backtest-terminal-go/CLAUDE.md → main-trading-terminal-go/CLAUDE.md
- `Chart+Compare Bloomberg Build (CLAUDE.md)` --semantically_similar_to--> `Chart+Compare GUI (Rust + egui)`  [INFERRED] [semantically similar]
  chart-compare-bloomberg/CLAUDE.md → main-trading-terminal-rust/CLAUDE.md
- `Indicator Math (SMA/EMA/BB/RSI/MACD/VWAP/ATR)` --semantically_similar_to--> `Python FastAPI Sidecar`  [INFERRED] [semantically similar]
  main-trading-terminal-rust/README.md → chart-compare-bloomberg/CLAUDE.md
- `Monte Carlo (Xorshift64 + Box-Muller, numpy)` --semantically_similar_to--> `egui Monte Carlo (inline Xorshift64)`  [INFERRED] [semantically similar]
  chart-compare-bloomberg/CLAUDE.md → main-trading-terminal-rust/CLAUDE.md
- `Chart-Tab Indicator Hotkeys (V B S E U I O)` --semantically_similar_to--> `egui Chart-Tab Indicator Hotkeys (V B S E U I O)`  [INFERRED] [semantically similar]
  chart-compare-bloomberg/CLAUDE.md → main-trading-terminal-rust/CLAUDE.md

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
- 1-file cycle: `ratatui-trading-terminal-rust/src/api.rs -> ratatui-trading-terminal-rust/src/api.rs`
- 1-file cycle: `ratatui-trading-terminal-rust/src/app.rs -> ratatui-trading-terminal-rust/src/app.rs`
- 1-file cycle: `ratatui-trading-terminal-rust/src/chart.rs -> ratatui-trading-terminal-rust/src/chart.rs`
- 1-file cycle: `ratatui-trading-terminal-rust/src/config.rs -> ratatui-trading-terminal-rust/src/config.rs`
- 1-file cycle: `ratatui-trading-terminal-rust/src/fmp.rs -> ratatui-trading-terminal-rust/src/fmp.rs`
- 1-file cycle: `ratatui-trading-terminal-rust/src/indicators.rs -> ratatui-trading-terminal-rust/src/indicators.rs`
- 1-file cycle: `ratatui-trading-terminal-rust/src/input.rs -> ratatui-trading-terminal-rust/src/input.rs`

## Hyperedges (group relationships)
- **Shared credentials.json reused across all builds** — claude_shared_credentials_file, main_trading_terminal_go_claude, ratatui_trading_terminal_rust_claude, chart_compare_bloomberg_claude, backtest_terminal_go_claude, chart_compare_gui_rust_readme [EXTRACTED 1.00]
- **Chart+Compare GUI ported across egui and Electron stacks** — chart_compare_gui_rust_claude, chart_compare_bloomberg_claude, chart_compare_gui_rust_readme [EXTRACTED 0.85]
- **Xorshift64+Box-Muller Monte Carlo reproducible across Rust and Python** — chart_compare_gui_rust_montecarlo, chart_compare_bloomberg_montecarlo, chart_compare_bloomberg_python_sidecar [EXTRACTED 0.85]

## Communities (67 total, 2 thin omitted)

### Community 0 - "Rust egui App State"
Cohesion: 0.07
Nodes (69): FillSide, Account, Activity, AlpacaClient, Arc, AssetCache, Bar, Color32 (+61 more)

### Community 1 - "Rust Ratatui App Core"
Cohesion: 0.06
Nodes (50): ActRow, Account, Activity, AlpacaClient, Arc, AssetCache, Autocomplete, Bar (+42 more)

### Community 2 - "Go Braille Chart Renderer"
Cohesion: 0.10
Nodes (65): newBrailleLayer(), aggregateBars(), computeEMA(), drawString(), fmtVolume(), Bar, Box, Color (+57 more)

### Community 3 - "Rust egui UI Rendering"
Cohesion: 0.09
Nodes (53): AlpacaClient, Arc, AssetCache, Bar, Color32, Context, Msg, Option (+45 more)

### Community 4 - "Rust Input Handling"
Cohesion: 0.10
Nodes (64): KeyCode, MouseEvent, App, KeyEvent, Msg, Option, Rect, String (+56 more)

### Community 5 - "Rust egui Compare State"
Cohesion: 0.08
Nodes (32): AppState, Cell, Command, CompareState, EApp, AlpacaClient, Arc, AssetCache (+24 more)

### Community 6 - "Go tview Tables/Forms"
Cohesion: 0.08
Nodes (27): DropDown, EventKey, Flex, Form, actRow, activityToRow(), closedOrderToRow(), fmtMoney() (+19 more)

### Community 7 - "Rust egui Chart Plotting"
Cohesion: 0.07
Nodes (44): AxisHints, BarMark, BoxElem, ChartApp, Fn, GridMark, LastTick, Line (+36 more)

### Community 8 - "Project Architecture Docs"
Cohesion: 0.07
Nodes (41): Backtest Terminal (Go + tview), Backtest Regime/Strategy Engine, Release Binaries End-User README.txt, Chart+Compare Bloomberg Build (CLAUDE.md), Chart-Tab Indicator Hotkeys (V B S E U I O), lightweight-charts Pane Sync, Monte Carlo (Xorshift64 + Box-Muller, numpy), Python FastAPI Sidecar (+33 more)

### Community 9 - "Rust Client Formatting/Errors"
Cohesion: 0.12
Nodes (41): Debug, Formatter, AlpacaClient, Arc, Context, DateTime, Duration, Error (+33 more)

### Community 10 - "Rust Alpaca API Client"
Cohesion: 0.13
Nodes (27): Agent, Credentials, DateTime, Error, Option, Request, Response, Result (+19 more)

### Community 11 - "Go Backtest UI App"
Cohesion: 0.09
Nodes (28): app, fmtPct(), fmtPctSigned(), Application, Bar, Color, InputField, Int64 (+20 more)

### Community 12 - "Electron Main/Sidecar"
Cohesion: 0.10
Nodes (29): bool, Any, Credentials, str, str, findFreePort(), sidecarPort(), startSidecar() (+21 more)

### Community 13 - "Rust API Client (port)"
Cohesion: 0.15
Nodes (23): Agent, Credentials, DateTime, Error, Option, Request, Response, Result (+15 more)

### Community 14 - "Rust UI Supply-Chain Draw"
Cohesion: 0.20
Nodes (31): Block, App, Autocomplete, Frame, HashSet, Order, Rect, String (+23 more)

### Community 15 - "Rust Chart Canvas"
Cohesion: 0.17
Nodes (27): Buffer, F, IndicatorState, Bar, Color, Option, Rect, String (+19 more)

### Community 16 - "Electron/React Dependencies"
Cohesion: 0.07
Nodes (29): dependencies, lightweight-charts, react, react-dom, description, devDependencies, concurrently, electron (+21 more)

### Community 17 - "Rust egui Tick/Asset Cache"
Cohesion: 0.13
Nodes (24): AlpacaClient, Arc, AssetCache, Msg, Option, Rect, Self, Sender (+16 more)

### Community 18 - "Go Strategy Tests"
Cohesion: 0.22
Nodes (27): Signal, simulate(), NewMACDRSI(), barsFromCloses(), T, TestATRReflectsBarRange(), TestBBBoundsAreSymmetricAroundMean(), TestBuyHoldReturn() (+19 more)

### Community 19 - "React Compare Visualizations"
Cohesion: 0.11
Nodes (17): Heatmap(), Props, MonteCarlo(), Props, NormalizedAndDrawdown(), Props, Props, Scatter() (+9 more)

### Community 20 - "Go Markov/HMM Tests"
Cohesion: 0.15
Nodes (22): NewMarkovChain(), T, TestClassifyReturnStateBoundaries(), TestHMMConvergesOnSyntheticTwoRegime(), TestHMMTradesAlignWithRegime(), TestHMMWarmupSafe(), TestLogSumExpBasic(), TestLogSumExpNumericalStability() (+14 more)

### Community 21 - "Python FastAPI Server"
Cohesion: 0.17
Nodes (21): BaseModel, Any, Bar, Credentials, str, _bars_to_payload(), BarsBody, CompareBody (+13 more)

### Community 22 - "Go Alpaca API Client"
Cohesion: 0.16
Nodes (13): Account, Activity, AlpacaClient, Client, Credentials, Time, NewAlpacaClient(), Asset (+5 more)

### Community 23 - "TypeScript Config (renderer)"
Cohesion: 0.09
Nodes (22): compilerOptions, allowSyntheticDefaultImports, baseUrl, esModuleInterop, forceConsistentCasingInFileNames, isolatedModules, jsx, lib (+14 more)

### Community 24 - "Go Chart Tab Controls"
Cohesion: 0.20
Nodes (4): Duration, termApp, Time, chartRange

### Community 25 - "Rust Command Palette Parser"
Cohesion: 0.10
Nodes (8): Option, String, Command, is_tickerish(), Page, parse(), Side, TradeIntent

### Community 26 - "Rust Terminal Setup"
Cohesion: 0.15
Nodes (18): Credentials, CrosstermBackend, Frame, KeyEvent, Option, Rect, Result, Self (+10 more)

### Community 27 - "Go ADX/Optimization Tests"
Cohesion: 0.16
Nodes (20): adx(), Bar, closesOf(), Bar, T, TestADXBoundedAndRisesWithTrend(), TestADXChoppyStaysLow(), TestADXShortInputSafe() (+12 more)

### Community 28 - "Rust LLM/Claude Client"
Cohesion: 0.15
Nodes (18): ContentBlock, Agent, Option, Relation, Result, Self, String, SupplyChainData (+10 more)

### Community 29 - "React Chart Tab"
Cohesion: 0.13
Nodes (16): ChartTab(), COMMON_GRID, COMMON_LAYOUT, DEFAULTS, HOTKEYS, IndState, PALETTE, PanesProps (+8 more)

### Community 30 - "Rust FMP Client"
Cohesion: 0.21
Nodes (12): Agent, Option, Relation, Result, Self, String, SupplyChainData, Value (+4 more)

### Community 31 - "Rust Indicator Math"
Cohesion: 0.27
Nodes (18): Bar, atr_is_positive_when_there_is_range(), bars_with_closes(), bollinger_middle_equals_sma(), compute_atr(), compute_bollinger(), compute_ema(), compute_macd() (+10 more)

### Community 32 - "Rust Indicator Math (port)"
Cohesion: 0.27
Nodes (18): Bar, atr_is_positive_when_there_is_range(), bars_with_closes(), bollinger_middle_equals_sma(), compute_atr(), compute_bollinger(), compute_ema(), compute_macd() (+10 more)

### Community 33 - "Rust State Persistence"
Cohesion: 0.16
Nodes (13): IndicatorPrefs, Default, PathBuf, Result, Self, String, Vec, AppState (+5 more)

### Community 34 - "Rust Async Workers"
Cohesion: 0.44
Nodes (18): AlpacaClient, Arc, Context, OrderRequest, Sender, spawn_assets(), spawn_cancel_order(), spawn_place_order() (+10 more)

### Community 35 - "Go MACD Strategies"
Cohesion: 0.23
Nodes (12): MACD, MACDRSI, Signal, Strategy, atr(), bb(), ema(), Bar (+4 more)

### Community 36 - "React UI Components"
Cohesion: 0.15
Nodes (13): CompareTab(), CredentialsModal(), Props, FuncCode, FunctionBar(), Props, Props, StatusBar() (+5 more)

### Community 37 - "Rust Async Workers (port)"
Cohesion: 0.37
Nodes (15): AlpacaClient, Arc, ClaudeClient, FmpClient, Msg, OrderRequest, Sender, spawn_assets() (+7 more)

### Community 38 - "Go HMM Strategy"
Cohesion: 0.26
Nodes (12): finite(), fitHMMGaussian(), gaussianLogPDF(), Bar, Signal, hmmForwardLog(), logSumExp(), NewHMMStrategy() (+4 more)

### Community 39 - "TypeScript Config (main)"
Cohesion: 0.13
Nodes (14): compilerOptions, baseUrl, esModuleInterop, forceConsistentCasingInFileNames, lib, module, moduleResolution, outDir (+6 more)

### Community 40 - "Rust Strategy Signals"
Cohesion: 0.42
Nodes (14): Bar, Vec, bars(), bollinger_alternates_buy_and_sell(), bollinger_signals(), ma_cross_buys_when_price_goes_above_then_sells_when_below(), ma_cross_signals(), macd_cross_signals() (+6 more)

### Community 41 - "Go Bollinger Strategy"
Cohesion: 0.24
Nodes (10): Bar, Signal, NewBollingerBands(), T, TestBollingerBuysOnLowerBandTouch(), TestBollingerExitsAtMeanAfterLong(), TestBollingerPositionStateIsConsistent(), TestBollingerShortsOnUpperBandTouch() (+2 more)

### Community 42 - "Python Indicator Math"
Cohesion: 0.36
Nodes (13): float, int, ndarray, atr(), bollinger(), ema(), _ewma_seeded(), macd() (+5 more)

### Community 43 - "TypeScript Bridge Types"
Cohesion: 0.14
Nodes (13): Bar, BarsResponse, BollingerPayload, CcbBridge, ChartRange, CompareSeries, CompareSeriesErr, IndicatorKey (+5 more)

### Community 44 - "Python Risk Metrics"
Cohesion: 0.27
Nodes (11): float, ndarray, aligned(), compute(), correlation_matrix(), drawdown_series(), log_returns(), Metrics (+3 more)

### Community 45 - "React API/Autocomplete"
Cohesion: 0.22
Nodes (10): Props, SymbolAutocomplete(), base(), bridgeAvailable(), get(), port(), post(), rangeWindow() (+2 more)

### Community 46 - "Rust Asset Cache"
Cohesion: 0.29
Nodes (7): Asset, HashMap, RwLock, Self, AssetCache, String, Vec

### Community 47 - "Rust Asset Cache (port)"
Cohesion: 0.29
Nodes (7): Asset, HashMap, RwLock, Self, AssetCache, String, Vec

### Community 48 - "Go API Client Setup"
Cohesion: 0.27
Nodes (8): AlpacaClient, Client, Credentials, Time, NewAlpacaClient(), Asset, Bar, barsResponse

### Community 49 - "Python Monte Carlo"
Cohesion: 0.29
Nodes (9): float, int, ndarray, _box_muller(), MCResult, Geometric Brownian Monte Carlo with the same Xorshift64 + Box-Muller path genera, Generate `n` U(0,1) samples using xorshift64 — same algorithm as Rust impl., run() (+1 more)

### Community 50 - "Go Backtest Engine"
Cohesion: 0.38
Nodes (9): buyHoldReturn(), Bar, Duration, Strategy, Time, runStrategiesAtTimeframe(), sliceFrom(), Result (+1 more)

### Community 51 - "Rust Credentials Config"
Cohesion: 0.38
Nodes (9): Option, PathBuf, Result, config_path(), Credentials, delete_credentials(), load_credentials(), save_credentials() (+1 more)

### Community 52 - "Go Markov Chain Strategy"
Cohesion: 0.33
Nodes (6): classifyReturnState(), Bar, Signal, returnsFromBars(), rollingReturns(), MarkovChain

### Community 53 - "Rust Credentials Config (port)"
Cohesion: 0.44
Nodes (8): PathBuf, Result, config_path(), Credentials, delete_credentials(), load_credentials(), save_credentials(), String

### Community 54 - "Go Credentials Config"
Cohesion: 0.57
Nodes (6): configPath(), deleteCredentials(), loadCredentials(), runSetup(), saveCredentials(), Credentials

### Community 55 - "Go Credentials Config (port)"
Cohesion: 0.57
Nodes (6): configPath(), deleteCredentials(), loadCredentials(), runSetup(), saveCredentials(), Credentials

### Community 56 - "Rust Main Entrypoint"
Cohesion: 0.52
Nodes (6): CrosstermBackend, Result, main(), Stdout, Terminal, run()

### Community 57 - "Go Strategy Registry"
Cohesion: 0.60
Nodes (5): TestRegistryIncludesMarkovStrategies(), availableStrategies(), registerStrategies(), TestRegistryIncludesAllStrategies(), TestStrategyRegistryPopulated()

### Community 58 - "App Logo Branding"
Cohesion: 0.83
Nodes (4): App Logo (Candlestick Chart Icon), Candlestick Chart Motif, Alpaca Trading Terminal Brand, Bullish Uptrend Motif

### Community 59 - "Claude Settings/Permissions"
Cohesion: 0.50
Nodes (3): permissions, additionalDirectories, allow

### Community 61 - "CLAUDE.md Authoring Skill"
Cohesion: 0.67
Nodes (3): CLAUDE.md Authoring Principles, Monorepo CLAUDE.md Splitting, claude-md-writer Skill

## Knowledge Gaps
- **271 isolated node(s):** `allow`, `additionalDirectories`, `version`, `configurations`, `Bar` (+266 more)
  These have ≤1 connection - possible missing edges or undocumented components.
- **2 thin communities (<3 nodes) omitted from report** — run `graphify query` to explore isolated nodes.

## Suggested Questions
_Questions this graph is uniquely positioned to answer:_

- **Why does `abs()` connect `Go Strategy Tests` to `Go Braille Chart Renderer`, `Go MACD Strategies`, `Project Architecture Docs`, `Go Markov/HMM Tests`, `Go ADX/Optimization Tests`?**
  _High betweenness centrality (0.031) - this node is a cross-community bridge._
- **Why does `TestComputeEMAMath()` connect `Go Braille Chart Renderer` to `Go Strategy Tests`?**
  _High betweenness centrality (0.021) - this node is a cross-community bridge._
- **Are the 19 inferred relationships involving `barsFromCloses()` (e.g. with `TestBollingerBuysOnLowerBandTouch()` and `TestBollingerExitsAtMeanAfterLong()`) actually correct?**
  _`barsFromCloses()` has 19 INFERRED edges - model-reasoned connections that need verification._
- **What connects `allow`, `additionalDirectories`, `version` to the rest of the system?**
  _284 weakly-connected nodes found - possible documentation gaps or missing edges._
- **Should `Rust egui App State` be split into smaller, more focused modules?**
  _Cohesion score 0.07493388186893916 - nodes in this community are weakly interconnected._
- **Should `Rust Ratatui App Core` be split into smaller, more focused modules?**
  _Cohesion score 0.06169772256728778 - nodes in this community are weakly interconnected._
- **Should `Go Braille Chart Renderer` be split into smaller, more focused modules?**
  _Cohesion score 0.09745390693590869 - nodes in this community are weakly interconnected._