// EN: Peer prekey retrieval for the decentralized 1:1 stack (X3DH). Reads a peer's
// on-chain `pallet-msg-identity` anchor (IK + endorsement + prekey epoch, SPK +
// endorsement), verifies every published DH key relay-trustlessly against the account's
// sr25519 endorsement (design §4.4 / §17.2), and assembles a `PeerPrekeyBundle` the
// engine consumes via `initOutbound`. v1 uses the SPK as the X3DH prekey (no one-time
// key); relay-served OPK single-dispense (§19/§21 `opk_fetch`) is a Phase-2 hardening,
// so `opk` is left undefined and the engine falls back to the SPK (design §6). Also
// implements the §20 stack negotiation (DR vs MLS-Wire). Import-decoupled from `@/mls/*`.
// CN: 去中心化 1:1 栈（X3DH）的对端预密钥取回。读取对端链上 `pallet-msg-identity` 锚
// （IK + 背书 + 预密钥纪元、SPK + 背书），对每个发布的 DH 公钥做 relay-trustless 校验
// （账户 sr25519 背书，设计 §4.4 / §17.2），装配出引擎经 `initOutbound` 消费的
// `PeerPrekeyBundle`。v1 以 SPK 作 X3DH 预密钥（无一次性钥）；relay 单发 OPK（§19/§21
// `opk_fetch`）属 Phase-2 加固，故 `opk` 留空、引擎回退到 SPK（设计 §6）。并实现 §20 栈协商
// （DR vs MLS-Wire）。与 `@/mls/*` import 解耦。

import {
  chainClient,
  type RawDeviceRecord,
  type RawSignedPrekey,
} from "@/chain/chainClient";
import { deviceIdFromIk } from "@/crypto-dr/dmEnvelope";
import { CTX_IK_ENDORSE, CTX_SPK_ENDORSE, verifyEndorsement } from "@/crypto-dr/identityBridge";
import { STACK_DR, STACK_MLS_WIRE, type PeerPrekeyBundle } from "@/crypto-dr/types";

const eqBytes = (a: Uint8Array, b: Uint8Array): boolean =>
  a.length === b.length && a.every((x, i) => x === b[i]);

/// EN: Verify a peer device record + signed prekey and assemble a `PeerPrekeyBundle`.
/// Throws if the self-certifying device id mismatches `blake2_128(ik)` or either
/// account-key endorsement fails (relay-trustless gate). `opk` is optional. CN: 校验对端
/// 设备记录 + 签名预密钥并装配 `PeerPrekeyBundle`。自证设备 id 不等于 `blake2_128(ik)` 或
/// 任一账户钥背书失败即抛错（relay-trustless 闸）。`opk` 可选。
export function assemblePeerBundle(
  account: string,
  dev: RawDeviceRecord,
  spk: RawSignedPrekey,
  opk?: Uint8Array,
): PeerPrekeyBundle {
  if (!eqBytes(deviceIdFromIk(dev.ik), dev.deviceId)) {
    throw new Error("assemblePeerBundle: device id != blake2_128(ik)");
  }
  if (!verifyEndorsement(CTX_IK_ENDORSE, dev.ik, dev.ikEndorsement, account)) {
    throw new Error("assemblePeerBundle: invalid IK endorsement");
  }
  if (!verifyEndorsement(CTX_SPK_ENDORSE, spk.spk, spk.spkEndorsement, account)) {
    throw new Error("assemblePeerBundle: invalid SPK endorsement");
  }
  return {
    account,
    device: dev.deviceId,
    ik: dev.ik,
    ikEndorsement: dev.ikEndorsement,
    spk: spk.spk,
    spkEndorsement: spk.spkEndorsement,
    opk,
    prekeyEpoch: dev.prekeyEpoch,
  };
}

/// EN: Options for `fetchPeerBundle`. CN: `fetchPeerBundle` 选项。
export interface FetchBundleOptions {
  /// EN: Target a specific peer device (else the first registered device). CN: 指定对端设备
  /// （否则取首个已注册设备）。
  deviceId?: Uint8Array;
}

/// EN: Fetch + verify a peer's X3DH prekey bundle from chain. Picks the requested (or
/// first) registered device, reads its SPK, and returns a verified bundle (SPK-fallback
/// X3DH; no OPK in v1). CN: 从链上取回并校验对端 X3DH 预密钥包。选指定（或首个）已注册设备、
/// 读其 SPK，返回校验后的包（SPK 回退 X3DH；v1 无 OPK）。
export async function fetchPeerBundle(
  account: string,
  opts: FetchBundleOptions = {},
): Promise<PeerPrekeyBundle> {
  const devices = await chainClient.msgIdentityDevices(account);
  if (devices.length === 0) {
    throw new Error(`fetchPeerBundle: ${account} has no registered DR devices`);
  }
  const dev = opts.deviceId
    ? devices.find((d) => eqBytes(d.deviceId, opts.deviceId!))
    : devices[0];
  if (!dev) throw new Error("fetchPeerBundle: requested device not found");
  const spk = await chainClient.msgIdentitySignedPrekey(account, dev.deviceId);
  if (!spk) throw new Error("fetchPeerBundle: device has no signed prekey published");
  return assemblePeerBundle(account, dev, spk);
}

/// EN: 1:1 stack choice (design §20). CN: 1:1 栈选择（设计 §20）。
export type StackChoice = "dr" | "mls_wire" | "none";

/// EN: Pure §20 mode picker: prefer DR when both ends support it, else MLS-Wire, else
/// incompatible. CN: §20 模式选择纯函数：双方都支持则选 DR，否则 MLS-Wire，否则不可通。
export function chooseStack(peerFlags: number, selfFlags: number): StackChoice {
  if (peerFlags & selfFlags & STACK_DR) return "dr";
  if (peerFlags & selfFlags & STACK_MLS_WIRE) return "mls_wire";
  return "none";
}

/// EN: Negotiate the 1:1 stack with `peer` (§20). An account with no advertised caps is
/// treated as a legacy MLS-Wire-only client. CN: 与 `peer` 协商 1:1 栈（§20）。未公告能力的
/// 账户按旧版仅 MLS-Wire 客户端处理。
export async function negotiateStack(
  peer: string,
  selfFlags: number = STACK_DR | STACK_MLS_WIRE,
): Promise<StackChoice> {
  const caps = await chainClient.msgIdentityStackCaps(peer);
  const peerFlags = caps?.flags ?? STACK_MLS_WIRE;
  return chooseStack(peerFlags, selfFlags);
}
