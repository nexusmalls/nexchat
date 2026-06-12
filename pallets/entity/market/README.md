# pallet-entity-market v2.5.0

> 实体代币 P2P 交易市场 | Runtime Index: 126

## 概述

每个 Entity 可独立运营自己的代币市场。所有交易以原生 NEX 代币结算，链上原子交换，**零手续费**。

**核心能力**

- 5 种订单类型 — 限价、市价、IOC、FOK、Post-Only
- 自动交叉撮合 — 限价单挂单时自动与价格交叉的对手方订单成交
- 三周期 TWAP 预言机 — 1h / 24h / 7d 时间加权平均价格
- 熔断机制 — 价格偏离 7d TWAP 超阈值自动暂停，到期自动恢复并清理状态
- 价格偏离保护 — 限价单 / 改单价格不得偏离参考价过大
- 自吃单防护 — 所有撮合路径均跳过自己的订单
- KYC 准入 / 内幕交易限制 — 集成 KycProvider + DisclosureProvider
- 过期订单自动清理 — `on_idle` 游标扫描，权重精确追踪

## 架构

```
┌──────────────────────────────────────────────────────────────┐
│                     pallet-entity-market                      │
├──────────────────────────────────────────────────────────────┤
│  用户交易 (signed)                                            │
│  place_sell_order(0)  place_buy_order(1)  take_order(2)       │
│  cancel_order(3)      market_buy(12)      market_sell(13)     │
│  batch_cancel_orders(28)  modify_order(30)                    │
│  cancel_all_entity_orders(35)                                 │
│  place_ioc_order(38)  place_fok_order(39)                     │
│  place_post_only_order(40)                                    │
├──────────────────────────────────────────────────────────────┤
│  市场管理 (Entity Owner)                                      │
│  configure_market(4)        configure_price_protection(15)    │
│  lift_circuit_breaker(16)   set_initial_price(17)             │
│  pause_market(26)           resume_market(27)                 │
│  set_kyc_requirement(33)    close_market(34)                  │
├──────────────────────────────────────────────────────────────┤
│  治理 / Root                                                  │
│  force_cancel_order(23)     global_market_pause(32)           │
│  governance_configure_market(36)                              │
│  force_close_market(37)                                       │
│  governance_configure_price_protection(41)                    │
│  force_lift_circuit_breaker(42)                               │
├──────────────────────────────────────────────────────────────┤
│  维护 (任何人)               │  on_idle 自动清理               │
│  cleanup_expired_orders(29)  │  游标扫描 → 退资产 → 释名额    │
├──────────────────────────────┴────────────────────────────────┤
│  TWAP 预言机: 异常价格过滤 → 累积器 → 1h/24h/7d 双 checkpoint │
└──────────────────────────────────────────────────────────────┘
         │               │               │
    EntityProvider   TokenProvider   DisclosureProvider
    (查询/权限)      (余额/锁定)     (内幕交易检查)
         │               │
    KycProvider      PricingProvider
    (KYC 等级)       (NEX/USDT 价格)
```

## 交易流程

```
Alice (卖家)                                    Bob (买家)
    │ place_sell_order(entity, 1000, 100)          │
    │ → 检查链: 市场/KYC/内幕/价格偏离/熔断        │
    │ → Token reserved                             │
    │ → 自动撮合价格交叉的买单                       │
    │                                               │
    │                     take_order(order_id, None) │
    │                     → 检查链: 市场/KYC/内幕/熔断│
    ▼                                               ▼
┌─────────────────────────────────────────────────────┐
│  原子交换（零手续费）                                 │
│  Token: Alice(reserved) ──→ Bob(free)                │
│  NEX:   Bob(free) ──→ Alice(free)                    │
└─────────────────────────────────────────────────────┘
    │
    ▼
on_trade_completed → TWAP 更新 → 日统计 → 熔断检查
```

## 订单类型

