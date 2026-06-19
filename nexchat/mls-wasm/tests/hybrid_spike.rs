// EN: Hybrid-scheme spike (group = Track A, 1:1 = Wire-style multi-leaf). Proves on the pinned
// OpenMLS 0.8.x — WITHOUT any virtual-clients feature, i.e. NOT gated by Gate-B — that:
//   Part 1 (1:1 Wire multi-leaf): one account can hold TWO leaves (two devices) in a pairwise
//     group bound to the same identity; both devices send concurrently with no AEAD nonce reuse;
//     each device can read the OTHER device's messages; removing a device gives per-leaf PCS.
//   Part 3 (coexistence): a Track A group vault (whole-provider escrow minus signer) stays
//     isolated from 1:1 leaf private keys WHEN groups and DMs use SEPARATE providers/clients —
//     which is the recommended resolution of the client-level `signer` conflict (design O1).
// Mirrors the harness of `escrow_spike.rs`. See pallets/chat/CHAT_MULTIDEVICE_HYBRID_DESIGN.md §10.
//
// CN: 混合方案 spike（群=轨 A，1:1=Wire 式多 leaf）。在锁定的 OpenMLS 0.8.x 上证明——**不依赖任何
// virtual-clients feature，即不受 Gate-B 阻断**：
//   Part 1（1:1 Wire 多 leaf）：同一账户在 pairwise 群里持**两个 leaf**（两台设备）、绑同一 identity；
//     两设备并发发不触发 AEAD nonce 重用；每设备能读**另一台**的消息；移除设备得到按 leaf 的 PCS。
//   Part 3（共存）：当群与 1:1 使用**独立 provider / 客户端**时，轨 A 群 vault（整 provider KV 剔签名钥）
//     与 1:1 leaf 私钥**互相隔离**——这是 client 级 `signer` 冲突的推荐解法（设计开放项 O1）。
// 复用 `escrow_spike.rs` 的 harness。详见 pallets/chat/CHAT_MULTIDEVICE_HYBRID_DESIGN.md §10。

use openmls::prelude::*;
use openmls_basic_credential::SignatureKeyPair;
use openmls_rust_crypto::OpenMlsRustCrypto;
use openmls_traits::OpenMlsProvider;
use std::collections::{HashMap, HashSet};
use tls_codec::Serialize as _;

const SUITE: Ciphersuite = Ciphersuite::MLS_128_DHKEMX25519_AES128GCM_SHA256_Ed25519;

// EN: A single device = its own provider + signer. Two devices of the SAME account are two
// `Party`s that share the same `identity` bytes (this is the minimal E2EI binding model: distinct
// leaves, one identity). CN: 单台设备 = 独立 provider + signer。同账户两设备 = 两个共享同一
// `identity` 字节的 `Party`（最小 E2EI 绑定模型：多 leaf、同身份）。
struct Party {
    provider: OpenMlsRustCrypto,
    signer: SignatureKeyPair,
    cred: CredentialWithKey,
}

fn party(identity: &str) -> Party {
    let provider = OpenMlsRustCrypto::default();
    let signer = SignatureKeyPair::new(SUITE.signature_algorithm()).unwrap();
    signer.store(provider.storage()).unwrap();
    let cred = CredentialWithKey {
        credential: BasicCredential::new(identity.as_bytes().to_vec()).into(),
        signature_key: signer.public().into(),
    };
    Party { provider, signer, cred }
}

fn create_cfg() -> MlsGroupCreateConfig {
    MlsGroupCreateConfig::builder()
        .ciphersuite(SUITE)
        .use_ratchet_tree_extension(true)
        .build()
}

fn join_cfg() -> MlsGroupJoinConfig {
    MlsGroupJoinConfig::builder().use_ratchet_tree_extension(true).build()
}

fn key_package(p: &Party) -> KeyPackage {
    KeyPackage::builder()
        .build(SUITE, &p.provider, &p.signer, p.cred.clone())
        .unwrap()
        .key_package()
        .clone()
}

fn new_group(owner: &Party) -> MlsGroup {
    MlsGroup::new(&owner.provider, &owner.signer, &create_cfg(), owner.cred.clone()).unwrap()
}

