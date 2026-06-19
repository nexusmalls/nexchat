// EN: LIVE full-path E2E for the §6.5 data-layer login self-heal (ADR CHAT_SYNC_ANCHOR
// §12 acceptance, checklist F-5): against a running `nexus-node --dev`, "device A"
// publishes the encrypted manifest on-chain; "device B" starts from NOTHING but the suri
// (fresh localStorage, EMPTY relay) and runs `coordinator.restore()` — the real
// orchestration path: live `chat_syncAnchor` read → decrypt → §6.2 injection → restore →
// §6.3 relay write-back → §6.5 flags. Only the relay transport and the blob-level slot
// restores are faked (no IPFS daemon in this harness); chain, crypto and pointer
// semantics are real. Gated behind CHAIN_E2E=1; never runs in normal CI.
// CN: §6.5 数据层登录自愈的**实时**全路径 E2E（ADR §12 验收，checklist F-5）：对运行中的
// `nexus-node --dev`，「设备 A」把加密清单上链；「设备 B」仅凭 suri 起步（全新
// localStorage、**空 relay**）执行 `coordinator.restore()`——真实编排路径：实链
// `chat_syncAnchor` 读取 → 解密 → §6.2 注入 → 恢复 → §6.3 写回 relay → §6.5 标志。
// 仅 relay 传输与 blob 级槽位恢复为假实现（本测试框架无 IPFS 守护进程）；链、加解密与
// 指针语义均为真。CHAIN_E2E=1 门控，正常 CI 不触发。

import { afterAll, describe, expect, it, vi } from "vitest";

const h = vi.hoisted(() => ({
  relayKv: new Map<string, { cid: string; updated_at: number }>(),
}));

function slotOf(type: string): string {
  if (type.startsWith("contacts")) return "contacts";
  if (type.startsWith("index")) return "index";
  return "archive";
}

vi.mock("@/config", async (importOriginal) => {
  const mod = await importOriginal<typeof import("@/config")>();
  return {
    config: {
      ...mod.config,
      useMock: false,
      ipfsEnabled: true,
      relayWs: "ws://e2e-fake-relay",
      syncAnchorTier: "standard" as const,
      syncAnchorPayer: "main" as const,
      contactsVaultEnabled: true,
      convIndexEnabled: true,
      msgArchiveEnabled: true,
    },
  };
});

vi.mock("@/relay/relayOneShot", () => ({
  async relayOneShotFetch(
    account: string,
    msg: { type: string },
    parse: (m: Record<string, unknown>, _requestId?: string) => unknown,
  ) {
    const row = h.relayKv.get(`${account}:${slotOf(msg.type)}`);
    const replyType = msg.type.replace(/_fetch$/, "_reply");
    const reply = row
      ? { type: replyType, cid: row.cid, updated_at: row.updated_at }
      : { type: replyType };
    const parsed = parse(reply, "mock-req-id");
    return parsed === undefined ? null : parsed;
  },
  async relayOneShotSend(account: string, msg: Record<string, unknown>) {
    h.relayKv.set(`${account}:${slotOf(String(msg.type))}`, {
      cid: String(msg.cid),
      updated_at: Number(msg.updated_at),
    });
  },
}));

