// EN: MlsCoordinator — the off-chain handshake control-plane that brings multiple tabs
// into ONE shared OpenMLS group over the relay, so real MLS application messages can flow.
// It mirrors the chain's DS/AS role minimally for the multi-tab demo:
//   1. peers announce (hello); owner = lexicographically-smallest endpoint id, and once a
//      group exists its owner is sticky (no double-owner on a later-joining smaller id).
//   2. each joiner publishes a KeyPackage (kp); the owner lazily creates the group on the
//      first kp, then add_members → directed Welcome + broadcast Commit.
//   3. joiners process the Welcome; existing members process each Commit to advance epoch.
// All bytes are real OpenMLS output (base64); the relay never sees plaintext.
// CN: MlsCoordinator —— 链下握手控制面：经 relay 把多个标签页拉入同一个 OpenMLS 群，从而跑真实
// MLS 应用消息。最小化复刻链上 DS/AS 角色：①peer 宣告(hello)，owner=endpoint id 字典序最小者，
// 且群一旦建立 owner 即“粘滞”（避免后加入的更小 id 造成双 owner）；②加入者发布 KeyPackage(kp)，
// owner 收到首个 kp 时惰性建群，再 add_members → 定向 Welcome + 广播 Commit；③加入者处理 Welcome，
// 旧成员处理每个 Commit 推进 epoch。所有字节均为真实 OpenMLS 产物(base64)，relay 不见明文。

import type { OpenMlsEngine } from "@/mls/openMlsEngine";
import {
  bytesToB64,
  b64ToBytes,
  type ControlMsg,
  type RelayClient,
} from "@/relay/relayClient";

export interface MlsStatus {
  role: "owner" | "member" | "unknown";
  ready: boolean;
  members: number;
}

interface CoordinatorDeps {
  engine: OpenMlsEngine;
  relay: RelayClient;
  endpointId: string;
  identity: string;
  groupId: number;
  onStatus: (s: MlsStatus) => void;
}

const SETTLE_MS = 400;

export class MlsCoordinator {
  private readonly convId: string;
  private peers = new Map<string, { identity: string; owner: boolean }>();
  private role: MlsStatus["role"] = "unknown";
  private iAmOwner = false;
  private groupCreated = false;
  private joined = false;
  private kpSent = false;
  private addedMembers = new Set<string>();
  private pendingKps = new Map<string, Uint8Array>();
  private settleTimer: ReturnType<typeof setTimeout> | null = null;
  private addQueue: Promise<void> = Promise.resolve();

  constructor(private deps: CoordinatorDeps) {
    this.convId = `g:${deps.groupId}`;
    this.peers.set(deps.endpointId, { identity: deps.identity, owner: false });
  }

  /// EN: Begin announcing and listening. CN: 开始宣告并监听。
  start(): void {
    this.deps.relay.onControl((m) => this.handle(m));
    this.hello();
    this.scheduleSettle();
  }

  private hello(): void {
    void this.deps.relay.sendControl({
      t: "hello",
      from: this.deps.endpointId,
      identity: this.deps.identity,
      convId: this.convId,
      owner: this.iAmOwner,
    });
  }

  private scheduleSettle(): void {
    if (this.settleTimer) clearTimeout(this.settleTimer);
    this.settleTimer = setTimeout(() => this.finalizeRole(), SETTLE_MS);
  }

