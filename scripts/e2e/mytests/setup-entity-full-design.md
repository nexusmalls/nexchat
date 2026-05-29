# Metaverse Door — Entity Setup Automation Script Design

> 数据来源：`元宇宙模式.docx` + `Metaverse Door(5).pdf`

## 一、业务模型概览

Metaverse Door 的核心分配制度：

- **总佣金率 = 100%**（套餐金额全额进入分配池，扣除 1% 平台管理费后 99% 进入佣金分配）
- **静态奖励 50%** — 共赢（Single-Line 全网一条线公排，上下社区共 100 层）
- **动态奖励 50%** — 助力（Multi-Level 太阳线，最多 13 级推荐奖励分配）
- **平台管理费 1%** — 平台收取

## 二、套餐 → 会员等级映射

7 个套餐对应 7 个会员等级，threshold 为套餐投资金额（原始金额÷100，方便测试）：

| Level | Name   | 套餐金额 (USDT) | 奖励级数 | 间推要求 | 上层 | 下层 | 提现  | 复投  |
|-------|--------|---------------|---------|---------|------|------|-------|-------|
| 1     | Plan1  | 5             | 7 级    | 无      | 20层 | 30层 | 40%   | 60%   |
| 2     | Plan2  | 10            | 7 级    | 无      | 24层 | 36层 | 40%   | 60%   |
| 3     | Plan3  | 20            | 7 级    | 无      | 28层 | 42层 | 40%   | 60%   |
| 4     | Plan4  | 30            | 7 级    | 无      | 32层 | 48层 | 40%   | 60%   |
| 5     | Plan5  | 50            | 9 级    | 间推 5  | 40层 | 60层 | 50%   | 50%   |
| 6     | Plan6  | 150           | 11 级   | 间推 8  | 40层 | 60层 | 60%   | 40%   |
| 7     | Plan7  | 500           | 13 级   | 间推 10 | 40层 | 60层 | 70%   | 30%   |

**说明：**
- "奖励级数" = 该会员可解锁的 Multi-Level 推荐奖励深度
- "上层/下层" = Single-Line 公排可获得奖励的层数范围
- "间推要求" = 升级到该等级所需的间接推荐人数 (`min_indirect_referrals`)
- "提现/复投" = WithdrawalConfig 的 `withdrawal_rate / repurchase_rate`（按等级分层）

## 三、链上参数设计

### Step 0: 初始化
```
- cryptoWaitReady()
- 从助记词派生 sr25519 keypair（ss58Format=273）
- 连接链节点 ws://202.140.140.202:9944（默认）
- 打印地址、余额
```

### Step 1: 检测或创建 Entity
```
目标 Entity ID: 100000（可通过 TARGET_ENTITY_ID 环境变量覆盖）

1a. 先查询链上 entityRegistry.entities(100000)
    - 如果 entity 存在（isSome）：
      - 跳过创建，直接使用 entityId = 100000
      - 读取 primaryShopId
      - 打印 "Entity 100000 already exists, reusing"
    - 如果 entity 不存在（isNone）：
      - 走创建流程: entityRegistry.createEntity(name, null, null, null)
      - name: 可配置，默认 "MetaverseDoor-{timestamp}"
      - 等待 entity ID 出现（waitForNewEntityId 轮询）
      - 读取自动创建的 primaryShopId

注意：当复用已有 entity 时，后续 Step 2~5 的操作都在该 entity 上执行。
如果某些配置已存在（如等级系统已初始化），对应 extrinsic 可能会报错 —— 脚本应
catch 并跳过已完成的步骤，打印 warning 日志继续后续步骤。
```

### Step 2: 追加店铺运营资金
```
extrinsic: entityShop.fundOperating(shopId, amount)
- 默认 100000 NEX（套餐金额大，需要充足运营资金；含商品押金预留）
- 每次运行都会追加（幂等安全，不会因已存在而报错）
```

### Step 3: 创建 7 级会员等级

```
extrinsic: entityMember.initLevelSystem(shopId, true, 'AutoUpgrade')
extrinsic: entityMember.addCustomLevel(shopId, name, threshold, discount_rate, commission_bonus,
    0, 0, 0, min_indirect_referrals, 0)  × 7
```

