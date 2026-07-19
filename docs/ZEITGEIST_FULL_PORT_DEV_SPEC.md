# Zeitgeist 全量移植到 Nexus 开发规范

> Status: Design / 状态：设计稿  
> Target: Nexus standalone L1 runtime  
> Upstream baseline: `/home/xiaodong/文档/zeitgeist`, `main` @ `39ad8d60`  
> Nexus baseline: 当前 Nexus `main`，FRAME 45.x + ISMP 2512  
> Scope note: 本文按要求不讨论许可证问题。
> Development readiness: 仅 Phase 0 可立即开工；通过第 21 节三项门禁后才允许批量移植。

## 1. 目标与完成定义

本项目把 Zeitgeist 当前仓库中的全部预测市场业务模块移植到 Nexus，并保持 Nexus 现有
NexChat、Entity、NEX Market、争议、广告、GroupRobot、USDX 和 ISMP 功能不回退。

“全量移植”定义为：

1. 移植 `primitives`、`macros` 和全部 13 个 `zrml` pallet。
2. 移植 prediction-markets、swaps 的 runtime API 与 swaps RPC，并补齐 Nexus 查询 API。
3. 保留 categorical、scalar、complete-set、组合代币、LMSR、订单簿、混合路由、
   parimutuel、Authorized、Court、Global Disputes、Futarchy、legacy swaps 和 Styx 功能。
4. 所有模块升级到 Nexus 使用的 FRAME 版本，进入同一个 Nexus runtime WASM。
5. 上游单测、迁移后的 fuzz、Nexus runtime benchmark 和 E2E 全部通过。
6. 现有 Nexus pallet index、storage、Runtime API 和业务行为保持兼容。

以下内容不属于“业务模块全量移植”：

- Zeitgeist node、chain spec、Battery Station runtime、Zeitgeist runtime。
- Nimbus、Moonkit、parachain staking、Cumulus、XCM 和 ORML XTokens。
- Zeitgeist 主网历史状态及其历史 staking/XCM migrations。
- Zeitgeist 网络参数、tokenomics、SS58、ZTG 名称和主网 genesis。

因此本项目是 **Zeitgeist 业务栈在 Nexus L1 上的完整功能移植**，不是把 Nexus 改造成
Zeitgeist parachain。

## 2. 强制架构决策

### 2.1 保持 Nexus L1 架构

- 共识继续使用 BABE + GRANDPA + NPoS。
- 块时间继续为 6 秒。
- 跨链继续使用 ISMP/Hyperbridge。
- 不引入 Cumulus、Nimbus 或 XCM runtime wiring。
- Zeitgeist `parachain` feature 在移植 crate 中永久关闭。

### 2.2 使用隔离的 ORML 预测资产域

为最大程度保留上游行为和测试，预测市场内部继续使用：

- `orml-currencies`
- `orml-tokens`
- `orml-traits`

但不使用：

- `orml-asset-registry`
- `orml-unknown-tokens`
- `orml-xcm-support`
- `orml-xtokens`

资产职责：

```text
Balances
  └── Native/NEX

pallet-assets
  ├── USDX = 900_000
  ├── Entity tokens = 1_000_000+
  └── ISMP/HFT receipt assets

ORML PredictionTokens
  ├── categorical/scalar outcome assets
  ├── pool shares
  ├── parimutuel shares
  ├── combinatorial tokens
  └── mirrored foreign collateral
```

NEX 通过 `orml-currencies` 的 native currency adapter 直接路由到 `Balances`，不得产生第二份
NEX ledger。非原生预测资产由 `orml-tokens` 管理。

#### 2.2.1 ORML/FRAME 依赖锁定门禁

Zeitgeist 上游使用
`zeitgeistpm/open-runtime-module-library:zeitgeist-polkadot-stable2409`，该分支不能直接进入
Nexus FRAME 45.x 依赖图。Phase 0 必须在以下方案中选定且只选定一种：

1. 使用已经与 Nexus FRAME 45.x 完全一致的 ORML revision。
2. 在 Nexus 仓库内 vendor `orml-currencies`、`orml-tokens`、`orml-traits` 的最小闭包，
   仅实施 FRAME 45 API 适配。
3. 若前两项均不可行，停止批量移植并重新评审资产架构；不得通过同时保留两套 FRAME/SP
   依赖来绕过。

Dependency lock report 必须记录 ORML commit、所有本地 patch、`cargo tree` 输出和以下核心
crate 的唯一版本证明：

```text
frame-support
frame-system
sp-runtime
sp-core
sp-io
parity-scale-codec
scale-info
```

允许存在与 FRAME 无关、经解释的普通依赖重复版本；`cargo tree -d` 非空本身不等于失败。
失败条件是上述核心 crate 出现跨 SDK 重复，或 ORML 与 Nexus runtime 的 trait/type 无法统一。

#### 2.2.2 可编译的资产管理组合

目标 runtime 类型组合固定为：

```rust
type PredictionAsset = Asset<MarketId>;
type PredictionNativeCurrency = BasicCurrencyAdapter<Runtime, Balances, Amount, Balance>;
type PredictionMultiCurrency = PredictionTokens;
type PredictionAssetManager = PredictionCurrencies;
```

其中：

