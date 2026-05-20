"""Thin async Alpaca client. Bars + assets only — the order/account surface is
handled by the canonical Go terminal; this app is read-only market data."""

from __future__ import annotations

import asyncio
from dataclasses import dataclass
from typing import Any

import httpx

from config import Credentials


DATA_BASE = "https://data.alpaca.markets"
TIMEFRAMES = ["1Min", "5Min", "15Min", "30Min", "1Hour", "1Day", "1Week", "1Month"]


@dataclass
class Bar:
    t: str       # ISO timestamp (UTC)
    o: float
    h: float
    l: float
    c: float
    v: float

    def to_dict(self) -> dict[str, Any]:
        return {"t": self.t, "o": self.o, "h": self.h, "l": self.l, "c": self.c, "v": self.v}


@dataclass
class Asset:
    symbol: str
    name: str

    def to_dict(self) -> dict[str, str]:
        return {"symbol": self.symbol, "name": self.name}


def _headers(creds: Credentials) -> dict[str, str]:
    return {
        "APCA-API-KEY-ID": creds.api_key,
        "APCA-API-SECRET-KEY": creds.api_secret,
    }


async def fetch_bars(
    creds: Credentials,
    symbol: str,
    timeframe: str,
    start_iso: str,
    end_iso: str,
    feed: str = "iex",
) -> list[Bar]:
    if timeframe not in TIMEFRAMES:
        raise ValueError(f"unknown timeframe {timeframe}")
    url = f"{DATA_BASE}/v2/stocks/{symbol.upper()}/bars"
    params: dict[str, Any] = {
        "timeframe": timeframe,
        "start": start_iso,
        "end": end_iso,
        "limit": 10000,
        "adjustment": "split",
        "feed": feed,
    }
    # Perf-fix: collect each page into its own list via a C-level comprehension
    # and flatten once at the end. The previous code did one Python-level
    # `bars.append(...)` per row across up to 50,000 rows, which compounded
    # interpreter overhead with list reallocations. Chunked accumulation lets
    # the underlying CPython list resize in larger steps and removes the
    # per-row append/lookup cost.
    pages: list[list[Bar]] = []
    total = 0
    cap = 50_000
    async with httpx.AsyncClient(timeout=15.0, headers=_headers(creds)) as client:
        while True:
            resp = await client.get(url, params=params)
            if resp.status_code >= 400:
                msg = resp.text
                try:
                    msg = resp.json().get("message", msg)
                except Exception:
                    pass
                raise RuntimeError(f"alpaca bars {resp.status_code}: {msg}")
            payload = resp.json()
            raw = payload.get("bars") or []
            page = [
                Bar(t=b["t"], o=b["o"], h=b["h"], l=b["l"], c=b["c"], v=b["v"])
                for b in raw
            ]
            pages.append(page)
            total += len(page)
            token = payload.get("next_page_token")
            if not token or total >= cap:
                break
            params["page_token"] = token
    # Single allocation sized exactly to the result.
    bars: list[Bar] = []
    if pages:
        bars = pages[0] if len(pages) == 1 else [b for page in pages for b in page]
        if len(bars) > cap:
            bars = bars[:cap]
    return bars


async def fetch_assets(creds: Credentials) -> list[Asset]:
    url = f"{creds.base_url.rstrip('/')}/v2/assets"
    params = {"status": "active", "asset_class": "us_equity"}
    async with httpx.AsyncClient(timeout=30.0, headers=_headers(creds)) as client:
        resp = await client.get(url, params=params)
        if resp.status_code >= 400:
            raise RuntimeError(f"alpaca assets {resp.status_code}: {resp.text}")
        out: list[Asset] = []
        for a in resp.json():
            sym = a.get("symbol")
            name = a.get("name") or ""
            if not sym:
                continue
            out.append(Asset(symbol=sym, name=name))
        return out
