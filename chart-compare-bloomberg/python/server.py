"""FastAPI sidecar. Electron main spawns this with a port argument and an
optional auth token; the renderer talks to it via localhost HTTP.

Endpoints
  GET  /health                         -> {ok: true, version, has_credentials}
  POST /credentials   {api_key, api_secret, base_url} -> {ok}
  POST /credentials/clear              -> {ok}
  GET  /assets                         -> [{symbol, name}, ...]
  POST /bars          {symbol, timeframe, start, end} -> {bars: [...], indicators: {...}}
  POST /compare       {symbols, range}                 -> per-symbol bars + metrics + matrix
  POST /montecarlo    {symbol, range, horizon_years, n_sims, seed} -> MCResult
"""

from __future__ import annotations

import argparse
import asyncio
import datetime as dt
import os
import sys
from typing import Any

import numpy as np
import uvicorn
from fastapi import FastAPI, HTTPException
from fastapi.middleware.cors import CORSMiddleware
from pydantic import BaseModel, Field

import alpaca
import config
import indicators as ind
import metrics as met
import monte_carlo as mc


APP_VERSION = "0.1.0"
_assets_cache: list[alpaca.Asset] = []


# ----- request models -----

class CredsBody(BaseModel):
    api_key: str
    api_secret: str
    base_url: str = "https://paper-api.alpaca.markets"


class BarsBody(BaseModel):
    symbol: str
    timeframe: str = "1Day"
    start: str   # ISO
    end: str     # ISO
    indicators: list[str] = Field(default_factory=list)  # ["ema","sma","bb","vwap","rsi","macd"]


class CompareBody(BaseModel):
    symbols: list[str]
    range: str = "3Y"   # 1Y, 3Y, 5Y, 10Y


class MCBody(BaseModel):
    symbol: str
    range: str = "5Y"
    horizon_years: float = 5.0
    n_sims: int = 1000
    seed: int = 0xDEADBEEF


# ----- helpers -----

def _now_utc_iso() -> str:
    return dt.datetime.now(dt.timezone.utc).isoformat(timespec="seconds")


def _range_to_window(range_key: str) -> tuple[str, str]:
    end = dt.datetime.now(dt.timezone.utc)
    deltas = {
        "1D": dt.timedelta(days=2),
        "1W": dt.timedelta(days=10),
        "1M": dt.timedelta(days=45),
        "YTD": end - dt.datetime(end.year, 1, 1, tzinfo=dt.timezone.utc),
        "1Y": dt.timedelta(days=400),
        "3Y": dt.timedelta(days=365 * 3 + 30),
        "5Y": dt.timedelta(days=365 * 5 + 30),
        "10Y": dt.timedelta(days=365 * 10 + 30),
        "MAX": dt.timedelta(days=365 * 25),
    }
    delta = deltas.get(range_key, dt.timedelta(days=400))
    # data API rejects requests for the most recent ~15 minutes on the free
    # IEX feed; back off slightly
    end = end - dt.timedelta(minutes=20)
    start = end - delta
    return start.isoformat(timespec="seconds"), end.isoformat(timespec="seconds")


def _require_creds() -> config.Credentials:
    c = config.load()
    if not c:
        raise HTTPException(status_code=401, detail="no credentials configured")
    return c


def _bars_to_payload(bars: list[alpaca.Bar]) -> list[dict[str, Any]]:
    return [b.to_dict() for b in bars]


def _compute_indicators(bars: list[alpaca.Bar], names: list[str]) -> dict[str, Any]:
    if not bars:
        return {}
    closes = np.array([b.c for b in bars], dtype=np.float64)
    highs = np.array([b.h for b in bars], dtype=np.float64)
    lows = np.array([b.l for b in bars], dtype=np.float64)
    vols = np.array([b.v for b in bars], dtype=np.float64)
    out: dict[str, Any] = {}
    for n in names:
        n = n.lower()
        if n == "ema":
            out["ema"] = ind.nan_to_none(ind.ema(closes, 20))
        elif n == "sma":
            out["sma"] = ind.nan_to_none(ind.sma(closes, 50))
        elif n == "bb":
            u, m, l = ind.bollinger(closes, 20, 2.0)
            out["bb"] = {
                "upper": ind.nan_to_none(u),
                "mid": ind.nan_to_none(m),
                "lower": ind.nan_to_none(l),
            }
        elif n == "vwap":
            out["vwap"] = ind.nan_to_none(ind.vwap(highs, lows, closes, vols))
        elif n == "rsi":
            out["rsi"] = ind.nan_to_none(ind.rsi(closes, 14))
        elif n == "macd":
            line, sig, hist = ind.macd(closes, 12, 26, 9)
            out["macd"] = {
                "line": ind.nan_to_none(line),
                "signal": ind.nan_to_none(sig),
                "hist": ind.nan_to_none(hist),
            }
    return out


