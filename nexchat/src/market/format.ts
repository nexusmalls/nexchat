// EN: NEX market display formatting (mirrors nexus-com-dapp chain-helpers).
// CN: NEX 市场展示格式化（与 nexus-com-dapp chain-helpers 对齐）。

export function formatNexPrice(raw: number | string): string {
  const n = typeof raw === "number" ? raw : Number(raw);
  if (isNaN(n) || n === 0) return "0";
  const value = n / 1e6;
  if (value >= 1) return value.toFixed(2);
  if (value >= 0.01) return value.toFixed(4);
  const s = value.toFixed(6);
  return s.replace(/0+$/, "").replace(/\.$/, "");
}

export function formatBalance(raw: string | bigint, decimals = 12, displayDecimals = 2): string {
  const n = typeof raw === "bigint" ? raw : BigInt(raw || "0");
  const divisor = 10n ** BigInt(decimals);
  const whole = n / divisor;
  const frac = n % divisor;
  const fracStr = frac.toString().padStart(decimals, "0").slice(0, displayDecimals);
  if (displayDecimals === 0) return `${whole}`;
  return `${whole}.${fracStr}`;
}

export function formatUsdt(raw: string | bigint, displayDecimals = 2): string {
  const bi = typeof raw === "bigint" ? raw : BigInt(raw || "0");
  const divisor = 1_000_000n;
  const whole = bi / divisor;
  const frac = bi % divisor;
  const fracStr = frac.toString().padStart(6, "0").slice(0, displayDecimals);
  return `${whole}.${fracStr}`;
}
