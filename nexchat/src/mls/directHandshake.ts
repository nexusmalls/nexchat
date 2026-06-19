// EN: Off-chain 1:1 MLS handshake over relay control-plane (no on-chain group).
// Owner = lexicographically smaller SS58 address; creates a local 2-member OpenMLS
// group and adds the peer via KeyPackage (chain-published or relay kp). Control
// messages use the canonical `directMlsKey` as `convId`.
// CN: 经 relay 控制面的链下 1:1 MLS 握手（无链上群）。owner = 字典序较小的 SS58 地址；
// 本地建 2 人 OpenMLS 群并用 KeyPackage 加对端（链上已发布或 relay kp）。控制消息以
// 规范 `directMlsKey` 作为 `convId`。

import type { ChainClient } from "@/chain/chainClient";
import { config } from "@/config";
import {
  directHandshakeOwner,
  directMlsKey,
} from "@/mls/directConv";
import { verifyIncomingCommit } from "@/mls/followCommitGuard";
import type { OpenMlsEngine } from "@/mls/openMlsEngine";
import {
  bytesToB64,
  b64ToBytes,
  type ControlMsg,
  type RelayClient,
} from "@/relay/relayClient";

export interface DirectMlsStatus {
  ready: boolean;
  role: "owner" | "member";
}

export interface DirectCoordinatorDeps {
  engine: OpenMlsEngine;
  relay: RelayClient;
  endpointId: string;
  selfAddress: string;
  peerAddress: string;
  chain?: Pick<ChainClient, "keyPackagesOf">;
  /** EN: Allow owner to add from on-chain KP when relay kp absent (default: no relay WS). CN: relay kp 缺失时允许 owner 用链上 KP 加人（默认：无 relay WS 时）。 */
  chainKpFallback?: boolean;
  pollMs?: number;
  kpRetryMs?: number;
  onStatus: (s: DirectMlsStatus) => void;
}

export class DirectMlsCoordinator {
  /// EN: Cap owner chain-KP add attempts per window — avoids unbounded chain RPC when the
  /// peer has no published KeyPackage. CN: 限制 owner 每个窗口的链上 KP 加人尝试次数——对端
  /// 未发布 KeyPackage 时避免无界链上 RPC。
  private static readonly MAX_CHAIN_ADD_ATTEMPTS = 8;

  readonly mlsKey: string;
  private readonly isOwner: boolean;
  private readonly pollMs: number;
  private readonly kpRetryMs: number;
  private readonly chainKpFallback: boolean;
  private chainAddAttempts = 0;
  private groupCreated = false;
  private joined = false;
  private memberAdded = false;
  private peerMlsReady = false;
  private pollTimer: ReturnType<typeof setInterval> | null = null;
  private memberRetryTimer: ReturnType<typeof setInterval> | null = null;
  private addQueue: Promise<void> = Promise.resolve();
  private lastPeerKpB64: string | null = null;
  private lastWelcomeB64: string | null = null;
  private pendingCommitB64: string | null = null;
  /// EN: Reused until Welcome (avoid new KP every retry → owner re-add loop). CN: 收到 Welcome 前复用，避免每次重试新 KP 触发 owner 反复加人。
  private pendingKpB64: string | null = null;

  constructor(private deps: DirectCoordinatorDeps) {
    this.mlsKey = directMlsKey(deps.selfAddress, deps.peerAddress);
    this.isOwner = directHandshakeOwner(deps.selfAddress, deps.peerAddress) === deps.selfAddress;
    this.pollMs = deps.pollMs ?? 1000;
    this.kpRetryMs = deps.kpRetryMs ?? 3000;
    this.chainKpFallback = deps.chainKpFallback ?? !config.relayWs;
  }

