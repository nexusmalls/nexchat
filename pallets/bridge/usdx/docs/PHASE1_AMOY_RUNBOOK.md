# Phase 1 Polygon Amoy runbook

This runbook activates one USDX collateral lane on Polygon Amoy. It does not
authorize mainnet deployment.

本手册用于在 Polygon Amoy 激活单条 USDX 抵押通道，不授权主网部署。

## 1. Hard gates / 硬门禁

Do not submit activation governance until all values below are independently
verified:

在以下参数全部独立核验前，不得提交激活治理：

- canonical Nexus `StateMachine` bytes confirmed by Polytope Labs;
- testnet Coprocessor and bidirectional consensus paths confirmed;
- Polygon Amoy ISMP Host and CallDispatcher addresses confirmed;
- Circle Amoy test USDC address reconfirmed;
- messaging/consensus relayers running in both directions;
- protocol asset migration executed and `try-runtime` checks passed;
- official Wrapped HFT pause/timeout-refund blocker resolved or an audited
  compatible patch deployed.

The last item is currently open: pinned commit
`3979482228d9001f0463f3192524fa41bc76989b` applies `whenNotPaused` to
`onPostRequestTimeout`.

最后一项当前仍未关闭：锁定 commit
`3979482228d9001f0463f3192524fa41bc76989b` 对
`onPostRequestTimeout` 使用了 `whenNotPaused`。

## 2. Build and verify / 构建与验证

```bash
cargo test -p pallet-usdx
cargo test -p pallet-hyper-fungible-token
cargo test -p nexus-runtime
RUSTFLAGS="--cfg substrate_runtime" cargo check --release -p nexus-runtime \
  --no-default-features --target wasm32-unknown-unknown --locked

cd pallets/bridge/usdx/evm
./script/bootstrap.sh
forge build
forge test -vv
```

Runtime spec 105 is the deterministic Phase-1 Amoy artifact. It enables only the
reviewed HFT calls and strict asset inspection; empty registry and zero limits
still prevent economic activation.

Runtime spec 105 是确定性的 Phase 1 Amoy 制品，仅开放经审查的 HFT 调用与严格资产检查；
空 registry 与零限额仍会阻止经济激活。

## 3. EVM deployment / EVM 部署

1. Deploy `HftGovernanceController`.
2. Deploy the official `WrappedHyperFungibleToken` with the controller as
   `initialOwner`.
3. Bind and configure it once with Amoy Host, CallDispatcher and test USDC.
4. Record controller/HFT addresses, bytecode hashes, deployment block and block
   hash.
5. Through the timelock, install the confirmed Nexus state machine with raw
   module ID `pall_hft`.

Use `evm/script/DeployWrappedHftPolygon.s.sol` and
`evm/script/BuildNexusPeerCalldata.s.sol`; schedule the generated peer call
through the configured timelock.

## 4. Runtime upgrade and governance / Runtime 升级与治理

Upgrade to the reviewed activation Wasm, then execute in this order:

升级到经审查的激活 Wasm 后，按以下顺序执行：

1. Verify assets `900000..=900002` and their sovereign roles.
2. `HyperFungibleToken::register_token`:
   - local asset: `900001`
   - custody mode: imported/non-native
   - source: `StateMachine::Evm(80002)`
   - contract: deployed Amoy Wrapped HFT
   - decimals: `6`
3. `Usdx::register_collateral(900001, ...)` with complete
   `LaneActivationEvidence`.
4. Set policy and non-zero window length, but keep all amount/debt limits zero.
5. Initialize and verify both consensus directions.
6. Raise per-transaction, window and global debt ceilings to canary values.
7. Enable only receipt asset `900001`.

At every step, abort if the canonical HFT descriptor or protocol-asset roles
differ from the reviewed values.

任一步发现 HFT descriptor 或协议资产角色与审查值不一致时，立即终止。

## 5. Acceptance / 验收

Run repeated end-to-end cases:

- USDC lock → xUSDC mint → USDX mint;
- USDX redeem → xUSDC burn → USDC release;
- insufficient relayer fee;
- duplicate/replayed delivery;
- destination failure followed by timeout refund;
- relayer/indexer restart recovery;
- pause and recovery drill;
- configuration-drift alert.

The lane passes only when every completed transfer is reconciled independently:

```text
Wrapped HFT USDC balance
= live xUSDC issuance
+ pending deposits
+ pending withdrawals
+ bounded reconciliation slack
```

Also continuously assert:

```text
USDX issuance == total USDX debt == Polygon lane debt
Polygon lane debt <= PSM xUSDC balance
```

## 6. Emergency stop / 紧急停止

Pause the lane immediately on any proof failure, unexpected HFT/controller
configuration change, asset-role drift, reconciliation mismatch, or Circle
freeze event. Keep debt ceilings at zero until the discrepancy is explained
and a full snapshot is retained.

出现 proof 失败、HFT/controller 配置异常、资产角色漂移、对账不一致或 Circle 冻结事件时，
立即暂停通道。在原因查明并保留完整快照前，债务上限保持为零。
