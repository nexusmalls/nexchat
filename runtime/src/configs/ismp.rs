//! ISMP / Hyperbridge protocol-layer runtime configuration.
//! ISMP / Hyperbridge 协议层运行时配置。
//!
//! Stage 1a wired the ISMP core engine `pallet-ismp` (request/response, host,
//! dispatcher, consensus-state store) plus its runtime API. Stage 1b adds the
//! vendored `pallet-hyperbridge` (host-param / fee module that receives governance
//! updates from the Hyperbridge coprocessor) — vendored under D3=(c) because the
//! published crate does not compile against the only available `ismp 2512.1.0` (the
//! matching `ismp 2512.0.0` was yanked). Stage 1b-2 wires `ismp-grandpa` (the
//! GRANDPA consensus client used to verify Hyperbridge proofs). Stage 2 wires
//! `pallet-bridge-ismp` (the self-built native-NEX asset bridge, vendoring the HFT
//! core per D3=(c)). See `docs/HYPERBRIDGE_INTEGRATION.md` §13 for the supply-chain
//! rationale and `docs/HB_ASSET_01_NEX_HFT_DEV_SPEC.md` for the bridge design.
//!
//! Stage 1a 已接入 ISMP 核心引擎 `pallet-ismp`（请求/响应、host、dispatcher、共识状态
//! 存储）及其 runtime API。Stage 1b 加入 vendor 的 `pallet-hyperbridge`（host-param / 费用
//! 模块，接收 Hyperbridge 协处理器的治理更新）——按 D3=(c) vendor，因为已发布 crate 无法对
//! 唯一可用的 `ismp 2512.1.0` 编译（对应的 `ismp 2512.0.0` 已被 yank）。仍暂缓一个组件：
//! Stage 1b-2 接入 `ismp-grandpa`（用于验证 Hyperbridge 证明的 GRANDPA 共识客户端）。
//! Stage 2 接入 `pallet-bridge-ismp`（自建原生 NEX 资产桥，按 D3=(c) vendor HFT 核心）。
//! 供应链原因见 `docs/HYPERBRIDGE_INTEGRATION.md` §13，桥设计见
//! `docs/HB_ASSET_01_NEX_HFT_DEV_SPEC.md`。

use alloc::{boxed::Box, vec::Vec};

use frame_support::parameter_types;
use frame_system::EnsureRoot;
use ismp::{host::StateMachine, module::IsmpModule, router::IsmpRouter};

use crate::{
	AccountId, Balance, Balances, BlockNumber, RuntimeEvent, Runtime, Timestamp, DAYS, MILLI_NEX,
};

parameter_types! {
    /// The state machine identifier for this host chain. Remote chains and the
    /// Hyperbridge coprocessor use this id to address requests to Nexus.
    /// 本链的状态机标识。远端链与 Hyperbridge 协处理器用此 id 向 Nexus 寻址请求。
    ///
    /// TODO(G-B3): confirm the canonical 4-byte consensus-state id with Polytope
    /// Labs before mainnet; `*b"NEXS"` is a development placeholder.
    /// TODO(G-B3)：主网前需与 Polytope Labs 确认规范的 4 字节共识状态 id；
    /// `*b"NEXS"` 为开发期占位。
    pub const HostStateMachine: StateMachine = StateMachine::Substrate(*b"NEXS");

    /// The coprocessor that proxies/aggregates proofs on our behalf. This points
    /// at the Hyperbridge state machine (mainnet "Nexus" parachain on Polkadot).
    /// 代表本链代理/聚合证明的协处理器，指向 Hyperbridge 状态机
    /// （Polkadot 上的 Hyperbridge 主网 "Nexus" 平行链）。
    ///
    /// TODO(G-B3): confirm the live coprocessor `StateMachine` (mainnet vs Paseo
    /// testnet "Gargantua") with Polytope Labs before mainnet; `Polkadot(3367)`
    /// is the documented Hyperbridge mainnet placeholder.
    /// TODO(G-B3)：主网前需与 Polytope Labs 确认实际协处理器 `StateMachine`
    /// （主网 vs Paseo 测试网 "Gargantua"）；`Polkadot(3367)` 为文档化的占位。
    pub const Coprocessor: Option<StateMachine> = Some(StateMachine::Polkadot(3367));
}

/// ISMP request/response router.
/// ISMP 请求/响应路由。
///
/// Stage 1b registers the vendored `pallet-hyperbridge` fee/host-param module under
/// its [`PALLET_HYPERBRIDGE_ID`], so the Hyperbridge coprocessor can push host-param
/// updates and relayer-fee withdrawals to this chain. Asset / cross-order modules
/// arrive in later stages. Unknown ids are rejected.
/// Stage 1b 注册 vendor 的 `pallet-hyperbridge` 费用/host-param 模块（按其
/// [`PALLET_HYPERBRIDGE_ID`]），以便 Hyperbridge 协处理器向本链推送 host-param 更新与
/// relayer 费用提取。资产 / 跨链下单模块在后续阶段加入；未知 id 一律拒绝。
#[derive(Default)]
pub struct Router;

