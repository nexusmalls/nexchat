import { useCallback, useEffect, useState } from "react";
import { chainClient } from "@/chain/chainClient";
import { config } from "@/config";

// EN: Poll on-chain KeyPackage pool size for profile / join diagnostics.
// CN: 轮询链上 KeyPackage 池大小，供 Profile / 入群诊断。
export function useChainKeyPackageCount(address: string | null, enabled = true) {
  const [count, setCount] = useState<number | null>(null);
  const [loading, setLoading] = useState(false);

  const refresh = useCallback(async () => {
    if (!enabled || config.useMock || !address) {
      setCount(null);
      return;
    }
    setLoading(true);
    try {
      setCount(await chainClient.keyPackageCountOf(address));
    } catch {
      setCount(null);
    } finally {
      setLoading(false);
    }
  }, [enabled, address]);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  return { count, loading, refresh };
}
