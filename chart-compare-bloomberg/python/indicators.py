"""Vectorized indicator math. NaN-padded to match input length; the renderer
drops NaNs before plotting (lightweight-charts requires that)."""

from __future__ import annotations

import numpy as np


# Perf-fix: shared vectorized seeded EW-smoother used by ema/rsi/atr.
# Reason: the previous Python `for` loops dominated indicator latency on 50k-bar
# datasets. The recursion y[i] = a*x[i] + (1-a)*y[i-1] (with y[-1] = seed)
# is solved in closed form via the scaled-cumsum identity:
#     y[i] = β^(i+1)·seed + α·β^i·Σ_{k=0..i} (x[k] / β^k)
# We chunk the cumsum so β^(-k) stays well within float64 range, then feed the
# last value of each chunk as the seed for the next — fully numpy-vectorized
# inside each chunk while remaining numerically stable.
def _ewma_seeded(x: np.ndarray, alpha: float, seed: float) -> np.ndarray:
    n = len(x)
    if n == 0:
        return np.empty(0, dtype=np.float64)
    beta = 1.0 - alpha
    if beta <= 0.0:
        return x.astype(np.float64, copy=True)
    out = np.empty(n, dtype=np.float64)
    log_inv_beta = -np.log(beta)
    # cap so β^(-(chunk-1)) ≲ e^600 — comfortably below float64 max ≈ 1.8e308
    chunk_size = max(1, int(600.0 / log_inv_beta)) if log_inv_beta > 0 else n
    chunk_size = min(chunk_size, n)
    prev = float(seed)
    for start in range(0, n, chunk_size):
        end = min(start + chunk_size, n)
        m = end - start
        idx = np.arange(m, dtype=np.float64)
        powers = beta ** idx                # β^0 .. β^(m-1)
        chunk = x[start:end].astype(np.float64, copy=False)
        cs = np.cumsum(chunk / powers)      # Σ x[k]·β^(-k)
        out[start:end] = (beta * prev) * powers + alpha * powers * cs
        prev = float(out[end - 1])
    return out


def sma(close: np.ndarray, period: int) -> np.ndarray:
    out = np.full_like(close, np.nan, dtype=np.float64)
    if period <= 0 or len(close) < period:
        return out
    cumsum = np.cumsum(np.insert(close, 0, 0.0))
    out[period - 1:] = (cumsum[period:] - cumsum[:-period]) / period
    return out


def ema(close: np.ndarray, period: int) -> np.ndarray:
    # Perf-fix: replaced the per-bar Python `for` loop with the vectorized
    # _ewma_seeded helper. Seeds the first dense slot with the SMA of the
    # opening window (unchanged behavior) and then evaluates the rest in numpy.
    out = np.full_like(close, np.nan, dtype=np.float64)
    n = len(close)
    if period <= 0 or n < period:
        return out
    alpha = 2.0 / (period + 1.0)
    seed = float(close[:period].mean())
    out[period - 1] = seed
    if period < n:
        out[period:n] = _ewma_seeded(close[period:n], alpha, seed)
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
    # Perf-fix: the per-bar smoothing loop is replaced with two calls to
    # _ewma_seeded (one for gains, one for losses). Wilder's smoothing reduces
    # to the same recurrence with α = 1/period, so this is a numerical no-op
    # but moves the hot path into numpy.
    n = len(close)
    out = np.full(n, np.nan)
    if n <= period:
        return out
    delta = np.diff(close)
    gains = np.where(delta > 0, delta, 0.0)
    losses = np.where(delta < 0, -delta, 0.0)
    gain_seed = float(gains[:period].mean())
    loss_seed = float(losses[:period].mean())
    # Seed RSI value at output index `period`
    if loss_seed == 0.0:
        out[period] = 100.0
    else:
        rs = gain_seed / loss_seed
        out[period] = 100.0 - 100.0 / (1.0 + rs)
    if period + 1 < n:
        alpha = 1.0 / period
        avg_gain = _ewma_seeded(gains[period:n - 1], alpha, gain_seed)
        avg_loss = _ewma_seeded(losses[period:n - 1], alpha, loss_seed)
        with np.errstate(divide="ignore", invalid="ignore"):
            rs_arr = avg_gain / np.where(avg_loss == 0.0, np.nan, avg_loss)
            rsi_arr = 100.0 - 100.0 / (1.0 + rs_arr)
        rsi_arr = np.where(avg_loss == 0.0, 100.0, rsi_arr)
        out[period + 1:n] = rsi_arr
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
    # Perf-fix: the smoothing step is delegated to _ewma_seeded with
    # α = 1/period, matching Wilder's TR smoothing exactly while removing the
    # Python-level per-bar loop.
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
    seed = float(tr[:period].mean())
    out[period] = seed
    if period + 1 < n:
        out[period + 1:n] = _ewma_seeded(tr[period:n - 1], 1.0 / period, seed)
    return out


def nan_to_none(arr: np.ndarray) -> list[float | None]:
    # Perf-fix: the previous list comprehension paid Python-level overhead for
    # every element (per-item np.isnan + float()). We now mask in C-level numpy
    # and convert once via .tolist(), which keeps allocation linear but avoids
    # the per-element interpreter dispatch on large arrays.
    if arr.size == 0:
        return []
    mask = np.isnan(arr)
    out = arr.astype(object)
    out[mask] = None
    return out.tolist()
