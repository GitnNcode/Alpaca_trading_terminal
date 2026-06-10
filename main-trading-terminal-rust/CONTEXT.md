# Chart + Compare + Terminal GUI (Rust + egui)

Desktop trading terminal built on eframe/egui. Top-level tabs each own a
distinct trading surface. This glossary fixes the language for the Options
and Crypto work; the rest of the app's behaviour lives in CLAUDE.md.

## Language

**Options desk**:
The top-level Options tab. A self-contained options surface with its own
sub-tab strip (Chain / Positions / Orders), parallel to how the Terminal tab
owns Positions / Trade / Orders / Activity.
_Avoid_: Options panel, Options view (those imply read-only — the desk trades).

**Underlying**:
The equity symbol an option derives from (e.g. AAPL). The user picks one
Underlying; the Chain is everything listed against it.
_Avoid_: Ticker, base, root (root has a specific OCC meaning — see OCC symbol).

**Chain**:
The grid of option Contracts for one Underlying at one Expiration — calls on
one side, puts on the other, indexed by Strike.
_Avoid_: Option list, grid.

**Contract**:
A single tradable option (one Underlying + Expiration + Strike + call/put).
Identified on the wire by its OCC symbol. The unit of an options order is a
whole number of Contracts.
_Avoid_: Option (ambiguous — "options" is the asset class), instrument.

**OCC symbol**:
The 21-character options ticker Alpaca uses to identify a Contract, e.g.
`AAPL260619C00150000` (root + YYMMDD + C/P + strike×1000). This is the
`symbol` field on an options OrderRequest and on an options Position.
_Avoid_: Option symbol, contract id.

**Expiration**:
The date a Contract expires. The Chain is always shown for one Expiration at a
time; the user switches Expirations to see different Chains.
_Avoid_: Expiry date, maturity.

**Snapshot**:
The REST response (`/v1beta1/options/snapshots/{underlying}`) that supplies, in
one call for the whole Expiration, each Contract's bid/ask, open interest,
daily/prev bars (%chg, volume). Distinct from the live quote stream, which
carries only bid/ask updates. Together they form the Chain's data: Snapshot for
structure + open interest + initial quotes, stream for live prices.
_Avoid_: Quote dump, chain fetch.

**Crypto desk**:
The top-level Crypto tab. A self-contained crypto trading surface with its own
sub-tab strip (Markets / Positions / Orders), parallel to the Options desk.
_Avoid_: Crypto panel, Crypto view (the desk trades).

**Pair**:
A tradable crypto symbol quoted against USD, written with a slash
(e.g. `BTC/USD`). The Pair is the unit the user browses, holds, and trades —
the crypto analog of a Contract. The slash form is the only spelling
recognized globally (palette, Chart, Watchlist): bare coin symbols collide
with real equity tickers (`BTC` is the Grayscale Bitcoin Mini Trust ETF), so
a bare coin normalizes to its `/USD` Pair only inside crypto-scoped inputs
(the `CRY` palette code and the Crypto desk's own pair input).
_Avoid_: Coin, token, ticker (a Pair is a market, not an asset).

**Markets**:
The Crypto desk's first sub-tab: the grid of all tradable Pairs with live
prices. Plays the role the Chain plays in Options — the browsable surface
that doubles as the entry point to the trade ticket (click a row, trade
that Pair).
_Avoid_: Pair list, watchlist (the Watchlist is a different, cross-tab thing).

**Notional**:
A crypto order sized in dollars instead of units ("$100 of BTC"). Valid only
on market orders — a limit or stop-limit order must be sized in quantity.
The ticket offers Qty and Notional as two modes of one size field, never both.
_Avoid_: Dollar amount, value (ambiguous against market value of a position).

**Greeks** _(deferred — not in v1)_:
Risk sensitivities (IV, delta, theta, gamma, vega). Alpaca returns them only on
the paid `opra` feed; the account's `indicative` feed omits them entirely, so
the v1 Chain shows none. Listed here only so the term isn't re-proposed as if
available — see ADR-0001.
_Avoid_: Sensitivities, risk metrics.
