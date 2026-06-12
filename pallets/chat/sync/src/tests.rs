//! Unit tests for `pallet-chat-sync` (ADR CHAT_SYNC_ANCHOR §5.4–§5.5 contract).

use crate::{mock::*, AnchorId, Error, Event, MIN_CIPHERTEXT_LEN};
use frame_support::{assert_noop, assert_ok, BoundedVec};
use sp_core::{ed25519, Pair};
use sp_io::hashing::blake2_256;

type Ciphertext = BoundedVec<u8, <Test as crate::Config>::MaxAnchorLen>;

fn keypair(seed: u8) -> ed25519::Pair {
    ed25519::Pair::from_seed(&[seed; 32])
}

fn anchor_id(pair: &ed25519::Pair) -> AnchorId {
    blake2_256(&pair.public().0)
}

fn ct(byte: u8, len: usize) -> Ciphertext {
    vec![byte; len].try_into().unwrap()
}

fn sign_publish(pair: &ed25519::Pair, updated_at: u64, ciphertext: &[u8]) -> [u8; 64] {
    let payload = ChatSync::publish_payload(&anchor_id(pair), updated_at, ciphertext);
    pair.sign(&payload).0
}

fn sign_clear(pair: &ed25519::Pair, stored_updated_at: u64) -> [u8; 64] {
    let payload = ChatSync::clear_payload(&anchor_id(pair), stored_updated_at);
    pair.sign(&payload).0
}

fn publish(
    origin: u64,
    pair: &ed25519::Pair,
    updated_at: u64,
    ciphertext: Ciphertext,
) -> frame_support::dispatch::DispatchResult {
    let sig = sign_publish(pair, updated_at, &ciphertext);
    ChatSync::publish_sync_anchor(
        RuntimeOrigin::signed(origin),
        pair.public().0,
        updated_at,
        ciphertext,
        sig,
    )
}

#[test]
fn first_publish_stores_record_and_reserves_deposit() {
    new_test_ext().execute_with(|| {
        let pair = keypair(1);
        let free_before = Balances::free_balance(1);

        assert_ok!(publish(1, &pair, 1_000, ct(0xAA, 64)));

        let record = ChatSync::sync_anchors(anchor_id(&pair)).unwrap();
        assert_eq!(record.version, 1);
        assert_eq!(record.updated_at, 1_000);
        assert_eq!(record.ciphertext.to_vec(), vec![0xAA; 64]);
        assert_eq!(record.depositor, 1);
        assert_eq!(record.deposit, 100);
        assert_eq!(record.last_publish_block, 1);
        assert_eq!(Balances::reserved_balance(1), 100);
        assert_eq!(Balances::free_balance(1), free_before - 100);

        System::assert_last_event(
            Event::AnchorPublished { anchor_id: anchor_id(&pair), updated_at: 1_000 }.into(),
        );
        assert_eq!(
            ChatSync::sync_anchor(anchor_id(&pair)),
            Some((1_000, vec![0xAA; 64]))
        );
    });
}

#[test]
fn bad_signature_rejected() {
    new_test_ext().execute_with(|| {
        let pair = keypair(1);
        let other = keypair(2);
        let body = ct(0xAA, 64);

        // Signed by the wrong key. / 错误密钥签名。
        let sig = sign_publish(&other, 1_000, &body);
        assert_noop!(
            ChatSync::publish_sync_anchor(
                RuntimeOrigin::signed(1),
                pair.public().0,
                1_000,
                body.clone(),
                sig,
            ),
            Error::<Test>::BadAnchorSignature
        );

        // Signature over a different updated_at. / 对不同 updated_at 的签名。
        let sig = sign_publish(&pair, 999, &body);
        assert_noop!(
            ChatSync::publish_sync_anchor(
                RuntimeOrigin::signed(1),
                pair.public().0,
                1_000,
                body.clone(),
                sig,
            ),
            Error::<Test>::BadAnchorSignature
        );

        // Signature over different ciphertext bytes. / 对不同密文的签名。
        let sig = sign_publish(&pair, 1_000, &ct(0xBB, 64));
        assert_noop!(
            ChatSync::publish_sync_anchor(
                RuntimeOrigin::signed(1),
                pair.public().0,
                1_000,
                body,
                sig,
            ),
            Error::<Test>::BadAnchorSignature
        );
    });
}

