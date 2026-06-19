// EN: Identity bridge — ties the vodozemac engine (which owns the X25519 IK/SPK/OPK
// material) to the on-chain `pallet-msg-identity` anchor. It (1) endorses each published
// DH key with the account's sr25519 key (`CTX ‖ key`, verified relay-trustlessly by
// peers), (2) computes the OPK Merkle root (§19), and (3) submits register/SPK/OPK-root
// extrinsics + advertises DR capability (§20). This is the shared *identity layer*: it
// never touches ratchet state and is import-decoupled from `@/mls/*`.
// CN: 身份桥——把 vodozemac 引擎（持有 X25519 IK/SPK/OPK 材料）接到链上 `pallet-msg-identity`
// 锚。它 (1) 用账户 sr25519 钥对每个发布的 DH 公钥背书（`CTX ‖ key`，对端 relay-trustless 验证），
// (2) 计算 OPK Merkle 根（§19），(3) 提交 register/SPK/OPK-root extrinsic 并公告 DR 能力（§20）。
// 这是共用*身份层*：不触碰棘轮态，且与 `@/mls/*` import 解耦。
//
// SPK 派生说明 / SPK derivation note (v1):
// EN: §4.2 envisaged an HKDF-from-`vault_master` recomputable SPK, but vodozemac generates
// its fallback key randomly and exposes no way to import external key bytes. Since §4.2
// marks SPK recomputability as an optimization (not a security requirement) and pickle
// persistence already survives refresh/restore, v1 uses the vodozemac fallback key as SPK.
// CN: §4.2 设想 SPK 由 `vault_master` HKDF 可重算，但 vodozemac 的 fallback key 随机生成且不支持
// 导入外部钥字节。鉴于 §4.2 已注明 SPK 可重算仅为优化（非安全要求），且 pickle 持久化已能跨刷新/恢复，
// v1 直接以 vodozemac fallback key 作为 SPK。

import { chainClient } from "@/chain/chainClient";
import { signRawWithAccountKey } from "@/chain/signer";
import { opkMerkleRoot } from "@/crypto-dr/opkMerkle";
import type { DrPersistence } from "@/crypto-dr/sessionStore";
import { STACK_DR, STACK_MLS_WIRE } from "@/crypto-dr/types";
import type { VodozemacEngine } from "@/crypto-dr/vodozemacEngine";
import { signatureVerify } from "@polkadot/util-crypto";

const te = new TextEncoder();

const toHex = (b: Uint8Array): string =>
  Array.from(b, (x) => x.toString(16).padStart(2, "0")).join("");

/// EN: Account-key endorsement contexts. MUST match `pallet-msg-identity` (`CTX_IK_ENDORSE`
/// / `CTX_SPK_ENDORSE`, design §17.2). Changing them breaks every existing endorsement.
/// CN: 账户钥背书上下文。必须与 `pallet-msg-identity` 一致（设计 §17.2）。改动即破坏所有既有背书。
export const CTX_IK_ENDORSE = te.encode("nexchat/x3dh/ik-endorse/v1");
export const CTX_SPK_ENDORSE = te.encode("nexchat/x3dh/spk-endorse/v1");

/// EN: 1:1 stack protocol version advertised in `ChatStackCaps.version` (§20). CN: `ChatStackCaps.version` 公告的 1:1 栈协议版本（§20）。
export const DR_STACK_VERSION = 1;

const concat = (ctx: Uint8Array, key: Uint8Array): Uint8Array => {
  const out = new Uint8Array(ctx.length + key.length);
  out.set(ctx, 0);
  out.set(key, ctx.length);
  return out;
};

/// EN: Sign `CTX ‖ key` with the active account sr25519 key. Throws if the active signer
/// cannot raw-sign (injector wallets). CN: 用当前账户 sr25519 钥签 `CTX ‖ key`。当前签名者
/// 不支持裸签（注入器钱包）时抛错。
export function endorseKey(ctx: Uint8Array, key: Uint8Array): Uint8Array {
  const sig = signRawWithAccountKey(concat(ctx, key));
  if (!sig) {
    throw new Error("endorseKey: active signer cannot raw-sign (injector wallet unsupported)");
  }
  if (sig.length !== 64) {
    throw new Error(`endorseKey: expected 64-byte sr25519 signature, got ${sig.length}`);
  }
  return sig;
}

/// EN: Relay-trustless verification that `key` was endorsed by `accountAddress` under
/// `ctx` (the check a peer / relay runs on a fetched prekey). CN: relay-trustless 校验
/// `key` 在 `ctx` 下确由 `accountAddress` 背书（对端 / relay 取到预密钥后所做）。
export function verifyEndorsement(
  ctx: Uint8Array,
  key: Uint8Array,
  sig: Uint8Array,
  accountAddress: string,
): boolean {
  return signatureVerify(concat(ctx, key), sig, accountAddress).isValid;
}

