import { useCallback, useEffect, useRef, useState } from "react";
import { config } from "@/config";
import { chainClient } from "@/chain/chainClient";
import { fetchBuyerOrders } from "@/shop/entityOrderQueries";
import type { EntityOrder } from "@/shop/types";
import { canonicalAddress } from "@/wallet/address";

const REFRESH_MS = 15_000;

// EN: Poll buyer orders from chain (disabled in mock mode).
// CN: 从链上轮询买家订单（mock 模式关闭）。
export function useBuyerOrders(address: string | null, enabled: boolean) {
  const [orders, setOrders] = useState<EntityOrder[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const timerRef = useRef<ReturnType<typeof setInterval> | null>(null);

  const refresh = useCallback(async () => {
    if (!enabled || config.useMock || !address) {
      setOrders([]);
      return;
    }
    setLoading(true);
    setError(null);
    try {
      const api = (await chainClient.getApiForWallet()) as unknown as Parameters<
        typeof fetchBuyerOrders
      >[0];
      const buyer = canonicalAddress(address);
      const data = await fetchBuyerOrders(api, buyer);
      setOrders(data);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setLoading(false);
    }
  }, [enabled, address]);

  useEffect(() => {
    if (!enabled || config.useMock || !address) {
      setOrders([]);
      setError(null);
      setLoading(false);
      return;
    }

    void refresh();
    timerRef.current = setInterval(() => void refresh(), REFRESH_MS);
    return () => {
      if (timerRef.current) clearInterval(timerRef.current);
    };
  }, [enabled, address, refresh]);

  return { orders, loading, error, refresh };
}
