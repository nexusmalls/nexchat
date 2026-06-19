// EN: OPK control-plane exchange for the DR stack (design §19 / §21). One-time prekeys
// (OPK) are anchored on chain only as a Merkle ROOT (`pallet-msg-identity.DeviceOpkRoots`);
// the individual leaves are distributed off-chain. This module is the CLIENT side of that
// distribution over the existing relay control plane (`opk_fetch` → `opk_publish`), which
// the relay merely forwards untouched (like the MLS `hello`/`kp` control messages):
//   - responder (OPK owner): serves ONE unused leaf + its Merkle proof per request,
//     tracking a best-effort spent-set so a leaf is single-dispensed (§19);
//   - initiator: requests a leaf for the peer device, then verifies the proof against the
//     on-chain root (relay-trustless) before using it for X3DH.
// The relay ALSO caches an owner's uploaded leaf set (`OpkResponder.upload()`) and single-dispenses
// from it while the owner is OFFLINE; on a cache miss it forwards the fetch to an online owner whose
// `serve` reply carries `toAddr` so the relay routes the single leaf back to the initiator (relay-rs
// `server/src/{state,protocol}.rs`, design §21 "实现状态"). X3DH still falls back to the SPK when no
// OPK is obtained (timeout / verify fail, design §6). Decoupled from `@/mls/*`.
// CN: DR 栈的 OPK 控制面交换（设计 §19 / §21）。一次性预密钥（OPK）链上仅以 Merkle 根锚定
// （`pallet-msg-identity.DeviceOpkRoots`），叶子链下分发。本模块是该分发在现有 relay 控制面
// （`opk_fetch` → `opk_publish`）上的**客户端**侧，relay 仅原样转发（如 MLS `hello`/`kp`）：
// 响应方（OPK 持有者）每次请求单发一条未用叶子 + 其 Merkle 证明，按 best-effort 已用集合做单发
// （§19）；发起方为对端设备请求叶子，使用前对链上根校验证明（relay-trustless）。持有者**离线**时
// 由 relay 缓存代发，属独立 `relay-rs` 仓库（Phase 2，设计 §21「实现状态」）；在此之前由持有者按需
// 服务、取不到 OPK 时 X3DH 回退到 SPK（设计 §6）。与 `@/mls/*` 解耦。

import { chainClient } from "@/chain/chainClient";
import { fetchPeerBundle } from "@/crypto-dr/prekeyFetch";
import {
  decodeOpkProof,
  encodeOpkProof,
  opkMerkleProof,
  verifyOpkProof,
} from "@/crypto-dr/opkMerkle";
import type { DrPersistence, PublishedOpkBundle } from "@/crypto-dr/sessionStore";
import type { PeerPrekeyBundle } from "@/crypto-dr/types";
import type { ControlMsg, RelayClient } from "@/relay/relayClient";
import { b64ToBytes, bytesToB64 } from "@/util/b64";

const toHex = (b: Uint8Array): string =>
  Array.from(b, (x) => x.toString(16).padStart(2, "0")).join("");

const fromHex = (hex: string): Uint8Array => {
  const out = new Uint8Array(hex.length / 2);
  for (let i = 0; i < out.length; i++) out[i] = parseInt(hex.slice(i * 2, i * 2 + 2), 16);
  return out;
};

/// EN: A single served OPK leaf: its public key (hex) + a base64 Merkle proof against the
/// device's on-chain OPK root. CN: 单条已服务的 OPK 叶子：公钥（hex）+ 对设备链上 OPK 根的
/// base64 Merkle 证明。
export interface ServedOpkLeaf {
  opk_pub: string;
  proof: string;
}

