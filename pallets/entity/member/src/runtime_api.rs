//! Runtime API 定义：用于前端查询会员仪表盘及推荐团队信息
//!
//! 提供以下接口：
//! - `get_member_info`: 会员仪表盘数据（等级/消费/推荐/过期/升级历史）
//! - `get_referral_team`: 批量返回直推列表 + 各下线的等级/消费概览（支持 1-2 层深度）
//! - `get_upline_chain`: 上行链路，从指定会员向上追溯到根节点的完整推荐路径
//! - `get_referral_tree`: 深层下行树，递归展开推荐子树（支持 1-10 层深度）
//! - `get_referrals_by_generation`: 按代分页查询，获取指定会员第 N 代下级

use codec::{Codec, Decode, Encode};
use scale_info::TypeInfo;
use sp_std::vec::Vec;
use Debug;

/// 升级记录（Runtime API 返回用，区块号统一为 u64）
#[derive(Encode, Decode, Clone, PartialEq, Eq, TypeInfo, Debug)]
pub struct UpgradeRecordInfo {
    /// 触发的规则 ID
    pub rule_id: u32,
    /// 升级前等级
    pub from_level_id: u8,
    /// 升级后等级
    pub to_level_id: u8,
    /// 升级时间（区块号）
    pub upgraded_at: u64,
    /// 过期时间（区块号，None = 永久）
    pub expires_at: Option<u64>,
}

/// 会员仪表盘信息（聚合查询）
#[derive(Encode, Decode, Clone, PartialEq, Eq, TypeInfo, Debug)]
pub struct MemberDashboardInfo<AccountId> {
    /// 推荐人
    pub referrer: Option<AccountId>,
    /// 存储中的等级 ID（raw，不考虑过期）
    pub custom_level_id: u8,
    /// 有效等级 ID（考虑过期后的实际等级）
    pub effective_level_id: u8,
    /// 累计消费（USDT 精度 10^6）
    pub total_spent: u64,
    /// 直接推荐人数
    pub direct_referrals: u32,
    /// 间接推荐人数
    pub indirect_referrals: u32,
    /// 团队总人数
    pub team_size: u32,
    /// 订单数量
    pub order_count: u32,
    /// 加入时间（区块号）
    pub joined_at: u64,
    /// 最后活跃时间（区块号）
    pub last_active_at: u64,
    /// 是否已激活（首次合格消费后）
    pub activated: bool,
    /// 是否被封禁
    pub is_banned: bool,
    /// 封禁时间（区块号，None = 未封禁）
    pub banned_at: Option<u64>,
    /// 封禁原因
    pub ban_reason: Option<Vec<u8>>,
    /// 等级过期时间（区块号，None = 永久或无规则升级）
    pub level_expires_at: Option<u64>,
    /// 升级历史记录
    pub upgrade_history: Vec<UpgradeRecordInfo>,
}

/// 团队成员概览信息
#[derive(Encode, Decode, Clone, PartialEq, Eq, TypeInfo, Debug)]
pub struct TeamMemberInfo<AccountId> {
    /// 会员账户
    pub account: AccountId,
    /// 自定义等级 ID（有效等级，已考虑过期）
    pub level_id: u8,
    /// 累计消费（USDT 精度 10^6）
    pub total_spent: u64,
    /// 直推人数
    pub direct_referrals: u32,
    /// 团队总人数
    pub team_size: u32,
    /// 加入时间（区块号）
    pub joined_at: u64,
    /// 最后活跃时间（区块号）
    pub last_active_at: u64,
    /// 是否被封禁
    pub is_banned: bool,
    /// 下级列表（depth > 1 时递归填充，否则为空）
    pub children: Vec<TeamMemberInfo<AccountId>>,
}

