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

- Global: `set_paused(None, true)` · Per-lane: `set_paused(Some(Evm(56)), true)`.
- `deregister_chain(chain)` disables both inbound and outbound for that chain.

### 5. Monitoring & reconciliation / 监控与对账

- Off-chain, periodically assert: `BridgedOutByChain[Evm(n)]` (Nexus) ==
  `totalSupply` of the NEX contract on chain `n` (EVM), within in-flight slack.
- Alert on `try_state` failure, on `BridgeRefunded` spikes (timeout/liveness), and
  on any inbound rejected for invariant violation.

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
- EVM `NEX` contract deployment + **independent security audit**.
- Testnet Nexus↔BSC round-trip + off-chain reconciliation alerting.