# ----- app -----

app = FastAPI(title="chart-compare-bloomberg", version=APP_VERSION)
app.add_middleware(
    CORSMiddleware,
    allow_origins=["*"],
    allow_methods=["*"],
    allow_headers=["*"],
)


@app.get("/health")
async def health():
    return {"ok": True, "version": APP_VERSION, "has_credentials": config.load() is not None, "ts": _now_utc_iso()}


@app.post("/credentials")
async def set_credentials(body: CredsBody):
    creds = config.Credentials(api_key=body.api_key.strip(), api_secret=body.api_secret.strip(), base_url=body.base_url.strip() or "https://paper-api.alpaca.markets")
    config.save(creds)
    return {"ok": True}


@app.post("/credentials/clear")
async def clear_credentials():
    config.clear()
    return {"ok": True}


@app.get("/assets")
async def get_assets():
    global _assets_cache
    if _assets_cache:
        return [a.to_dict() for a in _assets_cache]
    creds = _require_creds()
    try:
        _assets_cache = await alpaca.fetch_assets(creds)
    except Exception as e:
        raise HTTPException(status_code=502, detail=f"assets: {e}")
    return [a.to_dict() for a in _assets_cache]


@app.post("/bars")
async def post_bars(body: BarsBody):
    creds = _require_creds()
    try:
        bars = await alpaca.fetch_bars(creds, body.symbol, body.timeframe, body.start, body.end)
    except Exception as e:
        raise HTTPException(status_code=502, detail=f"bars: {e}")
    return {
        "symbol": body.symbol.upper(),
        "timeframe": body.timeframe,
        "bars": _bars_to_payload(bars),
        "indicators": _compute_indicators(bars, body.indicators),
    }


@app.post("/compare")
async def post_compare(body: CompareBody):
    symbols = [s.strip().upper() for s in body.symbols if s.strip()]
    if not symbols:
        raise HTTPException(status_code=400, detail="no symbols")
    creds = _require_creds()
    start, end = _range_to_window(body.range)

    async def one(sym: str):
        try:
            return sym, await alpaca.fetch_bars(creds, sym, "1Day", start, end)
        except Exception as e:
            return sym, e

    results = await asyncio.gather(*[one(s) for s in symbols])

    payload: dict[str, Any] = {"range": body.range, "series": [], "matrix": [], "labels": []}
    series_closes: list[np.ndarray] = []
    series_rets: list[np.ndarray] = []
    series_times: list[list[str]] = []

    for sym, res in results:
        if isinstance(res, Exception):
            payload["series"].append({"symbol": sym, "error": str(res)})
            continue
        bars: list[alpaca.Bar] = res
        if len(bars) < 2:
            payload["series"].append({"symbol": sym, "error": "not enough bars"})
            continue
        closes = np.array([b.c for b in bars], dtype=np.float64)
        m = met.compute(closes)
        payload["series"].append({
            "symbol": sym,
            "bars": _bars_to_payload(bars),
            "metrics": m.to_dict(),
            "normalized": met.normalized_series(closes).tolist(),
            "drawdown": met.drawdown_series(closes).tolist(),
            "times": [b.t for b in bars],
        })
        series_closes.append(closes)
        series_rets.append(met.log_returns(closes))
        series_times.append([b.t for b in bars])
        payload["labels"].append(sym)

    if len(series_rets) >= 2:
        payload["matrix"] = met.correlation_matrix(series_rets)
    elif len(series_rets) == 1:
        payload["matrix"] = [[1.0]]
    return payload


@app.post("/montecarlo")
async def post_mc(body: MCBody):
    creds = _require_creds()
    start, end = _range_to_window(body.range)
    try:
        bars = await alpaca.fetch_bars(creds, body.symbol, "1Day", start, end)
    except Exception as e:
        raise HTTPException(status_code=502, detail=f"bars: {e}")
    if len(bars) < 30:
        raise HTTPException(status_code=400, detail="not enough history")
    closes = np.array([b.c for b in bars], dtype=np.float64)
    result = mc.run(closes, body.horizon_years, body.n_sims, body.seed)
    return result.to_dict()


# ----- entrypoint -----

def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--port", type=int, default=int(os.environ.get("CCB_PORT", "8765")))
    parser.add_argument("--host", default="127.0.0.1")
    args = parser.parse_args()
    print(f"[ccb] listening on http://{args.host}:{args.port}", file=sys.stderr, flush=True)
    uvicorn.run(app, host=args.host, port=args.port, log_level="warning")


if __name__ == "__main__":
    main()
