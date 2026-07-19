# pallet-bridge-ismp — native NEX asset bridge / 原生 NEX 资产桥

Self-built ISMP asset bridge for the **native NEX** token (HB-ASSET-01, decision
**D3=(c)**). It vendors the audited core of Polytope Labs'
`pallet-hyper-fungible-token` (the `Message` ABI, the precision functions, and the
send-burn / `on_accept` / `on_timeout` logic) and adapts it to a **burn/mint**
model: NEX is *really burned* (`Currency::withdraw` → `TotalIssuance↓`) when it
leaves and *really minted* (`Currency::deposit_creating` → `TotalIssuance↑`) when
it comes back. No liquidity pool, no fork, no unpublished crates.

针对**原生 NEX** 的自建 ISMP 资产桥（HB-ASSET-01，决策 **D3=(c)**）。vendor 了
Polytope Labs `pallet-hyper-fungible-token` 已审计的核心（`Message` ABI、精度函数、
send-burn / `on_accept` / `on_timeout`），并改造为 **burn/mint** 模型：出站真销毁
（`TotalIssuance↓`）、回桥真铸造（`TotalIssuance↑`）。无资金池、无 fork、不依赖
未发布 crate。

Design docs: `docs/HYPERBRIDGE_INTEGRATION.md`,
`docs/HB_ASSET_01_NEX_HFT_DEV_SPEC.md`. EVM side: `./evm/README.md`.

## How it fits together / 接线关系

```
bridge_out ─burn→ ledger(BridgedOut/ByChain) ─dispatch→ pallet-hyperbridge ─fee→ pallet-ismp ─POST→ Hyperbridge ─→ EVM NEX contract (mint)
EVM NEX contract (burn) ─POST→ Hyperbridge ─proof→ pallet-ismp ─route(nexbridg)→ on_accept ─mint→ recipient   (ledger −)
```

- **Dispatcher** = `pallet-hyperbridge` (charges per-byte protocol fee, commits to
  `pallet-ismp`). **Consensus** = `ismp-grandpa`. **Routing**: `runtime/src/configs/ismp.rs`
  routes the module id `"nexbridg"` to this pallet.
- The bridge is **inert until governance configures it**: limits default to 0 and
  no chains are registered. 桥在治理配置前处于停用状态：限额默认为 0，且未注册任何链。

## Guardrails / 护栏

Outbound, **before** any burn: not globally/lane paused · `>= MinBridgeAmount` ·
`<= per_tx` · rolling-window `<= daily` · spend respects locks + ED (`KeepAlive`).
Inbound: source chain registered · `from == registered contract` · not paused ·
`amount <= BridgedOut` and `<= BridgedOutByChain[src]` (anti-inflation).

Recipient derivation: a 32-byte recipient is a native account used as-is; a 20-byte
recipient is an EVM address derived through the **same** `EvmToSubstrate` mapping
(`NexusEvmDerivation`, blake2) that cross-order / withdraw use, so plain transfers
and HB-ENT-01 resolve a given EVM identity to one account (no stranded funds).
收款人派生：32 字节为原生账户，原样使用；20 字节为 EVM 地址，用与跨链下单 / 提款
**相同**的 `EvmToSubstrate`（`NexusEvmDerivation`，blake2）映射派生，使纯转账与 HB-ENT-01
对同一 EVM 身份解析到同一账户（资金不被冻结）。

## Ledger invariant / 账本不变量

