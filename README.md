# Nexus Blockchain

Nexus 是一个基于 [Polkadot SDK (Substrate)](https://github.com/paritytech/polkadot-sdk) 构建的 Layer-1 区块链，为 Web3 商业场景提供完整的链上基础设施——涵盖商业实体管理、P2P 交易、佣金分销、争议仲裁、去中心化广告、TEE 社群机器人（GroupRobot）及 IPFS 存储。

Runtime 共注册 **51 个 Pallet**（其中 34 个自定义），Workspace 包含 **46 个 crate**。

## 系统架构

```
┌───────────────────────────────────────────────────────────────────────┐
│                       Nexus Blockchain (L1)                            │
│          Polkadot SDK · Aura + GRANDPA · 6 s Block · WASM             │
├───────────────────────────────────────────────────────────────────────┤
│                    Runtime — 51 Pallets                                │
│                                                                       │
│  ┌──────────────┐  ┌────────────┐  ┌─────────────┐  ┌─────────────┐ │
│  │ Entity 商业   │  │ NEX Market │  │ GroupRobot  │  │  Ads 广告    │ │
│  │  19 pallets  │  │  1 pallet  │  │  6 pallets  │  │  3 pallets  │ │
│  │ 实体·代币·   │  │ P2P 交易·  │  │ Bot注册·    │  │ 广告投放·   │ │
│  │ 治理·会员·   │  │ 做市·定价· │  │ 节点共识·   │  │ 渠道分发·   │ │
│  │ 佣金·市场    │  │ 信用·TRC20 │  │ 订阅·奖励   │  │ 结算        │ │
│  └──────────────┘  └────────────┘  └─────────────┘  └─────────────┘ │
│                                                                       │
│  ┌──────────────┐  ┌────────────┐  ┌───────────────────────────────┐ │
│  │  争议解决     │  │ 去中心化   │  │ Substrate 基础层               │ │
│  │  3 pallets   │  │  存储      │  │ System · Balances · Assets ·  │ │
│  │ 托管·证据·   │  │  2 pallets │  │ Contracts · 4 委员会          │ │
│  │ 仲裁         │  │ IPFS·周期  │  │ (技术·仲裁·财务·内容)         │ │
│  └──────────────┘  └────────────┘  └───────────────────────────────┘ │
├───────────────────────────────────────────────────────────────────────┤
│  GroupRobot TEE 离链执行 (独立二进制)                                   │
│  Telegram + Discord · TDX/SGX 双证明 · Gramine SGX · subxt 链交互     │
└───────────────────────────────────────────────────────────────────────┘
```

## 核心子系统

### 1. Entity 商业平台（19 Pallets）

完整的去中心化商业实体全栈管理：

**核心业务（5 Pallets）**

| Pallet | Index | 功能 |
|--------|:-----:|------|
| `entity-registry` | 120 | 实体创建、审核、暂停、关闭、转让 |
| `entity-shop` | 129 | 主店铺/子店铺、运营资金、统计级联 |
| `entity-product` | 121 | 商品与服务上架、管理 |
| `entity-order` | 122 | 订单创建、支付、履约、完成 |
| `entity-review` | 123 | 订单评价、店铺评分聚合 |

**代币经济（5 Pallets）**

| Pallet | Index | 功能 |
|--------|:-----:|------|
| `entity-token` | 124 | 7 类代币铸造、分红、锁仓、转账限制、白/黑名单 |
| `entity-governance` | 125 | DAO 提案、投票（时间加权）、委托、委员会否决 |
| `entity-market` | 128 | 实体代币 NEX/USDT 双通道交易、TWAP、熔断 |
| `entity-tokensale` | 132 | 多轮次 Token Sale、白名单、退款、Vesting |
| `entity-disclosure` | 130 | 内幕人管理、交易窗口期控制 |

**用户管理（2 Pallets）**

| Pallet | Index | 功能 |
|--------|:-----:|------|
| `entity-member` | 126 | 多级会员、推荐链、自动升级、团队统计 |
| `entity-kyc` | 131 | 多级身份认证（KYC/AML） |

**佣金引擎（7 Pallets）**

插件化分佣架构，`commission-core` 为调度引擎，6 种可组合插件：

| Pallet | Index | 模式 |
|--------|:-----:|------|
| `commission-core` | 127 | 调度引擎、存储、提现（4 种模式） |
| `commission-referral` | 133 | 推荐链佣金（直推/多级/固定/首单/复购） |
| `commission-multi-level` | 138 | 多级分佣 |
| `commission-level-diff` | 134 | 级差佣金（自定义等级） |
| `commission-single-line` | 135 | 单线上下级佣金 |
| `commission-team` | 136 | 团队佣金 |
| `commission-pool-reward` | 137 | 池化奖励 |

> 共享类型: `entity-common`、`commission-common` 为库 crate，不直接注册到 Runtime。

### 2. NEX Market 交易系统（1 Pallet + 2 Library）

统一的 P2P 交易模块，整合做市商、定价、信用、OTC 买单与 Swap 卖单：

| Crate | Index | 功能 |
|-------|:-----:|------|
| `nex-market` | 56 | 买单（法币→NEX）+ 卖单（NEX→法币），做市商管理、CNY/USD 汇率定价、买卖方信用评分、价格保护 |
| `trading-common` | — | 共享 trait（PricingProvider / PriceOracle / ExchangeRateProvider / DepositCalculator） |
| `trading-trc20-verifier` | — | TRC20 USDT 链上交易验证（Off-Chain Worker，TronGrid API） |

### 3. GroupRobot — TEE 去中心化社群管理（6 Pallets + 离链 TEE）

**链上 Pallet（6 个）：**

| Pallet | Index | 功能 |
|--------|:-----:|------|
| `grouprobot-registry` | 150 | Bot 注册、TEE 证明（DCAP）、MRTD/MRENCLAVE 白名单、Operator 管理 |
| `grouprobot-consensus` | 151 | 节点注册/质押/退出、序列去重、TEE 加权 |
| `grouprobot-community` | 152 | 群规则配置（CAS 乐观锁）、Action Log 单条/批量提交 |
| `grouprobot-ceremony` | 153 | RA-TLS 仪式记录/撤销、Enclave 审批、仪式过期检测 |
| `grouprobot-subscription` | 154 | 订阅计划管理、到期处理 |
| `grouprobot-rewards` | 155 | 节点奖励分配、Era 结算 |

> 共享类型: `grouprobot-primitives` 提供 `BotRegistryProvider` 等 trait 定义。

**离链执行程序（`grouprobot/`）：**

独立 Rust 二进制，运行在 TDX/SGX 可信执行环境中：

- **平台适配**: Telegram（Webhook + Bot API）+ Discord（Gateway WebSocket + REST API）
- **TEE 安全层**: EnclaveBridge (Ed25519) + SealedStorage (AES-GCM) + TokenVault (Zeroizing+mlock)
- **密钥恢复**: Shamir K-of-N 分片 → Peer RA-TLS 收集 → 环境变量 fallback
- **证明系统**: TDX + SGX 双 Quote 生成，24h 自动刷新
- **规则引擎**: 5 条可插拔规则链（防刷屏/黑名单/命令/加群/默认）
- **链交互**: subxt 动态调用，ActionLogBatcher 批量提交（6s 窗口）
- **进程隔离**: Vault IPC（加密 Unix socket）+ Gramine SGX 部署

### 4. Ads 广告系统（3 Pallets）

去中心化广告投放与结算：

| Pallet | Index | 功能 |
|--------|:-----:|------|
| `ads-core` | 160 | 广告活动 CRUD、资金托管、交付验证、结算 |
| `ads-grouprobot` | 161 | GroupRobot 渠道广告投放 |
| `ads-entity` | 162 | Entity 渠道广告投放 |

> 共享类型: `ads-primitives`；路由逻辑: `ads-router`（库 crate）。

### 5. 争议解决（3 Pallets）

| Pallet | Index | 功能 |
|--------|:-----:|------|
| `escrow` | 60 | 多方资金托管、条件释放、到期处理 |
| `evidence` | 63 | 链上证据存证（IPFS CID）、隐私内容、访问控制 |
| `arbitration` | 64 | 投诉提交、双向押金、域路由、仲裁裁决（部分/全额退款/释放） |

### 6. 去中心化存储（2 Pallets）

| Pallet | Index | 功能 |
|--------|:-----:|------|
| `storage-service` | 62 | IPFS 文件存储注册、Operator 管理、计费、健康检查 |
| `storage-lifecycle` | 65 | 归档管线（Active → L1 → L2 → Purge） |

### 7. 链上治理

4 个委员会实例（基于 `pallet-collective` + `pallet-membership`）：

| 委员会 | Index | 职能 |
|--------|:-----:|------|
| 技术委员会 | 70-71 | 协议升级与技术决策 |
| 仲裁委员会 | 72-73 | 争议裁决 |
| 财务委员会 | 74-75 | 国库资金管理 |
| 内容委员会 | 76-77 | 内容审核 |

## 项目结构

```
nexus/
├── node/                           # Substrate 节点 (CLI · RPC · 共识)
├── runtime/                        # WASM Runtime (51 pallets)
│   └── src/
│       ├── lib.rs                  #   区块参数、pallet 注册
│       ├── configs/                #   各 pallet Config 实现
│       └── genesis_config_presets/ #   创世配置
├── pallets/                        # 自定义 Pallet
│   ├── entity/                     #   Entity 商业平台 (22 crate)
│   │   ├── common/                 #     共享类型与 trait
│   │   ├── registry/               #     实体注册
│   │   ├── shop/                   #     店铺管理
│   │   ├── product/                #     商品/服务
│   │   ├── order/                  #     订单管理
│   │   ├── review/                 #     评价系统
│   │   ├── token/                  #     实体代币
│   │   ├── governance/             #     实体治理
│   │   ├── member/                 #     会员体系
│   │   ├── market/                 #     内部市场
│   │   ├── disclosure/             #     信息披露
│   │   ├── kyc/                    #     KYC 认证
│   │   ├── tokensale/              #     代币销售
│   │   └── commission/             #     佣金引擎
│   │       ├── common/             #       共享类型 + CommissionPlugin trait
│   │       ├── core/               #       调度引擎
│   │       ├── referral/           #       推荐链佣金
│   │       ├── multi-level/        #       多级分佣
│   │       ├── level-diff/         #       级差佣金
│   │       ├── single-line/        #       单线佣金
│   │       ├── team/               #       团队佣金
│   │       └── pool-reward/        #       池化奖励
│   ├── trading/                    #   NEX Market 交易 (3 crate)
│   │   ├── common/                 #     共享 trait
│   │   ├── nex-market/             #     统一 P2P 交易
│   │   └── trc20-verifier/         #     TRC20 验证 (OCW)
│   ├── dispute/                    #   争议解决 (3 crate)
│   │   ├── escrow/                 #     资金托管
│   │   ├── evidence/               #     证据存证
│   │   └── arbitration/            #     仲裁裁决
│   ├── storage/                    #   去中心化存储 (2 crate)
│   │   ├── service/                #     IPFS 存储
│   │   └── lifecycle/              #     生命周期管理
│   ├── grouprobot/                 #   GroupRobot 链上 (7 crate)
│   │   ├── primitives/             #     共享类型与 Trait
│   │   ├── registry/               #     Bot 注册与 TEE 证明
│   │   ├── consensus/              #     节点共识
│   │   ├── community/              #     群规则与审计
│   │   ├── ceremony/               #     RA-TLS 仪式
│   │   ├── subscription/           #     订阅管理
│   │   └── rewards/                #     奖励分配
│   └── ads/                        #   广告系统 (5 crate)
│       ├── primitives/             #     共享类型
│       ├── core/                   #     广告核心
│       ├── grouprobot/             #     GroupRobot 渠道
│       ├── entity/                 #     Entity 渠道
│       └── router/                 #     广告路由
├── grouprobot/                     # GroupRobot TEE 离链执行 (独立二进制)
│   ├── src/                        #   源码
│   │   ├── chain/                  #     subxt 链客户端
│   │   ├── tee/                    #     TEE 安全层
│   │   ├── platform/               #     Telegram + Discord 适配
│   │   ├── processing/             #     规则引擎
│   │   └── infra/                  #     指标/限流/缓存
│   └── gramine/                    #   SGX Gramine 部署配置
├── common/                         # 共享库
│   ├── crypto/                     #   加密工具
│   └── media/                      #   媒体工具
├── scripts/                        # 测试脚本与 E2E 框架
│   └── e2e/                        #   E2E 集成测试 (35 文件)
│       ├── core/                   #     Runner · Reporter · Assertions
│       └── flows/                  #     entity · trading · dispute · grouprobot · storage · ads
├── docs/                           # 架构与审计文档
├── .github/                        # CI/CD 工作流
│   ├── workflows/                  #   ci.yml · release.yml · pr-reminder.yml
│   └── actions/                    #   ubuntu/macOS 依赖 · 磁盘清理
├── Cargo.toml                      # Workspace (46 成员)
└── Dockerfile                      # 区块链节点 Docker 镜像
```

## Runtime Pallet 索引

| 索引 | Pallet | 类别 |
|:----:|--------|------|
| 0 | System | 基础 |
| 1 | Timestamp | 基础 |
| 2 | Aura | 共识 (出块) |
| 3 | Grandpa | 共识 (终局) |
| 4 | Balances | 基础 |
| 5 | TransactionPayment | 基础 |
| 6 | Sudo | 管理 |
| 56 | NexMarket | 交易 |
| 60 | Escrow | 争议 |
| 62 | StorageService | 存储 |
| 63 | Evidence | 争议 |
| 64 | Arbitration | 争议 |
| 65 | StorageLifecycle | 存储 |
| 70-71 | TechnicalCommittee / Membership | 治理 |
| 72-73 | ArbitrationCommittee / Membership | 治理 |
| 74-75 | TreasuryCouncil / Membership | 治理 |
| 76-77 | ContentCommittee / Membership | 治理 |
| 90 | Contracts | 智能合约 |
| 110 | Assets | 资产 |
| 120 | EntityRegistry | Entity |
| 121 | EntityProduct | Entity |
| 122 | EntityOrder | Entity |
| 123 | EntityReview | Entity |
| 124 | EntityToken | Entity |
| 125 | EntityGovernance | Entity |
| 126 | EntityMember | Entity |
| 127 | CommissionCore | Entity — 佣金 |
| 128 | EntityMarket | Entity |
| 129 | EntityShop | Entity |
| 130 | EntityDisclosure | Entity |
| 131 | EntityKyc | Entity |
| 132 | EntityTokenSale | Entity |
| 133 | CommissionReferral | Entity — 佣金 |
| 134 | CommissionLevelDiff | Entity — 佣金 |
| 135 | CommissionSingleLine | Entity — 佣金 |
| 136 | CommissionTeam | Entity — 佣金 |
| 137 | CommissionPoolReward | Entity — 佣金 |
| 138 | CommissionMultiLevel | Entity — 佣金 |
| 150 | GroupRobotRegistry | GroupRobot |
| 151 | GroupRobotConsensus | GroupRobot |
| 152 | GroupRobotCommunity | GroupRobot |
| 153 | GroupRobotCeremony | GroupRobot |
| 154 | GroupRobotSubscription | GroupRobot |
| 155 | GroupRobotRewards | GroupRobot |
| 160 | AdsCore | 广告 |
| 161 | AdsGroupRobot | 广告 |
| 162 | AdsEntity | 广告 |

## 忽略文件 (.gitignore)

项目 `.gitignore` 涵盖以下类别，确保构建产物、敏感数据与本地配置不被提交：

| 类别 | 说明 |
|------|------|
| **Rust / Cargo** | `target/`、增量编译、Cargo 缓存、`*.rs.bk` |
| **Substrate** | 链数据 `chains/`、`node-data/`、WASM 产物、创世/链规格、keystore |
| **Node.js** | `node_modules/`、锁文件、`.npm/`、`.yarn/` |
| **前端** | `frontend/.next/`、`website/.next/`、`.vercel`、`.tsbuildinfo` |
| **IDE / AI** | `.idea/`、`.vscode/`、`.cursor/`、`.claude/` 等 |
| **环境与密钥** | `.env`、`*.pem`、`*.key`、`secrets/` |
| **构建与缓存** | `build/`、`out/`、`dist/`、`coverage/` |
| **脚本与测试** | `scripts/*.log`、`scripts/reports/` |
| **基础设施** | `*.tfstate`、`.terraform/`、`kubeconfig` |
| **其他** | `telegram/`、`my-chain-state/`、`grouprobot/.env`、`media-utils/output/` |

> `Cargo.lock` 已保留，用于可重现构建。`runtime/src/**/*.wasm` 为源码中的 WASM 例外保留。

## 快速开始

### 获取源码

```bash
git clone https://github.com/nexusmalls/nexus.git
cd nexus
```

### 环境要求

- **Linux / macOS** 开发环境（推荐 Ubuntu 22.04+）
- **Rust** stable（含 `wasm32-unknown-unknown` target）
- **Clang / LLVM**、`libssl-dev`、`pkg-config`、`build-essential`
- **protobuf-compiler**（部分依赖与工具链需要）
- **Node.js** 18+（E2E 测试脚本）
- **Docker**（可选，容器化部署）

### 安装 Substrate 标准开发环境

```bash
# Ubuntu / Debian
sudo apt update
sudo apt install -y git clang curl libssl-dev llvm libudev-dev make protobuf-compiler pkg-config build-essential

# 安装 Rust toolchain
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source "$HOME/.cargo/env"

# 安装 WASM target
rustup target add wasm32-unknown-unknown
```

如使用 macOS，可先安装 Xcode Command Line Tools 与 Homebrew，再安装 `llvm`、`openssl`、`protobuf`、`pkg-config` 等依赖。

### 构建与运行

```bash
# 构建节点
cargo build --release

# 启动开发链（单节点）
./target/release/nexus-node --dev

# 连接 Polkadot.js Apps
# https://polkadot.js.org/apps/#/explorer?rpc=ws://localhost:9944
```

### 运行测试

```bash
# 全部 pallet 单元测试
cargo test

# 指定 pallet
cargo test -p pallet-nex-market
cargo test -p pallet-entity-token
cargo test -p pallet-commission-core
cargo test -p pallet-grouprobot-consensus
cargo test -p pallet-ads-core

# GroupRobot 离链执行测试（独立 workspace）
cd grouprobot && cargo test

# E2E 集成测试（需先启动开发链）
cd scripts && npm run e2e
```

### Docker 部署

```bash
# 构建节点镜像
docker build . -t nexus-node

# 运行开发链
docker run -p 9944:9944 -p 30333:30333 nexus-node --dev --rpc-external
```

## 链参数

| 参数 | 值 |
|------|-----|
| **代币符号** | NEX |
| **精度** | 12 位小数（1 NEX = 10¹² 单位） |
| **初始供应** | 100,000,000,000 NEX |
| **存在性押金** | 0.001 NEX |
| **出块时间** | 6 秒 |
| **出块共识** | Aura |
| **终局共识** | GRANDPA |
| **SS58 格式** | 42 |
| **Runtime 名称** | nexus |
| **Spec 版本** | 100 |

## 技术栈

### 区块链层

- **Polkadot SDK (Substrate)** — Runtime 框架
- **Rust** — 全部链上逻辑与节点
- **FRAME** — Pallet 开发框架
- **Aura + GRANDPA** — 混合共识
- **pallet-contracts** — ink! 智能合约
- **pallet-assets** — 多资产管理

### GroupRobot 离链层

- **Axum 0.7** — HTTP 服务
- **Tokio** — 异步运行时
- **subxt 0.38** — Substrate 链交互（动态 API）
- **ed25519-dalek 2** — 消息签名
- **aes-gcm 0.10** — 密封存储 + IPC 加密
- **tokio-tungstenite** — Discord Gateway WebSocket
- **prometheus-client** — Prometheus 指标
- **tikv-jemallocator** — jemalloc（zero-on-free 安全分配）

### E2E 测试

- **TypeScript** + **Polkadot.js API** — 集成测试脚本
- 覆盖 Entity / Trading / Dispute / GroupRobot / Storage / Ads 全流程

## 文档

| 文档 | 路径 |
|------|------|
| NEX Market 审计 | `docs/NEX_MARKET_AUDIT.md` |
| Ads Pallet 审计 | `docs/ADS_PALLETS_AUDIT.md` |
| GroupRobot 广告审计 | `docs/GROUPROBOT_ADS_AUDIT.md` |
| GroupRobot 奖励审计 | `docs/GROUPROBOT_REWARDS_AUDIT.md` |
| GroupRobot 订阅审计 | `docs/GROUPROBOT_SUBSCRIPTION_AUDIT.md` |
| Entity 订单/TokenSale/Disclosure 审计 | `docs/ENTITY_ORDER_TOKENSALE_DISCLOSURE_AUDIT.md` |
| Entity 广告开发计划 | `docs/ENTITY_ADS_DEV_PLAN.md` |
| Entity 链接口提取 | `docs/ENTITY_CHAIN_INTERFACE_EXTRACTION.md` |
| Entity 锁定设计 | `docs/ENTITY_LOCK_DESIGN.md` |
| Entity 主网缺失功能 | `docs/ENTITY_MAINNET_MISSING_FEATURES.md` |
| IPFS 存储集成方案 | `docs/IPFS_STORAGE_INTEGRATION_PLAN.md` |
| 知识库设计 | `docs/KNOWLEDGE_BASE_DESIGN.md` |
| E2E 测试计划 | `scripts/docs/NEXUS_TEST_PLAN.md` |
| E2E 实施计划 | `scripts/docs/IMPLEMENTATION_PLAN.md` |

## CI/CD

| 工作流 | 触发 | 内容 |
|--------|------|------|
| `ci.yml` | PR / push to main | 构建、Clippy、测试、文档、节点启动验证、Docker 构建（Ubuntu + macOS） |
| `release.yml` | GitHub Release | Docker 镜像推送 ghcr.io、二进制上传 Release Assets |
| `pr-reminder.yml` | 新 PR | PR 提醒 |

## 许可证

MIT-0
