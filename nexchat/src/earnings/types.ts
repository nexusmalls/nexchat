// EN: Commission / earnings types for NexChat Me tab (mirrors nexus-com-dapp).
// CN: NexChat「我」Tab 佣金收益类型（对齐 nexus-com-dapp）。

export interface CommissionMemberStats {
  totalEarned: string;
  pending: string;
  withdrawn: string;
  repurchased: string;
  orderCount: number;
}

export interface CommissionOverview {
  enabledModes: number;
  commissionRate: number;
  isEnabled: boolean;
  multiLevelPaused: boolean;
  singleLineEnabled: boolean;
  teamStatus: [boolean, boolean];
  poolRewardPaused: boolean;
  withdrawalPaused: boolean;
  unallocatedPoolNex: string;
}

export interface CommissionDashboard {
  nexStats: CommissionMemberStats;
  multiLevelStats: { totalEarned: string; totalOrders: number } | null;
  teamTier: { name: string; tierIndex: number; rate: number; totalEarned: string } | null;
  singleLine: { position: number | null; isEnabled: boolean };
  poolReward: {
    claimableNex: string;
    currentRoundId: number;
    isPaused: boolean;
  };
  referral: { totalEarned: string } | null;
  multiLevelProgress: Array<{ level: number; activated: boolean }>;
}

export interface WithdrawalRecord {
  totalAmount: string;
  withdrawn: string;
  repurchased: string;
  bonus: string;
  blockNumber: number;
}

export interface RepurchaseConfig {
  maxShoppingBalanceUsdt: string;
}

export interface EarningEntityOption {
  entityId: number;
  name: string;
  /** EN: Pending NEX for default picker sort. CN: 待提取 NEX（用于默认选中排序）。 */
  pending?: string;
}

/** EN: Active entity from `entityRegistry` (join / switch UI). CN: registry 活跃 Entity（加入/切换 UI）。 */
export interface RegistryEntity {
  id: number;
  name: string;
  primaryShopId: number;
  verified: boolean;
  entityType: string;
  status?: string;
}

export interface MultiLevelPayoutRecord {
  buyer: string;
  orderId: number;
  amount: string;
  level: number;
  blockNumber: number;
  /** EN: Tier rate in basis points (10000 = 100%). CN: 层级费率（基点，10000 = 100%）。 */
  rateBps: number | null;
}

export interface MultiLevelMemberStats {
  totalEarned: string;
  totalOrders: number;
  lastCommissionBlock: number;
}

export type SingleLineDirection = "upline" | "downline";

export interface SingleLinePayoutRecord {
  orderId: number;
  buyer: string;
  amount: string;
  direction: SingleLineDirection;
  levelDistance: number;
  blockNumber: number;
  shopId: number | null;
}

export interface SingleLineMemberStats {
  totalEarnedAsUpline: string;
  totalEarnedAsDownline: string;
  totalPayoutCount: number;
  lastPayoutBlock: number;
}

export interface PoolRewardLevelProgress {
  levelId: number;
  ratioBps: number;
  memberCount: number;
  claimedCount: number;
  perMemberReward: string;
}

export interface PoolRewardClaimRecord {
  roundId: number;
  amount: string;
  tokenAmount: string;
  levelId: number;
  claimedAt: number;
}

export interface PoolRewardCapInfo {
  cumulativeClaimedUsdt: string;
  currentCapUsdt: string;
  remainingCapUsdt: string;
  isCapped: boolean;
  rateSnapshotUsed: number | null;
  unlockPercent: number | null;
}

export interface PoolRewardMemberView {
  roundDuration: number;
  tokenPoolEnabled: boolean;
  currentRoundId: number;
  roundStartBlock: number;
  roundEndBlock: number;
  poolSnapshot: string;
  tokenPoolSnapshot: string | null;
  effectiveLevel: number;
  claimableNex: string;
  claimableToken: string;
  alreadyClaimed: boolean;
  roundExpired: boolean;
  lastClaimedRound: number;
  capInfo: PoolRewardCapInfo;
  levelProgress: PoolRewardLevelProgress[];
  claimHistory: PoolRewardClaimRecord[];
  isPaused: boolean;
  hasPendingConfig: boolean;
}

export interface PoolRewardRoundFunding {
  nexCommissionRemainder: string;
  tokenPlatformFeeRetention: string;
  tokenCommissionRemainder: string;
  nexCancelReturn: string;
  totalFundingCount: number;
}

export interface EarningsPluginCard {
  key: string;
  label: string;
  icon: string;
  status: "enabled" | "paused";
  description: string;
  stat?: string;
  stat2?: string;
  /** EN: NEXCOM sub-page path suffix (e.g. `/earnings/multi-level`). CN: NEXCOM 子页路径后缀。 */
  href?: string;
}
