// EN: Gate 1 runtime skeleton — wires account self-channel presence + commit_intent routing
// into a live RelayClient. CD election uses `directCommitCoordination` pure functions; execution
// of MLS Commits remains in the Wire multi-leaf layer (H2/H3). Instantiate once per account session
// alongside DirectMlsRegistry when Wire multi-leaf is enabled.
// CN: 闸一运行时骨架——把账户自通道 presence + commit_intent 路由接到 live RelayClient。CD 选举用
// `directCommitCoordination` 纯函数；MLS Commit 执行仍在 Wire 多 leaf 层（H2/H3）。Wire 多 leaf 启用时
// 与 DirectMlsRegistry 并存，每账户会话实例化一次。

import {
  accountFromSelfConv,
  applyPresenceUpdate,
  buildCommitIntent,
  buildCommitResult,
  buildDeviceJoinKp,
  buildDeviceJoinOffer,
  buildDeviceJoinRequest,
  buildPresence,
  buildWireDmCommit,
  currentCoordinator,
  initCoordinatorElection,
  isSelfCoordinator,
  newCommitMsgId,
  parseAccountSelfControl,
  routeCommitIntent,
  tickCoordinatorElection,
  type CommitIntentControlMsg,
  type CommitIntentKind,
  type CommitIntentPayload,
  type CoordinatorElectionState,
  type DeviceJoinKpControlMsg,
  type DeviceJoinOfferControlMsg,
  type DeviceJoinRequestControlMsg,
} from "@/mls/directCommitCoordination";
import {
  reduceCommit,
  startCommit,
  type CommitAction,
  type CommitLifecycleState,
} from "@/mls/directWireCommitLifecycle";
import type { CommitReject, ControlMsg, RelayClient } from "@/relay/relayClient";

/// EN: MLS-facing operations the CD lifecycle driver delegates to (the real OpenMLS Wire
/// multi-leaf execution; injected because it lives in the H2/H3 layer). PURE-relay coordinator ↔
/// impure MLS engine boundary. CN: CD 生命周期驱动委托的 MLS 侧操作（真实 OpenMLS Wire 多 leaf 执行；
/// 注入，因其属 H2/H3 层）。纯 relay 协调器 ↔ 有副作用 MLS 引擎的边界。
export interface WireCommitExecutor {
  /** EN: Run the OpenMLS op for an intent as a STAGED commit (NOT merged): returns serialized
   *  commit/welcome + the PRE-op epoch (which is also the still-current local epoch, since staging
   *  does not advance it) for the wire `commit_epoch`. Merge happens only via `commitAccepted`.
   *  CN: 把意图的 OpenMLS 操作跑成 STAGED commit（**不合并**）：返回序列化 commit/welcome + 操作前 epoch
   *  （也是当前本地 epoch，暂存不推进 epoch）作 wire `commit_epoch`。合并仅经 `commitAccepted`。 */
  runIntent(intent: CommitIntentControlMsg): Promise<{ commitB64: string; welcomeB64: string; preEpoch: number }>;
  /** EN: Relay ACCEPTed the `(conv, epoch)` slot → merge the staged commit into local state.
   *  CN: relay 已 ACCEPT 该 `(conv, epoch)` 槽位 → 把暂存 commit 合并进本地状态。 */
  commitAccepted(intent: CommitIntentControlMsg): Promise<void>;
  /** EN: Give up on this attempt → discard any staged commit so the group is not left pending.
   *  CN: 放弃本次尝试 → 丢弃暂存 commit，避免群停留在 pending 态。 */
  commitAbandoned(intent: CommitIntentControlMsg): Promise<void>;
  /** EN: On EPOCH_STALE: discard our forked staged commit, catch the local group up to `toEpoch`,
   *  then re-stage the op. Clearing is always safe and prevents a permanent 1:1 fork.
   *  CN: EPOCH_STALE 时：丢弃我方分叉暂存 commit，把本地群追平到 `toEpoch`，再重新暂存该操作。
   *  清除永远安全，可防 1:1 永久分叉。 */
  catchUpAndRerun(
    intent: CommitIntentControlMsg,
    toEpoch: number,
  ): Promise<{ commitB64: string; welcomeB64: string; newEpoch: number }>;
  /** EN: Deliver the Welcome to the joining device (add_device); no-op for remove/rekey.
   *  CN: 向加入设备投递 Welcome（add_device）；remove/rekey 为空操作。 */
  deliverWelcome(intent: CommitIntentControlMsg, welcomeB64: string): Promise<void>;
  /** EN: OPTIONAL — actively ask the relay to re-deliver `convId`'s stored winning Commit(s) so a lost
   *  CAS race catches up deterministically (instead of relying on incidental fan-out). Fired once per
   *  catch-up cycle; the re-delivered Commit is applied by `DirectMlsRegistry` over the normal control
   *  path, after which the bounded poll succeeds (spec §3.3). CN: 可选——主动请求 relay 重投 `convId`
   *  已存的胜出 Commit，使落败 CAS 确定性追平（不依赖偶发扇出）。每次追平周期触发一次；重投的 Commit 由
   *  `DirectMlsRegistry` 经正常控制面应用，之后有界轮询即成功（规范 §3.3）。 */
  requestCatchUp?(convId: string): void;
}

