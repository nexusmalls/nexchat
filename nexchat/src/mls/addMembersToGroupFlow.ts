// EN: Invite contacts into an existing on-chain MLS group (post-create commit add).
// CN: 向已有链上 MLS 群邀请联系人（建群后 commit 加人）。

import type { ChainClient } from "@/chain/chainClient";
import {
  assertMembersJoinable,
  fundMembers,
  loadCommitAuth,
  submitGroupCommit,
  syncGroupEpoch,
  validateMemberDelta,
  waitForKeyPackages,
} from "@/mls/groupMemberFlow";
import type { OpenMlsEngine } from "@/mls/openMlsEngine";
import { canonicalAddress } from "@/wallet/address";

const pendingAddsByGroup = new Map<number, string[]>();

/// EN: Merge one-at-a-time picks when a solo group must add ≥2 in one commit.
/// CN: 仅 1 人的群须一次 commit 加 ≥2 人时，合并多次单人邀请。
export function queuePendingGroupAdds(groupId: number, members: string[]): string[] | null {
  const pending = pendingAddsByGroup.get(groupId) ?? [];
  const merged = [...new Set([...pending, ...members.map(canonicalAddress)])].filter(Boolean);
  if (merged.length < 2) {
    pendingAddsByGroup.set(groupId, merged);
    return null;
  }
  pendingAddsByGroup.delete(groupId);
  return merged;
}

export function clearPendingGroupAdds(groupId: number): void {
  pendingAddsByGroup.delete(groupId);
}

export interface AddMembersToGroupDeps {
  engine: OpenMlsEngine;
  chain: ChainClient;
  selfAddress: string;
  groupId: number;
  groupName: string;
  memberAddresses: string[];
  onProgress?: (message: string) => void;
  notifyMembers?: (groupId: number, groupName: string, members: string[]) => Promise<void>;
}

/// EN: Commit-add new members to an existing group (owner/admin). CN: 向已有群 commit 加人。
export async function addMembersToGroup(
  deps: AddMembersToGroupDeps,
): Promise<"committed" | "queued"> {
  const progress = deps.onProgress ?? (() => undefined);
  const gid = deps.groupId;

  if (!deps.engine.hasGroup(`g:${gid}`)) {
    throw new Error("本地 MLS 群状态缺失，请重新打开群聊后再试");
  }

  const ctx = await loadCommitAuth(deps.chain, deps.selfAddress, gid);
  if (ctx.groupRole !== "owner" && ctx.groupRole !== "admin") {
    throw new Error("仅群主或管理员可以邀请成员");
  }

  const members = [
    ...new Set(
      deps.memberAddresses.map(canonicalAddress).filter((a) => a && a !== ctx.self),
    ),
  ];
  if (members.length === 0) throw new Error("请至少选择 1 位联系人");

  let toAdd = members;
  if (ctx.memberCount === 1) {
    const merged = queuePendingGroupAdds(gid, members);
    if (!merged) {
      if (deps.notifyMembers) {
        progress("正在发送群邀请通知…");
        await deps.notifyMembers(gid, deps.groupName, members);
      }
      progress("已发送邀请。再邀请至少 1 人后将一并完成加密入群。");
      return "queued";
    }
    toAdd = merged;
  }

  validateMemberDelta(ctx.memberCount, toAdd.length, 0);

  progress("正在同步群加密状态…");
  const epoch = await syncGroupEpoch(deps.engine, deps.chain, gid);

  progress("正在检查成员余额…");
  await fundMembers(deps.chain, ctx.self, toAdd);

  if (deps.notifyMembers) {
    progress("正在通知受邀成员…");
    await deps.notifyMembers(gid, deps.groupName, toAdd);
  }

  progress("等待成员发布 KeyPackage（最多 90 秒）…");
  const ready = await waitForKeyPackages(deps.chain, toAdd, toAdd.length);
  await assertMembersJoinable(
    deps.chain,
    ready.map((r) => r.addr),
  );

  progress("请在钱包中确认：添加成员");
  const out = deps.engine.addMembers(gid, ready.map((r) => r.kp));
  await submitGroupCommit(
    deps.chain,
    gid,
    epoch,
    out,
    ready.map((r) => r.addr),
    [],
  );
  return "committed";
}
