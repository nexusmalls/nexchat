// Copyright (C) Polytope Labs Ltd. (vendored on_accept / on_timeout shape)
// Copyright (C) Nexus contributors (native mint/refund adaptation + guardrails)
// SPDX-License-Identifier: Apache-2.0

//! ISMP module callbacks for the NEX asset bridge.
//! NEX 资产桥的 ISMP 模块回调。
//!
//! Replay / idempotency note: `pallet-ismp` records request receipts and rejects
//! duplicates *before* dispatching to this module, so neither `on_accept` (inbound
//! mint) nor `on_timeout` (refund mint) can be replayed. We therefore do not keep
//! a redundant commitment set, matching the audited HFT design.
//! 重放 / 幂等说明：`pallet-ismp` 在派发到本模块**之前**记录请求回执并拒绝重复，故
//! `on_accept`（入站铸造）与 `on_timeout`（退款铸造）均不会被重放。因此我们不额外维护
//! commitment 集合，与已审计的 HFT 设计一致。

use crate::{
    error::BridgeError,
    pallet::{Chains, Event, Paused, PausedChain, PayoutRefunds},
    types::{CrossChainOrderHandler, EvmToSubstrate, InboundOp, OrderIntent, PayoutRefundHandler},
    BalanceOf, Config, Message, Pallet,
};
use alloy_sol_types::SolValue;
use codec::Decode;
use frame_support::{
    storage::{transactional::with_transaction_opaque_err, with_storage_layer},
    weights::Weight,
};
use ismp::{
    host::StateMachine,
    module::IsmpModule,
    router::{GetResponse, PostRequest, Request},
};
use sp_core::H160;
use sp_runtime::{traits::Get, TransactionOutcome};

/// Maps a [`BridgeError`] into the `anyhow::Error` that `pallet-ismp` expects.
/// 将 [`BridgeError`] 映射为 `pallet-ismp` 期望的 `anyhow::Error`。
fn into_anyhow(e: BridgeError) -> anyhow::Error {
    anyhow::anyhow!(alloc::format!("{e}"))
}

/// Runs an inbound-callback body inside a transactional storage layer: on `Ok` the
/// writes commit, on **any** `Err` they are fully rolled back before the error
/// propagates. Because `pallet-ismp` deletes the request receipt whenever a callback
/// returns `Err` (so the request can be re-delivered or timed out), this makes the
/// "no fallible step after a mint" rule **structural** rather than a convention: a
/// future change that mints and then fails can no longer leave the mint persisted
/// while the receipt is dropped — which would otherwise enable a retry double-mint.
/// Intentional `Ok` returns that keep a mint (e.g. a cross-order whose order failed
/// but whose NEX is kept as DerivedCredit) still commit normally.
/// 在事务存储层内执行入站回调主体：返回 `Ok` 则提交写入，返回**任何** `Err` 则在错误传播前
/// 全部回滚。由于 `pallet-ismp` 在回调返回 `Err` 时会删除请求回执（以便请求被重投或超时），
/// 这把「mint 之后不得有可失败步骤」从约定变为**结构性保证**：未来若有改动先 mint 再失败，
/// 也不会出现「mint 已落库而回执被删」从而导致重试双铸的情况。刻意返回 `Ok` 并保留 mint 的
/// 路径（例如下单失败但 NEX 作为 DerivedCredit 保留的跨链下单）仍正常提交。
fn in_transaction(
    f: impl FnOnce() -> Result<Weight, anyhow::Error>,
) -> Result<Weight, anyhow::Error> {
    match with_transaction_opaque_err(|| match f() {
        Ok(weight) => TransactionOutcome::Commit(Ok(weight)),
        Err(e) => TransactionOutcome::Rollback(Err(e)),
    }) {
        Ok(inner) => inner,
        // `Err(())` means the nested transactional-layer limit was hit; surface it as
        // a callback error so nothing is committed (the request stays retriable).
        // `Err(())` 表示触及嵌套事务层上限；作为回调错误抛出，从而不提交任何写入（请求仍可重试）。
        Err(()) => Err(anyhow::anyhow!(
            "bridge inbound exceeded transactional layer limit"
        )),
    }
}

