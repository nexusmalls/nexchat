# Phase 5 Futarchy and Styx Development

Phase 5 is closed at the isolated workspace boundary. Production runtime
indices and wiring remain Phase 6 work.

Phase 5 已在隔离 workspace 边界收口；生产 runtime index 与接线仍属于 Phase 6。

## Batch 1: governance, oracle, and native burn boundaries

### Futarchy

- `SubmitOrigin` is runtime-configurable instead of hard-coded to Root.
- The isolated mock accepts Root or a Technical Committee stand-in and rejects
  unrelated signed accounts.
- Proposal approval, rejection, cache bounds, short duration, and Scheduler
  failure paths are covered.
- NeoSwaps decision-market tests cover positive and negative outcome prices,
  absolute/relative thresholds, observation start, and victory margin.
- Futarchy continues to use FRAME Scheduler v3 `ScheduleAnon`, matching the
  existing Nexus Scheduler boundary without adding Democracy.
- The NeoSwaps integrated mock includes real `pallet_preimage` and
  `pallet_scheduler` implementations.
- A positive decision market schedules and executes a Root
  `PredictionControl::set_prediction_mode` call through the real Scheduler.
- A negative decision market emits `Rejected` before scheduling and leaves the
  Scheduler agenda empty. This is the upstream pallet's cancellation semantic;
  Futarchy exposes no explicit cancel extrinsic.
- The Futarchy suite passes 9 tests; the complete NeoSwaps suite passes 445
  tests.

### Futarchy / 未来政治

- `SubmitOrigin` 已由硬编码 Root 改为 runtime 可配置。
- 隔离 mock 接受 Root 或 Technical Committee 替身，并拒绝无关签名账户。
- 已覆盖提案批准、拒绝、缓存上限、持续时间过短和 Scheduler failure。
- NeoSwaps decision-market 测试覆盖正/负结果价格、绝对/相对阈值、观测起点及
  victory margin。
- Futarchy 继续使用 FRAME Scheduler v3 `ScheduleAnon`，与 Nexus 现有 Scheduler
  边界一致，不新增 Democracy。
- NeoSwaps 集成 mock 已接入真实 `pallet_preimage` 与 `pallet_scheduler`。
- 正向 decision market 会通过真实 Scheduler 调度并执行 Root
  `PredictionControl::set_prediction_mode` 调用。
- 负向 decision market 在调度前产生 `Rejected`，Scheduler agenda 保持为空；
  这就是上游 pallet 的取消语义，Futarchy 不提供显式 cancel extrinsic。
- Futarchy 共 9 项测试通过；NeoSwaps 完整测试共 445 项通过。

### Styx

- The burn currency is generic over the configured native `Currency`; the
  Nexus mock wires it directly to `pallet-balances`.
- `DefaultBurnAmount` is runtime-configurable and the mock uses NEX 12-decimal
  precision: `200 * 10^12` planck, displayed as `200 NEX`.
- A successful crossing reduces both the account balance and total NEX
  issuance by exactly the configured amount, then writes the registry entry.
- A failed crossing does not write the registry; a second crossing is rejected.
- `SetBurnAmountOrigin` accepts Root or a Treasury Council stand-in.
- The Styx suite passes 10 tests.

### Styx / 冥河

- 销毁货币通过 runtime 配置的原生 `Currency` 提供；Nexus mock 直接接入
  `pallet-balances`。
- `DefaultBurnAmount` 可由 runtime 配置；mock 使用 NEX 12 位精度：
  `200 * 10^12` planck，即 `200 NEX`。
- 成功跨越会精确扣减账户余额与 NEX 总发行量，然后写入 registry。
- 失败跨越不写 registry，重复跨越会被拒绝。
- `SetBurnAmountOrigin` 接受 Root 或 Treasury Council 替身。
- Styx 共 10 项测试通过。

## Verification commands / 验证命令

```bash
cargo test -p zrml-futarchy --features mock
cargo test -p zrml-styx
cargo test -p zrml-neo-swaps --features mock --lib
cargo check -p zrml-futarchy --features runtime-benchmarks
cargo check -p zrml-styx --features runtime-benchmarks
RUSTFLAGS="--cfg substrate_runtime" cargo check \
  -p zrml-futarchy -p zrml-styx -p zrml-neo-swaps \
  --no-default-features --target wasm32-unknown-unknown
```

## Styx economic parameter record / Styx 经济参数记录

- Raw amount / 原始单位：`200_000_000_000_000` planck.
- Display amount / 显示值：`200 NEX`.
- Worst user loss / 用户最坏损失：每个账户最多一次 `200 NEX`.
- Spam cost / spam 成本：每个新账户至少销毁 `200 NEX`.
- Adjustment origin / 调整 origin：Root 或 Treasury Council 2/3（Phase 6
  runtime mapping）。
- Storage-adjustable / storage 可调：是，`BurnAmount` 通过
  `set_burn_amount` 更新。

## Formal closure record / 正式收口记录

Phase 5 closed on 2026-07-12 at the isolated workspace boundary:

- All 464 tests pass: 9 Futarchy, 10 Styx, and 445 NeoSwaps.
- Positive/negative decision thresholds and victory margins are covered.
- Proposal scheduling, rejection-before-scheduling, Scheduler failure, and
  real Scheduler execution are covered.
- Styx uses NEX `Balances`, burns total issuance, enforces one crossing per
  account, and preserves the registry on failure.
- Runtime-benchmark and runtime-valid WASM no-std builds pass.
- Concrete Technical Committee and Treasury Council 2/3 aliases are deferred
  only to Phase 6 production runtime wiring; the pallet origins and isolated
  stand-ins are already configurable and tested.

Phase 5 于 2026-07-12 在隔离 workspace 边界正式收口。Futarchy 9 项、Styx
10 项、NeoSwaps 445 项，共 464 项测试全部通过；正/负决策阈值、victory margin、
调度、调度前拒绝、Scheduler failure、真实 Scheduler 执行及 Styx NEX 销毁与
registry 行为均已覆盖。runtime-benchmark 与 runtime-valid WASM no-std 编译通过。
真实 Technical Committee 与 Treasury Council 2/3 alias 仅留待 Phase 6 生产
runtime 接线；pallet origin 与隔离替身已可配置并通过测试。
