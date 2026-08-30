//! Core Provider traits: Entity, Shop, Product, Order
//!
//! These are the foundational read/write interfaces for the four core business domains.

use super::super::types::*;
use codec::{Decode, Encode, MaxEncodedLen};
use scale_info::TypeInfo;
use sp_runtime::DispatchError;

// ============================================================================
// Entity 查询接口
// ============================================================================

/// 实体查询接口
///
/// 供其他模块查询实体信息
pub trait EntityProvider<AccountId> {
    /// 检查实体是否存在
    fn entity_exists(entity_id: u64) -> bool;

    /// 检查实体是否激活
    fn is_entity_active(entity_id: u64) -> bool;

    /// 获取实体状态
    fn entity_status(entity_id: u64) -> Option<EntityStatus>;

    /// 获取实体所有者
    fn entity_owner(entity_id: u64) -> Option<AccountId>;

    /// 获取实体派生账户
    fn entity_account(entity_id: u64) -> AccountId;

    /// 获取实体类型
    fn entity_type(entity_id: u64) -> Option<EntityType> {
        let _ = entity_id;
        None
    }

    /// 更新实体统计（销售额、订单数）
    fn update_entity_stats(
        entity_id: u64,
        sales_amount: u128,
        order_count: u32,
    ) -> Result<(), DispatchError>;

    // ==================== Entity-Shop 关联接口 ====================

    /// 注册 Shop 到 Entity（Shop 创建时调用）
    fn register_shop(entity_id: u64, shop_id: u64) -> Result<(), DispatchError> {
        let _ = (entity_id, shop_id);
        Ok(())
    }

    /// 从 Entity 注销 Shop（Shop 关闭时调用）
    fn unregister_shop(entity_id: u64, shop_id: u64) -> Result<(), DispatchError> {
        let _ = (entity_id, shop_id);
        Ok(())
    }

    /// 检查是否为 Entity 管理员且拥有指定权限
    ///
    /// `required_permission` 为 `AdminPermission` 位掩码，owner 天然通过。
    fn is_entity_admin(entity_id: u64, account: &AccountId, required_permission: u32) -> bool {
        let _ = (entity_id, account, required_permission);
        false
    }

    /// 设置 Entity 的 Primary Shop ID（由 Shop pallet 调用）
    fn set_primary_shop_id(entity_id: u64, shop_id: u64) {
        let _ = (entity_id, shop_id);
    }

    /// 获取 Entity 的 Primary Shop ID（0 = 无主店）
    fn get_primary_shop_id(entity_id: u64) -> u64 {
        let _ = entity_id;
        0
    }

    /// 获取 Entity 下所有 Shop IDs
    fn entity_shops(entity_id: u64) -> sp_std::vec::Vec<u64> {
        let _ = entity_id;
        sp_std::vec::Vec::new()
    }

    // ==================== 治理调用接口 ====================

    /// 暂停实体（治理调用）
    fn pause_entity(entity_id: u64) -> Result<(), DispatchError> {
        let _ = entity_id;
        Ok(()) // 默认空实现
    }

    /// 恢复实体（治理调用）
    fn resume_entity(entity_id: u64) -> Result<(), DispatchError> {
        let _ = entity_id;
        Ok(())
    }

    /// 设置实体治理模式（治理 pallet 同步调用）
    fn set_governance_mode(entity_id: u64, mode: GovernanceMode) -> Result<(), DispatchError> {
        let _ = (entity_id, mode);
        Ok(())
    }

    /// 实体是否被全局锁定（governance lock 生效时返回 true）
    ///
    /// 锁定后所有 Owner/Admin 配置操作被拒绝，不可逆。
    fn is_entity_locked(entity_id: u64) -> bool {
        let _ = entity_id;
        false
    }

    // ==================== 所有权转移 ====================

    /// 发起所有权转移请求（owner 调用，new_owner 需 accept）
    fn initiate_ownership_transfer(
        entity_id: u64,
        new_owner: &AccountId,
    ) -> Result<(), DispatchError> {
        let _ = (entity_id, new_owner);
        Err(DispatchError::Other("not implemented"))
    }

    /// 接受所有权转移（new_owner 调用）
    fn accept_ownership_transfer(
        entity_id: u64,
        new_owner: &AccountId,
    ) -> Result<(), DispatchError> {
        let _ = (entity_id, new_owner);
        Err(DispatchError::Other("not implemented"))
    }

