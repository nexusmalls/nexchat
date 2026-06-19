// EN: Tiny IndexedDB-backed blob store for OpenMLS state snapshots (one row per
// persistence key, normally the account address). The whole OpenMLS crypto state is
// an opaque byte blob produced by `MlsClient.exportState()`; we just stash/fetch it.
// Degrades to a no-op when IndexedDB is unavailable (e.g. Node test runner) so the
// engine still works purely in-memory there.
// CN: 一个极小的、基于 IndexedDB 的 blob 存储，用于 OpenMLS 状态快照（每个持久化键一行，
// 通常是账户地址）。整套 OpenMLS 密码状态是 `MlsClient.exportState()` 产出的不透明字节 blob，
// 这里只负责存取。当 IndexedDB 不可用时（如 Node 测试）降级为 no-op，引擎仍可纯内存运行。

const DB_NAME = "nexchat-mls";
const STORE = "state";
const VERSION = 1;

function hasIdb(): boolean {
  return typeof indexedDB !== "undefined";
}

function openDb(): Promise<IDBDatabase> {
  return new Promise((resolve, reject) => {
    const req = indexedDB.open(DB_NAME, VERSION);
    req.onupgradeneeded = () => {
      const db = req.result;
      if (!db.objectStoreNames.contains(STORE)) db.createObjectStore(STORE);
    };
    req.onsuccess = () => resolve(req.result);
    req.onerror = () => reject(req.error);
  });
}

/// EN: Load a persisted snapshot for `key`, or null if none / unavailable.
/// CN: 读取 `key` 的持久化快照；无或不可用则返回 null。
export async function loadMlsState(key: string): Promise<Uint8Array | null> {
  if (!hasIdb()) return null;
  try {
    const db = await openDb();
    return await new Promise<Uint8Array | null>((resolve, reject) => {
      const tx = db.transaction(STORE, "readonly");
      const req = tx.objectStore(STORE).get(key);
      req.onsuccess = () => {
        const v = req.result as ArrayBuffer | Uint8Array | undefined;
        if (!v) resolve(null);
        else resolve(v instanceof Uint8Array ? v : new Uint8Array(v));
      };
      req.onerror = () => reject(req.error);
      tx.oncomplete = () => db.close();
    });
  } catch {
    return null;
  }
}

/// EN: Persist a snapshot for `key`. CN: 持久化 `key` 的快照。
export async function saveMlsState(key: string, blob: Uint8Array): Promise<void> {
  if (!hasIdb()) return;
  try {
    const db = await openDb();
    await new Promise<void>((resolve, reject) => {
      const tx = db.transaction(STORE, "readwrite");
      // Copy into a standalone ArrayBuffer; the WASM-owned view can be detached.
      tx.objectStore(STORE).put(blob.slice(), key);
      tx.oncomplete = () => {
        db.close();
        resolve();
      };
      tx.onerror = () => reject(tx.error);
    });
  } catch {
    /* best-effort persistence */
  }
}

/// EN: Drop the snapshot for `key` (e.g. on logout / reset). CN: 删除 `key` 的快照（登出/重置）。
export async function clearMlsState(key: string): Promise<void> {
  if (!hasIdb()) return;
  try {
    const db = await openDb();
    await new Promise<void>((resolve, reject) => {
      const tx = db.transaction(STORE, "readwrite");
      tx.objectStore(STORE).delete(key);
      tx.oncomplete = () => {
        db.close();
        resolve();
      };
      tx.onerror = () => reject(tx.error);
    });
  } catch {
    /* ignore */
  }
}
