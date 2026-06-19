// EN: fetch() with AbortController timeout — prevents cloud restore / chain reads from
// hanging indefinitely when IPFS gateway or JSON-RPC is slow or unreachable.
// CN: 带 AbortController 超时的 fetch()——IPFS 网关或 JSON-RPC 慢/不可达时避免云恢复、
// 链上读无限挂起。

export class FetchTimeoutError extends Error {
  constructor(label: string, timeoutMs: number) {
    super(`${label} timed out after ${timeoutMs}ms`);
    this.name = "FetchTimeoutError";
  }
}

/// EN: Reject a promise if it does not settle within `timeoutMs`. Use to bound awaits that
/// have no internal timeout (e.g. polkadot.js `ApiPromise.create` over a dead WS), so the UI
/// never hangs forever. The underlying work keeps running but the caller stops waiting.
/// CN: 给无内置超时的 await 设上限（如 WS 不可达时的 polkadot.js `ApiPromise.create`），避免
/// UI 永久挂起。底层任务仍继续，但调用方不再等待。
export function withTimeout<T>(p: Promise<T>, timeoutMs: number, label = "operation"): Promise<T> {
  let timer: ReturnType<typeof setTimeout> | undefined;
  const timeout = new Promise<never>((_, reject) => {
    timer = setTimeout(() => reject(new FetchTimeoutError(label, timeoutMs)), timeoutMs);
  });
  return Promise.race([p, timeout]).finally(() => {
    if (timer) clearTimeout(timer);
  }) as Promise<T>;
}

/// EN: `fetch` that rejects with `FetchTimeoutError` when `timeoutMs` elapses.
/// CN: 超过 `timeoutMs` 以 `FetchTimeoutError` 拒绝的 `fetch`。
export async function fetchWithTimeout(
  input: RequestInfo | URL,
  init?: RequestInit,
  timeoutMs = 15_000,
  label = "fetch",
): Promise<Response> {
  const ctrl = new AbortController();
  const t = setTimeout(() => ctrl.abort(), timeoutMs);
  try {
    return await fetch(input, { ...init, signal: ctrl.signal });
  } catch (e) {
    if (e instanceof DOMException && e.name === "AbortError") {
      throw new FetchTimeoutError(label, timeoutMs);
    }
    throw e;
  } finally {
    clearTimeout(t);
  }
}
