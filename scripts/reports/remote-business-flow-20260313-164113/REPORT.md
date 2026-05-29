# 远程业务流测试报告（按“跳过已测试流”执行）

- 日期：2026-03-13
- 节点：`wss://202.140.140.202`
- 当前报告目录：`reports/remote-business-flow-20260313-164113/`

## 1. 测试范围

用户点名模块：

- `pallet-commission-single-line`
- `pallet-commission-pool-reward`
- `pallet-commission-multi-level`
- `pallet-entity-shop`
- `pallet-entity-member`
- `pallet-entity-loyalty`
- `pallet-entity-product`
- `pallet-entity-order`（链上实际下单/订单 pallet 为 `entityTransaction`）
- `pallet-nex-market`

## 2. 跳过策略

按“跳过已经测试的流”执行，本次**不重复复跑**仓库中已经有完整成功证据的远端业务流，直接复用同日已落盘的成功结果：

- `remote-business-flows-20260313/artifacts/latest.json`
- `remote-business-flows-20260313/artifacts/execution-status.json`

其中已成功的远端 case：

| case | 模块 | 状态 |
|---|---|---|
| `entity-shop-flow` | `pallet-entity-shop` | Passed |
| `entity-member-loyalty-flow` | `pallet-entity-member` / `pallet-entity-loyalty` | Passed |
| `entity-product-order-physical-flow` | `pallet-entity-product` / `pallet-entity-order` | Passed |
| `commission-admin-controls` | `pallet-commission-single-line` / `pallet-commission-multi-level` / `pallet-commission-pool-reward` | Passed |
| `nex-market-trade-flow` | `pallet-nex-market` | Passed |

## 3. 本次实际复跑内容

### 3.1 远端节点快照

来自 `inspect.json`：

- Chain：`Nexus Development`
- Node：`Nexus Node 0.1.0-unknown`
- Runtime：`nexus v100`
- finalized head：`1018 -> 1019`
- `nexMarket.priceProtection.initialPrice = 10`
- `nexMarket.marketPaused = false`

对应文件：

- `inspect.json`
- `inspect.stderr`

### 3.2 复跑：Entity commerce + commission 组合流

执行方式：直接导入当前 `.ts` 源码运行，避免旧 runner 兼容问题。  
日志文件：

- `entity-commerce-commission-flow-direct.log`
- `entity-commerce-commission-flow-direct.stderr`

结果：**Passed**

关键结论：

- 创建 entity / primary shop 成功
- `entityShop.fundOperating` 成功
- `entityMember` 注册、推荐链、激活成功
- `commissionSingleLine` / `commissionMultiLevel` / `commissionPoolReward` 配置成功
- `entityProduct` 创建发布成功
- `entityTransaction.placeOrder` 连续订单、分佣、提现到 loyalty shopping balance、再次消费、claim pool reward 均成功

关键现场值：

- seller：`Ferdie`
- `entityId = 100005`
- `primaryShopId = 6`
- `poolAfterCharlieOrder = 70.2 NEX`
- `bobShoppingBalance = 3 NEX`
- `poolBeforeClaim = 87.733 NEX`
- `poolAfterClaim = 58.489 NEX`

### 3.3 复跑：NEX market smoke

日志文件：

- `nex-market-smoke-direct.log`
- `nex-market-smoke-direct.stderr`

结果：**Passed**

本次复跑结果：

1. `placeSellOrder`：**Passed**
   - 使用合法 TRON 地址成功创建卖单
   - 日志显示：`seller active order index contains 8`
2. `cancelOrder(sell)`：**Passed**
3. `placeBuyOrder`：**Passed**
   - 使用预充足余额账户 `Alice` 作为买方
   - 并将 smoke 下单数量调整为 `10 NEX`
   - 日志显示：`buyer active order index contains 9`
4. `cancelOrder(buy)`：**Passed**

说明：

