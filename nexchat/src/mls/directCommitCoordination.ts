// EN: Gate 1 — intra-account coordinator-device election + Wire 1:1 Commit wire helpers
// (CHAT_1TO1_WIRE_COMMIT_SERIALIZATION_SPEC §2). PURE logic + codecs only — no relay IO, no
// OpenMLS. Each account elects at most one coordinator device (CD) from the settled-online set
// (lexicographically smallest `device_id`); non-CD devices route `commit_intent` over the
// account self-channel `s:<account>` instead of emitting Commits directly. Gate 2 (relay CAS) lives
// in relay-rs; this module builds the client-side frames it expects (`commit_epoch`, `msgId`) and
// parses `commit_reject{epoch_stale}`.
// CN: 闸一——账户内协调设备选举 + Wire 1:1 Commit 线消息辅助（规范 §2）。**纯**逻辑 + 编解码——无
// relay IO、无 OpenMLS。每账户从 settled-online 集合中至多选一台协调设备（CD，`device_id` 字典序最小）；
// 非 CD 经账户自通道 `s:<account>` 路由 `commit_intent` 而非直接发 Commit。闸二（relay CAS）在
// relay-rs；本模块构造其期望的客户端帧（`commit_epoch`、`msgId`）并解析 `commit_reject{epoch_stale}`。

import type { CommitReject, ControlMsg } from "@/relay/relayClient";

/// EN: Presence settle window before CD may switch (spec §2.3 / §6). CN: CD 切换前的 presence 静默期（规范 §2.3/§6）。
export const CD_SETTLE_MS = 2000;

/// EN: Max `epoch_stale` retries before giving up (spec §3.3 / §6). CN: `epoch_stale` 最大重试次数（规范 §3.3/§6）。
export const MAX_COMMIT_RETRY = 5;

/// EN: `commit_intent` wait for CD ack (spec §6). CN: 等待 CD 回执的 `commit_intent` 超时（规范 §6）。
export const COMMIT_INTENT_TTL_MS = 30_000;

/// EN: Account-scoped relay convId (`s:<account>`). CN: 账户级 relay convId（`s:<account>`）。
export function accountSelfConvId(account: string): string {
  return `s:${account}`;
}

/// EN: Parse `s:<account>`; null if malformed. CN: 解析 `s:<account>`；格式不对返回 null。
export function accountFromSelfConv(convId: string): string | null {
  if (!convId.startsWith("s:")) return null;
  const acct = convId.slice(2);
  return acct.length > 0 ? acct : null;
}

/// EN: Deterministic CD = lexicographically smallest device id in the online set (spec §2.2).
/// CN: 确定性 CD = 在线集合中字典序最小的 device id（规范 §2.2）。
export function pickCoordinatorDevice(onlineDeviceIds: readonly string[]): string | null {
  if (onlineDeviceIds.length === 0) return null;
  const sorted = [...onlineDeviceIds].sort();
  return sorted[0] ?? null;
}

/// EN: True when `selfDeviceId` is the elected CD for `onlineDeviceIds`. CN: `selfDeviceId` 是否为
/// `onlineDeviceIds` 选出的 CD。
export function isCoordinatorDevice(onlineDeviceIds: readonly string[], selfDeviceId: string): boolean {
  const cd = pickCoordinatorDevice(onlineDeviceIds);
  return cd !== null && cd === selfDeviceId;
}

/// EN: Mutable election state with presence-settle (spec §2.3). CN: 带 presence 静默期的可变选举态（规范 §2.3）。
export interface CoordinatorElectionState {
  settledOnline: string[];
  pendingOnline: string[];
  pendingSinceMs: number;
}

export function initCoordinatorElection(selfDeviceId: string, nowMs = 0): CoordinatorElectionState {
  return {
    settledOnline: [selfDeviceId],
    pendingOnline: [selfDeviceId],
    pendingSinceMs: nowMs,
  };
}

