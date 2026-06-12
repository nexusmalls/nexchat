# pallet-nex-market

NEX/USDT P2P 订单簿交易市场 — 无做市商模型，任何人可挂单/吃单。

## 概述

`pallet-nex-market` 是 Nexus 链上唯一的 NEX ↔ USDT 交易市场，同时也是全链的价格预言机数据源。

**交易对**: NEX（链上原生代币，精度 10^12）↔ USDT（TRC20 链下支付，精度 10^6）

| 特性 | 说明 |
|------|------|
| **订单簿** | 限价卖单 + 限价买单，支持部分成交、原子改价、改量、最低成交量 |
| **USDT 通道** | TRC20 链下支付 + OCW 三阶段自动验证 + 自动结算 |
| **多档判定** | Exact / Overpaid / Underpaid / SeverelyUnderpaid / Invalid |
| **买家保证金** | 按 USDT 金额计算 NEX 保证金，梯度没收，动态汇率可治理调整 |
| **补付窗口** | 少付 50%~99.5% 给予 2h 补付时间，避免网络延迟误判 |
| **TWAP 预言机** | 1h / 24h / 7d 三周期时间加权平均价，`on_idle` 每区块推进 |
| **价格保护** | 挂单/吃单偏离检查 + 7d TWAP 熔断机制 |
| **seed_liquidity** | 瀑布式定价 + 溢价 + 四层防御，冷启动引流 |
| **tx_hash 防重放** | 同一 TRON 交易不能用于多笔交易的支付证明 |
| **交易手续费** | 可治理配置（最高 10%），结算时从 NEX 扣除转入国库 |
| **争议仲裁** | 双方举证 + 争议窗口锚定终态时间 + 管理员根据链下证据裁决 |
| **用户封禁** | 管理员可封禁用户并自动取消其挂单 |
| **卖家确认收款** | OCW 故障时卖家可手动确认完成交易 |
| **紧急暂停** | 管理员一键暂停/恢复 + 队列满自动暂停保护 |
| **批量管理** | 管理员可批量强制结算/取消交易（单次最多 20 笔） |
| **过期订单 GC** | `on_idle` 自动清理过期订单，释放锁定资产 |

---

## 交易流程

### 卖 NEX（卖家锁 NEX，收 USDT）

```text
卖家                         买家                          OCW
  │                           │                             │
  │ place_sell_order           │                             │
  │ ──→ 锁 NEX + 提供 TRON 地址                              │
  │                           │                             │
  │                  reserve_sell_order                      │
  │                  ──→ 提供买家 TRON 地址                   │
  │                  ──→ 锁保证金（或免保证金）                │
  │                  ──→ 创建 UsdtTrade (AwaitingPayment)     │
  │                           │                             │
  │                  链下转 USDT → confirm_payment            │
  │                  ──→ 状态 → AwaitingVerification          │
  │                           │                             │
  │                           │          TronGrid 验证       │
  │                           │     ←── submit_ocw_result    │
  │                           │     ──→ 自动结算（Exact/Overpaid）
  │                           │     ──→ 释放 NEX 给买家（扣手续费）
  │                           │     ──→ 退还保证金             │
  │                           │                             │
  │ [备用] seller_confirm_received                           │
  │ ──→ 卖家手动确认收款，跳过 OCW 验证                       │
```

### 买 NEX（买家挂单，卖家接单）

```text
买家                         卖家                          OCW
  │                           │                             │
  │ place_buy_order            │                             │
  │ ──→ 提供买家 TRON 地址      │                             │
  │ ──→ 预锁保证金             │                             │
  │                           │                             │
  │                  accept_buy_order                        │
  │                  ──→ 提供卖家 TRON 地址                   │
  │                  ──→ 锁卖家 NEX                          │
  │                  ──→ 按比例分配预锁保证金                  │
  │                  ──→ 创建 UsdtTrade (AwaitingPayment)     │
  │                           │                             │
  │ 链下转 USDT → confirm_payment                            │
  │                           │      ←── submit_ocw_result   │
  │                           │      ──→ 自动结算              │
```

### 少付补付流程（50% ~ 99.5%）

```text
OCW 检测到少付
  │
  ▼
submit_ocw_result → UnderpaidPending → 开启补付窗口 (2h)
  │
  ├─ 窗口内 OCW 持续扫描 TronGrid
  │   └─ submit_underpaid_update（仅接受递增金额）
  │       ├─ 累计 ≥ 99.5% → 自动结算，直接完成交易
  │       └─ 累计仍 < 99.5% → 等待窗口到期
  │
  └─ 窗口到期 → finalize_underpaid（终裁）
      └─ 按最终比例释放 NEX + 梯度没收保证金
```

### OCW 预检兜底

```text
AwaitingPayment 超过 50% 超时期
  │
  ▼
OCW 预检扫描 → 检测到 USDT 已到账但买家忘记 confirm
  │
  ▼
auto_confirm_payment → 一步完成：确认付款 + 验证 + 自动结算
  ├─ Exact/Overpaid → 直接完成交易 (Completed)
  ├─ Underpaid → UnderpaidPending（进入补付窗口）
  └─ SeverelyUnderpaid/Invalid → 按少付结算
```

### 争议仲裁流程

```text
交易进入终态（Completed 或 Refunded）
  │
  ▼                   ┌── Refunded 交易需 payment_confirmed=true
争议窗口内 ←──────────┤
  │                   └── 窗口锚定 completed_at（非 timeout_at）
  ▼
买家/卖家 → dispute_trade (提交 IPFS 证据 CID)
  │
  ▼
对方 → submit_counter_evidence (提交反驳证据，一次性不可覆盖)
  │
  ▼
管理员审核 → resolve_dispute
  ├─ ReleaseToBuyer → 国库补偿 NEX 给买家（仅 Refunded 交易）
  └─ RefundToSeller → 维持原判，关闭争议
```

---

## Extrinsics

### 用户操作