    /// 取消所有权转移请求（owner 调用）
    fn cancel_ownership_transfer(entity_id: u64) -> Result<(), DispatchError> {
        let _ = entity_id;
        Err(DispatchError::Other("not implemented"))
    }

    /// 获取待处理的所有权转移目标（None = 无待转移）
    fn pending_ownership_transfer(entity_id: u64) -> Option<AccountId> {
        let _ = entity_id;
        None
    }

    // ==================== P6: 元数据查询 ====================

    /// 获取实体名称（UTF-8 字节）
    fn entity_name(entity_id: u64) -> sp_std::vec::Vec<u8> {
        let _ = entity_id;
        sp_std::vec::Vec::new()
    }

    /// 获取实体元数据 IPFS CID
    fn entity_metadata_cid(entity_id: u64) -> Option<sp_std::vec::Vec<u8>> {
        let _ = entity_id;
        None
    }

    /// 获取实体描述（UTF-8 字节）
    fn entity_description(entity_id: u64) -> sp_std::vec::Vec<u8> {
        let _ = entity_id;
        sp_std::vec::Vec::new()
    }

    /// 获取实体支付通道配置
    fn payment_config(entity_id: u64) -> PaymentConfig {
        let _ = entity_id;
        PaymentConfig::default()
    }
}

/// 空实体提供者（测试用）
pub struct NullEntityProvider;

impl<AccountId: Default> EntityProvider<AccountId> for NullEntityProvider {
    fn entity_exists(_entity_id: u64) -> bool {
        false
    }
    fn is_entity_active(_entity_id: u64) -> bool {
        false
    }
    fn entity_status(_entity_id: u64) -> Option<EntityStatus> {
        None
    }
    fn entity_owner(_entity_id: u64) -> Option<AccountId> {
        None
    }
    fn entity_account(_entity_id: u64) -> AccountId {
        AccountId::default()
    }
    fn update_entity_stats(
        _entity_id: u64,
        _sales_amount: u128,
        _order_count: u32,
    ) -> Result<(), DispatchError> {
        Ok(())
    }
}

// ============================================================================
// Shop 查询接口 (Entity-Shop 分离架构)
// ============================================================================

/// Shop 查询接口
///
/// 供业务模块查询 Shop 信息（与 EntityProvider 区分）
pub trait ShopProvider<AccountId> {
    /// 检查 Shop 是否存在
    fn shop_exists(shop_id: u64) -> bool;

    /// 检查 Shop 是否营业中
    fn is_shop_active(shop_id: u64) -> bool;

    /// 获取 Shop 所属 Entity ID
    fn shop_entity_id(shop_id: u64) -> Option<u64>;

    /// 获取 Shop 所有者（通过 Entity 查询）
    fn shop_owner(shop_id: u64) -> Option<AccountId>;

    /// 获取 Shop 运营账户
    fn shop_account(shop_id: u64) -> AccountId;

    /// 获取 Shop 类型
    fn shop_type(shop_id: u64) -> Option<ShopType>;

    /// 检查是否为 Shop 管理员
    fn is_shop_manager(shop_id: u64, account: &AccountId) -> bool;

    // ==================== 统计更新 ====================

    /// 更新 Shop 统计（销售额、订单数）
    fn update_shop_stats(
        shop_id: u64,
        sales_amount: u128,
        order_count: u32,
    ) -> Result<(), DispatchError>;

    /// 更新 Shop 评分（0-100 分制，与 `ReviewProvider::shop_average_rating` 同刻度）
    fn update_shop_rating(shop_id: u64, rating: u8) -> Result<(), DispatchError>;

    /// 回退 Shop 评分（评价删除/修改时调用，减去旧评分并可选追加新评分）
    /// `old_rating`: 需要回退的旧评分 (1-5)
    /// `new_rating`: 如果是修改评价，传入新评分；如果是删除评价，传 None
    fn revert_shop_rating(
        shop_id: u64,
        old_rating: u8,
        new_rating: Option<u8>,
    ) -> Result<(), DispatchError> {
        let _ = (shop_id, old_rating, new_rating);
        Ok(())
    }

    // ==================== 商品统计 ====================

