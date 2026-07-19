# pallet-usdx

`pallet-usdx` is the Nexus-local Peg Stability Module (PSM) for fully
collateralized USDX.

`pallet-usdx` 是 Nexus 链内的 USDX 足额抵押锚定稳定模块（PSM）。

## Phase-0 scope / Phase 0 范围

- Registers authenticated HFT receipt assets as disabled-by-default lanes.
- Mints USDX only after receipt transfer into the deterministic PSM account.
- Burns USDX before returning a selected lane's receipt asset.
- Enforces per-lane/global debt ceilings, rolling windows, pause controls,
  policy bounds, descriptor revalidation, and issuance/debt invariants.
- Keeps mint/redeem fees as non-withdrawable receipt surplus.

- 将已认证 HFT 收据资产注册为默认停用的通道。
- 收据转入确定性 PSM 账户后才铸造 USDX。
- 销毁 USDX 后才返还所选通道的收据资产。
- 执行逐通道/全局债务上限、滚动窗口、暂停、策略边界、descriptor 复验与发行债务不变量。
- mint/redeem 费用作为不可提取的收据安全缓冲。

Cross-chain verification, HFT message handling, timeout refunds, EVM contract
governance, and production lane activation are deliberately outside this
pallet. See `docs/USDX_USDC_HYPERBRIDGE_DEV_SPEC.md`.

跨链验证、HFT 消息处理、超时退款、EVM 合约治理与生产通道启用不属于本 pallet。

## Current status / 当前状态

The crate is wired into the Nexus runtime at pallet index `174`. Runtime spec
104 introduced the idempotent migration that reserves and creates protocol
assets `900000..=900002`. Runtime spec 105 is the Phase-1 Amoy activation
upgrade: it enables the strict asset inspector, Root-only HFT registry
governance, and empty-calldata HFT sends. The HFT registry remains empty and all
USDX debt/window limits remain zero until explicit governance activation.

该 crate 已接入 Nexus runtime（pallet index `174`）。runtime spec 104 引入幂等迁移，
创建并保留协议资产 `900000..=900002`。runtime spec 105 是 Phase 1 Amoy 激活升级：
启用严格资产 inspector、仅 Root 可用的 HFT registry 治理，以及空 calldata 的 HFT
发送。治理显式激活前，HFT registry 仍为空，全部 USDX 债务与窗口限额仍为零。

No lane may receive non-zero limits until deployment evidence, consensus paths,
relayers, timeout behavior and reconciliation have passed the Phase-1 runbook.

在部署证据、共识路径、relayer、timeout 行为和对账全部通过 Phase 1 手册前，任何通道
都不得获得非零限额。

## Phase-1 Amoy runtime / Phase 1 Amoy runtime

Runtime spec 105 targets `StateMachine::Evm(80002)` and Circle Amoy test USDC
for receipt asset `900001`. It enables the strict protocol-asset inspector,
Root HFT registry governance, and HFT `send` calls with `call_data = None` only.

Runtime spec 105 为收据资产 `900001` 固定
`StateMachine::Evm(80002)` 与 Circle Amoy 测试 USDC，并启用严格协议资产
inspector、Root HFT registry 治理以及仅限 `call_data = None` 的 HFT `send`。

The HFT registry remains empty and all debt ceilings/limits remain zero by
default. The runtime upgrade does not activate a lane; governance must still
verify migration state, submit deployment evidence, register the HFT token and
collateral, and set conservative limits.

HFT registry 默认仍为空，全部债务上限与限额仍为零。runtime 升级不会自动启用通道；
治理仍须验证迁移状态、提交部署证据、注册 HFT token/collateral 并设置保守限额。

```bash
cargo test -p nexus-runtime
cargo build -p nexus-node --release
```

## Runtime benchmark record / Runtime 基准记录

All USDX dispatchables were benchmarked on 2026-07-12 with Substrate benchmark
CLI 53.0.0, 50 steps, 20 repeats, compiled Wasm, `max` regression and measured
PoV. Benchmark builds use the strict protocol-asset inspector and canonical HFT
registry adapter so mint/redeem weights include the production validation reads.
Runtime spec 105 uses the same strict inspector.

全部 USDX dispatchable 已于 2026-07-12 使用 Substrate benchmark CLI 53.0.0、
50 steps、20 repeats、compiled Wasm、`max` 回归和 measured PoV 完成实测。
Benchmark build 使用严格协议资产 inspector 与 canonical HFT registry adapter，
因此 mint/redeem 权重包含生产校验读取；生产 Phase 0 仍使用全拒绝 inspector。
runtime spec 105 使用同一严格 inspector。

## Test / 测试

```bash
cargo test -p pallet-usdx
```