export interface DirectAccountCommitCoordinatorDeps {
  relay: RelayClient;
  account: string;
  deviceId: string;
  endpointId: string;
  /** EN: CD-side OpenMLS executor; when present the coordinator drives the §4.1 Commit lifecycle.
   *  CN: CD 侧 OpenMLS 执行器；存在时协调器驱动 §4.1 Commit 生命周期。 */
  executor?: WireCommitExecutor;
  /** EN: Implicit-accept settle window (ms): no `commit_reject` within it ⇒ relay accepted (§3.2).
   *  CN: 隐式采纳静默窗口（ms）：窗口内无 `commit_reject` ⇒ relay 采纳（§3.2）。 */
  settleMs?: number;
  /** EN: Poll interval (ms) while waiting for the winning commit to be applied locally (via the
   *  registry) before re-staging on EPOCH_STALE. CN: EPOCH_STALE 后、重新暂存前，等待胜出 commit 被
   *  本地（经 registry）应用的轮询间隔（ms）。 */
  catchupPollMs?: number;
  /** EN: Max catch-up polls before giving up → fall back to recover/re-handshake. CN: 放弃前的
   *  追平轮询上限 → 回退 recover/重握手。 */
  maxCatchupPolls?: number;
  /** EN: Fallback raw intent handler when no `executor` is supplied. CN: 无 `executor` 时的原始意图回调。 */
  onExecuteIntent?: (intent: CommitIntentControlMsg) => void | Promise<void>;
  /** EN: When BOTH `executor` and `onExecuteIntent` are set (unified 1:1 + group Wire), pick the relay
   *  CAS lifecycle for `d:` intents and `onExecuteIntent` for `g:` intents. Defaults to `!!executor`.
   *  CN: 当 `executor` 与 `onExecuteIntent` 同时存在（统一 1:1 + 群 Wire）时，为 `d:` 意图选 relay CAS
   *  生命周期、为 `g:` 意图走 `onExecuteIntent`。默认 `!!executor`。 */
  shouldUseRelayExecutor?: (intent: CommitIntentControlMsg) => boolean;
  onCommitReject?: (reject: CommitReject) => void;
  /** EN: CD-only: a sibling device asked to be grafted into our existing 1:1s (Wire join trigger,
   *  §3.7). CN: 仅 CD：兄弟设备请求嫁接进我们已有的 1:1（Wire 加入触发，§3.7）。 */
  onDeviceJoinRequest?: (msg: DeviceJoinRequestControlMsg) => void | Promise<void>;
  /** EN: Target-device-only: the CD listed the convs I should join (mint a KeyPackage per conv).
   *  CN: 仅目标设备：CD 列出我应加入的会话（每会话造一个 KeyPackage）。 */
  onDeviceJoinOffer?: (msg: DeviceJoinOfferControlMsg) => void | Promise<void>;
  /** EN: CD-only: the new device returned its per-conv KeyPackages → drive add_device per conv.
   *  CN: 仅 CD：新设备返回每会话 KeyPackage → 按会话驱动 add_device。 */
  onDeviceJoinKp?: (msg: DeviceJoinKpControlMsg) => void | Promise<void>;
}