    /// 增加 Shop 商品计数（创建商品时调用）
    fn increment_product_count(shop_id: u64) -> Result<(), DispatchError> {
        let _ = shop_id;
        Ok(())
    }

    /// 减少 Shop 商品计数（删除商品时调用）
    fn decrement_product_count(shop_id: u64) -> Result<(), DispatchError> {
        let _ = shop_id;
        Ok(())
    }

    // ==================== 运营资金 ====================

    /// 扣减运营资金
    fn deduct_operating_fund(shop_id: u64, amount: u128) -> Result<(), DispatchError>;

    /// 获取运营资金余额
    fn operating_balance(shop_id: u64) -> u128;

    // ==================== Primary Shop ====================

    /// 创建主 Shop（Entity 创建时自动调用，绕过 is_entity_active 检查）
    fn create_primary_shop(
        entity_id: u64,
        name: sp_std::vec::Vec<u8>,
        shop_type: ShopType,
    ) -> Result<u64, DispatchError> {
        let _ = (entity_id, name, shop_type);
        Err(DispatchError::Other("not implemented"))
    }

    /// 检查 Shop 是否为主 Shop
    fn is_primary_shop(shop_id: u64) -> bool {
        let _ = shop_id;
        false
    }

    // ==================== 控制接口 ====================

    /// 获取 Shop 自身状态（不考虑 Entity）
    fn shop_own_status(shop_id: u64) -> Option<ShopOperatingStatus> {
        let _ = shop_id;
        None
    }

    /// 获取 Shop 有效状态（考虑 Entity 状态）
    fn effective_status(shop_id: u64) -> Option<EffectiveShopStatus> {
        let _ = shop_id;
        None
    }

    /// 暂停 Shop
    fn pause_shop(shop_id: u64) -> Result<(), DispatchError> {
        let _ = shop_id;
        Ok(())
    }

    /// 恢复 Shop
    fn resume_shop(shop_id: u64) -> Result<(), DispatchError> {
        let _ = shop_id;
        Ok(())
    }

    /// 强制关闭 Shop（Entity 级联调用，绕过 is_primary 检查）
    fn force_close_shop(shop_id: u64) -> Result<(), DispatchError> {
        let _ = shop_id;
        Ok(())
    }

    /// 强制暂停 Shop（治理层调用，可被 owner 恢复）
    fn force_pause_shop(shop_id: u64) -> Result<(), DispatchError> {
        let _ = shop_id;
        Ok(())
    }

    // ==================== #7 补充: 治理封禁接口 ====================

    /// 封禁 Shop（治理调用，不可被 owner 恢复，需通过治理解封）
    fn ban_shop(shop_id: u64) -> Result<(), DispatchError> {
        let _ = shop_id;
        Ok(())
    }

    /// 解除 Shop 封禁（治理调用）
    fn unban_shop(shop_id: u64) -> Result<(), DispatchError> {
        let _ = shop_id;
        Ok(())
    }

    // ==================== R10: 治理提案链上执行接口 ====================

    /// 关闭 Shop（治理提案执行，不可逆）
    fn governance_close_shop(shop_id: u64) -> Result<(), DispatchError> {
        let _ = shop_id;
        Ok(())
    }

    /// 变更 Shop 类型（治理提案执行）
    fn governance_set_shop_type(shop_id: u64, new_type: ShopType) -> Result<(), DispatchError> {
        let _ = (shop_id, new_type);
        Ok(())
    }

    /// Entity 金库向 Shop 划拨运营资金（治理提案执行 / Owner 直接调用）
    fn governance_allocate_from_treasury(
        entity_id: u64,
        shop_id: u64,
        amount: u128,
    ) -> Result<(), DispatchError> {
        let _ = (entity_id, shop_id, amount);
        Ok(())
    }
}

/// 空 Shop 提供者（测试用）
pub struct NullShopProvider;