  /// EN: Owner attempts to add the peer from its on-chain KeyPackage, bounded to a few
  /// retries (re-armed on relay reconnect / live kp). CN: owner 用对端链上 KeyPackage 加人，
  /// 仅做有限次重试（relay 重连 / 收到 live kp 时重新触发）。
  private startOwnerChainPoll(): void {
    if (!this.chainKpFallback) return;
    this.stopOwnerPoll();
    this.chainAddAttempts = 0;
    void this.tryAddFromChain();
    this.pollTimer = setInterval(() => {
      this.chainAddAttempts += 1;
      if (this.memberAdded || this.chainAddAttempts >= DirectMlsCoordinator.MAX_CHAIN_ADD_ATTEMPTS) {
        this.stopOwnerPoll();
        return;
      }
      void this.tryAddFromChain();
    }, this.pollMs);
  }

  /// EN: Begin handshake (idempotent if group already restored from persistence).
  /// CN: 开始握手（若持久化已恢复群则幂等）。
  start(): void {
    if (this.deps.engine.hasGroup(this.mlsKey)) {
      if (this.isOwner) {
        // EN: Restored owner group with member — wait for member's relay kp (they re-kp each session).
        // Do not chain-add (DuplicateSignatureKey) or mark ready until fresh welcome round-trip.
        // CN: 恢复的 owner 群已有成员——等待成员 relay kp（每会话重发）；勿链上加人；未完成新 welcome 前不算 ready。
        this.joined = true;
        this.groupCreated = true;
        this.memberAdded = false;
        this.peerMlsReady = false;
        if (this.groupEpoch() < 1) {
          this.startOwnerChainPoll();
        }
        this.emit();
        return;
      }
      // EN: Member always re-KeyPackages on session start — stale IDB group caused WrongGroupId.
      // CN: 成员每次会话启动都重发 KeyPackage——IDB 里过期的群会导致 WrongGroupId。
      this.deps.engine.forgetGroupByConv(this.mlsKey);
    }
    if (this.isOwner) {
      this.ensureOwnerGroup();
      this.startOwnerChainPoll();
    } else {
      this.joined = false;
      this.lastWelcomeB64 = null;
      this.pendingKpB64 = null;
      this.sendKp();
      this.memberRetryTimer = setInterval(() => this.sendKp(), this.kpRetryMs);
    }
    this.emit();
  }

  stop(): void {
    this.stopOwnerPoll();
    this.stopMemberRetry();
  }

  /// EN: Drop local group and restart member handshake (after decrypt failure / stale welcome replay).
  /// CN: 丢弃本地群并重启成员握手（解密失败 / 旧 welcome 重放后）。
  recoverMemberSession(): void {
    if (this.isOwner) return;
    if (this.deps.engine.hasGroup(this.mlsKey)) {
      this.deps.engine.forgetGroupByConv(this.mlsKey);
    }
    this.joined = false;
    this.lastWelcomeB64 = null;
    this.pendingKpB64 = null;
    this.sendKp();
    if (!this.memberRetryTimer) {
      this.memberRetryTimer = setInterval(() => this.sendKp(), this.kpRetryMs);
    }
    this.emit();
  }

  /// EN: Owner-side recovery when decrypt fails (drop group, wait for member KeyPackage).
  /// CN: 解密失败时 owner 端恢复（丢弃群，等待成员 KeyPackage）。
  recoverOwnerSession(): void {
    if (!this.isOwner) return;
    this.resetLocalGroup();
    this.ensureOwnerGroup();
    this.startOwnerChainPoll();
    this.emit();
  }

  /// EN: Re-run handshake after relay reconnect (kp / chain poll). CN: relay 重连后重试握手。
  onRelayConnected(): void {
    if (this.isOwner) {
      if (!this.memberAdded && this.chainKpFallback && this.groupEpoch() < 1) {
        this.startOwnerChainPoll();
      }
    } else if (!this.joined) {
      this.sendKp();
    }
  }

