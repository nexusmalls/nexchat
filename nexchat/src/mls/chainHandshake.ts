// EN: ChainMlsCoordinator — the handshake control-plane that uses the REAL on-chain DS/AS
// (the chat-group pallet) instead of a relay simulation. The chain provides global ordering
// for identity (publish_key_package), membership (commit / epoch), and a Welcome mailbox
// (pending_welcome / claim_welcome). All cryptography stays in OpenMLS; the chain only moves
// opaque blobs.
//
// Roster-driven, owner-led orchestration (poll loop):
//   • owner = roster[0]. It funds the other members (dev chain only endows the creator),
//     creates the group once (keying OpenMLS by the chain-minted group id), then polls each
//     member's published KeyPackage and commits Adds. The first commit must add ≥2 members
//     (the pallet forbids exactly-2-member groups).
//   • members publish one KeyPackage, poll their conversation list to discover the group the
//     owner added them to, claim+process their Welcome, then catch up any later epochs from
//     the handshake log.
// Survives refresh via OpenMLS persistence: a restored engine already holds the group, so the
// coordinator just resumes polling.
//
// CN: ChainMlsCoordinator —— 用**真实链上 DS/AS**（chat-group pallet）取代 relay 模拟的握手控制面。
// 链负责身份（publish_key_package）、成员/epoch（commit）、Welcome 信箱（pending_welcome/claim_welcome）
// 的全局排序；密码学全在 OpenMLS，链只搬运不透明字节。
// 名册驱动、owner 主导的轮询编排：owner=roster[0]，先给其他成员转账（dev 链只给创建者发币），
// 建群一次（用链铸造的 group id 作 OpenMLS 键），轮询各成员已发布的 KeyPackage 并 commit 加人
// （首个 commit 须加 ≥2 人，链禁止恰好 2 人群）；成员发布一个 KeyPackage，轮询会话列表发现被加入的群，
// 领取并处理 Welcome，再从握手日志补齐后续 epoch。配合 OpenMLS 持久化跨刷新：恢复的引擎已持有群，
// 协调器仅恢复轮询。

import type { ChainClient } from "@/chain/chainClient";
import { hex, hexToBytes, hexUtf8 } from "@/mls/chainBytes";
import type { OpenMlsEngine } from "@/mls/openMlsEngine";
import type { MlsStatus } from "@/mls/handshake";

export interface ChainCoordinatorDeps {
  engine: OpenMlsEngine;
  chain: ChainClient;
  selfAddress: string;
  /// EN: SS58 addresses of every demo participant; roster[0] is the owner. CN: 全体演示成员地址，roster[0] 为 owner。
  roster: string[];
  isPublic?: boolean;
  pollMs?: number;
  /// EN: amount (planck) the owner sends each member to cover deposits + fees. CN: owner 给每个成员转账（planck）覆盖押金与手续费。
  fundPlanck?: bigint;
  /// EN: display name the owner sets on the freshly-minted group (so members can find it).
  /// CN: owner 给新建群设置的显示名（便于成员在列表中识别）。
  groupName?: string;
  onStatus: (s: MlsStatus) => void;
  onGroupId?: (groupId: number) => void;
  onError?: (e: string) => void;
}

const DEFAULT_POLL_MS = 3000;
const DEFAULT_FUND = 100_000_000_000_000n; // 100 NEX (12 decimals) — covers deposits + fees.
const FUND_THRESHOLD = 10_000_000_000_000n; // 10 NEX — below this a member is considered unfunded.

export class ChainMlsCoordinator {
  private readonly isOwner: boolean;
  private readonly isPublic: boolean;
  private readonly pollMs: number;
  private readonly fundPlanck: bigint;

  private groupId: number | null = null;
  private created = false;
  private funded = false;
  private kpPublished = false;
  private addedMembers = new Set<string>();
  private members = 1;

  private timer: ReturnType<typeof setTimeout> | null = null;
  private running = false;
  private ticking = false;

  constructor(private deps: ChainCoordinatorDeps) {
    this.isOwner = deps.roster[0] === deps.selfAddress;
    this.isPublic = deps.isPublic ?? true;
    this.pollMs = deps.pollMs ?? DEFAULT_POLL_MS;
    this.fundPlanck = deps.fundPlanck ?? DEFAULT_FUND;
  }

  /// EN: Begin the poll loop. CN: 启动轮询循环。
  start(): void {
    if (this.running) return;
    this.running = true;
    this.emit();
    void this.loop();
  }

  /// EN: Stop polling. CN: 停止轮询。
  stop(): void {
    this.running = false;
    if (this.timer) clearTimeout(this.timer);
    this.timer = null;
  }

