// EN: Integration tests for `OffchainSyncCoordinator.restore()` — the §6.2 + §6.3 + §6.5
// orchestration itself (previously only its pure helpers were tested): chain-anchor
// decrypt → strictly-newer field injection into the local pointer store → three-slot
// restore proceeding off the injected pointers → empty-relay write-back → orchestration
// flags. Chain + relay transport are in-memory fakes; the anchor crypto
// (deriveAnchorKeys / encryptManifest / decryptManifest) and the pointer modules
// (localStorage merge semantics) are REAL.
// CN: `OffchainSyncCoordinator.restore()` 的集成测试——直接覆盖 §6.2+§6.3+§6.5 编排本体
// （此前仅测过其纯函数）：链锚解密 → 严格更新字段注入本地指针存储 → 三槽恢复沿注入指针
// 继续 → 空 Relay 写回 → 编排标志。链与 relay 传输用内存假实现；锚加解密
// （deriveAnchorKeys / encryptManifest / decryptManifest）与指针模块（localStorage 合并
// 语义）为**真实现**。

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

// ---------------------------------------------------------------------------
// EN: Shared mutable fixtures for the hoisted module mocks.
// CN: 供 hoisted 模块 mock 使用的共享可变夹具。
// ---------------------------------------------------------------------------
const h = vi.hoisted(() => ({
  /** relay KV: `${account}:${slot}` -> pointer. */
  relayKv: new Map<string, { cid: string; updated_at: number }>(),
  /** Simulates the relay being unreachable (vs reachable-but-empty). */
  relayDown: { value: false },
  /** The on-chain anchor row returned by `chat_syncAnchor`. */
  chainRow: { value: null as { updatedAt: number; ciphertext: string } | null },
  /** Simulates the chain RPC failing. */
  chainReadError: { value: false },
  /** Simulates a pointer whose IPFS blob is unfetchable (slot restore fails). */
  blobMissing: { contacts: false },
}));

function slotOf(type: string): string {
  if (type.startsWith("contacts")) return "contacts";
  if (type.startsWith("index")) return "index";
  return "archive";
}

vi.mock("@/config", async (importOriginal) => {
  const mod = await importOriginal<typeof import("@/config")>();
  return {
    ...mod, // EN: keep all other named exports (e.g. signingPinBackupActive). CN: 保留其余具名导出。
    config: {
      ...mod.config,
      useMock: false,
      ipfsEnabled: true,
      relayWs: "ws://test-relay",
      syncAnchorTier: "standard" as const,
      syncAnchorPayer: "main" as const,
      contactsVaultEnabled: true,
      convIndexEnabled: true,
      msgArchiveEnabled: true,
    },
  };
});

vi.mock("@/chain/chainClient", () => ({
  chainClient: {
    async syncAnchorOf(_anchorIdHex: string) {
      if (h.chainReadError.value) throw new Error("chain rpc down");
      return h.chainRow.value;
    },
    async genesisHashBytes() {
      return new Uint8Array(32).fill(0x22);
    },
    async publishSyncAnchor() {
      /* not exercised by restore() */
    },
  },
}));

// EN: In-memory relay emulating the *_fetch / *_put one-shot protocol used by the three
// pointer channels. CN: 内存 relay，模拟三个指针通道的 *_fetch / *_put 一次性协议。
vi.mock("@/relay/relayOneShot", () => ({
  async relayOneShotFetch(
    account: string,
    msg: { type: string },
    parse: (m: Record<string, unknown>, _requestId?: string) => unknown,
  ) {
    if (h.relayDown.value) throw new Error("relay down");
    const row = h.relayKv.get(`${account}:${slotOf(msg.type)}`);
    const replyType = msg.type.replace(/_fetch$/, "_reply");
    const reply = row
      ? { type: replyType, cid: row.cid, updated_at: row.updated_at }
      : { type: replyType };
    const parsed = parse(reply, "mock-req-id");
    return parsed === undefined ? null : parsed;
  },
  async relayOneShotSend(account: string, msg: Record<string, unknown>) {
    if (h.relayDown.value) throw new Error("relay down");
    h.relayKv.set(`${account}:${slotOf(String(msg.type))}`, {
      cid: String(msg.cid),
      updated_at: Number(msg.updated_at),
    });
  },
}));

vi.mock("@/wallet/vaultMaster", () => ({
  getVaultMaster: () => new Uint8Array(32).fill(0x11),
}));