| 类型 | 枚举值 | 行为 |
|------|--------|------|
| **限价单** | `Limit` | 挂单等待撮合，自动与价格交叉的对手方订单成交，剩余部分挂入订单簿 |
| **市价单** | `Market` | 立即以最优价格逐笔成交，`max_cost` / `min_receive` 滑点保护 |
| **IOC** | `ImmediateOrCancel` | 立即成交能成交的部分，剩余自动取消退还 |
| **FOK** | `FillOrKill` | 全部能成交才执行，否则整单取消（预检查可填充量） |
| **Post-Only** | `PostOnly` | 仅挂单入簿，若价格会立即撮合则拒绝（使用动态计算过滤过期对手单） |

## 市场生命周期

```
                configure_market(nex_enabled=true)
                         │
                         ▼
                    ┌─────────┐
           ┌──────→│  Active  │◄──────┐
           │       └────┬────┘       │
    resume_market       │       pause_market
           │            │            │
           │       ┌────▼────┐       │
           └───────│ Paused  │───────┘
                   │(config) │
                   └─────────┘
                        │
              close_market / force_close_market
                        │
                   ┌────▼────┐
                   │ Closed  │  不可逆，所有订单退还
                   └─────────┘
```

**两层暂停机制**:
- **实体级**: `MarketConfig.paused` — Entity Owner 通过 `pause_market` / `resume_market` 控制
- **全局级**: `GlobalMarketPaused` — Root 通过 `global_market_pause` 控制所有市场

**禁用行为**: `configure_market(nex_enabled=false)` 或 `governance_configure_market(nex_enabled=false)` 会自动取消所有活跃订单并退还锁定资产。

## TWAP 预言机

三周期时间加权平均价格，防止价格操纵。

```
每次成交 → on_trade_completed()
  │
  ├── update_twap_accumulator()
  │     ├── 异常价格过滤: 偏离上次价格 >100% → 限幅至 ±50%
  │     ├── 累积价格更新: cumulative += last_price × blocks_elapsed
  │     ├── 记录 first_trade_block（首笔真实成交，历史覆盖起点）
  │     └── 双 checkpoint 轮换: curr 距今 >= 1 个周期时 prev ← curr, curr ← 当前
  │           ├── 1h:  prev/curr（活跃市场 prev 年龄 1~2 小时）
  │           ├── 24h: prev/curr（活跃市场 prev 年龄 1~2 天）
  │           └── 7d:  prev/curr（活跃市场 prev 年龄 1~2 周）
  │
  ├── update_last_trade_price()
  ├── emit TwapUpdated { twap_1h, twap_24h, twap_7d }
  └── check_circuit_breaker()
        └── 偏离 7d TWAP > threshold → 触发熔断
```

**TWAP 计算**: `(current_cumulative - checkpoint_cumulative) / block_diff`。
选取"距今至少一个周期、且最接近一个周期"的 checkpoint（curr 满周期则用 curr，否则用 prev），保证测量窗口真实覆盖命名周期；市场早期两者都不足一个周期时退化为最旧 checkpoint（短窗口尽力估计）。

**价格偏离检查优先级** (`check_price_deviation`):
1. 成交量 >= `min_trades_for_twap` 且真实成交历史 >= 1 小时（`current_block - first_trade_block >= BlocksPerHour`）→ 使用 1h TWAP
2. 不满足但有 `initial_price` → 使用实体所有者设定的初始价格
3. 都没有 → 跳过检查

> v2.5.0 修复: 历史覆盖时长从首笔真实成交（`first_trade_block`）起算，而非快照年龄。
> 旧实现要求快照"距今 >= 周期"，但快照随成交滚动刷新、市场越活跃年龄越小，
> 导致参考价永远停留在 `initial_price`、无法切换到 TWAP。

**熔断机制**:
- 触发: 成交价偏离 7d TWAP 超过 `circuit_breaker_threshold` → 暂停 `CircuitBreakerDuration` 个区块
- 恢复: 到期后下一笔交易检查时**自动清理存储状态**并发出 `CircuitBreakerLifted` 事件
- 手动解除: Owner 调用 `lift_circuit_breaker`（须到期）或 Root 调用 `force_lift_circuit_breaker`（无需到期）

