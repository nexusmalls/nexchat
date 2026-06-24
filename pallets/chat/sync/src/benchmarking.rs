//! Benchmarks for `pallet-chat-sync`.
//! `pallet-chat-sync` 的基准测试。
//!
//! EN: Run via the node benchmark harness to generate the real `WeightInfo`.
//! Both extrinsics include one Ed25519 verification over the canonical payload,
//! so the setup signs with the keystore host functions (`sp_io::crypto`) — the
//! benchmark CLI registers a `MemoryKeystore`; the std test suite registers one
//! in `new_bench_ext`. The ciphertext is worst-case `MaxAnchorLen` bytes (its
//! blake2 hash is part of the signed payload).
//! CN: 通过节点基准框架运行以生成真实 `WeightInfo`。两个 extrinsic 均含一次对
//! 规范 payload 的 Ed25519 校验，因此 setup 用 keystore 宿主函数（`sp_io::crypto`）
//! 签名——benchmark CLI 自带 `MemoryKeystore`；std 测试套件在 `new_bench_ext`
//! 中注册。密文取 `MaxAnchorLen` 最坏情况（其 blake2 哈希参与签名 payload）。

#![cfg(feature = "runtime-benchmarks")]

use super::*;
use crate::types::AnchorId;
use crate::Pallet as ChatSync;
use frame_benchmarking::v2::*;
use sp_runtime::Saturating;
use frame_support::{
    traits::{Currency, Get},
    BoundedVec,
};
use frame_system::RawOrigin;
use sp_core::{ed25519, testing::ED25519};
use sp_io::hashing::blake2_256;
use sp_std::vec;

/// EN: Fresh anchor keypair in the keystore; returns the public key.
/// CN: 在 keystore 中生成新的锚密钥对；返回公钥。
fn anchor_public() -> ed25519::Public {
    sp_io::crypto::ed25519_generate(ED25519, None)
}

/// EN: Sign `payload` with the keystore key behind `public`.
/// CN: 用 keystore 中 `public` 对应的密钥签名 `payload`。
fn sign(public: &ed25519::Public, payload: &[u8]) -> [u8; 64] {
    sp_io::crypto::ed25519_sign(ED25519, public, payload)
        .expect("benchmark keystore holds the key; qed")
        .0
}

/// EN: Fund `who` well above the anchor deposit so reserve succeeds.
/// CN: 给 `who` 充值远超锚押金，使预留成功。
fn fund<T: Config>(who: &T::AccountId) {
    let amount = T::AnchorDeposit::get().saturating_mul(1_000u32.into());
    let _ = T::Currency::deposit_creating(who, amount);
}

/// EN: Worst-case ciphertext: exactly `MaxAnchorLen` bytes.
/// CN: 最坏情况密文：恰为 `MaxAnchorLen` 字节。
fn max_ciphertext<T: Config>() -> BoundedVec<u8, T::MaxAnchorLen> {
    vec![0x33u8; T::MaxAnchorLen::get() as usize]
        .try_into()
        .expect("length equals the bound")
}

/// EN: `updated_at` = current on-chain time, floored at 1 so the pre-set clear
/// tombstone (`updated_at - 1`) stays strictly below it even at a genesis where
/// `now == 0`; the `MaxClockSkew` ceiling still holds. CN: `updated_at` = 当前链上
/// 时间，下限取 1——使预置墓碑（`updated_at - 1`）在 `now == 0` 的 genesis 下仍
/// 严格小于它；`MaxClockSkew` 上界依然满足。
fn now_ms<T: Config>() -> u64 {
    pallet_timestamp::Pallet::<T>::get().saturated_into::<u64>().max(1)
}

/// EN: Mock externalities + in-memory keystore for the std benchmark test suite.
/// CN: std 基准测试套件用的 mock externalities + 内存 keystore。
#[cfg(test)]
pub fn new_bench_ext() -> sp_io::TestExternalities {
    use sp_keystore::{testing::MemoryKeystore, KeystoreExt};
    let mut ext = crate::mock::new_test_ext();
    ext.register_extension(KeystoreExt::new(MemoryKeystore::new()));
    ext
}

