// EN: Session-level orchestrator for group Wire-ification (CHAT_GROUP_WIREIFY_DESIGN §6/§7/§15.3, G3c).
// Group-side twin of `DirectWireSession` (1:1 Wire). It reuses `DirectAccountCommitCoordinator` for the
// RELAY-side machinery that is identical to 1:1 — CD election, presence, and the `s:<account>`
// device-join handshake (request → offer → kp) — but swaps the COMMIT ORDERING backend: a group does
// NOT serialize via the relay's off-chain `commit_slot` CAS; it serializes via the chain
// `pallet-chat-group::commit` `expected_epoch` total order (`GroupCommitOrderingDriver` +
// `createChainSubmitGroupCommit`). The coordinator is therefore constructed WITHOUT an executor: its
// `onExecuteIntent` hook routes every CD-resolved intent (locally submitted OR delegated over relay)
// into the chain ordering driver instead of the relay CAS lifecycle.
//
// Scope is per device-op: `add_device` grafts one of MY new devices' leaves into a group I am in,
// `rekey` self-updates my leaf (forward secrecy), `remove_device` evicts one of my device leaves
// (per-device PCS). All three are EMPTY-`member_delta` chain commits (the account roster is unchanged).
// A follower applies an incoming group commit only after `verifyIncomingGroupCommit` re-checks every
// added leaf's E2EI binding against current membership (§6.4), relay-/chain-trustlessly.
//
// CN: 群 Wire 化的会话级编排器（设计 §6/§7/§15.3，G3c）。`DirectWireSession`（1:1 Wire）的群侧孪生。它复用
// `DirectAccountCommitCoordinator` 中与 1:1 相同的 **relay 侧**机制——CD 选举、presence、`s:<account>` 设备
// join 握手（request → offer → kp）——但替换 **commit 定序**后端：群**不**经 relay 链下 `commit_slot` CAS 定序，
// 而经链上 `pallet-chat-group::commit` 的 `expected_epoch` 全序（`GroupCommitOrderingDriver` +
// `createChainSubmitGroupCommit`）。故协调器**不带** executor 构造：其 `onExecuteIntent` 钩子把每个 CD 裁定的
// 意图（本地提交**或**经 relay 委托）路由进链定序驱动，而非 relay CAS 生命周期。
//
// 作用域按设备操作：`add_device` 把我某新设备的 leaf 嫁接进我所在的群、`rekey` 自更新我的 leaf（前向保密）、
// `remove_device` 驱逐我某设备 leaf（按设备 PCS）。三者均为**空-`member_delta`** 链 commit（账户名册不变）。
// 跟随者仅在 `verifyIncomingGroupCommit` 按当前成员资格复验每个被加 leaf 的 E2EI 绑定（§6.4）后，才应用进入的
// 群 commit，relay-/链-trustless。

import {
  DirectAccountCommitCoordinator,
  type WireCommitExecutor,
} from "@/mls/directAccountCommitCoordinator";
import {
  createChainSubmitGroupCommit,
  type GroupCommitChain,
} from "@/mls/chainSubmitGroupCommit";
import type {
  CommitIntentControlMsg,
  DeviceJoinKpControlMsg,
  DeviceJoinOfferControlMsg,
  DeviceJoinRequestControlMsg,
} from "@/mls/directCommitCoordination";
import {
  accountFromLeafIdentity,
  deviceFromLeafIdentity,
} from "@/mls/directConv";
import { verifyLeafKeyBinding } from "@/mls/deviceLeafCredential";
import { createGroupDeviceExecutor } from "@/mls/groupDeviceCommitExecutor";
import {
  GroupCommitOrderingDriver,
  type GroupCommitOutcome,
} from "@/mls/groupCommitOrderingDriver";
import { planWireGroupJoin, type WireGroupJoinPlan } from "@/mls/wireGroupJoinPlan";
import type { WireExecutorEngine } from "@/mls/directWireCommitExecutor";
import {
  bytesToHex,
  verifyIncomingGroupCommit,
  type CommitInspectEngine,
} from "@/mls/followCommitGuard";
import { b64ToBytes, bytesToB64, type ControlMsg, type RelayClient } from "@/relay/relayClient";
import type { WireSessionJoinBridge } from "@/mls/accountWireCommitCoordinator";
import { canonicalAddress } from "@/wallet/address";

