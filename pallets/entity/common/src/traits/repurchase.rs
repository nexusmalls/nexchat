//! AutoRepurchasePort — auto-repurchase order creation and product validation interface.
//! AutoRepurchasePort — 自动复购订单创建与商品归属校验接口。
//!
//! Implemented by the order pallet via a runtime bridge, injected into commission/core Config.
//! 由 order pallet 在 runtime 中通过 bridge 实现，注入 commission/core Config。
//!
//! Decoupling: commission/core depends on this trait (defined in common),
//! order pallet implements it — no direct dependency, no circular imports.
//! 解耦方向：commission/core 依赖此 trait（定义在 common），order pallet 实现此 trait，
//! 两者不直接依赖，避免循环依赖。

use sp_runtime::DispatchError;

/// Auto-repurchase order creation and product validation interface.
/// 自动复购订单创建与商品归属校验接口。
///
/// Implementation constraints (guaranteed by bridge):
/// 实现约束（由 bridge 保证）：
/// - Uses `PaymentAsset::ShoppingBalance` channel / 使用 `PaymentAsset::ShoppingBalance` 通道
/// - quantity fixed at 1 / quantity 固定为 1
/// - No referrer passed (uses account's existing referral) / 不传入 referrer（使用账户已绑定的推荐关系）
/// - No shipping_cid / note_cid / slippage params / 不传入 shipping_cid / note_cid / slippage 参数
///
/// On failure returns `Err`; caller must degrade to emitting event, never panic or unwrap.
/// 失败时返回 `Err`，调用方须降级为发事件，不得 panic 或 unwrap。
pub trait AutoRepurchasePort<AccountId> {
    /// Place a repurchase order using shopping balance for the given account.
    /// 用购物余额为指定账户创建复购订单。
    ///
    /// # Returns / 返回
    /// - `Ok(order_id)` — order created successfully / 订单创建成功
    /// - `Err(_)` — precondition failed (product delisted, out of stock, stale price, etc.) / 前置条件失败
    fn try_place_repurchase_order(
        entity_id: u64,
        buyer: &AccountId,
        product_id: u64,
    ) -> Result<u64, DispatchError>;

    /// Validate that `product_id` belongs to the given `entity_id`.
    /// 校验 `product_id` 是否归属于指定 `entity_id`。
    ///
    /// Called during `set_repurchase_config` when `auto_order=true` to prevent
    /// cross-entity product reference attacks.
    /// 在 `set_repurchase_config` 中 `auto_order=true` 时调用，
    /// 防止跨 Entity 商品引用攻击。
    fn validate_repurchase_product(entity_id: u64, product_id: u64) -> Result<(), DispatchError>;
}

/// Null implementation — used in test environments or when auto-repurchase is not configured.
/// 空实现 — 测试环境或未配置自动复购时使用。
pub struct NullAutoRepurchasePort;

impl<AccountId> AutoRepurchasePort<AccountId> for NullAutoRepurchasePort {
    fn try_place_repurchase_order(
        _entity_id: u64,
        _buyer: &AccountId,
        _product_id: u64,
    ) -> Result<u64, DispatchError> {
        Err(DispatchError::Other("auto repurchase not configured"))
    }

    fn validate_repurchase_product(_entity_id: u64, _product_id: u64) -> Result<(), DispatchError> {
        Ok(())
    }
}
