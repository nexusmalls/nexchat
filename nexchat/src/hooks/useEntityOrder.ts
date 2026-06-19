import { useCallback, useEffect, useRef, useState } from "react";
import { config } from "@/config";
import { chainClient } from "@/chain/chainClient";
import { fetchOrder } from "@/shop/entityOrderQueries";
import type { EntityOrder } from "@/shop/types";

const REFRESH_MS = 10_000;

// EN: Poll single entity order (disabled in mock mode).
// CN: 轮询单个 Entity 订单（mock 模式关闭）。
export function useEntityOrder(orderId: number | null, enabled: boolean) {
  const [order, setOrder] = useState<EntityOrder | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const timerRef = useRef<ReturnType<typeof setInterval> | null>(null);

  const refresh = useCallback(async () => {
    if (!enabled || config.useMock || orderId == null) {
      setOrder(null);
      return;
    }
    setLoading(true);
    setError(null);
    try {
      const api = (await chainClient.getApiForWallet()) as unknown as Parameters<
        typeof fetchOrder
      >[0];
      const data = await fetchOrder(api, orderId);
      setOrder(data);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setLoading(false);
    }
  }, [enabled, orderId]);

  useEffect(() => {
    if (!enabled || config.useMock || orderId == null) {
      setOrder(null);
      setError(null);
      setLoading(false);
      return;
    }

    void refresh();
    timerRef.current = setInterval(() => void refresh(), REFRESH_MS);
    return () => {
      if (timerRef.current) clearInterval(timerRef.current);
    };
  }, [enabled, orderId, refresh]);

  return { order, loading, error, refresh };
}