/// O1: 分页会员列表项
#[derive(Encode, Decode, Clone, PartialEq, Eq, TypeInfo, Debug)]
pub struct PaginatedMemberInfo<AccountId> {
    /// 会员账户
    pub account: AccountId,
    /// 有效等级 ID
    pub level_id: u8,
    /// 累计消费（USDT 精度 10^6）
    pub total_spent: u64,
    /// 直推人数
    pub direct_referrals: u32,
    /// 团队总人数
    pub team_size: u32,
    /// 加入时间（区块号）
    pub joined_at: u64,
    /// 是否被封禁
    pub is_banned: bool,
    /// A2: 封禁原因
    pub ban_reason: Option<Vec<u8>>,
}

/// O1: 分页查询结果
#[derive(Encode, Decode, Clone, PartialEq, Eq, TypeInfo, Debug)]
pub struct PaginatedMembersResult<AccountId> {
    /// 会员列表
    pub members: Vec<PaginatedMemberInfo<AccountId>>,
    /// 总会员数
    pub total: u32,
    /// 是否还有更多数据
    pub has_more: bool,
}

/// Entity 会员总览信息（Owner 视角）
#[derive(Encode, Decode, Clone, PartialEq, Eq, TypeInfo, Debug)]
pub struct EntityMemberOverview {
    /// 会员总数
    pub total_members: u32,
    /// 各等级会员分布 (level_id, count)
    pub level_distribution: Vec<(u8, u32)>,
    /// 待审批会员数量
    pub pending_count: u32,
    /// 被封禁会员数量
    pub banned_count: u32,
}

// ============================================================================
// 推荐链路可视化 DTO
// ============================================================================

/// 上行链路节点
#[derive(Encode, Decode, Clone, PartialEq, Eq, TypeInfo, Debug)]
pub struct UplineNode<AccountId> {
    /// 会员账户
    pub account: AccountId,
    /// 有效等级 ID
    pub level_id: u8,
    /// 团队总人数
    pub team_size: u32,
    /// 加入时间（区块号）
    pub joined_at: u64,
}

/// 上行链路查询结果
#[derive(Encode, Decode, Clone, PartialEq, Eq, TypeInfo, Debug)]
pub struct UplineChainResult<AccountId> {
    /// 路径: [account, referrer, grand_referrer, ..., root]
    pub chain: Vec<UplineNode<AccountId>>,
    /// 是否因 max_depth 截断（true = 链路可能更长）
    pub truncated: bool,
    /// 链路总深度（= chain.len()）
    pub depth: u32,
}

/// 下行推荐树节点（递归结构）
#[derive(Encode, Decode, Clone, PartialEq, Eq, TypeInfo, Debug)]
pub struct ReferralTreeNode<AccountId> {
    /// 会员账户
    pub account: AccountId,
    /// 有效等级 ID
    pub level_id: u8,
    /// 直推人数
    pub direct_referrals: u32,
    /// 团队总人数
    pub team_size: u32,
    /// 累计消费（USDT 精度 10^6）
    pub total_spent: u64,
    /// 加入时间（区块号）
    pub joined_at: u64,
    /// 是否被封禁
    pub is_banned: bool,
    /// 子节点（depth > 0 时递归展开）
    pub children: Vec<ReferralTreeNode<AccountId>>,
    /// 是否还有未展开的子节点（子节点数超过展开上限或深度已耗尽）
    pub has_more_children: bool,
}

/// 按代查询的会员信息
#[derive(Encode, Decode, Clone, PartialEq, Eq, TypeInfo, Debug)]
pub struct GenerationMemberInfo<AccountId> {
    /// 会员账户
    pub account: AccountId,
    /// 有效等级 ID
    pub level_id: u8,
    /// 直推人数
    pub direct_referrals: u32,
    /// 团队总人数
    pub team_size: u32,
    /// 累计消费（USDT 精度 10^6）
    pub total_spent: u64,
    /// 加入时间（区块号）
    pub joined_at: u64,
    /// 是否被封禁
    pub is_banned: bool,
    /// 该成员的直接推荐人（上一代中的某位）
    pub referrer: AccountId,
}