impl<T: Config> IsmpModule for Pallet<T>
where
    T::AccountId: From<[u8; 32]> + Into<[u8; 32]>,
    BalanceOf<T>: core::str::FromStr + Into<u128>,
    <T as pallet_ismp::Config>::Balance: From<BalanceOf<T>>,
{
    /// Inbound: an EVM chain burned `NEX` and Hyperbridge proved it. Mint the
    /// local NEX (within the in-flight ledger) to the beneficiary.
    /// 入站：某 EVM 链销毁了 `NEX` 且 Hyperbridge 已证明。在在途账本内向受益人铸造本地 NEX。
    fn on_accept(&self, request: PostRequest) -> Result<Weight, anyhow::Error> {
        in_transaction(move || {
            let PostRequest {
                body, from, source, ..
            } = request;
            // Pause checks first / 先做暂停检查
            if Paused::<T>::get() || PausedChain::<T>::get(source) {
                Err(into_anyhow(BridgeError::Paused(source)))?
            }

            // Source allow-list: the registered contract for `source` must equal `from`.
            // 来源 allow-list：`source` 已注册合约必须等于 `from`。
            let cfg = Chains::<T>::get(source)
                .ok_or_else(|| into_anyhow(BridgeError::UnregisteredChain(source)))?;
            if from != cfg.contract.0.to_vec() {
                Err(into_anyhow(BridgeError::UnknownSourceContract(source)))?
            }

            // Decode the vendored Message ABI. / 解码 vendor 的 Message ABI。
            let message =
                Message::abi_decode(&body).map_err(|e| into_anyhow(BridgeError::DecodeError(e)))?;

            // `data` empty → plain asset transfer (Stage 2). Non-empty → an authenticated
            // operation (HB-ENT-01, Stage 3) decoded from the SCALE `InboundOp` enum. Only
            // the asset-bearing paths read `message.amount` (a withdraw carries `amount=0`).
            // `data` 为空 → 纯资产转账（Stage 2）。非空 → 经鉴权操作（HB-ENT-01，Stage 3），
            // 按 SCALE `InboundOp` 枚举解码。仅携带资产的路径读取 `message.amount`（提款 `amount=0`）。
            if message.data.is_empty() {
                let amount = Pallet::<T>::decode_amount(message.amount, cfg.erc_decimals)
                    .map_err(into_anyhow)?;
                let beneficiary =
                    Pallet::<T>::account_from_bytes(message.to.as_ref()).map_err(into_anyhow)?;
                Pallet::<T>::credit_inbound(&beneficiary, source, amount).map_err(into_anyhow)?;
                Pallet::<T>::deposit_event(Event::<T>::BridgedIn {
                    beneficiary,
                    source,
                    amount,
                });
                return Ok(<T as frame_system::Config>::DbWeight::get().reads_writes(6, 3));
            }

            let op = InboundOp::decode(&mut message.data.as_ref())
                .map_err(|_| into_anyhow(BridgeError::InvalidPayload))?;
            match op {
                InboundOp::Order(intent) => {
                    if intent.schema_version != crate::types::PAYLOAD_SCHEMA_VERSION {
                        Err(into_anyhow(BridgeError::UnsupportedSchemaVersion(
                            intent.schema_version,
                        )))?
                    }
                    let amount = Pallet::<T>::decode_amount(message.amount, cfg.erc_decimals)
                        .map_err(into_anyhow)?;
                    Self::on_cross_order(source, amount, intent)
                }
                InboundOp::Withdraw(req) => {
                    if req.schema_version != crate::types::PAYLOAD_SCHEMA_VERSION {
                        Err(into_anyhow(BridgeError::UnsupportedSchemaVersion(
                            req.schema_version,
                        )))?
                    }
                    Self::on_withdraw(source, req)
                }
            }
        })
    }

    // (cross-order handling lives in the inherent impl below)

    /// The asset bridge does not issue GET requests, so it never receives responses.
    /// 资产桥不发起 GET 请求，故永不接收响应。
    fn on_response(&self, _response: GetResponse) -> Result<Weight, anyhow::Error> {
        Err(into_anyhow(BridgeError::ResponsesNotSupported))?
    }

    /// Outbound POST timed out: refund the original sender by re-minting the burned
    /// NEX and decrementing the in-flight ledger for the destination chain.
    /// 出站 POST 超时：通过重新铸造已销毁的 NEX 并对目标链递减在途账本，退款给原发送方。
    fn on_timeout(&self, request: Request) -> Result<Weight, anyhow::Error> {
        in_transaction(move || {
            // Recompute the request commitment (the same hash the dispatcher returned)
            // so a tracked payout (HB-WD-01 mechanism 2) can be matched and compensated.
            // 重算请求 commitment（与派发器返回的同一哈希），以便匹配并补偿已跟踪派发
            //（HB-WD-01 机制 2）。
            let commitment = ismp::messaging::hash_request::<pallet_ismp::Pallet<T>>(&request);
            match request {
                Request::Post(PostRequest { body, to, dest, .. }) => {
                    let cfg = Chains::<T>::get(dest)
                        .ok_or_else(|| into_anyhow(BridgeError::UnregisteredChain(dest)))?;
                    // Sanity: the timed-out request must have targeted the registered contract.
                    // 完整性：超时的请求必须曾以已注册合约为目标。
                    if to != cfg.contract.0.to_vec() {
                        Err(into_anyhow(BridgeError::UnknownSourceContract(dest)))?
                    }

                    let message = Message::abi_decode(&body)
                        .map_err(|e| into_anyhow(BridgeError::DecodeError(e)))?;

                    // Refund recipient = the original sender encoded in `message.from`.
                    // 退款收款人 = `message.from` 中编码的原发送方。
                    let sender_bytes = message.from.as_ref();
                    let sender = Pallet::<T>::account_from_bytes(sender_bytes).map_err(|_| {
                        into_anyhow(BridgeError::InvalidSenderLength(sender_bytes.len()))
                    })?;
                    let amount = Pallet::<T>::decode_amount(message.amount, cfg.erc_decimals)
                        .map_err(into_anyhow)?;

                    Pallet::<T>::credit_inbound(&sender, dest, amount).map_err(into_anyhow)?;

                    Pallet::<T>::deposit_event(Event::<T>::BridgeRefunded {
                        beneficiary: sender,
                        dest,
                        amount,
                    });

                    // Mechanism 2: if this was a tracked payout, hand the meta back to the
                    // business layer for compensation (e.g. restore a promoter's pending).
                    // The handler runs in a nested storage layer so its failure rolls back
                    // only the business side — the re-mint above always stands.
                    // 机制 2：若为已跟踪派发，把 meta 交还业务层补偿（例如恢复推广员 pending）。
                    // handler 在嵌套存储层内执行，其失败仅回滚业务侧——上面的重铸始终保留。
                    if let Some((_dispatched_at, meta)) = PayoutRefunds::<T>::take(commitment) {
                        let handled = with_storage_layer(|| {
                            T::PayoutRefundHandler::on_payout_timeout(meta.as_slice())
                        })
                        .is_ok();
                        Pallet::<T>::deposit_event(Event::<T>::PayoutRefundNotified {
                            commitment,
                            handled,
                        });
                    }
                    Ok(<T as frame_system::Config>::DbWeight::get().reads_writes(8, 5))
                }
                Request::Get(_) => Err(into_anyhow(BridgeError::UnsupportedTimeoutType))?,
            }
        })
    }
}

