# NEXUS Pallets

NEXUS 区块链的模块化 Pallet 架构。基于 Substrate FRAME 框架构建，包含 5 大领域、33 个 crate。

## 架构总览

```
pallets/
├── entity/          # 实体商业系统 (16 子模块)
├── dispute/         # 争议解决系统 (3 子模块)
├── trading/         # 交易基础设施 (3 子模块)
├── storage/         # IPFS 存储管理 (2 子模块)
└── inscription/     # 创世铭文 (1 模块)
```

## 领域模块

### Entity — 实体商业系统

核心商业领域，管理实体、商铺、商品、订单、会员、佣金、治理等完整商业生态。

| Pallet | 索引 | 说明 |
|--------|------|------|
| [common](entity/common/) | — | 共享类型、Trait、错误定义 |
| [registry](entity/registry/) | 120 | 实体注册、生命周期、运营资金 |
| [shop](entity/shop/) | 129 | 商铺管理、运营余额、评级统计 |
| [product](entity/product/) | 121 | 商品目录、库存、定价 |
| [order](entity/order/) | 122 | 订单生命周期、支付、退款 |
| [review](entity/review/) | 123 | 商品评价、商铺评分 |
| [member](entity/member/) | 126 | 会员注册、推荐链、等级升级 |
| [token](entity/token/) | 124 | 实体代币发行、转账、分红 |
| [loyalty](entity/loyalty/) | 139 | 积分系统、NEX/Token 消费余额 |
| [commission](entity/commission/) | 127-138 | 插件化佣金引擎 (7 个插件) |
| [governance](entity/governance/) | 125 | DAO 提案投票、资金保护 |
| [disclosure](entity/disclosure/) | 130 | 信息披露、内幕交易管控 |
| [kyc](entity/kyc/) | 131 | KYC 身份验证 (5 级) |
| [market](entity/market/) | 128 | 实体代币交易市场 |
| [tokensale](entity/tokensale/) | 132 | 代币预售 (IDO/拍卖/白名单) |

### Dispute — 争议解决

链上仲裁、资金托管、IPFS 加密证据管理。

| Pallet | 索引 | 说明 |
|--------|------|------|
| [arbitration](dispute/arbitration/) | 64 | 仲裁 + 投诉双子系统 |
| [escrow](dispute/escrow/) | 60 | 通用资金托管 |
| [evidence](dispute/evidence/) | 63 | IPFS 证据 (加密/密封/归档) |

### Trading — 交易基础设施

NEX/USDT P2P 交易市场与 TRC20 链下验证。

| Pallet | 索引 | 说明 |
|--------|------|------|
| [common](trading/common/) | — | 共享类型、PII 脱敏、TWAP |
| [nex-market](trading/nex-market/) | 56 | NEX/USDT 订单簿交易 |
| [trc20-verifier](trading/trc20-verifier/) | — | TRC20 离线验证库 |

### Storage — 存储管理

IPFS Pin 服务协调与多级归档生命周期管理。

| Pallet | 索引 | 说明 |
|--------|------|------|
| [service](storage/service/) | 62 | IPFS Pin 服务 (三级/SLA/配额) |
| [lifecycle](storage/lifecycle/) | 65 | 分级归档 (Active→L1→L2→Purge) |

索引 150–155（原 GroupRobot）、160–162（原 Ads）与 176–193（原 Prediction）已退役，禁止复用。

### Inscription — 创世铭文

| Pallet | 索引 | 说明 |
|--------|------|------|
| [inscription](inscription/) | 11 | 不可变创世铭文 |

## 架构特性

- **Hook 驱动**: 订单完成触发 Hook 链 (会员→商铺→佣金→积分)，取代直接跨模块调用
- **Port 模式**: 细粒度 Trait 接口 (AssetLedgerPort, GovernancePort 等) 实现松耦合
- **双资产管道**: NEX + Entity Token 并行处理佣金、消费余额
- **Bridge 适配**: Runtime 中定义 Bridge 结构体，零改动桥接 Pallet 间通信
- **插件化佣金**: 7 种佣金模式通过 CommissionPlugin trait 热插拔

## 技术栈

- **Substrate**: Polkadot SDK (frame-support v45, sp-runtime v45)
- **编码**: SCALE (parity-scale-codec v3.7)
- **Rust Edition**: 2021
- **共识**: Aura + GRANDPA
