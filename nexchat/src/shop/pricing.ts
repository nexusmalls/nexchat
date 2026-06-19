// EN: NEX/USDT dynamic pricing for entity orders (mirrors nexus-com-dapp).
// CN: Entity 订单动态 NEX/USDT 定价（与 nexus-com-dapp 对齐）。

const ORDER_SLIPPAGE_BPS = 300;
const MIN_NATIVE_RESERVE = 100_000_000_000n; // 0.1 NEX

export function usdtToNexDynamic(
  usdtPrice: number | string,
  nexUsdtRate: string,
): string | null {
  const rate = BigInt(nexUsdtRate || "0");
  if (rate <= 0n) return null;
  const usdt = BigInt(usdtPrice);
  if (usdt <= 0n) return null;
  return ((usdt * 10n ** 12n) / rate).toString();
}

export function nexToUsdtDynamic(
  nexRaw: string | bigint,
  nexUsdtRate: string,
): string | null {
  const rate = BigInt(nexUsdtRate || "0");
  if (rate <= 0n) return null;
  const nex = typeof nexRaw === "bigint" ? nexRaw : BigInt(nexRaw || "0");
  if (nex <= 0n) return null;
  return ((nex * rate) / 10n ** 12n).toString();
}

export function applySlippageBps(rawAmount: string | bigint, bps: number): string {
  const amount = typeof rawAmount === "bigint" ? rawAmount : BigInt(rawAmount || "0");
  if (amount <= 0n || bps <= 0) return amount.toString();
  return ((amount * BigInt(10_000 + bps) + 9999n) / 10_000n).toString();
}

export function formatNexBalance(raw: string | bigint, displayDecimals = 4): string {
  const n = typeof raw === "bigint" ? raw : BigInt(raw || "0");
  const divisor = 10n ** 12n;
  const whole = n / divisor;
  const frac = n % divisor;
  const fracStr = frac.toString().padStart(12, "0").slice(0, displayDecimals);
  if (displayDecimals === 0) return `${whole}`;
  return `${whole}.${fracStr}`;
}

export interface OrderQuoteInput {
  product: {
    price: string;
    usdtPrice: number;
  };
  quantity: number;
  marketRate: string | null;
  paymentAsset: "Native" | "ShoppingBalance";
  shoppingBalanceRaw?: string | null;
}

export interface OrderQuote {
  hasUsdtPrice: boolean;
  unitNexDynamic: string | null;
  totalNex: bigint;
  totalUsdt: number | null;
  maxNexAmount: string | null;
  shoppingBalSpend: string | null;
  priceReady: boolean;
}

export function computeOrderQuote(input: OrderQuoteInput): OrderQuote {
  const hasUsdtPrice = input.product.usdtPrice > 0;
  const unitNexDynamic =
    hasUsdtPrice && input.marketRate
      ? usdtToNexDynamic(input.product.usdtPrice, input.marketRate)
      : null;

  const totalNex = unitNexDynamic
    ? BigInt(unitNexDynamic) * BigInt(input.quantity)
    : BigInt(input.product.price || "0") * BigInt(input.quantity);

  const totalUsdt = hasUsdtPrice ? input.product.usdtPrice * input.quantity : null;
  const priceReady = !hasUsdtPrice || unitNexDynamic != null;
  const maxNexAmount =
    hasUsdtPrice && priceReady
      ? applySlippageBps(totalNex.toString(), ORDER_SLIPPAGE_BPS)
      : null;

  let shoppingBalSpend: string | null = null;
  if (
    input.paymentAsset === "ShoppingBalance" &&
    input.shoppingBalanceRaw &&
    priceReady
  ) {
    const bal = BigInt(input.shoppingBalanceRaw);
    if (bal > 0n && totalNex > MIN_NATIVE_RESERVE) {
      const maxDeduct = totalNex - MIN_NATIVE_RESERVE;
      const cap = maxDeduct < bal ? maxDeduct : bal;
      if (cap > 0n) shoppingBalSpend = cap.toString();
    }
  }

  return {
    hasUsdtPrice,
    unitNexDynamic,
    totalNex,
    totalUsdt,
    maxNexAmount,
    shoppingBalSpend,
    priceReady,
  };
}