| # | 调用 | 权限 | 参数 | 说明 |
|---|------|------|------|------|
| 0 | `place_sell_order` | 签名 | `nex_amount`, `usdt_price`, `tron_address`, `min_fill_amount?` | 挂卖单：锁定 NEX，校验 TRON 地址/价格偏离/数量上下限/封禁状态 |
| 1 | `place_buy_order` | 签名 | `nex_amount`, `usdt_price`, `buyer_tron_address` | 挂买单：预锁保证金，校验价格偏离/数量上下限/封禁状态 |
| 2 | `cancel_order` | Owner | `order_id` | 取消订单：退还未成交的锁定 NEX 或保证金 |
| 3 | `reserve_sell_order` | 签名 | `order_id`, `amount?`, `buyer_tron_address` | 吃卖单：校验过期/价格偏离/熔断/暂停/封禁/最低成交量，锁保证金或免保证金 |
| 4 | `accept_buy_order` | 签名 | `order_id`, `amount?`, `tron_address` | 接买单：校验过期/价格偏离/熔断/暂停/封禁，锁卖家 NEX，按比例分配保证金 |
| 5 | `confirm_payment` | 买家 | `trade_id` | 确认已付款：AwaitingPayment → AwaitingVerification，设置 `payment_confirmed` |
| 6 | `process_timeout` | 参与方/Admin | `trade_id` | 分阶段超时处理（详见下方） |
| 8 | `claim_verification_reward` | 任何人 | `trade_id` | 手动结算兜底：仅当 submit_ocw_result 自动结算异常时使用 |
| 17 | `finalize_underpaid` | 任何人 | `trade_id` | 补付窗口到期后终裁 |
| 22 | `dispute_trade` | 参与方 | `trade_id`, `evidence_cid` | 对终态交易发起争议（Refunded 需 `payment_confirmed`） |
| 25 | `update_order_price` | Owner | `order_id`, `new_price` | 原子改价（买单自动重算保证金差额） |
| 27 | `seller_confirm_received` | 卖家 | `trade_id` | 卖家手动确认收款（仅 AwaitingVerification / UnderpaidPending） |
| 30 | `submit_counter_evidence` | 对方 | `trade_id`, `evidence_cid` | 提交反驳证据（一次性不可覆盖） |
| 31 | `update_order_amount` | Owner | `order_id`, `new_amount` | 修改订单数量（无活跃交易，不低于已成交量） |

### OCW 操作（Unsigned）

| # | 调用 | 参数 | 说明 |
|---|------|------|------|
| 7 | `submit_ocw_result` | `trade_id`, `actual_amount`, `tx_hash?` | OCW 结果先持久化 → Exact/Overpaid 自动结算 → Underpaid 进补付窗口 → SeverelyUnderpaid/Invalid 按少付结算。结算失败时 OCW 结果保留供手动恢复 |
| 15 | `auto_confirm_payment` | `trade_id`, `actual_amount`, `tx_hash?` | 预检兜底：一步完成确认 + 验证 + 自动结算 |
| 16 | `submit_underpaid_update` | `trade_id`, `new_actual_amount` | 补付窗口内更新累计金额（仅递增，达 99.5% 自动结算） |

> 三个 unsigned extrinsic 均拒绝 `TransactionSource::External`，仅接受 Local / InBlock 来源。`propagate(false)` 阻止广播。ValidateUnsigned 层增加安全边界：`actual_amount > expected × 10` 时拒绝。

### 管理操作（MarketAdmin = TreasuryCouncil 2/3）

| # | 调用 | 参数 | 说明 |
|---|------|------|------|
| 9 | `configure_price_protection` | `enabled`, `max_deviation`, `cb_threshold`, `min_trades` | 配置价格保护参数 |
| 10 | `set_initial_price` | `initial_price` | 设置初始基准价格（冷启动） |
| 11 | `lift_circuit_breaker` | — | 手动解除已到期的熔断 |
| 13 | `fund_seed_account` | `amount` | 国库 → 种子账户转账 |
| 14 | `seed_liquidity` | `order_count`, `usdt_override?` | 批量挂免保证金卖单（瀑布式定价 + 溢价） |
| 18 | `force_pause_market` | — | 紧急暂停市场 |
| 19 | `force_resume_market` | — | 恢复市场交易 |
| 20 | `force_settle_trade` | `trade_id`, `actual_amount`, `resolution` | 强制结算单笔交易 |
| 21 | `force_cancel_trade` | `trade_id` | 强制取消单笔交易（退 NEX + 退保证金，不没收） |
| 23 | `resolve_dispute` | `trade_id`, `resolution` | 裁决争议：ReleaseToBuyer / RefundToSeller |
| 24 | `set_trading_fee` | `fee_bps` | 设置交易手续费率（最大 1000 bps = 10%） |
| 26 | `update_deposit_exchange_rate` | `new_rate` | 更新保证金动态汇率 |
| 28 | `ban_user` | `account` | 封禁用户 + 自动取消其所有无活跃交易的挂单 |
| 29 | `unban_user` | `account` | 解封用户 |
| 32 | `batch_force_settle` | `trade_ids(≤20)`, `actual_amount`, `resolution` | 批量强制结算（动态权重） |
| 33 | `batch_force_cancel` | `trade_ids(≤20)` | 批量强制取消（动态权重） |

---

## process_timeout 分阶段策略

| 交易状态 | 超时条件 | 处理 |
|----------|---------|------|
| **AwaitingPayment** | `now > timeout_at` | 退还卖家 NEX，没收买家保证金，回滚订单 |
| **AwaitingVerification** | `now > timeout_at + VerificationGracePeriod` | 先检查 OCW 结果：有结果按正常结算，无结果退款 + 没收 + `VerificationTimeoutRefunded` 事件 |
| **UnderpaidPending** | `now > underpaid_deadline` | 读取最终 OCW 金额做终裁，无结果走通用超时退款 |

调用者限制：仅交易参与方（buyer/seller）或 MarketAdmin（Root/Council）可触发。

---

## 多档判定 & 保证金没收

### 付款金额判定

| 实际 / 应付比例 | 判定结果 | NEX 释放 | 保证金 |
|----------------|----------|----------|--------|
| ≥ 100.5% | Overpaid | 全额释放（扣手续费） | 退还 |
| 99.5% ~ 100.5% | Exact | 全额释放（扣手续费） | 退还 |
| 50% ~ 99.5% | Underpaid | 进入补付窗口(2h) | 待定 |
| < 50% | SeverelyUnderpaid | 按比例释放 | 没收 |
| = 0 | Invalid | 不释放（NEX 退还卖家） | 没收 |