interface InflightCommit {
  state: CommitLifecycleState;
  intent: CommitIntentControlMsg;
  welcomeB64: string;
  settleTimer: ReturnType<typeof setTimeout> | null;
}

const DEFAULT_SETTLE_MS = 2500;
const DEFAULT_CATCHUP_POLL_MS = 400;
const DEFAULT_MAX_CATCHUP_POLLS = 8;

export class DirectAccountCommitCoordinator {
  private election: CoordinatorElectionState;
  private pendingIntents = new Map<string, CommitIntentControlMsg>();
  /** EN: in-flight CD commits keyed by current `msgId`. CN: 按当前 `msgId` 索引的在途 CD commit。 */
  private inflight = new Map<string, InflightCommit>();
  private wired = false;
  private electionTimer: ReturnType<typeof setInterval> | null = null;
  private readonly settleMs: number;
  private readonly catchupPollMs: number;
  private readonly maxCatchupPolls: number;

  constructor(private deps: DirectAccountCommitCoordinatorDeps) {
    this.election = initCoordinatorElection(deps.deviceId);
    this.settleMs = deps.settleMs ?? DEFAULT_SETTLE_MS;
    this.catchupPollMs = deps.catchupPollMs ?? DEFAULT_CATCHUP_POLL_MS;
    this.maxCatchupPolls = deps.maxCatchupPolls ?? DEFAULT_MAX_CATCHUP_POLLS;
  }

  /// EN: Subscribe to relay control + start presence settle ticker. CN: 订阅 relay 控制面并启动 presence 静默 ticker。
  wire(): void {
    if (this.wired) return;
    this.wired = true;
    this.deps.relay.onControl((m) => this.onControl(m));
    this.deps.relay.onCommitReject?.((r) => this.onCommitReject(r));
    this.electionTimer = setInterval(() => {
      this.election = tickCoordinatorElection(this.election, Date.now());
    }, 500);
  }

  stop(): void {
    if (this.electionTimer) {
      clearInterval(this.electionTimer);
      this.electionTimer = null;
    }
    for (const f of this.inflight.values()) {
      if (f.settleTimer) clearTimeout(f.settleTimer);
    }
    this.inflight.clear();
    void this.publishPresence(false);
  }

  /// EN: Call after relay WS connect/reconnect. CN: relay WS 连接/重连后调用。
  onRelayConnected(): void {
    void this.publishPresence(true);
  }

  coordinatorDeviceId(): string | null {
    return currentCoordinator(this.election);
  }

  isCoordinator(): boolean {
    return isSelfCoordinator(this.election, this.deps.deviceId);
  }

  async publishPresence(online: boolean): Promise<void> {
    await this.deps.relay.sendControl(
      buildPresence({ account: this.deps.account, deviceId: this.deps.deviceId, online }),
    );
  }

  /// EN: Route a local Commit intent (execute if CD, else delegate). CN: 路由本地 Commit 意图（CD 执行，否则委托）。
  async submitIntent(args: {
    reqId?: string;
    kind: CommitIntentKind;
    payload: CommitIntentPayload;
  }): Promise<"execute" | "delegated"> {
    const reqId = args.reqId ?? globalThis.crypto?.randomUUID?.() ?? `ci-${Date.now()}`;
    const routed = routeCommitIntent({
      election: this.election,
      selfDeviceId: this.deps.deviceId,
      account: this.deps.account,
      endpointId: this.deps.endpointId,
      reqId,
      kind: args.kind,
      payload: args.payload,
    });
    if (routed.action === "delegate") {
      await this.deps.relay.sendControl(routed.intent);
      return "delegated";
    }
    // EN: self is CD → execute the intent locally (same path as a delegated intent), deduped by
    // reqId. CN: 自身为 CD → 本地执行该意图（与被委托意图同一路径），按 reqId 去重。
    const intent = buildCommitIntent({
      account: this.deps.account,
      from: this.deps.endpointId,
      reqId,
      kind: args.kind,
      payload: args.payload,
    });
    if (!this.pendingIntents.has(reqId)) {
      this.pendingIntents.set(reqId, intent);
      if (this.shouldRunRelayExecutor(intent)) {
        void this.executeIntent(intent);
      } else {
        void this.deps.onExecuteIntent?.(intent);
      }
    }
    return "execute";
  }