  private async loop(): Promise<void> {
    if (!this.running) return;
    try {
      if (!this.ticking) {
        this.ticking = true;
        // EN: A read-only (Track A escrow-restored) device holds NO signing key — owner/member ticks
        // mint KeyPackages / create / commit, all of which throw `no_signer`. It already has its group
        // state from the vault, so it only needs to FOLLOW the chain (catch up epochs). Sending
        // authority is regained via the §5 handoff / PIN restore, not here. CN: 只读（路线 A 托管恢复）
        // 设备**无签名钥**——owner/member tick 会生成 KeyPackage / 建群 / commit，均抛 `no_signer`。它已
        // 从 vault 持有群状态，故只需**跟随**链（追平 epoch）；发送权经 §5 交接 / PIN 恢复重获，不在此处。
        if (!this.deps.engine.canExportEscrow()) await this.readOnlyTick();
        else if (this.isOwner) await this.ownerTick();
        else await this.memberTick();
        this.ticking = false;
      }
    } catch (e) {
      this.ticking = false;
      this.deps.onError?.(String(e));
    }
    this.emit();
    if (this.running) this.timer = setTimeout(() => void this.loop(), this.pollMs);
  }

  // ---------------------------------------------------------------- owner ----

  private async ownerTick(): Promise<void> {
    if (!this.created) {
      await this.fundMembers();
      await this.ensureGroup();
      return; // give members a tick to publish KeyPackages before the first Add
    }
    await this.commitPendingMembers();
  }

  // EN: Endow each member so it can pay the KeyPackage/group deposits (dev genesis only
  // funds the creator). CN: 给每个成员转账以支付 KeyPackage/建群押金（dev 创世只给创建者发币）。
  private async fundMembers(): Promise<void> {
    if (this.funded) return;
    for (const m of this.deps.roster) {
      if (m === this.deps.selfAddress) continue;
      const bal = await this.deps.chain.freeBalance(m);
      if (bal < FUND_THRESHOLD) {
        await this.deps.chain.signAndSendDev("balances", "transferKeepAlive", [
          m,
          this.fundPlanck,
        ]);
      }
    }
    this.funded = true;
  }

  private async ensureGroup(): Promise<void> {
    // Reuse a group we already own (e.g. restored after a refresh) before minting a new one.
    const rows = await this.deps.chain.listConversations(this.deps.selfAddress);
    const owned = rows
      .filter((r) => r.kind === "group" && r.groupRole === 0 && r.groupId != null)
      .map((r) => r.groupId as number)
      .sort((a, b) => b - a);
    for (const gid of owned) {
      if (this.deps.engine.hasGroup(`g:${gid}`)) {
        this.bindGroup(gid);
        this.created = true;
        return;
      }
    }

    // EN: Recover epoch-0 solo owner groups after a lost IndexedDB snapshot.
    // CN: IndexedDB 快照丢失后，恢复 epoch 0 的单人群主本地 MLS。
    for (const gid of owned) {
      const snap = await this.deps.chain.groupSnapshot(gid);
      if (snap && snap.epoch === 0 && snap.memberCount === 1) {
        this.deps.engine.createGroup(gid);
        this.bindGroup(gid);
        this.created = true;
        return;
      }
    }

    // EN: Already own on-chain groups but cannot rebuild MLS — do not mint another id.
    // CN: 链上已有群但无法重建 MLS 时，不再误建新群。
    if (owned.length > 0) {
      this.bindGroup(owned[0]!);
      this.created = true;
      return;
    }

    // Fresh group: predict the id the chain will mint, key OpenMLS by it, then create.
    const predicted = await this.deps.chain.nextGroupId();
    const fp = this.deps.engine.createGroup(predicted);
    const gid = await this.deps.chain.createGroupDev([
      hex(new Uint8Array([1])),
      this.deps.engine.cipherSuite(),
      this.isPublic,
      hex(fp.tree_hash),
      hex(fp.transcript_hash),
    ]);
    if (gid !== predicted) {
      throw new Error(`group id race: predicted ${predicted}, minted ${gid}`);
    }
    // Name the group so members can identify it in their conversation list.
    const name = this.deps.groupName ?? "NexChat 链上群 / On-chain Group";
    await this.deps.chain.signAndSendDev("chatGroup", "setGroupProfile", [
      gid,
      hexUtf8(name),
      null,
      null,
    ]);
    this.bindGroup(gid);
    this.created = true;
  }

  private async commitPendingMembers(): Promise<void> {
    if (this.groupId == null) return;
    const gid = this.groupId;

    // Collect a KeyPackage for every roster member not yet added.
    const pending: { addr: string; kp: Uint8Array }[] = [];
    for (const m of this.deps.roster) {
      if (m === this.deps.selfAddress || this.addedMembers.has(m)) continue;
      const kps = await this.deps.chain.keyPackagesOf(m);
      if (kps.length > 0) pending.push({ addr: m, kp: kps[0] });
    }
    if (pending.length === 0) return;

    const epoch = this.deps.engine.epochOf(gid);
    // First commit (epoch 0) must add ≥2 — the pallet forbids exactly-2-member groups.
    if (epoch === 0 && this.addedMembers.size === 0 && pending.length < 2) return;

    const expectedEpoch = epoch;
    const kps = pending.map((p) => p.kp);
    const out = this.deps.engine.addMembers(gid, kps);
    const welcomes = pending.map((p) => [p.addr, hex(out.welcome)] as [string, string]);
    await this.deps.chain.signAndSendDev("chatGroup", "commit", [
      gid,
      expectedEpoch,
      hex(out.commit),
      hex(out.tree_hash),
      hex(out.transcript_hash),
      hex(new Uint8Array([2])),
      welcomes,
      { added: pending.map((p) => p.addr), removed: [] },
    ]);
    for (const p of pending) this.addedMembers.add(p.addr);
    this.members = this.addedMembers.size + 1;
  }