### 保证金梯度没收

| 最终付款比例 | 没收比例 |
|-------------|---------|
| ≥ 99.5% | 0% |
| 95% ~ 99.5% | 20% |
| 80% ~ 95% | 50% |
| < 80% | 100% |

### 少付 NEX 释放计算

```text
nex_to_release = nex_amount × payment_ratio / 10000
nex_to_refund  = nex_amount - nex_to_release  (退还卖家)
```

> 比例计算使用 `compute_payment_ratio_bps()` 返回 u32，防止超付 >6.55 倍时 u16 截断。

---

## 交易手续费

结算时从锁定的 NEX 中扣除手续费，通过 `repatriate_reserved` 转入国库：

```text
fee_amount = nex_amount × fee_bps / 10000
nex_to_buyer = nex_amount - fee_amount
```

- 默认 0 bps（关闭），管理员通过 `set_trading_fee` 配置
- 最大 1000 bps（10%）
- 仅在全额付款（Exact/Overpaid）结算时收取

---

## 争议仲裁机制

### 争议准入条件

| 条件 | 说明 |
|------|------|
| 交易状态 | `Completed` 或 `Refunded` |
| 付款确认 | `Refunded` 交易要求 `payment_confirmed = true`（防未付款恶意争议） |
| 争议窗口 | `now ≤ completed_at + DisputeWindowBlocks`（锚定实际终态时间） |
| 唯一性 | 同一交易不可重复争议 |

### 争议流程

1. **发起争议** `dispute_trade` — 买方或卖方提交 IPFS 证据 CID
2. **反驳证据** `submit_counter_evidence` — 对方提交反驳（一次性不可覆盖）
3. **管理员裁决** `resolve_dispute`:
   - `ReleaseToBuyer` → 仅 `Refunded` 交易从国库补偿 NEX 给买家（`Completed` 交易已结算，不重复支付）
   - `RefundToSeller` → 维持原判

### 国库补偿安全

- 补偿金额 = `min(trade.nex_amount, treasury_balance - existential_deposit)`
- 国库余额不足时尽力补偿，不阻断裁决流程

---

## 市场暂停机制

### 手动暂停

管理员通过 `force_pause_market` / `force_resume_market` 控制。

| 操作 | 暂停时 |
|------|--------|
| `place_sell_order` / `place_buy_order` | 禁止 |
| `reserve_sell_order` / `accept_buy_order` | 禁止 |
| `confirm_payment` | 禁止 |
| `update_order_price` | 禁止 |
| `process_timeout` / `claim_verification_reward` | 正常 |
| `cancel_order` | 正常 |
| 管理操作 | 正常 |

### 队列满自动暂停

当 `PendingUsdtTrades` 队列容量超过 `QueueFullThresholdBps`（默认 80%）时自动暂停，恢复需管理员手动调用 `force_resume_market`。

---

## OCW 三阶段工作流

### Hooks

- **`on_idle`**：
  - 推进 TWAP 累积器（用 `last_price` 填充空白区间）
  - 过期订单 GC（每次最多 `MaxExpiredOrdersPerBlock` 笔）
  - UsedTxHashes TTL 清理（cursor-based，每次最多 10 条）
  - 刷新最优价格
- **`offchain_worker`**：执行链下三阶段验证

### 三阶段验证

| 阶段 | 扫描队列 | 触发条件 | 链上动作 |
|------|---------|---------|---------|
| 1. 正常验证 | `PendingUsdtTrades` | AwaitingVerification | `submit_ocw_result` |
| 2. 补付扫描 | `PendingUnderpaidTrades` | UnderpaidPending | `submit_underpaid_update` |
| 3. 预检兜底 | `AwaitingPaymentTrades` | AwaitingPayment + 超 50% 超时 | `auto_confirm_payment` |

### OCW ↔ Sidecar 通信

OCW 将验证结果写入 offchain local storage（PERSISTENT），外部 sidecar 服务读取后提交 unsigned 交易：

| 存储键前缀 | 值 | 对应 extrinsic |
|-----------|------|---------------|
| `nex_market_ocw::{trade_id}` | `(bool, u64, Vec<u8>)` | `submit_ocw_result` |
| `nex_market_auto::{trade_id}` | `(bool, u64, Vec<u8>)` | `auto_confirm_payment` |
| `nex_market_undp::{trade_id}` | `(bool, u64)` | `submit_underpaid_update` |

---

## tx_hash 防重放

1. `submit_ocw_result` 和 `auto_confirm_payment` 接受可选 `tx_hash` 参数
2. ValidateUnsigned 层和 dispatch 层均检查 `UsedTxHashes`
3. 验证通过后写入 `UsedTxHashes` 并记录区块号
4. `on_idle` 按 `TxHashTtlBlocks`（默认 7 天）TTL 清理过期记录

---

## TWAP 预言机

### 累积器机制

每笔成交调用 `on_trade_completed(trade_price)` 更新累积器：

1. **异常价格过滤**：偏离 > 100% 时钳制到 ±50%
2. **累积价格推进**：`cumulative += last_price × blocks_elapsed`
3. **记录 `first_trade_block`**：首笔真实成交区块（历史覆盖起点）
4. **双 checkpoint 轮换**：`curr` 距今 >= 1 个周期时 `prev ← curr, curr ← 当前`

### checkpoint 轮换周期（成交 + on_idle 共同推进）

| 周期 | 轮换间隔 | 活跃市场 prev 年龄 |
|------|---------|------------------|
| `hour_prev/curr` | `BlocksPerHour` (~1h) | 1~2 小时 |
| `day_prev/curr` | `BlocksPerDay` (~24h) | 1~2 天 |
| `week_prev/curr` | `BlocksPerWeek` (~7d) | 1~2 周 |

### TWAP 计算

```text
twap = (current_cumulative - checkpoint.cumulative_price) / (current_block - checkpoint.block_number)
```