  private shouldRunRelayExecutor(intent: CommitIntentControlMsg): boolean {
    if (this.deps.shouldUseRelayExecutor) {
      return this.deps.shouldUseRelayExecutor(intent);
    }
    return !!this.deps.executor;
  }

  /// EN: Send a Gate-2-aware Wire Commit on a pairwise conv (caller supplies MLS bytes + epoch). CN: 在 pairwise conv 上发送闸二感知的 Wire Commit（调用方提供 MLS 字节 + epoch）。
  async sendWireCommit(args: {
    convId: string;
    commitB64: string;
    commitEpoch: number;
    msgId?: string;
  }): Promise<void> {
    await this.deps.relay.sendControl(
      buildWireDmCommit({
        from: this.deps.endpointId,
        convId: args.convId,
        commitB64: args.commitB64,
        commitEpoch: args.commitEpoch,
        msgId: args.msgId,
      }),
    );
  }

  /// EN: CD replies to a delegated intent over `s:<account>`. CN: CD 经 `s:<account>` 回复被委托意图。
  async replyIntentResult(
    reqId: string,
    ok: boolean,
    opts?: { reason?: "epoch_stale"; currentEpoch?: number },
  ): Promise<void> {
    await this.deps.relay.sendControl(
      buildCommitResult({
        account: this.deps.account,
        reqId,
        ok,
        reason: opts?.reason,
        currentEpoch: opts?.currentEpoch,
      }),
    );
    this.pendingIntents.delete(reqId);
  }

  private onControl(m: ControlMsg): void {
    const self = parseAccountSelfControl(m);
    if (!self) return;
    if (accountFromSelfConv(self.convId) !== this.deps.account) return;
    switch (self.t) {
      case "presence":
        this.election = applyPresenceUpdate(this.election, self.device_id, self.online, Date.now());
        break;
      case "commit_intent":
        if (!this.isCoordinator()) return;
        if (this.pendingIntents.has(self.req_id)) return;
        this.pendingIntents.set(self.req_id, self);
        if (this.shouldRunRelayExecutor(self)) {
          void this.executeIntent(self);
        } else {
          void this.deps.onExecuteIntent?.(self);
        }
        break;
      case "commit_result":
        this.pendingIntents.delete(self.req_id);
        break;
      case "device_join_request":
        // EN: only the elected CD grafts new siblings. CN: 仅当选 CD 嫁接新兄弟设备。
        if (!this.isCoordinator()) return;
        if (self.device_id === this.deps.deviceId) return;
        void this.deps.onDeviceJoinRequest?.(self);
        break;
      case "device_join_offer":
        // EN: only the addressed new device acts on the offer. CN: 仅被点名的新设备响应 offer。
        if (self.device_id !== this.deps.deviceId) return;
        void this.deps.onDeviceJoinOffer?.(self);
        break;
      case "device_join_kp":
        if (!this.isCoordinator()) return;
        if (self.device_id === this.deps.deviceId) return;
        void this.deps.onDeviceJoinKp?.(self);
        break;
    }
  }

  /// EN: New device → broadcast a graft request on `s:<account>`. CN: 新设备 → 在 `s:<account>` 广播
  /// 嫁接请求。
  async sendDeviceJoinRequest(): Promise<void> {
    await this.deps.relay.sendControl(
      buildDeviceJoinRequest({ account: this.deps.account, deviceId: this.deps.deviceId }),
    );
  }

  /// EN: CD → tell `targetDeviceId` which pairwise convs to join. CN: CD → 告知 `targetDeviceId` 应加入
  /// 哪些 pairwise 会话。
  async sendDeviceJoinOffer(targetDeviceId: string, convIds: string[]): Promise<void> {
    await this.deps.relay.sendControl(
      buildDeviceJoinOffer({ account: this.deps.account, deviceId: targetDeviceId, convIds }),
    );
  }

