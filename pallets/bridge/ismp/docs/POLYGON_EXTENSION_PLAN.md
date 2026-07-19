# NEX 资产桥扩展到 Polygon — 编码计划

> 状态：Plan（待开工）
> 范围：`pallets/bridge/ismp/`（Substrate 侧，~零代码改动）+ EVM 侧 NEX 合约部署 + relayer/运维配置
> 关联：`docs/HYPERBRIDGE_DEV_ROADMAP.md` Stage 2、`pallets/prediction/docs/PREDICTION_PALLET_DESIGN.md`（Polygon USDC 桥的 NEX 侧基础）

---

## 0. 可行性结论

| 验证项 | 结论 | 证据 |
|---|---|---|
| Hyperbridge 在 Polygon 上线？ | **是** | Polygon 主网 + Amoy 测试网均有 `IsmpHost` / `HandlerV1` 部署（Hyperbridge 官方文档合约地址表） |
| Polygon 共识客户端存在？ | **是** | Hyperbridge relayer 内置 `type = "polygon"`，通过 Heimdall RPC 验证 Bor header finality |
| nexus 需要新写 Polygon 共识客户端？ | **否** | nexus 用 **coprocessor 模型**（`ConsensusClients = (ismp_grandpa::GrandpaConsensusClient,)`），只验证 Hyperbridge 的 GRANDPA 共识；Polygon 共识由 coprocessor 验证，nexus 信任 coprocessor 的 state commitment |
| `pallet-bridge-ismp` 支持 Polygon？ | **是（已支持）** | `register_chain(chain: StateMachine, contract: H160, erc_decimals: u8)` 只校验 `chain.is_evm()`，Polygon = `StateMachine::Evm(137)` 直接通过 |
| 精度换算需要改？ | **否** | NEX 12 位 native、ERC 18 位，与 BSC 完全一致；pallet 内 `convert_to_erc20` / `convert_to_balance` 通用 |
| burn/mint 模型适用？ | **是** | 与 BSC 同构：出站真 burn、入站真 mint，`BridgedOutByChain[Evm(137)]` 防通胀 |

**一句话**：Substrate 侧代码层面 **不需要新功能开发**，Polygon 是一条新的 EVM lane，通过治理注册即可启用。真正的工作量在 EVM 合约部署 + 测试覆盖 + 运维。

---

## 1. 为什么 Substrate 侧几乎零改动

`pallet-bridge-ismp` 在设计时就是 **EVM-chain-agnostic**：

```rust
// pallets/bridge/ismp/src/lib.rs L519-538
pub fn register_chain(
    origin: OriginFor<T>,
    chain: StateMachine,        // 任意 Evm(chain_id)
    contract: H160,             // 该链上的 NEX ERC-6160 合约地址
    erc_decimals: u8,           // 18
) -> DispatchResult {
    ensure!(chain.is_evm(), Error::<T>::NotEvmChain);  // Polygon 通过
    // ... 精度校验、写入 Chains storage
}
```

- `bridge_out` / `on_accept` / `on_timeout` 全部按 `StateMachine::Evm(chain_id)` 索引，不硬编码链 id
- `BridgedOutByChain: Map<StateMachine, Balance>` 自动为每条链独立记账
- `set_paused(Some(Evm(137)), true)` 可单独暂停 Polygon lane
- `Limits`（per_tx / daily）是全局的——若要 per-lane 限额需小改（见 §3.B）

**结论**：BSC 能跑的代码，Polygon 一字不改也能跑。

---

## 2. 编码任务分解

### A. EVM 侧 NEX 合约部署（主要新工作量）

| 任务 | 说明 | 工作量 |
|---|---|---|
| A1. 选择合约 | 优先 Polytope Labs 官方已审计 `HyperFungibleToken`（burn-custody / native 模式），ABI 与 Substrate 侧 `Message{from,to,amount,data}` 逐字节一致 | 0（用现成） |
| A2. Polygon Amoy 测试网部署 | 部署 `HyperFungibleToken`，初始化时传入 Amoy 的 `IsmpHost` 地址 | 小 |
| A3. Polygon 主网部署 | 同上，主网 `IsmpHost` 地址 | 小（需审计后） |
| A4. 注册 Nexus 为 peer | 合约上 `addChain(bytes("SUBSTRATE-NEXS"), bytes("nexbridg"))` | 小 |
| A5. 独立安全审计 | 若用官方合约则免审；若用参考合约 `NexHyperFungibleToken.sol` 则需审计 | 中（仅在自写时） |
| A6. ERC-6160 tokenomics | 确认 Polygon 上 NEX 总量与 nexus `BridgedOutByChain[Evm(137)]` 对账关系 | 小 |