#[benchmarks]
mod benchmarks {
    use super::*;

    #[benchmark]
    fn publish_sync_anchor() {
        let caller: T::AccountId = whitelisted_caller();
        fund::<T>(&caller);
        let public = anchor_public();
        let anchor_pk: [u8; 32] = public.0;
        let anchor_id: AnchorId = blake2_256(&anchor_pk);
        let ciphertext = max_ciphertext::<T>();
        let updated_at = now_ms::<T>();
        let payload = ChatSync::<T>::publish_payload(&anchor_id, updated_at, &ciphertext);
        let sig = sign(&public, &payload);
        // Pre-set a tombstone so the ClearedAt read is non-empty (worst case).
        // 预置墓碑，使 ClearedAt 读取命中（最坏情况）。
        ClearedAt::<T>::insert(anchor_id, updated_at.saturating_sub(1));

        // First publish = worst case: ed25519 verify + tombstone check +
        // Currency::reserve + insert. 首次发布即最坏情况：ed25519 校验 + 墓碑
        // 检查 + 押金预留 + 写入。
        #[extrinsic_call]
        publish_sync_anchor(RawOrigin::Signed(caller), anchor_pk, updated_at, ciphertext, sig);

        assert!(SyncAnchors::<T>::contains_key(anchor_id));
    }

    #[benchmark]
    fn clear_sync_anchor() {
        let caller: T::AccountId = whitelisted_caller();
        fund::<T>(&caller);
        let public = anchor_public();
        let anchor_pk: [u8; 32] = public.0;
        let anchor_id: AnchorId = blake2_256(&anchor_pk);
        let ciphertext = max_ciphertext::<T>();
        let updated_at = now_ms::<T>();
        let publish_payload = ChatSync::<T>::publish_payload(&anchor_id, updated_at, &ciphertext);
        let publish_sig = sign(&public, &publish_payload);
        ChatSync::<T>::publish_sync_anchor(
            RawOrigin::Signed(caller.clone()).into(),
            anchor_pk,
            updated_at,
            ciphertext,
            publish_sig,
        )
        .expect("setup publish succeeds");

        let clear_payload = ChatSync::<T>::clear_payload(&anchor_id, updated_at);
        let sig = sign(&public, &clear_payload);

        #[extrinsic_call]
        clear_sync_anchor(RawOrigin::Signed(caller), anchor_pk, sig);

        assert!(!SyncAnchors::<T>::contains_key(anchor_id));
    }

    #[benchmark]
    fn force_clear_sync_anchor() -> Result<(), BenchmarkError> {
        let caller: T::AccountId = whitelisted_caller();
        fund::<T>(&caller);
        let public = anchor_public();
        let anchor_pk: [u8; 32] = public.0;
        let anchor_id: AnchorId = blake2_256(&anchor_pk);
        let ciphertext = max_ciphertext::<T>();
        let updated_at = now_ms::<T>();
        let publish_payload = ChatSync::<T>::publish_payload(&anchor_id, updated_at, &ciphertext);
        let publish_sig = sign(&public, &publish_payload);
        ChatSync::<T>::publish_sync_anchor(
            RawOrigin::Signed(caller).into(),
            anchor_pk,
            updated_at,
            ciphertext,
            publish_sig,
        )
        .expect("setup publish succeeds");

        let origin =
            T::ForceOrigin::try_successful_origin().map_err(|_| BenchmarkError::Weightless)?;

        #[extrinsic_call]
        force_clear_sync_anchor(origin, anchor_id);

        assert!(!SyncAnchors::<T>::contains_key(anchor_id));
        assert_eq!(ClearedAt::<T>::get(anchor_id), Some(updated_at));
        Ok(())
    }

    impl_benchmark_test_suite!(
        ChatSync,
        crate::benchmarking::new_bench_ext(),
        crate::mock::Test
    );
}