export function applyPresenceUpdate(
  state: CoordinatorElectionState,
  deviceId: string,
  online: boolean,
  nowMs: number,
): CoordinatorElectionState {
  const pending = new Set(state.pendingOnline);
  if (online) pending.add(deviceId);
  else pending.delete(deviceId);
  const pendingOnline = [...pending].sort();
  const changed = pendingOnline.join("\0") !== state.pendingOnline.join("\0");
  if (!changed) return state;
  return { ...state, pendingOnline, pendingSinceMs: nowMs };
}

export function tickCoordinatorElection(
  state: CoordinatorElectionState,
  nowMs: number,
): CoordinatorElectionState {
  if (nowMs - state.pendingSinceMs < CD_SETTLE_MS) return state;
  if (state.settledOnline.join("\0") === state.pendingOnline.join("\0")) return state;
  return { ...state, settledOnline: state.pendingOnline };
}

export function currentCoordinator(state: CoordinatorElectionState): string | null {
  return pickCoordinatorDevice(state.settledOnline);
}

export function isSelfCoordinator(state: CoordinatorElectionState, selfDeviceId: string): boolean {
  return isCoordinatorDevice(state.settledOnline, selfDeviceId);
}

/// EN: Kinds of account-local Commit intents routed to CD (spec §2.4). CN: 经 CD 路由的账户内 Commit 意图类型（规范 §2.4）。
export type CommitIntentKind = "add_device" | "remove_device" | "rekey";

export interface CommitIntentPayload {
  /** EN: Target pairwise `d:` convId for the eventual MLS Commit. CN: 最终 MLS Commit 的目标 pairwise `d:` convId。 */
  dmConvId: string;
  /** EN: Base64 KeyPackage (`add_device`). CN: Base64 KeyPackage（`add_device`）。 */
  kp?: string;
  /** EN: Device / leaf hint (`remove_device`). CN: 设备 / leaf 提示（`remove_device`）。 */
  target?: string;
  /** EN: Account the Welcome should be delivered to. Defaults to self (sibling add). For a
   *  PEER-ASSISTED add (§3.8) the joining device belongs to the OTHER account, so this is set to the
   *  requester's account so the relay fans the Welcome to that account's devices. CN: Welcome 应投递到
   *  的账户。默认自身（兄弟 add）。**对端代 Add**（§3.8）的加入设备属于**对方**账户，故置为请求方账户，使
   *  relay 把 Welcome 扇出到该账户的设备。 */
  welcomeTo?: string;
}

/// EN: `s:<account>` presence frame (spec §2.3). CN: `s:<account>` presence 帧（规范 §2.3）。
export interface PresenceControlMsg {
  t: "presence";
  convId: string;
  device_id: string;
  online: boolean;
}

/// EN: Non-CD → CD intent (spec §2.4). CN: 非 CD → CD 意图（规范 §2.4）。
export interface CommitIntentControlMsg {
  t: "commit_intent";
  convId: string;
  from: string;
  req_id: string;
  kind: CommitIntentKind;
  payload: CommitIntentPayload;
}

/// EN: CD → requester result (spec §2.4). CN: CD → 请求方结果（规范 §2.4）。
export interface CommitResultControlMsg {
  t: "commit_result";
  convId: string;
  req_id: string;
  ok: boolean;
  reason?: "epoch_stale";
  current_epoch?: number;
}

/// EN: New device → account: "I'm a fresh device of this account; graft me into our existing 1:1s"
/// (spec §3.7 join trigger). Broadcast on `s:<account>`; only the elected CD acts. CN: 新设备 → 账户：
/// “我是本账户新设备，请把我嫁接进我们已有的 1:1”（规范 §3.7 加入触发）。在 `s:<account>` 广播；仅当选
/// CD 响应。
export interface DeviceJoinRequestControlMsg {
  t: "device_join_request";
  convId: string;
  device_id: string;
}

