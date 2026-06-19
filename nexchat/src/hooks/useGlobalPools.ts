import { useCallback, useEffect, useRef, useState } from "react";
import { config } from "@/config";
import { chainClient } from "@/chain/chainClient";
import { fetchGlobalPools, type GlobalPoolsData } from "@/chain/chainQueries";

const REFRESH_MS = 60_000;

// EN: Poll global modl fund pool balances.
// CN: 轮询全局 modl 资金池余额。
export function useGlobalPools(enabled: boolean) {
  const [pools, setPools] = useState<GlobalPoolsData | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const timerRef = useRef<ReturnType<typeof setInterval> | null>(null);

  const refresh = useCallback(async () => {
    if (!enabled || config.useMock) {
      setPools(null);
      setError(null);
      return;
    }
    setLoading(true);
    setError(null);
    try {
      const api = (await chainClient.getApiForWallet()) as unknown as Parameters<
        typeof fetchGlobalPools
      >[0];
      setPools(await fetchGlobalPools(api));
    } catch (e) {
      setPools(null);
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setLoading(false);
    }
  }, [enabled]);

  useEffect(() => {
    if (!enabled || config.useMock) {
      setPools(null);
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

  return { pools, loading, error, refresh };
}
