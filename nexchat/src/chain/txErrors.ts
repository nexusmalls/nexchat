// EN: Shared Substrate transaction-pool / dispatch error classifiers for ChainClient callers.
// CN: ChainClient 调用方共用的 Substrate 交易池 / dispatch 错误分类。

/// EN: Tx pool rejected replacement (1014) — usually a duplicate submit with the same nonce.
/// CN: 交易池拒绝替换（1014）——通常是相同 nonce 的重复提交。
export function isPoolConflictError(e: unknown): boolean {
  const msg = e instanceof Error ? e.message : String(e);
  return msg.includes("1014") || msg.includes("Priority is too low");
}

/// EN: Same priority as an in-pool tx `(N vs N)` — wait for inclusion, do not resubmit immediately.
/// CN: 与池中交易 priority 相同 `(N vs N)`——应等待上链，勿立即重提。
export function isEqualPoolPriorityError(e: unknown): boolean {
  const msg = e instanceof Error ? e.message : String(e);
  const m = msg.match(/\((\d+) vs (\d+)\)/);
  return !!m && m[1] === m[2];
}