#[test]
fn short_ciphertext_rejected() {
    new_test_ext().execute_with(|| {
        let pair = keypair(1);
        assert_noop!(
            publish(1, &pair, 1_000, ct(0xAA, MIN_CIPHERTEXT_LEN as usize - 1)),
            Error::<Test>::CiphertextTooShort
        );
        assert_ok!(publish(1, &pair, 1_000, ct(0xAA, MIN_CIPHERTEXT_LEN as usize)));
    });
}

#[test]
fn lww_rejects_older_allows_equal_and_newer() {
    new_test_ext().execute_with(|| {
        let pair = keypair(1);
        assert_ok!(publish(1, &pair, 1_000, ct(0xAA, 32)));

        System::set_block_number(20);
        assert_noop!(publish(1, &pair, 999, ct(0xBB, 32)), Error::<Test>::StaleUpdatedAt);

        // `==` allowed: idempotent resend / same-ts overwrite (ADR §5.5 rule 4).
        assert_ok!(publish(1, &pair, 1_000, ct(0xBB, 32)));
        assert_eq!(
            ChatSync::sync_anchor(anchor_id(&pair)).unwrap().1,
            vec![0xBB; 32]
        );

        System::set_block_number(40);
        assert_ok!(publish(1, &pair, 2_000, ct(0xCC, 32)));
        assert_eq!(ChatSync::sync_anchor(anchor_id(&pair)).unwrap().0, 2_000);
    });
}

#[test]
fn far_future_updated_at_rejected() {
    new_test_ext().execute_with(|| {
        let pair = keypair(1);
        // mock now = 1_000_000, MaxClockSkew = 3_600_000.
        let limit = 1_000_000 + 3_600_000;
        assert_noop!(
            publish(1, &pair, limit + 1, ct(0xAA, 32)),
            Error::<Test>::UpdatedAtTooFarInFuture
        );
        assert_ok!(publish(1, &pair, limit, ct(0xAA, 32)));
    });
}

#[test]
fn republish_rate_limited_per_anchor() {
    new_test_ext().execute_with(|| {
        let pair = keypair(1);
        assert_ok!(publish(1, &pair, 1_000, ct(0xAA, 32)));

        // Same block and within the window → rejected. / 同块与窗口内 → 拒绝。
        assert_noop!(publish(1, &pair, 2_000, ct(0xBB, 32)), Error::<Test>::PublishTooFrequent);
        System::set_block_number(10); // 9 blocks later < 10
        assert_noop!(publish(1, &pair, 2_000, ct(0xBB, 32)), Error::<Test>::PublishTooFrequent);

        System::set_block_number(11); // exactly MinBlocksBetweenPublish
        assert_ok!(publish(1, &pair, 2_000, ct(0xBB, 32)));

        // A different anchor is not affected. / 不影响其他锚。
        let other = keypair(2);
        assert_ok!(publish(2, &other, 1_000, ct(0xCC, 32)));
    });
}

#[test]
fn depositor_unchanged_on_later_publish_from_other_origin() {
    new_test_ext().execute_with(|| {
        let pair = keypair(1);
        assert_ok!(publish(1, &pair, 1_000, ct(0xAA, 32)));

        System::set_block_number(20);
        // Origin 2 pays fees for this publish, but deposit stays with account 1.
        // 本次由账户 2 付费，押金仍记在账户 1 名下。
        assert_ok!(publish(2, &pair, 2_000, ct(0xBB, 32)));

        let record = ChatSync::sync_anchors(anchor_id(&pair)).unwrap();
        assert_eq!(record.depositor, 1);
        assert_eq!(Balances::reserved_balance(1), 100);
        assert_eq!(Balances::reserved_balance(2), 0);
    });
}