/// EN: Engine surface a group Wire session needs: the staged-commit executor subset, member-side commit
/// inspection (§6.4), the G3b staged fingerprint reader (for the chain submit), plus KeyPackage /
/// Welcome / Commit application for the graft + follow paths. CN: 群 Wire 会话所需引擎接口：staged-commit
/// 执行器子集、成员侧 commit 检视（§6.4）、G3b 暂存指纹读取（供链提交），以及嫁接 + 跟随路径的 KeyPackage /
/// Welcome / Commit 应用。
export interface GroupWireSessionEngine extends WireExecutorEngine, CommitInspectEngine {
  stagedCommitFingerprintByConv(convKey: string): {
    treeHash: Uint8Array;
    transcriptHash: Uint8Array;
    epoch: number;
  };
  generateKeyPackage(): Uint8Array;
  processWelcomeByConv(convId: string, welcome: Uint8Array): Promise<void>;
  processCommitByConv(convId: string, commit: Uint8Array): void;
  keyPackageBinding?(keyPackage: Uint8Array): {
    identity: string;
    signatureKey: Uint8Array;
    binding: Uint8Array;
  };
  /** EN: Current leaf identities (`{account}#{device}`) in a group; used by peer-add (§8.4) to skip a
   *  device already grafted (idempotency / race reducer). CN: 群当前 leaf identity（`{account}#{device}`）；
   *  供 peer-add（§8.4）跳过已嫁接设备（幂等 / 防撞）。 */
  memberIdentities?(convKey: string): string[];
}

export interface GroupWireSessionDeps {
  engine: GroupWireSessionEngine;
  relay: RelayClient;
  /** EN: Chain surface for the ordering CAS (`pallet-chat-group::commit`). CN: 定序 CAS 的链接口
   *  （`pallet-chat-group::commit`）。 */
  chain: GroupCommitChain;
  /** EN: My account address (= the `s:<account>` channel key, base of the device leaf identity).
   *  CN: 我的账户地址（= `s:<account>` 通道键，设备 leaf identity 的基）。 */
  selfAddress: string;
  /** EN: Stable per-device id (signing-key fingerprint) for CD election. CN: 稳定设备级 id（签名钥指纹），
   *  用于 CD 选举。 */
  deviceId: string;
  /** EN: Relay endpoint id (frame `from`). CN: relay 端点 id（帧 `from`）。 */
  endpointId: string;
  /** EN: Pull + apply winning chain commits so the local group reaches `toEpoch` after an EpochStale
   *  loss (used by the executor's `catchUpAndRerun`). CN: EpochStale 落败后拉取并应用胜出链上 commit，使
   *  本地群追平到 `toEpoch`（供执行器 `catchUpAndRerun`）。 */
  syncGroupEpoch?: (convId: string, toEpoch: number) => Promise<void>;
  /** EN: Group membership predicate for member-side E2EI re-verification (§6.4): is `account` a current
   *  member of `convId`? Backed by the chain `GroupMembers` set / local roster. CN: 成员侧 E2EI 复验
   *  （§6.4）的成员判定：`account` 是否为 `convId` 当前成员？由链上 `GroupMembers` / 本地名册支撑。 */
  isGroupMember?: (convId: string, account: string) => boolean;
  /** EN: CD-side enumeration of group convs (`g:<id>`) this account participates in (the local engine
   *  holds), used to build the device-join offer. CN: CD 侧枚举本账户参与的群会话（`g:<id>`，本地引擎持有），
   *  用于构造设备 join offer。 */
  listJoinableGroups?: () => string[];
  /** EN: Fired ONCE when the join phase settles (offer arrived → its convs; or fallback window elapsed →
   *  []). CN: join 阶段安定时**触发一次**（收到 offer → 其会话；或回退窗口超时 → []）。 */
  onJoinSettled?: (graftConvs: string[]) => void;
  /** EN: Lazy/on-demand Add (§8.1): is a group ACTIVE now (opened / recently messaged)? Active groups are
   *  grafted immediately on offer; dormant ones are DEFERRED and grafted when `activateGroup` is later
   *  called. Absent → eager (every offered group joins now), preserving prior behavior. CN: 延迟/按需 Add
   *  （§8.1）：某群当前是否**活跃**（打开 / 近期发言）？活跃群在 offer 时立即嫁接；休眠群**延迟**，待之后
   *  `activateGroup` 时嫁接。缺省 → 急加载（每个被提供群现在都加入），保持原行为。 */
  isGroupActive?: (convId: string) => boolean;
  /** EN: Observe the join plan computed on each offer (telemetry / tests). CN: 观察每次 offer 计算出的
   *  join 计划（遥测 / 测试）。 */
  onJoinPlanned?: (plan: WireGroupJoinPlan) => void;
  /** EN: Fired when a graft Welcome lands and the local engine holds the group (UI refresh hook). CN:
   *  嫁接 Welcome 落地且本地引擎已持群时触发（供 UI 刷新）。 */
  onGroupGrafted?: (convId: string) => void;
  /** EN: Peer-add (§8.4) cold-start fallback: a new device asked existing GROUP members to graft it via
   *  `requestGroupPeerAdd`, but no member grafted it within the window (none online / none holds the
   *  group). The host can then surface "waiting for a member" or fall back to External Commit (§8.4, an
   *  orthogonal optional capability). CN: peer-add（§8.4）冷启动回退：新设备经 `requestGroupPeerAdd` 请求
   *  既有**群成员**代为嫁接，但窗口内无人嫁接（无人在线 / 无人持群）。宿主可提示「等待成员」或回退到 External
   *  Commit（§8.4，正交可选能力）。 */
  onPeerAddTimeout?: (groupConvId: string) => void;
  /** EN: Override the peer-add cold-start fallback window (ms). CN: 覆盖 peer-add 冷启动回退窗口（ms）。 */
  peerAddFallbackMs?: number;
  /** EN: Fallback window (ms) to wait for a CD offer before settling as the first device. CN: 等待 CD
   *  offer 的回退窗口（ms）；超时则以首设备安定。 */
  joinSettleMs?: number;
  /** EN: Max EpochStale retries per op (driver bound). CN: 每操作 EpochStale 重试上限（驱动有界）。 */
  maxRetries?: number;
  /** EN: Inject a custom executor / driver / submit (tests). CN: 注入自定义 executor / driver / submit
   *  （测试）。 */
  executor?: WireCommitExecutor;
  driver?: GroupCommitOrderingDriver;
  /** EN: Use an existing account coordinator (dual Wire mode). CN: 复用已有账户协调器（双 Wire 模式）。 */
  coordinator?: DirectAccountCommitCoordinator;
  /** EN: When false (default with injected `coordinator`), skip coordinator wire/stop/presence. CN: 为 false
   *  （注入 `coordinator` 时默认）则跳过协调器 wire/stop/presence。 */
  ownsCoordinator?: boolean;
  /** EN: When false, `announceJoin` does NOT broadcast `device_join_request`. Default true. CN: 为 false 时
   *  `announceJoin` **不**广播 `device_join_request`。默认 true。 */
  broadcastDeviceJoinRequest?: boolean;
}

