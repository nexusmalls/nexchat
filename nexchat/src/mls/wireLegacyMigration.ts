// EN: Detect legacy Track-A / account-scoped 1:1 MLS groups that still live on the account
// `openMlsEngine` but are absent from the Wire `wireEngine` after enabling Wire multi-leaf. Those
// sessions must re-handshake on the Wire engine (peer-assisted Add or cold establish) instead of
// silently staying on the legacy engine while the UI routes sends through Wire.
// CN: 检测仍留在账户级 `openMlsEngine`、但启用 Wire 多 leaf 后未出现在 Wire `wireEngine` 上的遗留
// Track A / 账户域 1:1 MLS 群。这些会话必须在 Wire 引擎上重握手（对端代 Add 或冷启动），而非静默留在
// 遗留引擎而 UI 已走 Wire 发送。

import { canonicalAddress } from "@/wallet/address";
import { peerFromMlsKey } from "@/mls/directConv";

/// EN: Minimal engine surface for group enumeration. CN: 群枚举所需的最小引擎接口。
export interface MlsGroupEngine {
  listGroups(): string[];
  hasGroup(convId: string): boolean;
}

/// EN: Peers whose canonical pairwise MLS key exists on `accountEngine` but NOT on `wireEngine`.
/// CN: 在 `accountEngine` 上存在规范成对 MLS 键、但 `wireEngine` 尚未持有的对端列表。
export function legacyDirectPeersForWireMigration(
  accountEngine: MlsGroupEngine,
  wireEngine: MlsGroupEngine,
  selfAddress: string,
): string[] {
  const out = new Set<string>();
  for (const key of accountEngine.listGroups()) {
    if (!key.startsWith("d:")) continue;
    const parts = key.slice(2).split(":");
    if (parts.length !== 2) continue;
    if (wireEngine.hasGroup(key)) continue;
    const peer = peerFromMlsKey(key, selfAddress);
    if (peer) out.add(canonicalAddress(peer));
  }
  return [...out];
}

/// EN: Merge UI thread peers with legacy account-engine peers (deduped, canonicalized).
/// CN: 合并 UI 线索对端与遗留 account-engine 对端（去重、归一）。
export function mergeWireJoinThreadPeers(
  threadPeers: readonly string[],
  legacyPeers: readonly string[],
): string[] {
  const seen = new Set<string>();
  const out: string[] = [];
  for (const raw of [...threadPeers, ...legacyPeers]) {
    const peer = canonicalAddress(raw);
    if (!peer || seen.has(peer)) continue;
    seen.add(peer);
    out.push(peer);
  }
  return out;
}
