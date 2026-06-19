// EN: DR session persistence (design §17.2 `nexchat/x3dh/pickle/v1`). Olm pickles — the
// long-term account (identity + prekeys) and one Double Ratchet session per peer device —
// are serialized by the engine as JSON and stored ENCRYPTED at rest under a vault-derived
// AES-256-GCM key (`keyVault.deriveDrSessionKey`), so ratchet state survives refresh /
// restart without ever writing plaintext key material to disk. Physically isolated from
// MLS state (decoupling invariant, design §9): a dedicated IndexedDB database, a dedicated
// HKDF context, and no import of `@/mls/*`.
// CN: DR 会话持久化（设计 §17.2 `nexchat/x3dh/pickle/v1`）。Olm pickle——长期账户（身份 +
// 预密钥）与每对端设备一条双棘轮会话——由引擎序列化为 JSON，并用 vault 派生的 AES-256-GCM
// 钥（`keyVault.deriveDrSessionKey`）**静态加密**落盘，使棘轮态跨刷新/重启存活，且绝不把
// 明文钥料写盘。与 MLS 状态物理隔离（解耦不变量，设计 §9）：独立 IndexedDB 库、独立 HKDF
// 上下文、不 import `@/mls/*`。

import { keyVault } from "@/keyvault/keyvault";

/// EN: The published OPK set a device serves to X3DH initiators (§19): the on-chain root,
/// the leaf public keys (hex) needed to build Merkle proofs, and the best-effort spent set
/// of already-dispensed leaves. Persisted so the OPK responder survives restart. CN: 设备
/// 向 X3DH 发起方服务的已发布 OPK 集合（§19）：链上根、构造 Merkle 证明所需的叶公钥（hex），
/// 以及已单发叶子的 best-effort 已用集合。持久化以使 OPK 响应方跨重启存活。
export interface PublishedOpkBundle {
  /// EN: This device id (hex). CN: 本设备 id（hex）。
  device: string;
  /// EN: On-chain OPK Merkle root (hex). CN: 链上 OPK Merkle 根（hex）。
  root: string;
  /// EN: Published OPK public keys (hex). CN: 已发布 OPK 公钥（hex）。
  opks: string[];
  /// EN: Already-dispensed OPK public keys (hex). CN: 已单发 OPK 公钥（hex）。
  spent: string[];
}

/// EN: At-rest persistence boundary for the DR engine. All pickles are opaque strings; the
/// implementation encrypts them. CN: DR 引擎的静态持久化边界。pickle 均为不透明字符串，由实现加密。
export interface DrPersistence {
  /// EN: Open the per-account store (idempotent). CN: 打开按账户隔离的存储（幂等）。
  open(namespace: string): Promise<void>;
  loadAccount(): Promise<string | null>;
  saveAccount(pickle: string): Promise<void>;
  listSessions(): Promise<string[]>;
  loadSession(deviceHex: string): Promise<string | null>;
  saveSession(deviceHex: string, pickle: string): Promise<void>;
  removeSession(deviceHex: string): Promise<void>;
  loadOpkBundle(): Promise<PublishedOpkBundle | null>;
  saveOpkBundle(bundle: PublishedOpkBundle): Promise<void>;
  /// EN: Drop all DR state (account switch / wipe). CN: 清除全部 DR 状态（切账户/擦除）。
  clearAll(): Promise<void>;
}

/// EN: In-memory `DrPersistence` (tests / SSR / ephemeral sessions). CN: 内存版
/// `DrPersistence`（测试 / SSR / 临时会话）。
export class MemoryDrSessionStore implements DrPersistence {
  private account: string | null = null;
  private sessions = new Map<string, string>();
  private opk: PublishedOpkBundle | null = null;

  async open(_namespace?: string): Promise<void> {}
  async loadAccount(): Promise<string | null> {
    return this.account;
  }
  async saveAccount(pickle: string): Promise<void> {
    this.account = pickle;
  }
  async listSessions(): Promise<string[]> {
    return [...this.sessions.keys()];
  }
  async loadSession(deviceHex: string): Promise<string | null> {
    return this.sessions.get(deviceHex) ?? null;
  }
  async saveSession(deviceHex: string, pickle: string): Promise<void> {
    this.sessions.set(deviceHex, pickle);
  }
  async removeSession(deviceHex: string): Promise<void> {
    this.sessions.delete(deviceHex);
  }
  async loadOpkBundle(): Promise<PublishedOpkBundle | null> {
    return this.opk;
  }
  async saveOpkBundle(bundle: PublishedOpkBundle): Promise<void> {
    this.opk = bundle;
  }
  async clearAll(): Promise<void> {
    this.account = null;
    this.sessions.clear();
    this.opk = null;
  }
}

interface Sealed {
  iv: Uint8Array;
  ct: ArrayBuffer;
}

const DB_PREFIX = "nexchat-dr";
const STORE = "dr";
const ACCOUNT_KEY = "__account__";
const OPK_KEY = "__opk__";
const SESSION_PREFIX = "session/";

