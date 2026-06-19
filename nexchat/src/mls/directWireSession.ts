// EN: Session-level orchestrator for 1:1 Wire multi-leaf (CHAT_MULTIDEVICE_HYBRID_DESIGN §4,
// CHAT_1TO1_WIRE_COMMIT_SERIALIZATION_SPEC). Wires the real `add_device`/`rekey` `WireCommitExecutor`
// (OpenMlsEngine-backed) into the `DirectAccountCommitCoordinator` (Gate 1: CD election + intent
// routing) and manages presence broadcast over the relay lifecycle (online on connect, offline on
// disconnect/stop). Instantiate once per account session, alongside `DirectMlsRegistry`, when Wire
// multi-leaf is enabled.
//
// CN: 1:1 Wire 多 leaf 的会话级编排器（设计 §4、串行化规范）。把真实的 `add_device`/`rekey`
// `WireCommitExecutor`（OpenMlsEngine 支撑）接到 `DirectAccountCommitCoordinator`（闸一：CD 选举 + 意图
// 路由），并随 relay 生命周期管理 presence 广播（连接上线、断开/停止下线）。Wire 多 leaf 启用时与
// `DirectMlsRegistry` 并存，每账户会话实例化一次。

import {
  DirectAccountCommitCoordinator,
  type WireCommitExecutor,
} from "@/mls/directAccountCommitCoordinator";
import type {
  DeviceJoinKpControlMsg,
  DeviceJoinOfferControlMsg,
  DeviceJoinRequestControlMsg,
} from "@/mls/directCommitCoordination";
import {
  accountFromLeafIdentity,
  deviceFromLeafIdentity,
  directMlsKey,
  directMlsKeyInvolves,
} from "@/mls/directConv";
import { verifyLeafKeyBinding } from "@/mls/deviceLeafCredential";
import { createAddDeviceExecutor, type WireExecutorEngine } from "@/mls/directWireCommitExecutor";
import type { WireSessionJoinBridge } from "@/mls/accountWireCommitCoordinator";
import {
  bytesToHex,
  verifyIncomingCommit,
  type CommitInspectEngine,
} from "@/mls/followCommitGuard";
import { b64ToBytes, bytesToB64, type ControlMsg, type RelayClient } from "@/relay/relayClient";
import { canonicalAddress } from "@/wallet/address";

/// EN: Engine surface the session needs ON TOP of the executor's staged-commit subset: minting a
/// single-use KeyPackage for a sibling graft, and processing the resulting Welcome to actually join.
/// CN: 会话在执行器 staged-commit 子集**之外**还需的引擎接口：为兄弟嫁接造一次性 KeyPackage，并处理
/// 随后的 Welcome 以真正加入。
export interface WireSessionEngine extends WireExecutorEngine, CommitInspectEngine {
  generateKeyPackage(): Uint8Array;
  processWelcomeByConv(convId: string, welcome: Uint8Array): Promise<void>;
  processCommitByConv(convId: string, commit: Uint8Array): void;
  /** EN: Parse + validate a KeyPackage → leaf identity, leaf signature key, and embedded E2EI binding
   *  (§3.9; binding empty if none). Used to verify account ownership of a graft candidate straight
   *  from the KeyPackage. CN: 解析并校验 KeyPackage → leaf identity、leaf 签名钥、嵌入的 E2EI 绑定
   *  （§3.9；无则空）。用于直接从 KeyPackage 验证嫁接候选的账户归属。 */
  keyPackageBinding?(keyPackage: Uint8Array): {
    identity: string;
    signatureKey: Uint8Array;
    binding: Uint8Array;
  };
}

