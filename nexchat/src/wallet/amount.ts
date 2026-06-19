// EN: Parse human NEX amount to planck (12 decimals).
// CN: 将可读 NEX 金额解析为 planck（12 位小数）。

const DECIMALS = 12n;
const DIVISOR = 10n ** DECIMALS;

export function parseNexAmount(input: string): bigint | null {
  const trimmed = input.trim();
  if (!trimmed) return null;
  if (!/^\d+(\.\d+)?$/.test(trimmed)) return null;

  const [wholePart, fracPart = ""] = trimmed.split(".");
  const whole = BigInt(wholePart || "0");
  const fracPadded = (fracPart + "0".repeat(Number(DECIMALS))).slice(0, Number(DECIMALS));
  const frac = BigInt(fracPadded || "0");
  if (frac >= DIVISOR) return null;
  return whole * DIVISOR + frac;
}
