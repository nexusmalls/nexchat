import { describe, expect, it } from "vitest";
import {
  getOrderDisplayAmount,
  getOrderPaymentLabel,
  matchOrderStatusFilter,
  orderStatusLabel,
} from "@/shop/orderFormat";
import type { EntityOrder } from "@/shop/types";

function order(partial: Partial<EntityOrder>): EntityOrder {
  return {
    id: 1,
    entityId: 1,
    shopId: 1,
    productId: 1,
    buyer: "",
    seller: "",
    payer: null,
    quantity: 1,
    unitPrice: "0",
    totalAmount: "1000",
    platformFee: "0",
    productCategory: "Physical",
    shippingCid: null,
    trackingCid: null,
    status: "Paid",
    createdAt: 0,
    shippedAt: null,
    completedAt: null,
    paymentAsset: "Native",
    tokenPaymentAmount: "0",
    shoppingBalanceUsed: "500",
    confirmExtended: false,
    disputeRejected: false,
    disputeDeadline: null,
    noteCid: null,
    refundReasonCid: null,
    ...partial,
  };
}

describe("shop/orderFormat", () => {
  it("getOrderDisplayAmount by payment asset", () => {
    expect(getOrderDisplayAmount(order({ paymentAsset: "Native" }))).toBe("1000");
    expect(
      getOrderDisplayAmount(order({ paymentAsset: "ShoppingBalance" })),
    ).toBe("500");
  });

  it("orderStatusLabel and filter", () => {
    expect(orderStatusLabel("Paid")).toBe("待发货");
    expect(matchOrderStatusFilter("Shipped", "shipped")).toBe(true);
    expect(matchOrderStatusFilter("Paid", "shipped")).toBe(false);
    expect(matchOrderStatusFilter("Cancelled", "closed")).toBe(true);
  });

  it("getOrderPaymentLabel", () => {
    expect(getOrderPaymentLabel(order({ paymentAsset: "ShoppingBalance" }))).toBe(
      "购物余额",
    );
  });
});