impl<AccountId: Default> ShopProvider<AccountId> for NullShopProvider {
    fn shop_exists(_shop_id: u64) -> bool {
        false
    }
    fn is_shop_active(_shop_id: u64) -> bool {
        false
    }
    fn shop_entity_id(_shop_id: u64) -> Option<u64> {
        None
    }
    fn shop_owner(_shop_id: u64) -> Option<AccountId> {
        None
    }
    fn shop_account(_shop_id: u64) -> AccountId {
        AccountId::default()
    }
    fn shop_type(_shop_id: u64) -> Option<ShopType> {
        None
    }
    fn is_shop_manager(_shop_id: u64, _account: &AccountId) -> bool {
        false
    }
    fn shop_own_status(_shop_id: u64) -> Option<ShopOperatingStatus> {
        None
    }
    fn effective_status(_shop_id: u64) -> Option<EffectiveShopStatus> {
        None
    }
    fn update_shop_stats(
        _shop_id: u64,
        _sales_amount: u128,
        _order_count: u32,
    ) -> Result<(), DispatchError> {
        Ok(())
    }
    fn update_shop_rating(_shop_id: u64, _rating: u8) -> Result<(), DispatchError> {
        Ok(())
    }
    fn revert_shop_rating(
        _shop_id: u64,
        _old_rating: u8,
        _new_rating: Option<u8>,
    ) -> Result<(), DispatchError> {
        Ok(())
    }
    fn deduct_operating_fund(_shop_id: u64, _amount: u128) -> Result<(), DispatchError> {
        Ok(())
    }
    fn operating_balance(_shop_id: u64) -> u128 {
        0
    }
}

// ============================================================================
// Product 查询接口
// ============================================================================

/// 商品聚合查询信息（单次存储读取返回下单所需全部字段）
#[derive(Encode, Decode, codec::DecodeWithMemTracking, Clone, PartialEq, Eq, TypeInfo, Debug)]
pub struct ProductQueryInfo {
    pub shop_id: u64,
    pub usdt_price: u64,
    pub stock: u32,
    pub status: ProductStatus,
    pub category: ProductCategory,
    pub visibility: ProductVisibility,
    pub min_order_quantity: u32,
    pub max_order_quantity: u32,
}

/// 商品查询接口
///
/// 供 order 模块查询和更新商品信息
pub trait ProductProvider<AccountId> {
    /// 检查商品是否存在
    fn product_exists(product_id: u64) -> bool;

    /// 检查商品是否在售
    fn is_product_on_sale(product_id: u64) -> bool;

    /// 获取商品所属店铺
    fn product_shop_id(product_id: u64) -> Option<u64>;

    /// 获取商品库存
    fn product_stock(product_id: u64) -> Option<u32>;

    /// 获取商品类别
    fn product_category(product_id: u64) -> Option<ProductCategory>;

    /// 扣减库存
    fn deduct_stock(product_id: u64, quantity: u32) -> Result<(), DispatchError>;

    /// 恢复库存
    fn restore_stock(product_id: u64, quantity: u32) -> Result<(), DispatchError>;

    /// 增加销量
    fn add_sold_count(product_id: u64, quantity: u32) -> Result<(), DispatchError>;

    // ==================== 扩展查询接口 ====================

    /// 获取商品状态
    fn product_status(product_id: u64) -> Option<ProductStatus> {
        let _ = product_id;
        None
    }

    /// 获取商品 USDT 价格（精度 10^6）
    fn product_usdt_price(product_id: u64) -> Option<u64> {
        let _ = product_id;
        None
    }

    /// 获取商品所有者（通过 Shop → Owner）
    fn product_owner(product_id: u64) -> Option<AccountId> {
        let _ = product_id;
        None
    }

    /// 获取店铺下所有商品 ID
    fn shop_product_ids(shop_id: u64) -> sp_std::vec::Vec<u64> {
        let _ = shop_id;
        sp_std::vec::Vec::new()
    }

    /// 获取商品可见性
    fn product_visibility(product_id: u64) -> Option<ProductVisibility> {
        let _ = product_id;
        None
    }

    /// 获取商品最小购买数量（0 表示不限，默认 1）
    fn product_min_order_quantity(product_id: u64) -> Option<u32> {
        let _ = product_id;
        None
    }

    /// 获取商品最大购买数量（0 表示不限）
    fn product_max_order_quantity(product_id: u64) -> Option<u32> {
        let _ = product_id;
        None
    }

    /// 聚合查询：一次存储读取返回下单所需全部字段
    fn get_product_info(product_id: u64) -> Option<ProductQueryInfo> {
        let _ = product_id;
        None
    }

    // ==================== 治理调用接口 ====================

