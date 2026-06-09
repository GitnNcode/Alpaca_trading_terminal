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
//   WATCH <TICKER>            → add to watchlist
//   API CHANGE / API          → open the credentials modal to re-enter keys
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
    AddToWatchlist(String),
    /// Pop the credentials modal so the user can re-enter API key/secret +
    /// flip between paper and live. Mutation lives in `dispatch_command`;
    /// this variant is just the dispatch token.
    ApiChange,
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
            // "API" alone or "API CHANGE" both pop the credentials modal.
            // Any other follow-up token is treated as a typo so the user
            // sees a clear error instead of accidentally opening the modal.
            match tokens.first().map(|t| t.to_ascii_uppercase()).as_deref() {
                None => Command::ApiChange,
                Some("CHANGE") if tokens.len() == 1 => Command::ApiChange,
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

/// Cheap heuristic: 1–6 letters/dot/dash, all uppercase ASCII. Doesn't try
/// to validate against the asset cache — that's the caller's job.
fn is_tickerish(s: &str) -> bool {
    if s.is_empty() || s.len() > 6 {
        return false;
    }
    s.chars().all(|c| c.is_ascii_uppercase() || c == '.' || c == '-')
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
    fn api_change_opens_creds_modal() {
        assert_eq!(parse("api"), Command::ApiChange);
        assert_eq!(parse("API"), Command::ApiChange);
        assert_eq!(parse("api change"), Command::ApiChange);
        assert_eq!(parse("API CHANGE"), Command::ApiChange);
        assert_eq!(parse("  Api   Change  "), Command::ApiChange);
    }

    #[test]
    fn api_with_garbage_arg_is_unknown() {
        assert_eq!(parse("api foo"), Command::Unknown("api foo".into()));
        assert_eq!(parse("api change extra"), Command::Unknown("api change extra".into()));
    }

    #[test]
    fn truly_unknown_input() {
        assert_eq!(parse("xyzzy 42"), Command::Unknown("xyzzy 42".into()));
        // Six-letter token isn't a known function code AND isn't a tickerish
        // string with trailing tokens, so it's Unknown.
        assert_eq!(parse("aapll extra"), Command::Unknown("aapll extra".into()));
    }
}
