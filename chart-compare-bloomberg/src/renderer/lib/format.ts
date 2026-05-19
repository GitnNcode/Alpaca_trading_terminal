export function fmtMoney(v: number | null | undefined, dp = 2): string {
  if (v == null || !isFinite(v)) return "—";
  return v.toLocaleString(undefined, { minimumFractionDigits: dp, maximumFractionDigits: dp });
}

export function fmtVol(v: number): string {
  if (!isFinite(v)) return "—";
  const a = Math.abs(v);
  if (a >= 1e9) return (v / 1e9).toFixed(2) + "B";
  if (a >= 1e6) return (v / 1e6).toFixed(2) + "M";
  if (a >= 1e3) return (v / 1e3).toFixed(2) + "K";
  return v.toFixed(0);
}

export function fmtPct(v: number, dp = 2): string {
  if (!isFinite(v)) return "—";
  return (v * 100).toFixed(dp) + "%";
}

export function fmtSigned(v: number, dp = 2): string {
  if (!isFinite(v)) return "—";
  const sign = v >= 0 ? "+" : "";
  return sign + v.toFixed(dp);
}

export function classChange(v: number): string {
  if (v > 0) return "ccb-pos";
  if (v < 0) return "ccb-neg";
  return "";
}

export function toUnixSec(iso: string): number {
  return Math.floor(new Date(iso).getTime() / 1000);
}
