//! ISMP / Hyperbridge protocol-layer runtime configuration.
//! ISMP / Hyperbridge 协议层运行时配置。
//!
//! Stage 1a wired the ISMP core engine `pallet-ismp` (request/response, host,
//! dispatcher, consensus-state store) plus its runtime API. Stage 1b adds the
//! vendored `pallet-hyperbridge` (host-param / fee module that receives governance
//! updates from the Hyperbridge coprocessor) — vendored under D3=(c) because the
//! published crate does not compile against the only available `ismp 2512.1.0` (the
//! matching `ismp 2512.0.0` was yanked). One component is still deferred:
//!   - `ismp-grandpa` — GRANDPA consensus client used to verify Hyperbridge proofs.
//! Asset bridging (`pallet-bridge-ismp`, vendored from HFT per D3=(c)) is added in
//! Stage 2. See `docs/HYPERBRIDGE_INTEGRATION.md` §13 for the supply-chain rationale.
//!
//! Stage 1a 已接入 ISMP 核心引擎 `pallet-ismp`（请求/响应、host、dispatcher、共识状态
//! 存储）及其 runtime API。Stage 1b 加入 vendor 的 `pallet-hyperbridge`（host-param / 费用
//! 模块，接收 Hyperbridge 协处理器的治理更新）——按 D3=(c) vendor，因为已发布 crate 无法对
//! 唯一可用的 `ismp 2512.1.0` 编译（对应的 `ismp 2512.0.0` 已被 yank）。仍暂缓一个组件：
//!   - `ismp-grandpa`——用于验证 Hyperbridge 证明的 GRANDPA 共识客户端。
//! 资产桥（`pallet-bridge-ismp`，按 D3=(c) 从 HFT vendor）在 Stage 2 引入。
//! 供应链原因见 `docs/HYPERBRIDGE_INTEGRATION.md` §13。

use alloc::{boxed::Box, vec::Vec};

use frame_support::parameter_types;
use frame_system::EnsureRoot;
use ismp::{host::StateMachine, module::IsmpModule, router::IsmpRouter};

use crate::{AccountId, Balance, Balances, Runtime, Timestamp};

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
        match id.as_slice() {
            pallet_hyperbridge::PALLET_HYPERBRIDGE_ID =>
                Ok(Box::new(pallet_hyperbridge::Pallet::<Runtime>::default())),
            _ => Err(anyhow::anyhow!("No ISMP module registered for id {:?}", id)),
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