## 入口检查矩阵

所有交易入口的前置检查一致性（经审计验证）:

| 检查项 | sell/buy | take | market | IOC/FOK/PostOnly | modify |
|--------|:-------:|:----:|:------:|:----------------:|:------:|
| `ensure_market_enabled` | ✓ | ✓ | ✓ | ✓ | ✓ |
| `ensure_kyc_requirement` | ✓ | ✓ | ✓ | ✓ | — |
| `can_insider_trade` | ✓ | ✓ | ✓ | ✓ | ✓ |
| `min_order_amount` | ✓ | — | ✓ | ✓ | ✓ |
| `check_price_deviation` | ✓ | — | — | ✓ | ✓ |
| 熔断检查 | via deviation | ✓ | ✓ | via deviation | via deviation |

`ensure_market_enabled` 包含: 全局暂停 → 实体存在 → 实体激活 → Token 启用 → 市场未关闭 → nex_enabled → 未暂停。

## 数据结构

### TradeOrder

```rust
pub struct TradeOrder<T: Config> {
    pub order_id: u64,
    pub entity_id: u64,
    pub maker: T::AccountId,
    pub side: OrderSide,              // Buy | Sell
    pub order_type: OrderType,        // Limit | Market | IOC | FOK | PostOnly
    pub token_amount: T::TokenBalance,
    pub filled_amount: T::TokenBalance,
    pub price: BalanceOf<T>,          // NEX per Token
    pub status: OrderStatus,          // Open | PartiallyFilled | Filled | Cancelled | Expired
    pub created_at: BlockNumber,
    pub expires_at: BlockNumber,
}
```

### MarketConfig

```rust
pub struct MarketConfig {
    pub nex_enabled: bool,        // 启用 NEX 交易（禁用时自动取消所有订单）
    pub min_order_amount: u128,   // 最小订单 Token 数量（所有订单类型均检查）
    pub order_ttl: u32,           // 订单有效期（区块数，≥10）
    pub paused: bool,             // 实体级暂停开关（pause_market / resume_market）
}
```

### PriceProtectionConfig

```rust
pub struct PriceProtectionConfig<Balance> {
    pub enabled: bool,                    // 默认 true
    pub max_price_deviation: u16,         // 限价单最大偏离（bps，默认 2000 = 20%）
    pub max_slippage: u16,                // 市价单最大滑点（bps，默认 500 = 5%）
    pub circuit_breaker_threshold: u16,   // 熔断阈值（bps，默认 5000 = 50%）
    pub min_trades_for_twap: u64,         // 启用 TWAP 保护的最小成交数（默认 100）
    pub circuit_breaker_active: bool,     // 熔断是否激活（到期后自动清理为 false）
    pub circuit_breaker_until: u32,       // 熔断到期区块
    pub initial_price: Option<Balance>,   // TWAP 冷启动参考价格
}
```

### MarketStatus

```rust
pub enum MarketStatus {
    Active,  // 活跃（暂停通过 MarketConfig.paused 控制）
    Closed,  // 已关闭（不可逆，所有订单已清退）
}
```

### TradeRecord

```rust
pub struct TradeRecord<T: Config> {
    pub trade_id: u64,
    pub order_id: u64,
    pub entity_id: u64,
    pub maker: T::AccountId,
    pub taker: T::AccountId,
    pub side: OrderSide,              // taker 视角
    pub token_amount: T::TokenBalance,
    pub price: BalanceOf<T>,
    pub nex_amount: BalanceOf<T>,
    pub block_number: BlockNumberFor<T>,
}
```

### DailyStats

```rust
pub struct DailyStats<Balance> {
    pub open_price: Balance,
    pub high_price: Balance,
    pub low_price: Balance,
    pub close_price: Balance,
    pub volume_nex: u128,     // 24h NEX 交易量
    pub trade_count: u32,     // 24h 成交笔数
    pub period_start: u32,    // 统计起始区块
}
```

