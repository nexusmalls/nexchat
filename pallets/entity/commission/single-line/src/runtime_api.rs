//! Runtime API 定义：消费单链详情查询接口
//!
//! 提供以下接口：
//! - `single_line_member_position`: 会员在消费单链中的位置与邻位信息
//! - `single_line_member_view`: 会员消费单链详情（位置 + 汇总 + 分佣历史）
//! - `single_line_overview`: Entity 维度的消费单链总览
//! - `single_line_member_payouts`: 会员消费单链分佣历史
//! - `single_line_preview_commission`: 预览一次消费将触发的单链分佣输出

use codec::{Codec, Decode, Encode};
use pallet_commission_common::CommissionType;
use scale_info::TypeInfo;
use sp_std::vec::Vec;
use Debug;

/// 会员在消费单链中的位置与邻位信息
#[derive(Encode, Decode, Clone, PartialEq, Eq, TypeInfo, Debug)]
pub struct SingleLineMemberPositionInfo<AccountId> {
    pub position: u32,
    pub queue_length: u32,
    pub upline_levels: u8,
    pub downline_levels: u8,
    pub previous_account: Option<AccountId>,
    pub next_account: Option<AccountId>,
}

/// 会员单链分佣记录视图
#[derive(Encode, Decode, Clone, PartialEq, Eq, TypeInfo, Debug)]
pub struct SingleLinePayoutRecordView<AccountId, Balance> {
    pub order_id: u64,
    pub buyer: AccountId,
    pub amount: Balance,
    pub direction: u8,
    pub level_distance: u16,
    pub block_number: u64,
}

/// 会员单链汇总视图
#[derive(Encode, Decode, Clone, PartialEq, Eq, TypeInfo, Debug, Default)]
pub struct SingleLineMemberSummaryView<Balance> {
    pub total_earned_as_upline: Balance,
    pub total_earned_as_downline: Balance,
    pub total_payout_count: u32,
    pub last_payout_block: u64,
}

/// 单链预览输出视图
#[derive(Encode, Decode, Clone, PartialEq, Eq, TypeInfo, Debug)]
pub struct SingleLinePreviewOutput<AccountId, Balance> {
    pub beneficiary: AccountId,
    pub amount: Balance,
    pub commission_type: CommissionType,
    pub level: u16,
}

/// 会员消费单链详情视图
#[derive(Encode, Decode, Clone, PartialEq, Eq, TypeInfo, Debug)]
pub struct SingleLineMemberView<AccountId, Balance> {
    pub position_info: Option<SingleLineMemberPositionInfo<AccountId>>,
    pub is_enabled: bool,
    pub summary: SingleLineMemberSummaryView<Balance>,
    pub recent_payouts: Vec<SingleLinePayoutRecordView<AccountId, Balance>>,
}

/// Entity 维度的消费单链统计视图
#[derive(Encode, Decode, Clone, PartialEq, Eq, TypeInfo, Debug, Default)]
pub struct SingleLineEntityStatsView {
    pub total_orders: u32,
    pub total_upline_payouts: u32,
    pub total_downline_payouts: u32,
}

/// Entity 维度的消费单链总览
#[derive(Encode, Decode, Clone, PartialEq, Eq, TypeInfo, Debug)]
pub struct SingleLineOverview {
    pub is_enabled: bool,
    pub queue_length: u32,
    pub remaining_capacity_in_tail_segment: u32,
    pub segment_count: u32,
    pub stats: SingleLineEntityStatsView,
}

sp_api::decl_runtime_apis! {
    /// 消费单链详情查询 API
    ///
    /// 与 member pallet 的推荐链查询严格区分：这里查询的是按首次消费时间形成的单链公排，而不是 referrer 树。
    pub trait SingleLineQueryApi<AccountId, Balance>
    where
        AccountId: Codec,
        Balance: Codec,
    {
        fn single_line_member_position(
            entity_id: u64,
            account: AccountId,
        ) -> Option<SingleLineMemberPositionInfo<AccountId>>;

        fn single_line_member_view(
            entity_id: u64,
            account: AccountId,
        ) -> Option<SingleLineMemberView<AccountId, Balance>>;

        fn single_line_overview(
            entity_id: u64,
        ) -> SingleLineOverview;

        fn single_line_member_payouts(
            entity_id: u64,
            account: AccountId,
        ) -> Vec<SingleLinePayoutRecordView<AccountId, Balance>>;

        fn single_line_preview_commission(
            entity_id: u64,
            buyer: AccountId,
            order_amount: Balance,
        ) -> Vec<SingleLinePreviewOutput<AccountId, Balance>>;
    }
}
