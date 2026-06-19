// EN: Shell navigation — bottom tabs + settings/contact/discover sub-views.
// CN: 外壳导航——底部 Tab + 设置/联系人/发现子视图。

import { create } from "zustand";
import { decodeHexUtf8String } from "@/mls/chainBytes";

export type MainTab = "chats" | "contacts" | "discover" | "me";

export type SettingsView =
  | "list"
  | "profile"
  | "wallet"
  | "entity"
  | "earnings"
  | "earningsMultiLevel"
  | "earningsSingleLine"
  | "earningsPoolReward"
  | "staking"
  | "chain"
  | "language"
  | "sendingKey"
  | "privacy"
  | "notifications"
  | "data"
  | "about";

export type DiscoverView = "list" | "market" | "prediction" | "shop";

export type ShopScreen =
  | "hub"
  | "shop"
  | "product"
  | "order"
  | "orders"
  | "orderDetail"
  | "addProduct";

export type ShopOrdersTab = "buyer" | "seller";

export interface ShopNav {
  screen: ShopScreen;
  shopId: number | null;
  productId: number | null;
  orderId: number | null;
  /** EN: orders list tab when screen=orders. CN: screen=orders 时的列表 Tab。 */
  ordersTab: ShopOrdersTab;
  /** EN: optional shop filter on seller orders. CN: 卖家订单可选商铺筛选。 */
  ordersShopFilter: number | null;
}

const DEFAULT_SHOP_NAV: ShopNav = {
  screen: "hub",
  shopId: null,
  productId: null,
  orderId: null,
  ordersTab: "buyer",
  ordersShopFilter: null,
};

const CURRENT_ENTITY_ID_KEY = "nexchat-current-entity-id";
const CURRENT_ENTITY_NAME_KEY = "nexchat-current-entity-name";

function readCurrentEntityId(): number | null {
  if (typeof localStorage === "undefined") return null;
  const raw =
    localStorage.getItem(CURRENT_ENTITY_ID_KEY) ??
    localStorage.getItem("nexchat-shop-entity-id");
  if (!raw) return null;
  const n = Number(raw);
  return Number.isFinite(n) && n > 0 ? n : null;
}

function readCurrentEntityName(): string | null {
  if (typeof localStorage === "undefined") return null;
  const name = localStorage.getItem(CURRENT_ENTITY_NAME_KEY);
  return name && name.length > 0 ? name : null;
}

interface UiState {
  mainTab: MainTab;
  settingsView: SettingsView;
  discoverView: DiscoverView;
  shopNav: ShopNav;
  /** EN: Global selected entity (Me tab; shared by earnings / shopping). CN: 全局选中实体（「我」页；收益/购物共用）。 */
  currentEntityId: number | null;
  currentEntityName: string | null;
  selectedContact: string | null;
  selectedContactRequestId: string | null;
  /** EN: selected group_invite row in contacts tab. CN: 联系人 Tab 选中的群邀请。 */
  selectedGroupInviteId: string | null;
  /** EN: selected joined group (`g:{id}`) in contacts tab. CN: 联系人 Tab 选中的已加入群。 */
  selectedGroupConvId: string | null;
  /** EN: product share picker draft. CN: 商品分享会话选择器草稿。 */
  shareProductDraft: { productId: number; shopId: number; label: string } | null;
  /** EN: open register-shop wizard from Me or Shopping. CN: 从「我/购物」打开注册开店向导。 */
  registerShopOpen: boolean;

  setMainTab: (tab: MainTab) => void;
  setSettingsView: (view: SettingsView) => void;
  setDiscoverView: (view: DiscoverView) => void;
  openMarket: () => void;
  openPrediction: () => void;
  openShop: () => void;
  setCurrentEntity: (entityId: number | null, entityName?: string | null) => void;
  openShopDetail: (shopId: number) => void;
  openAddProduct: (shopId: number) => void;
  openProductDetail: (productId: number, shopId?: number) => void;
  openProductOrder: (productId: number, shopId?: number) => void;
  openShopOrders: (tab?: ShopOrdersTab, shopFilter?: number | null) => void;
  openShopOrderDetail: (orderId: number) => void;
  openShareProductPicker: (productId: number, shopId: number, label?: string) => void;
  closeShareProductPicker: () => void;
  backShop: () => void;
  openRegisterShop: () => void;
  closeRegisterShop: () => void;
  selectContact: (address: string | null) => void;
  selectContactRequest: (reqId: string | null) => void;
  selectGroupInvite: (inviteId: string | null) => void;
  selectGroup: (convId: string | null) => void;
}