| Level | Name   | threshold (USDT×10^6) | discount_rate (bps) | commission_bonus (bps) | min_indirect_referrals |
|-------|--------|-----------------------|---------------------|------------------------|------------------------|
| 1     | Plan1  | 5_000_000             | 0                   | 0                      | 0                      |
| 2     | Plan2  | 10_000_000            | 0                   | 0                      | 0                      |
| 3     | Plan3  | 20_000_000            | 0                   | 0                      | 0                      |
| 4     | Plan4  | 30_000_000            | 0                   | 0                      | 0                      |
| 5     | Plan5  | 50_000_000            | 0                   | 0                      | 5                      |
| 6     | Plan6  | 150_000_000           | 0                   | 0                      | 8                      |
| 7     | Plan7  | 500_000_000           | 0                   | 0                      | 10                     |

**备注：**
- `discount_rate` / `commission_bonus` 设为 0 — Metaverse Door 模式中不存在打折/佣金加成概念，等级主要控制的是 "解锁推荐级数" 和 "Single-Line 层数"，这些由插件自身的配置控制
- `min_indirect_referrals`: 套餐1~4 无间推要求 (0)，套餐5=5人，套餐6=8人，套餐7=10人
- threshold 精度: USDT 10^6，所以 5 USDT = 5_000_000（原始金额÷100，方便测试）

### Step 3b: 创建 7 个套餐商品

每个套餐对应一个商品，USDT 价格 = 套餐金额，用于会员购买升级。

**幂等处理：**
- 先查询 `entityProduct.shopProducts(shopId)` 获取已有商品列表
- 如果店铺已有 ≥ 7 个商品，跳过创建，复用已有商品 ID
- 创建前自动追加运营资金（`fundOperating`），确保押金充足
  - 每个商品需要 `ProductDepositUsdt`（1 USDT）等值 NEX 押金
  - 从店铺运营资金账户扣除，不足则创建失败
  - 脚本在创建商品前自动从创建者账户转入运营资金

```
extrinsic: entityProduct.createProduct(
  shopId, name_cid, images_cid, detail_cid,
  nex_price,      // NEX 价格（链上自动换算，传 0 让链用 USDT 价格换算）
  usdt_price,     // USDT 价格（精度 10^6）
  stock,          // 0 = 无限库存
  category,       // 'Digital'
  sort_weight,    // 排序权重（1~7，套餐序号）
  tags_cid,       // 空
  sku_cid,        // 空
  min_order_quantity, // 1
  max_order_quantity, // 1（每单限购 1 份）
  visibility,     // 'MembersOnly'（仅会员可见可购）
)
extrinsic: entityProduct.publishProduct(productId)  × 7
```

| # | Name          | usdt_price (10^6) | stock | visibility  | sort_weight |
|---|---------------|-------------------|-------|-------------|-------------|
| 1 | Plan1-5USDT   | 5_000_000         | 0     | MembersOnly | 1           |
| 2 | Plan2-10USDT  | 10_000_000        | 0     | MembersOnly | 2           |
| 3 | Plan3-20USDT  | 20_000_000        | 0     | MembersOnly | 3           |
| 4 | Plan4-30USDT  | 30_000_000        | 0     | MembersOnly | 4           |
| 5 | Plan5-50USDT  | 50_000_000        | 0     | MembersOnly | 5           |
| 6 | Plan6-150USDT | 150_000_000       | 0     | MembersOnly | 6           |
| 7 | Plan7-500USDT | 500_000_000       | 0     | MembersOnly | 7           |

**备注：**
- `nex_price` 传 `0`（链上根据 USDT/NEX 汇率自动换算）
- `stock = 0` 表示无限库存（套餐可反复购买）
- `visibility = MembersOnly` — 只有注册会员才能查看和购买
- `min/max_order_quantity = 1` — 每笔订单只能购买 1 份套餐
- `category = Digital` — 虚拟商品
- 创建后立即 `publishProduct` 将状态从 Draft → OnSale

### Step 4: 配置佣金系统核心