- `orml_tokens::Config::CurrencyId = PredictionAsset`。
- `orml_currencies::Config::MultiCurrency = PredictionTokens`。
- `orml_currencies::Config::NativeCurrency = PredictionNativeCurrency`。
- `orml_currencies::Config::GetNativeCurrencyId = Asset::Native`。
- 各业务 pallet 的 `AssetManager = PredictionCurrencies`。
- 直接要求 fungibles/multi-currency token 后端的 associated type 才使用
  `PredictionTokens`，不得将其误配为 `PredictionCurrencies`。

Phase 0 smoke runtime 必须先证明
`PredictionCurrencies: NamedMultiReservableCurrency<AccountId, CurrencyId = PredictionAsset,
Balance = Balance, ReserveIdentifier = [u8; 8]>`。若目标 ORML revision 不再提供该实现，必须在
依赖报告中列出 trait 差异，并先适配 ORML，不得在 13 个业务 pallet 中分别打补丁。

#### 2.2.3 非 parachain ForeignAsset 准入

上游 `prediction-markets` 仅在 `parachain` feature 下允许 `ForeignAsset` 作为 base asset；
Nexus 永久关闭该 feature，因此必须把“是否可作为预测市场抵押品”从 XCM asset registry
解耦为业务 trait：

```rust
pub trait PredictionBaseAssetPolicy<AssetId> {
    fn is_allowed(asset_id: AssetId) -> bool;
}
```

`zrml-prediction-markets::Config` 新增：

```rust
type BaseAssetPolicy: PredictionBaseAssetPolicy<AssetId>;
```

市场创建时的规则固定为：

```text
Asset::Native            => allowed
Asset::ForeignAsset(id)  => BaseAssetPolicy::is_allowed(id)
all outcome/pool assets  => rejected as base asset
```

生产 runtime 由 `PredictionCollateral` 实现该 trait。`is_allowed(id)` 必须同时满足：

1. 资产位于 prediction collateral whitelist。
2. deposit 未因资产冻结、销毁、bridge/PSM 风险或治理操作而暂停。
3. 对应 `pallet-assets` 资产存在且镜像配置有效。

该路径不得依赖 `orml-asset-registry`、XCM 或 `parachain` feature。mock runtime 使用显式的
静态 policy；禁止用 `Everything` 掩盖准入测试。

### 2.3 增加两个 Nexus 专用适配 pallet

#### `pallet-prediction-control`

负责：

- 子系统启停和分阶段上线。
- 紧急暂停创建、交易、加池和新争议。
- 暂停时仍允许报告、结算、赎回、撤单、撤流动性和取回抵押。
- 为 `BaseCallFilter` 提供只读模式查询。

建议模式：

```rust
pub enum PredictionMode {
    Disabled,
    ResolutionOnly,
    Trading,
    Full,
}
```

全局模式不承担单模块开关职责，另设稳定模块枚举与开关 storage：

```rust
pub enum PredictionModule {
    PredictionMarkets,
    Authorized,
    Court,
    GlobalDisputes,
    LegacySwaps,
    NeoSwaps,
    Orderbook,
    Parimutuel,
    HybridRouter,
    CombinatorialTokens,
    Futarchy,
    Styx,
}
```

`ModuleEnabled<PredictionModule>` 默认全部为 `false`。模块有效状态为：

```text
global mode permits call class
    AND module flag is enabled
```

模式语义固定为：

- `Disabled`：拒绝创建风险敞口的调用；保留撤单、退出、赎回、退款和治理恢复调用。
- `ResolutionOnly`：允许报告、争议、结算、赎回和退出；禁止新市场、新订单和新增流动性。
- `Trading`：允许已启用模块使用 Native NEX 创建和交易；禁止 foreign collateral deposit
  和以 `ForeignAsset` 新建市场。
- `Full`：允许已启用模块按 whitelist 使用 foreign collateral；不自动启用任何模块。

每个 ported pallet 的 dispatchable 必须在接线前归类为
`RiskIncreasing`、`Resolution`、`Unwind` 或 `AdminRecovery`，并生成 call-filter 单元测试矩阵。
治理变更全局模式和模块开关的调用不得被自身过滤。

`on_initialize` 自动关市和自动结算不得被 call filter 停止。

#### `pallet-prediction-collateral`

负责 `pallet-assets` 与 ORML prediction assets 之间的抵押镜像：

```text
deposit(asset_id, amount)
  pallet-assets: user -> prediction collateral sovereign account
  ORML: mint ForeignAsset(asset_id) -> user

withdraw(asset_id, amount)
  ORML: burn ForeignAsset(asset_id) from user
  pallet-assets: sovereign account -> user
```

必须保持：

```text
ORML mirrored issuance(asset_id)
    == pallet-assets escrow balance(asset_id, sovereign account)
```

只有治理白名单内的资产允许镜像。USDX 只有在 `pallet-usdx` protocol asset 已正式启用、
可转账且 PSM 不变量验证通过后才能加入白名单。

### 2.4 不复用同名但不同语义的 Nexus 模块

- Zeitgeist Court 与 Nexus Arbitration 并行存在。
- Zeitgeist Orderbook 与 Nexus EntityMarket/NexMarket 并行存在。
- Zeitgeist Futarchy 与 Nexus EntityGovernance 并行存在。
- Prediction outcome oracle 不得复用 NEX/USD `PricingProvider`。
- 两套系统只能通过明确的 adapter/trait 交互，禁止直接读取对方 storage。

### 2.5 先兼容、后重命名

SDK 升级阶段保留上游 package/crate 名称和文件布局，减少无意义 diff：

- `zeitgeist-primitives`
- `zeitgeist-macros`
- `zrml-*`

