// EN: On-chain group member removal / swap / self-leave via commit + MemberDelta.
// CN: 通过 commit + MemberDelta 完成链上踢人 / 替换 / 自助退群。

import type { ChainClient } from "@/chain/chainClient";
import type { OpenMlsEngine } from "@/mls/openMlsEngine";
import { canonicalAddress } from "@/wallet/address";
import {
  assertCanDisband,
  assertCanLeave,
  assertCanRemoveOthers,
  assertMembersJoinable,
  fundMembers,
  loadCommitAuth,
  removeRequiresSwap,
  submitGroupCommit,
  syncGroupEpoch,
  validateMemberDelta,
  waitForKeyPackages,
} from "@/mls/groupMemberFlow";

export interface ChangeGroupMembersDeps {
  engine: OpenMlsEngine;
  chain: ChainClient;
  selfAddress: string;
  groupId: number;
  onProgress?: (message: string) => void;
}

export interface RemoveGroupMembersDeps extends ChangeGroupMembersDeps {
  removeAddresses: string[];
}

export interface SwapGroupMembersDeps extends ChangeGroupMembersDeps {
  removeAddresses: string[];
  addAddresses: string[];
}

async function ensureLocalGroup(deps: ChangeGroupMembersDeps): Promise<void> {
  if (!deps.engine.hasGroup(`g:${deps.groupId}`)) {
    throw new Error("本地 MLS 群状态缺失，请重新打开群聊后再试");
  }
}

async function assertNotOwnerTargets(
  chain: ChainClient,
  groupId: number,
  removeAddresses: string[],
): Promise<void> {
  const members = await chain.listGroupMembers(groupId);
  for (const addr of removeAddresses) {
    const who = canonicalAddress(addr);
    const row = members.find((m) => canonicalAddress(m.address) === who);
    if (row?.role === "owner") {
      throw new Error("不能移除群主");
    }
  }
}

/// EN: Remove members (4+ → 3+ only; 3-member groups must use swap). CN: 踢人（4+ 人可用；3 人须 swap）。
export async function removeGroupMembers(deps: RemoveGroupMembersDeps): Promise<void> {
  const progress = deps.onProgress ?? (() => undefined);
  const gid = deps.groupId;
  const removed = [
    ...new Set(deps.removeAddresses.map(canonicalAddress).filter(Boolean)),
  ];
  if (removed.length === 0) throw new Error("请选择要移除的成员");

  await ensureLocalGroup(deps);
  const ctx = await loadCommitAuth(deps.chain, deps.selfAddress, gid);
  assertCanRemoveOthers(ctx);
  if (removed.includes(ctx.self)) {
    throw new Error("移除他人请使用踢人；退群请使用「退出群聊」");
  }
  await assertNotOwnerTargets(deps.chain, gid, removed);

  if (removeRequiresSwap(ctx.memberCount, removed.length)) {
    throw new Error(
      `当前 ${ctx.memberCount} 人，不能只移除 ${removed.length} 人。请使用「移除并替换」同时添加新成员。`,
    );
  }
  validateMemberDelta(ctx.memberCount, 0, removed.length);

  progress("正在同步群加密状态…");
  const epoch = await syncGroupEpoch(deps.engine, deps.chain, gid);

  progress("请在钱包中确认：移除成员");
  const out = deps.engine.removeMembers(gid, removed);
  await submitGroupCommit(deps.chain, gid, epoch, out, [], removed);
}

/// EN: Swap members (remove + add in one commit). CN: 同一 commit 内移除并添加成员。
export async function swapGroupMembers(deps: SwapGroupMembersDeps): Promise<void> {
  const progress = deps.onProgress ?? (() => undefined);
  const gid = deps.groupId;
  const removed = [
    ...new Set(deps.removeAddresses.map(canonicalAddress).filter(Boolean)),
  ];
  const toAdd = [...new Set(deps.addAddresses.map(canonicalAddress).filter(Boolean))];
  if (removed.length === 0) throw new Error("请选择要移除的成员");
  if (toAdd.length === 0) throw new Error("请至少选择 1 位替换成员");
  if (removed.length !== toAdd.length) {
    throw new Error("移除与添加人数须相同（当前一次仅支持 1 换 1）");
  }
  if (removed.length !== 1) {
    throw new Error("当前一次仅支持移除 1 人并添加 1 人");
  }

  await ensureLocalGroup(deps);
  const ctx = await loadCommitAuth(deps.chain, deps.selfAddress, gid);
  assertCanRemoveOthers(ctx);
  if (removed.includes(ctx.self)) {
    throw new Error("不能将自己作为踢人目标");
  }
  await assertNotOwnerTargets(deps.chain, gid, removed);
  validateMemberDelta(ctx.memberCount, toAdd.length, removed.length);

  progress("正在同步群加密状态…");
  const epoch = await syncGroupEpoch(deps.engine, deps.chain, gid);

  progress("正在检查新成员 KeyPackage…");
  await fundMembers(deps.chain, ctx.self, toAdd);
  const ready = await waitForKeyPackages(deps.chain, toAdd, 1);
  await assertMembersJoinable(
    deps.chain,
    ready.map((r) => r.addr),
  );

  progress("请在钱包中确认：移除并替换成员");
  const out = deps.engine.swapMembers(gid, removed, ready.map((r) => r.kp));
  await submitGroupCommit(
    deps.chain,
    gid,
    epoch,
    out,
    ready.map((r) => r.addr),
    removed,
  );
}

/// EN: Owner disbands the group on-chain (`disband_group`). No MLS commit is required — the pallet
/// tears down storage directly (may need repeated calls for large groups). Local MLS state is
/// dropped best-effort when present. CN: 群主链上解散群（`disband_group`）。无需 MLS commit——pallet 直接
/// 拆除存储（大群可能需多次调用）。若存在则尽力丢弃本地 MLS 状态。
export async function disbandGroup(deps: ChangeGroupMembersDeps): Promise<void> {
  const progress = deps.onProgress ?? (() => undefined);
  const gid = deps.groupId;
  const ctx = await loadCommitAuth(deps.chain, deps.selfAddress, gid);
  assertCanDisband(ctx);

  progress("正在签名并提交链上交易…");
  await deps.chain.disbandGroup(gid, (message) => progress(message));

  const convKey = `g:${gid}`;
  if (deps.engine.hasGroup(convKey)) {
    deps.engine.forgetGroup(gid);
    await deps.engine.flush();
  }
}

/// EN: Self-leave via commit with removed=[self]. CN: 自助退群（removed 仅含自己）。
export async function leaveGroup(deps: ChangeGroupMembersDeps): Promise<void> {
  const progress = deps.onProgress ?? (() => undefined);
  const gid = deps.groupId;

  await ensureLocalGroup(deps);
  const ctx = await loadCommitAuth(deps.chain, deps.selfAddress, gid);
  assertCanLeave(ctx);

  if (removeRequiresSwap(ctx.memberCount, 1)) {
    throw new Error(
      "当前为 3 人群，单人退群会导致非法 2 人群。请让管理员先添加成员，或使用「移除并替换」。",
    );
  }
  validateMemberDelta(ctx.memberCount, 0, 1);

  progress("正在同步群加密状态…");
  const epoch = await syncGroupEpoch(deps.engine, deps.chain, gid);

  progress("请在钱包中确认：退出群聊");
  const out = deps.engine.removeMembers(gid, [ctx.self]);
  await submitGroupCommit(deps.chain, gid, epoch, out, [], [ctx.self]);
  deps.engine.forgetGroup(gid);
  await deps.engine.flush();
}
