// EN: Global concurrency cap for short-lived ("one-shot") relay WebSockets. Each KV-style
// relay op (mailbox fetch/consume, pointer push/fetch, inbox lookup/register) opens its own
// WebSocket. A burst (e.g. clearing dozens of stale mailbox frames) can open them faster than
// they close and trip the browser's per-host WebSocket cap ("Insufficient resources"), which
// then also blocks the persistent relay socket. This semaphore bounds how many one-shot
// sockets are alive at once; excess callers queue until a slot frees.
// CN: 短命（"一次性"）relay WebSocket 的全局并发上限。每个 KV 型 relay 操作（信箱拉取/删除、
// 指针推送/拉取、inbox 查询/注册）都各开一个 WebSocket。爆发时（如清理几十条陈旧信箱帧）开得
// 比关得快，会触发浏览器单 host 的 WebSocket 上限（"Insufficient resources"），进而连持久连接
// 也建不起来。此信号量限制同时存活的一次性套接字数量，超出的调用排队等待空位。

// EN: 6 leaves ample headroom under the browser per-host WS cap while the persistent relay
// socket keeps its own slot. CN: 取 6，在浏览器单 host WS 上限下留足余量，同时持久 relay
// 连接另占其槽。
const MAX_CONCURRENT = 6;

let active = 0;
const waiters: Array<() => void> = [];

function acquire(): Promise<void> {
  if (active < MAX_CONCURRENT) {
    active++;
    return Promise.resolve();
  }
  // EN: slot is handed off directly on release (active stays at the cap). CN: 释放时直接移交
  // 槽位（active 保持在上限），故此处不再自增。
  return new Promise<void>((resolve) => waiters.push(resolve));
}

function release(): void {
  const next = waiters.shift();
  if (next) {
    next();
  } else {
    active = Math.max(0, active - 1);
  }
}

/// EN: Run `fn` while holding a one-shot WS slot; releases on settle (success or failure).
/// CN: 持有一个一次性 WS 槽位运行 `fn`；无论成功失败结束即释放。
export async function withOneShotSlot<T>(fn: () => Promise<T>): Promise<T> {
  await acquire();
  try {
    return await fn();
  } finally {
    release();
  }
}