/// EN: Pure single-dispense: pick the first unspent OPK from `bundle`, build its Merkle
/// proof, and return the served leaf plus the updated bundle (with the leaf marked spent).
/// Returns null when the set is exhausted. CN: 纯单发：从 `bundle` 取首个未用 OPK、构造其
/// Merkle 证明，返回服务叶子与更新后的 bundle（该叶标记已用）。耗尽时返回 null。
export function dispenseOpkLeaf(
  bundle: PublishedOpkBundle,
): { leaf: ServedOpkLeaf; bundle: PublishedOpkBundle } | null {
  const spent = new Set(bundle.spent);
  const chosenHex = bundle.opks.find((h) => !spent.has(h));
  if (!chosenHex) return null;
  const opks = bundle.opks.map(fromHex);
  const proof = opkMerkleProof(opks, fromHex(chosenHex));
  return {
    leaf: { opk_pub: chosenHex, proof: bytesToB64(encodeOpkProof(proof)) },
    bundle: { ...bundle, spent: [...bundle.spent, chosenHex] },
  };
}

/// EN: Build the served-leaf list for ALL not-yet-spent OPKs in `bundle` (each with a Merkle proof
/// against the bundle root) — the payload a device uploads so the relay can single-dispense leaves
/// while the device is OFFLINE (design §19/§21). CN: 为 `bundle` 中所有未用 OPK 构造服务叶子列表
/// （各带对 bundle 根的 Merkle 证明）——设备上传给 relay、使其在设备**离线**时单发的载荷（设计 §19/§21）。
export function buildOpkLeaves(bundle: PublishedOpkBundle): ServedOpkLeaf[] {
  const spent = new Set(bundle.spent);
  const unspent = bundle.opks.filter((h) => !spent.has(h));
  if (unspent.length === 0) return [];
  const opks = bundle.opks.map(fromHex);
  return unspent.map((h) => ({
    opk_pub: h,
    proof: bytesToB64(encodeOpkProof(opkMerkleProof(opks, fromHex(h)))),
  }));
}

/// EN: OPK responder: answers `opk_fetch` for this device with a single-dispensed leaf,
/// persisting the advancing spent-set. Construct, then `attach()`. CN: OPK 响应方：以单发
/// 叶子回应本设备的 `opk_fetch`，持久化推进的已用集合。先构造、再 `attach()`。
export class OpkResponder {
  constructor(
    private readonly relay: RelayClient,
    private readonly store: DrPersistence,
    private readonly selfRef: string,
    private readonly deviceHex: string,
  ) {}

  attach(): void {
    this.relay.onControl((msg) => {
      if (msg.t === "opk_fetch" && msg.target_device === this.deviceHex) {
        void this.serve(msg.convId, msg.from);
      }
    });
  }

  /// EN: Upload this device's whole unspent OPK leaf set to the relay so it can single-dispense
  /// leaves to X3DH initiators while we are OFFLINE (design §19/§21). Idempotent; a no-op when the
  /// bundle is missing/exhausted or doesn't match this device. CN: 把本设备整个未用 OPK 叶子集合
  /// 上传给 relay，使其在我们**离线**时向 X3DH 发起方单发（设计 §19/§21）。幂等；bundle 缺失/耗尽
  /// 或不匹配本设备时为空操作。
  async upload(): Promise<void> {
    const bundle = await this.store.loadOpkBundle();
    if (!bundle || bundle.device !== this.deviceHex) return;
    const leaves = buildOpkLeaves(bundle);
    if (leaves.length === 0) return;
    await this.relay.sendControl({
      t: "opk_publish",
      convId: `s:${this.selfRef}`,
      from: this.selfRef,
      device_id: this.deviceHex,
      root: bundle.root,
      leaves,
    });
  }

  /// EN: `toAddr` is the requesting initiator's account (the `opk_fetch` `from`). On the WS relay a
  /// live reply needs it so the relay routes the single leaf back to the initiator rather than to
  /// our own account; in-process relays fan out and the initiator matches on `device_id`, so it is
  /// harmless there. CN: `toAddr` 为请求发起方账户（`opk_fetch` 的 `from`）。WS relay 上实时回复需
  /// 借此把单叶路由回发起方而非我们自己账户；进程内 relay 为扇出、发起方按 `device_id` 匹配，故无害。
  private async serve(convId: string, toAddr?: string): Promise<void> {
    const bundle = await this.store.loadOpkBundle();
    if (!bundle || bundle.device !== this.deviceHex) return;
    const out = dispenseOpkLeaf(bundle);
    if (!out) return; // exhausted — initiator falls back to SPK
    await this.store.saveOpkBundle(out.bundle);
    await this.relay.sendControl({
      t: "opk_publish",
      convId,
      from: this.selfRef,
      device_id: this.deviceHex,
      root: bundle.root,
      leaves: [out.leaf],
      ...(toAddr ? { toAddr } : {}),
    });
  }
}

