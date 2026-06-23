//! ISMP / Hyperbridge protocol-layer runtime configuration.
//! ISMP / Hyperbridge 协议层运行时配置。
//!
//! Stage 1a wires only the ISMP core engine `pallet-ismp` (request/response, host,
//! dispatcher, consensus-state store) plus its runtime API. Two components are
//! deferred to Stage 1b because the published crates do not compile against the only
//! available `ismp 2512.1.0` (the matching `ismp 2512.0.0` was yanked):
//!   - `pallet-hyperbridge` — host-param / fee module (governance updates from the
//!     Hyperbridge coprocessor);
//!   - `ismp-grandpa` — GRANDPA consensus client used to verify Hyperbridge proofs.
//! Asset bridging (`pallet-bridge-ismp`, vendored from HFT per D3=(c)) is added in
//! Stage 2. See `docs/HYPERBRIDGE_INTEGRATION.md` §13 for the supply-chain rationale.
//!
//! Stage 1a 仅接入 ISMP 核心引擎 `pallet-ismp`（请求/响应、host、dispatcher、共识状态
//! 存储）及其 runtime API。两个组件暂缓至 Stage 1b——因为已发布 crate 无法对唯一可用的
//! `ismp 2512.1.0` 编译（对应的 `ismp 2512.0.0` 已被 yank）：
//!   - `pallet-hyperbridge`——host-param / 费用模块（接收 Hyperbridge 协处理器的治理更新）；
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
/// Stage 1a registers no modules yet: the `pallet-hyperbridge` fee/host-param
/// module is deferred (Stage 1b), and asset / cross-order modules arrive in later
/// stages. Until a module is registered the host accepts no inbound application
/// requests (the core consensus/state-proof engine still runs).
/// Stage 1a 尚未注册任何模块：`pallet-hyperbridge` 费用/host-param 模块暂缓（Stage 1b），
/// 资产 / 跨链下单模块在后续阶段加入。在注册模块前，host 不接受入站应用请求
///（核心共识/状态证明引擎仍正常运行）。
#[derive(Default)]
pub struct Router;

impl IsmpRouter for Router {
    fn module_for_id(&self, id: Vec<u8>) -> Result<Box<dyn IsmpModule>, anyhow::Error> {
        // Stage 1b: route `pallet_hyperbridge::PALLET_HYPERBRIDGE_ID` to
        // `pallet_hyperbridge::Pallet::<Runtime>` here once that pallet is enabled.
        // Stage 1b：启用该 pallet 后，在此将 `pallet_hyperbridge::PALLET_HYPERBRIDGE_ID`
        // 路由到 `pallet_hyperbridge::Pallet::<Runtime>`。
        Err(anyhow::anyhow!("No ISMP module registered for id {:?}", id))
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
    // Stage 1b: register the live GRANDPA consensus client here, i.e.
    // `(ismp_grandpa::consensus::GrandpaConsensusClient<Runtime>,)`. Until then no
    // consensus client is configured, so inbound proofs cannot yet be verified.
    // Stage 1b：此处注册实际 GRANDPA 共识客户端，即
    // `(ismp_grandpa::consensus::GrandpaConsensusClient<Runtime>,)`；在此之前未配置
    // 共识客户端，入站证明尚不可验证。
    type ConsensusClients = ();
    type FeeHandler = ();
    type OffchainDB = ();
    type MigrationWeightInfo = ();
}

// Stage 1b: `pallet_hyperbridge::Config` and `ismp_grandpa::Config` are added once
// those pallets have releases compatible with `ismp 2512.1.0` (or are vendored per
// D3=(c)). `pallet_ismp::Pallet<Runtime>` implements `IsmpDispatcher + IsmpHost +
// Default`, so both will reuse it as the host:
//
//   impl pallet_hyperbridge::Config for Runtime {
//       type IsmpHost = pallet_ismp::Pallet<Runtime>;
//   }
//   impl ismp_grandpa::Config for Runtime {
//       type IsmpHost = pallet_ismp::Pallet<Runtime>;
//       type WeightInfo = ();
//       type RootOrigin = EnsureRoot<AccountId>;
//   }
//
// Stage 1b：待 `pallet-hyperbridge`/`ismp-grandpa` 有与 `ismp 2512.1.0` 兼容的发布
//（或按 D3=(c) vendor）后补回上述 Config。`pallet_ismp::Pallet<Runtime>` 实现
// `IsmpDispatcher + IsmpHost + Default`，两者都将复用它作为 host。
