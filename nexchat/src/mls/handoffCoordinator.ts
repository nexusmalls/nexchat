// EN: Track A sending-authority handoff coordinator (design CHAT_MULTIDEVICE_MLS_SYNC §5.2/§5.4). Ties
// the pure crypto/arithmetic of `sendingAuthority` to the relay `handoff` pointer channel. The wire
// envelope is `base64(JSON{ receipt, sig })` carried in the pointer `cid`, with the monotone handoff
// `seq` carried in `updated_at` (so relay/local LWW == seq ordering). Responsibilities here: encode/
// decode the envelope, VERIFY the directory-key signature on fetch (the relay does not), and expose
// the authority decision (`resolveAuthority`) + the mint-and-publish path (`publishHandoff`). The
// signing-KEY transfer itself (encrypted to the target device peer key, §5.2 step 2) rides the
// existing account self-channel and is a separate concern from this authority-pointer plane.
// CN: 路线 A 发送权交接协调器（设计 §5.2/§5.4）。把 `sendingAuthority` 的纯密码学/算术接到 relay `handoff`
// 指针通道。线信封为 `base64(JSON{ receipt, sig })`，置于指针 `cid`；单调交接 `seq` 置于 `updated_at`
// （故 relay/本地 LWW == seq 排序）。本模块职责：编解码信封、取回时**验证**目录钥签名（relay 不验）、暴露
// 权威决策（`resolveAuthority`）与铸券发布路径（`publishHandoff`）。签名**钥**本身的传输（加密给目标设备
// 对端钥，§5.2 步骤 2）走既有账户自通道，与本权威指针面是分开的关注点。

import { b64ToBytes, bytesToB64 } from "@/delivery/b64";
import {
  openSealed,
  sealToPeer,
  verifyDevicePeerEndorsement,
  type DevicePeerEndorsement,
  type SealedHandoffPayload,
} from "@/mls/devicePeerKey";
import {
  buildNextReceipt,
  resolveAuthoritativeDevice,
  signHandoffReceipt,
  verifyHandoffReceipt,
  type DeviceDirectoryKey,
  type HandoffReceipt,
  type SignedHandoffReceipt,
} from "@/mls/sendingAuthority";
import {
  fetchHandoffPointer,
  publishHandoffPointer,
  readLocalHandoffPointer,
} from "@/relay/handoffPointer";
import type { SyncPointer } from "@/store/syncAnchor";

/// EN: Minimal engine surface the handoff needs (the real `openMlsEngine` satisfies it). Injecting an
/// interface keeps this orchestration unit-testable without the wasm engine. CN: 交接所需的最小引擎
/// 接口（真实 `openMlsEngine` 满足之）。注入接口使本编排无需 wasm 引擎即可单测。
export interface SigningKeyTransfer {
  exportSigningKeys(): Uint8Array;
  installSigningKeys(bundle: Uint8Array): void;
}

function fromHex(hex: string): Uint8Array {
  const c = hex.startsWith("0x") ? hex.slice(2) : hex;
  const out = new Uint8Array(c.length / 2);
  for (let i = 0; i < out.length; i++) out[i] = parseInt(c.slice(i * 2, i * 2 + 2), 16);
  return out;
}

const enc = new TextEncoder();
const dec = new TextDecoder();

/// EN: Encode a signed receipt into the opaque pointer `cid` payload. CN: 把签名收据编码为不透明指针
/// `cid` 载荷。
export function encodeHandoffEnvelope(signed: SignedHandoffReceipt): string {
  return bytesToB64(enc.encode(JSON.stringify(signed)));
}

/// EN: Decode the pointer `cid` payload back into a signed receipt; null on any shape error (never
/// throws). CN: 把指针 `cid` 载荷解码回签名收据；形状错误返回 null（绝不抛错）。
export function decodeHandoffEnvelope(cid: string): SignedHandoffReceipt | null {
  try {
    const obj = JSON.parse(dec.decode(b64ToBytes(cid))) as Partial<SignedHandoffReceipt>;
    const r = obj.receipt as Partial<HandoffReceipt> | undefined;
    if (
      !r ||
      r.v !== 1 ||
      typeof r.from !== "string" ||
      typeof r.to !== "string" ||
      typeof r.seq !== "number" ||
      typeof r.ts !== "number" ||
      typeof obj.sig !== "string"
    ) {
      return null;
    }
    return { receipt: { v: 1, from: r.from, to: r.to, seq: r.seq, ts: r.ts }, sig: obj.sig };
  } catch {
    return null;
  }
}

/// EN: Mint the next receipt handing authority `from → to`, sign it with the device-directory key,
/// and publish it to the relay handoff channel (monotone by seq). Returns the published receipt.
/// CN: 铸造把发送权从 `from → to` 的下一张收据，用设备目录钥签名，并发布到 relay 交接通道（按 seq 单调）。
/// 返回已发布收据。
export async function publishHandoff(args: {
  account: string;
  dir: DeviceDirectoryKey;
  from: string;
  to: string;
  now?: number;
}): Promise<SignedHandoffReceipt> {
  const latest = await fetchLatestReceipt(args.account, args.dir.publicKey);
  const receipt = buildNextReceipt({
    from: args.from,
    to: args.to,
    latest: latest?.receipt ?? null,
    now: args.now ?? Date.now(),
  });
  const sig = await signHandoffReceipt(args.dir, receipt);
  const ptr: SyncPointer = { cid: encodeHandoffEnvelope({ receipt, sig }), updated_at: receipt.seq };
  await publishHandoffPointer(args.account, ptr);
  return { receipt, sig };
}