## Extrinsics

### 用户交易 (signed)

| Index | 签名 | 说明 |
|:-----:|------|------|
| 0 | `place_sell_order(entity_id, token_amount, price)` | 限价卖单（锁定 Token，自动交叉撮合） |
| 1 | `place_buy_order(entity_id, token_amount, price)` | 限价买单（锁定 NEX，自动交叉撮合，退还价格改善差额） |
| 2 | `take_order(order_id, amount?)` | 吃单（原子交换，amount=None 全量成交） |
| 3 | `cancel_order(order_id)` | 取消自己的订单（退还锁定资产） |
| 12 | `market_buy(entity_id, token_amount, max_cost)` | 市价买入（`max_cost` 滑点保护） |
| 13 | `market_sell(entity_id, token_amount, min_receive)` | 市价卖出（`min_receive` 滑点保护） |
| 28 | `batch_cancel_orders(order_ids: BoundedVec<u64, 50>)` | 批量取消（解码阶段限制 ≤50） |
| 30 | `modify_order(order_id, new_price, new_amount)` | 改价/减量（仅 Open，不可增量） |
| 35 | `cancel_all_entity_orders(entity_id)` | 取消自己在指定实体的所有活跃订单 |
| 38 | `place_ioc_order(entity_id, side, token_amount, price)` | IOC 立即成交或取消 |
| 39 | `place_fok_order(entity_id, side, token_amount, price)` | FOK 全部成交或全部取消 |
| 40 | `place_post_only_order(entity_id, side, token_amount, price)` | Post-Only 仅挂单 |

### 市场管理 (Entity Owner)

| Index | 签名 | 说明 |
|:-----:|------|------|
| 4 | `configure_market(entity_id, nex_enabled, min_order_amount, order_ttl)` | 配置市场（禁用时自动取消所有订单） |
| 15 | `configure_price_protection(entity_id, enabled, max_deviation, max_slippage, threshold, min_trades)` | 配置价格保护参数 |
| 16 | `lift_circuit_breaker(entity_id)` | 熔断到期后手动解除 |
| 17 | `set_initial_price(entity_id, initial_price)` | TWAP 冷启动参考价（市场无真实成交时） |
| 26 | `pause_market(entity_id)` | 暂停实体市场（须未暂停） |
| 27 | `resume_market(entity_id)` | 恢复实体市场（须已暂停） |
| 33 | `set_kyc_requirement(entity_id, min_kyc_level)` | 设置市场最低 KYC 等级（0=无要求） |
| 34 | `close_market(entity_id)` | 永久关闭市场（取消所有订单，退还资产，不可逆） |

### 治理 / Root

| Index | 签名 | 说明 |
|:-----:|------|------|
| 23 | `force_cancel_order(order_id)` | 强制取消任意订单 |
| 32 | `global_market_pause(paused)` | 全局市场暂停/恢复 |
| 36 | `governance_configure_market(entity_id, nex_enabled, min_order_amount, order_ttl)` | 治理级配置（绕过 owner，禁用时自动取消订单） |
| 37 | `force_close_market(entity_id)` | 强制关闭市场（不可逆） |
| 41 | `governance_configure_price_protection(entity_id, enabled, max_deviation, max_slippage, cb_threshold, min_trades)` | 治理级价格保护配置 |
| 42 | `force_lift_circuit_breaker(entity_id)` | 强制解除熔断（无需等待到期） |

### 维护 (任何人)

| Index | 签名 | 说明 |
|:-----:|------|------|
| 29 | `cleanup_expired_orders(entity_id, max_count)` | 手动清理过期订单（≤100 笔） |

## 存储

### 订单簿

