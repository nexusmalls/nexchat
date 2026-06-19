import { useCallback, useEffect, useRef, useState } from "react";
import { config } from "@/config";
import { chainClient } from "@/chain/chainClient";
import { fetchSingleLineEarnings } from "@/earnings/singleLineQueries";
import type { SingleLineMemberStats, SingleLinePayoutRecord } from "@/earnings/types";

const REFRESH_MS = 15_000;

// EN: Poll single-line (win-win) commission detail for selected entity.
// CN: 轮询选定 Entity 的单线共赢佣金详情。
export function useSingleLineEarnings(
  address: string | null,
  entityId: number | null,
  enabled: boolean,
) {
  const [stats, setStats] = useState<SingleLineMemberStats | null>(null);
  const [records, setRecords] = useState<SingleLinePayoutRecord[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const timerRef = useRef<ReturnType<typeof setInterval> | null>(null);

  const refresh = useCallback(async () => {
    if (!enabled || config.useMock || !address || entityId == null) {
      setStats(null);
      setRecords([]);
      return;
    }
    setLoading(true);
    setError(null);
    try {
      const api = (await chainClient.getApiForWallet()) as unknown as Parameters<
        typeof fetchSingleLineEarnings
      >[0];
      const { stats: memberStats, records: payouts } = await fetchSingleLineEarnings(
        api,
        entityId,
        address,
      );
      setStats(memberStats);
      setRecords(payouts);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setLoading(false);
    }
  }, [enabled, address, entityId]);

  useEffect(() => {
    if (!enabled || config.useMock) {
      setStats(null);
      setRecords([]);
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

  return { stats, records, loading, error, refresh };
}
