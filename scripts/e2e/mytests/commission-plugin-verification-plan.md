# 分佣插件验证测试方案

> 目标：验证 Entity 100000 (Metaverse Door) 的分佣系统在实际多人购买场景下是否正确分配佣金

## 一、测试概述

### 目标
- 创建 20 个测试账户（派生自指定助记词）
- 构建推荐链关系（链式推荐，模拟真实太阳线场景）
- 每个账户转入 1 亿 NEX 作为购买资金
- 反复购买 Entity 100000 商铺的商品
- 验证 Multi-Level（动态奖励）和 Single-Line（静态奖励）分佣的正确性

### 助记词
```
fabric smile father unique elbow buffalo until emerge novel orient rally basket
```
此助记词对应的地址为 Entity 100000 的创建者（seller/owner）。

### 前置条件
- Entity 100000 已存在且分佣系统已配置（通过 `setup-entity-full.ts` 完成）
- 节点地址：`ws://202.140.140.202:9944`

## 二、账户设计

### 派生方式
从同一助记词派生 20 个子账户：
- `//Test/0` ~ `//Test/19`（sr25519, ss58Format=273）

如果这些账户已经注册为 Entity 100000 的会员，则跳过创建。

### 推荐链结构（太阳线）
```
Owner (seller)
  └── Test/0（由 seller 直推）
        └── Test/1（由 Test/0 推荐）
              └── Test/2（由 Test/1 推荐）
                    └── ... (链式)
                          └── Test/19（由 Test/18 推荐）
```
这形成了一条 20 层的推荐链，用于测试 Multi-Level 13 级分佣和 Single-Line 公排分佣。

## 三、测试流程

### Phase 1: 账户准备

1. **检测已有账户**
   - 遍历 Test/0 ~ Test/19
   - 查询 `entityMember.entityMembers(100000, address)` 判断是否已注册
   - 如果已注册，跳过创建
   - 如果未注册，进入创建流程

2. **转账**
   - 从 seller 账户向每个测试账户转入 1 亿 NEX（`100_000_000 NEX = 100_000_000_000_000_000_000n`）
   - 如果已有足够余额（≥ 1 亿 NEX），跳过转账

3. **注册会员**
   - 确保商铺已开放注册（`entityMember.setMemberPolicy(shopId, 0)`）
   - 按序注册：Test/0 无推荐人 → Test/1 推荐人为 Test/0 → ... → Test/19 推荐人为 Test/18
   - 注册后由 seller 激活每个会员

### Phase 2: 购买测试

1. **查询商品**
   - 查询 Entity 100000 商铺下的可购买商品
   - 选择一个 Digital 类商品（Digital 商品会自动完成订单，触发分佣）
   - 如果不存在合适商品，使用最小金额商品 (Plan1-5USDT)

2. **反复购买**
   - 每个测试账户购买 N 次（默认 3 次）
   - 购买顺序：Test/0 先买（建立 single-line 位置），然后 Test/1, Test/2, ...
   - 首次购买用于建立 single-line 索引
   - 第二次及后续购买触发真实的分佣分配

3. **记录每次购买的关键数据**
   - 订单 ID
   - 订单金额
   - 购买者地址
   - 购买前后各账户的 `memberCommissionStats` 变化
   - `unallocatedPool` 变化

### Phase 3: 分佣验证

#### 3.1 Multi-Level 验证

以 Test/5 购买一笔金额为 `P` 的订单为例：

```
佣金池 = P × 9900 / 10000 = P × 99%
创建人收益 = 佣金池 × 500 / 10000 = 佣金池 × 5%
插件可分配 = 佣金池 - 创建人收益
ML 预算 = P × 4702 / 10000

推荐链（向上追溯）：
  Test/4 = L1 = P × 1500 / 10000 (15%)
  Test/3 = L2 = P × 500 / 10000  (5%)
  Test/2 = L3 = P × 200 / 10000  (2%)
  Test/1 = L4 = P × 200 / 10000  (2%)
  Test/0 = L5 = P × 200 / 10000  (2%)
  (无更多上级，L6~L13 无受益人)
```

验证逻辑：
- 检查 Test/4 的 `pending` 增量是否包含 ML L1 份额
- 检查 Test/3 的 `pending` 增量是否包含 ML L2 份额
- 未分配的 ML 份额应归入沉淀池

#### 3.2 Single-Line 验证

- 检查购买者的 `singleLineIndex` 是否正确分配
- 检查上线/下线邻居是否正确获得 SL 分佣
- 按等级层数限制验证（Plan1: 上20下30，但测试中只有20人，所以不会超限）

