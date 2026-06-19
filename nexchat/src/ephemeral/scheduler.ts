// EN: Periodic purge of expired ephemeral messages from LocalStore + UI refresh hook.
// CN: 定期从 LocalStore 清除过期的阅后即焚消息，并触发 UI 刷新。

import { localStore } from "@/store/localStore";

export interface PurgeHit {
  convId: string;
  removed: string[];
}

let timer: ReturnType<typeof setInterval> | null = null;

/// EN: Start the 1s purge loop (idempotent). CN: 启动 1s 清理循环（幂等）。
export function startEphemeralScheduler(onPurge: (hits: PurgeHit[]) => void): void {
  if (timer) return;
  timer = setInterval(() => {
    void localStore.purgeExpiredEphemeral(Date.now()).then((hits) => {
      if (hits.length > 0) onPurge(hits);
    });
  }, 1000);
}

/// EN: Stop the purge loop. CN: 停止清理循环。
export function stopEphemeralScheduler(): void {
  if (timer) clearInterval(timer);
  timer = null;
}
