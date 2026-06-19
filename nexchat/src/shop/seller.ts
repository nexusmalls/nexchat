// EN: Seller-side shop helpers (managed shops detection).
// CN: 卖家侧商铺辅助（可管理店铺识别）。

import type { EntityShop, ShopCatalog } from "@/shop/types";
import { canonicalAddress } from "@/wallet/address";

// EN: Shop ids where `address` is listed in `managers`, or owns the shop's entity.
// CN: `address` 在 `managers` 中，或为商铺所属 Entity 的 owner 时，返回 shop id。
export function getManagedShopIds(
  catalog: ShopCatalog | null | undefined,
  address: string | null | undefined,
  ownedEntityIds: number[] = [],
): number[] {
  if (!catalog || !address) return [];
  const who = canonicalAddress(address);
  const owned = new Set(ownedEntityIds);
  const ids = new Set<number>();
  for (const s of catalog.shops) {
    if (owned.has(s.entityId)) ids.add(s.id);
    if (s.managers.some((m) => canonicalAddress(m) === who)) ids.add(s.id);
  }
  return [...ids].sort((a, b) => a - b);
}

export function isShopManager(
  shop: EntityShop | undefined,
  address: string | null,
  ownedEntityIds: number[] = [],
): boolean {
  if (!shop || !address) return false;
  if (ownedEntityIds.includes(shop.entityId)) return true;
  const who = canonicalAddress(address);
  return shop.managers.some((m) => canonicalAddress(m) === who);
}