  /// EN: True when encrypt/decrypt is allowed for this pairwise session. CN: 允许加解密时返回 true。
  isReady(): boolean {
    return (
      this.joined &&
      this.deps.engine.hasGroup(this.mlsKey) &&
      (!this.isOwner || (this.memberAdded && this.peerMlsReady))
    );
  }

  /// EN: Dispatch a relay control frame for this pairwise session. CN: 分发本私聊会话的 relay 控制帧。
  handleControl(m: ControlMsg): void {
    if (m.t === "contact_req" || m.t === "contact_ack" || m.t === "group_invite") return;
    if (!("convId" in m) || m.convId !== this.mlsKey) return;
    switch (m.t) {
      case "kp":
        if (this.isOwner) {
          this.addQueue = this.addQueue.then(async () => {
            const kpB64 = m.kp;
            if (kpB64 === this.lastPeerKpB64) return;
            this.lastPeerKpB64 = kpB64;

            // EN: Welcome round in flight — try grafting another device leaf; if that fails (member
            // re-keyed / restarted), reset and accept a fresh pairwise handshake. Never silently
            // ignore a *different* kp (that deadlocked most 1:1 sessions after unlock).
            // CN: Welcome 往返进行中——先尝试嫁接另一台设备 leaf；失败（成员重钥/重启）则重置并接受新
            // 成对握手。绝不静默忽略*不同* kp（否则解锁后大部分 1:1 会卡死）。
            if (this.memberAdded && !this.peerMlsReady) {
              try {
                await this.addAdditionalPeerLeaf(b64ToBytes(kpB64));
                return;
              } catch (e) {
                const msg = e instanceof Error ? e.message : String(e);
                if (msg.includes("DuplicateSignatureKey")) return;
                this.resetLocalGroup();
              }
            } else if (this.memberAdded && this.peerMlsReady) {
              try {
                await this.addAdditionalPeerLeaf(b64ToBytes(kpB64));
                return;
              } catch (e) {
                const msg = e instanceof Error ? e.message : String(e);
                if (msg.includes("DuplicateSignatureKey")) return;
                // EN: Peer reinstalled — fall through to a full re-handshake. CN: 对端重装——走完整重握手。
                this.resetLocalGroup();
              }
            } else if (this.deps.engine.hasGroup(this.mlsKey) && this.groupEpoch() >= 1) {
              // EN: Restored owner group with a stale member leaf — reset before re-add. CN: 恢复的 owner
              // 群含过期 member leaf——重加前先重置。
              this.resetLocalGroup();
            }

            this.ensureOwnerGroup();
            try {
              await this.addPeer(b64ToBytes(kpB64));
            } catch (e) {
              const msg = e instanceof Error ? e.message : String(e);
              console.warn("[nexchat] direct MLS addPeer from kp failed:", msg);
            }
          });
        }
        break;
      case "welcome": {
        const forMe =
          m.toAddr === this.deps.selfAddress || m.to === this.deps.endpointId;
        if (!forMe || this.isOwner) break;
        this.addQueue = this.addQueue.then(async () => {
          const welcomeB64 = m.welcome;
          if (
            this.joined &&
            this.deps.engine.hasGroup(this.mlsKey) &&
            welcomeB64 === this.lastWelcomeB64
          ) {
            this.emit();
            return;
          }
          if (this.deps.engine.hasGroup(this.mlsKey)) {
            this.deps.engine.forgetGroupByConv(this.mlsKey);
            this.joined = false;
          }
          try {
            await this.deps.engine.processWelcomeByConv(this.mlsKey, b64ToBytes(welcomeB64));
          } catch (e) {
            const msg = e instanceof Error ? e.message : String(e);
            if (msg.includes("NoMatchingKeyPackage")) {
              this.recoverMemberSession();
              return;
            }
            console.warn("[nexchat] direct MLS processWelcome failed:", msg);
            return;
          }
          this.lastWelcomeB64 = welcomeB64;
          this.pendingKpB64 = null;
          this.joined = true;
          if (this.pendingCommitB64) await this.applyCommit(this.pendingCommitB64);
          this.stopMemberRetry();
          void this.deps.relay
            .sendControl({
              t: "mls_ready",
              from: this.deps.endpointId,
              identity: this.deps.selfAddress,
              convId: this.mlsKey,
            })
            .catch((e) => console.warn("[nexchat] direct MLS mls_ready failed:", e));
          this.emit();
        });
        break;
      }
      case "commit":
        this.addQueue = this.addQueue.then(async () => {
          await this.applyCommit(m.commit);
        });
        break;
      case "mls_ready":
        if (!this.isOwner) break;
        this.peerMlsReady = true;
        this.emit();
        break;
    }
  }

