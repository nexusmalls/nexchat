// EN: Client-side shop catalog search and category filter (Phase 3).
// CN: 购物目录客户端搜索与分类筛选（Phase 3）。

import type { EntityProduct, EntityShop, ProductCategory } from "@/shop/types";

export function normalizeSearchQuery(q: string): string {
  return q.trim().toLowerCase();
}

export function filterProductsByCategory(
  products: EntityProduct[],
  category: ProductCategory | "all",
): EntityProduct[] {
  if (category === "all") return products;
  return products.filter((p) => p.category === category);
}

export function filterProductsBySearch(
  products: EntityProduct[],
  nameMap: Map<string, string>,
  query: string,
): EntityProduct[] {
  const q = normalizeSearchQuery(query);
  if (!q) return products;
  return products.filter((p) => {
    const name = nameMap.get(p.nameCid)?.toLowerCase() ?? "";
    const idStr = String(p.id);
    return name.includes(q) || idStr.includes(q);
  });
}

export function filterShopsBySearch(shops: EntityShop[], query: string): EntityShop[] {
  const q = normalizeSearchQuery(query);
  if (!q) return shops;
  return shops.filter((s) => {
    const name = s.name.toLowerCase();
    const idStr = String(s.id);
    return name.includes(q) || idStr.includes(q);
  });
}