/// EN: CD → new device: the set of pairwise `d:` convs the account participates in; the new device
/// mints one KeyPackage per conv it still lacks. CN: CD → 新设备：本账户参与的 pairwise `d:` 会话集；
/// 新设备为每个仍缺失的会话各造一个 KeyPackage。
export interface DeviceJoinOfferControlMsg {
  t: "device_join_offer";
  convId: string;
  /** EN: Target new device id (only it should act). CN: 目标新设备 id（仅其响应）。 */
  device_id: string;
  conv_ids: string[];
}

/// EN: Conversation scope for device-join grafting (CHAT_GROUP_WIREIFY_DESIGN §15.2). `"dm"` =
/// 1:1 Wire (relay CAS ordering); `"group"` = group Wire (chain `expected_epoch` ordering). The
/// scope is also derivable from the conv id prefix (`g:` → group, else `dm`), so the field is an
/// optional hint that the parser back-fills via `scopeOfConv`. CN: 设备加入嫁接的会话作用域（设计
/// §15.2）。`"dm"` = 1:1 Wire（relay CAS 定序）；`"group"` = 群 Wire（链 `expected_epoch` 定序）。作用域
/// 亦可由 conv id 前缀推导（`g:` → group，否则 dm），故该字段是可选提示，解析器经 `scopeOfConv` 回填。
export type JoinScope = "dm" | "group";

/// EN: Derive the join scope from a conv id prefix (the authoritative fallback). CN: 由 conv id 前缀
/// 推导加入作用域（权威回退）。
export function scopeOfConv(convId: string): JoinScope {
  return convId.startsWith("g:") ? "group" : "dm";
}

/// EN: New device → CD: a fresh single-use KeyPackage per conv to be grafted. Each entry MAY carry a
/// `scope` hint; when absent the CD derives it from the conv id prefix. A single handshake can mix
/// 1:1 (`d:`) and group (`g:`) entries (graft all of an account's sessions at once). CN: 新设备 → CD：
/// 每个待嫁接会话各一个一次性 KeyPackage。每条**可**带 `scope` 提示；缺省时 CD 由 conv id 前缀推导。
/// 单次握手可混合 1:1（`d:`）与群（`g:`）条目（一次嫁接账户的全部会话）。
export interface DeviceJoinKpControlMsg {
  t: "device_join_kp";
  convId: string;
  /** EN: The new device providing the KeyPackages. CN: 提供 KeyPackage 的新设备。 */
  device_id: string;
  kps: Array<{ conv_id: string; kp: string; scope?: JoinScope }>;
}

export type AccountSelfControlMsg =
  | PresenceControlMsg
  | CommitIntentControlMsg
  | CommitResultControlMsg
  | DeviceJoinRequestControlMsg
  | DeviceJoinOfferControlMsg
  | DeviceJoinKpControlMsg;

export function buildPresence(args: {
  account: string;
  deviceId: string;
  online: boolean;
}): PresenceControlMsg {
  return {
    t: "presence",
    convId: accountSelfConvId(args.account),
    device_id: args.deviceId,
    online: args.online,
  };
}

export function buildCommitIntent(args: {
  account: string;
  from: string;
  reqId: string;
  kind: CommitIntentKind;
  payload: CommitIntentPayload;
}): CommitIntentControlMsg {
  return {
    t: "commit_intent",
    convId: accountSelfConvId(args.account),
    from: args.from,
    req_id: args.reqId,
    kind: args.kind,
    payload: args.payload,
  };
}

export function buildCommitResult(args: {
  account: string;
  reqId: string;
  ok: boolean;
  reason?: "epoch_stale";
  currentEpoch?: number;
}): CommitResultControlMsg {
  const out: CommitResultControlMsg = {
    t: "commit_result",
    convId: accountSelfConvId(args.account),
    req_id: args.reqId,
    ok: args.ok,
  };
  if (args.reason) out.reason = args.reason;
  if (args.currentEpoch != null) out.current_epoch = args.currentEpoch;
  return out;
}