const DEFAULT_JOIN_SETTLE_MS = 3000;
const DEFAULT_PEER_ADD_FALLBACK_MS = 5000;

export class GroupWireSession {
  private readonly coordinator: DirectAccountCommitCoordinator;
  private readonly driver: GroupCommitOrderingDriver;
  private readonly ownsCoordinator: boolean;
  private readonly broadcastDeviceJoinRequest: boolean;
  private readonly deps: GroupWireSessionDeps;
  private started = false;
  /** EN: group convs this (new) device asked to be grafted into and awaits a Welcome for. CN: 本（新）
   *  设备请求嫁接、正等待 Welcome 的群会话集。 */
  private pendingGrafts = new Set<string>();
  /** EN: offered-but-dormant groups deferred by the §8.1 plan → grafted lazily on `activateGroup`. CN:
   *  被 §8.1 计划延迟的「已提供但休眠」群 → `activateGroup` 时懒嫁接。 */
  private deferredGroups = new Set<string>();
  /** EN: per-group peer-add (§8.4) cold-start fallback timers (cleared on graft / stop). CN: 按群的
   *  peer-add（§8.4）冷启动回退计时器（嫁接 / stop 时清除）。 */
  private peerAddTimers = new Map<string, ReturnType<typeof setTimeout>>();
  private joinSettled = false;
  private joinSettleTimer: ReturnType<typeof setTimeout> | null = null;