| 存储项 | 类型 | 说明 |
|--------|------|------|
| `NextOrderId` | `StorageValue<u64>` | 自增订单 ID |
| `Orders` | `StorageMap<u64 → TradeOrder>` | 订单主数据 |
| `EntitySellOrders` | `StorageMap<u64 → BoundedVec<u64, 1000>>` | 实体卖单索引 |
| `EntityBuyOrders` | `StorageMap<u64 → BoundedVec<u64, 1000>>` | 实体买单索引 |
| `UserOrders` | `StorageMap<AccountId → BoundedVec<u64, 100>>` | 用户活跃订单索引 |
| `MarketConfigs` | `StorageMap<u64 → MarketConfig>` | 实体市场配置 |

### 价格与 TWAP

| 存储项 | 类型 | 说明 |
|--------|------|------|
| `BestAsk` | `StorageMap<u64 → Balance>` | 最优卖价缓存 |
| `BestBid` | `StorageMap<u64 → Balance>` | 最优买价缓存 |
| `LastTradePrice` | `StorageMap<u64 → Balance>` | 最新成交价 |
| `TwapAccumulators` | `StorageMap<u64 → TwapAccumulator>` | TWAP 累积器（first_trade_block + 三周期双 checkpoint，v1 存储） |
| `PriceProtection` | `StorageMap<u64 → PriceProtectionConfig>` | 价格保护配置 |

### 交易历史与统计

| 存储项 | 类型 | 说明 |
|--------|------|------|
| `NextTradeId` | `StorageValue<u64>` | 自增成交 ID |
| `TradeRecords` | `StorageMap<u64 → TradeRecord>` | 成交记录 |
| `UserTradeHistory` | `StorageMap<AccountId → BoundedVec<u64, 200>>` | 用户成交历史（环形覆盖） |
| `EntityTradeHistory` | `StorageMap<u64 → BoundedVec<u64, 500>>` | 实体成交历史（环形覆盖） |
| `UserOrderHistory` | `StorageMap<AccountId → BoundedVec<u64, 200>>` | 用户已完结订单历史 |
| `MarketStatsStorage` | `StorageMap<u64 → MarketStats>` | 实体累计统计 |
| `EntityDailyStats` | `StorageMap<u64 → DailyStats>` | 实体日 K 线 |
| `GlobalStats` | `StorageValue<MarketStats>` | 全局累计统计 |

### 系统状态

| 存储项 | 类型 | 说明 |
|--------|------|------|
| `GlobalMarketPaused` | `StorageValue<bool>` | 全局暂停开关（Root） |
| `MarketStatusStorage` | `StorageMap<u64 → MarketStatus>` | Active / Closed |
| `MarketKycRequirement` | `StorageMap<u64 → u8>` | 市场最低 KYC 等级 |
| `OnIdleCursor` | `StorageValue<u64>` | on_idle 扫描游标 |

## Events

| 事件 | 字段 | 触发时机 |
|------|------|----------|
| `OrderCreated` | order_id, entity_id, maker, side, token_amount, price | 限价单 / PostOnly 创建 |
| `OrderFilled` | order_id, entity_id, maker, taker, filled_amount, total_next | 订单被成交（含部分） |
| `OrderCancelled` | order_id, entity_id | 用户取消订单 |
| `OrderModified` | order_id, new_price, new_amount | 改价/减量 |
| `OrderForceCancelled` | order_id | Root 强制取消 |
| `MarketConfigured` | entity_id | 市场配置变更 |
| `MarketOrderExecuted` | entity_id, trader, side, filled_amount, total_next | 市价单 / IOC / FOK 执行 |
| `TradeExecuted` | trade_id, order_id, entity_id, maker, taker, side, token_amount, price, nex_amount | 成交记录 |
| `TwapUpdated` | entity_id, new_price, twap_1h, twap_24h, twap_7d | 成交后 TWAP 更新 |
| `CircuitBreakerTriggered` | entity_id, current_price, twap_7d, deviation_bps, until_block | 熔断触发 |
| `CircuitBreakerLifted` | entity_id | 熔断解除（手动 / 到期自动 / Root 强制） |
| `PriceProtectionConfigured` | entity_id, enabled, max_deviation, max_slippage | 价格保护配置 |
| `InitialPriceSet` | entity_id, initial_price | 初始价格设置 |
| `MarketPausedEvent` | entity_id | 实体市场暂停 |
| `MarketResumedEvent` | entity_id | 实体市场恢复 |
| `MarketClosed` | entity_id, orders_cancelled | Owner 关闭市场 |
| `MarketForceClosed` | entity_id, orders_cancelled | Root 强制关闭 |
| `AllEntityOrdersCancelled` | entity_id, user, cancelled_count | 用户实体订单全部取消 |
| `KycRequirementSet` | entity_id, min_kyc_level | KYC 准入等级设置 |
| `ExpiredOrdersCleaned` | entity_id, count, cleaner | 手动清理过期订单 |
| `ExpiredOrdersAutoCleaned` | count | on_idle 自动清理过期订单 |
| `GlobalMarketPauseToggled` | paused | 全局暂停状态变更 |
| `BatchOrdersCancelled` | cancelled_count, failed_count | 批量取消完成 |

