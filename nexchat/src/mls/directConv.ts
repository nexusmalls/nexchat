// EN: 1:1 direct conversation id conventions. UI conv id is `d:{peer}` (Merge Spec);
// both parties share a canonical MLS group key `d:{sorted_a}:{sorted_b}`.
// CN: 1:1 私聊会话 id 约定。UI 会话 id 为 `d:{peer}`（Merge Spec）；双方共用规范
// MLS 群键 `d:{排序后_a}:{排序后_b}`。

import { canonicalAddress, tryCanonicalAddress } from "@/wallet/address";
import type { RelayFrame } from "@/relay/relayClient";

/// EN: Canonical MLS group handle for a pairwise session (same on both ends). Both addresses
/// are normalized to RPC_SS58 (273) first, so a 42-form input (e.g. echoed by the relay) and a
/// 273-form input map to the SAME group key — without this, mixed prefixes split the key and
/// the handshake never matches. CN: 成对会话的规范 MLS 群句柄（两端相同）。先把两个地址归一到
/// RPC_SS58（273），使 42 形态（如 relay 回显）与 273 形态映射到**同一**群 key——否则混前缀会
/// 拆分 key，导致握手永远对不上。
export function directMlsKey(a: string, b: string): string {
  return `d:${[canonicalAddress(a), canonicalAddress(b)].sort().join(":")}`;
}

/// EN: UI / relay message routing id — always the counterparty account (canonicalized).
/// CN: UI / relay 消息路由 id——始终为对端账户（已归一）。
export function directConvId(peer: string): string {
  return `d:${canonicalAddress(peer)}`;
}

/// EN: Device-distinct MLS leaf credential identity for 1:1 Wire multi-leaf (HYBRID_DESIGN §4.2):
/// `{account}#{deviceId}`. Each of my devices carries its OWN leaf bound to the same account, so a
/// single device can be targeted by `removeMembers` (per-device PCS). The account is canonicalized
/// so both ends agree. CN: 1:1 Wire 多 leaf 的设备区分 MLS leaf 凭证 identity（设计 §4.2）：
/// `{account}#{deviceId}`。我的每个设备持有**各自**绑定同一账户的 leaf，故可用 `removeMembers` 定位单个
/// 设备（按设备 PCS）。账户已归一以保两端一致。
export function deviceLeafIdentity(account: string, deviceId: string): string {
  return `${canonicalAddress(account)}#${deviceId}`;
}

/// EN: Map an MLS leaf credential identity back to its account, tolerating both the plain account
/// form and the device-distinct `{account}#{deviceId}` form. CN: 把 MLS leaf 凭证 identity 映射回账户，
/// 兼容纯账户形态与设备区分 `{account}#{deviceId}` 形态。
export function accountFromLeafIdentity(identity: string): string {
  const hash = identity.indexOf("#");
  const account = hash >= 0 ? identity.slice(0, hash) : identity;
  return canonicalAddress(account);
}

/// EN: Extract the device id (the `#`-suffix) from a leaf credential identity, or "" if the identity
/// is account-only. Used to verify the E2EI device-leaf credential (§3.9), whose binding commits to
/// `(account, deviceId, leafKey)`. CN: 从 leaf 凭证 identity 提取设备 id（`#` 后缀）；纯账户形态返回 ""。
/// 用于验证 E2EI 设备 leaf 凭证（§3.9），其绑定承诺 `(account, deviceId, leafKey)`。
export function deviceFromLeafIdentity(identity: string): string {
  const hash = identity.indexOf("#");
  return hash >= 0 ? identity.slice(hash + 1) : "";
}

/// EN: Extract peer address from a UI direct conv id (canonicalized to RPC_SS58).
/// CN: 从 UI 私聊 conv id 解析对端地址（归一到 RPC_SS58）。
export function peerFromDirectConvId(convId: string): string | null {
  if (!convId.startsWith("d:")) return null;
  const rest = convId.slice(2);
  // EN: MLS keys contain an extra ':' — UI ids do not. CN: MLS 键含额外 ':'，UI id 不含。
  if (rest.includes(":")) return null;
  return canonicalAddress(rest);
}

/// EN: Map UI conv id → MLS encrypt/decrypt key for `self`. CN: UI conv id → 本方 MLS 加解密密钥。
export function resolveMlsConvId(uiConvId: string, selfAddress: string): string {
  const peer = peerFromDirectConvId(uiConvId);
  if (!peer) return uiConvId;
  return directMlsKey(selfAddress, peer);
}

/// EN: True when `addr` participates in canonical direct MLS key `mlsKey` (prefix-agnostic).
/// CN: `addr` 是否参与该规范私聊 MLS 键（前缀无关）。
export function directMlsKeyInvolves(mlsKey: string, addr: string): boolean {
  if (!mlsKey.startsWith("d:")) return false;
  const parts = mlsKey.slice(2).split(":");
  return parts.length === 2 && parts.includes(canonicalAddress(addr));
}

/// EN: Owner of a pairwise handshake = lexicographically smaller address (deterministic).
/// Both ends MUST agree, so addresses are canonicalized first — a 42/273 mismatch would
/// otherwise elect different owners and deadlock the handshake. CN: 成对握手 owner = 字典序较小
/// 地址（确定性）。两端必须一致，故先归一——否则 42/273 不一致会选出不同 owner，握手死锁。
export function directHandshakeOwner(a: string, b: string): string {
  const ca = canonicalAddress(a);
  const cb = canonicalAddress(b);
  return ca < cb ? ca : cb;
}

/// EN: Counterparty in a canonical direct MLS key for `self`. CN: 规范私聊 MLS 键中对 `self` 的对端。
export function peerFromMlsKey(mlsKey: string, selfAddress: string): string | null {
  if (!mlsKey.startsWith("d:")) return null;
  const parts = mlsKey.slice(2).split(":");
  if (parts.length !== 2) return null;
  const self = canonicalAddress(selfAddress);
  const a = canonicalAddress(parts[0]!);
  const b = canonicalAddress(parts[1]!);
  if (a === self) return b;
  if (b === self) return a;
  return null;
}

/// EN: Remap inbound direct `convId` from sender view (`d:{receiver}`) to receiver UI id (`d:{sender}`).
/// CN: 将入站 direct `convId` 从发送方视角（`d:{receiver}`）映射为接收方 UI id（`d:{sender}`）。
export function resolveDirectInboundConv(
  frame: RelayFrame,
  selfAddress: string,
  senderRef: string,
): { convId: string; senderRef: string } {
  const convId = frame.convId;
  const sender = senderRef;
  if (!convId.startsWith("d:")) return { convId, senderRef: sender };

  // EN: `peerFromDirectConvId` already canonicalizes, so this matches regardless of whether the
  // relay echoed a 42-form or 273-form address in `convId`. CN: `peerFromDirectConvId` 已归一，
  // 故无论 relay 在 convId 里回显 42 还是 273 形态都能匹配。
  const inboundPeer = peerFromDirectConvId(convId);
  if (inboundPeer !== canonicalAddress(selfAddress)) return { convId, senderRef: sender };

  if (sender && sender !== "peer") {
    const canon = tryCanonicalAddress(sender);
    if (canon && canon !== canonicalAddress(selfAddress)) {
      return { convId: directConvId(canon), senderRef: canon };
    }
  }

  const fromKey =
    (frame.delivery?.mlsKey && peerFromMlsKey(frame.delivery.mlsKey, selfAddress)) || null;
  if (fromKey) {
    return { convId: directConvId(fromKey), senderRef: fromKey };
  }

  return { convId, senderRef: sender };
}