**EVM 合约地址（Hyperbridge 已部署，NEX 合约对接用）**

| 网络 | IsmpHost | HandlerV1 |
|---|---|---|
| Polygon Amoy (testnet) | 查 Hyperbridge 官方 testnet 地址表 | 同上 |
| Polygon Mainnet | 查 Hyperbridge 官方 mainnet 地址表 | 同上 |

> G-B3 依赖：nexus 的 `HostStateMachine = Substrate(*b"NEXS")` 和 `Coprocessor = Polkadot(3367)` 需与 Polytope Labs 确认主网最终值。

### B. Substrate 侧测试覆盖（代码改动主要在此）

现有测试以 BSC (`Evm(56)`) 为 canonical chain。需增加 Polygon (`Evm(137)`) 测试用例，验证多 lane 并存。

| 文件 | 改动 |
|---|---|
| `src/mock.rs` | 增加 `polygon()` helper 返回 `StateMachine::Evm(137)`；增加 `polygon_contract()` 常量；mock 环境注册 Polygon lane |
| `src/tests.rs` | 新增测试组：`polygon_register` / `polygon_bridge_out_round_trip` / `polygon_inbound_anti_inflation` / `polygon_pause_lane` / `multi_lane_bsc_and_polygon`（两条 lane 同时活跃，账本独立） |
| `src/benchmarking.rs` | benchmark 用例的 chain 参数化（当前硬编码 `Evm(56)`，改为可配置或加 Polygon benchmark） |

**关键测试场景**

```rust
#[test]
fn polygon_bridge_out_round_trip() {
    // 1. register_chain(Evm(137), polygon_contract, 18)
    // 2. set_limits(per_tx, daily)
    // 3. bridge_out(Evm(137), recipient, amount) → burn NEX, BridgedOutByChain[Evm(137)] += amount
    // 4. 模拟 inbound POST from Evm(137) → on_accept → mint to recipient
    // 5. assert BridgedOutByChain[Evm(137)] -= amount
    // 6. assert total_issuance 回到初始
}

#[test]
fn multi_lane_bsc_and_polygon_independent() {
    // 同时注册 BSC(56) + Polygon(137)
    // bridge_out 到两条链，assert BridgedOutByChain 各自独立
    // pause BSC lane → BSC bridge_out 失败，Polygon 仍可用
}

#[test]
fn polygon_inbound_exceeds_bridged_out_rejected() {
    // 伪造入站 amount > BridgedOutByChain[Evm(137)] → 拒绝（防通胀）
}
```

### C. 可选小改：per-lane 限额（若需独立风控）

当前 `Limits { per_tx, daily }` 是全局的。若要对 Polygon 和 BSC 设不同限额：

```rust
// 当前：BridgeLimits 全局
// 改为：PerLaneLimits: Map<StateMachine, BridgeLimits>
pub fn set_lane_limits(origin, chain: StateMachine, per_tx, daily) -> DispatchResult {
    T::BridgeOrigin::ensure_origin(origin)?;
    ensure!(chain.is_evm(), Error::<T>::NotEvmChain);
    LaneLimits::<T>::insert(chain, BridgeLimits { per_tx, daily });
    Self::deposit_event(Event::<T>::LaneLimitsChanged { chain, per_tx, daily });
    Ok(())
}
```

`bridge_out` 的限额校验从读全局 `Limits` 改为读 `LaneLimits::get(chain).unwrap_or(global_limits)`。

**建议**：本期先不做 per-lane，用全局限额 + per-lane pause 足够；待 Polygon 流量上来再做。

### D. Runtime / 治理注册（运维，非代码）

无代码改动，纯治理调用（主网前在 testnet 验证）：

```rust
// 1. 注册 Polygon（root / BridgeOrigin）
pallet_bridge_ismp::register_chain(
    RuntimeOrigin::root(),
    StateMachine::Evm(137),                    // Polygon mainnet
    hex!("...NEX_POLYGON_CONTRACT..."),        // H160
    18,                                         // ERC decimals
)

// 2. 设置限额（若沿用全局）
pallet_bridge_ismp::set_limits(per_tx, daily)

// 3. 共识状态初始化（一次性，由 ismp-grandpa + pallet-ismp 完成）
//    若 coprocessor 已知 Polygon，nexus 侧自动可验证
```

