import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { config } from "@/config";
import { chainClient } from "@/chain/chainClient";
import {
  discoverEarningEntities,
  fetchCommissionDashboard,
  fetchCommissionOverview,
  fetchMemberCommissionStats,
  fetchRepurchaseConfig,
  fetchWithdrawalRecords,
} from "@/earnings/commissionQueries";
import { buildEarningsPlugins } from "@/earnings/plugins";
import type {
  CommissionDashboard,
  CommissionMemberStats,
  CommissionOverview,
  EarningEntityOption,
  RepurchaseConfig,
  WithdrawalRecord,
} from "@/earnings/types";
import { useEntityShop } from "@/hooks/useEntityShop";
import { useOwnedEntities } from "@/hooks/useOwnedEntities";
import { getManagedShopIds } from "@/shop/seller";

const REFRESH_MS = 15_000;

// EN: Poll commission earnings for selected entity (Me tab).
// CN: 轮询选定 Entity 的佣金收益（「我」Tab）。
export function useEarnings(address: string | null, entityId: number | null, enabled: boolean) {
  const [memberStats, setMemberStats] = useState<CommissionMemberStats | null>(null);
  const [overview, setOverview] = useState<CommissionOverview | null>(null);
  const [dashboard, setDashboard] = useState<CommissionDashboard | null>(null);
  const [withdrawals, setWithdrawals] = useState<WithdrawalRecord[]>([]);
  const [repurchaseConfig, setRepurchaseConfig] = useState<RepurchaseConfig | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const timerRef = useRef<ReturnType<typeof setInterval> | null>(null);

  const plugins = useMemo(
    () => buildEarningsPlugins(overview, dashboard),
    [overview, dashboard],
  );

  const refresh = useCallback(async () => {
    if (!enabled || config.useMock || !address || entityId == null) {
      setMemberStats(null);
      setOverview(null);
      setDashboard(null);
      setWithdrawals([]);
      setRepurchaseConfig(null);
      return;
    }
    setLoading(true);
    setError(null);
    try {
      const api = (await chainClient.getApiForWallet()) as unknown as Parameters<
        typeof fetchMemberCommissionStats
      >[0];
      const [stats, ov, dash, records, repurchase] = await Promise.all([
        fetchMemberCommissionStats(api, entityId, address),
        fetchCommissionOverview(api, entityId),
        fetchCommissionDashboard(api, entityId, address),
        fetchWithdrawalRecords(api, entityId, address),
        fetchRepurchaseConfig(api, entityId),
      ]);
      setMemberStats(stats);
      setOverview(ov);
      setDashboard(dash);
      setWithdrawals(records);
      setRepurchaseConfig(repurchase);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setLoading(false);
    }
  }, [enabled, address, entityId]);

  useEffect(() => {
    if (!enabled || config.useMock) {
      setMemberStats(null);
      setOverview(null);
      setDashboard(null);
      setWithdrawals([]);
      setRepurchaseConfig(null);
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

  return {
    memberStats,
    overview,
    dashboard,
    withdrawals,
    repurchaseConfig,
    plugins,
    loading,
    error,
    refresh,
  };
}

// EN: Earning entities — catalog seeds + commission stats + registry membership.
// CN: 收益页 Entity 列表（目录种子 + 佣金记录 + registry 会员）。
export function useEarningEntities(address: string | null, enabled: boolean) {
  const { catalog } = useEntityShop(enabled, address);
  const { ownedEntityIds } = useOwnedEntities(address, enabled);
  const [entities, setEntities] = useState<EarningEntityOption[]>([]);
  const [loading, setLoading] = useState(false);

  const seedIds = useMemo(() => {
    const ids = new Set<number>();
    for (const id of ownedEntityIds) ids.add(id);
    if (catalog) {
      for (const id of catalog.memberByEntity.keys()) ids.add(id);
      for (const shopId of getManagedShopIds(catalog, address, ownedEntityIds)) {
        const shop = catalog.shopById.get(shopId);
        if (shop) ids.add(shop.entityId);
      }
    }
    return [...ids];
  }, [catalog, address, ownedEntityIds]);

  const seedKey = seedIds.join(",");

  const refresh = useCallback(async () => {
    if (!enabled || config.useMock || !address) {
      setEntities([]);
      return;
    }
    setLoading(true);
    try {
      const api = (await chainClient.getApiForWallet()) as unknown as Parameters<
        typeof discoverEarningEntities
      >[0];
      const list = await discoverEarningEntities(api, address, seedIds);
      setEntities(list);
    } catch {
      setEntities([]);
    } finally {
      setLoading(false);
    }
  }, [enabled, address, seedKey]);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  return { entities, loading, seedIds, refresh };
}
