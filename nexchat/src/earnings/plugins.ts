// EN: Build commission plugin summary cards from overview + dashboard.
// CN: 根据总览与仪表盘构建佣金插件卡片（文案对齐 nexus-com-dapp earnings）。

import { COMMISSION_MODES } from "@/earnings/commissionModes";
import type {
  CommissionDashboard,
  CommissionOverview,
  EarningsPluginCard,
} from "@/earnings/types";
import { formatBalance } from "@/market/format";

function bpsToPercent(bps: number): string {
  return `${(bps / 100).toFixed(1)}%`;
}

// EN: Plugin cards for enabled commission modes (aligned with nexus-com-dapp /earnings).
// CN: 已启用佣金模式的插件卡片（对齐 nexus-com-dapp /earnings）。
export function buildEarningsPlugins(
  overview: CommissionOverview | null,
  dashboard: CommissionDashboard | null,
): EarningsPluginCard[] {
  if (!overview?.isEnabled) return [];

  const modes = overview.enabledModes;
  const plugins: EarningsPluginCard[] = [];

  const referralModes =
    modes &
    (COMMISSION_MODES.DIRECT_REWARD |
      COMMISSION_MODES.FIXED_AMOUNT |
      COMMISSION_MODES.FIRST_ORDER |
      COMMISSION_MODES.REPEAT_PURCHASE);
  if (referralModes > 0) {
    plugins.push({
      key: "referral",
      label: "直推奖",
      icon: "👥",
      status: "enabled",
      description: "直接推荐人奖励",
      stat:
        dashboard?.referral && BigInt(dashboard.referral.totalEarned) > 0n
          ? `${formatBalance(dashboard.referral.totalEarned)} NEX`
          : undefined,
    });
  }

  if ((modes & COMMISSION_MODES.MULTI_LEVEL) > 0) {
    const activated =
      dashboard?.multiLevelProgress?.filter((p) => p.activated).length ?? 0;
    plugins.push({
      key: "multiLevel",
      label: "助力奖励",
      icon: "📊",
      href: "/earnings/multi-level",
      status: overview.multiLevelPaused ? "paused" : "enabled",
      description: overview.multiLevelPaused
        ? "已暂停"
        : activated > 0
          ? `已激活 ${activated} 级`
          : "多级层级佣金",
      stat: dashboard?.multiLevelStats
        ? `${formatBalance(dashboard.multiLevelStats.totalEarned)} NEX`
        : undefined,
    });
  }

  if ((modes & COMMISSION_MODES.TEAM_PERFORMANCE) > 0) {
    const teamConfigured = overview.teamStatus[0] || overview.teamStatus[1];
    plugins.push({
      key: "team",
      label: "团队业绩",
      icon: "🏆",
      status: teamConfigured ? "enabled" : "paused",
      description: dashboard?.teamTier
        ? `当前档位 ${dashboard.teamTier.name || `T${dashboard.teamTier.tierIndex}`} (${bpsToPercent(dashboard.teamTier.rate)})`
        : "按团队业绩阶梯返利",
      stat: dashboard?.teamTier
        ? `${formatBalance(dashboard.teamTier.totalEarned)} NEX`
        : undefined,
    });
  }

  if ((modes & COMMISSION_MODES.LEVEL_DIFF) > 0) {
    plugins.push({
      key: "levelDiff",
      label: "级差奖励",
      icon: "📈",
      status: "enabled",
      description: "上下级等级差额佣金",
    });
  }

  const slUpline = (modes & COMMISSION_MODES.SINGLE_LINE_UPLINE) > 0;
  const slDownline = (modes & COMMISSION_MODES.SINGLE_LINE_DOWNLINE) > 0;
  if (slUpline || slDownline) {
    plugins.push({
      key: "singleLine",
      label: "共赢奖励",
      icon: "↕️",
      href: "/earnings/single-line",
      status: overview.singleLineEnabled ? "enabled" : "paused",
      description:
        dashboard?.singleLine?.position != null
          ? `队列位置 #${dashboard.singleLine.position}`
          : "排位上下线分佣",
    });
  }

  if ((modes & COMMISSION_MODES.POOL_REWARD) > 0) {
    plugins.push({
      key: "poolReward",
      label: "奖池领取",
      icon: "🏆",
      href: "/earnings/pool-reward",
      status: overview.poolRewardPaused ? "paused" : "enabled",
      description: dashboard?.poolReward?.currentRoundId
        ? `当前轮次 #${dashboard.poolReward.currentRoundId}`
        : "等级奖池领取",
      stat: dashboard?.poolReward
        ? `可领取: ${formatBalance(dashboard.poolReward.claimableNex ?? "0")} NEX`
        : undefined,
      stat2: `沉淀池: ${formatBalance(overview.unallocatedPoolNex)} NEX`,
    });
  }

  return plugins;
}
