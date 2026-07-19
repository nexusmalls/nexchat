//! Unit tests for `pallet-msg-identity` (incl. M0 frozen vectors).
//! `pallet-msg-identity` 单元测试（含 M0 冻结向量）。

use crate::mock::*;
use crate::types::*;
use crate::{Error, Event};
use frame_support::{assert_noop, assert_ok};

/// Helper: a deterministic IK and its self-certifying device id.
/// 辅助：确定性 IK 及其自证设备 id。
fn ik_for(seed: u8) -> (X25519Pub, DeviceId) {
    let ik = [seed; 32];
    let device_id = sp_io::hashing::blake2_128(&ik);
    (ik, device_id)
}

const ACC: u64 = 1;
const ACC2: u64 = 2;
const DEPOSIT: u128 = 100;

// ==================== register_device ====================

#[test]
fn register_device_works_and_reserves_deposit() {
    new_test_ext().execute_with(|| {
        let (ik, dev) = ik_for(0x11);
        assert_ok!(MsgIdentity::register_device(
            RuntimeOrigin::signed(ACC),
            dev,
            ik,
            [0u8; 64]
        ));
        assert!(MsgIdentity::device_exists(&ACC, dev));
        assert_eq!(MsgIdentity::device_ik(&ACC, dev), Some((ik, 0)));
        assert_eq!(Balances::reserved_balance(ACC), DEPOSIT);
        System::assert_has_event(
            Event::DeviceRegistered {
                account: ACC,
                device_id: dev,
            }
            .into(),
        );
    });
}

#[test]
fn register_rejects_device_id_not_matching_ik() {
    new_test_ext().execute_with(|| {
        let (ik, _dev) = ik_for(0x11);
        let wrong = [0xAAu8; 16];
        assert_noop!(
            MsgIdentity::register_device(RuntimeOrigin::signed(ACC), wrong, ik, [0u8; 64]),
            Error::<Test>::DeviceIdMismatch
        );
    });
}

#[test]
fn register_rejects_duplicate() {
    new_test_ext().execute_with(|| {
        let (ik, dev) = ik_for(0x11);
        assert_ok!(MsgIdentity::register_device(
            RuntimeOrigin::signed(ACC),
            dev,
            ik,
            [0u8; 64]
        ));
        assert_noop!(
            MsgIdentity::register_device(RuntimeOrigin::signed(ACC), dev, ik, [0u8; 64]),
            Error::<Test>::DeviceAlreadyExists
        );
    });
}

#[test]
fn register_enforces_device_cap() {
    new_test_ext().execute_with(|| {
        for seed in 0..3u8 {
            let (ik, dev) = ik_for(seed);
            assert_ok!(MsgIdentity::register_device(
                RuntimeOrigin::signed(ACC),
                dev,
                ik,
                [0u8; 64]
            ));
        }
        let (ik, dev) = ik_for(99);
        assert_noop!(
            MsgIdentity::register_device(RuntimeOrigin::signed(ACC), dev, ik, [0u8; 64]),
            Error::<Test>::TooManyDevices
        );
    });
}

// ==================== signed prekey ====================

#[test]
fn set_signed_prekey_requires_device_then_stores() {
    new_test_ext().execute_with(|| {
        let (ik, dev) = ik_for(0x22);
        let (spk, _) = ik_for(0x33);
        assert_noop!(
            MsgIdentity::set_signed_prekey(RuntimeOrigin::signed(ACC), dev, spk, [0u8; 64], 100),
            Error::<Test>::DeviceNotFound
        );
        assert_ok!(MsgIdentity::register_device(
            RuntimeOrigin::signed(ACC),
            dev,
            ik,
            [0u8; 64]
        ));
        assert_ok!(MsgIdentity::set_signed_prekey(
            RuntimeOrigin::signed(ACC),
            dev,
            spk,
            [0u8; 64],
            100
        ));
        assert_eq!(MsgIdentity::device_spk(&ACC, dev), Some((spk, 100)));
    });
}

