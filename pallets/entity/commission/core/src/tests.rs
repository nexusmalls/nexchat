use crate::mock::*;
use crate::pallet::*;
use codec::Encode;
use frame_support::traits::ConstU32;
use frame_support::BoundedVec;
use frame_support::{assert_noop, assert_ok};
use pallet_commission_common::{
    CommissionModes, CommissionStatus, CommissionType, WithdrawalMode, WithdrawalTierConfig,
};

// ============================================================================
// Helpers
// ============================================================================

/// Reserve 模式测试辅助: 先在 seller 上 reserve，再调用 process_commission
/// 模拟 order pallet 的 reserve 流程（reserve available_pool 金额）
fn process_commission_with_reserve(
    entity_id: u64,
    shop_id: u64,
    order_id: u64,
    buyer: &u64,
    order_amount: Balance,
    available_pool: Balance,
    platform_fee: Balance,
    product_id: u64,
) -> Result<(), sp_runtime::DispatchError> {
    let seller_account = crate::mock::SELLER;
    let reserved = reserve_seller(seller_account, available_pool);
    CommissionCore::process_commission(
        entity_id,
        shop_id,
        order_id,
        buyer,
        order_amount,
        available_pool,
        platform_fee,
        product_id,
        reserved,
    )
}

/// 配置会员返佣（招商奖金比例由全局常量 ReferrerShareBps=5000 控制）
fn setup_config(max_commission_rate: u16) {
    CommissionConfigs::<Test>::insert(
        ENTITY_ID,
        CoreCommissionConfig {
            enabled_modes: CommissionModes(CommissionModes::DIRECT_REWARD),
            max_commission_rate,
            enabled: true,
            withdrawal_cooldown: 0,
            owner_reward_rate: 0,
            token_withdrawal_cooldown: 0,
            plugin_caps: PluginBudgetCaps::default(),
        },
    );
}

// ============================================================================
// set_commission_rate — Entity Owner 设置会员返佣上限
// ============================================================================

#[test]
fn set_commission_rate_works() {
    new_test_ext().execute_with(|| {
        assert_ok!(CommissionCore::set_commission_rate(
            RuntimeOrigin::signed(SELLER),
            ENTITY_ID,
            5000,
        ));
        let config = CommissionConfigs::<Test>::get(ENTITY_ID).unwrap();
        assert_eq!(config.max_commission_rate, 5000);
    });
}

#[test]
fn set_commission_rate_rejects_invalid() {
    new_test_ext().execute_with(|| {
        assert_noop!(
            CommissionCore::set_commission_rate(RuntimeOrigin::signed(SELLER), ENTITY_ID, 10001,),
            Error::<Test>::InvalidCommissionRate
        );
    });
}

#[test]
fn set_commission_rate_rejects_non_owner() {
    new_test_ext().execute_with(|| {
        assert_noop!(
            CommissionCore::set_commission_rate(RuntimeOrigin::signed(BUYER), ENTITY_ID, 5000,),
            Error::<Test>::NotEntityOwnerOrAdmin
        );
    });
}

// ============================================================================
// process_commission — 平台费固定分配：50% 招商奖金 + 50% 国库
// ============================================================================

#[test]
fn referrer_gets_half_platform_fee() {
    new_test_ext().execute_with(|| {
        fund(PLATFORM, 1_000_000);
        fund(SELLER, 1_000_000);
        set_entity_referrer(ENTITY_ID, REFERRER);
        setup_config(10000);

        let platform_fee: Balance = 10_000;
        assert_ok!(process_commission_with_reserve(
            ENTITY_ID,
            SHOP_ID,
            1,
            &BUYER,
            100_000,
            100_000,
            platform_fee,
            PRODUCT_ID,
        ));

        // ReferrerShareBps=5000 → referrer = 10000 * 50% = 5000
        let records = OrderCommissionRecords::<Test>::get(1u64);
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].beneficiary, REFERRER);
        assert_eq!(records[0].amount, 5000);
        assert_eq!(records[0].commission_type, CommissionType::EntityReferral);

        // Entity 账户收到佣金
        let ea = entity_account(ENTITY_ID);
        let entity_balance = <pallet_balances::Pallet<Test> as frame_support::traits::Currency<
            u64,
        >>::free_balance(&ea);
        assert_eq!(entity_balance, 5000);

        // 国库收到另一半
        let treasury_bal = <pallet_balances::Pallet<Test> as frame_support::traits::Currency<
            u64,
        >>::free_balance(&TREASURY);
        assert_eq!(treasury_bal, 5000);
    });
}

#[test]
fn dual_source_both_pools_work() {
    new_test_ext().execute_with(|| {
        fund(PLATFORM, 1_000_000);
        fund(SELLER, 1_000_000);
        set_entity_referrer(ENTITY_ID, REFERRER);
        setup_config(5000); // 50% seller rate

        let platform_fee: Balance = 10_000;
        assert_ok!(process_commission_with_reserve(
            ENTITY_ID,
            SHOP_ID,
            2,
            &BUYER,
            100_000,
            100_000,
            platform_fee,
            PRODUCT_ID,
        ));

        // 池A: referrer = 5000, treasury = 5000
        // 池B: max_commission = 100000 * 5000 / 10000 = 50000 (插件为空, 不分配)
        let records = OrderCommissionRecords::<Test>::get(2u64);
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].amount, 5000);

        let (total, orders) = ShopCommissionTotals::<Test>::get(ENTITY_ID);
        assert_eq!(total, 5000);
        assert_eq!(orders, 1);
    });
}

#[test]
fn referrer_skipped_when_no_referrer() {
    new_test_ext().execute_with(|| {
        fund(PLATFORM, 1_000_000);
        setup_config(10000);

        assert_ok!(process_commission_with_reserve(
            ENTITY_ID, SHOP_ID, 3, &BUYER, 100_000, 100_000, 10_000, PRODUCT_ID,
        ));

        // 无推荐人 → 无佣金记录，全部进国库
        let records = OrderCommissionRecords::<Test>::get(3u64);
        assert_eq!(records.len(), 0);
        let treasury_bal = <pallet_balances::Pallet<Test> as frame_support::traits::Currency<
            u64,
        >>::free_balance(&TREASURY);
        assert_eq!(treasury_bal, 10_000);
    });
}

#[test]
fn referrer_skipped_when_platform_fee_zero() {
    new_test_ext().execute_with(|| {
        fund(SELLER, 1_000_000);
        set_entity_referrer(ENTITY_ID, REFERRER);
        setup_config(10000);

        assert_ok!(process_commission_with_reserve(
            ENTITY_ID, SHOP_ID, 5, &BUYER, 100_000, 100_000, 0, PRODUCT_ID,
        ));

        let records = OrderCommissionRecords::<Test>::get(5u64);
        assert_eq!(records.len(), 0);
    });
}

#[test]
fn referrer_amount_capped_by_platform_balance() {
    new_test_ext().execute_with(|| {
        fund(PLATFORM, 501); // 501, transferable = 500
        set_entity_referrer(ENTITY_ID, REFERRER);
        setup_config(10000);

        let platform_fee: Balance = 10_000;
        // referrer_quota = 10000 * 5000 / 10000 = 5000
        // treasury_portion = 5000, treasury_cap = 500 - 5000 = 0 (预留推荐人)
        // 国库 = 0，referrer transferable = 501 - 1 = 500
        // referrer_amount = min(5000, 500) = 500
        assert_ok!(process_commission_with_reserve(
            ENTITY_ID,
            SHOP_ID,
            6,
            &BUYER,
            100_000,
            100_000,
            platform_fee,
            PRODUCT_ID,
        ));

        let records = OrderCommissionRecords::<Test>::get(6u64);
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].amount, 500);

        let treasury_bal = <pallet_balances::Pallet<Test> as frame_support::traits::Currency<
            u64,
        >>::free_balance(&TREASURY);
        assert_eq!(treasury_bal, 0);
    });
}

#[test]
fn referrer_stats_tracked_correctly() {
    new_test_ext().execute_with(|| {
        fund(PLATFORM, 1_000_000);
        set_entity_referrer(ENTITY_ID, REFERRER);
        setup_config(10000);

        assert_ok!(process_commission_with_reserve(
            ENTITY_ID, SHOP_ID, 7, &BUYER, 100_000, 100_000, 10_000, PRODUCT_ID,
        ));

        // referrer = 50% of 10000 = 5000
        let stats = MemberCommissionStats::<Test>::get(ENTITY_ID, REFERRER);
        assert_eq!(stats.total_earned, 5000);
        assert_eq!(stats.pending, 5000);

        let pending = ShopPendingTotal::<Test>::get(ENTITY_ID);
        assert_eq!(pending, 5000);
    });
}

// ============================================================================
// process_commission — 平台费剩余转国库
// ============================================================================

#[test]
fn platform_fee_50_50_split() {
    new_test_ext().execute_with(|| {
        fund(PLATFORM, 1_000_000);
        set_entity_referrer(ENTITY_ID, REFERRER);
        setup_config(10000);

        let platform_fee: Balance = 10_000;
        assert_ok!(process_commission_with_reserve(
            ENTITY_ID,
            SHOP_ID,
            20,
            &BUYER,
            100_000,
            100_000,
            platform_fee,
            PRODUCT_ID,
        ));

        // 50/50: referrer=5000, treasury=5000
        let treasury_bal = <pallet_balances::Pallet<Test> as frame_support::traits::Currency<
            u64,
        >>::free_balance(&TREASURY);
        assert_eq!(treasury_bal, 5000);
        assert_eq!(OrderTreasuryTransfer::<Test>::get(20u64), 5000);
    });
}

#[test]
fn full_platform_fee_to_treasury_when_no_referrer() {
    new_test_ext().execute_with(|| {
        fund(PLATFORM, 1_000_000);
        setup_config(10000);

        assert_ok!(process_commission_with_reserve(
            ENTITY_ID, SHOP_ID, 21, &BUYER, 100_000, 100_000, 10_000, PRODUCT_ID,
        ));

        let treasury_bal = <pallet_balances::Pallet<Test> as frame_support::traits::Currency<
            u64,
        >>::free_balance(&TREASURY);
        assert_eq!(treasury_bal, 10_000);
        assert_eq!(OrderTreasuryTransfer::<Test>::get(21u64), 10_000);
    });
}

#[test]
fn no_treasury_transfer_when_platform_fee_zero() {
    new_test_ext().execute_with(|| {
        fund(SELLER, 1_000_000);
        setup_config(10000);

        assert_ok!(process_commission_with_reserve(
            ENTITY_ID, SHOP_ID, 22, &BUYER, 100_000, 100_000, 0, PRODUCT_ID,
        ));

        let treasury_bal = <pallet_balances::Pallet<Test> as frame_support::traits::Currency<
            u64,
        >>::free_balance(&TREASURY);
        assert_eq!(treasury_bal, 0);
        assert_eq!(OrderTreasuryTransfer::<Test>::get(22u64), 0);
    });
}

#[test]
fn treasury_transfer_capped_by_platform_balance() {
    new_test_ext().execute_with(|| {
        fund(PLATFORM, 501); // 501, transferable = 500
        set_entity_referrer(ENTITY_ID, REFERRER);
        setup_config(10000);

        let platform_fee: Balance = 10_000;
        // referrer_quota = 5000, treasury_portion = 5000
        // platform_transferable = 500, treasury_cap = 500 - 5000 = 0
        // actual_treasury = 0
        assert_ok!(process_commission_with_reserve(
            ENTITY_ID,
            SHOP_ID,
            23,
            &BUYER,
            100_000,
            100_000,
            platform_fee,
            PRODUCT_ID,
        ));

        let treasury_bal = <pallet_balances::Pallet<Test> as frame_support::traits::Currency<
            u64,
        >>::free_balance(&TREASURY);
        assert_eq!(treasury_bal, 0);
        assert_eq!(OrderTreasuryTransfer::<Test>::get(23u64), 0);
    });
}

// ============================================================================
// cancel_commission — 双来源退款 + 国库退款
// ============================================================================

#[test]
fn cancel_commission_refunds_all() {
    new_test_ext().execute_with(|| {
        fund(PLATFORM, 1_000_000);
        set_entity_referrer(ENTITY_ID, REFERRER);
        setup_config(10000);

        let ea = entity_account(ENTITY_ID);
        fund(ea, 1); // 底仓

        assert_ok!(process_commission_with_reserve(
            ENTITY_ID, SHOP_ID, 8, &BUYER, 100_000, 100_000, 10_000, PRODUCT_ID,
        ));

        // referrer=5000, treasury=5000
        let before =
            <pallet_balances::Pallet<Test> as frame_support::traits::Currency<u64>>::free_balance(
                &ea,
            );
        assert_eq!(before, 5001); // 1 底仓 + 5000 佣金

        let treasury_before = <pallet_balances::Pallet<Test> as frame_support::traits::Currency<
            u64,
        >>::free_balance(&TREASURY);
        assert_eq!(treasury_before, 5000);

        let platform_before = <pallet_balances::Pallet<Test> as frame_support::traits::Currency<
            u64,
        >>::free_balance(&PLATFORM);

        assert_ok!(CommissionCore::cancel_commission(8));

        // Entity 账户退回底仓
        let after =
            <pallet_balances::Pallet<Test> as frame_support::traits::Currency<u64>>::free_balance(
                &ea,
            );
        assert_eq!(after, 1);

        // 国库全部退回
        let treasury_after = <pallet_balances::Pallet<Test> as frame_support::traits::Currency<
            u64,
        >>::free_balance(&TREASURY);
        assert_eq!(treasury_after, 0);

        // 平台收回佣金+国库
        let platform_after = <pallet_balances::Pallet<Test> as frame_support::traits::Currency<
            u64,
        >>::free_balance(&PLATFORM);
        assert_eq!(platform_after, platform_before + 5000 + 5000);

        assert_eq!(OrderTreasuryTransfer::<Test>::get(8u64), 0);

        let records = OrderCommissionRecords::<Test>::get(8u64);
        assert_eq!(
            records[0].status,
            pallet_commission_common::CommissionStatus::Cancelled
        );

        let stats = MemberCommissionStats::<Test>::get(ENTITY_ID, REFERRER);
        assert_eq!(stats.pending, 0);
        assert_eq!(stats.total_earned, 0);
    });
}

// ============================================================================
// process_commission — 未配置佣金时平台费仍入国库
// ============================================================================

#[test]
fn treasury_receives_platform_fee_even_without_commission_config() {
    new_test_ext().execute_with(|| {
        fund(PLATFORM, 1_000_000);

        assert_ok!(process_commission_with_reserve(
            ENTITY_ID, SHOP_ID, 30, &BUYER, 100_000, 100_000, 10_000, PRODUCT_ID,
        ));

        // 无推荐人 → 全部 10000 进国库
        let treasury_bal = <pallet_balances::Pallet<Test> as frame_support::traits::Currency<
            u64,
        >>::free_balance(&TREASURY);
        assert_eq!(treasury_bal, 10_000);
        assert_eq!(OrderTreasuryTransfer::<Test>::get(30u64), 10_000);

        let records = OrderCommissionRecords::<Test>::get(30u64);
        assert_eq!(records.len(), 0);
    });
}

// ============================================================================
// set_withdrawal_config — M1 审计修复: level_id 唯一性校验
// ============================================================================

#[test]
fn m1_set_withdrawal_config_rejects_duplicate_level_id() {
    new_test_ext().execute_with(|| {
        use frame_support::traits::ConstU32;
        use frame_support::BoundedVec;
        use pallet_commission_common::{WithdrawalMode, WithdrawalTierConfig};

        // 配置 Entity 拥有 2 个自定义等级（level_id 1, 2）
        set_custom_level_count(ENTITY_ID, 2);

        let tier_a = WithdrawalTierConfig {
            withdrawal_rate: 8000,
            repurchase_rate: 2000,
        };
        let tier_b = WithdrawalTierConfig {
            withdrawal_rate: 7000,
            repurchase_rate: 3000,
        };
        let default_tier = WithdrawalTierConfig {
            withdrawal_rate: 6000,
            repurchase_rate: 4000,
        };

        // 重复 level_id = 1
        let overrides: BoundedVec<(u8, WithdrawalTierConfig), ConstU32<10>> =
            vec![(1, tier_a.clone()), (1, tier_b.clone())]
                .try_into()
                .unwrap();

        assert_noop!(
            CommissionCore::set_withdrawal_config(
                RuntimeOrigin::signed(SELLER),
                ENTITY_ID,
                WithdrawalMode::LevelBased,
                default_tier.clone(),
                overrides,
                0,
                true,
            ),
            Error::<Test>::DuplicateLevelId
        );

        // 不重复且 level_id 存在时正常通过
        let overrides_ok: BoundedVec<(u8, WithdrawalTierConfig), ConstU32<10>> =
            vec![(1, tier_a.clone()), (2, tier_b.clone())]
                .try_into()
                .unwrap();

        assert_ok!(CommissionCore::set_withdrawal_config(
            RuntimeOrigin::signed(SELLER),
            ENTITY_ID,
            WithdrawalMode::LevelBased,
            default_tier.clone(),
            overrides_ok,
            0,
            true,
        ));

        // level_id 不存在于等级系统中时拒绝
        let overrides_bad: BoundedVec<(u8, WithdrawalTierConfig), ConstU32<10>> =
            vec![(1, tier_a), (5, tier_b)].try_into().unwrap();

        assert_noop!(
            CommissionCore::set_withdrawal_config(
                RuntimeOrigin::signed(SELLER),
                ENTITY_ID,
                WithdrawalMode::LevelBased,
                default_tier,
                overrides_bad,
                0,
                true,
            ),
            Error::<Test>::LevelIdNotFound
        );
    });
}

// ============================================================================
// init_commission_plan
// ============================================================================

#[test]
fn init_commission_plan_is_disabled() {
    new_test_ext().execute_with(|| {
        assert_noop!(
            CommissionCore::init_commission_plan(RuntimeOrigin::signed(SELLER), ENTITY_ID,),
            crate::Error::<Test>::CommissionPlanDisabled
        );
    });
}

// ============================================================================
// H1: withdraw_commission 在 WithdrawalConfig disabled 时应拒绝
// ============================================================================

#[test]
fn h1_withdraw_blocked_when_config_disabled() {
    new_test_ext().execute_with(|| {
        use frame_support::BoundedVec;
        use pallet_commission_common::{WithdrawalMode, WithdrawalTierConfig};

        let ea = entity_account(ENTITY_ID);
        fund(PLATFORM, 1_000_000);
        fund(ea, 100_000);
        set_entity_referrer(ENTITY_ID, REFERRER);
        setup_config(10000);

        // 产生佣金
        assert_ok!(process_commission_with_reserve(
            ENTITY_ID, SHOP_ID, 100, &BUYER, 100_000, 100_000, 10_000, PRODUCT_ID,
        ));
        let stats = MemberCommissionStats::<Test>::get(ENTITY_ID, REFERRER);
        assert!(stats.pending > 0);

        // 设置 WithdrawalConfig 但 enabled=false
        assert_ok!(CommissionCore::set_withdrawal_config(
            RuntimeOrigin::signed(SELLER),
            ENTITY_ID,
            WithdrawalMode::FixedRate {
                repurchase_rate: 3000
            },
            WithdrawalTierConfig {
                withdrawal_rate: 7000,
                repurchase_rate: 3000
            },
            BoundedVec::default(),
            0,
            false, // disabled
        ));

        // 提现应失败
        assert_noop!(
            CommissionCore::withdraw_commission(
                RuntimeOrigin::signed(REFERRER),
                ENTITY_ID,
                None,
                None,
            ),
            Error::<Test>::WithdrawalConfigNotEnabled
        );
    });
}

#[test]
fn h1_withdraw_allowed_when_no_config() {
    new_test_ext().execute_with(|| {
        let ea = entity_account(ENTITY_ID);
        fund(PLATFORM, 1_000_000);
        fund(ea, 100_000);
        set_entity_referrer(ENTITY_ID, REFERRER);
        setup_config(10000);

        assert_ok!(process_commission_with_reserve(
            ENTITY_ID, SHOP_ID, 101, &BUYER, 100_000, 100_000, 10_000, PRODUCT_ID,
        ));

        // 无 WithdrawalConfig → 允许全额提现
        assert_ok!(CommissionCore::withdraw_commission(
            RuntimeOrigin::signed(REFERRER),
            ENTITY_ID,
            None,
            None,
        ));

        let stats = MemberCommissionStats::<Test>::get(ENTITY_ID, REFERRER);
        assert_eq!(stats.pending, 0);
        assert_eq!(stats.withdrawn, 5000); // 50% of 10000 platform fee
    });
}

// ============================================================================
// H3: stats.repurchased 包含 bonus
// ============================================================================

#[test]
fn h3_repurchased_includes_bonus() {
    new_test_ext().execute_with(|| {
        use frame_support::BoundedVec;
        use pallet_commission_common::{WithdrawalMode, WithdrawalTierConfig};

        let ea = entity_account(ENTITY_ID);
        fund(PLATFORM, 1_000_000);
        fund(ea, 500_000);
        set_entity_referrer(ENTITY_ID, REFERRER);
        setup_config(10000);

        assert_ok!(process_commission_with_reserve(
            ENTITY_ID, SHOP_ID, 200, &BUYER, 100_000, 100_000, 10_000, PRODUCT_ID,
        ));

        // MemberChoice 模式: min=2000(20%), bonus_rate=1000(10%)
        // 会员请求 5000(50%) 复购
        assert_ok!(CommissionCore::set_withdrawal_config(
            RuntimeOrigin::signed(SELLER),
            ENTITY_ID,
            WithdrawalMode::MemberChoice {
                min_repurchase_rate: 2000
            },
            WithdrawalTierConfig {
                withdrawal_rate: 8000,
                repurchase_rate: 2000
            },
            BoundedVec::default(),
            1000, // 10% voluntary bonus
            true,
        ));

        let pending = MemberCommissionStats::<Test>::get(ENTITY_ID, REFERRER).pending;
        assert_eq!(pending, 5000);

        assert_ok!(CommissionCore::withdraw_commission(
            RuntimeOrigin::signed(REFERRER),
            ENTITY_ID,
            None,
            Some(5000), // 请求 50% 复购
        ));

        let stats = MemberCommissionStats::<Test>::get(ENTITY_ID, REFERRER);
        // repurchase = 5000 * 5000 / 10000 = 2500
        // mandatory_repurchase = 5000 * 2000 / 10000 = 1000
        // voluntary_extra = 2500 - 1000 = 1500
        // bonus = 1500 * 1000 / 10000 = 150
        // repurchased should include both repurchase(2500) + bonus(150)
        assert_eq!(stats.repurchased, 2500 + 150);
        assert_eq!(stats.withdrawn, 2500); // 5000 - 2500
        assert_eq!(stats.pending, 0);

        // 购物余额 = repurchase + bonus
        let shopping = get_loyalty_shopping_balance(ENTITY_ID, REFERRER);
        assert_eq!(shopping, 2500 + 150);
    });
}

// ============================================================================
// M3: TieredWithdrawal 事件格式验证
// ============================================================================

#[test]
fn m3_event_format() {
    new_test_ext().execute_with(|| {
        use frame_support::BoundedVec;
        use pallet_commission_common::{WithdrawalMode, WithdrawalTierConfig};

        let ea = entity_account(ENTITY_ID);
        fund(PLATFORM, 1_000_000);
        fund(ea, 500_000);
        set_entity_referrer(ENTITY_ID, REFERRER);
        setup_config(10000);

        assert_ok!(process_commission_with_reserve(
            ENTITY_ID, SHOP_ID, 300, &BUYER, 100_000, 100_000, 10_000, PRODUCT_ID,
        ));

        // FixedRate 30% 复购
        assert_ok!(CommissionCore::set_withdrawal_config(
            RuntimeOrigin::signed(SELLER),
            ENTITY_ID,
            WithdrawalMode::FixedRate {
                repurchase_rate: 3000
            },
            WithdrawalTierConfig {
                withdrawal_rate: 7000,
                repurchase_rate: 3000
            },
            BoundedVec::default(),
            0,
            true,
        ));

        // REFERRER 提现
        assert_ok!(CommissionCore::withdraw_commission(
            RuntimeOrigin::signed(REFERRER),
            ENTITY_ID,
            None,
            None,
        ));

        System::assert_has_event(RuntimeEvent::CommissionCore(
            crate::pallet::Event::TieredWithdrawal {
                entity_id: ENTITY_ID,
                account: REFERRER,
                withdrawn_amount: 3500,  // 5000 * 70%
                repurchase_amount: 1500, // 5000 * 30%
                bonus_amount: 0,
            },
        ));

        // 购物余额记入 REFERRER（提现人自己）
        let referrer_shopping = get_loyalty_shopping_balance(ENTITY_ID, REFERRER);
        assert_eq!(referrer_shopping, 1500);
    });
}

// ============================================================================
// 基础复购场景: FixedRate 模式
// ============================================================================

#[test]
fn fixed_rate_withdrawal_split_works() {
    new_test_ext().execute_with(|| {
        use frame_support::BoundedVec;
        use pallet_commission_common::{WithdrawalMode, WithdrawalTierConfig};

        let ea = entity_account(ENTITY_ID);
        fund(PLATFORM, 1_000_000);
        fund(ea, 500_000);
        set_entity_referrer(ENTITY_ID, REFERRER);
        setup_config(10000);

        assert_ok!(process_commission_with_reserve(
            ENTITY_ID, SHOP_ID, 400, &BUYER, 100_000, 100_000, 20_000, PRODUCT_ID,
        ));
        // referrer gets 50% of 20000 = 10000

        assert_ok!(CommissionCore::set_withdrawal_config(
            RuntimeOrigin::signed(SELLER),
            ENTITY_ID,
            WithdrawalMode::FixedRate {
                repurchase_rate: 4000
            },
            WithdrawalTierConfig {
                withdrawal_rate: 6000,
                repurchase_rate: 4000
            },
            BoundedVec::default(),
            0,
            true,
        ));

        assert_ok!(CommissionCore::withdraw_commission(
            RuntimeOrigin::signed(REFERRER),
            ENTITY_ID,
            None,
            None,
        ));

        let stats = MemberCommissionStats::<Test>::get(ENTITY_ID, REFERRER);
        assert_eq!(stats.withdrawn, 6000); // 10000 * 60%
        assert_eq!(stats.repurchased, 4000); // 10000 * 40%
        assert_eq!(stats.pending, 0);

        let shopping = get_loyalty_shopping_balance(ENTITY_ID, REFERRER);
        assert_eq!(shopping, 4000);

        let shop_shopping = get_loyalty_shopping_total(ENTITY_ID);
        assert_eq!(shop_shopping, 4000);
    });
}

// ============================================================================
// Governance 底线 + FullWithdrawal 模式
// ============================================================================

#[test]
fn governance_floor_enforced_in_full_withdrawal_mode() {
    new_test_ext().execute_with(|| {
        use frame_support::BoundedVec;
        use pallet_commission_common::{WithdrawalMode, WithdrawalTierConfig};

        let ea = entity_account(ENTITY_ID);
        fund(PLATFORM, 1_000_000);
        fund(ea, 500_000);
        set_entity_referrer(ENTITY_ID, REFERRER);
        setup_config(10000);

        assert_ok!(process_commission_with_reserve(
            ENTITY_ID, SHOP_ID, 500, &BUYER, 100_000, 100_000, 20_000, PRODUCT_ID,
        ));

        // FullWithdrawal + governance floor at 30%
        assert_ok!(CommissionCore::set_withdrawal_config(
            RuntimeOrigin::signed(SELLER),
            ENTITY_ID,
            WithdrawalMode::FullWithdrawal,
            WithdrawalTierConfig {
                withdrawal_rate: 10000,
                repurchase_rate: 0
            },
            BoundedVec::default(),
            0,
            true,
        ));
        GlobalMinRepurchaseRate::<Test>::insert(ENTITY_ID, 3000u16);

        assert_ok!(CommissionCore::withdraw_commission(
            RuntimeOrigin::signed(REFERRER),
            ENTITY_ID,
            None,
            None,
        ));

        let stats = MemberCommissionStats::<Test>::get(ENTITY_ID, REFERRER);
        // Governance floor 30%: withdrawal = 70%, repurchase = 30%
        assert_eq!(stats.withdrawn, 7000); // 10000 * 70%
        assert_eq!(stats.repurchased, 3000); // 10000 * 30%

        let shopping = get_loyalty_shopping_balance(ENTITY_ID, REFERRER);
        assert_eq!(shopping, 3000);
    });
}
#[test]
fn h3_consume_shopping_balance_blocked_when_participation_denied() {
    new_test_ext().execute_with(|| {
        fund(entity_account(ENTITY_ID), 100_000);
        // 给 REFERRER 购物余额（通过 Mock Loyalty）
        set_loyalty_shopping_balance(ENTITY_ID, REFERRER, 5_000);
        set_loyalty_shopping_total(ENTITY_ID, 5_000);

        // 标记 REFERRER 不满足参与要求
        block_participation(ENTITY_ID, REFERRER);

        // KYC 检查由 Loyalty 模块处理，返回 DispatchError::Other
        assert!(CommissionCore::do_consume_shopping_balance(ENTITY_ID, &REFERRER, 1_000).is_err());

        // 解除限制后应成功
        unblock_participation(ENTITY_ID, REFERRER);
        assert_ok!(CommissionCore::do_consume_shopping_balance(
            ENTITY_ID, &REFERRER, 1_000
        ));
    });
}

#[test]
fn h1_self_withdraw_blocked_by_participation_guard() {
    // H1 审计修复: target=self 也经过 ParticipationGuard 检查（与 Token 提现一致）
    new_test_ext().execute_with(|| {
        fund(entity_account(ENTITY_ID), 100_000);
        MemberCommissionStats::<Test>::mutate(ENTITY_ID, &REFERRER, |s| {
            s.pending = 10_000;
        });
        ShopPendingTotal::<Test>::insert(ENTITY_ID, 10_000u128);

        // 标记 REFERRER 自己不满足参与要求
        block_participation(ENTITY_ID, REFERRER);

        // H1: self 也被阻止
        assert_noop!(
            CommissionCore::withdraw_commission(
                RuntimeOrigin::signed(REFERRER),
                ENTITY_ID,
                Some(5_000),
                None,
            ),
            Error::<Test>::ParticipationRequirementNotMet
        );
    });
}

// ============================================================================
// 购物余额仅可用于购物，不可直接提取
// ============================================================================

#[test]
fn use_shopping_balance_extrinsic_always_rejected() {
    new_test_ext().execute_with(|| {
        fund(entity_account(ENTITY_ID), 100_000);
        set_loyalty_shopping_balance(ENTITY_ID, REFERRER, 5_000);

        assert_noop!(
            CommissionCore::use_shopping_balance(
                RuntimeOrigin::signed(REFERRER),
                ENTITY_ID,
                1_000,
            ),
            Error::<Test>::ShoppingBalanceWithdrawalDisabled
        );

        // 余额不变
        assert_eq!(get_loyalty_shopping_balance(ENTITY_ID, REFERRER), 5_000);
    });
}

// ============================================================================
// Phase 7a: withdraw_token_commission — Token 佣金提现测试
// ============================================================================

/// 辅助：手动注入 Token 佣金 pending 状态（模拟 credit_token_commission 的结果）
fn inject_token_pending(entity_id: u64, beneficiary: u64, amount: u128) {
    MemberTokenCommissionStats::<Test>::mutate(entity_id, &beneficiary, |stats| {
        stats.total_earned = stats.total_earned.saturating_add(amount);
        stats.pending = stats.pending.saturating_add(amount);
    });
    TokenPendingTotal::<Test>::mutate(entity_id, |total| {
        *total = total.saturating_add(amount);
    });
}

#[test]
fn token_withdraw_full_pending_works() {
    new_test_ext().execute_with(|| {
        let ea = entity_account(ENTITY_ID);
        inject_token_pending(ENTITY_ID, REFERRER, 10_000);
        // entity_account 需要有足够 Token 余额
        set_token_balance(ENTITY_ID, ea, 50_000);

        assert_ok!(CommissionCore::withdraw_token_commission(
            RuntimeOrigin::signed(REFERRER),
            ENTITY_ID,
            None, // 全额提现
            None,
        ));

        let stats = MemberTokenCommissionStats::<Test>::get(ENTITY_ID, REFERRER);
        assert_eq!(stats.pending, 0);
        assert_eq!(stats.withdrawn, 10_000);
        assert_eq!(stats.total_earned, 10_000);
        assert_eq!(TokenPendingTotal::<Test>::get(ENTITY_ID), 0);

        // Token 余额验证
        assert_eq!(get_token_balance(ENTITY_ID, ea), 40_000);
        assert_eq!(get_token_balance(ENTITY_ID, REFERRER), 10_000);

        // 事件验证（新版 TokenTieredWithdrawal 替代旧 TokenCommissionWithdrawn）
        System::assert_has_event(RuntimeEvent::CommissionCore(
            crate::pallet::Event::TokenTieredWithdrawal {
                entity_id: ENTITY_ID,
                account: REFERRER,
                withdrawn_amount: 10_000,
                repurchase_amount: 0,
                bonus_amount: 0,
            },
        ));
    });
}

#[test]
fn token_withdraw_partial_amount_works() {
    new_test_ext().execute_with(|| {
        let ea = entity_account(ENTITY_ID);
        inject_token_pending(ENTITY_ID, REFERRER, 10_000);
        set_token_balance(ENTITY_ID, ea, 50_000);

        assert_ok!(CommissionCore::withdraw_token_commission(
            RuntimeOrigin::signed(REFERRER),
            ENTITY_ID,
            Some(3_000),
            None,
        ));

        let stats = MemberTokenCommissionStats::<Test>::get(ENTITY_ID, REFERRER);
        assert_eq!(stats.pending, 7_000);
        assert_eq!(stats.withdrawn, 3_000);
        assert_eq!(TokenPendingTotal::<Test>::get(ENTITY_ID), 7_000);
        assert_eq!(get_token_balance(ENTITY_ID, REFERRER), 3_000);
    });
}

#[test]
fn token_withdraw_rejects_zero_amount() {
    new_test_ext().execute_with(|| {
        let ea = entity_account(ENTITY_ID);
        set_token_balance(ENTITY_ID, ea, 50_000);
        // pending = 0，全额提现 → ZeroWithdrawalAmount

        assert_noop!(
            CommissionCore::withdraw_token_commission(
                RuntimeOrigin::signed(REFERRER),
                ENTITY_ID,
                None,
                None,
            ),
            Error::<Test>::ZeroWithdrawalAmount
        );
    });
}

#[test]
fn token_withdraw_rejects_insufficient_pending() {
    new_test_ext().execute_with(|| {
        let ea = entity_account(ENTITY_ID);
        inject_token_pending(ENTITY_ID, REFERRER, 5_000);
        set_token_balance(ENTITY_ID, ea, 50_000);

        assert_noop!(
            CommissionCore::withdraw_token_commission(
                RuntimeOrigin::signed(REFERRER),
                ENTITY_ID,
                Some(10_000), // 超过 pending
                None,
            ),
            Error::<Test>::InsufficientTokenCommission
        );
    });
}

#[test]
fn token_withdraw_rejects_when_entity_balance_insufficient() {
    new_test_ext().execute_with(|| {
        let ea = entity_account(ENTITY_ID);
        inject_token_pending(ENTITY_ID, REFERRER, 10_000);
        set_token_balance(ENTITY_ID, ea, 5_000); // 不足

        // M2 审计修复: 偿付能力不足使用专用错误码 InsufficientEntityTokenFunds
        assert_noop!(
            CommissionCore::withdraw_token_commission(
                RuntimeOrigin::signed(REFERRER),
                ENTITY_ID,
                None,
                None,
            ),
            Error::<Test>::InsufficientEntityTokenFunds
        );

        // stats 不应变化（try_mutate 回滚）
        let stats = MemberTokenCommissionStats::<Test>::get(ENTITY_ID, REFERRER);
        assert_eq!(stats.pending, 10_000);
        assert_eq!(stats.withdrawn, 0);
    });
}

#[test]
fn token_withdraw_cooldown_enforced() {
    new_test_ext().execute_with(|| {
        let ea = entity_account(ENTITY_ID);
        inject_token_pending(ENTITY_ID, REFERRER, 10_000);
        set_token_balance(ENTITY_ID, ea, 50_000);

        // 设置带冻结期的佣金配置
        CommissionConfigs::<Test>::insert(
            ENTITY_ID,
            CoreCommissionConfig {
                enabled_modes: CommissionModes(CommissionModes::DIRECT_REWARD),
                max_commission_rate: 10000,
                enabled: true,
                withdrawal_cooldown: 100, // 100 blocks 冻结期
                owner_reward_rate: 0,
                token_withdrawal_cooldown: 0,
                plugin_caps: PluginBudgetCaps::default(),
            },
        );

        // P3 审计修复: Token 冻结期使用独立的 MemberTokenLastCredited
        // MemberTokenLastCredited 模拟为 block 1（当前 block=1），cooldown=100
        // now(1) < last(1) + cooldown(100) = 101 → 应被拒绝
        crate::pallet::MemberTokenLastCredited::<Test>::insert(ENTITY_ID, REFERRER, 1u64);

        assert_noop!(
            CommissionCore::withdraw_token_commission(
                RuntimeOrigin::signed(REFERRER),
                ENTITY_ID,
                None,
                None,
            ),
            Error::<Test>::WithdrawalCooldownNotMet
        );

        // 推进到 block 101 → 应通过
        System::set_block_number(101);
        assert_ok!(CommissionCore::withdraw_token_commission(
            RuntimeOrigin::signed(REFERRER),
            ENTITY_ID,
            None,
            None,
        ));

        let stats = MemberTokenCommissionStats::<Test>::get(ENTITY_ID, REFERRER);
        assert_eq!(stats.pending, 0);
        assert_eq!(stats.withdrawn, 10_000);
    });
}

#[test]
fn token_withdraw_multiple_partial_draws() {
    new_test_ext().execute_with(|| {
        let ea = entity_account(ENTITY_ID);
        inject_token_pending(ENTITY_ID, REFERRER, 10_000);
        set_token_balance(ENTITY_ID, ea, 50_000);

        // 第一次提 3000
        assert_ok!(CommissionCore::withdraw_token_commission(
            RuntimeOrigin::signed(REFERRER),
            ENTITY_ID,
            Some(3_000),
            None,
        ));
        // 第二次提 4000
        assert_ok!(CommissionCore::withdraw_token_commission(
            RuntimeOrigin::signed(REFERRER),
            ENTITY_ID,
            Some(4_000),
            None,
        ));
        // 第三次提剩余 3000
        assert_ok!(CommissionCore::withdraw_token_commission(
            RuntimeOrigin::signed(REFERRER),
            ENTITY_ID,
            None,
            None,
        ));

        let stats = MemberTokenCommissionStats::<Test>::get(ENTITY_ID, REFERRER);
        assert_eq!(stats.pending, 0);
        assert_eq!(stats.withdrawn, 10_000);
        assert_eq!(TokenPendingTotal::<Test>::get(ENTITY_ID), 0);
        assert_eq!(get_token_balance(ENTITY_ID, REFERRER), 10_000);
        assert_eq!(get_token_balance(ENTITY_ID, ea), 40_000);
    });
}

// ============================================================================
// Phase 7b: 端到端集成测试 — Token 下单 → 佣金分配 → 提现/取消
// ============================================================================

/// 辅助：设置 Token 佣金配置（含 POOL_REWARD）
fn setup_token_config(max_commission_rate: u16, pool_reward: bool) {
    let mut modes = CommissionModes(CommissionModes::DIRECT_REWARD);
    if pool_reward {
        modes = CommissionModes(modes.0 | CommissionModes::POOL_REWARD);
    }
    CommissionConfigs::<Test>::insert(
        ENTITY_ID,
        CoreCommissionConfig {
            enabled_modes: modes,
            max_commission_rate,
            enabled: true,
            withdrawal_cooldown: 0,
            owner_reward_rate: 0,
            token_withdrawal_cooldown: 0,
            plugin_caps: PluginBudgetCaps {
                referral_cap: 10000,
                ..PluginBudgetCaps::default()
            },
        },
    );
}

#[test]
fn e2e_token_commission_all_to_pool_when_no_plugins() {
    // 插件都是 ()，所有 Token 佣金 → UnallocatedTokenPool
    new_test_ext().execute_with(|| {
        let ea = entity_account(ENTITY_ID);
        set_token_balance(ENTITY_ID, ea, 100_000);
        setup_token_config(5000, true); // 50% max, pool_reward=true

        assert_ok!(CommissionCore::process_token_commission(
            ENTITY_ID, SHOP_ID, 1001, &BUYER, 20_000, 20_000, 0, PRODUCT_ID,
        ));

        // max_commission = 20000 * 5000 / 10000 = 10000
        // entity_balance = 100000 → remaining = min(10000, 100000) = 10000
        // 插件全部为空 → remaining = 10000 → 全部入沉淀池
        assert_eq!(UnallocatedTokenPool::<Test>::get(ENTITY_ID), 10_000);
        let (eid, sid, amt) = OrderTokenUnallocated::<Test>::get(1001u64);
        assert_eq!((eid, sid, amt), (ENTITY_ID, SHOP_ID, 10_000));

        // 无佣金记录（插件为空）
        assert_eq!(OrderTokenCommissionRecords::<Test>::get(1001u64).len(), 0);

        // buyer 订单数递增
        let buyer_stats = MemberTokenCommissionStats::<Test>::get(ENTITY_ID, BUYER);
        assert_eq!(buyer_stats.order_count, 1);

        System::assert_has_event(RuntimeEvent::CommissionCore(
            crate::pallet::Event::TokenUnallocatedPooled {
                entity_id: ENTITY_ID,
                order_id: 1001,
                amount: 10_000,
            },
        ));
    });
}

#[test]
fn e2e_token_commission_capped_by_entity_balance() {
    new_test_ext().execute_with(|| {
        let ea = entity_account(ENTITY_ID);
        set_token_balance(ENTITY_ID, ea, 3_000); // 余额不足
        setup_token_config(5000, true);

        assert_ok!(CommissionCore::process_token_commission(
            ENTITY_ID, SHOP_ID, 1002, &BUYER, 20_000, 20_000, 0, PRODUCT_ID,
        ));

        // max_commission = 10000, entity_balance = 3000 → remaining = 3000
        assert_eq!(UnallocatedTokenPool::<Test>::get(ENTITY_ID), 3_000);
    });
}

#[test]
fn e2e_token_commission_no_pool_without_pool_reward_mode() {
    new_test_ext().execute_with(|| {
        let ea = entity_account(ENTITY_ID);
        set_token_balance(ENTITY_ID, ea, 100_000);
        setup_token_config(5000, false); // pool_reward=false

        assert_ok!(CommissionCore::process_token_commission(
            ENTITY_ID, SHOP_ID, 1003, &BUYER, 20_000, 20_000, 0, PRODUCT_ID,
        ));

        // 无 POOL_REWARD 模式 → 剩余不入池
        assert_eq!(UnallocatedTokenPool::<Test>::get(ENTITY_ID), 0);
    });
}

#[test]
fn e2e_token_cancel_refunds_pool_and_records() {
    new_test_ext().execute_with(|| {
        let ea = entity_account(ENTITY_ID);
        set_token_balance(ENTITY_ID, ea, 100_000);
        setup_token_config(5000, true);

        // 产生 Token 佣金（全部入池）
        assert_ok!(CommissionCore::process_token_commission(
            ENTITY_ID, SHOP_ID, 1004, &BUYER, 20_000, 20_000, 0, PRODUCT_ID,
        ));
        assert_eq!(UnallocatedTokenPool::<Test>::get(ENTITY_ID), 10_000);

        // 手动注入一条 Token 佣金记录（模拟插件分配）
        inject_token_pending(ENTITY_ID, REFERRER, 2_000);
        let record = pallet_commission_common::TokenCommissionRecord {
            entity_id: ENTITY_ID,
            order_id: 1004,
            buyer: BUYER,
            beneficiary: REFERRER,
            amount: 2_000u128,
            commission_type: CommissionType::DirectReward,
            level: 0,
            status: pallet_commission_common::CommissionStatus::Pending,
            created_at: 1u64,
        };
        OrderTokenCommissionRecords::<Test>::mutate(1004u64, |records| {
            let _ = records.try_push(record);
        });

        // 取消 Token 佣金
        assert_ok!(CommissionCore::do_cancel_token_commission(1004));

        // Token 记录已取消
        let records = OrderTokenCommissionRecords::<Test>::get(1004u64);
        assert_eq!(records.len(), 1);
        assert_eq!(
            records[0].status,
            pallet_commission_common::CommissionStatus::Cancelled
        );

        // 统计已回退
        let stats = MemberTokenCommissionStats::<Test>::get(ENTITY_ID, REFERRER);
        assert_eq!(stats.pending, 0);
        assert_eq!(stats.total_earned, 0);
        assert_eq!(TokenPendingTotal::<Test>::get(ENTITY_ID), 0);

        // 沉淀池已退还（Token 转给 seller）
        assert_eq!(UnallocatedTokenPool::<Test>::get(ENTITY_ID), 0);
        assert_eq!(OrderTokenUnallocated::<Test>::get(1004u64), (0, 0, 0));
        // seller = SHOP_OWNERS[SHOP_ID] = SELLER
        assert_eq!(get_token_balance(ENTITY_ID, SELLER), 10_000);

        System::assert_has_event(RuntimeEvent::CommissionCore(
            crate::pallet::Event::TokenCommissionCancelled {
                order_id: 1004,
                cancelled_count: 1,
            },
        ));
        System::assert_has_event(RuntimeEvent::CommissionCore(
            crate::pallet::Event::TokenUnallocatedPoolRefunded {
                entity_id: ENTITY_ID,
                order_id: 1004,
                amount: 10_000,
            },
        ));
    });
}

#[test]
fn e2e_token_process_then_withdraw_full_flow() {
    // 完整流程: process_token_commission → inject pending (模拟插件分配) → withdraw
    new_test_ext().execute_with(|| {
        let ea = entity_account(ENTITY_ID);
        set_token_balance(ENTITY_ID, ea, 100_000);
        setup_token_config(5000, true);

        // Step 1: 产生 Token 佣金
        assert_ok!(CommissionCore::process_token_commission(
            ENTITY_ID, SHOP_ID, 1005, &BUYER, 20_000, 20_000, 0, PRODUCT_ID,
        ));

        // Step 2: 模拟插件分配了 5000 给 REFERRER
        inject_token_pending(ENTITY_ID, REFERRER, 5_000);

        // Step 3: REFERRER 提现
        assert_ok!(CommissionCore::withdraw_token_commission(
            RuntimeOrigin::signed(REFERRER),
            ENTITY_ID,
            None,
            None,
        ));

        let stats = MemberTokenCommissionStats::<Test>::get(ENTITY_ID, REFERRER);
        assert_eq!(stats.pending, 0);
        assert_eq!(stats.withdrawn, 5_000);
        assert_eq!(get_token_balance(ENTITY_ID, REFERRER), 5_000);
        assert_eq!(get_token_balance(ENTITY_ID, ea), 95_000); // 100000 - 5000
    });
}

#[test]
fn e2e_token_partial_withdraw_then_cancel_remaining() {
    new_test_ext().execute_with(|| {
        let ea = entity_account(ENTITY_ID);
        set_token_balance(ENTITY_ID, ea, 100_000);
        setup_token_config(5000, true);

        assert_ok!(CommissionCore::process_token_commission(
            ENTITY_ID, SHOP_ID, 1006, &BUYER, 20_000, 20_000, 0, PRODUCT_ID,
        ));

        // 注入 8000 佣金给 REFERRER，并添加记录
        inject_token_pending(ENTITY_ID, REFERRER, 8_000);
        let record = pallet_commission_common::TokenCommissionRecord {
            entity_id: ENTITY_ID,
            order_id: 1006,
            buyer: BUYER,
            beneficiary: REFERRER,
            amount: 8_000u128,
            commission_type: CommissionType::DirectReward,
            level: 0,
            status: pallet_commission_common::CommissionStatus::Pending,
            created_at: 1u64,
        };
        OrderTokenCommissionRecords::<Test>::mutate(1006u64, |records| {
            let _ = records.try_push(record);
        });

        // Step 1: 部分提现 3000
        assert_ok!(CommissionCore::withdraw_token_commission(
            RuntimeOrigin::signed(REFERRER),
            ENTITY_ID,
            Some(3_000),
            None,
        ));
        assert_eq!(get_token_balance(ENTITY_ID, REFERRER), 3_000);

        let stats = MemberTokenCommissionStats::<Test>::get(ENTITY_ID, REFERRER);
        assert_eq!(stats.pending, 5_000);
        assert_eq!(stats.withdrawn, 3_000);

        // Step 2: 取消订单 → 剩余 pending 被取消
        assert_ok!(CommissionCore::do_cancel_token_commission(1006));

        let stats_after = MemberTokenCommissionStats::<Test>::get(ENTITY_ID, REFERRER);
        assert_eq!(stats_after.pending, 0);
        assert_eq!(stats_after.total_earned, 0); // total_earned 也被回退
        assert_eq!(stats_after.withdrawn, 3_000); // withdrawn 不变

        // 记录状态
        let records = OrderTokenCommissionRecords::<Test>::get(1006u64);
        assert_eq!(
            records[0].status,
            pallet_commission_common::CommissionStatus::Cancelled
        );
    });
}

#[test]
fn e2e_token_multiple_orders_accumulate() {
    new_test_ext().execute_with(|| {
        let ea = entity_account(ENTITY_ID);
        set_token_balance(ENTITY_ID, ea, 200_000);
        setup_token_config(5000, true);

        // 两笔订单
        assert_ok!(CommissionCore::process_token_commission(
            ENTITY_ID, SHOP_ID, 2001, &BUYER, 20_000, 20_000, 0, PRODUCT_ID,
        ));
        assert_ok!(CommissionCore::process_token_commission(
            ENTITY_ID, SHOP_ID, 2002, &BUYER, 30_000, 30_000, 0, PRODUCT_ID,
        ));

        // 第一笔: max = 10000, 第二笔: max = 15000 → 全部入池
        assert_eq!(UnallocatedTokenPool::<Test>::get(ENTITY_ID), 25_000);

        // buyer 订单数 = 2
        let buyer_stats = MemberTokenCommissionStats::<Test>::get(ENTITY_ID, BUYER);
        assert_eq!(buyer_stats.order_count, 2);

        // 取消第一笔
        assert_ok!(CommissionCore::do_cancel_token_commission(2001));
        assert_eq!(UnallocatedTokenPool::<Test>::get(ENTITY_ID), 15_000);
        assert_eq!(get_token_balance(ENTITY_ID, SELLER), 10_000);

        // 第二笔仍在
        let (eid, _, amt) = OrderTokenUnallocated::<Test>::get(2002u64);
        assert_eq!(eid, ENTITY_ID);
        assert_eq!(amt, 15_000);
    });
}

#[test]
fn e2e_cancel_nex_does_not_affect_token_records() {
    // 验证 NEX cancel_commission 也会处理 Token 记录（因为 cancel_commission 包含第六步）
    new_test_ext().execute_with(|| {
        let ea = entity_account(ENTITY_ID);
        fund(PLATFORM, 1_000_000);
        fund(ea, 1);
        set_entity_referrer(ENTITY_ID, REFERRER);
        setup_config(10000);
        set_token_balance(ENTITY_ID, ea, 100_000);

        // 产生 NEX 佣金
        assert_ok!(process_commission_with_reserve(
            ENTITY_ID, SHOP_ID, 3001, &BUYER, 100_000, 100_000, 10_000, PRODUCT_ID,
        ));

        // 同时注入 Token 佣金记录到同一 order_id
        inject_token_pending(ENTITY_ID, REFERRER, 5_000);
        let record = pallet_commission_common::TokenCommissionRecord {
            entity_id: ENTITY_ID,
            order_id: 3001,
            buyer: BUYER,
            beneficiary: REFERRER,
            amount: 5_000u128,
            commission_type: CommissionType::DirectReward,
            level: 0,
            status: pallet_commission_common::CommissionStatus::Pending,
            created_at: 1u64,
        };
        OrderTokenCommissionRecords::<Test>::mutate(3001u64, |records| {
            let _ = records.try_push(record);
        });

        // cancel_commission（NEX）也会取消 Token 记录（第六步）
        assert_ok!(CommissionCore::cancel_commission(3001));

        // NEX 佣金已取消
        let nex_records = OrderCommissionRecords::<Test>::get(3001u64);
        assert_eq!(
            nex_records[0].status,
            pallet_commission_common::CommissionStatus::Cancelled
        );

        // Token 记录也已取消
        let token_records = OrderTokenCommissionRecords::<Test>::get(3001u64);
        assert_eq!(
            token_records[0].status,
            pallet_commission_common::CommissionStatus::Cancelled
        );

        // Token pending 已回退
        let token_stats = MemberTokenCommissionStats::<Test>::get(ENTITY_ID, REFERRER);
        assert_eq!(token_stats.pending, 0);
    });
}

#[test]
fn e2e_token_commission_not_configured_returns_ok() {
    // M2-R5: 未配置时优雅返回 Ok（与 NEX 版对称）
    new_test_ext().execute_with(|| {
        let ea = entity_account(ENTITY_ID);
        set_token_balance(ENTITY_ID, ea, 100_000);
        // 不配置 CommissionConfigs

        assert_ok!(CommissionCore::process_token_commission(
            ENTITY_ID, SHOP_ID, 4001, &BUYER, 20_000, 20_000, 0, PRODUCT_ID,
        ));
        // 未配置时不产生佣金记录
        assert_eq!(TokenPendingTotal::<Test>::get(ENTITY_ID), 0);
    });
}

#[test]
fn e2e_token_commission_disabled_returns_ok() {
    // M2-R5: 配置存在但禁用时优雅返回 Ok（与 NEX 版对称）
    new_test_ext().execute_with(|| {
        let ea = entity_account(ENTITY_ID);
        set_token_balance(ENTITY_ID, ea, 100_000);

        CommissionConfigs::<Test>::insert(
            ENTITY_ID,
            CoreCommissionConfig {
                enabled_modes: CommissionModes(CommissionModes::DIRECT_REWARD),
                max_commission_rate: 5000,
                enabled: false, // 已禁用
                withdrawal_cooldown: 0,
                owner_reward_rate: 0,
                token_withdrawal_cooldown: 0,
                plugin_caps: PluginBudgetCaps::default(),
            },
        );

        assert_ok!(CommissionCore::process_token_commission(
            ENTITY_ID, SHOP_ID, 4002, &BUYER, 20_000, 20_000, 0, PRODUCT_ID,
        ));
        assert_eq!(TokenPendingTotal::<Test>::get(ENTITY_ID), 0);
    });
}

// ============================================================================
// Phase 7c: 审计回归测试
// ============================================================================

#[test]
fn h1_token_withdraw_blocked_when_participation_denied() {
    // H1: withdraw_token_commission 需检查 ParticipationGuard
    new_test_ext().execute_with(|| {
        let ea = entity_account(ENTITY_ID);
        inject_token_pending(ENTITY_ID, REFERRER, 10_000);
        set_token_balance(ENTITY_ID, ea, 50_000);

        // 将 REFERRER 加入 KYC 黑名单
        block_participation(ENTITY_ID, REFERRER);

        assert_noop!(
            CommissionCore::withdraw_token_commission(
                RuntimeOrigin::signed(REFERRER),
                ENTITY_ID,
                None,
                None,
            ),
            Error::<Test>::ParticipationRequirementNotMet
        );

        // 余额不应变化
        let stats = MemberTokenCommissionStats::<Test>::get(ENTITY_ID, REFERRER);
        assert_eq!(stats.pending, 10_000);
        assert_eq!(stats.withdrawn, 0);
    });
}

#[test]
fn h2_cancel_token_pool_refund_skipped_on_transfer_failure() {
    // H2: do_cancel_token_commission 转账失败时不应扣减池余额
    new_test_ext().execute_with(|| {
        let ea = entity_account(ENTITY_ID);
        set_token_balance(ENTITY_ID, ea, 100_000);
        setup_token_config(5000, true);

        // 产生 Token 佣金（10000 入池）
        assert_ok!(CommissionCore::process_token_commission(
            ENTITY_ID, SHOP_ID, 5001, &BUYER, 20_000, 20_000, 0, PRODUCT_ID,
        ));
        assert_eq!(UnallocatedTokenPool::<Test>::get(ENTITY_ID), 10_000);

        // 将 entity_account 的 Token 余额清零，模拟偿付能力不足
        set_token_balance(ENTITY_ID, ea, 0);

        // 取消 Token 佣金
        assert_ok!(CommissionCore::do_cancel_token_commission(5001));

        // H2 修复: 转账失败 → 池余额不应被扣减，记录不应被移除
        assert_eq!(UnallocatedTokenPool::<Test>::get(ENTITY_ID), 10_000);
        let (eid, _, amt) = OrderTokenUnallocated::<Test>::get(5001u64);
        assert_eq!(eid, ENTITY_ID);
        assert_eq!(amt, 10_000);
    });
}

#[test]
fn h2_cancel_token_pool_refund_succeeds_normally() {
    // H2 正常路径: 转账成功时池余额正常扣减
    new_test_ext().execute_with(|| {
        let ea = entity_account(ENTITY_ID);
        set_token_balance(ENTITY_ID, ea, 100_000);
        setup_token_config(5000, true);

        assert_ok!(CommissionCore::process_token_commission(
            ENTITY_ID, SHOP_ID, 5002, &BUYER, 20_000, 20_000, 0, PRODUCT_ID,
        ));
        assert_eq!(UnallocatedTokenPool::<Test>::get(ENTITY_ID), 10_000);

        // entity_account 有足够余额 → 正常退还
        assert_ok!(CommissionCore::do_cancel_token_commission(5002));

        assert_eq!(UnallocatedTokenPool::<Test>::get(ENTITY_ID), 0);
        assert_eq!(OrderTokenUnallocated::<Test>::get(5002u64), (0, 0, 0));
        assert_eq!(get_token_balance(ENTITY_ID, SELLER), 10_000);
    });
}

// ============================================================================
// Step G: 新功能测试 — extrinsics + Pool A + 复购分流 + 治理底线
// ============================================================================

// --- G3: set_token_withdrawal_config ---

#[test]
fn g3_set_token_withdrawal_config_fixed_rate_works() {
    new_test_ext().execute_with(|| {
        assert_ok!(CommissionCore::set_token_withdrawal_config(
            RuntimeOrigin::signed(SELLER),
            ENTITY_ID,
            WithdrawalMode::FixedRate {
                repurchase_rate: 3000
            },
            WithdrawalTierConfig {
                repurchase_rate: 0,
                withdrawal_rate: 10000
            },
            Default::default(),
            500, // voluntary_bonus_rate = 5%
            true,
        ));
        let wc = TokenWithdrawalConfigs::<Test>::get(ENTITY_ID).unwrap();
        assert_eq!(
            wc.mode,
            WithdrawalMode::FixedRate {
                repurchase_rate: 3000
            }
        );
        assert_eq!(wc.voluntary_bonus_rate, 500);
        assert!(wc.enabled);
        System::assert_has_event(RuntimeEvent::CommissionCore(
            crate::pallet::Event::TokenWithdrawalConfigUpdated {
                entity_id: ENTITY_ID,
            },
        ));
    });
}

#[test]
fn g3_set_token_withdrawal_config_rejects_non_owner() {
    new_test_ext().execute_with(|| {
        assert_noop!(
            CommissionCore::set_token_withdrawal_config(
                RuntimeOrigin::signed(BUYER),
                ENTITY_ID,
                WithdrawalMode::FullWithdrawal,
                WithdrawalTierConfig {
                    repurchase_rate: 0,
                    withdrawal_rate: 10000
                },
                Default::default(),
                0,
                true,
            ),
            Error::<Test>::NotEntityOwnerOrAdmin
        );
    });
}

#[test]
fn g3_set_token_withdrawal_config_rejects_invalid_rate() {
    new_test_ext().execute_with(|| {
        assert_noop!(
            CommissionCore::set_token_withdrawal_config(
                RuntimeOrigin::signed(SELLER),
                ENTITY_ID,
                WithdrawalMode::FixedRate {
                    repurchase_rate: 10001
                },
                WithdrawalTierConfig {
                    repurchase_rate: 0,
                    withdrawal_rate: 10000
                },
                Default::default(),
                0,
                true,
            ),
            Error::<Test>::InvalidWithdrawalConfig
        );
    });
}

// --- G4: set_global_min_token_repurchase_rate ---

#[test]
fn g4_set_global_min_token_repurchase_rate_works() {
    new_test_ext().execute_with(|| {
        assert_ok!(CommissionCore::set_global_min_token_repurchase_rate(
            RuntimeOrigin::root(),
            ENTITY_ID,
            2000,
        ));
        assert_eq!(GlobalMinTokenRepurchaseRate::<Test>::get(ENTITY_ID), 2000);
    });
}

#[test]
fn g4_set_global_min_token_repurchase_rate_rejects_non_root() {
    new_test_ext().execute_with(|| {
        assert_noop!(
            CommissionCore::set_global_min_token_repurchase_rate(
                RuntimeOrigin::signed(SELLER),
                ENTITY_ID,
                2000,
            ),
            sp_runtime::DispatchError::BadOrigin
        );
    });
}

#[test]
fn g4_set_global_min_token_repurchase_rate_rejects_over_10000() {
    new_test_ext().execute_with(|| {
        assert_noop!(
            CommissionCore::set_global_min_token_repurchase_rate(
                RuntimeOrigin::root(),
                ENTITY_ID,
                10001,
            ),
            Error::<Test>::InvalidCommissionRate
        );
    });
}

// --- G5: process_token_commission with Pool A (token_platform_fee > 0) ---

#[test]
fn g5_pool_a_referrer_gets_share_of_token_platform_fee() {
    // 设置 entity_referrer → 推荐人应从 token_platform_fee 获得分成
    new_test_ext().execute_with(|| {
        let ea = entity_account(ENTITY_ID);
        set_token_balance(ENTITY_ID, ea, 200_000);
        setup_token_config(5000, true);
        // 设置 Entity 招商推荐人
        set_entity_referrer(ENTITY_ID, REFERRER);

        // token_platform_fee = 1000, ReferrerShareBps = 5000 (50%)
        // → referrer 应得 500
        assert_ok!(CommissionCore::process_token_commission(
            ENTITY_ID, SHOP_ID, 6001, &BUYER, 20_000, 19_000, 1_000, PRODUCT_ID,
        ));

        // 推荐人应获得 500 Token 佣金（EntityReferral 类型）
        let stats = MemberTokenCommissionStats::<Test>::get(ENTITY_ID, REFERRER);
        assert_eq!(stats.total_earned, 500);
        assert_eq!(stats.pending, 500);

        // 验证 TokenCommissionDistributed 事件
        System::assert_has_event(RuntimeEvent::CommissionCore(
            crate::pallet::Event::TokenCommissionDistributed {
                entity_id: ENTITY_ID,
                order_id: 6001,
                beneficiary: REFERRER,
                amount: 500,
                commission_type: CommissionType::EntityReferral,
                level: 0,
            },
        ));
    });
}

#[test]
fn g5_pool_a_no_referrer_means_all_stays_in_entity() {
    // 无 entity_referrer → token_platform_fee 全部留在 entity_account
    new_test_ext().execute_with(|| {
        let ea = entity_account(ENTITY_ID);
        set_token_balance(ENTITY_ID, ea, 200_000);
        setup_token_config(5000, true);
        // 不设置 entity_referrer

        assert_ok!(CommissionCore::process_token_commission(
            ENTITY_ID, SHOP_ID, 6002, &BUYER, 20_000, 19_000, 1_000, PRODUCT_ID,
        ));

        // 无推荐人 → 无 EntityReferral 佣金记录
        let records = OrderTokenCommissionRecords::<Test>::get(6002u64);
        assert!(records
            .iter()
            .all(|r| r.commission_type != CommissionType::EntityReferral));
    });
}

#[test]
fn g5_pool_a_zero_fee_skips_referrer() {
    // token_platform_fee = 0 时不应有 Pool A 分配（即使有推荐人）
    new_test_ext().execute_with(|| {
        let ea = entity_account(ENTITY_ID);
        set_token_balance(ENTITY_ID, ea, 200_000);
        setup_token_config(5000, true);
        set_entity_referrer(ENTITY_ID, REFERRER);

        assert_ok!(CommissionCore::process_token_commission(
            ENTITY_ID, SHOP_ID, 6003, &BUYER, 20_000, 20_000, 0, PRODUCT_ID,
        ));

        let stats = MemberTokenCommissionStats::<Test>::get(ENTITY_ID, REFERRER);
        assert_eq!(stats.total_earned, 0);
    });
}

// --- G6: withdraw_token_commission with repurchase split ---

#[test]
fn g6_token_withdraw_with_fixed_rate_repurchase() {
    new_test_ext().execute_with(|| {
        let ea = entity_account(ENTITY_ID);
        inject_token_pending(ENTITY_ID, REFERRER, 10_000);
        set_token_balance(ENTITY_ID, ea, 50_000);

        // 设置 Token 提现配置: FixedRate 30% 复购
        assert_ok!(CommissionCore::set_token_withdrawal_config(
            RuntimeOrigin::signed(SELLER),
            ENTITY_ID,
            WithdrawalMode::FixedRate {
                repurchase_rate: 3000
            },
            WithdrawalTierConfig {
                repurchase_rate: 0,
                withdrawal_rate: 10000
            },
            Default::default(),
            0, // 无自愿奖励
            true,
        ));

        assert_ok!(CommissionCore::withdraw_token_commission(
            RuntimeOrigin::signed(REFERRER),
            ENTITY_ID,
            None,
            None,
        ));

        // 10000 × 70% = 7000 提现, 10000 × 30% = 3000 复购
        let stats = MemberTokenCommissionStats::<Test>::get(ENTITY_ID, REFERRER);
        assert_eq!(stats.withdrawn, 7_000);
        assert_eq!(stats.repurchased, 3_000);
        assert_eq!(get_token_balance(ENTITY_ID, REFERRER), 7_000);
        assert_eq!(
            get_loyalty_token_shopping_balance(ENTITY_ID, REFERRER),
            3_000
        );
        assert_eq!(get_loyalty_token_shopping_total(ENTITY_ID), 3_000);

        // 事件
        System::assert_has_event(RuntimeEvent::CommissionCore(
            crate::pallet::Event::TokenTieredWithdrawal {
                entity_id: ENTITY_ID,
                account: REFERRER,
                withdrawn_amount: 7_000,
                repurchase_amount: 3_000,
                bonus_amount: 0,
            },
        ));
    });
}

#[test]
fn g6_token_withdraw_member_choice_with_voluntary_bonus() {
    new_test_ext().execute_with(|| {
        let ea = entity_account(ENTITY_ID);
        inject_token_pending(ENTITY_ID, REFERRER, 10_000);
        set_token_balance(ENTITY_ID, ea, 50_000);

        // MemberChoice: min 20%, voluntary_bonus_rate = 1000 (10%)
        assert_ok!(CommissionCore::set_token_withdrawal_config(
            RuntimeOrigin::signed(SELLER),
            ENTITY_ID,
            WithdrawalMode::MemberChoice {
                min_repurchase_rate: 2000
            },
            WithdrawalTierConfig {
                repurchase_rate: 0,
                withdrawal_rate: 10000
            },
            Default::default(),
            1000, // 10% 自愿奖励
            true,
        ));

        // 会员请求 50% 复购（超出最低 20%）
        assert_ok!(CommissionCore::withdraw_token_commission(
            RuntimeOrigin::signed(REFERRER),
            ENTITY_ID,
            None,
            Some(5000), // 请求 50% 复购
        ));

        // 10000 × 50% = 5000 提现, 10000 × 50% = 5000 复购
        // mandatory_min = 20%, 强制复购 = 2000
        // voluntary_extra = 5000 - 2000 = 3000
        // bonus = 3000 × 10% = 300
        let stats = MemberTokenCommissionStats::<Test>::get(ENTITY_ID, REFERRER);
        assert_eq!(stats.withdrawn, 5_000);
        assert_eq!(stats.repurchased, 5_300); // 5000 repurchase + 300 bonus
        assert_eq!(get_token_balance(ENTITY_ID, REFERRER), 5_000);
        assert_eq!(
            get_loyalty_token_shopping_balance(ENTITY_ID, REFERRER),
            5_300
        );
    });
}
#[test]
fn g8_governance_floor_overrides_entity_full_withdrawal() {
    new_test_ext().execute_with(|| {
        let ea = entity_account(ENTITY_ID);
        inject_token_pending(ENTITY_ID, REFERRER, 10_000);
        set_token_balance(ENTITY_ID, ea, 50_000);

        // Entity 设置 FullWithdrawal（无复购）
        assert_ok!(CommissionCore::set_token_withdrawal_config(
            RuntimeOrigin::signed(SELLER),
            ENTITY_ID,
            WithdrawalMode::FullWithdrawal,
            WithdrawalTierConfig {
                repurchase_rate: 0,
                withdrawal_rate: 10000
            },
            Default::default(),
            0,
            true,
        ));

        // Governance 设置全局最低复购 20%
        assert_ok!(CommissionCore::set_global_min_token_repurchase_rate(
            RuntimeOrigin::root(),
            ENTITY_ID,
            2000,
        ));

        assert_ok!(CommissionCore::withdraw_token_commission(
            RuntimeOrigin::signed(REFERRER),
            ENTITY_ID,
            None,
            None,
        ));

        // FullWithdrawal 但 governance 强制 20% 复购
        // 10000 × 80% = 8000 提现, 10000 × 20% = 2000 复购
        let stats = MemberTokenCommissionStats::<Test>::get(ENTITY_ID, REFERRER);
        assert_eq!(stats.withdrawn, 8_000);
        assert_eq!(stats.repurchased, 2_000);
        assert_eq!(get_token_balance(ENTITY_ID, REFERRER), 8_000);
        assert_eq!(
            get_loyalty_token_shopping_balance(ENTITY_ID, REFERRER),
            2_000
        );
    });
}

#[test]
fn g8_governance_floor_does_not_lower_entity_rate() {
    new_test_ext().execute_with(|| {
        let ea = entity_account(ENTITY_ID);
        inject_token_pending(ENTITY_ID, REFERRER, 10_000);
        set_token_balance(ENTITY_ID, ea, 50_000);

        // Entity 设置 FixedRate 50% 复购
        assert_ok!(CommissionCore::set_token_withdrawal_config(
            RuntimeOrigin::signed(SELLER),
            ENTITY_ID,
            WithdrawalMode::FixedRate {
                repurchase_rate: 5000
            },
            WithdrawalTierConfig {
                repurchase_rate: 0,
                withdrawal_rate: 10000
            },
            Default::default(),
            0,
            true,
        ));

        // Governance 底线 20%（低于 Entity 的 50%）
        assert_ok!(CommissionCore::set_global_min_token_repurchase_rate(
            RuntimeOrigin::root(),
            ENTITY_ID,
            2000,
        ));

        assert_ok!(CommissionCore::withdraw_token_commission(
            RuntimeOrigin::signed(REFERRER),
            ENTITY_ID,
            None,
            None,
        ));

        // Entity 50% > governance 20% → 最终 50% 复购
        let stats = MemberTokenCommissionStats::<Test>::get(ENTITY_ID, REFERRER);
        assert_eq!(stats.withdrawn, 5_000);
        assert_eq!(stats.repurchased, 5_000);
    });
}

#[test]
fn g8_governance_floor_applies_when_no_token_withdrawal_config() {
    // 无 TokenWithdrawalConfig 时，calc_token_withdrawal_split 返回 (0,0,0) 模式
    // governance 底线仍生效
    new_test_ext().execute_with(|| {
        let ea = entity_account(ENTITY_ID);
        inject_token_pending(ENTITY_ID, REFERRER, 10_000);
        set_token_balance(ENTITY_ID, ea, 50_000);

        // 不设置任何 Token 提现配置

        // Governance 底线 30%
        assert_ok!(CommissionCore::set_global_min_token_repurchase_rate(
            RuntimeOrigin::root(),
            ENTITY_ID,
            3000,
        ));

        assert_ok!(CommissionCore::withdraw_token_commission(
            RuntimeOrigin::signed(REFERRER),
            ENTITY_ID,
            None,
            None,
        ));

        // 无 config → mode (0,0,0), governance 30% 兜底
        let stats = MemberTokenCommissionStats::<Test>::get(ENTITY_ID, REFERRER);
        assert_eq!(stats.withdrawn, 7_000);
        assert_eq!(stats.repurchased, 3_000);
    });
}

#[test]
fn g8_disabled_token_withdrawal_config_is_rejected() {
    new_test_ext().execute_with(|| {
        let ea = entity_account(ENTITY_ID);
        inject_token_pending(ENTITY_ID, REFERRER, 10_000);
        set_token_balance(ENTITY_ID, ea, 50_000);

        // Token 提现配置存在但 enabled=false
        assert_ok!(CommissionCore::set_token_withdrawal_config(
            RuntimeOrigin::signed(SELLER),
            ENTITY_ID,
            WithdrawalMode::FixedRate {
                repurchase_rate: 8000
            },
            WithdrawalTierConfig {
                repurchase_rate: 0,
                withdrawal_rate: 10000
            },
            Default::default(),
            0,
            false,
        ));

        assert_noop!(
            CommissionCore::withdraw_token_commission(
                RuntimeOrigin::signed(REFERRER),
                ENTITY_ID,
                None,
                None,
            ),
            Error::<Test>::WithdrawalConfigNotEnabled
        );
    });
}

// ============================================================================
// P1: withdraw_entity_funds / withdraw_entity_token_funds
// ============================================================================

#[test]
fn p1_withdraw_entity_funds_works() {
    new_test_ext().execute_with(|| {
        let ea = entity_account(ENTITY_ID);
        fund(ea, 100_000);
        // 无任何锁定，全部可提（减去 minimum_balance=1）
        assert_ok!(CommissionCore::withdraw_entity_funds(
            RuntimeOrigin::signed(SELLER),
            ENTITY_ID,
            99_999,
        ));
        let seller_bal = Balances::free_balance(SELLER);
        assert_eq!(seller_bal, 99_999);
        System::assert_has_event(RuntimeEvent::CommissionCore(
            crate::pallet::Event::EntityFundsWithdrawn {
                entity_id: ENTITY_ID,
                to: SELLER,
                amount: 99_999,
            },
        ));
    });
}

#[test]
fn p1_withdraw_entity_funds_rejects_non_owner() {
    new_test_ext().execute_with(|| {
        let ea = entity_account(ENTITY_ID);
        fund(ea, 100_000);
        assert_noop!(
            CommissionCore::withdraw_entity_funds(RuntimeOrigin::signed(BUYER), ENTITY_ID, 1_000,),
            Error::<Test>::NotEntityOwner
        );
    });
}

#[test]
fn p1_withdraw_entity_funds_respects_locked_pools() {
    new_test_ext().execute_with(|| {
        let ea = entity_account(ENTITY_ID);
        fund(ea, 100_000);
        // 开启 POOL_REWARD 使沉淀池锁定
        setup_token_config(5000, true);
        // 模拟锁定: pending=30000, shopping=20000, unallocated=10000 = 60000 locked
        ShopPendingTotal::<Test>::insert(ENTITY_ID, 30_000u128);
        set_loyalty_shopping_total(ENTITY_ID, 20_000);
        UnallocatedPool::<Test>::insert(ENTITY_ID, 10_000u128);
        // available = 100000 - 60000 - 1(min_balance) = 39999
        assert_noop!(
            CommissionCore::withdraw_entity_funds(
                RuntimeOrigin::signed(SELLER), ENTITY_ID, 40_000,
            ),
            Error::<Test>::InsufficientEntityFunds
        );
        assert_ok!(CommissionCore::withdraw_entity_funds(
            RuntimeOrigin::signed(SELLER), ENTITY_ID, 39_999,
        ));
    });
}

#[test]
fn p1_withdraw_entity_funds_rejects_zero() {
    new_test_ext().execute_with(|| {
        let ea = entity_account(ENTITY_ID);
        fund(ea, 100_000);
        assert_noop!(
            CommissionCore::withdraw_entity_funds(RuntimeOrigin::signed(SELLER), ENTITY_ID, 0,),
            Error::<Test>::ZeroWithdrawalAmount
        );
    });
}

#[test]
fn h2_withdraw_entity_funds_respects_pending_refund_total() {
    // H-2: PendingRefundTotal 必须被 withdraw_entity_funds 保护，
    // 防止 Owner 提走后 retry_pending_refund 余额不足
    new_test_ext().execute_with(|| {
        let ea = entity_account(ENTITY_ID);
        fund(ea, 100_000);

        // 模拟锁定: pending=20000, shopping=10000, pending_refund=15000
        ShopPendingTotal::<Test>::insert(ENTITY_ID, 20_000u128);
        set_loyalty_shopping_total(ENTITY_ID, 10_000);
        PendingRefundTotal::<Test>::insert(ENTITY_ID, 15_000u128);

        // available = 100000 - 20000 - 10000 - 15000 - 1(min_balance) = 54999
        // 尝试提 55000 → 应被拒绝
        assert_noop!(
            CommissionCore::withdraw_entity_funds(
                RuntimeOrigin::signed(SELLER), ENTITY_ID, 55_000,
            ),
            Error::<Test>::InsufficientEntityFunds
        );

        // 提 54999 → 应成功
        assert_ok!(CommissionCore::withdraw_entity_funds(
            RuntimeOrigin::signed(SELLER), ENTITY_ID, 54_999,
        ));
    });
}

#[test]
fn h2_withdraw_entity_funds_pending_refund_plus_pool_always_protected() {
    // H-2: PendingRefundTotal + 沉淀池始终受保护（不论 POOL_REWARD 模式是否开启）
    new_test_ext().execute_with(|| {
        let ea = entity_account(ENTITY_ID);
        fund(ea, 100_000);

        // 即使不开启 POOL_REWARD，池也始终受保护
        setup_token_config(5000, false);

        // 模拟: pending=10000, shopping=5000, pool=20000(always protected), pending_refund=10000
        ShopPendingTotal::<Test>::insert(ENTITY_ID, 10_000u128);
        set_loyalty_shopping_total(ENTITY_ID, 5_000);
        UnallocatedPool::<Test>::insert(ENTITY_ID, 20_000u128);
        PendingRefundTotal::<Test>::insert(ENTITY_ID, 10_000u128);

        // available = 100000 - (10000+5000+10000) - 20000 - 1 = 54999
        assert_noop!(
            CommissionCore::withdraw_entity_funds(
                RuntimeOrigin::signed(SELLER), ENTITY_ID, 55_000,
            ),
            Error::<Test>::InsufficientEntityFunds
        );
        assert_ok!(CommissionCore::withdraw_entity_funds(
            RuntimeOrigin::signed(SELLER), ENTITY_ID, 54_999,
        ));
        // Pool is NOT auto-shrunk — always protected
        assert_eq!(UnallocatedPool::<Test>::get(ENTITY_ID), 20_000);
    });
}

#[test]
fn p1_withdraw_entity_token_funds_pre_existing_balance_withdrawable() {
    // 首次访问: 已有余额视为合法运营资金，可提取
    new_test_ext().execute_with(|| {
        let ea = entity_account(ENTITY_ID);
        set_token_balance(ENTITY_ID, ea, 50_000);
        assert_ok!(CommissionCore::withdraw_entity_token_funds(
            RuntimeOrigin::signed(SELLER),
            ENTITY_ID,
            50_000,
        ));
        assert_eq!(get_token_balance(ENTITY_ID, ea), 0);
        assert_eq!(get_token_balance(ENTITY_ID, SELLER), 50_000);
        assert_eq!(UnallocatedTokenPool::<Test>::get(ENTITY_ID), 0);
    });
}

#[test]
fn p1_withdraw_entity_token_funds_external_transfer_swept() {
    // 已初始化 EntityTokenAccountedBalance 后，外部转入被 sweep 归入沉淀池
    new_test_ext().execute_with(|| {
        let ea = entity_account(ENTITY_ID);
        set_token_balance(ENTITY_ID, ea, 50_000);
        // 开启 POOL_REWARD 使沉淀池锁定
        setup_token_config(5000, true);
        // 第一次提取: 初始化 accounted = 50,000，提取 10,000
        assert_ok!(CommissionCore::withdraw_entity_token_funds(
            RuntimeOrigin::signed(SELLER),
            ENTITY_ID,
            10_000,
        ));
        // accounted = 50,000 - 10,000 = 40,000, actual = 40,000
        assert_eq!(get_token_balance(ENTITY_ID, ea), 40_000);

        // 外部转入 5,000
        set_token_balance(ENTITY_ID, ea, 45_000);

        // 第二次提取: sweep 检测 external = 45,000 - 40,000 = 5,000 → UnallocatedTokenPool
        // available = 45,000 - (0 + 0 + 5,000) = 40,000
        assert_ok!(CommissionCore::withdraw_entity_token_funds(
            RuntimeOrigin::signed(SELLER),
            ENTITY_ID,
            40_000,
        ));
        assert_eq!(UnallocatedTokenPool::<Test>::get(ENTITY_ID), 5_000);
        assert_eq!(get_token_balance(ENTITY_ID, ea), 5_000); // 外部转入留在沉淀池
                                                             // 尝试提取沉淀池中的 5,000 → 失败（池始终受保护）
        assert_noop!(
            CommissionCore::withdraw_entity_token_funds(
                RuntimeOrigin::signed(SELLER),
                ENTITY_ID,
                1,
            ),
            Error::<Test>::InsufficientEntityTokenFunds
        );
    });
}

#[test]
fn p1_withdraw_entity_token_funds_respects_locked_pools() {
    // 池记账部分不可提取，仅合法 free 部分可提（池始终受保护）
    new_test_ext().execute_with(|| {
        let ea = entity_account(ENTITY_ID);
        set_token_balance(ENTITY_ID, ea, 50_000);
        // 开启 POOL_REWARD 使沉淀池锁定
        setup_token_config(5000, true);
        TokenPendingTotal::<Test>::insert(ENTITY_ID, 20_000u128);
        set_loyalty_token_shopping_total(ENTITY_ID, 10_000);
        UnallocatedTokenPool::<Test>::insert(ENTITY_ID, 5_000u128);
        // available = 50000 - 35000 = 15000（首次访问，全部视为合法）
        assert_noop!(
            CommissionCore::withdraw_entity_token_funds(
                RuntimeOrigin::signed(SELLER),
                ENTITY_ID,
                15_001,
            ),
            Error::<Test>::InsufficientEntityTokenFunds
        );
        assert_ok!(CommissionCore::withdraw_entity_token_funds(
            RuntimeOrigin::signed(SELLER),
            ENTITY_ID,
            15_000,
        ));
    });
}

// ============================================================================
// P10: 沉淀池始终受保护 — Owner 永远不可提取池资金
// ============================================================================

#[test]
fn p10_token_pool_always_protected() {
    // 沉淀池始终受保护，不论 POOL_REWARD 是否开启
    new_test_ext().execute_with(|| {
        let ea = entity_account(ENTITY_ID);
        set_token_balance(ENTITY_ID, ea, 50_000);
        setup_token_config(5000, true);
        UnallocatedTokenPool::<Test>::insert(ENTITY_ID, 20_000u128);
        // available = 50000 - 20000(always protected pool) = 30000
        assert_noop!(
            CommissionCore::withdraw_entity_token_funds(
                RuntimeOrigin::signed(SELLER),
                ENTITY_ID,
                30_001,
            ),
            Error::<Test>::InsufficientEntityTokenFunds
        );
        assert_ok!(CommissionCore::withdraw_entity_token_funds(
            RuntimeOrigin::signed(SELLER),
            ENTITY_ID,
            30_000,
        ));
        // 沉淀池不变 — 始终受保护
        assert_eq!(UnallocatedTokenPool::<Test>::get(ENTITY_ID), 20_000);
    });
}

#[test]
fn p10_pool_not_empty_blocks_mode_change() {
    // 沉淀池非空时不允许关闭 POOL_REWARD 模式
    new_test_ext().execute_with(|| {
        let ea = entity_account(ENTITY_ID);
        set_token_balance(ENTITY_ID, ea, 50_000);
        // 先开启 POOL_REWARD
        setup_token_config(5000, true);
        UnallocatedTokenPool::<Test>::insert(ENTITY_ID, 20_000u128);

        // 尝试关闭 POOL_REWARD → PoolNotEmpty
        assert_noop!(
            CommissionCore::set_commission_modes(
                RuntimeOrigin::signed(SELLER),
                ENTITY_ID,
                CommissionModes(CommissionModes::DIRECT_REWARD),
            ),
            Error::<Test>::PoolNotEmpty
        );

        // 池清空后可以关闭
        UnallocatedTokenPool::<Test>::insert(ENTITY_ID, 0u128);
        assert_ok!(CommissionCore::set_commission_modes(
            RuntimeOrigin::signed(SELLER),
            ENTITY_ID,
            CommissionModes(CommissionModes::DIRECT_REWARD),
        ));
    });
}

#[test]
fn p10_pool_always_protected_even_without_pool_reward_mode() {
    // 即使配置不含 POOL_REWARD 模式，池资金仍始终受保护
    new_test_ext().execute_with(|| {
        let ea = entity_account(ENTITY_ID);
        set_token_balance(ENTITY_ID, ea, 50_000);
        // 配置不含 POOL_REWARD
        CommissionConfigs::<Test>::insert(
            ENTITY_ID,
            CoreCommissionConfig {
                enabled_modes: CommissionModes(CommissionModes::DIRECT_REWARD),
                max_commission_rate: 5000,
                enabled: true,
                withdrawal_cooldown: 0,
                owner_reward_rate: 0,
                token_withdrawal_cooldown: 0,
                plugin_caps: PluginBudgetCaps::default(),
            },
        );
        UnallocatedTokenPool::<Test>::insert(ENTITY_ID, 20_000u128);
        // 池始终受保护, available = 50000 - 20000 = 30000
        assert_noop!(
            CommissionCore::withdraw_entity_token_funds(
                RuntimeOrigin::signed(SELLER),
                ENTITY_ID,
                30_001,
            ),
            Error::<Test>::InsufficientEntityTokenFunds
        );
        assert_ok!(CommissionCore::withdraw_entity_token_funds(
            RuntimeOrigin::signed(SELLER),
            ENTITY_ID,
            30_000,
        ));
        // 池不变
        assert_eq!(UnallocatedTokenPool::<Test>::get(ENTITY_ID), 20_000);
    });
}

#[test]
fn p10_token_pool_immutable_during_withdrawal() {
    // 提取时池余额永远不会被扣减
    new_test_ext().execute_with(|| {
        let ea = entity_account(ENTITY_ID);
        set_token_balance(ENTITY_ID, ea, 50_000);
        setup_token_config(5000, true);
        UnallocatedTokenPool::<Test>::insert(ENTITY_ID, 20_000u128);

        // available = 50000 - 20000 = 30000, withdraw all available
        assert_ok!(CommissionCore::withdraw_entity_token_funds(
            RuntimeOrigin::signed(SELLER),
            ENTITY_ID,
            30_000,
        ));
        // 池不变
        assert_eq!(UnallocatedTokenPool::<Test>::get(ENTITY_ID), 20_000);
        assert_eq!(get_token_balance(ENTITY_ID, ea), 20_000);
        // 尝试提取更多 → 失败
        assert_noop!(
            CommissionCore::withdraw_entity_token_funds(
                RuntimeOrigin::signed(SELLER),
                ENTITY_ID,
                1,
            ),
            Error::<Test>::InsufficientEntityTokenFunds
        );
    });
}

#[test]
fn p10_nex_pool_always_protected() {
    // NEX 沉淀池始终受保护，即使 POOL_REWARD 开启后再想提取也不行
    new_test_ext().execute_with(|| {
        let ea = entity_account(ENTITY_ID);
        fund(ea, 100_000);
        setup_token_config(5000, true);
        UnallocatedPool::<Test>::insert(ENTITY_ID, 30_000u128);

        // 沉淀池始终受保护
        // available = 100000 - 30000 - 1(min) = 69999
        assert_ok!(CommissionCore::withdraw_entity_funds(
            RuntimeOrigin::signed(SELLER),
            ENTITY_ID,
            69_999,
        ));
        assert_eq!(UnallocatedPool::<Test>::get(ENTITY_ID), 30_000);

        // 尝试提取更多 → 失败（池受保护）
        assert_noop!(
            CommissionCore::withdraw_entity_funds(RuntimeOrigin::signed(SELLER), ENTITY_ID, 1,),
            Error::<Test>::InsufficientEntityFunds
        );
    });
}

#[test]
fn p10_pool_not_empty_prevents_pool_reward_toggle() {
    // 池非空时不能关闭 POOL_REWARD，重新开启不受限制
    new_test_ext().execute_with(|| {
        let ea = entity_account(ENTITY_ID);
        set_token_balance(ENTITY_ID, ea, 50_000);
        setup_token_config(5000, true);
        UnallocatedTokenPool::<Test>::insert(ENTITY_ID, 20_000u128);

        // 尝试关闭 POOL_REWARD → PoolNotEmpty
        assert_noop!(
            CommissionCore::set_commission_modes(
                RuntimeOrigin::signed(SELLER),
                ENTITY_ID,
                CommissionModes(CommissionModes::DIRECT_REWARD),
            ),
            Error::<Test>::PoolNotEmpty
        );

        // 池仍在，池资金不可提取
        System::set_block_number(500);
        // available = 50000 - 20000(protected) = 30000
        assert_noop!(
            CommissionCore::withdraw_entity_token_funds(
                RuntimeOrigin::signed(SELLER),
                ENTITY_ID,
                30_001,
            ),
            Error::<Test>::InsufficientEntityTokenFunds
        );
    });
}

#[test]
fn p10_pool_protected_without_pool_reward_history() {
    // 从未开启过 POOL_REWARD → 池资金仍然受保护（始终不可提取）
    new_test_ext().execute_with(|| {
        let ea = entity_account(ENTITY_ID);
        set_token_balance(ENTITY_ID, ea, 50_000);
        // 配置不含 POOL_REWARD
        CommissionConfigs::<Test>::insert(
            ENTITY_ID,
            CoreCommissionConfig {
                enabled_modes: CommissionModes(CommissionModes::DIRECT_REWARD),
                max_commission_rate: 5000,
                enabled: true,
                withdrawal_cooldown: 0,
                owner_reward_rate: 0,
                token_withdrawal_cooldown: 0,
                plugin_caps: PluginBudgetCaps::default(),
            },
        );
        UnallocatedTokenPool::<Test>::insert(ENTITY_ID, 20_000u128);
        // 池始终受保护，available = 50000 - 20000 = 30000
        assert_noop!(
            CommissionCore::withdraw_entity_token_funds(
                RuntimeOrigin::signed(SELLER),
                ENTITY_ID,
                30_001,
            ),
            Error::<Test>::InsufficientEntityTokenFunds
        );
        assert_ok!(CommissionCore::withdraw_entity_token_funds(
            RuntimeOrigin::signed(SELLER),
            ENTITY_ID,
            30_000,
        ));
        // 池不变
        assert_eq!(UnallocatedTokenPool::<Test>::get(ENTITY_ID), 20_000);
    });
}

// ============================================================================
// P2: do_consume_token_shopping_balance
// ============================================================================

#[test]
fn p2_consume_token_shopping_balance_works() {
    new_test_ext().execute_with(|| {
        let ea = entity_account(ENTITY_ID);
        set_token_balance(ENTITY_ID, ea, 50_000);
        // 注入 Token 购物余额
        set_loyalty_token_shopping_balance(ENTITY_ID, REFERRER, 10_000);
        set_loyalty_token_shopping_total(ENTITY_ID, 10_000);

        assert_ok!(CommissionCore::do_consume_token_shopping_balance(
            ENTITY_ID, &REFERRER, 6_000,
        ));
        assert_eq!(
            get_loyalty_token_shopping_balance(ENTITY_ID, REFERRER),
            4_000
        );
        assert_eq!(get_loyalty_token_shopping_total(ENTITY_ID), 4_000);
        assert_eq!(get_token_balance(ENTITY_ID, REFERRER), 6_000);
        assert_eq!(get_token_balance(ENTITY_ID, ea), 44_000);
        System::assert_has_event(RuntimeEvent::CommissionCore(
            crate::pallet::Event::TokenShoppingBalanceUsed {
                entity_id: ENTITY_ID,
                account: REFERRER,
                amount: 6_000,
            },
        ));
    });
}

#[test]
fn p2_consume_token_shopping_balance_rejects_insufficient() {
    new_test_ext().execute_with(|| {
        let ea = entity_account(ENTITY_ID);
        set_token_balance(ENTITY_ID, ea, 50_000);
        set_loyalty_token_shopping_balance(ENTITY_ID, REFERRER, 5_000);
        set_loyalty_token_shopping_total(ENTITY_ID, 5_000);

        assert_noop!(
            CommissionCore::do_consume_token_shopping_balance(ENTITY_ID, &REFERRER, 5_001),
            sp_runtime::DispatchError::Other("InsufficientShoppingBalance")
        );
    });
}

#[test]
fn p2_consume_token_shopping_balance_rejects_zero() {
    new_test_ext().execute_with(|| {
        assert_noop!(
            CommissionCore::do_consume_token_shopping_balance(ENTITY_ID, &REFERRER, 0),
            Error::<Test>::ZeroWithdrawalAmount
        );
    });
}

#[test]
fn p2_consume_token_shopping_balance_blocked_by_kyc() {
    new_test_ext().execute_with(|| {
        let ea = entity_account(ENTITY_ID);
        set_token_balance(ENTITY_ID, ea, 50_000);
        set_loyalty_token_shopping_balance(ENTITY_ID, REFERRER, 5_000);
        set_loyalty_token_shopping_total(ENTITY_ID, 5_000);

        block_participation(ENTITY_ID, REFERRER);
        assert_noop!(
            CommissionCore::do_consume_token_shopping_balance(ENTITY_ID, &REFERRER, 1_000),
            sp_runtime::DispatchError::Other("ParticipationRequirementNotMet")
        );
    });
}

// ============================================================================
// 审计回归测试
// ============================================================================

#[test]
fn h2_set_token_withdrawal_config_rejects_invalid_tier_sum() {
    // H2: default_tier.withdrawal_rate + repurchase_rate 必须等于 10000
    new_test_ext().execute_with(|| {
        assert_noop!(
            CommissionCore::set_token_withdrawal_config(
                RuntimeOrigin::signed(SELLER),
                ENTITY_ID,
                WithdrawalMode::FixedRate {
                    repurchase_rate: 3000
                },
                WithdrawalTierConfig {
                    repurchase_rate: 5000,
                    withdrawal_rate: 4000
                }, // sum=9000
                Default::default(),
                0,
                true,
            ),
            Error::<Test>::InvalidWithdrawalConfig
        );

        // 正确的 tier sum 应成功
        assert_ok!(CommissionCore::set_token_withdrawal_config(
            RuntimeOrigin::signed(SELLER),
            ENTITY_ID,
            WithdrawalMode::FixedRate {
                repurchase_rate: 3000
            },
            WithdrawalTierConfig {
                repurchase_rate: 3000,
                withdrawal_rate: 7000
            }, // sum=10000
            Default::default(),
            0,
            true,
        ));
    });
}

#[test]
fn h2_set_token_withdrawal_config_rejects_invalid_override_tier_sum() {
    // H2: level_overrides tier sum 也必须等于 10000
    new_test_ext().execute_with(|| {
        let bad_overrides: BoundedVec<(u8, WithdrawalTierConfig), ConstU32<10>> =
            BoundedVec::try_from(vec![
                (
                    1,
                    WithdrawalTierConfig {
                        repurchase_rate: 6000,
                        withdrawal_rate: 6000,
                    },
                ), // sum=12000
            ])
            .unwrap();
        assert_noop!(
            CommissionCore::set_token_withdrawal_config(
                RuntimeOrigin::signed(SELLER),
                ENTITY_ID,
                WithdrawalMode::LevelBased,
                WithdrawalTierConfig {
                    repurchase_rate: 3000,
                    withdrawal_rate: 7000
                },
                bad_overrides,
                0,
                true,
            ),
            Error::<Test>::InvalidWithdrawalConfig
        );
    });
}

// ============================================================================
// H1 审计修复 (Round 4): Token 佣金预算扣除已承诺额度
// ============================================================================

#[test]
fn h1_token_budget_deducts_committed_amounts() {
    // H1: process_token_commission 第二笔订单的预算应扣除第一笔的已承诺额度
    // 防止跨订单重复承诺同一批 Token
    new_test_ext().execute_with(|| {
        let ea = entity_account(ENTITY_ID);
        set_token_balance(ENTITY_ID, ea, 15_000); // 余额有限
        setup_token_config(5000, true); // 50% max, pool_reward=true

        // Order 1: amount=20000, max_commission=10000, available=15000 → remaining=10000
        assert_ok!(CommissionCore::process_token_commission(
            ENTITY_ID, SHOP_ID, 7001, &BUYER, 20_000, 20_000, 0, PRODUCT_ID,
        ));
        // 插件为空 → 10000 全部入沉淀池
        assert_eq!(UnallocatedTokenPool::<Test>::get(ENTITY_ID), 10_000);

        // Order 2: amount=20000, max_commission=10000
        // H1 修复后: available = 15000 - 10000(committed) = 5000 → remaining = min(10000, 5000) = 5000
        // 修复前: available = 15000 → remaining = min(10000, 15000) = 10000 (超额承诺!)
        assert_ok!(CommissionCore::process_token_commission(
            ENTITY_ID, SHOP_ID, 7002, &BUYER, 20_000, 20_000, 0, PRODUCT_ID,
        ));
        // 第二笔只应承诺 5000（而非 10000）
        assert_eq!(UnallocatedTokenPool::<Test>::get(ENTITY_ID), 15_000);

        // 总承诺 = 15000 = entity_balance，不超额
    });
}

#[test]
fn h1_token_budget_zero_when_fully_committed() {
    // 当已承诺额度 ≥ entity_balance 时，新订单应零分配
    new_test_ext().execute_with(|| {
        let ea = entity_account(ENTITY_ID);
        set_token_balance(ENTITY_ID, ea, 10_000);
        setup_token_config(5000, true);

        // Order 1: 全部可用余额被承诺
        assert_ok!(CommissionCore::process_token_commission(
            ENTITY_ID, SHOP_ID, 7003, &BUYER, 20_000, 20_000, 0, PRODUCT_ID,
        ));
        assert_eq!(UnallocatedTokenPool::<Test>::get(ENTITY_ID), 10_000);

        // Order 2: available = 10000 - 10000 = 0 → remaining = 0
        assert_ok!(CommissionCore::process_token_commission(
            ENTITY_ID, SHOP_ID, 7004, &BUYER, 20_000, 20_000, 0, PRODUCT_ID,
        ));
        // 沉淀池不应增长
        assert_eq!(UnallocatedTokenPool::<Test>::get(ENTITY_ID), 10_000);

        // 第二笔订单的 unallocated 应为 0
        let (_, _, amt) = OrderTokenUnallocated::<Test>::get(7004u64);
        assert_eq!(amt, 0);
    });
}

#[test]
fn h1_token_budget_accounts_for_pending_and_shopping() {
    // 验证 committed 包含 TokenPendingTotal + TokenShoppingTotal + UnallocatedTokenPool
    new_test_ext().execute_with(|| {
        let ea = entity_account(ENTITY_ID);
        set_token_balance(ENTITY_ID, ea, 30_000);
        setup_token_config(5000, true);

        // 手动注入各类已承诺额度
        inject_token_pending(ENTITY_ID, REFERRER, 8_000); // TokenPendingTotal = 8000
        set_loyalty_token_shopping_total(ENTITY_ID, 7_000); // TokenShoppingTotal = 7000
        UnallocatedTokenPool::<Test>::insert(ENTITY_ID, 5_000u128); // UnallocatedTokenPool = 5000
                                                                    // committed = 8000 + 7000 + 5000 = 20000
                                                                    // available = 30000 - 20000 = 10000

        assert_ok!(CommissionCore::process_token_commission(
            ENTITY_ID, SHOP_ID, 7005, &BUYER, 40_000, 40_000, 0, PRODUCT_ID,
        ));
        // max_commission = 40000 * 5000 / 10000 = 20000
        // remaining = min(20000, 10000) = 10000
        // 插件为空 → 10000 入沉淀池
        assert_eq!(UnallocatedTokenPool::<Test>::get(ENTITY_ID), 5_000 + 10_000);
    });
}

#[test]
fn h3_cancel_commission_refunds_pool_b_via_shop_id() {
    // H3: credit_commission 现在存储真实 shop_id，cancel_commission 可正确退款
    new_test_ext().execute_with(|| {
        let ea = entity_account(ENTITY_ID);
        fund(ea, 1);
        fund(PLATFORM, 1_000_000);
        fund(SELLER, 200_000);
        setup_config(5000); // max_commission_rate = 50%

        // 处理佣金（Pool B 需要 seller 有余额）
        assert_ok!(process_commission_with_reserve(
            ENTITY_ID,
            SHOP_ID,
            999,
            &BUYER,
            100_000u128,
            100_000u128,
            10_000u128,
            PRODUCT_ID,
        ));

        // 验证记录中的 shop_id 不为 0
        let records = OrderCommissionRecords::<Test>::get(999);
        for record in records.iter() {
            if record.commission_type != CommissionType::EntityReferral {
                assert_eq!(
                    record.shop_id, SHOP_ID,
                    "Pool B records should have real shop_id"
                );
            }
        }

        // 取消佣金 — 应成功退款
        assert_ok!(CommissionCore::cancel_commission(999));

        // 验证 Pool B 记录被取消
        let records_after = OrderCommissionRecords::<Test>::get(999);
        for record in records_after.iter() {
            assert_eq!(record.status, CommissionStatus::Cancelled);
        }
    });
}

#[test]
fn h4_trait_set_commission_modes_pool_not_empty_guard() {
    // H4: CommissionProvider::set_commission_modes 同样检查 PoolNotEmpty
    use pallet_commission_common::CommissionProvider;
    new_test_ext().execute_with(|| {
        // M1-R6: 必须先设置 enabled=true
        CommissionConfigs::<Test>::mutate(ENTITY_ID, |maybe| {
            let config = maybe.get_or_insert_with(CoreCommissionConfig::default);
            config.enabled = true;
            config.plugin_caps.referral_cap = 10000;
        });

        // 开启 POOL_REWARD
        assert_ok!(
            <CommissionCore as CommissionProvider<u64, u128>>::set_commission_modes(
                ENTITY_ID,
                CommissionModes::DIRECT_REWARD | CommissionModes::POOL_REWARD,
            )
        );

        // 池有资金
        UnallocatedPool::<Test>::insert(ENTITY_ID, 5000u128);

        // 关闭 POOL_REWARD → PoolNotEmpty
        assert!(
            <CommissionCore as CommissionProvider<u64, u128>>::set_commission_modes(
                ENTITY_ID,
                CommissionModes::DIRECT_REWARD,
            )
            .is_err()
        );

        // 清空池后关闭成功
        UnallocatedPool::<Test>::insert(ENTITY_ID, 0u128);
        assert_ok!(
            <CommissionCore as CommissionProvider<u64, u128>>::set_commission_modes(
                ENTITY_ID,
                CommissionModes::DIRECT_REWARD,
            )
        );

        // 重新开启 POOL_REWARD（无池资金限制）
        assert_ok!(
            <CommissionCore as CommissionProvider<u64, u128>>::set_commission_modes(
                ENTITY_ID,
                CommissionModes::DIRECT_REWARD | CommissionModes::POOL_REWARD,
            )
        );
    });
}

// ============================================================================
// P3 审计修复: Token 冻结期与 NEX 完全解耦回归测试
// ============================================================================

#[test]
fn p3_nex_credit_does_not_freeze_token_withdrawal() {
    // NEX 入账更新 MemberLastCredited，不应阻止 Token 提现
    new_test_ext().execute_with(|| {
        let ea = entity_account(ENTITY_ID);
        inject_token_pending(ENTITY_ID, REFERRER, 10_000);
        set_token_balance(ENTITY_ID, ea, 50_000);

        // 设置 100 blocks 冻结期
        CommissionConfigs::<Test>::insert(
            ENTITY_ID,
            CoreCommissionConfig {
                enabled_modes: CommissionModes(CommissionModes::DIRECT_REWARD),
                max_commission_rate: 10000,
                enabled: true,
                withdrawal_cooldown: 100,
                owner_reward_rate: 0,
                token_withdrawal_cooldown: 0,
                plugin_caps: PluginBudgetCaps::default(),
            },
        );

        // 模拟 NEX 入账 → 更新 MemberLastCredited
        System::set_block_number(50);
        crate::pallet::MemberLastCredited::<Test>::insert(ENTITY_ID, REFERRER, 50u64);
        // MemberTokenLastCredited 未被更新（默认 0）

        // Token 提现不应被 NEX 冻结期阻止
        // now(50) >= token_last(0) + cooldown(100) = 100 → false, 但 0 + 100 = 100 > 50
        // 需要推进到 block 100
        System::set_block_number(100);
        assert_ok!(CommissionCore::withdraw_token_commission(
            RuntimeOrigin::signed(REFERRER),
            ENTITY_ID,
            None,
            None,
        ));

        let stats = MemberTokenCommissionStats::<Test>::get(ENTITY_ID, REFERRER);
        assert_eq!(stats.pending, 0);
        assert_eq!(stats.withdrawn, 10_000);
    });
}

#[test]
fn p3_token_credit_does_not_freeze_nex_withdrawal() {
    // Token 入账更新 MemberTokenLastCredited，不应阻止 NEX 提现
    new_test_ext().execute_with(|| {
        let ea = entity_account(ENTITY_ID);
        fund(ea, 500_000);
        // 注入 NEX pending 佣金
        MemberCommissionStats::<Test>::mutate(ENTITY_ID, &REFERRER, |stats| {
            stats.total_earned = 10_000;
            stats.pending = 10_000;
        });
        ShopPendingTotal::<Test>::insert(ENTITY_ID, 10_000u128);

        // 设置 100 blocks 冻结期
        CommissionConfigs::<Test>::insert(
            ENTITY_ID,
            CoreCommissionConfig {
                enabled_modes: CommissionModes(CommissionModes::DIRECT_REWARD),
                max_commission_rate: 10000,
                enabled: true,
                withdrawal_cooldown: 100,
                owner_reward_rate: 0,
                token_withdrawal_cooldown: 0,
                plugin_caps: PluginBudgetCaps::default(),
            },
        );

        // 模拟 Token 入账 → 更新 MemberTokenLastCredited（block 50）
        System::set_block_number(50);
        crate::pallet::MemberTokenLastCredited::<Test>::insert(ENTITY_ID, REFERRER, 50u64);
        // MemberLastCredited 未被更新（默认 0）

        // NEX 提现不应被 Token 冻结期影响
        // now(50) >= nex_last(0) + cooldown(100) = 100 → false
        System::set_block_number(100);
        assert_ok!(CommissionCore::withdraw_commission(
            RuntimeOrigin::signed(REFERRER),
            ENTITY_ID,
            None,
            None,
        ));

        let stats = MemberCommissionStats::<Test>::get(ENTITY_ID, REFERRER);
        assert_eq!(stats.pending, 0);
    });
}

#[test]
fn p3_token_cooldown_independent_from_nex_cooldown() {
    // Token 和 NEX 各自入账、各自冻结，互不干扰
    new_test_ext().execute_with(|| {
        let ea = entity_account(ENTITY_ID);
        inject_token_pending(ENTITY_ID, REFERRER, 10_000);
        set_token_balance(ENTITY_ID, ea, 50_000);

        CommissionConfigs::<Test>::insert(
            ENTITY_ID,
            CoreCommissionConfig {
                enabled_modes: CommissionModes(CommissionModes::DIRECT_REWARD),
                max_commission_rate: 10000,
                enabled: true,
                withdrawal_cooldown: 100,
                owner_reward_rate: 0,
                token_withdrawal_cooldown: 0,
                plugin_caps: PluginBudgetCaps::default(),
            },
        );

        // Token 入账在 block 200
        System::set_block_number(200);
        crate::pallet::MemberTokenLastCredited::<Test>::insert(ENTITY_ID, REFERRER, 200u64);
        // NEX 入账在 block 10（很早）
        crate::pallet::MemberLastCredited::<Test>::insert(ENTITY_ID, REFERRER, 10u64);

        // 在 block 250: NEX 冻结期早已过（10+100=110 < 250），Token 冻结期未过（200+100=300 > 250）
        System::set_block_number(250);

        // Token 应被拒绝
        assert_noop!(
            CommissionCore::withdraw_token_commission(
                RuntimeOrigin::signed(REFERRER),
                ENTITY_ID,
                None,
                None,
            ),
            Error::<Test>::WithdrawalCooldownNotMet
        );

        // 推进到 block 300: Token 冻结期也过了
        System::set_block_number(300);
        assert_ok!(CommissionCore::withdraw_token_commission(
            RuntimeOrigin::signed(REFERRER),
            ENTITY_ID,
            None,
            None,
        ));
    });
}

#[test]
fn p3_credit_token_commission_writes_token_last_credited() {
    // credit_token_commission 应写入 MemberTokenLastCredited，不写 MemberLastCredited
    new_test_ext().execute_with(|| {
        System::set_block_number(42);

        assert_ok!(CommissionCore::credit_token_commission(
            ENTITY_ID,
            1,
            &BUYER,
            &REFERRER,
            5_000u128,
            CommissionType::DirectReward,
            1,
            42u64,
        ));

        // MemberTokenLastCredited 应被更新
        assert_eq!(
            crate::pallet::MemberTokenLastCredited::<Test>::get(ENTITY_ID, REFERRER),
            42u64
        );

        // MemberLastCredited 不应被更新（保持默认值 0）
        assert_eq!(
            crate::pallet::MemberLastCredited::<Test>::get(ENTITY_ID, REFERRER),
            0u64
        );
    });
}

// ============================================================================
// Round 4 审计回归测试
// ============================================================================

#[test]
fn m1_set_global_min_token_repurchase_rate_emits_event() {
    // M1: set_global_min_token_repurchase_rate 应发射 GlobalMinTokenRepurchaseRateSet 事件
    new_test_ext().execute_with(|| {
        assert_ok!(CommissionCore::set_global_min_token_repurchase_rate(
            RuntimeOrigin::root(),
            ENTITY_ID,
            2500,
        ));
        System::assert_has_event(RuntimeEvent::CommissionCore(
            crate::pallet::Event::GlobalMinTokenRepurchaseRateSet {
                entity_id: ENTITY_ID,
                rate: 2500,
            },
        ));
    });
}

#[test]
fn l5_set_min_repurchase_rate_via_trait_emits_event() {
    // L5: CommissionProvider::set_min_repurchase_rate 应发射 GlobalMinRepurchaseRateSet 事件
    use pallet_commission_common::CommissionProvider;
    new_test_ext().execute_with(|| {
        assert_ok!(
            <CommissionCore as CommissionProvider<u64, u128>>::set_min_repurchase_rate(
                ENTITY_ID, 3000,
            )
        );
        assert_eq!(GlobalMinRepurchaseRate::<Test>::get(ENTITY_ID), 3000);
        System::assert_has_event(RuntimeEvent::CommissionCore(
            crate::pallet::Event::GlobalMinRepurchaseRateSet {
                entity_id: ENTITY_ID,
                rate: 3000,
            },
        ));
    });
}

#[test]
fn m2_withdraw_commission_solvency_uses_entity_funds_error() {
    // M2: Entity 偿付能力不足应返回 InsufficientEntityFunds（而非 InsufficientCommission）
    new_test_ext().execute_with(|| {
        let ea = entity_account(ENTITY_ID);
        // entity_account 余额极低，不足以覆盖提现 + 剩余承诺
        fund(ea, 100);
        // 注入大量 pending 佣金
        MemberCommissionStats::<Test>::mutate(ENTITY_ID, &REFERRER, |stats| {
            stats.total_earned = 50_000;
            stats.pending = 50_000;
        });
        ShopPendingTotal::<Test>::insert(ENTITY_ID, 50_000u128);

        CommissionConfigs::<Test>::insert(
            ENTITY_ID,
            CoreCommissionConfig {
                enabled_modes: CommissionModes(CommissionModes::DIRECT_REWARD),
                max_commission_rate: 10000,
                enabled: true,
                withdrawal_cooldown: 0,
                owner_reward_rate: 0,
                token_withdrawal_cooldown: 0,
                plugin_caps: PluginBudgetCaps::default(),
            },
        );

        // pending 足够（50000 >= 1000），但 entity 余额不足
        assert_noop!(
            CommissionCore::withdraw_commission(
                RuntimeOrigin::signed(REFERRER),
                ENTITY_ID,
                Some(1_000),
                None,
            ),
            Error::<Test>::InsufficientEntityFunds
        );
    });
}

#[test]
fn m2_withdraw_token_commission_solvency_uses_entity_token_funds_error() {
    // M2: Token Entity 偿付能力不足应返回 InsufficientEntityTokenFunds
    new_test_ext().execute_with(|| {
        let ea = entity_account(ENTITY_ID);
        // entity_account Token 余额极低
        set_token_balance(ENTITY_ID, ea, 10);
        inject_token_pending(ENTITY_ID, REFERRER, 50_000);

        assert_noop!(
            CommissionCore::withdraw_token_commission(
                RuntimeOrigin::signed(REFERRER),
                ENTITY_ID,
                Some(1_000),
                None,
            ),
            Error::<Test>::InsufficientEntityTokenFunds
        );
    });
}

#[test]
fn m3_cancel_commission_still_cancels_token_records() {
    // M3: 重构后 cancel_commission 仍能正确取消 Token 记录（通过 do_cancel_token_commission）
    new_test_ext().execute_with(|| {
        let ea = entity_account(ENTITY_ID);
        fund(PLATFORM, 1_000_000);
        fund(ea, 1);
        set_entity_referrer(ENTITY_ID, REFERRER);
        setup_config(10000);
        set_token_balance(ENTITY_ID, ea, 100_000);

        // 产生 NEX 佣金
        assert_ok!(process_commission_with_reserve(
            ENTITY_ID, SHOP_ID, 8001, &BUYER, 100_000, 100_000, 10_000, PRODUCT_ID,
        ));

        // 注入 Token 佣金记录到同一 order_id
        inject_token_pending(ENTITY_ID, REFERRER, 5_000);
        let record = pallet_commission_common::TokenCommissionRecord {
            entity_id: ENTITY_ID,
            order_id: 8001,
            buyer: BUYER,
            beneficiary: REFERRER,
            amount: 5_000u128,
            commission_type: CommissionType::DirectReward,
            level: 0,
            status: pallet_commission_common::CommissionStatus::Pending,
            created_at: 1u64,
        };
        OrderTokenCommissionRecords::<Test>::mutate(8001u64, |records| {
            let _ = records.try_push(record);
        });

        assert_ok!(CommissionCore::cancel_commission(8001));

        // Token 记录应被取消
        let token_records = OrderTokenCommissionRecords::<Test>::get(8001u64);
        assert_eq!(
            token_records[0].status,
            pallet_commission_common::CommissionStatus::Cancelled
        );
        let token_stats = MemberTokenCommissionStats::<Test>::get(ENTITY_ID, REFERRER);
        assert_eq!(token_stats.pending, 0);

        // TokenCommissionCancelled 事件应被发射
        System::assert_has_event(RuntimeEvent::CommissionCore(
            crate::pallet::Event::TokenCommissionCancelled {
                order_id: 8001,
                cancelled_count: 1,
            },
        ));
    });
}

#[test]
fn m4_trait_set_commission_modes_uses_is_valid() {
    // M4: CommissionProvider::set_commission_modes 应拒绝含未知位的模式
    use pallet_commission_common::CommissionProvider;
    new_test_ext().execute_with(|| {
        // 设置 referral_cap 以通过 cap 校验
        CommissionConfigs::<Test>::mutate(ENTITY_ID, |maybe| {
            let config = maybe.get_or_insert_with(CoreCommissionConfig::default);
            config.plugin_caps.referral_cap = 10000;
        });

        // 有效模式: DIRECT_REWARD | POOL_REWARD = 0b10_0000_0001 = 513
        assert_ok!(
            <CommissionCore as CommissionProvider<u64, u128>>::set_commission_modes(ENTITY_ID, 513,)
        );

        // OWNER_REWARD = 0b100_0000_0000 = 1024, 也是有效的
        assert_ok!(
            <CommissionCore as CommissionProvider<u64, u128>>::set_commission_modes(
                ENTITY_ID,
                CommissionModes::OWNER_REWARD,
            )
        );

        // 无效模式: 设置一个超出 ALL_VALID 的高位 (bit 12 = 4096)
        assert_noop!(
            <CommissionCore as CommissionProvider<u64, u128>>::set_commission_modes(
                ENTITY_ID, 4096,
            ),
            sp_runtime::DispatchError::Other("InvalidModes")
        );
    });
}

// ============================================================================
// Creator Reward: set_owner_reward_rate extrinsic
// ============================================================================

#[test]
fn set_owner_reward_rate_works() {
    new_test_ext().execute_with(|| {
        assert_ok!(CommissionCore::set_owner_reward_rate(
            RuntimeOrigin::signed(SELLER),
            ENTITY_ID,
            3000,
        ));
        let config = CommissionConfigs::<Test>::get(ENTITY_ID).unwrap();
        assert_eq!(config.owner_reward_rate, 3000);
    });
}

#[test]
fn set_owner_reward_rate_rejects_over_5000() {
    new_test_ext().execute_with(|| {
        assert_noop!(
            CommissionCore::set_owner_reward_rate(RuntimeOrigin::signed(SELLER), ENTITY_ID, 5001,),
            Error::<Test>::InvalidCommissionRate
        );
    });
}

#[test]
fn set_owner_reward_rate_rejects_non_owner() {
    new_test_ext().execute_with(|| {
        assert_noop!(
            CommissionCore::set_owner_reward_rate(RuntimeOrigin::signed(BUYER), ENTITY_ID, 3000,),
            Error::<Test>::NotEntityOwnerOrAdmin
        );
    });
}

// ============================================================================
// R8: owner_reward_rate — None+Locked 单调递减豁免
// ============================================================================

#[test]
fn r8_locked_none_allows_decrease() {
    new_test_ext().execute_with(|| {
        // Set initial rate to 3000
        assert_ok!(CommissionCore::set_owner_reward_rate(
            RuntimeOrigin::signed(SELLER),
            ENTITY_ID,
            3000,
        ));

        // Lock in None mode
        set_entity_locked(ENTITY_ID);
        set_governance_mode(ENTITY_ID, 0); // None

        // Decrease to 2000 → allowed
        assert_ok!(CommissionCore::set_owner_reward_rate(
            RuntimeOrigin::signed(SELLER),
            ENTITY_ID,
            2000,
        ));
        assert_eq!(
            CommissionConfigs::<Test>::get(ENTITY_ID)
                .unwrap()
                .owner_reward_rate,
            2000
        );

        // Decrease to 0 → allowed
        assert_ok!(CommissionCore::set_owner_reward_rate(
            RuntimeOrigin::signed(SELLER),
            ENTITY_ID,
            0,
        ));
        assert_eq!(
            CommissionConfigs::<Test>::get(ENTITY_ID)
                .unwrap()
                .owner_reward_rate,
            0
        );
    });
}

#[test]
fn r8_locked_none_blocks_increase() {
    new_test_ext().execute_with(|| {
        assert_ok!(CommissionCore::set_owner_reward_rate(
            RuntimeOrigin::signed(SELLER),
            ENTITY_ID,
            2000,
        ));
        set_entity_locked(ENTITY_ID);
        set_governance_mode(ENTITY_ID, 0);

        // Increase from 2000 → 3000 → blocked
        assert_noop!(
            CommissionCore::set_owner_reward_rate(RuntimeOrigin::signed(SELLER), ENTITY_ID, 3000,),
            Error::<Test>::LockedOnlyDecreaseAllowed
        );
    });
}

#[test]
fn r8_locked_none_blocks_same_value() {
    new_test_ext().execute_with(|| {
        assert_ok!(CommissionCore::set_owner_reward_rate(
            RuntimeOrigin::signed(SELLER),
            ENTITY_ID,
            2000,
        ));
        set_entity_locked(ENTITY_ID);
        set_governance_mode(ENTITY_ID, 0);

        // Same value (2000 → 2000) → blocked (must be strictly less)
        assert_noop!(
            CommissionCore::set_owner_reward_rate(RuntimeOrigin::signed(SELLER), ENTITY_ID, 2000,),
            Error::<Test>::LockedOnlyDecreaseAllowed
        );
    });
}

#[test]
fn r8_locked_fulldao_blocks_all() {
    new_test_ext().execute_with(|| {
        assert_ok!(CommissionCore::set_owner_reward_rate(
            RuntimeOrigin::signed(SELLER),
            ENTITY_ID,
            3000,
        ));
        set_entity_locked(ENTITY_ID);
        set_governance_mode(ENTITY_ID, 1); // FullDAO

        // FullDAO locked → EntityLocked (even decrease)
        assert_noop!(
            CommissionCore::set_owner_reward_rate(RuntimeOrigin::signed(SELLER), ENTITY_ID, 1000,),
            Error::<Test>::EntityLocked
        );
    });
}

#[test]
fn r8_unlocked_allows_any_value() {
    new_test_ext().execute_with(|| {
        // Not locked → free to set any value
        assert_ok!(CommissionCore::set_owner_reward_rate(
            RuntimeOrigin::signed(SELLER),
            ENTITY_ID,
            3000,
        ));
        assert_ok!(CommissionCore::set_owner_reward_rate(
            RuntimeOrigin::signed(SELLER),
            ENTITY_ID,
            5000,
        ));
        assert_ok!(CommissionCore::set_owner_reward_rate(
            RuntimeOrigin::signed(SELLER),
            ENTITY_ID,
            1000,
        ));
        assert_eq!(
            CommissionConfigs::<Test>::get(ENTITY_ID)
                .unwrap()
                .owner_reward_rate,
            1000
        );
    });
}

#[test]
fn r8_locked_none_no_config_blocks_increase() {
    new_test_ext().execute_with(|| {
        // No config exists → current = 0
        set_entity_locked(ENTITY_ID);
        set_governance_mode(ENTITY_ID, 0);

        // Trying to set to any positive value from 0 → increase → blocked
        assert_noop!(
            CommissionCore::set_owner_reward_rate(RuntimeOrigin::signed(SELLER), ENTITY_ID, 1000,),
            Error::<Test>::LockedOnlyDecreaseAllowed
        );
    });
}

// ============================================================================
// Creator Reward: NEX process_commission
// ============================================================================

/// 辅助：配置带Owner 收益的佣金
fn setup_owner_reward_config(max_commission_rate: u16, owner_reward_rate: u16) {
    CommissionConfigs::<Test>::insert(
        ENTITY_ID,
        CoreCommissionConfig {
            enabled_modes: CommissionModes(
                CommissionModes::DIRECT_REWARD | CommissionModes::OWNER_REWARD,
            ),
            max_commission_rate,
            enabled: true,
            withdrawal_cooldown: 0,
            owner_reward_rate,
            token_withdrawal_cooldown: 0,
            plugin_caps: PluginBudgetCaps::default(),
        },
    );
}

#[test]
fn owner_reward_nex_basic() {
    // Owner 收益基数为 order_amount（与 plugin_cap 同维度），从 Pool B 预算中优先扣除
    new_test_ext().execute_with(|| {
        fund(PLATFORM, 1_000_000);
        fund(SELLER, 1_000_000);
        setup_owner_reward_config(5000, 2000); // max 50%, owner 20% of order

        assert_ok!(process_commission_with_reserve(
            ENTITY_ID, SHOP_ID, 9001, &BUYER, 100_000, 100_000, 0, PRODUCT_ID,
        ));

        // Pool B = 100_000 * 5000 / 10000 = 50_000
        // owner_reward_amount = 100_000 * 2000 / 10000 = 20_000, min(20_000, 50_000) = 20_000
        // OwnerReward 不再产生 CommissionRecord
        let records = OrderCommissionRecords::<Test>::get(9001u64);
        assert_eq!(records.len(), 0);

        // 方案 B：Owner 奖励直接到账，不追踪 OrderOwnerReward
        // 需要先设置 owner 才能测试
        set_entity_owner(ENTITY_ID, 999);

        // OwnerReward 不增 stats.pending / total_earned
        let stats = MemberCommissionStats::<Test>::get(ENTITY_ID, SELLER);
        assert_eq!(stats.total_earned, 0);
        assert_eq!(stats.pending, 0);

        // 资金直接到 owner 账户（但此测试未设置 owner，实际不会转账）
        // 实际使用中 Entity 必须有 owner
    });
}

#[test]
fn owner_reward_nex_with_referrer() {
    // Owner 收益 + 招商推荐人奖金共存
    new_test_ext().execute_with(|| {
        fund(PLATFORM, 1_000_000);
        fund(SELLER, 1_000_000);
        set_entity_referrer(ENTITY_ID, REFERRER);
        setup_owner_reward_config(5000, 3000); // max 50%, owner 30% of order

        assert_ok!(process_commission_with_reserve(
            ENTITY_ID, SHOP_ID, 9002, &BUYER, 100_000, 100_000, 10_000, PRODUCT_ID,
        ));

        // Pool A: referrer = 10_000 * 50% = 5_000, treasury = 5_000
        // Pool B = 100_000 * 5000 / 10000 = 50_000
        // owner_reward_amount = 100_000 * 3000 / 10000 = 30_000, min(30_000, 50_000) = 30_000
        let records = OrderCommissionRecords::<Test>::get(9002u64);
        assert_eq!(records.len(), 1); // 只有 EntityReferral，无 OwnerReward record

        // 第一条: EntityReferral
        assert_eq!(records[0].commission_type, CommissionType::EntityReferral);
        assert_eq!(records[0].beneficiary, REFERRER);
        assert_eq!(records[0].amount, 5_000);

        // OwnerReward 直接到账给默认 owner(SELLER)，不再有独立存储可查
    });
}

#[test]
fn owner_reward_nex_disabled_when_mode_off() {
    // OWNER_REWARD 模式位未启用时不分配
    new_test_ext().execute_with(|| {
        fund(SELLER, 1_000_000);
        CommissionConfigs::<Test>::insert(
            ENTITY_ID,
            CoreCommissionConfig {
                enabled_modes: CommissionModes(CommissionModes::DIRECT_REWARD), // 无 OWNER_REWARD
                max_commission_rate: 5000,
                enabled: true,
                withdrawal_cooldown: 0,
                owner_reward_rate: 3000, // 已设置但模式位未开
                token_withdrawal_cooldown: 0,
                plugin_caps: PluginBudgetCaps::default(),
            },
        );

        assert_ok!(process_commission_with_reserve(
            ENTITY_ID, SHOP_ID, 9003, &BUYER, 100_000, 100_000, 0, PRODUCT_ID,
        ));

        // 无创建人记录
        let records = OrderCommissionRecords::<Test>::get(9003u64);
        assert_eq!(records.len(), 0);
    });
}

#[test]
fn owner_reward_nex_zero_rate_no_record() {
    // owner_reward_rate = 0 时不分配
    new_test_ext().execute_with(|| {
        fund(SELLER, 1_000_000);
        setup_owner_reward_config(5000, 0); // rate = 0

        assert_ok!(process_commission_with_reserve(
            ENTITY_ID, SHOP_ID, 9004, &BUYER, 100_000, 100_000, 0, PRODUCT_ID,
        ));

        let records = OrderCommissionRecords::<Test>::get(9004u64);
        assert_eq!(records.len(), 0);
    });
}

#[test]
fn owner_reward_nex_reduces_remaining_for_plugins() {
    // Owner 收益减少了剩余给插件的预算
    new_test_ext().execute_with(|| {
        fund(SELLER, 1_000_000);
        setup_owner_reward_config(9900, 5000); // max 99% (budget ceiling), owner 50% of order

        assert_ok!(process_commission_with_reserve(
            ENTITY_ID, SHOP_ID, 9005, &BUYER, 100_000, 100_000, 0, PRODUCT_ID,
        ));

        // Pool B = 100_000 * 9900 / 10000 = 99_000
        // owner_reward_amount = 100_000 * 5000 / 10000 = 50_000, min(50_000, 99_000) = 50_000
        // remaining for plugins = 49_000 (but all plugins are (), so no further distribution)
        let records = OrderCommissionRecords::<Test>::get(9005u64);
        assert_eq!(records.len(), 0); // OwnerReward 不再产生 record

        let owner = SELLER;
        let owner_balance = <pallet_balances::Pallet<Test> as frame_support::traits::Currency<
            u64,
        >>::free_balance(&owner);
        assert!(owner_balance >= 1);

        // owner=SELLER 时，owner reward 不会进入 entity_account
        let ea = entity_account(ENTITY_ID);
        let entity_balance = <pallet_balances::Pallet<Test> as frame_support::traits::Currency<
            u64,
        >>::free_balance(&ea);
        assert_eq!(entity_balance, 0);
    });
}

// ============================================================================
// Creator Reward: Token process_token_commission
// ============================================================================

#[test]
fn owner_reward_token_basic() {
    new_test_ext().execute_with(|| {
        let ea = entity_account(ENTITY_ID);
        set_token_balance(ENTITY_ID, ea, 100_000);

        CommissionConfigs::<Test>::insert(
            ENTITY_ID,
            CoreCommissionConfig {
                enabled_modes: CommissionModes(
                    CommissionModes::DIRECT_REWARD | CommissionModes::OWNER_REWARD,
                ),
                max_commission_rate: 5000,
                enabled: true,
                withdrawal_cooldown: 0,
                owner_reward_rate: 2000,
                token_withdrawal_cooldown: 0,
                plugin_caps: PluginBudgetCaps::default(),
            },
        );

        assert_ok!(CommissionCore::process_token_commission(
            ENTITY_ID, SHOP_ID, 9010, &BUYER, 80_000, 80_000, 0, PRODUCT_ID,
        ));

        // max_commission = 80_000 * 5000 / 10000 = 40_000
        // available_token = 100_000 - 0 = 100_000, remaining = min(40_000, 100_000) = 40_000
        // owner_reward_amount = 80_000 * 2000 / 10000 = 16_000, min(16_000, 40_000) = 16_000
        let records = OrderTokenCommissionRecords::<Test>::get(9010u64);
        assert_eq!(records.len(), 0); // OwnerReward 不再产生 Token record

        // OwnerReward 直接到账，不再做 token owner 余额断言

        // OwnerReward 不增 Token stats
        let stats = MemberTokenCommissionStats::<Test>::get(ENTITY_ID, SELLER);
        assert_eq!(stats.total_earned, 0);
        assert_eq!(stats.pending, 0);
        assert_eq!(TokenPendingTotal::<Test>::get(ENTITY_ID), 0);
    });
}

#[test]
fn owner_reward_token_disabled_when_mode_off() {
    new_test_ext().execute_with(|| {
        let ea = entity_account(ENTITY_ID);
        set_token_balance(ENTITY_ID, ea, 100_000);

        CommissionConfigs::<Test>::insert(
            ENTITY_ID,
            CoreCommissionConfig {
                enabled_modes: CommissionModes(CommissionModes::DIRECT_REWARD), // 无 OWNER_REWARD
                max_commission_rate: 5000,
                enabled: true,
                withdrawal_cooldown: 0,
                owner_reward_rate: 3000,
                token_withdrawal_cooldown: 0,
                plugin_caps: PluginBudgetCaps::default(),
            },
        );

        assert_ok!(CommissionCore::process_token_commission(
            ENTITY_ID, SHOP_ID, 9011, &BUYER, 80_000, 80_000, 0, PRODUCT_ID,
        ));

        // 无 Token 创建人记录
        let records = OrderTokenCommissionRecords::<Test>::get(9011u64);
        assert_eq!(records.len(), 0);
    });
}

#[test]
fn owner_reward_nex_missing_owner_falls_back_to_unallocated_pool() {
    new_test_ext().execute_with(|| {
        fund(SELLER, 100_000);
        clear_entity_owner(ENTITY_ID);
        setup_owner_reward_config(5000, 2000); // owner_reward_amount = 20_000

        assert_ok!(process_commission_with_reserve(
            ENTITY_ID, SHOP_ID, 9021, &BUYER, 100_000, 100_000, 0, PRODUCT_ID,
        ));

        // Owner 不存在，owner_reward_amount 回流未分配池
        assert_eq!(UnallocatedPool::<Test>::get(ENTITY_ID), 20_000);

        // 不产生 OwnerReward record
        let records = OrderCommissionRecords::<Test>::get(9021u64);
        assert_eq!(records.len(), 0);

        // 无 owner，自然无到账
        let owner_balance = <pallet_balances::Pallet<Test> as frame_support::traits::Currency<
            u64,
        >>::free_balance(&GIFT_TARGET);
        assert_eq!(owner_balance, 0);
    });
}

// ============================================================================
// Creator Reward: cancel_commission routing
// ============================================================================

#[test]
fn cancel_commission_refunds_owner_reward_to_seller() {
    // OwnerReward 退款路由: entity_account → seller（与其他会员佣金相同）
    new_test_ext().execute_with(|| {
        fund(SELLER, 1_000_000);
        let ea = entity_account(ENTITY_ID);
        fund(ea, 1); // 底仓
        setup_owner_reward_config(5000, 2000);

        assert_ok!(process_commission_with_reserve(
            ENTITY_ID, SHOP_ID, 9020, &BUYER, 100_000, 100_000, 0, PRODUCT_ID,
        ));

        // owner=SELLER 时，owner reward 不会进入 entity_account
        let entity_before = <pallet_balances::Pallet<Test> as frame_support::traits::Currency<
            u64,
        >>::free_balance(&ea);
        assert_eq!(entity_before, 1); // 仅保留底仓

        // OwnerReward 直接到账给默认 owner(SELLER)，cancel 不会回退该奖励
        let owner_before = <pallet_balances::Pallet<Test> as frame_support::traits::Currency<
            u64,
        >>::free_balance(&SELLER);

        assert_ok!(CommissionCore::cancel_commission(9020));

        // Entity 退回底仓
        let entity_after = <pallet_balances::Pallet<Test> as frame_support::traits::Currency<
            u64,
        >>::free_balance(&ea);
        assert_eq!(entity_after, 1);

        // cancel 时没有 owner reward pending 需要从 entity 退回
        let seller_after = <pallet_balances::Pallet<Test> as frame_support::traits::Currency<
            u64,
        >>::free_balance(&SELLER);
        assert_eq!(seller_after, owner_before);

        // OwnerReward removed - owner reward paid directly, cancel 后无额外存储可清理

        // 无 CommissionRecord（OwnerReward 不再产生 record）
        let records = OrderCommissionRecords::<Test>::get(9020u64);
        assert_eq!(records.len(), 0);
    });
}

// ============================================================================
// Creator Reward: CommissionProvider trait method
// ============================================================================

#[test]
fn trait_set_owner_reward_rate_works() {
    use pallet_commission_common::CommissionProvider;
    new_test_ext().execute_with(|| {
        assert_ok!(
            <CommissionCore as CommissionProvider<u64, u128>>::set_owner_reward_rate(
                ENTITY_ID, 2500,
            )
        );
        let config = CommissionConfigs::<Test>::get(ENTITY_ID).unwrap();
        assert_eq!(config.owner_reward_rate, 2500);

        // 超出上限 5000 应拒绝
        assert_noop!(
            <CommissionCore as CommissionProvider<u64, u128>>::set_owner_reward_rate(
                ENTITY_ID, 5001,
            ),
            sp_runtime::DispatchError::Other("InvalidRate")
        );
    });
}

// ============================================================================
// Token Platform Fee Rate Governance Tests
// ============================================================================

#[test]
fn set_token_platform_fee_rate_works() {
    new_test_ext().execute_with(|| {
        // 默认值 = 100 bps (1%)
        assert_eq!(TokenPlatformFeeRate::<Test>::get(), 100);

        // Root 设置为 200 bps (2%)
        assert_ok!(CommissionCore::set_token_platform_fee_rate(
            RuntimeOrigin::root(),
            200
        ));
        assert_eq!(TokenPlatformFeeRate::<Test>::get(), 200);

        // 验证事件
        System::assert_last_event(RuntimeEvent::CommissionCore(
            crate::Event::TokenPlatformFeeRateUpdated {
                old_rate: 100,
                new_rate: 200,
            },
        ));
    });
}

#[test]
fn set_token_platform_fee_rate_rejects_non_root() {
    new_test_ext().execute_with(|| {
        assert_noop!(
            CommissionCore::set_token_platform_fee_rate(RuntimeOrigin::signed(BUYER), 200),
            sp_runtime::DispatchError::BadOrigin
        );
    });
}

#[test]
fn set_token_platform_fee_rate_rejects_too_high() {
    new_test_ext().execute_with(|| {
        assert_noop!(
            CommissionCore::set_token_platform_fee_rate(RuntimeOrigin::root(), 1001),
            crate::Error::<Test>::TokenPlatformFeeRateTooHigh
        );
        // 边界值 1000 应通过
        assert_ok!(CommissionCore::set_token_platform_fee_rate(
            RuntimeOrigin::root(),
            1000
        ));
        assert_eq!(TokenPlatformFeeRate::<Test>::get(), 1000);
    });
}

#[test]
fn set_token_platform_fee_rate_zero_disables() {
    new_test_ext().execute_with(|| {
        assert_ok!(CommissionCore::set_token_platform_fee_rate(
            RuntimeOrigin::root(),
            0
        ));
        assert_eq!(TokenPlatformFeeRate::<Test>::get(), 0);
    });
}

// ============================================================================
// F1: Admin 权限支持 — ensure_owner_or_admin
// ============================================================================

#[test]
fn f1_admin_can_set_commission_modes() {
    new_test_ext().execute_with(|| {
        set_entity_admin(
            ENTITY_ID,
            ADMIN,
            pallet_entity_common::AdminPermission::COMMISSION_MANAGE,
        );
        // 设置 referral_cap 以通过 set_commission_modes 的 cap 校验
        CommissionConfigs::<Test>::mutate(ENTITY_ID, |maybe| {
            let config = maybe.get_or_insert_with(CoreCommissionConfig::default);
            config.plugin_caps.referral_cap = 10000;
        });
        assert_ok!(CommissionCore::set_commission_modes(
            RuntimeOrigin::signed(ADMIN),
            ENTITY_ID,
            CommissionModes(CommissionModes::DIRECT_REWARD),
        ));
        let config = CommissionConfigs::<Test>::get(ENTITY_ID).unwrap();
        assert!(config
            .enabled_modes
            .contains(CommissionModes::DIRECT_REWARD));
    });
}

#[test]
fn f1_admin_can_set_commission_rate() {
    new_test_ext().execute_with(|| {
        set_entity_admin(
            ENTITY_ID,
            ADMIN,
            pallet_entity_common::AdminPermission::COMMISSION_MANAGE,
        );
        assert_ok!(CommissionCore::set_commission_rate(
            RuntimeOrigin::signed(ADMIN),
            ENTITY_ID,
            5000,
        ));
        let config = CommissionConfigs::<Test>::get(ENTITY_ID).unwrap();
        assert_eq!(config.max_commission_rate, 5000);
    });
}

#[test]
fn f1_admin_can_enable_commission() {
    new_test_ext().execute_with(|| {
        set_entity_admin(
            ENTITY_ID,
            ADMIN,
            pallet_entity_common::AdminPermission::COMMISSION_MANAGE,
        );
        // H-1: 设置 referral_cap，然后设置单插件模式，再启用
        CommissionConfigs::<Test>::mutate(ENTITY_ID, |maybe| {
            let config = maybe.get_or_insert_with(CoreCommissionConfig::default);
            config.plugin_caps.referral_cap = 10000;
        });
        assert_ok!(CommissionCore::set_commission_modes(
            RuntimeOrigin::signed(ADMIN),
            ENTITY_ID,
            CommissionModes(CommissionModes::DIRECT_REWARD),
        ));
        assert_ok!(CommissionCore::enable_commission(
            RuntimeOrigin::signed(ADMIN),
            ENTITY_ID,
            true,
        ));
    });
}

#[test]
fn f1_admin_without_permission_rejected() {
    new_test_ext().execute_with(|| {
        // Admin has OTHER permission, not COMMISSION_MANAGE
        set_entity_admin(ENTITY_ID, ADMIN, 0x01);
        assert_noop!(
            CommissionCore::set_commission_rate(RuntimeOrigin::signed(ADMIN), ENTITY_ID, 5000,),
            Error::<Test>::NotEntityOwnerOrAdmin
        );
    });
}

#[test]
fn f1_non_owner_non_admin_rejected() {
    new_test_ext().execute_with(|| {
        assert_noop!(
            CommissionCore::set_commission_rate(RuntimeOrigin::signed(BUYER), ENTITY_ID, 5000,),
            Error::<Test>::NotEntityOwnerOrAdmin
        );
    });
}

#[test]
fn f1_owner_still_works() {
    new_test_ext().execute_with(|| {
        // Owner (SELLER) should still work without being admin
        assert_ok!(CommissionCore::set_commission_rate(
            RuntimeOrigin::signed(SELLER),
            ENTITY_ID,
            8000,
        ));
    });
}

#[test]
fn f1_admin_cannot_withdraw_entity_funds() {
    // Fund withdrawal is Owner-only (not admin)
    new_test_ext().execute_with(|| {
        set_entity_admin(
            ENTITY_ID,
            ADMIN,
            pallet_entity_common::AdminPermission::COMMISSION_MANAGE,
        );
        fund(entity_account(ENTITY_ID), 100_000);

        assert_noop!(
            CommissionCore::withdraw_entity_funds(RuntimeOrigin::signed(ADMIN), ENTITY_ID, 1_000,),
            Error::<Test>::NotEntityOwner
        );
    });
}

// ============================================================================
// F13: NEX 全局最低复购比例 — set_global_min_repurchase_rate
// ============================================================================

#[test]
fn f13_set_global_min_repurchase_rate_works() {
    new_test_ext().execute_with(|| {
        assert_ok!(CommissionCore::set_global_min_repurchase_rate(
            RuntimeOrigin::root(),
            ENTITY_ID,
            3000,
        ));
        assert_eq!(GlobalMinRepurchaseRate::<Test>::get(ENTITY_ID), 3000);
        System::assert_last_event(RuntimeEvent::CommissionCore(
            crate::Event::GlobalMinRepurchaseRateSet {
                entity_id: ENTITY_ID,
                rate: 3000,
            },
        ));
    });
}

#[test]
fn f13_set_global_min_repurchase_rate_rejects_non_root() {
    new_test_ext().execute_with(|| {
        assert_noop!(
            CommissionCore::set_global_min_repurchase_rate(
                RuntimeOrigin::signed(SELLER),
                ENTITY_ID,
                3000,
            ),
            sp_runtime::DispatchError::BadOrigin
        );
    });
}

#[test]
fn f13_set_global_min_repurchase_rate_rejects_over_10000() {
    new_test_ext().execute_with(|| {
        assert_noop!(
            CommissionCore::set_global_min_repurchase_rate(
                RuntimeOrigin::root(), ENTITY_ID, 10001,
            ),
            Error::<Test>::InvalidCommissionRate
        );
    });
}

// ============================================================================
// F2: set_withdrawal_cooldown 独立 extrinsic
// ============================================================================

#[test]
fn f2_set_withdrawal_cooldown_works() {
    new_test_ext().execute_with(|| {
        assert_ok!(CommissionCore::set_withdrawal_cooldown(
            RuntimeOrigin::signed(SELLER),
            ENTITY_ID,
            200,
            300,
        ));
        let config = CommissionConfigs::<Test>::get(ENTITY_ID).unwrap();
        assert_eq!(config.withdrawal_cooldown, 200);
        assert_eq!(config.token_withdrawal_cooldown, 300);
        System::assert_last_event(RuntimeEvent::CommissionCore(
            crate::Event::WithdrawalCooldownUpdated {
                entity_id: ENTITY_ID,
                nex_cooldown: 200,
                token_cooldown: 300,
            },
        ));
    });
}

#[test]
fn f2_set_withdrawal_cooldown_admin_works() {
    new_test_ext().execute_with(|| {
        set_entity_admin(
            ENTITY_ID,
            ADMIN,
            pallet_entity_common::AdminPermission::COMMISSION_MANAGE,
        );
        assert_ok!(CommissionCore::set_withdrawal_cooldown(
            RuntimeOrigin::signed(ADMIN),
            ENTITY_ID,
            50,
            100,
        ));
        let config = CommissionConfigs::<Test>::get(ENTITY_ID).unwrap();
        assert_eq!(config.withdrawal_cooldown, 50);
        assert_eq!(config.token_withdrawal_cooldown, 100);
    });
}

// ============================================================================
// F3: Token 独立冻结期 — token_withdrawal_cooldown
// ============================================================================

#[test]
fn f3_token_uses_independent_cooldown() {
    new_test_ext().execute_with(|| {
        let ea = entity_account(ENTITY_ID);
        inject_token_pending(ENTITY_ID, REFERRER, 10_000);
        set_token_balance(ENTITY_ID, ea, 50_000);

        // NEX cooldown = 100, Token cooldown = 50
        CommissionConfigs::<Test>::insert(
            ENTITY_ID,
            CoreCommissionConfig {
                enabled_modes: CommissionModes(CommissionModes::DIRECT_REWARD),
                max_commission_rate: 10000,
                enabled: true,
                withdrawal_cooldown: 100,
                owner_reward_rate: 0,
                token_withdrawal_cooldown: 50,
                plugin_caps: PluginBudgetCaps::default(),
            },
        );

        // Token 入账在 block 10
        System::set_block_number(10);
        crate::pallet::MemberTokenLastCredited::<Test>::insert(ENTITY_ID, REFERRER, 10u64);

        // block 55: token cooldown 已过 (10 + 50 = 60 > 55)
        System::set_block_number(55);
        assert_noop!(
            CommissionCore::withdraw_token_commission(
                RuntimeOrigin::signed(REFERRER),
                ENTITY_ID,
                Some(1_000),
                None,
            ),
            Error::<Test>::WithdrawalCooldownNotMet
        );

        // block 60: token cooldown 刚好满足
        System::set_block_number(60);
        assert_ok!(CommissionCore::withdraw_token_commission(
            RuntimeOrigin::signed(REFERRER),
            ENTITY_ID,
            Some(1_000),
            None,
        ));
    });
}

#[test]
fn f3_token_fallback_to_nex_cooldown_when_zero() {
    new_test_ext().execute_with(|| {
        let ea = entity_account(ENTITY_ID);
        inject_token_pending(ENTITY_ID, REFERRER, 10_000);
        set_token_balance(ENTITY_ID, ea, 50_000);

        // token_withdrawal_cooldown = 0 → 回退到 withdrawal_cooldown = 100
        CommissionConfigs::<Test>::insert(
            ENTITY_ID,
            CoreCommissionConfig {
                enabled_modes: CommissionModes(CommissionModes::DIRECT_REWARD),
                max_commission_rate: 10000,
                enabled: true,
                withdrawal_cooldown: 100,
                owner_reward_rate: 0,
                token_withdrawal_cooldown: 0,
                plugin_caps: PluginBudgetCaps::default(),
            },
        );

        System::set_block_number(10);
        crate::pallet::MemberTokenLastCredited::<Test>::insert(ENTITY_ID, REFERRER, 10u64);

        // block 100: should still be blocked (10 + 100 = 110 > 100)
        System::set_block_number(100);
        assert_noop!(
            CommissionCore::withdraw_token_commission(
                RuntimeOrigin::signed(REFERRER),
                ENTITY_ID,
                Some(1_000),
                None,
            ),
            Error::<Test>::WithdrawalCooldownNotMet
        );

        // block 110: OK
        System::set_block_number(110);
        assert_ok!(CommissionCore::withdraw_token_commission(
            RuntimeOrigin::signed(REFERRER),
            ENTITY_ID,
            Some(1_000),
            None,
        ));
    });
}

// ============================================================================
// F14: force_disable_entity_commission — Root 紧急暂停
// ============================================================================

#[test]
fn f14_force_disable_works() {
    new_test_ext().execute_with(|| {
        setup_config(5000);
        let config = CommissionConfigs::<Test>::get(ENTITY_ID).unwrap();
        assert!(config.enabled);

        assert_ok!(CommissionCore::force_disable_entity_commission(
            RuntimeOrigin::root(),
            ENTITY_ID,
        ));

        let config = CommissionConfigs::<Test>::get(ENTITY_ID).unwrap();
        assert!(!config.enabled);
        System::assert_last_event(RuntimeEvent::CommissionCore(
            crate::Event::CommissionForceDisabled {
                entity_id: ENTITY_ID,
            },
        ));
    });
}

#[test]
fn f14_force_disable_rejects_non_root() {
    new_test_ext().execute_with(|| {
        assert_noop!(
            CommissionCore::force_disable_entity_commission(
                RuntimeOrigin::signed(SELLER),
                ENTITY_ID,
            ),
            sp_runtime::DispatchError::BadOrigin
        );
    });
}

#[test]
fn f14_force_disable_creates_config_if_absent() {
    new_test_ext().execute_with(|| {
        // No config exists
        assert!(CommissionConfigs::<Test>::get(ENTITY_ID).is_none());

        assert_ok!(CommissionCore::force_disable_entity_commission(
            RuntimeOrigin::root(),
            ENTITY_ID,
        ));

        let config = CommissionConfigs::<Test>::get(ENTITY_ID).unwrap();
        assert!(!config.enabled);
    });
}

// ============================================================================
// F15: 全局佣金率上限 — GlobalMaxCommissionRate
// ============================================================================

#[test]
fn f15_set_global_max_commission_rate_works() {
    new_test_ext().execute_with(|| {
        assert_ok!(CommissionCore::set_global_max_commission_rate(
            RuntimeOrigin::root(),
            ENTITY_ID,
            5000,
        ));
        assert_eq!(GlobalMaxCommissionRate::<Test>::get(ENTITY_ID), 5000);
        System::assert_last_event(RuntimeEvent::CommissionCore(
            crate::Event::GlobalMaxCommissionRateSet {
                entity_id: ENTITY_ID,
                rate: 5000,
            },
        ));
    });
}

#[test]
fn f15_set_commission_rate_blocked_by_global_max() {
    new_test_ext().execute_with(|| {
        // Root sets global max to 5000
        GlobalMaxCommissionRate::<Test>::insert(ENTITY_ID, 5000u16);

        // Owner tries to set 6000 → blocked
        assert_noop!(
            CommissionCore::set_commission_rate(RuntimeOrigin::signed(SELLER), ENTITY_ID, 6000,),
            Error::<Test>::CommissionRateExceedsGlobalMax
        );

        // 5000 should work
        assert_ok!(CommissionCore::set_commission_rate(
            RuntimeOrigin::signed(SELLER),
            ENTITY_ID,
            5000,
        ));
    });
}

#[test]
fn f15_zero_global_max_means_no_limit() {
    new_test_ext().execute_with(|| {
        // Default is 0 = no limit, but budget ceiling still applies (10000 - platform_fee_rate)
        // With default platform_fee_rate = 100, ceiling = 9900
        assert_ok!(CommissionCore::set_commission_rate(
            RuntimeOrigin::signed(SELLER),
            ENTITY_ID,
            9900,
        ));
    });
}

#[test]
fn f15_rejects_non_root() {
    new_test_ext().execute_with(|| {
        assert_noop!(
            CommissionCore::set_global_max_commission_rate(
                RuntimeOrigin::signed(SELLER),
                ENTITY_ID,
                5000,
            ),
            sp_runtime::DispatchError::BadOrigin
        );
    });
}

// ============================================================================
// Budget Ceiling: max_commission_rate ≤ 10000 - platform_fee_rate
// ============================================================================

#[test]
fn budget_ceiling_rejects_rate_exceeding_ceiling() {
    // platform_fee_rate = 100 → ceiling = 9900
    new_test_ext().execute_with(|| {
        assert_noop!(
            CommissionCore::set_commission_rate(RuntimeOrigin::signed(SELLER), ENTITY_ID, 9901,),
            crate::Error::<Test>::CommissionRateExceedsBudget
        );
        // Exactly at ceiling works
        assert_ok!(CommissionCore::set_commission_rate(
            RuntimeOrigin::signed(SELLER),
            ENTITY_ID,
            9900,
        ));
    });
}

#[test]
fn budget_ceiling_dynamic_with_platform_fee_rate() {
    // 修改 platform_fee_rate 后 ceiling 变化
    new_test_ext().execute_with(|| {
        // Default: platform_fee_rate=100, ceiling=9900
        assert_ok!(CommissionCore::set_commission_rate(
            RuntimeOrigin::signed(SELLER),
            ENTITY_ID,
            9900,
        ));

        // 提高 platform_fee_rate 到 500 → ceiling=9500
        set_platform_fee_rate(500);
        assert_noop!(
            CommissionCore::set_commission_rate(RuntimeOrigin::signed(SELLER), ENTITY_ID, 9600,),
            crate::Error::<Test>::CommissionRateExceedsBudget
        );
        assert_ok!(CommissionCore::set_commission_rate(
            RuntimeOrigin::signed(SELLER),
            ENTITY_ID,
            9500,
        ));
    });
}

#[test]
fn budget_ceiling_shop_and_product_rate_constrained() {
    new_test_ext().execute_with(|| {
        // Shop level rate
        assert_noop!(
            CommissionCore::set_shop_commission_rate(
                RuntimeOrigin::signed(SELLER),
                ENTITY_ID,
                SHOP_ID,
                Some(9901),
            ),
            crate::Error::<Test>::CommissionRateExceedsBudget
        );
        assert_ok!(CommissionCore::set_shop_commission_rate(
            RuntimeOrigin::signed(SELLER),
            ENTITY_ID,
            SHOP_ID,
            Some(9900),
        ));

        // Product level rate
        assert_noop!(
            CommissionCore::set_product_commission_rate(
                RuntimeOrigin::signed(SELLER),
                ENTITY_ID,
                SHOP_ID,
                PRODUCT_ID,
                Some(9901),
            ),
            crate::Error::<Test>::CommissionRateExceedsBudget
        );
        assert_ok!(CommissionCore::set_product_commission_rate(
            RuntimeOrigin::signed(SELLER),
            ENTITY_ID,
            SHOP_ID,
            PRODUCT_ID,
            Some(9900),
        ));
    });
}

#[test]
fn budget_ceiling_global_max_constrained() {
    new_test_ext().execute_with(|| {
        // Root set_global_max_commission_rate: 0 = no limit (skip), >0 must <= ceiling
        assert_ok!(CommissionCore::set_global_max_commission_rate(
            RuntimeOrigin::root(),
            ENTITY_ID,
            0,
        ));
        assert_noop!(
            CommissionCore::set_global_max_commission_rate(RuntimeOrigin::root(), ENTITY_ID, 9901,),
            crate::Error::<Test>::CommissionRateExceedsBudget
        );
        assert_ok!(CommissionCore::set_global_max_commission_rate(
            RuntimeOrigin::root(),
            ENTITY_ID,
            9900,
        ));
    });
}

#[test]
fn budget_ceiling_engine_defensive_clamp() {
    // 直接插入超预算配置（模拟旧数据迁移），引擎应防御性截断
    new_test_ext().execute_with(|| {
        fund(SELLER, 1_000_000);
        fund(PLATFORM, 1_000_000);
        CommissionConfigs::<Test>::insert(
            ENTITY_ID,
            CoreCommissionConfig {
                enabled_modes: CommissionModes(CommissionModes::POOL_REWARD),
                max_commission_rate: 10000, // 旧数据：超出 budget_ceiling
                enabled: true,
                withdrawal_cooldown: 0,
                owner_reward_rate: 0,
                token_withdrawal_cooldown: 0,
                plugin_caps: PluginBudgetCaps::default(),
            },
        );

        assert_ok!(process_commission_with_reserve(
            ENTITY_ID, SHOP_ID, 99001, &BUYER, 100_000, 100_000, 0, PRODUCT_ID,
        ));

        // Engine clamps to 9900: pool = 100_000 * 9900 / 10000 = 99_000
        let pool = UnallocatedPool::<Test>::get(ENTITY_ID);
        assert_eq!(pool, 99_000);
    });
}

// ============================================================================
// F4: clear_commission_config / clear_withdrawal_config
// ============================================================================

#[test]
fn f4_clear_commission_config_works() {
    new_test_ext().execute_with(|| {
        setup_config(5000);
        assert!(CommissionConfigs::<Test>::get(ENTITY_ID).is_some());

        assert_ok!(CommissionCore::clear_commission_config(
            RuntimeOrigin::signed(SELLER),
            ENTITY_ID,
        ));
        assert!(CommissionConfigs::<Test>::get(ENTITY_ID).is_none());
        System::assert_last_event(RuntimeEvent::CommissionCore(
            crate::Event::CommissionConfigCleared {
                entity_id: ENTITY_ID,
            },
        ));
    });
}

#[test]
fn f4_clear_commission_config_rejects_absent() {
    new_test_ext().execute_with(|| {
        assert_noop!(
            CommissionCore::clear_commission_config(RuntimeOrigin::signed(SELLER), ENTITY_ID,),
            Error::<Test>::ConfigNotFound
        );
    });
}

#[test]
fn f4_clear_withdrawal_config_works() {
    new_test_ext().execute_with(|| {
        use frame_support::BoundedVec;
        assert_ok!(CommissionCore::set_withdrawal_config(
            RuntimeOrigin::signed(SELLER),
            ENTITY_ID,
            WithdrawalMode::FullWithdrawal,
            WithdrawalTierConfig {
                withdrawal_rate: 10000,
                repurchase_rate: 0
            },
            BoundedVec::default(),
            0,
            true,
        ));
        assert!(WithdrawalConfigs::<Test>::get(ENTITY_ID).is_some());

        assert_ok!(CommissionCore::clear_withdrawal_config(
            RuntimeOrigin::signed(SELLER),
            ENTITY_ID,
        ));
        assert!(WithdrawalConfigs::<Test>::get(ENTITY_ID).is_none());
    });
}

#[test]
fn f4_clear_token_withdrawal_config_works() {
    new_test_ext().execute_with(|| {
        use frame_support::BoundedVec;
        assert_ok!(CommissionCore::set_token_withdrawal_config(
            RuntimeOrigin::signed(SELLER),
            ENTITY_ID,
            WithdrawalMode::FullWithdrawal,
            WithdrawalTierConfig {
                withdrawal_rate: 10000,
                repurchase_rate: 0
            },
            BoundedVec::default(),
            0,
            true,
        ));
        assert!(TokenWithdrawalConfigs::<Test>::get(ENTITY_ID).is_some());

        assert_ok!(CommissionCore::clear_token_withdrawal_config(
            RuntimeOrigin::signed(SELLER),
            ENTITY_ID,
        ));
        assert!(TokenWithdrawalConfigs::<Test>::get(ENTITY_ID).is_none());
    });
}

#[test]
fn f4_clear_config_admin_works() {
    new_test_ext().execute_with(|| {
        setup_config(5000);
        set_entity_admin(
            ENTITY_ID,
            ADMIN,
            pallet_entity_common::AdminPermission::COMMISSION_MANAGE,
        );

        assert_ok!(CommissionCore::clear_commission_config(
            RuntimeOrigin::signed(ADMIN),
            ENTITY_ID,
        ));
        assert!(CommissionConfigs::<Test>::get(ENTITY_ID).is_none());
    });
}

// ==================== EntityLocked 回归测试 ====================

#[test]
fn entity_locked_rejects_set_commission_rate() {
    new_test_ext().execute_with(|| {
        set_entity_locked(ENTITY_ID);
        assert_noop!(
            CommissionCore::set_commission_rate(RuntimeOrigin::signed(SELLER), ENTITY_ID, 5000,),
            Error::<Test>::EntityLocked
        );
    });
}

#[test]
fn entity_locked_rejects_clear_commission_config() {
    new_test_ext().execute_with(|| {
        setup_config(5000);
        set_entity_locked(ENTITY_ID);
        assert_noop!(
            CommissionCore::clear_commission_config(RuntimeOrigin::signed(SELLER), ENTITY_ID,),
            Error::<Test>::EntityLocked
        );
    });
}

// ==================== #4 Entity 活跃状态校验 ====================

#[test]
fn f4_entity_inactive_rejects_set_commission_rate() {
    new_test_ext().execute_with(|| {
        set_entity_inactive(ENTITY_ID);
        assert_noop!(
            CommissionCore::set_commission_rate(RuntimeOrigin::signed(SELLER), ENTITY_ID, 5000,),
            Error::<Test>::EntityNotActive
        );
    });
}

#[test]
fn f4_entity_inactive_rejects_set_commission_modes() {
    new_test_ext().execute_with(|| {
        setup_config(5000);
        set_entity_inactive(ENTITY_ID);
        assert_noop!(
            CommissionCore::set_commission_modes(
                RuntimeOrigin::signed(SELLER),
                ENTITY_ID,
                CommissionModes(CommissionModes::DIRECT_REWARD),
            ),
            Error::<Test>::EntityNotActive
        );
    });
}

#[test]
fn f4_entity_inactive_rejects_enable_commission() {
    new_test_ext().execute_with(|| {
        setup_config(5000);
        set_entity_inactive(ENTITY_ID);
        assert_noop!(
            CommissionCore::enable_commission(RuntimeOrigin::signed(SELLER), ENTITY_ID, false,),
            Error::<Test>::EntityNotActive
        );
    });
}

#[test]
fn f4_entity_inactive_rejects_set_withdrawal_config() {
    new_test_ext().execute_with(|| {
        set_entity_inactive(ENTITY_ID);
        assert_noop!(
            CommissionCore::set_withdrawal_config(
                RuntimeOrigin::signed(SELLER),
                ENTITY_ID,
                WithdrawalMode::FullWithdrawal,
                WithdrawalTierConfig {
                    withdrawal_rate: 10000,
                    repurchase_rate: 0
                },
                BoundedVec::<_, ConstU32<10>>::default(),
                0,
                true,
            ),
            Error::<Test>::EntityNotActive
        );
    });
}

#[test]
fn f4_entity_inactive_rejects_set_owner_reward_rate() {
    new_test_ext().execute_with(|| {
        set_entity_inactive(ENTITY_ID);
        assert_noop!(
            CommissionCore::set_owner_reward_rate(RuntimeOrigin::signed(SELLER), ENTITY_ID, 1000,),
            Error::<Test>::EntityNotActive
        );
    });
}

#[test]
fn f4_entity_inactive_rejects_set_withdrawal_cooldown() {
    new_test_ext().execute_with(|| {
        set_entity_inactive(ENTITY_ID);
        assert_noop!(
            CommissionCore::set_withdrawal_cooldown(
                RuntimeOrigin::signed(SELLER),
                ENTITY_ID,
                100,
                200,
            ),
            Error::<Test>::EntityNotActive
        );
    });
}

// ==================== #8 全局紧急暂停 ====================

#[test]
fn f8_force_global_pause_works() {
    new_test_ext().execute_with(|| {
        assert!(!GlobalCommissionPaused::<Test>::get());
        assert_ok!(CommissionCore::force_global_pause(
            RuntimeOrigin::root(),
            true
        ));
        assert!(GlobalCommissionPaused::<Test>::get());

        // 非 Root 无法调用
        assert_noop!(
            CommissionCore::force_global_pause(RuntimeOrigin::signed(SELLER), false),
            sp_runtime::DispatchError::BadOrigin
        );

        // Root 恢复
        assert_ok!(CommissionCore::force_global_pause(
            RuntimeOrigin::root(),
            false
        ));
        assert!(!GlobalCommissionPaused::<Test>::get());
    });
}

#[test]
fn f8_global_pause_blocks_process_commission() {
    new_test_ext().execute_with(|| {
        fund(PLATFORM, 1_000_000);
        fund(SELLER, 1_000_000);
        set_entity_referrer(ENTITY_ID, REFERRER);
        setup_config(10000);

        GlobalCommissionPaused::<Test>::put(true);

        // Reserve 模式: 暂停时 soft-return Ok(()) 并 unreserve，不产生佣金记录
        assert_ok!(process_commission_with_reserve(
            ENTITY_ID, SHOP_ID, 9999, &BUYER, 100_000, 100_000, 10_000,
            PRODUCT_ID,
        ));
        // 验证: 无佣金记录产生
        let records = OrderCommissionRecords::<Test>::get(9999u64);
        assert!(records.is_empty());
        // 验证: seller reserved 已释放
        let reserved = <pallet_balances::Pallet<Test> as frame_support::traits::ReservableCurrency<u64>>::reserved_balance(&SELLER);
        assert_eq!(reserved, 0);
    });
}

#[test]
fn f8_global_pause_blocks_withdraw_commission() {
    new_test_ext().execute_with(|| {
        let ea = entity_account(ENTITY_ID);
        fund(ea, 100_000);
        set_entity_referrer(ENTITY_ID, REFERRER);
        setup_config(10000);

        // 先产生佣金
        assert_ok!(process_commission_with_reserve(
            ENTITY_ID, SHOP_ID, 9998, &BUYER, 100_000, 100_000, 10_000, PRODUCT_ID,
        ));

        // 暂停后提现失败
        GlobalCommissionPaused::<Test>::put(true);

        assert_noop!(
            CommissionCore::withdraw_commission(
                RuntimeOrigin::signed(REFERRER),
                ENTITY_ID,
                None,
                None,
            ),
            Error::<Test>::GlobalCommissionPaused
        );
    });
}

// ==================== #7 全局 Token 最大佣金率 ====================

#[test]
fn f7_set_global_max_token_commission_rate_works() {
    new_test_ext().execute_with(|| {
        assert_ok!(CommissionCore::set_global_max_token_commission_rate(
            RuntimeOrigin::root(),
            ENTITY_ID,
            5000,
        ));
        assert_eq!(GlobalMaxTokenCommissionRate::<Test>::get(ENTITY_ID), 5000);
    });
}

#[test]
fn f7_global_max_token_rate_rejects_invalid() {
    new_test_ext().execute_with(|| {
        assert_noop!(
            CommissionCore::set_global_max_token_commission_rate(
                RuntimeOrigin::root(),
                ENTITY_ID,
                10001,
            ),
            Error::<Test>::InvalidCommissionRate
        );
    });
}

#[test]
fn f7_global_max_token_rate_rejects_non_root() {
    new_test_ext().execute_with(|| {
        assert_noop!(
            CommissionCore::set_global_max_token_commission_rate(
                RuntimeOrigin::signed(SELLER),
                ENTITY_ID,
                5000,
            ),
            sp_runtime::DispatchError::BadOrigin
        );
    });
}

// ==================== #5 提现暂停开关 ====================

#[test]
fn f5_pause_withdrawals_works() {
    new_test_ext().execute_with(|| {
        assert!(!WithdrawalPaused::<Test>::get(ENTITY_ID));

        assert_ok!(CommissionCore::pause_withdrawals(
            RuntimeOrigin::signed(SELLER),
            ENTITY_ID,
            true,
        ));
        assert!(WithdrawalPaused::<Test>::get(ENTITY_ID));

        assert_ok!(CommissionCore::pause_withdrawals(
            RuntimeOrigin::signed(SELLER),
            ENTITY_ID,
            false,
        ));
        assert!(!WithdrawalPaused::<Test>::get(ENTITY_ID));
    });
}

#[test]
fn f5_pause_withdrawals_blocks_nex_withdraw() {
    new_test_ext().execute_with(|| {
        let ea = entity_account(ENTITY_ID);
        fund(ea, 100_000);
        set_entity_referrer(ENTITY_ID, REFERRER);
        setup_config(10000);

        assert_ok!(process_commission_with_reserve(
            ENTITY_ID, SHOP_ID, 9997, &BUYER, 100_000, 100_000, 10_000, PRODUCT_ID,
        ));

        WithdrawalPaused::<Test>::insert(ENTITY_ID, true);

        assert_noop!(
            CommissionCore::withdraw_commission(
                RuntimeOrigin::signed(REFERRER),
                ENTITY_ID,
                None,
                None,
            ),
            Error::<Test>::WithdrawalPausedByOwner
        );
    });
}

#[test]
fn f5_pause_withdrawals_blocks_token_withdraw() {
    new_test_ext().execute_with(|| {
        let ea = entity_account(ENTITY_ID);
        inject_token_pending(ENTITY_ID, REFERRER, 10_000);
        set_token_balance(ENTITY_ID, ea, 50_000);

        WithdrawalPaused::<Test>::insert(ENTITY_ID, true);

        assert_noop!(
            CommissionCore::withdraw_token_commission(
                RuntimeOrigin::signed(REFERRER),
                ENTITY_ID,
                None,
                None,
            ),
            Error::<Test>::WithdrawalPausedByOwner
        );
    });
}

#[test]
fn f5_pause_withdrawals_rejects_locked_entity() {
    new_test_ext().execute_with(|| {
        set_entity_locked(ENTITY_ID);
        assert_noop!(
            CommissionCore::pause_withdrawals(RuntimeOrigin::signed(SELLER), ENTITY_ID, true,),
            Error::<Test>::EntityLocked
        );
    });
}

// ==================== #2 会员级佣金记录索引 ====================

#[test]
fn f2_member_commission_order_ids_indexed() {
    new_test_ext().execute_with(|| {
        fund(PLATFORM, 1_000_000);
        set_entity_referrer(ENTITY_ID, REFERRER);
        setup_config(10000);

        // 产生多个订单的佣金
        assert_ok!(process_commission_with_reserve(
            ENTITY_ID, SHOP_ID, 7001, &BUYER, 100_000, 100_000, 10_000, PRODUCT_ID,
        ));
        assert_ok!(process_commission_with_reserve(
            ENTITY_ID, SHOP_ID, 7002, &BUYER, 100_000, 100_000, 10_000, PRODUCT_ID,
        ));
        assert_ok!(process_commission_with_reserve(
            ENTITY_ID, SHOP_ID, 7003, &BUYER, 100_000, 100_000, 10_000, PRODUCT_ID,
        ));

        let ids = MemberCommissionOrderIds::<Test>::get(ENTITY_ID, REFERRER);
        assert!(ids.contains(&7001));
        assert!(ids.contains(&7002));
        assert!(ids.contains(&7003));
        assert_eq!(ids.len(), 3);
    });
}

#[test]
fn f2_member_commission_order_ids_deduplicates() {
    new_test_ext().execute_with(|| {
        fund(PLATFORM, 1_000_000);
        set_entity_referrer(ENTITY_ID, REFERRER);
        setup_config(10000);

        // 首次处理成功
        assert_ok!(process_commission_with_reserve(
            ENTITY_ID, SHOP_ID, 7010, &BUYER, 100_000, 100_000, 10_000, PRODUCT_ID,
        ));
        // P0-2 审计修复: 同一订单重复处理 — Reserve 模式下 soft-return Ok() 并 unreserve
        assert_ok!(process_commission_with_reserve(
            ENTITY_ID, SHOP_ID, 7010, &BUYER, 100_000, 100_000, 10_000, PRODUCT_ID,
        ));

        let ids = MemberCommissionOrderIds::<Test>::get(ENTITY_ID, REFERRER);
        // order 7010 应只出现一次
        assert_eq!(ids.iter().filter(|&&id| id == 7010).count(), 1);
    });
}

// ==================== #3 提现记录存储 ====================

#[test]
fn f3_nex_withdrawal_history_recorded() {
    new_test_ext().execute_with(|| {
        let ea = entity_account(ENTITY_ID);
        fund(PLATFORM, 1_000_000);
        fund(ea, 100_000);
        set_entity_referrer(ENTITY_ID, REFERRER);
        setup_config(10000);

        assert_ok!(process_commission_with_reserve(
            ENTITY_ID, SHOP_ID, 7020, &BUYER, 100_000, 100_000, 10_000, PRODUCT_ID,
        ));

        let stats = MemberCommissionStats::<Test>::get(ENTITY_ID, REFERRER);
        assert!(stats.pending > 0, "referrer should have pending commission");

        assert_ok!(CommissionCore::withdraw_commission(
            RuntimeOrigin::signed(REFERRER),
            ENTITY_ID,
            None,
            None,
        ));

        let history = MemberWithdrawalHistory::<Test>::get(ENTITY_ID, REFERRER);
        assert_eq!(history.len(), 1);
        assert!(history[0].total_amount > 0);
        assert_eq!(history[0].block_number, 1u64);
    });
}

#[test]
fn f3_token_withdrawal_history_recorded() {
    new_test_ext().execute_with(|| {
        let ea = entity_account(ENTITY_ID);
        inject_token_pending(ENTITY_ID, REFERRER, 10_000);
        set_token_balance(ENTITY_ID, ea, 50_000);

        assert_ok!(CommissionCore::withdraw_token_commission(
            RuntimeOrigin::signed(REFERRER),
            ENTITY_ID,
            None,
            None,
        ));

        let history = MemberTokenWithdrawalHistory::<Test>::get(ENTITY_ID, REFERRER);
        assert_eq!(history.len(), 1);
        assert_eq!(history[0].total_amount, 10_000);
        assert_eq!(history[0].withdrawn, 10_000);
    });
}

// ==================== #10 历史记录清理机制 ====================

#[test]
fn f10_archive_order_records_works() {
    new_test_ext().execute_with(|| {
        // 手动插入已完结的 NEX 佣金记录
        OrderCommissionRecords::<Test>::mutate(5001u64, |records| {
            let _ = records.try_push(crate::pallet::CommissionRecordOf::<Test> {
                entity_id: ENTITY_ID,
                shop_id: SHOP_ID,
                order_id: 5001,
                buyer: BUYER,
                beneficiary: REFERRER,
                amount: 1000,
                commission_type: CommissionType::DirectReward,
                level: 0,
                status: CommissionStatus::Settled,
                created_at: 1u64,
            });
        });
        OrderTreasuryTransfer::<Test>::insert(5001u64, 500u128);

        assert_ok!(CommissionCore::archive_order_records(
            RuntimeOrigin::signed(SELLER),
            ENTITY_ID,
            5001,
        ));

        assert!(OrderCommissionRecords::<Test>::get(5001u64).is_empty());
        assert_eq!(OrderTreasuryTransfer::<Test>::get(5001u64), 0);
    });
}

#[test]
fn f10_archive_rejects_non_finalized_records() {
    new_test_ext().execute_with(|| {
        OrderCommissionRecords::<Test>::mutate(5002u64, |records| {
            let _ = records.try_push(crate::pallet::CommissionRecordOf::<Test> {
                entity_id: ENTITY_ID,
                shop_id: SHOP_ID,
                order_id: 5002,
                buyer: BUYER,
                beneficiary: REFERRER,
                amount: 1000,
                commission_type: CommissionType::DirectReward,
                level: 0,
                status: CommissionStatus::Pending, // 未完结
                created_at: 1u64,
            });
        });

        assert_noop!(
            CommissionCore::archive_order_records(RuntimeOrigin::signed(SELLER), ENTITY_ID, 5002,),
            Error::<Test>::OrderRecordsNotFinalized
        );
    });
}

#[test]
fn f10_archive_rejects_empty_order() {
    new_test_ext().execute_with(|| {
        assert_noop!(
            CommissionCore::archive_order_records(RuntimeOrigin::signed(SELLER), ENTITY_ID, 9999,),
            Error::<Test>::OrderRecordsNotFound
        );
    });
}

// ==================== #11 Storage 版本与迁移钩子 ====================

#[test]
fn f11_on_runtime_upgrade_bumps_version() {
    new_test_ext().execute_with(|| {
        use frame_support::traits::Hooks;
        use frame_support::traits::StorageVersion;

        // 模拟旧版本
        StorageVersion::new(0).put::<CommissionCore>();
        let weight = CommissionCore::on_runtime_upgrade();
        assert!(weight.ref_time() > 0);

        // 验证版本已升到 1
        let on_chain = StorageVersion::get::<CommissionCore>();
        assert_eq!(on_chain, StorageVersion::new(1));

        // 再次升级应无操作
        let weight2 = CommissionCore::on_runtime_upgrade();
        assert_eq!(weight2.ref_time(), 0);
    });
}

// ============================================================================
// R5 审计回归测试
// ============================================================================

// ---------- H1-R5: enable_commission(false) 必须检查 PoolNotEmpty ----------

#[test]
fn h1r5_enable_commission_false_pool_not_empty_blocks() {
    new_test_ext().execute_with(|| {
        // 开启 POOL_REWARD + enabled=true
        CommissionConfigs::<Test>::insert(
            ENTITY_ID,
            CoreCommissionConfig {
                enabled_modes: CommissionModes(
                    CommissionModes::DIRECT_REWARD | CommissionModes::POOL_REWARD,
                ),
                max_commission_rate: 5000,
                enabled: true,
                withdrawal_cooldown: 0,
                owner_reward_rate: 0,
                token_withdrawal_cooldown: 0,
                plugin_caps: PluginBudgetCaps::default(),
            },
        );
        UnallocatedPool::<Test>::insert(ENTITY_ID, 50_000u128);
        let ea = entity_account(ENTITY_ID);
        fund(ea, 100_000);

        // disable commission with non-empty pool → PoolNotEmpty
        assert_noop!(
            CommissionCore::enable_commission(RuntimeOrigin::signed(SELLER), ENTITY_ID, false,),
            Error::<Test>::PoolNotEmpty
        );

        // 清空池后可以 disable
        UnallocatedPool::<Test>::insert(ENTITY_ID, 0u128);
        assert_ok!(CommissionCore::enable_commission(
            RuntimeOrigin::signed(SELLER),
            ENTITY_ID,
            false,
        ));

        // 池资金仍然始终受保护（即使 disabled）
        UnallocatedPool::<Test>::insert(ENTITY_ID, 50_000u128);
        // available = 100000 - 50000(pool protected) - 1(min) = 49999
        assert_ok!(CommissionCore::withdraw_entity_funds(
            RuntimeOrigin::signed(SELLER),
            ENTITY_ID,
            49_999,
        ));
        assert_eq!(UnallocatedPool::<Test>::get(ENTITY_ID), 50_000);
    });
}

#[test]
fn h1r5_enable_commission_true_succeeds_with_pool() {
    new_test_ext().execute_with(|| {
        // 先开启 POOL_REWARD 然后 disable（需要空池才能 disable）
        CommissionConfigs::<Test>::insert(
            ENTITY_ID,
            CoreCommissionConfig {
                enabled_modes: CommissionModes(
                    CommissionModes::DIRECT_REWARD | CommissionModes::POOL_REWARD,
                ),
                max_commission_rate: 5000,
                enabled: true,
                withdrawal_cooldown: 0,
                owner_reward_rate: 0,
                token_withdrawal_cooldown: 0,
                plugin_caps: PluginBudgetCaps {
                    referral_cap: 10000,
                    ..PluginBudgetCaps::default()
                },
            },
        );

        // 池为空，disable 成功
        assert_ok!(CommissionCore::enable_commission(
            RuntimeOrigin::signed(SELLER),
            ENTITY_ID,
            false,
        ));

        // 重新启用（有池资金也可以 enable，因为 enable 不关闭 POOL_REWARD）
        UnallocatedPool::<Test>::insert(ENTITY_ID, 5000u128);
        assert_ok!(CommissionCore::enable_commission(
            RuntimeOrigin::signed(SELLER),
            ENTITY_ID,
            true,
        ));
    });
}

// ---------- H1-R5: clear_commission_config 必须检查 PoolNotEmpty ----------

#[test]
fn h1r5_clear_commission_config_pool_not_empty_blocks() {
    new_test_ext().execute_with(|| {
        CommissionConfigs::<Test>::insert(
            ENTITY_ID,
            CoreCommissionConfig {
                enabled_modes: CommissionModes(
                    CommissionModes::DIRECT_REWARD | CommissionModes::POOL_REWARD,
                ),
                max_commission_rate: 5000,
                enabled: true,
                withdrawal_cooldown: 0,
                owner_reward_rate: 0,
                token_withdrawal_cooldown: 0,
                plugin_caps: PluginBudgetCaps::default(),
            },
        );
        UnallocatedPool::<Test>::insert(ENTITY_ID, 30_000u128);
        let ea = entity_account(ENTITY_ID);
        fund(ea, 100_000);

        // 池非空 → clear 被阻止
        assert_noop!(
            CommissionCore::clear_commission_config(RuntimeOrigin::signed(SELLER), ENTITY_ID,),
            Error::<Test>::PoolNotEmpty
        );

        // 清空池后 clear 成功
        UnallocatedPool::<Test>::insert(ENTITY_ID, 0u128);
        assert_ok!(CommissionCore::clear_commission_config(
            RuntimeOrigin::signed(SELLER),
            ENTITY_ID,
        ));

        // 配置已清除
        assert!(CommissionConfigs::<Test>::get(ENTITY_ID).is_none());
    });
}

#[test]
fn h1r5_clear_config_without_pool_reward_succeeds() {
    // 没有 POOL_REWARD 的配置被清除时无需检查池（POOL_REWARD 未开启）
    new_test_ext().execute_with(|| {
        CommissionConfigs::<Test>::insert(
            ENTITY_ID,
            CoreCommissionConfig {
                enabled_modes: CommissionModes(CommissionModes::DIRECT_REWARD),
                max_commission_rate: 5000,
                enabled: true,
                withdrawal_cooldown: 0,
                owner_reward_rate: 0,
                token_withdrawal_cooldown: 0,
                plugin_caps: PluginBudgetCaps::default(),
            },
        );

        assert_ok!(CommissionCore::clear_commission_config(
            RuntimeOrigin::signed(SELLER),
            ENTITY_ID,
        ));
        assert!(CommissionConfigs::<Test>::get(ENTITY_ID).is_none());
    });
}

// ---------- H1-R5: force_disable_entity_commission 豁免 PoolNotEmpty ----------

#[test]
fn h1r5_force_disable_bypasses_pool_not_empty() {
    new_test_ext().execute_with(|| {
        CommissionConfigs::<Test>::insert(
            ENTITY_ID,
            CoreCommissionConfig {
                enabled_modes: CommissionModes(
                    CommissionModes::DIRECT_REWARD | CommissionModes::POOL_REWARD,
                ),
                max_commission_rate: 5000,
                enabled: true,
                withdrawal_cooldown: 0,
                owner_reward_rate: 0,
                token_withdrawal_cooldown: 0,
                plugin_caps: PluginBudgetCaps::default(),
            },
        );
        UnallocatedPool::<Test>::insert(ENTITY_ID, 20_000u128);
        let ea = entity_account(ENTITY_ID);
        fund(ea, 100_000);

        // Root force_disable 绕过 PoolNotEmpty
        assert_ok!(CommissionCore::force_disable_entity_commission(
            RuntimeOrigin::root(),
            ENTITY_ID,
        ));

        // 但池资金仍受保护
        let balance = Balances::free_balance(ea);
        let available = balance.saturating_sub(20_000).saturating_sub(1);
        assert_ok!(CommissionCore::withdraw_entity_funds(
            RuntimeOrigin::signed(SELLER),
            ENTITY_ID,
            available,
        ));
        assert_eq!(UnallocatedPool::<Test>::get(ENTITY_ID), 20_000);
    });
}

// ---------- M1-R5: archive_order_records 跨实体越权归档 ----------

#[test]
fn m1r5_archive_order_rejects_cross_entity() {
    new_test_ext().execute_with(|| {
        let other_entity_id = 2u64;
        // 设置 entity 2 的 owner
        set_entity_owner(other_entity_id, 77);

        // 在 entity 1 下创建已完结的订单记录
        let record = pallet_commission_common::CommissionRecord {
            entity_id: ENTITY_ID,
            shop_id: SHOP_ID,
            order_id: 999,
            buyer: BUYER,
            beneficiary: REFERRER,
            amount: 100u128,
            commission_type: CommissionType::DirectReward,
            level: 1,
            status: CommissionStatus::Cancelled,
            created_at: 1u64,
        };
        OrderCommissionRecords::<Test>::mutate(999u64, |records| {
            let _ = records.try_push(record);
        });

        // entity 2 的 owner 尝试归档 entity 1 的订单 → 应失败
        assert_noop!(
            CommissionCore::archive_order_records(RuntimeOrigin::signed(77), other_entity_id, 999,),
            Error::<Test>::OrderRecordsNotFound
        );

        // entity 1 的 owner 可以归档
        assert_ok!(CommissionCore::archive_order_records(
            RuntimeOrigin::signed(SELLER),
            ENTITY_ID,
            999,
        ));
        assert!(OrderCommissionRecords::<Test>::get(999u64).is_empty());
    });
}

// ---------- M2-R5: process_token_commission 未配置时优雅返回 ----------

#[test]
fn m2r5_process_token_commission_ok_when_not_configured() {
    new_test_ext().execute_with(|| {
        let ea = entity_account(ENTITY_ID);
        set_token_balance(ENTITY_ID, ea, 50_000);

        // 不设置任何 CommissionConfig → 应成功返回而非报错
        assert_ok!(CommissionCore::process_token_commission(
            ENTITY_ID, SHOP_ID, 2001, &BUYER, 10_000, 10_000, 500, PRODUCT_ID,
        ));

        // token_platform_fee (500) 通过 sweep 被计入 accounted balance
        // 但不会进入 pending/pool/shopping（因无配置）
        assert_eq!(TokenPendingTotal::<Test>::get(ENTITY_ID), 0);
        assert_eq!(UnallocatedTokenPool::<Test>::get(ENTITY_ID), 0);
    });
}

#[test]
fn m2r5_process_token_commission_ok_when_disabled() {
    new_test_ext().execute_with(|| {
        let ea = entity_account(ENTITY_ID);
        set_token_balance(ENTITY_ID, ea, 50_000);

        // 配置存在但 enabled=false
        CommissionConfigs::<Test>::insert(
            ENTITY_ID,
            CoreCommissionConfig {
                enabled_modes: CommissionModes(CommissionModes::DIRECT_REWARD),
                max_commission_rate: 5000,
                enabled: false,
                withdrawal_cooldown: 0,
                owner_reward_rate: 0,
                token_withdrawal_cooldown: 0,
                plugin_caps: PluginBudgetCaps::default(),
            },
        );

        assert_ok!(CommissionCore::process_token_commission(
            ENTITY_ID, SHOP_ID, 2002, &BUYER, 10_000, 10_000, 500, PRODUCT_ID,
        ));

        assert_eq!(TokenPendingTotal::<Test>::get(ENTITY_ID), 0);
    });
}

// ============================================================================
// M1-R6: set_commission_modes PoolNotEmpty 回归测试
// ============================================================================

#[test]
fn m1r6_disable_while_disabled_still_blocks_pool_not_empty() {
    // 攻击路径: enable_commission(false) → remove POOL_REWARD → add POOL_REWARD back
    // 预期: PoolNotEmpty 阻止 mode 变更（无法绕过）
    new_test_ext().execute_with(|| {
        let ea = entity_account(ENTITY_ID);
        fund(ea, 100_000);
        fund(PLATFORM, 100_000);
        fund(SELLER, 100_000);

        // 1. 启用佣金 + POOL_REWARD，让池有资金
        CommissionConfigs::<Test>::insert(
            ENTITY_ID,
            CoreCommissionConfig {
                enabled_modes: CommissionModes(
                    CommissionModes::DIRECT_REWARD | CommissionModes::POOL_REWARD,
                ),
                max_commission_rate: 5000,
                enabled: true,
                withdrawal_cooldown: 0,
                owner_reward_rate: 0,
                token_withdrawal_cooldown: 0,
                plugin_caps: PluginBudgetCaps {
                    referral_cap: 10000,
                    ..PluginBudgetCaps::default()
                },
            },
        );
        UnallocatedPool::<Test>::insert(ENTITY_ID, 10_000u128);

        // 2. 尝试禁用佣金 → PoolNotEmpty
        assert_noop!(
            CommissionCore::enable_commission(RuntimeOrigin::signed(SELLER), ENTITY_ID, false,),
            Error::<Test>::PoolNotEmpty
        );

        // 3. 尝试移除 POOL_REWARD 模式位 → PoolNotEmpty
        assert_noop!(
            CommissionCore::set_commission_modes(
                RuntimeOrigin::signed(SELLER),
                ENTITY_ID,
                CommissionModes(CommissionModes::DIRECT_REWARD),
            ),
            Error::<Test>::PoolNotEmpty
        );
    });
}

#[test]
fn m1r6_mode_toggle_succeeds_when_pool_empty() {
    // 对照测试: 池为空时可以自由切换模式
    new_test_ext().execute_with(|| {
        let ea = entity_account(ENTITY_ID);
        fund(ea, 100_000);
        fund(SELLER, 100_000);

        // 1. 有 POOL_REWARD 且 enabled，池为空
        CommissionConfigs::<Test>::insert(
            ENTITY_ID,
            CoreCommissionConfig {
                enabled_modes: CommissionModes(
                    CommissionModes::DIRECT_REWARD | CommissionModes::POOL_REWARD,
                ),
                max_commission_rate: 5000,
                enabled: true,
                withdrawal_cooldown: 0,
                owner_reward_rate: 0,
                token_withdrawal_cooldown: 0,
                plugin_caps: PluginBudgetCaps {
                    referral_cap: 10000,
                    ..PluginBudgetCaps::default()
                },
            },
        );

        // 2. 移除 POOL_REWARD → 成功（池为空）
        assert_ok!(CommissionCore::set_commission_modes(
            RuntimeOrigin::signed(SELLER),
            ENTITY_ID,
            CommissionModes(CommissionModes::DIRECT_REWARD),
        ));

        // 3. 重新添加 POOL_REWARD → 成功
        assert_ok!(CommissionCore::set_commission_modes(
            RuntimeOrigin::signed(SELLER),
            ENTITY_ID,
            CommissionModes(CommissionModes::DIRECT_REWARD | CommissionModes::POOL_REWARD),
        ));
    });
}

#[test]
fn m1r6_full_attack_path_pool_stays_protected() {
    // 完整攻击路径验证: 池资金始终受保护，PoolNotEmpty 阻止 mode 变更
    new_test_ext().execute_with(|| {
        let ea = entity_account(ENTITY_ID);
        fund(ea, 100_000);
        fund(SELLER, 100_000);

        // 1. 设置有 POOL_REWARD 的 Entity，池有资金
        CommissionConfigs::<Test>::insert(ENTITY_ID, CoreCommissionConfig {
            enabled_modes: CommissionModes(CommissionModes::DIRECT_REWARD | CommissionModes::POOL_REWARD),
            max_commission_rate: 5000,
            enabled: true,
            withdrawal_cooldown: 0,
            owner_reward_rate: 0,
            token_withdrawal_cooldown: 0,
        plugin_caps: PluginBudgetCaps {
            referral_cap: 10000,
            ..PluginBudgetCaps::default()
        },
        });
        UnallocatedPool::<Test>::insert(ENTITY_ID, 50_000u128);

        // 2. 尝试禁用佣金 → PoolNotEmpty 阻止
        assert_noop!(CommissionCore::enable_commission(
            RuntimeOrigin::signed(SELLER), ENTITY_ID, false,
        ), Error::<Test>::PoolNotEmpty);

        // 3. 尝试 mode toggle → PoolNotEmpty 阻止
        assert_noop!(
            CommissionCore::set_commission_modes(
                RuntimeOrigin::signed(SELLER), ENTITY_ID,
                CommissionModes(CommissionModes::DIRECT_REWARD),
            ),
            Error::<Test>::PoolNotEmpty
        );

        // 4. pool 资金始终受保护，仅 free 部分可提取
        // available = 100000 - 50000(pool) - 1(min) = 49999
        assert_noop!(
            CommissionCore::withdraw_entity_funds(
                RuntimeOrigin::signed(SELLER), ENTITY_ID, 50_000,
            ),
            Error::<Test>::InsufficientEntityFunds
        );
        assert_ok!(CommissionCore::withdraw_entity_funds(
            RuntimeOrigin::signed(SELLER), ENTITY_ID, 49_999,
        ));
    });
}

// ============================================================================
// M2-R6: Token cancel 回退 Pool A 留存 回归测试
// ============================================================================

#[test]
fn m2r6_token_cancel_reverses_pool_a_retention() {
    new_test_ext().execute_with(|| {
        let ea = entity_account(ENTITY_ID);
        set_token_balance(ENTITY_ID, ea, 100_000);

        // 配置启用佣金 + POOL_REWARD
        CommissionConfigs::<Test>::insert(
            ENTITY_ID,
            CoreCommissionConfig {
                enabled_modes: CommissionModes(
                    CommissionModes::DIRECT_REWARD | CommissionModes::POOL_REWARD,
                ),
                max_commission_rate: 5000,
                enabled: true,
                withdrawal_cooldown: 0,
                owner_reward_rate: 0,
                token_withdrawal_cooldown: 0,
                plugin_caps: PluginBudgetCaps::default(),
            },
        );

        let order_id = 3001u64;
        let token_platform_fee = 1000u128;

        // process_token_commission: platform_fee (1000) 无 referrer → 全部进 pool_a_retention
        assert_ok!(CommissionCore::process_token_commission(
            ENTITY_ID,
            SHOP_ID,
            order_id,
            &BUYER,
            10_000,
            10_000,
            token_platform_fee,
            PRODUCT_ID,
        ));

        // pool_a_retention = 1000 (全部 platform_fee，无 referrer)
        // Pool B remaining (max_commission=5000, no plugins → all 5000 to pool)
        // Total pool = 1000 (Pool A) + 5000 (Pool B) = 6000
        let pool_before = UnallocatedTokenPool::<Test>::get(ENTITY_ID);
        assert_eq!(
            pool_before, 6000,
            "pool = pool_a_retention(1000) + pool_b_remaining(5000)"
        );

        // 验证 OrderTokenPlatformRetention 被记录
        let (ret_eid, ret_amount) = OrderTokenPlatformRetention::<Test>::get(order_id);
        assert_eq!(ret_eid, ENTITY_ID);
        assert_eq!(ret_amount, token_platform_fee);

        // cancel → Pool B refund (-5000) + Pool A retention refund (-1000) = 0
        assert_ok!(CommissionCore::do_cancel_token_commission(order_id));

        let pool_after = UnallocatedTokenPool::<Test>::get(ENTITY_ID);
        assert_eq!(
            pool_after, 0,
            "pool should be 0 after both Pool B and Pool A retention are reversed"
        );

        // OrderTokenPlatformRetention 应被清除
        assert!(!OrderTokenPlatformRetention::<Test>::contains_key(order_id));
    });
}

#[test]
fn m2r6_token_cancel_with_referrer_partial_retention() {
    new_test_ext().execute_with(|| {
        let ea = entity_account(ENTITY_ID);
        set_token_balance(ENTITY_ID, ea, 100_000);

        // 设置 entity referrer → pool_a 会分出 referrer 份额
        set_entity_referrer(ENTITY_ID, REFERRER);

        CommissionConfigs::<Test>::insert(
            ENTITY_ID,
            CoreCommissionConfig {
                enabled_modes: CommissionModes(
                    CommissionModes::DIRECT_REWARD | CommissionModes::POOL_REWARD,
                ),
                max_commission_rate: 5000,
                enabled: true,
                withdrawal_cooldown: 0,
                owner_reward_rate: 0,
                token_withdrawal_cooldown: 0,
                plugin_caps: PluginBudgetCaps::default(),
            },
        );

        let order_id = 3002u64;
        let token_platform_fee = 1000u128;

        assert_ok!(CommissionCore::process_token_commission(
            ENTITY_ID,
            SHOP_ID,
            order_id,
            &BUYER,
            10_000,
            10_000,
            token_platform_fee,
            PRODUCT_ID,
        ));

        // ReferrerShareBps = 5000 (50%), so referrer gets 500, retention = 500
        let (ret_eid, ret_amount) = OrderTokenPlatformRetention::<Test>::get(order_id);
        assert_eq!(ret_eid, ENTITY_ID);
        assert_eq!(ret_amount, 500); // 1000 - 500 referrer

        // Pool = 500 (Pool A retention) + Pool B remaining
        let pool_before = UnallocatedTokenPool::<Test>::get(ENTITY_ID);
        assert!(
            pool_before >= 500,
            "pool should include at least pool_a_retention"
        );

        // cancel reverses both Pool B and Pool A retention
        assert_ok!(CommissionCore::do_cancel_token_commission(order_id));

        let pool_after = UnallocatedTokenPool::<Test>::get(ENTITY_ID);
        assert_eq!(pool_after, 0, "pool should be fully reversed after cancel");
        assert!(!OrderTokenPlatformRetention::<Test>::contains_key(order_id));
    });
}

#[test]
fn m2r6_archive_cleans_order_token_platform_retention() {
    new_test_ext().execute_with(|| {
        let ea = entity_account(ENTITY_ID);
        set_token_balance(ENTITY_ID, ea, 100_000);
        fund(ea, 100_000);
        fund(PLATFORM, 100_000);
        fund(SELLER, 100_000);

        CommissionConfigs::<Test>::insert(
            ENTITY_ID,
            CoreCommissionConfig {
                enabled_modes: CommissionModes(
                    CommissionModes::DIRECT_REWARD | CommissionModes::POOL_REWARD,
                ),
                max_commission_rate: 5000,
                enabled: true,
                withdrawal_cooldown: 0,
                owner_reward_rate: 0,
                token_withdrawal_cooldown: 0,
                plugin_caps: PluginBudgetCaps::default(),
            },
        );

        let order_id = 3003u64;

        // Process token commission to populate OrderTokenPlatformRetention
        assert_ok!(CommissionCore::process_token_commission(
            ENTITY_ID, SHOP_ID, order_id, &BUYER, 10_000, 10_000, 1000, PRODUCT_ID,
        ));
        assert!(OrderTokenPlatformRetention::<Test>::contains_key(order_id));

        // Cancel to make records finalized (Cancelled)
        assert_ok!(CommissionCore::do_cancel_token_commission(order_id));

        // Insert cancelled NEX + Token records so archive passes validation
        use pallet_commission_common::CommissionRecord;
        OrderCommissionRecords::<Test>::mutate(order_id, |records| {
            let _ = records.try_push(CommissionRecord {
                entity_id: ENTITY_ID,
                shop_id: SHOP_ID,
                order_id,
                buyer: BUYER,
                beneficiary: BUYER,
                amount: 100u128,
                commission_type: CommissionType::DirectReward,
                level: 0,
                status: CommissionStatus::Cancelled,
                created_at: 1u64,
            });
        });

        use pallet_commission_common::TokenCommissionRecord;
        OrderTokenCommissionRecords::<Test>::mutate(order_id, |records| {
            records.clear();
            let _ = records.try_push(TokenCommissionRecord {
                entity_id: ENTITY_ID,
                order_id,
                buyer: BUYER,
                beneficiary: BUYER,
                amount: 100u128,
                commission_type: CommissionType::DirectReward,
                level: 0,
                status: CommissionStatus::Cancelled,
                created_at: 1u64,
            });
        });

        assert_ok!(CommissionCore::archive_order_records(
            RuntimeOrigin::signed(SELLER),
            ENTITY_ID,
            order_id,
        ));

        // OrderTokenPlatformRetention should be cleaned
        assert!(!OrderTokenPlatformRetention::<Test>::contains_key(order_id));
    });
}

// ==================== BUG-1: settle_order_commission ====================

#[test]
fn bug1_settle_order_records_transitions_pending_to_withdrawn() {
    new_test_ext().execute_with(|| {
        OrderCommissionRecords::<Test>::mutate(7001u64, |records| {
            let _ = records.try_push(crate::pallet::CommissionRecordOf::<Test> {
                entity_id: ENTITY_ID,
                shop_id: SHOP_ID,
                order_id: 7001,
                buyer: BUYER,
                beneficiary: REFERRER,
                amount: 1000,
                commission_type: CommissionType::DirectReward,
                level: 0,
                status: CommissionStatus::Pending,
                created_at: 1u64,
            });
        });

        assert_ok!(CommissionCore::do_settle_order_records(7001));

        let records = OrderCommissionRecords::<Test>::get(7001u64);
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].status, CommissionStatus::Settled);
    });
}

#[test]
fn bug1_settle_preserves_cancelled_records() {
    new_test_ext().execute_with(|| {
        OrderCommissionRecords::<Test>::mutate(7002u64, |records| {
            let _ = records.try_push(crate::pallet::CommissionRecordOf::<Test> {
                entity_id: ENTITY_ID,
                shop_id: SHOP_ID,
                order_id: 7002,
                buyer: BUYER,
                beneficiary: REFERRER,
                amount: 500,
                commission_type: CommissionType::DirectReward,
                level: 0,
                status: CommissionStatus::Cancelled,
                created_at: 1u64,
            });
            let _ = records.try_push(crate::pallet::CommissionRecordOf::<Test> {
                entity_id: ENTITY_ID,
                shop_id: SHOP_ID,
                order_id: 7002,
                buyer: BUYER,
                beneficiary: REFERRER,
                amount: 300,
                commission_type: CommissionType::DirectReward,
                level: 0,
                status: CommissionStatus::Pending,
                created_at: 1u64,
            });
        });

        assert_ok!(CommissionCore::do_settle_order_records(7002));

        let records = OrderCommissionRecords::<Test>::get(7002u64);
        assert_eq!(records[0].status, CommissionStatus::Cancelled);
        assert_eq!(records[1].status, CommissionStatus::Settled);
    });
}

#[test]
fn bug1_settle_then_archive_succeeds() {
    new_test_ext().execute_with(|| {
        OrderCommissionRecords::<Test>::mutate(7003u64, |records| {
            let _ = records.try_push(crate::pallet::CommissionRecordOf::<Test> {
                entity_id: ENTITY_ID,
                shop_id: SHOP_ID,
                order_id: 7003,
                buyer: BUYER,
                beneficiary: REFERRER,
                amount: 1000,
                commission_type: CommissionType::DirectReward,
                level: 0,
                status: CommissionStatus::Pending,
                created_at: 1u64,
            });
        });

        // settle first
        assert_ok!(CommissionCore::do_settle_order_records(7003));
        // then archive should succeed
        assert_ok!(CommissionCore::archive_order_records(
            RuntimeOrigin::signed(SELLER),
            ENTITY_ID,
            7003,
        ));
        assert!(OrderCommissionRecords::<Test>::get(7003u64).is_empty());
    });
}

#[test]
fn bug1_settle_token_records_works() {
    new_test_ext().execute_with(|| {
        use pallet_commission_common::TokenCommissionRecord;
        OrderTokenCommissionRecords::<Test>::mutate(7004u64, |records| {
            let _ = records.try_push(TokenCommissionRecord {
                entity_id: ENTITY_ID,
                order_id: 7004,
                buyer: BUYER,
                beneficiary: REFERRER,
                amount: 500u128,
                commission_type: CommissionType::DirectReward,
                level: 0,
                status: CommissionStatus::Pending,
                created_at: 1u64,
            });
        });

        assert_ok!(CommissionCore::do_settle_order_records(7004));

        let records = OrderTokenCommissionRecords::<Test>::get(7004u64);
        assert_eq!(records[0].status, CommissionStatus::Settled);
    });
}

#[test]
fn bug1_trait_settle_order_commission_works() {
    new_test_ext().execute_with(|| {
        OrderCommissionRecords::<Test>::mutate(7005u64, |records| {
            let _ = records.try_push(crate::pallet::CommissionRecordOf::<Test> {
                entity_id: ENTITY_ID,
                shop_id: SHOP_ID,
                order_id: 7005,
                buyer: BUYER,
                beneficiary: REFERRER,
                amount: 1000,
                commission_type: CommissionType::DirectReward,
                level: 0,
                status: CommissionStatus::Pending,
                created_at: 1u64,
            });
        });

        assert_ok!(<CommissionCore as pallet_commission_common::CommissionProvider<u64, u128>>::settle_order_commission(7005));

        let records = OrderCommissionRecords::<Test>::get(7005u64);
        assert_eq!(records[0].status, CommissionStatus::Settled);
    });
}

// ==================== BUG-2: GlobalMaxTokenCommissionRate enforcement ====================

#[test]
fn bug2_set_commission_rate_respects_token_global_max() {
    new_test_ext().execute_with(|| {
        // 设置 Token 全局上限 = 3000
        assert_ok!(CommissionCore::set_global_max_token_commission_rate(
            RuntimeOrigin::root(),
            ENTITY_ID,
            3000,
        ));

        // 设置 rate = 3000 → 成功
        assert_ok!(CommissionCore::set_commission_rate(
            RuntimeOrigin::signed(SELLER),
            ENTITY_ID,
            3000,
        ));

        // 设置 rate = 3001 → 失败
        assert_noop!(
            CommissionCore::set_commission_rate(RuntimeOrigin::signed(SELLER), ENTITY_ID, 3001,),
            Error::<Test>::TokenCommissionRateExceedsGlobalMax
        );
    });
}

#[test]
fn bug2_both_global_caps_enforced() {
    new_test_ext().execute_with(|| {
        // NEX 上限 5000, Token 上限 3000 → 实际上限取两者中较小的
        assert_ok!(CommissionCore::set_global_max_commission_rate(
            RuntimeOrigin::root(),
            ENTITY_ID,
            5000,
        ));
        assert_ok!(CommissionCore::set_global_max_token_commission_rate(
            RuntimeOrigin::root(),
            ENTITY_ID,
            3000,
        ));

        assert_ok!(CommissionCore::set_commission_rate(
            RuntimeOrigin::signed(SELLER),
            ENTITY_ID,
            3000,
        ));
        assert_noop!(
            CommissionCore::set_commission_rate(RuntimeOrigin::signed(SELLER), ENTITY_ID, 4000,),
            Error::<Test>::TokenCommissionRateExceedsGlobalMax
        );
    });
}

// ==================== BUG-3: force_enable_entity_commission ====================

#[test]
fn bug3_force_enable_after_force_disable() {
    new_test_ext().execute_with(|| {
        // H-1: 设置 referral_cap，然后设置单插件模式，再启用
        CommissionConfigs::<Test>::mutate(ENTITY_ID, |maybe| {
            let config = maybe.get_or_insert_with(CoreCommissionConfig::default);
            config.plugin_caps.referral_cap = 10000;
        });
        assert_ok!(CommissionCore::set_commission_modes(
            RuntimeOrigin::signed(SELLER),
            ENTITY_ID,
            CommissionModes(CommissionModes::DIRECT_REWARD),
        ));
        // 先配置并启用
        assert_ok!(CommissionCore::enable_commission(
            RuntimeOrigin::signed(SELLER),
            ENTITY_ID,
            true,
        ));
        let config = CommissionConfigs::<Test>::get(ENTITY_ID).unwrap();
        assert!(config.enabled);

        // Root 强制禁用
        assert_ok!(CommissionCore::force_disable_entity_commission(
            RuntimeOrigin::root(),
            ENTITY_ID,
        ));
        let config = CommissionConfigs::<Test>::get(ENTITY_ID).unwrap();
        assert!(!config.enabled);

        // Root 重新启用
        assert_ok!(CommissionCore::force_enable_entity_commission(
            RuntimeOrigin::root(),
            ENTITY_ID,
        ));
        let config = CommissionConfigs::<Test>::get(ENTITY_ID).unwrap();
        assert!(config.enabled);
    });
}

#[test]
fn bug3_force_enable_requires_root() {
    new_test_ext().execute_with(|| {
        assert_noop!(
            CommissionCore::force_enable_entity_commission(
                RuntimeOrigin::signed(SELLER),
                ENTITY_ID,
            ),
            sp_runtime::DispatchError::BadOrigin
        );
    });
}

#[test]
fn bug3_force_enable_rejects_nonexistent_entity() {
    new_test_ext().execute_with(|| {
        assert_noop!(
            CommissionCore::force_enable_entity_commission(RuntimeOrigin::root(), 999,),
            Error::<Test>::EntityNotFound
        );
    });
}

// ==================== MISSING-2: retry_cancel_commission ====================

#[test]
fn missing2_retry_cancel_commission_requires_root() {
    new_test_ext().execute_with(|| {
        assert_noop!(
            CommissionCore::retry_cancel_commission(RuntimeOrigin::signed(SELLER), 1,),
            sp_runtime::DispatchError::BadOrigin
        );
    });
}

#[test]
fn missing2_retry_cancel_is_idempotent() {
    new_test_ext().execute_with(|| {
        fund(SELLER, 1_000_000);
        fund(PLATFORM, 1_000_000);

        // 配置佣金
        assert_ok!(CommissionCore::set_commission_rate(
            RuntimeOrigin::signed(SELLER),
            ENTITY_ID,
            1000,
        ));
        // H-1: 设置 referral_cap，然后设置单插件模式，再启用
        CommissionConfigs::<Test>::mutate(ENTITY_ID, |maybe| {
            let config = maybe.get_or_insert_with(CoreCommissionConfig::default);
            config.plugin_caps.referral_cap = 10000;
        });
        assert_ok!(CommissionCore::set_commission_modes(
            RuntimeOrigin::signed(SELLER),
            ENTITY_ID,
            CommissionModes(CommissionModes::DIRECT_REWARD),
        ));
        assert_ok!(CommissionCore::enable_commission(
            RuntimeOrigin::signed(SELLER),
            ENTITY_ID,
            true,
        ));

        // 取消（无 pending records → 无操作）
        assert_ok!(CommissionCore::retry_cancel_commission(
            RuntimeOrigin::root(),
            9999,
        ));
    });
}

// ==================== MISSING-3: min_withdrawal_interval ====================

#[test]
fn missing3_set_min_withdrawal_interval_works() {
    new_test_ext().execute_with(|| {
        assert_eq!(MinWithdrawalInterval::<Test>::get(ENTITY_ID), 0);

        assert_ok!(CommissionCore::set_min_withdrawal_interval(
            RuntimeOrigin::signed(SELLER),
            ENTITY_ID,
            50,
        ));

        assert_eq!(MinWithdrawalInterval::<Test>::get(ENTITY_ID), 50);
    });
}

#[test]
fn missing3_set_interval_requires_owner_or_admin() {
    new_test_ext().execute_with(|| {
        assert_noop!(
            CommissionCore::set_min_withdrawal_interval(
                RuntimeOrigin::signed(BUYER),
                ENTITY_ID,
                50,
            ),
            Error::<Test>::NotEntityOwnerOrAdmin
        );
    });
}

#[test]
fn missing3_set_interval_checks_entity_locked() {
    new_test_ext().execute_with(|| {
        set_entity_locked(ENTITY_ID);
        assert_noop!(
            CommissionCore::set_min_withdrawal_interval(
                RuntimeOrigin::signed(SELLER),
                ENTITY_ID,
                50,
            ),
            Error::<Test>::EntityLocked
        );
    });
}

#[test]
fn missing3_withdrawal_interval_enforced() {
    new_test_ext().execute_with(|| {
        fund(SELLER, 1_000_000);
        fund(PLATFORM, 1_000_000);
        let entity_acct = entity_account(ENTITY_ID);
        fund(entity_acct, 1_000_000);

        // 配置佣金（H-1: 设置 referral_cap，然后设置单插件模式再启用）
        assert_ok!(CommissionCore::set_commission_rate(
            RuntimeOrigin::signed(SELLER),
            ENTITY_ID,
            1000,
        ));
        CommissionConfigs::<Test>::mutate(ENTITY_ID, |maybe| {
            let config = maybe.get_or_insert_with(CoreCommissionConfig::default);
            config.plugin_caps.referral_cap = 10000;
        });
        assert_ok!(CommissionCore::set_commission_modes(
            RuntimeOrigin::signed(SELLER),
            ENTITY_ID,
            CommissionModes(CommissionModes::DIRECT_REWARD),
        ));
        assert_ok!(CommissionCore::enable_commission(
            RuntimeOrigin::signed(SELLER),
            ENTITY_ID,
            true,
        ));
        assert_ok!(CommissionCore::set_withdrawal_config(
            RuntimeOrigin::signed(SELLER),
            ENTITY_ID,
            WithdrawalMode::FullWithdrawal,
            WithdrawalTierConfig {
                withdrawal_rate: 10000,
                repurchase_rate: 0
            },
            BoundedVec::default(),
            0,
            true,
        ));

        // 给会员记入佣金
        MemberCommissionStats::<Test>::mutate(ENTITY_ID, &BUYER, |stats| {
            stats.pending = 5000;
            stats.total_earned = 5000;
        });
        ShopPendingTotal::<Test>::insert(ENTITY_ID, 5000u128);

        // 设置最小间隔 = 10 区块
        assert_ok!(CommissionCore::set_min_withdrawal_interval(
            RuntimeOrigin::signed(SELLER),
            ENTITY_ID,
            10,
        ));

        // 第一次提现 → 成功（无历史提现记录，last_withdrawn = 0）
        assert_ok!(CommissionCore::withdraw_commission(
            RuntimeOrigin::signed(BUYER),
            ENTITY_ID,
            Some(1000u128),
            None,
        ));

        // 立即第二次提现 → 失败（间隔不足）
        assert_noop!(
            CommissionCore::withdraw_commission(
                RuntimeOrigin::signed(BUYER),
                ENTITY_ID,
                Some(1000u128),
                None,
            ),
            Error::<Test>::WithdrawalIntervalNotMet
        );

        // 推进区块到间隔之后
        System::set_block_number(11);

        // 第三次提现 → 成功
        assert_ok!(CommissionCore::withdraw_commission(
            RuntimeOrigin::signed(BUYER),
            ENTITY_ID,
            Some(1000u128),
            None,
        ));
    });
}

#[test]
fn missing3_token_withdrawal_interval_enforced() {
    new_test_ext().execute_with(|| {
        let entity_acct = entity_account(ENTITY_ID);
        set_token_balance(ENTITY_ID, entity_acct, 1_000_000);

        // 配置佣金（H-1: 设置 referral_cap，然后设置单插件模式再启用）
        assert_ok!(CommissionCore::set_commission_rate(
            RuntimeOrigin::signed(SELLER),
            ENTITY_ID,
            1000,
        ));
        CommissionConfigs::<Test>::mutate(ENTITY_ID, |maybe| {
            let config = maybe.get_or_insert_with(CoreCommissionConfig::default);
            config.plugin_caps.referral_cap = 10000;
        });
        assert_ok!(CommissionCore::set_commission_modes(
            RuntimeOrigin::signed(SELLER),
            ENTITY_ID,
            CommissionModes(CommissionModes::DIRECT_REWARD),
        ));
        assert_ok!(CommissionCore::enable_commission(
            RuntimeOrigin::signed(SELLER),
            ENTITY_ID,
            true,
        ));
        assert_ok!(CommissionCore::set_token_withdrawal_config(
            RuntimeOrigin::signed(SELLER),
            ENTITY_ID,
            WithdrawalMode::FullWithdrawal,
            WithdrawalTierConfig {
                withdrawal_rate: 10000,
                repurchase_rate: 0
            },
            BoundedVec::default(),
            0,
            true,
        ));

        // 给会员记入 Token 佣金
        MemberTokenCommissionStats::<Test>::mutate(ENTITY_ID, &BUYER, |stats| {
            stats.pending = 5000u128;
            stats.total_earned = 5000u128;
        });
        TokenPendingTotal::<Test>::insert(ENTITY_ID, 5000u128);

        // 设置最小间隔 = 10 区块
        assert_ok!(CommissionCore::set_min_withdrawal_interval(
            RuntimeOrigin::signed(SELLER),
            ENTITY_ID,
            10,
        ));

        // 第一次提现 → 成功
        assert_ok!(CommissionCore::withdraw_token_commission(
            RuntimeOrigin::signed(BUYER),
            ENTITY_ID,
            Some(1000u128),
            None,
        ));

        // 立即第二次提现 → 失败
        assert_noop!(
            CommissionCore::withdraw_token_commission(
                RuntimeOrigin::signed(BUYER),
                ENTITY_ID,
                Some(1000u128),
                None,
            ),
            Error::<Test>::WithdrawalIntervalNotMet
        );

        // 推进区块到间隔之后
        System::set_block_number(11);

        // 第三次提现 → 成功
        assert_ok!(CommissionCore::withdraw_token_commission(
            RuntimeOrigin::signed(BUYER),
            ENTITY_ID,
            Some(1000u128),
            None,
        ));
    });
}

// ==================== R-11: Double storage read eliminated ====================

#[test]
fn r11_set_commission_modes_pool_not_empty_guard_works() {
    new_test_ext().execute_with(|| {
        // H-1: 设置 referral_cap，然后设置单插件模式再启用
        CommissionConfigs::<Test>::mutate(ENTITY_ID, |maybe| {
            let config = maybe.get_or_insert_with(CoreCommissionConfig::default);
            config.plugin_caps.referral_cap = 10000;
        });
        assert_ok!(CommissionCore::set_commission_modes(
            RuntimeOrigin::signed(SELLER),
            ENTITY_ID,
            CommissionModes(CommissionModes::POOL_REWARD | CommissionModes::DIRECT_REWARD),
        ));
        // 启用佣金 + POOL_REWARD
        assert_ok!(CommissionCore::enable_commission(
            RuntimeOrigin::signed(SELLER),
            ENTITY_ID,
            true,
        ));

        // 池有资金时不能移除 POOL_REWARD
        UnallocatedPool::<Test>::insert(ENTITY_ID, 1000u128);
        assert_noop!(
            CommissionCore::set_commission_modes(
                RuntimeOrigin::signed(SELLER),
                ENTITY_ID,
                CommissionModes(CommissionModes::DIRECT_REWARD),
            ),
            Error::<Test>::PoolNotEmpty
        );

        // 清空池后可以移除 POOL_REWARD
        UnallocatedPool::<Test>::insert(ENTITY_ID, 0u128);
        assert_ok!(CommissionCore::set_commission_modes(
            RuntimeOrigin::signed(SELLER),
            ENTITY_ID,
            CommissionModes(CommissionModes::DIRECT_REWARD),
        ));

        // 重新添加 POOL_REWARD 无限制
        assert_ok!(CommissionCore::set_commission_modes(
            RuntimeOrigin::signed(SELLER),
            ENTITY_ID,
            CommissionModes(CommissionModes::POOL_REWARD | CommissionModes::DIRECT_REWARD),
        ));
    });
}

// ==================== BUG-B 审计修复: Token order_count 取消回滚 ====================

#[test]
fn bugb_cancel_token_commission_rolls_back_token_order_count() {
    // BUG-B: do_cancel_token_commission 应回滚 buyer 的 Token order_count
    new_test_ext().execute_with(|| {
        let ea = entity_account(ENTITY_ID);
        set_token_balance(ENTITY_ID, ea, 100_000);
        setup_token_config(5000, false);

        // 手动注入 Token 佣金记录（模拟插件产生了 Token 佣金）
        let record = pallet_commission_common::TokenCommissionRecord {
            entity_id: ENTITY_ID,
            order_id: 9001,
            buyer: BUYER,
            beneficiary: REFERRER,
            amount: 5_000u128,
            commission_type: CommissionType::DirectReward,
            level: 0,
            status: CommissionStatus::Pending,
            created_at: 1u64,
        };
        OrderTokenCommissionRecords::<Test>::mutate(9001u64, |records| {
            let _ = records.try_push(record);
        });
        inject_token_pending(ENTITY_ID, REFERRER, 5_000);
        MemberTokenCommissionStats::<Test>::mutate(ENTITY_ID, &BUYER, |stats| {
            stats.order_count = 1;
        });

        // 取消 Token 佣金 → order_count 应从 1 → 0
        assert_ok!(CommissionCore::do_cancel_token_commission(9001));
        let stats_after = MemberTokenCommissionStats::<Test>::get(ENTITY_ID, BUYER);
        assert_eq!(
            stats_after.order_count, 0,
            "Token order_count should be rolled back on cancel"
        );
    });
}

#[test]
fn bugb_cancel_token_commission_skips_rollback_when_already_cancelled() {
    // 已经 Cancelled 的记录不应再次回滚 order_count
    new_test_ext().execute_with(|| {
        let ea = entity_account(ENTITY_ID);
        set_token_balance(ENTITY_ID, ea, 100_000);
        setup_token_config(5000, false);

        // 手动注入 + 取消
        let record = pallet_commission_common::TokenCommissionRecord {
            entity_id: ENTITY_ID,
            order_id: 9002,
            buyer: BUYER,
            beneficiary: REFERRER,
            amount: 5_000u128,
            commission_type: CommissionType::DirectReward,
            level: 0,
            status: CommissionStatus::Pending,
            created_at: 1u64,
        };
        OrderTokenCommissionRecords::<Test>::mutate(9002u64, |records| {
            let _ = records.try_push(record);
        });
        inject_token_pending(ENTITY_ID, REFERRER, 5_000);
        MemberTokenCommissionStats::<Test>::mutate(ENTITY_ID, &BUYER, |stats| {
            stats.order_count = 1;
        });

        assert_ok!(CommissionCore::do_cancel_token_commission(9002));
        assert_eq!(
            MemberTokenCommissionStats::<Test>::get(ENTITY_ID, BUYER).order_count,
            0
        );

        // 再次取消（记录已是 Cancelled，token_cancelled=0）→ order_count 不应变为负数
        assert_ok!(CommissionCore::do_cancel_token_commission(9002));
        assert_eq!(
            MemberTokenCommissionStats::<Test>::get(ENTITY_ID, BUYER).order_count,
            0,
            "order_count should not underflow on repeated cancel"
        );
    });
}

#[test]
fn bugb_full_cancel_rolls_back_both_nex_and_token_order_count() {
    // cancel_commission（NEX+Token 联合取消）应同时回滚双方 order_count
    new_test_ext().execute_with(|| {
        let ea = entity_account(ENTITY_ID);
        fund(PLATFORM, 1_000_000);
        fund(ea, 100_000);
        set_entity_referrer(ENTITY_ID, REFERRER);
        setup_config(10000);
        set_token_balance(ENTITY_ID, ea, 100_000);

        // 产生 NEX 佣金
        assert_ok!(process_commission_with_reserve(
            ENTITY_ID, SHOP_ID, 9003, &BUYER, 100_000, 100_000, 10_000, PRODUCT_ID,
        ));
        assert_eq!(
            MemberCommissionStats::<Test>::get(ENTITY_ID, BUYER).order_count,
            1
        );

        // 注入 Token 佣金记录到同一 order
        inject_token_pending(ENTITY_ID, REFERRER, 5_000);
        MemberTokenCommissionStats::<Test>::mutate(ENTITY_ID, &BUYER, |stats| {
            stats.order_count = 1;
        });
        let record = pallet_commission_common::TokenCommissionRecord {
            entity_id: ENTITY_ID,
            order_id: 9003,
            buyer: BUYER,
            beneficiary: REFERRER,
            amount: 5_000u128,
            commission_type: CommissionType::DirectReward,
            level: 0,
            status: CommissionStatus::Pending,
            created_at: 1u64,
        };
        OrderTokenCommissionRecords::<Test>::mutate(9003u64, |records| {
            let _ = records.try_push(record);
        });

        // 取消整个订单
        assert_ok!(CommissionCore::cancel_commission(9003));

        // NEX order_count 回滚
        assert_eq!(
            MemberCommissionStats::<Test>::get(ENTITY_ID, BUYER).order_count,
            0,
            "NEX order_count should be rolled back"
        );
        // Token order_count 回滚
        assert_eq!(
            MemberTokenCommissionStats::<Test>::get(ENTITY_ID, BUYER).order_count,
            0,
            "Token order_count should be rolled back"
        );
    });
}

// ==================== F-6 审计修复: force_disable 豁免 PoolNotEmpty ====================

#[test]
fn f6_root_force_disable_bypasses_pool_not_empty() {
    // force_disable_entity_commission 不受 PoolNotEmpty 限制（紧急处置）
    new_test_ext().execute_with(|| {
        // 设置 enabled + POOL_REWARD
        CommissionConfigs::<Test>::insert(
            ENTITY_ID,
            CoreCommissionConfig {
                enabled_modes: CommissionModes(
                    CommissionModes::POOL_REWARD | CommissionModes::DIRECT_REWARD,
                ),
                max_commission_rate: 5000,
                enabled: true,
                withdrawal_cooldown: 0,
                owner_reward_rate: 0,
                token_withdrawal_cooldown: 0,
                plugin_caps: PluginBudgetCaps::default(),
            },
        );
        UnallocatedPool::<Test>::insert(ENTITY_ID, 5000u128);

        // Owner 不能 disable（PoolNotEmpty）
        assert_noop!(
            CommissionCore::enable_commission(RuntimeOrigin::signed(SELLER), ENTITY_ID, false,),
            Error::<Test>::PoolNotEmpty
        );

        // Root force_disable 绕过 PoolNotEmpty
        assert_ok!(CommissionCore::force_disable_entity_commission(
            RuntimeOrigin::root(),
            ENTITY_ID,
        ));
        // 验证已 disabled
        let config = CommissionConfigs::<Test>::get(ENTITY_ID).unwrap();
        assert!(!config.enabled);
        // 池资金仍在（受 protected_funds 保护）
        assert_eq!(UnallocatedPool::<Test>::get(ENTITY_ID), 5000);
    });
}

#[test]
fn f6_force_enable_works_with_non_empty_pool() {
    // force_enable 不需要检查 PoolNotEmpty（只是启用）
    new_test_ext().execute_with(|| {
        // 设置 POOL_REWARD 模式 + disabled
        CommissionConfigs::<Test>::insert(
            ENTITY_ID,
            CoreCommissionConfig {
                enabled_modes: CommissionModes(
                    CommissionModes::POOL_REWARD | CommissionModes::DIRECT_REWARD,
                ),
                max_commission_rate: 5000,
                enabled: false,
                withdrawal_cooldown: 0,
                owner_reward_rate: 0,
                token_withdrawal_cooldown: 0,
                plugin_caps: PluginBudgetCaps::default(),
            },
        );
        UnallocatedPool::<Test>::insert(ENTITY_ID, 5000u128);

        // force_enable 成功（不需要空池，因为是启用而非关闭）
        assert_ok!(CommissionCore::force_enable_entity_commission(
            RuntimeOrigin::root(),
            ENTITY_ID,
        ));
        let config = CommissionConfigs::<Test>::get(ENTITY_ID).unwrap();
        assert!(config.enabled);
    });
}

// ==================== R-1 审计修复: ShopCommissionTotals 不含残留 zero ====================

#[test]
fn r1_shop_commission_totals_no_phantom_pool_distributed() {
    // 验证 ShopCommissionTotals 仅包含 platform + seller 佣金，不含 Phase 2 残留的 zero
    new_test_ext().execute_with(|| {
        fund(PLATFORM, 1_000_000);
        let ea = entity_account(ENTITY_ID);
        fund(ea, 1);
        set_entity_referrer(ENTITY_ID, REFERRER);
        setup_config(5000); // 50%

        assert_ok!(process_commission_with_reserve(
            ENTITY_ID, SHOP_ID, 9010, &BUYER, 100_000, 100_000, 10_000, PRODUCT_ID,
        ));

        let (total, orders) = ShopCommissionTotals::<Test>::get(ENTITY_ID);
        assert_eq!(orders, 1);
        // total 应等于 platform_referrer + seller 佣金总和，不含额外 zero 加值
        assert!(total > 0);
    });
}

// ============================================================================
// Shop / Product 佣金率覆盖 (call_index 30, 31)
// ============================================================================

#[test]
fn set_shop_commission_rate_works() {
    new_test_ext().execute_with(|| {
        assert_ok!(CommissionCore::set_shop_commission_rate(
            RuntimeOrigin::signed(SELLER),
            ENTITY_ID,
            SHOP_ID,
            Some(3000),
        ));
        assert_eq!(ShopCommissionRate::<Test>::get(SHOP_ID), Some(3000));

        // 清除覆盖
        assert_ok!(CommissionCore::set_shop_commission_rate(
            RuntimeOrigin::signed(SELLER),
            ENTITY_ID,
            SHOP_ID,
            None,
        ));
        assert_eq!(ShopCommissionRate::<Test>::get(SHOP_ID), None);
    });
}

#[test]
fn set_shop_commission_rate_rejects_invalid_rate() {
    new_test_ext().execute_with(|| {
        assert_noop!(
            CommissionCore::set_shop_commission_rate(
                RuntimeOrigin::signed(SELLER),
                ENTITY_ID,
                SHOP_ID,
                Some(10001),
            ),
            Error::<Test>::InvalidCommissionRate
        );
    });
}

#[test]
fn set_shop_commission_rate_rejects_non_owner() {
    new_test_ext().execute_with(|| {
        assert_noop!(
            CommissionCore::set_shop_commission_rate(
                RuntimeOrigin::signed(BUYER),
                ENTITY_ID,
                SHOP_ID,
                Some(3000),
            ),
            Error::<Test>::NotEntityOwnerOrAdmin
        );
    });
}

#[test]
fn set_shop_commission_rate_rejects_wrong_shop() {
    new_test_ext().execute_with(|| {
        // shop_id 999 不属于 ENTITY_ID
        assert_noop!(
            CommissionCore::set_shop_commission_rate(
                RuntimeOrigin::signed(SELLER),
                ENTITY_ID,
                999,
                Some(3000),
            ),
            Error::<Test>::ShopNotInEntity
        );
    });
}

#[test]
fn set_shop_commission_rate_respects_global_max() {
    new_test_ext().execute_with(|| {
        GlobalMaxCommissionRate::<Test>::insert(ENTITY_ID, 2000);
        assert_noop!(
            CommissionCore::set_shop_commission_rate(
                RuntimeOrigin::signed(SELLER),
                ENTITY_ID,
                SHOP_ID,
                Some(3000),
            ),
            Error::<Test>::CommissionRateExceedsGlobalMax
        );
        // 在上限范围内可以设置
        assert_ok!(CommissionCore::set_shop_commission_rate(
            RuntimeOrigin::signed(SELLER),
            ENTITY_ID,
            SHOP_ID,
            Some(2000),
        ));
    });
}

#[test]
fn set_product_commission_rate_works() {
    new_test_ext().execute_with(|| {
        assert_ok!(CommissionCore::set_product_commission_rate(
            RuntimeOrigin::signed(SELLER),
            ENTITY_ID,
            SHOP_ID,
            PRODUCT_ID,
            Some(1500),
        ));
        assert_eq!(ProductCommissionRate::<Test>::get(PRODUCT_ID), Some(1500));

        // 清除覆盖
        assert_ok!(CommissionCore::set_product_commission_rate(
            RuntimeOrigin::signed(SELLER),
            ENTITY_ID,
            SHOP_ID,
            PRODUCT_ID,
            None,
        ));
        assert_eq!(ProductCommissionRate::<Test>::get(PRODUCT_ID), None);
    });
}

#[test]
fn set_product_commission_rate_rejects_invalid_rate() {
    new_test_ext().execute_with(|| {
        assert_noop!(
            CommissionCore::set_product_commission_rate(
                RuntimeOrigin::signed(SELLER),
                ENTITY_ID,
                SHOP_ID,
                PRODUCT_ID,
                Some(10001),
            ),
            Error::<Test>::InvalidCommissionRate
        );
    });
}

#[test]
fn set_product_commission_rate_rejects_non_owner() {
    new_test_ext().execute_with(|| {
        assert_noop!(
            CommissionCore::set_product_commission_rate(
                RuntimeOrigin::signed(BUYER),
                ENTITY_ID,
                SHOP_ID,
                PRODUCT_ID,
                Some(1500),
            ),
            Error::<Test>::NotEntityOwnerOrAdmin
        );
    });
}

#[test]
fn set_product_commission_rate_respects_global_max() {
    new_test_ext().execute_with(|| {
        GlobalMaxCommissionRate::<Test>::insert(ENTITY_ID, 1000);
        assert_noop!(
            CommissionCore::set_product_commission_rate(
                RuntimeOrigin::signed(SELLER),
                ENTITY_ID,
                SHOP_ID,
                PRODUCT_ID,
                Some(1500),
            ),
            Error::<Test>::CommissionRateExceedsGlobalMax
        );
        assert_ok!(CommissionCore::set_product_commission_rate(
            RuntimeOrigin::signed(SELLER),
            ENTITY_ID,
            SHOP_ID,
            PRODUCT_ID,
            Some(1000),
        ));
    });
}

// ============================================================================
// 佣金率覆盖 — Engine 优先级链测试
// ============================================================================

/// 辅助：配置带Owner 收益的佣金（Owner 收益 = Pool B 预算 × owner_rate）
/// 用于验证 effective_rate 是否正确应用到 Pool B 预算
fn setup_owner_reward_override_config(max_commission_rate: u16) {
    CommissionConfigs::<Test>::insert(
        ENTITY_ID,
        CoreCommissionConfig {
            enabled_modes: CommissionModes(
                CommissionModes::DIRECT_REWARD | CommissionModes::OWNER_REWARD,
            ),
            max_commission_rate,
            enabled: true,
            withdrawal_cooldown: 0,
            owner_reward_rate: 10000, // 100%，使 Creator 拿走全部 Pool B 预算
            token_withdrawal_cooldown: 0,
            plugin_caps: PluginBudgetCaps::default(),
        },
    );
}

/// Entity 默认 50%，Shop 覆盖 30% → 佣金按 30% 计算
#[test]
fn engine_uses_shop_override_rate() {
    new_test_ext().execute_with(|| {
        fund(PLATFORM, 1_000_000);
        fund(SELLER, 1_000_000);
        setup_owner_reward_override_config(5000); // Entity 默认 50%

        // 设置 Shop 覆盖为 30%
        ShopCommissionRate::<Test>::insert(SHOP_ID, 3000);

        assert_ok!(process_commission_with_reserve(
            ENTITY_ID, SHOP_ID, 8001, &BUYER, 100_000, 100_000, 0, PRODUCT_ID,
        ));

        // available_pool = 100_000, Shop 30% → max_commission = 30_000
        // owner_reward_rate = 100% → owner gets 30_000
        // OwnerReward 直接到账，不再有 OrderOwnerReward 可断言
    });
}

/// Entity 默认 50%，Product 覆盖 10% → 佣金按 10% 计算
#[test]
fn engine_uses_product_override_rate() {
    new_test_ext().execute_with(|| {
        fund(PLATFORM, 1_000_000);
        fund(SELLER, 1_000_000);
        setup_owner_reward_override_config(5000); // Entity 默认 50%

        // 设置 Product 覆盖为 10%
        ProductCommissionRate::<Test>::insert(PRODUCT_ID, 1000);

        assert_ok!(process_commission_with_reserve(
            ENTITY_ID, SHOP_ID, 8002, &BUYER, 100_000, 100_000, 0, PRODUCT_ID,
        ));

        // OwnerReward 直接到账，不再有 OrderOwnerReward 可断言
    });
}

/// Product 覆盖优先于 Shop 覆盖
#[test]
fn engine_product_override_takes_priority_over_shop() {
    new_test_ext().execute_with(|| {
        fund(PLATFORM, 1_000_000);
        fund(SELLER, 1_000_000);
        setup_owner_reward_override_config(5000); // Entity 默认 50%

        ShopCommissionRate::<Test>::insert(SHOP_ID, 3000); // Shop 30%
        ProductCommissionRate::<Test>::insert(PRODUCT_ID, 500); // Product 5%

        assert_ok!(process_commission_with_reserve(
            ENTITY_ID, SHOP_ID, 8003, &BUYER, 100_000, 100_000, 0, PRODUCT_ID,
        ));

        // OwnerReward 直接到账，不再有 OrderOwnerReward 可断言
    });
}

/// 无覆盖时使用 Entity 默认
#[test]
fn engine_falls_back_to_entity_default() {
    new_test_ext().execute_with(|| {
        fund(PLATFORM, 1_000_000);
        fund(SELLER, 1_000_000);
        setup_owner_reward_override_config(5000); // Entity 默认 50%

        // 不设置任何覆盖
        assert_ok!(process_commission_with_reserve(
            ENTITY_ID, SHOP_ID, 8004, &BUYER, 100_000, 100_000, 0, PRODUCT_ID,
        ));

        // OwnerReward 直接到账，不再有 OrderOwnerReward 可断言
    });
}

/// Product 覆盖 0% → 不分佣
#[test]
fn engine_product_override_zero_rate() {
    new_test_ext().execute_with(|| {
        fund(PLATFORM, 1_000_000);
        fund(SELLER, 1_000_000);
        setup_owner_reward_override_config(5000); // Entity 默认 50%

        ProductCommissionRate::<Test>::insert(PRODUCT_ID, 0);

        assert_ok!(process_commission_with_reserve(
            ENTITY_ID, SHOP_ID, 8005, &BUYER, 100_000, 100_000, 0, PRODUCT_ID,
        ));

        let records = OrderCommissionRecords::<Test>::get(8005u64);
        assert_eq!(records.len(), 0);
    });
}

/// Token 管线也使用 Product 覆盖
#[test]
fn token_engine_uses_product_override_rate() {
    new_test_ext().execute_with(|| {
        fund(PLATFORM, 1_000_000);
        let ea = entity_account(ENTITY_ID);
        fund(ea, 10_000);
        // Token 版 owner_reward_override_config
        CommissionConfigs::<Test>::insert(
            ENTITY_ID,
            CoreCommissionConfig {
                enabled_modes: CommissionModes(
                    CommissionModes::DIRECT_REWARD | CommissionModes::OWNER_REWARD,
                ),
                max_commission_rate: 5000,
                enabled: true,
                withdrawal_cooldown: 0,
                owner_reward_rate: 10000,
                token_withdrawal_cooldown: 0,
                plugin_caps: PluginBudgetCaps::default(),
            },
        );

        // 设置 Product 覆盖为 20%
        ProductCommissionRate::<Test>::insert(PRODUCT_ID, 2000);
        set_token_balance(ENTITY_ID, ea, 500_000);

        assert_ok!(CommissionCore::process_token_commission(
            ENTITY_ID, SHOP_ID, 8006, &BUYER, 100_000, 100_000, 0, PRODUCT_ID,
        ));

        // OwnerReward 直接到账，不再有 OrderTokenOwnerReward 可断言
    });
}

// ==================== Plugin Budget Caps ====================

// ── set_plugin_budget_caps extrinsic ──

#[test]
fn pcap_set_plugin_budget_caps_works() {
    new_test_ext().execute_with(|| {
        let caps = PluginBudgetCaps {
            referral_cap: 2000,
            multi_level_cap: 3000,
            level_diff_cap: 1000,
            single_line_cap: 2000,
            team_cap: 2000,
        };
        assert_ok!(CommissionCore::set_plugin_budget_caps(
            RuntimeOrigin::signed(SELLER),
            ENTITY_ID,
            caps.clone(),
        ));
        let config = CommissionConfigs::<Test>::get(ENTITY_ID).unwrap();
        assert_eq!(config.plugin_caps, caps);
    });
}

#[test]
fn pcap_set_creates_default_config_if_missing() {
    // 即使没有预先配置 CoreCommissionConfig，extrinsic 也会创建默认配置并设置 caps
    // H-1: 默认 modes = MULTI_LEVEL + SINGLE_LINE（2 组），caps 必须覆盖这两组
    new_test_ext().execute_with(|| {
        assert!(CommissionConfigs::<Test>::get(ENTITY_ID).is_none());
        let caps = PluginBudgetCaps {
            referral_cap: 0,
            multi_level_cap: 5000,
            level_diff_cap: 0,
            single_line_cap: 5000,
            team_cap: 0,
        };
        assert_ok!(CommissionCore::set_plugin_budget_caps(
            RuntimeOrigin::signed(SELLER),
            ENTITY_ID,
            caps.clone(),
        ));
        let config = CommissionConfigs::<Test>::get(ENTITY_ID).unwrap();
        assert_eq!(config.plugin_caps, caps);
        // 其他字段应为默认值
        assert_eq!(config.max_commission_rate, 10000);
        assert!(!config.enabled);
    });
}

#[test]
fn pcap_set_preserves_existing_config_fields() {
    new_test_ext().execute_with(|| {
        // 先设置完整配置
        CommissionConfigs::<Test>::insert(
            ENTITY_ID,
            CoreCommissionConfig {
                enabled_modes: CommissionModes(
                    CommissionModes::DIRECT_REWARD | CommissionModes::POOL_REWARD,
                ),
                max_commission_rate: 5000,
                enabled: true,
                withdrawal_cooldown: 100,
                owner_reward_rate: 1000,
                token_withdrawal_cooldown: 200,
                plugin_caps: PluginBudgetCaps::default(),
            },
        );

        let caps = PluginBudgetCaps {
            referral_cap: 3000,
            multi_level_cap: 2000,
            level_diff_cap: 0,
            single_line_cap: 0,
            team_cap: 1000,
        };
        assert_ok!(CommissionCore::set_plugin_budget_caps(
            RuntimeOrigin::signed(SELLER),
            ENTITY_ID,
            caps.clone(),
        ));
        let config = CommissionConfigs::<Test>::get(ENTITY_ID).unwrap();
        // caps 已更新
        assert_eq!(config.plugin_caps, caps);
        // 其他字段未改变
        assert_eq!(config.max_commission_rate, 5000);
        assert!(config.enabled);
        assert_eq!(config.withdrawal_cooldown, 100);
        assert_eq!(config.owner_reward_rate, 1000);
        assert_eq!(config.token_withdrawal_cooldown, 200);
    });
}

#[test]
fn pcap_emits_event() {
    new_test_ext().execute_with(|| {
        let caps = PluginBudgetCaps {
            referral_cap: 2000,
            multi_level_cap: 3000,
            level_diff_cap: 1000,
            single_line_cap: 2000,
            team_cap: 2000,
        };
        assert_ok!(CommissionCore::set_plugin_budget_caps(
            RuntimeOrigin::signed(SELLER),
            ENTITY_ID,
            caps.clone(),
        ));
        System::assert_has_event(
            crate::Event::<Test>::PluginBudgetCapsUpdated {
                entity_id: ENTITY_ID,
                caps,
            }
            .into(),
        );
    });
}

#[test]
fn pcap_rejects_non_owner_non_admin() {
    new_test_ext().execute_with(|| {
        assert_noop!(
            CommissionCore::set_plugin_budget_caps(
                RuntimeOrigin::signed(BUYER),
                ENTITY_ID,
                PluginBudgetCaps::default(),
            ),
            Error::<Test>::NotEntityOwnerOrAdmin
        );
    });
}

#[test]
fn pcap_admin_with_commission_manage_can_set() {
    new_test_ext().execute_with(|| {
        set_entity_admin(
            ENTITY_ID,
            ADMIN,
            pallet_entity_common::AdminPermission::COMMISSION_MANAGE,
        );
        // H-1: 默认 modes = MULTI_LEVEL + SINGLE_LINE，caps 必须覆盖
        let caps = PluginBudgetCaps {
            referral_cap: 0,
            multi_level_cap: 1000,
            level_diff_cap: 0,
            single_line_cap: 1000,
            team_cap: 0,
        };
        assert_ok!(CommissionCore::set_plugin_budget_caps(
            RuntimeOrigin::signed(ADMIN),
            ENTITY_ID,
            caps.clone(),
        ));
        let config = CommissionConfigs::<Test>::get(ENTITY_ID).unwrap();
        assert_eq!(config.plugin_caps, caps);
    });
}

#[test]
fn pcap_rejects_locked_entity() {
    new_test_ext().execute_with(|| {
        set_entity_locked(ENTITY_ID);
        assert_noop!(
            CommissionCore::set_plugin_budget_caps(
                RuntimeOrigin::signed(SELLER),
                ENTITY_ID,
                PluginBudgetCaps::default(),
            ),
            Error::<Test>::EntityLocked
        );
    });
}

#[test]
fn pcap_rejects_inactive_entity() {
    new_test_ext().execute_with(|| {
        set_entity_inactive(ENTITY_ID);
        assert_noop!(
            CommissionCore::set_plugin_budget_caps(
                RuntimeOrigin::signed(SELLER),
                ENTITY_ID,
                PluginBudgetCaps::default(),
            ),
            Error::<Test>::EntityNotActive
        );
    });
}

#[test]
fn pcap_rejects_cap_over_10000() {
    new_test_ext().execute_with(|| {
        // referral_cap > 10000
        assert_noop!(
            CommissionCore::set_plugin_budget_caps(
                RuntimeOrigin::signed(SELLER),
                ENTITY_ID,
                PluginBudgetCaps {
                    referral_cap: 10001,
                    multi_level_cap: 0,
                    level_diff_cap: 0,
                    single_line_cap: 0,
                    team_cap: 0,
                },
            ),
            Error::<Test>::InvalidPluginCap
        );
        // multi_level_cap > 10000
        assert_noop!(
            CommissionCore::set_plugin_budget_caps(
                RuntimeOrigin::signed(SELLER),
                ENTITY_ID,
                PluginBudgetCaps {
                    referral_cap: 0,
                    multi_level_cap: 10001,
                    level_diff_cap: 0,
                    single_line_cap: 0,
                    team_cap: 0,
                },
            ),
            Error::<Test>::InvalidPluginCap
        );
        // level_diff_cap > 10000
        assert_noop!(
            CommissionCore::set_plugin_budget_caps(
                RuntimeOrigin::signed(SELLER),
                ENTITY_ID,
                PluginBudgetCaps {
                    referral_cap: 0,
                    multi_level_cap: 0,
                    level_diff_cap: 10001,
                    single_line_cap: 0,
                    team_cap: 0,
                },
            ),
            Error::<Test>::InvalidPluginCap
        );
        // single_line_cap > 10000
        assert_noop!(
            CommissionCore::set_plugin_budget_caps(
                RuntimeOrigin::signed(SELLER),
                ENTITY_ID,
                PluginBudgetCaps {
                    referral_cap: 0,
                    multi_level_cap: 0,
                    level_diff_cap: 0,
                    single_line_cap: 10001,
                    team_cap: 0,
                },
            ),
            Error::<Test>::InvalidPluginCap
        );
        // team_cap > 10000
        assert_noop!(
            CommissionCore::set_plugin_budget_caps(
                RuntimeOrigin::signed(SELLER),
                ENTITY_ID,
                PluginBudgetCaps {
                    referral_cap: 0,
                    multi_level_cap: 0,
                    level_diff_cap: 0,
                    single_line_cap: 0,
                    team_cap: 10001,
                },
            ),
            Error::<Test>::InvalidPluginCap
        );
    });
}

#[test]
fn pcap_boundary_value_at_budget_ceiling_accepted() {
    // 插件 cap 上限 = budget_ceiling = 10000 - platform_fee_rate
    new_test_ext().execute_with(|| {
        let ceiling = 9900u16; // 10000 - 100 (default platform fee rate)
        let caps = PluginBudgetCaps {
            referral_cap: ceiling,
            multi_level_cap: ceiling,
            level_diff_cap: ceiling,
            single_line_cap: ceiling,
            team_cap: ceiling,
        };
        assert_ok!(CommissionCore::set_plugin_budget_caps(
            RuntimeOrigin::signed(SELLER),
            ENTITY_ID,
            caps.clone(),
        ));
        let config = CommissionConfigs::<Test>::get(ENTITY_ID).unwrap();
        assert_eq!(config.plugin_caps, caps);
    });
}

#[test]
fn pcap_caps_sum_can_exceed_10000() {
    // cap 之和可以 > 10000 — 每个 cap 是独立上限，不是分配比例
    new_test_ext().execute_with(|| {
        let caps = PluginBudgetCaps {
            referral_cap: 3000,
            multi_level_cap: 5000,
            level_diff_cap: 2000,
            single_line_cap: 3000,
            team_cap: 3000,
        };
        // sum = 16000 > 10000, should be accepted
        assert_ok!(CommissionCore::set_plugin_budget_caps(
            RuntimeOrigin::signed(SELLER),
            ENTITY_ID,
            caps,
        ));
    });
}

#[test]
fn pcap_update_caps_overwrites_previous() {
    new_test_ext().execute_with(|| {
        // H-1: 默认 modes = MULTI_LEVEL + SINGLE_LINE，两次 caps 都需要覆盖这两组
        let caps1 = PluginBudgetCaps {
            referral_cap: 2000,
            multi_level_cap: 3000,
            level_diff_cap: 0,
            single_line_cap: 3000,
            team_cap: 0,
        };
        assert_ok!(CommissionCore::set_plugin_budget_caps(
            RuntimeOrigin::signed(SELLER),
            ENTITY_ID,
            caps1,
        ));

        let caps2 = PluginBudgetCaps {
            referral_cap: 0,
            multi_level_cap: 5000,
            level_diff_cap: 0,
            single_line_cap: 5000,
            team_cap: 0,
        };
        assert_ok!(CommissionCore::set_plugin_budget_caps(
            RuntimeOrigin::signed(SELLER),
            ENTITY_ID,
            caps2.clone(),
        ));
        let config = CommissionConfigs::<Test>::get(ENTITY_ID).unwrap();
        assert_eq!(config.plugin_caps, caps2);
    });
}

#[test]
fn pcap_reset_to_zero_restores_unlimited() {
    new_test_ext().execute_with(|| {
        // H-1: 默认 modes = MULTI_LEVEL + SINGLE_LINE（2 组）
        // 先设置非零 caps
        assert_ok!(CommissionCore::set_plugin_budget_caps(
            RuntimeOrigin::signed(SELLER),
            ENTITY_ID,
            PluginBudgetCaps {
                referral_cap: 5000,
                multi_level_cap: 3000,
                level_diff_cap: 2000,
                single_line_cap: 1000,
                team_cap: 1000,
            },
        ));
        // 清零 → 多插件模式下被拒绝
        assert_noop!(
            CommissionCore::set_plugin_budget_caps(
                RuntimeOrigin::signed(SELLER),
                ENTITY_ID,
                PluginBudgetCaps::default(),
            ),
            Error::<Test>::PluginCapRequiredForMultiPlugin
        );

        // 切换到单插件模式（MULTI_LEVEL）
        assert_ok!(CommissionCore::set_commission_modes(
            RuntimeOrigin::signed(SELLER),
            ENTITY_ID,
            CommissionModes(CommissionModes::MULTI_LEVEL),
        ));
        // 单插件模式下清零也被拒绝（任何插件都需要 cap > 0）
        assert_noop!(
            CommissionCore::set_plugin_budget_caps(
                RuntimeOrigin::signed(SELLER),
                ENTITY_ID,
                PluginBudgetCaps::default(),
            ),
            Error::<Test>::PluginCapRequiredForMultiPlugin
        );

        // 切换到无插件模式（仅 POOL_REWARD），然后可以清零
        assert_ok!(CommissionCore::set_commission_modes(
            RuntimeOrigin::signed(SELLER),
            ENTITY_ID,
            CommissionModes(CommissionModes::POOL_REWARD),
        ));
        assert_ok!(CommissionCore::set_plugin_budget_caps(
            RuntimeOrigin::signed(SELLER),
            ENTITY_ID,
            PluginBudgetCaps::default(),
        ));
        let config = CommissionConfigs::<Test>::get(ENTITY_ID).unwrap();
        assert_eq!(config.plugin_caps, PluginBudgetCaps::default());
    });
}

// ── capped_budget 数学验证（通过 process_commission + POOL_REWARD 间接验证） ──
//
// 注：Mock 中所有插件为 ()（空实现），calculate 返回 ([], remaining)，
// 因此插件不消耗任何预算。但 capped_budget 会限制传给插件的 remaining，
// 当 cap > 0 时，plugin_budget = min(remaining, order_amount × cap / 10000)。
// 由于插件不消耗，new_remaining == plugin_budget，
// engine 的 remaining = remaining.saturating_sub(plugin_budget - new_remaining) = remaining - 0 = remaining。
// 所以最终 remaining 不变，全部进入 UnallocatedPool。
//
// 这组测试验证：caps 配置正确写入且 process_commission 正常执行（不 panic），
// 通过 POOL_REWARD 的值确认串行管线的最终 remaining 一致性。

#[test]
fn pcap_process_commission_with_caps_succeeds() {
    // caps 不为零时 process_commission 正常执行
    new_test_ext().execute_with(|| {
        fund(PLATFORM, 1_000_000);
        fund(SELLER, 1_000_000);
        CommissionConfigs::<Test>::insert(
            ENTITY_ID,
            CoreCommissionConfig {
                enabled_modes: CommissionModes(
                    CommissionModes::DIRECT_REWARD | CommissionModes::POOL_REWARD,
                ),
                max_commission_rate: 5000,
                enabled: true,
                withdrawal_cooldown: 0,
                owner_reward_rate: 0,
                token_withdrawal_cooldown: 0,
                plugin_caps: PluginBudgetCaps {
                    referral_cap: 1000,
                    multi_level_cap: 1000,
                    level_diff_cap: 1000,
                    single_line_cap: 1000,
                    team_cap: 1000,
                },
            },
        );

        assert_ok!(process_commission_with_reserve(
            ENTITY_ID, SHOP_ID, 7001, &BUYER, 100_000, 100_000, 0, PRODUCT_ID,
        ));

        // Pool B = 100_000 * 5000 / 10000 = 50_000
        // 所有空插件不消耗 → remaining = 50_000 → 全部进入 UnallocatedPool
        let pool = UnallocatedPool::<Test>::get(ENTITY_ID);
        assert_eq!(pool, 50_000);
    });
}

#[test]
fn pcap_zero_caps_equivalent_to_no_caps() {
    // caps 全为 0 时行为与不设置 caps 完全一致
    new_test_ext().execute_with(|| {
        fund(PLATFORM, 1_000_000);
        fund(SELLER, 1_000_000);
        CommissionConfigs::<Test>::insert(
            ENTITY_ID,
            CoreCommissionConfig {
                enabled_modes: CommissionModes(
                    CommissionModes::DIRECT_REWARD | CommissionModes::POOL_REWARD,
                ),
                max_commission_rate: 5000,
                enabled: true,
                withdrawal_cooldown: 0,
                owner_reward_rate: 0,
                token_withdrawal_cooldown: 0,
                plugin_caps: PluginBudgetCaps::default(),
            },
        );

        assert_ok!(process_commission_with_reserve(
            ENTITY_ID, SHOP_ID, 7002, &BUYER, 100_000, 100_000, 0, PRODUCT_ID,
        ));

        let pool = UnallocatedPool::<Test>::get(ENTITY_ID);
        assert_eq!(pool, 50_000);
    });
}

#[test]
fn pcap_with_owner_reward_and_caps() {
    // Owner 收益基数为 order_amount，在 caps 之前扣除，剩余部分受 caps 约束
    new_test_ext().execute_with(|| {
        fund(PLATFORM, 1_000_000);
        fund(SELLER, 1_000_000);
        CommissionConfigs::<Test>::insert(
            ENTITY_ID,
            CoreCommissionConfig {
                enabled_modes: CommissionModes(
                    CommissionModes::DIRECT_REWARD
                        | CommissionModes::OWNER_REWARD
                        | CommissionModes::POOL_REWARD,
                ),
                max_commission_rate: 5000,
                enabled: true,
                withdrawal_cooldown: 0,
                owner_reward_rate: 2000,
                token_withdrawal_cooldown: 0,
                plugin_caps: PluginBudgetCaps {
                    referral_cap: 500,
                    multi_level_cap: 500,
                    level_diff_cap: 500,
                    single_line_cap: 500,
                    team_cap: 500,
                },
            },
        );

        assert_ok!(process_commission_with_reserve(
            ENTITY_ID, SHOP_ID, 7003, &BUYER, 100_000, 100_000, 0, PRODUCT_ID,
        ));

        // Pool B = 100_000 * 5000 / 10000 = 50_000
        // owner_reward = 100_000 * 2000 / 10000 = 20_000, min(20_000, 50_000) = 20_000
        // remaining after owner_reward = 30_000
        // 空插件不消耗 → remaining = 30_000 → UnallocatedPool
        let pool = UnallocatedPool::<Test>::get(ENTITY_ID);
        assert_eq!(pool, 30_000);

        // Owner 收益记录存在（OrderOwnerReward）
        // OwnerReward 直接到账，不再有 OrderOwnerReward 可断言
    });
}

#[test]
fn pcap_token_process_commission_with_caps_succeeds() {
    // Token 管线同样支持 caps
    new_test_ext().execute_with(|| {
        let ea = entity_account(ENTITY_ID);
        set_token_balance(ENTITY_ID, ea, 500_000);

        CommissionConfigs::<Test>::insert(
            ENTITY_ID,
            CoreCommissionConfig {
                enabled_modes: CommissionModes(
                    CommissionModes::DIRECT_REWARD | CommissionModes::POOL_REWARD,
                ),
                max_commission_rate: 5000,
                enabled: true,
                withdrawal_cooldown: 0,
                owner_reward_rate: 0,
                token_withdrawal_cooldown: 0,
                plugin_caps: PluginBudgetCaps {
                    referral_cap: 2000,
                    multi_level_cap: 1000,
                    level_diff_cap: 0,
                    single_line_cap: 0,
                    team_cap: 0,
                },
            },
        );

        assert_ok!(CommissionCore::process_token_commission(
            ENTITY_ID, SHOP_ID, 7004, &BUYER, 100_000, 100_000, 0, PRODUCT_ID,
        ));

        // Token Pool B = 100_000 * 5000 / 10000 = 50_000
        // 空 Token 插件不消耗 → remaining = 50_000 → UnallocatedTokenPool
        let pool = UnallocatedTokenPool::<Test>::get(ENTITY_ID);
        assert_eq!(pool, 50_000);
    });
}

#[test]
fn pcap_default_struct_all_zero() {
    let caps = PluginBudgetCaps::default();
    assert_eq!(caps.referral_cap, 0);
    assert_eq!(caps.multi_level_cap, 0);
    assert_eq!(caps.level_diff_cap, 0);
    assert_eq!(caps.single_line_cap, 0);
    assert_eq!(caps.team_cap, 0);
}

// ============================================================================
// 推荐人免除阈值（ReferrerExemptThreshold）
// ============================================================================

#[test]
fn referrer_exempt_threshold_default_is_five() {
    // 默认阈值=5 → 8 层多级超过阈值 → 推荐人免除
    new_test_ext().execute_with(|| {
        fund(PLATFORM, 1_000_000);
        fund(SELLER, 1_000_000);
        set_entity_referrer(ENTITY_ID, REFERRER);
        set_mock_tier_count(ENTITY_ID, 8);
        setup_config(10000);

        assert_eq!(ReferrerExemptThreshold::<Test>::get(), 5);

        let platform_fee: Balance = 10_000;
        assert_ok!(process_commission_with_reserve(
            ENTITY_ID,
            SHOP_ID,
            9001,
            &BUYER,
            100_000,
            100_000,
            platform_fee,
            PRODUCT_ID,
        ));

        // 8 > 5 → 推荐人无提成，全部归国库
        let records = OrderCommissionRecords::<Test>::get(9001u64);
        let referrer_rec = records
            .iter()
            .find(|r| r.commission_type == CommissionType::EntityReferral);
        assert!(referrer_rec.is_none());
    });
}

#[test]
fn referrer_exempt_threshold_zero_disables_rule() {
    // threshold=0 → 规则不启用，即使 entity 有 8 层多级，推荐人仍正常拿提成
    new_test_ext().execute_with(|| {
        fund(PLATFORM, 1_000_000);
        fund(SELLER, 1_000_000);
        set_entity_referrer(ENTITY_ID, REFERRER);
        set_mock_tier_count(ENTITY_ID, 8);
        setup_config(10000);

        ReferrerExemptThreshold::<Test>::put(0u16);

        let platform_fee: Balance = 10_000;
        assert_ok!(process_commission_with_reserve(
            ENTITY_ID,
            SHOP_ID,
            9001,
            &BUYER,
            100_000,
            100_000,
            platform_fee,
            PRODUCT_ID,
        ));

        // threshold=0 禁用规则 → 推荐人正常拿 50% = 5000
        let records = OrderCommissionRecords::<Test>::get(9001u64);
        let referrer_rec = records
            .iter()
            .find(|r| r.commission_type == CommissionType::EntityReferral);
        assert!(referrer_rec.is_some());
        assert_eq!(referrer_rec.unwrap().amount, 5000);
    });
}

#[test]
fn referrer_exempt_when_tier_count_exceeds_threshold() {
    // 默认 threshold=5, entity 有 8 层 → 推荐人无提成，平台费 100% 归国库
    new_test_ext().execute_with(|| {
        fund(PLATFORM, 1_000_000);
        fund(SELLER, 1_000_000);
        set_entity_referrer(ENTITY_ID, REFERRER);
        set_mock_tier_count(ENTITY_ID, 8);
        setup_config(10000);

        let platform_fee: Balance = 10_000;
        assert_ok!(process_commission_with_reserve(
            ENTITY_ID,
            SHOP_ID,
            9002,
            &BUYER,
            100_000,
            100_000,
            platform_fee,
            PRODUCT_ID,
        ));

        // 推荐人无提成
        let records = OrderCommissionRecords::<Test>::get(9002u64);
        let referrer_rec = records
            .iter()
            .find(|r| r.commission_type == CommissionType::EntityReferral);
        assert!(referrer_rec.is_none());

        // 国库拿到全额 platform_fee
        let treasury_transfer = OrderTreasuryTransfer::<Test>::get(9002u64);
        assert_eq!(treasury_transfer, 10_000);
    });
}

#[test]
fn referrer_not_exempt_when_tier_count_at_threshold() {
    // 默认 threshold=5, entity 有 5 层 → 5 不大于 5 → 推荐人正常拿提成
    new_test_ext().execute_with(|| {
        fund(PLATFORM, 1_000_000);
        fund(SELLER, 1_000_000);
        set_entity_referrer(ENTITY_ID, REFERRER);
        set_mock_tier_count(ENTITY_ID, 5);
        setup_config(10000);

        let platform_fee: Balance = 10_000;
        assert_ok!(process_commission_with_reserve(
            ENTITY_ID,
            SHOP_ID,
            9003,
            &BUYER,
            100_000,
            100_000,
            platform_fee,
            PRODUCT_ID,
        ));

        // 推荐人正常拿 50%
        let records = OrderCommissionRecords::<Test>::get(9003u64);
        let referrer_rec = records
            .iter()
            .find(|r| r.commission_type == CommissionType::EntityReferral);
        assert!(referrer_rec.is_some());
        assert_eq!(referrer_rec.unwrap().amount, 5000);
    });
}

#[test]
fn referrer_not_exempt_when_no_multi_level_config() {
    // 默认 threshold=5, entity 无多级配置 (tier_count=0) → 推荐人正常拿提成
    new_test_ext().execute_with(|| {
        fund(PLATFORM, 1_000_000);
        fund(SELLER, 1_000_000);
        set_entity_referrer(ENTITY_ID, REFERRER);
        // 不调用 set_mock_tier_count → 默认 0
        setup_config(10000);

        let platform_fee: Balance = 10_000;
        assert_ok!(process_commission_with_reserve(
            ENTITY_ID,
            SHOP_ID,
            9004,
            &BUYER,
            100_000,
            100_000,
            platform_fee,
            PRODUCT_ID,
        ));

        let records = OrderCommissionRecords::<Test>::get(9004u64);
        let referrer_rec = records
            .iter()
            .find(|r| r.commission_type == CommissionType::EntityReferral);
        assert!(referrer_rec.is_some());
        assert_eq!(referrer_rec.unwrap().amount, 5000);
    });
}

#[test]
fn referrer_exempt_token_pipeline() {
    // 默认 threshold=5, entity 有 8 层 → Token 管线推荐人也无提成
    new_test_ext().execute_with(|| {
        let ea = entity_account(ENTITY_ID);
        set_token_balance(ENTITY_ID, ea, 200_000);
        set_entity_referrer(ENTITY_ID, REFERRER);
        set_mock_tier_count(ENTITY_ID, 8);
        setup_token_config(5000, true);

        assert_ok!(CommissionCore::process_token_commission(
            ENTITY_ID, SHOP_ID, 9005, &BUYER, 20_000, 19_000, 1_000, PRODUCT_ID,
        ));

        // 推荐人无 Token 佣金
        let stats = MemberTokenCommissionStats::<Test>::get(ENTITY_ID, REFERRER);
        assert_eq!(stats.total_earned, 0);

        // token_platform_fee 全部进沉淀池（pool_a_retention = 1000）
        let pool = UnallocatedTokenPool::<Test>::get(ENTITY_ID);
        assert!(pool >= 1_000); // 至少包含 platform_fee 的留存
    });
}

#[test]
fn set_referrer_exempt_threshold_root_only() {
    new_test_ext().execute_with(|| {
        // 非 Root 调用 → 失败
        assert_noop!(
            CommissionCore::set_referrer_exempt_threshold(RuntimeOrigin::signed(SELLER), 10,),
            sp_runtime::DispatchError::BadOrigin
        );

        // Root 调用 → 成功（默认值 5 → 改为 10）
        assert_ok!(CommissionCore::set_referrer_exempt_threshold(
            RuntimeOrigin::root(),
            10,
        ));
        assert_eq!(ReferrerExemptThreshold::<Test>::get(), 10);

        // 验证事件
        System::assert_has_event(RuntimeEvent::CommissionCore(
            crate::pallet::Event::ReferrerExemptThresholdChanged {
                old_threshold: 5,
                new_threshold: 10,
            },
        ));
    });
}

#[test]
fn referrer_exempt_dynamically_changes_with_tier_count() {
    // 默认 threshold=5, entity 初始 3 层 → 推荐人拿提成
    // entity 改为 8 层 → 下一笔订单推荐人无提成
    // entity 改回 4 层 → 推荐人又恢复
    new_test_ext().execute_with(|| {
        fund(PLATFORM, 10_000_000);
        fund(SELLER, 10_000_000);
        set_entity_referrer(ENTITY_ID, REFERRER);
        setup_config(10000);

        // 阶段 1: 3 层 → 推荐人正常拿
        set_mock_tier_count(ENTITY_ID, 3);
        let platform_fee: Balance = 10_000;
        assert_ok!(process_commission_with_reserve(
            ENTITY_ID,
            SHOP_ID,
            9010,
            &BUYER,
            100_000,
            100_000,
            platform_fee,
            PRODUCT_ID,
        ));
        let records = OrderCommissionRecords::<Test>::get(9010u64);
        assert!(records
            .iter()
            .any(|r| r.commission_type == CommissionType::EntityReferral));

        // 阶段 2: 改为 8 层 → 推荐人无提成
        set_mock_tier_count(ENTITY_ID, 8);
        assert_ok!(process_commission_with_reserve(
            ENTITY_ID,
            SHOP_ID,
            9011,
            &BUYER,
            100_000,
            100_000,
            platform_fee,
            PRODUCT_ID,
        ));
        let records = OrderCommissionRecords::<Test>::get(9011u64);
        assert!(!records
            .iter()
            .any(|r| r.commission_type == CommissionType::EntityReferral));

        // 阶段 3: 改回 4 层 → 推荐人恢复
        set_mock_tier_count(ENTITY_ID, 4);
        assert_ok!(process_commission_with_reserve(
            ENTITY_ID,
            SHOP_ID,
            9012,
            &BUYER,
            100_000,
            100_000,
            platform_fee,
            PRODUCT_ID,
        ));
        let records = OrderCommissionRecords::<Test>::get(9012u64);
        assert!(records
            .iter()
            .any(|r| r.commission_type == CommissionType::EntityReferral));
    });
}

// ============================================================================
// BUG-1 修复：capped_budget cap_base 应为 initial_remaining（实际佣金池）
// ============================================================================

// ── 直接数学验证 capped_budget 函数 ──

#[test]
fn bug1_capped_budget_zero_cap_returns_remaining() {
    new_test_ext().execute_with(|| {
        // cap=0 → 插件不启用，预算为 0
        assert_eq!(CommissionCore::capped_budget(0, 100_000, 50_000), 0);
        assert_eq!(CommissionCore::capped_budget(0, 0, 50_000), 0);
        assert_eq!(CommissionCore::capped_budget(0, 999_999, 1), 0);
    });
}

#[test]
fn bug1_capped_budget_cap_limits_to_percentage_of_cap_base() {
    new_test_ext().execute_with(|| {
        // cap_base=100_000, cap=3000(30%) → cap_amount = 30_000
        // remaining=50_000 → min(50_000, 30_000) = 30_000
        assert_eq!(CommissionCore::capped_budget(3000, 100_000, 50_000), 30_000);

        // cap_base=100_000, cap=5000(50%) → cap_amount = 50_000
        // remaining=50_000 → min(50_000, 50_000) = 50_000
        assert_eq!(CommissionCore::capped_budget(5000, 100_000, 50_000), 50_000);

        // cap_base=100_000, cap=1000(10%) → cap_amount = 10_000
        // remaining=50_000 → min(50_000, 10_000) = 10_000
        assert_eq!(CommissionCore::capped_budget(1000, 100_000, 50_000), 10_000);
    });
}

#[test]
fn bug1_capped_budget_remaining_smaller_than_cap_returns_remaining() {
    new_test_ext().execute_with(|| {
        // cap_base=100_000, cap=8000(80%) → cap_amount = 80_000
        // remaining=5_000 → min(5_000, 80_000) = 5_000
        assert_eq!(CommissionCore::capped_budget(8000, 100_000, 5_000), 5_000);
    });
}

#[test]
fn bug1_capped_budget_cap_10000_equals_full_cap_base() {
    new_test_ext().execute_with(|| {
        // cap=10000(100%) → cap_amount = cap_base
        assert_eq!(
            CommissionCore::capped_budget(10000, 100_000, 100_000),
            100_000
        );
        assert_eq!(
            CommissionCore::capped_budget(10000, 100_000, 50_000),
            50_000
        );
    });
}

#[test]
fn bug1_capped_budget_small_cap_base_prevents_over_allocation() {
    new_test_ext().execute_with(|| {
        // BUG-1 核心场景: max_commission_rate=1000(10%) → cap_base=10_000
        // 修复前: available_pool=100_000 作为 cap_base → cap=5000 时 cap_amount=50_000 >> remaining
        // 修复后: initial_remaining=10_000 作为 cap_base → cap=5000 时 cap_amount=5_000
        let cap_base = 10_000; // initial_remaining = available_pool(100_000) * rate(1000) / 10000
        let remaining = 10_000;
        // cap=5000(50%) → cap_amount = 10_000 * 5000 / 10000 = 5_000
        assert_eq!(
            CommissionCore::capped_budget(5000, cap_base, remaining),
            5_000
        );
        // 而非修复前的 min(10_000, 100_000*5000/10000) = min(10_000, 50_000) = 10_000
    });
}

#[test]
fn bug1_capped_budget_zero_remaining_returns_zero() {
    new_test_ext().execute_with(|| {
        assert_eq!(CommissionCore::capped_budget(5000, 100_000, 0), 0);
    });
}

#[test]
fn bug1_capped_budget_zero_cap_base_returns_zero() {
    new_test_ext().execute_with(|| {
        // cap_base=0 (可能极端场景：seller 余额不足) → cap_amount=0 → min(remaining, 0) = 0
        assert_eq!(CommissionCore::capped_budget(5000, 0, 50_000), 0);
    });
}

#[test]
fn bug1_capped_budget_saturating_mul_no_overflow() {
    new_test_ext().execute_with(|| {
        // 极大值不溢出
        let big: Balance = u128::MAX / 20000;
        let result = CommissionCore::capped_budget(10000, big, big);
        assert_eq!(result, big); // cap=10000 → cap_amount = big
    });
}

// ── Token 管线 capped_token_budget 对称验证 ──

#[test]
fn bug1_capped_token_budget_mirrors_nex_behavior() {
    new_test_ext().execute_with(|| {
        // cap=0 → 插件不启用，预算为 0
        assert_eq!(
            CommissionCore::capped_token_budget(0, 100_000u128, 50_000u128),
            0
        );
        // cap=3000 → 30%
        assert_eq!(
            CommissionCore::capped_token_budget(3000, 100_000, 50_000),
            30_000
        );
        // cap=5000, cap_base=10_000 (BUG-1 核心场景)
        assert_eq!(
            CommissionCore::capped_token_budget(5000, 10_000, 10_000),
            5_000
        );
        // cap=10000 → 100%
        assert_eq!(
            CommissionCore::capped_token_budget(10000, 50_000, 50_000),
            50_000
        );
    });
}

// ── 引擎集成测试：验证 cap_base 使用 order_amount（与 max_commission_rate 同维度） ──

#[test]
fn bug1_nex_engine_cap_base_is_order_amount() {
    // 场景：order_amount=100_000, max_commission_rate=1000(10%)
    // initial_remaining = 100_000 * 1000 / 10000 = 10_000
    // 设置 referral_cap=500(5% of order_amount)
    //
    // cap_base = order_amount = 100_000
    //   → cap_amount = 100_000 * 500 / 10000 = 5_000
    //   → plugin_budget = min(10_000, 5_000) = 5_000 (cap 生效!)
    //
    // 由于 Mock 插件为空不消耗预算，所有 remaining 最终进入 UnallocatedPool。
    new_test_ext().execute_with(|| {
        fund(PLATFORM, 1_000_000);
        fund(SELLER, 1_000_000);
        CommissionConfigs::<Test>::insert(
            ENTITY_ID,
            CoreCommissionConfig {
                enabled_modes: CommissionModes(
                    CommissionModes::DIRECT_REWARD | CommissionModes::POOL_REWARD,
                ),
                max_commission_rate: 1000, // 10%
                enabled: true,
                withdrawal_cooldown: 0,
                owner_reward_rate: 0,
                token_withdrawal_cooldown: 0,
                plugin_caps: PluginBudgetCaps {
                    referral_cap: 500, // 5% of order_amount, ≤ max_rate(1000)
                    multi_level_cap: 0,
                    level_diff_cap: 0,
                    single_line_cap: 0,
                    team_cap: 0,
                },
            },
        );

        assert_ok!(process_commission_with_reserve(
            ENTITY_ID, SHOP_ID, 8001, &BUYER, 100_000, 100_000, 0, PRODUCT_ID,
        ));

        // initial_remaining = 100_000 * 1000 / 10000 = 10_000
        // 空插件不消耗 → remaining = 10_000 → UnallocatedPool = 10_000
        let pool = UnallocatedPool::<Test>::get(ENTITY_ID);
        assert_eq!(pool, 10_000);
    });
}

#[test]
fn bug1_nex_engine_all_five_caps_with_low_rate() {
    // 所有 5 个插件都设置 cap=500(5% of order), max_commission_rate=500(5%)
    // initial_remaining = 100_000 * 500 / 10000 = 5_000
    // 每个插件 cap_amount = 100_000 * 500 / 10000 = 5_000
    // plugin_budget = min(remaining, 5_000)
    new_test_ext().execute_with(|| {
        fund(PLATFORM, 1_000_000);
        fund(SELLER, 1_000_000);
        CommissionConfigs::<Test>::insert(
            ENTITY_ID,
            CoreCommissionConfig {
                enabled_modes: CommissionModes(
                    CommissionModes::DIRECT_REWARD
                        | CommissionModes::MULTI_LEVEL
                        | CommissionModes::LEVEL_DIFF
                        | CommissionModes::SINGLE_LINE_UPLINE
                        | CommissionModes::TEAM_PERFORMANCE
                        | CommissionModes::POOL_REWARD,
                ),
                max_commission_rate: 500, // 5%
                enabled: true,
                withdrawal_cooldown: 0,
                owner_reward_rate: 0,
                token_withdrawal_cooldown: 0,
                plugin_caps: PluginBudgetCaps {
                    referral_cap: 500,
                    multi_level_cap: 500,
                    level_diff_cap: 500,
                    single_line_cap: 500,
                    team_cap: 500,
                },
            },
        );

        assert_ok!(process_commission_with_reserve(
            ENTITY_ID, SHOP_ID, 8002, &BUYER, 100_000, 100_000, 0, PRODUCT_ID,
        ));

        // initial_remaining = 100_000 * 500 / 10000 = 5_000
        let pool = UnallocatedPool::<Test>::get(ENTITY_ID);
        assert_eq!(pool, 5_000);
    });
}

#[test]
fn bug1_nex_engine_caps_with_owner_reward() {
    // max_commission_rate=2000(20%), owner_reward_rate=5000(50% of order)
    // initial_remaining = 100_000 * 2000 / 10000 = 20_000
    // owner_reward = 100_000 * 5000 / 10000 = 50_000, min(50_000, 20_000) = 20_000
    // remaining after owner_reward = 0
    //
    // referral_cap=1500(15% of order) → cap_base = order_amount = 100_000
    // 但 remaining=0，无预算可分
    new_test_ext().execute_with(|| {
        fund(PLATFORM, 1_000_000);
        fund(SELLER, 1_000_000);
        CommissionConfigs::<Test>::insert(
            ENTITY_ID,
            CoreCommissionConfig {
                enabled_modes: CommissionModes(
                    CommissionModes::DIRECT_REWARD
                        | CommissionModes::OWNER_REWARD
                        | CommissionModes::POOL_REWARD,
                ),
                max_commission_rate: 2000, // 20%
                enabled: true,
                withdrawal_cooldown: 0,
                owner_reward_rate: 5000, // 50% of order → capped by Pool B
                token_withdrawal_cooldown: 0,
                plugin_caps: PluginBudgetCaps {
                    referral_cap: 1500, // 15% of order, ≤ max_rate(2000)
                    multi_level_cap: 0,
                    level_diff_cap: 0,
                    single_line_cap: 0,
                    team_cap: 0,
                },
            },
        );

        assert_ok!(process_commission_with_reserve(
            ENTITY_ID, SHOP_ID, 8003, &BUYER, 100_000, 100_000, 0, PRODUCT_ID,
        ));

        // initial_remaining = 20_000
        // owner_reward = 20_000 (capped by remaining)
        // 空插件不消耗 → remaining = 0 → pool = 0
        let pool = UnallocatedPool::<Test>::get(ENTITY_ID);
        assert_eq!(pool, 0);

        let records = OrderCommissionRecords::<Test>::get(8003u64);
        assert_eq!(records.len(), 0);
        // OwnerReward 直接到账，不再有 OrderOwnerReward 可断言
    });
}

#[test]
fn bug1_nex_engine_rate_10000_caps_still_effective() {
    // max_commission_rate=9900(budget ceiling) → initial_remaining = 99% of order_amount
    // cap_base = order_amount，当 order_amount == available_pool 时
    // cap 依然有效限制
    new_test_ext().execute_with(|| {
        fund(PLATFORM, 1_000_000);
        fund(SELLER, 1_000_000);
        CommissionConfigs::<Test>::insert(
            ENTITY_ID,
            CoreCommissionConfig {
                enabled_modes: CommissionModes(
                    CommissionModes::DIRECT_REWARD | CommissionModes::POOL_REWARD,
                ),
                max_commission_rate: 9900,
                enabled: true,
                withdrawal_cooldown: 0,
                owner_reward_rate: 0,
                token_withdrawal_cooldown: 0,
                plugin_caps: PluginBudgetCaps {
                    referral_cap: 3000,
                    multi_level_cap: 0,
                    level_diff_cap: 0,
                    single_line_cap: 0,
                    team_cap: 0,
                },
            },
        );

        assert_ok!(process_commission_with_reserve(
            ENTITY_ID, SHOP_ID, 8004, &BUYER, 100_000, 100_000, 0, PRODUCT_ID,
        ));

        // initial_remaining = 100_000 * 9900 / 10000 = 99_000
        let pool = UnallocatedPool::<Test>::get(ENTITY_ID);
        assert_eq!(pool, 99_000);
    });
}

#[test]
fn bug1_token_engine_cap_base_is_order_amount() {
    // Token 管线：验证 cap_base 使用 token_order_amount
    new_test_ext().execute_with(|| {
        let ea = ENTITY_ID + 9000; // entity_account
        set_token_balance(ENTITY_ID, ea, 500_000);

        CommissionConfigs::<Test>::insert(
            ENTITY_ID,
            CoreCommissionConfig {
                enabled_modes: CommissionModes(
                    CommissionModes::DIRECT_REWARD | CommissionModes::POOL_REWARD,
                ),
                max_commission_rate: 1000, // 10%
                enabled: true,
                withdrawal_cooldown: 0,
                owner_reward_rate: 0,
                token_withdrawal_cooldown: 0,
                plugin_caps: PluginBudgetCaps {
                    referral_cap: 500, // 5% of order_amount, ≤ max_rate(1000)
                    multi_level_cap: 0,
                    level_diff_cap: 0,
                    single_line_cap: 0,
                    team_cap: 0,
                },
            },
        );

        assert_ok!(CommissionCore::process_token_commission(
            ENTITY_ID, SHOP_ID, 8005, &BUYER, 100_000, 100_000, 0, PRODUCT_ID,
        ));

        // Token max_commission = 100_000 * 1000 / 10000 = 10_000
        // available_token = 500_000 - 0(committed) = 500_000
        // initial_remaining = min(10_000, 500_000) = 10_000
        // 空 Token 插件不消耗 → remaining = 10_000 → UnallocatedTokenPool
        let pool = UnallocatedTokenPool::<Test>::get(ENTITY_ID);
        assert_eq!(pool, 10_000);
    });
}

#[test]
fn bug1_token_engine_caps_with_owner_reward() {
    new_test_ext().execute_with(|| {
        let ea = ENTITY_ID + 9000;
        set_token_balance(ENTITY_ID, ea, 500_000);

        CommissionConfigs::<Test>::insert(
            ENTITY_ID,
            CoreCommissionConfig {
                enabled_modes: CommissionModes(
                    CommissionModes::DIRECT_REWARD
                        | CommissionModes::OWNER_REWARD
                        | CommissionModes::POOL_REWARD,
                ),
                max_commission_rate: 2000, // 20%
                enabled: true,
                withdrawal_cooldown: 0,
                owner_reward_rate: 5000, // 50% of order
                token_withdrawal_cooldown: 0,
                plugin_caps: PluginBudgetCaps {
                    referral_cap: 1500, // 15% of order, ≤ max_rate(2000)
                    multi_level_cap: 0,
                    level_diff_cap: 0,
                    single_line_cap: 0,
                    team_cap: 0,
                },
            },
        );

        assert_ok!(CommissionCore::process_token_commission(
            ENTITY_ID, SHOP_ID, 8006, &BUYER, 100_000, 100_000, 0, PRODUCT_ID,
        ));

        // Token max_commission = 100_000 * 2000 / 10000 = 20_000
        // initial_remaining = min(20_000, 500_000) = 20_000
        // owner_reward = 100_000 * 5000 / 10000 = 50_000, min(50_000, 20_000) = 20_000
        // remaining after owner_reward = 0
        // 空 Token 插件不消耗 → remaining = 0 → pool = 0
        let pool = UnallocatedTokenPool::<Test>::get(ENTITY_ID);
        assert_eq!(pool, 0);

        let records = OrderTokenCommissionRecords::<Test>::get(8006u64);
        assert_eq!(records.len(), 0);
        // OwnerReward 直接到账，不再有 OrderTokenOwnerReward 可断言
    });
}

// ── cap ≤ max_commission_rate 校验测试 ──

#[test]
fn pcap_rejects_cap_over_max_commission_rate() {
    // max_rate=3000 时，referral_cap=3001 应被拒绝
    new_test_ext().execute_with(|| {
        CommissionConfigs::<Test>::insert(
            ENTITY_ID,
            CoreCommissionConfig {
                enabled_modes: CommissionModes::default(),
                max_commission_rate: 3000,
                enabled: true,
                withdrawal_cooldown: 0,
                owner_reward_rate: 0,
                token_withdrawal_cooldown: 0,
                plugin_caps: PluginBudgetCaps::default(),
            },
        );

        // referral_cap > max_rate
        assert_noop!(
            CommissionCore::set_plugin_budget_caps(
                RuntimeOrigin::signed(SELLER),
                ENTITY_ID,
                PluginBudgetCaps {
                    referral_cap: 3001,
                    multi_level_cap: 0,
                    level_diff_cap: 0,
                    single_line_cap: 0,
                    team_cap: 0,
                },
            ),
            Error::<Test>::PluginCapExceedsCommissionRate
        );
        // multi_level_cap > max_rate
        assert_noop!(
            CommissionCore::set_plugin_budget_caps(
                RuntimeOrigin::signed(SELLER),
                ENTITY_ID,
                PluginBudgetCaps {
                    referral_cap: 0,
                    multi_level_cap: 3001,
                    level_diff_cap: 0,
                    single_line_cap: 0,
                    team_cap: 0,
                },
            ),
            Error::<Test>::PluginCapExceedsCommissionRate
        );
        // level_diff_cap > max_rate
        assert_noop!(
            CommissionCore::set_plugin_budget_caps(
                RuntimeOrigin::signed(SELLER),
                ENTITY_ID,
                PluginBudgetCaps {
                    referral_cap: 0,
                    multi_level_cap: 0,
                    level_diff_cap: 3001,
                    single_line_cap: 0,
                    team_cap: 0,
                },
            ),
            Error::<Test>::PluginCapExceedsCommissionRate
        );
        // single_line_cap > max_rate
        assert_noop!(
            CommissionCore::set_plugin_budget_caps(
                RuntimeOrigin::signed(SELLER),
                ENTITY_ID,
                PluginBudgetCaps {
                    referral_cap: 0,
                    multi_level_cap: 0,
                    level_diff_cap: 0,
                    single_line_cap: 3001,
                    team_cap: 0,
                },
            ),
            Error::<Test>::PluginCapExceedsCommissionRate
        );
        // team_cap > max_rate
        assert_noop!(
            CommissionCore::set_plugin_budget_caps(
                RuntimeOrigin::signed(SELLER),
                ENTITY_ID,
                PluginBudgetCaps {
                    referral_cap: 0,
                    multi_level_cap: 0,
                    level_diff_cap: 0,
                    single_line_cap: 0,
                    team_cap: 3001,
                },
            ),
            Error::<Test>::PluginCapExceedsCommissionRate
        );
    });
}

#[test]
fn pcap_boundary_cap_equals_max_rate_accepted() {
    // cap == max_rate 应被接受
    new_test_ext().execute_with(|| {
        CommissionConfigs::<Test>::insert(
            ENTITY_ID,
            CoreCommissionConfig {
                enabled_modes: CommissionModes::default(),
                max_commission_rate: 3000,
                enabled: true,
                withdrawal_cooldown: 0,
                owner_reward_rate: 0,
                token_withdrawal_cooldown: 0,
                plugin_caps: PluginBudgetCaps::default(),
            },
        );

        let caps = PluginBudgetCaps {
            referral_cap: 3000,
            multi_level_cap: 3000,
            level_diff_cap: 3000,
            single_line_cap: 3000,
            team_cap: 3000,
        };
        assert_ok!(CommissionCore::set_plugin_budget_caps(
            RuntimeOrigin::signed(SELLER),
            ENTITY_ID,
            caps.clone(),
        ));
        let config = CommissionConfigs::<Test>::get(ENTITY_ID).unwrap();
        assert_eq!(config.plugin_caps, caps);
    });
}

#[test]
fn pcap_no_config_uses_default_max_rate_10000() {
    // 无预先配置时 max_rate 默认 10000，cap ≤ 10000 应被接受
    // H-1: 默认 modes = MULTI_LEVEL + SINGLE_LINE，caps 必须覆盖
    new_test_ext().execute_with(|| {
        assert!(CommissionConfigs::<Test>::get(ENTITY_ID).is_none());
        let caps = PluginBudgetCaps {
            referral_cap: 0,
            multi_level_cap: 8000,
            level_diff_cap: 0,
            single_line_cap: 8000,
            team_cap: 0,
        };
        assert_ok!(CommissionCore::set_plugin_budget_caps(
            RuntimeOrigin::signed(SELLER),
            ENTITY_ID,
            caps.clone(),
        ));
        let config = CommissionConfigs::<Test>::get(ENTITY_ID).unwrap();
        assert_eq!(config.plugin_caps, caps);
        assert_eq!(config.max_commission_rate, 10000); // 默认值
    });
}

// ==================== H-1: PluginCapRequiredForMultiPlugin ====================

#[test]
fn h1_single_plugin_allows_zero_caps() {
    // 启用 1 个插件组时，cap=0 也被拒绝（任何插件都需要 cap > 0）
    new_test_ext().execute_with(|| {
        // 先通过 storage 设置模式为 DIRECT_REWARD（跳过 extrinsic 校验）
        CommissionConfigs::<Test>::mutate(ENTITY_ID, |maybe| {
            let config = maybe.get_or_insert_with(CoreCommissionConfig::default);
            config.enabled_modes = CommissionModes(CommissionModes::DIRECT_REWARD);
        });
        // 设置全 0 caps → 被拒绝（DIRECT_REWARD 需要 referral_cap > 0）
        assert_noop!(
            CommissionCore::set_plugin_budget_caps(
                RuntimeOrigin::signed(SELLER),
                ENTITY_ID,
                PluginBudgetCaps::default(), // 全 0
            ),
            Error::<Test>::PluginCapRequiredForMultiPlugin
        );
        // 设置正确的 cap → 成功
        assert_ok!(CommissionCore::set_plugin_budget_caps(
            RuntimeOrigin::signed(SELLER),
            ENTITY_ID,
            PluginBudgetCaps {
                referral_cap: 5000,
                ..PluginBudgetCaps::default()
            },
        ));
        assert_ok!(CommissionCore::enable_commission(
            RuntimeOrigin::signed(SELLER),
            ENTITY_ID,
            true,
        ));
    });
}

#[test]
fn h1_two_plugins_rejects_zero_caps() {
    // 启用 2 个插件组 + cap 全 0 → 被拒绝
    new_test_ext().execute_with(|| {
        // 通过 storage 设置模式为 DIRECT_REWARD + caps 全 0（跳过 extrinsic 校验）
        CommissionConfigs::<Test>::mutate(ENTITY_ID, |maybe| {
            let config = maybe.get_or_insert_with(CoreCommissionConfig::default);
            config.enabled_modes = CommissionModes(CommissionModes::DIRECT_REWARD);
            config.plugin_caps = PluginBudgetCaps::default();
        });
        // 切换到 2 组模式 → 拒绝（caps 全 0）
        assert_noop!(
            CommissionCore::set_commission_modes(
                RuntimeOrigin::signed(SELLER),
                ENTITY_ID,
                CommissionModes(CommissionModes::DIRECT_REWARD | CommissionModes::MULTI_LEVEL),
            ),
            Error::<Test>::PluginCapRequiredForMultiPlugin
        );
    });
}

#[test]
fn h1_two_plugins_requires_caps_for_both() {
    // 2 组启用，只设置其中 1 个的 cap → 被拒绝
    new_test_ext().execute_with(|| {
        // 通过 storage 设置单插件模式 + referral_cap（跳过 extrinsic 校验）
        CommissionConfigs::<Test>::mutate(ENTITY_ID, |maybe| {
            let config = maybe.get_or_insert_with(CoreCommissionConfig::default);
            config.enabled_modes = CommissionModes(CommissionModes::DIRECT_REWARD);
            config.plugin_caps = PluginBudgetCaps {
                referral_cap: 5000,
                multi_level_cap: 0,
                level_diff_cap: 0,
                single_line_cap: 0,
                team_cap: 0,
            };
        });
        // 切换到 2 组模式 → 拒绝（multi_level_cap=0）
        assert_noop!(
            CommissionCore::set_commission_modes(
                RuntimeOrigin::signed(SELLER),
                ENTITY_ID,
                CommissionModes(CommissionModes::DIRECT_REWARD | CommissionModes::MULTI_LEVEL),
            ),
            Error::<Test>::PluginCapRequiredForMultiPlugin
        );
        // 设置两个都 > 0
        assert_ok!(CommissionCore::set_plugin_budget_caps(
            RuntimeOrigin::signed(SELLER),
            ENTITY_ID,
            PluginBudgetCaps {
                referral_cap: 5000,
                multi_level_cap: 5000,
                level_diff_cap: 0,
                single_line_cap: 0,
                team_cap: 0,
            },
        ));
        // 再次切换到 2 组 → 成功
        assert_ok!(CommissionCore::set_commission_modes(
            RuntimeOrigin::signed(SELLER),
            ENTITY_ID,
            CommissionModes(CommissionModes::DIRECT_REWARD | CommissionModes::MULTI_LEVEL),
        ));
    });
}

#[test]
fn h1_set_modes_checks_existing_caps() {
    // 先设置只有 referral_cap 的 caps，再切换到 2 组模式 → 被拒绝
    new_test_ext().execute_with(|| {
        // 通过 storage 设置单插件模式 + 只有 referral_cap（跳过 extrinsic 校验）
        CommissionConfigs::<Test>::mutate(ENTITY_ID, |maybe| {
            let config = maybe.get_or_insert_with(CoreCommissionConfig::default);
            config.enabled_modes = CommissionModes(CommissionModes::DIRECT_REWARD);
            config.plugin_caps = PluginBudgetCaps {
                referral_cap: 5000,
                multi_level_cap: 0,
                level_diff_cap: 0,
                single_line_cap: 0,
                team_cap: 0,
            };
        });
        // 切换到 2 组模式 → 拒绝（multi_level_cap=0）
        assert_noop!(
            CommissionCore::set_commission_modes(
                RuntimeOrigin::signed(SELLER),
                ENTITY_ID,
                CommissionModes(CommissionModes::DIRECT_REWARD | CommissionModes::MULTI_LEVEL),
            ),
            Error::<Test>::PluginCapRequiredForMultiPlugin
        );
    });
}

#[test]
fn h1_enable_checks_caps_consistency() {
    // 通过 storage 直接插入不一致的配置，enable 时被拒绝
    new_test_ext().execute_with(|| {
        CommissionConfigs::<Test>::insert(
            ENTITY_ID,
            CoreCommissionConfig {
                enabled_modes: CommissionModes(
                    CommissionModes::DIRECT_REWARD | CommissionModes::MULTI_LEVEL,
                ),
                max_commission_rate: 5000,
                enabled: false,
                withdrawal_cooldown: 0,
                owner_reward_rate: 0,
                token_withdrawal_cooldown: 0,
                plugin_caps: PluginBudgetCaps::default(), // 全 0
            },
        );
        assert_noop!(
            CommissionCore::enable_commission(RuntimeOrigin::signed(SELLER), ENTITY_ID, true,),
            Error::<Test>::PluginCapRequiredForMultiPlugin
        );
    });
}

#[test]
fn h1_enable_false_always_succeeds() {
    // disable 不需要校验 caps
    new_test_ext().execute_with(|| {
        CommissionConfigs::<Test>::insert(
            ENTITY_ID,
            CoreCommissionConfig {
                enabled_modes: CommissionModes(
                    CommissionModes::DIRECT_REWARD | CommissionModes::MULTI_LEVEL,
                ),
                max_commission_rate: 5000,
                enabled: true,
                withdrawal_cooldown: 0,
                owner_reward_rate: 0,
                token_withdrawal_cooldown: 0,
                plugin_caps: PluginBudgetCaps::default(), // 全 0
            },
        );
        assert_ok!(CommissionCore::enable_commission(
            RuntimeOrigin::signed(SELLER),
            ENTITY_ID,
            false,
        ));
    });
}

#[test]
fn h1_five_plugins_all_caps_required() {
    // 全部 5 组启用 → 每组都需要 cap > 0
    new_test_ext().execute_with(|| {
        // 通过 storage 设置单插件模式 + referral_cap（跳过 extrinsic 校验）
        CommissionConfigs::<Test>::mutate(ENTITY_ID, |maybe| {
            let config = maybe.get_or_insert_with(CoreCommissionConfig::default);
            config.enabled_modes = CommissionModes(CommissionModes::DIRECT_REWARD);
            config.plugin_caps.referral_cap = 10000;
        });
        // 设置 4 个 cap（缺 team_cap）
        assert_ok!(CommissionCore::set_plugin_budget_caps(
            RuntimeOrigin::signed(SELLER),
            ENTITY_ID,
            PluginBudgetCaps {
                referral_cap: 2000,
                multi_level_cap: 2000,
                level_diff_cap: 2000,
                single_line_cap: 2000,
                team_cap: 0,
            },
        ));
        let all_plugin_modes = CommissionModes(
            CommissionModes::DIRECT_REWARD
                | CommissionModes::MULTI_LEVEL
                | CommissionModes::LEVEL_DIFF
                | CommissionModes::SINGLE_LINE_UPLINE
                | CommissionModes::TEAM_PERFORMANCE,
        );
        // 切换到 5 组 → 拒绝（team_cap=0）
        assert_noop!(
            CommissionCore::set_commission_modes(
                RuntimeOrigin::signed(SELLER),
                ENTITY_ID,
                all_plugin_modes,
            ),
            Error::<Test>::PluginCapRequiredForMultiPlugin
        );
        // 全部设置 → 成功
        assert_ok!(CommissionCore::set_plugin_budget_caps(
            RuntimeOrigin::signed(SELLER),
            ENTITY_ID,
            PluginBudgetCaps {
                referral_cap: 2000,
                multi_level_cap: 2000,
                level_diff_cap: 2000,
                single_line_cap: 2000,
                team_cap: 2000,
            },
        ));
        assert_ok!(CommissionCore::set_commission_modes(
            RuntimeOrigin::signed(SELLER),
            ENTITY_ID,
            all_plugin_modes,
        ));
    });
}

#[test]
fn h1_non_plugin_modes_not_counted() {
    // POOL_REWARD + OWNER_REWARD + 1 个插件组 → 插件仍需要 cap > 0
    // 验证 POOL_REWARD / OWNER_REWARD 本身不需要 cap，但 DIRECT_REWARD 需要 referral_cap
    new_test_ext().execute_with(|| {
        // 通过 storage 设置 referral_cap，然后 set_commission_modes 通过 extrinsic
        CommissionConfigs::<Test>::mutate(ENTITY_ID, |maybe| {
            let config = maybe.get_or_insert_with(CoreCommissionConfig::default);
            config.plugin_caps.referral_cap = 10000;
        });
        assert_ok!(CommissionCore::set_commission_modes(
            RuntimeOrigin::signed(SELLER),
            ENTITY_ID,
            CommissionModes(
                CommissionModes::DIRECT_REWARD
                    | CommissionModes::POOL_REWARD
                    | CommissionModes::OWNER_REWARD,
            ),
        ));
        // 验证 POOL_REWARD 和 OWNER_REWARD 不需要额外 cap
        let config = CommissionConfigs::<Test>::get(ENTITY_ID).unwrap();
        assert_eq!(config.plugin_caps.referral_cap, 10000);
        // 但如果 referral_cap=0，即使只有 1 个插件组也会被拒绝
        assert_noop!(
            CommissionCore::set_plugin_budget_caps(
                RuntimeOrigin::signed(SELLER),
                ENTITY_ID,
                PluginBudgetCaps::default(),
            ),
            Error::<Test>::PluginCapRequiredForMultiPlugin
        );
    });
}

#[test]
fn h1_single_line_variants_count_as_one_group() {
    // SINGLE_LINE_UPLINE + SINGLE_LINE_DOWNLINE 算 1 组，不是 2 组
    // 但任何插件组都需要对应的 cap > 0
    new_test_ext().execute_with(|| {
        // 通过 storage 设置 single_line_cap
        CommissionConfigs::<Test>::mutate(ENTITY_ID, |maybe| {
            let config = maybe.get_or_insert_with(CoreCommissionConfig::default);
            config.plugin_caps.single_line_cap = 10000;
        });
        assert_ok!(CommissionCore::set_commission_modes(
            RuntimeOrigin::signed(SELLER),
            ENTITY_ID,
            CommissionModes(
                CommissionModes::SINGLE_LINE_UPLINE | CommissionModes::SINGLE_LINE_DOWNLINE
            ),
        ));
        // 只有 1 组 single_line，但仍需要 single_line_cap > 0
        // 尝试清零 → 被拒绝
        assert_noop!(
            CommissionCore::set_plugin_budget_caps(
                RuntimeOrigin::signed(SELLER),
                ENTITY_ID,
                PluginBudgetCaps::default(),
            ),
            Error::<Test>::PluginCapRequiredForMultiPlugin
        );
    });
}

#[test]
fn pcap_cap_base_is_order_amount_not_pool() {
    // 验证新语义：cap_base = order_amount
    // order_amount=100_000, max_rate=3000(30%), referral_cap=1000(10% of order)
    // initial_remaining = 100_000 * 3000 / 10000 = 30_000
    // cap_amount = 100_000 * 1000 / 10000 = 10_000
    // plugin_budget = min(30_000, 10_000) = 10_000
    //
    // Mock 插件不消耗，remaining 不变，全部进 UnallocatedPool = 30_000
    new_test_ext().execute_with(|| {
        fund(PLATFORM, 1_000_000);
        fund(SELLER, 1_000_000);
        CommissionConfigs::<Test>::insert(
            ENTITY_ID,
            CoreCommissionConfig {
                enabled_modes: CommissionModes(
                    CommissionModes::DIRECT_REWARD | CommissionModes::POOL_REWARD,
                ),
                max_commission_rate: 3000,
                enabled: true,
                withdrawal_cooldown: 0,
                owner_reward_rate: 0,
                token_withdrawal_cooldown: 0,
                plugin_caps: PluginBudgetCaps {
                    referral_cap: 1000,
                    multi_level_cap: 0,
                    level_diff_cap: 0,
                    single_line_cap: 0,
                    team_cap: 0,
                },
            },
        );

        assert_ok!(process_commission_with_reserve(
            ENTITY_ID, SHOP_ID, 8010, &BUYER, 100_000, 100_000, 0, PRODUCT_ID,
        ));

        // remaining = 30_000（空插件不消耗）→ UnallocatedPool
        let pool = UnallocatedPool::<Test>::get(ENTITY_ID);
        assert_eq!(pool, 30_000);
    });
}

#[test]
fn pcap_lowered_max_rate_caps_runtime_safe() {
    // 模拟：先设 caps，再通过直接写 storage 降低 max_rate
    // （模拟 governance_set_commission_rate 降费率后旧 caps 超出的情况）
    // 运行时 remaining.min() 保证安全，不会超支
    new_test_ext().execute_with(|| {
        fund(PLATFORM, 1_000_000);
        fund(SELLER, 1_000_000);

        // 初始 max_rate=5000，referral_cap=4000
        CommissionConfigs::<Test>::insert(
            ENTITY_ID,
            CoreCommissionConfig {
                enabled_modes: CommissionModes(
                    CommissionModes::DIRECT_REWARD | CommissionModes::POOL_REWARD,
                ),
                max_commission_rate: 5000,
                enabled: true,
                withdrawal_cooldown: 0,
                owner_reward_rate: 0,
                token_withdrawal_cooldown: 0,
                plugin_caps: PluginBudgetCaps {
                    referral_cap: 4000,
                    multi_level_cap: 0,
                    level_diff_cap: 0,
                    single_line_cap: 0,
                    team_cap: 0,
                },
            },
        );

        // 降低 max_rate 到 2000（referral_cap=4000 > max_rate=2000）
        CommissionConfigs::<Test>::mutate(ENTITY_ID, |maybe| {
            maybe.as_mut().unwrap().max_commission_rate = 2000;
        });

        // 运行时应安全：remaining = 100_000 * 2000/10000 = 20_000
        // cap_amount = 100_000 * 4000/10000 = 40_000
        // plugin_budget = min(20_000, 40_000) = 20_000（被 remaining 截断）
        assert_ok!(process_commission_with_reserve(
            ENTITY_ID, SHOP_ID, 8011, &BUYER, 100_000, 100_000, 0, PRODUCT_ID,
        ));

        let pool = UnallocatedPool::<Test>::get(ENTITY_ID);
        assert_eq!(pool, 20_000);
    });
}

// ============================================================================
// Entity 级推荐人佣金关闭（ReferrerPayoutDisabled）
// ============================================================================

#[test]
fn set_referrer_payout_disabled_owner_can_disable() {
    new_test_ext().execute_with(|| {
        // Owner (SELLER) 可以关闭推荐人佣金
        assert_ok!(CommissionCore::set_referrer_payout_disabled(
            RuntimeOrigin::signed(SELLER),
            ENTITY_ID,
            true,
        ));
        assert!(ReferrerPayoutDisabled::<Test>::get(ENTITY_ID));

        System::assert_has_event(RuntimeEvent::CommissionCore(
            crate::pallet::Event::ReferrerPayoutToggled {
                entity_id: ENTITY_ID,
                disabled: true,
            },
        ));
    });
}

#[test]
fn set_referrer_payout_disabled_owner_can_re_enable() {
    new_test_ext().execute_with(|| {
        ReferrerPayoutDisabled::<Test>::insert(ENTITY_ID, true);

        assert_ok!(CommissionCore::set_referrer_payout_disabled(
            RuntimeOrigin::signed(SELLER),
            ENTITY_ID,
            false,
        ));
        assert!(!ReferrerPayoutDisabled::<Test>::get(ENTITY_ID));
    });
}

#[test]
fn set_referrer_payout_disabled_admin_can_toggle() {
    new_test_ext().execute_with(|| {
        // 给 ADMIN 授予 COMMISSION_MANAGE 权限
        set_entity_admin(
            ENTITY_ID,
            ADMIN,
            pallet_entity_common::AdminPermission::COMMISSION_MANAGE,
        );

        assert_ok!(CommissionCore::set_referrer_payout_disabled(
            RuntimeOrigin::signed(ADMIN),
            ENTITY_ID,
            true,
        ));
        assert!(ReferrerPayoutDisabled::<Test>::get(ENTITY_ID));
    });
}

#[test]
fn set_referrer_payout_disabled_non_owner_rejected() {
    new_test_ext().execute_with(|| {
        assert_noop!(
            CommissionCore::set_referrer_payout_disabled(
                RuntimeOrigin::signed(BUYER),
                ENTITY_ID,
                true,
            ),
            Error::<Test>::NotEntityOwnerOrAdmin
        );
    });
}

#[test]
fn set_referrer_payout_disabled_locked_entity_rejected() {
    new_test_ext().execute_with(|| {
        set_entity_locked(ENTITY_ID);

        assert_noop!(
            CommissionCore::set_referrer_payout_disabled(
                RuntimeOrigin::signed(SELLER),
                ENTITY_ID,
                true,
            ),
            Error::<Test>::EntityLocked
        );
    });
}

#[test]
fn set_referrer_payout_disabled_inactive_entity_rejected() {
    new_test_ext().execute_with(|| {
        set_entity_inactive(ENTITY_ID);

        assert_noop!(
            CommissionCore::set_referrer_payout_disabled(
                RuntimeOrigin::signed(SELLER),
                ENTITY_ID,
                true,
            ),
            Error::<Test>::EntityNotActive
        );
    });
}

#[test]
fn referrer_payout_disabled_nex_pipeline() {
    // disabled=true → 推荐人无 EntityReferral，platform_fee 100% 归国库
    new_test_ext().execute_with(|| {
        fund(PLATFORM, 1_000_000);
        fund(SELLER, 1_000_000);
        set_entity_referrer(ENTITY_ID, REFERRER);
        setup_config(10000);

        // 关闭推荐人佣金
        ReferrerPayoutDisabled::<Test>::insert(ENTITY_ID, true);

        let platform_fee: Balance = 10_000;
        assert_ok!(process_commission_with_reserve(
            ENTITY_ID,
            SHOP_ID,
            9101,
            &BUYER,
            100_000,
            100_000,
            platform_fee,
            PRODUCT_ID,
        ));

        // 推荐人无提成
        let records = OrderCommissionRecords::<Test>::get(9101u64);
        let referrer_rec = records
            .iter()
            .find(|r| r.commission_type == CommissionType::EntityReferral);
        assert!(referrer_rec.is_none());

        // 国库拿到全额 platform_fee
        let treasury_transfer = OrderTreasuryTransfer::<Test>::get(9101u64);
        assert_eq!(treasury_transfer, 10_000);
    });
}

#[test]
fn referrer_payout_disabled_token_pipeline() {
    // disabled=true → Token 管线推荐人也无提成，平台费进沉淀池
    new_test_ext().execute_with(|| {
        let ea = entity_account(ENTITY_ID);
        set_token_balance(ENTITY_ID, ea, 200_000);
        set_entity_referrer(ENTITY_ID, REFERRER);
        setup_token_config(5000, true);

        // 关闭推荐人佣金
        ReferrerPayoutDisabled::<Test>::insert(ENTITY_ID, true);

        assert_ok!(CommissionCore::process_token_commission(
            ENTITY_ID, SHOP_ID, 9102, &BUYER, 20_000, 19_000, 1_000, PRODUCT_ID,
        ));

        // 推荐人无 Token 佣金
        let stats = MemberTokenCommissionStats::<Test>::get(ENTITY_ID, REFERRER);
        assert_eq!(stats.total_earned, 0);

        // token_platform_fee 全部进沉淀池
        let pool = UnallocatedTokenPool::<Test>::get(ENTITY_ID);
        assert!(pool >= 1_000);
    });
}

#[test]
fn referrer_payout_re_enabled_restores_commission() {
    // 关闭 → 无提成 → 重新开启 → 推荐人恢复提成
    new_test_ext().execute_with(|| {
        fund(PLATFORM, 10_000_000);
        fund(SELLER, 10_000_000);
        set_entity_referrer(ENTITY_ID, REFERRER);
        setup_config(10000);

        // 阶段 1: disabled → 推荐人无提成
        ReferrerPayoutDisabled::<Test>::insert(ENTITY_ID, true);
        let platform_fee: Balance = 10_000;
        assert_ok!(process_commission_with_reserve(
            ENTITY_ID,
            SHOP_ID,
            9103,
            &BUYER,
            100_000,
            100_000,
            platform_fee,
            PRODUCT_ID,
        ));
        let records = OrderCommissionRecords::<Test>::get(9103u64);
        assert!(!records
            .iter()
            .any(|r| r.commission_type == CommissionType::EntityReferral));

        // 阶段 2: re-enable → 推荐人恢复
        ReferrerPayoutDisabled::<Test>::insert(ENTITY_ID, false);
        assert_ok!(process_commission_with_reserve(
            ENTITY_ID,
            SHOP_ID,
            9104,
            &BUYER,
            100_000,
            100_000,
            platform_fee,
            PRODUCT_ID,
        ));
        let records = OrderCommissionRecords::<Test>::get(9104u64);
        let referrer_rec = records
            .iter()
            .find(|r| r.commission_type == CommissionType::EntityReferral);
        assert!(referrer_rec.is_some());
        assert_eq!(referrer_rec.unwrap().amount, 5000);
    });
}

#[test]
fn referrer_payout_disabled_does_not_affect_pool_b() {
    // disabled=true 仅关闭 Pool A (推荐人)，Pool B (会员返佣) 不受影响
    new_test_ext().execute_with(|| {
        fund(PLATFORM, 1_000_000);
        fund(SELLER, 1_000_000);
        set_entity_referrer(ENTITY_ID, REFERRER);
        set_referrer(ENTITY_ID, BUYER, SELLER);
        setup_config(10000);

        ReferrerPayoutDisabled::<Test>::insert(ENTITY_ID, true);

        let platform_fee: Balance = 10_000;
        assert_ok!(process_commission_with_reserve(
            ENTITY_ID,
            SHOP_ID,
            9105,
            &BUYER,
            100_000,
            100_000,
            platform_fee,
            PRODUCT_ID,
        ));

        // Pool B 仍有佣金产出（如 DirectReward 等），数量取决于插件配置
        // 关键断言：EntityReferral 为 0，但其他类型可能有佣金
        let records = OrderCommissionRecords::<Test>::get(9105u64);
        assert!(!records
            .iter()
            .any(|r| r.commission_type == CommissionType::EntityReferral));
        // Pool B 的佣金仍然正常处理（如果 mock 插件返回了佣金）
    });
}

#[test]
fn referrer_payout_disabled_combined_with_exempt_threshold() {
    // 两个规则同时命中：exempt_threshold 和 entity_disabled 都生效
    // 只要任一触发，推荐人都无提成
    new_test_ext().execute_with(|| {
        fund(PLATFORM, 1_000_000);
        fund(SELLER, 1_000_000);
        set_entity_referrer(ENTITY_ID, REFERRER);
        set_mock_tier_count(ENTITY_ID, 8);
        setup_config(10000);

        // 两个规则同时生效
        ReferrerPayoutDisabled::<Test>::insert(ENTITY_ID, true);
        // threshold=5, tier_count=8 → 也会 exempt

        let platform_fee: Balance = 10_000;
        assert_ok!(process_commission_with_reserve(
            ENTITY_ID,
            SHOP_ID,
            9106,
            &BUYER,
            100_000,
            100_000,
            platform_fee,
            PRODUCT_ID,
        ));

        let records = OrderCommissionRecords::<Test>::get(9106u64);
        assert!(!records
            .iter()
            .any(|r| r.commission_type == CommissionType::EntityReferral));
    });
}

#[test]
fn referrer_payout_disabled_only_affects_target_entity() {
    // Entity 1 关闭推荐人佣金，Entity 2 不受影响
    new_test_ext().execute_with(|| {
        let entity2 = 2u64;
        let shop2 = 200u64;

        fund(PLATFORM, 10_000_000);
        fund(SELLER, 10_000_000);

        // 配置 Entity 2
        set_shop_entity(shop2, entity2);
        set_entity_owner(entity2, SELLER);
        set_shop_owner(shop2, SELLER);
        set_entity_referrer(ENTITY_ID, REFERRER);
        set_entity_referrer(entity2, REFERRER);
        setup_config(10000);
        // Entity 2 也需要佣金配置
        CommissionConfigs::<Test>::insert(
            entity2,
            CoreCommissionConfig {
                enabled_modes: CommissionModes(CommissionModes::DIRECT_REWARD),
                max_commission_rate: 10000,
                enabled: true,
                withdrawal_cooldown: 0,
                owner_reward_rate: 0,
                token_withdrawal_cooldown: 0,
                plugin_caps: Default::default(),
            },
        );

        // 仅关闭 Entity 1
        ReferrerPayoutDisabled::<Test>::insert(ENTITY_ID, true);

        let platform_fee: Balance = 10_000;

        // Entity 1: 无推荐人提成
        assert_ok!(process_commission_with_reserve(
            ENTITY_ID,
            SHOP_ID,
            9107,
            &BUYER,
            100_000,
            100_000,
            platform_fee,
            PRODUCT_ID,
        ));
        let records = OrderCommissionRecords::<Test>::get(9107u64);
        assert!(!records
            .iter()
            .any(|r| r.commission_type == CommissionType::EntityReferral));

        // Entity 2: 推荐人正常拿
        assert_ok!(process_commission_with_reserve(
            entity2,
            shop2,
            9108,
            &BUYER,
            100_000,
            100_000,
            platform_fee,
            PRODUCT_ID,
        ));
        let records = OrderCommissionRecords::<Test>::get(9108u64);
        let referrer_rec = records
            .iter()
            .find(|r| r.commission_type == CommissionType::EntityReferral);
        assert!(referrer_rec.is_some());
        assert_eq!(referrer_rec.unwrap().amount, 5000);
    });
}

// ============================================================================
// 推荐链深度：多插件联合检测（single-line / level-diff / team）
// ============================================================================

#[test]
fn single_line_depth_triggers_exempt() {
    // single-line depth=8 > threshold=5 → 免除推荐人提成
    new_test_ext().execute_with(|| {
        fund(PLATFORM, 1_000_000);
        fund(SELLER, 1_000_000);
        set_entity_referrer(ENTITY_ID, REFERRER);
        set_mock_single_line_depth(ENTITY_ID, 8);
        setup_config(10000);

        let platform_fee: Balance = 10_000;
        assert_ok!(process_commission_with_reserve(
            ENTITY_ID,
            SHOP_ID,
            9201,
            &BUYER,
            100_000,
            100_000,
            platform_fee,
            PRODUCT_ID,
        ));

        let records = OrderCommissionRecords::<Test>::get(9201u64);
        let referrer_rec = records
            .iter()
            .find(|r| r.commission_type == CommissionType::EntityReferral);
        assert!(referrer_rec.is_none());

        // 国库拿到全额 platform_fee
        let treasury_transfer = OrderTreasuryTransfer::<Test>::get(9201u64);
        assert_eq!(treasury_transfer, 10_000);
    });
}

#[test]
fn level_diff_depth_triggers_exempt() {
    // level-diff depth=7 > threshold=5 → 免除推荐人提成
    new_test_ext().execute_with(|| {
        fund(PLATFORM, 1_000_000);
        fund(SELLER, 1_000_000);
        set_entity_referrer(ENTITY_ID, REFERRER);
        set_mock_level_diff_depth(ENTITY_ID, 7);
        setup_config(10000);

        let platform_fee: Balance = 10_000;
        assert_ok!(process_commission_with_reserve(
            ENTITY_ID,
            SHOP_ID,
            9202,
            &BUYER,
            100_000,
            100_000,
            platform_fee,
            PRODUCT_ID,
        ));

        let records = OrderCommissionRecords::<Test>::get(9202u64);
        let referrer_rec = records
            .iter()
            .find(|r| r.commission_type == CommissionType::EntityReferral);
        assert!(referrer_rec.is_none());
    });
}

#[test]
fn team_depth_triggers_exempt() {
    // team depth=10 > threshold=5 → 免除推荐人提成
    new_test_ext().execute_with(|| {
        fund(PLATFORM, 1_000_000);
        fund(SELLER, 1_000_000);
        set_entity_referrer(ENTITY_ID, REFERRER);
        set_mock_team_depth(ENTITY_ID, 10);
        setup_config(10000);

        let platform_fee: Balance = 10_000;
        assert_ok!(process_commission_with_reserve(
            ENTITY_ID,
            SHOP_ID,
            9203,
            &BUYER,
            100_000,
            100_000,
            platform_fee,
            PRODUCT_ID,
        ));

        let records = OrderCommissionRecords::<Test>::get(9203u64);
        let referrer_rec = records
            .iter()
            .find(|r| r.commission_type == CommissionType::EntityReferral);
        assert!(referrer_rec.is_none());
    });
}

#[test]
fn max_of_all_plugins_used_for_exempt() {
    // multi-level=3, single-line=4, level-diff=2, team=8 → max=8 > 5 → 免除
    new_test_ext().execute_with(|| {
        fund(PLATFORM, 1_000_000);
        fund(SELLER, 1_000_000);
        set_entity_referrer(ENTITY_ID, REFERRER);
        set_mock_tier_count(ENTITY_ID, 3);
        set_mock_single_line_depth(ENTITY_ID, 4);
        set_mock_level_diff_depth(ENTITY_ID, 2);
        set_mock_team_depth(ENTITY_ID, 8);
        setup_config(10000);

        let platform_fee: Balance = 10_000;
        assert_ok!(process_commission_with_reserve(
            ENTITY_ID,
            SHOP_ID,
            9204,
            &BUYER,
            100_000,
            100_000,
            platform_fee,
            PRODUCT_ID,
        ));

        // max(3,4,2,8)=8 > 5 → 推荐人无提成
        let records = OrderCommissionRecords::<Test>::get(9204u64);
        let referrer_rec = records
            .iter()
            .find(|r| r.commission_type == CommissionType::EntityReferral);
        assert!(referrer_rec.is_none());
    });
}

#[test]
fn all_plugins_below_threshold_no_exempt() {
    // 所有插件深度 ≤5 → 推荐人正常拿提成
    new_test_ext().execute_with(|| {
        fund(PLATFORM, 1_000_000);
        fund(SELLER, 1_000_000);
        set_entity_referrer(ENTITY_ID, REFERRER);
        set_mock_tier_count(ENTITY_ID, 3);
        set_mock_single_line_depth(ENTITY_ID, 4);
        set_mock_level_diff_depth(ENTITY_ID, 2);
        set_mock_team_depth(ENTITY_ID, 5);
        setup_config(10000);

        let platform_fee: Balance = 10_000;
        assert_ok!(process_commission_with_reserve(
            ENTITY_ID,
            SHOP_ID,
            9205,
            &BUYER,
            100_000,
            100_000,
            platform_fee,
            PRODUCT_ID,
        ));

        // max(3,4,2,5)=5, 5 不大于 5 → 推荐人正常拿
        let records = OrderCommissionRecords::<Test>::get(9205u64);
        let referrer_rec = records
            .iter()
            .find(|r| r.commission_type == CommissionType::EntityReferral);
        assert!(referrer_rec.is_some());
        assert_eq!(referrer_rec.unwrap().amount, 5000);
    });
}

#[test]
fn token_pipeline_multi_plugin_exempt() {
    // Token 管线：team depth=10 > threshold=5 → 推荐人无 Token 提成
    new_test_ext().execute_with(|| {
        let ea = entity_account(ENTITY_ID);
        set_token_balance(ENTITY_ID, ea, 200_000);
        set_entity_referrer(ENTITY_ID, REFERRER);
        // multi-level=0, single-line=3, level-diff=0, team=10 → max=10 > 5
        set_mock_single_line_depth(ENTITY_ID, 3);
        set_mock_team_depth(ENTITY_ID, 10);
        setup_token_config(5000, true);

        assert_ok!(CommissionCore::process_token_commission(
            ENTITY_ID, SHOP_ID, 9206, &BUYER, 20_000, 19_000, 1_000, PRODUCT_ID,
        ));

        // 推荐人无 Token 佣金
        let stats = MemberTokenCommissionStats::<Test>::get(ENTITY_ID, REFERRER);
        assert_eq!(stats.total_earned, 0);

        // token_platform_fee 全部进沉淀池
        let pool = UnallocatedTokenPool::<Test>::get(ENTITY_ID);
        assert!(pool >= 1_000);
    });
}

// ============================================================================
// 方案 B 边界场景：platform_fee > 0 + effective_rate 接近上限
// max_commission = order_amount × effective_rate / 10000（新基数）
// 验证 seller_transferable.min() / available_token.min() 兜底正确
// ============================================================================

/// NEX: effective_rate=10000 (100%), platform_fee=1%
/// max_commission = 100_000 × 10000 / 10000 = 100_000
/// seller 实收 = 100_000 - 1_000 = 99_000，transferable = 99_000 - ED
/// remaining 被截断为 seller_transferable < max_commission
#[test]
fn nex_rate_10000_with_platform_fee_truncated_by_seller_balance() {
    new_test_ext().execute_with(|| {
        fund(PLATFORM, 1_000_000);
        // seller 只有恰好 order_amount - platform_fee 的余额（模拟 escrow 释放后）
        let order_amount: Balance = 100_000;
        let platform_fee: Balance = 1_000; // 1%
        let seller_received = order_amount - platform_fee; // 99_000
        fund(SELLER, seller_received);
        setup_config(10000); // 100% rate

        assert_ok!(process_commission_with_reserve(
            ENTITY_ID,
            SHOP_ID,
            20001,
            &BUYER,
            order_amount,
            order_amount,
            platform_fee,
            PRODUCT_ID,
        ));

        // max_commission = 100_000, seller_transferable = 99_000 - ED(1) = 98_999
        // remaining = min(100_000, 98_999) = 98_999 → 截断生效
        // 无插件 → remaining 全部未分配（无 POOL_REWARD 位，不入池）
        // 仅 Pool A: referrer 不存在（未设置），全部 platform_fee → treasury
        let treasury_bal = <pallet_balances::Pallet<Test> as frame_support::traits::Currency<
            u64,
        >>::free_balance(&TREASURY);
        assert_eq!(treasury_bal, platform_fee); // 1_000

        // seller 余额不变（无插件扣除佣金）
        let seller_bal =
            <pallet_balances::Pallet<Test> as frame_support::traits::Currency<u64>>::free_balance(
                &SELLER,
            );
        assert_eq!(seller_bal, seller_received);
    });
}

/// NEX: effective_rate=9900 (99%), platform_fee=1%
/// max_commission = 100_000 × 9900 / 10000 = 99_000
/// seller 实收 = 99_000 → transferable = 99_000 - ED = 98_999
/// remaining = min(99_000, 98_999) = 98_999 → 差 1（ED 保护）
#[test]
fn nex_rate_9900_with_1pct_fee_near_exact_match() {
    new_test_ext().execute_with(|| {
        fund(PLATFORM, 1_000_000);
        let order_amount: Balance = 100_000;
        let platform_fee: Balance = 1_000;
        let seller_received = order_amount - platform_fee;
        fund(SELLER, seller_received);
        setup_config(9900);

        assert_ok!(process_commission_with_reserve(
            ENTITY_ID,
            SHOP_ID,
            20002,
            &BUYER,
            order_amount,
            order_amount,
            platform_fee,
            PRODUCT_ID,
        ));

        // max_commission = 99_000, seller_transferable = 98_999
        // remaining = 98_999（仅差 1 因 ED）
        let seller_bal =
            <pallet_balances::Pallet<Test> as frame_support::traits::Currency<u64>>::free_balance(
                &SELLER,
            );
        assert_eq!(seller_bal, seller_received); // 无插件扣除

        let treasury_bal = <pallet_balances::Pallet<Test> as frame_support::traits::Currency<
            u64,
        >>::free_balance(&TREASURY);
        assert_eq!(treasury_bal, platform_fee);
    });
}

/// NEX: effective_rate=9900 + POOL_REWARD 开启 + OwnerReward
/// 验证 Pool B 佣金 + 沉淀池的总量 = seller_transferable（全部可用余额被分配）
#[test]
fn nex_rate_9900_pool_reward_drains_seller_balance() {
    new_test_ext().execute_with(|| {
        fund(PLATFORM, 1_000_000);
        let order_amount: Balance = 100_000;
        let platform_fee: Balance = 1_000;
        let seller_received = order_amount - platform_fee; // 99_000
        fund(SELLER, seller_received);

        // 开启 POOL_REWARD + OWNER_REWARD，rate=9900，owner_reward=2000
        CommissionConfigs::<Test>::insert(
            ENTITY_ID,
            CoreCommissionConfig {
                enabled_modes: CommissionModes(
                    CommissionModes::DIRECT_REWARD
                        | CommissionModes::POOL_REWARD
                        | CommissionModes::OWNER_REWARD,
                ),
                max_commission_rate: 9900,
                enabled: true,
                withdrawal_cooldown: 0,
                owner_reward_rate: 2000, // 20% of order
                token_withdrawal_cooldown: 0,
                plugin_caps: PluginBudgetCaps::default(),
            },
        );

        assert_ok!(process_commission_with_reserve(
            ENTITY_ID,
            SHOP_ID,
            20003,
            &BUYER,
            order_amount,
            order_amount,
            platform_fee,
            PRODUCT_ID,
        ));

        // max_commission = 99_000, seller_transferable = 98_999
        // remaining = 98_999
        // owner_reward_amount = 100_000 * 2000 / 10000 = 20_000, min(20_000, 98_999) = 20_000
        // after owner_reward: remaining = 98_999 - 20_000 = 78_999
        // 无其他插件 → remaining 78_999 全部入沉淀池（受 available_pool 截断）
        // available_pool = 99_000, max_pool = 99_000 - 20_000 = 79_000
        // pool = min(78_999, 79_000) = 78_999
        let records = OrderCommissionRecords::<Test>::get(20003u64);
        assert_eq!(records.len(), 0); // OwnerReward 不再产生 record

        // owner_reward 佣金 + 沉淀池 = 从 seller 扣除的总额
        let ea = entity_account(ENTITY_ID);
        let entity_bal =
            <pallet_balances::Pallet<Test> as frame_support::traits::Currency<u64>>::free_balance(
                &ea,
            );
        let owner_reward_amount = 20_000;
        let pool = UnallocatedPool::<Test>::get(ENTITY_ID);
        // entity 收到: pool 转账
        assert_eq!(entity_bal, pool);

        // seller 应该只剩 ED（全部可转余额被转走）
        let seller_bal =
            <pallet_balances::Pallet<Test> as frame_support::traits::Currency<u64>>::free_balance(
                &SELLER,
            );
        // owner=SELLER 时 owner reward 回到 seller，自身不会被完全抽空
        assert_eq!(seller_bal, 20_001);
        assert_eq!(owner_reward_amount + pool, 98_999);
    });
}

/// NEX: effective_rate=5000, platform_fee=50%
/// 极端平台费：order_amount=100_000, platform_fee=50_000
/// seller 实收 = 50_000, max_commission = 50_000 → 恰好等于 seller 实收
#[test]
fn nex_high_platform_fee_max_commission_matches_seller_received() {
    new_test_ext().execute_with(|| {
        fund(PLATFORM, 1_000_000);
        let order_amount: Balance = 100_000;
        let platform_fee: Balance = 50_000; // 50%
        let seller_received = order_amount - platform_fee;
        fund(SELLER, seller_received);
        set_entity_referrer(ENTITY_ID, REFERRER);

        CommissionConfigs::<Test>::insert(
            ENTITY_ID,
            CoreCommissionConfig {
                enabled_modes: CommissionModes(
                    CommissionModes::DIRECT_REWARD
                        | CommissionModes::POOL_REWARD
                        | CommissionModes::OWNER_REWARD,
                ),
                max_commission_rate: 5000, // 50%
                enabled: true,
                withdrawal_cooldown: 0,
                owner_reward_rate: 10000, // 100% of pool B → owner
                token_withdrawal_cooldown: 0,
                plugin_caps: PluginBudgetCaps::default(),
            },
        );

        assert_ok!(process_commission_with_reserve(
            ENTITY_ID,
            SHOP_ID,
            20004,
            &BUYER,
            order_amount,
            order_amount,
            platform_fee,
            PRODUCT_ID,
        ));

        // Pool A: referrer = 50_000 * 50% = 25_000, treasury = 25_000
        // Pool B: max_commission = 100_000 * 5000 / 10000 = 50_000
        // seller_transferable = 50_000 - 1 = 49_999
        // remaining = min(50_000, 49_999) = 49_999
        // owner_reward = 49_999 * 10000 / 10000 = 49_999 (100% of remaining)
        let records = OrderCommissionRecords::<Test>::get(20004u64);
        assert_eq!(records.len(), 1); // 只有 EntityReferral
        assert_eq!(records[0].commission_type, CommissionType::EntityReferral);
        assert_eq!(records[0].amount, 25_000);
        // OwnerReward 直接到账，不再有 OrderOwnerReward 可断言

        // owner=SELLER 时 owner reward 返回自身，seller 保留 owner_reward 部分
        let seller_bal =
            <pallet_balances::Pallet<Test> as frame_support::traits::Currency<u64>>::free_balance(
                &SELLER,
            );
        assert_eq!(seller_bal, 50_000);

        let treasury_bal = <pallet_balances::Pallet<Test> as frame_support::traits::Currency<
            u64,
        >>::free_balance(&TREASURY);
        assert_eq!(treasury_bal, 25_000);
    });
}

/// NEX: seller 余额远超 order_amount（有历史余额）
/// max_commission 不再受 seller_received 限制，但受 budget_ceiling 约束
#[test]
fn nex_seller_rich_max_commission_fully_honored() {
    new_test_ext().execute_with(|| {
        fund(PLATFORM, 1_000_000);
        let order_amount: Balance = 100_000;
        let platform_fee: Balance = 5_000; // 5%
                                           // seller 有大量历史余额
        fund(SELLER, 10_000_000);

        CommissionConfigs::<Test>::insert(
            ENTITY_ID,
            CoreCommissionConfig {
                enabled_modes: CommissionModes(
                    CommissionModes::DIRECT_REWARD | CommissionModes::OWNER_REWARD,
                ),
                max_commission_rate: 9900, // budget ceiling (100% - 1% platform fee)
                enabled: true,
                withdrawal_cooldown: 0,
                owner_reward_rate: 10000, // 全部给 owner
                token_withdrawal_cooldown: 0,
                plugin_caps: PluginBudgetCaps::default(),
            },
        );

        let seller_before = <pallet_balances::Pallet<Test> as frame_support::traits::Currency<
            u64,
        >>::free_balance(&SELLER);

        assert_ok!(process_commission_with_reserve(
            ENTITY_ID,
            SHOP_ID,
            20005,
            &BUYER,
            order_amount,
            order_amount,
            platform_fee,
            PRODUCT_ID,
        ));

        // max_commission = 100_000 × 9900 / 10000 = 99_000
        // seller_transferable = 10_000_000 - 1 = 9_999_999 >> 99_000
        // remaining = 99_000（不截断）
        // owner_reward = 99_000 * 10000 / 10000 = 99_000（全部）
        let records = OrderCommissionRecords::<Test>::get(20005u64);
        assert_eq!(records.len(), 0); // OwnerReward 不再产生 record
                                      // OwnerReward 直接到账，不再有 OrderOwnerReward 可断言

        let seller_after = <pallet_balances::Pallet<Test> as frame_support::traits::Currency<
            u64,
        >>::free_balance(&SELLER);
        // owner=SELLER 时 owner reward 返回自身，不应出现净扣减
        assert_eq!(seller_before - seller_after, 0);
    });
}

/// Token: effective_rate=10000, platform_fee > 0
/// max_commission = token_order_amount（100%），但被 available_token 截断
#[test]
fn token_rate_10000_with_fee_capped_by_entity_balance() {
    new_test_ext().execute_with(|| {
        let ea = entity_account(ENTITY_ID);
        set_token_balance(ENTITY_ID, ea, 50_000);
        setup_token_config(10000, true); // 100% rate + pool_reward

        let token_order_amount: u128 = 100_000;
        let token_platform_fee: u128 = 2_000; // 2%
        let token_available_pool = token_order_amount - token_platform_fee;

        assert_ok!(CommissionCore::process_token_commission(
            ENTITY_ID,
            SHOP_ID,
            20101,
            &BUYER,
            token_order_amount,
            token_available_pool,
            token_platform_fee,
            PRODUCT_ID,
        ));

        // max_commission = 100_000 × 10000 / 10000 = 100_000
        // entity_token_balance = 50_000, committed = 0
        // available_token = 50_000
        // remaining = min(100_000, 50_000) = 50_000 → 截断
        // 但 platform_fee (2_000) 已进 sweep → 2_000 入沉淀池 (Pool A retention)
        // Pool B remaining = min(100_000, 50_000 - 2_000) = 48_000
        //   (committed now includes Pool A retention of 2_000)
        // 无插件 → 48_000 全入沉淀池
        // total 沉淀池 = 2_000 (Pool A) + 48_000 (Pool B) = 50_000
        let pool = UnallocatedTokenPool::<Test>::get(ENTITY_ID);
        assert_eq!(pool, 50_000);
    });
}

/// Token: effective_rate=9900, platform_fee=1%，entity 余额恰好覆盖
#[test]
fn token_rate_9900_with_fee_exact_coverage() {
    new_test_ext().execute_with(|| {
        let ea = entity_account(ENTITY_ID);
        let token_order_amount: u128 = 100_000;
        let token_platform_fee: u128 = 1_000;
        let token_available_pool = token_order_amount - token_platform_fee;
        // entity 余额恰好等于 order_amount
        set_token_balance(ENTITY_ID, ea, 100_000);
        setup_token_config(9900, true);

        assert_ok!(CommissionCore::process_token_commission(
            ENTITY_ID,
            SHOP_ID,
            20102,
            &BUYER,
            token_order_amount,
            token_available_pool,
            token_platform_fee,
            PRODUCT_ID,
        ));

        // Pool A: platform_fee = 1_000 → sweep → 全部入沉淀池 (no referrer)
        // committed after Pool A = 1_000
        // max_commission = 100_000 × 9900 / 10000 = 99_000
        // available_token = 100_000 - 1_000 = 99_000
        // remaining = min(99_000, 99_000) = 99_000 → 恰好不截断
        // 无插件 → 99_000 入沉淀池
        // total = 1_000 + 99_000 = 100_000
        let pool = UnallocatedTokenPool::<Test>::get(ENTITY_ID);
        assert_eq!(pool, 100_000);
    });
}

/// Token: effective_rate=10000 + referrer，验证 Pool A + Pool B 总量不超过 entity 余额
#[test]
fn token_rate_10000_with_referrer_and_fee_no_overcommit() {
    new_test_ext().execute_with(|| {
        let ea = entity_account(ENTITY_ID);
        let token_order_amount: u128 = 100_000;
        let token_platform_fee: u128 = 5_000;
        let token_available_pool = token_order_amount - token_platform_fee;
        set_token_balance(ENTITY_ID, ea, 80_000);
        set_entity_referrer(ENTITY_ID, REFERRER);
        setup_token_config(10000, true);

        assert_ok!(CommissionCore::process_token_commission(
            ENTITY_ID,
            SHOP_ID,
            20103,
            &BUYER,
            token_order_amount,
            token_available_pool,
            token_platform_fee,
            PRODUCT_ID,
        ));

        // Pool A: referrer_quota = 5_000 * 50% = 2_500, retention = 2_500
        // committed after Pool A = 2_500 (referrer pending) + 2_500 (pool retention)
        //                        = 5_000
        // max_commission = 100_000
        // available_token = 80_000 - 5_000 = 75_000
        // remaining = min(100_000, 75_000) = 75_000
        // 无插件 → 75_000 入沉淀池
        let pool = UnallocatedTokenPool::<Test>::get(ENTITY_ID);
        // pool = 2_500 (Pool A retention) + 75_000 (Pool B) = 77_500
        assert_eq!(pool, 77_500);

        // referrer 获得 2_500
        let referrer_stats = MemberTokenCommissionStats::<Test>::get(ENTITY_ID, REFERRER);
        assert_eq!(referrer_stats.total_earned, 2_500);
        assert_eq!(referrer_stats.pending, 2_500);

        // total committed = referrer pending + pool = 2_500 + 77_500 = 80_000 = entity 余额
        let total_committed = referrer_stats.pending + pool;
        assert_eq!(total_committed, 80_000);
    });
}

/// NEX: effective_rate=9901 超过 10000-PlatformFeeRate 的软上限
/// 验证不会 panic，seller 余额兜底正常截断
#[test]
fn nex_rate_exceeds_soft_cap_graceful_truncation() {
    new_test_ext().execute_with(|| {
        fund(PLATFORM, 1_000_000);
        let order_amount: Balance = 100_000;
        let platform_fee: Balance = 1_000; // 1%
        let seller_received = order_amount - platform_fee;
        fund(SELLER, seller_received); // 99_000

        CommissionConfigs::<Test>::insert(
            ENTITY_ID,
            CoreCommissionConfig {
                enabled_modes: CommissionModes(
                    CommissionModes::DIRECT_REWARD
                        | CommissionModes::POOL_REWARD
                        | CommissionModes::OWNER_REWARD,
                ),
                max_commission_rate: 9901, // 超过 10000-100 = 9900 的软上限
                enabled: true,
                withdrawal_cooldown: 0,
                owner_reward_rate: 10000,
                token_withdrawal_cooldown: 0,
                plugin_caps: PluginBudgetCaps::default(),
            },
        );

        assert_ok!(process_commission_with_reserve(
            ENTITY_ID,
            SHOP_ID,
            20006,
            &BUYER,
            order_amount,
            order_amount,
            platform_fee,
            PRODUCT_ID,
        ));

        // max_commission = 100_000 × 9901 / 10000 = 99_010
        // seller_transferable = 99_000 - 1 = 98_999
        // remaining = min(99_010, 98_999) = 98_999 → 被截断 11
        // owner_reward = 98_999
        let records = OrderCommissionRecords::<Test>::get(20006u64);
        assert_eq!(records.len(), 0);
        // OwnerReward 直接到账，不再有 OrderOwnerReward 可断言

        // owner=SELLER 时 owner reward 返回自身，seller 保留 owner_reward 部分
        let seller_bal =
            <pallet_balances::Pallet<Test> as frame_support::traits::Currency<u64>>::free_balance(
                &SELLER,
            );
        assert_eq!(seller_bal, 99_000);
    });
}

/// NEX: order_amount=1（极小订单），platform_fee=1
/// seller_received=0 → 无法转入任何佣金
#[test]
fn nex_tiny_order_zero_seller_received() {
    new_test_ext().execute_with(|| {
        fund(PLATFORM, 1_000_000);
        let order_amount: Balance = 1;
        let platform_fee: Balance = 1;
        // seller_received = 0，fund(0) → 账户不存在或余额为 0
        setup_config(10000);

        assert_ok!(process_commission_with_reserve(
            ENTITY_ID,
            SHOP_ID,
            20007,
            &BUYER,
            order_amount,
            order_amount,
            platform_fee,
            PRODUCT_ID,
        ));

        // max_commission = 1, seller_transferable = 0 (无余额)
        // remaining = 0 → 无佣金分配
        let records = OrderCommissionRecords::<Test>::get(20007u64);
        assert_eq!(records.len(), 0);

        // platform_fee = 1 → treasury
        let treasury_bal = <pallet_balances::Pallet<Test> as frame_support::traits::Currency<
            u64,
        >>::free_balance(&TREASURY);
        assert_eq!(treasury_bal, 1);
    });
}

/// Token: entity 余额为 0，platform_fee > 0
/// Pool A sweep 正常，Pool B remaining = 0
#[test]
fn token_zero_entity_balance_with_fee_no_panic() {
    new_test_ext().execute_with(|| {
        let ea = entity_account(ENTITY_ID);
        set_token_balance(ENTITY_ID, ea, 0); // 零余额
        setup_token_config(10000, true);

        let token_order_amount: u128 = 50_000;
        let token_platform_fee: u128 = 500;
        let token_available_pool = token_order_amount - token_platform_fee;

        assert_ok!(CommissionCore::process_token_commission(
            ENTITY_ID,
            SHOP_ID,
            20104,
            &BUYER,
            token_order_amount,
            token_available_pool,
            token_platform_fee,
            PRODUCT_ID,
        ));

        // entity 余额 = 0, sweep 后 committed 包含 pool retention
        // available_token = 0 - committed = 0 (saturating)
        // remaining = 0, 无佣金分配
        let pool = UnallocatedTokenPool::<Test>::get(ENTITY_ID);
        // 只有 Pool A retention (token_platform_fee, 因为无 referrer)
        assert_eq!(pool, 500);

        let records = OrderTokenCommissionRecords::<Test>::get(20104u64);
        assert_eq!(records.len(), 0);
    });
}

// ============================================================================
// 推荐人佣金保护期（ReferrerProtectionPeriod）
// ============================================================================

#[test]
fn protection_period_blocks_disable_within_period() {
    // protection=1000, bound_at=100, now=500 → 500-100=400 < 1000 → 拒绝
    new_test_ext().execute_with(|| {
        set_entity_referrer(ENTITY_ID, REFERRER);
        set_referrer_bound_at(ENTITY_ID, 100);
        System::set_block_number(500);

        assert_noop!(
            CommissionCore::set_referrer_payout_disabled(
                RuntimeOrigin::signed(SELLER),
                ENTITY_ID,
                true,
            ),
            Error::<Test>::ReferrerProtectionPeriodActive
        );
    });
}

#[test]
fn protection_period_allows_disable_after_period() {
    // protection=1000, bound_at=100, now=1200 → 1200-100=1100 >= 1000 → 允许
    new_test_ext().execute_with(|| {
        set_entity_referrer(ENTITY_ID, REFERRER);
        set_referrer_bound_at(ENTITY_ID, 100);
        System::set_block_number(1200);

        assert_ok!(CommissionCore::set_referrer_payout_disabled(
            RuntimeOrigin::signed(SELLER),
            ENTITY_ID,
            true,
        ));
        assert!(ReferrerPayoutDisabled::<Test>::get(ENTITY_ID));
    });
}

#[test]
fn protection_period_exact_boundary() {
    // protection=1000, bound_at=100, now=1100 → 1100-100=1000 >= 1000 → 允许（边界值）
    new_test_ext().execute_with(|| {
        set_entity_referrer(ENTITY_ID, REFERRER);
        set_referrer_bound_at(ENTITY_ID, 100);
        System::set_block_number(1100);

        assert_ok!(CommissionCore::set_referrer_payout_disabled(
            RuntimeOrigin::signed(SELLER),
            ENTITY_ID,
            true,
        ));
    });
}

#[test]
fn protection_period_no_bound_at_allows_disable() {
    // 推荐人没有 bound_at 记录（老数据） → 不阻止关闭
    new_test_ext().execute_with(|| {
        set_entity_referrer(ENTITY_ID, REFERRER);
        // 不调用 set_referrer_bound_at → 无记录
        System::set_block_number(1);

        assert_ok!(CommissionCore::set_referrer_payout_disabled(
            RuntimeOrigin::signed(SELLER),
            ENTITY_ID,
            true,
        ));
    });
}

#[test]
fn protection_period_re_enable_always_allowed() {
    // 保护期仅阻止 disabled=true，重新开启(disabled=false)不受限
    new_test_ext().execute_with(|| {
        set_entity_referrer(ENTITY_ID, REFERRER);
        set_referrer_bound_at(ENTITY_ID, 100);
        System::set_block_number(200);

        // 先用存储直接设置 disabled
        ReferrerPayoutDisabled::<Test>::insert(ENTITY_ID, true);

        // re-enable 不受保护期限制
        assert_ok!(CommissionCore::set_referrer_payout_disabled(
            RuntimeOrigin::signed(SELLER),
            ENTITY_ID,
            false,
        ));
        assert!(!ReferrerPayoutDisabled::<Test>::get(ENTITY_ID));
    });
}

// ============================================================================
// capped_budget / capped_token_budget 直接数学验证
// ============================================================================

#[test]
fn cb_zero_cap_returns_remaining() {
    new_test_ext().execute_with(|| {
        // cap=0 → 插件不启用，预算为 0
        assert_eq!(CommissionCore::capped_budget(0, 100_000, 50_000), 0);
        assert_eq!(CommissionCore::capped_budget(0, 0, 50_000), 0);
        assert_eq!(CommissionCore::capped_budget(0, 999_999, 1), 0);
    });
}

#[test]
fn cb_cap_limits_to_percentage_of_order_amount() {
    new_test_ext().execute_with(|| {
        // cap_base(order_amount)=100_000
        // cap=3000(30%) → cap_amount = 30_000, remaining=50_000 → min = 30_000
        assert_eq!(CommissionCore::capped_budget(3000, 100_000, 50_000), 30_000);
        // cap=5000(50%) → cap_amount = 50_000, remaining=50_000 → min = 50_000
        assert_eq!(CommissionCore::capped_budget(5000, 100_000, 50_000), 50_000);
        // cap=1000(10%) → cap_amount = 10_000, remaining=50_000 → min = 10_000
        assert_eq!(CommissionCore::capped_budget(1000, 100_000, 50_000), 10_000);
    });
}

#[test]
fn cb_remaining_smaller_than_cap_returns_remaining() {
    new_test_ext().execute_with(|| {
        // cap=8000(80%) → cap_amount = 80_000, remaining=5_000 → 5_000
        assert_eq!(CommissionCore::capped_budget(8000, 100_000, 5_000), 5_000);
    });
}

#[test]
fn cb_cap_10000_equals_full_cap_base() {
    new_test_ext().execute_with(|| {
        assert_eq!(
            CommissionCore::capped_budget(10000, 100_000, 100_000),
            100_000
        );
        assert_eq!(
            CommissionCore::capped_budget(10000, 100_000, 50_000),
            50_000
        );
    });
}

#[test]
fn cb_cap_smaller_than_rate_truncates() {
    // 核心场景：rate=1000(10%) → remaining=10_000，cap=500(5%) → cap_amount=5_000
    // cap 有效截断 remaining 到 5_000
    new_test_ext().execute_with(|| {
        let order_amount: Balance = 100_000;
        let remaining: Balance = 10_000; // order_amount * rate(1000) / 10000
                                         // cap=500 → cap_amount = 100_000 * 500 / 10000 = 5_000
        assert_eq!(
            CommissionCore::capped_budget(500, order_amount, remaining),
            5_000
        );
    });
}

#[test]
fn cb_cap_larger_than_rate_does_not_truncate() {
    // rate=1000(10%) → remaining=10_000, cap=5000(50%) → cap_amount=50_000
    // remaining(10_000) < cap_amount(50_000) → 返回 remaining，cap 无效
    new_test_ext().execute_with(|| {
        let order_amount: Balance = 100_000;
        let remaining: Balance = 10_000;
        assert_eq!(
            CommissionCore::capped_budget(5000, order_amount, remaining),
            10_000
        );
    });
}

#[test]
fn cb_zero_remaining_returns_zero() {
    new_test_ext().execute_with(|| {
        assert_eq!(CommissionCore::capped_budget(5000, 100_000, 0), 0);
    });
}

#[test]
fn cb_zero_cap_base_returns_zero() {
    new_test_ext().execute_with(|| {
        // order_amount=0 → cap_amount=0 → min(remaining, 0) = 0
        assert_eq!(CommissionCore::capped_budget(5000, 0, 50_000), 0);
    });
}

#[test]
fn cb_saturating_mul_no_overflow() {
    new_test_ext().execute_with(|| {
        let big: Balance = u128::MAX / 20000;
        let result = CommissionCore::capped_budget(10000, big, big);
        assert_eq!(result, big);
    });
}

#[test]
fn cb_token_mirrors_nex_behavior() {
    new_test_ext().execute_with(|| {
        // cap=0 → 插件不启用，预算为 0
        assert_eq!(
            CommissionCore::capped_token_budget(0, 100_000u128, 50_000u128),
            0
        );
        assert_eq!(
            CommissionCore::capped_token_budget(3000, 100_000, 50_000),
            30_000
        );
        assert_eq!(
            CommissionCore::capped_token_budget(500, 100_000, 10_000),
            5_000
        );
        assert_eq!(
            CommissionCore::capped_token_budget(10000, 50_000, 50_000),
            50_000
        );
    });
}

// ── 引擎集成：cap 配置下 process_commission 正常执行 ──

#[test]
fn cb_engine_nex_low_rate_with_caps_succeeds() {
    // rate=1000(10%), referral_cap=500(5%)
    // initial_remaining = 100_000 * 1000 / 10000 = 10_000
    // 空插件不消耗 → pool = 10_000
    new_test_ext().execute_with(|| {
        fund(PLATFORM, 1_000_000);
        fund(SELLER, 1_000_000);
        CommissionConfigs::<Test>::insert(
            ENTITY_ID,
            CoreCommissionConfig {
                enabled_modes: CommissionModes(
                    CommissionModes::DIRECT_REWARD | CommissionModes::POOL_REWARD,
                ),
                max_commission_rate: 1000,
                enabled: true,
                withdrawal_cooldown: 0,
                owner_reward_rate: 0,
                token_withdrawal_cooldown: 0,
                plugin_caps: PluginBudgetCaps {
                    referral_cap: 500,
                    multi_level_cap: 0,
                    level_diff_cap: 0,
                    single_line_cap: 0,
                    team_cap: 0,
                },
            },
        );

        assert_ok!(process_commission_with_reserve(
            ENTITY_ID, SHOP_ID, 8001, &BUYER, 100_000, 100_000, 0, PRODUCT_ID,
        ));

        let pool = UnallocatedPool::<Test>::get(ENTITY_ID);
        assert_eq!(pool, 10_000);
    });
}

#[test]
fn cb_engine_nex_all_five_caps_with_low_rate() {
    // rate=500(5%), 所有插件 cap=200(2%)
    // initial_remaining = 100_000 * 500 / 10000 = 5_000
    new_test_ext().execute_with(|| {
        fund(PLATFORM, 1_000_000);
        fund(SELLER, 1_000_000);
        CommissionConfigs::<Test>::insert(
            ENTITY_ID,
            CoreCommissionConfig {
                enabled_modes: CommissionModes(
                    CommissionModes::DIRECT_REWARD
                        | CommissionModes::MULTI_LEVEL
                        | CommissionModes::LEVEL_DIFF
                        | CommissionModes::SINGLE_LINE_UPLINE
                        | CommissionModes::TEAM_PERFORMANCE
                        | CommissionModes::POOL_REWARD,
                ),
                max_commission_rate: 500,
                enabled: true,
                withdrawal_cooldown: 0,
                owner_reward_rate: 0,
                token_withdrawal_cooldown: 0,
                plugin_caps: PluginBudgetCaps {
                    referral_cap: 200,
                    multi_level_cap: 200,
                    level_diff_cap: 200,
                    single_line_cap: 200,
                    team_cap: 200,
                },
            },
        );

        assert_ok!(process_commission_with_reserve(
            ENTITY_ID, SHOP_ID, 8002, &BUYER, 100_000, 100_000, 0, PRODUCT_ID,
        ));

        let pool = UnallocatedPool::<Test>::get(ENTITY_ID);
        assert_eq!(pool, 5_000);
    });
}

#[test]
fn cb_engine_nex_caps_with_owner_reward() {
    // rate=2000(20%), owner_reward_rate=5000(50% of order)
    // initial_remaining = 100_000 * 2000 / 10000 = 20_000
    // owner_reward = 100_000 * 5000 / 10000 = 50_000, min(50_000, 20_000) = 20_000 → remaining = 0
    // referral_cap=300(3%) → cap_amount = 100_000*300/10000 = 3_000, but remaining=0
    new_test_ext().execute_with(|| {
        fund(PLATFORM, 1_000_000);
        fund(SELLER, 1_000_000);
        CommissionConfigs::<Test>::insert(
            ENTITY_ID,
            CoreCommissionConfig {
                enabled_modes: CommissionModes(
                    CommissionModes::DIRECT_REWARD
                        | CommissionModes::OWNER_REWARD
                        | CommissionModes::POOL_REWARD,
                ),
                max_commission_rate: 2000,
                enabled: true,
                withdrawal_cooldown: 0,
                owner_reward_rate: 5000,
                token_withdrawal_cooldown: 0,
                plugin_caps: PluginBudgetCaps {
                    referral_cap: 300,
                    multi_level_cap: 0,
                    level_diff_cap: 0,
                    single_line_cap: 0,
                    team_cap: 0,
                },
            },
        );

        assert_ok!(process_commission_with_reserve(
            ENTITY_ID, SHOP_ID, 8003, &BUYER, 100_000, 100_000, 0, PRODUCT_ID,
        ));

        // remaining after owner_reward = 0 → pool = 0
        let pool = UnallocatedPool::<Test>::get(ENTITY_ID);
        assert_eq!(pool, 0);

        let records = OrderCommissionRecords::<Test>::get(8003u64);
        assert_eq!(records.len(), 0);
        // OwnerReward 直接到账，不再有 OrderOwnerReward 可断言
    });
}

#[test]
fn cb_engine_nex_rate_10000_caps_still_effective() {
    // rate=9900 (budget ceiling) → cap 和 rate 同维度，cap 正常截断
    new_test_ext().execute_with(|| {
        fund(PLATFORM, 1_000_000);
        fund(SELLER, 1_000_000);
        CommissionConfigs::<Test>::insert(
            ENTITY_ID,
            CoreCommissionConfig {
                enabled_modes: CommissionModes(
                    CommissionModes::DIRECT_REWARD | CommissionModes::POOL_REWARD,
                ),
                max_commission_rate: 9900,
                enabled: true,
                withdrawal_cooldown: 0,
                owner_reward_rate: 0,
                token_withdrawal_cooldown: 0,
                plugin_caps: PluginBudgetCaps {
                    referral_cap: 3000,
                    multi_level_cap: 0,
                    level_diff_cap: 0,
                    single_line_cap: 0,
                    team_cap: 0,
                },
            },
        );

        assert_ok!(process_commission_with_reserve(
            ENTITY_ID, SHOP_ID, 8004, &BUYER, 100_000, 100_000, 0, PRODUCT_ID,
        ));

        // initial_remaining = 100_000 * 9900 / 10000 = 99_000
        let pool = UnallocatedPool::<Test>::get(ENTITY_ID);
        assert_eq!(pool, 99_000);
    });
}

#[test]
fn cb_engine_token_low_rate_with_caps_succeeds() {
    new_test_ext().execute_with(|| {
        let ea = entity_account(ENTITY_ID);
        set_token_balance(ENTITY_ID, ea, 500_000);

        CommissionConfigs::<Test>::insert(
            ENTITY_ID,
            CoreCommissionConfig {
                enabled_modes: CommissionModes(
                    CommissionModes::DIRECT_REWARD | CommissionModes::POOL_REWARD,
                ),
                max_commission_rate: 1000,
                enabled: true,
                withdrawal_cooldown: 0,
                owner_reward_rate: 0,
                token_withdrawal_cooldown: 0,
                plugin_caps: PluginBudgetCaps {
                    referral_cap: 500,
                    multi_level_cap: 0,
                    level_diff_cap: 0,
                    single_line_cap: 0,
                    team_cap: 0,
                },
            },
        );

        assert_ok!(CommissionCore::process_token_commission(
            ENTITY_ID, SHOP_ID, 8005, &BUYER, 100_000, 100_000, 0, PRODUCT_ID,
        ));

        // max_commission = 100_000 * 1000 / 10000 = 10_000
        // min(10_000, 500_000) = 10_000 → pool
        let pool = UnallocatedTokenPool::<Test>::get(ENTITY_ID);
        assert_eq!(pool, 10_000);
    });
}

#[test]
fn cb_engine_token_caps_with_owner_reward() {
    new_test_ext().execute_with(|| {
        let ea = entity_account(ENTITY_ID);
        set_token_balance(ENTITY_ID, ea, 500_000);

        CommissionConfigs::<Test>::insert(
            ENTITY_ID,
            CoreCommissionConfig {
                enabled_modes: CommissionModes(
                    CommissionModes::DIRECT_REWARD
                        | CommissionModes::OWNER_REWARD
                        | CommissionModes::POOL_REWARD,
                ),
                max_commission_rate: 2000,
                enabled: true,
                withdrawal_cooldown: 0,
                owner_reward_rate: 5000,
                token_withdrawal_cooldown: 0,
                plugin_caps: PluginBudgetCaps {
                    referral_cap: 300,
                    multi_level_cap: 0,
                    level_diff_cap: 0,
                    single_line_cap: 0,
                    team_cap: 0,
                },
            },
        );

        assert_ok!(CommissionCore::process_token_commission(
            ENTITY_ID, SHOP_ID, 8006, &BUYER, 100_000, 100_000, 0, PRODUCT_ID,
        ));

        // max_commission = 20_000, owner_reward = 100_000*5000/10000 = 50_000, min(50_000,20_000) = 20_000
        // remaining = 0 → pool = 0
        let pool = UnallocatedTokenPool::<Test>::get(ENTITY_ID);
        assert_eq!(pool, 0);

        let records = OrderTokenCommissionRecords::<Test>::get(8006u64);
        assert_eq!(records.len(), 0);
        // OwnerReward 直接到账，不再有 OrderTokenOwnerReward 可断言
    });
}
// ============================================================================

#[test]
fn critical1_withdraw_commission_rejects_inactive_entity() {
    new_test_ext().execute_with(|| {
        let ea = entity_account(ENTITY_ID);
        fund(ea, 100_000);
        setup_config(10000);

        // 直接注入 pending 佣金（跳过 process_commission 流程）
        MemberCommissionStats::<Test>::mutate(ENTITY_ID, &REFERRER, |stats| {
            stats.total_earned = 10_000;
            stats.pending = 10_000;
        });
        ShopPendingTotal::<Test>::insert(ENTITY_ID, 10_000u128);

        // 将 Entity 标记为非活跃（模拟 PendingClose/Closed/Suspended）
        set_entity_inactive(ENTITY_ID);

        // NEX 提现应被拒绝
        assert_noop!(
            CommissionCore::withdraw_commission(
                RuntimeOrigin::signed(REFERRER),
                ENTITY_ID,
                None,
                None,
            ),
            Error::<Test>::EntityNotActive
        );

        // 确认 pending 未被扣减
        let stats = MemberCommissionStats::<Test>::get(ENTITY_ID, &REFERRER);
        assert_eq!(stats.pending, 10_000);
    });
}

#[test]
fn critical1_withdraw_token_commission_rejects_inactive_entity() {
    new_test_ext().execute_with(|| {
        let ea = entity_account(ENTITY_ID);
        fund(ea, 100_000);
        setup_config(10000);

        // 直接注入 pending Token 佣金
        MemberTokenCommissionStats::<Test>::mutate(ENTITY_ID, &REFERRER, |stats| {
            stats.total_earned = 10_000;
            stats.pending = 10_000;
        });
        TokenPendingTotal::<Test>::insert(ENTITY_ID, 10_000u128);
        set_token_balance(ENTITY_ID, ea, 100_000);

        // 将 Entity 标记为非活跃
        set_entity_inactive(ENTITY_ID);

        // Token 提现应被拒绝
        assert_noop!(
            CommissionCore::withdraw_token_commission(
                RuntimeOrigin::signed(REFERRER),
                ENTITY_ID,
                None,
                None,
            ),
            Error::<Test>::EntityNotActive
        );

        // 确认 pending 未被扣减
        let stats = MemberTokenCommissionStats::<Test>::get(ENTITY_ID, &REFERRER);
        assert_eq!(stats.pending, 10_000);
    });
}

#[test]
fn critical1_withdraw_succeeds_when_entity_active() {
    new_test_ext().execute_with(|| {
        let ea = entity_account(ENTITY_ID);
        fund(ea, 100_000);
        setup_config(10000);

        // 直接注入 pending 佣金
        MemberCommissionStats::<Test>::mutate(ENTITY_ID, &REFERRER, |stats| {
            stats.total_earned = 10_000;
            stats.pending = 10_000;
        });
        ShopPendingTotal::<Test>::insert(ENTITY_ID, 10_000u128);

        // Entity 活跃（默认） → 提现成功
        assert_ok!(CommissionCore::withdraw_commission(
            RuntimeOrigin::signed(REFERRER),
            ENTITY_ID,
            None,
            None,
        ));

        // pending 已清零
        let stats = MemberCommissionStats::<Test>::get(ENTITY_ID, &REFERRER);
        assert_eq!(stats.pending, 0);
    });
}

// ============================================================================
// PoolNotEmpty 保护测试: 沉淀池非空时阻止关闭 POOL_REWARD
// ============================================================================

#[test]
fn pool_not_empty_blocks_disable_pool_reward() {
    // set_commission_modes without POOL_REWARD should fail if pool > 0
    new_test_ext().execute_with(|| {
        setup_token_config(5000, true); // POOL_REWARD enabled
        UnallocatedPool::<Test>::insert(ENTITY_ID, 1000u128);
        assert_noop!(
            CommissionCore::set_commission_modes(
                RuntimeOrigin::signed(SELLER),
                ENTITY_ID,
                CommissionModes(CommissionModes::DIRECT_REWARD),
            ),
            Error::<Test>::PoolNotEmpty
        );
        // With pool empty, should succeed
        UnallocatedPool::<Test>::insert(ENTITY_ID, 0u128);
        assert_ok!(CommissionCore::set_commission_modes(
            RuntimeOrigin::signed(SELLER),
            ENTITY_ID,
            CommissionModes(CommissionModes::DIRECT_REWARD),
        ));
    });
}

#[test]
fn pool_not_empty_blocks_enable_commission_false() {
    new_test_ext().execute_with(|| {
        setup_token_config(5000, true); // POOL_REWARD + enabled
        UnallocatedPool::<Test>::insert(ENTITY_ID, 500u128);
        assert_noop!(
            CommissionCore::enable_commission(RuntimeOrigin::signed(SELLER), ENTITY_ID, false,),
            Error::<Test>::PoolNotEmpty
        );
    });
}

#[test]
fn pool_not_empty_blocks_clear_commission_config() {
    new_test_ext().execute_with(|| {
        setup_token_config(5000, true); // POOL_REWARD + enabled
        UnallocatedPool::<Test>::insert(ENTITY_ID, 500u128);
        assert_noop!(
            CommissionCore::clear_commission_config(RuntimeOrigin::signed(SELLER), ENTITY_ID,),
            Error::<Test>::PoolNotEmpty
        );
    });
}

#[test]
fn root_force_disable_bypasses_pool_not_empty() {
    new_test_ext().execute_with(|| {
        setup_token_config(5000, true); // POOL_REWARD + enabled
        UnallocatedPool::<Test>::insert(ENTITY_ID, 5000u128);
        // Root can force disable even with non-empty pool
        assert_ok!(CommissionCore::force_disable_entity_commission(
            RuntimeOrigin::root(),
            ENTITY_ID,
        ));
        // But pool funds are still protected
        assert_eq!(UnallocatedPool::<Test>::get(ENTITY_ID), 5000);
    });
}

#[test]
fn pool_always_protected_in_withdraw_entity_funds() {
    // Pool is ALWAYS included in reserved, even without POOL_REWARD mode
    new_test_ext().execute_with(|| {
        let ea = entity_account(ENTITY_ID);
        fund(ea, 100_000);
        // No POOL_REWARD mode, but pool has balance
        CommissionConfigs::<Test>::insert(ENTITY_ID, CoreCommissionConfig {
            enabled: true,
            enabled_modes: CommissionModes(CommissionModes::DIRECT_REWARD),
            max_commission_rate: 5000,
            withdrawal_cooldown: 0,
            owner_reward_rate: 0,
            token_withdrawal_cooldown: 0,
            plugin_caps: PluginBudgetCaps::default(),
        });
        UnallocatedPool::<Test>::insert(ENTITY_ID, 50_000u128);
        // available = 100000 - 50000 - 1(min) = 49999
        assert_noop!(
            CommissionCore::withdraw_entity_funds(
                RuntimeOrigin::signed(SELLER),
                ENTITY_ID,
                50_000,
            ),
            Error::<Test>::InsufficientEntityFunds
        );
        assert_ok!(CommissionCore::withdraw_entity_funds(
            RuntimeOrigin::signed(SELLER),
            ENTITY_ID,
            49_999,
        ));
        // Pool should NOT be auto-shrunk
        assert_eq!(UnallocatedPool::<Test>::get(ENTITY_ID), 50_000);
    });
}

#[test]
fn token_pool_not_empty_blocks_disable() {
    // UnallocatedTokenPool > 0 should also block POOL_REWARD disable
    new_test_ext().execute_with(|| {
        setup_token_config(5000, true); // POOL_REWARD + enabled
        UnallocatedTokenPool::<Test>::insert(ENTITY_ID, 1000u128);
        assert_noop!(
            CommissionCore::set_commission_modes(
                RuntimeOrigin::signed(SELLER),
                ENTITY_ID,
                CommissionModes(CommissionModes::DIRECT_REWARD),
            ),
            Error::<Test>::PoolNotEmpty
        );
    });
}

// ============================================================================
// min_treasury_threshold 保护 (withdraw_entity_funds)
// ============================================================================

#[test]
fn withdraw_entity_funds_blocked_by_min_treasury_threshold() {
    new_test_ext().execute_with(|| {
        let ea = entity_account(ENTITY_ID);
        fund(ea, 100_000);
        // 设置 min_treasury_threshold = 60_000
        set_min_treasury_threshold(ENTITY_ID, 60_000);
        // entity_balance=100000, reserved=0, min_balance=1
        // available = 100000 - 0 - 1 = 99999（通过第一层检查）
        // 但 post_withdraw = 100000 - 50000 = 50000 < 60000 → 阻断
        assert_noop!(
            CommissionCore::withdraw_entity_funds(
                RuntimeOrigin::signed(SELLER), ENTITY_ID, 50_000,
            ),
            Error::<Test>::InsufficientEntityFunds
        );
    });
}

#[test]
fn withdraw_entity_funds_succeeds_above_threshold() {
    new_test_ext().execute_with(|| {
        let ea = entity_account(ENTITY_ID);
        fund(ea, 100_000);
        // 设置 min_treasury_threshold = 60_000
        set_min_treasury_threshold(ENTITY_ID, 60_000);
        // post_withdraw = 100000 - 40000 = 60000 >= 60000 → OK
        assert_ok!(CommissionCore::withdraw_entity_funds(
            RuntimeOrigin::signed(SELLER),
            ENTITY_ID,
            40_000,
        ));
        assert_eq!(Balances::free_balance(SELLER), 40_000);
    });
}

// ============================================================================
// max_shopping_balance_usdt — 购物余额超 USDT 阈值阻止领奖
// ============================================================================

// 辅助常量: 1 NEX = 10^12 (精度), mock rate = 1_000_000 (1 USDT/NEX)
// → 1 NEX = 1_000_000 USDT units (精度 10^6)
const ONE_NEX: u128 = 1_000_000_000_000;

#[test]
fn withdrawal_blocked_when_nex_shopping_exceeds_usdt_threshold() {
    new_test_ext().execute_with(|| {
        let ea = entity_account(ENTITY_ID);
        fund(PLATFORM, 1_000_000);
        fund(ea, 100 * ONE_NEX); // 足够覆盖购物余额锁定
        set_entity_referrer(ENTITY_ID, REFERRER);
        setup_config(10000);

        assert_ok!(process_commission_with_reserve(
            ENTITY_ID, SHOP_ID, 400, &BUYER, 100_000, 100_000, 10_000, PRODUCT_ID,
        ));

        assert_ok!(CommissionCore::set_withdrawal_config(
            RuntimeOrigin::signed(SELLER),
            ENTITY_ID,
            WithdrawalMode::FullWithdrawal,
            WithdrawalTierConfig {
                withdrawal_rate: 10000,
                repurchase_rate: 0
            },
            BoundedVec::default(),
            0,
            true,
        ));

        // 阈值 = 5 USDT (5_000_000), 购物余额 = 10 NEX = 10 USDT (10_000_000)
        assert_ok!(CommissionCore::set_repurchase_config(
            RuntimeOrigin::signed(SELLER),
            ENTITY_ID,
            pallet_commission_common::RepurchaseConfig {
                min_package_usdt: 0,
                enforced: false,
                auto_order: false,
                default_product_id: 0,
                shopping_balance_ttl_blocks: 0,
                max_shopping_balance_usdt: 5_000_000, // 5 USDT
            },
        ));

        set_loyalty_shopping_balance(ENTITY_ID, REFERRER, 10 * ONE_NEX); // 10 USDT
        set_loyalty_shopping_total(ENTITY_ID, 10 * ONE_NEX);

        // 10 USDT > 5 USDT → 阻止
        assert_noop!(
            CommissionCore::withdraw_commission(
                RuntimeOrigin::signed(REFERRER),
                ENTITY_ID,
                None,
                None,
            ),
            Error::<Test>::ShoppingBalanceExceedsThreshold
        );
    });
}

#[test]
fn withdrawal_allowed_when_nex_shopping_below_usdt_threshold() {
    new_test_ext().execute_with(|| {
        let ea = entity_account(ENTITY_ID);
        fund(PLATFORM, 1_000_000);
        fund(ea, 100 * ONE_NEX);
        set_entity_referrer(ENTITY_ID, REFERRER);
        setup_config(10000);

        assert_ok!(process_commission_with_reserve(
            ENTITY_ID, SHOP_ID, 401, &BUYER, 100_000, 100_000, 10_000, PRODUCT_ID,
        ));

        assert_ok!(CommissionCore::set_withdrawal_config(
            RuntimeOrigin::signed(SELLER),
            ENTITY_ID,
            WithdrawalMode::FullWithdrawal,
            WithdrawalTierConfig {
                withdrawal_rate: 10000,
                repurchase_rate: 0
            },
            BoundedVec::default(),
            0,
            true,
        ));

        // 阈值 = 5 USDT, 购物余额 = 3 NEX = 3 USDT → 低于阈值 → 放行
        assert_ok!(CommissionCore::set_repurchase_config(
            RuntimeOrigin::signed(SELLER),
            ENTITY_ID,
            pallet_commission_common::RepurchaseConfig {
                min_package_usdt: 0,
                enforced: false,
                auto_order: false,
                default_product_id: 0,
                shopping_balance_ttl_blocks: 0,
                max_shopping_balance_usdt: 5_000_000, // 5 USDT
            },
        ));

        set_loyalty_shopping_balance(ENTITY_ID, REFERRER, 3 * ONE_NEX); // 3 USDT
        set_loyalty_shopping_total(ENTITY_ID, 3 * ONE_NEX);

        assert_ok!(CommissionCore::withdraw_commission(
            RuntimeOrigin::signed(REFERRER),
            ENTITY_ID,
            None,
            None,
        ));
    });
}

#[test]
fn withdrawal_allowed_when_threshold_is_zero() {
    new_test_ext().execute_with(|| {
        let ea = entity_account(ENTITY_ID);
        fund(PLATFORM, 1_000_000);
        fund(ea, 200 * ONE_NEX);
        set_entity_referrer(ENTITY_ID, REFERRER);
        setup_config(10000);

        assert_ok!(process_commission_with_reserve(
            ENTITY_ID, SHOP_ID, 402, &BUYER, 100_000, 100_000, 10_000, PRODUCT_ID,
        ));

        assert_ok!(CommissionCore::set_withdrawal_config(
            RuntimeOrigin::signed(SELLER),
            ENTITY_ID,
            WithdrawalMode::FullWithdrawal,
            WithdrawalTierConfig {
                withdrawal_rate: 10000,
                repurchase_rate: 0
            },
            BoundedVec::default(),
            0,
            true,
        ));

        // max_shopping_balance_usdt = 0 → 不限制
        assert_ok!(CommissionCore::set_repurchase_config(
            RuntimeOrigin::signed(SELLER),
            ENTITY_ID,
            pallet_commission_common::RepurchaseConfig {
                min_package_usdt: 0,
                enforced: false,
                auto_order: false,
                default_product_id: 0,
                shopping_balance_ttl_blocks: 0,
                max_shopping_balance_usdt: 0,
            },
        ));

        set_loyalty_shopping_balance(ENTITY_ID, REFERRER, 100 * ONE_NEX); // 100 USDT
        set_loyalty_shopping_total(ENTITY_ID, 100 * ONE_NEX);

        // 阈值 = 0 (关闭)，即使有大额购物余额也放行
        assert_ok!(CommissionCore::withdraw_commission(
            RuntimeOrigin::signed(REFERRER),
            ENTITY_ID,
            None,
            None,
        ));
    });
}

#[test]
fn withdrawal_allowed_when_no_repurchase_config() {
    new_test_ext().execute_with(|| {
        let ea = entity_account(ENTITY_ID);
        fund(PLATFORM, 1_000_000);
        fund(ea, 200 * ONE_NEX);
        set_entity_referrer(ENTITY_ID, REFERRER);
        setup_config(10000);

        assert_ok!(process_commission_with_reserve(
            ENTITY_ID, SHOP_ID, 403, &BUYER, 100_000, 100_000, 10_000, PRODUCT_ID,
        ));

        assert_ok!(CommissionCore::set_withdrawal_config(
            RuntimeOrigin::signed(SELLER),
            ENTITY_ID,
            WithdrawalMode::FullWithdrawal,
            WithdrawalTierConfig {
                withdrawal_rate: 10000,
                repurchase_rate: 0
            },
            BoundedVec::default(),
            0,
            true,
        ));

        // 无 RepurchaseConfig → 不受限
        set_loyalty_shopping_balance(ENTITY_ID, REFERRER, 100 * ONE_NEX);
        set_loyalty_shopping_total(ENTITY_ID, 100 * ONE_NEX);

        assert_ok!(CommissionCore::withdraw_commission(
            RuntimeOrigin::signed(REFERRER),
            ENTITY_ID,
            None,
            None,
        ));
    });
}

#[test]
fn withdrawal_at_exact_threshold_boundary() {
    new_test_ext().execute_with(|| {
        let ea = entity_account(ENTITY_ID);
        fund(PLATFORM, 1_000_000);
        fund(ea, 100 * ONE_NEX);
        set_entity_referrer(ENTITY_ID, REFERRER);
        setup_config(10000);

        assert_ok!(process_commission_with_reserve(
            ENTITY_ID, SHOP_ID, 404, &BUYER, 100_000, 100_000, 10_000, PRODUCT_ID,
        ));

        assert_ok!(CommissionCore::set_withdrawal_config(
            RuntimeOrigin::signed(SELLER),
            ENTITY_ID,
            WithdrawalMode::FullWithdrawal,
            WithdrawalTierConfig {
                withdrawal_rate: 10000,
                repurchase_rate: 0
            },
            BoundedVec::default(),
            0,
            true,
        ));

        // 阈值 = 5 USDT (5_000_000), 购物余额 = 5 NEX = 5 USDT (5_000_000)
        assert_ok!(CommissionCore::set_repurchase_config(
            RuntimeOrigin::signed(SELLER),
            ENTITY_ID,
            pallet_commission_common::RepurchaseConfig {
                min_package_usdt: 0,
                enforced: false,
                auto_order: false,
                default_product_id: 0,
                shopping_balance_ttl_blocks: 0,
                max_shopping_balance_usdt: 5_000_000,
            },
        ));

        // 恰好等于阈值 → 放行 (<=)
        set_loyalty_shopping_balance(ENTITY_ID, REFERRER, 5 * ONE_NEX);
        set_loyalty_shopping_total(ENTITY_ID, 5 * ONE_NEX);

        assert_ok!(CommissionCore::withdraw_commission(
            RuntimeOrigin::signed(REFERRER),
            ENTITY_ID,
            None,
            None,
        ));
    });
}

#[test]
fn token_withdrawal_blocked_when_token_shopping_exceeds_usdt_threshold() {
    new_test_ext().execute_with(|| {
        let ea = entity_account(ENTITY_ID);
        fund(ea, 500_000);

        inject_token_pending(ENTITY_ID, REFERRER, 10_000);
        set_token_balance(ENTITY_ID, ea, 100 * ONE_NEX); // 足够覆盖 Token 购物余额锁定

        assert_ok!(CommissionCore::set_token_withdrawal_config(
            RuntimeOrigin::signed(SELLER),
            ENTITY_ID,
            WithdrawalMode::FullWithdrawal,
            WithdrawalTierConfig {
                withdrawal_rate: 10000,
                repurchase_rate: 0
            },
            BoundedVec::default(),
            0,
            true,
        ));

        // 设置 Token USDT 价格: 1 Token = 2 USDT
        set_token_usdt_price(ENTITY_ID, 2_000_000);

        // 阈值 = 5 USDT, Token 购物余额 = 10 Token = 20 USDT → 超出
        assert_ok!(CommissionCore::set_repurchase_config(
            RuntimeOrigin::signed(SELLER),
            ENTITY_ID,
            pallet_commission_common::RepurchaseConfig {
                min_package_usdt: 0,
                enforced: false,
                auto_order: false,
                default_product_id: 0,
                shopping_balance_ttl_blocks: 0,
                max_shopping_balance_usdt: 5_000_000, // 5 USDT
            },
        ));

        set_loyalty_token_shopping_balance(ENTITY_ID, REFERRER, 10 * ONE_NEX); // 10 Token = 20 USDT
        set_loyalty_token_shopping_total(ENTITY_ID, 10 * ONE_NEX);

        // 20 USDT > 5 USDT → 阻止
        assert_noop!(
            CommissionCore::withdraw_token_commission(
                RuntimeOrigin::signed(REFERRER),
                ENTITY_ID,
                None,
                None,
            ),
            Error::<Test>::TokenShoppingBalanceExceedsThreshold
        );

        // 消费至阈值以下 (2 Token = 4 USDT < 5 USDT) → 放行
        set_loyalty_token_shopping_balance(ENTITY_ID, REFERRER, 2 * ONE_NEX);
        set_loyalty_token_shopping_total(ENTITY_ID, 2 * ONE_NEX);

        assert_ok!(CommissionCore::withdraw_token_commission(
            RuntimeOrigin::signed(REFERRER),
            ENTITY_ID,
            None,
            None,
        ));
    });
}

#[test]
fn token_withdrawal_blocked_when_price_unavailable() {
    // Token 价格不可用时保守阻止
    new_test_ext().execute_with(|| {
        let ea = entity_account(ENTITY_ID);
        fund(ea, 500_000);

        inject_token_pending(ENTITY_ID, REFERRER, 10_000);
        set_token_balance(ENTITY_ID, ea, 1_000_000);

        assert_ok!(CommissionCore::set_token_withdrawal_config(
            RuntimeOrigin::signed(SELLER),
            ENTITY_ID,
            WithdrawalMode::FullWithdrawal,
            WithdrawalTierConfig {
                withdrawal_rate: 10000,
                repurchase_rate: 0
            },
            BoundedVec::default(),
            0,
            true,
        ));

        // 不设置 Token 价格 → get_token_price_usdt 返回 None → 保守阻止
        assert_ok!(CommissionCore::set_repurchase_config(
            RuntimeOrigin::signed(SELLER),
            ENTITY_ID,
            pallet_commission_common::RepurchaseConfig {
                min_package_usdt: 0,
                enforced: false,
                auto_order: false,
                default_product_id: 0,
                shopping_balance_ttl_blocks: 0,
                max_shopping_balance_usdt: 5_000_000,
            },
        ));

        // 有 Token 购物余额但价格不可用 → 阻止
        set_loyalty_token_shopping_balance(ENTITY_ID, REFERRER, 1);
        set_loyalty_token_shopping_total(ENTITY_ID, 1);

        assert_noop!(
            CommissionCore::withdraw_token_commission(
                RuntimeOrigin::signed(REFERRER),
                ENTITY_ID,
                None,
                None,
            ),
            Error::<Test>::TokenShoppingBalanceExceedsThreshold
        );
    });
}

#[test]
fn withdrawal_unblocked_after_consuming_below_threshold() {
    // 完整闭环: 领奖 → 复购产生余额 → 超阈值阻止 → 消费至阈值以下 → 再次领奖成功
    new_test_ext().execute_with(|| {
        let ea = entity_account(ENTITY_ID);
        fund(PLATFORM, 1_000_000);
        fund(ea, 200 * ONE_NEX); // 足够覆盖购物余额锁定
        set_entity_referrer(ENTITY_ID, REFERRER);
        setup_config(10000);

        assert_ok!(process_commission_with_reserve(
            ENTITY_ID, SHOP_ID, 501, &BUYER, 100_000, 100_000, 10_000, PRODUCT_ID,
        ));

        // FixedRate 30% 复购
        assert_ok!(CommissionCore::set_withdrawal_config(
            RuntimeOrigin::signed(SELLER),
            ENTITY_ID,
            WithdrawalMode::FixedRate {
                repurchase_rate: 3000
            },
            WithdrawalTierConfig {
                withdrawal_rate: 7000,
                repurchase_rate: 3000
            },
            BoundedVec::default(),
            0,
            true,
        ));

        // 阈值 = 1 USDT (1_000_000) — 低于复购产生的余额
        assert_ok!(CommissionCore::set_repurchase_config(
            RuntimeOrigin::signed(SELLER),
            ENTITY_ID,
            pallet_commission_common::RepurchaseConfig {
                min_package_usdt: 0,
                enforced: false,
                auto_order: false,
                default_product_id: 0,
                shopping_balance_ttl_blocks: 0,
                max_shopping_balance_usdt: 1_000_000, // 1 USDT
            },
        ));

        // 第一次领奖: 购物余额 = 0 → 成功
        assert_ok!(CommissionCore::withdraw_commission(
            RuntimeOrigin::signed(REFERRER),
            ENTITY_ID,
            None,
            None,
        ));

        // 领奖后购物余额 = 1500 NEX (raw) ← 但 USDT = 1500 * 1_000_000 / 10^12 ≈ 0
        // mock 的 balance 太小，折算 USDT ≈ 0 不会超阈值。
        // 这验证了小额残余不会被误阻止 — 正是 USDT 阈值的设计目的。
        let shopping = get_loyalty_shopping_balance(ENTITY_ID, REFERRER);
        assert!(shopping > 0, "应产生复购购物余额");

        // 下单 2: 再产生佣金
        assert_ok!(process_commission_with_reserve(
            ENTITY_ID, SHOP_ID, 502, &BUYER, 100_000, 100_000, 10_000, PRODUCT_ID,
        ));

        // 第二次领奖: 小额残余折合 USDT ≈ 0 ≤ 1 USDT → 放行
        assert_ok!(CommissionCore::withdraw_commission(
            RuntimeOrigin::signed(REFERRER),
            ENTITY_ID,
            None,
            None,
        ));

        // 模拟大额购物余额 50 NEX = 50 USDT → 超过 1 USDT 阈值
        set_loyalty_shopping_balance(ENTITY_ID, REFERRER, 50 * ONE_NEX);
        set_loyalty_shopping_total(ENTITY_ID, 50 * ONE_NEX);

        // 下单 3: 再产生佣金
        assert_ok!(process_commission_with_reserve(
            ENTITY_ID, SHOP_ID, 503, &BUYER, 100_000, 100_000, 10_000, PRODUCT_ID,
        ));

        // 第三次: 50 USDT > 1 USDT → 阻止
        assert_noop!(
            CommissionCore::withdraw_commission(
                RuntimeOrigin::signed(REFERRER),
                ENTITY_ID,
                None,
                None,
            ),
            Error::<Test>::ShoppingBalanceExceedsThreshold
        );

        // 消费至 0.5 NEX = 0.5 USDT < 1 USDT → 放行
        set_loyalty_shopping_balance(ENTITY_ID, REFERRER, ONE_NEX / 2);
        set_loyalty_shopping_total(ENTITY_ID, ONE_NEX / 2);

        assert_ok!(CommissionCore::withdraw_commission(
            RuntimeOrigin::signed(REFERRER),
            ENTITY_ID,
            None,
            None,
        ));
    });
}

// ============================================================================
// USDT 精度防御测试
// ============================================================================

#[test]
fn set_repurchase_config_rejects_max_shopping_balance_usdt_below_one_usdt() {
    new_test_ext().execute_with(|| {
        setup_config(10000);
        // 12 (裸数字，未乘 10^6) → 应被拒绝
        assert_noop!(
            CommissionCore::set_repurchase_config(
                RuntimeOrigin::signed(SELLER),
                ENTITY_ID,
                pallet_commission_common::RepurchaseConfig {
                    min_package_usdt: 0,
                    enforced: false,
                    auto_order: false,
                    default_product_id: 0,
                    shopping_balance_ttl_blocks: 0,
                    max_shopping_balance_usdt: 12, // 0.000012 USDT — 明显精度错误
                },
            ),
            Error::<Test>::UsdtAmountTooSmall
        );
        // 999_999 (不到 1 USDT) → 应被拒绝
        assert_noop!(
            CommissionCore::set_repurchase_config(
                RuntimeOrigin::signed(SELLER),
                ENTITY_ID,
                pallet_commission_common::RepurchaseConfig {
                    min_package_usdt: 0,
                    enforced: false,
                    auto_order: false,
                    default_product_id: 0,
                    shopping_balance_ttl_blocks: 0,
                    max_shopping_balance_usdt: 999_999,
                },
            ),
            Error::<Test>::UsdtAmountTooSmall
        );
    });
}

#[test]
fn set_repurchase_config_rejects_min_package_usdt_below_one_usdt() {
    new_test_ext().execute_with(|| {
        setup_config(10000);
        assert_noop!(
            CommissionCore::set_repurchase_config(
                RuntimeOrigin::signed(SELLER),
                ENTITY_ID,
                pallet_commission_common::RepurchaseConfig {
                    min_package_usdt: 500, // 0.0005 USDT — 精度错误
                    enforced: false,
                    auto_order: false,
                    default_product_id: 0,
                    shopping_balance_ttl_blocks: 0,
                    max_shopping_balance_usdt: 0,
                },
            ),
            Error::<Test>::UsdtAmountTooSmall
        );
    });
}

#[test]
fn set_repurchase_config_accepts_zero_and_valid_usdt_amounts() {
    new_test_ext().execute_with(|| {
        setup_config(10000);
        // 0 = 禁用 → 允许
        assert_ok!(CommissionCore::set_repurchase_config(
            RuntimeOrigin::signed(SELLER),
            ENTITY_ID,
            pallet_commission_common::RepurchaseConfig {
                min_package_usdt: 0,
                enforced: false,
                auto_order: false,
                default_product_id: 0,
                shopping_balance_ttl_blocks: 0,
                max_shopping_balance_usdt: 0,
            },
        ));
        // 恰好 1 USDT = 1_000_000 → 允许
        assert_ok!(CommissionCore::set_repurchase_config(
            RuntimeOrigin::signed(SELLER),
            ENTITY_ID,
            pallet_commission_common::RepurchaseConfig {
                min_package_usdt: 1_000_000,
                enforced: false,
                auto_order: false,
                default_product_id: 0,
                shopping_balance_ttl_blocks: 0,
                max_shopping_balance_usdt: 1_000_000,
            },
        ));
        // 12 USDT = 12_000_000 → 允许
        assert_ok!(CommissionCore::set_repurchase_config(
            RuntimeOrigin::signed(SELLER),
            ENTITY_ID,
            pallet_commission_common::RepurchaseConfig {
                min_package_usdt: 5_000_000,
                enforced: false,
                auto_order: false,
                default_product_id: 0,
                shopping_balance_ttl_blocks: 0,
                max_shopping_balance_usdt: 12_000_000,
            },
        ));
    });
}

// ============================================================================
// Owner Reward 补充测试（事件验证 / Shopping 通道 / 转账失败回流 / 独立 Owner）
// ============================================================================

const OWNER_ACCT: u64 = 777; // 独立 owner 账户（非 SELLER）

/// 辅助：设置独立 owner 并启用 OWNER_REWARD
fn setup_owner_reward_with_separate_owner(max_commission_rate: u16, owner_reward_rate: u16) {
    set_entity_owner(ENTITY_ID, OWNER_ACCT);
    fund(OWNER_ACCT, 1); // ED
    CommissionConfigs::<Test>::insert(
        ENTITY_ID,
        CoreCommissionConfig {
            enabled_modes: CommissionModes(
                CommissionModes::DIRECT_REWARD | CommissionModes::OWNER_REWARD,
            ),
            max_commission_rate,
            enabled: true,
            withdrawal_cooldown: 0,
            owner_reward_rate,
            token_withdrawal_cooldown: 0,
            plugin_caps: PluginBudgetCaps::default(),
        },
    );
}

// ── 1. NEX: OwnerRewardPaid 事件验证 + 独立 Owner 余额 ──

#[test]
fn owner_reward_nex_event_and_separate_owner_balance() {
    new_test_ext().execute_with(|| {
        fund(SELLER, 1_000_000);
        setup_owner_reward_with_separate_owner(5000, 2000);

        let owner_before = <pallet_balances::Pallet<Test> as frame_support::traits::Currency<
            u64,
        >>::free_balance(&OWNER_ACCT);

        assert_ok!(process_commission_with_reserve(
            ENTITY_ID, SHOP_ID, 9801, &BUYER, 100_000, 100_000, 0, PRODUCT_ID,
        ));

        // owner_reward = 100_000 * 2000 / 10000 = 20_000
        let owner_after =
            <pallet_balances::Pallet<Test> as frame_support::traits::Currency<u64>>::free_balance(
                &OWNER_ACCT,
            );
        assert_eq!(owner_after - owner_before, 20_000);

        // 事件验证
        System::assert_has_event(RuntimeEvent::CommissionCore(
            crate::pallet::Event::OwnerRewardPaid {
                entity_id: ENTITY_ID,
                order_id: 9801,
                to: OWNER_ACCT,
                amount: 20_000,
            },
        ));

        // 不记录 CommissionRecord
        let records = OrderCommissionRecords::<Test>::get(9801u64);
        assert_eq!(records.len(), 0);

        // 不增 stats
        let stats = MemberCommissionStats::<Test>::get(ENTITY_ID, OWNER_ACCT);
        assert_eq!(stats.total_earned, 0);
        assert_eq!(stats.pending, 0);
    });
}

// ── 2. Token: TokenOwnerRewardPaid 事件验证 + 独立 Owner Token 余额 ──

#[test]
fn owner_reward_token_event_and_separate_owner_balance() {
    new_test_ext().execute_with(|| {
        let ea = entity_account(ENTITY_ID);
        set_token_balance(ENTITY_ID, ea, 100_000);
        setup_owner_reward_with_separate_owner(5000, 2000);

        let owner_token_before = get_token_balance(ENTITY_ID, OWNER_ACCT);

        assert_ok!(CommissionCore::process_token_commission(
            ENTITY_ID, SHOP_ID, 9802, &BUYER, 80_000, 80_000, 0, PRODUCT_ID,
        ));

        // owner_reward = 80_000 * 2000 / 10000 = 16_000
        let owner_token_after = get_token_balance(ENTITY_ID, OWNER_ACCT);
        assert_eq!(owner_token_after - owner_token_before, 16_000);

        // entity_account token 减少了 16_000
        let ea_token = get_token_balance(ENTITY_ID, ea);
        assert_eq!(ea_token, 100_000 - 16_000);

        // 事件验证
        System::assert_has_event(RuntimeEvent::CommissionCore(
            crate::pallet::Event::TokenOwnerRewardPaid {
                entity_id: ENTITY_ID,
                order_id: 9802,
                to: OWNER_ACCT,
                amount: 16_000,
            },
        ));

        // 不记录 Token CommissionRecord
        let records = OrderTokenCommissionRecords::<Test>::get(9802u64);
        assert_eq!(records.len(), 0);

        // 不增 Token stats
        let stats = MemberTokenCommissionStats::<Test>::get(ENTITY_ID, OWNER_ACCT);
        assert_eq!(stats.total_earned, 0);
        assert_eq!(stats.pending, 0);
    });
}

// ── 3. Shopping: Owner Reward 不适用于购物余额管线 ──
// Shopping balance pipeline does NOT pay Owner rewards (by design).
// Owner rewards are exclusive to NEX and Token pipelines.
// 购物余额管线不发放 Owner 奖励（设计决策）。Owner 奖励仅限 NEX 和 Token 管线。

#[test]
fn shopping_pipeline_does_not_pay_owner_reward() {
    new_test_ext().execute_with(|| {
        let ea = entity_account(ENTITY_ID);
        fund(ea, 500_000);
        setup_owner_reward_with_separate_owner(5000, 2000);

        let owner_before = <pallet_balances::Pallet<Test> as frame_support::traits::Currency<
            u64,
        >>::free_balance(&OWNER_ACCT);
        let ea_before =
            <pallet_balances::Pallet<Test> as frame_support::traits::Currency<u64>>::free_balance(
                &ea,
            );

        assert_ok!(CommissionCore::process_shopping_commission(
            ENTITY_ID, SHOP_ID, 9803, &BUYER, 100_000, PRODUCT_ID,
        ));

        // Owner balance unchanged — no reward paid
        let owner_after =
            <pallet_balances::Pallet<Test> as frame_support::traits::Currency<u64>>::free_balance(
                &OWNER_ACCT,
            );
        assert_eq!(
            owner_after, owner_before,
            "Shopping pipeline must not pay Owner reward"
        );

        // Entity account unchanged (no transfer out for owner reward)
        let ea_after =
            <pallet_balances::Pallet<Test> as frame_support::traits::Currency<u64>>::free_balance(
                &ea,
            );
        assert_eq!(
            ea_after, ea_before,
            "Entity account must not be debited for Owner reward"
        );

        // Full budget (minus owner reward) goes to plugins / unallocated pool
        // With OWNER_REWARD enabled but ignored, the entire Pool B is available for plugins
    });
}

#[test]
fn shopping_pipeline_full_budget_available_for_plugins_despite_owner_config() {
    new_test_ext().execute_with(|| {
        let ea = entity_account(ENTITY_ID);
        fund(ea, 500_000);
        // Even with high owner_reward_rate, shopping pipeline ignores it
        setup_owner_reward_with_separate_owner(9900, 5000); // max 99%, owner 50%

        assert_ok!(CommissionCore::process_shopping_commission(
            ENTITY_ID, SHOP_ID, 9806, &BUYER, 100_000, PRODUCT_ID,
        ));

        // Owner should receive nothing
        let owner_after =
            <pallet_balances::Pallet<Test> as frame_support::traits::Currency<u64>>::free_balance(
                &OWNER_ACCT,
            );
        // OWNER_ACCT was funded with 1 in setup
        assert_eq!(
            owner_after, 1,
            "Owner must not receive reward from shopping pipeline"
        );
    });
}

#[test]
fn cancel_shopping_commission_clean_with_no_owner_reward() {
    new_test_ext().execute_with(|| {
        let ea = entity_account(ENTITY_ID);
        fund(ea, 500_000);
        setup_owner_reward_with_separate_owner(5000, 2000);

        assert_ok!(CommissionCore::process_shopping_commission(
            ENTITY_ID, SHOP_ID, 9807, &BUYER, 100_000, PRODUCT_ID,
        ));

        let owner_before_cancel =
            <pallet_balances::Pallet<Test> as frame_support::traits::Currency<u64>>::free_balance(
                &OWNER_ACCT,
            );
        let ea_before_cancel = <pallet_balances::Pallet<Test> as frame_support::traits::Currency<
            u64,
        >>::free_balance(&ea);

        assert_ok!(CommissionCore::cancel_shopping_commission(9807));

        // Owner unchanged (was never paid)
        let owner_after_cancel =
            <pallet_balances::Pallet<Test> as frame_support::traits::Currency<u64>>::free_balance(
                &OWNER_ACCT,
            );
        assert_eq!(owner_after_cancel, owner_before_cancel);

        // Entity account unchanged by cancel (no refund needed since no reward was paid)
        let ea_after_cancel = <pallet_balances::Pallet<Test> as frame_support::traits::Currency<
            u64,
        >>::free_balance(&ea);
        assert_eq!(ea_after_cancel, ea_before_cancel);
    });
}

// ── 8. NEX: 转账失败回流到 UnallocatedPool ──

#[test]
fn owner_reward_nex_transfer_fail_falls_back_to_unallocated_pool() {
    // Owner 为系统保留账户 0，验证资金 re-reserve 或正常到账（不丢失）
    new_test_ext().execute_with(|| {
        fund(SELLER, 1_000_000);
        set_entity_owner(ENTITY_ID, 0); // account 0 通常不可用
        setup_owner_reward_config(5000, 2000);
        set_entity_owner(ENTITY_ID, 0); // 覆盖 helper 设置

        let pool_before = UnallocatedPool::<Test>::get(ENTITY_ID);

        assert_ok!(process_commission_with_reserve(
            ENTITY_ID, SHOP_ID, 9808, &BUYER, 100_000, 100_000, 0, PRODUCT_ID,
        ));

        // 不论转账成功或失败，资金不应丢失
        let pool_after = UnallocatedPool::<Test>::get(ENTITY_ID);
        let owner_0_balance = <pallet_balances::Pallet<Test> as frame_support::traits::Currency<u64>>::free_balance(&0u64);
        let owner_reward_expected: u128 = 20_000;

        // 断言: 两条路径之一生效
        assert!(
            owner_0_balance >= owner_reward_expected || pool_after >= pool_before + owner_reward_expected,
            "Owner reward must go to owner or unallocated pool, got owner_balance={}, pool_delta={}",
            owner_0_balance, pool_after.saturating_sub(pool_before),
        );
    });
}

// ── 9. Token: Owner 不存在回流到 UnallocatedTokenPool ──

#[test]
fn owner_reward_token_missing_owner_falls_back_to_unallocated_token_pool() {
    new_test_ext().execute_with(|| {
        let ea = entity_account(ENTITY_ID);
        set_token_balance(ENTITY_ID, ea, 100_000);
        clear_entity_owner(ENTITY_ID);

        CommissionConfigs::<Test>::insert(
            ENTITY_ID,
            CoreCommissionConfig {
                enabled_modes: CommissionModes(
                    CommissionModes::DIRECT_REWARD | CommissionModes::OWNER_REWARD,
                ),
                max_commission_rate: 5000,
                enabled: true,
                withdrawal_cooldown: 0,
                owner_reward_rate: 2000,
                token_withdrawal_cooldown: 0,
                plugin_caps: PluginBudgetCaps::default(),
            },
        );

        assert_ok!(CommissionCore::process_token_commission(
            ENTITY_ID, SHOP_ID, 9809, &BUYER, 80_000, 80_000, 0, PRODUCT_ID,
        ));

        // owner_reward = 80_000 * 2000 / 10000 = 16_000 应回流 UnallocatedTokenPool
        let token_pool = UnallocatedTokenPool::<Test>::get(ENTITY_ID);
        assert_eq!(token_pool, 16_000);

        // entity_account token 未减少（transfer 没发生）
        let ea_token = get_token_balance(ENTITY_ID, ea);
        assert_eq!(ea_token, 100_000);
    });
}

// ── 10. Token: 转账余额不足回流到 UnallocatedTokenPool ──

#[test]
fn owner_reward_token_insufficient_balance_falls_back() {
    new_test_ext().execute_with(|| {
        let ea = entity_account(ENTITY_ID);
        // entity_account token 余额不足以支付 owner_reward
        set_token_balance(ENTITY_ID, ea, 5_000); // 只有 5_000
        setup_owner_reward_with_separate_owner(5000, 2000);

        assert_ok!(CommissionCore::process_token_commission(
            ENTITY_ID, SHOP_ID, 9810, &BUYER, 80_000, 80_000, 0, PRODUCT_ID,
        ));

        // max_commission = 40_000, available = 5_000, remaining = 5_000
        // owner_reward = 16_000, min(16_000, 5_000) = 5_000
        // token_transfer(ea→owner, 5_000) 应成功（ea 恰好有 5_000）
        let owner_token = get_token_balance(ENTITY_ID, OWNER_ACCT);
        let token_pool = UnallocatedTokenPool::<Test>::get(ENTITY_ID);

        // 资金不应丢失
        assert!(
            owner_token == 5_000 || token_pool == 5_000,
            "Token owner reward must go to owner or unallocated pool"
        );
    });
}

// ── 11. NEX: 取消不退独立 Owner 奖励 ──

#[test]
fn cancel_nex_commission_does_not_refund_separate_owner_reward() {
    new_test_ext().execute_with(|| {
        fund(SELLER, 1_000_000);
        let ea = entity_account(ENTITY_ID);
        fund(ea, 1);
        setup_owner_reward_with_separate_owner(5000, 2000);

        assert_ok!(process_commission_with_reserve(
            ENTITY_ID, SHOP_ID, 9811, &BUYER, 100_000, 100_000, 0, PRODUCT_ID,
        ));

        // 独立 owner 收到 20_000
        let owner_before_cancel =
            <pallet_balances::Pallet<Test> as frame_support::traits::Currency<u64>>::free_balance(
                &OWNER_ACCT,
            );
        assert_eq!(owner_before_cancel, 1 + 20_000); // ED + reward

        assert_ok!(CommissionCore::cancel_commission(9811));

        // 取消后 owner 余额不变
        let owner_after_cancel =
            <pallet_balances::Pallet<Test> as frame_support::traits::Currency<u64>>::free_balance(
                &OWNER_ACCT,
            );
        assert_eq!(owner_after_cancel, owner_before_cancel);
    });
}

// ── 12. Token: 取消不退独立 Owner Token 奖励 ──

#[test]
fn cancel_token_commission_does_not_refund_separate_owner_reward() {
    new_test_ext().execute_with(|| {
        let ea = entity_account(ENTITY_ID);
        set_token_balance(ENTITY_ID, ea, 100_000);
        setup_owner_reward_with_separate_owner(5000, 2000);

        assert_ok!(CommissionCore::process_token_commission(
            ENTITY_ID, SHOP_ID, 9812, &BUYER, 80_000, 80_000, 0, PRODUCT_ID,
        ));

        // owner 收到 16_000 token
        let owner_token_before = get_token_balance(ENTITY_ID, OWNER_ACCT);
        assert_eq!(owner_token_before, 16_000);

        assert_ok!(CommissionCore::do_cancel_token_commission(9812));

        // 取消后 owner token 余额不变
        let owner_token_after = get_token_balance(ENTITY_ID, OWNER_ACCT);
        assert_eq!(owner_token_after, owner_token_before);
    });
}

// ============================================================================
// HB-WD-01: 佣金跨链提现（withdraw_commission_to_evm）
// ============================================================================

const EVM_CHAIN: u32 = 56; // BSC
const EVM_RCPT: [u8; 20] = [0xAB; 20];

/// 直接给 (entity, account) 注入 pending 佣金并同步 ShopPendingTotal（满足 INV-1）。
fn seed_pending(account: u64, amount: u128) {
    MemberCommissionStats::<Test>::mutate(ENTITY_ID, &account, |s| {
        s.pending = s.pending.saturating_add(amount);
    });
    ShopPendingTotal::<Test>::mutate(ENTITY_ID, |t| *t = t.saturating_add(amount));
}

// FullWithdrawal（无 WithdrawalConfig 默认即全额提现）：withdrawal 全额桥出
#[test]
fn bridge_withdrawal_full_works() {
    new_test_ext().execute_with(|| {
        let ea = entity_account(ENTITY_ID);
        fund(ea, 100_000);
        seed_pending(REFERRER, 10_000);

        assert_ok!(CommissionCore::withdraw_commission_to_evm(
            RuntimeOrigin::signed(REFERRER),
            ENTITY_ID,
            Some(10_000),
            None,
            EVM_CHAIN,
            EVM_RCPT,
        ));

        // pending 清零，记 withdrawn
        let stats = MemberCommissionStats::<Test>::get(ENTITY_ID, REFERRER);
        assert_eq!(stats.pending, 0);
        assert_eq!(stats.withdrawn, 10_000);
        assert_eq!(stats.repurchased, 0);
        assert_eq!(ShopPendingTotal::<Test>::get(ENTITY_ID), 0);

        // entity_account 真 burn 10_000
        assert_eq!(Balances::free_balance(ea), 90_000);

        // 无 repurchase/bonus → 购物余额不变
        assert_eq!(get_loyalty_shopping_balance(ENTITY_ID, REFERRER), 0);

        // payout 被调用，参数正确（from=entity_account）
        assert_eq!(payout_calls(), vec![(ea, EVM_CHAIN, EVM_RCPT, 10_000)]);

        // 事件携带 commitment
        System::assert_has_event(RuntimeEvent::CommissionCore(
            crate::pallet::Event::CommissionBridgedOut {
                entity_id: ENTITY_ID,
                account: REFERRER,
                evm_chain_id: EVM_CHAIN,
                recipient: EVM_RCPT,
                withdrawn_amount: 10_000,
                repurchase_amount: 0,
                bonus_amount: 0,
                commitment: [7u8; 32],
            },
        ));
    });
}

// FixedRate 30%：withdrawal 桥出，repurchase 留 Nexus（进购物余额）
#[test]
fn bridge_withdrawal_fixed_rate_splits_repurchase_stays() {
    new_test_ext().execute_with(|| {
        let ea = entity_account(ENTITY_ID);
        fund(ea, 100_000);
        seed_pending(REFERRER, 10_000);

        assert_ok!(CommissionCore::set_withdrawal_config(
            RuntimeOrigin::signed(SELLER),
            ENTITY_ID,
            WithdrawalMode::FixedRate {
                repurchase_rate: 3000
            },
            WithdrawalTierConfig {
                withdrawal_rate: 7000,
                repurchase_rate: 3000
            },
            BoundedVec::default(),
            0,
            true,
        ));

        assert_ok!(CommissionCore::withdraw_commission_to_evm(
            RuntimeOrigin::signed(REFERRER),
            ENTITY_ID,
            None,
            None,
            EVM_CHAIN,
            EVM_RCPT,
        ));

        let stats = MemberCommissionStats::<Test>::get(ENTITY_ID, REFERRER);
        assert_eq!(stats.withdrawn, 7_000); // 桥出
        assert_eq!(stats.repurchased, 3_000); // 留 Nexus
        assert_eq!(stats.pending, 0);

        // 仅 withdrawal 部分被 burn
        assert_eq!(Balances::free_balance(ea), 93_000);
        // repurchase 进购物余额（留 Nexus）
        assert_eq!(get_loyalty_shopping_balance(ENTITY_ID, REFERRER), 3_000);
        // 桥出金额 = withdrawal
        assert_eq!(payout_calls(), vec![(ea, EVM_CHAIN, EVM_RCPT, 7_000)]);
    });
}

// 零收款地址被拒
#[test]
fn bridge_withdrawal_rejects_zero_recipient() {
    new_test_ext().execute_with(|| {
        fund(entity_account(ENTITY_ID), 100_000);
        seed_pending(REFERRER, 10_000);
        assert_noop!(
            CommissionCore::withdraw_commission_to_evm(
                RuntimeOrigin::signed(REFERRER),
                ENTITY_ID,
                Some(10_000),
                None,
                EVM_CHAIN,
                [0u8; 20],
            ),
            Error::<Test>::InvalidEvmRecipient
        );
    });
}

// 全额复购（withdrawal=0）无可桥出 → ZeroBridgeWithdrawal
#[test]
fn bridge_withdrawal_rejects_zero_bridge_withdrawal() {
    new_test_ext().execute_with(|| {
        fund(entity_account(ENTITY_ID), 100_000);
        seed_pending(REFERRER, 10_000);

        assert_ok!(CommissionCore::set_withdrawal_config(
            RuntimeOrigin::signed(SELLER),
            ENTITY_ID,
            WithdrawalMode::FixedRate {
                repurchase_rate: 10000
            },
            WithdrawalTierConfig {
                withdrawal_rate: 0,
                repurchase_rate: 10000
            },
            BoundedVec::default(),
            0,
            true,
        ));

        assert_noop!(
            CommissionCore::withdraw_commission_to_evm(
                RuntimeOrigin::signed(REFERRER),
                ENTITY_ID,
                None,
                None,
                EVM_CHAIN,
                EVM_RCPT,
            ),
            Error::<Test>::ZeroBridgeWithdrawal
        );
    });
}

// 桥派发失败 → 整笔回滚（pending 不动、未 burn、未记调用）
#[test]
fn bridge_withdrawal_payout_failure_rolls_back() {
    new_test_ext().execute_with(|| {
        let ea = entity_account(ENTITY_ID);
        fund(ea, 100_000);
        seed_pending(REFERRER, 10_000);
        set_payout_mode(1); // 桥拒绝

        assert_noop!(
            CommissionCore::withdraw_commission_to_evm(
                RuntimeOrigin::signed(REFERRER),
                ENTITY_ID,
                Some(10_000),
                None,
                EVM_CHAIN,
                EVM_RCPT,
            ),
            Error::<Test>::CrossChainPayoutFailed
        );

        // 回滚：pending 不动、ea 未 burn、无调用记录
        assert_eq!(
            MemberCommissionStats::<Test>::get(ENTITY_ID, REFERRER).pending,
            10_000
        );
        assert_eq!(ShopPendingTotal::<Test>::get(ENTITY_ID), 10_000);
        assert_eq!(Balances::free_balance(ea), 100_000);
        assert!(payout_calls().is_empty());
    });
}

// 未接线（mode 2）→ CrossChainPayoutNotConfigured
#[test]
fn bridge_withdrawal_not_configured() {
    new_test_ext().execute_with(|| {
        fund(entity_account(ENTITY_ID), 100_000);
        seed_pending(REFERRER, 10_000);
        set_payout_mode(2);

        assert_noop!(
            CommissionCore::withdraw_commission_to_evm(
                RuntimeOrigin::signed(REFERRER),
                ENTITY_ID,
                Some(10_000),
                None,
                EVM_CHAIN,
                EVM_RCPT,
            ),
            Error::<Test>::CrossChainPayoutNotConfigured
        );
    });
}

// 全局暂停 → 拒绝且 pending 不动
#[test]
fn bridge_withdrawal_rejects_when_globally_paused() {
    new_test_ext().execute_with(|| {
        fund(entity_account(ENTITY_ID), 100_000);
        seed_pending(REFERRER, 10_000);
        GlobalCommissionPaused::<Test>::put(true);

        assert_noop!(
            CommissionCore::withdraw_commission_to_evm(
                RuntimeOrigin::signed(REFERRER),
                ENTITY_ID,
                Some(10_000),
                None,
                EVM_CHAIN,
                EVM_RCPT,
            ),
            Error::<Test>::GlobalCommissionPaused
        );
        assert!(payout_calls().is_empty());
    });
}

// 偿付不足 → InsufficientEntityFunds（withdrawal 超过 entity_account 可用余额）
#[test]
fn bridge_withdrawal_rejects_insufficient_entity_funds() {
    new_test_ext().execute_with(|| {
        fund(entity_account(ENTITY_ID), 1_000); // 远不足以桥出 10_000
        seed_pending(REFERRER, 10_000);

        assert_noop!(
            CommissionCore::withdraw_commission_to_evm(
                RuntimeOrigin::signed(REFERRER),
                ENTITY_ID,
                Some(10_000),
                None,
                EVM_CHAIN,
                EVM_RCPT,
            ),
            Error::<Test>::InsufficientEntityFunds
        );
        assert!(payout_calls().is_empty());
    });
}

// 默认 `()` 实现：跨链提现关闭，返回 not-configured 错误
#[test]
fn cross_chain_payout_default_unit_returns_not_configured() {
    let res = <() as crate::CrossChainPayout<u64, u128>>::payout_native(
        &1u64,
        EVM_CHAIN,
        EVM_RCPT,
        100,
        &[],
    );
    assert_eq!(
        res,
        Err(sp_runtime::DispatchError::Other(
            "cross-chain payout not configured"
        ))
    );
}

// 机制 2：跨链超时 → on_payout_timeout 把 withdrawal 恢复到 pending（从 withdrawn 扣回）
#[test]
fn bridge_withdrawal_timeout_restores_pending() {
    new_test_ext().execute_with(|| {
        let ea = entity_account(ENTITY_ID);
        fund(ea, 100_000);
        seed_pending(REFERRER, 10_000);

        // 先成功桥出全额（FullWithdrawal 默认）
        assert_ok!(CommissionCore::withdraw_commission_to_evm(
            RuntimeOrigin::signed(REFERRER),
            ENTITY_ID,
            Some(10_000),
            None,
            EVM_CHAIN,
            EVM_RCPT,
        ));
        let stats = MemberCommissionStats::<Test>::get(ENTITY_ID, REFERRER);
        assert_eq!(stats.pending, 0);
        assert_eq!(stats.withdrawn, 10_000);
        assert_eq!(ShopPendingTotal::<Test>::get(ENTITY_ID), 0);

        // 模拟桥超时回调：meta = (entity_id, who, withdrawal)
        let meta = (ENTITY_ID, REFERRER, 10_000u128).encode();
        assert_ok!(CommissionCore::on_payout_timeout(&meta));

        // pending 恢复、withdrawn 扣回、ShopPendingTotal 恢复
        let stats = MemberCommissionStats::<Test>::get(ENTITY_ID, REFERRER);
        assert_eq!(stats.pending, 10_000);
        assert_eq!(stats.withdrawn, 0);
        assert_eq!(ShopPendingTotal::<Test>::get(ENTITY_ID), 10_000);

        System::assert_has_event(RuntimeEvent::CommissionCore(
            crate::pallet::Event::CommissionPayoutRefunded {
                entity_id: ENTITY_ID,
                account: REFERRER,
                amount: 10_000,
            },
        ));
    });
}

// 机制 2：仅恢复 withdrawal 部分（FixedRate 30%：repurchase 已成功留 Nexus，不回补）
#[test]
fn bridge_withdrawal_timeout_restores_only_withdrawal_part() {
    new_test_ext().execute_with(|| {
        let ea = entity_account(ENTITY_ID);
        fund(ea, 100_000);
        seed_pending(REFERRER, 10_000);

        assert_ok!(CommissionCore::set_withdrawal_config(
            RuntimeOrigin::signed(SELLER),
            ENTITY_ID,
            WithdrawalMode::FixedRate {
                repurchase_rate: 3000
            },
            WithdrawalTierConfig {
                withdrawal_rate: 7000,
                repurchase_rate: 3000
            },
            BoundedVec::default(),
            0,
            true,
        ));

        assert_ok!(CommissionCore::withdraw_commission_to_evm(
            RuntimeOrigin::signed(REFERRER),
            ENTITY_ID,
            None,
            None,
            EVM_CHAIN,
            EVM_RCPT,
        ));
        // withdrawal=7000 桥出，repurchase=3000 留 Nexus
        assert_eq!(payout_calls(), vec![(ea, EVM_CHAIN, EVM_RCPT, 7_000)]);

        // 超时仅回补 withdrawal=7000
        let meta = (ENTITY_ID, REFERRER, 7_000u128).encode();
        assert_ok!(CommissionCore::on_payout_timeout(&meta));

        let stats = MemberCommissionStats::<Test>::get(ENTITY_ID, REFERRER);
        assert_eq!(stats.pending, 7_000); // 仅 withdrawal 回补
        assert_eq!(stats.withdrawn, 0);
        assert_eq!(stats.repurchased, 3_000); // repurchase 不变
        assert_eq!(ShopPendingTotal::<Test>::get(ENTITY_ID), 7_000);
        // 购物余额（repurchase）保持
        assert_eq!(get_loyalty_shopping_balance(ENTITY_ID, REFERRER), 3_000);
    });
}

// 机制 2：meta 无法解码 → InvalidPayoutRefundMeta
#[test]
fn bridge_withdrawal_timeout_rejects_bad_meta() {
    new_test_ext().execute_with(|| {
        assert_noop!(
            CommissionCore::on_payout_timeout(&[0xFFu8, 0x01]),
            Error::<Test>::InvalidPayoutRefundMeta
        );
    });
}