### E. Relayer 配置

在 relayer 配置文件加 Polygon section（relayer 是链下程序，非本仓库代码）：

```toml
[polygon]
type     = "evm"
rpc_urls = ["https://polygon-mainnet.g.alchemy.com/v2/YOUR_KEY"]
signer   = "SIGNER-KEY"

[polygon.consensus]
type             = "polygon"
heimdall_rpc_url = "https://polygon-heimdall-rpc.publicnode.com:443"
```

测试网用 Amoy 对应 RPC。

### F. 监控与对账

扩展 README §5 "Monitoring & reconciliation" 的对账脚本，增加 Polygon lane：

```text
off-chain keeper 周期性断言：
  BridgedOutByChain[Evm(137)] (Nexus) == totalSupply of NEX contract on Polygon
  （允许 in-flight slack）
```

告警项增加：Polygon lane 的 `BridgeRefunded` 异常、`on_accept` 拒绝、对账偏移。

---

## 3. 文件级改动清单

| 文件 | 改动类型 | 说明 |
|---|---|---|
| `pallets/bridge/ismp/src/mock.rs` | 新增 | `polygon()` / `polygon_contract()` helper + mock 注册 |
| `pallets/bridge/ismp/src/tests.rs` | 新增 | Polygon lane 测试组 + 多 lane 并存测试 |
| `pallets/bridge/ismp/src/benchmarking.rs` | 参数化 | chain 从硬编码 `Evm(56)` 改为可配置 |
| `pallets/bridge/ismp/src/lib.rs` | 可选 | 若做 per-lane 限额（§2.C），加 `set_lane_limits` + `LaneLimits` storage |
| `pallets/bridge/ismp/README.md` | 文档 | 运维手册增加 Polygon 注册示例 |
| `pallets/bridge/ismp/evm/README.md` | 文档 | 增加 Polygon 合约地址 + Amoy/mainnet 对接步骤 |
| `pallets/bridge/ismp/evm/` | 参考 | 若自写合约，加 Polygon 部署脚本（Foundry/Hardhat） |
| `runtime/src/configs/ismp.rs` | 无代码改动 | 仅 G-B3 确认后更新 Coprocessor 常量 |
| `docs/HYPERBRIDGE_DEV_ROADMAP.md` | 文档 | Stage 2 增加 Polygon lane 里程碑 |

**Substrate 侧核心代码改动量：mock + tests 占 90%，lib.rs 仅在 per-lane 限额时触及。**

---

## 4. 测试计划

### 4.1 单测（`cargo test -p pallet-bridge-ismp`）

| 测试 | 验证点 |
|---|---|
| `polygon_register_chain` | 注册 Polygon 成功；非 EVM 拒绝；精度校验 |
| `polygon_bridge_out_burns_and_ledgers` | 出站 burn NEX + `BridgedOutByChain[Evm(137)]` 增加 |
| `polygon_inbound_mints_and_decrements` | 入站 mint + ledger 减少 |
| `polygon_round_trip_total_issuance_restored` | 往返后总供应量回到初始 |
| `polygon_inbound_exceeds_bridged_out_rejected` | 防通胀 |
| `polygon_pause_lane_blocks_outbound` | 单 lane 暂停 |
| `polygon_pause_does_not_affect_bsc` | 跨 lane 隔离 |
| `multi_lane_independent_ledgers` | BSC + Polygon 账本独立 |
| `polygon_precision_scaling` | 18↔12 换算与 BSC 一致 |
| `polygon_min_bridge_amount_floor` | dust 截断下限 |

### 4.2 集成 / 测试网

| 测试 | 环境 |
|---|---|
| Nexus testnet ↔ Polygon Amoy 往返 | Amoy NEX 合约部署后 |
| 对账脚本验证 | keeper 跑 24h |
| relayer Polygon consensus 跟踪 | Heimdall RPC 连通性 |

### 4.3 主网前门禁

沿用 `HYPERBRIDGE_DEV_ROADMAP.md` Stage 0 门禁，Polygon lane 额外要求：
- [ ] Polygon NEX 合约独立审计（若自写）
- [ ] G-B3 主网 Coprocessor / HostStateMachine 确认
- [ ] G-A1-2 tokenomics 签字（Polygon lane 的 burn/mint 影响 total_issuance）
- [ ] Amoy 往返成功 + 对账无偏差

