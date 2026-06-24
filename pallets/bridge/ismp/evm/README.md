# EVM side of the NEX asset bridge / NEX 资产桥的 EVM 侧

This folder documents how the **EVM counterpart** of `pallet-bridge-ismp` is
deployed and configured so that the cross-chain wire format byte-matches the
Substrate side.

本目录说明 `pallet-bridge-ismp` 的 **EVM 对端**如何部署与配置，以保证跨链 wire
格式与 Substrate 侧逐字节一致。

> Status: integration spec + reference contract. The on-chain Substrate code is
> implemented and tested; the EVM contract still needs an independent audit, the
> canonical state-machine / coprocessor ids (G-B3), and a tokenomics sign-off
> (G-A1-2) before mainnet. 状态：集成规范 + 参考合约。Substrate 侧已实现并测试；
> EVM 合约在主网前仍需独立审计、规范状态机/协处理器 id（G-B3）与 tokenomics
> 签字（G-A1-2）。

## Which contract to deploy / 部署哪个合约

**Prefer Polytope Labs' official, audited `HyperFungibleToken`** (burn-custody /
native mode). It already ABI-encodes the identical `Message` struct that
`pallet-bridge-ismp` vendored, so no custom Solidity (and no extra audit surface)
is required. Hand-rolling a token contract re-introduces exactly the audit
surface that decision **D3=(c)** removed on the Rust side.

**优先使用 Polytope Labs 官方已审计的 `HyperFungibleToken`**（burn 托管 / native
模式）。它 ABI 编码的 `Message` 结构与 `pallet-bridge-ismp` vendor 的完全一致，因此
无需自写 Solidity（也不增加审计面）。自写代币合约会重新引入 D3=(c) 在 Rust 侧已
消除的审计面。

`NexHyperFungibleToken.sol` in this folder is a **reference** implementation for
teams that need a standalone, self-custodied token. It is **not** compiled in CI
and is **not** a substitute for an audit.

本目录的 `NexHyperFungibleToken.sol` 是给需要独立自托管代币的团队的**参考**实现，
**不**纳入 CI，**不**替代审计。

## Canonical wire format / 规范 wire 格式

The POST body is `abi.encode(Message)` with the struct
(`pallets/bridge/ismp/src/types.rs` ↔ Solidity):

POST body 为 `abi.encode(Message)`，结构（`pallets/bridge/ismp/src/types.rs` ↔
Solidity）：

```solidity
struct Message {
    bytes   from;   // original sender (timeout refunds)
    bytes   to;     // recipient on the destination chain
    uint256 amount; // amount in the DESTINATION chain's ERC-20 precision
    bytes   data;   // optional calldata; IGNORED by the Stage-2 pallet
}
```

Precision: NEX is 12 decimals on Nexus, 18 on EVM. The pallet scales
18↔12 on each crossing (`convert_to_erc20` / `convert_to_balance`); EVM dust
below `10^(18-12)=10^6` is truncated, which is why `MinBridgeAmount` is set well
above that floor. 精度：NEX 在 Nexus 为 12 位、EVM 为 18 位。pallet 每次跨链做
18↔12 缩放；EVM 上低于 `10^(18-12)=10^6` 的 dust 会被截断，故 `MinBridgeAmount`
远高于该下限。

## Cross-order & withdraw payload (HB-ENT-01) / 跨链下单与提款负载

For HB-ENT-01 the `Message.data` field is **non-empty** and carries a **SCALE-encoded**
`InboundOp` (Substrate codec — *not* ABI). An empty `data` is still a plain asset
transfer (Stage 2). `NexusDigitalOrderGateway.sol` shows the exact byte layout.

HB-ENT-01 中 `Message.data` **非空**，携带 **SCALE 编码**的 `InboundOp`（Substrate codec，
**非** ABI）。空 `data` 仍为纯资产转账（Stage 2）。`NexusDigitalOrderGateway.sol` 给出精确
字节布局。

SCALE rules used here: integers are **little-endian**, fixed arrays (`[u8; 20]`) have
**no** length prefix, `Option<T>` is `0x00` (None) or `0x01 ++ T`, and an enum is a
1-byte variant index followed by the variant body. SCALE 规则：整数**小端**，定长数组
（`[u8; 20]`）**无**长度前缀，`Option<T>` 为 `0x00`（None）或 `0x01 ++ T`，枚举为 1 字节
变体索引后接变体体。