// EN: `owner` adds `kp`; returns the serialized commit + welcome and merges the pending commit.
// CN: `owner` 加入 `kp`；返回序列化的 commit + welcome 并合并待定 commit。
fn add_member(group: &mut MlsGroup, owner: &Party, kp: KeyPackage) -> (Vec<u8>, MlsMessageOut) {
    let (commit, welcome, _gi) = group.add_members(&owner.provider, &owner.signer, &[kp]).unwrap();
    group.merge_pending_commit(&owner.provider).unwrap();
    (commit.tls_serialize_detached().unwrap(), welcome)
}

fn join_from_welcome(joiner: &Party, welcome: &MlsMessageOut) -> MlsGroup {
    let welcome_in = match MlsMessageIn::tls_deserialize_bytes(&welcome.tls_serialize_detached().unwrap())
        .unwrap()
        .0
        .extract()
    {
        MlsMessageBodyIn::Welcome(w) => w,
        _ => panic!("not a welcome"),
    };
    StagedWelcome::new_from_welcome(&joiner.provider, &join_cfg(), welcome_in, None)
        .unwrap()
        .into_group(&joiner.provider)
        .unwrap()
}

fn send_app(group: &mut MlsGroup, p: &Party, msg: &[u8]) -> Vec<u8> {
    group
        .create_message(&p.provider, &p.signer, msg)
        .unwrap()
        .tls_serialize_detached()
        .unwrap()
}

fn decrypt_app(group: &mut MlsGroup, provider: &OpenMlsRustCrypto, wire: &[u8]) -> Vec<u8> {
    let (msg, _) = MlsMessageIn::tls_deserialize_bytes(wire).unwrap();
    let protocol: ProtocolMessage = msg.try_into_protocol_message().unwrap();
    let processed = group.process_message(provider, protocol).unwrap();
    match processed.into_content() {
        ProcessedMessageContent::ApplicationMessage(app) => app.into_bytes(),
        other => panic!("expected application message, got {other:?}"),
    }
}

// EN: Like `decrypt_app` but returns Err instead of panicking — for the PCS negative assertion.
// CN: 类似 `decrypt_app`，但返回 Err 而非 panic——用于 PCS 负向断言。
fn try_decrypt_app(group: &mut MlsGroup, provider: &OpenMlsRustCrypto, wire: &[u8]) -> Result<Vec<u8>, String> {
    let (msg, _) = MlsMessageIn::tls_deserialize_bytes(wire).map_err(|e| e.to_string())?;
    let protocol: ProtocolMessage = msg.try_into_protocol_message().map_err(|e| e.to_string())?;
    let processed = group.process_message(provider, protocol).map_err(|e| format!("{e:?}"))?;
    match processed.into_content() {
        ProcessedMessageContent::ApplicationMessage(app) => Ok(app.into_bytes()),
        other => Err(format!("not an application message: {other:?}")),
    }
}

fn process_commit(group: &mut MlsGroup, provider: &OpenMlsRustCrypto, wire: &[u8]) {
    let (msg, _) = MlsMessageIn::tls_deserialize_bytes(wire).unwrap();
    let protocol: ProtocolMessage = msg.try_into_protocol_message().unwrap();
    let processed = group.process_message(provider, protocol).unwrap();
    match processed.into_content() {
        ProcessedMessageContent::StagedCommitMessage(staged) => {
            group.merge_staged_commit(provider, *staged).unwrap();
        }
        other => panic!("expected commit, got {other:?}"),
    }
}

fn credential_identity(c: &Credential) -> String {
    if c.credential_type() == CredentialType::Basic {
        String::from_utf8_lossy(c.serialized_content()).into_owned()
    } else {
        String::new()
    }
}

// EN: Count leaves whose credential identity == `identity`. CN: 统计 identity 等于该值的 leaf 数。
fn count_identity(group: &MlsGroup, identity: &str) -> usize {
    group.members().filter(|m| credential_identity(&m.credential) == identity).count()
}

