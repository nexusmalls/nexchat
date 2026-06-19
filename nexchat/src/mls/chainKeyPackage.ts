// EN: Publish OpenMLS KeyPackage to chain (chatGroup.publish_key_package).
// CN: 向链上发布 OpenMLS KeyPackage。

import type { ChainClient } from "@/chain/chainClient";
import { isPoolConflictError } from "@/chain/txErrors";
import { hex } from "@/mls/chainBytes";
import type { OpenMlsEngine } from "@/mls/openMlsEngine";
import { canonicalAddress } from "@/wallet/address";

const kpPublishInflight = new Map<string, Promise<void>>();

const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));

async function waitForKeyPackages(
  chain: ChainClient,
  who: string,
  timeoutMs = 60_000,
): Promise<boolean> {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    if ((await chain.keyPackagesOf(who)).length > 0) return true;
    await sleep(2000);
  }
  return false;
}

/// EN: Ensure at least `minPool` KeyPackages are published for `who`. Each published KP is a
/// one-shot pre-key consumed when a peer adds `who` to a group (incl. 1:1 owner bootstrap via
/// on-chain KP), so keeping a small pool lets several peers add `who` while offline.
/// CN: 确保账户至少有 `minPool` 个链上 KeyPackage。每个 KP 为一次性预共享公钥包，对端把 `who`
/// 加入群（含 1:1 owner 用链上 KP 引导）时被消费，故保留小池可让多个对端在 `who` 离线时加人。
export async function ensureChainKeyPackagePublished(
  engine: OpenMlsEngine,
  chain: ChainClient,
  who: string,
  minPool = 1,
): Promise<void> {
  const addr = canonicalAddress(who);
  // EN: Track A read-only escrow devices cannot mint KeyPackages (no signing key). CN: 路线 A 只读托管
  // 设备无法生成 KeyPackage（无签名钥）。
  if (!engine.canExportEscrow()) return;
  const target = Math.max(1, minPool);
  const inflight = kpPublishInflight.get(addr);
  if (inflight) return inflight;

  const job = (async () => {
    let have = (await chain.keyPackagesOf(addr)).length;
    if (have >= target) return;

    // EN: Only rotate stale ids when starting from an empty pool (keep existing KPs otherwise).
    // CN: 仅在池为空时轮换旧 id（否则保留已有 KP）。
    if (have === 0) {
      const stale = await chain.keyPackageIdsOf(addr);
      for (const id of stale) {
        try {
          await chain.signAndSend("chatGroup", "revokeKeyPackage", [id]);
        } catch (e) {
          if (!isPoolConflictError(e)) {
            console.warn("[nexchat] revokeKeyPackage skipped:", id, e);
          }
        }
      }
      have = (await chain.keyPackagesOf(addr)).length;
    }

    while (have < target) {
      const kp = engine.generateKeyPackage();
      try {
        await chain.signAndSend("chatGroup", "publishKeyPackage", [hex(kp)]);
        have += 1;
      } catch (e) {
        if (isPoolConflictError(e)) {
          if (await waitForKeyPackages(chain, addr)) return;
          console.warn("[nexchat] publishKeyPackage already pending in pool; will retry later");
          return;
        }
        throw e;
      }
    }
  })();

  kpPublishInflight.set(addr, job);
  try {
    await job;
  } finally {
    kpPublishInflight.delete(addr);
  }
}
