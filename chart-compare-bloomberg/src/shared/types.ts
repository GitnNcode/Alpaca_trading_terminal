export interface Bar {
  t: string; // ISO UTC
  o: number;
  h: number;
  l: number;
  c: number;
  v: number;
}

export interface Asset {
  symbol: string;
  name: string;
}

export type IndicatorKey = "ema" | "sma" | "bb" | "vwap" | "rsi" | "macd";

export interface BollingerPayload {
  upper: (number | null)[];
  mid: (number | null)[];
  lower: (number | null)[];
}

export interface MACDPayload {
  line: (number | null)[];
  signal: (number | null)[];
  hist: (number | null)[];
}

export interface IndicatorPayload {
  ema?: (number | null)[];
  sma?: (number | null)[];
  bb?: BollingerPayload;
  vwap?: (number | null)[];
  rsi?: (number | null)[];
  macd?: MACDPayload;
}

export interface BarsResponse {
  symbol: string;
  timeframe: string;
  bars: Bar[];
  indicators: IndicatorPayload;
}

export interface Metrics {
  cagr: number;
  ann_vol: number;
  sharpe: number;
  sortino: number;
  max_dd: number;
  calmar: number;
}

export interface CompareSeriesOk {
  symbol: string;
  bars: Bar[];
  metrics: Metrics;
  normalized: number[];
  drawdown: number[];
  times: string[];
}

export interface CompareSeriesErr {
  symbol: string;
  error: string;
}

export type CompareSeries = CompareSeriesOk | CompareSeriesErr;

export interface CompareResponse {
  range: string;
  series: CompareSeries[];
  matrix: number[][];
  labels: string[];
}

export interface MCResponse {
  horizon_years: number;
  n_sims: number;
  days: number;
  mu_daily: number;
  sigma_daily: number;
  p05: number[];
  p50: number[];
  p95: number[];
  final_p05: number;
  final_p50: number;
  final_p95: number;
  prob_above_start: number;
  prob_50_dd: number;
}

export interface HealthResponse {
  ok: boolean;
  version: string;
  has_credentials: boolean;
  ts: string;
}

// 1D, 1W, 1M, YTD, 1Y, 5Y, MAX
export type ChartRange = "1D" | "1W" | "1M" | "YTD" | "1Y" | "5Y" | "MAX";
export type Timeframe = "1Min" | "5Min" | "15Min" | "30Min" | "1Hour" | "1Day" | "1Week" | "1Month";
export type CompareRange = "1Y" | "3Y" | "5Y" | "10Y";

export interface CcbBridge {
  sidecarPort: () => Promise<number>;
  platform: string;
}

declare global {
  interface Window {
    ccb: CcbBridge;
  }
}