fn signer_storage_keys(signer: &SignatureKeyPair) -> HashSet<Vec<u8>> {
    let probe = OpenMlsRustCrypto::default();
    signer.store(probe.storage()).unwrap();
    let keys = probe.storage().values.read().unwrap().keys().cloned().collect();
    keys
}

// ─────────────────────────────────────────────────────────────────────────────────────────────
// Part 1 — 1:1 Wire multi-leaf (C2/C3/C4/C5/C6)
// ─────────────────────────────────────────────────────────────────────────────────────────────
#[test]
fn dm_multileaf_concurrent_send_and_cross_device_read() {
    // alice has TWO devices (same identity "alice"); bob is a single-device peer.
    let alice_a = party("alice");
    let alice_b = party("alice");
    let bob = party("bob");

    // C1: alice_a creates the pairwise group and adds bob (the existing 1:1 baseline).
    let mut a_group = new_group(&alice_a);
    let (_c1, w_bob) = add_member(&mut a_group, &alice_a, key_package(&bob));
    let mut bob_group = join_from_welcome(&bob, &w_bob);

    // C2 (core): a SECOND device of alice joins the EXISTING 1:1 — alice_a adds alice_b's leaf.
    let (commit_add_b, w_ab) = add_member(&mut a_group, &alice_a, key_package(&alice_b));
    process_commit(&mut bob_group, &bob.provider, &commit_add_b); // peer follows the membership change
    let mut b_group = join_from_welcome(&alice_b, &w_ab);

    assert_eq!(a_group.epoch(), bob_group.epoch());
    assert_eq!(a_group.epoch(), b_group.epoch());
    assert_eq!(a_group.members().count(), 3, "alice_a + bob + alice_b");

    // C3: two leaves carry the SAME identity "alice" (multi-leaf bound to one account).
    assert_eq!(count_identity(&a_group, "alice"), 2, "two alice devices = two leaves, one identity");
    assert_eq!(count_identity(&a_group, "bob"), 1);

    // C4 (core): alice_a and alice_b both send at the SAME epoch. Different leaves → different
    // secret-tree branches → no (key,nonce) reuse by MLS construction. We assert the two wires
    // differ and both decrypt at the peer.
    let from_a = send_app(&mut a_group, &alice_a, b"hi-from-device-A");
    let from_b = send_app(&mut b_group, &alice_b, b"hi-from-device-B");
    assert_ne!(from_a, from_b, "distinct leaves must not produce identical ciphertext");
    assert_eq!(decrypt_app(&mut bob_group, &bob.provider, &from_a), b"hi-from-device-A");
    assert_eq!(decrypt_app(&mut bob_group, &bob.provider, &from_b), b"hi-from-device-B");

    // C5: each of my devices can read the OTHER device's message (different members → decryptable).
    assert_eq!(decrypt_app(&mut b_group, &alice_b.provider, &from_a), b"hi-from-device-A");
    assert_eq!(decrypt_app(&mut a_group, &alice_a.provider, &from_b), b"hi-from-device-B");

    // C6 (privacy disclosure): the peer can SEE that account "alice" has 2 leaves (device count
    // is visible to the peer — the explicit cost of Wire multi-leaf vs Track B virtual clients).
    assert_eq!(count_identity(&bob_group, "alice"), 2, "peer sees alice's device count");
}

