export const NEX_PLANCK = 1_000_000_000_000n;

export function nex(amount: number): bigint {
  return BigInt(Math.round(amount * 1_000_000_000_000));
}

export function formatNex(raw: bigint): string {
  return `${(Number(raw) / 1e12).toLocaleString()} NEX`;
}

export function asBigInt(value: unknown): bigint {
  if (typeof value === 'bigint') return value;
  if (typeof value === 'number') return BigInt(value);
  if (typeof value === 'string') {
    const cleaned = value.replace(/,/g, '').trim();
    return cleaned ? BigInt(cleaned) : 0n;
  }
  if (value != null && typeof (value as any).toString === 'function') {
    try { return BigInt((value as any).toString()); } catch { return 0n; }
  }
  return 0n;
}