  // --------------------------------------------------------------- member ----

  private async memberTick(): Promise<void> {
    // 1) wait to be funded by the owner.
    if (!this.funded) {
      const bal = await this.deps.chain.freeBalance(this.deps.selfAddress);
      if (bal < FUND_THRESHOLD) return;
      this.funded = true;
    }

    // 2) publish exactly one fresh KeyPackage so the owner can Add us. Revoke any stale
    // prekeys first (from earlier sessions / a dead engine) so the owner can only pick the
    // prekey whose private half this engine actually holds.
    if (!this.kpPublished) {
      const stale = await this.deps.chain.keyPackageIdsOf(this.deps.selfAddress);
      for (const id of stale) {
        await this.deps.chain.signAndSendDev("chatGroup", "revokeKeyPackage", [id]);
      }
      const kp = this.deps.engine.generateKeyPackage();
      await this.deps.chain.signAndSendDev("chatGroup", "publishKeyPackage", [hex(kp)]);
      this.kpPublished = true;
      return;
    }

    // 3–5) Try every on-chain group (newest first) until one is joined locally.
    const rows = await this.deps.chain.listConversations(this.deps.selfAddress);
    const gids = rows
      .filter((r) => r.kind === "group" && r.groupId != null)
      .map((r) => r.groupId as number)
      .sort((a, b) => b - a);
    if (gids.length === 0) return;

    for (const gid of gids) {
      if (this.deps.engine.hasGroup(`g:${gid}`)) {
        this.bindGroup(gid);
        await this.catchUp(gid);
        const snap = await this.deps.chain.groupSnapshot(gid);
        if (snap) this.members = snap.memberCount;
        return;
      }

      const wHex = await this.deps.chain.pendingWelcome(gid, this.deps.selfAddress);
      if (!wHex) continue;

      this.bindGroup(gid);
      await this.deps.engine.processWelcome(gid, hexToBytes(wHex));
      await this.deps.chain.signAndSendDev("chatGroup", "claimWelcome", [gid]);
      await this.catchUp(gid);
      const snap = await this.deps.chain.groupSnapshot(gid);
      if (snap) this.members = snap.memberCount;
      return;
    }
  }

  // EN: Read-only (escrow-restored) device tick — no signing, follow-only. Bind every on-chain group
  // this device already holds locally (from the vault) and catch up its epoch. Never mints a
  // KeyPackage, creates a group, or commits (those need a signing key). CN: 只读（托管恢复）设备 tick——
  // 不签名、仅跟随。绑定本设备已本地持有（来自 vault）的每个链上群并追平 epoch；绝不生成 KeyPackage、建群
  // 或 commit（这些需签名钥）。
  private async readOnlyTick(): Promise<void> {
    const rows = await this.deps.chain.listConversations(this.deps.selfAddress);
    const gids = rows
      .filter((r) => r.kind === "group" && r.groupId != null)
      .map((r) => r.groupId as number)
      .sort((a, b) => b - a);
    for (const gid of gids) {
      if (!this.deps.engine.hasGroup(`g:${gid}`)) continue;
      this.bindGroup(gid);
      await this.catchUp(gid);
      const snap = await this.deps.chain.groupSnapshot(gid);
      if (snap) this.members = snap.memberCount;
    }
  }

  // EN: Replay missed Commits so a member that joined earlier stays at the chain's epoch.
  // CN: 回放遗漏的 Commit，使早先入群的成员追到链上 epoch。
  private async catchUp(gid: number): Promise<void> {
    const snap = await this.deps.chain.groupSnapshot(gid);
    if (!snap) return;
    let local = this.deps.engine.epochOf(gid);
    while (local < snap.epoch) {
      const next = local + 1;
      const cHex = await this.deps.chain.handshakeAtEpoch(gid, next);
      if (!cHex) break;
      this.deps.engine.processCommit(gid, hexToBytes(cHex));
      local = this.deps.engine.epochOf(gid);
    }
  }

  // ----------------------------------------------------------------- util ----

  private bindGroup(gid: number): void {
    if (this.groupId === gid) return;
    this.groupId = gid;
    this.deps.onGroupId?.(gid);
  }

  private emit(): void {
    const ready = this.groupId != null && this.deps.engine.hasGroup(`g:${this.groupId}`);
    this.deps.onStatus({
      role: this.isOwner ? "owner" : ready ? "member" : "unknown",
      ready,
      members: this.members,
    });
  }
}