Phase 1 至 Phase 3 differential baseline 通过前，资产 variant 继续使用 `Asset::Ztg`。基线通过
后以独立 PR 进行 Nexus 语义清理：

- `Asset::Ztg` 改名为 `Asset::Native`，保持 enum discriminant 顺序。
- UI、metadata 和文档只显示 NEX，不显示 ZTG。
- `ForeignAsset` 的 ID 扩展为 Nexus `u64 AssetId`。
- `BlockNumber` 对齐 Nexus `u32`；业务周期继续使用 runtime 泛型和饱和运算。

不得在 SDK 升级、业务改名和算法重构之间交叉进行。

`ForeignAsset(u32) -> ForeignAsset(u64)` 是首次 Nexus 公开 runtime 之前有意做出的编码差异，
不承诺与 Zeitgeist 对应字段保持 SCALE 字节兼容。必须提供上游 `u32` fixture 到 Nexus `u64`
语义值的 golden test；首次公开 runtime 后禁止再次更改字段宽度。

## 3. 目标目录结构

```text
pallets/prediction/
├── UPSTREAM.md
├── control/
├── collateral/
├── macros/
├── primitives/
├── market-commons/
├── authorized/
├── court/
├── global-disputes/
├── prediction-markets/
│   ├── runtime-api/
│   └── fuzz/
├── combinatorial-tokens/
│   └── fuzz/
├── neo-swaps/
│   └── fuzz/
├── orderbook/
│   └── fuzz/
├── hybrid-router/
├── parimutuel/
├── futarchy/
│   └── fuzz/
├── swaps/
│   ├── runtime-api/
│   ├── rpc/
│   └── fuzz/
└── styx/
```

Runtime 侧：

```text
runtime/src/configs/prediction.rs
runtime/src/apis.rs
runtime/src/benchmarks.rs
runtime/src/lib.rs
runtime/Cargo.toml
node/src/rpc.rs
node/Cargo.toml
scripts/e2e/suites/prediction-*.ts
```

`UPSTREAM.md` 至少记录：

- 上游仓库 URL 和固定 commit。
- 每个移植 crate 的原始路径。
- Nexus 修改分类：SDK、资产、runtime、命名、bug fix。
- 后续同步上游时的 patch 顺序。

## 4. 模块与依赖顺序

| 层 | 模块 | Nexus 适配 |
|----|------|------------|
| L0 | macros | 仅升级 edition/lints |
| L0 | primitives | 类型、`Asset`、跨 pallet traits |
| L1 | control | Nexus 新增，全局模式与暂停 |
| L1 | collateral | Nexus 新增，pallet-assets ↔ ORML 镜像 |
| L1 | market-commons | 市场共享 storage/API |
| L2 | authorized | 映射 Arbitration Committee origin |
| L2 | court | BABE epoch randomness、NEX stake |
| L2 | global-disputes | NEX 锁仓投票 |
| L3 | prediction-markets | 市场状态机、bond、complete-set、结算 |
| L4 | combinatorial-tokens | 组合 token ID、split/merge/redeem |
| L4 | swaps | Legacy Balancer 池，完整保留但默认关闭 |
| L4 | neo-swaps | LMSR、LP tree、组合池 |
| L4 | orderbook | 预测 outcome 订单簿 |
| L4 | parimutuel | 彩池市场 |
| L5 | hybrid-router | NeoSwaps + Orderbook 路由 |
| L5 | futarchy | NeoSwaps decision oracle + Scheduler |
| L5 | styx | NEX burn registry |
| L6 | runtime API/RPC | 查询、报价和客户端接口 |

严格按层合入。禁止在 L0–L3 未稳定前并行接入 L4/L5 到正式 runtime。

## 5. Runtime pallet index

现有 Nexus index 不得修改。Prediction 子系统使用连续保留区间 176–192：

| Index | Runtime alias | Crate |
|------:|---------------|-------|
| 176 | `PredictionControl` | `pallet-prediction-control` |
| 177 | `PredictionCollateral` | `pallet-prediction-collateral` |
| 178 | `PredictionCurrencies` | `orml-currencies` |
| 179 | `PredictionTokens` | `orml-tokens` |
| 180 | `PredictionMarketCommons` | `zrml-market-commons` |
| 181 | `PredictionAuthorized` | `zrml-authorized` |
| 182 | `PredictionCourt` | `zrml-court` |
| 183 | `PredictionGlobalDisputes` | `zrml-global-disputes` |
| 184 | `PredictionMarkets` | `zrml-prediction-markets` |
| 185 | `PredictionLegacySwaps` | `zrml-swaps` |
| 186 | `PredictionNeoSwaps` | `zrml-neo-swaps` |
| 187 | `PredictionOrderbook` | `zrml-orderbook` |
| 188 | `PredictionParimutuel` | `zrml-parimutuel` |
| 189 | `PredictionHybridRouter` | `zrml-hybrid-router` |
| 190 | `PredictionCombinatorialTokens` | `zrml-combinatorial-tokens` |
| 191 | `PredictionFutarchy` | `zrml-futarchy` |
| 192 | `PredictionStyx` | `zrml-styx` |

规则：

- index 一经进入公开 runtime 不得调整或复用。
- 不复制 Zeitgeist 原 index。
- `MarketCommons` 即使无 call 也保留独立 index。
- runtime upgrade 前必须生成 metadata diff，确认 0–175 编码未变化。

## 6. 核心类型与 SCALE 约束

目标类型：

