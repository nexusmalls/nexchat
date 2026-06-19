// EN: Track A PIN-wrapped signing-key backup sync (design §5.3 path C, P0). Producer: primary seals
// `exportSigningKeys()` under K_pin_wrap → IPFS → `mls_signing` pointer. Consumer: fetch blob + PIN
// → plaintext for `installSigningKeys` (P2). CN: 路线 A PIN 包裹签名钥备份同步（设计 §5.3 路径 C，P0）。
// 生产者：主设备用 K_pin_wrap 密封 `exportSigningKeys()` → IPFS → `mls_signing` 指针。消费者：取 blob +
// PIN → 明文供 `installSigningKeys`（P2）。

import { signingPinBackupActive } from "@/config";
import { ipfsClient } from "@/ipfs/ipfsClient";
import {
  buildSigningBackupPlain,
  deriveMlsSigningPinWrapKey,
  normalizeSigningPin,
  openSigningBackup,
  sealSigningBackup,
  signingBundleFromPlain,
  type SigningBackupPlain,
} from "@/mls/signingPinBackup";
import { deriveDeviceDirectoryKey, type HandoffReceipt } from "@/mls/sendingAuthority";
import { fetchLatestReceipt, publishHandoff } from "@/mls/handoffCoordinator";
import {
  fetchMlsSigningPointer,
  publishMlsSigningPointer,
  readLocalMlsSigningPointer,
} from "@/relay/mlsSigningPointer";
import { deriveAnchorKeys } from "@/store/syncAnchor";
import type { SyncPointer } from "@/store/syncAnchor";
import { getVaultMaster } from "@/wallet/vaultMaster";

function enabled(): boolean {
  return signingPinBackupActive();
}

/// EN: Seal + upload + publish a signing-key backup. Returns the pointer written. CN: 密封 + 上传 +
/// 发布签名钥备份；返回写入的指针。
export async function pushSigningPinBackup(args: {
  account: string;
  deviceId: string;
  backupSeq: number;
  pin: string;
  signingBundle: Uint8Array;
}): Promise<SyncPointer> {
  if (!enabled()) throw new Error("PIN 签名备份未启用");
  const master = getVaultMaster();
  if (!master) throw new Error("需要已解锁的钱包（vault_master）");
  const pinNorm = normalizeSigningPin(args.pin);
  const { anchorId } = await deriveAnchorKeys(master);
  const key = await deriveMlsSigningPinWrapKey(master, anchorId, pinNorm);
  const plain = buildSigningBackupPlain({
    account: args.account,
    deviceId: args.deviceId,
    backupSeq: args.backupSeq,
    bundle: args.signingBundle,
  });
  const packed = await sealSigningBackup(key, plain);
  const cid = await ipfsClient.add(packed, "mls-signing-backup.enc");
  const ptr: SyncPointer = { cid, updated_at: args.backupSeq };
  await publishMlsSigningPointer(args.account, ptr);
  return ptr;
}

/// EN: Whether a signing backup pointer exists for `account` (no PIN required). CN: `account` 是否存在签名备份指针（无需 PIN）。
export async function hasSigningPinBackup(account: string): Promise<boolean> {
  if (!enabled()) return false;
  const ptr = await fetchMlsSigningPointer(account);
  return !!ptr?.cid;
}

/// EN: After PIN restore, decide if we must mint a new handoff receipt to claim authority on this
/// device (when the latest receipt names another device). Pure. CN: PIN 恢复后，若最新收据指向其它设备，
/// 是否需铸造新交接收据以在本设备认领发送权。纯函数。
export function handoffTransferAfterPinRestore(args: {
  latestReceipt: HandoffReceipt | null;
  selfDeviceId: string;
}): { from: string; to: string } | null {
  if (!args.latestReceipt) return null;
  if (args.latestReceipt.to === args.selfDeviceId) return null;
  return { from: args.latestReceipt.to, to: args.selfDeviceId };
}

/// EN: Offline restore (design §5.3 path C): decrypt backup → install signing keys → claim handoff
/// authority when needed. CN: 离线恢复（设计 §5.3 路径 C）：解密备份 → 装入签名钥 → 必要时认领交接发送权。
export async function restoreSigningPinBackup(args: {
  account: string;
  selfDeviceId: string;
  pin: string;
  engine: { installSigningKeys(bundle: Uint8Array): void };
}): Promise<{ claimedAuthority: boolean }> {
  if (!enabled()) throw new Error("PIN 签名备份未启用");
  const plain = await fetchSigningPinBackup({ account: args.account, pin: args.pin });
  if (!plain) throw new Error("未找到 PIN 备份或 PIN 错误");
  if (plain.account !== args.account) throw new Error("备份与当前账户不匹配");

  const master = getVaultMaster();
  if (!master) throw new Error("需要已解锁的钱包（vault_master）");

  args.engine.installSigningKeys(signingBundleFromPlain(plain));

  const dir = await deriveDeviceDirectoryKey(master);
  const latest = await fetchLatestReceipt(args.account, dir.publicKey);
  const transfer = handoffTransferAfterPinRestore({
    latestReceipt: latest?.receipt ?? null,
    selfDeviceId: args.selfDeviceId,
  });
  if (transfer) {
    await publishHandoff({
      account: args.account,
      dir,
      from: transfer.from,
      to: transfer.to,
    });
  }
  return { claimedAuthority: !!transfer };
}

/// EN: Fetch + decrypt the newest signing backup for `account`. CN: 取回并解密 `account` 的最新签名备份。
export async function fetchSigningPinBackup(args: {
  account: string;
  pin: string;
}): Promise<SigningBackupPlain | null> {
  if (!enabled()) return null;
  const ptr = await fetchMlsSigningPointer(args.account);
  if (!ptr) return null;
  const master = getVaultMaster();
  if (!master) throw new Error("需要已解锁的钱包（vault_master）");
  const pinNorm = normalizeSigningPin(args.pin);
  const { anchorId } = await deriveAnchorKeys(master);
  const key = await deriveMlsSigningPinWrapKey(master, anchorId, pinNorm);
  const packed = await ipfsClient.cat(ptr.cid);
  return openSigningBackup(key, packed);
}

/// EN: Monotone backup seq for the next publish (max(prev, now) + 1). CN: 下次发布的单调 backup seq。
export function nextSigningBackupSeq(prev: SyncPointer | null): number {
  const now = Date.now();
  const last = prev?.updated_at ?? 0;
  return Math.max(last + 1, now);
}

export { readLocalMlsSigningPointer };
