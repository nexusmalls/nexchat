// EN: Pure roster derivation for the 1:1 Wire multi-leaf device-disclosure UX (design §8 / spec §3.9).
// Turns the engine's flat leaf-identity list (`account#deviceId`, from `OpenMlsEngine.memberIdentities`)
// into per-side device sets so the UI can (a) disclose how many devices each party has — the privacy
// cost of Wire multi-leaf is that the peer can SEE your device count — and (b) pick one of MY OTHER
// devices to remove (PCS self-heal). Every leaf here was E2EI account-bound-verified at its add path
// (§3.9 induction), so membership == a verified device. Pure + deterministic → unit-testable.
//
// CN: 1:1 Wire 多 leaf 设备披露 UX（设计 §8 / 规范 §3.9）的纯名册推导。把引擎扁平的 leaf 身份列表
// （`account#deviceId`，来自 `OpenMlsEngine.memberIdentities`）归并为按方分组的设备集，使 UI 能 (a) 披露
// 各方有几台设备——Wire 多 leaf 的隐私代价正是对端**能看到你的设备数**——并 (b) 从我**其他**设备中挑一台移除
// （PCS 自愈）。此处每个 leaf 均在其 add 路径经 E2EI 账户绑定校验（§3.9 归纳），故在列即为已验证设备。
// 纯函数、确定性 → 可单测。

import { canonicalAddress } from "@/wallet/address";
import { accountFromLeafIdentity, deviceFromLeafIdentity } from "@/mls/directConv";

/// EN: One MLS leaf = one device, resolved from its credential identity. CN: 一个 MLS leaf = 一台设备，
///由其凭证身份解析而来。
export interface WireDevice {
  /** EN: Full leaf credential identity `account#deviceId` (the value `removeDevice` expects). CN: 完整
   *  leaf 凭证身份 `account#deviceId`（`removeDevice` 所需值）。 */
  identity: string;
  /** EN: Canonical SS58 account this leaf is bound to. CN: 该 leaf 绑定的规范 SS58 账户。 */
  account: string;
  /** EN: Device id portion of the identity. CN: 身份中的设备 id 段。 */
  deviceId: string;
  /** EN: True when this is the local user's current device (must not self-remove). CN: 是否为本地用户
   *  当前这台设备（不可自移）。 */
  isThisDevice: boolean;
}

/// EN: Per-side device roster of a 1:1 Wire conversation. CN: 1:1 Wire 会话按方分组的设备名册。
export interface WireDeviceRoster {
  /** EN: The local user's devices in this conv. CN: 本地用户在此会话的设备。 */
  self: WireDevice[];
  /** EN: The peer's devices in this conv. CN: 对端在此会话的设备。 */
  peer: WireDevice[];
  /** EN: Leaves bound to neither party (should be empty in a 1:1; surfaced for safety). CN: 不属任一方的
   *  leaf（1:1 下应为空；为安全起见暴露）。 */
  other: WireDevice[];
  /** EN: Total leaves (devices) in the group. CN: 群内 leaf（设备）总数。 */
  total: number;
}

/// EN: Derive the per-side device roster from the engine's leaf-identity list. Accounts are canonicalized
/// so SS58-prefix variants compare equal. `thisDeviceId` marks the local device (excluded from removable
/// self devices). CN: 由引擎的 leaf 身份列表推导按方设备名册。账户规范化以消除 SS58 前缀差异。
/// `thisDeviceId` 标记本地设备（从可移除的本端设备中排除）。
export function computeWireDeviceRoster(
  identities: string[],
  selfAccount: string,
  peerAccount: string,
  thisDeviceId?: string,
): WireDeviceRoster {
  const self = canonicalAddress(selfAccount);
  const peer = canonicalAddress(peerAccount);
  const roster: WireDeviceRoster = { self: [], peer: [], other: [], total: 0 };
  for (const identity of identities) {
    const account = accountFromLeafIdentity(identity);
    const deviceId = deviceFromLeafIdentity(identity);
    const device: WireDevice = {
      identity,
      account,
      deviceId,
      isThisDevice: account === self && !!thisDeviceId && deviceId === thisDeviceId,
    };
    roster.total += 1;
    if (account === self) roster.self.push(device);
    else if (account === peer) roster.peer.push(device);
    else roster.other.push(device);
  }
  return roster;
}

/// EN: My OTHER devices in this conv — the only ones eligible for a remote remove (never the device the
/// user is on, which would lock them out). CN: 我在此会话的**其他**设备——唯一可远程移除的对象（绝不含
/// 用户当前所在的这台，以免把自己锁出）。
export function removableSelfDevices(roster: WireDeviceRoster): WireDevice[] {
  return roster.self.filter((d) => !d.isThisDevice);
}

/// EN: Device roster of a GROUP Wire conversation (CHAT_GROUP_WIREIFY_DESIGN §9). Unlike a 1:1 there is
/// no single "peer": every other account contributes its own device leaves. The UI uses this to disclose
/// the privacy cost of group Wire multi-leaf (members can SEE roughly how many devices you run) and to
/// offer per-device PCS self-heal on MY OWN other devices. CN: 群 Wire 会话的设备名册（设计 §9）。与 1:1
/// 不同，群里没有单一「对端」：每个其他账户各贡献自己的设备 leaf。UI 用它披露群 Wire 多 leaf 的隐私代价
/// （成员**能看到你大约有几台设备**），并对我**自己**的其他设备提供按设备 PCS 自愈。
export interface WireGroupRoster {
  /** EN: my devices in this group. CN: 我在此群的设备。 */
  self: WireDevice[];
  /** EN: every OTHER member account's devices, flat. CN: 所有**其他**成员账户的设备（扁平）。 */
  members: WireDevice[];
  /** EN: distinct other accounts (≈ how many people, not devices). CN: 其他账户去重数（≈ 几个人，非设备数）。 */
  memberAccounts: number;
  /** EN: total leaves (devices) in the group. CN: 群内 leaf（设备）总数。 */
  total: number;
}

/// EN: Derive the group device roster from the engine's leaf-identity list. Accounts are canonicalized so
/// SS58-prefix variants compare equal; `thisDeviceId` marks the local device (never removable). CN: 由引擎
/// 的 leaf 身份列表推导群设备名册。账户规范化以消除 SS58 前缀差异；`thisDeviceId` 标记本机设备（绝不可移）。
export function computeWireGroupRoster(
  identities: string[],
  selfAccount: string,
  thisDeviceId?: string,
): WireGroupRoster {
  const self = canonicalAddress(selfAccount);
  const roster: WireGroupRoster = { self: [], members: [], memberAccounts: 0, total: 0 };
  const otherAccounts = new Set<string>();
  for (const identity of identities) {
    const account = accountFromLeafIdentity(identity);
    const deviceId = deviceFromLeafIdentity(identity);
    const device: WireDevice = {
      identity,
      account,
      deviceId,
      isThisDevice: account === self && !!thisDeviceId && deviceId === thisDeviceId,
    };
    roster.total += 1;
    if (account === self) roster.self.push(device);
    else {
      roster.members.push(device);
      otherAccounts.add(account);
    }
  }
  roster.memberAccounts = otherAccounts.size;
  return roster;
}