// EN: Slot restores reduced to their pointer-visibility contract: succeed iff the merged
// (local + relay) pointer resolves — exactly the seam §6.2 injection must feed.
// CN: 槽位恢复收敛为指针可见性合同：合并（本地 + relay）指针可解析才成功——恰是 §6.2
// 注入必须打通的接缝。
vi.mock("@/store/contactVaultSync", () => ({
  restoreContactsVault: async (account: string) => {
    // EN: blobMissing models a present pointer whose IPFS blob can't be fetched/decrypted.
    // CN: blobMissing 模拟指针存在但 IPFS blob 无法取回/解密。
    if (h.blobMissing.contacts) return false;
    const { fetchContactsPointer } = await import("@/relay/contactsPointer");
    return !!(await fetchContactsPointer(account));
  },
  contactVaultSyncFor: () => ({ push: async () => {} }),
  scheduleContactsVaultPush: () => {},
}));
vi.mock("@/store/convIndexSync", () => ({
  restoreConvIndex: async (account: string) => {
    const { fetchIndexPointer } = await import("@/relay/indexPointer");
    return !!(await fetchIndexPointer(account));
  },
  convIndexSyncFor: () => ({ push: async () => {} }),
  scheduleConvIndexPush: () => {},
}));
vi.mock("@/store/msgArchiveSync", () => ({
  restoreMsgArchive: async (account: string) => {
    const { fetchMsgArchivePointer } = await import("@/relay/msgArchivePointer");
    return !!(await fetchMsgArchivePointer(account));
  },
  msgArchiveSyncFor: () => ({ push: async () => {} }),
  scheduleMsgArchivePush: () => {},
}));

// ---------------------------------------------------------------------------

import { readLocalContactsPointer } from "@/relay/contactsPointer";
import { readLocalIndexPointer } from "@/relay/indexPointer";
import { readLocalMsgArchivePointer } from "@/relay/msgArchivePointer";
import type { LocalStore } from "@/store/localStore";
import { OffchainSyncCoordinator } from "@/store/offchainSyncCoordinator";
import { deriveAnchorKeys, encryptManifest, type SyncManifest } from "@/store/syncAnchor";

const ACCOUNT = "5RestoreTestAccount";
const MASTER = new Uint8Array(32).fill(0x11);

class MemStorage {
  private m = new Map<string, string>();
  getItem(k: string): string | null {
    return this.m.has(k) ? (this.m.get(k) as string) : null;
  }
  setItem(k: string, v: string): void {
    this.m.set(k, String(v));
  }
  removeItem(k: string): void {
    this.m.delete(k);
  }
  clear(): void {
    this.m.clear();
  }
}

(globalThis as Record<string, unknown>).localStorage = new MemStorage();

function toHex(bytes: Uint8Array): string {
  let s = "0x";
  for (const b of bytes) s += b.toString(16).padStart(2, "0");
  return s;
}

const CHAIN_MANIFEST: SyncManifest = {
  v: 1,
  updated_at: 5_000,
  index: { cid: "bafy-chain-index", updated_at: 4_000 },
  contacts: { cid: "bafy-chain-contacts", updated_at: 5_000 },
  archive: { cid: "bafy-chain-archive", updated_at: 3_000 },
};

async function publishChainAnchor(manifest: SyncManifest): Promise<void> {
  const keys = await deriveAnchorKeys(MASTER);
  const ciphertext = await encryptManifest(keys, manifest);
  h.chainRow.value = { updatedAt: manifest.updated_at, ciphertext: toHex(ciphertext) };
}

