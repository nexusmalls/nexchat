// EN: ChatOrchestrator — the explicit 2↔3 transition state machine (design §11). DR (2-party)
// and MLS (3+ group) crypto NEVER inherit keys from each other; a switch is "archive old +
// build new on the shared identity", not a key migration (§2.3). This machine enforces the
// hard invariant that a single conversation is EITHER DR-active OR group-active, never both,
// via a per-conversation single write-lock + monotonic version. Each transition snapshots
// history to the archive first (readable history self-heals via `K_archive`, key-orthogonal),
// then performs the risky external step with rollback:
//   - 2→3: archive → dr.freeze → mls.createGroup → (success: dr.retire | failure: dr.resume)
//   - 3→2: archive → mls.dissolve(pivot) → dr.open (retryable; group already gone)
// The orchestrator only calls each engine's public port (`ports.ts`) and never touches key
// state. It is the single component allowed to depend on BOTH engines.
// CN: ChatOrchestrator —— 显式 2↔3 切换状态机（设计 §11）。DR（2 人）与 MLS（3+ 群）密码学**绝不**
// 互相继承密钥；切换是「归档旧 + 在共享身份上建新」而非密钥迁移（§2.3）。本状态机以每会话单写锁 +
// 单调版本号强制硬不变量：同一会话**要么** DR 活跃**要么**群活跃，绝不双活。每次切换先把历史快照入
// 归档（可读历史凭 `K_archive` 自愈，密钥正交），再带回滚地执行有风险的外部步骤：
//   2→3：archive → dr.freeze → mls.createGroup →（成功 dr.retire | 失败 dr.resume）
//   3→2：archive → mls.dissolve（枢轴）→ dr.open（可重试；群已不存在）
// 编排器只调用各引擎公开端口（`ports.ts`），绝不触碰密钥态；它是唯一可同时依赖两引擎的组件。

import type { ArchivePort, DrSessionPort, GroupId, MlsGroupPort } from "@/orchestrator/ports";

/// EN: Crypto mode a conversation is currently pinned to. CN: 会话当前钉定的密码模式。
export type ConvMode = "direct" | "group";

/// EN: Per-conversation orchestrator state. CN: 每会话编排状态。
export interface ConvState {
  convId: string;
  mode: ConvMode;
  /// EN: Monotonic version bumped on every committed transition. CN: 每次提交切换递增的单调版本。
  version: number;
  /// EN: Peer account for a direct (DR) conversation. CN: 私聊（DR）会话的对端账户。
  peer?: string;
  /// EN: Group id for a group (MLS) conversation. CN: 群（MLS）会话的群 id。
  groupId?: GroupId;
  /// EN: True once the target session is fully established (direct mode may be pending after
  /// a 3→2 dissolve if `dr.open` failed and needs a retry). CN: 目标会话已完全建立则为 true
  /// （3→2 解散后若 `dr.open` 失败待重试，private 模式可能 pending）。
  ready: boolean;
}

/// EN: Result of a transition attempt. CN: 切换尝试结果。
export type SwitchResult =
  | { ok: true; mode: "group"; groupId: GroupId; version: number }
  | { ok: true; mode: "direct"; version: number }
  | {
      ok: false;
      error: string;
      /// EN: true = restored to the pre-switch state (no change); false = committed to the
      /// new mode but the target session is pending a retry. CN: true = 已恢复到切换前状态
      /// （无变化）；false = 已提交到新模式但目标会话待重试。
      rolledBack: boolean;
    };

export class ChatOrchestrator {
  private readonly states = new Map<string, ConvState>();
  private readonly locks = new Set<string>();

  constructor(
    private readonly dr: DrSessionPort,
    private readonly mls: MlsGroupPort,
    private readonly archive: ArchivePort,
  ) {}

  /// EN: Current state of `convId` (or null if untracked). CN: `convId` 当前状态（未跟踪则 null）。
  getState(convId: string): ConvState | null {
    return this.states.get(convId) ?? null;
  }

  getMode(convId: string): ConvMode | "none" {
    return this.states.get(convId)?.mode ?? "none";
  }

  /// EN: Register an existing direct (DR) conversation so the orchestrator can later promote
  /// it. CN: 登记一个已存在的私聊（DR）会话，便于后续升群。
  trackDirect(convId: string, peer: string): ConvState {
    const state: ConvState = { convId, mode: "direct", version: 0, peer, ready: true };
    this.states.set(convId, state);
    return state;
  }

