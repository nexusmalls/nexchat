// EN: Pre-flight limit-order price band (mirrors chain `check_price_deviation` bps math).
// CN: 限价单价格区间预检（复刻链上 `check_price_deviation` 基点算法）。

import type { NexPriceProtection } from "@/market/types";

export interface PriceBand {
  referencePrice: bigint | null;
  minPrice: bigint | null;
  maxPrice: bigint | null;
  maxDeviationBps: number;
}

export function computePriceBand(
  protection: NexPriceProtection,
  referencePriceRaw: string | null,
): PriceBand {
  const maxDeviationBps = protection.maxPriceDeviation;
  if (!protection.enabled || !referencePriceRaw || referencePriceRaw === "0") {
    return {
      referencePrice: null,
      minPrice: null,
      maxPrice: null,
      maxDeviationBps,
    };
  }

  const referencePrice = BigInt(referencePriceRaw);
  const k = BigInt(maxDeviationBps);
  const slack = (referencePrice * k) / 10000n;
  const minPrice = referencePrice > slack ? referencePrice - slack : 1n;
  const maxPrice = referencePrice + slack;

  return { referencePrice, minPrice, maxPrice, maxDeviationBps };
}

export function validateLimitPrice(
  priceRaw: bigint,
  protection: NexPriceProtection,
  band: PriceBand,
): { ok: true } | { ok: false; reason: "circuit_breaker" | "too_low" | "too_high" | "invalid" } {
  if (!protection.enabled) return { ok: true };
  if (protection.circuitBreakerActive) return { ok: false, reason: "circuit_breaker" };
  if (priceRaw <= 0n) return { ok: false, reason: "invalid" };
  if (band.minPrice == null || band.maxPrice == null) return { ok: true };
  if (priceRaw < band.minPrice) return { ok: false, reason: "too_low" };
  if (priceRaw > band.maxPrice) return { ok: false, reason: "too_high" };
  return { ok: true };
}
