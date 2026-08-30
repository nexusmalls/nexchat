# pallet-entity-commission

> Entity 返佣管理模块 — 插件化架构，支持 NEX + Entity Token 双资产全管线返佣
>
> 首版主网（ML + SL + 沉淀池）参数模板与上链顺序见 [`docs/COMMISSION_ML_SL_POOL_MAINNET_TEMPLATE.md`](../../../docs/COMMISSION_ML_SL_POOL_MAINNET_TEMPLATE.md)。插件 README 中 `level_ratios` / ML `max_total_rate` 已过时，以该规格 + 源码为准。

## 概述

`pallet-entity-commission` 是 Entity 商城系统的返佣管理模块，采用**插件化架构**，由 1 个核心调度引擎 + 5 个返佣插件 + 1 个沉淀池插件 + 1 个共享类型库组成。

每个 Entity 可同时启用多种返佣模式（位标志多选），佣金按固定顺序叠加计算。NEX 和 Entity Token 两条完全对称的调度管线共用同一套配置（比率基点制），**双资产独立记账、独立提现**。

## 模块结构

```
pallet-entity-commission/          <-- re-export wrapper
+-- common/                        <-- 共享类型 + trait 定义
+-- core/                          <-- 调度引擎 + 记账 + 提现 + 偿付安全
|   +-- src/
|       +-- lib.rs                 <-- Config + Storage + Extrinsics + Trait 实现
|       +-- engine.rs              <-- 佣金计算引擎（调度 + 记账 + 取消 + 结算）
|       +-- settlement.rs          <-- 购物余额结算（NEX 委托 Loyalty / Token 直接管理）
|       +-- withdraw.rs            <-- 提现分配计算 + 权限校验 + Token sweep
+-- referral/                      <-- 推荐链返佣（直推/固定金额/首单/复购）
+-- multi-level/                   <-- 多级分销（N 层 + 三维激活条件）
+-- level-diff/                    <-- 等级极差返佣（自定义等级体系）
+-- single-line/                   <-- 单线收益（上线/下线，分段存储）
+-- pool-reward/                   <-- 沉淀池奖励（周期性等额分配）
```

| 子模块 | Crate | 说明 |
|--------|-------|------|
| common | `pallet-commission-common` | 共享类型、枚举、trait（`CommissionPlugin` / `TokenCommissionPlugin` / `MemberProvider` / PlanWriter 等） |
| core | `pallet-commission-core` | 核心调度引擎：配置管理、`process_commission` / `process_token_commission` 分发、提现系统（NEX + Token 独立）、偿付安全、创建人收益。代码拆分为 engine.rs（调度+记账）、settlement.rs（购物余额）、withdraw.rs（提现计算） |
| referral | `pallet-commission-referral` | 推荐链返佣：直推(DirectReward)、固定金额(FixedAmount)、首单(FirstOrder)、复购(RepeatPurchase)；附带推荐人激活条件、返佣上限、推荐关系有效期 |
| multi-level | `pallet-commission-multi-level` | 多级分销：N 层推荐链遍历 + 三维激活条件（直推人数/团队规模/USDT 消费）+ 总佣金上限 + 暂停/恢复 + 延迟生效配置 + 审计日志 |
| level-diff | `pallet-commission-level-diff` | 等级极差返佣：基于 Entity 自定义等级体系，沿推荐链按等级差价分配 |
| single-line | `pallet-commission-single-line` | 单线收益：基于全局消费注册顺序的上/下线佣金，分段存储（`BoundedVec` 段满自动扩展），层数随消费额动态增长，支持按会员等级自定义层数 |
| pool-reward | `pallet-commission-pool-reward` | 沉淀池奖励 v2：周期性等额分配（Periodic Equal-Share Claim），NEX + Token 双池统一入口，轮次快照 + 历史归档 + 分配统计 |

> `pallet-commission-team`（团队业绩阶梯奖金）作为独立 crate 存在，通过 runtime 层配置 `TeamPlugin` / `TeamWriter` 接入核心引擎，不在本 umbrella crate 内 re-export。

## 核心引擎代码拆分

commission-core 的核心逻辑按职责拆分为三个独立文件：

| 文件 | 职责 | 包含函数 |
|------|------|---------|
| `engine.rs` | 佣金计算引擎 | `process_commission`、`credit_commission`、`process_token_commission`、`credit_token_commission`、`cancel_commission`、`do_cancel_token_commission`、`do_settle_order_records` |
| `settlement.rs` | 购物余额结算 | `do_use_shopping_balance`（委托 Loyalty）、`do_consume_shopping_balance`（委托 Loyalty）、`do_consume_token_shopping_balance`（直接管理） |
| `withdraw.rs` | 提现分配 + 辅助 | `calc_withdrawal_split`、`calc_token_withdrawal_split`、`is_pool_reward_locked`、`ensure_owner_or_admin`、`ensure_entity_owner`、`sweep_token_free_balance` |

