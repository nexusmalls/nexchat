# Phase 4 Trading Modules Development

This report tracks Phase 4 development for the isolated prediction trading
graph. Phase 4 is closed at the workspace/isolation boundary. This closure does
not claim runtime readiness or production activation.

本报告跟踪隔离 prediction 交易图中的 Phase 4 开发。Phase 4 已在 workspace/
隔离测试边界收口；本文不宣称 runtime 就绪或生产启用。

## Required order / 强制顺序

1. Legacy Swaps
2. Combinatorial Tokens
3. NeoSwaps
4. Orderbook
5. Parimutuel
6. Hybrid Router

All modules remain workspace-only. Runtime indices, production call filtering,
generated weights, runtime APIs/RPC, E2E, and activation remain Phase 6/7 work.

所有模块仍仅存在于 workspace。Runtime index、生产 call filter、实测权重、
runtime API/RPC、E2E 和启用仍属于 Phase 6/7。

## Batch 1: Legacy Swaps baseline / 第一批：Legacy Swaps 基线

The imported Legacy Swaps implementation was re-verified without changing its
algorithm or asset model:

- 108 library/mock tests pass.
- Runtime-valid WASM no-std compilation passes.
- The `runtime-benchmarks` feature compiles.
- Legacy Swaps remains separate from NeoSwaps and must be disabled by default
  when runtime wiring is eventually added.

已在不修改算法或资产模型的前提下复核 Legacy Swaps：108 项 library/mock
测试、runtime-valid WASM no-std 编译及 `runtime-benchmarks` feature 编译均
通过。未来 runtime 接线时仍须与 NeoSwaps 隔离，并默认关闭。

## Batch 2: Combinatorial Tokens baseline / 第二批：Combinatorial Tokens 基线

The imported Combinatorial Tokens implementation was re-verified and connected
to the Phase 2 collateral fixture before advancing to NeoSwaps:

- FRAME Assets, Prediction Control, and Prediction Collateral are part of its
  isolated mock runtime.
- USDX market admission uses the live collateral policy rather than the static
  shared mock policy.
- USDX is minted only as a real FRAME asset and mirrored through
  `PredictionCollateral::deposit`.
- 753 library/mock tests pass, including the imported split, merge, redeem,
  cryptographic-id, and fuel-bound vectors.
- New split/merge roundtrip and winner-redemption tests prove that combinatorial
  operations do not change mirror issuance or real escrow.
- Runtime-valid WASM no-std compilation passes.
- The `runtime-benchmarks` feature compiles.
- A normalized split/merge roundtrip is pinned in the differential harness.

在推进 NeoSwaps 前，Combinatorial Tokens 隔离 mock 已接入 FRAME Assets、
Prediction Control 与 Prediction Collateral；USDX 市场准入使用实时 collateral
policy，且镜像只通过 `PredictionCollateral::deposit` 建立。753 项 library/mock
测试通过，包括导入的 split、merge、redeem、加密 ID 与 fuel 上限向量；新增
split/merge 往返和赢家赎回测试，证明操作不改变镜像发行量或真实托管。
runtime-valid WASM no-std 和 `runtime-benchmarks` feature 编译通过；归一化差分
框架已固定 split/merge 往返场景。

## Batch 3: NeoSwaps collateral integration / 第三批：NeoSwaps 抵押集成

NeoSwaps' isolated mock now uses the Phase 2 collateral boundary instead of a
static foreign-asset policy:

- FRAME Assets, Prediction Control, and Prediction Collateral are part of the
  mock runtime.
- USDX is force-created as a live test asset, whitelisted under `Full`, and
  mirrored only through `PredictionCollateral::deposit`.
- Prediction Markets uses the live collateral pallet as `BaseAssetPolicy`.
- AMM buy, sell, join, exit, fee accrual, and fee withdrawal are covered with
  native upstream vectors and live-backed USDX conservation tests.
- USDX mirror issuance remains exactly equal to real escrow across every
  covered operation.
- The full NeoSwaps suite passes 440 tests.
- Runtime-valid WASM no-std and `runtime-benchmarks` compilation pass.

NeoSwaps 隔离 mock 已使用 Phase 2 collateral 边界替换静态外部资产策略：
mock runtime 接入 FRAME Assets、Prediction Control 与 Prediction Collateral；
USDX 仅通过 `PredictionCollateral::deposit` 建立镜像；Prediction Markets
使用实时 collateral pallet 作为 `BaseAssetPolicy`。native 上游向量与实时 USDX
测试共同覆盖 AMM buy、sell、join、exit、fee accrual 和 fee withdrawal，并固定
USDX 镜像发行量始终等于真实托管。NeoSwaps 完整测试共 440 项通过，
runtime-valid WASM no-std 与 `runtime-benchmarks` 编译通过。