export function buildDeviceJoinRequest(args: {
  account: string;
  deviceId: string;
}): DeviceJoinRequestControlMsg {
  return {
    t: "device_join_request",
    convId: accountSelfConvId(args.account),
    device_id: args.deviceId,
  };
}

export function buildDeviceJoinOffer(args: {
  account: string;
  deviceId: string;
  convIds: string[];
}): DeviceJoinOfferControlMsg {
  return {
    t: "device_join_offer",
    convId: accountSelfConvId(args.account),
    device_id: args.deviceId,
    conv_ids: args.convIds,
  };
}

export function buildDeviceJoinKp(args: {
  account: string;
  deviceId: string;
  kps: Array<{ conv_id: string; kp: string; scope?: JoinScope }>;
}): DeviceJoinKpControlMsg {
  return {
    t: "device_join_kp",
    convId: accountSelfConvId(args.account),
    device_id: args.deviceId,
    // EN: back-fill scope from the conv prefix so downstream never has to re-derive. CN: 由 conv 前缀
    // 回填 scope，使下游无需再推导。
    kps: args.kps.map((e) => ({ ...e, scope: e.scope ?? scopeOfConv(e.conv_id) })),
  };
}

function isRecord(x: unknown): x is Record<string, unknown> {
  return !!x && typeof x === "object";
}

export function parseAccountSelfControl(msg: ControlMsg): AccountSelfControlMsg | null {
  if (!("convId" in msg) || typeof msg.convId !== "string") return null;
  if (!accountFromSelfConv(msg.convId)) return null;
  const m = msg as Record<string, unknown>;
  if (m.t === "presence") {
    if (typeof m.device_id !== "string" || typeof m.online !== "boolean") return null;
    return { t: "presence", convId: msg.convId, device_id: m.device_id, online: m.online };
  }
  if (m.t === "commit_intent") {
    if (typeof m.from !== "string" || typeof m.req_id !== "string" || typeof m.kind !== "string") return null;
    if (!isRecord(m.payload) || typeof m.payload.dmConvId !== "string") return null;
    const kind = m.kind as CommitIntentKind;
    if (kind !== "add_device" && kind !== "remove_device" && kind !== "rekey") return null;
    return {
      t: "commit_intent",
      convId: msg.convId,
      from: m.from,
      req_id: m.req_id,
      kind,
      payload: {
        dmConvId: m.payload.dmConvId,
        kp: typeof m.payload.kp === "string" ? m.payload.kp : undefined,
        target: typeof m.payload.target === "string" ? m.payload.target : undefined,
        welcomeTo: typeof m.payload.welcomeTo === "string" ? m.payload.welcomeTo : undefined,
      },
    };
  }
  if (m.t === "commit_result") {
    if (typeof m.req_id !== "string" || typeof m.ok !== "boolean") return null;
    return {
      t: "commit_result",
      convId: msg.convId,
      req_id: m.req_id,
      ok: m.ok,
      reason: m.reason === "epoch_stale" ? "epoch_stale" : undefined,
      current_epoch: typeof m.current_epoch === "number" ? m.current_epoch : undefined,
    };
  }
  if (m.t === "device_join_request") {
    if (typeof m.device_id !== "string") return null;
    return { t: "device_join_request", convId: msg.convId, device_id: m.device_id };
  }
  if (m.t === "device_join_offer") {
    if (typeof m.device_id !== "string" || !Array.isArray(m.conv_ids)) return null;
    const convIds = m.conv_ids.filter((c): c is string => typeof c === "string");
    return { t: "device_join_offer", convId: msg.convId, device_id: m.device_id, conv_ids: convIds };
  }
  if (m.t === "device_join_kp") {
    if (typeof m.device_id !== "string" || !Array.isArray(m.kps)) return null;
    const kps = m.kps
      .filter(isRecord)
      .filter((e) => typeof e.conv_id === "string" && typeof e.kp === "string")
      .map((e) => {
        const conv_id = e.conv_id as string;
        const scope: JoinScope = e.scope === "group" || e.scope === "dm" ? e.scope : scopeOfConv(conv_id);
        return { conv_id, kp: e.kp as string, scope };
      });
    return { t: "device_join_kp", convId: msg.convId, device_id: m.device_id, kps };
  }
  return null;
}

