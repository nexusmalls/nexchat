//! EN: Thin reserve/unreserve helpers for chat pallets that gate registrations with
//! an anti-spam deposit (inbox, msg-identity, sync anchor, group key package / creation).
//! CN: 聊天 pallet 共享的薄 reserve/unreserve 辅助函数，用于以反垃圾押金门控注册
//! （inbox、msg-identity、sync 锚、group KeyPackage / 建群）。

use frame_support::traits::ReservableCurrency;
use sp_runtime::DispatchResult;

/// EN: Reserve `amount` from `who` via pallet `Currency`.
/// CN: 经 pallet `Currency` 从 `who` 预留 `amount`。
pub fn reserve_deposit<C, A, Balance>(who: &A, amount: Balance) -> DispatchResult
where
    C: ReservableCurrency<A, Balance = Balance>,
{
    C::reserve(who, amount)?;
    Ok(())
}

/// EN: Unreserve `amount` back to `who` (leftover ignored).
/// CN: 向 `who` 退还预留的 `amount`（忽略 leftover）。
pub fn unreserve_deposit<C, A, Balance>(who: &A, amount: Balance)
where
    C: ReservableCurrency<A, Balance = Balance>,
{
    let _ = C::unreserve(who, amount);
}