## Errors

| 错误 | 说明 |
|------|------|
| `EntityNotFound` | 实体不存在 |
| `NotEntityOwner` | 不是实体所有者 |
| `TokenNotEnabled` | 实体代币未启用 |
| `MarketNotEnabled` | 市场未配置或未启用 |
| `OrderNotFound` | 订单不存在 |
| `NotOrderOwner` | 不是订单所有者 |
| `OrderClosed` | 订单已关闭或已过期 |
| `InsufficientBalance` | NEX 余额不足 |
| `InsufficientTokenBalance` | Token 余额不足 |
| `AmountTooSmall` | 数量为零 |
| `ZeroPrice` | 价格为零 |
| `OrderBookFull` | 订单簿已满（1000/边） |
| `UserOrdersFull` | 用户活跃订单数已满（100） |
| `CannotTakeOwnOrder` | 不能吃自己的单 |
| `ArithmeticOverflow` | 算术溢出 |
| `NoOrdersAvailable` | 没有可用的对手方订单 |
| `SlippageExceeded` | 滑点超限 |
| `PriceDeviationTooHigh` | 价格偏离参考价过大 |
| `MarketCircuitBreakerActive` | 市场处于熔断状态 |
| `InvalidBasisPoints` | 基点参数无效（>10000） |
| `EntityNotActive` | 实体未激活（Banned / Closed） |
| `OrderTtlTooShort` | 订单 TTL < 10 |
| `InsiderTradingRestricted` | 内幕人员黑窗口期禁止交易 |
| `EntityLocked` | 实体已被全局锁定 |
| `MarketPaused` | 实体市场已暂停 |
| `GlobalMarketPausedError` | 全局市场已暂停 |
| `OrderAmountBelowMinimum` | 订单数量低于 `min_order_amount` |
| `CircuitBreakerNotActive` | 熔断未激活 |
| `ModifyAmountExceedsOriginal` | 改单数量不得超过原始数量 |
| `InvalidOrderStatus` | 订单状态无效 |
| `TooManyOrders` | 清理数量超限（>100） |
| `InsufficientKycLevel` | KYC 等级不足 |
| `MarketAlreadyClosed` | 市场已永久关闭 |
| `FokNotFullyFillable` | FOK 无法全部成交 |
| `PostOnlyWouldMatch` | Post-Only 会立即撮合 |
| `InitialPriceAlreadySet` | 已有真实成交，不可重设初始价格 |
| `MarketNotPaused` | 市场未暂停（resume 时） |

## Runtime 配置

