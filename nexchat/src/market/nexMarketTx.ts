// EN: NEX market extrinsics via ChainClient.signAndSend.
// CN: 经 ChainClient.signAndSend 提交 NEX 市场 extrinsic。

import { config } from "@/config";
import { chainClient } from "@/chain/chainClient";

function ensureLive(): void {
  if (config.useMock) {
    throw new Error("Mock 模式无法提交链上交易，请设置 VITE_USE_MOCK=false");
  }
}

// EN: Place buy limit order (buy NEX with USDT off-chain).
// CN: 挂买单（链下 USDT 买 NEX）。
export async function placeBuyOrder(
  nexAmountRaw: string,
  usdtPriceRaw: string,
  buyerTron: string,
): Promise<string> {
  ensureLive();
  return chainClient.signAndSend("nexMarket", "placeBuyOrder", [
    nexAmountRaw,
    usdtPriceRaw,
    buyerTron.trim(),
  ]);
}

// EN: Place sell limit order (lock NEX, receive USDT off-chain).
// CN: 挂卖单（锁定 NEX，链下收 USDT）。
export async function placeSellOrder(
  nexAmountRaw: string,
  usdtPriceRaw: string,
  sellerTron: string,
  minFillRaw: string | null,
): Promise<string> {
  ensureLive();
  return chainClient.signAndSend("nexMarket", "placeSellOrder", [
    nexAmountRaw,
    usdtPriceRaw,
    sellerTron.trim(),
    minFillRaw,
  ]);
}

// EN: Cancel own open order.
// CN: 取消自己的挂单。
export async function cancelOrder(orderId: number): Promise<string> {
  ensureLive();
  return chainClient.signAndSend("nexMarket", "cancelOrder", [orderId]);
}

// EN: Seller takes a buy order (accept buy / sell NEX).
// CN: 卖家吃买单（accept buy / 卖 NEX）。
export async function acceptBuyOrder(
  orderId: number,
  amountRaw: string | null,
  sellerTron: string,
): Promise<string> {
  ensureLive();
  return chainClient.signAndSend("nexMarket", "acceptBuyOrder", [
    orderId,
    amountRaw,
    sellerTron.trim(),
  ]);
}

// EN: Buyer takes a sell order (reserve sell / buy NEX).
// CN: 买家吃卖单（reserve sell / 买 NEX）。
export async function reserveSellOrder(
  orderId: number,
  amountRaw: string | null,
  buyerTron: string,
): Promise<string> {
  ensureLive();
  return chainClient.signAndSend("nexMarket", "reserveSellOrder", [
    orderId,
    amountRaw,
    buyerTron.trim(),
  ]);
}

// EN: Buyer confirms USDT payment sent (triggers OCW verification).
// CN: 买家确认已付款（触发 OCW 验证）。
export async function confirmPayment(tradeId: number): Promise<string> {
  ensureLive();
  return chainClient.signAndSend("nexMarket", "confirmPayment", [tradeId]);
}

// EN: Seller manually confirms USDT received (fallback when OCW lags).
// CN: 卖家手动确认已收款（OCW 延迟时的备用结算）。
export async function sellerConfirmReceived(tradeId: number): Promise<string> {
  ensureLive();
  return chainClient.signAndSend("nexMarket", "sellerConfirmReceived", [tradeId]);
}

// EN: Either party processes an expired trade timeout.
// CN: 任一方处理已过期交易超时。
export async function processTimeout(tradeId: number): Promise<string> {
  ensureLive();
  return chainClient.signAndSend("nexMarket", "processTimeout", [tradeId]);
}
