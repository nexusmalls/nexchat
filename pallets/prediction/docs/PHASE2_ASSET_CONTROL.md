# Phase 2 Asset/Control Development Closure

This report closes Phase 2 development after all scoped gates were verified.
It does not claim runtime readiness or production candidacy, and it does not
include runtime filtering, runtime/node wiring, production validation,
production weights, or activation.

本报告在全部范围内门禁验证通过后完成 Phase 2 开发收口；不宣称 runtime
已就绪或具备生产候选资格，且不包含 runtime 过滤、runtime/node 接线、生产
validator、生产权重或启用工作。

## Completed in this batch / 本批已完成

- Unified `PredictionBaseAssetPolicy<AssetId>` under
  `zeitgeist_primitives::traits`. Prediction Markets and all current mock
  runtimes now use that single trait.
- Added Nexus-only `pallet-prediction-control` with explicit storage version 1,
  default `Disabled` global mode, and default-disabled flags for all 12 modules.
- Added governance-only mode/module updates, events, a runtime-independent
  read-only provider trait, and pure mode/module gate functions.
- Added SCALE/FRAME-compatible `PredictionMode`, `PredictionModule`, and
  `CallClass` types, including `DecodeWithMemTracking`.
- Added Phase 2 non-production weight interfaces. Phase 7 must regenerate
  benchmark weights before production wiring.

## Completed in batch 2 / 第二批已完成

- Added Nexus-only `pallet-prediction-collateral` with explicit storage version
  1, an initially empty whitelist, per-asset deposit pauses, and a global
  deposit pause.
- Added transactional deposit/withdraw paths with the fixed order
  `pallet-assets user -> sovereign -> ORML mint` and
  `ORML burn -> pallet-assets sovereign -> user`.
- Enforced exact `ORML mirror issuance == pallet-assets escrow` checks before
  and after every mutation. The pallet stores no duplicate user balance.
- Deposits require `Full`, whitelist admission, unpaused state, a runtime
  `AssetValidator`, and a consistent mirror. Withdrawals intentionally ignore
  mode, whitelist, pause, and validator changes while preserving ORML
  free/liquidity checks.
- Implemented `PredictionBaseAssetPolicy<u64>` from the same live deposit gates
  and exposed read-only account, mirror, issuance, escrow, invariant, and policy
  helpers.
- Added an isolated FRAME 45 mock with Balances, Assets, ORML Tokens/Currencies,
  PredictionControl, and PredictionCollateral. Its validator checks real asset
  existence and supports frozen/protocol-not-ready simulation.
- Added Phase 2 non-production collateral weights. Phase 7 must regenerate them.

- 新增 Nexus 专用 `pallet-prediction-collateral`，显式 storage version 1，
  白名单初始为空，并提供逐资产与全局存入暂停。
- 新增事务化存取路径，固定顺序为
  `pallet-assets 用户 -> 主权账户 -> ORML 铸造` 与
  `ORML 销毁 -> pallet-assets 主权账户 -> 用户`。
- 每次变更前后严格检查
  `ORML 镜像总发行量 == pallet-assets 托管余额`，且不保存用户余额副本。
- 存入要求 `Full`、白名单、未暂停、runtime `AssetValidator` 与镜像一致；
  提取有意不受模式、白名单、暂停和验证器变化影响，但继续执行 ORML
  free/liquidity 检查。
- 使用相同实时门禁实现 `PredictionBaseAssetPolicy<u64>`，并公开账户、镜像、
  发行量、托管、不变量和策略只读 helper。
- 新增隔离的 FRAME 45 mock，包含 Balances、Assets、ORML Tokens/Currencies、
  PredictionControl 与 PredictionCollateral；validator 检查真实资产存在性，
  并支持冻结及协议未就绪模拟。
- 新增 Phase 2 非生产 collateral 权重；Phase 7 必须重新生成。

## Completed in batch 3 / 第三批已完成

- Prediction Markets' isolated mock runtime now wires FRAME Assets,
  PredictionControl, and PredictionCollateral directly. Its
  `BaseAssetPolicy` is the real collateral pallet rather than the shared static
  mock policy.
- The mock validator requires both a real `pallet-assets` entry and
  `AssetStatus::Live`. USDX protocol readiness is represented by that live
  status only in this isolated mock; the production protocol/PSM adapter
  remains Phase 6 work.
- The ORML foreign-asset genesis shortcut was removed. Setup force-creates
  USDX, mints `INITIAL_BALANCE + min_balance` real assets to accounts `0..69`,
  enables `Full`, whitelists USDX, and deposits exactly `INITIAL_BALANCE`
  through PredictionCollateral. The retained minimum unit satisfies
  `Preservation::Preserve` without expanding ORML test balances.
- Integration tests exercise native acceptance, live/whitelisted USDX,
  unapproved assets, `Trading`, both pause layers, and the real
  `pallet-assets` Frozen/Live transition, including recovery after each gate is
  lifted.
- Conservation tests pin initial mirror issuance to escrow and verify ordinary
  ORML transfers do not change that total.
- Enabling collateral whitelist admission now validates the asset immediately;
  removal remains available even when the asset is invalid.

- Prediction Markets 隔离 mock runtime 已直接接入 FRAME Assets、
  PredictionControl 与 PredictionCollateral，并以真实 collateral pallet
  作为 `BaseAssetPolicy`，不再使用共享静态 mock policy。