### Loyalty 集成

NEX 购物余额存储（`MemberShoppingBalance`、`ShopShoppingTotal`）已从 commission-core 迁移至独立的 **Loyalty 模块**。commission-core 通过 Config 关联类型 `type Loyalty: LoyaltyWritePort<AccountId, Balance>` 委托所有 NEX 购物余额的读写操作：

| 操作 | 委托方式 |
|------|---------|
| 查询购物余额 | `T::Loyalty::shopping_balance(entity_id, account)` |
| 查询余额总量 | `T::Loyalty::shopping_total(entity_id)` |
| 增加购物余额（复购） | `T::Loyalty::credit_shopping_balance(entity_id, account, amount)` |
| 消费购物余额 | `T::Loyalty::consume_shopping_balance(entity_id, account, amount)` |

Token 购物余额（`MemberTokenShoppingBalance`、`TokenShoppingTotal`）仍由 commission-core 本地存储直接管理。

## 返佣模式（位标志多选）

| 模式 | 位标志 | 插件 | 说明 |
|------|--------|------|------|
| `DIRECT_REWARD` | `0x001` | referral | 直推奖励（按订单金额比例，发给直接推荐人） |
| `MULTI_LEVEL` | `0x002` | multi-level | 多级分销（N 层推荐链 + 三维激活条件） |
| `TEAM_PERFORMANCE` | `0x004` | team | 团队业绩阶梯奖金 |
| `LEVEL_DIFF` | `0x008` | level-diff | 等级极差（自定义等级差价） |
| `FIXED_AMOUNT` | `0x010` | referral | 固定金额（每单给推荐人固定数额） |
| `FIRST_ORDER` | `0x020` | referral | 首单奖励（买家首次下单的推荐人奖励） |
| `REPEAT_PURCHASE` | `0x040` | referral | 复购奖励（买家达到最低订单数后的推荐人奖励） |
| `SINGLE_LINE_UPLINE` | `0x080` | single-line | 单线上线收益 |
| `SINGLE_LINE_DOWNLINE` | `0x100` | single-line | 单线下线收益 |
| `POOL_REWARD` | `0x200` | pool-reward | 沉淀池奖励（未分配佣金周期性分配） |
| `OWNER_REWARD` | `0x400` | core 内置 | 创建人收益（从佣金预算中优先扣除） |
| `ENTITY_REFERRAL` | -- | core 内置 | 招商推荐人奖金（从平台费中扣除） |

## 双资产佣金架构

每笔订单产生两个独立的佣金资金池，NEX 和 Token 各走一条完整管线：

```
订单完成
|
+-- NEX 管线 (process_commission)            [engine.rs]
|   +-- 池 A: 平台费 (platform_fee)
|   |   +-- 招商推荐人 -> platform_fee x ReferrerShareBps
|   |   +-- 国库 -> 剩余部分
|   |
|   +-- 池 B: available_pool (= 卖家货款 x max_commission_rate)
|       +-- 创建人收益 -> available_pool x owner_reward_rate (OWNER_REWARD 启用时)
|       +-- Referral 插件 -> remaining
|       +-- MultiLevel 插件 -> remaining
|       +-- LevelDiff 插件 -> remaining
|       +-- SingleLine 插件 -> remaining
|       +-- Team 插件 -> remaining
|       +-- 沉淀池 <- remaining（POOL_REWARD 启用时）
|
+-- Token 管线 (process_token_commission)    [engine.rs]
|   +-- 对称结构，使用 TokenCommissionPlugin trait
|       Token 版跳过固定金额模式（金额以 NEX 计价）
|
+-- 提现 (withdraw_commission)               [lib.rs + withdraw.rs]
|   +-- NEX 复购 -> T::Loyalty::credit_shopping_balance (Loyalty 模块)
|   +-- Token 复购 -> MemberTokenShoppingBalance (commission-core 本地)
|
+-- 购物余额消费                              [settlement.rs]
    +-- NEX -> 委托 T::Loyalty::consume_shopping_balance
    +-- Token -> do_consume_token_shopping_balance (直接管理)
```

## 核心引擎 (commission-core)

### 配置 (`CoreCommissionConfig`)

