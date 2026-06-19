// EN: Entity order display helpers (mirrors nexus-com-dapp use-order).
// CN: Entity 订单展示辅助（对齐 nexus-com-dapp use-order）。

import { formatNexBalance } from "@/shop/pricing";
import type { EntityOrder } from "@/shop/types";

export function getOrderDisplayAmount(
  order: Pick<
    EntityOrder,
    "paymentAsset" | "totalAmount" | "tokenPaymentAmount" | "shoppingBalanceUsed"
  >,
): string {
  switch (order.paymentAsset) {
    case "ShoppingBalance":
      return order.shoppingBalanceUsed;
    case "EntityToken":
      return order.tokenPaymentAmount;
    case "Native":
    default:
      return order.totalAmount;
  }
}

export function getOrderDisplayUnit(
  order: Pick<EntityOrder, "paymentAsset">,
): "NEX" | "Entity Token" {
  return order.paymentAsset === "EntityToken" ? "Entity Token" : "NEX";
}

export function getOrderPaymentLabel(
  order: Pick<EntityOrder, "paymentAsset">,
): string {
  switch (order.paymentAsset) {
    case "ShoppingBalance":
      return "购物余额";
    case "EntityToken":
      return "Entity Token";
    case "Native":
    default:
      return "NEX";
  }
}

export function formatOrderAmount(order: EntityOrder): string {
  const raw = getOrderDisplayAmount(order);
  const unit = getOrderDisplayUnit(order);
  return `${formatNexBalance(raw)} ${unit}`;
}

const STATUS_LABEL: Record<string, string> = {
  Paid: "待发货",
  Shipped: "待收货",
  Completed: "已完成",
  Disputed: "争议中",
  Refunded: "已退款",
  Cancelled: "已取消",
};

export function orderStatusLabel(status: string): string {
  return STATUS_LABEL[status] ?? status;
}

export type OrderStatusFilter =
  | "all"
  | "active"
  | "shipped"
  | "completed"
  | "disputed"
  | "closed";

export function matchOrderStatusFilter(
  status: string,
  filter: OrderStatusFilter,
): boolean {
  switch (filter) {
    case "all":
      return true;
    case "active":
      return status === "Paid";
    case "shipped":
      return status === "Shipped";
    case "completed":
      return status === "Completed";
    case "disputed":
      return status === "Disputed";
    case "closed":
      return status === "Refunded" || status === "Cancelled";
  }
}