// ─────────────────────────────────────────────────────────────────────────────────────────────
// Part 1 — per-leaf PCS on device removal (C7)
// ─────────────────────────────────────────────────────────────────────────────────────────────
#[test]
fn dm_multileaf_remove_device_gives_pcs() {
    let alice_a = party("alice");
    let alice_b = party("alice");
    let bob = party("bob");

    let mut a_group = new_group(&alice_a);
    let (_c1, w_bob) = add_member(&mut a_group, &alice_a, key_package(&bob));
    let mut bob_group = join_from_welcome(&bob, &w_bob);
    let (commit_add_b, w_ab) = add_member(&mut a_group, &alice_a, key_package(&alice_b));
    process_commit(&mut bob_group, &bob.provider, &commit_add_b);
    let mut b_group = join_from_welcome(&alice_b, &w_ab);

    // Find alice_b's leaf index and remove it (alice_a commits the removal).
    let b_leaf = a_group
        .members()
        .find(|m| credential_identity(&m.credential) == "alice" && m.index != a_group.own_leaf_index())
        .map(|m| m.index)
        .expect("alice_b leaf present");
    let (remove_commit, _w, _gi) =
        a_group.remove_members(&alice_a.provider, &alice_a.signer, &[b_leaf]).unwrap();
    a_group.merge_pending_commit(&alice_a.provider).unwrap();
    process_commit(&mut bob_group, &bob.provider, &remove_commit.tls_serialize_detached().unwrap());

    // After removal, a fresh message in the new epoch must NOT be decryptable by the removed device
    // (alice_b is stuck at the old epoch and has lost forward access — per-leaf PCS, no mnemonic
    // rotation needed).
    let post = send_app(&mut a_group, &alice_a, b"after-removal");
    assert_eq!(decrypt_app(&mut bob_group, &bob.provider, &post), b"after-removal");
    assert!(
        try_decrypt_app(&mut b_group, &alice_b.provider, &post).is_err(),
        "removed device must NOT decrypt post-removal messages (PCS)"
    );
}

// ─────────────────────────────────────────────────────────────────────────────────────────────
// Part 3 — coexistence: Track A group escrow stays isolated from 1:1 keys with SEPARATE providers
// (E1/E3; demonstrates design O1 — group and DM use separate MlsClient instances).
// ─────────────────────────────────────────────────────────────────────────────────────────────
#[test]
fn group_escrow_with_separate_provider_does_not_contain_dm_state() {
    // Group client (Track A) and DM client (Wire) are SEPARATE providers for the same account.
    let alice_group = party("alice");
    let alice_dm = party("alice");
    let bob = party("bob");
    let carol = party("carol");

    // Track A group on alice_group's provider (3 members, mirrors the on-chain >=3 shape).
    let mut g = new_group(&alice_group);
    let (_cb, _wb) = add_member(&mut g, &alice_group, key_package(&bob));
    let (_cc, _wc) = add_member(&mut g, &alice_group, key_package(&carol));
    let group_id = g.group_id().clone();

    // A 1:1 Wire group on alice_dm's SEPARATE provider (with bob as peer).
    let bob_dm = party("bob");
    let mut dm = new_group(&alice_dm);
    let (_cd, _wd) = add_member(&mut dm, &alice_dm, key_package(&bob_dm));
    let dm_group_id = dm.group_id().clone();

    // E1: both groups load from their own providers (coexistence at storage level).
    assert!(MlsGroup::load(alice_group.provider.storage(), &group_id).unwrap().is_some());
    assert!(MlsGroup::load(alice_dm.provider.storage(), &dm_group_id).unwrap().is_some());

    // ── Track A escrow on the GROUP provider only: whole KV minus the signature keypair ──────────
    let sig_keys = signer_storage_keys(&alice_group.signer);
    let full_kv: HashMap<Vec<u8>, Vec<u8>> =
        alice_group.provider.storage().values.read().unwrap().clone();
    let escrow_kv: HashMap<Vec<u8>, Vec<u8>> =
        full_kv.into_iter().filter(|(k, _)| !sig_keys.contains(k)).collect();

    // E3: restore the group escrow on a fresh device → the GROUP loads, but the DM group does NOT
    // exist in it (its private keys never entered the group vault, because DMs use a separate
    // provider). This is the isolation guarantee that justifies O1 (separate MlsClient instances).
    let dev2 = OpenMlsRustCrypto::default();
    {
        let mut v = dev2.storage().values.write().unwrap();
        for (k, val) in escrow_kv {
            v.insert(k, val);
        }
    }
    assert!(
        MlsGroup::load(dev2.storage(), &group_id).unwrap().is_some(),
        "Track A group restores from its own escrow"
    );
    assert!(
        MlsGroup::load(dev2.storage(), &dm_group_id).unwrap().is_none(),
        "1:1 Wire state MUST NOT leak into the Track A group vault (separate providers, O1)"
    );
}
