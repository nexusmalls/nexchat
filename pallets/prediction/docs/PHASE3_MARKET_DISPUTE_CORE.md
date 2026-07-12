# Phase 3 Market and Dispute Core Closure

This report closes Phase 3 development for the isolated prediction mock graph.
It does not wire or activate any prediction pallet in the Nexus production
runtime.

本报告完成隔离 prediction mock 图中的 Phase 3 开发收口；不在 Nexus 生产
runtime 中接线或启用任何 prediction pallet。

## Scope / 范围

The verified dependency order is:

1. `zrml-market-commons`
2. `zrml-authorized`
3. `zrml-court`
4. `zrml-global-disputes`
5. `zrml-prediction-markets`

All five crates remain workspace-only. Prediction Markets uses the Phase 2
collateral adapter in its mock, and no trading-pool implementation is required
for the lifecycle scenarios below.

五个 crate 仍仅存在于 workspace。Prediction Markets mock 使用 Phase 2
collateral adapter；以下生命周期验证不依赖交易池实现。

## Exit-gate coverage / 退出条件覆盖

- Permissionless lifecycle: categorical and scalar integration scenarios cover
  creation, complete-set purchase, report, dispute, resolution, and redemption.
- Advised lifecycle: approval/rejection and oracle/outsider bond branches cover
  both successful admission and rejected-market cleanup.
- Trusted lifecycle: direct trusted report and resolution is covered without a
  dispute mechanism.
- Complete sets: categorical and scalar buy/sell/redeem tests pin issuance,
  balances, and payout boundaries.
- Reporting: oracle-window, outsider-window, invalid-outcome, and deadline
  boundaries are covered for block and timestamp markets.
- Resolution: Authorized, Court, and full Court-to-GlobalDisputes escalation
  paths all reach `Resolved`.
- Bonds: creation, advisory, oracle, outsider, dispute, early-close, court
  stake, and appeal-bond slash/refund paths are covered. The global-dispute
  integration test additionally proves that all Court appeal reserves return
  to zero when the final outcome makes the appeals justified.
- Automatic processing: block- and timestamp-based close, stalled recovery,
  report resolution, Court resolution, and GlobalDisputes resolution run with
  `MockBlockU32`.

- Permissionless 生命周期覆盖创建、complete-set 买入、报告、争议、决议和赎回，
  并同时覆盖 categorical 与 scalar。
- Advised 生命周期覆盖批准、拒绝、oracle/outsider 保证金及拒绝后的清理。
- Trusted 生命周期覆盖无争议机制的直接报告和决议。
- Complete-set 测试固定 categorical/scalar 的发行量、余额和 payout 边界。
- 报告测试覆盖 block/timestamp 市场的 oracle 窗口、outsider 窗口、非法结果和
  deadline 边界。
- Authorized、Court 以及 Court 升级 GlobalDisputes 的三条路径均到达
  `Resolved`。
- creation、advisory、oracle、outsider、dispute、early-close、Court stake 和
  appeal bond 的 slash/refund 分支均有覆盖；全局争议集成测试还证明最终结果确认
  上诉合理时，Court appeal reserve 全部归零。
- 自动 close/resolve、停滞恢复及三种决议均运行在 `MockBlockU32` 下。

## Court-to-GlobalDisputes correctness fix / 正确性修复

The fixed upstream scenario stopped after starting a global dispute. Extending
it through voting and automatic resolution exposed a latent failure:
`Court::on_global_dispute` deleted `CourtInfo`, while Prediction Markets later
called `Court::exchange` to settle outcome-dependent appeal bonds. Resolution
therefore rolled back with `CourtNotFound`.

The Nexus fix:

1. clears the mechanism-owned auto-resolution schedule before destructive
   escalation;
2. removes Court draws and unlocks participants at escalation;
3. retains `CourtInfo` only until the final outcome-dependent appeal-bond
   exchange;
4. removes the retained Court and both market/court mappings atomically after
   exchange.

固定 upstream 场景只验证启动全局争议。将其延伸到投票和自动决议后暴露出潜在
故障：`Court::on_global_dispute` 已删除 `CourtInfo`，但 Prediction Markets
随后仍需调用 `Court::exchange` 按最终结果结算 appeal bond，导致
`CourtNotFound` 并回滚。

