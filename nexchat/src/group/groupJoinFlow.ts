// EN: Resolve join eligibility + lookup aggregation for private groups.
// CN: 私群入群资格判定与查找聚合。

import type { ChainClient, GroupMlsSnapshot } from "@/chain/chainClient";
import type { GroupJoinFlags, GroupJoinStatus, GroupLookupVM } from "@/group/groupJoinTypes";
import { canonicalAddress } from "@/wallet/address";

export function resolveGroupJoinStatus(
  snap: GroupMlsSnapshot | null,
  flags: GroupJoinFlags,
): GroupJoinStatus {
  if (!snap) return "not_found";
  if (snap.isPublic) return "public_group";
  if (snap.frozen) return "group_frozen";
  if (flags.isBanned) return "banned";
  if (flags.isMember) return "already_member";
  if (flags.hasJoinApproval) return "approved_pending_welcome";
  if (flags.hasJoinRequest) return "pending_request";
  if (flags.keyPackageCount <= 0) return "key_package_missing";
  return "not_member";
}

export function buildGroupLookup(args: {
  groupId: number;
  snap: GroupMlsSnapshot | null;
  flags: GroupJoinFlags;
  profile: { name: string; avatarCid: string; announcement: string } | null;
  adminAddress: string;
}): GroupLookupVM {
  const status = resolveGroupJoinStatus(args.snap, args.flags);
  const name =
    args.profile?.name?.trim() ||
    (args.snap ? `群 #${args.groupId}` : `群 #${args.groupId}`);
  return {
    groupId: args.groupId,
    exists: args.snap != null,
    isPublic: args.snap?.isPublic ?? false,
    frozen: args.snap?.frozen ?? false,
    name,
    avatarCid: args.profile?.avatarCid ?? "",
    announcement: args.profile?.announcement ?? "",
    memberCount: args.snap?.memberCount ?? 0,
    adminAddress: args.adminAddress,
    status,
  };
}

export function joinStatusLabel(status: GroupJoinStatus): string {
  switch (status) {
    case "not_found":
      return "群不存在";
    case "public_group":
      return "公开群";
    case "group_frozen":
      return "已冻结";
    case "key_package_missing":
      return "需发布 KeyPackage";
    case "not_member":
      return "可申请加入";
    case "already_member":
      return "已是成员";
    case "pending_request":
      return "等待批准";
    case "approved_pending_welcome":
      return "已批准，待入群";
    case "banned":
      return "已被封禁";
    default:
      return "";
  }
}

export function chainJoinErrorMessage(err: unknown): string {
  const msg = err instanceof Error ? err.message : String(err);
  if (msg.includes("PublicGroupNoApproval")) return "这是公开群，无法自助申请，请联系管理员添加";
  if (msg.includes("AlreadyMember")) return "你已是群成员";
  if (msg.includes("AlreadyRequested")) return "你已提交过申请";
  if (msg.includes("Banned")) return "你已被该群封禁";
  if (msg.includes("GroupFrozen")) return "群已冻结，暂不接受新申请";
  if (msg.includes("GroupNotFound")) return "群不存在";
  if (msg.includes("JoinRequestNotFound")) return "入群申请不存在或已撤回";
  if (msg.includes("TooManyPendingJoins")) return "该群待批申请过多，请稍后再试";
  if (msg.includes("NotApproved")) return "私群须先批准入群申请";
  if (msg.includes("NotAuthorized")) return "仅群主或管理员可操作";
  return msg;
}

/// EN: Aggregate on-chain + profile data for the join preview screen.
/// CN: 聚合链上与资料数据，供加入预览页使用。
export async function lookupGroupForJoin(
  chain: ChainClient,
  groupId: number,
  who: string,
): Promise<GroupLookupVM> {
  const self = canonicalAddress(who);
  const [snap, flags, profile, members] = await Promise.all([
    chain.groupSnapshot(groupId),
    chain.groupJoinFlags(groupId, self),
    chain.groupProfile(groupId),
    chain.listGroupMembers(groupId),
  ]);
  const adminAddress = members.find((m) => m.role === "owner")?.address ?? "";
  return buildGroupLookup({ groupId, snap, flags, profile, adminAddress });
}

const RECENT_KEY = "nexchat:recent-group-lookups:";

export function loadRecentLookups(account: string): import("@/group/groupJoinTypes").RecentGroupLookup[] {
  if (typeof localStorage === "undefined") return [];
  try {
    const raw = localStorage.getItem(RECENT_KEY + account);
    if (!raw) return [];
    const parsed = JSON.parse(raw) as import("@/group/groupJoinTypes").RecentGroupLookup[];
    return Array.isArray(parsed) ? parsed.slice(0, 5) : [];
  } catch {
    return [];
  }
}

export function saveRecentLookup(
  account: string,
  entry: import("@/group/groupJoinTypes").RecentGroupLookup,
): void {
  if (typeof localStorage === "undefined") return;
  const prev = loadRecentLookups(account).filter((r) => r.groupId !== entry.groupId);
  const next = [entry, ...prev].slice(0, 5);
  localStorage.setItem(RECENT_KEY + account, JSON.stringify(next));
}