export interface DirectWireSessionDeps {
  /** EN: OpenMLS engine (staged-commit + KeyPackage/Welcome surface). CN: OpenMLS 引擎（staged-commit + KeyPackage/Welcome 接口）。 */
  engine: WireSessionEngine;
  relay: RelayClient;
  /** EN: My account address (= MLS leaf credential identity, the `s:<account>` channel key).
   *  CN: 我的账户地址（= MLS leaf 凭证 identity，`s:<account>` 通道键）。 */
  selfAddress: string;
  /** EN: Stable per-device id (signing-key fingerprint) used for CD election. CN: 稳定的设备级 id
   *  （签名钥指纹），用于 CD 选举。 */
  deviceId: string;
  /** EN: Relay endpoint id (frame `from`). CN: relay 端点 id（帧 `from`）。 */
  endpointId: string;
  /** EN: CD-side enumeration of the pairwise `d:` convs this account currently participates in
   *  (i.e. groups the local engine holds). Used to build the join offer. CN: CD 侧枚举本账户当前参与的
   *  pairwise `d:` 会话（即本地引擎持有的群），用于构造 join offer。 */
  listJoinableConvs?: () => string[];
  /** EN: New device: the CD offered these convs → they are owned by the graft path; the host (appStore)
   *  must tell `DirectMlsRegistry` to stop managing them (`markGraftManaged`) so the per-conv handshake
   *  never forks the multi-leaf group. Fired on every offer (idempotent downstream). CN: 新设备：CD
   *  提供了这些会话 → 由嫁接路径拥有；宿主（appStore）须告知 `DirectMlsRegistry` 停止管理它们
   *  （`markGraftManaged`），使 1:1 握手不再分叉多 leaf 群。每次 offer 都触发（下游幂等）。 */
  onGraftConvs?: (convIds: string[]) => void;
  /** EN: Fired ONCE when the join phase settles — either an offer arrived (`graftConvs` = offered) or
   *  the fallback window elapsed with no CD (`graftConvs` = []). The host then starts the normal
   *  pairwise handshake for the REST of the roster (convs NOT graft-owned). CN: join 阶段安定时**触发
   *  一次**——收到 offer（`graftConvs` = 已提供）或回退窗口内无 CD（`graftConvs` = []）。宿主据此对
   *  roster 其余（非嫁接拥有）会话发起常规 1:1 握手。 */
  onJoinSettled?: (graftConvs: string[]) => void;
  /** EN: Fallback window (ms) to wait for a CD offer before assuming we are the first device (no
   *  sibling) and settling with no grafts. CN: 等待 CD offer 的回退窗口（ms）；超时则认定我们是首设备
   *  （无兄弟），以零嫁接安定。 */
  joinSettleMs?: number;
  /** EN: Peer-assisted Add (§3.8) cold-start fallback: when neither party (nor any sibling) yet holds
   *  the wire 1:1 group, the peer cannot graft us (`onPeerAddReq` drops on `!hasGroup`), so a lone
   *  `requestPeerAdd` would deadlock. After this window with no graft, fire `onPeerAddTimeout` so the
   *  host cold-establishes the group via the deterministic pairwise handshake (same wire engine).
   *  CN: 对端代 Add（§3.8）冷启动回退：当双方（及兄弟）都尚未持有该 wire 1:1 群时，对端无法嫁接我们
   *  （`onPeerAddReq` 在 `!hasGroup` 时丢弃），单发 `requestPeerAdd` 会死锁。此窗口内未被嫁接则触发
   *  `onPeerAddTimeout`，由宿主用确定性 1:1 握手（同一 wire 引擎）冷启动建群。 */
  onPeerAddTimeout?: (peerAccount: string, conv: string) => void;
  /** EN: Override the peer-assist cold-start fallback window. CN: 覆盖对端代 Add 冷启动回退窗口。 */
  peerAddFallbackMs?: number;
  /** EN: Override the implicit-accept settle window. CN: 覆盖隐式采纳静默窗口。 */
  settleMs?: number;
  /** EN: Inject a custom executor (tests). Defaults to the real `createAddDeviceExecutor`.
   *  CN: 注入自定义执行器（测试）。默认真实 `createAddDeviceExecutor`。 */
  executor?: WireCommitExecutor;
  /** EN: Use an existing account coordinator (dual Wire mode). When set, this session does NOT own
   *  coordinator lifecycle (wire/stop/presence) unless `ownsCoordinator` is true. CN: 复用已有账户协调器
   *  （双 Wire 模式）。设置时本 session **不**拥有协调器生命周期（wire/stop/presence），除非 `ownsCoordinator`
   *  为 true。 */
  coordinator?: DirectAccountCommitCoordinator;
  /** EN: When false (default with injected `coordinator`), skip coordinator wire/stop/presence. CN: 为 false
   *  （注入 `coordinator` 时默认）则跳过协调器 wire/stop/presence。 */
  ownsCoordinator?: boolean;
  /** EN: When false, `announceJoin` arms the settle timer but does NOT broadcast `device_join_request`
   *  (the unified coordinator sends once). Default true. CN: 为 false 时 `announceJoin` 仅武装 settle 计时器、
   *  **不**广播 `device_join_request`（由统一协调器发一次）。默认 true。 */
  broadcastDeviceJoinRequest?: boolean;
}

