// EN: LIVE end-to-end test for the EISA recovery loop (ADR CHAT_SYNC_ANCHOR §12
// acceptance): against a running `nexus-node --dev`, "device A" derives the anchor from a
// fresh suri, publishes the encrypted manifest on-chain; "device B" re-derives EVERYTHING
// from the suri alone (no shared state), locates the anchor via `chat_syncAnchor`, decrypts
// and gets the identical manifest — the mnemonic-only + empty-relay recovery closure. Also
// proves a non-anchor-key signature cannot publish, and that clear_sync_anchor removes the
// record and refunds the deposit. Gated behind CHAIN_E2E=1 + VITE_USE_MOCK=false (same as
// chainCoordinator.e2e); never runs in normal CI.
// CN: EISA 恢复闭环的**实时**端到端测试（ADR CHAT_SYNC_ANCHOR §12 验收）：对运行中的
// `nexus-node --dev`，「设备 A」从全新 suri 派生锚并把加密清单上链；「设备 B」仅凭同一
// suri 重新派生全部材料（无共享状态），经 `chat_syncAnchor` 定位锚、解密得到完全一致的
// 清单——助记词 + 空 Relay 的恢复闭环。同时验证非锚密钥签名无法发布、clear_sync_anchor
// 删除记录并退还押金。用 CHAIN_E2E=1 + VITE_USE_MOCK=false 门控（与 chainCoordinator.e2e
// 一致），正常 CI 不触发。

import { describe, expect, it } from "vitest";

import { ChainClient } from "@/chain/chainClient";
import {
  buildClearPayload,
  buildPublishPayload,
  decryptManifest,
  deriveAnchorKeys,
  encryptManifest,
  signAnchorPayload,
  type SyncManifest,
} from "@/store/syncAnchor";
import { deriveSyncPayerPair, payerTopUpAmount } from "@/store/syncAnchorPayer";
import { deriveVaultMasterFromSuri } from "@/wallet/vaultMaster";

const RUN = process.env.CHAIN_E2E === "1";
const STAMP = Date.now();
const USER = `//nexchat-sync-e2e-${STAMP}`;
const BOOTSTRAP = 5_000_000_000_000n; // 5 NEX: 0.5 deposit + fees

function toHex(bytes: Uint8Array): string {
  let s = "0x";
  for (const b of bytes) s += b.toString(16).padStart(2, "0");
  return s;
}