## Batch 4: Orderbook collateral integration / 第四批：Orderbook 抵押集成

Orderbook keeps its intended boundary: it consumes already-admitted market
records and does not duplicate market-creation collateral policy checks. Its
isolated mock and accounting tests now use live-backed USDX balances:

- FRAME Assets, Prediction Control, and Prediction Collateral are part of the
  mock runtime.
- USDX balances for maker, taker, and fee recipient are minted as real assets
  and mirrored only through `PredictionCollateral::deposit`.
- Foreign-collateral partial and complete fills cover maker named reserves,
  taker proceeds, outcome transfer, external fees, and remaining order state.
- Cancellation restores the maker's free balance and clears the named reserve.
- Mirror issuance remains equal to real escrow across placement, partial/full
  fills, fee transfer, and cancellation.
- A normalized partial-fill accounting vector is pinned in the differential
  harness.
- The full Orderbook suite passes 37 tests.

Orderbook 保持既定边界：仅消费已经准入的市场记录，不重复执行市场创建阶段的抵押
策略检查。其隔离 mock 已接入 FRAME Assets、Prediction Control 和 Prediction
Collateral；maker、taker 与 fee recipient 的 USDX 仅通过真实资产存入建立镜像。
外部抵押 partial/full fill 测试覆盖 maker 命名储备、taker 收款、结果代币转移、
外部费用和剩余订单状态；撤单测试验证 free balance 恢复及命名储备清零。挂单、
部分/完整成交、费用转移与撤单全程保持镜像发行量等于真实托管。差分框架固定了
归一化 partial-fill 账务向量。Orderbook 完整测试共 37 项通过。

## Batch 5: Parimutuel collateral integration / 第五批：Parimutuel 抵押集成

Parimutuel keeps market admission at the market-creation boundary while its
isolated mock now uses live-backed USDX balances:

- FRAME Assets, Prediction Control, and Prediction Collateral are part of the
  mock runtime.
- Participant and fee-recipient USDX balances are mirrored only through
  `PredictionCollateral::deposit`.
- The winner branch verifies fee collection, proportional pot payout, empty
  final pot, and unchanged mirror issuance/escrow.
- The no-winner branch verifies both refunds after fees, empty final pot, and
  unchanged mirror issuance/escrow.
- The normalized no-winner accounting branch is pinned in the differential
  harness.
- The full Parimutuel suite passes 42 tests.

Parimutuel 继续由市场创建边界负责抵押准入，其隔离 mock 已接入 FRAME Assets、
Prediction Control 与 Prediction Collateral；参与者和 fee recipient 的 USDX
仅通过 `PredictionCollateral::deposit` 建立镜像。有赢家分支验证费用、彩池
支付、最终空池及镜像守恒；无赢家分支验证双方扣费退款、最终空池及镜像守恒。
差分框架固定了归一化无赢家账务分支。Parimutuel 完整测试共 42 项通过。

## Batch 6: Hybrid Router collateral integration / 第六批：Hybrid Router 抵押集成

Hybrid Router remains restricted to NeoSwaps and Orderbook. Its integrated mock
now uses the Phase 2 collateral boundary:

- FRAME Assets, Prediction Control, and Prediction Collateral are part of the
  combined Prediction Markets + NeoSwaps + Orderbook mock graph.
- Prediction Markets uses the live collateral pallet as `BaseAssetPolicy`, and
  USDX is mirrored only through `PredictionCollateral::deposit`.
- Existing buy/sell numerical soft-failure fallback and order-price limit tests
  were re-verified.
- A foreign-collateral soft-failure test proves that an AMM numerical failure
  falls back to a reserved Orderbook limit order without changing mirror
  issuance or escrow.
- A foreign-collateral maximum-price failure test proves transactional rollback
  across AMM and Orderbook state.
- The full Hybrid Router suite passes 48 tests.

Hybrid Router 继续仅路由 NeoSwaps 与 Orderbook。组合 mock 图已接入 FRAME
Assets、Prediction Control 与 Prediction Collateral；Prediction Markets 使用
实时 collateral pallet 作为 `BaseAssetPolicy`，USDX 仅通过
`PredictionCollateral::deposit` 建立镜像。既有 buy/sell 数值 soft failure
回退与订单价格限制测试已复核；新增外部抵押测试证明 AMM 数值失败会回退为带储备
的 Orderbook 限价单，且最大价格失败会跨 AMM/Orderbook 完整事务回滚。
Hybrid Router 完整测试共 48 项通过。