  private async applyCommit(commitB64: string): Promise<void> {
    if (!this.joined || !this.deps.engine.hasGroup(this.mlsKey)) {
      this.pendingCommitB64 = commitB64;
      return;
    }
    const commitBytes = b64ToBytes(commitB64);
    // EN: member-side E2EI re-verification (§3.9) — independently confirm every leaf this Commit ADDS is
    // account-bound to a conv party before merging; reject a Commit that admits an unverifiable leaf
    // (e.g. a malicious committer injecting a foreign leaf), relay-trustlessly. CN: 成员侧 E2EI 复验
    // （§3.9）——合并前独立确认该 Commit **新增**的每个 leaf 都账户绑定到会话方；拒绝混入不可验证 leaf 的
    // Commit（如恶意提交方注入外来 leaf），relay-trustless。
    if (!(await verifyIncomingCommit(this.deps.engine, this.mlsKey, commitBytes))) {
      console.warn("[nexchat] direct MLS: rejected Commit adding an unverifiable leaf (§3.9)");
      this.pendingCommitB64 = null;
      return;
    }
    try {
      this.deps.engine.processCommitByConv(this.mlsKey, commitBytes);
    } catch {
      /* already applied */
    }
    this.pendingCommitB64 = null;
  }

  private resetLocalGroup(): void {
    if (this.deps.engine.hasGroup(this.mlsKey)) {
      this.deps.engine.forgetGroupByConv(this.mlsKey);
    }
    this.groupCreated = false;
    this.joined = false;
    this.memberAdded = false;
    this.peerMlsReady = false;
    this.pendingCommitB64 = null;
    this.stopOwnerPoll();
  }

  private stopOwnerPoll(): void {
    if (this.pollTimer) clearInterval(this.pollTimer);
    this.pollTimer = null;
  }

  private stopMemberRetry(): void {
    if (this.memberRetryTimer) clearInterval(this.memberRetryTimer);
    this.memberRetryTimer = null;
  }

  private ensureOwnerGroup(): void {
    if (this.groupCreated) return;
    this.deps.engine.createGroupByConv(this.mlsKey);
    this.groupCreated = true;
    this.joined = true;
  }

  /// EN: Member publishes KeyPackage to owner (retried until Welcome). CN: 成员向 owner 发送 KeyPackage（直到收到 Welcome）。
  private sendKp(): void {
    if (this.joined || this.isOwner) return;
    if (!this.pendingKpB64) {
      this.pendingKpB64 = bytesToB64(this.deps.engine.generateKeyPackage());
    }
    void this.deps.relay
      .sendControl({
        t: "kp",
        from: this.deps.endpointId,
        identity: this.deps.selfAddress,
        convId: this.mlsKey,
        kp: this.pendingKpB64,
      })
      .catch((e) => console.warn("[nexchat] direct MLS sendKp failed:", e));
  }

