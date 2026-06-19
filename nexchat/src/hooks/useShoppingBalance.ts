import { useCallback, useEffect, useState } from "react";
import { config } from "@/config";
import { chainClient } from "@/chain/chainClient";
import { canonicalAddress } from "@/wallet/address";

// EN: Entity member shopping balance (NEX) for mixed payment.
// CN: Entity 会员购物余额（NEX），用于混合支付。
export function useShoppingBalance(
  entityId: number | null | undefined,
  address: string | null | undefined,
) {
  const [balance, setBalance] = useState<string>("0");
  const [loading, setLoading] = useState(false);

  const refresh = useCallback(async () => {
    if (!entityId || !address || config.useMock) {
      setBalance("0");
      return;
    }
    setLoading(true);
    try {
      const api = (await chainClient.getApiForWallet()) as unknown as {
        query: {
          entityLoyalty?: {
            memberShoppingBalance?: (
              e: number,
              a: string,
            ) => Promise<{ toJSON?: () => unknown }>;
          };
        };
      };
      const q = api.query.entityLoyalty?.memberShoppingBalance;
      if (!q) {
        setBalance("0");
        return;
      }
      const raw = await q(entityId, canonicalAddress(address));
      setBalance(String(raw?.toJSON?.() ?? raw ?? "0"));
    } catch {
      setBalance("0");
    } finally {
      setLoading(false);
    }
  }, [entityId, address]);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  return { balance, loading, refresh };
}
