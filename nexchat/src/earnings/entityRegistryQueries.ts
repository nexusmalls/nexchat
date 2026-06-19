// EN: Read queries for `entityRegistry.entities` (join-entity picker).
// CN: `entityRegistry.entities` 只读查询（加入实体选择器）。

import type { RegistryEntity } from "@/earnings/types";
import { decodeChainText, unwrapChainJson } from "@/mls/chainBytes";

type StorageQuery = {
  (...args: unknown[]): Promise<unknown>;
  entries?: () => Promise<unknown>;
};

export type EntityRegistryApi = {
  query: {
    entityRegistry?: Record<string, StorageQuery>;
  };
};

type RawOption = {
  isNone?: boolean;
  unwrap?: () => { toJSON: () => Record<string, unknown> };
};

function bytesToString(raw: unknown): string {
  return decodeChainText(raw);
}

function parseChainEnum(raw: unknown, fallback: string): string {
  if (typeof raw === "string") return raw;
  if (raw && typeof raw === "object") {
    const key = Object.keys(raw)[0];
    if (key) return parseChainEnum(key, fallback);
  }
  return fallback;
}

function parseRegistryEntity(
  data: Record<string, unknown>,
  opts?: { activeOnly?: boolean },
): RegistryEntity | null {
  const id = Number(data.id ?? 0);
  if (!Number.isFinite(id) || id <= 0) return null;
  const status = parseChainEnum(data.status, "Active");
  if (opts?.activeOnly !== false && status !== "Active") return null;
  const name = bytesToString(data.name).trim() || `Entity #${id}`;
  const primaryShopId = Number(data.primaryShopId ?? data.primary_shop_id ?? 0);
  return {
    id,
    name,
    primaryShopId,
    verified: Boolean(data.verified ?? false),
    entityType: parseChainEnum(data.entityType ?? data.entity_type, "Merchant"),
    status,
  };
}

// EN: Fetch entity ids owned by `address` (`entityRegistry.userEntity`).
// CN: 拉取账户拥有的 Entity id（`entityRegistry.userEntity`）。
export async function fetchUserEntityIds(
  api: EntityRegistryApi,
  address: string,
): Promise<number[]> {
  const q = api.query.entityRegistry?.userEntity;
  if (!q) return [];
  const raw = (await q(address)) as RawOption & { toJSON?: () => unknown };
  if (raw?.isNone) return [];
  const vec = raw.toJSON?.() ?? raw;
  if (!Array.isArray(vec)) return [];
  return vec.map((v) => Number(v)).filter((id) => Number.isFinite(id) && id > 0);
}

// EN: Owned entities for current account (any registry status).
// CN: 当前账户拥有的 Entity（不限 Active）。
export async function fetchOwnedEntities(
  api: EntityRegistryApi,
  address: string,
): Promise<RegistryEntity[]> {
  const ids = await fetchUserEntityIds(api, address);
  const out: RegistryEntity[] = [];
  for (const id of ids) {
    const reg = await fetchRegistryEntityById(api, id);
    if (reg) out.push(reg);
  }
  out.sort((a, b) => a.id - b.id);
  return out;
}

// EN: List all active entities from chain registry.
// CN: 列出链上 registry 中所有活跃 Entity。
export async function fetchAllActiveEntities(api: EntityRegistryApi): Promise<RegistryEntity[]> {
  const q = api.query.entityRegistry?.entities;
  if (!q?.entries) return [];
  const entries = (await q.entries()) as Array<[unknown, RawOption]>;
  const out: RegistryEntity[] = [];
  for (const [, raw] of entries) {
    if (raw?.isNone) continue;
    const data = unwrapChainJson(raw);
    if (!data) continue;
    const parsed = parseRegistryEntity(data, { activeOnly: true });
    if (parsed) out.push(parsed);
  }
  out.sort((a, b) => a.name.localeCompare(b.name, "zh-CN") || a.id - b.id);
  return out;
}

// EN: Fetch one entity by id (any registry status; for manual ID setup).
// CN: 按 id 拉取单个 Entity（不限 Active；用于手动按 ID 设置）。
export async function fetchRegistryEntityById(
  api: EntityRegistryApi,
  entityId: number,
): Promise<RegistryEntity | null> {
  if (!Number.isFinite(entityId) || entityId <= 0) return null;
  const q = api.query.entityRegistry?.entities;
  if (!q) return null;
  const raw = (await q(entityId)) as RawOption;
  if (raw?.isNone) return null;
  const data = unwrapChainJson(raw);
  if (!data) return null;
  return parseRegistryEntity(data, { activeOnly: false });
}