```text
AccountId    = Nexus AccountId32
Balance      = u128
BlockNumber  = u32
Moment       = u64 milliseconds
MarketId     = u128
PoolId       = u128
AssetId      = u64
CategoryIndex = u16
CombinatorialId = 32 bytes
```

`PredictionAsset` 保持上游 variant 顺序：

```rust
pub enum Asset<MarketId> {
    CategoricalOutcome(MarketId, CategoryIndex),
    ScalarOutcome(MarketId, ScalarPosition),
    CombinatorialOutcomeLegacy,
    PoolShare(PoolId),
    Native,
    ForeignAsset(u64),
    ParimutuelShare(MarketId, CategoryIndex),
    CombinatorialToken(CombinatorialId),
}
```

约束：

- 从首次公开 runtime 起，variant 顺序和字段编码视为稳定 ABI。
- `CombinatorialOutcomeLegacy` 即使 Nexus 无历史余额也保留，以维持完整上游模型。
- 所有时间加法使用 checked/saturating 运算。
- 从 `u64` 上游 BlockNumber 转为 Nexus `u32` 时，周期常量必须重新推导并测试边界。
- 不允许用 `as` 静默截断 MarketId、PoolId、BlockNumber 或 Balance。

## 7. 资产与资金安全设计

### 7.1 Native NEX

`Asset::Native` 通过 `orml-currencies` 路由到 `Balances`：

- reserve/unreserve 必须落在同一个 Nexus 账户余额。
- Court stake、market bond、Styx burn 全部使用真实 NEX。
- 不允许在 `PredictionTokens` 中创建 Native token issuance。

### 7.2 Outcome 与 pool assets

以下资产只存在于 ORML：

- categorical/scalar outcome
- pool share
- parimutuel share
- combinatorial token

只有相应 pallet sovereign account 或受控内部 API 可以 mint/burn。普通用户不得通过 root 之外的
通用资产管理调用创建这些资产。

### 7.3 Foreign collateral

`ForeignAsset(u64)` 是 `pallet-assets` 资产的 1:1 ORML 镜像：

- mirror mint 必须发生在 escrow transfer 成功之后。
- withdraw 必须先 burn mirror，再释放 escrow。
- 两步操作必须 `#[transactional]`。
- 被 prediction market 锁定的 mirror 不可直接 withdraw。
- 资产冻结、销毁或 bridge timeout 时必须先禁止新增 deposit。
- 不自动镜像 Entity token；每种 collateral 由 Technical Committee 单独批准。

### 7.4 资金不变量

必须实现 property/invariant tests：

```text
foreign mirror issuance == collateral escrow
complete-set collateral reserve >= outstanding complete sets
sum(outcome mint/burn deltas) obeys complete-set conservation
resolved market payout <= market collateral reserve
pool accounting never creates base asset
court/global-dispute slash + refund + reward conserves funds
parimutuel payout + fee <= pot
```

## 8. Origin、治理与账户映射

新增 origin aliases 放在 `runtime/src/configs/prediction.rs`：

| Zeitgeist 语义 | Nexus 映射 |
|----------------|------------|
| Market approve/reject/admin close | Root 或 Technical Committee 2/3 |
| Authorized resolve/correct | Root 或 Arbitration Committee 2/3 |
| Court admin/config | Root 或 Arbitration Committee 2/3 |
| Global dispute emergency admin | Root 或 Technical Committee 2/3 |
| Collateral whitelist | Root 或 Technical Committee 2/3 |
| Futarchy proposal submission | Root 或 Technical Committee 2/3 |
| Styx burn amount | Root 或 Treasury Council 2/3 |
| Prediction mode/pause | Root 或 Technical Committee 2/3 |

新增独立 PalletId：

```text
Prediction collateral escrow
Prediction market reserve
Prediction legacy swaps
Prediction neo-swaps
Prediction orderbook
Prediction parimutuel
Prediction court
Prediction global disputes
```

不得复用 Entity、USDX、Treasury、Escrow 或 Arbitration 的 sovereign account。

Slash 通过 `PredictionSlashToTreasury` adapter 进入现有 `TreasuryAccountId`，不要求引入
`pallet-treasury`。

## 9. Court 随机性

上游 Court 的 juror selection 是经济安全边界。Nexus 不使用当前
`CollectiveFlipRandomness` 作为 Court 随机源。

目标：

- 优先使用 BABE 一个 epoch 之前的随机性。
- 若 FRAME 45 API 无直接 `Randomness` 实现，编写只读 adapter。
- 随机 subject 必须包含 court ID、round、draw index 和 domain separator。
- juror draw 不得使用当前块 author 可单独操纵的数据。
- mock runtime 使用确定性随机源，生产 runtime 使用 BABE adapter。

Court 上线前必须完成：

- selection distribution 统计测试。
- 重复 juror、delegation、appeal 边界测试。
- stake overflow、slash、reward 守恒测试。
- BABE epoch 切换和 runtime upgrade 回归测试。

## 10. Zeitgeist runtime 配置映射

### 10.1 可直接复用

- `Timestamp`
- `Scheduler`
- `Preimage`
- `Balances`
- `TechnicalCommittee`
- `ArbitrationCommittee`
- `TreasuryAccountId`

### 10.2 必须重写

- `AssetManager`
- `ExistentialDeposits`
- `MarketCreatorFee`
- `Slash`
- `Randomness`
- 所有 market/court/bond/duration constants
- dust whitelist
- proxy/call filter
- runtime API wiring

### 10.3 不接入

