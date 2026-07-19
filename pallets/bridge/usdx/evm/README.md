# Polygon Amoy USDX HFT

This workspace deploys the pinned official `WrappedHyperFungibleToken` for the
Phase-1 Polygon Amoy lane and binds it to the non-upgradeable
`HftGovernanceController`.

本工作区为 Phase 1 Polygon Amoy 通道部署锁定版本的官方
`WrappedHyperFungibleToken`，并将其绑定到不可升级的
`HftGovernanceController`。

## Pinned dependencies / 锁定依赖

- Hyperbridge commit:
  `3979482228d9001f0463f3192524fa41bc76989b`
- OpenZeppelin Contracts: `v5.4.0`
- Polytope Solidity Merkle Trees `1.0.0` commit:
  `12f352fb9b0b311bff26df6a6571329d39ad59be`
- forge-std: `v1.16.1`
- Circle Polygon Amoy test USDC:
  `0x41E94Eb019C0762f9Bfcf9Fb1E58725BfB0e7582`

Re-verify the USDC, ISMP Host, and CallDispatcher addresses with Circle and
Polytope Labs immediately before deployment.

部署前必须再次向 Circle 与 Polytope Labs 核验 USDC、ISMP Host 和
CallDispatcher 地址。

## Build and test / 构建与测试

```bash
./script/bootstrap.sh
forge build
forge test -vv
```

## Deploy / 部署

```bash
export DEPLOYER_PRIVATE_KEY=...
export TIMELOCK=0x...
export PAUSE_GUARDIAN=0x...
export AMOY_ISMP_HOST=0x...
export AMOY_CALL_DISPATCHER=0x...

forge script script/DeployWrappedHftPolygon.s.sol:DeployWrappedHftPolygon \
  --rpc-url "$AMOY_RPC_URL" --broadcast
```

The deployment transaction fixes the controller as HFT owner from construction
and permanently binds `host`, `dispatcher`, Amoy USDC, and `isWeth=false`.
The deployment key is the one-time configurator and receives no continuing
controller authority after binding.

部署交易从构造时即把 controller 设为 HFT owner，并永久绑定 `host`、
`dispatcher`、Amoy USDC 与 `isWeth=false`。部署密钥仅作为一次性
configurator，绑定完成后不再拥有 controller 持续权限。

Install the Nexus peer only after Polytope Labs confirms the canonical Nexus
state-machine bytes:

仅在 Polytope Labs 确认 Nexus 规范状态机字节后安装 peer：

```bash
export HFT_CONTROLLER=0x...
export NEXUS_STATE_MACHINE=0x...

forge script script/BuildNexusPeerCalldata.s.sol:BuildNexusPeerCalldata
```

Schedule the printed target/calldata through the configured timelock; never
replace the timelock with a deployment EOA for convenience. The peer module ID
is fixed to the raw eight bytes `pall_hft`.

必须通过已配置 timelock 调度输出的 target/calldata；不得为方便而把 timelock 替换为
部署 EOA。Peer module ID 固定为原始八字节 `pall_hft`。

## Release gate / 发布门禁

The pinned official contract applies `whenNotPaused` to
`onPostRequestTimeout`. Consequently, pausing the HFT also blocks timeout
refund execution. This is a known Phase-1 fault-drill and mainnet release
blocker: do not activate a non-zero lane until an upstream fix or independently
audited compatible patch preserves refunds while paused.

锁定的官方合约对 `onPostRequestTimeout` 使用了 `whenNotPaused`，因此暂停 HFT
也会阻止 timeout refund。该行为是 Phase 1 故障演练与主网发布阻塞项：在上游修复或
经独立审计的兼容补丁保证暂停期间仍可退款前，不得启用非零额度通道。