选取"距今至少一个周期、且最接近一个周期"的 checkpoint（`curr` 满周期用 `curr`，否则用 `prev`），保证测量窗口真实覆盖命名周期；市场早期退化为最旧 checkpoint（短窗口尽力估计）。

### 对外接口

本模块实现 `pallet_trading_common::PriceOracle` trait：

| 方法 | 说明 |
|------|------|
| `get_twap(window)` | 获取 1h/24h/7d TWAP（精度 10^6） |
| `get_last_trade_price()` | 获取最新成交价 |
| `is_price_stale(max_age)` | 价格是否超过指定区块数未更新 |
| `get_trade_count()` | 累计交易数 |

---

## 价格保护

### 偏离检查（挂单 + 吃单）

```text
check_price_deviation(usdt_price):
  1. 检查市场暂停 → MarketIsPaused
  2. 检查熔断是否激活 → MarketCircuitBreakerActive
  3. 获取参考价：TWAP 数据充足 → 1h TWAP，否则 → initial_price
  4. 计算偏离 bps = |price - ref_price| × 10000 / ref_price
  5. 偏离 > max_price_deviation → PriceDeviationTooHigh
```

**TWAP 数据充足条件** (存储 v3)：`trade_count >= min_trades_for_twap` 且真实成交历史 >= 1 小时（`current_block - first_trade_block >= BlocksPerHour`）。

> v3 修复: 历史覆盖时长从首笔真实成交（`first_trade_block`）起算，而非快照年龄。
> 旧实现要求快照"距今 >= 周期"，但快照随成交和 on_idle 滚动刷新，
> 导致参考价永远停留在 `initial_price`、无法切换到 TWAP。

### 熔断机制

每笔成交后检查 `check_circuit_breaker(trade_price)`：
- 条件：`|trade_price - 7d TWAP| / 7d TWAP > circuit_breaker_threshold`
- 触发：暂停所有交易 `CircuitBreakerDuration` 个区块
- 解除：自动到期 或 `lift_circuit_breaker`（需熔断已到期）

---

## seed_liquidity 流动性引导

### 定价策略

```text
ref_price = get_seed_reference_price()
seed_price = ref_price × (1 + SeedPricePremiumBps / 10000)
nex_per_order = usdt_amount × 10^12 / seed_price
```

| 市场阶段 | 条件 | 基准价来源 |
|----------|------|-----------|
| 成熟期 | `trade_count ≥ min_trades_for_twap` | 7d TWAP |
| 过渡期 | `trade_count ≥ 30` | `max(24h TWAP, InitialPrice)` |
| 冷启动 | `trade_count < 30` | InitialPrice |

### 四层防御

| 层级 | 机制 | 参数 |
|------|------|------|
| **L0 定价** | 瀑布式基准价 + 溢价下限 | 20% 溢价 |
| **L1 资金隔离** | 独立种子账户，需 `fund_seed_account` 显式注资 | — |
| **L2 防 Grief** | 单笔上限 + 每账户仅 1 笔活跃免保证金 + 短超时 | 100 NEX / 1h |
| **L3 防 Sybil** | 完成首单后标记 `CompletedBuyers`，不再免保证金 | — |

---

## 用户封禁

| 操作 | 说明 |
|------|------|
| `ban_user` | 封禁用户 + 自动取消其所有无活跃交易的挂单（退还 NEX/保证金） |
| `unban_user` | 解封用户 |

封禁后影响范围：`place_sell_order`、`place_buy_order`、`reserve_sell_order`、`accept_buy_order`、`confirm_payment` 均被拒绝。

---

## 数据结构

### Order

| 字段 | 类型 | 说明 |
|------|------|------|
| `order_id` | `u64` | 订单 ID |
| `maker` | `AccountId` | 挂单者 |
| `side` | `OrderSide` | `Buy` / `Sell` |
| `nex_amount` | `Balance` | NEX 数量（精度 10^12） |
| `filled_amount` | `Balance` | 已成交 NEX 数量 |
| `usdt_price` | `u64` | 每 NEX 的 USDT 单价（精度 10^6） |
| `tron_address` | `Option<TronAddress>` | 卖单 = 卖家收款地址，买单 = 买家付款地址 |
| `status` | `OrderStatus` | `Open` / `PartiallyFilled` / `Filled` / `Cancelled` / `Expired` |
| `created_at` | `BlockNumber` | 创建区块 |
| `expires_at` | `BlockNumber` | 过期区块 |
| `buyer_deposit` | `Balance` | 预锁保证金（仅买单） |
| `deposit_waived` | `bool` | 是否免保证金（仅 seed_liquidity 卖单） |
| `min_fill_amount` | `Balance` | 最低成交量（0 表示无限制） |

### UsdtTrade

| 字段 | 类型 | 说明 |
|------|------|------|
| `trade_id` | `u64` | 交易 ID |
| `order_id` | `u64` | 关联订单 |
| `seller` / `buyer` | `AccountId` | 卖方 / 买方 |
| `nex_amount` | `Balance` | NEX 数量 |
| `usdt_amount` | `u64` | 应付 USDT 金额 |
| `seller_tron_address` | `TronAddress` | 卖家 TRON 收款地址 |
| `buyer_tron_address` | `Option<TronAddress>` | 买家 TRON 付款地址 |
| `status` | `UsdtTradeStatus` | 见状态机 |
| `created_at` | `BlockNumber` | 创建区块 |
| `timeout_at` | `BlockNumber` | 超时区块 |
| `buyer_deposit` | `Balance` | 买家保证金 |
| `deposit_status` | `BuyerDepositStatus` | `None` / `Locked` / `Released` / `Forfeited` |
| `first_verified_at` | `Option<BlockNumber>` | 首次 OCW 验证区块（少付场景） |
| `first_actual_amount` | `Option<u64>` | 首次检测金额 |
| `underpaid_deadline` | `Option<BlockNumber>` | 补付截止区块 |
| `completed_at` | `Option<BlockNumber>` | 交易终态时间（用于争议窗口锚定） |
| `payment_confirmed` | `bool` | 买家是否已确认/OCW 检测到付款 |

### TradeDispute