```text
InboundOp = 0x00 ++ OrderIntent      // Order
          | 0x01 ++ WithdrawRequest  // Withdraw

OrderIntent (76 or 96 bytes):
  schema_version : u8           (1)   // = 1
  buyer_evm      : [u8;20]      (20)
  product_id     : u64  LE      (8)
  quantity       : u32  LE      (4)
  amount_nex     : u128 LE      (16)  // = Message.amount in EVM precision
  max_nex_amount : u128 LE      (16)  // slippage cap (EVM precision)
  referrer       : Option<[u8;20]>    // 0x00 | 0x01 ++ 20 bytes
  nonce          : u64  LE      (8)

WithdrawRequest (65 bytes):
  schema_version : u8           (1)   // = 1
  owner_evm      : [u8;20]      (20)  // == msg.sender on the EVM gateway
  amount_nex     : u128 LE      (16)  // EVM precision; Message.amount = 0
  dest_recipient : [u8;20]      (20)
  nonce          : u64  LE      (8)
```

Order: `Message.amount` = the NEX burned on the EVM side; the pallet mints to the
derived buyer `blake2_256("nexus-evm" ++ buyer_evm)` and dispatches the digital order.
Withdraw: `Message.amount` = 0; the pallet debits `blake2_256("nexus-evm" ++ owner_evm)`
and POSTs the NEX back to `dest_recipient` (a plain transfer the NEX token contract mints).
下单：`Message.amount` = EVM 侧销毁的 NEX；pallet 向派生买家
`blake2_256("nexus-evm" ++ buyer_evm)` 铸造并派发数字下单。提款：`Message.amount` = 0；
pallet 从 `blake2_256("nexus-evm" ++ owner_evm)` 扣款并将 NEX POST 回 `dest_recipient`
（由 NEX 代币合约铸造的纯转账）。

> **Authorisation / 鉴权**：a withdraw moves a derived account's funds, so the **EVM
> gateway must enforce `msg.sender == owner_evm`** before dispatching. Nexus trusts the
> registered source contract (the same allow-list as every inbound message) and only ever
> debits the derived owner. 提款会动用派生账户资金，故 **EVM 网关须在派发前强制
> `msg.sender == owner_evm`**。Nexus 信任已注册来源合约（与所有入站消息同一 allow-list），
> 且只扣派生持有人本人。

## Peer identifiers (must match exactly) / 对端标识（必须完全一致）

| Field / 字段 | Value / 取值 | Source / 来源 |
| --- | --- | --- |
| Nexus state-machine id | `SUBSTRATE-NEXS` *(placeholder, G-B3)* | `runtime/src/configs/ismp.rs` `HostStateMachine` |
| Nexus module id (`to`/`from`) | 8 bytes `"nexbridg"` = `0x6e65786272696467` | `pallet_bridge_ismp::PALLET_ID` (`ModuleId::Pallet`) |
| EVM state-machine id | e.g. `EVM-56` (BSC) | `StateMachine.evm(56)` |
| EVM module id (`to`/`from`) | the 20-byte NEX contract address | deployment |

`ModuleId::Pallet(PalletId(*b"nexbridg")).to_bytes()` returns the **raw 8 bytes**
`nexbridg` (verified against `pallet-ismp 2512.2.0`), so the EVM peer's
`moduleId` is exactly those 8 bytes — not a 20/32-byte padded value.

## End-to-end configuration / 端到端配置

1. **Deploy** the chosen EVM contract, passing the ISMP `Host` address for that
   chain (BSC mainnet/testnet host address: confirm with Polytope Labs, G-B3).
2. **EVM → register Nexus as a peer**:
   - official `HyperFungibleToken`: `addChain(bytes("SUBSTRATE-NEXS"), bytes("nexbridg"))`
   - reference contract: `setPeer(bytes("SUBSTRATE-NEXS"), hex"6e65786272696467", true)`
3. **Nexus → register the EVM chain** (root / `BridgeOrigin`):
   `pallet_bridge_ismp::register_chain(StateMachine::Evm(56), <NEX EVM address>, 18)`
4. **Nexus → set limits** (root): `set_limits(per_tx, daily)` — until then the
   bridge is inert (limits default to 0).
5. **Init consensus state** on both sides so proofs verify (see the pallet
   `README.md` runbook → "Consensus state").

See `../README.md` for the full operational runbook (consensus-state init,
register/limits, monitoring). 完整运维手册（共识状态初始化、注册/限额、监控）见
`../README.md`。

## Dependencies (reference contract) / 依赖（参考合约）

- `@polytope-labs/ismp-solidity-abi` — `BaseIsmpModule`, `IDispatcher`, ISMP
  structs. Pin the version that matches the deployed `Host`. 与部署的 `Host`
  对应的版本须锁定。
- `@openzeppelin/contracts` — `ERC20`, `Ownable`.

Compile/test with Foundry or Hardhat in your EVM workspace; this Rust repo does
not build Solidity. 在 EVM 工作区用 Foundry/Hardhat 编译测试；本 Rust 仓库不构建
Solidity。