function req<T>(r: IDBRequest<T>): Promise<T> {
  return new Promise((resolve, reject) => {
    r.onsuccess = () => resolve(r.result);
    r.onerror = () => reject(r.error);
  });
}

function txDone(tx: IDBTransaction): Promise<void> {
  return new Promise((resolve, reject) => {
    tx.oncomplete = () => resolve();
    tx.onerror = () => reject(tx.error);
    tx.onabort = () => reject(tx.error);
  });
}

/// EN: IndexedDB-backed, at-rest-encrypted `DrPersistence`. Each row is `{ iv, ct }` where
/// `ct` is AES-256-GCM(UTF-8 pickle) under the vault-derived DR key. CN: 基于 IndexedDB 的
/// 静态加密 `DrPersistence`。每行为 `{ iv, ct }`，`ct` 为 vault 派生 DR 钥下的
/// AES-256-GCM(UTF-8 pickle)。
export class EncryptedDrSessionStore implements DrPersistence {
  private db: IDBDatabase | null = null;
  private key: CryptoKey | null = null;
  private ready: Promise<void> | null = null;

  open(namespace: string): Promise<void> {
    if (this.ready) return this.ready;
    this.ready = (async () => {
      this.key = await keyVault.deriveDrSessionKey(namespace);
      this.db = await new Promise<IDBDatabase>((resolve, reject) => {
        const o = indexedDB.open(`${DB_PREFIX}-${namespace}`, 1);
        o.onupgradeneeded = () => {
          const db = o.result;
          if (!db.objectStoreNames.contains(STORE)) db.createObjectStore(STORE);
        };
        o.onsuccess = () => resolve(o.result);
        o.onerror = () => reject(o.error);
      });
    })();
    return this.ready;
  }

  private async seal(value: string): Promise<Sealed> {
    const iv = crypto.getRandomValues(new Uint8Array(12));
    const ct = await crypto.subtle.encrypt(
      { name: "AES-GCM", iv: iv as BufferSource },
      this.key!,
      new TextEncoder().encode(value),
    );
    return { iv, ct };
  }

  private async unseal(sealed: Sealed | undefined): Promise<string | null> {
    if (!sealed) return null;
    try {
      const pt = await crypto.subtle.decrypt(
        { name: "AES-GCM", iv: sealed.iv as BufferSource },
        this.key!,
        sealed.ct,
      );
      return new TextDecoder().decode(pt);
    } catch (e) {
      if (e instanceof DOMException && e.name === "OperationError") return null;
      throw e;
    }
  }

  private async get(key: string): Promise<string | null> {
    await this.ready;
    const tx = this.db!.transaction(STORE, "readonly");
    const sealed = await req(tx.objectStore(STORE).get(key) as IDBRequest<Sealed | undefined>);
    await txDone(tx);
    return this.unseal(sealed);
  }

  private async put(key: string, value: string): Promise<void> {
    await this.ready;
    const sealed = await this.seal(value);
    const tx = this.db!.transaction(STORE, "readwrite");
    tx.objectStore(STORE).put(sealed, key);
    await txDone(tx);
  }

  async loadAccount(): Promise<string | null> {
    return this.get(ACCOUNT_KEY);
  }
  async saveAccount(pickle: string): Promise<void> {
    return this.put(ACCOUNT_KEY, pickle);
  }

  async listSessions(): Promise<string[]> {
    await this.ready;
    const tx = this.db!.transaction(STORE, "readonly");
    const keys = await req(tx.objectStore(STORE).getAllKeys() as IDBRequest<IDBValidKey[]>);
    await txDone(tx);
    return keys
      .map(String)
      .filter((k) => k.startsWith(SESSION_PREFIX))
      .map((k) => k.slice(SESSION_PREFIX.length));
  }
  async loadSession(deviceHex: string): Promise<string | null> {
    return this.get(SESSION_PREFIX + deviceHex);
  }
  async saveSession(deviceHex: string, pickle: string): Promise<void> {
    return this.put(SESSION_PREFIX + deviceHex, pickle);
  }
  async removeSession(deviceHex: string): Promise<void> {
    await this.ready;
    const tx = this.db!.transaction(STORE, "readwrite");
    tx.objectStore(STORE).delete(SESSION_PREFIX + deviceHex);
    await txDone(tx);
  }

  async loadOpkBundle(): Promise<PublishedOpkBundle | null> {
    const raw = await this.get(OPK_KEY);
    return raw ? (JSON.parse(raw) as PublishedOpkBundle) : null;
  }
  async saveOpkBundle(bundle: PublishedOpkBundle): Promise<void> {
    return this.put(OPK_KEY, JSON.stringify(bundle));
  }

  async clearAll(): Promise<void> {
    await this.ready;
    const tx = this.db!.transaction(STORE, "readwrite");
    tx.objectStore(STORE).clear();
    await txDone(tx);
  }
}