| 字段 | 类型 | 说明 |
|------|------|------|
| `trade_id` | `u64` | 交易 ID |
| `initiator` | `AccountId` | 发起者 |
| `status` | `DisputeStatus` | `Open` / `ResolvedForBuyer` / `ResolvedForSeller` |
| `created_at` | `BlockNumber` | 创建区块 |
| `evidence_cid` | `BoundedVec<u8, 128>` | 发起方 IPFS 证据 |
| `counter_evidence_cid` | `Option<BoundedVec<u8, 128>>` | 反驳方 IPFS 证据 |
| `counter_party` | `Option<AccountId>` | 反驳方 |

### 辅助结构

| 类型 | 说明 |
|------|------|
| `MarketStats` | `total_orders: u64`, `total_trades: u64`, `total_volume_usdt: u128` |
| `TwapAccumulator` | `current_cumulative`, `current_block`, `last_price`, `trade_count`, `{hour,day,week}_snapshot`, `last_{hour,day,week}_update` |
| `PriceSnapshot` | `cumulative_price: u128`, `block_number: u32` |
| `PriceProtectionConfig` | `enabled`, `max_price_deviation`, `circuit_breaker_threshold`, `min_trades_for_twap`, `circuit_breaker_active`, `circuit_breaker_until`, `initial_price` |
| `DisputeResolution` | `ReleaseToBuyer` / `RefundToSeller` |

---

## 存储

| 存储项 | 类型 | 说明 |
|--------|------|------|
| **订单** | | |
| `NextOrderId` | `ValueQuery<u64>` | 订单 ID 计数器 |
| `Orders` | `Map<u64, Order>` | 订单映射 |
| `SellOrders` | `BoundedVec<u64, MaxSellOrders>` | 卖单索引 |
| `BuyOrders` | `BoundedVec<u64, MaxBuyOrders>` | 买单索引 |
| `UserOrders` | `Map<AccountId, BoundedVec<u64, 100>>` | 用户订单索引 |
| **USDT 交易** | | |
| `NextUsdtTradeId` | `ValueQuery<u64>` | USDT 交易 ID 计数器 |
| `UsdtTrades` | `Map<u64, UsdtTrade>` | USDT 交易映射 |
| `PendingUsdtTrades` | `BoundedVec<u64, MaxPendingTrades>` | OCW 待验证队列 |
| `AwaitingPaymentTrades` | `BoundedVec<u64, MaxAwaitingPaymentTrades>` | 待付款跟踪队列 |
| `PendingUnderpaidTrades` | `BoundedVec<u64, MaxUnderpaidTrades>` | 少付补付队列 |
| `OcwVerificationResults` | `Map<u64, (PaymentVerificationResult, u64)>` | OCW 验证结果（结算成功后清理，失败时保留） |
| **索引** | | |
| `UserTrades` | `Map<AccountId, BoundedVec<u64, MaxTradesPerUser>>` | 用户交易索引 |
| `OrderTrades` | `Map<u64, BoundedVec<u64, MaxOrderTrades>>` | 订单关联交易索引 |
| **价格** | | |
| `BestAsk` / `BestBid` | `Option<u64>` | 最优卖价 / 买价 |
| `LastTradePrice` | `Option<u64>` | 最新成交价 |
| `TwapAccumulatorStore` | `Option<TwapAccumulator>` | TWAP 累积器 |
| **保护 & 配置** | | |
| `PriceProtectionStore` | `Option<PriceProtectionConfig>` | 价格保护 + 熔断配置 |
| `MarketPausedStore` | `ValueQuery<bool>` | 市场暂停标志 |
| `TradingFeeBps` | `ValueQuery<u16>` | 交易手续费率 |
| `DepositExchangeRate` | `Option<u64>` | 保证金动态汇率覆盖值 |
| **防御 & 审计** | | |
| `CompletedBuyers` | `Map<AccountId, bool>` | 已完成首单的买家（L3 防 Sybil） |
| `ActiveWaivedTrades` | `Map<AccountId, u32>` | 活跃免保证金交易数（L2 防 grief） |
| `UsedTxHashes` | `Map<TxHash, (u64, BlockNumber)>` | 已使用的 TRON tx_hash（TTL 自动清理） |
| `TxHashGcCursor` | `Option<TxHash>` | UsedTxHashes GC 游标 |
| `BannedAccounts` | `Map<AccountId, bool>` | 封禁用户列表 |
| **统计 & 争议** | | |
| `MarketStatsStore` | `MarketStats` | 累计市场统计 |
| `TradeDisputeStore` | `Map<u64, TradeDispute>` | 交易争议记录 |

---

## Events