    /// 更新商品 USDT 价格（治理调用）
    fn update_usdt_price(product_id: u64, new_usdt_price: u64) -> Result<(), DispatchError> {
        let _ = (product_id, new_usdt_price);
        Ok(())
    }

    /// 级联 unpin 某 Shop 下所有 Product 的 CID（Shop 关闭/封禁时调用）。
    fn force_unpin_shop_products(shop_id: u64) -> Result<(), DispatchError> {
        let _ = shop_id;
        Ok(())
    }

    /// 强制移除某 Shop 下所有商品（Shop 关闭时调用）：
    /// 删除全部商品存储、退还押金、unpin CID、更新统计。
    fn force_remove_all_shop_products(shop_id: u64) -> Result<(), DispatchError> {
        let _ = shop_id;
        Ok(())
    }

    /// 强制下架某 Shop 下所有在售/售罄商品（Shop 封禁时调用）：
    /// OnSale/SoldOut → OffShelf，更新统计。
    fn force_delist_all_shop_products(shop_id: u64) -> Result<(), DispatchError> {
        let _ = shop_id;
        Ok(())
    }

    /// 下架商品（治理调用）
    fn delist_product(product_id: u64) -> Result<(), DispatchError> {
        let _ = product_id;
        Ok(())
    }

    /// 调整库存（治理调用）
    fn set_inventory(product_id: u64, new_inventory: u32) -> Result<(), DispatchError> {
        let _ = (product_id, new_inventory);
        Ok(())
    }

    /// 设置商品可见性（治理提案执行）
    fn governance_set_visibility(
        product_id: u64,
        visibility: ProductVisibility,
    ) -> Result<(), DispatchError> {
        let _ = (product_id, visibility);
        Err(DispatchError::Other("not implemented"))
    }
}

/// 空商品提供者（测试用）
pub struct NullProductProvider;

impl<AccountId> ProductProvider<AccountId> for NullProductProvider {
    fn product_exists(_product_id: u64) -> bool {
        false
    }
    fn is_product_on_sale(_product_id: u64) -> bool {
        false
    }
    fn product_shop_id(_product_id: u64) -> Option<u64> {
        None
    }
    fn product_stock(_product_id: u64) -> Option<u32> {
        None
    }
    fn product_category(_product_id: u64) -> Option<ProductCategory> {
        None
    }
    fn deduct_stock(_product_id: u64, _quantity: u32) -> Result<(), DispatchError> {
        Ok(())
    }
    fn restore_stock(_product_id: u64, _quantity: u32) -> Result<(), DispatchError> {
        Ok(())
    }
    fn add_sold_count(_product_id: u64, _quantity: u32) -> Result<(), DispatchError> {
        Ok(())
    }
}

// ============================================================================
// PaymentAsset — 支付资产类型
// ============================================================================

/// 订单支付资产类型
#[derive(
    Encode,
    Decode,
    codec::DecodeWithMemTracking,
    Clone,
    Copy,
    PartialEq,
    Eq,
    TypeInfo,
    MaxEncodedLen,
    Debug,
    Default,
)]
pub enum PaymentAsset {
    /// NEX 原生代币支付
    #[default]
    Native,
    /// Entity Token 支付
    EntityToken,
    /// 购物余额全额支付
    ShoppingBalance,
}

// ============================================================================
// Order 查询接口
// ============================================================================

/// 订单聚合查询信息（一次 storage read 获取常用字段）
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct OrderQueryInfo<AccountId, Balance> {
    pub order_id: u64,
    pub entity_id: u64,
    pub shop_id: u64,
    pub product_id: u64,
    pub buyer: AccountId,
    pub seller: AccountId,
    pub quantity: u32,
    pub total_amount: Balance,
    pub token_payment_amount: u128,
    pub payment_asset: PaymentAsset,
    pub status: OrderStatus,
    pub product_category: ProductCategory,
    pub created_at: u64,
    pub shipped_at: Option<u64>,
    pub completed_at: Option<u64>,
}

pub trait OrderProvider<AccountId, Balance> {
    /// 检查订单是否存在
    fn order_exists(order_id: u64) -> bool;

    /// 获取订单买家
    fn order_buyer(order_id: u64) -> Option<AccountId>;

