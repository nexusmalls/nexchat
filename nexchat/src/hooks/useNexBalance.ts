import { useCallback, useEffect, useRef, useState } from "react";
import { config } from "@/config";
import { chainClient } from "@/chain/chainClient";
import { canonicalAddress } from "@/wallet/address";

export interface NexBalance {
  free: bigint;
  reserved: bigint;
}

const REFRESH_MS = 15_000;

// EN: Poll NEX free/reserved balance for an account.
// CN: 轮询账户 NEX 可用/保留余额。
export function useNexBalance(address: string | null, enabled = true) {
  const [balance, setBalance] = useState<NexBalance | null>(null);
  const [loading, setLoading] = useState(false);
  const timerRef = useRef<ReturnType<typeof setInterval> | null>(null);

  const refresh = useCallback(async () => {
    if (!enabled || config.useMock || !address) {
      setBalance(null);
      return;
    }
    setLoading(true);
    try {
      const api = (await chainClient.getApiForWallet()) as unknown as {
        query: {
          system: {
            account: (who: string) => Promise<{
              data: { free: { toString: () => string }; reserved: { toString: () => string } };
            }>;
          };
        };
      };
      const who = canonicalAddress(address);
      const acc = await api.query.system.account(who);
      setBalance({
        free: BigInt(acc.data.free.toString()),
        reserved: BigInt(acc.data.reserved.toString()),
      });
    } catch {
      setBalance(null);
    } finally {
      setLoading(false);
    }
  }, [enabled, address]);

  useEffect(() => {
    if (!enabled || config.useMock || !address) {
      setBalance(null);
      return;
    }
    void refresh();
    timerRef.current = setInterval(() => void refresh(), REFRESH_MS);
    return () => {
      if (timerRef.current) clearInterval(timerRef.current);
    };
  }, [enabled, address, refresh]);

  return { balance, loading, refresh };
}