---

## 5. 依赖与阻塞项

| 项 | 状态 | 阻塞内容 |
|---|---|---|
| Hyperbridge Polygon 主网上线 | ✅ 已上线 | 无 |
| Polygon 共识客户端 | ✅ relayer 内置 | 无 |
| G-B3 Coprocessor 常量 | 🟡 占位 `Polkadot(3367)` | 主网注册前需确认 |
| G-A1-2 tokenomics 签字 | 🟜 待评审 | 主网启用前 |
| EVM NEX 合约审计 | 🟜 待做 | 若自写合约；用官方合约则免 |
| relayer Polygon 配置 | 🟜 待做 | 测试网联调 |

---

## 6. 工作量估计

| 阶段 | 内容 | 估时 |
|---|---|---|
| **P0** | mock + tests 增加 Polygon lane（§2.B） | 1d |
| **P0** | benchmarking 参数化 | 0.5d |
| **P1** | Polygon Amoy 部署 NEX 合约 + 注册 peer | 1d（含联调） |
| **P1** | relayer Polygon 配置 + 测试网往返 | 1.5d |
| **P1** | 对账 keeper 扩展 Polygon lane | 0.5d |
| **P2** | README / evm/README 文档更新 | 0.5d |
| **P2** | （可选）per-lane 限额 lib.rs 改动 + 测试 | 1d |
| **P3** | 主网部署 + 审计 + tokenomics 签字 | 外部依赖，非编码 |

**纯编码工作量：约 4-5 人日**（不含外部审计/签字/主网部署）。

---

## 7. 与预测市场的战略衔接

把 NEX 桥到 Polygon 后，打开了一条与预测市场设计直接相关的路径：

```
NEX (Nexus native)
   │ bridge_out (burn)
   ▼
wNEX (ERC-6160 on Polygon)
   │ QuickSwap / Uniswap swap
   ▼
USDC (Polygon)
   │ 桥回 nexus（nUSDC IOU，见 PREDICTION_PALLET_DESIGN.md §B）
   ▼
nUSDC (nexus 链内)
   │ complete-set 铸造
   ▼
Outcome Token (预测市场持仓)
```

即：**NEX 桥到 Polygon 是"NEX ↔ Polygon USDC 兑换"长路径的第一段**。用户拿 NEX → 桥到 Polygon → DEX 换 USDC → 桥回 nexus nUSDC → 下注。

> 短路径（P2P NEX/USDC 直接兑换、或链内 NEX/nUSDC 订单簿）在 `PREDICTION_PALLET_DESIGN.md` 已规划。NEX→Polygon 桥是补充路径，也服务于"NEX 进入 Polygon DeFi 生态"这个独立目标。

---

## 8. 立即可启动的 P0 编码任务（✅ 已完成）

无外部依赖、可立即开工的部分：

1. **`src/mock.rs`**：加 `polygon()` / `polygon_contract()` + mock 注册 Polygon lane ✅
2. **`src/tests.rs`**：加 §4.1 的 10 个 Polygon 测试用例 ✅
3. **`src/benchmarking.rs`**：chain 参数化 — **未改**（权重 chain-agnostic，存储读写与 chain id 无关，单链 benchmark 已代表任意 EVM 链的权重；遵循"最小必要修改"原则）
4. **`cargo test -p pallet-bridge-ismp`**：全绿（47 unit + 6 benchmark bodies = 53 测试通过） ✅

完成后即可提 PR，不依赖 G-B3 / 审计 / 主网部署。

### 已完成改动清单

| 文件 | 改动 | 状态 |
|---|---|---|
| `pallets/bridge/ismp/src/mock.rs` | 新增 `polygon()` helper（`StateMachine::Evm(137)`） | ✅ |
| `pallets/bridge/ismp/src/tests.rs` | 新增 `polygon_contract()` / `setup_polygon()` / `setup_both_chains()` helper + `Chains` import + 10 个 Polygon lane 测试 | ✅ |
| `pallets/bridge/ismp/src/benchmarking.rs` | 未改（权重 chain-agnostic） | — |
| `pallets/bridge/ismp/src/lib.rs` | 未改（已 EVM 通用） | — |
| `pallets/bridge/ismp/README.md` | 运维手册加 Polygon 注册示例 + 多 lane 暂停/对账说明 | ✅ |
| `pallets/bridge/ismp/evm/README.md` | peer 标识表加 Polygon + 端到端配置说明多链重复 | ✅ |

