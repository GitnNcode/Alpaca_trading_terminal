"""Compare-tab risk/return metrics. All annualized using 252 trading days."""

from __future__ import annotations

import math
from dataclasses import dataclass
from typing import Sequence

import numpy as np


TRADING_DAYS = 252


@dataclass
class Metrics:
    cagr: float
    ann_vol: float
    sharpe: float
    sortino: float
    max_dd: float    # negative number, e.g. -0.42
    calmar: float

    def to_dict(self):
        return {
            "cagr": self.cagr,
            "ann_vol": self.ann_vol,
            "sharpe": self.sharpe,
            "sortino": self.sortino,
            "max_dd": self.max_dd,
            "calmar": self.calmar,
        }


def log_returns(closes: np.ndarray) -> np.ndarray:
    closes = np.asarray(closes, dtype=np.float64)
    return np.log(closes[1:] / closes[:-1])


def compute(closes: np.ndarray) -> Metrics:
    closes = np.asarray(closes, dtype=np.float64)
    if len(closes) < 2:
        return Metrics(0, 0, 0, 0, 0, 0)
    rets = log_returns(closes)
    years = (len(closes) - 1) / TRADING_DAYS
    cagr = (closes[-1] / closes[0]) ** (1.0 / max(years, 1e-9)) - 1.0 if closes[0] > 0 else 0.0
    mu = rets.mean()
    sigma = rets.std(ddof=0)
    ann_vol = sigma * math.sqrt(TRADING_DAYS)
    sharpe = (mu / sigma) * math.sqrt(TRADING_DAYS) if sigma > 0 else 0.0
    downside = rets[rets < 0]
    dstd = downside.std(ddof=0) if len(downside) > 0 else 0.0
    sortino = (mu / dstd) * math.sqrt(TRADING_DAYS) if dstd > 0 else 0.0
    peaks = np.maximum.accumulate(closes)
    dd = (closes - peaks) / peaks
    max_dd = float(dd.min())
    calmar = cagr / abs(max_dd) if max_dd < 0 else 0.0
    return Metrics(cagr, ann_vol, sharpe, sortino, max_dd, calmar)


def drawdown_series(closes: np.ndarray) -> np.ndarray:
    closes = np.asarray(closes, dtype=np.float64)
    peaks = np.maximum.accumulate(closes)
    return (closes - peaks) / peaks


def normalized_series(closes: np.ndarray, base: float = 100.0) -> np.ndarray:
    closes = np.asarray(closes, dtype=np.float64)
    if len(closes) == 0 or closes[0] == 0:
        return closes
    return closes / closes[0] * base


def aligned(series_list: Sequence[np.ndarray]) -> list[np.ndarray]:
    """Right-truncate to the shortest length (newest bars kept)."""
    if not series_list:
        return []
    n = min(len(s) for s in series_list)
    return [np.asarray(s[-n:], dtype=np.float64) for s in series_list]


def correlation_matrix(returns_list: Sequence[np.ndarray]) -> list[list[float]]:
    if not returns_list:
        return []
    aligned_rets = aligned(returns_list)
    mat = np.vstack(aligned_rets)
    # Pearson; np.corrcoef handles centering + normalization
    c = np.corrcoef(mat)
    if c.ndim == 0:
        return [[1.0]]
    return c.tolist()