## Differential closure / 差分收口

The differential harness remains pinned to upstream commit
`39ad8d60aa2f7af0a465d58c5e87dcc509602df5`. Phase 4 adds normalized accounting
goldens for:

- Combinatorial Tokens native split/merge roundtrip.
- Orderbook native partial fill, remaining order, named reserve, and fee flow.
- Parimutuel native no-winner refunds, fees, share burns, and empty final pot.

NeoSwaps, Legacy Swaps, and Hybrid Router retain their imported upstream
mathematical vectors and exact routing/fallback assertions in their own suites.
The differential crate now passes five active scenarios plus one ignored manual
golden-capture helper.

差分框架继续固定上游 commit `39ad8d60`。Phase 4 新增 Combinatorial Tokens
native split/merge 往返、Orderbook native 部分成交/剩余订单/命名储备/费用流，
以及 Parimutuel native 无赢家退款/费用/share 销毁/最终空池的归一化 golden。
NeoSwaps、Legacy Swaps 与 Hybrid Router 继续由各自完整测试中的上游数学向量和
精确路由/回退断言约束。差分 crate 共五项活动场景通过，另有一项手动 golden
采集 helper 保持 ignored。

## Verification commands / 验证命令

```bash
cargo test -p zrml-swaps --features mock --lib
cargo test -p zrml-combinatorial-tokens --features mock --lib
cargo test -p zrml-neo-swaps --features mock --lib
cargo test -p zrml-orderbook --features mock --lib
cargo test -p zrml-parimutuel --features mock --lib
cargo test -p zrml-hybrid-router --features mock --lib
cargo test -p prediction-differential
RUSTFLAGS="--cfg substrate_runtime" cargo check -p zrml-swaps \
  --no-default-features --target wasm32-unknown-unknown
RUSTFLAGS="--cfg substrate_runtime" cargo check -p zrml-combinatorial-tokens \
  --no-default-features --target wasm32-unknown-unknown
RUSTFLAGS="--cfg substrate_runtime" cargo check -p zrml-neo-swaps \
  --no-default-features --target wasm32-unknown-unknown
RUSTFLAGS="--cfg substrate_runtime" cargo check -p zrml-orderbook \
  --no-default-features --target wasm32-unknown-unknown
RUSTFLAGS="--cfg substrate_runtime" cargo check -p zrml-parimutuel \
  --no-default-features --target wasm32-unknown-unknown
RUSTFLAGS="--cfg substrate_runtime" cargo check -p zrml-hybrid-router \
  --no-default-features --target wasm32-unknown-unknown
cargo check -p zrml-swaps --features runtime-benchmarks
cargo check -p zrml-combinatorial-tokens --features runtime-benchmarks
cargo check -p zrml-neo-swaps --features runtime-benchmarks
cargo check -p zrml-orderbook --features runtime-benchmarks
cargo check -p zrml-parimutuel --features runtime-benchmarks
cargo check -p zrml-hybrid-router --features runtime-benchmarks
```

## Formal closure record / 正式收口记录

Phase 4 closed on 2026-07-12 at the isolated workspace boundary:

- All 1,428 tests across the six trading modules pass: 108 Legacy Swaps, 753
  Combinatorial Tokens, 440 NeoSwaps, 37 Orderbook, 42 Parimutuel, and 48 Hybrid
  Router.
- All Phase 4 exit criteria in `ZEITGEIST_FULL_PORT_DEV_SPEC.md` are covered:
  upstream mathematical vectors; AMM buy/sell/join/exit/fee conservation;
  Orderbook partial fill/cancel/fee; Hybrid Router soft-failure fallback;
  Parimutuel winner/no-winner/fee; and Combinatorial split/merge/redeem/fuel.
- No-std and `runtime-benchmarks` compilation pass for all six modules.
- Production runtime wiring, generated production weights, runtime APIs/RPC,
  E2E activation, and the imported fuzz campaign remain Phase 6/7 work and are
  not Phase 4 closure blockers.

Phase 4 于 2026-07-12 在隔离 workspace 边界正式收口。六个交易模块共 1,428
项测试全部通过；开发规范规定的上游数学向量、AMM 全操作守恒、Orderbook
partial fill/cancel/fee、Hybrid Router soft-failure 回退、Parimutuel
赢家/无赢家/费用及 Combinatorial split/merge/redeem/fuel 退出条件均已覆盖。
六模块 no-std 与 `runtime-benchmarks` 编译通过。生产 runtime 接线、生产权重、
runtime API/RPC、E2E 启用及 fuzz campaign 仍归 Phase 6/7，不阻塞 Phase 4 收口。