impl IsmpRouter for Router {
    fn module_for_id(&self, id: Vec<u8>) -> Result<Box<dyn IsmpModule>, anyhow::Error> {
        if id.as_slice() == pallet_hyperbridge::PALLET_HYPERBRIDGE_ID {
            Ok(Box::new(pallet_hyperbridge::Pallet::<Runtime>::default()))
        } else if id == pallet_bridge_ismp::module_id_bytes() {
            // Stage 2: route the NEX asset bridge's well-known module id to its pallet.
            // Stage 2：将 NEX 资产桥的 well-known 模块 id 路由到其 pallet。
            Ok(Box::new(pallet_bridge_ismp::Pallet::<Runtime>::default()))
        } else {
            Err(anyhow::anyhow!("No ISMP module registered for id {:?}", id))
        }
    }
}

impl pallet_ismp::Config for Runtime {
    type AdminOrigin = EnsureRoot<AccountId>;
    type TimestampProvider = Timestamp;
    type Balance = Balance;
    type Currency = Balances;
    type HostStateMachine = HostStateMachine;
    type Coprocessor = Coprocessor;
    type Router = Router;
    // GRANDPA consensus client (Stage 1b-2): verifies finality proofs from the
    // Hyperbridge coprocessor / connected Substrate chains, enabling inbound message
    // verification. Uses the published `ismp-grandpa 2512.1.0`.
    // GRANDPA 共识客户端（Stage 1b-2）：验证来自 Hyperbridge 协处理器 / 所连 Substrate 链的
    // 终局性证明，从而支持入站消息验证。使用已发布的 `ismp-grandpa 2512.1.0`。
    type ConsensusClients = (ismp_grandpa::consensus::GrandpaConsensusClient<Runtime>,);
    type FeeHandler = ();
    type OffchainDB = ();
    type MigrationWeightInfo = ();
}

/// `pallet-hyperbridge` (vendored, D3=(c)): the fee/host-param module. It reuses
/// `pallet_ismp::Pallet<Runtime>` as its [`IsmpHost`], which implements
/// `IsmpDispatcher + Default`, to perform the actual outbound dispatch after charging
/// the per-byte protocol fee.
/// `pallet-hyperbridge`（已 vendor，D3=(c)）：费用/host-param 模块。它复用
/// `pallet_ismp::Pallet<Runtime>` 作为 [`IsmpHost`]（实现 `IsmpDispatcher + Default`），
/// 在收取按字节计的协议费用后执行实际出站派发。
impl pallet_hyperbridge::Config for Runtime {
    type IsmpHost = pallet_ismp::Pallet<Runtime>;
}

/// `ismp-grandpa` (Stage 1b-2): the GRANDPA consensus client pallet. It maintains the
/// whitelist of supported state machines (`AdminOrigin`-gated) and, via
/// [`GrandpaConsensusClient`](ismp_grandpa::consensus::GrandpaConsensusClient),
/// verifies GRANDPA finality proofs. It reuses `pallet_ismp::Pallet<Runtime>` as its
/// [`IsmpHost`], and gates its privileged calls behind root.
/// `ismp-grandpa`（Stage 1b-2）：GRANDPA 共识客户端 pallet。维护受支持状态机白名单
///（由 `AdminOrigin` 把关），并通过
/// [`GrandpaConsensusClient`](ismp_grandpa::consensus::GrandpaConsensusClient)
/// 验证 GRANDPA 终局性证明。复用 `pallet_ismp::Pallet<Runtime>` 作为 [`IsmpHost`]，
/// 特权调用由 root 把关。
impl ismp_grandpa::Config for Runtime {
    type IsmpHost = pallet_ismp::Pallet<Runtime>;
    type WeightInfo = ();
    type RootOrigin = EnsureRoot<AccountId>;
}

parameter_types! {
    /// Native NEX decimals (NEX = 10^12). Used for ERC↔local precision conversion.
    /// 原生 NEX 精度（NEX = 10^12）。用于 ERC↔本地精度换算。
    pub const BridgeNativeDecimals: u8 = 12;

    /// Minimum bridge amount: `MILLI_NEX` (0.001 NEX = 10^9 planck). Comfortably
    /// above the `10^(18-12) = 10^6` precision floor so inbound 18→12 conversion
    /// never truncates to zero (HB-ASSET-01 §3A.5 / G-A1-1).
    /// 最小桥接额：`MILLI_NEX`（0.001 NEX = 10^9 planck）。远高于 `10^(18-12)=10^6`
    /// 精度下限，保证入站 18→12 换算不会被截断为 0（HB-ASSET-01 §3A.5 / G-A1-1）。
    pub const BridgeMinAmount: Balance = MILLI_NEX;

    /// Rolling daily-limit window length (one day in blocks).
    /// 滚动单日限额窗口长度（一天的区块数）。
    pub const BridgeDailyWindow: BlockNumber = DAYS;

    /// Outbound ISMP request timeout in seconds (1 hour).
    /// 出站 ISMP 请求超时（秒，1 小时）。
    pub const BridgeRequestTimeout: u64 = 60 * 60;
}