// ==================== opk root ====================

#[test]
fn set_opk_root_bumps_epoch_and_rejects_empty() {
    new_test_ext().execute_with(|| {
        let (ik, dev) = ik_for(0x44);
        assert_ok!(MsgIdentity::register_device(
            RuntimeOrigin::signed(ACC),
            dev,
            ik,
            [0u8; 64]
        ));
        assert_noop!(
            MsgIdentity::set_opk_root(RuntimeOrigin::signed(ACC), dev, [1u8; 32], 0),
            Error::<Test>::EmptyOpkSet
        );
        assert_ok!(MsgIdentity::set_opk_root(
            RuntimeOrigin::signed(ACC),
            dev,
            [1u8; 32],
            100
        ));
        assert_eq!(
            MsgIdentity::device_opk_root(&ACC, dev),
            Some(([1u8; 32], 100, 0))
        );
        assert_ok!(MsgIdentity::set_opk_root(
            RuntimeOrigin::signed(ACC),
            dev,
            [2u8; 32],
            80
        ));
        assert_eq!(
            MsgIdentity::device_opk_root(&ACC, dev),
            Some(([2u8; 32], 80, 1))
        );
    });
}

#[test]
fn set_opk_root_requires_device() {
    new_test_ext().execute_with(|| {
        let (_ik, dev) = ik_for(0x44);
        assert_noop!(
            MsgIdentity::set_opk_root(RuntimeOrigin::signed(ACC), dev, [1u8; 32], 100),
            Error::<Test>::DeviceNotFound
        );
    });
}

// ==================== revocation epoch ====================

#[test]
fn bump_prekey_epoch_increments() {
    new_test_ext().execute_with(|| {
        let (ik, dev) = ik_for(0x55);
        assert_ok!(MsgIdentity::register_device(
            RuntimeOrigin::signed(ACC),
            dev,
            ik,
            [0u8; 64]
        ));
        assert_ok!(MsgIdentity::bump_prekey_epoch(
            RuntimeOrigin::signed(ACC),
            dev
        ));
        assert_eq!(MsgIdentity::device_ik(&ACC, dev), Some((ik, 1)));
        assert_ok!(MsgIdentity::bump_prekey_epoch(
            RuntimeOrigin::signed(ACC),
            dev
        ));
        assert_eq!(MsgIdentity::device_ik(&ACC, dev), Some((ik, 2)));
    });
}

// ==================== unregister ====================

#[test]
fn unregister_refunds_and_purges() {
    new_test_ext().execute_with(|| {
        let (ik, dev) = ik_for(0x66);
        let (spk, _) = ik_for(0x77);
        assert_ok!(MsgIdentity::register_device(
            RuntimeOrigin::signed(ACC),
            dev,
            ik,
            [0u8; 64]
        ));
        assert_ok!(MsgIdentity::set_signed_prekey(
            RuntimeOrigin::signed(ACC),
            dev,
            spk,
            [0u8; 64],
            0
        ));
        assert_ok!(MsgIdentity::set_opk_root(
            RuntimeOrigin::signed(ACC),
            dev,
            [1u8; 32],
            10
        ));
        assert_eq!(Balances::reserved_balance(ACC), DEPOSIT);

        assert_ok!(MsgIdentity::unregister_device(
            RuntimeOrigin::signed(ACC),
            dev
        ));
        assert!(!MsgIdentity::device_exists(&ACC, dev));
        assert_eq!(MsgIdentity::device_spk(&ACC, dev), None);
        assert_eq!(MsgIdentity::device_opk_root(&ACC, dev), None);
        assert_eq!(Balances::reserved_balance(ACC), 0);

        // 注销后可重新注册（计数已回退）/ can re-register after unregister (count rolled back).
        let (ik2, dev2) = ik_for(0x88);
        assert_ok!(MsgIdentity::register_device(
            RuntimeOrigin::signed(ACC),
            dev2,
            ik2,
            [0u8; 64]
        ));
    });
}

