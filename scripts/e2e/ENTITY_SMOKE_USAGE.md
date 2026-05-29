# Entity Smoke Suites 使用说明

本文档说明新增的 5 个 Entity smoke suites 如何运行、适合在哪些改动后执行，以及常见注意事项。

## 1. 这些 smoke suites 是做什么的

这 5 个 suite 属于**开发者自助测试层**，定位是：

- 验证角色主流程是否还能跑通
- 帮助开发者在改动后快速自测
- 作为联调和发版前的轻量回归入口

它们**不是** pallet 单测的替代品。

建议搭配使用：

- `cargo test`：验证链上规则正确性
- `npm run e2e:entity:*`：验证开发者视角下的主流程可用性

---

## 2. 当前提供的 5 个 suites

### 1) Buyer core flow

命令：

```bash
npm run e2e:entity:buyer
```

验证内容：

- 创建/准备一个 Entity 与商品
- 买家发起一笔 Digital 商品订单
- 订单自动完成
- 买家会员状态可查询
- Loyalty / shopping balance 路径可查询

适合在这些改动后运行：

- `pallets/entity/order/*`
- `pallets/entity/member/*`
- `pallets/entity/loyalty/*`
- 订单完成后的 Hook 联动逻辑

---

### 2) Seller core flow

命令：

```bash
npm run e2e:entity:seller
```

验证内容：

- 创建/准备一个 Physical 商品
- 买家下单
- 商家发货
- 买家确认收货
- 订单状态从 `Paid -> Shipped -> Completed`

适合在这些改动后运行：

- `pallets/entity/shop/*`
- `pallets/entity/order/*`
- 履约、发货、确认收货相关逻辑

---

### 3) Commission core flow

命令：

```bash
npm run e2e:entity:commission
```

验证内容：

- 创建/准备推荐关系
- 配置最小可工作的 direct commission 路径
- 买家下单触发佣金
- 推荐人 commission stats 增长
- 订单 commission record 可查询

适合在这些改动后运行：

- `pallets/entity/commission/*`
- `pallets/entity/member/*`
- `pallets/entity/order/*`
- 订单完成后佣金触发逻辑

---

### 4) Governance core flow

命令：

```bash
npm run e2e:entity:governance
```

验证内容：

- 创建 Entity
- 创建 governance token
- 配置治理模式
- 创建 proposal
- 投票
- proposal 与 vote record 可查询

适合在这些改动后运行：

- `pallets/entity/governance/*`
- `pallets/entity/token/*`
- 提案/投票/治理配置相关逻辑

---

### 5) Market core flow

命令：

```bash
npm run e2e:entity:market
```

验证内容：

- 创建 Entity token
- 配置 entity market
- 卖家挂卖单
- 买家接单
- 市场订单状态可查询

适合在这些改动后运行：

- `pallets/entity/market/*`
- `pallets/entity/token/*`
- entity 市场配置、挂单、成交逻辑

---

## 3. 一次性运行全部 Entity smoke suites

命令：

```bash
npm run e2e:entity:smoke
```

适合场景：

- 发版前做一轮轻量回归
- 改动多个 Entity 核心模块之后做联调检查
- 确认主要角色路径没有明显断裂

---

## 4. 运行前建议

### 推荐优先在本地开发链运行

因为这些 suites 会写链状态，推荐优先在本地 dev chain 上执行。

如果你在远程链上跑：

- 可能受链上已有状态影响
- 可能受余额、已有 entity、已有配置影响
- 失败原因会更难区分

### 运行前至少确认两件事

1. 节点可连接
2. 测试账户有足够余额

可以先跑：

```bash
npm run e2e:list
npm run e2e:smoke
```

---

## 5. 推荐的开发使用方式

### 改订单相关逻辑后

至少跑：

```bash
npm run e2e:entity:buyer
npm run e2e:entity:seller
```

### 改佣金相关逻辑后

至少跑：

```bash
npm run e2e:entity:commission
npm run e2e:entity:buyer
```

### 改治理相关逻辑后

至少跑：

```bash
npm run e2e:entity:governance
```

### 改市场相关逻辑后

至少跑：

```bash
npm run e2e:entity:market
```

### 改多个核心模块后

建议跑：

```bash
npm run e2e:entity:smoke
```

---

## 6. 常见失败原因

### 1) 链状态不干净

表现：

- 已存在相同资源
- 某些配置与脚本预期不一致
- 某些 ID/状态不是脚本假设的初始值

建议：

- 优先在本地链运行
- 必要时重启本地链再跑

### 2) 账户余额不足

表现：

- 下单失败
- 创建 entity / fundOperating 失败
- 市场挂单失败

建议：

- 先确认开发账户资金是否充足

### 3) Runtime 签名或调用参数变化

表现：

- 脚本编译能过，但链上调用失败
- 某个 extrinsic 参数签名已经变了

建议：

- 先跑：

```bash
npm run e2e:typecheck
npm run e2e:list
```

- 再检查对应 pallet 的当前 call 签名

### 4) 联动逻辑变化

表现：

- 订单成功了，但会员/佣金/市场状态没有按预期变化

建议：

- 回查相关 Hook 链路
- 分别检查 order、member、commission、loyalty、governance、market 的联动点

---

## 7. 与现有测试体系的关系

推荐理解方式：

### Rust pallet 单测
负责：

- 规则正确性
- 边界条件
- 错误路径
- 精确状态机与资金计算

### Entity smoke suites
负责：

- 角色主流程是否跑通
- 开发者联调是否顺畅
- 改动后快速回归

### 远程链专项脚本
负责：

- 复杂佣金
- 多账户真实业务流
- 发版前高风险专项回归

---

## 8. 建议的最小习惯

如果你是开发者，建议形成这个习惯：

- 改单模块：先跑对应 `cargo test`
- 改主流程：再跑对应 `npm run e2e:entity:*`
- 改多个核心模块：最后跑 `npm run e2e:entity:smoke`

这是当前成本最低、收益最高的使用方式。
