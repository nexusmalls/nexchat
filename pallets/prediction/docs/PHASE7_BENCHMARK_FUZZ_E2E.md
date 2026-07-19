# Phase 7 Benchmark, Fuzz and E2E / Phase 7 基准、模糊测试与端到端测试

## Scope / 范围

Phase 7 replaces every integration or upstream weight with measurements from
the Nexus runtime, ports the upstream fuzz targets, adds Nexus adapter
properties, and restores prediction E2E coverage.

Phase 7 使用 Nexus runtime 实测值替换全部集成期或上游权重，移植上游 fuzz
targets，补充 Nexus adapter property tests，并恢复 prediction E2E。

No prediction business module may be enabled while any runtime `WeightInfo`
still delegates to `()`, uses a Phase 2 estimate, or contains an upstream-only
measurement.

只要任一 runtime `WeightInfo` 仍委托给 `()`、使用 Phase 2 估算或仅包含上游实测值，
任何 prediction 业务模块都不得启用。

## Benchmark profile / 基准配置

The first Nexus production-weight batch was generated on 2026-07-13 with:

- Substrate benchmark CLI 53.0.0;
- 50 steps and 20 repeats;
- compiled Wasm execution;
- `max` time and proof-size regression;
- measured PoV mode;
- Nexus development genesis;
- Intel Xeon E5-2686 v4 reference host.

首批 Nexus 生产权重于 2026-07-13 使用以下配置生成：

- Substrate benchmark CLI 53.0.0；
- 50 steps、20 repeats；
- compiled Wasm；
- 时间与 proof size 均使用 `max` 回归；
- measured PoV；
- Nexus development genesis；
- Intel Xeon E5-2686 v4 参考主机。

Canonical command:

```bash
./target/release/nexus-node benchmark pallet \
  --runtime target/release/wbuild/nexus-runtime/nexus_runtime.compact.wasm \
  --genesis-builder runtime \
  --pallets <pallet_name> \
  --extrinsic '*' \
  --steps 50 \
  --repeat 20 \
  --wasm-execution compiled \
  --output-analysis max \
  --default-pov-mode measured \
  --output-pov-analysis max \
  --output <output.rs>
```

## Completed batch 1 / 已完成首批

- [x] `pallet_prediction_control`: both governance calls benchmarked.
- [x] `pallet_prediction_collateral`: deposit, withdraw and three governance
  calls benchmarked against the full Assets ↔ ORML mirror path.
- [x] `orml_currencies`: native and non-native transfer/update paths
  benchmarked in the Nexus asset configuration.
- [x] `orml_tokens`: all five dispatchables benchmarked in the Nexus prediction
  asset configuration.
- [x] Runtime wiring now uses each crate's benchmark-generated
  `SubstrateWeight<Runtime>` and no longer wraps the ORML `()` fallback.

- [x] `pallet_prediction_control`：两个治理调用完成 benchmark。
- [x] `pallet_prediction_collateral`：deposit、withdraw 与三个治理调用均按完整
  Assets ↔ ORML 镜像路径完成 benchmark。
- [x] `orml_currencies`：原生与非原生 transfer/update 路径均按 Nexus 资产配置实测。
- [x] `orml_tokens`：五个 dispatchable 均按 Nexus prediction 资产配置实测。
- [x] Runtime 已切换到各 crate 的 `SubstrateWeight<Runtime>`，不再包装 ORML
  的 `()` fallback。

## Completed batch 2 / 已完成第二批

Regenerated against the Nexus runtime via `scripts/benchmark-prediction-zrml-weights.sh`:

- [x] `zrml_authorized` — 50/20
- [x] `zrml_court` — 20/10 (multi-component; rerun at 50/20 before production)
- [x] `zrml_prediction_markets` — 20/10 (multi-component; rerun at 50/20 before production)
- [x] `zrml_global_disputes` — 50/20
- [x] `zrml_swaps` — 50/20
- [x] `zrml_orderbook` — 50/20
- [x] `zrml_parimutuel` — 50/20
- [x] `zrml_hybrid_router` — 50/20
- [x] `zrml_styx` — 50/20
- [x] `zrml_neo_swaps` — 10/5 (heavy LMSR paths; rerun at 50/20 before production)
- [x] `zrml_combinatorial_tokens` — 10/5 (multi-component; rerun at 50/20 before production)
- [x] `zrml_futarchy` — 50/20

- [x] `zrml_authorized` — 50/20
- [x] `zrml_court` — 20/10（多 component；生产前需以 50/20 重跑）
- [x] `zrml_prediction_markets` — 20/10（多 component；生产前需以 50/20 重跑）
- [x] `zrml_global_disputes` — 50/20
- [x] `zrml_swaps` — 50/20
- [x] `zrml_orderbook` — 50/20
- [x] `zrml_parimutuel` — 50/20
- [x] `zrml_hybrid_router` — 50/20
- [x] `zrml_styx` — 50/20
- [x] `zrml_neo_swaps` — 10/5（重 LMSR 路径；生产前需以 50/20 重跑）
- [x] `zrml_combinatorial_tokens` — 10/5（多 component；生产前需以 50/20 重跑）
- [x] `zrml_futarchy` — 50/20