- Zeitgeist `Treasury` pallet
- Zeitgeist `Democracy`
- Advisory Committee/Council 实例
- XCM asset registry
- parachain-specific Config

Futarchy 不要求新增 Democracy pallet。Nexus 治理通过现有委员会提交 call，Futarchy 使用
NeoSwaps decision oracle 判断后交给现有 Scheduler 执行。

## 11. 经济参数

不得直接沿用 ZTG 数值。所有参数按 NEX 12 位精度重新定义：

- validity bond
- advisory bond
- oracle bond
- outsider bond
- dispute bond
- close early bond
- court minimum stake
- juror inflation/reward
- global dispute vote fee
- market creator fee upper bound
- swap fee bounds
- minimum liquidity
- Styx burn amount

参数文档必须同时给出：

1. 原始单位。
2. NEX 显示值。
3. 最坏损失上限。
4. spam 成本。
5. 调整 origin。
6. 是否可通过 runtime storage parameter 修改。

首个生产版本必须保守限制：

- 每块自动关闭市场数。
- 每块自动结算市场数。
- 单市场 outcome 数。
- metadata 长度。
- 同结束时间市场数。
- Court participants、appeals、draws。
- Hybrid Router order vector 长度。
- combinatorial fuel/depth。
- 单用户 open orders。

所有上限必须是 bounded 类型并有最坏路径 benchmark。

## 12. Runtime API 与 RPC

### 12.1 Runtime API

接入：

- `SwapsApi`
- `PredictionMarketsApi`
- Nexus 新增只读 `PredictionViewApi`

`PredictionViewApi` 至少提供：

- market summary/status
- outcome assets
- report/resolved outcome
- market deadlines
- dispute mechanism and phase
- canonical pool
- spot prices
- user redeemable amount
- court case public summary
- collateral mirror status
- global prediction mode

不得在 runtime API 中执行无界遍历。分页参数必须有硬上限。

### 12.2 Node RPC

在 `node/src/rpc.rs` 合并：

- upstream swaps RPC
- `prediction_*` 查询 RPC

RPC 只封装 runtime API，不直接读数据库中的 pallet storage key。

### 12.3 客户端兼容

- 不承诺 Zeitgeist UI/SDK 零修改接入。
- SCALE 类型和语义尽量兼容，网络与资产名称使用 Nexus。
- 生成 TypeScript metadata types 后加入 E2E 固定流程。

## 13. Storage 与 runtime upgrade

Nexus 当前没有 prediction storage，因此：

- 所有移植 pallet 从其最新 storage layout 启动。
- 不把 Zeitgeist 主网历史 migration 加入 Nexus migration tuple。
- 保留 migration 源码仅用于上游可维护性，不在 Nexus 执行。
- 新增 pallet 的 `StorageVersion` 必须显式设置为当前版本。
- `PredictionControl` 初始模式为 `Disabled`。
- collateral whitelist 初始为空。

Runtime upgrade 验收：

1. 当前 Nexus state 运行 try-runtime 成功。
2. 0–175 pallet index 不变。
3. 原有 storage prefix 和 key 不变。
4. 新 pallet `on_runtime_upgrade` 在空 storage 上有界。
5. 旧 Runtime API 不删除、不改签名。
6. 新增 metadata 后现有 Entity/Chat/ISMP E2E 继续通过。

## 14. 开发阶段

### Phase 0 — 基线与依赖可行性

目标：确认 FRAME 45.x 下依赖闭包可编译。

任务：

- 固定 upstream commit `39ad8d60`。
- 建立 `UPSTREAM.md` 和 crate 清单。
- 为 ORML 选择与 Nexus FRAME 45.x 单一版本图兼容的来源。
- 升级 `hydra-dx-math`、`ark-bn254`、`fixed` 等依赖。
- 禁止引入第二份 `frame-support`、`sp-runtime` 或 `parity-scale-codec`。
- 建立最小 no_std smoke crate。

退出条件：

```bash
cargo tree -d
RUSTFLAGS="--cfg substrate_runtime" cargo check -p zeitgeist-primitives \
  --no-default-features --target wasm32-unknown-unknown
RUSTFLAGS="--cfg substrate_runtime" cargo check -p zrml-market-commons \
  --no-default-features --target wasm32-unknown-unknown
RUSTFLAGS="--cfg substrate_runtime" cargo check -p prediction-phase0-smoke \
  --no-default-features --target wasm32-unknown-unknown
```

不存在跨 SDK 的 FRAME/SP 类型重复。

说明：ORML stable2512 官方 no-std 检查要求 runtime cfg 和 WASM target。仅在 host 上执行普通
`cargo check --no-default-features` 会错误地把 `sp-state-machine` 拉入无 std 组合，不作为
runtime 可编译性的有效门禁。

### Phase 1 — Primitives 与 mock runtime

目标：所有业务 crate 可在 Nexus SDK 上编译，但暂不进入正式 runtime。

任务：

- 移植 primitives/macros。
- 对齐 `BlockNumber = u32`、`ForeignAsset(u64)`。
- 暂时保留 `Asset::Ztg` 名称，锁定 variant 顺序和 Nexus 字段编码。
- 建立 prediction shared mock runtime。
- 逐 crate 修复 FRAME API。
- 禁止业务重构。

退出条件：

- primitives SCALE golden tests 通过。
- 所有 pallet `cargo check --no-default-features` 通过。
- 上游 mock tests 能开始运行。

Phase 3 differential baseline 通过后，单独提交 `Ztg -> Native` 重命名 PR；该 PR 不得改变
variant 顺序、discriminant 或字段编码。