function fromHex(hex: string): Uint8Array {
  const clean = hex.startsWith("0x") ? hex.slice(2) : hex;
  const out = new Uint8Array(clean.length / 2);
  for (let i = 0; i < out.length; i++) out[i] = parseInt(clean.slice(i * 2, i * 2 + 2), 16);
  return out;
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

describe.runIf(RUN)("EISA sync anchor recovery loop (live node)", () => {
  it(
    "mnemonic-only device recovers the manifest; foreign sig rejected; clear refunds deposit",
    async () => {
      expect(await nodeUp()).toBe(true);

      // EN: faucet endows the fresh payer (deposit + fees). CN: 水龙头给新付费账户充值。
      const faucet = new ChainClient();
      await faucet.useDevAccount("//Alice");
      const payerAddr = await faucet.deriveAddress(USER);
      await faucet.signAndSendDev("balances", "transferKeepAlive", [payerAddr, BOOTSTRAP]);

      // ---- Device A: derive from the suri, publish the encrypted manifest ----
      const deviceA = new ChainClient();
      await deviceA.useDevAccount(USER);
      const masterA = await deriveVaultMasterFromSuri(USER);
      const keysA = await deriveAnchorKeys(masterA);
      const manifest: SyncManifest = {
        v: 1,
        updated_at: Date.now(),
        index: { cid: `bafy-e2e-index-${STAMP}`, updated_at: Date.now() - 1000 },
        contacts: { cid: `bafy-e2e-contacts-${STAMP}`, updated_at: Date.now() },
      };
      const ciphertext = await encryptManifest(keysA, manifest);
      const genesis = await deviceA.genesisHashBytes();
      const sig = await signAnchorPayload(
        keysA,
        await buildPublishPayload(genesis, keysA.anchorId, manifest.updated_at, ciphertext),
      );
      await deviceA.publishSyncAnchor(
        toHex(keysA.anchorPk),
        manifest.updated_at,
        toHex(ciphertext),
        toHex(sig),
      );

      // ---- Device B: ONLY the suri — re-derive, locate, decrypt ----
      const deviceB = new ChainClient();
      const masterB = await deriveVaultMasterFromSuri(USER);
      const keysB = await deriveAnchorKeys(masterB);
      expect(toHex(keysB.anchorId)).toBe(toHex(keysA.anchorId));
      const row = await deviceB.syncAnchorOf(toHex(keysB.anchorId));
      expect(row).not.toBeNull();
      expect(row!.updatedAt).toBe(manifest.updated_at);
      const recovered = await decryptManifest(keysB, fromHex(row!.ciphertext));
      expect(recovered).toEqual(manifest);

      // ---- Authorization: a foreign anchor key cannot overwrite the anchor ----
      const foreignKeys = await deriveAnchorKeys(
        await deriveVaultMasterFromSuri(`${USER}-attacker`),
      );
      const forgedTs = manifest.updated_at + 60_000;
      const forgedCt = await encryptManifest(foreignKeys, manifest);
      const forgedSig = await signAnchorPayload(
        foreignKeys, // signs correctly, but for ITS OWN pk — submitted under keysA.anchorPk
        await buildPublishPayload(genesis, keysA.anchorId, forgedTs, forgedCt),
      );
      await expect(
        deviceA.publishSyncAnchor(
          toHex(keysA.anchorPk),
          forgedTs,
          toHex(forgedCt),
          toHex(forgedSig),
        ),
      ).rejects.toThrow(/BadAnchorSignature/);

      // ---- Clear: signature binds stored.updated_at; deposit refunded ----
      const freeBefore = await deviceA.freeBalance(payerAddr);
      const clearSig = await signAnchorPayload(
        keysA,
        buildClearPayload(genesis, keysA.anchorId, row!.updatedAt),
      );
      await deviceA.clearSyncAnchor(toHex(keysA.anchorPk), toHex(clearSig));
      expect(await deviceB.syncAnchorOf(toHex(keysB.anchorId))).toBeNull();
      const freeAfter = await deviceA.freeBalance(payerAddr);
      // EN: unreserve(0.5 NEX) dwarfs the clear fee — free balance must increase.
      // CN: 解除 0.5 NEX 押金远大于 clear 手续费——free 余额必须增加。
      expect(freeAfter > freeBefore).toBe(true);
    },
    120_000,
  );

  it(
    "v2 burner payer (P3): main account funds the derived payer, the payer pays publish + clear",
    async () => {
      expect(await nodeUp()).toBe(true);
      const SUI = `${USER}-burner`;

      const faucet = new ChainClient();
      await faucet.useDevAccount("//Alice");
      const mainAddr = await faucet.deriveAddress(SUI);
      await faucet.signAndSendDev("balances", "transferKeepAlive", [mainAddr, BOOTSTRAP]);

      // EN: main account tops up the deterministic burner payer (the real coordinator
      // flow). CN: 主账户给确定性 burner payer 充值（即 coordinator 的真实流程）。
      const main = new ChainClient();
      await main.useDevAccount(SUI);
      const master = await deriveVaultMasterFromSuri(SUI);
      const payer = await deriveSyncPayerPair(master);
      const topUp = payerTopUpAmount(await main.freeBalance(payer.address));
      expect(topUp > 0n).toBe(true);
      await main.signAndSend("balances", "transferKeepAlive", [payer.address, topUp]);

      // EN: publish signed by the PAYER — the main account leaves no per-publish trail.
      // CN: 由 PAYER 签名发布——主账户不留逐次 publish 痕迹。
      const keys = await deriveAnchorKeys(master);
      const manifest: SyncManifest = {
        v: 1,
        updated_at: Date.now(),
        index: { cid: `bafy-e2e-burner-${STAMP}`, updated_at: Date.now() },
      };
      const ciphertext = await encryptManifest(keys, manifest);
      const genesis = await main.genesisHashBytes();
      const sig = await signAnchorPayload(
        keys,
        await buildPublishPayload(genesis, keys.anchorId, manifest.updated_at, ciphertext),
      );
      await main.publishSyncAnchor(
        toHex(keys.anchorPk),
        manifest.updated_at,
        toHex(ciphertext),
        toHex(sig),
        payer,
      );

      const row = await main.syncAnchorOf(toHex(keys.anchorId));
      expect(row).not.toBeNull();
      expect(await decryptManifest(keys, fromHex(row!.ciphertext))).toEqual(manifest);

      // EN: deposit was reserved from the payer and is refunded to the payer on clear.
      // CN: 押金从 payer reserve，clear 时退回 payer。
      const payerFreeBefore = await main.freeBalance(payer.address);
      const clearSig = await signAnchorPayload(
        keys,
        buildClearPayload(genesis, keys.anchorId, row!.updatedAt),
      );
      await main.clearSyncAnchor(toHex(keys.anchorPk), toHex(clearSig), payer);
      expect(await main.syncAnchorOf(toHex(keys.anchorId))).toBeNull();
      expect((await main.freeBalance(payer.address)) > payerFreeBefore).toBe(true);
    },
    120_000,
  );
});