/// 按代分页查询结果
#[derive(Encode, Decode, Clone, PartialEq, Eq, TypeInfo, Debug)]
pub struct PaginatedGenerationResult<AccountId> {
    /// 查询的代数
    pub generation: u32,
    /// 该代会员列表
    pub members: Vec<GenerationMemberInfo<AccountId>>,
    /// 该代总会员数
    pub total_count: u32,
    /// 每页数量
    pub page_size: u32,
    /// 当前页码（0-based）
    pub page_index: u32,
    /// 是否还有更多数据
    pub has_more: bool,
}

sp_api::decl_runtime_apis! {
    /// 会员 Runtime API
    ///
    /// 提供会员仪表盘查询、推荐团队树查询、Entity 总览查询
    pub trait MemberTeamApi<AccountId>
    where
        AccountId: Codec,
    {
        /// 获取会员仪表盘信息（聚合查询）
        ///
        /// ### 参数
        /// - `entity_id`: 实体 ID
        /// - `account`: 查询的会员账户
        ///
        /// ### 返回
        /// - `Some(MemberDashboardInfo)` — 会员存在时返回聚合信息
        /// - `None` — 会员不存在
        fn get_member_info(entity_id: u64, account: AccountId) -> Option<MemberDashboardInfo<AccountId>>;

        /// 获取指定会员的推荐团队树
        ///
        /// ### 参数
        /// - `entity_id`: 实体 ID
        /// - `account`: 查询的会员账户
        /// - `depth`: 查询深度（1 = 仅直推，2 = 直推 + 二级）
        ///
        /// ### 返回
        /// - 直推成员列表，每个成员包含等级/消费概览及可选的下级列表
        fn get_referral_team(entity_id: u64, account: AccountId, depth: u32) -> Vec<TeamMemberInfo<AccountId>>;

        /// 获取 Entity 会员总览（Owner 视角）
        ///
        /// ### 参数
        /// - `entity_id`: 实体 ID
        ///
        /// ### 返回
        /// - 会员总数、等级分布、待审批数、封禁数
        fn get_entity_member_overview(entity_id: u64) -> EntityMemberOverview;

        /// O1: 分页查询实体会员列表
        ///
        /// ### 参数
        /// - `entity_id`: 实体 ID
        /// - `page_size`: 每页数量（上限 100）
        /// - `page_index`: 页码（0-based）
        ///
        /// ### 返回
        /// - 分页会员列表、总数、是否有更多
        fn get_members_paginated(entity_id: u64, page_size: u32, page_index: u32) -> PaginatedMembersResult<AccountId>;

        /// 获取上行推荐链路（从指定会员向上追溯到根节点）
        ///
        /// ### 参数
        /// - `entity_id`: 实体 ID
        /// - `account`: 起始会员账户
        /// - `max_depth`: 最大追溯深度（上限 100）
        ///
        /// ### 返回
        /// - 完整上行路径 [account → referrer → ... → root]，是否截断，总深度
        fn get_upline_chain(entity_id: u64, account: AccountId, max_depth: u32) -> UplineChainResult<AccountId>;

        /// 获取深层下行推荐树（递归展开子树）
        ///
        /// ### 参数
        /// - `entity_id`: 实体 ID
        /// - `account`: 根节点会员账户
        /// - `depth`: 展开深度（1~10，超出范围自动 clamp）
        ///
        /// ### 返回
        /// - 以 account 为根的推荐树，含递归子节点
        fn get_referral_tree(entity_id: u64, account: AccountId, depth: u32) -> ReferralTreeNode<AccountId>;

        /// 按代分页查询（获取指定会员第 N 代下级）
        ///
        /// ### 参数
        /// - `entity_id`: 实体 ID
        /// - `account`: 起始会员账户
        /// - `generation`: 代数（1 = 直推，2 = 间推，...，上限 20）
        /// - `page_size`: 每页数量（上限 100）
        /// - `page_index`: 页码（0-based）
        ///
        /// ### 返回
        /// - 该代的分页会员列表
        fn get_referrals_by_generation(entity_id: u64, account: AccountId, generation: u32, page_size: u32, page_index: u32) -> PaginatedGenerationResult<AccountId>;
    }
}
