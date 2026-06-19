import { useCallback, useEffect, useState } from "react";
import { config } from "@/config";
import { chainClient } from "@/chain/chainClient";
import { fetchAllActiveEntities } from "@/earnings/entityRegistryQueries";
import type { RegistryEntity } from "@/earnings/types";
import { fetchMembershipsForEntities } from "@/shop/entityMemberQueries";
import type { EntityMemberApi } from "@/shop/entityMemberQueries";

// EN: Poll active entities from `entityRegistry` (settings-style join picker).
// CN: 轮询 registry 活跃 Entity（设置页式加入选择器）。
export function useAllEntities(enabled: boolean) {
  const [entities, setEntities] = useState<RegistryEntity[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const refresh = useCallback(async () => {
    if (!enabled || config.useMock) {
      setEntities([]);
      setError(null);
      return;
    }
    setLoading(true);
    setError(null);
    try {
      const api = (await chainClient.getApiForWallet()) as unknown as Parameters<
        typeof fetchAllActiveEntities
      >[0];
      const list = await fetchAllActiveEntities(api);
      setEntities(list);
    } catch (e) {
      setEntities([]);
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setLoading(false);
    }
  }, [enabled]);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  return { entities, loading, error, refresh };
}

// EN: Which of `entityIds` the account has joined as member.
// CN: 当前账户已加入的 Entity id 列表。
export function useMyMemberships(
  entityIds: number[],
  address: string | null,
  enabled: boolean,
) {
  const [memberIds, setMemberIds] = useState<number[]>([]);
  const [loading, setLoading] = useState(false);

  const key = entityIds.join(",");

  const refresh = useCallback(async () => {
    if (!enabled || config.useMock || !address || entityIds.length === 0) {
      setMemberIds([]);
      return;
    }
    setLoading(true);
    try {
      const api = (await chainClient.getApiForWallet()) as unknown as EntityMemberApi;
      const map = await fetchMembershipsForEntities(api, entityIds, address);
      setMemberIds([...map.keys()].sort((a, b) => a - b));
    } catch {
      setMemberIds([]);
    } finally {
      setLoading(false);
    }
  }, [enabled, address, key]);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  return { memberIds, loading, refresh };
}