/// EN: Options for `requestOpk`. CN: `requestOpk` 选项。
export interface RequestOpkOptions {
  selfRef: string;
  peerAccount: string;
  peerDevice: Uint8Array;
  /// EN: On-chain OPK Merkle root the served leaf is verified against. CN: 服务叶子据以校验
  /// 的链上 OPK Merkle 根。
  root: Uint8Array;
  /// EN: Control-plane channel id (default the peer account self-channel `s:<account>`).
  /// CN: 控制面通道 id（默认对端账户自通道 `s:<account>`）。
  convId?: string;
  /// EN: Give up after this many ms (default 5000) → caller uses SPK fallback. CN: 超时
  /// 毫秒（默认 5000）→ 调用方回退 SPK。
  timeoutMs?: number;
}

/// EN: Request ONE one-time prekey for `peerDevice` over the control plane and verify its
/// Merkle proof against the on-chain `root`. Resolves to the 32-byte OPK public key, or
/// null on timeout / verification failure (→ SPK fallback). CN: 经控制面为 `peerDevice`
/// 请求一条一次性预密钥并对链上 `root` 校验其 Merkle 证明。返回 32 字节 OPK 公钥，或在超时/
/// 校验失败时返回 null（→ SPK 回退）。
export function requestOpk(relay: RelayClient, opts: RequestOpkOptions): Promise<Uint8Array | null> {
  const targetHex = toHex(opts.peerDevice);
  const convId = opts.convId ?? `s:${opts.peerAccount}`;
  return new Promise((resolve) => {
    let done = false;
    const finish = (v: Uint8Array | null) => {
      if (done) return;
      done = true;
      clearTimeout(timer);
      resolve(v);
    };
    const timer = setTimeout(() => finish(null), opts.timeoutMs ?? 5000);
    relay.onControl((msg: ControlMsg) => {
      if (done || msg.t !== "opk_publish" || msg.device_id !== targetHex) return;
      const leaf = msg.leaves[0];
      if (!leaf) return;
      const opk = fromHex(leaf.opk_pub);
      let proof;
      try {
        proof = decodeOpkProof(b64ToBytes(leaf.proof));
      } catch {
        return; // malformed proof — keep waiting until timeout
      }
      if (verifyOpkProof(opts.root, opk, proof)) finish(opk);
    });
    void relay.sendControl({ t: "opk_fetch", convId, from: opts.selfRef, target_device: targetHex });
  });
}

/// EN: Fetch a peer's verified prekey bundle (chain) and try to upgrade it with a one-time
/// prekey served over the relay (§19); on timeout / no OPK published, returns the
/// SPK-fallback bundle unchanged (design §6). The single call X3DH initiation should use.
/// CN: 取回对端已校验预密钥包（链上）并尝试用经 relay 服务的一次性预密钥升级（§19）；超时/
/// 未发布 OPK 时原样返回 SPK 回退包（设计 §6）。X3DH 发起应使用的单一入口。
export async function fetchPeerBundleWithOpk(
  relay: RelayClient,
  selfRef: string,
  peerAccount: string,
  opts: { deviceId?: Uint8Array; timeoutMs?: number } = {},
): Promise<PeerPrekeyBundle> {
  const bundle = await fetchPeerBundle(peerAccount, { deviceId: opts.deviceId });
  const opkRoot = await chainClient.msgIdentityOpkRoot(peerAccount, bundle.device);
  if (opkRoot && opkRoot.count > 0) {
    const opk = await requestOpk(relay, {
      selfRef,
      peerAccount,
      peerDevice: bundle.device,
      root: opkRoot.root,
      timeoutMs: opts.timeoutMs,
    });
    if (opk) return { ...bundle, opk };
  }
  return bundle;
}