#[test]
fn first_publish_fails_without_deposit_balance() {
    new_test_ext().execute_with(|| {
        let pair = keypair(1);
        // Account 99 has no balance. / 账户 99 无余额。
        let sig = sign_publish(&pair, 1_000, &ct(0xAA, 32));
        assert!(ChatSync::publish_sync_anchor(
            RuntimeOrigin::signed(99),
            pair.public().0,
            1_000,
            ct(0xAA, 32),
            sig,
        )
        .is_err());
        assert!(ChatSync::sync_anchors(anchor_id(&pair)).is_none());
    });
}

#[test]
fn clear_refunds_depositor_and_removes() {
    new_test_ext().execute_with(|| {
        let pair = keypair(1);
        assert_ok!(publish(1, &pair, 1_000, ct(0xAA, 32)));
        assert_eq!(Balances::reserved_balance(1), 100);

        // Cleared by a different origin: refund still goes to depositor 1.
        // 由不同 origin 触发 clear：押金仍退还给 depositor 1。
        let sig = sign_clear(&pair, 1_000);
        assert_ok!(ChatSync::clear_sync_anchor(
            RuntimeOrigin::signed(2),
            pair.public().0,
            sig,
        ));

        assert!(ChatSync::sync_anchors(anchor_id(&pair)).is_none());
        assert_eq!(Balances::reserved_balance(1), 0);
        System::assert_last_event(Event::AnchorCleared { anchor_id: anchor_id(&pair) }.into());
    });
}

#[test]
fn clear_missing_anchor_returns_not_found() {
    new_test_ext().execute_with(|| {
        let pair = keypair(1);
        let sig = sign_clear(&pair, 0);
        assert_noop!(
            ChatSync::clear_sync_anchor(RuntimeOrigin::signed(1), pair.public().0, sig),
            Error::<Test>::AnchorNotFound
        );
    });
}

#[test]
fn clear_signature_binds_stored_updated_at_no_replay() {
    new_test_ext().execute_with(|| {
        let pair = keypair(1);
        assert_ok!(publish(1, &pair, 1_000, ct(0xAA, 32)));

        // Capture a clear signature for the CURRENT state… / 截获当前状态的 clear 签名…
        let old_clear_sig = sign_clear(&pair, 1_000);

        // …then the anchor advances. / …随后锚已前进。
        System::set_block_number(20);
        assert_ok!(publish(1, &pair, 2_000, ct(0xBB, 32)));

        // The captured signature no longer verifies (anti-replay across states).
        // 截获的签名不再有效（跨状态防重放）。
        assert_noop!(
            ChatSync::clear_sync_anchor(
                RuntimeOrigin::signed(1),
                pair.public().0,
                old_clear_sig,
            ),
            Error::<Test>::BadAnchorSignature
        );

        // A fresh signature over the new stored value works. / 对新存值的新签名有效。
        let sig = sign_clear(&pair, 2_000);
        assert_ok!(ChatSync::clear_sync_anchor(
            RuntimeOrigin::signed(1),
            pair.public().0,
            sig,
        ));
    });
}

