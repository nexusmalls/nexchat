// EN: View models for private-group join-by-ID flow.
// CN: 私群按 ID 申请加入流程的视图模型。

export type GroupJoinStatus =
  | "not_found"
  | "public_group"
  | "group_frozen"
  | "key_package_missing"
  | "not_member"
  | "already_member"
  | "pending_request"
  | "approved_pending_welcome"
  | "banned";

export interface GroupLookupVM {
  groupId: number;
  exists: boolean;
  isPublic: boolean;
  frozen: boolean;
  name: string;
  avatarCid: string;
  announcement: string;
  memberCount: number;
  adminAddress: string;
  status: GroupJoinStatus;
}

export interface PendingJoinVM {
  groupId: number;
  title: string;
  avatarCid: string;
  status: "pending" | "approved" | "joining";
}

export interface GroupJoinFlags {
  isMember: boolean;
  hasJoinRequest: boolean;
  hasJoinApproval: boolean;
  isBanned: boolean;
  keyPackageCount: number;
}

/** EN: Pending join request row for group admins. CN: 群主/管理员看到的待批入群申请。 */
export interface GroupJoinRequestRow {
  address: string;
  requestedAt: number;
  approved: boolean;
  hasKeyPackage: boolean;
}

export interface RecentGroupLookup {
  groupId: number;
  title: string;
  lookedAt: number;
}