```
4a. setCommissionRate(entityId, 9900)
    — 总佣金率 99%（100% 套餐金额 - 1% 平台管理费 = 99% 进入佣金分配池）

4b. setPluginBudgetCaps(entityId, caps)
    — 已启用插件的 cap 必须 > 0（cap=0 表示插件预算为零，等于关闭）
    — cap 基于订单金额的 bps
    — creator 先从佣金池扣 500 bps of pool = 495 bps of order
    — 插件可用 = 9900 - 495 = 9405 bps of order
    — caps:
      referral_cap:    0       // 未启用
      multi_level_cap: 4702   // ≈47.02%（动态奖励）
      level_diff_cap:  0       // 未启用
      single_line_cap: 4703   // ≈47.03%（静态奖励）
      team_cap:        0       // 未启用
    — 校验: 4702 + 4703 = 9405 = 佣金池 - creator ✓

4c. setCommissionModes(entityId, modes)
    — 开启 MULTI_LEVEL + SINGLE_LINE_UPLINE + SINGLE_LINE_DOWNLINE + POOL_REWARD + OWNER_REWARD
    modes = 0b10 | 0b1000_0000 | 0b1_0000_0000 | 0b10_0000_0000 | 0b100_0000_0000 = 1922

4d. setOwnerRewardRate(entityId, 500)
    — 创建人收益 = 佣金池的 5% (500 bps of pool)
    — 等效于订单金额的 4.95% (495 bps of order)
    — 从佣金池中优先扣除（在所有插件分配之前）

4e. enableCommission(entityId, true)
    — 激活佣金系统

4f. setWithdrawalConfig(entityId, ...)
    — 提现模式: LevelBased（按会员等级自动决定提现/复投比例）
    — 每个等级有独立的 withdrawal_rate / repurchase_rate

    mode: 'LevelBased'
    defaultTier: { withdrawal_rate: 4000, repurchase_rate: 6000 }  // 兜底: 40/60
    level_overrides: [
      [1, { withdrawal_rate: 4000, repurchase_rate: 6000 }],  // Plan1: 40/60
      [2, { withdrawal_rate: 4000, repurchase_rate: 6000 }],  // Plan2: 40/60
      [3, { withdrawal_rate: 4000, repurchase_rate: 6000 }],  // Plan3: 40/60
      [4, { withdrawal_rate: 4000, repurchase_rate: 6000 }],  // Plan4: 40/60
      [5, { withdrawal_rate: 5000, repurchase_rate: 5000 }],  // Plan5: 50/50
      [6, { withdrawal_rate: 6000, repurchase_rate: 4000 }],  // Plan6: 60/40
      [7, { withdrawal_rate: 7000, repurchase_rate: 3000 }],  // Plan7: 70/30
    ]
    voluntary_bonus_rate: 0
    enabled: true
```

### Step 5: 配置三个佣金插件

#### 5a. Multi-Level 插件（动态奖励 — 太阳线 13 级）

```
extrinsic: commissionMultiLevel.setMultiLevelConfig(entityId, tiers, maxTotalRate)
```

文档原文分配比例（基于订单金额 bps），maxTotalRate=4702 截断超出部分：

| Tier | 级别    | 文档比例 | rate (bps) | required_directs | required_team_size | required_spent |
|------|---------|---------|------------|------------------|--------------------|----------------|
| L1   | 第一级  | 15%     | 1500       | 0                | 0                  | 0              |
| L2   | 第二级  | 5%      | 500        | 0                | 0                  | 0              |
| L3   | 第三级  | 2%      | 200        | 0                | 0                  | 0              |
| L4   | 第四级  | 2%      | 200        | 0                | 0                  | 0              |
| L5   | 第五级  | 2%      | 200        | 0                | 0                  | 0              |
| L6   | 第六级  | 2%      | 200        | 0                | 0                  | 0              |
| L7   | 第七级  | 2%      | 200        | 0                | 0                  | 0              |
| L8   | 第八级  | 7%      | 700        | 0                | 0                  | 0              |
| L9   | 第九级  | 2%      | 200        | 0                | 0                  | 0              |
| L10  | 第十级  | 2%      | 200        | 0                | 0                  | 0              |
| L11  | 第十一级 | 2%     | 200        | 0                | 0                  | 0              |
| L12  | 第十二级 | 2%     | 200        | 0                | 0                  | 0              |
| L13  | 第十三级 | 5%     | 500        | 0                | 0                  | 0              |

```
maxTotalRate = 4702 (cap=4702, 各级 rate 总和=5000 超出部分被 maxTotalRate 截断)
```

**解锁逻辑：** 套餐1~4 只能拿 7 级，套餐5 拿 9 级，套餐6 拿 11 级，套餐7 拿 13 级。
这通过会员等级 + 链上引擎的 `member level → unlocked tiers` 映射来实现（engine 层面根据会员等级截断层数）。

