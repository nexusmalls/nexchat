import { useCallback, useEffect, useRef, useState } from "react";
import { config } from "@/config";
import { chainClient } from "@/chain/chainClient";
import { fetchStakingOverview } from "@/staking/stakingQueries";
import type { StakingOverview } from "@/staking/types";

const REFRESH_MS = 15_000;

// EN: Poll staking overview for nominator UI (Me → 节点提名).
// CN: 轮询质押概览（「我」→ 节点提名）。
export function useStaking(address: string | null, enabled: boolean) {
  const [overview, setOverview] = useState<StakingOverview | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const timerRef = useRef<ReturnType<typeof setInterval> | null>(null);

  const refresh = useCallback(async () => {
    if (!enabled || config.useMock || !address) {
      setOverview(null);
      setError(null);
      return;
    }
    setLoading(true);
    setError(null);
    try {
      const api = (await chainClient.getApiForWallet()) as unknown as Parameters<
        typeof fetchStakingOverview
      >[0];
      const data = await fetchStakingOverview(api, address);
      setOverview(data);
    } catch (e) {
      setOverview(null);
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setLoading(false);
    }
  }, [enabled, address]);

  useEffect(() => {
    if (!enabled || config.useMock || !address) {
      setOverview(null);
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

  return { overview, loading, error, refresh };
}