  private handle(m: ControlMsg): void {
    if (m.t === "contact_req" || m.t === "contact_ack" || m.t === "group_invite") return;
    if (m.convId !== this.convId) return;
    switch (m.t) {
      case "hello": {
        const known = this.peers.has(m.from);
        this.peers.set(m.from, { identity: m.identity, owner: m.owner });
        // EN: reply once so the newcomer learns about us (and our owner flag).
        // CN: 首次见到则回一次，让新来者知道我们（含 owner 标志）。
        if (!known) this.hello();
        this.scheduleSettle();
        break;
      }
      case "kp": {
        this.peers.set(m.from, { identity: m.identity, owner: false });
        this.pendingKps.set(m.from, b64ToBytes(m.kp));
        this.scheduleSettle();
        if (this.iAmOwner) this.flushPendingKps();
        break;
      }
      case "welcome": {
        if (m.to === this.deps.endpointId && !this.joined) {
          void this.deps.engine.processWelcome(this.deps.groupId, b64ToBytes(m.welcome));
          this.joined = true;
          this.emit();
        }
        break;
      }
      case "commit": {
        // EN: existing members advance epoch; non-joined ignore (Welcome carries them).
        // CN: 旧成员推进 epoch；未入群者忽略（靠 Welcome 进入）。
        if (this.joined && this.deps.engine.hasGroup(this.convId)) {
          try {
            this.deps.engine.processCommit(this.deps.groupId, b64ToBytes(m.commit));
          } catch {
            /* out-of-order / already applied — ignore for the demo */
          }
        }
        break;
      }
    }
  }

  private finalizeRole(): void {
    if (this.iAmOwner) {
      this.role = "owner";
      this.emit();
      return;
    }
    // EN: a sticky owner already exists → we are a member. CN: 已有粘滞 owner → 我是成员。
    const ownerExists = [...this.peers.entries()].some(
      ([id, p]) => id !== this.deps.endpointId && p.owner,
    );
    const minId = [...this.peers.keys()].sort()[0];
    const amOwner = !ownerExists && minId === this.deps.endpointId;

    if (amOwner) {
      this.role = "owner";
      this.iAmOwner = true;
      this.hello(); // re-announce with owner=true so newcomers defer
      this.flushPendingKps();
    } else {
      this.role = "member";
      this.sendKpIfNeeded();
    }
    this.emit();
  }

  private sendKpIfNeeded(): void {
    if (this.kpSent || this.joined) return;
    const kp = this.deps.engine.generateKeyPackage();
    this.kpSent = true;
    void this.deps.relay.sendControl({
      t: "kp",
      from: this.deps.endpointId,
      identity: this.deps.identity,
      convId: this.convId,
      kp: bytesToB64(kp),
    });
  }

  private ensureGroup(): void {
    if (this.groupCreated) return;
    this.deps.engine.createGroup(this.deps.groupId);
    this.groupCreated = true;
    this.joined = true;
    this.addedMembers.add(this.deps.endpointId);
  }

  // EN: serialise add operations so commit/welcome ordering and epochs stay consistent.
  // CN: 串行化加人，保证 commit/welcome 顺序与 epoch 一致。
  private flushPendingKps(): void {
    for (const [from, kp] of [...this.pendingKps.entries()]) {
      if (this.addedMembers.has(from)) {
        this.pendingKps.delete(from);
        continue;
      }
      this.pendingKps.delete(from);
      this.addQueue = this.addQueue.then(() => this.addMember(from, kp));
    }
  }

  private async addMember(from: string, kp: Uint8Array): Promise<void> {
    if (this.addedMembers.has(from)) return;
    this.ensureGroup();
    const out = this.deps.engine.addMembers(this.deps.groupId, [kp]);
    this.addedMembers.add(from);
    // broadcast the Commit so already-joined members advance their epoch
    void this.deps.relay.sendControl({
      t: "commit",
      from: this.deps.endpointId,
      convId: this.convId,
      commit: bytesToB64(out.commit),
    });
    // directed Welcome for the new member
    void this.deps.relay.sendControl({
      t: "welcome",
      from: this.deps.endpointId,
      to: from,
      convId: this.convId,
      welcome: bytesToB64(out.welcome),
    });
    this.emit();
  }

  private emit(): void {
    this.deps.onStatus({
      role: this.role,
      ready: this.joined && this.deps.engine.hasGroup(this.convId),
      members: this.addedMembers.size || (this.joined ? 1 : 0),
    });
  }
}