const DEFAULT_JOIN_SETTLE_MS = 3000;
const DEFAULT_PEER_ADD_FALLBACK_MS = 3500;

export class DirectWireSession {
  private readonly coordinator: DirectAccountCommitCoordinator;
  private readonly ownsCoordinator: boolean;
  private readonly broadcastDeviceJoinRequest: boolean;
  private readonly deps: DirectWireSessionDeps;
  private started = false;
  /** EN: convs this (new) device asked to be grafted into and is awaiting a Welcome for. CN: 本（新）
   *  设备请求嫁接、正等待 Welcome 的会话集。 */
  private pendingGrafts = new Set<string>();
  /** EN: convs joined via graft → follow their ongoing Commits (rekey/add/remove) to stay in epoch
   *  sync, since the registry no longer manages them. CN: 经嫁接加入的会话 → 跟随其后续 Commit
   *  （rekey/add/remove）以保持 epoch 同步，因 registry 不再管理它们。 */
  private graftedConvs = new Set<string>();
  private joinSettled = false;
  private joinSettleTimer: ReturnType<typeof setTimeout> | null = null;
  /** EN: serializes grafted-conv Commit following so member-side E2EI re-verification (§3.9) cannot
   *  reorder epochs across the verification `await`. CN: 串行化嫁接会话的 Commit 跟随，使成员侧 E2EI
   *  复验（§3.9）的 `await` 期间不会打乱 epoch 顺序。 */
  private graftCommitQueue: Promise<void> = Promise.resolve();
  /** EN: per-conv peer-assist cold-start fallback timers (cleared on graft/stop). CN: 按会话的对端代
   *  Add 冷启动回退计时器（嫁接/停止时清除）。 */
  private peerAddTimers = new Map<string, ReturnType<typeof setTimeout>>();

  constructor(deps: DirectWireSessionDeps) {
    this.deps = deps;
    this.ownsCoordinator = deps.ownsCoordinator ?? !deps.coordinator;
    this.broadcastDeviceJoinRequest = deps.broadcastDeviceJoinRequest ?? this.ownsCoordinator;
    if (deps.coordinator) {
      this.coordinator = deps.coordinator;
    } else {
      const executor =
        deps.executor ??
        createAddDeviceExecutor({
          engine: deps.engine,
          relay: deps.relay,
          endpointId: deps.endpointId,
          selfAddress: deps.selfAddress,
        });
      this.coordinator = new DirectAccountCommitCoordinator({
        relay: deps.relay,
        account: deps.selfAddress,
        deviceId: deps.deviceId,
        endpointId: deps.endpointId,
        executor,
        settleMs: deps.settleMs,
        onDeviceJoinRequest: (m) => this.onDeviceJoinRequest(m),
        onDeviceJoinOffer: (m) => this.onDeviceJoinOffer(m),
        onDeviceJoinKp: (m) => this.onDeviceJoinKp(m),
      });
    }
  }

  /// EN: Join bridge for the unified account coordinator (dual Wire mode). CN: 统一账户协调器的 join 桥
  /// （双 Wire 模式）。
  joinBridge(): WireSessionJoinBridge {
    return {
      listJoinableConvs: () =>
        (this.deps.listJoinableConvs?.() ?? []).filter((c) => c.startsWith("d:")),
      handleDeviceJoinOffer: (m) => this.handleDeviceJoinOffer(m),
      handleDeviceJoinKp: (m) => this.handleDeviceJoinKp(m),
    };
  }