  constructor(deps: GroupWireSessionDeps) {
    this.deps = deps;
    this.ownsCoordinator = deps.ownsCoordinator ?? !deps.coordinator;
    this.broadcastDeviceJoinRequest = deps.broadcastDeviceJoinRequest ?? this.ownsCoordinator;
    const executor =
      deps.executor ??
      createGroupDeviceExecutor({
        engine: deps.engine,
        relay: deps.relay,
        endpointId: deps.endpointId,
        selfAddress: deps.selfAddress,
        syncGroupEpoch: deps.syncGroupEpoch,
      });
    this.driver =
      deps.driver ??
      new GroupCommitOrderingDriver({
        executor,
        submitGroupCommit: createChainSubmitGroupCommit({
          chain: deps.chain,
          engine: deps.engine,
        }),
        maxRetries: deps.maxRetries,
      });
    if (deps.coordinator) {
      this.coordinator = deps.coordinator;
    } else {
      this.coordinator = new DirectAccountCommitCoordinator({
        relay: deps.relay,
        account: deps.selfAddress,
        deviceId: deps.deviceId,
        endpointId: deps.endpointId,
        onExecuteIntent: (intent) => this.runViaDriver(intent),
        onDeviceJoinRequest: (m) => this.onDeviceJoinRequest(m),
        onDeviceJoinOffer: (m) => this.onDeviceJoinOffer(m),
        onDeviceJoinKp: (m) => this.onDeviceJoinKp(m),
      });
    }
  }

  /// EN: Join bridge for the unified account coordinator (dual Wire mode). CN: 统一账户协调器的 join 桥。
  joinBridge(): WireSessionJoinBridge {
    return {
      listJoinableConvs: () =>
        (this.deps.listJoinableGroups?.() ?? []).filter((c) => c.startsWith("g:")),
      handleDeviceJoinOffer: (m) => this.handleDeviceJoinOffer(m),
      handleDeviceJoinKp: (m) => this.handleDeviceJoinKp(m),
    };
  }

  /// EN: Group-side CD intent execution (for unified coordinator routing). CN: 群侧 CD 意图执行（供统一协调器路由）。
  async handleExecuteIntent(intent: CommitIntentControlMsg): Promise<void> {
    return this.runViaDriver(intent);
  }

  /// EN: Subscribe to relay control + (if connected) broadcast presence online; also receive graft
  /// Welcomes. CN: 订阅 relay 控制面 +（若已连接）广播 presence 上线；并接收嫁接 Welcome。
  start(connected = true): void {
    if (this.started) return;
    this.started = true;
    if (this.ownsCoordinator) this.coordinator.wire();
    this.deps.relay.onControl((m) => this.onGraftControl(m));
    if (connected && this.ownsCoordinator) this.coordinator.onRelayConnected();
  }

  /// EN: Tear down: stop election + timers + presence offline. CN: 拆除：停选举 + 计时器 + presence 下线。
  stop(): void {
    if (!this.started) return;
    this.started = false;
    if (this.joinSettleTimer) {
      clearTimeout(this.joinSettleTimer);
      this.joinSettleTimer = null;
    }
    for (const t of this.peerAddTimers.values()) clearTimeout(t);
    this.peerAddTimers.clear();
    if (this.ownsCoordinator) this.coordinator.stop();
  }

  onRelayConnected(): void {
    if (this.ownsCoordinator) this.coordinator.onRelayConnected();
  }

  onRelayDisconnected(): void {
    if (this.ownsCoordinator) void this.coordinator.publishPresence(false);
  }

  isCoordinator(): boolean {
    return this.coordinator.isCoordinator();
  }

  // ----------------------------------------------------------- public ops ----

  /// EN: Add one of MY new devices to a group (graft its leaf). `groupConvId` = `g:<id>`. CN: 把我某新
  /// 设备加入群（嫁接其 leaf）。`groupConvId` = `g:<id>`。
  async addDevice(groupConvId: string, keyPackageB64: string): Promise<"execute" | "delegated"> {
    return this.coordinator.submitIntent({
      kind: "add_device",
      payload: { dmConvId: groupConvId, kp: keyPackageB64 },
    });
  }

  /// EN: Rekey (self-update) my leaf in a group for forward secrecy. CN: 在群中对我的 leaf 做 rekey。
  async rekey(groupConvId: string): Promise<"execute" | "delegated"> {
    return this.coordinator.submitIntent({ kind: "rekey", payload: { dmConvId: groupConvId } });
  }

  /// EN: Remove one of my device leaves from a group (per-device PCS). `targetDeviceIdentity` =
  /// `{account}#{deviceId}`. CN: 从群移除我某设备 leaf（按设备 PCS）。`targetDeviceIdentity` =
  /// `{account}#{deviceId}`。
  async removeDevice(
    groupConvId: string,
    targetDeviceIdentity: string,
  ): Promise<"execute" | "delegated"> {
    return this.coordinator.submitIntent({
      kind: "remove_device",
      payload: { dmConvId: groupConvId, target: targetDeviceIdentity },
    });
  }

