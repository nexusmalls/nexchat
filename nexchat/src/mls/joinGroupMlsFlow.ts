// EN: Join / catch-up OpenMLS state for an on-chain group the user already belongs to.
// CN: 用户已在链上的群，补齐本地 OpenMLS 状态（领取 Welcome + 回放 Commit）。

import type { ChainClient } from "@/chain/chainClient";
import { hexToBytes } from "@/mls/chainBytes";
import { syncGroupEpoch } from "@/mls/groupMemberFlow";
import type { OpenMlsEngine } from "@/mls/openMlsEngine";
import { canonicalAddress } from "@/wallet/address";

export interface EnsureGroupMlsDeps {
  engine: OpenMlsEngine;
  chain: ChainClient;
  selfAddress: string;
  groupId: number;
}

export type EnsureGroupMlsResult =
  | { ok: true }
  | { ok: false; reason: "not_member" | "welcome_pending" | "local_state_lost" | "sync_failed" };

/// EN: Ensure local OpenMLS has `g:{groupId}` when the account is already a chain member.
/// CN: 账户已是链上成员时，确保本地 OpenMLS 持有 `g:{groupId}`。
export async function ensureGroupMlsReady(
  deps: EnsureGroupMlsDeps,
): Promise<EnsureGroupMlsResult> {
  const self = canonicalAddress(deps.selfAddress);
  const gid = deps.groupId;
  const convKey = `g:${gid}`;

  if (deps.engine.hasGroup(convKey)) {
    try {
      await syncGroupEpoch(deps.engine, deps.chain, gid);
      await deps.engine.flush();
      return { ok: true };
    } catch {
      return { ok: false, reason: "sync_failed" };
    }
  }

  const rows = await deps.chain.listConversations(self);
  const mine = rows.find((r) => r.kind === "group" && r.groupId === gid);
  if (!mine) return { ok: false, reason: "not_member" };

  const snap = await deps.chain.groupSnapshot(gid);

  // EN: Joiner path — read Welcome before claim_welcome deletes it.
  // CN: 新成员路径——在 claim_welcome 删除前先读取 Welcome。
  const wHex = await deps.chain.pendingWelcome(gid, self);
  if (wHex) {
    try {
      await deps.engine.processWelcome(gid, hexToBytes(wHex));
      await deps.chain.signAndSend("chatGroup", "claimWelcome", [gid]);
      await syncGroupEpoch(deps.engine, deps.chain, gid);
      await deps.engine.flush();
      return deps.engine.hasGroup(convKey) ? { ok: true } : { ok: false, reason: "sync_failed" };
    } catch {
      return { ok: false, reason: "sync_failed" };
    }
  }

  // EN: Owner-only solo group at epoch 0 — recreate local MLS from chain group id.
  // CN: 仅群主、epoch 0、单人群——用链上群 id 重建本地 MLS。
  if (
    mine.groupRole === 0 &&
    snap &&
    snap.epoch === 0 &&
    snap.memberCount === 1
  ) {
    try {
      deps.engine.createGroup(gid);
      await deps.engine.flush();
      return deps.engine.hasGroup(convKey) ? { ok: true } : { ok: false, reason: "sync_failed" };
    } catch {
      return { ok: false, reason: "sync_failed" };
    }
  }

  if (mine.groupRole === 0 && snap && snap.epoch > 0) {
    return { ok: false, reason: "local_state_lost" };
  }

  return { ok: false, reason: "welcome_pending" };
}

/// EN: Best-effort sync for every group in the user's conversation list.
/// CN: 对会话列表中的每个群尽力同步 MLS。
export async function syncAllGroupMls(
  engine: OpenMlsEngine,
  chain: ChainClient,
  selfAddress: string,
): Promise<void> {
  const self = canonicalAddress(selfAddress);
  const rows = await chain.listConversations(self);
  const memberGids = new Set(
    rows.filter((r) => r.kind === "group" && r.groupId != null).map((r) => r.groupId as number),
  );
  for (const gid of memberGids) {
    try {
      await ensureGroupMlsReady({ engine, chain, selfAddress: self, groupId: gid });
    } catch (e) {
      console.warn("[nexchat] group MLS sync skipped:", gid, e);
    }
  }
  await pruneStaleLocalGroups(engine, chain, self, memberGids);
}

/// EN: Drop local MLS groups the account no longer belongs to on-chain.
/// CN: 清理链上已不在群但本地仍残留的 MLS 群状态。
export async function pruneStaleLocalGroups(
  engine: OpenMlsEngine,
  chain: ChainClient,
  selfAddress: string,
  memberGids?: Set<number>,
): Promise<void> {
  const self = canonicalAddress(selfAddress);
  const gids =
    memberGids ??
    new Set(
      (await chain.listConversations(self))
        .filter((r) => r.kind === "group" && r.groupId != null)
        .map((r) => r.groupId as number),
    );
  for (const convKey of engine.listGroups()) {
    if (!convKey.startsWith("g:")) continue;
    const gid = Number(convKey.slice(2));
    if (!Number.isFinite(gid)) continue;
    if (gids.has(gid)) continue;
    engine.forgetGroupByConv(convKey);
    await engine.flush();
  }
}

export function ensureGroupMlsErrorMessage(result: EnsureGroupMlsResult): string {
  if (result.ok) return "";
  switch (result.reason) {
    case "not_member":
      return "你不在该群成员列表中";
    case "welcome_pending":
      return "加密入群凭证尚未就绪，请稍候或让群主重新提交加人";
    case "local_state_lost":
      return "本地 MLS 加密状态缺失（若曾清除浏览器数据，需重新建群或由管理员替换成员）";
    case "sync_failed":
      return "MLS 同步失败，请稍后重试";
    default:
      return "MLS 群尚未就绪";
  }
}