describe("OffchainSyncCoordinator.restore() (§6.2 + §6.3 + §6.5 integration)", () => {
  let coordinator: OffchainSyncCoordinator;

  beforeEach(() => {
    localStorage.clear();
    h.relayKv.clear();
    h.relayDown.value = false;
    h.chainRow.value = null;
    h.chainReadError.value = false;
    h.blobMissing.contacts = false;
    coordinator = new OffchainSyncCoordinator();
    coordinator.bind(ACCOUNT, {} as unknown as LocalStore);
  });

  afterEach(() => {
    coordinator.unbind();
  });

  it("mnemonic-only device + empty relay: injects chain fields, restores, writes back, sets flags", async () => {
    await publishChainAnchor(CHAIN_MANIFEST);

    const result = await coordinator.restore();

    // EN: restore proceeded off the injected pointers (NOT no_backup).
    // CN: 恢复沿注入指针继续（而非 no_backup）。
    expect(result.phase).toBe("ok");
    expect(result.usedChainAnchor).toBe(true);
    expect(result.needsEpochBump).toBe(true);
    expect(result.message).toContain("已从链上加密锚恢复");

    // §6.2 injection into the local pointer store. / §6.2 注入本地指针存储。
    expect(readLocalIndexPointer(ACCOUNT)).toEqual(CHAIN_MANIFEST.index);
    expect(readLocalContactsPointer(ACCOUNT)).toEqual(CHAIN_MANIFEST.contacts);
    expect(readLocalMsgArchivePointer(ACCOUNT)).toEqual(CHAIN_MANIFEST.archive);

    // §6.3 write-back repopulated the empty relay. / §6.3 写回填充空 relay。
    expect(h.relayKv.get(`${ACCOUNT}:index`)).toEqual(CHAIN_MANIFEST.index);
    expect(h.relayKv.get(`${ACCOUNT}:contacts`)).toEqual(CHAIN_MANIFEST.contacts);
    expect(h.relayKv.get(`${ACCOUNT}:archive`)).toEqual(CHAIN_MANIFEST.archive);
  });

  it("no anchor + empty relay + empty local: no_backup with flags off", async () => {
    const result = await coordinator.restore();

    expect(result.phase).toBe("no_backup");
    expect(result.usedChainAnchor).toBe(false);
    expect(result.needsEpochBump).toBe(false);
    expect(h.relayKv.size).toBe(0);
  });

  it("relay newer than chain: nothing injected, no epoch-bump prompt, relay untouched", async () => {
    await publishChainAnchor({
      v: 1,
      updated_at: 1_000,
      contacts: { cid: "bafy-chain-old", updated_at: 1_000 },
    });
    const relayPtr = { cid: "bafy-relay-new", updated_at: 2_000 };
    h.relayKv.set(`${ACCOUNT}:contacts`, relayPtr);

    const result = await coordinator.restore();

    // EN: only the contacts slot resolves → partial; chain contributed nothing.
    // CN: 仅 contacts 槽可解析→partial；链未贡献任何字段。
    expect(result.phase).toBe("partial");
    expect(result.usedChainAnchor).toBe(false);
    expect(result.needsEpochBump).toBe(false);
    expect(readLocalContactsPointer(ACCOUNT)).toBeNull();
    expect(h.relayKv.get(`${ACCOUNT}:contacts`)).toEqual(relayPtr);
  });

  it("chain read failure degrades gracefully to relay-only restore", async () => {
    h.chainReadError.value = true;
    h.relayKv.set(`${ACCOUNT}:contacts`, { cid: "bafy-relay", updated_at: 1_000 });

    const result = await coordinator.restore();

    expect(result.phase).toBe("partial");
    expect(result.usedChainAnchor).toBe(false);
    expect(result.needsEpochBump).toBe(false);
  });

  it("unreachable relay: chain anchor still restores and needsEpochBump stays true despite failed write-back", async () => {
    await publishChainAnchor(CHAIN_MANIFEST);
    h.relayDown.value = true;

    const result = await coordinator.restore();

    // EN: fetch* errors fall back to the injected local pointers → restore proceeds;
    // the write-back fails but the replay-window warning must NOT be suppressed
    // (regression guard for the `usedChainAnchor && wroteBack` gate).
    // CN: fetch* 出错回退到已注入的本地指针→恢复继续；写回失败但重放窗口提示**不得**
    // 被吞掉（针对旧 `usedChainAnchor && wroteBack` 门控的回归守护）。
    expect(result.phase).toBe("ok");
    expect(result.usedChainAnchor).toBe(true);
    expect(result.needsEpochBump).toBe(true);
    expect(h.relayKv.size).toBe(0);
  });

  it("idempotent re-run: second restore injects nothing and clears the prompt", async () => {
    await publishChainAnchor(CHAIN_MANIFEST);
    await coordinator.restore();

    const second = await coordinator.restore();

    // EN: local + relay now match the chain → chain is no longer strictly newer.
    // CN: 本地 + relay 已与链一致→链不再严格更新。
    expect(second.phase).toBe("ok");
    expect(second.usedChainAnchor).toBe(false);
    expect(second.needsEpochBump).toBe(false);
  });

  it("§6.3 anti-dangling: a field whose blob is unfetchable is NOT written back to the relay", async () => {
    await publishChainAnchor(CHAIN_MANIFEST);
    // EN: contacts pointer injects from chain but its blob can't be restored this run.
    // CN: contacts 指针从链上注入，但本次其 blob 无法恢复。
    h.blobMissing.contacts = true;

    const result = await coordinator.restore();

    // EN: contacts slot failed → partial; index/archive succeeded. CN: contacts 槽失败→partial。
    expect(result.phase).toBe("partial");
    expect(result.contacts).toBe(false);

    // EN: verified fields are advertised to the relay; the unverified one is withheld.
    // CN: 已校验字段写回 relay；未校验的 contacts 被扣留。
    expect(h.relayKv.get(`${ACCOUNT}:index`)).toEqual(CHAIN_MANIFEST.index);
    expect(h.relayKv.get(`${ACCOUNT}:archive`)).toEqual(CHAIN_MANIFEST.archive);
    expect(h.relayKv.get(`${ACCOUNT}:contacts`)).toBeUndefined();
  });

  it("re-entrancy guard: concurrent restore() calls coalesce onto one in-flight run", async () => {
    await publishChainAnchor(CHAIN_MANIFEST);

    const p1 = coordinator.restore();
    const p2 = coordinator.restore();
    // EN: same in-flight promise → no duplicate run racing on pointers / write-back.
    // CN: 同一进行中 promise → 不会有重复运行在指针/写回上竞争。
    expect(p1).toBe(p2);

    const [r1, r2] = await Promise.all([p1, p2]);
    expect(r1).toBe(r2);
    expect(r1.phase).toBe("ok");

    // EN: once settled, a fresh call starts a new run. CN: 完成后新的调用开启新运行。
    const p3 = coordinator.restore();
    expect(p3).not.toBe(p1);
    await p3;
  });
});