- 当前远端链上，`placeBuyOrder` 对买方保证金前置条件较重；
- 在满足该前置条件后，直接买单/撤单 smoke 已验证通过。

## 4. 结合“已测试流”后的最终模块结论

| 模块 | 结论 | 证据 |
|---|---|---|
| `pallet-commission-single-line` | Passed | `entity-commerce-commission-flow-direct.log` + `remote-business-flows-20260313/artifacts/commission-admin-controls.json` |
| `pallet-commission-pool-reward` | Passed | `entity-commerce-commission-flow-direct.log` + `remote-business-flows-20260313/artifacts/commission-admin-controls.json` |
| `pallet-commission-multi-level` | Passed | `entity-commerce-commission-flow-direct.log` + `remote-business-flows-20260313/artifacts/commission-admin-controls.json` |
| `pallet-entity-shop` | Passed | `entity-commerce-commission-flow-direct.log` + `remote-business-flows-20260313/artifacts/entity-shop-flow.json` |
| `pallet-entity-member` | Passed | `entity-commerce-commission-flow-direct.log` + `remote-business-flows-20260313/artifacts/entity-member-loyalty-flow.json` |
| `pallet-entity-loyalty` | Passed | `entity-commerce-commission-flow-direct.log` + `remote-business-flows-20260313/artifacts/entity-member-loyalty-flow.json` |
| `pallet-entity-product` | Passed | `entity-commerce-commission-flow-direct.log` + `remote-business-flows-20260313/artifacts/entity-product-order-physical-flow.json` |
| `pallet-entity-order` / `entityTransaction` | Passed | `entity-commerce-commission-flow-direct.log` + `remote-business-flows-20260313/artifacts/entity-product-order-physical-flow.json` |
| `pallet-nex-market` | Passed | `nex-market-smoke-direct.log` + `remote-business-flows-20260313/artifacts/nex-market-trade-flow.json` |

## 5. `pallet-nex-market` 额外说明

`pallet-nex-market` 现在有两类证据同时成立：

1. **本次 direct smoke 已通过**
   - `placeSellOrder`
   - `cancelOrder(sell)`
   - `placeBuyOrder`
   - `cancelOrder(buy)`

2. **同日已有完整成交业务流通过**

- 文件：`remote-business-flows-20260313/artifacts/nex-market-trade-flow.json`

该文件记录了完整成功链路：

1. `placeSellOrder` → `OrderCreated`
2. `reserveSellOrder` → `UsdtTradeCreated` / `BuyerDepositLocked`
3. `confirmPayment` → `UsdtPaymentSubmitted`
4. `sellerConfirmReceived` → `BuyerDepositReleased` / `UsdtTradeCompleted`

关键结果：

- `orderStatusAfterPlace = Open`
- `tradeStatusAfterReserve = AwaitingPayment`
- `tradeStatusFinal = Completed`
- `orderStatusFinal = Filled`
- `buyerDeposit = 30000000000000000`

因此，对 `pallet-nex-market` 的最终判定为：**模块业务流已完整验证通过**。  
同时也确认了一点：当前远端节点的买单路径需要更高的买方保证金准备，不能用低余额账户直接做 smoke。

## 6. 输出文件

本次新增/复用的关键文件：

- `reports/remote-business-flow-20260313-164113/inspect.json`
- `reports/remote-business-flow-20260313-164113/entity-commerce-commission-flow-direct.log`
- `reports/remote-business-flow-20260313-164113/nex-market-smoke-direct.log`
- `remote-business-flows-20260313/REPORT.md`
- `remote-business-flows-20260313/artifacts/latest.json`
- `remote-business-flows-20260313/artifacts/entity-shop-flow.json`
- `remote-business-flows-20260313/artifacts/entity-member-loyalty-flow.json`
- `remote-business-flows-20260313/artifacts/entity-product-order-physical-flow.json`
- `remote-business-flows-20260313/artifacts/commission-admin-controls.json`
- `remote-business-flows-20260313/artifacts/nex-market-trade-flow.json`
