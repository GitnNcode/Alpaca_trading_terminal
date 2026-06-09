# Options Chain data: REST snapshot for structure + open interest, live WS for prices

The Options desk needs a live Chain. We feed it from **two** sources: a REST
snapshot (`/v1beta1/options/snapshots/{underlying}`, `feed=indicative`) returns
bid/ask + open interest + daily/prev bars for the *entire* Expiration in one
call — the instant initial fill plus open interest and %change, none of which
the quote stream carries — re-polled periodically; a **second** live WebSocket
(`wss://stream.data.alpaca.markets/v1beta1/{feed}`) then overlays real-time
bid/ask onto the contracts currently on screen.

This shape was confirmed by probing the live account, which also reshaped it:
Greeks/IV are **not available** on the `indicative` feed and `opra` returns
`403 "OPRA agreement is not signed"`, so **Greeks are dropped from v1** (Chain
shows bid/ask/size, last, %chg, volume, open interest, mark). The snapshot is
kept not for Greeks but because the stream carries neither open interest,
prev-close (%chg), nor a bulk initial quote for cold contracts.

## Considered Options

- **REST snapshot + ~10s poll only** (the Terminal-tab pattern). Simplest, no
  new streaming infra. Rejected: quotes lag up to ~10s, defeating a trading
  desk's Chain.
- **WS only.** Rejected: the quote stream carries no open interest, no
  prev-close (%chg), and no bulk initial fill, so cold contracts would render
  blank until they happen to tick.

## Consequences

- A second stream thread/module beyond the existing IEX stock stream in
  `stream.rs`. Subscription set = the displayed Expiration's calls+puts ∪ held
  option positions; it churns (unsubscribe old / subscribe new) every time the
  user changes Expiration.
- Feed defaults to `indicative` (works on paper, like IEX does for stocks),
  behind a single swap-point constant.
- Greeks/IV deferred, not impossible: revisit if the OPRA agreement is signed
  (feed carries them) or if client-side Black-Scholes + an IV solver is added
  (cheap per-snapshot, but BS is European vs Alpaca's American-style options).