    /// 获取订单卖家
    fn order_seller(order_id: u64) -> Option<AccountId>;

    /// 获取订单总金额
    fn order_amount(order_id: u64) -> Option<Balance>;

    /// 获取订单店铺
    fn order_shop_id(order_id: u64) -> Option<u64>;

    /// 检查订单是否已完成
    fn is_order_completed(order_id: u64) -> bool;

    /// 检查订单是否处于争议状态
    fn is_order_disputed(order_id: u64) -> bool;

    /// 检查用户是否可以对该订单发起争议（必须是买家或卖家，且订单状态允许）
    fn can_dispute(order_id: u64, who: &AccountId) -> bool;

    /// 获取订单 Token 支付金额（u128，Token 订单返回实际值，NEX 订单返回 0）
    fn order_token_amount(order_id: u64) -> Option<u128> {
        let _ = order_id;
        None
    }

    /// 获取订单支付资产类型（Native / EntityToken）
    fn order_payment_asset(order_id: u64) -> Option<PaymentAsset> {
        let _ = order_id;
        None
    }

    /// 获取订单完成时间（区块号，u64 表示）
    fn order_completed_at(order_id: u64) -> Option<u64> {
        let _ = order_id;
        None
    }

    // ==================== #1 补充: 关键查询方法 ====================

    /// 获取订单状态
    fn order_status(order_id: u64) -> Option<OrderStatus> {
        let _ = order_id;
        None
    }

    /// 获取订单所属 Entity ID（通过 shop_id 间接获取或直接存储）
    fn order_entity_id(order_id: u64) -> Option<u64> {
        let _ = order_id;
        None
    }

    /// 获取订单商品 ID
    fn order_product_id(order_id: u64) -> Option<u64> {
        let _ = order_id;
        None
    }

    /// 获取订单购买数量
    fn order_quantity(order_id: u64) -> Option<u32> {
        let _ = order_id;
        None
    }

    // ==================== P3: 时间戳查询补全 ====================

    /// 获取订单创建时间（区块号）
    fn order_created_at(order_id: u64) -> Option<u64> {
        let _ = order_id;
        None
    }

    /// 获取订单支付时间（区块号）
    fn order_paid_at(order_id: u64) -> Option<u64> {
        let _ = order_id;
        None
    }

    /// 获取订单发货时间（区块号）
    fn order_shipped_at(order_id: u64) -> Option<u64> {
        let _ = order_id;
        None
    }

    /// 聚合查询：一次 storage read 获取订单常用字段（替代多次独立查询）
    fn get_order_info(order_id: u64) -> Option<OrderQueryInfo<AccountId, Balance>> {
        let _ = order_id;
        None
    }

    /// 检查指定 Shop 是否存在活跃（未终结）订单
    fn has_active_orders_for_shop(shop_id: u64) -> bool {
        let _ = shop_id;
        false
    }

    // ==================== Phase 1 新增: 代付查询 ====================

    /// 获取订单代付方（无代付则返回 None）
    fn order_payer(order_id: u64) -> Option<AccountId> {
        let _ = order_id;
        None
    }

    /// 获取订单资金方（有代付返回 payer，否则返回 buyer）
    fn order_fund_account(order_id: u64) -> Option<AccountId> {
        let _ = order_id;
        None
    }
}

/// 空订单提供者（测试用）
pub struct NullOrderProvider;

impl<AccountId, Balance> OrderProvider<AccountId, Balance> for NullOrderProvider {
    fn order_exists(_order_id: u64) -> bool {
        false
    }
    fn order_buyer(_order_id: u64) -> Option<AccountId> {
        None
    }
    fn order_seller(_order_id: u64) -> Option<AccountId> {
        None
    }
    fn order_amount(_order_id: u64) -> Option<Balance> {
        None
    }
    fn order_shop_id(_order_id: u64) -> Option<u64> {
        None
    }
    fn is_order_completed(_order_id: u64) -> bool {
        false
    }
    fn is_order_disputed(_order_id: u64) -> bool {
        false
    }
    fn can_dispute(_order_id: u64, _who: &AccountId) -> bool {
        false
    }
    fn order_token_amount(_order_id: u64) -> Option<u128> {
        None
    }
    fn order_payment_asset(_order_id: u64) -> Option<PaymentAsset> {
        None
    }
}