  /// EN: New device → hand the CD one fresh KeyPackage per conv. CN: 新设备 → 给 CD 每会话一个一次性
  /// KeyPackage。
  async sendDeviceJoinKp(kps: Array<{ conv_id: string; kp: string }>): Promise<void> {
    await this.deps.relay.sendControl(
      buildDeviceJoinKp({ account: this.deps.account, deviceId: this.deps.deviceId, kps }),
    );
  }

  /// EN: CD: run the OpenMLS op, open a lifecycle, send the Wire Commit, arm the implicit-accept
  /// settle timer. CN: CD：跑 OpenMLS 操作、开生命周期、发 Wire Commit、武装隐式采纳静默计时器。
  private async executeIntent(intent: CommitIntentControlMsg): Promise<void> {
    const executor = this.deps.executor;
    if (!executor) return;
    let out: { commitB64: string; welcomeB64: string; preEpoch: number };
    try {
      out = await executor.runIntent(intent);
    } catch (e) {
      console.warn("[nexchat] Wire commit intent execution failed:", e);
      await this.replyIntentResult(intent.req_id, false);
      return;
    }
    const msgId = newCommitMsgId();
    const state = startCommit({
      attempt: {
        convId: intent.payload.dmConvId,
        reqId: intent.req_id,
        kind: intent.kind,
        payload: intent.payload,
      },
      commitEpoch: out.preEpoch,
      msgId,
    });
    this.trackInflight({ state, intent, welcomeB64: out.welcomeB64 });
    await this.sendWireCommit({
      convId: intent.payload.dmConvId,
      commitB64: out.commitB64,
      commitEpoch: out.preEpoch,
      msgId,
    });
  }

  private trackInflight(f: { state: CommitLifecycleState; intent: CommitIntentControlMsg; welcomeB64: string }): void {
    const settleTimer = setTimeout(() => {
      this.dispatch(f.state.msgId, { t: "settle_timeout", msgId: f.state.msgId });
    }, this.settleMs);
    this.inflight.set(f.state.msgId, { ...f, settleTimer });
  }

  private onCommitReject(reject: CommitReject): void {
    this.deps.onCommitReject?.(reject);
    if (!reject.msgId) return;
    this.dispatch(reject.msgId, {
      t: "epoch_stale",
      msgId: reject.msgId,
      currentEpoch: reject.current_epoch,
    });
  }

  /// EN: Feed an event into a tracked commit's lifecycle and run the resulting actions. CN: 把事件
  /// 喂入某在途 commit 的生命周期并执行其动作。
  private dispatch(msgId: string, event: Parameters<typeof reduceCommit>[1]): void {
    const f = this.inflight.get(msgId);
    if (!f) return;
    if (f.settleTimer) {
      clearTimeout(f.settleTimer);
      f.settleTimer = null;
    }
    const step = reduceCommit(f.state, event);
    f.state = step.state;
    void this.runActions(f, step.actions);
  }