  /// EN: 2→3: promote a direct conversation with `peer` into an MLS group that also includes
  /// `invitees`. Rolls the DR session back to active if group creation fails — the pair is
  /// never left with both stacks active. CN: 2→3：把与 `peer` 的私聊升级为同时含 `invitees`
  /// 的 MLS 群。建群失败则把 DR 会话回滚为活跃——绝不让该对双栈同时活跃。
  async promoteToGroup(
    convId: string,
    peer: string,
    invitees: string[],
  ): Promise<SwitchResult> {
    if (this.locks.has(convId)) {
      return { ok: false, error: "orchestrator: conversation switch already in progress", rolledBack: true };
    }
    const prev = this.states.get(convId);
    if (prev && prev.mode !== "direct") {
      return { ok: false, error: "promoteToGroup: conversation is not in direct mode", rolledBack: true };
    }
    this.locks.add(convId);
    try {
      await this.archive.archive(convId);
      await this.dr.freeze(peer);

      let groupId: GroupId;
      try {
        groupId = await this.mls.createGroup([peer, ...invitees]);
      } catch (e) {
        // Group creation failed → roll the DR session back to active (no double-active).
        await this.dr.resume(peer);
        return { ok: false, error: `createGroup failed: ${errMsg(e)}`, rolledBack: true };
      }

      // Committed to group. DR retirement is best-effort cleanup; the group is authoritative.
      try {
        await this.dr.retire(peer);
      } catch {
        /* best-effort: history lives in the archive; a stale DR session is inert once group-active */
      }
      const version = (prev?.version ?? 0) + 1;
      this.states.set(convId, { convId, mode: "group", version, peer, groupId, ready: true });
      return { ok: true, mode: "group", groupId, version };
    } finally {
      this.locks.delete(convId);
    }
  }

  /// EN: 3→2: demote a group back to a direct DR conversation with `peer`. `mls.dissolve` is
  /// the irreversible pivot: if it fails the group stays active (full rollback); once it
  /// succeeds the conversation is committed to direct and `dr.open` is retried via
  /// `ensureDirect` if it throws. CN: 3→2：把群降级回与 `peer` 的私聊。`mls.dissolve` 是不可逆
  /// 枢轴：失败则群保持活跃（完整回滚）；一旦成功会话即提交为私聊，`dr.open` 抛错则经
  /// `ensureDirect` 重试。
  async demoteToDirect(convId: string, groupId: GroupId, peer: string): Promise<SwitchResult> {
    if (this.locks.has(convId)) {
      return { ok: false, error: "orchestrator: conversation switch already in progress", rolledBack: true };
    }
    const prev = this.states.get(convId);
    if (prev && prev.mode !== "group") {
      return { ok: false, error: "demoteToDirect: conversation is not in group mode", rolledBack: true };
    }
    this.locks.add(convId);
    try {
      await this.archive.archive(convId);

      try {
        await this.mls.dissolve(groupId);
      } catch (e) {
        // Dissolve failed → group remains the active stack (no double-active).
        return { ok: false, error: `dissolve failed: ${errMsg(e)}`, rolledBack: true };
      }

      // Committed to direct (group is gone). Record the target mode BEFORE opening so a failed
      // open leaves the conversation correctly in direct mode, pending a retry.
      const version = (prev?.version ?? 0) + 1;
      this.states.set(convId, { convId, mode: "direct", version, peer, ready: false });
      try {
        await this.dr.open(peer);
      } catch (e) {
        return { ok: false, error: `dr.open failed (retry via ensureDirect): ${errMsg(e)}`, rolledBack: false };
      }
      this.states.set(convId, { convId, mode: "direct", version, peer, ready: true });
      return { ok: true, mode: "direct", version };
    } finally {
      this.locks.delete(convId);
    }
  }

  /// EN: Retry establishing the DR session for a direct conversation left pending by a failed
  /// `dr.open` during 3→2. Idempotent. CN: 重试为 3→2 中 `dr.open` 失败而 pending 的私聊会话
  /// 建链。幂等。
  async ensureDirect(convId: string): Promise<SwitchResult> {
    const state = this.states.get(convId);
    if (!state || state.mode !== "direct" || !state.peer) {
      return { ok: false, error: "ensureDirect: conversation is not a direct mode pending session", rolledBack: true };
    }
    if (state.ready) return { ok: true, mode: "direct", version: state.version };
    if (this.locks.has(convId)) {
      return { ok: false, error: "orchestrator: conversation switch already in progress", rolledBack: false };
    }
    this.locks.add(convId);
    try {
      await this.dr.open(state.peer);
      state.ready = true;
      return { ok: true, mode: "direct", version: state.version };
    } catch (e) {
      return { ok: false, error: `dr.open failed: ${errMsg(e)}`, rolledBack: false };
    } finally {
      this.locks.delete(convId);
    }
  }
}

function errMsg(e: unknown): string {
  return e instanceof Error ? e.message : String(e);
}
