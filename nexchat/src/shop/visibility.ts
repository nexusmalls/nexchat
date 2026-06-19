// EN: Product visibility parsing and viewer access checks.
// CN: 商品可见性解析与浏览权限判断。

import type { EntityMemberInfo, EntityProduct, EntityShop } from "@/shop/types";

export interface ParsedVisibility {
  kind: "Public" | "MembersOnly" | "LevelGated";
  minLevel: number;
}

// EN: Parse chain `ProductVisibility` (string or `{ levelGated: N }`).
// CN: 解析链上 `ProductVisibility`（字符串或 `{ levelGated: N }`）。
export function parseVisibility(raw: unknown): ParsedVisibility {
  if (typeof raw === "string") {
    if (raw === "MembersOnly" || raw === "membersOnly") {
      return { kind: "MembersOnly", minLevel: 0 };
    }
    if (raw.startsWith("LevelGated")) {
      const lvl = Number(raw.split(":")[1] ?? (raw.replace(/\D/g, "") || "0"));
      return { kind: "LevelGated", minLevel: lvl };
    }
    return { kind: "Public", minLevel: 0 };
  }
  if (raw && typeof raw === "object") {
    const key = Object.keys(raw)[0];
    const val = (raw as Record<string, unknown>)[key ?? ""];
    if (key === "membersOnly") return { kind: "MembersOnly", minLevel: 0 };
    if (key === "levelGated") return { kind: "LevelGated", minLevel: Number(val ?? 0) };
    if (key === "public") return { kind: "Public", minLevel: 0 };
    if (key) {
      const kind =
        key === "membersOnly"
          ? "MembersOnly"
          : key === "levelGated"
            ? "LevelGated"
            : "Public";
      return {
        kind: kind as ParsedVisibility["kind"],
        minLevel: kind === "LevelGated" ? Number(val ?? 0) : 0,
      };
    }
  }
  return { kind: "Public", minLevel: 0 };
}

export function visibilityLabel(product: EntityProduct): string | null {
  if (product.visibility === "MembersOnly") return "会员专享";
  if (product.visibility === "LevelGated" && product.visibilityMinLevel > 0) {
    return `Lv${product.visibilityMinLevel}+`;
  }
  return null;
}

// EN: Whether product is on sale in an active shop (ignores visibility).
// CN: 商品是否在售且店铺营业（不含可见性）。
export function isOnSaleProduct(
  product: EntityProduct,
  shop: EntityShop | undefined,
): boolean {
  if (product.status !== "OnSale") return false;
  if (!shop || shop.status !== "Active") return false;
  return true;
}

// EN: Viewer can see product in catalog (Public / member / level).
// CN: 当前用户是否可在目录中看到该商品。
export function canViewProduct(
  product: EntityProduct,
  shop: EntityShop | undefined,
  member: EntityMemberInfo | null | undefined,
): boolean {
  if (!isOnSaleProduct(product, shop)) return false;
  if (product.visibility === "Public") return true;
  if (!member?.isMember || member.bannedAt != null) return false;
  if (product.visibility === "MembersOnly") return true;
  if (product.visibility === "LevelGated") {
    return member.level >= product.visibilityMinLevel;
  }
  return false;
}