  private async tryAddFromChain(): Promise<void> {
    if (!this.chainKpFallback || !this.isOwner || this.memberAdded || !this.deps.chain) return;
    if (this.groupEpoch() >= 1) return;
    try {
      const kps = await this.deps.chain.keyPackagesOf(this.deps.peerAddress);
      if (kps.length === 0) return;
      // EN: Spread owners deterministically across the peer's published KP pool so two
      // different owners are less likely to grab the same one-shot KeyPackage (the peer can
      // only consume each KP once locally). CN: 按 owner 地址确定性地分散到对端已发布的 KP
      // 池，降低两个不同 owner 抢到同一个一次性 KeyPackage 的概率（对端每个 KP 本地只能消费一次）。
      const idx = this.chainKpIndex(kps.length);
      await this.addPeer(kps[idx]!);
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e);
      if (!msg.includes("DuplicateSignatureKey")) {
        console.warn("[nexchat] direct MLS tryAddFromChain failed:", msg);
      }
    }
  }

  private chainKpIndex(len: number): number {
    if (len <= 1) return 0;
    let h = 0;
    for (let i = 0; i < this.deps.selfAddress.length; i += 1) {
      h = (h * 31 + this.deps.selfAddress.charCodeAt(i)) >>> 0;
    }
    return h % len;
  }

  private groupEpoch(): number {
    if (!this.deps.engine.hasGroup(this.mlsKey)) return 0;
    try {
      return this.deps.engine.epochByConv(this.mlsKey);
    } catch {
      return 0;
    }
  }

  private markMemberAdded(): void {
    this.memberAdded = true;
    this.peerMlsReady = false;
    this.stopOwnerPoll();
    this.emit();
  }

  /// EN: Add another device leaf of the peer account without resetting the pairwise group (Wire
  /// multi-leaf). CN: 在不重置成对群的前提下加入对端账户的另一台设备 leaf（Wire 多 leaf）。
  private async addAdditionalPeerLeaf(kp: Uint8Array): Promise<void> {
    const out = this.deps.engine.addMembersByConv(this.mlsKey, [kp]);
    void this.deps.relay
      .sendControl({
        t: "welcome",
        from: this.deps.endpointId,
        to: "",
        toAddr: this.deps.peerAddress,
        convId: this.mlsKey,
        welcome: bytesToB64(out.welcome),
      })
      .catch((e) => console.warn("[nexchat] direct MLS extra-device welcome failed:", e));
    void this.deps.relay
      .sendControl({
        t: "commit",
        from: this.deps.endpointId,
        convId: this.mlsKey,
        commit: bytesToB64(out.commit),
      })
      .catch((e) => console.warn("[nexchat] direct MLS extra-device commit failed:", e));
  }

  private async addPeer(kp: Uint8Array): Promise<void> {
    if (this.memberAdded) return;
    this.ensureOwnerGroup();
    try {
      const out = this.deps.engine.addMembersByConv(this.mlsKey, [kp]);
      this.markMemberAdded();
      void this.deps.relay
        .sendControl({
          t: "welcome",
          from: this.deps.endpointId,
          to: "",
          toAddr: this.deps.peerAddress,
          convId: this.mlsKey,
          welcome: bytesToB64(out.welcome),
        })
        .catch((e) => console.warn("[nexchat] direct MLS welcome send failed:", e));
      void this.deps.relay
        .sendControl({
          t: "commit",
          from: this.deps.endpointId,
          convId: this.mlsKey,
          commit: bytesToB64(out.commit),
        })
        .catch((e) => console.warn("[nexchat] direct MLS commit send failed:", e));
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e);
      if (msg.includes("DuplicateSignatureKey")) {
        this.memberAdded = true;
        this.peerMlsReady = true;
        this.stopOwnerPoll();
        this.emit();
        return;
      }
      throw e;
    }
  }

  private emit(): void {
    const ready =
      this.joined &&
      this.deps.engine.hasGroup(this.mlsKey) &&
      (!this.isOwner || (this.memberAdded && this.peerMlsReady));
    if (ready) this.stopMemberRetry();
    this.deps.onStatus({
      role: this.isOwner ? "owner" : "member",
      ready,
    });
  }
}