## Fuzz and E2E / Fuzz 与 E2E

- [x] Upstream fuzz targets compile in the Nexus workspace (`cargo check` on all
  six fuzz crates).
- [x] Collateral mirror property tests live in `pallet-prediction-collateral`
  proptest coverage.
- [x] Host-side smoke runner: `scripts/fuzz-prediction-smoke.sh` /
  `npm run fuzz:prediction:smoke`.
- [x] TypeScript E2E suites:
  - `prediction-emergency-pause`
  - `prediction-collateral-gate`
  - `prediction-collateral-usdx`
  - `prediction-usdx-market`
  - `prediction-community-fee` (needs USDX credit + neo-swaps)
  - `prediction-court-gate`
  - `prediction-core-lifecycle`
  - `prediction-authorized-dispute`
  - `prediction-orderbook-smoke`
  - `prediction-neo-swaps`
  - `prediction-hybrid-router`
  - `prediction-combinatorial`
  - `prediction-parimutuel`
  - `prediction-futarchy`
  - `prediction-styx`
  - npm scripts: `e2e:prediction` and per-suite aliases
  - `--dev` note: `ensureUsdxProtocolAsset` bootstraps asset `900_000` when
    `InitializeUsdxProtocolAssets` migration has not run

- [x] TypeScript E2E suites：
  - 同上十五套（含 USDX market / community-fee）
  - npm scripts：`e2e:prediction` 及分套件别名
  - `--dev` 说明：迁移未跑时由 `ensureUsdxProtocolAsset` 引导创建 `900_000`

## Remaining gates / 剩余门禁

- [x] Regenerate all imported `zrml-*` weights against the Nexus runtime
  (heavy pallets used reduced steps; production needs a 50/20 rerun).
- [ ] Review worst-case component ranges against runtime constants and block
  limits.
- [ ] Install `cargo-fuzz` and run timed upstream fuzz campaigns.
- [ ] Add collateral, bridge-collateral, router and Court sequence fuzzers.
- [x] Live batch-2 suites (hybrid-router / combinatorial / futarchy / USDX
  collateral + market + community-fee) pass on `--dev` after
  `ensureUsdxProtocolAsset` bootstrap; Full mode admits USDX `deposit` past
  CallFilter (economic failures still OK).
- [x] Combinatorial `redeem` after trusted `report` (skips admin resolve when
  already `Resolved`).
- [x] Hybrid-router orderbook fill path covered (maker ask + `buy` with `orders`).
- [x] Community-fee Path A/B: neo-swaps Standard buy/sell charge `ExternalFees`
  from the trader (`who`) so FeeAllowance binds the signer; D19 charge marker
  includes block number to avoid cross-block extrinsic-index collisions.
- [ ] Optional: futarchy schedule/execute after ≥600 blocks (`waitUntilBlock` helper
  ready; default suites only assert `Proposals` queue + `Submitted`).
- [ ] Full `e2e:prediction` (all 15 suites) on a fresh `--dev` when convenient.
- [x] Confirm by code search that no prediction `weights.rs` still carries
  upstream CLI 48.0.0-only measurements.

- [x] 全部已导入 `zrml-*` 权重已按 Nexus runtime 重新生成
  （重 pallet 使用缩减 steps；生产前需以 50/20 重跑）。
- [ ] 按 runtime 常量与区块限制审核最坏 component range。
- [ ] 安装 `cargo-fuzz` 并运行限时上游 fuzz campaign。
- [ ] 新增 collateral、bridge-collateral、router 与 Court sequence fuzz。
- [x] 第二批 E2E（hybrid-router / combinatorial / futarchy / USDX collateral +
  market + community-fee）在 `--dev` 上通过；缺迁移时用 `ensureUsdxProtocolAsset`
  引导资产；Full 下 USDX `deposit` 已过 CallFilter。
- [x] combinatorial 信任市场 `report` 后直接 `Resolved` 再 `redeem`；hybrid-router
  orderbook 成交路径已覆盖。
- [x] community-fee Path A/B：neo-swaps 标准买卖从交易者扣 `ExternalFees`；D19
  标记含块高，避免跨块 extrinsic index 误伤。
- [ ] 可选：futarchy ≥600 块后的 schedule/execute（`waitUntilBlock` 已就绪；默认套件
  仅断言 `Proposals` 队列与 `Submitted`）。
- [ ] 方便时在全新 `--dev` 上跑完整 15 套 `e2e:prediction`。
- [x] 代码搜索确认 prediction `weights.rs` 已无上游 CLI 48.0.0 独占实测残留。