  // ----------------------------------------------------------- join phase ----

  /// EN: New device: broadcast a graft request so the account's CD grafts us into the groups it is in,
  /// and arm the fallback settle timer. CN: 新设备：广播嫁接请求让账户 CD 把我们接进它所在的群，并武装回退
  /// 安定计时器。
  async announceJoin(): Promise<void> {
    if (!this.joinSettled && !this.joinSettleTimer) {
      this.joinSettleTimer = setTimeout(
        () => this.settleJoin([]),
        this.deps.joinSettleMs ?? DEFAULT_JOIN_SETTLE_MS,
      );
    }
    if (this.broadcastDeviceJoinRequest) {
      await this.coordinator.sendDeviceJoinRequest();
    }
  }

  private settleJoin(graftConvs: string[]): void {
    if (this.joinSettleTimer) {
      clearTimeout(this.joinSettleTimer);
      this.joinSettleTimer = null;
    }
    if (this.joinSettled) return;
    this.joinSettled = true;
    this.deps.onJoinSettled?.(graftConvs);
  }

  /// EN: CD: a sibling wants in → offer the GROUP convs we participate in. CN: CD：兄弟设备请求加入 →
  /// 提供我们参与的**群**会话。
  private async onDeviceJoinRequest(msg: DeviceJoinRequestControlMsg): Promise<void> {
    const convs = (this.deps.listJoinableGroups?.() ?? []).filter((c) => c.startsWith("g:"));
    if (convs.length === 0) return;
    await this.coordinator.sendDeviceJoinOffer(msg.device_id, convs);
  }

  /// EN: New device: split offered groups via the §8.1 lazy/on-demand plan — graft ACTIVE groups now
  /// (mint a KeyPackage each), DEFER dormant ones for `activateGroup`; settle the join phase. CN: 新设备：
  /// 用 §8.1 延迟/按需计划切分被提供群——**活跃**群现在嫁接（各造一个 KeyPackage），**休眠**群延迟交
  /// `activateGroup`；安定 join 阶段。
  async handleDeviceJoinOffer(msg: DeviceJoinOfferControlMsg): Promise<void> {
    const groups = msg.conv_ids.filter((c) => c.startsWith("g:"));
    this.settleJoin(groups);
    const plan = planWireGroupJoin({
      offeredGroups: groups,
      isHeld: (c) => this.deps.engine.hasGroup(c),
      isActive: (c) => this.deps.isGroupActive?.(c) ?? true,
    });
    this.deps.onJoinPlanned?.(plan);
    for (const conv of plan.defer) this.deferredGroups.add(conv);
    if (plan.joinNow.length === 0) return;
    const kps: Array<{ conv_id: string; kp: string }> = [];
    for (const conv of plan.joinNow) {
      this.pendingGrafts.add(conv);
      this.deferredGroups.delete(conv);
      kps.push({ conv_id: conv, kp: bytesToB64(this.deps.engine.generateKeyPackage()) });
    }
    await this.coordinator.sendDeviceJoinKp(kps);
  }

  async handleDeviceJoinKp(msg: DeviceJoinKpControlMsg): Promise<void> {
    const self = canonicalAddress(this.deps.selfAddress);
    for (const entry of msg.kps) {
      if (entry.scope && entry.scope !== "group") continue;
      const conv = entry.conv_id;
      if (!conv.startsWith("g:")) continue;
      if (!this.deps.engine.hasGroup(conv)) continue;
      if ((await this.kpInMlsBinding(b64ToBytes(entry.kp), self)) === "invalid") continue;
      await this.coordinator.submitIntent({
        kind: "add_device",
        payload: { dmConvId: conv, kp: entry.kp },
      });
    }
  }

  private async onDeviceJoinOffer(msg: DeviceJoinOfferControlMsg): Promise<void> {
    return this.handleDeviceJoinOffer(msg);
  }

  private async onDeviceJoinKp(msg: DeviceJoinKpControlMsg): Promise<void> {
    return this.handleDeviceJoinKp(msg);
  }