vi.mock("@/store/contactVaultSync", () => ({
  restoreContactsVault: async (account: string) => {
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

import { ChainClient } from "@/chain/chainClient";
import { readLocalContactsPointer } from "@/relay/contactsPointer";
import { readLocalIndexPointer } from "@/relay/indexPointer";
import type { LocalStore } from "@/store/localStore";
import { OffchainSyncCoordinator } from "@/store/offchainSyncCoordinator";
import {
  buildPublishPayload,
  deriveAnchorKeys,
  encryptManifest,
  signAnchorPayload,
  type SyncManifest,
} from "@/store/syncAnchor";
import { clearVaultMaster, deriveVaultMasterFromSuri, setVaultMaster } from "@/wallet/vaultMaster";

const RUN = process.env.CHAIN_E2E === "1";
const STAMP = Date.now();
const USER = `//nexchat-sync-restore-e2e-${STAMP}`;
const BOOTSTRAP = 5_000_000_000_000n; // 5 NEX: 0.5 deposit + fees

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

async function nodeUp(): Promise<boolean> {
  try {
    const res = await fetch("http://127.0.0.1:9944", {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ jsonrpc: "2.0", id: 1, method: "system_chain", params: [] }),
    });
    return res.ok;
  } catch {
    return false;
  }
}

describe.runIf(RUN)("EISA data-layer login self-heal (live node, §6.5 full path)", () => {
  afterAll(() => clearVaultMaster());

  it(
    "device B with suri only + empty relay: coordinator.restore() injects, restores, writes back, flags epoch bump",
    async () => {
      expect(await nodeUp()).toBe(true);

      // ---- Faucet: endow the payer used by device A. / 水龙头给设备 A 付费账户充值。
      const faucet = new ChainClient();
      await faucet.useDevAccount("//Alice");
      const payerAddr = await faucet.deriveAddress(USER);
      await faucet.signAndSendDev("balances", "transferKeepAlive", [payerAddr, BOOTSTRAP]);

      // ---- Device A: publish the encrypted manifest on-chain. / 设备 A：加密清单上链。
      const deviceA = new ChainClient();
      await deviceA.useDevAccount(USER);
      const master = await deriveVaultMasterFromSuri(USER);
      const keys = await deriveAnchorKeys(master);
      const manifest: SyncManifest = {
        v: 1,
        updated_at: Date.now(),
        index: { cid: `bafy-e2e-index-${STAMP}`, updated_at: Date.now() - 2_000 },
        contacts: { cid: `bafy-e2e-contacts-${STAMP}`, updated_at: Date.now() - 1_000 },
        archive: { cid: `bafy-e2e-archive-${STAMP}`, updated_at: Date.now() },
      };
      const ciphertext = await encryptManifest(keys, manifest);
      const genesis = await deviceA.genesisHashBytes();
      const sig = await signAnchorPayload(
        keys,
        await buildPublishPayload(genesis, keys.anchorId, manifest.updated_at, ciphertext),
      );
      await deviceA.publishSyncAnchor(
        toHex(keys.anchorPk),
        manifest.updated_at,
        toHex(ciphertext),
        toHex(sig),
      );

      // ---- Device B: suri-only fresh state + EMPTY relay. / 设备 B：仅 suri + 空 relay。
      localStorage.clear();
      h.relayKv.clear();
      setVaultMaster(await deriveVaultMasterFromSuri(USER));

      const coordinator = new OffchainSyncCoordinator();
      coordinator.bind("e2e-restore-account", {} as unknown as LocalStore);
      try {
        const result = await coordinator.restore();

        // §6.5 flags + §6.2 injection + restore continuation.
        expect(result.phase).toBe("ok");
        expect(result.usedChainAnchor).toBe(true);
        expect(result.needsEpochBump).toBe(true);
        expect(result.message).toContain("已从链上加密锚恢复");
        expect(readLocalIndexPointer("e2e-restore-account")).toEqual(manifest.index);
        expect(readLocalContactsPointer("e2e-restore-account")).toEqual(manifest.contacts);

        // §6.3 write-back repopulated the empty relay. / §6.3 写回填充空 relay。
        expect(h.relayKv.get("e2e-restore-account:index")).toEqual(manifest.index);
        expect(h.relayKv.get("e2e-restore-account:contacts")).toEqual(manifest.contacts);
        expect(h.relayKv.get("e2e-restore-account:archive")).toEqual(manifest.archive);

        // Idempotent second run: chain no longer strictly newer → no prompt.
        // 幂等复跑：链不再严格更新→不再提示。
        const second = await coordinator.restore();
        expect(second.usedChainAnchor).toBe(false);
        expect(second.needsEpochBump).toBe(false);
      } finally {
        coordinator.unbind();
      }
    },
    120_000,
  );
});