```rust
impl pallet_entity_market::Config for Runtime {
    type RuntimeEvent = RuntimeEvent;
    type Currency = Balances;
    type Balance = u128;
    type TokenBalance = u128;
    type EntityProvider = EntityRegistry;
    type TokenProvider = EntityToken;
    type DisclosureProvider = EntityDisclosure;
    type KycProvider = EntityKyc;
    type PricingProvider = PriceOracle;
    type DefaultOrderTTL = ConstU32<100800>;     // 7 天
    type MaxActiveOrdersPerUser = ConstU32<100>;
    type BlocksPerHour = ConstU32<600>;          // 6s/block
    type BlocksPerDay = ConstU32<14400>;
    type BlocksPerWeek = ConstU32<100800>;
    type CircuitBreakerDuration = ConstU32<600>; // 1h
    type MaxTradeHistoryPerUser = ConstU32<200>;
    type MaxOrderHistoryPerUser = ConstU32<200>;
}
```

`integrity_test` 校验: `DefaultOrderTTL >= 10`, `BlocksPerHour > 0`, `BlocksPerDay > BlocksPerHour`, `BlocksPerWeek > BlocksPerDay`, `CircuitBreakerDuration > 0`。

## 查询接口

```rust
// 订单簿
fn get_order_book_depth(entity_id, depth) -> OrderBookDepth;
fn get_order_book_snapshot(entity_id) -> (asks, bids);
fn get_sell_orders(entity_id) -> Vec<TradeOrder>;  // 过滤过期
fn get_buy_orders(entity_id) -> Vec<TradeOrder>;   // 过滤过期
fn get_user_orders(user) -> Vec<TradeOrder>;

// 市场信息
fn get_market_summary(entity_id) -> MarketSummary;
fn get_best_prices(entity_id) -> (Option<Balance>, Option<Balance>);
fn get_spread(entity_id) -> Option<Balance>;
fn get_market_status(entity_id) -> MarketStatus;
fn get_kyc_requirement(entity_id) -> u8;

// TWAP
fn calculate_twap(entity_id, period: TwapPeriod) -> Option<Balance>;

// 分页历史
fn get_user_trade_history(user, page, page_size) -> Vec<TradeRecord>;
fn get_entity_trade_history(entity_id, page, page_size) -> Vec<TradeRecord>;
fn get_user_order_history(user, page, page_size) -> Vec<TradeOrder>;

// 统计
fn get_daily_stats(entity_id) -> DailyStats;
fn get_global_stats() -> MarketStats;
```

## EntityTokenPriceProvider

本模块实现 `EntityTokenPriceProvider` trait，供其他模块查询代币价格:

| 函数 | 返回 | 说明 |
|------|------|------|
| `get_token_price(entity_id)` | `Option<Balance>` | 优先级: 1h TWAP → LastTradePrice → initial_price |
| `get_token_price_usdt(entity_id)` | `Option<u64>` | token_nex × nex_usdt / 10^12（精度 10^6） |
| `token_price_confidence(entity_id)` | `u8` | 0~95: TWAP+活跃=95, TWAP=80, LastTrade=65, initial=35, stale≤25 |
| `is_token_price_stale(entity_id, max_age)` | `bool` | 最后成交距今是否超过 max_age 区块 |

## 安全机制

| 机制 | 说明 |
|------|------|
| **原子交换** | 单交易内完成 Token ↔ NEX 双向转移，不可部分完成 |
| **价格偏离保护** | 限价单/改单不得偏离 TWAP 或 initial_price 超过 `max_price_deviation` |
| **异常价格过滤** | TWAP 累积时偏离上次价格 >100% 的成交价限幅至 ±50% |
| **熔断机制** | 所有交易入口强制检查；到期后自动清理存储状态并发出事件 |
| **滑点保护** | 市价单 `max_cost` / `min_receive`；market_sell 最终滑点双重检查 |
| **自吃单防护** | `do_cross_match` / `do_market_buy` / `do_market_sell` / `take_order` 四处均跳过 |
| **KYC 准入** | 可配置最低 KYC 等级，不达标禁止交易 |
| **内幕交易限制** | 黑窗口期禁止交易和改单（DisclosureProvider） |
| **改单安全** | `modify_order` 验证市场状态 + 内幕限制 + 价格偏离 + 不可增量 |
| **过期订单过滤** | `get_sorted_orders` 撮合前过滤已过期但未清理的订单 |
| **PostOnly 动态检查** | 使用 `calculate_best_ask/bid` 动态计算而非缓存，防过期订单误判 |
| **on_idle 权重** | `consumed_weight` 追踪 ref_time + proof_size，预算不足时停止 |
| **禁用市场退资产** | `nex_enabled=false` 自动取消所有订单退还资产（owner + governance 一致） |