`Σ BridgedOutByChain == BridgedOut`, checked by `try_state` (enabled in the
runtime's `try-runtime` feature). Run with `try-runtime-cli` against live state to
verify no mint ever exceeds in-flight supply. 由 `try_state` 校验（已在 runtime 的
`try-runtime` feature 启用）。

## Operational runbook / 运维手册

All steps are root / `AdminOrigin` / `BridgeOrigin` (currently `EnsureRoot`).

### 1. Consensus state (one-time, per connected chain) / 共识状态（每条链一次）

So inbound proofs verify, register the GRANDPA consensus and the peer state
machines:

- `ismp_grandpa::add_state_machines(vec![AddStateMachine { state_machine, slot_duration }])`
- `pallet_ismp::create_consensus_client(CreateConsensusState { .. })` — seed the
  initial consensus state / commitments for the coprocessor.
- Confirm the canonical `HostStateMachine` / `Coprocessor` ids first (**G-B3**,
  see TODOs in `runtime/src/configs/ismp.rs`).

### 2. Register the EVM chain / 注册 EVM 链

```
pallet_bridge_ismp::register_chain(
    chain        = StateMachine::Evm(56),       // BSC
    contract     = 0x<NEX EVM contract>,        // ERC-6160 NEX address
    erc_decimals = 18,
)
```

Multiple EVM chains can be registered independently (per-chain ledger, per-lane
pause). For example, Polygon (chain id 137) is added the same way:

可同时注册多条 EVM 链（按链独立账本、按 lane 独立暂停）。例如 Polygon（chain id
137）以同样方式接入：

```
pallet_bridge_ismp::register_chain(
    chain        = StateMachine::Evm(137),      // Polygon mainnet
    contract     = 0x<NEX Polygon contract>,    // ERC-6160 NEX address on Polygon
    erc_decimals = 18,
)
```

`erc_decimals` must be `>= NativeDecimals (12)`. On the EVM side register Nexus as
a peer with state-machine id `SUBSTRATE-NEXS` and module id `"nexbridg"`
(`0x6e65786272696467`) — see `./evm/README.md`.

### 3. Set limits / 设置限额

```
pallet_bridge_ismp::set_limits(per_tx, daily)
```

Start conservative on testnet; raise after monitoring. Until set, all
`bridge_out` calls fail with `PerTxLimitExceeded`/`DailyLimitExceeded`.

### 4. Pause controls / 暂停控制

- Global: `set_paused(None, true)` · Per-lane: `set_paused(Some(Evm(56)), true)` or
  `set_paused(Some(Evm(137)), true)` (BSC / Polygon respectively).
- `deregister_chain(chain)` disables both inbound and outbound for that chain
  (e.g. `deregister_chain(StateMachine::Evm(137))` halts the Polygon lane while
  BSC keeps running — see `multi_lane_independent_ledgers` /
  `polygon_pause_does_not_affect_bsc` tests).

### 5. Monitoring & reconciliation / 监控与对账

- Off-chain, periodically assert: `BridgedOutByChain[Evm(n)]` (Nexus) ==
  `totalSupply` of the NEX contract on chain `n` (EVM), within in-flight slack.
  Run this **per connected lane** — e.g. both `Evm(56)` (BSC) and `Evm(137)`
  (Polygon) — since the per-chain ledgers are independent.
  按每条已接入 lane 分别对账——例如 `Evm(56)`（BSC）与 `Evm(137)`（Polygon）各自
  独立校验，因为按链账本相互独立。
- Alert on `try_state` failure, on `BridgeRefunded` spikes (timeout/liveness), and
  on any inbound rejected for invariant violation.
- Tracked-payout (HB-WD-01 mechanism 2) refund contexts are reaped by the
  permissionless `prune_payout_refunds(limit)` once older than `PayoutRefundTtl`
  (successfully delivered payouts never time out, so their entries would otherwise
  linger). Run a keeper that calls it periodically to bound `PayoutRefunds` growth.
  已跟踪派发（HB-WD-01 机制 2）的退款上下文在超过 `PayoutRefundTtl` 后由无许可的
  `prune_payout_refunds(limit)` 回收（成功投递的派发永不超时，否则条目会一直残留）。
  建议运行 keeper 周期性调用以限制 `PayoutRefunds` 增长。

## Cross-chain digital ordering (HB-ENT-01) / 跨链数字下单

On top of the asset bridge, an inbound message whose `Message.data` is **non-empty**
carries a SCALE `InboundOp` (`src/types.rs`) instead of a plain transfer:

资产桥之上，`Message.data` **非空**的入站消息携带 SCALE `InboundOp`（`src/types.rs`），
而非纯转账：

- **`InboundOp::Order(OrderIntent)`** — mints the bridged NEX to the derived buyer
  `blake2_256("nexus-evm" ++ buyer_evm)` (within the in-flight ledger), then dispatches
  `pallet-entity-order::do_cross_order` (Digital + Public only) in a nested storage
  layer. On order failure the mint is **kept** as DerivedCredit and `on_accept` still
  returns `Ok` (receipt persisted, no replay): "never burned without settlement".
  向派生买家铸造已桥接 NEX，再在嵌套存储层内派发 `do_cross_order`（仅 Digital + Public）；
  下单失败则保留铸造额为 DerivedCredit，且 `on_accept` 仍返回 `Ok`（持久化回执、不重放）。
- **`InboundOp::Withdraw(WithdrawRequest)`** — debits the derived owner
  `blake2_256("nexus-evm" ++ owner_evm)` and reuses the outbound core (`do_outbound`)
  to POST the NEX back to the EVM `dest_recipient`. Authorisation is on the EVM gateway
  (`msg.sender == owner_evm`) + the inbound source-contract allow-list.
  从派生持有人扣款并复用出站核心将 NEX POST 回 EVM `dest_recipient`；鉴权在 EVM 网关
  （`msg.sender == owner_evm`）+ 入站来源合约 allow-list。

Wiring: `EvmToSubstrate = NexusEvmDerivation` (blake2) and `CrossOrderHandler =
NexusCrossOrderHandler` (→ `pallet-entity-order`) in `runtime/src/configs/ismp.rs`.
The exact `data` byte layout and the EVM `NexusDigitalOrderGateway` reference contract
are in `./evm/README.md` and `./evm/NexusDigitalOrderGateway.sol`.

> Sybil (G-ENT-1): the referrer is honoured if supplied, but the first phase
> recommends `referrer = None` on the EVM side; level/commission rules are unchanged
> (driven by real spend), so derived accounts gain nothing without paying. Sybil
> 风控（G-ENT-1）：传入则采用 referrer，但首期建议 EVM 侧置空；等级/佣金规则不变。

## Weights / 权重

`weights.rs` ships conservative DB-weight-based estimates in
`SubstrateWeight<T>` (wired in the runtime). Regenerate real weights before
mainnet:

```bash
cargo run --release --features runtime-benchmarks -- benchmark pallet \
  --chain dev --pallet pallet_bridge_ismp --extrinsic '*' \
  --steps 50 --repeat 20 --output pallets/bridge/ismp/src/weights.rs
```

## Tests / 测试

```bash
cargo test -p pallet-bridge-ismp                          # unit tests
cargo test -p pallet-bridge-ismp --features runtime-benchmarks  # + benchmark bodies
```

## Pending before mainnet / 主网前未决项

- **G-B3** canonical state-machine / coprocessor / EVM `Host` addresses.
- **G-A1-2** tokenomics sign-off for `TotalIssuance` movement.
- EVM `NEX` contract deployment + **independent security audit** (now also covering
  the HB-ENT-01 order/withdraw `data` path + EVM gateway).
- Testnet Nexus↔BSC round-trip + off-chain reconciliation alerting.
- **G-ENT-1/2** Sybil + pricing sign-off for cross-chain ordering.