#[test]
fn unregister_requires_device() {
    new_test_ext().execute_with(|| {
        let (_ik, dev) = ik_for(0x66);
        assert_noop!(
            MsgIdentity::unregister_device(RuntimeOrigin::signed(ACC), dev),
            Error::<Test>::DeviceNotFound
        );
    });
}

// ==================== stack caps ====================

#[test]
fn set_stack_caps_roundtrips() {
    new_test_ext().execute_with(|| {
        assert_eq!(MsgIdentity::stack_caps(&ACC), None);
        assert_ok!(MsgIdentity::set_stack_caps(
            RuntimeOrigin::signed(ACC),
            STACK_DR | STACK_MLS_WIRE,
            3
        ));
        assert_eq!(
            MsgIdentity::stack_caps(&ACC),
            Some((STACK_DR | STACK_MLS_WIRE, 3))
        );
    });
}

// ==================== force unregister ====================

#[test]
fn force_unregister_only_by_force_origin() {
    new_test_ext().execute_with(|| {
        let (ik, dev) = ik_for(0x99);
        assert_ok!(MsgIdentity::register_device(
            RuntimeOrigin::signed(ACC),
            dev,
            ik,
            [0u8; 64]
        ));

        // 非特权来源被拒 / non-privileged origin rejected.
        assert!(
            MsgIdentity::force_unregister_device(RuntimeOrigin::signed(ACC2), ACC, dev).is_err()
        );

        // Root 强制注销并退押金 / Root force-unregisters and refunds.
        assert_ok!(MsgIdentity::force_unregister_device(
            RuntimeOrigin::root(),
            ACC,
            dev
        ));
        assert!(!MsgIdentity::device_exists(&ACC, dev));
        assert_eq!(Balances::reserved_balance(ACC), 0);
    });
}

// ==================== M0 冻结向量 / frozen vectors ====================

/// EN: Domain-separation contexts are part of the wire/endorsement format. Changing
/// them breaks every existing endorsement — treat a failure here as a breaking change.
/// CN: 域分隔上下文属于 wire/背书格式的一部分。改动即破坏所有既有背书——此测试失败按
/// 重大变更处理。
#[test]
fn frozen_domain_separation_contexts() {
    assert_eq!(CTX_IK_ENDORSE, b"nexchat/x3dh/ik-endorse/v1");
    assert_eq!(CTX_SPK_ENDORSE, b"nexchat/x3dh/spk-endorse/v1");
    assert_eq!(STACK_DR, 0b0000_0001);
    assert_eq!(STACK_MLS_WIRE, 0b0000_0010);
}

/// EN: `DeviceId = blake2_128(ik)` is the self-certifying derivation. Pin it against a
/// fixed IK (`[0x11;32]`) so the algorithm/encoding cannot silently change; also assert
/// distinct IKs yield distinct device ids. CN: `DeviceId = blake2_128(ik)` 为自证派生。
/// 以固定 IK（`[0x11;32]`）锚定，防算法/编码静默变更；并断言不同 IK 得到不同设备 id。
#[test]
fn frozen_device_id_derivation() {
    // Golden: blake2_128([0x11; 32]). Changing the hash/encoding breaks every device id.
    // 冻结：blake2_128([0x11; 32])。改动哈希/编码将破坏所有设备 id。
    const GOLDEN_IK11: DeviceId = [
        0x7f, 0x9c, 0x29, 0x9f, 0x1d, 0x9b, 0xbe, 0x85, 0x6f, 0xbf, 0x2c, 0x98, 0xf0, 0xf9, 0x14,
        0x35,
    ];
    let (_ik, dev) = ik_for(0x11);
    assert_eq!(dev, GOLDEN_IK11);
    assert_eq!(dev, sp_io::hashing::blake2_128(&[0x11u8; 32]));
    assert_ne!(dev, ik_for(0x12).1);
}