impl<T: Config> Pallet<T>
where
    T::AccountId: From<[u8; 32]> + Into<[u8; 32]>,
    BalanceOf<T>: core::str::FromStr + Into<u128>,
    <T as pallet_ismp::Config>::Balance: From<BalanceOf<T>>,
{
    /// Handle an inbound [`OrderIntent`] (HB-ENT-01): mint the bridged NEX to the
    /// derived buyer (within the in-flight ledger), then dispatch the digital order
    /// in a nested storage layer. On order failure the mint is **kept** (DerivedCredit)
    /// and only the order side is rolled back, satisfying the "never burned without
    /// settlement" invariant; `on_accept` still returns `Ok` so the ISMP receipt is
    /// persisted and the request is not replayed.
    /// 处理入站 [`OrderIntent`]（HB-ENT-01）：在在途账本内向派生买家铸造已桥接 NEX，再在嵌套
    /// 存储层内派发数字下单。下单失败时**保留**铸造额（DerivedCredit），仅回滚订单侧，满足
    /// “绝不已 burn 却无结算”不变量；`on_accept` 仍返回 `Ok`，以持久化 ISMP 回执、避免重放。
    fn on_cross_order(
        source: StateMachine,
        amount: BalanceOf<T>,
        intent: OrderIntent,
    ) -> Result<Weight, anyhow::Error> {
        let buyer = <T::EvmToSubstrate as EvmToSubstrate<T>>::convert(H160(intent.buyer_evm));
        let referrer = intent
            .referrer
            .map(|e| <T::EvmToSubstrate as EvmToSubstrate<T>>::convert(H160(e)));

        // Mint NEX to the derived buyer (anti-inflation invariant enforced inside).
        // 向派生买家铸造 NEX（防增发不变量在内部强制）。
        Pallet::<T>::credit_inbound(&buyer, source, amount).map_err(into_anyhow)?;

        // The bridged amount is the buyer's budget and the slippage cap: the order
        // may charge at most what was bridged; any remainder stays as DerivedCredit.
        // 已桥接额即买家预算与滑点上限：下单最多扣已桥接额；余额留作 DerivedCredit。
        let res = with_storage_layer(|| {
            T::CrossOrderHandler::do_cross_order(
                buyer.clone(),
                buyer.clone(),
                intent.product_id,
                intent.quantity,
                amount,
                referrer.clone(),
            )
        });

        match res {
            Ok(order_id) => Pallet::<T>::deposit_event(Event::<T>::CrossOrderPlaced {
                order_id,
                buyer,
                source,
                product_id: intent.product_id,
            }),
            Err(error) => Pallet::<T>::deposit_event(Event::<T>::CrossOrderFailed {
                buyer,
                source,
                amount,
                error,
            }),
        }
        // Bridge base work (decode + mint + ledger + events) plus the order handler's
        // own worst-case weight, so pallet-ismp meters the full inbound cost.
        // 桥基础开销（解码 + 铸造 + 账本 + 事件）加上订单 handler 自身的最坏权重，使
        // pallet-ismp 计量完整入站成本。
        let base = <T as frame_system::Config>::DbWeight::get().reads_writes(6, 4);
        Ok(base.saturating_add(T::CrossOrderHandler::cross_order_weight()))
    }

    /// Handle an inbound [`WithdrawRequest`](crate::types::WithdrawRequest)
    /// (HB-ENT-01 §7, G-B4): **queue** a time-locked move of a derived account's NEX
    /// back to the source EVM chain. A derived account has no Substrate key, so the
    /// withdrawal is authorised entirely by the EVM gateway (it checks
    /// `msg.sender == owner_evm`) plus the inbound source-contract allow-list. To bound
    /// the blast radius of a compromised/buggy gateway (H2), the callback does **not**
    /// burn here: it validates + decodes the amount and records a
    /// [`PendingWithdraw`](crate::types::PendingWithdraw). The burn + outbound POST run
    /// later in [`execute_withdraw`](Pallet::execute_withdraw), no earlier than
    /// [`WithdrawDelay`](Config::WithdrawDelay) blocks, during which the guardian can
    /// [`cancel_withdraw`](Pallet::cancel_withdraw). No funds move in this callback.
    /// 处理入站 [`WithdrawRequest`](crate::types::WithdrawRequest)（HB-ENT-01 §7，G-B4）：
    /// **排队**一笔时间锁的派生账户 NEX 提回来源 EVM 链。派生账户无 Substrate 私钥，故提款完全由
    /// EVM 网关鉴权（其校验 `msg.sender == owner_evm`）加入站来源合约 allow-list。为限制网关被攻破/
    /// 有 bug 的爆炸半径（H2），回调此处**不**销毁：仅校验 + 解码金额并记录
    /// [`PendingWithdraw`](crate::types::PendingWithdraw)。销毁 + 出站 POST 在之后由
    /// [`execute_withdraw`](Pallet::execute_withdraw) 执行，最早须经
    /// [`WithdrawDelay`](Config::WithdrawDelay) 个区块，窗口内 guardian 可
    /// [`cancel_withdraw`](Pallet::cancel_withdraw)。本回调不移动任何资金。
    fn on_withdraw(
        source: StateMachine,
        req: crate::types::WithdrawRequest,
    ) -> Result<Weight, anyhow::Error> {
        // Validate the source + decode the amount up front so an unregistered chain or a
        // sub-minimum amount is rejected at queue time (cheap fail-fast).
        // 先校验来源 + 解码金额，使未注册链或低于下限的金额在排队时即被拒（廉价快速失败）。
        let cfg = Chains::<T>::get(source)
            .ok_or_else(|| into_anyhow(BridgeError::UnregisteredChain(source)))?;
        let owner = <T::EvmToSubstrate as EvmToSubstrate<T>>::convert(H160(req.owner_evm));
        let recipient = H160(req.dest_recipient);
        let amount = Pallet::<T>::decode_amount(
            alloy_primitives::U256::from(req.amount_nex),
            cfg.erc_decimals,
        )
        .map_err(into_anyhow)?;

        // Queue the time-locked withdrawal (no burn here — see doc above).
        // 排队时间锁提款（此处不销毁——见上文）。
        let (id, execute_at) =
            Pallet::<T>::queue_withdraw(owner.clone(), source, req.dest_recipient, amount);
        Pallet::<T>::deposit_event(Event::<T>::WithdrawQueued {
            id,
            owner,
            dest: source,
            recipient,
            amount,
            execute_at,
        });
        // Queue-only work: Chains read, id read+write, entry write, event.
        // 仅排队开销：Chains 读取、id 读+写、条目写入、事件。
        Ok(<T as frame_system::Config>::DbWeight::get().reads_writes(2, 2))
    }
}
