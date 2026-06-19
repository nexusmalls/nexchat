// EN: MlsGroupAdapter — binds the orchestrator's `MlsGroupPort` to the real OpenMLS group
// lifecycle flows (`createGroupWithMembers` / `disbandGroup`). It only calls those public
// flows and `engine.hasGroup`; it never reads MLS key/epoch state directly (§2.3). Lives in
// `orchestrator/` — the single bridge module allowed to depend on BOTH `@/mls/*` and
// `@/crypto-dr/*` — so the MLS engine itself stays import-clean (enforced by
// `decoupling.test.ts`, which only scans the two engine dirs). CN: MlsGroupAdapter —— 把编排器
// `MlsGroupPort` 绑定到真实 OpenMLS 群生命周期流程（`createGroupWithMembers` / `disbandGroup`）。
// 它只调用这些公开流程与 `engine.hasGroup`，绝不直接读 MLS 密钥/epoch 态（§2.3）。置于
// `orchestrator/`——唯一允许同时依赖 `@/mls/*` 与 `@/crypto-dr/*` 的桥接模块——使 MLS 引擎自身
// 保持 import 纯净（由 `decoupling.test.ts` 强制，它只扫两引擎目录）。

import type { ChainClient } from "@/chain/chainClient";
import { disbandGroup } from "@/mls/changeGroupMembersFlow";
import { createGroupWithMembers } from "@/mls/createGroupFlow";
import type { OpenMlsEngine } from "@/mls/openMlsEngine";
import type { GroupId, MlsGroupPort } from "@/orchestrator/ports";

export interface MlsGroupAdapterDeps {
  engine: OpenMlsEngine;
  chain: ChainClient;
  selfAddress: string;
  /// EN: Name for the group created when promoting a 1:1 (2→3). CN: 1:1 升群（2→3）时所建群的群名。
  groupName?: string;
  isPublic?: boolean;
  onProgress?: (message: string) => void;
}

export class MlsGroupAdapter implements MlsGroupPort {
  constructor(private readonly deps: MlsGroupAdapterDeps) {}

  /// EN: Create an on-chain MLS group over `members` (= peer + invitees; the flow excludes
  /// self, dedups, and requires ≥2 others) and return the group id. CN: 对 `members`（= 对端 +
  /// 被邀者；流程会排除自己、去重并要求 ≥2 人）建链上 MLS 群并返回群 id。
  async createGroup(members: string[]): Promise<GroupId> {
    return createGroupWithMembers({
      engine: this.deps.engine,
      chain: this.deps.chain,
      selfAddress: this.deps.selfAddress,
      name: this.deps.groupName ?? "群聊",
      memberAddresses: members,
      isPublic: this.deps.isPublic,
    });
  }

  /// EN: Owner-disband the group on chain (3→2 pivot). Throws if the caller is not the owner —
  /// the orchestrator treats that as a failed pivot and keeps the group active (no
  /// double-active). CN: 群主链上解散群（3→2 枢轴）。非群主调用会抛错——编排器视为枢轴失败并
  /// 保持群活跃（不双活）。
  async dissolve(groupId: GroupId): Promise<void> {
    await disbandGroup({
      engine: this.deps.engine,
      chain: this.deps.chain,
      selfAddress: this.deps.selfAddress,
      groupId,
      onProgress: this.deps.onProgress,
    });
  }

  /// EN: Whether the local engine still holds the group's MLS state. CN: 本地引擎是否仍持有该群
  /// MLS 态。
  isActive(groupId: GroupId): boolean {
    return this.deps.engine.hasGroup(`g:${groupId}`);
  }
}
