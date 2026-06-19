// EN: Shop catalog display formatting.
// CN: 购物目录展示格式化。

/** EN: Runtime `MinInitialFundUsdt` (micro-USDT, 10^6). CN: Runtime 最低初始运营资金（micro-USDT）。 */
export const MIN_INITIAL_FUND_USDT = 5_000_000;

export function formatUsdtPrice(microUsdt: number, displayDecimals = 2): string {
  const whole = Math.floor(microUsdt / 1_000_000);
  const frac = microUsdt % 1_000_000;
  const fracStr = frac.toString().padStart(6, "0").slice(0, displayDecimals);
  return `${whole}.${fracStr}`;
}

export function formatSoldCount(n: number): string {
  if (n >= 10_000) return `${(n / 10_000).toFixed(1)}万`;
  return String(n);
}

// EN: Parse user USDT input to micro-USDT (10^6 precision).
// CN: 将用户输入的 USDT 解析为 micro-USDT（10^6 精度）。
export function parseUsdtInput(input: string): number | null {
  const trimmed = input.trim();
  if (!trimmed) return null;
  const parts = trimmed.split(".");
  if (parts.length > 2) return null;
  const whole = parts[0] ?? "0";
  if (!/^\d+$/.test(whole)) return null;
  let frac = parts[1] ?? "";
  if (frac && !/^\d+$/.test(frac)) return null;
  frac = frac.padEnd(6, "0").slice(0, 6);
  const micro = Number(whole) * 1_000_000 + Number(frac || "0");
  if (!Number.isFinite(micro) || micro <= 0) return null;
  return micro;
}