#### 5b. Single-Line 插件（静态奖励 — 全网一条线公排, cap=4703）

```
extrinsic: commissionSingleLine.setSingleLineConfig(
  entityId,
  48,    // upline_rate: 0.48% per level (bps of order_amount)
  47,    // downline_rate: 0.47% per level
  20,    // base_upline_levels (最低套餐的上层数)
  30,    // base_downline_levels (最低套餐的下层数)
  0,     // level_increment_threshold
  40,    // max_upline_levels (最高套餐的上层数)
  60,    // max_downline_levels (最高套餐的下层数)
)
```

**费率校验（最高套餐 max levels）：**
- 48 × 40 + 47 × 60 = 1920 + 2820 = 4740 bps ≤ single_line_cap (4703)
- 实际分配受 cap (4703) 和 remaining 限制，per-level 设高一点没关系

#### 5b-2. Single-Line 等级层数覆盖（为不同会员等级配置不同上/下线层数）

```
extrinsic: commissionSingleLine.setLevelBasedLevels(entityId, level_id, upline_levels, downline_levels)  × 7
```

不设覆盖时，所有会员使用 base_upline_levels / base_downline_levels（最低套餐层数）。
通过 LevelOverride 为每个套餐等级配置精确层数：

| level_id | 套餐    | upline_levels | downline_levels | 总层数 |
|----------|---------|---------------|-----------------|--------|
| 1        | Plan1   | 20            | 30              | 50     |
| 2        | Plan2   | 24            | 36              | 60     |
| 3        | Plan3   | 28            | 42              | 70     |
| 4        | Plan4   | 32            | 48              | 80     |
| 5        | Plan5   | 40            | 60              | 100    |
| 6        | Plan6   | 40            | 60              | 100    |
| 7        | Plan7   | 40            | 60              | 100    |

**校验：** 所有 upline_levels ≤ max_upline_levels(40) ✓，downline_levels ≤ max_downline_levels(60) ✓

**备注：**
- `level_id` 是 1-based（对应 entityMember 的等级 ID）
- 套餐1 的层数 = base 配置（20/30），但仍显式设置 override 以保持一致性
- 套餐5~7 的层数 = max 配置（40/60）
- 数据来源: 元宇宙模式.docx「提现与复投机制」表 + Metaverse Door(5).pdf 第14页

**Static reward 分配逻辑：**
- 50% 总佣金由 Single-Line 分配
- 全网一条线公排，每个会员加入后按序排列
- 按套餐等级，可获得收益的上层/下层范围不同：
  - 套餐1: 上20层 / 下30层（共50层）
  - 套餐2: 上24层 / 下36层（共60层）
  - 套餐3: 上28层 / 下42层（共70层）
  - 套餐4: 上32层 / 下48层（共80层）
  - 套餐5~7: 上40层 / 下60层（共100层）

#### 5c. Pool-Reward 插件（沉淀资金分红）

```
extrinsic: commissionPoolReward.setPoolRewardConfig(
  entityId,
  level_ratios,
  round_duration,
)
```

文档原文（docx 第七节）：
- 套餐5 (5000)：沉淀资金 20% 人工分红
- 套餐6 (15000)：沉淀资金 30% 人工分红
- 套餐7 (50000)：沉淀资金 50% 人工分红
- 套餐1~4：不参与沉淀分红

```
level_ratios:
  [4, 2000],   // Level 5 (Plan5): 20%
  [5, 3000],   // Level 6 (Plan6): 30%
  [6, 5000],   // Level 7 (Plan7): 50%

round_duration: 14_400  (~24h @ 6s/block)
```

**备注：** 比例之和 = 10000 (100%)，套餐1~4 (Level 0~3) 不分配 pool reward。

### Step 6: 验证 & 输出
```
- 查询 commissionCore.commissionConfigs(entityId) 确认佣金配置
- 查询 commissionMultiLevel.multiLevelConfigs(entityId) 确认 13 级 ML 配置
- 查询 commissionSingleLine.singleLineConfigs(entityId) 确认 SL 配置
- 查询 commissionPoolReward.poolRewardConfigs(entityId) 确认 PR 配置
- 查询 entityMember EntityLevelSystems 确认 7 级别
- 查询 entityProduct.products(productId) 确认 7 个套餐商品（遍历 productIds 列表）
- 打印完整摘要 JSON（entityId, shopId, productIds, 各配置状态）
- 保存到 secrets/setup-result-{timestamp}.json
```