/// EN: Fresh idempotency key for Wire Commits (spec §3.2). CN: Wire Commit 的幂等键（规范 §3.2）。
export function newCommitMsgId(): string {
  return globalThis.crypto?.randomUUID?.() ?? `cm-${Date.now()}-${Math.random().toString(36).slice(2)}`;
}

/// EN: Build a Gate-2-aware 1:1 Commit control frame (`commit_epoch` + `msgId`). Legacy handshakes
/// omit both fields and bypass relay CAS. CN: 构造闸二感知的 1:1 Commit 控制帧（`commit_epoch` + `msgId`）。
/// 旧握手省略两字段、绕过 relay CAS。
export function buildWireDmCommit(args: {
  from: string;
  convId: string;
  commitB64: string;
  commitEpoch: number;
  msgId?: string;
}): Extract<ControlMsg, { t: "commit" }> {
  return {
    t: "commit",
    from: args.from,
    convId: args.convId,
    commit: args.commitB64,
    commit_epoch: args.commitEpoch,
    msgId: args.msgId ?? newCommitMsgId(),
  };
}

/// EN: Track B subgroup state snapshot on `s:<account>` (relay fans to all account devices). Wire
/// multi-leaf does not emit this yet; callers reserve `msgId` for relay dedup. CN: 路线 B 子群状态快照
/// （`s:<account>`，relay 扇出到账户全部设备）。Wire 多 leaf 尚未发送；调用方保留 `msgId` 供 relay 去重。
export function buildWireNewDeviceState(args: {
  from: string;
  account: string;
  stateB64: string;
  msgId?: string;
}): Extract<ControlMsg, { t: "new_device_state" }> {
  return {
    t: "new_device_state",
    from: args.from,
    convId: accountSelfConvId(args.account),
    state: args.stateB64,
    msgId: args.msgId ?? newCommitMsgId(),
  };
}

/// EN: Relay `commit_reject` (Gate 2 loser, spec §3.3). CN: relay 的 `commit_reject`（闸二落败，规范 §3.3）。
export type { CommitReject } from "@/relay/relayClient";

export function parseCommitReject(raw: unknown): CommitReject | null {
  if (!isRecord(raw) || raw.type !== "commit_reject") return null;
  if (raw.reason !== "epoch_stale") return null;
  if (typeof raw.convId !== "string" || typeof raw.current_epoch !== "number") return null;
  return {
    reason: "epoch_stale",
    convId: raw.convId,
    current_epoch: raw.current_epoch,
    msgId: typeof raw.msgId === "string" ? raw.msgId : undefined,
  };
}

export function shouldRetryCommitReject(attempts: number): boolean {
  return attempts < MAX_COMMIT_RETRY;
}

/// EN: Route a local Commit intent: CD executes immediately; non-CD returns a wire intent for CD.
/// CN: 路由本地 Commit 意图：CD 立即执行；非 CD 返回发给 CD 的 wire intent。
export type CommitIntentRoute =
  | { action: "execute" }
  | { action: "delegate"; intent: CommitIntentControlMsg };

export function routeCommitIntent(args: {
  election: CoordinatorElectionState;
  selfDeviceId: string;
  account: string;
  endpointId: string;
  reqId: string;
  kind: CommitIntentKind;
  payload: CommitIntentPayload;
}): CommitIntentRoute {
  if (isSelfCoordinator(args.election, args.selfDeviceId)) {
    return { action: "execute" };
  }
  return {
    action: "delegate",
    intent: buildCommitIntent({
      account: args.account,
      from: args.endpointId,
      reqId: args.reqId,
      kind: args.kind,
      payload: args.payload,
    }),
  };
}
