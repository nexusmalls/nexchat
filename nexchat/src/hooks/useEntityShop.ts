import { useCallback, useEffect, useRef, useState } from "react";
import { config } from "@/config";
import { chainClient } from "@/chain/chainClient";
import { fetchShopCatalog } from "@/shop/entityQueries";
import type { ShopCatalog } from "@/shop/types";

const REFRESH_MS = 30_000;

// EN: Poll entity shop catalog from chain (disabled in mock mode).
// CN: 从链上轮询 Entity 购物目录（mock 模式关闭）。
export function useEntityShop(enabled: boolean, address?: string | null) {
  const [catalog, setCatalog] = useState<ShopCatalog | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const timerRef = useRef<ReturnType<typeof setInterval> | null>(null);

  const refresh = useCallback(async () => {
    if (!enabled || config.useMock) return;
    setLoading(true);
    setError(null);
    try {
      const api = (await chainClient.getApiForWallet()) as unknown as Parameters<
        typeof fetchShopCatalog
      >[0];
      const data = await fetchShopCatalog(api, { viewerAddress: address ?? null });
      setCatalog(data);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setLoading(false);
    }
  }, [enabled, address]);

  useEffect(() => {
    if (!enabled || config.useMock) {
      setCatalog(null);
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

  return { catalog, loading, error, refresh };
}
