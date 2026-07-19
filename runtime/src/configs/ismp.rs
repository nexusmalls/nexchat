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

use alloy_sol_types::SolValue;
use codec::Encode;
use frame_support::{parameter_types, BoundedVec, PalletId};
use frame_system::EnsureRoot;
use ismp::{
    host::StateMachine,
    module::IsmpModule,
    router::{GetResponse, IsmpRouter, PostRequest, Request},
};
use sp_runtime::traits::AccountIdConversion;

use crate::{
    AccountId, Assets, Balance, Balances, BlockNumber, Runtime, RuntimeEvent, Timestamp, DAYS,
    HOURS, MILLI_NEX,
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

/// V1 safety adapter that rejects inbound HFT optional calldata before the
/// upstream module can mint or transfer any asset.
/// V1 安全 adapter：在上游模块铸造或转移资产前拒绝入站 HFT optional calldata。
///
/// Upstream `2512.0.0` credits the beneficiary before dispatching optional
/// calldata. If that call fails, ISMP deletes the request receipt while those
/// asset effects are not explicitly rolled back, creating an unsafe retry
/// boundary. V1 requires empty `Message.data`, so enforce it at the router.
/// 上游 `2512.0.0` 会先给 beneficiary 入账，再派发 optional calldata。若调用失败，ISMP
/// 删除 request receipt，而资产副作用未被显式回滚，形成不安全的重试边界。V1 要求
/// `Message.data` 为空，因此在 router 层强制执行。
#[derive(Default)]
pub struct NoCallDataHftModule;

impl IsmpModule for NoCallDataHftModule {
    fn on_accept(
        &self,
        request: PostRequest,
    ) -> Result<frame_support::weights::Weight, anyhow::Error> {
        let message = pallet_hyper_fungible_token::types::Message::abi_decode(&request.body)
            .map_err(|error| anyhow::anyhow!("invalid HFT message: {error:?}"))?;
        if !message.data.is_empty() {
            return Err(anyhow::anyhow!(
                "HFT optional calldata is disabled in Nexus V1"
            ));
        }
        pallet_hyper_fungible_token::Pallet::<Runtime>::default().on_accept(request)
    }

    fn on_response(
        &self,
        response: GetResponse,
    ) -> Result<frame_support::weights::Weight, anyhow::Error> {
        pallet_hyper_fungible_token::Pallet::<Runtime>::default().on_response(response)
    }

    fn on_timeout(
        &self,
        request: Request,
    ) -> Result<frame_support::weights::Weight, anyhow::Error> {
        pallet_hyper_fungible_token::Pallet::<Runtime>::default().on_timeout(request)
    }
}

impl IsmpRouter for Router {
    fn module_for_id(&self, id: Vec<u8>) -> Result<Box<dyn IsmpModule>, anyhow::Error> {
        if id.as_slice() == pallet_hyperbridge::PALLET_HYPERBRIDGE_ID {
            Ok(Box::new(pallet_hyperbridge::Pallet::<Runtime>::default()))
        } else if id == pallet_bridge_ismp::module_id_bytes() {
            // Stage 2: route the NEX asset bridge's well-known module id to its pallet.
            // Stage 2：将 NEX 资产桥的 well-known 模块 id 路由到其 pallet。
            Ok(Box::new(pallet_bridge_ismp::Pallet::<Runtime>::default()))
        } else if id == pallet_hyper_fungible_token::PALLET_ID.to_bytes() {
            // Official HFT behind the V1 no-calldata safety boundary.
            // 官方 HFT，前置 V1 禁用 calldata 的安全边界。
            Ok(Box::new(NoCallDataHftModule))
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

    /// Blocks after which a resolved tracked-payout refund context may be pruned by
    /// `prune_payout_refunds`. Set to 7 days — far above the 1-hour request timeout —
    /// so an entry is never pruned while its timeout could still fire.
    /// 已解决的已跟踪派发退款上下文在多少区块后可被 `prune_payout_refunds` 清理。设为 7 天，
    /// 远高于 1 小时的请求超时，保证条目永不会在其超时仍可能触发时被清理。
    pub const BridgePayoutRefundTtl: BlockNumber = 7 * DAYS;

    /// Veto window for derived-account withdrawals (H2 containment). A queued
    /// withdrawal becomes executable only after this delay, during which the
    /// `BridgeOrigin` guardian can `cancel_withdraw` a suspicious entry stemming from a
    /// compromised/buggy EVM gateway. Set to 6 hours: long enough for monitoring/human
    /// response, short enough not to unduly delay legitimate withdrawals.
    /// 派生账户提款的否决窗口（H2 收敛）。排队提款须在此延迟后才可执行，窗口内 `BridgeOrigin`
    /// guardian 可对源自被攻破/有 bug 的 EVM 网关的可疑条目 `cancel_withdraw`。设为 6 小时：
    /// 足够监控/人工响应，又不过度延迟正常提款。
    pub const BridgeWithdrawDelay: BlockNumber = 6 * HOURS;
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

impl pallet_hyper_fungible_token::types::EvmToSubstrate<Runtime> for NexusEvmDerivation {
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
impl pallet_bridge_ismp::types::CrossChainOrderHandler<AccountId, Balance>
    for NexusCrossOrderHandler
{
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

    /// `do_cross_order` wraps `do_place_order`, so its worst-case weight is the
    /// benchmarked `place_order` weight (covers product/member/payment/commission).
    /// `do_cross_order` 包装 `do_place_order`，故其最坏权重即基准的 `place_order` 权重
    ///（覆盖商品/会员/支付/佣金）。
    fn cross_order_weight() -> frame_support::weights::Weight {
        <pallet_entity_order::weights::SubstrateWeight<Runtime> as pallet_entity_order::weights::WeightInfo>::place_order()
    }
}

/// Bridges the `withdrawal` part of a tiered commission withdrawal (HB-WD-01) from
/// `pallet-commission-core` into the bridge's outbound core
/// (`pallet-bridge-ismp::do_outbound`: burn NEX from the Entity account → ISMP POST),
/// keeping commission-core decoupled from the bridge crate (business→bridge, the dual
/// of `NexusCrossOrderHandler`). The `u32` chain id maps to `StateMachine::Evm`; ERC20
/// precision conversion (12→18) happens inside the bridge; `relayer_fee = 0` (entity
/// prepays no ISMP fee in this baseline). All bridge guardrails (pause / per-tx / daily
/// / chain registration / min / ED) apply automatically.
/// 将分级佣金提现的 `withdrawal` 部分（HB-WD-01）从 `pallet-commission-core` 接到桥的
/// 出站核心（`pallet-bridge-ismp::do_outbound`：从 Entity 账户 burn NEX → ISMP POST），
/// 使 commission-core 与桥 crate 解耦（业务→桥，`NexusCrossOrderHandler` 的对偶）。`u32`
/// 链 id 映射为 `StateMachine::Evm`；ERC20 精度换算（12→18）在桥内发生；`relayer_fee = 0`
///（本底线下 entity 不预付 ISMP 费）。桥的全部护栏（暂停 / 单笔 / 单日 / 链注册 / 最小额 /
/// ED）自动生效。
pub struct NexusCommissionPayout;
impl pallet_commission_core::CrossChainPayout<AccountId, Balance> for NexusCommissionPayout {
    fn payout_native(
        from: &AccountId,
        evm_chain_id: u32,
        recipient: [u8; 20],
        amount: Balance,
        refund_ctx: &[u8],
    ) -> Result<[u8; 32], sp_runtime::DispatchError> {
        let dest = StateMachine::Evm(evm_chain_id);
        let recipient = sp_core::H160(recipient);
        // Non-empty refund_ctx → tracked payout (HB-WD-01 mechanism 2): on timeout
        // the bridge will hand it back via PayoutRefundHandler.
        // refund_ctx 非空 → 已跟踪派发（HB-WD-01 机制 2）：超时时桥经 PayoutRefundHandler 交还。
        let commitment = if refund_ctx.is_empty() {
            pallet_bridge_ismp::Pallet::<Runtime>::do_outbound(
                from, dest, recipient, amount, 0u128,
            )?
        } else {
            let meta = BoundedVec::try_from(refund_ctx.to_vec())
                .map_err(|_| sp_runtime::DispatchError::Other("payout refund meta too long"))?;
            pallet_bridge_ismp::Pallet::<Runtime>::do_outbound_tracked(
                from, dest, recipient, amount, 0u128, meta,
            )?
        };
        Ok(commitment.0)
    }
}

/// Bridge → business timeout callback (HB-WD-01 mechanism 2): when a tracked
/// commission payout times out, `pallet-bridge-ismp::on_timeout` (after re-minting
/// the NEX back to the Entity account) hands the opaque meta here, which forwards
/// to `pallet-commission-core::on_payout_timeout` to restore the promoter's pending.
/// Dual of `NexusCommissionPayout` (business → bridge → business).
/// 桥 → 业务的超时回调（HB-WD-01 机制 2）：已跟踪的佣金派发超时时，
/// `pallet-bridge-ismp::on_timeout`（在把 NEX 铸回 Entity 账户后）把不透明 meta 交到这里，
/// 转发给 `pallet-commission-core::on_payout_timeout` 恢复推广员 pending。是
/// `NexusCommissionPayout` 的对偶（业务 → 桥 → 业务）。
pub struct NexusPayoutRefundHandler;
impl pallet_bridge_ismp::types::PayoutRefundHandler for NexusPayoutRefundHandler {
    fn on_payout_timeout(meta: &[u8]) -> Result<(), sp_runtime::DispatchError> {
        pallet_commission_core::Pallet::<Runtime>::on_payout_timeout(meta)
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
    type PayoutRefundHandler = NexusPayoutRefundHandler;
    // HB-WD-01 meta = (entity_id: u64, who: AccountId[32], amount: u128) ≈ 56 bytes;
    // 128 leaves headroom.
    type MaxPayoutMeta = frame_support::traits::ConstU32<128>;
    type PayoutRefundTtl = BridgePayoutRefundTtl;
    type WithdrawDelay = BridgeWithdrawDelay;
    type WeightInfo = pallet_bridge_ismp::weights::SubstrateWeight<Runtime>;
}

parameter_types! {
    /// Sentinel AssetId selecting the native-currency path in the official HFT pallet.
    /// 官方 HFT pallet 中选择原生货币路径的哨兵 AssetId。
    ///
    /// No HFT token is registered at genesis. Governance must not register this
    /// sentinel until native-NEX HFT coexistence with `pallet-bridge-ismp` is separately specified.
    /// genesis 不注册 HFT token；在原生 NEX HFT 与 `pallet-bridge-ismp` 共存方案另行制定前，
    /// 治理不得注册该哨兵值。
    pub const HftNativeAssetId: u64 = u64::MAX;
}

/// Official Hyperbridge HFT pallet, published as `2512.0.0` from upstream
/// commit `3979482228d9001f0463f3192524fa41bc76989b`.
/// 官方 Hyperbridge HFT pallet；`2512.0.0` 发布自上游 commit
/// `3979482228d9001f0463f3192524fa41bc76989b`。
///
/// The pallet is wired but has no genesis token registrations. Nexus-generated
/// weights cover bounded send/registry calls; the optional calldata callback
/// path still requires an explicit audit before production token registration.
/// 该 pallet 已接线但 genesis 不注册 token。Nexus 实测权重已覆盖有界 send/registry
/// 调用；可选 calldata callback 路径仍须在生产 token 注册前完成专项审计。
/// Phase-1 registry governance is restricted to Root.
/// Phase 1 registry 治理仅限 Root。
type HftCreateOrigin = EnsureRoot<AccountId>;

impl pallet_hyper_fungible_token::Config for Runtime {
    type Dispatcher = pallet_hyperbridge::Pallet<Runtime>;
    type NativeCurrency = Balances;
    type CreateOrigin = HftCreateOrigin;
    type Assets = Assets;
    type NativeAssetId = HftNativeAssetId;
    type Decimals = BridgeNativeDecimals;
    type EvmToSubstrate = NexusEvmDerivation;
    type WeightInfo = pallet_hyper_fungible_token::weights::SubstrateWeight<Runtime>;
    #[cfg(feature = "runtime-benchmarks")]
    type BenchmarkHelper = NexusHftBenchmarkHelper;
}

#[cfg(feature = "runtime-benchmarks")]
pub struct NexusHftBenchmarkHelper;

#[cfg(feature = "runtime-benchmarks")]
impl pallet_hyper_fungible_token::BenchmarkHelper<Runtime> for NexusHftBenchmarkHelper {
    fn create_asset(asset_id: u64, owner: AccountId) {
        let _ = Assets::force_create(
            crate::RuntimeOrigin::root(),
            codec::Compact(asset_id),
            sp_runtime::MultiAddress::Id(owner),
            true,
            1,
        );
        let _ = Assets::force_set_metadata(
            crate::RuntimeOrigin::root(),
            codec::Compact(asset_id),
            b"xUSDC".to_vec(),
            b"xUSDC".to_vec(),
            6,
            false,
        );
    }
}

/// Canonical receipt adapter backed exclusively by the official HFT pallet's
/// forward/reverse registries and custody-mode storage.
/// 仅由官方 HFT pallet 的正向/反向 registry 与 custody-mode storage 支持的规范收据 adapter。
pub struct NexusHftReceiptValidator;

impl NexusHftReceiptValidator {
    /// Returns the Polygon state machine selected by this runtime build.
    /// 返回当前 runtime 构建选择的 Polygon 状态机。
    fn polygon_source() -> StateMachine {
        StateMachine::Evm(80_002)
    }

    /// Returns the Circle USDC contract selected for the Polygon lane.
    /// 返回 Polygon 通道当前选择的 Circle USDC 合约。
    fn polygon_underlying() -> [u8; 20] {
        // Circle test USDC on Polygon Amoy. Re-verify with Circle before deployment.
        // Polygon Amoy 上的 Circle 测试 USDC；部署前须再次向 Circle 核验。
        [
            0x41, 0xe9, 0x4e, 0xb0, 0x19, 0xc0, 0x76, 0x2f, 0x9b, 0xfc, 0xf9, 0xfb, 0x1e, 0x58,
            0x72, 0x5b, 0xfb, 0x0e, 0x75, 0x82,
        ]
    }

    fn source(asset_id: u64) -> Option<StateMachine> {
        match asset_id {
            900_001 => Some(Self::polygon_source()),
            900_002 => Some(StateMachine::Evm(1)),
            _ => None,
        }
    }

    fn expected_underlying(asset_id: u64) -> Option<[u8; 20]> {
        match asset_id {
            900_001 => Some(Self::polygon_underlying()),
            // Circle native USDC on Ethereum.
            900_002 => Some([
                0xa0, 0xb8, 0x69, 0x91, 0xc6, 0x21, 0x8b, 0x36, 0xc1, 0xd1, 0x9d, 0x4a, 0x2e, 0x9e,
                0xb0, 0xce, 0x36, 0x06, 0xeb, 0x48,
            ]),
            _ => None,
        }
    }

    fn canonical(asset_id: u64) -> Option<(StateMachine, Vec<u8>, u8)> {
        let source = Self::source(asset_id)?;
        let contract =
            pallet_hyper_fungible_token::TokenContracts::<Runtime>::get(source, asset_id)?;
        if contract.len() != 20
            || pallet_hyper_fungible_token::NativeAssets::<Runtime>::get(asset_id)
            || pallet_hyper_fungible_token::ContractToAsset::<Runtime>::get(
                source,
                contract.clone(),
            ) != Some(asset_id)
        {
            return None;
        }
        let decimals = pallet_hyper_fungible_token::Precisions::<Runtime>::get(asset_id, source)?;
        if decimals != 6 {
            return None;
        }
        Some((source, contract, decimals))
    }
}

impl pallet_usdx::ReceiptValidator for NexusHftReceiptValidator {
    fn descriptor_hash(asset_id: u64) -> Option<sp_core::H256> {
        let (source, contract, decimals) = Self::canonical(asset_id)?;
        let descriptor = (
            asset_id,
            source,
            contract,
            decimals,
            false,
            pallet_hyper_fungible_token::PALLET_ID.to_bytes(),
        )
            .encode();
        Some(sp_core::H256::from(sp_core::hashing::blake2_256(
            &descriptor,
        )))
    }

    fn validate_evidence(
        asset_id: u64,
        descriptor_hash: sp_core::H256,
        evidence: &pallet_usdx::LaneActivationEvidence,
    ) -> bool {
        let Some((source, contract, _)) = Self::canonical(asset_id) else {
            return false;
        };
        let Some(expected_underlying) = Self::expected_underlying(asset_id) else {
            return false;
        };
        let expected_peer_hash = sp_core::H256::from(sp_core::hashing::blake2_256(
            &(
                HostStateMachine::get(),
                pallet_hyper_fungible_token::PALLET_ID.to_bytes(),
            )
                .encode(),
        ));
        Self::descriptor_hash(asset_id) == Some(descriptor_hash)
            && evidence.wrapper_contract.as_slice() == contract.as_slice()
            && evidence.underlying_contract == expected_underlying
            && evidence.owner_contract != [0; 20]
            && evidence.host_contract != [0; 20]
            && evidence.dispatcher_contract != [0; 20]
            && !evidence.is_weth
            && evidence.hft_bytecode_hash != sp_core::H256::zero()
            && evidence.controller_bytecode_hash != sp_core::H256::zero()
            && evidence.config_block > 0
            && evidence.config_block_hash != sp_core::H256::zero()
            && evidence.nexus_peer_hash == expected_peer_hash
            && evidence.proof_bundle_hash != sp_core::H256::zero()
            && source.is_evm()
    }
}

parameter_types! {
    /// Reserved USDX protocol AssetId. The asset is not created by this Phase-0 wiring.
    /// 预留的 USDX 协议 AssetId；Phase 0 接线不会创建该资产。
    pub const UsdxAssetId: u64 = 900_000;

    /// Sovereign account holding PSM collateral receipts.
    /// 持有 PSM 抵押收据的 sovereign account。
    pub const UsdxPsmPalletId: PalletId = PalletId(*b"nex/usdx");

    /// Inactive sovereign account owning protocol-level asset configuration.
    /// 持有协议级资产配置所有权的不可签名 sovereign account。
    pub const ProtocolAssetsAdminPalletId: PalletId = PalletId(*b"nex/asts");
}

/// Immutable runtime specification for a USDX protocol asset.
/// USDX 协议资产的不可变 runtime 规格。
pub(crate) struct ProtocolAssetSpec {
    pub asset_id: u64,
    pub name: &'static [u8],
    pub symbol: &'static [u8],
    pub issuer: AccountId,
}

/// Returns the fixed protocol-asset specification for migration and validation.
/// 返回供迁移和校验共同使用的固定协议资产规格。
pub(crate) fn protocol_asset_spec(asset_id: u64) -> Option<ProtocolAssetSpec> {
    let issuer = match asset_id {
        900_000 => pallet_usdx::Pallet::<Runtime>::psm_account(),
        900_001 | 900_002 => pallet_hyper_fungible_token::Pallet::<Runtime>::pallet_account(),
        _ => return None,
    };
    let (name, symbol): (&'static [u8], &'static [u8]) = match asset_id {
        900_000 => (b"Nexus USD", b"USDX"),
        900_001 => (b"Polygon USDC Receipt", b"xUSDC-POL"),
        900_002 => (b"Ethereum USDC Receipt", b"xUSDC-ETH"),
        _ => return None,
    };
    Some(ProtocolAssetSpec {
        asset_id,
        name,
        symbol,
        issuer,
    })
}

/// Strict inspector for the three reserved pallet-assets protocol assets.
/// 三个保留 pallet-assets 协议资产的严格检查器。
pub struct NexusProtocolAssetInspector;

impl NexusProtocolAssetInspector {
    fn validate(asset_id: u64, expected_issuer: &AccountId) -> bool {
        let Some(spec) = protocol_asset_spec(asset_id) else {
            return false;
        };
        if &spec.issuer != expected_issuer {
            return false;
        }
        let Some(details) = pallet_assets::Asset::<Runtime>::get(asset_id) else {
            return false;
        };
        let metadata = pallet_assets::Metadata::<Runtime>::get(asset_id);
        let admin = ProtocolAssetsAdminPalletId::get().into_account_truncating();
        details.owner == admin
            && details.issuer == spec.issuer
            && details.admin == admin
            && details.freezer == admin
            && details.min_balance == 1
            && details.is_sufficient
            && details.status == pallet_assets::AssetStatus::Live
            && metadata.name.as_slice() == spec.name
            && metadata.symbol.as_slice() == spec.symbol
            && metadata.decimals == 6
            && metadata.is_frozen
    }
}

impl pallet_usdx::ProtocolAssetInspector<AccountId> for NexusProtocolAssetInspector {
    fn validate_usdx(asset_id: u64, psm_account: &AccountId) -> bool {
        asset_id == UsdxAssetId::get() && Self::validate(asset_id, psm_account)
    }

    fn validate_receipt(asset_id: u64) -> bool {
        matches!(asset_id, 900_001 | 900_002)
            && Self::validate(
                asset_id,
                &pallet_hyper_fungible_token::Pallet::<Runtime>::pallet_account(),
            )
    }
}

/// Phase-1 USDX wiring for the Polygon Amoy lane.
/// Polygon Amoy 通道的 Phase 1 USDX 接线。
///
/// Receipt identity is read from the official HFT pallet's canonical storage.
/// The strict protocol-asset inspector is enabled, while the HFT registry, debt
/// ceilings and lane limits still default to empty/zero.
///
/// 收据身份读取自官方 HFT pallet 的规范 storage，并启用严格协议资产 inspector；
/// HFT registry、债务上限和通道限额仍默认为空或零。
type UsdxProtocolAssetInspector = NexusProtocolAssetInspector;

impl pallet_usdx::Config for Runtime {
    type Assets = Assets;
    type AdminOrigin = EnsureRoot<AccountId>;
    type PauseOrigin = EnsureRoot<AccountId>;
    type ReceiptValidator = NexusHftReceiptValidator;
    type ProtocolAssetInspector = UsdxProtocolAssetInspector;
    type UsdxAssetId = UsdxAssetId;
    type PsmPalletId = UsdxPsmPalletId;
    type WeightInfo = pallet_usdx::weights::SubstrateWeight<Runtime>;
    #[cfg(feature = "runtime-benchmarks")]
    type BenchmarkHelper = NexusUsdxBenchmarkHelper;
}

#[cfg(feature = "runtime-benchmarks")]
pub struct NexusUsdxBenchmarkHelper;

#[cfg(feature = "runtime-benchmarks")]
impl pallet_usdx::BenchmarkHelper<Runtime> for NexusUsdxBenchmarkHelper {
    fn prepare() {
        use frame_support::traits::OnRuntimeUpgrade;

        crate::migrations::InitializeUsdxProtocolAssets::on_runtime_upgrade();
        let source = NexusHftReceiptValidator::polygon_source();
        let wrapper = [0x33; 20].to_vec();
        pallet_hyper_fungible_token::TokenContracts::<Runtime>::insert(
            source,
            900_001,
            wrapper.clone(),
        );
        pallet_hyper_fungible_token::ContractToAsset::<Runtime>::insert(source, wrapper, 900_001);
        pallet_hyper_fungible_token::NativeAssets::<Runtime>::insert(900_001, false);
        pallet_hyper_fungible_token::Precisions::<Runtime>::insert(900_001, source, 6);
    }

    fn evidence(receipt_asset_id: u64) -> pallet_usdx::LaneActivationEvidence {
        let underlying = NexusHftReceiptValidator::expected_underlying(receipt_asset_id)
            .expect("benchmark receipt has a fixed underlying");
        let peer_hash = sp_core::H256::from(sp_core::hashing::blake2_256(
            &(
                HostStateMachine::get(),
                pallet_hyper_fungible_token::PALLET_ID.to_bytes(),
            )
                .encode(),
        ));
        pallet_usdx::LaneActivationEvidence {
            wrapper_contract: [0x33; 20],
            underlying_contract: underlying,
            owner_contract: [0x44; 20],
            host_contract: [0x55; 20],
            dispatcher_contract: [0x66; 20],
            is_weth: false,
            hft_bytecode_hash: sp_core::H256::repeat_byte(0x77),
            controller_bytecode_hash: sp_core::H256::repeat_byte(0x88),
            config_block: 1,
            config_block_hash: sp_core::H256::repeat_byte(0x99),
            nexus_peer_hash: peer_hash,
            proof_bundle_hash: sp_core::H256::repeat_byte(0xAA),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{configs::NexusBaseCallFilter, RuntimeCall};
    use frame_support::traits::Contains;
    use pallet_usdx::ReceiptValidator;

    #[test]
    fn hft_router_rejects_optional_calldata_before_dispatch() {
        let message = pallet_hyper_fungible_token::types::Message {
            from: alloc::vec![0x11; 20].into(),
            to: alloc::vec![0x22; 32].into(),
            amount: Default::default(),
            data: alloc::vec![0x01].into(),
        };
        let request = PostRequest {
            source: StateMachine::Evm(137),
            dest: HostStateMachine::get(),
            nonce: 1,
            from: alloc::vec![0x33; 20],
            to: pallet_hyper_fungible_token::PALLET_ID.to_bytes(),
            timeout_timestamp: u64::MAX,
            body: message.abi_encode(),
        };

        let error = NoCallDataHftModule
            .on_accept(request)
            .expect_err("non-empty HFT calldata must fail before upstream dispatch");
        assert!(error.to_string().contains("optional calldata is disabled"));
    }

    #[test]
    fn usdx_receipt_descriptor_requires_consistent_hft_registry() {
        sp_io::TestExternalities::default().execute_with(|| {
            assert_eq!(NexusHftReceiptValidator::descriptor_hash(900_001), None);

            let source = NexusHftReceiptValidator::polygon_source();
            let wrapper = alloc::vec![0x33; 20];
            pallet_hyper_fungible_token::TokenContracts::<Runtime>::insert(
                source,
                900_001,
                wrapper.clone(),
            );
            pallet_hyper_fungible_token::ContractToAsset::<Runtime>::insert(
                source,
                wrapper.clone(),
                900_001,
            );
            pallet_hyper_fungible_token::NativeAssets::<Runtime>::insert(900_001, false);
            pallet_hyper_fungible_token::Precisions::<Runtime>::insert(900_001, source, 6);

            let descriptor = NexusHftReceiptValidator::descriptor_hash(900_001)
                .expect("consistent official HFT registry should produce a descriptor");
            let peer_hash = sp_core::H256::from(sp_core::hashing::blake2_256(
                &(
                    HostStateMachine::get(),
                    pallet_hyper_fungible_token::PALLET_ID.to_bytes(),
                )
                    .encode(),
            ));
            let evidence = pallet_usdx::LaneActivationEvidence {
                wrapper_contract: [0x33; 20],
                underlying_contract: NexusHftReceiptValidator::expected_underlying(900_001)
                    .expect("Polygon USDC is fixed for this runtime profile"),
                owner_contract: [0x44; 20],
                host_contract: [0x55; 20],
                dispatcher_contract: [0x66; 20],
                is_weth: false,
                hft_bytecode_hash: sp_core::H256::repeat_byte(0x77),
                controller_bytecode_hash: sp_core::H256::repeat_byte(0x88),
                config_block: 1,
                config_block_hash: sp_core::H256::repeat_byte(0x99),
                nexus_peer_hash: peer_hash,
                proof_bundle_hash: sp_core::H256::repeat_byte(0xAA),
            };
            assert!(NexusHftReceiptValidator::validate_evidence(
                900_001, descriptor, &evidence,
            ));

            pallet_hyper_fungible_token::Precisions::<Runtime>::insert(900_001, source, 18);
            assert_eq!(NexusHftReceiptValidator::descriptor_hash(900_001), None);
        });
    }

    #[test]
    fn polygon_receipt_profile_targets_amoy_test_usdc() {
        assert_eq!(
            NexusHftReceiptValidator::source(900_001),
            Some(StateMachine::Evm(80_002))
        );
        assert_eq!(
            NexusHftReceiptValidator::expected_underlying(900_001),
            Some([
                0x41, 0xe9, 0x4e, 0xb0, 0x19, 0xc0, 0x76, 0x2f, 0x9b, 0xfc, 0xf9, 0xfb, 0x1e, 0x58,
                0x72, 0x5b, 0xfb, 0x0e, 0x75, 0x82,
            ])
        );
    }

    #[test]
    fn phase_one_filter_allows_only_empty_calldata_send_and_registry_governance() {
        let send = |call_data| {
            RuntimeCall::HyperFungibleToken(pallet_hyper_fungible_token::Call::send {
                params: pallet_hyper_fungible_token::types::SendParams {
                    asset_id: 900_001,
                    destination: StateMachine::Evm(80_002),
                    recipient: BoundedVec::truncate_from(alloc::vec![0x11; 20]),
                    amount: 1,
                    timeout: 60,
                    relayer_fee: 0,
                    call_data,
                },
            })
        };
        assert!(NexusBaseCallFilter::contains(&send(None)));
        assert!(!NexusBaseCallFilter::contains(&send(Some(
            BoundedVec::default()
        ))));
        assert!(!NexusBaseCallFilter::contains(&send(Some(
            BoundedVec::truncate_from(alloc::vec![1])
        ))));

        let register =
            RuntimeCall::HyperFungibleToken(pallet_hyper_fungible_token::Call::register_token {
                registration: pallet_hyper_fungible_token::types::TokenRegistration {
                    local_id: 900_001,
                    native: false,
                    chains: Default::default(),
                },
            });
        let update =
            RuntimeCall::HyperFungibleToken(pallet_hyper_fungible_token::Call::update_token {
                update: pallet_hyper_fungible_token::types::TokenUpdate {
                    asset_id: 900_001,
                    add_chains: Default::default(),
                    remove_chains: Default::default(),
                },
            });
        assert!(NexusBaseCallFilter::contains(&register));
        assert!(NexusBaseCallFilter::contains(&update));
        assert!(NexusBaseCallFilter::contains(&RuntimeCall::System(
            frame_system::Call::remark {
                remark: alloc::vec![]
            },
        )));
    }
}
