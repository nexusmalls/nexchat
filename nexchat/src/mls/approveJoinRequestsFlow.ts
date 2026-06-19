// EN: Approve private-group join requests and commit-add members on-chain.
// CN: 批准私群入群申请并通过 commit 加人。

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

export interface ApproveJoinRequestsDeps {
  engine: OpenMlsEngine;
  chain: ChainClient;
  selfAddress: string;
  groupId: number;
  applicantAddresses: string[];
  onProgress?: (message: string) => void;
}

/// EN: Minimum applicants required for a solo-owner group (TwoMemberGroupForbidden).
/// CN: 仅群主群一次 commit 至少加几人（防 2 人群）。
export function minApplicantsForSoloGroup(memberCount: number, addCount: number): number {
  if (memberCount !== 1) return 1;
  return addCount >= 2 ? addCount : 2;
}

export function validateApproveJoinBatch(memberCount: number, addCount: number): void {
  if (addCount < 1) throw new Error("请至少选择 1 位申请人");
  if (memberCount === 1 && addCount < 2) {
    throw new Error("当前群仅有群主 1 人，须一次批准并加入至少 2 位成员（链上禁止 2 人群）");
  }
  validateMemberDelta(memberCount, addCount, 0);
}

/// EN: Approve (if needed) then MLS commit-add applicants. CN: 必要时 approve 后 commit 加人。
export async function approveAndCommitJoinRequests(
  deps: ApproveJoinRequestsDeps,
): Promise<void> {
  const progress = deps.onProgress ?? (() => undefined);
  const gid = deps.groupId;

  if (!deps.engine.hasGroup(`g:${gid}`)) {
    throw new Error("本地 MLS 群状态缺失，请重新打开群聊后再试");
  }

  const ctx = await loadCommitAuth(deps.chain, deps.selfAddress, gid);
  if (ctx.groupRole !== "owner" && ctx.groupRole !== "admin") {
    throw new Error("仅群主或管理员可以批准入群申请");
  }

  const toAdd = [
    ...new Set(
      deps.applicantAddresses.map(canonicalAddress).filter((a) => a && a !== ctx.self),
    ),
  ];
  validateApproveJoinBatch(ctx.memberCount, toAdd.length);

  for (const addr of toAdd) {
    const flags = await deps.chain.groupJoinFlags(gid, addr);
    if (flags.isMember) continue;
    if (flags.isBanned) throw new Error(`申请人 ${addr.slice(0, 12)}… 已被封禁`);
    if (!flags.hasJoinRequest && !flags.hasJoinApproval) {
      throw new Error(`申请人 ${addr.slice(0, 12)}… 没有待批入群申请`);
    }
    if (!flags.hasJoinApproval) {
      progress(`正在批准 ${addr.slice(0, 12)}… 的入群申请…`);
      await deps.chain.approveJoin(gid, addr);
    }
  }

  progress("正在同步群加密状态…");
  const epoch = await syncGroupEpoch(deps.engine, deps.chain, gid);

  progress("正在检查成员余额…");
  await fundMembers(deps.chain, ctx.self, toAdd);

  progress("等待申请人发布 KeyPackage（最多 90 秒）…");
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
}
