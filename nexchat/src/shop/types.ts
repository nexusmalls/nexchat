// EN: Entity shop / product types for NexChat discover shopping (Phase 1 read-only).
// CN: NexChat 发现购物只读阶段——Entity 商铺与商品类型。

export type ShopStatus = "Active" | "Paused" | "Closed" | string;
export type ProductStatus = "OnSale" | "SoldOut" | "Draft" | "OffShelf" | string;
export type ProductVisibility = "Public" | "MembersOnly" | "LevelGated" | string;
export type ProductCategory = "Physical" | "Digital" | "Service" | string;

export type PaymentAsset = "Native" | "ShoppingBalance" | "EntityToken";

export type OrderStatus =
  | "Paid"
  | "Shipped"
  | "Completed"
  | "Disputed"
  | "Refunded"
  | "Cancelled"
  | string;

export interface EntityOrder {
  id: number;
  entityId: number;
  shopId: number;
  productId: number;
  buyer: string;
  seller: string;
  payer: string | null;
  quantity: number;
  unitPrice: string;
  totalAmount: string;
  platformFee: string;
  productCategory: ProductCategory;
  shippingCid: string | null;
  trackingCid: string | null;
  status: OrderStatus;
  createdAt: number;
  shippedAt: number | null;
  completedAt: number | null;
  paymentAsset: PaymentAsset;
  tokenPaymentAmount: string;
  shoppingBalanceUsed: string;
  confirmExtended: boolean;
  disputeRejected: boolean;
  disputeDeadline: number | null;
  noteCid: string | null;
  refundReasonCid: string | null;
}

export interface EntityShop {
  id: number;
  entityId: number;
  name: string;
  logoCid: string | null;
  descriptionCid: string | null;
  status: ShopStatus;
  managers: string[];
  productCount: number;
  totalSales: string;
  totalOrders: number;
  createdAt: number;
}

export interface EntityProduct {
  id: number;
  shopId: number;
  nameCid: string;
  imagesCid: string;
  detailCid: string;
  price: string;
  usdtPrice: number;
  stock: number;
  soldCount: number;
  status: ProductStatus;
  visibility: ProductVisibility;
  /** EN: min level for LevelGated visibility. CN: LevelGated 可见性所需最低等级。 */
  visibilityMinLevel: number;
  category: ProductCategory;
  sortWeight: number;
  minOrderQuantity: number;
  maxOrderQuantity: number;
  createdAt: number;
}

export interface EntityMemberInfo {
  isMember: boolean;
  level: number;
  activated: boolean;
  bannedAt: number | null;
}

export interface ShopCatalog {
  shops: EntityShop[];
  /** EN: Viewer-visible on-sale products (visibility + membership). CN: 当前用户可见的在售商品。 */
  products: EntityProduct[];
  /** EN: All on-sale products in active shops (ignores visibility), sorted by sales. CN: 全部在售商品（不含可见性限制），按销量排序。 */
  allOnSaleProducts: EntityProduct[];
  shopById: Map<number, EntityShop>;
  /** EN: entity_id → member snapshot for signed-in viewer. CN: 当前查看者的 entity 会员快照。 */
  memberByEntity: Map<number, EntityMemberInfo>;
}
