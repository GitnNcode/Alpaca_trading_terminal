// Bloomberg-style command palette parser.
//
// One pure function: `parse(&str) -> Command`. The renderer (in `app.rs`)
// applies the parsed Command by mutating ChartApp / CompareState /
// TerminalState — keeping this module side-effect-free so it can be
// exhaustively unit-tested.
//
// Grammar (case-insensitive, whitespace-separated):
//   <TICKER>                  → load on Chart tab
//   COMP <T1> [<T2> ...]      → replace Compare slots with these
//   PORT / POSITIONS          → jump to Trading Terminal / Positions
//   TRADE [<TICKER>]          → jump to Trade form; prefill if given
//   BUY  <QTY> <TICKER>       → Trade form prefill (Market, Buy)
//   SELL <QTY> <TICKER>       → Trade form prefill (Market, Sell)
//   ORDERS                    → Trading Terminal / Orders
//   ACT / ACTIVITY            → Trading Terminal / Activity
//   OPT <TICKER>              → Options desk, load chain
//   CRY [<PAIR>]              → Crypto desk; bare coin normalizes to /USD
//   WATCH <TICKER>            → add to watchlist (accepts Pairs: BTC/USD)
//   API CHANGE / API          → open the credentials modal to re-enter keys
//   API KEYS / API COPY       → show stored API key + secret with copy buttons
//   HELP / ?                  → show the help overlay
//
// Anything else parses to `Command::Unknown(input)` so the renderer can
// display "unknown command" rather than silently ignoring.

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    LoadSymbol(String),
    Compare(Vec<String>),
    GoTo(Page),
    Trade(TradeIntent),
    /// `OPT <TICKER>` — jump to the Options desk and load that underlying's
    /// chain. The actual load lives in `dispatch_command`.
    Options(String),
    /// `CRY [<PAIR>]` — jump to the Crypto desk. Bare `CRY` opens the Markets
    /// grid; `CRY BTC` normalizes to the `BTC/USD` Pair (the bare-coin →
    /// `/USD` inference is allowed ONLY here, inside a crypto-scoped code —
    /// globally `BTC` stays an equity ticker; see CONTEXT.md "Pair").
    Crypto(Option<String>),
    AddToWatchlist(String),
    /// Pop the credentials modal so the user can re-enter API key/secret +
    /// flip between paper and live. Mutation lives in `dispatch_command`;
    /// this variant is just the dispatch token.
    ApiChange,
    /// `API KEYS` / `API COPY` — pop a read-only modal listing the stored API
    /// key + secret, each with a copy-to-clipboard button. Reuse-friendly: lift
    /// the keys the app already loaded (from the shared credentials.json) into
    /// another tool without retyping. Pure display — never mutates credentials
    /// (that's `ApiChange`). Rendering lives in `app.rs`.
    ApiKeys,
    Help,
    Noop,
    Unknown(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Page {
    Positions,
    Orders,
    Activity,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Side {
    Buy,
    Sell,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TradeIntent {
    pub side: Option<Side>,
    /// Quantity as the user typed it — we keep it as a string so the Trade
    /// form's qty field can accept it verbatim (Alpaca takes string-typed
    /// quantities, and we round-trip rather than parse + reformat).
    pub qty: Option<String>,
    pub symbol: Option<String>,
}

/// Parse a user-typed command string. Empty input ⇒ `Noop`; anything that
/// doesn't match a known shape ⇒ `Unknown` so the UI can surface a clear
/// error instead of dropping the keystroke.
pub fn parse(input: &str) -> Command {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Command::Noop;
    }
    let mut tokens: Vec<&str> = trimmed.split_whitespace().collect();
    if tokens.is_empty() {
        return Command::Noop;
    }

    // Normalize the first token; arguments stay as-is and get uppercased
    // when interpreted as tickers below.
    let head = tokens[0].to_ascii_uppercase();
    let head_str: &str = head.as_str();
    tokens.remove(0);

    match head_str {
        "PORT" | "POSITIONS" => Command::GoTo(Page::Positions),
        "TRADE" => {
            // Optional symbol arg.
            let symbol = tokens.first().map(|t| t.to_ascii_uppercase());
            if let Some(sym) = &symbol {
                if !is_tickerish(sym) {
                    return Command::Unknown(input.to_string());
                }
            }
            Command::Trade(TradeIntent { side: None, qty: None, symbol })
        }
        "BUY" | "SELL" => {
            // Expect: <head> <qty> <ticker>
            if tokens.len() < 2 {
                return Command::Unknown(input.to_string());
            }
            let qty_raw = tokens[0];
            let sym = tokens[1].to_ascii_uppercase();
            if qty_raw.parse::<f64>().is_err() || !is_tickerish(&sym) {
                return Command::Unknown(input.to_string());
            }
            Command::Trade(TradeIntent {
                side: Some(if head_str == "BUY" { Side::Buy } else { Side::Sell }),
                qty: Some(qty_raw.to_string()),
                symbol: Some(sym),
            })
        }
        "ORDERS" => Command::GoTo(Page::Orders),
        "ACT" | "ACTIVITY" => Command::GoTo(Page::Activity),
        "OPT" | "OPTION" | "OPTIONS" => {
            // Requires exactly one tickerish underlying.
            match tokens.first().map(|t| t.to_ascii_uppercase()) {
                Some(sym) if is_tickerish(&sym) && tokens.len() == 1 => Command::Options(sym),
                _ => Command::Unknown(input.to_string()),
            }
        }
        "CRY" | "CRYPTO" => {
            // Optional pair arg. Crypto-scoped, so a bare coin ("BTC")
            // normalizes to its /USD Pair here — and only here.
            match tokens.first().map(|t| t.to_ascii_uppercase()) {
                None => Command::Crypto(None),
                Some(sym) if tokens.len() == 1 => {
                    let pair = normalize_pair(&sym);
                    if is_pairish(&pair) {
                        Command::Crypto(Some(pair))
                    } else {
                        Command::Unknown(input.to_string())
                    }
                }
                _ => Command::Unknown(input.to_string()),
            }
        }
        "COMP" | "COMPARE" => {
            let syms: Vec<String> = tokens
                .iter()
                .map(|t| t.to_ascii_uppercase())
                .filter(|t| is_tickerish(t))
                .collect();
            if syms.is_empty() {
                return Command::Unknown(input.to_string());
            }
            Command::Compare(syms)
        }
        "WATCH" => {
            let sym = tokens.first().map(|t| t.to_ascii_uppercase());
            match sym {
                Some(s) if is_tickerish(&s) => Command::AddToWatchlist(s),
                _ => Command::Unknown(input.to_string()),
            }
        }
        "API" => {
            // "API" alone or "API CHANGE" both pop the credentials modal;
            // "API KEYS" / "API COPY" pops the read-only copy-keys modal.
            // Any other follow-up token is treated as a typo so the user
            // sees a clear error instead of accidentally opening a modal.
            match tokens.first().map(|t| t.to_ascii_uppercase()).as_deref() {
                None => Command::ApiChange,
                Some("CHANGE") if tokens.len() == 1 => Command::ApiChange,
                Some("KEYS" | "COPY") if tokens.len() == 1 => Command::ApiKeys,
                _ => Command::Unknown(input.to_string()),
            }
        }
        "HELP" | "?" => Command::Help,
        // Bare ticker: just the symbol with nothing after it.
        single if is_tickerish(single) && tokens.is_empty() => {
            Command::LoadSymbol(single.to_string())
        }
        _ => Command::Unknown(input.to_string()),
    }
}

/// Cheap heuristic: an equity ticker (1–6 letters/dot/dash, all uppercase
/// ASCII) OR a crypto Pair in slash form (`BTC/USD`). Doesn't try to validate
/// against the asset cache — that's the caller's job. The slash form is the
/// ONLY global spelling for a Pair: bare coins collide with real equity
/// tickers (`BTC` is an ETF), so no /USD inference happens here.
fn is_tickerish(s: &str) -> bool {
    if is_pairish(s) {
        return true;
    }
    if s.is_empty() || s.len() > 6 {
        return false;
    }
    s.chars().all(|c| c.is_ascii_uppercase() || c == '.' || c == '-')
}

/// `BASE/QUOTE` with both legs 1–6 uppercase alphanumerics. Crypto bases can
/// carry digits (e.g. `1INCH`), equity tickers can't — hence alnum here only.
fn is_pairish(s: &str) -> bool {
    let mut parts = s.split('/');
    let (Some(base), Some(quote), None) = (parts.next(), parts.next(), parts.next()) else {
        return false;
    };
    let leg_ok = |l: &str| {
        !l.is_empty() && l.len() <= 6 && l.chars().all(|c| c.is_ascii_uppercase() || c.is_ascii_digit())
    };
    leg_ok(base) && leg_ok(quote)
}

/// Crypto-scoped normalization: a bare coin becomes its USD Pair
/// (`BTC` → `BTC/USD`); anything already slash-form passes through.
pub fn normalize_pair(s: &str) -> String {
    let s = s.trim().to_ascii_uppercase();
    if s.is_empty() || s.contains('/') {
        s
    } else {
        format!("{}/USD", s)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_is_noop() {
        assert_eq!(parse(""), Command::Noop);
        assert_eq!(parse("   "), Command::Noop);
    }

    #[test]
    fn bare_ticker_loads_symbol() {
        assert_eq!(parse("AAPL"), Command::LoadSymbol("AAPL".into()));
        assert_eq!(parse("aapl"), Command::LoadSymbol("AAPL".into()));
        assert_eq!(parse("  nvda  "), Command::LoadSymbol("NVDA".into()));
    }

    #[test]
    fn dotted_ticker_is_accepted() {
        // Berkshire B-shares, e.g.
        assert_eq!(parse("BRK.B"), Command::LoadSymbol("BRK.B".into()));
    }

    #[test]
    fn function_codes_jump_to_pages() {
        assert_eq!(parse("port"), Command::GoTo(Page::Positions));
        assert_eq!(parse("POSITIONS"), Command::GoTo(Page::Positions));
        assert_eq!(parse("orders"), Command::GoTo(Page::Orders));
        assert_eq!(parse("act"), Command::GoTo(Page::Activity));
        assert_eq!(parse("ACTIVITY"), Command::GoTo(Page::Activity));
    }

    #[test]
    fn comp_with_tickers_returns_them() {
        assert_eq!(
            parse("comp aapl msft nvda googl"),
            Command::Compare(vec!["AAPL".into(), "MSFT".into(), "NVDA".into(), "GOOGL".into()]),
        );
    }

    #[test]
    fn comp_without_tickers_is_unknown() {
        assert_eq!(parse("comp"), Command::Unknown("comp".into()));
    }

    #[test]
    fn buy_sell_prefills_trade() {
        assert_eq!(
            parse("buy 10 aapl"),
            Command::Trade(TradeIntent {
                side: Some(Side::Buy),
                qty: Some("10".into()),
                symbol: Some("AAPL".into()),
            }),
        );
        assert_eq!(
            parse("SELL 5 NVDA"),
            Command::Trade(TradeIntent {
                side: Some(Side::Sell),
                qty: Some("5".into()),
                symbol: Some("NVDA".into()),
            }),
        );
    }

    #[test]
    fn buy_with_garbage_qty_is_unknown() {
        assert_eq!(parse("buy abc aapl"), Command::Unknown("buy abc aapl".into()));
    }

    #[test]
    fn trade_with_symbol_only() {
        assert_eq!(
            parse("trade aapl"),
            Command::Trade(TradeIntent {
                side: None,
                qty: None,
                symbol: Some("AAPL".into()),
            }),
        );
    }

    #[test]
    fn help_aliases() {
        assert_eq!(parse("help"), Command::Help);
        assert_eq!(parse("?"), Command::Help);
        assert_eq!(parse("HELP"), Command::Help);
    }

    #[test]
    fn watch_adds_to_watchlist() {
        assert_eq!(parse("watch tsla"), Command::AddToWatchlist("TSLA".into()));
    }

    #[test]
    fn opt_jumps_to_options_desk() {
        assert_eq!(parse("opt aapl"), Command::Options("AAPL".into()));
        assert_eq!(parse("OPT nvda"), Command::Options("NVDA".into()));
        assert_eq!(parse("options tsla"), Command::Options("TSLA".into()));
    }

    #[test]
    fn opt_without_ticker_is_unknown() {
        assert_eq!(parse("opt"), Command::Unknown("opt".into()));
        assert_eq!(parse("opt aapl msft"), Command::Unknown("opt aapl msft".into()));
    }

    #[test]
    fn bare_pair_loads_symbol_globally() {
        // The slash form is the one global Pair spelling — it routes to the
        // Chart tab exactly like an equity ticker.
        assert_eq!(parse("BTC/USD"), Command::LoadSymbol("BTC/USD".into()));
        assert_eq!(parse("eth/usd"), Command::LoadSymbol("ETH/USD".into()));
    }

    #[test]
    fn bare_coin_stays_an_equity_ticker_globally() {
        // "BTC" is a real NYSE ticker (Grayscale Bitcoin Mini Trust) — no
        // global /USD inference.
        assert_eq!(parse("BTC"), Command::LoadSymbol("BTC".into()));
    }

    #[test]
    fn pairs_work_in_watch_and_comp() {
        assert_eq!(parse("watch btc/usd"), Command::AddToWatchlist("BTC/USD".into()));
        assert_eq!(
            parse("comp btc/usd eth/usd"),
            Command::Compare(vec!["BTC/USD".into(), "ETH/USD".into()]),
        );
    }

    #[test]
    fn cry_opens_crypto_desk() {
        assert_eq!(parse("cry"), Command::Crypto(None));
        assert_eq!(parse("CRYPTO"), Command::Crypto(None));
        // Crypto-scoped: bare coin normalizes to its /USD Pair here.
        assert_eq!(parse("cry btc"), Command::Crypto(Some("BTC/USD".into())));
        assert_eq!(parse("cry eth/usd"), Command::Crypto(Some("ETH/USD".into())));
        assert_eq!(parse("crypto sol"), Command::Crypto(Some("SOL/USD".into())));
    }

    #[test]
    fn cry_with_extra_tokens_is_unknown() {
        assert_eq!(parse("cry btc eth"), Command::Unknown("cry btc eth".into()));
    }

    #[test]
    fn malformed_pairs_are_rejected() {
        assert_eq!(parse("BTC//USD"), Command::Unknown("BTC//USD".into()));
        assert_eq!(parse("/USD"), Command::Unknown("/USD".into()));
        assert_eq!(parse("BTC/"), Command::Unknown("BTC/".into()));
    }

    #[test]
    fn api_change_opens_creds_modal() {
        assert_eq!(parse("api"), Command::ApiChange);
        assert_eq!(parse("API"), Command::ApiChange);
        assert_eq!(parse("api change"), Command::ApiChange);
        assert_eq!(parse("API CHANGE"), Command::ApiChange);
        assert_eq!(parse("  Api   Change  "), Command::ApiChange);
    }

    #[test]
    fn api_keys_opens_copy_modal() {
        assert_eq!(parse("api keys"), Command::ApiKeys);
        assert_eq!(parse("API KEYS"), Command::ApiKeys);
        assert_eq!(parse("api copy"), Command::ApiKeys);
        assert_eq!(parse("  Api   Copy  "), Command::ApiKeys);
    }

    #[test]
    fn api_with_garbage_arg_is_unknown() {
        assert_eq!(parse("api foo"), Command::Unknown("api foo".into()));
        assert_eq!(parse("api change extra"), Command::Unknown("api change extra".into()));
        assert_eq!(parse("api keys extra"), Command::Unknown("api keys extra".into()));
    }

    #[test]
    fn truly_unknown_input() {
        assert_eq!(parse("xyzzy 42"), Command::Unknown("xyzzy 42".into()));
        // Six-letter token isn't a known function code AND isn't a tickerish
        // string with trailing tokens, so it's Unknown.
        assert_eq!(parse("aapll extra"), Command::Unknown("aapll extra".into()));
    }
}
