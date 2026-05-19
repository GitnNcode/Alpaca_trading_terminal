"""Geometric Brownian Monte Carlo with the same Xorshift64 + Box-Muller path
generator as the Rust port. Numpy-vectorized for speed."""

from __future__ import annotations

import math
from dataclasses import dataclass

import numpy as np


@dataclass
class MCResult:
    horizon_years: float
    n_sims: int
    days: int
    mu_daily: float
    sigma_daily: float
    p05: list[float]
    p50: list[float]
    p95: list[float]
    final_p05: float
    final_p50: float
    final_p95: float
    prob_above_start: float
    prob_50_dd: float

    def to_dict(self):
        return self.__dict__


def _xorshift64_floats(seed: int, n: int) -> np.ndarray:
    """Generate `n` U(0,1) samples using xorshift64 — same algorithm as Rust impl.
    Vectorized using numpy's int64 arithmetic isn't safe (overflow), so we use a
    python loop but only need to do it once; the bulk of work is numpy."""
    s = seed & 0xFFFFFFFFFFFFFFFF
    if s == 0:
        s = 0xDEADBEEFCAFEBABE
    out = np.empty(n, dtype=np.float64)
    for i in range(n):
        s ^= (s << 13) & 0xFFFFFFFFFFFFFFFF
        s ^= (s >> 7) & 0xFFFFFFFFFFFFFFFF
        s ^= (s << 17) & 0xFFFFFFFFFFFFFFFF
        out[i] = ((s >> 11) & ((1 << 53) - 1)) / float(1 << 53)
        if out[i] == 0.0:
            out[i] = 1e-15
    return out


def _box_muller(uniforms: np.ndarray) -> np.ndarray:
    pairs = uniforms.reshape(-1, 2)
    u1 = pairs[:, 0]
    u2 = pairs[:, 1]
    z = np.sqrt(-2.0 * np.log(u1)) * np.cos(2.0 * math.pi * u2)
    return z


def run(closes: np.ndarray, horizon_years: float, n_sims: int, seed: int) -> MCResult:
    closes = np.asarray(closes, dtype=np.float64)
    if len(closes) < 2:
        raise ValueError("need at least 2 closes to fit μ/σ")
    n_sims = int(max(100, min(10000, n_sims)))
    days = int(round(horizon_years * 252))
    days = max(days, 1)
    rets = np.log(closes[1:] / closes[:-1])
    mu = float(rets.mean())
    sigma = float(rets.std(ddof=0))

    # Box-Muller consumes 2 uniforms per normal (we use only the cos branch,
    # matching the Rust port). So uniforms_needed = 2 * total_normals.
    total_normals = n_sims * days
    uniforms_needed = total_normals * 2
    u = _xorshift64_floats(seed, uniforms_needed)
    z = _box_muller(u).reshape(n_sims, days)

    # cumulative log-returns; multiplicative growth path starts at 1.0
    increments = mu + sigma * z
    log_paths = np.cumsum(increments, axis=1)
    paths = np.exp(log_paths)  # shape (n_sims, days), starting > 1 with drift
    # Pre-pend start=1.0 column
    paths = np.hstack([np.ones((n_sims, 1)), paths])

    # Per-day percentiles across sims
    p05 = np.percentile(paths, 5, axis=0)
    p50 = np.percentile(paths, 50, axis=0)
    p95 = np.percentile(paths, 95, axis=0)

    final = paths[:, -1]
    prob_above_start = float((final > 1.0).mean())
    peaks = np.maximum.accumulate(paths, axis=1)
    dd = (paths - peaks) / peaks
    prob_50_dd = float((dd.min(axis=1) <= -0.5).mean())

    return MCResult(
        horizon_years=horizon_years,
        n_sims=n_sims,
        days=days,
        mu_daily=mu,
        sigma_daily=sigma,
        p05=p05.tolist(),
        p50=p50.tolist(),
        p95=p95.tolist(),
        final_p05=float(np.percentile(final, 5)),
        final_p50=float(np.percentile(final, 50)),
        final_p95=float(np.percentile(final, 95)),
        prob_above_start=prob_above_start,
        prob_50_dd=prob_50_dd,
    )