### 新增测试覆盖（10 个，全绿）

| 测试 | 验证点 |
|---|---|
| `polygon_register_chain_succeeds` | 注册 Polygon lane 成功 + 事件 |
| `polygon_bridge_out_burns_and_books` | 出站 burn + `BridgedOutByChain[137]` 记账 + BSC lane 不受影响 |
| `polygon_round_trip_restores_total_issuance` | 出站+入站往返后 total_issuance 还原 |
| `polygon_inbound_exceeds_bridged_out_rejected` | 防通胀 |
| `polygon_inbound_rejects_unknown_source_contract` | source 合约校验（拒绝 BSC 合约冒充 Polygon） |
| `polygon_pause_lane_blocks_outbound` | per-lane 暂停 |
| `polygon_pause_does_not_affect_bsc` | lane 隔离（暂停 Polygon 不影响 BSC） |
| `multi_lane_independent_ledgers` | 双 lane 并存账本独立 + 单 lane 赎回不影响另一 lane |
| `polygon_precision_scaling_matches_bsc` | 18↔12 dust 截断与 BSC 一致 |
| `polygon_deregister_disables_lane` | 注销 Polygon 不影响 BSC |

---

## 9. 相关文档

- `pallets/bridge/ismp/README.md` — 桥 pallet 运维手册
- `pallets/bridge/ismp/evm/README.md` — EVM 侧对接规范
- `docs/HYPERBRIDGE_DEV_ROADMAP.md` — Stage 0-4 路线图
- `docs/HYPERBRIDGE_INTEGRATION.md` — 集成设计主文档
- `pallets/prediction/docs/PREDICTION_PALLET_DESIGN.md` — 预测市场设计（Polygon USDC 桥的下游消费方）

---

## 10. 路线 B 升级：把 `NexHyperFungibleToken.sol` 补齐到生产可用（✅ 已完成）

> 决策背景：上轮对比路线 A（官方 `HyperFungibleToken`）与路线 B（仓库参考合约）
> 后，按"以路线 B 为起点补齐"的要求完成本轮升级。Substrate 侧零改动；EVM 侧
> 合约重写 + Foundry 工程化 + 单元/集成测试齐备。

### 10.1 升级内容（相对原路线 B）

| 项 | 原路线 B | 升级后 |
|---|---|---|
| ERC165 / `IHyperFungibleToken` | 无 | 实现，接口 id `0x7200c457`，SDK 可自动识别 |
| `SendParams` + `quote()` | 扁平参数、无 quote | 与官方一致的 `SendParams` + `quote(SendParams)` / `quote(DispatchPost)` |
| fee token 支付 | 仅原生 `msg.value` | `msg.value > 0` 走原生，否则 `dispatchWithFeeToken` |
| `onPostRequestTimeout` 签名 | `PostRequest calldata`（错位风险） | `PostRequestTimeout memory`（对齐 `ismp-solidity` IApp） |
| Pause 语义 | 仅 `send` 检查；`onAccept`/`onPostRequestTimeout`/`transfer` 不受控 | 继承 OZ `Pausable`，全部冻结 |
| `data` 字段 | 写死 `""` 且忽略 | 透传到 `Message.data`，`onAccept` 经 `CallDispatcher` 执行 |
| 可升级 | 无 | 新增 `NexHyperFungibleTokenUpgradeable.sol`（UUPS，`initialize`） |
| 测试 | 无 | Foundry 单元 + 集成测试，本地 mock ISMP host |

### 10.2 文件清单

