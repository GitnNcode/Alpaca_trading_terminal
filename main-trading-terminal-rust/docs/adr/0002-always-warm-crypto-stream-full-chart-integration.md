# Crypto desk: always-warm third stream + full Chart/Watchlist integration

The Crypto desk (Tab::Crypto) deliberately breaks the containment precedent
ADR-0001 set for Options. Crypto Pairs are first-class across the app: the
Chart tab charts them (bars branch to `/v1beta3/crypto/us/bars`), the
Watchlist/ticker tape carry them, and a THIRD WebSocket
(`wss://stream.data.alpaca.markets/v1beta3/crypto/us`) stays **always active**
— it does not participate in the stock↔options `SetActive` handoff. Its
subscription set (chart Pair ∪ Compare Pair slots ∪ watchlist Pairs ∪ held
crypto positions ∪ Markets-grid Pairs while the tab is open) just goes empty
when nothing crypto is on screen.

Why the deviation: options containment was cheap because charting an OCC
contract is meaningless and the options market keeps equity hours. Crypto is
the opposite on both axes — the market runs 24/7 (live prices are interesting
from any tab) and Alpaca's crypto bars are shaped identically to stock bars,
so the whole indicator / live-patching stack works on `BTC/USD` unchanged.

## Considered Options

- **Contained, tab-gated stream (Options parity).** Rejected: a 24/7 asset
  that goes dark when you switch tabs feels broken in a way options never did,
  and it forfeits charting — half the value of crypto in this app.
- **Fold crypto into the stock↔options SetActive handoff.** Rejected: the
  one-connection limit that forced the handoff was empirically confirmed for
  the *stock + options* feeds; the crypto endpoint is a separate free product
  with its own connection allowance, so serializing it would cost liveness for
  nothing. **Assumption to watch:** if the crypto socket ever starts logging
  406 "connection limit exceeded", the allowance turned out to be shared —
  fall back to adding crypto to the handoff. Degraded mode is safe either way
  (the stream just retries with backoff; stock stays connected).

## Consequences

- Symbol identity: the slash form (`BTC/USD`) is the only global spelling;
  bare coins normalize to `/USD` only inside crypto-scoped inputs (`CRY`
  palette code, the desk's pair input) because bare coins collide with real
  equity tickers (`BTC` = Grayscale Bitcoin Mini Trust). The trading API
  returns crypto symbols slashless (`BTCUSD`) — positions/orders are
  canonicalized to the slash form at the API boundary so tick-cache lookups
  and fill markers join correctly.
- The stock stream's subscription union must EXCLUDE Pairs (the IEX feed
  rejects them); the crypto stream gets them instead.
- Crypto sizes/volumes are fractional on the wire, so the stream frames and
  `Bar` volume parse as f64 (stock payloads still parse — integers are valid
  f64 JSON).
- Ticket scope is the full crypto surface: market/limit/stop-limit, GTC/IOC,
  qty or notional (notional is market-only per Alpaca). TIF `day` does not
  exist for crypto.
