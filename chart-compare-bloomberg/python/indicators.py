"""Vectorized indicator math. NaN-padded to match input length; the renderer
drops NaNs before plotting (lightweight-charts requires that)."""

from __future__ import annotations

import numpy as np


def sma(close: np.ndarray, period: int) -> np.ndarray:
    out = np.full_like(close, np.nan, dtype=np.float64)
    if period <= 0 or len(close) < period:
        return out
    cumsum = np.cumsum(np.insert(close, 0, 0.0))
    out[period - 1:] = (cumsum[period:] - cumsum[:-period]) / period
    return out


def ema(close: np.ndarray, period: int) -> np.ndarray:
    out = np.full_like(close, np.nan, dtype=np.float64)
    n = len(close)
    if period <= 0 or n < period:
        return out
    alpha = 2.0 / (period + 1.0)
    seed = close[:period].mean()
    out[period - 1] = seed
    prev = seed
    for i in range(period, n):
        prev = close[i] * alpha + prev * (1.0 - alpha)
        out[i] = prev
    return out


def bollinger(close: np.ndarray, period: int = 20, mult: float = 2.0):
    mid = sma(close, period)
    n = len(close)
    upper = np.full(n, np.nan)
    lower = np.full(n, np.nan)
    if n < period:
        return upper, mid, lower
    # rolling std (population, matching the Rust impl)
    rolling = np.lib.stride_tricks.sliding_window_view(close, period)
    std = rolling.std(axis=1, ddof=0)
    upper[period - 1:] = mid[period - 1:] + mult * std
    lower[period - 1:] = mid[period - 1:] - mult * std
    return upper, mid, lower


def vwap(highs: np.ndarray, lows: np.ndarray, closes: np.ndarray, vols: np.ndarray) -> np.ndarray:
    tp = (highs + lows + closes) / 3.0
    pv = tp * vols
    cum_pv = np.cumsum(pv)
    cum_v = np.cumsum(vols)
    out = np.where(cum_v > 0, cum_pv / np.maximum(cum_v, 1e-12), np.nan)
    return out


def rsi(close: np.ndarray, period: int = 14) -> np.ndarray:
    n = len(close)
    out = np.full(n, np.nan)
    if n <= period:
        return out
    delta = np.diff(close)
    gains = np.where(delta > 0, delta, 0.0)
    losses = np.where(delta < 0, -delta, 0.0)
    avg_gain = gains[:period].mean()
    avg_loss = losses[:period].mean()
    if avg_loss == 0:
        out[period] = 100.0
    else:
        rs = avg_gain / avg_loss
        out[period] = 100.0 - 100.0 / (1.0 + rs)
    for i in range(period + 1, n):
        g = gains[i - 1]
        l = losses[i - 1]
        avg_gain = (avg_gain * (period - 1) + g) / period
        avg_loss = (avg_loss * (period - 1) + l) / period
        if avg_loss == 0:
            out[i] = 100.0
        else:
            rs = avg_gain / avg_loss
            out[i] = 100.0 - 100.0 / (1.0 + rs)
    return out


def macd(close: np.ndarray, fast: int = 12, slow: int = 26, signal: int = 9):
    ema_fast = ema(close, fast)
    ema_slow = ema(close, slow)
    macd_line = ema_fast - ema_slow
    # Signal EMA over the macd line (NaN-aware: only seed once macd is dense)
    n = len(close)
    signal_line = np.full(n, np.nan)
    start = slow - 1
    if start + signal <= n:
        seed = macd_line[start:start + signal].mean()
        idx = start + signal - 1
        signal_line[idx] = seed
        alpha = 2.0 / (signal + 1.0)
        prev = seed
        for i in range(idx + 1, n):
            prev = macd_line[i] * alpha + prev * (1.0 - alpha)
            signal_line[i] = prev
    hist = macd_line - signal_line
    return macd_line, signal_line, hist


def atr(highs: np.ndarray, lows: np.ndarray, closes: np.ndarray, period: int = 14) -> np.ndarray:
    n = len(closes)
    out = np.full(n, np.nan)
    if n < 2:
        return out
    tr = np.maximum.reduce([
        highs[1:] - lows[1:],
        np.abs(highs[1:] - closes[:-1]),
        np.abs(lows[1:] - closes[:-1]),
    ])
    if len(tr) < period:
        return out
    seed = tr[:period].mean()
    out[period] = seed
    prev = seed
    for i in range(period + 1, n):
        prev = (prev * (period - 1) + tr[i - 1]) / period
        out[i] = prev
    return out


def nan_to_none(arr: np.ndarray) -> list[float | None]:
    return [None if np.isnan(x) else float(x) for x in arr]
