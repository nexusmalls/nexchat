import { useCallback, useState } from "react";

export type TxStatus = "idle" | "pending" | "ok" | "error";

// EN: Simple async tx state for market extrinsics.
// CN: 市场 extrinsic 的简易异步状态。
export function useMarketTx(onSuccess?: () => void) {
  const [status, setStatus] = useState<TxStatus>("idle");
  const [error, setError] = useState<string | null>(null);

  const reset = useCallback(() => {
    setStatus("idle");
    setError(null);
  }, []);

  const run = useCallback(
    async (fn: () => Promise<string>) => {
      setStatus("pending");
      setError(null);
      try {
        await fn();
        setStatus("ok");
        onSuccess?.();
      } catch (e) {
        const msg = e instanceof Error ? e.message : String(e);
        setError(msg);
        setStatus("error");
      }
    },
    [onSuccess],
  );

  return {
    status,
    error,
    busy: status === "pending",
    run,
    reset,
  };
}