  /// EN: Subscribe to relay control + (if already connected) broadcast presence online. CN: 订阅
  /// relay 控制面 +（若已连接）广播 presence 上线。
  start(connected = true): void {
    if (this.started) return;
    this.started = true;
    if (this.ownsCoordinator) this.coordinator.wire();
    this.deps.relay.onControl((m) => this.onGraftControl(m));
    if (connected && this.ownsCoordinator) this.coordinator.onRelayConnected();
  }

  /// EN: New device: broadcast a graft request so the account's CD grafts us into existing 1:1s, and
  /// arm the fallback settle timer (fires `onJoinSettled([])` if no CD answers → we are the first
  /// device). Idempotent — convs we already hold are filtered out when the offer arrives. CN: 新设备：
  /// 广播嫁接请求，让账户 CD 把我们接进已有 1:1，并武装回退安定计时器（无 CD 应答则 `onJoinSettled([])`
  /// → 我们是首设备）。幂等——offer 到达时会过滤掉我们已持有的会话。
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

  /// EN: Settle the join phase exactly once → host starts normal handshakes for the non-graft roster.
  /// CN: join 阶段安定一次 → 宿主对非嫁接 roster 发起常规握手。
  private settleJoin(graftConvs: string[]): void {
    if (this.joinSettleTimer) {
      clearTimeout(this.joinSettleTimer);
      this.joinSettleTimer = null;
    }
    if (this.joinSettled) return;
    this.joinSettled = true;
    this.deps.onJoinSettled?.(graftConvs);
  }

  /// EN: CD: a sibling wants in → offer the convs we participate in. CN: CD：兄弟设备请求加入 → 提供我们
  /// 参与的会话。
  private async onDeviceJoinRequest(msg: DeviceJoinRequestControlMsg): Promise<void> {
    const convs = this.deps.listJoinableConvs?.() ?? [];
    if (convs.length === 0) return;
    await this.coordinator.sendDeviceJoinOffer(msg.device_id, convs);
  }

  /// EN: New device: every offered **1:1** conv is now graft-owned; mint KPs and settle join. CN: 新设备：
  /// 每个被提供的 **1:1** 会话现归嫁接拥有；铸造 KP 并安定 join。
  async handleDeviceJoinOffer(msg: DeviceJoinOfferControlMsg): Promise<void> {
    const dmConvs = msg.conv_ids.filter((c) => c.startsWith("d:"));
    if (dmConvs.length > 0) this.deps.onGraftConvs?.(dmConvs);
    this.settleJoin(dmConvs);
    const need = dmConvs.filter((c) => !this.deps.engine.hasGroup(c));
    if (need.length === 0) return;
    const kps: Array<{ conv_id: string; kp: string }> = [];
    for (const conv of need) {
      this.pendingGrafts.add(conv);
      kps.push({ conv_id: conv, kp: bytesToB64(this.deps.engine.generateKeyPackage()) });
    }
    await this.coordinator.sendDeviceJoinKp(kps);
  }

  /// EN: CD: graft the new device into each offered **1:1** conv. CN: CD：把新设备接进每个被提供的 **1:1** 会话。
  async handleDeviceJoinKp(msg: DeviceJoinKpControlMsg): Promise<void> {
    const self = canonicalAddress(this.deps.selfAddress);
    for (const { conv_id, kp } of msg.kps) {
      if (!conv_id.startsWith("d:")) continue;
      if (!this.deps.engine.hasGroup(conv_id)) continue;
      if ((await this.kpInMlsBinding(b64ToBytes(kp), self)) === "invalid") continue;
      await this.coordinator.submitIntent({ kind: "add_device", payload: { dmConvId: conv_id, kp } });
    }
  }

  private async onDeviceJoinOffer(msg: DeviceJoinOfferControlMsg): Promise<void> {
    return this.handleDeviceJoinOffer(msg);
  }

  private async onDeviceJoinKp(msg: DeviceJoinKpControlMsg): Promise<void> {
    return this.handleDeviceJoinKp(msg);
  }

