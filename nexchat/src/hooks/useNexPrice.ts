import { useCallback, useEffect, useMemo, useState } from "react";
import { config } from "@/config";
import { chainClient } from "@/chain/chainClient";
import { fetchMarketSnapshot } from "@/market/nexMarketQueries";
import { nexToUsdtDynamic, usdtToNexDynamic } from "@/shop/pricing";

// EN: NEX/USDT market rate for entity order pricing.
// CN: Entity 订单定价用的 NEX/USDT 行情。
export function useNexPrice(enabled: boolean) {
  const [marketRate, setMarketRate] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);

  const refresh = useCallback(async () => {
    if (!enabled || config.useMock) return;
    setLoading(true);
    try {
      const api = (await chainClient.getApiForWallet()) as unknown as Parameters<
        typeof fetchMarketSnapshot
      >[0];
      const snap = await fetchMarketSnapshot(api);
      const last = snap.stats.lastPrice;
      const ref = snap.stats.referencePrice;
      const rate =
        last && last !== "0" ? last : ref && ref !== "0" ? ref : null;
      setMarketRate(rate);
    } catch {
      setMarketRate(null);
    } finally {
      setLoading(false);
    }
  }, [enabled]);

  useEffect(() => {
    if (!enabled || config.useMock) {
      setMarketRate(null);
      return;
    }
    void refresh();
  }, [enabled, refresh]);

  const toNex = useMemo(() => {
    if (!marketRate) return (_usdt: number | string) => null as string | null;
    return (usdtPrice: number | string) => usdtToNexDynamic(usdtPrice, marketRate);
  }, [marketRate]);

  const toUsdt = useMemo(() => {
    if (!marketRate) return (_nex: string | bigint) => null as string | null;
    return (nexRaw: string | bigint) => nexToUsdtDynamic(nexRaw, marketRate);
  }, [marketRate]);

  return { marketRate, toNex, toUsdt, loading, refresh };
}