/// EVM `H160` → Nexus `AccountId` derivation for cross-chain identities (HB-ENT-01,
/// G-B4): `blake2_256(b"nexus-evm" ++ h160)`. EVM wallet users have no Substrate key,
/// so their funds live in this deterministic derived account and can only be moved by
/// the matching EVM private key via the gateway (`bridge_out_from_derived`).
/// 跨链身份的 EVM `H160` → Nexus `AccountId` 派生（HB-ENT-01，G-B4）：
/// `blake2_256(b"nexus-evm" ++ h160)`。EVM 钱包用户无 Substrate 私钥，其资金存于此确定性
/// 派生账户，且只能由对应 EVM 私钥经网关（`bridge_out_from_derived`）驱动动用。
pub struct NexusEvmDerivation;
impl pallet_bridge_ismp::types::EvmToSubstrate<Runtime> for NexusEvmDerivation {
    fn convert(addr: sp_core::H160) -> AccountId {
        let mut data = Vec::with_capacity(9 + 20);
        data.extend_from_slice(b"nexus-evm");
        data.extend_from_slice(&addr.0);
        AccountId::from(sp_core::hashing::blake2_256(&data))
    }
}

/// Bridges authenticated cross-chain digital orders (HB-ENT-01) from
/// `pallet-bridge-ismp` into `pallet-entity-order::do_cross_order`, keeping the
/// low-level bridge decoupled from the order pallet. The bridge wraps this call in a
/// nested storage layer, so a returned error rolls back only the order side while the
/// inbound NEX mint is kept as DerivedCredit.
/// 将经鉴权的跨链数字下单（HB-ENT-01）从 `pallet-bridge-ismp` 接到
/// `pallet-entity-order::do_cross_order`，使底层桥与订单 pallet 解耦。桥会在嵌套存储层内
/// 调用本方法，故返回错误仅回滚订单侧，入站 NEX 铸造作为 DerivedCredit 保留。
pub struct NexusCrossOrderHandler;
impl pallet_bridge_ismp::types::CrossChainOrderHandler<AccountId, Balance> for NexusCrossOrderHandler {
    fn do_cross_order(
        buyer: AccountId,
        payer: AccountId,
        product_id: u64,
        quantity: u32,
        max_nex_amount: Balance,
        referrer: Option<AccountId>,
    ) -> Result<u64, sp_runtime::DispatchError> {
        pallet_entity_order::Pallet::<Runtime>::do_cross_order(
            buyer,
            payer,
            product_id,
            quantity,
            max_nex_amount,
            referrer,
        )
    }
}

/// `pallet-bridge-ismp` (Stage 2 / HB-ASSET-01): the self-built native-NEX asset
/// bridge. It dispatches outbound requests through `pallet-hyperbridge` (per-byte
/// fee + ISMP commit) and burns/mints native NEX directly via `Balances`. All
/// limits start at zero and no chains are registered, so the bridge is inert until
/// governance (`BridgeOrigin`) calls `register_chain` + `set_limits`.
/// `pallet-bridge-ismp`（Stage 2 / HB-ASSET-01）：自建原生 NEX 资产桥。出站请求经
/// `pallet-hyperbridge`（按字节费用 + ISMP 提交）派发，并直接经 `Balances` 对原生 NEX
/// burn/mint。限额初始为 0 且未注册任何链，故在治理（`BridgeOrigin`）调用
/// `register_chain` + `set_limits` 之前桥处于停用状态。
impl pallet_bridge_ismp::Config for Runtime {
    type RuntimeEvent = RuntimeEvent;
    type Dispatcher = pallet_hyperbridge::Pallet<Runtime>;
    type NativeCurrency = Balances;
    type EvmToSubstrate = NexusEvmDerivation;
    type NativeDecimals = BridgeNativeDecimals;
    type MinBridgeAmount = BridgeMinAmount;
    type DailyLimitWindow = BridgeDailyWindow;
    type RequestTimeout = BridgeRequestTimeout;
    type BridgeOrigin = EnsureRoot<AccountId>;
    type CrossOrderHandler = NexusCrossOrderHandler;
    type WeightInfo = pallet_bridge_ismp::weights::SubstrateWeight<Runtime>;
}