  /// EN: §8.1 on-demand graft: a previously-deferred (dormant) group became ACTIVE (opened / messaged) →
  /// mint a KeyPackage and request a graft for JUST that group now. No-op if it was not deferred or is
  /// already held. CN: §8.1 按需嫁接：此前被延迟（休眠）的群变为**活跃**（打开 / 发言）→ 现在仅为该群铸造
  /// KeyPackage 并请求嫁接。未被延迟或已持有则空操作。
  async activateGroup(convId: string): Promise<void> {
    if (!convId.startsWith("g:")) return;
    if (!this.deferredGroups.delete(convId)) return; // not deferred → nothing to do
    if (this.deps.engine.hasGroup(convId)) return; // already a leaf
    this.pendingGrafts.add(convId);
    await this.coordinator.sendDeviceJoinKp([
      { conv_id: convId, kp: bytesToB64(this.deps.engine.generateKeyPackage()) },
    ]);
  }

  /// EN: §8.1 deferred CD graft OR §8.4 peer-add fallback — ensure this device is being grafted into
  /// `convId`. Tries sibling CD lazy graft first; if not deferred / still not held, broadcasts peer-add
  /// once (idempotent while a graft is already pending). CN: §8.1 延迟 CD 嫁接 **或** §8.4 peer-add 回退——
  /// 确保本设备正被接入 `convId`。先尝试兄弟 CD 懒嫁接；若非延迟 / 仍未持群，则广播 peer-add（已有待嫁接时幂等）。
  async ensureGraftOrPeerAdd(convId: string): Promise<void> {
    if (!convId.startsWith("g:")) return;
    if (this.deps.engine.hasGroup(convId)) return;
    await this.activateGroup(convId);
    if (this.deps.engine.hasGroup(convId)) return;
    if (this.pendingGrafts.has(convId)) return;
    await this.requestGroupPeerAdd(convId);
  }

  /// EN: New device: consume the graft Welcome delivered to our account over `s:<account>`. CN: 新设备：
  /// 消费经 `s:<account>` 投给我们账户的嫁接 Welcome。
  private onGraftControl(m: ControlMsg): void {
    if (m.t === "peer_add_req") {
      void this.onPeerAddReq(m);
      return;
    }
    if (m.t !== "welcome") return;
    const conv = m.convId;
    if (!conv.startsWith("g:")) return;
    if (!this.pendingGrafts.has(conv)) return;
    if (m.toAddr !== this.deps.selfAddress && m.to !== this.deps.endpointId) return;
    this.clearPeerAddTimer(conv);
    if (this.deps.engine.hasGroup(conv)) {
      this.pendingGrafts.delete(conv);
      return;
    }
    void this.deps.engine
      .processWelcomeByConv(conv, b64ToBytes(m.welcome))
      .then(() => {
        this.pendingGrafts.delete(conv);
        this.deps.onGroupGrafted?.(conv);
      })
      .catch((e) => {
        console.warn("[nexchat][group-wire] graft welcome failed:", e instanceof Error ? e.message : e);
      });
  }

  // --------------------------------------------------- peer-add (§8.4) ----

  /// EN: New device, no sibling/CD online to graft me → ask EXISTING members of `groupConvId` to graft
  /// this device's leaf (group twin of 1:1 `requestPeerAdd`, SERIALIZATION_SPEC §3.8). Mint a fresh
  /// single-use KeyPackage carrying my in-MLS E2EI binding (§6.4), register the conv as a pending graft
  /// (so the resulting Welcome over `s:<account>` is consumed), and broadcast an authenticated
  /// `peer_add_req`. Arms a cold-start fallback: if no member grafts us in time, fire `onPeerAddTimeout`
  /// (host may wait / fall back to External Commit). CN: 新设备、无在线兄弟/CD 嫁接我 → 请求 `groupConvId`
  /// 的**既有成员**代为嫁接本设备 leaf（1:1 `requestPeerAdd` 的群侧孪生，规范 §3.8）。造携带我 MLS 内 E2EI
  /// 绑定（§6.4）的一次性 KeyPackage、把会话登记为待嫁接（以消费随后经 `s:<account>` 的 Welcome），并广播认证
  /// 的 `peer_add_req`。武装冷启动回退：窗口内无人嫁接则触发 `onPeerAddTimeout`（宿主可等待 / 回退 External Commit）。
  async requestGroupPeerAdd(groupConvId: string): Promise<void> {
    if (!groupConvId.startsWith("g:")) {
      throw new Error(`requestGroupPeerAdd: expected a group conv (g:…), got ${groupConvId}`);
    }
    this.pendingGrafts.add(groupConvId);
    const kp = bytesToB64(this.deps.engine.generateKeyPackage());
    const self = canonicalAddress(this.deps.selfAddress);
    this.clearPeerAddTimer(groupConvId);
    this.peerAddTimers.set(
      groupConvId,
      setTimeout(() => {
        this.peerAddTimers.delete(groupConvId);
        // EN: resolved only if a graft Welcome actually landed (we now hold the group). CN: 仅当嫁接
        // Welcome 确实落地（已持群）才算解决。
        if (this.deps.engine.hasGroup(groupConvId)) return;
        this.pendingGrafts.delete(groupConvId);
        console.info("[nexchat][group-wire] peer-add timeout → host fallback", { conv: groupConvId });
        this.deps.onPeerAddTimeout?.(groupConvId);
      }, this.deps.peerAddFallbackMs ?? DEFAULT_PEER_ADD_FALLBACK_MS),
    );
    await this.deps.relay.sendControl({
      t: "peer_add_req",
      from: this.deps.endpointId,
      convId: groupConvId,
      requester_account: self,
      device_id: this.deps.deviceId,
      kp,
    });
  }