  private async runActions(f: InflightCommit, actions: CommitAction[]): Promise<void> {
    for (const a of actions) {
      switch (a.t) {
        case "deliver_welcome_and_ok":
          // EN: relay accepted (no reject within settle window) → MERGE the staged commit first,
          // then deliver the Welcome and ack. CN: relay 采纳（静默窗口内无 reject）→ 先**合并**暂存
          // commit，再投递 Welcome 并回执。
          try {
            await this.deps.executor?.commitAccepted(f.intent);
          } catch (e) {
            console.warn("[nexchat] Wire commit merge failed:", e);
          }
          try {
            await this.deps.executor?.deliverWelcome(f.intent, f.welcomeB64);
          } catch (e) {
            console.warn("[nexchat] Wire welcome delivery failed:", e);
          }
          await this.replyIntentResult(f.intent.req_id, true);
          this.inflight.delete(f.state.msgId);
          break;
        case "catch_up_and_retry":
          await this.catchUpAndRetry(f, a.currentEpoch);
          break;
        case "resend_commit":
          // EN: re-key inflight under the new msgId and re-arm the settle timer. CN: 用新 msgId 重新
          // 索引在途项并重新武装静默计时器。
          this.inflight.delete(f.state.msgId);
          this.trackInflight({ state: f.state, intent: f.intent, welcomeB64: f.welcomeB64 });
          await this.sendWireCommit({
            convId: f.intent.payload.dmConvId,
            commitB64: a.commitB64,
            commitEpoch: a.commitEpoch,
            msgId: a.msgId,
          });
          break;
        case "reply_give_up":
          // EN: discard any staged commit so the group isn't stuck pending. CN: 丢弃暂存 commit，
          // 避免群停在 pending 态。
          try {
            await this.deps.executor?.commitAbandoned(f.intent);
          } catch (e) {
            console.warn("[nexchat] Wire commit abandon failed:", e);
          }
          await this.replyIntentResult(f.intent.req_id, false, {
            reason: "epoch_stale",
            currentEpoch: a.currentEpoch,
          });
          this.inflight.delete(f.state.msgId);
          break;
      }
    }
  }

  /// EN: Discard our forked staged commit and re-stage once the local group has caught up to the
  /// winning epoch. The winner's Commit is applied to the same engine by `DirectMlsRegistry` over the
  /// normal control path; since that is async we POLL the engine (bounded) rather than failing on the
  /// first miss. Exhausting the budget → give up → fall back to recover/re-handshake (spec §5).
  /// CN: 丢弃我方分叉暂存 commit，待本地群追平到胜出 epoch 后重新暂存。胜出方 Commit 由 `DirectMlsRegistry`
  /// 经正常控制面应用到同一引擎；因其异步，故对引擎做**有界轮询**而非首次未追平即失败。预算耗尽 → 放弃 →
  /// 回退 recover/重握手（规范 §5）。
  private async catchUpAndRetry(f: InflightCommit, currentEpoch: number, attempt = 0): Promise<void> {
    const executor = this.deps.executor;
    if (!executor) return;
    const prevMsgId = f.state.msgId;
    // EN: only act if this attempt is still the tracked inflight (not superseded/stopped). CN: 仅当本
    // 在途项仍被跟踪时才动作（未被取代/停止）。
    if (this.inflight.get(prevMsgId) !== f) return;
    let rerun: { commitB64: string; welcomeB64: string; newEpoch: number };
    try {
      rerun = await executor.catchUpAndRerun(f.intent, currentEpoch);
    } catch (e) {
      if (attempt < this.maxCatchupPolls) {
        // EN: winner not applied locally yet. On the FIRST miss, actively pull the conv's stored
        // winning Commit from the relay so catch-up is deterministic (not reliant on incidental
        // fan-out); then poll again shortly. CN: 胜出 commit 尚未本地应用。**首次**未追平时主动从 relay
        // 拉取该会话已存的胜出 Commit，使追平确定（不靠偶发扇出）；随后稍后重轮询。
        if (attempt === 0) {
          try {
            executor.requestCatchUp?.(f.intent.payload.dmConvId);
          } catch {
            /* best-effort backlog pull */
          }
        }
        f.settleTimer = setTimeout(() => {
          void this.catchUpAndRetry(f, currentEpoch, attempt + 1);
        }, this.catchupPollMs);
        return;
      }
      console.warn("[nexchat] Wire commit catch-up exhausted:", e);
      try {
        await executor.commitAbandoned(f.intent);
      } catch {
        /* clear is best-effort */
      }
      await this.replyIntentResult(f.intent.req_id, false, { reason: "epoch_stale", currentEpoch });
      this.inflight.delete(prevMsgId);
      return;
    }
    f.welcomeB64 = rerun.welcomeB64;
    const step = reduceCommit(f.state, {
      t: "caught_up",
      newEpoch: rerun.newEpoch,
      commitB64: rerun.commitB64,
      welcomeB64: rerun.welcomeB64,
      newMsgId: newCommitMsgId(),
    });
    f.state = step.state;
    await this.runActions(f, step.actions);
  }
}
