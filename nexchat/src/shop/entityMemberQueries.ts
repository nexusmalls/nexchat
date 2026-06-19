// EN: On-chain entity member read queries (entityMember + MemberTeamApi).
// CN: 链上 Entity 会员只读查询（storage + MemberTeamApi）。

import type { EntityMemberInfo } from "@/shop/types";
import { canonicalAddress } from "@/wallet/address";

type StorageQuery = {
  (...args: unknown[]): Promise<unknown>;
};

export type EntityMemberApi = {
  query: {
    entityMember: Record<string, StorageQuery>;
  };
  call?: {
    memberTeamApi?: {
      getMemberInfo: (
        entityId: number,
        account: string,
      ) => Promise<{
        isNone?: boolean;
        unwrap?: () => { toJSON: () => Record<string, unknown> };
      }>;
    };
  };
};

type RawOption = {
  isNone?: boolean;
  unwrap?: () => { toJSON: () => Record<string, unknown> };
};

function parseMemberFromStorage(data: Record<string, unknown>): EntityMemberInfo {
  const banned = data.bannedAt ?? data.banned_at;
  return {
    isMember: true,
    level: Number(data.customLevelId ?? data.custom_level_id ?? 0),
    activated: Boolean(data.activated ?? false),
    bannedAt: banned != null ? Number(banned) : null,
  };
}

function parseMemberFromRuntime(data: Record<string, unknown>): EntityMemberInfo {
  const banned = data.bannedAt ?? data.banned_at;
  const isBanned = Boolean(data.isBanned ?? data.is_banned ?? false);
  return {
    isMember: true,
    level: Number(
      data.effectiveLevelId ??
        data.effective_level_id ??
        data.customLevelId ??
        data.custom_level_id ??
        0,
    ),
    activated: Boolean(data.activated ?? false),
    bannedAt: isBanned && banned != null ? Number(banned) : null,
  };
}

// EN: Fetch member via storage (`entityMember.entityMembers`).
// CN: 通过 storage 查询会员。
export async function fetchEntityMember(
  api: EntityMemberApi,
  entityId: number,
  address: string,
): Promise<EntityMemberInfo | null> {
  const q = api.query.entityMember;
  if (!q?.entityMembers) return null;
  const who = canonicalAddress(address);
  const raw = (await q.entityMembers(entityId, who)) as RawOption;
  if (raw?.isNone) return null;
  return parseMemberFromStorage(raw.unwrap!().toJSON());
}

// EN: Fetch member with effective level via `MemberTeamApi` when available.
// CN: 优先用 `MemberTeamApi` 获取含有效等级的会员信息。
export async function fetchEntityMemberWithLevel(
  api: EntityMemberApi,
  entityId: number,
  address: string,
): Promise<EntityMemberInfo | null> {
  const who = canonicalAddress(address);
  const runtime = api.call?.memberTeamApi?.getMemberInfo;
  if (runtime) {
    try {
      const raw = await runtime(entityId, who);
      if (!raw?.isNone && raw.unwrap) {
        return parseMemberFromRuntime(raw.unwrap().toJSON());
      }
      if (raw?.isNone) return null;
    } catch {
      /* fall through to storage */
    }
  }
  return fetchEntityMember(api, entityId, address);
}

// EN: Batch membership lookup for multiple entities.
// CN: 批量查询多个 entity 的会员状态。
export async function fetchMembershipsForEntities(
  api: EntityMemberApi,
  entityIds: number[],
  address: string,
): Promise<Map<number, EntityMemberInfo>> {
  const unique = [...new Set(entityIds)].filter((id) => id > 0);
  const map = new Map<number, EntityMemberInfo>();
  await Promise.all(
    unique.map(async (entityId) => {
      const m = await fetchEntityMemberWithLevel(api, entityId, address);
      if (m) map.set(entityId, m);
    }),
  );
  return map;
}