  /// EN: Member side of group peer-add (§8.4): another account's device asks us to graft its leaf into a
  /// GROUP we are in. Authorize relay-/chain-trustlessly before committing: (a) the relay-stamped
  /// authenticated sender equals the claimed requester (anti-impersonation); (b) the requester is not us;
  /// (c) it is a group conv we actually hold; (d) the requester account is ALREADY a current member of the
  /// group (peer-add grafts a NEW DEVICE of an existing member — it MUST NOT smuggle a new account in);
  /// (e) the KeyPackage carries a VALID in-MLS E2EI binding signed by the requester's account key (§6.4),
  /// else a compromised relay could inject a foreign leaf. Idempotent: skip if that device leaf is already
  /// present. Then run a chain-ordered `add_device` whose Welcome is delivered to the requester's account.
  /// CN: 群 peer-add（§8.4）的成员侧：别的账户的设备请求我们把其 leaf 接进我们所在的**群**。提交前 relay-/链-
  /// trustless 鉴权：(a) relay 盖章认证发送者 == 声称请求方（防冒充）；(b) 请求方非我；(c) 是我们确实持有的群
  /// 会话；(d) 请求方账户**已是**该群当前成员（peer-add 接的是既有成员的**新设备**——**严禁**夹带新账户）；
  /// (e) KeyPackage 携带由请求方账户钥签名的**有效** MLS 内 E2EI 绑定（§6.4），否则被攻陷 relay 可注入外来
  /// leaf。幂等：该设备 leaf 已在则跳过。随后跑链定序 `add_device`，其 Welcome 投递到请求方账户。
  private async onPeerAddReq(m: Extract<ControlMsg, { t: "peer_add_req" }>): Promise<void> {
    const conv = m.convId;
    if (!conv.startsWith("g:")) return; // 1:1 peer-add is owned by DirectWireSession
    const requester = canonicalAddress(m.requester_account);
    // (a) anti-impersonation: the relay stamps the authenticated sender; it MUST match the claim.
    if (!m._senderAccount || canonicalAddress(m._senderAccount) !== requester) {
      console.info("[nexchat][group-wire] peer_add_req drop: unauthenticated/forged sender", {
        conv,
        stamped: m._senderAccount,
        claimed: requester,
      });
      return;
    }
    // (b) the requester must not be us.
    if (requester === canonicalAddress(this.deps.selfAddress)) return;
    // (c) we can only graft into a group we actually hold.
    if (!this.deps.engine.hasGroup(conv)) return;
    // (d) GROUP authz: the requester account must ALREADY be a member (graft a device of an existing
    // member, never a new account). Absent a predicate we fail CLOSED (cannot prove membership → drop).
    if (!(this.deps.isGroupMember?.(conv, requester) ?? false)) {
      console.info("[nexchat][group-wire] peer_add_req drop: requester is not a group member", {
        conv,
        requester,
      });
      return;
    }
    // (e) E2EI device-leaf credential (§6.4): cross-account path REQUIRES a valid in-MLS binding.
    const kpBytes = b64ToBytes(m.kp);
    if ((await this.kpInMlsBinding(kpBytes, requester)) !== "valid") {
      console.info("[nexchat][group-wire] peer_add_req drop: KeyPackage E2EI binding not valid", { conv });
      return;
    }
    // idempotency / race reducer: if this device leaf is already grafted, do nothing (another member or a
    // prior round already added it; the chain CAS still serializes any true concurrency).
    const info = this.deps.engine.keyPackageBinding?.(kpBytes);
    if (info && this.deps.engine.memberIdentities?.(conv)?.includes(info.identity)) return;
    await this.coordinator.submitIntent({
      kind: "add_device",
      payload: { dmConvId: conv, kp: m.kp, welcomeTo: requester },
    });
  }

