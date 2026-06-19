// EN: Pure planner for the Wire 1:1 multi-device NO-SIBLING join settle
// (CHAT_1TO1_WIRE_COMMIT_SERIALIZATION_SPEC §3.7/§3.8). When a fresh device finds no online sibling
// to graft it, it must reach existing 1:1 peers WITHOUT broadcasting "a new device came online" to
// every contact. This splits the candidate set using the authoritative existing-1:1 thread list
// (only trustworthy AFTER cloud restore settles — see appStore): peers we already share a 1:1 with
// get a targeted peer-assisted Add (§3.8); contacts we have NO 1:1 with get a normal pairwise
// handshake (establishing a brand-new chat — not a "new device" signal). Conversations already
// grafted (e.g. via a sibling offer) are excluded from both. Pure + deterministic → unit-testable.
//
// CN: Wire 1:1 多设备**无兄弟** join 安定的纯规划器（串行化规范 §3.7/§3.8）。新设备找不到在线兄弟来嫁接
// 时，必须触达已有 1:1 对端，而**不向每个联系人广播**「新设备上线」。本规划器用权威的已有 1:1 线索表
// （仅在云恢复安定**后**可信——见 appStore）切分候选集：已共享 1:1 的对端走定向对端代 Add（§3.8）；无 1:1
// 的联系人走常规 1:1 握手（建立全新会话——非「新设备」信号）。已嫁接（如经兄弟 offer）的会话两者都排除。
// 纯函数、确定性 → 可单测。

export interface WireJoinPlanInput {
  /** EN: self account (excluded from all targets). CN: 自身账户（从所有目标排除）。 */
  self: string;
  /** EN: full candidate peer set (roster + contacts). CN: 全部候选对端（roster + 联系人）。 */
  contacts: readonly string[];
  /** EN: peers we already share an existing 1:1 conversation thread with (authoritative AFTER restore
   *  settles). CN: 已与之共享既有 1:1 会话线索的对端（云恢复安定**后**才权威）。 */
  threadPeers: readonly string[];
  /** EN: conv for this peer is already graft-owned → skip (the Wire session owns it). CN: 该对端会话已
   *  归嫁接拥有 → 跳过（Wire 会话拥有它）。 */
  isGraftManaged: (peer: string) => boolean;
}

export interface WireJoinPlan {
  /** EN: existing 1:1 peers → send a targeted peer-assisted Add (low leak). CN: 已有 1:1 对端 → 发定向
   *  对端代 Add（低泄漏）。 */
  peerAssist: string[];
  /** EN: contacts with NO existing 1:1 → normal pairwise handshake (new chat). CN: 无既有 1:1 的联系人
   *  → 常规 1:1 握手（新会话）。 */
  registry: string[];
}

/// EN: Split no-sibling join targets into peer-assist vs registry. CN: 把无兄弟 join 目标切分为对端代
/// Add 与 registry 握手。
export function planWireJoinTargets(input: WireJoinPlanInput): WireJoinPlan {
  const threadSet = new Set(input.threadPeers);
  // EN: candidate universe = contacts ∪ existing-thread peers (a 1:1 peer may not be in the contact
  // roster). CN: 候选全集 = 联系人 ∪ 既有线索对端（1:1 对端未必在联系人 roster 里）。
  const universe = new Set<string>([...input.contacts, ...input.threadPeers]);
  const peerAssist: string[] = [];
  const registry: string[] = [];
  for (const peer of universe) {
    if (!peer || peer === input.self) continue;
    if (input.isGraftManaged(peer)) continue;
    if (threadSet.has(peer)) peerAssist.push(peer);
    else registry.push(peer);
  }
  return { peerAssist, registry };
}