### Phase 2 — 资产层

目标：NEX、ORML outcome 和 pallet-assets collateral 安全互通。

任务：

- 接入 ORML currencies/tokens。
- 实现 `PredictionCollateral`。
- 实现 `PredictionControl`。
- 实现非 parachain `PredictionBaseAssetPolicy` 并删除市场准入对 XCM registry 的依赖。
- 实现 ED、dust、whitelist、mirror invariants。
- USDX 仅测试环境启用；生产 whitelist 为空。

退出条件：

- NEX 无重复 issuance。
- mirror deposit/withdraw property tests 通过。
- 任意失败点事务回滚。
- freeze/deny/insufficient escrow 测试通过。
- `parachain` feature 关闭时，Native 市场成功、白名单 ForeignAsset 市场成功、未准入资产失败。

### Phase 3 — 市场与争议核心

目标：无交易池也可完成完整市场生命周期。

顺序：

1. market-commons
2. authorized
3. court
4. global-disputes
5. prediction-markets

退出条件：

- permissionless/advised/trusted 市场流程通过。
- categorical/scalar complete-set 通过。
- oracle/outsider report 通过。
- Authorized/Court/GlobalDisputes 三种决议路径通过。
- 所有 bond 分支资金守恒。
- 自动 close/resolve 在 Nexus `u32` BlockNumber 下通过。

### Phase 4 — 交易模块

顺序：

1. legacy swaps
2. combinatorial-tokens
3. neo-swaps
4. orderbook
5. parimutuel
6. hybrid-router

说明：

- Legacy Swaps 必须移植，但生产初始模式下关闭。
- NeoSwaps 与 legacy swaps 使用不同 PalletId 和 pool namespace。
- Hybrid Router 只路由 PredictionNeoSwaps 与 PredictionOrderbook。
- 不路由 EntityMarket 或 AssetConversion。

退出条件：

- 上游数学向量测试全部通过。
- AMM buy/sell/join/exit/fee 资金守恒。
- order partial fill/cancel/fee 测试通过。
- router 在 AMM soft failure 时正确回退。
- parimutuel 无赢家、赢家和 fee 分支通过。
- combinatorial split/merge/redeem 与 fuel 上限通过。

### Phase 5 — Futarchy 与 Styx

目标：完成剩余全部业务模块。

任务：

- Futarchy oracle 接 NeoSwaps。
- proposal execution 接 Nexus Scheduler。
- Styx burn 使用 NEX `Balances`。
- admin origins 映射到 Nexus committees。

退出条件：

- 正/负 decision market 阈值测试通过。
- proposal schedule/cancel/failure 测试通过。
- Styx burn 与 registry 行为通过。

### Phase 6 — 正式 runtime 接线

任务：

- 添加 index 176–192。
- 新建 `runtime/src/configs/prediction.rs`。
- 更新 runtime dependencies/features。
- 注册 benchmarks。
- 接入非 `()` 的临时保守 `WeightInfo`，仅供 `Disabled` 状态集成验证。
- 实现 Runtime API。
- 合并 node RPC。
- 更新 chain spec/dev genesis，仅添加安全默认值。
- `PredictionMode = Disabled`。

当前进度（2026-07-13）：

- [x] 固定注册 index 176–192，并用 runtime 单测锁定既有 0–175。
- [x] 完成 `runtime/src/configs/prediction.rs`、dependencies/features 与安全 genesis 默认值。
- [x] 注册当前已实现 FRAME benchmark 的 prediction pallets；control/collateral/
  orml-currencies 的 benchmark 实现明确留到 Phase 7。
- [x] 接入非 `()` 的集成期 WeightInfo；所有 imported/Phase 2 weights 仍禁止生产启用。
- [x] 接入 `SwapsApi`、`PredictionMarketsApi` 与 Nexus 有界 `PredictionViewApi`。
- [x] 合并 `swaps_*` 与 `prediction_*` node RPC。
- [x] `PredictionMode = Disabled`、全部模块关闭、collateral whitelist 为空，并由
  runtime upgrade marker 强制验证。
- [x] release WASM 构建成功并记录产物体积；详见 Phase 6 验证记录。
- [ ] 完成现有 Nexus 全量测试回归。
- [ ] 基于接线前 metadata 产物审核 diff。
- [ ] 在当前 Nexus state snapshot 上执行 try-runtime。

实现与验证记录见 `pallets/prediction/docs/PHASE6_RUNTIME_WIRING.md`。

退出条件：

- release WASM 构建成功。
- 现有 Nexus 全量测试不回退。
- metadata diff 审核通过。
- try-runtime 通过。

Phase 6 产物不得进入 `ResolutionOnly` 或更高模式。Phase 7 生成并审核 Nexus runtime benchmark
weights 后，才能进行任何可调用上线。

### Phase 7 — Benchmark、fuzz 与 E2E

任务：

- 对所有 extrinsic 重新 benchmark。
- 生成 Nexus 专用 `weights.rs`。
- 移植并升级全部 upstream fuzz targets。
- 新增 Nexus adapter fuzz/property tests。
- 新增 TypeScript E2E。

当前进度（2026-07-13）：

- [x] 补齐并注册 `pallet-prediction-control`、`pallet-prediction-collateral`、
  `orml-currencies` runtime benchmark。
- [x] 使用 Nexus runtime、50 steps、20 repeats、compiled Wasm、max 回归与 measured
  PoV 生成首批 control、collateral、ORML currencies/tokens 权重。
