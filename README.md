# Nexus Blockchain

**连接、聊天、共建链上社交** — 基于 [Polkadot SDK (Substrate)](https://github.com/paritytech/polkadot-sdk) 的 Layer-1 区块链，以端到端加密即时通讯（NexChat）为核心入口，在社交网络中自然延伸链上商城（Entity）与链上游戏，让关系、交易与娱乐在同一生态中发生。

- **官网**：[nexuschain.network](https://nexuschain.network/)
- **Runtime**：67 个 Pallet（42 个自定义）
- **Workspace**：55 个 crate

## 生态定位

NEXUS 以**聊天社交**为核心，**链上商城**与**链上游戏**为自然延伸，共享同一链上身份与代币经济。

```
链上身份 → 加密聊天 → 社群互动 → 商城成交 → 游戏激励 → 治理扩展 → 更多连接
```

| 支柱 | 产品 | 定位 |
|------|------|------|
| **核心** | NexChat | E2EE 私聊 · MLS 群聊 · 链上权限 · 云同步锚定 |
| **延伸** | Entity 链上商城 | 通证化商品 · 店铺 · 透明分佣 · DAO 治理 |
| **延伸** | 链上游戏 | 群聊竞技 · 链上任务 · 代币互通 · 可治理规则 |

### 适用角色

| 角色 | 入口 | 能力 |
|------|------|------|
| 聊天用户 | NexChat | E2EE 私聊、MLS 群聊、链上身份、换机云同步 |
| 社群建设者 | 社群增长 | 加密群组、透明分佣、活动与广告分发 |
| 商家 / 项目方 | 链上商城 | 社群内开店、通证化商品、链上结算与治理 |
| 运营方 | GroupRobot | TEE 保护 Bot、定价监控、治理辅助 |
| 开发者 | 本仓库 | Pallet、Runtime API、RPC、E2E 集成 |

## 系统架构

```
┌─────────────────────────────────────────────────────────────────────────┐
│                    Nexus Blockchain (L1)                                 │
│         Polkadot SDK · Babe + GRANDPA · Staking · 6 s Block · WASM      │
├─────────────────────────────────────────────────────────────────────────┤
│                         Runtime — 67 Pallets                             │
│                                                                         │
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐  ┌─────────────┐ │
│  │ NexChat 聊天  │  │ Entity 商城  │  │ GroupRobot   │  │  Ads 广告   │ │
│  │  6 pallets   │  │  20 pallets  │  │  6 pallets   │  │  3 pallets  │ │
│  │ E2EE·MLS·   │  │ 实体·代币·   │  │ TEE Bot·     │  │ 活动投放·   │ │
│  │ 权限·同步   │  │ 治理·分佣    │  │ 共识·订阅    │  │ 渠道结算    │ │
│  └──────────────┘  └──────────────┘  └──────────────┘  └─────────────┘ │
│                                                                         │
│  ┌──────────────┐  ┌──────────────┐  ┌────────────────────────────────┐ │
│  │ NEX Market   │  │ 争议 · 存储  │  │ Substrate 基础层               │ │
│  │  1 pallet    │  │  5 pallets   │  │ System · Balances · Staking ·  │ │
│  │ P2P 交易     │  │ 托管·证据·   │  │ Contracts · Assets · 4 委员会  │ │
│  │ 信用·TRC20   │  │ IPFS·生命周期│  │ (技术·仲裁·财务·内容)         │ │
│  └──────────────┘  └──────────────┘  └────────────────────────────────┘ │
├─────────────────────────────────────────────────────────────────────────┤
│  NexChat 客户端（Web / Android）          GroupRobot TEE 离链执行        │
│  E2EE · MLS WASM · Relay · IPFS 同步      Telegram + Discord · subxt    │
└─────────────────────────────────────────────────────────────────────────┘
```

### NexChat 三层架构

链上锚定、链下加密、Relay 投递，职责清晰不重叠：

| 层级 | 职责 |
|------|------|
| **链上层** | ChatUserId、场景授权、MLS Commit 锚定、Inbox epoch、EISA 同步锚点 — **消息正文不上链** |
| **客户端** | NexChat：账户主密钥派生、MLS WASM、IndexedDB 本地缓存、Polkadot 扩展签名 |
| **基础设施** | Relay 热路径投递、IPFS 加密 blob、运维 GPG 整库备份 |

> 隐私承诺：人类消息正文与 MLS 群状态密文不上链；Blind-RSA 私钥与敏感密钥仅存客户端。详见 `pallets/chat/README.md`。

## 核心子系统

### 1. NexChat 聊天社交（6 Pallets + 1 Library）

| Crate / Pallet | Index | 功能 |
|----------------|:-----:|------|
| `chat-permission` | 67 | 场景授权、隐私级别、平台禁言、能力 epoch |
| `chat-core` | 68 | ChatUserId、System 通知、会话元数据（人类消息链下） |
| `chat-group` | 69 | MLS（RFC 9420）群锚定：KeyPackage、Commit、Welcome |
| `chat-inbox` | 78 | 链下投递信箱：inbox epoch、定向标签撤销 |
| `chat-sync` | 79 | 账户派生加密同步锚 EISA |
| `msg-identity` | 80 | X3DH 预密钥锚（IK/SPK/OPK） |
| `chat-common` | — | RateLimit、`ChatViewApi` Runtime API 定义 |

**Runtime API / RPC**：`ChatViewApi` 聚合私聊与群聊视图；node 封装为 `chat_*` RPC，客户端 Merge 链上链下状态。

### 2. Entity 链上商城（20 Pallets）

完整的去中心化商业实体全栈，在社交网络中自然延伸商业能力。

**核心业务**

| Pallet | Index | 功能 |
|--------|:-----:|------|
| `entity-registry` | 120 | 实体创建、审核、暂停、关闭、转让 |
| `entity-shop` | 129 | 主店铺/子店铺、运营资金、统计级联 |
| `entity-product` | 121 | 商品与服务上架、管理 |
| `entity-order` | 122 | 订单创建、支付、履约、完成 |
| `entity-review` | 123 | 订单评价、店铺评分聚合 |

**代币经济**

| Pallet | Index | 功能 |
|--------|:-----:|------|
| `entity-token` | 124 | 7 类代币铸造、分红、锁仓、转账限制 |
| `entity-governance` | 125 | DAO 提案、时间加权投票、委托、委员会否决 |
| `entity-market` | 128 | 实体代币 NEX/USDT 双通道交易、TWAP、熔断 |
| `entity-tokensale` | 132 | 多轮次 Token Sale、白名单、退款、Vesting |
| `entity-disclosure` | 130 | 内幕人管理、交易窗口期控制 |
| `entity-loyalty` | 139 | 会员忠诚度与权益 |

**用户与分佣**

| Pallet | Index | 功能 |
|--------|:-----:|------|
| `entity-member` | 126 | 多级会员、推荐链、自动升级、团队统计 |
| `entity-kyc` | 131 | 多级身份认证（KYC/AML） |
| `commission-core` | 127 | 调度引擎、存储、提现（4 种模式） |
| `commission-referral` | 133 | 推荐链佣金 |
| `commission-multi-level` | 138 | 多级分佣 |
| `commission-level-diff` | 134 | 级差佣金 |
| `commission-single-line` | 135 | 单线上下级佣金 |
| `commission-team` | 136 | 团队佣金 |
| `commission-pool-reward` | 137 | 池化奖励 |

> 共享库：`entity-common`、`commission-common` 不直接注册 Runtime。

### 3. NEX Market 交易系统（1 Pallet + 2 Library）

| Crate | Index | 功能 |
|-------|:-----:|------|
| `nex-market` | 56 | 买单（法币→NEX）+ 卖单（NEX→法币），做市商、信用评分、价格保护 |
| `trading-common` | — | PricingProvider / PriceOracle / ExchangeRateProvider |
| `trading-trc20-verifier` | — | TRC20 USDT 链上验证（OCW，TronGrid API） |

### 4. GroupRobot — TEE 社群运营自动化（6 Pallets + 离链 TEE）

**链上**

| Pallet | Index | 功能 |
|--------|:-----:|------|
| `grouprobot-registry` | 150 | Bot 注册、TEE 证明（DCAP）、MRTD/MRENCLAVE 白名单 |
| `grouprobot-consensus` | 151 | 节点注册/质押/退出、序列去重、TEE 加权 |
| `grouprobot-community` | 152 | 群规则配置、Action Log 单条/批量提交 |
| `grouprobot-ceremony` | 153 | RA-TLS 仪式记录/撤销、Enclave 审批 |
| `grouprobot-subscription` | 154 | 订阅计划管理、到期处理 |
| `grouprobot-rewards` | 155 | 节点奖励分配、Era 结算 |

**离链执行（`grouprobot/`）**：Telegram + Discord 适配、TDX/SGX 双证明、Gramine SGX、subxt 链交互、规则引擎、Vault IPC。

### 5. Ads 广告系统（3 Pallets）

| Pallet | Index | 功能 |
|--------|:-----:|------|
| `ads-core` | 160 | 广告活动 CRUD、资金托管、交付验证、结算 |
| `ads-grouprobot` | 161 | GroupRobot 渠道广告投放 |
| `ads-entity` | 162 | Entity 渠道广告投放 |

### 6. 争议解决与存储（5 Pallets）

| Pallet | Index | 功能 |
|--------|:-----:|------|
| `escrow` | 60 | 多方资金托管、条件释放、到期处理 |
| `evidence` | 63 | 链上证据存证（IPFS CID）、隐私内容、访问控制 |
| `arbitration` | 64 | 投诉提交、双向押金、域路由、仲裁裁决 |
| `storage-service` | 62 | IPFS 文件存储注册、Operator 管理、计费 |
| `storage-lifecycle` | 65 | 归档管线（Active → L1 → L2 → Purge） |

### 7. 链上治理

4 个委员会实例（`pallet-collective` + `pallet-membership`）：

| 委员会 | Index | 职能 |
|--------|:-----:|------|
| 技术委员会 | 70–71 | 协议升级与技术决策 |
| 仲裁委员会 | 72–73 | 争议裁决 |
| 财务委员会 | 74–75 | 国库资金管理 |
| 内容委员会 | 76–77 | 内容审核 |

## 项目结构

```
nexus/
├── node/                           # Substrate 节点 (CLI · RPC · Chat RPC)
├── runtime/                        # WASM Runtime (67 pallets)
├── pallets/
│   ├── chat/                       # NexChat 聊天社交 (7 crate)
│   │   ├── common/                 #   RateLimit + ChatViewApi
│   │   ├── permission/             #   场景授权与能力 epoch
│   │   ├── core/                   #   私聊核心
│   │   ├── group/                  #   MLS 群聊锚定
│   │   ├── inbox/                  #   链下投递信箱
│   │   ├── sync/                   #   EISA 云同步锚
│   │   └── msg-identity/           #   X3DH 预密钥锚
│   ├── entity/                     #   Entity 链上商城
│   ├── trading/                    #   NEX Market (3 crate)
│   ├── dispute/                    #   争议解决 (3 crate)
│   ├── storage/                    #   去中心化存储 (2 crate)
│   ├── grouprobot/                 #   GroupRobot 链上 (7 crate)
│   ├── ads/                        #   广告系统 (5 crate)
│   └── inscription/                #   创世铭文
├── grouprobot/                     # GroupRobot TEE 离链执行 (独立 workspace)
├── website/                        # 官网 (Next.js)
├── common/                         # 共享库 (crypto · media)
├── scripts/                        # E2E 测试框架
│   └── e2e/                        #   entity · trading · dispute · grouprobot · storage · ads
├── docs/                           # 架构与审计文档
└── Dockerfile                      # 节点 Docker 镜像
```

## Runtime Pallet 索引（摘要）

完整列表见 `runtime/src/lib.rs`。

| 索引 | Pallet | 类别 |
|:----:|--------|------|
| 0–15 | System · Timestamp · Babe · Grandpa · Balances · … · Staking · Inscription | 基础 |
| 56 | NexMarket | 交易 |
| 60–65 | Escrow · StorageService · Evidence · Arbitration · StorageLifecycle | 争议 / 存储 |
| 67–69, 78–80 | ChatPermission · ChatCore · ChatGroup · ChatInbox · ChatSync · MsgIdentity | NexChat |
| 70–77 | 4 × (Collective + Membership) | 治理 |
| 90 | Contracts | 智能合约 |
| 110 | Assets | 资产 |
| 120–139 | Entity 商业栈 + Commission 引擎 | 商城 |
| 150–155 | GroupRobot | 社群自动化 |
| 160–162 | AdsCore · AdsGroupRobot · AdsEntity | 广告 |

## 快速开始

### 获取源码

```bash
git clone https://github.com/nexusmalls/nexus.git
cd nexus
```

### 环境要求

- **Linux / macOS**（推荐 Ubuntu 22.04+）
- **Rust** stable + `wasm32-unknown-unknown`
- **Clang / LLVM**、`libclang-dev`、`libssl-dev`、`pkg-config`、`build-essential`
- **protobuf-compiler**
- **Node.js** 18+（E2E 与官网）
- **Docker**（可选）

### 安装开发环境

```bash
# Ubuntu / Debian
sudo apt update && sudo apt install -y \
  clang libclang-dev git curl libssl-dev llvm libudev-dev \
  make protobuf-compiler pkg-config build-essential

curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source "$HOME/.cargo/env"
rustup target add wasm32-unknown-unknown
```

### 构建与运行

```bash
cargo build --release
./target/release/nexus-node --dev

# Polkadot.js Apps
# https://polkadot.js.org/apps/#/explorer?rpc=ws://localhost:9944
```

### 运行测试

```bash
# 全部单元测试
cargo test

# 指定模块
cargo test -p pallet-chat-core
cargo test -p pallet-chat-group
cargo test -p pallet-entity-token
cargo test -p pallet-nex-market
cargo test -p pallet-grouprobot-consensus
cargo test -p pallet-ads-core

# GroupRobot 离链（独立 workspace）
cd grouprobot && cargo test

# E2E（需先启动开发链）
cd scripts && npm run e2e
```

### Docker

```bash
docker build . -t nexus-node
docker run -p 9944:9944 -p 30333:30333 nexus-node --dev --rpc-external
```

### 官网本地预览

```bash
cd website && npm install && npm run dev
```

## 链参数

| 参数 | 值 |
|------|-----|
| **代币符号** | NEX |
| **精度** | 12 位小数（1 NEX = 10¹² 单位） |
| **存在性押金** | 0.001 NEX |
| **出块时间** | 6 秒 |
| **出块共识** | Babe |
| **终局共识** | GRANDPA |
| **质押** | Nominated Proof-of-Stake |
| **SS58 格式** | 42 |
| **Runtime 名称** | nexus |
| **Spec 版本** | 103 |

## 技术栈

| 层级 | 技术 |
|------|------|
| **区块链** | Polkadot SDK · FRAME · Rust · WASM |
| **共识** | Babe + GRANDPA + Staking |
| **智能合约** | pallet-contracts（ink!） |
| **NexChat** | MLS RFC 9420 · X3DH · E2EE · ChatView Runtime API |
| **GroupRobot** | Axum · Tokio · subxt · TDX/SGX · Gramine |
| **存储** | IPFS 集成 · 生命周期归档 |
| **E2E** | TypeScript · Polkadot.js API |
| **官网** | Next.js · Tailwind CSS |

## 文档

| 文档 | 路径 |
|------|------|
| NexChat 模块总览 | `pallets/chat/README.md` |
| NexChat 1:1 设计 | `pallets/chat/CHAT_1TO1_X3DH_DOUBLE_RATCHET_DESIGN.md` |
| MLS 群聊设计 | `pallets/chat/CHAT_GROUP_WIREIFY_DESIGN.md` |
| 多设备同步 | `pallets/chat/CHAT_MULTIDEVICE_MLS_SYNC_DESIGN.md` |
| E2E 测试计划 | `scripts/docs/NEXUS_TEST_PLAN.md` |
| NEX Market 审计 | `docs/NEX_MARKET_AUDIT.md` |
| Ads Pallet 审计 | `docs/ADS_PALLETS_AUDIT.md` |
| GroupRobot 审计 | `docs/GROUPROBOT_ADS_AUDIT.md` |
| Entity 主网缺失功能 | `docs/ENTITY_MAINNET_MISSING_FEATURES.md` |
| IPFS 存储方案 | `docs/IPFS_STORAGE_INTEGRATION_PLAN.md` |

## CI/CD

| 工作流 | 触发 | 内容 |
|--------|------|------|
| `ci.yml` | PR / push to main | 构建、Clippy、测试、文档、节点启动、Docker（Ubuntu + macOS） |
| `release.yml` | GitHub Release | Docker 推送 ghcr.io、二进制上传 Release Assets |
| `pr-reminder.yml` | 新 PR | PR 提醒 |

## 许可证

MIT-0