## on_idle 自动清理

```
每块 on_idle(remaining_weight):
  ├── 从 OnIdleCursor 开始扫描（每批 ≤200 ID）
  ├── 同时检查 ref_time 和 proof_size 预算
  ├── 每块最多清理 20 个过期订单
  ├── 对每个过期订单:
  │     ├── 退还锁定资产（Token unreserve / NEX unreserve）
  │     ├── 状态 → Expired，从订单簿 + 用户索引移除
  │     └── 添加到已完结订单历史
  ├── 更新受影响实体的 BestAsk/BestBid 缓存
  ├── 发出 ExpiredOrdersAutoCleaned 事件
  ├── 游标前进（到达末尾归零循环）
  └── 返回 consumed_weight（精确追踪）
```

## 已知技术债

| 项目 | 状态 | 说明 |
|------|------|------|
| Weight benchmarking | 🟡 占位 | 所有 extrinsic 使用硬编码占位值 |
| 订单簿排序 | 🟡 O(N log N) | 每次撮合全量排序，大订单簿时性能待优化 |

## 版本历史

| 版本 | 日期 | 变更 |
|------|------|------|
| v2.5.0 | 2026-06-12 | **TWAP 修复**: 参考价 initial_price → TWAP 切换不可达 bug；TWAP 数据充足性改为按 `first_trade_block` 历史覆盖判断；快照改为双 checkpoint 轮换使窗口真实覆盖命名周期；存储 v0→v1 迁移 |
| v2.4.0 | 2026-03-06 | **审计 R11**: governance_configure_market 禁用一致性；熔断到期自动清理存储；market_buy/sell min_order_amount |
| v2.3.0 | 2026-03-06 | **审计 R10**: IOC/FOK/PostOnly min_order_amount；configure_market 禁用取消订单；governance_configure_price_protection(41)；force_lift_circuit_breaker(42)；移除 MarketStatus::Paused；on_idle 事件；PostOnly 动态计算 |
| v2.2.0 | 2026-03-05 | **审计 R9**: on_idle consumed_weight + proof_size；modify_order 市场状态检查；batch_cancel BoundedVec；移除手续费 |
| v2.1.0 | 2026-03-05 | **审计 R7+R8**: 自吃单防护 market_buy/sell；熔断全入口检查；on_idle 游标扫描；resume 对称性 |
| v2.0.0 | 2026-03-04 | USDT 通道移除；IOC/FOK/PostOnly；close/force_close；KYC；治理配置；日统计；分页查询 |
| v1.2.0 | 2026-03-04 | pause/resume, batch_cancel, cleanup, modify, force_cancel, global_pause |
| v1.1.0 | 2026-02-26 | EntityTokenPriceProvider（含 USDT 间接换算） |
| v1.0.0 | 2026-02-24 | 架构重构: shop_id → entity_id |
| v0.5.0 | 2026-02-01 | 三周期 TWAP 预言机 + 熔断 |
| v0.3.0 | 2026-02-01 | 市价单 + 滑点保护 |
| v0.1.0 | 2026-02-01 | NEX 限价单 |

## 相关模块

- [pallet-entity-common](../common/) — 共享 Trait（EntityProvider, EntityTokenProvider, DisclosureProvider, KycProvider, PricingProvider）
- [pallet-entity-registry](../registry/) — 实体管理（EntityProvider 实现方）
- [pallet-entity-token](../token/) — 实体代币（EntityTokenProvider 实现方）
- [pallet-entity-disclosure](../disclosure/) — 信息披露（DisclosureProvider 实现方）
- [pallet-entity-kyc](../kyc/) — KYC 管理（KycProvider 实现方）
