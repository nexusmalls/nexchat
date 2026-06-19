import { useCallback, useEffect, useRef, useState } from "react";
import { config } from "@/config";
import { chainClient } from "@/chain/chainClient";
import { fetchChainInfo, type ChainInfo } from "@/chain/chainQueries";

const REFRESH_MS = 30_000;

// EN: Poll chain metadata + subscribe to new heads for best block (Me → 链上详情).
// CN: 轮询链元数据并订阅新区块高度（「我」→ 链上详情）。
export function useChainInfo(enabled: boolean) {
  const [info, setInfo] = useState<ChainInfo | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const timerRef = useRef<ReturnType<typeof setInterval> | null>(null);
  const unsubRef = useRef<(() => void) | null>(null);

  const refresh = useCallback(async () => {
    if (!enabled || config.useMock) {
      setInfo(null);
      setError(null);
      return;
    }
    setLoading(true);
    setError(null);
    try {
      const api = (await chainClient.getApiForWallet()) as unknown as Parameters<
        typeof fetchChainInfo
      >[0];
      const data = await fetchChainInfo(api);
      setInfo((prev) => (prev ? { ...data, bestBlock: prev.bestBlock || data.bestBlock } : data));
    } catch (e) {
      setInfo(null);
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setLoading(false);
    }
  }, [enabled]);

  useEffect(() => {
    if (!enabled || config.useMock) {
      setInfo(null);
      setError(null);
      setLoading(false);
      return;
    }

    let cancelled = false;

    void refresh();
    timerRef.current = setInterval(() => void refresh(), REFRESH_MS);

    void (async () => {
      try {
        const api = (await chainClient.getApiForWallet()) as unknown as {
          rpc: {
            chain: {
              subscribeNewHeads: (
                cb: (header: { number: { toNumber: () => number } }) => void,
              ) => Promise<() => void>;
            };
          };
        };
        const unsub = await api.rpc.chain.subscribeNewHeads((header) => {
          if (cancelled) return;
          const n = header.number.toNumber();
          setInfo((prev) => (prev ? { ...prev, bestBlock: n } : null));
        });
        if (cancelled) unsub();
        else unsubRef.current = unsub;
      } catch {
        /* subscription optional */
      }
    })();

    return () => {
      cancelled = true;
      if (timerRef.current) clearInterval(timerRef.current);
      unsubRef.current?.();
      unsubRef.current = null;
    };
  }, [enabled, refresh]);

  return { info, loading, error, refresh };
}
