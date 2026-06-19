// EN: Shared helpers for on-chain group member commits (add / remove / swap).
// CN: 链上群成员 commit（加人 / 踢人 / 替换）共用辅助逻辑。

import type { ChainClient } from "@/chain/chainClient";
import { hex, hexToBytes } from "@/mls/chainBytes";
import type { OpenMlsEngine } from "@/mls/openMlsEngine";
import { canonicalAddress, shortAddress } from "@/wallet/address";
import type { GroupRole } from "@/types/viewModels";

export const FUND_PLANCK = 100_000_000_000_000n;
export const FUND_THRESHOLD = 10_000_000_000_000n;
export const KP_WAIT_MS = 90_000;
export const KP_POLL_MS = 2_000;

export interface GroupMemberRow {
  address: string;
  role: GroupRole;
}

export function sleep(ms: number): Promise<void> {
  return new Promise((r) => setTimeout(r, ms));
}

export function roleFromChain(raw: number): GroupRole {
  if (raw === 0) return "owner";
  if (raw === 1) return "admin";
  if (raw === 2) return "member";
  return "na";
}

/// EN: Guard `TwoMemberGroupForbidden` before submit. CN: 提交前校验禁止 2 人群。
export function validateMemberDelta(
  currentCount: number,
  addCount: number,
  removeCount: number,
): void {
  if (addCount < 0 || removeCount < 0) throw new Error("成员变更参数无效");
  const newCount = currentCount + addCount - removeCount;
  if (newCount < 1) throw new Error("群成员数不能少于 1 人");
  if (newCount === 2) {
    throw new Error(
      "本次变更会导致群成员恰好为 2 人（链上不允许）。3 人群踢人须同时添加替换成员。",
    );
  }
}

/// EN: True when remove-only would hit TwoMemberGroupForbidden. CN: 仅移除会触发 2 人禁止。
export function removeRequiresSwap(currentCount: number, removeCount: number): boolean {
  return currentCount - removeCount === 2;
}

export async function fundMembers(
  chain: ChainClient,
  self: string,
  members: string[],
): Promise<void> {
  for (const m of members) {
    if (m === self) continue;
    try {
      const bal = await chain.freeBalance(m);
      if (bal < FUND_THRESHOLD) {
        await chain.signAndSend("balances", "transferKeepAlive", [m, FUND_PLANCK]);
      }
    } catch (e) {
      console.warn("[groupMember] fund member skipped:", m, e);
    }
  }
}

export async function waitForKeyPackages(
  chain: ChainClient,
  members: string[],
  min = 1,
): Promise<{ addr: string; kp: Uint8Array }[]> {
  const deadline = Date.now() + KP_WAIT_MS;
  while (Date.now() < deadline) {
    const ready: { addr: string; kp: Uint8Array }[] = [];
    for (const addr of members) {
      const who = canonicalAddress(addr);
      const kps = await chain.keyPackagesOf(who);
      if (kps.length > 0) ready.push({ addr: who, kp: kps[0]! });
    }
    if (ready.length >= min) return ready;
    await sleep(KP_POLL_MS);
  }
  throw new Error(
    "等待成员 KeyPackage 超时：请让对方解锁 NexChat 并保持在线（需发布 KeyPackage）",
  );
}

export async function assertMembersJoinable(chain: ChainClient, addrs: string[]): Promise<void> {
  const missing: string[] = [];
  for (const addr of addrs) {
    const who = canonicalAddress(addr);
    const kps = await chain.keyPackagesOf(who);
    if (kps.length <= 0) missing.push(who);
  }
  if (missing.length === 0) return;
  const hint = missing.map((a) => shortAddress(a)).join("、");
  throw new Error(
    `成员 ${hint} 尚未发布 KeyPackage：请让对方解锁 NexChat 并在钱包中确认发布交易后重试`,
  );
}

export async function syncGroupEpoch(
  engine: OpenMlsEngine,
  chain: ChainClient,
  groupId: number,
): Promise<number> {
  const snap = await chain.groupSnapshot(groupId);
  if (!snap) throw new Error("群不存在");
  let local = engine.epochOf(groupId);
  while (local < snap.epoch) {
    const next = local + 1;
    const cHex = await chain.handshakeAtEpoch(groupId, next);
    if (!cHex) break;
    engine.processCommit(groupId, hexToBytes(cHex));
    local = engine.epochOf(groupId);
  }
  if (local !== snap.epoch) {
    throw new Error("MLS epoch 与链不同步，请稍后重试");
  }
  return local;
}

export interface CommitAuthContext {
  self: string;
  groupRole: GroupRole;
  memberCount: number;
  frozen: boolean;
}

export async function loadCommitAuth(
  chain: ChainClient,
  selfAddress: string,
  groupId: number,
): Promise<CommitAuthContext> {
  const self = canonicalAddress(selfAddress);
  const [snap, rows, frozen] = await Promise.all([
    chain.groupSnapshot(groupId),
    chain.listConversations(self),
    chain.isGroupFrozen(groupId),
  ]);
  if (!snap) throw new Error("群不存在");
  const mine = rows.find((r) => r.kind === "group" && r.groupId === groupId);
  if (!mine) throw new Error("你不在该群成员列表中");
  return {
    self,
    groupRole: roleFromChain(mine.groupRole),
    memberCount: snap.memberCount,
    frozen,
  };
}

export function assertCanRemoveOthers(ctx: CommitAuthContext): void {
  if (ctx.frozen) throw new Error("群已冻结，无法变更成员");
  if (ctx.groupRole !== "owner" && ctx.groupRole !== "admin") {
    throw new Error("仅群主或管理员可以移除其他成员");
  }
}

export function assertCanLeave(ctx: CommitAuthContext): void {
  if (ctx.frozen) throw new Error("群已冻结，无法退群");
  if (ctx.groupRole === "owner") {
    throw new Error("群主须先转让群主后才能退群");
  }
}

/// EN: Owner-only guard for `disband_group` (no MLS commit; chain teardown only). CN: 仅群主可调用
/// `disband_group`（无需 MLS commit，仅链上拆除）。
export function assertCanDisband(ctx: CommitAuthContext): void {
  if (ctx.groupRole !== "owner") {
    throw new Error("仅群主可以解散群");
  }
}

export async function submitGroupCommit(
  chain: ChainClient,
  groupId: number,
  epoch: number,
  out: {
    commit: Uint8Array;
    tree_hash: Uint8Array;
    transcript_hash: Uint8Array;
    welcome: Uint8Array;
  },
  added: string[],
  removed: string[],
): Promise<void> {
  const welcomeHex = hex(out.welcome);
  const welcomes = added.map((addr) => [addr, welcomeHex] as [string, string]);
  await chain.signAndSend("chatGroup", "commit", [
    groupId,
    epoch,
    hex(out.commit),
    hex(out.tree_hash),
    hex(out.transcript_hash),
    hex(new Uint8Array([2])),
    welcomes,
    { added, removed },
  ]);
}
