import { useCallback, useEffect, useRef, useState } from "react";
import { config } from "@/config";
import { chainClient } from "@/chain/chainClient";
import { fetchMarketSnapshot, fetchUserOrders, fetchUserTrades } from "@/market/nexMarketQueries";
import type { MarketSnapshot, NexMarketOrder, NexMarketTrade } from "@/market/types";

const REFRESH_MS = 12_000;

// EN: Poll NEX global market from chain (disabled in mock mode).
// CN: 从链上轮询 NEX 全局市场（mock 模式关闭）。
export function useNexMarket(enabled: boolean, userAddress?: string | null) {
  const [data, setData] = useState<MarketSnapshot | null>(null);
  const [userOrders, setUserOrders] = useState<NexMarketOrder[]>([]);
  const [userTrades, setUserTrades] = useState<NexMarketTrade[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const timerRef = useRef<ReturnType<typeof setInterval> | null>(null);

  const refresh = useCallback(async () => {
    if (!enabled || config.useMock) return;
    setLoading(true);
    setError(null);
    try {
      const api = (await chainClient.getApiForWallet()) as unknown as Parameters<
        typeof fetchMarketSnapshot
      >[0];
      const [snap, mine, trades] = await Promise.all([
        fetchMarketSnapshot(api),
        userAddress ? fetchUserOrders(api, userAddress) : Promise.resolve([]),
        userAddress ? fetchUserTrades(api, userAddress) : Promise.resolve([]),
      ]);
      setData(snap);
      setUserOrders(mine);
      setUserTrades(trades);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setLoading(false);
    }
  }, [enabled, userAddress]);

  useEffect(() => {
    if (!enabled || config.useMock) {
      setData(null);
      setUserOrders([]);
      setUserTrades([]);
      setError(null);
      setLoading(false);
      return;
    }

    void refresh();
    timerRef.current = setInterval(() => void refresh(), REFRESH_MS);
    return () => {
      if (timerRef.current) clearInterval(timerRef.current);
    };
  }, [enabled, refresh]);

  return { data, userOrders, userTrades, loading, error, refresh };
}