/// EN: Options for `publishPrekeyBundle`. CN: `publishPrekeyBundle` 选项。
export interface PublishOptions {
  /// EN: Number of one-time prekeys to generate (design §19 suggests ~100). CN: 生成的一次性预密钥数量。
  opkCount?: number;
  /// EN: Advisory SPK expiry block (0 = unset). CN: SPK 建议过期区块（0 = 未设）。
  validUntil?: number;
  /// EN: Also advertise DR capability via `set_stack_caps` (default true). CN: 是否同时经
  /// `set_stack_caps` 公告 DR 能力（默认 true）。
  advertiseStack?: boolean;
  /// EN: When provided, persist the published OPK set (root + leaves) so the `OpkResponder`
  /// can serve single-dispensed leaves to X3DH initiators (§19). CN: 提供时持久化已发布 OPK
  /// 集合（根 + 叶子），供 `OpkResponder` 向 X3DH 发起方单发（§19）。
  store?: DrPersistence;
}

/// EN: The prekey material published for a device (returned so the caller can hand the OPK
/// leaves to the relay control plane, §19). CN: 为某设备发布的预密钥材料（返回以便调用方把
/// OPK 叶子交给 relay 控制面，§19）。
export interface PublishedBundle {
  deviceId: Uint8Array;
  ik: Uint8Array;
  spk: Uint8Array;
  opks: Uint8Array[];
  opkRoot: Uint8Array;
}

/// EN: Publish this device's full X3DH prekey bundle to `pallet-msg-identity`: register
/// the IK (idempotent), set the SPK, set the OPK Merkle root, then mark the engine's keys
/// published and advertise DR capability. CN: 把本设备完整 X3DH 预密钥包发布到
/// `pallet-msg-identity`：注册 IK（幂等）、设置 SPK、设置 OPK Merkle 根，随后把引擎的钥标记
/// 已发布并公告 DR 能力。
export async function publishPrekeyBundle(
  engine: VodozemacEngine,
  opts: PublishOptions = {},
): Promise<PublishedBundle> {
  const opkCount = opts.opkCount ?? 100;
  const ik = engine.identityKey();
  const deviceId = engine.deviceId();

  // 1. 注册设备 IK（幂等：已注册则忽略）/ register device IK (idempotent).
  const ikEndorsement = endorseKey(CTX_IK_ENDORSE, ik);
  try {
    await chainClient.signAndSend("msgIdentity", "registerDevice", [deviceId, ik, ikEndorsement]);
  } catch (e) {
    if (!String(e).includes("DeviceAlreadyExists")) throw e;
  }

  // 2. 设置签名预密钥 SPK（= vodozemac fallback key）/ set signed prekey.
  const spk = engine.rotateSignedPreKey();
  const spkEndorsement = endorseKey(CTX_SPK_ENDORSE, spk);
  await chainClient.signAndSend("msgIdentity", "setSignedPrekey", [
    deviceId,
    spk,
    spkEndorsement,
    opts.validUntil ?? 0,
  ]);

  // 3. 生成 OPK 集合并上根 / generate OPK set and publish the root.
  const opks = engine.generateOneTimePreKeys(opkCount);
  const opkRoot = opkMerkleRoot(opks);
  await chainClient.signAndSend("msgIdentity", "setOpkRoot", [deviceId, opkRoot, opks.length]);

  // 4. 链上提交成功后再标记已发布 / mark published only after on-chain commit.
  engine.markKeysAsPublished();

  // 4b. 持久化已发布 OPK 集合，供 OpkResponder 单发（§19）/ persist the published OPK set
  //     so the OpkResponder can single-dispense leaves (§19).
  if (opts.store) {
    await opts.store.saveOpkBundle({
      device: toHex(deviceId),
      root: toHex(opkRoot),
      opks: opks.map(toHex),
      spent: [],
    });
  }

  // 5. 公告栈能力（§20）：DR 优先，保留 MLS-Wire 回退位。/ advertise stack caps (§20): DR +
  //     MLS-Wire fallback so legacy peers can still negotiate down.
  if (opts.advertiseStack !== false) {
    await advertiseStackCaps(STACK_DR | STACK_MLS_WIRE, DR_STACK_VERSION);
  }

  return { deviceId, ik, spk, opks, opkRoot };
}

/// EN: Advertise this account's 1:1 stack capability flags + version (`set_stack_caps`,
/// §20). CN: 公告本账户 1:1 栈能力位 + 版本（`set_stack_caps`，§20）。
export async function advertiseStackCaps(flags: number, version: number): Promise<void> {
  await chainClient.signAndSend("msgIdentity", "setStackCaps", [flags, version]);
}