- mock validator 同时要求真实 `pallet-assets` 条目存在且
  `AssetStatus::Live`。仅在该隔离 mock 中，USDX 协议 readiness 由 Live
  状态代表；生产协议/PSM adapter 仍留待 Phase 6。
- 已删除 ORML 外部资产 genesis 直注捷径。初始化会 force-create USDX，
  为 `0..69` 账户铸造 `INITIAL_BALANCE + min_balance` 的真实资产，设置
  `Full`、批准 USDX，再经 PredictionCollateral 存入恰好
  `INITIAL_BALANCE`。保留的最小单位满足 `Preservation::Preserve`，不会扩大
  ORML 测试余额语义。
- 集成测试覆盖 Native、Live 且白名单 USDX、未批准资产、`Trading`、两层
  pause、真实 `pallet-assets` Frozen/Live 转换及各门禁撤销后的恢复。
- 守恒测试固定初始化镜像发行量等于托管余额，并验证常规 ORML 转账不改变总量。
- collateral 开启白名单时现会立即验证资产；即使资产无效，关闭白名单仍可执行。

## Source-audited call registry / 源码核对调用注册表

The registry was checked against every `#[pallet::call_index]` in the current
source of the 12 specified business pallets. The actual total is **68**, not an
assumed round number.

注册表逐一核对了 12 个指定业务 pallet 当前源码中的每个
`#[pallet::call_index]`。实际总数为 **68**，并非预设的整数估计。

| Module | Dispatchables |
|---|---:|
| PredictionMarkets | 19 |
| Authorized | 1 |
| Court | 10 |
| GlobalDisputes | 6 |
| LegacySwaps | 9 |
| NeoSwaps | 9 |
| Orderbook | 3 |
| Parimutuel | 3 |
| HybridRouter | 2 |
| CombinatorialTokens | 3 |
| Futarchy | 1 |
| Styx | 2 |
| **Total / 合计** | **68** |

Tests pin the count at 68 and require every `(module, call_index)` key to be
unique and every call name to be non-empty. They also run every registered call
through the complete mode/module matrix and pin security-sensitive mixed-leg
and governance classifications. In particular, Neo Swaps `combo_sell` is
`RiskIncreasing`, because its buy legs can increase pool exposure; it is not a
pure unwind.

测试将数量固定为 68，并要求每个 `(module, call_index)` key 唯一、调用名称
非空；同时让所有注册调用通过完整模式/模块矩阵，并固定安全敏感的混合腿及治理
分类。尤其是 Neo Swaps `combo_sell` 因买入腿可能增加池风险敞口，被归类为
`RiskIncreasing`，而非纯 `Unwind`。

## Gate semantics / 门禁语义

- `Disabled`: `Unwind`, `AdminRecovery`
- `ResolutionOnly`: `Resolution`, `Unwind`, `AdminRecovery`
- `Trading`, `Full`: all four classes
- Every business call additionally requires its module flag to be enabled.
- Prediction-control governance calls are intentionally outside the registry and
  must be self-exempted by the Phase 6 runtime call filter.

所有业务调用还必须通过对应模块开关。Prediction-control 自身治理调用有意不进入
registry，Phase 6 runtime call filter 必须先对其自豁免。

## Deferred boundaries / 延后边界

- `create_market_and_deploy_pool` is registered as PredictionMarkets /
  `RiskIncreasing`; Phase 6 must additionally require the NeoSwaps gate.
- HybridRouter `buy`/`sell` are registered under HybridRouter; Phase 6 must also
  require both NeoSwaps and Orderbook gates.
- `Trading` versus `Full` foreign-collateral semantics cannot be enforced by
  `CallClass` alone. The Phase 6 filter/collateral boundary must inspect relevant
  call data and policy state.
- Automatic `on_initialize` market close/settlement hooks are not dispatchables
  and must not be blocked by the future call filter.
- No runtime or node production wiring is included in this batch.

## Closure verification / 收口验证

The final verification passed:

- `cargo fmt --all`
- Prediction Control: 11 tests
- Prediction Collateral: 17 tests
- Prediction Markets with `mock`: 182 tests
- Phase 0 conservation/rollback smoke: 4 tests
- all 13 imported prediction pallet suites
- runtime-valid WASM no-std checks for Control, Collateral, Primitives, and
  Prediction Markets using `RUSTFLAGS="--cfg substrate_runtime"`
- runtime-benchmark feature checks for Control, Collateral, and Prediction
  Markets
- strict `clippy -D warnings --no-deps` for both new Phase 2 pallets

最终验证全部通过：格式检查、Control 11 项测试、Collateral 17 项测试、
Prediction Markets 182 项测试、Phase 0 守恒/回滚 smoke 4 项测试、13 个导入
预测 pallet 的完整测试组、Control/Collateral/Primitives/Prediction Markets
的 runtime-valid WASM no-std 检查、三个相关 crate 的 runtime-benchmark
feature 编译，以及两个 Phase 2 新 pallet 的 `clippy -D warnings --no-deps`。

Production `AssetValidator` (including USDX protocol/PSM readiness), runtime
call filtering and self-exemption, runtime/node wiring, generated production
weights, and activation remain Phase 6/7 work. Therefore Phase 2 development is
closed, but the prediction subsystem must remain unwired and inactive.

生产 `AssetValidator`（含 USDX 协议/PSM readiness）、runtime 调用过滤与自豁免、
runtime/node 接线、生成的生产权重及启用仍属于 Phase 6/7。因此 Phase 2 开发已
收口，但预测子系统必须继续保持未接线、未启用状态。
