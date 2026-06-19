// EN: Entity order extrinsics via ChainClient.signAndSend.
// CN: Entity 订单 extrinsic（ChainClient.signAndSend）。

import { config } from "@/config";
import { chainClient } from "@/chain/chainClient";
import type { PaymentAsset } from "@/shop/types";

function ensureLive(): void {
  if (config.useMock) {
    throw new Error("Mock 模式无法提交链上交易，请设置 VITE_USE_MOCK=false");
  }
}

export interface PlaceOrderParams {
  productId: number;
  quantity: number;
  shippingCid: string | null;
  paymentAsset: PaymentAsset | null;
  referrer: string | null;
  maxNexAmount: string | null;
}

// EN: Place entity product order (`entityTransaction.placeOrder`).
// CN: 提交 Entity 商品订单。
export async function placeOrder(params: PlaceOrderParams): Promise<string> {
  ensureLive();
  return chainClient.signAndSend("entityTransaction", "placeOrder", [
    params.productId,
    params.quantity,
    params.shippingCid,
    null, // use_tokens
    params.paymentAsset,
    null, // note_cid
    params.referrer,
    params.maxNexAmount,
    null, // max_token_amount
  ]);
}

// EN: Buyer cancels unpaid order (`entityTransaction.cancelOrder`).
// CN: 买家取消未发货订单。
export async function cancelOrder(orderId: number): Promise<string> {
  ensureLive();
  return chainClient.signAndSend("entityTransaction", "cancelOrder", [orderId]);
}

// EN: Seller ships physical order (`entityTransaction.shipOrder`).
// CN: 卖家发货。
export async function shipOrder(
  orderId: number,
  trackingCid: string,
): Promise<string> {
  ensureLive();
  return chainClient.signAndSend("entityTransaction", "shipOrder", [
    orderId,
    trackingCid,
  ]);
}

// EN: Buyer confirms receipt (`entityTransaction.confirmReceipt`).
// CN: 买家确认收货。
export async function confirmReceipt(orderId: number): Promise<string> {
  ensureLive();
  return chainClient.signAndSend("entityTransaction", "confirmReceipt", [orderId]);
}

// EN: Buyer requests refund after shipment (`entityTransaction.requestRefund`).
// CN: 买家申请退款。
export async function requestRefund(
  orderId: number,
  reasonCid: string,
): Promise<string> {
  ensureLive();
  return chainClient.signAndSend("entityTransaction", "requestRefund", [
    orderId,
    reasonCid,
  ]);
}

// EN: Seller starts service order (`entityTransaction.startService`).
// CN: 卖家开始服务类订单。
export async function startService(orderId: number): Promise<string> {
  ensureLive();
  return chainClient.signAndSend("entityTransaction", "startService", [orderId]);
}

// EN: Seller completes service (`entityTransaction.completeService`).
// CN: 卖家完成服务。
export async function completeService(orderId: number): Promise<string> {
  ensureLive();
  return chainClient.signAndSend("entityTransaction", "completeService", [orderId]);
}

// EN: Buyer confirms service done (`entityTransaction.confirmService`).
// CN: 买家确认服务完成。
export async function confirmService(orderId: number): Promise<string> {
  ensureLive();
  return chainClient.signAndSend("entityTransaction", "confirmService", [orderId]);
}

// EN: Seller approves buyer refund (`entityTransaction.approveRefund`).
// CN: 卖家同意退款。
export async function approveRefund(orderId: number): Promise<string> {
  ensureLive();
  return chainClient.signAndSend("entityTransaction", "approveRefund", [orderId]);
}

// EN: Seller rejects refund with reason CID (`entityTransaction.rejectRefund`).
// CN: 卖家拒绝退款。
export async function rejectRefund(
  orderId: number,
  reasonCid: string,
): Promise<string> {
  ensureLive();
  return chainClient.signAndSend("entityTransaction", "rejectRefund", [
    orderId,
    reasonCid,
  ]);
}

// EN: Buyer withdraws dispute (`entityTransaction.withdrawDispute`).
// CN: 买家撤回争议。
export async function withdrawDispute(orderId: number): Promise<string> {
  ensureLive();
  return chainClient.signAndSend("entityTransaction", "withdrawDispute", [orderId]);
}

// EN: Seller cancels paid order (`entityTransaction.sellerCancelOrder`).
// CN: 卖家取消已付款订单。
export async function sellerCancelOrder(
  orderId: number,
  reasonCid: string,
): Promise<string> {
  ensureLive();
  return chainClient.signAndSend("entityTransaction", "sellerCancelOrder", [
    orderId,
    reasonCid,
  ]);
}