/// EN: Fetch the newest handoff envelope and return it ONLY if its signature verifies against the
/// account device-directory public key (a forged/tampered receipt is discarded → null). CN: 取最新
/// 交接信封，仅当其签名能用账户设备目录公钥验证时返回（伪造/篡改收据丢弃 → null）。
export async function fetchLatestReceipt(
  account: string,
  dirPublicKey: Uint8Array,
): Promise<SignedHandoffReceipt | null> {
  const ptr = await fetchHandoffPointer(account);
  if (!ptr) return null;
  const signed = decodeHandoffEnvelope(ptr.cid);
  if (!signed) return null;
  const ok = await verifyHandoffReceipt(dirPublicKey, signed);
  return ok ? signed : null;
}

/// EN: Resolve which device currently holds sending authority: the verified latest receipt's `to`,
/// falling back to `primaryDeviceId` (§5.1 bootstrap) when no valid receipt exists. CN: 裁定当前持
/// 发送权设备：已验证的最新收据 `to`，无有效收据时回退 `primaryDeviceId`（§5.1 引导）。
export async function resolveAuthority(args: {
  account: string;
  dirPublicKey: Uint8Array;
  primaryDeviceId: string | null;
}): Promise<string | null> {
  const latest = await fetchLatestReceipt(args.account, args.dirPublicKey);
  return resolveAuthoritativeDevice(latest?.receipt ?? null, args.primaryDeviceId);
}

/// EN: Local-only fast read of the last handoff receipt this device observed (no relay round-trip);
/// used by the send guard for an offline-tolerant authority hint. NOT signature-verified — only the
/// locally cached envelope, which this device itself wrote. CN: 仅本地快速读取本设备观察到的最近交接
/// 收据（不走 relay）；供发送守卫做容忍离线的权威提示。未验签——仅本设备自身写入的本地缓存信封。
export function readLocalReceipt(account: string): HandoffReceipt | null {
  const ptr = readLocalHandoffPointer(account);
  if (!ptr) return null;
  return decodeHandoffEnvelope(ptr.cid)?.receipt ?? null;
}

/// EN: OLD-primary side of the online handoff (§5.2 steps 2–3). Given the recipient's directory-key-
/// endorsed peer key, verify it belongs to this account and names `to`, export THIS device's signing-
/// key bundle, seal it to the recipient peer key, and mint+publish the authority receipt. Returns the
/// payload (plain receipt + sealed bundle) to deliver over the account self-channel. After this the
/// caller should drop its own signing authority (engine read-only / re-derive). CN: 在线交接的**旧主
/// 设备**侧（§5.2 步骤 2–3）。给定接收方经目录钥背书的对端钥，校验其属于本账户且指向 `to`，导出**本设备**
/// 签名钥 bundle、封装给接收方对端钥，并铸造+发布权威收据。返回载荷（明文收据 + 封装 bundle）供经账户自通道
/// 投递。此后调用方应放弃自身发送权（引擎只读 / 重新派生）。
export async function sealHandoff(args: {
  account: string;
  dir: DeviceDirectoryKey;
  from: string;
  to: string;
  recipientEndorsement: DevicePeerEndorsement;
  engine: SigningKeyTransfer;
  now?: number;
}): Promise<SealedHandoffPayload> {
  const ok = await verifyDevicePeerEndorsement(args.dir.publicKey, args.recipientEndorsement);
  if (!ok) throw new Error("recipient device-peer endorsement failed verification");
  if (args.recipientEndorsement.deviceId !== args.to) {
    throw new Error("recipient endorsement device id does not match handoff target");
  }
  const bundle = args.engine.exportSigningKeys();
  const sealedBundle = bytesToB64(await sealToPeer(fromHex(args.recipientEndorsement.peerPublicKey), bundle));
  const signed = await publishHandoff({
    account: args.account,
    dir: args.dir,
    from: args.from,
    to: args.to,
    now: args.now,
  });
  return { receipt: signed, sealedBundle };
}

/// EN: NEW-device side of the online handoff (§5.2 step 4). Verify the receipt signature + that it
/// names this device + that it is not stale vs the relay's latest, open the sealed signing-key bundle
/// with this device's peer private key, and install it (upgrading the read-only escrow client to an
/// active sender). Returns true on success; false on any verification/decryption/install failure (the
/// device stays read-only). CN: 在线交接的**新设备**侧（§5.2 步骤 4）。校验收据签名 + 指向本设备 + 相对
/// relay 最新不过期，用本设备对端私钥打开封装签名钥 bundle 并装入（把只读托管客户端升级为活跃发送者）。
/// 成功返回 true；任何验证/解密/装入失败返回 false（设备保持只读）。
export async function openHandoff(args: {
  account: string;
  dirPublicKey: Uint8Array;
  myDeviceId: string;
  myPeerPrivate: CryptoKey;
  payload: SealedHandoffPayload;
  engine: SigningKeyTransfer;
}): Promise<boolean> {
  if (!(await verifyHandoffReceipt(args.dirPublicKey, args.payload.receipt))) return false;
  if (args.payload.receipt.receipt.to !== args.myDeviceId) return false;
  const latest = await fetchLatestReceipt(args.account, args.dirPublicKey);
  if (latest && latest.receipt.seq > args.payload.receipt.receipt.seq) return false;
  const bundle = await openSealed(args.myPeerPrivate, b64ToBytes(args.payload.sealedBundle));
  if (!bundle) return false;
  try {
    args.engine.installSigningKeys(bundle);
    return true;
  } catch {
    return false;
  }
}