/// EN: Cross-language frozen vectors — the SAME hex constants are asserted by the
/// client in `nexchat/src/store/syncAnchor.test.ts`. A failure here means the JS/Rust
/// byte contract diverged (ADR §5.5); never "fix the expected value" unilaterally.
/// CN: 跨语言冻结向量——完全相同的 hex 常量由客户端 `nexchat/src/store/syncAnchor.test.ts`
/// 断言。此处失败 = JS/Rust 字节合同分叉（ADR §5.5）；绝不允许单边「改期望值」。
#[test]
fn cross_language_vector_signatures_verify() {
    new_test_ext().execute_with(|| {
        fn from_hex<const N: usize>(s: &str) -> [u8; N] {
            let mut out = [0u8; N];
            for (i, byte) in out.iter_mut().enumerate() {
                *byte = u8::from_str_radix(&s[i * 2..i * 2 + 2], 16).unwrap();
            }
            out
        }

        // vault_master = 0x11×32 → frozen derivation outputs (generated by the client):
        let anchor_pk: [u8; 32] =
            from_hex("2daa51ff2538648c2e83228865a62e787c4591de51f4df34e9bc2ec51391e344");
        let anchor_id: [u8; 32] =
            from_hex("06973db6aa8fd39ea645fe7c4ed01814905e39cb2957de9cd6eba455e2f4c2b0");
        let publish_sig: [u8; 64] = from_hex(
            "9f2d8de5c58c59d70049e45706d456083b402ddf57e24e357564d1e33ef3817d\
             8f236c371d81592e03b6830be0f3c751f31f2ccd7a6b0bb301edb8b017291b07",
        );
        let clear_sig: [u8; 64] = from_hex(
            "bb32a0f667eb91615250d83b6736b586de06814274aaa498c3882cd5cb2fd960\
             ef85a68c8dff827957ff84c7efe60b4f4d46a2e7e7ce6f799e3d876f9e518f06",
        );
        let genesis = [0x22u8; 32];
        let updated_at: u64 = 1_738_665_600_000;
        let ciphertext = [0x33u8; 48];

        // anchor_id binding. / anchor_id 绑定。
        assert_eq!(blake2_256(&anchor_pk), anchor_id);

        // Rebuild both payloads with the SAME encoding rules as the pallet helpers
        // (fixed genesis instead of storage). / 按与 pallet 辅助函数相同的编码规则重建
        // payload（用固定 genesis 而非存储值）。
        let mut publish_payload = Vec::new();
        publish_payload.extend_from_slice(crate::PUBLISH_CONTEXT);
        publish_payload.extend_from_slice(&genesis);
        publish_payload.extend_from_slice(&anchor_id);
        publish_payload.extend_from_slice(&updated_at.to_le_bytes());
        publish_payload.extend_from_slice(&blake2_256(&ciphertext));

        let mut clear_payload = Vec::new();
        clear_payload.extend_from_slice(crate::CLEAR_CONTEXT);
        clear_payload.extend_from_slice(&genesis);
        clear_payload.extend_from_slice(&anchor_id);
        clear_payload.extend_from_slice(&updated_at.to_le_bytes());

        let pk = ed25519::Public::from_raw(anchor_pk);
        assert!(sp_io::crypto::ed25519_verify(
            &ed25519::Signature::from_raw(publish_sig),
            &publish_payload,
            &pk,
        ));
        assert!(sp_io::crypto::ed25519_verify(
            &ed25519::Signature::from_raw(clear_sig),
            &clear_payload,
            &pk,
        ));
    });
}

#[test]
fn anchor_id_is_computed_by_chain_not_caller() {
    new_test_ext().execute_with(|| {
        let pair = keypair(1);
        assert_ok!(publish(1, &pair, 1_000, ct(0xAA, 32)));
        // The storage key is exactly blake2_256(anchor_pk). / 存储键恰为 blake2_256(anchor_pk)。
        assert!(ChatSync::sync_anchors(blake2_256(&pair.public().0)).is_some());
    });
}