| 字段 | 类型 | 说明 |
|------|------|------|
| `enabled_modes` | `CommissionModes` | 启用的返佣模式位标志 |
| `max_commission_rate` | `u16` | 会员返佣上限比例（基点，从卖家货款扣除） |
| `enabled` | `bool` | 全局启用开关 |
| `withdrawal_cooldown` | `u32` | NEX 提现冻结期（区块数） |
| `owner_reward_rate` | `u16` | 创建人收益比例（基点，从 Pool B 优先扣除） |
| `token_withdrawal_cooldown` | `u32` | Token 提现冻结期（区块数，0 = 使用 NEX 冻结期） |

### 插件调度

Core 通过 Config 中的关联类型引用各插件，NEX 和 Token 各有独立的插件管线：

| NEX 插件 | Token 插件 | 实现 trait |
|----------|-----------|-----------|
| `ReferralPlugin` | `TokenReferralPlugin` | `CommissionPlugin` / `TokenCommissionPlugin` |
| `MultiLevelPlugin` | `TokenMultiLevelPlugin` | 同上 |
| `LevelDiffPlugin` | `TokenLevelDiffPlugin` | 同上 |
| `SingleLinePlugin` | `TokenSingleLinePlugin` | 同上 |
| `TeamPlugin` | `TokenTeamPlugin` | 同上 |

### 提现系统

四种提现模式（NEX 和 Token 独立配置）：

| 模式 | 说明 |
|------|------|
| `FullWithdrawal` | 不强制复购（Governance 底线仍生效） |
| `FixedRate` | 所有会员统一复购比率 |
| `LevelBased` | 按会员等级查 `default_tier` / `level_overrides` |
| `MemberChoice` | 会员自选比率，不低于 `min_repurchase_rate` |

三层约束叠加：`Governance 底线 >= Entity 配置 >= 会员选择`

自愿多复购可获得 `voluntary_bonus_rate` 加成，超出强制最低线的部分按奖励比例额外计入购物余额。

购物余额仅可用于订单抵扣（通过 Loyalty 模块消费），不可直接提取为 NEX。

## 推荐链返佣 (commission-referral)

4 种子模式 + 3 项增强配置：

| 子模式 | 配置 | 计算方式 |
|--------|------|----------|
| DirectReward | `rate: u16` | `order_amount x rate / 10000` |
| FixedAmount | `amount: Balance` | 固定金额（Token 版跳过） |
| FirstOrder | `amount / rate / use_amount` | 买家首次下单时，按金额或比例发放 |
| RepeatPurchase | `rate: u16, min_orders: u32` | 买家订单数 >= min_orders 时生效 |

增强功能：

| 功能 | 存储 | 说明 |
|------|------|------|
| 推荐人激活条件 | `ReferrerGuardConfigs` | 推荐人需满足最低消费/最低订单数才能获得返佣 |
| 返佣上限 | `CommissionCapConfigs` | 单笔上限 (`max_per_order`) + 累计上限 (`max_total_earned`) |
| 配置生效时间 | `ConfigEffectiveAfter` | 延迟生效（区块号之前不执行计算） |
| 全局返佣率上限 | `MaxTotalReferralRate` | 运行时常量，所有子模式合计不超过此比例 |

## 多级分销 (commission-multi-level)

独立 pallet，N 层推荐链遍历 + 每层三维激活条件：

| 激活条件 | 字段 | 数据来源 |
|----------|------|----------|
| 有效直推人数 | `required_directs` | `MemberProvider::get_member_stats().0` |
| 最低团队规模 | `required_team_size` | `MemberProvider::get_member_stats().1` |
| 最低 USDT 消费 | `required_spent` | `MemberProvider::get_member_spent_usdt()` |

条件之间为 AND 逻辑，值为 0 的条件自动跳过。`rate = 0` 的层级为占位层（跳过但消耗推荐链深度）。

关键特性：

- **总佣金上限** -- `max_total_rate` 基点制，超出截断最后一笔并终止
- **循环检测** -- `BTreeSet<AccountId>` 防止环形推荐链
- **全局暂停** -- `pause_multi_level` / `resume_multi_level`
- **延迟生效配置** -- `schedule_config_change` -> 等待 `ConfigChangeDelay` 区块 -> `apply_pending_config`
- **审计日志** -- 环形缓冲（最多 1000 条），记录所有配置变更
- **佣金统计** -- 个人 (`MemberMultiLevelStats`) + Entity 级 (`EntityMultiLevelStats`)
- **部分更新** -- `update_multi_level_params` 单独修改 `max_total_rate` 或指定层配置
- **增删层级** -- `add_tier` / `remove_tier`

## 等级极差返佣 (commission-level-diff)

