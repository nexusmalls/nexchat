import { useCallback, useEffect, useRef, useState } from "react";
import { config } from "@/config";
import { chainClient } from "@/chain/chainClient";
import { fetchSellerOrders } from "@/shop/entityOrderQueries";
import { getManagedShopIds } from "@/shop/seller";
import type { EntityOrder, ShopCatalog } from "@/shop/types";

const REFRESH_MS = 15_000;

// EN: Poll seller orders for shops managed by `address`.
// CN: 轮询 `address` 所管理商铺的卖家订单。
export function useSellerOrders(
  address: string | null,
  catalog: ShopCatalog | null,
  enabled: boolean,
  ownedEntityIds: number[] = [],
) {
  const [orders, setOrders] = useState<EntityOrder[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const timerRef = useRef<ReturnType<typeof setInterval> | null>(null);

  const shopIds = getManagedShopIds(catalog, address, ownedEntityIds);
  const shopIdsKey = shopIds.join(",");

  const refresh = useCallback(async () => {
    if (!enabled || config.useMock || !address || shopIds.length === 0) {
      setOrders([]);
      return;
    }
    setLoading(true);
    setError(null);
    try {
      const api = (await chainClient.getApiForWallet()) as unknown as Parameters<
        typeof fetchSellerOrders
      >[0];
      const data = await fetchSellerOrders(api, shopIds);
      setOrders(data);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setLoading(false);
    }
  }, [enabled, address, shopIdsKey]);

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

  return { orders, shopIds, loading, error, refresh };
}