- [x] Runtime 已切换首批 `SubstrateWeight<Runtime>`，ORML 不再通过包装类型委托给
  `WeightInfo = ()`。
- [x] 新增 `scripts/benchmark-prediction-zrml-weights.sh` 与
  `scripts/merge-zrml-benchmark-weights.py`，开始批量重生成 `zrml-*` Nexus 权重。
- [x] 全部 12 个 `zrml-*` Nexus runtime 权重已合并；重 pallet
  （court / prediction-markets / neo-swaps / combinatorial-tokens）使用缩减
  steps，生产前需以 50/20 重跑。
- [x] 六个 upstream fuzz crate 已在 Nexus workspace 编译通过；collateral proptest 已覆盖
  镜像序列不变量。
- [x] 恢复并扩展 Phase 7 prediction TypeScript E2E：
  emergency pause、collateral gate、USDX surface、USDX market、community-fee、
  court gate、core lifecycle、authorized dispute、orderbook、neo-swaps、
  hybrid-router、combinatorial、parimutuel、futarchy submit、styx；宿主侧
  fuzz/property 冒烟脚本已落地。
- [x] 在重建 `--dev` 节点上跑通上述套件（含 neo-swaps `PoolId` 映射、信任市场
  `disputeDuration=0`、Full 下 USDX `deposit` 过 CallFilter、hybrid-router
  orderbook 成交、community-fee Path A/B；neo-swaps 标准买卖从交易者扣
  ExternalFees；D19 标记含块高）。
- [ ] 安装 `cargo-fuzz` 跑限时 campaign；可选补齐 futarchy ≥600 块 schedule/execute；
  方便时全新 `--dev` 上跑完整 15 套 `e2e:prediction`。

实现与验证记录见
`pallets/prediction/docs/PHASE7_BENCHMARK_FUZZ_E2E.md`。

生产 runtime 禁止任何移植 pallet 使用 `WeightInfo = ()`。Phase 6 的临时保守 weights 必须在
Phase 7 被实测生成值全部替换，并通过 metadata/代码审查确认无遗漏。

### Phase 8 — 分阶段上线

建议升级顺序：

1. 部署全部 pallet，`Disabled`。
2. 仅启用 Authorized 和核心市场生命周期模块，切换 `ResolutionOnly`，处理预置内部测试市场。
3. 切换 `Trading`，只使用 NEX collateral，按模块逐个启用 NeoSwaps、Orderbook 和 Parimutuel。
4. 在独立升级中按审计结果启用 Court、Combinatorial、HybridRouter 和 Futarchy。
5. 切换 `Full` 后再独立开放 USDX mirror；`Full` 不隐式开启任何模块。
6. Legacy Swaps 和 Styx 最后单独评审、单独启用。

每一步都是独立 runtime upgrade，并有回滚/暂停预案。

## 15. 测试规范

### 15.1 单元测试

要求：

- 上游所有现存单测移植后通过。
- 修改预期只能由明确的 Nexus 语义差异解释。
- 每个修改过的上游测试在注释中记录差异原因。
- 新增公开核心 API 与关键 Config 注释必须英文 + 中文。

### 15.2 Differential tests

对固定上游 commit 和 Nexus port 运行同一场景，归一化以下差异后比较：

- native asset 名称
- BlockNumber 宽度
- AccountId
- pallet index
- event enum 外层路径

比较内容：

- market status
- balances/reserves
- minted/burned issuance
- report/resolution
- pool state/spot price
- court payouts
- emitted business events

### 15.3 Property tests

至少覆盖：

- complete-set 守恒
- scalar payout 边界
- categorical 单赢家
- market 状态不可非法回退
- report/dispute deadline 边界
- AMM 单调性和限价
- orderbook fill 守恒
- router 最差价格约束
- combinatorial partition 守恒
- Court stake/slash/reward 守恒
- collateral escrow equality

### 15.4 Fuzz

移植：

- prediction-markets full workflow
- swaps targets
- neo-swaps targets
- combinatorial-tokens targets
- orderbook target
- futarchy target

新增：

- collateral deposit/withdraw/freeze sequence
- market + bridge collateral combined workflow
- router with stale/filled/malformed order lists
- Court appeal and delegation sequence

### 15.5 E2E

新增 suites：

```text
prediction-market-lifecycle.ts
prediction-authorized-dispute.ts
prediction-court-dispute.ts
prediction-global-dispute.ts
prediction-neo-swaps.ts
prediction-orderbook-router.ts
prediction-parimutuel.ts
prediction-combinatorial.ts
prediction-futarchy.ts
prediction-collateral-usdx.ts
prediction-emergency-pause.ts
```

更新 `scripts/docs/NEXUS_TEST_PLAN.md`，恢复 prediction E2E，但不得复用已删除旧模块的脚本假设。

### 15.6 回归测试

每次 runtime 接线变更至少运行：

```bash
cargo test
cargo test -p nexus-runtime
cargo test -p zrml-prediction-markets
cargo test -p zrml-court
cargo test -p zrml-neo-swaps
cargo test -p zrml-combinatorial-tokens
cd scripts && npm run e2e:entity:smoke
```

进入生产候选后运行全部 prediction E2E、fuzz campaign、try-runtime 和 release build。

## 16. Benchmark 与性能门禁

接线前记录 Nexus 基线：

- native/release 编译时间
- runtime WASM 大小
- metadata 大小
- block import 时间
- 空块和满块执行时间
- storage proof 大小