基于 Entity 自定义等级体系（`custom_level_id`），沿推荐链向上遍历，按等级差价计算每层返佣。

- 配置 `CustomLevelDiffConfig`：`level_rates: BoundedVec<u16>` + `max_depth: u8`
- `level_rates[i]` 表示自定义等级 i 对应的返佣率（基点）
- 每笔只取差额：`(当前节点等级率 - 已达到的最高率) x order_amount / 10000`

## 单线收益 (commission-single-line)

基于全局消费注册顺序的上/下线收益：

| 参数 | 说明 |
|------|------|
| `upline_rate` / `downline_rate` | 上/下线收益比率（基点，最大 1000） |
| `base_upline_levels` / `base_downline_levels` | 基础覆盖层数 |
| `max_upline_levels` / `max_downline_levels` | 层数上限 |
| `level_increment_threshold` | 每累计收益达到此值增加 1 层 |

关键特性：

- **分段存储** -- `SingleLineSegments` 使用 `BoundedVec<AccountId>` 分段，段满自动扩展新段
- **按等级自定义层数** -- `SingleLineCustomLevelOverrides` 按 `custom_level_id` 覆盖基础层数
- **暂停/恢复** -- `pause_single_line` / `resume_single_line`
- **强制重置** -- `force_reset_single_line` 清除所有段和索引（Root only）
- **自动加入** -- 未在链中的用户每次消费时自动加入

## 沉淀池奖励 (commission-pool-reward) v2

周期性等额分配模型（Periodic Equal-Share Claim）：

1. **配置** -- `level_ratios: Vec<(level_id, ratio_bps)>`（sum = 10000）+ `round_duration`（区块数）
2. **新轮次** -- 首个 `claim_pool_reward` 或 `force_new_round` 触发快照，记录池余额和各等级会员数
3. **领取** -- 用户签名调用 `claim_pool_reward`，按等级配比和人数均分
4. **NEX + Token 双池** -- `token_pool_enabled = true` 时同时分配 Entity Token（Token 部分 best-effort）

关键特性：

- **Entity Owner 不可提取** -- 沉淀池资金完全算法驱动
- **轮次历史** -- `RoundHistory` 归档已完成轮次（`MaxRoundHistory` 上限）
- **分配统计** -- `DistributionStatistics` 记录累计分配量、轮次数、领取次数
- **暂停** -- per-entity (`pause_pool_reward`) + 全局 (`set_global_pool_reward_paused`)
- **最小轮次间隔** -- `MinRoundDuration` 运行时常量
- **KYC 守卫** -- `ParticipationGuard` 在领取时强制合规检查
- **领取回调** -- `ClaimCallback` 将 claim 记录写入 core 统一佣金体系

## CommissionProvider Trait

供订单模块调用的 NEX 佣金服务接口：

```rust
pub trait CommissionProvider<AccountId, Balance> {
    fn process_commission(
        entity_id: u64, shop_id: u64, order_id: u64,
        buyer: &AccountId, order_amount: Balance,
        available_pool: Balance, platform_fee: Balance,
    ) -> Result<(), DispatchError>;

    fn cancel_commission(order_id: u64) -> Result<(), DispatchError>;
    fn pending_commission(entity_id: u64, account: &AccountId) -> Balance;
    fn set_commission_modes(entity_id: u64, modes: u16) -> Result<(), DispatchError>;
    fn set_direct_reward_rate(entity_id: u64, rate: u16) -> Result<(), DispatchError>;
    fn set_level_diff_config(entity_id: u64, level_rates: Vec<u16>) -> Result<(), DispatchError>;
    fn set_fixed_amount(entity_id: u64, amount: Balance) -> Result<(), DispatchError>;
    fn set_first_order_config(entity_id: u64, amount: Balance, rate: u16, use_amount: bool) -> Result<(), DispatchError>;
    fn set_repeat_purchase_config(entity_id: u64, rate: u16, min_orders: u32) -> Result<(), DispatchError>;
    fn set_withdrawal_config_by_governance(entity_id: u64, enabled: bool) -> Result<(), DispatchError>;
    fn shopping_balance(entity_id: u64, account: &AccountId) -> Balance;
    fn use_shopping_balance(entity_id: u64, account: &AccountId, amount: Balance) -> Result<(), DispatchError>;
    fn set_min_repurchase_rate(entity_id: u64, rate: u16) -> Result<(), DispatchError>;
    fn set_owner_reward_rate(entity_id: u64, rate: u16) -> Result<(), DispatchError>;
}
```