  /// EN: For graft-owned convs the registry steps aside, so the session owns them end-to-end: consume
  /// the join Welcome, then follow every subsequent Commit to stay in epoch sync. CN: 对嫁接拥有的会话
  /// registry 退场，故由会话端到端拥有：消费加入 Welcome，随后跟随每条 Commit 保持 epoch 同步。
  private onGraftControl(m: ControlMsg): void {
    if (m.t === "peer_add_req") {
      void this.onPeerAddReq(m);
      return;
    }
    if (m.t === "welcome") {
      const conv = m.convId;
      if (!this.pendingGrafts.has(conv)) return;
      if (m.toAddr !== this.deps.selfAddress && m.to !== this.deps.endpointId) return;
      if (this.deps.engine.hasGroup(conv)) {
        this.markGrafted(conv);
        return;
      }
      void this.deps.engine
        .processWelcomeByConv(conv, b64ToBytes(m.welcome))
        .then(() => {
          this.markGrafted(conv);
        })
        .catch((e) => {
          console.warn("[nexchat] Wire graft welcome failed:", e instanceof Error ? e.message : e);
        });
      return;
    }
    if (m.t === "commit") {
      // EN: follow Commits only for convs we joined via graft (the registry no longer applies them).
      // The Commit that added us is already reflected by our Welcome → re-applying it throws; ignore.
      // Serialized through `graftCommitQueue` because member-side E2EI re-verification (§3.9) awaits.
      // CN: 仅对经嫁接加入的会话跟随 Commit（registry 不再应用）。把我们加入的那条 Commit 已由 Welcome
      // 反映 → 重复应用会抛错；忽略之。经 `graftCommitQueue` 串行化，因成员侧 E2EI 复验（§3.9）需 await。
      if (!this.graftedConvs.has(m.convId)) return;
      const conv = m.convId;
      const commitBytes = b64ToBytes(m.commit);
      this.graftCommitQueue = this.graftCommitQueue.then(async () => {
        if (!(await verifyIncomingCommit(this.deps.engine, conv, commitBytes))) {
          console.warn("[nexchat] Wire graft: rejected Commit adding an unverifiable leaf (§3.9)");
          return;
        }
        try {
          this.deps.engine.processCommitByConv(conv, commitBytes);
        } catch {
          /* already applied (e.g. our own join commit) */
        }
      });
    }
  }

  /// EN: Adopt a 1:1 wire group we ALREADY hold locally (restored from persistence) as graft-owned,
  /// WITHOUT requesting a redundant peer-assisted Add. On a fresh process the in-memory `graftedConvs`
  /// set is empty even though the engine still holds the group from IndexedDB; the no-sibling join
  /// planner would then route this established conv to `requestPeerAdd`, which (a) never marks it ready
  /// (no registry coordinator + not graft-managed → `isReady` stays false → banner stuck forever) and
  /// (b) makes the peer re-graft a fresh leaf into a group we are already a member of, bumping the epoch
  /// while our `welcome` handler only `markGrafted`s (it skips processing when `hasGroup`) → the two
  /// sides desync ("can send but peer never receives"). Adopting instead: follow future commits, tell
  /// the host the registry may report ready (`hasGroup`), and let the peer-assist path serve ONLY the
  /// party that genuinely lacks the group (it requests, we — holding the group — graft it). Returns true
  /// iff the group was present and is now adopted. CN: 把本地**已持有**（持久化恢复）的 1:1 wire 群当作
  /// 嫁接拥有，而**不发**冗余的对端代 Add。新进程下内存 `graftedConvs` 为空，但引擎仍从 IndexedDB 持有该群；
  /// 无兄弟 join 规划器会把这个已建立的会话路由到 `requestPeerAdd`——它 (a) 永不标就绪（无 registry 协调器
  /// 且非 graft-managed → `isReady` 恒为 false → 横幅永久卡住），(b) 让对端把一个新 leaf 重新嫁接进我们
  /// 已在的群、抬升 epoch，而我们的 `welcome` 处理器在 `hasGroup` 时只 `markGrafted`（跳过处理）→ 双方
  /// epoch 错位（「能发但对方永远收不到」）。改为采纳：跟随后续 commit、告知宿主 registry 可按 `hasGroup`
  /// 报告就绪，并让对端代 Add 路径**只**服务真正缺群的一方（它请求，我们持群方嫁接它）。仅当本地确有该群
  /// 并完成采纳时返回 true。
  adoptRestoredGroup(conv: string): boolean {
    if (!this.deps.engine.hasGroup(conv)) return false;
    if (!this.graftedConvs.has(conv)) this.markGrafted(conv);
    return true;
  }