#### 3.3 汇总验证

对每笔订单验证：
```
平台费 = 订单金额 × (10000 - 9900) / 10000 = 1%
创建人收益 = (订单金额 × 9900 / 10000) × 500 / 10000 = 4.95%
ML 总分配 ≤ 订单金额 × 4702 / 10000
SL 总分配 ≤ 订单金额 × 4703 / 10000
未分配 → 沉淀池

所有分配总和 + 平台费 + 未分配 = 订单金额
```

## 四、验证指标

### 每笔订单后检查
| 指标 | 查询方式 | 预期 |
|------|----------|------|
| 订单状态 | `entityTransaction.orders(orderId)` → status | Completed |
| ML 受益人 pending 增量 | `commissionCore.memberCommissionStats(entityId, addr)` | 符合费率表 |
| SL 受益人 pending 增量 | 同上 | 符合 per-level rate |
| 创建人收益 | `commissionCore.memberCommissionStats(entityId, sellerAddr)` | = pool × 5% |
| 沉淀池余额 | `commissionCore.unallocatedPool(entityId)` | 增长 |
| 订单佣金记录 | `commissionCore.orderCommissionRecords(orderId)` | 记录条目正确 |

### 最终汇总
| 指标 | 预期 |
|------|------|
| 所有账户 pending 总和 + withdrawn 总和 | 与理论计算匹配 |
| 沉淀池余额 | = 所有订单的未分配佣金之和 |
| 创建人收益总额 | = 所有订单金额 × 4.95% |

## 五、分佣费率速查表

### Multi-Level 13 级费率（基于订单金额 bps）
| 层级 | 费率(bps) | 百分比 |
|------|-----------|--------|
| L1   | 1500      | 15%    |
| L2   | 500       | 5%     |
| L3   | 200       | 2%     |
| L4   | 200       | 2%     |
| L5   | 200       | 2%     |
| L6   | 200       | 2%     |
| L7   | 200       | 2%     |
| L8   | 700       | 7%     |
| L9   | 200       | 2%     |
| L10  | 200       | 2%     |
| L11  | 200       | 2%     |
| L12  | 200       | 2%     |
| L13  | 500       | 5%     |
| **合计** | **5000** | **50%** |
| **maxTotalRate** | **4702** | **47.02% (截断)** |

### Single-Line 费率
| 参数 | 值 |
|------|-----|
| upline_rate | 48 bps/级 (0.48%) |
| downline_rate | 47 bps/级 (0.47%) |
| base_upline_levels | 20 |
| base_downline_levels | 30 |
| max_upline_levels | 40 |
| max_downline_levels | 60 |
| **cap** | **4703 bps** |

### 资金流向示意（每笔订单金额 P）
```
P (100%)
├── 平台费: P × 1% (100 bps)
└── 佣金池: P × 99% (9900 bps)
      ├── 创建人: 佣金池 × 5% = P × 4.95% (495 bps)
      └── 插件: P × 94.05% (9405 bps)
            ├── ML cap: P × 47.02% (4702 bps)
            ├── SL cap: P × 47.03% (4703 bps)
            └── 剩余 → 沉淀池
```

## 六、脚本文件

```
scripts/commission-verification-test.ts
```

### 运行方式
```bash
cd scripts
node --import tsx commission-verification-test.ts

# 可选环境变量
ENTITY_ID=100000 \
ROUNDS=3 \
PRODUCT_ID=auto \
WS_URL=ws://202.140.140.202:9944 \
node --import tsx commission-verification-test.ts
```

### 关键依赖
- `e2e/framework/api.ts` — connectApi, submitTx, disconnectApi
- `e2e/framework/accounts.ts` — readFreeBalance
- `e2e/framework/assert.ts` — assertTxSuccess
- `e2e/framework/units.ts` — nex, formatNex
- `e2e/framework/codec.ts` — codecToJson, readObjectField, coerceNumber
- `e2e/suites/helpers.ts` — readEntity, resolvePrimaryShopId, readMember, readCommissionStats, readShoppingBalance, readOrder

## 七、预期输出

脚本运行后会输出：

1. **账户准备阶段**：每个账户的地址、余额、会员状态
2. **购买阶段**：每笔订单的 ID、金额、状态
3. **验证阶段**：
   - 每笔订单的分佣明细 vs 理论值对比表
   - PASS/FAIL 判定（容许 ±1 planck 的精度误差）
4. **最终汇总**：
   - 所有账户的佣金统计
   - 沉淀池余额
   - 分佣正确率（PASS 订单数 / 总订单数）
