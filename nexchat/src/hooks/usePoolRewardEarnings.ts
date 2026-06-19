import { useCallback, useEffect, useRef, useState } from "react";
import { config } from "@/config";
import { chainClient } from "@/chain/chainClient";
import {
  fetchCurrentBlock,
  fetchCurrentRoundFunding,
  fetchPoolRewardMemberView,
  fetchUnallocatedPool,
} from "@/earnings/poolRewardQueries";
import type { PoolRewardMemberView, PoolRewardRoundFunding } from "@/earnings/types";

const REFRESH_MS = 15_000;

// EN: Poll pool reward detail for selected entity.
// CN: 轮询选定 Entity 的奖池领取详情。
export function usePoolRewardEarnings(
  address: string | null,
  entityId: number | null,
  enabled: boolean,
) {
  const [memberView, setMemberView] = useState<PoolRewardMemberView | null>(null);
  const [poolBalance, setPoolBalance] = useState("0");
  const [funding, setFunding] = useState<PoolRewardRoundFunding | null>(null);
  const [currentBlock, setCurrentBlock] = useState(0);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const timerRef = useRef<ReturnType<typeof setInterval> | null>(null);

  const refresh = useCallback(async () => {
    if (!enabled || config.useMock || !address || entityId == null) {
      setMemberView(null);
      setPoolBalance("0");
      setFunding(null);
      setCurrentBlock(0);
      return;
    }
    setLoading(true);
    setError(null);
    try {
      const api = (await chainClient.getApiForWallet()) as unknown as Parameters<
        typeof fetchPoolRewardMemberView
      >[0];
      const [view, balance, roundFunding, block] = await Promise.all([
        fetchPoolRewardMemberView(api, entityId, address),
        fetchUnallocatedPool(api, entityId),
        fetchCurrentRoundFunding(api, entityId),
        fetchCurrentBlock(api),
      ]);
      setMemberView(view);
      setPoolBalance(balance);
      setFunding(roundFunding);
      setCurrentBlock(block);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setLoading(false);
    }
  }, [enabled, address, entityId]);

  useEffect(() => {
    if (!enabled || config.useMock) {
      setMemberView(null);
      setPoolBalance("0");
      setFunding(null);
      setCurrentBlock(0);
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

  return { memberView, poolBalance, funding, currentBlock, loading, error, refresh };
}
