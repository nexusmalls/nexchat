// EN: Read-only NEX global market types for NexChat discover view.
// CN: NexChat 发现页只读 NEX 全局市场类型。

export interface NexMarketOrder {
  id: number;
  side: "Buy" | "Sell";
  price: string;
  amount: string;
  filled: string;
  deposit: string;
  depositWaived: boolean;
  createdAt: number;
}

export interface NexPriceProtection {
  enabled: boolean;
  maxPriceDeviation: number;
  circuitBreakerActive: boolean;
  initialPrice: string | null;
}

export interface OrderActionPrefill {
  target: "takeBuy" | "takeSell";
  orderId: string;
  amount: string;
  tron: string;
}

export interface NexMarketStats {
  lastPrice: string;
  totalOrders: number;
  totalTrades: number;
  totalVolumeUsdt: string;
  referencePrice: string | null;
  referenceSource: "twap" | "initial" | null;
}

export interface NexDepthLevel {
  price: string;
  totalAmount: bigint;
  orderCount: number;
  cumulative: bigint;
  hasSeedOrder: boolean;
}

export type NexTradeStatus =
  | "AwaitingPayment"
  | "AwaitingVerification"
  | "UnderpaidPending"
  | "Completed"
  | "Refunded"
  | "Cancelled"
  | "Disputed";

export interface NexMarketTrade {
  tradeId: number;
  orderId: number;
  buyer: string;
  seller: string;
  nexAmount: string;
  usdtAmount: string;
  sellerTronAddress: string;
  buyerTronAddress: string;
  status: NexTradeStatus;
  paymentConfirmed: boolean;
  createdAt: number;
  buyerDeposit: string;
}

export interface MarketSnapshot {
  stats: NexMarketStats;
  protection: NexPriceProtection;
  buyOrders: NexMarketOrder[];
  sellOrders: NexMarketOrder[];
  asks: NexDepthLevel[];
  bids: NexDepthLevel[];
  maxDepth: bigint;
}