/// EN: §5.5 front-run analysis (audit C-3): an attacker who copies a first-publish
/// (payload, sig) from the mempool and lands it first only donates the deposit —
/// the stored state is exactly what the owner signed, and the owner's own tx then
/// settles as an idempotent no-op. The attacker can never alter content (the sig
/// binds it) nor block the owner's future updates.
/// CN: §5.5 抢跑分析（审计 C-3）：攻击者从内存池复制首发 (payload, sig) 并抢先上链，
/// 只是替持有者垫付了押金——存储状态恰为持有者所签内容，持有者自己的交易随后以幂等
/// no-op 落账。攻击者既无法篡改内容（签名绑定），也无法阻止持有者后续更新。
#[test]
fn mempool_front_run_of_first_publish_only_donates_deposit() {
    new_test_ext().execute_with(|| {
        let pair = keypair(1);
        let body = ct(0xAA, 32);
        let sig = sign_publish(&pair, 1_000, &body);

        // Attacker (origin 2) front-runs with the victim's exact payload + sig.
        // 攻击者（origin 2）用受害者的原始 payload + 签名抢跑。
        assert_ok!(ChatSync::publish_sync_anchor(
            RuntimeOrigin::signed(2),
            pair.public().0,
            1_000,
            body.clone(),
            sig,
        ));
        let record = ChatSync::sync_anchors(anchor_id(&pair)).unwrap();
        assert_eq!(record.depositor, 2); // attacker paid / 攻击者垫付
        assert_eq!(record.ciphertext.to_vec(), vec![0xAA; 32]); // content intact / 内容未变

        // The victim's original tx settles as an idempotent no-op (not an error).
        // 受害者原始交易以幂等 no-op 落账（不报错）。
        let sig = sign_publish(&pair, 1_000, &body);
        assert_ok!(ChatSync::publish_sync_anchor(
            RuntimeOrigin::signed(1),
            pair.public().0,
            1_000,
            body,
            sig,
        ));

        // The owner can still advance the anchor afterwards. / 持有者随后仍可推进锚。
        System::set_block_number(20);
        assert_ok!(publish(1, &pair, 2_000, ct(0xBB, 32)));
    });
}

/// EN: F-1 fix: a byte-identical resend (same updated_at + ciphertext) must not
/// reset the per-anchor rate-limit clock — otherwise any observer could replay the
/// public (payload, sig) every block to keep `last_publish_block` fresh and starve
/// the owner's next real update out of every window.
/// CN: F-1 修复：字节级等值重发（updated_at 与密文均相同）不得重置每锚限频时钟——
/// 否则任何观察者都能逐块重放公开的 (payload, sig)，把 `last_publish_block` 顶到
/// 最新，使持有者的下一次真实更新永远赶不上窗口。
#[test]
fn equal_resend_is_noop_and_does_not_reset_rate_limit_clock() {
    new_test_ext().execute_with(|| {
        let pair = keypair(1);
        let body = ct(0xAA, 32);
        assert_ok!(publish(1, &pair, 1_000, body.clone()));
        assert_eq!(
            ChatSync::sync_anchors(anchor_id(&pair)).unwrap().last_publish_block,
            1
        );

        // Attacker replays the identical publish inside the window. / 攻击者在窗口内重放。
        System::set_block_number(8);
        let sig = sign_publish(&pair, 1_000, &body);
        assert_ok!(ChatSync::publish_sync_anchor(
            RuntimeOrigin::signed(2),
            pair.public().0,
            1_000,
            body,
            sig,
        ));
        // No-op: the rate-limit clock did NOT move. / no-op：限频时钟未被推进。
        assert_eq!(
            ChatSync::sync_anchors(anchor_id(&pair)).unwrap().last_publish_block,
            1
        );

        // The owner's real update at block 11 still fits the window (would be
        // PublishTooFrequent until block 18 if the replay had reset the clock).
        // 持有者在第 11 块的真实更新仍落在窗口内（若重放重置了时钟，将一直
        // PublishTooFrequent 到第 18 块）。
        System::set_block_number(11);
        assert_ok!(publish(1, &pair, 2_000, ct(0xBB, 32)));
    });
}