  /// EN: A conv is now joined via graft → follow its future commits AND tell the host so the registry
  /// stops managing it (no fork), regardless of whether the graft came from a sibling or a peer.
  /// CN: 某会话已经嫁接加入 → 跟随其后续 commit，并告知宿主让 registry 停止管理它（不分叉），无论嫁接来
  /// 自兄弟还是对端。
  private markGrafted(conv: string): void {
    this.pendingGrafts.delete(conv);
    this.clearPeerAddTimer(conv);
    if (!this.graftedConvs.has(conv)) {
      this.graftedConvs.add(conv);
      this.deps.onGraftConvs?.([conv]);
    }
  }

  /// EN: Relay-trustless check of a KeyPackage's in-MLS account binding (§3.9), shared by every
  /// relay-only add path. Returns "valid" (the KP carries a binding signed by `expectedAccount`'s SS58
  /// key over its stable leaf key), "invalid" (a binding is carried but is forged / mismatched, OR the
  /// KP is malformed → the caller MUST drop the KP), or "absent" (no in-MLS binding → cross-account
  /// peer-assisted Add rejects it, same-account join-trigger allows it). A compromised relay that injects
  /// a foreign KeyPackage cannot forge the account signature, so it always lands on "invalid". CN:
  /// relay-trustless 校验 KeyPackage 的 MLS 内账户绑定（§3.9），供所有 relay-only 加入路径复用。返回
  /// "valid"（携带由 `expectedAccount` 的 SS58 钥对其稳定 leaf key 的签名）、"invalid"（携带但伪造/不匹配，
  /// 或 KP 畸形 → 调用方必须丢弃）、或 "absent"（未携带 → 跨账户对端代 Add 拒绝、同账户 join-trigger 放行）。
  /// 被攻陷的 relay 注入外来 KeyPackage 无法伪造账户签名，故必落 "invalid"。
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
      return "invalid"; // malformed / unvalidatable KeyPackage
    }
  }

  /// EN: Peer side of peer-assisted Add (§3.8): a device of the OTHER account asks us to graft its
  /// leaf into our shared 1:1. Verify (a) the relay-stamped authenticated sender equals the claimed
  /// requester, (b) the requester is the other party of the conv, (c) we hold the group — then run a
  /// serialized `add_device` whose Welcome is delivered to the requester's account. CN: 对端代 Add
  /// （§3.8）的对端侧：对方账户的某设备请求我们把其 leaf 接进共享 1:1。校验 (a) relay 盖章的认证发送者等于
  /// 声称的请求方、(b) 请求方是会话另一方、(c) 我们持有该群——然后跑串行化 `add_device`，其 Welcome 投递
  /// 到请求方账户。
  private async onPeerAddReq(m: Extract<ControlMsg, { t: "peer_add_req" }>): Promise<void> {
    const requester = canonicalAddress(m.requester_account);
    // (a) anti-impersonation: the relay stamps the authenticated sender; it MUST match the claim.
    if (!m._senderAccount || canonicalAddress(m._senderAccount) !== requester) {
      console.info("[nexchat][wire] peer_add_req drop: unauthenticated/forged sender", {
        conv: m.convId,
        stamped: m._senderAccount,
        claimed: requester,
      });
      return;
    }
    // (b) the requester must be the OTHER party of this conv, and it must not be us.
    if (requester === canonicalAddress(this.deps.selfAddress)) return;
    if (!directMlsKeyInvolves(m.convId, this.deps.selfAddress)) return;
    if (!directMlsKeyInvolves(m.convId, requester)) return;
    // (c) we can only graft into a group we actually hold.
    if (!this.deps.engine.hasGroup(m.convId)) {
      console.info("[nexchat][wire] peer_add_req drop: no local wire group (cold start)", {
        conv: m.convId,
        requester,
      });
      return;
    }
    console.info("[nexchat][wire] peer_add_req accepted → graft", { conv: m.convId, requester });
    // (d) E2EI device-leaf credential (§3.9): the KeyPackage MUST carry a relay-trustless in-MLS account
    // binding (signed by the requester's SS58 key over its stable leaf key), else we never graft it. This
    // is the cross-account path, so we REQUIRE the binding ("valid"): a malformed/forged binding
    // ("invalid") or a missing one ("absent", e.g. a compromised relay injecting a foreign unbound KP) is
    // dropped. (The legacy request-level `cred` fallback was retired once every engine embeds the in-MLS
    // binding.) CN: E2EI 设备 leaf 凭证（§3.9）：KeyPackage 必须携带 relay-trustless 的 MLS 内账户绑定
    // （由请求方 SS58 钥对其稳定 leaf key 签名），否则绝不嫁接。这是跨账户路径，故**要求**绑定（"valid"）：
    // 畸形/伪造（"invalid"）或缺失（"absent"，如被攻陷 relay 注入的外来无绑定 KP）一律丢弃。（全引擎嵌入
    // MLS 内绑定后，旧的请求级 `cred` 回退已退役。）
    const kpBytes = b64ToBytes(m.kp);
    if ((await this.kpInMlsBinding(kpBytes, requester)) !== "valid") return;
    await this.coordinator.submitIntent({
      kind: "add_device",
      payload: { dmConvId: m.convId, kp: m.kp, welcomeTo: requester },
    });
  }

  /// EN: Relay (re)connected → re-announce presence online. CN: relay（重）连 → 重新广播上线。
  onRelayConnected(): void {
    if (this.ownsCoordinator) this.coordinator.onRelayConnected();
  }

  /// EN: Relay disconnected → announce presence offline (best-effort). CN: relay 断开 → 广播下线
  /// （尽力而为）。
  onRelayDisconnected(): void {
    if (this.ownsCoordinator) void this.coordinator.publishPresence(false);
  }

  /// EN: Peer-assisted Add (§3.8): ask `peerAccount` (the other party of our 1:1) to graft THIS new
  /// device's leaf into the existing group. Used when none of my own (sibling) devices are online to
  /// graft me. Mints a fresh single-use KeyPackage, registers the conv as a pending graft (so the
  /// resulting Welcome is consumed), and sends an authenticated request over the pairwise channel.
  /// CN: 对端代 Add（§3.8）：请求 `peerAccount`（我们 1:1 的另一方）把本新设备的 leaf 接进已有群。当我
  /// 自己（兄弟）设备都不在线、无法嫁接我时使用。造一次性 KeyPackage、把会话登记为待嫁接（以消费随后的
  /// Welcome），并经 pairwise 通道发出认证请求。
  async requestPeerAdd(peerAccount: string): Promise<string> {
    const conv = directMlsKey(this.deps.selfAddress, peerAccount);
    this.pendingGrafts.add(conv);
    const kp = bytesToB64(this.deps.engine.generateKeyPackage());
    const self = canonicalAddress(this.deps.selfAddress);
    // EN: arm the cold-start fallback — if no graft Welcome arrives in time (peer holds no group yet,
    // e.g. first-ever wire establishment for a pre-existing 1:1), hand off to the host so it
    // cold-establishes via the deterministic pairwise handshake. Cleared by `markGrafted`. CN: 武装冷启动
    // 回退——若窗口内未收到嫁接 Welcome（对端尚无该群，如既有 1:1 的首次 wire 建群），交回宿主用确定性
    // 1:1 握手冷启动建群。被 `markGrafted` 清除。
    this.clearPeerAddTimer(conv);
    this.peerAddTimers.set(
      conv,
      setTimeout(() => {
        this.peerAddTimers.delete(conv);
        // EN: resolved ONLY when a graft Welcome actually landed (`markGrafted` → `graftedConvs`). A bare
        // `hasGroup(conv)` is NOT enough: a stale/half-established group persisted locally (e.g. a prior
        // engine, or a graft that never converged) would otherwise be mistaken for success and silently
        // skip the cold-start — leaving the conv un-ready forever (banner stuck + inbound frames dropped
        // as "handshake incomplete"). When not grafted, fall through to `onPeerAddTimeout`.
        // CN: 仅当嫁接 Welcome 确实落地（`markGrafted` → `graftedConvs`）才算解决。单凭 `hasGroup(conv)` 不
        // 够：本地残留的过期/半建立群（如旧引擎遗留，或从未收敛的嫁接）会被误判为成功而静默跳过冷启动——
        // 使该会话永远不就绪（横幅常驻 + 入站帧以「握手未完成」被丢弃）。未嫁接时继续走 `onPeerAddTimeout`。
        if (this.graftedConvs.has(conv)) {
          console.info("[nexchat][wire] peer-assist resolved before fallback", { conv });
          return;
        }
        // EN: stop treating this as a pending graft so a late Welcome cannot double-establish (fork)
        // alongside the handshake the host is about to start. CN: 不再当作待嫁接，避免迟到 Welcome 与
        // 宿主即将发起的握手重复建群（分叉）。
        this.pendingGrafts.delete(conv);
        console.info("[nexchat][wire] peer-assist timeout → cold-start fallback", { conv, peer: peerAccount });
        this.deps.onPeerAddTimeout?.(peerAccount, conv);
      }, this.deps.peerAddFallbackMs ?? DEFAULT_PEER_ADD_FALLBACK_MS),
    );
    // EN: E2EI binding (§3.9) rides INSIDE the KeyPackage's leaf node (installed at engine init via
    // `setLeafBinding`), so the peer verifies ownership relay-trustlessly straight from the KP — no
    // separate request-level credential is sent. CN: E2EI 绑定（§3.9）驻留在 KeyPackage 的 leaf 节点内
    // （引擎 init 时经 `setLeafBinding` 装入），对端直接从 KP 做 relay-trustless 归属验证——不再单发请求级凭证。
    await this.deps.relay.sendControl({
      t: "peer_add_req",
      from: this.deps.endpointId,
      convId: conv,
      requester_account: self,
      device_id: this.deps.deviceId,
      kp,
    });
    console.info("[nexchat][wire] peer_add_req sent", { conv, peer: peerAccount });
    return conv;
  }

  private clearPeerAddTimer(conv: string): void {
    const t = this.peerAddTimers.get(conv);
    if (t) {
      clearTimeout(t);
      this.peerAddTimers.delete(conv);
    }
  }

  /// EN: Add one of MY new devices to an existing 1:1 (graft its leaf). `dmConvId` is the canonical
  /// `d:{a}:{b}` MLS key; `keyPackageB64` is the new device's published KeyPackage. CN: 把我的某个
  /// 新设备加入已有 1:1（嫁接其 leaf）。`dmConvId` 为规范 `d:{a}:{b}` MLS 键；`keyPackageB64` 为新设备
  /// 已发布的 KeyPackage。
  async addDevice(dmConvId: string, keyPackageB64: string): Promise<"execute" | "delegated"> {
    return this.coordinator.submitIntent({ kind: "add_device", payload: { dmConvId, kp: keyPackageB64 } });
  }

  /// EN: Rekey (self-update) my leaf in a 1:1 for forward secrecy. CN: 在某 1:1 中对我的 leaf 做
  /// rekey（自更新）以获前向保密。
  async rekey(dmConvId: string): Promise<"execute" | "delegated"> {
    return this.coordinator.submitIntent({ kind: "rekey", payload: { dmConvId } });
  }

  /// EN: Remove one of my devices' leaves from a 1:1 (per-device PCS). `targetDeviceIdentity` is the
  /// device-distinct credential `{account}#{deviceId}` (see `deviceLeafIdentity`). CN: 从某 1:1 移除
  /// 我某设备的 leaf（按设备 PCS）。`targetDeviceIdentity` 为设备区分凭证 `{account}#{deviceId}`
  /// （见 `deviceLeafIdentity`）。
  async removeDevice(dmConvId: string, targetDeviceIdentity: string): Promise<"execute" | "delegated"> {
    return this.coordinator.submitIntent({
      kind: "remove_device",
      payload: { dmConvId, target: targetDeviceIdentity },
    });
  }

  isCoordinator(): boolean {
    return this.coordinator.isCoordinator();
  }

  /// EN: Tear down: stop election ticker + broadcast presence offline. CN: 拆除：停选举 ticker +
  /// 广播 presence 下线。
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
}