  private clearPeerAddTimer(conv: string): void {
    const t = this.peerAddTimers.get(conv);
    if (t) {
      clearTimeout(t);
      this.peerAddTimers.delete(conv);
    }
  }

  // -------------------------------------------------------- member follow ----

  /// EN: Follower path: verify an incoming group Commit's added leaves (§6.4) BEFORE applying it. Returns
  /// `true` if applied (verified + processed), `false` if rejected (an added leaf fails E2EI / is a
  /// non-member) or if processing threw (already applied / stale). Drives `verifyIncomingGroupCommit`
  /// with the `isGroupMember` predicate. CN: 跟随者路径：应用进入的群 Commit **前**复验其被加 leaf（§6.4）。
  /// 应用（已验证 + 已处理）返回 `true`；拒绝（被加 leaf E2EI 失败 / 非成员）或处理抛错（已应用 / 过期）返回
  /// `false`。以 `isGroupMember` 谓词驱动 `verifyIncomingGroupCommit`。
  async followGroupCommit(convId: string, commit: Uint8Array): Promise<boolean> {
    const isMember = (acct: string) =>
      this.deps.isGroupMember?.(convId, acct) ?? true;
    const ok = await verifyIncomingGroupCommit(this.deps.engine, convId, commit, isMember);
    if (!ok) {
      console.warn("[nexchat][group-wire] rejected Commit adding an unverifiable leaf (§6.4)");
      return false;
    }
    try {
      this.deps.engine.processCommitByConv(convId, commit);
      return true;
    } catch {
      return false; // already applied (e.g. our own commit) / stale
    }
  }

  // ----------------------------------------------------------------- util ----

  /// EN: Route a CD-resolved intent through the chain ordering driver, then reply the result to the
  /// (possibly delegating) requester over `s:<account>`. CN: 把 CD 裁定的意图经链定序驱动路由，再经
  /// `s:<account>` 把结果回执给（可能是委托方的）请求方。
  private async runViaDriver(intent: CommitIntentControlMsg): Promise<void> {
    let outcome: GroupCommitOutcome;
    try {
      outcome = await this.driver.run(intent);
    } catch (e) {
      console.warn("[nexchat][group-wire] driver run failed:", e);
      await this.coordinator.replyIntentResult(intent.req_id, false);
      return;
    }
    if (outcome.ok) {
      await this.coordinator.replyIntentResult(intent.req_id, true);
    } else if (outcome.reason === "epoch_stale_exhausted") {
      await this.coordinator.replyIntentResult(intent.req_id, false, {
        reason: "epoch_stale",
        currentEpoch: outcome.finalEpoch,
      });
    } else {
      await this.coordinator.replyIntentResult(intent.req_id, false);
    }
  }

  /// EN: Relay-trustless check of a KeyPackage's in-MLS account binding (§6.4 / §3.9), shared with the
  /// 1:1 path semantics. "valid" = binding signed by `expectedAccount`'s SS58 key over its leaf key;
  /// "invalid" = carried but forged / mismatched / malformed (caller MUST drop); "absent" = no binding
  /// (same-account join-trigger allows it). CN: relay-trustless 校验 KeyPackage 的 MLS 内账户绑定
  /// （§6.4 / §3.9），语义与 1:1 一致。"valid"=由 `expectedAccount` 的 SS58 钥对其 leaf key 签名；
  /// "invalid"=携带但伪造/不匹配/畸形（调用方必须丢弃）；"absent"=无绑定（同账户 join-trigger 放行）。
  private async kpInMlsBinding(
    kpBytes: Uint8Array,
    expectedAccount: string,
  ): Promise<"valid" | "invalid" | "absent"> {
    const probe = this.deps.engine.keyPackageBinding;
    if (!probe) return "absent";
    try {
      const info = probe.call(this.deps.engine, kpBytes);
      if (info.binding.length === 0) return "absent";
      const acct = accountFromLeafIdentity(info.identity);
      const ok =
        acct === expectedAccount &&
        (await verifyLeafKeyBinding(
          acct,
          deviceFromLeafIdentity(info.identity),
          info.signatureKey,
          bytesToHex(info.binding),
        ));
      return ok ? "valid" : "invalid";
    } catch {
      return "invalid";
    }
  }
}
