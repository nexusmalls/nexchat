// EN: Pure planner for group Wire NO-SIBLING join settle (CHAT_GROUP_WIREIFY_DESIGN §8.4). When a fresh
// same-account device finds no online sibling/CD to graft it into groups, it must ask EXISTING group
// members to peer-add its leaf — but only for groups it is already a member of (from the authoritative
// conversation list AFTER cloud restore settles). ACTIVE groups get a targeted `requestGroupPeerAdd` now;
// dormant groups defer until the user opens them (`ensureGraftOrPeerAdd`). Already-held groups are
// excluded. Pure + deterministic → unit-testable.
//
// CN: 群 Wire **无兄弟** join 安定的纯规划器（设计 §8.4）。同账户新设备找不到在线兄弟/CD 把它接进群时，须请
// **既有群成员** peer-add 其 leaf——但仅限其**已是成员**的群（来自云恢复安定**后**权威的会话列表）。**活跃**群现在
// 走定向 `requestGroupPeerAdd`；**休眠**群延迟到用户打开时再 `ensureGraftOrPeerAdd`。已持群排除。纯函数、确定性 → 可单测。

export interface WireGroupJoinSettleInput {
  /** EN: group conv ids (`g:<id>`) the account is already a member of. CN: 账户**已是成员**的群会话 id
   *  （`g:<id>`）。 */
  memberGroups: readonly string[];
  /** EN: local engine already holds the group (already a leaf). CN: 本地引擎已持群（已是 leaf）。 */
  isHeld: (conv: string) => boolean;
  /** EN: group is ACTIVE now (opened / recently messaged). CN: 群当前**活跃**（打开 / 近期发言）。 */
  isActive: (conv: string) => boolean;
}

export interface WireGroupJoinSettlePlan {
  /** EN: active, not-yet-held member groups → `requestGroupPeerAdd` now. CN: 活跃且尚未持有的成员群 → 现在
   *  `requestGroupPeerAdd`。 */
  peerAssist: string[];
  /** EN: dormant member groups → defer until next activation. CN: 休眠成员群 → 延迟到下次激活。 */
  defer: string[];
}

/// EN: Split no-sibling group join targets into peer-assist now vs defer. CN: 把无兄弟群 join 目标切分为
/// 现在 peer-assist 与延迟。
export function planWireGroupJoinSettle(input: WireGroupJoinSettleInput): WireGroupJoinSettlePlan {
  const peerAssist: string[] = [];
  const defer: string[] = [];
  const seen = new Set<string>();
  for (const conv of input.memberGroups) {
    if (!conv || !conv.startsWith("g:")) continue;
    if (seen.has(conv)) continue;
    seen.add(conv);
    if (input.isHeld(conv)) continue;
    if (input.isActive(conv)) peerAssist.push(conv);
    else defer.push(conv);
  }
  return { peerAssist, defer };
}