| 事件 | 字段 | 触发场景 |
|------|------|---------|
| `OrderCreated` | `order_id`, `maker`, `side`, `nex_amount`, `usdt_price` | 创建订单 |
| `OrderCancelled` | `order_id` | 取消订单 / ban_user 自动取消 |
| `OrderPriceUpdated` | `order_id`, `old_price`, `new_price` | 修改价格 |
| `OrderAmountUpdated` | `order_id`, `old_amount`, `new_amount` | 修改数量 |
| `UsdtTradeCreated` | `trade_id`, `order_id`, `seller`, `buyer`, `nex_amount`, `usdt_amount` | 创建交易 |
| `UsdtPaymentSubmitted` | `trade_id` | 买家确认付款 |
| `UsdtTradeCompleted` | `trade_id`, `order_id` | 全额付款结算 |
| `UsdtTradeVerificationFailed` | `trade_id`, `reason` | OCW 验证失败 |
| `UsdtTradeRefunded` | `trade_id` | AwaitingPayment 超时退款 |
| `VerificationTimeoutRefunded` | `trade_id`, `buyer`, `seller`, `usdt_amount` | AwaitingVerification 宽限期后超时退款 |
| `SellerConfirmedReceived` | `trade_id`, `seller` | 卖家手动确认收款 |
| `OcwResultSubmitted` | `trade_id`, `verification_result`, `actual_amount` | OCW 提交验证结果 |
| `AutoPaymentDetected` | `trade_id`, `actual_amount` | OCW 预检检测到付款 |
| `UnderpaidDetected` | `trade_id`, `expected_amount`, `actual_amount`, `payment_ratio`, `deadline` | 少付进入补付窗口 |
| `UnderpaidAmountUpdated` | `trade_id`, `previous_amount`, `new_amount` | 补付金额更新 |
| `UnderpaidFinalized` | `trade_id`, `final_amount`, `payment_ratio`, `deposit_forfeit_rate` | 补付终裁 |
| `UnderpaidAutoProcessed` | `trade_id`, `expected_amount`, `actual_amount`, `payment_ratio`, `nex_released`, `deposit_forfeited` | 少付自动结算 |
| `BuyerDepositLocked` | `trade_id`, `buyer`, `deposit` | 保证金锁定 |
| `BuyerDepositReleased` | `trade_id`, `buyer`, `deposit` | 保证金退还 |
| `BuyerDepositForfeited` | `trade_id`, `buyer`, `forfeited`, `to_treasury` | 保证金没收 |
| `TradingFeeCharged` | `trade_id`, `fee_amount`, `to_treasury` | 手续费扣除 |
| `TwapUpdated` | `new_price`, `twap_1h`, `twap_24h`, `twap_7d` | TWAP 更新 |
| `CircuitBreakerTriggered` | `current_price`, `twap_7d`, `deviation_bps`, `until_block` | 熔断触发 |
| `CircuitBreakerLifted` | — | 熔断解除 |
| `PriceProtectionConfigured` | `enabled`, `max_deviation` | 价格保护配置 |
| `InitialPriceSet` | `initial_price` | 初始价格设置 |
| `LiquiditySeeded` | `order_count`, `total_nex`, `source` | 种子流动性挂单 |
| `SeedAccountFunded` | `amount`, `treasury`, `seed_account` | 种子账户注资 |
| `WaivedDepositTradeCreated` | `trade_id`, `buyer`, `nex_amount` | 免保证金交易创建 |
| `MarketPaused` | — | 市场暂停 |
| `MarketResumed` | — | 市场恢复 |
| `QueueOverflowPaused` | `pending_count`, `max_capacity` | 队列满自动暂停 |
| `TradeForceSettled` | `trade_id`, `actual_amount`, `resolution` | 强制结算 |
| `TradeForceCancelled` | `trade_id` | 强制取消 |
| `BatchForceSettled` | `trade_ids`, `resolution` | 批量强制结算 |
| `BatchForceCancelled` | `trade_ids` | 批量强制取消 |
| `TradeDisputed` | `trade_id`, `initiator`, `evidence_cid` | 发起争议 |
| `CounterEvidenceSubmitted` | `trade_id`, `party`, `evidence_cid` | 提交反驳证据 |
| `DisputeResolved` | `trade_id`, `resolution` | 争议裁决 |
| `TradingFeeUpdated` | `old_fee_bps`, `new_fee_bps` | 手续费变更 |
| `DepositExchangeRateUpdated` | `old_rate`, `new_rate` | 保证金汇率变更 |
| `VerificationRewardClaimed` | `trade_id`, `claimer`, `reward`, `reward_paid` | 手动结算兜底 |
| `UserBanned` | `account` | 用户封禁 |
| `UserUnbanned` | `account` | 用户解封 |

---

## Errors

| 错误 | 说明 |
|------|------|
| `OrderNotFound` | 订单不存在 |
| `NotOrderOwner` | 不是订单所有者 |
| `OrderClosed` | 订单已关闭 |
| `OrderExpired` | 订单已过期 |
| `InsufficientBalance` | NEX 余额不足 |
| `AmountTooSmall` | 数量为零或过小 |
| `OrderAmountBelowMinimum` | 低于最低挂单/吃单限额 |
| `OrderAmountTooLarge` | 超过最大挂单限额 |
| `AmountBelowFilledAmount` | 修改后数量低于已成交量 |
| `BelowMinFillAmount` | 低于卖单最低成交量 |
| `ZeroPrice` | USDT 单价为零 |
| `OrderBookFull` | 订单簿已满 |
| `UserOrdersFull` | 用户订单数已满 |
| `CannotTakeOwnOrder` | 不能吃自己的单 |
| `ArithmeticOverflow` | 算术溢出 |
| `OrderSideMismatch` | 订单方向不匹配 |
| `InvalidTronAddress` | TRON 地址校验失败 |
| `UsdtTradeNotFound` | USDT 交易不存在 |
| `NotTradeParticipant` | 不是交易参与者 |
| `InvalidTradeStatus` | 当前状态不允许该操作 |
| `TradeTimeout` | 交易已超时 |
| `PendingQueueFull` | 待验证队列已满 |
| `AwaitingPaymentQueueFull` | 待付款队列已满 |
| `UnderpaidQueueFull` | 补付队列已满 |
| `PriceDeviationTooHigh` | 价格偏离超过阈值 |
| `MarketCircuitBreakerActive` | 市场熔断中 |
| `CircuitBreakerNotActive` | 熔断未激活 |
| `CircuitBreakerNotExpired` | 熔断持续时间未到期 |
| `OcwResultNotFound` | OCW 验证结果不存在 |
| `InsufficientDepositBalance` | 保证金余额不足 |
| `InvalidBasisPoints` | 基点参数超过 10000 |
| `FirstOrderLimitReached` | 免保证金每账户仅 1 笔活跃 |
| `FirstOrderAmountTooLarge` | 免保证金金额超上限 |
| `BuyerAlreadyCompleted` | 已完成过交易，不再免保证金 |
| `TooManySeedOrders` | 单次订单数超限 |
| `NoPriceReference` | 无可用基准价格 |
| `StillInGracePeriod` | 仍在验证宽限期内 |
| `UnderpaidGraceNotExpired` | 补付窗口尚未到期 |
| `NotUnderpaidPending` | 不在 UnderpaidPending 状态 |
| `TxHashAlreadyUsed` | TRON tx_hash 已被使用 |
| `MarketIsPaused` | 市场已暂停 |
| `TradeAlreadyDisputed` | 交易已存在争议 |
| `TradeNotDisputable` | 交易状态不可争议 |
| `DisputeNotFound` | 争议不存在 |
| `DisputeAlreadyClosed` | 争议已解决 |
| `FeeTooHigh` | 手续费过高 |
| `OrderHasActiveTrades` | 订单有活跃交易 |
| `ZeroExchangeRate` | 汇率不能为零 |
| `UserTradesFull` | 用户交易列表已满 |
| `OrderTradesFull` | 订单交易列表已满 |
| `UserIsBanned` | 用户已被封禁 |
| `DisputeWindowExpired` | 争议窗口已过期 |
| `NotParticipantOrAdmin` | 不是参与方或管理员 |
| `CounterEvidenceAlreadySubmitted` | 反驳证据已提交 |
| `PaymentNotConfirmed` | 买家从未确认付款 |

