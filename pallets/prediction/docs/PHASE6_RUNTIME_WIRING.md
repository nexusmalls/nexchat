# Phase 6 Runtime Wiring Closure / Phase 6 正式 Runtime 接线收口

## Scope / 范围

Phase 6 integrates the complete prediction subsystem into Nexus while keeping it
economically inert. It does not authorize `ResolutionOnly`, `Trading`, or `Full`.

Phase 6 将完整预测子系统接入 Nexus，但保持经济惰性；本阶段不授权进入
`ResolutionOnly`、`Trading` 或 `Full`。

## Integrated surface / 已接入范围

- Runtime indices 176–192 are assigned to the fixed aliases in the development
  specification. A runtime unit test pins all pre-existing indices 0–175.
- `runtime/src/configs/prediction.rs` wires ORML currencies/tokens, all 13
  upstream pallets, the two Nexus adapters, committee origins, independent
  sovereign accounts, conservative integration weights, and BABE Court
  randomness.
- `NexusBaseCallFilter` applies the audited call registry, leaves control
  governance self-recoverable, preserves unwind paths in `Disabled`, enforces
  cross-module dependencies, and rejects foreign-collateral market creation in
  `Trading`.
- The upgrade marker rejects a first deployment unless mode is `Disabled`, all
  modules are disabled, and the collateral whitelist is empty.
- The live collateral validator requires a live `pallet-assets` asset. USDX also
  requires the canonical protocol asset, an unpaused PSM, a non-zero global debt
  ceiling, and issuance equal to PSM debt. Safe defaults therefore reject USDX.
- Runtime APIs include upstream `SwapsApi`, upstream `PredictionMarketsApi`, and
  Nexus `PredictionViewApi`. The view API uses keyed, bounded reads only.
- Node RPC includes upstream `swaps_*` methods and Nexus `prediction_*` methods.
- Runtime benchmark registration includes every prediction pallet that currently
  implements FRAME benchmarking. `control`, `collateral`, and `orml-currencies`
  still require Phase 7 benchmark implementations.

- Runtime 索引 176–192 已按开发规范固定分配；runtime 单测锁定既有 0–175 索引。
- `runtime/src/configs/prediction.rs` 已接入 ORML currencies/tokens、13 个上游
  pallet、两个 Nexus adapter、委员会 origin、独立主权账户、保守集成权重和 BABE
  Court 随机源。
- `NexusBaseCallFilter` 已应用审计后的调用注册表，保留 control 治理自恢复与
  `Disabled` 下的退出路径，执行跨模块依赖，并在 `Trading` 下拒绝外部抵押市场创建。
- 首次部署迁移仅在模式为 `Disabled`、全部模块关闭且抵押白名单为空时通过。
- 实时抵押验证要求 `pallet-assets` 资产处于 Live。USDX 还必须满足标准协议资产、
  PSM 未暂停、全局债务上限非零以及发行量等于 PSM 债务；安全默认值因此拒绝 USDX。
- Runtime API 已接入上游 `SwapsApi`、上游 `PredictionMarketsApi` 与 Nexus
  `PredictionViewApi`；视图 API 仅执行按 key 的有界读取。
- Node RPC 已接入上游 `swaps_*` 与 Nexus `prediction_*` 方法。
- 已注册当前具备 FRAME benchmark 实现的全部 prediction pallet；`control`、
  `collateral` 与 `orml-currencies` 的 benchmark 实现留待 Phase 7。

## Safety defaults / 安全默认值

The subsystem ships with:

```text
PredictionMode = Disabled
ModuleEnabled[*] = false
CollateralWhitelist = empty
```

Imported and Phase 2 weights are integration-only. No business module may be
enabled until Phase 7 generates and reviews Nexus-specific production weights.

导入权重与 Phase 2 权重仅用于集成验证。在 Phase 7 生成并审核 Nexus 专用生产权重前，
不得启用任何业务模块。

## Verification gates / 验收门禁

Locally verified during closure:

```bash
SKIP_WASM_BUILD=1 cargo check -p nexus-node
SKIP_WASM_BUILD=1 cargo check -p nexus-runtime --features runtime-benchmarks
SKIP_WASM_BUILD=1 cargo test -p nexus-runtime --lib prediction
SKIP_WASM_BUILD=1 cargo check -p nexus-runtime --features try-runtime
RUSTFLAGS="--cfg substrate_runtime" cargo check -p nexus-runtime \
  --no-default-features --target wasm32-unknown-unknown
cargo build --release -p nexus-runtime
```

Fresh Phase 6 release artifacts:

| Artifact | Bytes | Delta from Phase 0 |
|---|---:|---:|
| `nexus_runtime.wasm` | 23,786,141 | +3,197,833 (+15.53%) |
| `nexus_runtime.compact.wasm` | 22,607,546 | +3,094,554 (+15.86%) |
| `nexus_runtime.compact.compressed.wasm` | 3,075,172 | +384,976 (+14.31%) |

Compressed WASM SHA-256:

```text
9aec00f9e849855a3daac3310455df01446231b8c0cf620e93d1bb836df91648
```

The build completed successfully. The builder warned that it could not infer the
workspace lockfile from the isolated target directory; CI should set
`WASM_BUILD_WORKSPACE_HINT` to the repository root for reproducible rebuilds.

Release-candidate gates that must still be recorded against the target chain
state before Phase 6 is declared deployment-ready:

```bash
cargo test
cargo test -p nexus-runtime
# Run try-runtime against the current Nexus state snapshot.
# Generate and review metadata diff, proving indices 0–175 and old APIs unchanged.
```

在宣布 Phase 6 可部署前，仍必须针对目标链状态记录 release build、全量测试、
try-runtime 与 metadata diff 结果；metadata 审核必须证明索引 0–175 和旧 Runtime API
未变化。