Nexus 修复会先清理旧自动决议队列，升级时删除 draws 并解锁参与者，仅保留
`CourtInfo` 到最终 appeal-bond exchange 完成，然后原子删除 Court 及双向映射。
该行为差异归类为独立 `BUGFIX`；生产权重必须在 Phase 7 重新生成。

## Differential baseline / 差分基线

The fixed Zeitgeist commit
`39ad8d60aa2f7af0a465d58c5e87dcc509602df5` was checked out separately and the
same five upstream unit suites were executed:

- Market Commons: 19
- Authorized: 15
- Court: 118
- Global Disputes: 42
- Prediction Markets: 179
- Total: 373 upstream tests passed

The Nexus port passed the same scenario inventory after normalizing SDK,
`BlockNumber`, account, asset-admission, and outer event-path differences.
Nexus has three additional Prediction Markets tests:

- fully collateralized foreign-asset fixture without direct ORML genesis mint;
- live collateral-policy admission;
- foreign-asset edit flow.

Nexus therefore passes 376 tests across the five crates (19 + 15 + 118 + 42 +
182). The Court escalation expectation is the one intentional business
deviation: upstream expects immediate Court deletion, while Nexus retains only
the settlement record until appeal bonds are exchanged.

已单独检出固定 Zeitgeist commit，并运行相同五个 upstream 单测组，共 373 项
全部通过。归一化 SDK、`BlockNumber`、账户、资产准入和事件外层路径差异后，
Nexus 通过相同场景，并额外通过 3 项 foreign-collateral 测试，合计 376 项。
唯一有意的业务差异是 Court 升级记录保留到 appeal-bond exchange 完成。

## Differential baseline harness / 差分基线框架

Added Nexus-only `prediction-differential` with normalized snapshot comparison
against upstream-derived goldens pinned at commit `39ad8d60`. The first catalog
covers four native lifecycle scenarios:

- `permissionless_resolve_native`
- `authorized_dispute_native`
- `trusted_market_native`
- `scalar_lifecycle_native`

Normalization rules follow spec §15.2: native asset identity (`Ztg` alias),
`u32` `BlockNumber` semantics, stable account ids, and foreign-asset `u64`
width. Compared fields include market status, resolved outcome, tracked native
balances for core accounts, and bond settlement flags.

新增 Nexus 专用 `prediction-differential`，以归一化快照对比固定于 commit
`39ad8d60` 的上游 golden。首批目录覆盖 4 个 native 生命周期场景，归一化规则
遵循规范 §15.2，比较市场状态、决议结果、核心账户 native 余额与 bond 结算标记。

Added Phase 3 property tests in prediction-markets for native and USDX
complete-set buy/sell roundtrip conservation.

prediction-markets 新增 native 与 USDX complete-set 买卖往返守恒 property 测试。

## Verification / 验证

The closure gates passed:

- complete five-crate unit suites;
- runtime-valid WASM no-std checks using
  `RUSTFLAGS="--cfg substrate_runtime"` and `wasm32-unknown-unknown`;
- `runtime-benchmarks` feature compilation for all five crates;
- focused Court-to-GlobalDisputes lifecycle and appeal-bond conservation test;
- `cargo test -p prediction-differential`;
- `cargo test -p zrml-prediction-markets --features mock properties`;
- formatting and whitespace checks.

收口门禁包括五个 crate 完整单测、runtime-valid WASM no-std、五个 crate 的
`runtime-benchmarks` feature 编译、Court-to-GlobalDisputes 完整生命周期与
appeal-bond 守恒、`prediction-differential` 差分基线、complete-set property
测试，以及格式检查。

Production runtime wiring, origins, pallet indices, generated production
weights, call filtering, RPC, E2E, and activation remain deferred to later
phases. `Asset::Ztg` is intentionally unchanged; the separately reviewable
`Ztg -> Native` rename may begin only after this differential baseline.

生产 runtime 接线、origin、pallet index、生产权重、调用过滤、RPC、E2E 和启用
仍延后处理。`Asset::Ztg` 本阶段有意保持不变；本差分基线通过后，才可单独启动
可审查的 `Ztg -> Native` 重命名。