| 文件 | 说明 |
|---|---|
| `evm/src/NexHyperFungibleToken.sol` | 重写：ERC165 + IHyperFungibleToken + SendParams + quote + fee token + Pausable 全冻结 + PostRequestTimeout 签名修正 |
| `evm/src/NexHyperFungibleTokenUpgradeable.sol` | 新增：UUPS 可升级变体，逻辑与上一致 |
| `evm/src/interfaces/IHyperFungibleToken.sol` | 新增：vendored 接口定义（ID `0x7200c457`） |
| `evm/src/NexusDigitalOrderGateway.sol` | 迁入 `src/`（原在 `evm/` 根，内容未改） |
| `evm/foundry.toml` | 新增：Foundry 配置，测试 remapping 指向本地 mock |
| `evm/script/bootstrap.sh` | 新增：`forge install` 一键拉 openzeppelin / ismp-solidity / upgradeable |
| `evm/.gitignore` | 新增：忽略 `lib/ out/ cache/` |
| `evm/test/mocks/ismp-solidity-abi/*` | 新增：本地 mock `BaseIsmpModule` / `IDispatcher` / `IApp` / struct |
| `evm/test/mocks/MockIsmpHost.sol` | 新增：`IDispatcher` 实现 + 入站/超时回调驱动，记录派发账本 |
| `evm/test/NexHyperFungibleToken.t.sol` | 新增：单元测试（ERC165 / chain 注册 / send-burn / pause 全路径 / quote / timeout / 鉴权 / host 配置） |
| `evm/test/NexHyperFungibleTokenUpgradeable.t.sol` | 新增：UUPS 单测（initialize 一次 / 升级权限 / 行为对齐） |
| `evm/test/BridgeIntegration.t.sol` | 新增：集成测试——`Message` ABI 编码与手算参考一致、`nexbridg` 恰 8 字节、Substrate↔EVM 入站铸造/出站销毁/超时退款与 mock host 账本对账 |
| `evm/README.md` | 更新：升级说明 + Build & test 段 + 测试布局 |

### 10.3 运行

```bash
cd pallets/bridge/ismp/evm
./script/bootstrap.sh        # 一次性：forge install openzeppelin + ismp-solidity + upgradeable
forge build
forge test -vv
forge test --match-contract BridgeIntegration -vvv
forge coverage
```

> 默认 `foundry.toml` 的 `@polytope-labs/ismp-solidity-abi/` remapping 指向
> `test/mocks/ismp-solidity-abi/`，使 `forge test` 离线跑本地 mock。生产部署请改回
> `lib/ismp-solidity/`（由 `bootstrap.sh` 安装）后重新编译。

### 10.4 二次审计与修复（本轮）

对升级后的合约 + 测试做了一轮静态审计，修复以下编译性问题（否则 `forge build`
失败）：

| 问题 | 修复 |
|---|---|
| `onAccept` / `onPostRequestTimeout` 写了 `override` 但 `BaseIsmpModule` 未声明这两个方法 | mock `BaseIsmpModule` 改 `abstract contract is IApp`，提供 `virtual` 空实现 |
| `dispatcher` / `supportedChain` / `configure` / `addChain` / `removeChain` / `pause` / `unpause` / `quote(SendParams)` 实现接口但缺 `override`（Solidity 0.8.x 必需） | 两份合约全部补 `override` |
| `vm.expectRevert("EnforcedPause()")` 不匹配 OZ 5.x 自定义 error（revert reason 是 4 字节 selector） | 改 `vm.expectRevert(Pausable.EnforcedPause.selector)`，测试 import `Pausable` |
| `NexHyperFungibleToken.sol` 未使用的 `IERC20` / `SafeERC20` import | 删除 |
| `_toAddr` 注释 "first 20 bytes" 与代码（严格 length==20）不符 | 改注释为 "strictly 20-byte payload" |

### 10.5 仍未在本轮验证项与原因

- **Foundry 仍未本地编译验证**：本机未安装 `forge`。本轮静态审计修复了已知编译性
  问题，但未跑 `forge build` / `forge test`。首次在装 Foundry 的环境执行
  `bootstrap.sh` 后可能仍有少量编译细节需调（例如 `onAccept` 单 `override` 是否
  需写成 `override(BaseIsmpModule, IApp)`、OZ 5.x `Pausable` import 路径、
  `ERC1967Proxy` 路径）。
- **真实 `ismp-solidity` 包未集成**：mock 复刻了生产合约 import 的形状，但真实包
  的 `BaseIsmpModule` 可能有额外 abstract 方法或不同的 `dispatchWithFeeToken` 实现；
  切回真实 remapping 后需回归。
- **`IHyperFungibleToken` 接口为本地 vendored**：与官方 `@hyperbridge/core` 的接口
  应在部署前用官方包 ABI 做一次 diff，确认 `0x7200c457` 与方法签名一致。
- **mock `PostRequest` 含 `timestamp` 字段**：真实 host 的 `PostRequest` 字段顺序/
  数量若不同，`onAccept` 的 calldata 解码会错位。仅影响切回真实包后的集成，mock
  内部自洽。
- **独立审计未做**：仍为参考合约，主网前需审计。