---

## 对外价格服务

本模块是全链唯一的 NEX/USDT 价格数据源，通过三层 Trait 向外部模块提供价格：

```text
┌──────────────────────────────────────────────┐
│       ExchangeRateProvider (高级封装)          │
│  get_nex_usdt_rate() + price_confidence()     │
│  is_rate_reliable() (默认阈值 30)             │
├──────────────┬───────────────────────────────┤
│ PricingProvider │      PriceOracle             │
│ (底层汇率查询)  │ (底层 TWAP 预言机)            │
│ get_cos_to_usd  │ get_twap / get_last_trade    │
│ report_p2p_trade│ is_price_stale / trade_count  │
└──────────────┴───────────────────────────────┘
                       │
           ┌───────────┴───────────┐
           │   pallet-nex-market    │
           │  TWAP 累积器 + 订单簿   │
           └───────────────────────┘
```

### 价格优先级

```text
1h TWAP → LastTradePrice → initial_price（治理设定）
```

### Runtime 适配器

| 适配器 | 接口 | 消费方 |
|--------|------|--------|
| `TradingPricingProvider` | `PricingProvider<Balance>` | 全局底层 |
| `EntityPricingProvider` | `entity_common::PricingProvider` | entity-registry 初始资金 |
| `NexExchangeRateProvider` | `ExchangeRateProvider` | 佣金/打赏等 |

---

## Runtime API

通过 `NexMarketApi` trait 提供前端查询接口（支持分页）：

| 方法 | 返回 | 说明 |
|------|------|------|
| `get_sell_orders(offset, limit)` | `Vec<OrderInfo>` | 活跃卖单（按价格升序，分页） |
| `get_buy_orders(offset, limit)` | `Vec<OrderInfo>` | 活跃买单（按价格降序，分页） |
| `get_user_orders(user)` | `Vec<OrderInfo>` | 用户所有订单 |
| `get_user_trades(user, offset, limit)` | `Vec<TradeInfo>` | 用户交易历史（分页） |
| `get_order_trades(order_id)` | `Vec<TradeInfo>` | 订单关联交易 |
| `get_active_trades(user)` | `Vec<TradeInfo>` | 用户活跃交易 |
| `get_order_depth()` | `(Vec<DepthEntry>, Vec<DepthEntry>)` | 订单深度图 |
| `get_best_prices()` | `(Option<u64>, Option<u64>)` | (最优卖价, 最优买价) |
| `get_market_summary()` | `MarketSummary` | 市场摘要 |
| `get_order_by_id(order_id)` | `Option<OrderInfo>` | 单个订单详情 |
| `get_trade_by_id(trade_id)` | `Option<TradeInfo>` | 单个交易详情 |

### API 返回类型

| 类型 | 字段 |
|------|------|
| `OrderInfo` | `order_id`, `side`(0/1), `owner`, `nex_amount`, `filled_amount`, `usdt_price`, `status`(0-4), `created_at`, `expires_at`, `min_fill_amount` |
| `TradeInfo` | `trade_id`, `order_id`, `seller`, `buyer`, `nex_amount`, `usdt_amount`, `status`(0-4), `created_at`, `timeout_at`, `buyer_deposit`, `deposit_status`(0-3), `underpaid_deadline`, `completed_at`, `payment_confirmed` |
| `DepthEntry` | `price`, `amount` |
| `MarketSummary` | `best_ask`, `best_bid`, `last_trade_price`, `is_paused`, `trading_fee_bps`, `pending_trades_count` |

---

## 精度说明

| 数据 | 精度 | 示例 |
|------|------|------|
| NEX 数量 | 10^12 | `1_000_000_000_000` = 1 NEX |
| USDT 金额 | 10^6 | `1_000_000` = 1 USDT |
| usdt_price | 10^6 (USDT per NEX) | `500_000` = 0.5 USDT/NEX |
| USDT 金额计算 | `nex_amount × usdt_price / 10^12` | — |
| 保证金计算 | `DepositCalculator::calculate_deposit(usdt_deposit, fallback)` | 实时市场价格换算 |

> 前端精度转换：`price / 1_000_000` → USDT 显示，`amount / 1_000_000_000_000` → NEX 显示。

---

## Config 参数