新增模块后必须报告增量，不预设未经测量的固定百分比。

硬性门禁：

- 所有 `on_initialize` 循环有界。
- 同一时间关闭/结算市场使用 bounded queue。
- worst-case call 不超过 runtime 允许的单 extrinsic weight。
- 自动 close + resolve 的 worst-case 总权重小于 block 初始化预算。
- Court draw、global outcome、router orders、combinatorial depth 均有 benchmark 上限。
- release WASM 仍满足节点执行器限制。

## 17. 安全审计优先级

P0：

- collateral mirror 与 escrow
- prediction-markets 状态机、bond、redeem
- complete-set collateral solvency
- NeoSwaps LMSR 数学与 fee
- combinatorial ID/split/merge/redeem
- Court randomness、stake、appeal、slash

P1：

- Hybrid Router price limit 与 fallback
- Orderbook partial fill
- Global Disputes vote locking
- Parimutuel payout
- Futarchy call scheduling

P2：

- Legacy Swaps
- Styx
- read-only Runtime API/RPC

任何 P0 finding 未关闭时不得进入 `Trading` 或 `Full` 模式。

## 18. 风险与应对

| 风险 | 影响 | 应对 |
|------|------|------|
| FRAME 2409 → 45 API 变化 | 编译与行为漂移 | 分离 SDK patch，先不重构 |
| ORML 版本形成第二套 FRAME | 类型无法统一 | `cargo tree -d` 作为 Phase 0 门禁 |
| 双资产账本 | 发行或赎回失衡 | 仅 foreign mirror 双账；NEX 单账；持续 invariant |
| `u64 -> u32` BlockNumber | deadline 截断 | 泛型化、checked conversion、边界测试 |
| Court 随机性弱 | 陪审操纵 | BABE 历史 epoch randomness |
| Runtime 体积与 metadata 膨胀 | 升级/执行风险 | 实测预算、分阶段启用、无界 API 禁止 |
| 与现有交易/治理混淆 | 错误调用与用户风险 | runtime alias、PalletId、UI namespace 全隔离 |
| 上游 legacy 代码 | 安全与维护债 | 完整移植但默认 Disabled |
| 大型一次性合并 | 难 review/难定位 | 按 Phase 独立 PR，禁止 big-bang merge |

## 19. PR 拆分

建议每个 PR 可独立编译和测试：

1. `prediction: add upstream manifest and dependency spike`
2. `prediction: port primitives and macros`
3. `prediction: add control and collateral adapters`
4. `prediction: port market commons and authorized disputes`
5. `prediction: port court and global disputes`
6. `prediction: port market lifecycle and complete sets`
7. `prediction: port legacy swaps`
8. `prediction: port combinatorial tokens`
9. `prediction: port neo swaps`
10. `prediction: port orderbook and parimutuel`
11. `prediction: port hybrid router`
12. `prediction: port futarchy and styx`
13. `runtime: wire prediction subsystem disabled`
14. `node: expose prediction runtime APIs and RPC`
15. `tests: add prediction differential, fuzz and E2E coverage`
16. `runtime: add generated prediction weights`
17. `runtime: enable resolution-only prediction rollout`

一个 PR 不得同时包含：

- SDK 大版本适配和算法重构。
- 资产模型变更和经济参数调优。
- pallet index 调整和无关 runtime cleanup。
- prediction 代码和 Entity/Chat 重构。

## 20. Definition of Done

全量移植只有同时满足以下条件才算完成：

- [ ] 全部 13 个 Zeitgeist 业务 pallet 已进入 Nexus workspace。
- [ ] control/collateral 两个 Nexus adapter 完成。
- [ ] 全部 crate 使用 Nexus 单一 FRAME/SP 版本图。
- [ ] `cargo check --no-default-features` 全部通过。
- [ ] 上游单元测试移植并通过。
- [ ] Nexus 新增 property/differential tests 通过。
- [ ] 全部 fuzz targets 可运行且生产候选 campaign 无 crash。
- [ ] Runtime index 176–192 固定，0–175 未变化。
- [ ] Runtime API/RPC 接线完成。
- [ ] 所有 production `WeightInfo` 为重新生成的 Nexus weights。
- [ ] try-runtime 在当前 Nexus state 上通过。
- [ ] 现有 Nexus E2E 无回退。
- [ ] Prediction 全流程 E2E 通过。
- [ ] P0 安全审计问题全部关闭。
- [ ] 资金与资产不变量监控就绪。
- [ ] `Disabled → ResolutionOnly → Trading → Full` 演练通过。
- [ ] 运维具有暂停、恢复和 runtime 回滚手册。

## 21. 开工前最终检查

正式写代码前必须先完成 Phase 0 spike，并输出三份结果：

1. **Dependency lock report**：ORML、FRAME、HydraDX math、arkworks 的最终版本图。
2. **Asset proof-of-concept**：NEX native + outcome token + USDX mirror 的完整存取与守恒测试。
3. **Runtime budget baseline**：当前 Nexus WASM、metadata、block weight 和构建基线。

三项任一失败，不进入批量源码移植；先修正架构决策，再继续。

在三项结果获审后，开发权限按层开放：

```text
Phase 0 passed  -> 允许 primitives/macros 和 shared mock runtime
Asset POC passed -> 允许 collateral、market core 和交易模块
Budget accepted  -> 允许正式 runtime 接线
Generated weights + security gates passed -> 允许 ResolutionOnly 及更高模式
```