export const useUiStore = create<UiState>((set, get) => ({
  mainTab: "chats",
  settingsView: "list",
  discoverView: "list",
  shopNav: DEFAULT_SHOP_NAV,
  currentEntityId: readCurrentEntityId(),
  currentEntityName: readCurrentEntityName(),
  selectedContact: null,
  selectedContactRequestId: null,
  selectedGroupInviteId: null,
  selectedGroupConvId: null,
  shareProductDraft: null,
  registerShopOpen: false,

  setMainTab: (tab) =>
    set((s) => ({
      mainTab: tab,
      // EN: Re-tap 「我」 returns to the entry list (WeChat-style). CN: 再次点「我」回到入口列表。
      settingsView: tab !== "me" ? "list" : s.mainTab === "me" ? "list" : s.settingsView,
      discoverView: tab === "discover" ? s.discoverView : "list",
      shopNav: tab === "discover" && s.discoverView === "shop" ? s.shopNav : DEFAULT_SHOP_NAV,
      selectedContact: tab === "contacts" ? s.selectedContact : null,
      selectedContactRequestId: tab === "contacts" ? s.selectedContactRequestId : null,
      selectedGroupInviteId: tab === "contacts" ? s.selectedGroupInviteId : null,
      selectedGroupConvId: tab === "contacts" ? s.selectedGroupConvId : null,
    })),

  setSettingsView: (view) => set({ settingsView: view, mainTab: "me" }),

  setDiscoverView: (view) =>
    set({
      discoverView: view,
      mainTab: "discover",
      shopNav: view === "shop" ? get().shopNav : DEFAULT_SHOP_NAV,
    }),

  openMarket: () =>
    set({
      mainTab: "discover",
      discoverView: "market",
      shopNav: DEFAULT_SHOP_NAV,
    }),

  openPrediction: () =>
    set({
      mainTab: "discover",
      discoverView: "prediction",
      shopNav: DEFAULT_SHOP_NAV,
    }),

  openShop: () =>
    set({
      mainTab: "discover",
      discoverView: "shop",
      shopNav: DEFAULT_SHOP_NAV,
    }),

  setCurrentEntity: (entityId, entityName) => {
    const normalizedName =
      entityName != null && entityName.length > 0
        ? decodeHexUtf8String(entityName).trim() || entityName
        : entityName;
    if (typeof localStorage !== "undefined") {
      if (entityId == null) {
        localStorage.removeItem(CURRENT_ENTITY_ID_KEY);
        localStorage.removeItem(CURRENT_ENTITY_NAME_KEY);
      } else {
        localStorage.setItem(CURRENT_ENTITY_ID_KEY, String(entityId));
        if (entityName != null && entityName.length > 0) {
          localStorage.setItem(CURRENT_ENTITY_NAME_KEY, normalizedName ?? entityName);
        }
      }
    }
    set({
      currentEntityId: entityId,
      currentEntityName:
        entityName !== undefined
          ? normalizedName ?? null
          : entityId == null
            ? null
            : get().currentEntityName,
    });
  },

  openShopDetail: (shopId) =>
    set({
      mainTab: "discover",
      discoverView: "shop",
      shopNav: {
        ...DEFAULT_SHOP_NAV,
        screen: "shop",
        shopId,
      },
    }),

  openAddProduct: (shopId) =>
    set({
      mainTab: "discover",
      discoverView: "shop",
      shopNav: {
        ...DEFAULT_SHOP_NAV,
        screen: "addProduct",
        shopId,
      },
    }),

  openProductDetail: (productId, shopId) =>
    set({
      mainTab: "discover",
      discoverView: "shop",
      shopNav: {
        ...DEFAULT_SHOP_NAV,
        screen: "product",
        shopId: shopId ?? get().shopNav.shopId,
        productId,
      },
    }),

  openProductOrder: (productId, shopId) =>
    set({
      mainTab: "discover",
      discoverView: "shop",
      shopNav: {
        ...DEFAULT_SHOP_NAV,
        screen: "order",
        shopId: shopId ?? get().shopNav.shopId,
        productId,
      },
    }),

  openShopOrders: (tab = "buyer", shopFilter = null) =>
    set({
      mainTab: "discover",
      discoverView: "shop",
      shopNav: {
        screen: "orders",
        shopId: shopFilter,
        productId: null,
        orderId: null,
        ordersTab: tab,
        ordersShopFilter: shopFilter,
      },
    }),

  openShopOrderDetail: (orderId) =>
    set((s) => ({
      mainTab: "discover",
      discoverView: "shop",
      shopNav: {
        ...s.shopNav,
        screen: "orderDetail",
        productId: null,
        orderId,
      },
    })),

  openShareProductPicker: (productId, shopId, label = "") =>
    set({
      shareProductDraft: { productId, shopId, label },
    }),

  closeShareProductPicker: () => set({ shareProductDraft: null }),

  openRegisterShop: () =>
    set({
      mainTab: "me",
      settingsView: "entity",
      registerShopOpen: true,
    }),

  closeRegisterShop: () => set({ registerShopOpen: false }),

  backShop: () => {
    const { shopNav } = get();
    if (shopNav.screen === "orderDetail") {
      set({
        shopNav: {
          ...shopNav,
          screen: "orders",
          productId: null,
          orderId: null,
          shopId: shopNav.ordersShopFilter,
        },
      });
      return;
    }
    if (shopNav.screen === "orders") {
      set({ shopNav: DEFAULT_SHOP_NAV });
      return;
    }
    if (shopNav.screen === "order" && shopNav.productId != null) {
      set({
        shopNav: {
          ...DEFAULT_SHOP_NAV,
          screen: "product",
          shopId: shopNav.shopId,
          productId: shopNav.productId,
        },
      });
      return;
    }
    if (shopNav.screen === "product") {
      if (shopNav.shopId != null) {
        set({
          shopNav: {
            ...DEFAULT_SHOP_NAV,
            screen: "shop",
            shopId: shopNav.shopId,
          },
        });
      } else {
        set({ shopNav: DEFAULT_SHOP_NAV });
      }
      return;
    }
    if (shopNav.screen === "shop") {
      set({ shopNav: DEFAULT_SHOP_NAV });
      return;
    }
    if (shopNav.screen === "addProduct" && shopNav.shopId != null) {
      set({
        shopNav: {
          ...DEFAULT_SHOP_NAV,
          screen: "shop",
          shopId: shopNav.shopId,
        },
      });
      return;
    }
    set({ discoverView: "list", shopNav: DEFAULT_SHOP_NAV });
  },

  selectContact: (address) =>
    set({
      selectedContact: address,
      selectedContactRequestId: null,
      selectedGroupInviteId: null,
      selectedGroupConvId: null,
      mainTab: "contacts",
    }),

  selectContactRequest: (reqId) =>
    set({
      selectedContactRequestId: reqId,
      selectedContact: null,
      selectedGroupInviteId: null,
      selectedGroupConvId: null,
      mainTab: "contacts",
    }),

  selectGroupInvite: (inviteId) =>
    set({
      selectedGroupInviteId: inviteId,
      selectedContact: null,
      selectedContactRequestId: null,
      selectedGroupConvId: null,
      mainTab: "contacts",
    }),

  selectGroup: (convId) =>
    set({
      selectedGroupConvId: convId,
      selectedContact: null,
      selectedContactRequestId: null,
      selectedGroupInviteId: null,
      mainTab: "contacts",
    }),
}));
