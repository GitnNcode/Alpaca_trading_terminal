import type {
  Asset,
  BarsResponse,
  ChartRange,
  CompareRange,
  CompareResponse,
  HealthResponse,
  IndicatorKey,
  MCResponse,
  Timeframe,
} from "@shared/types";

let cachedPort: number | null = null;

// Browser fallback: when the Electron bridge is missing (i.e. you opened
// http://localhost:5173 directly), talk to the sidecar's dev port instead.
// `npm run dev` boots the Python server on 8765 alongside Vite.
const BROWSER_FALLBACK_PORT = 8765;

export function bridgeAvailable(): boolean {
  return typeof window !== "undefined" && typeof window.ccb?.sidecarPort === "function";
}

async function port(): Promise<number> {
  if (cachedPort) return cachedPort;
  if (bridgeAvailable()) {
    cachedPort = await window.ccb.sidecarPort();
  } else {
    cachedPort = BROWSER_FALLBACK_PORT;
  }
  return cachedPort;
}

async function base(): Promise<string> {
  return `http://127.0.0.1:${await port()}`;
}

async function get<T>(path: string): Promise<T> {
  const r = await fetch((await base()) + path);
  if (!r.ok) throw new Error(`${path}: ${r.status} ${await r.text()}`);
  return r.json() as Promise<T>;
}

async function post<T>(path: string, body: unknown): Promise<T> {
  const r = await fetch((await base()) + path, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(body),
  });
  if (!r.ok) {
    let msg = `${r.status}`;
    try { msg = (await r.json()).detail ?? msg; } catch { /* noop */ }
    throw new Error(`${path}: ${msg}`);
  }
  return r.json() as Promise<T>;
}

export const api = {
  health: () => get<HealthResponse>("/health"),
  setCredentials: (api_key: string, api_secret: string, base_url = "https://paper-api.alpaca.markets") =>
    post<{ ok: boolean }>("/credentials", { api_key, api_secret, base_url }),
  clearCredentials: () => post<{ ok: boolean }>("/credentials/clear", {}),
  assets: () => get<Asset[]>("/assets"),
  bars: (symbol: string, timeframe: Timeframe, start: string, end: string, indicators: IndicatorKey[]) =>
    post<BarsResponse>("/bars", { symbol, timeframe, start, end, indicators }),
  compare: (symbols: string[], range: CompareRange) =>
    post<CompareResponse>("/compare", { symbols, range }),
  montecarlo: (symbol: string, range: CompareRange, horizon_years: number, n_sims: number, seed: number) =>
    post<MCResponse>("/montecarlo", { symbol, range, horizon_years, n_sims, seed }),
};

// Convert a Bloomberg-style range key into ISO start/end strings for the bars endpoint.
export function rangeWindow(range: ChartRange): { start: string; end: string } {
  const end = new Date(Date.now() - 20 * 60 * 1000); // back off 20m for IEX feed lag
  const start = new Date(end);
  switch (range) {
    case "1D": start.setDate(start.getDate() - 2); break;
    case "1W": start.setDate(start.getDate() - 10); break;
    case "1M": start.setDate(start.getDate() - 45); break;
    case "YTD": start.setMonth(0); start.setDate(1); break;
    case "1Y": start.setDate(start.getDate() - 400); break;
    case "5Y": start.setDate(start.getDate() - 365 * 5 - 30); break;
    case "MAX": start.setDate(start.getDate() - 365 * 25); break;
  }
  return { start: start.toISOString(), end: end.toISOString() };
}

export function defaultTimeframe(range: ChartRange): Timeframe {
  switch (range) {
    case "1D": return "5Min";
    case "1W": return "30Min";
    case "1M": return "1Hour";
    case "YTD": return "1Day";
    case "1Y": return "1Day";
    case "5Y": return "1Week";
    case "MAX": return "1Month";
  }
}
