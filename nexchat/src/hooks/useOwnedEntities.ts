import { useCallback, useEffect, useState } from "react";
import { config } from "@/config";
import { chainClient } from "@/chain/chainClient";
import { fetchOwnedEntities } from "@/earnings/entityRegistryQueries";
import type { RegistryEntity } from "@/earnings/types";

// EN: Poll entities owned by `address` (`entityRegistry.userEntity`).
// CN: 轮询账户拥有的 Entity（`entityRegistry.userEntity`）。
export function useOwnedEntities(address: string | null, enabled: boolean) {
  const [entities, setEntities] = useState<RegistryEntity[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const refresh = useCallback(async () => {
    if (!enabled || config.useMock || !address) {
      setEntities([]);
      setError(null);
      return;
    }
    setLoading(true);
    setError(null);
    try {
      const api = (await chainClient.getApiForWallet()) as unknown as Parameters<
        typeof fetchOwnedEntities
      >[0];
      const list = await fetchOwnedEntities(api, address);
      setEntities(list);
    } catch (e) {
      setEntities([]);
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setLoading(false);
    }
  }, [enabled, address]);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  const ownedEntityIds = entities.map((e) => e.id);

  return { entities, ownedEntityIds, loading, error, refresh };
}