## 四、佣金分配数值汇总

所有费率均基于**订单支付金额**的 bps (万分比)，总和不得超过 10000 (100%)。

以套餐7 (500 USDT, 原50,000÷100) 为例的资金流向：

```
订单金额: 500 USDT (10000 bps = 100%)
  ├── 平台管理费: 5 (100 bps = 1%)        ← 100% - commission_rate(9900)
  └── 佣金池: 495 (9900 bps = 99%)       ← commission_rate
        ├── 创建人收益: 24.75 (495 bps)   ← pool × owner_reward_rate(500/10000)
        │     = 495 × 500 / 10000 = 24.75 USDT
        └── 插件可分配: 470.25 (9405 bps) ← pool - creator
              ├── 动态奖励 ML cap: 4702 bps (47.02%)
              │     L1=1500 L2=500 L3~L7=200×5 L8=700 L9~L12=200×4 L13=500
              │     maxTotalRate=4702 截断（各级总和5000超出部分不分配）
              ├── 静态奖励 SL cap: 4703 bps (47.03%)
              │     上40层×48bps + 下60层×47bps = 4740 (受 cap 4703 截断)
              └── 未分配部分 → 沉淀池 (Pool Reward)
                    套餐5: 20%  套餐6: 30%  套餐7: 50%

费率校验:
  commission_rate (9900) ≤ 10000 ✓
  creator (495 bps of order) = pool × 500/10000 ✓
  ML cap (4702) + SL cap (4703) = 9405 = pool - creator ✓
  ML maxTotalRate (4702) ≤ ML cap (4702) ✓
  全部费率总和 = 100(平台) + 495(creator) + 4702(ML) + 4703(SL) = 10000 ✓
```

## 五、提现与复投机制

| 套餐    | 等级 ID | 提现比例          | 复投比例          |
|---------|---------|-----------------|-----------------|
| 1~4     | 1~4     | 40% (4000 bps)  | 60% (6000 bps)  |
| 5       | 5       | 50% (5000 bps)  | 50% (5000 bps)  |
| 6       | 6       | 60% (6000 bps)  | 40% (4000 bps)  |
| 7       | 7       | 70% (7000 bps)  | 30% (3000 bps)  |

**实现方式：** `WithdrawalMode::LevelBased` + 7 个 `level_overrides`，按会员等级自动适用对应比例。
`defaultTier` 设为 40/60（兜底，未升级会员适用最低比例）。

## 六、脚本文件 & 运行方式

### 脚本文件

`/home/xiaodong/桌面/nexus/scripts/setup-entity-full.ts`

### 复用现有框架

- `e2e/framework/api.ts` — `connectApi()`, `submitTx()`, `disconnectApi()`
- `e2e/framework/accounts.ts` — `readFreeBalance()`
- `e2e/framework/assert.ts` — `assertTxSuccess()`
- `e2e/framework/units.ts` — `nex()`, `formatNex()`
- `e2e/framework/codec.ts` — `codecToJson()`, `readObjectField()`
- `e2e/suites/helpers.ts` — `readEntity()`, `resolvePrimaryShopId()`, `readEntityIds()`, `waitForNewEntityId()`, `readNextProductId()`
- `utils/ss58.ts` — `NEXUS_SS58_FORMAT`

### 运行方式

```bash
cd scripts
# 默认连接远端节点 202.140.140.202:9944
node --import tsx setup-entity-full.ts

# 可选环境变量
WS_URL=ws://127.0.0.1:9944 \
ENTITY_NAME="MetaverseDoor" \
SHOP_FUND_NEX=10000 \
TARGET_ENTITY_ID=100000 \
node --import tsx setup-entity-full.ts
```

### package.json

```json
"setup:entity": "node --import tsx setup-entity-full.ts"
```

### 关键文件

| 文件 | 用途 |
|------|------|
| `scripts/setup-entity-full.ts` | 新建 — 主脚本 |
| `scripts/e2e/framework/api.ts` | 复用 — 连接 & 提交交易 |
| `scripts/e2e/suites/helpers.ts` | 复用 — 读 entity/shop |
| `scripts/e2e/framework/units.ts` | 复用 — NEX 单位换算 |
| `scripts/utils/ss58.ts` | 复用 — SS58 格式 |
| `scripts/commission-e2e-test.ts` | 参考 — 佣金配置模式 |
| `scripts/e2e/suites/entity-commerce-commission-flow.ts` | 参考 — pool-reward 配置 |