| 参数 | 类型 | 默认值 | 说明 |
|------|------|--------|------|
| **订单** | | | |
| `DefaultOrderTTL` | `u32` | 24h | 订单有效期 |
| `MaxActiveOrdersPerUser` | `u32` | 100 | 每用户最大活跃订单 |
| `MinOrderNexAmount` | `Balance` | 1 NEX | 最低挂单/吃单量 |
| `MaxOrderNexAmount` | `Balance` | 可配置 | 最大挂单/吃单量 |
| `MaxSellOrders` | `u32` | 可配置 | 卖单簿容量 |
| `MaxBuyOrders` | `u32` | 可配置 | 买单簿容量 |
| **USDT 交易** | | | |
| `UsdtTimeout` | `u32` | 12h | 付款超时 |
| `VerificationGracePeriod` | `u32` | 1h | AwaitingVerification 超时宽限期 |
| `UnderpaidGracePeriod` | `u32` | 2h | 少付补付窗口 |
| **队列** | | | |
| `MaxPendingTrades` | `u32` | 200 | OCW 待验证队列容量 |
| `MaxAwaitingPaymentTrades` | `u32` | 200 | 待付款队列容量 |
| `MaxUnderpaidTrades` | `u32` | 100 | 少付补付队列容量 |
| `QueueFullThresholdBps` | `u16` | 8000 (80%) | 队列满暂停阈值 |
| **索引** | | | |
| `MaxTradesPerUser` | `u32` | 500 | 每用户最大交易记录数 |
| `MaxOrderTrades` | `u32` | 100 | 每订单最大关联交易数 |
| **TWAP** | | | |
| `BlocksPerHour` | `u32` | 600 | 1h 区块数 |
| `BlocksPerDay` | `u32` | 14400 | 24h 区块数 |
| `BlocksPerWeek` | `u32` | 100800 | 7d 区块数 |
| **价格保护** | | | |
| `CircuitBreakerDuration` | `u32` | 1h | 熔断持续时间 |
| **OCW 奖励** | | | |
| `VerificationReward` | `Balance` | 0.1 NEX | 验证奖励 |
| `RewardSource` | `AccountId` | nxm/rwds | 奖励来源账户 |
| **保证金** | | | |
| `BuyerDepositRate` | `u16` | 1000 (10%) | 保证金比例 |
| `MinBuyerDeposit` | `Balance` | 10 NEX | 最低保证金 |
| `DepositForfeitRate` | `u16` | 10000 (100%) | 超时没收比例 |
| `DepositCalculator` | trait impl | `DepositCalculatorImpl` | 实时价格换算 |
| **账户** | | | |
| `TreasuryAccount` | `AccountId` | nxm/trsy | 国库账户 |
| `SeedLiquidityAccount` | `AccountId` | nxm/seed | 种子账户 |
| **治理** | | | |
| `MarketAdminOrigin` | `EnsureOrigin` | TreasuryCouncil 2/3 | 管理操作审批 |
| **seed_liquidity** | | | |
| `FirstOrderTimeout` | `u32` | 1h | 免保证金短超时 |
| `MaxFirstOrderAmount` | `Balance` | 100 NEX | 免保证金单笔上限 |
| `MaxWaivedSeedOrders` | `u32` | 20 | 单次最多挂单数 |
| `SeedPricePremiumBps` | `u16` | 2000 (20%) | 溢价比例 |
| `SeedOrderUsdtAmount` | `u64` | 10 USDT | 每笔固定金额 |
| `SeedTronAddress` | `[u8; 34]` | 固定地址 | 种子 TRON 收款地址 |
| **争议** | | | |
| `DisputeWindowBlocks` | `u32` | 7d | 争议窗口（从 completed_at 起算） |
| **GC & 清理** | | | |
| `MaxExpiredOrdersPerBlock` | `u32` | 20 | on_idle 过期订单 GC 处理数 |
| `TxHashTtlBlocks` | `u32` | 7d | UsedTxHashes 保留时间 |

---

## 订单回滚机制

交易超时/失败时回滚父订单 `filled_amount`，包含多重防护：

| 场景 | 处理 |
|------|------|
| 订单 Filled → 回滚后仍有余量 | 恢复为 Open/PartiallyFilled，重新加入订单簿 |
| 回滚时订单已过期 | 标记为 Expired，不加回订单簿 |
| 回滚时订单已 Cancelled/Expired | 仅回退 filled_amount，不改变状态 |
| 订单簿/用户索引满 | 记录 warn 日志，订单成为幽灵记录 |

---

## 最优价格维护

| 触发 | 策略 | 复杂度 |
|------|------|--------|
| 新订单创建 | O(1) 比较当前 best price | O(1) |
| 订单取消/成交 | 仅在影响最优价时重扫 | O(1) 或 O(n) |
| on_idle GC / seed_liquidity | 全量刷新 | O(n) |

---

## 测试

```bash
cargo test -p pallet-nex-market    # 176 个单元测试
```

覆盖范围：
- 卖单/买单创建、取消、过期、改价、改量、最低成交量
- reserve_sell_order / accept_buy_order（含部分成交、最低限额、数量上限）
- confirm_payment 流程 + 队列满自动暂停
- 完整交易结算（Exact / Overpaid）+ 手续费扣除 + 自动结算
- 少付处理（Underpaid / SeverelyUnderpaid / Invalid）
- 补付窗口（submit_underpaid_update / finalize_underpaid）
- 超时退款 + 保证金没收（三阶段）
- process_timeout 调用者限制（参与方 + Admin）
- 价格偏离检查 + 熔断触发/解除
- TWAP 累积器更新 + 快照推进
- 最优价格维护（增量 + 全量）
- seed_liquidity 多层防御（L0-L3）
- 订单回滚（filled_amount 恢复 + 索引恢复 + 过期保护）
- 市场暂停/恢复
- 卖家手动确认收款（AwaitingVerification / UnderpaidPending 限制）
- 用户封禁/解封 + 自动取消挂单 + 跳过活跃交易
- 争议发起与裁决（completed_at 窗口、payment_confirmed 准入）
- 反驳证据提交 + 不可覆盖验证
- 批量强制结算/取消
- resolve_dispute 国库补偿安全（Completed 不重复支付、余额不足尽力补偿）
- OCW 结果持久化（结算前存储、成功后清理、Underpaid 保留）
- tx_hash 防重放
- ValidateUnsigned 安全边界
- 过期订单 GC
- 保证金动态汇率

---

## 与 pallet-entity-market 的区别

| 维度 | entity-market | nex-market |
|------|---------------|------------|
| 交易对 | Entity Token ↔ NEX/USDT | NEX ↔ USDT |
| 市场数量 | 多个（每店铺一个） | 单一全局市场 |
| NEX 通道 | 原子交换 | 无（NEX 是标的物） |
| USDT 通道 | ✅ | ✅ |
| TWAP 预言机 | 无 | ✅ 全链价格数据源 |
| seed_liquidity | 无 | ✅ 冷启动引流 |
| 交易手续费 | 无 | ✅ 可治理配置 |
| 争议仲裁 | 无 | ✅ 双方举证 + 管理员裁决 |
| 用户封禁 | 无 | ✅ 封禁 + 自动取消挂单 |
| 紧急暂停 | 无 | ✅ 手动 + 自动 |
| tx_hash 防重放 | 无 | ✅ TTL 自动清理 |
| 批量管理 | 无 | ✅ 批量结算/取消 |

---

**License**: Unlicense
