// EN: Pure planner for group Wire-ification lazy / on-demand device Add (CHAT_GROUP_WIREIFY_DESIGN
// §8.1 — "the most effective" fan-out lever). When a fresh same-account device is offered the groups its
// CD participates in, eagerly grafting a leaf into EVERY group costs one chain commit per group
// (fan-out = O(groups)). Instead, the new device only joins groups that are ACTIVE now (opened / recently
// messaged); dormant / archived groups are DEFERRED and grafted lazily when they are next activated
// (history is read from the archive meanwhile). Groups the device already holds (it is already a leaf)
// are excluded. Pure + deterministic → unit-testable; the session (`GroupWireSession`) consumes the plan
// to decide which KeyPackages to mint now vs. remember for later.
//
// CN: 群 Wire 化**延迟 / 按需**设备 Add 的纯规划器（设计 §8.1——"最有效"的 fan-out 杠杆）。同账户新设备被提供
// 其 CD 参与的群后，急着往**每个**群嫁接 leaf 要每群一条链 commit（fan-out = O(群数)）。改为：新设备只加入**当前
// 活跃**（打开 / 近期发言）的群；休眠 / 归档群**延迟**，待下次激活时再懒加载嫁接（其间历史从 archive 读）。设备
// **已持有**（已是该群 leaf）的群排除。纯函数、确定性 → 可单测；会话（`GroupWireSession`）消费该计划，决定哪些
// KeyPackage 现在铸造、哪些记下稍后再用。

export interface WireGroupJoinPlanInput {
  /** EN: group convs (`g:<id>`) the CD offered. CN: CD 提供的群会话（`g:<id>`）。 */
  offeredGroups: readonly string[];
  /** EN: this device already holds the group (already a leaf) → exclude from both buckets. CN: 本设备
   *  已持有该群（已是 leaf）→ 两桶都排除。 */
  isHeld: (conv: string) => boolean;
  /** EN: the group is ACTIVE now (opened / recently messaged) → join now; otherwise defer. When the
   *  caller has no activity signal it should pass `() => true` to preserve eager behavior. CN: 该群当前
   *  **活跃**（打开 / 近期发言）→ 现在加入；否则延迟。调用方无活跃度信号时传 `() => true` 保持急加载行为。 */
  isActive: (conv: string) => boolean;
}

export interface WireGroupJoinPlan {
  /** EN: active, not-yet-held groups → mint a KeyPackage + request a graft NOW. CN: 活跃且尚未持有的群
   *  → 现在铸造 KeyPackage + 请求嫁接。 */
  joinNow: string[];
  /** EN: dormant, not-yet-held groups → remember; graft lazily when next activated. CN: 休眠且尚未持有
   *  的群 → 记下；下次激活时懒嫁接。 */
  defer: string[];
}

/// EN: Split offered groups into join-now (active) vs defer (dormant), excluding already-held groups.
/// Order-preserving and de-duplicated; non-`g:` entries are ignored. CN: 把被提供的群切分为现在加入
/// （活跃）与延迟（休眠），排除已持有群。保序、去重；忽略非 `g:` 项。
export function planWireGroupJoin(input: WireGroupJoinPlanInput): WireGroupJoinPlan {
  const joinNow: string[] = [];
  const defer: string[] = [];
  const seen = new Set<string>();
  for (const conv of input.offeredGroups) {
    if (!conv || !conv.startsWith("g:")) continue;
    if (seen.has(conv)) continue;
    seen.add(conv);
    if (input.isHeld(conv)) continue;
    if (input.isActive(conv)) joinNow.push(conv);
    else defer.push(conv);
  }
  return { joinNow, defer };
}