/// EN: F-2 fix: after a clear, replaying a historical publish (payload, sig) at or
/// below the tombstone watermark must fail — a deliberately cleared anchor cannot
/// be resurrected by third parties. Only a strictly newer owner-signed manifest
/// re-creates it.
/// CN: F-2 修复：clear 之后，重放不高于墓碑水位的历史 publish (payload, sig) 必须
/// 失败——被主动 clear 的锚不能被第三方复活。只有严格更新的持有者签名清单才能重建。
#[test]
fn cleared_anchor_cannot_be_resurrected_by_replayed_publish() {
    new_test_ext().execute_with(|| {
        let pair = keypair(1);
        let body = ct(0xAA, 32);
        assert_ok!(publish(1, &pair, 1_000, body.clone()));
        // Capture the historical publish signature. / 截获历史 publish 签名。
        let old_sig = sign_publish(&pair, 1_000, &body);

        let sig = sign_clear(&pair, 1_000);
        assert_ok!(ChatSync::clear_sync_anchor(RuntimeOrigin::signed(1), pair.public().0, sig));
        assert_eq!(crate::ClearedAt::<Test>::get(anchor_id(&pair)), Some(1_000));

        // Replay at the watermark (==) is rejected. / 等于水位的重放被拒。
        System::set_block_number(20);
        assert_noop!(
            ChatSync::publish_sync_anchor(
                RuntimeOrigin::signed(2),
                pair.public().0,
                1_000,
                body,
                old_sig,
            ),
            Error::<Test>::StaleUpdatedAt
        );

        // A strictly newer owner-signed publish re-creates the anchor. / 严格更新的
        // 持有者签名 publish 可重建锚。
        assert_ok!(publish(1, &pair, 2_000, ct(0xBB, 32)));
        assert_eq!(ChatSync::sync_anchor(anchor_id(&pair)).unwrap().0, 2_000);
    });
}

/// EN: F-4: only [`Config::ForceOrigin`] may force-clear. CN: 仅 [`Config::ForceOrigin`]
/// 可强制清除。
#[test]
fn force_clear_requires_force_origin() {
    new_test_ext().execute_with(|| {
        let pair = keypair(1);
        assert_ok!(publish(1, &pair, 1_000, ct(0xAA, 32)));
        assert_noop!(
            ChatSync::force_clear_sync_anchor(RuntimeOrigin::signed(1), anchor_id(&pair)),
            sp_runtime::DispatchError::BadOrigin
        );
        assert_noop!(
            ChatSync::force_clear_sync_anchor(RuntimeOrigin::root(), [9u8; 32]),
            Error::<Test>::AnchorNotFound
        );
    });
}

/// EN: F-4: force-clear refunds the depositor, removes the record, and sets the
/// same tombstone as a regular clear (no resurrection from history); the owner can
/// always re-publish newer state (no censorship of future state).
/// CN: F-4：force-clear 退押金给 depositor、删除记录，并写入与常规 clear 相同的墓碑
/// （历史不可复活）；持有者随时可重新发布更新状态（不构成对未来状态的审查）。
#[test]
fn force_clear_refunds_depositor_and_sets_tombstone() {
    new_test_ext().execute_with(|| {
        let pair = keypair(1);
        let body = ct(0xAA, 32);
        assert_ok!(publish(1, &pair, 1_000, body.clone()));
        let old_sig = sign_publish(&pair, 1_000, &body);
        assert_eq!(Balances::reserved_balance(1), 100);

        assert_ok!(ChatSync::force_clear_sync_anchor(RuntimeOrigin::root(), anchor_id(&pair)));

        assert!(ChatSync::sync_anchors(anchor_id(&pair)).is_none());
        assert_eq!(Balances::reserved_balance(1), 0);
        assert_eq!(crate::ClearedAt::<Test>::get(anchor_id(&pair)), Some(1_000));
        System::assert_last_event(
            Event::AnchorForceCleared { anchor_id: anchor_id(&pair) }.into(),
        );

        // History replay stays dead; a newer owner publish still works.
        // 历史重放仍然无效；持有者的更新发布依旧可行。
        System::set_block_number(20);
        assert_noop!(
            ChatSync::publish_sync_anchor(
                RuntimeOrigin::signed(2),
                pair.public().0,
                1_000,
                body,
                old_sig,
            ),
            Error::<Test>::StaleUpdatedAt
        );
        assert_ok!(publish(1, &pair, 2_000, ct(0xBB, 32)));
    });
}