> 注意：`shopping_balance` 和 `use_shopping_balance` 的 NEX 实现现已委托给 Loyalty 模块（通过 `T::Loyalty` Port）。

Token 版：`TokenCommissionProvider` 提供 `process_token_commission` / `cancel_token_commission` / `pending_token_commission` / `token_platform_fee_rate`。

## CommissionPlugin Trait

每个返佣插件实现此 trait，由 core 调度引擎（engine.rs）调用：

```rust
pub trait CommissionPlugin<AccountId, Balance> {
    fn calculate(
        entity_id: u64, buyer: &AccountId, order_amount: Balance,
        remaining: Balance, enabled_modes: CommissionModes,
        is_first_order: bool, buyer_order_count: u32,
    ) -> (Vec<CommissionOutput<AccountId, Balance>>, Balance);
}
```

Token 版对称接口：`TokenCommissionPlugin::calculate_token`。

## PlanWriter Traits

供 Governance 模块通过治理路径写入各插件配置：

| Trait | 插件 | 关键方法 |
|-------|------|----------|
| `ReferralPlanWriter` | referral | `set_direct_rate` / `set_fixed_amount` / `set_first_order` / `set_repeat_purchase` / `clear_config` |
| `MultiLevelPlanWriter` | multi-level | `set_multi_level` / `set_multi_level_full` / `clear_multi_level_config` |
| `LevelDiffPlanWriter` | level-diff | `set_level_rates` / `clear_config` |
| `TeamPlanWriter` | team | `set_team_config` / `clear_config` |
| `SingleLinePlanWriter` | single-line | `set_single_line_config` / `set_level_based_levels` / `clear_config` / `clear_level_overrides` |
| `PoolRewardPlanWriter` | pool-reward | `set_pool_reward_config` / `set_token_pool_enabled` / `clear_config` |

## 安全机制

| 机制 | 说明 |
|------|------|
| **偿付安全** | `withdraw_entity_funds` 检查 `balance >= PendingTotal + Loyalty::shopping_total + UnallocatedPool` |
| **循环检测** | referral / level-diff / multi-level 推荐链遍历使用 `BTreeSet<AccountId>` 防环 |
| **KYC 守卫** | `ParticipationGuard` trait 在提现、购物余额消费、池奖励领取时强制合规检查 |
| **取消安全** | `cancel_commission` 先读后写，转账失败不修改记录状态 |
| **沉淀池冷却** | 关闭 `POOL_REWARD` 后 `PoolRewardWithdrawCooldown` 区块内不可提取沉淀池资金 |
| **封禁检查** | 所有插件在计算时跳过被封禁(`is_banned`)、未激活(`!is_activated`)、冻结/暂停(`!is_member_active`)的会员 |
| **Entity 锁定** | 全局锁定时所有配置操作不可用（`is_entity_locked`） |
| **Entity 活跃** | 暂停/封禁/关闭的 Entity 不可修改配置、不分配佣金 |
| **配置校验** | `integrity_test` 在运行时启动时校验常量合法性（如 `MaxMultiLevels in [1, 100]`） |

## 安装

```toml
[dependencies]
pallet-entity-commission = { path = "pallets/entity/commission", default-features = false }

[features]
std = ["pallet-entity-commission/std"]
```

本 crate 是 re-export wrapper，各子模块也可单独引用：

```toml
pallet-commission-core = { path = "pallets/entity/commission/core", default-features = false }
pallet-commission-referral = { path = "pallets/entity/commission/referral", default-features = false }
```

## 返佣模式组合推荐

| 场景 | 推荐组合 | 说明 |
|------|----------|------|
| 社交电商 | 直推 + 多级分销 | 激励分享裂变，多级分销配合激活条件筛选优质推荐人 |
| 代理商体系 | 等级差价 + 团队业绩 | 激励代理升级和团队达标 |
| 拉新活动 | 直推 + 首单奖励 + 固定金额 | 快速拉新，首单额外激励 |
| 复购型 | 直推 + 复购奖励 | 提高复购率 |
| 被动收益型 | 单线上线 + 单线下线 | 无需推荐即可获益 |
| 高等级回馈 | 上述任意 + 沉淀池奖励 | 未分配佣金按等级 cap + 全池等额回馈（不是按比率切池） |
| **首版主网（推荐）** | **多级 + 单线上下 + 沉淀池** | **货款 20% / ML cap 8% / SL cap 4% / 目标进池 8%；见仓库 `docs/COMMISSION_ML_SL_POOL_MAINNET_TEMPLATE.md`** |
| 创建人激励 | 上述任意 + 创建人收益 | 从佣金预算中优先给创建人分成 |