### Extrinsic 签名参考（来自 pallet 源码）

**addCustomLevel** (`pallets/entity/member/src/lib.rs:1172`)
```rust
pub fn add_custom_level(
    origin: OriginFor<T>,
    shop_id: u64,
    name: BoundedVec<u8, ConstU32<32>>,
    threshold: u64,
    discount_rate: u16,
    commission_bonus: u16,
    min_direct_referrals: u32,
    min_qualified_referrals: u32,
    min_team_size: u32,
    min_indirect_referrals: u32,
    min_qualified_indirect_referrals: u32,
) -> DispatchResult
```

**createProduct** (`pallets/entity/product/src/lib.rs:393`)
```rust
pub fn create_product(
    origin: OriginFor<T>,
    shop_id: u64,
    name_cid: Vec<u8>,
    images_cid: Vec<u8>,
    detail_cid: Vec<u8>,
    usdt_price: u64,       // USDT 价格，精度 10^6
    stock: u32,            // 0 = 无限库存
    category: ProductCategory,
    sort_weight: u32,
    tags_cid: Vec<u8>,
    sku_cid: Vec<u8>,
    min_order_quantity: u32,
    max_order_quantity: u32,
    visibility: ProductVisibility,
) -> DispatchResult
```

**publishProduct** (`pallets/entity/product/src/lib.rs`)
```rust
pub fn publish_product(origin: OriginFor<T>, product_id: u64) -> DispatchResult
```

**setLevelBasedLevels** (`pallets/entity/commission/single-line/src/lib.rs:447`)
```rust
pub fn set_level_based_levels(
    origin: OriginFor<T>,
    entity_id: u64,
    level_id: u8,
    upline_levels: u8,
    downline_levels: u8,
) -> DispatchResult
```

### 验证方式

1. 运行脚本后检查控制台输出各步骤成功日志
2. 访问 http://202.140.140.202:3003/ 用该助记词导入账户，验证：
   - Entity 出现在管理列表
   - 店铺可见且运营资金已到账
   - 会员等级页面显示 7 个自定义等级（Plan1~Plan7）
   - 商品页面显示 7 个套餐商品（Plan1-5USDT ~ Plan7-500USDT），状态为 OnSale
   - 佣金页面显示 multi-level(13级) / single-line / pool-reward 已启用
3. 通过 polkadot.js apps 查询链上存储确认各配置值

## 七、与旧设计的主要差异

| 项目 | 旧设计 | 新设计（Metaverse Door） |
|------|--------|------------------------|
| 会员等级名称 | VIP1~VIP7 | Plan1~Plan7 |
| 等级 threshold | 100~100,000 USDT | 5~500 USDT（原始÷100，方便测试） |
| discount_rate | 100~800 bps | 0（无折扣概念） |
| commission_bonus | 100~1500 bps | 0（无加成概念） |
| min_indirect_referrals | 全部 0 | 0,0,0,0,5,8,10（套餐5+有间推要求） |
| 总佣金率 | 20% (2000 bps) | 99% (9900 bps)（100%-1%平台费） |
| ML 层数 | 7 级 | 13 级 |
| ML 费率 | 500,300,200,150,100,50,50 | 1500,500,200×5,700,200×4,500 (maxTotalRate=4512截断) |
| ML maxTotalRate | 1350 bps | 4702 bps (≤ cap) |
| SL upline/downline rate | 1%/1% | 0.48%/0.47% per level |
| SL 层数 | 3/3 ~ 5/5 | 20/30 ~ 40/60 |
| 创建人收益 | 无 | 500 bps of pool (4.95% of order) |
| 提现/复投 | 统一 50/50 | LevelBased: Plan1~4=40/60 Plan5=50/50 Plan6=60/40 Plan7=70/30 |
| Plugin budget caps | 无 | ML=4702 SL=4703 (sum=9405=pool-creator) |
| Pool Reward | 全等级均分 | 仅套餐5~7，比例 20/30/50 |
| 店铺运营资金 | 2000 NEX | 100000 NEX（含商品押金预留） |
| 套餐商品 | 无 | 7 个 Plan 商品（5~500 USDT，MembersOnly） |
